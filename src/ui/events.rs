// ============================================================================
// Gestion des événements clavier
// ============================================================================
// CONCEPTS RUST :
// 1. Pattern matching sur KeyCode avec guards (`if on_chart`)
// 2. Fonctions pures : handle_key ne fait ni I/O ni réseau, elle retourne
//    les commandes à envoyer — c'est ce qui la rend testable
// ============================================================================

use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};

use crate::app::{App, InputKind, Screen};
use crate::models::Interval;
use crate::worker::AppCommand;

/// Lecteur d'événements clavier
pub struct EventHandler;

impl EventHandler {
    /// Attend une touche au plus 250 ms
    ///
    /// None si rien n'arrive, si c'est un relâchement de touche (certains OS
    /// envoient Press ET Release) ou un autre événement (resize : le prochain
    /// rendu s'adapte de lui-même).
    ///
    /// # Errors
    /// Si le terminal ne peut pas être lu.
    pub fn next(&self) -> Result<Option<KeyEvent>> {
        if !event::poll(Duration::from_millis(250))? {
            return Ok(None);
        }
        match event::read()? {
            CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => Ok(Some(key)),
            _ => Ok(None),
        }
    }
}

/// Traduit une touche en changement d'état + commandes réseau
///
/// CONCEPT : Routage par écran d'abord. En saisie, toutes les lettres vont au
/// buffer — c'est ce qui empêche `q` de quitter pendant qu'on tape "QQQ".
pub fn handle_key(app: &mut App, key: KeyEvent, now: Instant) -> Vec<AppCommand> {
    // Ctrl+C quitte partout : en raw mode le terminal ne l'envoie plus en signal
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.quit();
        return Vec::new();
    }
    if app.current_screen == Screen::InputMode {
        return handle_input_key(app, key, now);
    }

    // Toute touche annule une confirmation en cours ; on retient laquelle était active
    // CONCEPT RUST : std::mem::take remplace la valeur par Default (false) et rend l'ancienne
    let confirming_quit = std::mem::take(&mut app.confirm_quit);
    let confirming_delete = std::mem::take(&mut app.confirm_delete);
    let on_chart = app.current_screen == Screen::ChartView;

    match key.code {
        KeyCode::Char('q' | 'Q') if confirming_quit => app.quit(),
        KeyCode::Char('q' | 'Q') => app.confirm_quit = true,
        KeyCode::Char('r' | 'R') => return app.refresh_commands(now),

        // Vue graphique
        KeyCode::Esc | KeyCode::Char(' ') if on_chart => app.show_dashboard(),
        KeyCode::Char('l' | 'L') if on_chart => {
            return app.change_interval(Interval::next).into_iter().collect()
        }
        KeyCode::Char('h' | 'H') if on_chart => {
            return app
                .change_interval(Interval::previous)
                .into_iter()
                .collect()
        }
        _ if on_chart => {}

        // Dashboard
        KeyCode::Up | KeyCode::Char('k' | 'K') => app.navigate_up(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => app.navigate_down(),
        KeyCode::Enter => return app.open_chart().into_iter().collect(),
        KeyCode::Char('a' | 'A') => app.start_input(),
        KeyCode::Char('p' | 'P') => app.start_position_input(),
        KeyCode::Char('d' | 'D') if confirming_delete => app.delete_selected(now),
        KeyCode::Char('d' | 'D') if app.selected_item().is_some() => app.confirm_delete = true,
        _ => {}
    }
    Vec::new()
}

/// Touches du mode saisie : tout caractère de symbole va dans le buffer
fn handle_input_key(app: &mut App, key: KeyEvent, now: Instant) -> Vec<AppCommand> {
    match key.code {
        KeyCode::Esc => app.cancel_input(),
        KeyCode::Enter => {
            let kind = app.input_kind.clone();
            let text = app.take_input();
            match kind {
                InputKind::AddTicker => return app.request_add(&text, now).into_iter().collect(),
                InputKind::Position(symbol) => app.set_position(&symbol, &text, now),
            }
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Char(c)
            if accepts(&app.input_kind, c)
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            // La virgule décimale est lue comme un point
            let c = if c == ',' {
                '.'
            } else {
                c.to_ascii_uppercase()
            };
            app.input_buffer.push(c);
        }
        _ => {}
    }
    Vec::new()
}

