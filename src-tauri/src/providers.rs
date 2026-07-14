use std::{env, fs, path::PathBuf, process::Command};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use chrono::Utc;
use reqwest::{Client, StatusCode};
use rusqlite::Connection;
use serde_json::Value;

use crate::{
    model::{Metric, ProviderSnapshot, ProviderStatus},
    scanner,
};

const SERVICE: &str = "codeusage";

pub fn registry() -> Vec<ProviderSnapshot> {
    vec![
        provider(
            "antigravity",
            "Antigravity",
            "#5f7cff",
            true,
            detect_antigravity(),
            "Mở Antigravity và đăng nhập để đọc pool Gemini/Claude.",
        ),
        provider(
            "claude",
            "Claude",
            "#d97757",
            false,
            claude_auth_path().is_some(),
            "Chạy `claude` và đăng nhập.",
        ),
        provider(
            "codex",
            "Codex",
            "#10a37f",
            false,
            codex_auth_path().is_some(),
            "Chạy `codex` và đăng nhập bằng tài khoản ChatGPT.",
        ),
        provider(
            "copilot",
            "Copilot",
            "#8b5cf6",
            true,
            command_exists("gh"),
            "Cài GitHub CLI rồi chạy `gh auth login`.",
        ),
        provider(
            "cursor",
            "Cursor",
            "#4f4741",
            true,
            cursor_state_path().is_some(),
            "Mở Cursor và đăng nhập.",
        ),
        provider(
            "devin",
            "Devin",
            "#7dd3fc",
            true,
            devin_auth_path().is_some(),
            "Chạy `devin auth login`.",
        ),
        provider(
            "grok",
            "Grok",
            "#e5e7eb",
            true,
            grok_auth_path().is_some(),
            "Chạy `grok login`.",
        ),
        provider(
            "opencode",
            "OpenCode",
            "#fbbf24",
            true,
            opencode_auth_path().is_some(),
            "Đăng nhập OpenCode Go/Zen.",
        ),
        api_provider("openrouter", "OpenRouter", "#34d399"),
        api_provider("zai", "Z.ai", "#f97316"),
    ]
}

pub fn rescan_local(providers: &mut [ProviderSnapshot]) {
    for provider in providers {
        provider.local = scanner::scan(&provider.id);
        if provider.metrics.is_empty() {
            provider.metrics = scanner::quota_metrics(&provider.id);
        }
        if provider.local.detected && matches!(provider.status, ProviderStatus::NeedsLogin) {
            provider.status = ProviderStatus::Detected;
            provider.message = Some("Local sessions detected on this computer.".into());
        }
    }
}

fn provider(
    id: &str,
    name: &str,
    color: &str,
    experimental: bool,
    detected: bool,
    help: &str,
) -> ProviderSnapshot {
    ProviderSnapshot {
        id: id.into(),
        name: name.into(),
        color: color.into(),
        status: if detected {
            ProviderStatus::Detected
        } else {
            ProviderStatus::NeedsLogin
        },
        plan: None,
        metrics: scanner::quota_metrics(id),
        message: Some(if detected {
            "Đã tìm thấy thông tin đăng nhập. Sẵn sàng refresh.".into()
        } else {
            help.into()
        }),
        refreshed_at: None,
        experimental,
        local: scanner::scan(id),
    }
}

fn api_provider(id: &str, name: &str, color: &str) -> ProviderSnapshot {
    let has_key = read_secret(id).is_some();
    ProviderSnapshot {
        id: id.into(),
        name: name.into(),
        color: color.into(),
        status: if has_key {
            ProviderStatus::Detected
        } else {
            ProviderStatus::NeedsApiKey
        },
        plan: None,
        metrics: vec![],
        message: Some(if has_key {
            "API key đã được lưu an toàn.".into()
        } else {
            "Thêm API key trong Settings để bắt đầu.".into()
        }),
        refreshed_at: None,
        experimental: false,
        local: scanner::scan(id),
    }
}

