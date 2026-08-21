use crate::app::AppState;
use crate::config;
use crate::theme;
use ratatui::{
    prelude::*,
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

/// Bounding boxes of all interactive areas returned by render(), used by mouse hit-testing.
#[derive(Debug, Clone, Default)]
pub struct LayoutAreas {
    /// Inner area of the network list (excluding border)
    pub list_area: Rect,
    /// Outer area of the error panel, if visible
    pub error_area: Option<Rect>,
    /// Outer area of the password popup, if visible
    pub password_popup_area: Option<Rect>,
    /// Outer area of the QR popup, if visible
    pub qr_popup_area: Option<Rect>,
    /// Outer area of the manual-add popup, if visible
    pub manual_popup_area: Option<Rect>,
    /// Per-field bounding boxes inside the manual-add popup (SSID=0, Password=1, Security=2, Hidden=3)
    pub manual_field_areas: [Option<Rect>; 4],
    /// Bounding box of the Connect button inside the manual-add popup
    pub manual_connect_area: Option<Rect>,
}

fn display_auth_name(auth: &str) -> &str {
    match auth {
        "Open" => "Open",
        "WPA-PSK" => "WPA-Personal",
        "WPA2-PSK" => "WPA2-Personal",
        "WPA3-SAE" => "WPA3-Personal",
        "WPA" => "WPA-Enterprise",
        "WPA2" => "WPA2-Enterprise",
        "WPA3" | "WPA3ENT" | "WPA3ENT192" => "WPA3-Enterprise",
        "Shared" => "WEP (Shared)",
        "WEP" => "WEP",
        "OWE" => "Enhanced Open (OWE)",
        "WPA-None" => "WPA-None",
        _ => auth,
    }
}

fn spinner_char(loading_frame: usize) -> &'static str {
    config::LOADING_CHARS[loading_frame % config::LOADING_CHARS.len()]
}

/// Compute the parity-adjusted, centered rectangle for the fixed-size main window.
fn centered_main_area(area: Rect) -> Rect {
    // Adjust width/height to match the parity of the terminal size
    let target_height = config::MAIN_WINDOW_HEIGHT;
    let height = if area.height.is_multiple_of(2) {
        if target_height.is_multiple_of(2) {
            target_height
        } else {
            target_height + 1
        }
    } else if !target_height.is_multiple_of(2) {
        target_height
    } else {
        target_height + 1
    };

    let target_width = config::MAIN_WINDOW_WIDTH;
    let width = if area.width.is_multiple_of(2) {
        if target_width.is_multiple_of(2) {
            target_width
        } else {
            target_width + 1
        }
    } else if !target_width.is_multiple_of(2) {
        target_width
    } else {
        target_width + 1
    };

    // Center the main window
    let vertical_layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);

    let horizontal_layout = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .split(vertical_layout[1]);

    horizontal_layout[1]
}

/// Compute the visible substring of a single-line input and the on-screen cursor column,
/// scrolling the viewport so the cursor stays within `max_width` columns.
fn scrolled_input_view(text: &str, cursor_pos: usize, max_width: usize) -> (String, usize) {
    if max_width == 0 {
        return (String::new(), 0);
    }

    let input_len = text.chars().count();
    if input_len < max_width {
        (text.to_string(), cursor_pos)
    } else if cursor_pos >= max_width {
        // If cursor is near the end, show the end
        let skip = cursor_pos - max_width + 1;
        let display: String = text.chars().skip(skip).take(max_width).collect();
        (display, max_width - 1)
    } else {
        // If cursor is at the beginning, show the beginning
        let display: String = text.chars().take(max_width).collect();
        (display, cursor_pos)
    }
}

/// Build the styled spans for one rendered single-line input.
///
/// A block-style cursor is drawn over the character at `cursor_x` (or as a trailing
/// space when the cursor sits past the end) while `cursor_active` is set. When
/// `dim_inactive` is set, non-cursor characters use the dimmed style.
fn input_line_spans(
    display_text: &str,
    cursor_x: usize,
    cursor_active: bool,
    dim_inactive: bool,
) -> Vec<Span<'static>> {
    let cursor_style = Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND);
    let mut spans = Vec::new();

    for (i, c) in display_text.chars().enumerate() {
        if i == cursor_x && cursor_active {
            spans.push(Span::styled(c.to_string(), cursor_style));
        } else if dim_inactive {
            spans.push(Span::styled(
                c.to_string(),
                Style::default().fg(theme::DIMMED),
            ));
        } else {
            spans.push(Span::raw(c.to_string()));
        }
    }

    if cursor_x == display_text.chars().count() && cursor_active {
        spans.push(Span::styled(" ", cursor_style));
    }

    spans
}

