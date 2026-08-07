//! Linux Wi-Fi backend dispatcher.
//!
//! Linux support is selected at runtime because NetworkManager and iwd expose
//! different, mutually exclusive control surfaces. Both adapters implement the
//! same small trait so the rest of the application continues to use the stable
//! facade in `wifi::mod`.

#[path = "linux_iwd.rs"]
mod linux_iwd;
#[path = "linux_listener.rs"]
mod linux_listener;
#[path = "linux_network_manager.rs"]
mod linux_network_manager;

use crate::error::{WifiError, WifiResult};
use crate::wifi::types::{ConnectionEvent, WifiInfo};
use linux_iwd::IwdBackend;
use linux_network_manager::NetworkManagerBackend;
use secrecy::SecretString;
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::mpsc::UnboundedSender;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant;
use zbus::zvariant::OwnedValue;

pub(crate) const NETWORK_MANAGER_SERVICE: &str = "org.freedesktop.NetworkManager";
pub(crate) const NETWORK_MANAGER_PATH: &str = "/org/freedesktop/NetworkManager";
pub(crate) const NETWORK_MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager";
pub(crate) const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
pub(crate) const NM_DEVICE_WIFI_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub(crate) const NM_ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
pub(crate) const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
pub(crate) const NM_SETTINGS_INTERFACE: &str = "org.freedesktop.NetworkManager.Settings";
pub(crate) const NM_CONNECTION_INTERFACE: &str =
    "org.freedesktop.NetworkManager.Settings.Connection";

pub(crate) const IWD_SERVICE: &str = "net.connman.iwd";
pub(crate) const IWD_PATH: &str = "/net/connman/iwd";
pub(crate) const IWD_STATION_INTERFACE: &str = "net.connman.iwd.Station";
pub(crate) const IWD_DEVICE_INTERFACE: &str = "net.connman.iwd.Device";
pub(crate) const IWD_NETWORK_INTERFACE: &str = "net.connman.iwd.Network";
pub(crate) const IWD_KNOWN_NETWORK_INTERFACE: &str = "net.connman.iwd.KnownNetwork";
pub(crate) const IWD_AGENT_MANAGER_INTERFACE: &str = "net.connman.iwd.AgentManager";
pub(crate) const IWD_AGENT_PATH: &str = "/org/wifui/IwdAgent";

/// Runtime backend selected by the `--backend` argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum BackendChoice {
    /// Prefer NetworkManager and then iwd.
    Auto,
    /// Use NetworkManager and fail if it is unavailable.
    Nm,
    /// Use iwd and fail if it is unavailable.
    Iwd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectedBackend {
    NetworkManager,
    Iwd,
}

/// A selected adapter's immutable target. D-Bus connections are intentionally
/// opened per operation; this keeps the existing synchronous frontend calls
/// independent from the long-lived listener connection.
pub(crate) trait WifiBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn listener_spec(&self) -> ListenerSpec;
    fn connect_profile(&self, ssid: &str) -> WifiResult<()>;
    fn connect_with_password(
        &self,
        ssid: &str,
        password: &SecretString,
        auth: &str,
        cipher: &str,
        hidden: bool,
    ) -> WifiResult<()>;
    fn connect_open(&self, ssid: &str, hidden: bool) -> WifiResult<()>;
    fn disconnect(&self) -> WifiResult<()>;
    fn disconnect_and_wait(&self) -> WifiResult<()>;
    fn get_connected_ssid(&self) -> WifiResult<Option<String>>;
    fn get_wifi_networks(&self) -> WifiResult<Vec<WifiInfo>>;
    fn scan_networks(&self) -> WifiResult<()>;
    #[allow(dead_code)]
    fn get_saved_profiles(&self) -> WifiResult<Vec<String>>;
    fn set_auto_connect(&self, ssid: &str, enable: bool) -> WifiResult<()>;
    fn forget_network(&self, ssid: &str) -> WifiResult<()>;
    fn get_wifi_password(&self, ssid: &str) -> WifiResult<Option<SecretString>>;
}

#[derive(Clone, Debug)]
pub(crate) enum ListenerSpec {
    NetworkManager { device_path: String },
    Iwd { station_path: String },
}

struct BackendStatus {
    available: bool,
    name: String,
    message: String,
}

