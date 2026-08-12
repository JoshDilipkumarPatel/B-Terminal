use ratatui::layout::{Constraint, Direction, Layout, Rect};
use std::collections::HashMap;
use crate::theme::{LayoutConfig, PaneConfig};
pub use crate::theme::PaneType;
pub use crate::theme::PaneType as FocusablePane;

#[derive(Debug, Clone)]
pub struct Pane {
    pub config: PaneConfig,
    pub area: Rect,
    pub focused: bool,
}

impl Pane {
    pub fn new(config: PaneConfig) -> Self {
        Self {
            config,
            area: Rect::default(),
            focused: false,
        }
    }

    pub fn title(&self) -> &str {
        &self.config.title
    }

    pub fn pane_type(&self) -> PaneType {
        self.config.type_
    }
}

#[allow(dead_code)]
pub struct LayoutManager {
    panes: Vec<Pane>,
    config: LayoutConfig,
    focus_index: usize,
    ratios: Vec<f32>,
    split_direction: Direction,
}

impl LayoutManager {
    pub fn new(config: &LayoutConfig) -> Self {
        let panes: Vec<Pane> = config.panes.iter().cloned().map(Pane::new).collect();
        let focus_index = config.default_focus.min(panes.len().saturating_sub(1));
        let ratios = config.panes.iter().map(|p| p.ratio).collect();
        let split_direction = if panes.len() <= 2 { Direction::Vertical } else { Direction::Horizontal };

        Self {
            panes,
            config: config.clone(),
            focus_index,
            ratios,
            split_direction,
        }
    }

    pub fn set_focus(&mut self, pane_type: PaneType) {
        if let Some(pos) = self.panes.iter().position(|p| p.pane_type() == pane_type) {
            self.focus_index = pos;
            for (i, p) in self.panes.iter_mut().enumerate() {
                p.focused = i == pos ;
            }
        }
    }

    pub fn load_workspace(&mut self, _path: &str) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn calculate_layout(&self, area: Rect) -> Vec<(PaneType, Rect)> {
        if self.panes.is_empty() {
            return vec![];
        }

        if self.panes.len() == 1 {
            return vec![(self.panes[0].pane_type(), area)];
        }

        // For 4 panes, use 2x2 grid
        if self.panes.len() == 4 {
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(area);

            let top = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(horizontal[0]);

            let bottom = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(horizontal[1]);

            return vec![
                (self.panes[0].pane_type(), top[0]),
                (self.panes[1].pane_type(), top[1]),
                (self.panes[2].pane_type(), bottom[0]),
                (self.panes[3].pane_type(), bottom[1]),
            ];
        }

        // Fallback to horizontal split
        let constraints: Vec<Constraint> = self.ratios.iter()
            .map(|r| Constraint::Percentage((*r * 100.0) as u16))
            .collect();

        let areas = Layout::default()
            .direction(self.split_direction)
            .constraints(constraints)
            .split(area);

        self.panes.iter().enumerate().map(|(i, pane)| {
            let a = if i < areas.len() { areas[i] } else { area };
            (pane.pane_type(), a)
        }).collect()
    }

    pub fn focus_next(&mut self) {
        if !self.panes.is_empty() {
            self.focus_index = (self.focus_index + 1) % self.panes.len();
            self.update_focus();
        }
    }

    pub fn focus_prev(&mut self) {
        if !self.panes.is_empty() {
            self.focus_index = (self.focus_index + self.panes.len() - 1) % self.panes.len();
            self.update_focus();
        }
    }

    pub fn focus_pane(&mut self, index: usize) {
        if index < self.panes.len() {
            self.focus_index = index;
            self.update_focus();
        }
    }

    fn update_focus(&mut self) {
        for (i, pane) in self.panes.iter_mut().enumerate() {
            pane.focused = i == self.focus_index;
        }
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.panes.get(self.focus_index)
    }

    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.focus_index)
    }

    pub fn panes(&self) -> &[Pane] {
        &self.panes
    }

    pub fn panes_mut(&mut self) -> &mut [Pane] {
        &mut self.panes
    }

    pub fn resize_focused(&mut self, direction: ResizeDirection) {
        // For 2x2 grid, adjust ratios
        if self.panes.len() == 4 && self.focus_index < 4 {
            let idx = self.focus_index;
            let step = 0.05;
            match direction {
                ResizeDirection::Up => {
                    if idx >= 2 { // Bottom row
                        self.ratios[idx] += step;
                        self.ratios[idx - 2] -= step;
                    }
                }
                ResizeDirection::Down => {
                    if idx < 2 { // Top row
                        self.ratios[idx] -= step;
                        self.ratios[idx + 2] += step;
                    }
                }
                ResizeDirection::Left => {
                    if idx % 2 == 1 { // Right column
                        self.ratios[idx] += step;
                        self.ratios[idx - 1] -= step;
                    }
                }
                ResizeDirection::Right => {
                    if idx.is_multiple_of(2) { // Left column
                        self.ratios[idx] -= step;
                        self.ratios[idx + 1] += step;
                    }
                }
            }
            // Clamp ratios
            for r in &mut self.ratios {
                *r = r.clamp(0.15, 0.85);
            }
            // Normalize
            let sum: f32 = self.ratios.iter().sum();
            for r in &mut self.ratios {
                *r /= sum;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ResizeDirection {
    Up,
    Down,
    Left,
    Right,
}

pub struct Workspace {
    pub name: String,
    pub layout: LayoutManager,
    pub pane_data: HashMap<String, serde_json::Value>,
}

impl Workspace {
    pub fn new(name: String, config: LayoutConfig) -> Self {
        Self {
            name,
            layout: LayoutManager::new(&config),
            pane_data: HashMap::new(),
        }
    }
}