pub fn render(frame: &mut Frame, state: &mut AppState) -> LayoutAreas {
    let mut areas = LayoutAreas::default();
    let area = frame.area();
    let is_dimmed = state.is_popup_open();

    // Set background color for the entire screen
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BACKGROUND).fg(theme::FOREGROUND)),
        area,
    );

    // Calculate dynamic dimensions to ensure perfect centering
    let main_area = centered_main_area(area);

    let border_style = Style::default().fg(theme::DIMMED);

    let title_style = if is_dimmed {
        Style::default()
            .fg(theme::DIMMED)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::CYAN)
            .add_modifier(Modifier::BOLD)
    };

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title_top(
            Line::from(Span::styled(
                format!(" WIFUI v{} ", env!("CARGO_PKG_VERSION")),
                title_style,
            ))
            .centered(),
        )
        .title_bottom(
            Line::from(Span::styled(
                format!(" {} ", crate::wifi::backend_name()),
                border_style,
            ))
            .right_aligned(),
        );

    frame.render_widget(main_block, main_area);

    let inner_area = main_area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });

    let mut constraints = vec![
        Constraint::Min(9),     // Network list
        Constraint::Length(10), // Details
        Constraint::Length(2),  // Bottom bar
    ];

    if state.ui.is_searching || !state.inputs.search_input.value.is_empty() {
        constraints.insert(0, Constraint::Length(3));
    }

    let content_layout = Layout::vertical(constraints).split(inner_area);

    let (search_area, list_area, details_area, help_area) =
        if state.ui.is_searching || !state.inputs.search_input.value.is_empty() {
            (
                Some(content_layout[0]),
                content_layout[1],
                content_layout[2],
                content_layout[3],
            )
        } else {
            (
                None,
                content_layout[0],
                content_layout[1],
                content_layout[2],
            )
        };

    if let Some(area) = search_area {
        render_search_bar(frame, area, state, is_dimmed);
    }

    render_networks_panel(frame, list_area, details_area, state, is_dimmed, &mut areas);

    render_help_bar(frame, help_area, state);

    if let Some(error) = &state.ui.error_message {
        let error_area = Rect::new(
            area.x.saturating_add(2),
            area.height.saturating_sub(4),
            area.width.saturating_sub(4),
            3,
        );
        areas.error_area = Some(error_area);
        let error_paragraph = Paragraph::new(error.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme::RED))
                    .title(" ERROR "),
            )
            .style(Style::default().fg(theme::RED).bg(theme::BACKGROUND))
            .wrap(Wrap { trim: true });
        frame.render_widget(Clear, error_area);
        frame.render_widget(error_paragraph, error_area);
    }

    if state.ui.show_password_popup {
        render_password_popup(frame, list_area, state, &mut areas);
    }

    if state.ui.show_manual_add_popup {
        render_manual_add_popup(frame, list_area, state, &mut areas);
    }

    render_key_logger(frame, main_area, state);

    // QR Code popup
    if state.ui.show_qr_popup {
        render_qr_popup(frame, area, state, &mut areas);
    }

    areas
}

fn render_search_bar(frame: &mut Frame, area: Rect, state: &AppState, is_dimmed: bool) {
    let search_style = if is_dimmed {
        Style::default().fg(theme::DIMMED)
    } else if state.ui.is_searching {
        Style::default().fg(theme::YELLOW)
    } else {
        Style::default().fg(theme::CYAN)
    };

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Search (/) ")
        .border_style(search_style);

    let max_width = (area.width.saturating_sub(2)) as usize;
    let (display_text, cursor_x) = scrolled_input_view(
        &state.inputs.search_input.value,
        state.inputs.search_input.cursor,
        max_width,
    );

    let spans = input_line_spans(
        &display_text,
        cursor_x,
        state.ui.is_searching && !is_dimmed,
        is_dimmed,
    );

    let search_text = Paragraph::new(Line::from(spans)).block(search_block);

    frame.render_widget(search_text, area);
}

fn combined_list_details_area(list_area: Rect, details_area: Rect) -> Rect {
    Rect {
        x: list_area.x,
        y: list_area.y,
        width: list_area.width,
        height: list_area.height + details_area.height,
    }
}

