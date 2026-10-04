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

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Discovered {
    pub ip: String,
    /// True if `/app/handshake1` exists (KLAP local API present).
    pub klap: bool,
    /// Hardware MAC address if resolvable via ARP or query.
    #[serde(default)]
    pub mac: Option<String>,
}

/// Normalize MAC string for consistent matching (uppercase, hex digits only).
pub fn normalize_mac(mac: &str) -> String {
    mac.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Query local ARP table / cache for IPv4 MAC address.
pub fn get_arp_mac(ip: &str) -> Option<String> {
    let ipv4: Ipv4Addr = ip.parse().ok()?;
    #[cfg(windows)]
    {
        #[link(name = "iphlpapi")]
        extern "system" {
            fn SendARP(dest_ip: u32, src_ip: u32, p_mac: *mut u8, p_len: *mut u32) -> u32;
        }
        let mut mac = [0u8; 6];
        let mut len = 6u32;
        let dest = u32::from_ne_bytes(ipv4.octets());
        let res = unsafe { SendARP(dest, 0, mac.as_mut_ptr(), &mut len) };
        if res == 0 && len == 6 {
            return Some(format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ));
        }
    }
    None
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
    let mac = get_arp_mac(&ip);
    Some(Discovered { ip, klap, mac })
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

/// Find current IP of a device with `target_mac` on the local /24 using parallel probing and ARP.
pub async fn find_ip_by_mac(target_mac: &str) -> Option<String> {
    let norm = normalize_mac(target_mac);
    if norm.is_empty() {
        return None;
    }

    // Run parallel discovery (probes all 254 IPs concurrently in ~700ms, populating ARP)
    let found = discover().await;
    for d in &found {
        if let Some(ref mac) = d.mac {
            if normalize_mac(mac) == norm {
                return Some(d.ip.clone());
            }
        }
    }

    None
}

/// Robust IP resolver: checks discovered devices via fast ARP, and if needed,
/// connects to candidate Tapo devices concurrently using `username`/`password` to query firmware `get_device_info().mac`.
pub async fn resolve_ip_by_mac_with_auth(
    target_mac: &str,
    username: &str,
    password: &str,
) -> Option<String> {
    let norm = normalize_mac(target_mac);
    if norm.is_empty() {
        return None;
    }

    // Run fast parallel discovery
    let found = discover().await;
    for d in &found {
        if let Some(ref mac) = d.mac {
            if normalize_mac(mac) == norm {
                return Some(d.ip.clone());
            }
        }
    }

    if username.is_empty() {
        return None;
    }

    // Authenticate with candidate Tapo devices in parallel
    let mut tasks = Vec::with_capacity(found.len());
    for d in found {
        let u = username.to_string();
        let p = password.to_string();
        tasks.push(async move {
            let probe = tokio::time::timeout(Duration::from_millis(2000), async {
                let dev = crate::device::TapoDevice::connect(&d.ip, &u, &p).await?;
                dev.get_device_info().await
            })
            .await;
            if let Ok(Ok(info)) = probe {
                Some((d.ip, info.mac))
            } else {
                None
            }
        });
    }

    let results = futures_util::future::join_all(tasks).await;
    for (ip, mac) in results.into_iter().flatten() {
        if normalize_mac(&mac) == norm {
            return Some(ip);
        }
    }

    None
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_normalization() {
        assert_eq!(normalize_mac("3C-6A-D2-7A-13-3E"), "3C6AD27A133E");
        assert_eq!(normalize_mac("3c:6a:d2:7a:13:3e"), "3C6AD27A133E");
        assert_eq!(normalize_mac("3C6AD27A133E"), "3C6AD27A133E");
    }

    #[test]
    #[cfg(windows)]
    fn arp_lookup_gateway_or_local() {
        if let Some(ip) = local_ipv4() {
            let o = ip.octets();
            let gateway = format!("{}.{}.{}.1", o[0], o[1], o[2]);
            if let Some(mac) = get_arp_mac(&gateway) {
                assert_eq!(mac.len(), 17);
                assert_eq!(mac.matches(':').count(), 5);
            }
        }
    }
}

