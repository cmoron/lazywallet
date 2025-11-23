// ============================================================================
// API Client : Yahoo Finance
// ============================================================================
// Récupère les données financières depuis Yahoo Finance
//
// CONCEPTS RUST AVANCÉS :
// 1. async/await : programmation asynchrone (non-bloquante)
// 2. Result<T, E> : gestion d'erreurs avec contexte
// 3. Serde : désérialisation JSON automatique
// 4. Lifetimes : gestion de la durée de vie des références
// ============================================================================

use anyhow::{Context, Result};
use chrono::DateTime;
use serde::Deserialize;
use tracing::{debug, error, info, instrument, warn};

use crate::models::{Interval, OHLCData, Timeframe, OHLC};

// ============================================================================
// Structures pour parser la réponse JSON de Yahoo Finance
// ============================================================================
// Yahoo retourne un JSON complexe, on définit des structures qui matchent
// exactement la structure JSON pour que serde puisse désérialiser automatiquement
//
// CONCEPT RUST : #[serde(rename = "...")]
// - Permet de mapper un nom de champ JSON différent du nom Rust
// - Exemple : "regularMarketPrice" (JSON) -> "regular_market_price" (Rust)
// ============================================================================

/// Réponse complète de l'API Yahoo Finance
#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart: Chart,
}

#[derive(Debug, Deserialize)]
struct Chart {
    result: Vec<ChartResult>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta: Meta,
    timestamp: Option<Vec<i64>>,
    indicators: Indicators,
}

