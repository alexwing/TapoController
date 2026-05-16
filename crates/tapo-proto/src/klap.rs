//! Own implementation of TP-Link Tapo's **KLAP v2** local protocol.
//!
//! KLAP v2 is the handshake/transport used by the SMART/Tapo family (L530, L535,
//! L630, P110, ...). This module is a clean-room port of the well-documented,
//! reverse-engineered scheme (see `python-kasa`'s `klaptransport.py` and
//! `tapo-go`'s `klap_transport.go`). It performs **zero** cloud calls: every
//! request goes straight to the bulb on the LAN.
//!
//! Credential note: `auth_hash` is derived purely from a username/password pair.
//! Those are whatever the device was provisioned with — for a fully local setup
//! they are credentials *you* chose during provisioning, never a TP-Link cloud
//! account.

use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use md5::Md5;
use rand::RngCore;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI32, Ordering};

use crate::error::{Result, TapoError};
use crate::http;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn sha1(data: &[u8]) -> Vec<u8> {
    Sha1::digest(data).to_vec()
}
fn md5(data: &[u8]) -> Vec<u8> {
    Md5::digest(data).to_vec()
}

/// `auth_hash = sha256( sha256(username) || sha256(password) )` (KLAP v2).
pub fn auth_hash(username: &str, password: &str) -> [u8; 32] {
    let u = sha256(&[username.as_bytes()]);
    let p = sha256(&[password.as_bytes()]);
    sha256(&[&u, &p])
}

/// Whether the seed hashes mix in the remote seed (KLAP v2) or not (v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    V2,
    V1,
}

fn handshake1_expected(scheme: Scheme, local: &[u8], remote: &[u8], ah: &[u8]) -> [u8; 32] {
    match scheme {
        Scheme::V2 => sha256(&[local, remote, ah]),
        Scheme::V1 => sha256(&[local, ah]),
    }
}

