// ============================================================================
// Candlestick Chart - Rendu texte ligne par ligne
// ============================================================================
// Implémentation inspirée de cli-candlestick-chart mais intégrée à ratatui
// Utilise des caractères Unicode pour dessiner les chandeliers japonais
//
// ALGORITHME :
// - Rendu vertical : ligne par ligne de haut en bas
// - Pour chaque ligne, on détermine quel caractère Unicode afficher
// - Logique des 3 zones : mèche supérieure, corps, mèche inférieure
// - Seuils fractionnaires (0.25, 0.75) pour précision sub-caractère
//
// CARACTÈRES UNICODE :
// ┃ Corps plein          │ Mèche pleine
// ╻ Demi-corps (bas)     ╹ Demi-corps (haut)
// ╽ Transition top       ╿ Transition bottom
// ╷ Demi-mèche sup       ╵ Demi-mèche inf
// ============================================================================

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use chrono::{Datelike, Timelike};

use crate::app::App;
use crate::models::{Interval, LabelStrategy, OHLC};

// ============================================================================
// Constantes
// ============================================================================

/// Caractères Unicode pour le rendu des chandeliers
const UNICODE_VOID: char = ' ';
const UNICODE_BODY: char = '┃'; // Corps plein
const UNICODE_HALF_BODY_BOTTOM: char = '╻'; // Corps avec espace en bas
const UNICODE_HALF_BODY_TOP: char = '╹'; // Corps avec espace en haut
const UNICODE_WICK: char = '│'; // Mèche pleine
const UNICODE_TOP: char = '╽'; // Transition corps→mèche (haut)
const UNICODE_BOTTOM: char = '╿'; // Transition corps→mèche (bas)
const UNICODE_UPPER_WICK: char = '╷'; // Demi-mèche supérieure
const UNICODE_LOWER_WICK: char = '╵'; // Demi-mèche inférieure

/// Couleurs pour chandeliers haussiers et baissiers
const BULLISH_COLOR: Color = Color::Rgb(52, 208, 88); // Vert
const BEARISH_COLOR: Color = Color::Rgb(234, 74, 90); // Rouge

/// Largeur de l'axe Y (pour les prix)
const Y_AXIS_WIDTH: u16 = 12;

/// Constantes pour le design réactif (Bug 6)
/// CONCEPT : Responsive terminal design
/// - MIN_TERMINAL_WIDTH : largeur minimale absolue pour afficher le graphique
/// - ADAPTIVE_Y_AXIS_THRESHOLD : en dessous, on réduit la largeur de l'axe Y
/// - NARROW_Y_AXIS_WIDTH : largeur réduite de l'axe Y pour terminaux étroits
const MIN_TERMINAL_WIDTH: u16 = 80;
const ADAPTIVE_Y_AXIS_THRESHOLD: u16 = 80;
const NARROW_Y_AXIS_WIDTH: u16 = 8;

// ============================================================================
// Structure principale
// ============================================================================

/// Renderer de chandeliers japonais en mode texte
pub struct CandlestickRenderer<'a> {
    candles: &'a [OHLC],
    interval: Interval,
    ticker_type: crate::models::TickerType,
    min_price: f64,
    max_price: f64,
    height: u16,
    width: u16,
    y_axis_width: u16,
}

/// Position d'un chandelier dans le graphique
///
/// CONCEPT : Single source of truth for alignment
/// - Toutes les couches (chandeliers, ticks, labels, dates) utilisent les mêmes positions
/// - Garantit l'alignement parfait chandelier ↔ timestamp
#[derive(Debug, Clone, Copy)]
struct CandlePosition {
    /// Position absolue de la colonne (0-based depuis le début de la zone graphique)
    column: usize,
    /// Nombre de caractères alloués à ce chandelier (généralement 1)
    width: usize,
}

