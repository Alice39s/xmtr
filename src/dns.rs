use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use hickory_resolver::config::{
    CLOUDFLARE, ConnectionConfig, GOOGLE, NameServerConfig, QUAD9, ResolverConfig, ResolverOpts,
    ServerGroup,
};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::{Name, RData};
use hickory_resolver::{ResolverBuilder, TokioResolver};
use tokio::runtime::Runtime;
use tokio::task::JoinSet;
use ureq::http::Uri;

use crate::model::Route;

const LOOKUP_BUDGET: Duration = Duration::from_secs(5);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);
/// Hop addresses looked up at the same time, so one resolver is not flooded.
const LOOKUP_CONCURRENCY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Udp,
    Tcp,
    Tls,
    Https,
    Quic,
}

impl Transport {
    const fn default_port(self) -> u16 {
        match self {
            Self::Udp | Self::Tcp => 53,
            Self::Tls | Self::Quic => 853,
            Self::Https => 443,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    Preset(&'static str),
    Ip(IpAddr),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsSpec {
    /// The system resolver configuration; target hostnames go through the operating system.
    Auto,
    Server {
        transport: Transport,
        host: Host,
        port: Option<u16>,
        path: Option<String>,
    },
}

fn preset(name: &str) -> Option<&'static str> {
    ["cloudflare", "google", "quad9"]
        .into_iter()
        .find(|preset| preset.eq_ignore_ascii_case(name))
}

const fn preset_group(name: &str) -> ServerGroup<'static> {
    match name.as_bytes() {
        b"google" => GOOGLE,
        b"quad9" => QUAD9,
        _ => CLOUDFLARE,
    }
}

fn parse_host(value: &str) -> Host {
    let bare = value.trim_start_matches('[').trim_end_matches(']');
    if let Some(name) = preset(bare) {
        Host::Preset(name)
    } else if let Ok(ip) = bare.parse() {
        Host::Ip(ip)
    } else {
        Host::Name(bare.to_owned())
    }
}

impl FromStr for DnsSpec {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.eq_ignore_ascii_case("auto") {
            return Ok(Self::Auto);
        }
        if !value.contains("://") {
            if let Ok(socket) = value.parse::<SocketAddr>() {
                return Ok(Self::Server {
                    transport: Transport::Udp,
                    host: Host::Ip(socket.ip()),
                    port: Some(socket.port()),
                    path: None,
                });
            }
            return Ok(Self::Server {
                transport: Transport::Udp,
                host: parse_host(value),
                port: None,
                path: None,
            });
        }
        let uri: Uri = value
            .parse()
            .map_err(|error| format!("{value} is not a resolver URL: {error}"))?;
        let transport = match uri.scheme_str().unwrap_or_default() {
            "udp" => Transport::Udp,
            "tcp" => Transport::Tcp,
            "tls" | "dot" => Transport::Tls,
            "https" | "doh" => Transport::Https,
            "quic" | "doq" => Transport::Quic,
            other => {
                return Err(format!(
                    "unknown resolver scheme `{other}`; use udp, tcp, tls, https or quic"
                ));
            }
        };
        let host = uri.host().ok_or_else(|| format!("{value} has no host"))?;
        // hickory uses the server name both as the HTTP authority and for TLS, which rules out IPv6 literals.
        if transport == Transport::Https && host.starts_with('[') {
            return Err(
                "DNS over HTTPS needs a hostname or IPv4 address, such as https://cloudflare-dns.com/dns-query"
                    .to_owned(),
            );
        }
        let path = Some(uri.path())
            .filter(|path| transport == Transport::Https && !path.is_empty() && *path != "/")
            .map(str::to_owned);
        Ok(Self::Server {
            transport,
            host: parse_host(host),
            port: uri.port_u16(),
            path,
        })
    }
}

impl fmt::Display for DnsSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => formatter.write_str("auto"),
            Self::Server {
                transport, host, ..
            } => {
                let host = match host {
                    Host::Preset(name) => (*name).to_owned(),
                    Host::Ip(ip) => ip.to_string(),
                    Host::Name(name) => name.clone(),
                };
                write!(formatter, "{transport:?} {host}")
            }
        }
    }
}

