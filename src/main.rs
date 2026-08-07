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
    wifi::{
        backend_unavailable_message, get_connected_ssid, get_wifi_networks, is_backend_available,
        scan_networks,
    },
};

#[cfg(target_os = "linux")]
use crate::wifi::{BackendChoice, initialize_backend};

/// A lightweight, keyboard-driven TUI for managing Wi-Fi connections.
#[derive(Parser, Debug)]
#[command(
    name = "wifui",
    author = "Soham Waghmare",
    about = "A lightweight, keyboard-driven TUI for managing Wi-Fi connections.\n\nAuthor: Soham Waghmare",
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

    /// Select the Linux Wi-Fi backend
    #[cfg(target_os = "linux")]
    #[arg(long, value_enum, default_value_t = BackendChoice::Auto)]
    backend: BackendChoice,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut state = AppState::new(Vec::new(), args.show_keys, args.ascii);

    #[cfg(target_os = "linux")]
    let backend_init_error = tokio::task::spawn_blocking(move || initialize_backend(args.backend))
        .await
        .map(|result| result.err())
        .unwrap_or_else(|error| Some(crate::error::WifiError::Internal(error.to_string())));
    #[cfg(not(target_os = "linux"))]
    let backend_init_error: Option<crate::error::WifiError> = None;

    if is_backend_available() {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        state.refresh.is_refreshing_networks = true;
        state.refresh.network_update_rx = Some(rx);
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(|| {
                scan_networks()?;
                let networks = get_wifi_networks()?;
                let connected = get_connected_ssid()?;
                Ok((networks, connected))
            })
            .await;
            let result = match result {
                Ok(inner) => inner,
                Err(e) => Err(color_eyre::eyre::eyre!(e.to_string())),
            };
            let _ = tx.send(result).await;
        });
    } else {
        state.refresh.is_initial_loading = false;
        state.ui.error_message = Some(
            backend_init_error
                .map(|error| error.to_string())
                .unwrap_or_else(backend_unavailable_message),
        );
    }

    color_eyre::install()?;
    let terminal = ratatui::init();
    enable_raw_mode()?;
    let result = run(terminal, &mut state).await;
    disable_raw_mode()?;

    ratatui::restore();
    result
}
