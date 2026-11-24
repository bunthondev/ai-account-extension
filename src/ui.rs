use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{AddAccountField, App, FocusedPane, ModalState};
use crate::models::{ProviderKind, Role};

pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Main layout: Header (3), Main Body (Min 0), Status Bar (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(size);

    draw_header(frame, app, chunks[0]);
    draw_main_body(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);

    // Draw modals if open
    match app.modal {
        ModalState::WebSignIn { port } => draw_web_signin_modal(frame, port),
        ModalState::AddAccount(field) => draw_add_account_modal(frame, app, field),
        ModalState::ConfirmDelete => draw_confirm_delete_modal(frame, app),
        ModalState::Help => draw_help_modal(frame),
        ModalState::None => {}
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let active_desc = if let Some(a) = app.config.active_account() {
        format!(
            " Active: {} | Model: {} [{}] ",
            a.name,
            a.model,
            a.provider.short_badge()
        )
    } else {
        " No Active Account ".to_string()
    };

    let title_line = Line::from(vec![
        Span::styled(" ◈ SUBAGENT TERMINAL ", Style::default().fg(Color::Cyan).bold()),
        Span::styled("• Multi-Provider LLM Client ", Style::default().fg(Color::DarkGray)),
    ]);

    let right_line = Line::from(vec![
        Span::styled(active_desc, Style::default().fg(Color::Green).bold()),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let header_p = Paragraph::new(title_line)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(header_p, area);

    // Right-aligned status in header
    let right_p = Paragraph::new(right_line).alignment(Alignment::Right);
    let right_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: 1,
    };
    frame.render_widget(right_p, right_area);
}

fn draw_main_body(frame: &mut Frame, app: &App, area: Rect) {
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(34), // Left sidebar
            Constraint::Min(40),    // Right chat & input
        ])
        .split(area);

    draw_sidebar(frame, app, body_chunks[0]);
    draw_chat_area(frame, app, body_chunks[1]);
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::Sidebar;
    let border_color = if is_focused { Color::Yellow } else { Color::DarkGray };

    let sidebar_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),    // Accounts list
            Constraint::Length(8), // Accounts quick actions
        ])
        .split(area);

    // Accounts list
    let accounts: Vec<ListItem> = if app.config.accounts.is_empty() {
        vec![
            ListItem::new(Line::from(vec![
                Span::styled("No accounts added yet.", Style::default().fg(Color::DarkGray)),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("Press [Ctrl+W] Web Sign-in", Style::default().fg(Color::Yellow)),
            ])),
            ListItem::new(Line::from(vec![
                Span::styled("Press [Ctrl+A] Manual Add", Style::default().fg(Color::Cyan)),
            ])),
        ]
    } else {
        app.config
            .accounts
            .iter()
            .enumerate()
            .map(|(idx, acc)| {
                let is_active = app.config.active_account_id.as_deref() == Some(&acc.id);
                let is_selected = idx == app.selected_account_idx && is_focused;

                let indicator = if is_active { "●" } else { "○" };
                let ind_color = if is_active { Color::Green } else { Color::DarkGray };

                let prov_color = match acc.provider {
                    ProviderKind::GoogleGemini => Color::Blue,
                    ProviderKind::AnthropicClaude => Color::LightRed,
                    ProviderKind::OpenAiCodex => Color::LightGreen,
                };

                let prefix = if is_selected { "> " } else { "  " };

                let style = if is_selected {
                    Style::default().bg(Color::Rgb(30, 30, 45)).bold()
                } else {
                    Style::default()
                };

                ListItem::new(vec![
                    Line::from(vec![
                        Span::raw(prefix),
                        Span::styled(format!("{} ", indicator), Style::default().fg(ind_color).bold()),
                        Span::styled(&acc.name, Style::default().bold()),
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            format!("[{}] ", acc.provider.short_badge()),
                            Style::default().fg(prov_color).bold(),
                        ),
                        Span::styled(&acc.model, Style::default().fg(Color::DarkGray)),
                        Span::styled(format!(" ({})", acc.masked_key()), Style::default().fg(Color::DarkGray)),
                    ]),
                ])
                .style(style)
            })
            .collect()
    };

    let title = format!(" Accounts ({}) ", app.config.accounts.len());
    let list_widget = List::new(accounts).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(title),
    );

    frame.render_widget(list_widget, sidebar_chunks[0]);

    // Quick actions box
    let actions_text = vec![
        Line::from(vec![
            Span::styled("Ctrl+W", Style::default().fg(Color::Yellow).bold()),
            Span::raw(" Web Sign-In"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+A", Style::default().fg(Color::Cyan).bold()),
            Span::raw(" Add account"),
        ]),
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(" Switch to active"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+D", Style::default().fg(Color::Red).bold()),
            Span::raw(" Delete account"),
        ]),
        Line::from(vec![
            Span::styled("Tab   ", Style::default().fg(Color::DarkGray).bold()),
            Span::raw(" Switch to Chat"),
        ]),
    ];

    let actions_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Controls ");

    let actions_p = Paragraph::new(actions_text).block(actions_block);
    frame.render_widget(actions_p, sidebar_chunks[1]);
}

