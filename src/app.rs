use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Local};
use keyring::Entry;
use reqwest::{
    Client, StatusCode,
    header::{ETAG, HeaderMap, HeaderName, HeaderValue, IF_NONE_MATCH},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{path::PathBuf, time::Instant};
use tokio::sync::mpsc::UnboundedSender;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::{
    consts::DEFAULT_URL,
    crypto::make_jwt,
    types::{HistoryEntry, PrintStats, SseInit, WorkerInfo},
};

const KEYRING_SERVICE: &str = "preprinttui";
const KEYRING_WORKER_KEY: &str = "worker_key";
const KEYRING_PASSWORD: &str = "password";

#[derive(Serialize, Deserialize, Default)]
struct StoredCreds {
    worker_key: Option<String>,
    password: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Workers,
    History,
    Search,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Workers => Self::History,
            Self::History => Self::Search,
            Self::Search => Self::Workers,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Workers => Self::Search,
            Self::History => Self::Workers,
            Self::Search => Self::History,
        }
    }
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

impl RefreshData {
    fn new(
        stats: Option<PrintStats>,
        workers: Option<Vec<WorkerInfo>>,
        history: Option<Vec<HistoryEntry>>,
        has_new_data: bool,
    ) -> Self {
        Self {
            stats,
            workers,
            history,
            is_unauthorized: false,
            timestamp: Local::now(),
            latency_ms: Some(0),
            has_new_data,
        }
    }

    fn unauthorized() -> Self {
        Self {
            is_unauthorized: true,
            latency_ms: None,
            ..Self::new(None, None, None, false)
        }
    }
}

#[derive(Default)]
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
    pub input_password: String,
    pub mask_credentials: bool,
    pub status_msg: String,
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

impl App {
    pub fn new() -> Self {
        let (worker_key, password) = load_saved_credentials();
        Self {
            worker_key,
            password,
            mask_credentials: true,
            status_msg: "Connecting...".to_string(),
            auto_refresh: true,
            is_loading: true,
            ..Self::default()
        }
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn active_toast(&self) -> Option<&str> {
        self.toast
            .as_ref()
            .filter(|(_, created)| created.elapsed().as_secs_f32() < 2.5)
            .map(|(msg, _)| msg.as_str())
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
        self.input_password = self.password.clone().unwrap_or_default();
    }

    pub fn commit_auth(&mut self) {
        let password = self.input_password.trim();
        self.worker_key = None;
        self.password = (!password.is_empty()).then(|| password.to_string());
        save_credentials(None, self.password.as_deref());
        self.auth_mode = false;
        self.notify("Saved");
    }

    pub fn clear_credentials(&mut self) {
        self.worker_key = None;
        self.password = None;
        self.input_password.clear();
        save_credentials(None, None);
        self.notify("Cleared");
    }

    fn notify(&mut self, msg: &str) {
        self.status_msg = msg.to_string();
        self.set_toast(msg);
        self.is_loading = true;
    }

    pub fn realtime_uptime(&self) -> Option<u64> {
        let (anchor, base) = self.uptime_anchor?;
        Some(base.saturating_add(anchor.elapsed().as_secs()))
    }

    pub fn apply_data(&mut self, data: RefreshData) {
        self.is_loading = false;
        self.is_unauthorized = data.is_unauthorized;

        if let Some(stats) = data.stats {
            let uptime = stats.uptime_seconds.unwrap_or(0);
            if self
                .uptime_anchor
                .is_none_or(|(_, base)| uptime < base || uptime.abs_diff(base) > 5)
            {
                self.uptime_anchor = Some((Instant::now(), uptime));
            }
            self.stats = Some(stats);
        }

        if let Some(workers) = data.workers
            && self.workers != workers
        {
            self.workers = workers;
            self.clamp_selection();
        }

        if let Some(history) = data.history
            && self.history != history
        {
            self.history = history;
            self.clamp_selection();
        }

        self.latency_ms = data.latency_ms;
        if data.has_new_data {
            self.flash_highlight = Some(Instant::now());
        }

        self.status_msg = if self.is_unauthorized {
            "Unauthorized".to_string()
        } else {
            data.timestamp.format("%H:%M:%S").to_string()
        };
    }

    pub fn filtered_workers(&self) -> Vec<&WorkerInfo> {
        let query = self.search_query.to_lowercase();
        self.workers
            .iter()
            .filter(|w| {
                matches_query(
                    &query,
                    &[w.id.as_deref(), w.ip.as_deref(), w.user_agent.as_deref()],
                )
            })
            .collect()
    }

    pub fn filtered_history(&self) -> Vec<&HistoryEntry> {
        let query = self.search_query.to_lowercase();
        self.history
            .iter()
            .filter(|h| {
                matches_query(
                    &query,
                    &[
                        h.student.as_deref(),
                        h.status.as_deref(),
                        h.hostname.as_deref(),
                        h.client_ip.as_deref(),
                        h.ip.as_deref(),
                        h.queue.as_deref(),
                        Some(&h.display_id()),
                    ],
                )
            })
            .collect()
    }

    fn list_len(&self) -> usize {
        match self.tab {
            Tab::Workers => self.filtered_workers().len(),
            Tab::History | Tab::Search => self.filtered_history().len(),
        }
    }

    fn selected(&self) -> usize {
        match self.tab {
            Tab::Workers => self.selected_worker,
            Tab::History | Tab::Search => self.selected_history,
        }
    }

    pub fn select(&mut self, index: usize) {
        if index < self.list_len() {
            match self.tab {
                Tab::Workers => self.selected_worker = index,
                Tab::History | Tab::Search => self.selected_history = index,
            }
        }
    }

    pub fn next_item(&mut self) {
        self.select(self.selected() + 1);
    }

    pub fn prev_item(&mut self) {
        self.select(self.selected().saturating_sub(1));
    }

    pub fn reset_selection(&mut self) {
        self.selected_worker = 0;
        self.selected_history = 0;
    }

    fn clamp_selection(&mut self) {
        let last_worker = self.filtered_workers().len().saturating_sub(1);
        let last_history = self.filtered_history().len().saturating_sub(1);
        self.selected_worker = self.selected_worker.min(last_worker);
        self.selected_history = self.selected_history.min(last_history);
    }
}

fn matches_query(query: &str, fields: &[Option<&str>]) -> bool {
    query.is_empty()
        || fields
            .iter()
            .flatten()
            .any(|field| field.to_lowercase().contains(query))
}

#[derive(Default)]
pub struct Cached<T> {
    etag: Option<String>,
    data: Option<T>,
}

impl<T: DeserializeOwned> Cached<T> {
    async fn refresh(&mut self, client: &Client, headers: &HeaderMap, path: &str) -> bool {
        let mut request = client
            .get(format!("{DEFAULT_URL}{path}"))
            .headers(headers.clone());
        if let Some(etag) = &self.etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let Ok(response) = request.send().await else {
            return false;
        };
        if response.status() == StatusCode::UNAUTHORIZED {
            self.etag = None;
            self.data = None;
            return true;
        }
        if response.status().is_success() {
            let etag = response
                .headers()
                .get(ETAG)
                .and_then(|value| value.to_str().ok())
                .map(String::from);
            if let Ok(data) = response.json().await {
                self.etag = etag;
                self.data = Some(data);
            }
        }
        false
    }
}

#[derive(Default)]
pub struct HttpCache {
    stats: Cached<PrintStats>,
    workers: Cached<Vec<WorkerInfo>>,
    history: Cached<Vec<HistoryEntry>>,
}

impl HttpCache {
    fn lengths(&self) -> (usize, usize) {
        (
            self.workers.data.as_ref().map_or(0, Vec::len),
            self.history.data.as_ref().map_or(0, Vec::len),
        )
    }

