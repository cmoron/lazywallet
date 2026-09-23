// ============================================================================
// Structure : App
// ============================================================================
// Gère l'état global de l'application TUI
//
// CONCEPTS RUST :
// 1. State Management : centraliser l'état dans une seule structure
// 2. Mutabilité contrôlée : &mut self pour modifier l'état
// 3. Propriétaire unique : seul le thread principal possède App
//
// PATTERN : Les transitions qui demandent du réseau ne l'appellent pas :
// elles *retournent* des `AppCommand`, que main() envoie au worker.
// L'état reste ainsi testable sans réseau ni thread.
// ============================================================================

use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::models::{Interval, WatchlistItem};
use crate::watchlist_file::{self, Entry};
use crate::worker::{AppCommand, AppResult};

/// Intervalle entre deux rafraîchissements automatiques des prix
pub const REFRESH_EVERY: Duration = Duration::from_mins(1);

/// Durée d'affichage d'un message dans la barre d'état
pub const STATUS_TTL: Duration = Duration::from_secs(5);

// ============================================================================
// Enum : Screen
// ============================================================================
// CONCEPT RUST : Enums pour state machines
// - Représente les différents écrans de l'application
// - Le compilateur force à gérer tous les cas (exhaustivité)
// ============================================================================

/// Écrans de l'application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// Vue principale : liste des tickers (watchlist)
    Dashboard,
    /// Vue graphique : graphique du ticker sélectionné
    ChartView,
    /// Mode saisie : les touches construisent un symbole (Enter valide, ESC annule)
    InputMode,
}

/// Ce que la saisie en cours va produire
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    /// Symbole d'un nouveau ticker
    AddTicker,
    /// "QUANTITÉ PRIX_DE_REVIENT" pour ce symbole (vide = retirer la position)
    Position(String),
}

/// Repères datés du graphique affiché, pour survivre à un rechargement
#[derive(Debug, Clone, Copy)]
struct ChartAnchor {
    interval: Interval,
    end: DateTime<Utc>,
    cursor: Option<DateTime<Utc>>,
}

/// Message affiché dans la barre d'état
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
    /// Instant d'affichage : le message disparaît après `STATUS_TTL`
    pub since: Instant,
}

/// État principal de l'application
#[allow(clippy::struct_excessive_bools)] // 3 drapeaux indépendants, pas une machine à états
pub struct App {
    /// Indique si l'application doit continuer à tourner
    pub running: bool,

    /// Liste des tickers à surveiller (watchlist)
    pub watchlist: Vec<WatchlistItem>,

    /// Index du ticker sélectionné dans la watchlist
    pub selected_index: usize,

    /// Écran actuellement affiché
    pub current_screen: Screen,

    /// Intervalle choisi pour les graphiques, commun à tous les tickers
    pub current_interval: Interval,

    /// Premier appui sur 'q' reçu : un second quitte
    pub confirm_quit: bool,

    /// Premier appui sur 'd' reçu : un second supprime
    pub confirm_delete: bool,

    /// Commandes envoyées au worker dont le résultat n'est pas encore arrivé
    ///
    /// CONCEPT : Un compteur plutôt qu'un booléen — plusieurs chargements
    /// peuvent être en vol, le premier terminé ne doit pas masquer les autres.
    pub pending: usize,

    /// Message (info ou erreur) affiché dans la barre d'état
    pub status: Option<Status>,

    /// Buffer de saisie pour le mode Input
    pub input_buffer: String,

    /// Nature de la saisie en cours
    pub input_kind: InputKind,

    /// Fichier de la watchlist (None dans les tests : rien n'est écrit)
    pub watchlist_path: Option<PathBuf>,

    /// Première ligne visible de la watchlist
    ///
    /// CONCEPT RUST : Cell (mutabilité intérieure)
    /// - Le rendu ne reçoit que `&App`, mais c'est lui qui sait combien de lignes
    ///   tiennent à l'écran : il lit le décalage, ratatui l'ajuste pour garder la
    ///   sélection visible, et le rendu le réécrit
    /// - `Cell<usize>` permet de modifier une valeur Copy à travers une référence
    ///   partagée, sans emprunt mutable
    pub list_offset: Cell<usize>,