fn draw_chat_area(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::ChatInput;
    let input_border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

    let chat_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),    // History viewport
            Constraint::Length(3), // Input box
        ])
        .split(area);

    // Chat history
    let mut lines = Vec::new();

    for msg in &app.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(vec![
                    Span::styled("You: ", Style::default().fg(Color::Yellow).bold()),
                    Span::raw(&msg.content),
                ]));
                lines.push(Line::raw(""));
            }
            Role::Assistant => {
                let badge = msg.provider.map(|p| p.short_badge()).unwrap_or("AI");
                let label = msg.account_label.as_deref().unwrap_or("Assistant");
                let color = match msg.provider {
                    Some(ProviderKind::GoogleGemini) => Color::Blue,
                    Some(ProviderKind::AnthropicClaude) => Color::LightRed,
                    Some(ProviderKind::OpenAiCodex) => Color::LightGreen,
                    None => Color::Cyan,
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("[{}] {}: ", badge, label), Style::default().fg(color).bold()),
                ]));

                for l in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::raw(l),
                    ]));
                }
                lines.push(Line::raw(""));
            }
            Role::System => {
                lines.push(Line::from(vec![
                    Span::styled("⚙ System: ", Style::default().fg(Color::Magenta).italic()),
                    Span::styled(&msg.content, Style::default().fg(Color::DarkGray)),
                ]));
                lines.push(Line::raw(""));
            }
        }
    }

    // If currently streaming
    if app.is_streaming {
        let active = app.config.active_account();
        let badge = active.map(|a| a.provider.short_badge()).unwrap_or("AI");
        let name = active.map(|a| a.name.as_str()).unwrap_or("Assistant");

        lines.push(Line::from(vec![
            Span::styled(format!("[{}] {}: ", badge, name), Style::default().fg(Color::Cyan).bold()),
            Span::styled("⚡ streaming...", Style::default().fg(Color::Yellow).italic()),
        ]));

        for l in app.current_stream_buffer.lines() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(l),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("  ▋", Style::default().fg(Color::Cyan)),
        ]));
        lines.push(Line::raw(""));
    }

    let history_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Conversation ");

    let total_lines = lines.len() as u16;
    let window_height = chat_chunks[0].height.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(window_height);

    let scroll_offset = if app.chat_scroll > 0 {
        max_scroll.saturating_sub(app.chat_scroll)
    } else {
        max_scroll
    };

    let chat_p = Paragraph::new(lines)
        .block(history_block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(chat_p, chat_chunks[0]);

    // Input box
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(input_border_color))
        .title(" Message (Enter to send, Tab to sidebar) ");

    let input_p = Paragraph::new(app.input.as_str()).block(input_block);
    frame.render_widget(input_p, chat_chunks[1]);

    if is_focused && app.modal == ModalState::None {
        frame.set_cursor_position((
            chat_chunks[1].x + app.input_cursor as u16 + 1,
            chat_chunks[1].y + 1,
        ));
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let status_text = if let Some(status) = app.get_status() {
        Span::styled(format!(" ℹ {} ", status), Style::default().fg(Color::Black).bg(Color::Yellow))
    } else {
        Span::styled(" Ready ", Style::default().fg(Color::DarkGray))
    };

    let hotkey_line = Line::from(vec![
        status_text,
        Span::raw(" | "),
        Span::styled("[Ctrl+W]", Style::default().fg(Color::Yellow).bold()),
        Span::raw(" Web Sign-in | "),
        Span::styled("[Tab]", Style::default().fg(Color::Cyan)),
        Span::raw(" Focus | "),
        Span::styled("[Ctrl+A]", Style::default().fg(Color::Cyan)),
        Span::raw(" Add | "),
        Span::styled("[Ctrl+S]", Style::default().fg(Color::Cyan)),
        Span::raw(" Switch | "),
        Span::styled("[Ctrl+L]", Style::default().fg(Color::Cyan)),
        Span::raw(" Clear | "),
        Span::styled("[Ctrl+H]", Style::default().fg(Color::Cyan)),
        Span::raw(" Help | "),
        Span::styled("[Ctrl+C]", Style::default().fg(Color::Red)),
        Span::raw(" Quit"),
    ]);

    let footer_p = Paragraph::new(hotkey_line).alignment(Alignment::Left);
    frame.render_widget(footer_p, area);
}

