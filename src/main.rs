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
    app::{App, HttpCache, RefreshData, Tab, fetch_update, read_events},
    ui::{auth_modal_area, inspector_modal_area, render_ui},
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
    tab: Tab,
    worker_key: Option<String>,
    password: Option<String>,
    auto_refresh: bool,
}

impl FetchReq {
    fn from_app(app: &App) -> Self {
        Self {
            tab: app.tab,
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

fn refresh(app: &mut App, tx: &Requests) {
    app.is_loading = true;
    request(app, tx);
}

fn reload(app: &mut App, tx: &Requests) {
    app.set_toast("Refreshed");
    refresh(app, tx);
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

fn select_tab(app: &mut App, tab: Tab, tx: &Requests) {
    app.search_mode = tab == Tab::Search;
    if app.tab != tab {
        app.tab = tab;
        refresh(app, tx);
    }
}

fn open_inspector(app: &mut App) {
    app.inspector_mode = true;
    app.inspector_scroll = 0;
}

fn handle_key(app: &mut App, key: KeyEvent, tx: &Requests) {
    if app.auth_mode {
        handle_auth_key(app, key.code, tx);
    } else if app.inspector_mode {
        handle_inspector_key(app, key.code);
    } else if app.search_mode {
        handle_search_key(app, key.code, tx);
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
            app.next_item();
            app.inspector_scroll = 0;
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.prev_item();
            app.inspector_scroll = 0;
        }
        _ => {}
    }
}

fn handle_search_key(app: &mut App, code: KeyCode, tx: &Requests) {
    match code {
        KeyCode::Esc => app.search_mode = false,
        KeyCode::Enter => open_inspector(app),
        KeyCode::Tab => select_tab(app, app.tab.next(), tx),
        KeyCode::BackTab => select_tab(app, app.tab.prev(), tx),
        KeyCode::Down => app.next_item(),
        KeyCode::Up => app.prev_item(),
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
        KeyCode::Char('1') => select_tab(app, Tab::Workers, tx),
        KeyCode::Char('2') => select_tab(app, Tab::History, tx),
        KeyCode::Char('3') => select_tab(app, Tab::Search, tx),
        KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => select_tab(app, app.tab.next(), tx),
        KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
            select_tab(app, app.tab.prev(), tx);
        }
        KeyCode::Enter | KeyCode::Char(' ' | 'v') => open_inspector(app),
        KeyCode::Down | KeyCode::Char('j') => app.next_item(),
        KeyCode::Up | KeyCode::Char('k') => app.prev_item(),
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
        click_auth_modal(app, pos, auth_modal_area(screen), tx);
    } else if app.inspector_mode {
        let modal = inspector_modal_area(screen);
        if !modal.contains(pos) || pos.y == modal.y + modal.height.saturating_sub(2) {
            app.inspector_mode = false;
        }
    } else if pos.y <= 1 {
        click_tab_bar(app, pos.x, tx);
    } else if pos.y == screen.height.saturating_sub(1) {
        click_footer(app, pos.x, tx);
    } else {
        let table_start = if screen.height < 18 { 3 } else { 4 };
        if let Some(index) = pos.y.checked_sub(table_start)
            && app.select(usize::from(index))
        {
            open_inspector(app);
        }
    }
}

fn click_auth_modal(app: &mut App, pos: Position, modal: Rect, tx: &Requests) {
    if !modal.contains(pos) {
        app.auth_mode = false;
    } else if pos.y == modal.y + modal.height.saturating_sub(2) {
        match pos.x - modal.x {
            6..=16 => save_auth(app, tx),
            17..=28 => app.mask_credentials = !app.mask_credentials,
            29..=42 => app.auth_mode = false,
            _ => {}
        }
    }
}

fn click_tab_bar(app: &mut App, x: u16, tx: &Requests) {
    let pill_width = |empty: bool| if empty { 11 } else { 16 };
    let workers_end = pill_width(app.workers.is_empty());
    let history_end = workers_end + pill_width(app.history.is_empty());
    let tab = if x < workers_end {
        Tab::Workers
    } else if x < history_end {
        Tab::History
    } else if x < history_end + 10 {
        Tab::Search
    } else {
        return;
    };
    select_tab(app, tab, tx);
}

fn click_footer(app: &mut App, x: u16, tx: &Requests) {
    match x {
        0..14 => select_tab(app, app.tab.next(), tx),
        20..=30 => app.search_mode = true,
        32..=42 => app.open_auth_modal(),
        44..=54 => clear_auth(app, tx),
        56..=68 => reload(app, tx),
        70..=80 => app.should_quit = true,
        _ => {}
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
                    last.tab,
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
            match event::read()? {
                Event::Key(key) => handle_key(&mut app, key, &tx_req),
                Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                    let size = terminal.size()?;
                    let screen = Rect::new(0, 0, size.width, size.height);
                    let pos = Position::new(mouse.column, mouse.row);
                    handle_click(&mut app, pos, screen, &tx_req);
                }
                _ => {}
            }
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
