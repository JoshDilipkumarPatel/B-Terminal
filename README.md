# B-Terminal

> **Bloomberg Terminal Recreation with Ki Assistant Algorithmic Trading**  
> *Production-ready terminal trading platform with real-time data, strategy DSL, backtesting, and live execution*

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-active%20development-green.svg)]()

## 🚀 Overview

B-Terminal is a complete Bloomberg Terminal recreation built in Rust with a terminal user interface (TUI) using `ratatui`. It includes **Ki Assistant** - an integrated algorithmic trading system with:

- **Strategy DSL** - Domain-specific language for writing trading strategies
- **Real-time Signal Generation** - 12 built-in technical indicators, compiled to zero-overhead Rust closures
- **Backtesting Engine** - Event-driven simulation with realistic market microstructure (slippage, spread, latency, commission)
- **Risk Management** - Multi-layered: global, per-strategy, per-symbol with hard guards
- **Kill Switch** - Emergency flattening (<100ms target)
- **Multi-Broker Support** - Alpaca (paper/live), Binance, Coinbase, Simulator
- **Audit Trail** - Tamper-evident hash chains for compliance

## 🏗 Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        B-TERMINAL                                │
├─────────────────────────────────────────────────────────────────┤
│  bt-cli          │  bt-tui         │  bt-core                   │
│  ────────────    │  ──────────     │  ────────                   │
│  • Entry point   │  • App loop     │  • Types (Symbol, Order...) │
│  • CLI commands  │  • Widgets      │  • Events                  │
│  • Config mgmt   │  • Keybindings  │  • Config                  │
│                  │  • Layout       │  • Risk Limits             │
│                  │  • Themes       │  • Kill Switch             │
├─────────────────────────────────────────────────────────────────┤
│  bt-data         │  bt-strategy    │  bt-execution              │
│  ────────        │  ────────────   │  ────────────              │
│  • Providers     │  • DSL Parser   │  • Broker Adapter          │
│  • Polygon       │  • AST          │  • Alpaca (Paper/Live)     │
│  • Binance       │  • Compiler     │  • Simulator               │
│  • Coinbase      │  • Indicators   │  • OMS                     │
│  • Cache/Norm    │  • Signal Engine│  • Order Builder           │
│  • Manager       │  • Backtest     │  • Reconciliation          │
│                  │  • Strategy Risk│                              │
└─────────────────────────────────────────────────────────────────┘
```

## 📦 Crates

| Crate | Description |
|-------|-------------|
| `bt-core` | Core domain types, events, configuration, risk management, kill switch |
| `bt-data` | Market data providers (Polygon, Binance, Coinbase), caching, normalization |
| `bt-strategy` | Strategy DSL, parser, compiler, indicators, signal engine, backtesting |
| `bt-execution` | Broker adapters, OMS, order routing, simulator |
| `bt-tui` | Terminal UI with widgets, layouts, themes, keybindings |
| `bt-cli` | Main entry point, CLI commands, configuration management |

## ⚡ Quick Start

### Prerequisites

- Rust 1.75+
- API keys for data providers (Polygon, Binance, etc.)
- Alpaca account for paper/live trading (optional)

### Installation

```bash
# Clone and build
git clone https://github.com/your-org/b-terminal
cd b-terminal
cargo build --release

