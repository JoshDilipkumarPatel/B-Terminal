# Institutional Security & Governance Policy
**B-Terminal v2.5 Autonomous Quantitative & Multi-Agent Execution Platform**

Security, credential secrecy, capital preservation, and deterministic runtime safety are the foundational pillars of **B-Terminal**. This document outlines our institutional security architecture, operational safety protocols, and vulnerability reporting procedures.

---

## 🔐 1. Credential Security & Memory Protection

### A. In-Place Heap Memory Scrubbing (`bt-execution::angel_one`)
To prevent memory scrapers or core dump analysis from harvesting sensitive authentication credentials:
* **Zeroization on Drop & Teardown**: Broker adapters storing plaintext passwords, MPINs, and TOTP secret keys implement custom memory zeroizing routines (`scrub_sensitive_credentials`).
* **Direct Heap Byte Overwriting**: Rather than reassigning strings (which abandons the un-scrubbed original memory buffer on the heap for eventual garbage collection), B-Terminal modifies the existing String vector buffer directly via unsafe byte iteration (`*b = 0`), followed by truncating the length and appending a explicit `"[SCRUBBED]"` verification tag.
* **Automatic Teardown Trigger**: Scrubbing is automatically executed immediately upon session termination (`disconnect()`) or when the configuration struct goes out of scope via explicit `Drop` implementations.

### B. Option vs Empty String Verification
* **Strict Authentication Guards**: The `BrokerCredentials::validate_required` method strictly discriminates between unassigned optional credentials (`None`) and explicitly supplied empty strings (`Some("")`). Any configuration attempting to initialize a broker connector with an empty string key, secret, or token is instantly rejected at startup, regardless of optional configuration parameters.

### C. Cryptographic Payload Signing
* **HMAC-SHA256 Authorization**: High-security exchange integrations (such as CoinDCX and Binance) generate authenticated cryptographic signatures directly within the order routing pipeline (`place_order`), ensuring order payloads cannot be intercepted or tampered with in transit.

---

## 🛑 2. Production Runtime & Broker Safety Lockouts

### A. Stub Broker Safety Lockout (`enforce_stub_broker_safety`)
To protect traders from accidentally deploying live financial capital against developing or unauthenticated brokerage connectors (e.g., Zerodha, Groww, Upstox, CoinDCX):
* **Execution Guarding**: Critical state-changing trait methods—specifically `connect()`, `place_order()`, `cancel_order()`, and `cancel_all_orders()`—are protected by runtime safety assertions. If executed in a production environment without explicit compilation overrides (`cfg!(feature = "stub-brokers")`), the system aborts the operation immediately with an informative security error.
* **Paper & Introspection Separation**: To facilitate operational monitoring and dashboard research, stub adapters explicitly return `is_paper() == true` and allow unrestricted execution of read-only introspection methods (`get_account()`, `get_positions()`, `health_check()`).

---

## 🛡️ 3. Risk Management & Governance Controls

### A. Conservative Default Limits (`bt-core::config`)
All financial execution parameters default to strict capital preservation boundaries:
* **Maximum Default Leverage**: Strictly capped at **`1.0x`** across both Global Portfolio Risk and Strategy-Level Risk parameters to prevent leveraged blowup regimes.
* **Maximum Default Order Size**: Limited to **`$1,000`** (or 1.0% portfolio position sizing on standard account structures).
* **Audit Trail Key Management**: Encrypted audit logs employ tamper-evident hash chaining linked to dedicated environment encryption keys (`B_TERMINAL_AUDIT_KEY`) and enforce systematic 30-day key rotation schedules.

### B. Multi-Layered Emergency Protection
1. **Global Kill Switch (`bt-core::kill_switch`)**: Monitors real-time intraday PnL and equity drawdowns using atomic concurrency tracking (`Arc<AtomicBool>`). Upon breaching defined drawdown thresholds, an emergency liquidation routine cancels all resting orders and flattens active exposure in $<100\text{ms}$.
2. **Intelligent Capital Alarms (`bt-core::alarms`)**: Designed to prevent alarm fatigue while safeguarding assets:
   - **Normal Regime**: Completely silent during standard portfolio growth and normal volatility oscillations.
   - **Caution Regime**: Emits non-intrusive visual amber warnings during minor capital dips.
   - **Emergency Liquidation Alarm**: Activates auditory/terminal alarms exclusively when sharp capital impairment or excessive liquidation threatens portfolio solvency.

---

## 🚨 4. Vulnerability Reporting Protocol

If you discover a potential security flaw, authentication bypass, cryptographic discrepancy, or memory zeroization bug:
1. **Do NOT open a public GitHub issue.**
2. Report your findings directly to the lead institutional maintainers via private email or security advisory communication.
3. Include clear reproduction instructions, affected code paths, and observed behavior.
4. All confirmed security disclosures will be prioritized for immediate emergency patching and documented in subsequent release security advisories.
