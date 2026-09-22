// ============================================================================
// Dashboard - Rendu de l'interface principale
// ============================================================================
// Dessine la watchlist, la barre d'état et la ligne de saisie
//
// CONCEPTS RATATUI :
// 1. Frame : surface de dessin
// 2. Widgets : composants UI (Block, Paragraph, List)
// 3. Layout : découpage de l'espace en zones
// 4. Line et Span : texte multi-styles sur une ligne
// ============================================================================

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Screen, REFRESH_EVERY};
use crate::ui::candlestick_text;

/// Dessine l'écran courant
///
/// CONCEPT RUST : Match sur enum pour router (state machine)
pub fn render(frame: &mut Frame, app: &App) {
    match app.current_screen {
        Screen::ChartView => candlestick_text::render_candlestick_chart(frame, app, frame.size()),
        Screen::Dashboard | Screen::InputMode => render_dashboard(frame, app),
    }
}

/// Dashboard : header, watchlist, footer (raccourcis ou saisie)
fn render_dashboard(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Watchlist
            Constraint::Length(3), // Footer
        ])
        .split(frame.size());

    render_header(frame, app, chunks[0]);
    render_watchlist(frame, app, chunks[1]);
    if app.current_screen == Screen::InputMode {
        render_input_footer(frame, app, chunks[2]);
    } else {
        render_footer(frame, app, chunks[2]);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" LazyWallet ")
        .title_alignment(Alignment::Center);
    let text = format!(
        "{} tickers · rafraîchi toutes les {} s",
        app.watchlist.len(),
        REFRESH_EVERY.as_secs()
    );
    let paragraph = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default().fg(Color::Gray),
    )))
    .block(block)
    .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Tronque un texte à une longueur maximale avec ellipse
///
/// CONCEPT RUST : .chars() compte les caractères Unicode, pas les bytes
///
/// ```text
/// truncate_with_ellipsis("Microsoft Corporation", 20) // "Microsoft Corporatio…"
/// truncate_with_ellipsis("Apple Inc.", 20)            // "Apple Inc."
/// ```
fn truncate_with_ellipsis(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

/// Watchlist : une ligne par ticker, verte ou rouge selon la variation du jour
fn render_watchlist(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Watchlist ");

    if app.watchlist.is_empty() {
        let paragraph = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Watchlist vide — [a] pour ajouter un ticker",
                Style::default().fg(Color::Gray),
            )),
        ])
        .block(block)
        .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .watchlist
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let name = truncate_with_ellipsis(&item.name, 20);
            let (price, change, color) = match (item.current_price(), item.change_percent()) {
                (Some(price), change) => {
                    let change = change.map_or_else(String::new, |c| {
                        let arrow = if c >= 0.0 { "▲" } else { "▼" };
                        format!("{arrow} {c:+.2}%")
                    });
                    let color = if item.is_positive() {
                        Color::Green
                    } else {
                        Color::Red
                    };
                    (item.format_price(price), change, color)
                }
                // Pas encore de prix : en cours de chargement, ou échec (voir barre d'état)
                (None, _) => {
                    let placeholder = if app.is_loading() { "…" } else { "N/A" };
                    (placeholder.to_string(), String::new(), Color::Gray)
                }
            };
            let line = format!(
                " {:<10} {:<20} {:>16}  {}",
                item.symbol, name, price, change
            );

            let mut style = Style::default().fg(color);
            if index == app.selected_index {
                style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
            }
            ListItem::new(line).style(style)
        })
        .collect();

    frame.render_widget(List::new(items).block(block), area);
}

/// Ligne d'état commune au dashboard et au graphique
///
/// Par priorité : confirmation en attente, message, chargement. None si rien à dire.
pub(crate) fn status_line(app: &App) -> Option<Line<'static>> {
    let warning = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let key = Style::default()
        .fg(Color::Red)
        .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);

    if app.confirm_delete {
        let symbol = app.selected_item().map_or("?", |item| item.symbol.as_str());
        return Some(Line::from(vec![
            Span::styled("⚠  Appuyez sur ", warning),
            Span::styled("[d]", key),
            Span::styled(
                format!(" à nouveau pour supprimer {symbol}, autre touche pour annuler"),
                warning,
            ),
        ]));
    }
    if app.confirm_quit {
        return Some(Line::from(vec![
            Span::styled("⚠  Appuyez sur ", warning),
            Span::styled("[q]", key),
            Span::styled(
                " à nouveau pour quitter, autre touche pour annuler",
                warning,
            ),
        ]));
    }
    if let Some(status) = &app.status {
        let color = if status.is_error {
            Color::Red
        } else {
            Color::Green
        };
        return Some(Line::from(Span::styled(
            status.text.clone(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
    }
    if app.is_loading() {
        return Some(Line::from(Span::styled(
            format!("⏳ Chargement… ({})", app.pending),
            Style::default().fg(Color::Cyan),
        )));
    }
    None
}

/// Footer : ligne d'état si besoin, sinon les raccourcis
fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let key = |k: &'static str, color: Color| {
        Span::styled(k, Style::default().fg(color).add_modifier(Modifier::BOLD))
    };
    let line = status_line(app).unwrap_or_else(|| {
        Line::from(vec![
            key("[q]", Color::Yellow),
            Span::raw(" Quit  "),
            key("[↑↓ / j k]", Color::Yellow),
            Span::raw(" Navigate  "),
            key("[Enter]", Color::Yellow),
            Span::raw(" Chart  "),
            key("[a]", Color::Green),
            Span::raw(" Add  "),
            key("[d]", Color::Red),
            Span::raw(" Delete  "),
            key("[r]", Color::Cyan),
            Span::raw(" Refresh"),
        ])
    });
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(line)
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

/// Footer en mode saisie : prompt + buffer + curseur
fn render_input_footer(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" [Enter] Ajouter  [Esc] Annuler ");
    let input_line = Line::from(vec![
        Span::styled(
            "Ajouter : ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(app.input_buffer.clone(), Style::default().fg(Color::White)),
        Span::styled(
            "█",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);
    frame.render_widget(Paragraph::new(input_line).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use chrono::{FixedOffset, Utc};
    use ratatui::{backend::TestBackend, Terminal};

    use crate::models::{FetchedTicker, Interval, OHLCData, Quote, OHLC};

    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.get(x, y).symbol().to_string())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    #[test]
    fn shows_prices_loading_and_errors() {
        let now = Instant::now();
        let mut app = App::new(vec!["EURUSD=X".into(), "TSLA".into()], None, now);
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut data = OHLCData::new("EURUSD=X".into(), Interval::M30, utc);
        data.add_candle(OHLC::new(Utc::now(), 1.1, 1.2, 1.0, 1.1453, 0));
        app.watchlist[0].apply(FetchedTicker {
            data,
            long_name: Some("EUR/USD".into()),
            quote: Quote {
                price: 1.1453,
                previous_close: Some(1.1400),
            },
            currency: Some("USD".into()),
            price_decimals: 4,
        });
        app.pending = 1;
        let text = screen(&app);
        assert!(text.contains("1.1453 USD"), "{text}");
        assert!(text.contains('…'), "TSLA en chargement : {text}");
        assert!(text.contains("Chargement"), "{text}");

        app.pending = 0;
        app.set_error("NOPE : symbole inconnu de Yahoo Finance".into(), now);
        let text = screen(&app);
        assert!(text.contains("NOPE : symbole inconnu"), "{text}");
        assert!(
            text.contains("N/A"),
            "TSLA sans données ni chargement : {text}"
        );
    }
}