fn render_networks_panel(
    frame: &mut Frame,
    list_area: Rect,
    details_area: Rect,
    state: &mut AppState,
    is_dimmed: bool,
    areas: &mut LayoutAreas,
) {
    if state.refresh.is_initial_loading {
        render_loading_view(
            frame,
            combined_list_details_area(list_area, details_area),
            state.ui.loading_frame,
        );
    } else if !crate::wifi::is_backend_available() {
        render_unavailable_view(frame, combined_list_details_area(list_area, details_area));
    } else {
        render_network_list(frame, list_area, state, is_dimmed, areas);
        render_details_panel(frame, details_area, state, is_dimmed);
    }
}

fn render_loading_view(frame: &mut Frame, combined_area: Rect, loading_frame: usize) {
    let inner_height = combined_area.height.saturating_sub(2);
    let text_height = 2u16;
    let top_padding = inner_height.saturating_sub(text_height) / 2;

    let padded_block = Block::default()
        .title(" Networks ")
        .title_style(
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BLUE))
        .padding(Padding::new(0, 0, top_padding, 0));

    let spinner_paragraph = Paragraph::new(vec![
        Line::from(Span::styled(
            spinner_char(loading_frame),
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Scanning networks...",
            Style::default().fg(theme::FOREGROUND),
        )),
    ])
    .block(padded_block)
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false });

    frame.render_widget(spinner_paragraph, combined_area);
}

fn render_unavailable_view(frame: &mut Frame, combined_area: Rect) {
    let unavailable = Paragraph::new(crate::wifi::backend_unavailable_message())
        .block(
            Block::default()
                .title(" Networks ")
                .title_style(
                    Style::default()
                        .fg(theme::BLUE)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::BLUE))
                .padding(Padding::new(1, 1, 0, 0)),
        )
        .style(Style::default().fg(theme::YELLOW))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(unavailable, combined_area);
}

fn render_network_list(
    frame: &mut Frame,
    list_area: Rect,
    state: &mut AppState,
    is_dimmed: bool,
    areas: &mut LayoutAreas,
) {
    let icons = state.ui.icon_set;
    let spinner = spinner_char(state.ui.loading_frame);
    let connecting_ssid = if state.connection.is_connecting {
        state.connection.target_ssid.as_deref()
    } else {
        None
    };
    let disconnecting_ssid = if state.connection.is_disconnecting {
        state.connection.disconnecting_ssid.as_deref()
    } else {
        None
    };

    let list_items: Vec<ListItem> = state
        .network
        .filtered_wifi_list
        .iter()
        .enumerate()
        .map(|(index, w)| {
            let is_this_connecting = connecting_ssid.is_some_and(|s| s == w.ssid.as_str());
            let is_this_disconnecting = disconnecting_ssid.is_some_and(|s| s == w.ssid.as_str());
            let is_connected = (w.is_connected
                || state
                    .network
                    .connected_ssid
                    .as_deref()
                    .is_some_and(|ssid| ssid == w.ssid))
                && !is_this_disconnecting;

            // Preserve the original row-wide coloring: saved rows are blue,
            // connected rows are green and bold, connecting is yellow, and disconnecting is orange.
            // Mouse hover gets a subtle background when nothing else overrides.
            let row_style = if is_dimmed {
                if is_connected {
                    Style::default()
                        .fg(theme::DIMMED)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::DIMMED)
                }
            } else if is_this_connecting {
                Style::default()
                    .fg(theme::YELLOW)
                    .add_modifier(Modifier::BOLD)
            } else if is_this_disconnecting {
                Style::default()
                    .fg(theme::PURPLE)
                    .add_modifier(Modifier::BOLD)
            } else if is_connected {
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::BOLD)
            } else if w.is_saved {
                Style::default().fg(theme::BLUE)
            } else if state.mouse.hovered_row == Some(index) {
                Style::default().bg(theme::HOVER_BG)
            } else {
                Style::default()
            };

            // Prefix: spinner while connecting/disconnecting, otherwise the usual icon
            let prefix_text = if is_this_connecting || is_this_disconnecting {
                spinner
            } else if w.is_saved {
                icons.saved()
            } else if w.authentication == "Open" {
                icons.open()
            } else {
                icons.locked()
            };

            let mut spans = vec![
                Span::styled(prefix_text, row_style),
                // Spinner char has no trailing space; icons do — add one to keep width stable
                if is_this_connecting || is_this_disconnecting {
                    Span::raw(" ")
                } else {
                    Span::raw("")
                },
            ];

            // SSID text
            spans.push(Span::styled(w.ssid.clone(), row_style));

            // Connected indicator
            if is_connected {
                spans.push(Span::styled(icons.connected(), row_style));
            }

            // Suffix: "connecting..." / "disconnecting...", otherwise auto-connect status
            if is_this_connecting {
                spans.push(Span::styled(" connecting...", row_style));
            } else if is_this_disconnecting {
                spans.push(Span::styled(" disconnecting...", row_style));
            } else if w.is_saved {
                if w.auto_connect {
                    spans.push(Span::styled(format!(" {}", icons.auto_on()), row_style));
                } else {
                    spans.push(Span::styled(format!(" {}", icons.auto_off()), row_style));
                }
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list_border_style = if is_dimmed {
        Style::default().fg(theme::DIMMED)
    } else {
        Style::default().fg(theme::BLUE)
    };

    let list_title_style = if is_dimmed {
        Style::default()
            .fg(theme::DIMMED)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::BLUE)
            .add_modifier(Modifier::BOLD)
    };

    let networks_title = if state.refresh.is_refreshing_networks {
        Line::from(vec![
            Span::styled(" Networks ", list_title_style),
            Span::styled(spinner_char(state.ui.loading_frame), list_title_style),
            Span::raw(" "),
        ])
    } else {
        Line::from(Span::styled(" Networks ", list_title_style))
    };

    let list = List::new(list_items)
        .block(
            Block::default()
                .title(networks_title)
                .title_style(list_title_style)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(list_border_style),
        )
        .highlight_symbol(icons.highlight())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(if is_dimmed {
                    theme::BACKGROUND
                } else {
                    theme::SELECTION_BG
                }),
        );

    frame.render_stateful_widget(list, list_area, &mut state.ui.l_state);
    areas.list_area = list_area;

    let viewport_height = list_area.height.saturating_sub(2) as usize;
    let content_len = state.network.filtered_wifi_list.len();

    let mut scroll_state = ScrollbarState::new(content_len)
        .position(state.ui.l_state.selected().unwrap_or(0))
        .viewport_content_length(viewport_height);

    if content_len > viewport_height {
        let scrollbar_style = if is_dimmed {
            Style::default().fg(theme::DIMMED)
        } else {
            Style::default().fg(theme::BLUE)
        };

        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some(""))
            .end_symbol(Some(""))
            .thumb_symbol("█")
            .track_symbol(Some("│"))
            .style(scrollbar_style);

        frame.render_stateful_widget(
            scrollbar,
            list_area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scroll_state,
        );
    }
}