pub fn save_secret(provider: &str, value: &str) -> Result<(), String> {
    if !matches!(provider, "openrouter" | "zai") {
        return Err("Provider không hỗ trợ API key thủ công".into());
    }
    let entry = keyring::Entry::new(SERVICE, provider).map_err(|e| e.to_string())?;
    if value.trim().is_empty() {
        entry
            .delete_credential()
            .or_else(|e| {
                if matches!(e, keyring::Error::NoEntry) {
                    Ok(())
                } else {
                    Err(e)
                }
            })
            .map_err(|e| e.to_string())
    } else {
        entry.set_password(value.trim()).map_err(|e| e.to_string())
    }
}

fn read_secret(provider: &str) -> Option<String> {
    let env_key = match provider {
        "openrouter" => "OPENROUTER_API_KEY",
        "zai" => "ZAI_API_KEY",
        _ => return None,
    };
    keyring::Entry::new(SERVICE, provider)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.is_empty())
        .or_else(|| env::var(env_key).ok().filter(|s| !s.is_empty()))
}

pub async fn refresh(mut snapshot: ProviderSnapshot, client: &Client) -> ProviderSnapshot {
    let was_detected = matches!(snapshot.status, ProviderStatus::Detected);
    snapshot.status = ProviderStatus::Refreshing;
    let result = match snapshot.id.as_str() {
        "openrouter" => refresh_openrouter(client).await,
        "zai" => refresh_zai(client).await,
        "codex" => {
            refresh_bearer_json(
                client,
                codex_auth_path(),
                "https://chatgpt.com/backend-api/wham/usage",
                "codex",
            )
            .await
        }
        "claude" => {
            refresh_bearer_json(
                client,
                claude_auth_path(),
                "https://api.anthropic.com/api/oauth/usage",
                "claude",
            )
            .await
        }
        "cursor" => refresh_cursor(client).await,
        "copilot" => refresh_copilot(client).await,
        "devin" => refresh_devin(client).await,
        "grok" => {
            refresh_bearer_json(
                client,
                grok_auth_path(),
                "https://cli-chat-proxy.grok.com/v1/billing?format=credits",
                "grok",
            )
            .await
        }
        _ => Err("Connector live đang được hoàn thiện; credential discovery đã hoạt động.".into()),
    };
    match result {
        Ok((plan, metrics)) => {
            snapshot.status = ProviderStatus::Ready;
            snapshot.plan = plan;
            snapshot.metrics = metrics;
            snapshot.message = None;
            snapshot.refreshed_at = Some(Utc::now().to_rfc3339());
        }
        Err(message) => {
            snapshot.status = if was_detected && message.contains("đang được hoàn thiện") {
                ProviderStatus::Detected
            } else if message.contains("API key") {
                ProviderStatus::NeedsApiKey
            } else if message.contains("đăng nhập") || message.contains("credential") {
                ProviderStatus::NeedsLogin
            } else {
                ProviderStatus::Error
            };
            snapshot.message = Some(message);
        }
    }
    snapshot
}

async fn refresh_openrouter(client: &Client) -> Result<(Option<String>, Vec<Metric>), String> {
    let key = read_secret("openrouter").ok_or("Chưa có OpenRouter API key")?;
    let credits = get_json(client, "https://openrouter.ai/api/v1/credits", &key, None).await?;
    let data = credits.get("data").unwrap_or(&credits);
    let total = number(data, &["total_credits"]).unwrap_or(0.0);
    let used = number(data, &["total_usage"]).unwrap_or(0.0);
    let mut metrics = vec![Metric {
        id: "openrouter.credits".into(),
        label: "Credits".into(),
        used: Some(used),
        limit: Some(total),
        unit: "$".into(),
        reset_at: None,
        detail: Some(format!("${:.2} remaining", (total - used).max(0.0))),
    }];
    let key_info = get_json(client, "https://openrouter.ai/api/v1/key", &key, None)
        .await
        .ok();
    if let Some(info) = key_info {
        let d = info.get("data").unwrap_or(&info);
        for (id, label, field) in [
            ("daily", "Today", "usage_daily"),
            ("weekly", "This week", "usage_weekly"),
            ("monthly", "This month", "usage_monthly"),
        ] {
            if let Some(v) = number(d, &[field]) {
                metrics.push(Metric {
                    id: format!("openrouter.{id}"),
                    label: label.into(),
                    used: Some(v),
                    limit: None,
                    unit: "$".into(),
                    reset_at: None,
                    detail: None,
                });
            }
        }
    }
    Ok((
        Some(
            if total > 0.0 {
                "Pay as you go"
            } else {
                "Free tier"
            }
            .into(),
        ),
        metrics,
    ))
}

