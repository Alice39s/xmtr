use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{UPLOAD_TIMEOUT, upload, upload_with_timeout};
use crate::dns::Dns;
use crate::model::{Client, Upload};

const CHILD: &str = "XMTR_PROXY_TEST_CHILD";
const TIMEOUT: Duration = Duration::from_secs(5);
const SHORT_TIMEOUT: Duration = Duration::from_millis(250);
const REMOTE: &str = "http://gateway.invalid:8080";
const LOCAL: &str = "http://127.0.0.2:8080";

// Every upload has its own environment. Never mutate the test runner's proxy
// variables: these tests run in parallel, including on Windows.
#[test]
fn upload_child() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let spec = std::env::var("XMTR_PROXY_TEST_DNS").unwrap();
    let dns = if spec == "auto" {
        Dns::system_for_tests()
    } else {
        Dns::new(&spec.parse().unwrap()).unwrap()
    };
    let body = Upload {
        schema: 1,
        client: Client::current(),
        share_origin: false,
        pin: None,
        routes: vec![],
    };
    let api = std::env::var("XMTR_PROXY_TEST_API").unwrap();
    let expectation = std::env::var("XMTR_PROXY_TEST_EXPECT").unwrap();
    let start = Instant::now();
    let result = if expectation == "short-timeout" {
        upload_with_timeout(&api, &body, &dns, SHORT_TIMEOUT)
    } else {
        upload(&api, &body, &dns)
    };
    let elapsed = start.elapsed();
    if expectation.ends_with("timeout") {
        let error = result.unwrap_err();
        assert!(
            error.chain().any(|cause| matches!(
                cause.downcast_ref::<ureq::Error>(),
                Some(ureq::Error::Timeout(ureq::Timeout::Global))
            )),
            "{error:#}"
        );
        let (lower, upper) = if expectation == "short-timeout" {
            (Duration::from_millis(200), Duration::from_secs(1))
        } else {
            (
                UPLOAD_TIMEOUT - Duration::from_millis(500),
                UPLOAD_TIMEOUT + Duration::from_secs(2),
            )
        };
        assert!(
            elapsed >= lower && elapsed < upper,
            "request budget violated: {elapsed:?}"
        );
        println!("timeout after {elapsed:?}");
    } else if expectation.ends_with("-rejected") {
        let error = result.unwrap_err();
        assert!(
            !error.chain().any(|cause| matches!(
                cause.downcast_ref::<ureq::Error>(),
                Some(ureq::Error::Timeout(_))
            )),
            "unexpected timeout: {error:#}"
        );
        let kind = if expectation == "auth-rejected" {
            std::io::ErrorKind::PermissionDenied
        } else {
            std::io::ErrorKind::ConnectionRefused
        };
        assert!(
            error
                .chain()
                .any(|cause| matches!(cause.downcast_ref::<ureq::Error>(),
            Some(ureq::Error::Io(error)) if error.kind() == kind)),
            "{error:#}"
        );
        assert!(elapsed < Duration::from_secs(1));
    } else {
        let accepted = result.unwrap();
        assert_eq!(accepted.id, "proxy-test");
        assert_eq!(accepted.url, "https://example.invalid/proxy-test");
        assert!(!accepted.pin);
    }
}

fn child(api: &str, dns: &str, expectation: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "upload::proxy_tests::upload_child",
        "--nocapture",
    ]);
    command
        .env(CHILD, "1")
        .env("XMTR_PROXY_TEST_API", api)
        .env("XMTR_PROXY_TEST_DNS", dns)
        .env("XMTR_PROXY_TEST_EXPECT", expectation);
    for name in [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "NO_PROXY",
        "no_proxy",
    ] {
        command.env_remove(name);
    }
    command
}

// A stuck child is a failure, even if killing it would unblock the proxy. The
// server waits for client EOF instead of releasing the connection on a timer.
fn run_child(mut command: Command, limit: Duration) -> Output {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + limit;
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "upload exceeded watchdog {limit:?}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn headers(stream: &mut BufReader<TcpStream>) -> String {
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        assert_ne!(stream.read_line(&mut line).unwrap(), 0);
        if line == "\r\n" {
            return headers;
        }
        headers.push_str(&line);
        assert!(headers.len() < 16_384);
    }
}

fn accept(listener: TcpListener, production: bool) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "upload never connected to proxy");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accepting proxy connection: {error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(if production {
            UPLOAD_TIMEOUT + TIMEOUT
        } else {
            TIMEOUT
        }))
        .unwrap();
    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
    stream.set_nodelay(true).unwrap();
    stream
}