    fn last_job_at(&self) -> Option<u64> {
        self.stats.data.as_ref().and_then(|s| s.last_job_at)
    }

    fn history_is_stale(&self, previous_last_job: Option<u64>) -> bool {
        match (&self.stats.data, &self.history.data) {
            (_, None) => true,
            (Some(stats), Some(history)) => {
                stats.history_count.unwrap_or(0) != history.len()
                    || stats.last_job_at != previous_last_job
            }
            (None, Some(_)) => false,
        }
    }

    async fn refresh_history(&mut self, client: &Client, headers: &HeaderMap) -> bool {
        self.history
            .refresh(client, headers, "/print/history")
            .await
    }
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".config"))
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("preprinttui")
        .join("creds.json")
}

fn non_blank(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn load_secret(name: &str) -> Option<String> {
    non_blank(Entry::new(KEYRING_SERVICE, name).ok()?.get_password().ok())
}

fn store_secret(name: &str, value: Option<&str>) {
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, name) {
        let _ = match value.filter(|v| !v.trim().is_empty()) {
            Some(v) => entry.set_password(v),
            None => entry.delete_credential(),
        };
    }
}

fn load_saved_credentials() -> (Option<String>, Option<String>) {
    let stored: StoredCreds = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default();
    (
        load_secret(KEYRING_WORKER_KEY).or_else(|| non_blank(stored.worker_key)),
        load_secret(KEYRING_PASSWORD).or_else(|| non_blank(stored.password)),
    )
}

