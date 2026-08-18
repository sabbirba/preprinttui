use chrono::{DateTime, Local};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Tabs},
};

use crate::app::{App, AuthField, Tab};

pub fn fmt_bytes(b: u64) -> String {
    if b < 1024 {
        format!("{b} B")
    } else if b < 1048576 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{:.2} MB", b as f64 / 1048576.0)
    }
}

pub fn fmt_dur(s: u64) -> String {
    let d = s / 86400;
    let h = (s % 86400) / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if d > 0 {
        format!("{d}d {h}h {m}m {sec}s")
    } else if h > 0 {
        format!("{h}h {m}m {sec}s")
    } else if m > 0 {
        format!("{m}m {sec}s")
    } else {
        format!("{sec}s")
    }
}

pub fn fmt_ts(ts: u64) -> String {
    if ts == 0 {
        return "-".to_string();
    }
    let ms = if ts < 10_000_000_000 { ts * 1000 } else { ts };
    DateTime::from_timestamp_millis(ms as i64)
        .map(|dt| {
            DateTime::<Local>::from(dt)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "-".to_string())
}

pub fn render_ui(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < 25 || area.height < 6 {
        let msg = Paragraph::new(format!(
            "Resize window ({x}x{y})",
            x = area.width,
            y = area.height
        ))
        .style(Style::default().fg(Color::White));
        frame.render_widget(msg, area);
        return;
    }

    let is_compact = area.height < 18;
    let header_height = if is_compact { 1 } else { 3 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(app, frame, chunks[0], is_compact);

    match app.tab {
        Tab::Workers => render_workers(app, frame, chunks[1]),
        Tab::History => render_history(app, frame, chunks[1]),
    }

    let footer = if app.auth_mode {
        Line::from(vec![
            Span::styled(
                " Tab/Down ",
                Style::default().fg(Color::Black).bg(Color::White),
            ),
            Span::styled(" Next ", Style::default().fg(Color::White)),
            Span::styled(
                " • Enter ",
                Style::default().fg(Color::Black).bg(Color::Green),
            ),
            Span::styled(" Save ", Style::default().fg(Color::White)),
            Span::styled(
                " • Esc ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" Cancel ", Style::default().fg(Color::White)),
        ])
    } else if app.search_mode {
        Line::from(vec![
            Span::styled(
                " Esc/Enter ",
                Style::default().fg(Color::Black).bg(Color::White),
            ),
            Span::styled(" Exit Search ", Style::default().fg(Color::White)),
        ])
    } else if area.width < 60 {
        Line::from(vec![
            Span::styled(" 1-2 ", Style::default().fg(Color::Black).bg(Color::White)),
            Span::styled(" Tab ", Style::default().fg(Color::White)),
            Span::styled(" • e ", Style::default().fg(Color::Black).bg(Color::White)),
            Span::styled(" Auth ", Style::default().fg(Color::White)),
            Span::styled(
                " • q ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" Quit ", Style::default().fg(Color::White)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " 1-2/Tab ",
                Style::default().fg(Color::Black).bg(Color::White),
            ),
            Span::styled(" View ", Style::default().fg(Color::White)),
            Span::styled(" • e ", Style::default().fg(Color::Black).bg(Color::White)),
            Span::styled(" Auth ", Style::default().fg(Color::White)),
            Span::styled(
                " • x ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" Clear ", Style::default().fg(Color::White)),
            Span::styled(
                " • j/k ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" Move ", Style::default().fg(Color::White)),
            Span::styled(
                " • / ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" Filter ", Style::default().fg(Color::White)),
            Span::styled(
                " • r ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" Refresh ", Style::default().fg(Color::White)),
            Span::styled(
                " • q ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" Quit ", Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled(&app.status_msg, Style::default().fg(Color::DarkGray)),
        ])
    };
    frame.render_widget(Paragraph::new(footer), chunks[2]);

    if app.auth_mode {
        render_auth_modal(app, frame, area);
    }
}

fn render_header(app: &App, frame: &mut Frame, area: Rect, is_compact: bool) {
    let stats = app.stats.as_ref();
    let is_online = stats
        .and_then(|s| s.status.as_deref())
        .map(|s| s.eq_ignore_ascii_case("online"))
        .unwrap_or(!app.workers.is_empty());

    let worker_count = stats
        .and_then(|s| s.active_printers)
        .unwrap_or(app.workers.len());
    let uptime_str = app
        .realtime_uptime()
        .map(fmt_dur)
        .unwrap_or_else(|| "-".to_string());
    let history_count = stats
        .and_then(|s| s.history_count)
        .unwrap_or(app.history.len());

    let full_t1 = Tab::Workers.label(worker_count);
    let full_t2 = Tab::History.label(history_count);
    let full_len = (full_t1.len() + full_t2.len() + 3 + 6) as u16;

    let short_t1 = Tab::Workers.short_label(worker_count);
    let short_t2 = Tab::History.short_label(history_count);
    let short_len = (short_t1.len() + short_t2.len() + 3 + 6) as u16;

    let (tab_titles, tabs_len) = if area.width >= full_len + 15 {
        (vec![Line::from(full_t1), Line::from(full_t2)], full_len)
    } else if area.width >= short_len + 10 {
        (vec![Line::from(short_t1), Line::from(short_t2)], short_len)
    } else {
        (
            vec![
                Line::from(format!("W({}) [1]", worker_count)),
                Line::from(format!("H({}) [2]", history_count)),
            ],
            (area.width / 2).max(1),
        )
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(tabs_len), Constraint::Min(10)])
        .split(area);

    let tabs_widget = Tabs::new(tab_titles)
        .select(app.tab as usize)
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" | ", Style::default().fg(Color::DarkGray)));

    if is_compact {
        frame.render_widget(tabs_widget, cols[0]);
    } else {
        frame.render_widget(
            tabs_widget.block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            cols[0],
        );
    }

    let metrics = vec![
        Span::styled(
            if is_online { " ONLINE " } else { " OFFLINE " },
            Style::default().fg(if is_online {
                Color::Green
            } else {
                Color::DarkGray
            }),
        ),
        Span::styled("• ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{worker_count} Workers "),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("• ", Style::default().fg(Color::DarkGray)),
        Span::styled("Uptime: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{uptime_str} "), Style::default().fg(Color::White)),
    ];

    let p_stats = Paragraph::new(Line::from(metrics));

    if is_compact {
        frame.render_widget(p_stats, cols[1]);
    } else {
        frame.render_widget(
            p_stats.block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            cols[1],
        );
    }
}

fn render_auth_modal(app: &App, frame: &mut Frame, area: Rect) {
    let modal_width = 56.min(area.width.saturating_sub(2));
    let modal_height = 9.min(area.height.saturating_sub(2));

    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    frame.render_widget(Clear, modal_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(modal_area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::White))
        .title(" [ Credentials ] ");
    frame.render_widget(modal_block, modal_area);

    let mask = |val: &str| {
        if app.mask_credentials {
            if val.is_empty() {
                String::new()
            } else {
                "•".repeat(val.len())
            }
        } else {
            val.to_string()
        }
    };

    let f1_active = app.auth_field == AuthField::WorkerKey;
    let f1_val = mask(&app.input_worker_key);
    let f1 = Paragraph::new(Line::from(vec![
        Span::styled(
            if f1_val.is_empty() { "<None>" } else { &f1_val },
            Style::default().fg(if f1_val.is_empty() {
                Color::DarkGray
            } else {
                Color::White
            }),
        ),
        if f1_active {
            Span::styled(" █", Style::default().fg(Color::White))
        } else {
            Span::raw("")
        },
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if f1_active {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            })
            .title(" WORKER KEY "),
    );
    frame.render_widget(f1, chunks[0]);

    let f2_active = app.auth_field == AuthField::Password;
    let f2_val = mask(&app.input_password);
    let f2 = Paragraph::new(Line::from(vec![
        Span::styled(
            if f2_val.is_empty() { "<None>" } else { &f2_val },
            Style::default().fg(if f2_val.is_empty() {
                Color::DarkGray
            } else {
                Color::White
            }),
        ),
        if f2_active {
            Span::styled(" █", Style::default().fg(Color::White))
        } else {
            Span::raw("")
        },
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if f2_active {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            })
            .title(" PASSWORD "),
    );
    frame.render_widget(f2, chunks[1]);

    let hint = Paragraph::new(Line::from(vec![Span::styled(
        "Tab: Next • Enter: Save • Esc: Cancel",
        Style::default().fg(Color::White),
    )]));
    frame.render_widget(hint, chunks[2]);
}

fn render_workers(app: &App, frame: &mut Frame, area: Rect) {
    let worker_count = app
        .stats
        .as_ref()
        .and_then(|s| s.active_printers)
        .unwrap_or(app.workers.len());

    if app.workers.is_empty() {
        let msg = if app.is_loading && !app.is_unauthorized {
            "Loading active workers..."
        } else if app.is_unauthorized {
            "401 Unauthorized"
        } else {
            "No active workers."
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(" Active Workers ({worker_count}) "));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let y_offset = inner_area.height.saturating_sub(1) / 2;
        let text_area = Rect::new(inner_area.x, inner_area.y + y_offset, inner_area.width, 1);

        let p = Paragraph::new(Span::styled(
            msg,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);

        frame.render_widget(p, text_area);
        return;
    }

    let sel_worker = app.workers.get(app.selected_worker);
    let is_single_line = if let Some(w) = sel_worker {
        let id_str = w.id.as_deref().unwrap_or("?");
        let ip_str = w.ip.as_deref().unwrap_or("?");
        let agent_str = w.user_agent.as_deref().unwrap_or("?");
        let jobs_str = format!("{}", w.jobs_completed.unwrap_or(0));
        let full_len = id_str.len() + ip_str.len() + agent_str.len() + jobs_str.len() + 45;
        area.width >= full_len as u16
    } else {
        true
    };

    let detail_height = if is_single_line { 3 } else { 4 };
    let show_details = area.height >= 14;

    let chunks = if show_details {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(detail_height)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4)])
            .split(area)
    };

    let (headers, widths): (Vec<&str>, Vec<Constraint>) = if area.width >= 80 {
        (
            vec!["IDENT", "IP", "USER AGENT", "JOBS", "UPTIME", "STATUS"],
            vec![
                Constraint::Percentage(25),
                Constraint::Percentage(20),
                Constraint::Percentage(25),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
            ],
        )
    } else if area.width >= 50 {
        (
            vec!["IDENT", "IP", "JOBS", "STATUS"],
            vec![
                Constraint::Percentage(35),
                Constraint::Percentage(30),
                Constraint::Percentage(15),
                Constraint::Percentage(20),
            ],
        )
    } else {
        (
            vec!["IDENT", "JOBS", "STATUS"],
            vec![
                Constraint::Percentage(50),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ],
        )
    };

    let header = Row::new(headers.into_iter().map(|h| {
        Cell::from(Span::styled(
            h,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
    }))
    .height(1)
    .bottom_margin(1);

    let rows = app.workers.iter().enumerate().map(|(idx, w)| {
        let sel = idx == app.selected_worker;
        let st = if sel {
            Style::default()
                .bg(Color::Rgb(40, 40, 45))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let mut cells = vec![Cell::from(Span::styled(
            w.id.as_deref().unwrap_or("-"),
            Style::default().fg(Color::White),
        ))];

        if area.width >= 50 {
            cells.push(Cell::from(Span::styled(
                w.ip.as_deref().unwrap_or("-"),
                Style::default().fg(Color::White),
            )));
        }

        if area.width >= 80 {
            cells.push(Cell::from(Span::styled(
                w.user_agent.as_deref().unwrap_or("-"),
                Style::default().fg(Color::DarkGray),
            )));
        }

        cells.push(Cell::from(Span::styled(
            w.jobs_completed.unwrap_or(0).to_string(),
            Style::default().fg(Color::LightGreen),
        )));

        if area.width >= 80 {
            cells.push(Cell::from(Span::styled(
                w.age_seconds
                    .map(fmt_dur)
                    .unwrap_or_else(|| "-".to_string()),
                Style::default().fg(Color::DarkGray),
            )));
        }

        cells.push(Cell::from(Span::styled(
            "online",
            Style::default().fg(Color::Green),
        )));

        Row::new(cells).style(st).height(1)
    });

    let mut state = TableState::default();
    state.select(Some(app.selected_worker));
    frame.render_stateful_widget(
        Table::new(rows, widths).header(header).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(format!(" Active Workers ({worker_count}) ")),
        ),
        chunks[0],
        &mut state,
    );

    if show_details && let Some(w) = sel_worker {
        let id_str = w.id.as_deref().unwrap_or("?");
        let ip_str = w.ip.as_deref().unwrap_or("?");
        let agent_str = w.user_agent.as_deref().unwrap_or("?");
        let jobs_str = format!("{}", w.jobs_completed.unwrap_or(0));

        let detail_lines = if is_single_line {
            vec![Line::from(vec![
                Span::styled("Ident: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    id_str,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  • IP: ", Style::default().fg(Color::DarkGray)),
                Span::styled(ip_str, Style::default().fg(Color::White)),
                Span::styled("  • Status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "ONLINE",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  • Agent: ", Style::default().fg(Color::DarkGray)),
                Span::styled(agent_str, Style::default().fg(Color::Gray)),
                Span::styled("  • Jobs: ", Style::default().fg(Color::DarkGray)),
                Span::styled(jobs_str, Style::default().fg(Color::LightGreen)),
            ])]
        } else {
            vec![
                Line::from(vec![
                    Span::styled("Ident: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        id_str,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  • IP: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(ip_str, Style::default().fg(Color::White)),
                    Span::styled("  • Status: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        "ONLINE",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Agent: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(agent_str, Style::default().fg(Color::Gray)),
                    Span::styled("  • Jobs: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(jobs_str, Style::default().fg(Color::LightGreen)),
                ]),
            ]
        };

        let detail = Paragraph::new(detail_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Selected Worker "),
        );
        frame.render_widget(detail, chunks[1]);
    }
}

fn render_history(app: &App, frame: &mut Frame, area: Rect) {
    let history_count = app
        .stats
        .as_ref()
        .and_then(|s| s.history_count)
        .unwrap_or(app.history.len());
    let filtered = app.filtered_history();
    if filtered.is_empty() {
        let msg = if app.is_loading && !app.is_unauthorized {
            "Loading job history..."
        } else if app.is_unauthorized {
            "401 Unauthorized"
        } else if !app.search_query.is_empty() {
            "No matching history records."
        } else {
            "No history records found."
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(" History ({history_count}) "));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let y_offset = inner_area.height.saturating_sub(1) / 2;
        let text_area = Rect::new(inner_area.x, inner_area.y + y_offset, inner_area.width, 1);

        let p = Paragraph::new(Span::styled(
            msg,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);

        frame.render_widget(p, text_area);
        return;
    }

    let sel_history = filtered.get(app.selected_history).copied();
    let is_single_line = if let Some(h) = sel_history {
        let qid_str = h
            .queue_id
            .as_ref()
            .or(h.id.as_ref())
            .map(|v| match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => "-".to_string(),
            })
            .unwrap_or_else(|| "-".to_string());
        let file_str = h.file_name.as_deref().unwrap_or("?");
        let student_str = h.student.as_deref().unwrap_or("-");
        let status_str = h.status.as_deref().unwrap_or("-");
        let client_ip_str = h.client_ip.as_deref().unwrap_or("-");
        let hostname_str = h.hostname.as_deref().unwrap_or("-");
        let size_str = fmt_bytes(h.size_bytes.unwrap_or(0));
        let time_str = fmt_ts(h.timestamp.unwrap_or(0));
        let full_len = qid_str.len()
            + file_str.len()
            + student_str.len()
            + status_str.len()
            + client_ip_str.len()
            + hostname_str.len()
            + size_str.len()
            + time_str.len()
            + 60;
        area.width >= full_len as u16
    } else {
        true
    };

    let detail_height = if is_single_line { 3 } else { 4 };
    let show_details = area.height >= 14;

    let chunks = if show_details {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(4),
                Constraint::Length(detail_height),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(4)])
            .split(area)
    };

    let search_bar = Paragraph::new(Line::from(vec![
        Span::styled("Query: ", Style::default().fg(Color::White)),
        Span::styled(
            &app.search_query,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        if app.search_mode {
            Span::styled(" █", Style::default().fg(Color::White))
        } else {
            Span::raw("")
        },
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if app.search_mode {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            })
            .title(if app.search_mode {
                " Search (Type to filter, Esc to exit) "
            } else {
                " Search (Press '/' to filter) "
            }),
    );
    frame.render_widget(search_bar, chunks[0]);

    let (headers, widths): (Vec<&str>, Vec<Constraint>) = if area.width >= 100 {
        (
            vec!["ID", "STUDENT", "FILENAME", "IP", "SIZE", "STATUS", "TIME"],
            vec![
                Constraint::Length(8),
                Constraint::Percentage(16),
                Constraint::Percentage(28),
                Constraint::Percentage(16),
                Constraint::Percentage(10),
                Constraint::Percentage(10),
                Constraint::Percentage(12),
            ],
        )
    } else if area.width >= 65 {
        (
            vec!["ID", "STUDENT", "FILENAME", "STATUS", "TIME"],
            vec![
                Constraint::Length(8),
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(15),
                Constraint::Percentage(17),
            ],
        )
    } else {
        (
            vec!["ID", "FILENAME", "STATUS"],
            vec![
                Constraint::Length(8),
                Constraint::Percentage(60),
                Constraint::Percentage(32),
            ],
        )
    };

    let header = Row::new(headers.into_iter().map(|h| {
        Cell::from(Span::styled(
            h,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
    }))
    .height(1)
    .bottom_margin(1);

    let rows = filtered.iter().enumerate().map(|(idx, h)| {
        let sel = idx == app.selected_history;
        let st = if sel {
            Style::default()
                .bg(Color::Rgb(40, 40, 45))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let qid = h
            .queue_id
            .as_ref()
            .or(h.id.as_ref())
            .map(|v| match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => "-".to_string(),
            })
            .unwrap_or_else(|| "-".to_string());
        let status = h.status.as_deref().unwrap_or("-");
        let status_col = if status == "released" || status == "printed" {
            Color::Green
        } else {
            Color::DarkGray
        };

        let mut cells = vec![Cell::from(Span::styled(
            qid,
            Style::default().fg(Color::White),
        ))];

        if area.width >= 65 {
            cells.push(Cell::from(Span::styled(
                h.student.as_deref().unwrap_or("-"),
                Style::default().fg(Color::LightGreen),
            )));
        }

        cells.push(Cell::from(Span::styled(
            h.file_name.as_deref().unwrap_or("-"),
            Style::default().fg(Color::White),
        )));

        if area.width >= 100 {
            cells.push(Cell::from(Span::styled(
                h.client_ip.as_deref().unwrap_or("-"),
                Style::default().fg(Color::DarkGray),
            )));
            cells.push(Cell::from(Span::styled(
                fmt_bytes(h.size_bytes.unwrap_or(0)),
                Style::default().fg(Color::White),
            )));
        }

        cells.push(Cell::from(Span::styled(
            status,
            Style::default().fg(status_col),
        )));

        if area.width >= 65 {
            cells.push(Cell::from(Span::styled(
                fmt_ts(h.timestamp.unwrap_or(0)),
                Style::default().fg(Color::DarkGray),
            )));
        }

        Row::new(cells).style(st).height(1)
    });

    let mut state = TableState::default();
    state.select(Some(app.selected_history));
    frame.render_stateful_widget(
        Table::new(rows, widths).header(header).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(if app.search_query.is_empty() {
                    format!(" History ({history_count} records) ")
                } else {
                    format!(" History ({}/{} records) ", filtered.len(), history_count)
                }),
        ),
        chunks[1],
        &mut state,
    );

    if show_details && let Some(h) = sel_history {
        let qid_str = h
            .queue_id
            .as_ref()
            .or(h.id.as_ref())
            .map(|v| match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => "-".to_string(),
            })
            .unwrap_or_else(|| "-".to_string());
        let file_str = h.file_name.as_deref().unwrap_or("?");
        let student_str = h.student.as_deref().unwrap_or("-");
        let status_str = h.status.as_deref().unwrap_or("-");
        let client_ip_str = h.client_ip.as_deref().unwrap_or("-");
        let hostname_str = h.hostname.as_deref().unwrap_or("-");
        let size_str = fmt_bytes(h.size_bytes.unwrap_or(0));
        let time_str = fmt_ts(h.timestamp.unwrap_or(0));

        let detail_lines = if is_single_line {
            vec![Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(qid_str, Style::default().fg(Color::White)),
                Span::styled("  • File: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    file_str,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  • Student: ", Style::default().fg(Color::DarkGray)),
                Span::styled(student_str, Style::default().fg(Color::LightGreen)),
                Span::styled("  • Status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    status_str,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  • IP: ", Style::default().fg(Color::DarkGray)),
                Span::styled(client_ip_str, Style::default().fg(Color::White)),
                Span::styled("  • Hostname: ", Style::default().fg(Color::DarkGray)),
                Span::styled(hostname_str, Style::default().fg(Color::Gray)),
                Span::styled("  • Size: ", Style::default().fg(Color::DarkGray)),
                Span::styled(size_str, Style::default().fg(Color::White)),
                Span::styled("  • Time: ", Style::default().fg(Color::DarkGray)),
                Span::styled(time_str, Style::default().fg(Color::White)),
            ])]
        } else {
            vec![
                Line::from(vec![
                    Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(qid_str, Style::default().fg(Color::White)),
                    Span::styled("  • File: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        file_str,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  • Student: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(student_str, Style::default().fg(Color::LightGreen)),
                    Span::styled("  • Status: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        status_str,
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("IP: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(client_ip_str, Style::default().fg(Color::White)),
                    Span::styled("  • Hostname: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(hostname_str, Style::default().fg(Color::Gray)),
                    Span::styled("  • Size: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(size_str, Style::default().fg(Color::White)),
                    Span::styled("  • Time: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(time_str, Style::default().fg(Color::White)),
                ]),
            ]
        };

        let detail = Paragraph::new(detail_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Selected Record "),
        );
        frame.render_widget(detail, chunks[2]);
    }
}
