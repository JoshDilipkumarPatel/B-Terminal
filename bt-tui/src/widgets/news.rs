use ratatui::{
    layout::Rect,
    style::{Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use bt_core::events::NewsItem;
use crate::theme::Theme;
use chrono::Utc;

pub struct NewsWidget {
    theme: Theme,
    news: Vec<NewsItem>,
    filtered: Vec<usize>,
    filter_symbol: Option<String>,
    filter_keyword: Option<String>,
    list_state: ListState,
    selected_index: usize,
}

impl NewsWidget {
    pub fn new(theme: Theme) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            theme,
            news: Vec::new(),
            filtered: Vec::new(),
            filter_symbol: None,
            filter_keyword: None,
            list_state,
            selected_index: 0,
        }
    }

    pub fn set_news(&mut self, news: Vec<NewsItem>) {
        self.news = news;
        self.apply_filters();
    }

    pub fn add_news(&mut self, item: NewsItem) {
        self.news.insert(0, item);
        if self.news.len() > 500 {
            self.news.truncate(500);
        }
        self.apply_filters();
    }

    pub fn set_filter_symbol(&mut self, symbol: Option<String>) {
        self.filter_symbol = symbol;
        self.apply_filters();
    }

    pub fn set_filter_keyword(&mut self, keyword: Option<String>) {
        self.filter_keyword = keyword;
        self.apply_filters();
    }

    fn apply_filters(&mut self) {
        self.filtered = self.news.iter().enumerate()
            .filter_map(|(i, item)| {
                if let Some(ref sym) = self.filter_symbol {
                    if !item.symbols.iter().any(|s| s.ticker == *sym) {
                        return None;
                    }
                }
                if let Some(ref kw) = self.filter_keyword {
                    let kw = kw.to_lowercase();
                    if !item.headline.to_lowercase().contains(&kw) && !item.summary.as_ref().map(|s| s.to_lowercase().contains(&kw)).unwrap_or(false) {
                        return None;
                    }
                }
                Some(i)
            })
            .collect();
        self.selected_index = 0;
        self.list_state.select(Some(0));
    }

    pub fn scroll_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    pub fn scroll_down(&mut self) {
        if self.selected_index + 1 < self.filtered.len() {
            self.selected_index += 1;
            self.list_state.select(Some(self.selected_index));
        }
    }

    pub fn selected_news(&self) -> Option<&NewsItem> {
        self.filtered.get(self.selected_index).and_then(|&i| self.news.get(i))
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" NEWS ")
            .title_style(self.theme.title_style())
            .borders(Borders::ALL)
            .border_style(self.theme.border_style())
            .style(self.theme.base_style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Filters
                Constraint::Min(0),     // News list
            ])
            .split(inner);

        self.render_filters(frame, chunks[0]);
        self.render_news_list(frame, chunks[1]);
    }

    fn render_filters(&self, frame: &mut Frame, area: Rect) {
        let parts = vec![
            Line::from(vec![
                Span::styled("Symbol: ", self.theme.accent_style()),
                Span::styled(self.filter_symbol.as_deref().unwrap_or("ALL"), self.theme.base_style()),
                Span::raw("  "),
                Span::styled("Keyword: ", self.theme.accent_style()),
                Span::styled(self.filter_keyword.as_deref().unwrap_or("NONE"), self.theme.base_style()),
                Span::raw("  "),
                Span::styled(format!("Total: {} | Filtered: {}", self.news.len(), self.filtered.len()), self.theme.info_style()),
            ]),
        ];

        let paragraph = Paragraph::new(parts)
            .block(Block::default()
                .borders(Borders::BOTTOM)
                .border_style(self.theme.border_style())
            );

        frame.render_widget(paragraph, area);
    }

    fn render_news_list(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.filtered.iter().map(|&i| {
            let item = &self.news[i];
            let time_str = item.timestamp.format("%H:%M:%S").to_string();
            let age = Utc::now() - item.timestamp;
            let age_str = if age.num_hours() > 0 {
                format!("{}h ago", age.num_hours())
            } else {
                format!("{}m ago", age.num_minutes())
            };

            let hl_lower = item.headline.to_lowercase();
            let positive_words = ["beat", "surges", "rally", "growth", "profit", "upgrade"];
            let negative_words = ["crash", "loss", "decline", "downgrade", "bankruptcy", "selloff"];
            let mut badge = Span::styled("🟡 ", Style::default().fg(Color::Yellow));
            for w in positive_words.iter() {
                if hl_lower.contains(w) {
                    badge = Span::styled("🟢 ", Style::default().fg(Color::Green));
                    break;
                }
            }
            if badge.content == "🟡 " {
                for w in negative_words.iter() {
                    if hl_lower.contains(w) {
                        badge = Span::styled("🔴 ", Style::default().fg(Color::Red));
                        break;
                    }
                }
            }

            let content = vec![
                Line::from(vec![
                    Span::styled(format!("[{}] ", time_str), self.theme.info_style()),
                    badge,
                    Span::styled(&item.headline, Style::default().add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled(format!("{} | ", age_str), Style::default().fg(Color::DarkGray)),
                    Span::styled(item.source.clone(), self.theme.accent_style()),
                    Span::raw(" | "),
                    Span::styled(
                        item.symbols.iter().map(|s| s.ticker.clone()).collect::<Vec<_>>().join(","),
                        self.theme.warning_style()
                    ),
                ]),
                Line::from(""),
            ];
            ListItem::new(content)
        }).collect();

        let list = List::new(items)
            .block(Block::default()
                .title(" HEADLINES ")
                .title_style(self.theme.title_style())
                .borders(Borders::ALL)
                .border_style(self.theme.border_style())
            )
            .highlight_style(self.theme.highlight_style())
            .highlight_symbol("► ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    pub fn render_detail(&self, frame: &mut Frame, area: Rect) {
        if let Some(item) = self.selected_news() {
            let text = vec![
                Line::from(vec![
                    Span::styled(&item.headline, Style::default().add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Source: ", self.theme.accent_style()),
                    Span::raw(&item.source),
                    Span::raw("  |  "),
                    Span::styled("Time: ", self.theme.accent_style()),
                    Span::raw(item.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
                ]),
                Line::from(vec![
                    Span::styled("Symbols: ", self.theme.accent_style()),
                    Span::raw(item.symbols.iter().map(|s| s.ticker.clone()).collect::<Vec<_>>().join(", ")),
                ]),
                Line::from(""),
                Line::from(item.summary.as_deref().unwrap_or("No summary available")),
                Line::from(""),
                Line::from(vec![
                    Span::styled("URL: ", self.theme.accent_style()),
                    Span::styled(item.url.as_deref().unwrap_or("N/A"), self.theme.info_style()),
                ]),
            ];

            let paragraph = Paragraph::new(text)
                .block(Block::default()
                    .title(" ARTICLE ")
                    .title_style(self.theme.title_style())
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style())
                )
                .wrap(Wrap { trim: true });

            frame.render_widget(paragraph, area);
        }
    }
}

impl NewsWidget {
    pub fn from_theme(theme: &Theme) -> Self {
        Self::new(theme.clone())
    }
}

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Color;