    /// Graphique : nombre de chandelles récentes masquées à droite (0 = présent)
    pub chart_offset: usize,

    /// Graphique : chandelle sous le curseur (index dans les données), None = pas de curseur
    pub cursor: Option<usize>,

    /// Graphique : chandelles visibles au dernier rendu (`first..end`), écrit par le
    /// widget — même principe que `list_offset`
    pub chart_view: Cell<Option<(usize, usize)>>,

    /// Graphique : moyennes mobiles MA20 / MA50 affichées (touche m)
    pub show_moving_averages: bool,

    /// Dernier rafraîchissement (automatique ou manuel)
    last_refresh: Instant,
}

impl App {
    /// Crée l'application avec des tickers suivis, pas encore chargés
    pub fn new(symbols: Vec<String>, watchlist_path: Option<PathBuf>, now: Instant) -> Self {
        let entries = symbols.into_iter().map(Entry::watch).collect();
        Self::from_entries(entries, watchlist_path, now)
    }

    /// Crée l'application depuis les lignes du fichier de watchlist (positions comprises)
    pub fn from_entries(
        entries: Vec<Entry>,
        watchlist_path: Option<PathBuf>,
        now: Instant,
    ) -> Self {
        let watchlist = entries
            .into_iter()
            .map(|entry| {
                let mut item = WatchlistItem::new(entry.symbol);
                item.position = entry.position;
                item
            })
            .collect();
        Self {
            running: true,
            watchlist,
            selected_index: 0,
            current_screen: Screen::Dashboard,
            current_interval: Interval::default(),
            confirm_quit: false,
            confirm_delete: false,
            pending: 0,
            status: None,
            input_buffer: String::new(),
            input_kind: InputKind::AddTicker,
            watchlist_path,
            list_offset: Cell::new(0),
            chart_offset: 0,
            cursor: None,
            chart_view: Cell::new(None),
            show_moving_averages: true,
            last_refresh: now,
        }
    }

    // ========================================================================
    // Chargements (réseau via le worker)
    // ========================================================================

    /// Premier chargement de chaque ticker, dans l'intervalle par défaut
    pub fn initial_commands(&self) -> Vec<AppCommand> {
        self.watchlist
            .iter()
            .map(|item| AppCommand::Load {
                symbol: item.symbol.clone(),
                interval: Interval::default(),
            })
            .collect()
    }

