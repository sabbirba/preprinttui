mod app;
mod types;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Position, Rect},
};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::{
    app::{App, HttpCache, List, RefreshData, Tab, fetch_update, lookup_ip, read_events},
    types::IpInfo,
    ui::{
        auth_hint_at, auth_modal_area, footer_key_at, inspector_max_scroll, inspector_modal_area,
        list_row_at, render_ui, tab_at,
    },
};

const REFRESH_INTERVAL_SECS: u64 = 10;
const TICK_RATE_MS: u64 = 16;
const CLOCK_REDRAW: Duration = Duration::from_secs(1);
const MIN_STREAM_RETRY: Duration = Duration::from_millis(500);
const MAX_STREAM_RETRY: Duration = Duration::from_secs(30);
const HEALTHY_STREAM: Duration = Duration::from_secs(5);
const HELP: &str = "Interactive TUI for PreConnect printer.

USAGE:
    preprinttui

FLAGS:
    -h, --help       Print help information
    -v, --version    Print version information";

struct FetchReq {
    worker_key: Option<String>,
    password: Option<String>,
    auto_refresh: bool,
}

impl FetchReq {
    fn from_app(app: &App) -> Self {
        Self {
            worker_key: app.worker_key.clone(),
            password: app.password.clone(),
            auto_refresh: app.auto_refresh,
        }
    }
}

type Requests = UnboundedSender<FetchReq>;

fn request(app: &App, tx: &Requests) {
    let _ = tx.send(FetchReq::from_app(app));
}

fn reload(app: &mut App, tx: &Requests) {
    app.set_toast("Refreshed");
    app.is_loading = true;
    request(app, tx);
}

fn save_auth(app: &mut App, tx: &Requests) {
    app.commit_auth();
    request(app, tx);
}

fn clear_auth(app: &mut App, tx: &Requests) {
    app.clear_credentials();
    request(app, tx);
}

fn toggle_auto_refresh(app: &mut App, tx: &Requests) {
    app.auto_refresh = !app.auto_refresh;
    app.set_toast(if app.auto_refresh { "Auto" } else { "Paused" });
    request(app, tx);
}

fn select_tab(app: &mut App, tab: Tab) {
    app.search_mode = tab == Tab::Search;
    app.tab = tab;
}

fn open_inspector(app: &mut App, list: List) {
    app.inspecting = list;
    app.inspector_mode = true;
    app.inspector_scroll = 0;
}

fn navigate(app: &mut App, code: KeyCode) {
    let list = app.tab.list();
    let page =
        isize::try_from(app.view(list).get().area.height.saturating_sub(4).max(1)).unwrap_or(1);
    match code {
        KeyCode::Down => app.move_by(list, 1),
        KeyCode::Up => app.move_by(list, -1),
        KeyCode::PageDown => app.move_by(list, page),
        KeyCode::PageUp => app.move_by(list, -page),
        KeyCode::Home => app.move_to(list, 0),
        KeyCode::End => app.move_to(list, usize::MAX),
        _ => {}
    }
}

fn handle_key(app: &mut App, key: KeyEvent, tx: &Requests) {
    if app.auth_mode {
        handle_auth_key(app, key.code, tx);
    } else if app.inspector_mode {
        handle_inspector_key(app, key.code);
    } else if app.search_mode {
        handle_search_key(app, key.code);
    } else {
        handle_normal_key(app, key, tx);
    }
}

fn handle_auth_key(app: &mut App, code: KeyCode, tx: &Requests) {
    match code {
        KeyCode::Esc => app.auth_mode = false,
        KeyCode::Tab => app.mask_credentials = !app.mask_credentials,
        KeyCode::Enter => save_auth(app, tx),
        KeyCode::Backspace => {
            app.input_password.pop();
        }
        KeyCode::Char(c) => app.input_password.push(c),
        _ => {}
    }
}

