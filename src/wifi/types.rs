/// WiFi network information
#[derive(Debug, Default, Clone)]
pub struct WifiInfo {
    pub ssid: String,
    pub authentication: String,
    pub encryption: String,
    pub signal: u8,
    pub is_saved: bool,
    pub is_connected: bool,
    pub auto_connect: bool,
    pub phy_type: String,
    pub channel: u32,
    /// Center frequency in Hz. `0` means the backend does not expose it.
    pub frequency: u64,
    pub link_speed: Option<u32>,
    /// BSSID of the access point represented by this row, when exposed.
    pub bssid: Option<String>,
}

impl WifiInfo {
    /// Fold a second observation of the same network into this row.
    ///
    /// Radio metadata (signal, channel, frequency, PHY type, BSSID) is replaced only
    /// when the incoming observation comes from a connected radio or is strictly
    /// stronger. Saved/connected/auto-connect flags accumulate, and the connected
    /// radio always contributes its link speed and BSSID.
    pub(crate) fn merge_observation(&mut self, incoming: &WifiInfo) {
        let replace_radio = if incoming.is_connected != self.is_connected {
            incoming.is_connected
        } else {
            incoming.signal > self.signal
        };
        if replace_radio {
            self.signal = incoming.signal;
            self.channel = incoming.channel;
            self.frequency = incoming.frequency;
            self.phy_type = incoming.phy_type.clone();
            self.bssid = incoming.bssid.clone();
        }
        self.is_saved |= incoming.is_saved;
        self.is_connected |= incoming.is_connected;
        self.auto_connect |= incoming.auto_connect;
        if incoming.is_connected {
            self.link_speed = incoming.link_speed;
            self.bssid = incoming.bssid.clone();
        }
    }
}

/// Sort network rows for display: connected first, then saved, then strongest signal,
/// with the SSID as a final tiebreaker so equal-priority rows stay stable across refreshes.
pub(crate) fn sort_wifi_infos(list: &mut [WifiInfo]) {
    list.sort_by(|left, right| {
        right
            .is_connected
            .cmp(&left.is_connected)
            .then_with(|| right.is_saved.cmp(&left.is_saved))
            .then_with(|| right.signal.cmp(&left.signal))
            .then_with(|| left.ssid.cmp(&right.ssid))
    });
}

/// Normalize a hardware address string to lowercase without surrounding whitespace.
/// Returns `None` for empty addresses.
#[cfg(target_os = "linux")]
pub(crate) fn normalize_bssid(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_prefers_connected_radio_over_stronger_signal() {
        let mut stored = WifiInfo {
            ssid: "net".to_string(),
            signal: 90,
            channel: 6,
            frequency: 2_437_000,
            phy_type: "802.11ax (Wi-Fi 6)".to_string(),
            bssid: Some("aa:aa".to_string()),
            ..WifiInfo::default()
        };
        let incoming = WifiInfo {
            ssid: "net".to_string(),
            signal: 10,
            is_connected: true,
            channel: 11,
            frequency: 2_462_000,
            link_speed: Some(100),
            bssid: Some("bb:bb".to_string()),
            ..WifiInfo::default()
        };

        stored.merge_observation(&incoming);

        assert!(stored.is_connected);
        assert_eq!(stored.signal, 10);
        assert_eq!(stored.channel, 11);
        assert_eq!(stored.link_speed, Some(100));
        assert_eq!(stored.bssid.as_deref(), Some("bb:bb"));
    }

    #[test]
    fn merge_keeps_stronger_radio_metadata_and_accumulates_flags() {
        let mut stored = WifiInfo {
            ssid: "net".to_string(),
            signal: 80,
            channel: 1,
            is_saved: false,
            ..WifiInfo::default()
        };
        let incoming = WifiInfo {
            ssid: "net".to_string(),
            signal: 40,
            channel: 36,
            frequency: 5_180_000,
            is_saved: true,
            auto_connect: true,
            bssid: Some("cc:cc".to_string()),
            ..WifiInfo::default()
        };

        stored.merge_observation(&incoming);

        assert_eq!(stored.signal, 80);
        assert_eq!(stored.channel, 1);
        assert!(stored.is_saved);
        assert!(stored.auto_connect);
        assert!(!stored.is_connected);
        // Not connected: BSSID stays with the stronger radio.
        assert!(stored.bssid.is_none());
    }

    #[test]
    fn merge_weak_duplicate_keeps_existing_radio() {
        let mut stored = WifiInfo {
            ssid: "net".to_string(),
            signal: 70,
            channel: 6,
            bssid: Some("aa:aa".to_string()),
            ..WifiInfo::default()
        };
        let incoming = WifiInfo {
            ssid: "net".to_string(),
            signal: 50,
            channel: 149,
            ..WifiInfo::default()
        };

        stored.merge_observation(&incoming);

        assert_eq!(stored.signal, 70);
        assert_eq!(stored.channel, 6);
        assert_eq!(stored.bssid.as_deref(), Some("aa:aa"));
    }

    #[test]
    fn sorting_puts_connected_first_then_saved_then_signal_then_ssid() {
        let saved = |ssid: &str, signal: u8| WifiInfo {
            ssid: ssid.to_string(),
            is_saved: true,
            signal,
            ..WifiInfo::default()
        };
        let mut list = vec![
            saved("weak-saved", 10),
            WifiInfo {
                ssid: "plain".to_string(),
                signal: 99,
                ..WifiInfo::default()
            },
            WifiInfo {
                ssid: "conn".to_string(),
                is_connected: true,
                signal: 1,
                ..WifiInfo::default()
            },
            saved("strong-saved", 80),
        ];

        sort_wifi_infos(&mut list);

        let ssids: Vec<&str> = list.iter().map(|w| w.ssid.as_str()).collect();
        assert_eq!(ssids, vec!["conn", "strong-saved", "weak-saved", "plain"]);
    }

    #[test]
    fn sorting_ties_break_alphabetically_for_stability() {
        let mut list = vec![
            WifiInfo {
                ssid: "zeta".to_string(),
                signal: 50,
                ..WifiInfo::default()
            },
            WifiInfo {
                ssid: "alpha".to_string(),
                signal: 50,
                ..WifiInfo::default()
            },
        ];

        sort_wifi_infos(&mut list);

        let ssids: Vec<&str> = list.iter().map(|w| w.ssid.as_str()).collect();
        assert_eq!(ssids, vec!["alpha", "zeta"]);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn normalizes_bssids() {
        assert_eq!(
            normalize_bssid(" 00:11:22:AA:BB:CC "),
            Some("00:11:22:aa:bb:cc".to_string())
        );
        assert_eq!(normalize_bssid("   "), None);
        assert_eq!(normalize_bssid(""), None);
    }
}

/// Connection events from the WiFi listener
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Connected(String),
    Disconnected,
    #[allow(dead_code)]
    Failed {
        ssid: String,
        reason_code: u32,
        reason_str: String,
    },
}
