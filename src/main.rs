mod app;
mod config;
mod error;
mod event;
mod input;
mod theme;
mod ui;
mod wifi;

use clap::Parser;
use color_eyre::eyre::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::{
    app::AppState,
    event::run,
    wifi::{get_connected_ssid, get_wifi_networks, scan_networks},
};

/// A lightweight, keyboard-driven TUI for managing Wi-Fi connections on Windows
#[derive(Parser, Debug)]
#[command(
    name = "wifui",
    author = "Soham Waghmare",
    about = "A lightweight, keyboard-driven TUI for managing Wi-Fi connections on Windows.\n\nAuthor: Soham Waghmare",
    long_about = None,
    version = env!("CARGO_PKG_VERSION"),
    disable_version_flag = true
)]
struct Args {
    /// Print version information
    #[arg(short = 'v', long = "version", action = clap::ArgAction::Version)]
    version: (),

    /// Use ASCII icons (no Nerd Fonts required)
    #[arg(long)]
    ascii: bool,

    /// Show key logger for debugging
    #[arg(long = "show-keys")]
    show_keys: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut state = AppState::new(Vec::new(), args.show_keys, args.ascii);

    // Trigger a scan on startup in background
    state.refresh.is_refreshing_networks = true;
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    state.refresh.network_update_rx = Some(rx);

    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(|| {
            let _ = scan_networks();
            let networks = get_wifi_networks().unwrap_or_default();
            let connected = get_connected_ssid().unwrap_or(None);
            Ok((networks, connected))
        })
        .await
        .unwrap_or_else(|_| Ok((Vec::new(), None)));

        let _ = tx.send(result).await;
    });

    color_eyre::install()?;
    let terminal = ratatui::init();
    enable_raw_mode()?;
    let result = run(terminal, &mut state).await;
    disable_raw_mode()?;

    ratatui::restore();
    result
}