fn handle_inspector_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | ' ') => app.inspector_mode = false,
        KeyCode::Down | KeyCode::Char('j') => app.inspector_scroll += 1,
        KeyCode::Up | KeyCode::Char('k') => {
            app.inspector_scroll = app.inspector_scroll.saturating_sub(1);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.move_by(app.inspecting, 1);
            app.inspector_scroll = 0;
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.move_by(app.inspecting, -1);
            app.inspector_scroll = 0;
        }
        _ => {}
    }
}

fn handle_search_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => app.search_mode = false,
        KeyCode::Enter => open_inspector(app, app.tab.list()),
        KeyCode::Tab => select_tab(app, app.tab.next()),
        KeyCode::BackTab => select_tab(app, app.tab.prev()),
        KeyCode::Down
        | KeyCode::Up
        | KeyCode::PageDown
        | KeyCode::PageUp
        | KeyCode::Home
        | KeyCode::End => {
            navigate(app, code);
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.reset_selection();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.reset_selection();
        }
        _ => {}
    }
}

fn handle_normal_key(app: &mut App, key: KeyEvent, tx: &Requests) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Char('1') => select_tab(app, Tab::Workers),
        KeyCode::Char('2') => select_tab(app, Tab::Queue),
        KeyCode::Char('3') => select_tab(app, Tab::History),
        KeyCode::Char('4') => select_tab(app, Tab::Search),
        KeyCode::Char('g') => app.move_to(app.tab.list(), 0),
        KeyCode::Char('G') => app.move_to(app.tab.list(), usize::MAX),
        KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => select_tab(app, app.tab.next()),
        KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
            select_tab(app, app.tab.prev());
        }
        KeyCode::Enter | KeyCode::Char(' ' | 'v') => open_inspector(app, app.tab.list()),
        KeyCode::Char('j') => app.move_by(app.tab.list(), 1),
        KeyCode::Char('k') => app.move_by(app.tab.list(), -1),
        KeyCode::Char('/') => app.search_mode = true,
        KeyCode::Char('e' | 'c') => app.open_auth_modal(),
        KeyCode::Char('x') => clear_auth(app, tx),
        KeyCode::Char('r') => reload(app, tx),
        KeyCode::Char('a') => toggle_auto_refresh(app, tx),
        code => navigate(app, code),
    }
}

fn handle_click(app: &mut App, pos: Position, screen: Rect, tx: &Requests) {
    if app.auth_mode {
        match auth_hint_at(app, screen, pos) {
            Some("enter") => save_auth(app, tx),
            Some("tab") => app.mask_credentials = !app.mask_credentials,
            Some("esc") => app.auth_mode = false,
            _ if !auth_modal_area(screen).contains(pos) => app.auth_mode = false,
            _ => {}
        }
    } else if app.inspector_mode {
        let modal = inspector_modal_area(app, screen);
        if !modal.contains(pos) || pos.y == modal.y + modal.height.saturating_sub(2) {
            app.inspector_mode = false;
        }
    } else if pos.y == 0 {
        if let Some(tab) = tab_at(app, pos.x) {
            select_tab(app, tab);
        }
    } else if pos.y == screen.height.saturating_sub(1) {
        match footer_key_at(app, screen, pos.x) {
            Some("tab") => select_tab(app, app.tab.next()),
            Some("/") => app.search_mode = true,
            Some("e") => app.open_auth_modal(),
            Some("x") => clear_auth(app, tx),
            Some("r") => reload(app, tx),
            Some("q") => app.should_quit = true,
            _ => {}
        }
    } else if let Some((list, index)) = list_row_at(app, pos)
        && app.select(list, index)
    {
        open_inspector(app, list);
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .tcp_nodelay(true)
        .tcp_keepalive(Some(Duration::from_secs(60)))
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .pool_max_idle_per_host(8)
        .build()
        .unwrap_or_default()
}

fn spawn_fetcher(
    client: reqwest::Client,
    initial: FetchReq,
    tx_data: UnboundedSender<RefreshData>,
) -> Requests {
    let (tx_req, mut rx_req) = mpsc::unbounded_channel::<FetchReq>();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
        let mut cache = HttpCache::default();
        let mut last = initial;
        let mut stream_at = tokio::time::Instant::now();
        let mut backoff = MIN_STREAM_RETRY;

        loop {
            let fetch_now = tokio::select! {
                Some(req) = rx_req.recv() => {
                    if last.worker_key != req.worker_key || last.password != req.password {
                        cache = HttpCache::default();
                    }
                    last = req;
                    true
                }
                healthy = async {
                    tokio::time::sleep_until(stream_at).await;
                    let started = tokio::time::Instant::now();
                    let res = read_events(&client, last.worker_key.as_deref(), last.password.as_deref(), &tx_data, &mut cache).await;
                    res.is_ok() && started.elapsed() >= HEALTHY_STREAM
                } => {
                    backoff = if healthy { MIN_STREAM_RETRY } else { (backoff * 2).min(MAX_STREAM_RETRY) };
                    stream_at = tokio::time::Instant::now() + if healthy { Duration::ZERO } else { backoff };
                    false
                }
                _ = interval.tick() => last.auto_refresh,
            };

            if fetch_now {
                let data = fetch_update(
                    &client,
                    last.worker_key.as_deref(),
                    last.password.as_deref(),
                    &mut cache,
                )
                .await;
                let _ = tx_data.send(data);
            }
        }
    });

    tx_req
}

