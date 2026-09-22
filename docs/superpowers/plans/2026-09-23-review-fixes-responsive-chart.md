# Review Fixes + Responsive Chart Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix every issue found in the 2026-09-22 review and rebuild the candlestick chart so it adapts cleanly to any terminal size.

**Architecture:** The worker thread stops touching `App`: it only turns `AppCommand`s into `AppResult`s keyed by symbol, and the main thread owns `App` outright (no `Arc<Mutex>`). Key handling becomes a pure `handle_key(&mut App, KeyEvent, Instant) -> Vec<AppCommand>` routed by screen, so it is unit-testable. The chart becomes a ratatui `Widget` that writes straight into the `Buffer` of its exact inner area. Its three pure helpers (geometry, price axis, time axis) each carry their own tests.

**Tech Stack:** Rust 2021, ratatui 0.26.3, crossterm 0.27, tokio 1.48, reqwest 0.11.27, chrono 0.4.42, dirs 5.

**Spec:** review findings in `README.md` § Known Issues and `docs/candlestick-alignment.md` § Limites Connues (commit `12fb349`), plus the decisions below.

## Decisions (from Cyril, 2026-09-23)

- Wide terminal → **more history**: fixed density, 2 columns per candle (candle + gap) when at least 60 candles fit, else 1. More width means further back in time. Candles are right-aligned, never stretched.
- Time axis in the **exchange's timezone** (Yahoo `meta.gmtoffset`).
- Watchlist persisted in `dirs::config_dir()/lazywallet/watchlist.txt`, one symbol per line; `AAPL`, `TSLA`, `BTC-USD` when the file does not exist. Automatic refresh every **60 s** plus the `r` key.
- Every review remark gets applied, dead code included.

## Global Constraints

- Code comments and doc comments in French, pedagogical tone (`// CONCEPT RUST : ...` where it teaches something); README and commits in English, Conventional Commits.
- No new crate dependency. `dirs` (already declared) becomes used; `serde` stays for Yahoo parsing only.
- No `unwrap()`/`expect()` outside tests; errors reach the UI status line, not only `tracing`.
- Each task ends with `cargo test` green and the tree compiling; `cargo fmt` before each commit.
- Final gate: `cargo test`, `cargo clippy --all-targets -- -D warnings` (pedantic via `[lints]`), `cargo fmt --check`.

## Review Focus

- A ticker deleted while its load is in flight → the late result is ignored; no panic, no data written to another ticker (Task 3).
- Terminal resized to tiny or zero sizes (0×0, 1×1, width < 30, height < 8) → a short message, never a panic or overflow (Task 8).
- Flat price series (all candles equal) or a single candle → no division by zero or NaN; the chart still draws (Task 8).
- Watchlist file with blank lines, lowercase, comments, or an unwritable config dir → sanitized load; the write error shows in the status line and the app keeps running (Task 5).
- Yahoo answering `result: null`, all-null quotes, or no `gmtoffset` → a readable error in the status line, never a panic (Task 2).

## File Map

| File                                            | After this plan                                                    |
| ----------------------------------------------- | ------------------------------------------------------------------ |
| `src/main.rs`                                   | logging, panic hook, terminal setup, event loop only               |
| `src/lib.rs`                                    | `pub mod api, app, models, ui, watchlist_file, worker`             |
| `src/app.rs`                                    | `App` state + transitions returning `AppCommand`s                  |
| `src/worker.rs`                                 | **new**: `AppCommand`, `AppResult`, `spawn_worker`                 |
| `src/watchlist_file.rs`                         | **new**: `default_path`, `load`, `save`                            |
| `src/api/yahoo.rs`                              | `http_client`, `fetch_ticker_data`, parse + tests on JSON fixtures |
| `src/models/ohlc.rs`                            | `Interval`, `OHLC`, `OHLCData` (with `utc_offset`)                 |
| `src/models/watchlist_item.rs`                  | `WatchlistItem`, `Quote`, `FetchedTicker`                          |
| `src/models/ticker.rs`                          | **deleted**                                                        |
| `src/ui/events.rs`                              | `EventHandler`, `handle_key`                                       |
| `src/ui/dashboard.rs`                           | routing, watchlist, footer, input line, `status_line`              |
| `src/ui/chart/mod.rs`                           | **new**: chart screen (header + bordered widget)                   |
| `src/ui/chart/geometry.rs`                      | **new**: slot width, visible candles, columns                      |
| `src/ui/chart/price_axis.rs`                    | **new**: nice ticks, decimals                                      |
| `src/ui/chart/time_axis.rs`                     | **new**: step ladder, buckets, label placement                     |
| `src/ui/chart/widget.rs`                        | **new**: `CandleChart` widget, glyph algorithm, `Scale`            |
| `src/ui/chart.rs`, `src/ui/candlestick_text.rs` | **deleted**                                                        |
| `docs/candlestick-alignment.md`                 | **replaced** by `docs/chart-rendering.md`                          |

---

### Task 1: Green baseline

**Files:** Modify `src/models/ohlc.rs:639-658`, `Cargo.toml`. Delete `.specify/` (untracked, empty spec-kit template).

- [ ] **Step 1: Fix the two stale expectations** — in `test_interval_default_timeframe` expect `D1 → TwoYears`, `W1 → FiveYears`; in `test_ohlcdata_with_interval` expect `SixMonths` for `H1` (these match `default_timeframe()` since commit `3a31684`).
- [ ] **Step 2:** `cargo test` → expected `24 passed; 0 failed`.
- [ ] **Step 3: Declare lints** at the end of `Cargo.toml`:

```toml
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
# Doc comments en français : les mots capitalisés déclenchent des faux positifs
doc_markdown = "allow"
must_use_candidate = "allow"
module_name_repetitions = "allow"
```

- [ ] **Step 4:** `rm -rf .specify && cargo fmt && cargo test` → green.
- [ ] **Step 5: Commit twice** — `test(models): align timeframe tests with current defaults` (ohlc.rs + Cargo.toml), then `style: apply cargo fmt` (everything else). Clippy warnings are cleaned in Task 10.

---

### Task 2: Data model + Yahoo client

Fixes: no HTTP timeout, a new client per request, an unfriendly 404, "daily" change depending on the loaded interval, UTC-only sessions, forex precision, `$` hardcoded.

**Files:** Modify `src/models/ohlc.rs`, `src/models/watchlist_item.rs`, `src/models/mod.rs`, `src/api/yahoo.rs`, `src/api/mod.rs`, `src/main.rs` (call sites only).

**Interfaces — Produces:**

```rust
// models/ohlc.rs
impl Interval { pub fn history_days(self) -> i64 }            // replaces default_timeframe()
pub struct OHLCData { pub symbol: String, pub interval: Interval, pub utc_offset: FixedOffset, pub candles: Vec<OHLC> }
impl OHLCData {
    pub fn new(symbol: String, interval: Interval, utc_offset: FixedOffset) -> Self;
    pub fn local_time(&self, candle: &OHLC) -> DateTime<FixedOffset>;
    pub fn previous_session_close(&self) -> Option<f64>;
}
// models/watchlist_item.rs
pub struct Quote { pub price: f64, pub previous_close: Option<f64> }
impl Quote { pub fn change_percent(&self) -> Option<f64> }
pub struct FetchedTicker { pub data: OHLCData, pub long_name: Option<String>, pub quote: Quote, pub currency: Option<String>, pub price_decimals: usize }
pub struct WatchlistItem { pub symbol: String, pub name: String, pub data: Option<OHLCData>, pub quote: Option<Quote>, pub currency: Option<String>, pub price_decimals: usize }
impl WatchlistItem { pub fn new(symbol: String) -> Self; pub fn apply(&mut self, fetched: FetchedTicker); pub fn current_price(&self) -> Option<f64>; pub fn change_percent(&self) -> Option<f64>; pub fn is_positive(&self) -> bool; pub fn format_price(&self, price: f64) -> String }
// api/yahoo.rs
pub fn http_client() -> Result<reqwest::Client>;
pub async fn fetch_ticker_data(client: &reqwest::Client, symbol: &str, interval: Interval) -> Result<FetchedTicker>;
```

- [ ] **Step 1: Failing model tests** (in `ohlc.rs` tests; replace the timeframe, `daily_change_percent` and `with_interval` tests, which cover code deleted in Step 3):

```rust
fn at(offset: FixedOffset, y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
    offset.with_ymd_and_hms(y, m, d, h, min, 0).unwrap().with_timezone(&Utc)
}

#[test]
fn previous_session_close_uses_exchange_timezone() {
    // New York (UTC-4) : la chandelle de 22:00 locale est le 21/09 en NY mais le 22/09 en UTC
    let ny = FixedOffset::west_opt(4 * 3600).unwrap();
    let mut data = OHLCData::new("AAPL".into(), Interval::M30, ny);
    data.add_candle(OHLC::new(at(ny, 2026, 9, 21, 15, 30), 100.0, 101.0, 99.0, 100.5, 0));
    data.add_candle(OHLC::new(at(ny, 2026, 9, 21, 22, 0), 100.5, 102.0, 100.0, 101.5, 0));
    data.add_candle(OHLC::new(at(ny, 2026, 9, 22, 9, 30), 101.5, 104.0, 101.0, 103.0, 0));
    assert_eq!(data.previous_session_close(), Some(101.5));
}

#[test]
fn previous_session_close_daily_and_weekly() {
    let utc = FixedOffset::east_opt(0).unwrap();
    let mut daily = OHLCData::new("BTC-USD".into(), Interval::D1, utc);
    daily.add_candle(OHLC::new(at(utc, 2026, 9, 21, 0, 0), 1.0, 1.0, 1.0, 10.0, 0));
    daily.add_candle(OHLC::new(at(utc, 2026, 9, 22, 0, 0), 10.0, 12.0, 9.0, 11.0, 0));
    assert_eq!(daily.previous_session_close(), Some(10.0));
    let mut weekly = daily.clone();
    weekly.interval = Interval::W1;
    // Une chandelle hebdo couvre plusieurs séances : pas de clôture de veille fiable
    assert_eq!(weekly.previous_session_close(), None);
}

#[test]
fn history_days_stay_within_yahoo_limits() {
    // Yahoo refuse l'intraday (< 1h) au-delà de 60 jours
    for i in [Interval::M5, Interval::M15, Interval::M30] {
        assert!(i.history_days() < 60);
    }
    assert_eq!(Interval::W1.history_days(), 3650);
}
```

