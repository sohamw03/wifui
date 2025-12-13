use crate::app::AppState;
use ratatui::
{
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap},
};

pub fn render(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();

    // Center the main window
    let vertical_layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(30), // Fixed height for the main window
        Constraint::Fill(1),
    ])
    .split(area);

    let horizontal_layout = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(81),
        Constraint::Fill(1),
    ])
    .split(vertical_layout[1]);

    let main_area = horizontal_layout[1];

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" WIFUI ")
        .title_alignment(Alignment::Center)
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    frame.render_widget(main_block, main_area);

    let inner_area = main_area.inner(Margin { vertical: 1, horizontal: 2 });

    let content_layout = Layout::vertical([
        Constraint::Min(10),   // Network list
        Constraint::Length(10), // Details
        Constraint::Length(1), // Bottom bar
    ])
    .split(inner_area);

    let list_items: Vec<ListItem> = state
        .wifi_list
        .iter()
        .map(|w| {
            let mut ssid = w.ssid.clone();
            let mut style = Style::default();

            if let Some(connected_ssid) = &state.connected_ssid {
                if w.ssid == *connected_ssid {
                    ssid = format!("{} 󰖩", ssid); // nf-md-wifi_check
                    style = style.fg(Color::Green).add_modifier(Modifier::BOLD);
                }
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
                .title_style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .highlight_symbol(" > ")
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        );

    frame.render_stateful_widget(list, content_layout[0], &mut state.l_state);

    if let Some(selected) = state.l_state.selected() {
        if let Some(wifi) = state.wifi_list.get(selected) {
            let mut info = vec![
                Line::from(vec![
                    Span::styled("SSID: ", Style::default().fg(Color::Cyan)),
                    Span::raw(&wifi.ssid),
                ]),
                Line::from(vec![
                    Span::styled("Signal: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}% ", wifi.signal)),
                    // Add signal bar
                    Span::styled(
                        "█".repeat((wifi.signal as usize / 10).min(10)),
                        if wifi.signal > 70 {
                            Style::default().fg(Color::Green)
                        } else if wifi.signal > 40 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::Red)
                        },
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Security: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{} / {}", wifi.authentication, wifi.encryption)),
                ]),
                Line::from(vec![
                    Span::styled("Type: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{} ({})", wifi.phy_type, wifi.network_type)),
                ]),
                Line::from(vec![
                    Span::styled("Channel: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{} ({:.1} GHz)", wifi.channel, wifi.frequency as f32 / 1_000_000.0)),
                ]),
            ];

            if wifi.is_saved {
                info.push(Line::from(vec![
                    Span::styled("Auto-Connect: ", Style::default().fg(Color::Cyan)),
                    Span::raw(if wifi.auto_connect { "Yes 󰁪" } else { "No 󱧧" }),
                ]));
            }

            if let Some(speed) = wifi.link_speed {
                info.push(Line::from(vec![
                    Span::styled("Link Speed: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{} Mbps", speed)),
                ]));
            }

            let paragraph = Paragraph::new(info)
                .block(
                    Block::default()
                        .title(" Details ")
                        .title_style(Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Magenta))
                        .padding(Padding::new(1, 1, 1, 1)),
                );
            frame.render_widget(paragraph, content_layout[1]);
        }
    }

    let help_text = "q: quit | j/k: nav | enter: connect | f: forget | r: refresh | a: auto-conn";
    let help_paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

    frame.render_widget(help_paragraph, content_layout[2]);

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
                    .border_style(Style::default().fg(Color::Yellow)),
            )
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
                    .border_style(Style::default().fg(Color::Red))
                    .title(" ERROR "),
            )
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: true });
        frame.render_widget(Clear, error_area);
        frame.render_widget(error_paragraph, error_area);
    }

    if state.show_password_popup {
        let networks_area = content_layout[0];
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

        let popup_block = Block::default()
            .title(format!(" Password for {} ", state.connecting_to_ssid.as_deref().unwrap_or("")))
            .title_alignment(Alignment::Left)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow))
            .padding(Padding::new(1, 1, 0, 0)); // Add padding to center vertically

        let popup = Paragraph::new(popup_text)
            .block(popup_block)
            .alignment(Alignment::Left);

        frame.render_widget(Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }
}
