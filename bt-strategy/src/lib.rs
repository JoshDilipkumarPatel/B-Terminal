//! bt-strategy - Ki Assistant trading strategy engine

pub mod backtest;
pub mod dsl;
pub mod engine;
pub mod indicators;
pub mod predictor;
pub mod risk;
pub mod garch;
pub mod options;
pub mod sentiment;
pub mod stat_arb;
pub mod syndicate;
pub mod conformal;
pub mod huggingface;
pub mod local_llm;
pub mod rl_model;
pub mod rbergomi;
pub mod rough_path;
pub mod alpha_synthesis;
pub mod portfolio;
pub mod execution;
pub mod mev;
pub mod tda;

pub use backtest::{BacktestConfig, BacktestEngine, BacktestResult, PositionSizingMethod, TradeRecord, EquityPoint, MonthlyReturn};
pub use dsl::{ast::*, compiler::{CompiledStrategy, CompileError, StrategyCompiler}, parser::StrategyParser};
pub use engine::{SignalEngine, EngineConfig, StrategyState, Signal, SignalSide};
pub use indicators::{Indicator, IndicatorInput, IndicatorOutput, create_indicator};
pub use predictor::{MarketRegime, PredictionResult, TrendPredictor};
pub use risk::{StrategyRiskManager, PositionSizer, RiskParams, FixedFractionalSizer, KellySizer, VolatilityTargetSizer, FixedNotionalSizer};
pub use garch::{GarchModel, GarchParams};
pub use options::{BlackScholes, OptionPricing, OptionKind, Greeks, OptionsChain, StrikeData};
pub use sentiment::{SentimentScorer, SentimentResult, SentimentClass};
pub use stat_arb::{StatArbSignal, StatArbResult, PairsArbitrageEngine};
pub use huggingface::{InferenceSource, HuggingFaceInferenceResult, HuggingFaceEngine};
pub use syndicate::{AgentLayer, AgentRole, MarketRegimeContext, AgentOutput, SyndicateDecision, SyndicateCouncil};