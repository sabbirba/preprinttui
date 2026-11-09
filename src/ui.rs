use chrono::{DateTime, Local};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph, Row, Table},
};

use crate::{
    app::{App, LISTS, List, Tab, View},
    types::{HistoryEntry, WorkerInfo},
};

const WHITE: Style = Style::new().fg(Color::White);
const BOLD: Style = WHITE.add_modifier(Modifier::BOLD);
const DIM: Style = Style::new().fg(Color::DarkGray);
const GRAY: Style = Style::new().fg(Color::Gray);
const GREEN: Style = Style::new().fg(Color::Green);
const LIGHT_GREEN: Style = Style::new().fg(Color::LightGreen);
const ACTIVE_PILL: Style = Style::new()
    .fg(Color::Black)
    .bg(Color::White)
    .add_modifier(Modifier::BOLD);
const SELECTED_ROW: Style = Style::new()
    .bg(Color::Rgb(35, 35, 40))
    .add_modifier(Modifier::BOLD);
const SEPARATOR: &str = "  •  ";
const YELLOW: Style = Style::new().fg(Color::Yellow);
const STALE_AFTER_MS: u64 = 30_000;
const NARROW_SEPARATOR: &str = " • ";
const FOOTER_HINTS: [(&str, &str); 7] = [
    ("tab", "switch"),
    ("j/k", "move"),
    ("/", "search"),
    ("e", "auth"),
    ("x", "clear"),
    ("r", "refresh"),
    ("q", "quit"),
];
const NARROW_FOOTER_HINTS: [(&str, &str); 3] = [("tab", "switch"), ("e", "auth"), ("q", "quit")];
const TABLE_CHROME_ROWS: u16 = 3;

struct Column<T> {
    min_width: u16,
    title: &'static str,
    width: Constraint,
    cell: fn(&T) -> Span<'_>,
}

type Detail<'a> = (u16, &'static str, Span<'a>);

fn fmt_bytes(b: u64) -> String {
    match b {
        0..1024 => format!("{b} B"),
        1024..1_048_576 => format!("{:.1} KB", b as f64 / 1024.0),
        _ => format!("{:.2} MB", b as f64 / 1_048_576.0),
    }
}

fn fmt_dur(s: u64) -> String {
    let (d, h, m, sec) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60, s % 60);
    if d > 0 {
        format!("{d}d {h:02}h {m:02}m")
    } else if h > 0 {
        format!("{h}h {m:02}m {sec:02}s")
    } else if m > 0 {
        format!("{m}m {sec:02}s")
    } else {
        format!("{sec}s")
    }
}

fn fmt_age(s: u64) -> String {
    match s {
        0..60 => format!("{s}s ago"),
        60..3600 => format!("{}m ago", s / 60),
        3600..86400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
    }
}

fn to_millis(ts: u64) -> u64 {
    if ts < 10_000_000_000 {
        ts.saturating_mul(1000)
    } else {
        ts
    }
}

fn fmt_ts(ts: u64) -> String {
    i64::try_from(to_millis(ts))
        .ok()
        .filter(|_| ts != 0)
        .and_then(DateTime::from_timestamp_millis)
        .map_or_else(
            || "-".to_string(),
            |dt| {
                dt.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            },
        )
}

fn or_dash(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

fn panel<'a>(title: impl Into<Line<'a>>, border: Style) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(title)
}

fn key_hints<'a>(hints: &[(&'a str, &'a str)], separator: &str) -> Vec<Span<'a>> {
    let last = hints.len().saturating_sub(1);
    let mut spans = Vec::with_capacity(hints.len() * 2);
    for (i, &(key, label)) in hints.iter().enumerate() {
        let tail = if i == last { "" } else { separator };
        spans.push(Span::styled(key, WHITE));
        spans.push(Span::styled(format!(" {label}{tail}"), DIM));
    }
    spans
}

fn hint_at<'a>(hints: &[(&'a str, &'a str)], separator: &str, x: u16) -> Option<&'a str> {
    let mut start = 0;
    for &(key, label) in hints {
        let end = start + key.chars().count() + label.chars().count() + 1;
        if (start..end).contains(&usize::from(x)) {
            return Some(key);
        }
        start = end + separator.chars().count();
    }
    None
}