fn draw_web_signin_modal(frame: &mut Frame, port: u16) {
    let area = centered_rect(65, 45, frame.area());
    frame.render_widget(Clear, area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" 🌐 Web Browser Sign-In Active ");

    let url = format!("http://127.0.0.1:{}", port);

    let text = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled(" Local Authentication Portal is live at: ", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::raw("   ▶ "),
            Span::styled(url, Style::default().fg(Color::Cyan).bold()),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" 1. ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Your default browser has been launched automatically."),
        ]),
        Line::from(vec![
            Span::styled(" 2. ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Select provider (Google Gemini, Anthropic Claude, OpenAI)."),
        ]),
        Line::from(vec![
            Span::styled(" 3. ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("Fill in credentials and click 'Connect to Terminal'."),
        ]),
        Line::from(vec![
            Span::styled(" 4. ", Style::default().fg(Color::Yellow).bold()),
            Span::raw("This terminal will instantly link the account and activate it!"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("[O] Re-open Browser", Style::default().fg(Color::Cyan).bold()),
            Span::raw("    "),
            Span::styled("[Esc] Cancel Web Sign-in", Style::default().fg(Color::Red).bold()),
        ]),
    ];

    let p = Paragraph::new(text).block(modal_block).alignment(Alignment::Left);
    frame.render_widget(p, area);
}

