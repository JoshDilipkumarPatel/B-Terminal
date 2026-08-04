use crate::provider::{MarketDataProvider, HistoricalDataProvider, NewsProvider, Subscription, Symbol, BarsRequest, Bar, Quote, Trade, OrderBook, NewsItem, ConnectionStatus, HealthStatus, Timeframe};
use anyhow::Result;
use bt_core::events::MarketEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

pub struct DataFeedManager {
    providers: Arc<RwLock<HashMap<String, Box<dyn MarketDataProvider>>>>,
    historical_providers: Arc<RwLock<HashMap<String, Box<dyn HistoricalDataProvider>>>>,
    news_providers: Arc<RwLock<HashMap<String, Box<dyn NewsProvider>>>>,
    primary_provider: Arc<RwLock<Option<String>>>,
    fallback_order: Arc<RwLock<Vec<String>>>,
}

impl DataFeedManager {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            historical_providers: Arc::new(RwLock::new(HashMap::new())),
            news_providers: Arc::new(RwLock::new(HashMap::new())),
            primary_provider: Arc::new(RwLock::new(None)),
            fallback_order: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_provider(
        &self,
        name: String,
        provider: Box<dyn MarketDataProvider>,
    ) -> Result<()> {
        info!("Adding market data provider: {}", name);
        self.providers.write().await.insert(name.clone(), provider);
        if self.primary_provider.read().await.is_none() {
            *self.primary_provider.write().await = Some(name.clone());
        }
        self.fallback_order.write().await.push(name);
        Ok(())
    }

    pub async fn add_historical_provider(
        &self,
        name: String,
        provider: Box<dyn HistoricalDataProvider>,
    ) -> Result<()> {
        info!("Adding historical data provider: {}", name);
        self.historical_providers.write().await.insert(name, provider);
        Ok(())
    }

    pub async fn add_news_provider(
        &self,
        name: String,
        provider: Box<dyn NewsProvider>,
    ) -> Result<()> {
        info!("Adding news provider: {}", name);
        self.news_providers.write().await.insert(name, provider);
        Ok(())
    }

    pub async fn connect_all(&self) -> Result<()> {
        let mut providers = self.providers.write().await;
        for (name, provider) in providers.iter_mut() {
            if let Err(e) = provider.connect().await {
                error!("Failed to connect provider {}: {}", name, e);
            }
        }
        Ok(())
    }

    pub async fn disconnect_all(&self) -> Result<()> {
        let mut providers = self.providers.write().await;
        for (name, provider) in providers.iter_mut() {
            if let Err(e) = provider.disconnect().await {
                error!("Failed to disconnect provider {}: {}", name, e);
            }
        }
        Ok(())
    }

    pub async fn refresh_all(&self) -> Result<()> {
        Ok(())
    }

    pub async fn subscribe(&self, symbols: &[Symbol]) -> Result<Subscription> {
        let primary = self.primary_provider.read().await;
        if let Some(name) = primary.as_ref() {
            let providers = self.providers.read().await;
            if let Some(provider) = providers.get(name) {
                return provider.subscribe(symbols).await;
            }
        }

        // Try fallback providers
        let providers = self.providers.read().await;
        let fallback_order = self.fallback_order.read().await;
        for name in fallback_order.iter() {
            if let Some(provider) = providers.get(name) {
                if provider.connection_status() == ConnectionStatus::Connected {
                    return provider.subscribe(symbols).await;
                }
            }
        }

        anyhow::bail!("No available provider for subscription")
    }

    pub async fn get_bars(&self, request: BarsRequest) -> Result<Vec<Bar>> {
        let providers = self.historical_providers.read().await;
        let fallback_order = self.fallback_order.read().await;
        for name in fallback_order.iter() {
            if let Some(provider) = providers.get(name) {
                match provider.get_bars(request.clone()).await {
                    Ok(bars) if !bars.is_empty() => return Ok(bars),
                    Ok(_) => continue,
                    Err(e) => warn!("Provider {} failed: {}", name, e),
                }
            }
        }
        Ok(Vec::new())
    }

    pub async fn get_latest_bar(&self, symbol: &Symbol, timeframe: Timeframe) -> Result<Option<Bar>> {
        let providers = self.historical_providers.read().await;
        let fallback_order = self.fallback_order.read().await;
        for name in fallback_order.iter() {
            if let Some(provider) = providers.get(name) {
                if let Ok(Some(bar)) = provider.get_latest_bar(symbol, timeframe).await {
                    return Ok(Some(bar));
                }
            }
        }
        Ok(None)
    }

    pub async fn get_quotes(&self, symbols: &[Symbol]) -> Result<HashMap<Symbol, Quote>> {
        let providers = self.historical_providers.read().await;
        let fallback_order = self.fallback_order.read().await;
        for name in fallback_order.iter() {
            if let Some(provider) = providers.get(name) {
                if let Ok(quotes) = provider.get_quotes(symbols).await {
                    if !quotes.is_empty() {
                        return Ok(quotes);
                    }
                }
            }
        }
        Ok(HashMap::new())
    }

    pub async fn get_trades(&self, symbol: &Symbol, start: chrono::DateTime<chrono::Utc>, end: chrono::DateTime<chrono::Utc>, limit: usize) -> Result<Vec<Trade>> {
        let providers = self.historical_providers.read().await;
        let fallback_order = self.fallback_order.read().await;
        for name in fallback_order.iter() {
            if let Some(provider) = providers.get(name) {
                if let Ok(trades) = provider.get_trades(symbol, start, end, limit).await {
                    return Ok(trades);
                }
            }
        }
        Ok(Vec::new())
    }

