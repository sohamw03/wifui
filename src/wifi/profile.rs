use crate::error::{WifiError, WifiResult};
use crate::wifi::handle::WlanHandle;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::writer::Writer;
use secrecy::{ExposeSecret, SecretString};
use std::io::Cursor;
use windows::{
    Win32::{Foundation::ERROR_SUCCESS, NetworkManagement::WiFi::*},
    core::{PCWSTR, PWSTR},
};

/// WLAN_PROFILE_GET_PLAINTEXT_KEY flag to retrieve password from profile
const WLAN_PROFILE_GET_PLAINTEXT_KEY: u32 = 4;

/// Create a WiFi profile XML document
pub fn create_profile_xml(
    ssid: &str,
    auth: &str,
    cipher: &str,
    password: Option<&SecretString>,
    hidden: bool,
) -> String {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let _ = writer.write_event(Event::Decl(BytesDecl::new("1.0", None, None)));

    let mut wlan_profile = BytesStart::new("WLANProfile");
    wlan_profile.push_attribute((
        "xmlns",
        "http://www.microsoft.com/networking/WLAN/profile/v1",
    ));
    let _ = writer.write_event(Event::Start(wlan_profile));

    write_element(&mut writer, "name", ssid);

    let _ = writer.write_event(Event::Start(BytesStart::new("SSIDConfig")));
    let _ = writer.write_event(Event::Start(BytesStart::new("SSID")));
    write_element(&mut writer, "name", ssid);
    let _ = writer.write_event(Event::End(BytesEnd::new("SSID")));

    if hidden {
        write_element(&mut writer, "nonBroadcast", "true");
    }
    let _ = writer.write_event(Event::End(BytesEnd::new("SSIDConfig")));

    write_element(&mut writer, "connectionType", "ESS");
    write_element(&mut writer, "connectionMode", "manual");

    let _ = writer.write_event(Event::Start(BytesStart::new("MSM")));
    let _ = writer.write_event(Event::Start(BytesStart::new("security")));
    let _ = writer.write_event(Event::Start(BytesStart::new("authEncryption")));

    let (xml_auth, xml_cipher) = match auth {
        "WPA3-SAE" => ("WPA3SAE", "AES"),
        "WPA3ENT" => ("WPA3ENT", "AES"),
        "WPA3ENT192" => ("WPA3ENT192", "AES"),
        "WPA3" => ("WPA3ENT192", "AES"),
        "WPA2-PSK" => ("WPA2PSK", "AES"),
        "WPA2" => ("WPA2", "AES"),
        "WPA-PSK" => ("WPAPSK", if cipher == "AES" { "AES" } else { "TKIP" }),
        "WPA" => ("WPA", if cipher == "AES" { "AES" } else { "TKIP" }),
        "Shared" | "WEP" => ("shared", "WEP"),
        "Open" | "open" => ("open", "none"),
        _ => ("WPA2PSK", "AES"),
    };
    let final_cipher = if cipher == "GCMP" { "GCMP" } else { xml_cipher };

    write_element(&mut writer, "authentication", xml_auth);
    write_element(&mut writer, "encryption", final_cipher);
    write_element(&mut writer, "useOneX", "false");
    let _ = writer.write_event(Event::End(BytesEnd::new("authEncryption")));

    if let Some(pwd) = password {
        let _ = writer.write_event(Event::Start(BytesStart::new("sharedKey")));
        write_element(&mut writer, "keyType", "passPhrase");
        write_element(&mut writer, "protected", "false");
        write_element(&mut writer, "keyMaterial", pwd.expose_secret());
        let _ = writer.write_event(Event::End(BytesEnd::new("sharedKey")));
    }

    let _ = writer.write_event(Event::End(BytesEnd::new("security")));
    let _ = writer.write_event(Event::End(BytesEnd::new("MSM")));

    let _ = writer.write_event(Event::End(BytesEnd::new("WLANProfile")));

    String::from_utf8(writer.into_inner().into_inner()).unwrap_or_default()
}

fn write_element<W: std::io::Write>(writer: &mut Writer<W>, name: &str, value: &str) {
    let _ = writer.write_event(Event::Start(BytesStart::new(name)));
    let _ = writer.write_event(Event::Text(BytesText::new(value)));
    let _ = writer.write_event(Event::End(BytesEnd::new(name)));
}

/// Check if a profile has auto-connect enabled
pub fn is_profile_auto_connect(
    handle: &WlanHandle,
    guid: &windows::core::GUID,
    profile_name: &str,
) -> bool {
    unsafe {
        let profile_name_wide: Vec<u16> = profile_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let p_profile_name = PCWSTR(profile_name_wide.as_ptr());
        let mut p_profile_xml = PWSTR::null();
        let mut flags = 0;

        let result = WlanGetProfile(
            handle.as_raw(),
            guid,
            p_profile_name,
            None,
            &mut p_profile_xml,
            Some(&mut flags),
            None,
        );

        if result == ERROR_SUCCESS.0 && !p_profile_xml.is_null() {
            let xml = p_profile_xml.to_string().unwrap_or_default();
            WlanFreeMemory(p_profile_xml.as_ptr() as *mut _);
            return xml.contains("<connectionMode>auto</connectionMode>");
        }
    }
    false
}

