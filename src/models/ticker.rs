// ============================================================================
// Structure : Ticker
// ============================================================================
// Représente un symbole boursier (action, crypto, ETF, etc.)
//
// CONCEPTS RUST :
// 1. #[derive(...)] : génère automatiquement l'implémentation de traits
//    - Debug : permet d'afficher la structure avec {:?}
//    - Clone : permet de dupliquer la valeur
//    - PartialEq : permet de comparer deux tickers avec ==
//
// 2. String vs &str :
//    - String : owned string (possède la mémoire, heap allocated)
//    - &str : borrowed string slice (référence, ne possède pas)
//    - On utilise String ici car le Ticker possède ses données
// ============================================================================

use serde::{Deserialize, Serialize};

/// Type d'actif financier
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TickerType {
    Stock,      // Action (ex: AAPL, TSLA)
    Crypto,     // Cryptomonnaie (ex: BTC, ETH)
    ETF,        // Exchange-Traded Fund (ex: SPY, QQQ)
    Index,      // Indice (ex: ^GSPC, ^DJI)
    Forex,      // Devise (ex: EURUSD)
}

/// Ticker représentant un symbole boursier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    /// Symbole du ticker (ex: "AAPL", "BTC-USD")
    pub symbol: String,

    /// Nom complet (ex: "Apple Inc.", "Bitcoin USD")
    pub name: String,

    /// Type d'actif
    pub ticker_type: TickerType,

    /// Prix actuel (optionnel car peut ne pas être chargé)
    /// CONCEPT RUST : Option<T>
    /// - Some(value) : contient une valeur
    /// - None : pas de valeur (comme null, mais type-safe)
    pub current_price: Option<f64>,

    /// Variation sur 24h en pourcentage
    pub change_percent_24h: Option<f64>,
}

impl Ticker {
    /// Constructeur : crée un nouveau Ticker
    ///
    /// CONCEPT RUST : Ownership
    /// - Les paramètres String sont "moved" dans la fonction
    /// - Le Ticker devient le nouveau propriétaire de ces Strings
    pub fn new(symbol: String, name: String, ticker_type: TickerType) -> Self {
        Self {
            symbol,
            name,
            ticker_type,
            current_price: None,
            change_percent_24h: None,
        }
    }

    /// Met à jour le prix actuel
    ///
    /// CONCEPT RUST : &mut self
    /// - &mut : référence mutable (modifie l'objet)
    /// - &self : référence immutable (lecture seule)
    /// - self : consomme l'objet (move)
    pub fn update_price(&mut self, price: f64, change_percent: f64) {
        self.current_price = Some(price);
        self.change_percent_24h = Some(change_percent);
    }

    /// Formatte le ticker pour l'affichage
    pub fn display(&self) -> String {
        let price_str = match self.current_price {
            Some(price) => format!("${:.2}", price),
            None => "N/A".to_string(),
        };

        let change_str = match self.change_percent_24h {
            Some(change) => {
                let arrow = if change >= 0.0 { "▲" } else { "▼" };
                format!("{} {:+.2}%", arrow, change)
            }
            None => "".to_string(),
        };

        format!("{:<8} {:<20} {:>12}  {}",
                self.symbol, self.name, price_str, change_str)
    }
}

// ============================================================================
// Tests unitaires
// ============================================================================
// CONCEPT RUST : Tests
// - #[cfg(test)] : compile uniquement en mode test
// - #[test] : marque une fonction comme test
// - assert_eq! : macro pour vérifier l'égalité
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticker_creation() {
        let ticker = Ticker::new(
            "AAPL".to_string(),
            "Apple Inc.".to_string(),
            TickerType::Stock,
        );

        assert_eq!(ticker.symbol, "AAPL");
        assert_eq!(ticker.current_price, None);
    }

    #[test]
    fn test_ticker_update_price() {
        let mut ticker = Ticker::new(
            "AAPL".to_string(),
            "Apple Inc.".to_string(),
            TickerType::Stock,
        );

        ticker.update_price(185.23, 2.34);

        assert_eq!(ticker.current_price, Some(185.23));
        assert_eq!(ticker.change_percent_24h, Some(2.34));
    }
}
