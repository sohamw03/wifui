use crate::error::{WifiError, WifiResult};
use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HANDLE},
        NetworkManagement::WiFi::*,
    },
    core::GUID,
};

/// Safe wrapper around WLAN handle that automatically closes on drop
#[derive(Debug)]
pub struct WlanHandle {
    handle: HANDLE,
}

impl WlanHandle {
    /// Open a new WLAN handle
    pub fn open() -> WifiResult<Self> {
        let mut negotiated_version = 0;
        let mut handle = HANDLE::default();
        unsafe {
            let result = WlanOpenHandle(2, None, &mut negotiated_version, &mut handle);
            if result != ERROR_SUCCESS.0 {
                return Err(WifiError::HandleOpenFailed { code: result });
            }
        }
        Ok(Self { handle })
    }

    /// Get the raw handle for API calls
    pub fn as_raw(&self) -> HANDLE {
        self.handle
    }

    /// Get the first interface GUID
    pub fn get_interface_guid(&self) -> WifiResult<GUID> {
        unsafe {
            let mut interface_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
            let result = WlanEnumInterfaces(self.handle, None, &mut interface_list);
            if result != ERROR_SUCCESS.0 {
                return Err(WifiError::InterfaceEnumFailed { code: result });
            }

            if (*interface_list).dwNumberOfItems == 0 {
                WlanFreeMemory(interface_list as *mut _);
                return Err(WifiError::NoInterface);
            }

            let interface_info = &(*interface_list).InterfaceInfo[0];
            let guid = interface_info.InterfaceGuid;
            WlanFreeMemory(interface_list as *mut _);
            Ok(guid)
        }
    }
}

impl Drop for WlanHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = WlanCloseHandle(self.handle, None);
        }
    }
}
