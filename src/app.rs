// ============================================================================
// Structure : App
// ============================================================================
// Gère l'état global de l'application TUI
//
// CONCEPTS RUST :
// 1. State Management : centraliser l'état dans une seule structure
// 2. Mutabilité contrôlée : &mut self pour modifier l'état
// 3. Encapsulation : les champs sont privés, accès via méthodes publiques
//
// PATTERN : Cette structure suit le pattern "Application State"
// - Tous les composants de l'UI lisent depuis App
// - Toutes les modifications passent par les méthodes de App
// - Garantit la cohérence de l'état
// ============================================================================

use crate::models::{Interval, WatchlistItem};

// ============================================================================
// Enum : Screen
// ============================================================================
// CONCEPT RUST : Enums pour state machines
// - Représente les différents écrans de l'application
// - Pattern "State Machine" : un seul écran actif à la fois
// - Le compilateur force à gérer tous les cas (exhaustivité)
// ============================================================================

/// Écrans de l'application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// Vue principale : liste des tickers (watchlist)
    Dashboard,

    /// Vue graphique : graphique du ticker sélectionné
    ChartView,

    /// Mode saisie : permet de capturer du texte utilisateur
    /// CONCEPT : Modal input mode (Vim-like)
    /// - Capture les touches pour construire un buffer
    /// - Enter valide, ESC annule
    InputMode,
}

/// État principal de l'application
///
/// CONCEPT RUST : Struct avec champs privés
/// - Par défaut, tous les champs sont privés au module
/// - L'extérieur ne peut que lire/modifier via les méthodes publiques
/// - Encapsulation et contrôle total sur l'état
pub struct App {
    /// Indique si l'application doit continuer à tourner
    pub running: bool,

    /// Liste des tickers à surveiller (watchlist)
    pub watchlist: Vec<WatchlistItem>,

    /// Index du ticker sélectionné dans la watchlist
    pub selected_index: usize,

    /// Écran actuellement affiché
    /// CONCEPT RUST : Enum pour state management
    /// - Screen::Dashboard : vue watchlist
    /// - Screen::ChartView : vue graphique
    /// - Un seul écran actif à la fois (state machine)
    pub current_screen: Screen,

    /// Intervalle actuel pour les graphiques (1m, 5m, 30m, 1h, 1d, etc.)
    /// Peut être modifié avec les touches [ et ]
    pub current_interval: Interval,

    /// Indique si l'utilisateur a demandé à quitter (attend confirmation)
    /// CONCEPT : Two-step quit pour éviter les sorties accidentelles
    /// - Première pression de 'q' : confirm_quit = true
    /// - Deuxième pression de 'q' : running = false (quit réel)
    /// - N'importe quelle autre touche : confirm_quit = false (annulation)
    pub confirm_quit: bool,

    /// Indique si des données sont en cours de chargement
    /// CONCEPT : Background loading state
    /// - true : affiche un indicateur de chargement
    /// - false : affichage normal
    pub is_loading: bool,

    /// Message de chargement optionnel
    /// CONCEPT : Status message pour l'utilisateur
    /// - Some(msg) : affiche le message pendant le chargement
    /// - None : pas de message spécifique
    pub loading_message: Option<String>,

    /// Buffer de saisie pour le mode Input
    /// CONCEPT : Input buffer (Vim-like)
    /// - Contient le texte en cours de saisie
    /// - Vidé après validation ou annulation
    pub input_buffer: String,

    /// Prompt affiché en mode Input
    /// CONCEPT : User prompt
    /// - Ex: "Add ticker: ", "Search: ", etc.
    pub input_prompt: String,

    /// Indique si l'utilisateur a demandé à supprimer un item (attend confirmation)
    /// CONCEPT : Two-step delete pour éviter les suppressions accidentelles
    /// - Première pression de 'd' : confirm_delete = true
    /// - Deuxième pression de 'd' : suppression réelle
    /// - N'importe quelle autre touche : confirm_delete = false (annulation)
    pub confirm_delete: bool,
}

impl App {
    /// Crée une nouvelle instance de App avec une watchlist vide
    ///
    /// CONCEPT RUST : Constructor pattern
    /// - Convention : fonction associée nommée "new()"
    /// - Retourne Self (alias pour le type App)
    /// - Initialise tous les champs avec des valeurs par défaut
    pub fn new() -> Self {
        Self {
            running: true,
            watchlist: Vec::new(),
            selected_index: 0,
            current_screen: Screen::Dashboard,  // Commence sur le dashboard
            current_interval: Interval::default(), // 30m par défaut
            confirm_quit: false,
            is_loading: false,
            loading_message: None,
            input_buffer: String::new(),
            input_prompt: String::new(),
            confirm_delete: false,
        }
    }

    /// Crée une App avec une watchlist préchargée
    pub fn with_watchlist(watchlist: Vec<WatchlistItem>) -> Self {
        Self {
            running: true,
            watchlist,
            selected_index: 0,
            current_screen: Screen::Dashboard,
            current_interval: Interval::default(), // 30m par défaut
            confirm_quit: false,
            is_loading: false,
            loading_message: None,
            input_buffer: String::new(),
            input_prompt: String::new(),
            confirm_delete: false,
        }
    }

