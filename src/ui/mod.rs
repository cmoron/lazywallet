// ============================================================================
// Module : ui
// ============================================================================
// Gère toute l'interface utilisateur (Terminal User Interface)
// ============================================================================

pub mod chart; // Graphique en chandeliers (widget + écran)
pub mod dashboard; // Rendu de l'interface principale
pub mod events; // Gestion des événements clavier

// Re-exports pour simplifier les imports
pub use dashboard::render;
pub use events::EventHandler;