fn hints_width(hints: &[(&str, &str)], separator: &str) -> u16 {
    let text: usize = hints
        .iter()
        .map(|(k, l)| k.chars().count() + l.chars().count() + 1)
        .sum();
    let gaps = separator.chars().count() * hints.len().saturating_sub(1);
    u16::try_from(text + gaps).unwrap_or(u16::MAX)
}

fn badges<'a>(items: &[(&'a str, Color, &'a str)]) -> Line<'a> {
    let mut spans = Vec::with_capacity(items.len() * 2);
    for &(key, color, label) in items {
        spans.push(Span::styled(key, Style::new().fg(Color::Black).bg(color)));
        spans.push(Span::styled(label, DIM));
    }
    Line::from(spans)
}

fn centered(area: Rect, width: u16, height: u16, margin: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(margin));
    let height = height.min(area.height.saturating_sub(margin));
    Rect::new(
        (area.width - width) / 2,
        (area.height - height) / 2,
        width,
        height,
    )
}

pub fn auth_modal_area(area: Rect) -> Rect {
    centered(area, 46, 7, 2)
}

pub fn inspector_modal_area(area: Rect) -> Rect {
    centered(area, 72, 22, 4)
}

fn auth_hints(app: &App) -> [(&'static str, &'static str); 3] {
    let toggle = if app.mask_credentials { "show" } else { "hide" };
    [("enter", "save"), ("tab", toggle), ("esc", "cancel")]
}

fn auth_hint_row(modal: Rect) -> Rect {
    Rect::new(modal.x + 1, modal.y + 4, modal.width.saturating_sub(2), 1)
}

pub fn auth_hint_at(app: &App, screen: Rect, pos: Position) -> Option<&'static str> {
    let row = auth_hint_row(auth_modal_area(screen));
    let hints = auth_hints(app);
    let start = row.x + row.width.saturating_sub(hints_width(&hints, SEPARATOR)) / 2;
    (pos.y == row.y)
        .then(|| hint_at(&hints, SEPARATOR, pos.x.checked_sub(start)?))
        .flatten()
}

fn footer_hints(width: u16) -> (&'static [(&'static str, &'static str)], &'static str) {
    if width < 60 {
        (&NARROW_FOOTER_HINTS, NARROW_SEPARATOR)
    } else {
        (&FOOTER_HINTS, SEPARATOR)
    }
}

pub fn footer_key_at(app: &App, screen: Rect, x: u16) -> Option<&'static str> {
    if app.auth_mode || app.search_mode {
        return None;
    }
    let (hints, separator) = footer_hints(screen.width);
    hint_at(hints, separator, x)
}

pub fn tab_at(app: &App, x: u16) -> Option<Tab> {
    let mut start = 0;
    for (tab, title) in tab_titles(app) {
        let end = start + title.chars().count();
        if (start..end).contains(&usize::from(x)) {
            return Some(tab);
        }
        start = end + 1;
    }
    None
}

pub fn list_row_at(app: &App, pos: Position) -> Option<(List, usize)> {
    LISTS.into_iter().find_map(|list| {
        let view = app.view(list).get();
        let row = pos.y.checked_sub(view.area.y + TABLE_CHROME_ROWS)?;
        (view.area.contains(pos) && row < view.area.height.saturating_sub(TABLE_CHROME_ROWS + 1))
            .then_some((list, view.offset + usize::from(row)))
    })
}

pub fn render_ui(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < 25 || area.height < 6 {
        let msg = format!("Resize window ({}x{})", area.width, area.height);
        frame.render_widget(Paragraph::new(msg).style(WHITE), area);
        return;
    }

    for list in LISTS {
        let offset = app.view(list).get().offset;
        app.view(list).set(View {
            area: Rect::default(),
            offset,
        });
    }

    let compact = area.height < 18;
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(if compact { 1 } else { 2 }),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(app, frame, header, compact);
    match app.tab {
        Tab::Workers => render_workers(app, frame, body),
        Tab::Queue => render_jobs(app, frame, body, List::Queue),
        Tab::History => render_jobs(app, frame, body, List::History),
        Tab::Search => render_search(app, frame, body),
    }
    render_footer(app, frame, footer);

    if app.auth_mode {
        render_auth_modal(app, frame, area);
    } else if app.inspector_mode {
        render_inspector_modal(app, frame, area);
    }
}