impl<'a> CandlestickRenderer<'a> {
    /// Crée un nouveau renderer
    ///
    /// CONCEPT : Responsive design
    /// - Adapte la largeur de l'axe Y selon la largeur du terminal
    /// - Largeur < 80 cols : axe Y réduit à 8 caractères
    /// - Largeur >= 80 cols : axe Y normal à 12 caractères
    pub fn new(
        candles: &'a [OHLC],
        interval: Interval,
        ticker_type: crate::models::TickerType,
        area: Rect,
    ) -> Self {
        // CORRECTION : Calcule les bornes de prix sur les chandeliers VISIBLES uniquement
        // Évite que des pics/creux hors de la fenêtre d'affichage n'étirent l'axe Y
        let visible = Self::get_visible_slice(candles);
        let (min_price, max_price) = Self::compute_price_bounds(visible);

        // Largeur adaptative de l'axe Y selon la largeur du terminal
        let y_axis_width = if area.width < ADAPTIVE_Y_AXIS_THRESHOLD {
            NARROW_Y_AXIS_WIDTH // Mode étroit : 8 caractères
        } else {
            Y_AXIS_WIDTH // Mode normal : 12 caractères
        };

        Self {
            candles,
            interval,
            ticker_type,
            min_price,
            max_price,
            // Réserve 3 pour header + 3 pour x-axis (ticks + labels + dates) = 6 lignes
            height: area.height.saturating_sub(6),
            width: area.width.saturating_sub(y_axis_width),
            y_axis_width,
        }
    }

    /// Calcule les prix min et max sur tous les chandeliers
    fn compute_price_bounds(candles: &[OHLC]) -> (f64, f64) {
        let max_price = candles
            .iter()
            .fold(f64::NEG_INFINITY, |max, c| max.max(c.high));

        let min_price = candles.iter().fold(f64::INFINITY, |min, c| min.min(c.low));

        // Ajoute une marge de 2%
        let margin = (max_price - min_price) * 0.02;
        ((min_price - margin).max(0.0), max_price + margin)
    }

    /// Convertit un prix en coordonnée de hauteur
    fn price_to_height(&self, price: f64) -> f64 {
        if self.max_price == self.min_price {
            return self.height as f64 / 2.0;
        }

        (price - self.min_price) / (self.max_price - self.min_price) * self.height as f64
    }

    /// Détermine si un chandelier est haussier (bullish)
    fn is_bullish(candle: &OHLC) -> bool {
        candle.close >= candle.open
    }

    /// Retourne la couleur du chandelier
    fn candle_color(candle: &OHLC) -> Color {
        if Self::is_bullish(candle) {
            BULLISH_COLOR
        } else {
            BEARISH_COLOR
        }
    }

