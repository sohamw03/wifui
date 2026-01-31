use crate::{
    config::{self, IconSet},
    input::InputState,
    wifi::{ConnectionEvent, WifiInfo, WifiListener, start_wifi_listener},
};
use color_eyre::eyre::Result;
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver};

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
    pub connecting_to_ssid: Option<String>,
    pub target_ssid: Option<String>,
    pub connection_start_time: Option<Instant>,
    pub connection_result_rx: Option<Receiver<Result<()>>>,
    #[allow(dead_code)]
    pub wifi_listener: Option<WifiListener>,
    pub connection_event_rx: Option<UnboundedReceiver<ConnectionEvent>>,
}

impl ConnectionState {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let wifi_listener = start_wifi_listener(tx).ok();

        Self {
            is_connecting: false,
            connecting_to_ssid: None,
            target_ssid: None,
            connection_start_time: None,
            connection_result_rx: None,
            wifi_listener,
            connection_event_rx: Some(rx),
        }
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
            manual_security: "WPA2-PSK".to_string(),
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

/// Refresh and timing state
#[derive(Debug)]
pub struct RefreshState {
    pub last_refresh: Instant,
    pub last_interaction: Instant,
    pub last_manual_refresh: Instant,
    pub is_refreshing_networks: bool,
    pub network_update_rx: Option<Receiver<Result<(Vec<WifiInfo>, Option<String>)>>>,
    pub refresh_burst: u8,
    pub startup_time: Instant,
    pub auto_connect_attempted: bool,
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
            startup_time: Instant::now(),
            auto_connect_attempted: false,
        }
    }
}

/// Main application state
#[derive(Debug)]
pub struct AppState {
    pub network: NetworkState,
    pub ui: UiState,
    pub connection: ConnectionState,
    pub inputs: InputStates,
    pub refresh: RefreshState,
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
        if let Some(selected) = self.ui.l_state.selected() {
            if selected >= self.network.filtered_wifi_list.len() {
                self.ui.l_state.select(Some(0));
            }
        }
    }

    /// Check if any popup is open (for dimming the background)
    pub fn is_popup_open(&self) -> bool {
        self.ui.show_manual_add_popup || self.ui.show_password_popup || self.ui.show_qr_popup
    }

    /// Find the strongest saved network with auto_connect enabled
    /// Returns Some(ssid) if found, None otherwise
    pub fn find_best_auto_connect_network(&self) -> Option<String> {
        self.network
            .wifi_list
            .iter()
            .filter(|w| w.is_saved && w.auto_connect)
            .max_by_key(|w| w.signal)
            .map(|w| w.ssid.clone())
    }
}
