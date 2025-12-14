use crate::wifi::{get_connected_ssid, WifiInfo};
use color_eyre::eyre::Result;
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;

#[derive(Debug)]
pub struct AppState {
    pub wifi_list: Vec<WifiInfo>,
    pub l_state: ListState,
    pub connected_ssid: Option<String>,
    pub show_password_popup: bool,
    pub password_input: String,
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
}

impl AppState {
    pub fn new(wifi_list: Vec<WifiInfo>) -> AppState {
        AppState {
            wifi_list,
            l_state: ListState::default().with_selected(Some(0)),
            connected_ssid: get_connected_ssid().unwrap_or(None),
            show_password_popup: false,
            password_input: String::new(),
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
        }
    }

    pub fn next(&mut self) {
        let i = match self.l_state.selected() {
            Some(i) => {
                if i >= self.wifi_list.len() - 1 {
                    0
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
                    self.wifi_list.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.l_state.select(Some(i));
    }
}
