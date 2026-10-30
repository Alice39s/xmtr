//! Upload-only workaround for ureq 3.4.2's scoped SOCKS worker, which is joined
//! even after its timeout fires. Use ureq's TCP/TLS transports, but negotiate on
//! the calling thread with one deadline shared by every handshake read/write.
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, NextTimeout, TcpConnector, Transport, TransportAdapter,
};
use ureq::{Error, Proxy, ProxyProtocol};

#[derive(Debug)]
pub(super) struct SocksConnector;

impl Connector for SocksConnector {
    type Out = Box<dyn Transport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, Error> {
        if chained.is_some() {
            return Ok(chained.map(|v| v.boxed()));
        }
        let Some(proxy) = details.config.proxy().filter(|p| {
            matches!(
                p.protocol(),
                ProxyProtocol::Socks4
                    | ProxyProtocol::Socks4A
                    | ProxyProtocol::Socks5
                    | ProxyProtocol::Socks5h
            ) && !p.is_no_proxy(details.uri)
        }) else {
            return Ok(None);
        };
        let deadline = (!details.timeout.after.is_not_happening())
            .then(|| Instant::now() + *details.timeout.after);
        let addrs = details
            .resolver
            .resolve(proxy.uri(), details.config, details.timeout)?;
        let mut proxy_details = ConnectionDetails {
            uri: proxy.uri(),
            addrs,
            config: details.config,
            request_level: details.request_level,
            resolver: details.resolver,
            now: details.now,
            timeout: remaining(details.timeout, deadline, details.timeout.reason)?,
            current_time: details.current_time.clone(),
            run_connector: details.run_connector.clone(),
        };
        let target_port =
            details
                .uri
                .port_u16()
                .unwrap_or(if details.needs_tls() { 443 } else { 80 });
        let targets: Vec<Target<'_>> = if proxy.resolve_target() {
            details.addrs.iter().copied().map(Target::Ip).collect()
        } else {
            let host = details.uri.host().ok_or(Error::HostNotFound)?;
            let host = host.trim_start_matches('[').trim_end_matches(']');
            vec![match host.parse() {
                Ok(ip) => Target::Ip(SocketAddr::new(ip, target_port)),
                Err(_) => Target::Domain(host, target_port),
            }]
        };
        let mut last_error = Error::HostNotFound;
        for target in targets {
            proxy_details.timeout = remaining(details.timeout, deadline, details.timeout.reason)?;
            let transport = TcpConnector::default()
                .connect(&proxy_details, None::<()>)?
                .ok_or(Error::ConnectionFailed)?;
            // Retain the deadline after negotiation: TLS and HTTP must consume
            // the remainder of the same global budget, including read-ahead.
            let mut io = TransportAdapter::new(DeadlineTransport {
                inner: transport,
                deadline,
                reason: details.timeout.reason,
            });
            io.set_timeout(details.timeout);
            match handshake(&mut io, proxy, target) {
                Ok(()) => return Ok(Some(io.into_inner().boxed())),
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    last_error = error.into();
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(last_error)
    }
}

fn remaining(
    mut timeout: NextTimeout,
    deadline: Option<Instant>,
    reason: ureq::Timeout,
) -> Result<NextTimeout, Error> {
    if let Some(deadline) = deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Error::Timeout(reason));
        }
        if *timeout.after >= left {
            timeout.after = left.into();
            timeout.reason = reason;
        }
    }
    Ok(timeout)
}

#[derive(Debug)]
struct DeadlineTransport<T> {
    inner: T,
    deadline: Option<Instant>,
    reason: ureq::Timeout,
}

impl<T: Transport> Transport for DeadlineTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), Error> {
        self.inner
            .transmit_output(amount, remaining(timeout, self.deadline, self.reason)?)
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, Error> {
        let timeout = remaining(timeout, self.deadline, self.reason)?;
        let deadline = (!timeout.after.is_not_happening()).then(|| Instant::now() + *timeout.after);
        loop {
            let mut slice = remaining(timeout, deadline, timeout.reason)?;
            // Long SO_RCVTIMEO waits can be rounded up by seconds on Linux.
            // Poll in bounded slices without resetting the logical deadline.
            if *slice.after > std::time::Duration::from_secs(1) {
                slice.after = std::time::Duration::from_secs(1).into();
            }
            match self.inner.await_input(slice) {
                Err(Error::Timeout(_)) if deadline.is_some_and(|end| Instant::now() < end) => {}
                result => return result,
            }
        }
    }
    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }
}

