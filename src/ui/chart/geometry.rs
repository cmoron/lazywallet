// ============================================================================
// Géométrie du graphique : combien de chandelles, à quelles colonnes
// ============================================================================
// CONCEPT : Densité fixe. Plus le terminal est large, plus on remonte dans le
// temps ; on n'étire jamais les chandelles. Les plus récentes collent à droite,
// contre l'axe des prix.
// ============================================================================

/// Lignes sous le graphique : trait + labels fins + contexte
pub const AXIS_ROWS: u16 = 3;

/// En dessous de ce nombre de chandelles visibles, on les serre (1 colonne chacune)
const WIDE_MIN_CANDLES: u16 = 60;

/// Colonnes par chandelle : 2 (chandelle + espace) si on en voit au moins 60 ainsi, sinon 1
pub fn slot_width(plot_width: u16) -> u16 {
    if plot_width >= 2 * WIDE_MIN_CANDLES {
        2
    } else {
        1
    }
}

/// Chandelles visibles : (index de la première, colonne de chacune)
///
/// Les plus récentes sont gardées et alignées à droite.
pub fn visible_columns(plot_width: u16, total: usize) -> (usize, Vec<u16>) {
    let slot = slot_width(plot_width);
    let fit = usize::from(plot_width / slot).min(total);
    let first = total - fit;
    // fit ≤ plot_width / slot : la conversion ne peut pas échouer
    let fit = u16::try_from(fit).unwrap_or(0);
    let offset = plot_width - fit * slot;
    (first, (0..fit).map(|i| offset + i * slot).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_plot_uses_gaps_and_shows_more_history() {
        assert_eq!(slot_width(119), 1);
        assert_eq!(slot_width(120), 2);
        let (first, cols) = visible_columns(200, 1_000);
        assert_eq!((first, cols.len()), (900, 100));
        assert_eq!(*cols.last().unwrap(), 198); // colonne vide avant l'axe
    }

    #[test]
    fn narrow_plot_is_one_column_per_candle() {
        let (first, cols) = visible_columns(50, 1_000);
        assert_eq!((first, cols.len(), cols[0], cols[49]), (950, 50, 0, 49));
    }

    #[test]
    fn few_candles_are_right_aligned_not_stretched() {
        let (first, cols) = visible_columns(200, 10);
        assert_eq!(first, 0);
        assert_eq!(cols, (0..10).map(|i| 180 + 2 * i).collect::<Vec<u16>>());
    }

    #[test]
    fn zero_width_or_no_candles() {
        assert_eq!(visible_columns(0, 10), (10, vec![]));
        assert_eq!(visible_columns(80, 0), (0, vec![]));
    }
}
