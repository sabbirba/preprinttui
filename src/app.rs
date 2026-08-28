use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Local};
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

#[derive(Copy, Clone, PartialEq, Eq)]
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
    pub latency_ms: Option<u64>,
    pub has_new_data: bool,
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
    pub latency_ms: Option<u64>,
    pub inspector_mode: bool,
    pub inspector_scroll: usize,
    pub flash_highlight: Option<Instant>,
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
            latency_ms: None,
            inspector_mode: false,
            inspector_scroll: 0,
            flash_highlight: None,
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
        self.auth_field = AuthField::Password;
        self.input_password = self.password.clone().unwrap_or_default();
    }

    pub fn commit_auth(&mut self) {
        self.worker_key = None;
        self.input_worker_key.clear();
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

        if let Some(w) = data.workers
            && self.workers != w
        {
            self.workers = w;
            if self.selected_worker >= self.filtered_workers().len() {
                self.selected_worker = self.filtered_workers().len().saturating_sub(1);
            }
        }

        if let Some(h) = data.history
            && self.history != h
        {
            self.history = h;
            if self.selected_history >= self.filtered_history().len() {
                self.selected_history = self.filtered_history().len().saturating_sub(1);
            }
        }

        self.latency_ms = data.latency_ms;
        if data.has_new_data {
            self.flash_highlight = Some(Instant::now());
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
                    || h.display_id().to_lowercase().contains(&q)
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

#[derive(Clone, Default)]
pub struct HttpCache {
    pub etag_stats: Option<String>,
    pub etag_workers: Option<String>,
    pub etag_history: Option<String>,
    pub stats: Option<PrintStats>,
    pub workers: Option<Vec<WorkerInfo>>,
    pub history: Option<Vec<HistoryEntry>>,
}

impl HttpCache {
    pub fn clear(&mut self) {
        self.etag_stats = None;
        self.etag_workers = None;
        self.etag_history = None;
        self.stats = None;
        self.workers = None;
        self.history = None;
    }
}

fn config_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        return home.join(".config").join("preprinttui").join("creds.json");
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("preprinttui")
        .join("creds.json")
}

fn load_saved_credentials() -> (Option<String>, Option<String>) {
    let mut key = None;
    let mut pwd = None;

    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY)
        && let Ok(k) = e.get_password()
        && !k.trim().is_empty()
    {
        key = Some(k);
    }

    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD)
        && let Ok(p) = e.get_password()
        && !p.trim().is_empty()
    {
        pwd = Some(p);
    }

    let p = config_path();
    if (key.is_none() || pwd.is_none())
        && let Ok(data) = std::fs::read_to_string(&p)
        && let Ok(creds) = serde_json::from_str::<StoredCreds>(&data)
    {
        if key.is_none() {
            key = creds.worker_key.filter(|s| !s.trim().is_empty());
        }
        if pwd.is_none() {
            pwd = creds.password.filter(|s| !s.trim().is_empty());
        }
    }

    (key, pwd)
}

fn save_credentials(worker_key: &Option<String>, password: &Option<String>) {
    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_WORKER_KEY) {
        match worker_key {
            Some(k) if !k.trim().is_empty() => {
                let _ = e.set_password(k);
            }
            _ => {
                let _ = e.delete_credential();
            }
        }
    }

    if let Ok(e) = Entry::new(KEYRING_SERVICE, KEYRING_PASSWORD) {
        match password {
            Some(p) if !p.trim().is_empty() => {
                let _ = e.set_password(p);
            }
            _ => {
                let _ = e.delete_credential();
            }
        }
    }

    let p = config_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if worker_key.is_some() || password.is_some() {
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
            && let Ok(v) =
                HeaderValue::from_str(&format!("Basic {}", STANDARD.encode(format!(":{pwd}"))))
        {
            map.insert("Authorization", v);
        }
        if let Ok(v) = HeaderValue::from_str(pwd) {
            map.insert("X-Print-Password", v);
        }
        if let Ok(v) = HeaderValue::from_str("preprinttui/1.0") {
            map.insert("X-Worker-Ident", v);
        }
    }
    map
}

