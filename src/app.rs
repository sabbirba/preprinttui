use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Local, Utc};
use keyring::Entry;
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Instant};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::{
    consts::DEFAULT_URL,
    crypto::make_jwt,
    types::{HistoryEntry, PrintStats, WorkerInfo},
};

const KEYRING_SERVICE: &str = "preprinttui";
const KEYRING_WORKER_KEY: &str = "worker_key";
const KEYRING_PASSWORD: &str = "password";

#[derive(Serialize, Deserialize)]
struct StoredCreds {
    worker_key: Option<String>,
    password: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Tab {
    Workers = 0,
    History = 1,
    Search = 2,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum AuthField {
    WorkerKey = 0,
    Password = 1,
}

pub struct RefreshData {
    pub stats: Option<PrintStats>,
    pub workers: Option<Vec<WorkerInfo>>,
    pub history: Option<Vec<HistoryEntry>>,
    pub is_unauthorized: bool,
    pub timestamp: DateTime<Local>,
}

pub struct App {
    pub worker_key: Option<String>,
    pub password: Option<String>,
    pub tab: Tab,
    pub stats: Option<PrintStats>,
    pub uptime_anchor: Option<(Instant, u64)>,
    pub workers: Vec<WorkerInfo>,
    pub history: Vec<HistoryEntry>,
    pub selected_worker: usize,
    pub selected_history: usize,
    pub search_mode: bool,
    pub search_query: String,
    pub auth_mode: bool,
    pub auth_field: AuthField,
    pub input_worker_key: String,
    pub input_password: String,
    pub mask_credentials: bool,
    pub status_msg: String,
    pub last_refresh: Option<DateTime<Local>>,
    pub auto_refresh: bool,
    pub is_loading: bool,
    pub is_unauthorized: bool,
    pub should_quit: bool,
    pub tick_count: usize,
    pub toast: Option<(String, Instant)>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let (saved_key, saved_pwd) = load_saved_credentials();

        Self {
            worker_key: saved_key,
            password: saved_pwd,
            tab: Tab::Workers,
            stats: None,
            uptime_anchor: None,
            workers: Vec::new(),
            history: Vec::new(),
            selected_worker: 0,
            selected_history: 0,
            search_mode: false,
            search_query: String::new(),
            auth_mode: false,
            auth_field: AuthField::WorkerKey,
            input_worker_key: String::new(),
            input_password: String::new(),
            mask_credentials: true,
            status_msg: "Connecting...".to_string(),
            last_refresh: None,
            auto_refresh: true,
            is_loading: true,
            is_unauthorized: false,
            should_quit: false,
            tick_count: 0,
            toast: None,
        }
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn active_toast(&self) -> Option<&str> {
        if let Some((msg, created)) = &self.toast
            && created.elapsed().as_secs_f32() < 2.5
        {
            return Some(msg.as_str());
        }
        None
    }

    pub fn spinner(&self) -> &'static str {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[(self.tick_count / 2) % FRAMES.len()]
    }

    pub fn cursor_visible(&self) -> bool {
        (self.tick_count / 8).is_multiple_of(2)
    }

    pub fn open_auth_modal(&mut self) {
        self.auth_mode = true;
        self.auth_field = AuthField::WorkerKey;
        self.input_worker_key = self.worker_key.clone().unwrap_or_default();
        self.input_password = self.password.clone().unwrap_or_default();
    }

    pub fn commit_auth(&mut self) {
        self.worker_key = if self.input_worker_key.trim().is_empty() {
            None
        } else {
            Some(self.input_worker_key.trim().to_string())
        };
        self.password = if self.input_password.trim().is_empty() {
            None
        } else {
            Some(self.input_password.trim().to_string())
        };
        save_credentials(&self.worker_key, &self.password);
        self.auth_mode = false;
        self.status_msg = "Saved".to_string();
        self.set_toast("Saved");
        self.is_loading = true;
    }

    pub fn clear_credentials(&mut self) {
        self.worker_key = None;
        self.password = None;
        self.input_worker_key.clear();
        self.input_password.clear();
        delete_credentials();
        self.status_msg = "Cleared".to_string();
        self.set_toast("Cleared");
        self.is_loading = true;
    }

    pub fn realtime_uptime(&self) -> Option<u64> {
        let (anchor_time, base_uptime) = self.uptime_anchor?;
        let elapsed = anchor_time.elapsed().as_secs();
        Some(base_uptime.saturating_add(elapsed))
    }

    pub fn apply_data(&mut self, data: RefreshData) {
        self.is_loading = false;
        self.is_unauthorized = data.is_unauthorized;
        self.last_refresh = Some(data.timestamp);

        if let Some(stats) = data.stats {
            let new_uptime = stats.uptime_seconds.unwrap_or(0);
            match self.uptime_anchor {
                Some((_, current_base)) => {
                    let drift = (new_uptime as i64 - current_base as i64).abs();
                    if drift > 5 || new_uptime < current_base {
                        self.uptime_anchor = Some((Instant::now(), new_uptime));
                    }
                }
                None => {
                    self.uptime_anchor = Some((Instant::now(), new_uptime));
                }
            }
            self.stats = Some(stats);
        }

        if let Some(w) = data.workers {
            self.workers = w;
            if self.selected_worker >= self.filtered_workers().len() {
                self.selected_worker = self.filtered_workers().len().saturating_sub(1);
            }
        }

        if let Some(h) = data.history {
            self.history = h;
            if self.selected_history >= self.filtered_history().len() {
                self.selected_history = self.filtered_history().len().saturating_sub(1);
            }
        }

        self.status_msg = if self.is_unauthorized {
            "Unauthorized".to_string()
        } else {
            self.last_refresh
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_default()
        };
    }

    pub fn filtered_workers(&self) -> Vec<&WorkerInfo> {
        if self.search_query.is_empty() {
            return self.workers.iter().collect();
        }
        let q = self.search_query.to_lowercase();
        self.workers
            .iter()
            .filter(|w| {
                w.id.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    || w.ip.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    || w.user_agent
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
            })
            .collect()
    }

    pub fn filtered_history(&self) -> Vec<&HistoryEntry> {
        if self.search_query.is_empty() {
            return self.history.iter().collect();
        }
        let q = self.search_query.to_lowercase();
        self.history
            .iter()
            .filter(|h| {
                h.student
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
                    || h.status
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
                    || h.hostname
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
                    || h.client_ip
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
                    || h.ip.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    || h.queue.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    || h.queue_id
                        .as_ref()
                        .or(h.id.as_ref())
                        .map(|v| match v {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            _ => String::new(),
                        })
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&q)
            })
            .collect()
    }

