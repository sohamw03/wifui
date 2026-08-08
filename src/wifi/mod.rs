//! Platform-independent Wi-Fi facade for WifUI.
//!
//! The frontend talks to this module through a stable API. The Windows backend
//! uses the native WLAN APIs. Linux selects a D-Bus adapter at startup, while
//! other platforms expose a typed unsupported-backend result.

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
#[allow(unused_imports)]
pub use profile::{forget_network, get_saved_profiles, get_wifi_password, set_auto_connect};
#[cfg(windows)]
pub use scanning::scan_networks;

#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub use linux::{
    BackendChoice, WifiListener, connect_open, connect_profile, connect_with_password, disconnect,
    disconnect_and_wait, forget_network, get_connected_ssid, get_saved_profiles, get_wifi_networks,
    get_wifi_password, initialize_backend, scan_networks, set_auto_connect, start_wifi_listener,
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
    #[cfg(target_os = "linux")]
    {
        linux::is_backend_available()
    }

    #[cfg(windows)]
    {
        true
    }

    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        false
    }
}

/// Name of the currently selected backend, if one has been initialized.
pub fn backend_name() -> String {
    #[cfg(target_os = "linux")]
    {
        return linux::backend_name();
    }

    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        return "unsupported".to_string();
    }

    #[cfg(windows)]
    {
        "wlan".to_string()
    }
}

/// Message shown when the current target has no Wi-Fi backend.
pub fn backend_unavailable_message() -> String {
    #[cfg(target_os = "linux")]
    {
        return linux::backend_unavailable_message();
    }

    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        return "Wi-Fi backend is not implemented for this platform".to_string();
    }

    #[cfg(windows)]
    {
        "Wi-Fi backend unavailable".to_string()
    }
}
