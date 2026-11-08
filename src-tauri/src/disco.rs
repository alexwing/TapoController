//! Audio-reactive "disco" effect.
//!
//! Captures whatever the PC is playing through WASAPI loopback on an output
//! device (same approach as LiveTranscriberLite: open the *render* device and
//! initialise it with `Direction::Capture`, which makes the `wasapi` crate set
//! `AUDCLNT_STREAMFLAGS_LOOPBACK`; `autoconvert` lets Windows downmix to mono
//! and resample to our rate). Every `HOP` samples we run an FFT, derive
//! bass/mid/treble energies and a beat detector, turn that into a light colour
//! and push it to the shared, rate-limited stream pipeline.
//!
//! The UI gets a `disco-frame` event (~30 fps) with the spectrum, the levels,
//! the target colour and a *simulation* of what the bulb will actually show
//! after the pipeline's rate limit + smoothing, so the effect can be tuned
//! without (or before) sending anything to the bulb.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use serde::Serialize;
use tapo_proto::device::rgb_to_hsv;
use tapo_proto::{ControlService, DiscoConfig};
use tauri::{AppHandle, Emitter};

use crate::SharedSvc;

pub const SAMPLE_RATE: usize = 48_000;
const FFT_SIZE: usize = 2048;
/// 48000 / 800 = 60 analyses per second.
const HOP: usize = 800;
const DT: f32 = HOP as f32 / SAMPLE_RATE as f32;
pub const NUM_BANDS: usize = 32;
/// Emit a UI frame every N analyses (60 / 2 = 30 fps).
const EMIT_EVERY: u32 = 2;
/// Time the L530 takes to process one colour command (measured with
/// `examples/fx_probe latency`: ~185-230 ms, commands are serialised on the
/// bulb, so this is also the minimum spacing between changes, ~5/s).
const BULB_SEND_S: f32 = 0.21;
/// How early a native-effect (re)start is sent so it lands on the beat.
const NATIVE_LEAD_S: f32 = 0.15;
/// Tempo must hold (within 3 %) this long before a native effect is programmed.
const NATIVE_STABLE_S: f32 = 3.0;
/// Re-align the on-bulb animation every N beats (the BPM estimate drifts).
const NATIVE_RESYNC_BEATS: u32 = 16;
/// Shortest gap between two beats (300 BPM).
const MIN_BEAT_GAP_S: f32 = 0.2;

// ---------------------------------------------------------------- devices --

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// `RPC_E_CHANGED_MODE`: the thread already had COM initialised as STA.
#[cfg(windows)]
const RPC_E_CHANGED_MODE: i32 = 0x8001_0106u32 as i32;

#[cfg(windows)]
fn ensure_com() -> Result<(), String> {
    let hr = wasapi::initialize_mta();
    if hr.is_ok() || hr.0 == RPC_E_CHANGED_MODE {
        Ok(())
    } else {
        Err(format!("COM init failed: HRESULT 0x{:08x}", hr.0))
    }
}

/// Active output (render) devices. Runs on its own thread so COM always gets
/// a clean MTA (Tauri's command thread is STA because of WebView2).
#[cfg(windows)]
pub fn list_output_devices() -> Result<Vec<AudioDevice>, String> {
    std::thread::spawn(|| -> Result<Vec<AudioDevice>, String> {
        use wasapi::{DeviceEnumerator, DeviceState, Direction};
        ensure_com()?;
        let e = |x: wasapi::WasapiError| x.to_string();
        let enumerator = DeviceEnumerator::new().map_err(e)?;
        let default_id = enumerator
            .get_default_device(&Direction::Render)
            .ok()
            .and_then(|d| d.get_id().ok());
        let collection = enumerator
            .get_device_collection(&Direction::Render)
            .map_err(e)?;
        let mut out = Vec::new();
        for device in &collection {
            let device = device.map_err(e)?;
            if !matches!(device.get_state(), Ok(DeviceState::Active)) {
                continue;
            }
            let id = device.get_id().map_err(e)?;
            out.push(AudioDevice {
                name: device
                    .get_friendlyname()
                    .unwrap_or_else(|_| "(unnamed)".into()),
                is_default: default_id.as_deref() == Some(id.as_str()),
                id,
            });
        }
        Ok(out)
    })
    .join()
    .unwrap_or_else(|_| Err("device enumeration thread panicked".into()))
}

#[cfg(not(windows))]
pub fn list_output_devices() -> Result<Vec<AudioDevice>, String> {
    Ok(Vec::new())
}

// --------------------------------------------------------------- analysis --

/// Result of one analysis step (one hop).
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// Log-spaced spectrum bars, 0..=1.
    pub bands: Vec<f32>,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub level: f32,
    pub beat: bool,
    pub bpm: f32,
    /// Spectral "position" 0 (all bass) ..= 1 (all treble).
    pub centroid: f32,
    /// False when the input is (near) digital silence.
    pub active: bool,
    /// Seconds since the last beat (on the tempo grid when it is locked).
    pub since_beat: f32,
    /// Bass "punch": how far the bass is above its recent floor, 0..=1.
    /// Unlike `bass` it does not saturate on loud, compressed mixes.
    pub punch: f32,
    /// Confidence of the tempo estimate 0..=1 (0 = no tempo).
    pub tempo_conf: f32,
}

/// Running-peak reference in dB that decays slowly, so levels self-normalise
/// to the system volume (loopback is captured *after* the volume control).
struct AutoRef {
    db: f32,
    floor: f32,
}

impl AutoRef {
    fn new(floor: f32) -> Self {
        Self { db: floor, floor }
    }
    /// Feed a new dB value; returns the value normalised to 0..=1 over `range` dB.
    fn norm(&mut self, db: f32, range: f32, gain_db: f32) -> f32 {
        // ~6 dB/s decay at 60 Hz, instant attack.
        self.db = (self.db - 0.1).max(db).max(self.floor);
        ((db + gain_db - (self.db - range)) / range).clamp(0.0, 1.0)
    }
}

fn to_db(power: f32) -> f32 {
    10.0 * (power + 1e-12).log10()
}

/// Adaptive floor/peak tracker: maps a dB value into 0..=1 relative to its
/// recent range, so kicks reach 1 and the gaps between them fall near 0 even
/// when the absolute level barely moves (heavily compressed music).
struct Contrast {
    lo: f32,
    hi: f32,
}

impl Contrast {
    fn new() -> Self {
        Self { lo: 0.0, hi: -200.0 }
    }
    fn step(&mut self, db: f32) -> f32 {
        if self.hi < -150.0 {
            self.lo = db;
            self.hi = db;
        }
        // Peak: instant attack, ~3 dB/s release. Floor: instant drop, ~6 dB/s rise.
        self.hi = (self.hi - 0.05).max(db);
        self.lo = (self.lo + 0.1).min(db);
        let range = (self.hi - self.lo).max(6.0);
        ((db - (self.hi - range)) / range).clamp(0.0, 1.0)
    }
}

/// Tempo + beat-phase tracker on an onset-strength envelope (spectral flux).
/// BPM comes from the autocorrelation of the last 8 s (with a mild prior
/// around 120 BPM and a harmonic check), the phase from a comb over the same
/// window. Far steadier than timing individual detected beats.
pub struct Tempo {
    onset: VecDeque<f32>,
    frame: u64,
    estimates: VecDeque<f32>,
    pub bpm: f32,
    pub conf: f32,
    /// Absolute frame of a beat and the period in frames.
    origin: f64,
    period: f64,
    last_grid_beat: u64,
    prev_phase: f64,
}

const TEMPO_WIN: usize = 480; // 8 s at 60 Hz
const TEMPO_MIN_FILL: usize = 240; // need 4 s before estimating
const TEMPO_EVERY: u64 = 15; // re-estimate 4x per second
const TEMPO_MIN_CONF: f32 = 0.12;

