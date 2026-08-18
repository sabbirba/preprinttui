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
}

impl Tab {
    pub fn label(self, count: usize) -> String {
        match self {
            Self::Workers => format!(" Active Workers ({count}) [1] "),
            Self::History => format!(" Job History ({count}) [2] "),
        }
    }

    pub fn short_label(self, count: usize) -> String {
        match self {
            Self::Workers => format!(" Workers ({count}) [1] "),
            Self::History => format!(" History ({count}) [2] "),
        }
    }
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
        }
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
        self.is_loading = true;
        self.auth_mode = false;
    }

    pub fn clear_credentials(&mut self) {
        self.worker_key = None;
        self.password = None;
        self.input_worker_key.clear();
        self.input_password.clear();
        delete_saved_credentials();
        self.status_msg = "Cleared credentials".to_string();
    }

    pub fn realtime_uptime(&self) -> Option<u64> {
        let (anchor_instant, anchor_secs) = self.uptime_anchor?;
        Some(anchor_secs + anchor_instant.elapsed().as_secs())
    }

    pub fn apply_data(&mut self, data: RefreshData) {
        self.is_loading = false;

        if let Some(s) = data.stats {
            if let Some(server_uptime) = s.uptime_seconds {
                if let Some((anchor_instant, anchor_secs)) = self.uptime_anchor {
                    let local_calc = anchor_secs + anchor_instant.elapsed().as_secs();
                    if server_uptime.abs_diff(local_calc) > 2 {
                        self.uptime_anchor = Some((Instant::now(), server_uptime));
                    }
                } else {
                    self.uptime_anchor = Some((Instant::now(), server_uptime));
                }
            }
            self.stats = Some(s);
        }

        if let Some(w) = data.workers {
            self.workers = w;
            if !self.workers.is_empty() && self.selected_worker >= self.workers.len() {
                self.selected_worker = self.workers.len() - 1;
            }
        }

        if let Some(h) = data.history {
            self.history = h;
            let count = self.filtered_history().len();
            if count > 0 && self.selected_history >= count {
                self.selected_history = count - 1;
            }
        }

        self.is_unauthorized = data.is_unauthorized;
        self.last_refresh = Some(data.timestamp);

        self.status_msg = if self.is_unauthorized {
            "401 Unauthorized".to_string()
        } else {
            format!(
                "Updated {}",
                self.last_refresh
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_default()
            )
        };
    }

    pub fn filtered_history(&self) -> Vec<&HistoryEntry> {
        if self.search_query.is_empty() {
            return self.history.iter().collect();
        }
        let q = self.search_query.to_lowercase();
        self.history
            .iter()
            .filter(|h| {
                h.file_name
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
                    || h.student
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
            })
            .collect()
    }

    pub fn next_item(&mut self) {
        match self.tab {
            Tab::Workers => {
                if !self.workers.is_empty() {
                    self.selected_worker = (self.selected_worker + 1).min(self.workers.len() - 1);
                }
            }
            Tab::History => {
                let count = self.filtered_history().len();
                if count > 0 {
                    self.selected_history = (self.selected_history + 1).min(count - 1);
                }
            }
        }
    }

    pub fn prev_item(&mut self) {
        match self.tab {
            Tab::Workers => self.selected_worker = self.selected_worker.saturating_sub(1),
            Tab::History => self.selected_history = self.selected_history.saturating_sub(1),
        }
    }
}

fn get_creds_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|mut p| {
        p.push("preprinttui");
        p.push("credentials.json");
        p
    })
}

pub fn load_saved_credentials() -> (Option<String>, Option<String>) {
    let kr_key = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.trim().is_empty());

    let kr_pwd = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.trim().is_empty());

    if kr_key.is_some() || kr_pwd.is_some() {
        return (kr_key, kr_pwd);
    }

    if let Some(path) = get_creds_file_path()
        && let Ok(data) = std::fs::read_to_string(path)
        && let Ok(creds) = serde_json::from_str::<StoredCreds>(&data)
    {
        return (
            creds.worker_key.filter(|s| !s.trim().is_empty()),
            creds.password.filter(|s| !s.trim().is_empty()),
        );
    }

    (None, None)
}

pub fn save_credentials(worker_key: &Option<String>, password: &Option<String>) {
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY) {
        if let Some(k) = worker_key {
            let _ = entry.set_password(k);
        } else {
            let _ = entry.delete_credential();
        }
    }

    if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD) {
        if let Some(p) = password {
            let _ = entry.set_password(p);
        } else {
            let _ = entry.delete_credential();
        }
    }

    if let Some(path) = get_creds_file_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
            #[cfg(unix)]
            {
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        let stored = StoredCreds {
            worker_key: worker_key.clone(),
            password: password.clone(),
        };
        if let Ok(json) = serde_json::to_string(&stored) {
            let _ = std::fs::write(&path, json);
            #[cfg(unix)]
            {
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
}

pub fn delete_saved_credentials() {
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY) {
        let _ = entry.delete_credential();
    }
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD) {
        let _ = entry.delete_credential();
    }
    if let Some(path) = get_creds_file_path() {
        let _ = std::fs::remove_file(path);
    }
}

pub fn make_headers(worker_key: &Option<String>, password: &Option<String>) -> HeaderMap {
    let mut map = HeaderMap::new();
    if let Some(key) = worker_key {
        let jwt = make_jwt(key);
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {jwt}")) {
            map.insert("Authorization", v);
        }
        if let Ok(v) = HeaderValue::from_str(key) {
            map.insert("X-Worker-Key", v);
        }
        if let Ok(v) = HeaderValue::from_str("preprinttui/1.0") {
            map.insert("X-Worker-Ident", v);
        }
    } else if let Some(pwd) = password {
        let enc = STANDARD.encode(format!("{pwd}:{pwd}"));
        if let Ok(v) = HeaderValue::from_str(&format!("Basic {enc}")) {
            map.insert("Authorization", v);
        }
        if let Ok(v) = HeaderValue::from_str(pwd) {
            map.insert("X-Worker-Key", v.clone());
            map.insert("X-Print-Password", v);
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

    if let Ok(resp) = client
        .get(format!("{url}/print/stats?_t={ms}&ms={ms}"))
        .headers(hdrs.clone())
        .send()
        .await
    {
        if resp.status() == StatusCode::UNAUTHORIZED {
            unauth = true;
        } else if let Ok(s) = resp.json::<PrintStats>().await {
            stats = Some(s);
        }
    }

    match tab {
        Tab::Workers => {
            if let Ok(resp) = client
                .get(format!("{url}/print/active?_t={ms}"))
                .headers(hdrs.clone())
                .send()
                .await
            {
                if resp.status() == StatusCode::UNAUTHORIZED {
                    unauth = true;
                    workers = Some(Vec::new());
                } else if let Ok(w) = resp.json::<Vec<WorkerInfo>>().await {
                    workers = Some(w);
                }
            }
        }
        Tab::History => {
            if let Ok(resp) = client
                .get(format!("{url}/print/history?_t={ms}"))
                .headers(hdrs)
                .send()
                .await
            {
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
