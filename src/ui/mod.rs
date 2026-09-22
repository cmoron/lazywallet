// ============================================================================
// Module : ui
// ============================================================================
// Gère toute l'interface utilisateur (Terminal User Interface)
// ============================================================================

pub mod candlestick_text; // Rendu des chandeliers japonais (Unicode text)
pub mod chart; // Nouveau graphique responsive
pub mod dashboard; // Rendu de l'interface principale
pub mod events; // Gestion des événements clavier

// Re-exports pour simplifier les imports
pub use dashboard::render;
pub use events::EventHandler;
