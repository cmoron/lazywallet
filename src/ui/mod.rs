// ============================================================================
// Module : ui
// ============================================================================
// Gère toute l'interface utilisateur (Terminal User Interface)
// ============================================================================

pub mod events;           // Gestion des événements clavier
pub mod dashboard;        // Rendu de l'interface principale
pub mod chart;            // Rendu du graphique ligne
pub mod candlestick_text; // Rendu des chandeliers japonais (Unicode text)

// Re-exports pour simplifier les imports
pub use events::{Event, EventHandler};
pub use dashboard::render;
