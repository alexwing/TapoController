//! TapoController desktop app: Tauri IPC + embedded HTTP/WS API server, both
//! driving the shared `ControlService` with multi-bulb and MAC auto-resolution.

mod ambilight;
mod api_server;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tapo_proto::{
    diagnose, BulbConfig, BulbStateSummary, ControlService, Diagnosis, DeviceInfo, TapoConfig,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tokio::sync::Mutex;

/// Shared, hot-swappable control service.
pub type SharedSvc = Arc<Mutex<Arc<ControlService>>>;

pub struct AppState {
    cfg_path: PathBuf,
    cfg: Mutex<TapoConfig>,
    svc: SharedSvc,
    amb: ambilight::Ambilight,
}

/// Effective smoothing for the pipeline: instant => no smoothing.
fn effective_alpha(mode: &str, smoothing: f64) -> f64 {
    if mode == "instant" {
        1.0
    } else {
        smoothing.clamp(0.05, 1.0)
    }
}

/// Resolve `tapo-config.toml`:
fn resolve_config_path() -> PathBuf {
    if let Ok(p) = std::env::var("TAPO_CONFIG") {
        return PathBuf::from(p);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut dir = cwd.as_path();
    for _ in 0..5 {
        let candidate = dir.join("tapo-config.toml");
        if candidate.is_file() {
            return candidate;
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    let base = std::env::var("APPDATA")
        .ok()
        .or_else(|| std::env::var("XDG_CONFIG_HOME").ok())
        .or_else(|| std::env::var("HOME").ok())
        .map(PathBuf::from)
        .unwrap_or(cwd);
    let app_dir = base.join("TapoController");
    let _ = std::fs::create_dir_all(&app_dir);
    app_dir.join("tapo-config.toml")
}

fn build_service(cfg: &TapoConfig) -> Arc<ControlService> {
    ControlService::new_with_config(cfg.clone())
}

async fn current(svc: &SharedSvc) -> Arc<ControlService> {
    svc.lock().await.clone()
}

#[derive(Serialize)]
struct CmdError {
    message: String,
}
impl From<tapo_proto::TapoError> for CmdError {
    fn from(e: tapo_proto::TapoError) -> Self {
        Self {
            message: e.to_string(),
        }
    }
}
type CmdResult<T> = Result<T, CmdError>;

#[derive(Serialize, Deserialize, Clone)]
pub struct UiConfig {
    pub host: String,
    pub username: String,
    pub password: String,
    pub protocol: Option<String>,
    pub bulbs: Vec<BulbConfig>,
    pub active_bulb: String,
    pub sync_all: bool,
    pub api_enabled: bool,
    pub api_bind: String,
    pub api_port: u16,
    pub stream_max_hz: f64,
    pub stream_smoothing: f64,
    pub stream_max_brightness: u8,
    pub stream_mode: String,
    pub stream_monitor: usize,
    pub stream_fps: u32,
    pub language: String,
}

#[tauri::command]
async fn get_config(state: tauri::State<'_, AppState>) -> CmdResult<UiConfig> {
    let c = state.cfg.lock().await;
    Ok(UiConfig {
        host: c.device.host.clone(),
        username: c.device.username.clone(),
        password: c.device.password.clone(),
        protocol: c.device.protocol.clone(),
        bulbs: c.bulbs.clone(),
        active_bulb: c.active_bulb.clone(),
        sync_all: c.sync_all,
        api_enabled: c.api.enabled,
        api_bind: c.api.bind.clone(),
        api_port: c.api.port,
        stream_max_hz: c.stream.max_hz,
        stream_smoothing: c.stream.smoothing,
        stream_max_brightness: c.stream.max_brightness,
        stream_mode: c.stream.mode.clone(),
        stream_monitor: c.stream.monitor,
        stream_fps: c.stream.fps,
        language: c.ui.language.clone(),
    })
}

#[tauri::command]
async fn save_config(
    state: tauri::State<'_, AppState>,
    new: UiConfig,
) -> CmdResult<()> {
    let mut c = state.cfg.lock().await;
    c.device.host = new.host;
    c.device.username = new.username;
    c.device.password = new.password;
    c.device.protocol = new.protocol.filter(|s| !s.is_empty());
    c.bulbs = new.bulbs;
    c.active_bulb = new.active_bulb;
    c.sync_all = new.sync_all;
    c.api.enabled = new.api_enabled;
    c.api.bind = new.api_bind;
    c.api.port = new.api_port;
    c.stream.max_hz = new.stream_max_hz;
    c.stream.smoothing = new.stream_smoothing;
    c.stream.max_brightness = new.stream_max_brightness;
    c.stream.mode = new.stream_mode;
    c.stream.monitor = new.stream_monitor;
    c.stream.fps = new.stream_fps;
    c.ui.language = new.language;
    c.normalize_bulbs();
    c.save(&state.cfg_path).map_err(CmdError::from)?;

    // Apply stream tuning live
    let alpha = effective_alpha(&c.stream.mode, c.stream.smoothing);
    let svc = current(&state.svc).await;
    svc.set_stream_tuning(c.stream.max_hz, alpha, c.stream.max_brightness);
    svc.update_config(&c).await;
    Ok(())
}

#[tauri::command]
async fn get_all_bulbs(state: tauri::State<'_, AppState>) -> CmdResult<Vec<BulbStateSummary>> {
    Ok(current(&state.svc).await.get_all_states().await)
}

#[tauri::command]
async fn set_active_bulb(state: tauri::State<'_, AppState>, target: String) -> CmdResult<()> {
    let svc = current(&state.svc).await;
    svc.set_active_target(target.clone()).await;
    let mut c = state.cfg.lock().await;
    c.active_bulb = target;
    let _ = c.save(&state.cfg_path);
    Ok(())
}

#[tauri::command]
async fn set_sync_all(state: tauri::State<'_, AppState>, sync: bool) -> CmdResult<()> {
    let svc = current(&state.svc).await;
    svc.set_sync_all(sync);
    let mut c = state.cfg.lock().await;
    c.sync_all = sync;
    let _ = c.save(&state.cfg_path);
    Ok(())
}

#[tauri::command]
async fn relocate_bulb(state: tauri::State<'_, AppState>, bulb_id: String) -> CmdResult<String> {
    let svc = current(&state.svc).await;
    let new_ip = svc.relocate_bulb_by_mac(&bulb_id).await?;
    let mut c = state.cfg.lock().await;
    for b in &mut c.bulbs {
        if b.id == bulb_id {
            b.host = new_ip.clone();
        }
    }
    let _ = c.save(&state.cfg_path);
    Ok(new_ip)
}

#[tauri::command]
async fn set_bulb_power(
    state: tauri::State<'_, AppState>,
    bulb_id: String,
    on: bool,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_bulb_power(&bulb_id, on).await?)
}

#[tauri::command]
async fn set_bulb_brightness(
    state: tauri::State<'_, AppState>,
    bulb_id: String,
    value: u8,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_bulb_brightness(&bulb_id, value).await?)
}

#[tauri::command]
async fn set_bulb_color(
    state: tauri::State<'_, AppState>,
    bulb_id: String,
    hue: u16,
    saturation: u8,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_bulb_hue_saturation(&bulb_id, hue, saturation).await?)
}

#[tauri::command]
async fn set_bulb_rgb(
    state: tauri::State<'_, AppState>,
    bulb_id: String,
    r: u8,
    g: u8,
    b: u8,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_bulb_rgb(&bulb_id, r, g, b).await?)
}

