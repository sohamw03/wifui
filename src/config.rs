/// Centralized configuration constants for WifUI

// UI Dimensions
pub const MAIN_WINDOW_HEIGHT: u16 = 32;
pub const MAIN_WINDOW_WIDTH: u16 = 77;

// Timing
pub const CONNECTION_TIMEOUT_SECS: u64 = 60;
pub const PROFILE_REGISTRATION_DELAY_MS: u64 = 1500;
pub const OPEN_PROFILE_REGISTRATION_DELAY_MS: u64 = 1000;
pub const SCAN_DELAY_MS: u64 = 2000;
pub const AUTO_REFRESH_INTERVAL_SECS: u64 = 10;
pub const SEARCHING_REFRESH_INTERVAL_SECS: u64 = 15;
pub const BURST_REFRESH_INTERVAL_SECS: u64 = 1;
pub const INTERACTION_COOLDOWN_SECS: u64 = 1;
pub const EVENT_POLL_MS: u64 = 100;
pub const MANUAL_REFRESH_DEBOUNCE_MS: u64 = 500;
pub const AUTO_CONNECT_DELAY_SECS: u64 = 5;

// Refresh burst counts
pub const STARTUP_REFRESH_BURST: u8 = 5;
pub const CONNECTION_REFRESH_BURST: u8 = 15;
pub const DISCONNECT_REFRESH_BURST: u8 = 5;

// Loading animation frames
pub const LOADING_CHARS: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// Icons - Nerd Fonts
pub mod icons {
    pub mod nerd {
        pub const SAVED: &str = "󰆓 "; // nf-md-content_save
        pub const OPEN: &str = " "; // nf-fa-rss
        pub const LOCKED: &str = " "; // nf-fa-lock
        pub const CONNECTED: &str = " 󰖩"; // nf-md-wifi_check
        pub const AUTO_ON: &str = "󰁪"; // nf-md-bell
        pub const AUTO_OFF: &str = "󱧧"; // nf-md-bell_off
        pub const HIGHLIGHT: &str = "  "; // Two spaces for alignment
        // UI symbols for help bar and popups
        pub const ENTER: &str = "󰌑"; // nf-md-keyboard_return
        pub const TAB_NEXT: &str = "⇥ / ↓";
        pub const TAB_PREV: &str = "⇤ / ↑";
        pub const SPACE: &str = "󱁐"; // nf-md-keyboard_space
        pub const CHECKBOX_ON: &str = " "; // nf-fa-check_square_o
        pub const CHECKBOX_OFF: &str = " "; // nf-fa-square_o
        pub const BTN_LEFT: &str = "";
        pub const BTN_RIGHT: &str = "";
        pub const ARROW_LEFT: &str = "◀";
        pub const ARROW_RIGHT: &str = "▶";
    }

    pub mod ascii {
        pub const SAVED: &str = "[S] ";
        pub const OPEN: &str = "[O] ";
        pub const LOCKED: &str = "[*] ";
        pub const CONNECTED: &str = " <-";
        pub const AUTO_ON: &str = "(A)";
        pub const AUTO_OFF: &str = "(M)";
        pub const HIGHLIGHT: &str = "> ";
        // UI symbols for help bar and popups
        pub const ENTER: &str = "Enter";
        pub const TAB_NEXT: &str = "Tab/Down";
        pub const TAB_PREV: &str = "S-Tab/Up";
        pub const SPACE: &str = "Space";
        pub const CHECKBOX_ON: &str = "[x]";
        pub const CHECKBOX_OFF: &str = "[ ]";
        pub const BTN_LEFT: &str = "[";
        pub const BTN_RIGHT: &str = "]";
        pub const ARROW_LEFT: &str = "<";
        pub const ARROW_RIGHT: &str = ">";
    }
}

/// Icon set to use based on configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSet {
    Nerd,
    Ascii,
}

impl Default for IconSet {
    fn default() -> Self {
        IconSet::Nerd
    }
}

impl IconSet {
    pub fn saved(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::SAVED,
            IconSet::Ascii => icons::ascii::SAVED,
        }
    }

    pub fn open(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::OPEN,
            IconSet::Ascii => icons::ascii::OPEN,
        }
    }

    pub fn locked(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::LOCKED,
            IconSet::Ascii => icons::ascii::LOCKED,
        }
    }

    pub fn connected(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::CONNECTED,
            IconSet::Ascii => icons::ascii::CONNECTED,
        }
    }

    pub fn auto_on(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::AUTO_ON,
            IconSet::Ascii => icons::ascii::AUTO_ON,
        }
    }

    pub fn auto_off(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::AUTO_OFF,
            IconSet::Ascii => icons::ascii::AUTO_OFF,
        }
    }

    pub fn highlight(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::HIGHLIGHT,
            IconSet::Ascii => icons::ascii::HIGHLIGHT,
        }
    }

    pub fn enter(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::ENTER,
            IconSet::Ascii => icons::ascii::ENTER,
        }
    }

    pub fn tab_next(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::TAB_NEXT,
            IconSet::Ascii => icons::ascii::TAB_NEXT,
        }
    }

    pub fn tab_prev(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::TAB_PREV,
            IconSet::Ascii => icons::ascii::TAB_PREV,
        }
    }

    pub fn space(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::SPACE,
            IconSet::Ascii => icons::ascii::SPACE,
        }
    }

    pub fn checkbox(&self, checked: bool) -> &'static str {
        match self {
            IconSet::Nerd => {
                if checked {
                    icons::nerd::CHECKBOX_ON
                } else {
                    icons::nerd::CHECKBOX_OFF
                }
            }
            IconSet::Ascii => {
                if checked {
                    icons::ascii::CHECKBOX_ON
                } else {
                    icons::ascii::CHECKBOX_OFF
                }
            }
        }
    }

    pub fn btn_left(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::BTN_LEFT,
            IconSet::Ascii => icons::ascii::BTN_LEFT,
        }
    }

    pub fn btn_right(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::BTN_RIGHT,
            IconSet::Ascii => icons::ascii::BTN_RIGHT,
        }
    }

    pub fn arrow_left(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::ARROW_LEFT,
            IconSet::Ascii => icons::ascii::ARROW_LEFT,
        }
    }

    pub fn arrow_right(&self) -> &'static str {
        match self {
            IconSet::Nerd => icons::nerd::ARROW_RIGHT,
            IconSet::Ascii => icons::ascii::ARROW_RIGHT,
        }
    }
}
