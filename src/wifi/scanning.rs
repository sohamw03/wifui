use crate::error::{WifiError, WifiResult};
use crate::wifi::handle::WlanHandle;
use windows::Win32::{Foundation::ERROR_SUCCESS, NetworkManagement::WiFi::*};

/// Trigger a network scan
pub fn scan_networks() -> WifiResult<()> {
    let handle = WlanHandle::open()?;
    let guid = handle.get_interface_guid()?;

    unsafe {
        let result = WlanScan(handle.as_raw(), &guid, None, None, None);
        if result != ERROR_SUCCESS.0 {
            return Err(WifiError::ScanFailed { code: result });
        }
    }
    Ok(())
}