    pub fn next_item(&mut self) {
        match self.tab {
            Tab::Workers => {
                let count = self.filtered_workers().len();
                if count > 0 {
                    self.selected_worker = (self.selected_worker + 1).min(count - 1);
                }
            }
            Tab::History | Tab::Search => {
                let count = self.filtered_history().len();
                if count > 0 {
                    self.selected_history = (self.selected_history + 1).min(count - 1);
                }
            }
        }
    }

    pub fn prev_item(&mut self) {
        match self.tab {
            Tab::Workers => {
                let count = self.filtered_workers().len();
                if count > 0 {
                    self.selected_worker = self.selected_worker.saturating_sub(1);
                }
            }
            Tab::History | Tab::Search => {
                let count = self.filtered_history().len();
                if count > 0 {
                    self.selected_history = self.selected_history.saturating_sub(1);
                }
            }
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("preprinttui")
        .join("creds.json")
}

fn load_saved_credentials() -> (Option<String>, Option<String>) {
    let k_key = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.trim().is_empty());
    let k_pwd = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.trim().is_empty());

    if k_key.is_some() || k_pwd.is_some() {
        return (k_key, k_pwd);
    }

    let p = config_path();
    if let Ok(data) = std::fs::read_to_string(&p)
        && let Ok(creds) = serde_json::from_str::<StoredCreds>(&data)
    {
        return (
            creds.worker_key.filter(|s| !s.trim().is_empty()),
            creds.password.filter(|s| !s.trim().is_empty()),
        );
    }

    (None, None)
}

fn save_credentials(worker_key: &Option<String>, password: &Option<String>) {
    let mut kr_ok = true;

    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY) {
        match worker_key {
            Some(k) if !k.trim().is_empty() => {
                if e.set_password(k).is_err() {
                    kr_ok = false;
                }
            }
            _ => {
                let _ = e.delete_credential();
            }
        }
    } else {
        kr_ok = false;
    }

    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD) {
        match password {
            Some(p) if !p.trim().is_empty() => {
                if e.set_password(p).is_err() {
                    kr_ok = false;
                }
            }
            _ => {
                let _ = e.delete_credential();
            }
        }
    } else {
        kr_ok = false;
    }

    let p = config_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if !kr_ok {
        let creds = StoredCreds {
            worker_key: worker_key.clone(),
            password: password.clone(),
        };
        if let Ok(json) = serde_json::to_string(&creds) {
            let _ = std::fs::write(&p, json);
            #[cfg(unix)]
            {
                let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
            }
        }
    } else if p.exists() {
        let _ = std::fs::remove_file(&p);
    }
}