async fn refresh_zai(client: &Client) -> Result<(Option<String>, Vec<Metric>), String> {
    let key = read_secret("zai").ok_or("Chưa có Z.ai API key")?;
    let root = get_json(
        client,
        "https://api.z.ai/api/monitor/usage/quota/limit",
        &key,
        Some("Bearer"),
    )
    .await?;
    let limits = root
        .pointer("/data/limits")
        .and_then(Value::as_array)
        .ok_or("Z.ai không trả về Coding Plan quota")?;
    let mut metrics = vec![];
    for item in limits {
        let kind = item
            .get("type")
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if kind == "TOKENS_LIMIT" {
            if let Some(p) = number(item, &["percentage"]) {
                let unit = number(item, &["unit"]).unwrap_or(0.0);
                let label = if unit == 3.0 { "Session" } else { "Weekly" };
                metrics.push(Metric::percent(
                    &format!("zai.{}", label.to_lowercase()),
                    label,
                    p,
                    epoch_ms(item.get("nextResetTime")),
                ));
            }
        } else if kind == "TIME_LIMIT" {
            metrics.push(Metric {
                id: "zai.search".into(),
                label: "Web searches".into(),
                used: number(item, &["currentValue"]),
                limit: number(item, &["usage"]),
                unit: "searches".into(),
                reset_at: epoch_ms(item.get("nextResetTime")),
                detail: None,
            });
        }
    }
    Ok((Some("GLM Coding Plan".into()), metrics))
}

async fn refresh_bearer_json(
    client: &Client,
    path: Option<PathBuf>,
    url: &str,
    provider: &str,
) -> Result<(Option<String>, Vec<Metric>), String> {
    let path =
        path.ok_or_else(|| format!("Không tìm thấy credential {provider}; hãy đăng nhập trước"))?;
    let json: Value = serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let token = find_string(
        &json,
        &["access_token", "accessToken", "oauth_token", "token"],
    )
    .ok_or("Credential không chứa access token")?;
    let root = get_json(client, url, &token, Some("Bearer")).await?;
    let metrics = if provider == "codex" {
        codex_metrics(&root)
    } else if provider == "grok" {
        grok_metrics(&root)
    } else {
        generic_percent_metrics(provider, &root)
    };
    let plan = find_string(&root, &["plan_type", "plan", "subscription_type"]);
    if metrics.is_empty() {
        Err("API trả về dữ liệu nhưng chưa có meter tương thích".into())
    } else {
        Ok((plan, metrics))
    }
}