And in `watchlist_item.rs`:

```rust
#[test]
fn quote_change_percent() {
    assert_eq!(Quote { price: 110.0, previous_close: Some(100.0) }.change_percent(), Some(10.0));
    assert_eq!(Quote { price: 110.0, previous_close: None }.change_percent(), None);
    assert_eq!(Quote { price: 110.0, previous_close: Some(0.0) }.change_percent(), None);
}

#[test]
fn apply_keeps_previous_close_when_new_one_is_unknown() {
    let utc = FixedOffset::east_opt(0).unwrap();
    let mut item = WatchlistItem::new("AAPL".into());
    let fetched = |interval, previous_close| FetchedTicker {
        data: OHLCData::new("AAPL".into(), interval, utc),
        long_name: Some("Apple Inc.".into()),
        quote: Quote { price: 110.0, previous_close },
        currency: Some("USD".into()),
        price_decimals: 2,
    };
    item.apply(fetched(Interval::M30, Some(100.0)));
    item.apply(fetched(Interval::W1, None)); // passage en hebdo dans le graphique
    assert_eq!(item.change_percent(), Some(10.0));
    assert_eq!(item.name, "Apple Inc.");
    assert_eq!(item.format_price(1.14532), "1.15 USD");
}
```

- [ ] **Step 2:** `cargo test --lib models` → FAIL (missing `history_days`, `Quote`, 3-arg `OHLCData::new`, ...).
- [ ] **Step 3: Implement the models.** In `ohlc.rs`:
  - delete `Timeframe`, `default_timeframe`, `Interval::all`, `with_interval`, `min_price`, `max_price`, `total_change_percent`, `daily_change_percent`, `OHLC::{is_bearish, body, upper_wick, lower_wick, change_percent}` and every `Serialize/Deserialize` derive;
  - `OHLC::is_bullish` becomes `close >= open` (a doji counts as up, as in the chart);
  - `ticker_type` goes away from `OHLCData` (the chart used it for an unfinished TODO; `TickerType` is deleted in Task 9).

```rust
impl Interval {
    /// Jours d'historique demandés à Yahoo
    ///
    /// CONCEPT : Assez de chandeliers pour remplir un terminal large (~500 pour une action,
    /// qui ne cote que 6h30 par jour), en restant sous la limite Yahoo de 60 jours en intraday.
    pub fn history_days(self) -> i64 {
        match self {
            Interval::M5 => 14,
            Interval::M15 => 30,
            Interval::M30 => 58,
            Interval::H1 => 180,
            Interval::H4 => 365,
            Interval::D1 => 730,
            Interval::W1 => 3650,
        }
    }
}

pub struct OHLCData {
    pub symbol: String,
    pub interval: Interval,
    /// Décalage horaire de la place de cotation (New York : UTC-4 en été)
    pub utc_offset: FixedOffset,
    pub candles: Vec<OHLC>,
}

impl OHLCData {
    pub fn new(symbol: String, interval: Interval, utc_offset: FixedOffset) -> Self {
        Self { symbol, interval, utc_offset, candles: Vec::new() }
    }

    /// Heure de la chandelle à l'heure de la place
    pub fn local_time(&self, candle: &OHLC) -> DateTime<FixedOffset> {
        candle.timestamp.with_timezone(&self.utc_offset)
    }

    /// Clôture de la dernière séance avant celle de la chandelle la plus récente
    ///
    /// CONCEPT : Une "séance" = une date à l'heure de la place, pas en UTC.
    /// None en W1 : une chandelle couvre plusieurs séances.
    pub fn previous_session_close(&self) -> Option<f64> {
        if self.interval == Interval::W1 {
            return None;
        }
        let last_day = self.local_time(self.last()?).date_naive();
        self.candles
            .iter()
            .rev()
            .find(|c| self.local_time(c).date_naive() < last_day)
            .map(|c| c.close)
    }
}
```

ponytail: a fixed offset is taken at fetch time, so a daylight-saving change inside the window shifts older intraday labels by 1 h. Upgrade path: `chrono-tz` with `meta.exchangeTimezoneName`. Write this as a `ponytail:` comment on `utc_offset`.

In `watchlist_item.rs`, replace `display()` (dead code) and the old methods:

```rust
/// Prix courant et clôture de référence pour la variation du jour
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quote {
    pub price: f64,
    pub previous_close: Option<f64>,
}

impl Quote {
    /// Variation depuis la clôture de la séance précédente, en %
    pub fn change_percent(&self) -> Option<f64> {
        let previous = self.previous_close.filter(|p| *p != 0.0)?;
        Some((self.price - previous) / previous * 100.0)
    }
}

/// Tout ce qu'un chargement Yahoo rapporte pour un ticker
#[derive(Debug, Clone)]
pub struct FetchedTicker {
    pub data: OHLCData,
    pub long_name: Option<String>,
    pub quote: Quote,
    pub currency: Option<String>,
    /// Décimales conseillées par Yahoo (`priceHint` : 2 pour une action, 4 pour du forex)
    pub price_decimals: usize,
}

#[derive(Debug, Clone)]
pub struct WatchlistItem {
    pub symbol: String,
    pub name: String,
    pub data: Option<OHLCData>,
    pub quote: Option<Quote>,
    pub currency: Option<String>,
    pub price_decimals: usize,
}

impl WatchlistItem {
    /// Nouvel item sans données : le nom affiché est le symbole jusqu'au premier chargement
    pub fn new(symbol: String) -> Self {
        Self { name: symbol.clone(), symbol, data: None, quote: None, currency: None, price_decimals: 2 }
    }

    /// Intègre un chargement
    ///
    /// CONCEPT : La clôture de référence survit à un chargement qui ne sait pas la calculer
    /// (W1), sinon la variation du jour disparaîtrait en passant le graphique en hebdo.
    pub fn apply(&mut self, fetched: FetchedTicker) {
        if let Some(name) = fetched.long_name {
            self.name = name;
        }
        let previous_close = fetched
            .quote
            .previous_close
            .or(self.quote.and_then(|q| q.previous_close));
        self.quote = Some(Quote { price: fetched.quote.price, previous_close });
        self.currency = fetched.currency;
        self.price_decimals = fetched.price_decimals;
        self.data = Some(fetched.data);
    }

    pub fn current_price(&self) -> Option<f64> {
        self.quote.map(|q| q.price)
    }

    pub fn change_percent(&self) -> Option<f64> {
        self.quote?.change_percent()
    }

    pub fn is_positive(&self) -> bool {
        self.change_percent().is_some_and(|c| c >= 0.0)
    }

    /// Prix avec la précision et la devise de la place ("339.75 USD", "1.1453 USD")
    pub fn format_price(&self, price: f64) -> String {
        match &self.currency {
            Some(currency) => format!("{price:.prec$} {currency}", prec = self.price_decimals),
            None => format!("{price:.prec$}", prec = self.price_decimals),
        }
    }
}
```

Export `Quote` and `FetchedTicker` from `models/mod.rs`; drop the `Timeframe` and `LabelStrategy` re-exports only once nothing uses them (`LabelStrategy`/`AxisFormats` stay until Task 9 because `candlestick_text.rs` still uses them).

- [ ] **Step 4:** `cargo test --lib models` → PASS.
- [ ] **Step 5: Failing Yahoo parse tests** (replace `test_build_yahoo_url` and `test_fetch_ticker_data` in `yahoo.rs`):

```rust
fn fixture(result: &str) -> YahooResponse {
    serde_json::from_str(&format!(r#"{{"chart":{{"result":{result},"error":null}}}}"#)).unwrap()
}

fn ts(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
    Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap().timestamp()
}

#[test]
fn parses_candles_quote_and_offset() {
    let (t1, t2, t3, t4) = (ts(2026, 9, 21, 13, 30), ts(2026, 9, 21, 19, 30), ts(2026, 9, 22, 13, 30), ts(2026, 9, 22, 14, 0));
    let body = fixture(&format!(
        r#"[{{"meta":{{"longName":"Apple Inc.","regularMarketPrice":103.5,"gmtoffset":-14400,"currency":"USD","priceHint":2}},
            "timestamp":[{t1},{t2},{t3},{t4}],
            "indicators":{{"quote":[{{"open":[100,101,null,103],"high":[101,102,103,104],"low":[99,100,101,102],
                                    "close":[100.5,101.5,102.5,103.2],"volume":[1,2,3,null]}}]}}}}]"#
    ));
    let fetched = parse_yahoo_response(body, "AAPL", Interval::M30).unwrap();
    assert_eq!(fetched.data.len(), 3); // la chandelle avec open null est ignorée
    assert_eq!(fetched.data.utc_offset.local_minus_utc(), -14400);
    assert_eq!(fetched.quote, Quote { price: 103.5, previous_close: Some(101.5) });
    assert_eq!(fetched.long_name.as_deref(), Some("Apple Inc."));
    assert_eq!((fetched.currency.as_deref(), fetched.price_decimals), (Some("USD"), 2));
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
    assert!(url.ends_with("/%5EGSPC?interval=1d&period1=936928000&period2=1000000000"), "{url}");
}

// Test réseau : exclu par défaut, `cargo test -- --ignored` pour le lancer
#[tokio::test]
#[ignore = "appelle Yahoo Finance"]
async fn fetch_real_ticker() {
    let fetched = fetch_ticker_data(&http_client().unwrap(), "AAPL", Interval::D1).await.unwrap();
    assert!(!fetched.data.is_empty());
}
```

- [ ] **Step 6:** `cargo test --lib yahoo` → FAIL.
- [ ] **Step 7: Implement the client.** Key changes to `yahoo.rs`:

