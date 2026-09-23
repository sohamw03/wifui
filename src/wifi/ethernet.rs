use crate::error::{WifiError, WifiResult};
use crate::wifi::types::EthernetStatus;
use std::ffi::c_void;
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetBestInterface, GetBestInterfaceEx, GetIfTable2, IF_TYPE_ETHERNET_CSMACD,
    MIB_IF_ROW2, MIB_IF_TABLE2,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET6, SOCKADDR, SOCKADDR_IN6, SOCKADDR_INET};

const HARDWARE_INTERFACE: u8 = 0x01;

struct MibTable(*mut MIB_IF_TABLE2);

impl Drop for MibTable {
    fn drop(&mut self) {
        unsafe {
            FreeMibTable(self.0.cast::<c_void>());
        }
    }
}

pub fn get_ethernet_status() -> WifiResult<EthernetStatus> {
    let mut table = std::ptr::null_mut();
    let result = unsafe { GetIfTable2(&mut table) };
    if result.0 != 0 || table.is_null() {
        return Err(WifiError::EthernetStatusFailed {
            operation: "enumerate Windows interfaces".to_string(),
            code: result.0,
        });
    }

    let table = MibTable(table);
    let entry_count = unsafe { (*table.0).NumEntries as usize };
    let table_offset = std::mem::offset_of!(MIB_IF_TABLE2, Table);
    let rows = unsafe {
        std::slice::from_raw_parts(
            table.0.cast::<u8>().add(table_offset).cast::<MIB_IF_ROW2>(),
            entry_count,
        )
    };

    let mut active_interfaces = Vec::new();
    for row in rows {
        if is_active_ethernet(row) {
            active_interfaces.push(row.InterfaceIndex);
        }
    }

    if active_interfaces.is_empty() {
        return Ok(EthernetStatus::Inactive);
    }

    let uses_ethernet = best_ipv4_interface()
        .into_iter()
        .chain(best_ipv6_interface())
        .any(|index| active_interfaces.contains(&index));
    if uses_ethernet {
        Ok(EthernetStatus::Connected)
    } else {
        Ok(EthernetStatus::Active)
    }
}

fn is_active_ethernet(row: &MIB_IF_ROW2) -> bool {
    row.Type == IF_TYPE_ETHERNET_CSMACD
        && row.OperStatus == IfOperStatusUp
        && row.InterfaceAndOperStatusFlags._bitfield & HARDWARE_INTERFACE != 0
}

fn best_ipv4_interface() -> Option<u32> {
    let mut interface = 0;
    let result = unsafe { GetBestInterface(0x0101_0101, &mut interface) };
    (result == 0).then_some(interface)
}

fn best_ipv6_interface() -> Option<u32> {
    let mut address = SOCKADDR_IN6::default();
    address.sin6_family = AF_INET6;
    address.sin6_addr.u.Byte = [
        0x20, 0x01, 0x48, 0x60, 0x48, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x88,
        0x88,
    ];
    let destination = SOCKADDR_INET { Ipv6: address };
    let mut interface = 0;
    let result = unsafe {
        GetBestInterfaceEx(
            (&destination as *const SOCKADDR_INET).cast::<SOCKADDR>(),
            &mut interface,
        )
    };
    (result == 0).then_some(interface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hardware_ethernet_interfaces_are_active() {
        let mut row = MIB_IF_ROW2::default();
        row.Type = IF_TYPE_ETHERNET_CSMACD;
        row.OperStatus = IfOperStatusUp;
        assert!(!is_active_ethernet(&row));

        row.InterfaceAndOperStatusFlags._bitfield = HARDWARE_INTERFACE;
        assert!(is_active_ethernet(&row));
    }
}
