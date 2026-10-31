//! Public address the probes of each route leave from; IPMap starts the route there.
//!
//! A public local address is used as it is. Behind NAT, a request sent straight to the gateway from
//! the same local address returns the public address the gateway saw.
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use socket2::{Domain, Socket, Type};
use ureq::Agent;
use ureq::http::Uri;
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, RustlsConnector, Transport,
};

use super::{PinnedResolver, USER_AGENT, gateway_addrs};
use crate::dns::{Dns, is_public};
use crate::plan::Job;

/// Time the lookup may take; it runs while the routes are traced.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);

/// Reply of `GET /api/v1/traces/source-address`.
#[derive(Debug, Deserialize)]
struct SourceAddress {
    ip: IpAddr,
}

/// Public source address of each job, keyed by its destination. A job is left out when its local
/// address is not public and the gateway reports no public address of the same family in time.
pub fn lookup(jobs: &[Job], api: &str, dns: &Dns) -> HashMap<IpAddr, IpAddr> {
    lookup_with_timeout(jobs, api, dns, LOOKUP_TIMEOUT)
}

fn lookup_with_timeout(
    jobs: &[Job],
    api: &str,
    dns: &Dns,
    timeout: Duration,
) -> HashMap<IpAddr, IpAddr> {
    let deadline = Instant::now() + timeout;
    let mut locals: Vec<IpAddr> = jobs.iter().map(|job| job.local).collect();
    locals.sort_unstable();
    locals.dedup();
    let public: HashMap<IpAddr, IpAddr> = thread::scope(|scope| {
        let pending: Vec<_> = locals
            .into_iter()
            .map(|local| {
                let request = (!is_public(local))
                    .then(|| scope.spawn(move || request(api, dns, local, deadline)));
                (local, request)
            })
            .collect();
        pending
            .into_iter()
            .filter_map(|(local, request)| {
                let Some(request) = request else {
                    return Some((local, local));
                };
                // A route without a source address is still uploaded, so a failed request only
                // leaves the address out.
                let seen = request
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                    .ok()?;
                (seen.is_ipv4() == local.is_ipv4() && is_public(seen)).then_some((local, seen))
            })
            .collect()
    });
    jobs.iter()
        .filter_map(|job| Some((job.addr, *public.get(&job.local)?)))
        .collect()
}

/// Address the gateway saw on a request sent from `local` straight to the gateway, ignoring proxy
/// settings. `--dns` resolves the gateway as for the upload.
fn request(api: &str, dns: &Dns, local: IpAddr, deadline: Instant) -> Result<IpAddr> {
    let url = format!("{}/api/v1/traces/source-address", api.trim_end_matches('/'));
    let uri: Uri = url.parse().with_context(|| format!("parsing {url}"))?;
    let pinned = if dns.uses_system() {
        None
    } else {
        Some(PinnedResolver {
            addrs: gateway_addrs(&uri, dns)?,
            gateway: uri,
        })
    };
    let timeout = deadline.saturating_duration_since(Instant::now());
    if timeout.is_zero() {
        bail!("no time left to ask {url} from {local}");
    }
    let config = Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .user_agent(USER_AGENT)
        .build();
    let connector = ().chain(BoundConnector { local }).chain(RustlsConnector::default());
    let agent = match pinned {
        Some(resolver) => Agent::with_parts(config, connector, resolver),
        None => Agent::with_parts(config, connector, DefaultResolver::default()),
    };
    let reply: SourceAddress = agent
        .get(&url)
        .call()
        .with_context(|| format!("asking {url} from {local}"))?
        .body_mut()
        .read_json()
        .with_context(|| format!("reading the reply of {url}"))?;
    Ok(reply.ip)
}

/// Connects from a fixed local address to the gateway addresses of the same family.
#[derive(Debug)]
struct BoundConnector {
    local: IpAddr,
}

impl Connector for BoundConnector {
    type Out = BoundTransport;

    fn connect(
        &self,
        details: &ConnectionDetails,
        _: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let deadline = details
            .timeout
            .not_zero()
            .map(|after| Instant::now() + *after);
        let mut last_error = ureq::Error::HostNotFound;
        for remote in details
            .addrs
            .iter()
            .filter(|remote| remote.is_ipv4() == self.local.is_ipv4())
        {
            match connect(self.local, *remote, deadline) {
                Ok(stream) => {
                    return Ok(Some(BoundTransport {
                        stream,
                        buffers: LazyBuffers::new(
                            details.config.input_buffer_size(),
                            details.config.output_buffer_size(),
                        ),
                    }));
                }
                Err(error) => last_error = transport_error(error, details.timeout),
            }
        }
        Err(last_error)
    }
}

