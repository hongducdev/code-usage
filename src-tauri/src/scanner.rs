use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde_json::Value;

use crate::model::{
    AgentActivity, AgentActivityStatus, DailyUsage, LocalPresence, LocalUsage, Metric,
};

const MAX_FILES: usize = 10_000;
const MAX_ACTIVITY_FILES: usize = 256;
const ACTIVITY_WINDOW_MINUTES: i64 = 15;
const ACTIVITY_TAIL_BYTES: u64 = 96 * 1024;

pub fn active_agents() -> Vec<AgentActivity> {
    let cutoff = SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(
            (ACTIVITY_WINDOW_MINUTES * 60) as u64,
        ))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut files = Vec::new();

    for provider in ["codex", "claude"] {
        let mut provider_files = Vec::new();
        for (index, (root, extensions)) in roots(provider).into_iter().enumerate() {
            if provider == "codex" && index > 0 {
                break;
            }
            collect_recent_files(&root, &extensions, 0, cutoff, &mut provider_files);
        }
        for (path, modified) in provider_files {
            files.push((provider, path, modified));
        }
    }

    files.sort_by_key(|(_, _, modified)| std::cmp::Reverse(*modified));
    files
        .into_iter()
        .take(12)
        .filter_map(|(provider, path, modified)| parse_agent_activity(provider, &path, modified))
        .collect()
}

fn collect_recent_files(
    path: &Path,
    extensions: &[&str],
    depth: u8,
    cutoff: SystemTime,
    files: &mut Vec<(PathBuf, SystemTime)>,
) {
    if files.len() >= MAX_ACTIVITY_FILES || depth > 8 {
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_file() {
        let matches = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extensions
                    .iter()
                    .any(|allowed| extension.eq_ignore_ascii_case(allowed))
            });
        if matches {
            if let Ok(modified) = metadata.modified() {
                if modified >= cutoff {
                    files.push((path.to_owned(), modified));
                }
            }
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok()),
        )
    });
    for entry in entries {
        collect_recent_files(&entry.path(), extensions, depth + 1, cutoff, files);
        if files.len() >= MAX_ACTIVITY_FILES {
            break;
        }
    }
}

