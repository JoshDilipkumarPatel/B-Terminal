use ratatui::{
    layout::{Constraint, Rect},
    style::{Style, Modifier, Color},
    text::{Line, Span},
    widgets::{Block, Borders, ListState, Table, Row, Cell},
    Frame,
};
use bt_core::events::{OrderBook, Quote};
use bt_core::types::Symbol;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use crate::theme::Theme;

pub struct OrderBookWidget {
    theme: Theme,
    symbol: Option<Symbol>,
    order_book: Option<OrderBook>,
    depth: usize,
    list_state: ListState,
}

impl OrderBookWidget {
    pub fn new(theme: Theme) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            theme,
            symbol: None,
            order_book: None,
            depth: 20,
            list_state,
        }
    }

    pub fn set_symbol(&mut self, symbol: Symbol) {
        self.symbol = Some(symbol);
    }

    pub fn update_order_book(&mut self, book: OrderBook) {
        self.order_book = Some(book);
    }

    pub fn update_quote(&mut self, _quote: Quote) {}

    pub fn set_depth(&mut self, depth: usize) {
        self.depth = depth;
    }

    pub fn increase_depth(&mut self) {
        self.depth = (self.depth + 5).min(100);
    }

    pub fn decrease_depth(&mut self) {
        self.depth = self.depth.saturating_sub(5).max(5);
    }

    pub fn scroll_up(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        if i > 0 {
            self.list_state.select(Some(i - 1));
        }
    }

    pub fn scroll_down(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        if i < self.depth * 2 - 1 {
            self.list_state.select(Some(i + 1));
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(format!(" {} L2 ", self.symbol.as_ref().map(|s| s.ticker.as_str()).unwrap_or("ORDER BOOK")))
            .title_style(self.theme.title_style())
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .style(self.theme.base_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(book) = &self.order_book {
            // Calculate cumulative volumes
            let mut cum_bid_vol = Decimal::ZERO;
            let mut cum_ask_vol = Decimal::ZERO;

            let max_bid = book.bids.iter().map(|b| b.size).fold(Decimal::ZERO, |a, b| a.max(b)).to_f64().unwrap_or(1.0);
            let max_ask = book.asks.iter().map(|b| b.size).fold(Decimal::ZERO, |a, b| a.max(b)).to_f64().unwrap_or(1.0);
            let max_vol = max_bid.max(max_ask).max(1.0);
            
            let mut bid_imbalance = 0.0;
            let mut ask_imbalance = 0.0;
            if max_vol > 0.0 {
                let total_bid: f64 = book.bids.iter().map(|b| b.size.to_f64().unwrap_or(0.0)).sum();
                let total_ask: f64 = book.asks.iter().map(|b| b.size.to_f64().unwrap_or(0.0)).sum();
                if total_bid + total_ask > 0.0 {
                    bid_imbalance = total_bid / (total_bid + total_ask) * 100.0;
                    ask_imbalance = total_ask / (total_bid + total_ask) * 100.0;
                }
            }

            let max_bar_len = 10;

            let rows: Vec<Row> = (0..self.depth.min(book.bids.len().max(book.asks.len()))).map(|i| {
                cum_bid_vol += book.bids.get(i).map(|l| l.size).unwrap_or(Decimal::ZERO);
                cum_ask_vol += book.asks.get(i).map(|l| l.size).unwrap_or(Decimal::ZERO);

                let bid = book.bids.get(i);
                let ask = book.asks.get(i);

                let bid_price = bid.map(|l| format!("{:.2}", l.price)).unwrap_or_else(|| "".into());
                let bid_size = bid.map(|l| format!("{:.0}", l.size)).unwrap_or_else(|| "".into());
                let bid_cum = bid.map(|_| format!("{:.0}", cum_bid_vol)).unwrap_or_else(|| "".into());
                
                let bid_bar_len = bid.map(|l| (l.size.to_f64().unwrap_or(0.0) / max_vol * max_bar_len as f64).round() as usize).unwrap_or(0);
                let bid_bar = format!("{:>10}", "█".repeat(bid_bar_len));

                let ask_price = ask.map(|l| format!("{:.2}", l.price)).unwrap_or_else(|| "".into());
                let ask_size = ask.map(|l| format!("{:.0}", l.size)).unwrap_or_else(|| "".into());
                let ask_cum = ask.map(|_| format!("{:.0}", cum_ask_vol)).unwrap_or_else(|| "".into());
                
                let ask_bar_len = ask.map(|l| (l.size.to_f64().unwrap_or(0.0) / max_vol * max_bar_len as f64).round() as usize).unwrap_or(0);
                let ask_bar = format!("{:<10}", "█".repeat(ask_bar_len));

                Row::new(vec![
                    Cell::from(Span::styled(bid_cum, self.theme.positive_style())),
                    Cell::from(Span::styled(bid_size, self.theme.base_style())),
                    Cell::from(Span::styled(bid_bar, self.theme.positive_style().add_modifier(Modifier::DIM))),
                    Cell::from(Span::styled(bid_price, self.theme.positive_style().add_modifier(Modifier::BOLD))),
                    Cell::from(Span::styled(" | ", Style::default().fg(Color::DarkGray))),
                    Cell::from(Span::styled(ask_price, self.theme.negative_style().add_modifier(Modifier::BOLD))),
                    Cell::from(Span::styled(ask_bar, self.theme.negative_style().add_modifier(Modifier::DIM))),
                    Cell::from(Span::styled(ask_size, self.theme.base_style())),
                    Cell::from(Span::styled(ask_cum, self.theme.negative_style())),
                ])
            }).collect();
            
            let best_bid = book.bids.first().map(|b| b.price).unwrap_or(Decimal::ZERO);
            let best_ask = book.asks.first().map(|b| b.price).unwrap_or(Decimal::ZERO);
            let spread = best_ask - best_bid;
            let spread_pct = if best_ask > Decimal::ZERO { spread / best_ask * Decimal::new(100, 0) } else { Decimal::ZERO };
            
            let table_title = format!(" BIDS | Spread: {:.2} ({:.2}%) | ASKS ", spread, spread_pct);
            let title_span = format!(" Bid Pressure: {:.1}% | Ask Pressure: {:.1}% ", bid_imbalance, ask_imbalance);

            let table = Table::new(rows, [
                Constraint::Length(12), // Bid cum
                Constraint::Length(10), // Bid size
                Constraint::Length(10), // Bid bar
                Constraint::Length(10), // Bid price
                Constraint::Length(3),  // Separator
                Constraint::Length(10), // Ask price
                Constraint::Length(10), // Ask bar
                Constraint::Length(10), // Ask size
                Constraint::Length(12), // Ask cum
            ])
            .block(Block::default()
                .title(ratatui::widgets::block::Title::from(table_title).alignment(ratatui::layout::Alignment::Center))
                .title(ratatui::widgets::block::Title::from(title_span).alignment(ratatui::layout::Alignment::Right))
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            )
            .highlight_style(self.theme.highlight_style())
            .highlight_symbol("► ");

            frame.render_widget(table, inner);
        } else {
            let text = vec![Line::from(Span::styled("Waiting for order book data...", self.theme.base_style()))];
            let paragraph = ratatui::widgets::Paragraph::new(text).alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(paragraph, inner);
        }
    }
}

impl OrderBookWidget {
    pub fn from_theme(theme: &Theme) -> Self {
        Self::new(theme.clone())
    }
}