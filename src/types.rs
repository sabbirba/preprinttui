use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct PrintStats {
    pub status: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub active_printers: Option<usize>,
    pub worker_seen: Option<u64>,
    pub queued_count: Option<usize>,
    pub queue_bytes: Option<u64>,
    pub claimed_count: Option<usize>,
    pub history_count: Option<usize>,
    pub jobs_released: Option<usize>,
    pub last_job_at: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct WorkerInfo {
    pub id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub jobs_completed: Option<usize>,
    pub age_seconds: Option<u64>,
    pub last_seen_seconds: Option<u64>,
    pub connected_at: Option<u64>,
    pub last_seen_at: Option<u64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct HistoryEntry {
    pub id: Option<serde_json::Value>,
    pub queue_id: Option<serde_json::Value>,
    pub file_name: Option<String>,
    pub student: Option<String>,
    pub hostname: Option<String>,
    pub client_ip: Option<String>,
    pub ip: Option<String>,
    pub queue: Option<String>,
    pub status: Option<String>,
    pub age_seconds: Option<u64>,
    pub timestamp: Option<u64>,
    pub size_bytes: Option<u64>,
}
