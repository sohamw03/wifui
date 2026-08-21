use crate::app::AppState;
use crate::config;
use crate::error::WifiError;
use crate::ui::LayoutAreas;
use crate::wifi::{disconnect, get_connected_ssid, get_wifi_networks};
use color_eyre::eyre::eyre;
use crossterm::event::{self, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use secrecy::SecretString;
use std::time::Instant;
use tokio::sync::mpsc;

async fn disconnect_if_connected() {
    let is_connected = matches!(
        tokio::task::spawn_blocking(get_connected_ssid).await,
        Ok(Ok(Some(_)))
    );
    if is_connected {
        let _ = tokio::task::spawn_blocking(crate::wifi::disconnect_and_wait).await;
    }
}

const MANUAL_SECURITY_OPTIONS: [&str; 5] = [
    "WPA2-Personal",
    "WPA3-Personal",
    "Open",
    "WPA-Personal",
    "WEP",
];

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
                4 => trigger_manual_connect(state),
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
                    let current_idx = MANUAL_SECURITY_OPTIONS
                        .iter()
                        .position(|&s| s == state.inputs.manual_security)
                        .unwrap_or(0);
                    match c {
                        'h' | 'k' => {
                            let next_idx = if current_idx == 0 {
                                MANUAL_SECURITY_OPTIONS.len() - 1
                            } else {
                                current_idx - 1
                            };
                            state.inputs.manual_security =
                                MANUAL_SECURITY_OPTIONS[next_idx].to_string();
                        }
                        'l' | 'j' => {
                            let next_idx = (current_idx + 1) % MANUAL_SECURITY_OPTIONS.len();
                            state.inputs.manual_security =
                                MANUAL_SECURITY_OPTIONS[next_idx].to_string();
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
                let current_idx = MANUAL_SECURITY_OPTIONS
                    .iter()
                    .position(|&s| s == state.inputs.manual_security)
                    .unwrap_or(0);
                let next_idx = if current_idx == 0 {
                    MANUAL_SECURITY_OPTIONS.len() - 1
                } else {
                    current_idx - 1
                };
                state.inputs.manual_security = MANUAL_SECURITY_OPTIONS[next_idx].to_string();
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
                let current_idx = MANUAL_SECURITY_OPTIONS
                    .iter()
                    .position(|&s| s == state.inputs.manual_security)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % MANUAL_SECURITY_OPTIONS.len();
                state.inputs.manual_security = MANUAL_SECURITY_OPTIONS[next_idx].to_string();
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
            if let Some(ssid) = state.connection.pending_password_ssid.take() {
                let operation_id = state.connection.begin_connection_attempt(ssid.clone());
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
                    disconnect_if_connected().await;
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
                    let _ = tx
                        .send((operation_id, result.map_err(|e: WifiError| e.into())))
                        .await;
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
                state.connection.cancel_operation();
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
                connect_selected(state, selected);
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
                    crate::wifi::scan_networks()?;
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
            if let Some(selected) = state.ui.l_state.selected()
                && let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned()
                && wifi.is_saved
            {
                let ssid = wifi.ssid.clone();
                let auto_connect = !wifi.auto_connect;
                let operation_id = state.connection.begin_operation();
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
                    let _ = tx.send((operation_id, result)).await;
                });
            }
        }
        event::KeyCode::Char('f') => {
            if let Some(selected) = state.ui.l_state.selected()
                && let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned()
                && wifi.is_saved
            {
                let ssid = wifi.ssid.clone();
                let operation_id = state.connection.begin_operation();
                let (tx, rx) = mpsc::channel(1);
                state.connection.connection_result_rx = Some(rx);

                tokio::spawn(async move {
                    let result =
                        tokio::task::spawn_blocking(move || crate::wifi::forget_network(&ssid))
                            .await;
                    let result = match result {
                        Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                        Err(e) => Err(eyre!(e.to_string())),
                    };
                    let _ = tx.send((operation_id, result)).await;
                });
            }
        }
        event::KeyCode::Char('s') => {
            if let Some(selected) = state.ui.l_state.selected()
                && let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned()
                && wifi.is_saved
            {
                let ssid = wifi.ssid.clone();
                let auth = wifi.authentication.clone();
                if auth == "Open" || auth == "open" {
                    let qr_lines = generate_wifi_qr(&ssid, &auth, None);
                    state.ui.qr_code_lines = qr_lines;
                    state.ui.show_qr_popup = true;
                } else if qr_auth_type(&auth).is_none() {
                    state.ui.error_message = Some(
                        "Secured-network QR sharing is unavailable for this security type"
                            .to_string(),
                    );
                } else if state.ui.qr_result_rx.is_none() {
                    let password_ssid = ssid.clone();
                    let (tx, rx) = mpsc::channel(1);
                    state.ui.qr_result_rx = Some(rx);

                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            crate::wifi::get_wifi_password(&password_ssid)
                        })
                        .await;
                        let result = match result {
                            Ok(Ok(Some(password))) => {
                                Ok(generate_wifi_qr(&ssid, &auth, Some(&password)))
                            }
                            Ok(Ok(None)) => {
                                Err(eyre!("the saved profile has no readable password"))
                            }
                            Ok(Err(error)) => Err(eyre!(error.to_string())),
                            Err(error) => Err(eyre!(error.to_string())),
                        };
                        let _ = tx.send(result).await;
                    });
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

    let auth_type = qr_auth_type(auth).unwrap_or("WPA");

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

