//! B-Terminal TUI - Bloomberg Terminal Recreation with Algorithmic Trading

pub mod app;
pub use app::App;
pub mod command;
pub mod keybindings;
pub mod layout;
pub mod theme;
pub mod widgets;
pub mod config {
    pub use crate::theme::{KeybindingConfig, LayoutConfig, PaneConfig};
}

use bt_core::config::Config;
use anyhow::Result;

/// Initialize the TUI application
pub async fn run(config: Config) -> Result<()> {
    let mut app = crate::app::App::new(config).await?;
    app.run().await
}

#[cfg(test)]
mod tests {
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_tui_module_compiles() {
        assert!(true);
    }
}