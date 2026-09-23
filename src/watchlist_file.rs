// ============================================================================
// Persistance de la watchlist
// ============================================================================
// Un symbole par ligne, lisible et éditable à la main
// ============================================================================

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::models::Position;

/// Watchlist du premier lancement
pub const DEFAULT_SYMBOLS: [&str; 3] = ["AAPL", "TSLA", "BTC-USD"];

/// Une ligne de la watchlist : `SYMBOLE [QUANTITÉ PRIX_DE_REVIENT] [# commentaire]`
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub symbol: String,
    pub position: Option<Position>,
}

impl Entry {
    /// Ticker simplement suivi, sans position
    pub fn watch(symbol: String) -> Self {
        Self {
            symbol,
            position: None,
        }
    }

    /// Texte de la ligne, sans commentaire
    fn to_line(&self) -> String {
        match self.position {
            Some(p) => format!("{} {} {}", self.symbol, p.quantity, p.unit_cost),
            None => self.symbol.clone(),
        }
    }
}

/// `~/.config/lazywallet/watchlist.txt` sous Linux (via la crate `dirs`)
///
/// # Errors
/// Si le système ne fournit pas de répertoire de configuration.
pub fn default_path() -> Result<PathBuf> {
    let config = dirs::config_dir().context("Répertoire de configuration introuvable")?;
    Ok(config.join("lazywallet").join("watchlist.txt"))
}

/// Sépare une ligne en contenu et commentaire (`#...`, gardé pour la réécriture)
fn split_comment(line: &str) -> (&str, Option<&str>) {
    match line.find('#') {
        Some(i) => (line[..i].trim(), Some(line[i..].trim_end())),
        None => (line.trim(), None),
    }
}

/// Lit un nombre positif (la virgule décimale est acceptée)
fn parse_positive(text: &str, what: &str) -> Result<f64> {
    let value: f64 = text
        .replace(',', ".")
        .parse()
        .with_context(|| format!("{what} invalide « {text} »"))?;
    if !value.is_finite() || value <= 0.0 {
        bail!("{what} doit être positive (« {text} »)");
    }
    Ok(value)
}

/// Lit une position saisie : "QUANTITÉ PRIX_DE_REVIENT", ou rien (pas de position)
///
/// # Errors
/// Si le texte n'a pas exactement 0 ou 2 nombres positifs.
pub fn parse_position(text: &str) -> Result<Option<Position>> {
    match text.split_whitespace().collect::<Vec<_>>()[..] {
        [] => Ok(None),
        [quantity, unit_cost] => Ok(Some(Position {
            quantity: parse_positive(quantity, "quantité")?,
            unit_cost: parse_positive(unit_cost, "prix de revient")?,
        })),
        _ => bail!("0 ou 2 nombres attendus (quantité prix_de_revient)"),
    }
}

/// Analyse le contenu d'une ligne (sans commentaire, non vide)
fn parse_entry(content: &str) -> Result<Entry> {
    let (symbol, rest) = content
        .split_once(char::is_whitespace)
        .unwrap_or((content, ""));
    Ok(Entry {
        symbol: symbol.to_uppercase(),
        position: parse_position(rest)?,
    })
}

/// Charge la watchlist : lignes vides et `#` ignorées, symboles en majuscules,
/// premier doublon gardé
///
/// CONCEPT RUST : match sur ErrorKind
/// - Fichier absent = premier lancement, pas une erreur
/// - Toute autre erreur (droits...) remonte avec le chemin en contexte
///
/// # Errors
/// Si le fichier existe mais ne peut pas être lu, ou si une position est invalide
/// (le message donne le numéro de ligne).
pub fn load(path: &Path) -> Result<Vec<Entry>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(DEFAULT_SYMBOLS
                .iter()
                .map(|s| Entry::watch((*s).to_string()))
                .collect())
        }
        Err(e) => return Err(e).with_context(|| format!("Lecture de {}", path.display())),
    };
    let mut entries: Vec<Entry> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let (content, _) = split_comment(line);
        if content.is_empty() {
            continue;
        }
        let entry = parse_entry(content)
            .with_context(|| format!("{} ligne {}", path.display(), number + 1))?;
        if entries.iter().all(|e| e.symbol != entry.symbol) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// Écrit la watchlist, en créant le dossier si besoin
