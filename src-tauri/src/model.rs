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
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub pet_enabled: bool,
    pub pet_auto_hide_ms: u64,
    pub pet_scale: f64,
    pub app_ui_scale: f64,
    pub animations_enabled: bool,
    pub privacy_mode: bool,
    pub notifications_enabled: bool,
    pub agent_notifications: bool,
    pub smart_alerts_enabled: bool,
    pub quota_threshold: u8,
    pub launch_at_startup: bool,
    pub check_updates_on_startup: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            pet_enabled: true,
            pet_auto_hide_ms: 7_500,
            pet_scale: 1.0,
            app_ui_scale: 1.0,
            animations_enabled: true,
            privacy_mode: false,
            notifications_enabled: true,
            agent_notifications: true,
            smart_alerts_enabled: true,
            quota_threshold: 85,
            launch_at_startup: false,
            check_updates_on_startup: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub available: bool,
    pub release_url: Option<String>,
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
