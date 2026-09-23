# 💼 LazyWallet

A fast, lightweight Terminal User Interface (TUI) for tracking cryptocurrency and stock prices in real-time.

## ✨ Features

- **Market Data**: Prices and OHLC history from the Yahoo Finance chart API, refreshed every 60 s (or on `r`)
- **Persistent Watchlist**: Stocks, ETFs, crypto, forex and indices, with the change since the previous session close, a trend sparkline and a market open/closed dot
- **Portfolio**: Positions (quantity, unit cost) with value, unrealized P&L and day P&L, totalled per currency
- **Responsive Candlestick Charts**: Unicode charts that use the whole terminal — a wider window shows more history
- **Chart Tools**: Cursor with OHLC and volume, scrolling back in history, volume pane, MA20/MA50 overlays
- **Multiple Timeframes**: Switch between 5m, 15m, 30m, 1h, 4h, 1d, and 1w intervals
- **Vim-inspired Navigation**: Efficient keyboard shortcuts for power users
- **Safe Operations**: Two-step confirmation for quit and delete actions
- **Structured Logging**: Daily-rotated log files for debugging

## 🚀 Installation

### Prerequisites

- A recent stable Rust toolchain (last verified with Rust 1.95)

### Building from Source

```bash
git clone https://github.com/cmoron/lazywallet.git
cd lazywallet
cargo build --release
```

The binary will be available at `./target/release/lazywallet`

## 📖 Usage

### Starting the Application

```bash
cargo run
# or if you built the release binary:
./target/release/lazywallet
```

The UI appears immediately and prices fill in as they arrive. The watchlist is stored in `~/.config/lazywallet/watchlist.txt`; on first run it contains `AAPL`, `TSLA` and `BTC-USD`. It is plain text and can be edited by hand:

```
# One symbol per line, optionally followed by a position: quantity and unit cost
AAPL 10 150.25     # comments are kept when LazyWallet rewrites the file
BTC-USD 0.05 62000
TSLA               # watch only
^GSPC
```

Portfolio totals are shown per currency in the header (no FX conversion).

### Keyboard Shortcuts

#### Dashboard (Watchlist View)

| Key | Action |
|-----|--------|
| `a` | Add a new ticker to the watchlist |
| `d` | Delete selected ticker (requires confirmation) |
| `↑` / `k` | Navigate up in the list |
| `↓` / `j` | Navigate down in the list |
| `Enter` | Open candlestick chart for selected ticker |
| `p` | Edit the position of the selected ticker (`quantity unit_cost`, empty = none) |
| `r` | Refresh all prices |
| `q` | Quit application (requires confirmation) |
| `Ctrl+C` | Quit immediately (any screen) |

#### Chart View

| Key | Action |
|-----|--------|
| `h` | Switch to previous interval (cycle: 5m → 15m → 30m → 1h → 4h → 1d → 1w) |
| `l` | Switch to next interval |
| `←` / `→` | Move the cursor (shows date, OHLC, volume; scrolls at the edges) |
| `PgUp` / `PgDn` | Scroll half a page back / forward in time |
| `End` | Back to the latest candles, cursor off |
| `m` | Show / hide MA20 and MA50 |
| `ESC` | Hide the cursor, then return to dashboard |
| `Space` | Return to dashboard |
| `r` | Refresh |
| `q` | Quit application (requires confirmation) |

#### Input Mode (Adding a Ticker or Editing a Position)

| Key | Action |
|-----|--------|
| `Enter` | Confirm and add ticker |
| `ESC` | Cancel input |
| `Backspace` | Delete last character |

Tickers accept letters, digits, `-`, `.`, `=` and `^` (typed letters are upper-cased); positions accept digits, `.`, `,` and spaces.

### Supported Tickers

LazyWallet supports any ticker available on Yahoo Finance:

- **Stocks**: `AAPL`, `GOOGL`, `TSLA`, `MSFT`, etc.
- **Cryptocurrencies**: `BTC-USD`, `ETH-USD`, `SOL-USD`, etc.
- **ETFs**: `SPY`, `QQQ`, `VOO`, etc.
- **Forex**: `EURUSD=X`, `GBPUSD=X`, etc.
- **Indices**: `^GSPC`, `^FCHI`, etc.

Unknown symbols are rejected with a message in the status bar.

## 🎨 Interface

