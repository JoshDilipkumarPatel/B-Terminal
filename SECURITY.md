# Security Policy

As a multi-asset quantitative trading terminal and autonomous algorithmic execution system, security and operational safety are paramount in **B-Terminal**. This document explains best practices for securing your credentials and how to report vulnerabilities.

---

## 🔐 API Key & Credential Safety

When configuring connections to live broker architectures (e.g., Groww, CoinDCX, Binance, Alpaca, Interactive Brokers):
1. **Never Share Your API Secrets**: Your API Secret gives programmatic access to fund execution and portfolio accounting. Never paste your API keys or secrets in bug reports, screenshots, public issue trackers, or chat channels.
2. **Use Environment Variables**: Store sensitive credentials inside a local `.env` file or locally ignored configuration overrides (`config.local.toml`). Both are explicitly ignored in version control via `.gitignore`.
3. **Restrict API Permissions**: Wherever supported by your brokerage platform:
   - Restrict access exclusively to specific static IP addresses.
   - Grant **Trade/Order Execution** and **Read Market Data** permissions only.
   - **Disable Withdrawal Permissions** on all API keys configured within B-Terminal.
4. **Use Paper Trading First**: Always validate quantitative trading rules and AI autonomous predictions using `--mode paper` simulation or sandbox brokerage endpoints before executing with live portfolio capital.

---

## 🛑 Autonomous & Algorithmic Risk Protections

B-Terminal integrates several safety mechanisms to guard against aberrant high-frequency market shocks or runaway order logic:
- **Global Kill Switch (`bt-core/src/kill_switch.rs`)**: Monitors portfolio equity drawdown in real-time. If intraday losses exceed defined risk tolerances (e.g., 2% of total equity), the system engages an atomic kill switch that instantly aborts all outgoing order routines and attempts to flatten active exposure.
- **Kelly Criterion Envelope**: The AI Autopilot utilizes conservative Half-Kelly allocation scaling to prevent over-leveraging during volatile regimes.

---

## 🛡 Reporting Vulnerabilities

If you discover a potential security flaw, authentication bypass, or data leakage flaw in B-Terminal's network or data execution adapters:
- **Do not open a public GitHub issue.**
- Please email the maintainers or project security lead privately with details, reproduction steps, and potential mitigation suggestions.
- All confirmed security vulnerability disclosures will be addressed with emergency bug-fix patches and credited appropriately in official release advisory notes.
