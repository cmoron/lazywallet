// ============================================================================
// Portefeuille : totaux par devise
// ============================================================================

// CONCEPT : Pas de conversion de devises — chaque devise a son propre total.
// Exact et sans dépendre de taux de change.
// ============================================================================

use crate::models::WatchlistItem;

/// Totaux d'une devise
#[derive(Debug, Clone, PartialEq)]
pub struct CurrencyTotal {
    pub currency: String,
    /// Valeur au cours actuel
    pub value: f64,
    /// Coût d'acquisition
    pub cost: f64,
    /// Gain ou perte du jour
    pub day_pnl: f64,
}

impl CurrencyTotal {
    /// Plus-value latente
    pub fn pnl(&self) -> f64 {
        self.value - self.cost
    }

    /// Plus-value latente en % du coût (0 si le coût est nul)
    pub fn pnl_percent(&self) -> f64 {
        if self.cost == 0.0 {
            0.0
        } else {
            self.pnl() / self.cost * 100.0
        }
    }
}

/// Totaux par devise, triés par devise
///
/// Seuls les items avec une position ET un cours comptent : une position
/// pas encore cotée fausserait la valeur totale.
pub fn totals(items: &[WatchlistItem]) -> Vec<CurrencyTotal> {
    let mut totals: Vec<CurrencyTotal> = Vec::new();
    for item in items {
        let (Some(value), Some(cost)) = (item.market_value(), item.cost_basis()) else {
            continue;
        };
        let currency = item.currency.clone().unwrap_or_else(|| "?".to_string());
        let day_pnl = item.day_pnl().unwrap_or(0.0);
        match totals.iter_mut().find(|t| t.currency == currency) {
            Some(total) => {
                total.value += value;
                total.cost += cost;
                total.day_pnl += day_pnl;
            }
            None => totals.push(CurrencyTotal {
                currency,
                value,
                cost,
                day_pnl,
            }),
        }
    }
    totals.sort_by(|a, b| a.currency.cmp(&b.currency));
    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Position, Quote, WatchlistItem};

    fn item(
        symbol: &str,
        currency: Option<&str>,
        price: Option<f64>,
        position: Option<(f64, f64)>,
    ) -> WatchlistItem {
        let mut item = WatchlistItem::new(symbol.into());
        item.currency = currency.map(String::from);
        item.quote = price.map(|price| Quote {
            price,
            previous_close: Some(price - 1.0),
        });
        item.position = position.map(|(quantity, unit_cost)| Position {
            quantity,
            unit_cost,
        });
        item
    }

    #[test]
    fn totals_are_grouped_by_currency() {
        let items = [
            item("AAPL", Some("USD"), Some(110.0), Some((10.0, 100.0))),
            item("TSLA", Some("USD"), Some(200.0), Some((1.0, 250.0))),
            item("AI.PA", Some("EUR"), Some(50.0), Some((4.0, 40.0))),
            item("BTC-USD", Some("USD"), None, Some((1.0, 60_000.0))), // pas encore coté
            item("QQQ", Some("USD"), Some(500.0), None),               // simple suivi
        ];
        let totals = totals(&items);
        assert_eq!(
            totals,
            [
                CurrencyTotal {
                    currency: "EUR".into(),
                    value: 200.0,
                    cost: 160.0,
                    day_pnl: 4.0
                },
                CurrencyTotal {
                    currency: "USD".into(),
                    value: 1300.0,
                    cost: 1250.0,
                    day_pnl: 11.0
                },
            ]
        );
        assert!((totals[1].pnl_percent() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn no_position_no_total() {
        assert!(totals(&[item("QQQ", Some("USD"), Some(1.0), None)]).is_empty());
    }
}