fn qr_auth_type(auth: &str) -> Option<&'static str> {
    match auth {
        "Open" | "open" => Some("nopass"),
        "WPA3-SAE" | "WPA3" | "WPA2-PSK" | "WPA2-Personal" | "WPA-PSK" | "WPA-Personal" | "WPA" => {
            Some("WPA")
        }
        "Shared" | "WEP" => Some("WEP"),
        _ => None,
    }
}

/// Escape special characters for WiFi QR code format
fn escape_special_chars(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace(':', "\\:")
}

// ─── Mouse helpers ────────────────────────────────────────────────────────────

/// Returns true if terminal cell (col, row) falls inside the given Rect.
#[inline]
fn contains(rect: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// Returns the index into `filtered_wifi_list` that the cursor is currently over,
/// accounting for the current scroll offset and list border.
fn row_under_cursor(col: u16, row: u16, state: &AppState, areas: &LayoutAreas) -> Option<usize> {
    let list = areas.list_area;
    // list_area includes the rounded border; inner content starts one cell inside
    if col < list.x + 1 || col >= list.x + list.width.saturating_sub(1) {
        return None;
    }
    if row < list.y + 1 || row >= list.y + list.height.saturating_sub(1) {
        return None;
    }
    let relative_row = (row - list.y - 1) as usize;
    let actual_index = state.mouse.scroll_offset + relative_row;
    if actual_index < state.network.filtered_wifi_list.len() {
        Some(actual_index)
    } else {
        None
    }
}

/// Shared logic for "activate the currently selected network" — called by both
/// the keyboard Enter handler and mouse double-click.
fn connect_selected(state: &mut AppState, selected: usize) {
    if let Some(wifi) = state.network.filtered_wifi_list.get(selected).cloned() {
        let is_connected = wifi.is_connected;

        if is_connected {
            let ssid = wifi.ssid.clone();
            let operation_id = state.connection.begin_disconnect_attempt(ssid);
            let (tx, rx) = mpsc::channel(1);
            state.connection.connection_result_rx = Some(rx);
            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(disconnect).await;
                let result = match result {
                    Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                    Err(e) => Err(eyre!(e.to_string())),
                };
                let _ = tx.send((operation_id, result)).await;
            });
        } else if wifi.authentication != "Open" {
            if wifi.is_saved {
                let ssid = wifi.ssid.clone();
                let operation_id = state.connection.begin_connection_attempt(ssid.clone());
                let (tx, rx) = mpsc::channel(1);
                state.connection.connection_result_rx = Some(rx);
                tokio::spawn(async move {
                    disconnect_if_connected().await;
                    let result =
                        tokio::task::spawn_blocking(move || crate::wifi::connect_profile(&ssid))
                            .await;
                    let result = match result {
                        Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                        Err(e) => Err(eyre!(e.to_string())),
                    };
                    let _ = tx.send((operation_id, result)).await;
                });
            } else {
                state.ui.show_password_popup = true;
                state.inputs.password_input.clear();
                state.connection.pending_password_ssid = Some(wifi.ssid.clone());
            }
        } else {
            let ssid = wifi.ssid.clone();
            let operation_id = state.connection.begin_connection_attempt(ssid.clone());
            let (tx, rx) = mpsc::channel(1);
            state.connection.connection_result_rx = Some(rx);
            tokio::spawn(async move {
                disconnect_if_connected().await;
                let result =
                    tokio::task::spawn_blocking(move || crate::wifi::connect_open(&ssid, false))
                        .await;
                let result = match result {
                    Ok(inner) => inner.map_err(|e: WifiError| e.into()),
                    Err(e) => Err(eyre!(e.to_string())),
                };
                let _ = tx.send((operation_id, result)).await;
            });
        }
    }
}

