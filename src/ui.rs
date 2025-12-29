use crate::app::AppState;
use crate::theme;
use ratatui::{
    prelude::*,
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();
    let is_dimmed = state.show_manual_add_popup || state.show_password_popup;

    // Set background color for the entire screen
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BACKGROUND).fg(theme::FOREGROUND)),
        area,
    );

    // Calculate dynamic dimensions to ensure perfect centering
    // Adjust width/height to match the parity of the terminal size
    let target_height = 32;
    let height = if area.height % 2 == 0 {
        if target_height % 2 == 0 {
            target_height
        } else {
            target_height + 1
        }
    } else {
        if target_height % 2 != 0 {
            target_height
        } else {
            target_height + 1
        }
    };

    let target_width = 77;
    let width = if area.width % 2 == 0 {
        if target_width % 2 == 0 {
            target_width
        } else {
            target_width + 1
        }
    } else {
        if target_width % 2 != 0 {
            target_width
        } else {
            target_width + 1
        }
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

    let main_area = horizontal_layout[1];

    let border_style = Style::default().fg(theme::DIMMED);

    let title_style = Style::default()
        .fg(theme::CYAN)
        .add_modifier(Modifier::BOLD);

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(format!(" WIFUI v{} ", env!("CARGO_PKG_VERSION")))
        .title_alignment(Alignment::Center)
        .title_style(title_style);

    frame.render_widget(main_block, main_area);

    let inner_area = main_area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });

    let mut constraints = vec![
        Constraint::Min(10),   // Network list
        Constraint::Length(9), // Details
        Constraint::Length(2), // Bottom bar
    ];

    if state.is_searching || !state.search_input.is_empty() {
        constraints.insert(0, Constraint::Length(3));
    }

    let content_layout = Layout::vertical(constraints).split(inner_area);

    let (search_area, list_area, details_area, help_area) =
        if state.is_searching || !state.search_input.is_empty() {
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
        let search_style = if is_dimmed {
            Style::default().fg(theme::DIMMED)
        } else if state.is_searching {
            Style::default().fg(theme::YELLOW)
        } else {
            Style::default().fg(theme::CYAN)
        };

        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Search (/) ")
            .border_style(search_style);

        let max_width = (area.width - 2) as usize;
        let input_len = state.search_input.chars().count();
        let cursor_pos = state.search_cursor;

        let (display_text, cursor_x) = if input_len < max_width {
            (state.search_input.clone(), cursor_pos)
        } else {
            // If cursor is near the end, show the end
            if cursor_pos >= max_width {
                let skip = cursor_pos - max_width + 1;
                let take = max_width;
                let text: String = state.search_input.chars().skip(skip).take(take).collect();
                (text, max_width - 1)
            } else {
                // If cursor is at the beginning, show the beginning
                let text: String = state.search_input.chars().take(max_width).collect();
                (text, cursor_pos)
            }
        };

        let mut spans = Vec::new();
        let chars: Vec<char> = display_text.chars().collect();

        for (i, c) in chars.iter().enumerate() {
            if i == cursor_x && state.is_searching && !is_dimmed {
                spans.push(Span::styled(
                    c.to_string(),
                    Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
                ));
            } else if is_dimmed {
                spans.push(Span::styled(
                    c.to_string(),
                    Style::default().fg(theme::DIMMED),
                ));
            } else {
                spans.push(Span::raw(c.to_string()));
            }
        }

        if cursor_x == chars.len() && state.is_searching && !is_dimmed {
            spans.push(Span::styled(
                " ",
                Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
            ));
        }

        let search_text = Paragraph::new(Line::from(spans)).block(search_block);

        frame.render_widget(search_text, area);
    }

    let list_items: Vec<ListItem> = state
        .filtered_wifi_list
        .iter()
        .map(|w| {
            let mut ssid = w.ssid.clone();
            let mut style = if is_dimmed {
                Style::default().fg(theme::DIMMED)
            } else {
                Style::default()
            };

            let prefix = if w.is_saved {
                if !is_dimmed {
                    style = style.fg(theme::BLUE);
                }
                "󰆓 " // nf-md-content_save
            } else if w.authentication == "Open" {
                " " // nf-fa-rss
            } else {
                " " // nf-fa-lock
            };

            ssid = format!("{}{}", prefix, ssid);

            if let Some(connected_ssid) = &state.connected_ssid
                && w.ssid == *connected_ssid
            {
                ssid = format!("{} 󰖩", ssid); // nf-md-wifi_check
                if is_dimmed {
                    style = style.fg(theme::DIMMED).add_modifier(Modifier::BOLD);
                } else {
                    style = style.fg(theme::GREEN).add_modifier(Modifier::BOLD);
                }
            }

            if w.is_saved {
                if w.auto_connect {
                    ssid = format!("{} 󰁪", ssid);
                } else {
                    ssid = format!("{} 󱧧", ssid);
                }
            }

            ListItem::new(ssid).style(style)
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

    let list = List::new(list_items)
        .block(
            Block::default()
                .title(" Networks ")
                .title_style(list_title_style)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(list_border_style),
        )
        .highlight_symbol("  ")
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(if is_dimmed {
                    theme::BACKGROUND
                } else {
                    theme::SELECTION_BG
                }),
        );

    frame.render_stateful_widget(list, list_area, &mut state.l_state);

    let viewport_height = list_area.height.saturating_sub(2) as usize;
    let content_len = state.filtered_wifi_list.len();

    let mut scroll_state = ScrollbarState::new(content_len)
        .position(state.l_state.selected().unwrap_or(0))
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

    if let Some(selected) = state.l_state.selected()
        && let Some(wifi) = state.filtered_wifi_list.get(selected)
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

        let mut info = vec![
            Line::from(vec![
                Span::styled("SSID: ", label_style),
                Span::styled(&wifi.ssid, value_style),
            ]),
            Line::from(vec![
                Span::styled("Signal: ", label_style),
                Span::styled(format!("{}% ", wifi.signal), value_style),
                // Add signal bar
                Span::styled(
                    "█".repeat((wifi.signal as usize / 10).min(10)),
                    if is_dimmed {
                        Style::default().fg(theme::DIMMED)
                    } else if wifi.signal > 70 {
                        Style::default().fg(theme::GREEN)
                    } else if wifi.signal > 40 {
                        Style::default().fg(theme::YELLOW)
                    } else {
                        Style::default().fg(theme::RED)
                    },
                ),
            ]),
            Line::from(vec![
                Span::styled("Security: ", label_style),
                Span::styled(
                    format!("{} / {}", wifi.authentication, wifi.encryption),
                    value_style,
                ),
            ]),
            Line::from(vec![
                Span::styled("Type: ", label_style),
                Span::styled(
                    format!("{} ({})", wifi.phy_type, wifi.network_type),
                    value_style,
                ),
            ]),
            Line::from(vec![
                Span::styled("Channel: ", label_style),
                Span::styled(
                    format!(
                        "{} ({:.1} GHz)",
                        wifi.channel,
                        wifi.frequency as f32 / 1_000_000.0
                    ),
                    value_style,
                ),
            ]),
        ];

        if wifi.is_saved {
            info.push(Line::from(vec![
                Span::styled("Auto-Connect: ", label_style),
                Span::styled(
                    if wifi.auto_connect {
                        "Yes 󰁪"
                    } else {
                        "No 󱧧"
                    },
                    value_style,
                ),
            ]));
        }

        if let Some(speed) = wifi.link_speed {
            info.push(Line::from(vec![
                Span::styled("Link Speed: ", label_style),
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

        let paragraph = Paragraph::new(info).block(
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

    let help_text = if state.show_password_popup {
        // Password input active - show password-specific shortcuts
        vec![Line::from(vec![
            Span::styled("󰌑", Style::default().fg(theme::FOREGROUND)),
            Span::styled(" connect • ", Style::default().fg(theme::DIMMED)),
            Span::styled("esc", Style::default().fg(theme::FOREGROUND)),
            Span::styled(" cancel", Style::default().fg(theme::DIMMED)),
        ])]
    } else if state.show_manual_add_popup {
        // Manual add popup active - show relevant navigation & actions
        vec![
            Line::from(vec![
                Span::styled("⇥ / ↓", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" next • ", Style::default().fg(theme::DIMMED)),
                Span::styled("⇤ / ↑", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" prev • ", Style::default().fg(theme::DIMMED)),
                Span::styled("󰌑", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" connect • ", Style::default().fg(theme::DIMMED)),
                Span::styled("esc", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" cancel", Style::default().fg(theme::DIMMED)),
            ]),
            Line::from(vec![
                Span::styled("󱁐", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" checkbox • ", Style::default().fg(theme::DIMMED)),
                Span::styled("h/l/j/k", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" dropdown", Style::default().fg(theme::DIMMED)),
            ]),
        ]
    } else if state.is_searching || !state.search_input.is_empty() {
        // Search active - show search-specific shortcuts
        vec![Line::from(vec![
            Span::styled("󰌑", Style::default().fg(theme::FOREGROUND)),
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
                Span::styled("󰌑", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" conn / dconn • ", Style::default().fg(theme::DIMMED)),
                Span::styled("f", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" forget • ", Style::default().fg(theme::DIMMED)),
                Span::styled("r", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" refresh", Style::default().fg(theme::DIMMED)),
            ]),
            Line::from(vec![
                Span::styled("a", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" auto-conn • ", Style::default().fg(theme::DIMMED)),
                Span::styled("n", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" add new • ", Style::default().fg(theme::DIMMED)),
                Span::styled("/", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" search • ", Style::default().fg(theme::DIMMED)),
                Span::styled("esc", Style::default().fg(theme::FOREGROUND)),
                Span::styled(" back/clear", Style::default().fg(theme::DIMMED)),
            ]),
        ]
    };
    let help_paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(theme::DIMMED))
        .alignment(Alignment::Center);

    frame.render_widget(help_paragraph, help_area);

    if state.is_connecting {
        let loading_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let loading_char = loading_chars[state.loading_frame % loading_chars.len()];

        let area = frame.area();
        let loading_area = Rect::new(area.width / 2 - 10, area.height / 2 - 1, 20, 3);

        let loading_paragraph = Paragraph::new(format!("{} Connecting...", loading_char))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme::YELLOW)),
            )
            .style(Style::default().fg(theme::FOREGROUND).bg(theme::BACKGROUND))
            .alignment(Alignment::Center);

        frame.render_widget(Clear, loading_area);
        frame.render_widget(loading_paragraph, loading_area);
    }

    if let Some(error) = &state.error_message {
        let error_area = Rect::new(area.x + 2, area.height - 4, area.width - 4, 3);
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

    if state.show_password_popup {
        let networks_area = list_area;
        let popup_height = 3;
        let popup_area = Rect {
            x: networks_area.x,
            y: networks_area.y + networks_area.height.saturating_sub(popup_height),
            width: networks_area.width,
            height: popup_height,
        };

        let popup_text = state
            .password_input
            .chars()
            .map(|_| '•')
            .collect::<String>();

        let max_width = (popup_area.width - 4) as usize;
        let input_len = popup_text.chars().count();
        let cursor_pos = state.password_cursor;

        let (display_text, cursor_x) = if input_len < max_width {
            (popup_text, cursor_pos)
        } else {
            if cursor_pos >= max_width {
                let skip = cursor_pos - max_width + 1;
                let take = max_width;
                let text: String = popup_text.chars().skip(skip).take(take).collect();
                (text, max_width - 1)
            } else {
                let text: String = popup_text.chars().take(max_width).collect();
                (text, cursor_pos)
            }
        };

        let mut spans = Vec::new();
        let chars: Vec<char> = display_text.chars().collect();

        for (i, c) in chars.iter().enumerate() {
            if i == cursor_x {
                spans.push(Span::styled(
                    c.to_string(),
                    Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
                ));
            } else {
                spans.push(Span::raw(c.to_string()));
            }
        }

        if cursor_x == chars.len() {
            spans.push(Span::styled(
                " ",
                Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
            ));
        }

        let popup_block = Block::default()
            .title(format!(
                " Password for {} ",
                state.connecting_to_ssid.as_deref().unwrap_or("")
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

    if state.show_manual_add_popup {
        let networks_area = list_area;
        let popup_height = 13;
        let popup_area = Rect {
            x: networks_area.x,
            y: networks_area.y + networks_area.height.saturating_sub(popup_height),
            width: networks_area.width,
            height: popup_height,
        };

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

        // SSID Input
        let ssid_style = if state.manual_input_field == 0 {
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

        // SSID Cursor Logic
        let max_width_ssid = (layout[0].width - 2) as usize;
        let ssid_text = &state.manual_ssid_input;
        let ssid_len = ssid_text.chars().count();
        let ssid_cursor = state.manual_ssid_cursor;

        let (display_ssid, ssid_cursor_x) = if ssid_len < max_width_ssid {
            (ssid_text.clone(), ssid_cursor)
        } else {
            if ssid_cursor >= max_width_ssid {
                let skip = ssid_cursor - max_width_ssid + 1;
                let take = max_width_ssid;
                let text: String = ssid_text.chars().skip(skip).take(take).collect();
                (text, max_width_ssid - 1)
            } else {
                let text: String = ssid_text.chars().take(max_width_ssid).collect();
                (text, ssid_cursor)
            }
        };

        let mut ssid_spans = Vec::new();
        let ssid_chars: Vec<char> = display_ssid.chars().collect();
        for (i, c) in ssid_chars.iter().enumerate() {
            if i == ssid_cursor_x && state.manual_input_field == 0 {
                ssid_spans.push(Span::styled(
                    c.to_string(),
                    Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
                ));
            } else {
                ssid_spans.push(Span::raw(c.to_string()));
            }
        }
        if ssid_cursor_x == ssid_chars.len() && state.manual_input_field == 0 {
            ssid_spans.push(Span::styled(
                " ",
                Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
            ));
        }

        let ssid_para = Paragraph::new(Line::from(ssid_spans)).block(ssid_block);
        frame.render_widget(ssid_para, layout[0]);

        // Password Input
        let pass_style = if state.manual_input_field == 1 {
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

        // Password Cursor Logic
        let max_width_pass = (layout[1].width - 2) as usize;
        let pass_text: String = state.manual_password_input.chars().map(|_| '•').collect();
        let pass_len = pass_text.chars().count();
        let pass_cursor = state.manual_password_cursor;

        let (display_pass, pass_cursor_x) = if pass_len < max_width_pass {
            (pass_text, pass_cursor)
        } else {
            if pass_cursor >= max_width_pass {
                let skip = pass_cursor - max_width_pass + 1;
                let take = max_width_pass;
                let text: String = pass_text.chars().skip(skip).take(take).collect();
                (text, max_width_pass - 1)
            } else {
                let text: String = pass_text.chars().take(max_width_pass).collect();
                (text, pass_cursor)
            }
        };

        let mut pass_spans = Vec::new();
        let pass_chars: Vec<char> = display_pass.chars().collect();
        for (i, c) in pass_chars.iter().enumerate() {
            if i == pass_cursor_x && state.manual_input_field == 1 {
                pass_spans.push(Span::styled(
                    c.to_string(),
                    Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
                ));
            } else {
                pass_spans.push(Span::raw(c.to_string()));
            }
        }
        if pass_cursor_x == pass_chars.len() && state.manual_input_field == 1 {
            pass_spans.push(Span::styled(
                " ",
                Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND),
            ));
        }

        let pass_para = Paragraph::new(Line::from(pass_spans)).block(pass_block);
        frame.render_widget(pass_para, layout[1]);

        // Security Selector
        let sec_style = if state.manual_input_field == 2 {
            Style::default().fg(theme::YELLOW)
        } else {
            Style::default().fg(theme::FOREGROUND)
        };
        let sec_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Security ")
            .border_style(sec_style)
            .style(Style::default().bg(theme::BACKGROUND));
        let sec_para = Paragraph::new(format!(" < {} > ", state.manual_security))
            .block(sec_block)
            .alignment(Alignment::Center);
        frame.render_widget(sec_para, layout[2]);

        // Hidden Checkbox + Connect Button Row
        let bottom_layout =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(15)]).split(layout[4]);

        // Hidden Checkbox
        let hidden_style = if state.manual_input_field == 3 {
            Style::default().fg(theme::YELLOW)
        } else {
            Style::default().fg(theme::FOREGROUND)
        };
        let hidden_text = if state.manual_hidden {
            "  Hidden Network"
        } else {
            "  Hidden Network"
        };
        let hidden_para = Paragraph::new(hidden_text).style(hidden_style);
        frame.render_widget(hidden_para, bottom_layout[0]);

        // Connect Button
        let connect_btn = if state.manual_input_field == 4 {
            Paragraph::new(Line::from(vec![
                Span::styled("", Style::default().fg(theme::GREEN)),
                Span::styled(
                    "Connect",
                    Style::default().bg(theme::GREEN).fg(theme::BACKGROUND),
                ),
                Span::styled("", Style::default().fg(theme::GREEN)),
            ]))
        } else {
            Paragraph::new(" Connect ").style(Style::default().fg(theme::GREEN))
        }
        .alignment(Alignment::Right);
        frame.render_widget(connect_btn, bottom_layout[1]);
    }

    if state.show_key_logger {
        if let Some((key, time)) = &state.last_key_press {
            if time.elapsed() < std::time::Duration::from_secs(2) {
                let key_text = format!(" {} ", key);
                let width = key_text.len() as u16 + 2;

                // Position right below the bottom right of the main UI
                let key_area = Rect::new(
                    main_area.x + main_area.width - width,
                    main_area.y + main_area.height,
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
        }
    }
}
