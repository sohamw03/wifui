use super::{
    NETWORK_MANAGER_INTERFACE, NETWORK_MANAGER_PATH, NETWORK_MANAGER_SERVICE,
    NM_ACTIVE_CONNECTION_INTERFACE, NM_DEVICE_INTERFACE, new_proxy, service_has_owner,
    system_connection,
};
use crate::error::{WifiError, WifiResult};
use crate::wifi::types::EthernetStatus;
use zbus::zvariant::OwnedObjectPath;

const DEVICE_TYPE_ETHERNET: u32 = 1;
const DEVICE_STATE_CONNECTED: u32 = 80;
const INTERFACE_FLAG_CARRIER: u32 = 0x0001_0000;
const PRIMARY_ETHERNET: &str = "802-3-ethernet";

pub(crate) fn get_status() -> WifiResult<EthernetStatus> {
    let manager_status = network_manager_status();
    if matches!(
        &manager_status,
        Ok(EthernetStatus::Active | EthernetStatus::Connected)
    ) {
        return manager_status;
    }
    super::linux_netlink::get_status()
}

fn network_manager_status() -> WifiResult<EthernetStatus> {
    let connection = system_connection()?;
    if !service_has_owner(&connection, NETWORK_MANAGER_SERVICE)? {
        return Ok(EthernetStatus::Unsupported);
    }

    let manager = new_proxy(
        &connection,
        NETWORK_MANAGER_SERVICE,
        NETWORK_MANAGER_PATH,
        NETWORK_MANAGER_INTERFACE,
    )?;
    let devices: Vec<OwnedObjectPath> =
        manager
            .call("GetDevices", &())
            .map_err(|error| WifiError::Dbus {
                operation: format!("enumerate NetworkManager devices: {error}"),
            })?;

    let mut active = false;
    for device_path in devices {
        let device = new_proxy(
            &connection,
            NETWORK_MANAGER_SERVICE,
            &device_path.to_string(),
            NM_DEVICE_INTERFACE,
        )?;
        let device_type: u32 = match device.get_property("DeviceType") {
            Ok(value) => value,
            Err(_) => continue,
        };
        if device_type != DEVICE_TYPE_ETHERNET {
            continue;
        }
        let interface: String = device.get_property("Interface").unwrap_or_default();
        if interface.is_empty() {
            continue;
        }
        let state: u32 = device.get_property("State").unwrap_or_default();
        let interface_flags: u32 = device.get_property("InterfaceFlags").unwrap_or_default();
        if device_is_active(state, interface_flags) {
            active = true;
            break;
        }
    }

    if !active {
        return Ok(EthernetStatus::Inactive);
    }

    let primary_type: Option<String> = manager.get_property("PrimaryConnectionType").ok();
    if primary_type.as_deref() == Some(PRIMARY_ETHERNET) {
        return Ok(EthernetStatus::Connected);
    }

    let primary_connection: OwnedObjectPath =
        match manager.get_property::<OwnedObjectPath>("PrimaryConnection") {
            Ok(path) if path.as_str() != "/" => path,
            _ => return Ok(EthernetStatus::Active),
        };
    let active_connection = new_proxy(
        &connection,
        NETWORK_MANAGER_SERVICE,
        &primary_connection.to_string(),
        NM_ACTIVE_CONNECTION_INTERFACE,
    )?;
    let primary_type: String = active_connection.get_property("Type").unwrap_or_default();
    if primary_type == PRIMARY_ETHERNET {
        Ok(EthernetStatus::Connected)
    } else {
        Ok(EthernetStatus::Active)
    }
}

fn device_is_active(state: u32, interface_flags: u32) -> bool {
    state >= DEVICE_STATE_CONNECTED || interface_flags & INTERFACE_FLAG_CARRIER != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrier_or_activated_state_marks_ethernet_active() {
        assert!(device_is_active(DEVICE_STATE_CONNECTED, 0));
        assert!(device_is_active(0, INTERFACE_FLAG_CARRIER));
        assert!(!device_is_active(20, 0));
    }
}
