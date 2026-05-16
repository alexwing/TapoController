//! Screen-capture ambilight: grabs the chosen monitor, computes an average
//! colour and feeds it into the shared `ControlService` stream pipeline (which
//! applies the configurable fade/instant smoothing + device rate-limit).
//!
//! Player-agnostic on purpose — works with PotPlayer, VLC, browsers, games.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tapo_proto::ControlService;
use xcap::Monitor;

#[derive(Serialize, Clone)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

pub fn list_monitors() -> Vec<MonitorInfo> {
    Monitor::all()
        .map(|ms| {
            ms.into_iter()
                .enumerate()
                .map(|(i, m)| {
                    let raw = m.name().unwrap_or_default();
                    // Windows device paths like `\\.\DISPLAY1` aren't friendly.
                    let name = if raw.is_empty() || raw.starts_with("\\\\") {
                        format!("Display {}", i + 1)
                    } else {
                        raw
                    };
                    MonitorInfo {
                        index: i,
                        name,
                        width: m.width().unwrap_or(0),
                        height: m.height().unwrap_or(0),
                        primary: m.is_primary().unwrap_or(false),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Average colour of a captured frame. Samples a sparse grid (fast) and skips
/// near-black pixels so letterboxing/black bars don't wash the colour out.
fn average_color(img: &image::RgbaImage) -> (u8, u8, u8) {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return (0, 0, 0);
    }
    let total = (w as u64) * (h as u64);
    // ~20k samples regardless of resolution.
    let step = ((total / 20_000).max(1) as usize).max(1);
    let buf = img.as_raw(); // RGBA8
    let px_count = buf.len() / 4;
    let (mut sr, mut sg, mut sb, mut n) = (0u64, 0u64, 0u64, 0u64);
    let mut i = 0;
    while i < px_count {
        let o = i * 4;
        let (r, g, b) = (buf[o] as u64, buf[o + 1] as u64, buf[o + 2] as u64);
        if r + g + b > 30 {
            sr += r;
            sg += g;
            sb += b;
            n += 1;
        }
        i += step;
    }
    if n == 0 {
        return (0, 0, 0);
    }
    (
        (sr / n) as u8,
        (sg / n) as u8,
        (sb / n) as u8,
    )
}

pub struct Ambilight {
    running: Arc<AtomicBool>,
}

impl Ambilight {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// (Re)start the capture loop for `monitor_idx` at `fps`. Colours go to the
    /// shared service pipeline; fade/instant + rate are tuned separately via
    /// `ControlService::set_stream_tuning`.
    pub fn start(&self, svc: Arc<ControlService>, monitor_idx: usize, fps: u32) {
        self.running.store(false, Ordering::Relaxed);
        // Let any previous loop observe the stop before we start a new one.
        std::thread::sleep(Duration::from_millis(50));
        self.running.store(true, Ordering::Relaxed);

        let running = self.running.clone();
        let fps = fps.clamp(1, 30);
        let frame = Duration::from_secs_f64(1.0 / fps as f64);

        std::thread::Builder::new()
            .name("tapo-ambilight".into())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    let t0 = Instant::now();
                    match Monitor::all() {
                        Ok(mons) if !mons.is_empty() => {
                            let m = mons
                                .get(monitor_idx)
                                .or_else(|| mons.first())
                                .unwrap();
                            match m.capture_image() {
                                Ok(img) => {
                                    let (r, g, b) = average_color(&img);
                                    svc.submit_color(r, g, b);
                                }
                                Err(e) => tracing::debug!("ambilight capture failed: {e}"),
                            }
                        }
                        _ => tracing::debug!("ambilight: no monitors"),
                    }
                    let elapsed = t0.elapsed();
                    if elapsed < frame {
                        std::thread::sleep(frame - elapsed);
                    }
                }
                tracing::info!("ambilight loop stopped");
            })
            .expect("spawn ambilight thread");
    }
}