fn render_details_panel(frame: &mut Frame, details_area: Rect, state: &AppState, is_dimmed: bool) {
    let icons = state.ui.icon_set;

    if let Some(selected) = state.ui.l_state.selected()
        && let Some(wifi) = state.network.filtered_wifi_list.get(selected)
    {
        let label_style = if is_dimmed {
            Style::default().fg(theme::DIMMED)
        } else {
            Style::default().fg(theme::CYAN)
        };

        let value_style = if is_dimmed {
            Style::default().fg(theme::DIMMED)
        } else {
            Style::default()
        };

        let label = |text: &str| Span::styled(format!("{:>11} ", text), label_style);

        let sec_icon = if wifi.authentication == "Open" {
            icons.open()
        } else {
            icons.locked()
        };
        let saved_icon = icons.saved();

        let signal_bar_width = (wifi.signal as usize / 10).min(10);
        let signal_color = if is_dimmed {
            theme::DIMMED
        } else if wifi.signal > 70 {
            theme::GREEN
        } else if wifi.signal > 40 {
            theme::YELLOW
        } else {
            theme::RED
        };
        let signal_bar = "█".repeat(signal_bar_width) + &"░".repeat(10 - signal_bar_width);
        let channel_text = if wifi.channel == 0 || wifi.frequency == 0 {
            "Unknown".to_string()
        } else {
            format!(
                "{} @ {:.3} GHz",
                wifi.channel,
                wifi.frequency as f64 / 1_000_000.0
            )
        };

        let mut info = vec![
            if wifi.is_connected {
                Line::from(vec![
                    label("Status"),
                    Span::styled(
                        format!("{} Connected ", icons.connected().trim()),
                        if is_dimmed {
                            Style::default().fg(theme::DIMMED)
                        } else {
                            Style::default()
                                .fg(theme::GREEN)
                                .add_modifier(Modifier::BOLD)
                        },
                    ),
                    Span::styled(
                        format!("{}Saved", saved_icon),
                        if is_dimmed {
                            Style::default().fg(theme::DIMMED)
                        } else {
                            Style::default().fg(theme::BLUE)
                        },
                    ),
                ])
            } else if wifi.is_saved {
                Line::from(vec![
                    label("Status"),
                    Span::styled(
                        format!("{}Saved", saved_icon),
                        if is_dimmed {
                            Style::default().fg(theme::DIMMED)
                        } else {
                            Style::default().fg(theme::BLUE)
                        },
                    ),
                ])
            } else {
                Line::from(vec![
                    label("Status"),
                    Span::styled(
                        "Available",
                        if is_dimmed {
                            Style::default().fg(theme::DIMMED)
                        } else {
                            value_style
                        },
                    ),
                ])
            },
            Line::from(vec![
                label("SSID"),
                Span::styled(
                    wifi.ssid.to_string(),
                    value_style.add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                label("Signal"),
                Span::styled(format!("{}% ", wifi.signal), value_style),
                Span::styled(signal_bar, Style::default().fg(signal_color)),
            ]),
            Line::from(vec![
                label("Security"),
                Span::styled(
                    format!(
                        "{}{} / {}",
                        sec_icon,
                        display_auth_name(&wifi.authentication),
                        wifi.encryption
                    ),
                    value_style,
                ),
            ]),
            Line::from(vec![
                label("Standard"),
                Span::styled(wifi.phy_type.to_string(), value_style),
            ]),
            Line::from(vec![
                label("Channel"),
                Span::styled(channel_text, value_style),
            ]),
        ];

        if wifi.is_saved {
            let auto_text = if wifi.auto_connect {
                format!("{} Enabled", icons.auto_on())
            } else {
                format!("{} Disabled", icons.auto_off())
            };
            info.push(Line::from(vec![
                label("Auto-Conn"),
                Span::styled(auto_text, value_style),
            ]));
        }

        if let Some(speed) = wifi.link_speed {
            info.push(Line::from(vec![
                label("Link Speed"),
                Span::styled(format!("{} Mbps", speed), value_style),
            ]));
        }

        let details_border_style = if is_dimmed {
            Style::default().fg(theme::DIMMED)
        } else {
            Style::default().fg(theme::PURPLE)
        };

        let details_title_style = if is_dimmed {
            Style::default()
                .fg(theme::DIMMED)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme::PURPLE)
                .add_modifier(Modifier::BOLD)
        };

        let paragraph = Paragraph::new(info).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Details ")
                .title_style(details_title_style)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(details_border_style)
                .padding(Padding::new(1, 1, 0, 0)),
        );
        frame.render_widget(paragraph, details_area);
    }
}

