//! The signatures' wire.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct FailedChain {
    pub chain: String,
    pub why: String,
}

/// `POST /api/act/device`: a push token is a fact about a phone, not the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DevicePayload {
    /// Always true; a failure is a refusal.
    pub ok: bool,
    pub devices: u32,
}

/// `POST /api/act/device`, signed by the page, since the Android portal holds no key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Registering {
    pub platform: crate::devices::Platform,
    pub token: String,
}

/// A sum, because the tab branches on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RefusalKind {
    /// Held and named, and everything resting on it with it.
    Final,
    /// Taken once its device is trusted and it is signed.
    Trust,
}
