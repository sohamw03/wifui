# WifUI Codebase Index and Support Guide

This document is the quick orientation guide for contributors and coding agents working in WifUI. The user-facing overview and installation instructions remain in [README.md](README.md).

## Project status

WifUI is a Rust terminal UI for Wi-Fi management.

- Windows uses the native Windows WLAN API.
- Linux builds and launches the TUI with a runtime-selected NetworkManager or iwd D-Bus backend.
- Linux initially targets the first usable Wi-Fi interface. NetworkManager is preferred in `auto` mode.
- Linux supports saved-profile auto-connect toggling and profile deletion through the selected daemon.
- Other non-Windows targets compile against an unsupported placeholder backend.
- The frontend talks only to the platform facade in `src/wifi/mod.rs`; UI code should not import Windows APIs.

## Quick commands

Run these from the repository root:

```sh
cargo fmt -- --check
cargo check --all-targets
cargo test
cargo run -- --ascii
```

For a normal Linux development environment, the same checks can be run offline when dependencies are already cached:

```sh
cargo check --offline --all-targets
cargo test --offline
```

`--ascii` avoids requiring a Nerd Font while testing the TUI. The application expects an interactive terminal; use a PTY when testing launch, key handling, or rendering.

## Architecture at a glance

```text
src/main.rs
  ├─ creates AppState
  ├─ initializes the selected Linux backend and starts the initial refresh when available
  └─ initializes the terminal and enters event::run

src/event/mod.rs
  ├─ draws the UI
  ├─ initializes the Windows listener when applicable
  ├─ coordinates background scans, refreshes, and connection results
  └─ dispatches key events to src/event/handlers.rs

src/ui.rs
  └─ renders the shared TUI from AppState

src/wifi/mod.rs
  └─ selects the platform backend at compile time and re-exports the stable API
       └─ Linux dispatches to NetworkManager or iwd over system D-Bus
```

The normal runtime flow is:

1. `main` creates an empty `AppState`.
2. Windows starts an initial scan and connected-SSID query. Unsupported platforms skip this work and clear the startup loading state immediately.
3. `event::run` renders frames, polls input, handles background results, and performs Windows automatic refreshes.
4. `ui::render` displays the network list, details, popups, and the shared error panel.

## Source index

| Path | Responsibility |
| --- | --- |
| `Cargo.toml` | Package metadata, shared dependencies, and target-specific Windows/Linux dependency selection |
| `Cargo.lock` | Resolved dependency versions; retain Windows entries even on Linux |
| `src/main.rs` | CLI arguments, terminal setup/restore, initial backend refresh |
| `src/app.rs` | `AppState` and the network, UI, connection, input, and refresh state models |
| `src/config.rs` | UI dimensions, timing constants, refresh burst sizes, and icons |
| `src/error.rs` | `WifiError`, `WifiResult`, and Windows WLAN reason-code formatting |
| `src/event/mod.rs` | Main async event loop, background task result handling, listener setup |
| `src/event/handlers.rs` | Keyboard handlers, connection/profile actions, search, and QR generation |
| `src/input.rs` | Editable input state and cursor/word navigation |
| `src/ui.rs` | Ratatui rendering for the main view and popups |
| `src/theme.rs` | Shared TUI colors and styles |
| `src/wifi/mod.rs` | Platform facade and compile-time backend selection |
| `src/wifi/types.rs` | Shared `WifiInfo` and `ConnectionEvent` data types |
| `src/wifi/connection.rs` | Windows connect, disconnect, connected-SSID, and network-list operations |
| `src/wifi/profile.rs` | Windows profile XML, saved profiles, passwords, auto-connect, and forget operations |
| `src/wifi/scanning.rs` | Windows scan trigger |
| `src/wifi/listener.rs` | Windows WLAN notification listener |
| `src/wifi/handle.rs` | Safe Windows WLAN handle wrapper |
| `src/wifi/linux.rs` | Linux runtime registry, backend choice, dispatcher, and stable API |
| `src/wifi/linux_network_manager.rs` | NetworkManager system-D-Bus adapter and conversion helpers |
| `src/wifi/linux_iwd.rs` | iwd system-D-Bus adapter, conversion helpers, and temporary credential agent |
| `src/wifi/linux_listener.rs` | Long-lived Linux D-Bus signal worker and shutdown guard |
| `src/wifi/unsupported.rs` | Same placeholder contract for other unsupported targets |
| `dist-workspace.toml` | cargo-dist release targets and installer configuration |
| `wix/main.wxs` | WiX installer template for Windows |

## Wi-Fi backend boundary

`src/wifi/mod.rs` is the only backend boundary. It always compiles the shared types, then selects modules with target configuration:

| Target | Backend | `is_backend_available()` |
| --- | --- | --- |
| Windows | `connection`, `handle`, `listener`, `profile`, `scanning` | `true` |
| Linux | runtime NetworkManager or iwd adapter | `true` after successful initialization |
| Other non-Windows targets | `unsupported` placeholder | `false` |