```rust
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

#[derive(Debug, Deserialize)]
struct Chart {
    /// null quand Yahoo ne connaît pas le symbole
    result: Option<Vec<ChartResult>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    long_name: Option<String>,
    regular_market_price: Option<f64>,
    currency: Option<String>,
    price_hint: Option<usize>,
    /// Décalage de la place en secondes (champ Yahoo tout en minuscules)
    #[serde(rename = "gmtoffset")]
    gmt_offset: Option<i32>,
}

/// Client HTTP partagé par tous les appels (pool de connexions réutilisé)
pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(10))
        .build()
        .context("Échec de la création du client HTTP")
}

pub async fn fetch_ticker_data(client: &reqwest::Client, symbol: &str, interval: Interval) -> Result<FetchedTicker> {
    let url = build_yahoo_url(symbol, interval, Utc::now().timestamp());
    let response = client.get(&url).send().await
        .with_context(|| format!("{symbol} : Yahoo Finance injoignable"))?;
    match response.status() {
        StatusCode::NOT_FOUND => bail!("{symbol} : symbole inconnu de Yahoo Finance"),
        status if !status.is_success() => bail!("{symbol} : Yahoo Finance a répondu {status}"),
        _ => {}
    }
    let body: YahooResponse = response.json().await
        .with_context(|| format!("{symbol} : réponse Yahoo illisible"))?;
    parse_yahoo_response(body, symbol, interval)
}

/// `^` est encodé (%5E) : les indices comme ^GSPC passent sans ambiguïté dans l'URL
fn build_yahoo_url(symbol: &str, interval: Interval, now: i64) -> String {
    let period1 = now - interval.history_days() * 86_400;
    let symbol = symbol.replace('^', "%5E");
    format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval={}&period1={period1}&period2={now}",
        interval.to_yahoo_string()
    )
}
```

In `parse_yahoo_response(body, symbol, interval) -> Result<FetchedTicker>`:

- first result: `body.chart.result.and_then(|r| r.into_iter().next()).with_context(|| format!("{symbol} : aucune donnée renvoyée par Yahoo"))?`;
- `utc_offset = FixedOffset::east_opt(meta.gmt_offset.unwrap_or(0)).with_context(...)?`;
- candle loop rewritten with `let (Some(open), Some(high), Some(low), Some(close)) = (...) else { skipped += 1; continue; };`;
- empty data → `bail!("{symbol} : aucune chandelle valide")`;
- `price = meta.regular_market_price.unwrap_or(last.close)`;
- `quote = Quote { price, previous_close: data.previous_session_close() }`;
- `price_decimals = meta.price_hint.unwrap_or(2)`.
- [ ] **Step 8: Update call sites** so everything compiles. Delete `src/ui/chart.rs` (dead code that used `Timeframe`) and its `pub mod chart;`. In `candlestick_text.rs`, pass `TickerType::from_symbol(&data.symbol)` where it read `data.ticker_type`, and build the title with `data.interval.label()` instead of `data.timeframe.label()`. In `main.rs`: the worker builds `let client = http_client()?` before `thread::spawn` and calls `fetch_ticker_data(&client, ...)`; `TickerDataLoaded` sets `item.apply(fetched)`; startup and `TickerAdded` build items with `WatchlistItem::new` + `apply`. Minimal glue, since Task 3 rewrites this file.
- [ ] **Step 9:** `cargo test` → all green (the network test shows as `ignored`). `cargo build` → OK. `rg -n "Timeframe|daily_change_percent|ticker_type" src` → only `candlestick_text.rs` hits via `TickerType::from_symbol` (removed in Task 9).
- [ ] **Step 10:** `cargo fmt`, commit `fix(data): per-exchange sessions, shared HTTP client with timeout, readable Yahoo errors`.

---

### Task 3: Worker by symbol + App owns state

Fixes: results addressed by index, `Arc<Mutex<App>>`, a single loading flag shared by concurrent commands, errors only logged, UI blocked during the initial load, no refresh, interval mismatch when opening another chart, swallowed event errors, one result per frame, no panic hook, mouse capture blocking text selection.

**Files:** Create `src/worker.rs`. Modify `src/app.rs` (rewrite), `src/lib.rs`, `src/main.rs` (rewrite of `run`, `main`, worker removal).

**Interfaces — Consumes:** Task 2's `FetchedTicker`, `WatchlistItem::apply`, `http_client`, `fetch_ticker_data`. **Produces:**

```rust
// worker.rs
pub enum AppCommand { Load { symbol: String, interval: Interval }, Add { symbol: String } }
pub enum AppResult { Loaded { symbol: String, fetched: FetchedTicker }, Added { symbol: String, fetched: FetchedTicker }, Failed { error: String } }
pub fn spawn_worker(commands: Receiver<AppCommand>, results: Sender<AppResult>) -> Result<()>;
// app.rs
pub const REFRESH_EVERY: Duration;      // 60 s
pub const STATUS_TTL: Duration;         // 5 s
pub struct Status { pub text: String, pub is_error: bool, pub since: Instant }
pub struct App { pub running: bool, pub watchlist: Vec<WatchlistItem>, pub selected_index: usize, pub current_screen: Screen,
                 pub current_interval: Interval, pub confirm_quit: bool, pub confirm_delete: bool, pub pending: usize,
                 pub status: Option<Status>, pub input_buffer: String, pub watchlist_path: Option<PathBuf>, last_refresh: Instant }
impl App {
    pub fn new(symbols: Vec<String>, watchlist_path: Option<PathBuf>, now: Instant) -> Self;
    pub fn initial_commands(&self) -> Vec<AppCommand>;
    pub fn refresh_commands(&mut self, now: Instant) -> Vec<AppCommand>;
    pub fn tick(&mut self, now: Instant) -> Vec<AppCommand>;
    pub fn apply_result(&mut self, result: AppResult, now: Instant);
    pub fn is_loading(&self) -> bool;
    pub fn open_chart(&mut self) -> Option<AppCommand>;
    pub fn change_interval(&mut self, step: fn(Interval) -> Interval) -> Option<AppCommand>;
    pub fn delete_selected(&mut self, now: Instant);
    pub fn request_add(&mut self, symbol: &str, now: Instant) -> Option<AppCommand>;
    pub fn set_status(&mut self, text: String, now: Instant);
    pub fn set_error(&mut self, text: String, now: Instant);
    pub fn selected_item(&self) -> Option<&WatchlistItem>;
    // + navigate_up/down, show_dashboard, start_input, cancel_input, take_input, quit, is_running (kept)
}
```

- [ ] **Step 1: Failing App tests** (replace the old ones in `app.rs`; the helper builds an item with data without going through the network):

```rust
fn fetched(symbol: &str, interval: Interval) -> FetchedTicker {
    let utc = FixedOffset::east_opt(0).unwrap();
    let mut data = OHLCData::new(symbol.into(), interval, utc);
    data.add_candle(OHLC::new(Utc::now(), 1.0, 2.0, 0.5, 1.5, 0));
    FetchedTicker { data, long_name: None, quote: Quote { price: 1.5, previous_close: Some(1.0) }, currency: None, price_decimals: 2 }
}

fn app(symbols: &[&str]) -> (App, Instant) {
    let now = Instant::now();
    (App::new(symbols.iter().map(|s| s.to_string()).collect(), None, now), now)
}

#[test]
fn late_result_for_deleted_ticker_is_ignored() {
    let (mut app, now) = app(&["AAPL", "TSLA"]);
    app.pending = 1;
    app.delete_selected(now); // supprime AAPL pendant que son chargement est en vol
    app.apply_result(AppResult::Loaded { symbol: "AAPL".into(), fetched: fetched("AAPL", Interval::M30) }, now);
    assert_eq!(app.watchlist.len(), 1);
    assert!(app.watchlist[0].data.is_none(), "TSLA ne doit pas recevoir les données d'AAPL");
    assert_eq!(app.pending, 0);
}

#[test]
fn failure_reaches_status_line() {
    let (mut app, now) = app(&["AAPL"]);
    app.pending = 1;
    app.apply_result(AppResult::Failed { error: "NOPE : symbole inconnu".into() }, now);
    let status = app.status.as_ref().unwrap();
    assert!(status.is_error && status.text.contains("NOPE"));
    assert!(!app.is_loading());
}

#[test]
fn added_ticker_is_appended_once() {
    let (mut app, now) = app(&["AAPL"]);
    app.apply_result(AppResult::Added { symbol: "TSLA".into(), fetched: fetched("TSLA", Interval::M30) }, now);
    app.apply_result(AppResult::Added { symbol: "TSLA".into(), fetched: fetched("TSLA", Interval::M30) }, now);
    assert_eq!(app.watchlist.iter().map(|i| i.symbol.as_str()).collect::<Vec<_>>(), ["AAPL", "TSLA"]);
}

#[test]
fn request_add_rejects_duplicates_and_blank() {
    let (mut app, now) = app(&["AAPL"]);
    assert!(app.request_add("  ", now).is_none());
    assert!(app.request_add("aapl", now).is_none());
    assert!(app.status.as_ref().unwrap().is_error);
    assert!(matches!(app.request_add("qqq", now), Some(AppCommand::Add { symbol }) if symbol == "QQQ"));
}

#[test]
fn opening_chart_reloads_when_interval_differs() {
    let (mut app, now) = app(&["AAPL"]);
    app.apply_result(AppResult::Loaded { symbol: "AAPL".into(), fetched: fetched("AAPL", Interval::M30) }, now);
    assert!(app.open_chart().is_none(), "données déjà en 30m");
    app.current_interval = Interval::D1;
    assert!(matches!(app.open_chart(), Some(AppCommand::Load { interval: Interval::D1, .. })));
}

#[test]
fn tick_refreshes_every_minute_when_idle() {
    let (mut app, now) = app(&["AAPL", "TSLA"]);
    assert!(app.tick(now + Duration::from_secs(59)).is_empty());
    assert_eq!(app.tick(now + REFRESH_EVERY).len(), 2);
    assert!(app.tick(now + REFRESH_EVERY + Duration::from_secs(1)).is_empty(), "compteur remis à zéro");
    app.pending = 1;
    assert!(app.tick(now + REFRESH_EVERY * 3).is_empty(), "pas d'empilement pendant un chargement");
}

#[test]
fn status_expires() {
    let (mut app, now) = app(&["AAPL"]);
    app.set_status("ok".into(), now);
    app.tick(now + STATUS_TTL);
    assert!(app.status.is_none());
}

#[test]
fn delete_keeps_selection_in_range() {
    let (mut app, now) = app(&["AAPL", "TSLA"]);
    app.navigate_down();
    app.delete_selected(now);
    assert_eq!(app.selected_index, 0);
    app.delete_selected(now);
    assert!(app.watchlist.is_empty() && app.selected_index == 0);
    app.delete_selected(now); // liste vide : aucun panic
}
```

