// ============================================================================
// Structure : WatchlistItem
// ============================================================================
// Représente un item dans la watchlist avec ses données chargées
//
// CONCEPTS RUST :
// 1. Composition : WatchlistItem contient OHLCData et une Quote
// 2. Option : gérer les données manquantes (pas encore chargées)
// 3. Types Copy : Quote est petite, on la copie au lieu de l'emprunter
// ============================================================================

use crate::models::OHLCData;

/// Prix courant et clôture de référence pour la variation du jour
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quote {
    pub price: f64,
    pub previous_close: Option<f64>,
}

impl Quote {
    /// Variation depuis la clôture de la séance précédente, en %
    pub fn change_percent(&self) -> Option<f64> {
        let previous = self.previous_close.filter(|p| *p != 0.0)?;
        Some((self.price - previous) / previous * 100.0)
    }
}

/// Position détenue sur un ticker
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// Quantité détenue (fractionnaire pour la crypto)
    pub quantity: f64,
    /// Prix de revient unitaire, dans la devise du ticker
    pub unit_cost: f64,
}

/// Tout ce qu'un chargement Yahoo rapporte pour un ticker
#[derive(Debug, Clone)]
pub struct FetchedTicker {
    pub data: OHLCData,
    pub long_name: Option<String>,
    pub quote: Quote,
    pub currency: Option<String>,
    /// Décimales conseillées par Yahoo (`priceHint` : 2 pour une action, 4 pour du forex)
    pub price_decimals: usize,
}

/// Un ticker dans la watchlist avec ses données
#[derive(Debug, Clone)]
pub struct WatchlistItem {
    /// Symbole du ticker (ex: "AAPL")
    pub symbol: String,

    /// Nom complet (ex: "Apple Inc."), le symbole tant que rien n'est chargé
    pub name: String,

    /// Chandelles du dernier intervalle chargé (None si pas encore chargées)
    pub data: Option<OHLCData>,

    /// Prix et clôture de la veille, indépendants de l'intervalle du graphique
    pub quote: Option<Quote>,

    /// Devise de cotation (ex: "USD")
    pub currency: Option<String>,

    /// Nombre de décimales pour afficher le prix
    pub price_decimals: usize,

    /// Position détenue (None = simple suivi)
    pub position: Option<Position>,
}

impl WatchlistItem {
    /// Nouvel item sans données : le nom affiché est le symbole jusqu'au premier chargement
    pub fn new(symbol: String) -> Self {
        Self {
            name: symbol.clone(),
            symbol,
            data: None,
            quote: None,
            currency: None,
            price_decimals: 2,
            position: None,
        }
    }

    /// Intègre un chargement
    ///
    /// CONCEPT : La clôture de référence survit à un chargement qui ne sait pas la calculer
    /// (W1), sinon la variation du jour disparaîtrait en passant le graphique en hebdo.
    pub fn apply(&mut self, fetched: FetchedTicker) {
        if let Some(name) = fetched.long_name {
            self.name = name;
        }
        let previous_close = fetched
            .quote
            .previous_close
            .or(self.quote.and_then(|q| q.previous_close));
        self.quote = Some(Quote {
            price: fetched.quote.price,
            previous_close,
        });
        self.currency = fetched.currency;
        self.price_decimals = fetched.price_decimals;
        self.data = Some(fetched.data);
    }

    /// Prix courant (None tant que rien n'est chargé)
    pub fn current_price(&self) -> Option<f64> {
        self.quote.map(|q| q.price)
    }

    /// Variation du jour en %
    pub fn change_percent(&self) -> Option<f64> {
        self.quote?.change_percent()
    }

    /// Retourne true si le ticker est en hausse sur la journée
    pub fn is_positive(&self) -> bool {
        self.change_percent().is_some_and(|c| c >= 0.0)
    }

