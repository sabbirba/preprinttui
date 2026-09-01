mod app;
mod consts;
mod crypto;
mod types;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::mpsc;

use crate::{
    app::{App, HttpCache, RefreshData, Tab, fetch_update, read_events},
    consts::{REFRESH_INTERVAL_SECS, TICK_RATE_MS},
    ui::render_ui,
};

struct FetchReq {
    tab: Tab,
    worker_key: Option<String>,
    password: Option<String>,
    auto_refresh: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-v" || a == "--version") {
        println!("preprinttui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "preprinttui {}\nInteractive TUI for PreConnect printer.\n\nUSAGE:\n    preprinttui\n\nFLAGS:\n    -h, --help       Print help information\n    -v, --version    Print version information",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }

    let mut app = App::new();

    let (tx_data, mut rx_data) = mpsc::unbounded_channel::<RefreshData>();
    let (tx_req, mut rx_req) = mpsc::unbounded_channel::<FetchReq>();

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .tcp_nodelay(true)
        .tcp_keepalive(Some(Duration::from_secs(60)))
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .pool_max_idle_per_host(8)
        .build()
        .unwrap_or_default();

    let initial_tab = app.tab;

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
        let mut http_cache = HttpCache::default();
        let mut last_req = FetchReq {
            tab: initial_tab,
            worker_key: None,
            password: None,
            auto_refresh: true,
        };

        loop {
            tokio::select! {
                Some(req) = rx_req.recv() => {
                    if last_req.worker_key != req.worker_key || last_req.password != req.password {
                        http_cache.clear();
                    }
                    last_req = req;
                    let data = fetch_update(&client, last_req.tab, &last_req.worker_key, &last_req.password, &mut http_cache).await;
                    let _ = tx_data.send(data);
                }
                res = read_events(&client, &last_req.worker_key, &last_req.password, &tx_data, &mut http_cache) => {
                    if res.is_err() {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
                _ = interval.tick() => {
                    if last_req.auto_refresh {
                        let data = fetch_update(&client, last_req.tab, &last_req.worker_key, &last_req.password, &mut http_cache).await;
                        let _ = tx_data.send(data);
                    }
                }
            }
        }
    });

    let _ = tx_req.send(FetchReq {
        tab: app.tab,
        worker_key: app.worker_key.clone(),
        password: app.password.clone(),
        auto_refresh: app.auto_refresh,
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let tick_rate = Duration::from_millis(TICK_RATE_MS);

    loop {
        app.tick_count = app.tick_count.wrapping_add(1);

        while let Ok(data) = rx_data.try_recv() {
            app.apply_data(data);
        }

        terminal.draw(|f| render_ui(&app, f))?;

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    if app.auth_mode {
                        match key.code {
                            KeyCode::Esc => app.auth_mode = false,
                            KeyCode::Tab => app.mask_credentials = !app.mask_credentials,
                            KeyCode::Enter => {
                                app.commit_auth();
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            }
                            KeyCode::Backspace => {
                                app.input_password.pop();
                            }
                            KeyCode::Char(c) => app.input_password.push(c),
                            _ => {}
                        }
                    } else if app.inspector_mode {
                        match key.code {
                            KeyCode::Esc
                            | KeyCode::Enter
                            | KeyCode::Char('q')
                            | KeyCode::Char(' ') => {
                                app.inspector_mode = false;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.inspector_scroll = app.inspector_scroll.saturating_add(1);
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.inspector_scroll = app.inspector_scroll.saturating_sub(1);
                            }
                            _ => {}
                        }
                    } else if app.search_mode {
                        match key.code {
                            KeyCode::Esc => {
                                app.search_mode = false;
                            }
                            KeyCode::Enter => {
                                app.inspector_mode = true;
                                app.inspector_scroll = 0;
                            }
                            KeyCode::Tab => {
                                app.tab = match app.tab {
                                    Tab::Workers => Tab::History,
                                    Tab::History => Tab::Search,
                                    Tab::Search => Tab::Workers,
                                };
                                app.search_mode = app.tab == Tab::Search;
                                app.is_loading = true;
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            }
                            KeyCode::BackTab => {
                                app.tab = match app.tab {
                                    Tab::Workers => Tab::Search,
                                    Tab::History => Tab::Workers,
                                    Tab::Search => Tab::History,
                                };
                                app.search_mode = app.tab == Tab::Search;
                                app.is_loading = true;
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            }
                            KeyCode::Down => app.next_item(),
                            KeyCode::Up => app.prev_item(),
                            KeyCode::Backspace => {
                                app.search_query.pop();
                                app.selected_worker = 0;
                                app.selected_history = 0;
                            }
                            KeyCode::Char(c) => {
                                app.search_query.push(c);
                                app.selected_worker = 0;
                                app.selected_history = 0;
                            }
                            _ => {}
                        }
                    } else if (key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c'))
                        || key.code == KeyCode::Char('q')
                    {
                        app.should_quit = true;
                    } else {
                        let prev_tab = app.tab;
                        match key.code {
                            KeyCode::Char('1') => app.tab = Tab::Workers,
                            KeyCode::Char('2') => app.tab = Tab::History,
                            KeyCode::Char('3') => app.tab = Tab::Search,
                            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('v') => {
                                app.inspector_mode = true;
                                app.inspector_scroll = 0;
                            }
                            KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                                app.tab = match app.tab {
                                    Tab::Workers => Tab::History,
                                    Tab::History => Tab::Search,
                                    Tab::Search => Tab::Workers,
                                };
                            }
                            KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
                                app.tab = match app.tab {
                                    Tab::Workers => Tab::Search,
                                    Tab::History => Tab::Workers,
                                    Tab::Search => Tab::History,
                                };
                            }
                            KeyCode::Char('j') | KeyCode::Down => app.next_item(),
                            KeyCode::Char('k') | KeyCode::Up => app.prev_item(),
                            KeyCode::Char('/') => {
                                app.search_mode = true;
                            }
                            KeyCode::Char('e') | KeyCode::Char('c') => {
                                app.open_auth_modal();
                            }
                            KeyCode::Char('x') => {
                                app.clear_credentials();
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: None,
                                    password: None,
                                    auto_refresh: app.auto_refresh,
                                });
                            }
                            KeyCode::Char('r') => {
                                app.is_loading = true;
                                app.set_toast("Refreshed");
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            }
                            KeyCode::Char('a') => {
                                app.auto_refresh = !app.auto_refresh;
                                app.set_toast(if app.auto_refresh { "Auto" } else { "Paused" });
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            }
                            _ => {}
                        }

                        if app.tab != prev_tab {
                            if app.tab == Tab::Search {
                                app.search_mode = true;
                            }
                            app.is_loading = true;
                            let _ = tx_req.send(FetchReq {
                                tab: app.tab,
                                worker_key: app.worker_key.clone(),
                                password: app.password.clone(),
                                auto_refresh: app.auto_refresh,
                            });
                        }
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollDown => {
                        if app.inspector_mode {
                            app.inspector_scroll = app.inspector_scroll.saturating_add(1);
                        } else {
                            app.next_item();
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        if app.inspector_mode {
                            app.inspector_scroll = app.inspector_scroll.saturating_sub(1);
                        } else {
                            app.prev_item();
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        let col = mouse.column;
                        let row = mouse.row;
                        let size = terminal.size()?;
                        let width = size.width;
                        let height = size.height;

                        if app.auth_mode {
                            let modal_width = 46.min(width.saturating_sub(2));
                            let modal_height = 7.min(height.saturating_sub(2));
                            let x = (width.saturating_sub(modal_width)) / 2;
                            let y = (height.saturating_sub(modal_height)) / 2;

                            if col < x
                                || col >= x + modal_width
                                || row < y
                                || row >= y + modal_height
                            {
                                app.auth_mode = false;
                            } else if row == y + modal_height.saturating_sub(2) {
                                let local_x = col.saturating_sub(x);
                                if (6..=16).contains(&local_x) {
                                    app.commit_auth();
                                    let _ = tx_req.send(FetchReq {
                                        tab: app.tab,
                                        worker_key: app.worker_key.clone(),
                                        password: app.password.clone(),
                                        auto_refresh: app.auto_refresh,
                                    });
                                } else if (17..=28).contains(&local_x) {
                                    app.mask_credentials = !app.mask_credentials;
                                } else if (29..=42).contains(&local_x) {
                                    app.auth_mode = false;
                                }
                            }
                        } else if app.inspector_mode {
                            let modal_width = 72.min(width.saturating_sub(4));
                            let modal_height = 22.min(height.saturating_sub(4));
                            let x = (width.saturating_sub(modal_width)) / 2;
                            let y = (height.saturating_sub(modal_height)) / 2;

                            if col < x
                                || col >= x + modal_width
                                || row < y
                                || row >= y + modal_height
                                || row == y + modal_height.saturating_sub(2)
                            {
                                app.inspector_mode = false;
                            }
                        } else if row <= 1 {
                            let w_end = if app.workers.is_empty() { 11 } else { 16 };
                            let h_end = w_end + if app.history.is_empty() { 11 } else { 16 };
                            let s_end = h_end + 10;

                            let prev_tab = app.tab;
                            if col < w_end {
                                app.tab = Tab::Workers;
                                app.search_mode = false;
                            } else if col < h_end {
                                app.tab = Tab::History;
                                app.search_mode = false;
                            } else if col < s_end {
                                app.tab = Tab::Search;
                                app.search_mode = true;
                            }

                            if prev_tab != app.tab {
                                app.is_loading = true;
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            }
                        } else if row == height.saturating_sub(1) {
                            if col < 14 {
                                app.tab = match app.tab {
                                    Tab::Workers => Tab::History,
                                    Tab::History => Tab::Search,
                                    Tab::Search => Tab::Workers,
                                };
                                app.search_mode = app.tab == Tab::Search;
                                app.is_loading = true;
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            } else if (20..=30).contains(&col) {
                                app.search_mode = true;
                            } else if (32..=42).contains(&col) {
                                app.open_auth_modal();
                            } else if (44..=54).contains(&col) {
                                app.clear_credentials();
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: None,
                                    password: None,
                                    auto_refresh: app.auto_refresh,
                                });
                            } else if (56..=68).contains(&col) {
                                app.is_loading = true;
                                app.set_toast("Refreshed");
                                let _ = tx_req.send(FetchReq {
                                    tab: app.tab,
                                    worker_key: app.worker_key.clone(),
                                    password: app.password.clone(),
                                    auto_refresh: app.auto_refresh,
                                });
                            } else if (70..=80).contains(&col) {
                                app.should_quit = true;
                            }
                        } else {
                            let is_compact = height < 18;
                            let header_offset = if is_compact { 1 } else { 2 };
                            let table_start = header_offset + 2;
                            if row >= table_start {
                                let item_idx = (row - table_start) as usize;
                                match app.tab {
                                    Tab::Workers => {
                                        let count = app.filtered_workers().len();
                                        if item_idx < count {
                                            app.selected_worker = item_idx;
                                        }
                                    }
                                    Tab::History | Tab::Search => {
                                        let count = app.filtered_history().len();
                                        if item_idx < count {
                                            app.selected_history = item_idx;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        if app.should_quit {
            break;
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