- [ ] **Step 2:** `cargo test --lib app` → FAIL.
- [ ] **Step 3: Create `src/worker.rs`:**

```rust
//! Thread réseau : transforme des commandes en résultats
//!
//! CONCEPT RUST : Le worker ne touche jamais à `App`. Il reçoit des `AppCommand`,
//! renvoie exactement un `AppResult` par commande, et le thread principal reste
//! seul propriétaire de l'état : plus besoin d'`Arc<Mutex<App>>`.

use std::sync::mpsc::{Receiver, Sender};
use anyhow::{Context, Result};
use crate::api::yahoo::{fetch_ticker_data, http_client};
use crate::models::{FetchedTicker, Interval};

#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    /// Charge (ou recharge) un ticker de la watchlist
    Load { symbol: String, interval: Interval },
    /// Vérifie qu'un nouveau symbole existe et récupère ses données
    Add { symbol: String },
}

#[derive(Debug)]
pub enum AppResult {
    Loaded { symbol: String, fetched: FetchedTicker },
    Added { symbol: String, fetched: FetchedTicker },
    /// Le message contient déjà le symbole concerné
    Failed { error: String },
}

pub fn spawn_worker(commands: Receiver<AppCommand>, results: Sender<AppResult>) -> Result<()> {
    // Créés avant le thread : une erreur remonte à main() au lieu d'un panic dans le worker
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
            AppResult::Failed { error: format!("{error:#}") }
        }
    }
}
```

- [ ] **Step 4: Rewrite `src/app.rs`** around the Interfaces above. The non-obvious bodies:

```rust
pub fn initial_commands(&self) -> Vec<AppCommand> {
    self.watchlist
        .iter()
        .map(|item| AppCommand::Load { symbol: item.symbol.clone(), interval: Interval::default() })
        .collect()
}

/// Recharge chaque ticker dans l'intervalle déjà chargé pour lui
pub fn refresh_commands(&mut self, now: Instant) -> Vec<AppCommand> {
    self.last_refresh = now;
    self.watchlist
        .iter()
        .map(|item| AppCommand::Load {
            symbol: item.symbol.clone(),
            interval: item.data.as_ref().map_or(Interval::default(), |d| d.interval),
        })
        .collect()
}

pub fn tick(&mut self, now: Instant) -> Vec<AppCommand> {
    if self.status.as_ref().is_some_and(|s| now.duration_since(s.since) >= STATUS_TTL) {
        self.status = None;
    }
    if self.pending == 0 && now.duration_since(self.last_refresh) >= REFRESH_EVERY {
        return self.refresh_commands(now);
    }
    Vec::new()
}

pub fn apply_result(&mut self, result: AppResult, now: Instant) {
    self.pending = self.pending.saturating_sub(1);
    match result {
        // Ticker supprimé entre-temps : on jette le résultat
        AppResult::Loaded { symbol, fetched } => {
            if let Some(item) = self.watchlist.iter_mut().find(|i| i.symbol == symbol) {
                item.apply(fetched);
            }
        }
        AppResult::Added { symbol, fetched } => {
            if self.watchlist.iter().all(|i| i.symbol != symbol) {
                let mut item = WatchlistItem::new(symbol.clone());
                item.apply(fetched);
                self.watchlist.push(item);
                self.set_status(format!("{symbol} ajouté"), now);
                self.save_watchlist(now);
            }
        }
        AppResult::Failed { error } => self.set_error(error, now),
    }
}

/// Ouvre le graphique ; recharge si les données ne sont pas dans l'intervalle choisi
pub fn open_chart(&mut self) -> Option<AppCommand> {
    let item = self.selected_item()?;
    self.current_screen = Screen::ChartView;
    let loaded = item.data.as_ref().map(|d| d.interval);
    (loaded != Some(self.current_interval))
        .then(|| AppCommand::Load { symbol: item.symbol.clone(), interval: self.current_interval })
}

pub fn change_interval(&mut self, step: fn(Interval) -> Interval) -> Option<AppCommand> {
    self.current_interval = step(self.current_interval);
    let symbol = self.selected_item()?.symbol.clone();
    Some(AppCommand::Load { symbol, interval: self.current_interval })
}

pub fn request_add(&mut self, symbol: &str, now: Instant) -> Option<AppCommand> {
    let symbol = symbol.trim().to_uppercase();
    if symbol.is_empty() {
        return None;
    }
    if self.watchlist.iter().any(|i| i.symbol == symbol) {
        self.set_error(format!("{symbol} est déjà dans la watchlist"), now);
        return None;
    }
    Some(AppCommand::Add { symbol })
}
```

`delete_selected(now)` removes the item, clamps `selected_index` with `saturating_sub`, then calls `save_watchlist(now)`. In this task `save_watchlist(&mut self, _now: Instant)` has an empty body with the comment `// Écriture du fichier : Task 5`; Task 5 replaces it. `Interval::next/previous` take `self` by value so they fit `fn(Interval) -> Interval`. `App::default()` and `App::with_watchlist` are deleted (unused). `is_loading()` is `self.pending > 0`.

- [ ] **Step 5:** `cargo test --lib app` → PASS.
- [ ] **Step 6: Rewrite `main.rs` around the loop** (keep `init_logging` and the pedagogical header; delete `load_watchlist_data`, `spawn_background_worker`, the local `AppCommand/AppResult`; `handle_event` stays until Task 4 but takes `&mut App` and returns `Vec<AppCommand>` instead of sending):

```rust
fn main() -> Result<()> {
    init_logging().unwrap_or_else(|e| eprintln!("⚠️  Logging désactivé : {e:#}"));
    let (command_tx, command_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    spawn_worker(command_rx, result_tx)?;

    let symbols = vec!["AAPL".into(), "TSLA".into(), "BTC-USD".into()]; // Task 5 : fichier
    let mut app = App::new(symbols, None, Instant::now());

    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal, &mut app, &command_tx, &result_rx);
    restore_terminal(&mut terminal)?;
    result
}

/// CONCEPT : Boucle principale — résultats → rendu → événements → envoi des commandes
fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App,
       commands: &Sender<AppCommand>, results: &Receiver<AppResult>) -> Result<()> {
    let events = EventHandler;
    let initial = app.initial_commands();
    send_all(app, commands, initial)?;
    while app.is_running() {
        // Vide tous les résultats arrivés, pas un seul par image
        loop {
            match results.try_recv() {
                Ok(result) => app.apply_result(result, Instant::now()),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => bail!("Le thread réseau s'est arrêté"),
            }
        }
        terminal.draw(|frame| render(frame, app))?;
        let now = Instant::now();
        let mut pending = app.tick(now);
        if let Some(key) = events.next()? {
            pending.extend(handle_event(app, key, now));
        }
        send_all(app, commands, pending)?;
    }
    Ok(())
}

fn send_all(app: &mut App, commands: &Sender<AppCommand>, batch: Vec<AppCommand>) -> Result<()> {
    for command in batch {
        commands.send(command).context("Le thread réseau s'est arrêté")?;
        app.pending += 1;
    }
    Ok(())
}

/// Restaure le terminal avant d'afficher un panic, sinon le message est illisible
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best effort : pendant un panic, une erreur de restauration n'a nulle part où aller
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));
}
```

`EventHandler::next` becomes `pub fn next(&self) -> Result<Option<KeyEvent>>` (`None` on timeout, a non-key event, or a key release). Delete `Event`/`Event::Error`. `setup_terminal`/`restore_terminal` drop `EnableMouseCapture`/`DisableMouseCapture` (unused; it blocked terminal text selection).

- [ ] **Step 7:** `cargo test && cargo build` → green. Quick manual smoke: `cargo run` in tmux (`tmux new -d -s lw -x 120 -y 30 'cargo run'; sleep 8; tmux capture-pane -pt lw; tmux kill-session -t lw`). The UI must appear immediately, then prices fill in.
- [ ] **Step 8:** `cargo fmt`, commit `refactor(app): symbol-addressed worker results and single-owner app state`, body listing the fixed issues.

---

### Task 4: Keyboard routing by screen + dashboard feedback

Fixes: `q`/`Q` in input quitting the app, `=`/`^` rejected, confirmation surviving other keys, no loading or error display on the dashboard. Adds `r` and Ctrl+C.

**Files:** Modify `src/ui/events.rs` (rewrite), `src/ui/dashboard.rs`, `src/main.rs` (use `handle_key`, delete `handle_event`), `src/app.rs` (`start_input`, `cancel_input`, `take_input`; drop `input_prompt`).

**Interfaces — Produces:** `pub fn handle_key(app: &mut App, key: KeyEvent, now: Instant) -> Vec<AppCommand>`; `pub(crate) fn status_line(app: &App) -> Option<Line<'static>>` in `dashboard.rs` (Task 9 reuses it in the chart header).

- [ ] **Step 1: Failing tests** in `events.rs`:

```rust
fn key(c: char) -> KeyEvent { KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE) }
fn code(k: KeyCode) -> KeyEvent { KeyEvent::new(k, KeyModifiers::NONE) }
fn app() -> App { App::new(vec!["AAPL".into(), "TSLA".into()], None, Instant::now()) }

fn type_str(app: &mut App, s: &str) -> Vec<AppCommand> {
    s.chars().flat_map(|c| handle_key(app, key(c), Instant::now())).collect()
}

#[test]
fn typing_qqq_adds_instead_of_quitting() {
    let mut app = app();
    handle_key(&mut app, key('a'), Instant::now());
    type_str(&mut app, "qqq");
    let commands = handle_key(&mut app, code(KeyCode::Enter), Instant::now());
    assert!(app.is_running());
    assert_eq!(commands, [AppCommand::Add { symbol: "QQQ".into() }]);
}

#[test]
fn forex_and_index_symbols_can_be_typed() {
    let mut app = app();
    handle_key(&mut app, key('a'), Instant::now());
    type_str(&mut app, "eurusd=x");
    assert_eq!(app.input_buffer, "EURUSD=X");
    app.input_buffer.clear();
    type_str(&mut app, "^gspc");
    assert_eq!(app.input_buffer, "^GSPC");
}

#[test]
fn any_other_key_cancels_pending_delete() {
    let mut app = app();
    handle_key(&mut app, key('d'), Instant::now());
    handle_key(&mut app, key('a'), Instant::now()); // ouvre la saisie : annule la confirmation
    handle_key(&mut app, code(KeyCode::Esc), Instant::now());
    handle_key(&mut app, key('d'), Instant::now());
    assert_eq!(app.watchlist.len(), 2, "un seul d ne doit pas supprimer");
    handle_key(&mut app, key('d'), Instant::now());
    assert_eq!(app.watchlist.len(), 1);
}

#[test]
fn quit_needs_two_presses_and_ctrl_c_is_immediate() {
    let mut app = app();
    handle_key(&mut app, key('q'), Instant::now());
    assert!(app.is_running());
    handle_key(&mut app, key('q'), Instant::now());
    assert!(!app.is_running());
    let mut app2 = self::app();
    handle_key(&mut app2, KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), Instant::now());
    assert!(!app2.is_running());
}

#[test]
fn interval_keys_only_on_chart() {
    let mut app = app();
    assert!(handle_key(&mut app, key('l'), Instant::now()).is_empty());
    handle_key(&mut app, code(KeyCode::Enter), Instant::now()); // ouvre le graphique (+ Load)
    let commands = handle_key(&mut app, key('l'), Instant::now());
    assert_eq!(commands, [AppCommand::Load { symbol: "AAPL".into(), interval: Interval::H1 }]);
}

#[test]
fn refresh_key_reloads_everything() {
    let mut app = app();
    assert_eq!(handle_key(&mut app, key('r'), Instant::now()).len(), 2);
}
```

- [ ] **Step 2:** `cargo test --lib events` → FAIL.
- [ ] **Step 3: Implement** (the `is_*_event` helpers are deleted; the match reads directly):

```rust
/// Traduit une touche en changement d'état + commandes réseau
///
/// CONCEPT : Routage par écran d'abord. En saisie, toutes les lettres vont au
/// buffer — c'est ce qui empêche `q` de quitter pendant qu'on tape "QQQ".
pub fn handle_key(app: &mut App, key: KeyEvent, now: Instant) -> Vec<AppCommand> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.quit();
        return Vec::new();
    }
    if app.current_screen == Screen::InputMode {
        return handle_input_key(app, key, now);
    }
    // Toute touche annule une confirmation en cours ; on retient laquelle était active
    let confirming_quit = std::mem::take(&mut app.confirm_quit);
    let confirming_delete = std::mem::take(&mut app.confirm_delete);
    let on_chart = app.current_screen == Screen::ChartView;
    match key.code {
        KeyCode::Char('q' | 'Q') if confirming_quit => app.quit(),
        KeyCode::Char('q' | 'Q') => app.confirm_quit = true,
        KeyCode::Char('r' | 'R') => return app.refresh_commands(now),
        KeyCode::Esc | KeyCode::Char(' ') if on_chart => app.show_dashboard(),
        KeyCode::Char('l' | 'L') if on_chart => return app.change_interval(Interval::next).into_iter().collect(),
        KeyCode::Char('h' | 'H') if on_chart => return app.change_interval(Interval::previous).into_iter().collect(),
        _ if on_chart => {}
        KeyCode::Up | KeyCode::Char('k' | 'K') => app.navigate_up(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => app.navigate_down(),
        KeyCode::Enter => return app.open_chart().into_iter().collect(),
        KeyCode::Char('a' | 'A') => app.start_input(),
        KeyCode::Char('d' | 'D') if confirming_delete => app.delete_selected(now),
        KeyCode::Char('d' | 'D') if app.selected_item().is_some() => app.confirm_delete = true,
        _ => {}
    }
    Vec::new()
}

fn handle_input_key(app: &mut App, key: KeyEvent, now: Instant) -> Vec<AppCommand> {
    match key.code {
        KeyCode::Esc => app.cancel_input(),
        KeyCode::Enter => {
            let symbol = app.take_input();
            return app.request_add(&symbol, now).into_iter().collect();
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Char(c)
            if is_ticker_char(c) && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            app.input_buffer.push(c.to_ascii_uppercase());
        }
        _ => {}
    }
    Vec::new()
}

/// Caractères des symboles Yahoo : AAPL, BTC-USD, BRK.B, EURUSD=X, ^GSPC
fn is_ticker_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '=' | '^')
}
```

- [ ] **Step 4: Dashboard feedback.** In `dashboard.rs`:
  - `status_line(app)` returns, by priority: delete confirmation, then quit confirmation (existing texts), then `status` (red if `is_error`, green otherwise), then `⏳ Chargement… ({pending})` in cyan when `is_loading()`, else `None`.
  - Footer shows `status_line` if any, else the shortcuts with `[r] Refresh` added.
  - Header text: `format!("{} tickers · rafraîchi toutes les {} s", n, REFRESH_EVERY.as_secs())`. The rocket banner goes.
  - Rows: price via `item.format_price`. Without a quote, show `…` while `app.is_loading()`, else `N/A`. Name truncated to 20 chars with `truncate_with_ellipsis` (keep).
  - Input footer prompt is the literal `"Ajouter : "` (`input_prompt` field deleted).
- [ ] **Step 5:** Replace `handle_event` in `main.rs` with `handle_key`. Run `cargo test`, then the tmux smoke: type `a`, `qqq`, Enter → footer shows `⏳`, then `QQQ ajouté`; `a`, `nope123`, Enter → red `NOPE123 : symbole inconnu de Yahoo Finance`.
- [ ] **Step 6:** `cargo fmt`, commit `fix(ui): route keys by screen so ticker input never triggers shortcuts`.

---

### Task 5: Persisted watchlist

**Files:** Create `src/watchlist_file.rs`. Modify `src/lib.rs`, `src/app.rs` (`save_watchlist`), `src/main.rs`.

**Interfaces — Produces:** `pub fn default_path() -> Result<PathBuf>`, `pub fn load(path: &Path) -> Result<Vec<String>>`, `pub fn save(path: &Path, symbols: &[&str]) -> Result<()>`, `pub const DEFAULT_SYMBOLS: [&str; 3]`.

- [ ] **Step 1: Failing tests** (no `tempfile` crate: unique paths under `std::env::temp_dir()`):

```rust
fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("lazywallet-test-{}-{name}", std::process::id())).join("watchlist.txt")
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
```

And in `app.rs`, a test that a save failure lands in the status line without panicking. Point `watchlist_path` at `blocker/watchlist.txt` built the same way, call `delete_selected`, then assert `status.is_error`.

- [ ] **Step 2:** `cargo test --lib watchlist_file` → FAIL.
- [ ] **Step 3: Implement:**

```rust
//! Persistance de la watchlist : un symbole par ligne, lisible et éditable à la main

pub const DEFAULT_SYMBOLS: [&str; 3] = ["AAPL", "TSLA", "BTC-USD"];

pub fn default_path() -> Result<PathBuf> {
    let config = dirs::config_dir().context("Répertoire de configuration introuvable")?;
    Ok(config.join("lazywallet").join("watchlist.txt"))
}

/// Lignes vides et `#` ignorées, symboles en majuscules, doublons retirés
pub fn load(path: &Path) -> Result<Vec<String>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(DEFAULT_SYMBOLS.map(String::from).to_vec()),
        Err(e) => return Err(e).with_context(|| format!("Lecture de {}", path.display())),
    };
    let mut symbols: Vec<String> = Vec::new();
    for symbol in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let symbol = symbol.to_uppercase();
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    Ok(symbols)
}

pub fn save(path: &Path, symbols: &[&str]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("Création de {}", dir.display()))?;
    }
    let mut text = symbols.join("\n");
    text.push('\n');
    fs::write(path, text).with_context(|| format!("Écriture de {}", path.display()))
}
```

`App::save_watchlist(now)`: if `watchlist_path` is `Some`, build the `&str` list and call `save`; `Err(e)` → `self.set_error(format!("{e:#}"), now)`. `main.rs`: `let path = watchlist_file::default_path()?; let symbols = watchlist_file::load(&path)?; App::new(symbols, Some(path), now)`.

- [ ] **Step 4:** `cargo test` → green. Smoke: add `QQQ`, quit, relaunch → `QQQ` still there; `cat ~/.config/lazywallet/watchlist.txt`.
- [ ] **Step 5:** `cargo fmt`, commit `feat(app): persist watchlist in the user config directory`.

---

### Task 6: Price axis (pure)

**Files:** Create `src/ui/chart/mod.rs` (only `pub mod price_axis;` for now) and `src/ui/chart/price_axis.rs`; `ui/mod.rs` declares `pub mod chart;` (the old `ui/chart.rs` was deleted in Task 2).

**Interfaces — Produces:**

```rust
pub fn nice_step(raw: f64) -> f64;
pub fn decimals_for(step: f64) -> usize;
pub struct PriceTicks { pub step: f64, pub decimals: usize, pub values: Vec<f64> }
impl PriceTicks { pub fn format(&self, price: f64) -> String }
pub fn price_ticks(min: f64, max: f64, rows: u16) -> PriceTicks;   // requires min < max, rows ≥ 1
pub const ROWS_PER_TICK: u16 = 4;
```

- [ ] **Step 1: Failing tests:**

```rust
#[test]
fn nice_steps() {
    for (raw, expected) in [(0.7, 1.0), (1.3, 2.0), (3.0, 5.0), (7.0, 10.0), (0.013, 0.02), (2.88, 5.0), (1_234.0, 2_000.0)] {
        assert!((nice_step(raw) - expected).abs() < 1e-9, "{raw} → {}", nice_step(raw));
    }
}

