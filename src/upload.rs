use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ureq::Agent;
use ureq::config::Config;
use ureq::http::Uri;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{
    ConnectProxyConnector, Connector, NextTimeout, RustlsConnector, TcpConnector,
};

use crate::dns::Dns;
use crate::model::{Problem, Upload, UploadAccepted};

#[path = "upload/socks.rs"]
mod socks;

#[path = "upload/source.rs"]
pub mod source;

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[cfg(test)]
#[path = "upload/proxy_tests.rs"]
mod proxy_tests;

/// Hands ureq addresses that were resolved with the resolver chosen by `--dns`.
#[derive(Debug)]
struct PinnedResolver {
    gateway: Uri,
    addrs: Vec<SocketAddr>,
}

impl Resolver for PinnedResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        // CONNECT and SOCKS ask this same resolver for the proxy endpoint.
        // Only the gateway gets the --dns addresses; never reuse its port or
        // address for a different authority.
        if uri.authority() != self.gateway.authority() {
            return DefaultResolver::default().resolve(uri, config, timeout);
        }
        let mut addrs = self.empty();
        for addr in self.addrs.iter().take(16) {
            addrs.push(*addr);
        }
        if addrs.is_empty() {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(addrs)
    }
}

fn config(timeout: Duration) -> Config {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        // Addresses are pinned to the gateway host, so a redirect elsewhere would reach the wrong server.
        .max_redirects(0)
        .user_agent(USER_AGENT)
        .build()
}

/// Connections through the SOCKS or HTTP proxy from the environment, if any, then TLS.
fn connector() -> impl Connector {
    ().chain(socks::SocksConnector)
        .chain(ConnectProxyConnector::default())
        .chain(TcpConnector::default())
        .chain(RustlsConnector::default())
}

/// An agent that resolves with the system and connects like the upload does.
pub fn proxied_agent(timeout: Duration) -> Agent {
    Agent::with_parts(config(timeout), connector(), DefaultResolver::default())
}

/// The host of the proxy the environment configures.
pub fn proxy_host() -> Option<String> {
    ureq::Proxy::try_from_env().map(|proxy| proxy.host().to_owned())
}

fn agent(url: &str, dns: &Dns, timeout: Duration) -> Result<Agent> {
    let config = config(timeout);
    let connector = connector();
    let uri: Uri = url.parse().with_context(|| format!("parsing {url}"))?;
    let remote_proxy = config
        .proxy()
        .is_some_and(|proxy| !proxy.is_no_proxy(&uri) && !proxy.resolve_target());
    // Remote-resolving proxies must receive the hostname without a local
    // gateway lookup, including when --dns selects a custom resolver.
    if dns.uses_system() || remote_proxy {
        return Ok(Agent::with_parts(
            config,
            connector,
            DefaultResolver::default(),
        ));
    }
    Ok(Agent::with_parts(
        config,
        connector,
        PinnedResolver {
            addrs: gateway_addrs(&uri, dns)?,
            gateway: uri,
        },
    ))
}

fn gateway_addrs(uri: &Uri, dns: &Dns) -> Result<Vec<SocketAddr>> {
    let host = uri.host().context("gateway URL has no host")?;
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("http") {
            80
        } else {
            443
        });
    Ok(dns
        .resolve(host)?
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect())
}

/// Gateway text with control characters replaced, so a reply cannot send escape sequences to the terminal.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() {
                char::REPLACEMENT_CHARACTER
            } else {
                c
            }
        })
        .collect()
}

/// Sends the routes to the gateway and returns the stored trace ID, view key and IPMap link.
pub fn upload(api: &str, body: &Upload, dns: &Dns) -> Result<UploadAccepted> {
    upload_with_timeout(api, body, dns, UPLOAD_TIMEOUT)
}

fn upload_with_timeout(
    api: &str,
    body: &Upload,
    dns: &Dns,
    timeout: Duration,
) -> Result<UploadAccepted> {
    let url = format!("{}/api/v1/traces", api.trim_end_matches('/'));
    let mut response = agent(&url, dns, timeout)?
        .post(&url)
        .send_json(body)
        .with_context(|| format!("uploading to {url}"))?;
    let status = response.status();
    if status.is_success() {
        let accepted = response
            .body_mut()
            .read_json::<UploadAccepted>()
            .context("reading upload response")?;
        return Ok(UploadAccepted {
            id: printable(&accepted.id),
            url: printable(&accepted.url),
            ..accepted
        });
    }
    let problem = response.body_mut().read_json::<Problem>().ok();
    let reason = problem
        .and_then(|problem| problem.detail.or(problem.title))
        .map(|reason| printable(&reason))
        .unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("unknown error")
                .to_owned()
        });
    bail!("upload rejected with HTTP {}: {reason}", status.as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_resolver_preserves_gateway_literals_and_ports() {
        let dns = Dns::new(&"127.0.0.1:9".parse().unwrap()).unwrap();
        for (url, expected) in [
            ("http://[::1]:8080/api/v1/traces", "[::1]:8080"),
            ("http://127.0.0.1:8080/api/v1/traces", "127.0.0.1:8080"),
            ("http://[::1]/api/v1/traces", "[::1]:80"),
            ("http://127.0.0.1/api/v1/traces", "127.0.0.1:80"),
            ("https://[::1]/api/v1/traces", "[::1]:443"),
            ("https://127.0.0.1/api/v1/traces", "127.0.0.1:443"),
        ] {
            let uri = url.parse().unwrap();
            assert_eq!(
                gateway_addrs(&uri, &dns).unwrap(),
                [expected.parse().unwrap()]
            );
            agent(url, &dns, UPLOAD_TIMEOUT).unwrap();
        }
    }

    #[test]
    fn replaces_control_characters_in_gateway_text() {
        assert_eq!(
            printable("bad\u{1b}]0;title\u{7}\u{1b}[2J\ntext"),
            "bad\u{fffd}]0;title\u{fffd}\u{fffd}[2J\u{fffd}text"
        );
        assert_eq!(
            printable("https://xmsl.dev/ipmap?trace-id=1"),
            "https://xmsl.dev/ipmap?trace-id=1"
        );
    }
}
