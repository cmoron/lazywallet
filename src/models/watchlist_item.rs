// ============================================================================
// Structure : WatchlistItem
// ============================================================================
// Représente un item dans la watchlist avec ses données chargées
//
// CONCEPTS RUST :
// 1. Composition : WatchlistItem contient OHLCData
// 2. Methods : calculer le prix actuel et la variation
// 3. Option : gérer les données manquantes
// ============================================================================

use crate::models::{OHLCData, OHLC};

/// Un ticker dans la watchlist avec ses données
#[derive(Debug, Clone)]
pub struct WatchlistItem {
    /// Symbole du ticker (ex: "AAPL")
    pub symbol: String,

    /// Nom complet (ex: "Apple Inc.")
    pub name: String,

    /// Données OHLC chargées (None si pas encore chargées ou erreur)
    /// CONCEPT RUST : Option pour les données optionnelles
    /// - Some(data) : données disponibles
    /// - None : pas encore chargées ou erreur de chargement
    pub data: Option<OHLCData>,
}

impl WatchlistItem {
    /// Crée un nouvel item de watchlist sans données
    pub fn new(symbol: String, name: String) -> Self {
        Self {
            symbol,
            name,
            data: None,
        }
    }

    /// Crée un item avec des données déjà chargées
    pub fn with_data(symbol: String, name: String, data: OHLCData) -> Self {
        Self {
            symbol,
            name,
            data: Some(data),
        }
    }

    /// Retourne le prix actuel (close de la dernière chandelle)
    ///
    /// CONCEPT RUST : Option chaining avec ?
    /// - self.data? : early return si None
    /// - .last()? : early return si la liste est vide
    /// - Some(ohlc.close) : retourne le prix
    pub fn current_price(&self) -> Option<f64> {
        let data = self.data.as_ref()?;  // &Option<T> -> Option<&T>
        let last = data.last()?;
        Some(last.close)
    }

    /// Retourne la variation journalière en pourcentage
    ///
    /// CONCEPT RUST : Method chaining
    /// - self.data.as_ref() : &Option<OHLCData> -> Option<&OHLCData>
    /// - .and_then() : transforme Option<A> en Option<B>
    /// - Équivalent à un if let Some(data) = ... imbriqué
    ///
    /// CONCEPT : Daily change instead of total change
    /// - Affiche l'évolution du jour (ou dernière journée disponible)
    /// - Plus pertinent pour la watchlist que la variation totale
    pub fn change_percent(&self) -> Option<f64> {
        self.data
            .as_ref()
            .and_then(|data| data.daily_change_percent())
    }

    /// Retourne la dernière chandelle OHLC
    pub fn last_ohlc(&self) -> Option<&OHLC> {
        self.data.as_ref()?.last()
    }

    /// Vérifie si les données sont chargées
    pub fn has_data(&self) -> bool {
        self.data.is_some()
    }

    /// Formatte l'item pour l'affichage dans la liste
    ///
    /// Format : "AAPL    Apple Inc.         $271.49  ▲ +2.11%"
    ///
    /// CONCEPT RUST : String building
    /// - format! pour créer des strings formatées
    /// - match pour gérer les Option
    ///
    /// Note : Le nom est tronqué à 20 caractères pour éviter le débordement
    pub fn display(&self) -> String {
        // Prix
        let price_str = match self.current_price() {
            Some(price) => format!("${:.2}", price),
            None => "Loading...".to_string(),
        };

        // Variation avec flèche
        let change_str = match self.change_percent() {
            Some(change) => {
                let arrow = if change >= 0.0 { "▲" } else { "▼" };
                format!("{} {:+.2}%", arrow, change)
            }
            None => String::new(),
        };

        // Tronque le nom à 20 caractères avec ellipse si nécessaire
        let truncated_name = if self.name.chars().count() <= 20 {
            self.name.clone()
        } else {
            let truncated: String = self.name.chars().take(19).collect();
            format!("{}…", truncated)
        };

        format!(
            "{:<8} {:<20} {:>12}  {}",
            self.symbol, truncated_name, price_str, change_str
        )
    }

    /// Retourne true si le ticker est en hausse
    pub fn is_positive(&self) -> bool {
        self.change_percent().map(|c| c >= 0.0).unwrap_or(false)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Timeframe;
    use crate::models::Interval;
    use chrono::Utc;

    #[test]
    fn test_watchlist_item_new() {
        let item = WatchlistItem::new("AAPL".to_string(), "Apple Inc.".to_string());
        assert_eq!(item.symbol, "AAPL");
        assert!(!item.has_data());
        assert!(item.current_price().is_none());
    }

    #[test]
    fn test_watchlist_item_with_data() {
        let mut data = OHLCData::new("AAPL".to_string(), Interval::D1, Timeframe::OneWeek);
        data.add_candle(OHLC::new(
            Utc::now(),
            100.0,
            110.0,
            95.0,
            105.0,
            1000,
        ));

        let item = WatchlistItem::with_data(
            "AAPL".to_string(),
            "Apple Inc.".to_string(),
            data,
        );

        assert!(item.has_data());
        assert_eq!(item.current_price(), Some(105.0));
    }

    #[test]
    fn test_is_positive() {
        let mut data = OHLCData::new("AAPL".to_string(), Interval::D1, Timeframe::OneWeek);
        data.add_candle(OHLC::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000));

        let item = WatchlistItem::with_data(
            "AAPL".to_string(),
            "Apple Inc.".to_string(),
            data,
        );

        assert!(item.is_positive());
    }
}
