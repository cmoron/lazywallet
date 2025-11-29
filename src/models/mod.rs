// ============================================================================
// Module : models
// ============================================================================
// Ce module contient toutes les structures de données de l'application
//
// CONCEPT RUST : Modules et visibilité
// - "pub mod" : déclare un sous-module publique (accessible depuis l'extérieur)
// - Sans "pub", le module serait privé au crate
// ============================================================================

pub mod ticker;         // Déclaration du module ticker (fichier ticker.rs)
pub mod ohlc;           // Déclaration du module ohlc (fichier ohlc.rs)
pub mod watchlist_item; // Déclaration du module watchlist_item (fichier watchlist_item.rs)

// Re-export des structures principales pour simplifier les imports
// Au lieu de : use lazywallet::models::ticker::Ticker;
// On peut faire : use lazywallet::models::Ticker;
pub use ticker::Ticker;
pub use ohlc::{Interval, LabelStrategy, OHLC, OHLCData, Timeframe};
pub use watchlist_item::WatchlistItem;
