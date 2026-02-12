use crate::app::AppState;
use crate::config;
use crate::error::WifiError;
use crate::wifi::{disconnect, get_connected_ssid, get_wifi_networks};
use color_eyre::eyre::eyre;
use crossterm::event::{self, KeyEvent, KeyModifiers};
use secrecy::SecretString;
use std::time::Instant;
use tokio::sync::mpsc;

/// Handle keyboard events for the QR code popup
pub fn handle_qr_popup(key: KeyEvent, state: &mut AppState) -> bool {
    match key.code {
        event::KeyCode::Esc | event::KeyCode::Char('q') | event::KeyCode::Enter => {
            state.ui.show_qr_popup = false;
            state.ui.qr_code_lines.clear();
        }
        event::KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.show_qr_popup = false;
            state.ui.qr_code_lines.clear();
        }
        _ => {}
    }
    false
}

/// Handle keyboard events for the manual add network popup
pub fn handle_manual_add_popup(key: KeyEvent, state: &mut AppState) -> bool {
    match key.code {
        event::KeyCode::Esc => {
            state.ui.show_manual_add_popup = false;
            state.inputs.clear_manual();
        }
        event::KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Close popup like Esc
            state.ui.show_manual_add_popup = false;
            state.inputs.clear_manual();
        }
        event::KeyCode::Tab | event::KeyCode::Down => {
            state.inputs.manual_input_field = (state.inputs.manual_input_field + 1) % 6;
        }
        event::KeyCode::BackTab | event::KeyCode::Up => {
            if state.inputs.manual_input_field == 0 {
                state.inputs.manual_input_field = 5;
            } else {
                state.inputs.manual_input_field -= 1;
            }
        }
        event::KeyCode::Enter => {
            match state.inputs.manual_input_field {
                3 => state.inputs.manual_hidden = !state.inputs.manual_hidden,
                4 => {
                    // Connect
                    if !state.inputs.manual_ssid_input.value.is_empty() {
                        state.connection.is_connecting = true;
                        state.connection.target_ssid =
                            Some(state.inputs.manual_ssid_input.value.clone());
                        state.connection.connection_start_time = Some(Instant::now());
                        let ssid = state.inputs.manual_ssid_input.value.clone();
                        let password =
                            SecretString::from(state.inputs.manual_password_input.value.clone());
                        let security = state.inputs.manual_security.clone();
                        let hidden = state.inputs.manual_hidden;

                        let (tx, rx) = mpsc::channel(1);
                        state.connection.connection_result_rx = Some(rx);

                        tokio::spawn(async move {
                            if get_connected_ssid().unwrap_or(None).is_some() {
                                let _ = tokio::task::spawn_blocking(crate::wifi::disconnect_and_wait).await;
                            }
                            let result = tokio::task::spawn_blocking(move || {
                                if security == "Open" {
                                    crate::wifi::connect_open(&ssid, hidden)
                                } else {
                                    // Map security string to auth/cipher
                                    let (auth, cipher) = match security.as_str() {
                                        "WPA3-Personal" => ("WPA3-SAE", "AES"),
                                        "WPA2-Personal" => ("WPA2-PSK", "AES"),
                                        "WPA-Personal" => ("WPA-PSK", "AES"),
                                        "WEP" => ("Shared", "WEP"),
                                        _ => ("WPA2-PSK", "AES"),
                                    };
                                    crate::wifi::connect_with_password(
                                        &ssid, &password, auth, cipher, hidden,
                                    )
                                }
                            })
                            .await
                            .unwrap_or_else(|e| Err(WifiError::Internal(e.to_string())));
                            let _ = tx.send(result.map_err(|e: WifiError| e.into())).await;
                        });

                        state.ui.show_manual_add_popup = false;
                        state.inputs.clear_manual();
                    }
                }
                5 => {
                    // Cancel
                    state.ui.show_manual_add_popup = false;
                    state.inputs.clear_manual();
                }
                _ => {}
            }
        }
        event::KeyCode::Char(' ') if state.inputs.manual_input_field == 3 => {
            state.inputs.manual_hidden = !state.inputs.manual_hidden;
        }
        event::KeyCode::Char(c) => {
            match state.inputs.manual_input_field {
                0 => state.inputs.manual_ssid_input.insert(c),
                1 => state.inputs.manual_password_input.insert(c),
                2 => {
                    // Handle h/j/k/l for Security field
                    let options = [
                        "WPA2-Personal",
                        "WPA3-Personal",
                        "Open",
                        "WPA-Personal",
                        "WEP",
                    ];
                    let current_idx = options
                        .iter()
                        .position(|&s| s == state.inputs.manual_security)
                        .unwrap_or(0);
                    match c {
                        'h' | 'k' => {
                            let next_idx = if current_idx == 0 {
                                options.len() - 1
                            } else {
                                current_idx - 1
                            };
                            state.inputs.manual_security = options[next_idx].to_string();
                        }
                        'l' | 'j' => {
                            let next_idx = (current_idx + 1) % options.len();
                            state.inputs.manual_security = options[next_idx].to_string();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        event::KeyCode::Backspace
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            match state.inputs.manual_input_field {
                0 => state.inputs.manual_ssid_input.backspace_word(),
                1 => state.inputs.manual_password_input.backspace_word(),
                _ => {}
            }
        }
        event::KeyCode::Backspace => match state.inputs.manual_input_field {
            0 => state.inputs.manual_ssid_input.backspace(),
            1 => state.inputs.manual_password_input.backspace(),
            _ => {}
        },
        event::KeyCode::Left
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            match state.inputs.manual_input_field {
                0 => state.inputs.manual_ssid_input.move_word_left(),
                1 => state.inputs.manual_password_input.move_word_left(),
                _ => {}
            }
        }
        event::KeyCode::Left => match state.inputs.manual_input_field {
            0 => state.inputs.manual_ssid_input.move_left(),
            1 => state.inputs.manual_password_input.move_left(),
            2 => {
                let options = [
                    "WPA2-Personal",
                    "WPA3-Personal",
                    "Open",
                    "WPA-Personal",
                    "WEP",
                ];
                let current_idx = options
                    .iter()
                    .position(|&s| s == state.inputs.manual_security)
                    .unwrap_or(0);
                let next_idx = if current_idx == 0 {
                    options.len() - 1
                } else {
                    current_idx - 1
                };
                state.inputs.manual_security = options[next_idx].to_string();
            }
            _ => {}
        },
        event::KeyCode::Right
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            match state.inputs.manual_input_field {
                0 => state.inputs.manual_ssid_input.move_word_right(),
                1 => state.inputs.manual_password_input.move_word_right(),
                _ => {}
            }
        }
        event::KeyCode::Right => match state.inputs.manual_input_field {
            0 => state.inputs.manual_ssid_input.move_right(),
            1 => state.inputs.manual_password_input.move_right(),
            2 => {
                let options = [
                    "WPA2-Personal",
                    "WPA3-Personal",
                    "Open",
                    "WPA-Personal",
                    "WEP",
                ];
                let current_idx = options
                    .iter()
                    .position(|&s| s == state.inputs.manual_security)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % options.len();
                state.inputs.manual_security = options[next_idx].to_string();
            }
            _ => {}
        },
        event::KeyCode::Home => match state.inputs.manual_input_field {
            0 => state.inputs.manual_ssid_input.move_home(),
            1 => state.inputs.manual_password_input.move_home(),
            _ => {}
        },
        event::KeyCode::End => match state.inputs.manual_input_field {
            0 => state.inputs.manual_ssid_input.move_end(),
            1 => state.inputs.manual_password_input.move_end(),
            _ => {}
        },
        _ => {}
    }
    false
}

/// Handle keyboard events for the password popup
pub fn handle_password_popup(key: KeyEvent, state: &mut AppState) -> bool {
    match key.code {
        event::KeyCode::Enter => {
            if let Some(ssid) = state.connection.connecting_to_ssid.take() {
                state.connection.is_connecting = true;
                state.connection.target_ssid = Some(ssid.clone());
                state.connection.connection_start_time = Some(Instant::now());
                let password = SecretString::from(state.inputs.password_input.value.clone());
                let (tx, rx) = mpsc::channel(1);
                state.connection.connection_result_rx = Some(rx);

                let wifi_info = state
                    .network
                    .wifi_list
                    .iter()
                    .find(|w| w.ssid == ssid)
                    .cloned();

                tokio::spawn(async move {
                    if get_connected_ssid().unwrap_or(None).is_some() {
                        let _ = tokio::task::spawn_blocking(crate::wifi::disconnect_and_wait).await;
                    }
                    let result = tokio::task::spawn_blocking(move || {
                        if let Some(info) = wifi_info {
                            crate::wifi::connect_with_password(
                                &ssid,
                                &password,
                                &info.authentication,
                                &info.encryption,
                                false,
                            )
                        } else {
                            crate::wifi::connect_with_password(
                                &ssid, &password, "WPA2-PSK", "AES", false,
                            )
                        }
                    })
                    .await
                    .unwrap_or_else(|e| Err(WifiError::Internal(e.to_string())));
                    let _ = tx.send(result.map_err(|e: WifiError| e.into())).await;
                });
            }
            state.ui.show_password_popup = false;
            state.inputs.password_input.clear();
        }
        event::KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.show_password_popup = false;
            state.inputs.password_input.clear();
        }
        event::KeyCode::Esc => {
            state.ui.show_password_popup = false;
            state.inputs.password_input.clear();
        }
        _ => {
            // Use the input helper for common key handling
            state.inputs.password_input.handle_key(&key);
        }
    }
    false
}

