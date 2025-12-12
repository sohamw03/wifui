mod app;
mod event;
mod ui;
mod wifi;

use color_eyre::eyre::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui;

use crate::{app::AppState, event::run, wifi::{get_wifi_networks, scan_networks}};

#[tokio::main]
async fn main() -> Result<()> {
    // Trigger a scan on startup to ensure the network list is up-to-date
    let _ = scan_networks();

    let wifi_list = get_wifi_networks()?;
    let mut state = AppState::new(wifi_list);

    color_eyre::install()?;
    let terminal = ratatui::init();
    enable_raw_mode()?;
    let result = run(terminal, &mut state).await;
    disable_raw_mode()?;

    ratatui::restore();
    result
}
