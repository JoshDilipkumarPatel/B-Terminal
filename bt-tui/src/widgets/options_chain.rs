use ratatui::prelude::*;
use ratatui::widgets::*;
use rust_decimal::Decimal;

pub struct OptionsChainWidget {
    pub underlying: String,
    pub spot_price: f64,
    pub expiry: String,
    pub strikes: Vec<OptionsStrikeRow>,
    pub selected_index: usize,
    pub list_state: ListState,
}

pub struct OptionsStrikeRow {
    pub strike: f64,
    pub call_price: f64,
    pub call_delta: f64,
    pub call_gamma: f64,
    pub call_theta: f64,
    pub call_vega: f64,
    pub call_iv: f64,
    pub put_price: f64,
    pub put_delta: f64,
    pub put_gamma: f64,
    pub put_theta: f64,
    pub put_vega: f64,
    pub put_iv: f64,
}

impl OptionsChainWidget {
    pub fn new() -> Self {
        let spot = 24500.0;
        let mut strikes = Vec::new();
        for strike in (23500..=25500).step_by(50) {
            let s = strike as f64;
            // Generate some dummy data
            strikes.push(OptionsStrikeRow {
                strike: s,
                call_price: (spot - s).max(0.0) + 150.0 - ((s - spot).abs() * 0.1).min(100.0),
                call_delta: if s < spot { 0.8 } else { 0.2 },
                call_gamma: 0.02,
                call_theta: -12.5,
                call_vega: 8.4,
                call_iv: 14.2,
                put_price: (s - spot).max(0.0) + 150.0 - ((s - spot).abs() * 0.1).min(100.0),
                put_delta: if s < spot { -0.2 } else { -0.8 },
                put_gamma: 0.02,
                put_theta: -12.5,
                put_vega: 8.4,
                put_iv: 14.5,
            });
        }
        
        Self {
            underlying: "NIFTY".to_string(),
            spot_price: spot,
            expiry: "2026-08-06".to_string(),
            strikes,
            selected_index: 0,
            list_state: ListState::default(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(format!(" {} OPTIONS CHAIN (Spot: {}) Exp: {} ", self.underlying, self.spot_price, self.expiry))
            .borders(Borders::ALL);
            
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let rows: Vec<Row> = self.strikes.iter().map(|row| {
            let is_call_itm = row.strike < self.spot_price;
            let is_put_itm = row.strike > self.spot_price;
            let is_atm = (row.strike - self.spot_price).abs() <= 50.0;
            
            let strike_style = if is_atm {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            
            let call_style = if is_call_itm {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Gray)
            };
            
            let put_style = if is_put_itm {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Gray)
            };

            Row::new(vec![
                Cell::from(format!("{:.1}%", row.call_iv)).style(call_style),
                Cell::from(format!("{:.2}", row.call_vega)).style(call_style),
                Cell::from(format!("{:.2}", row.call_theta)).style(call_style),
                Cell::from(format!("{:.4}", row.call_gamma)).style(call_style),
                Cell::from(format!("{:.2}", row.call_delta)).style(call_style),
                Cell::from(format!("{:.2}", row.call_price)).style(call_style),
                Cell::from(format!("{}", row.strike)).style(strike_style),
                Cell::from(format!("{:.2}", row.put_price)).style(put_style),
                Cell::from(format!("{:.2}", row.put_delta)).style(put_style),
                Cell::from(format!("{:.4}", row.put_gamma)).style(put_style),
                Cell::from(format!("{:.2}", row.put_theta)).style(put_style),
                Cell::from(format!("{:.2}", row.put_vega)).style(put_style),
                Cell::from(format!("{:.1}%", row.put_iv)).style(put_style),
            ])
        }).collect();

        let widths = [
            Constraint::Length(8), // IV
            Constraint::Length(8), // Vega
            Constraint::Length(8), // Theta
            Constraint::Length(8), // Gamma
            Constraint::Length(8), // Delta
            Constraint::Length(10), // Price
            Constraint::Length(10), // Strike
            Constraint::Length(10), // Price
            Constraint::Length(8), // Delta
            Constraint::Length(8), // Gamma
            Constraint::Length(8), // Theta
            Constraint::Length(8), // Vega
            Constraint::Length(8), // IV
        ];

        let table = Table::new(rows, widths)
            .header(Row::new(vec![
                "IV%", "Vega", "Theta", "Gamma", "Delta", "CALLS", "STRIKE", "PUTS", "Delta", "Gamma", "Theta", "Vega", "IV%"
            ]).style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)))
            .highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol(">> ");

        frame.render_stateful_widget(table, inner, &mut self.list_state);
    }
}
