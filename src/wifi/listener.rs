use crate::error::{WifiError, WifiResult, wlan_reason_to_string};
use crate::wifi::handle::WlanHandle;
use crate::wifi::types::ConnectionEvent;
use tokio::sync::mpsc::UnboundedSender;
use windows::Win32::{Foundation::ERROR_SUCCESS, NetworkManagement::WiFi::*};

/// WiFi event listener that receives connection notifications
#[derive(Debug)]
pub struct WifiListener {
    handle: WlanHandle,
    context: *mut std::ffi::c_void,
}

unsafe impl Send for WifiListener {}
unsafe impl Sync for WifiListener {}

impl Drop for WifiListener {
    fn drop(&mut self) {
        unsafe {
            let _ = WlanRegisterNotification(
                self.handle.as_raw(),
                WLAN_NOTIFICATION_SOURCE_NONE,
                true,
                None,
                None,
                None,
                None,
            );
            // WlanHandle will be dropped automatically
            let _ = Box::from_raw(self.context as *mut UnboundedSender<ConnectionEvent>);
        }
    }
}

unsafe extern "system" fn notification_callback(
    data: *mut L2_NOTIFICATION_DATA,
    context: *mut std::ffi::c_void,
) {
    if data.is_null() || context.is_null() {
        return;
    }

    // SAFETY: We checked for null above.
    // The context is a pointer to UnboundedSender<ConnectionEvent> created in start_wifi_listener
    let (data, sender) = unsafe {
        (
            &*data,
            &*(context as *const UnboundedSender<ConnectionEvent>),
        )
    };

    if data.NotificationSource != WLAN_NOTIFICATION_SOURCE_ACM {
        return;
    }

    if data.NotificationCode == wlan_notification_acm_connection_complete.0 as u32
        || data.NotificationCode == wlan_notification_acm_connection_attempt_fail.0 as u32
        || data.NotificationCode == wlan_notification_acm_disconnected.0 as u32
    {
        if data.dwDataSize < std::mem::size_of::<WLAN_CONNECTION_NOTIFICATION_DATA>() as u32 {
            return;
        }

        // SAFETY: The documentation guarantees pData points to WLAN_CONNECTION_NOTIFICATION_DATA
        // for these notification codes, and we checked the size above.
        let conn_data = unsafe { &*(data.pData as *const WLAN_CONNECTION_NOTIFICATION_DATA) };

        // Extract SSID
        let ssid_len = conn_data.dot11Ssid.uSSIDLength as usize;
        let ssid_bytes = &conn_data.dot11Ssid.ucSSID[..ssid_len];
        let ssid = String::from_utf8_lossy(ssid_bytes).to_string();

        if data.NotificationCode == wlan_notification_acm_connection_complete.0 as u32 {
            let _ = sender.send(ConnectionEvent::Connected(ssid));
        } else if data.NotificationCode == wlan_notification_acm_disconnected.0 as u32 {
            let _ = sender.send(ConnectionEvent::Disconnected(ssid));
        } else if data.NotificationCode == wlan_notification_acm_connection_attempt_fail.0 as u32 {
            let reason_code = conn_data.wlanReasonCode;
            let reason_str = wlan_reason_to_string(reason_code);

            let _ = sender.send(ConnectionEvent::Failed {
                ssid,
                reason_code,
                reason_str,
            });
        }
    }
}

/// Start listening for WiFi connection events
pub fn start_wifi_listener(sender: UnboundedSender<ConnectionEvent>) -> WifiResult<WifiListener> {
    let wlan_handle = WlanHandle::open()?;
    let handle = wlan_handle.as_raw();

    // Box the sender to pass as context
    let context = Box::into_raw(Box::new(sender));

    unsafe {
        let result = WlanRegisterNotification(
            handle,
            WLAN_NOTIFICATION_SOURCE_ACM,
            false,
            Some(notification_callback),
            Some(context as *mut std::ffi::c_void),
            None,
            None,
        );

        if result != ERROR_SUCCESS.0 {
            let _ = Box::from_raw(context as *mut UnboundedSender<ConnectionEvent>); // Cleanup
            return Err(WifiError::NotificationRegistrationFailed { code: result });
        }
    }

    Ok(WifiListener {
        handle: wlan_handle,
        context: context as *mut std::ffi::c_void,
    })
}
