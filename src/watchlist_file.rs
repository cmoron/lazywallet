// ============================================================================
// Persistance de la watchlist
// ============================================================================
// Un symbole par ligne, lisible et éditable à la main
// ============================================================================

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Watchlist du premier lancement
pub const DEFAULT_SYMBOLS: [&str; 3] = ["AAPL", "TSLA", "BTC-USD"];

/// `~/.config/lazywallet/watchlist.txt` sous Linux (via la crate `dirs`)
///
/// # Errors
/// Si le système ne fournit pas de répertoire de configuration.
pub fn default_path() -> Result<PathBuf> {
    let config = dirs::config_dir().context("Répertoire de configuration introuvable")?;
    Ok(config.join("lazywallet").join("watchlist.txt"))
}

/// Charge les symboles : lignes vides et `#` ignorées, majuscules, sans doublon
///
/// CONCEPT RUST : match sur ErrorKind
/// - Fichier absent = premier lancement, pas une erreur
/// - Toute autre erreur (droits...) remonte avec le chemin en contexte
///
/// # Errors
/// Si le fichier existe mais ne peut pas être lu.
pub fn load(path: &Path) -> Result<Vec<String>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(DEFAULT_SYMBOLS.map(String::from).to_vec())
        }
        Err(e) => return Err(e).with_context(|| format!("Lecture de {}", path.display())),
    };
    let mut symbols: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let symbol = line.to_uppercase();
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    Ok(symbols)
}

/// Écrit la watchlist, en créant le dossier si besoin
///
/// # Errors
/// Si le dossier ne peut pas être créé ou le fichier écrit.
pub fn save(path: &Path, symbols: &[&str]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("Création de {}", dir.display()))?;
    }
    let mut text = symbols.join("\n");
    text.push('\n');
    fs::write(path, text).with_context(|| format!("Écriture de {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("lazywallet-test-{}-{name}", std::process::id()))
            .join("watchlist.txt")
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(load(&temp_path("missing")).unwrap(), DEFAULT_SYMBOLS);
    }

    #[test]
    fn round_trip_and_sanitizing() {
        let path = temp_path("roundtrip");
        save(&path, &["AAPL", "^GSPC"]).unwrap();
        assert_eq!(load(&path).unwrap(), ["AAPL", "^GSPC"]);
        std::fs::write(&path, "# mes tickers\n\n  btc-usd \nAAPL\naapl\n").unwrap();
        assert_eq!(load(&path).unwrap(), ["BTC-USD", "AAPL"]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn unwritable_path_is_an_error() {
        // Un fichier à la place du dossier parent rend l'écriture impossible
        let blocker = temp_path("blocker");
        std::fs::create_dir_all(blocker.parent().unwrap()).unwrap();
        std::fs::write(&blocker, "").unwrap();
        assert!(save(&blocker.join("watchlist.txt"), &["AAPL"]).is_err());
        std::fs::remove_dir_all(blocker.parent().unwrap()).unwrap();
    }
}
