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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TapoConfig {
    #[serde(default)]
    pub device: DeviceConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub stream: StreamConfig,
    #[serde(default)]
    pub ui: UiConfigFile,
}

impl TapoConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| TapoError::InvalidParam(format!("read config: {e}")))?;
        toml::from_str(&text).map_err(|e| TapoError::InvalidParam(format!("parse config: {e}")))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let text = toml::to_string_pretty(self)
            .map_err(|e| TapoError::InvalidParam(format!("serialize config: {e}")))?;
        std::fs::write(path, text)
            .map_err(|e| TapoError::InvalidParam(format!("write config: {e}")))
    }

    /// Load from `path`, or create it with defaults if it does not exist.
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self> {
        let p = path.as_ref();
        if p.exists() {
            Self::load(p)
        } else {
            let cfg = Self::default();
            cfg.save(p)?;
            Ok(cfg)
        }
    }
}
