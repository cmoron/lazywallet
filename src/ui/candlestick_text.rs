// ============================================================================
// Candlestick Chart - Rendu texte ligne par ligne
// ============================================================================
// Implémentation inspirée de cli-candlestick-chart mais intégrée à ratatui
// Utilise des caractères Unicode pour dessiner les chandeliers japonais
//
// ALGORITHME :
// - Rendu vertical : ligne par ligne de haut en bas
// - Pour chaque ligne, on détermine quel caractère Unicode afficher
// - Logique des 3 zones : mèche supérieure, corps, mèche inférieure
// - Seuils fractionnaires (0.25, 0.75) pour précision sub-caractère
//
// CARACTÈRES UNICODE :
// ┃ Corps plein          │ Mèche pleine
// ╻ Demi-corps (bas)     ╹ Demi-corps (haut)
// ╽ Transition top       ╿ Transition bottom
// ╷ Demi-mèche sup       ╵ Demi-mèche inf
// ============================================================================

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use chrono::Timelike;

use crate::app::App;
use crate::models::{Interval, OHLC};

// ============================================================================
// Constantes
// ============================================================================

/// Caractères Unicode pour le rendu des chandeliers
const UNICODE_VOID: char = ' ';
const UNICODE_BODY: char = '┃';              // Corps plein
const UNICODE_HALF_BODY_BOTTOM: char = '╻';  // Corps avec espace en bas
const UNICODE_HALF_BODY_TOP: char = '╹';     // Corps avec espace en haut
const UNICODE_WICK: char = '│';              // Mèche pleine
const UNICODE_TOP: char = '╽';               // Transition corps→mèche (haut)
const UNICODE_BOTTOM: char = '╿';            // Transition corps→mèche (bas)
const UNICODE_UPPER_WICK: char = '╷';        // Demi-mèche supérieure
const UNICODE_LOWER_WICK: char = '╵';        // Demi-mèche inférieure

/// Couleurs pour chandeliers haussiers et baissiers
const BULLISH_COLOR: Color = Color::Rgb(52, 208, 88);   // Vert
const BEARISH_COLOR: Color = Color::Rgb(234, 74, 90);   // Rouge

/// Largeur de l'axe Y (pour les prix)
const Y_AXIS_WIDTH: u16 = 12;

/// Constantes pour le design réactif (Bug 6)
/// CONCEPT : Responsive terminal design
/// - MIN_TERMINAL_WIDTH : largeur minimale absolue pour afficher le graphique
/// - ADAPTIVE_Y_AXIS_THRESHOLD : en dessous, on réduit la largeur de l'axe Y
/// - NARROW_Y_AXIS_WIDTH : largeur réduite de l'axe Y pour terminaux étroits
const MIN_TERMINAL_WIDTH: u16 = 60;
const ADAPTIVE_Y_AXIS_THRESHOLD: u16 = 80;
const NARROW_Y_AXIS_WIDTH: u16 = 8;

// ============================================================================
// Structure principale
// ============================================================================

/// Renderer de chandeliers japonais en mode texte
pub struct CandlestickRenderer<'a> {
    candles: &'a [OHLC],
    interval: Interval,
    min_price: f64,
    max_price: f64,
    height: u16,
    width: u16,
    y_axis_width: u16,
}

/// Position d'un chandelier dans le graphique
///
/// CONCEPT : Single source of truth for alignment
/// - Toutes les couches (chandeliers, ticks, labels, dates) utilisent les mêmes positions
/// - Garantit l'alignement parfait chandelier ↔ timestamp
#[derive(Debug, Clone, Copy)]
struct CandlePosition {
    /// Position absolue de la colonne (0-based depuis le début de la zone graphique)
    column: usize,
    /// Nombre de caractères alloués à ce chandelier (généralement 1)
    width: usize,
}

