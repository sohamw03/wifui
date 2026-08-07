//! NetworkManager D-Bus adapter for Linux.

use super::{
    ListenerSpec, NETWORK_MANAGER_INTERFACE, NETWORK_MANAGER_PATH, NETWORK_MANAGER_SERVICE,
    NM_ACCESS_POINT_INTERFACE, NM_CONNECTION_INTERFACE, NM_DEVICE_INTERFACE,
    NM_DEVICE_WIFI_INTERFACE, NM_SETTINGS_INTERFACE, NM_SETTINGS_PATH, WifiBackend, new_proxy,
    owned_object_path, owned_value, system_connection, value_bool, value_bytes, value_string,
};
use crate::error::{WifiError, WifiResult};
use crate::wifi::types::WifiInfo;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const DEVICE_TYPE_WIFI: u32 = 2;
const DEVICE_STATE_ACTIVATED: u32 = 100;
const AP_FLAGS_PRIVACY: u32 = 0x0000_0001;
const SEC_PAIR_WEP40: u32 = 0x0000_0001;
const SEC_PAIR_WEP104: u32 = 0x0000_0002;
const SEC_PAIR_TKIP: u32 = 0x0000_0004;
const SEC_PAIR_CCMP: u32 = 0x0000_0008;
const SEC_GROUP_WEP40: u32 = 0x0000_0010;
const SEC_GROUP_WEP104: u32 = 0x0000_0020;
const SEC_GROUP_TKIP: u32 = 0x0000_0040;
const SEC_GROUP_CCMP: u32 = 0x0000_0080;
const SEC_KEY_MGMT_PSK: u32 = 0x0000_0100;
const SEC_KEY_MGMT_8021X: u32 = 0x0000_0200;
const SEC_KEY_MGMT_SAE: u32 = 0x0000_0400;
const SEC_KEY_MGMT_OWE: u32 = 0x0000_0800;

#[derive(Clone, Debug)]
pub(crate) struct NetworkManagerBackend {
    pub(crate) device_path: String,
    pub(crate) interface: String,
}

#[derive(Clone, Debug)]
struct SavedProfile {
    path: String,
    ssid: String,
    auto_connect: bool,
}

type SettingsMap = HashMap<String, HashMap<String, OwnedValue>>;

impl NetworkManagerBackend {
    pub(crate) fn discover(connection: &Connection) -> WifiResult<Self> {
        let manager = new_proxy(
            connection,
            NETWORK_MANAGER_SERVICE,
            NETWORK_MANAGER_PATH,
            NETWORK_MANAGER_INTERFACE,
        )?;
        let devices: Vec<OwnedObjectPath> =
            manager
                .call("GetDevices", &())
                .map_err(|_| WifiError::Dbus {
                    operation: "enumerate NetworkManager devices".to_string(),
                })?;

        for device_path in devices {
            let device_path = device_path.to_string();
            let device = new_proxy(
                connection,
                NETWORK_MANAGER_SERVICE,
                &device_path,
                NM_DEVICE_INTERFACE,
            )?;
            let device_type: u32 = match device.get_property("DeviceType") {
                Ok(value) => value,
                Err(_) => continue,
            };
            let managed: bool = device.get_property("Managed").unwrap_or(true);
            let interface: String = match device.get_property::<String>("Interface") {
                Ok(value) if !value.is_empty() => value,
                _ => continue,
            };
            if device_type == DEVICE_TYPE_WIFI && managed {
                return Ok(Self {
                    device_path,
                    interface,
                });
            }
        }

        Err(WifiError::MissingInterface {
            backend: "NetworkManager".to_string(),
        })
    }