fn connection(
    transport: Transport,
    server_name: &str,
    port: u16,
    path: Option<&str>,
) -> ConnectionConfig {
    let name: Arc<str> = Arc::from(server_name);
    let mut config = match transport {
        Transport::Udp => ConnectionConfig::udp(),
        Transport::Tcp => ConnectionConfig::tcp(),
        Transport::Tls => ConnectionConfig::tls(name),
        Transport::Https => ConnectionConfig::https(name, path.map(Arc::from)),
        Transport::Quic => ConnectionConfig::quic(name),
    };
    config.port = port;
    config
}

fn name_servers(
    transport: Transport,
    host: &Host,
    port: Option<u16>,
    path: Option<&str>,
) -> Result<Vec<NameServerConfig>> {
    let port = port.unwrap_or(transport.default_port());
    let (ips, server_name): (Vec<IpAddr>, String) = match host {
        Host::Preset(name) => {
            let group = preset_group(name);
            let path = path.or(Some(group.path));
            return Ok(group
                .ips
                .iter()
                .map(|ip| {
                    NameServerConfig::new(
                        *ip,
                        true,
                        vec![connection(transport, group.server_name, port, path)],
                    )
                })
                .collect());
        }
        Host::Ip(ip) => (vec![*ip], ip.to_string()),
        Host::Name(name) => (
            (name.as_str(), port)
                .to_socket_addrs()
                .with_context(|| format!("resolving the DNS server {name}"))?
                .map(|addr| addr.ip())
                .collect(),
            name.clone(),
        ),
    };
    Ok(ips
        .into_iter()
        .map(|ip| {
            NameServerConfig::new(
                ip,
                true,
                vec![connection(transport, &server_name, port, path)],
            )
        })
        .collect())
}

#[cfg(not(target_os = "macos"))]
fn system_resolver() -> Result<ResolverBuilder<TokioRuntimeProvider>> {
    TokioResolver::builder_tokio().context("reading the system resolver configuration")
}

/// Resolver built from `/etc/resolv.conf`, which macOS keeps in sync with its network settings.
#[cfg(target_os = "macos")]
fn system_resolver() -> Result<ResolverBuilder<TokioRuntimeProvider>> {
    let text = std::fs::read("/etc/resolv.conf").context("reading /etc/resolv.conf")?;
    let config = resolv_conf::Config::parse(&text).context("parsing /etc/resolv.conf")?;
    let servers: Vec<NameServerConfig> = config
        .nameservers
        .iter()
        .map(|server| NameServerConfig::udp_and_tcp(server.into()))
        .collect();
    if servers.is_empty() {
        bail!("/etc/resolv.conf lists no nameserver; pass --dns");
    }
    Ok(TokioResolver::builder_with_config(
        ResolverConfig::from_name_servers(servers),
        TokioRuntimeProvider::default(),
    ))
}

#[derive(Debug, Default, Clone)]
struct Names {
    hostname: Option<String>,
    asn: Option<u32>,
}

/// Resolver chosen with `--dns`, used for target hostnames, the upload host, reverse DNS and AS numbers.
pub struct Dns {
    runtime: Runtime,
    resolver: TokioResolver,
    system_targets: bool,
}

