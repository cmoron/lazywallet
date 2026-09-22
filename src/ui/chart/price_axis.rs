// ============================================================================
// Axe des prix : graduations "rondes" (1, 2, 5 × 10^k)
// ============================================================================

/// Une graduation toutes les 4 lignes environ
pub const ROWS_PER_TICK: u16 = 4;

/// Plus petit pas rond ≥ `raw` (1, 2, 5 ou 10 × une puissance de 10)
pub fn nice_step(raw: f64) -> f64 {
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

/// Décimales nécessaires pour écrire les multiples de `step` sans perte
///
/// CONCEPT : On teste 10^d × step plutôt que -log10(step) : les flottants
/// (0.1 n'est pas exact en binaire) feraient tomber log10 du mauvais côté.
pub fn decimals_for(step: f64) -> usize {
    (0..8usize)
        .find(|&d| {
            let scaled = step * 10f64.powi(i32::try_from(d).unwrap_or(0));
            (scaled - scaled.round()).abs() < 1e-6 * scaled.max(1.0)
        })
        .unwrap_or(8)
}

/// Graduations de l'axe des prix et leur format commun
#[derive(Debug, Clone, PartialEq)]
pub struct PriceTicks {
    pub step: f64,
    pub decimals: usize,
    pub values: Vec<f64>,
}

impl PriceTicks {
    /// Formate un prix avec la précision des graduations
    pub fn format(&self, price: f64) -> String {
        format!("{price:.prec$}", prec = self.decimals)
    }
}

/// Graduations rondes dans [min, max], environ une toutes les `ROWS_PER_TICK` lignes
///
/// Suppose `min < max` (le widget élargit une série plate avant d'appeler).
pub fn price_ticks(min: f64, max: f64, rows: u16) -> PriceTicks {
    let wanted = (rows / ROWS_PER_TICK).max(1);
    let step = nice_step((max - min) / f64::from(wanted));
    // Multiples entiers du pas : pas d'accumulation d'erreurs d'arrondi
    #[allow(clippy::cast_possible_truncation)] // prix / pas : très loin des bornes d'i64
    let (first, last) = ((min / step).ceil() as i64, (max / step).floor() as i64);
    #[allow(clippy::cast_precision_loss)] // quelques dizaines de graduations
    let values = (first..=last).map(|k| k as f64 * step).collect();
    PriceTicks {
        step,
        decimals: decimals_for(step),
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_steps() {
        for (raw, expected) in [
            (0.7, 1.0),
            (1.3, 2.0),
            (3.0, 5.0),
            (7.0, 10.0),
            (0.013, 0.02),
            (2.88, 5.0),
            (1_234.0, 2_000.0),
        ] {
            assert!(
                (nice_step(raw) - expected).abs() < 1e-9,
                "{raw} → {}",
                nice_step(raw)
            );
        }
    }

    #[test]
    fn decimals_follow_step() {
        assert_eq!(decimals_for(5.0), 0);
        assert_eq!(decimals_for(0.1), 1);
        assert_eq!(decimals_for(0.02), 2);
        assert_eq!(decimals_for(0.0005), 4);
    }

    #[test]
    fn ticks_are_round_and_inside_range() {
        let ticks = price_ticks(98.3, 112.7, 20);
        assert_eq!(ticks.values, [100.0, 105.0, 110.0]);
        assert_eq!(ticks.format(105.0), "105");
        let fx = price_ticks(1.1402, 1.1466, 16);
        assert_eq!(fx.decimals, 3);
        assert!(fx.values.iter().all(|v| (1.1402..=1.1466).contains(v)));
    }

    #[test]
    fn single_row_still_gives_a_step() {
        let ticks = price_ticks(10.0, 11.0, 1);
        assert!(ticks.step > 0.0 && !ticks.values.is_empty());
    }
}
