use thiserror::Error;

/// Errors that can occur while talking to a Tapo device over the local KLAP protocol.
#[derive(Debug, Error)]
pub enum TapoError {
    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON (de)serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("handshake1 failed: device returned status {0}")]
    Handshake1Status(u16),

    #[error("handshake1 failed: unexpected response length {0} (expected 48)")]
    Handshake1Length(usize),

    #[error(
        "handshake1 auth mismatch: the credentials do not match the ones the bulb was \
         provisioned with (server_hash != local computed hash)"
    )]
    AuthMismatch,

    #[error("handshake2 failed: device returned status {0}")]
    Handshake2Status(u16),

    #[error("no TP_SESSIONID cookie returned by device during handshake1")]
    MissingSessionCookie,

    #[error("encrypted request failed: device returned status {0}")]
    RequestStatus(u16),

    #[error("response too short to contain signature + ciphertext ({0} bytes)")]
    ResponseTooShort(usize),

    #[error("response signature verification failed (possible session desync)")]
    ResponseSignatureMismatch,

    #[error("decrypted payload is not valid UTF-8 JSON")]
    InvalidPayload,

    #[error("device returned application error_code {0}")]
    DeviceError(i64),

    #[error("session not established (call connect() first)")]
    NotConnected,

    #[error("invalid parameter: {0}")]
    InvalidParam(String),
}

pub type Result<T> = std::result::Result<T, TapoError>;
