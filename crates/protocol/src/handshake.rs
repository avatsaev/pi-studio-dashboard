//! Handshake wire types: the client `hello`, the server `status/server_info`, and the
//! capability set derived from `features` (D26, D30).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::lenient::cap_str;

/// Strings from `server_info` that can reach the UI are capped at decode (lenient rule 6).
const MAX_INFO_STR_BYTES: usize = 256;

/// Wire protocol version this client speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// `appVersion` prefix identifying this app to the daemon (D23).
pub const APP_VERSION_PREFIX: &str = "pistudiodashboard/";

/// `clientType` is a strict enum in pi-studio (`mobile|browser|cli|mcp`); we only ever send `cli` (D23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientType {
    Cli,
}

/// The first frame a client sends (top-level, not enveloped). Encoder-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub client_id: String,
    pub client_type: ClientType,
    pub protocol_version: u32,
    pub app_version: String,
}

impl Hello {
    /// `app_version` is the bare semver; the `pistudiodashboard/` prefix is added here.
    #[must_use]
    pub fn new(client_id: impl Into<String>, app_version: &str) -> Self {
        Self {
            client_id: client_id.into(),
            client_type: ClientType::Cli,
            protocol_version: PROTOCOL_VERSION,
            app_version: format!("{APP_VERSION_PREFIX}{app_version}"),
        }
    }

    /// The wire object. `capabilities` is always empty: the dashboard advertises nothing.
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "type": "hello",
            "clientId": self.client_id,
            "clientType": self.client_type,
            "protocolVersion": self.protocol_version,
            "appVersion": self.app_version,
            "capabilities": {},
        })
    }

    #[must_use]
    pub fn encode(&self) -> String {
        self.to_value().to_string()
    }
}

/// Why a `server_info` payload was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandshakeError {
    #[error("bad handshake: {0}")]
    BadHandshake(&'static str),
}

/// The decoded `status` payload with `status:"server_info"`.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerInfo {
    pub server_id: String,
    pub hostname: Option<String>,
    pub version: Option<String>,
    pub home_dir: Option<String>,
    pub capabilities: BTreeMap<String, Value>,
    pub features: BTreeMap<String, Value>,
}

fn capped(o: &Map<String, Value>, k: &str) -> Option<String> {
    o.get(k).and_then(Value::as_str).map(|s| cap_str(s, MAX_INFO_STR_BYTES).0)
}

fn map_of(o: &Map<String, Value>, k: &str) -> BTreeMap<String, Value> {
    match o.get(k) {
        Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => BTreeMap::new(),
    }
}

impl ServerInfo {
    /// Decode the `payload` of a `status` frame whose `status` is `server_info`.
    ///
    /// # Errors
    /// [`HandshakeError::BadHandshake`] when the payload is not an object or `serverId` is
    /// missing, not a string, or empty.
    pub fn from_payload(payload: &Value) -> Result<Self, HandshakeError> {
        let Value::Object(o) = payload else {
            return Err(HandshakeError::BadHandshake("server_info payload is not an object"));
        };
        let server_id = capped(o, "serverId").filter(|s| !s.trim().is_empty());
        let Some(server_id) = server_id else {
            return Err(HandshakeError::BadHandshake("server_info has no serverId"));
        };
        Ok(Self {
            server_id,
            hostname: capped(o, "hostname"),
            version: capped(o, "version"),
            home_dir: capped(o, "homeDir"),
            capabilities: map_of(o, "capabilities"),
            features: map_of(o, "features"),
        })
    }

    #[must_use]
    pub fn caps(&self) -> ServerCaps {
        ServerCaps::from_features(&self.features)
    }
}

/// Why a daemon is shown as degraded (D26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedReason {
    NoObservability,
}

/// Capabilities derived from `server_info.features`; only a literal `true` counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ServerCaps {
    pub daemon_observability: bool,
}

impl ServerCaps {
    /// `daemon_observability` iff `features.daemonObservability == true`. The legacy
    /// `daemonStatusRpc` flag is advertised but unimplemented, so it is ignored (D30).
    #[must_use]
    pub fn from_features(features: &BTreeMap<String, Value>) -> Self {
        Self { daemon_observability: features.get("daemonObservability") == Some(&Value::Bool(true)) }
    }

    #[must_use]
    pub fn degraded(&self) -> BTreeSet<DegradedReason> {
        let mut s = BTreeSet::new();
        if !self.daemon_observability {
            s.insert(DegradedReason::NoObservability);
        }
        s
    }
}
