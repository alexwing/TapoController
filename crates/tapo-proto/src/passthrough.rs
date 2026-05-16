//! Own implementation of TP-Link Tapo's **legacy "secure passthrough"** local
//! protocol (RSA-1024 key exchange + AES-128-CBC session + `login_device`
//! token). This is what older/`SHIP 2.0` firmware speaks (the bulb at
//! 192.168.1.226 uses this, not KLAP).
//!
//! Clean-room port of the widely documented scheme (PyP100 / python-kasa
//! `AesTransport`). Zero cloud traffic: all requests are direct LAN HTTP.

use aes::Aes128;
use base64::Engine;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use rsa::pkcs8::{EncodePublicKey, LineEnding};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::Digest;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Result, TapoError};
use crate::http;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;
const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn now_mils() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn sha1_hex(input: &str) -> String {
    let digest = Sha1::digest(input.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Credential encodings the various firmwares accept for `login_device`.
#[derive(Clone, Copy)]
enum LoginVariant {
    /// v1: username = b64(hex(sha1(user))), password = b64(pass)
    V1,
    /// v2: username = b64(hex(sha1(user))), password = b64(hex(sha1(pass)))
    V2,
}

impl LoginVariant {
    fn encode(self, user: &str, pass: &str) -> (String, String) {
        let un = B64.encode(sha1_hex(user));
        let pw = match self {
            LoginVariant::V1 => B64.encode(pass),
            LoginVariant::V2 => B64.encode(sha1_hex(pass)),
        };
        (un, pw)
    }
}

pub struct PassthroughClient {
    host: String,
    cookie: String,
    key: [u8; 16],
    iv: [u8; 16],
    token: Option<String>,
}

impl PassthroughClient {
    /// Full connect: RSA handshake -> AES session -> `login_device` (tries the
    /// known credential encodings) -> ready to send requests.
    pub async fn connect(host: &str, username: &str, password: &str) -> Result<Self> {
        // ---- RSA handshake ----
        // Scope the (non-Send) RNG so it is dropped before any `.await`.
        let priv_key = {
            let mut rng = rand::thread_rng();
            RsaPrivateKey::new(&mut rng, 1024)
                .map_err(|_| TapoError::InvalidParam("RSA keygen failed".into()))?
        };
        let pub_pem = RsaPublicKey::from(&priv_key)
            .to_public_key_pem(LineEnding::LF)
            .map_err(|_| TapoError::InvalidParam("RSA PEM encode failed".into()))?;

        let hs = json!({
            "method": "handshake",
            "params": { "key": pub_pem, "requestTimeMils": now_mils() }
        });
        let resp = http::post(
            host,
            "/app",
            "application/json",
            None,
            &serde_json::to_vec(&hs)?,
        )
        .await?;
        if resp.status != 200 {
            return Err(TapoError::Handshake1Status(resp.status));
        }
        let cookie = resp.set_cookie.ok_or(TapoError::MissingSessionCookie)?;
        let body: Value =
            serde_json::from_slice(&resp.body).map_err(|_| TapoError::InvalidPayload)?;
        check_error(&body)?;
        let enc_key_b64 = body
            .get("result")
            .and_then(|r| r.get("key"))
            .and_then(Value::as_str)
            .ok_or(TapoError::InvalidPayload)?;
        let enc_key = B64
            .decode(enc_key_b64)
            .map_err(|_| TapoError::InvalidPayload)?;
        let secret = priv_key
            .decrypt(Pkcs1v15Encrypt, &enc_key)
            .map_err(|_| TapoError::AuthMismatch)?;
        if secret.len() < 32 {
            return Err(TapoError::InvalidPayload);
        }
        let mut key = [0u8; 16];
        let mut iv = [0u8; 16];
        key.copy_from_slice(&secret[..16]);
        iv.copy_from_slice(&secret[16..32]);

        let mut client = Self {
            host: host.to_string(),
            cookie,
            key,
            iv,
            token: None,
        };

        // ---- login_device (try credential encodings) ----
        let mut last_err = TapoError::DeviceError(-1501);
        for variant in [LoginVariant::V1, LoginVariant::V2] {
            let (un, pw) = variant.encode(username, password);
            let login = json!({
                "method": "login_device",
                "params": { "username": un, "password": pw },
                "requestTimeMils": now_mils(),
            });
            match client.secure_request(&serde_json::to_vec(&login)?).await {
                Ok(resp) => {
                    let v: Value =
                        serde_json::from_slice(&resp).map_err(|_| TapoError::InvalidPayload)?;
                    if let Some(tok) = v
                        .get("result")
                        .and_then(|r| r.get("token"))
                        .and_then(Value::as_str)
                    {
                        client.token = Some(tok.to_string());
                        return Ok(client);
                    }
                    last_err = TapoError::DeviceError(
                        v.get("error_code").and_then(Value::as_i64).unwrap_or(-1501),
                    );
                }
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// Wrap `inner` in a `securePassthrough` envelope, POST it, and return the
    /// decrypted inner response bytes.
    async fn secure_request(&self, inner: &[u8]) -> Result<Vec<u8>> {
        let ct = Aes128CbcEnc::new(&self.key.into(), &self.iv.into())
            .encrypt_padded_vec_mut::<Pkcs7>(inner);
        let outer = json!({
            "method": "securePassthrough",
            "params": { "request": B64.encode(&ct) }
        });

        let path = match &self.token {
            Some(tok) => format!("/app?token={tok}"),
            None => "/app".to_string(),
        };
        let resp = http::post(
            &self.host,
            &path,
            "application/json",
            Some(&self.cookie),
            &serde_json::to_vec(&outer)?,
        )
        .await?;
        if resp.status != 200 {
            return Err(TapoError::RequestStatus(resp.status));
        }
        let body: Value =
            serde_json::from_slice(&resp.body).map_err(|_| TapoError::InvalidPayload)?;
        check_error(&body)?;
        let resp_b64 = body
            .get("result")
            .and_then(|r| r.get("response"))
            .and_then(Value::as_str)
            .ok_or(TapoError::InvalidPayload)?;
        let resp_ct = B64.decode(resp_b64).map_err(|_| TapoError::InvalidPayload)?;
        let pt = Aes128CbcDec::new(&self.key.into(), &self.iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(&resp_ct)
            .map_err(|_| TapoError::InvalidPayload)?;
        Ok(pt)
    }

    /// Send a device JSON-RPC request (already serialized) and return the
    /// decrypted JSON response bytes.
    pub async fn request_raw(&self, json: &[u8]) -> Result<Vec<u8>> {
        self.secure_request(json).await
    }
}

fn check_error(body: &Value) -> Result<()> {
    match body.get("error_code").and_then(Value::as_i64) {
        Some(0) | None => Ok(()),
        Some(code) => Err(TapoError::DeviceError(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_hex_known_vector() {
        // sha1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
        assert_eq!(sha1_hex("abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn login_encoding_shapes() {
        let (un, pw) = LoginVariant::V1.encode("a@b.com", "secret");
        assert!(!un.is_empty() && !pw.is_empty());
        assert_eq!(B64.decode(pw).unwrap(), b"secret");
        let (_, pw2) = LoginVariant::V2.encode("a@b.com", "secret");
        assert_eq!(B64.decode(pw2).unwrap().len(), 40); // hex sha1
    }
}
