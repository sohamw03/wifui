//! Event handling module for WifUI
//!
//! This module handles keyboard input, connection events, and the main event loop.

mod handlers;

use crate::{
    app::AppState,
    config,
    ui::render,
    wifi::{ConnectionEvent, get_connected_ssid, get_wifi_networks},
};
use color_eyre::eyre::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, KeyModifiers},
};
use handlers::{
    handle_main_view, handle_manual_add_popup, handle_password_popup, handle_search_mode,
};
use ratatui::DefaultTerminal;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub async fn run(mut terminal: DefaultTerminal, state: &mut AppState) -> Result<()> {
    // Set cursor style to blinking block
    crossterm::execute!(std::io::stdout(), SetCursorStyle::BlinkingBlock)?;

    loop {
        terminal.draw(|frame| render(frame, state))?;

        // Check for connection result
        if let Some(rx) = &mut state.connection.connection_result_rx {
            if let Ok(result) = rx.try_recv() {
                state.connection.connection_result_rx = None;
                if let Err(e) = result {
                    state.connection.is_connecting = false;
                    state.connection.target_ssid = None;
                    state.connection.connection_start_time = None;
                    state.ui.error_message = Some(format!("Failed to connect: {}", e));
                } else {
                    // Connection initiated successfully, now wait for it to actually connect
                    state.refresh.refresh_burst = config::CONNECTION_REFRESH_BURST;
                }
                // Trigger background refresh instead of blocking
                state.refresh.is_refreshing_networks = true;
                let (tx, rx) = mpsc::channel(1);
                state.refresh.network_update_rx = Some(rx);
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(|| {
                        let networks = get_wifi_networks()?;
                        let connected = get_connected_ssid()?;
                        Ok((networks, connected))
                    })
                    .await
                    .unwrap();
                    let _ = tx.send(result).await;
                });
            }
        }

        // Check for network updates
        if let Some(rx) = &mut state.refresh.network_update_rx {
            if let Ok(result) = rx.try_recv() {
                if let Ok((new_list, connected_ssid)) = result {
                    let connection_changed = state.network.connected_ssid != connected_ssid;

                    // Try to preserve selection
                    let selected_ssid = state
                        .ui
                        .l_state
                        .selected()
                        .and_then(|i| state.network.wifi_list.get(i))
                        .map(|w| w.ssid.clone());

                    state.network.wifi_list = new_list;
                    state.network.connected_ssid = connected_ssid;
                    state.update_filtered_list();

                    if connection_changed && state.network.connected_ssid.is_some() {
                        state.ui.l_state.select(Some(0));
                    } else if let Some(ssid) = selected_ssid {
                        if let Some(pos) = state
                            .network
                            .filtered_wifi_list
                            .iter()
                            .position(|w| w.ssid == ssid)
                        {
                            state.ui.l_state.select(Some(pos));
                        } else {
                            state.ui.l_state.select(Some(0));
                        }
                    }
                }
                state.refresh.is_refreshing_networks = false;
                state.refresh.network_update_rx = None;
                state.refresh.last_refresh = Instant::now();
            }
        }

        // Check for connection events
        if let Some(rx) = &mut state.connection.connection_event_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    ConnectionEvent::Connected(ssid) => {
                        if let Some(target) = &state.connection.target_ssid {
                            if *target == ssid {
                                state.connection.is_connecting = false;
                                state.connection.target_ssid = None;
                                state.connection.connection_start_time = None;
                                state.refresh.refresh_burst = config::DISCONNECT_REFRESH_BURST;
                            }
                        }
                    }
                    ConnectionEvent::Disconnected(_) => {
                        state.refresh.refresh_burst = config::DISCONNECT_REFRESH_BURST;
                    }
                    ConnectionEvent::Failed {
                        ssid, reason_str, ..
                    } => {
                        if let Some(target) = &state.connection.target_ssid {
                            if *target == ssid {
                                state.connection.is_connecting = false;
                                state.connection.target_ssid = None;
                                state.connection.connection_start_time = None;
                                state.ui.error_message =
                                    Some(format!("Connection failed: {}", reason_str));

                                // Forget the failed profile to prevent stale saved entries
                                let ssid_clone = ssid.clone();
                                tokio::spawn(async move {
                                    let _ = tokio::task::spawn_blocking(move || {
                                        crate::wifi::forget_network(&ssid_clone)
                                    })
                                    .await;
                                });
                            }
                        }
                    }
                }
            }
        }

        // Check if connected to target SSID
        if state.connection.is_connecting {
            state.ui.loading_frame = state.ui.loading_frame.wrapping_add(1);

            if let Some(target) = &state.connection.target_ssid {
                if let Some(connected) = &state.network.connected_ssid {
                    if connected == target {
                        state.connection.is_connecting = false;
                        state.connection.target_ssid = None;
                        state.connection.connection_start_time = None;
                    }
                }

                // Check for timeout
                if let Some(start_time) = state.connection.connection_start_time {
                    if start_time.elapsed() > Duration::from_secs(config::CONNECTION_TIMEOUT_SECS) {
                        state.connection.is_connecting = false;
                        state.connection.target_ssid = None;
                        state.connection.connection_start_time = None;
                        state.ui.error_message =
                            Some("Connection timed out (No response from OS)".to_string());
                    }
                }
            } else {
                // If no target SSID is set but is_connecting is true, check connection result
                if state.connection.connection_result_rx.is_none() {
                    state.connection.is_connecting = false;
                }
            }
        }

        // Auto-refresh logic
        let refresh_interval = if state.refresh.refresh_burst > 0 {
            Duration::from_secs(config::BURST_REFRESH_INTERVAL_SECS)
        } else if state.ui.is_searching || !state.inputs.search_input.value.is_empty() {
            Duration::from_secs(config::SEARCHING_REFRESH_INTERVAL_SECS)
        } else {
            Duration::from_secs(config::AUTO_REFRESH_INTERVAL_SECS)
        };

        if !state.refresh.is_refreshing_networks
            && !state.ui.show_manual_add_popup
            && !state.ui.show_password_popup
            && state.refresh.last_refresh.elapsed() >= refresh_interval
            && state.refresh.last_interaction.elapsed()
                >= Duration::from_secs(config::INTERACTION_COOLDOWN_SECS)
        {
            if state.refresh.refresh_burst > 0 {
                state.refresh.refresh_burst -= 1;
            }
            state.refresh.is_refreshing_networks = true;
            let (tx, rx) = mpsc::channel(1);
            state.refresh.network_update_rx = Some(rx);

            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(|| {
                    let networks = get_wifi_networks()?;
                    let connected = get_connected_ssid()?;
                    Ok((networks, connected))
                })
                .await
                .unwrap();
                let _ = tx.send(result).await;
            });
        }

        if event::poll(Duration::from_millis(config::EVENT_POLL_MS))? {
            if let Event::Key(key) = event::read()? {
                state.refresh.last_interaction = Instant::now();
                if key.kind == event::KeyEventKind::Press {
                    // Log key press if enabled
                    if state.ui.show_key_logger {
                        let mut key_str = String::new();
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            key_str.push_str("Ctrl+");
                        }
                        if key.modifiers.contains(KeyModifiers::ALT) {
                            key_str.push_str("Alt+");
                        }
                        if key.modifiers.contains(KeyModifiers::SHIFT)
                            && !matches!(key.code, event::KeyCode::Char(_))
                        {
                            key_str.push_str("Shift+");
                        }

                        let code_str = match key.code {
                            event::KeyCode::Char(c) => c.to_string(),
                            event::KeyCode::Enter => "Enter".to_string(),
                            event::KeyCode::Backspace => "Backspace".to_string(),
                            event::KeyCode::Left => "Left".to_string(),
                            event::KeyCode::Right => "Right".to_string(),
                            event::KeyCode::Up => "Up".to_string(),
                            event::KeyCode::Down => "Down".to_string(),
                            event::KeyCode::Tab => "Tab".to_string(),
                            event::KeyCode::Delete => "Delete".to_string(),
                            event::KeyCode::Home => "Home".to_string(),
                            event::KeyCode::End => "End".to_string(),
                            event::KeyCode::PageUp => "PageUp".to_string(),
                            event::KeyCode::PageDown => "PageDown".to_string(),
                            event::KeyCode::Esc => "Esc".to_string(),
                            event::KeyCode::F(n) => format!("F{}", n),
                            _ => format!("{:?}", key.code),
                        };
                        key_str.push_str(&code_str);
                        state.ui.last_key_press = Some((key_str, Instant::now()));
                    }

                    // Clear error message on any key press
                    if state.ui.error_message.is_some() {
                        state.ui.error_message = None;
                    }

                    // Global shortcuts
                    if key.code == event::KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        break;
                    }

                    // Route to appropriate handler
                    let should_quit = if state.ui.show_manual_add_popup {
                        handle_manual_add_popup(key, state)
                    } else if state.ui.show_password_popup {
                        handle_password_popup(key, state)
                    } else if state.ui.is_searching {
                        handle_search_mode(key, state)
                    } else {
                        handle_main_view(key, state)
                    };

                    if should_quit {
                        break;
                    }
                }
            }
        } else {
            // Update loading animation frame if connecting
            if state.connection.is_connecting {
                state.ui.loading_frame = (state.ui.loading_frame + 1) % 10;
            }
        }
    }
    Ok(())
}
