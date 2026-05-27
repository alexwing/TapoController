//! LAN discovery of Tapo devices. Scans the local /24 for hosts whose
//! embedded HTTP server identifies as Tapo ("SHIP") and exposes the KLAP
//! `/app/handshake1` endpoint. No credentials needed for discovery.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::http;

#[derive(Debug, Clone, Serialize)]
pub struct Discovered {
    pub ip: String,
    /// True if `/app/handshake1` exists (KLAP local API present).
    pub klap: bool,
}

/// Best-effort local IPv4 of the primary interface (no packets actually sent).
fn local_ipv4() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()?.ip() {
        IpAddr::V4(v) => Some(v),
        _ => None,
    }
}

/// Probe one host: is it a Tapo "SHIP" device on :80?
async fn probe(ip: String) -> Option<Discovered> {
    let head = timeout(Duration::from_millis(700), async {
        let mut st = TcpStream::connect((ip.as_str(), 80)).await.ok()?;
        let req = format!(
            "GET / HTTP/1.1\r\nHost: {ip}\r\nConnection: close\r\n\r\n"
        );
        st.write_all(req.as_bytes()).await.ok()?;
        let mut buf = [0u8; 512];
        let n = st.read(&mut buf).await.ok()?;
        Some(String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase())
    })
    .await
    .ok()??;

    if !head.contains("server: ship") {
        return None;
    }
    // Confirm the KLAP local API endpoint exists.
    let klap = matches!(
        http::post(&ip, "/app/handshake1", "application/octet-stream", None, &[0u8; 16]).await,
        Ok(r) if matches!(r.status, 200 | 400 | 403)
    );
    Some(Discovered { ip, klap })
}

/// Scan the local /24 and return Tapo-like devices found.
pub async fn discover() -> Vec<Discovered> {
    let Some(base) = local_ipv4() else {
        return Vec::new();
    };
    let o = base.octets();
    let prefix = format!("{}.{}.{}", o[0], o[1], o[2]);

    let mut handles = Vec::with_capacity(254);
    for i in 1..=254u8 {
        let ip = format!("{prefix}.{i}");
        handles.push(tokio::spawn(probe(ip)));
    }
    let mut found = Vec::new();
    for h in handles {
        if let Ok(Some(d)) = h.await {
            found.push(d);
        }
    }
    // Stable order by last octet.
    found.sort_by_key(|d| {
        d.ip.rsplit('.').next().and_then(|s| s.parse::<u8>().ok()).unwrap_or(0)
    });
    found
}
