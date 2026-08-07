//! iwd D-Bus adapter for Linux.

use super::{
    IWD_AGENT_MANAGER_INTERFACE, IWD_AGENT_PATH, IWD_DEVICE_INTERFACE, IWD_KNOWN_NETWORK_INTERFACE,
    IWD_NETWORK_INTERFACE, IWD_PATH, IWD_SERVICE, IWD_STATION_INTERFACE, ListenerSpec, WifiBackend,
    new_proxy, owned_object_path, system_connection, value_bool, value_string,
};
use crate::error::{WifiError, WifiResult};
use crate::wifi::types::WifiInfo;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, Proxy};
use zbus::names::OwnedInterfaceName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::linux_network_manager::{nm_frequency_to_channel, nm_frequency_to_hz};

type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>>;

#[derive(Clone, Debug)]
pub(crate) struct IwdBackend {
    pub(crate) station_path: String,
}

#[derive(Clone, Debug)]
struct NetworkRecord {
    path: String,
    ssid: String,
    network_type: String,
    known_network_path: Option<String>,
    signal: i16,
    is_saved: bool,
    is_connected: bool,
    auto_connect: bool,
}

#[derive(Clone, Debug, Default)]
struct StationDiagnostics {
    frequency_mhz: u32,
    bssid: Option<String>,
    link_speed: Option<u32>,
}

impl IwdBackend {
    pub(crate) fn discover(
        connection: &Connection,
        target_interface: Option<&str>,
    ) -> WifiResult<Self> {
        let objects = managed_objects(connection)?;
        for (path, interfaces) in objects {
            let Some(device) = interface_properties(&interfaces, IWD_DEVICE_INTERFACE) else {
                continue;
            };
            if !interfaces
                .keys()
                .any(|interface| interface.as_str() == IWD_STATION_INTERFACE)
            {
                continue;
            }
            let powered = device.get("Powered").and_then(value_bool).unwrap_or(false);
            let mode = device.get("Mode").and_then(value_string);
            if !powered || mode.as_deref() == Some("ap") {
                continue;
            }
            let usable_interface = device
                .get("Name")
                .and_then(value_string)
                .filter(|name| !name.is_empty());
            if let Some(usable) = usable_interface {
                if let Some(target) = target_interface {
                    if usable != target {
                        continue;
                    }
                }
                return Ok(Self {
                    station_path: path.to_string(),
                });
            }
        }
        Err(WifiError::MissingInterface {
            backend: target_interface
                .map(|iface| format!("iwd ({iface})"))
                .unwrap_or_else(|| "iwd".to_string()),
        })
    }