### Dashboard View
![Dashboard](docs/images/dashboard.png)

The main dashboard displays your watchlist with real-time prices, daily changes, and quick navigation shortcuts.

### Chart View
![Chart](docs/images/chart.png)

Beautiful Unicode candlestick charts with:
- Green candles for bullish periods (close ≥ open), red for bearish ones
- Round price ticks on a right-hand axis, with the last price highlighted
- A time axis that picks the finest labels that fit (5 min to 10 years), in the exchange's local time
- One column per candle on narrow terminals, candle + gap on wide ones; the most recent candles always touch the price axis
- Works down to 30×8 characters

## 🛠️ Tech Stack

- **Language**: Rust 🦀
- **TUI Framework**: [ratatui](https://github.com/ratatui-org/ratatui)
- **Terminal Backend**: [crossterm](https://github.com/crossterm-rs/crossterm)
- **HTTP Client**: [reqwest](https://github.com/seanmonstar/reqwest)
- **Async Runtime**: [tokio](https://tokio.rs/)
- **Data API**: Yahoo Finance API
- **Logging**: [tracing](https://github.com/tokio-rs/tracing) + [tracing-appender](https://docs.rs/tracing-appender/)
- **Serialization**: [serde](https://serde.rs/)
- **Date/Time**: [chrono](https://github.com/chronotope/chrono)

## 📁 Project Structure

```
src/
├── api/yahoo.rs          # Yahoo Finance client (shared, 10 s timeout)
├── models/
│   ├── ohlc.rs           # Interval, OHLC candles, exchange sessions
│   └── watchlist_item.rs # Watchlist item, quote, position, P&L
├── ui/
│   ├── dashboard.rs      # Watchlist table, portfolio totals, status bar, input line
│   ├── events.rs         # Key handling, routed by screen
│   ├── sparkline.rs      # Per-row trend sparkline
│   └── chart/            # Candlestick widget, axes, volume, moving averages
├── app.rs                # Application state
├── worker.rs             # Network thread (commands → results)
├── watchlist_file.rs     # Watchlist + positions persistence
├── portfolio.rs          # Per-currency totals
├── lib.rs
└── main.rs               # Terminal setup and event loop
```

See [docs/architecture.md](docs/architecture.md) and [docs/chart-rendering.md](docs/chart-rendering.md).

## 🔧 Configuration

### Logging

Logs are written to `./logs/lazywallet.log.YYYY-MM-DD`, relative to the directory you launch from. Default filter is `lazywallet=debug,info`; override it with `RUST_LOG` (e.g. `RUST_LOG=lazywallet=trace`). Levels used:
- `DEBUG`: API calls, data parsing details
- `INFO`: User actions, state changes
- `ERROR`: API failures, parsing errors

### Intervals and Timeframes

Each interval fetches a fixed history window, sized to fill a wide terminal; the chart shows as many of the most recent candles as fit:

| Interval | History fetched |
|----------|-----------------|
| 5m  | 14 days |
| 15m | 30 days |
| 30m (default) | 58 days |
| 1h  | 6 months |
| 4h  | 1 year |
| 1d  | 2 years |
| 1w  | 10 years |

Chart times are in the exchange's timezone (New York for US stocks, UTC for crypto).

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/cmoron/lazywallet.git
cd lazywallet

# Run in development mode with logs
cargo run

# Run tests (offline)
cargo test

# Also run the test that calls Yahoo Finance
cargo test -- --ignored

# Lint (pedantic, configured in Cargo.toml)
cargo clippy --all-targets -- -D warnings

# Format code
cargo fmt
```

## 📝 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🙏 Acknowledgments

- Yahoo Finance for providing free market data API
- The Rust community for excellent crates and documentation
- [ratatui](https://github.com/ratatui-org/ratatui) for the amazing TUI framework

## 🐛 Known Issues

- The exchange timezone is a fixed offset taken at load time: across a daylight-saving change, older intraday labels are off by one hour.
- Market data may be delayed, and some tickers may not be available depending on your region.

## 🚧 Roadmap

- [ ] Customizable color themes
- [ ] Price alerts and notifications
- [ ] Export data to CSV
- [ ] More technical indicators (EMA, RSI, Bollinger bands)
- [ ] Multiple watchlist support
- [ ] Search/filter functionality

---

Built with ❤️ and Rust 🦀
