//! High-level device API on top of the KLAP transport.
//!
//! Tapo bulbs speak a small JSON-RPC dialect over the encrypted KLAP channel:
//! `get_device_info` to read state, `set_device_info` to change it.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Result, TapoError};
use crate::klap::KlapClient;
use crate::passthrough::PassthroughClient;

/// Which local protocol a device speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Newer SMART/Tapo firmware (`/app/handshake1` returns 200).
    Klap,
    /// Legacy `SHIP 2.0` securePassthrough (RSA + AES + token).
    Passthrough,
}

enum Transport {
    Klap(KlapClient),
    Passthrough(PassthroughClient),
}

impl Transport {
    async fn request_raw(&self, json: &[u8]) -> Result<Vec<u8>> {
        match self {
            Transport::Klap(c) => c.request_raw(json).await,
            Transport::Passthrough(c) => c.request_raw(json).await,
        }
    }
}

/// Probe `host` to decide which local protocol it speaks. Sends an
/// unauthenticated 16-byte `handshake1`; 200 => KLAP, anything else =>
/// legacy passthrough.
pub async fn detect_protocol(host: &str) -> Result<Protocol> {
    // Must be a *random* 16-byte seed: this firmware rejects an all-zero body
    // with 400 even though KLAP is available.
    let mut seed = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);
    let resp = crate::http::post(
        host,
        "/app/handshake1",
        "application/octet-stream",
        None,
        &seed,
    )
    .await?;
    if resp.status == 200 {
        Ok(Protocol::Klap)
    } else {
        Ok(Protocol::Passthrough)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInfo {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub fw_ver: String,
    #[serde(default)]
    pub hw_ver: String,
    #[serde(default)]
    pub mac: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub device_on: bool,
    #[serde(default)]
    pub brightness: Option<u8>,
    #[serde(default)]
    pub hue: Option<u16>,
    #[serde(default)]
    pub saturation: Option<u8>,
    #[serde(default)]
    pub color_temp: Option<u16>,
    /// Base64-decoded friendly name (Tapo stores `nickname` base64-encoded).
    #[serde(default)]
    pub nickname: String,
    /// Anything else the firmware reports, kept for diagnostics.
    #[serde(flatten)]
    pub extra: Value,
}

/// A connected Tapo device.
pub struct TapoDevice {
    transport: Transport,
    protocol: Protocol,
}

impl TapoDevice {
    /// Connect to `host` (IP/hostname, no scheme) using local credentials,
    /// auto-detecting the protocol the firmware speaks.
    pub async fn connect(host: &str, username: &str, password: &str) -> Result<Self> {
        let protocol = detect_protocol(host).await?;
        Self::connect_with(host, username, password, protocol).await
    }

    /// Connect using an explicitly chosen protocol (skips the probe).
    pub async fn connect_with(
        host: &str,
        username: &str,
        password: &str,
        protocol: Protocol,
    ) -> Result<Self> {
        let transport = match protocol {
            Protocol::Klap => {
                Transport::Klap(KlapClient::connect(host, username, password).await?)
            }
            Protocol::Passthrough => {
                Transport::Passthrough(PassthroughClient::connect(host, username, password).await?)
            }
        };
        Ok(Self {
            transport,
            protocol,
        })
    }

    /// The protocol negotiated with the device.
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    async fn rpc(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let request_time_mils = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut req = json!({
            "method": method,
            "requestTimeMils": request_time_mils,
        });
        if let Some(p) = params {
            req["params"] = p;
        }
        let body = serde_json::to_vec(&req)?;
        let resp = self.transport.request_raw(&body).await?;
        let v: Value = serde_json::from_slice(&resp).map_err(|_| TapoError::InvalidPayload)?;
        match v.get("error_code").and_then(Value::as_i64) {
            Some(0) | None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
            Some(code) => Err(TapoError::DeviceError(code)),
        }
    }

    /// Read the current device state.
    pub async fn get_device_info(&self) -> Result<DeviceInfo> {
        let result = self.rpc("get_device_info", None).await?;
        let mut info: DeviceInfo =
            serde_json::from_value(result).map_err(|_| TapoError::InvalidPayload)?;
        if !info.nickname.is_empty() {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&info.nickname) {
                if let Ok(s) = String::from_utf8(bytes) {
                    info.nickname = s;
                }
            }
        }
        Ok(info)
    }

    /// Raw `get_device_info` value (every field the firmware reports).
    pub async fn get_device_info_raw(&self) -> Result<Value> {
        self.rpc("get_device_info", None).await
    }

    /// Escape hatch: invoke any JSON-RPC method with optional params. Used for
    /// protocol introspection / debugging.
    pub async fn call_raw(&self, method: &str, params: Option<Value>) -> Result<Value> {
        self.rpc(method, params).await
    }

    async fn set(&self, params: Value) -> Result<()> {
        self.rpc("set_device_info", Some(params)).await?;
        Ok(())
    }

    /// Turn the bulb on or off.
    pub async fn set_power(&self, on: bool) -> Result<()> {
        self.set(json!({ "device_on": on })).await
    }

    /// Run a smooth rainbow **on the bulb itself** using the L530's built-in
    /// dynamic-light-effect rule engine (the mechanism the Tapo app uses for
    /// bulbs — `edit_dynamic_light_effect_rule` + enable). The firmware fades
    /// between the colour stops, so it's smooth with zero per-frame traffic.
    ///
    /// `brightness` 1..=100. `transition_ms` and `display_ms` tune the on-bulb
    /// fade/hold timing per stop.
    pub async fn set_rainbow_effect(
        &self,
        change_time_ms: u32,
        brightness: u8,
    ) -> Result<()> {
        // The L530's "L1" rule requires EXACTLY 8 stops, each
        // [brightness, hue, saturation, colortemp]. `change_mode: "bln"` is the
        // firmware's smooth blend/fade; `change_time` is the fade duration.
        let b = brightness.clamp(1, 100) as u32;
        let stops: Vec<[u32; 4]> = (0..8).map(|i| [b, i * 45, 100, 0]).collect();
        let rule = json!({
            "id": "L1",
            "scene_name": "Y29sb3Jlcw==",
            "change_mode": "bln",
            "change_time": change_time_ms.clamp(400, 8000),
            "color_status_list": stops,
        });
        // Editing may briefly fail if the bulb is mid-handshake; the enable is
        // what actually starts the smooth on-device animation.
        let _ = self.rpc("edit_dynamic_light_effect_rule", Some(rule)).await;
        self.rpc(
            "set_dynamic_light_effect_rule_enable",
            Some(json!({ "enable": true, "id": "L1" })),
        )
        .await?;
        Ok(())
    }

    /// Stop any running dynamic effect and settle on a plain state.
    pub async fn clear_effect(&self) -> Result<()> {
        let _ = self
            .rpc(
                "set_dynamic_light_effect_rule_enable",
                Some(json!({ "enable": false })),
            )
            .await;
        self.set(json!({ "device_on": true, "dynamic_light_effect_enable": 0 }))
            .await
    }

    /// Set brightness (1..=100).
    pub async fn set_brightness(&self, value: u8) -> Result<()> {
        if !(1..=100).contains(&value) {
            return Err(TapoError::InvalidParam(format!(
                "brightness must be 1..=100, got {value}"
            )));
        }
        self.set(json!({ "device_on": true, "brightness": value }))
            .await
    }

    /// Set hue/saturation **and** brightness in a single request (used by the
    /// ambilight so the bulb brightness tracks the scene, capped by the user's
    /// max). hue 0..=360, saturation 0..=100, brightness 1..=100.
    pub async fn set_color_brightness(
        &self,
        hue: u16,
        saturation: u8,
        brightness: u8,
    ) -> Result<()> {
        self.set(json!({
            "device_on": true,
            "hue": hue.min(360),
            "saturation": saturation.min(100),
            "color_temp": 0,
            "brightness": brightness.clamp(1, 100),
        }))
        .await
    }

    /// Set HSV color: hue 0..=360, saturation 0..=100.
    /// (Tapo derives a sensible brightness; pass `brightness` too if you want.)
    pub async fn set_hue_saturation(&self, hue: u16, saturation: u8) -> Result<()> {
        if hue > 360 {
            return Err(TapoError::InvalidParam(format!(
                "hue must be 0..=360, got {hue}"
            )));
        }
        if saturation > 100 {
            return Err(TapoError::InvalidParam(format!(
                "saturation must be 0..=100, got {saturation}"
            )));
        }
        self.set(json!({
            "device_on": true,
            "hue": hue,
            "saturation": saturation,
            "color_temp": 0,
        }))
        .await
    }

    /// Set color temperature in Kelvin (typically ~2500..=6500).
    pub async fn set_color_temp(&self, kelvin: u16) -> Result<()> {
        if !(2500..=6500).contains(&kelvin) {
            return Err(TapoError::InvalidParam(format!(
                "color_temp must be 2500..=6500 K, got {kelvin}"
            )));
        }
        self.set(json!({ "device_on": true, "color_temp": kelvin }))
            .await
    }

    /// Convenience: set an sRGB color (0..=255 per channel). Converts to HSV.
    pub async fn set_rgb(&self, r: u8, g: u8, b: u8) -> Result<()> {
        let (h, s, _v) = rgb_to_hsv(r, g, b);
        self.set_hue_saturation(h, s).await
    }

}

