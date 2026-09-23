// ============================================================================
// Sparkline : mini-courbe en caractères blocs (▁▂▃▄▅▆▇█)
// ============================================================================

// CONCEPT : 8 niveaux par caractère. La série est rééchantillonnée à la
// largeur voulue (jamais étirée si elle est plus courte), puis chaque valeur
// est placée entre le min et le max de la série.
// ============================================================================

use crate::models::{Interval, OHLCData};

/// Blocs de hauteur croissante (1/8 à 8/8 de cellule)
pub(crate) const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Nombre de chandelles journalières/hebdo gardées pour la tendance
const LONG_TREND_CANDLES: usize = 60;

/// Mini-courbe d'au plus `width` caractères
pub fn sparkline(values: &[f64], width: usize) -> String {
    let count = values.len().min(width);
    if count == 0 {
        return String::new();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (0..count)
        .map(|i| {
            // Échantillon i parmi count, répartis du premier au dernier point
            let index = if count == 1 {
                values.len() - 1
            } else {
                i * (values.len() - 1) / (count - 1)
            };
            let level = if max > min {
                (values[index] - min) / (max - min) * 7.0
            } else {
                3.0 // série plate : niveau médian
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0.0..=7.0
            let level = level.round() as usize;
            LEVELS[level.min(7)]
        })
        .collect()
}

/// Clôtures utilisées pour la tendance d'un ticker
///
/// En intraday : la dernière séance (à l'heure de la place), comme la variation
/// du jour. En journalier/hebdo : les dernières chandelles.
pub fn trend_closes(data: &OHLCData) -> Vec<f64> {
    if matches!(data.interval, Interval::D1 | Interval::W1) {
        let start = data.candles.len().saturating_sub(LONG_TREND_CANDLES);
        return data.candles[start..].iter().map(|c| c.close).collect();
    }
    let Some(last) = data.last() else {
        return Vec::new();
    };
    let session = data.local_time(last).date_naive();
    data.candles
        .iter()
        .filter(|c| data.local_time(c).date_naive() == session)
        .map(|c| c.close)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rising_series_goes_from_low_to_high() {
        let line = sparkline(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 8);
        assert_eq!(line, "▁▂▃▄▅▆▇█");
    }

    #[test]
    fn long_series_is_resampled_to_width() {
        let values: Vec<f64> = (0..100).map(f64::from).collect();
        let line = sparkline(&values, 10);
        assert_eq!(line.chars().count(), 10);
        assert!(line.starts_with('▁') && line.ends_with('█'), "{line}");
    }

    #[test]
    fn short_flat_and_empty_series() {
        assert_eq!(sparkline(&[5.0, 5.0, 5.0], 3), "▄▄▄");
        assert_eq!(
            sparkline(&[1.0, 2.0], 10).chars().count(),
            2,
            "pas d'étirement"
        );
        assert_eq!(sparkline(&[], 10), "");
        assert_eq!(sparkline(&[1.0, 2.0], 0), "");
    }

    #[test]
    fn intraday_trend_uses_the_last_session_only() {
        use crate::models::{Interval, OHLC};
        use chrono::{FixedOffset, TimeZone};
        let ny = FixedOffset::west_opt(4 * 3600).unwrap();
        let mut data = OHLCData::new("AAPL".into(), Interval::M30, ny);
        for (day, hour, close) in [(21, 10, 1.0), (21, 15, 2.0), (22, 10, 3.0), (22, 15, 4.0)] {
            let t = ny
                .with_ymd_and_hms(2026, 9, day, hour, 0, 0)
                .unwrap()
                .to_utc();
            data.add_candle(OHLC::new(t, close, close, close, close, 0));
        }
        assert_eq!(trend_closes(&data), [3.0, 4.0]);
        data.interval = Interval::D1;
        assert_eq!(trend_closes(&data), [1.0, 2.0, 3.0, 4.0]);
    }
}