#[tauri::command]
async fn set_bulb_color_temp(
    state: tauri::State<'_, AppState>,
    bulb_id: String,
    kelvin: u16,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_bulb_color_temp(&bulb_id, kelvin).await?)
}

#[tauri::command]
async fn set_bulb_white(
    state: tauri::State<'_, AppState>,
    bulb_id: String,
    kelvin: u16,
    brightness: u8,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_bulb_white(&bulb_id, kelvin, brightness).await?)
}

#[tauri::command]
fn list_monitors() -> Vec<ambilight::MonitorInfo> {
    ambilight::list_monitors()
}

#[tauri::command]
async fn get_ambilight(state: tauri::State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.amb.is_running())
}

#[tauri::command]
async fn set_ambilight(
    state: tauri::State<'_, AppState>,
    on: bool,
    monitor: usize,
    fps: u32,
    mode: String,
    smoothing: f64,
    max_hz: f64,
    max_brightness: u8,
) -> CmdResult<()> {
    {
        let mut c = state.cfg.lock().await;
        c.stream.monitor = monitor;
        c.stream.fps = fps;
        c.stream.mode = mode.clone();
        c.stream.smoothing = smoothing;
        c.stream.max_hz = max_hz;
        c.stream.max_brightness = max_brightness;
        let _ = c.save(&state.cfg_path);
    }
    let svc = current(&state.svc).await;
    svc.set_stream_tuning(max_hz, effective_alpha(&mode, smoothing), max_brightness);
    if on {
        svc.set_power(true).await?;
        state.amb.start(svc, monitor, fps);
    } else {
        state.amb.stop();
    }
    Ok(())
}

