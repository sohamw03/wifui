use crate::{
    input::InputState,
    wifi::{ConnectionEvent, WifiInfo, WifiListener, get_connected_ssid, start_wifi_listener},
};
use color_eyre::eyre::Result;
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver};

#[derive(Debug)]
pub struct AppState {
    pub wifi_list: Vec<WifiInfo>,
    pub filtered_wifi_list: Vec<WifiInfo>,
    pub l_state: ListState,
    pub connected_ssid: Option<String>,
    pub show_password_popup: bool,
    pub password_input: InputState,
    pub search_input: InputState,
    pub is_searching: bool,
    pub connecting_to_ssid: Option<String>,
    pub last_refresh: Instant,
    pub last_interaction: Instant,
    pub is_refreshing_networks: bool,
    pub network_update_rx: Option<Receiver<Result<(Vec<WifiInfo>, Option<String>)>>>,
    pub error_message: Option<String>,
    pub is_connecting: bool,
    pub loading_frame: usize,
    pub connection_result_rx: Option<Receiver<Result<()>>>,
    pub refresh_burst: u8,
    pub target_ssid: Option<String>,
    pub connection_start_time: Option<Instant>,
    pub show_key_logger: bool,
    pub last_key_press: Option<(String, Instant)>,
    pub show_manual_add_popup: bool,
    pub manual_ssid_input: InputState,
    pub manual_password_input: InputState,
    pub manual_security: String,
    pub manual_hidden: bool,
    pub manual_input_field: usize,
    #[allow(dead_code)]
    pub wifi_listener: Option<WifiListener>,
    pub connection_event_rx: Option<UnboundedReceiver<ConnectionEvent>>,
}

impl AppState {
    pub fn new(wifi_list: Vec<WifiInfo>, show_key_logger: bool) -> AppState {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let wifi_listener = start_wifi_listener(tx).ok();

        AppState {
            filtered_wifi_list: wifi_list.clone(),
            wifi_list: wifi_list.clone(),
            l_state: ListState::default().with_selected(Some(0)),
            connected_ssid: get_connected_ssid().unwrap_or(None),
            show_password_popup: false,
            password_input: InputState::new(),
            search_input: InputState::new(),
            is_searching: false,
            connecting_to_ssid: None,
            last_refresh: Instant::now() - Duration::from_secs(15), // Force immediate refresh
            last_interaction: Instant::now(),
            is_refreshing_networks: false,
            network_update_rx: None,
            error_message: None,
            is_connecting: false,
            loading_frame: 0,
            connection_result_rx: None,
            refresh_burst: 5, // Burst refresh on startup to catch scan results
            target_ssid: None,
            connection_start_time: None,
            show_key_logger,
            last_key_press: None,
            show_manual_add_popup: false,
            manual_ssid_input: InputState::new(),
            manual_password_input: InputState::new(),
            manual_security: "WPA2-PSK".to_string(),
            manual_hidden: false,
            manual_input_field: 0,
            wifi_listener,
            connection_event_rx: Some(rx),
        }
    }

    pub fn next(&mut self) {
        let i = match self.l_state.selected() {
            Some(i) => {
                if i >= self.filtered_wifi_list.len().saturating_sub(1) {
                    i
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.l_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.l_state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.l_state.select(Some(i));
    }

    pub fn go_to_top(&mut self) {
        if !self.filtered_wifi_list.is_empty() {
            self.l_state.select(Some(0));
        }
    }

    pub fn go_to_bottom(&mut self) {
        if !self.filtered_wifi_list.is_empty() {
            self.l_state.select(Some(self.filtered_wifi_list.len() - 1));
        }
    }

    pub fn update_filtered_list(&mut self) {
        if self.search_input.value.is_empty() {
            self.filtered_wifi_list = self.wifi_list.clone();
        } else {
            let search_lower = self.search_input.value.to_lowercase();
            self.filtered_wifi_list = self
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
        if let Some(selected) = self.l_state.selected() {
            if selected >= self.filtered_wifi_list.len() {
                self.l_state.select(Some(0));
            }
        }
    }
}
