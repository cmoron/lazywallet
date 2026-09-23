// ============================================================================
// Module : models
// ============================================================================
// Ce module contient toutes les structures de données de l'application
//
// CONCEPT RUST : Modules et visibilité
// - "pub mod" : déclare un sous-module publique (accessible depuis l'extérieur)
// - Sans "pub", le module serait privé au crate
// ============================================================================

pub mod ohlc; // Déclaration du module ohlc (fichier ohlc.rs)
pub mod watchlist_item; // Déclaration du module watchlist_item (fichier watchlist_item.rs)

// Re-export des structures principales pour simplifier les imports
// Au lieu de : use lazywallet::models::ohlc::Interval;
// On peut faire : use lazywallet::models::Interval;
pub use ohlc::{Interval, OHLCData, OHLC};
pub use watchlist_item::{FetchedTicker, Position, Quote, WatchlistItem};