fn tab_title(name: &str, has_data: bool, count: usize) -> String {
    if has_data {
        format!(" {name} ({count}) ")
    } else {
        format!(" {name} ")
    }
}

fn tab_titles(app: &App) -> [(Tab, String); 4] {
    let stats = app.stats.as_ref();
    [
        (
            Tab::Workers,
            tab_title(
                "Workers",
                stats.is_some() || !app.workers.is_empty(),
                stats
                    .and_then(|s| s.active_printers)
                    .unwrap_or(app.workers.len()),
            ),
        ),
        (
            Tab::Queue,
            tab_title(
                "Queue",
                stats.is_some() || !app.queue.is_empty(),
                stats
                    .and_then(|s| s.queued_count)
                    .unwrap_or(app.queue.len()),
            ),
        ),
        (
            Tab::History,
            tab_title(
                "History",
                stats.is_some() || !app.history.is_empty(),
                stats
                    .and_then(|s| s.history_count)
                    .unwrap_or(app.history.len()),
            ),
        ),
        (Tab::Search, " Search ".to_string()),
    ]
}

fn render_header(app: &App, frame: &mut Frame, area: Rect, compact: bool) {
    let stats = app.stats.as_ref();
    let tabs = tab_titles(app);

    let mut left = Vec::new();
    for (i, (tab, title)) in tabs.into_iter().enumerate() {
        if i > 0 {
            left.push(Span::raw(" "));
        }
        left.push(Span::styled(
            title,
            if tab == app.tab { ACTIVE_PILL } else { DIM },
        ));
    }
    left.push(Span::raw("  "));
    if app.search_mode || !app.search_query.is_empty() {
        left.push(Span::styled("❯ ", BOLD));
        left.push(Span::styled(app.search_query.as_str(), WHITE));
        if app.search_mode {
            let cursor = if app.cursor_visible() { "█" } else { " " };
            left.push(Span::styled(cursor, WHITE));
        }
    }

    let right = match stats {
        _ if app.is_unauthorized => vec![Span::styled("unauthorized", DIM)],
        None => vec![
            Span::styled(format!("{} ", app.spinner()), WHITE),
            Span::styled("connecting", DIM),
        ],
        Some(stats) => {
            let online = stats
                .status
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("online"));
            let mut spans = vec![if online {
                Span::styled("online", GREEN)
            } else {
                Span::styled("offline", DIM)
            }];
            if let Some(latency) = app.latency_ms
                && area.width >= 75
            {
                spans.push(Span::styled(format!("{SEPARATOR}{latency:>4}ms"), DIM));
            }
            if area.width >= 90 {
                let uptime = app.realtime_uptime().map_or_else(|| "-".into(), fmt_dur);
                spans.push(Span::styled(format!("{SEPARATOR}{uptime}"), DIM));
            }
            spans
        }
    };

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);
    frame.render_widget(Paragraph::new(Line::from(left)), left_area);
    frame.render_widget(
        Paragraph::new(Line::from(right)).alignment(Alignment::Right),
        right_area,
    );

    if !compact && area.height >= 2 {
        let rule = Paragraph::new("─".repeat(usize::from(area.width))).style(DIM);
        frame.render_widget(rule, Rect::new(area.x, area.y + 1, area.width, 1));
    }
}

