//! Placeholder Linux Wi-Fi backend.
//!
//! Linux compilation and the TUI are supported experimentally. Actual Wi-Fi
//! management will be added in a later milestone.

use crate::error::{WifiError, WifiResult};
use crate::wifi::types::{ConnectionEvent, WifiInfo};
use secrecy::SecretString;
use tokio::sync::mpsc::UnboundedSender;

fn unsupported<T>() -> WifiResult<T> {
    Err(WifiError::UnsupportedPlatform)
}

/// Placeholder event listener kept compatible with the frontend API.
#[derive(Debug)]
pub struct WifiListener;

pub fn start_wifi_listener(_sender: UnboundedSender<ConnectionEvent>) -> WifiResult<WifiListener> {
    unsupported()
}

pub fn connect_profile(_ssid: &str) -> WifiResult<()> {
    unsupported()
}

pub fn connect_with_password(
    _ssid: &str,
    _password: &SecretString,
    _auth: &str,
    _cipher: &str,
    _hidden: bool,
) -> WifiResult<()> {
    unsupported()
}

pub fn connect_open(_ssid: &str, _hidden: bool) -> WifiResult<()> {
    unsupported()
}

pub fn disconnect() -> WifiResult<()> {
    unsupported()
}

pub fn disconnect_and_wait() -> WifiResult<()> {
    unsupported()
}

pub fn get_connected_ssid() -> WifiResult<Option<String>> {
    unsupported()
}

pub fn get_wifi_networks() -> WifiResult<Vec<WifiInfo>> {
    unsupported()
}

pub fn scan_networks() -> WifiResult<()> {
    unsupported()
}

pub fn get_saved_profiles() -> WifiResult<Vec<String>> {
    unsupported()
}

pub fn set_auto_connect(_ssid: &str, _enable: bool) -> WifiResult<()> {
    unsupported()
}

pub fn forget_network(_ssid: &str) -> WifiResult<()> {
    unsupported()
}

pub fn get_wifi_password(_ssid: &str) -> WifiResult<Option<SecretString>> {
    unsupported()
}
