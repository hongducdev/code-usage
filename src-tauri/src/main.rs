#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod providers;
mod scanner;

use model::{AgentActivity, Dashboard};
use reqwest::Client;
use std::sync::Mutex;
use tauri::{
    Manager, PhysicalPosition, PhysicalSize, State,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

const PET_COLLAPSED_WIDTH: u32 = 116;
const PET_COLLAPSED_HEIGHT: u32 = 132;
const PET_EXPANDED_WIDTH: u32 = 376;
const PET_EXPANDED_HEIGHT: u32 = 276;

struct AppState {
    dashboard: Mutex<Dashboard>,
    client: Client,
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
        .manage(AppState {
            dashboard: Mutex::new(Dashboard {
                providers: providers::registry(),
                last_refresh: None,
                refreshing: false,
            }),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("HTTP client"),
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
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
                let _ = anchor_pet_window(&pet);
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
