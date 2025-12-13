use color_eyre::eyre::{Result, eyre};
use std::collections::HashMap;
use windows::{
    core::{GUID, PCWSTR},
    Win32::{
        Foundation::{ERROR_SUCCESS, HANDLE},
        NetworkManagement::WiFi::*,
    },
};

#[derive(Debug, Default, Clone)]
pub struct WifiInfo {
    pub ssid: String,
    #[allow(dead_code)]
    pub network_type: String,
    pub authentication: String,
    pub encryption: String,
    pub signal: u8,
    pub is_saved: bool,
    pub phy_type: String,
    pub channel: u32,
    pub frequency: u32,
    pub link_speed: Option<u32>,
}

// Helper to open WLAN handle
fn get_wlan_handle() -> Result<(HANDLE, u32)> {
    let mut negotiated_version = 0;
    let mut handle = HANDLE::default();
    unsafe {
        let result = WlanOpenHandle(2, None, &mut negotiated_version, &mut handle);
        if result != ERROR_SUCCESS.0 {
            return Err(eyre!("Failed to open WLAN handle: {}", result));
        }
    }
    Ok((handle, negotiated_version))
}

// Helper to get the first interface GUID
fn get_interface_guid(handle: HANDLE) -> Result<GUID> {
    unsafe {
        let mut interface_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        let result = WlanEnumInterfaces(handle, None, &mut interface_list);
        if result != ERROR_SUCCESS.0 {
            return Err(eyre!("Failed to enum interfaces: {}", result));
        }

        if (*interface_list).dwNumberOfItems == 0 {
             WlanFreeMemory(interface_list as *mut _);
             return Err(eyre!("No WiFi interface found"));
        }

        let interface_info = &(*interface_list).InterfaceInfo[0];
        let guid = interface_info.InterfaceGuid;
        WlanFreeMemory(interface_list as *mut _);
        Ok(guid)
    }
}

pub fn get_connected_ssid() -> Result<Option<String>> {
    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    let mut connected_ssid = None;

    unsafe {
        let mut data_size = 0;
        let mut data_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut opcode_value_type = wlan_opcode_value_type_invalid;

        let result = WlanQueryInterface(
            handle,
            &guid,
            wlan_intf_opcode_current_connection,
            None,
            &mut data_size,
            &mut data_ptr,
            Some(&mut opcode_value_type),
        );

        if result == ERROR_SUCCESS.0 {
            let connection_attributes = &*(data_ptr as *const WLAN_CONNECTION_ATTRIBUTES);
            if connection_attributes.isState == wlan_interface_state_connected {
                 let ssid_len = connection_attributes.wlanAssociationAttributes.dot11Ssid.uSSIDLength as usize;
                 let ssid_bytes = &connection_attributes.wlanAssociationAttributes.dot11Ssid.ucSSID[..ssid_len];
                 connected_ssid = Some(String::from_utf8_lossy(ssid_bytes).to_string());
            }
            WlanFreeMemory(data_ptr);
        }

        WlanCloseHandle(handle, None);
    }

    Ok(connected_ssid)
}

pub fn scan_networks() -> Result<()> {
    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    unsafe {
        let result = WlanScan(handle, &guid, None, None, None);
        WlanCloseHandle(handle, None);
        if result != ERROR_SUCCESS.0 {
            return Err(eyre!("Failed to scan networks: {}", result));
        }
    }
    Ok(())
}