    pub async fn get_order_book(&self, symbol: &Symbol, depth: usize) -> Result<Option<OrderBook>> {
        let providers = self.historical_providers.read().await;
        let fallback_order = self.fallback_order.read().await;
        for name in fallback_order.iter() {
            if let Some(provider) = providers.get(name) {
                if let Ok(Some(book)) = provider.get_order_book(symbol, depth).await {
                    return Ok(Some(book));
                }
            }
        }
        Ok(None)
    }

    pub async fn search_symbols(&self, query: &str) -> Result<Vec<Symbol>> {
        let providers = self.historical_providers.read().await;
        let fallback_order = self.fallback_order.read().await;
        for name in fallback_order.iter() {
            if let Some(provider) = providers.get(name) {
                if let Ok(symbols) = provider.search_symbols(query).await {
                    if !symbols.is_empty() {
                        return Ok(symbols);
                    }
                }
            }
        }
        Ok(Vec::new())
    }

    pub async fn get_news(&self, symbols: Option<&[Symbol]>, limit: usize) -> Result<Vec<NewsItem>> {
        let providers = self.news_providers.read().await;
        for name in providers.keys() {
            if let Some(provider) = providers.get(name) {
                if let Ok(news) = provider.get_news(symbols, limit).await {
                    return Ok(news);
                }
            }
        }
        Ok(Vec::new())
    }

    pub async fn health_check_all(&self) -> HashMap<String, HealthStatus> {
        let mut results = HashMap::new();
        let providers = self.providers.read().await;
        for (name, provider) in providers.iter() {
            if let Ok(status) = provider.health_check().await {
                results.insert(name.clone(), status);
            }
        }
        results
    }

    pub async fn set_primary(&self, name: &str) -> Result<()> {
        let providers = self.providers.read().await;
        if providers.contains_key(name) {
            *self.primary_provider.write().await = Some(name.to_string());
            info!("Primary provider set to: {}", name);
            Ok(())
        } else {
            anyhow::bail!("Provider not found: {}", name)
        }
    }

    pub async fn get_events(&self) -> tokio::sync::broadcast::Receiver<MarketEvent> {
        // Return merged events from all providers
        // For simplicity, return primary provider's events
        let primary = self.primary_provider.read().await;
        if let Some(name) = primary.as_ref() {
            let providers = self.providers.read().await;
            if let Some(provider) = providers.get(name) {
                return provider.events();
            }
        }
        // Return empty channel if no primary
        let (_tx, rx) = tokio::sync::broadcast::channel(100);
        rx
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFeedConfig {
    pub providers: Vec<DataProviderConfig>,
    pub primary: String,
    pub fallback_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataProviderConfig {
    pub name: String,
    pub type_: ProviderType,
    pub enabled: bool,
    pub priority: u32,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Polygon,
    Binance,
    Coinbase,
    Mock,
}

impl DataFeedManager {
    pub async fn from_config(config: DataFeedConfig) -> Result<Self> {
        let manager = Self::new();

        for provider_config in config.providers {
            if !provider_config.enabled {
                continue;
            }

            match provider_config.type_ {
                ProviderType::Polygon => {
                    let polygon_config: crate::polygon::PolygonConfig = serde_json::from_value(provider_config.config)?;
                    let provider = crate::polygon::PolygonProvider::new(polygon_config.clone());
                    manager.add_provider(provider_config.name.clone(), Box::new(provider)).await?;
                    manager.add_historical_provider(provider_config.name.clone(), Box::new(crate::polygon::PolygonProvider::new(polygon_config.clone()))).await?;
                    manager.add_news_provider(provider_config.name.clone(), Box::new(crate::polygon::PolygonProvider::new(polygon_config))).await?;
                }
                ProviderType::Binance => {
                    let binance_config: crate::crypto::BinanceConfig = serde_json::from_value(provider_config.config)?;
                    let provider = crate::crypto::BinanceProvider::new(binance_config.clone());
                    manager.add_provider(provider_config.name.clone(), Box::new(provider)).await?;
                    manager.add_historical_provider(provider_config.name.clone(), Box::new(crate::crypto::BinanceProvider::new(binance_config.clone()))).await?;
                }
                ProviderType::Coinbase => {
                    let coinbase_config: crate::crypto::CoinbaseConfig = serde_json::from_value(provider_config.config)?;
                    let provider = crate::crypto::CoinbaseProvider::new(coinbase_config.clone());
                    manager.add_provider(provider_config.name.clone(), Box::new(provider)).await?;
                    manager.add_historical_provider(provider_config.name.clone(), Box::new(crate::crypto::CoinbaseProvider::new(coinbase_config))).await?;
                }
                ProviderType::Mock => {
                    let provider = crate::mock::MockProvider::new();
                    manager.add_provider(provider_config.name.clone(), Box::new(provider)).await?;
                    manager.add_historical_provider(provider_config.name.clone(), Box::new(crate::mock::MockProvider::new())).await?;
                    manager.add_news_provider(provider_config.name.clone(), Box::new(crate::mock::MockProvider::new())).await?;
                }
            }
        }

        *manager.fallback_order.write().await = config.fallback_order;
        if !config.primary.is_empty() {
            manager.set_primary(&config.primary).await?;
        }

        Ok(manager)
    }
}