struct BackendRegistry {
    backend: Option<Arc<dyn WifiBackend>>,
    status: BackendStatus,
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self {
            backend: None,
            status: BackendStatus {
                available: false,
                name: "none".to_string(),
                message: "Linux Wi-Fi backend unavailable; start NetworkManager or iwd".to_string(),
            },
        }
    }
}

static REGISTRY: OnceLock<RwLock<BackendRegistry>> = OnceLock::new();

fn registry() -> &'static RwLock<BackendRegistry> {
    REGISTRY.get_or_init(|| RwLock::new(BackendRegistry::default()))
}

pub(crate) fn system_connection() -> WifiResult<Connection> {
    Connection::system().map_err(|e| WifiError::dbus("connect to the system bus", &e))
}

pub(crate) fn new_proxy(
    connection: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
) -> WifiResult<Proxy<'static>> {
    Proxy::new_owned(
        connection.clone(),
        destination.to_string(),
        path.to_string(),
        interface.to_string(),
    )
    .map_err(|e| WifiError::dbus(&format!("create {interface} proxy"), &e))
}

pub(crate) fn owned_object_path(path: &str) -> WifiResult<zvariant::OwnedObjectPath> {
    zvariant::OwnedObjectPath::try_from(path.to_string())
        .map_err(|e| WifiError::dbus(&format!("build D-Bus object path ({path})"), &e))
}

pub(crate) fn owned_value<T>(value: T) -> OwnedValue
where
    T: Into<zvariant::Value<'static>>,
{
    let value: zvariant::Value<'static> = value.into();
    OwnedValue::try_from(value)
        .expect("values used in NetworkManager settings are valid D-Bus values")
}

pub(crate) fn value_string(value: &OwnedValue) -> Option<String> {
    String::try_from(value.clone()).ok()
}

pub(crate) fn value_bytes(value: &OwnedValue) -> Option<Vec<u8>> {
    Vec::<u8>::try_from(value.clone()).ok()
}

pub(crate) fn value_bool(value: &OwnedValue) -> Option<bool> {
    bool::try_from(value.clone()).ok()
}

fn service_has_owner(connection: &Connection, service: &str) -> WifiResult<bool> {
    let proxy = new_proxy(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    proxy
        .call("NameHasOwner", &service)
        .map_err(|e| WifiError::dbus(&format!("check whether {service} is running"), &e))
}

fn selected_backend_for(
    choice: BackendChoice,
    network_manager_available: bool,
    iwd_available: bool,
) -> Option<SelectedBackend> {
    match choice {
        BackendChoice::Auto => {
            if network_manager_available {
                Some(SelectedBackend::NetworkManager)
            } else if iwd_available {
                Some(SelectedBackend::Iwd)
            } else {
                None
            }
        }
        BackendChoice::Nm if network_manager_available => Some(SelectedBackend::NetworkManager),
        BackendChoice::Iwd if iwd_available => Some(SelectedBackend::Iwd),
        _ => None,
    }
}

fn backend_unavailable_for(choice: BackendChoice) -> WifiError {
    let backend = match choice {
        BackendChoice::Auto => "NetworkManager or iwd",
        BackendChoice::Nm => "NetworkManager",
        BackendChoice::Iwd => "iwd",
    };
    WifiError::BackendUnavailable {
        backend: backend.to_string(),
    }
}

/// Discover and store the selected Linux adapter.
pub fn initialize_backend(
    choice: BackendChoice,
    target_interface: Option<&str>,
) -> WifiResult<&'static str> {
    let connection = system_connection()?;
    let network_manager_available = service_has_owner(&connection, NETWORK_MANAGER_SERVICE)?;
    let iwd_available = service_has_owner(&connection, IWD_SERVICE)?;

    if choice == BackendChoice::Auto {
        let mut last_error = None;

        if network_manager_available {
            match NetworkManagerBackend::discover(&connection, target_interface) {
                Ok(backend) => return store_backend(Arc::new(backend)),
                Err(error) => last_error = Some(error),
            }
        }
        if iwd_available {
            match IwdBackend::discover(&connection, target_interface) {
                Ok(backend) => return store_backend(Arc::new(backend)),
                Err(error) => last_error = Some(error),
            }
        }

        return Err(last_error.unwrap_or_else(|| backend_unavailable_for(choice)));
    }

    let selected = selected_backend_for(choice, network_manager_available, iwd_available)
        .ok_or_else(|| backend_unavailable_for(choice))?;

    let backend: Arc<dyn WifiBackend> = match selected {
        SelectedBackend::NetworkManager => Arc::new(NetworkManagerBackend::discover(
            &connection,
            target_interface,
        )?),
        SelectedBackend::Iwd => Arc::new(IwdBackend::discover(&connection, target_interface)?),
    };
    store_backend(backend)
}