#[tauri::command]
async fn run_diagnose(
    state: tauri::State<'_, AppState>,
    host: Option<String>,
) -> CmdResult<Diagnosis> {
    let target = match host {
        Some(h) if !h.trim().is_empty() => h,
        _ => state.cfg.lock().await.device.host.clone(),
    };
    Ok(diagnose(&target).await)
}


#[derive(Serialize)]
pub struct DiscoveredBulb {
    pub ip: String,
    pub mac: Option<String>,
    pub klap: bool,
    pub nickname: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
async fn discover_bulbs(state: tauri::State<'_, AppState>) -> CmdResult<Vec<DiscoveredBulb>> {
    let found = tapo_proto::discover().await;
    let (user, pass) = {
        let c = state.cfg.lock().await;
        (c.device.username.clone(), c.device.password.clone())
    };

    let mut out = Vec::with_capacity(found.len());
    for d in found {
        let (mut nickname, mut model, mut mac) = (None, None, d.mac.clone());
        if !user.is_empty() && d.klap {
            let probe = tokio::time::timeout(std::time::Duration::from_secs(4), async {
                let dev = tapo_proto::TapoDevice::connect(&d.ip, &user, &pass).await?;
                dev.get_device_info().await
            })
            .await;
            if let Ok(Ok(info)) = probe {
                if !info.nickname.is_empty() {
                    nickname = Some(info.nickname);
                }
                if !info.model.is_empty() {
                    model = Some(info.model);
                }
                if mac.is_none() && !info.mac.is_empty() {
                    mac = Some(info.mac);
                }
            }
        }
        out.push(DiscoveredBulb {
            ip: d.ip,
            mac,
            klap: d.klap,
            nickname,
            model,
        });
    }
    Ok(out)
}

#[tauri::command]
async fn get_state(state: tauri::State<'_, AppState>) -> CmdResult<DeviceInfo> {
    let svc = current(&state.svc).await;
    Ok(svc.get_state().await?)
}

#[tauri::command]
async fn set_power(state: tauri::State<'_, AppState>, on: bool) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_power(on).await?)
}

#[tauri::command]
async fn set_brightness(state: tauri::State<'_, AppState>, value: u8) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_brightness(value).await?)
}

#[tauri::command]
async fn set_color(
    state: tauri::State<'_, AppState>,
    hue: u16,
    saturation: u8,
) -> CmdResult<()> {
    Ok(current(&state.svc)
        .await
        .set_hue_saturation(hue, saturation)
        .await?)
}

#[tauri::command]
async fn set_rgb(
    state: tauri::State<'_, AppState>,
    r: u8,
    g: u8,
    b: u8,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_rgb(r, g, b).await?)
}

#[tauri::command]
async fn set_color_temp(state: tauri::State<'_, AppState>, kelvin: u16) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_color_temp(kelvin).await?)
}

#[tauri::command]
async fn set_white(
    state: tauri::State<'_, AppState>,
    kelvin: u16,
    brightness: u8,
) -> CmdResult<()> {
    Ok(current(&state.svc).await.set_white(kelvin, brightness).await?)
}

#[tauri::command]
async fn submit_stream_color(
    state: tauri::State<'_, AppState>,
    r: u8,
    g: u8,
    b: u8,
) -> CmdResult<()> {
    current(&state.svc).await.submit_color(r, g, b);
    Ok(())
}

#[tauri::command]
async fn set_animation(
    state: tauri::State<'_, AppState>,
    on: bool,
    speed: Option<u8>,
) -> CmdResult<()> {
    let svc = current(&state.svc).await;
    if on {
        svc.set_power(true).await?;
    }
    svc.set_animation(on, speed);
    Ok(())
}