    /// Rend un chandelier à une hauteur donnée
    ///
    /// Ceci est le cœur de l'algorithme, adapté de cli-candlestick-chart.
    /// Il détermine quel caractère Unicode afficher selon la position verticale.
    fn render_candle(&self, candle: &OHLC, y: u16) -> char {
        let height_unit = y as f64;

        // Convertit les prix en coordonnées de hauteur
        let high_y = self.price_to_height(candle.high);
        let low_y = self.price_to_height(candle.low);
        let max_y = self.price_to_height(candle.open.max(candle.close));
        let min_y = self.price_to_height(candle.close.min(candle.open));

        let mut output = UNICODE_VOID;

        // ========================================
        // ZONE 1 : Mèche supérieure (high → max)
        // ========================================
        if high_y.ceil() >= height_unit && height_unit >= max_y.floor() {
            if max_y - height_unit > 0.75 {
                // Corps s'étend significativement dans cette ligne
                output = UNICODE_BODY;
            } else if (max_y - height_unit) > 0.25 {
                // Corps partiellement présent
                if (high_y - height_unit) > 0.75 {
                    // Mèche s'étend aussi → transition
                    output = UNICODE_TOP;
                } else {
                    // Juste le corps avec espace
                    output = UNICODE_HALF_BODY_BOTTOM;
                }
            } else if (high_y - height_unit) > 0.75 {
                // Que la mèche, pleine
                output = UNICODE_WICK;
            } else if (high_y - height_unit) > 0.25 {
                // Demi-mèche
                output = UNICODE_UPPER_WICK;
            }
        }
        // ========================================
        // ZONE 2 : Corps (min → max)
        // ========================================
        else if max_y.floor() >= height_unit && height_unit >= min_y.ceil() {
            // Toujours corps plein dans la zone du corps
            output = UNICODE_BODY;
        }
        // ========================================
        // ZONE 3 : Mèche inférieure (min → low)
        // ========================================
        else if min_y.ceil() >= height_unit && height_unit >= low_y.floor() {
            if (min_y - height_unit) < 0.25 {
                // Corps encore très proche
                output = UNICODE_BODY;
            } else if (min_y - height_unit) < 0.75 {
                // Corps partiellement présent
                if (low_y - height_unit) < 0.25 {
                    // Mèche proche aussi → transition
                    output = UNICODE_BOTTOM;
                } else {
                    // Juste le corps avec espace
                    output = UNICODE_HALF_BODY_TOP;
                }
            } else if low_y - height_unit < 0.25 {
                // Que la mèche, pleine
                output = UNICODE_WICK;
            } else if low_y - height_unit < 0.75 {
                // Demi-mèche
                output = UNICODE_LOWER_WICK;
            }
        }

        output
    }

    /// Rend une ligne de l'axe Y avec le prix
    fn render_y_axis(&self, y: u16) -> String {
        // Affiche le prix tous les 4 lignes
        if y % 4 == 0 {
            let price = self.min_price
                + (y as f64 * (self.max_price - self.min_price) / self.height as f64);
            format!("{:>9.2} │ ", price)
        } else {
            format!("{:>9} │ ", "")
        }
    }

    /// Fonction helper : extrait les chandeliers visibles (les ~250 derniers)
    fn get_visible_slice(candles: &[OHLC]) -> &[OHLC] {
        const MAX_VISIBLE_CANDLES: usize = 250;

        if candles.len() <= MAX_VISIBLE_CANDLES {
            candles
        } else {
            &candles[candles.len() - MAX_VISIBLE_CANDLES..]
        }
    }

    /// Sélectionne les chandeliers visibles (les ~250 derniers pour cohérence visuelle)
    fn visible_candles(&self) -> &[OHLC] {
        // CONCEPT : Limite d'affichage à ~200-300 chandeliers
        // - On requête plus de données (pour avoir assez pour les actions)
        // - Mais on affiche seulement les ~250 derniers (cohérence visuelle)
        // - Fonctionne pour crypto (24h/24) ET actions (6.5h/jour)
        Self::get_visible_slice(self.candles)
    }

    /// Pré-calcule les positions exactes de chaque chandelier
    ///
    /// CONCEPT : Accumulator pattern pour éviter le drift
    /// - Chaque position = index × spacing (pas position_précédente + spacing)
    /// - Évite l'accumulation d'erreurs d'arrondi
    /// - Garantit que chandeliers et labels utilisent les mêmes positions
    ///
    /// Cas gérés :
    /// - Terminal trop étroit : 1 chandelier par colonne (spacing ≈ 1.0)
    /// - Terminal trop large : chandeliers répartis uniformément (spacing > 1.0)
    /// - Spacing fractionnaire : accumulator évite le drift
    /// - Chandelier unique : centré dans la largeur disponible
    fn compute_candle_positions(chart_width: usize, num_candles: usize) -> Vec<CandlePosition> {
        if num_candles == 0 {
            return Vec::new();
        }

        if num_candles == 1 {
            // Cas spécial : chandelier unique centré
            return vec![CandlePosition {
                column: chart_width / 2,
                width: 1,
            }];
        }

        let mut positions = Vec::with_capacity(num_candles);
        let spacing = chart_width as f64 / num_candles as f64;

        for i in 0..num_candles {
            // Pattern accumulator : calcul depuis l'index, pas depuis la position précédente
            // Cela évite l'accumulation d'erreurs d'arrondi sur plusieurs chandeliers
            let exact_position = i as f64 * spacing;
            let column = exact_position.round() as usize;

            positions.push(CandlePosition {
                column: column.min(chart_width.saturating_sub(1)),
                width: 1,
            });
        }

        positions
    }

