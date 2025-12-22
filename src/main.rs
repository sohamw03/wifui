mod app;
mod event;
mod theme;
mod ui;
mod wifi;

use color_eyre::eyre::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::{app::AppState, event::run, wifi::{get_wifi_networks, scan_networks}};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.contains(&"-v".to_string()) || args.contains(&"--version".to_string()) {
        println!("wifui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let show_key_logger = args.contains(&"--show-keys".to_string());

    // Trigger a scan on startup to ensure the network list is up-to-date
    let _ = scan_networks();

    let wifi_list = get_wifi_networks()?;
    let mut state = AppState::new(wifi_list, show_key_logger);

    color_eyre::install()?;
    let terminal = ratatui::init();
    enable_raw_mode()?;
    let result = run(terminal, &mut state).await;
    disable_raw_mode()?;

    ratatui::restore();
    result
}