    /// Recharge chaque ticker
    ///
    /// - Le graphique affiché : dans l'intervalle choisi (réessaie après un échec)
    /// - Les autres : dans l'intervalle déjà chargé, sauf W1 qui repasse à
    ///   l'intervalle par défaut — une chandelle hebdo ne permet pas de recalculer
    ///   la clôture de la veille, la variation du jour deviendrait périmée
    pub fn refresh_commands(&mut self, now: Instant) -> Vec<AppCommand> {
        self.last_refresh = now;
        let displayed = (self.current_screen == Screen::ChartView).then_some(self.selected_index);
        self.watchlist
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let interval = if Some(index) == displayed {
                    self.current_interval
                } else {
                    match item.data.as_ref().map(|d| d.interval) {
                        Some(Interval::W1) | None => Interval::default(),
                        Some(interval) => interval,
                    }
                };
                AppCommand::Load {
                    symbol: item.symbol.clone(),
                    interval,
                }
            })
            .collect()
    }

    /// Appelé à chaque tour de boucle : expiration du statut, refresh automatique
    pub fn tick(&mut self, now: Instant) -> Vec<AppCommand> {
        if self
            .status
            .as_ref()
            .is_some_and(|s| now.duration_since(s.since) >= STATUS_TTL)
        {
            self.status = None;
        }
        // Pas de refresh pendant un chargement : évite d'empiler les requêtes
        if self.pending == 0 && now.duration_since(self.last_refresh) >= REFRESH_EVERY {
            return self.refresh_commands(now);
        }
        Vec::new()
    }

    /// Intègre un résultat du worker
    pub fn apply_result(&mut self, result: AppResult, now: Instant) {
        self.pending = self.pending.saturating_sub(1);
        match result {
            // Ticker supprimé entre-temps : on jette le résultat
            AppResult::Loaded { symbol, fetched } => {
                // Graphique affiché rechargé : garder le curseur et la vue sur les
                // mêmes dates, même si la fenêtre de données a glissé
                let displayed = self.current_screen == Screen::ChartView
                    && self.selected_item().is_some_and(|i| i.symbol == symbol);
                let anchor = if displayed { self.chart_anchor() } else { None };
                if let Some(item) = self.watchlist.iter_mut().find(|i| i.symbol == symbol) {
                    item.apply(fetched);
                }
                if displayed {
                    self.reanchor_chart(anchor);
                }
            }
            AppResult::Added { symbol, fetched } => {
                if self.watchlist.iter().all(|i| i.symbol != symbol) {
                    let mut item = WatchlistItem::new(symbol.clone());
                    item.apply(fetched);
                    self.watchlist.push(item);
                    self.set_status(format!("{symbol} ajouté"), now);
                    self.save_watchlist(now);
                }
            }
            AppResult::Failed { error } => self.set_error(error, now),
        }
    }

    /// Vérifie si des données sont en cours de chargement
    pub fn is_loading(&self) -> bool {
        self.pending > 0
    }

    // ========================================================================
    // Barre d'état
    // ========================================================================

    pub fn set_status(&mut self, text: String, now: Instant) {
        self.status = Some(Status {
            text,
            is_error: false,
            since: now,
        });
    }

    pub fn set_error(&mut self, text: String, now: Instant) {
        self.status = Some(Status {
            text,
            is_error: true,
            since: now,
        });
    }

    // ========================================================================
    // Navigation et écrans
    // ========================================================================

    /// Quitte l'application
    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Vérifie si l'application doit continuer
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Navigue vers le haut dans la watchlist
    ///
    /// CONCEPT RUST : saturating_sub() ne descend pas en dessous de 0 (pas de panic)
    pub fn navigate_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    /// Navigue vers le bas dans la watchlist
    pub fn navigate_down(&mut self) {
        let max_index = self.watchlist.len().saturating_sub(1);
        self.selected_index = (self.selected_index + 1).min(max_index);
    }

    /// Retourne l'item sélectionné dans la watchlist
    pub fn selected_item(&self) -> Option<&WatchlistItem> {
        self.watchlist.get(self.selected_index)
    }

    /// Ouvre le graphique ; recharge si les données ne sont pas dans l'intervalle choisi
    pub fn open_chart(&mut self) -> Option<AppCommand> {
        let item = self.selected_item()?;
        let loaded = item.data.as_ref().map(|d| d.interval);
        let command = (loaded != Some(self.current_interval)).then(|| AppCommand::Load {
            symbol: item.symbol.clone(),
            interval: self.current_interval,
        });
        self.current_screen = Screen::ChartView;
        self.reset_chart_view();
        command
    }

    /// Retourne à la vue dashboard
    pub fn show_dashboard(&mut self) {
        self.current_screen = Screen::Dashboard;
    }

    /// Change l'intervalle et recharge le ticker affiché
    ///
    /// CONCEPT RUST : fn pointer
    /// - `step` vaut `Interval::next` ou `Interval::previous`
    pub fn change_interval(&mut self, step: fn(Interval) -> Interval) -> Option<AppCommand> {
        self.current_interval = step(self.current_interval);
        self.reset_chart_view();
        let symbol = self.selected_item()?.symbol.clone();
        Some(AppCommand::Load {
            symbol,
            interval: self.current_interval,
        })
    }

    // ========================================================================
    // Navigation dans le graphique
    // ========================================================================

    /// Nombre de chandelles du graphique affiché
    fn chart_len(&self) -> usize {
        self.selected_item()
            .and_then(|item| item.data.as_ref())
            .map_or(0, crate::models::OHLCData::len)
    }

    /// Chandelles visibles (`first..end`) : celles du dernier rendu, sinon une
    /// estimation minimale d'après le décalage
    fn visible_range(&self, len: usize) -> (usize, usize) {
        let estimated_end = len.saturating_sub(self.chart_offset).max(1);
        // La vue vient du rendu précédent : elle peut dépasser des données
        // rechargées entre-temps, on la borne ici, une seule fois
        let (first, end) = self
            .chart_view
            .get()
            .unwrap_or((estimated_end - 1, estimated_end));
        let end = end.min(len).max(1);
        (first.min(end - 1), end)
    }

    /// Déplace le curseur de `delta` chandelles, en faisant défiler si besoin
    ///
    /// Le premier appui fait apparaître le curseur sur la chandelle la plus
    /// récente affichée, sans la déplacer.
    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.chart_len();
        if len == 0 {
            return;
        }
        let (first, end) = self.visible_range(len);
        let Some(current) = self.cursor.filter(|&c| c < len) else {
            self.cursor = Some(end - 1);
            return;
        };
        // CONCEPT RUST : saturating_add_signed — usize + isize sans passer sous 0
        let target = current.saturating_add_signed(delta).min(len - 1);
        if target < first {
            self.chart_offset += first - target;
        } else if target >= end {
            self.chart_offset = self.chart_offset.saturating_sub(target + 1 - end);
        }
        self.chart_offset = self.chart_offset.min(len - 1);
        self.cursor = Some(target);
    }

    /// Défile d'une demi-page : `direction < 0` vers le passé, `> 0` vers le présent
    pub fn scroll_chart(&mut self, direction: isize) {
        let len = self.chart_len();
        if len == 0 {
            return;
        }
        let (first, end) = self.visible_range(len);
        let half_page = ((end - first) / 2).max(1);
        self.chart_offset = if direction < 0 {
            // Au moins une chandelle reste visible
            (self.chart_offset + half_page).min(len - 1)
        } else {
            self.chart_offset.saturating_sub(half_page)
        };
    }

    /// Repères datés de la vue : (intervalle, date de la dernière chandelle
    /// visible, date sous le curseur)
    fn chart_anchor(&self) -> Option<ChartAnchor> {
        let data = self.selected_item()?.data.as_ref()?;
        let end = data.len().checked_sub(self.chart_offset)?.checked_sub(1)?;
        Some(ChartAnchor {
            interval: data.interval,
            end: data.candles.get(end)?.timestamp,
            cursor: self
                .cursor
                .and_then(|c| data.candles.get(c))
                .map(|c| c.timestamp),
        })
    }

    /// Replace vue et curseur sur les dates d'avant le rechargement
    ///
    /// CONCEPT : Les index bougent quand la fenêtre de données glisse (nouvelles
    /// chandelles à droite, anciennes qui sortent à gauche) ; les dates, non.
    /// Autre intervalle, ou dates disparues : retour au présent.
    fn reanchor_chart(&mut self, anchor: Option<ChartAnchor>) {
        // La vue du rendu précédent ne correspond plus aux nouvelles données
        self.chart_view.set(None);
        let Some(anchor) = anchor else {
            self.reset_chart_view();
            return;
        };
        let Some(data) = self.selected_item().and_then(|item| item.data.as_ref()) else {
            self.reset_chart_view();
            return;
        };
        let find = |timestamp| data.candles.iter().position(|c| c.timestamp == timestamp);
        let end = (data.interval == anchor.interval)
            .then(|| find(anchor.end))
            .flatten();
        let Some(end) = end else {
            self.reset_chart_view();
            return;
        };
        let cursor = anchor.cursor.and_then(find);
        self.chart_offset = data.len() - 1 - end;
        self.cursor = cursor;
    }

    /// Retour au présent, sans curseur
    pub fn reset_chart_view(&mut self) {
        self.chart_offset = 0;
        self.cursor = None;
        self.chart_view.set(None);
    }

    // ========================================================================
    // Watchlist : ajout, suppression, sauvegarde
    // ========================================================================

    /// Valide un symbole saisi ; None si vide ou déjà présent (erreur affichée)
    pub fn request_add(&mut self, symbol: &str, now: Instant) -> Option<AppCommand> {
        let symbol = symbol.trim().to_uppercase();
        if symbol.is_empty() {
            return None;
        }
        if self.watchlist.iter().any(|i| i.symbol == symbol) {
            self.set_error(format!("{symbol} est déjà dans la watchlist"), now);
            return None;
        }
        Some(AppCommand::Add { symbol })
    }

    /// Supprime l'item sélectionné et garde la sélection dans les bornes
    pub fn delete_selected(&mut self, now: Instant) {
        self.confirm_delete = false;
        if self.selected_index >= self.watchlist.len() {
            return;
        }
        self.watchlist.remove(self.selected_index);
        self.selected_index = self
            .selected_index
            .min(self.watchlist.len().saturating_sub(1));
        self.save_watchlist(now);
    }

    /// Sauvegarde la watchlist ; une erreur s'affiche sans interrompre l'app
    fn save_watchlist(&mut self, now: Instant) {
        let Some(path) = &self.watchlist_path else {
            return;
        };
        let entries: Vec<Entry> = self
            .watchlist
            .iter()
            .map(|item| Entry {
                symbol: item.symbol.clone(),
                position: item.position,
            })
            .collect();
        if let Err(e) = watchlist_file::save(path, &entries) {
            self.set_error(format!("{e:#}"), now);
        }
    }

    // ========================================================================
    // Input Mode
    // ========================================================================

    /// Ouvre la saisie d'un symbole (annule toute confirmation en cours)
    pub fn start_input(&mut self) {
        self.current_screen = Screen::InputMode;
        self.input_kind = InputKind::AddTicker;
        self.input_buffer.clear();
    }

    /// Ouvre la saisie de la position du ticker sélectionné, pré-remplie
    pub fn start_position_input(&mut self) {
        // CONCEPT RUST : borrow checker
        // - `item` emprunte `self` : on en extrait des valeurs possédées (String)
        //   avant de modifier `self`, sinon l'emprunt serait encore vivant
        let Some((symbol, prefill)) = self.selected_item().map(|item| {
            let prefill = item
                .position
                .map_or_else(String::new, |p| format!("{} {}", p.quantity, p.unit_cost));
            (item.symbol.clone(), prefill)
        }) else {
            return;
        };
        self.input_buffer = prefill;
        self.input_kind = InputKind::Position(symbol);
        self.current_screen = Screen::InputMode;
    }

    /// Applique une position saisie ; texte vide = retirer la position
    pub fn set_position(&mut self, symbol: &str, text: &str, now: Instant) {
        let position = match watchlist_file::parse_position(text) {
            Ok(position) => position,
            Err(e) => {
                self.set_error(format!("{symbol} : {e:#}"), now);
                return;
            }
        };
        let Some(item) = self.watchlist.iter_mut().find(|i| i.symbol == symbol) else {
            return;
        };
        item.position = position;
        let text = match position {
            Some(p) => format!("{symbol} : position {} à {}", p.quantity, p.unit_cost),
            None => format!("{symbol} : position retirée"),
        };
        self.set_status(text, now);
        self.save_watchlist(now);
    }

    /// Annule la saisie et retourne au dashboard
    pub fn cancel_input(&mut self) {
        self.current_screen = Screen::Dashboard;
        self.input_buffer.clear();
    }

    /// Récupère la valeur saisie et retourne au dashboard
    pub fn take_input(&mut self) -> String {
        self.current_screen = Screen::Dashboard;
        std::mem::take(&mut self.input_buffer)
    }
}

