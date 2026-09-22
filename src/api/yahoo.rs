// ============================================================================
// API Client : Yahoo Finance
// ============================================================================
// Récupère les données financières depuis l'endpoint `v8/finance/chart`
//
// CONCEPTS RUST AVANCÉS :
// 1. async/await : programmation asynchrone (non-bloquante)
// 2. Result<T, E> : gestion d'erreurs avec contexte (anyhow)
// 3. Serde : désérialisation JSON automatique
// 4. let-else : sortir tôt quand un motif ne correspond pas
// ============================================================================

use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, FixedOffset, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::{debug, instrument, warn};

use crate::models::{FetchedTicker, Interval, OHLCData, Quote, OHLC};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

// ============================================================================
// Structures pour parser la réponse JSON de Yahoo Finance
// ============================================================================
// On ne déclare que les champs utilisés : serde ignore les autres.
// ============================================================================

#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart: Chart,
}

#[derive(Debug, Deserialize)]
struct Chart {
    /// null quand Yahoo ne connaît pas le symbole
    result: Option<Vec<ChartResult>>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta: Meta,
    timestamp: Option<Vec<i64>>,
    indicators: Indicators,
}

/// Métadonnées du ticker
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")] // regular_market_price <- "regularMarketPrice"
struct Meta {
    long_name: Option<String>,
    regular_market_price: Option<f64>,
    currency: Option<String>,
    price_hint: Option<usize>,
    /// Décalage de la place en secondes (champ Yahoo tout en minuscules)
    #[serde(rename = "gmtoffset")]
    gmt_offset: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct Indicators {
    quote: Vec<QuoteArrays>,
}

/// Données OHLCV en colonnes : une valeur par timestamp, null si absente
#[derive(Debug, Deserialize)]
struct QuoteArrays {
    open: Option<Vec<Option<f64>>>,
    high: Option<Vec<Option<f64>>>,
    low: Option<Vec<Option<f64>>>,
    close: Option<Vec<Option<f64>>>,
    volume: Option<Vec<Option<u64>>>,
}

// ============================================================================
// Fonctions publiques de l'API
// ============================================================================

/// Client HTTP partagé par tous les appels (pool de connexions réutilisé)
///
/// CONCEPT : Timeout obligatoire — sans lui, une requête bloquée fige le worker.
///
/// # Errors
/// Si le backend TLS ne peut pas être initialisé.
pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(10))
        .build()
        .context("Échec de la création du client HTTP")
}

/// Récupère chandelles, prix courant et métadonnées d'un ticker
///
/// Les messages d'erreur commencent par le symbole : ils sont affichés tels quels.
///
/// # Errors
/// Réseau injoignable ou timeout, symbole inconnu (404), autre statut HTTP,
/// JSON illisible, ou aucune chandelle exploitable.
#[instrument(skip(client))]
pub async fn fetch_ticker_data(
    client: &reqwest::Client,
    symbol: &str,
    interval: Interval,
) -> Result<FetchedTicker> {
    let url = build_yahoo_url(symbol, interval, Utc::now().timestamp());
    debug!(%url, "Requête Yahoo Finance");

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("{symbol} : Yahoo Finance injoignable"))?;

    match response.status() {
        StatusCode::NOT_FOUND => bail!("{symbol} : symbole inconnu de Yahoo Finance"),
        status if !status.is_success() => bail!("{symbol} : Yahoo Finance a répondu {status}"),
        _ => {}
    }

    let body: YahooResponse = response
        .json()
        .await
        .with_context(|| format!("{symbol} : réponse Yahoo illisible"))?;

    parse_yahoo_response(body, symbol, interval)
}

/// Construit l'URL de l'API chart
///
/// `^` est encodé (%5E) : les indices comme ^GSPC passent sans ambiguïté dans l'URL.
/// `now` est un paramètre pour rendre la fonction testable.
fn build_yahoo_url(symbol: &str, interval: Interval, now: i64) -> String {
    let period1 = now - interval.history_days() * 86_400;
    let symbol = symbol.replace('^', "%5E");
    format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval={}&period1={period1}&period2={now}",
        interval.to_yahoo_string()
    )
}

