/// WiFi network information
#[derive(Debug, Default, Clone)]
pub struct WifiInfo {
    pub ssid: String,
    pub network_type: String,
    pub authentication: String,
    pub encryption: String,
    pub signal: u8,
    pub is_saved: bool,
    pub is_connected: bool,
    pub auto_connect: bool,
    pub phy_type: String,
    pub channel: u32,
    pub frequency: u32,
    pub link_speed: Option<u32>,
}

/// Connection events from the WiFi listener
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Connected(String),
    #[allow(dead_code)]
    Disconnected(String),
    Failed {
        ssid: String,
        #[allow(dead_code)]
        reason_code: u32,
        reason_str: String,
    },
}