#[test]
fn decimals_follow_step() {
    assert_eq!(decimals_for(5.0), 0);
    assert_eq!(decimals_for(0.1), 1);
    assert_eq!(decimals_for(0.02), 2);
    assert_eq!(decimals_for(0.0005), 4);
}

#[test]
fn ticks_are_round_and_inside_range() {
    let ticks = price_ticks(98.3, 112.7, 20);
    assert_eq!(ticks.values, [100.0, 105.0, 110.0]);
    assert_eq!(ticks.format(105.0), "105");
    let fx = price_ticks(1.1402, 1.1466, 16);
    assert_eq!(fx.decimals, 3);
    assert!(fx.values.iter().all(|v| (1.1402..=1.1466).contains(v)));
}

#[test]
fn single_row_still_gives_a_step() {
    let ticks = price_ticks(10.0, 11.0, 1);
    assert!(ticks.step > 0.0 && !ticks.values.is_empty());
}
```

- [ ] **Step 2:** `cargo test --lib price_axis` → FAIL.
- [ ] **Step 3: Implement:**

```rust
//! Graduations de l'axe des prix : des valeurs "rondes" (1, 2, 5 × 10^k)

/// Une graduation toutes les 4 lignes environ
pub const ROWS_PER_TICK: u16 = 4;

/// Plus petit pas rond ≥ `raw`
pub fn nice_step(raw: f64) -> f64 {
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice = if normalized <= 1.0 { 1.0 } else if normalized <= 2.0 { 2.0 } else if normalized <= 5.0 { 5.0 } else { 10.0 };
    nice * magnitude
}

/// Décimales nécessaires pour écrire les multiples de `step` sans perte
///
/// CONCEPT : On teste 10^d × step plutôt que -log10(step) : les flottants
/// (0.1 n'est pas exact en binaire) feraient tomber log10 du mauvais côté.
pub fn decimals_for(step: f64) -> usize {
    (0..8)
        .find(|&d| {
            let scaled = step * 10f64.powi(d);
            (scaled - scaled.round()).abs() < 1e-6 * scaled.max(1.0)
        })
        .map_or(8, |d| d as usize)
}

pub struct PriceTicks { pub step: f64, pub decimals: usize, pub values: Vec<f64> }

impl PriceTicks {
    pub fn format(&self, price: f64) -> String {
        format!("{price:.prec$}", prec = self.decimals)
    }
}

/// Graduations rondes dans [min, max]
pub fn price_ticks(min: f64, max: f64, rows: u16) -> PriceTicks {
    let wanted = (rows / ROWS_PER_TICK).max(1);
    let step = nice_step((max - min) / f64::from(wanted));
    // Multiples entiers du pas : pas d'accumulation d'erreurs d'arrondi
    let first = (min / step).ceil() as i64;
    let last = (max / step).floor() as i64;
    let values = (first..=last).map(|k| k as f64 * step).collect();
    PriceTicks { step, decimals: decimals_for(step), values }
}
```

- [ ] **Step 4:** `cargo test --lib price_axis` → PASS; `cargo build` → OK.
- [ ] **Step 5:** `cargo fmt`, commit `feat(chart): round price-axis ticks with adaptive precision`.

---

### Task 7: Time axis (pure)

Fixes: 1h axis without labels, fixed width thresholds (`width < 140`), labels in UTC.

**Files:** Create `src/ui/chart/time_axis.rs`; add `pub mod time_axis;` to `chart/mod.rs`.

**Interfaces — Produces:**

```rust
pub enum Step { Minutes(i64), Hours(i64), Days(i64), Weeks(i64), Months(i64), Years(i64) }
pub struct Label { pub column: u16, pub text: String }
pub struct TimeAxis { pub primary: Vec<Label>, pub secondary: Vec<Label> }
pub fn time_axis(times: &[DateTime<FixedOffset>], columns: &[u16], width: u16) -> TimeAxis;
```

`columns[i]` is the column of candle `i`, strictly increasing, all `< width`.

- [ ] **Step 1: Failing tests:**

```rust
fn series(offset: FixedOffset, start: DateTime<FixedOffset>, every: Duration, n: usize, market_hours: Option<(u32, u32)>) -> Vec<DateTime<FixedOffset>> {
    let mut out = Vec::new();
    let mut t = start;
    while out.len() < n {
        let open = market_hours.map_or(true, |(from, to)| {
            (from..to).contains(&t.hour()) && t.weekday().num_days_from_monday() < 5
        });
        if open { out.push(t.with_timezone(&offset)); }
        t += every;
    }
    out
}
fn cols(n: usize, slot: u16) -> Vec<u16> { (0..n as u16).map(|i| i * slot).collect() }
fn utc() -> FixedOffset { FixedOffset::east_opt(0).unwrap() }

#[test]
fn hourly_crypto_gets_date_labels() {
    // Bug revue : RegularDays comparait au chandelier précédent → 0 label en 24/7
    let times = series(utc(), utc().with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(), Duration::hours(1), 200, None);
    let axis = time_axis(&times, &cols(200, 1), 200);
    assert!(axis.primary.len() >= 5, "{:?}", axis.primary.iter().map(|l| &l.text).collect::<Vec<_>>());
}

#[test]
fn labels_never_overlap_nor_overflow() {
    let times = series(utc(), utc().with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(), Duration::minutes(5), 300, None);
    for (slot, width) in [(1, 300), (2, 600), (1, 40)] {
        let axis = time_axis(&times, &cols(300, slot)[..usize::from(width / slot).min(300)], width);
        for row in [&axis.primary, &axis.secondary] {
            for pair in row.windows(2) {
                assert!(pair[0].column + pair[0].text.chars().count() as u16 < pair[1].column);
            }
            assert!(row.iter().all(|l| l.column + l.text.chars().count() as u16 <= width));
        }
    }
}

#[test]
fn stock_intraday_uses_exchange_time() {
    // 30 min, 09:30–16:00 à New York : les labels doivent parler en heure de NY
    let ny = FixedOffset::west_opt(4 * 3600).unwrap();
    let start = ny.with_ymd_and_hms(2026, 9, 14, 9, 30, 0).unwrap();
    let times = series(ny, start, Duration::minutes(30), 130, Some((9, 16)));
    let axis = time_axis(&times, &cols(130, 2), 260);
    // 260 colonnes, 13 chandelles/jour à 2 colonnes : pas de 3 h → 09:30, 12:00, 15:00
    assert!(axis.primary.iter().all(|l| ("09:00"..="16:00").contains(&l.text.as_str())));
    assert!(axis.secondary.iter().any(|l| l.text.starts_with("Tue 15/09")));
}

#[test]
fn daily_candles_label_months_and_years() {
    let times = series(utc(), utc().with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(), Duration::days(1), 500, None);
    let axis = time_axis(&times, &cols(500, 1), 500);
    assert!(axis.primary.iter().any(|l| l.text == "Mar"));
    assert!(axis.secondary.iter().any(|l| l.text == "Jan 2026"));
}

#[test]
fn empty_and_single_candle() {
    assert!(time_axis(&[], &[], 50).primary.is_empty());
    let one = [utc().with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()];
    let axis = time_axis(&one, &[10], 50);
    assert!(axis.primary.is_empty() && axis.secondary.len() == 1); // contexte à gauche
}
```

- [ ] **Step 2:** `cargo test --lib time_axis` → FAIL.
- [ ] **Step 3: Implement:**

```rust
//! Axe du temps : choisit le pas de graduation selon la place disponible
//!
//! CONCEPT : Plutôt que des seuils de largeur fixés à la main, on essaie des pas du
//! plus fin au plus grossier (5 min → 10 ans) et on garde le premier dont les labels
//! tiennent sans se chevaucher. Un label tombe sur la chandelle qui ouvre une nouvelle
//! "case" (heure, jour, mois...) : les trous du marché (nuits, week-ends) sont gérés
//! d'office, et le décalage horaire de la place est déjà dans `DateTime<FixedOffset>`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step { Minutes(i64), Hours(i64), Days(i64), Weeks(i64), Months(i64), Years(i64) }

const LADDER: [Step; 19] = [
    Step::Minutes(5), Step::Minutes(15), Step::Minutes(30),
    Step::Hours(1), Step::Hours(2), Step::Hours(3), Step::Hours(6), Step::Hours(12),
    Step::Days(1), Step::Days(2), Step::Weeks(1), Step::Weeks(2),
    Step::Months(1), Step::Months(3), Step::Months(6),
    Step::Years(1), Step::Years(2), Step::Years(5), Step::Years(10),
];

impl Step {
    /// Numéro de case temporelle à l'heure de la place
    fn bucket(self, t: DateTime<FixedOffset>) -> i64 {
        let local_seconds = t.timestamp() + i64::from(t.offset().local_minus_utc());
        let local_days = local_seconds.div_euclid(86_400);
        match self {
            Step::Minutes(m) => local_seconds.div_euclid(60 * m),
            Step::Hours(h) => local_seconds.div_euclid(3_600 * h),
            Step::Days(d) => local_days.div_euclid(d),
            // Le 01/01/1970 était un jeudi : +3 aligne les semaines sur le lundi
            Step::Weeks(w) => (local_days + 3).div_euclid(7 * w),
            Step::Months(m) => (i64::from(t.year()) * 12 + i64::from(t.month0())).div_euclid(m),
            Step::Years(y) => i64::from(t.year()).div_euclid(y),
        }
    }