    // ========================================================================
    // Portefeuille
    // ========================================================================
    // CONCEPT RUST : Option chaînée avec `?`
    // - Sans position ou sans cours, chaque méthode rend None : pas de NaN,
    //   et l'appelant décide quoi afficher
    // ========================================================================

    /// Valeur de la position au cours actuel
    pub fn market_value(&self) -> Option<f64> {
        Some(self.position?.quantity * self.current_price()?)
    }

    /// Coût d'acquisition de la position
    pub fn cost_basis(&self) -> Option<f64> {
        let position = self.position?;
        Some(position.quantity * position.unit_cost)
    }

    /// Plus-value latente (valeur − coût)
    pub fn unrealized_pnl(&self) -> Option<f64> {
        Some(self.market_value()? - self.cost_basis()?)
    }

    /// Plus-value latente en % du coût
    pub fn unrealized_pnl_percent(&self) -> Option<f64> {
        let cost = self.cost_basis().filter(|c| *c != 0.0)?;
        Some(self.unrealized_pnl()? / cost * 100.0)
    }

    /// Gain ou perte du jour sur la position
    pub fn day_pnl(&self) -> Option<f64> {
        let quote = self.quote?;
        Some(self.position?.quantity * (quote.price - quote.previous_close?))
    }

    /// Prix avec la précision et la devise de la place ("339.75 USD", "1.1453 USD")
    pub fn format_price(&self, price: f64) -> String {
        match &self.currency {
            Some(currency) => format!("{price:.prec$} {currency}", prec = self.price_decimals),
            None => format!("{price:.prec$}", prec = self.price_decimals),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Interval;
    use chrono::FixedOffset;

    #[test]
    fn quote_change_percent() {
        let quote = |previous_close| Quote {
            price: 110.0,
            previous_close,
        };
        assert_eq!(quote(Some(100.0)).change_percent(), Some(10.0));
        assert_eq!(quote(None).change_percent(), None);
        assert_eq!(quote(Some(0.0)).change_percent(), None);
    }

    #[test]
    fn apply_keeps_previous_close_when_new_one_is_unknown() {
        let utc = FixedOffset::east_opt(0).unwrap();
        let mut item = WatchlistItem::new("AAPL".into());
        assert_eq!(item.name, "AAPL");
        let fetched = |interval, previous_close| FetchedTicker {
            data: OHLCData::new("AAPL".into(), interval, utc),
            long_name: Some("Apple Inc.".into()),
            quote: Quote {
                price: 110.0,
                previous_close,
            },
            currency: Some("USD".into()),
            price_decimals: 2,
        };
        item.apply(fetched(Interval::M30, Some(100.0)));
        item.apply(fetched(Interval::W1, None)); // passage en hebdo dans le graphique
        assert_eq!(item.change_percent(), Some(10.0));
        assert!(item.is_positive());
        assert_eq!(item.name, "Apple Inc.");
        assert_eq!(item.format_price(1.14532), "1.15 USD");
    }

    fn quoted(price: f64, previous_close: f64, position: Option<Position>) -> WatchlistItem {
        let mut item = WatchlistItem::new("AAPL".into());
        item.quote = Some(Quote {
            price,
            previous_close: Some(previous_close),
        });
        item.position = position;
        item
    }

    #[test]
    fn position_values_and_pnl() {
        let position = Position {
            quantity: 10.0,
            unit_cost: 100.0,
        };
        let item = quoted(110.0, 105.0, Some(position));
        assert_eq!(item.market_value(), Some(1100.0));
        assert_eq!(item.unrealized_pnl(), Some(100.0));
        assert_eq!(item.unrealized_pnl_percent(), Some(10.0));
        assert_eq!(item.day_pnl(), Some(50.0));
        // Sans position ou sans cours : rien, jamais NaN
        assert_eq!(quoted(110.0, 105.0, None).market_value(), None);
        let mut no_quote = quoted(1.0, 1.0, Some(position));
        no_quote.quote = None;
        assert_eq!(no_quote.unrealized_pnl(), None);
    }
}
