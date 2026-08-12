//! Dynamic correlation groups for aggregate sector-exposure risk checks.
//!
//! Extracted from `risk_limits.rs` so the clustering logic can grow independently.
//! `risk_limits.rs` re-exports these items, so existing `bt_core::risk_limits::*`
//! paths keep working.

use std::collections::HashMap;

/// Categorizes instruments into correlated asset/sector groups (e.g., banking equities, energy, tech, cryptocurrencies).
/// Used by aggregate portfolio exposure checks to prevent multi-agent or algorithmic circumvention of sizing limits.
pub fn get_correlation_group(ticker: &str) -> &'static str {
    let t_upper = ticker.to_uppercase();
    match t_upper.as_str() {
        "HDFCBANK" | "ICICIBANK" | "SBIN" | "KOTAKBANK" | "AXISBANK" => "IN_BANKING",
        "RELIANCE" | "ONGC" | "IOC" | "BPCL" => "IN_ENERGY",
        "TCS" | "INFY" | "WIPRO" | "HCLTECH" | "TECHM" | "AAPL" | "MSFT" | "GOOGL" | "NVDA" => "TECH_SECTOR",
        s if s.starts_with("BTC") || s.starts_with("ETH") || s.starts_with("SOL") || s.ends_with("USDT") || s.ends_with("INR") && (s.contains("BTC") || s.contains("ETH")) => "CRYPTO_ASSET",
        _ => "GENERAL_EQUITY",
    }
}

/// Default number of `update_returns` calls between dynamic-group re-clusters
/// (100 ≈ 100 trading days when fed once per day per position).
pub const DEFAULT_RECOMPUTE_INTERVAL: usize = 100;

/// Dynamically computes correlation groups from live return data.
/// Starts with the static seed taxonomy from `get_correlation_group()`,
/// then self-corrects as live daily returns flow in via Pearson correlation.
///
/// Recompute is **interval-gated**: `update_returns()` re-clusters only every
/// `recompute_interval` calls (default 100). `recompute_groups()` forces an
/// immediate re-cluster for scheduled tasks / tests.
#[derive(Debug)]
pub struct CorrelationGroupRegistry {
    return_history: HashMap<String, Vec<f64>>,
    dynamic_groups: HashMap<String, String>,
    window_size: usize,
    correlation_threshold: f64,
    recompute_interval: usize,
    recompute_counter: usize,
}

impl Default for CorrelationGroupRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CorrelationGroupRegistry {
    pub fn new() -> Self {
        Self {
            return_history: HashMap::new(),
            dynamic_groups: HashMap::new(),
            window_size: 30,
            correlation_threshold: 0.70,
            recompute_interval: DEFAULT_RECOMPUTE_INTERVAL,
            recompute_counter: 0,
        }
    }

    /// Override the recompute interval (in `update_returns` calls). Minimum 1.
    pub fn with_recompute_interval(mut self, interval: usize) -> Self {
        self.recompute_interval = interval.max(1);
        self
    }

    /// Push a daily return observation for a ticker.
    /// Interval-gated: re-clusters dynamic groups every `recompute_interval` calls.
    pub fn update_returns(&mut self, ticker: &str, daily_return: f64) {
        let history = self.return_history.entry(ticker.to_string()).or_default();
        history.push(daily_return);
        if history.len() > self.window_size {
            history.remove(0);
        }

        self.recompute_counter = self.recompute_counter.saturating_add(1);
        if self.recompute_counter >= self.recompute_interval {
            self.recompute_counter = 0;
            self.do_recompute();
        }
    }

    /// Force an immediate re-cluster regardless of the interval (scheduled tasks / tests).
    pub fn recompute_groups(&mut self) {
        self.recompute_counter = 0;
        self.do_recompute();
    }