async fn refresh_copilot(client: &Client) -> Result<(Option<String>, Vec<Metric>), String> {
    let output = silent_command("gh")
        .args(["auth", "token"])
        .output()
        .map_err(|_| "Cài GitHub CLI và đăng nhập trước")?;
    if !output.status.success() {
        return Err("Chạy `gh auth login` trước".into());
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let root = get_json(
        client,
        "https://api.github.com/copilot_internal/user",
        &token,
        Some("token"),
    )
    .await?;
    let mut metrics = vec![];
    if let Some(q) = root.get("quota_snapshots").and_then(Value::as_object) {
        for (id, item) in q {
            let rem = number(item, &["percent_remaining"]);
            let used = rem.map(|v| 100.0 - v);
            if let Some(used) = used {
                metrics.push(Metric::percent(
                    &format!("copilot.{id}"),
                    &title(id),
                    used,
                    root.get("quota_reset_date")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                ));
            }
        }
    }
    Ok((find_string(&root, &["copilot_plan"]), metrics))
}

async fn refresh_cursor(client: &Client) -> Result<(Option<String>, Vec<Metric>), String> {
    let path =
        cursor_state_path().ok_or("Không tìm thấy Cursor state; hãy mở Cursor và đăng nhập")?;
    let token = vscdb_value(&path, "cursorAuth/accessToken")
        .ok_or("Chưa đăng nhập Cursor")?
        .trim_matches('"')
        .to_owned();

    let usage = cursor_rpc(client, &token, "GetCurrentPeriodUsage").await?;
    let remote_plan = match cursor_rpc(client, &token, "GetPlanInfo").await {
        Ok(response) => response
            .pointer("/planInfo/planName")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Err(_) => None,
    };
    let local_plan = vscdb_value(&path, "cursorAuth/stripeMembershipType")
        .map(|value| value.trim_matches('"').to_owned());
    let metrics = cursor_usage_metrics(&usage);
    if metrics.is_empty() {
        return Err("Cursor không trả về dữ liệu Auto + Composer hoặc API".into());
    }

    Ok((remote_plan.or(local_plan), metrics))
}

async fn cursor_rpc(client: &Client, token: &str, method: &str) -> Result<Value, String> {
    let response = client
        .post(format!(
            "https://api2.cursor.sh/aiserver.v1.DashboardService/{method}"
        ))
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|error| format!("Không thể kết nối Cursor: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Cursor trả về HTTP {}", response.status()));
    }
    response.json().await.map_err(|error| error.to_string())
}

fn cursor_usage_metrics(root: &Value) -> Vec<Metric> {
    let Some(usage) = root.get("planUsage") else {
        return vec![];
    };
    let reset_at = epoch_ms(root.get("billingCycleEnd"));
    let mut metrics = vec![];
    if let Some(used) = number(usage, &["autoPercentUsed"]) {
        metrics.push(Metric::percent(
            "cursor.auto_composer",
            "Auto + Composer",
            used,
            reset_at.clone(),
        ));
    }
    if let Some(used) = number(usage, &["apiPercentUsed"]) {
        metrics.push(Metric::percent("cursor.api", "API", used, reset_at));
    }
    metrics
}
async fn refresh_devin(client: &Client) -> Result<(Option<String>, Vec<Metric>), String> {
    let path = devin_auth_path()
        .ok_or("Không tìm thấy credential Devin; chạy `devin auth login` hoặc mở Devin app")?;

    let is_vscdb = path.extension().and_then(|e| e.to_str()) == Some("vscdb");

    let (key, server) = if is_vscdb {
        let auth_json = vscdb_value(&path, "windsurfAuthStatus")
            .ok_or("Không tìm thấy session Devin trong vscdb")?;
        let auth_val: Value = serde_json::from_str(&auth_json).map_err(|e| e.to_string())?;
        let key = auth_val
            .get("apiKey")
            .and_then(Value::as_str)
            .ok_or("Devin session không chứa apiKey")?
            .to_string();
        (key, "https://server.codeium.com".to_string())
    } else {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let key =
            toml_string(&text, "windsurf_api_key").ok_or("Credential Devin không chứa API key")?;
        let server = toml_string(&text, "api_server_url")
            .unwrap_or_else(|| "https://server.codeium.com".into());
        (key, server)
    };

    let url = format!(
        "{}/exa.seat_management_pb.SeatManagementService/GetUserStatus",
        server.trim_end_matches('/')
    );
    let body = serde_json::json!({"metadata":{"apiKey":key,"ideName":"devin","ideVersion":"1.108.2","extensionName":"devin","extensionVersion":"1.108.2","locale":"en"}});
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Không thể kết nối Devin: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Devin trả về HTTP {}", response.status()));
    }
    let root: Value = response.json().await.map_err(|e| e.to_string())?;
    let status = root
        .pointer("/userStatus/planStatus")
        .ok_or("Devin không trả về quota")?;
    let mut metrics = vec![];
    if let Some(remaining) = number(status, &["dailyQuotaRemainingPercent"]) {
        metrics.push(Metric::percent(
            "devin.daily",
            "Daily quota",
            100.0 - remaining,
            unix_seconds(status.get("dailyQuotaResetAtUnix")),
        ));
    }
    if let Some(remaining) = number(status, &["weeklyQuotaRemainingPercent"]) {
        metrics.push(Metric::percent(
            "devin.weekly",
            "Weekly quota",
            100.0 - remaining,
            unix_seconds(status.get("weeklyQuotaResetAtUnix")),
        ));
    }
    if let Some(micros) = number(status, &["overageBalanceMicros"]) {
        metrics.push(Metric {
            id: "devin.extra".into(),
            label: "Extra usage balance".into(),
            used: Some((micros / 1_000_000.0).max(0.0)),
            limit: None,
            unit: "$".into(),
            reset_at: None,
            detail: None,
        });
    }
    let plan = root
        .pointer("/userStatus/planStatus/planInfo/planName")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if metrics.is_empty() {
        Err("Devin quota chưa khả dụng cho tài khoản này".into())
    } else {
        Ok((plan, metrics))
    }
}

