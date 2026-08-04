use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, ACCEPT, REFERER};
use serde::{Deserialize, Serialize};

use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptionContractData {
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_interest: u64,
    pub change_in_open_interest: i64,
    pub implied_volatility: f64,
    pub last_price: f64,
    pub bid_qty: u64,
    pub ask_qty: u64,
    pub total_traded_volume: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NseOptionChainSnapshot {
    pub symbol: String,
    pub timestamp: String,
    pub underlying_value: f64,
    pub calls: Vec<OptionContractData>,
    pub puts: Vec<OptionContractData>,
}

#[derive(Debug, Deserialize)]
struct NseRawResponse {
    records: Option<NseRecords>,
}

#[derive(Debug, Deserialize)]
struct NseRecords {
    #[serde(rename = "underlyingValue")]
    underlying_value: f64,
    timestamp: String,
    data: Vec<NseRawDataRow>,
}

#[derive(Debug, Deserialize)]
struct NseRawDataRow {
    #[serde(rename = "strikePrice")]
    strike_price: f64,
    #[serde(rename = "expiryDate")]
    expiry_date: String,
    #[serde(rename = "CE")]
    ce: Option<NseRawContract>,
    #[serde(rename = "PE")]
    pe: Option<NseRawContract>,
}

#[derive(Debug, Deserialize)]
struct NseRawContract {
    #[serde(rename = "openInterest", default)]
    open_interest: u64,
    #[serde(rename = "changeinOpenInterest", default)]
    change_in_open_interest: i64,
    #[serde(rename = "impliedVolatility", default)]
    implied_volatility: f64,
    #[serde(rename = "lastPrice", default)]
    last_price: f64,
    #[serde(rename = "bidQty", default)]
    bid_qty: u64,
    #[serde(rename = "askQty", default)]
    ask_qty: u64,
    #[serde(rename = "totalTradedVolume", default)]
    total_traded_volume: u64,
}

pub struct NsePublicConnector {
    client: reqwest::Client,
    base_url: String,
}

impl NsePublicConnector {
    pub fn new() -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
        headers.insert(REFERER, HeaderValue::from_static("https://www.nseindia.com/option-chain"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .timeout(Duration::from_secs(10))
            .build()?;

        Ok(Self {
            client,
            base_url: "https://www.nseindia.com".to_string(),
        })
    }

    /// Initializes cookie session by pinging NSE landing page
    pub async fn establish_session(&self) -> anyhow::Result<()> {
        let _resp = self.client.get(&self.base_url).send().await?;
        Ok(())
    }

    /// Parses a raw JSON string into a structured NseOptionChainSnapshot
    pub fn parse_option_chain_json(symbol: &str, json_str: &str) -> anyhow::Result<NseOptionChainSnapshot> {
        let parsed: NseRawResponse = serde_json::from_str(json_str)?;
        let records = parsed.records.ok_or_else(|| anyhow::anyhow!("Missing records in NSE payload"))?;
        
        let mut calls = Vec::new();
        let mut puts = Vec::new();

        for row in records.data {
            if let Some(c) = row.ce {
                calls.push(OptionContractData {
                    strike_price: row.strike_price,
                    expiry_date: row.expiry_date.clone(),
                    open_interest: c.open_interest,
                    change_in_open_interest: c.change_in_open_interest,
                    implied_volatility: c.implied_volatility,
                    last_price: c.last_price,
                    bid_qty: c.bid_qty,
                    ask_qty: c.ask_qty,
                    total_traded_volume: c.total_traded_volume,
                });
            }
            if let Some(p) = row.pe {
                puts.push(OptionContractData {
                    strike_price: row.strike_price,
                    expiry_date: row.expiry_date.clone(),
                    open_interest: p.open_interest,
                    change_in_open_interest: p.change_in_open_interest,
                    implied_volatility: p.implied_volatility,
                    last_price: p.last_price,
                    bid_qty: p.bid_qty,
                    ask_qty: p.ask_qty,
                    total_traded_volume: p.total_traded_volume,
                });
            }
        }

        Ok(NseOptionChainSnapshot {
            symbol: symbol.to_string(),
            timestamp: records.timestamp,
            underlying_value: records.underlying_value,
            calls,
            puts,
        })
    }

    /// Fetches real-time option chain for index symbols like "NIFTY" or "BANKNIFTY"
    pub async fn get_index_option_chain(&self, symbol: &str) -> anyhow::Result<NseOptionChainSnapshot> {
        let url = format!("{}/api/option-chain-indices?symbol={}", self.base_url, symbol);
        let resp_text = self.client.get(&url).send().await?.text().await?;
        Self::parse_option_chain_json(symbol, &resp_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nse_option_chain_payload() {
        let mock_json = r#"
        {
          "records": {
            "underlyingValue": 23500.75,
            "timestamp": "03-Aug-2026 15:30:00",
            "data": [
              {
                "strikePrice": 23500.0,
                "expiryDate": "06-Aug-2026",
                "CE": {
                  "openInterest": 150000,
                  "changeinOpenInterest": 12000,
                  "impliedVolatility": 14.5,
                  "lastPrice": 185.50,
                  "bidQty": 750,
                  "askQty": 800,
                  "totalTradedVolume": 450000
                },
                "PE": {
                  "openInterest": 180000,
                  "changeinOpenInterest": -5000,
                  "impliedVolatility": 15.2,
                  "lastPrice": 170.25,
                  "bidQty": 1000,
                  "askQty": 950,
                  "totalTradedVolume": 520000
                }
              }
            ]
          }
        }
        "#;

        let snapshot = NsePublicConnector::parse_option_chain_json("NIFTY", mock_json).unwrap();
        assert_eq!(snapshot.symbol, "NIFTY");
        assert_eq!(snapshot.underlying_value, 23500.75);
        assert_eq!(snapshot.calls.len(), 1);
        assert_eq!(snapshot.puts.len(), 1);
        
        let call_atm = &snapshot.calls[0];
        assert_eq!(call_atm.strike_price, 23500.0);
        assert_eq!(call_atm.implied_volatility, 14.5);
        assert_eq!(call_atm.open_interest, 150000);
        
        let put_atm = &snapshot.puts[0];
        assert_eq!(put_atm.last_price, 170.25);
        assert_eq!(put_atm.change_in_open_interest, -5000);
    }
}
