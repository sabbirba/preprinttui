mod app;
mod consts;
mod crypto;
mod types;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::mpsc;

use crate::{
    app::{App, RefreshData, Tab, fetch_update},
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
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    let initial_tab = app.tab;

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
        let mut last_req = FetchReq {
            tab: initial_tab,
            worker_key: None,
            password: None,
            auto_refresh: true,
        };

        loop {
            tokio::select! {
                Some(req) = rx_req.recv() => {
                    last_req = req;
                    let data = fetch_update(&client, last_req.tab, &last_req.worker_key, &last_req.password).await;
                    let _ = tx_data.send(data);
                }
                _ = interval.tick() => {
                    if last_req.auto_refresh {
                        let data = fetch_update(&client, last_req.tab, &last_req.worker_key, &last_req.password).await;
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

        if event::poll(tick_rate)?
            && let Event::Key(key) = event::read()?
        {
            if app.auth_mode {
                match key.code {
                    KeyCode::Esc => app.auth_mode = false,
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
            } else if app.search_mode {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        app.search_mode = false;
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