fn store_backend(backend: Arc<dyn WifiBackend>) -> WifiResult<&'static str> {
    let name = backend.name();
    let mut state = registry()
        .write()
        .map_err(|_| WifiError::Internal("Linux backend registry was poisoned".to_string()))?;
    state.backend = Some(backend);
    state.status = BackendStatus {
        available: true,
        name: name.to_string(),
        message: format!("Linux backend: {name}"),
    };
    Ok(name)
}

pub(crate) fn active_backend() -> WifiResult<Arc<dyn WifiBackend>> {
    let state = registry()
        .read()
        .map_err(|_| WifiError::Internal("Linux backend registry was poisoned".to_string()))?;
    state
        .backend
        .clone()
        .ok_or_else(|| WifiError::BackendUnavailable {
            backend: "Linux Wi-Fi".to_string(),
        })
}

pub fn is_backend_available() -> bool {
    registry()
        .read()
        .map(|state| state.status.available)
        .unwrap_or(false)
}

pub fn backend_name() -> String {
    registry()
        .read()
        .map(|state| state.status.name.clone())
        .unwrap_or_else(|_| "none".to_string())
}

pub fn backend_unavailable_message() -> String {
    registry()
        .read()
        .map(|state| state.status.message.clone())
        .unwrap_or_else(|_| "Linux Wi-Fi backend unavailable".to_string())
}

pub struct WifiListener {
    _inner: linux_listener::WifiListener,
}

impl std::fmt::Debug for WifiListener {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("WifiListener").finish()
    }
}

pub fn start_wifi_listener(sender: UnboundedSender<ConnectionEvent>) -> WifiResult<WifiListener> {
    let backend = active_backend()?;
    Ok(WifiListener {
        _inner: linux_listener::start(backend.listener_spec(), sender)?,
    })
}

pub fn connect_profile(ssid: &str) -> WifiResult<()> {
    active_backend()?.connect_profile(ssid)
}

pub fn connect_with_password(
    ssid: &str,
    password: &SecretString,
    auth: &str,
    cipher: &str,
    hidden: bool,
) -> WifiResult<()> {
    active_backend()?.connect_with_password(ssid, password, auth, cipher, hidden)
}

pub fn connect_open(ssid: &str, hidden: bool) -> WifiResult<()> {
    active_backend()?.connect_open(ssid, hidden)
}

pub fn disconnect() -> WifiResult<()> {
    active_backend()?.disconnect()
}

pub fn disconnect_and_wait() -> WifiResult<()> {
    active_backend()?.disconnect_and_wait()
}

pub fn get_connected_ssid() -> WifiResult<Option<String>> {
    active_backend()?.get_connected_ssid()
}

pub fn get_wifi_networks() -> WifiResult<Vec<WifiInfo>> {
    active_backend()?.get_wifi_networks()
}

pub fn scan_networks() -> WifiResult<()> {
    active_backend()?.scan_networks()
}

#[allow(dead_code)]
pub fn get_saved_profiles() -> WifiResult<Vec<String>> {
    active_backend()?.get_saved_profiles()
}

pub fn set_auto_connect(ssid: &str, enable: bool) -> WifiResult<()> {
    active_backend()?.set_auto_connect(ssid, enable)
}

pub fn forget_network(ssid: &str) -> WifiResult<()> {
    active_backend()?.forget_network(ssid)
}

pub fn get_wifi_password(ssid: &str) -> WifiResult<Option<SecretString>> {
    active_backend()?.get_wifi_password(ssid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_selection_prefers_network_manager() {
        assert_eq!(
            selected_backend_for(BackendChoice::Auto, true, true),
            Some(SelectedBackend::NetworkManager)
        );
    }

    #[test]
    fn explicit_selection_does_not_fall_back() {
        assert_eq!(selected_backend_for(BackendChoice::Nm, false, true), None);
        assert_eq!(selected_backend_for(BackendChoice::Iwd, true, false), None);
    }

    #[test]
    fn auto_selection_reports_no_daemon() {
        assert_eq!(
            selected_backend_for(BackendChoice::Auto, false, false),
            None
        );
    }
}
