// ============================================================================
// Chart - Rendu du graphique pour un ticker
// ============================================================================
// Affiche un graphique ligne (line chart) pour le ticker sélectionné
//
// CONCEPTS RUST :
// 1. Option handling : gérer l'absence de données
// 2. Iterator chaining : transformer les données OHLC en points (x, y)
// 3. Closures : pour les labels des axes
//
// CONCEPTS RATATUI :
// 1. Chart widget : graphique ligne
// 2. Dataset : série de données à afficher
// 3. Axis : configuration des axes X et Y
// ============================================================================

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph},
    Frame,
};

use crate::app::App;

// ============================================================================
// Fonction principale de rendu du graphique
// ============================================================================

/// Dessine le graphique pour le ticker sélectionné
///
/// CONCEPT RUST : Early return avec ?
/// - Si pas de ticker sélectionné, affiche un message et return
/// - Si pas de données, affiche un message et return
pub fn render_chart(frame: &mut Frame, app: &App, area: Rect) {
    // Récupère le ticker sélectionné
    // CONCEPT RUST : Option et if let
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

    // Crée le layout : titre + graphique
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Titre
            Constraint::Min(0),      // Graphique
        ])
        .split(area)
        .to_vec();

    // Dessine le titre
    render_chart_header(frame, item, chunks[0]);

    // Dessine le graphique
    render_chart_graph(frame, item, data, chunks[1]);
}

// ============================================================================
// Header du graphique
// ============================================================================

/// Dessine le header avec infos du ticker
fn render_chart_header(frame: &mut Frame, item: &crate::models::WatchlistItem, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" 📈 {} - {} ", item.symbol, item.name));

    // Affiche prix et variation
    let text = if let (Some(price), Some(change)) = (item.current_price(), item.change_percent()) {
        let color = if change >= 0.0 {
            Color::Green
        } else {
            Color::Red
        };

        let arrow = if change >= 0.0 { "▲" } else { "▼" };

        vec![Line::from(vec![
            Span::raw("Prix: "),
            Span::styled(
                format!("${:.2}", price),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{} {:+.2}%", arrow, change),
                Style::default().fg(color),
            ),
            Span::raw("  "),
            Span::styled(
                "[ESC]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Retour"),
        ])]
    } else {
        vec![Line::from("Chargement...")]
    };

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

// ============================================================================
// Graphique principal
// ============================================================================

/// Dessine le graphique ligne
///
/// CONCEPT RUST : Iterator chaining complexe
/// - .iter() : itère sur les chandelles
/// - .enumerate() : ajoute l'index
/// - .map() : transforme en points (x, y)
/// - .collect() : collecte en Vec
fn render_chart_graph(
    frame: &mut Frame,
    item: &crate::models::WatchlistItem,
    data: &crate::models::OHLCData,
    area: Rect,
) {
    // Convertit les données OHLC en points (x, y)
    let points: Vec<(f64, f64)> = data
        .candles
        .iter()
        .enumerate()
        .map(|(i, candle)| (i as f64, candle.close))
        .collect();

    // Si pas de points, affiche un message
    if points.is_empty() {
        render_no_data(frame, area, "Pas de données à afficher");
        return;
    }

    // Calcule les bornes pour les axes
    let (min_price, max_price) = points.iter().fold(
        (f64::MAX, f64::MIN),
        |(min, max), &(_x, y)| (min.min(y), max.max(y)),
    );

    // Ajoute une marge de 5% pour que le graphique respire
    let margin = (max_price - min_price) * 0.05;
    let y_min = (min_price - margin).max(0.0);  // Ne descend pas en dessous de 0
    let y_max = max_price + margin;

    // Crée le dataset (série de données)
    // CONCEPT RATATUI : Dataset
    // - name() : nom de la série
    // - marker() : type de marqueur (Dot, Braille, etc.)
    // - graph_type() : Line ou Bar
    // - style() : couleur et style
    // - data() : les points (x, y)
    let color = if item.is_positive() {
        Color::Green
    } else {
        Color::Red
    };

    // CONCEPT RATATUI : Marker types
    // - Dot : points simples connectés
    // - Block : blocs pleins (ligne plus visible)
    // - Braille : points Braille (pointillé)
    // - Bar : barres verticales
    let datasets = vec![Dataset::default()
        .name(item.symbol.as_str())
        .marker(symbols::Marker::Dot)  // Ligne continue avec points connectés
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(&points)];

    // Crée les axes
    // CONCEPT RATATUI : Axis
    // - title() : titre de l'axe
    // - bounds() : min et max
    // - labels() : labels affichés
    let x_axis = Axis::default()
        .title("Jours")
        .style(Style::default().fg(Color::Gray))
        .bounds([0.0, (points.len() - 1) as f64])
        .labels(vec![
            Span::raw(""),
            Span::raw(format!("{} jours", data.timeframe.to_days())),
            Span::raw(""),
        ]);

    let y_axis = Axis::default()
        .title("Prix ($)")
        .style(Style::default().fg(Color::Gray))
        .bounds([y_min, y_max])
        .labels(vec![
            Span::raw(format!("${:.0}", y_min)),
            Span::raw(format!("${:.0}", (y_min + y_max) / 2.0)),
            Span::raw(format!("${:.0}", y_max)),
        ]);

    // Crée le widget Chart
    // CONCEPT RATATUI : Chart widget
    // - block() : bordures et titre
    // - x_axis() / y_axis() : configuration des axes
    // - datasets() : les séries de données à afficher
    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
                .title(format!(" {} - {} jours ", item.symbol, data.timeframe.to_days())),
        )
        .x_axis(x_axis)
        .y_axis(y_axis);

    frame.render_widget(chart, area);
}

// ============================================================================
// Helper : Message quand pas de données
// ============================================================================

/// Affiche un message quand il n'y a pas de données à afficher
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

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

// ============================================================================
// Notes pédagogiques
// ============================================================================
//
// CONCEPTS RATATUI APPRIS :
//
// 1. Chart widget
//    - Dataset : série de données (points x, y)
//    - GraphType : Line ou Bar
//    - Marker : type de point (Dot, Braille, Block)
//
// 2. Axes
//    - Axis : configuration d'un axe
//    - bounds() : min et max
//    - labels() : labels affichés
//
// 3. Styling conditionnel
//    - Couleur selon is_positive()
//    - Style dynamique
//
// CONCEPTS RUST APPRIS :
//
// 1. Iterator avec fold
//    - fold() pour calculer min/max en un seul passage
//    - Plus efficace que deux passes
//
// 2. Early return avec match
//    - Gestion élégante des Option
//    - Évite l'imbrication excessive
//
// ============================================================================