fn status_span(app: &App) -> Span<'_> {
    if let Some(toast) = app.active_toast() {
        Span::styled(
            format!(" ✓ {toast} "),
            Style::new()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else if app.is_loading {
        Span::styled(format!("{} {}", app.spinner(), app.status_msg), WHITE)
    } else {
        Span::styled(app.status_msg.as_str(), DIM)
    }
}

fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let line = if app.auth_mode {
        badges(&[
            (" tab ", Color::White, " next  "),
            (" enter ", Color::Green, " save  "),
            (" esc ", Color::DarkGray, " cancel"),
        ])
    } else if app.search_mode {
        badges(&[
            (" esc ", Color::White, " exit  "),
            (" enter ", Color::White, " confirm"),
        ])
    } else {
        let (hints, separator) = footer_hints(area.width);
        let mut spans = key_hints(hints, separator);
        if area.width >= 60 {
            spans.push(Span::raw("    "));
            spans.push(status_span(app));
        }
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_auth_modal(app: &App, frame: &mut Frame, area: Rect) {
    let modal = auth_modal_area(area);
    frame.render_widget(Clear, modal);
    frame.render_widget(panel(" Password ", WHITE), modal);

    let input_area = Rect::new(modal.x + 1, modal.y + 1, modal.width.saturating_sub(2), 3);

    let value = if app.mask_credentials {
        "•".repeat(app.input_password.len())
    } else {
        app.input_password.clone()
    };
    let field = if value.is_empty() {
        Span::styled("enter password...", DIM)
    } else {
        Span::styled(value, WHITE)
    };
    let input = Paragraph::new(Line::from(vec![
        Span::styled("› ", WHITE),
        field,
        Span::styled("█", WHITE),
    ]))
    .block(panel(" Password ", WHITE));
    frame.render_widget(input, input_area);

    let hint = key_hints(&auth_hints(app), SEPARATOR);
    frame.render_widget(
        Paragraph::new(Line::from(hint)).alignment(Alignment::Center),
        auth_hint_row(modal),
    );
}

fn empty_message<'a>(app: &App, loading: &'a str, no_match: &'a str, empty: &'a str) -> &'a str {
    if app.is_unauthorized {
        "401 Unauthorized"
    } else if app.is_loading {
        loading
    } else if !app.search_query.is_empty() {
        no_match
    } else {
        empty
    }
}

fn render_empty(frame: &mut Frame, area: Rect, title: &str, message: &str) {
    let block = panel(title, DIM);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let line = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1) / 2,
        inner.width,
        1,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(message, BOLD)).alignment(Alignment::Center),
        line,
    );
}

fn scrolled_offset(previous: usize, selected: usize, rows: usize, len: usize) -> usize {
    let offset = previous.min(len.saturating_sub(rows));
    if selected < offset {
        selected
    } else if rows > 0 && selected >= offset + rows {
        selected + 1 - rows
    } else {
        offset
    }
}

#[allow(clippy::too_many_arguments)]
fn render_table<T>(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    title: &str,
    list: List,
    items: &[&T],
    columns: &[Column<T>],
    details: fn(&T) -> Vec<Detail<'_>>,
) {
    let selected = app.selected[list as usize];
    let visible: Vec<&Column<T>> = columns
        .iter()
        .filter(|c| area.width >= c.min_width)
        .collect();
    let show_details = area.height >= 14;
    let [table_area, detail_area] = Layout::vertical([
        Constraint::Min(4),
        Constraint::Length(if show_details { 3 } else { 0 }),
    ])
    .areas(area);

    let rows = usize::from(table_area.height.saturating_sub(TABLE_CHROME_ROWS + 1));
    let offset = scrolled_offset(app.view(list).get().offset, selected, rows, items.len());
    app.view(list).set(View {
        area: table_area,
        offset,
    });

    let header = Row::new(
        std::iter::once("")
            .chain(visible.iter().map(|c| c.title))
            .map(|title| Span::styled(title, BOLD)),
    )
    .bottom_margin(1);
    let widths = std::iter::once(Constraint::Length(2)).chain(visible.iter().map(|c| c.width));
    let window = items.iter().copied().enumerate().skip(offset).take(rows);
    let table_rows = window.map(|(index, item)| {
        let is_selected = index == selected;
        let indicator = if is_selected {
            Span::styled("❯", WHITE)
        } else {
            Span::styled(" ", DIM)
        };
        Row::new(std::iter::once(indicator).chain(visible.iter().map(|c| (c.cell)(item)))).style(
            if is_selected {
                SELECTED_ROW
            } else {
                Style::new()
            },
        )
    });

    frame.render_widget(
        Table::new(table_rows, widths)
            .header(header)
            .block(panel(title, DIM)),
        table_area,
    );

    if let Some(item) = items.get(selected).filter(|_| show_details) {
        let mut spans = Vec::new();
        for (min_width, label, value) in details(item) {
            if area.width >= min_width {
                let lead = if spans.is_empty() { "" } else { SEPARATOR };
                spans.push(Span::styled(format!("{lead}{label}: "), DIM));
                spans.push(value);
            }
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).block(panel(" Selected ", DIM)),
            detail_area,
        );
    }
}