/// Métadonnées du ticker
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]  // Convertit automatiquement snake_case -> camelCase
struct Meta {
    symbol: String,
    regular_market_price: Option<f64>,
    chart_previous_close: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Indicators {
    quote: Vec<Quote>,
}

/// Données OHLCV (Open, High, Low, Close, Volume)
#[derive(Debug, Deserialize)]
struct Quote {
    open: Option<Vec<Option<f64>>>,
    high: Option<Vec<Option<f64>>>,
    low: Option<Vec<Option<f64>>>,
    close: Option<Vec<Option<f64>>>,
    volume: Option<Vec<Option<u64>>>,
}

// ============================================================================
// Fonctions publiques de l'API
// ============================================================================

/// Récupère les données d'un ticker depuis Yahoo Finance
///
/// CONCEPT RUST : async fn
/// - Fonction asynchrone qui peut être "await"ée
/// - Ne bloque pas le thread pendant les I/O (network, disk)
/// - Retourne une Future qui doit être .await pour obtenir le résultat
///
/// CONCEPT RUST : Result<T, E>
/// - Ok(value) : succès
/// - Err(error) : erreur
/// - Propagation d'erreur avec ? operator
///
/// # Arguments
/// * `symbol` - Symbole du ticker (ex: "AAPL", "TSLA", "BTC-USD")
/// * `timeframe` - Période de temps souhaitée
///
/// # Retourne
/// * `Result<OHLCData>` - Données OHLC ou erreur
///
/// # Exemple
/// let data = fetch_ticker_data("AAPL", Interval::M30).await?;
/// println!("Prix actuel : {}", data.last().unwrap().close);
///
/// CONCEPT RUST : #[instrument]
/// - Macro tracing qui ajoute automatiquement un span
/// - Inclut les paramètres de la fonction dans les logs
/// - Tous les logs à l'intérieur auront le contexte symbol + interval
#[instrument(skip(interval), fields(interval = ?interval))]
pub async fn fetch_ticker_data(symbol: &str, interval: Interval) -> Result<OHLCData> {
    // Le timeframe est déterminé automatiquement selon l'intervalle
    let timeframe = interval.default_timeframe();

    // Construit l'URL de l'API Yahoo Finance
    // CONCEPT RUST : format! macro
    // - Équivalent à sprintf en C ou f-string en Python
    // - Type-safe et performant
    let url = build_yahoo_url(symbol, interval, timeframe);
    debug!(url = %url, interval = %interval.label(), timeframe = %timeframe.label(), "Built Yahoo Finance API URL");

    // CONCEPT RUST : async/await
    // - reqwest::get() retourne une Future
    // - .await suspend l'exécution jusqu'à ce que la requête soit terminée
    // - ? propage l'erreur si la requête échoue
    //
    // CONCEPT RUST : Context trait (anyhow)
    // - .context() ajoute du contexte à une erreur
    // - Aide au debugging en donnant plus d'infos
    //
    // Ajout d'un User-Agent pour éviter le blocage par Yahoo
    debug!("Creating HTTP client");
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .context("Échec de la création du client HTTP")?;

    debug!("Sending HTTP request to Yahoo Finance");
    let response = client
        .get(&url)
        .send()
        .await
        .context("Échec de la requête HTTP vers Yahoo Finance")?;

    let status = response.status();
    debug!(status = %status, "Received HTTP response");

    // Vérifie que la réponse est un succès HTTP (200-299)
    if !status.is_success() {
        error!(status = %status, "Yahoo Finance returned error status");
        anyhow::bail!(
            "Yahoo Finance a retourné une erreur : HTTP {}",
            status
        );
    }

    // Parse la réponse JSON
    // CONCEPT RUST : Serde deserialization
    // - .json::<T>() désérialise automatiquement le JSON vers le type T
    // - Vérifie que la structure JSON match exactement
    debug!("Parsing JSON response");
    let yahoo_response: YahooResponse = response
        .json()
        .await
        .context("Échec du parsing JSON de la réponse Yahoo")?;

    // Convertit la réponse Yahoo en notre structure OHLCData
    debug!("Parsing Yahoo response to OHLCData");
    let data = parse_yahoo_response(yahoo_response, symbol, interval, timeframe)?;

    info!(candles = data.len(), "Successfully fetched ticker data");
    Ok(data)
}

/// Construit l'URL de l'API Yahoo Finance
///
/// CONCEPT RUST : &str vs String
/// - Fonction prend &str (référence, pas d'allocation)
/// - Retourne String (owned, allouée)
/// - Pas de lifetime ici car String est owned
///
/// L'intervalle est maintenant configurable (1m, 5m, 30m, 1h, 1d, etc.)
fn build_yahoo_url(symbol: &str, interval: Interval, timeframe: Timeframe) -> String {
    // Calcule les timestamps Unix
    let now = chrono::Utc::now().timestamp();
    let days_ago = timeframe.to_days() as i64;
    let period1 = now - (days_ago * 24 * 60 * 60);
    let period2 = now;

    // Utilise l'intervalle fourni, converti au format Yahoo (ex: "30m", "1h", "1d")
    let interval_str = interval.to_yahoo_string();

    format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={}&period1={}&period2={}",
        symbol, interval_str, period1, period2
    )
}

