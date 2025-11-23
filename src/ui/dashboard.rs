// ============================================================================
// Dashboard - Rendu de l'interface principale
// ============================================================================
// Dessine l'interface TUI en utilisant les widgets de ratatui
//
// CONCEPTS RUST :
// 1. Lifetimes : 'a pour gérer la durée de vie des références
// 2. Traits : Frame implémente des traits pour le rendering
// 3. Builder pattern : construction fluide des widgets
//
// CONCEPTS RATATUI :
// 1. Frame : surface de dessin
// 2. Widgets : composants UI (Block, Paragraph, etc.)
// 3. Layout : découpage de l'espace en zones
// 4. Style : couleurs et attributs de texte
// ============================================================================

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Screen};
use crate::ui::candlestick_text;

// ============================================================================
// Fonction principale de rendu
// ============================================================================
// CONCEPT RUST : Lifetime 'a
// - Frame a un lifetime 'a
// - Les références dans Frame ne doivent pas outlive 'a
// - Le compilateur vérifie que tout est safe
//
// CONCEPT RUST : &mut Frame
// - On passe Frame par référence mutable (on va dessiner dedans)
// - &App : on lit l'état, pas de modification
// ============================================================================

/// Dessine l'interface complète
///
/// CONCEPT RUST : Routing avec match sur enum
/// - Pattern matching sur app.current_screen
/// - Affiche Dashboard OU ChartView selon l'état
/// - Le compilateur garantit l'exhaustivité (tous les cas gérés)
///
/// # Arguments
/// * `frame` - Surface de dessin ratatui
/// * `app` - État de l'application
pub fn render(frame: &mut Frame, app: &App) {
    // CONCEPT RUST : Match sur enum pour router
    // - Pattern "State Machine"
    // - Le compilateur force à gérer tous les variants
    match app.current_screen {
        Screen::Dashboard => {
            // Affiche la watchlist
            render_dashboard(frame, app);
        }
        Screen::ChartView => {
            // Affiche le graphique en chandeliers japonais (Unicode text)
            candlestick_text::render_candlestick_chart(frame, app, frame.size());
        }
        Screen::InputMode => {
            // Affiche le dashboard avec l'input mode en bas
            render_input_mode(frame, app);
        }
    }
}

/// Dessine le dashboard (watchlist)
fn render_dashboard(frame: &mut Frame, app: &App) {
    let size = frame.size();
    let chunks = create_layout(size);

    // Dessine le header (titre)
    render_header(frame, chunks[0]);

    // Dessine le contenu principal (watchlist)
    render_main_content(frame, app, chunks[1]);

    // Dessine le footer (instructions)
    render_footer(frame, app, chunks[2]);
}

// ============================================================================
// Layout : Découpage de l'écran
// ============================================================================
// CONCEPT RATATUI : Layout
// - split() découpe un Rect en plusieurs zones
// - Constraints définissent les tailles :
//   - Length(n) : exactement n lignes/colonnes
//   - Percentage(n) : n% de l'espace
//   - Min(n) : minimum n
//   - Max(n) : maximum n
// ============================================================================

/// Crée le layout principal (header, content, footer)
///
/// CONCEPT RUST : Rc<[T]> vs Vec<T>
/// - Layout::split() retourne Rc<[Rect]> (reference counted slice)
/// - Rc permet le partage sans copie (efficient)
/// - On le convertit en Vec avec .to_vec() pour simplifier
fn create_layout(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Header : 3 lignes
            Constraint::Min(0),          // Content : tout le reste
            Constraint::Length(3),       // Footer : 3 lignes
        ])
        .split(area)
        .to_vec()  // Convertit Rc<[Rect]> en Vec<Rect>
}

// ============================================================================
// Header : Titre de l'application
// ============================================================================
// CONCEPT RATATUI : Widgets
// - Block : bordures et titre
// - Paragraph : texte formaté
// - Style : couleurs et attributs
// ============================================================================

/// Dessine le header avec le titre
fn render_header(frame: &mut Frame, area: Rect) {
    // Crée un Block avec bordures
    // CONCEPT : Builder pattern
    // - Chaque méthode retourne self
    // - Permet de chaîner les appels
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" LazyWallet ")
        .title_alignment(Alignment::Center);

    // Texte du header
    // CONCEPT RATATUI : Span et Line
    // - Span : morceau de texte avec style
    // - Line : une ligne composée de Spans
    // - Vec<Line> : paragraphe multi-lignes
    let text = vec![
        Line::from(Span::styled(
            "🚀 Terminal User Interface Mode",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    // CONCEPT RUST : Rendering
    // - frame.render_widget() prend ownership du widget
    // - Le widget est "consumed" (moved)
    frame.render_widget(paragraph, area);
}

// ============================================================================
// Main Content : Contenu principal
// ============================================================================

/// Dessine le contenu principal : la watchlist
///
/// CONCEPT RATATUI : List widget
/// - Widget pour afficher une liste d'items
/// - Highlight : style spécial pour l'item sélectionné
/// - ListItem : chaque ligne de la liste
fn render_main_content(frame: &mut Frame, app: &App, area: Rect) {
    // Block principal
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" 📊 Watchlist ");

    // Si la watchlist est vide, affiche un message
    if app.watchlist.is_empty() {
        let text = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Watchlist vide",
                Style::default().fg(Color::Gray),
            )),
        ];

        let paragraph = Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
        return;
    }

    // Crée les items de la liste
    // CONCEPT RUST : Iterator chaining
    // - .iter() : itère sur les WatchlistItem
    // - .enumerate() : ajoute l'index
    // - .map() : transforme chaque item en ListItem
    // - .collect() : collecte dans un Vec<ListItem>
    let items: Vec<ListItem> = app
        .watchlist
        .iter()
        .enumerate()
        .map(|(index, item)| {
            // Détermine le style selon la variation
            let style = if item.has_data() {
                if item.is_positive() {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                }
            } else {
                Style::default().fg(Color::Gray)
            };

            // Formate la ligne pour cet item
            let line = if item.has_data() {
                // Données chargées : affiche prix et variation
                let price_str = item
                    .current_price()
                    .map(|p| format!("${:.2}", p))
                    .unwrap_or_else(|| "N/A".to_string());

                let change_str = item
                    .change_percent()
                    .map(|c| {
                        let arrow = if c >= 0.0 { "▲" } else { "▼" };
                        format!("{} {:+.2}%", arrow, c)
                    })
                    .unwrap_or_else(|| String::new());

                format!(
                    " {:<8} {:<20} {:>12}  {}",
                    item.symbol, item.name, price_str, change_str
                )
            } else {
                // Pas de données : affiche "Loading..."
                format!(" {:<8} {:<20} {:>12}", item.symbol, item.name, "Loading...")
            };

            // Crée un ListItem avec style
            let mut list_item = ListItem::new(line).style(style);

            // Si c'est l'item sélectionné, ajoute un indicateur
            if index == app.selected_index {
                list_item = list_item.style(
                    style
                        .add_modifier(Modifier::BOLD)
                        .add_modifier(Modifier::REVERSED),  // Inverse les couleurs
                );
            }

            list_item
        })
        .collect();

    // Crée le widget List
    let list = List::new(items).block(block);

    frame.render_widget(list, area);
}