    fn station(&self, connection: &Connection) -> WifiResult<Proxy<'static>> {
        new_proxy(
            connection,
            IWD_SERVICE,
            &self.station_path,
            IWD_STATION_INTERFACE,
        )
    }

    fn network(&self, connection: &Connection, path: &str) -> WifiResult<Proxy<'static>> {
        new_proxy(connection, IWD_SERVICE, path, IWD_NETWORK_INTERFACE)
    }

    fn known_network(&self, connection: &Connection, path: &str) -> WifiResult<Proxy<'static>> {
        new_proxy(connection, IWD_SERVICE, path, IWD_KNOWN_NETWORK_INTERFACE)
    }

    fn ordered_networks(&self, connection: &Connection) -> WifiResult<Vec<NetworkRecord>> {
        let station = self.station(connection)?;
        let ordered: Vec<(OwnedObjectPath, i16)> = station
            .call("GetOrderedNetworks", &())
            .map_err(|e| WifiError::Dbus {
                operation: format!("enumerate iwd Wi-Fi networks: {e}"),
            })?;
        let mut records = Vec::new();
        for (path, signal) in ordered {
            let path = path.to_string();
            let network = match self.network(connection, &path) {
                Ok(network) => network,
                Err(_) => continue,
            };
            let ssid: String = match network.get_property("Name") {
                Ok(value) => value,
                Err(_) => continue,
            };
            if ssid.is_empty() {
                continue;
            }
            let network_type: String = network
                .get_property("Type")
                .unwrap_or_else(|_| "unknown".to_string());
            let is_connected = network.get_property::<bool>("Connected").unwrap_or(false);
            let known_path: Option<OwnedObjectPath> = network.get_property("KnownNetwork").ok();
            let (known_network_path, is_saved, auto_connect) = if let Some(known_path) = known_path
            {
                if let Ok(known) = self.known_network(connection, known_path.as_str()) {
                    let known_name = known.get_property::<String>("Name").ok();
                    let auto_connect = known.get_property::<bool>("AutoConnect").unwrap_or(true);
                    let is_saved = known_name
                        .as_deref()
                        .is_some_and(|name| iwd_saved_network_matches(name, &ssid));
                    (
                        is_saved.then(|| known_path.to_string()),
                        is_saved,
                        is_saved && auto_connect,
                    )
                } else {
                    (None, false, false)
                }
            } else {
                (None, false, false)
            };
            records.push(NetworkRecord {
                path,
                ssid,
                network_type,
                known_network_path,
                signal,
                is_saved,
                is_connected,
                auto_connect,
            });
        }
        Ok(records)
    }

    fn find_network(&self, connection: &Connection, ssid: &str) -> WifiResult<NetworkRecord> {
        self.ordered_networks(connection)?
            .into_iter()
            .find(|network| network.ssid == ssid)
            .ok_or_else(|| WifiError::NetworkNotFound {
                ssid: ssid.to_string(),
            })
    }

    fn connect_network(
        &self,
        connection: &Connection,
        network: &NetworkRecord,
        password: Option<&SecretString>,
    ) -> WifiResult<()> {
        if network.network_type == "wep" {
            return Err(WifiError::UnsupportedOperation {
                backend: "iwd".to_string(),
                operation: "WEP connections".to_string(),
            });
        }
        let state = password.map(|password| {
            Arc::new(Mutex::new(CredentialAgentState::new(
                Some(network.path.clone()),
                None,
                password.clone(),
            )))
        });
        if let Some(state) = &state {
            register_credential_agent(connection, state.clone())?;
        }
        let result = self
            .network(connection, &network.path)?
            .call::<_, _, ()>("Connect", &())
            .map_err(|e| WifiError::Dbus {
                operation: format!("connect an iwd network: {e}"),
            });
        if let Some(state) = state {
            unregister_credential_agent(connection, state);
        }
        result
    }

    fn connect_hidden(
        &self,
        connection: &Connection,
        ssid: &str,
        password: Option<&SecretString>,
    ) -> WifiResult<()> {
        let state = password.map(|password| {
            Arc::new(Mutex::new(CredentialAgentState::new(
                None,
                Some(ssid.to_string()),
                password.clone(),
            )))
        });
        if let Some(state) = &state {
            register_credential_agent(connection, state.clone())?;
        }
        let result = self
            .station(connection)?
            .call::<_, _, ()>("ConnectHiddenNetwork", &ssid)
            .map_err(|e| WifiError::Dbus {
                operation: format!("connect a hidden iwd network: {e}"),
            });
        if let Some(state) = state {
            unregister_credential_agent(connection, state);
        }
        result
    }

    fn station_diagnostics(&self, connection: &Connection) -> Option<StationDiagnostics> {
        let proxy = new_proxy(
            connection,
            IWD_SERVICE,
            &self.station_path,
            "net.connman.iwd.StationDiagnostic",
        )
        .ok()?;
        let diag: HashMap<String, OwnedValue> = proxy
            .call("GetDiagnostics", &())
            .or_else(|_| proxy.call("GetDiagnostic", &()))
            .ok()?;
        let freq_mhz = diag.get("Frequency").and_then(|v| {
            u32::try_from(v.clone())
                .ok()
                .or_else(|| u64::try_from(v.clone()).ok().map(|val| val as u32))
        })?;
        let link_speed = diag
            .get("RxRate")
            .or_else(|| diag.get("TxRate"))
            .or_else(|| diag.get("RxBitrate"))
            .or_else(|| diag.get("TxBitrate"))
            .and_then(|v| {
                u32::try_from(v.clone())
                    .ok()
                    .or_else(|| u64::try_from(v.clone()).ok().map(|val| val as u32))
            })
            .map(|rate_100kbps| rate_100kbps / 10);
        Some(StationDiagnostics {
            frequency_mhz: freq_mhz,
            bssid: diag
                .get("ConnectedBss")
                .or_else(|| diag.get("ConnectedBSS"))
                .and_then(value_string)
                .and_then(|value| normalize_bssid(&value)),
            link_speed,
        })
    }
}