fn closed(stream: &mut impl Read) {
    // A timed-out client can close with FIN or RST, depending on the OS and
    // unread reply bytes. Both prove socket disposal. A read timeout or any
    // application byte is still a failure; the child separately asserts the
    // exact global-timeout error and elapsed request budget.
    match stream.read(&mut [0]) {
        Ok(0) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("client did not close the timed-out socket: {result:?}"),
    }
}

fn target(stream: &mut TcpStream, api: &str, version: u8) {
    let uri: ureq::http::Uri = api.parse().unwrap();
    let host = uri
        .host()
        .unwrap()
        .trim_start_matches('[')
        .trim_end_matches(']');
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("https") {
            443
        } else {
            80
        });
    if version == 4 {
        let mut request = [0; 7];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(request[0], 1);
        assert_eq!(&request[1..3], &port.to_be_bytes());
        if let Ok(IpAddr::V4(ip)) = host.parse() {
            assert_eq!(&request[3..7], &ip.octets());
        }
        // Empty SOCKS4 user ID, matching ureq's existing behavior.
        let mut nul = [0];
        stream.read_exact(&mut nul).unwrap();
        assert_eq!(nul, [0]);
        if host.parse::<IpAddr>().is_err() {
            assert_eq!(&request[3..7], &[0, 0, 0, 1]);
            let mut domain = vec![0; host.len() + 1];
            stream.read_exact(&mut domain).unwrap();
            assert_eq!(&domain[..host.len()], host.as_bytes());
            assert_eq!(domain[host.len()], 0);
        }
        return;
    }
    let mut request = [0; 4];
    stream.read_exact(&mut request).unwrap();
    assert_eq!(&request[..3], &[5, 1, 0]);
    match host.parse::<IpAddr>() {
        Ok(ip) => {
            let bytes = match ip {
                IpAddr::V4(ip) => ip.octets().to_vec(),
                IpAddr::V6(ip) => ip.octets().to_vec(),
            };
            assert_eq!(request[3], if ip.is_ipv4() { 1 } else { 4 });
            let mut received = vec![0; bytes.len()];
            stream.read_exact(&mut received).unwrap();
            assert_eq!(received, bytes);
        }
        Err(_) => {
            assert_eq!(request[3], 3, "hostname must be resolved by the proxy");
            let mut len = [0];
            stream.read_exact(&mut len).unwrap();
            let mut received = vec![0; usize::from(len[0])];
            stream.read_exact(&mut received).unwrap();
            assert_eq!(received, host.as_bytes());
        }
    }
    let mut received = [0; 2];
    stream.read_exact(&mut received).unwrap();
    assert_eq!(received, port.to_be_bytes());
}

