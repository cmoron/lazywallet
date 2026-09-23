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
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use chrono::{DateTime, Local, Utc};

use crate::app::{App, InputKind, Screen, REFRESH_EVERY};
use crate::models::WatchlistItem;
use crate::portfolio;
use crate::ui::chart::render_chart_screen;
use crate::ui::sparkline::{sparkline, trend_closes};

/// Dessine l'écran courant
///
/// CONCEPT RUST : Match sur enum pour router (state machine)
pub fn render(frame: &mut Frame, app: &App) {
    match app.current_screen {
        Screen::ChartView => render_chart_screen(frame, app),
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
    let totals = portfolio::totals(&app.watchlist);
    let line = if totals.is_empty() {
        Line::from(Span::styled(
            format!(
                "{} tickers · rafraîchi toutes les {} s",
                app.watchlist.len(),
                REFRESH_EVERY.as_secs()
            ),
            Style::default().fg(Color::Gray),
        ))
    } else {
        // Une section par devise : valeur, plus-value latente, gain du jour
        let mut spans = vec![Span::styled(
            "Portefeuille",
            Style::default().fg(Color::Gray),
        )];
        for total in &totals {
            spans.extend([
                Span::styled(
                    format!("  ·  {} {:.2} ", total.currency, total.value),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:+.2} ({:+.2}%)", total.pnl(), total.pnl_percent()),
                    Style::default().fg(sign_color(total.pnl())),
                ),
                Span::styled(" jour ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:+.2}", total.day_pnl),
                    Style::default().fg(sign_color(total.day_pnl)),
                ),
            ]);
        }
        Line::from(spans)
    };
    let mut line = line;
    // Heure (locale de la machine) du chargement le plus récent
    if let Some(updated) = app
        .watchlist
        .iter()
        .filter_map(|item| item.updated_at)
        .max()
    {
        line.spans.push(Span::styled(
            format!(
                "  ·  MàJ {}",
                updated.with_timezone(&Local).format("%H:%M:%S")
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }
    let paragraph = Paragraph::new(line)
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

/// Vert si positif (ou nul), rouge sinon
fn sign_color(value: f64) -> Color {
    if value >= 0.0 {
        Color::Green
    } else {
        Color::Red
    }
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

/// Colonnes possibles de la watchlist
///
/// CONCEPT : Responsive — on garde les colonnes par ordre de priorité tant
/// qu'elles tiennent, puis on les affiche dans l'ordre naturel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Column {
    Market,
    Symbol,
    Name,
    Price,
    Change,
    Trend,
    Quantity,
    Value,
    Pnl,
}

impl Column {
    /// Colonnes par ordre de priorité (la première est toujours affichée)
    const PRIORITY: [Column; 9] = [
        Column::Symbol,
        Column::Market,
        Column::Price,
        Column::Change,
        Column::Trend,
        Column::Value,
        Column::Pnl,
        Column::Name,
        Column::Quantity,
    ];

    /// Colonnes qui n'ont de sens qu'avec au moins une position
    fn is_portfolio(self) -> bool {
        matches!(self, Column::Quantity | Column::Value | Column::Pnl)
    }

    fn width(self) -> u16 {
        match self {
            Column::Market => 1,
            Column::Symbol | Column::Change | Column::Quantity => 10,
            Column::Name | Column::Trend => 20,
            Column::Price => 16,
            Column::Value => 14,
            Column::Pnl => 22,
        }
    }

    fn header(self) -> &'static str {
        match self {
            Column::Market => "",
            Column::Symbol => "Symbole",
            Column::Name => "Nom",
            Column::Price => "Prix",
            Column::Change => "Jour",
            Column::Trend => "Tendance",
            Column::Quantity => "Qté",
            Column::Value => "Valeur",
            Column::Pnl => "+/- latent",
        }
    }

    fn right_aligned(self) -> bool {
        !matches!(
            self,
            Column::Market | Column::Symbol | Column::Name | Column::Trend
        )
    }

    /// Contenu de la cellule pour un item
    fn text(self, item: &WatchlistItem, loading: bool, now: DateTime<Utc>) -> String {
        match self {
            // ● marché ouvert, ○ fermé, rien si Yahoo n'a pas donné la séance
            Column::Market => match item.is_market_open(now) {
                Some(true) => "●".to_string(),
                Some(false) => "○".to_string(),
                None => String::new(),
            },
            Column::Symbol => item.symbol.clone(),
            Column::Name => truncate_with_ellipsis(&item.name, usize::from(self.width())),
            Column::Price => item.current_price().map_or_else(
                // Pas encore de prix : en cours de chargement, ou échec (voir barre d'état)
                || if loading { "…" } else { "N/A" }.to_string(),
                |price| item.format_price(price),
            ),
            Column::Change => item.change_percent().map_or_else(String::new, |c| {
                let arrow = if c >= 0.0 { "▲" } else { "▼" };
                format!("{arrow} {c:+.2}%")
            }),
            Column::Trend => item.data.as_ref().map_or_else(String::new, |data| {
                sparkline(&trend_closes(data), usize::from(self.width()))
            }),
            Column::Quantity => item
                .position
                .map_or_else(String::new, |p| p.quantity.to_string()),
            Column::Value => item
                .market_value()
                .map_or_else(String::new, |v| format!("{v:.2}")),
            Column::Pnl => match (item.unrealized_pnl(), item.unrealized_pnl_percent()) {
                (Some(pnl), Some(percent)) => format!("{pnl:+.2} ({percent:+.2}%)"),
                _ => String::new(),
            },
        }
    }

    /// Couleur propre à la cellule (la plus-value suit son signe, pas la journée)
    fn color(self, item: &WatchlistItem, now: DateTime<Utc>) -> Option<Color> {
        match self {
            Column::Market => {
                item.is_market_open(now)
                    .map(|open| if open { Color::Green } else { Color::DarkGray })
            }
            Column::Pnl => item.unrealized_pnl().map(sign_color),
            _ => None,
        }
    }
}

/// Colonnes qui tiennent dans `width`, dans l'ordre d'affichage
fn columns_for(width: u16, has_positions: bool) -> Vec<Column> {
    let mut used = 0u16;
    let mut columns = Vec::new();
    for column in Column::PRIORITY {
        if column.is_portfolio() && !has_positions {
            continue;
        }
        // +1 : espacement entre colonnes
        let needed = column.width() + 1;
        if columns.is_empty() || used + needed <= width {
            used += needed;
            columns.push(column);
        }
    }
    columns.sort();
    columns
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

    let now = Utc::now();
    let has_positions = app.watchlist.iter().any(|item| item.position.is_some());
    let columns = columns_for(block.inner(area).width, has_positions);
    let cell = |column: Column, text: String, color: Option<Color>| {
        let line = Line::from(match color {
            Some(color) => Span::styled(text, Style::default().fg(color)),
            None => Span::raw(text),
        });
        Cell::from(if column.right_aligned() {
            line.alignment(Alignment::Right)
        } else {
            line
        })
    };

    let header = Row::new(
        columns
            .iter()
            .map(|&c| cell(c, c.header().to_string(), None))
            .collect::<Vec<_>>(),
    )
    .style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.watchlist.iter().map(|item| {
        let color = match item.current_price() {
            None => Color::Gray,
            Some(_) if item.is_positive() => Color::Green,
            Some(_) => Color::Red,
        };
        Row::new(
            columns
                .iter()
                .map(|&c| {
                    let text = c.text(item, app.is_loading(), now);
                    cell(c, text, c.color(item, now))
                })
                .collect::<Vec<_>>(),
        )
        .style(Style::default().fg(color))
    });

    let widths: Vec<Constraint> = columns
        .iter()
        .map(|c| Constraint::Length(c.width()))
        .collect();
    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED));

    // Décalage conservé entre deux images : ratatui ne fait défiler que si la
    // sélection sort de la vue (voir App::list_offset)
    let mut state = TableState::default()
        .with_offset(app.list_offset.get())
        .with_selected(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut state);
    app.list_offset.set(state.offset());
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
        .title(" [Enter] Valider  [Esc] Annuler ");
    let prompt = match &app.input_kind {
        InputKind::AddTicker => "Ajouter : ".to_string(),
        InputKind::Position(symbol) => {
            format!("Position {symbol} (quantité prix_de_revient, vide = aucune) : ")
        }
    };
    let input_line = Line::from(vec![
        Span::styled(
            prompt,
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

    use chrono::{FixedOffset, TimeZone, Utc};
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
            session: None,
            fetched_at: chrono::Utc::now(),
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

    fn screen_sized(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
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

    fn many(n: usize) -> App {
        let symbols = (0..n).map(|i| format!("T{i:02}")).collect();
        App::new(symbols, None, Instant::now())
    }

    #[test]
    fn selection_stays_visible_when_scrolling() {
        // Revue : au-delà de la hauteur de l'écran, la sélection sortait de la vue
        let mut app = many(40);
        app.selected_index = 30;
        let text = screen_sized(&app, 80, 16);
        assert!(text.contains("T30"), "{text}");
    }

    #[test]
    fn scrolling_up_keeps_offset() {
        let mut app = many(40);
        app.selected_index = 30;
        screen_sized(&app, 80, 16);
        app.selected_index = 25;
        let text = screen_sized(&app, 80, 16);
        // La vue ne saute pas : T30 reste visible en remontant de quelques lignes
        assert!(text.contains("T25") && text.contains("T30"), "{text}");
    }

    #[test]
    fn positions_show_value_pnl_and_currency_totals() {
        let now = Instant::now();
        let mut app = App::new(vec!["AAPL".into(), "TSLA".into()], None, now);
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut data = OHLCData::new("AAPL".into(), Interval::M30, utc);
        data.add_candle(OHLC::new(Utc::now(), 1.0, 1.0, 1.0, 110.0, 0));
        app.watchlist[0].apply(FetchedTicker {
            data,
            long_name: Some("Apple Inc.".into()),
            quote: Quote {
                price: 110.0,
                previous_close: Some(105.0),
            },
            currency: Some("USD".into()),
            price_decimals: 2,
            session: None,
            fetched_at: chrono::Utc::now(),
        });
        app.watchlist[0].position = Some(crate::models::Position {
            quantity: 10.0,
            unit_cost: 100.0,
        });
        let text = screen_sized(&app, 140, 12);
        assert!(text.contains("1100.00"), "valeur : {text}");
        assert!(
            text.contains("+100.00") && text.contains("+10.00%"),
            "plus-value : {text}"
        );
        assert!(
            text.contains("USD") && text.contains("+50.00"),
            "total du jour : {text}"
        );

        // Saisie d'une position : le prompt nomme le ticker
        app.start_position_input();
        assert!(screen_sized(&app, 140, 12).contains("Position AAPL"));
    }

    #[test]
    fn rows_show_a_trend_sparkline() {
        let now = Instant::now();
        let mut app = App::new(vec!["BTC-USD".into()], None, now);
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut data = OHLCData::new("BTC-USD".into(), Interval::M30, utc);
        let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 10, 0, 0).unwrap();
        for i in 0..8u32 {
            let p = 100.0 + f64::from(i);
            data.add_candle(OHLC::new(
                t0 + chrono::Duration::minutes(30 * i64::from(i)),
                p,
                p,
                p,
                p,
                0,
            ));
        }
        app.watchlist[0].apply(FetchedTicker {
            data,
            long_name: None,
            quote: Quote {
                price: 107.0,
                previous_close: Some(100.0),
            },
            currency: Some("USD".into()),
            price_decimals: 2,
            session: None,
            fetched_at: chrono::Utc::now(),
        });
        let text = screen_sized(&app, 120, 10);
        assert!(text.contains('█') && text.contains('▁'), "{text}");
    }

    #[test]
    fn market_dot_and_last_update_are_shown() {
        let now = Instant::now();
        let mut app = App::new(vec!["BTC-USD".into()], None, now);
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut data = OHLCData::new("BTC-USD".into(), Interval::M30, utc);
        data.add_candle(OHLC::new(Utc::now(), 1.0, 1.0, 1.0, 1.0, 0));
        app.watchlist[0].apply(FetchedTicker {
            data,
            long_name: None,
            quote: Quote {
                price: 1.0,
                previous_close: Some(1.0),
            },
            currency: None,
            price_decimals: 2,
            session: Some((
                Utc::now() - chrono::Duration::hours(1),
                Utc::now() + chrono::Duration::hours(1),
            )),
            fetched_at: Utc::now(),
        });
        let text = screen_sized(&app, 120, 10);
        assert!(text.contains('●'), "marché ouvert : {text}");
        assert!(text.contains("MàJ"), "heure de mise à jour : {text}");
    }
}