    fn device(&self, connection: &Connection) -> WifiResult<Proxy<'static>> {
        new_proxy(
            connection,
            NETWORK_MANAGER_SERVICE,
            &self.device_path,
            NM_DEVICE_INTERFACE,
        )
    }

    fn wireless(&self, connection: &Connection) -> WifiResult<Proxy<'static>> {
        new_proxy(
            connection,
            NETWORK_MANAGER_SERVICE,
            &self.device_path,
            NM_DEVICE_WIFI_INTERFACE,
        )
    }

    fn manager(&self, connection: &Connection) -> WifiResult<Proxy<'static>> {
        new_proxy(
            connection,
            NETWORK_MANAGER_SERVICE,
            NETWORK_MANAGER_PATH,
            NETWORK_MANAGER_INTERFACE,
        )
    }

    fn saved_profiles(&self, connection: &Connection) -> WifiResult<Vec<SavedProfile>> {
        let settings = new_proxy(
            connection,
            NETWORK_MANAGER_SERVICE,
            NM_SETTINGS_PATH,
            NM_SETTINGS_INTERFACE,
        )?;
        let paths: Vec<OwnedObjectPath> =
            settings
                .call("ListConnections", &())
                .map_err(|_| WifiError::Dbus {
                    operation: "enumerate NetworkManager profiles".to_string(),
                })?;
        let mut profiles = Vec::new();

        for path in paths {
            let path = path.to_string();
            let connection_proxy = new_proxy(
                connection,
                NETWORK_MANAGER_SERVICE,
                &path,
                NM_CONNECTION_INTERFACE,
            )?;
            let settings: SettingsMap = match connection_proxy.call("GetSettings", &()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !is_wifi_profile(&settings) {
                continue;
            }
            let Some(ssid) = profile_ssid(&settings) else {
                continue;
            };
            let auto_connect = settings
                .get("connection")
                .and_then(|values| values.get("autoconnect"))
                .and_then(value_bool)
                .unwrap_or(true);
            profiles.push(SavedProfile {
                path,
                ssid,
                auto_connect,
            });
        }
        Ok(profiles)
    }

    fn connection_settings(
        &self,
        ssid: &str,
        password: Option<&SecretString>,
        auth: Option<&str>,
        cipher: Option<&str>,
        hidden: bool,
    ) -> SettingsMap {
        let mut connection = HashMap::new();
        connection.insert("id".to_string(), owned_value(ssid.to_string()));
        connection.insert(
            "type".to_string(),
            owned_value("802-11-wireless".to_string()),
        );
        connection.insert(
            "interface-name".to_string(),
            owned_value(self.interface.clone()),
        );
        connection.insert("autoconnect".to_string(), owned_value(true));

        let mut wireless = HashMap::new();
        wireless.insert("ssid".to_string(), owned_value(ssid.as_bytes().to_vec()));
        wireless.insert(
            "mode".to_string(),
            owned_value("infrastructure".to_string()),
        );
        if hidden {
            wireless.insert("hidden".to_string(), owned_value(true));
        }

        let mut settings = HashMap::new();
        settings.insert("connection".to_string(), connection);
        settings.insert("802-11-wireless".to_string(), wireless);

        if let Some(password) = password {
            let auth = auth.unwrap_or("WPA2-PSK").to_ascii_lowercase();
            let cipher = cipher.unwrap_or("AES").to_ascii_lowercase();
            let mut security = HashMap::new();
            if auth.contains("wep") || auth.contains("shared") || cipher == "wep" {
                security.insert("key-mgmt".to_string(), owned_value("none".to_string()));
                security.insert("auth-alg".to_string(), owned_value("shared".to_string()));
                security.insert(
                    "wep-key0".to_string(),
                    owned_value(password.expose_secret().to_string()),
                );
                security.insert("wep-key-type".to_string(), owned_value(2u32));
            } else {
                let key_mgmt = if auth.contains("wpa3") || auth.contains("sae") {
                    "sae"
                } else if auth.contains("enterprise") || auth == "wpa" || auth == "wpa2" {
                    "wpa-eap"
                } else {
                    "wpa-psk"
                };
                security.insert("key-mgmt".to_string(), owned_value(key_mgmt.to_string()));
                security.insert(
                    "psk".to_string(),
                    owned_value(password.expose_secret().to_string()),
                );
                let pairwise = if cipher.contains("tkip") {
                    vec!["tkip".to_string()]
                } else {
                    vec!["ccmp".to_string()]
                };
                security.insert("pairwise".to_string(), owned_value(pairwise.clone()));
                security.insert("group".to_string(), owned_value(pairwise));
            }
            settings.insert("802-11-wireless-security".to_string(), security);
        }
        settings
    }

    fn activate(
        &self,
        connection: &Connection,
        settings: SettingsMap,
        specific_object: &str,
    ) -> WifiResult<()> {
        let manager = self.manager(connection)?;
        let device = owned_object_path(&self.device_path)?;
        let specific = owned_object_path(specific_object)?;
        let _: (OwnedObjectPath, OwnedObjectPath) = manager
            .call("AddAndActivateConnection", &(settings, device, specific))
            .map_err(|_| WifiError::Dbus {
                operation: "activate a NetworkManager connection".to_string(),
            })?;
        Ok(())
    }

    fn find_profile(&self, connection: &Connection, ssid: &str) -> WifiResult<SavedProfile> {
        self.saved_profiles(connection)?
            .into_iter()
            .find(|profile| nm_saved_network_matches(&profile.ssid, ssid))
            .ok_or_else(|| WifiError::NetworkNotFound {
                ssid: ssid.to_string(),
            })
    }
}