/// Handle keyboard events for the search mode
pub fn handle_search_mode(key: KeyEvent, state: &mut AppState) -> bool {
    match key.code {
        event::KeyCode::Esc => {
            state.ui.is_searching = false;
        }
        event::KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.ui.is_searching = false;
        }
        event::KeyCode::Enter => {
            state.ui.is_searching = false;
            if !state.network.filtered_wifi_list.is_empty() {
                state.ui.l_state.select(Some(0));
            }
        }
        event::KeyCode::Char(c) => {
            state.inputs.search_input.insert(c);
            state.update_filtered_list();
        }
        _ => {
            if state.inputs.search_input.handle_key(&key) {
                state.update_filtered_list();
            }
        }
    }
    false
}

/// Handle keyboard events for the main view (network list)
pub fn handle_main_view(key: KeyEvent, state: &mut AppState) -> bool {
    use std::time::Duration;

    match key.code {
        event::KeyCode::Char('/') => {
            state.ui.is_searching = true;
        }
        event::KeyCode::Char('n') => {
            state.ui.show_manual_add_popup = true;
            state.inputs.manual_input_field = 0;
        }
        event::KeyCode::Esc => {
            if state.connection.is_connecting {
                state.connection.is_connecting = false;
                state.connection.target_ssid = None;
                state.connection.connection_result_rx = None;
            } else if !state.inputs.search_input.value.is_empty() {
                state.inputs.search_input.clear();
                state.update_filtered_list();
            }
        }
        event::KeyCode::Char('q') => return true,
        event::KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !state.inputs.search_input.value.is_empty() {
                state.inputs.search_input.clear();
                state.update_filtered_list();
            }
        }
        event::KeyCode::Char('j') | event::KeyCode::Down => state.next(),
        event::KeyCode::Char('k') | event::KeyCode::Up => state.previous(),
        event::KeyCode::Char('g') | event::KeyCode::Home => state.go_to_top(),
        event::KeyCode::Char('G') | event::KeyCode::End => state.go_to_bottom(),
        event::KeyCode::Enter => {
            if let Some(selected) = state.ui.l_state.selected() {
                if let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned() {
                    let is_connected = if let Some(connected_ssid) = &state.network.connected_ssid {
                        wifi.ssid == *connected_ssid
                    } else {
                        false
                    };

                    if is_connected {
                        let (tx, rx) = mpsc::channel(1);
                        state.connection.connection_result_rx = Some(rx);
                        tokio::spawn(async move {
                            let result = tokio::task::spawn_blocking(disconnect).await;
                            let result = match result {
                                Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                                Err(e) => Err(eyre!(e.to_string())),
                            };
                            let _ = tx.send(result).await;
                        });
                    } else if wifi.authentication != "Open" {
                        // Check if profile exists
                        let saved_profiles = crate::wifi::get_saved_profiles().unwrap_or_default();
                        if saved_profiles.contains(&wifi.ssid) {
                            state.connection.is_connecting = true;
                            state.connection.target_ssid = Some(wifi.ssid.clone());
                            state.connection.connection_start_time = Some(Instant::now());
                            let ssid = wifi.ssid.clone();
                            let (tx, rx) = mpsc::channel(1);
                            state.connection.connection_result_rx = Some(rx);

                            tokio::spawn(async move {
                                if get_connected_ssid().unwrap_or(None).is_some() {
                                    let _ = tokio::task::spawn_blocking(crate::wifi::disconnect_and_wait).await;
                                }
                                let result = tokio::task::spawn_blocking(move || {
                                    crate::wifi::connect_profile(&ssid)
                                })
                                .await;
                                let result = match result {
                                    Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                                    Err(e) => Err(eyre!(e.to_string())),
                                };
                                let _ = tx.send(result).await;
                            });
                        } else {
                            state.ui.show_password_popup = true;
                            state.inputs.password_input.cursor = 0;
                            state.connection.connecting_to_ssid = Some(wifi.ssid.clone());
                        }
                    } else {
                        state.connection.is_connecting = true;
                        state.connection.target_ssid = Some(wifi.ssid.clone());
                        state.connection.connection_start_time = Some(Instant::now());
                        let ssid = wifi.ssid.clone();
                        let (tx, rx) = mpsc::channel(1);
                        state.connection.connection_result_rx = Some(rx);

                        tokio::spawn(async move {
                            if get_connected_ssid().unwrap_or(None).is_some() {
                                let _ = tokio::task::spawn_blocking(crate::wifi::disconnect_and_wait).await;
                            }
                            let result = tokio::task::spawn_blocking(move || {
                                crate::wifi::connect_open(&ssid, false)
                            })
                            .await;
                            let result = match result {
                                Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                                Err(e) => Err(eyre!(e.to_string())),
                            };
                            let _ = tx.send(result).await;
                        });
                    }
                }
            }
        }
        event::KeyCode::Char('r') => {
            // Debounce rapid 'r' key presses
            if state.refresh.last_manual_refresh.elapsed()
                < Duration::from_millis(config::MANUAL_REFRESH_DEBOUNCE_MS)
            {
                return false;
            }
            state.refresh.last_manual_refresh = Instant::now();
            state.refresh.is_refreshing_networks = true;
            let (tx, rx) = mpsc::channel(1);
            state.refresh.network_update_rx = Some(rx);

            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(|| {
                    let _ = crate::wifi::scan_networks();
                    std::thread::sleep(Duration::from_millis(config::SCAN_DELAY_MS));
                    let networks = get_wifi_networks()?;
                    let connected = get_connected_ssid()?;
                    Ok((networks, connected))
                })
                .await;
                let result = match result {
                    Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                    Err(e) => Err(eyre!(e.to_string())),
                };
                let _ = tx.send(result).await;
            });
        }
        event::KeyCode::Char('a') => {
            if let Some(selected) = state.ui.l_state.selected() {
                if let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned() {
                    if wifi.is_saved {
                        let ssid = wifi.ssid.clone();
                        let auto_connect = !wifi.auto_connect;
                        let (tx, rx) = mpsc::channel(1);
                        state.connection.connection_result_rx = Some(rx);

                        tokio::spawn(async move {
                            let result = tokio::task::spawn_blocking(move || {
                                crate::wifi::set_auto_connect(&ssid, auto_connect)
                            })
                            .await;
                            let result = match result {
                                Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                                Err(e) => Err(eyre!(e.to_string())),
                            };
                            let _ = tx.send(result).await;
                        });
                    }
                }
            }
        }
        event::KeyCode::Char('f') => {
            if let Some(selected) = state.ui.l_state.selected() {
                if let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned() {
                    if wifi.is_saved {
                        let ssid = wifi.ssid.clone();
                        let (tx, rx) = mpsc::channel(1);
                        state.connection.connection_result_rx = Some(rx);

                        tokio::spawn(async move {
                            let result = tokio::task::spawn_blocking(move || {
                                crate::wifi::forget_network(&ssid)
                            })
                            .await;
                            let result = match result {
                                Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                                Err(e) => Err(eyre!(e.to_string())),
                            };
                            let _ = tx.send(result).await;
                        });
                    }
                }
            }
        }
        event::KeyCode::Char('s') => {
            if let Some(selected) = state.ui.l_state.selected() {
                if let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned() {
                    if wifi.is_saved {
                        let ssid = wifi.ssid.clone();
                        let auth = wifi.authentication.clone();
                        let password_result = crate::wifi::get_wifi_password(&ssid);

                        match password_result {
                            Ok(password_opt) => {
                                let qr_lines =
                                    generate_wifi_qr(&ssid, &auth, password_opt.as_ref());
                                state.ui.qr_code_lines = qr_lines;
                                state.ui.show_qr_popup = true;
                            }
                            Err(_) => {
                                let qr_lines = generate_wifi_qr(&ssid, &auth, None);
                                state.ui.qr_code_lines = qr_lines;
                                state.ui.show_qr_popup = true;
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    false
}

/// Generate WiFi QR code in standard format: WIFI:S:ssid;T:auth;P:password;;
fn generate_wifi_qr(ssid: &str, auth: &str, password: Option<&SecretString>) -> Vec<String> {
    use qrcode::QrCode;
    use qrcode::render::unicode;
    use secrecy::ExposeSecret;

    let auth_type = match auth {
        "WPA3-SAE" | "WPA3" => "WPA",
        "WPA2-PSK" | "WPA2" | "WPA-PSK" | "WPA" => "WPA",
        "Open" | "open" => "nopass",
        _ => "WPA",
    };

    let qr_string = if auth_type == "nopass" {
        format!("WIFI:S:{};T:nopass;;", escape_special_chars(ssid))
    } else if let Some(pwd) = password {
        format!(
            "WIFI:S:{};T:{};P:{};;",
            escape_special_chars(ssid),
            auth_type,
            escape_special_chars(pwd.expose_secret())
        )
    } else {
        format!("WIFI:S:{};T:{};;", escape_special_chars(ssid), auth_type)
    };

    match QrCode::new(&qr_string) {
        Ok(code) => {
            let string = code.render::<unicode::Dense1x2>().build();
            string.lines().map(|s| s.to_string()).collect()
        }
        Err(_) => vec!["Error generating QR code".to_string()],
    }
}

/// Escape special characters for WiFi QR code format
fn escape_special_chars(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace(':', "\\:")
}