impl<'a> CandlestickRenderer<'a> {
    /// Crée un nouveau renderer
    ///
    /// CONCEPT : Responsive design
    /// - Adapte la largeur de l'axe Y selon la largeur du terminal
    /// - Largeur < 80 cols : axe Y réduit à 8 caractères
    /// - Largeur >= 80 cols : axe Y normal à 12 caractères
    pub fn new(candles: &'a [OHLC], interval: Interval, area: Rect) -> Self {
        // Calcule les bornes de prix
        let (min_price, max_price) = Self::compute_price_bounds(candles);

        // Largeur adaptative de l'axe Y selon la largeur du terminal
        let y_axis_width = if area.width < ADAPTIVE_Y_AXIS_THRESHOLD {
            NARROW_Y_AXIS_WIDTH  // Mode étroit : 8 caractères
        } else {
            Y_AXIS_WIDTH  // Mode normal : 12 caractères
        };

        Self {
            candles,
            interval,
            min_price,
            max_price,
            // Réserve 3 pour header + 3 pour x-axis (ticks + labels + dates) = 6 lignes
            height: area.height.saturating_sub(6),
            width: area.width.saturating_sub(y_axis_width),
            y_axis_width,
        }
    }

    /// Calcule les prix min et max sur tous les chandeliers
    fn compute_price_bounds(candles: &[OHLC]) -> (f64, f64) {
        let max_price = candles
            .iter()
            .fold(f64::NEG_INFINITY, |max, c| max.max(c.high));

        let min_price = candles
            .iter()
            .fold(f64::INFINITY, |min, c| min.min(c.low));

        // Ajoute une marge de 2%
        let margin = (max_price - min_price) * 0.02;
        (
            (min_price - margin).max(0.0),
            max_price + margin,
        )
    }

    /// Convertit un prix en coordonnée de hauteur
    fn price_to_height(&self, price: f64) -> f64 {
        if self.max_price == self.min_price {
            return self.height as f64 / 2.0;
        }

        (price - self.min_price) / (self.max_price - self.min_price) * self.height as f64
    }

    /// Détermine si un chandelier est haussier (bullish)
    fn is_bullish(candle: &OHLC) -> bool {
        candle.close >= candle.open
    }

    /// Retourne la couleur du chandelier
    fn candle_color(candle: &OHLC) -> Color {
        if Self::is_bullish(candle) {
            BULLISH_COLOR
        } else {
            BEARISH_COLOR
        }
    }

    /// Rend un chandelier à une hauteur donnée
    ///
    /// Ceci est le cœur de l'algorithme, adapté de cli-candlestick-chart.
    /// Il détermine quel caractère Unicode afficher selon la position verticale.
    fn render_candle(&self, candle: &OHLC, y: u16) -> char {
        let height_unit = y as f64;

        // Convertit les prix en coordonnées de hauteur
        let high_y = self.price_to_height(candle.high);
        let low_y = self.price_to_height(candle.low);
        let max_y = self.price_to_height(candle.open.max(candle.close));
        let min_y = self.price_to_height(candle.close.min(candle.open));

        let mut output = UNICODE_VOID;

        // ========================================
        // ZONE 1 : Mèche supérieure (high → max)
        // ========================================
        if high_y.ceil() >= height_unit && height_unit >= max_y.floor() {
            if max_y - height_unit > 0.75 {
                // Corps s'étend significativement dans cette ligne
                output = UNICODE_BODY;
            } else if (max_y - height_unit) > 0.25 {
                // Corps partiellement présent
                if (high_y - height_unit) > 0.75 {
                    // Mèche s'étend aussi → transition
                    output = UNICODE_TOP;
                } else {
                    // Juste le corps avec espace
                    output = UNICODE_HALF_BODY_BOTTOM;
                }
            } else if (high_y - height_unit) > 0.75 {
                // Que la mèche, pleine
                output = UNICODE_WICK;
            } else if (high_y - height_unit) > 0.25 {
                // Demi-mèche
                output = UNICODE_UPPER_WICK;
            }
        }
        // ========================================
        // ZONE 2 : Corps (min → max)
        // ========================================
        else if max_y.floor() >= height_unit && height_unit >= min_y.ceil() {
            // Toujours corps plein dans la zone du corps
            output = UNICODE_BODY;
        }
        // ========================================
        // ZONE 3 : Mèche inférieure (min → low)
        // ========================================
        else if min_y.ceil() >= height_unit && height_unit >= low_y.floor() {
            if (min_y - height_unit) < 0.25 {
                // Corps encore très proche
                output = UNICODE_BODY;
            } else if (min_y - height_unit) < 0.75 {
                // Corps partiellement présent
                if (low_y - height_unit) < 0.25 {
                    // Mèche proche aussi → transition
                    output = UNICODE_BOTTOM;
                } else {
                    // Juste le corps avec espace
                    output = UNICODE_HALF_BODY_TOP;
                }
            } else if low_y - height_unit < 0.25 {
                // Que la mèche, pleine
                output = UNICODE_WICK;
            } else if low_y - height_unit < 0.75 {
                // Demi-mèche
                output = UNICODE_LOWER_WICK;
            }
        }

        output
    }