/// sRGB -> HSV. Returns (hue 0..=360, saturation 0..=100, value 0..=100).
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (u16, u8, u8) {
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let mut h = if delta == 0.0 {
        0.0
    } else if max == rf {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if max == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    if h < 0.0 {
        h += 360.0;
    }
    let s = if max == 0.0 { 0.0 } else { delta / max };
    (
        h.round() as u16 % 361,
        (s * 100.0).round() as u8,
        (max * 100.0).round() as u8,
    )
}

/// HSV -> sRGB. hue 0..=360, saturation 0..=100, value 0..=100.
pub fn hsv_to_rgb(h: u16, s: u8, v: u8) -> (u8, u8, u8) {
    let h = (h % 360) as f64;
    let s = (s.min(100)) as f64 / 100.0;
    let v = (v.min(100)) as f64 / 100.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_rgb_roundtrip_primaries() {
        assert_eq!(hsv_to_rgb(0, 100, 100), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120, 100, 100), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240, 100, 100), (0, 0, 255));
    }

    #[test]
    fn rgb_to_hsv_primaries() {
        assert_eq!(rgb_to_hsv(255, 0, 0), (0, 100, 100));
        assert_eq!(rgb_to_hsv(0, 255, 0), (120, 100, 100));
        assert_eq!(rgb_to_hsv(0, 0, 255), (240, 100, 100));
        assert_eq!(rgb_to_hsv(0, 0, 0), (0, 0, 0));
        assert_eq!(rgb_to_hsv(255, 255, 255), (0, 0, 100));
    }
}
