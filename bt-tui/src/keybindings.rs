use crossterm::event::{KeyCode, KeyModifiers, KeyEvent};
use std::collections::HashMap;
use crate::config::KeybindingConfig;
use crate::app::AppCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    KillSwitch,
    FlattenPositions,
    ToggleCommandMode,
    FocusNextPane,
    FocusPrevPane,
    FocusPane(usize),
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    PageUp,
    PageDown,
    Home,
    End,
    RefreshData,
    ToggleFullscreen,
    SwitchTabNext,
    SwitchTabPrev,
    ExecuteSignal,
    ValidateStrategy,
    RunBacktest,
    DeployStrategy,
    ShowHelp,
    IncreaseDepth,
    DecreaseDepth,
    ToggleVolume,
    ToggleIndicators,
    ZoomIn,
    ZoomOut,
    CrosshairToggle,
}

pub struct KeybindingManager {
    bindings: HashMap<KeyEvent, Action>,
    action_to_keys: HashMap<Action, Vec<KeyEvent>>,
}

impl KeybindingManager {
    pub fn new(config: &KeybindingConfig) -> Self {
        let mut manager = Self {
            bindings: HashMap::new(),
            action_to_keys: HashMap::new(),
        };
        manager.load_from_config(config);
        manager
    }

    fn load_from_config(&mut self, config: &KeybindingConfig) {
        // Clear existing
        self.bindings.clear();
        self.action_to_keys.clear();

        // Load global bindings
        for (key_str, action_str) in &config.bindings {
            if let (Some(key), Some(action)) = (parse_key(key_str), parse_action(action_str)) {
                self.bind(key, action);
            }
        }

        // Load mode-specific bindings
        for bindings in config.mode_specific.values() {
            for (key_str, action_str) in bindings {
                if let (Some(key), Some(action)) = (parse_key(key_str), parse_action(action_str)) {
                    self.bind(key, action);
                }
            }
        }
    }

    pub fn bind(&mut self, key: KeyEvent, action: Action) {
        self.bindings.insert(key, action);
        self.action_to_keys.entry(action).or_default().push(key);
    }

    pub fn unbind(&mut self, key: &KeyEvent) {
        if let Some(action) = self.bindings.remove(key) {
            if let Some(keys) = self.action_to_keys.get_mut(&action) {
                keys.retain(|k| k != key);
            }
        }
    }

    pub fn get_action(&self, key: &KeyEvent) -> Option<Action> {
        self.bindings.get(key).copied()
    }

    pub fn get_keys_for_action(&self, action: Action) -> Vec<KeyEvent> {
        self.action_to_keys.get(&action).cloned().unwrap_or_default()
    }

    pub fn to_app_command(&self, action: Action) -> Option<AppCommand> {
        match action {
            Action::Quit => Some(AppCommand::Quit),
            Action::KillSwitch => Some(AppCommand::KillSwitch),
            Action::FlattenPositions => Some(AppCommand::FlattenPositions),
            Action::ToggleCommandMode => Some(AppCommand::ToggleCommandMode),
            Action::FocusNextPane => None, // Handled directly
            Action::FocusPrevPane => None, // Handled directly
            Action::FocusPane(idx) => Some(AppCommand::FocusPane(match idx {
                0 => crate::layout::FocusablePane::MarketOverview,
                1 => crate::layout::FocusablePane::SecurityDetail,
                2 => crate::layout::FocusablePane::Chart,
                3 => crate::layout::FocusablePane::OrderBook,
                4 => crate::layout::FocusablePane::News,
                5 => crate::layout::FocusablePane::Portfolio,
                6 => crate::layout::FocusablePane::KiAssistant,
                _ => crate::layout::FocusablePane::MarketOverview,
            })),
            Action::ScrollUp => Some(AppCommand::ScrollUp),
            Action::ScrollDown => Some(AppCommand::ScrollDown),
            Action::ScrollLeft => Some(AppCommand::ScrollLeft),
            Action::ScrollRight => Some(AppCommand::ScrollRight),
            Action::PageUp => Some(AppCommand::PageUp),
            Action::PageDown => Some(AppCommand::PageDown),
            Action::Home => Some(AppCommand::Home),
            Action::End => Some(AppCommand::End),
            Action::RefreshData => Some(AppCommand::RefreshData),
            Action::ToggleFullscreen => None, // Handled directly
            Action::SwitchTabNext => None, // Handled by Ki Assistant
            Action::SwitchTabPrev => None, // Handled by Ki Assistant
            Action::ExecuteSignal => None,
            Action::ValidateStrategy => None,
            Action::RunBacktest => None,
            Action::DeployStrategy => None,
            Action::ShowHelp => None,
            Action::IncreaseDepth => None,
            Action::DecreaseDepth => None,
            Action::ToggleVolume => None,
            Action::ToggleIndicators => None,
            Action::ZoomIn => None,
            Action::ZoomOut => None,
            Action::CrosshairToggle => None,
        }
    }