# Run with default config
./target/release/b-terminal
```

### Configuration

1. Copy `config.toml` and customize:
   ```toml
   # Set your API keys
   [data.providers]
   api_key = "YOUR_POLYGON_KEY"
   
   [execution.brokers]
   api_key = "YOUR_ALPACA_KEY"
   api_secret = "YOUR_ALPACA_SECRET"
   ```

2. Or use environment variables:
   ```bash
   export POLYGON_API_KEY="your_key"
   export ALPACA_PAPER_API_KEY="your_key"
   export ALPACA_PAPER_API_SECRET="your_secret"
   ```

3. Run:
   ```bash
   # Paper trading (default)
   b-terminal --paper
   
   # Live trading (REAL MONEY - requires explicit flag)
   b-terminal --live
   
   # Headless mode (no TUI)
   b-terminal --headless
   
   # Run backtest
   b-terminal backtest --strategy mean_reversion_rsi_bb --symbol AAPL
   
   # Validate strategy
   b-terminal validate --file strategies/my_strategy.bt
   ```

## 🎮 TUI Controls

### Global Shortcuts

| Key | Action |
|-----|--------|
| `F1`-`F7` | Focus panes (Market, Detail, Chart, Book, News, Portfolio, Ki) |
| `Tab` / `Shift+Tab` | Cycle focus |
| `:` | Enter command mode |
| `Esc` | Exit command mode / close popups |
| `Ctrl+K` | **Kill Switch** (emergency) |
| `Ctrl+F` | Flatten all positions |
| `Ctrl+C` | Quit |

### Bloomberg-Style Commands

```
AAPL US <EQUITY> GO     # Select symbol (Bloomberg syntax)
:symbol AAPL            # Same as above
:chart 1h               # Set chart timeframe
:buy AAPL 100 150.00    # Buy 100 shares limit $150
:sell AAPL 100          # Sell 100 shares market
:ki builder             # Open strategy builder
:backtest "My Strat"    # Run backtest
:deploy "My Strat"      # Deploy live
:kill                   # Kill switch
:theme dark             # Change theme
:layout trading         # Change layout
```

### Pane-Specific

| Pane | Keys |
|------|------|
| Chart | `←/→` scroll, `↑/↓` zoom, `V` volume, `I` indicators |
| OrderBook | `+/-` depth |
| News | `Enter` open, `/` filter, `S` filter symbol |
| Portfolio | `Enter` select position |
| Ki Assistant | `←/→` tabs, `V` validate, `B` backtest, `D` deploy |

## 📝 Strategy DSL

Write strategies in a clean, readable syntax:

```dsl
strategy "My Strategy" {
    description: "RSI mean reversion with BB confirmation"
    author: "Trader"
    version: "1.0.0"
    symbols: ["AAPL", "MSFT", "GOOGL"]
    timeframe: "5m"

    indicators {
        rsi_14 = RSI(close, 14)
        bb_upper = BB_UPPER(close, 20, 2.0)
        bb_lower = BB_LOWER(close, 20, 2.0)
        bb_middle = BB_MIDDLE(close, 20)
        volume_sma = SMA(volume, 20)
        atr_14 = ATR(high, low, close, 14)
    }

    entry {
        long: rsi_14 < 30 AND close < bb_lower AND volume > volume_sma * 1.5
        short: rsi_14 > 70 AND close > bb_upper AND volume > volume_sma * 1.5
    }

    exit {
        long: close > bb_middle OR rsi_14 > 60
        short: close < bb_middle OR rsi_14 < 40
        stop_loss: atr_14 * 2.0
        take_profit: atr_14 * 3.0
    }

    risk {
        max_position_size: 0.10
        max_daily_loss: 0.02
        max_drawdown: 0.05
        position_sizing: "volatility_target"
        volatility_target: 0.15
    }
}
```

### Built-in Indicators (12)

| Indicator | Function | Description |
|-----------|----------|-------------|
| RSI | `RSI(source, period)` | Relative Strength Index |
| SMA | `SMA(source, period)` | Simple Moving Average |
| EMA | `EMA(source, period)` | Exponential Moving Average |
| BB | `BB_UPPER/MIDDLE/LOWER(source, period, mult)` | Bollinger Bands |
| MACD | `MACD_LINE/SIGNAL/HIST(source, fast, slow, signal)` | MACD |
| ATR | `ATR(high, low, close, period)` | Average True Range |
| VWAP | `VWAP(high, low, close, volume)` | Volume Weighted Average Price |
| StdDev | `STDDEV(source, period)` | Standard Deviation |
| Highest | `HIGHEST(source, period)` | Highest value |
| Lowest | `LOWEST(source, period)` | Lowest value |
| Crossover | `CROSSOVER(a, b)` | Cross detection |
| Funding | `FUNDING_RATE()` | Perpetual funding rate (crypto) |

### Operators & Functions

- **Comparison**: `>`, `<`, `>=`, `<=`, `==`, `!=`
- **Logic**: `AND`, `OR`, `NOT`
- **Math**: `+`, `-`, `*`, `/`, `%`
- **References**: `indicator[1]` for previous bar
- **Variables**: `open`, `high`, `low`, `close`, `volume`, `vwap`

## 🛡 Risk Management

### Layers of Protection

1. **Pre-Trade Checks** (OMS)
   - Buying power validation
   - Position size limits
   - Order rate limiting
   - Symbol restrictions

2. **Strategy Risk Manager**
   - Position sizing: Fixed Fractional, Kelly, Volatility Target, Fixed Notional
   - Per-strategy daily loss limits
   - Per-strategy drawdown limits
   - Max positions per strategy

3. **Global Risk Manager**
   - Portfolio-level daily loss limit (default 3%)
   - Portfolio-level max drawdown (default 10%)
   - Leverage monitoring (default 2x)
   - Sector exposure limits (default 30%)
   - Correlation limits

4. **Kill Switch**
   - Manual activation (`Ctrl+K`)
   - Auto-trigger on risk limit breach
   - Flattens all positions in <100ms
   - Cancels all pending orders
   - Blocks new orders until reset

## 📊 Backtesting

Realistic simulation with:
- Slippage (configurable bps)
- Bid-ask spread
- Latency simulation
- Commission model
- Partial fills
- Order rejection probability

```bash
# Run backtest
b-terminal backtest \
  --strategy mean_reversion_rsi_bb \
  --symbol AAPL \
  --timeframe 1h \
  --start 2023-01-01 \
  --end 2023-12-31 \
  --output results.json