/// Caractères acceptés selon la saisie en cours
fn accepts(kind: &InputKind, c: char) -> bool {
    match kind {
        InputKind::AddTicker => is_ticker_char(c),
        InputKind::Position(_) => c.is_ascii_digit() || matches!(c, '.' | ',' | ' '),
    }
}

/// Caractères des symboles Yahoo : AAPL, BTC-USD, BRK.B, EURUSD=X, ^GSPC
fn is_ticker_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '=' | '^')
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn code(k: KeyCode) -> KeyEvent {
        KeyEvent::new(k, KeyModifiers::NONE)
    }

    fn app() -> App {
        App::new(vec!["AAPL".into(), "TSLA".into()], None, Instant::now())
    }

    fn press(app: &mut App, k: KeyEvent) -> Vec<AppCommand> {
        handle_key(app, k, Instant::now())
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, key(c));
        }
    }

    #[test]
    fn typing_qqq_adds_instead_of_quitting() {
        let mut app = app();
        press(&mut app, key('a'));
        type_str(&mut app, "qqq");
        let commands = press(&mut app, code(KeyCode::Enter));
        assert!(app.is_running());
        assert_eq!(
            commands,
            [AppCommand::Add {
                symbol: "QQQ".into()
            }]
        );
    }

    #[test]
    fn forex_and_index_symbols_can_be_typed() {
        let mut app = app();
        press(&mut app, key('a'));
        type_str(&mut app, "eurusd=x");
        assert_eq!(app.input_buffer, "EURUSD=X");
        app.input_buffer.clear();
        type_str(&mut app, "^gspc");
        assert_eq!(app.input_buffer, "^GSPC");
    }

    #[test]
    fn any_other_key_cancels_pending_delete() {
        let mut app = app();
        press(&mut app, key('d'));
        press(&mut app, key('a')); // ouvre la saisie : annule la confirmation
        press(&mut app, code(KeyCode::Esc));
        press(&mut app, key('d'));
        assert_eq!(app.watchlist.len(), 2, "un seul d ne doit pas supprimer");
        press(&mut app, key('d'));
        assert_eq!(app.watchlist.len(), 1);
    }

    #[test]
    fn quit_needs_two_presses_and_ctrl_c_is_immediate() {
        let mut app = app();
        press(&mut app, key('q'));
        assert!(app.is_running());
        press(&mut app, key('q'));
        assert!(!app.is_running());

        let mut other = self::app();
        press(
            &mut other,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(!other.is_running());
    }

    #[test]
    fn interval_keys_only_on_chart() {
        let mut app = app();
        assert!(press(&mut app, key('l')).is_empty());
        press(&mut app, code(KeyCode::Enter)); // ouvre le graphique (+ Load)
        let commands = press(&mut app, key('l'));
        assert_eq!(
            commands,
            [AppCommand::Load {
                symbol: "AAPL".into(),
                interval: Interval::H1
            }]
        );
    }

    #[test]
    fn refresh_key_reloads_everything() {
        let mut app = app();
        assert_eq!(press(&mut app, key('r')).len(), 2);
    }

    #[test]
    fn p_edits_the_position_of_the_selected_ticker() {
        let mut app = app();
        press(&mut app, key('p'));
        assert_eq!(app.input_buffer, "", "pas de position : saisie vide");
        type_str(&mut app, "10 150,25");
        press(&mut app, code(KeyCode::Enter));
        let position = app.watchlist[0].position.unwrap();
        assert_eq!((position.quantity, position.unit_cost), (10.0, 150.25));

        // Rouvrir pré-remplit ; vider puis valider retire la position
        press(&mut app, key('p'));
        assert_eq!(app.input_buffer, "10 150.25");
        for _ in 0..app.input_buffer.len() {
            press(&mut app, code(KeyCode::Backspace));
        }
        press(&mut app, code(KeyCode::Enter));
        assert!(app.watchlist[0].position.is_none());
    }

    #[test]
    fn invalid_position_is_rejected_with_an_error() {
        let mut app = app();
        press(&mut app, key('p'));
        type_str(&mut app, "10");
        press(&mut app, code(KeyCode::Enter));
        assert!(app.watchlist[0].position.is_none());
        assert!(app.status.as_ref().is_some_and(|s| s.is_error));
        // Les lettres ne s'écrivent pas dans une saisie de position
        press(&mut app, key('p'));
        type_str(&mut app, "q1x");
        assert_eq!(app.input_buffer, "1");
        assert!(app.is_running());
    }
}
