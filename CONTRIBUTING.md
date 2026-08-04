# Contributing to B-Terminal

We welcome contributions from developers, quantitative traders, algorithmic researchers, and open-source enthusiasts! Whether you are implementing a new broker adapter, enhancing the AI predictive engines, building TUI dashboard components, or reporting bugs, your contributions help make B-Terminal the premier open-source multi-asset trading terminal.

---

## 🛠 Getting Started

### 1. Prerequisites
- **Rust Toolchain**: Install stable Rust (1.80+ recommended) via [rustup.rs](https://rustup.rs/).
- **Git**: Ensure Git is installed and configured on your workstation.
- **Optional API Credentials**: If testing live executions, you will need sandbox/paper or live API keys for supported broker architectures (Groww, CoinDCX, Alpaca, Binance).

### 2. Fork & Clone
```bash
git clone https://github.com/<your-username>/B-Terminal.git
cd B-Terminal
```

### 3. Build & Run Tests
Verify your environment is clean before making modifications:
```bash
# Build the entire multi-crate workspace
cargo build --all

# Run all automated unit and integration tests
cargo test --all
```

---

## 🏗 Workspace Architecture

When contributing code, place your modifications in the appropriate crate:
- **`bt-core`**: Core event definitions, Order/Trade types, Symbol structures, and risk parameter interfaces.
- **`bt-data`**: Market data feed connectors, WebSocket streaming adapters (Groww, CoinDCX, Binance, Polygon), and historical data caching.
- **`bt-strategy`**: Quantitative calculation engines, technical indicators (RSI, Bollinger Bands, EMA/MACD, ATR), quantitative DSL parser, and **Ki Assistant AI Predictor** regression modeling (`TrendPredictor`).
- **`bt-execution`**: Order Management System (OMS), position reconciliation, risk limits, and broker execution adapters.
- **`bt-tui`**: Bloomberg-inspired dynamic rich Terminal User Interface dashboards and reactive interactive widgets.
- **`bt-cli`**: Command-line interface binaries, configuration parsing, and top-level commands (`tui`, `predict`, `autopilot`, `backtest`).

---

## 📝 Contribution Workflow

1. **Create a Branch**: Create a feature or bugfix branch off `main` (e.g., `git checkout -b feature/zerodha-adapter` or `fix/rsi-warmup-calculation`).
2. **Implement Cleanly**: Follow standard Rust idioms, avoid unhandled `.unwrap()` panics in production execution paths, and preserve modular crate encapsulation.
3. **Add Unit Tests**: Ensure new statistical formulas, broker data mapping functions, or strategy parsers come accompanied by targeted unit tests in the same module.
4. **Formatting & Linting**: Run `cargo fmt` and check against `cargo clippy --workspace` before submitting code.
5. **Security Check**: Never commit live API keys, secrets, or `.env` files. Always reference `config.example.toml` and `.env.example` in pull requests.

---

## 🚀 Submitting Your Pull Request

Provide a clear description in your Pull Request explaining:
- What functionality was added or what problem was addressed.
- Verification methodology (unit test commands executed, live/paper simulation logs).
- Any breaking changes to configuration format or CLI syntax.

Thank you for contributing to the open-source financial algorithmic community!
