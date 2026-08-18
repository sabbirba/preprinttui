use chrono::{DateTime, Local};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
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
        format!("{d}d {h}h {m}m")
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
    let header_height = if is_compact { 1 } else { 2 };

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
        Tab::Search => render_search(app, frame, chunks[1]),
    }

    render_footer(app, frame, chunks[2], area.width);

    if app.auth_mode {
        render_auth_modal(app, frame, area);
    }
}

fn render_header(app: &App, frame: &mut Frame, area: Rect, is_compact: bool) {
    let stats = app.stats.as_ref();
    let has_stats = stats.is_some();
    let is_online = stats
        .and_then(|s| s.status.as_deref())
        .map(|s| s.eq_ignore_ascii_case("online"))
        .unwrap_or(false);

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

    let w_title = if !has_stats && app.workers.is_empty() {
        " Workers ".to_string()
    } else {
        format!(" Workers ({worker_count}) ")
    };

    let h_title = if !has_stats && app.history.is_empty() {
        " History ".to_string()
    } else {
        format!(" History ({history_count}) ")
    };

    let s_title = " Search ".to_string();

    let mut left_spans = Vec::new();

    let pill_active = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);
    let pill_inactive = Style::default().fg(Color::DarkGray);

    match app.tab {
        Tab::Workers => {
            left_spans.push(Span::styled(w_title, pill_active));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(h_title, pill_inactive));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(s_title, pill_inactive));
        }
        Tab::History => {
            left_spans.push(Span::styled(w_title, pill_inactive));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(h_title, pill_active));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(s_title, pill_inactive));
        }
        Tab::Search => {
            left_spans.push(Span::styled(w_title, pill_inactive));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(h_title, pill_inactive));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(s_title, pill_active));
        }
    }

    left_spans.push(Span::raw("  "));
    if app.search_mode {
        left_spans.push(Span::styled(
            "❯ ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        left_spans.push(Span::styled(
            &app.search_query,
            Style::default().fg(Color::White),
        ));
        if app.cursor_visible() {
            left_spans.push(Span::styled("█", Style::default().fg(Color::White)));
        } else {
            left_spans.push(Span::styled(" ", Style::default().fg(Color::White)));
        }
    } else if !app.search_query.is_empty() {
        left_spans.push(Span::styled(
            "❯ ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        left_spans.push(Span::styled(
            &app.search_query,
            Style::default().fg(Color::White),
        ));
    }

    let right_spans = if app.is_unauthorized {
        vec![
            Span::styled("● ", Style::default().fg(Color::DarkGray)),
            Span::styled("unauthorized", Style::default().fg(Color::White)),
        ]
    } else if !has_stats {
        vec![
            Span::styled(
                format!("{} ", app.spinner()),
                Style::default().fg(Color::White),
            ),
            Span::styled("connecting", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        let mut r = vec![
            Span::styled(
                if is_online { "● " } else { "○ " },
                Style::default().fg(if is_online {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
            Span::styled(
                if is_online { "online" } else { "offline" },
                Style::default().fg(if is_online {
                    Color::White
                } else {
                    Color::DarkGray
                }),
            ),
        ];

        if area.width >= 70 {
            r.push(Span::styled("  •  ", Style::default().fg(Color::DarkGray)));
            r.push(Span::styled(
                uptime_str,
                Style::default().fg(Color::DarkGray),
            ));
        }
        r
    };

    let left_p = Paragraph::new(Line::from(left_spans));
    let right_p = Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right);

    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    frame.render_widget(left_p, header_cols[0]);
    frame.render_widget(right_p, header_cols[1]);

    if !is_compact && area.height >= 2 {
        let sep_area = Rect::new(area.x, area.y + 1, area.width, 1);
        let sep = Paragraph::new("─".repeat(area.width as usize))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(sep, sep_area);
    }
}

fn render_footer(app: &App, frame: &mut Frame, area: Rect, width: u16) {
    let footer_line = if app.auth_mode {
        Line::from(vec![
            Span::styled(" tab ", Style::default().fg(Color::Black).bg(Color::White)),
            Span::styled(" next  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                " enter ",
                Style::default().fg(Color::Black).bg(Color::Green),
            ),
            Span::styled(" save  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                " esc ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
        ])
    } else if app.search_mode {
        Line::from(vec![
            Span::styled(" esc ", Style::default().fg(Color::Black).bg(Color::White)),
            Span::styled(" exit  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                " enter ",
                Style::default().fg(Color::Black).bg(Color::White),
            ),
            Span::styled(" confirm", Style::default().fg(Color::DarkGray)),
        ])
    } else if width < 60 {
        Line::from(vec![
            Span::styled("tab", Style::default().fg(Color::White)),
            Span::styled(" switch • ", Style::default().fg(Color::DarkGray)),
            Span::styled("e", Style::default().fg(Color::White)),
            Span::styled(" auth • ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::White)),
            Span::styled(" quit", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let status_span = if let Some(toast) = app.active_toast() {
            Span::styled(
                format!(" ✓ {toast} "),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else if app.is_loading {
            Span::styled(
                format!("{} {}", app.spinner(), app.status_msg),
                Style::default().fg(Color::White),
            )
        } else {
            Span::styled(&app.status_msg, Style::default().fg(Color::DarkGray))
        };

        Line::from(vec![
            Span::styled("tab", Style::default().fg(Color::White)),
            Span::styled(" switch  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("j/k", Style::default().fg(Color::White)),
            Span::styled(" move  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("/", Style::default().fg(Color::White)),
            Span::styled(" search  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("e", Style::default().fg(Color::White)),
            Span::styled(" auth  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("x", Style::default().fg(Color::White)),
            Span::styled(" clear  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("r", Style::default().fg(Color::White)),
            Span::styled(" refresh  •  ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::White)),
            Span::styled(" quit", Style::default().fg(Color::DarkGray)),
            Span::raw("    "),
            status_span,
        ])
    };

    frame.render_widget(Paragraph::new(footer_line), area);
}

fn render_auth_modal(app: &App, frame: &mut Frame, area: Rect) {
    let modal_width = 54.min(area.width.saturating_sub(2));
    let modal_height = 10.min(area.height.saturating_sub(2));

    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    frame.render_widget(Clear, modal_area);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::White))
        .title(" Credentials ");
    frame.render_widget(modal_block, modal_area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(modal_area);

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
            if f1_active { "› " } else { "  " },
            Style::default().fg(if f1_active {
                Color::White
            } else {
                Color::DarkGray
            }),
        ),
        Span::styled(
            if f1_val.is_empty() {
                "enter worker key..."
            } else {
                &f1_val
            },
            Style::default().fg(if f1_val.is_empty() {
                Color::DarkGray
            } else {
                Color::White
            }),
        ),
        if f1_active {
            Span::styled("█", Style::default().fg(Color::White))
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
            .title(" Key "),
    );
    frame.render_widget(f1, inner_chunks[0]);

    let f2_active = app.auth_field == AuthField::Password;
    let f2_val = mask(&app.input_password);
    let f2 = Paragraph::new(Line::from(vec![
        Span::styled(
            if f2_active { "› " } else { "  " },
            Style::default().fg(if f2_active {
                Color::White
            } else {
                Color::DarkGray
            }),
        ),
        Span::styled(
            if f2_val.is_empty() {
                "enter password..."
            } else {
                &f2_val
            },
            Style::default().fg(if f2_val.is_empty() {
                Color::DarkGray
            } else {
                Color::White
            }),
        ),
        if f2_active {
            Span::styled("█", Style::default().fg(Color::White))
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
            .title(" Password "),
    );
    frame.render_widget(f2, inner_chunks[1]);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("tab", Style::default().fg(Color::White)),
        Span::styled(" next  •  ", Style::default().fg(Color::DarkGray)),
        Span::styled("enter", Style::default().fg(Color::White)),
        Span::styled(" save  •  ", Style::default().fg(Color::DarkGray)),
        Span::styled("esc", Style::default().fg(Color::White)),
        Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, inner_chunks[2]);
}

fn render_workers(app: &App, frame: &mut Frame, area: Rect) {
    let filtered = app.filtered_workers();
    if filtered.is_empty() {
        let msg = if app.is_loading && !app.is_unauthorized {
            "Loading..."
        } else if app.is_unauthorized {
            "401 Unauthorized"
        } else if !app.search_query.is_empty() {
            "No matches."
        } else {
            "No workers."
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Workers ");

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

    let sel_worker = filtered.get(app.selected_worker).copied();
    let detail_height = 3;
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
            vec!["", "IDENT", "IP", "AGENT", "JOBS", "UPTIME", "STATUS"],
            vec![
                Constraint::Length(2),
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
            vec!["", "IDENT", "IP", "JOBS", "STATUS"],
            vec![
                Constraint::Length(2),
                Constraint::Percentage(35),
                Constraint::Percentage(30),
                Constraint::Percentage(15),
                Constraint::Percentage(20),
            ],
        )
    } else {
        (
            vec!["", "IDENT", "JOBS", "STATUS"],
            vec![
                Constraint::Length(2),
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

    let rows = filtered.iter().enumerate().map(|(idx, w)| {
        let sel = idx == app.selected_worker;
        let st = if sel {
            Style::default()
                .bg(Color::Rgb(35, 35, 40))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let indicator = if sel { "❯" } else { " " };
        let mut cells = vec![
            Cell::from(Span::styled(
                indicator,
                Style::default().fg(if sel { Color::White } else { Color::DarkGray }),
            )),
            Cell::from(Span::styled(
                w.id.as_deref().unwrap_or("-"),
                Style::default().fg(Color::White),
            )),
        ];

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
                .title(" Workers "),
        ),
        chunks[0],
        &mut state,
    );

    if show_details && let Some(w) = sel_worker {
        let id_str = w.id.as_deref().unwrap_or("?");
        let ip_str = w.ip.as_deref().unwrap_or("?");
        let agent_str = w.user_agent.as_deref().unwrap_or("?");
        let jobs_str = format!("{}", w.jobs_completed.unwrap_or(0));

        let mut detail_spans = vec![
            Span::styled("Ident: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                id_str,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  •  Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled("● online", Style::default().fg(Color::Green)),
            Span::styled("  •  Jobs: ", Style::default().fg(Color::DarkGray)),
            Span::styled(jobs_str, Style::default().fg(Color::LightGreen)),
        ];

        if area.width >= 65 {
            detail_spans.push(Span::styled(
                "  •  IP: ",
                Style::default().fg(Color::DarkGray),
            ));
            detail_spans.push(Span::styled(ip_str, Style::default().fg(Color::White)));
        }

        if area.width >= 90 {
            detail_spans.push(Span::styled(
                "  •  Agent: ",
                Style::default().fg(Color::DarkGray),
            ));
            detail_spans.push(Span::styled(agent_str, Style::default().fg(Color::Gray)));
        }

        let detail = Paragraph::new(Line::from(detail_spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Selected "),
        );
        frame.render_widget(detail, chunks[1]);
    }
}

fn render_history(app: &App, frame: &mut Frame, area: Rect) {
    let filtered = app.filtered_history();
    if filtered.is_empty() {
        let msg = if app.is_loading && !app.is_unauthorized {
            "Loading..."
        } else if app.is_unauthorized {
            "401 Unauthorized"
        } else if !app.search_query.is_empty() {
            "No matches."
        } else {
            "No history."
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" History ");

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
    let detail_height = 3;
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

    let (headers, widths): (Vec<&str>, Vec<Constraint>) = if area.width >= 90 {
        (
            vec!["", "ID", "STUDENT", "IP", "SIZE", "STATUS", "TIME"],
            vec![
                Constraint::Length(2),
                Constraint::Length(8),
                Constraint::Percentage(24),
                Constraint::Percentage(24),
                Constraint::Percentage(14),
                Constraint::Percentage(14),
                Constraint::Percentage(24),
            ],
        )
    } else if area.width >= 60 {
        (
            vec!["", "ID", "STUDENT", "SIZE", "STATUS", "TIME"],
            vec![
                Constraint::Length(2),
                Constraint::Length(8),
                Constraint::Percentage(32),
                Constraint::Percentage(18),
                Constraint::Percentage(18),
                Constraint::Percentage(32),
            ],
        )
    } else {
        (
            vec!["", "ID", "STUDENT", "STATUS"],
            vec![
                Constraint::Length(2),
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
                .bg(Color::Rgb(35, 35, 40))
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

        let indicator = if sel { "❯" } else { " " };
        let mut cells = vec![
            Cell::from(Span::styled(
                indicator,
                Style::default().fg(if sel { Color::White } else { Color::DarkGray }),
            )),
            Cell::from(Span::styled(qid, Style::default().fg(Color::White))),
            Cell::from(Span::styled(
                h.student.as_deref().unwrap_or("-"),
                Style::default().fg(Color::LightGreen),
            )),
        ];

        if area.width >= 90 {
            cells.push(Cell::from(Span::styled(
                h.client_ip.as_deref().unwrap_or("-"),
                Style::default().fg(Color::DarkGray),
            )));
        }

        if area.width >= 60 {
            cells.push(Cell::from(Span::styled(
                fmt_bytes(h.size_bytes.unwrap_or(0)),
                Style::default().fg(Color::White),
            )));
        }

        cells.push(Cell::from(Span::styled(
            status,
            Style::default().fg(status_col),
        )));

        if area.width >= 60 {
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
                .title(" History "),
        ),
        chunks[0],
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
        let student_str = h.student.as_deref().unwrap_or("-");
        let status_str = h.status.as_deref().unwrap_or("-");
        let client_ip_str = h.client_ip.as_deref().unwrap_or("-");
        let hostname_str = h.hostname.as_deref().unwrap_or("-");
        let size_str = fmt_bytes(h.size_bytes.unwrap_or(0));
        let time_str = fmt_ts(h.timestamp.unwrap_or(0));

        let mut detail_spans = vec![
            Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
            Span::styled(qid_str, Style::default().fg(Color::White)),
            Span::styled("  •  Student: ", Style::default().fg(Color::DarkGray)),
            Span::styled(student_str, Style::default().fg(Color::LightGreen)),
            Span::styled("  •  Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("● {status_str}"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if area.width >= 70 {
            detail_spans.push(Span::styled(
                "  •  Size: ",
                Style::default().fg(Color::DarkGray),
            ));
            detail_spans.push(Span::styled(size_str, Style::default().fg(Color::White)));
        }

        if area.width >= 90 {
            detail_spans.push(Span::styled(
                "  •  IP: ",
                Style::default().fg(Color::DarkGray),
            ));
            detail_spans.push(Span::styled(
                client_ip_str,
                Style::default().fg(Color::White),
            ));
        }

        if area.width >= 115 {
            detail_spans.push(Span::styled(
                "  •  Host: ",
                Style::default().fg(Color::DarkGray),
            ));
            detail_spans.push(Span::styled(hostname_str, Style::default().fg(Color::Gray)));
        }

        if area.width >= 60 {
            detail_spans.push(Span::styled(
                "  •  Time: ",
                Style::default().fg(Color::DarkGray),
            ));
            detail_spans.push(Span::styled(time_str, Style::default().fg(Color::White)));
        }

        let detail = Paragraph::new(Line::from(detail_spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Selected "),
        );
        frame.render_widget(detail, chunks[1]);
    }
}

fn render_search(app: &App, frame: &mut Frame, area: Rect) {
    let f_workers = app.filtered_workers();
    let f_history = app.filtered_history();

    let has_matches = !f_workers.is_empty() || !f_history.is_empty();
    if !has_matches {
        let msg = if app.is_loading && !app.is_unauthorized {
            "Searching..."
        } else if app.is_unauthorized {
            "401 Unauthorized"
        } else if !app.search_query.is_empty() {
            "No matches found."
        } else {
            "Type query to search workers and history."
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Search ");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let y_offset = inner.height.saturating_sub(1) / 2;
        let text_area = Rect::new(inner.x, inner.y + y_offset, inner.width, 1);
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

    if !f_workers.is_empty() && !f_history.is_empty() && area.height >= 16 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        render_workers(app, frame, chunks[0]);
        render_history(app, frame, chunks[1]);
    } else if !f_workers.is_empty() && f_history.is_empty() {
        render_workers(app, frame, area);
    } else {
        render_history(app, frame, area);
    }
}
