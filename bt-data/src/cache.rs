use anyhow::Result;
use bt_core::events::{Bar, Quote, Timeframe};
use bt_core::types::Symbol;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCacheConfig {
    pub enabled: bool,
    pub db_path: PathBuf,
    pub memory_ttl_seconds: u64,
    pub max_bars_per_symbol: usize,
}

impl Default for DataCacheConfig {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bloomberg-terminal");
        Self {
            enabled: true,
            db_path: data_dir.join("cache.db"),
            memory_ttl_seconds: 300,
            max_bars_per_symbol: 10000,
        }
    }
}

#[derive(Debug, Clone)]
struct CachedBar {
    bar: Bar,
    cached_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct CachedQuote {
    quote: Quote,
    cached_at: DateTime<Utc>,
}

pub struct BarCache {
    config: DataCacheConfig,
    memory: Arc<RwLock<HashMap<Symbol, Vec<CachedBar>>>>,
    db: Option<Pool<Sqlite>>,
}

impl BarCache {
    pub async fn new(config: DataCacheConfig) -> Result<Self> {
        let db = if config.enabled {
            std::fs::create_dir_all(config.db_path.parent().unwrap_or(&PathBuf::from(".")))?;
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect(&format!("sqlite:{}?mode=rwc", config.db_path.display()))
                .await?;
            Self::init_db(&pool).await?;
            Some(pool)
        } else {
            None
        };

        Ok(Self {
            config,
            memory: Arc::new(RwLock::new(HashMap::new())),
            db,
        })
    }

