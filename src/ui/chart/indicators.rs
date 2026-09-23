// ============================================================================
// Indicateurs techniques : moyennes mobiles simples
// ============================================================================

/// Moyenne mobile simple sur `period` valeurs
///
/// `None` tant que `period` valeurs ne sont pas disponibles : on n'affiche
/// jamais une moyenne partielle sous le nom de MA50.
///
/// ponytail: somme recalculée par fenêtre, O(n × période) ; ~2 000 × 50
/// opérations par rendu. Somme glissante si des périodes longues le justifient.
pub fn sma(values: &[f64], period: usize) -> Vec<Option<f64>> {
    if period == 0 || values.len() < period {
        return vec![None; values.len()];
    }
    #[allow(clippy::cast_precision_loss)] // période de quelques dizaines
    let divisor = period as f64;
    std::iter::repeat_n(None, period - 1)
        .chain(
            values
                .windows(period)
                .map(|window| Some(window.iter().sum::<f64>() / divisor)),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sma_waits_for_a_full_period() {
        let closes = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(
            sma(&closes, 3),
            [None, None, Some(2.0), Some(3.0), Some(4.0)]
        );
        assert_eq!(sma(&closes, 6), [None; 5], "pas de moyenne partielle");
        assert!(sma(&[], 20).is_empty());
    }

    #[test]
    fn sma_does_not_drift_on_long_series() {
        let closes: Vec<f64> = (0..10_000)
            .map(|i| f64::from(i % 3) * 0.1 + 1_000_000.0)
            .collect();
        let last = sma(&closes, 3).last().copied().flatten().unwrap();
        assert!((last - 1_000_000.1).abs() < 1e-6, "{last}");
    }
}