#[allow(non_upper_case_globals)]
pub fn get_wifi_networks() -> Result<Vec<WifiInfo>> {
    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    let mut wifi_list: Vec<WifiInfo>;

    unsafe {
        let mut available_network_list: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
        let result = WlanGetAvailableNetworkList(
            handle,
            &guid,
            WLAN_AVAILABLE_NETWORK_INCLUDE_ALL_ADHOC_PROFILES | WLAN_AVAILABLE_NETWORK_INCLUDE_ALL_MANUAL_HIDDEN_PROFILES,
            None,
            &mut available_network_list
        );

        if result != ERROR_SUCCESS.0 {
             WlanCloseHandle(handle, None);
             return Err(eyre!("Failed to get available networks: {}", result));
        }

        // Get current connection info for link speed
        let mut current_connection: Option<(String, u32)> = None;
        let mut data_size = 0;
        let mut data_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut opcode_value_type = wlan_opcode_value_type_invalid;

        let result_query = WlanQueryInterface(
            handle,
            &guid,
            wlan_intf_opcode_current_connection,
            None,
            &mut data_size,
            &mut data_ptr,
            Some(&mut opcode_value_type),
        );

        if result_query == ERROR_SUCCESS.0 {
             let conn = &*(data_ptr as *const WLAN_CONNECTION_ATTRIBUTES);
             if conn.isState == wlan_interface_state_connected {
                 let ssid_len = conn.wlanAssociationAttributes.dot11Ssid.uSSIDLength as usize;
                 let ssid_bytes = &conn.wlanAssociationAttributes.dot11Ssid.ucSSID[..ssid_len];
                 let ssid = String::from_utf8_lossy(ssid_bytes).to_string();
                 let tx_rate = conn.wlanAssociationAttributes.ulTxRate;
                 current_connection = Some((ssid, tx_rate));
             }
             WlanFreeMemory(data_ptr);
        }

        // Get BSS List to find channel, frequency and rate
        let mut bss_list: *mut WLAN_BSS_LIST = std::ptr::null_mut();
        let result_bss = WlanGetNetworkBssList(
            handle,
            &guid,
            None,
            dot11_BSS_type_any,
            false,
            None,
            &mut bss_list,
        );

        let mut bss_entries: &[WLAN_BSS_ENTRY] = &[];
        if result_bss == ERROR_SUCCESS.0 && !bss_list.is_null() {
             let num_bss = (*bss_list).dwNumberOfItems;
             bss_entries = std::slice::from_raw_parts(
                (*bss_list).wlanBssEntries.as_ptr(),
                num_bss as usize
            );
        }

        let num_items = (*available_network_list).dwNumberOfItems;
        let items = std::slice::from_raw_parts(
            (*available_network_list).Network.as_ptr(),
            num_items as usize
        );

        let mut wifi_map: HashMap<(String, String), WifiInfo> = HashMap::new();

        for item in items {
            let ssid_len = item.dot11Ssid.uSSIDLength as usize;
            if ssid_len == 0 { continue; }

            let ssid_bytes = &item.dot11Ssid.ucSSID[..ssid_len];
            let ssid = String::from_utf8_lossy(ssid_bytes).to_string();

            // Find best BSS entry for this SSID
            let best_bss = bss_entries.iter()
                .filter(|bss| {
                    let bss_ssid_len = bss.dot11Ssid.uSSIDLength as usize;
                    if bss_ssid_len != ssid_len { return false; }
                    &bss.dot11Ssid.ucSSID[..bss_ssid_len] == ssid_bytes
                })
                .max_by_key(|bss| bss.lRssi);

            let (frequency, channel) = if let Some(bss) = best_bss {
                let freq = bss.ulChCenterFrequency;
                let ch = if freq >= 2412000 && freq <= 2484000 {
                    if freq == 2484000 { 14 } else { (freq - 2407000) / 5000 }
                } else if freq >= 5000000 && freq <= 5900000 {
                    (freq - 5000000) / 5000
                } else if freq >= 5925000 && freq <= 7125000 {
                    (freq - 5950000) / 5000
                } else {
                    0
                };

                (freq, ch)
            } else {
                (0, 0)
            };

            let mut link_speed = None;
            if let Some((ref conn_ssid, conn_rate)) = current_connection {
                if *conn_ssid == ssid {
                    link_speed = Some(conn_rate / 1000); // Kbps to Mbps
                }
            }

            let authentication = match item.dot11DefaultAuthAlgorithm {
                DOT11_AUTH_ALGO_80211_OPEN => "Open",
                DOT11_AUTH_ALGO_80211_SHARED_KEY => "Shared",
                DOT11_AUTH_ALGO_WPA => "WPA",
                DOT11_AUTH_ALGO_WPA_PSK => "WPA-PSK",
                DOT11_AUTH_ALGO_WPA_NONE => "WPA-None",
                DOT11_AUTH_ALGO_RSNA => "WPA2",
                DOT11_AUTH_ALGO_RSNA_PSK => "WPA2-PSK",
                DOT11_AUTH_ALGO_WPA3 => "WPA3",
                DOT11_AUTH_ALGO_WPA3_SAE => "WPA3-SAE",
                _ => "Unknown",
            }.to_string();

            let encryption = match item.dot11DefaultCipherAlgorithm {
                DOT11_CIPHER_ALGO_NONE => "None",
                DOT11_CIPHER_ALGO_WEP40 => "WEP",
                DOT11_CIPHER_ALGO_TKIP => "TKIP",
                DOT11_CIPHER_ALGO_CCMP => "AES",
                DOT11_CIPHER_ALGO_WEP104 => "WEP",
                DOT11_CIPHER_ALGO_WPA_USE_GROUP => "WPA-Group",
                DOT11_CIPHER_ALGO_GCMP => "GCMP",
                _ => "Unknown",
            }.to_string();

            let is_saved = (item.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE) != 0;

            let bss_type = match item.dot11BssType {
                dot11_BSS_type_infrastructure => "Infrastructure",
                dot11_BSS_type_independent => "Ad-hoc",
                dot11_BSS_type_any => "Any",
                _ => "Unknown",
            }.to_string();

            let phy_types = std::slice::from_raw_parts(
                item.dot11PhyTypes.as_ptr(),
                item.uNumberOfPhyTypes as usize
            );

            let phy_type = if let Some(phy) = phy_types.first() {
                 match *phy {
                    dot11_phy_type_ofdm => "802.11a",
                    dot11_phy_type_hrdsss => "802.11b",
                    dot11_phy_type_erp => "802.11g",
                    dot11_phy_type_ht => "802.11n (Wi-Fi 4)",
                    dot11_phy_type_vht => "802.11ac (Wi-Fi 5)",
                    dot11_phy_type_he => "802.11ax (Wi-Fi 6)",
                    dot11_phy_type_eht => "802.11be (Wi-Fi 7)",
                    _ => "Legacy/Unknown",
                 }.to_string()
            } else {
                "Unknown".to_string()
            };

            let signal = item.wlanSignalQuality as u8;

            let new_info = WifiInfo {
                ssid: ssid.clone(),
                network_type: bss_type,
                authentication: authentication.clone(),
                encryption,
                signal,
                is_saved,
                phy_type,
                channel,
                frequency,
                link_speed,
            };

            wifi_map.entry((ssid, authentication))
                .and_modify(|info| {
                    if new_info.is_saved { info.is_saved = true; }
                    if new_info.signal > info.signal { info.signal = new_info.signal; }
                })
                .or_insert(new_info);
        }

        wifi_list = wifi_map.into_values().collect();

        if !bss_list.is_null() {
            WlanFreeMemory(bss_list as *mut _);
        }
        WlanFreeMemory(available_network_list as *mut _);
        WlanCloseHandle(handle, None);
    }

    // Sort by signal strength descending
    wifi_list.sort_by(|a, b| b.signal.cmp(&a.signal));

    Ok(wifi_list)
}

