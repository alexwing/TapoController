//! `ControlService` — the single source of truth for device control.
//!
//! Holds persistent sessions for multiple bulbs (auto-reconnect on transient failures,
//! auto-discovery/fallback by hardware MAC address if IP changed), and a coalescing,
//! rate-limited color pipeline for the ambilight/stream use case.
//! Both the Tauri IPC layer and the embedded HTTP/WS API server consume this
//! one service so they share sessions, multi-bulb synchronization, and back-pressure.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::config::{BulbConfig, DeviceConfig, StreamConfig, TapoConfig};
use crate::device::{DeviceInfo, Protocol, TapoDevice};
use crate::error::{Result, TapoError};

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Power(bool),
    Brightness(u8),
    Hsv(u16, u8),
    ColorBri(u16, u8, u8),
    ColorTemp(u16),
    White(u16, u8),
}

impl TapoError {
    /// Transient errors worth a single reconnect+retry.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            TapoError::Http(_)
                | TapoError::RequestStatus(_)
                | TapoError::ResponseSignatureMismatch
                | TapoError::NotConnected
                | TapoError::InvalidPayload
        )
    }
}

async fn connect_device(
    host: &str,
    username: &str,
    password: &str,
    protocol: Option<&str>,
) -> Result<TapoDevice> {
    let proto = match protocol {
        Some("klap") => Some(Protocol::Klap),
        Some("passthrough") => Some(Protocol::Passthrough),
        _ => None,
    };
    match proto {
        Some(p) => TapoDevice::connect_with(host, username, password, p).await,
        None => TapoDevice::connect(host, username, password).await,
    }
}

/// Represents an individual bulb connection handle with MAC-based self-healing.
pub struct BulbHandle {
    pub cfg: Mutex<BulbConfig>,
    dev: Mutex<Option<Arc<TapoDevice>>>,
    default_user: String,
    default_pass: String,
    ip_updater: Option<mpsc::UnboundedSender<(String, String, String)>>,
}

impl BulbHandle {
    pub fn new(
        cfg: BulbConfig,
        default_user: String,
        default_pass: String,
        ip_updater: Option<mpsc::UnboundedSender<(String, String, String)>>,
    ) -> Self {
        Self {
            cfg: Mutex::new(cfg),
            dev: Mutex::new(None),
            default_user,
            default_pass,
            ip_updater,
        }
    }

    pub async fn id(&self) -> String {
        self.cfg.lock().await.id.clone()
    }

    pub async fn name(&self) -> String {
        self.cfg.lock().await.name.clone()
    }

    pub async fn is_enabled(&self) -> bool {
        self.cfg.lock().await.enabled
    }

    pub async fn invalidate(&self) {
        *self.dev.lock().await = None;
    }

    /// Retrieve or establish a connection to the bulb.
    /// If connecting to `host` fails and a MAC is configured, it automatically
    /// resolves the new IP on the local network by MAC and reconnects!
    pub async fn device(&self) -> Result<Arc<TapoDevice>> {
        let mut guard = self.dev.lock().await;
        if let Some(d) = guard.as_ref() {
            return Ok(d.clone());
        }

        let (host, user, pass, protocol, mac, id) = {
            let c = self.cfg.lock().await;
            let (u, p) = c.effective_credentials(&self.default_user, &self.default_pass);
            (
                c.host.clone(),
                u.to_string(),
                p.to_string(),
                c.protocol.clone(),
                c.mac.clone(),
                c.id.clone(),
            )
        };

        if user.is_empty() {
            return Err(TapoError::InvalidParam(
                "credentials not configured (edit tapo-config.toml)".into(),
            ));
        }

        let connect_res = if !host.is_empty() {
            connect_device(&host, &user, &pass, protocol.as_deref()).await
        } else {
            Err(TapoError::NotConnected)
        };

        let dev = match connect_res {
            Ok(d) => {
                let d = Arc::new(d);
                // If MAC address was empty in config, learn it from device info!
                if mac.is_empty() {
                    let d_clone = d.clone();
                    let id_clone = id.clone();
                    let host_clone = host.clone();
                    let tx_clone = self.ip_updater.clone();
                    tokio::spawn(async move {
                        if let Ok(info) = d_clone.get_device_info().await {
                            if !info.mac.is_empty() {
                                if let Some(tx) = tx_clone {
                                    let _ = tx.send((id_clone, host_clone, info.mac));
                                }
                            }
                        }
                    });
                }
                d
            }
            Err(e) => {
                // Connection failed! If MAC is stored, attempt auto-resolution
                if !mac.is_empty() {
                    tracing::warn!(
                        "Bulb {} unreachable at '{}' ({}); scanning network for MAC '{}'...",
                        id,
                        host,
                        e,
                        mac
                    );
                    match crate::discover::resolve_ip_by_mac_with_auth(&mac, &user, &pass).await {
                        Some(new_ip) => {
                            tracing::info!(
                                "Bulb {} found at relocated IP '{}'! Reconnecting...",
                                id,
                                new_ip
                            );
                            let d =
                                connect_device(&new_ip, &user, &pass, protocol.as_deref()).await?;
                            let d = Arc::new(d);
                            {
                                let mut c = self.cfg.lock().await;
                                c.host = new_ip.clone();
                            }
                            if let Some(ref tx) = self.ip_updater {
                                let _ = tx.send((id.clone(), new_ip, mac.clone()));
                            }
                            d
                        }
                        None => {
                            tracing::warn!(
                                "Bulb {} with MAC '{}' not found on local network.",
                                id,
                                mac
                            );
                            return Err(e);
                        }
                    }
                } else {
                    return Err(e);
                }
            }
        };

        *guard = Some(dev.clone());
        Ok(dev)
    }