fn draw_add_account_modal(frame: &mut Frame, app: &App, active_field: AddAccountField) {
    let area = centered_rect(65, 58, frame.area());
    frame.render_widget(Clear, area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" ✦ Add New AI Account ✦ ");

    frame.render_widget(modal_block.clone(), area);

    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });

    let fields_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Provider
            Constraint::Length(3), // Name / Label
            Constraint::Length(3), // API Key
            Constraint::Length(3), // Model
            Constraint::Length(2), // Error message or hints
            Constraint::Length(2), // Footer hints
        ])
        .split(inner);

    // 1. Provider
    let prov = app.add_form.current_provider();
    let prov_color = match prov {
        ProviderKind::GoogleGemini => Color::Blue,
        ProviderKind::AnthropicClaude => Color::LightRed,
        ProviderKind::OpenAiCodex => Color::LightGreen,
    };

    let prov_style = if active_field == AddAccountField::Provider {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let prov_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(prov_style)
        .title(" Provider (← / → to switch) ");

    let prov_line = Line::from(vec![
        Span::styled(format!(" ◀  {}  ▶ ", prov.display_name()), Style::default().fg(prov_color).bold()),
        Span::styled(format!(" [{}]", prov.short_badge()), Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(prov_line).block(prov_block), fields_chunks[0]);

    // 2. Name
    let name_style = if active_field == AddAccountField::Name {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let name_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(name_style)
        .title(" Account Name / Label (e.g. Work Gemini, Personal Claude) ");

    let name_p = Paragraph::new(format!(" {}", app.add_form.name)).block(name_block);
    frame.render_widget(name_p, fields_chunks[1]);

    // 3. API Key
    let key_style = if active_field == AddAccountField::ApiKey {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let masked_key = if app.add_form.api_key.is_empty() {
        String::new()
    } else {
        "•".repeat(app.add_form.api_key.len())
    };

    let key_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(key_style)
        .title(" API Key (Hidden) ");

    let key_p = Paragraph::new(format!(" {}", masked_key)).block(key_block);
    frame.render_widget(key_p, fields_chunks[2]);

    // 4. Model
    let model_style = if active_field == AddAccountField::Model {
        Style::default().fg(Color::Yellow).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let model_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(model_style)
        .title(" Model ID (Default prefilled, editable) ");

    let model_p = Paragraph::new(format!(" {}", app.add_form.model)).block(model_block);
    frame.render_widget(model_p, fields_chunks[3]);

    // Error or guide
    if let Some(err) = &app.add_form.error_msg {
        let err_p = Paragraph::new(Span::styled(format!("⚠ {}", err), Style::default().fg(Color::Red).bold()));
        frame.render_widget(err_p, fields_chunks[4]);
    } else {
        let hint = Line::from(vec![
            Span::styled("Tip: Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("[W / Ctrl+W]", Style::default().fg(Color::Yellow).bold()),
            Span::styled(" to open Browser Sign-In portal instead!", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(hint), fields_chunks[4]);
    }

    // Modal navigation guide
    let nav_line = Line::from(vec![
        Span::styled("[Tab]", Style::default().fg(Color::Yellow).bold()),
        Span::raw(" Next Field | "),
        Span::styled("[Enter]", Style::default().fg(Color::Green).bold()),
        Span::raw(" Save & Activate | "),
        Span::styled("[W]", Style::default().fg(Color::Yellow).bold()),
        Span::raw(" Web Sign-In | "),
        Span::styled("[Esc]", Style::default().fg(Color::Red).bold()),
        Span::raw(" Cancel"),
    ]);
    frame.render_widget(Paragraph::new(nav_line).alignment(Alignment::Center), fields_chunks[5]);
}

fn draw_confirm_delete_modal(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 25, frame.area());
    frame.render_widget(Clear, area);

    let target = app.config.accounts.get(app.selected_account_idx);
    let name = target.map(|a| a.name.as_str()).unwrap_or("Unknown");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red))
        .title(" Confirm Deletion ");

    let text = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw("Are you sure you want to delete account: "),
            Span::styled(name, Style::default().fg(Color::Yellow).bold()),
            Span::raw("?"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("[Y / Enter] Confirm Delete", Style::default().fg(Color::Red).bold()),
            Span::raw("   "),
            Span::styled("[N / Esc] Cancel", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let p = Paragraph::new(text).block(block).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

fn draw_help_modal(frame: &mut Frame) {
    let area = centered_rect(65, 65, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help & Keybindings ");

    let lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled("Ctrl + W", Style::default().fg(Color::Yellow).bold()),
            Span::raw("     Sign in from Web Browser (starts local web portal)"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl + A", Style::default().fg(Color::Yellow).bold()),
            Span::raw("     Manual account entry modal (Gemini, Claude, OpenAI)"),
        ]),
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(Color::Yellow).bold()),
            Span::raw("          Switch focus between Chat and Sidebar"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl + S", Style::default().fg(Color::Yellow).bold()),
            Span::raw("     Switch active account to highlighted item"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl + D", Style::default().fg(Color::Yellow).bold()),
            Span::raw("     Delete selected account"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl + L", Style::default().fg(Color::Yellow).bold()),
            Span::raw("     Clear conversation history"),
        ]),
        Line::from(vec![
            Span::styled("PageUp/Down", Style::default().fg(Color::Yellow).bold()),
            Span::raw("  Scroll chat conversation"),
        ]),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow).bold()),
            Span::raw("        Send message (in Chat) or Activate (in Sidebar)"),
        ]),
        Line::from(vec![
            Span::styled("Ctrl + C", Style::default().fg(Color::Yellow).bold()),
            Span::raw("     Exit application"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Press [Esc] or [Enter] to close", Style::default().fg(Color::DarkGray).italic()),
        ]),
    ];

    let p = Paragraph::new(lines).block(block).alignment(Alignment::Left);
    frame.render_widget(p, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
