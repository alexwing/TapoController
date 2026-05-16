//! Reusable connectivity/diagnosis helper. Given a host, figure out *why*
//! control might not work and return a human-actionable verdict. Used by the
//! CLI `doctor` command (and available to the UI).

use serde::Serialize;

use crate::http;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Verdict {
    /// Host unreachable on the LAN.
    Unreachable,
    /// KLAP firmware with the local API gated off (HTTP 403 on handshake1).
    /// The user must enable "Third-Party Services" in the Tapo app once.
    KlapGatedOff,
    /// KLAP available (handshake1 returns 200) — should work with creds.
    KlapReady,
    /// Legacy securePassthrough firmware (SHIP 2.0) — should work with creds.
    PassthroughReady,
    /// Reachable but didn't match a known pattern.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnosis {
    pub host: String,
    pub reachable: bool,
    pub handshake1_status: Option<u16>,
    pub app_root_status: Option<u16>,
    pub verdict: Verdict,
    pub message: String,
}

/// Probe a host without credentials and return an actionable diagnosis.
pub async fn diagnose(host: &str) -> Diagnosis {
    let app_root_status = http::get(host, "/").await.ok().map(|r| r.status);

    // Random seed: this firmware 400s an all-zero handshake1 body.
    let mut seed = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut seed);
    let hs1 = http::post(
        host,
        "/app/handshake1",
        "application/octet-stream",
        None,
        &seed,
    )
    .await;

    let (reachable, handshake1_status) = match &hs1 {
        Ok(r) => (true, Some(r.status)),
        Err(_) => (app_root_status.is_some(), None),
    };

    let (verdict, message) = match (reachable, handshake1_status) {
        (false, _) => (
            Verdict::Unreachable,
            format!("No hay respuesta de {host} en la LAN. Revisa IP/red/encendido."),
        ),
        (true, Some(200)) => (
            Verdict::KlapReady,
            "KLAP disponible (handshake1=200). Debería funcionar con credenciales \
             correctas en tapo-config.toml."
                .into(),
        ),
        (true, Some(403)) => (
            Verdict::KlapGatedOff,
            "KLAP presente pero la API local está DESACTIVADA (handshake1=403). \
             Abre la app Tapo -> tu cuenta -> 'Servicios de terceros / Third-Party \
             Compatibility' -> ACTIVAR. No hay forma software de evitar esto."
                .into(),
        ),
        (true, _) if app_root_status.is_some() => (
            Verdict::PassthroughReady,
            "Firmware legacy securePassthrough (handshake1 no-200 pero /app \
             responde). Debería funcionar con credenciales correctas."
                .into(),
        ),
        _ => (
            Verdict::Unknown,
            "Dispositivo accesible pero patrón no reconocido.".into(),
        ),
    };

    Diagnosis {
        host: host.to_string(),
        reachable,
        handshake1_status,
        app_root_status,
        verdict,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unreachable_host_is_classified() {
        // 192.0.2.0/24 is TEST-NET-1 (RFC 5737), guaranteed non-routable.
        let d = diagnose("192.0.2.123").await;
        assert!(!d.reachable);
        assert_eq!(d.verdict, Verdict::Unreachable);
    }
}
