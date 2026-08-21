use crate::{
    config::{self, IconSet},
    input::InputState,
    wifi::{ConnectionEvent, WifiInfo, WifiListener},
};
use color_eyre::eyre::Result;
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender};

/// Network-related state
#[derive(Debug)]
pub struct NetworkState {
    pub wifi_list: Vec<WifiInfo>,
    pub filtered_wifi_list: Vec<WifiInfo>,
    pub connected_ssid: Option<String>,
}

impl NetworkState {
    pub fn new(wifi_list: Vec<WifiInfo>) -> Self {
        Self {
            filtered_wifi_list: wifi_list.clone(),
            wifi_list,
            connected_ssid: None,
        }
    }
}

/// UI state for display and navigation
#[derive(Debug)]
pub struct UiState {
    pub l_state: ListState,
    pub is_searching: bool,
    pub show_password_popup: bool,
    pub show_manual_add_popup: bool,
    pub show_qr_popup: bool,
    pub qr_code_lines: Vec<String>,
    pub qr_result_rx: Option<Receiver<Result<Vec<String>>>>,
    pub error_message: Option<String>,
    pub loading_frame: usize,
    pub show_key_logger: bool,
    pub last_key_press: Option<(String, Instant)>,
    pub icon_set: IconSet,
}

impl UiState {
    pub fn new(show_key_logger: bool, use_ascii_icons: bool, has_networks: bool) -> Self {
        Self {
            l_state: ListState::default().with_selected(if has_networks { Some(0) } else { None }),
            is_searching: false,
            show_password_popup: false,
            show_manual_add_popup: false,
            show_qr_popup: false,
            qr_code_lines: Vec::new(),
            qr_result_rx: None,
            error_message: None,
            loading_frame: 0,
            show_key_logger,
            last_key_press: None,
            icon_set: if use_ascii_icons {
                IconSet::Ascii
            } else {
                IconSet::Nerd
            },
        }
    }
}

/// Connection operation state
#[derive(Debug)]
pub struct ConnectionState {
    pub is_connecting: bool,
    pub pending_password_ssid: Option<String>,
    pub target_ssid: Option<String>,
    pub is_disconnecting: bool,
    pub disconnecting_ssid: Option<String>,
    pub connection_start_time: Option<Instant>,
    pub connection_result_rx: Option<Receiver<OperationResult>>,
    pub next_operation_id: u64,
    pub active_operation_id: Option<u64>,
    pub active_connection_attempt_id: Option<u64>,
    #[allow(dead_code)]
    pub wifi_listener: Option<WifiListener>,
    pub listener_init_rx: Option<Receiver<crate::error::WifiResult<WifiListener>>>,
    pub connection_event_tx: Option<UnboundedSender<ConnectionEvent>>,
    pub connection_event_rx: Option<UnboundedReceiver<ConnectionEvent>>,
}

pub type OperationResult = (u64, Result<()>);

impl ConnectionState {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            is_connecting: false,
            pending_password_ssid: None,
            target_ssid: None,
            is_disconnecting: false,
            disconnecting_ssid: None,
            connection_start_time: None,
            connection_result_rx: None,
            next_operation_id: 0,
            active_operation_id: None,
            active_connection_attempt_id: None,
            wifi_listener: None,
            listener_init_rx: None,
            connection_event_tx: Some(tx),
            connection_event_rx: Some(rx),
        }
    }

    pub fn begin_operation(&mut self) -> u64 {
        self.next_operation_id = self.next_operation_id.wrapping_add(1).max(1);
        let operation_id = self.next_operation_id;
        self.active_operation_id = Some(operation_id);
        operation_id
    }

    pub fn begin_connection_attempt(&mut self, ssid: String) -> u64 {
        let operation_id = self.begin_operation();
        self.is_connecting = true;
        self.target_ssid = Some(ssid);
        self.connection_start_time = Some(Instant::now());
        self.active_connection_attempt_id = Some(operation_id);
        operation_id
    }

    pub fn begin_disconnect_attempt(&mut self, ssid: String) -> u64 {
        let operation_id = self.begin_operation();
        self.is_disconnecting = true;
        self.disconnecting_ssid = Some(ssid);
        self.connection_start_time = Some(Instant::now());
        operation_id
    }

    pub fn finish_disconnect_attempt(&mut self) {
        self.is_disconnecting = false;
        self.disconnecting_ssid = None;
    }

    pub fn finish_connection_attempt(&mut self) {
        self.active_operation_id = None;
        self.connection_result_rx = None;
        self.is_connecting = false;
        self.target_ssid = None;
        self.connection_start_time = None;
        self.active_connection_attempt_id = None;
    }

    pub fn cancel_operation(&mut self) {
        self.active_operation_id = None;
        self.finish_connection_attempt();
    }
}