fn delete_credentials() {
    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY) {
        let _ = e.delete_credential();
    }
    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD) {
        let _ = e.delete_credential();
    }
    let p = config_path();
    if p.exists() {
        let _ = std::fs::remove_file(&p);
    }
}

fn make_headers(worker_key: &Option<String>, password: &Option<String>) -> HeaderMap {
    let mut map = HeaderMap::new();
    if let Some(key) = worker_key
        && !key.trim().is_empty()
    {
        let jwt = make_jwt(key);
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {jwt}")) {
            map.insert("Authorization", v);
        }
        if let Ok(v) = HeaderValue::from_str(key) {
            map.insert("X-Worker-Key", v);
        }
    }
    if let Some(pwd) = password
        && !pwd.trim().is_empty()
    {
        if !map.contains_key("Authorization")
            && let Ok(v) = HeaderValue::from_str(&format!(
                "Basic {}",
                STANDARD.encode(format!("{pwd}:{pwd}"))
            ))
        {
            map.insert("Authorization", v);
        }
        if let Ok(v) = HeaderValue::from_str(pwd) {
            map.insert("X-Print-Password", v.clone());
            if !map.contains_key("X-Worker-Key") {
                map.insert("X-Worker-Key", v);
            }
        }
        if let Ok(v) = HeaderValue::from_str("preprinttui/1.0") {
            map.insert("X-Worker-Ident", v);
        }
    }
    map
}

pub async fn fetch_update(
    client: &reqwest::Client,
    tab: Tab,
    worker_key: &Option<String>,
    password: &Option<String>,
) -> RefreshData {
    let hdrs = make_headers(worker_key, password);
    let mut unauth = false;
    let mut stats = None;
    let mut workers = None;
    let mut history = None;
    let ms = Utc::now().timestamp_millis();
    let url = DEFAULT_URL.trim_end_matches('/');

    match tab {
        Tab::Workers => {
            let (stats_res, active_res) = tokio::join!(
                client
                    .get(format!("{url}/print/stats?_t={ms}&ms={ms}"))
                    .headers(hdrs.clone())
                    .send(),
                client
                    .get(format!("{url}/print/active?_t={ms}"))
                    .headers(hdrs)
                    .send(),
            );
            if let Ok(resp) = stats_res {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                } else if let Ok(s) = resp.json::<PrintStats>().await {
                    stats = Some(s);
                }
            }
            if let Ok(resp) = active_res {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                    workers = Some(Vec::new());
                } else if let Ok(w) = resp.json::<Vec<WorkerInfo>>().await {
                    workers = Some(w);
                }
            }
        }
        Tab::History => {
            let (stats_res, history_res) = tokio::join!(
                client
                    .get(format!("{url}/print/stats?_t={ms}&ms={ms}"))
                    .headers(hdrs.clone())
                    .send(),
                client
                    .get(format!("{url}/print/history?_t={ms}"))
                    .headers(hdrs)
                    .send(),
            );
            if let Ok(resp) = stats_res {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                } else if let Ok(s) = resp.json::<PrintStats>().await {
                    stats = Some(s);
                }
            }
            if let Ok(resp) = history_res {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                    history = Some(Vec::new());
                } else if let Ok(h) = resp.json::<Vec<HistoryEntry>>().await {
                    history = Some(h);
                }
            }
        }
        Tab::Search => {
            let (stats_res, active_res, history_res) = tokio::join!(
                client
                    .get(format!("{url}/print/stats?_t={ms}&ms={ms}"))
                    .headers(hdrs.clone())
                    .send(),
                client
                    .get(format!("{url}/print/active?_t={ms}"))
                    .headers(hdrs.clone())
                    .send(),
                client
                    .get(format!("{url}/print/history?_t={ms}"))
                    .headers(hdrs)
                    .send(),
            );
            if let Ok(resp) = stats_res {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                } else if let Ok(s) = resp.json::<PrintStats>().await {
                    stats = Some(s);
                }
            }
            if let Ok(resp) = active_res {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                    workers = Some(Vec::new());
                } else if let Ok(w) = resp.json::<Vec<WorkerInfo>>().await {
                    workers = Some(w);
                }
            }
            if let Ok(resp) = history_res {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                    history = Some(Vec::new());
                } else if let Ok(h) = resp.json::<Vec<HistoryEntry>>().await {
                    history = Some(h);
                }
            }
        }
    }

    RefreshData {
        stats,
        workers,
        history,
        is_unauthorized: unauth,
        timestamp: Local::now(),
    }
}