impl Tempo {
    pub fn new() -> Self {
        Self {
            onset: VecDeque::with_capacity(TEMPO_WIN),
            frame: 0,
            estimates: VecDeque::with_capacity(8),
            bpm: 0.0,
            conf: 0.0,
            origin: 0.0,
            period: 0.0,
            last_grid_beat: 0,
            prev_phase: 0.0,
        }
    }

    pub fn locked(&self) -> bool {
        self.bpm > 0.0 && self.conf >= TEMPO_MIN_CONF
    }

    /// Feed one onset value; returns `(grid_beat_now, seconds_since_grid_beat)`.
    pub fn push(&mut self, onset: f32) -> (bool, f32) {
        self.frame += 1;
        if self.onset.len() == TEMPO_WIN {
            self.onset.pop_front();
        }
        self.onset.push_back(onset);
        if self.onset.len() >= TEMPO_MIN_FILL && self.frame % TEMPO_EVERY == 0 {
            self.estimate();
        }
        if !self.locked() {
            self.prev_phase = 0.0;
            return (false, 0.0);
        }
        let phase = (self.frame as f64 - self.origin).rem_euclid(self.period);
        let wrapped = phase < self.prev_phase;
        self.prev_phase = phase;
        // A re-estimate may shift the grid a little: never fire twice per beat.
        let spaced = (self.frame - self.last_grid_beat) as f64 >= self.period * 0.6;
        let beat = wrapped && spaced;
        if beat {
            self.last_grid_beat = self.frame;
        }
        (beat, (phase / 60.0) as f32)
    }

    fn estimate(&mut self) {
        let n = self.onset.len();
        let mean = self.onset.iter().sum::<f32>() / n as f32;
        let o: Vec<f32> = self.onset.iter().map(|v| v - mean).collect();
        let r0: f32 = o.iter().map(|v| v * v).sum::<f32>() / n as f32;
        if r0 <= 1e-9 {
            self.conf = 0.0;
            return;
        }
        // lags 17..=56 frames -> 211..64 BPM; r up to 2x for the harmonic check.
        let (lmin, lmax) = (17usize, 56usize);
        let ac = |l: usize| -> f32 {
            if l >= n {
                return 0.0;
            }
            (l..n).map(|i| o[i] * o[i - l]).sum::<f32>() / (n - l) as f32
        };
        let r: Vec<f32> = (0..=lmax * 2).map(ac).collect();
        let score = |l: usize| r[l] + 0.5 * r[l * 2];
        let mut best = lmin;
        let mut best_v = f32::MIN;
        for l in lmin..=lmax {
            let bpm = 3600.0 / l as f32;
            let w = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
            let v = score(l) * w;
            if v > best_v {
                best_v = v;
                best = l;
            }
        }
        // Parabolic refinement of the peak lag.
        let (a, b, c) = (score(best - 1), score(best), score((best + 1).min(lmax)));
        let denom = a - 2.0 * b + c;
        let shift = if denom.abs() > 1e-12 {
            (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        let lag = best as f32 + shift;
        let conf = (r[best] / r0).clamp(0.0, 1.0);
        self.conf = conf;
        if conf < TEMPO_MIN_CONF {
            return;
        }
        let raw = 3600.0 / lag;
        if self.estimates.len() == 8 {
            self.estimates.pop_front();
        }
        self.estimates.push_back(raw);
        let mut sorted: Vec<f32> = self.estimates.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        self.bpm = sorted[sorted.len() / 2];
        self.period = 3600.0 / self.bpm as f64;

        // Phase: comb over the window, recent beats weigh more.
        let p = self.period;
        let mut best_phi = 0.0f64;
        let mut best_s = f32::MIN;
        let steps = p.ceil() as usize;
        for phi in 0..steps {
            let mut sc = 0.0f32;
            let mut w = 1.0f32;
            let mut k = 0.0f64;
            loop {
                let back = phi as f64 + k * p;
                let idx = n as f64 - 1.0 - back;
                if idx < 0.0 {
                    break;
                }
                sc += w * o[idx.round() as usize];
                w *= 0.92;
                k += 1.0;
            }
            if sc > best_s {
                best_s = sc;
                best_phi = phi as f64;
            }
        }
        self.origin = self.frame as f64 - best_phi;
    }
}

pub struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buf: Vec<Complex32>,
    ring: Vec<f32>,
    pos: usize,
    since_hop: usize,
    hop_sq: f32,
    band_bins: Vec<(usize, usize, f32)>,
    spec_ref: AutoRef,
    bass_ref: AutoRef,
    mid_ref: AutoRef,
    treble_ref: AutoRef,
    level_ref: AutoRef,
    bass_hist: VecDeque<f32>,
    prev_bass: f32,
    t: f32,
    last_beat: f32,
    silent_for: f32,
    prev_band_db: Vec<f32>,
    punch: Contrast,
    pub tempo: Tempo,
}

fn bin_of(freq: f32) -> usize {
    ((freq * FFT_SIZE as f32 / SAMPLE_RATE as f32).round() as usize).clamp(1, FFT_SIZE / 2 - 1)
}

impl Analyzer {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|i| {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos()
            })
            .collect();
        // 32 log-spaced bands, 30 Hz .. 16 kHz, each at least one bin, with a
        // +3 dB/octave tilt so a typical (pinkish) mix looks roughly flat.
        let (lo, hi) = (30.0f32, 16_000.0f32);
        let mut band_bins = Vec::with_capacity(NUM_BANDS);
        for i in 0..NUM_BANDS {
            let f0 = lo * (hi / lo).powf(i as f32 / NUM_BANDS as f32);
            let f1 = lo * (hi / lo).powf((i + 1) as f32 / NUM_BANDS as f32);
            let a = bin_of(f0);
            let b = bin_of(f1).max(a + 1);
            let center = (f0 * f1).sqrt();
            let tilt = 3.0 * (center / 100.0).log2();
            band_bins.push((a, b, tilt));
        }
        Self {
            fft,
            window,
            buf: vec![Complex32::new(0.0, 0.0); FFT_SIZE],
            ring: vec![0.0; FFT_SIZE],
            pos: 0,
            since_hop: 0,
            hop_sq: 0.0,
            band_bins,
            spec_ref: AutoRef::new(-60.0),
            bass_ref: AutoRef::new(-65.0),
            mid_ref: AutoRef::new(-70.0),
            treble_ref: AutoRef::new(-75.0),
            level_ref: AutoRef::new(-60.0),
            bass_hist: VecDeque::with_capacity(64),
            prev_bass: 0.0,
            t: 0.0,
            last_beat: -10.0,
            silent_for: 10.0,
            prev_band_db: vec![-120.0; NUM_BANDS],
            punch: Contrast::new(),
            tempo: Tempo::new(),
        }
    }

    /// Push samples; calls `on_hop` after every completed hop.
    pub fn push(&mut self, samples: &[f32], cfg: &DiscoConfig, mut on_hop: impl FnMut(Analysis)) {
        for &s in samples {
            self.ring[self.pos] = s;
            self.pos = (self.pos + 1) % FFT_SIZE;
            self.hop_sq += s * s;
            self.since_hop += 1;
            if self.since_hop >= HOP {
                let a = self.analyze(cfg);
                self.since_hop = 0;
                self.hop_sq = 0.0;
                on_hop(a);
            }
        }
    }

    fn analyze(&mut self, cfg: &DiscoConfig) -> Analysis {
        self.t += DT;
        for i in 0..FFT_SIZE {
            let s = self.ring[(self.pos + i) % FFT_SIZE];
            self.buf[i] = Complex32::new(s * self.window[i], 0.0);
        }
        self.fft.process(&mut self.buf);
        // Power per bin, normalised so a full-scale sine is ~0 dB.
        let scale = 2.0 / (FFT_SIZE as f32 * 0.5);
        let power: Vec<f32> = self.buf[..FFT_SIZE / 2]
            .iter()
            .map(|c| {
                let m = c.norm() * scale;
                m * m
            })
            .collect();

        let gain_db = 20.0 * (cfg.gain.clamp(0.1, 10.0) as f32).log10();
        let range_sum = |f0: f32, f1: f32| -> f32 { power[bin_of(f0)..bin_of(f1)].iter().sum() };

        // Display bands: mean power per band vs. one global reference.
        let mut band_db = Vec::with_capacity(NUM_BANDS);
        for &(a, b, tilt) in &self.band_bins {
            let p = power[a..b].iter().sum::<f32>() / (b - a) as f32;
            band_db.push(to_db(p) + tilt);
        }
        let max_db = band_db.iter().cloned().fold(f32::MIN, f32::max);
        // Update the reference once with the loudest band, then normalise all.
        self.spec_ref.norm(max_db, 45.0, 0.0);
        let r = self.spec_ref.db;
        let bands: Vec<f32> = band_db
            .iter()
            .map(|d| ((d + gain_db - (r - 45.0)) / 45.0).clamp(0.0, 1.0))
            .collect();

        let bass_p = range_sum(40.0, 160.0);
        let mid_p = range_sum(250.0, 2_000.0);
        let treble_p = range_sum(2_000.0, 12_000.0);
        let bass = self.bass_ref.norm(to_db(bass_p), 30.0, gain_db);
        let mid = self.mid_ref.norm(to_db(mid_p), 30.0, gain_db);
        let treble = self.treble_ref.norm(to_db(treble_p), 30.0, gain_db);
        let rms_p = self.hop_sq / HOP as f32;
        let level = self.level_ref.norm(to_db(rms_p), 30.0, gain_db);

        let active = rms_p > 1e-8; // ~ -80 dBFS
        if active {
            self.silent_for = 0.0;
        } else {
            self.silent_for += DT;
        }

        // Onset strength: positive spectral flux, low bands weighted up (kick
        // and snare carry the pulse). Feeds the tempo tracker.
        let mut flux = 0.0f32;
        for (i, (&d, prev)) in band_db.iter().zip(self.prev_band_db.iter_mut()).enumerate() {
            let w = if i < 8 { 2.0 } else if i < 20 { 1.0 } else { 0.5 };
            flux += w * (d - *prev).max(0.0);
            *prev = d;
        }
        let (grid_beat, grid_since) = self.tempo.push(if active { flux } else { 0.0 });
        let punch = if active { self.punch.step(to_db(bass_p)) } else { 0.0 };

        // Onset beat: bass energy jumps above C x its recent average and rises.
        let avg = if self.bass_hist.is_empty() {
            f32::MAX
        } else {
            self.bass_hist.iter().sum::<f32>() / self.bass_hist.len() as f32
        };
        let c = 1.8 - 0.65 * cfg.sensitivity.clamp(0.0, 1.0) as f32;
        let onset_beat = active
            && bass > 0.25
            && bass_p > c * avg
            && bass_p > self.prev_bass
            && self.t - self.last_beat >= MIN_BEAT_GAP_S;
        // With a locked tempo the beat follows the grid (no double triggers on
        // off-beat hits); otherwise the raw bass onset.
        // Per-hop digital silence (gaps between hits) must not drop the lock;
        // only sustained silence (see `active` in the result) does.
        let locked = self.tempo.locked() && self.silent_for < 1.0;
        let beat = if locked { grid_beat } else { onset_beat };
        if beat {
            self.last_beat = self.t;
        }
        if self.bass_hist.len() >= 43 {
            self.bass_hist.pop_front();
        }
        self.bass_hist.push_back(bass_p);
        self.prev_bass = bass_p;

        let total: f32 = bands.iter().sum();
        let centroid = if total > 1e-3 {
            bands
                .iter()
                .enumerate()
                .map(|(i, v)| i as f32 * v)
                .sum::<f32>()
                / total
                / (NUM_BANDS - 1) as f32
        } else {
            0.0
        };

        Analysis {
            bands,
            bass,
            mid,
            treble,
            level,
            beat,
            bpm: if locked { self.tempo.bpm } else { 0.0 },
            centroid,
            active: self.silent_for < 1.0,
            since_beat: if locked { grid_since } else { self.t - self.last_beat },
            punch,
            tempo_conf: if self.silent_for < 1.0 { self.tempo.conf } else { 0.0 },
        }
    }
}

