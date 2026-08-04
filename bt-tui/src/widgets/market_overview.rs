use ratatui::{
    layout::Rect,
    style::{Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use bt_core::types::{Symbol, Decimal};
use bt_core::events::{Quote, Trade};
use crate::theme::Theme;
use std::collections::HashMap;

pub struct MarketOverviewWidget {
    theme: Theme,
    quotes: HashMap<Symbol, Quote>,
    trades: Vec<Trade>,
    top_movers: Vec<Mover>,
    sector_perf: Vec<SectorPerf>,
    list_state: ListState,
    scroll: usize,
}

#[derive(Debug, Clone)]
pub struct Mover {
    pub symbol: Symbol,
    pub price: Decimal,
    pub change: Decimal,
    pub change_pct: Decimal,
    pub volume: Decimal,
}

#[derive(Debug, Clone)]
pub struct SectorPerf {
    pub name: String,
    pub change_pct: Decimal,
}

impl MarketOverviewWidget {
    pub fn new(theme: Theme) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            theme,
            quotes: HashMap::new(),
            trades: Vec::new(),
            top_movers: Vec::new(),
            sector_perf: Vec::new(),
            list_state,
            scroll: 0,
        }
    }

    pub fn selected_symbol(&self) -> Option<Symbol> {
        self.list_state.selected().and_then(|idx| self.top_movers.get(idx).map(|m| m.symbol.clone()))
    }

    pub fn set_focus(&mut self, _focused: bool) {}
    pub fn update_connection_status(&mut self, _status: bt_core::events::ConnectionStatus) {}

    pub fn update_quote(&mut self, quote: Quote) {
        self.quotes.insert(quote.symbol.clone(), quote);
    }

    pub fn update_trade(&mut self, trade: Trade) {
        self.trades.push(trade);
        if self.trades.len() > 1000 {
            self.trades.drain(0..500);
        }
    }

    pub fn set_top_movers(&mut self, movers: Vec<Mover>) {
        self.top_movers = movers;
    }

    pub fn set_sector_perf(&mut self, sectors: Vec<SectorPerf>) {
        self.sector_perf = sectors;
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
        self.list_state.select(Some(self.scroll.min(self.top_movers.len().saturating_sub(1))));
    }

    pub fn scroll_down(&mut self) {
        if self.scroll < self.top_movers.len().saturating_sub(1) {
            self.scroll += 1;
            self.list_state.select(Some(self.scroll));
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" MARKET OVERVIEW ")
            .title_style(self.theme.title_style())
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .style(self.theme.base_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // Top movers
                Constraint::Length(6),  // Sector performance
                Constraint::Min(0),     // Market breadcrumbs
            ])
            .split(inner);

        self.render_movers(frame, chunks[0]);
        self.render_sectors(frame, chunks[1]);
        self.render_breadcrumbs(frame, chunks[2]);
    }

    fn render_movers(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.top_movers.iter().enumerate().map(|(i, m)| {
            let change_style = if m.change_pct >= Decimal::ZERO {
                self.theme.positive_style()
            } else {
                self.theme.negative_style()
            };

            let content = vec![
                Line::from(vec![
                    Span::styled(format!("{:>6} ", i + 1), self.theme.accent_style()),
                    Span::styled(&m.symbol.ticker, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled(format!("${:.2}", m.price), self.theme.base_style()),
                    Span::raw("  "),
                    Span::styled(format!("{:+.2}%", m.change_pct), change_style),
                    Span::raw("  "),
                    Span::styled(format!("Vol: {:.0}", m.volume), self.theme.info_style()),
                ]),
            ];
            ListItem::new(content)
        }).collect();

        let list = List::new(items)
            .block(Block::default()
                .title(" TOP MOVERS ")
                .title_style(self.theme.title_style())
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            )
            .highlight_style(self.theme.highlight_style())
            .highlight_symbol("► ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_sectors(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.sector_perf.iter().map(|s| {
            let style = if s.change_pct >= Decimal::ZERO {
                self.theme.positive_style()
            } else {
                self.theme.negative_style()
            };
            let content = vec![
                Line::from(vec![
                    Span::styled(format!("{:<20}", s.name), self.theme.base_style()),
                    Span::styled(format!("{:+.2}%", s.change_pct), style),
                ]),
            ];
            ListItem::new(content)
        }).collect();

        let list = List::new(items)
            .block(Block::default()
                .title(" SECTOR PERFORMANCE ")
                .title_style(self.theme.title_style())
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            );

        frame.render_widget(list, area);
    }

    fn render_breadcrumbs(&self, frame: &mut Frame, area: Rect) {
        let text = vec![
            Line::from(vec![
                Span::styled("Quotes: ", self.theme.accent_style()),
                Span::raw(self.quotes.len().to_string()),
                Span::raw("  |  "),
                Span::styled("Trades: ", self.theme.accent_style()),
                Span::raw(self.trades.len().to_string()),
                Span::raw("  |  "),
                Span::styled("Movers: ", self.theme.accent_style()),
                Span::raw(self.top_movers.len().to_string()),
            ]),
        ];

        let paragraph = Paragraph::new(text)
            .block(Block::default()
                .title(" SUMMARY ")
                .title_style(self.theme.title_style())
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}

impl MarketOverviewWidget {
    pub fn from_theme(theme: &Theme) -> Self {
        Self::new(theme.clone())
    }
}

use ratatui::layout::{Constraint, Direction, Layout};