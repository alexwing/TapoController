//! Local configuration (the user chose a plain config file). Holds the device
//! address + local credentials and the embedded API-server / streaming
//! settings. These credentials are local-only (used for the `auth_hash`); they
//! are never sent to any TP-Link cloud endpoint.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{Result, TapoError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// IP or hostname (no scheme). Defaults to the user's bulb.
    pub host: String,
    /// Local device username (the TP-Link account the bulb is bound to, or the
    /// credentials you chose during local provisioning).
    pub username: String,
    pub password: String,
    /// `"klap"`, `"passthrough"`, or omitted for auto-detection.
    #[serde(default)]
    pub protocol: Option<String>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            host: "192.168.1.226".into(),
            username: String::new(),
            password: String::new(),
            protocol: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub enabled: bool,
    /// Bind address for the embedded API server.
    pub bind: String,
    pub port: u16,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "127.0.0.1".into(),
            port: 7755,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Max device updates per second for the ambilight/stream path. The KLAP
    /// round-trip on LAN is ~30-150ms, so realistic sustained rate is ~5-15 Hz.
    pub max_hz: f64,
    /// Exponential smoothing factor 0.0..=1.0 (1.0 = no smoothing).
    pub smoothing: f64,
    /// `"fade"` (client-smoothed) or `"instant"` (no smoothing).
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Index of the monitor to sample for ambilight (0 = first).
    #[serde(default)]
    pub monitor: usize,
    /// Screen-capture frames per second.
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Max bulb brightness (1..=100) for the ambilight; the captured scene
    /// luminance is scaled to this ceiling.
    #[serde(default = "default_max_bri")]
    pub max_brightness: u8,
}

fn default_max_bri() -> u8 {
    100
}

fn default_mode() -> String {
    "fade".into()
}
fn default_fps() -> u32 {
    10
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_hz: 12.0,
            smoothing: 0.5,
            mode: default_mode(),
            monitor: 0,
            fps: default_fps(),
            max_brightness: default_max_bri(),
        }
    }
}

/// Audio-reactive "disco" effect driven by the system output (WASAPI loopback).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DiscoConfig {
    /// `"beat"` (hue jumps on each beat), `"spectrum"` (hue follows the
    /// dominant frequency), `"pulse"` (fixed hue, brightness pulses with the
    /// bass) or `"bands"` (bass=red, mids=green, treble=blue).
    pub mode: String,
    /// Output device id to capture (empty = system default output).
    pub device_id: String,
    /// Beat-detector sensitivity 0.0..=1.0 (higher = more beats).
    pub sensitivity: f64,
    /// Extra gain applied on top of the automatic level normalisation.
    pub gain: f64,
    /// Brightness floor as % of `max_brightness` while music plays (0..=100).
    pub min_brightness: u8,
    /// Bulb brightness ceiling (1..=100).
    pub max_brightness: u8,
    /// Degrees the hue advances on each beat (`beat` mode).
    pub hue_step: u16,
    /// Base hue for `pulse` mode (0..=360).
    pub base_hue: u16,
    /// Colour saturation (0..=100).
    pub saturation: u8,
    /// How long a flash takes to fade out, in milliseconds.
    pub release_ms: u32,
    /// Max bulb updates per second (shared stream pipeline).
    pub max_hz: f64,
    /// Pipeline smoothing 0.05..=1.0 (1.0 = instant).
    pub smoothing: f64,
    /// `beat`/`pulse`: once the tempo is stable, program an on-bulb animation
    /// locked to the BPM instead of streaming every change (the bulb only
    /// takes ~5 commands/s, measured).
    pub native: bool,
}

impl Default for DiscoConfig {
    fn default() -> Self {
        Self {
            mode: "beat".into(),
            device_id: String::new(),
            sensitivity: 0.5,
            gain: 1.0,
            min_brightness: 10,
            max_brightness: 100,
            hue_step: 60,
            base_hue: 280,
            saturation: 100,
            release_ms: 250,
            // Measured on L530: each colour command takes ~200 ms on the bulb,
            // so ~5/s is the real ceiling; smoothing would eat the flashes.
            max_hz: 5.0,
            smoothing: 1.0,
            native: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfigFile {
    /// `"system"`, `"es"` or `"en"`.
    pub language: String,
}

impl Default for UiConfigFile {
    fn default() -> Self {
        Self {
            language: "system".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulbConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub mac: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default = "default_bulb_enabled")]
    pub enabled: bool,
}

fn default_bulb_enabled() -> bool {
    true
}

impl BulbConfig {
    pub fn effective_credentials<'a>(
        &'a self,
        default_user: &'a str,
        default_pass: &'a str,
    ) -> (&'a str, &'a str) {
        let user = self
            .username
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(default_user);
        let pass = self
            .password
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(default_pass);
        (user, pass)
    }
}

fn default_active_bulb() -> String {
    "all".into()
}

fn default_sync_all() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TapoConfig {
    #[serde(default)]
    pub device: DeviceConfig,
    #[serde(default)]
    pub bulbs: Vec<BulbConfig>,
    #[serde(default = "default_active_bulb")]
    pub active_bulb: String,
    #[serde(default = "default_sync_all")]
    pub sync_all: bool,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub stream: StreamConfig,
    #[serde(default)]
    pub disco: DiscoConfig,
    #[serde(default)]
    pub ui: UiConfigFile,
}

impl TapoConfig {
    pub fn normalize_bulbs(&mut self) {
        if self.bulbs.is_empty() {
            let host = if self.device.host.is_empty() {
                "192.168.1.226".into()
            } else {
                self.device.host.clone()
            };
            self.bulbs.push(BulbConfig {
                id: "bulb-1".into(),
                name: "Bombilla 1".into(),
                host,
                mac: String::new(),
                username: None,
                password: None,
                protocol: self.device.protocol.clone(),
                enabled: true,
            });
        }
        if self.active_bulb.is_empty() {
            self.active_bulb = "all".into();
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| TapoError::InvalidParam(format!("read config: {e}")))?;
        let mut cfg: Self = toml::from_str(&text)
            .map_err(|e| TapoError::InvalidParam(format!("parse config: {e}")))?;
        cfg.normalize_bulbs();
        Ok(cfg)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let text = toml::to_string_pretty(self)
            .map_err(|e| TapoError::InvalidParam(format!("serialize config: {e}")))?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, text)
            .map_err(|e| TapoError::InvalidParam(format!("write config: {e}")))
    }

    /// Load from `path`, or create it with defaults if it does not exist.
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self> {
        let p = path.as_ref();
        if p.exists() {
            Self::load(p)
        } else {
            let mut cfg = Self::default();
            cfg.normalize_bulbs();
            cfg.save(p)?;
            Ok(cfg)
        }
    }
}

