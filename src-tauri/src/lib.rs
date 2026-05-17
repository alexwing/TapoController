//! TapoController desktop app: Tauri IPC + embedded HTTP/WS API server, both
//! driving the single shared `ControlService`.

mod ambilight;
mod api_server;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tapo_proto::{diagnose, ControlService, Diagnosis, DeviceInfo, TapoConfig};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tokio::sync::Mutex;

/// Shared, hot-swappable control service (rebuilt when credentials change).
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
/// 1. `TAPO_CONFIG` env (explicit override).
/// 2. An existing file walking up from the cwd — dev convenience so running
///    from the repo / `tauri dev` keeps using the repo config.
/// 3. Otherwise the per-user app-data dir (`%APPDATA%\TapoController` on
///    Windows). The install dir / Start-Menu cwd is not writable, which is why
///    credentials weren't persisting in the packaged build.
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
    // Installed app: persist in a writable per-user location.
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
    ControlService::new(cfg.device.clone(), cfg.stream.clone())
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
    let device_changed = c.device.host != new.host
        || c.device.username != new.username
        || c.device.password != new.password
        || c.device.protocol != new.protocol.clone().filter(|s| !s.is_empty());

    c.device.host = new.host;
    c.device.username = new.username;
    c.device.password = new.password;
    c.device.protocol = new.protocol.filter(|s| !s.is_empty());
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
    c.save(&state.cfg_path).map_err(CmdError::from)?;

    // Apply stream tuning live (no service rebuild needed).
    let alpha = effective_alpha(&c.stream.mode, c.stream.smoothing);
    current(&state.svc)
        .await
        .set_stream_tuning(c.stream.max_hz, alpha, c.stream.max_brightness);

    // Only rebuild the session if the device/credentials actually changed.
    if device_changed {
        let fresh = build_service(&c);
        *state.svc.lock().await = fresh;
    }
    Ok(())
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
async fn run_diagnose(state: tauri::State<'_, AppState>) -> CmdResult<Diagnosis> {
    let host = state.cfg.lock().await.device.host.clone();
    Ok(diagnose(&host).await)
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
        // Make sure the bulb is on before the rainbow starts.
        svc.set_power(true).await?;
    }
    svc.set_animation(on, speed);
    Ok(())
}

#[tauri::command]
async fn get_animation(state: tauri::State<'_, AppState>) -> CmdResult<bool> {
    Ok(current(&state.svc).await.animation_enabled())
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
    let svc: SharedSvc = Arc::new(Mutex::new(build_service(&cfg)));

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
            // Close button -> hide to tray instead of quitting.
            WindowEvent::CloseRequested { api, .. } => {
                let _ = window.hide();
                api.prevent_close();
            }
            // Minimize -> live only in the tray (no taskbar button).
            WindowEvent::Resized(_) => {
                if window.is_minimized().unwrap_or(false) {
                    let _ = window.unminimize();
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            run_diagnose,
            get_state,
            set_power,
            set_brightness,
            set_color,
            set_rgb,
            set_color_temp,
            submit_stream_color,
            set_animation,
            get_animation,
            list_monitors,
            get_ambilight,
            set_ambilight
        ])
        .run(tauri::generate_context!())
        .expect("error while running TapoController");
}
