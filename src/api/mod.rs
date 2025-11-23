// ============================================================================
// Module : api
// ============================================================================
// Ce module contient tous les clients API pour récupérer les données
// financières depuis différentes sources (Yahoo Finance, CoinGecko, etc.)
// ============================================================================

pub mod yahoo;  // Client API Yahoo Finance

// Re-export des fonctions principales
pub use yahoo::fetch_ticker_data;