impl Dns {
    pub fn new(spec: &DnsSpec) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("starting the DNS runtime")?;
        let _guard = runtime.enter();
        let mut builder = match spec {
            // Android exposes no resolver configuration to native programs.
            DnsSpec::Auto if cfg!(target_os = "android") => TokioResolver::builder_with_config(
                ResolverConfig::udp_and_tcp(&CLOUDFLARE),
                TokioRuntimeProvider::default(),
            ),
            DnsSpec::Auto => system_resolver()?,
            DnsSpec::Server {
                transport,
                host,
                port,
                path,
            } => TokioResolver::builder_with_config(
                ResolverConfig::from_name_servers(name_servers(
                    *transport,
                    host,
                    *port,
                    path.as_deref(),
                )?),
                TokioRuntimeProvider::default(),
            ),
        };
        let options: &mut ResolverOpts = builder.options_mut();
        options.timeout = LOOKUP_TIMEOUT;
        options.attempts = 2;
        let resolver = builder
            .build()
            .with_context(|| format!("configuring the {spec} resolver"))?;
        Ok(Self {
            runtime,
            resolver,
            system_targets: *spec == DnsSpec::Auto,
        })
    }

    /// Addresses of `host`; with `--dns auto` the operating system resolver answers, so hosts files apply.
    pub fn resolve(&self, host: &str) -> Result<Vec<IpAddr>> {
        if let Ok(ip) = host.parse() {
            return Ok(vec![ip]);
        }
        let addrs: Vec<IpAddr> = if self.system_targets {
            (host, 0)
                .to_socket_addrs()
                .with_context(|| {
                    format!(
                        "resolving {host} with the system resolver (try --dns https://cloudflare)"
                    )
                })?
                .map(|addr| addr.ip())
                .collect()
        } else {
            self.runtime
                .block_on(self.resolver.lookup_ip(host))
                .with_context(|| format!("resolving {host}"))?
                .iter()
                .collect()
        };
        if addrs.is_empty() {
            bail!("{host} has no A or AAAA record");
        }
        Ok(addrs)
    }

    pub const fn uses_system(&self) -> bool {
        self.system_targets
    }

    /// Adds reverse DNS names and origin AS numbers to hop addresses, waiting at most a few seconds.
    /// Private and other non-public addresses are only looked up in the system resolver, and only
    /// for their names, so they are not sent to a public resolver. `progress` receives the answered and
    /// total address counts as lookups finish. Returns how many addresses got no answer in time.
    pub fn annotate(&self, routes: &mut [Route], progress: impl Fn(usize, usize)) -> usize {
        let mut addrs: Vec<IpAddr> = routes
            .iter()
            .flat_map(|route| route.hops.iter())
            .flat_map(|hop| hop.hosts.iter())
            .filter_map(|host| host.ip.parse().ok())
            .filter(|addr| self.system_targets || is_public(*addr))
            .collect();
        addrs.sort_unstable();
        addrs.dedup();
        let total = addrs.len();
        progress(0, total);

        let mut names: HashMap<IpAddr, Names> = HashMap::new();
        self.runtime.block_on(async {
            let mut pending = addrs.into_iter();
            let mut tasks = JoinSet::new();
            for addr in pending.by_ref().take(LOOKUP_CONCURRENCY) {
                tasks.spawn(lookup_names(self.resolver.clone(), addr));
            }
            let _ = tokio::time::timeout(LOOKUP_BUDGET, async {
                while let Some(joined) = tasks.join_next().await {
                    if let Ok((addr, found)) = joined {
                        names.insert(addr, found);
                        progress(names.len(), total);
                    }
                    if let Some(addr) = pending.next() {
                        tasks.spawn(lookup_names(self.resolver.clone(), addr));
                    }
                }
            })
            .await;
        });
        let answered = names.len();

        for host in routes
            .iter_mut()
            .flat_map(|route| route.hops.iter_mut())
            .flat_map(|hop| hop.hosts.iter_mut())
        {
            if let Some(found) = host
                .ip
                .parse()
                .ok()
                .and_then(|addr: IpAddr| names.get(&addr))
            {
                host.hostname.clone_from(&found.hostname);
                host.asn = found.asn;
            }
        }
        total - answered
    }
}

/// True for globally routable unicast addresses; AS lookups only make sense for these.
fn is_public(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                || a >= 240
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
                || (first == 0x2001 && ip.segments()[1] == 0x0db8))
        }
    }
}

/// Team Cymru's IP-to-ASN zone: reversed octets or nibbles under `origin(6).asn.cymru.com`.
fn cymru_name(addr: IpAddr) -> Result<Name> {
    let name = match addr {
        IpAddr::V4(ip) => {
            let [a, b, c, d] = ip.octets();
            format!("{d}.{c}.{b}.{a}.origin.asn.cymru.com.")
        }
        IpAddr::V6(ip) => {
            let nibbles: Vec<String> = ip
                .octets()
                .iter()
                .rev()
                .flat_map(|byte| [byte & 0x0f, byte >> 4])
                .map(|nibble| format!("{nibble:x}"))
                .collect();
            format!("{}.origin6.asn.cymru.com.", nibbles.join("."))
        }
    };
    Name::from_ascii(&name).map_err(|error| anyhow!("{name}: {error}"))
}

