// ============================================================================
// Widget : graphique en chandeliers dessiné directement dans le Buffer
// ============================================================================

// CONCEPT RATATUI : Widget
// - `render(self, area, buf)` écrit des cellules dans `buf`, dans `area` seulement
// - Le widget reçoit la zone *intérieure* du cadre : impossible de déborder
//   sur la bordure (bug de l'ancien rendu, qui coupait les 2 dernières colonnes)
// ============================================================================

use std::cell::Cell;

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

// Caractères Unicode des mèches (algorithme de cli-candlestick-chart)
const UNICODE_VOID: char = ' ';
const UNICODE_WICK: char = '│'; // Mèche pleine
const UNICODE_UPPER_WICK: char = '╷'; // Demi-mèche supérieure
const UNICODE_LOWER_WICK: char = '╵'; // Demi-mèche inférieure

/// Caractères du corps d'une chandelle
///
/// CONCEPT : Deux jeux selon la densité
/// - Chandelles espacées : blocs pleins, un corps lisible qui remplit la cellule
/// - Chandelles serrées : traits épais, sinon des blocs voisins forment un mur
struct BodyGlyphs {
    /// Corps plein
    full: char,
    /// Corps dans la moitié basse de la cellule
    lower_half: char,
    /// Corps dans la moitié haute de la cellule
    upper_half: char,
    /// Corps en bas + mèche au-dessus
    top_transition: char,
    /// Corps en haut + mèche en dessous
    bottom_transition: char,
}

const THICK_BODY: BodyGlyphs = BodyGlyphs {
    full: '█',
    lower_half: '▄',
    upper_half: '▀',
    // Pas de caractère "demi-bloc + trait fin" : le demi-bloc prime, la mèche
    // continue dans la cellule voisine
    top_transition: '▄',
    bottom_transition: '▀',
};