    /// Rend une ligne de l'axe Y avec le prix
    fn render_y_axis(&self, y: u16) -> String {
        // Affiche le prix tous les 4 lignes
        if y % 4 == 0 {
            let price = self.min_price
                + (y as f64 * (self.max_price - self.min_price) / self.height as f64);
            format!("{:>9.2} │ ", price)
        } else {
            format!("{:>9} │ ", "")
        }
    }

    /// Sélectionne les chandeliers visibles (les N derniers qui tiennent à l'écran)
    fn visible_candles(&self) -> &[OHLC] {
        let max_visible = self.width as usize;
        if self.candles.len() <= max_visible {
            self.candles
        } else {
            &self.candles[self.candles.len() - max_visible..]
        }
    }

    /// Pré-calcule les positions exactes de chaque chandelier
    ///
    /// CONCEPT : Accumulator pattern pour éviter le drift
    /// - Chaque position = index × spacing (pas position_précédente + spacing)
    /// - Évite l'accumulation d'erreurs d'arrondi
    /// - Garantit que chandeliers et labels utilisent les mêmes positions
    ///
    /// Cas gérés :
    /// - Terminal trop étroit : 1 chandelier par colonne (spacing ≈ 1.0)
    /// - Terminal trop large : chandeliers répartis uniformément (spacing > 1.0)
    /// - Spacing fractionnaire : accumulator évite le drift
    /// - Chandelier unique : centré dans la largeur disponible
    fn compute_candle_positions(chart_width: usize, num_candles: usize) -> Vec<CandlePosition> {
        if num_candles == 0 {
            return Vec::new();
        }

        if num_candles == 1 {
            // Cas spécial : chandelier unique centré
            return vec![CandlePosition {
                column: chart_width / 2,
                width: 1,
            }];
        }

        let mut positions = Vec::with_capacity(num_candles);
        let spacing = chart_width as f64 / num_candles as f64;

        for i in 0..num_candles {
            // Pattern accumulator : calcul depuis l'index, pas depuis la position précédente
            // Cela évite l'accumulation d'erreurs d'arrondi sur plusieurs chandeliers
            let exact_position = i as f64 * spacing;
            let column = exact_position.round() as usize;

            positions.push(CandlePosition {
                column: column.min(chart_width.saturating_sub(1)),
                width: 1,
            });
        }

        positions
    }

    /// Génère toutes les lignes du graphique (chandeliers + axe X)
    pub fn render_lines(&self) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let visible = self.visible_candles();

        if visible.is_empty() {
            return lines;
        }

        // Calcule l'espacement entre chandeliers pour remplir toute la largeur
        // Chaque chandelier = 1 caractère + espaces après
        let spacing = if visible.len() > 1 {
            self.width as f64 / visible.len() as f64
        } else {
            1.0
        };

