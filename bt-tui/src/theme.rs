use ratatui::style::{Color, Style, Modifier};

pub use bt_core::config::{ThemeConfig, ColorConfig, LayoutConfig, PaneConfig, PaneType, KeybindingConfig};

#[derive(Clone)]
pub struct Theme {
    pub config: ColorConfig,
    pub bg: Color,
    pub fg: Color,
    pub border: Color,
    pub title: Color,
    pub highlight: Color,
    pub positive: Color,
    pub negative: Color,
    pub warning: Color,
    pub info: Color,
    pub accent: Color,
}

impl Theme {
    pub fn new(config: ColorConfig) -> Self {
        Self {
            bg: parse_color(&config.bg),
            fg: parse_color(&config.fg),
            border: parse_color(&config.border),
            title: parse_color(&config.title),
            highlight: parse_color(&config.highlight),
            positive: parse_color(&config.positive),
            negative: parse_color(&config.negative),
            warning: parse_color(&config.warning),
            info: parse_color(&config.info),
            accent: parse_color(&config.accent),
            config,
        }
    }

    pub fn from_config(config: &ThemeConfig) -> Self {
        Self::new(config.colors.clone())
    }

    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "bloomberg" => {
                let mut config = ColorConfig::default();
                config.bg = "#000000".to_string();
                config.fg = "#FFB400".to_string();
                Self::new(config)
            }
            "dark" => {
                let mut config = ColorConfig::default();
                config.bg = "#121212".to_string();
                config.fg = "#FFFFFF".to_string();
                Self::new(config)
            }
            "light" => {
                let mut config = ColorConfig::default();
                config.bg = "#FFFFFF".to_string();
                config.fg = "#000000".to_string();
                Self::new(config)
            }
            _ => Self::default(),
        }
    }

    pub fn default() -> Self {
        Self::new(ColorConfig::default())
    }

    pub fn base_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border).bg(self.bg)
    }

    pub fn title_style(&self) -> Style {
        Style::default().fg(self.title).bg(self.bg).add_modifier(Modifier::BOLD)
    }

    pub fn highlight_style(&self) -> Style {
        Style::default().fg(self.highlight).bg(self.bg).add_modifier(Modifier::BOLD)
    }

    pub fn positive_style(&self) -> Style {
        Style::default().fg(self.positive).bg(self.bg)
    }

    pub fn negative_style(&self) -> Style {
        Style::default().fg(self.negative).bg(self.bg)
    }

    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning).bg(self.bg)
    }

    pub fn info_style(&self) -> Style {
        Style::default().fg(self.info).bg(self.bg)
    }

    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.accent).bg(self.bg).add_modifier(Modifier::BOLD)
    }

    pub fn pnl_style(&self, value: rust_decimal::Decimal) -> Style {
        if value > rust_decimal::Decimal::ZERO {
            self.positive_style()
        } else if value < rust_decimal::Decimal::ZERO {
            self.negative_style()
        } else {
            self.base_style()
        }
    }
}

fn parse_color(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        if let Ok(r) = u8::from_str_radix(&s[0..2], 16) {
            if let Ok(g) = u8::from_str_radix(&s[2..4], 16) {
                if let Ok(b) = u8::from_str_radix(&s[4..6], 16) {
                    return Color::Rgb(r, g, b);
                }
            }
        }
    }
    // Try named colors
    match s.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::DarkGray,
        _ => Color::Rgb(255, 180, 0), // Default to Bloomberg amber
    }
}
