//! `ControlService` — the single source of truth for device control.
//!
//! Holds a persistent session (auto-reconnect on transient failures) and a
//! coalescing, rate-limited color pipeline for the ambilight/stream use case.
//! Both the Tauri IPC layer and the embedded HTTP/WS API server consume this
//! one service so they share session + back-pressure.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

use crate::config::{DeviceConfig, StreamConfig};
use crate::device::{DeviceInfo, Protocol, TapoDevice};
use crate::error::{Result, TapoError};

#[derive(Debug, Clone, Copy)]
enum Action {
    Power(bool),
    Brightness(u8),
    Hsv(u16, u8),
    ColorBri(u16, u8, u8),
    ColorTemp(u16),
}

impl TapoError {
    /// Transient errors worth a single reconnect+retry.
    fn is_transient(&self) -> bool {
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

pub struct ControlService {
    cfg: DeviceConfig,
    dev: Mutex<Option<Arc<TapoDevice>>>,
    color_tx: mpsc::UnboundedSender<(u8, u8, u8)>,
    anim_on: std::sync::atomic::AtomicBool,
    /// Milliseconds between hue steps (lower = faster rainbow).
    anim_step_ms: std::sync::atomic::AtomicU64,
    anim_hue: std::sync::atomic::AtomicU32,
    /// Runtime-tunable stream pipeline params (so the Streaming tab can change
    /// fade/rate live without rebuilding the service). Stored *1000 as ints.
    stream_max_hz_milli: std::sync::atomic::AtomicU64,
    stream_alpha_milli: std::sync::atomic::AtomicU64,
    /// Ambilight brightness ceiling 1..=100 (scene luminance scaled to this).
    stream_max_bri: std::sync::atomic::AtomicU32,
}

impl ControlService {
    /// Build the service and start the background color pipeline.
    ///
    /// Safe to call from anywhere: if a Tokio runtime is already running we
    /// spawn onto it; otherwise (e.g. constructed before Tauri starts its
    /// runtime) we run the pipeline on a dedicated current-thread runtime so
    /// the service is fully self-contained.
    pub fn new(cfg: DeviceConfig, stream_cfg: StreamConfig) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let max_hz0 = (stream_cfg.max_hz.max(0.5) * 1000.0) as u64;
        let alpha0 = (stream_cfg.smoothing.clamp(0.05, 1.0) * 1000.0) as u64;
        let maxbri0 = stream_cfg.max_brightness.clamp(1, 100) as u32;
        let svc = Arc::new(Self {
            cfg,
            dev: Mutex::new(None),
            color_tx: tx,
            anim_on: std::sync::atomic::AtomicBool::new(false),
            anim_step_ms: std::sync::atomic::AtomicU64::new(120),
            anim_hue: std::sync::atomic::AtomicU32::new(0),
            stream_max_hz_milli: std::sync::atomic::AtomicU64::new(max_hz0),
            stream_alpha_milli: std::sync::atomic::AtomicU64::new(alpha0),
            stream_max_bri: std::sync::atomic::AtomicU32::new(maxbri0),
        });
        let driver = {
            let s = svc.clone();
            async move {
                tokio::join!(color_pipeline(s.clone(), rx), animation_loop(s));
            }
        };
        let pipeline = driver;
        match tokio::runtime::Handle::try_current() {
            Ok(_) => {
                tokio::spawn(pipeline);
            }
            Err(_) => {
                std::thread::Builder::new()
                    .name("tapo-color-pipeline".into())
                    .spawn(move || {
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("color pipeline runtime");
                        rt.block_on(pipeline);
                    })
                    .expect("spawn color pipeline thread");
            }
        }
        svc
    }

    fn parse_protocol(&self) -> Option<Protocol> {
        match self.cfg.protocol.as_deref() {
            Some("klap") => Some(Protocol::Klap),
            Some("passthrough") => Some(Protocol::Passthrough),
            _ => None,
        }
    }

    /// Get the current device handle, connecting (handshake) if needed.
    async fn device(&self) -> Result<Arc<TapoDevice>> {
        let mut guard = self.dev.lock().await;
        if let Some(d) = guard.as_ref() {
            return Ok(d.clone());
        }
        if self.cfg.username.is_empty() {
            return Err(TapoError::InvalidParam(
                "credentials not configured (edit tapo-config.toml)".into(),
            ));
        }
        let d = match self.parse_protocol() {
            Some(p) => {
                TapoDevice::connect_with(&self.cfg.host, &self.cfg.username, &self.cfg.password, p)
                    .await?
            }
            None => {
                TapoDevice::connect(&self.cfg.host, &self.cfg.username, &self.cfg.password).await?
            }
        };
        let d = Arc::new(d);
        *guard = Some(d.clone());
        Ok(d)
    }

    async fn invalidate(&self) {
        *self.dev.lock().await = None;
    }

    async fn dispatch(dev: &TapoDevice, a: Action) -> Result<()> {
        match a {
            Action::Power(on) => dev.set_power(on).await,
            Action::Brightness(v) => dev.set_brightness(v).await,
            Action::Hsv(h, s) => dev.set_hue_saturation(h, s).await,
            Action::ColorBri(h, s, b) => dev.set_color_brightness(h, s, b).await,
            Action::ColorTemp(k) => dev.set_color_temp(k).await,
        }
    }

    async fn exec(&self, a: Action) -> Result<()> {
        let mut last = TapoError::NotConnected;
        for attempt in 0..2 {
            let dev = self.device().await?;
            match Self::dispatch(&dev, a).await {
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

    // ---- public control API ----

    pub async fn set_power(&self, on: bool) -> Result<()> {
        self.exec(Action::Power(on)).await
    }
    pub async fn set_brightness(&self, v: u8) -> Result<()> {
        self.exec(Action::Brightness(v)).await
    }
    pub async fn set_hue_saturation(&self, h: u16, s: u8) -> Result<()> {
        self.exec(Action::Hsv(h, s)).await
    }
    pub async fn set_color_temp(&self, k: u16) -> Result<()> {
        self.exec(Action::ColorTemp(k)).await
    }
    pub async fn set_rgb(&self, r: u8, g: u8, b: u8) -> Result<()> {
        let (h, s, _) = crate::device::rgb_to_hsv(r, g, b);
        self.exec(Action::Hsv(h, s)).await
    }

    /// Read live device state (with one reconnect retry).
    pub async fn get_state(&self) -> Result<DeviceInfo> {
        let mut last = TapoError::NotConnected;
        for attempt in 0..2 {
            let dev = self.device().await?;
            match dev.get_device_info().await {
                Ok(info) => return Ok(info),
                Err(e) if attempt == 0 && e.is_transient() => {
                    self.invalidate().await;
                    last = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// Non-blocking: submit an RGB target for the rate-limited stream pipeline.
    /// Intermediate frames are dropped; only the newest is applied.
    pub fn submit_color(&self, r: u8, g: u8, b: u8) {
        let _ = self.color_tx.send((r, g, b));
    }

    /// Enable/disable the animated rainbow mode. `speed` is 1..=100 (higher =
    /// faster); when `None` the current speed is kept.
    pub fn set_animation(&self, on: bool, speed: Option<u8>) {
        use std::sync::atomic::Ordering;
        if let Some(sp) = speed {
            // Map speed 1..=100 to the on-device fade time (ms): higher speed
            // => shorter transition => faster rainbow. This is the bulb's own
            // smooth fade, not a client-side step.
            let sp = sp.clamp(1, 100) as u64;
            let trans_ms = 4000 - (sp - 1) * (4000 - 400) / 99;
            self.anim_step_ms.store(trans_ms, Ordering::Relaxed);
        }
        self.anim_on.store(on, Ordering::Relaxed);
    }

    pub fn animation_enabled(&self) -> bool {
        self.anim_on.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Live-tune the stream pipeline (used by the Streaming/ambilight tab).
    /// `max_hz` = device updates/s cap; `smoothing` 0.05..=1.0 (1.0 = instant,
    /// lower = softer fade).
    pub fn set_stream_tuning(&self, max_hz: f64, smoothing: f64, max_brightness: u8) {
        use std::sync::atomic::Ordering;
        self.stream_max_hz_milli
            .store((max_hz.clamp(0.5, 30.0) * 1000.0) as u64, Ordering::Relaxed);
        self.stream_alpha_milli
            .store((smoothing.clamp(0.05, 1.0) * 1000.0) as u64, Ordering::Relaxed);
        self.stream_max_bri
            .store(max_brightness.clamp(1, 100) as u32, Ordering::Relaxed);
    }
}

/// Background task: coalesce + smooth + rate-limit streamed colors.
async fn color_pipeline(
    svc: Arc<ControlService>,
    mut rx: mpsc::UnboundedReceiver<(u8, u8, u8)>,
) {
    use std::sync::atomic::Ordering;
    let mut state: Option<(f64, f64, f64)> = None;

    while let Some(mut latest) = rx.recv().await {
        // Drain everything queued; keep only the newest target.
        while let Ok(c) = rx.try_recv() {
            latest = c;
        }
        // Read live-tunable params each iteration.
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

        // Scene luminance (0..100) -> bulb brightness, scaled to the user's
        // max brightness ceiling. Dark scene = dim, bright scene = up to max.
        let max_bri = svc.stream_max_bri.load(Ordering::Relaxed).clamp(1, 100) as f64;
        let value = sr.max(sg).max(sb) / 255.0; // 0..1
        let bri = ((value * max_bri).round() as u8).clamp(1, max_bri as u8);
        let (hue, sat, _) =
            crate::device::rgb_to_hsv(sr.round() as u8, sg.round() as u8, sb.round() as u8);

        if let Err(e) = svc.exec(Action::ColorBri(hue, sat, bri)).await {
            tracing::warn!("stream color apply failed: {e}");
        }
        tokio::time::sleep(min_interval).await;
    }
}

/// Background task: drives the bulb's **on-device** dynamic rainbow effect.
/// Edge-triggered — we send ONE `set_lighting_effect` when enabled (and again
/// only if the speed changes) and clear it when disabled. The smooth fades are
/// produced by the bulb's firmware, exactly like the Tapo app, so there are no
/// jumps and no per-frame network traffic.
async fn animation_loop(svc: Arc<ControlService>) {
    use std::sync::atomic::Ordering;
    let mut applied = false;
    let mut applied_trans: u64 = 0;
    let mut fallback = false; // bulb rejected the effect -> client-side sweep

    loop {
        let want = svc.anim_on.load(Ordering::Relaxed);
        let trans = svc.anim_step_ms.load(Ordering::Relaxed).clamp(200, 6000);

        if want && (!applied || trans != applied_trans) && !fallback {
            match svc.device().await {
                Ok(dev) => match dev.set_rainbow_effect(trans as u32, 100).await {
                    Ok(()) => {
                        applied = true;
                        applied_trans = trans;
                    }
                    Err(e) => {
                        tracing::warn!("dynamic effect unsupported ({e}); using fallback sweep");
                        fallback = true;
                    }
                },
                Err(e) => tracing::debug!("animation: device not ready: {e}"),
            }
        } else if !want && applied {
            if let Ok(dev) = svc.device().await {
                let _ = dev.clear_effect().await;
            }
            applied = false;
        }

        if want && fallback {
            // Smooth-ish client fallback: small hue steps via the rate limiter.
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
        // Flooding the pipeline must never block the caller even though no
        // device is reachable / no credentials are set.
        for i in 0..1000u32 {
            svc.submit_color((i % 255) as u8, 0, 0);
        }
        // get_state should fail fast with a clear credential error.
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