/// Convertit la réponse Yahoo en `FetchedTicker`
fn parse_yahoo_response(
    body: YahooResponse,
    symbol: &str,
    interval: Interval,
) -> Result<FetchedTicker> {
    let result = body
        .chart
        .result
        .and_then(|results| results.into_iter().next())
        .with_context(|| format!("{symbol} : aucune donnée renvoyée par Yahoo"))?;

    let meta = result.meta;
    let offset_seconds = meta.gmt_offset.unwrap_or(0);
    let utc_offset = FixedOffset::east_opt(offset_seconds)
        .with_context(|| format!("{symbol} : décalage horaire invalide ({offset_seconds} s)"))?;

    let mut data = OHLCData::new(symbol.to_string(), interval, utc_offset);

    let timestamps = result.timestamp.unwrap_or_default();
    let quote = result
        .indicators
        .quote
        .into_iter()
        .next()
        .with_context(|| format!("{symbol} : pas de données OHLC dans la réponse"))?;
    let opens = quote.open.unwrap_or_default();
    let highs = quote.high.unwrap_or_default();
    let lows = quote.low.unwrap_or_default();
    let closes = quote.close.unwrap_or_default();
    let volumes = quote.volume.unwrap_or_default();

    // CONCEPT RUST : let-else
    // - Yahoo met null dans les colonnes pour les chandelles sans cotation
    // - Si une des 4 valeurs manque, on saute la chandelle
    let value = |column: &[Option<f64>], i: usize| column.get(i).copied().flatten();
    let mut skipped = 0;
    for (i, &timestamp) in timestamps.iter().enumerate() {
        let (Some(open), Some(high), Some(low), Some(close)) = (
            value(&opens, i),
            value(&highs, i),
            value(&lows, i),
            value(&closes, i),
        ) else {
            skipped += 1;
            continue;
        };
        let volume = volumes.get(i).copied().flatten().unwrap_or(0);
        let datetime = DateTime::from_timestamp(timestamp, 0)
            .with_context(|| format!("{symbol} : timestamp invalide {timestamp}"))?;
        data.add_candle(OHLC::new(datetime, open, high, low, close, volume));
    }
    if skipped > 0 {
        warn!(
            skipped,
            total = timestamps.len(),
            "Chandelles incomplètes ignorées"
        );
    }

    let Some(last) = data.last() else {
        bail!("{symbol} : aucune chandelle valide");
    };
    let quote = Quote {
        price: meta.regular_market_price.unwrap_or(last.close),
        previous_close: data.previous_session_close(),
    };

    Ok(FetchedTicker {
        data,
        long_name: meta.long_name,
        quote,
        currency: meta.currency,
        price_decimals: meta.price_hint.unwrap_or(2),
    })
}

// ============================================================================
// Tests unitaires
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture(result: &str) -> YahooResponse {
        serde_json::from_str(&format!(
            r#"{{"chart":{{"result":{result},"error":null}}}}"#
        ))
        .unwrap()
    }

    fn ts(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
            .timestamp()
    }

    #[test]
    fn parses_candles_quote_and_offset() {
        let (t1, t2, t3, t4) = (
            ts(2026, 9, 21, 13, 30),
            ts(2026, 9, 21, 19, 30),
            ts(2026, 9, 22, 13, 30),
            ts(2026, 9, 22, 14, 0),
        );
        let body = fixture(&format!(
            r#"[{{"meta":{{"longName":"Apple Inc.","regularMarketPrice":103.5,"gmtoffset":-14400,"currency":"USD","priceHint":2}},
                "timestamp":[{t1},{t2},{t3},{t4}],
                "indicators":{{"quote":[{{"open":[100,101,null,103],"high":[101,102,103,104],"low":[99,100,101,102],
                                        "close":[100.5,101.5,102.5,103.2],"volume":[1,2,3,null]}}]}}}}]"#
        ));
        let fetched = parse_yahoo_response(body, "AAPL", Interval::M30).unwrap();
        assert_eq!(fetched.data.len(), 3); // la chandelle avec open null est ignorée
        assert_eq!(fetched.data.utc_offset.local_minus_utc(), -14400);
        assert_eq!(
            fetched.quote,
            Quote {
                price: 103.5,
                previous_close: Some(101.5)
            }
        );
        assert_eq!(fetched.long_name.as_deref(), Some("Apple Inc."));
        assert_eq!(
            (fetched.currency.as_deref(), fetched.price_decimals),
            (Some("USD"), 2)
        );
    }

    #[test]
    fn null_result_is_a_readable_error() {
        let err = parse_yahoo_response(fixture("null"), "NOPE", Interval::D1).unwrap_err();
        assert!(err.to_string().contains("NOPE"), "{err}");
    }

    #[test]
    fn all_null_quotes_is_a_readable_error() {
        let t = ts(2026, 9, 22, 0, 0);
        let body = fixture(&format!(
            r#"[{{"meta":{{}},"timestamp":[{t}],"indicators":{{"quote":[{{"open":[null],"high":[null],"low":[null],"close":[null],"volume":[null]}}]}}}}]"#
        ));
        let err = parse_yahoo_response(body, "AAPL", Interval::D1).unwrap_err();
        assert!(err.to_string().contains("AAPL"), "{err}");
    }

    #[test]
    fn url_uses_history_window() {
        let url = build_yahoo_url("^GSPC", Interval::D1, 1_000_000_000);
        assert!(
            url.ends_with("/%5EGSPC?interval=1d&period1=936928000&period2=1000000000"),
            "{url}"
        );
    }

    // Test réseau : exclu par défaut, `cargo test -- --ignored` pour le lancer
    #[tokio::test]
    #[ignore = "appelle Yahoo Finance"]
    async fn fetch_real_ticker() {
        let fetched = fetch_ticker_data(&http_client().unwrap(), "AAPL", Interval::D1)
            .await
            .unwrap();
        assert!(!fetched.data.is_empty());
    }
}