#[tokio::main]
async fn main() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-v" | "--version" => {
                println!("preprinttui {version}");
                return Ok(());
            }
            "-h" | "--help" => {
                println!("preprinttui {version}\n{HELP}");
                return Ok(());
            }
            _ => {}
        }
    }

    let mut app = App::new();
    let (tx_data, mut rx_data) = mpsc::unbounded_channel::<RefreshData>();
    let client = http_client();
    let tx_req = spawn_fetcher(client.clone(), FetchReq::from_app(&app), tx_data);
    let (tx_lookup, mut rx_lookup) = mpsc::unbounded_channel::<(String, Result<IpInfo, String>)>();
    request(&app, &tx_req);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let tick_rate = Duration::from_millis(TICK_RATE_MS);

    let mut dirty = true;
    let mut last_draw = Instant::now();
    while !app.should_quit {
        app.tick_count = app.tick_count.wrapping_add(1);
        while let Ok(data) = rx_data.try_recv() {
            app.apply_data(data);
            dirty = true;
        }
        while let Ok((ip, result)) = rx_lookup.try_recv() {
            app.ip_lookups.insert(ip, Some(result));
            dirty = true;
        }
        for ip in app.ips_to_preload() {
            app.ip_lookups.insert(ip.clone(), None);
            let (client, tx) = (client.clone(), tx_lookup.clone());
            tokio::spawn(async move {
                let result = lookup_ip(&client, &ip).await;
                let _ = tx.send((ip, result));
            });
        }

        if dirty || app.is_animating() || last_draw.elapsed() >= CLOCK_REDRAW {
            terminal.draw(|f| render_ui(&app, f))?;
            dirty = false;
            last_draw = Instant::now();
        }

        if event::poll(tick_rate)? {
            dirty = true;
            let size = terminal.size()?;
            let screen = Rect::new(0, 0, size.width, size.height);
            match event::read()? {
                Event::Key(key) => handle_key(&mut app, key, &tx_req),
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let pos = Position::new(mouse.column, mouse.row);
                        handle_click(&mut app, pos, screen, &tx_req);
                    }
                    MouseEventKind::ScrollDown if app.inspector_mode => app.inspector_scroll += 1,
                    MouseEventKind::ScrollUp if app.inspector_mode => {
                        app.inspector_scroll = app.inspector_scroll.saturating_sub(1);
                    }
                    _ => {}
                },
                _ => {}
            }
            app.inspector_scroll = app.inspector_scroll.min(inspector_max_scroll(&app, screen));
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
