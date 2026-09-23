// ============================================================================
// Module : chart
// ============================================================================
// Graphique en chandeliers : géométrie, axes et widget
// ============================================================================

pub mod geometry; // Colonnes des chandelles
pub mod indicators; // Moyennes mobiles
pub mod price_axis; // Graduations de l'axe des prix
pub mod time_axis; // Graduations de l'axe du temps
pub mod widget; // Widget ratatui du graphique

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use chrono::Utc;

use crate::app::App;
use crate::models::{OHLCData, WatchlistItem, OHLC};
use crate::ui::dashboard::status_line;
use widget::CandleChart;

/// Écran graphique : header (prix, variation, raccourcis) + chandeliers
pub fn render_chart_screen(frame: &mut Frame, app: &App) {
    let area = frame.size();
    let Some(item) = app.selected_item() else {
        render_message(frame, area, "Aucun ticker sélectionné");
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_header(frame, app, item, chunks[0]);

    let Some(data) = item.data.as_ref().filter(|d| !d.is_empty()) else {
        let text = if app.is_loading() {
            "Chargement…".to_string()
        } else {
            format!("Pas de données pour {} — [r] pour réessayer", item.symbol)
        };
        render_message(frame, chunks[1], &text);
        return;
    };

    // Intervalle affiché : celui des données, et le choix en cours s'il diffère
    let interval = if data.interval == app.current_interval {
        data.interval.label().to_string()
    } else {
        {
            // ⏳ pendant le chargement ; ⚠ s'il a échoué ([r] réessaie)
            let marker = if app.is_loading() { "⏳" } else { "⚠ [r]" };
            format!(
                "{} → {} {marker}",
                data.interval.label(),
                app.current_interval.label()
            )
        }
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        // Pas de plage de dates ici : le widget n'en montre qu'une partie, l'axe les porte
        .title(chart_title(app, &interval));

    // CONCEPT : le widget reçoit la zone intérieure du cadre, jamais la bordure
    let inner = block.inner(chunks[1]);
    let chart = CandleChart::new(data, item.price_decimals)
        .window(data.len().saturating_sub(app.chart_offset))
        .cursor(app.cursor)
        .moving_averages(app.show_moving_averages)
        .report_view(&app.chart_view);
    frame.render_widget(block, chunks[1]);
    frame.render_widget(chart, inner);
}

/// Header : état (confirmation, erreur, chargement) ou prix + raccourcis
fn render_header(frame: &mut Frame, app: &App, item: &WatchlistItem, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} · {} ", item.symbol, item.name));

    let cursor_candle = app.cursor.and_then(|c| {
        item.data
            .as_ref()
            .and_then(|d| Some((d, d.candles.get(c)?)))
    });
    let line = status_line(app)
        .or_else(|| cursor_candle.map(|(data, candle)| cursor_line(item, data, candle)))
        .unwrap_or_else(|| {
            let key = |k: &'static str| {
                Span::styled(
                    k,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            };
            let mut spans = Vec::new();
            if let Some(price) = item.current_price() {
                let color = if item.is_positive() {
                    Color::Green
                } else {
                    Color::Red
                };
                spans.push(Span::styled(
                    item.format_price(price),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ));
                if let Some(change) = item.change_percent() {
                    let arrow = if change >= 0.0 { "▲" } else { "▼" };
                    spans.push(Span::styled(
                        format!("  {arrow} {change:+.2}%"),
                        Style::default().fg(color),
                    ));
                }
                spans.push(Span::raw("    "));
            }
            if let Some(open) = item.is_market_open(Utc::now()) {
                let (text, color) = if open {
                    ("● Marché ouvert    ", Color::Green)
                } else {
                    ("○ Marché fermé    ", Color::DarkGray)
                };
                spans.push(Span::styled(text, Style::default().fg(color)));
            }
            spans.extend([
                key("[h/l]"),
                Span::raw(" Intervalle  "),
                key("[←→]"),
                Span::raw(" Curseur  "),
                key("[PgUp/PgDn]"),
                Span::raw(" Défiler  "),
                key("[r]"),
                Span::raw(" Rafraîchir  "),
                key("[Esc]"),
                Span::raw(" Retour  "),
                key("[q]"),
                Span::raw(" Quitter"),
            ]);
            Line::from(spans)
        });

    frame.render_widget(
        Paragraph::new(line)
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

/// Titre du cadre : intervalle, décalage dans le passé, légende des moyennes
fn chart_title(app: &App, interval: &str) -> Line<'static> {
    let mut spans = vec![Span::raw(format!(" {interval} "))];
    if app.chart_offset > 0 {
        // Vue décalée dans le passé : le rappeler, avec le moyen d'en revenir
        spans.push(Span::raw(format!(
            "· -{} chandelles [End] ",
            app.chart_offset
        )));
    }
    if app.show_moving_averages {
        spans.extend([
            Span::styled("· MA20 • ", Style::default().fg(Color::Yellow)),
            Span::styled("MA50 • ", Style::default().fg(Color::Magenta)),
        ]);
    }
    Line::from(spans)
}

/// Détail de la chandelle sous le curseur : date, OHLC, volume
fn cursor_line(item: &WatchlistItem, data: &OHLCData, candle: &OHLC) -> Line<'static> {
    let price =
        |label: &str, value: f64| format!("{label} {value:.prec$}  ", prec = item.price_decimals);
    let color = if candle.is_bullish() {
        Color::Green
    } else {
        Color::Red
    };
    let mut spans = vec![
        Span::styled(
            format!(
                "{}    ",
                data.local_time(candle).format("%a %d/%m/%Y %H:%M")
            ),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            [
                price("O", candle.open),
                price("H", candle.high),
                price("B", candle.low),
                price("C", candle.close),
            ]
            .concat(),
            Style::default().fg(color),
        ),
    ];
    if candle.volume > 0 {
        spans.push(Span::raw(format!("Vol {}  ", abbreviate(candle.volume))));
    }
    spans.push(Span::styled(
        "  [←→] Curseur  [End] Présent  [Esc] Masquer",
        Style::default().fg(Color::DarkGray),
    ));
    Line::from(spans)
}

/// Volume abrégé : 950, 12.3k, 1.2M, 3.4G
pub fn abbreviate(volume: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // affichage à 1 décimale
    let value = volume as f64;
    match volume {
        0..1_000 => volume.to_string(),
        1_000..1_000_000 => format!("{:.1}k", value / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", value / 1e6),
        _ => format!("{:.1}G", value / 1e9),
    }
}

/// Message centré dans un cadre (pas de données, chargement...)
fn render_message(frame: &mut Frame, area: Rect, text: &str) {
    let paragraph = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(Color::Gray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "[Esc] Retour",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use chrono::{FixedOffset, TimeZone, Utc};
    use ratatui::{backend::TestBackend, Terminal};

    use crate::app::{App, Screen};
    use crate::models::{FetchedTicker, Interval, OHLCData, Quote, OHLC};

    fn sample_fetched(interval: Interval, n: u32) -> FetchedTicker {
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut data = OHLCData::new("BTC-USD".into(), interval, utc);
        let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        for i in 0..n {
            let p = 100.0 + f64::from(i % 17);
            let t = t0 + chrono::Duration::minutes(30 * i64::from(i));
            data.add_candle(OHLC::new(t, p, p + 1.0, p - 1.0, p + 0.5, 0));
        }
        FetchedTicker {
            data,
            long_name: Some("Bitcoin USD".into()),
            quote: Quote {
                price: 116.5,
                previous_close: Some(110.0),
            },
            currency: Some("USD".into()),
            price_decimals: 2,
            session: None,
            fetched_at: chrono::Utc::now(),
        }
    }

    fn screen(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render_chart_screen(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn chart_screen_shows_title_marker_and_status() {
        let mut app = App::new(vec!["BTC-USD".into()], None, Instant::now());
        // données 30m chargées, intervalle choisi 1h : l'écran doit signaler le rechargement
        app.watchlist[0].apply(sample_fetched(Interval::M30, 200));
        app.current_interval = Interval::H1;
        app.current_screen = Screen::ChartView;
        app.pending = 1;
        let text = screen(&app, 120, 30);
        assert!(text.contains("30m → 1h"), "{text}");
        assert!(text.contains("Chargement"), "{text}");
        assert!(text.contains("Bitcoin USD"), "{text}");
    }

    #[test]
    fn chart_screen_without_data() {
        let mut app = App::new(vec!["NOPE".into()], None, Instant::now());
        app.current_screen = Screen::ChartView;
        assert!(screen(&app, 80, 20).contains("Pas de données pour NOPE"));
        app.pending = 1;
        assert!(screen(&app, 80, 20).contains("Chargement"));
    }

    #[test]
    fn chart_header_shows_market_state() {
        let mut app = App::new(vec!["BTC-USD".into()], None, Instant::now());
        let mut fetched = sample_fetched(Interval::M30, 50);
        let now = Utc::now();
        fetched.session = Some((
            now - chrono::Duration::hours(1),
            now + chrono::Duration::hours(1),
        ));
        app.watchlist[0].apply(fetched);
        app.current_screen = Screen::ChartView;
        assert!(screen(&app, 140, 20).contains("Marché ouvert"));
        app.watchlist[0].session = Some((
            now + chrono::Duration::hours(1),
            now + chrono::Duration::hours(2),
        ));
        assert!(screen(&app, 140, 20).contains("Marché fermé"));
    }

    #[test]
    fn cursor_shows_the_candle_in_the_header() {
        let mut app = App::new(vec!["BTC-USD".into()], None, Instant::now());
        app.watchlist[0].apply(sample_fetched(Interval::M30, 50));
        app.current_screen = Screen::ChartView;
        app.cursor = Some(10);
        let text = screen(&app, 140, 20);
        assert!(
            text.contains("O 110.00") && text.contains("C 110.50"),
            "{text}"
        );
    }

    #[test]
    fn volumes_are_abbreviated() {
        assert_eq!(abbreviate(950), "950");
        assert_eq!(abbreviate(12_345), "12.3k");
        assert_eq!(abbreviate(1_234_567), "1.2M");
        assert_eq!(abbreviate(3_400_000_000), "3.4G");
    }

    #[test]
    fn title_shows_the_moving_average_legend() {
        let mut app = App::new(vec!["BTC-USD".into()], None, Instant::now());
        app.watchlist[0].apply(sample_fetched(Interval::M30, 100));
        app.current_screen = Screen::ChartView;
        assert!(screen(&app, 140, 20).contains("MA20"));
        app.show_moving_averages = false;
        assert!(!screen(&app, 140, 20).contains("MA20"));
    }
}