// ------------------------------------------------------------ light engine --

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0]
}

/// Turns analyses into a target colour (brightness encoded as the value of
/// the RGB, which is exactly how the stream pipeline derives bulb brightness).
pub struct LightEngine {
    hue: f32,
    flash: f32,
    env: f32,
}

impl LightEngine {
    pub fn hue(&self) -> f32 {
        self.hue
    }

    pub fn new() -> Self {
        Self {
            hue: 0.0,
            flash: 0.0,
            env: 0.0,
        }
    }

    pub fn step(&mut self, a: &Analysis, cfg: &DiscoConfig) -> [u8; 3] {
        let release = (cfg.release_ms.clamp(30, 3000) as f32) / 1000.0;
        let k = (-DT / release).exp();
        self.flash *= k;
        if a.beat {
            self.flash = 1.0;
        }
        self.env = (self.env * k).max(a.punch);
        let floor = (cfg.min_brightness.min(100) as f32) / 100.0;
        let sat = (cfg.saturation.min(100) as f32) / 100.0;
        let lift = |x: f32| floor + (1.0 - floor) * x.clamp(0.0, 1.0);

        let rgb = match cfg.mode.as_str() {
            "pulse" => {
                self.hue = cfg.base_hue as f32;
                // Shape: kicks (punch) and grid beats (flash), squared for a
                // darker gap so the throb reads on a slow bulb.
                let x = self.env.max(self.flash * 0.9);
                hsv(self.hue, sat, lift(x * x))
            }
            "spectrum" => {
                // Bass -> red, mids -> green, treble -> violet; circular ease.
                let target = a.centroid.clamp(0.0, 1.0) * 270.0;
                let mut d = (target - self.hue).rem_euclid(360.0);
                if d > 180.0 {
                    d -= 360.0;
                }
                self.hue = (self.hue + d * 0.12).rem_euclid(360.0);
                hsv(self.hue, sat, lift(a.level.max(self.flash * 0.8)))
            }
            "bands" => {
                let (r, g, b) = (a.punch, a.mid, a.treble);
                let m = r.max(g).max(b);
                let v = lift(m.max(self.flash * 0.6)) * 255.0;
                if m < 1e-3 {
                    [v, v * 0.6, v]
                } else {
                    // Desaturate towards white as saturation goes down.
                    let mix = |c: f32| (c / m) * sat + (1.0 - sat);
                    [mix(r) * v, mix(g) * v, mix(b) * v]
                }
            }
            // "beat" (default)
            _ => {
                if a.beat {
                    self.hue = (self.hue + cfg.hue_step as f32).rem_euclid(360.0);
                }
                hsv(self.hue, sat, lift(self.flash.max(0.35 * self.env)))
            }
        };
        rgb.map(|c| c.round().clamp(0.0, 255.0) as u8)
    }
}

/// Mirrors `ControlService::color_pipeline` (rate limit + exponential
/// smoothing + value -> brightness) so the preview shows what the bulb does.
pub struct BulbSim {
    s: [f32; 3],
    next: f32,
    t: f32,
    first: bool,
    last: Option<(u16, u8, u8)>,
    pub shown: [u8; 3],
    pub bri: u8,
}

impl BulbSim {
    pub fn new() -> Self {
        Self {
            s: [0.0; 3],
            next: 0.0,
            t: 0.0,
            first: true,
            last: None,
            shown: [0; 3],
            bri: 0,
        }
    }