fn render_help_bar(frame: &mut Frame, help_area: Rect, state: &AppState) {
    let icons = state.ui.icon_set;

    let help_text = if state.ui.show_password_popup {
        // Password input active - show password-specific shortcuts
        vec![Line::from(vec![
            Span::styled(icons.enter(), Style::default().fg(theme::FOREGROUND)),
            Span::styled(" connect • ", Style::default().fg(theme::DIMMED)),
            Span::styled("esc", Style::default().fg(theme::FOREGROUND)),
            Span::styled(" cancel", Style::default().fg(theme::DIMMED)),
        ])]
    } else if state.ui.show_manual_add_popup {
        // Manual add popup active - show relevant navigation & actions
        vec![
            Line::from(vec![
                Span::styled(icons.tab_next(), Style::default().fg(theme::FOREGROUND)),
                Span::styled(" next • ", Style::default().fg(theme::DIMMED)),
                Span::styled(icons.tab_prev(), Style::default().fg(theme::FOREGROUND)),
                Span::styled(" prev • ", Style::default().fg(theme::DIMMED)),
                Span::styled(icons.enter(), Style::default().fg(theme::FOREGROUND)),
                Span::styled(" connect • ", Style::default().fg(theme::DIMMED)),
                Span::styled("esc", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" cancel", Style::default().fg(theme::DIMMED)),
            ]),
            Line::from(vec![
                Span::styled(icons.space(), Style::default().fg(theme::FOREGROUND)),
                Span::styled(" checkbox • ", Style::default().fg(theme::DIMMED)),
                Span::styled("h/l/j/k", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" dropdown", Style::default().fg(theme::DIMMED)),
            ]),
        ]
    } else if state.ui.is_searching || !state.inputs.search_input.value.is_empty() {
        // Search active - show search-specific shortcuts
        vec![Line::from(vec![
            Span::styled(icons.enter(), Style::default().fg(theme::FOREGROUND)),
            Span::styled(" apply • ", Style::default().fg(theme::DIMMED)),
            Span::styled("esc esc", Style::default().fg(theme::FOREGROUND)),
            Span::styled(" cancel", Style::default().fg(theme::DIMMED)),
        ])]
    } else {
        // Default global help
        vec![
            Line::from(vec![
                Span::styled("q", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" quit • ", Style::default().fg(theme::DIMMED)),
                Span::styled("j/k", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" nav • ", Style::default().fg(theme::DIMMED)),
                Span::styled(icons.enter(), Style::default().fg(theme::FOREGROUND)),
                Span::styled(" conn / dconn • ", Style::default().fg(theme::DIMMED)),
                Span::styled("f", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" forget • ", Style::default().fg(theme::DIMMED)),
                Span::styled("r", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" refresh", Style::default().fg(theme::DIMMED)),
            ]),
            Line::from(vec![
                Span::styled("a", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" auto-conn • ", Style::default().fg(theme::DIMMED)),
                Span::styled("s", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" share • ", Style::default().fg(theme::DIMMED)),
                Span::styled("n", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" add • ", Style::default().fg(theme::DIMMED)),
                Span::styled("/", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" search • ", Style::default().fg(theme::DIMMED)),
                Span::styled("esc", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" back", Style::default().fg(theme::DIMMED)),
            ]),
        ]
    };

    let help_paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(theme::DIMMED))
        .alignment(Alignment::Center);

    frame.render_widget(help_paragraph, help_area);
}

