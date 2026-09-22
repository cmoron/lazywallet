// ============================================================================
// Widget : graphique en chandeliers dessiné directement dans le Buffer
// ============================================================================

// CONCEPT RATATUI : Widget
// - `render(self, area, buf)` écrit des cellules dans `buf`, dans `area` seulement
// - Le widget reçoit la zone *intérieure* du cadre : impossible de déborder
//   sur la bordure (bug de l'ancien rendu, qui coupait les 2 dernières colonnes)
// ============================================================================

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::models::{OHLCData, OHLC};
use crate::ui::chart::geometry::{visible_columns, AXIS_ROWS};
use crate::ui::chart::price_axis::{price_ticks, PriceTicks};
use crate::ui::chart::time_axis::time_axis;

/// Taille minimale : en dessous, un message remplace le graphique
pub const MIN_WIDTH: u16 = 30;
pub const MIN_HEIGHT: u16 = AXIS_ROWS + 5;

/// Largeur provisoire de l'axe des prix pour la première passe
const PROVISIONAL_AXIS: u16 = 10;

// Caractères Unicode des chandeliers (algorithme de cli-candlestick-chart)
const UNICODE_VOID: char = ' ';
const UNICODE_BODY: char = '┃'; // Corps plein
const UNICODE_HALF_BODY_BOTTOM: char = '╻'; // Corps avec espace en bas
const UNICODE_HALF_BODY_TOP: char = '╹'; // Corps avec espace en haut
const UNICODE_WICK: char = '│'; // Mèche pleine
const UNICODE_TOP: char = '╽'; // Transition corps→mèche (haut)
const UNICODE_BOTTOM: char = '╿'; // Transition corps→mèche (bas)
const UNICODE_UPPER_WICK: char = '╷'; // Demi-mèche supérieure
const UNICODE_LOWER_WICK: char = '╵'; // Demi-mèche inférieure

const BULLISH_COLOR: Color = Color::Rgb(52, 208, 88); // Vert
const BEARISH_COLOR: Color = Color::Rgb(234, 74, 90); // Rouge

/// Correspondance prix ↔ hauteur en "lignes" (fractionnaires) du graphique
struct Scale {
    min: f64,
    max: f64,
    rows: u16,
}

impl Scale {
    /// Bornes sur les chandelles visibles, 2 % de marge ; un prix plat reçoit ±1 %
    fn new(candles: &[OHLC], rows: u16) -> Self {
        let low = candles.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        let high = candles
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let spread = if high > low {
            high - low
        } else {
            high.abs().max(1.0) * 0.02
        };
        let margin = spread * 0.02;
        let (low, high) = if high > low {
            (low, high)
        } else {
            (low - spread / 2.0, high + spread / 2.0)
        };
        Self {
            min: low - margin,
            max: high + margin,
            rows,
        }
    }

    /// Hauteur (en lignes depuis le bas) d'un prix
    fn height(&self, price: f64) -> f64 {
        (price - self.min) / (self.max - self.min) * f64::from(self.rows)
    }

    /// Ligne (0 = haut) où s'affiche un prix, cohérente avec `glyph`
    fn row_of(&self, price: f64) -> u16 {
        let from_bottom = self.height(price).floor().clamp(1.0, f64::from(self.rows));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // borné par clamp
        let from_bottom = from_bottom as u16;
        self.rows - from_bottom
    }
}