    pub fn get_help_text(&self) -> String {
        let mut sections = Vec::new();

        // Global bindings
        let mut global: Vec<_> = self.bindings.iter()
            .filter(|(k, _)| !self.is_pane_specific(k))
            .collect();
        global.sort_by_key(|(k, _)| format!("{:?}", k));

        let global_text = global.iter()
            .map(|(k, a)| format!("  {:<20} {:?}", format_key(k), a))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("GLOBAL:\n{}", global_text));

        // Group by action for display
        let mut action_groups: HashMap<Action, Vec<KeyEvent>> = HashMap::new();
        for (k, a) in &self.bindings {
            action_groups.entry(*a).or_default().push(*k);
        }

        sections.push("\nACTIONS:".to_string());
        for (action, keys) in action_groups {
            let key_strs = keys.iter().map(format_key).collect::<Vec<_>>().join(", ");
            sections.push(format!("  {:<25} {}", format!("{:?}", action), key_strs));
        }

        sections.join("\n")
    }

    fn is_pane_specific(&self, _key: &KeyEvent) -> bool {
        // Simplified - in reality would check context
        false
    }
}

fn parse_key(key_str: &str) -> Option<KeyEvent> {
    let key_str = key_str.trim();
    let mut modifiers = KeyModifiers::NONE;
    let mut remaining = key_str;

    // Parse modifiers
    while let Some(stripped) = remaining.strip_prefix("Ctrl+") {
        modifiers |= KeyModifiers::CONTROL;
        remaining = stripped;
    }
    while let Some(stripped) = remaining.strip_prefix("Alt+") {
        modifiers |= KeyModifiers::ALT;
        remaining = stripped;
    }
    while let Some(stripped) = remaining.strip_prefix("Shift+") {
        modifiers |= KeyModifiers::SHIFT;
        remaining = stripped;
    }
    while let Some(stripped) = remaining.strip_prefix("Meta+") {
        modifiers |= KeyModifiers::META;
        remaining = stripped;
    }

    // Parse key code
    let code = match remaining.to_lowercase().as_str() {
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "up" | "up_arrow" => KeyCode::Up,
        "down" | "down_arrow" => KeyCode::Down,
        "left" | "left_arrow" => KeyCode::Left,
        "right" | "right_arrow" => KeyCode::Right,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "insert" | "ins" => KeyCode::Insert,
        "f1" => KeyCode::F(1),
        "f2" => KeyCode::F(2),
        "f3" => KeyCode::F(3),
        "f4" => KeyCode::F(4),
        "f5" => KeyCode::F(5),
        "f6" => KeyCode::F(6),
        "f7" => KeyCode::F(7),
        "f8" => KeyCode::F(8),
        "f9" => KeyCode::F(9),
        "f10" => KeyCode::F(10),
        "f11" => KeyCode::F(11),
        "f12" => KeyCode::F(12),
        "space" | " " => KeyCode::Char(' '),
        c if c.len() == 1 => {
            let ch = c.chars().next().unwrap();
            if ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch.is_ascii_punctuation() {
                KeyCode::Char(ch)
            } else {
                return None;
            }
        }
        _ => return None,
    };

    Some(KeyEvent::new(code, modifiers))
}