    pub fn step(&mut self, target: [u8; 3], cfg: &DiscoConfig) {
        self.t += DT;
        if self.t < self.next {
            return;
        }
        let alpha = cfg.smoothing.clamp(0.05, 1.0) as f32;
        for i in 0..3 {
            let tg = target[i] as f32;
            self.s[i] = if self.first {
                tg
            } else {
                self.s[i] + (tg - self.s[i]) * alpha
            };
        }
        self.first = false;
        let max_bri = cfg.max_brightness.clamp(1, 100) as f32;
        let value = self.s[0].max(self.s[1]).max(self.s[2]) / 255.0;
        let bri = (value * max_bri).round().clamp(1.0, max_bri);
        let (h, sat, _) = rgb_to_hsv(self.s[0] as u8, self.s[1] as u8, self.s[2] as u8);
        // The pipeline skips colours the bulb already shows (no slot used).
        if self.last == Some((h, sat, bri as u8)) {
            return;
        }
        self.last = Some((h, sat, bri as u8));
        self.bri = bri as u8;
        self.shown = hsv(h as f32, sat as f32 / 100.0, bri / 100.0).map(|c| c.round() as u8);
        let hz = cfg.max_hz.clamp(0.5, 30.0) as f32;
        self.next = self.t + (1.0 / hz).max(BULB_SEND_S);
    }

    /// Show a native-effect colour directly (the bulb animates it itself).
    pub fn show_native(&mut self, stop: [f32; 4]) {
        self.t += DT;
        self.bri = stop[0].round().clamp(1.0, 100.0) as u8;
        self.shown = hsv(stop[1], stop[2] / 100.0, stop[0] / 100.0)
            .map(|c| c.round().clamp(0.0, 255.0) as u8);
        self.last = None; // force a send when streaming resumes
        self.next = self.t;
    }
}

// ----------------------------------------------------------- native sync --

/// Command for the on-bulb effect engine.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeCmd {
    /// Store + start a rule.
    Program {
        change_mode: &'static str,
        change_time_ms: u32,
        stops: Vec<[u32; 4]>,
    },
    /// Re-enable the stored rule (restarts it: used to land it on the beat).
    Resync,
    /// Stop the on-bulb effect (streaming takes over again).
    Stop,
}

#[derive(Debug, Clone)]
struct NativePlan {
    mode: &'static str,
    ct: f32,
    stops: Vec<[u32; 4]>,
}

/// Locks an on-bulb animation to the detected tempo for `beat`/`pulse`.
/// The bulb can't follow individual beats at ~5 commands/s, but it can run
/// its own rule (`change_time` down to 50 ms): program it to the BPM, start
/// it on a beat and re-align it every few bars.
pub struct NativeSync {
    t: f32,
    stable: Option<(f32, f32)>,
    pub running: bool,
    locked_bpm: f32,
    plan: Option<NativePlan>,
    prog_rev: u64,
    origin: f32,
    beats_since_sync: u32,
    need_align: bool,
    lost_for: f32,
}

impl NativeSync {
    pub fn new() -> Self {
        Self {
            t: 0.0,
            stable: None,
            running: false,
            locked_bpm: 0.0,
            plan: None,
            prog_rev: 0,
            origin: 0.0,
            beats_since_sync: 0,
            need_align: false,
            lost_for: 0.0,
        }
    }

    pub fn eligible(cfg: &DiscoConfig) -> bool {
        cfg.native && matches!(cfg.mode.as_str(), "beat" | "pulse")
    }

    fn plan(bpm: f32, hue0: f32, cfg: &DiscoConfig) -> NativePlan {
        let period = 60.0 / bpm;
        let maxb = cfg.max_brightness.clamp(1, 100) as f32;
        let floor = cfg.min_brightness.min(100) as f32 / 100.0;
        let sat = cfg.saturation.min(100) as u32;
        let b = |x: f32| (maxb * x).round().clamp(1.0, 100.0) as u32;
        if cfg.mode == "pulse" {
            // Fade bright -> dim -> bright once per beat, peak on the beat.
            let h = cfg.base_hue.min(360) as u32;
            NativePlan {
                mode: "bln",
                ct: period / 2.0,
                stops: vec![[b(1.0), h, sat, 0], [b(floor), h, sat, 0]],
            }
        } else {
            // New colour on every beat, flash on the beat, dimmer off-beat.
            let dim = b(floor + (1.0 - floor) * 0.3);
            let stops = (0..4)
                .flat_map(|k| {
                    let h = (hue0 + k as f32 * cfg.hue_step as f32).rem_euclid(360.0) as u32;
                    [[b(1.0), h, sat, 0], [dim, h, sat, 0]]
                })
                .collect();
            NativePlan {
                mode: "direct",
                ct: period / 2.0,
                stops,
            }
        }
    }

    /// Colour the bulb shows now according to the running plan (preview):
    /// `[brightness, hue, saturation, _]`.
    pub fn current(&self) -> Option<[f32; 4]> {
        let p = self.plan.as_ref().filter(|_| self.running)?;
        let n = p.stops.len();
        let x = (self.t - self.origin).max(0.0) / p.ct;
        let i = x.floor() as usize % n;
        let a = p.stops[i].map(|v| v as f32);
        if p.mode == "direct" {
            return Some(a);
        }
        let b = p.stops[(i + 1) % n].map(|v| v as f32);
        let f = x.fract();
        let mut dh = (b[1] - a[1]).rem_euclid(360.0);
        if dh > 180.0 {
            dh -= 360.0;
        }
        Some([
            a[0] + (b[0] - a[0]) * f,
            (a[1] + dh * f).rem_euclid(360.0),
            a[2] + (b[2] - a[2]) * f,
            0.0,
        ])
    }

    /// Advance one hop. `can_send` is false while a previous command is still
    /// in flight; `rev` changes whenever the settings change.
    pub fn step(
        &mut self,
        a: &Analysis,
        cfg: &DiscoConfig,
        rev: u64,
        hue0: f32,
        can_send: bool,
    ) -> Option<NativeCmd> {
        self.t += DT;
        if a.beat {
            self.beats_since_sync += 1;
        }
        if !Self::eligible(cfg) {
            return self.stop();
        }

        // Tempo stability tracking.
        if a.bpm > 0.0 && a.active {
            match self.stable {
                Some((_, ref_bpm)) if (a.bpm - ref_bpm).abs() / ref_bpm <= 0.03 => {}
                _ => self.stable = Some((self.t, a.bpm)),
            }
        } else {
            self.stable = None;
        }
        let stable_bpm = self
            .stable
            .filter(|(since, _)| self.t - since >= NATIVE_STABLE_S)
            .map(|(_, b)| b);

        if self.running {
            if !a.active || a.bpm <= 0.0 {
                self.lost_for += DT;
                if self.lost_for >= 1.5 {
                    return self.stop();
                }
                return None;
            }
            self.lost_for = 0.0;
        }

        if !can_send {
            return None;
        }

        // (Re)program when the tempo is locked and the plan is missing/outdated;
        // a settings change while running re-programs right away with the
        // tempo already locked (no need to wait for a new 3 s of stability).
        let settings_changed = self.running && rev != self.prog_rev;
        let target_bpm = stable_bpm.or(if settings_changed { Some(self.locked_bpm) } else { None });
        if let Some(bpm) = target_bpm {
            let tempo_moved = (bpm - self.locked_bpm).abs() / self.locked_bpm.max(1.0) > 0.04;
            if !self.running || tempo_moved || settings_changed {
                let plan = Self::plan(bpm, hue0, cfg);
                self.locked_bpm = bpm;
                self.prog_rev = rev;
                self.running = true;
                self.need_align = true;
                // Until aligned, assume it starts after edit + enable.
                self.origin = self.t + BULB_SEND_S * 2.0;
                let cmd = NativeCmd::Program {
                    change_mode: plan.mode,
                    change_time_ms: (plan.ct * 1000.0).round() as u32,
                    stops: plan.stops.clone(),
                };
                self.plan = Some(plan);
                return Some(cmd);
            }
        }

        // Land the (re)start on the next predicted beat.
        if self.running && (self.need_align || self.beats_since_sync >= NATIVE_RESYNC_BEATS) {
            let period = 60.0 / self.locked_bpm;
            let to_next = period - a.since_beat.rem_euclid(period);
            if to_next <= NATIVE_LEAD_S && to_next > NATIVE_LEAD_S - DT * 1.5 {
                self.need_align = false;
                self.beats_since_sync = 0;
                self.origin = self.t + to_next;
                return Some(NativeCmd::Resync);
            }
        }
        None
    }

