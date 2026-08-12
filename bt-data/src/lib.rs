//! bt-data - Market data providers, caching, and normalization

pub mod cache;
pub mod crypto;
pub mod fpga_lob;
pub mod zero_alloc_queue;
pub mod manager;
pub mod mock;
pub mod normalization;
pub mod polygon;
pub mod provider;
pub mod orderbook_aggregator;
pub mod parquet_store;
pub mod nse_connector;
pub mod turbo_quant;
pub mod doc_parser;
pub mod vpin;
pub mod microstructure;

pub use cache::{BarCache, DataCacheConfig, QuoteCache};
pub use crypto::{BinanceProvider, BinanceConfig, CoinbaseProvider, CoinbaseConfig};
pub use manager::{DataFeedConfig, DataFeedManager, DataProviderConfig, DataProviderConfig as ProviderConfig, ProviderType};
pub use mock::MockProvider;
pub use normalization::{NormalizationConfig, SymbolNormalizer};
pub use polygon::{PolygonProvider, PolygonConfig};
pub use provider::{
    BarsRequest, Bar, Quote, Trade, OrderBook, NewsItem,
    ConnectionStatus, HealthStatus, MarketDataProvider, HistoricalDataProvider, NewsProvider,
    ProviderInfo, RateLimitInfo, Subscription,
};
pub use orderbook_aggregator::OrderBookAnalyzer;
pub use parquet_store::{ParquetStore, StoreStats};
pub use nse_connector::{NsePublicConnector, NseOptionChainSnapshot, OptionContractData};
pub use turbo_quant::{TurboQuantIndex, PatternRecord, PatternMatchResult};
pub use doc_parser::{DocumentType, DocumentSnippet, OcrModelEngine, UnlimitedOcrConfig, OcrDocumentParser};