/// First AS number of a Cymru answer such as `13335 | 1.1.1.0/24 | AU | apnic | 2011-08-11`.
fn cymru_asn(text: &str) -> Option<u32> {
    text.split('|')
        .next()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

async fn lookup_names(resolver: TokioResolver, addr: IpAddr) -> (IpAddr, Names) {
    let ptr = resolver.reverse_lookup(Name::from(addr));
    let asn = async {
        if !is_public(addr) {
            return None;
        }
        match cymru_name(addr) {
            Ok(name) => resolver.txt_lookup(name).await.ok(),
            Err(_) => None,
        }
    };
    let (ptr, asn) = tokio::join!(ptr, asn);
    let hostname = ptr.ok().and_then(|lookup| {
        lookup
            .answers()
            .iter()
            .find_map(|record| match &record.data {
                RData::PTR(name) => Some(name.to_string().trim_end_matches('.').to_owned()),
                _ => None,
            })
    });
    let asn = asn.and_then(|lookup| {
        lookup
            .answers()
            .iter()
            .find_map(|record| match &record.data {
                RData::TXT(txt) => {
                    let text: Vec<u8> = txt
                        .txt_data
                        .iter()
                        .flat_map(|part| part.iter().copied())
                        .collect();
                    cymru_asn(&String::from_utf8_lossy(&text))
                }
                _ => None,
            })
    });
    (addr, Names { hostname, asn })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(transport: Transport, host: Host, port: Option<u16>, path: Option<&str>) -> DnsSpec {
        DnsSpec::Server {
            transport,
            host,
            port,
            path: path.map(str::to_owned),
        }
    }

    #[test]
    fn parses_resolver_specs() {
        assert_eq!("auto".parse(), Ok(DnsSpec::Auto));
        assert_eq!(
            "cloudflare".parse(),
            Ok(server(
                Transport::Udp,
                Host::Preset("cloudflare"),
                None,
                None
            ))
        );
        assert_eq!(
            "9.9.9.9".parse(),
            Ok(server(
                Transport::Udp,
                Host::Ip([9, 9, 9, 9].into()),
                None,
                None
            ))
        );
        assert_eq!(
            "https://dns.google/dns-query".parse(),
            Ok(server(
                Transport::Https,
                Host::Name("dns.google".into()),
                None,
                Some("/dns-query")
            ))
        );
        assert_eq!(
            "tls://[2606:4700:4700::1111]:853".parse(),
            Ok(server(
                Transport::Tls,
                Host::Ip("2606:4700:4700::1111".parse().unwrap()),
                Some(853),
                None
            ))
        );
        assert_eq!(
            "doq://dns.adguard-dns.com".parse(),
            Ok(server(
                Transport::Quic,
                Host::Name("dns.adguard-dns.com".into()),
                None,
                None
            ))
        );
        assert!("ftp://1.1.1.1".parse::<DnsSpec>().is_err());
        assert!(
            "https://[2606:4700:4700::1111]/dns-query"
                .parse::<DnsSpec>()
                .is_err()
        );
    }

    #[test]
    fn builds_cymru_names() {
        assert_eq!(
            cymru_name([1, 2, 3, 4].into()).unwrap().to_string(),
            "4.3.2.1.origin.asn.cymru.com."
        );
        assert!(
            cymru_name("2001:db8::1".parse().unwrap())
                .unwrap()
                .to_string()
                .starts_with(
                    "1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2.origin6"
                )
        );
        assert_eq!(cymru_asn("13335 209242 | 1.1.1.0/24 | AU"), Some(13335));
        assert_eq!(cymru_asn("NA | 10.0.0.0/8"), None);
    }

    #[test]
    fn parses_an_address_with_a_port() {
        assert_eq!(
            "192.0.2.53:5353".parse(),
            Ok(server(
                Transport::Udp,
                Host::Ip([192, 0, 2, 53].into()),
                Some(5353),
                None
            ))
        );
        assert_eq!(
            "[2001:db8::53]:5353".parse(),
            Ok(server(
                Transport::Udp,
                Host::Ip("2001:db8::53".parse().unwrap()),
                Some(5353),
                None
            ))
        );
    }

    #[test]
    fn keeps_private_addresses_out_of_public_lookups() {
        for private in [
            "10.0.0.1",
            "192.168.1.1",
            "172.16.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "127.0.0.1",
            "240.0.0.1",
            "fd00::1",
            "fe80::1",
            "::1",
            "2001:db8::1",
        ] {
            assert!(!is_public(private.parse().unwrap()), "{private}");
        }
        for public in ["1.1.1.1", "100.128.0.1", "2606:4700::1111"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
    }
}