fn render_password_popup(
    frame: &mut Frame,
    list_area: Rect,
    state: &AppState,
    areas: &mut LayoutAreas,
) {
    let networks_area = list_area;
    let popup_height = 3;
    let popup_area = Rect {
        x: networks_area.x,
        y: networks_area.y + networks_area.height.saturating_sub(popup_height),
        width: networks_area.width,
        height: popup_height,
    };
    areas.password_popup_area = Some(popup_area);

    let popup_text: String = state
        .inputs
        .password_input
        .value
        .chars()
        .map(|_| '•')
        .collect();

    let max_width = (popup_area.width.saturating_sub(4)) as usize;
    let (display_text, cursor_x) =
        scrolled_input_view(&popup_text, state.inputs.password_input.cursor, max_width);

    let spans = input_line_spans(&display_text, cursor_x, true, false);

    let popup_block = Block::default()
        .title(format!(
            " Password for {} ",
            state
                .connection
                .pending_password_ssid
                .as_deref()
                .unwrap_or("")
        ))
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::YELLOW))
        .padding(Padding::new(1, 1, 0, 0)); // Add padding to center vertically

    let popup = Paragraph::new(Line::from(spans))
        .block(popup_block)
        .style(Style::default().fg(theme::FOREGROUND).bg(theme::BACKGROUND))
        .alignment(Alignment::Left);

    frame.render_widget(Clear, popup_area);
    frame.render_widget(popup, popup_area);
}