    /// Génère toutes les lignes du graphique (chandeliers + axe X)
    ///
    /// CONCEPT : Position array pour alignement parfait
    /// - Pré-calcule toutes les positions avec compute_candle_positions()
    /// - Construit chaque ligne avec un tableau de caractères
    /// - Place les chandeliers exactement aux positions calculées
    /// - Utilise les MÊMES positions pour l'axe X → alignement garanti
    pub fn render_lines(&self) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let visible = self.visible_candles();

        if visible.is_empty() {
            return lines;
        }

        // Pré-calcule les positions de tous les chandeliers (source unique de vérité)
        let positions = Self::compute_candle_positions(self.width as usize, visible.len());

        // Parcourt de haut en bas (reversed)
        for y in (1..=self.height).rev() {
            let mut spans = Vec::new();

            // Ajoute l'axe Y
            spans.push(Span::styled(
                self.render_y_axis(y),
                Style::default().fg(Color::Gray),
            ));

            // Construit la ligne avec un tableau de caractères
            let mut line_chars = vec![' '; self.width as usize];
            let mut line_colors: Vec<Option<Color>> = vec![None; self.width as usize];

            // Place chaque chandelier à sa position exacte
            for (candle, pos) in visible.iter().zip(positions.iter()) {
                if pos.column < line_chars.len() {
                    line_chars[pos.column] = self.render_candle(candle, y);
                    line_colors[pos.column] = Some(Self::candle_color(candle));
                }
            }

            // Convertit le tableau de caractères en spans avec couleurs
            let mut current_color = line_colors[0];
            let mut current_string = String::new();
            current_string.push(line_chars[0]);

            for i in 1..line_chars.len() {
                if line_colors[i] == current_color {
                    // Continue le span actuel
                    current_string.push(line_chars[i]);
                } else {
                    // Émet le span actuel et commence un nouveau
                    if let Some(color) = current_color {
                        spans.push(Span::styled(
                            current_string.clone(),
                            Style::default().fg(color),
                        ));
                    } else {
                        spans.push(Span::raw(current_string.clone()));
                    }

                    current_string.clear();
                    current_string.push(line_chars[i]);
                    current_color = line_colors[i];
                }
            }

            // Émet le dernier span
            if let Some(color) = current_color {
                spans.push(Span::styled(current_string, Style::default().fg(color)));
            } else {
                spans.push(Span::raw(current_string));
            }

            lines.push(Line::from(spans));
        }

        // Ajoute l'axe X en passant les positions (pas spacing)
        lines.extend(self.render_x_axis(visible, &positions));

