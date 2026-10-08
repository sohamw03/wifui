//! Inline Ratatui saved-network connection command.

use crate::{
    app::AppState,
    config::{self, IconSet},
    error::WifiResult,
    theme,
    wifi::{
        WifiInfo, connect_profile, disconnect, disconnect_and_wait, forget_network,
        get_connected_ssid, get_saved_profiles, get_wifi_networks, scan_networks,
    },
};
use color_eyre::eyre::{Result, eyre};
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    style::Print,
    terminal::disable_raw_mode,
};
use ratatui::{
    DefaultTerminal, TerminalOptions, Viewport,
    layout::Position,
    prelude::{Frame, Line, Modifier, Span, Style},
    widgets::{Block, List, ListItem, ListState},
};
use std::{
    collections::HashSet,
    io::{self, IsTerminal, Write},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum RowStatus<'a> {
    Normal,
    Connecting(&'a str),
    Disconnecting(&'a str),
    Connected(&'a str),
    Disconnected(&'a str),
    Failed(&'a str),
}

#[derive(Clone, Copy)]
enum Operation {
    Connect,
    Disconnect,
}

#[derive(Clone, Copy)]
enum WaitOutcome {
    Completed,
    Cancelled,
}

/// Restores raw mode and the cursor without touching the alternate screen.
struct InlineTerminalGuard;

impl Drop for InlineTerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show);
        let _ = stdout.flush();
    }
}

pub fn run(search_term: &str, use_ascii_icons: bool) -> Result<()> {
    let saved_profiles = get_saved_profiles()
        .map_err(|error| eyre!(format!("Could not read saved networks: {error}")))?;
    // Best-effort scan: warn and continue with whatever metadata can be
    // read; `connect_profile` works even when the target was not scanned.
    if let Err(error) = scan_networks() {
        eprintln!("Warning: could not scan current networks: {error}");
    }
    thread::sleep(Duration::from_millis(config::SCAN_DELAY_MS));
    let network_metadata = match get_wifi_networks() {
        Ok(networks) => networks,
        Err(error) => {
            eprintln!("Warning: could not read current networks: {error}");
            Vec::new()
        }
    };
    let connected_ssid = get_connected_ssid().ok().flatten();
    let mut rows = matching_rows(
        search_term,
        saved_profiles,
        &network_metadata,
        connected_ssid.as_deref(),
    );

    if rows.is_empty() {
        println!("No saved networks matched \"{search_term}\".");
        return Ok(());
    }

    let icon_set = if use_ascii_icons {
        IconSet::Ascii
    } else {
        IconSet::Nerd
    };
    let height = rows.len().min(usize::from(u16::MAX)) as u16;
    let mut terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(height),
    })
    .map_err(|error| eyre!(format!("Could not initialize inline terminal: {error}")))?;
    let _terminal_guard = InlineTerminalGuard;
    terminal
        .hide_cursor()
        .map_err(|error| eyre!(format!("Could not hide terminal cursor: {error}")))?;

    let selected = if rows.len() > 1 {
        choose_profile(&mut terminal, &mut rows, icon_set)?
    } else {
        Some(0)
    };

    if rows.is_empty() {
        exit_with_message(&mut terminal, "All matching saved networks were forgotten.")?;
        return Ok(());
    }

    // Out-of-range fallbacks would burn the whole timeout with no chance
    // of success, so leave the entry on screen instead of connecting.
    if let Some(selected) = selected.filter(|&index| !is_out_of_range(&rows[index])) {
        run_inline_operation(&mut terminal, &rows, selected, icon_set)?;
    } else {
        terminal.clear()?;
        draw_rows(
            &mut terminal,
            &rows,
            selected.unwrap_or(0),
            icon_set,
            RowStatus::Normal,
            0,
        )?;
    }

    exit_below_rows(&mut terminal, rows.len())?;
    Ok(())
}

/// Clear the viewport and leave a one-line message where the rows were.
fn exit_with_message(terminal: &mut DefaultTerminal, message: &str) -> Result<()> {
    terminal.clear()?;
    let top = terminal.get_frame().area().top();
    terminal.set_cursor_position(Position { x: 0, y: top })?;
    execute!(terminal.backend_mut(), Show)?;
    terminal.backend_mut().flush()?;
    println!("{message}");
    Ok(())
}