/// Input field states
#[derive(Debug, Default)]
pub struct InputStates {
    pub password_input: InputState,
    pub search_input: InputState,
    pub manual_ssid_input: InputState,
    pub manual_password_input: InputState,
    pub manual_security: String,
    pub manual_hidden: bool,
    pub manual_input_field: usize,
}

impl InputStates {
    pub fn new() -> Self {
        Self {
            password_input: InputState::new(),
            search_input: InputState::new(),
            manual_ssid_input: InputState::new(),
            manual_password_input: InputState::new(),
            manual_security: "WPA2-Personal".to_string(),
            manual_hidden: false,
            manual_input_field: 0,
        }
    }

    pub fn clear_manual(&mut self) {
        self.manual_ssid_input.clear();
        self.manual_password_input.clear();
        self.manual_input_field = 0;
    }
}

/// Payload sent back to the event loop when a background network refresh finishes.
pub type NetworkUpdate = Result<(Vec<WifiInfo>, Option<String>)>;

/// Refresh and timing state
#[derive(Debug)]
pub struct RefreshState {
    pub last_refresh: Instant,
    pub last_interaction: Instant,
    pub last_manual_refresh: Instant,
    pub is_refreshing_networks: bool,
    pub network_update_rx: Option<Receiver<NetworkUpdate>>,
    pub refresh_burst: u8,
    pub is_initial_loading: bool,
}

impl RefreshState {
    pub fn new() -> Self {
        Self {
            last_refresh: Instant::now() - Duration::from_secs(15), // Force immediate refresh
            last_interaction: Instant::now(),
            last_manual_refresh: Instant::now() - Duration::from_secs(15), // Allow immediate manual refresh
            is_refreshing_networks: false,
            network_update_rx: None,
            refresh_burst: config::STARTUP_REFRESH_BURST,
            is_initial_loading: true,
        }
    }
}

/// Mouse interaction state
#[derive(Debug, Default)]
pub struct MouseState {
    /// Row index inside filtered_wifi_list the cursor is currently over, if any
    pub hovered_row: Option<usize>,
    /// Instant and row index of the most recent left-click on a list item (double-click detection)
    pub last_list_click: Option<(usize, Instant)>,
    /// Scroll offset for the network list viewport (mirrors ListState::offset)
    pub scroll_offset: usize,
}

/// Main application state
#[derive(Debug)]
pub struct AppState {
    pub network: NetworkState,
    pub ui: UiState,
    pub connection: ConnectionState,
    pub inputs: InputStates,
    pub refresh: RefreshState,
    pub mouse: MouseState,
}

impl AppState {
    pub fn new(wifi_list: Vec<WifiInfo>, show_key_logger: bool, use_ascii_icons: bool) -> AppState {
        let has_networks = !wifi_list.is_empty();
        AppState {
            network: NetworkState::new(wifi_list),
            ui: UiState::new(show_key_logger, use_ascii_icons, has_networks),
            connection: ConnectionState::new(),
            inputs: InputStates::new(),
            refresh: RefreshState::new(),
            mouse: MouseState::default(),
        }
    }