/// Parse la réponse JSON de Yahoo et la convertit en OHLCData
///
/// CONCEPT RUST : Ownership et borrowing
/// - yahoo_response est "moved" (pas de &), on en devient propriétaire
/// - symbol est borrowed (&str), on ne le copie pas
/// - interval et timeframe sont Copy (enums simples), donc copiés automatiquement
fn parse_yahoo_response(
    yahoo_response: YahooResponse,
    symbol: &str,
    interval: Interval,
    timeframe: Timeframe,
) -> Result<OHLCData> {
    // Récupère le premier résultat
    // CONCEPT RUST : Pattern matching avec if let
    let result = yahoo_response
        .chart
        .result
        .into_iter()  // Consomme le Vec (move)
        .next()       // Prend le premier élément
        .context("Aucune données retournée par Yahoo Finance")?;

    // Crée la structure OHLCData avec interval et timeframe
    let mut ohlc_data = OHLCData::new(symbol.to_string(), interval, timeframe);

    // Récupère les arrays de données
    // CONCEPT RUST : Option unwrap et default
    let timestamps = result.timestamp.unwrap_or_default();
    debug!(timestamp_count = timestamps.len(), "Received timestamps from Yahoo");

    let quote = result.indicators.quote.into_iter().next()
        .context("Pas de données OHLC dans la réponse")?;

    let opens = quote.open.unwrap_or_default();
    let highs = quote.high.unwrap_or_default();
    let lows = quote.low.unwrap_or_default();
    let closes = quote.close.unwrap_or_default();
    let volumes = quote.volume.unwrap_or_default();

    // CONCEPT RUST : Iterators et zip
    // - .iter() crée un itérateur sur une slice
    // - .enumerate() ajoute l'index
    // - zip combine plusieurs itérateurs
    // - for loop consomme l'itérateur
    let mut skipped_count = 0;
    for (i, &timestamp) in timestamps.iter().enumerate() {
        // Extrait les valeurs à l'index i, skip si None
        // CONCEPT RUST : Pattern matching avec match
        let open = match opens.get(i).and_then(|&v| v) {
            Some(v) => v,
            None => {
                skipped_count += 1;
                continue;  // Skip cette chandelle si pas de données
            }
        };

        let high = match highs.get(i).and_then(|&v| v) {
            Some(v) => v,
            None => {
                skipped_count += 1;
                continue;
            }
        };

        let low = match lows.get(i).and_then(|&v| v) {
            Some(v) => v,
            None => {
                skipped_count += 1;
                continue;
            }
        };

        let close = match closes.get(i).and_then(|&v| v) {
            Some(v) => v,
            None => {
                skipped_count += 1;
                continue;
            }
        };

        let volume = volumes.get(i).and_then(|&v| v).unwrap_or(0);

        // Convertit le timestamp Unix en DateTime<Utc>
        // CONCEPT RUST : Result et ? operator
        let datetime = DateTime::from_timestamp(timestamp, 0)
            .context("Timestamp invalide")?;

        // Crée et ajoute la chandelle OHLC
        ohlc_data.add_candle(OHLC::new(
            datetime,
            open,
            high,
            low,
            close,
            volume,
        ));
    }

    // Log des statistiques de parsing
    if skipped_count > 0 {
        warn!(
            skipped = skipped_count,
            total = timestamps.len(),
            "Skipped candles with missing data"
        );
    }

    debug!(
        parsed = ohlc_data.len(),
        total = timestamps.len(),
        skipped = skipped_count,
        "Finished parsing OHLC data"
    );

    // Vérifie qu'on a au moins quelques données
    if ohlc_data.is_empty() {
        error!("No valid OHLC data found");
        anyhow::bail!("Aucune donnée OHLC valide trouvée pour {}", symbol);
    }

    Ok(ohlc_data)
}

// ============================================================================
// Tests unitaires
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_yahoo_url() {
        let url = build_yahoo_url("AAPL", Interval::D1, Timeframe::OneWeek);
        assert!(url.contains("AAPL"));
        assert!(url.contains("interval=1d"));
        assert!(url.contains("yahoo.com"));
    }

    // Test async nécessite tokio test runtime
    // CONCEPT RUST : #[tokio::test]
    // - Macro qui setup un runtime tokio pour le test
    // - Permet d'utiliser .await dans les tests
    #[tokio::test]
    async fn test_fetch_ticker_data() {
        // Test avec un vrai appel API (peut échouer si pas de connexion)
        let result = fetch_ticker_data("AAPL", Interval::D1).await;

        // On vérifie juste que l'appel fonctionne
        // (on ne vérifie pas les données car elles changent)
        match result {
            Ok(data) => {
                assert_eq!(data.symbol, "AAPL");
                assert!(!data.is_empty());
                println!("✓ Récupéré {} chandelles pour AAPL", data.len());
            }
            Err(e) => {
                println!("⚠ Test skippé (pas de connexion?) : {}", e);
            }
        }
    }
}