impl WifiBackend for IwdBackend {
    fn name(&self) -> &'static str {
        "iwd"
    }

    fn listener_spec(&self) -> ListenerSpec {
        ListenerSpec::Iwd {
            station_path: self.station_path.clone(),
        }
    }

    fn connect_profile(&self, ssid: &str) -> WifiResult<()> {
        let connection = system_connection()?;
        let network = self.find_network(&connection, ssid)?;
        if !network.is_saved {
            return Err(WifiError::NetworkNotFound {
                ssid: ssid.to_string(),
            });
        }
        self.connect_network(&connection, &network, None)
    }

    fn connect_with_password(
        &self,
        ssid: &str,
        password: &SecretString,
        auth: &str,
        cipher: &str,
        hidden: bool,
    ) -> WifiResult<()> {
        if auth.to_ascii_lowercase().contains("wep")
            || auth.to_ascii_lowercase().contains("shared")
            || cipher.eq_ignore_ascii_case("wep")
        {
            return Err(WifiError::UnsupportedOperation {
                backend: "iwd".to_string(),
                operation: "WEP connections".to_string(),
            });
        }
        let connection = system_connection()?;
        if hidden {
            self.connect_hidden(&connection, ssid, Some(password))
        } else {
            let network = self.find_network(&connection, ssid)?;
            self.connect_network(&connection, &network, Some(password))
        }
    }

    fn connect_open(&self, ssid: &str, hidden: bool) -> WifiResult<()> {
        let connection = system_connection()?;
        if hidden {
            return self.connect_hidden(&connection, ssid, None);
        }
        let network = self.find_network(&connection, ssid)?;
        if network.network_type != "open" {
            return Err(WifiError::UnsupportedOperation {
                backend: "iwd".to_string(),
                operation: "connecting to a secured network without a password".to_string(),
            });
        }
        self.connect_network(&connection, &network, None)
    }

    fn disconnect(&self) -> WifiResult<()> {
        let connection = system_connection()?;
        self.station(&connection)?
            .call::<_, _, ()>("Disconnect", &())
            .map_err(|e| WifiError::Dbus {
                operation: format!("disconnect iwd Wi-Fi: {e}"),
            })
    }

    fn disconnect_and_wait(&self) -> WifiResult<()> {
        self.disconnect()?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if self.get_connected_ssid()?.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn get_connected_ssid(&self) -> WifiResult<Option<String>> {
        let connection = system_connection()?;
        let station = self.station(&connection)?;
        let network_path: OwnedObjectPath = match station.get_property("ConnectedNetwork") {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        if network_path.as_str() == "/" {
            return Ok(None);
        }
        let network = self.network(&connection, network_path.as_str())?;
        network
            .get_property("Name")
            .map(Some)
            .map_err(|e| WifiError::Dbus {
                operation: format!("read the iwd connected SSID: {e}"),
            })
    }

    fn get_wifi_networks(&self) -> WifiResult<Vec<WifiInfo>> {
        let connection = system_connection()?;
        let records = self.ordered_networks(&connection)?;
        let diagnostics = self.station_diagnostics(&connection);
        let mut networks = HashMap::<String, WifiInfo>::new();
        for record in records {
            let (authentication, encryption) = iwd_security_names(&record.network_type);
            let (phy_type, mut channel, mut frequency) = iwd_unknown_metadata();
            let mut link_speed = None;
            let mut bssid = None;
            if record.is_connected {
                if let Some(diagnostics) = &diagnostics {
                    frequency = nm_frequency_to_hz(diagnostics.frequency_mhz);
                    channel = nm_frequency_to_channel(diagnostics.frequency_mhz);
                    link_speed = diagnostics.link_speed;
                    bssid = diagnostics.bssid.clone();
                }
            }
            let info = WifiInfo {
                ssid: record.ssid.clone(),
                authentication: authentication.to_string(),
                encryption: encryption.to_string(),
                signal: iwd_signal_to_percent(record.signal),
                is_saved: record.is_saved,
                is_connected: record.is_connected,
                auto_connect: record.auto_connect,
                phy_type: phy_type.to_string(),
                channel,
                frequency,
                link_speed,
                bssid,
            };
            networks
                .entry(record.ssid)
                .and_modify(|current| {
                    let replace_radio = if info.is_connected != current.is_connected {
                        info.is_connected
                    } else {
                        info.signal > current.signal
                    };
                    if replace_radio {
                        current.signal = info.signal;
                        current.channel = info.channel;
                        current.frequency = info.frequency;
                        current.phy_type = info.phy_type.clone();
                        current.bssid = info.bssid.clone();
                    }
                    current.is_saved |= info.is_saved;
                    current.is_connected |= info.is_connected;
                    current.auto_connect |= info.auto_connect;
                    if info.is_connected {
                        current.link_speed = info.link_speed;
                        current.bssid = info.bssid.clone();
                    }
                })
                .or_insert(info);
        }
        let mut networks: Vec<_> = networks.into_values().collect();
        networks.sort_by(|left, right| {
            right
                .is_connected
                .cmp(&left.is_connected)
                .then_with(|| right.is_saved.cmp(&left.is_saved))
                .then_with(|| right.signal.cmp(&left.signal))
                .then_with(|| left.ssid.cmp(&right.ssid))
        });
        Ok(networks)
    }

    fn scan_networks(&self) -> WifiResult<()> {
        let connection = system_connection()?;
        self.station(&connection)?
            .call::<_, _, ()>("Scan", &())
            .map_err(|e| WifiError::Dbus {
                operation: format!("request an iwd Wi-Fi scan: {e}"),
            })
    }

    fn get_saved_profiles(&self) -> WifiResult<Vec<String>> {
        let connection = system_connection()?;
        let objects = managed_objects(&connection)?;
        let mut profiles = Vec::new();
        for (_path, interfaces) in objects {
            let Some(properties) = interface_properties(&interfaces, IWD_KNOWN_NETWORK_INTERFACE)
            else {
                continue;
            };
            if let Some(name) = properties.get("Name").and_then(value_string) {
                profiles.push(name);
            }
        }
        profiles.sort();
        profiles.dedup();
        Ok(profiles)
    }

    fn set_auto_connect(&self, ssid: &str, enable: bool) -> WifiResult<()> {
        let connection = system_connection()?;
        let network = self.find_network(&connection, ssid)?;
        let known_path = network
            .known_network_path
            .ok_or_else(|| WifiError::NetworkNotFound {
                ssid: ssid.to_string(),
            })?;
        let known = self.known_network(&connection, &known_path)?;
        known
            .set_property("AutoConnect", enable)
            .map_err(|e| WifiError::Dbus {
                operation: format!("update iwd auto-connect setting: {e}"),
            })
    }

    fn forget_network(&self, ssid: &str) -> WifiResult<()> {
        let connection = system_connection()?;
        let network = self.find_network(&connection, ssid)?;
        let known_path = network
            .known_network_path
            .ok_or_else(|| WifiError::NetworkNotFound {
                ssid: ssid.to_string(),
            })?;
        self.known_network(&connection, &known_path)?
            .call::<_, _, ()>("Forget", &())
            .map_err(|e| WifiError::Dbus {
                operation: format!("forget an iwd saved profile: {e}"),
            })
    }

    fn get_wifi_password(&self, _ssid: &str) -> WifiResult<Option<SecretString>> {
        Err(WifiError::UnsupportedOperation {
            backend: "iwd".to_string(),
            operation: "reading saved passwords through D-Bus".to_string(),
        })
    }
}

fn normalize_bssid(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn managed_objects(connection: &Connection) -> WifiResult<ManagedObjects> {
    let manager = new_proxy(
        connection,
        IWD_SERVICE,
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )?;
    manager
        .call("GetManagedObjects", &())
        .map_err(|e| WifiError::Dbus {
            operation: format!("enumerate iwd D-Bus objects: {e}"),
        })
}

fn interface_properties<'a>(
    interfaces: &'a HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>,
    name: &str,
) -> Option<&'a HashMap<String, OwnedValue>> {
    interfaces
        .iter()
        .find(|(interface, _)| interface.as_str() == name)
        .map(|(_, properties)| properties)
}

