use bt_core::types::Symbol;
use anyhow::Result;
use rust_decimal::Decimal;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum ParsedCommand {
    Help,
    Quit,
    Refresh,
    Symbol(Symbol),
    Chart(String),
    News(Option<String>),
    Portfolio,
    KiAssistant(crate::widgets::ki_assistant::KiMode),
    Backtest(String),
    Deploy(String),
    Stop(String),
    Kill,
    Flatten,
    Buy { symbol: Symbol, qty: Decimal, price: Option<Decimal> },
    Sell { symbol: Symbol, qty: Decimal, price: Option<Decimal> },
    Cancel(String),
    Positions,
    Account,
    Theme(String),
    Layout(String),
}

pub struct CommandParser;

impl Default for CommandParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, input: &str) -> Result<ParsedCommand> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }

        // Handle Bloomberg-style syntax: "AAPL US <EQUITY> GO"
        // This is equivalent to :symbol AAPL
        if trimmed.ends_with(" GO") || trimmed.ends_with(" go") {
            let without_go = trimmed.trim_end_matches(" GO").trim_end_matches(" go").trim();
            if let Some(symbol_str) = without_go.split_whitespace().next() {
                let symbol = Symbol::parse(symbol_str)?;
                return Ok(ParsedCommand::Symbol(symbol));
            }
        }

        // Handle standard colon commands
        if !trimmed.starts_with(':') {
            return Err(anyhow::anyhow!("Commands must start with ':' or end with ' GO'"));
        }

        let cmd_body = &trimmed[1..].trim();
        let parts: Vec<&str> = cmd_body.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow::anyhow!("Empty command"));
        }

        match parts[0].to_lowercase().as_str() {
            "help" | "h" | "?" => Ok(ParsedCommand::Help),
            "quit" | "q" | "exit" => Ok(ParsedCommand::Quit),
            "refresh" | "r" | "reload" => Ok(ParsedCommand::Refresh),
            "symbol" | "sym" | "ticker" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :symbol <SYMBOL> (e.g., :symbol AAPL)"));
                }
                let symbol = Symbol::parse(parts[1])?;
                Ok(ParsedCommand::Symbol(symbol))
            }
            "chart" | "c" => {
                let tf = parts.get(1).unwrap_or(&"5m").to_string();
                Ok(ParsedCommand::Chart(tf))
            }
            "news" | "n" => {
                let filter = parts.get(1).map(|s| s.to_string());
                Ok(ParsedCommand::News(filter))
            }
            "portfolio" | "pos" | "p" | "holdings" => Ok(ParsedCommand::Portfolio),
            "ki" | "assistant" | "algo" => {
                let mode = parts.get(1).map(|s| match s.to_lowercase().as_str() {
                    "builder" | "b" | "edit" => crate::widgets::ki_assistant::KiMode::StrategyBuilder,
                    "signals" | "s" | "monitor" => crate::widgets::ki_assistant::KiMode::SignalMonitor,
                    "backtest" | "bt" | "test" => crate::widgets::ki_assistant::KiMode::Backtest,
                    "deploy" | "d" | "live" => crate::widgets::ki_assistant::KiMode::Deploy,
                    _ => crate::widgets::ki_assistant::KiMode::StrategyBuilder,
                }).unwrap_or(crate::widgets::ki_assistant::KiMode::StrategyBuilder);
                Ok(ParsedCommand::KiAssistant(mode))
            }
            "backtest" | "bt" | "test" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :backtest <STRATEGY_NAME>"));
                }
                Ok(ParsedCommand::Backtest(parts[1].to_string()))
            }
            "deploy" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :deploy <STRATEGY_NAME>"));
                }
                Ok(ParsedCommand::Deploy(parts[1].to_string()))
            }
            "stop" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :stop <STRATEGY_NAME>"));
                }
                Ok(ParsedCommand::Stop(parts[1].to_string()))
            }
            "kill" | "killswitch" | "panic" => Ok(ParsedCommand::Kill),
            "flatten" | "flat" | "closeall" => Ok(ParsedCommand::Flatten),
            "buy" | "b" | "long" => {
                if parts.len() < 3 {
                    return Err(anyhow::anyhow!("Usage: :buy <SYMBOL> <QTY> [PRICE]"));
                }
                let symbol = Symbol::parse(parts[1])?;
                let qty = Decimal::from_str(parts[2])?;
                let price = parts.get(3).and_then(|p| Decimal::from_str(p).ok());
                Ok(ParsedCommand::Buy { symbol, qty, price })
            }
            "sell" | "s" | "short" => {
                if parts.len() < 3 {
                    return Err(anyhow::anyhow!("Usage: :sell <SYMBOL> <QTY> [PRICE]"));
                }
                let symbol = Symbol::parse(parts[1])?;
                let qty = Decimal::from_str(parts[2])?;
                let price = parts.get(3).and_then(|p| Decimal::from_str(p).ok());
                Ok(ParsedCommand::Sell { symbol, qty, price })
            }
            "cancel" | "cxl" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :cancel <ORDER_ID>"));
                }
                Ok(ParsedCommand::Cancel(parts[1].to_string()))
            }
            "positions" | "position" => Ok(ParsedCommand::Positions),
            "account" | "acct" | "balance" => Ok(ParsedCommand::Account),
            "theme" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :theme <NAME> (bloomberg, dark, light, high_contrast)"));
                }
                Ok(ParsedCommand::Theme(parts[1].to_string()))
            }
            "layout" | "workspace" | "ws" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :layout <NAME> (default, trading, research, minimal)"));
                }
                Ok(ParsedCommand::Layout(parts[1].to_string()))
            }
            "save" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :save <STRATEGY_NAME>"));
                }
                // This would trigger strategy save in Ki Assistant
                Ok(ParsedCommand::Deploy(parts[1].to_string())) // placeholder
            }
            "load" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Usage: :load <STRATEGY_NAME>"));
                }
                Ok(ParsedCommand::Backtest(parts[1].to_string())) // placeholder
            }
            _ => Err(anyhow::anyhow!("Unknown command: ':{}'. Type :help for help.", parts[0])),
        }
    }

    pub fn get_suggestions(&self, partial: &str) -> Vec<String> {
        let commands = vec![
            "help", "quit", "refresh", "symbol", "chart", "news", "portfolio",
            "ki", "backtest", "deploy", "stop", "kill", "flatten",
            "buy", "sell", "cancel", "positions", "account",
            "theme", "layout", "save", "load",
        ];

        let bloomberg_commands = vec![
            "AAPL US <EQUITY> GO",
            "SPY US <EQUITY> GO",
            "BTCUSDT BINANCE <CRYPTO> GO",
            "ES1! CME <FUTURE> GO",
        ];

        let mut suggestions = Vec::new();

        if let Some(cmd_part) = partial.strip_prefix(':') {
            for cmd in commands {
                if cmd.starts_with(cmd_part) {
                    suggestions.push(format!(":{}", cmd));
                }
            }
        } else if partial.ends_with(" GO") || partial.ends_with(" go") {
            for bc in bloomberg_commands {
                if bc.to_lowercase().starts_with(&partial.to_lowercase()) {
                    suggestions.push(bc.to_string());
                }
            }
        } else {
            // Suggest both styles
            for cmd in commands {
                if cmd.starts_with(partial) {
                    suggestions.push(format!(":{}", cmd));
                }
            }
            for bc in bloomberg_commands {
                if bc.to_lowercase().starts_with(&partial.to_lowercase()) {
                    suggestions.push(bc.to_string());
                }
            }
        }

        suggestions
    }

    pub fn get_help_text(&self) -> String {
        r#"
B-Terminal Command Reference
=============================

Command Syntax:
  :command [args...]        Standard command (colon-prefixed)
  SYM US <EQUITY> GO        Bloomberg-style syntax

Global Commands:
  :help, :h, :?             Show this help
  :quit, :q, :exit          Exit application
  :refresh, :r              Refresh all market data
  :theme <name>             Change theme (bloomberg, dark, light, high_contrast)
  :layout <name>            Load workspace layout (default, trading, research, minimal)

Symbol Selection:
  :symbol <SYM>, :sym <SYM>, :s <SYM>   Select symbol (e.g., :symbol AAPL)
  AAPL US <EQUITY> GO       Bloomberg-style symbol selection

Market Data:
  :chart <TF>               Set chart timeframe (1m, 5m, 15m, 1h, 4h, 1d)
  :news [KEYWORD]           Filter news headlines

Trading:
  :buy <SYM> <QTY> [PRICE]  Place buy order (market if no price)
  :sell <SYM> <QTY> [PRICE] Place sell order (market if no price)
  :cancel <ORDER_ID>        Cancel pending order
  :positions                Show current positions
  :account                  Show account summary
  :flatten                  Close all positions immediately
  :kill                     Activate emergency kill switch

Ki Assistant (Algorithmic Trading):
  :ki builder               Strategy Builder mode
  :ki signals               Signal Monitor mode
  :ki backtest              Backtest mode
  :ki deploy                Deploy mode
  :backtest <STRATEGY>      Run backtest on saved strategy
  :deploy <STRATEGY>        Deploy strategy live
  :stop <STRATEGY>          Stop deployed strategy

Keyboard Shortcuts:
  F1-F7                     Focus panes (Market, Detail, Chart, Book, News, Portfolio, Ki)
  Tab / Shift+Tab           Cycle focus forward/backward
  :                         Enter command mode
  Esc                       Exit command mode / close popups
  Up/Down                   Scroll in focused pane
  Left/Right                Navigate chart / switch Ki tabs
  Enter                     Select symbol / execute in Ki
  Ctrl+K                    Kill switch
  Ctrl+F                    Flatten all positions
  Ctrl+C                    Quit application

Bloomberg-Style Shortcuts:
  <GO>                      Execute (type symbol then " GO")
  <HELP>                    Type :help
  <MENU>                    Type :ki
  <PORT>                    Type :portfolio
  <NEWS>                    Type :news
  <CHART>                   Type :chart

Examples:
  :symbol AAPL              Select Apple
  AAPL US <EQUITY> GO       Same as above (Bloomberg style)
  :chart 1h                 Set 1-hour chart
  :buy AAPL 100 150.00      Buy 100 shares limit $150
  :sell AAPL 100            Sell 100 shares market
  :ki builder               Open strategy builder
  :backtest "Mean Rev"      Backtest "Mean Rev" strategy
  :deploy "Mean Rev"        Deploy "Mean Rev" live
  :theme dark               Switch to dark theme
"#.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_help() {
        let parser = CommandParser::new();
        assert!(parser.parse(":help").is_ok());
        assert!(parser.parse(":h").is_ok());
        assert!(parser.parse(":?").is_ok());
    }

    #[test]
    fn test_parse_symbol() {
        let parser = CommandParser::new();
        let result = parser.parse(":symbol AAPL").unwrap();
        assert!(matches!(result, ParsedCommand::Symbol(s) if s.ticker == "AAPL"));
    }

    #[test]
    fn test_parse_bloomberg_style() {
        let parser = CommandParser::new();
        let result = parser.parse("AAPL US <EQUITY> GO").unwrap();
        assert!(matches!(result, ParsedCommand::Symbol(s) if s.ticker == "AAPL"));
    }

    #[test]
    fn test_parse_buy() {
        let parser = CommandParser::new();
        let result = parser.parse(":buy AAPL 100 150.00").unwrap();
        assert!(matches!(result, ParsedCommand::Buy { symbol, qty, price } if symbol.ticker == "AAPL" && qty == rust_decimal::Decimal::new(100, 0) && price == Some(rust_decimal::Decimal::new(15000, 2))));
    }

    #[test]
    fn test_parse_sell() {
        let parser = CommandParser::new();
        let result = parser.parse(":sell AAPL 100").unwrap();
        assert!(matches!(result, ParsedCommand::Sell { symbol, qty, price } if symbol.ticker == "AAPL" && qty == rust_decimal::Decimal::new(100, 0) && price.is_none()));
    }

    #[test]
    fn test_parse_chart() {
        let parser = CommandParser::new();
        let result = parser.parse(":chart 1h").unwrap();
        assert!(matches!(result, ParsedCommand::Chart(tf) if tf == "1h"));
    }

    #[test]
    fn test_parse_ki_modes() {
        let parser = CommandParser::new();
        assert!(matches!(parser.parse(":ki builder").unwrap(), ParsedCommand::KiAssistant(crate::widgets::ki_assistant::KiMode::StrategyBuilder)));
        assert!(matches!(parser.parse(":ki signals").unwrap(), ParsedCommand::KiAssistant(crate::widgets::ki_assistant::KiMode::SignalMonitor)));
        assert!(matches!(parser.parse(":ki backtest").unwrap(), ParsedCommand::KiAssistant(crate::widgets::ki_assistant::KiMode::Backtest)));
        assert!(matches!(parser.parse(":ki deploy").unwrap(), ParsedCommand::KiAssistant(crate::widgets::ki_assistant::KiMode::Deploy)));
    }
}