    pub fn stop(&mut self) -> Option<NativeCmd> {
        self.stable = None;
        self.lost_for = 0.0;
        if self.running {
            self.running = false;
            self.plan = None;
            return Some(NativeCmd::Stop);
        }
        None
    }
}

// --------------------------------------------------------------- runtime --

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiscoFrame {
    pub bands: Vec<f32>,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub level: f32,
    pub beat: bool,
    pub bpm: f32,
    pub active: bool,
    pub target: [u8; 3],
    pub bulb: [u8; 3],
    pub bulb_bri: u8,
    pub sending: bool,
    /// The on-bulb animation locked to the BPM is driving the light.
    pub native: bool,
    /// Native mode is on but still waiting for a steady tempo.
    pub native_waiting: bool,
    /// Tempo-estimate confidence 0..=1.
    pub tempo_conf: f32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiscoStatus {
    /// Friendly name of the device being captured, empty if none.
    pub device: String,
    /// Last capture error, empty when fine.
    pub error: String,
}

struct Shared {
    preview: AtomicBool,
    output: AtomicBool,
    running: AtomicBool,
    rev: AtomicU64,
    cfg: StdMutex<DiscoConfig>,
    status: StdMutex<DiscoStatus>,
}

pub struct Disco {
    sh: Arc<Shared>,
    handle: StdMutex<Option<JoinHandle<()>>>,
}

impl Disco {
    pub fn new(cfg: DiscoConfig) -> Self {
        Self {
            sh: Arc::new(Shared {
                preview: AtomicBool::new(false),
                output: AtomicBool::new(false),
                running: AtomicBool::new(false),
                rev: AtomicU64::new(1),
                cfg: StdMutex::new(cfg),
                status: StdMutex::new(DiscoStatus {
                    device: String::new(),
                    error: String::new(),
                }),
            }),
            handle: StdMutex::new(None),
        }
    }

    pub fn is_output(&self) -> bool {
        self.sh.output.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> DiscoStatus {
        self.sh.status.lock().unwrap().clone()
    }

    /// Live-update the settings (the worker picks them up on the next hop).
    pub fn tune(&self, cfg: DiscoConfig) {
        *self.sh.cfg.lock().unwrap() = cfg;
        self.sh.rev.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_preview(&self, on: bool, app: &AppHandle, svc: &SharedSvc) {
        self.sh.preview.store(on, Ordering::Relaxed);
        self.sync(app, svc);
    }

    pub fn set_output(&self, on: bool, app: &AppHandle, svc: &SharedSvc) {
        self.sh.output.store(on, Ordering::Relaxed);
        self.sh.rev.fetch_add(1, Ordering::Relaxed);
        self.sync(app, svc);
    }

    /// Start the worker if someone needs it, stop (and join) it otherwise.
    fn sync(&self, app: &AppHandle, svc: &SharedSvc) {
        let mut h = self.handle.lock().unwrap();
        let want = self.sh.preview.load(Ordering::Relaxed) || self.sh.output.load(Ordering::Relaxed);
        if want {
            if h.as_ref().map(|j| !j.is_finished()).unwrap_or(false) {
                return;
            }
            self.sh.running.store(true, Ordering::Relaxed);
            let sh = self.sh.clone();
            let app = app.clone();
            let svc = svc.clone();
            *h = Some(
                std::thread::Builder::new()
                    .name("tapo-disco".into())
                    .spawn(move || worker(sh, app, svc))
                    .expect("spawn disco thread"),
            );
        } else if let Some(j) = h.take() {
            self.sh.running.store(false, Ordering::Relaxed);
            let _ = j.join();
        }
    }
}

/// Per-worker processing state (analysis -> light -> bulb/UI).
struct Pipeline {
    sh: Arc<Shared>,
    app: AppHandle,
    svc: SharedSvc,
    svc_cached: Option<Arc<ControlService>>,
    svc_checked: Instant,
    cfg: DiscoConfig,
    seen_rev: u64,
    tuned_rev: u64,
    analyzer: Analyzer,
    light: LightEngine,
    sim: BulbSim,
    native: NativeSync,
    native_tx: tokio::sync::mpsc::UnboundedSender<NativeCmd>,
    native_pending: Arc<AtomicUsize>,
    native_failed: Arc<AtomicBool>,
    /// Hops left before retrying the native engine after a bulb error.
    native_cooldown: u32,
    /// Whether the real bulb currently runs our effect (needs a Stop later).
    native_on_bulb: bool,
    n: u32,
    beat_latch: bool,
}

impl Pipeline {
    fn current_svc(&mut self) -> Option<Arc<ControlService>> {
        // The service is hot-swapped when the config is saved; refresh the
        // cached handle about once a second without ever blocking.
        if self.svc_cached.is_none() || self.svc_checked.elapsed() > Duration::from_secs(1) {
            if let Ok(g) = self.svc.try_lock() {
                let fresh = g.clone();
                let swapped = self
                    .svc_cached
                    .as_ref()
                    .map(|c| !Arc::ptr_eq(c, &fresh))
                    .unwrap_or(true);
                if swapped {
                    self.tuned_rev = 0;
                }
                self.svc_cached = Some(fresh);
                self.svc_checked = Instant::now();
            }
        }
        self.svc_cached.clone()
    }

    fn feed(&mut self, samples: &[f32]) {
        let rev = self.sh.rev.load(Ordering::Relaxed);
        if rev != self.seen_rev {
            self.cfg = self.sh.cfg.lock().unwrap().clone();
            self.seen_rev = rev;
        }
        let cfg = self.cfg.clone();
        let mut results = Vec::with_capacity(2);
        self.analyzer.push(samples, &cfg, |a| results.push(a));
        for a in results {
            self.on_analysis(a, &cfg);
        }
    }

    fn send_native(&mut self, cmd: NativeCmd) {
        self.native_on_bulb = !matches!(cmd, NativeCmd::Stop);
        self.native_pending.fetch_add(1, Ordering::SeqCst);
        if self.native_tx.send(cmd).is_err() {
            self.native_pending.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn on_analysis(&mut self, a: Analysis, cfg: &DiscoConfig) {
        let target = self.light.step(&a, cfg);
        let sending = self.sh.output.load(Ordering::Relaxed);

        // With output off (pure preview) the native engine still runs so the
        // preview shows what the bulb would do; commands only go out when
        // sending, one at a time.
        let can_send = !sending || self.native_pending.load(Ordering::SeqCst) == 0;
        if self.native_failed.swap(false, Ordering::SeqCst) {
            // The bulb rejected the effect: fall back to streaming for a while.
            self.native.stop();
            self.native_on_bulb = false;
            self.native_cooldown = 600;
        }
        let cmd = if self.native_cooldown > 0 {
            self.native_cooldown -= 1;
            let mut c = cfg.clone();
            c.native = false;
            self.native.step(&a, &c, self.seen_rev, self.light.hue(), can_send)
        } else {
            self.native
                .step(&a, cfg, self.seen_rev, self.light.hue(), can_send)
        };
        if sending {
            match cmd {
                // A Stop for an effect that never reached the bulb is moot.
                Some(NativeCmd::Stop) if !self.native_on_bulb => {}
                Some(c) => self.send_native(c),
                None => {}
            }
            // Output was turned on while the preview had a plan running:
            // re-program it on the bulb (on the next eligible hop).
            if self.native.running && !self.native_on_bulb {
                self.native.stop();
            }
        } else if self.native_on_bulb {
            self.send_native(NativeCmd::Stop);
        }

        match self.native.current() {
            Some(stop) => self.sim.show_native(stop),
            None => self.sim.step(target, cfg),
        }
        if sending && !self.native.running {
            if let Some(svc) = self.current_svc() {
                if self.tuned_rev != self.seen_rev {
                    svc.set_stream_tuning(cfg.max_hz, cfg.smoothing, cfg.max_brightness);
                    self.tuned_rev = self.seen_rev;
                }
                svc.submit_color(target[0], target[1], target[2]);
            }
        }
        self.beat_latch |= a.beat;
        self.n = self.n.wrapping_add(1);
        if self.n % EMIT_EVERY == 0 && self.sh.preview.load(Ordering::Relaxed) {
            let frame = DiscoFrame {
                bands: a.bands,
                bass: a.bass,
                mid: a.mid,
                treble: a.treble,
                level: a.level,
                beat: self.beat_latch,
                bpm: a.bpm,
                active: a.active,
                target,
                bulb: self.sim.shown,
                bulb_bri: self.sim.bri,
                sending,
                native: self.native.running,
                native_waiting: sending
                    && !self.native.running
                    && self.native_cooldown == 0
                    && NativeSync::eligible(cfg),
                tempo_conf: a.tempo_conf,
            };
            self.beat_latch = false;
            let _ = self.app.emit("disco-frame", frame);
        }
    }

    fn set_status(&self, device: &str, error: &str) {
        let st = DiscoStatus {
            device: device.into(),
            error: error.into(),
        };
        *self.sh.status.lock().unwrap() = st.clone();
        let _ = self.app.emit("disco-status", st);
    }
}

/// Executes native-effect commands in order on the async runtime.
fn spawn_native_executor(
    svc: SharedSvc,
    pending: Arc<AtomicUsize>,
    failed: Arc<AtomicBool>,
) -> tokio::sync::mpsc::UnboundedSender<NativeCmd> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<NativeCmd>();
    tauri::async_runtime::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            let s = svc.lock().await.clone();
            let res = match &cmd {
                NativeCmd::Program {
                    change_mode,
                    change_time_ms,
                    stops,
                } => {
                    s.play_effect(change_mode, *change_time_ms, Some(stops.clone()))
                        .await
                }
                NativeCmd::Resync => s.play_effect("", 0, None).await,
                NativeCmd::Stop => {
                    s.stop_effect().await;
                    Ok(())
                }
            };
            match &cmd {
                NativeCmd::Program { change_mode, change_time_ms, stops } if res.is_ok() => {
                    tracing::info!(
                        "disco native: programmed {change_mode} {change_time_ms} ms x {} stops",
                        stops.len()
                    )
                }
                NativeCmd::Resync if res.is_ok() => tracing::debug!("disco native: resync"),
                NativeCmd::Stop => tracing::info!("disco native: stopped"),
                _ => {}
            }
            if let Err(e) = res {
                tracing::warn!("disco native {cmd:?} failed: {e}");
                if !matches!(cmd, NativeCmd::Stop) {
                    failed.store(true, Ordering::SeqCst);
                }
            }
            pending.fetch_sub(1, Ordering::SeqCst);
        }
    });
    tx
}

fn worker(sh: Arc<Shared>, app: AppHandle, svc: SharedSvc) {
    let cfg = sh.cfg.lock().unwrap().clone();
    let native_pending = Arc::new(AtomicUsize::new(0));
    let native_failed = Arc::new(AtomicBool::new(false));
    let native_tx =
        spawn_native_executor(svc.clone(), native_pending.clone(), native_failed.clone());
    let mut p = Pipeline {
        sh: sh.clone(),
        app,
        svc,
        svc_cached: None,
        svc_checked: Instant::now(),
        seen_rev: sh.rev.load(Ordering::Relaxed),
        cfg,
        tuned_rev: 0,
        analyzer: Analyzer::new(),
        light: LightEngine::new(),
        sim: BulbSim::new(),
        native: NativeSync::new(),
        native_tx,
        native_pending,
        native_failed,
        native_cooldown: 0,
        native_on_bulb: false,
        n: 0,
        beat_latch: false,
    };
    while sh.running.load(Ordering::Relaxed) {
        if let Err(e) = capture_session(&mut p) {
            tracing::warn!("disco capture failed: {e}");
            p.set_status("", &e);
            // Keep the preview alive (decaying to silence) and retry.
            let until = Instant::now() + Duration::from_secs(1);
            while Instant::now() < until && sh.running.load(Ordering::Relaxed) {
                p.feed(&[0.0; HOP]);
                std::thread::sleep(Duration::from_millis(16));
            }
        }
    }
    if p.native_on_bulb {
        p.send_native(NativeCmd::Stop);
    }
    p.set_status("", "");
    tracing::info!("disco loop stopped");
}

/// What a capture session talks to: the real pipeline, or a test probe.
trait Sink {
    fn running(&self) -> bool;
    fn wanted_device(&self) -> String;
    fn opened(&mut self, name: &str);
    fn feed(&mut self, samples: &[f32]);
}

impl Sink for Pipeline {
    fn running(&self) -> bool {
        self.sh.running.load(Ordering::Relaxed)
    }
    fn wanted_device(&self) -> String {
        self.sh.cfg.lock().unwrap().device_id.clone()
    }
    fn opened(&mut self, name: &str) {
        self.set_status(name, "");
    }
    fn feed(&mut self, samples: &[f32]) {
        Pipeline::feed(self, samples)
    }
}

/// One capture session on the configured device. Returns Ok when stopped or
/// when the device selection changed (the caller reopens).
#[cfg(windows)]
fn capture_session(p: &mut impl Sink) -> Result<(), String> {
    use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WasapiError, WaveFormat};
    let e = |x: WasapiError| x.to_string();

    ensure_com()?;
    let wanted = p.wanted_device();
    let enumerator = DeviceEnumerator::new().map_err(e)?;
    let mut device = None;
    if !wanted.is_empty() {
        let coll = enumerator
            .get_device_collection(&Direction::Render)
            .map_err(e)?;
        for d in &coll {
            let d = d.map_err(e)?;
            if d.get_id().map(|id| id == wanted).unwrap_or(false) {
                device = Some(d);
                break;
            }
        }
    }
    let device = match device {
        Some(d) => d,
        None => enumerator
            .get_default_device(&Direction::Render)
            .map_err(e)?,
    };
    let name = device
        .get_friendlyname()
        .unwrap_or_else(|_| "(unnamed)".into());

    let mut client = device.get_iaudioclient().map_err(e)?;
    let buffer_hns = client
        .get_device_period()
        .map(|(_, min)| min)
        .unwrap_or(100_000);
    let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE, 1, None);
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: buffer_hns,
    };
    // Render device + Direction::Capture => loopback.
    client
        .initialize_client(&format, &Direction::Capture, &mode)
        .map_err(e)?;
    let event = client.set_get_eventhandle().map_err(e)?;
    let capture = client.get_audiocaptureclient().map_err(e)?;
    client.start_stream().map_err(e)?;
    tracing::info!("disco capturing loopback of {name}");
    p.opened(&name);

    let mut raw: VecDeque<u8> = VecDeque::with_capacity(64 * 1024);
    let mut samples: Vec<f32> = Vec::with_capacity(4096);
    let mut last_fed = Instant::now();
    let start_rev_dev = wanted;

    let result = (|| -> Result<(), String> {
        while p.running() {
            if p.wanted_device() != start_rev_dev {
                return Ok(()); // device changed: reopen
            }
            capture.read_from_device_to_deque(&mut raw).map_err(e)?;
            samples.clear();
            while raw.len() >= 4 {
                let b = [
                    raw.pop_front().unwrap(),
                    raw.pop_front().unwrap(),
                    raw.pop_front().unwrap(),
                    raw.pop_front().unwrap(),
                ];
                samples.push(f32::from_le_bytes(b));
            }
            if !samples.is_empty() {
                last_fed = Instant::now();
                p.feed(&samples);
            } else {
                // WASAPI loopback delivers nothing while the device is silent:
                // synthesise silence in wall-clock time so the analysis (and
                // the light) decays instead of freezing.
                let gap = last_fed.elapsed();
                if gap > Duration::from_millis(40) {
                    let n = ((gap.as_secs_f64() * SAMPLE_RATE as f64) as usize).min(SAMPLE_RATE);
                    p.feed(&vec![0.0; n]);
                    last_fed = Instant::now();
                }
            }
            match event.wait_for_event(50) {
                Ok(()) | Err(WasapiError::EventTimeout) => {}
                Err(x) => return Err(x.to_string()),
            }
        }
        Ok(())
    })();
    let _ = client.stop_stream();
    result
}