// ============================================================================
// Tests unitaires
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FetchedTicker, OHLCData, Quote, OHLC};
    use chrono::{FixedOffset, TimeZone, Utc};

    fn fetched(symbol: &str, interval: Interval) -> FetchedTicker {
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut data = OHLCData::new(symbol.into(), interval, utc);
        data.add_candle(OHLC::new(Utc::now(), 1.0, 2.0, 0.5, 1.5, 0));
        FetchedTicker {
            data,
            long_name: None,
            quote: Quote {
                price: 1.5,
                previous_close: Some(1.0),
            },
            currency: None,
            price_decimals: 2,
            session: None,
            fetched_at: chrono::Utc::now(),
        }
    }

    fn app(symbols: &[&str]) -> (App, Instant) {
        let now = Instant::now();
        let symbols = symbols.iter().map(ToString::to_string).collect();
        (App::new(symbols, None, now), now)
    }

    #[test]
    fn late_result_for_deleted_ticker_is_ignored() {
        let (mut app, now) = app(&["AAPL", "TSLA"]);
        app.pending = 1;
        app.delete_selected(now); // supprime AAPL pendant que son chargement est en vol
        app.apply_result(
            AppResult::Loaded {
                symbol: "AAPL".into(),
                fetched: fetched("AAPL", Interval::M30),
            },
            now,
        );
        assert_eq!(app.watchlist.len(), 1);
        assert!(
            app.watchlist[0].data.is_none(),
            "TSLA ne doit pas recevoir les données d'AAPL"
        );
        assert_eq!(app.pending, 0);
    }

    #[test]
    fn failure_reaches_status_line() {
        let (mut app, now) = app(&["AAPL"]);
        app.pending = 1;
        app.apply_result(
            AppResult::Failed {
                error: "NOPE : symbole inconnu".into(),
            },
            now,
        );
        let status = app.status.as_ref().unwrap();
        assert!(status.is_error && status.text.contains("NOPE"));
        assert!(!app.is_loading());
    }

    #[test]
    fn added_ticker_is_appended_once() {
        let (mut app, now) = app(&["AAPL"]);
        for _ in 0..2 {
            app.apply_result(
                AppResult::Added {
                    symbol: "TSLA".into(),
                    fetched: fetched("TSLA", Interval::M30),
                },
                now,
            );
        }
        let symbols: Vec<_> = app.watchlist.iter().map(|i| i.symbol.as_str()).collect();
        assert_eq!(symbols, ["AAPL", "TSLA"]);
    }

    #[test]
    fn request_add_rejects_duplicates_and_blank() {
        let (mut app, now) = app(&["AAPL"]);
        assert!(app.request_add("  ", now).is_none());
        assert!(app.request_add("aapl", now).is_none());
        assert!(app.status.as_ref().unwrap().is_error);
        assert!(matches!(
            app.request_add("qqq", now),
            Some(AppCommand::Add { symbol }) if symbol == "QQQ"
        ));
    }

    #[test]
    fn opening_chart_reloads_when_interval_differs() {
        let (mut app, now) = app(&["AAPL"]);
        app.apply_result(
            AppResult::Loaded {
                symbol: "AAPL".into(),
                fetched: fetched("AAPL", Interval::M30),
            },
            now,
        );
        assert!(app.open_chart().is_none(), "données déjà en 30m");
        app.current_interval = Interval::D1;
        assert!(matches!(
            app.open_chart(),
            Some(AppCommand::Load {
                interval: Interval::D1,
                ..
            })
        ));
    }

    #[test]
    fn tick_refreshes_every_minute_when_idle() {
        let (mut app, now) = app(&["AAPL", "TSLA"]);
        assert!(app.tick(now + Duration::from_secs(59)).is_empty());
        assert_eq!(app.tick(now + REFRESH_EVERY).len(), 2);
        assert!(
            app.tick(now + REFRESH_EVERY + Duration::from_secs(1))
                .is_empty(),
            "compteur remis à zéro"
        );
        app.pending = 1;
        assert!(
            app.tick(now + REFRESH_EVERY * 3).is_empty(),
            "pas d'empilement pendant un chargement"
        );
    }

    #[test]
    fn status_expires() {
        let (mut app, now) = app(&["AAPL"]);
        app.set_status("ok".into(), now);
        app.tick(now + STATUS_TTL);
        assert!(app.status.is_none());
    }

    #[test]
    fn delete_keeps_selection_in_range() {
        let (mut app, now) = app(&["AAPL", "TSLA"]);
        app.navigate_down();
        app.delete_selected(now);
        assert_eq!(app.selected_index, 0);
        app.delete_selected(now);
        assert!(app.watchlist.is_empty() && app.selected_index == 0);
        app.delete_selected(now); // liste vide : aucun panic
    }

    #[test]
    fn save_failure_reaches_status_line() {
        // Un fichier à la place du dossier parent rend l'écriture impossible
        let blocker = std::env::temp_dir().join(format!("lazywallet-app-{}", std::process::id()));
        std::fs::write(&blocker, "").unwrap();
        let now = Instant::now();
        let mut app = App::new(
            vec!["AAPL".into()],
            Some(blocker.join("watchlist.txt")),
            now,
        );
        app.delete_selected(now);
        std::fs::remove_file(&blocker).unwrap();
        assert!(
            app.status.as_ref().is_some_and(|s| s.is_error),
            "{:?}",
            app.status
        );
    }

    #[test]
    fn refresh_after_failed_interval_change_retries_chosen_interval() {
        // Revue : 30m chargé, passage en 1h échoué → r rechargeait le 30m
        let (mut app, now) = app(&["AAPL", "TSLA"]);
        for symbol in ["AAPL", "TSLA"] {
            app.apply_result(
                AppResult::Loaded {
                    symbol: symbol.into(),
                    fetched: fetched(symbol, Interval::M30),
                },
                now,
            );
        }
        app.open_chart();
        app.current_interval = Interval::H1;
        let commands = app.refresh_commands(now);
        assert!(commands.contains(&AppCommand::Load {
            symbol: "AAPL".into(),
            interval: Interval::H1
        }));
        assert!(commands.contains(&AppCommand::Load {
            symbol: "TSLA".into(),
            interval: Interval::M30
        }));
    }

    #[test]
    fn weekly_data_is_refreshed_at_default_interval_off_screen() {
        // Revue : un ticker laissé en 1w gardait une clôture de veille périmée
        let (mut app, now) = app(&["AAPL", "TSLA"]);
        app.apply_result(
            AppResult::Loaded {
                symbol: "TSLA".into(),
                fetched: fetched("TSLA", Interval::W1),
            },
            now,
        );
        let commands = app.refresh_commands(now);
        assert!(commands.contains(&AppCommand::Load {
            symbol: "TSLA".into(),
            interval: Interval::default()
        }));
    }

    /// Chandelles horaires `from..from+len`, prix = index : chacune a sa propre date
    fn candles(from: u32, len: u32) -> Vec<OHLC> {
        let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        (from..from + len)
            .map(|i| {
                let p = f64::from(i);
                OHLC::new(t0 + chrono::Duration::hours(i64::from(i)), p, p, p, p, 0)
            })
            .collect()
    }

    fn reload(app: &mut App, candles: Vec<OHLC>) {
        let mut fetched = fetched("AAPL", Interval::M30);
        fetched.data.candles = candles;
        app.pending = 1;
        app.apply_result(
            AppResult::Loaded {
                symbol: "AAPL".into(),
                fetched,
            },
            Instant::now(),
        );
    }

    /// App sur le graphique d'un ticker avec `len` chandelles, `visible` affichées
    fn chart_app(len: u32, visible: usize) -> App {
        let (mut app, now) = app(&["AAPL"]);
        let mut fetched = fetched("AAPL", Interval::M30);
        fetched.data.candles = candles(0, len);
        app.apply_result(
            AppResult::Loaded {
                symbol: "AAPL".into(),
                fetched,
            },
            now,
        );
        app.open_chart();
        let len = usize::try_from(len).unwrap();
        app.chart_view.set(Some((len - visible, len)));
        app
    }

    #[test]
    fn first_arrow_shows_cursor_on_latest_visible_candle() {
        let mut app = chart_app(100, 20);
        app.move_cursor(-1);
        assert_eq!((app.cursor, app.chart_offset), (Some(99), 0));
        app.move_cursor(-1);
        assert_eq!(app.cursor, Some(98));
    }

    #[test]
    fn cursor_past_the_left_edge_scrolls_back_in_time() {
        let mut app = chart_app(100, 20); // visibles : 80..100
        app.cursor = Some(80);
        app.move_cursor(-1);
        assert_eq!((app.cursor, app.chart_offset), (Some(79), 1));
        // Le rendu suivant montrerait 79..99 ; à droite on revient vers le présent
        app.chart_view.set(Some((79, 99)));
        app.cursor = Some(98);
        app.move_cursor(1);
        assert_eq!((app.cursor, app.chart_offset), (Some(99), 0));
    }

    #[test]
    fn cursor_and_scroll_are_clamped_at_the_edges() {
        let mut app = chart_app(30, 20);
        app.cursor = Some(0);
        app.chart_view.set(Some((0, 20)));
        app.move_cursor(-1);
        assert_eq!(app.cursor, Some(0));
        app.cursor = Some(29);
        app.chart_view.set(Some((10, 30)));
        app.move_cursor(1);
        assert_eq!((app.cursor, app.chart_offset), (Some(29), 0));
        for _ in 0..10 {
            app.scroll_chart(-1); // demi-page vers le passé, 10 fois
        }
        assert_eq!(app.chart_offset, 29, "au moins une chandelle reste visible");
        app.scroll_chart(1);
        assert_eq!(app.chart_offset, 19);
    }

    #[test]
    fn end_key_and_interval_change_reset_navigation() {
        let mut app = chart_app(100, 20);
        app.cursor = Some(50);
        app.chart_offset = 40;
        app.reset_chart_view();
        assert_eq!((app.cursor, app.chart_offset), (None, 0));
        app.cursor = Some(50);
        app.chart_offset = 40;
        app.change_interval(Interval::next);
        assert_eq!((app.cursor, app.chart_offset), (None, 0));
    }

    #[test]
    fn shrinking_reload_while_scrolled_does_not_panic() {
        // Revue : vue périmée (168..256) + données réduites à 20 → soustraction négative
        let mut app = chart_app(300, 88);
        app.scroll_chart(-1);
        app.chart_view.set(Some((168, 256)));
        reload(&mut app, candles(0, 20));
        app.scroll_chart(-1);
        app.move_cursor(-1);
        assert!(app.chart_offset < 20);
        assert!(app.cursor.is_some_and(|c| c < 20));
    }

    #[test]
    fn refresh_keeps_cursor_and_view_on_the_same_candles() {
        // Revue : fenêtre glissante (5 anciennes chandelles en moins, 3 nouvelles)
        // → le curseur désignait une autre date
        let mut app = chart_app(100, 20);
        app.chart_offset = 10; // fin de vue : chandelle 89
        app.cursor = Some(80);
        reload(&mut app, candles(5, 98)); // chandelles 5..103
        let data = app.selected_item().unwrap().data.as_ref().unwrap();
        let at = |i: usize| data.candles[i].close;
        assert_eq!(
            app.cursor.map(at),
            Some(80.0),
            "même chandelle sous le curseur"
        );
        let end = data.len() - app.chart_offset;
        assert!((at(end - 1) - 89.0).abs() < f64::EPSILON, "même fin de vue");
    }
}