fn serve(listener: TcpListener, protocol: &str, stage: &str, api: &str) {
    let mut stream = accept(listener, stage == "production-greeting");
    if protocol.starts_with("socks4") {
        let mut version = [0];
        stream.read_exact(&mut version).unwrap();
        assert_eq!(version, [4]);
        target(&mut stream, api, 4);
        if stage == "connect" {
            closed(&mut stream);
            return;
        }
        stream.write_all(&[0, 90, 0, 0, 0, 0, 0, 0]).unwrap();
    } else if protocol.starts_with("socks") {
        let auth = stage == "auth" || stage == "auth-ok" || stage == "auth-rejected";
        let mut greeting = vec![0; if auth { 4 } else { 3 }];
        stream.read_exact(&mut greeting).unwrap();
        assert_eq!(
            greeting,
            if auth {
                vec![5, 2, 2, 0]
            } else {
                vec![5, 1, 0]
            }
        );
        if stage.ends_with("greeting") {
            closed(&mut stream);
            return;
        }
        stream
            .write_all(if auth { &[5, 2] } else { &[5, 0] })
            .unwrap();
        if auth {
            let mut credentials = [0; 11];
            stream.read_exact(&mut credentials).unwrap();
            assert_eq!(&credentials, b"\x01\x04user\x04pass");
            if stage == "auth" {
                closed(&mut stream);
                return;
            }
            stream
                .write_all(if stage == "auth-rejected" {
                    &[1, 1]
                } else {
                    &[1, 0]
                })
                .unwrap();
            if stage == "auth-rejected" {
                return;
            }
        }
        target(&mut stream, api, 5);
        if stage == "connect" {
            closed(&mut stream);
            return;
        }
        if stage == "connect-rejected" {
            stream.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
            return;
        }
        if stage == "partial-reply" {
            stream.write_all(&[5, 0, 0, 3, 4, b'h']).unwrap();
            closed(&mut stream);
            return;
        }
        if stage == "drip-reply" {
            // Each byte arrives within 250ms, but the whole reply exceeds the
            // shared budget. A fresh timeout on each read would fail this test.
            let start = Instant::now();
            for byte in [5, 0, 0, 1, 127, 0, 0, 1, 0, 0] {
                if start.elapsed() >= Duration::from_millis(300) {
                    break;
                }
                stream.write_all(&[byte]).unwrap();
                thread::sleep(Duration::from_millis(60));
            }
            closed(&mut stream);
            return;
        }
        if stage == "delayed-headers" {
            thread::sleep(Duration::from_millis(150));
        }
        stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
    }
    let mut stream = BufReader::new(stream);
    if protocol == "http" {
        let uri: ureq::http::Uri = api.parse().unwrap();
        let request = headers(&mut stream);
        assert!(request.starts_with(&format!(
            "CONNECT {} HTTP/1.1\r\n",
            uri.authority().unwrap()
        )));
        if stage == "connect" {
            closed(&mut stream);
            return;
        }
        stream
            .get_mut()
            .write_all(b"HTTP/1.1 200 OK\r\n\r\n")
            .unwrap();
    }
    if stage == "tls" {
        let mut record = [0; 5];
        stream.read_exact(&mut record).unwrap();
        assert_eq!(&record[..2], &[22, 3]); // TLS handshake, not plaintext HTTP.
        let mut hello = vec![0; u16::from_be_bytes([record[3], record[4]]) as usize];
        stream.read_exact(&mut hello).unwrap();
        assert_eq!(hello[0], 1);
        closed(&mut stream);
        return;
    }
    let request = headers(&mut stream);
    assert!(request.starts_with("POST /api/v1/traces HTTP/1.1\r\n"));
    let length: usize = request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .expect("upload has a content length");
    assert!(length < 16_384);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["routes"], serde_json::json!([]));
    if stage == "headers" || stage == "delayed-headers" {
        closed(&mut stream);
        return;
    }
    let response = r#"{"id":"proxy-test","pin":false,"url":"https://example.invalid/proxy-test"}"#;
    write!(stream.get_mut(), "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len()).unwrap();
    if stage == "body" {
        closed(&mut stream);
        return;
    }
    stream.get_mut().write_all(response.as_bytes()).unwrap();
}