fn render_manual_add_popup(
    frame: &mut Frame,
    list_area: Rect,
    state: &AppState,
    areas: &mut LayoutAreas,
) {
    let icons = state.ui.icon_set;
    let networks_area = list_area;
    let popup_height = 13;
    let popup_area = Rect {
        x: networks_area.x,
        y: networks_area.y + networks_area.height.saturating_sub(popup_height),
        width: networks_area.width,
        height: popup_height,
    };
    areas.manual_popup_area = Some(popup_area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Add Network ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(theme::CYAN).bg(theme::BACKGROUND));

    frame.render_widget(block.clone(), popup_area);

    let inner = popup_area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    let layout = Layout::vertical([
        Constraint::Length(3), // SSID
        Constraint::Length(3), // Password
        Constraint::Length(3), // Security
        Constraint::Length(1), // Spacer
        Constraint::Length(1), // Hidden + Connect
    ])
    .split(inner);

    // Capture field areas for mouse hit-testing
    areas.manual_field_areas[0] = Some(layout[0]);
    areas.manual_field_areas[1] = Some(layout[1]);
    areas.manual_field_areas[2] = Some(layout[2]);

    // SSID Input
    let ssid_style = if state.inputs.manual_input_field == 0 {
        Style::default().fg(theme::YELLOW)
    } else {
        Style::default().fg(theme::FOREGROUND)
    };
    let ssid_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" SSID ")
        .border_style(ssid_style)
        .style(Style::default().bg(theme::BACKGROUND));

    let max_width_ssid = (layout[0].width.saturating_sub(2)) as usize;
    let (display_ssid, ssid_cursor_x) = scrolled_input_view(
        &state.inputs.manual_ssid_input.value,
        state.inputs.manual_ssid_input.cursor,
        max_width_ssid,
    );

    let ssid_spans = input_line_spans(
        &display_ssid,
        ssid_cursor_x,
        state.inputs.manual_input_field == 0,
        false,
    );

    let ssid_para = Paragraph::new(Line::from(ssid_spans)).block(ssid_block);
    frame.render_widget(ssid_para, layout[0]);

    // Password Input
    let pass_style = if state.inputs.manual_input_field == 1 {
        Style::default().fg(theme::YELLOW)
    } else {
        Style::default().fg(theme::FOREGROUND)
    };
    let pass_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Password ")
        .border_style(pass_style)
        .style(Style::default().bg(theme::BACKGROUND));

    let pass_text: String = state
        .inputs
        .manual_password_input
        .value
        .chars()
        .map(|_| '•')
        .collect();
    let max_width_pass = (layout[1].width.saturating_sub(2)) as usize;
    let (display_pass, pass_cursor_x) = scrolled_input_view(
        &pass_text,
        state.inputs.manual_password_input.cursor,
        max_width_pass,
    );

    let pass_spans = input_line_spans(
        &display_pass,
        pass_cursor_x,
        state.inputs.manual_input_field == 1,
        false,
    );

    let pass_para = Paragraph::new(Line::from(pass_spans)).block(pass_block);
    frame.render_widget(pass_para, layout[1]);

    // Security Selector
    let is_active = state.inputs.manual_input_field == 2;
    let sec_border_style = if is_active {
        Style::default().fg(theme::YELLOW)
    } else {
        Style::default().fg(theme::FOREGROUND)
    };
    let sec_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Security ")
        .border_style(sec_border_style)
        .style(Style::default().bg(theme::BACKGROUND));

    let arrow_style = if is_active {
        Style::default().fg(theme::YELLOW)
    } else {
        Style::default().fg(theme::DIMMED)
    };

    let value_style = if is_active {
        Style::default()
            .fg(theme::FOREGROUND)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::FOREGROUND)
    };

    let sec_para = Paragraph::new(Line::from(vec![
        Span::styled(format!("{} ", icons.arrow_left()), arrow_style),
        Span::styled(format!(" {} ", state.inputs.manual_security), value_style),
        Span::styled(format!(" {}", icons.arrow_right()), arrow_style),
    ]))
    .block(sec_block)
    .alignment(Alignment::Center);
    frame.render_widget(sec_para, layout[2]);

    // Hidden Checkbox + Connect Button Row
    let bottom_layout =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(15)]).split(layout[4]);

    // Capture hidden checkbox and connect button areas for mouse hit-testing
    areas.manual_field_areas[3] = Some(bottom_layout[0]);
    areas.manual_connect_area = Some(bottom_layout[1]);

    // Hidden Checkbox
    let hidden_style = if state.inputs.manual_input_field == 3 {
        Style::default().fg(theme::YELLOW)
    } else {
        Style::default().fg(theme::FOREGROUND)
    };
    let hidden_text = format!(
        "{} Hidden Network",
        icons.checkbox(state.inputs.manual_hidden)
    );
    let hidden_para = Paragraph::new(hidden_text).style(hidden_style);
    frame.render_widget(hidden_para, bottom_layout[0]);

    // Connect Button
    let connect_btn = if state.inputs.manual_input_field == 4 {
        Paragraph::new(Line::from(vec![
            Span::styled(icons.btn_left(), Style::default().fg(theme::GREEN)),
            Span::styled(
                "Connect",
                Style::default().bg(theme::GREEN).fg(theme::BACKGROUND),
            ),
            Span::styled(
                format!("{} ", icons.btn_right()),
                Style::default().fg(theme::GREEN),
            ),
        ]))
    } else {
        Paragraph::new(" Connect  ").style(Style::default().fg(theme::GREEN))
    }
    .alignment(Alignment::Right);
    frame.render_widget(connect_btn, bottom_layout[1]);
}

fn render_key_logger(frame: &mut Frame, main_area: Rect, state: &AppState) {
    if !state.ui.show_key_logger {
        return;
    }
    let Some((key, time)) = &state.ui.last_key_press else {
        return;
    };
    if time.elapsed() >= std::time::Duration::from_secs(2) {
        return;
    }

    let key_text = format!(" {} ", key);
    let width = (key_text.len() as u16 + 2).min(main_area.width);

    // Position right below the bottom right of the main UI
    let key_area = Rect::new(
        main_area
            .x
            .saturating_add(main_area.width.saturating_sub(width)),
        main_area.y.saturating_add(main_area.height),
        width,
        3,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BRIGHT_PURPLE))
        .style(Style::default().bg(theme::BACKGROUND));

    let paragraph = Paragraph::new(key_text)
        .block(block)
        .style(
            Style::default()
                .fg(theme::BRIGHT_PURPLE)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);

    frame.render_widget(Clear, key_area);
    frame.render_widget(paragraph, key_area);
}