    /// Quitte l'application
    ///
    /// CONCEPT RUST : &mut self
    /// - self est une référence mutable (on peut modifier l'objet)
    /// - L'appelant doit avoir une référence mutable de App
    /// - Borrow checker s'assure qu'il n'y a qu'une seule ref mutable
    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Navigue vers le haut dans la watchlist
    ///
    /// CONCEPT RUST : Saturating arithmetic
    /// - saturating_sub() : soustrait mais ne descend pas en dessous de 0
    /// - Évite les panics avec les unsigned
    pub fn navigate_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    /// Navigue vers le bas dans la watchlist
    ///
    /// CONCEPT RUST : min() pour éviter le dépassement
    /// - Limite l'index à watchlist.len() - 1
    /// - saturating_sub(1) gère le cas watchlist vide (0 - 1 = 0)
    pub fn navigate_down(&mut self) {
        let max_index = self.watchlist.len().saturating_sub(1);
        self.selected_index = (self.selected_index + 1).min(max_index);
    }

    /// Retourne l'item sélectionné dans la watchlist
    ///
    /// CONCEPT RUST : Option<&T>
    /// - Retourne une référence à l'item (pas de copie)
    /// - None si la watchlist est vide
    pub fn selected_item(&self) -> Option<&WatchlistItem> {
        self.watchlist.get(self.selected_index)
    }

    /// Tick : appelé à chaque itération de la boucle
    ///
    /// CONCEPT : Event Loop Pattern
    /// - tick() est appelé régulièrement (chaque frame)
    /// - Permet de mettre à jour l'état même sans événement utilisateur
    /// - Utile pour animations, compteurs, rafraîchissements auto
    ///
    /// Pour l'instant c'est vide, mais on ajoutera du code plus tard
    /// (ex: décrémenter un compteur de rafraîchissement)
    pub fn tick(&mut self) {
        // Pour l'instant, rien à faire à chaque tick
        // Dans les prochaines étapes :
        // - Décrémenter un timer de rafraîchissement
        // - Mettre à jour des animations
        // - etc.
    }

    /// Vérifie si l'application doit continuer
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Affiche la vue graphique (ChartView)
    ///
    /// CONCEPT RUST : State transition
    /// - Change l'état de current_screen
    /// - Pattern "State Machine" : transition Dashboard → ChartView
    pub fn show_chart(&mut self) {
        self.current_screen = Screen::ChartView;
    }

    /// Retourne à la vue dashboard
    pub fn show_dashboard(&mut self) {
        self.current_screen = Screen::Dashboard;
    }

    /// Vérifie si on est sur le dashboard
    pub fn is_on_dashboard(&self) -> bool {
        self.current_screen == Screen::Dashboard
    }

    /// Vérifie si on est sur la vue graphique
    pub fn is_on_chart(&self) -> bool {
        self.current_screen == Screen::ChartView
    }

    /// Passe à l'intervalle suivant
    ///
    /// CONCEPT : Cycle d'états
    /// - M1 → M5 → M15 → M30 → H1 → H4 → D1 → W1 → M1
    /// - Utilisé avec la touche ]
    pub fn next_interval(&mut self) {
        self.current_interval = self.current_interval.next();
    }

    /// Passe à l'intervalle précédent
    ///
    /// CONCEPT : Cycle d'états (inverse)
    /// - W1 → D1 → H4 → H1 → M30 → M15 → M5 → M1 → W1
    /// - Utilisé avec la touche [
    pub fn previous_interval(&mut self) {
        self.current_interval = self.current_interval.previous();
    }

    /// Demande la confirmation de quitter
    ///
    /// CONCEPT : Two-step quit pattern
    /// - Appelé lors de la première pression de 'q'
    /// - Active l'état confirm_quit pour attendre une seconde pression
    /// - Évite les sorties accidentelles
    pub fn request_quit(&mut self) {
        self.confirm_quit = true;
    }

    /// Annule la demande de quit
    ///
    /// CONCEPT : Reset de l'état de confirmation
    /// - Appelé quand l'utilisateur presse une touche autre que 'q'
    /// - Remet confirm_quit à false
    pub fn cancel_quit(&mut self) {
        self.confirm_quit = false;
    }

    /// Vérifie si on attend la confirmation de quit
    pub fn is_awaiting_quit_confirmation(&self) -> bool {
        self.confirm_quit
    }

    /// Démarre le chargement avec un message optionnel
    ///
    /// CONCEPT : Loading state management
    /// - Active is_loading pour afficher l'indicateur
    /// - Stocke le message pour l'utilisateur
    pub fn start_loading(&mut self, message: Option<String>) {
        self.is_loading = true;
        self.loading_message = message;
    }

    /// Termine le chargement
    pub fn stop_loading(&mut self) {
        self.is_loading = false;
        self.loading_message = None;
    }

    /// Vérifie si des données sont en cours de chargement
    pub fn is_loading_data(&self) -> bool {
        self.is_loading
    }

    // ========================================================================
    // Input Mode Management
    // ========================================================================