    fn format(self) -> &'static str {
        match self {
            Step::Minutes(_) | Step::Hours(_) => "%H:%M",
            Step::Days(_) | Step::Weeks(_) => "%d/%m",
            Step::Months(_) => "%b",
            Step::Years(_) => "%Y",
        }
    }

    /// Case plus large affichée en 2e ligne pour situer les labels fins
    fn context(self) -> Option<(Step, &'static str)> {
        match self {
            Step::Minutes(_) | Step::Hours(_) => Some((Step::Days(1), "%a %d/%m")),
            Step::Days(_) | Step::Weeks(_) => Some((Step::Months(1), "%b %Y")),
            Step::Months(_) => Some((Step::Years(1), "%Y")),
            Step::Years(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label { pub column: u16, pub text: String }

#[derive(Debug, Default)]
pub struct TimeAxis { pub primary: Vec<Label>, pub secondary: Vec<Label> }

/// Indices des chandelles qui ouvrent une nouvelle case
fn boundaries(times: &[DateTime<FixedOffset>], step: Step) -> Vec<usize> {
    (1..times.len()).filter(|&i| step.bucket(times[i]) != step.bucket(times[i - 1])).collect()
}

fn label_len(time: DateTime<FixedOffset>, format: &str) -> u16 {
    u16::try_from(time.format(format).to_string().chars().count()).unwrap_or(u16::MAX)
}

/// Le pas le plus fin dont les labels sont espacés d'au moins leur largeur + 1
fn choose_step(times: &[DateTime<FixedOffset>], columns: &[u16]) -> Option<Step> {
    LADDER.into_iter().find(|&step| {
        let indices = boundaries(times, step);
        !indices.is_empty()
            && indices.windows(2).all(|w| {
                columns[w[1]] - columns[w[0]] > label_len(times[w[0]], step.format())
            })
    })
}

/// Place les labels de gauche à droite, en sautant ceux qui chevauchent ou dépassent
fn place(times: &[DateTime<FixedOffset>], columns: &[u16], indices: &[usize], format: &str, width: u16) -> Vec<Label> {
    let mut labels = Vec::new();
    let mut next_free = 0;
    for &i in indices {
        let text = times[i].format(format).to_string();
        let end = columns[i].saturating_add(label_len(times[i], format));
        if columns[i] >= next_free && end <= width {
            next_free = end + 1;
            labels.push(Label { column: columns[i], text });
        }
    }
    labels
}

pub fn time_axis(times: &[DateTime<FixedOffset>], columns: &[u16], width: u16) -> TimeAxis {
    if times.is_empty() {
        return TimeAxis::default();
    }
    // Pas trouvé (trop peu de chandelles) : pas de labels fins, seulement le contexte
    let step = choose_step(times, columns);
    let primary = step.map_or_else(Vec::new, |s| place(times, columns, &boundaries(times, s), s.format(), width));
    let (context_step, context_format) = step
        .map_or(Some((Step::Days(1), "%a %d/%m")), Step::context)
        .unwrap_or((Step::Years(1), "%Y"));
    // Le 1er chandelier porte toujours le contexte : on sait où commence le graphique
    let mut indices = vec![0];
    indices.extend(boundaries(times, context_step));
    let secondary = place(times, columns, &indices, context_format, width);
    TimeAxis { primary, secondary }
}
```

Years steps have no context: `unwrap_or((Years(1), "%Y"))` keeps a year on the left, which is harmless.

- [ ] **Step 4:** `cargo test --lib time_axis` → PASS. If `stock_intraday_uses_exchange_time` fails on the `Tue 15/09` label position, print both rows and check the labels by eye before loosening the assertion. The label must exist; its exact column does not matter.
- [ ] **Step 5:** `cargo fmt`, commit `feat(chart): time axis picks the finest non-overlapping step in exchange time`.

---

### Task 8: Geometry + `CandleChart` widget

Fixes: 2 columns clipped by the border, candles overwriting each other, `NARROW_Y_AXIS_WIDTH` unreachable, ticks shifted on overlap, fixed 80-column minimum.

**Files:** Create `src/ui/chart/geometry.rs`, `src/ui/chart/widget.rs`; declare both in `chart/mod.rs`.

**Interfaces — Consumes:** `price_ticks`, `PriceTicks`, `time_axis`, `OHLCData::local_time`. **Produces:**

```rust
// geometry.rs
pub const AXIS_ROWS: u16 = 3;
pub fn slot_width(plot_width: u16) -> u16;
pub fn visible_columns(plot_width: u16, total: usize) -> (usize, Vec<u16>);   // (index du 1er visible, colonnes)
// widget.rs
pub const MIN_WIDTH: u16 = 30;
pub const MIN_HEIGHT: u16 = AXIS_ROWS + 5;
pub struct CandleChart<'a> { pub data: &'a OHLCData }
impl Widget for CandleChart<'_>
```

- [ ] **Step 1: Failing geometry tests:**

```rust
#[test]
fn wide_plot_uses_gaps_and_shows_more_history() {
    assert_eq!(slot_width(119), 1);
    assert_eq!(slot_width(120), 2);
    let (first, cols) = visible_columns(200, 1_000);
    assert_eq!((first, cols.len()), (900, 100));
    assert_eq!(*cols.last().unwrap(), 198); // colonne vide avant l'axe
}

#[test]
fn narrow_plot_is_one_column_per_candle() {
    let (first, cols) = visible_columns(50, 1_000);
    assert_eq!((first, cols.len(), cols[0], cols[49]), (950, 50, 0, 49));
}

#[test]
fn few_candles_are_right_aligned_not_stretched() {
    let (first, cols) = visible_columns(200, 10);
    assert_eq!(first, 0);
    assert_eq!(cols, (0..10).map(|i| 180 + 2 * i).collect::<Vec<u16>>());
}

#[test]
fn zero_width_or_no_candles() {
    assert_eq!(visible_columns(0, 10), (10, vec![]));
    assert_eq!(visible_columns(80, 0), (0, vec![]));
}
```

- [ ] **Step 2:** FAIL → implement:

```rust
//! Géométrie du graphique : combien de chandelles, à quelles colonnes
//!
//! CONCEPT : Densité fixe. Plus le terminal est large, plus on remonte dans le
//! temps ; on n'étire jamais les chandelles. Les plus récentes collent à droite,
//! contre l'axe des prix.

/// Lignes sous le graphique : trait + labels fins + contexte
pub const AXIS_ROWS: u16 = 3;
/// En dessous de ce nombre de chandelles visibles, on les serre (1 colonne chacune)
const WIDE_MIN_CANDLES: u16 = 60;

/// Colonnes par chandelle : 2 (chandelle + espace) si on en voit au moins 60 ainsi, sinon 1
pub fn slot_width(plot_width: u16) -> u16 {
    if plot_width >= 2 * WIDE_MIN_CANDLES { 2 } else { 1 }
}

pub fn visible_columns(plot_width: u16, total: usize) -> (usize, Vec<u16>) {
    let slot = slot_width(plot_width);
    let fit = usize::from(plot_width / slot).min(total);
    let first = total - fit;
    // fit ≤ plot_width : la conversion ne peut pas échouer
    let fit = u16::try_from(fit).unwrap_or(0);
    let offset = plot_width - fit * slot;
    (first, (0..fit).map(|i| offset + i * slot).collect())
}
```

- [ ] **Step 3:** `cargo test --lib geometry` → PASS.
- [ ] **Step 4: Failing widget tests** (render into a bare `Buffer`, read cells back):

```rust
fn data(n: usize, price: impl Fn(usize) -> f64) -> OHLCData {
    let utc = FixedOffset::east_opt(0).unwrap();
    let mut d = OHLCData::new("BTC-USD".into(), Interval::H1, utc);
    let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
    for i in 0..n {
        let p = price(i);
        d.add_candle(OHLC::new(t0 + chrono::Duration::hours(i as i64), p, p + 1.0, p - 1.0, p + 0.5, 0));
    }
    d
}

fn draw(data: &OHLCData, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    CandleChart { data }.render(area, &mut buf);
    buf
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf.get(x, y).symbol().to_string()).collect()
}

#[test]
fn latest_candle_is_drawn_next_to_the_axis() {
    // Bug revue : les 2 dernières colonnes étaient coupées par la bordure
    let d = data(500, |i| 100.0 + (i % 20) as f64);
    let buf = draw(&d, 100, 20);
    let rows = 20 - AXIS_ROWS;
    // ┤ n'existe que sur l'axe (les mèches utilisent │) : il repère la colonne de l'axe
    let axis_x = (0..100).find(|&x| (0..rows).any(|y| buf.get(x, y).symbol() == "┤")).unwrap();
    assert!((0..rows).any(|y| !matches!(buf.get(axis_x - 1, y).symbol(), " " | "┈")), "dernière chandelle absente");
}

#[test]
fn every_row_fits_the_area_exactly() {
    let d = data(300, |i| 100.0 + i as f64);
    for (w, h) in [(30, 8), (79, 24), (120, 30), (250, 60)] {
        let buf = draw(&d, w, h);
        assert_eq!(buf.area, Rect::new(0, 0, w, h)); // rien écrit hors zone : Buffer panique sinon
        assert!(!row(&buf, h - 1).trim().is_empty(), "contexte temporel présent en {w}x{h}");
    }
}

#[test]
fn tiny_areas_show_a_message_without_panicking() {
    let d = data(50, |_| 100.0);
    for (w, h) in [(0, 0), (1, 1), (29, 20), (80, 7)] {
        draw(&d, w, h);
    }
    assert!(row(&draw(&d, 29, 20), 0).contains("petit"));
}

#[test]
fn flat_prices_and_single_candle_render() {
    draw(&data(40, |_| 42.0), 80, 20);
    draw(&data(1, |_| 42.0), 80, 20);
    draw(&OHLCData::new("X".into(), Interval::D1, FixedOffset::east_opt(0).unwrap()), 80, 20);
}

#[test]
fn last_price_is_marked_on_the_axis() {
    let d = data(100, |i| 100.0 + i as f64); // dernière clôture : 199.5
    let buf = draw(&d, 100, 30);
    assert!((0..27).any(|y| row(&buf, y).contains("199.5")), "prix courant affiché sur l'axe");
}
```

- [ ] **Step 5:** `cargo test --lib widget` → FAIL.
- [ ] **Step 6: Implement `widget.rs`.** Move the glyph constants and the body of `render_candle` (currently `candlestick_text.rs:37-50` and `171-239`) verbatim into `fn glyph(candle: &OHLC, y: u16, scale: &Scale) -> char`, replacing `self.price_to_height` with `scale.height`. Then:

```rust
/// Correspondance prix ↔ hauteur en "lignes" (fractionnaires) du graphique
struct Scale { min: f64, max: f64, rows: u16 }

impl Scale {
    /// Bornes sur les chandelles visibles, 2 % de marge ; un prix plat reçoit ±1 %
    fn new(candles: &[OHLC], rows: u16) -> Self {
        let low = candles.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        let high = candles.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
        let spread = if high > low { high - low } else { high.abs().max(1.0) * 0.02 };
        let margin = spread * 0.02;
        Self { min: low - margin, max: high + margin, rows }
    }

    fn height(&self, price: f64) -> f64 {
        (price - self.min) / (self.max - self.min) * f64::from(self.rows)
    }

    /// Ligne (0 = haut) où s'affiche un prix, cohérente avec `glyph`
    fn row_of(&self, price: f64) -> u16 {
        let from_bottom = self.height(price).floor().clamp(1.0, f64::from(self.rows));
        self.rows - from_bottom as u16
    }
}

pub struct CandleChart<'a> { pub data: &'a OHLCData }

