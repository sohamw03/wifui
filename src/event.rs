use crate::{
    app::AppState,
    ui::render,
    wifi::{connect_with_password, get_saved_profiles, connect_profile, get_wifi_networks, forget_network, get_connected_ssid, disconnect, connect_open, set_auto_connect, scan_networks},
};
use color_eyre::eyre::Result;
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub async fn run(mut terminal: DefaultTerminal, state: &mut AppState) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, state))?;

        // Check for connection result
        if let Some(rx) = &mut state.connection_result_rx
            && let Ok(result) = rx.try_recv() {
                state.connection_result_rx = None;
                if let Err(e) = result {
                    state.is_connecting = false;
                    state.target_ssid = None;
                    state.connection_start_time = None;
                    state.error_message = Some(format!("Failed to connect: {}", e));
                } else {
                    // Connection initiated successfully, now wait for it to actually connect
                    state.refresh_burst = 15; // Increase burst to check status frequently
                }
                // Trigger background refresh instead of blocking
                state.is_refreshing_networks = true;
                let (tx, rx) = mpsc::channel(1);
                state.network_update_rx = Some(rx);
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(|| {
                        let networks = get_wifi_networks()?;
                        let connected = get_connected_ssid()?;
                        Ok((networks, connected))
                    }).await.unwrap();
                    let _ = tx.send(result).await;
                });
            }

        // Check for network updates
        if let Some(rx) = &mut state.network_update_rx
            && let Ok(result) = rx.try_recv() {
                if let Ok((new_list, connected_ssid)) = result {
                    // Try to preserve selection
                    let selected_ssid = state.l_state.selected().and_then(|i| state.wifi_list.get(i)).map(|w| w.ssid.clone());

                    state.wifi_list = new_list;
                    state.connected_ssid = connected_ssid;

                    if let Some(ssid) = selected_ssid {
                        if let Some(pos) = state.wifi_list.iter().position(|w| w.ssid == ssid) {
                            state.l_state.select(Some(pos));
                        } else {
                            state.l_state.select(Some(0));
                        }
                    }
                }
                state.is_refreshing_networks = false;
                state.network_update_rx = None;
                state.last_refresh = Instant::now();
            }

        // Check if connected to target SSID
        if state.is_connecting {
            state.loading_frame = state.loading_frame.wrapping_add(1);

            if let Some(target) = &state.target_ssid {
                if let Some(connected) = &state.connected_ssid
                    && connected == target {
                        state.is_connecting = false;
                        state.target_ssid = None;
                        state.connection_start_time = None;
                    }

                // Check for timeout (e.g. 20 seconds)
                if let Some(start_time) = state.connection_start_time
                    && start_time.elapsed() > Duration::from_secs(20) {
                        state.is_connecting = false;
                        state.target_ssid = None;
                        state.connection_start_time = None;
                        state.error_message = Some("Connection timed out".to_string());
                    }
            } else {
                // If no target SSID is set but is_connecting is true, it might be a disconnect or forget operation
                // In those cases we should probably just turn off is_connecting when the operation completes
                // But for now, let's assume is_connecting implies we are waiting for a connection unless connection_result_rx is active
                if state.connection_result_rx.is_none() {
                     state.is_connecting = false;
                }
            }
        }

        // Auto-refresh logic
        let refresh_interval = if state.refresh_burst > 0 {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(5)
        };

        if !state.is_refreshing_networks
            && state.last_refresh.elapsed() >= refresh_interval
            && state.last_interaction.elapsed() >= Duration::from_secs(1)
        {
            if state.refresh_burst > 0 {
                state.refresh_burst -= 1;
            }
            state.is_refreshing_networks = true;
            let (tx, rx) = mpsc::channel(1);
            state.network_update_rx = Some(rx);

            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(|| {
                    let networks = get_wifi_networks()?;
                    let connected = get_connected_ssid()?;
                    Ok((networks, connected))
                }).await.unwrap();
                let _ = tx.send(result).await;
            });
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                state.last_interaction = Instant::now();
                if key.kind == event::KeyEventKind::Press {
                    // Clear error message on any key press
                    if state.error_message.is_some() {
                        state.error_message = None;
                    }

                    if state.show_password_popup {
                        match key.code {
                            event::KeyCode::Enter => {
                                if let Some(ssid) = state.connecting_to_ssid.take() {
                                    state.is_connecting = true;
                                    state.target_ssid = Some(ssid.clone());
                                    state.connection_start_time = Some(Instant::now());
                                    let password = state.password_input.clone();
                                    let (tx, rx) = mpsc::channel(1);
                                    state.connection_result_rx = Some(rx);

                                    let wifi_info = state.wifi_list.iter().find(|w| w.ssid == ssid).cloned();

                                    tokio::spawn(async move {
                                        if get_connected_ssid().unwrap_or(None).is_some() {
                                            let _ = tokio::task::spawn_blocking(disconnect).await;
                                        }
                                        let result = tokio::task::spawn_blocking(move || {
                                            if let Some(info) = wifi_info {
                                                connect_with_password(&ssid, &password, &info.authentication, &info.encryption)
                                            } else {
                                                connect_with_password(&ssid, &password, "WPA2-PSK", "AES")
                                            }
                                        }).await.unwrap();
                                        let _ = tx.send(result).await;
                                    });
                                }
                                state.show_password_popup = false;
                                state.password_input.clear();
                            }
                            event::KeyCode::Char(c) => {
                                state.password_input.push(c);
                            }
                            event::KeyCode::Backspace => {
                                state.password_input.pop();
                            }
                            event::KeyCode::Esc => {
                                state.show_password_popup = false;
                                state.password_input.clear();
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            event::KeyCode::Esc | event::KeyCode::Char('q') => break,
                            event::KeyCode::Char('[') if key.modifiers.contains(event::KeyModifiers::CONTROL) => break,
                            event::KeyCode::Char('j') | event::KeyCode::Down => state.next(),
                            event::KeyCode::Char('k') | event::KeyCode::Up => state.previous(),
                            event::KeyCode::Enter => {
                                if let Some(selected) = state.l_state.selected()
                                    && let Some(wifi) = state.wifi_list.get(selected).cloned() {
                                        let is_connected = if let Some(connected_ssid) = &state.connected_ssid {
                                            wifi.ssid == *connected_ssid
                                        } else {
                                            false
                                        };

                                        if is_connected {
                                            let (tx, rx) = mpsc::channel(1);
                                            state.connection_result_rx = Some(rx);
                                            tokio::spawn(async move {
                                                let result = tokio::task::spawn_blocking(disconnect).await.unwrap();
                                                let _ = tx.send(result).await;
                                            });
                                        } else if wifi.authentication != "Open" {
                                            // Check if profile exists
                                            let saved_profiles = get_saved_profiles().unwrap_or_default();
                                            if saved_profiles.contains(&wifi.ssid) {
                                                state.is_connecting = true;
                                                state.target_ssid = Some(wifi.ssid.clone());
                                                state.connection_start_time = Some(Instant::now());
                                                let ssid = wifi.ssid.clone();
                                                let (tx, rx) = mpsc::channel(1);
                                                state.connection_result_rx = Some(rx);

                                                tokio::spawn(async move {
                                                    if get_connected_ssid().unwrap_or(None).is_some() {
                                                        let _ = tokio::task::spawn_blocking(disconnect).await;
                                                    }
                                                    let result = tokio::task::spawn_blocking(move || connect_profile(&ssid)).await.unwrap();
                                                    let _ = tx.send(result).await;
                                                });
                                            } else {
                                                state.show_password_popup = true;
                                                state.connecting_to_ssid = Some(wifi.ssid.clone());
                                            }
                                        } else {
                                            state.is_connecting = true;
                                            state.target_ssid = Some(wifi.ssid.clone());
                                            state.connection_start_time = Some(Instant::now());
                                            let ssid = wifi.ssid.clone();
                                            let (tx, rx) = mpsc::channel(1);
                                            state.connection_result_rx = Some(rx);

                                            tokio::spawn(async move {
                                                if get_connected_ssid().unwrap_or(None).is_some() {
                                                    let _ = tokio::task::spawn_blocking(disconnect).await;
                                                }
                                                let result = tokio::task::spawn_blocking(move || connect_open(&ssid)).await.unwrap();
                                                let _ = tx.send(result).await;
                                            });
                                        }
                                    }
                            }
                            event::KeyCode::Char('r') => {
                                state.is_refreshing_networks = true;
                                let (tx, rx) = mpsc::channel(1);
                                state.network_update_rx = Some(rx);

                                tokio::spawn(async move {
                                    let result = tokio::task::spawn_blocking(|| {
                                        let _ = scan_networks();
                                        std::thread::sleep(Duration::from_secs(2));
                                        let networks = get_wifi_networks()?;
                                        let connected = get_connected_ssid()?;
                                        Ok((networks, connected))
                                    }).await.unwrap();
                                    let _ = tx.send(result).await;
                                });
                            }
                            event::KeyCode::Char('a') => {
                                if let Some(selected) = state.l_state.selected()
                                    && let Some(wifi) = state.wifi_list.get(selected).cloned()
                                        && wifi.is_saved {
                                            let ssid = wifi.ssid.clone();
                                            let auto_connect = !wifi.auto_connect;
                                            let (tx, rx) = mpsc::channel(1);
                                            state.connection_result_rx = Some(rx);

                                            tokio::spawn(async move {
                                                let result = tokio::task::spawn_blocking(move || {
                                                    set_auto_connect(&ssid, auto_connect)
                                                }).await.unwrap();
                                                let _ = tx.send(result).await;
                                            });
                                        }
                            }
                            event::KeyCode::Char('f') => {
                                if let Some(selected) = state.l_state.selected()
                                    && let Some(wifi) = state.wifi_list.get(selected).cloned() {
                                        let ssid = wifi.ssid.clone();
                                        let (tx, rx) = mpsc::channel(1);
                                        state.connection_result_rx = Some(rx);

                                        tokio::spawn(async move {
                                            let result = tokio::task::spawn_blocking(move || {
                                                forget_network(&ssid)
                                            }).await.unwrap();
                                            let _ = tx.send(result).await;
                                        });
                                    }
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else {
            // Update loading animation frame if connecting
            if state.is_connecting {
                state.loading_frame = (state.loading_frame + 1) % 10;
            }
        }
    }
    Ok(())
}