/// Caractère d'une chandelle à la hauteur `y` (1 = ligne du bas)
///
/// Cœur de l'algorithme, adapté de cli-candlestick-chart : trois zones
/// (mèche haute, corps, mèche basse) et des seuils 0.25 / 0.75 pour une
/// précision d'un demi-caractère.
fn glyph(candle: &OHLC, y: u16, scale: &Scale) -> char {
    let height_unit = f64::from(y);

    let high_y = scale.height(candle.high);
    let low_y = scale.height(candle.low);
    let max_y = scale.height(candle.open.max(candle.close));
    let min_y = scale.height(candle.close.min(candle.open));

    let mut output = UNICODE_VOID;

    // ZONE 1 : Mèche supérieure (high → max)
    if high_y.ceil() >= height_unit && height_unit >= max_y.floor() {
        if max_y - height_unit > 0.75 {
            output = UNICODE_BODY;
        } else if (max_y - height_unit) > 0.25 {
            if (high_y - height_unit) > 0.75 {
                output = UNICODE_TOP;
            } else {
                output = UNICODE_HALF_BODY_BOTTOM;
            }
        } else if (high_y - height_unit) > 0.75 {
            output = UNICODE_WICK;
        } else if (high_y - height_unit) > 0.25 {
            output = UNICODE_UPPER_WICK;
        }
    }
    // ZONE 2 : Corps (min → max)
    else if max_y.floor() >= height_unit && height_unit >= min_y.ceil() {
        output = UNICODE_BODY;
    }
    // ZONE 3 : Mèche inférieure (min → low)
    else if min_y.ceil() >= height_unit && height_unit >= low_y.floor() {
        if (min_y - height_unit) < 0.25 {
            output = UNICODE_BODY;
        } else if (min_y - height_unit) < 0.75 {
            if (low_y - height_unit) < 0.25 {
                output = UNICODE_BOTTOM;
            } else {
                output = UNICODE_HALF_BODY_TOP;
            }
        } else if low_y - height_unit < 0.25 {
            output = UNICODE_WICK;
        } else if low_y - height_unit < 0.75 {
            output = UNICODE_LOWER_WICK;
        }
    }

    output
}

fn candle_color(candle: &OHLC) -> Color {
    if candle.is_bullish() {
        BULLISH_COLOR
    } else {
        BEARISH_COLOR
    }
}

/// Largeur de texte (en colonnes) d'un label
fn text_width(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

/// Graphique en chandeliers d'un `OHLCData`
pub struct CandleChart<'a> {
    pub data: &'a OHLCData,
    /// Précision du ticker (`priceHint`) pour le repère du dernier prix
    pub price_decimals: usize,
}

impl Widget for CandleChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            let style = Style::default().fg(Color::Yellow);
            buf.set_stringn(
                area.x,
                area.y,
                "Terminal trop petit",
                usize::from(area.width),
                style,
            );
            return;
        }
        let candles = &self.data.candles;
        if candles.is_empty() {
            buf.set_string(
                area.x,
                area.y,
                "Aucune chandelle",
                Style::default().fg(Color::Gray),
            );
            return;
        }

        let rows = area.height - AXIS_ROWS;
        // Passe 1 : la largeur de l'axe dépend des prix visibles, qui dépendent de la largeur
        // ponytail: deux passes ; si la 2e plage élargit les labels d'un chiffre, le dernier
        // caractère du label est coupé. Évolution : itérer jusqu'à stabilité.
        let axis_width = axis_width(
            candles,
            area.width - PROVISIONAL_AXIS,
            rows,
            self.price_decimals,
        )
        .min(area.width / 2);
        let plot_width = area.width - axis_width;
        let (first, columns) = visible_columns(plot_width, candles.len());
        let visible = &candles[first..];
        let scale = Scale::new(visible, rows);
        let ticks = price_ticks(scale.min, scale.max, rows);

        draw_candles(buf, area, visible, &columns, &scale);
        let last = visible
            .last()
            .map(|c| (c, last_price_label(c, &ticks, self.price_decimals)));
        draw_price_axis(buf, area, plot_width, &scale, &ticks, last);
        draw_time_axis(buf, area, rows, plot_width, self.data, visible, &columns);
    }
}

/// Label du dernier prix : jamais moins précis que le ticker lui-même
fn last_price_label(candle: &OHLC, ticks: &PriceTicks, price_decimals: usize) -> String {
    let decimals = ticks.decimals.max(price_decimals);
    format!("{:.decimals$}", candle.close)
}