pub fn get_saved_profiles() -> Result<Vec<String>> {
    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    let mut profiles = Vec::new();

    unsafe {
        let mut profile_list: *mut WLAN_PROFILE_INFO_LIST = std::ptr::null_mut();
        let result = WlanGetProfileList(handle, &guid, None, &mut profile_list);

        if result == ERROR_SUCCESS.0 {
            let num_items = (*profile_list).dwNumberOfItems;
            let items = std::slice::from_raw_parts(
                (*profile_list).ProfileInfo.as_ptr(),
                num_items as usize
            );

            for item in items {
                let name = String::from_utf16_lossy(&item.strProfileName);
                // Trim null characters if any
                let name = name.trim_matches(char::from(0)).to_string();
                if !name.is_empty() {
                    profiles.push(name);
                }
            }
            WlanFreeMemory(profile_list as *mut _);
        }
        WlanCloseHandle(handle, None);
    }

    Ok(profiles)
}

pub fn connect_profile(ssid: &str) -> Result<()> {
    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    unsafe {
        let ssid_wide: Vec<u16> = ssid.encode_utf16().chain(std::iter::once(0)).collect();
        let p_profile_name = PCWSTR(ssid_wide.as_ptr());

        let connection_params = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: p_profile_name,
            pDot11Ssid: std::ptr::null_mut(),
            pDesiredBssidList: std::ptr::null_mut(),
            dot11BssType: dot11_BSS_type_infrastructure,
            dwFlags: 0,
        };

        let result = WlanConnect(handle, &guid, &connection_params, None);
        WlanCloseHandle(handle, None);

        if result != ERROR_SUCCESS.0 {
            return Err(eyre!("Failed to connect: {}", result));
        }
    }
    Ok(())
}