        // Parcourt de haut en bas (reversed)
        for y in (1..=self.height).rev() {
            let mut spans = Vec::new();

            // Ajoute l'axe Y
            spans.push(Span::styled(
                self.render_y_axis(y),
                Style::default().fg(Color::Gray),
            ));

            // Ajoute chaque chandelier avec espacement
            for (i, candle) in visible.iter().enumerate() {
                let ch = self.render_candle(candle, y);
                let color = Self::candle_color(candle);

                // Ajoute le caractère du chandelier
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(color),
                ));

                // Ajoute l'espacement après (sauf pour le dernier)
                if i < visible.len() - 1 {
                    let num_spaces = (spacing - 1.0).round() as usize;
                    if num_spaces > 0 {
                        spans.push(Span::raw(" ".repeat(num_spaces)));
                    }
                }
            }

            lines.push(Line::from(spans));
        }

        // Ajoute l'axe X (2 lignes)
        lines.extend(self.render_x_axis(visible, spacing));

        lines
    }

    /// Génère les lignes de l'axe X avec tick marks et labels
    ///
    /// CONCEPT : Dates au changement de jour
    /// - Ligne 1 : Tick marks (│)
    /// - Ligne 2 : Labels principaux (heures ou dates)
    /// - Ligne 3 : Dates aux changements de jour (intraday seulement)
    fn render_x_axis(&self, visible: &[OHLC], spacing: f64) -> Vec<Line<'a>> {
        let mut lines = vec![];

        let (format_str, is_intraday) = self.interval.x_axis_format();

        // Calcule la largeur estimée des labels selon l'intervalle
        // CONCEPT : Adaptive label spacing
        // - Tient compte de la largeur réelle du format de label
        // - Évite les chevauchements en garantissant un espacement minimum
        let estimated_label_width = match self.interval {
            Interval::M5 | Interval::M15 | Interval::M30 | Interval::H1 => 5,  // "HH:MM" = 5 chars
            Interval::H4 => 9,   // "DD/MM HHh" = 9 chars max (ex: "26/11 8h")
            Interval::D1 => 5,   // "DD/MM" = 5 chars
            Interval::W1 => 6,   // "DD Mon" = 6 chars
        };

        // Calcule combien de labels on peut afficher
        // +2 pour garantir au moins 2 caractères d'espacement entre labels
        let min_space_per_label = estimated_label_width + 2;
        let max_labels = (self.width as usize / min_space_per_label).max(2).min(10);

        // Détermine quels chandeliers auront un label
        let label_interval = if visible.len() <= max_labels {
            1
        } else {
            visible.len() / max_labels
        };

        // Ligne 1 : Tick marks
        let mut tick_spans = vec![Span::raw(format!("{:>width$}", "", width = self.y_axis_width as usize))];

        for (i, _candle) in visible.iter().enumerate() {
            let tick = if i % label_interval == 0 {
                "│"
            } else {
                " "
            };

            tick_spans.push(Span::styled(tick, Style::default().fg(Color::Gray)));

            if i < visible.len() - 1 {
                let num_spaces = (spacing - 1.0).round() as usize;
                if num_spaces > 0 {
                    tick_spans.push(Span::raw(" ".repeat(num_spaces)));
                }
            }
        }

        lines.push(Line::from(tick_spans));

        // Ligne 2 : Labels de temps
        let mut label_spans = vec![Span::raw(format!("{:>width$}", "", width = self.y_axis_width as usize))];

        let mut position = 0.0;
        for (i, candle) in visible.iter().enumerate() {
            if i % label_interval == 0 {
                let time_label = candle.timestamp.format(format_str).to_string();

                label_spans.push(Span::styled(
                    time_label.clone(),
                    Style::default().fg(Color::Gray),
                ));

                let next_label_position = if i + label_interval < visible.len() {
                    (i + label_interval) as f64 * spacing
                } else {
                    self.width as f64
                };

                let space_to_next = (next_label_position - position - time_label.len() as f64).max(0.0) as usize;
                if space_to_next > 0 {
                    label_spans.push(Span::raw(" ".repeat(space_to_next)));
                }

                position = next_label_position;
            }
        }

        lines.push(Line::from(label_spans));

        // Ligne 3 : Dates aux changements de jour (pour intraday)
        if is_intraday {
            let mut date_spans = vec![Span::raw(format!("{:>width$}", "", width = self.y_axis_width as usize))];

            let mut current_position = 0.0;
            let mut last_day = None;
            let mut last_date_end_position = 0.0;
            const MIN_DATE_SPACING: f64 = 2.0; // Espace minimum entre deux dates

            for (i, candle) in visible.iter().enumerate() {
                let current_day = candle.timestamp.date_naive();

                // Détecte le changement de jour
                let is_day_change = if let Some(prev_day) = last_day {
                    current_day != prev_day
                } else {
                    // Premier chandelier de la série : afficher seulement si c'est le début de journée
                    candle.timestamp.hour() < 2
                };

                if is_day_change {
                    // Calcule la position de ce chandelier
                    let candle_position = i as f64 * spacing;

                    // Vérifie qu'on a assez d'espace depuis la dernière date
                    let space_from_last_date = candle_position - last_date_end_position;

                    if space_from_last_date >= MIN_DATE_SPACING || last_day.is_none() {
                        // Ajoute des espaces pour arriver à cette position
                        let spaces_needed = (candle_position - current_position).max(0.0) as usize;
                        if spaces_needed > 0 {
                            date_spans.push(Span::raw(" ".repeat(spaces_needed)));
                        }

                        // Ajoute le label de date
                        let date_label = candle.timestamp.format("%d/%m").to_string();
                        date_spans.push(Span::styled(
                            date_label.clone(),
                            Style::default().fg(Color::Rgb(120, 120, 120)),
                        ));

                        current_position = candle_position + date_label.len() as f64;
                        last_date_end_position = current_position;
                    }
                }

                last_day = Some(current_day);
            }

            lines.push(Line::from(date_spans));
        }

        lines
    }
}