/// Largeur de l'axe des prix : "┤ " + le plus long label
fn axis_width(candles: &[OHLC], plot_width: u16, rows: u16, price_decimals: usize) -> u16 {
    let (first, _) = visible_columns(plot_width, candles.len());
    let visible = &candles[first..];
    let scale = Scale::new(visible, rows);
    let ticks = price_ticks(scale.min, scale.max, rows);
    let last = visible
        .last()
        .map(|c| last_price_label(c, &ticks, price_decimals));
    let widest = ticks
        .values
        .iter()
        .map(|v| ticks.format(*v))
        .chain(last)
        .map(|label| text_width(&label))
        .max()
        .unwrap_or(0);
    // "┤ " avant le label, un espace après (le repère du dernier prix est encadré d'espaces)
    widest + 3
}

fn draw_candles(buf: &mut Buffer, area: Rect, visible: &[OHLC], columns: &[u16], scale: &Scale) {
    for (candle, &column) in visible.iter().zip(columns) {
        let style = Style::default().fg(candle_color(candle));
        for y in 1..=scale.rows {
            let symbol = glyph(candle, y, scale);
            if symbol != UNICODE_VOID {
                buf.get_mut(area.x + column, area.y + scale.rows - y)
                    .set_char(symbol)
                    .set_style(style);
            }
        }
    }
}

/// Axe des prix à droite + repère du dernier prix
fn draw_price_axis(
    buf: &mut Buffer,
    area: Rect,
    plot_width: u16,
    scale: &Scale,
    ticks: &PriceTicks,
    last: Option<(&OHLC, String)>,
) {
    let axis_x = area.x + plot_width;
    let label_width = usize::from(area.width - plot_width - 1);
    let gray = Style::default().fg(Color::Gray);
    for row in 0..scale.rows {
        buf.get_mut(axis_x, area.y + row)
            .set_char('│')
            .set_style(gray);
    }

    let marker_row = last.as_ref().map(|(c, _)| scale.row_of(c.close));
    for &value in &ticks.values {
        let row = scale.row_of(value);
        if Some(row) != marker_row {
            buf.get_mut(axis_x, area.y + row)
                .set_char('┤')
                .set_style(gray);
            buf.set_stringn(
                axis_x + 1,
                area.y + row,
                format!(" {}", ticks.format(value)),
                label_width,
                gray,
            );
        }
    }

    // Repère du dernier prix : label inversé dans la couleur de la chandelle,
    // et une ligne pointillée dans les cases vides de sa ligne
    if let (Some((candle, label)), Some(row)) = (last, marker_row) {
        let y = area.y + row;
        let dotted = Style::default().fg(Color::DarkGray);
        for x in area.x..axis_x {
            let cell = buf.get_mut(x, y);
            if cell.symbol() == " " {
                cell.set_char('┈').set_style(dotted);
            }
        }
        let color = candle_color(candle);
        buf.get_mut(axis_x, y)
            .set_char('┤')
            .set_style(Style::default().fg(color));
        let marker = Style::default()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD);
        buf.set_stringn(axis_x + 1, y, format!(" {label} "), label_width, marker);
    }
}