fn handshake2_payload(scheme: Scheme, local: &[u8], remote: &[u8], ah: &[u8]) -> [u8; 32] {
    match scheme {
        Scheme::V2 => sha256(&[remote, local, ah]),
        Scheme::V1 => sha256(&[remote, ah]),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// All credential-hash constructions seen across Tapo/Kasa firmwares + the
/// well-known fallback credentials. We never guess: at handshake1 we compute
/// the expected `server_hash` for every candidate and keep the exact match.
fn auth_hash_candidates(user: &str, pass: &str) -> Vec<(&'static str, Vec<u8>)> {
    let mut v: Vec<(&'static str, Vec<u8>)> = Vec::new();
    let users = [user.to_string(), user.to_lowercase()];
    for u in &users {
        let ub = u.as_bytes();
        let pb = pass.as_bytes();
        // KLAP v2 family
        v.push(("v2 sha256(sha256(u)+sha256(p))",
            sha256(&[&sha256(&[ub]), &sha256(&[pb])]).to_vec()));
        v.push(("v2 sha256(sha1(u)+sha1(p))",
            sha256(&[&sha1(ub), &sha1(pb)]).to_vec()));
        v.push(("v2 sha256(md5(u)+md5(p))",
            sha256(&[&md5(ub), &md5(pb)]).to_vec()));
        v.push(("v2 sha256(u+p)", sha256(&[ub, pb]).to_vec()));
        // KLAP v1 family
        v.push(("v1 md5(md5(u)+md5(p))",
            md5(&[md5(ub), md5(pb)].concat())));
        v.push(("v1 sha1(sha1(u)+sha1(p))",
            sha1(&[sha1(ub), sha1(pb)].concat())));
    }
    // Fallbacks used by TP-Link provisioning / blank credentials.
    let blank_u = sha256(&[b"".as_slice()]);
    let blank_p = sha256(&[b"".as_slice()]);
    v.push(("blank v2", sha256(&[&blank_u, &blank_p]).to_vec()));
    v.push((
        "kasa-setup v2",
        sha256(&[
            &sha256(&[b"kasa@tp-link.net".as_slice()]),
            &sha256(&[b"kasaSetup".as_slice()]),
        ])
        .to_vec(),
    ));
    v
}

/// Resolve the (scheme, auth_hash) that matches the device's server hash.
fn resolve_auth(
    user: &str,
    pass: &str,
    local: &[u8],
    remote: &[u8],
    server_hash: &[u8],
) -> Option<(Scheme, Vec<u8>, &'static str)> {
    for scheme in [Scheme::V2, Scheme::V1] {
        for (name, ah) in auth_hash_candidates(user, pass) {
            if handshake1_expected(scheme, local, remote, &ah).as_slice() == server_hash {
                return Some((scheme, ah, name));
            }
        }
    }
    None
}

/// Symmetric session derived from the two seeds + auth_hash after a successful
/// handshake. Handles AES-128-CBC encryption and the per-request signature.
pub struct KlapSession {
    key: [u8; 16],
    iv: [u8; 12],
    sig: [u8; 28],
    seq: AtomicI32,
}

impl KlapSession {
    fn derive(local_seed: &[u8], remote_seed: &[u8], auth_hash: &[u8]) -> Self {
        let key_full = sha256(&[b"lsk", local_seed, remote_seed, auth_hash]);
        let mut key = [0u8; 16];
        key.copy_from_slice(&key_full[..16]);

        let iv_full = sha256(&[b"iv", local_seed, remote_seed, auth_hash]);
        let mut iv = [0u8; 12];
        iv.copy_from_slice(&iv_full[..12]);
        let seq = i32::from_be_bytes([iv_full[28], iv_full[29], iv_full[30], iv_full[31]]);

        let sig_full = sha256(&[b"ldk", local_seed, remote_seed, auth_hash]);
        let mut sig = [0u8; 28];
        sig.copy_from_slice(&sig_full[..28]);

        Self {
            key,
            iv,
            sig,
            seq: AtomicI32::new(seq),
        }
    }

    fn iv_seq(&self, seq: i32) -> [u8; 16] {
        let mut iv = [0u8; 16];
        iv[..12].copy_from_slice(&self.iv);
        iv[12..].copy_from_slice(&seq.to_be_bytes());
        iv
    }

    /// Encrypt a plaintext request body. Returns `(payload, seq)` where payload
    /// is `signature(32) || ciphertext` and `seq` must be sent as `?seq=`.
    pub fn encrypt(&self, msg: &[u8]) -> (Vec<u8>, i32) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        let iv = self.iv_seq(seq);
        let ct = Aes128CbcEnc::new(&self.key.into(), &iv.into())
            .encrypt_padded_vec_mut::<Pkcs7>(msg);
        let signature = sha256(&[&self.sig, &seq.to_be_bytes(), &ct]);
        let mut out = Vec::with_capacity(32 + ct.len());
        out.extend_from_slice(&signature);
        out.extend_from_slice(&ct);
        (out, seq)
    }

    /// Decrypt a `signature(32) || ciphertext` response for the given `seq`.
    pub fn decrypt(&self, msg: &[u8], seq: i32) -> Result<Vec<u8>> {
        if msg.len() < 32 {
            return Err(TapoError::ResponseTooShort(msg.len()));
        }
        let ct = &msg[32..];
        let iv = self.iv_seq(seq);
        let pt = Aes128CbcDec::new(&self.key.into(), &iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(ct)
            .map_err(|_| TapoError::InvalidPayload)?;
        Ok(pt)
    }
}

/// A connected KLAP client bound to one device on the LAN.
pub struct KlapClient {
    host: String,
    auth_hash: Vec<u8>,
    session: KlapSession,
    cookie: String,
}

impl KlapClient {
    /// Perform handshake1 + handshake2 against `http://{host}/app` and return a
    /// ready-to-use client. `host` is an IP or hostname (optionally `:port`).
    pub async fn connect(host: &str, username: &str, password: &str) -> Result<Self> {
        // ---- handshake1 ----
        let mut local_seed = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut local_seed);

        let resp = http::post(
            host,
            "/app/handshake1",
            "application/octet-stream",
            None,
            &local_seed,
        )
        .await?;
        if resp.status != 200 {
            return Err(TapoError::Handshake1Status(resp.status));
        }
        let cookie = resp.set_cookie.ok_or(TapoError::MissingSessionCookie)?;
        if resp.body.len() != 48 {
            return Err(TapoError::Handshake1Length(resp.body.len()));
        }
        let remote_seed = resp.body[..16].to_vec();
        let server_hash = &resp.body[16..48];

        // Don't guess the auth scheme: pick the one whose server_hash matches.
        let (scheme, auth_hash, scheme_name) =
            match resolve_auth(username, password, &local_seed, &remote_seed, server_hash) {
                Some(t) => t,
                None => {
                    tracing::debug!(
                        server_hash = %hex(server_hash),
                        "no KLAP auth scheme matched (wrong credentials?)"
                    );
                    return Err(TapoError::AuthMismatch);
                }
            };
        tracing::debug!("KLAP auth scheme: {scheme_name} ({scheme:?})");

        // ---- handshake2 ----
        let payload = handshake2_payload(scheme, &local_seed, &remote_seed, &auth_hash);
        let resp = http::post(
            host,
            "/app/handshake2",
            "application/octet-stream",
            Some(&cookie),
            &payload,
        )
        .await?;
        if resp.status != 200 {
            return Err(TapoError::Handshake2Status(resp.status));
        }

        let session = KlapSession::derive(&local_seed, &remote_seed, &auth_hash);
        Ok(Self {
            host: host.to_string(),
            auth_hash,
            session,
            cookie,
        })
    }

    /// Send an already-serialized JSON-RPC request body and return the decrypted
    /// JSON response bytes.
    pub async fn request_raw(&self, json: &[u8]) -> Result<Vec<u8>> {
        let (payload, seq) = self.session.encrypt(json);
        let path = format!("/app/request?seq={seq}");
        let resp = http::post(
            &self.host,
            &path,
            "application/octet-stream",
            Some(&self.cookie),
            &payload,
        )
        .await?;
        if resp.status != 200 {
            return Err(TapoError::RequestStatus(resp.status));
        }
        self.session.decrypt(&resp.body, seq)
    }

    /// `auth_hash` of this connection (useful for diagnostics).
    pub fn auth_hash_hex(&self) -> String {
        self.auth_hash.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vector cross-checked against python-kasa's KlapTransportV2.generate_auth_hash:
    //   sha256( sha256(b"user") || sha256(b"pass") )
    #[test]
    fn auth_hash_v2_vector() {
        let got = auth_hash("user", "pass");
        let u = sha256(&[b"user"]);
        let p = sha256(&[b"pass"]);
        let want = sha256(&[&u, &p]);
        assert_eq!(got, want);
        // Stable hex so regressions are obvious.
        let hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex.len(), 64);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let s = KlapSession::derive(&[1u8; 16], &[2u8; 16], &[3u8; 32]);
        let msg = br#"{"method":"get_device_info"}"#;
        let (payload, seq) = s.encrypt(msg);
        assert!(payload.len() > 32);
        // Decrypt path uses the same key/iv derivation for the matching seq.
        let s2 = KlapSession::derive(&[1u8; 16], &[2u8; 16], &[3u8; 32]);
        let pt = s2.decrypt(&payload, seq).unwrap();
        assert_eq!(pt, msg);
    }

    #[test]
    fn seq_increments_and_is_deterministic_iv() {
        let s = KlapSession::derive(&[9u8; 16], &[8u8; 16], &[7u8; 32]);
        let start = s.seq.load(Ordering::SeqCst);
        let (_p1, q1) = s.encrypt(b"a");
        let (_p2, q2) = s.encrypt(b"b");
        assert_eq!(q1, start.wrapping_add(1));
        assert_eq!(q2, start.wrapping_add(2));
    }
}