        lines
    }

    /// Détermine si une chandelle doit avoir un label selon la stratégie
    fn should_show_label(
        candle: &OHLC,
        prev_candle: Option<&OHLC>,
        strategy: LabelStrategy,
    ) -> bool {
        match strategy {
            LabelStrategy::RoundHours { interval_hours } => {
                // Affiche si l'heure est un multiple de interval_hours
                candle.timestamp.hour() % interval_hours == 0 && candle.timestamp.minute() == 0
            }
            LabelStrategy::DayChanges => {
                // Affiche si changement de jour
                if let Some(prev) = prev_candle {
                    candle.timestamp.date_naive() != prev.timestamp.date_naive()
                } else {
                    false // Première chandelle
                }
            }
            LabelStrategy::RegularDays { interval_days } => {
                // Affiche si jour est multiple de interval_days depuis la dernière chandelle
                if let Some(prev) = prev_candle {
                    let days_diff = (candle.timestamp.date_naive() - prev.timestamp.date_naive())
                        .num_days()
                        .abs();
                    days_diff >= interval_days as i64
                } else {
                    false // Première chandelle
                }
            }
            LabelStrategy::RegularWeeks { interval_days } => {
                // Affiche si le jour est multiple de interval_days depuis la dernière chandelle
                if let Some(prev) = prev_candle {
                    let days_diff = (candle.timestamp.date_naive() - prev.timestamp.date_naive())
                        .num_days()
                        .abs();
                    days_diff >= interval_days as i64
                } else {
                    false // Première chandelle
                }
            }
            LabelStrategy::RegularMonths { interval_months } => {
                // Affiche si le jour est multiple de interval_months depuis la dernière chandelle
                if let Some(prev) = prev_candle {
                    let months_diff = (candle.timestamp.year() - prev.timestamp.year()) * 12
                        + (candle.timestamp.month() as i32 - prev.timestamp.month() as i32);
                    months_diff.abs() >= interval_months as i32
                } else {
                    false // Première chandelle
                }
            }
            LabelStrategy::RegularYears { interval_years } => {
                // Affiche si le jour est multiple de interval_years depuis la dernière chandelle
                if let Some(prev) = prev_candle {
                    let years_diff = candle.timestamp.year() - prev.timestamp.year();
                    years_diff.abs() >= interval_years as i32
                } else {
                    false // Première chandelle
                }
            }
        }
    }

    /// Vérifie si un label centré sur une position rentre complètement dans la largeur disponible
    ///
    /// Paramètres:
    /// - center_pos: position centrale du label (colonne du chandelier)
    /// - label_len: longueur du label en caractères
    ///
    /// Retourne true si le label peut être affiché entièrement sans troncature
    fn label_fits(&self, center_pos: usize, label_len: usize) -> bool {
        let label_start = center_pos.saturating_sub(label_len / 2);
        let label_end = label_start + label_len;
        label_end <= self.width as usize
    }

    /// Génère les lignes de l'axe X avec tick marks et labels harmonisés
    ///
    /// CONCEPT : Structure uniformisée à 3 lignes
    /// - Ligne 1 : Tick marks (│)
    /// - Ligne 2 : Heures (HH:MM) pour intraday OU vide pour D1/W1
    /// - Ligne 3 : Dates (DD/MM ou DD/MM/YYYY) pour TOUS les intervalles
    ///
    /// HARMONISATION :
    /// - Séparation claire heures/dates
    /// - Format de date uniforme
    /// - Année affichée automatiquement si données multi-années
    fn render_x_axis(&self, visible: &[OHLC], positions: &[CandlePosition]) -> Vec<Line<'a>> {
        let mut lines = vec![];
        let axis_formats = self.interval.x_axis_format(&self.ticker_type);
        let label_strategy = axis_formats.label_strategy;

        // Détecte si le terminal est étroit et ajuste la stratégie
        // TODO: Ajuster avec tests empiriques
        // - Seuil actuel: 80 cols
        // - Multiplicateur actuel: x2
        // - À tester: seuils différents par intervalle? (50 pour M5, 80 pour D1, etc.)
        let is_narrow = self.width < 140;
        let adjusted_strategy = if is_narrow {
            match label_strategy {
                LabelStrategy::RoundHours { interval_hours } => {
                    // Double l'intervalle si étroit
                    LabelStrategy::RoundHours {
                        interval_hours: interval_hours * 2,
                    }
                }
                LabelStrategy::RegularDays { interval_days } => LabelStrategy::RegularDays {
                    interval_days: interval_days * 2,
                },
                // DayChanges et Weeks: pas d'ajustement
                other => other,
            }
        } else {
            label_strategy
        };

        let date_format = { axis_formats.date_format };

        // ========================================
        // Ligne 1 : Tick marks │ (alternance gris/jaune)
        // ========================================
        let mut tick_spans = vec![Span::raw(format!(
            "{:>width$}",
            "",
            width = self.y_axis_width as usize
        ))];
        let mut prev_candle = None;
        let mut label_count = 0;
        let mut last_pos = 0;

        for (candle, pos) in visible.iter().zip(positions.iter()) {
            if Self::should_show_label(candle, prev_candle, adjusted_strategy)
                && pos.column < self.width as usize
            {
                // Ajoute les espaces avant le tick
                if pos.column > last_pos {
                    tick_spans.push(Span::raw(" ".repeat(pos.column - last_pos)));
                }

                // Alterne gris/jaune
                let color = if label_count % 2 == 0 {
                    Color::Gray
                } else {
                    Color::Yellow
                };

                tick_spans.push(Span::styled("│", Style::default().fg(color)));
                last_pos = pos.column + 1;
                label_count += 1;
            }
            prev_candle = Some(candle);
        }

        // Ajoute les espaces restants
        if last_pos < self.width as usize {
            tick_spans.push(Span::raw(" ".repeat(self.width as usize - last_pos)));
        }

        lines.push(Line::from(tick_spans));

        // ========================================
        // Ligne 2 : Heures (HH:MM) ou vide
        // ========================================
        if let Some(time_fmt) = axis_formats.time_format {
            // Intraday : afficher les heures (alternance gris/jaune)
            let mut time_line = vec![' '; self.width as usize];
            let mut time_colors = vec![None; self.width as usize]; // Trace la couleur par position
            let mut prev_candle = None;
            let mut label_count = 0;

            for (candle, pos) in visible.iter().zip(positions.iter()) {
                if Self::should_show_label(candle, prev_candle, adjusted_strategy) {
                    let time_label = candle.timestamp.format(time_fmt).to_string();

                    // Vérifie que le label rentre complètement (pas de troncature au bord)
                    if !self.label_fits(pos.column, time_label.len()) {
                        prev_candle = Some(candle);
                        continue;
                    }

                    // Centre le label sur la position du chandelier
                    let label_start = pos.column.saturating_sub(time_label.len() / 2);
                    let label_end = (label_start + time_label.len()).min(time_line.len());

                    // Alterne gris/jaune
                    let color = if label_count % 2 == 0 {
                        Color::Gray
                    } else {
                        Color::Yellow
                    };

                    // Place le label caractère par caractère avec sa couleur
                    for (j, ch) in time_label.chars().enumerate() {
                        let idx = label_start + j;
                        if idx < label_end {
                            time_line[idx] = ch;
                            time_colors[idx] = Some(color);
                        }
                    }
                    label_count += 1;
                }
                prev_candle = Some(candle);
            }

            // Construit les spans avec alternance de couleurs
            let mut time_spans = vec![Span::raw(format!(
                "{:>width$}",
                "",
                width = self.y_axis_width as usize
            ))];
            let mut current_text = String::new();
            let mut current_color = None;

            for (i, &ch) in time_line.iter().enumerate() {
                let char_color = time_colors[i];

                if char_color != current_color {
                    // Change de couleur : flush le span précédent
                    if !current_text.is_empty() {
                        if let Some(color) = current_color {
                            time_spans.push(Span::styled(
                                current_text.clone(),
                                Style::default().fg(color),
                            ));
                        } else {
                            time_spans.push(Span::raw(current_text.clone()));
                        }
                        current_text.clear();
                    }
                    current_color = char_color;
                }
                current_text.push(ch);
            }

            // Flush le dernier span
            if !current_text.is_empty() {
                if let Some(color) = current_color {
                    time_spans.push(Span::styled(current_text, Style::default().fg(color)));
                } else {
                    time_spans.push(Span::raw(current_text));
                }
            }

            lines.push(Line::from(time_spans));
        } else {
            // D1/W1 : ligne vide
            let empty_spans = vec![Span::raw(format!(
                "{:>width$}",
                "",
                width = (self.y_axis_width + self.width) as usize
            ))];
            lines.push(Line::from(empty_spans));
        }

        // ========================================
        // Ligne 3 : Dates (DD/MM, Month or YYYY) (alternance gris/jaune)
        // ========================================
        let mut date_line = vec![' '; self.width as usize];
        let mut date_colors = vec![None; self.width as usize]; // Trace la couleur par position
        let mut prev_candle: Option<&OHLC> = None;

        // Pour la ligne des dates, toujours utiliser DayChanges si RoundHours
        // Sinon conserver la stratégie choisie
        let date_strategy = match label_strategy {
            LabelStrategy::RoundHours { .. } => LabelStrategy::DayChanges,
            other => other,
        };

        let mut label_count = 0;

        for (candle, pos) in visible.iter().zip(positions.iter()) {
            if Self::should_show_label(candle, prev_candle, date_strategy) {
                let date_label = candle.timestamp.format(date_format).to_string();

                // Vérifie que le label rentre complètement (pas de troncature au bord)
                if !self.label_fits(pos.column, date_label.len()) {
                    prev_candle = Some(candle);
                    continue;
                }

                // Centre la date sur la position du chandelier
                let date_start = pos.column.saturating_sub(date_label.len() / 2);
                let date_end = (date_start + date_label.len()).min(date_line.len());

                // Vérifie qu'on n'écrase pas une date déjà placée
                let has_overlap = (date_start..date_end).any(|idx| date_line[idx] != ' ');

                if !has_overlap {
                    // Alterne gris/jaune
                    let color = if label_count % 2 == 0 {
                        Color::Gray
                    } else {
                        Color::Yellow
                    };

                    for (j, ch) in date_label.chars().enumerate() {
                        let idx = date_start + j;
                        if idx < date_end {
                            date_line[idx] = ch;
                            date_colors[idx] = Some(color);
                        }
                    }
                    label_count += 1;
                }
            }

            prev_candle = Some(candle);
        }

        // Construit les spans avec alternance de couleurs
        let mut date_spans = vec![Span::raw(format!(
            "{:>width$}",
            "",
            width = self.y_axis_width as usize
        ))];
        let mut current_text = String::new();
        let mut current_color = None;

        for (i, &ch) in date_line.iter().enumerate() {
            let char_color = date_colors[i];

            if char_color != current_color {
                // Change de couleur : flush le span précédent
                if !current_text.is_empty() {
                    if let Some(color) = current_color {
                        date_spans.push(Span::styled(
                            current_text.clone(),
                            Style::default().fg(color),
                        ));
                    } else {
                        date_spans.push(Span::raw(current_text.clone()));
                    }
                    current_text.clear();
                }
                current_color = char_color;
            }
            current_text.push(ch);
        }

        // Flush le dernier span
        if !current_text.is_empty() {
            if let Some(color) = current_color {
                date_spans.push(Span::styled(current_text, Style::default().fg(color)));
            } else {
                date_spans.push(Span::raw(current_text));
            }
        }

        lines.push(Line::from(date_spans));

        lines
    }
}