/// Park the cursor below the last drawn row so the prompt lands there.
/// A shrunk viewport still reserves its original height, so only a full one
/// needs the extra newline (which scrolls solely at the screen bottom).
fn exit_below_rows(terminal: &mut DefaultTerminal, drawn: usize) -> Result<()> {
    let area = terminal.get_frame().area();
    let drawn = drawn.min(usize::from(area.height));
    if drawn < usize::from(area.height) {
        let target = area.top().saturating_add(drawn as u16);
        terminal.set_cursor_position(Position { x: 0, y: target })?;
        execute!(terminal.backend_mut(), Show)?;
    } else {
        let bottom = area.bottom().saturating_sub(1);
        terminal.set_cursor_position(Position { x: 0, y: bottom })?;
        execute!(terminal.backend_mut(), Print("\n"), Show)?;
    }
    terminal.backend_mut().flush()?;
    Ok(())
}

fn matching_rows(
    search_term: &str,
    saved_profiles: Vec<String>,
    network_metadata: &[WifiInfo],
    connected_ssid: Option<&str>,
) -> Vec<WifiInfo> {
    let mut seen = HashSet::new();
    let query_lower = search_term.to_lowercase();

    let mut scored: Vec<(WifiInfo, bool)> = saved_profiles
        .into_iter()
        .filter_map(|ssid| {
            if !AppState::matches_search(&ssid, search_term) || !seen.insert(ssid.clone()) {
                return None;
            }

            // Exact substrings rank before subsequence-only hits.
            let exact = ssid.to_lowercase().contains(&query_lower);
            // Attempt saved profiles the scan missed (out of range or stale).
            let is_connected = connected_ssid.is_some_and(|current| current == ssid.as_str());
            let row = match network_metadata.iter().find(|network| network.ssid == ssid) {
                Some(network) => {
                    let mut row = network.clone();
                    row.ssid = ssid;
                    row.is_saved = true;
                    row.is_connected |= is_connected;
                    row
                }
                None => WifiInfo::saved_fallback(ssid, is_connected),
            };
            Some((row, exact))
        })
        .collect();
    // Stable: exact hits keep saved order first, fuzzy hits follow.
    scored.sort_by_key(|(_, exact)| !*exact);
    scored.into_iter().map(|(row, _)| row).collect()
}