// ============================================================================
// Fonction principale de rendu
// ============================================================================

/// Dessine un graphique en chandeliers japonais pour le ticker sélectionné
pub fn render_candlestick_chart(frame: &mut Frame, app: &App, area: Rect) {
    // Récupère le ticker sélectionné
    let item = match app.watchlist.get(app.selected_index) {
        Some(item) => item,
        None => {
            render_no_data(frame, area, "Aucun ticker sélectionné");
            return;
        }
    };

    // Vérifie que le ticker a des données
    let data = match &item.data {
        Some(data) => data,
        None => {
            let msg = format!("Pas de données pour {}", item.symbol);
            render_no_data(frame, area, &msg);
            return;
        }
    };

    if data.candles.is_empty() {
        render_no_data(frame, area, "Pas de données à afficher");
        return;
    }

    // Vérifie si le terminal est assez large pour afficher le graphique
    // CONCEPT : Graceful degradation pour terminaux étroits
    if area.width < MIN_TERMINAL_WIDTH {
        render_too_narrow(frame, area);
        return;
    }

    // Crée le layout : header + graphique
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),      // Graphique
        ])
        .split(area)
        .to_vec();

    // Dessine le header
    render_header(frame, app, item, chunks[0]);

    // Crée le renderer et génère les lignes
    let renderer = CandlestickRenderer::new(&data.candles, data.interval, chunks[1]);
    let lines = renderer.render_lines();

    // Crée le widget Paragraph avec les lignes
    // Note : data.interval = interval des données chargées
    //        app.current_interval = interval sélectionné par l'utilisateur
    let displayed_interval = app.current_interval.label();
    let data_interval = data.interval.label();

    // Indicateur si l'intervalle sélectionné diffère des données chargées
    let interval_display = if displayed_interval != data_interval {
        format!("{} → {} ⚠️ ", data_interval, displayed_interval)
    } else {
        format!("{} ", displayed_interval)
    };

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White))
            .title(format!(
                " 🕯️ {} - {}({}, {} chandeliers) [h/l: changer interval] ",
                item.symbol,
                interval_display,
                data.timeframe.label(),
                data.candles.len()
            )),
    );

    frame.render_widget(paragraph, chunks[1]);
}