    /// Entre en mode input avec un prompt donné
    ///
    /// CONCEPT : Modal input (Vim-like)
    /// - Change l'écran vers InputMode
    /// - Initialise le buffer vide
    /// - Configure le prompt à afficher
    pub fn start_input(&mut self, prompt: String) {
        self.current_screen = Screen::InputMode;
        self.input_buffer.clear();
        self.input_prompt = prompt;
    }

    /// Annule le mode input et retourne au dashboard
    pub fn cancel_input(&mut self) {
        self.current_screen = Screen::Dashboard;
        self.input_buffer.clear();
        self.input_prompt.clear();
    }

    /// Récupère la valeur saisie et retourne au dashboard
    ///
    /// CONCEPT : Consume input
    /// - Retourne le contenu du buffer
    /// - Vide le buffer
    /// - Retourne au dashboard
    pub fn submit_input(&mut self) -> String {
        let value = self.input_buffer.clone();
        self.current_screen = Screen::Dashboard;
        self.input_buffer.clear();
        self.input_prompt.clear();
        value
    }

    /// Ajoute un caractère au buffer d'input
    pub fn append_char(&mut self, c: char) {
        self.input_buffer.push(c);
    }

    /// Supprime le dernier caractère du buffer
    pub fn backspace(&mut self) {
        self.input_buffer.pop();
    }

    /// Vérifie si on est en mode input
    pub fn is_in_input_mode(&self) -> bool {
        self.current_screen == Screen::InputMode
    }

    // ========================================================================
    // Delete Confirmation Management
    // ========================================================================

    /// Demande la confirmation de suppression
    ///
    /// CONCEPT : Two-step delete pattern
    /// - Appelé lors de la première pression de 'd'
    /// - Active l'état confirm_delete pour attendre une seconde pression
    /// - Évite les suppressions accidentelles
    pub fn request_delete(&mut self) {
        self.confirm_delete = true;
    }

    /// Annule la demande de suppression
    pub fn cancel_delete(&mut self) {
        self.confirm_delete = false;
    }

    /// Vérifie si on attend la confirmation de suppression
    pub fn is_awaiting_delete_confirmation(&self) -> bool {
        self.confirm_delete
    }

    /// Supprime l'item sélectionné de la watchlist
    ///
    /// CONCEPT : Safe deletion
    /// - Supprime l'item à selected_index
    /// - Ajuste selected_index si nécessaire
    /// - Reset confirm_delete
    pub fn delete_selected(&mut self) {
        if self.selected_index < self.watchlist.len() {
            self.watchlist.remove(self.selected_index);

            // Ajuste l'index si on a supprimé le dernier élément
            if self.selected_index >= self.watchlist.len() && self.selected_index > 0 {
                self.selected_index -= 1;
            }
        }

        self.confirm_delete = false;
    }
}

// ============================================================================
// Trait Default
// ============================================================================
// CONCEPT RUST : Traits
// - Un trait est comme une interface en Java ou un protocol en Swift
// - Default est un trait standard qui fournit une valeur par défaut
// - Permet d'utiliser App::default() au lieu de App::new()
//
// Convention Rust : si new() ne prend pas de paramètres, implémenter Default
// ============================================================================

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests unitaires
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert!(app.is_running());
        assert!(app.watchlist.is_empty());
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_app_with_watchlist() {
        let items = vec![
            WatchlistItem::new("AAPL".to_string(), "Apple Inc.".to_string()),
            WatchlistItem::new("TSLA".to_string(), "Tesla".to_string()),
        ];

        let app = App::with_watchlist(items);
        assert_eq!(app.watchlist.len(), 2);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_app_quit() {
        let mut app = App::new();
        assert!(app.is_running());

        app.quit();
        assert!(!app.is_running());
    }

    #[test]
    fn test_navigation() {
        let items = vec![
            WatchlistItem::new("AAPL".to_string(), "Apple Inc.".to_string()),
            WatchlistItem::new("TSLA".to_string(), "Tesla".to_string()),
            WatchlistItem::new("BTC-USD".to_string(), "Bitcoin".to_string()),
        ];

        let mut app = App::with_watchlist(items);

        // Au début, on est à l'index 0
        assert_eq!(app.selected_index, 0);

        // Navigate down
        app.navigate_down();
        assert_eq!(app.selected_index, 1);

        app.navigate_down();
        assert_eq!(app.selected_index, 2);

        // Navigate down au max : reste à 2
        app.navigate_down();
        assert_eq!(app.selected_index, 2);

        // Navigate up
        app.navigate_up();
        assert_eq!(app.selected_index, 1);

        app.navigate_up();
        assert_eq!(app.selected_index, 0);

        // Navigate up au min : reste à 0
        app.navigate_up();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_selected_item() {
        let items = vec![
            WatchlistItem::new("AAPL".to_string(), "Apple Inc.".to_string()),
            WatchlistItem::new("TSLA".to_string(), "Tesla".to_string()),
        ];

        let app = App::with_watchlist(items);

        let selected = app.selected_item().unwrap();
        assert_eq!(selected.symbol, "AAPL");
    }
}
