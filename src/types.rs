use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize, Default, PartialEq)]
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

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct WorkerInfo {
    pub id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub jobs_completed: Option<usize>,
    pub connected_at: Option<u64>,
    pub last_seen_at: Option<u64>,
    pub connections: Option<usize>,
    pub status: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
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
    pub timestamp: Option<u64>,
    pub size_bytes: Option<u64>,
}

impl HistoryEntry {
    pub fn display_id(&self) -> String {
        self.queue_id
            .as_ref()
            .or(self.id.as_ref())
            .map(|v| match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => "-".to_string(),
            })
            .unwrap_or_else(|| "-".to_string())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SseInit {
    pub stats: Option<PrintStats>,
    pub workers: Option<Vec<WorkerInfo>>,
}
