<!-- ![WifUIBanner](Banner Placeholder) -->

# WifUI

[![Downloads](https://img.shields.io/github/downloads/sohamw03/wifui/total)](https://github.com/sohamw03/wifui/releases)

**WifUI** is a blazing fast, lightweight Terminal User Interface (TUI) for managing Wi-Fi connections on Windows. Built with Rust and `ratatui`, it offers a keyboard-centric way to scan, connect, and monitor your network status without leaving the terminal.

## 🚀 Features

- **Network Scanning**: Instantly discover available Wi-Fi networks.
- **Seamless Connection**: Connect to open or WPA2-PSK secured networks.
- **Network Management**: View detailed network info (SSID, BSSID, Signal Strength).
- **Keyboard Driven**: Efficient navigation with Vim-like keybindings.

## 📸 Screenshots

| Home Screen | Password Prompt |
|:---:|:---:|
| ![Home Screen](images/main.png) | ![Password Prompt](images/password.png) |

## 📦 Installation

### Winget (Coming Soon)

```sh
winget install wifui
```

### From Source

Ensure you have the [Rust toolchain](https://www.rust-lang.org/tools/install) installed.

```sh
git clone https://github.com/sohamw03/wifui.git
cd wifui
cargo run --release
```

## 🎮 Usage

Run the application:

```sh
wifui
```

### Keybindings

| Key | Action |
| :--- | :--- |
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Enter` | Connect / Disconnect |
| `r` | Refresh network list |
| `f` | Forget network |
| `q` / `Esc` | Quit |

## 🤝 Contributing

Contributions are welcome! Feel free to open an issue or submit a pull request on [GitHub](https://github.com/sohamw03/wifui).

## 📄 License

This project is licensed under the MIT License.