pub(crate) fn iwd_signal_to_percent(signal: i16) -> u8 {
    let signal = signal.clamp(-10_000, 0);
    (((10_000 + signal) as u32 * 100) / 10_000) as u8
}

pub(crate) fn iwd_security_names(network_type: &str) -> (&'static str, &'static str) {
    match network_type {
        "open" => ("Open", "None"),
        "wep" => ("Shared", "WEP"),
        "psk" => ("WPA2-PSK", "AES"),
        "8021x" => ("WPA2", "AES"),
        _ => ("Unknown", "Unknown"),
    }
}

pub(crate) fn iwd_unknown_metadata() -> (&'static str, u32, u64) {
    ("Unknown", 0, 0)
}

fn encoded_ssid(ssid: &str) -> String {
    ssid.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn iwd_network_path_matches_ssid(path: &str, ssid: &str) -> bool {
    let Some(last_component) = path.rsplit('/').next() else {
        return false;
    };
    last_component
        .strip_prefix(&encoded_ssid(ssid))
        .is_some_and(|suffix| suffix.starts_with('_'))
}

pub(crate) fn iwd_saved_network_matches(known_name: &str, network_ssid: &str) -> bool {
    known_name == network_ssid
}

#[derive(Debug)]
pub(crate) struct CredentialAgentState {
    expected_network: Option<String>,
    expected_ssid: Option<String>,
    password: Option<SecretString>,
}

impl CredentialAgentState {
    pub(crate) fn new(
        expected_network: Option<String>,
        expected_ssid: Option<String>,
        password: SecretString,
    ) -> Self {
        Self {
            expected_network,
            expected_ssid,
            password: Some(password),
        }
    }

    pub(crate) fn take_for_network(&mut self, network: &str) -> Option<String> {
        let matches = self
            .expected_network
            .as_deref()
            .is_some_and(|expected| expected == network)
            || self
                .expected_ssid
                .as_deref()
                .is_some_and(|ssid| iwd_network_path_matches_ssid(network, ssid));
        if matches {
            self.password
                .take()
                .map(|password| password.expose_secret().to_string())
        } else {
            None
        }
    }

    pub(crate) fn clear(&mut self) {
        self.password = None;
    }
}

#[derive(Debug, Clone)]
struct CredentialAgent {
    state: Arc<Mutex<CredentialAgentState>>,
}

impl CredentialAgent {
    fn new(state: Arc<Mutex<CredentialAgentState>>) -> Self {
        Self { state }
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "net.connman.iwd.Agent.Error")]
enum CredentialAgentError {
    Canceled(String),
}

fn canceled() -> CredentialAgentError {
    CredentialAgentError::Canceled(
        "credential request was not for the selected network".to_string(),
    )
}

#[zbus::interface(name = "net.connman.iwd.Agent")]
impl CredentialAgent {
    fn release(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.clear();
        }
    }

    fn request_passphrase(
        &self,
        network: OwnedObjectPath,
    ) -> std::result::Result<String, CredentialAgentError> {
        self.state
            .lock()
            .map_err(|_| canceled())?
            .take_for_network(network.as_str())
            .ok_or_else(canceled)
    }

    fn request_private_key_passphrase(
        &self,
        _network: OwnedObjectPath,
    ) -> std::result::Result<String, CredentialAgentError> {
        Err(canceled())
    }

    fn request_user_name_and_password(
        &self,
        _network: OwnedObjectPath,
    ) -> std::result::Result<(String, String), CredentialAgentError> {
        Err(canceled())
    }

    fn request_user_password(
        &self,
        _network: OwnedObjectPath,
        _user: &str,
    ) -> std::result::Result<String, CredentialAgentError> {
        Err(canceled())
    }

    fn cancel(&self, _reason: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.clear();
        }
    }
}

