//! WiFi management module for WifUI
//!
//! This module provides functionality for managing WiFi connections on Windows,
//! including scanning, connecting, disconnecting, and monitoring connection events.

mod connection;
mod handle;
mod listener;
mod profile;
mod scanning;
mod types;

// Re-export public API
pub use connection::{
    connect_open, connect_profile, connect_with_password, disconnect, get_connected_ssid,
    get_wifi_networks,
};
pub use listener::{WifiListener, start_wifi_listener};
pub use profile::{forget_network, get_saved_profiles, set_auto_connect};
pub use scanning::scan_networks;
pub use types::{ConnectionEvent, WifiInfo};
