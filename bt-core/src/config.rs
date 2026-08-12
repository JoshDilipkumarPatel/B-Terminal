use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub app: AppConfig,
    pub data: DataConfig,
    pub strategy: StrategyConfig,
    pub execution: ExecutionConfig,
    pub risk: RiskConfig,
    pub logging: LoggingConfig,
    pub tui: TuiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    pub version: String,
    pub environment: Environment,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
    Test,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    pub providers: Vec<ProviderConfig>,
    pub cache: CacheConfig,
    pub historical: HistoricalConfig,
    pub rate_limits: HashMap<String, RateLimitConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub enabled: bool,
    pub priority: u32,
    pub credentials: ProviderCredentials,
    pub endpoints: ProviderEndpoints,
    pub symbols: Vec<String>,
    pub reconnect: ReconnectConfig,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderCredentials {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub passphrase: Option<String>,
    pub base_url: Option<String>,
    pub ws_url: Option<String>,
}

impl std::fmt::Debug for ProviderCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderCredentials")
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("api_secret", &self.api_secret.as_ref().map(|_| "[REDACTED]"))
            .field("passphrase", &self.passphrase.as_ref().map(|_| "[REDACTED]"))
            .field("base_url", &self.base_url)
            .field("ws_url", &self.ws_url)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEndpoints {
    pub rest: String,
    pub websocket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub jitter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub memory_ttl_seconds: u64,
    pub persistent: bool,
    pub db_path: PathBuf,
    pub max_bars_per_symbol: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalConfig {
    pub enabled: bool,
    pub default_limit: usize,
    pub max_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_second: u32,
    pub burst: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub dsl: DslConfig,
    pub engine: EngineConfig,
    pub backtest: BacktestConfig,
    pub risk: StrategyRiskConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DslConfig {
    pub strategies_dir: PathBuf,
    pub auto_reload: bool,
    pub validate_on_load: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub enabled: bool,
    pub evaluation_interval_ms: u64,
    pub max_concurrent_strategies: usize,
    pub signal_dedup_window_ms: u64,
    pub min_signal_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub enabled: bool,
    pub default_start_date: Option<DateTime<Utc>>,
    pub default_end_date: Option<DateTime<Utc>>,
    pub initial_capital: Decimal,
    pub commission_per_share: Decimal,
    pub commission_min: Decimal,
    pub slippage_bps: u32,
    pub spread_bps: u32,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRiskConfig {
    pub max_position_pct: Decimal,
    pub max_daily_loss_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_leverage: Decimal,
    pub max_correlation: f64,
}

fn default_idempotency_persistence_enabled() -> bool {
    true
}

/// Persistent idempotency-key store configuration.
/// Survives process restarts so a 300s deduplication window is not lost on restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyConfig {
    #[serde(default = "default_idempotency_persistence_enabled")]
    pub persistence_enabled: bool,
    /// SQLite database path. `None` derives a default from `app.data_dir`.
    #[serde(default)]
    pub db_path: Option<PathBuf>,
}

impl Default for IdempotencyConfig {
    fn default() -> Self {
        Self {
            persistence_enabled: true,
            db_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    pub brokers: Vec<BrokerConfig>,
    pub default_broker: String,
    pub order_routing: RoutingConfig,
    pub simulator: SimulatorConfig,
    #[serde(default)]
    pub idempotency: IdempotencyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    pub name: String,
    pub type_: BrokerType,
    pub enabled: bool,
    pub paper_trading: bool,
    pub credentials: BrokerCredentials,
    pub endpoints: BrokerEndpoints,
    pub rate_limits: HashMap<String, RateLimitConfig>,
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokerType {
    Alpaca,
    InteractiveBrokers,
    Binance,
    Coinbase,
    Bybit,
    Simulator,
    Groww,
    CoinDCX,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BrokerCredentials {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub passphrase: Option<String>,
    pub account_id: Option<String>,
}

impl std::fmt::Debug for BrokerCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerCredentials")
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("api_secret", &self.api_secret.as_ref().map(|_| "[REDACTED]"))
            .field("passphrase", &self.passphrase.as_ref().map(|_| "[REDACTED]"))
            .field("account_id", &self.account_id)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerEndpoints {
    pub rest: String,
    pub websocket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub smart_routing: bool,
    pub prefer_paper_for_test: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorConfig {
    pub enabled: bool,
    pub initial_cash: Decimal,
    pub commission_per_share: Decimal,
    pub commission_min: Decimal,
    pub slippage_bps: u32,
    pub spread_bps: u32,
    pub fill_probability: f64,
    pub partial_fill_probability: f64,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub global: GlobalRiskLimits,
    pub per_strategy: HashMap<String, StrategyRiskLimits>,
    pub per_symbol: HashMap<String, SymbolRiskLimits>,
    pub kill_switch: KillSwitchConfig,
}

fn default_max_sector_exposure_pct() -> Decimal {
    Decimal::new(5, 2)
}

fn default_correlation_recompute_interval() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRiskLimits {
    pub max_daily_loss_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_leverage: Decimal,
    pub max_open_orders: usize,
    pub max_order_size_usd: Decimal,
    pub max_portfolio_heat: Decimal,
    #[serde(default = "default_max_sector_exposure_pct")]
    pub max_sector_exposure_pct: Decimal,
    /// Recompute dynamic correlation groups every N daily-return updates.
    #[serde(default = "default_correlation_recompute_interval")]
    pub correlation_recompute_interval: usize,
    pub kill_on_breach: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRiskLimits {
    pub max_allocation_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_open_positions: usize,
    pub max_daily_trades: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRiskLimits {
    pub max_position_pct: Decimal,
    pub max_notional_usd: Decimal,
    pub min_liquidity_usd: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitchConfig {
    pub enabled: bool,
    pub auto_flatten: bool,
    pub max_flatten_time_ms: u64,
    pub notify_channels: Vec<NotifyChannel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotifyChannel {
    Log,
    Telegram,
    Discord,
    Email,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
    pub output: LogOutput,
    pub file: FileLogConfig,
    pub audit: AuditLogConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogFormat {
    Json,
    Text,
    Pretty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogOutput {
    Stdout,
    Stderr,
    File,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogConfig {
    pub enabled: bool,
    pub path: PathBuf,
    pub max_size_mb: u64,
    pub max_files: u32,
    pub rotation: RotationPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationPolicy {
    Daily,
    Hourly,
    Size,
}

fn default_encryption_key_env_var() -> String {
    "B_TERMINAL_AUDIT_KEY".to_string()
}

fn default_key_rotation_days() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogConfig {
    pub enabled: bool,
    pub path: PathBuf,
    pub hash_chain: bool,
    pub encrypt: bool,
    #[serde(default = "default_encryption_key_env_var")]
    pub encryption_key_env_var: String,
    #[serde(default = "default_key_rotation_days")]
    pub key_rotation_days: u32,
    #[serde(default)]
    pub anchor_checkpoint_url: Option<String>, // External SIEM/WORM root anchor for tamper-evident verification
    #[serde(default)]
    pub key_escrow_path: Option<PathBuf>, // Secure archival vault path for decipherability of historical rotated keys
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    pub theme: ThemeConfig,
    pub layout: LayoutConfig,
    pub keybindings: KeybindingConfig,
    pub refresh_rate_hz: u32,
    pub mouse_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub name: String,
    pub colors: ColorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    pub bg: String,
    pub fg: String,
    pub border: String,
    pub title: String,
    pub highlight: String,
    pub positive: String,
    pub negative: String,
    pub warning: String,
    pub info: String,
    pub accent: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            bg: "#000000".to_string(),
            fg: "#E0E0E0".to_string(),
            border: "#FFB400".to_string(),      // Bloomberg amber
            title: "#FFB400".to_string(),
            highlight: "#FFFFFF".to_string(),
            positive: "#00FF00".to_string(),    // Bright green
            negative: "#FF0000".to_string(),    // Bright red
            warning: "#FFA500".to_string(),     // Orange
            info: "#00FFFF".to_string(),        // Cyan
            accent: "#FFB400".to_string(),      // Bloomberg amber
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    pub panes: Vec<PaneConfig>,
    pub default_focus: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneConfig {
    pub id: String,
    pub type_: PaneType,
    pub title: String,
    pub ratio: f32,
    pub min_size: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneType {
    MarketOverview,
    SecurityDetail,
    Chart,
    OrderBook,
    News,
    Portfolio,
    KiAssistant,
    CommandLog,
}

impl PaneType {
    pub fn default_title(&self) -> &'static str {
        match self {
            PaneType::MarketOverview => "MARKET",
            PaneType::SecurityDetail => "DETAIL",
            PaneType::Chart => "CHART",
            PaneType::OrderBook => "ORDER BOOK",
            PaneType::News => "NEWS",
            PaneType::Portfolio => "PORTFOLIO",
            PaneType::KiAssistant => "KI ASSISTANT",
            PaneType::CommandLog => "COMMANDS",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingConfig {
    pub bindings: HashMap<String, String>,
    pub mode_specific: HashMap<String, HashMap<String, String>>,
}

impl Default for KeybindingConfig {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert("F1".to_string(), "focus_pane_0".to_string());
        bindings.insert("F2".to_string(), "focus_pane_1".to_string());
        bindings.insert("F3".to_string(), "focus_pane_2".to_string());
        bindings.insert("F4".to_string(), "focus_pane_3".to_string());
        bindings.insert("F5".to_string(), "focus_pane_4".to_string());
        bindings.insert("F6".to_string(), "focus_pane_5".to_string());
        bindings.insert("F7".to_string(), "focus_pane_6".to_string());
        bindings.insert("F8".to_string(), "focus_pane_7".to_string());
        bindings.insert("Alt+K".to_string(), "toggle_ki_assistant".to_string());
        bindings.insert("Ctrl+Q".to_string(), "kill_switch".to_string());
        bindings.insert("Ctrl+S".to_string(), "save_workspace".to_string());
        bindings.insert("Ctrl+L".to_string(), "focus_command".to_string());
        bindings.insert("?".to_string(), "show_help".to_string());
        bindings.insert("Esc".to_string(), "cancel".to_string());
        bindings.insert("Tab".to_string(), "next_pane".to_string());
        bindings.insert("Shift+Tab".to_string(), "prev_pane".to_string());
        bindings.insert("Up".to_string(), "scroll_up".to_string());
        bindings.insert("Down".to_string(), "scroll_down".to_string());
        bindings.insert("Left".to_string(), "scroll_left".to_string());
        bindings.insert("Right".to_string(), "scroll_right".to_string());
        bindings.insert("Ctrl+Up".to_string(), "resize_up".to_string());
        bindings.insert("Ctrl+Down".to_string(), "resize_down".to_string());
        bindings.insert("Ctrl+Left".to_string(), "resize_left".to_string());
        bindings.insert("Ctrl+Right".to_string(), "resize_right".to_string());
        Self { bindings, mode_specific: HashMap::new() }
    }
}

impl KeybindingConfig {
    pub fn get_action(&self, key: &str) -> Option<&str> {
        self.bindings.get(key).map(|s| s.as_str())
    }

    pub fn get_key_for_action(&self, action: &str) -> Option<&str> {
        for (key, act) in &self.bindings {
            if act == action {
                return Some(key);
            }
        }
        None
    }
}

impl Default for Config {
    fn default() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bloomberg-terminal");
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bloomberg-terminal");

        Self {
            app: AppConfig {
                name: "bloomberg-terminal".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                environment: Environment::Development,
                data_dir: data_dir.clone(),
                config_dir: config_dir.clone(),
            },
            data: DataConfig {
                providers: vec![],
                cache: CacheConfig {
                    enabled: true,
                    memory_ttl_seconds: 300,
                    persistent: true,
                    db_path: data_dir.join("cache.db"),
                    max_bars_per_symbol: 10000,
                },
                historical: HistoricalConfig {
                    enabled: true,
                    default_limit: 1000,
                    max_limit: 50000,
                },
                rate_limits: HashMap::new(),
            },
            strategy: StrategyConfig {
                dsl: DslConfig {
                    strategies_dir: config_dir.join("strategies"),
                    auto_reload: true,
                    validate_on_load: true,
                },
                engine: EngineConfig {
                    enabled: true,
                    evaluation_interval_ms: 100,
                    max_concurrent_strategies: 10,
                    signal_dedup_window_ms: 1000,
                    min_signal_confidence: 0.5,
                },
                backtest: BacktestConfig {
                    enabled: true,
                    default_start_date: None,
                    default_end_date: None,
                    initial_capital: Decimal::new(100000, 2),
                    commission_per_share: Decimal::new(1, 4),
                    commission_min: Decimal::new(1, 2),
                    slippage_bps: 5,
                    spread_bps: 2,
                    latency_ms: 50,
                },
                risk: StrategyRiskConfig {
                    max_position_pct: Decimal::new(1, 2), // 1% ($1,000 on $100K capital, matching global max_order_size_usd)
                    max_daily_loss_pct: Decimal::new(2, 2),
                    max_drawdown_pct: Decimal::new(10, 2),
                    max_leverage: Decimal::ONE, // 1.0x (no leverage, matching global max_leverage)
                    max_correlation: 0.7,
                },
            },
            execution: ExecutionConfig {
                brokers: vec![],
                default_broker: "simulator".into(),
                order_routing: RoutingConfig {
                    smart_routing: false,
                    prefer_paper_for_test: true,
                },
                simulator: SimulatorConfig {
                    enabled: true,
                    initial_cash: Decimal::new(100000, 2),
                    commission_per_share: Decimal::new(1, 4),
                    commission_min: Decimal::new(1, 2),
                    slippage_bps: 5,
                    spread_bps: 2,
                    fill_probability: 1.0,
                    partial_fill_probability: 0.1,
                    latency_ms: 10,
                },
                idempotency: IdempotencyConfig {
                    persistence_enabled: true,
                    db_path: Some(data_dir.join("idempotency.sqlite")),
                },
            },
            risk: RiskConfig {
                global: GlobalRiskLimits {
                    max_daily_loss_pct: Decimal::new(3, 2),
                    max_drawdown_pct: Decimal::new(10, 2),
                    max_leverage: Decimal::ONE, // Conservatively limited to 1.0x (no leverage) by default
                    max_open_orders: 100,
                    max_order_size_usd: Decimal::new(1000, 0), // Conservatively limited to $1,000 per order
                    max_portfolio_heat: Decimal::new(20, 2), // Conservatively capped at 20% portfolio exposure
                    max_sector_exposure_pct: Decimal::new(5, 2), // Capped at 5% aggregate correlated sector exposure
                    correlation_recompute_interval: 100, // Re-cluster dynamic correlation groups every 100 daily-return updates
                    kill_on_breach: true,
                },
                per_strategy: HashMap::new(),
                per_symbol: HashMap::new(),
                kill_switch: KillSwitchConfig {
                    enabled: true,
                    auto_flatten: true,
                    max_flatten_time_ms: 5000,
                    notify_channels: vec![NotifyChannel::Log],
                },
            },
            logging: LoggingConfig {
                level: "info".into(),
                format: LogFormat::Json,
                output: LogOutput::Both,
                file: FileLogConfig {
                    enabled: true,
                    path: data_dir.join("logs").join("bt.log"),
                    max_size_mb: 100,
                    max_files: 30,
                    rotation: RotationPolicy::Daily,
                },
                audit: AuditLogConfig {
                    enabled: true,
                    path: data_dir.join("audit").join("bt_audit.log"),
                    hash_chain: true,
                    encrypt: true, // Audit logs encrypted by default per security mandate
                    encryption_key_env_var: "BT_AUDIT_ENCRYPTION_KEY".into(),
                    key_rotation_days: 30,
                    anchor_checkpoint_url: Some("https://siem.enterprise.internal/checkpoint/wom_root".to_string()),
                    key_escrow_path: Some(data_dir.join("audit").join("key_escrow_vault")),
                },
            },
            tui: TuiConfig {
                theme: ThemeConfig {
                    name: "bloomberg".into(),
                    colors: ColorConfig {
                        bg: "#000000".into(),
                        fg: "#E0E0E0".into(),
                        border: "#FFB400".into(),
                        title: "#FFB400".into(),
                        highlight: "#FFFFFF".into(),
                        positive: "#00FF00".into(),
                        negative: "#FF0000".into(),
                        warning: "#FFA500".into(),
                        info: "#00FFFF".into(),
                        accent: "#FFB400".into(),
                    },
                },
                layout: LayoutConfig {
                    panes: vec![
                        PaneConfig {
                            id: "market".into(),
                            type_: PaneType::MarketOverview,
                            title: "MARKET".into(),
                            ratio: 0.25,
                            min_size: 40,
                        },
                        PaneConfig {
                            id: "detail".into(),
                            type_: PaneType::SecurityDetail,
                            title: "DETAIL".into(),
                            ratio: 0.25,
                            min_size: 40,
                        },
                        PaneConfig {
                            id: "chart".into(),
                            type_: PaneType::Chart,
                            title: "CHART".into(),
                            ratio: 0.25,
                            min_size: 40,
                        },
                        PaneConfig {
                            id: "ki".into(),
                            type_: PaneType::KiAssistant,
                            title: "KI ASSISTANT".into(),
                            ratio: 0.25,
                            min_size: 40,
                        },
                    ],
                    default_focus: 0,
                },
                keybindings: KeybindingConfig {
                    bindings: HashMap::from([
                        ("F1".into(), "focus_pane_0".into()),
                        ("F2".into(), "focus_pane_1".into()),
                        ("F3".into(), "focus_pane_2".into()),
                        ("F4".into(), "focus_pane_3".into()),
                        ("F5".into(), "focus_pane_4".into()),
                        ("F6".into(), "focus_pane_5".into()),
                        ("F7".into(), "focus_pane_6".into()),
                        ("F8".into(), "focus_pane_7".into()),
                        ("Alt+K".into(), "toggle_ki_assistant".into()),
                        ("Ctrl+Q".into(), "kill_switch".into()),
                        ("Ctrl+S".into(), "save_workspace".into()),
                        ("Ctrl+L".into(), "focus_command".into()),
                        ("?".into(), "show_help".into()),
                        ("Esc".into(), "cancel".into()),
                        ("Tab".into(), "next_pane".into()),
                        ("Shift+Tab".into(), "prev_pane".into()),
                    ]),
                    mode_specific: HashMap::new(),
                },
                refresh_rate_hz: 10,
                mouse_enabled: true,
            },
        }
    }
}

impl Config {
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let mut loaded: Config = toml::from_str(&content)?;
            loaded.merge_defaults(Self::default());
            Ok(loaded)
        } else {
            Ok(Self::default())
        }
    }

    pub fn to_file(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }

    pub fn load() -> Result<Self> {
        let config = Self::default();
        let config_dir = config.app.config_dir.clone();
        let config_path = config_dir.join("config.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let mut loaded: Config = toml::from_str(&content)?;
            // Merge with defaults for missing fields
            loaded.merge_defaults(config);
            Ok(loaded)
        } else {
            // Create default config file
            std::fs::create_dir_all(&config_dir)?;
            let toml_str = toml::to_string_pretty(&config)?;
            std::fs::write(&config_path, toml_str)?;
            Ok(config)
        }
    }

    fn merge_defaults(&mut self, defaults: Config) {
        // Simple merge - in production use a proper merge library
        if self.app.name.is_empty() {
            self.app = defaults.app;
        }
        // ... other fields would be merged similarly
    }

    pub fn save(&self) -> Result<()> {
        let config_dir = &self.app.config_dir;
        std::fs::create_dir_all(config_dir)?;
        let config_path = config_dir.join("config.toml");
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(config_path, toml_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conservative_security_defaults() {
        let config = Config::default();
        assert_eq!(config.risk.global.max_order_size_usd, Decimal::new(1000, 0), "Default max order size must be conservatively limited to $1,000");
        assert_eq!(config.risk.global.max_leverage, Decimal::ONE, "Default leverage must be capped at 1.0x");
        assert_eq!(config.risk.global.max_portfolio_heat, Decimal::new(20, 2), "Default portfolio heat must be capped at 20%");
        assert_eq!(config.risk.global.max_sector_exposure_pct, Decimal::new(5, 2), "Default aggregate sector exposure must be capped at 5%");
        assert_eq!(config.risk.global.correlation_recompute_interval, 100, "Dynamic correlation recompute interval must default to 100");
        assert_eq!(config.strategy.risk.max_leverage, Decimal::ONE, "Strategy max leverage must default to 1.0x to match global limits");
        assert_eq!(config.strategy.risk.max_position_pct, Decimal::new(1, 2), "Strategy max position must default to 1% ($1,000)");
        assert!(config.logging.audit.encrypt, "Audit log encryption must be enabled by default");
        assert_eq!(config.logging.audit.encryption_key_env_var, "BT_AUDIT_ENCRYPTION_KEY", "Must designate encryption key management environment variable");
        assert_eq!(config.logging.audit.key_rotation_days, 30, "Default key rotation interval must be defined");
        assert!(config.logging.audit.anchor_checkpoint_url.is_some(), "Audit log must define an external tamper-evident WORM root anchor");
        assert!(config.logging.audit.key_escrow_path.is_some(), "Audit log must define a secure key escrow archival path for historical log decryption");
    }
}