#[cfg(not(windows))]
fn capture_session(_p: &mut impl Sink) -> Result<(), String> {
    Err("system audio capture is only implemented on Windows".into())
}

// ------------------------------------------------------------------ tests --

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, amp: f32, secs: f32, phase0: usize) -> Vec<f32> {
        let n = (secs * SAMPLE_RATE as f32) as usize;
        (0..n)
            .map(|i| {
                amp * (2.0 * std::f32::consts::PI * freq * (i + phase0) as f32
                    / SAMPLE_RATE as f32)
                    .sin()
            })
            .collect()
    }

    fn run(an: &mut Analyzer, s: &[f32], cfg: &DiscoConfig) -> Vec<Analysis> {
        let mut out = Vec::new();
        an.push(s, cfg, |a| out.push(a));
        out
    }

    #[test]
    fn bass_tone_lights_bass_not_treble() {
        let cfg = DiscoConfig::default();
        let mut an = Analyzer::new();
        let out = run(&mut an, &tone(80.0, 0.5, 1.0, 0), &cfg);
        let last = out.last().unwrap();
        assert!(last.bass > 0.8, "bass {}", last.bass);
        assert!(last.treble < 0.2, "treble {}", last.treble);
        assert!(last.centroid < 0.3, "centroid {}", last.centroid);
        assert!(last.active);
    }

    #[test]
    fn treble_tone_moves_centroid_up() {
        let cfg = DiscoConfig::default();
        let mut an = Analyzer::new();
        let out = run(&mut an, &tone(6000.0, 0.5, 1.0, 0), &cfg);
        let last = out.last().unwrap();
        assert!(last.treble > 0.8, "treble {}", last.treble);
        assert!(last.centroid > 0.6, "centroid {}", last.centroid);
    }

    #[test]
    fn kick_pattern_detects_beats_and_bpm() {
        // 120 BPM: 100 ms kick at 60 Hz every 500 ms, quiet hats in between.
        let cfg = DiscoConfig::default();
        let mut an = Analyzer::new();
        let mut beats = 0;
        let mut bpm = 0.0;
        for k in 0..16 {
            let mut s = tone(60.0, 0.8, 0.1, 0);
            s.extend(tone(8000.0, 0.02, 0.4, 0));
            for a in run(&mut an, &s, &cfg) {
                if a.beat {
                    beats += 1;
                }
                if k > 8 {
                    bpm = a.bpm;
                }
            }
        }
        assert!((14..=17).contains(&beats), "beats {beats}");
        assert!((bpm - 120.0).abs() < 6.0, "bpm {bpm}");
    }

    #[test]
    fn silence_is_inactive_and_dark_at_floor() {
        let cfg = DiscoConfig::default();
        let mut an = Analyzer::new();
        let mut light = LightEngine::new();
        let out = run(&mut an, &vec![0.0; SAMPLE_RATE * 2], &cfg);
        let last = out.last().unwrap();
        assert!(!last.active);
        assert!(!out.iter().any(|a| a.beat));
        let rgb = light.step(last, &cfg);
        let v = *rgb.iter().max().unwrap() as f32 / 255.0;
        assert!((v - cfg.min_brightness as f32 / 100.0).abs() < 0.02, "v {v}");
    }

    #[test]
    fn beat_mode_advances_hue_on_beat() {
        let cfg = DiscoConfig::default();
        let mut light = LightEngine::new();
        let a = Analysis {
            beat: true,
            bands: vec![0.0; NUM_BANDS],
            ..Default::default()
        };
        light.step(&a, &cfg);
        assert_eq!(light.hue, cfg.hue_step as f32);
        let rgb = light.step(&Analysis { beat: false, ..a.clone() }, &cfg);
        assert!(rgb.iter().any(|&c| c > 200), "flash should be bright: {rgb:?}");
    }

    /// Real WASAPI loopback on this machine (play something for non-zero
    /// levels): `cargo test --lib loopback -- --ignored --nocapture`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn loopback_capture_smoke() {
        struct Probe {
            t0: Instant,
            n: usize,
            peak: f32,
            name: String,
            beats: usize,
            an: Analyzer,
        }
        impl Sink for Probe {
            fn running(&self) -> bool {
                self.t0.elapsed() < Duration::from_secs(4)
            }
            fn wanted_device(&self) -> String {
                String::new()
            }
            fn opened(&mut self, name: &str) {
                self.name = name.into();
            }
            fn feed(&mut self, s: &[f32]) {
                self.n += s.len();
                self.peak = s.iter().fold(self.peak, |m, x| m.max(x.abs()));
                let cfg = DiscoConfig::default();
                let mut beats = 0;
                self.an.push(s, &cfg, |a| beats += a.beat as usize);
                self.beats += beats;
            }
        }
        let mut p = Probe {
            t0: Instant::now(),
            n: 0,
            peak: 0.0,
            name: String::new(),
            beats: 0,
            an: Analyzer::new(),
        };
        capture_session(&mut p).expect("loopback capture");
        let secs = p.t0.elapsed().as_secs_f32();
        println!(
            "device={} samples={} ({:.0}/s) peak={:.4} beats={}",
            p.name,
            p.n,
            p.n as f32 / secs,
            p.peak,
            p.beats
        );
        assert!(!p.name.is_empty());
        // Real or synthesised-silence samples must track wall-clock time.
        assert!((p.n as f32 / secs - SAMPLE_RATE as f32).abs() < SAMPLE_RATE as f32 * 0.15);
    }

    fn steady(bpm: f32, frames: usize, cfg: &DiscoConfig, ns: &mut NativeSync) -> Vec<(usize, NativeCmd)> {
        // Synthetic analyses: a beat every 60/bpm s, BPM already estimated.
        let period = 60.0 / bpm;
        let mut since = 0.0f32;
        let mut out = Vec::new();
        for i in 0..frames {
            since += DT;
            let beat = since >= period;
            if beat {
                since -= period;
            }
            let a = Analysis {
                bands: vec![0.5; NUM_BANDS],
                beat,
                bpm,
                active: true,
                since_beat: since,
                ..Default::default()
            };
            if let Some(c) = ns.step(&a, cfg, 1, 0.0, true) {
                out.push((i, c));
            }
        }
        out
    }

    #[test]
    fn native_locks_to_tempo_then_aligns_and_resyncs() {
        let mut cfg = DiscoConfig::default();
        cfg.native = true;
        cfg.mode = "pulse".into();
        let mut ns = NativeSync::new();
        let cmds = steady(120.0, 60 * 20, &cfg, &mut ns);
        // First a Program after ~3 s of stable tempo, with half-beat steps.
        match &cmds[0] {
            (i, NativeCmd::Program { change_mode, change_time_ms, stops }) => {
                assert!((175..=190).contains(i), "programmed at frame {i}");
                assert_eq!(*change_mode, "bln");
                assert_eq!(*change_time_ms, 250);
                assert_eq!(stops.len(), 2);
                assert_eq!(stops[0][0], 100);
                assert_eq!(stops[1][0], 10);
            }
            other => panic!("expected Program, got {other:?}"),
        }
        // Then an aligned Resync, ~NATIVE_LEAD_S before a beat.
        assert!(matches!(cmds[1].1, NativeCmd::Resync));
        // ~17 s left at 2 beats/s -> 34 beats -> 2 periodic resyncs.
        let resyncs = cmds.iter().filter(|c| c.1 == NativeCmd::Resync).count();
        assert!((2..=4).contains(&resyncs), "resyncs {resyncs}: {cmds:?}");
        assert!(ns.running);
        // Preview follows the plan: bright near the beat origin.
        assert!(ns.current().is_some());
    }

    #[test]
    fn native_stops_on_silence_and_when_disabled() {
        let mut cfg = DiscoConfig::default();
        cfg.native = true;
        let mut ns = NativeSync::new();
        steady(128.0, 60 * 5, &cfg, &mut ns);
        assert!(ns.running);
        let silent = Analysis {
            bands: vec![0.0; NUM_BANDS],
            ..Default::default()
        };
        let mut stop_at = None;
        for i in 0..200 {
            if let Some(NativeCmd::Stop) = ns.step(&silent, &cfg, 1, 0.0, true) {
                stop_at = Some(i);
                break;
            }
        }
        let i = stop_at.expect("should stop");
        assert!((85..=95).contains(&i), "stopped after {i} frames");
        // Disabled -> never programs.
        cfg.native = false;
        let mut ns = NativeSync::new();
        assert!(steady(128.0, 60 * 6, &cfg, &mut ns).is_empty());
        // Spectrum mode isn't eligible either.
        cfg.native = true;
        cfg.mode = "spectrum".into();
        assert!(steady(128.0, 60 * 6, &cfg, &mut ns).is_empty());
    }

    #[test]
    fn tempo_ignores_offbeat_hats_and_finds_phase() {
        // 138 BPM: kick on the beat, open hat exactly between beats.
        let cfg = DiscoConfig::default();
        let mut an = Analyzer::new();
        let period = 60.0 / 138.0;
        let mut beats_t = Vec::new();
        let mut t = 0.0f32;
        let mut s = Vec::new();
        for _ in 0..40 {
            s.extend(tone(55.0, 0.8, 0.09, 0));
            s.extend(vec![0.0; ((period / 2.0 - 0.09) * SAMPLE_RATE as f32) as usize]);
            s.extend(tone(9000.0, 0.3, 0.05, 0));
            s.extend(vec![0.0; ((period / 2.0 - 0.05) * SAMPLE_RATE as f32) as usize]);
        }
        let mut last = Analysis::default();
        for a in run(&mut an, &s, &cfg) {
            t += DT;
            if a.beat && t > 10.0 {
                beats_t.push(t);
            }
            last = a;
        }
        assert!((last.bpm - 138.0).abs() < 3.0, "bpm {}", last.bpm);
        assert!(last.tempo_conf > 0.2, "conf {}", last.tempo_conf);
        // Grid beats land on the kicks (phase ~0 within a period), not the hats.
        for bt in &beats_t {
            let ph = (bt % period) / period;
            let d = ph.min(1.0 - ph);
            assert!(d < 0.2, "beat at {bt:.2}s has phase {ph:.2}");
        }
        assert!(beats_t.len() > 10);
    }

    #[test]
    fn native_beat_plan_steps_hue_per_beat() {
        let mut cfg = DiscoConfig::default();
        cfg.native = true;
        cfg.mode = "beat".into();
        cfg.hue_step = 90;
        let p = NativeSync::plan(100.0, 10.0, &cfg);
        assert_eq!(p.mode, "direct");
        assert!((p.ct - 0.3).abs() < 1e-4);
        let hues: Vec<u32> = p.stops.iter().map(|s| s[1]).collect();
        assert_eq!(hues, vec![10, 10, 100, 100, 190, 190, 280, 280]);
        assert!(p.stops[0][0] > p.stops[1][0]);
    }

    /// Live diagnosis with whatever is playing + the saved config:
    /// `cargo test --lib live_native -- --ignored --nocapture`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn live_native_diagnosis() {
        let cfg_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tapo-config.toml");
        let mut cfg = tapo_proto::TapoConfig::load(&cfg_path).map(|c| c.disco).unwrap_or_default();
        cfg.native = true;
        if let Ok(m) = std::env::var("DISCO_MODE") { cfg.mode = m; }
        if let Ok(v) = std::env::var("DISCO_SENS") { cfg.sensitivity = v.parse().unwrap(); }
        println!("cfg: {cfg:?}");
        struct Probe {
            t0: Instant,
            secs: u64,
            an: Analyzer,
            ns: NativeSync,
            light: LightEngine,
            cfg: DiscoConfig,
            beats: usize,
            hops: usize,
            last_print: f32,
            peak: f32,
        }
        impl Sink for Probe {
            fn running(&self) -> bool { self.t0.elapsed() < Duration::from_secs(self.secs) }
            fn wanted_device(&self) -> String { String::new() }
            fn opened(&mut self, name: &str) { println!("device: {name}"); }
            fn feed(&mut self, s: &[f32]) {
                self.peak = s.iter().fold(self.peak, |m, x| m.max(x.abs()));
                let cfg = self.cfg.clone();
                let mut out = Vec::new();
                self.an.push(s, &cfg, |a| out.push(a));
                for a in out {
                    self.hops += 1;
                    self.beats += a.beat as usize;
                    let rgb = self.light.step(&a, &cfg);
                    let t = self.hops as f32 * DT;
                    if let Some(c) = self.ns.step(&a, &cfg, 1, self.light.hue(), true) {
                        println!("{t:6.2}s NATIVE {c:?}");
                    }
                    if t - self.last_print >= 1.0 {
                        self.last_print = t;
                        println!(
                            "{t:6.2}s bpm={:5.1} conf={:.2} beats/s~{} punch={:.2} bass={:.2} active={} native={} rgb={rgb:?}",
                            a.bpm, a.tempo_conf, self.beats, a.punch, a.bass, a.active, self.ns.running
                        );
                        self.beats = 0;
                    }
                }
            }
        }
        let secs = std::env::var("DISCO_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
        let mut p = Probe {
            t0: Instant::now(), secs, an: Analyzer::new(), ns: NativeSync::new(),
            light: LightEngine::new(), cfg, beats: 0, hops: 0, last_print: 0.0, peak: 0.0,
        };
        capture_session(&mut p).expect("capture");
        println!("peak sample {:.3}", p.peak);
    }

    #[test]
    fn bulb_sim_respects_rate_limit() {
        let mut cfg = DiscoConfig::default();
        cfg.max_hz = 5.0;
        cfg.smoothing = 1.0;
        let mut sim = BulbSim::new();
        let mut changes = 0;
        let mut last = [0u8; 3];
        // One second of alternating targets at 60 Hz.
        for i in 0..60 {
            let t = [(i * 4) as u8, 0, 0];
            sim.step(t, &cfg);
            if sim.shown != last {
                changes += 1;
                last = sim.shown;
            }
        }
        // period = 1/5 + 0.06 = 0.26 s -> ~4 updates per second.
        assert!((3..=5).contains(&changes), "changes {changes}");
    }
}
