use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    Terminal,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use crossterm::{
    event::{self, Event, KeyEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
    sync::Arc,
};
use tokio::sync::{broadcast, mpsc};
use anyhow::Result;

use bt_core::{config::*, events::*, types::*};
use bt_data::DataFeedManager;
use bt_strategy::{SignalEngine, StrategyCompiler, BacktestEngine};
use bt_execution::OrderManagementSystem;
use bt_core::risk_limits::RiskManager;
use bt_core::kill_switch::GlobalKillSwitch;

use crate::theme::Theme;
use crate::layout::{LayoutManager, PaneType, FocusablePane};
use crate::widgets::{
    MarketOverviewWidget, SecurityDetailWidget, ChartWidget,
    OrderBookWidget, NewsWidget, PortfolioWidget, KiAssistantWidget,
    DeployStatus, StrategyDeployStatus, DeployableStrategy, LogLevel,
};
use crate::command::{CommandParser, ParsedCommand};

#[allow(dead_code)]
pub struct App {
    config: Config,
    theme: Theme,
    layout_manager: LayoutManager,
    terminal: Terminal<CrosstermBackend<Stdout>>,

    // Widgets
    market_overview: MarketOverviewWidget,
    security_detail: SecurityDetailWidget,
    chart: ChartWidget,
    order_book: OrderBookWidget,
    news: NewsWidget,
    portfolio: PortfolioWidget,
    ki_assistant: KiAssistantWidget,

    // Core systems
    data_manager: Option<DataFeedManager>,
    signal_engine: Option<SignalEngine>,
    oms: Option<OrderManagementSystem>,
    risk_manager: Option<RiskManager>,
    kill_switch: Arc<GlobalKillSwitch>,

    // Communication channels
    event_tx: broadcast::Sender<SystemEvent>,
    event_rx: broadcast::Receiver<SystemEvent>,
    command_tx: mpsc::Sender<AppCommand>,
    command_rx: mpsc::Receiver<AppCommand>,

    // State
    running: bool,
    focused_pane: FocusablePane,
    command_mode: bool,
    command_buffer: String,
    command_history: Vec<String>,
    command_history_index: usize,
    status_message: Option<(String, Instant)>,
    last_render: Instant,
    target_fps: u64,
}

#[derive(Debug, Clone)]
pub enum SystemEvent {
    MarketEvent(MarketEvent),
    SignalEvent(SignalEvent),
    ExecutionEvent(ExecutionEvent),
    RiskEvent(RiskEvent),
    ConnectionStatus(ConnectionStatus),
    LogEvent(LogEvent),
    KillSwitch,
}

#[derive(Debug, Clone)]
pub enum LogEvent {
    Info(String),
    Warn(String),
    Error(String),
    Debug(String),
}

#[derive(Debug)]
pub enum AppCommand {
    Quit,
    FocusPane(FocusablePane),
    ToggleCommandMode,
    SubmitCommand(String),
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    PageUp,
    PageDown,
    Home,
    End,
    RefreshData,
    ToggleFullscreen(PaneType),
    SwitchTab(usize),
    ExecuteTrade(TradeCommand),
    KillSwitch,
    FlattenPositions,
    RunBacktest(String),
    DeployStrategy(String),
    StopStrategy(String),
}

#[derive(Debug, Clone)]
pub struct TradeCommand {
    pub symbol: Symbol,
    pub side: Side,
    pub quantity: Decimal,
    pub order_type: OrderType,
    pub limit_price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub time_in_force: TimeInForce,
}

impl App {
    pub async fn new(config: Config) -> Result<Self> {
        let theme = Theme::from_config(&config.tui.theme);
        let layout_manager = LayoutManager::new(&config.tui.layout);

        // Initialize terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        // Create channels
        let (event_tx, event_rx) = broadcast::channel(1000);
        let (command_tx, command_rx) = mpsc::channel(100);

        // Initialize widgets
        let market_overview = MarketOverviewWidget::new(theme.clone());
        let security_detail = SecurityDetailWidget::new(theme.clone());
        let chart = ChartWidget::new(theme.clone());
        let order_book = OrderBookWidget::new(theme.clone());
        let news = NewsWidget::new(theme.clone());
        let portfolio = PortfolioWidget::new(theme.clone());
        let ki_assistant = KiAssistantWidget::new(theme.clone());

        // Initialize core systems
        let mut risk_limits = bt_core::risk_limits::RiskLimits::default();
        risk_limits.global.correlation_recompute_interval = config.risk.global.correlation_recompute_interval;
        let risk_manager = RiskManager::new(risk_limits);
        let kill_switch = Arc::new(GlobalKillSwitch::new(tokio::sync::broadcast::channel(100).0, 1000));
        let data_manager = DataFeedManager::new();
        let signal_engine = SignalEngine::new(bt_strategy::EngineConfig::default(), tokio::sync::broadcast::channel(100).0);
        let oms = OrderManagementSystem::new(config.execution.clone());

        Ok(Self {
            config,
            theme,
            layout_manager,
            terminal,
            market_overview,
            security_detail,
            chart,
            order_book,
            news,
            portfolio,
            ki_assistant,
            data_manager: Some(data_manager),
            signal_engine: Some(signal_engine),
            oms: Some(oms),
            risk_manager: Some(risk_manager),
            kill_switch,
            event_tx,
            event_rx,
            command_tx,
            command_rx,
            running: true,
            focused_pane: FocusablePane::MarketOverview,
            command_mode: false,
            command_buffer: String::new(),
            command_history: Vec::new(),
            command_history_index: 0,
            status_message: None,
            last_render: Instant::now(),
            target_fps: 30,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        // Spawn event processing task
        let _event_tx = self.event_tx.clone();
        let mut event_rx = self.event_rx.resubscribe();
        let command_tx = self.command_tx.clone();

        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                // Process system events
                match event {
                    SystemEvent::MarketEvent(_market_event) => {
                        // Forward to widgets
                    }
                    SystemEvent::SignalEvent(_signal_event) => {
                        // Forward to Ki Assistant
                    }
                    SystemEvent::ExecutionEvent(_exec_event) => {
                        // Update portfolio, positions
                    }
                    SystemEvent::KillSwitch => {
                        command_tx.send(AppCommand::KillSwitch).await.ok();
                    }
                    _ => {}
                }
            }
        });

        // Systems run via event channel reactions

        // Main event loop
        let tick_rate = Duration::from_millis(1000 / self.target_fps);

        while self.running {
            let timeout = tick_rate.saturating_sub(self.last_render.elapsed());

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key_event(key).await?;
                }
            }

            // Process app commands
            while let Ok(cmd) = self.command_rx.try_recv() {
                self.handle_command(cmd).await?;
            }

            // Process system events
            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_system_event(event).await?;
            }

            // Render
            if self.last_render.elapsed() >= tick_rate {
                self.render()?;
                self.last_render = Instant::now();
            }
        }

        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        if self.command_mode {
            return self.handle_command_mode_key(key).await;
        }

        // Global keybindings
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.command_tx.send(AppCommand::Quit).await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
                self.command_tx.send(AppCommand::KillSwitch).await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
                self.command_tx.send(AppCommand::FlattenPositions).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char(':')) => {
                self.command_tx.send(AppCommand::ToggleCommandMode).await?;
            }
            (KeyModifiers::NONE, KeyCode::Tab) => {
                self.focus_next_pane();
            }
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.focus_prev_pane();
            }
            (KeyModifiers::NONE, KeyCode::F(1)) => self.command_tx.send(AppCommand::FocusPane(FocusablePane::MarketOverview)).await?,
            (KeyModifiers::NONE, KeyCode::F(2)) => self.command_tx.send(AppCommand::FocusPane(FocusablePane::SecurityDetail)).await?,
            (KeyModifiers::NONE, KeyCode::F(3)) => self.command_tx.send(AppCommand::FocusPane(FocusablePane::Chart)).await?,
            (KeyModifiers::NONE, KeyCode::F(4)) => self.command_tx.send(AppCommand::FocusPane(FocusablePane::OrderBook)).await?,
            (KeyModifiers::NONE, KeyCode::F(5)) => self.command_tx.send(AppCommand::FocusPane(FocusablePane::News)).await?,
            (KeyModifiers::NONE, KeyCode::F(6)) => self.command_tx.send(AppCommand::FocusPane(FocusablePane::Portfolio)).await?,
            (KeyModifiers::NONE, KeyCode::F(7)) => self.command_tx.send(AppCommand::FocusPane(FocusablePane::KiAssistant)).await?,
            _ => {
                // Forward to focused pane
                self.forward_key_to_focused_pane(key).await?;
            }
        }

        Ok(())
    }

    async fn handle_command_mode_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.command_mode = false;
                self.command_buffer.clear();
            }
            KeyCode::Enter => {
                if !self.command_buffer.trim().is_empty() {
                    let cmd = self.command_buffer.trim().to_string();
                    self.command_history.push(cmd.clone());
                    self.command_history_index = self.command_history.len();
                    self.command_tx.send(AppCommand::SubmitCommand(cmd)).await?;
                }
                self.command_mode = false;
                self.command_buffer.clear();
            }
            KeyCode::Backspace => {
                self.command_buffer.pop();
            }
            KeyCode::Up => {
                if self.command_history_index > 0 {
                    self.command_history_index -= 1;
                    self.command_buffer = self.command_history[self.command_history_index].clone();
                }
            }
            KeyCode::Down => {
                if self.command_history_index < self.command_history.len() - 1 {
                    self.command_history_index += 1;
                    self.command_buffer = self.command_history[self.command_history_index].clone();
                } else {
                    self.command_history_index = self.command_history.len();
                    self.command_buffer.clear();
                }
            }
            KeyCode::Tab => {
                // Auto-complete
                self.auto_complete_command();
            }
            KeyCode::Char(c) => {
                self.command_buffer.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn auto_complete_command(&mut self) {
        let commands = vec![
            "help", "quit", "refresh", "symbol", "chart", "news", "portfolio",
            "ki", "backtest", "deploy", "stop", "kill", "flatten",
            "buy", "sell", "order", "cancel", "positions", "account",
            "theme", "layout", "save", "load", "focus",
        ];

        let input = self.command_buffer.trim();
        if let Some(completion) = commands.iter().find(|c| c.starts_with(input)) {
            self.command_buffer = completion.to_string();
        }
    }

    async fn forward_key_to_focused_pane(&mut self, key: KeyEvent) -> Result<()> {
        match self.focused_pane {
            FocusablePane::MarketOverview => {
                match key.code {
                    KeyCode::Up => self.market_overview.scroll_up(),
                    KeyCode::Down => self.market_overview.scroll_down(),
                    KeyCode::Enter => {
                        if let Some(symbol) = self.market_overview.selected_symbol() {
                            self.select_symbol(symbol.clone()).await?;
                        }
                    }
                    _ => {}
                }
            }
            FocusablePane::SecurityDetail => {
                match key.code {
                    KeyCode::Up => self.security_detail.scroll_up(),
                    KeyCode::Down => self.security_detail.scroll_down(),
                    _ => {}
                }
            }
            FocusablePane::Chart => {
                match key.code {
                    KeyCode::Left => self.chart.scroll_left(),
                    KeyCode::Right => self.chart.scroll_right(),
                    KeyCode::Up => self.chart.zoom_in(),
                    KeyCode::Down => self.chart.zoom_out(),
                    KeyCode::Char('v') => self.chart.toggle_volume(),
                    KeyCode::Char('i') => self.chart.toggle_indicators(),
                    _ => {}
                }
            }
            FocusablePane::OrderBook => {
                match key.code {
                    KeyCode::Up => self.order_book.scroll_up(),
                    KeyCode::Down => self.order_book.scroll_down(),
                    KeyCode::Char('+') => self.order_book.increase_depth(),
                    KeyCode::Char('-') => self.order_book.decrease_depth(),
                    _ => {}
                }
            }
            FocusablePane::News => {
                match key.code {
                    KeyCode::Up => self.news.scroll_up(),
                    KeyCode::Down => self.news.scroll_down(),
                    KeyCode::Enter => {
                        // Show detail view
                    }
                    KeyCode::Char('/') => {
                        // Start filter
                    }
                    _ => {}
                }
            }
            FocusablePane::Portfolio => {
                match key.code {
                    KeyCode::Up => self.portfolio.scroll_up(),
                    KeyCode::Down => self.portfolio.scroll_down(),
                    KeyCode::Enter => {
                        if let Some(pos) = self.portfolio.selected_position() {
                            self.select_symbol(pos.symbol.clone()).await?;
                        }
                    }
                    _ => {}
                }
            }
            FocusablePane::KiAssistant => {
                match key.code {
                    KeyCode::Left => self.ki_assistant.prev_tab(),
                    KeyCode::Right => self.ki_assistant.next_tab(),
                    KeyCode::Up => self.ki_assistant.scroll_up(),
                    KeyCode::Down => self.ki_assistant.scroll_down(),
                    KeyCode::Enter => self.ki_assistant.handle_enter().await?,
                    KeyCode::Char('v') => self.ki_assistant.validate_strategy().await?,
                    KeyCode::Char('b') => self.ki_assistant.run_backtest().await?,
                    KeyCode::Char('d') => self.ki_assistant.deploy_strategy().await?,
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn auto_deploy(&self, strategy_name: String) -> Result<()> {
        self.command_tx.send(AppCommand::DeployStrategy(strategy_name)).await.map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    async fn handle_command(&mut self, cmd: AppCommand) -> Result<()> {
        match cmd {
            AppCommand::Quit => {
                self.running = false;
            }
            AppCommand::FocusPane(pane) => {
                self.focused_pane = pane;
                self.layout_manager.set_focus(pane);
            }
            AppCommand::ToggleCommandMode => {
                self.command_mode = true;
                self.command_buffer.clear();
            }
            AppCommand::SubmitCommand(cmd_str) => {
                self.execute_command(&cmd_str).await?;
            }
            AppCommand::ScrollUp => {
                self.forward_key_to_focused_pane(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)).await?;
            }
            AppCommand::ScrollDown => {
                self.forward_key_to_focused_pane(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).await?;
            }
            AppCommand::KillSwitch => {
                self.kill_switch.activate(bt_core::KillReason::Manual).await?;
                self.set_status("KILL SWITCH ACTIVATED", true);
            }
            AppCommand::FlattenPositions => {
                if let Some(oms) = &mut self.oms {
                    oms.flatten_all_positions().await?;
                    self.set_status("Flattening all positions...", false);
                }
            }
            AppCommand::RunBacktest(strategy_name) => {
                self.run_backtest(&strategy_name).await?;
            }
            AppCommand::DeployStrategy(strategy_name) => {
                self.deploy_strategy(&strategy_name).await?;
            }
            AppCommand::StopStrategy(strategy_name) => {
                self.stop_strategy(&strategy_name).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn execute_command(&mut self, cmd: &str) -> Result<()> {
        let parser = CommandParser::new();
        match parser.parse(cmd) {
            Ok(parsed) => {
                match parsed {
                    ParsedCommand::Help => {
                        self.show_help();
                    }
                    ParsedCommand::Quit => {
                        self.command_tx.send(AppCommand::Quit).await?;
                    }
                    ParsedCommand::Refresh => {
                        self.refresh_data().await?;
                    }
                    ParsedCommand::Symbol(symbol) => {
                        self.select_symbol(symbol).await?;
                    }
                    ParsedCommand::Chart(timeframe) => {
                        self.chart.set_timeframe(&timeframe);
                    }
                    ParsedCommand::News(filter) => {
                        self.news.set_filter_keyword(filter);
                    }
                    ParsedCommand::Portfolio => {
                        self.command_tx.send(AppCommand::FocusPane(FocusablePane::Portfolio)).await?;
                    }
                    ParsedCommand::KiAssistant(mode) => {
                        self.command_tx.send(AppCommand::FocusPane(FocusablePane::KiAssistant)).await?;
                        self.ki_assistant.set_mode(mode);
                    }
                    ParsedCommand::Backtest(strategy) => {
                        self.command_tx.send(AppCommand::RunBacktest(strategy)).await?;
                    }
                    ParsedCommand::Deploy(strategy) => {
                        self.command_tx.send(AppCommand::DeployStrategy(strategy)).await?;
                    }
                    ParsedCommand::Stop(strategy) => {
                        self.command_tx.send(AppCommand::StopStrategy(strategy)).await?;
                    }
                    ParsedCommand::Kill => {
                        self.command_tx.send(AppCommand::KillSwitch).await?;
                    }
                    ParsedCommand::Flatten => {
                        self.command_tx.send(AppCommand::FlattenPositions).await?;
                    }
                    ParsedCommand::Buy { symbol, qty, price } => {
                        self.place_order(symbol, Side::Buy, qty, price).await?;
                    }
                    ParsedCommand::Sell { symbol, qty, price } => {
                        self.place_order(symbol, Side::Sell, qty, price).await?;
                    }
                    ParsedCommand::Cancel(order_id) => {
                        if let (Some(oms), Ok(uid)) = (&mut self.oms, uuid::Uuid::parse_str(&order_id)) {
                            oms.cancel_order(uid).await?;
                        }
                    }
                    ParsedCommand::Positions => {
                        self.show_positions();
                    }
                    ParsedCommand::Account => {
                        self.show_account();
                    }
                    ParsedCommand::Theme(theme_name) => {
                        self.theme = Theme::from_name(&theme_name);
                        self.set_status(&format!("Theme changed to {}", theme_name), false);
                    }
                    ParsedCommand::Layout(layout_name) => {
                        let _ = self.layout_manager.load_workspace(&layout_name);
                        self.set_status(&format!("Layout changed to {}", layout_name), false);
                    }
                }
            }
            Err(e) => {
                self.set_status(&format!("Command error: {}", e), true);
            }
        }
        Ok(())
    }

    async fn handle_system_event(&mut self, event: SystemEvent) -> Result<()> {
        match event {
            SystemEvent::MarketEvent(market_event) => {
                match market_event {
                    MarketEvent::Quote(quote) => {
                        self.market_overview.update_quote(quote.clone());
                        self.security_detail.update_quote(quote.clone());
                        self.chart.update_quote(quote.clone());
                        self.order_book.update_quote(quote);
                    }
                    MarketEvent::Trade(trade) => {
                        self.market_overview.update_trade(trade.clone());
                        self.security_detail.update_trade(trade.clone());
                        self.chart.update_trade(trade);
                    }
                    MarketEvent::Bar(bar) => {
                        self.chart.add_bar(bar.clone());
                        self.security_detail.update_bar(bar);
                    }
                    MarketEvent::OrderBook(book) => {
                        self.order_book.update_order_book(book.clone());
                        self.security_detail.update_order_book(book);
                    }
                    MarketEvent::News(news_item) => {
                        self.news.add_news(news_item);
                    }
                    _ => {}
                }
            }
            SystemEvent::SignalEvent(signal) => {
                self.ki_assistant.add_signal(&signal);
                // Auto-execute if in deploy mode
                if self.ki_assistant.deploy_panel.status == DeployStatus::Running {
                    self.execute_signal(&signal).await?;
                }
            }
            SystemEvent::ExecutionEvent(exec_event) => {
                match exec_event {
                    ExecutionEvent::OrderFilled(order_fill) => {
                        self.update_account_from_fill(&order_fill.fill).await?;
                    }
                    ExecutionEvent::PositionUpdate(pos) => {
                        self.portfolio.update_position(pos);
                    }
                    ExecutionEvent::OrderAcknowledged(_) => {
                        // Update order status
                    }
                    ExecutionEvent::OrderRejected(reject) => {
                        self.set_status(&format!("Order rejected: {}", reject.reason), true);
                    }
                    _ => {}
                }
            }
            SystemEvent::RiskEvent(risk_event) => {
                match risk_event {
                    RiskEvent::KillSwitchActivated(_) => {
                        self.kill_switch.activate(bt_core::KillReason::Manual).await?;
                    }
                    RiskEvent::LimitBreached(breach) => {
                        self.set_status(&format!("Risk limit breach: {:?}", breach.limit_type), true);
                    }
                    _ => {}
                }
            }
            SystemEvent::ConnectionStatus(status) => {
                self.market_overview.update_connection_status(status);
                self.set_status(&format!("Connection: {:?}", status), false);
            }
            SystemEvent::LogEvent(log) => {
                match log {
                    LogEvent::Error(msg) => self.ki_assistant.add_deploy_log(LogLevel::Error, msg),
                    LogEvent::Warn(msg) => self.ki_assistant.add_deploy_log(LogLevel::Warn, msg),
                    LogEvent::Info(msg) => self.ki_assistant.add_deploy_log(LogLevel::Info, msg),
                    LogEvent::Debug(msg) => self.ki_assistant.add_deploy_log(LogLevel::Debug, msg),
                }
            }
            SystemEvent::KillSwitch => {
                self.kill_switch.activate(bt_core::KillReason::Manual).await?;
            }
        }
        Ok(())
    }

    async fn select_symbol(&mut self, symbol: Symbol) -> Result<()> {
        self.security_detail.set_symbol(symbol.clone());
        self.chart.set_symbol(symbol.clone());
        self.order_book.set_symbol(symbol.clone());
        self.news.set_filter_symbol(Some(symbol.ticker.clone()));
        self.set_status(&format!("Selected {}", symbol.ticker), false);
        Ok(())
    }

    async fn place_order(&mut self, symbol: Symbol, side: Side, qty: Decimal, price: Option<Decimal>) -> Result<()> {
        if let Some(oms) = &mut self.oms {
            let order_type = price.map(|_| OrderType::Limit).unwrap_or(OrderType::Market);
            let mut order = bt_core::types::Order::new(symbol.clone(), side, order_type, qty)
                .with_tif(TimeInForce::Day);
            if let Some(p) = price {
                order = order.with_limit(p);
            }
            oms.submit_order(order, None).await?;
            self.set_status(&format!("Order submitted: {:?} {} {}", side, qty, symbol.ticker), false);
        }
        Ok(())
    }

    async fn update_account_from_fill(&mut self, _fill: &Fill) -> Result<()> {
        // Update portfolio account from fill
        if let Some(oms) = &self.oms {
            if let Ok(account) = oms.get_account(None).await {
                self.portfolio.update_account(account);
            }
        }
        Ok(())
    }

    async fn run_backtest(&mut self, strategy_name: &str) -> Result<()> {
        self.set_status("Running backtest...", false);

        // Load strategy
        if let Some(strategy) = self.ki_assistant.strategy_editor.saved_strategies
            .iter()
            .find(|s| s.name == strategy_name) {

            let backtest_engine = BacktestEngine::new(bt_strategy::backtest::BacktestConfig::default());
            let result = backtest_engine.run(&strategy.content).await?;
            self.ki_assistant.set_backtest_result(result);
            self.set_status("Backtest completed", false);
        } else {
            self.set_status(&format!("Strategy '{}' not found", strategy_name), true);
        }
        Ok(())
    }

    async fn deploy_strategy(&mut self, strategy_name: &str) -> Result<()> {
        if let Some(strategy) = self.ki_assistant.strategy_editor.saved_strategies
            .iter()
            .find(|s| s.name == strategy_name) {

            let compiler = StrategyCompiler::new();
            let compiled = compiler.compile(&strategy.content)?;

            let deployable = DeployableStrategy {
                name: strategy_name.to_string(),
                strategy: compiled.ast.clone(),
                symbols: vec![],
                capital_allocation: Decimal::new(10000, 0),
                risk_profile: "moderate".to_string(),
                status: StrategyDeployStatus::Starting,
            };

            self.ki_assistant.add_deployable_strategy(deployable);
            self.ki_assistant.set_deploy_status(DeployStatus::Running);
            self.set_status(&format!("Deployed strategy: {}", strategy_name), false);
        }
        Ok(())
    }

    async fn stop_strategy(&mut self, strategy_name: &str) -> Result<()> {
        if let Some(idx) = self.ki_assistant.deploy_panel.strategies
            .iter()
            .position(|s| s.name == strategy_name) {
            self.ki_assistant.deploy_panel.strategies[idx].status = StrategyDeployStatus::Stopping;
            self.set_status(&format!("Stopping strategy: {}", strategy_name), false);
        }
        Ok(())
    }

    async fn execute_signal(&mut self, signal: &SignalEvent) -> Result<()> {
        // Check kill switch
        if self.kill_switch.is_active() {
            return Ok(());
        }

        if let SignalEvent::Entry(entry) = signal {
            if let Some(risk) = &self.risk_manager {
                let order = bt_core::types::Order::new(
                    entry.symbol.clone(),
                    entry.side,
                    entry.entry_price.map(|_| OrderType::Limit).unwrap_or(OrderType::Market),
                    entry.quantity,
                );
                let check = risk.validate_order(&order).await;
                if let bt_core::risk_limits::RiskCheckResult::Reject(reason) = check {
                    self.set_status(&format!("Signal rejected: {}", reason), true);
                    return Ok(());
                }
            }
            self.place_order(entry.symbol.clone(), entry.side, entry.quantity, entry.entry_price).await?;
        }
        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        let is_market_focused = self.focused_pane == PaneType::MarketOverview;
        self.market_overview.set_focus(is_market_focused);
        let kill_active = self.kill_switch.is_active();

        self.terminal.draw(|frame| {
            let area = frame.area();

            // Render layout
            let panes = self.layout_manager.calculate_layout(area);

            // Render each pane
            for (pane_type, rect) in panes {
                match pane_type {
                    PaneType::MarketOverview => {
                        self.market_overview.render(frame, rect);
                    }
                    PaneType::SecurityDetail => {
                        self.security_detail.render(frame, rect);
                    }
                    PaneType::Chart => {
                        self.chart.render(frame, rect);
                    }
                    PaneType::OrderBook => {
                        self.order_book.render(frame, rect);
                    }
                    PaneType::News => {
                        self.news.render(frame, rect);
                    }
                    PaneType::Portfolio => {
                        self.portfolio.render(frame, rect);
                    }
                    PaneType::KiAssistant => {
                        self.ki_assistant.render(frame, rect);
                    }
                    _ => {}
                }
            }

            // Render command line if active
            if self.command_mode {
                Self::render_command_line_widget(frame, area, &self.command_buffer, &self.theme);
            }

            // Render status bar
            Self::render_status_bar_widget(frame, area, &self.focused_pane, kill_active, &self.theme);

            // Render any popups
            if let Some((msg, _)) = &self.status_message {
                Self::render_status_popup_widget(frame, area, msg, &self.theme);
            }
        })?;
        Ok(())
    }

    fn render_command_line_widget(frame: &mut Frame, area: Rect, command_buffer: &str, theme: &Theme) {
        let cmd_area = Rect::new(0, area.height - 1, area.width, 1);
        let text = format!(":{}", command_buffer);
        let paragraph = Paragraph::new(text)
            .style(theme.base_style())
            .block(Block::default()
                .borders(Borders::TOP)
                .border_style(theme.accent_style()));
        frame.render_widget(Clear, cmd_area);
        frame.render_widget(paragraph, cmd_area);
    }

    fn render_status_bar_widget(frame: &mut Frame, area: Rect, focused_pane: &FocusablePane, kill_active: bool, theme: &Theme) {
        let status_area = Rect::new(0, area.height - 2, area.width, 1);

        let focus_indicator = format!(" [{:?}] ", focused_pane);
        let time = chrono::Local::now().format("%H:%M:%S").to_string();
        let kill_status = if kill_active { " [KILL]" } else { "" };

        let text = format!("B-Terminal | {}{}{}", time, focus_indicator, kill_status);

        let paragraph = Paragraph::new(text)
            .style(theme.base_style())
            .block(Block::default()
                .borders(Borders::TOP)
                .border_style(theme.border_style()));
        frame.render_widget(paragraph, status_area);
    }

    fn render_status_popup_widget(frame: &mut Frame, area: Rect, message: &str, theme: &Theme) {
        let popup_area = Rect::new(
            area.width / 4,
            area.height / 2,
            area.width / 2,
            3,
        );

        let paragraph = Paragraph::new(message)
            .style(theme.base_style())
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default()
                .title(" STATUS ")
                .borders(Borders::ALL)
                .border_style(theme.negative_style()));

        frame.render_widget(Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    fn set_status(&mut self, message: &str, is_error: bool) {
        self.status_message = Some((message.to_string(), Instant::now()));
        if is_error {
            let _ = self.event_tx.send(SystemEvent::LogEvent(LogEvent::Error(message.to_string())));
        } else {
            let _ = self.event_tx.send(SystemEvent::LogEvent(LogEvent::Info(message.to_string())));
        }
    }

    fn show_help(&mut self) {
        let help = r#"
B-Terminal Commands:
  :help                    Show this help
  :quit                    Exit application
  :refresh                 Refresh all data
  :symbol <SYMBOL>         Select symbol (e.g., AAPL, BTCUSDT)
  :chart <TIMEFRAME>       Set chart timeframe (1m, 5m, 15m, 1h, 1d)
  :news [KEYWORD]          Filter news
  :portfolio               Focus portfolio pane
  :ki <mode>               Focus Ki Assistant (builder|signals|backtest|deploy)
  :backtest <STRATEGY>     Run backtest on strategy
  :deploy <STRATEGY>       Deploy strategy live
  :stop <STRATEGY>         Stop deployed strategy
  :kill                    Activate kill switch
  :flatten                 Flatten all positions
  :buy <SYMBOL> <QTY> [PRICE]  Place buy order
  :sell <SYMBOL> <QTY> [PRICE] Place sell order
  :cancel <ORDER_ID>       Cancel order
  :positions               Show positions
  :account                 Show account info
  :theme <NAME>            Change theme (bloomberg, dark, light)
  :layout <NAME>           Load layout workspace

Bloomberg-style shortcuts:
  <GO>                     Execute command (same as Enter in command mode)
  F1-F7                    Focus panes
  Tab                      Next pane
  Ctrl+K                   Kill switch
  Ctrl+F                   Flatten positions
  Ctrl+C                   Quit
"#;
        self.set_status(help, false);
    }

    fn focus_next_pane(&mut self) {
        let panes = [FocusablePane::MarketOverview,
            FocusablePane::SecurityDetail,
            FocusablePane::Chart,
            FocusablePane::OrderBook,
            FocusablePane::News,
            FocusablePane::Portfolio,
            FocusablePane::KiAssistant];

        if let Some(idx) = panes.iter().position(|p| *p == self.focused_pane) {
            let next = panes[(idx + 1) % panes.len()];
            self.focused_pane = next;
            self.layout_manager.set_focus(next);
        }
    }

    fn focus_prev_pane(&mut self) {
        let panes = [FocusablePane::MarketOverview,
            FocusablePane::SecurityDetail,
            FocusablePane::Chart,
            FocusablePane::OrderBook,
            FocusablePane::News,
            FocusablePane::Portfolio,
            FocusablePane::KiAssistant];

        if let Some(idx) = panes.iter().position(|p| *p == self.focused_pane) {
            let prev = if idx == 0 { panes.len() - 1 } else { idx - 1 };
            self.focused_pane = panes[prev];
            self.layout_manager.set_focus(panes[prev]);
        }
    }

    async fn refresh_data(&mut self) -> Result<()> {
        if let Some(data_manager) = &self.data_manager {
            data_manager.refresh_all().await?;
        }
        self.set_status("Data refreshed", false);
        Ok(())
    }

    fn show_positions(&mut self) {
        // Would show positions in a popup or focus portfolio
        let _ = self.command_tx.try_send(AppCommand::FocusPane(FocusablePane::Portfolio));
    }

    fn show_account(&mut self) {
        // Would show account details
        self.set_status("Account details shown in Portfolio pane", false);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}