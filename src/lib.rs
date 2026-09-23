// ============================================================================
// LazyWallet - Library
// ============================================================================
// Expose les modules publics pour les exemples et tests
// ============================================================================

pub mod api; // API Yahoo Finance
pub mod app; // État de l'application
pub mod models;
pub mod portfolio; // Totaux du portefeuille par devise // Structures de données
pub mod ui; // Interface utilisateur
pub mod watchlist_file; // Persistance de la watchlist
pub mod worker; // Thread réseau (commandes → résultats)
