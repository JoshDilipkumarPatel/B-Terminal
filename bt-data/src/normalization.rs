use anyhow::Result;
use bt_core::types::{Symbol, Venue, AssetClass, OptionType};
use chrono::{DateTime, Utc, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizationConfig {
    pub alias_map: HashMap<String, Symbol>,
    pub venue_priority: Vec<Venue>,
    pub default_venue: Venue,
}

impl Default for NormalizationConfig {
    fn default() -> Self {
        let mut alias_map = HashMap::new();
        // Common aliases
        alias_map.insert("SPX".to_string(), Symbol::new(Venue::Polygon, "SPX", AssetClass::Equity));
        alias_map.insert("SPY".to_string(), Symbol::new(Venue::Polygon, "SPY", AssetClass::Equity));
        alias_map.insert("QQQ".to_string(), Symbol::new(Venue::Polygon, "QQQ", AssetClass::Equity));
        alias_map.insert("DIA".to_string(), Symbol::new(Venue::Polygon, "DIA", AssetClass::Equity));
        alias_map.insert("IWM".to_string(), Symbol::new(Venue::Polygon, "IWM", AssetClass::Equity));
        alias_map.insert("VIX".to_string(), Symbol::new(Venue::Polygon, "VIX", AssetClass::Equity));
        alias_map.insert("BTC".to_string(), Symbol::crypto(Venue::Binance, "BTCUSDT"));
        alias_map.insert("ETH".to_string(), Symbol::crypto(Venue::Binance, "ETHUSDT"));
        alias_map.insert("SOL".to_string(), Symbol::crypto(Venue::Binance, "SOLUSDT"));

        Self {
            alias_map,
            venue_priority: vec![Venue::Polygon, Venue::Binance, Venue::Coinbase, Venue::Alpaca],
            default_venue: Venue::Polygon,
        }
    }
}

pub struct SymbolNormalizer {
    config: Arc<RwLock<NormalizationConfig>>,
    cache: Arc<RwLock<HashMap<String, Symbol>>>,
}

impl SymbolNormalizer {
    pub fn new(config: NormalizationConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn normalize(&self, input: &str) -> Result<Symbol> {
        let normalized = input.trim().to_uppercase();

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(symbol) = cache.get(&normalized) {
                return Ok(symbol.clone());
            }
        }

        // Check alias map
        {
            let config = self.config.read().await;
            if let Some(symbol) = config.alias_map.get(&normalized) {
                let mut cache = self.cache.write().await;
                cache.insert(normalized.clone(), symbol.clone());
                return Ok(symbol.clone());
            }
        }

        // Try parsing as full symbol format: VENUE:TICKER[:EXPIRY[:STRIKE[:TYPE]]]
        if normalized.contains(':') {
            return Symbol::from_str(&normalized);
        }

        // Try to determine from format
        // Crypto pairs often end with USDT, USDC, BTC, ETH
        let crypto_quotes = ["USDT", "USDC", "BUSD", "BTC", "ETH", "SOL", "BNB"];
        for quote in crypto_quotes {
            if normalized.ends_with(quote) && normalized.len() > quote.len() {
                let base = &normalized[..normalized.len() - quote.len()];
                let symbol = Symbol::crypto(Venue::Binance, format!("{}{}", base, quote));
                let mut cache = self.cache.write().await;
                cache.insert(normalized.clone(), symbol.clone());
                return Ok(symbol);
            }
        }

        // Options format: SYMBOL<YYMMDD>[C|P]<STRIKE>
        // e.g., AAPL240119C00150000
        if let Some(symbol) = Self::parse_option_format(&normalized)? {
            let mut cache = self.cache.write().await;
            cache.insert(normalized.clone(), symbol.clone());
            return Ok(symbol);
        }

        // Futures format: SYMBOL<YYMMDD>
        if let Some(symbol) = Self::parse_future_format(&normalized)? {
            let mut cache = self.cache.write().await;
            cache.insert(normalized.clone(), symbol.clone());
            return Ok(symbol);
        }

        // Default to equity on default venue
        let config = self.config.read().await;
        let symbol = Symbol::new(config.default_venue, normalized.clone(), AssetClass::Equity);
        let mut cache = self.cache.write().await;
        cache.insert(normalized.clone(), symbol.clone());
        Ok(symbol)
    }

    fn parse_option_format(input: &str) -> Result<Option<Symbol>> {
        // OCC format: ROOT + YYMMDD + C/P + STRIKE(8 digits with 3 decimals implied)
        // e.g., AAPL240119C00150000 = AAPL, 2024-01-19, Call, $150.00
        if input.len() < 15 {
            return Ok(None);
        }

        // Find the date part (6 digits)
        let mut date_pos = None;
        for i in 0..=input.len() - 6 {
            let slice = &input[i..i+6];
            if slice.chars().all(|c| c.is_ascii_digit()) {
                // Check if followed by C or P
                if i + 6 < input.len() {
                    let cp = &input[i+6..i+7];
                    if cp == "C" || cp == "P" {
                        date_pos = Some(i);
                        break;
                    }
                }
            }
        }

        if let Some(pos) = date_pos {
            let root = &input[..pos];
            let date_str = &input[pos..pos+6];
            let cp = &input[pos+6..pos+7];
            if input.len() < pos + 15 {
                return Ok(None);
            }
            let strike_str = &input[pos+7..];

            if strike_str.len() == 8 && strike_str.chars().all(|c| c.is_ascii_digit()) {
                let year = 2000 + date_str[0..2].parse::<i32>()?;
                let month = date_str[2..4].parse::<u32>()?;
                let day = date_str[4..6].parse::<u32>()?;

                let expiry = NaiveDate::from_ymd_opt(year, month, day)
                    .and_then(|d| d.and_hms_opt(16, 0, 0)) // 4PM ET
                    .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

                let strike_raw = strike_str.parse::<i64>()?;
                let strike = Decimal::new(strike_raw, 3); // 3 decimal places implied

                if let Some(expiry) = expiry {
                    let option_type = match cp {
                        "C" => OptionType::Call,
                        "P" => OptionType::Put,
                        _ => return Ok(None),
                    };

                    return Ok(Some(Symbol::option(
                        Venue::Polygon,
                        root,
                        expiry,
                        strike,
                        option_type,
                    )));
                }
            }
        }

        Ok(None)
    }

    fn parse_future_format(input: &str) -> Result<Option<Symbol>> {
        // Format: ROOT + YYMMDD
        // e.g., ES240315 = ES, 2024-03-15
        if input.len() < 7 {
            return Ok(None);
        }

        // Check last 6 chars are digits
        let len = input.len();
        let date_str = &input[len-6..];
        let root = &input[..len-6];

        if date_str.chars().all(|c| c.is_ascii_digit()) && !root.is_empty() {
            let year = 2000 + date_str[0..2].parse::<i32>()?;
            let month = date_str[2..4].parse::<u32>()?;
            let day = date_str[4..6].parse::<u32>()?;

            let expiry = NaiveDate::from_ymd_opt(year, month, day)
                .and_then(|d| d.and_hms_opt(16, 0, 0))
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

            if let Some(expiry) = expiry {
                return Ok(Some(Symbol::future(
                    Venue::Polygon,
                    root,
                    expiry,
                )));
            }
        }

        Ok(None)
    }

    pub async fn add_alias(&self, alias: &str, symbol: Symbol) {
        let mut config = self.config.write().await;
        config.alias_map.insert(alias.to_uppercase(), symbol);
    }

    pub async fn set_default_venue(&self, venue: Venue) {
        let mut config = self.config.write().await;
        config.default_venue = venue;
    }
}

impl Default for SymbolNormalizer {
    fn default() -> Self {
        Self::new(NormalizationConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_normalize_equity() {
        let normalizer = SymbolNormalizer::default();
        let symbol = normalizer.normalize("AAPL").await.unwrap();
        assert_eq!(symbol.ticker, "AAPL");
        assert_eq!(symbol.asset_class, AssetClass::Equity);
    }

    #[tokio::test]
    async fn test_normalize_crypto() {
        let normalizer = SymbolNormalizer::default();
        let symbol = normalizer.normalize("BTCUSDT").await.unwrap();
        assert_eq!(symbol.ticker, "BTCUSDT");
        assert_eq!(symbol.asset_class, AssetClass::Crypto);
        assert_eq!(symbol.venue, Venue::Binance);
    }

    #[tokio::test]
    async fn test_parse_option() {
        let normalizer = SymbolNormalizer::default();
        // AAPL240119C00150000 = AAPL, 2024-01-19, Call, $150.00
        let symbol = normalizer.normalize("AAPL240119C00150000").await.unwrap();
        assert_eq!(symbol.ticker, "AAPL");
        assert_eq!(symbol.asset_class, AssetClass::Option);
        assert_eq!(symbol.strike, Some(Decimal::new(150000, 3)));
        assert_eq!(symbol.option_type, Some(OptionType::Call));
    }

    #[tokio::test]
    async fn test_parse_future() {
        let normalizer = SymbolNormalizer::default();
        // ES240315 = ES, 2024-03-15
        let symbol = normalizer.normalize("ES240315").await.unwrap();
        assert_eq!(symbol.ticker, "ES");
        assert_eq!(symbol.asset_class, AssetClass::Future);
        assert!(symbol.expiry.is_some());
    }

    #[tokio::test]
    async fn test_full_format() {
        let normalizer = SymbolNormalizer::default();
        let symbol = normalizer.normalize("POLYGON:AAPL").await.unwrap();
        assert_eq!(symbol.venue, Venue::Polygon);
        assert_eq!(symbol.ticker, "AAPL");
    }
}
