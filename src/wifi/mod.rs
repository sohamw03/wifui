//! Platform-independent Wi-Fi facade for WifUI.
//!
//! The frontend talks to this module through a stable API. The Windows backend
//! uses the native WLAN APIs, while other platforms currently expose a typed
//! unsupported-backend result.

mod types;

#[cfg(windows)]
mod connection;
#[cfg(windows)]
mod handle;
#[cfg(windows)]
mod listener;
#[cfg(windows)]
mod profile;
#[cfg(windows)]
mod scanning;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(all(not(windows), not(target_os = "linux")))]
mod unsupported;

#[cfg(windows)]
pub use connection::{
    connect_open, connect_profile, connect_with_password, disconnect, disconnect_and_wait,
    get_connected_ssid, get_wifi_networks,
};
#[cfg(windows)]
pub use listener::{WifiListener, start_wifi_listener};
#[cfg(windows)]
pub use profile::{forget_network, get_saved_profiles, get_wifi_password, set_auto_connect};
#[cfg(windows)]
pub use scanning::scan_networks;

#[cfg(target_os = "linux")]
pub use linux::{
    WifiListener, connect_open, connect_profile, connect_with_password, disconnect,
    disconnect_and_wait, forget_network, get_connected_ssid, get_saved_profiles, get_wifi_networks,
    get_wifi_password, scan_networks, set_auto_connect, start_wifi_listener,
};
#[cfg(all(not(windows), not(target_os = "linux")))]
pub use unsupported::{
    WifiListener, connect_open, connect_profile, connect_with_password, disconnect,
    disconnect_and_wait, forget_network, get_connected_ssid, get_saved_profiles, get_wifi_networks,
    get_wifi_password, scan_networks, set_auto_connect, start_wifi_listener,
};

pub use types::{ConnectionEvent, WifiInfo};

/// Whether the current target has a functional Wi-Fi backend.
pub fn is_backend_available() -> bool {
    cfg!(windows)
}

/// Message shown when the current target has no Wi-Fi backend yet.
pub fn backend_unavailable_message() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "Linux Wi-Fi backend is not implemented yet"
    }

    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        "Wi-Fi backend is not implemented for this platform"
    }

    #[cfg(windows)]
    {
        "Wi-Fi backend unavailable"
    }
}