// ============================================================================
// Footer : Instructions
// ============================================================================

/// Dessine le footer avec les raccourcis clavier
fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    // CONCEPT : Confirmation de quit two-step
    // - Si app.is_awaiting_quit_confirmation(), affiche message d'avertissement
    // - Sinon, affiche les raccourcis normaux

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let shortcuts = if app.is_awaiting_delete_confirmation() {
        // Message de confirmation de suppression
        // CONCEPT : Style avec BLINK pour attirer l'attention
        let ticker_name = app.watchlist.get(app.selected_index)
            .map(|item| item.symbol.as_str())
            .unwrap_or("?");

        Line::from(vec![
            Span::styled(
                "⚠  Appuyez sur ",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[d]",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::styled(
                format!(" à nouveau pour supprimer {} ou autre touche pour annuler ⚠", ticker_name),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ])
    } else if app.is_awaiting_quit_confirmation() {
        // Message de confirmation de quit
        // CONCEPT : Style avec BLINK pour attirer l'attention
        Line::from(vec![
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
        ])
    } else {
        // Shortcuts normaux avec différentes couleurs
        // CONCEPT RATATUI : Spans multiples dans une Line
        // - Permet d'avoir plusieurs couleurs sur une même ligne
        Line::from(vec![
            Span::styled("[q]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Quit  "),
            Span::styled("[↑↓ / j k]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Navigate  "),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" Chart  "),
            Span::styled("[a]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" Add  "),
            Span::styled("[d]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" Delete"),
        ])
    };

    let paragraph = Paragraph::new(vec![shortcuts])
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

// ============================================================================
// Input Mode : Saisie de ticker
// ============================================================================

/// Dessine le dashboard avec le mode input actif
///
/// CONCEPT : Modal input (Vim-like)
/// - Affiche la watchlist en arrière-plan
/// - Affiche une ligne d'input en bas pour saisir le ticker
/// - ESC annule, Enter valide
fn render_input_mode(frame: &mut Frame, app: &App) {
    let size = frame.size();
    let chunks = create_layout(size);

    // Dessine le header
    render_header(frame, chunks[0]);

    // Dessine la watchlist (en arrière-plan)
    render_main_content(frame, app, chunks[1]);

    // Footer : affiche l'input line au lieu des shortcuts
    render_input_footer(frame, app, chunks[2]);
}

/// Dessine le footer en mode input avec la ligne de saisie
fn render_input_footer(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green)); // Vert pour indiquer mode input

    // Construit la ligne d'input avec le prompt et le buffer
    let input_line = Line::from(vec![
        Span::styled(
            &app.input_prompt,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &app.input_buffer,
            Style::default().fg(Color::White),
        ),
        Span::styled(
            "█", // Curseur
            Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);

    let help_line = Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Confirm  "),
        Span::styled(
            "[ESC]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);

    let paragraph = Paragraph::new(vec![input_line, help_line])
        .block(block)
        .alignment(Alignment::Left); // Alignement à gauche pour l'input

    frame.render_widget(paragraph, area);
}

// ============================================================================
// Notes pédagogiques
// ============================================================================
//
// CONCEPTS RATATUI APPRIS :
//
// 1. Frame et rendering
//    - Frame : surface de dessin
//    - render_widget() : dessine un widget dans une zone
//
// 2. Layout
//    - Direction : Vertical ou Horizontal
//    - Constraints : définir les tailles
//    - split() : découper en zones
//
// 3. Widgets de base
//    - Block : bordures et titre
//    - Paragraph : texte formaté
//    - Line et Span : composition de texte
//
// 4. Styles
//    - Color : couleurs (RGB, Named, Indexed)
//    - Modifier : Bold, Italic, etc.
//    - Builder pattern : .fg().add_modifier()
//
// PROCHAINES ÉTAPES :
// - Widgets List pour la watchlist
// - Widgets Chart pour les graphiques
// - State pour gérer la sélection
// - Scrolling et navigation
//
// ============================================================================