/// Axe du temps : trait avec graduations, labels fins, contexte
fn draw_time_axis(
    buf: &mut Buffer,
    area: Rect,
    rows: u16,
    plot_width: u16,
    data: &OHLCData,
    visible: &[OHLC],
    columns: &[u16],
) {
    let times: Vec<_> = visible.iter().map(|c| data.local_time(c)).collect();
    let axis = time_axis(&times, columns, plot_width);
    let gray = Style::default().fg(Color::Gray);
    let line_y = area.y + rows;

    for x in area.x..area.x + plot_width {
        buf.get_mut(x, line_y).set_char('─').set_style(gray);
    }
    buf.get_mut(area.x + plot_width, line_y)
        .set_char('┘')
        .set_style(gray);

    // Alternance gris / jaune pour distinguer des labels voisins (commit 8878ec3)
    for (i, label) in axis.primary.iter().enumerate() {
        let color = if i % 2 == 0 {
            Color::Gray
        } else {
            Color::Yellow
        };
        let x = area.x + label.column;
        buf.get_mut(x, line_y).set_char('┬').set_style(gray);
        let room = usize::from(plot_width - label.column);
        buf.set_stringn(x, line_y + 1, &label.text, room, Style::default().fg(color));
    }
    let context = gray.add_modifier(Modifier::BOLD);
    for label in &axis.secondary {
        let room = usize::from(plot_width - label.column);
        buf.set_stringn(
            area.x + label.column,
            line_y + 2,
            &label.text,
            room,
            context,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone, Utc};

    use crate::models::Interval;

    fn data(n: usize, price: impl Fn(usize) -> f64) -> OHLCData {
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut d = OHLCData::new("BTC-USD".into(), Interval::H1, utc);
        let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        for i in 0..n {
            let p = price(i);
            let t = t0 + chrono::Duration::hours(i64::try_from(i).unwrap());
            d.add_candle(OHLC::new(t, p, p + 1.0, p - 1.0, p + 0.5, 0));
        }
        d
    }

    fn draw(data: &OHLCData, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        CandleChart {
            data,
            price_decimals: 2,
        }
        .render(area, &mut buf);
        buf
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf.get(x, y).symbol().to_string())
            .collect()
    }

    #[test]
    fn latest_candle_is_drawn_next_to_the_axis() {
        // Bug revue : les 2 dernières colonnes étaient coupées par la bordure
        let d = data(500, |i| 100.0 + f64::from(u32::try_from(i % 20).unwrap()));
        let buf = draw(&d, 100, 20);
        let rows = 20 - AXIS_ROWS;
        // ┤ n'existe que sur l'axe (les mèches utilisent │) : il repère la colonne de l'axe
        let axis_x = (0..100)
            .find(|&x| (0..rows).any(|y| buf.get(x, y).symbol() == "┤"))
            .unwrap();
        assert!(
            (0..rows).any(|y| !matches!(buf.get(axis_x - 1, y).symbol(), " " | "┈")),
            "dernière chandelle absente"
        );
    }

    #[test]
    fn every_row_fits_the_area_exactly() {
        let d = data(300, |i| 100.0 + f64::from(u32::try_from(i).unwrap()));
        for (w, h) in [(30, 8), (79, 24), (120, 30), (250, 60)] {
            let buf = draw(&d, w, h);
            assert_eq!(buf.area, Rect::new(0, 0, w, h));
            assert!(
                !row(&buf, h - 1).trim().is_empty(),
                "contexte temporel présent en {w}x{h}"
            );
        }
    }

    #[test]
    fn tiny_areas_show_a_message_without_panicking() {
        let d = data(50, |_| 100.0);
        for (w, h) in [(0, 0), (1, 1), (29, 20), (80, 7)] {
            draw(&d, w, h);
        }
        assert!(row(&draw(&d, 29, 20), 0).contains("petit"));
    }

    #[test]
    fn flat_prices_and_single_candle_render() {
        draw(&data(40, |_| 42.0), 80, 20);
        draw(&data(1, |_| 42.0), 80, 20);
        let empty = OHLCData::new("X".into(), Interval::D1, FixedOffset::east_opt(0).unwrap());
        draw(&empty, 80, 20);
    }

    #[test]
    fn last_price_is_marked_on_the_axis() {
        let d = data(100, |i| 100.0 + f64::from(u32::try_from(i).unwrap())); // dernière clôture : 199.5
        let buf = draw(&d, 100, 30);
        assert!(
            (0..27).any(|y| row(&buf, y).contains("199.5")),
            "prix courant affiché sur l'axe"
        );
    }
}