pub fn connect_with_password(ssid: &str, password: &str) -> Result<()> {
    let profile_xml = format!(
        r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>{}</name>
    <SSIDConfig>
        <SSID>
            <name>{}</name>
        </SSID>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>auto</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>WPA2PSK</authentication>
                <encryption>AES</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
            <sharedKey>
                <keyType>passPhrase</keyType>
                <protected>false</protected>
                <keyMaterial>{}</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>"#,
        ssid, ssid, password
    );

    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    unsafe {
        let xml_wide: Vec<u16> = profile_xml.encode_utf16().chain(std::iter::once(0)).collect();
        let p_profile_xml = PCWSTR(xml_wide.as_ptr());

        let mut reason_code = 0;
        let result = WlanSetProfile(
            handle,
            &guid,
            0,
            p_profile_xml,
            None,
            true,
            None,
            &mut reason_code
        );

        if result != ERROR_SUCCESS.0 {
            WlanCloseHandle(handle, None);
            return Err(eyre!("Failed to add profile: {} (Reason: {})", result, reason_code));
        }
        WlanCloseHandle(handle, None);
    }

    connect_profile(ssid)
}

pub fn connect_open(ssid: &str) -> Result<()> {
    let profile_xml = format!(
        r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>{}</name>
    <SSIDConfig>
        <SSID>
            <name>{}</name>
        </SSID>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>manual</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>open</authentication>
                <encryption>none</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
        </security>
    </MSM>
</WLANProfile>"#,
        ssid, ssid
    );

    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    unsafe {
        let xml_wide: Vec<u16> = profile_xml.encode_utf16().chain(std::iter::once(0)).collect();
        let p_profile_xml = PCWSTR(xml_wide.as_ptr());

        let mut reason_code = 0;
        let result = WlanSetProfile(
            handle,
            &guid,
            0,
            p_profile_xml,
            None,
            true,
            None,
            &mut reason_code
        );

        if result != ERROR_SUCCESS.0 {
            WlanCloseHandle(handle, None);
            return Err(eyre!("Failed to add open profile: {} (Reason: {})", result, reason_code));
        }
        WlanCloseHandle(handle, None);
    }

    connect_profile(ssid)
}

pub fn forget_network(ssid: &str) -> Result<()> {
    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    unsafe {
        let ssid_wide: Vec<u16> = ssid.encode_utf16().chain(std::iter::once(0)).collect();
        let p_profile_name = PCWSTR(ssid_wide.as_ptr());

        let result = WlanDeleteProfile(handle, &guid, p_profile_name, None);
        WlanCloseHandle(handle, None);

        if result != ERROR_SUCCESS.0 {
            return Err(eyre!("Failed to forget network: {}", result));
        }
    }
    Ok(())
}

pub fn disconnect() -> Result<()> {
    let (handle, _) = get_wlan_handle()?;
    let guid = get_interface_guid(handle)?;

    unsafe {
        let result = WlanDisconnect(handle, &guid, None);
        WlanCloseHandle(handle, None);

        if result != ERROR_SUCCESS.0 {
            return Err(eyre!("Failed to disconnect: {}", result));
        }
    }
    Ok(())
}
