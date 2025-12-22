use crate::app::AppState;
use crate::theme;
use ratatui::
{
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap},
};

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();

    // Set background color for the entire screen
    frame.render_widget(Block::default().style(Style::default().bg(theme::BACKGROUND).fg(theme::FOREGROUND)), area);

    // Center the main window
    let vertical_layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(32), // Fixed height for the main window
        Constraint::Fill(1),
    ])
    .split(area);

    let horizontal_layout = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(77),
        Constraint::Fill(1),
    ])
    .split(vertical_layout[1]);

    let main_area = horizontal_layout[1];

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::CYAN))
        .title(" WIFUI ")
        .title_alignment(Alignment::Center)
        .title_style(Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD));

    frame.render_widget(main_block, main_area);

    let inner_area = main_area.inner(Margin { vertical: 1, horizontal: 2 });

    let mut constraints = vec![
        Constraint::Min(10),   // Network list
        Constraint::Length(9), // Details
        Constraint::Length(2), // Bottom bar
    ];

    if state.is_searching || !state.search_input.is_empty() {
        constraints.insert(0, Constraint::Length(3));
    }

    let content_layout = Layout::vertical(constraints).split(inner_area);

    let (search_area, list_area, details_area, help_area) = if state.is_searching || !state.search_input.is_empty() {
        (Some(content_layout[0]), content_layout[1], content_layout[2], content_layout[3])
    } else {
        (None, content_layout[0], content_layout[1], content_layout[2])
    };

    if let Some(area) = search_area {
        let search_style = if state.is_searching {
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
            if i == cursor_x && state.is_searching {
                spans.push(Span::styled(c.to_string(), Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND)));
            } else {
                spans.push(Span::raw(c.to_string()));
            }
        }

        if cursor_x == chars.len() && state.is_searching {
             spans.push(Span::styled(" ", Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND)));
        }

        let search_text = Paragraph::new(Line::from(spans))
            .block(search_block);

        frame.render_widget(search_text, area);
    }

    let list_items: Vec<ListItem> = state
        .filtered_wifi_list
        .iter()
        .map(|w| {
            let mut ssid = w.ssid.clone();
            let mut style = Style::default();

            if let Some(connected_ssid) = &state.connected_ssid
                && w.ssid == *connected_ssid {
                    ssid = format!("{} 󰖩", ssid); // nf-md-wifi_check
                    style = style.fg(theme::GREEN).add_modifier(Modifier::BOLD);
                }

            if w.is_saved {
                ssid = format!("{} 󰆓", ssid); // nf-md-content_save
                if w.auto_connect {
                    ssid = format!("{} 󰁪", ssid);
                } else {
                    ssid = format!("{} 󱧧", ssid);
                }
            }

            ListItem::new(ssid).style(style)
        })
        .collect();

    let list = List::new(list_items)
        .block(
            Block::default()
                .title(" Networks ")
                .title_style(Style::default().fg(theme::BLUE).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::BLUE)),
        )
        .highlight_symbol("  ")
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(theme::SELECTION_BG),
        );

    frame.render_stateful_widget(list, list_area, &mut state.l_state);

    if let Some(selected) = state.l_state.selected()
        && let Some(wifi) = state.filtered_wifi_list.get(selected) {
            let mut info = vec![
                Line::from(vec![
                    Span::styled("SSID: ", Style::default().fg(theme::CYAN)),
                    Span::raw(&wifi.ssid),
                ]),
                Line::from(vec![
                    Span::styled("Signal: ", Style::default().fg(theme::CYAN)),
                    Span::raw(format!("{}% ", wifi.signal)),
                    // Add signal bar
                    Span::styled(
                        "█".repeat((wifi.signal as usize / 10).min(10)),
                        if wifi.signal > 70 {
                            Style::default().fg(theme::GREEN)
                        } else if wifi.signal > 40 {
                            Style::default().fg(theme::YELLOW)
                        } else {
                            Style::default().fg(theme::RED)
                        },
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Security: ", Style::default().fg(theme::CYAN)),
                    Span::raw(format!("{} / {}", wifi.authentication, wifi.encryption)),
                ]),
                Line::from(vec![
                    Span::styled("Type: ", Style::default().fg(theme::CYAN)),
                    Span::raw(format!("{} ({})", wifi.phy_type, wifi.network_type)),
                ]),
                Line::from(vec![
                    Span::styled("Channel: ", Style::default().fg(theme::CYAN)),
                    Span::raw(format!("{} ({:.1} GHz)", wifi.channel, wifi.frequency as f32 / 1_000_000.0)),
                ]),
            ];

            if wifi.is_saved {
                info.push(Line::from(vec![
                    Span::styled("Auto-Connect: ", Style::default().fg(theme::CYAN)),
                    Span::raw(if wifi.auto_connect { "Yes 󰁪" } else { "No 󱧧" }),
                ]));
            }

            if let Some(speed) = wifi.link_speed {
                info.push(Line::from(vec![
                    Span::styled("Link Speed: ", Style::default().fg(theme::CYAN)),
                    Span::raw(format!("{} Mbps", speed)),
                ]));
            }

            let paragraph = Paragraph::new(info)
                .block(
                    Block::default()
                        .title(" Details ")
                        .title_style(Style::default().fg(theme::PURPLE).add_modifier(Modifier::BOLD))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(theme::PURPLE))
                        .padding(Padding::new(1, 1, 0, 0)),
                );
            frame.render_widget(paragraph, details_area);
        }

    let help_text = "q: quit | j/k: nav | enter: connect | f: forget | r: refresh\na: auto-conn | /: search | esc: back/clear";
    let help_paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(theme::BRIGHT_BLACK))
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
                spans.push(Span::styled(c.to_string(), Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND)));
            } else {
                spans.push(Span::raw(c.to_string()));
            }
        }

        if cursor_x == chars.len() {
             spans.push(Span::styled(" ", Style::default().bg(theme::FOREGROUND).fg(theme::BACKGROUND)));
        }

        let popup_block = Block::default()
            .title(format!(" Password for {} ", state.connecting_to_ssid.as_deref().unwrap_or("")))
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
}