    /// Look up a ticker's correlation group: dynamic first, then static fallback.
    pub fn get_group(&self, ticker: &str) -> &str {
        if let Some(group) = self.dynamic_groups.get(ticker) {
            group.as_str()
        } else {
            get_correlation_group(ticker)
        }
    }

    /// Recompute dynamic groups using Pearson correlation across all tickers
    /// with sufficient return history.
    fn do_recompute(&mut self) {
        let tickers: Vec<String> = self.return_history.keys()
            .filter(|t| self.return_history[*t].len() >= self.window_size)
            .cloned()
            .collect();

        if tickers.len() < 2 {
            return;
        }

        // Union-find parent map
        let mut parent: HashMap<String, String> = tickers.iter()
            .map(|t| (t.clone(), t.clone()))
            .collect();

        fn find(parent: &mut HashMap<String, String>, x: &str) -> String {
            let p = parent[x].clone();
            if p == x {
                return x.to_string();
            }
            let root = find(parent, &p);
            parent.insert(x.to_string(), root.clone());
            root
        }

        for i in 0..tickers.len() {
            for j in (i + 1)..tickers.len() {
                let corr = Self::pearson(
                    &self.return_history[&tickers[i]],
                    &self.return_history[&tickers[j]],
                );
                if corr > self.correlation_threshold {
                    let root_i = find(&mut parent, &tickers[i]);
                    let root_j = find(&mut parent, &tickers[j]);
                    if root_i != root_j {
                        parent.insert(root_j, root_i);
                    }
                }
            }
        }

        self.dynamic_groups.clear();
        let mut group_counter: HashMap<String, usize> = HashMap::new();
        let mut next_id = 0usize;
        for ticker in &tickers {
            let root = find(&mut parent, ticker);
            let group_id = *group_counter.entry(root).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            });
            self.dynamic_groups.insert(ticker.clone(), format!("DYN_GROUP_{}", group_id));
        }
    }

    /// Pearson correlation coefficient between two equal-length slices.
    fn pearson(x: &[f64], y: &[f64]) -> f64 {
        let n = x.len().min(y.len());
        if n == 0 {
            return 0.0;
        }
        let n_f = n as f64;
        let mean_x: f64 = x[..n].iter().sum::<f64>() / n_f;
        let mean_y: f64 = y[..n].iter().sum::<f64>() / n_f;

        let mut cov = 0.0;
        let mut var_x = 0.0;
        let mut var_y = 0.0;
        for i in 0..n {
            let dx = x[i] - mean_x;
            let dy = y[i] - mean_y;
            cov += dx * dy;
            var_x += dx * dx;
            var_y += dy * dy;
        }

        let denom = (var_x * var_y).sqrt();
        if denom < 1e-12 {
            return 0.0;
        }
        cov / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recompute_interval_gating() {
        // Interval 30: with 2 tickers × 30 pushes = 60 update_returns calls,
        // a re-cluster fires at counter == 30 (only 1 ticker has history yet → no-op)
        // and again at 60 (both have full history → clustered).
        let mut reg = CorrelationGroupRegistry::new().with_recompute_interval(30);

        for i in 0..30 {
            reg.update_returns("AAA", (i as f64) * 0.01);
            reg.update_returns("BBB", (i as f64) * 0.01);
        }

        // Perfectly correlated tickers must land in the same dynamic group.
        let group_a = reg.get_group("AAA").to_string();
        let group_b = reg.get_group("BBB").to_string();
        assert_eq!(group_a, group_b, "Correlated tickers should share a dynamic group");
        assert!(group_a.starts_with("DYN_GROUP_"), "Expected a dynamic group, got '{}'", group_a);
    }

    #[test]
    fn test_unknown_ticker_falls_back_to_static() {
        let mut reg = CorrelationGroupRegistry::new();
        reg.update_returns("ZZZ", 0.01);
        assert_eq!(reg.get_group("ZZZ"), "GENERAL_EQUITY");
        assert_eq!(reg.get_group("HDFCBANK"), "IN_BANKING");
    }
}