const THIN_BODY: BodyGlyphs = BodyGlyphs {
    full: '┃',
    lower_half: '╻',
    upper_half: '╹',
    top_transition: '╽',
    bottom_transition: '╿',
};

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
fn glyph(candle: &OHLC, y: u16, scale: &Scale, body: &BodyGlyphs) -> char {
    let height_unit = f64::from(y);

    let high_y = scale.height(candle.high);
    let low_y = scale.height(candle.low);
    let max_y = scale.height(candle.open.max(candle.close));
    let min_y = scale.height(candle.close.min(candle.open));

    let mut output = UNICODE_VOID;

    // ZONE 1 : Mèche supérieure (high → max)
    if high_y.ceil() >= height_unit && height_unit >= max_y.floor() {
        if max_y - height_unit > 0.75 {
            output = body.full;
        } else if (max_y - height_unit) > 0.25 {
            if (high_y - height_unit) > 0.75 {
                output = body.top_transition;
            } else {
                output = body.lower_half;
            }
        } else if (high_y - height_unit) > 0.75 {
            output = UNICODE_WICK;
        } else if (high_y - height_unit) > 0.25 {
            output = UNICODE_UPPER_WICK;
        }
    }
    // ZONE 2 : Corps (min → max)
    else if max_y.floor() >= height_unit && height_unit >= min_y.ceil() {
        output = body.full;
    }
    // ZONE 3 : Mèche inférieure (min → low)
    else if min_y.ceil() >= height_unit && height_unit >= low_y.floor() {
        if (min_y - height_unit) < 0.25 {
            output = body.full;
        } else if (min_y - height_unit) < 0.75 {
            if (low_y - height_unit) < 0.25 {
                output = body.bottom_transition;
            } else {
                output = body.upper_half;
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
///
/// CONCEPT RUST : Builder
/// - `new` donne le graphique par défaut (chandelles les plus récentes, sans curseur)
/// - chaque option (`window`, `cursor`, `report_view`) consomme et rend `self`
pub struct CandleChart<'a> {
    data: &'a OHLCData,
    /// Précision du ticker (`priceHint`) pour le repère du dernier prix
    price_decimals: usize,
    /// Fin (exclue) de la fenêtre affichée ; None = jusqu'à la dernière chandelle
    end: Option<usize>,
    /// Chandelle sous le curseur
    cursor: Option<usize>,
    /// Où écrire la plage visible (`first..end`) après le rendu
    view: Option<&'a Cell<Option<(usize, usize)>>>,
}

impl<'a> CandleChart<'a> {
    pub fn new(data: &'a OHLCData, price_decimals: usize) -> Self {
        Self {
            data,
            price_decimals,
            end: None,
            cursor: None,
            view: None,
        }
    }

    /// Affiche les chandelles jusqu'à `end` (exclue) : remonter dans le temps
    #[must_use]
    pub fn window(mut self, end: usize) -> Self {
        self.end = Some(end);
        self
    }

    /// Marque la colonne d'une chandelle
    #[must_use]
    pub fn cursor(mut self, cursor: Option<usize>) -> Self {
        self.cursor = cursor;
        self
    }

    /// Reporte la plage visible dans `view` (l'app en a besoin pour naviguer)
    #[must_use]
    pub fn report_view(mut self, view: &'a Cell<Option<(usize, usize)>>) -> Self {
        self.view = Some(view);
        self
    }
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
        let all = &self.data.candles;
        let candles = &all[..self.end.map_or(all.len(), |end| end.min(all.len()))];
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
        let axis_width = fit_axis_width(candles, area.width, rows, self.price_decimals);
        let plot_width = area.width - axis_width;
        let (first, columns) = visible_columns(plot_width, candles.len());
        let visible = &candles[first..];
        let scale = Scale::new(visible, rows);
        let ticks = price_ticks(scale.min, scale.max, rows);

        if let Some(view) = self.view {
            view.set(Some((first, candles.len())));
        }

        draw_candles(buf, area, visible, &columns, &scale);
        if let Some(column) = self
            .cursor
            .and_then(|c| c.checked_sub(first))
            .and_then(|i| columns.get(i))
        {
            draw_cursor(buf, area, *column, rows);
        }
        let last = visible
            .last()
            .map(|c| (c, last_price_label(c, &ticks, self.price_decimals)));
        draw_price_axis(buf, area, plot_width, &scale, &ticks, last);
        draw_time_axis(buf, area, rows, plot_width, self.data, visible, &columns);
    }
}

/// Largeur de l'axe des prix pour une zone de `width` colonnes
///
/// CONCEPT : Point fixe. La largeur de l'axe dépend des prix visibles, qui
/// dépendent de la largeur restante. On part d'une estimation et on élargit
/// tant que les labels ne tiennent pas ; la largeur ne fait que croître et
/// reste bornée par `width / 2`, donc la boucle s'arrête.
fn fit_axis_width(candles: &[OHLC], width: u16, rows: u16, price_decimals: usize) -> u16 {
    let max = width / 2;
    let mut axis = PROVISIONAL_AXIS.min(max);
    loop {
        let needed = axis_width(candles, width - axis, rows, price_decimals).min(max);
        if needed <= axis {
            return axis;
        }
        axis = needed;
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
    // Chandelles espacées (au moins une colonne libre entre elles) : corps épais
    let spaced = columns.windows(2).next().is_some_and(|w| w[1] - w[0] >= 2);
    let body = if spaced { &THICK_BODY } else { &THIN_BODY };
    for (candle, &column) in visible.iter().zip(columns) {
        let style = Style::default().fg(candle_color(candle));
        for y in 1..=scale.rows {
            let symbol = glyph(candle, y, scale, body);
            if symbol != UNICODE_VOID {
                buf.get_mut(area.x + column, area.y + scale.rows - y)
                    .set_char(symbol)
                    .set_style(style);
            }
        }
    }
}

/// Ligne verticale pointillée du curseur, dans les cases vides seulement
fn draw_cursor(buf: &mut Buffer, area: Rect, column: u16, rows: u16) {
    let style = Style::default().fg(Color::White);
    for row in 0..rows {
        let cell = buf.get_mut(area.x + column, area.y + row);
        if cell.symbol() == " " {
            cell.set_char('┆').set_style(style);
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
        CandleChart::new(data, 2).render(area, &mut buf);
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

    #[test]
    fn bodies_are_thick_when_candles_have_room() {
        let d = data(300, |i| {
            100.0 + f64::from(u32::try_from(i % 30).unwrap()) * 3.0
        });
        let symbols =
            |buf: &Buffer| -> String { (0..buf.area.height).map(|y| row(buf, y)).collect() };
        // 150 colonnes : chandelle + espace → corps en blocs pleins
        let wide = symbols(&draw(&d, 150, 30));
        assert!(wide.contains('█') && !wide.contains('┃'), "{wide}");
        // 60 colonnes : chandelles serrées → corps fins, sinon elles forment un mur
        let narrow = symbols(&draw(&d, 60, 30));
        assert!(narrow.contains('┃') && !narrow.contains('█'), "{narrow}");
    }

    #[test]
    fn axis_width_fits_the_labels_it_ends_up_showing() {
        // Revue : vieux prix énormes, récents petits → 2 passes ne suffisaient pas,
        // l'axe affichait "100000" pour 1 000 000
        let d = data(400, |i| if i < 350 { 1_234_567.0 } else { 99.5 });
        for width in [30u16, 40, 60, 90, 200] {
            let rows = 20 - AXIS_ROWS;
            let axis = fit_axis_width(&d.candles, width, rows, 2);
            let needed = axis_width(&d.candles, width - axis, rows, 2);
            assert!(
                needed <= axis,
                "largeur {width} : axe {axis}, labels {needed}"
            );
        }
    }

    #[test]
    fn window_end_shows_older_candles_and_reports_the_view() {
        let d = data(100, |i| 100.0 + f64::from(u32::try_from(i).unwrap()));
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        let view = std::cell::Cell::new(None);
        CandleChart::new(&d, 2)
            .window(50)
            .report_view(&view)
            .render(area, &mut buf);
        let text: String = (0..20).map(|y| row(&buf, y)).collect();
        // Dernière chandelle affichée : index 49, clôture 149.5
        assert!(text.contains("149.50"), "{text}");
        let (first, end) = view.get().unwrap();
        assert_eq!(end, 50);
        assert!(first < end);
    }

    #[test]
    fn cursor_column_is_marked() {
        let d = data(100, |i| 100.0 + f64::from(u32::try_from(i % 10).unwrap()));
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        CandleChart::new(&d, 2)
            .cursor(Some(90))
            .render(area, &mut buf);
        let text: String = (0..20).map(|y| row(&buf, y)).collect();
        assert!(text.contains('┆'), "{text}");
        // Curseur hors de la vue : rien n'est dessiné
        let mut buf = Buffer::empty(area);
        CandleChart::new(&d, 2)
            .window(50)
            .cursor(Some(90))
            .render(area, &mut buf);
        let text: String = (0..20).map(|y| row(&buf, y)).collect();
        assert!(!text.contains('┆'));
    }
}