impl Widget for CandleChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let candles = &self.data.candles;
        if area.is_empty() {
            return;
        }
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            buf.set_stringn(area.x, area.y, "Terminal trop petit", usize::from(area.width), Style::default().fg(Color::Yellow));
            return;
        }
        if candles.is_empty() {
            buf.set_string(area.x, area.y, "Aucune chandelle", Style::default().fg(Color::Gray));
            return;
        }
        let rows = area.height - AXIS_ROWS;
        // Passe 1 : largeur d'axe estimée sur les chandelles d'un axe provisoire de 10 colonnes
        let axis_width = axis_width(candles, area.width.saturating_sub(10), rows);
        let plot_width = area.width - axis_width;
        let (first, columns) = visible_columns(plot_width, candles.len());
        let visible = &candles[first..];
        let scale = Scale::new(visible, rows);
        let ticks = price_ticks(scale.min, scale.max, rows);

        draw_candles(buf, area, visible, &columns, &scale);
        draw_price_axis(buf, area, plot_width, &scale, &ticks, visible.last());
        let times: Vec<_> = visible.iter().map(|c| self.data.local_time(c)).collect();
        draw_time_axis(buf, area, rows, plot_width, &time_axis(&times, &columns, plot_width));
    }
}
```

Helper contracts (each ≤ 30 lines):

- `axis_width(candles, provisional_plot, rows) -> u16` = `2 + widest` formatted tick or last-close label over the candles visible at `provisional_plot`, using `price_ticks` on their `Scale`, clamped to `area.width - 10`. ponytail: two passes can differ by one character if the second visible set widens the range; the axis then truncates the label's last digit. Upgrade: iterate until stable.
- `draw_candles`: for each visible candle and each `y in 1..=rows`, set the cell `(area.x + col, area.y + rows - y)` to `glyph(...)` in bullish or bearish colour (constants kept from the old file).
- `draw_price_axis`: column `area.x + plot_width` gets `│` on every plot row and `┤` on tick rows, followed by ` label` in gray. The last close row gets `┤` plus `{label}` in `Style::reversed()` with the candle's colour, overriding any tick on that row. Empty plot cells on that row get `┈` in `Color::DarkGray` (a price line to the right of the history).
- `draw_time_axis`: row `rows` gets `─` across the plot and `┬` at primary label columns. Row `rows + 1` gets the primary labels, alternating `Color::Gray` / `Color::Yellow` (kept from commit `8878ec3`). Row `rows + 2` gets the secondary labels in `Color::Gray` bold. Every write goes through `buf.set_stringn(..., max = plot_width - col)` so nothing leaves the plot.
- [ ] **Step 7:** `cargo test --lib widget geometry` → PASS. Adjust `latest_candle_is_drawn_next_to_the_axis` only if the axis glyph search is wrong. It must still prove the last visible candle's column is non-empty.
- [ ] **Step 8:** `cargo fmt`, commit `feat(chart): responsive candlestick widget rendered into its exact area`.

---

### Task 9: Chart screen + removal of the old renderer

**Files:** Modify `src/ui/chart/mod.rs`, `src/ui/dashboard.rs` (route `ChartView` → `chart::render_chart_screen`), `src/ui/mod.rs`, `src/models/mod.rs`, `src/models/ohlc.rs`. Delete `src/ui/candlestick_text.rs`, `src/models/ticker.rs`.

- [ ] **Step 1: Failing screen test** (`TestBackend`, whole frame):

```rust
#[test]
fn chart_screen_shows_title_marker_and_status() {
    let mut app = App::new(vec!["BTC-USD".into()], None, Instant::now());
    // données 30m chargées, intervalle choisi 1h : l'écran doit signaler le rechargement
    app.watchlist[0].apply(sample_fetched(Interval::M30, 200));
    app.current_interval = Interval::H1;
    app.current_screen = Screen::ChartView;
    app.pending = 1;
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| render_chart_screen(f, &app)).unwrap();
    let text: String = terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect();
    assert!(text.contains("30m → 1h"));
    assert!(text.contains("Chargement"));
}
```

(`sample_fetched` builds a `FetchedTicker` with `n` hourly candles, like the widget test helper.)

- [ ] **Step 2:** FAIL → implement `render_chart_screen(frame, app)`:
  - Layout: header `Length(3)` + chart `Min(0)`.
  - Header block titled `{symbol} · {name}`. Its line is `status_line(app)` if any (Task 4), else price (`format_price`), change % with arrow and colour, and shortcuts `[h/l] Intervalle  [r] Rafraîchir  [Esc] Retour  [q] Quitter`.
  - Chart block `Borders::ALL`, titled `{interval} · {first local date} → {last local date} · {n} chandeliers`. When `data.interval != app.current_interval`, the interval part reads `{data} → {chosen} ⏳`.
  - Then `let inner = block.inner(area); frame.render_widget(block, area); frame.render_widget(CandleChart { data }, inner);` — the inner area is what fixes the clipping.
  - No data yet: centred `Chargement…` if `app.is_loading()`, else `Pas de données pour {symbol} — [r] pour réessayer`.
- [ ] **Step 3: Delete the old code.** `candlestick_text.rs`, `ticker.rs` (`TickerType` has no remaining user), `LabelStrategy`, `AxisFormats`, `x_axis_format`, `is_intraday` (if unused), and their re-exports. Check with `rg -n "candlestick_text|TickerType|LabelStrategy|AxisFormats|Timeframe" src` → no hits.
- [ ] **Step 4:** `cargo test` → green; `cargo build` → 0 warnings about dead code (`cargo build 2>&1 | grep -c "never used"` → 0).
- [ ] **Step 5: Visual check in tmux** at 3 sizes. Capture the output as evidence in the commit body summary:

```bash
for size in "60 20" "120 35" "220 50"; do
  set -- $size
  tmux new -d -s lw -x $1 -y $2 'cargo run'; sleep 10
  tmux send -t lw Enter; sleep 3; tmux capture-pane -pt lw > /tmp/lw-$1.txt
  tmux send -t lw l; sleep 3; tmux capture-pane -pt lw > /tmp/lw-$1-h1.txt
  tmux kill-session -t lw
done
```

Check each capture: the latest candle touches the price axis; the 1h capture has date labels; no label is cut; 60 columns shows 1 column per candle, 220 columns shows gaps.

- [ ] **Step 6:** `cargo fmt`, commit `feat(chart): new chart screen, remove legacy renderers`.

---

### Task 10: Lints, docs, final verification

**Files:** Any file flagged by clippy; `README.md`, `docs/architecture.md`, `AGENTS.md`; create `docs/chart-rendering.md`, delete `docs/candlestick-alignment.md`; `Cargo.toml` (remove chrono's `serde` feature if nothing needs it).

- [ ] **Step 1:** `cargo clippy --all-targets -- -D warnings` and fix every hit. Casts: prefer `u16::try_from(..)`/`From`. A remaining `as` cast on bounded values gets `#[expect(clippy::cast_possible_truncation, reason = "...")]` on the smallest item, with the bound written in the reason. `too_many_lines` → split the function.
- [ ] **Step 2: Docs.**
  - `docs/chart-rendering.md`, ≤ 120 lines: density rule, geometry, price ticks, time-axis ladder and buckets, the exchange-time fixed-offset ceiling, the widget-in-inner-area rule. It replaces `candlestick-alignment.md` (git keeps the history).
  - `docs/architecture.md`: update the module map, the worker/App flow, keys, persistence, refresh, and remove the "Limites connues" entries now fixed.
  - `README.md`: keyboard tables (+ `r`, Ctrl+C), watchlist file path, refresh, intervals table (new history windows, "the chart shows as many candles as fit"), timezone note, Known Issues reduced to what remains (Yahoo delays and regions, DST ceiling), dev commands (`cargo test -- --ignored` for the network test).
  - `AGENTS.md`: drop the debt paragraph (now: all three checks must pass), point to `docs/chart-rendering.md`, replace the index-addressing and `chart.rs` pitfalls with the current ones (worker returns exactly one result per command, since `pending` depends on it; the chart widget only draws inside the `Rect` it receives).
- [ ] **Step 3: Final evidence on the candidate commit:**

```bash
cargo fmt --check; echo "fmt rc=$?"
cargo clippy --all-targets -- -D warnings; echo "clippy rc=$?"
cargo test; echo "test rc=$?"
cargo test -- --ignored; echo "network rc=$?"
```

All `rc=0`; record SHA + outputs.

- [ ] **Step 4:** Commit `chore: clippy pedantic clean` and `docs: describe responsive chart and new app flow` separately.
- [ ] **Step 5:** Independent whole-branch review (`superpowers:requesting-code-review`) on `8878ec3..HEAD`, fix the findings, re-run Step 3 on the new SHA, then hand back to Cyril for the rebase onto `main`.