fn parse_action(action_str: &str) -> Option<Action> {
    match action_str.to_lowercase().as_str() {
        "quit" => Some(Action::Quit),
        "kill_switch" | "killswitch" => Some(Action::KillSwitch),
        "flatten_positions" | "flatten" => Some(Action::FlattenPositions),
        "toggle_command_mode" | "command_mode" => Some(Action::ToggleCommandMode),
        "focus_next_pane" | "next_pane" => Some(Action::FocusNextPane),
        "focus_prev_pane" | "prev_pane" => Some(Action::FocusPrevPane),
        "scroll_up" | "up" => Some(Action::ScrollUp),
        "scroll_down" | "down" => Some(Action::ScrollDown),
        "scroll_left" | "left" => Some(Action::ScrollLeft),
        "scroll_right" | "right" => Some(Action::ScrollRight),
        "page_up" | "pageup" => Some(Action::PageUp),
        "page_down" | "pagedown" => Some(Action::PageDown),
        "home" => Some(Action::Home),
        "end" => Some(Action::End),
        "refresh" | "refresh_data" => Some(Action::RefreshData),
        "toggle_fullscreen" | "fullscreen" => Some(Action::ToggleFullscreen),
        "switch_tab_next" | "next_tab" => Some(Action::SwitchTabNext),
        "switch_tab_prev" | "prev_tab" => Some(Action::SwitchTabPrev),
        "execute_signal" | "execute" => Some(Action::ExecuteSignal),
        "validate_strategy" | "validate" => Some(Action::ValidateStrategy),
        "run_backtest" | "backtest" => Some(Action::RunBacktest),
        "deploy_strategy" | "deploy" => Some(Action::DeployStrategy),
        "show_help" | "help" => Some(Action::ShowHelp),
        "increase_depth" | "depth+" => Some(Action::IncreaseDepth),
        "decrease_depth" | "depth-" => Some(Action::DecreaseDepth),
        "toggle_volume" | "volume" => Some(Action::ToggleVolume),
        "toggle_indicators" | "indicators" => Some(Action::ToggleIndicators),
        "zoom_in" | "zoom+" => Some(Action::ZoomIn),
        "zoom_out" | "zoom-" => Some(Action::ZoomOut),
        "crosshair_toggle" | "crosshair" => Some(Action::CrosshairToggle),
        _ => None,
    }
}

fn format_key(key: &KeyEvent) -> String {
    let mut parts = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }
    if key.modifiers.contains(KeyModifiers::META) {
        parts.push("Meta");
    }

    let key_str = match key.code {
        KeyCode::Enter => "Enter",
        KeyCode::Esc => "Esc",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        KeyCode::Up => "Up",
        KeyCode::Down => "Down",
        KeyCode::Left => "Left",
        KeyCode::Right => "Right",
        KeyCode::PageUp => "PgUp",
        KeyCode::PageDown => "PgDn",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::Insert => "Ins",
        KeyCode::F(n) => return format!("F{}", n),
        KeyCode::Char(c) => {
            if parts.is_empty() {
                return c.to_string();
            }
            return parts.join("+") + "+" + &c.to_string();
        },
        KeyCode::Null => "Null",
        _ => "Other",
    };

    if parts.is_empty() {
        key_str.to_string()
    } else {
        parts.join("+") + "+" + key_str
    }
}

impl Default for KeybindingManager {
    fn default() -> Self {
        let config = KeybindingConfig::default();
        Self::new(&config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_ctrl_c() {
        let key = parse_key("Ctrl+C").unwrap();
        assert_eq!(key.code, KeyCode::Char('c'));
        assert!(key.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn test_parse_key_f1() {
        let key = parse_key("F1").unwrap();
        assert_eq!(key.code, KeyCode::F(1));
    }

    #[test]
    fn test_parse_key_shift_tab() {
        let key = parse_key("Shift+Tab").unwrap();
        assert_eq!(key.code, KeyCode::Tab);
        assert!(key.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn test_parse_action() {
        assert_eq!(parse_action("quit"), Some(Action::Quit));
        assert_eq!(parse_action("kill_switch"), Some(Action::KillSwitch));
        assert_eq!(parse_action("scroll_up"), Some(Action::ScrollUp));
        assert_eq!(parse_action("invalid"), None);
    }

    #[test]
    fn test_format_key() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(format_key(&key), "Ctrl+c");

        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert_eq!(format_key(&key), "F1");

        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
        assert_eq!(format_key(&key), "Shift+Up");
    }
}