#[tauri::command]
async fn get_animation(state: tauri::State<'_, AppState>) -> CmdResult<bool> {
    Ok(current(&state.svc).await.animation_enabled())
}

#[tauri::command]
fn open_help_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("help") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "help",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .initialization_script("window.__IS_HELP_WINDOW__ = true;")
    .title("TapoController - Ayuda / Help")
    .inner_size(640.0, 720.0)
    .min_inner_size(440.0, 500.0)
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn show_or_hide(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

fn tray_power(app: &tauri::AppHandle, on: bool) {
    let svc = app.state::<AppState>().svc.clone();
    tauri::async_runtime::spawn(async move {
        let s = svc.lock().await.clone();
        if let Err(e) = s.set_power(on).await {
            tracing::warn!("tray power failed: {e}");
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg_path = resolve_config_path();
    tracing::info!("using config: {}", cfg_path.display());
    let cfg = TapoConfig::load_or_create(&cfg_path).unwrap_or_default();
    let service_arc = build_service(&cfg);
    let svc: SharedSvc = Arc::new(Mutex::new(service_arc.clone()));

    // Background listener for IP auto-healing by MAC address
    let cfg_path_bg = cfg_path.clone();
    let svc_for_bg = svc.clone();
    tauri::async_runtime::spawn(async move {
        let rx_opt = {
            let s = svc_for_bg.lock().await;
            s.take_ip_update_rx().await
        };
        if let Some(mut rx) = rx_opt {
            while let Some((bulb_id, new_host, mac)) = rx.recv().await {
                tracing::info!(
                    "IP auto-healing update for bulb {bulb_id}: IP -> {new_host}, MAC -> {mac}"
                );
                if let Ok(mut c) = TapoConfig::load(&cfg_path_bg) {
                    let mut changed = false;
                    for b in &mut c.bulbs {
                        if b.id == bulb_id {
                            if b.host != new_host {
                                b.host = new_host.clone();
                                changed = true;
                            }
                            if b.mac.is_empty() && !mac.is_empty() {
                                b.mac = mac.clone();
                                changed = true;
                            }
                        }
                    }
                    if changed {
                        let _ = c.save(&cfg_path_bg);
                    }
                }
            }
        }
    });

    // Embedded API server (M3) — shares the very same ControlService.
    if cfg.api.enabled {
        let bind = cfg.api.bind.clone();
        let port = cfg.api.port;
        let svc_for_api = svc.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = api_server::serve(bind, port, svc_for_api).await {
                tracing::error!("API server stopped: {e}");
            }
        });
    }

    let state = AppState {
        cfg_path,
        cfg: Mutex::new(cfg),
        svc,
        amb: ambilight::Ambilight::new(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .setup(|app| {
            let h = app.handle();
            let on = MenuItem::with_id(h, "on", "Encender", true, None::<&str>)?;
            let off = MenuItem::with_id(h, "off", "Apagar", true, None::<&str>)?;
            let toggle =
                MenuItem::with_id(h, "toggle", "Mostrar / Ocultar", true, None::<&str>)?;
            let quit = MenuItem::with_id(h, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(h, &[&on, &off, &toggle, &quit])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("TapoController")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "on" => tray_power(app, true),
                    "off" => tray_power(app, false),
                    "toggle" => show_or_hide(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_or_hide(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                } else if window.label() == "help" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
            WindowEvent::Resized(_) => {
                if window.label() == "main" && window.is_minimized().unwrap_or(false) {
                    let _ = window.unminimize();
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_all_bulbs,
            set_active_bulb,
            set_sync_all,
            relocate_bulb,
            set_bulb_power,
            set_bulb_brightness,
            set_bulb_color,
            set_bulb_rgb,
            set_bulb_color_temp,
            set_bulb_white,
            run_diagnose,
            discover_bulbs,
            get_state,
            set_power,
            set_brightness,
            set_color,
            set_rgb,
            set_color_temp,
            set_white,
            submit_stream_color,
            set_animation,
            get_animation,
            list_monitors,
            get_ambilight,
            set_ambilight,
            open_help_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running TapoController");
}