```

### Metrics Reported

- Total Return, Annualized Return
- Sharpe Ratio, Sortino Ratio, Calmar Ratio
- Max Drawdown
- Win Rate, Profit Factor, Expectancy
- Trade statistics (avg, best, worst)
- Equity curve

## 🔌 Data Providers

| Provider | Asset Classes | WebSocket | REST | Status |
|----------|---------------|-----------|------|--------|
| Polygon.io | Stocks, Options, Forex | ✅ | ✅ | Production |
| Binance | Crypto Spot/Futures | ✅ | ✅ | Production |
| Coinbase | Crypto Spot | ✅ | ✅ | Production |
| Alpha Vantage | Stocks, Forex, Crypto | ❌ | ✅ | Beta |
| IEX Cloud | Stocks | ❌ | ✅ | Beta |
| Mock | All (testing) | ✅ | ✅ | Testing |

## 🏢 Broker Adapters

| Broker | Mode | Asset Classes | Status |
|--------|------|---------------|--------|
| Alpaca | Paper/Live | Stocks, Options, Crypto | Production |
| Interactive Brokers | Live | Stocks, Options, Futures, Forex | Planned |
| Binance | Live | Spot, Futures | Beta |
| Coinbase Pro | Live | Spot | Beta |
| Simulator | Paper | All | Production |

## 📁 Project Structure

```
b-terminal/
├── Cargo.toml                 # Workspace root
├── config.toml                # Default configuration
├── strategies/                # Strategy files (*.bt)
│   ├── mean_reversion_rsi_bb.bt
│   ├── trend_macd_ema.bt
│   ├── breakout_donchian_vol.bt
│   ├── scalping_vwap_of.bt
│   └── crypto_funding_mr.bt
├── bt-core/                   # Core domain
│   ├── src/
│   │   ├── types.rs           # Symbol, Order, Position, Account...
│   │   ├── events.rs          # MarketEvent, SignalEvent, ExecutionEvent...
│   │   ├── config.rs          # AppConfig, all sub-configs
│   │   ├── risk_limits.rs     # RiskManager
│   │   └── kill_switch.rs     # GlobalKillSwitch, AutoKillSwitchMonitor
│   └── Cargo.toml
├── bt-data/                   # Market data
│   ├── src/
│   │   ├── provider.rs        # Traits: MarketDataProvider, HistoricalDataProvider
│   │   ├── polygon.rs         # Polygon.io WebSocket + REST
│   │   ├── crypto.rs          # Binance + Coinbase providers
│   │   ├── mock.rs            # Mock provider for testing
│   │   ├── cache.rs           # BarCache (SQLite), QuoteCache
│   │   ├── normalization.rs   # Symbol parsing/normalization
│   │   └── manager.rs         # DataFeedManager with fallback
│   └── Cargo.toml
├── bt-strategy/               # Algorithmic trading
│   ├── src/
│   │   ├── dsl/
│   │   │   ├── strategy.pest  # Pest grammar
│   │   │   ├── ast.rs         # AST types
│   │   │   └── compiler.rs    # Semantic validation + compilation
│   │   ├── indicators.rs      # 12 indicator implementations
│   │   ├── engine.rs          # SignalEngine
│   │   ├── backtest.rs        # BacktestEngine, Simulator
│   │   └── risk.rs            # StrategyRiskManager
│   └── Cargo.toml
├── bt-execution/              # Order execution
│   ├── src/
│   │   ├── broker.rs          # BrokerAdapter trait, configs
│   │   ├── alpaca.rs          # Alpaca REST + WebSocket
│   │   ├── simulator.rs       # Realistic simulator
│   │   └── oms.rs             # OrderManagementSystem
│   └── Cargo.toml
├── bt-tui/                    # Terminal UI
│   ├── src/
│   │   ├── app.rs             # Main app loop
│   │   ├── command.rs         # Command parser (Bloomberg + colon)
│   │   ├── keybindings.rs     # Keybinding manager
│   │   ├── layout.rs          # Layout manager, workspaces
│   │   ├── theme.rs           # Themes (Bloomberg, Dark, Light)
│   │   └── widgets/
│   │       ├── market_overview.rs
│   │       ├── security_detail.rs
│   │       ├── chart.rs
│   │       ├── order_book.rs
│   │       ├── news.rs
│   │       ├── portfolio.rs
│   │       └── ki_assistant.rs
│   └── Cargo.toml
└── bt-cli/                    # CLI entry point
    ├── src/
    │   └── main.rs            # Commands: run, backtest, validate, config, doctor
    └── Cargo.toml
