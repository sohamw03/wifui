/// Typed errors for WifUI WiFi operations
use thiserror::Error;

/// Result type alias for WiFi operations
pub type WifiResult<T> = Result<T, WifiError>;

/// Errors that can occur during WiFi operations
#[derive(Error, Debug)]
pub enum WifiError {
    #[error("Failed to open WLAN handle (code: {code})")]
    HandleOpenFailed { code: u32 },

    #[error("Failed to enumerate interfaces (code: {code})")]
    InterfaceEnumFailed { code: u32 },

    #[error("No WiFi interface found")]
    NoInterface,

    #[error("Failed to get available networks (code: {code})")]
    NetworkListFailed { code: u32 },

    #[error("Failed to register notification (code: {code})")]
    NotificationRegistrationFailed { code: u32 },

    #[error("Failed to scan networks (code: {code})")]
    ScanFailed { code: u32 },

    #[error("Failed to connect (code: {code})")]
    ConnectionFailed { code: u32 },

    #[error("Failed to add profile (code: {code}, reason: {reason})")]
    ProfileAddFailed { code: u32, reason: u32 },

    #[error("Failed to get profile (code: {code})")]
    ProfileGetFailed { code: u32 },

    #[error("Failed to set profile (code: {code}, reason: {reason})")]
    ProfileSetFailed { code: u32, reason: u32 },

    #[error("Failed to delete profile (code: {code})")]
    ProfileDeleteFailed { code: u32 },

    #[error("Failed to disconnect (code: {code})")]
    DisconnectFailed { code: u32 },

    #[error("Could not find connectionMode in profile XML")]
    ProfileXmlInvalid,

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Convert a WLAN reason code to a human-readable string
pub fn wlan_reason_to_string(code: u32) -> String {
    match code {
        0 => "Success".to_string(),
        1 => "Unknown Failure".to_string(),
        0x00010001 => "Network Not Compatible".to_string(),
        0x00010002 => "Profile Not Compatible".to_string(),
        0x00028002 => "Association Failed".to_string(),
        0x00028003 => "Association Timeout".to_string(),
        0x00028004 => "Pre-Security Failure".to_string(),
        0x00028005 => "Start Security Failure".to_string(),
        0x00028006 => "Security Failure".to_string(),
        0x00028007 => "Security Timeout".to_string(),
        0x00028008 => "Roaming Failure".to_string(),
        0x00028009 => "Roaming Security Failure".to_string(),
        0x0002800A => "Ad-hoc Security Failure".to_string(),
        0x0002800B => "Driver Disconnected (Possible Wrong Password)".to_string(),
        0x0002800C => "Driver Operation Failure".to_string(),
        0x0002800D => "IHV Not Available".to_string(),
        0x0002800E => "IHV Not Responding".to_string(),
        // ACM reason codes
        0x00038001 => "ACM Base".to_string(),
        0x00038002 => "Connection Failed (Network Not Available or Wrong Password)".to_string(),
        0x00038003 => "Profile Not Found".to_string(),
        0x00038004 => "Profile Already Exists".to_string(),
        0x00038005 => "Profile Name Too Long".to_string(),
        0x00038006 => "Profile Invalid".to_string(),
        0x00038014 => "Connection Failed (Profile Issue)".to_string(),
        0x00050004 => "Incorrect Password".to_string(),
        0x00048005 => "Incorrect Password (Key Exchange Timeout)".to_string(),
        0x00048014 => "Authentication Timeout (Possible Wrong Password)".to_string(),
        0x00080006 => "MSM Security Missing".to_string(),
        _ => format!("Unknown Error (Code: {code}, 0x{code:X})"),
    }
}
