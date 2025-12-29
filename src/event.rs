use crate::{
    app::AppState,
    ui::render,
    wifi::{
        connect_open, connect_profile, connect_with_password, disconnect, forget_network,
        get_connected_ssid, get_saved_profiles, get_wifi_networks, scan_networks, set_auto_connect,
    },
};
use color_eyre::eyre::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event},
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
        if let Some(rx) = &mut state.connection_result_rx
            && let Ok(result) = rx.try_recv()
        {
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
                })
                .await
                .unwrap();
                let _ = tx.send(result).await;
            });
        }

        // Check for network updates
        if let Some(rx) = &mut state.network_update_rx
            && let Ok(result) = rx.try_recv()
        {
            if let Ok((new_list, connected_ssid)) = result {
                let connection_changed = state.connected_ssid != connected_ssid;

                // Try to preserve selection
                let selected_ssid = state
                    .l_state
                    .selected()
                    .and_then(|i| state.wifi_list.get(i))
                    .map(|w| w.ssid.clone());

                state.wifi_list = new_list;
                state.connected_ssid = connected_ssid;
                state.update_filtered_list();

                if connection_changed && state.connected_ssid.is_some() {
                    state.l_state.select(Some(0));
                } else if let Some(ssid) = selected_ssid {
                    if let Some(pos) = state.filtered_wifi_list.iter().position(|w| w.ssid == ssid)
                    {
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
                    && connected == target
                {
                    state.is_connecting = false;
                    state.target_ssid = None;
                    state.connection_start_time = None;
                }

                // Check for timeout (e.g. 20 seconds)
                if let Some(start_time) = state.connection_start_time
                    && start_time.elapsed() > Duration::from_secs(20)
                {
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
            && !state.show_manual_add_popup
            && !state.show_password_popup
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
                })
                .await
                .unwrap();
                let _ = tx.send(result).await;
            });
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                state.last_interaction = Instant::now();
                if key.kind == event::KeyEventKind::Press {
                    // Log key press if enabled
                    if state.show_key_logger {
                        let mut key_str = String::new();
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            key_str.push_str("Ctrl+");
                        }
                        if key.modifiers.contains(event::KeyModifiers::ALT) {
                            key_str.push_str("Alt+");
                        }
                        if key.modifiers.contains(event::KeyModifiers::SHIFT)
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
                        state.last_key_press = Some((key_str, Instant::now()));
                    }

                    // Clear error message on any key press
                    if state.error_message.is_some() {
                        state.error_message = None;
                    }

                    // Global shortcuts
                    if key.code == event::KeyCode::Char('c')
                        && key.modifiers.contains(event::KeyModifiers::CONTROL)
                    {
                        break;
                    }

                    if state.show_manual_add_popup {
                        match key.code {
                            event::KeyCode::Esc => {
                                state.show_manual_add_popup = false;
                                state.manual_ssid_input.clear();
                                state.manual_password_input.clear();
                                state.manual_input_field = 0;
                            }
                            event::KeyCode::Char('[')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                let mut cleared = false;
                                match state.manual_input_field {
                                    0 => {
                                        if !state.manual_ssid_input.value.is_empty() {
                                            state.manual_ssid_input.clear();
                                            cleared = true;
                                        }
                                    }
                                    1 => {
                                        if !state.manual_password_input.value.is_empty() {
                                            state.manual_password_input.clear();
                                            cleared = true;
                                        }
                                    }
                                    _ => {}
                                }

                                if !cleared {
                                    state.show_manual_add_popup = false;
                                    state.manual_ssid_input.clear();
                                    state.manual_password_input.clear();
                                    state.manual_input_field = 0;
                                }
                            }
                            event::KeyCode::Tab | event::KeyCode::Down => {
                                state.manual_input_field = (state.manual_input_field + 1) % 6;
                            }
                            event::KeyCode::BackTab | event::KeyCode::Up => {
                                if state.manual_input_field == 0 {
                                    state.manual_input_field = 5;
                                } else {
                                    state.manual_input_field -= 1;
                                }
                            }
                            event::KeyCode::Enter => {
                                match state.manual_input_field {
                                    3 => state.manual_hidden = !state.manual_hidden,
                                    4 => {
                                        // Connect
                                        if !state.manual_ssid_input.value.is_empty() {
                                            state.is_connecting = true;
                                            state.target_ssid =
                                                Some(state.manual_ssid_input.value.clone());
                                            state.connection_start_time = Some(Instant::now());
                                            let ssid = state.manual_ssid_input.value.clone();
                                            let password =
                                                state.manual_password_input.value.clone();
                                            let security = state.manual_security.clone();
                                            let hidden = state.manual_hidden;

                                            let (tx, rx) = mpsc::channel(1);
                                            state.connection_result_rx = Some(rx);

                                            tokio::spawn(async move {
                                                if get_connected_ssid().unwrap_or(None).is_some() {
                                                    let _ = tokio::task::spawn_blocking(disconnect)
                                                        .await;
                                                }
                                                let result =
                                                    tokio::task::spawn_blocking(move || {
                                                        if security == "Open" {
                                                            connect_open(&ssid, hidden)
                                                        } else {
                                                            // Map security string to auth/cipher
                                                            let (auth, cipher) = match security
                                                                .as_str()
                                                            {
                                                                "WPA3-SAE" => ("WPA3-SAE", "AES"),
                                                                "WPA2-PSK" => ("WPA2-PSK", "AES"),
                                                                "WPA-PSK" => ("WPA-PSK", "AES"),
                                                                "WEP" => ("Shared", "WEP"),
                                                                _ => ("WPA2-PSK", "AES"),
                                                            };
                                                            connect_with_password(
                                                                &ssid, &password, auth, cipher,
                                                                hidden,
                                                            )
                                                        }
                                                    })
                                                    .await
                                                    .unwrap();
                                                let _ = tx.send(result).await;
                                            });

                                            state.show_manual_add_popup = false;
                                            state.manual_ssid_input.clear();
                                            state.manual_password_input.clear();
                                        }
                                    }
                                    5 => {
                                        // Cancel
                                        state.show_manual_add_popup = false;
                                        state.manual_ssid_input.clear();
                                        state.manual_password_input.clear();
                                    }
                                    _ => {}
                                }
                            }
                            event::KeyCode::Char(' ') if state.manual_input_field == 3 => {
                                state.manual_hidden = !state.manual_hidden;
                            }
                            event::KeyCode::Char(c) => {
                                match state.manual_input_field {
                                    0 => state.manual_ssid_input.insert(c),
                                    1 => state.manual_password_input.insert(c),
                                    2 => {
                                        // Handle h/j/k/l for Security field
                                        let options =
                                            ["WPA2-PSK", "WPA3-SAE", "Open", "WPA-PSK", "WEP"];
                                        let current_idx = options
                                            .iter()
                                            .position(|&s| s == state.manual_security)
                                            .unwrap_or(0);
                                        match c {
                                            'h' | 'k' => {
                                                let next_idx = if current_idx == 0 {
                                                    options.len() - 1
                                                } else {
                                                    current_idx - 1
                                                };
                                                state.manual_security =
                                                    options[next_idx].to_string();
                                            }
                                            'l' | 'j' => {
                                                let next_idx = (current_idx + 1) % options.len();
                                                state.manual_security =
                                                    options[next_idx].to_string();
                                            }
                                            _ => {}
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            event::KeyCode::Backspace
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                match state.manual_input_field {
                                    0 => state.manual_ssid_input.backspace_word(),
                                    1 => state.manual_password_input.backspace_word(),
                                    _ => {}
                                }
                            }
                            event::KeyCode::Backspace => match state.manual_input_field {
                                0 => state.manual_ssid_input.backspace(),
                                1 => state.manual_password_input.backspace(),
                                _ => {}
                            },
                            event::KeyCode::Left
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                match state.manual_input_field {
                                    0 => state.manual_ssid_input.move_word_left(),
                                    1 => state.manual_password_input.move_word_left(),
                                    _ => {}
                                }
                            }
                            event::KeyCode::Left => match state.manual_input_field {
                                0 => state.manual_ssid_input.move_left(),
                                1 => state.manual_password_input.move_left(),
                                2 => {
                                    let options =
                                        ["WPA2-PSK", "WPA3-SAE", "Open", "WPA-PSK", "WEP"];
                                    let current_idx = options
                                        .iter()
                                        .position(|&s| s == state.manual_security)
                                        .unwrap_or(0);
                                    let next_idx = if current_idx == 0 {
                                        options.len() - 1
                                    } else {
                                        current_idx - 1
                                    };
                                    state.manual_security = options[next_idx].to_string();
                                }
                                _ => {}
                            },
                            event::KeyCode::Right
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                match state.manual_input_field {
                                    0 => state.manual_ssid_input.move_word_right(),
                                    1 => state.manual_password_input.move_word_right(),
                                    _ => {}
                                }
                            }
                            event::KeyCode::Right => match state.manual_input_field {
                                0 => state.manual_ssid_input.move_right(),
                                1 => state.manual_password_input.move_right(),
                                2 => {
                                    let options =
                                        ["WPA2-PSK", "WPA3-SAE", "Open", "WPA-PSK", "WEP"];
                                    let current_idx = options
                                        .iter()
                                        .position(|&s| s == state.manual_security)
                                        .unwrap_or(0);
                                    let next_idx = (current_idx + 1) % options.len();
                                    state.manual_security = options[next_idx].to_string();
                                }
                                _ => {}
                            },
                            event::KeyCode::Home => match state.manual_input_field {
                                0 => state.manual_ssid_input.move_home(),
                                1 => state.manual_password_input.move_home(),
                                _ => {}
                            },
                            event::KeyCode::End => match state.manual_input_field {
                                0 => state.manual_ssid_input.move_end(),
                                1 => state.manual_password_input.move_end(),
                                _ => {}
                            },
                            _ => {}
                        }
                    } else if state.show_password_popup {
                        match key.code {
                            event::KeyCode::Enter => {
                                if let Some(ssid) = state.connecting_to_ssid.take() {
                                    state.is_connecting = true;
                                    state.target_ssid = Some(ssid.clone());
                                    state.connection_start_time = Some(Instant::now());
                                    let password = state.password_input.value.clone();
                                    let (tx, rx) = mpsc::channel(1);
                                    state.connection_result_rx = Some(rx);

                                    let wifi_info =
                                        state.wifi_list.iter().find(|w| w.ssid == ssid).cloned();

                                    tokio::spawn(async move {
                                        if get_connected_ssid().unwrap_or(None).is_some() {
                                            let _ = tokio::task::spawn_blocking(disconnect).await;
                                        }
                                        let result = tokio::task::spawn_blocking(move || {
                                            if let Some(info) = wifi_info {
                                                connect_with_password(
                                                    &ssid,
                                                    &password,
                                                    &info.authentication,
                                                    &info.encryption,
                                                    false,
                                                )
                                            } else {
                                                connect_with_password(
                                                    &ssid, &password, "WPA2-PSK", "AES", false,
                                                )
                                            }
                                        })
                                        .await
                                        .unwrap();
                                        let _ = tx.send(result).await;
                                    });
                                }
                                state.show_password_popup = false;
                                state.password_input.clear();
                            }
                            event::KeyCode::Char('[')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                state.show_password_popup = false;
                                state.password_input.clear();
                            }
                            event::KeyCode::Char(c) => state.password_input.insert(c),
                            event::KeyCode::Backspace
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                state.password_input.backspace_word();
                            }
                            event::KeyCode::Backspace => state.password_input.backspace(),
                            event::KeyCode::Left
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                state.password_input.move_word_left();
                            }
                            event::KeyCode::Left => state.password_input.move_left(),
                            event::KeyCode::Right
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                state.password_input.move_word_right();
                            }
                            event::KeyCode::Right => state.password_input.move_right(),
                            event::KeyCode::Home => state.password_input.move_home(),
                            event::KeyCode::End => state.password_input.move_end(),
                            event::KeyCode::Esc => {
                                state.show_password_popup = false;
                                state.password_input.clear();
                            }
                            _ => {}
                        }
                    } else if state.is_searching {
                        match key.code {
                            event::KeyCode::Esc => {
                                state.is_searching = false;
                            }
                            event::KeyCode::Char('[')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                state.is_searching = false;
                            }
                            event::KeyCode::Enter => {
                                state.is_searching = false;
                                if !state.filtered_wifi_list.is_empty() {
                                    state.l_state.select(Some(0));
                                }
                            }
                            event::KeyCode::Char(c) => {
                                state.search_input.insert(c);
                                state.update_filtered_list();
                            }
                            event::KeyCode::Backspace
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                state.search_input.backspace_word();
                                state.update_filtered_list();
                            }
                            event::KeyCode::Backspace => {
                                state.search_input.backspace();
                                state.update_filtered_list();
                            }
                            event::KeyCode::Left
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                state.search_input.move_word_left();
                            }
                            event::KeyCode::Left => state.search_input.move_left(),
                            event::KeyCode::Right
                                if key.modifiers.intersects(
                                    event::KeyModifiers::CONTROL | event::KeyModifiers::ALT,
                                ) =>
                            {
                                state.search_input.move_word_right();
                            }
                            event::KeyCode::Right => state.search_input.move_right(),
                            event::KeyCode::Home => state.search_input.move_home(),
                            event::KeyCode::End => state.search_input.move_end(),
                            _ => {}
                        }
                    } else {
                        match key.code {
                            event::KeyCode::Char('/') => {
                                state.is_searching = true;
                            }
                            event::KeyCode::Char('n') => {
                                state.show_manual_add_popup = true;
                                state.manual_input_field = 0;
                            }
                            event::KeyCode::Esc => {
                                if !state.search_input.value.is_empty() {
                                    state.search_input.clear();
                                    state.update_filtered_list();
                                }
                            }
                            event::KeyCode::Char('q') => break,
                            event::KeyCode::Char('[')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                if !state.search_input.value.is_empty() {
                                    state.search_input.clear();
                                    state.update_filtered_list();
                                }
                            }
                            event::KeyCode::Char('j') | event::KeyCode::Down => state.next(),
                            event::KeyCode::Char('k') | event::KeyCode::Up => state.previous(),
                            event::KeyCode::Char('g') => state.go_to_top(),
                            event::KeyCode::Char('G') => state.go_to_bottom(),
                            event::KeyCode::Enter => {
                                if let Some(selected) = state.l_state.selected()
                                    && let Some(wifi) =
                                        state.filtered_wifi_list.get(selected).cloned()
                                {
                                    let is_connected =
                                        if let Some(connected_ssid) = &state.connected_ssid {
                                            wifi.ssid == *connected_ssid
                                        } else {
                                            false
                                        };

                                    if is_connected {
                                        let (tx, rx) = mpsc::channel(1);
                                        state.connection_result_rx = Some(rx);
                                        tokio::spawn(async move {
                                            let result = tokio::task::spawn_blocking(disconnect)
                                                .await
                                                .unwrap();
                                            let _ = tx.send(result).await;
                                        });
                                    } else if wifi.authentication != "Open" {
                                        // Check if profile exists
                                        let saved_profiles =
                                            get_saved_profiles().unwrap_or_default();
                                        if saved_profiles.contains(&wifi.ssid) {
                                            state.is_connecting = true;
                                            state.target_ssid = Some(wifi.ssid.clone());
                                            state.connection_start_time = Some(Instant::now());
                                            let ssid = wifi.ssid.clone();
                                            let (tx, rx) = mpsc::channel(1);
                                            state.connection_result_rx = Some(rx);

                                            tokio::spawn(async move {
                                                if get_connected_ssid().unwrap_or(None).is_some() {
                                                    let _ = tokio::task::spawn_blocking(disconnect)
                                                        .await;
                                                }
                                                let result =
                                                    tokio::task::spawn_blocking(move || {
                                                        connect_profile(&ssid)
                                                    })
                                                    .await
                                                    .unwrap();
                                                let _ = tx.send(result).await;
                                            });
                                        } else {
                                            state.show_password_popup = true;
                                            state.password_input.cursor = 0;
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
                                                let _ =
                                                    tokio::task::spawn_blocking(disconnect).await;
                                            }
                                            let result = tokio::task::spawn_blocking(move || {
                                                connect_open(&ssid, false)
                                            })
                                            .await
                                            .unwrap();
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
                                    })
                                    .await
                                    .unwrap();
                                    let _ = tx.send(result).await;
                                });
                            }
                            event::KeyCode::Char('a') => {
                                if let Some(selected) = state.l_state.selected()
                                    && let Some(wifi) =
                                        state.filtered_wifi_list.get(selected).cloned()
                                    && wifi.is_saved
                                {
                                    let ssid = wifi.ssid.clone();
                                    let auto_connect = !wifi.auto_connect;
                                    let (tx, rx) = mpsc::channel(1);
                                    state.connection_result_rx = Some(rx);

                                    tokio::spawn(async move {
                                        let result = tokio::task::spawn_blocking(move || {
                                            set_auto_connect(&ssid, auto_connect)
                                        })
                                        .await
                                        .unwrap();
                                        let _ = tx.send(result).await;
                                    });
                                }
                            }
                            event::KeyCode::Char('f') => {
                                if let Some(selected) = state.l_state.selected()
                                    && let Some(wifi) =
                                        state.filtered_wifi_list.get(selected).cloned()
                                {
                                    let ssid = wifi.ssid.clone();
                                    let (tx, rx) = mpsc::channel(1);
                                    state.connection_result_rx = Some(rx);

                                    tokio::spawn(async move {
                                        let result = tokio::task::spawn_blocking(move || {
                                            forget_network(&ssid)
                                        })
                                        .await
                                        .unwrap();
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