async fn fetch_stats(
    client: &reqwest::Client,
    url: &str,
    hdrs: &HeaderMap,
    current_etag: Option<&str>,
    cached_stats: Option<&PrintStats>,
) -> (Option<String>, Option<PrintStats>, bool) {
    let mut req_hdrs = hdrs.clone();
    if let Some(etag) = current_etag
        && let Ok(val) = HeaderValue::from_str(etag)
    {
        req_hdrs.insert(reqwest::header::IF_NONE_MATCH, val);
    }

    if let Ok(resp) = client
        .get(format!("{url}/print/stats"))
        .headers(req_hdrs)
        .send()
        .await
    {
        if resp.status() == StatusCode::UNAUTHORIZED {
            return (None, None, true);
        }
        if resp.status() == StatusCode::NOT_MODIFIED {
            return (current_etag.map(String::from), cached_stats.cloned(), false);
        }
        if resp.status().is_success() {
            let mut new_etag = None;
            if let Some(etag_hdr) = resp.headers().get(reqwest::header::ETAG)
                && let Ok(etag_str) = etag_hdr.to_str()
            {
                new_etag = Some(etag_str.to_string());
            }
            if let Ok(s) = resp.json::<PrintStats>().await {
                return (new_etag, Some(s), false);
            }
        }
    }
    (current_etag.map(String::from), cached_stats.cloned(), false)
}

async fn fetch_workers(
    client: &reqwest::Client,
    url: &str,
    hdrs: &HeaderMap,
    current_etag: Option<&str>,
    cached_workers: Option<&[WorkerInfo]>,
) -> (Option<String>, Option<Vec<WorkerInfo>>, bool) {
    let mut req_hdrs = hdrs.clone();
    if let Some(etag) = current_etag
        && let Ok(val) = HeaderValue::from_str(etag)
    {
        req_hdrs.insert(reqwest::header::IF_NONE_MATCH, val);
    }

    if let Ok(resp) = client
        .get(format!("{url}/print/active"))
        .headers(req_hdrs)
        .send()
        .await
    {
        if resp.status() == StatusCode::UNAUTHORIZED {
            return (None, Some(Vec::new()), true);
        }
        if resp.status() == StatusCode::NOT_MODIFIED {
            return (
                current_etag.map(String::from),
                cached_workers.map(|v| v.to_vec()),
                false,
            );
        }
        if resp.status().is_success() {
            let mut new_etag = None;
            if let Some(etag_hdr) = resp.headers().get(reqwest::header::ETAG)
                && let Ok(etag_str) = etag_hdr.to_str()
            {
                new_etag = Some(etag_str.to_string());
            }
            if let Ok(w) = resp.json::<Vec<WorkerInfo>>().await {
                return (new_etag, Some(w), false);
            }
        }
    }
    (
        current_etag.map(String::from),
        cached_workers.map(|v| v.to_vec()),
        false,
    )
}

async fn fetch_history(
    client: &reqwest::Client,
    url: &str,
    hdrs: &HeaderMap,
    current_etag: Option<&str>,
    cached_history: Option<&[HistoryEntry]>,
) -> (Option<String>, Option<Vec<HistoryEntry>>, bool) {
    let mut req_hdrs = hdrs.clone();
    if let Some(etag) = current_etag
        && let Ok(val) = HeaderValue::from_str(etag)
    {
        req_hdrs.insert(reqwest::header::IF_NONE_MATCH, val);
    }

    if let Ok(resp) = client
        .get(format!("{url}/print/history"))
        .headers(req_hdrs)
        .send()
        .await
    {
        if resp.status() == StatusCode::UNAUTHORIZED {
            return (None, Some(Vec::new()), true);
        }
        if resp.status() == StatusCode::NOT_MODIFIED {
            return (
                current_etag.map(String::from),
                cached_history.map(|v| v.to_vec()),
                false,
            );
        }
        if resp.status().is_success() {
            let mut new_etag = None;
            if let Some(etag_hdr) = resp.headers().get(reqwest::header::ETAG)
                && let Ok(etag_str) = etag_hdr.to_str()
            {
                new_etag = Some(etag_str.to_string());
            }
            if let Ok(h) = resp.json::<Vec<HistoryEntry>>().await {
                return (new_etag, Some(h), false);
            }
        }
    }
    (
        current_etag.map(String::from),
        cached_history.map(|v| v.to_vec()),
        false,
    )
}