fn choose_profile(
    terminal: &mut DefaultTerminal,
    rows: &mut Vec<WifiInfo>,
    icon_set: IconSet,
) -> Result<Option<usize>> {
    let mut selected = 0;
    draw_rows(terminal, rows, selected, icon_set, RowStatus::Normal, 0)?;

    if !io::stdin().is_terminal() {
        return Ok(None);
    }

    loop {
        if !event::poll(Duration::from_millis(config::EVENT_POLL_MS))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Enter => return Ok(Some(selected)),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(None);
            }
            KeyCode::Char('[') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(None);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                selected = (selected + 1).min(rows.len() - 1);
                draw_rows(terminal, rows, selected, icon_set, RowStatus::Normal, 0)?;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                selected = selected.saturating_sub(1);
                draw_rows(terminal, rows, selected, icon_set, RowStatus::Normal, 0)?;
            }
            KeyCode::Char('g') | KeyCode::Home => {
                selected = 0;
                draw_rows(terminal, rows, selected, icon_set, RowStatus::Normal, 0)?;
            }
            KeyCode::Char('G') | KeyCode::End => {
                selected = rows.len() - 1;
                draw_rows(terminal, rows, selected, icon_set, RowStatus::Normal, 0)?;
            }
            // Forget the selected saved network and drop it from the picker,
            // mirroring the TUI `f` key.
            KeyCode::Char('f') | KeyCode::Delete => {
                let ssid = rows[selected].ssid.clone();
                match forget_network(&ssid) {
                    Ok(()) => {
                        rows.remove(selected);
                        if rows.is_empty() {
                            return Ok(None);
                        }
                        selected = selected.min(rows.len() - 1);
                        // Clear the dead lines so the forgotten row doesn't ghost.
                        terminal.clear()?;
                        draw_rows(terminal, rows, selected, icon_set, RowStatus::Normal, 0)?;
                    }
                    Err(_) => {
                        draw_rows(
                            terminal,
                            rows,
                            selected,
                            icon_set,
                            RowStatus::Failed(&ssid),
                            0,
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
}

fn run_inline_operation(
    terminal: &mut DefaultTerminal,
    rows: &[WifiInfo],
    selected: usize,
    icon_set: IconSet,
) -> Result<()> {
    let ssid = rows[selected].ssid.clone();
    let operation = if rows[selected].is_connected
        || matches!(get_connected_ssid(), Ok(Some(current)) if current == ssid)
    {
        Operation::Disconnect
    } else {
        Operation::Connect
    };
    let (result_tx, result_rx) = mpsc::channel();
    let target_ssid = ssid.clone();
    thread::spawn(move || {
        let result = match operation {
            Operation::Connect => connect_saved_profile(&target_ssid),
            Operation::Disconnect => disconnect(),
        };
        let _ = result_tx.send(result);
    });

    let result = wait_for_operation(
        terminal, rows, selected, &ssid, operation, icon_set, result_rx,
    );
    let status = match (operation, result) {
        (_, Ok(WaitOutcome::Cancelled)) => {
            terminal.clear()?;
            draw_rows(terminal, rows, selected, icon_set, RowStatus::Normal, 0)?;
            return Ok(());
        }
        (Operation::Connect, Ok(WaitOutcome::Completed)) => RowStatus::Connected(&ssid),
        (Operation::Disconnect, Ok(WaitOutcome::Completed)) => RowStatus::Disconnected(&ssid),
        (_, Err(_)) => RowStatus::Failed(&ssid),
    };

    terminal.clear()?;
    draw_rows(terminal, rows, selected, icon_set, status, 0)?;
    Ok(())
}

fn connect_saved_profile(ssid: &str) -> WifiResult<()> {
    let current_ssid = get_connected_ssid().ok().flatten();
    if current_ssid
        .as_deref()
        .is_some_and(|current| current != ssid)
    {
        disconnect_and_wait()?;
    }
    connect_profile(ssid)
}

fn wait_for_operation(
    terminal: &mut DefaultTerminal,
    rows: &[WifiInfo],
    selected: usize,
    ssid: &str,
    operation: Operation,
    icon_set: IconSet,
    result_rx: Receiver<WifiResult<()>>,
) -> Result<WaitOutcome> {
    let deadline = Instant::now() + Duration::from_secs(config::CONNECTION_TIMEOUT_SECS);
    let anim_start = Instant::now();
    draw_rows(
        terminal,
        rows,
        selected,
        icon_set,
        loading_status(operation, ssid),
        spinner_frame(anim_start),
    )?;

    loop {
        if cancellation_requested()? {
            return Ok(WaitOutcome::Cancelled);
        }

        match result_rx.try_recv() {
            Ok(Ok(())) => {
                return match operation {
                    Operation::Connect => wait_until_connected(
                        terminal, rows, selected, ssid, icon_set, deadline, anim_start,
                    ),
                    Operation::Disconnect => wait_until_disconnected(
                        terminal, rows, selected, ssid, icon_set, deadline, anim_start,
                    ),
                };
            }
            Ok(Err(error)) => return Err(eyre!(error.to_string())),
            Err(TryRecvError::Disconnected) => {
                return Err(eyre!("Connection worker stopped unexpectedly"));
            }
            Err(TryRecvError::Empty) => {}
        }

        draw_rows(
            terminal,
            rows,
            selected,
            icon_set,
            loading_status(operation, ssid),
            spinner_frame(anim_start),
        )?;
        if Instant::now() >= deadline {
            return Err(eyre!("Connection timed out (No response from OS)"));
        }
        thread::sleep(Duration::from_millis(config::EVENT_POLL_MS));
    }
}

/// Spinner position from wall-clock time so a slow pass never retards it.
fn spinner_frame(started: Instant) -> usize {
    (started.elapsed().as_millis() / u128::from(config::EVENT_POLL_MS)) as usize
}

fn cancellation_requested() -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(false);
    }

    while event::poll(Duration::ZERO)? {
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('c') | KeyCode::Char('[')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                return Ok(true);
            }
            _ => {}
        }
    }

    Ok(false)
}