async fn get_json(
    client: &Client,
    url: &str,
    token: &str,
    scheme: Option<&str>,
) -> Result<Value, String> {
    let auth = match scheme {
        Some("token") => format!("token {token}"),
        _ => format!("Bearer {token}"),
    };
    let mut request = client
        .get(url)
        .header("Authorization", auth)
        .header("Accept", "application/json")
        .header("User-Agent", "CodeUsage/0.1");
    if url.contains("api.anthropic.com") {
        request = request
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("User-Agent", "claude-code/2.1.69");
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Không thể kết nối: {e}"))?;
    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err("Phiên đăng nhập/API key không hợp lệ".into());
    }
    if !response.status().is_success() {
        return Err(format!("Provider trả về HTTP {}", response.status()));
    }
    response
        .json()
        .await
        .map_err(|e| format!("JSON không hợp lệ: {e}"))
}

fn codex_metrics(root: &Value) -> Vec<Metric> {
    let mut out = vec![];
    for (slot, label) in [
        ("primary_window", "Session"),
        ("secondary_window", "Weekly"),
    ] {
        if let Some(w) = root.pointer(&format!("/rate_limit/{slot}")) {
            if let Some(p) = number(w, &["used_percent"]) {
                out.push(Metric::percent(
                    &format!("codex.{}", label.to_lowercase()),
                    label,
                    p,
                    reset_time(w),
                ));
            }
        }
    }
    out
}
fn grok_metrics(root: &Value) -> Vec<Metric> {
    root.pointer("/config/creditUsagePercent")
        .and_then(Value::as_f64)
        .map(|p| {
            vec![Metric::percent(
                "grok.weekly",
                "Weekly",
                p,
                root.pointer("/config/currentPeriod/end")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )]
        })
        .unwrap_or_default()
}
fn generic_percent_metrics(provider: &str, root: &Value) -> Vec<Metric> {
    let mut out = vec![];
    for (key, label) in [
        ("five_hour", "Session"),
        ("seven_day", "Weekly"),
        ("five_hour_utilization", "Session"),
        ("seven_day_utilization", "Weekly"),
    ] {
        if let Some(v) = root.get(key) {
            let p = v
                .as_f64()
                .or_else(|| number(v, &["utilization", "used_percent"]));
            if let Some(p) = p {
                out.push(Metric::percent(
                    &format!("{provider}.{key}"),
                    label,
                    if p <= 1.0 { p * 100.0 } else { p },
                    find_string(v, &["resets_at", "reset_at"]),
                ));
            }
        }
    }
    out
}
fn number(v: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|k| v.get(k))
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
}
fn find_string(v: &Value, keys: &[&str]) -> Option<String> {
    if let Some(o) = v.as_object() {
        for k in keys {
            if let Some(s) = o.get(*k).and_then(Value::as_str) {
                if !s.is_empty() {
                    return Some(s.into());
                }
            }
        }
        for child in o.values() {
            if let Some(s) = find_string(child, keys) {
                return Some(s);
            }
        }
    } else if let Some(a) = v.as_array() {
        for child in a {
            if let Some(s) = find_string(child, keys) {
                return Some(s);
            }
        }
    }
    None
}
fn reset_time(v: &Value) -> Option<String> {
    if let Some(s) = find_string(v, &["reset_at", "resets_at"]) {
        return Some(s);
    }
    if let Some(sec) = number(v, &["reset_at"]) {
        return chrono::DateTime::from_timestamp(sec as i64, 0).map(|d| d.to_rfc3339());
    }
    None
}
fn epoch_ms(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse().ok()))
        .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
        .map(|d| d.to_rfc3339())
}
fn unix_seconds(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| x.as_i64())
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .map(|d| d.to_rfc3339())
}
fn toml_string(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let clean = value.split('#').next()?.trim().trim_matches(['"', '\'']);
        (!clean.is_empty()).then(|| clean.to_owned())
    })
}
fn title(s: &str) -> String {
    s.split('_')
        .map(|p| {
            let mut c = p.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn home() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}
fn existing(paths: Vec<PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|p| p.exists())
}
fn codex_auth_path() -> Option<PathBuf> {
    let base = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|p| p.join(".codex")))?;
    existing(vec![base.join("auth.json")])
}
fn claude_auth_path() -> Option<PathBuf> {
    let base = env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| home().map(|p| p.join(".claude")))?;
    existing(vec![
        base.join(".credentials.json"),
        base.join("credentials.json"),
    ])
}
fn grok_auth_path() -> Option<PathBuf> {
    let base = env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|p| p.join(".grok")))?;
    existing(vec![base.join("auth.json")])
}
fn devin_auth_path() -> Option<PathBuf> {
    let mut paths = vec![];
    if let Some(d) = env::var_os("APPDATA").map(PathBuf::from) {
        paths.push(d.join("devin/User/globalStorage/state.vscdb"));
        paths.push(d.join("Devin/User/globalStorage/state.vscdb"));
    }
    if let Some(h) = home() {
        paths.push(h.join(".local/share/devin/credentials.toml"));
    }
    existing(paths)
}
fn opencode_auth_path() -> Option<PathBuf> {
    let mut p = dirs::data_dir()?;
    p.push("opencode/auth.json");
    existing(vec![p])
}
fn cursor_state_path() -> Option<PathBuf> {
    let mut paths = vec![];
    if let Some(d) = dirs::data_dir() {
        paths.push(d.join("Cursor/User/globalStorage/state.vscdb"));
    }
    if let Some(d) = env::var_os("APPDATA").map(PathBuf::from) {
        paths.push(d.join("Cursor/User/globalStorage/state.vscdb"));
    }
    existing(paths)
}
fn detect_antigravity() -> bool {
    let mut paths = vec![];
    if let Some(d) = dirs::data_dir() {
        paths.push(d.join("Antigravity/User/globalStorage/state.vscdb"));
        paths.push(d.join("Antigravity IDE/User/globalStorage/state.vscdb"));
    }
    existing(paths).is_some()
}
fn command_exists(name: &str) -> bool {
    silent_command(name).arg("--version").output().is_ok()
}

