use serde::{Deserialize, Serialize};

/// Body of `POST /api/v1/traces`; field names follow the gateway's xmtr upload schema.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Upload {
    pub schema: u8,
    pub client: Client,
    pub share_origin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    pub routes: Vec<Route>,
}

#[derive(Debug, Serialize)]
pub struct Client {
    pub name: &'static str,
    pub version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
}

impl Client {
    pub const fn current() -> Self {
        Self {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub target: String,
    pub destination: String,
    pub protocol: &'static str,
    pub family: u8,
    pub started_at: String,
    pub duration_ms: u64,
    pub rounds: usize,
    pub reached: bool,
    pub hops: Vec<Hop>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hop {
    pub ttl: u8,
    pub hosts: Vec<Host>,
    pub sent: usize,
    pub recv: usize,
    pub loss_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worst_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdev_ms: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples_ms: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Host {
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asn: Option<u32>,
    pub replies: usize,
}

/// Successful response of `POST /api/v1/traces`.
#[derive(Debug, Deserialize)]
pub struct UploadAccepted {
    /// Snowflake ID and view key, such as `103115083070058496-nBjPEnny`.
    pub id: String,
    pub pin: bool,
    pub url: String,
}

/// RFC 9457 problem body returned by the gateway on errors.
#[derive(Debug, Deserialize)]
pub struct Problem {
    pub title: Option<String>,
    pub detail: Option<String>,
}