pub async fn fetch_update(
    client: &reqwest::Client,
    tab: Tab,
    worker_key: &Option<String>,
    password: &Option<String>,
    cache: &mut HttpCache,
) -> RefreshData {
    let start = Instant::now();
    let prev_history_len = cache.history.as_ref().map(|h| h.len()).unwrap_or(0);
    let prev_workers_len = cache.workers.as_ref().map(|w| w.len()).unwrap_or(0);
    let prev_last_job = cache.stats.as_ref().and_then(|s| s.last_job_at);
    let hdrs = make_headers(worker_key, password);
    let url = DEFAULT_URL.trim_end_matches('/');

    let (stats_etag, stats_data, stats_unauth) = fetch_stats(
        client,
        url,
        &hdrs,
        cache.etag_stats.as_deref(),
        cache.stats.as_ref(),
    )
    .await;
    cache.etag_stats = stats_etag;
    cache.stats = stats_data.clone();

    let history_changed = match (&stats_data, &cache.history) {
        (_, None) => true,
        (Some(s), Some(h)) => {
            s.history_count.unwrap_or(0) != h.len() || s.last_job_at != prev_last_job
        }
        _ => false,
    };

    let (workers, history, unauth) = match tab {
        Tab::Workers => {
            let (w_etag, w_data, w_unauth) = fetch_workers(
                client,
                url,
                &hdrs,
                cache.etag_workers.as_deref(),
                cache.workers.as_deref(),
            )
            .await;
            cache.etag_workers = w_etag;
            cache.workers = w_data.clone();
            (w_data, cache.history.clone(), stats_unauth || w_unauth)
        }
        Tab::History => {
            let (h_data, h_unauth) = if history_changed {
                let (h_etag, h_data, h_unauth) = fetch_history(
                    client,
                    url,
                    &hdrs,
                    cache.etag_history.as_deref(),
                    cache.history.as_deref(),
                )
                .await;
                cache.etag_history = h_etag;
                cache.history = h_data.clone();
                (h_data, h_unauth)
            } else {
                (cache.history.clone(), false)
            };
            (cache.workers.clone(), h_data, stats_unauth || h_unauth)
        }
        Tab::Search => {
            let (w_etag, w_data, w_unauth) = fetch_workers(
                client,
                url,
                &hdrs,
                cache.etag_workers.as_deref(),
                cache.workers.as_deref(),
            )
            .await;
            cache.etag_workers = w_etag;
            cache.workers = w_data.clone();

            let (h_data, h_unauth) = if history_changed {
                let (h_etag, h_data, h_unauth) = fetch_history(
                    client,
                    url,
                    &hdrs,
                    cache.etag_history.as_deref(),
                    cache.history.as_deref(),
                )
                .await;
                cache.etag_history = h_etag;
                cache.history = h_data.clone();
                (h_data, h_unauth)
            } else {
                (cache.history.clone(), false)
            };

            (w_data, h_data, stats_unauth || w_unauth || h_unauth)
        }
    };

    let latency_ms = Some(start.elapsed().as_millis() as u64);
    let has_new_data = cache.history.as_ref().map(|h| h.len()).unwrap_or(0) != prev_history_len
        || cache.workers.as_ref().map(|w| w.len()).unwrap_or(0) != prev_workers_len;

    RefreshData {
        stats: stats_data,
        workers,
        history,
        is_unauthorized: unauth,
        timestamp: Local::now(),
        latency_ms,
        has_new_data,
    }
}
