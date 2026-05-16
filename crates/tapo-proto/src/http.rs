//! Minimal raw HTTP/1.1 client.
//!
//! Why not `reqwest`/`hyper`? The Tapo "SHIP 2.0" embedded HTTP server is
//! **case-sensitive on header names**: it only accepts `Content-Length:` /
//! `Host:` in Title-Case and replies `400 Bad Request` to the lowercase header
//! names hyper emits. We therefore hand-write requests with Title-Case headers
//! over a plain TCP socket. We always send `Connection: close` and read the
//! response to EOF, which keeps parsing trivial and robust for this firmware.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::error::{Result, TapoError};

pub struct RawResponse {
    pub status: u16,
    pub set_cookie: Option<String>,
    pub body: Vec<u8>,
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn split_host_port(host: &str) -> (String, u16) {
    match host.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(port) => (h.to_string(), port),
            Err(_) => (host.to_string(), 80),
        },
        None => (host.to_string(), 80),
    }
}

/// Perform one HTTP request with Title-Case headers. `path` may include a query
/// string. `body` is sent verbatim; `content_type` and optional `cookie` are
/// added as headers.
pub async fn request(
    method: &str,
    host: &str,
    path: &str,
    content_type: &str,
    cookie: Option<&str>,
    body: &[u8],
) -> Result<RawResponse> {
    let (hostname, port) = split_host_port(host);
    let io = async {
        let mut stream = TcpStream::connect((hostname.as_str(), port)).await?;

        // `Connection: close` is sent but this firmware ignores it and keeps
        // the socket open, so we MUST length-delimit using Content-Length
        // instead of reading to EOF.
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\n\
             Host: {hostname}\r\n\
             Accept: */*\r\n\
             User-Agent: TapoController/0.1\r\n\
             Content-Type: {content_type}\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n",
            body.len()
        );
        if let Some(c) = cookie {
            head.push_str(&format!("Cookie: {c}\r\n"));
        }
        head.push_str("\r\n");

        stream.write_all(head.as_bytes()).await?;
        stream.write_all(body).await?;
        stream.flush().await?;

        let mut buf: Vec<u8> = Vec::with_capacity(512);
        let mut chunk = [0u8; 2048];

        // 1. Read until the end of headers.
        let header_end = loop {
            if let Some(p) = find_subslice(&buf, b"\r\n\r\n") {
                break p;
            }
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                break find_subslice(&buf, b"\r\n\r\n").unwrap_or(buf.len());
            }
            buf.extend_from_slice(&chunk[..n]);
        };

        // 2. Determine body length from Content-Length and read exactly that.
        let header_text = String::from_utf8_lossy(&buf[..header_end]);
        let content_len = header_text
            .split("\r\n")
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                if k.trim().eq_ignore_ascii_case("content-length") {
                    v.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let body_start = header_end + 4;
        while buf.len() < body_start + content_len {
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        Ok::<Vec<u8>, std::io::Error>(buf)
    };

    let raw = timeout(Duration::from_secs(6), io)
        .await
        .map_err(|_| TapoError::InvalidParam("HTTP timeout".into()))?
        .map_err(|e| TapoError::InvalidParam(format!("TCP error: {e}")))?;

    parse_response(&raw)
}

pub async fn post(
    host: &str,
    path: &str,
    content_type: &str,
    cookie: Option<&str>,
    body: &[u8],
) -> Result<RawResponse> {
    request("POST", host, path, content_type, cookie, body).await
}

pub async fn get(host: &str, path: &str) -> Result<RawResponse> {
    request("GET", host, path, "application/octet-stream", None, &[])
        .await
}

fn parse_response(raw: &[u8]) -> Result<RawResponse> {
    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(TapoError::InvalidPayload)?;
    let header_text = String::from_utf8_lossy(&raw[..sep]);
    let body = raw[sep + 4..].to_vec();

    let mut lines = header_text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or(TapoError::InvalidPayload)?;

    let mut set_cookie = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("set-cookie") {
                // Keep only the `TP_SESSIONID=...` pair.
                let first = v.trim().split(';').next().unwrap_or("").to_string();
                if first.starts_with("TP_SESSIONID=") {
                    set_cookie = Some(first);
                }
            }
        }
    }

    Ok(RawResponse {
        status,
        set_cookie,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_cookie_body() {
        let raw = b"HTTP/1.1 200 OK\r\nServer: SHIP 2.0\r\nSet-Cookie: TP_SESSIONID=ABC;TIMEOUT=86400\r\nContent-Length: 3\r\n\r\nabc";
        let r = parse_response(raw).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.set_cookie.as_deref(), Some("TP_SESSIONID=ABC"));
        assert_eq!(r.body, b"abc");
    }

    #[test]
    fn host_port_split() {
        assert_eq!(split_host_port("1.2.3.4"), ("1.2.3.4".into(), 80));
        assert_eq!(split_host_port("1.2.3.4:8080"), ("1.2.3.4".into(), 8080));
    }
}
