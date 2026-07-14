#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod model;
mod providers;
mod scanner;

use model::{
    AgentActivity, AgentActivityStatus, AppSettings, Dashboard, Metric, ProviderSnapshot,
    UpdateStatus,
};
use reqwest::Client;
use serde_json::Value;
use std::{collections::HashMap, fs, path::PathBuf, process::Command, sync::Mutex};
use tauri::{
    Manager, PhysicalPosition, PhysicalSize, State,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

const PET_COLLAPSED_WIDTH: u32 = 116;
const PET_COLLAPSED_HEIGHT: u32 = 132;
const PET_EXPANDED_WIDTH: u32 = 376;
const PET_EXPANDED_HEIGHT: u32 = 276;

struct AppState {
    dashboard: Mutex<Dashboard>,
    settings: Mutex<AppSettings>,
    quota_alert_levels: Mutex<HashMap<String, u8>>,
    pet_alerts: Mutex<Vec<AgentActivity>>,
    client: Client,
}

fn settings_path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|path| path.join("CodeUsage").join("settings.json"))
        .ok_or_else(|| "Không tìm thấy thư mục cấu hình".to_owned())
}

fn load_settings() -> AppSettings {
    settings_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn persist_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    state
        .dashboard
        .lock()
        .map(|d| d.clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_app_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    let mut settings = state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    settings.launch_at_startup = app
        .autolaunch()
        .is_enabled()
        .unwrap_or(settings.launch_at_startup);
    Ok(settings)
}

#[tauri::command]
fn save_app_settings(
    mut settings: AppSettings,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    settings.pet_auto_hide_ms = settings.pet_auto_hide_ms.clamp(3_000, 15_000);
    settings.pet_scale = settings.pet_scale.clamp(0.85, 1.2);
    settings.app_ui_scale = settings.app_ui_scale.clamp(0.9, 1.2);
    settings.quota_threshold = settings.quota_threshold.clamp(50, 95);
    let dashboard = state
        .dashboard
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let mut selected = Vec::new();
    for provider_id in std::mem::take(&mut settings.menu_bar_providers) {
        if selected.len() == 3 {
            break;
        }
        if dashboard
            .providers
            .iter()
            .any(|provider| provider.id == provider_id)
            && !selected.contains(&provider_id)
        {
            selected.push(provider_id);
        }
    }
    settings.menu_bar_providers = selected;

    let autostart = app.autolaunch();
    let currently_enabled = autostart.is_enabled().unwrap_or(false);
    if settings.launch_at_startup != currently_enabled {
        if settings.launch_at_startup {
            autostart.enable().map_err(|error| error.to_string())?;
        } else {
            autostart.disable().map_err(|error| error.to_string())?;
        }
    }
    settings.launch_at_startup = autostart.is_enabled().unwrap_or(settings.launch_at_startup);
    persist_settings(&settings)?;
    *state.settings.lock().map_err(|error| error.to_string())? = settings.clone();

    if let Some(pet) = app.get_webview_window("pet") {
        if settings.pet_enabled {
            let _ = pet.show();
            let _ = anchor_pet_window(&pet);
        } else {
            let _ = pet.hide();
        }
        let _ = pet.eval("window.applySettings && window.applySettings()");
    }
    sync_menu_bar_items(&app, &dashboard, &settings)?;
    Ok(settings)
}

fn smart_alert_level(used: f64, threshold: u8) -> Option<u8> {
    [threshold, threshold.max(85), 95]
        .into_iter()
        .filter(|level| *level >= threshold)
        .filter(|level| used >= f64::from(*level))
        .max()
}

#[tauri::command]
fn notify_quota_alert(
    provider: String,
    metric: String,
    used: f64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let settings = state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let key = format!("{provider}:{metric}");
    let Some(level) = smart_alert_level(used, settings.quota_threshold) else {
        state
            .quota_alert_levels
            .lock()
            .map_err(|error| error.to_string())?
            .remove(&key);
        return Ok(());
    };
    if !settings.smart_alerts_enabled {
        return Ok(());
    }

    let mut levels = state
        .quota_alert_levels
        .lock()
        .map_err(|error| error.to_string())?;
    if levels.get(&key).is_some_and(|previous| *previous >= level) {
        return Ok(());
    }
    levels.insert(key, level);
    drop(levels);
    let remaining = (100.0 - used).clamp(0.0, 100.0).round();
    state
        .pet_alerts
        .lock()
        .map_err(|error| error.to_string())?
        .push(AgentActivity {
            id: format!("quota:{provider}:{metric}:{level}"),
            provider: "codex".to_owned(),
            workspace: provider,
            status: AgentActivityStatus::QuotaWarning,
            progress: used.clamp(0.0, 100.0).round() as u8,
            message: format!("{metric}: đã dùng {}%, còn {remaining}%.", used.round()),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    Ok(())
}

#[tauri::command]
fn take_pet_alerts(state: State<'_, AppState>) -> Result<Vec<AgentActivity>, String> {
    let mut alerts = state.pet_alerts.lock().map_err(|error| error.to_string())?;
    Ok(std::mem::take(&mut *alerts))
}

fn version_parts(value: &str) -> [u32; 3] {
    let mut parts = value.trim_start_matches('v').split('.');
    [
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        parts.next().and_then(|part| part.parse().ok()).unwrap_or(0),
        parts
            .next()
            .and_then(|part| part.split('-').next())
            .and_then(|part| part.parse().ok())
            .unwrap_or(0),
    ]
}

#[tauri::command]
async fn check_for_update(state: State<'_, AppState>) -> Result<UpdateStatus, String> {
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let response = state
        .client
        .get("https://api.github.com/repos/hongducdev/code-usage/releases/latest")
        .header(reqwest::header::USER_AGENT, "CodeUsage update checker")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateStatus {
            current_version: current,
            latest_version: None,
            available: false,
            release_url: None,
        });
    }
    let payload = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<Value>()
        .await
        .map_err(|error| error.to_string())?;
    let latest = payload
        .get("tag_name")
        .and_then(Value::as_str)
        .map(|value| value.trim_start_matches('v').to_owned());
    let release_url = payload
        .get("html_url")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let available = latest
        .as_deref()
        .is_some_and(|version| version_parts(version) > version_parts(&current));
    Ok(UpdateStatus {
        current_version: current,
        latest_version: latest,
        available,
        release_url,
    })
}

#[tauri::command]
fn open_release_page() -> Result<(), String> {
    let url = "https://github.com/hongducdev/code-usage/releases/latest";
    #[cfg(target_os = "windows")]
    Command::new("explorer")
        .arg(url)
        .spawn()
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "macos")]
    Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn refresh_all(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Dashboard, String> {
    let snapshots = {
        let mut d = state.dashboard.lock().map_err(|e| e.to_string())?;
        d.refreshing = true;
        providers::rescan_local(&mut d.providers);
        d.providers.clone()
    };
    let mut jobs = tokio::task::JoinSet::new();
    let count = snapshots.len();
    for (index, snapshot) in snapshots.into_iter().enumerate() {
        let client = state.client.clone();
        jobs.spawn(async move { (index, providers::refresh(snapshot, &client).await) });
    }
    let mut slots = vec![None; count];
    while let Some(result) = jobs.join_next().await {
        let (index, snapshot) = result.map_err(|e| e.to_string())?;
        slots[index] = Some(snapshot);
    }
    let refreshed = slots.into_iter().flatten().collect();
    let dashboard = {
        let mut d = state.dashboard.lock().map_err(|e| e.to_string())?;
        d.providers = refreshed;
        d.refreshing = false;
        d.last_refresh = Some(chrono::Utc::now().to_rfc3339());
        d.clone()
    };
    let settings = state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    sync_menu_bar_items(&app, &dashboard, &settings)?;
    Ok(dashboard)
}

#[tauri::command]
async fn refresh_provider(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Dashboard, String> {
    let snapshot = {
        let d = state.dashboard.lock().map_err(|e| e.to_string())?;
        d.providers
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or("Provider không tồn tại")?
    };
    let refreshed = providers::refresh(snapshot, &state.client).await;
    let dashboard = {
        let mut d = state.dashboard.lock().map_err(|e| e.to_string())?;
        if let Some(slot) = d.providers.iter_mut().find(|p| p.id == id) {
            *slot = refreshed;
        }
        d.last_refresh = Some(chrono::Utc::now().to_rfc3339());
        d.clone()
    };
    let settings = state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    sync_menu_bar_items(&app, &dashboard, &settings)?;
    Ok(dashboard)
}

#[tauri::command]
fn scan_local(state: State<'_, AppState>) -> Result<Dashboard, String> {
    let mut d = state.dashboard.lock().map_err(|e| e.to_string())?;
    providers::rescan_local(&mut d.providers);
    Ok(d.clone())
}

#[tauri::command]
fn save_api_key(
    provider: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<Dashboard, String> {
    providers::save_secret(&provider, &value)?;
    let mut d = state.dashboard.lock().map_err(|e| e.to_string())?;
    d.providers = providers::registry();
    Ok(d.clone())
}

#[tauri::command]
fn get_agent_activity() -> Vec<AgentActivity> {
    scanner::active_agents()
}

fn pet_physical_size(window: &tauri::WebviewWindow, expanded: bool) -> (u32, u32) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let width = if expanded {
        PET_EXPANDED_WIDTH
    } else {
        PET_COLLAPSED_WIDTH
    };
    let height = if expanded {
        PET_EXPANDED_HEIGHT
    } else {
        PET_COLLAPSED_HEIGHT
    };
    (
        (width as f64 * scale).round() as u32,
        (height as f64 * scale).round() as u32,
    )
}

fn anchor_pet_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or("Không tìm thấy màn hình")?;
    let scale = monitor.scale_factor();
    let (physical_width, physical_height) = pet_physical_size(window, false);
    let margin = (18.0 * scale).round() as i32;
    let work = monitor.work_area();
    let x = work.position.x + work.size.width as i32 - physical_width as i32 - margin;
    let y = work.position.y + work.size.height as i32 - physical_height as i32 - margin;
    window
        .set_size(PhysicalSize::new(physical_width, physical_height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn resize_pet_window(window: &tauri::WebviewWindow, expanded: bool) -> Result<(), String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let current_size = window.outer_size().map_err(|error| error.to_string())?;
    let (width, height) = pet_physical_size(window, expanded);
    let right = position.x + current_size.width as i32;
    let bottom = position.y + current_size.height as i32;
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or("Không tìm thấy màn hình")?;
    let work = monitor.work_area();
    let min_x = work.position.x;
    let min_y = work.position.y;
    let max_x = work.position.x + work.size.width as i32 - width as i32;
    let max_y = work.position.y + work.size.height as i32 - height as i32;
    let x = (right - width as i32).clamp(min_x, max_x.max(min_x));
    let y = (bottom - height as i32).clamp(min_y, max_y.max(min_y));
    window
        .set_size(PhysicalSize::new(width, height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_pet_expanded(expanded: bool, app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("pet")
        .ok_or("Không tìm thấy cửa sổ pet")?;
    resize_pet_window(&window, expanded)
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Không tìm thấy cửa sổ chính")?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
fn toggle_pet_visibility(app: tauri::AppHandle) {
    toggle_pet_window(&app);
}

fn toggle_pet_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("pet") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
    }
}

fn tray_image() -> Image<'static> {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let index = (y * 32 + x) * 4;
            let distance = (x as i32 - 16).pow(2) + (y as i32 - 16).pow(2);
            let inside = distance < 220;
            let c_mark = (55..=120).contains(&distance) && (x < 20 || y < 10 || y > 22);
            if inside {
                let (red, green, blue) = if c_mark {
                    (247, 246, 242)
                } else {
                    (217, 119, 87)
                };
                rgba[index] = red;
                rgba[index + 1] = green;
                rgba[index + 2] = blue;
                rgba[index + 3] = 255;
            }
        }
    }
    Image::new_owned(rgba, 32, 32)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn provider_tray_icon(provider: &str) -> Result<Image<'static>, String> {
    let bytes: &'static [u8] = match provider {
        "antigravity" => include_bytes!("../icons/providers/antigravity.rgba"),
        "claude" => include_bytes!("../icons/providers/claude.rgba"),
        "codex" => include_bytes!("../icons/providers/codex.rgba"),
        "copilot" => include_bytes!("../icons/providers/copilot.rgba"),
        "cursor" => include_bytes!("../icons/providers/cursor.rgba"),
        "devin" => include_bytes!("../icons/providers/devin.rgba"),
        "grok" => include_bytes!("../icons/providers/grok.rgba"),
        "opencode" => include_bytes!("../icons/providers/opencode.rgba"),
        "openrouter" => include_bytes!("../icons/providers/openrouter.rgba"),
        "openusage" => include_bytes!("../icons/providers/openusage.rgba"),
        "zai" => include_bytes!("../icons/providers/zai.rgba"),
        _ => return Ok(tray_image()),
    };
    Ok(Image::new(bytes, 32, 32))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn remaining_percent(metric: &Metric) -> Option<u8> {
    (metric.unit == "%")
        .then_some(metric.used?)
        .map(|used| (100.0 - used).clamp(0.0, 100.0).round() as u8)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn menu_bar_limits(provider: &ProviderSnapshot) -> [(String, Option<u8>); 2] {
    let percentages: Vec<&Metric> = provider
        .metrics
        .iter()
        .filter(|metric| metric.unit == "%" && metric.used.is_some())
        .collect();
    let find = |terms: &[&str]| {
        percentages.iter().copied().find(|metric| {
            let label = metric.label.to_ascii_lowercase();
            terms.iter().any(|term| label.contains(term))
        })
    };
    let (top_label, top, bottom_label, bottom) = if provider.id == "cursor" {
        ("Auto", find(&["auto", "composer"]), "API", find(&["api"]))
    } else {
        (
            "Weekly",
            find(&["weekly", "week", "seven_day"]),
            "Daily",
            find(&["daily", "session", "five_hour"]),
        )
    };
    let top = top.or_else(|| percentages.first().copied());
    let bottom = bottom.or_else(|| {
        percentages
            .iter()
            .copied()
            .find(|metric| top.is_none_or(|selected| selected.id != metric.id))
    });
    [
        (top_label.to_owned(), top.and_then(remaining_percent)),
        (bottom_label.to_owned(), bottom.and_then(remaining_percent)),
    ]
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn build_menu_bar_items(
    app: &tauri::AppHandle,
    dashboard: &Dashboard,
    settings: &AppSettings,
) -> Result<(), String> {
    for slot in 0..3 {
        let tray_id = format!("provider-limit-{slot}");
        let Some(provider_id) = settings.menu_bar_providers.get(slot) else {
            let _ = app.remove_tray_by_id(&tray_id);
            continue;
        };
        let Some(provider) = dashboard
            .providers
            .iter()
            .find(|item| &item.id == provider_id)
        else {
            let _ = app.remove_tray_by_id(&tray_id);
            continue;
        };
        let limits = menu_bar_limits(provider);
        let title = limits
            .iter()
            .map(|(_, value)| value.map_or_else(|| "--".to_owned(), |value| format!("{value}%")))
            .collect::<Vec<_>>()
            .join("\n");
        let tooltip = format!(
            "{}\n{}: {} còn lại\n{}: {} còn lại",
            provider.name,
            limits[0].0,
            limits[0]
                .1
                .map_or_else(|| "--".to_owned(), |value| format!("{value}%")),
            limits[1].0,
            limits[1]
                .1
                .map_or_else(|| "--".to_owned(), |value| format!("{value}%")),
        );
        let icon = provider_tray_icon(provider_id)?;
        if let Some(tray) = app.tray_by_id(&tray_id) {
            tray.set_icon(Some(icon))
                .map_err(|error| error.to_string())?;
            tray.set_title(Some(&title))
                .map_err(|error| error.to_string())?;
            tray.set_tooltip(Some(&tooltip))
                .map_err(|error| error.to_string())?;
        } else {
            TrayIconBuilder::with_id(tray_id.clone())
                .icon(icon)
                .icon_as_template(true)
                .title(&title)
                .tooltip(&tooltip)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        }
                    ) {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_menu_bar_items(
    app: &tauri::AppHandle,
    dashboard: &Dashboard,
    settings: &AppSettings,
) -> Result<(), String> {
    build_menu_bar_items(app, dashboard, settings)
}

#[cfg(not(target_os = "macos"))]
fn sync_menu_bar_items(
    _app: &tauri::AppHandle,
    _dashboard: &Dashboard,
    _settings: &AppSettings,
) -> Result<(), String> {
    Ok(())
}

fn toggle_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.eval("window.refreshAll && window.refreshAll()");
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(AppState {
            dashboard: Mutex::new(Dashboard {
                providers: providers::registry(),
                last_refresh: None,
                refreshing: false,
            }),
            settings: Mutex::new(load_settings()),
            quota_alert_levels: Mutex::new(HashMap::new()),
            pet_alerts: Mutex::new(Vec::new()),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("HTTP client"),
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            get_app_settings,
            save_app_settings,
            notify_quota_alert,
            take_pet_alerts,
            check_for_update,
            open_release_page,
            refresh_all,
            refresh_provider,
            scan_local,
            save_api_key,
            get_agent_activity,
            set_pet_expanded,
            show_main_window,
            toggle_pet_visibility
        ])
        .setup(|app| {
            if std::env::args().any(|argument| argument == "--minimized") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            let toggle = MenuItem::with_id(app, "toggle", "Mở / ẩn CodeUsage", true, None::<&str>)?;
            let refresh = MenuItem::with_id(app, "refresh", "Làm mới tất cả", true, None::<&str>)?;
            let pet = MenuItem::with_id(app, "pet", "Hiện / ẩn Agent Pet", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Thoát CodeUsage", true, None::<&str>)?;
            let separator_one = PredefinedMenuItem::separator(app)?;
            let separator_two = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[
                    &toggle,
                    &pet,
                    &separator_one,
                    &refresh,
                    &separator_two,
                    &quit,
                ],
            )?;
            TrayIconBuilder::new()
                .icon(tray_image())
                .tooltip("CodeUsage - Theo dõi quota AI")
                .show_menu_on_left_click(false)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => toggle_main_window(app),
                    "refresh" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.eval("window.refreshAll && window.refreshAll()");
                        }
                    }
                    "pet" => toggle_pet_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        }
                    ) {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            {
                let state = app.state::<AppState>();
                let dashboard = state
                    .dashboard
                    .lock()
                    .map(|value| value.clone())
                    .unwrap_or_else(|_| Dashboard {
                        providers: Vec::new(),
                        last_refresh: None,
                        refreshing: false,
                    });
                let settings = state
                    .settings
                    .lock()
                    .map(|value| value.clone())
                    .unwrap_or_default();
                sync_menu_bar_items(app.handle(), &dashboard, &settings)
                    .map_err(std::io::Error::other)?;
            }
            if let Some(pet) = app.get_webview_window("pet") {
                let settings = app
                    .state::<AppState>()
                    .settings
                    .lock()
                    .ok()
                    .map(|settings| settings.clone())
                    .unwrap_or_default();
                if settings.pet_enabled {
                    let _ = anchor_pet_window(&pet);
                } else {
                    let _ = pet.hide();
                }
            }
            let launch_at_startup = app
                .state::<AppState>()
                .settings
                .lock()
                .ok()
                .is_some_and(|settings| settings.launch_at_startup);
            if launch_at_startup {
                let _ = app.autolaunch().enable();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running CodeUsage")
}

#[cfg(test)]
mod settings_tests {
    use super::{menu_bar_limits, smart_alert_level, version_parts};
    use crate::model::{LocalPresence, Metric, ProviderSnapshot, ProviderStatus};

    fn percent_metric(id: &str, label: &str, used: f64) -> Metric {
        Metric::percent(id, label, used, None)
    }

    fn provider(id: &str, metrics: Vec<Metric>) -> ProviderSnapshot {
        ProviderSnapshot {
            id: id.to_owned(),
            name: id.to_owned(),
            color: String::new(),
            status: ProviderStatus::Ready,
            plan: None,
            metrics,
            message: None,
            refreshed_at: None,
            experimental: false,
            local: LocalPresence::default(),
        }
    }

    #[test]
    fn smart_alerts_respect_configured_threshold_and_escalate() {
        assert_eq!(smart_alert_level(84.0, 85), None);
        assert_eq!(smart_alert_level(85.0, 85), Some(85));
        assert_eq!(smart_alert_level(94.0, 90), Some(90));
        assert_eq!(smart_alert_level(96.0, 90), Some(95));
        assert_eq!(smart_alert_level(86.0, 90), None);
    }

    #[test]
    fn versions_are_compared_numerically() {
        assert!(version_parts("v0.1.10") > version_parts("0.1.2"));
        assert_eq!(version_parts("0.1.1-beta.1"), [0, 1, 1]);
    }

    #[test]
    fn menu_bar_orders_weekly_above_daily_and_shows_remaining() {
        let limits = menu_bar_limits(&provider(
            "claude",
            vec![
                percent_metric("session", "Session", 64.0),
                percent_metric("weekly", "Weekly", 0.0),
            ],
        ));
        assert_eq!(
            limits,
            [
                ("Weekly".to_owned(), Some(100)),
                ("Daily".to_owned(), Some(36))
            ]
        );
    }

    #[test]
    fn menu_bar_uses_cursor_auto_and_api_order() {
        let limits = menu_bar_limits(&provider(
            "cursor",
            vec![
                percent_metric("api", "API", 100.0),
                percent_metric("auto", "Auto + Composer", 7.0),
            ],
        ));
        assert_eq!(
            limits,
            [("Auto".to_owned(), Some(93)), ("API".to_owned(), Some(0))]
        );
    }
}