fn now_millis() -> u64 {
    u64::try_from(Local::now().timestamp_millis()).unwrap_or(0)
}

fn worker_uptime(worker: &WorkerInfo) -> String {
    worker.connected_at.map_or_else(
        || "-".to_string(),
        |t| fmt_dur(now_millis().saturating_sub(t) / 1000),
    )
}

fn worker_last_check(worker: &WorkerInfo) -> String {
    worker.last_update_checked.filter(|&t| t != 0).map_or_else(
        || "never".to_string(),
        |t| fmt_age(now_millis().saturating_sub(to_millis(t)) / 1000),
    )
}

fn worker_status(worker: &WorkerInfo) -> Span<'static> {
    let silent_for = worker
        .last_seen_at
        .map(|t| now_millis().saturating_sub(to_millis(t)));
    if silent_for.is_some_and(|ms| ms > STALE_AFTER_MS) {
        Span::styled("stale", YELLOW)
    } else {
        Span::styled("online", GREEN)
    }
}

fn worker_columns() -> [Column<WorkerInfo>; 8] {
    [
        Column {
            min_width: 0,
            title: "IDENT",
            width: Constraint::Fill(22),
            cell: |w| Span::styled(or_dash(w.id.as_deref()), WHITE),
        },
        Column {
            min_width: 55,
            title: "IP",
            width: Constraint::Fill(18),
            cell: |w| Span::styled(or_dash(w.ip.as_deref()), WHITE),
        },
        Column {
            min_width: 90,
            title: "AGENT",
            width: Constraint::Fill(22),
            cell: |w| Span::styled(or_dash(w.user_agent.as_deref()), DIM),
        },
        Column {
            min_width: 90,
            title: "CHECKED",
            width: Constraint::Fill(10),
            cell: |w| Span::styled(worker_last_check(w), DIM),
        },
        Column {
            min_width: 0,
            title: "JOBS",
            width: Constraint::Fill(10),
            cell: |w| Span::styled(w.jobs_completed.unwrap_or(0).to_string(), LIGHT_GREEN),
        },
        Column {
            min_width: 55,
            title: "CONNS",
            width: Constraint::Fill(8),
            cell: |w| Span::styled(w.connections.unwrap_or(1).to_string(), WHITE),
        },
        Column {
            min_width: 90,
            title: "UPTIME",
            width: Constraint::Fill(10),
            cell: |w| Span::styled(worker_uptime(w), DIM),
        },
        Column {
            min_width: 0,
            title: "STATUS",
            width: Constraint::Fill(10),
            cell: worker_status,
        },
    ]
}

fn worker_details(w: &WorkerInfo) -> Vec<Detail<'_>> {
    vec![
        (
            0,
            "Ident",
            Span::styled(w.id.as_deref().unwrap_or("?"), BOLD),
        ),
        (0, "Status", worker_status(w)),
        (
            0,
            "Jobs",
            Span::styled(w.jobs_completed.unwrap_or(0).to_string(), LIGHT_GREEN),
        ),
        (
            0,
            "Conns",
            Span::styled(w.connections.unwrap_or(1).to_string(), WHITE),
        ),
        (
            75,
            "IP",
            Span::styled(w.ip.as_deref().unwrap_or("?"), WHITE),
        ),
        (
            100,
            "Agent",
            Span::styled(w.user_agent.as_deref().unwrap_or("?"), GRAY),
        ),
    ]
}

