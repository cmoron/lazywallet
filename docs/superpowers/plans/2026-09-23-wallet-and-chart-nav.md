# Wallet + Chart Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans (native, chosen for the previous plan and kept). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Ship the 7 improvements proposed after the review: scrolling watchlist, portfolio positions, chart navigation with a cursor, volume, per-row sparkline, market status and last update, and MA20/MA50.

**Architecture:** Same shape as before. `App` owns all state and returns `AppCommand`s; pure helpers carry their own tests; the chart widget draws only inside its `Rect`. Two pieces of view feedback flow from render back to state through `Cell` (interior mutability): the list scroll offset and the chart's visible range.

**Tech Stack:** Rust 2021, ratatui 0.26.3, crossterm 0.27, chrono 0.4.42. No new crate.

**Spec:** Cyril's request (2026-09-23): "Implémente les 7", on top of branch `review-fixes`, merged later with ff-only.

## Decisions (Cyril, 2026-09-23)

- Positions live in `watchlist.txt`, as optional columns on the symbol line: `AAPL 10 150.25` (quantity, unit cost). A bare symbol is watch-only. Inline `# comment` is allowed.
- Portfolio totals are shown **per currency**, with no FX conversion.
- Moving averages: MA20 + MA50 (simple, on closes), toggled with `m`.

## Global Constraints

