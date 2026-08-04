use ratatui::{
    layout::Rect,
    style::{Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap, Table, Row, Cell},
    Frame,
};
use bt_core::types::{Symbol, Decimal};
use bt_core::events::{Quote, Trade, Bar, OrderBook};
use crate::theme::Theme;
use std::collections::VecDeque;

pub struct SecurityDetailWidget {
    theme: Theme,
    symbol: Option<Symbol>,
    quote: Option<Quote>,
    last_trade: Option<Trade>,
    bars: VecDeque<Bar>,
    order_book: Option<OrderBook>,
    stats: SecurityStats,
}

#[derive(Debug, Clone, Default)]
pub struct SecurityStats {
    pub open: Option<Decimal>,
    pub high: Option<Decimal>,
    pub low: Option<Decimal>,
    pub close: Option<Decimal>,
    pub prev_close: Option<Decimal>,
    pub volume: Option<Decimal>,
    pub avg_volume: Option<Decimal>,
    pub market_cap: Option<Decimal>,
    pub pe_ratio: Option<Decimal>,
    pub dividend_yield: Option<Decimal>,
    pub fifty_two_week_high: Option<Decimal>,
    pub fifty_two_week_low: Option<Decimal>,
}

impl SecurityDetailWidget {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            symbol: None,
            quote: None,
            last_trade: None,
            bars: VecDeque::with_capacity(500),
            order_book: None,
            stats: SecurityStats::default(),
        }
    }

    pub fn scroll_up(&mut self) {}
    pub fn scroll_down(&mut self) {}

    pub fn set_symbol(&mut self, symbol: Symbol) {
        self.symbol = Some(symbol);
    }

    pub fn update_quote(&mut self, quote: Quote) {
        self.quote = Some(quote);
    }

    pub fn update_trade(&mut self, trade: Trade) {
        self.last_trade = Some(trade);
    }

    pub fn update_bar(&mut self, bar: Bar) {
        self.bars.push_back(bar);
        if self.bars.len() > 500 {
            self.bars.pop_front();
        }
    }

    pub fn update_order_book(&mut self, book: OrderBook) {
        self.order_book = Some(book);
    }

    pub fn update_stats(&mut self, stats: SecurityStats) {
        self.stats = stats;
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(format!(" {} ", self.symbol.as_ref().map(|s| s.ticker.as_str()).unwrap_or("DETAIL")))
            .title_style(self.theme.title_style())
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .style(self.theme.base_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),   // Quote header
                Constraint::Length(8),   // Stats
                Constraint::Min(0),      // Order book / recent trades
            ])
            .split(inner);

        self.render_quote_header(frame, chunks[0]);
        self.render_stats(frame, chunks[1]);
        self.render_order_book_ets(frame, chunks[2]);
    }

    fn render_quote_header(&self, frame: &mut Frame, area: Rect) {
        let (bid, ask, last, change, change_pct) = if let Some(q) = &self.quote {
            let bid = q.bid_price;
            let ask = q.ask_price;
            let last = self.last_trade.as_ref().map(|t| t.price).unwrap_or(q.bid_price);
            let prev = self.stats.prev_close.unwrap_or(last);
            let change = last - prev;
            let change_pct = if prev != Decimal::ZERO { (change / prev) * Decimal::new(100, 0) } else { Decimal::ZERO };
            (bid, ask, last, change, change_pct)
        } else {
            return;
        };

        let pnl_style = if change >= Decimal::ZERO { self.theme.positive_style() } else { self.theme.negative_style() };

        let text = vec![
            Line::from(vec![
                Span::styled("LAST: ", self.theme.accent_style()),
                Span::styled(format!("${:.2}", last), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(format!("{:+.2} ({:+.2}%)", change, change_pct), pnl_style),
            ]),
            Line::from(vec![
                Span::styled("BID: ", self.theme.accent_style()),
                Span::styled(format!("${:.2} x {}", bid, self.quote.as_ref().map(|q| q.bid_size).unwrap_or(Decimal::ZERO)), self.theme.base_style()),
                Span::raw("  "),
                Span::styled("ASK: ", self.theme.accent_style()),
                Span::styled(format!("${:.2} x {}", ask, self.quote.as_ref().map(|q| q.ask_size).unwrap_or(Decimal::ZERO)), self.theme.base_style()),
            ]),
            Line::from(vec![
                Span::styled("SPREAD: ", self.theme.accent_style()),
                Span::styled(format!("${:.2}", ask - bid), self.theme.base_style()),
                Span::raw("  |  "),
                Span::styled("VOLUME: ", self.theme.accent_style()),
                Span::styled(format!("{:.0}", self.last_trade.as_ref().map(|t| t.size).unwrap_or(Decimal::ZERO)), self.theme.base_style()),
            ]),
        ];

        let paragraph = Paragraph::new(text)
            .block(Block::default()
                .title(" QUOTE ")
                .title_style(self.theme.title_style())
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_stats(&self, frame: &mut Frame, area: Rect) {
        let cells = vec![
            ("Open", self.stats.open.map(|v| format!("{:.2}", v)).unwrap_or("-".into())),
            ("High", self.stats.high.map(|v| format!("{:.2}", v)).unwrap_or("-".into())),
            ("Low", self.stats.low.map(|v| format!("{:.2}", v)).unwrap_or("-".into())),
            ("Prev Close", self.stats.prev_close.map(|v| format!("{:.2}", v)).unwrap_or("-".into())),
            ("Volume", self.stats.volume.map(|v| format!("{:.0}", v)).unwrap_or("-".into())),
            ("Avg Vol", self.stats.avg_volume.map(|v| format!("{:.0}", v)).unwrap_or("-".into())),
            ("Market Cap", self.stats.market_cap.map(|v| format!("${:.0}B", v / Decimal::new(1_000_000_000, 0))).unwrap_or("-".into())),
            ("P/E", self.stats.pe_ratio.map(|v| format!("{:.1}", v)).unwrap_or("-".into())),
            ("Div Yield", self.stats.dividend_yield.map(|v| format!("{:.2}%", v)).unwrap_or("-".into())),
            ("52W High", self.stats.fifty_two_week_high.map(|v| format!("{:.2}", v)).unwrap_or("-".into())),
            ("52W Low", self.stats.fifty_two_week_low.map(|v| format!("{:.2}", v)).unwrap_or("-".into())),
        ];

        let rows: Vec<Row> = cells.chunks(2).map(|chunk| {
            Row::new(chunk.iter().flat_map(|(k, v)| vec![
                Cell::from(Span::styled(*k, self.theme.accent_style())),
                Cell::from(Span::styled(v.clone(), self.theme.base_style())),
            ]))
        }).collect();

        let table = Table::new(rows, [Constraint::Length(15), Constraint::Length(15)])
            .block(Block::default()
                .title(" STATISTICS ")
                .title_style(self.theme.title_style())
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            )
            .column_spacing(2);

        frame.render_widget(table, area);
    }

    fn render_order_book_ets(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Order book
        if let Some(book) = &self.order_book {
            let bids: Vec<Row> = book.bids.iter().take(10).map(|level| {
                Row::new(vec![
                    Cell::from(Span::styled(format!("{:.2}", level.price), self.theme.positive_style())),
                    Cell::from(Span::styled(format!("{:.0}", level.size), self.theme.base_style())),
                ])
            }).collect();

            let asks: Vec<Row> = book.asks.iter().take(10).rev().map(|level| {
                Row::new(vec![
                    Cell::from(Span::styled(format!("{:.2}", level.price), self.theme.negative_style())),
                    Cell::from(Span::styled(format!("{:.0}", level.size), self.theme.base_style())),
                ])
            }).collect();

            let mut rows = asks;
            rows.extend(bids);

            let table = Table::new(rows, [Constraint::Length(12), Constraint::Length(10)])
                .block(Block::default()
                    .title(" ORDER BOOK ")
                    .title_style(self.theme.title_style())
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style())
                );

            frame.render_widget(table, chunks[0]);
        }

        // Recent trades (Time & Sales)
        let trades_text = if let Some(trade) = &self.last_trade {
            vec![Line::from(vec![
                Span::styled(format!("{:<10}", trade.timestamp.format("%H:%M:%S")), self.theme.info_style()),
                Span::styled(format!("{:>4}", trade.side.map(|s| format!("{:?}", s)).unwrap_or_else(|| "-".to_string())), self.theme.accent_style()),
                Span::styled(format!("{:>10.2}", trade.price), self.theme.base_style()),
                Span::styled(format!("{:>10.0}", trade.size), self.theme.base_style()),
            ])]
        } else {
            vec![Line::from("No trades")]
        };

        let paragraph = Paragraph::new(trades_text)
            .block(Block::default()
                .title(" TIME & SALES ")
                .title_style(self.theme.title_style())
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, chunks[1]);
    }
}

impl SecurityDetailWidget {
    pub fn from_theme(theme: &Theme) -> Self {
        Self::new(theme.clone())
    }
}

use ratatui::layout::{Constraint, Direction, Layout};