fn history_status(h: &HistoryEntry) -> Span<'_> {
    let status = or_dash(h.status.as_deref());
    let style = if matches!(status, "released" | "printed") {
        GREEN
    } else {
        DIM
    };
    Span::styled(status, style)
}

fn history_columns() -> [Column<HistoryEntry>; 6] {
    [
        Column {
            min_width: 0,
            title: "ID",
            width: Constraint::Length(8),
            cell: |h| Span::styled(h.display_id(), WHITE),
        },
        Column {
            min_width: 0,
            title: "STUDENT",
            width: Constraint::Fill(24),
            cell: |h| Span::styled(or_dash(h.student.as_deref()), LIGHT_GREEN),
        },
        Column {
            min_width: 90,
            title: "IP",
            width: Constraint::Fill(24),
            cell: |h| Span::styled(or_dash(h.client_ip.as_deref()), DIM),
        },
        Column {
            min_width: 60,
            title: "SIZE",
            width: Constraint::Fill(14),
            cell: |h| Span::styled(fmt_bytes(h.size_bytes.unwrap_or(0)), WHITE),
        },
        Column {
            min_width: 0,
            title: "STATUS",
            width: Constraint::Fill(14),
            cell: history_status,
        },
        Column {
            min_width: 60,
            title: "TIME",
            width: Constraint::Fill(24),
            cell: |h| Span::styled(fmt_ts(h.timestamp.unwrap_or(0)), DIM),
        },
    ]
}

fn history_details(h: &HistoryEntry) -> Vec<Detail<'_>> {
    vec![
        (0, "ID", Span::styled(h.display_id(), WHITE)),
        (
            0,
            "Student",
            Span::styled(or_dash(h.student.as_deref()), LIGHT_GREEN),
        ),
        (
            0,
            "Status",
            Span::styled(
                or_dash(h.status.as_deref()),
                GREEN.add_modifier(Modifier::BOLD),
            ),
        ),
        (
            70,
            "Size",
            Span::styled(fmt_bytes(h.size_bytes.unwrap_or(0)), WHITE),
        ),
        (
            90,
            "IP",
            Span::styled(or_dash(h.client_ip.as_deref()), WHITE),
        ),
        (
            115,
            "Host",
            Span::styled(or_dash(h.hostname.as_deref()), GRAY),
        ),
        (
            60,
            "Time",
            Span::styled(fmt_ts(h.timestamp.unwrap_or(0)), WHITE),
        ),
    ]
}

fn render_workers(app: &App, frame: &mut Frame, area: Rect) {
    let items = app.filtered_workers();
    if items.is_empty() {
        let msg = empty_message(app, "Loading...", "No matches.", "No workers.");
        render_empty(frame, area, " Workers ", msg);
    } else {
        render_table(
            app,
            frame,
            area,
            " Workers ",
            List::Workers,
            &items,
            &worker_columns(),
            worker_details,
        );
    }
}

fn render_jobs(app: &App, frame: &mut Frame, area: Rect, list: List) {
    let (items, title, empty) = if list == List::Queue {
        (app.filtered_queue(), " Queue ", "Queue is empty.")
    } else {
        (app.filtered_history(), " History ", "No history.")
    };
    if items.is_empty() {
        let msg = empty_message(app, "Loading...", "No matches.", empty);
        render_empty(frame, area, title, msg);
    } else {
        render_table(
            app,
            frame,
            area,
            title,
            list,
            &items,
            &history_columns(),
            history_details,
        );
    }
}

fn render_search(app: &App, frame: &mut Frame, area: Rect) {
    let has_workers = !app.filtered_workers().is_empty();
    let has_history = !app.filtered_history().is_empty();
    match (has_workers, has_history) {
        (false, false) => {
            let msg = empty_message(
                app,
                "Searching...",
                "No matches found.",
                "Type query to search workers and history.",
            );
            render_empty(frame, area, " Search ", msg);
        }
        (true, true) if area.height >= 16 => {
            let [top, bottom] =
                Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .areas(area);
            render_workers(app, frame, top);
            render_jobs(app, frame, bottom, List::History);
        }
        (true, false) => render_workers(app, frame, area),
        _ => render_jobs(app, frame, area, List::History),
    }
}

