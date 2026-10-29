use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ureq::Agent;
use ureq::config::Config;
use ureq::http::Uri;
use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

use crate::dns::Dns;
use crate::model::{Problem, Upload, UploadAccepted};

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// Hands ureq addresses that were resolved with the resolver chosen by `--dns`.
#[derive(Debug)]
struct PinnedResolver(Vec<SocketAddr>);

impl Resolver for PinnedResolver {
    fn resolve(
        &self,
        _uri: &Uri,
        _config: &Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let mut addrs = self.empty();
        for addr in self.0.iter().take(16) {
            addrs.push(*addr);
        }
        if addrs.is_empty() {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(addrs)
    }
}

fn agent(url: &str, dns: &Dns) -> Result<Agent> {
    let config = Agent::config_builder()
        .timeout_global(Some(UPLOAD_TIMEOUT))
        .http_status_as_error(false)
        // Addresses are pinned to the gateway host, so a redirect elsewhere would reach the wrong server.
        .max_redirects(0)
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build();
    if dns.uses_system() {
        return Ok(config.into());
    }
    let uri: Uri = url.parse().with_context(|| format!("parsing {url}"))?;
    Ok(Agent::with_parts(
        config,
        DefaultConnector::new(),
        PinnedResolver(gateway_addrs(&uri, dns)?),
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
    let url = format!("{}/api/v1/traces", api.trim_end_matches('/'));
    let mut response = agent(&url, dns)?
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
            agent(url, &dns).unwrap();
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
