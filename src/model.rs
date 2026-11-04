use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize, Serializer};

/// Leading bits of an uploaded source or destination address that are kept: the /24 network of
/// IPv4 and the /48 network of IPv6.
const IPV4_PREFIX: u32 = 24;
const IPV6_PREFIX: u32 = 48;

/// `addr` with every bit after its network prefix cleared, such as `203.0.113.0` for
/// `203.0.113.7` and `2001:db8:1::` for `2001:db8:1:2::7`.
pub fn mask(addr: IpAddr) -> IpAddr {
    match addr {
        IpAddr::V4(addr) => IpAddr::V4(Ipv4Addr::from_bits(
            addr.to_bits() & !(u32::MAX >> IPV4_PREFIX),
        )),
        IpAddr::V6(addr) => IpAddr::V6(Ipv6Addr::from_bits(
            addr.to_bits() & !(u128::MAX >> IPV6_PREFIX),
        )),
    }
}

/// A target or destination: an IP address is uploaded as its network, a host name as it is.
fn masked_host<S: Serializer>(host: &str, serializer: S) -> Result<S::Ok, S::Error> {
    match host.parse::<IpAddr>() {
        Ok(addr) => serializer.collect_str(&mask(addr)),
        Err(_) => serializer.serialize_str(host),
    }
}

fn masked_addr<S: Serializer>(addr: &Option<IpAddr>, serializer: S) -> Result<S::Ok, S::Error> {
    addr.map(mask).serialize(serializer)
}

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
    /// The host as given on the command line; an IP address is uploaded as its network.
    #[serde(serialize_with = "masked_host")]
    pub target: String,
    /// Uploaded as its network, which keeps the place and the AS of the end of the route.
    #[serde(serialize_with = "masked_host")]
    pub destination: String,
    pub protocol: &'static str,
    pub family: u8,
    /// Public address the probes left from, the start of the route on the map; uploaded as its
    /// network.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "masked_addr"
    )]
    pub source_addr: Option<IpAddr>,
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
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asn: Option<u32>,
    pub replies: usize,
    /// Lowest round trip of the replies from this host. The hop's RTT statistics mix every host
    /// that answered at this TTL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_ms: Option<f64>,
    /// MPLS label stack (RFC 4950) of the latest reply from this host that carried one, top entry
    /// first: the host forwarded the probe inside an MPLS tunnel.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mpls: Vec<MplsLabel>,
}

/// One entry of an MPLS label stack that a router quoted in its ICMP reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MplsLabel {
    pub label: u32,
    /// Traffic class, the former EXP bits.
    pub tc: u8,
    /// Bottom of stack.
    pub bottom: bool,
    pub ttl: u8,
}

impl MplsLabel {
    /// The entry the way `mtr` prints it, such as `[MPLS: Lbl 24001 TC 0 S 1 TTL 1]`.
    pub fn text(&self) -> String {
        format!(
            "[MPLS: Lbl {} TC {} S {} TTL {}]",
            self.label,
            self.tc,
            u8::from(self.bottom),
            self.ttl
        )
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn route(source_addr: Option<&str>) -> Route {
        Route {
            target: "one.one.one.one".to_owned(),
            destination: "1.1.1.1".to_owned(),
            protocol: "icmp",
            family: 4,
            source_addr: source_addr.map(|addr| addr.parse().unwrap()),
            started_at: String::new(),
            duration_ms: 5000,
            rounds: 5,
            reached: true,
            hops: Vec::new(),
        }
    }

    #[test]
    fn masks_addresses_to_their_network() {
        for (addr, network) in [
            ("198.51.100.7", "198.51.100.0"),
            ("198.51.100.0", "198.51.100.0"),
            ("255.255.255.255", "255.255.255.0"),
            ("2001:db8:1:2::7", "2001:db8:1::"),
            ("2001:db8:ffff:ffff:ffff:ffff:ffff:ffff", "2001:db8:ffff::"),
            ("::1", "::"),
        ] {
            assert_eq!(mask(addr.parse().unwrap()).to_string(), network, "{addr}");
        }
    }

    #[test]
    fn uploads_the_source_and_destination_networks() {
        for (addr, network) in [
            ("198.51.100.7", "198.51.100.0"),
            ("2001:db8:1:2::7", "2001:db8:1::"),
        ] {
            let mut route = route(Some(addr));
            route.target = addr.to_owned();
            route.destination = addr.to_owned();
            let value = serde_json::to_value(&route).unwrap();
            for field in ["sourceAddr", "destination", "target"] {
                assert_eq!(value[field], network, "{field}");
            }
            assert_eq!(route.destination, addr);
        }
        assert_eq!(
            serde_json::to_value(route(None)).unwrap()["target"],
            "one.one.one.one"
        );
    }

    #[test]
    fn serializes_host_round_trips_and_label_stacks_only_when_known() {
        let mut host = Host {
            ip: "192.0.2.1".to_owned(),
            hostname: None,
            asn: None,
            replies: 3,
            best_ms: None,
            mpls: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(&host).unwrap(),
            serde_json::json!({ "ip": "192.0.2.1", "replies": 3 })
        );
        host.best_ms = Some(12.5);
        host.mpls = vec![MplsLabel {
            label: 24001,
            tc: 0,
            bottom: true,
            ttl: 1,
        }];
        assert_eq!(
            serde_json::to_value(&host).unwrap(),
            serde_json::json!({
                "ip": "192.0.2.1",
                "replies": 3,
                "bestMs": 12.5,
                "mpls": [{ "label": 24001, "tc": 0, "bottom": true, "ttl": 1 }],
            })
        );
        assert_eq!(host.mpls[0].text(), "[MPLS: Lbl 24001 TC 0 S 1 TTL 1]");
    }

    #[test]
    fn serializes_the_source_address_only_when_known() {
        assert!(
            serde_json::to_value(route(Some("198.51.100.7")))
                .unwrap()
                .get("sourceAddr")
                .is_some()
        );
        assert!(
            serde_json::to_value(route(None))
                .unwrap()
                .get("sourceAddr")
                .is_none()
        );
    }
}
