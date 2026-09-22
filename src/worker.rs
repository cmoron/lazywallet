// ============================================================================
// Worker réseau : transforme des commandes en résultats
// ============================================================================
// CONCEPT RUST : Séparation des responsabilités entre threads
// - Le worker ne touche jamais à `App` : il reçoit des `AppCommand` et renvoie
//   exactement un `AppResult` par commande
// - Le thread principal reste seul propriétaire de l'état : plus besoin
//   d'`Arc<Mutex<App>>`, et `App::pending` compte les commandes en vol
// ============================================================================

use std::sync::mpsc::{Receiver, Sender};

use anyhow::{Context, Result};

use crate::api::yahoo::{fetch_ticker_data, http_client};
use crate::models::{FetchedTicker, Interval};

/// Commandes envoyées au worker
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    /// Charge (ou recharge) un ticker de la watchlist dans un intervalle
    Load { symbol: String, interval: Interval },
    /// Vérifie qu'un nouveau symbole existe et récupère ses données
    Add { symbol: String },
}

/// Résultats renvoyés par le worker (un par commande)
#[derive(Debug)]
pub enum AppResult {
    Loaded {
        symbol: String,
        fetched: FetchedTicker,
    },
    Added {
        symbol: String,
        fetched: FetchedTicker,
    },
    /// Le message contient déjà le symbole concerné
    Failed { error: String },
}

/// Lance le thread réseau
///
/// Le runtime et le client sont créés avant le thread : une erreur remonte à
/// `main()` au lieu de faire paniquer le worker.
///
/// # Errors
/// Si le runtime tokio ou le client HTTP ne peuvent pas être créés.
pub fn spawn_worker(commands: Receiver<AppCommand>, results: Sender<AppResult>) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Échec de la création du runtime tokio")?;
    let client = http_client()?;
    std::thread::spawn(move || {
        // La boucle s'arrête quand main() lâche son Sender (fin de programme)
        for command in commands {
            let result = runtime.block_on(execute(&client, command));
            if results.send(result).is_err() {
                break;
            }
        }
    });
    Ok(())
}

async fn execute(client: &reqwest::Client, command: AppCommand) -> AppResult {
    let (symbol, interval, adding) = match command {
        AppCommand::Load { symbol, interval } => (symbol, interval, false),
        AppCommand::Add { symbol } => (symbol, Interval::default(), true),
    };
    match fetch_ticker_data(client, &symbol, interval).await {
        Ok(fetched) if adding => AppResult::Added { symbol, fetched },
        Ok(fetched) => AppResult::Loaded { symbol, fetched },
        Err(error) => {
            tracing::error!(%symbol, ?error, "Chargement échoué");
            // {:#} : message + contexte sur une seule ligne, lisible dans la barre d'état
            AppResult::Failed {
                error: format!("{error:#}"),
            }
        }
    }
}