// ============================================================================
// Fonction principale de rendu
// ============================================================================

/// Dessine un graphique en chandeliers japonais pour le ticker sélectionné
pub fn render_candlestick_chart(frame: &mut Frame, app: &App, area: Rect) {
    // Récupère le ticker sélectionné
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

    if data.candles.is_empty() {
        render_no_data(frame, area, "Pas de données à afficher");
        return;
    }

    // Vérifie si le terminal est assez large pour afficher le graphique
    // CONCEPT : Graceful degradation pour terminaux étroits
    if area.width < MIN_TERMINAL_WIDTH {
        render_too_narrow(frame, area);
        return;
    }

    // Crée le layout : header + graphique
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Graphique
        ])
        .split(area)
        .to_vec();

    // Dessine le header
    render_header(frame, app, item, chunks[0]);

    // Crée le renderer et génère les lignes
    let renderer = CandlestickRenderer::new(
        &data.candles,
        data.interval,
        crate::models::TickerType::from_symbol(&data.symbol),
        chunks[1],
    );
    let lines = renderer.render_lines();

    // Crée le widget Paragraph avec les lignes
    // Note : data.interval = interval des données chargées
    //        app.current_interval = interval sélectionné par l'utilisateur
    let displayed_interval = app.current_interval.label();
    let data_interval = data.interval.label();

    // Indicateur si l'intervalle sélectionné diffère des données chargées
    let interval_display = if displayed_interval != data_interval {
        format!("{} → {} ⚠️ ", data_interval, displayed_interval)
    } else {
        format!("{} ", displayed_interval)
    };

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White))
            .title(format!(
                " 🕯️ {} - {}({}, {} chandeliers) [h/l: changer interval] ",
                item.symbol,
                interval_display,
                data.interval.label(),
                data.candles.len()
            )),
    );

    frame.render_widget(paragraph, chunks[1]);
}