```

## 🧪 Testing

```bash
# Unit tests
cargo test --workspace

# Integration tests (requires API keys)
cargo test --workspace --features integration-tests

# Property-based testing
cargo test --workspace proptest
```

## 📈 Performance Targets

| Metric | Target |
|--------|--------|
| Signal Generation Latency | < 1ms |
| Order Submission Latency | < 10ms |
| Kill Switch Activation | < 100ms |
| Memory Usage (idle) | < 200MB |
| CPU Usage (idle) | < 5% |
| WebSocket Reconnection | < 5s |

## 🔐 Security

- API keys loaded from environment variables only
- No secrets in config files
- Audit logging with hash chains
- Read-only mode for research
- Kill switch accessible via hardware button (GPIO) - planned

## 🚧 Roadmap

- [ ] Interactive Brokers (IBKR) adapter
- [ ] WebSocket reconnection with exponential backoff
- [ ] Prometheus metrics endpoint (`/metrics`)
- [ ] Web-based dashboard (optional)
- [ ] Options Greeks calculation & display
- [ ] Portfolio margin support
- [ ] Multi-account support
- [ ] Strategy marketplace
- [ ] AI-assisted strategy generation (Ki Copilot)
- [ ] Mobile notifications via Push API

## ⚠️ Disclaimer

**This software is for educational and research purposes. Live trading involves substantial risk of loss and is not suitable for all investors. Past performance does not guarantee future results. The authors accept no liability for financial losses incurred through the use of this software.**

Always:
- Test thoroughly in paper trading first
- Understand the strategy and risk parameters
- Monitor positions actively
- Have a manual exit plan
- Never risk more than you can afford to lose

## 📄 License

Licensed under either of:
- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

## 🤝 Contributing

Contributions welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 🙏 Acknowledgments

- Bloomberg Terminal for inspiration
- `ratatui` team for the excellent TUI framework
- `pest` for the parser generator
- All open-source Rust crate authors