impl WifiBackend for NetworkManagerBackend {
    fn name(&self) -> &'static str {
        "NetworkManager"
    }

    fn listener_spec(&self) -> ListenerSpec {
        ListenerSpec::NetworkManager {
            device_path: self.device_path.clone(),
        }
    }

    fn connect_profile(&self, ssid: &str) -> WifiResult<()> {
        let connection = system_connection()?;
        let profile = self.find_profile(&connection, ssid)?;
        let manager = self.manager(&connection)?;
        let profile_path = owned_object_path(&profile.path)?;
        let device_path = owned_object_path(&self.device_path)?;
        let specific = owned_object_path("/")?;
        let _: OwnedObjectPath = manager
            .call("ActivateConnection", &(profile_path, device_path, specific))
            .map_err(|_| WifiError::Dbus {
                operation: "activate a saved NetworkManager profile".to_string(),
            })?;
        Ok(())
    }

    fn connect_with_password(
        &self,
        ssid: &str,
        password: &SecretString,
        auth: &str,
        cipher: &str,
        hidden: bool,
    ) -> WifiResult<()> {
        let connection = system_connection()?;
        let settings =
            self.connection_settings(ssid, Some(password), Some(auth), Some(cipher), hidden);
        self.activate(&connection, settings, "/")
    }

    fn connect_open(&self, ssid: &str, hidden: bool) -> WifiResult<()> {
        let connection = system_connection()?;
        let settings = self.connection_settings(ssid, None, None, None, hidden);
        self.activate(&connection, settings, "/")
    }

    fn disconnect(&self) -> WifiResult<()> {
        let connection = system_connection()?;
        self.device(&connection)?
            .call::<_, _, ()>("Disconnect", &())
            .map_err(|_| WifiError::Dbus {
                operation: "disconnect NetworkManager Wi-Fi".to_string(),
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
        let device = self.device(&connection)?;
        let state: u32 = device.get_property("State").map_err(|_| WifiError::Dbus {
            operation: "read NetworkManager connection state".to_string(),
        })?;
        if state != DEVICE_STATE_ACTIVATED {
            return Ok(None);
        }
        let wireless = self.wireless(&connection)?;
        let active_ap: OwnedObjectPath = match wireless.get_property("ActiveAccessPoint") {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        if active_ap.as_str() == "/" {
            return Ok(None);
        }
        let ap = new_proxy(
            &connection,
            NETWORK_MANAGER_SERVICE,
            active_ap.as_str(),
            NM_ACCESS_POINT_INTERFACE,
        )?;
        let ssid: Vec<u8> = ap.get_property("Ssid").map_err(|_| WifiError::Dbus {
            operation: "read the NetworkManager connected SSID".to_string(),
        })?;
        Ok(Some(String::from_utf8_lossy(&ssid).into_owned()))
    }

    fn get_wifi_networks(&self) -> WifiResult<Vec<WifiInfo>> {
        let connection = system_connection()?;
        let saved = self.saved_profiles(&connection)?;
        let connected = self.get_connected_ssid()?;
        let link_speed = self
            .device(&connection)?
            .get_property::<u32>("Bitrate")
            .ok()
            .map(|rate| rate / 1000);
        let wireless = self.wireless(&connection)?;
        let access_points: Vec<OwnedObjectPath> = wireless
            .call("GetAllAccessPoints", &())
            .or_else(|_| wireless.call("GetAccessPoints", &()))
            .map_err(|_| WifiError::Dbus {
                operation: "enumerate NetworkManager access points".to_string(),
            })?;

        let mut networks = HashMap::<String, WifiInfo>::new();
        for access_point in access_points {
            let path = access_point.to_string();
            let ap = new_proxy(
                &connection,
                NETWORK_MANAGER_SERVICE,
                &path,
                NM_ACCESS_POINT_INTERFACE,
            )?;
            let ssid_bytes: Vec<u8> = match ap.get_property("Ssid") {
                Ok(value) => value,
                Err(_) => continue,
            };
            if ssid_bytes.is_empty() {
                continue;
            }
            let ssid = String::from_utf8_lossy(&ssid_bytes).into_owned();
            let strength: u8 = ap.get_property("Strength").unwrap_or(0);
            let frequency_mhz: u32 = ap.get_property("Frequency").unwrap_or(0);
            let flags: u32 = ap.get_property("Flags").unwrap_or(0);
            let wpa_flags: u32 = ap.get_property("WpaFlags").unwrap_or(0);
            let rsn_flags: u32 = ap.get_property("RsnFlags").unwrap_or(0);
            let (authentication, encryption) = nm_security_names(flags, wpa_flags, rsn_flags);
            let is_connected = connected.as_deref() == Some(ssid.as_str());
            let profile = saved
                .iter()
                .find(|profile| nm_saved_network_matches(&profile.ssid, &ssid));
            let info = WifiInfo {
                ssid: ssid.clone(),
                authentication: authentication.to_string(),
                encryption: encryption.to_string(),
                signal: strength,
                is_saved: profile.is_some(),
                is_connected,
                auto_connect: profile.map(|profile| profile.auto_connect).unwrap_or(false),
                phy_type: "Unknown".to_string(),
                channel: nm_frequency_to_channel(frequency_mhz),
                frequency: nm_frequency_to_hz(frequency_mhz),
                link_speed: if is_connected { link_speed } else { None },
            };
            networks
                .entry(ssid)
                .and_modify(|current| {
                    if info.signal > current.signal {
                        current.signal = info.signal;
                        current.channel = info.channel;
                        current.frequency = info.frequency;
                    }
                    current.is_saved |= info.is_saved;
                    current.is_connected |= info.is_connected;
                    current.auto_connect |= info.auto_connect;
                    if info.link_speed.is_some() {
                        current.link_speed = info.link_speed;
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
        let wireless = self.wireless(&connection)?;
        let options: HashMap<String, OwnedValue> = HashMap::new();
        wireless
            .call::<_, _, ()>("RequestScan", &options)
            .map_err(|_| WifiError::Dbus {
                operation: "request a NetworkManager Wi-Fi scan".to_string(),
            })
    }

    fn get_saved_profiles(&self) -> WifiResult<Vec<String>> {
        Ok(self
            .saved_profiles(&system_connection()?)?
            .into_iter()
            .map(|profile| profile.ssid)
            .collect())
    }

    fn set_auto_connect(&self, ssid: &str, enable: bool) -> WifiResult<()> {
        let connection = system_connection()?;
        let profile = self.find_profile(&connection, ssid)?;
        let profile_proxy = new_proxy(
            &connection,
            NETWORK_MANAGER_SERVICE,
            &profile.path,
            NM_CONNECTION_INTERFACE,
        )?;
        let mut settings: SettingsMap =
            profile_proxy
                .call("GetSettings", &())
                .map_err(|_| WifiError::Dbus {
                    operation: "read a NetworkManager saved profile".to_string(),
                })?;
        set_profile_auto_connect(&mut settings, enable);
        profile_proxy
            .call::<_, _, ()>("Update", &settings)
            .map_err(|_| WifiError::Dbus {
                operation: "update NetworkManager auto-connect setting".to_string(),
            })
    }

    fn forget_network(&self, ssid: &str) -> WifiResult<()> {
        let connection = system_connection()?;
        let profile = self.find_profile(&connection, ssid)?;
        let profile_proxy = new_proxy(
            &connection,
            NETWORK_MANAGER_SERVICE,
            &profile.path,
            NM_CONNECTION_INTERFACE,
        )?;
        profile_proxy
            .call::<_, _, ()>("Delete", &())
            .map_err(|_| WifiError::Dbus {
                operation: "forget a NetworkManager saved profile".to_string(),
            })
    }

    fn get_wifi_password(&self, ssid: &str) -> WifiResult<Option<SecretString>> {
        let connection = system_connection()?;
        let profile = self.find_profile(&connection, ssid)?;
        let profile_proxy = new_proxy(
            &connection,
            NETWORK_MANAGER_SERVICE,
            &profile.path,
            NM_CONNECTION_INTERFACE,
        )?;
        let settings: SettingsMap =
            profile_proxy
                .call("GetSettings", &())
                .map_err(|_| WifiError::Dbus {
                    operation: "read a NetworkManager saved profile".to_string(),
                })?;

        let setting_name = if settings.contains_key("802-11-wireless-security") {
            "802-11-wireless-security"
        } else if settings.contains_key("802-1x") {
            "802-1x"
        } else {
            return Ok(None);
        };

        let secrets: SettingsMap =
            profile_proxy
                .call("GetSecrets", &setting_name)
                .map_err(|_| WifiError::Dbus {
                    operation: "read the NetworkManager saved password".to_string(),
                })?;
        Ok(extract_profile_secret(&secrets, setting_name))
    }
}

fn is_wifi_profile(settings: &SettingsMap) -> bool {
    settings
        .get("connection")
        .and_then(|values| values.get("type"))
        .and_then(value_string)
        .as_deref()
        == Some("802-11-wireless")
}

fn profile_ssid(settings: &SettingsMap) -> Option<String> {
    let wireless = settings.get("802-11-wireless")?;
    let value = wireless.get("ssid")?;
    value_bytes(value)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .or_else(|| value_string(value))
}

pub(crate) fn nm_saved_network_matches(profile_ssid: &str, network_ssid: &str) -> bool {
    profile_ssid == network_ssid
}

fn set_profile_auto_connect(settings: &mut SettingsMap, enable: bool) {
    settings
        .entry("connection".to_string())
        .or_default()
        .insert("autoconnect".to_string(), owned_value(enable));
}

fn extract_profile_secret(settings: &SettingsMap, setting_name: &str) -> Option<SecretString> {
    let values = settings.get(setting_name)?;
    [
        "psk", "wep-key0", "wep-key1", "wep-key2", "wep-key3", "password",
    ]
    .into_iter()
    .find_map(|name| {
        values
            .get(name)
            .and_then(value_string)
            .map(SecretString::from)
    })
}

/// Convert the NetworkManager AP frequency, which is in MHz, to the Hz unit
/// used by WifUI's shared `WifiInfo` model.
pub(crate) fn nm_frequency_to_hz(frequency_mhz: u32) -> u64 {
    u64::from(frequency_mhz) * 1_000_000
}

pub(crate) fn nm_frequency_to_channel(frequency_mhz: u32) -> u32 {
    match frequency_mhz {
        2412..=2472 => (frequency_mhz - 2407) / 5,
        2484 => 14,
        5000..=5900 => (frequency_mhz - 5000) / 5,
        5950..=7125 => (frequency_mhz - 5950) / 5,
        _ => 0,
    }
}

/// Normalize NetworkManager's WPA/RSN capability flags to the names already
/// used by the frontend.
pub(crate) fn nm_security_names(
    flags: u32,
    wpa_flags: u32,
    rsn_flags: u32,
) -> (&'static str, &'static str) {
    if flags == 0 && wpa_flags == 0 && rsn_flags == 0 {
        return ("Open", "None");
    }
    if (wpa_flags | rsn_flags) == 0 && (flags & AP_FLAGS_PRIVACY) != 0 {
        return ("Shared", "WEP");
    }
    let all = wpa_flags | rsn_flags;
    let encryption = if all & (SEC_PAIR_CCMP | SEC_GROUP_CCMP) != 0 {
        "AES"
    } else if all & (SEC_PAIR_TKIP | SEC_GROUP_TKIP) != 0 {
        "TKIP"
    } else if all & (SEC_PAIR_WEP40 | SEC_PAIR_WEP104 | SEC_GROUP_WEP40 | SEC_GROUP_WEP104) != 0 {
        "WEP"
    } else {
        "Unknown"
    };
    let authentication = if rsn_flags & SEC_KEY_MGMT_SAE != 0 {
        "WPA3-SAE"
    } else if rsn_flags & SEC_KEY_MGMT_OWE != 0 {
        "OWE"
    } else if rsn_flags & SEC_KEY_MGMT_PSK != 0 {
        "WPA2-PSK"
    } else if rsn_flags & SEC_KEY_MGMT_8021X != 0 {
        "WPA2"
    } else if wpa_flags & SEC_KEY_MGMT_PSK != 0 {
        "WPA-PSK"
    } else if wpa_flags & SEC_KEY_MGMT_8021X != 0 {
        "WPA"
    } else {
        "Unknown"
    };
    (authentication, encryption)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_open_and_wep_security() {
        assert_eq!(nm_security_names(0, 0, 0), ("Open", "None"));
        assert_eq!(nm_security_names(AP_FLAGS_PRIVACY, 0, 0), ("Shared", "WEP"));
    }

    #[test]
    fn maps_wpa_security_names() {
        assert_eq!(
            nm_security_names(0, SEC_KEY_MGMT_PSK | SEC_PAIR_CCMP, 0),
            ("WPA-PSK", "AES")
        );
        assert_eq!(
            nm_security_names(0, 0, SEC_KEY_MGMT_SAE | SEC_PAIR_CCMP),
            ("WPA3-SAE", "AES")
        );
    }

    #[test]
    fn normalizes_frequency_and_channel() {
        assert_eq!(nm_frequency_to_hz(2412), 2_412_000_000);
        assert_eq!(nm_frequency_to_channel(2412), 1);
        assert_eq!(nm_frequency_to_channel(5180), 36);
        assert_eq!(nm_frequency_to_channel(5955), 1);
        assert_eq!(nm_frequency_to_channel(1234), 0);
    }

    #[test]
    fn matches_saved_profiles_by_ssid() {
        assert!(nm_saved_network_matches("home", "home"));
        assert!(!nm_saved_network_matches("home", "guest"));
    }

    #[test]
    fn updates_profile_auto_connect_without_dropping_settings() {
        let mut connection = HashMap::new();
        connection.insert("id".to_string(), owned_value("home".to_string()));
        let mut settings = SettingsMap::new();
        settings.insert("connection".to_string(), connection);

        set_profile_auto_connect(&mut settings, false);

        let connection = settings.get("connection").expect("connection settings");
        assert_eq!(
            value_string(connection.get("id").expect("profile id")),
            Some("home".to_string())
        );
        assert_eq!(
            value_bool(connection.get("autoconnect").expect("auto-connect")),
            Some(false)
        );
    }

    #[test]
    fn extracts_supported_profile_secrets_without_logging_them() {
        let mut security = HashMap::new();
        security.insert("psk".to_string(), owned_value("secret".to_string()));
        let mut secrets = SettingsMap::new();
        secrets.insert("802-11-wireless-security".to_string(), security);

        let password =
            extract_profile_secret(&secrets, "802-11-wireless-security").expect("saved password");
        assert_eq!(password.expose_secret(), "secret");
    }
}