fn parse_agent_activity(
    provider: &str,
    path: &Path,
    modified: SystemTime,
) -> Option<AgentActivity> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(ACTIVITY_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = String::new();
    file.read_to_string(&mut tail).ok()?;
    if start > 0 {
        tail = tail.split_once('\n').map(|(_, rest)| rest.to_owned())?;
    }

    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default()
        .as_secs();
    let mut id = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(provider)
        .rsplit('-')
        .next()
        .unwrap_or(provider)
        .to_owned();
    let mut workspace = provider.to_owned();
    let mut last_message = None;
    let mut last_tool = None;
    let mut saw_error = false;
    let mut saw_final_message = false;
    let mut pending_approval_call = None;

    if let Ok(meta_file) = fs::File::open(path) {
        if let Some(Ok(line)) = BufReader::new(meta_file).lines().next() {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if let Some(value_id) = value.pointer("/payload/id").and_then(Value::as_str) {
                    id = value_id.to_owned();
                }
                if let Some(cwd) = value.pointer("/payload/cwd").and_then(Value::as_str) {
                    workspace = Path::new(cwd)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(cwd)
                        .to_owned();
                }
            }
        }
    }

    for line in tail.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let payload_type = value
            .pointer("/payload/type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        if event_type == "event_msg" && payload_type == "agent_message" {
            if let Some(message) = value.pointer("/payload/message").and_then(Value::as_str) {
                last_message = clean_agent_message(message);
                saw_final_message = true;
            }
        } else if event_type == "response_item" && payload_type == "message" {
            if value.pointer("/payload/role").and_then(Value::as_str) == Some("assistant") {
                if let Some(content) = value.pointer("/payload/content").and_then(Value::as_array) {
                    if let Some(text) = content.iter().rev().find_map(|item| {
                        item.get("text")
                            .and_then(Value::as_str)
                            .or_else(|| item.get("content").and_then(Value::as_str))
                    }) {
                        last_message = clean_agent_message(text);
                    }
                }
                saw_final_message = true;
            }
        } else if event_type == "response_item"
            && ["custom_tool_call", "function_call"].contains(&payload_type)
        {
            let tool_name = value
                .pointer("/payload/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let tool_input = value
                .pointer("/payload/input")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let requires_approval = is_approval_tool(tool_name, tool_input);
            if requires_approval {
                pending_approval_call = value
                    .pointer("/payload/call_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            last_tool = value
                .pointer("/payload/name")
                .and_then(Value::as_str)
                .map(localize_tool_name);
            saw_final_message = false;
        } else if event_type == "response_item" && payload_type == "custom_tool_call_output" {
            let completed_call = value.pointer("/payload/call_id").and_then(Value::as_str);
            if completed_call.is_some_and(|call| pending_approval_call.as_deref() == Some(call)) {
                pending_approval_call = None;
            }
        } else if payload_type.contains("error") || event_type.contains("error") {
            saw_error = true;
        }

        if provider == "claude" && event_type == "assistant" {
            if let Some(content) = value.pointer("/message/content").and_then(Value::as_array) {
                for item in content {
                    match item.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(text) = item.get("text").and_then(Value::as_str) {
                                last_message = clean_agent_message(text);
                                saw_final_message = true;
                            }
                        }
                        Some("tool_use") => {
                            last_tool = item
                                .get("name")
                                .and_then(Value::as_str)
                                .map(localize_tool_name);
                            saw_final_message = false;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let (status, progress, fallback) = if pending_approval_call.is_some() && age <= 300 {
        (
            AgentActivityStatus::NeedsApproval,
            88,
            "Đang chờ bạn phê duyệt",
        )
    } else if saw_error {
        (AgentActivityStatus::Error, 100, "Agent gặp lỗi")
    } else if age <= 12 {
        (AgentActivityStatus::Working, 68, "Đang xử lý")
    } else if saw_final_message {
        (
            AgentActivityStatus::Completed,
            100,
            "Đã hoàn tất lượt gần nhất",
        )
    } else {
        (AgentActivityStatus::Waiting, 84, "Đang chờ bước tiếp theo")
    };
    let message = if matches!(
        status,
        AgentActivityStatus::Working | AgentActivityStatus::NeedsApproval
    ) {
        last_tool
            .map(|tool| format!("Đang {tool}"))
            .or(last_message)
            .unwrap_or_else(|| fallback.to_owned())
    } else {
        last_message.unwrap_or_else(|| fallback.to_owned())
    };

    Some(AgentActivity {
        id,
        provider: provider.to_owned(),
        workspace,
        status,
        progress,
        message,
        updated_at: DateTime::<Utc>::from(modified).to_rfc3339(),
    })
}

fn clean_agent_message(message: &str) -> Option<String> {
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }
    let shortened: String = compact.chars().take(180).collect();
    Some(if compact.chars().count() > 180 {
        format!("{shortened}…")
    } else {
        shortened
    })
}

fn localize_tool_name(name: &str) -> String {
    match name {
        "exec" | "shell_command" => "chạy tác vụ hệ thống".into(),
        "apply_patch" => "cập nhật mã nguồn".into(),
        "web__run" | "web_search" => "tra cứu thông tin".into(),
        "view_image" => "kiểm tra hình ảnh".into(),
        "imagegen" | "image_gen__imagegen" => "tạo hình ảnh".into(),
        "spawn_agent" => "khởi tạo agent phụ".into(),
        "request_user_input" => "chờ bạn xác nhận".into(),
        _ => "thực hiện công cụ".into(),
    }
}

fn is_approval_tool(name: &str, input: &str) -> bool {
    name == "request_user_input"
        || (input.contains("sandbox_permissions")
            && input.contains("require_escalated")
            && input.contains("justification"))
}

pub fn scan(provider: &str) -> LocalPresence {
    let roots = roots(provider);
    let mut count = 0usize;
    let mut latest: Option<SystemTime> = None;
    let mut detected = false;

    for (path, extensions) in roots {
        if !path.exists() {
            continue;
        }
        detected = true;
        scan_path(&path, &extensions, 0, &mut count, &mut latest);
        if count >= MAX_FILES {
            break;
        }
    }

    let usage = match provider {
        "codex" => scan_jsonl_usage(provider, session_files(provider), parse_codex_line),
        "claude" => scan_jsonl_usage(provider, session_files(provider), parse_claude_line),
        _ => None,
    };

    LocalPresence {
        detected,
        session_count: count,
        last_activity: latest.map(|time| DateTime::<Utc>::from(time).to_rfc3339()),
        source: detected.then(|| source_name(provider).to_owned()),
        usage,
    }
}

pub fn quota_metrics(provider: &str) -> Vec<Metric> {
    if provider != "codex" {
        return vec![];
    }
    let mut latest: Option<(DateTime<Utc>, Value)> = None;
    for path in session_files(provider) {
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.pointer("/payload/type").and_then(Value::as_str) != Some("token_count") {
                continue;
            }
            let Some(rate_limits) = value.pointer("/payload/rate_limits") else {
                continue;
            };
            let Some(timestamp) = value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|date| date.with_timezone(&Utc))
            else {
                continue;
            };
            if latest
                .as_ref()
                .is_none_or(|(current, _)| timestamp > *current)
            {
                latest = Some((timestamp, rate_limits.clone()));
            }
        }
    }
    latest
        .map(|(_, limits)| quota_metrics_from_value(&limits))
        .unwrap_or_default()
}

fn quota_metrics_from_value(limits: &Value) -> Vec<Metric> {
    let mut metrics = vec![];
    for (key, fallback_label) in [("primary", "Session"), ("secondary", "Weekly")] {
        let Some(window) = limits.get(key) else {
            continue;
        };
        let Some(used) = window.get("used_percent").and_then(Value::as_f64) else {
            continue;
        };
        let minutes = window.get("window_minutes").and_then(Value::as_u64);
        let label = match minutes {
            Some(value) if value >= 24 * 60 => "Weekly",
            Some(_) => "Session",
            None => fallback_label,
        };
        let reset_at = window
            .get("resets_at")
            .and_then(Value::as_i64)
            .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
            .map(|date| date.to_rfc3339());
        metrics.push(Metric::percent(
            &format!("codex.{}", label.to_lowercase()),
            label,
            used,
            reset_at,
        ));
    }
    metrics
}

#[derive(Default)]
struct UsageAccumulator {
    daily: HashMap<NaiveDate, (u64, f64)>,
    models: HashMap<String, u64>,
    latest_timestamp: i64,
    latest_tokens: u64,
}

struct UsageEvent {
    timestamp: DateTime<Utc>,
    model: String,
    input: u64,
    cached: u64,
    output: u64,
}

fn scan_jsonl_usage(
    provider: &str,
    files: Vec<PathBuf>,
    parser: fn(&Value, &mut Option<String>) -> Option<UsageEvent>,
) -> Option<LocalUsage> {
    let cutoff = Utc::now() - Duration::days(31);
    let mut accumulator = UsageAccumulator::default();
    for path in files {
        if fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(DateTime::<Utc>::from)
            .is_some_and(|modified| modified < cutoff)
        {
            continue;
        }
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        let mut current_model = None;
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(event) = parser(&value, &mut current_model) else {
                continue;
            };
            if event.timestamp < cutoff {
                continue;
            }
            let tokens = event.input.saturating_add(event.output);
            let cost = estimate_cost(
                provider,
                &event.model,
                event.input,
                event.cached,
                event.output,
            );
            let day = event.timestamp.with_timezone(&Local).date_naive();
            let entry = accumulator.daily.entry(day).or_default();
            entry.0 = entry.0.saturating_add(tokens);
            entry.1 += cost;
            *accumulator.models.entry(event.model.clone()).or_default() += tokens;
            if event.timestamp.timestamp_millis() >= accumulator.latest_timestamp {
                accumulator.latest_timestamp = event.timestamp.timestamp_millis();
                accumulator.latest_tokens = tokens;
            }
        }
    }
    if accumulator.daily.is_empty() {
        return None;
    }
    let today = Local::now().date_naive();
    let mut daily = Vec::with_capacity(30);
    for offset in (0..30).rev() {
        let date = today - Duration::days(offset);
        let (tokens, cost) = accumulator.daily.get(&date).copied().unwrap_or_default();
        daily.push(DailyUsage {
            date: date.to_string(),
            tokens,
            cost,
        });
    }
    let today_cost = accumulator
        .daily
        .get(&today)
        .map(|v| v.1)
        .unwrap_or_default();
    let cost_30d = daily.iter().map(|d| d.cost).sum();
    let tokens_30d = daily.iter().map(|d| d.tokens).sum();
    let top_model = accumulator
        .models
        .into_iter()
        .max_by_key(|(_, tokens)| *tokens)
        .map(|(model, _)| model);
    Some(LocalUsage {
        today_cost,
        cost_30d,
        tokens_30d,
        latest_tokens: accumulator.latest_tokens,
        top_model,
        daily,
        estimated_cost: true,
    })
}

fn session_files(provider: &str) -> Vec<PathBuf> {
    let mut files = vec![];
    for (root, extensions) in roots(provider) {
        collect_files(&root, &extensions, 0, &mut files);
    }
    files
}

fn collect_files(path: &Path, extensions: &[&str], depth: u8, files: &mut Vec<PathBuf>) {
    if files.len() >= MAX_FILES || depth > 8 {
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_file() {
        if path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            extensions
                .iter()
                .any(|allowed| e.eq_ignore_ascii_case(allowed))
        }) {
            files.push(path.to_owned());
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect_files(&entry.path(), extensions, depth + 1, files);
    }
}

fn parse_codex_line(value: &Value, current_model: &mut Option<String>) -> Option<UsageEvent> {
    let payload = value.get("payload")?;
    if value.get("type").and_then(Value::as_str) == Some("turn_context") {
        if let Some(model) = payload.get("model").and_then(Value::as_str) {
            *current_model = Some(model.to_owned());
        }
        return None;
    }
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return None;
    }
    let usage = payload.pointer("/info/last_token_usage")?;
    let timestamp = DateTime::parse_from_rfc3339(value.get("timestamp")?.as_str()?)
        .ok()?
        .with_timezone(&Utc);
    let input = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached = usage
        .get("cached_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(
            usage
                .get("reasoning_output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
    Some(UsageEvent {
        timestamp,
        model: current_model.clone().unwrap_or_else(|| "codex".into()),
        input,
        cached,
        output,
    })
}

fn parse_claude_line(value: &Value, _: &mut Option<String>) -> Option<UsageEvent> {
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let message = value.get("message")?;
    let usage = message.get("usage")?;
    let timestamp = DateTime::parse_from_rfc3339(value.get("timestamp")?.as_str()?)
        .ok()?
        .with_timezone(&Utc);
    let regular_input = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_write = usage
        .get("cache_creation_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cached = usage
        .get("cache_read_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Some(UsageEvent {
        timestamp,
        model: message
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("claude")
            .to_owned(),
        input: regular_input
            .saturating_add(cache_write)
            .saturating_add(cached),
        cached,
        output,
    })
}

fn estimate_cost(provider: &str, model: &str, input: u64, cached: u64, output: u64) -> f64 {
    let name = model.to_ascii_lowercase();
    let (input_rate, cached_rate, output_rate) = if provider == "claude" {
        if name.contains("opus") {
            (15.0, 1.5, 75.0)
        } else if name.contains("haiku") {
            (1.0, 0.1, 5.0)
        } else {
            (3.0, 0.3, 15.0)
        }
    } else {
        (1.25, 0.125, 10.0)
    };
    let uncached = input.saturating_sub(cached) as f64;
    (uncached * input_rate + cached as f64 * cached_rate + output as f64 * output_rate)
        / 1_000_000.0
}

fn roots(provider: &str) -> Vec<(PathBuf, Vec<&'static str>)> {
    let Some(home) = user_home() else {
        return vec![];
    };
    let app_data = dirs::data_dir();
    match provider {
        "codex" => {
            let base = env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            vec![
                (base.join("sessions"), vec!["jsonl"]),
                (base.join("archived_sessions"), vec!["jsonl"]),
            ]
        }
        "claude" => {
            let base = env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude"));
            let mut result = vec![(base.join("projects"), vec!["jsonl"])];
            if let Some(app) = app_data {
                result.push((
                    app.join("Claude/local-agent-mode-sessions"),
                    vec!["jsonl", "json"],
                ));
            }
            result
        }
        "grok" => {
            let base = env::var_os("GROK_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".grok"));
            vec![(base.join("logs"), vec!["jsonl", "json"])]
        }
        "cursor" => app_data
            .map(|p| {
                vec![
                    (
                        p.join("Cursor/User/globalStorage/state.vscdb"),
                        vec!["vscdb"],
                    ),
                    (p.join("Cursor/User/workspaceStorage"), vec!["vscdb"]),
                ]
            })
            .unwrap_or_default(),
        "antigravity" => app_data
            .map(|p| {
                vec![
                    (
                        p.join("Antigravity/User/globalStorage/state.vscdb"),
                        vec!["vscdb"],
                    ),
                    (
                        p.join("Antigravity IDE/User/globalStorage/state.vscdb"),
                        vec!["vscdb"],
                    ),
                ]
            })
            .unwrap_or_default(),
        "opencode" => {
            let base = env::var_os("OPENCODE_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share/opencode"));
            vec![(base, vec!["db", "json"])]
        }
        "devin" => vec![(home.join(".local/share/devin"), vec!["toml", "db", "jsonl"])],
        _ => vec![],
    }
}

fn user_home() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

fn scan_path(
    path: &Path,
    extensions: &[&str],
    depth: u8,
    count: &mut usize,
    latest: &mut Option<SystemTime>,
) {
    if *count >= MAX_FILES || depth > 8 {
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_file() {
        let matches = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            extensions
                .iter()
                .any(|allowed| e.eq_ignore_ascii_case(allowed))
        });
        if matches {
            *count += 1;
            if let Ok(modified) = metadata.modified() {
                *latest = Some(latest.map_or(modified, |current| current.max(modified)));
            }
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        scan_path(&entry.path(), extensions, depth + 1, count, latest);
        if *count >= MAX_FILES {
            break;
        }
    }
}

fn source_name(provider: &str) -> &'static str {
    match provider {
        "codex" => "Codex session rollouts",
        "claude" => "Claude projects",
        "grok" => "Grok CLI logs",
        "cursor" => "Cursor local state",
        "antigravity" => "Antigravity local state",
        "opencode" => "OpenCode local database",
        "devin" => "Devin local data",
        _ => "Local data",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_provider_has_source_label() {
        assert_eq!(source_name("codex"), "Codex session rollouts");
    }

    #[test]
    fn maps_local_codex_quota() {
        let value = serde_json::json!({
            "primary": {"used_percent": 48.0, "window_minutes": 300, "resets_at": 1_900_000_000},
            "secondary": {"used_percent": 23.0, "window_minutes": 10080, "resets_at": 1_900_100_000}
        });
        let metrics = quota_metrics_from_value(&value);
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].label, "Session");
        assert_eq!(metrics[1].label, "Weekly");
    }

    #[test]
    fn detects_pending_approval_tools() {
        assert!(is_approval_tool("request_user_input", "{}"));
        assert!(is_approval_tool(
            "exec",
            r#"{"sandbox_permissions":"require_escalated","justification":"Allow?"}"#
        ));
        assert!(!is_approval_tool(
            "exec",
            r#"{"command":"rg require_escalated"}"#
        ));
    }
}
