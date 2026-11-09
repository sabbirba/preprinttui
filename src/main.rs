mod app;
mod crypto;
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
use std::{io, time::Duration};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::{
    app::{App, HttpCache, List, RefreshData, Tab, fetch_update, read_events},
    ui::{
        auth_hint_at, auth_modal_area, footer_key_at, inspector_max_scroll, inspector_modal_area,
        list_row_at, render_ui, tab_at,
    },
};

const REFRESH_INTERVAL_SECS: u64 = 10;
const TICK_RATE_MS: u64 = 50;
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
            app.step(app.inspecting, true);
            app.inspector_scroll = 0;
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.step(app.inspecting, false);
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
        KeyCode::Down => app.step(app.tab.list(), true),
        KeyCode::Up => app.step(app.tab.list(), false),
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
        KeyCode::Char('2') => select_tab(app, Tab::History),
        KeyCode::Char('3') => select_tab(app, Tab::Search),
        KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => select_tab(app, app.tab.next()),
        KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
            select_tab(app, app.tab.prev());
        }
        KeyCode::Enter | KeyCode::Char(' ' | 'v') => open_inspector(app, app.tab.list()),
        KeyCode::Down | KeyCode::Char('j') => app.step(app.tab.list(), true),
        KeyCode::Up | KeyCode::Char('k') => app.step(app.tab.list(), false),
        KeyCode::Char('/') => app.search_mode = true,
        KeyCode::Char('e' | 'c') => app.open_auth_modal(),
        KeyCode::Char('x') => clear_auth(app, tx),
        KeyCode::Char('r') => reload(app, tx),
        KeyCode::Char('a') => toggle_auto_refresh(app, tx),
        _ => {}
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
        let modal = inspector_modal_area(screen);
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

fn spawn_fetcher(initial: FetchReq, tx_data: UnboundedSender<RefreshData>) -> Requests {
    let (tx_req, mut rx_req) = mpsc::unbounded_channel::<FetchReq>();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .tcp_nodelay(true)
        .tcp_keepalive(Some(Duration::from_secs(60)))
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .pool_max_idle_per_host(8)
        .build()
        .unwrap_or_default();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
        let mut cache = HttpCache::default();
        let mut last = initial;

        loop {
            let fetch_now = tokio::select! {
                Some(req) = rx_req.recv() => {
                    if last.worker_key != req.worker_key || last.password != req.password {
                        cache = HttpCache::default();
                    }
                    last = req;
                    true
                }
                res = read_events(&client, last.worker_key.as_deref(), last.password.as_deref(), &tx_data, &mut cache) => {
                    if res.is_err() {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
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
    let tx_req = spawn_fetcher(FetchReq::from_app(&app), tx_data);
    request(&app, &tx_req);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let tick_rate = Duration::from_millis(TICK_RATE_MS);

    while !app.should_quit {
        app.tick_count = app.tick_count.wrapping_add(1);
        while let Ok(data) = rx_data.try_recv() {
            app.apply_data(data);
        }

        terminal.draw(|f| render_ui(&app, f))?;

        if event::poll(tick_rate)? {
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