/// Get list of saved WiFi profile names
pub fn get_saved_profiles() -> WifiResult<Vec<String>> {
    let handle = WlanHandle::open()?;
    let guid = handle.get_interface_guid()?;

    let mut profiles = Vec::new();

    unsafe {
        let mut profile_list: *mut WLAN_PROFILE_INFO_LIST = std::ptr::null_mut();
        let result = WlanGetProfileList(handle.as_raw(), &guid, None, &mut profile_list);

        if result == ERROR_SUCCESS.0 {
            let num_items = (*profile_list).dwNumberOfItems;
            let items = std::slice::from_raw_parts(
                (*profile_list).ProfileInfo.as_ptr(),
                num_items as usize,
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
    }

    Ok(profiles)
}

/// Set auto-connect for a profile
///
/// Note: Uses WLAN_PROFILE_GET_PLAINTEXT_KEY flag to get the actual key material,
/// which prevents Windows from reauthenticating when the profile is set back.
pub fn set_auto_connect(ssid: &str, enable: bool) -> WifiResult<()> {
    let handle = WlanHandle::open()?;
    let guid = handle.get_interface_guid()?;

    // WLAN_PROFILE_GET_PLAINTEXT_KEY = 4
    // This flag is needed to get the actual key material so we can set the profile
    // back without triggering reauthentication
    const WLAN_PROFILE_GET_PLAINTEXT_KEY: u32 = 4;

    unsafe {
        let profile_name_wide: Vec<u16> = ssid.encode_utf16().chain(std::iter::once(0)).collect();
        let p_profile_name = PCWSTR(profile_name_wide.as_ptr());
        let mut p_profile_xml = PWSTR::null();
        let mut flags = WLAN_PROFILE_GET_PLAINTEXT_KEY;

        let result = WlanGetProfile(
            handle.as_raw(),
            &guid,
            p_profile_name,
            None,
            &mut p_profile_xml,
            Some(&mut flags),
            None,
        );

        if result != ERROR_SUCCESS.0 || p_profile_xml.is_null() {
            return Err(WifiError::ProfileGetFailed { code: result });
        }

        let xml = p_profile_xml.to_string().unwrap_or_default();
        WlanFreeMemory(p_profile_xml.as_ptr() as *mut _);

        let new_mode = if enable { "auto" } else { "manual" };
        let new_xml = if xml.contains("<connectionMode>auto</connectionMode>") {
            xml.replace(
                "<connectionMode>auto</connectionMode>",
                &format!("<connectionMode>{}</connectionMode>", new_mode),
            )
        } else if xml.contains("<connectionMode>manual</connectionMode>") {
            xml.replace(
                "<connectionMode>manual</connectionMode>",
                &format!("<connectionMode>{}</connectionMode>", new_mode),
            )
        } else {
            return Err(WifiError::ProfileXmlInvalid);
        };

        let xml_wide: Vec<u16> = new_xml.encode_utf16().chain(std::iter::once(0)).collect();
        let p_new_profile_xml = PCWSTR(xml_wide.as_ptr());

        let mut reason_code = 0;
        let result = WlanSetProfile(
            handle.as_raw(),
            &guid,
            0,
            p_new_profile_xml,
            None,
            true,
            None,
            &mut reason_code,
        );

        if result != ERROR_SUCCESS.0 {
            return Err(WifiError::ProfileSetFailed {
                code: result,
                reason: reason_code,
            });
        }
    }
    Ok(())
}

/// Forget (delete) a saved network profile
pub fn forget_network(ssid: &str) -> WifiResult<()> {
    let handle = WlanHandle::open()?;
    let guid = handle.get_interface_guid()?;

    unsafe {
        let ssid_wide: Vec<u16> = ssid.encode_utf16().chain(std::iter::once(0)).collect();
        let p_profile_name = PCWSTR(ssid_wide.as_ptr());

        let result = WlanDeleteProfile(handle.as_raw(), &guid, p_profile_name, None);

        // ERROR_NOT_FOUND (1168) is acceptable - the profile doesn't exist
        if result != ERROR_SUCCESS.0 && result != 1168 {
            return Err(WifiError::ProfileDeleteFailed { code: result });
        }
    }
    Ok(())
}

/// Get WiFi password from a saved profile
/// Returns None if profile doesn't exist or has no password (open network)
pub fn get_wifi_password(ssid: &str) -> WifiResult<Option<SecretString>> {
    let handle = WlanHandle::open()?;
    let guid = handle.get_interface_guid()?;

    unsafe {
        let profile_name_wide: Vec<u16> = ssid.encode_utf16().chain(std::iter::once(0)).collect();
        let p_profile_name = PCWSTR(profile_name_wide.as_ptr());
        let mut p_profile_xml = PWSTR::null();
        let mut flags = WLAN_PROFILE_GET_PLAINTEXT_KEY;

        let result = WlanGetProfile(
            handle.as_raw(),
            &guid,
            p_profile_name,
            None,
            &mut p_profile_xml,
            Some(&mut flags),
            None,
        );

        if result != ERROR_SUCCESS.0 || p_profile_xml.is_null() {
            return Err(WifiError::ProfileGetFailed { code: result });
        }

        let xml = p_profile_xml.to_string().unwrap_or_default();
        WlanFreeMemory(p_profile_xml.as_ptr() as *mut _);

        if let Some(start) = xml.find("<keyMaterial>") {
            if let Some(end) = xml.find("</keyMaterial>") {
                let password = xml[start + 13..end].to_string();
                if !password.is_empty() {
                    return Ok(Some(SecretString::from(password)));
                }
            }
        }

        Ok(None)
    }
}