fn render_qr_popup(frame: &mut Frame, area: Rect, state: &AppState, areas: &mut LayoutAreas) {
    // Calculate QR popup size based on terminal size
    let qr_height = state.ui.qr_code_lines.len() as u16 + 4; // +4 for borders and padding
    let qr_width = state.ui.qr_code_lines.first().map(|l| l.len()).unwrap_or(0) as u16 + 4;

    // Center the popup
    let qr_x = area.width.saturating_sub(qr_width) / 2;
    let qr_y = area.height.saturating_sub(qr_height) / 2;

    let qr_area = Rect::new(
        qr_x,
        qr_y,
        qr_width.min(area.width),
        qr_height.min(area.height),
    );
    areas.qr_popup_area = Some(qr_area);

    // Clear background
    frame.render_widget(Clear, qr_area);

    // QR code block
    let qr_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::CYAN))
        .title(" Share WiFi (Scan with phone) ")
        .title_alignment(Alignment::Center)
        .title_style(
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme::BACKGROUND));

    frame.render_widget(qr_block.clone(), qr_area);

    // Render QR code lines inside the block
    let inner = qr_area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let qr_text = state.ui.qr_code_lines.join("\n");
    let qr_paragraph = Paragraph::new(qr_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme::FOREGROUND).bg(theme::BACKGROUND));

    frame.render_widget(qr_paragraph, inner);

    // Help text below QR code (clamp to terminal bounds)
    let help_y = qr_area.y.saturating_add(qr_area.height).saturating_add(1);
    if help_y < area.y.saturating_add(area.height) && area.width > 0 {
        let help_area = Rect::new(area.x, help_y, area.width, 1);
        let help_text = Paragraph::new("Press ESC, q, or Enter to close")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::DIMMED));
        frame.render_widget(help_text, help_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_chars_wrap_around() {
        assert_eq!(spinner_char(0), config::LOADING_CHARS[0]);
        let len = config::LOADING_CHARS.len();
        assert_eq!(spinner_char(len), config::LOADING_CHARS[0]);
        assert_eq!(spinner_char(len + 3), config::LOADING_CHARS[3]);
    }

    #[test]
    fn short_input_is_not_scrolled() {
        let (text, cursor) = scrolled_input_view("wifi", 2, 10);
        assert_eq!(text, "wifi");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn long_input_scrolls_to_keep_cursor_visible_at_end() {
        let text = "abcdefghijklmnop";
        let (view, cursor) = scrolled_input_view(text, 15, 8);
        assert_eq!(view, "ijklmnop");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn long_input_keeps_beginning_when_cursor_inside_viewport() {
        let text = "abcdefghijklmnop";
        let (view, cursor) = scrolled_input_view(text, 3, 8);
        assert_eq!(view, "abcdefgh");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn zero_width_viewport_renders_nothing() {
        let (view, cursor) = scrolled_input_view("abc", 1, 0);
        assert!(view.is_empty());
        assert_eq!(cursor, 0);
    }

    #[test]
    fn active_cursor_highlights_character_and_trailing_space() {
        let spans = input_line_spans("ab", 2, true, false);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "a");
        assert_eq!(spans[1].content.as_ref(), "b");
        assert_eq!(spans[2].content.as_ref(), " ");
        assert!(spans[2].style.bg.is_some());
    }

    #[test]
    fn cursor_mid_string_highlights_only_that_character() {
        let spans = input_line_spans("ab", 1, true, false);
        assert_eq!(spans.len(), 2);
        assert!(spans[0].style.bg.is_none());
        assert!(spans[1].style.bg.is_some());
    }

    #[test]
    fn inactive_cursor_renders_plain_text() {
        let spans = input_line_spans("ab", 1, false, false);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].style, Style::default());
        assert_eq!(spans[1].style, Style::default());
    }

    #[test]
    fn dimmed_mode_styles_all_characters() {
        let spans = input_line_spans("ab", 5, true, true);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].style.fg, Some(theme::DIMMED));
        assert_eq!(spans[1].style.fg, Some(theme::DIMMED));
    }
}
