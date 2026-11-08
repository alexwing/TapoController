//! `tapo-proto` — an own, cloud-free implementation of the TP-Link Tapo local
//! protocol (KLAP v2) for controlling bulbs such as the Tapo L530.
//!
//! Nothing in this crate ever contacts a TP-Link server: all traffic is direct
//! LAN HTTP to the device. Credentials are whatever the bulb was provisioned
//! with locally.

pub mod config;
pub mod device;
pub mod diagnose;
pub mod discover;
pub mod error;
pub mod http;
pub mod klap;
pub mod passthrough;
pub mod service;

pub use config::{
    ApiConfig, BulbConfig, DeviceConfig, DiscoConfig, StreamConfig, TapoConfig, UiConfigFile,
};
pub use diagnose::{diagnose, Diagnosis, Verdict};
pub use discover::{
    discover, find_ip_by_mac, get_arp_mac, normalize_mac, resolve_ip_by_mac_with_auth, Discovered,
};
pub use device::{detect_protocol, DeviceInfo, Protocol, TapoDevice};
pub use error::{Result, TapoError};
pub use service::{BulbStateSummary, ControlService};