/// Attempt to connect the network in the manual-add popup's Connect button slot.
/// Mirrors the `Enter` branch for `manual_input_field == 4` in `handle_manual_add_popup`.
fn trigger_manual_connect(state: &mut AppState) {
    if !state.inputs.manual_ssid_input.value.is_empty() {
        let ssid = state.inputs.manual_ssid_input.value.clone();
        let operation_id = state.connection.begin_connection_attempt(ssid.clone());
        let password = SecretString::from(state.inputs.manual_password_input.value.clone());
        let security = state.inputs.manual_security.clone();
        let hidden = state.inputs.manual_hidden;

        let (tx, rx) = mpsc::channel(1);
        state.connection.connection_result_rx = Some(rx);

        tokio::spawn(async move {
            disconnect_if_connected().await;
            let result = tokio::task::spawn_blocking(move || {
                if security == "Open" {
                    crate::wifi::connect_open(&ssid, hidden)
                } else {
                    let (auth, cipher) = match security.as_str() {
                        "WPA3-Personal" => ("WPA3-SAE", "AES"),
                        "WPA2-Personal" => ("WPA2-PSK", "AES"),
                        "WPA-Personal" => ("WPA-PSK", "AES"),
                        "WEP" => ("Shared", "WEP"),
                        _ => ("WPA2-PSK", "AES"),
                    };
                    crate::wifi::connect_with_password(&ssid, &password, auth, cipher, hidden)
                }
            })
            .await
            .unwrap_or_else(|e| Err(WifiError::Internal(e.to_string())));
            let _ = tx
                .send((operation_id, result.map_err(|e: WifiError| e.into())))
                .await;
        });

        state.ui.show_manual_add_popup = false;
        state.inputs.clear_manual();
    }
}