// ============================================================================
// Header
// ============================================================================

/// Dessine le header avec infos du ticker
fn render_header(frame: &mut Frame, app: &App, item: &crate::models::WatchlistItem, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" 🕯️ {} - {} ", item.symbol, item.name));

    // CONCEPT : Confirmation de quit two-step et loading indicator
    // - Si app.is_awaiting_quit_confirmation(), affiche message d'avertissement
    // - Si app.is_loading_data(), affiche indicateur de chargement
    // - Sinon, affiche les infos normales avec shortcuts
    let text = if app.is_awaiting_quit_confirmation() {
        // Message de confirmation de quit
        vec![Line::from(vec![
            Span::styled(
                "⚠  Appuyez sur ",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[q]",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::styled(
                " à nouveau pour quitter, ou n'importe quelle autre touche pour annuler ⚠",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ])]
    } else if app.is_loading_data() {
        // Indicateur de chargement
        let message = app.loading_message.clone().unwrap_or_else(|| "Chargement en cours...".to_string());
        vec![Line::from(vec![
            Span::styled(
                "⏳ ",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                message,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ])]
    } else if let (Some(price), Some(change)) = (item.current_price(), item.change_percent()) {
        let color = if change >= 0.0 { Color::Green } else { Color::Red };
        let arrow = if change >= 0.0 { "▲" } else { "▼" };

        vec![Line::from(vec![
            Span::raw("Prix: "),
            Span::styled(
                format!("${:.2}", price),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(format!("{} {:+.2}%", arrow, change), Style::default().fg(color)),
            Span::raw("  "),
            Span::styled(
                "[ESC]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Retour  "),
            Span::styled(
                "[q]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Quitter"),
        ])]
    } else {
        vec![Line::from("Chargement...")]
    };

    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

// ============================================================================
// Helper : Message d'erreur
// ============================================================================

/// Affiche un message quand il n'y a pas de données
fn render_no_data(frame: &mut Frame, area: Rect, message: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(" ⚠ Erreur ");

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default().fg(Color::Red),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "[ESC] Retour",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Affiche un message quand le terminal est trop étroit
///
/// CONCEPT : Responsive design - graceful degradation
/// - Prévient les problèmes d'affichage sur terminaux très étroits
/// - Informe clairement l'utilisateur de la largeur minimale requise
fn render_too_narrow(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" ⚠ Terminal trop petit ");

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Terminal trop étroit pour afficher le graphique",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Largeur minimale requise : {} colonnes", MIN_TERMINAL_WIDTH),
            Style::default().fg(Color::Gray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "[ESC] Retour",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

// ============================================================================
// Notes d'implémentation
// ============================================================================
//
// ALGORITHME INSPIRÉ DE : cli-candlestick-chart
// Source : https://github.com/Julien-R44/cli-candlestick-chart
//
// PRINCIPE :
// - Rendu ligne par ligne de haut en bas (reversed)
// - 3 zones : mèche sup, corps, mèche inf
// - Seuils 0.25 et 0.75 pour sub-caractère précision
// - Caractères Unicode box-drawing pour rendu professionnel
//
// AVANTAGES :
// ✓ Rendu professionnel identique à cli-candlestick-chart
// ✓ Intégration native ratatui (Paragraph + Line + Span)
// ✓ Pas de bugs externes
// ✓ Code maîtrisé et extensible
// ✓ Performant : O(hauteur × nb_chandeliers)
//
// AMÉLIORATIONS POSSIBLES :
// - Ajouter volume en sous-graphique
// - Indicateurs techniques (MA, RSI, Bollinger, etc.)
// - Zoom et navigation horizontale
// - Curseur pour afficher OHLC au survol
//
// ============================================================================