fn connect(local: IpAddr, remote: SocketAddr, deadline: Option<Instant>) -> io::Result<TcpStream> {
    let socket = Socket::new(
        Domain::for_address(remote),
        Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    socket.bind(&SocketAddr::new(local, 0).into())?;
    match deadline {
        Some(deadline) => socket.connect_timeout(
            &remote.into(),
            deadline.saturating_duration_since(Instant::now()),
        )?,
        None => socket.connect(&remote.into())?,
    }
    Ok(socket.into())
}

/// ureq expects `Error::Timeout` for an expired socket timeout, which some systems report as
/// `WouldBlock`.
fn transport_error(error: io::Error, timeout: NextTimeout) -> ureq::Error {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ureq::Error::Timeout(timeout.reason),
        _ => error.into(),
    }
}

#[derive(Debug)]
struct BoundTransport {
    stream: TcpStream,
    buffers: LazyBuffers,
}

impl Transport for BoundTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream
            .set_write_timeout(timeout.not_zero().map(|after| *after))?;
        self.stream
            .write_all(&self.buffers.output()[..amount])
            .map_err(|error| transport_error(error, timeout))
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream
            .set_read_timeout(timeout.not_zero().map(|after| *after))?;
        let amount = self
            .stream
            .read(self.buffers.input_append_buf())
            .map_err(|error| transport_error(error, timeout))?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    /// Open while the gateway has neither closed the connection nor sent bytes nobody asked for.
    fn is_open(&mut self) -> bool {
        self.stream.set_nonblocking(true).is_ok()
            && matches!(
                self.stream.peek(&mut [0]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock
            )
            && self.stream.set_nonblocking(false).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;
    use std::net::TcpListener;
    use std::process::Command;

    use super::*;
    use crate::upload::proxy_tests::{accept, closed, headers, run_child};

    const CHILD: &str = "XMTR_SOURCE_TEST_CHILD";
    const SHORT_TIMEOUT: Duration = Duration::from_millis(500);
    const WATCHDOG: Duration = Duration::from_secs(10);
    /// Local address the requests leave from. Linux and Windows answer on all of 127.0.0.0/8, so a
    /// gateway on 127.0.0.1 sees 127.0.0.2 only when the socket is bound to it; other systems
    /// configure 127.0.0.1 alone.
    const BOUND: &str = if cfg!(any(target_os = "linux", windows)) {
        "127.0.0.2"
    } else {
        "127.0.0.1"
    };

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn job(addr: &str, local: &str) -> Job {
        Job {
            target: addr.to_owned(),
            addr: ip(addr),
            protocol: trippy_core::Protocol::Icmp,
            fallback: None,
            local: ip(local),
        }
    }

    fn custom_dns() -> Dns {
        Dns::new(&"127.0.0.1:9".parse().unwrap()).unwrap()
    }

    fn gateway_url(listener: &TcpListener) -> String {
        format!("http://{}", listener.local_addr().unwrap())
    }

    fn assert_unused(listener: &TcpListener) {
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    /// Answers one source address request with `status` and the body `reply` builds from the
    /// client address, and returns that address.
    fn gateway(
        listener: &TcpListener,
        status: &str,
        reply: impl FnOnce(IpAddr) -> String,
    ) -> IpAddr {
        let mut stream = BufReader::new(accept(listener, false));
        let client = stream.get_ref().peer_addr().unwrap().ip();
        let request = headers(&mut stream);
        assert!(
            request.starts_with("GET /api/v1/traces/source-address HTTP/1.1\r\n"),
            "{request}"
        );
        let body = reply(client);
        write!(
            stream.get_mut(),
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        client
    }

    #[test]
    fn public_local_addresses_need_no_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let jobs = [
            job("1.1.1.1", "8.8.4.4"),
            job("2606:4700::1111", "2001:4860:4860::8844"),
        ];
        assert_eq!(
            lookup_with_timeout(&jobs, &gateway_url(&listener), &custom_dns(), SHORT_TIMEOUT),
            HashMap::from([
                (ip("1.1.1.1"), ip("8.8.4.4")),
                (ip("2606:4700::1111"), ip("2001:4860:4860::8844")),
            ])
        );
        assert_unused(&listener);
    }

    #[test]
    fn private_local_address_takes_the_address_the_gateway_saw() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = gateway_url(&listener);
        let server = thread::spawn(move || {
            let client = gateway(&listener, "200 OK", |_| r#"{"ip":"1.1.1.1"}"#.to_owned());
            (listener, client)
        });
        // Both routes leave from the same local address, which needs one request.
        let jobs = [job("9.9.9.9", BOUND), job("149.112.112.112", BOUND)];
        let sources = lookup_with_timeout(&jobs, &api, &custom_dns(), LOOKUP_TIMEOUT);
        let (listener, client) = server.join().unwrap();
        assert_eq!(client, ip(BOUND));
        assert_eq!(
            sources,
            HashMap::from([
                (ip("9.9.9.9"), ip("1.1.1.1")),
                (ip("149.112.112.112"), ip("1.1.1.1")),
            ])
        );
        assert_unused(&listener);
    }

    #[test]
    fn unusable_answers_leave_the_route_without_a_source() {
        for (status, body) in [
            ("200 OK", r#"{"ip":"10.0.0.1"}"#),
            ("200 OK", r#"{"ip":"100.64.0.1"}"#),
            ("200 OK", r#"{"ip":"2606:4700::1111"}"#),
            ("200 OK", r#"{"ip":"1.1.1"}"#),
            ("200 OK", "<html></html>"),
            ("429 Too Many Requests", r#"{"title":"Too Many Requests"}"#),
            ("500 Internal Server Error", r#"{"ip":"1.1.1.1"}"#),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let api = gateway_url(&listener);
            let server = thread::spawn(move || gateway(&listener, status, |_| body.to_owned()));
            let sources = lookup_with_timeout(
                &[job("1.1.1.1", "127.0.0.1")],
                &api,
                &custom_dns(),
                LOOKUP_TIMEOUT,
            );
            server.join().unwrap();
            assert!(sources.is_empty(), "{status} {body}: {sources:?}");
        }
    }

    #[test]
    fn silent_or_missing_gateway_ends_within_the_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = gateway_url(&listener);
        let server = thread::spawn(move || {
            let mut stream = BufReader::new(accept(&listener, false));
            headers(&mut stream);
            closed(&mut stream);
        });
        let started = Instant::now();
        let sources = lookup_with_timeout(
            &[job("1.1.1.1", "127.0.0.1")],
            &api,
            &custom_dns(),
            SHORT_TIMEOUT,
        );
        let elapsed = started.elapsed();
        server.join().unwrap();
        assert!(sources.is_empty());
        assert!(
            elapsed < SHORT_TIMEOUT + Duration::from_secs(1),
            "{elapsed:?}"
        );

        let missing = gateway_url(&TcpListener::bind("127.0.0.1:0").unwrap());
        let started = Instant::now();
        assert!(
            lookup_with_timeout(
                &[job("1.1.1.1", "127.0.0.1")],
                &missing,
                &custom_dns(),
                SHORT_TIMEOUT
            )
            .is_empty()
        );
        assert!(started.elapsed() < SHORT_TIMEOUT + Duration::from_secs(1));
    }

    #[test]
    fn request_child() {
        let Ok(api) = std::env::var(CHILD) else {
            return;
        };
        let dns = match std::env::var("XMTR_SOURCE_TEST_DNS").unwrap().as_str() {
            "auto" => Dns::system_for_tests(),
            spec => Dns::new(&spec.parse().unwrap()).unwrap(),
        };
        let seen = request(&api, &dns, ip(BOUND), Instant::now() + LOOKUP_TIMEOUT).unwrap();
        assert_eq!(seen, ip(BOUND));
    }

    // The proxy variables are set in a child process, so tests running in parallel keep their
    // own environment.
    #[test]
    fn request_leaves_from_the_bound_address_without_a_proxy() {
        let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        for dns in ["auto", "127.0.0.1:9"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "upload::source::tests::request_child",
                    "--nocapture",
                ])
                .env(CHILD, gateway_url(&listener))
                .env("XMTR_SOURCE_TEST_DNS", dns)
                .env_remove("NO_PROXY")
                .env_remove("no_proxy");
            for (name, scheme) in [
                ("ALL_PROXY", "socks5h"),
                ("all_proxy", "socks5h"),
                ("HTTPS_PROXY", "http"),
                ("https_proxy", "http"),
                ("HTTP_PROXY", "http"),
                ("http_proxy", "http"),
            ] {
                command.env(name, format!("{scheme}://{proxy_addr}"));
            }
            let server = thread::spawn(move || {
                gateway(&listener, "200 OK", |client| {
                    format!(r#"{{"ip":"{client}"}}"#)
                })
            });
            let output = run_child(command, WATCHDOG);
            assert!(
                output.status.success(),
                "{dns}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(server.join().unwrap(), ip(BOUND));
        }
        assert_unused(&proxy);
    }
}
