use ratatui::{
    layout::Rect,
    style::{Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Table, Row, Cell, Paragraph, Wrap},
    Frame,
};
use bt_core::types::{Symbol, Decimal, Position};
use crate::theme::Theme;
use std::collections::HashMap;

pub struct PortfolioWidget {
    theme: Theme,
    positions: HashMap<Symbol, Position>,
    account: Option<bt_core::types::Account>,
    list_state: ratatui::widgets::TableState,
    selected_index: usize,
}

impl PortfolioWidget {
    pub fn new(theme: Theme) -> Self {
        let mut list_state = ratatui::widgets::TableState::default();
        list_state.select(Some(0));
        Self {
            theme,
            positions: HashMap::new(),
            account: None,
            list_state,
            selected_index: 0,
        }
    }

    pub fn update_position(&mut self, position: Position) {
        self.positions.insert(position.symbol.clone(), position);
    }

    pub fn remove_position(&mut self, symbol: &Symbol) {
        self.positions.remove(symbol);
    }

    pub fn update_account(&mut self, account: bt_core::types::Account) {
        self.account = Some(account);
    }

    pub fn scroll_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    pub fn scroll_down(&mut self) {
        if self.selected_index + 1 < self.positions.len() {
            self.selected_index += 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    pub fn selected_position(&self) -> Option<&Position> {
        self.positions.values().nth(self.selected_index)
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" PORTFOLIO ")
            .title_style(self.theme.title_style())
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .style(self.theme.base_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),   // Account summary
                Constraint::Min(0),      // Positions table
            ])
            .split(inner);

        self.render_account_summary(frame, chunks[0]);
        self.render_positions_table(frame, chunks[1]);
    }

    fn render_account_summary(&self, frame: &mut Frame, area: Rect) {
        if let Some(acc) = &self.account {
            let text = vec![
                Line::from(vec![
                    Span::styled("Equity: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.equity), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("  |  "),
                    Span::styled("Cash: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.cash), self.theme.base_style()),
                    Span::raw("  |  "),
                    Span::styled("Buying Power: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.buying_power), self.theme.positive_style()),
                ]),
                Line::from(vec![
                    Span::styled("Long MV: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.long_market_value), self.theme.positive_style()),
                    Span::raw("  |  "),
                    Span::styled("Short MV: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.short_market_value), self.theme.negative_style()),
                    Span::raw("  |  "),
                    Span::styled("Day P&L: ", self.theme.accent_style()),
                    Span::styled("N/A", self.theme.base_style()),
                ]),
                Line::from(vec![
                    Span::styled("Init Margin: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.initial_margin), self.theme.base_style()),
                    Span::raw("  |  "),
                    Span::styled("Maint Margin: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.maintenance_margin), self.theme.base_style()),
                    Span::raw("  |  "),
                    Span::styled("Day Trade BP: ", self.theme.accent_style()),
                    Span::styled(format!("${:.2}", acc.day_trading_buying_power), self.theme.info_style()),
                ]),
            ];

            let paragraph = Paragraph::new(text)
                .block(Block::default()
                    .title(" ACCOUNT ")
                    .title_style(self.theme.title_style())
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style())
                )
                .wrap(Wrap { trim: true });

            frame.render_widget(paragraph, area);
        }
    }

    fn render_positions_table(&mut self, frame: &mut Frame, area: Rect) {
        let mut positions: Vec<&Position> = self.positions.values().collect();
        positions.sort_by(|a, b| {
            let a_mv = a.market_value.unwrap_or(Decimal::ZERO).abs();
            let b_mv = b.market_value.unwrap_or(Decimal::ZERO).abs();
            b_mv.cmp(&a_mv)
        });

        let rows: Vec<Row> = positions.iter().map(|pos| {
            let pnl = pos.unrealized_pnl.unwrap_or(Decimal::ZERO);
            let pnl_pct = if pos.avg_entry_price != Decimal::ZERO {
                if pos.quantity < Decimal::ZERO {
                    (pos.avg_entry_price - pos.current_price.unwrap_or(pos.avg_entry_price)) / pos.avg_entry_price * Decimal::new(100, 0)
                } else {
                    (pos.current_price.unwrap_or(pos.avg_entry_price) - pos.avg_entry_price) / pos.avg_entry_price * Decimal::new(100, 0)
                }
            } else { Decimal::ZERO };

            let pnl_style = if pnl >= Decimal::ZERO { self.theme.positive_style() } else { self.theme.negative_style() };
            let side_str = if pos.quantity >= Decimal::ZERO { "LONG" } else { "SHORT" };
            let side_style = if pos.quantity >= Decimal::ZERO { self.theme.positive_style() } else { self.theme.negative_style() };

            Row::new(vec![
                Cell::from(Span::styled(&pos.symbol.ticker, Style::default().add_modifier(Modifier::BOLD))),
                Cell::from(Span::styled(side_str, side_style)),
                Cell::from(Span::styled(format!("{:.0}", pos.quantity.abs()), self.theme.base_style())),
                Cell::from(Span::styled(format!("{:.2}", pos.avg_entry_price), self.theme.base_style())),
                Cell::from(Span::styled(
                    pos.current_price.map(|p| format!("{:.2}", p)).unwrap_or("-".into()),
                    self.theme.base_style()
                )),
                Cell::from(Span::styled(
                    pos.market_value.map(|v| format!("{:.2}", v)).unwrap_or("-".into()),
                    self.theme.base_style()
                )),
                Cell::from(Span::styled(format!("{:+.2}", pnl), pnl_style)),
                Cell::from(Span::styled(format!("{:+.2}%", pnl_pct), pnl_style)),
            ])
        }).collect();

        let table = Table::new(rows, [
            Constraint::Length(10),  // Symbol
            Constraint::Length(6),   // Side
            Constraint::Length(10),  // Qty
            Constraint::Length(10),  // Avg Entry
            Constraint::Length(10),  // Current
            Constraint::Length(12),  // Mkt Value
            Constraint::Length(10),  // P&L
            Constraint::Length(10),  // P&L %
        ])
        .header(Row::new(vec![
            Cell::from(Span::styled("SYMBOL", self.theme.accent_style())),
            Cell::from(Span::styled("SIDE", self.theme.accent_style())),
            Cell::from(Span::styled("QTY", self.theme.accent_style())),
            Cell::from(Span::styled("AVG ENTRY", self.theme.accent_style())),
            Cell::from(Span::styled("CURRENT", self.theme.accent_style())),
            Cell::from(Span::styled("MKT VALUE", self.theme.accent_style())),
            Cell::from(Span::styled("P&L", self.theme.accent_style())),
            Cell::from(Span::styled("P&L %", self.theme.accent_style())),
        ]))
        .block(Block::default()
            .title(" POSITIONS ")
            .title_style(self.theme.title_style())
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
        )
        .highlight_style(self.theme.highlight_style())
        .highlight_symbol("► ")
        .column_spacing(1);

        frame.render_stateful_widget(table, area, &mut self.list_state);
    }
}

impl PortfolioWidget {
    pub fn from_theme(theme: &Theme) -> Self {
        Self::new(theme.clone())
    }
}

use ratatui::layout::{Constraint, Direction, Layout};