    pub async fn exec(&self, a: Action) -> Result<()> {
        let mut last = TapoError::NotConnected;
        for attempt in 0..2 {
            let dev = self.device().await?;
            match ControlService::dispatch(&dev, a).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt == 0 && e.is_transient() => {
                    tracing::warn!("transient error, reconnecting: {e}");
                    self.invalidate().await;
                    last = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    pub async fn get_state(&self) -> Result<DeviceInfo> {
        let mut last = TapoError::NotConnected;
        for attempt in 0..2 {
            let dev = self.device().await?;
            match dev.get_device_info().await {
                Ok(info) => {
                    // Update MAC if needed
                    let mut c = self.cfg.lock().await;
                    if c.mac.is_empty() && !info.mac.is_empty() {
                        c.mac = info.mac.clone();
                        if let Some(ref tx) = self.ip_updater {
                            let _ = tx.send((c.id.clone(), c.host.clone(), info.mac.clone()));
                        }
                    }
                    return Ok(info);
                }
                Err(e) if attempt == 0 && e.is_transient() => {
                    self.invalidate().await;
                    last = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// Explicitly search LAN by stored MAC and relocate IP.
    pub async fn relocate_by_mac(&self) -> Result<String> {
        let (mac, user, pass, protocol, id) = {
            let c = self.cfg.lock().await;
            let (u, p) = c.effective_credentials(&self.default_user, &self.default_pass);
            (
                c.mac.clone(),
                u.to_string(),
                p.to_string(),
                c.protocol.clone(),
                c.id.clone(),
            )
        };
        if mac.is_empty() {
            return Err(TapoError::InvalidParam(
                "No MAC address configured for this bulb".into(),
            ));
        }
        let new_ip = crate::discover::resolve_ip_by_mac_with_auth(&mac, &user, &pass)
            .await
            .ok_or_else(|| {
                TapoError::InvalidParam(format!("Could not find bulb with MAC {mac} on LAN"))
            })?;

        let dev = connect_device(&new_ip, &user, &pass, protocol.as_deref()).await?;
        *self.dev.lock().await = Some(Arc::new(dev));
        {
            let mut c = self.cfg.lock().await;
            c.host = new_ip.clone();
        }
        if let Some(ref tx) = self.ip_updater {
            let _ = tx.send((id, new_ip.clone(), mac));
        }
        Ok(new_ip)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulbStateSummary {
    pub id: String,
    pub name: String,
    pub host: String,
    pub mac: String,
    pub enabled: bool,
    pub online: bool,
    pub state: Option<DeviceInfo>,
    pub error: Option<String>,
}

pub struct ControlService {
    bulbs: Mutex<Vec<Arc<BulbHandle>>>,
    active_target: Mutex<String>,
    sync_all: AtomicBool,
    default_user: Mutex<String>,
    default_pass: Mutex<String>,
    color_tx: mpsc::UnboundedSender<(u8, u8, u8)>,
    anim_on: AtomicBool,
    /// Milliseconds between hue steps (lower = faster rainbow).
    anim_step_ms: AtomicU64,
    anim_hue: AtomicU32,
    /// Runtime-tunable stream pipeline params. Stored *1000 as ints.
    stream_max_hz_milli: AtomicU64,
    stream_alpha_milli: AtomicU64,
    /// Ambilight brightness ceiling 1..=100.
    stream_max_bri: AtomicU32,
    ip_update_tx: mpsc::UnboundedSender<(String, String, String)>,
    ip_update_rx: Mutex<Option<mpsc::UnboundedReceiver<(String, String, String)>>>,
}

impl ControlService {
    /// Build service from legacy single-device config (backward compatibility).
    pub fn new(cfg: DeviceConfig, stream_cfg: StreamConfig) -> Arc<Self> {
        let bulb = BulbConfig {
            id: "bulb-1".into(),
            name: "Bombilla 1".into(),
            host: cfg.host.clone(),
            mac: String::new(),
            username: None,
            password: None,
            protocol: cfg.protocol.clone(),
            enabled: true,
        };
        let mut tapo_cfg = TapoConfig::default();
        tapo_cfg.device = cfg;
        tapo_cfg.bulbs = vec![bulb];
        tapo_cfg.stream = stream_cfg;
        tapo_cfg.active_bulb = "all".into();
        tapo_cfg.sync_all = true;
        Self::new_with_config(tapo_cfg)
    }

    /// Build service from a full TapoConfig supporting multi-bulb and synchronization.
    pub fn new_with_config(cfg: TapoConfig) -> Arc<Self> {
        let (color_tx, color_rx) = mpsc::unbounded_channel();
        let (ip_tx, ip_rx) = mpsc::unbounded_channel();

        let max_hz0 = (cfg.stream.max_hz.max(0.5) * 1000.0) as u64;
        let alpha0 = (cfg.stream.smoothing.clamp(0.05, 1.0) * 1000.0) as u64;
        let maxbri0 = cfg.stream.max_brightness.clamp(1, 100) as u32;

        let mut handles = Vec::new();
        for b in &cfg.bulbs {
            handles.push(Arc::new(BulbHandle::new(
                b.clone(),
                cfg.device.username.clone(),
                cfg.device.password.clone(),
                Some(ip_tx.clone()),
            )));
        }

        let active_target = if cfg.active_bulb.is_empty() {
            "all".to_string()
        } else {
            cfg.active_bulb.clone()
        };

        let svc = Arc::new(Self {
            bulbs: Mutex::new(handles),
            active_target: Mutex::new(active_target),
            sync_all: AtomicBool::new(cfg.sync_all),
            default_user: Mutex::new(cfg.device.username),
            default_pass: Mutex::new(cfg.device.password),
            color_tx,
            anim_on: AtomicBool::new(false),
            anim_step_ms: AtomicU64::new(120),
            anim_hue: AtomicU32::new(0),
            stream_max_hz_milli: AtomicU64::new(max_hz0),
            stream_alpha_milli: AtomicU64::new(alpha0),
            stream_max_bri: AtomicU32::new(maxbri0),
            ip_update_tx: ip_tx,
            ip_update_rx: Mutex::new(Some(ip_rx)),
        });

        let driver = {
            let s = svc.clone();
            async move {
                tokio::join!(color_pipeline(s.clone(), color_rx), animation_loop(s));
            }
        };

        match tokio::runtime::Handle::try_current() {
            Ok(_) => {
                tokio::spawn(driver);
            }
            Err(_) => {
                std::thread::Builder::new()
                    .name("tapo-color-pipeline".into())
                    .spawn(move || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("color pipeline runtime");
                        rt.block_on(driver);
                    })
                    .expect("spawn color pipeline thread");
            }
        }

        svc
    }

    /// Retrieve the IP update receiver channel (used by Tauri backend to persist config changes).
    pub async fn take_ip_update_rx(
        &self,
    ) -> Option<mpsc::UnboundedReceiver<(String, String, String)>> {
        self.ip_update_rx.lock().await.take()
    }

    /// Update bulb list and credentials without restarting the service.
    pub async fn update_config(&self, cfg: &TapoConfig) {
        *self.default_user.lock().await = cfg.device.username.clone();
        *self.default_pass.lock().await = cfg.device.password.clone();
        *self.active_target.lock().await = cfg.active_bulb.clone();
        self.sync_all.store(cfg.sync_all, Ordering::Relaxed);

        let mut current = self.bulbs.lock().await;
        let mut new_handles = Vec::with_capacity(cfg.bulbs.len());
        for b in &cfg.bulbs {
            if let Some(existing) = current.iter().find(|h| {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(h.id()) == b.id
                })
            }) {
                *existing.cfg.lock().await = b.clone();
                new_handles.push(existing.clone());
            } else {
                new_handles.push(Arc::new(BulbHandle::new(
                    b.clone(),
                    cfg.device.username.clone(),
                    cfg.device.password.clone(),
                    Some(self.ip_update_tx.clone()),
                )));
            }
        }
        *current = new_handles;
    }

    pub async fn active_target(&self) -> String {
        self.active_target.lock().await.clone()
    }

    pub async fn set_active_target(&self, target: String) {
        *self.active_target.lock().await = target;
    }

    pub fn sync_all(&self) -> bool {
        self.sync_all.load(Ordering::Relaxed)
    }

    pub fn set_sync_all(&self, val: bool) {
        self.sync_all.store(val, Ordering::Relaxed);
    }

    /// Helper to resolve target bulb handles for an operation.
    pub async fn resolve_targets(&self, explicit_target: Option<&str>) -> Vec<Arc<BulbHandle>> {
        let all = self.bulbs.lock().await.clone();
        let active = self.active_target.lock().await.clone();
        let target = explicit_target.unwrap_or(active.as_str());

        if target == "all" || self.sync_all() {
            let mut list = Vec::new();
            for h in all {
                if h.is_enabled().await {
                    list.push(h);
                }
            }
            list
        } else if target.contains(',') {
            let ids: std::collections::HashSet<&str> = target.split(',').map(|s| s.trim()).collect();
            let mut list = Vec::new();
            for h in all {
                let id = h.id().await;
                if ids.contains(id.as_str()) && h.is_enabled().await {
                    list.push(h);
                }
            }
            list
        } else {
            let mut list = Vec::new();
            for h in all {
                if h.id().await == target {
                    list.push(h);
                    break;
                }
            }
            list
        }
    }

    pub async fn dispatch(dev: &TapoDevice, a: Action) -> Result<()> {
        match a {
            Action::Power(on) => dev.set_power(on).await,
            Action::Brightness(v) => dev.set_brightness(v).await,
            Action::Hsv(h, s) => dev.set_hue_saturation(h, s).await,
            Action::ColorBri(h, s, b) => dev.set_color_brightness(h, s, b).await,
            Action::ColorTemp(k) => dev.set_color_temp(k).await,
            Action::White(k, b) => dev.set_white(k, b).await,
        }
    }

    async fn exec_on_targets(&self, target: Option<&str>, a: Action) -> Result<()> {
        let targets = self.resolve_targets(target).await;
        if targets.is_empty() {
            return Err(TapoError::NotConnected);
        }

        let mut tasks = Vec::with_capacity(targets.len());
        for h in targets {
            tasks.push(async move { h.exec(a).await });
        }
        let results = futures_util::future::join_all(tasks).await;

        let mut any_ok = false;
        let mut first_err = None;
        for r in results {
            match r {
                Ok(()) => any_ok = true,
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        if any_ok {
            Ok(())
        } else if let Some(e) = first_err {
            Err(e)
        } else {
            Err(TapoError::NotConnected)
        }
    }

    // ---- Public control API ----

    pub async fn set_power(&self, on: bool) -> Result<()> {
        self.exec_on_targets(None, Action::Power(on)).await
    }
    pub async fn set_bulb_power(&self, bulb_id: &str, on: bool) -> Result<()> {
        self.exec_on_targets(Some(bulb_id), Action::Power(on)).await
    }

    pub async fn set_brightness(&self, v: u8) -> Result<()> {
        self.exec_on_targets(None, Action::Brightness(v)).await
    }
    pub async fn set_bulb_brightness(&self, bulb_id: &str, v: u8) -> Result<()> {
        self.exec_on_targets(Some(bulb_id), Action::Brightness(v)).await
    }

    pub async fn set_hue_saturation(&self, h: u16, s: u8) -> Result<()> {
        self.exec_on_targets(None, Action::Hsv(h, s)).await
    }
    pub async fn set_bulb_hue_saturation(&self, bulb_id: &str, h: u16, s: u8) -> Result<()> {
        self.exec_on_targets(Some(bulb_id), Action::Hsv(h, s)).await
    }

    pub async fn set_color_temp(&self, k: u16) -> Result<()> {
        self.exec_on_targets(None, Action::ColorTemp(k)).await
    }
    pub async fn set_bulb_color_temp(&self, bulb_id: &str, k: u16) -> Result<()> {
        self.exec_on_targets(Some(bulb_id), Action::ColorTemp(k)).await
    }

    pub async fn set_white(&self, kelvin: u16, brightness: u8) -> Result<()> {
        self.exec_on_targets(None, Action::White(kelvin, brightness)).await
    }
    pub async fn set_bulb_white(&self, bulb_id: &str, kelvin: u16, brightness: u8) -> Result<()> {
        self.exec_on_targets(Some(bulb_id), Action::White(kelvin, brightness)).await
    }

    pub async fn set_rgb(&self, r: u8, g: u8, b: u8) -> Result<()> {
        let (h, s, _) = crate::device::rgb_to_hsv(r, g, b);
        self.exec_on_targets(None, Action::Hsv(h, s)).await
    }
    pub async fn set_bulb_rgb(&self, bulb_id: &str, r: u8, g: u8, b: u8) -> Result<()> {
        let (h, s, _) = crate::device::rgb_to_hsv(r, g, b);
        self.exec_on_targets(Some(bulb_id), Action::Hsv(h, s)).await
    }

    /// Read live state of the active target bulb (or first available bulb).
    pub async fn get_state(&self) -> Result<DeviceInfo> {
        let targets = self.resolve_targets(None).await;
        if let Some(first) = targets.first() {
            first.get_state().await
        } else {
            Err(TapoError::NotConnected)
        }
    }

    /// Read state of a specific bulb.
    pub async fn get_bulb_state(&self, bulb_id: &str) -> Result<DeviceInfo> {
        let bulbs = self.bulbs.lock().await.clone();
        for h in bulbs {
            if h.id().await == bulb_id {
                return h.get_state().await;
            }
        }
        Err(TapoError::InvalidParam(format!("Bulb '{bulb_id}' not found")))
    }

    /// Get all bulbs state summary concurrently.
    pub async fn get_all_states(&self) -> Vec<BulbStateSummary> {
        let handles = self.bulbs.lock().await.clone();
        let mut tasks = Vec::with_capacity(handles.len());
        for h in handles {
            tasks.push(async move {
                let (id, name, host, mac, enabled) = {
                    let c = h.cfg.lock().await;
                    (
                        c.id.clone(),
                        c.name.clone(),
                        c.host.clone(),
                        c.mac.clone(),
                        c.enabled,
                    )
                };
                if !enabled {
                    return BulbStateSummary {
                        id,
                        name,
                        host,
                        mac,
                        enabled: false,
                        online: false,
                        state: None,
                        error: None,
                    };
                }
                match h.get_state().await {
                    Ok(info) => BulbStateSummary {
                        id,
                        name,
                        host,
                        mac,
                        enabled: true,
                        online: true,
                        state: Some(info),
                        error: None,
                    },
                    Err(e) => BulbStateSummary {
                        id,
                        name,
                        host,
                        mac,
                        enabled: true,
                        online: false,
                        state: None,
                        error: Some(e.to_string()),
                    },
                }
            });
        }
        futures_util::future::join_all(tasks).await
    }

    /// Relocate bulb IP by its stored MAC address.
    pub async fn relocate_bulb_by_mac(&self, bulb_id: &str) -> Result<String> {
        let bulbs = self.bulbs.lock().await.clone();
        for h in bulbs {
            if h.id().await == bulb_id {
                return h.relocate_by_mac().await;
            }
        }
        Err(TapoError::InvalidParam(format!("Bulb '{bulb_id}' not found")))
    }

    /// Non-blocking: submit an RGB target for the rate-limited stream pipeline.
    /// Broadcasts to all synchronized bulbs concurrently.
    pub fn submit_color(&self, r: u8, g: u8, b: u8) {
        let _ = self.color_tx.send((r, g, b));
    }

    /// Enable/disable animated rainbow mode.
    pub fn set_animation(&self, on: bool, speed: Option<u8>) {
        if let Some(sp) = speed {
            let sp = sp.clamp(1, 100) as u64;
            let trans_ms = 4000 - (sp - 1) * (4000 - 400) / 99;
            self.anim_step_ms.store(trans_ms, Ordering::Relaxed);
        }
        self.anim_on.store(on, Ordering::Relaxed);
    }

    pub fn animation_enabled(&self) -> bool {
        self.anim_on.load(Ordering::Relaxed)
    }

    /// Live-tune the stream pipeline.
    pub fn set_stream_tuning(&self, max_hz: f64, smoothing: f64, max_brightness: u8) {
        self.stream_max_hz_milli
            .store((max_hz.clamp(0.5, 30.0) * 1000.0) as u64, Ordering::Relaxed);
        self.stream_alpha_milli
            .store((smoothing.clamp(0.05, 1.0) * 1000.0) as u64, Ordering::Relaxed);
        self.stream_max_bri
            .store(max_brightness.clamp(1, 100) as u32, Ordering::Relaxed);
    }
}

/// Background task: coalesce + smooth + rate-limit streamed colors, broadcasting to all targets.
async fn color_pipeline(
    svc: Arc<ControlService>,
    mut rx: mpsc::UnboundedReceiver<(u8, u8, u8)>,
) {
    let mut state: Option<(f64, f64, f64)> = None;

    while let Some(mut latest) = rx.recv().await {
        while let Ok(c) = rx.try_recv() {
            latest = c;
        }
        let max_hz = (svc.stream_max_hz_milli.load(Ordering::Relaxed) as f64 / 1000.0).max(0.5);
        let min_interval = Duration::from_secs_f64(1.0 / max_hz);
        let alpha = (svc.stream_alpha_milli.load(Ordering::Relaxed) as f64 / 1000.0)
            .clamp(0.05, 1.0);
        let (tr, tg, tb) = (latest.0 as f64, latest.1 as f64, latest.2 as f64);
        let (sr, sg, sb) = match state {
            Some((r, g, b)) => (
                r + (tr - r) * alpha,
                g + (tg - g) * alpha,
                b + (tb - b) * alpha,
            ),
            None => (tr, tg, tb),
        };
        state = Some((sr, sg, sb));

        let max_bri = svc.stream_max_bri.load(Ordering::Relaxed).clamp(1, 100) as f64;
        let value = sr.max(sg).max(sb) / 255.0;
        let bri = ((value * max_bri).round() as u8).clamp(1, max_bri as u8);
        let (hue, sat, _) =
            crate::device::rgb_to_hsv(sr.round() as u8, sg.round() as u8, sb.round() as u8);

        if let Err(e) = svc
            .exec_on_targets(None, Action::ColorBri(hue, sat, bri))
            .await
        {
            tracing::warn!("stream color apply failed: {e}");
        }
        tokio::time::sleep(min_interval).await;
    }
}

/// Background task: drives on-device dynamic rainbow effect across all synchronized bulbs.
async fn animation_loop(svc: Arc<ControlService>) {
    let mut applied = false;
    let mut applied_trans: u64 = 0;
    let mut fallback = false;

    loop {
        let want = svc.anim_on.load(Ordering::Relaxed);
        let trans = svc.anim_step_ms.load(Ordering::Relaxed).clamp(200, 6000);

        if want && (!applied || trans != applied_trans) && !fallback {
            let targets = svc.resolve_targets(None).await;
            let mut tasks = Vec::with_capacity(targets.len());
            for t in targets {
                let trans_val = trans as u32;
                tasks.push(async move {
                    if let Ok(dev) = t.device().await {
                        dev.set_rainbow_effect(trans_val, 100).await
                    } else {
                        Err(TapoError::NotConnected)
                    }
                });
            }
            let res = futures_util::future::join_all(tasks).await;
            if res.iter().any(|r| r.is_ok()) {
                applied = true;
                applied_trans = trans;
            } else if !res.is_empty() {
                fallback = true;
            }
        } else if !want && applied {
            let targets = svc.resolve_targets(None).await;
            for t in targets {
                if let Ok(dev) = t.device().await {
                    let _ = dev.clear_effect().await;
                }
            }
            applied = false;
        }

        if want && fallback {
            let hue = (svc.anim_hue.load(Ordering::Relaxed) + 3) % 360;
            svc.anim_hue.store(hue, Ordering::Relaxed);
            let (r, g, b) = crate::device::hsv_to_rgb(hue as u16, 100, 100);
            svc.submit_color(r, g, b);
            tokio::time::sleep(Duration::from_millis(60)).await;
        } else {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StreamConfig;

    #[tokio::test]
    async fn submit_color_never_blocks_without_credentials() {
        let svc = ControlService::new(DeviceConfig::default(), StreamConfig::default());
        for i in 0..1000u32 {
            svc.submit_color((i % 255) as u8, 0, 0);
        }
        let err = svc.get_state().await.unwrap_err();
        assert!(matches!(err, TapoError::InvalidParam(_)));
    }

    #[test]
    fn transient_classification() {
        assert!(TapoError::NotConnected.is_transient());
        assert!(!TapoError::DeviceError(-1501).is_transient());
        assert!(!TapoError::AuthMismatch.is_transient());
    }
}