fn loading_status<'a>(operation: Operation, ssid: &'a str) -> RowStatus<'a> {
    match operation {
        Operation::Connect => RowStatus::Connecting(ssid),
        Operation::Disconnect => RowStatus::Disconnecting(ssid),
    }
}

fn wait_until_connected(
    terminal: &mut DefaultTerminal,
    rows: &[WifiInfo],
    selected: usize,
    ssid: &str,
    icon_set: IconSet,
    deadline: Instant,
    anim_start: Instant,
) -> Result<WaitOutcome> {
    let mut tick = 0;
    loop {
        if cancellation_requested()? {
            return Ok(WaitOutcome::Cancelled);
        }

        if tick % config::STATUS_POLL_FRAMES == 0
            && get_connected_ssid()
                .map_err(|error| eyre!(error.to_string()))?
                .as_deref()
                == Some(ssid)
        {
            return Ok(WaitOutcome::Completed);
        }
        tick = tick.wrapping_add(1);

        draw_rows(
            terminal,
            rows,
            selected,
            icon_set,
            RowStatus::Connecting(ssid),
            spinner_frame(anim_start),
        )?;
        if Instant::now() >= deadline {
            return Err(eyre!("Connection timed out (No response from OS)"));
        }
        thread::sleep(Duration::from_millis(config::EVENT_POLL_MS));
    }
}

fn wait_until_disconnected(
    terminal: &mut DefaultTerminal,
    rows: &[WifiInfo],
    selected: usize,
    ssid: &str,
    icon_set: IconSet,
    deadline: Instant,
    anim_start: Instant,
) -> Result<WaitOutcome> {
    let mut tick = 0;
    loop {
        if cancellation_requested()? {
            return Ok(WaitOutcome::Cancelled);
        }

        if tick % config::STATUS_POLL_FRAMES == 0
            && get_connected_ssid()
                .map_err(|error| eyre!(error.to_string()))?
                .as_deref()
                != Some(ssid)
        {
            return Ok(WaitOutcome::Completed);
        }
        tick = tick.wrapping_add(1);

        draw_rows(
            terminal,
            rows,
            selected,
            icon_set,
            RowStatus::Disconnecting(ssid),
            spinner_frame(anim_start),
        )?;
        if Instant::now() >= deadline {
            return Err(eyre!("Disconnection timed out (No response from OS)"));
        }
        thread::sleep(Duration::from_millis(config::EVENT_POLL_MS));
    }
}

fn draw_rows(
    terminal: &mut DefaultTerminal,
    rows: &[WifiInfo],
    selected: usize,
    icon_set: IconSet,
    status: RowStatus<'_>,
    loading_frame: usize,
) -> Result<()> {
    terminal.draw(|frame| render_rows(frame, rows, selected, icon_set, status, loading_frame))?;
    Ok(())
}

/// Saved rows without scan metadata (no BSSID/channel/frequency/signal) are
/// either out of range or the scan has not observed them yet.
fn is_out_of_range(network: &WifiInfo) -> bool {
    !network.is_connected
        && network.signal == 0
        && network.channel == 0
        && network.frequency == 0
        && network.bssid.is_none()
}

