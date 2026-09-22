// ============================================================================
// Module : ui
// ============================================================================
// Gère toute l'interface utilisateur (Terminal User Interface)
// ============================================================================

pub mod candlestick_text;
pub mod dashboard; // Rendu de l'interface principale
pub mod events; // Gestion des événements clavier // Rendu des chandeliers japonais (Unicode text)

// Re-exports pour simplifier les imports
pub use dashboard::render;
pub use events::{Event, EventHandler};
