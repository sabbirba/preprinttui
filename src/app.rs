use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Local};
use keyring::Entry;
use ratatui::layout::Rect;
use reqwest::{
    Client, StatusCode,
    header::{ETAG, HeaderMap, HeaderName, HeaderValue, IF_NONE_MATCH},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{cell::Cell, collections::HashMap, net::IpAddr, path::PathBuf, time::Instant};
use tokio::sync::mpsc::UnboundedSender;

use crate::types::{HistoryEntry, IpInfo, PrintStats, SseInit, WorkerInfo};

const DEFAULT_URL: &str = "https://api.preconnect.app";
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
    Queue,
    History,
    Search,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Workers => Self::Queue,
            Self::Queue => Self::History,
            Self::History => Self::Search,
            Self::Search => Self::Workers,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Workers => Self::Search,
            Self::Queue => Self::Workers,
            Self::History => Self::Queue,
            Self::Search => Self::History,
        }
    }
}

impl Tab {
    pub fn list(self) -> List {
        match self {
            Self::Workers => List::Workers,
            Self::Queue => List::Queue,
            Self::History | Self::Search => List::History,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub enum List {
    #[default]
    Workers,
    Queue,
    History,
}

pub const LISTS: [List; 3] = [List::Workers, List::Queue, List::History];

#[derive(Copy, Clone, Default)]
pub struct View {
    pub area: Rect,
    pub offset: usize,
}

pub struct RefreshData {
    pub stats: Option<PrintStats>,
    pub workers: Option<Vec<WorkerInfo>>,
    pub history: Option<Vec<HistoryEntry>>,
    pub queue: Option<Vec<HistoryEntry>>,
    pub is_unauthorized: bool,
    pub timestamp: DateTime<Local>,
    pub latency_ms: Option<u64>,
}

impl RefreshData {
    fn new(
        stats: Option<PrintStats>,
        workers: Option<Vec<WorkerInfo>>,
        history: Option<Vec<HistoryEntry>>,
    ) -> Self {
        Self {
            stats,
            workers,
            history,
            queue: None,
            is_unauthorized: false,
            timestamp: Local::now(),
            latency_ms: None,
        }
    }

    fn unauthorized() -> Self {
        Self {
            is_unauthorized: true,
            ..Self::new(None, None, None)
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
    pub queue: Vec<HistoryEntry>,
    pub selected: [usize; 3],
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
    pub inspecting: List,
    pub views: [Cell<View>; 3],
    pub ip_lookups: HashMap<String, Option<Result<IpInfo, String>>>,
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

    pub fn is_animating(&self) -> bool {
        self.is_loading || self.stats.is_none() || self.search_mode || self.active_toast().is_some()
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

        if let Some(queue) = data.queue
            && self.queue != queue
        {
            self.queue = queue;
            self.clamp_selection();
        }

        if data.latency_ms.is_some() {
            self.latency_ms = data.latency_ms;
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
                query.is_empty()
                    || matches_query(
                        &query,
                        &[w.id.as_deref(), w.ip.as_deref(), w.user_agent.as_deref()],
                    )
            })
            .collect()
    }

    pub fn filtered_history(&self) -> Vec<&HistoryEntry> {
        self.filter_jobs(&self.history)
    }

    pub fn filtered_queue(&self) -> Vec<&HistoryEntry> {
        self.filter_jobs(&self.queue)
    }

    fn filter_jobs<'a>(&self, jobs: &'a [HistoryEntry]) -> Vec<&'a HistoryEntry> {
        let query = self.search_query.to_lowercase();
        jobs.iter()
            .filter(|h| {
                query.is_empty()
                    || matches_query(
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

    pub fn len(&self, list: List) -> usize {
        match list {
            List::Workers => self.filtered_workers().len(),
            List::Queue => self.filtered_queue().len(),
            List::History => self.filtered_history().len(),
        }
    }

    pub fn view(&self, list: List) -> &Cell<View> {
        &self.views[list as usize]
    }

    pub fn ip_at(&self, list: List, index: usize) -> Option<String> {
        let ip = match list {
            List::Workers => self.filtered_workers().get(index)?.ip.clone(),
            List::Queue => self.filtered_queue().get(index)?.client_ip.clone(),
            List::History => self.filtered_history().get(index)?.client_ip.clone(),
        }?;
        ip.parse::<IpAddr>().is_ok().then_some(ip)
    }

    pub fn inspected_ip(&self) -> Option<String> {
        self.ip_at(self.inspecting, self.selected[self.inspecting as usize])
    }

    pub fn ips_to_preload(&self) -> Vec<String> {
        let list = if self.inspector_mode {
            self.inspecting
        } else {
            self.tab.list()
        };
        let selected = self.selected[list as usize];
        (selected.saturating_sub(1)..=selected + 2)
            .filter_map(|index| self.ip_at(list, index))
            .filter(|ip| !self.ip_lookups.contains_key(ip))
            .collect()
    }

    pub fn move_to(&mut self, list: List, index: usize) {
        let last = self.len(list).saturating_sub(1);
        self.selected[list as usize] = index.min(last);
    }

    pub fn select(&mut self, list: List, index: usize) -> bool {
        let valid = index < self.len(list);
        if valid {
            self.selected[list as usize] = index;
        }
        valid
    }

    pub fn move_by(&mut self, list: List, delta: isize) {
        let current = self.selected[list as usize];
        self.move_to(list, current.saturating_add_signed(delta));
    }

    pub fn reset_selection(&mut self) {
        self.selected = [0; 3];
    }

    fn clamp_selection(&mut self) {
        for list in LISTS {
            self.move_to(list, self.selected[list as usize]);
        }
    }
}

fn matches_query(query: &str, fields: &[Option<&str>]) -> bool {
    fields
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
    queue: Cached<Vec<HistoryEntry>>,
}

impl HttpCache {
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

    fn apply_job(&mut self, event: &str, mut job: HistoryEntry) {
        let id = job.display_id();
        let history = self.history.data.get_or_insert_with(Vec::new);
        let queue = self.queue.data.get_or_insert_with(Vec::new);
        queue.retain(|entry| entry.display_id() != id);
        if event == "job_queued" {
            job.status = Some("queued".to_string());
            job.queue_id = job.id.take();
            history.retain(|entry| entry.display_id() != id);
            history.insert(0, job.clone());
            queue.push(job);
        } else if let Some(entry) = history.iter_mut().find(|entry| entry.display_id() == id) {
            entry.status = Some(event.trim_start_matches("job_").to_string());
        }
    }

    async fn refresh_queue(&mut self, client: &Client, headers: &HeaderMap) -> bool {
        self.queue.refresh(client, headers, "/print/queue").await
    }
}

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

pub async fn lookup_ip(client: &Client, ip: &str) -> Result<IpInfo, String> {
    if ip.parse::<IpAddr>().is_ok_and(is_private) {
        return Err("private network address".to_string());
    }
    let info: IpInfo = client
        .get(format!("https://ipwho.is/{ip}"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if info.success {
        Ok(info)
    } else {
        Err(info.message.unwrap_or_else(|| "lookup failed".to_string()))
    }
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".config"))
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("preprinttui")
}

fn legacy_creds_path() -> PathBuf {
    config_dir().join("creds.json")
}

fn non_blank(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn load_secret(name: &str) -> Option<String> {
    non_blank(Entry::new(KEYRING_SERVICE, name).ok()?.get_password().ok())
}

fn store_secret(name: &str, value: Option<&str>) -> bool {
    let Ok(entry) = Entry::new(KEYRING_SERVICE, name) else {
        return false;
    };
    match value.filter(|v| !v.trim().is_empty()) {
        Some(v) => entry.set_password(v).is_ok() && load_secret(name).as_deref() == Some(v),
        None => {
            let _ = entry.delete_credential();
            load_secret(name).is_none()
        }
    }
}

fn load_saved_credentials() -> (Option<String>, Option<String>) {
    let legacy = std::fs::read_to_string(legacy_creds_path()).ok();
    let stored: StoredCreds = legacy
        .as_deref()
        .and_then(|data| serde_json::from_str(data).ok())
        .unwrap_or_default();
    let creds = (
        load_secret(KEYRING_WORKER_KEY).or_else(|| non_blank(stored.worker_key)),
        load_secret(KEYRING_PASSWORD).or_else(|| non_blank(stored.password)),
    );
    if legacy.is_some() {
        save_credentials(creds.0.as_deref(), creds.1.as_deref());
    }
    creds
}

fn save_credentials(worker_key: Option<&str>, password: Option<&str>) {
    let in_keychain =
        store_secret(KEYRING_WORKER_KEY, worker_key) & store_secret(KEYRING_PASSWORD, password);
    let path = legacy_creds_path();
    if in_keychain || (worker_key.is_none() && password.is_none()) {
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
    let basic = password
        .filter(|p| !p.trim().is_empty())
        .map(|pwd| format!("Basic {}", STANDARD.encode(format!(":{pwd}"))));
    [
        ("x-worker-key", worker_key.map(String::from)),
        ("authorization", basic),
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
    worker_key: Option<&str>,
    password: Option<&str>,
    cache: &mut HttpCache,
) -> RefreshData {
    let start = Instant::now();
    let previous_last_job = cache.last_job_at();
    let headers = make_headers(worker_key, password);

    let mut unauthorized = cache.stats.refresh(client, &headers, "/print/stats").await;
    let history_stale = cache.history_is_stale(previous_last_job);
    unauthorized |= cache
        .workers
        .refresh(client, &headers, "/print/active")
        .await;
    if history_stale {
        unauthorized |= cache.refresh_history(client, &headers).await;
    }
    unauthorized |= cache.refresh_queue(client, &headers).await;

    let (workers, history, queue) = if unauthorized {
        (Some(Vec::new()), Some(Vec::new()), Some(Vec::new()))
    } else {
        (
            cache.workers.data.clone(),
            cache.history.data.clone(),
            cache.queue.data.clone(),
        )
    };

    RefreshData {
        is_unauthorized: unauthorized,
        latency_ms: Some(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)),
        queue,
        ..RefreshData::new(cache.stats.data.clone(), workers, history)
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

fn handle_event(cache: &mut HttpCache, message: &str) -> Option<RefreshData> {
    let (name, data) = parse_event(message);
    if data.is_empty() {
        return None;
    }

    match name {
        "init" => {
            let init: SseInit = serde_json::from_str(&data).ok()?;
            cache.stats.data.clone_from(&init.stats);
            cache.workers.data.clone_from(&init.workers);
            Some(RefreshData::new(init.stats, init.workers, None))
        }
        "stats" => {
            let stats: PrintStats = serde_json::from_str(&data).ok()?;
            cache.stats.data = Some(stats.clone());
            Some(RefreshData::new(Some(stats), None, None))
        }
        "workers" => {
            let workers: Vec<WorkerInfo> = serde_json::from_str(&data).ok()?;
            cache.workers.data = Some(workers.clone());
            Some(RefreshData::new(None, Some(workers), None))
        }
        "job_queued" | "job_claimed" | "job_printed" | "job_failed" => {
            cache.apply_job(name, serde_json::from_str(&data).ok()?);
            Some(RefreshData {
                queue: cache.queue.data.clone(),
                ..RefreshData::new(None, None, cache.history.data.clone())
            })
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
        .headers(headers)
        .send()
        .await?;

    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        let _ = tx_data.send(RefreshData::unauthorized());
    }
    if !status.is_success() {
        anyhow::bail!("events stream returned {status}");
    }

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await? {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buffer.find("\n\n") {
            let message: String = buffer.drain(..end + 2).collect();
            if let Some(update) = handle_event(cache, &message) {
                let _ = tx_data.send(update);
            }
        }
    }

    Ok(())
}
