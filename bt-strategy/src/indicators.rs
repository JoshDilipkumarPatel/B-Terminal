use crate::dsl::ast::IndicatorKind;
use rust_decimal::{Decimal, MathematicalOps};
use std::collections::VecDeque;
use std::fmt::Debug;

pub trait Indicator: Send + Sync + Debug {
    fn name(&self) -> &str;
    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput;
    fn reset(&mut self);
    fn is_ready(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct IndicatorInput {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub vwap: Option<Decimal>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum IndicatorOutput {
    Scalar(Decimal),
    Tuple(Vec<Decimal>),
    Bool(bool),
    None,
}

impl IndicatorOutput {
    pub fn as_scalar(&self) -> Option<Decimal> {
        match self {
            IndicatorOutput::Scalar(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_tuple(&self) -> Option<&[Decimal]> {
        match self {
            IndicatorOutput::Tuple(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            IndicatorOutput::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct RSI {
    period: usize,
    gains: VecDeque<Decimal>,
    losses: VecDeque<Decimal>,
    prev_close: Option<Decimal>,
    avg_gain: Option<Decimal>,
    avg_loss: Option<Decimal>,
    bars_seen: usize,
}

impl RSI {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            gains: VecDeque::with_capacity(period),
            losses: VecDeque::with_capacity(period),
            prev_close: None,
            avg_gain: None,
            avg_loss: None,
            bars_seen: 0,
        }
    }
}

impl Indicator for RSI {
    fn name(&self) -> &str { "RSI" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        let close = input.close;

        self.bars_seen += 1;

        if let Some(prev) = self.prev_close {
            let change = close - prev;
            if change >= Decimal::ZERO {
                self.gains.push_back(change);
                self.losses.push_back(Decimal::ZERO);
            } else {
                self.gains.push_back(Decimal::ZERO);
                self.losses.push_back(change.abs());
            }

            if self.bars_seen == self.period {
                let avg_gain = self.gains.iter().sum::<Decimal>() / Decimal::from(self.period);
                let avg_loss = self.losses.iter().sum::<Decimal>() / Decimal::from(self.period);

                self.avg_gain = Some(avg_gain);
                self.avg_loss = Some(avg_loss);
            } else if self.bars_seen > self.period {
                if let (Some(ag), Some(al)) = (self.avg_gain, self.avg_loss) {
                    let new_gain = self.gains.back().copied().unwrap_or(Decimal::ZERO);
                    let new_loss = self.losses.back().copied().unwrap_or(Decimal::ZERO);
                    self.avg_gain = Some((ag * Decimal::from(self.period - 1) + new_gain) / Decimal::from(self.period));
                    self.avg_loss = Some((al * Decimal::from(self.period - 1) + new_loss) / Decimal::from(self.period));
                }
                self.gains.pop_front();
                self.losses.pop_front();
            }
        }

        self.prev_close = Some(close);

        if self.bars_seen < self.period {
            return IndicatorOutput::None;
        }

        if let (Some(ag), Some(al)) = (self.avg_gain, self.avg_loss) {
            if al != Decimal::ZERO {
                let rs = ag / al;
                let rsi = Decimal::new(100, 0) - (Decimal::new(100, 0) / (Decimal::ONE + rs));
                return IndicatorOutput::Scalar(rsi);
            } else {
                return IndicatorOutput::Scalar(Decimal::new(100, 0));
            }
        }

        IndicatorOutput::None
    }

    fn reset(&mut self) {
        self.gains.clear();
        self.losses.clear();
        self.prev_close = None;
        self.avg_gain = None;
        self.avg_loss = None;
        self.bars_seen = 0;
    }

    fn is_ready(&self) -> bool {
        self.avg_gain.is_some() && self.avg_loss.is_some()
    }
}

#[derive(Debug)]
pub struct SMA {
    period: usize,
    values: VecDeque<Decimal>,
    sum: Decimal,
}

impl SMA {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: VecDeque::with_capacity(period),
            sum: Decimal::ZERO,
        }
    }
}

impl Indicator for SMA {
    fn name(&self) -> &str { "SMA" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        self.values.push_back(input.close);
        self.sum += input.close;

        if self.values.len() > self.period {
            if let Some(old) = self.values.pop_front() {
                self.sum -= old;
            }
        }

        if self.values.len() == self.period {
            IndicatorOutput::Scalar(self.sum / Decimal::from(self.period))
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.values.clear();
        self.sum = Decimal::ZERO;
    }

    fn is_ready(&self) -> bool {
        self.values.len() == self.period
    }
}

#[derive(Debug)]
pub struct EMA {
    #[allow(dead_code)]
    period: usize,
    multiplier: Decimal,
    value: Option<Decimal>,
    initialized: bool,
}

impl EMA {
    pub fn new(period: usize) -> Self {
        let multiplier = Decimal::new(2, 0) / Decimal::from(period + 1);
        Self {
            period,
            multiplier,
            value: None,
            initialized: false,
        }
    }
}

impl Indicator for EMA {
    fn name(&self) -> &str { "EMA" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        if !self.initialized {
            self.value = Some(input.close);
            self.initialized = true;
            return IndicatorOutput::None;
        }

        if let Some(prev) = self.value {
            let new_val = (input.close - prev) * self.multiplier + prev;
            self.value = Some(new_val);
            IndicatorOutput::Scalar(new_val)
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.value = None;
        self.initialized = false;
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }
}

#[derive(Debug)]
pub struct BollingerBands {
    period: usize,
    std_dev: Decimal,
    sma: SMA,
    values: VecDeque<Decimal>,
}

impl BollingerBands {
    pub fn new(period: usize, std_dev: Decimal) -> Self {
        Self {
            period,
            std_dev,
            sma: SMA::new(period),
            values: VecDeque::with_capacity(period),
        }
    }
}

impl Indicator for BollingerBands {
    fn name(&self) -> &str { "BB" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        let sma_out = self.sma.update(input);
        let sma_val = sma_out.as_scalar();

        self.values.push_back(input.close);
        if self.values.len() > self.period {
            self.values.pop_front();
        }

        if let Some(middle) = sma_val {
            if self.values.len() == self.period {
                let mean = middle; // Already calculated
                let variance = self.values.iter()
                    .map(|v| (*v - mean) * (*v - mean))
                    .sum::<Decimal>() / Decimal::from(self.period);

                let std = variance.sqrt().unwrap_or(Decimal::ZERO);
                let upper = middle + self.std_dev * std;
                let lower = middle - self.std_dev * std;

                IndicatorOutput::Tuple(vec![upper, middle, lower])
            } else {
                IndicatorOutput::None
            }
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.sma.reset();
        self.values.clear();
    }

    fn is_ready(&self) -> bool {
        self.sma.is_ready() && self.values.len() == self.period
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MACD {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    fast_ema: EMA,
    slow_ema: EMA,
    signal_ema: EMA,
    macd_line: Option<Decimal>,
    signal_line: Option<Decimal>,
    histogram: Option<Decimal>,
}

impl MACD {
    pub fn new(fast: usize, slow: usize, signal: usize) -> Self {
        Self {
            fast_period: fast,
            slow_period: slow,
            signal_period: signal,
            fast_ema: EMA::new(fast),
            slow_ema: EMA::new(slow),
            signal_ema: EMA::new(signal),
            macd_line: None,
            signal_line: None,
            histogram: None,
        }
    }
}

impl Indicator for MACD {
    fn name(&self) -> &str { "MACD" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        let fast_out = self.fast_ema.update(input);
        let slow_out = self.slow_ema.update(input);

        if let (Some(fast), Some(slow)) = (fast_out.as_scalar(), slow_out.as_scalar()) {
            let macd = fast - slow;
            self.macd_line = Some(macd);

            let signal_input = IndicatorInput {
                open: macd, high: macd, low: macd, close: macd,
                volume: Decimal::ZERO, vwap: None, timestamp: input.timestamp,
            };
            let signal_out = self.signal_ema.update(&signal_input);

            if let Some(signal) = signal_out.as_scalar() {
                self.signal_line = Some(signal);
                self.histogram = Some(macd - signal);
                IndicatorOutput::Tuple(vec![macd, signal, macd - signal])
            } else {
                IndicatorOutput::None
            }
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.fast_ema.reset();
        self.slow_ema.reset();
        self.signal_ema.reset();
        self.macd_line = None;
        self.signal_line = None;
        self.histogram = None;
    }

    fn is_ready(&self) -> bool {
        self.fast_ema.is_ready() && self.slow_ema.is_ready() && self.signal_ema.is_ready()
    }
}

#[derive(Debug)]
pub struct ATR {
    period: usize,
    tr_values: VecDeque<Decimal>,
    prev_close: Option<Decimal>,
    atr: Option<Decimal>,
    bars_seen: usize,
}

impl ATR {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            tr_values: VecDeque::with_capacity(period),
            prev_close: None,
            atr: None,
            bars_seen: 0,
        }
    }
}

impl Indicator for ATR {
    fn name(&self) -> &str { "ATR" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        let high_low = input.high - input.low;
        let high_close = if let Some(prev) = self.prev_close {
            (input.high - prev).abs()
        } else {
            Decimal::ZERO
        };
        let low_close = if let Some(prev) = self.prev_close {
            (input.low - prev).abs()
        } else {
            Decimal::ZERO
        };

        let tr = high_low.max(high_close).max(low_close);
        self.tr_values.push_back(tr);
        self.bars_seen += 1;
        self.prev_close = Some(input.close);

        if self.bars_seen == self.period {
            let atr = self.tr_values.iter().sum::<Decimal>() / Decimal::from(self.period);
            self.atr = Some(atr);
            IndicatorOutput::Scalar(atr)
        } else if self.bars_seen > self.period {
            if let Some(prev_atr) = self.atr {
                let new_tr = self.tr_values.back().copied().unwrap_or(Decimal::ZERO);
                let atr = (prev_atr * Decimal::from(self.period - 1) + new_tr) / Decimal::from(self.period);
                self.atr = Some(atr);
            }
            self.tr_values.pop_front();
            
            if let Some(atr) = self.atr {
                IndicatorOutput::Scalar(atr)
            } else {
                IndicatorOutput::None
            }
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.tr_values.clear();
        self.prev_close = None;
        self.atr = None;
        self.bars_seen = 0;
    }

    fn is_ready(&self) -> bool {
        self.atr.is_some()
    }
}

#[derive(Debug)]
pub struct VWAP {
    cumulative_pv: Decimal,
    cumulative_volume: Decimal,
    last_date: Option<chrono::NaiveDate>,
}

impl Default for VWAP {
    fn default() -> Self {
        Self::new()
    }
}

impl VWAP {
    pub fn new() -> Self {
        Self {
            cumulative_pv: Decimal::ZERO,
            cumulative_volume: Decimal::ZERO,
            last_date: None,
        }
    }
}

impl Indicator for VWAP {
    fn name(&self) -> &str { "VWAP" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        let bar_date = input.timestamp.date_naive();
        if Some(bar_date) != self.last_date {
            self.cumulative_pv = Decimal::ZERO;
            self.cumulative_volume = Decimal::ZERO;
            self.last_date = Some(bar_date);
        }

        let typical_price = (input.high + input.low + input.close) / Decimal::new(3, 0);
        self.cumulative_pv += typical_price * input.volume;
        self.cumulative_volume += input.volume;

        if self.cumulative_volume != Decimal::ZERO {
            IndicatorOutput::Scalar(self.cumulative_pv / self.cumulative_volume)
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.cumulative_pv = Decimal::ZERO;
        self.cumulative_volume = Decimal::ZERO;
        self.last_date = None;
    }

    fn is_ready(&self) -> bool {
        self.cumulative_volume != Decimal::ZERO
    }
}

#[derive(Debug)]
pub struct StdDev {
    period: usize,
    values: VecDeque<Decimal>,
    mean: Option<Decimal>,
}

impl StdDev {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            values: VecDeque::with_capacity(period),
            mean: None,
        }
    }
}

impl Indicator for StdDev {
    fn name(&self) -> &str { "STDDEV" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        self.values.push_back(input.close);
        if self.values.len() > self.period {
            self.values.pop_front();
        }

        if self.values.len() == self.period {
            let mean = self.values.iter().sum::<Decimal>() / Decimal::from(self.period);
            self.mean = Some(mean);
            let variance = self.values.iter()
                .map(|v| (*v - mean) * (*v - mean))
                .sum::<Decimal>() / Decimal::from(self.period);
            IndicatorOutput::Scalar(variance.sqrt().unwrap_or(Decimal::ZERO))
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.values.clear();
        self.mean = None;
    }

    fn is_ready(&self) -> bool {
        self.values.len() == self.period
    }
}

#[derive(Debug)]
pub struct Highest {
    period: usize,
    values: VecDeque<Decimal>,
}

impl Highest {
    pub fn new(period: usize) -> Self {
        Self { period, values: VecDeque::with_capacity(period) }
    }
}

impl Indicator for Highest {
    fn name(&self) -> &str { "HIGHEST" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        self.values.push_back(input.high);
        if self.values.len() > self.period {
            self.values.pop_front();
        }

        if self.values.len() == self.period {
            IndicatorOutput::Scalar(self.values.iter().copied().max().unwrap_or(Decimal::ZERO))
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.values.clear();
    }

    fn is_ready(&self) -> bool {
        self.values.len() == self.period
    }
}

#[derive(Debug)]
pub struct Lowest {
    period: usize,
    values: VecDeque<Decimal>,
}

impl Lowest {
    pub fn new(period: usize) -> Self {
        Self { period, values: VecDeque::with_capacity(period) }
    }
}

impl Indicator for Lowest {
    fn name(&self) -> &str { "LOWEST" }

    fn update(&mut self, input: &IndicatorInput) -> IndicatorOutput {
        self.values.push_back(input.low);
        if self.values.len() > self.period {
            self.values.pop_front();
        }

        if self.values.len() == self.period {
            IndicatorOutput::Scalar(self.values.iter().copied().min().unwrap_or(Decimal::MAX))
        } else {
            IndicatorOutput::None
        }
    }

    fn reset(&mut self) {
        self.values.clear();
    }

    fn is_ready(&self) -> bool {
        self.values.len() == self.period
    }
}

#[derive(Debug)]
pub struct CrossOver {
    prev_a: Option<Decimal>,
    prev_b: Option<Decimal>,
}

impl Default for CrossOver {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossOver {
    pub fn new() -> Self { Self { prev_a: None, prev_b: None } }
}

impl Indicator for CrossOver {
    fn name(&self) -> &str { "CROSS_OVER" }

    fn update(&mut self, _input: &IndicatorInput) -> IndicatorOutput {
        // This needs two series - simplified for single input
        // In practice, would be evaluated in expression engine
        IndicatorOutput::None
    }

    fn reset(&mut self) {
        self.prev_a = None;
        self.prev_b = None;
    }

    fn is_ready(&self) -> bool { false }
}

pub fn create_indicator(kind: IndicatorKind, params: &[f64]) -> Box<dyn Indicator> {
    match kind {
        IndicatorKind::RSI => {
            let period = params.first().copied().unwrap_or(14.0) as usize;
            Box::new(RSI::new(period))
        }
        IndicatorKind::SMA => {
            let period = params.first().copied().unwrap_or(20.0) as usize;
            Box::new(SMA::new(period))
        }
        IndicatorKind::EMA => {
            let period = params.first().copied().unwrap_or(20.0) as usize;
            Box::new(EMA::new(period))
        }
        IndicatorKind::BollingerBands => {
            let period = params.first().copied().unwrap_or(20.0) as usize;
            let std_dev = Decimal::from_f64_retain(params.get(1).copied().unwrap_or(2.0)).unwrap_or(Decimal::new(2, 0));
            Box::new(BollingerBands::new(period, std_dev))
        }
        IndicatorKind::MACD => {
            let fast = params.first().copied().unwrap_or(12.0) as usize;
            let slow = params.get(1).copied().unwrap_or(26.0) as usize;
            let signal = params.get(2).copied().unwrap_or(9.0) as usize;
            Box::new(MACD::new(fast, slow, signal))
        }
        IndicatorKind::ATR => {
            let period = params.first().copied().unwrap_or(14.0) as usize;
            Box::new(ATR::new(period))
        }
        IndicatorKind::VWAP => Box::new(VWAP::new()),
        IndicatorKind::StdDev => {
            let period = params.first().copied().unwrap_or(20.0) as usize;
            Box::new(StdDev::new(period))
        }
        IndicatorKind::Highest => {
            let period = params.first().copied().unwrap_or(20.0) as usize;
            Box::new(Highest::new(period))
        }
        IndicatorKind::Lowest => {
            let period = params.first().copied().unwrap_or(20.0) as usize;
            Box::new(Lowest::new(period))
        }
        _ => Box::new(DummyIndicator),
    }
}

#[derive(Debug)]
struct DummyIndicator;
impl Indicator for DummyIndicator {
    fn name(&self) -> &str { "DUMMY" }
    fn update(&mut self, _: &IndicatorInput) -> IndicatorOutput { IndicatorOutput::None }
    fn reset(&mut self) {}
    fn is_ready(&self) -> bool { false }
}