    async fn init_db(pool: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bars (
                venue TEXT NOT NULL,
                ticker TEXT NOT NULL,
                asset_class TEXT NOT NULL,
                timeframe TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                open TEXT NOT NULL,
                high TEXT NOT NULL,
                low TEXT NOT NULL,
                close TEXT NOT NULL,
                volume TEXT NOT NULL,
                vwap TEXT,
                trade_count INTEGER,
                expiry INTEGER,
                strike TEXT,
                option_type TEXT,
                PRIMARY KEY (venue, ticker, asset_class, timeframe, timestamp)
            )
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_bars_lookup
            ON bars (venue, ticker, asset_class, timeframe, timestamp DESC)
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn put(&self, bar: Bar) -> Result<()> {
        let symbol = bar.symbol.clone();
        let timeframe = bar.timeframe;

        // Update memory cache
        {
            let mut mem = self.memory.write().await;
            let entry = mem.entry(symbol.clone()).or_default();
            entry.push(CachedBar {
                bar: bar.clone(),
                cached_at: Utc::now(),
            });
            if entry.len() > self.config.max_bars_per_symbol {
                entry.drain(0..entry.len() - self.config.max_bars_per_symbol);
            }
        }

        // Persist to DB
        if let Some(db) = &self.db {
            let (expiry, strike, option_type) = match symbol.asset_class {
                bt_core::types::AssetClass::Option => (
                    symbol.expiry.map(|d| d.timestamp()),
                    symbol.strike.map(|s| s.to_string()),
                    symbol.option_type.map(|t| t as u8 as i64),
                ),
                bt_core::types::AssetClass::Future => (
                    symbol.expiry.map(|d| d.timestamp()),
                    None,
                    None,
                ),
                _ => (None, None, None),
            };

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO bars
                (venue, ticker, asset_class, timeframe, timestamp, open, high, low, close, volume, vwap, trade_count, expiry, strike, option_type)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(symbol.venue as u8 as i64)
            .bind(&symbol.ticker)
            .bind(symbol.asset_class as u8 as i64)
            .bind(timeframe as u8 as i64)
            .bind(bar.timestamp.timestamp())
            .bind(bar.open.to_string())
            .bind(bar.high.to_string())
            .bind(bar.low.to_string())
            .bind(bar.close.to_string())
            .bind(bar.volume.to_string())
            .bind(bar.vwap.map(|v| v.to_string()))
            .bind(bar.trade_count.map(|c| c as i64))
            .bind(expiry)
            .bind(strike)
            .bind(option_type)
            .execute(db)
            .await?;
        }

        Ok(())
    }

    pub async fn put_batch(&self, bars: Vec<Bar>) -> Result<()> {
        for bar in bars {
            self.put(bar).await?;
        }
        Ok(())
    }

    pub async fn get(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Bar>> {
        // Check memory first
        {
            let mem = self.memory.read().await;
            if let Some(cached) = mem.get(symbol) {
                let results: Vec<Bar> = cached.iter()
                    .filter(|c| c.bar.timeframe == timeframe && c.bar.timestamp >= start && c.bar.timestamp <= end)
                    .map(|c| c.bar.clone())
                    .collect();
                if !results.is_empty() {
                    debug!("Cache hit: {} bars for {}", results.len(), symbol);
                    return Ok(results);
                }
            }
        }

        // Fallback to DB
        if let Some(db) = &self.db {
            let (expiry, _strike, _option_type) = match symbol.asset_class {
                bt_core::types::AssetClass::Option => (
                    symbol.expiry.map(|d| d.timestamp()),
                    symbol.strike.map(|s| s.to_string()),
                    symbol.option_type.map(|t| t as u8 as i64),
                ),
                bt_core::types::AssetClass::Future => (
                    symbol.expiry.map(|d| d.timestamp()),
                    None,
                    None,
                ),
                _ => (None, None, None),
            };

            let query = sqlx::query(
                r#"
                SELECT timestamp, open, high, low, close, volume, vwap, trade_count
                FROM bars
                WHERE venue = ? AND ticker = ? AND asset_class = ? AND timeframe = ?
                AND timestamp >= ? AND timestamp <= ?
                ORDER BY timestamp DESC
                LIMIT ?
                "#,
            )
            .bind(symbol.venue as u8 as i64)
            .bind(&symbol.ticker)
            .bind(symbol.asset_class as u8 as i64)
            .bind(timeframe as u8 as i64)
            .bind(start.timestamp())
            .bind(end.timestamp())
            .bind(limit as i64);

            if let Some(_expiry) = expiry {
                // Not binding expiry since it's not in the select query where clause
            }

            let rows = query.fetch_all(db).await?;
            let mut bars = Vec::new();

            for row in rows.into_iter().rev() {
                bars.push(Bar {
                    symbol: symbol.clone(),
                    timeframe,
                    open: row.get::<String, _>(1).parse()?,
                    high: row.get::<String, _>(2).parse()?,
                    low: row.get::<String, _>(3).parse()?,
                    close: row.get::<String, _>(4).parse()?,
                    volume: row.get::<String, _>(5).parse()?,
                    vwap: row.try_get::<String, _>(6).ok().and_then(|v| v.parse().ok()),
                    trade_count: row.try_get::<i64, _>(7).ok().map(|v| v as u64),
                    timestamp: DateTime::from_timestamp(row.get::<i64, _>(0), 0).unwrap_or_else(Utc::now),
                    venue: symbol.venue,
                });
            }

            if !bars.is_empty() {
                info!("DB cache hit: {} bars for {}", bars.len(), symbol);
                // Populate memory cache
                let mut mem = self.memory.write().await;
                let entry = mem.entry(symbol.clone()).or_default();
                for bar in &bars {
                    entry.push(CachedBar {
                        bar: bar.clone(),
                        cached_at: Utc::now(),
                    });
                }
            }

            return Ok(bars);
        }

        Ok(Vec::new())
    }

    pub async fn get_latest(&self, symbol: &Symbol, timeframe: Timeframe) -> Result<Option<Bar>> {
        let bars = self.get(symbol, timeframe, Utc::now() - chrono::Duration::days(365), Utc::now(), 1).await?;
        Ok(bars.into_iter().next())
    }

    pub async fn clear_old(&self, before: DateTime<Utc>) -> Result<()> {
        if let Some(db) = &self.db {
            sqlx::query("DELETE FROM bars WHERE timestamp < ?")
                .bind(before.timestamp())
                .execute(db)
                .await?;
        }

        let mut mem = self.memory.write().await;
        for entry in mem.values_mut() {
            entry.retain(|c| c.cached_at > before);
        }
        mem.retain(|_, v| !v.is_empty());

        Ok(())
    }
}

pub struct QuoteCache {
    config: DataCacheConfig,
    memory: Arc<RwLock<HashMap<Symbol, CachedQuote>>>,
}

impl QuoteCache {
    pub fn new(config: DataCacheConfig) -> Self {
        Self {
            config,
            memory: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn put(&self, quote: Quote) -> Result<()> {
        let mut mem = self.memory.write().await;
        mem.insert(quote.symbol.clone(), CachedQuote {
            quote,
            cached_at: Utc::now(),
        });
        Ok(())
    }

    pub async fn get(&self, symbol: &Symbol) -> Result<Option<Quote>> {
        let mem = self.memory.read().await;
        if let Some(cached) = mem.get(symbol) {
            let age = Utc::now() - cached.cached_at;
            if age.num_seconds() < self.config.memory_ttl_seconds as i64 {
                return Ok(Some(cached.quote.clone()));
            }
        }
        Ok(None)
    }

    pub async fn get_multi(&self, symbols: &[Symbol]) -> Result<HashMap<Symbol, Quote>> {
        let mut results = HashMap::new();
        let mem = self.memory.read().await;
        for symbol in symbols {
            if let Some(cached) = mem.get(symbol) {
                let age = Utc::now() - cached.cached_at;
                if age.num_seconds() < self.config.memory_ttl_seconds as i64 {
                    results.insert(symbol.clone(), cached.quote.clone());
                }
            }
        }
        Ok(results)
    }

    pub async fn clear_stale(&self) {
        let mut mem = self.memory.write().await;
        let now = Utc::now();
        mem.retain(|_, v| (now - v.cached_at).num_seconds() < self.config.memory_ttl_seconds as i64);
    }
}