    pub fn next(&mut self) {
        let i = match self.ui.l_state.selected() {
            Some(i) => {
                if i >= self.network.filtered_wifi_list.len().saturating_sub(1) {
                    i
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.ui.l_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.ui.l_state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.ui.l_state.select(Some(i));
    }

    pub fn go_to_top(&mut self) {
        if !self.network.filtered_wifi_list.is_empty() {
            self.ui.l_state.select(Some(0));
        }
    }

    pub fn go_to_bottom(&mut self) {
        if !self.network.filtered_wifi_list.is_empty() {
            self.ui
                .l_state
                .select(Some(self.network.filtered_wifi_list.len() - 1));
        }
    }

    pub fn update_filtered_list(&mut self) {
        if self.inputs.search_input.value.is_empty() {
            self.network.filtered_wifi_list = self.network.wifi_list.clone();
        } else {
            let search_lower = self.inputs.search_input.value.to_lowercase();
            self.network.filtered_wifi_list = self
                .network
                .wifi_list
                .iter()
                .filter(|w| {
                    let ssid_lower = w.ssid.to_lowercase();
                    let mut search_chars = search_lower.chars();
                    let mut search_char = search_chars.next();

                    for c in ssid_lower.chars() {
                        if let Some(sc) = search_char {
                            if c == sc {
                                search_char = search_chars.next();
                            }
                        } else {
                            break;
                        }
                    }
                    search_char.is_none()
                })
                .cloned()
                .collect();
        }
        // Reset selection if out of bounds
        if let Some(selected) = self.ui.l_state.selected()
            && selected >= self.network.filtered_wifi_list.len()
        {
            self.ui.l_state.select(Some(0));
        }
    }

    /// Apply a completed background refresh, preserving the list selection when possible.
    ///
    /// Selection is kept by (SSID, BSSID) first, then by SSID alone; it resets to the
    /// top row only when the selection disappeared or the connection changed.
    pub fn apply_network_update(
        &mut self,
        new_list: Vec<WifiInfo>,
        connected_ssid: Option<String>,
    ) {
        let connection_changed = self.network.connected_ssid != connected_ssid;

        // Try to preserve selection
        let selected_network = self
            .ui
            .l_state
            .selected()
            .and_then(|i| self.network.filtered_wifi_list.get(i))
            .map(|w| (w.ssid.clone(), w.bssid.clone()));

        self.network.wifi_list = new_list;
        self.network.connected_ssid = connected_ssid;
        self.update_filtered_list();

        if connection_changed && self.network.connected_ssid.is_some() {
            self.ui.l_state.select(Some(0));
        } else if let Some((ssid, bssid)) = selected_network {
            let position = bssid.as_ref().and_then(|selected_bssid| {
                self.network
                    .filtered_wifi_list
                    .iter()
                    .position(|w| w.ssid == ssid && w.bssid.as_ref() == Some(selected_bssid))
            });
            if let Some(pos) = position.or_else(|| {
                self.network
                    .filtered_wifi_list
                    .iter()
                    .position(|w| w.ssid == ssid)
            }) {
                self.ui.l_state.select(Some(pos));
            } else {
                self.ui.l_state.select(Some(0));
            }
        } else {
            // No previous selection, select first item
            self.ui.l_state.select(Some(0));
        }
    }

    /// Check if any popup is open (for dimming the background)
    pub fn is_popup_open(&self) -> bool {
        self.ui.show_manual_add_popup || self.ui.show_password_popup || self.ui.show_qr_popup
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wifi(ssid: &str, bssid: Option<&str>) -> WifiInfo {
        WifiInfo {
            ssid: ssid.to_string(),
            bssid: bssid.map(str::to_string),
            ..WifiInfo::default()
        }
    }

    fn state_with(networks: Vec<WifiInfo>) -> AppState {
        let mut state = AppState::new(networks, false, true);
        state.update_filtered_list();
        state
    }

    #[test]
    fn selection_preserved_when_network_unchanged() {
        let mut state = state_with(vec![
            wifi("a", None),
            wifi("b", Some("aa:bb")),
            wifi("c", None),
        ]);
        state.ui.l_state.select(Some(1));

        state.apply_network_update(
            vec![wifi("a", None), wifi("b", Some("aa:bb")), wifi("c", None)],
            None,
        );

        assert_eq!(state.ui.l_state.selected(), Some(1));
    }

    #[test]
    fn selection_follows_ssid_when_bssid_changes() {
        let mut state = state_with(vec![wifi("a", None), wifi("b", Some("aa:bb"))]);
        state.ui.l_state.select(Some(1));

        // "b" roamed to a different access point and moved position.
        state.apply_network_update(vec![wifi("b", Some("cc:dd")), wifi("a", None)], None);

        assert_eq!(state.ui.l_state.selected(), Some(0));
    }

    #[test]
    fn selection_resets_when_connection_changes() {
        let mut state = state_with(vec![wifi("a", None), wifi("b", None)]);
        state.ui.l_state.select(Some(1));

        state.apply_network_update(
            vec![wifi("a", None), wifi("b", None)],
            Some("a".to_string()),
        );

        assert_eq!(state.ui.l_state.selected(), Some(0));
        assert_eq!(state.network.connected_ssid.as_deref(), Some("a"));
    }

    #[test]
    fn selection_resets_to_top_when_network_disappears() {
        let mut state = state_with(vec![wifi("a", None), wifi("b", None), wifi("c", None)]);
        state.ui.l_state.select(Some(2));

        state.apply_network_update(vec![wifi("a", None), wifi("b", None)], None);

        assert_eq!(state.ui.l_state.selected(), Some(0));
    }

    #[test]
    fn no_previous_selection_selects_first_row() {
        let mut state = state_with(vec![wifi("a", None)]);

        state.apply_network_update(vec![wifi("a", None), wifi("b", None)], None);

        assert_eq!(state.ui.l_state.selected(), Some(0));
        assert_eq!(state.network.wifi_list.len(), 2);
    }
}