// ============================================================================
// Header
// ============================================================================

/// Dessine le header avec infos du ticker
fn render_header(frame: &mut Frame, app: &App, item: &crate::models::WatchlistItem, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" 🕯️ {} - {} ", item.symbol, item.name));

    // CONCEPT : Confirmation de quit two-step et loading indicator
    // - Si app.is_awaiting_quit_confirmation(), affiche message d'avertissement
    // - Si app.is_loading(), affiche indicateur de chargement
    // - Sinon, affiche les infos normales avec shortcuts
    let text = if app.is_awaiting_quit_confirmation() {
        // Message de confirmation de quit
        vec![Line::from(vec![
            Span::styled(
                "⚠  Appuyez sur ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
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
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ])]
    } else if app.is_loading() {
        // Indicateur de chargement
        let message = format!("Chargement en cours ({})...", app.pending);
        vec![Line::from(vec![
            Span::styled(
                "⏳ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                message,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])]
    } else if let (Some(price), Some(change)) = (item.current_price(), item.change_percent()) {
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
            Span::raw(" Retour  "),
            Span::styled(
                "[q]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Quitter"),
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
// Helper : Message d'erreur
// ============================================================================

/// Affiche un message quand il n'y a pas de données
fn render_no_data(frame: &mut Frame, area: Rect, message: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(" ⚠ Erreur ");

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(Color::Red))),
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

/// Affiche un message quand le terminal est trop étroit
///
/// CONCEPT : Responsive design - graceful degradation
/// - Prévient les problèmes d'affichage sur terminaux très étroits
/// - Informe clairement l'utilisateur de la largeur minimale requise
fn render_too_narrow(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" ⚠ Terminal trop petit ");

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Terminal trop étroit pour afficher le graphique",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Largeur minimale requise : {} colonnes", MIN_TERMINAL_WIDTH),
            Style::default().fg(Color::Gray),
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
// Notes d'implémentation
// ============================================================================
//
// ALGORITHME INSPIRÉ DE : cli-candlestick-chart
// Source : https://github.com/Julien-R44/cli-candlestick-chart
//
// PRINCIPE :
// - Rendu ligne par ligne de haut en bas (reversed)
// - 3 zones : mèche sup, corps, mèche inf
// - Seuils 0.25 et 0.75 pour sub-caractère précision
// - Caractères Unicode box-drawing pour rendu professionnel
//
// AVANTAGES :
// ✓ Rendu professionnel identique à cli-candlestick-chart
// ✓ Intégration native ratatui (Paragraph + Line + Span)
// ✓ Pas de bugs externes
// ✓ Code maîtrisé et extensible
// ✓ Performant : O(hauteur × nb_chandeliers)
//
// AMÉLIORATIONS POSSIBLES :
// - Ajouter volume en sous-graphique
// - Indicateurs techniques (MA, RSI, Bollinger, etc.)
// - Zoom et navigation horizontale
// - Curseur pour afficher OHLC au survol
//
// ============================================================================