- French pedagogical comments; English README/commits; Conventional Commits.
- No `unwrap`/`expect` outside tests; user-facing errors go through `App::set_error`.
- Each lot: failing test first, `cargo test` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` green, one commit.
- A new shortcut goes in its screen's branch of `handle_key`; `InputMode` swallows all keys.
- The chart widget draws only inside its `Rect` and reuses `visible_columns` columns.

## Review Focus

- A hand-edited `watchlist.txt` with a bad position (`AAPL abc 1`, a negative quantity, 3 columns + comment) gives a clear error naming the line, never a silent drop.
- Chart navigation at the edges (cursor on the oldest or newest candle, data shorter than the screen, an interval change or refresh while scrolled) never panics or indexes out of range.
- Positions in several currencies, or on a ticker without a quote yet, stay out of the totals and never produce a NaN.
- Zero volume (forex) hides the volume pane instead of drawing an empty scale; too few rows hides it too.
- An MA with fewer candles than its period is not drawn (no partial average passed off as MA50).

## Lots

### Lot 1 — Scrolling watchlist as a table (`ui/dashboard.rs`, `app.rs`)

- Replace `List` with a `Table` + `TableState`. The offset is kept in `App::list_offset: Cell<usize>`: render reads it, ratatui adjusts it to keep the selection visible, render writes it back.
- Columns chosen by available width (the widest set that fits): symbol, name, price, day change, then (later lots) trend, quantity, value, P&L.
- Tests: `selection_stays_visible_when_scrolling` (40 tickers, 10 rows, select 30 → "T30" on screen); `scrolling_up_keeps_offset` (from 30 up to 25 → the row of 30 is still on screen, i.e. the offset did not reset to put 25 at the bottom).

### Lot 2 — Portfolio (`models/watchlist_item.rs`, `watchlist_file.rs`, `app.rs`, `ui/events.rs`, `ui/dashboard.rs`)

- `pub struct Position { pub quantity: f64, pub unit_cost: f64 }` and `WatchlistItem::position: Option<Position>`.
- On the item: `market_value()`, `unrealized_pnl()`, `unrealized_pnl_percent()`, `day_pnl()` (quantity × (price − previous close)). All return `Option`, `None` without a quote.
- `watchlist_file::Entry { symbol, position }`; `load -> Vec<Entry>`; `save(&[Entry])`. Line = `SYMBOL [QTY COST] [# comment]`; invalid → error with `ligne N`. `save` keeps comment lines, inline comments, and the original order.
- `portfolio::totals(&[WatchlistItem]) -> Vec<CurrencyTotal { currency, value, cost, day_pnl }>`, sorted by currency. Items without a quote or position are skipped.
- `p` on the dashboard opens `InputMode` with `InputKind::Position(symbol)`, prefilled `"10 150.25"`. Enter parses `QTY COST`; empty removes the position; invalid → status error. Accepted chars: digits, `.`, `,` (read as `.`), space.
- Dashboard: quantity / value / P&L columns shown when at least one position exists; the header shows the per-currency totals.
- Tests: file round trip with positions and inline comments; bad lines (`abc`, negative, 2 columns); `totals` over USD+EUR and a quote-less item; P&L maths; `p` → prefilled input → Enter sets or removes.

### Lot 3 — Sparkline column (`ui/sparkline.rs`)

- `pub fn sparkline(values: &[f64], width: usize) -> String` using `▁▂▃▄▅▆▇█`: resampled to `width` points, flat series → middle level, empty → "".
- Source: closes of the last session (exchange-local date) when intraday, else the last `width` closes.
- Tests: rising → ends with `█` and starts with `▁`; flat; width larger than the data; empty.

### Lot 4 — Market status + last update (`api/yahoo.rs`, `models`, `ui`)

- Parse `meta.currentTradingPeriod.regular.{start,end}` into `FetchedTicker::session: Option<(DateTime<Utc>, DateTime<Utc>)>`. `WatchlistItem::is_market_open(now)`.
- `WatchlistItem::updated_at: Option<DateTime<Utc>>`, set on `apply` from `FetchedTicker::fetched_at` (the worker stamps `Utc::now()`).
- Dashboard: a `●` (green, open) / `○` (gray, closed) column; header "Mis à jour HH:MM:SS" (machine local time, most recent `updated_at`). Chart header: "Marché ouvert/fermé".
- Tests: fixture parse of the period; `is_market_open` before/inside/after; missing period → `None` (no dot).

### Lot 5 — Chart navigation (`app.rs`, `ui/events.rs`, `ui/chart/*`)

- `App::chart_offset: usize` (newest candles hidden to the right), `App::cursor: Option<usize>` (absolute candle index), `App::chart_view: Cell<Option<(usize, usize)>>` (visible `first..end`, written by render).
- Keys (chart screen): `←`/`→` move the cursor (created on the newest visible candle), scrolling when it leaves the view; `PgUp`/`PgDn` scroll half a page; `End` back to the latest + cursor off; `Esc` removes the cursor first, then goes back. Changing ticker or interval resets both.
- `visible_columns` gets `end` (exclusive), so it windows `candles[..end]`. `CandleChart` takes `end` and `cursor`; the cursor column gets a `┆` in empty cells, and the header shows date, O/H/L/C and volume.
- Tests: pure helpers `scroll`/`move_cursor` on (len, view, offset, cursor) at the edges; widget with `end < len` shows the right candle next to the axis; cursor rendering; `handle_key` sequences.

### Lot 6 — Volume pane (`ui/chart/widget.rs`, `geometry.rs`)

- `VOLUME_ROWS = 3` taken from the plot when `rows ≥ 14` and some visible volume > 0. Bars use `▁▂▃▄▅▆▇█` (8 levels per row), in the candle colour, dimmed. The axis column shows the max volume abbreviated (`1.2M`).
- Tests: zero volume → no pane; small height → no pane; tallest bar is full blocks; `abbreviate(1_234_567) == "1.2M"`.

### Lot 7 — MA20 / MA50 (`ui/chart/indicators.rs`, widget)

- `pub fn sma(closes: &[f64], period: usize) -> Vec<Option<f64>>`, `None` until `period` values exist. Computed on all candles up to `end`, so the left edge is valid.
- Drawn as `•` in the cells not taken by a candle body (a gap column gets the average of its neighbours). MA20 yellow, MA50 magenta; the block title gets a legend; `m` toggles (`App::show_ma`, default on).
- Tests: `sma` values and `None` prefix; widget draws `•` when on and none when off; the price scale includes the MAs.

### Lot 8 — Docs, final gates, independent review

- README (keys, file format, features), architecture.md, chart-rendering.md, AGENTS.md pitfalls (Cell feedback).
- Final gates on the candidate SHA, then a whole-branch review on Opus and one fix pass.
