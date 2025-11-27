# 💼 LazyWallet

A fast, lightweight Terminal User Interface (TUI) for tracking cryptocurrency and stock prices in real-time.

## ✨ Features

- **Real-time Market Data**: Fetches live prices from Yahoo Finance API
- **Interactive Watchlist**: Track multiple tickers with daily change percentages
- **Beautiful Candlestick Charts**: Unicode-based chart visualization directly in your terminal
- **Multiple Timeframes**: Switch between 5m, 15m, 30m, 1h, 4h, 1d, and 1w intervals
- **Vim-inspired Navigation**: Efficient keyboard shortcuts for power users
- **Auto-refresh**: Data automatically updates when switching intervals
- **Safe Operations**: Two-step confirmation for quit and delete actions
- **Structured Logging**: Comprehensive logging system for debugging

## 🚀 Installation

### Prerequisites

- Rust 1.70 or higher
- Cargo (comes with Rust)

### Building from Source

```bash
git clone https://github.com/yourusername/lazywallet.git
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

The application starts with an empty watchlist. Add tickers to get started!

### Keyboard Shortcuts

#### Dashboard (Watchlist View)

| Key | Action |
|-----|--------|
| `a` | Add a new ticker to the watchlist |
| `d` | Delete selected ticker (requires confirmation) |
| `↑` / `k` | Navigate up in the list |
| `↓` / `j` | Navigate down in the list |
| `Enter` | Open candlestick chart for selected ticker |
| `q` | Quit application (requires confirmation) |

#### Chart View

| Key | Action |
|-----|--------|
| `h` | Switch to previous interval (cycle: 5m → 15m → 30m → 1h → 4h → 1d → 1w) |
| `l` | Switch to next interval |
| `ESC` / `Space` | Return to dashboard |

#### Input Mode (Adding Ticker)

| Key | Action |
|-----|--------|
| `Enter` | Confirm and add ticker |
| `ESC` | Cancel input |
| `Backspace` | Delete last character |

### Supported Tickers

LazyWallet supports any ticker available on Yahoo Finance:

- **Stocks**: `AAPL`, `GOOGL`, `TSLA`, `MSFT`, etc.
- **Cryptocurrencies**: `BTC-USD`, `ETH-USD`, `SOL-USD`, etc.
- **ETFs**: `SPY`, `QQQ`, `VOO`, etc.
- **Forex**: `EURUSD=X`, `GBPUSD=X`, etc.

## 🎨 Interface

### Dashboard View
```
┌────────────────────── LazyWallet ──────────────────────┐
│          🚀 Terminal User Interface Mode               │
└────────────────────────────────────────────────────────┘
┌────────────────────── 📊 Watchlist ────────────────────┐
│ BTC-USD  Bitcoin              $45,234.50  ▲ +2.34%     │
│ AAPL     Apple Inc.           $178.23     ▼ -0.87%     │
│ TSLA     Tesla                $242.56     ▲ +1.45%     │
└────────────────────────────────────────────────────────┘
┌────────────────────────────────────────────────────────┐
│ [q] Quit  [↑↓ / j k] Navigate  [Enter] Chart           │
│ [a] Add  [d] Delete                                    │
└────────────────────────────────────────────────────────┘
```

### Chart View
Displays Unicode-based candlestick charts with:
- Green candles for bullish (close > open)
- Red candles for bearish (close < open)
- Dynamic price and date axes
- Current interval indicator

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
├── api/
│   ├── mod.rs
│   └── yahoo.rs          # Yahoo Finance API integration
├── models/
│   ├── mod.rs
│   ├── ohlc.rs           # OHLC data structures and intervals
│   ├── ticker.rs         # Ticker model
│   └── watchlist_item.rs # Watchlist item with data
├── ui/
│   ├── mod.rs
│   ├── dashboard.rs      # Main dashboard rendering
│   ├── chart.rs          # Chart view rendering
│   ├── candlestick_text.rs # Unicode candlestick drawing
│   └── events.rs         # Keyboard event handling
├── app.rs                # Application state management
├── lib.rs                # Library root
└── main.rs               # Entry point and event loop
```

## 🔧 Configuration

### Logging

Logs are written to `./logs/lazywallet.log.YYYY-MM-DD` with the following levels:
- `DEBUG`: API calls, data parsing details
- `INFO`: User actions, state changes
- `ERROR`: API failures, parsing errors

### Intervals and Timeframes

The application automatically selects appropriate timeframes for each interval:
- **5m / 15m**: 7 days of data
- **30m / 1h**: 30 days of data
- **4h**: 90 days of data
- **1d**: 180 days of data
- **1w**: 365 days of data

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/lazywallet.git
cd lazywallet

# Run in development mode with logs
cargo run

# Run tests
cargo test

# Check for warnings
cargo clippy

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

- 1-minute interval (`1m`) is disabled due to Yahoo Finance API limitations (max 7 days of data)
- Market data may have a slight delay depending on Yahoo Finance
- Some tickers may not be available depending on your region

## 🚧 Roadmap

- [ ] Persist watchlist between sessions
- [ ] Customizable color themes
- [ ] Price alerts and notifications
- [ ] Portfolio tracking with cost basis
- [ ] Export data to CSV
- [ ] Technical indicators (SMA, EMA, RSI, etc.)
- [ ] Multiple watchlist support
- [ ] Search/filter functionality

---

Built with ❤️ and Rust 🦀
