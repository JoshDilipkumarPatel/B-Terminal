#[derive(pest_derive::Parser)]
#[grammar = "src/dsl/strategy.pest"]
pub struct StrategyParser;