fn check(
    protocol: &str,
    variable: &str,
    stage: &str,
    api: &str,
    dns: &str,
    extras: &[(&str, &str)],
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let expectation = match stage {
        "ok" | "auth-ok" => "ok",
        "auth-rejected" => "auth-rejected",
        "connect-rejected" => "connect-rejected",
        "production-greeting" => "production-timeout",
        _ => "short-timeout",
    };
    let mut command = child(api, dns, expectation);
    let credentials = if stage.starts_with("auth") {
        "user:pass@"
    } else {
        ""
    };
    // The working proxy is applied last, so same-name aliases on Windows never
    // accidentally overwrite it with a broken lower-priority configuration.
    for (name, value) in extras {
        command.env(name, value);
    }
    command.env(variable, format!("{protocol}://{credentials}{address}"));
    let case = format!("{protocol} {stage}");
    let (protocol, stage, api) = (protocol.to_owned(), stage.to_owned(), api.to_owned());
    let production = stage == "production-greeting";
    let server = thread::spawn(move || serve(listener, &protocol, &stage, &api));
    let limit = if production {
        UPLOAD_TIMEOUT + Duration::from_secs(3)
    } else {
        TIMEOUT
    };
    let output = run_child(command, limit);
    let server_result = server.join();
    assert!(
        output.status.success(),
        "upload subprocess failed ({case}):\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    server_result.unwrap_or_else(|_| panic!("proxy fixture failed: {case}"));
    if expectation.ends_with("timeout") {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

#[test]
fn all_proxy_socks5h_takes_precedence() {
    check(
        "socks5h",
        "ALL_PROXY",
        "ok",
        REMOTE,
        "auto",
        &[
            ("HTTPS_PROXY", "http://127.0.0.1:0"),
            ("HTTP_PROXY", "http://127.0.0.1:0"),
        ],
    );
}

#[test]
fn lowercase_all_proxy_supports_socks5h() {
    check(
        "socks5h",
        "all_proxy",
        "ok",
        REMOTE,
        "auto",
        &[("HTTPS_PROXY", "http://127.0.0.1:0")],
    );
}

// Distinct aliases only exist on case-sensitive platforms. Windows has both
// upper/lower spelling tests above, and an explicit alias replacement test below.
#[cfg(not(windows))]
#[test]
fn uppercase_all_proxy_wins_over_distinct_lowercase_value() {
    check(
        "socks5h",
        "ALL_PROXY",
        "ok",
        REMOTE,
        "auto",
        &[("all_proxy", "socks5h://127.0.0.1:0")],
    );
}

#[cfg(windows)]
#[test]
fn windows_proxy_alias_replaces_same_environment_variable() {
    check(
        "socks5h",
        "all_proxy",
        "ok",
        REMOTE,
        "auto",
        &[("ALL_PROXY", "socks5h://127.0.0.1:0")],
    );
}

#[test]
fn unset_all_proxy_falls_back_to_https_proxy() {
    check(
        "http",
        "HTTPS_PROXY",
        "ok",
        REMOTE,
        "auto",
        &[("HTTP_PROXY", "http://127.0.0.1:0")],
    );
}

#[test]
fn empty_all_proxy_falls_back_to_https_proxy() {
    check(
        "http",
        "HTTPS_PROXY",
        "ok",
        REMOTE,
        "auto",
        &[("ALL_PROXY", ""), ("all_proxy", "")],
    );
}

#[test]
fn lowercase_http_proxy_is_the_final_fallback() {
    check("http", "http_proxy", "ok", REMOTE, "auto", &[]);
}

#[test]
fn empty_uppercase_all_proxy_uses_lowercase_value() {
    check(
        "socks5h",
        "all_proxy",
        "ok",
        REMOTE,
        "auto",
        &[("ALL_PROXY", "")],
    );
}

#[test]
fn socks5_password_authentication_uploads() {
    check("socks5h", "ALL_PROXY", "auth-ok", REMOTE, "auto", &[]);
}

#[test]
fn socks5_authentication_failure_is_not_a_timeout() {
    check("socks5h", "ALL_PROXY", "auth-rejected", REMOTE, "auto", &[]);
}

#[test]
fn socks5_connect_failure_is_not_a_timeout() {
    check(
        "socks5h",
        "ALL_PROXY",
        "connect-rejected",
        REMOTE,
        "auto",
        &[],
    );
}

#[test]
fn local_and_remote_socks_dns_uploads() {
    for (protocol, api) in [
        ("socks4", LOCAL),
        ("socks4a", REMOTE),
        ("socks5", LOCAL),
        ("socks5h", REMOTE),
        ("socks5h", "http://[::1]:8080"),
    ] {
        check(protocol, "ALL_PROXY", "ok", api, "auto", &[]);
    }
}

#[test]
fn silent_socks_greeting_obeys_global_timeout() {
    check("socks5h", "ALL_PROXY", "greeting", REMOTE, "auto", &[]);
}

#[test]
fn production_timeout_bounds_silent_socks_greeting() {
    check(
        "socks5h",
        "ALL_PROXY",
        "production-greeting",
        REMOTE,
        "auto",
        &[],
    );
}

#[test]
fn stalled_socks_stages_share_global_deadline() {
    for stage in [
        "auth",
        "connect",
        "partial-reply",
        "drip-reply",
        "headers",
        "body",
        "delayed-headers",
    ] {
        check("socks5h", "ALL_PROXY", stage, REMOTE, "auto", &[]);
    }
    check("socks4a", "ALL_PROXY", "connect", REMOTE, "auto", &[]);
    check(
        "socks5h",
        "ALL_PROXY",
        "tls",
        "https://gateway.invalid:8080",
        "auto",
        &[],
    );
}

#[test]
fn silent_http_connect_obeys_global_timeout() {
    check("http", "HTTPS_PROXY", "connect", REMOTE, "auto", &[]);
}

#[test]
fn custom_dns_does_not_replace_proxy_addresses() {
    for protocol in ["http", "socks5"] {
        check(protocol, "ALL_PROXY", "ok", LOCAL, "127.0.0.1:9", &[]);
    }
}

#[test]
fn remote_proxy_dns_does_not_query_custom_resolver() {
    // An open UDP socket gives us evidence that no DNS request was leaked,
    // rather than merely relying on the .invalid hostname failing to resolve.
    let dns = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    for protocol in ["http", "socks4a", "socks5h"] {
        check(
            protocol,
            "ALL_PROXY",
            "ok",
            REMOTE,
            &dns.local_addr().unwrap().to_string(),
            &[],
        );
    }
    dns.set_nonblocking(true).unwrap();
    let error = dns.recv_from(&mut [0; 512]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}

#[test]
fn no_proxy_uploads_directly_with_system_and_custom_dns() {
    for dns in ["auto", "127.0.0.1:9"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let mut command = child(&api, dns, "ok");
        command
            .env("ALL_PROXY", "socks5h://proxy.invalid:1080")
            .env("NO_PROXY", "127.0.0.1");
        let server = thread::spawn(move || serve(listener, "direct", "ok", &api));
        let output = run_child(command, TIMEOUT);
        let server_result = server.join();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        server_result.unwrap();
    }
}