fn save_credentials(worker_key: Option<&str>, password: Option<&str>) {
    store_secret(KEYRING_WORKER_KEY, worker_key);
    store_secret(KEYRING_PASSWORD, password);

    let path = config_path();
    if worker_key.is_none() && password.is_none() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let creds = StoredCreds {
        worker_key: worker_key.map(String::from),
        password: password.map(String::from),
    };
    if let Ok(json) = serde_json::to_string(&creds)
        && std::fs::write(&path, json).is_ok()
    {
        #[cfg(unix)]
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
}

fn make_headers(worker_key: Option<&str>, password: Option<&str>) -> HeaderMap {
    let worker_key = worker_key.filter(|k| !k.trim().is_empty());
    let password = password.filter(|p| !p.trim().is_empty());
    let authorization = worker_key
        .map(|key| format!("Bearer {}", make_jwt(key)))
        .or_else(|| password.map(|pwd| format!("Basic {}", STANDARD.encode(format!(":{pwd}")))));

    [
        ("authorization", authorization),
        ("x-worker-key", worker_key.map(String::from)),
        ("x-print-password", password.map(String::from)),
        (
            "x-worker-ident",
            password.map(|_| "preprinttui/1.0".to_string()),
        ),
    ]
    .into_iter()
    .filter_map(|(name, value)| {
        Some((
            HeaderName::from_static(name),
            HeaderValue::from_str(&value?).ok()?,
        ))
    })
    .collect()
}

pub async fn fetch_update(
    client: &Client,
    tab: Tab,
    worker_key: Option<&str>,
    password: Option<&str>,
    cache: &mut HttpCache,
) -> RefreshData {
    let start = Instant::now();
    let previous_lengths = cache.lengths();
    let previous_last_job = cache.last_job_at();
    let headers = make_headers(worker_key, password);

    let mut unauthorized = cache.stats.refresh(client, &headers, "/print/stats").await;
    let history_stale = cache.history_is_stale(previous_last_job);
    if tab != Tab::History {
        unauthorized |= cache
            .workers
            .refresh(client, &headers, "/print/active")
            .await;
    }
    if tab != Tab::Workers && history_stale {
        unauthorized |= cache.refresh_history(client, &headers).await;
    }

    let (workers, history) = if unauthorized {
        (Some(Vec::new()), Some(Vec::new()))
    } else {
        (cache.workers.data.clone(), cache.history.data.clone())
    };
    let has_new_data = cache.lengths() != previous_lengths;

    RefreshData {
        is_unauthorized: unauthorized,
        latency_ms: Some(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)),
        ..RefreshData::new(cache.stats.data.clone(), workers, history, has_new_data)
    }
}

fn parse_event(message: &str) -> (&str, String) {
    let mut name = "message";
    let mut data = String::new();
    for line in message.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            name = rest.trim();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data.push_str(rest.trim());
        }
    }
    (name, data)
}

async fn handle_event(
    client: &Client,
    headers: &HeaderMap,
    cache: &mut HttpCache,
    message: &str,
) -> Option<RefreshData> {
    let (name, data) = parse_event(message);
    if data.is_empty() {
        return None;
    }

    match name {
        "init" => {
            let init: SseInit = serde_json::from_str(&data).ok()?;
            cache.stats.data.clone_from(&init.stats);
            cache.workers.data.clone_from(&init.workers);
            Some(RefreshData::new(init.stats, init.workers, None, true))
        }
        "stats" => {
            let stats: PrintStats = serde_json::from_str(&data).ok()?;
            let has_new_job = stats.last_job_at != cache.last_job_at();
            cache.stats.data = Some(stats.clone());
            let history = if has_new_job {
                cache.refresh_history(client, headers).await;
                cache.history.data.clone()
            } else {
                None
            };
            Some(RefreshData::new(Some(stats), None, history, has_new_job))
        }
        "workers" => {
            let workers: Vec<WorkerInfo> = serde_json::from_str(&data).ok()?;
            cache.workers.data = Some(workers.clone());
            Some(RefreshData::new(None, Some(workers), None, true))
        }
        "job_queued" | "job_claimed" => {
            cache.refresh_history(client, headers).await;
            Some(RefreshData::new(
                cache.stats.data.clone(),
                None,
                cache.history.data.clone(),
                true,
            ))
        }
        _ => None,
    }
}

pub async fn read_events(
    client: &Client,
    worker_key: Option<&str>,
    password: Option<&str>,
    tx_data: &UnboundedSender<RefreshData>,
    cache: &mut HttpCache,
) -> anyhow::Result<()> {
    let headers = make_headers(worker_key, password);
    let mut response = client
        .get(format!("{DEFAULT_URL}/print/events"))
        .headers(headers.clone())
        .send()
        .await?;

    if response.status() == StatusCode::UNAUTHORIZED {
        let _ = tx_data.send(RefreshData::unauthorized());
        return Ok(());
    }

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await? {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buffer.find("\n\n") {
            let message: String = buffer.drain(..end + 2).collect();
            if let Some(update) = handle_event(client, &headers, cache, &message).await {
                let _ = tx_data.send(update);
            }
        }
    }

    Ok(())
}
