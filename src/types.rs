use serde::{Deserialize, Deserializer, Serialize};

fn lenient_timestamp<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    Ok(
        match Option::<serde_json::Value>::deserialize(deserializer)? {
            Some(serde_json::Value::Number(n)) => n.as_u64(),
            Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
            _ => None,
        },
    )
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrintStats {
    pub status: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub active_printers: Option<usize>,
    pub queued_count: Option<usize>,
    pub history_count: Option<usize>,
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
    #[serde(default, deserialize_with = "lenient_timestamp")]
    pub last_update_checked: Option<u64>,
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
        match self.queue_id.as_ref().or(self.id.as_ref()) {
            Some(serde_json::Value::Number(n)) => n.to_string(),
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => "-".to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SseInit {
    pub stats: Option<PrintStats>,
    pub workers: Option<Vec<WorkerInfo>>,
}

#[derive(Clone, Deserialize)]
pub struct Student {
    #[serde(rename = "Student_ID")]
    pub id: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Dept")]
    pub dept: String,
}

#[derive(Deserialize)]
pub struct StudentFile {
    pub students: Vec<Student>,
}

#[derive(Clone, Deserialize, Default)]
pub struct IpInfo {
    #[serde(default)]
    pub success: bool,
    pub message: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub region_code: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub postal: Option<String>,
    pub calling_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    #[serde(default)]
    pub connection: IpConnection,
    #[serde(default)]
    pub timezone: IpTimezone,
}

#[derive(Clone, Deserialize, Default)]
pub struct IpConnection {
    pub asn: Option<u64>,
    pub org: Option<String>,
    pub isp: Option<String>,
    pub domain: Option<String>,
}

#[derive(Clone, Deserialize, Default)]
pub struct IpTimezone {
    pub id: Option<String>,
    pub utc: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::WorkerInfo;
    use serde_json::json;

    #[test]
    fn update_check_accepts_numbers_and_strings() {
        for (value, expected) in [
            (json!(1_700_000_000_000_u64), Some(1_700_000_000_000)),
            (json!("1700000000000"), Some(1_700_000_000_000)),
            (json!("soon"), None),
            (json!(null), None),
        ] {
            let worker: WorkerInfo =
                serde_json::from_value(json!({ "lastUpdateChecked": value })).expect("json");
            assert_eq!(worker.last_update_checked, expected);
        }
    }
}
