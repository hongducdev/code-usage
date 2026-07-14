#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod model;
mod providers;
mod scanner;

use model::{AgentActivity, AppSettings, Dashboard, UpdateStatus};
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
use tauri_plugin_notification::NotificationExt;

const PET_COLLAPSED_WIDTH: u32 = 116;
const PET_COLLAPSED_HEIGHT: u32 = 132;
const PET_EXPANDED_WIDTH: u32 = 376;
const PET_EXPANDED_HEIGHT: u32 = 276;

struct AppState {
    dashboard: Mutex<Dashboard>,
    settings: Mutex<AppSettings>,
    quota_alert_levels: Mutex<HashMap<String, u8>>,
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
    Ok(settings)
}

fn send_native_notification(app: &tauri::AppHandle, title: &str, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn send_test_notification(app: tauri::AppHandle) -> Result<(), String> {
    send_native_notification(&app, "CodeUsage", "Thông báo đang hoạt động bình thường.")
}

#[tauri::command]
fn notify_agent_event(
    workspace: String,
    status: String,
    message: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let settings = state
        .settings
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    if !settings.notifications_enabled || !settings.agent_notifications {
        return Ok(());
    }
    let title = match status.as_str() {
        "needs_approval" => "Agent cần bạn xử lý",
        "completed" => "Agent đã hoàn thành",
        "error" => "Agent bị gián đoạn",
        _ => return Ok(()),
    };
    let body = if settings.privacy_mode {
        "Mở CodeUsage để xem chi tiết.".to_owned()
    } else {
        format!("{workspace}: {message}")
    };
    send_native_notification(&app, title, &body)
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
    app: tauri::AppHandle,
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
    if !settings.notifications_enabled || !settings.smart_alerts_enabled {
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
    send_native_notification(
        &app,
        &format!("{provider} sắp hết quota"),
        &format!("{metric}: đã dùng {}%, còn {remaining}%.", used.round()),
    )
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
async fn refresh_all(state: State<'_, AppState>) -> Result<Dashboard, String> {
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
    let mut d = state.dashboard.lock().map_err(|e| e.to_string())?;
    d.providers = refreshed;
    d.refreshing = false;
    d.last_refresh = Some(chrono::Utc::now().to_rfc3339());
    Ok(d.clone())
}

#[tauri::command]
async fn refresh_provider(id: String, state: State<'_, AppState>) -> Result<Dashboard, String> {
    let snapshot = {
        let d = state.dashboard.lock().map_err(|e| e.to_string())?;
        d.providers
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or("Provider không tồn tại")?
    };
    let refreshed = providers::refresh(snapshot, &state.client).await;
    let mut d = state.dashboard.lock().map_err(|e| e.to_string())?;
    if let Some(slot) = d.providers.iter_mut().find(|p| p.id == id) {
        *slot = refreshed;
    }
    d.last_refresh = Some(chrono::Utc::now().to_rfc3339());
    Ok(d.clone())
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
        .plugin(tauri_plugin_notification::init())
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
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("HTTP client"),
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            get_app_settings,
            save_app_settings,
            send_test_notification,
            notify_agent_event,
            notify_quota_alert,
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
    use super::{smart_alert_level, version_parts};

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
}