fn render_rows(
    frame: &mut Frame,
    rows: &[WifiInfo],
    selected: usize,
    icon_set: IconSet,
    status: RowStatus<'_>,
    loading_frame: usize,
) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BACKGROUND).fg(theme::FOREGROUND)),
        area,
    );

    let spinner = config::LOADING_CHARS[loading_frame % config::LOADING_CHARS.len()];
    let items: Vec<ListItem> = rows
        .iter()
        .map(|network| {
            let is_connecting = matches!(
                status,
                RowStatus::Connecting(target) if target == network.ssid.as_str()
            );
            let is_disconnecting = matches!(
                status,
                RowStatus::Disconnecting(target) if target == network.ssid.as_str()
            );
            let is_failed = matches!(
                status,
                RowStatus::Failed(target) if target == network.ssid.as_str()
            );
            let is_disconnected = matches!(
                status,
                RowStatus::Disconnected(target) if target == network.ssid.as_str()
            );
            let is_connected = !is_connecting
                && !is_disconnecting
                && !is_failed
                && !is_disconnected
                && (network.is_connected
                    || matches!(
                        status,
                        RowStatus::Connected(target) if target == network.ssid.as_str()
                    ));

            let row_style = if is_connecting {
                Style::default()
                    .fg(theme::YELLOW)
                    .add_modifier(Modifier::BOLD)
            } else if is_disconnecting {
                Style::default()
                    .fg(theme::PURPLE)
                    .add_modifier(Modifier::BOLD)
            } else if is_failed {
                Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)
            } else if is_connected {
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::BLUE)
            };

            let prefix = if is_connecting || is_disconnecting {
                spinner
            } else {
                icon_set.saved()
            };
            let mut spans = vec![Span::styled(prefix, row_style)];
            if is_connecting || is_disconnecting {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(network.ssid.clone(), row_style));

            if is_connected {
                spans.push(Span::styled(icon_set.connected(), row_style));
            }
            if is_connecting {
                spans.push(Span::styled(" connecting...", row_style));
            } else if is_disconnecting {
                spans.push(Span::styled(" disconnecting...", row_style));
            } else if is_failed {
                spans.push(Span::styled(" failed", row_style));
            } else if is_out_of_range(network) {
                spans.push(Span::styled(" (not in range)", row_style));
            } else if network.is_saved {
                let auto_icon = if network.auto_connect {
                    icon_set.auto_on()
                } else {
                    icon_set.auto_off()
                };
                spans.push(Span::styled(format!(" {auto_icon}"), row_style));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(selected.min(rows.len().saturating_sub(1))));
    // The selection highlight must stay visible even when forgetting shrinks
    // the picker to a single row; the Nerd highlight symbol alone is blank.
    let list = List::new(items)
        .highlight_symbol(icon_set.highlight())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(theme::SELECTION_BG),
        );
    frame.render_stateful_widget(list, area, &mut list_state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovered(ssid: &str, is_connected: bool) -> WifiInfo {
        WifiInfo {
            ssid: ssid.to_string(),
            is_saved: true,
            is_connected,
            signal: 80,
            channel: 6,
            frequency: 2_437_000,
            bssid: Some("aa:bb:cc:dd:ee:ff".to_string()),
            ..WifiInfo::default()
        }
    }

    #[test]
    fn matching_rows_includes_saved_profiles_not_in_current_scan() {
        let rows = matching_rows(
            "204",
            vec!["Current 204".to_string(), "Stale 204".to_string()],
            &[discovered("Current 204", false)],
            None,
        );

        assert_eq!(
            rows.iter().map(|row| row.ssid.as_str()).collect::<Vec<_>>(),
            vec!["Current 204", "Stale 204"]
        );
        let stale = &rows[1];
        assert!(stale.is_saved);
        assert!(!stale.is_connected);
        assert!(is_out_of_range(stale));
        assert!(!is_out_of_range(&rows[0]));
    }

    #[test]
    fn matching_rows_marks_connected_fallback_row() {
        let rows = matching_rows("204", vec!["Stale 204".to_string()], &[], Some("Stale 204"));

        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_saved);
        assert!(rows[0].is_connected);
        assert!(!is_out_of_range(&rows[0]));
    }

    #[test]
    fn matching_rows_matches_subsequence_terms() {
        let rows = matching_rows(
            "hme2",
            vec!["Home 204".to_string()],
            &[discovered("Home 204", false)],
            None,
        );

        assert_eq!(
            rows.iter().map(|row| row.ssid.as_str()).collect::<Vec<_>>(),
            vec!["Home 204"]
        );
    }

    #[test]
    fn matching_rows_rejects_out_of_order_terms() {
        let rows = matching_rows(
            "2emh",
            vec!["Home 204".to_string()],
            &[discovered("Home 204", false)],
            None,
        );

        assert!(rows.is_empty());
    }

    #[test]
    fn matching_rows_ranks_exact_hits_before_subsequence() {
        let rows = matching_rows(
            "k4",
            vec!["KP PG 204".to_string(), "K4".to_string()],
            &[],
            None,
        );

        assert_eq!(
            rows.iter().map(|row| row.ssid.as_str()).collect::<Vec<_>>(),
            vec!["K4", "KP PG 204"]
        );
    }
}