fn register_credential_agent(
    connection: &Connection,
    state: Arc<Mutex<CredentialAgentState>>,
) -> WifiResult<()> {
    connection
        .object_server()
        .at(IWD_AGENT_PATH, CredentialAgent::new(state))
        .map_err(|e| WifiError::Dbus {
            operation: format!("register the temporary iwd credential agent: {e}"),
        })?;
    let manager = new_proxy(
        connection,
        IWD_SERVICE,
        IWD_PATH,
        IWD_AGENT_MANAGER_INTERFACE,
    )?;
    let path = owned_object_path(IWD_AGENT_PATH)?;
    manager
        .call::<_, _, ()>("RegisterAgent", &path)
        .map_err(|e| WifiError::Dbus {
            operation: format!("register the temporary iwd credential agent: {e}"),
        })
}

fn unregister_credential_agent(connection: &Connection, state: Arc<Mutex<CredentialAgentState>>) {
    if let Ok(mut state) = state.lock() {
        state.clear();
    }
    if let Ok(manager) = new_proxy(
        connection,
        IWD_SERVICE,
        IWD_PATH,
        IWD_AGENT_MANAGER_INTERFACE,
    ) {
        if let Ok(path) = owned_object_path(IWD_AGENT_PATH) {
            let _ = manager.call::<_, _, ()>("UnregisterAgent", &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_iwd_signal_strength_to_percent() {
        assert_eq!(iwd_signal_to_percent(0), 100);
        assert_eq!(iwd_signal_to_percent(-5_000), 50);
        assert_eq!(iwd_signal_to_percent(-10_000), 0);
        assert_eq!(iwd_signal_to_percent(100), 100);
    }

    #[test]
    fn maps_iwd_security_names() {
        assert_eq!(iwd_security_names("open"), ("Open", "None"));
        assert_eq!(iwd_security_names("psk"), ("WPA2-PSK", "AES"));
        assert_eq!(iwd_security_names("wep"), ("Shared", "WEP"));
    }

    #[test]
    fn iwd_metadata_is_explicitly_unknown() {
        assert_eq!(iwd_unknown_metadata(), ("Unknown", 0, 0));
    }

    #[test]
    fn credential_agent_only_returns_matching_network_once() {
        let mut state = CredentialAgentState::new(
            Some("/net/connman/iwd/phy0/1/network_psk".to_string()),
            None,
            SecretString::from("secret"),
        );
        assert!(
            state
                .take_for_network("/net/connman/iwd/phy0/1/other_psk")
                .is_none()
        );
        assert_eq!(
            state.take_for_network("/net/connman/iwd/phy0/1/network_psk"),
            Some("secret".to_string())
        );
        assert!(
            state
                .take_for_network("/net/connman/iwd/phy0/1/network_psk")
                .is_none()
        );
        state.clear();
    }

    #[test]
    fn hidden_credential_agent_matches_encoded_ssid() {
        let mut state =
            CredentialAgentState::new(None, Some("Test".to_string()), SecretString::from("secret"));
        assert_eq!(
            state.take_for_network("/net/connman/iwd/phy0/1/54657374_psk"),
            Some("secret".to_string())
        );
    }

    #[test]
    fn matches_saved_networks_by_name() {
        assert!(iwd_saved_network_matches("home", "home"));
        assert!(!iwd_saved_network_matches("home", "guest"));
    }
}
