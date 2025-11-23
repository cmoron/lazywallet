// ============================================================================
// Gestion des événements
// ============================================================================
// Gère les événements clavier et les ticks de l'application
//
// CONCEPTS RUST :
// 1. Enums avec variants : représenter différents types d'événements
// 2. Channels (mpsc) : communication entre threads
// 3. Threading : exécuter la lecture d'événements dans un thread séparé
// 4. Error handling avec Result
// ============================================================================

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind};

// ============================================================================
// Enum Event
// ============================================================================
// CONCEPT RUST : Enums avec données
// - Chaque variant peut contenir des données différentes
// - Key(KeyEvent) : stocke l'événement clavier complet
// - Tick : variant sans données (unit variant)
//
// C'est plus puissant que les enums en C/Java !
// ============================================================================

/// Événements de l'application
#[derive(Debug, Clone)]
pub enum Event {
    /// Touche pressée
    Key(KeyEvent),

    /// Tick régulier (pour animations, rafraîchissement)
    Tick,

    /// Erreur survenue
    Error,
}

// ============================================================================
// Structure EventHandler
// ============================================================================
// CONCEPT : Singleton pattern pour gérer les événements
// - Un seul handler pour toute l'application
// - Pas besoin de stocker d'état (stateless)
// ============================================================================

/// Gestionnaire d'événements
pub struct EventHandler;

impl EventHandler {
    /// Crée un nouveau gestionnaire d'événements
    pub fn new() -> Self {
        Self
    }

    /// Lit le prochain événement (bloquant avec timeout)
    ///
    /// CONCEPT RUST : Result et ?
    /// - poll() peut échouer (I/O error)
    /// - read() peut échouer
    /// - ? propage automatiquement les erreurs
    ///
    /// CONCEPT : Non-blocking I/O avec timeout
    /// - poll(timeout) attend max 250ms
    /// - Si pas d'événement, retourne Ok(Event::Tick)
    /// - Si événement, le lit et le convertit
    pub fn next(&self) -> Result<Event> {
        // Poll avec timeout de 250ms
        // CONCEPT RUST : if expression
        // - if retourne une valeur en Rust (comme un ternaire ?)
        if event::poll(Duration::from_millis(250))? {
            // Il y a un événement, on le lit
            match event::read()? {
                // Événement clavier
                CrosstermEvent::Key(key) => {
                    // CONCEPT : Filter sur KeyEventKind
                    // Sur certains OS, on reçoit Press ET Release
                    // On ne veut gérer que Press pour éviter les doublons
                    if key.kind == KeyEventKind::Press {
                        Ok(Event::Key(key))
                    } else {
                        // Ignore Release, retourne Tick
                        Ok(Event::Tick)
                    }
                }

                // Autres événements (resize, mouse, etc.) ignorés pour l'instant
                _ => Ok(Event::Tick),
            }
        } else {
            // Timeout : pas d'événement, retourne Tick
            Ok(Event::Tick)
        }
    }
}

// ============================================================================
// Helper : Convertir KeyEvent en action
// ============================================================================
// CONCEPT RUST : Pattern matching avancé
// - Match sur KeyCode pour identifier la touche
// - Peut aussi matcher sur les modifiers (Ctrl, Alt, Shift)
// ============================================================================

/// Vérifie si l'événement est la touche 'q' (quitter)
pub fn is_quit_event(event: &Event) -> bool {
    // CONCEPT RUST : Pattern matching avec if let
    // - Destructure Event::Key et vérifie le KeyCode en une ligne
    // - Plus élégant que match pour un seul cas
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
    } else {
        false
    }
}

/// Vérifie si l'événement est Échap
pub fn is_escape_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Esc)
    } else {
        false
    }
}

/// Vérifie si l'événement est Espace
pub fn is_space_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Char(' '))
    } else {
        false
    }
}

/// Vérifie si l'événement est Entrée
pub fn is_enter_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Enter)
    } else {
        false
    }
}

/// Vérifie si l'événement est la flèche vers le haut ou 'k' (vim)
///
/// CONCEPT RUST : Multiple patterns avec |
/// - KeyCode::Up | KeyCode::Char('k') : match l'un ou l'autre
/// - Support des touches Vim pour les power users !
pub fn is_up_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K'))
    } else {
        false
    }
}

/// Vérifie si l'événement est la flèche vers le bas ou 'j' (vim)
pub fn is_down_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J'))
    } else {
        false
    }
}

/// Vérifie si l'événement est 'l' (intervalle suivant)
pub fn is_next_interval_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Char('l'))
    } else {
        false
    }
}

/// Vérifie si l'événement est 'h' (intervalle précédent)
pub fn is_previous_interval_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Char('h'))
    } else {
        false
    }
}

/// Vérifie si l'événement est 'a' (add ticker)
///
/// CONCEPT : Vim-style 'a' for append
/// - Ouvre le mode input pour ajouter un ticker
pub fn is_add_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Char('a') | KeyCode::Char('A'))
    } else {
        false
    }
}

/// Vérifie si l'événement est 'd' (delete ticker)
///
/// CONCEPT : Vim-style 'd' for delete
/// - Demande confirmation avant suppression
pub fn is_delete_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D'))
    } else {
        false
    }
}

/// Vérifie si l'événement est Backspace
pub fn is_backspace_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Backspace)
    } else {
        false
    }
}

/// Vérifie si l'événement est un caractère alphanumérique ou tiret (pour saisie ticker)
pub fn is_ticker_char_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(key.code, KeyCode::Char(c) if c.is_alphanumeric() || c == '-' || c == '.')
    } else {
        false
    }
}

/// Extrait le caractère d'un événement clavier si c'est un caractère
pub fn get_char_from_event(event: &Event) -> Option<char> {
    if let Event::Key(key) = event {
        if let KeyCode::Char(c) = key.code {
            return Some(c);
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_quit_event() {
        let quit_event = Event::Key(KeyEvent::new(KeyCode::Char('q'), event::KeyModifiers::empty()));
        assert!(is_quit_event(&quit_event));

        let other_event = Event::Key(KeyEvent::new(KeyCode::Char('a'), event::KeyModifiers::empty()));
        assert!(!is_quit_event(&other_event));

        assert!(!is_quit_event(&Event::Tick));
    }
}