#[derive(Clone, Copy)]
enum Target<'a> {
    Ip(SocketAddr),
    Domain(&'a str, u16),
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn handshake(io: &mut (impl Read + Write), proxy: &Proxy, target: Target<'_>) -> io::Result<()> {
    if matches!(
        proxy.protocol(),
        ProxyProtocol::Socks4 | ProxyProtocol::Socks4A
    ) {
        let (ip, port, domain) = match target {
            Target::Ip(SocketAddr::V4(addr)) => (addr.ip().octets(), addr.port(), None),
            Target::Domain(host, port) => ([0, 0, 0, 1], port, Some(host)),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "SOCKS4 does not support IPv6",
                ));
            }
        };
        io.write_all(&[4, 1])?;
        io.write_all(&port.to_be_bytes())?;
        io.write_all(&ip)?;
        io.write_all(&[0])?; // Same empty SOCKS4 user ID as ureq.
        if let Some(host) = domain {
            io.write_all(host.as_bytes())?;
            io.write_all(&[0])?;
        }
        let mut reply = [0; 8];
        io.read_exact(&mut reply)?;
        if reply[0] != 0 {
            return Err(invalid("invalid SOCKS4 version"));
        }
        if reply[1] != 90 {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "SOCKS4 request rejected",
            ));
        }
        return Ok(());
    }
    let credentials = proxy
        .username()
        .map(|user| (user.as_bytes(), proxy.password().unwrap_or("").as_bytes()));
    if let Some((user, password)) = credentials {
        if user.is_empty() || user.len() > 255 || password.len() > 255 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid SOCKS5 credentials length",
            ));
        }
        io.write_all(&[5, 2, 2, 0])?;
    } else {
        io.write_all(&[5, 1, 0])?;
    }
    let mut reply = [0; 2];
    io.read_exact(&mut reply)?;
    if reply[0] != 5 {
        return Err(invalid("invalid SOCKS5 version"));
    }
    match (reply[1], credentials) {
        (0, _) => {}
        (2, Some((user, password))) => {
            io.write_all(&[1, user.len() as u8])?;
            io.write_all(user)?;
            io.write_all(&[password.len() as u8])?;
            io.write_all(password)?;
            io.read_exact(&mut reply)?;
            if reply != [1, 0] {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "SOCKS5 authentication failed",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "SOCKS5 proxy selected an unoffered authentication method",
            ));
        }
    }
    io.write_all(&[5, 1, 0])?;
    let port = match target {
        Target::Ip(addr) => {
            match addr.ip() {
                IpAddr::V4(ip) => {
                    io.write_all(&[1])?;
                    io.write_all(&ip.octets())?;
                }
                IpAddr::V6(ip) => {
                    io.write_all(&[4])?;
                    io.write_all(&ip.octets())?;
                }
            }
            addr.port()
        }
        Target::Domain(host, port) => {
            let len = u8::try_from(host.len())
                .ok()
                .filter(|len| *len > 0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid SOCKS5 hostname length",
                    )
                })?;
            io.write_all(&[3, len])?;
            io.write_all(host.as_bytes())?;
            port
        }
    };
    io.write_all(&port.to_be_bytes())?;
    let mut reply = [0; 4];
    io.read_exact(&mut reply)?;
    if reply[0] != 5 || reply[2] != 0 {
        return Err(invalid("invalid SOCKS5 reply"));
    }
    if reply[1] != 0 {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "SOCKS5 connect rejected",
        ));
    }
    let len = match reply[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut len = [0];
            io.read_exact(&mut len)?;
            usize::from(len[0])
        }
        _ => return Err(invalid("invalid SOCKS5 address type")),
    };
    // Read exactly the bound address and port. Any application bytes read ahead
    // stay in the underlying ureq transport for HTTP or TLS.
    let mut bound = [0; 257];
    io.read_exact(&mut bound[..len + 2])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct Script {
        reply: Cursor<Vec<u8>>,
        sent: Vec<u8>,
    }
    impl Read for Script {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            // Exercise fragmented replies, including authentication and IPv6.
            let len = out.len().min(1);
            self.reply.read(&mut out[..len])
        }
    }
    impl Write for Script {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            self.sent.extend_from_slice(&input[..1]);
            Ok(1)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn fragmented_replies_preserve_application_bytes_and_reject_truncation() {
        for (proxy, reply) in [
            ("socks4a://proxy", vec![0, 90, 0, 0, 0, 0, 0, 0]),
            (
                "socks5h://proxy",
                vec![5, 0, 5, 0, 0, 3, 4, b'h', b'o', b's', b't', 0, 0],
            ),
            (
                "socks5h://user:pass@proxy",
                vec![
                    5, 2, 1, 0, 5, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ],
            ),
        ] {
            let proxy = Proxy::new(proxy).unwrap();
            for len in 0..reply.len() {
                let mut script = Script {
                    reply: Cursor::new(reply[..len].to_vec()),
                    sent: vec![],
                };
                assert_eq!(
                    handshake(&mut script, &proxy, Target::Domain("gateway.invalid", 8080))
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::UnexpectedEof
                );
            }
            let mut script = Script {
                reply: Cursor::new([reply, b"HTTP/1.1".to_vec()].concat()),
                sent: vec![],
            };
            handshake(&mut script, &proxy, Target::Domain("gateway.invalid", 8080)).unwrap();
            let mut remaining = String::new();
            script.read_to_string(&mut remaining).unwrap();
            assert_eq!(remaining, "HTTP/1.1");
        }
    }

    #[test]
    fn rejects_malformed_versions_methods_and_addresses() {
        for reply in [
            vec![4, 0],
            vec![5, 2],
            vec![5, 255],
            vec![5, 0, 4, 0, 0, 1],
            vec![5, 0, 5, 0, 1, 1],
            vec![5, 0, 5, 0, 0, 2],
        ] {
            let mut script = Script {
                reply: Cursor::new(reply),
                sent: vec![],
            };
            assert_eq!(
                handshake(
                    &mut script,
                    &Proxy::new("socks5h://proxy").unwrap(),
                    Target::Domain("gateway.invalid", 8080)
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
}
