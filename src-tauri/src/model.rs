use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub id: String,
    pub label: String,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub unit: String,
    pub reset_at: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub id: String,
    pub name: String,
    pub color: String,
    pub status: ProviderStatus,
    pub plan: Option<String>,
    pub metrics: Vec<Metric>,
    pub message: Option<String>,
    pub refreshed_at: Option<String>,
    pub experimental: bool,
    pub local: LocalPresence,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalPresence {
    pub detected: bool,
    pub session_count: usize,
    pub last_activity: Option<String>,
    pub source: Option<String>,
    pub usage: Option<LocalUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUsage {
    pub today_cost: f64,
    pub cost_30d: f64,
    pub tokens_30d: u64,
    pub latest_tokens: u64,
    pub top_model: Option<String>,
    pub daily: Vec<DailyUsage>,
    pub estimated_cost: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    pub date: String,
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Ready,
    NeedsLogin,
    NeedsApiKey,
    Refreshing,
    Error,
    Detected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub providers: Vec<ProviderSnapshot>,
    pub last_refresh: Option<String>,
    pub refreshing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    pub id: String,
    pub provider: String,
    pub workspace: String,
    pub status: AgentActivityStatus,
    pub progress: u8,
    pub message: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentActivityStatus {
    Working,
    NeedsApproval,
    Waiting,
    Completed,
    Error,
}

impl Metric {
    pub fn percent(id: &str, label: &str, used: f64, reset_at: Option<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            used: Some(used.clamp(0.0, 100.0)),
            limit: Some(100.0),
            unit: "%".into(),
            reset_at,
            detail: None,
        }
    }
}