fn inspected_item(app: &App) -> (String, String) {
    let index = app.selected[app.inspecting as usize];
    let job = |h: &&HistoryEntry| {
        (
            format!(" Job: #{} ", h.display_id()),
            serde_json::to_string_pretty(h).unwrap_or_default(),
        )
    };
    match app.inspecting {
        List::Workers => app.filtered_workers().get(index).map(|w| {
            (
                format!(" Worker: {} ", w.id.as_deref().unwrap_or("worker")),
                serde_json::to_string_pretty(w).unwrap_or_default(),
            )
        }),
        List::Queue => app.filtered_queue().get(index).map(job),
        List::History => app.filtered_history().get(index).map(job),
    }
    .unwrap_or_else(|| (" Details ".to_string(), "{}".to_string()))
}

fn inspector_body_height(modal: Rect) -> usize {
    usize::from(modal.height.saturating_sub(3))
}

pub fn inspector_max_scroll(app: &App, area: Rect) -> usize {
    let body_height = inspector_body_height(inspector_modal_area(area));
    inspected_item(app)
        .1
        .lines()
        .count()
        .saturating_sub(body_height)
}

fn render_inspector_modal(app: &App, frame: &mut Frame, area: Rect) {
    let modal = inspector_modal_area(area);
    frame.render_widget(Clear, modal);

    let (title, json) = inspected_item(app);
    let lines: Vec<&str> = json.lines().collect();
    let body_height = inspector_body_height(modal);
    let scroll = app.inspector_scroll.min(inspector_max_scroll(app, area));

    let block = panel(title, WHITE);
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let [body, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let text: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(body_height)
        .map(|line| Line::styled(*line, WHITE))
        .collect();
    frame.render_widget(Paragraph::new(text), body);

    let mut hint = key_hints(
        &[("esc", "close"), ("←/→", "prev/next"), ("↑/↓", "scroll")],
        SEPARATOR,
    );
    if lines.len() > body_height {
        hint.push(Span::styled(
            format!("{SEPARATOR}line {}/{}", scroll + 1, lines.len()),
            DIM,
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(hint)).alignment(Alignment::Center),
        hint_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn rows_at(width: u16) -> (String, String) {
        let worker = serde_json::from_value(serde_json::json!({
            "id": "alpha", "ip": "10.0.0.1", "userAgent": "agent/1", "lastUpdateChecked": 1
        }))
        .expect("json");
        let app = App {
            workers: vec![worker],
            ..App::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(width, 20)).expect("terminal");
        terminal.draw(|f| render_ui(&app, f)).expect("draw");
        let buffer = terminal.backend().buffer();
        let line = |y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        let lines: Vec<String> = (0..20).map(line).collect();
        let find = |needle| {
            lines
                .iter()
                .find(|l| l.contains(needle))
                .cloned()
                .unwrap_or_default()
        };
        (find("IDENT"), find("alpha"))
    }

    #[test]
    fn worker_columns_align_at_every_width() {
        for (width, columns) in [
            (
                120,
                &[
                    ("IDENT", "alpha"),
                    ("IP", "10.0.0.1"),
                    ("AGENT", "agent/1"),
                    ("STATUS", "online"),
                ][..],
            ),
            (
                70,
                &[("IDENT", "alpha"), ("IP", "10.0.0.1"), ("STATUS", "online")][..],
            ),
            (40, &[("IDENT", "alpha"), ("STATUS", "online")][..]),
        ] {
            let (header, row) = rows_at(width);
            let column =
                |line: &str, needle: &str| line.find(needle).map(|at| line[..at].chars().count());
            for &(title, value) in columns {
                assert_eq!(
                    column(&header, title),
                    column(&row, value),
                    "{title} misaligned at {width}: {row}"
                );
            }
            assert_eq!(header.contains("CHECKED"), width >= 90);
            assert_eq!(row.contains("d ago"), width >= 90);
        }
    }
}
