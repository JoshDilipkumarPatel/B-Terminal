use ratatui::{
    layout::Rect,
    style::Color,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Dataset, GraphType, Chart, Axis},
    Frame,
};
use bt_core::types::{Decimal, Symbol};
use bt_core::events::{Bar, Quote, Trade};
use rust_decimal::prelude::ToPrimitive;
use crate::theme::Theme;
use std::collections::VecDeque;

pub struct ChartWidget {
    theme: Theme,
    symbol: Option<Symbol>,
    bars: VecDeque<Bar>,
    indicators: HashMap<String, Vec<Decimal>>,
    timeframe: String,
    show_volume: bool,
    show_indicators: Vec<String>,
    crosshair: Option<usize>,
    visible_start: usize,
    visible_count: usize,
}

impl ChartWidget {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            symbol: None,
            bars: VecDeque::with_capacity(500),
            indicators: HashMap::new(),
            timeframe: "5m".to_string(),
            show_volume: true,
            show_indicators: Vec::new(),
            crosshair: None,
            visible_start: 0,
            visible_count: 50,
        }
    }

    pub fn scroll_left(&mut self) {
        self.visible_start = self.visible_start.saturating_sub(5);
    }
    pub fn scroll_right(&mut self) {
        let max_start = self.bars.len().saturating_sub(self.visible_count);
        self.visible_start = (self.visible_start + 5).min(max_start);
    }
    pub fn zoom_in(&mut self) {
        self.visible_count = self.visible_count.saturating_sub(10).max(10);
        let max_start = self.bars.len().saturating_sub(self.visible_count);
        self.visible_start = self.visible_start.min(max_start);
    }
    pub fn zoom_out(&mut self) {
        self.visible_count = (self.visible_count + 10).min(self.bars.len().max(10));
        let max_start = self.bars.len().saturating_sub(self.visible_count);
        self.visible_start = self.visible_start.min(max_start);
    }
    pub fn toggle_indicators(&mut self) {
        if self.show_indicators.is_empty() {
            self.show_indicators = self.indicators.keys().cloned().collect();
        } else {
            self.show_indicators.clear();
        }
    }

    pub fn update_quote(&mut self, quote: Quote) {
        if let Some(last) = self.bars.back_mut() {
            last.close = quote.ask_price;
            last.high = last.high.max(quote.ask_price);
            last.low = last.low.min(quote.ask_price);
        }
    }
    pub fn update_trade(&mut self, trade: Trade) {
        if let Some(last) = self.bars.back_mut() {
            last.close = trade.price;
            last.high = last.high.max(trade.price);
            last.low = last.low.min(trade.price);
            last.volume += trade.size;
        }
    }

    pub fn set_symbol(&mut self, symbol: Symbol) {
        self.symbol = Some(symbol);
    }

    pub fn set_timeframe(&mut self, tf: &str) {
        self.timeframe = tf.to_string();
    }

    pub fn add_bar(&mut self, bar: Bar) {
        self.bars.push_back(bar);
        if self.bars.len() > 10000 {
            self.bars.pop_front();
        }
        let max_start = self.bars.len().saturating_sub(self.visible_count);
        self.visible_start = self.visible_start.min(max_start);
    }

    pub fn set_bars(&mut self, bars: Vec<Bar>) {
        self.bars = bars.into();
        let max_start = self.bars.len().saturating_sub(self.visible_count);
        self.visible_start = self.visible_start.min(max_start);
    }

    pub fn set_indicator(&mut self, name: String, values: Vec<Decimal>) {
        self.indicators.insert(name, values);
    }

    pub fn toggle_volume(&mut self) {
        self.show_volume = !self.show_volume;
    }

    pub fn set_crosshair(&mut self, index: Option<usize>) {
        self.crosshair = index;
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(format!(" {} {} ", self.symbol.as_ref().map(|s| s.ticker.as_str()).unwrap_or("CHART"), self.timeframe))
            .title_style(self.theme.title_style())
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .style(self.theme.base_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.bars.is_empty() {
            let text = vec![Line::from(Span::styled("No data", self.theme.base_style()))];
            let paragraph = Paragraph::new(text).alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(paragraph, inner);
            return;
        }

        // Split for volume
        let chunks = if self.show_volume {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
                .split(inner).to_vec()
        } else {
            vec![inner]
        };

        self.render_price_chart(frame, chunks[0]);
        if self.show_volume && chunks.len() > 1 {
            self.render_volume_chart(frame, chunks[1]);
        }
    }

    fn render_price_chart(&self, frame: &mut Frame, area: Rect) {
        let len = self.bars.len();
        if len == 0 { return; }

        // Calculate min/max for Y axis
        let visible_bars: Vec<&Bar> = self.bars.iter().skip(self.visible_start).take(self.visible_count).collect();
        let (min_price, max_price) = visible_bars.iter().fold(
            (Decimal::MAX, Decimal::ZERO),
            |(min, max), bar| (min.min(bar.low), max.max(bar.high))
        );

        let price_padding = (max_price - min_price) * Decimal::new(5, 2); // 5% padding
        let y_min = min_price - price_padding;
        let y_max = max_price + price_padding;

        // Candlestick data
        let candle_data: Vec<(f64, f64, f64, f64, f64)> = visible_bars.iter().enumerate().map(|(i, bar)| {
            (
                i as f64,
                bar.open.to_f64().unwrap_or(0.0),
                bar.high.to_f64().unwrap_or(0.0),
                bar.low.to_f64().unwrap_or(0.0),
                bar.close.to_f64().unwrap_or(0.0),
            )
        }).collect();

        // Create candlestick datasets (simplified - using lines for open/close and high/low)
        let _open_close_data: Vec<(f64, f64)> = candle_data.iter().map(|(i, o, _h, _l, c)| {
            let _color = if c >= o { self.theme.positive_style().fg.unwrap_or(Color::Green) } else { self.theme.negative_style().fg.unwrap_or(Color::Red) };
            (*i, if c >= o { *c } else { *o })
        }).collect();

        // For simplicity, render as line chart of close prices
        let close_data: Vec<(f64, f64)> = visible_bars.iter().enumerate()
            .map(|(i, bar)| (i as f64, bar.close.to_f64().unwrap_or(0.0)))
            .collect();

        let datasets = vec![
            Dataset::default()
                .name("Close")
                .marker(ratatui::symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(self.theme.accent_style())
                .data(&close_data),
        ];

        // Add indicator lines
        let mut all_datasets = datasets;
        let mut ind_series: Vec<(&String, Vec<(f64, f64)>)> = Vec::new();
        for (name, values) in &self.indicators {
            if self.show_indicators.contains(name) {
                let indicator_data: Vec<(f64, f64)> = values.iter().skip(self.visible_start).take(self.visible_count).enumerate()
                    .map(|(i, v)| (i as f64, v.to_f64().unwrap_or(0.0)))
                    .collect();
                ind_series.push((name, indicator_data));
            }
        }
        for (name, data) in &ind_series {
            all_datasets.push(
                Dataset::default()
                    .name(name.as_str())
                    .marker(ratatui::symbols::Marker::Dot)
                    .graph_type(GraphType::Line)
                    .style(self.theme.info_style())
                    .data(data)
            );
        }

        let x_labels: Vec<Span> = if visible_bars.len() <= 20 {
            (0..visible_bars.len()).step_by(1).map(|i| {
                Span::styled(
                    visible_bars[i].timestamp.format("%H:%M").to_string(),
                    self.theme.base_style()
                )
            }).collect()
        } else {
            (0..visible_bars.len()).step_by(visible_bars.len() / 10).map(|i| {
                Span::styled(
                    visible_bars[i].timestamp.format("%H:%M").to_string(),
                    self.theme.base_style()
                )
            }).collect()
        };

        let chart = Chart::new(all_datasets)
            .block(Block::default()
                .borders(Borders::NONE)
                .style(self.theme.base_style())
            )
            .x_axis(
                Axis::default()
                    .title(Span::styled("Time", self.theme.accent_style()))
                    .style(self.theme.base_style())
                    .labels(x_labels)
                    .bounds([0.0, (visible_bars.len().saturating_sub(1)) as f64]),
            )
            .y_axis(
                Axis::default()
                    .title(Span::styled("Price", self.theme.accent_style()))
                    .style(self.theme.base_style())
                    .labels(vec![
                        Span::styled(format!("{:.2}", y_min), self.theme.base_style()),
                        Span::styled(format!("{:.2}", (y_min + y_max) / Decimal::new(2, 0)), self.theme.base_style()),
                        Span::styled(format!("{:.2}", y_max), self.theme.base_style()),
                    ])
                    .bounds([y_min.to_f64().unwrap_or(0.0), y_max.to_f64().unwrap_or(100.0)]),
            );

        frame.render_widget(chart, area);

        // Crosshair info
        if let Some(idx) = self.crosshair {
            if idx < self.bars.len() {
                let bar = &self.bars[idx];
                let text = vec![
                    Line::from(vec![
                        Span::styled("O: ", self.theme.accent_style()),
                        Span::styled(format!("{:.2}", bar.open), self.theme.base_style()),
                        Span::raw("  H: "),
                        Span::styled(format!("{:.2}", bar.high), self.theme.base_style()),
                        Span::raw("  L: "),
                        Span::styled(format!("{:.2}", bar.low), self.theme.base_style()),
                        Span::raw("  C: "),
                        Span::styled(format!("{:.2}", bar.close), self.theme.base_style()),
                        Span::raw("  V: "),
                        Span::styled(format!("{:.0}", bar.volume), self.theme.base_style()),
                    ]),
                ];
                let paragraph = Paragraph::new(text)
                    .style(self.theme.highlight_style())
                    .alignment(ratatui::layout::Alignment::Right);
                frame.render_widget(paragraph, Rect::new(area.x + 2, area.y + 1, 60, 1));
            }
        }
    }

    fn render_volume_chart(&self, frame: &mut Frame, area: Rect) {
        let len = self.bars.len();
        if len == 0 { return; }

        let visible_bars: Vec<&Bar> = self.bars.iter().skip(self.visible_start).take(self.visible_count).collect();

        let max_vol = visible_bars.iter().map(|b| b.volume).fold(Decimal::ZERO, |a, b| a.max(b));

        let volume_data: Vec<(f64, f64)> = visible_bars.iter().enumerate()
            .map(|(i, bar)| (i as f64, bar.volume.to_f64().unwrap_or(0.0)))
            .collect();

        let dataset = Dataset::default()
            .name("Volume")
            .marker(ratatui::symbols::Marker::Braille)
            .graph_type(GraphType::Bar)
            .style(self.theme.info_style())
            .data(&volume_data);

        let chart = Chart::new(vec![dataset])
            .block(Block::default()
                .borders(Borders::NONE)
                .style(self.theme.base_style())
            )
            .x_axis(
                Axis::default()
                    .style(self.theme.base_style())
                    .bounds([0.0, (visible_bars.len().saturating_sub(1)) as f64]),
            )
            .y_axis(
                Axis::default()
                    .style(self.theme.base_style())
                    .bounds([0.0, max_vol.to_f64().unwrap_or(1.0)]),
            );

        frame.render_widget(chart, area);
    }
}

impl ChartWidget {
    pub fn from_theme(theme: &Theme) -> Self {
        Self::new(theme.clone())
    }
}

use ratatui::layout::{Constraint, Direction, Layout};
use std::collections::HashMap;