fn silent_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
}

fn vscdb_value(db_path: &PathBuf, key: &str) -> Option<String> {
    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .ok()
    .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_codex_windows() {
        let body = serde_json::json!({
            "rate_limit": {
                "primary_window": {"used_percent": 24, "reset_at": 1_900_000_000},
                "secondary_window": {"used_percent": 61}
            }
        });
        let metrics = codex_metrics(&body);
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].label, "Session");
        assert_eq!(metrics[1].used, Some(61.0));
    }

    #[test]
    fn maps_cursor_usage_pools() {
        let body = serde_json::json!({
            "billingCycleEnd": "1900000000000",
            "planUsage": {
                "autoPercentUsed": 42.5,
                "apiPercentUsed": 18.25
            }
        });
        let metrics = cursor_usage_metrics(&body);
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].label, "Auto + Composer");
        assert_eq!(metrics[0].used, Some(42.5));
        assert_eq!(metrics[1].label, "API");
        assert_eq!(metrics[1].used, Some(18.25));
        assert!(metrics[0].reset_at.is_some());
    }
    #[test]
    fn maps_fractional_generic_usage() {
        let body = serde_json::json!({"five_hour": {"utilization": 0.42}});
        let metrics = generic_percent_metrics("claude", &body);
        assert_eq!(metrics[0].used, Some(42.0));
    }
}