///
/// Les commentaires et lignes vides d'un fichier existant sont conservés à leur
/// place, commentaires en fin de ligne compris ; les symboles retirés
/// disparaissent, les nouveaux sont ajoutés à la fin.
///
/// # Errors
/// Si le fichier existant ne peut pas être lu, le dossier créé ou le fichier écrit.
pub fn save(path: &Path, entries: &[Entry]) -> Result<()> {
    let existing = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("Lecture de {}", path.display())),
    };

    let mut lines: Vec<String> = Vec::new();
    let mut written: Vec<&str> = Vec::new();
    for line in existing.lines() {
        let (content, comment) = split_comment(line);
        let Some(symbol) = content.split_whitespace().next() else {
            // Ligne vide ou commentaire seul : conservée telle quelle
            lines.push(line.to_string());
            continue;
        };
        let symbol = symbol.to_uppercase();
        let Some(entry) = entries.iter().find(|e| e.symbol == symbol) else {
            continue; // symbole retiré
        };
        if written.contains(&entry.symbol.as_str()) {
            continue;
        }
        written.push(&entry.symbol);
        lines.push(match comment {
            Some(comment) => format!("{}  {comment}", entry.to_line()),
            None => entry.to_line(),
        });
    }
    for entry in entries {
        if !written.contains(&entry.symbol.as_str()) {
            lines.push(entry.to_line());
        }
    }

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("Création de {}", dir.display()))?;
    }
    let mut text = lines.join("\n");
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

    fn symbols(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.symbol.as_str()).collect()
    }

    fn watch(symbols: &[&str]) -> Vec<Entry> {
        symbols
            .iter()
            .map(|s| Entry::watch((*s).to_string()))
            .collect()
    }

    fn cleanup(path: &Path) {
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(
            symbols(&load(&temp_path("missing")).unwrap()),
            DEFAULT_SYMBOLS
        );
    }

    #[test]
    fn round_trip_and_sanitizing() {
        let path = temp_path("roundtrip");
        save(&path, &watch(&["AAPL", "^GSPC"])).unwrap();
        assert_eq!(symbols(&load(&path).unwrap()), ["AAPL", "^GSPC"]);
        std::fs::write(&path, "# mes tickers\n\n  btc-usd \nAAPL\naapl\n").unwrap();
        assert_eq!(symbols(&load(&path).unwrap()), ["BTC-USD", "AAPL"]);
        cleanup(&path);
    }

    #[test]
    fn unwritable_path_is_an_error() {
        // Un fichier à la place du dossier parent rend l'écriture impossible
        let blocker = temp_path("blocker");
        std::fs::create_dir_all(blocker.parent().unwrap()).unwrap();
        std::fs::write(&blocker, "").unwrap();
        assert!(save(&blocker.join("watchlist.txt"), &watch(&["AAPL"])).is_err());
        cleanup(&blocker);
    }

    #[test]
    fn save_keeps_hand_written_comments() {
        // Revue : le premier ajout effaçait les commentaires écrits à la main
        let path = temp_path("comments");
        save(&path, &watch(&["AAPL"])).unwrap();
        std::fs::write(&path, "# Mes tickers\n\nAAPL\n# crypto\nBTC-USD\n").unwrap();
        save(&path, &watch(&["AAPL", "BTC-USD", "QQQ"])).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("# Mes tickers") && text.contains("# crypto"),
            "{text}"
        );
        assert_eq!(symbols(&load(&path).unwrap()), ["AAPL", "BTC-USD", "QQQ"]);
        cleanup(&path);
    }

    #[test]
    fn positions_round_trip_with_inline_comments() {
        let path = temp_path("positions");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "# Portefeuille\nAAPL 10 150.25  # PEA\nBTC-USD 0.05 62000\nTSLA          # suivi seul\n",
        )
        .unwrap();
        let entries = load(&path).unwrap();
        assert_eq!(
            entries[0].position,
            Some(Position {
                quantity: 10.0,
                unit_cost: 150.25
            })
        );
        assert_eq!(entries[2].position, None);

        // Nouvelle position sur TSLA, AAPL vendu
        let mut entries: Vec<Entry> = entries.into_iter().skip(1).collect();
        entries[1].position = Some(Position {
            quantity: 3.0,
            unit_cost: 210.0,
        });
        save(&path, &entries).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("TSLA 3 210"), "{text}");
        assert!(
            text.contains("# suivi seul") && !text.contains("AAPL"),
            "{text}"
        );
        assert_eq!(load(&path).unwrap(), entries);
        cleanup(&path);
    }

    #[test]
    fn bad_positions_name_the_line() {
        let path = temp_path("bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for (line, hint) in [
            ("AAPL abc 150", "quantité"),
            ("AAPL 10", "2 nombres"),
            ("AAPL -1 150", "positive"),
            ("AAPL 10 150 20", "2 nombres"),
        ] {
            std::fs::write(&path, format!("# en-tête\n{line}\n")).unwrap();
            let err = format!("{:#}", load(&path).unwrap_err());
            assert!(
                err.contains("ligne 2") && err.contains(hint),
                "{line} → {err}"
            );
        }
        cleanup(&path);
    }
}