On Linux, `--backend auto` is the default and chooses NetworkManager before iwd. `--backend nm`
and `--backend iwd` are explicit selections and never silently fall back to another daemon.
Initialization uses the system D-Bus name owner query and the first powered/managed Wi-Fi station.
The Linux dependency is target-specific `zbus`; no Linux D-Bus crate is added to Windows builds.

The stable frontend-facing functions are:

```text
get_wifi_networks          get_connected_ssid       scan_networks
connect_profile            connect_open             connect_with_password
disconnect                 disconnect_and_wait      get_saved_profiles
get_wifi_password          set_auto_connect         forget_network
start_wifi_listener        WifiListener
```

When adding or replacing a backend, preserve these signatures and return `WifiError` values through `WifiResult`. Add target-specific dependencies under a Cargo target dependency table; do not make Windows-only crates normal Linux dependencies.

## State and error handling

- `NetworkState` owns the discovered list, filtered list, and connected SSID.
- `UiState` owns selection-independent display state, popups, loading animation, and `error_message`.
- `ConnectionState` owns connection tasks, the listener, and connection events.
- `RefreshState` owns refresh timing, background refresh channels, burst refreshes, and startup loading state.

Background work must send its result back to the event loop. Refresh failures should set `state.ui.error_message`; do not silently discard a failed refresh. Backend-unavailable, D-Bus, missing-interface, and unsupported-operation failures must remain visible through `WifiError` rather than panic or report success. Passwords must never be included in errors or logs.

Linux phase one supports scanning, refresh, connected-state reporting, connection events, open
connections, password connections, hidden-network provisioning, saved-network detection and
saved-profile connection, and disconnect. iwd secured connections use a temporary credential
agent whose passphrase is restricted to the requested network and cleared after the attempt.
iwd WEP connection requests return a clear unsupported-operation error.

NetworkManager saved-password readback and secured-network QR sharing are supported through
the profile's D-Bus secret API. iwd does not expose stored passphrases through its public D-Bus
API, so iwd secured QR sharing must remain an explanatory unsupported-operation error. The UI
must never produce a secured QR code without credentials.

## Next Linux backend improvements

The next two backend items are intentionally deferred:

1. Reduce refresh D-Bus/API calls by reusing a single network snapshot and avoiding repeated
   connected-state, access-point, and saved-profile queries where the backend already has the
   required data.
2. Add event-driven access-point updates while retaining periodic refresh as a fallback. NetworkManager
   and iwd should report nearby-network additions, removals, and relevant metadata changes through
   their D-Bus signals; Windows should use WLAN scan/network notifications for the equivalent path.

Automated Linux checks do not replace backend validation on real systems. The user must manually test
the iwd backend on a host running iwd and system D-Bus, and manually test the Windows backend on a real
Windows WLAN adapter. Record any backend-specific findings before treating a release as verified.

## Troubleshooting

### Linux shows a startup spinner

The Linux path should set `RefreshState::is_initial_loading` to `false` before entering the event loop when no daemon or interface is available. Check `initialize_backend()` handling in `src/main.rs` and the backend status branch in `src/ui.rs`.

### Linux reports missing Wi-Fi support

Ensure the selected daemon owns its system-D-Bus name and that it exposes a powered Wi-Fi
interface. In `auto` mode NetworkManager is tried first; use `--backend iwd` when iwd is the
intended manager and NetworkManager is not concurrently managing the same interface.

### Windows crates appear in a Linux build

Check that the `windows` crate remains under `[target.'cfg(windows)'.dependencies]` in `Cargo.toml`, and that Windows-only modules remain behind `#[cfg(windows)]`. Windows entries in `Cargo.lock` may remain and should not be manually removed.

### Linux D-Bus operations fail

Linux assumes the system D-Bus is available and that the daemon handles IP configuration. Check
the daemon logs and run the matching explicit backend option. Do not substitute `nmcli`, `iwctl`,
or parsed command output in the adapter; all operations belong in the zbus D-Bus layer.

### Refresh or connection errors are not visible

Inspect the result channel handling in `src/event/mod.rs` and the shared error panel in `src/ui.rs`. The event loop should clear loading flags and set `UiState::error_message` when a background operation returns an error.

### TUI rendering looks wrong

Try `cargo run -- --ascii` in a real terminal. The default icon set expects a Nerd Font, and non-PTY execution cannot reliably validate cursor, alternate-screen, or key-event behavior.

## Release notes

The current cargo-dist configuration targets `x86_64-pc-windows-msvc` and generates a PowerShell installer. The WiX template packages `wifui.exe` and can add it to PATH. Linux support is not currently a managed cargo-dist release target; a Linux host must provide NetworkManager or iwd and system D-Bus.

## Support report template

When reporting a bug, include:

```text
OS and version:
Rust version (`rustc -V`):
Target (`rustc -vV`):
WifUI version/commit:
Command used:
Expected behavior:
Actual behavior:
Terminal and font:
Relevant error text or screenshot:
```