/// Handle all mouse events for the TUI.
pub fn handle_mouse(mouse: MouseEvent, state: &mut AppState, areas: &LayoutAreas) {
    let col = mouse.column;
    let row = mouse.row;

    match mouse.kind {
        // ── Scroll wheel ──────────────────────────────────────────────────────
        MouseEventKind::ScrollUp => {
            state.mouse.scroll_offset = state.mouse.scroll_offset.saturating_sub(1);
            *state.ui.l_state.offset_mut() = state.mouse.scroll_offset;
        }
        MouseEventKind::ScrollDown => {
            let list_height = areas.list_area.height.saturating_sub(2) as usize;
            let max_offset = state
                .network
                .filtered_wifi_list
                .len()
                .saturating_sub(list_height);
            if state.mouse.scroll_offset < max_offset {
                state.mouse.scroll_offset += 1;
            }
            *state.ui.l_state.offset_mut() = state.mouse.scroll_offset;
        }

        // ── Mouse move — hot-track ────────────────────────────────────────────
        MouseEventKind::Moved => {
            if !state.is_popup_open() {
                let new_hover = row_under_cursor(col, row, state, areas);
                state.mouse.hovered_row = new_hover;
            } else {
                state.mouse.hovered_row = None;
            }
        }

        // ── Left button press ─────────────────────────────────────────────────
        MouseEventKind::Down(MouseButton::Left) => {
            // Always dismiss the error message on any click
            if state.ui.error_message.is_some() {
                state.ui.error_message = None;
            }

            // --- QR popup: click-away to dismiss ---
            if state.ui.show_qr_popup {
                if let Some(qr_area) = areas.qr_popup_area
                    && !contains(qr_area, col, row)
                {
                    state.ui.show_qr_popup = false;
                    state.ui.qr_code_lines.clear();
                }
                return;
            }

            // --- Manual-add popup ---
            if state.ui.show_manual_add_popup {
                if let Some(popup_area) = areas.manual_popup_area
                    && !contains(popup_area, col, row)
                {
                    // Click outside — dismiss
                    state.ui.show_manual_add_popup = false;
                    state.inputs.clear_manual();
                    return;
                }
                // Click inside the popup: hit-test connect button first
                if let Some(btn) = areas.manual_connect_area
                    && contains(btn, col, row)
                {
                    trigger_manual_connect(state);
                    return;
                }
                // Hit-test individual fields (SSID=0, Password=1, Security=2, Hidden=3)
                for (i, field_area) in areas.manual_field_areas.iter().enumerate() {
                    if let Some(area) = field_area
                        && contains(*area, col, row)
                    {
                        state.inputs.manual_input_field = i;
                        return;
                    }
                }
                return;
            }

            // --- Password popup: click-away to dismiss ---
            if state.ui.show_password_popup {
                if let Some(pw_area) = areas.password_popup_area
                    && !contains(pw_area, col, row)
                {
                    state.ui.show_password_popup = false;
                    state.inputs.password_input.clear();
                }
                return;
            }

            // --- Main view: click on a list item ---
            if let Some(clicked_idx) = row_under_cursor(col, row, state, areas) {
                let is_double = state
                    .mouse
                    .last_list_click
                    .as_ref()
                    .map(|(prev_row, t)| {
                        *prev_row == clicked_idx
                            && t.elapsed()
                                < std::time::Duration::from_millis(config::DOUBLE_CLICK_MS)
                    })
                    .unwrap_or(false);

                if is_double {
                    // Double-click: select + activate (same as Enter)
                    state.ui.l_state.select(Some(clicked_idx));
                    state.mouse.last_list_click = None; // reset so triple-click doesn't re-fire
                    connect_selected(state, clicked_idx);
                } else {
                    // Single click: select only
                    state.ui.l_state.select(Some(clicked_idx));
                    state.mouse.last_list_click = Some((clicked_idx, Instant::now()));
                }
            }
        }

        // Right-click and any other events are intentionally ignored
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_supports_personal_security_types_only() {
        assert_eq!(qr_auth_type("Open"), Some("nopass"));
        assert_eq!(qr_auth_type("WPA3-SAE"), Some("WPA"));
        assert_eq!(qr_auth_type("Shared"), Some("WEP"));
        assert_eq!(qr_auth_type("WPA2"), None);
        assert_eq!(qr_auth_type("Unknown"), None);
    }

    #[test]
    fn qr_escapes_wifi_special_characters() {
        assert_eq!(
            escape_special_chars(r#"a;b,c:d"e\f"#),
            r#"a\;b\,c\:d\"e\\f"#
        );
    }
}

#[cfg(test)]
mod mouse_tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn contains_identifies_interior_cells() {
        let rect = Rect::new(5, 5, 10, 5); // x=5..14, y=5..9
        assert!(contains(rect, 5, 5));
        assert!(contains(rect, 14, 9));
        assert!(!contains(rect, 4, 5));
        assert!(!contains(rect, 15, 5));
        assert!(!contains(rect, 5, 10));
    }

    #[test]
    fn contains_zero_size_rect_never_matches() {
        let rect = Rect::new(0, 0, 0, 0);
        assert!(!contains(rect, 0, 0));
    }
}
