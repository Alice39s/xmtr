use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use super::upload;
use crate::dns::Dns;
use crate::model::{Client, Upload};

const CHILD: &str = "XMTR_PROXY_TEST_CHILD";
const TIMEOUT: Duration = Duration::from_secs(5);

// Run environment-sensitive uploads in separate processes so parallel tests never
// change each other's proxy settings. The reserved .invalid host must be sent to
// the proxy, not resolved locally. No trace or external gateway is involved.
#[test]
fn upload_child() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let dns = Dns::new(&"auto".parse().unwrap()).unwrap();
    let body = Upload {
        schema: 1,
        client: Client::current(),
        share_origin: false,
        pin: None,
        routes: vec![],
    };
    let accepted = upload("http://gateway.invalid:8080", &body, &dns).unwrap();
    assert_eq!(accepted.id, "proxy-test");
    assert_eq!(accepted.url, "https://example.invalid/proxy-test");
    assert!(!accepted.pin);
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

fn serve(listener: TcpListener, socks: bool) {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "upload never connected to proxy");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accepting proxy connection: {error}"),
        }
    };
    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
    if socks {
        let mut greeting = [0; 2];
        stream.read_exact(&mut greeting).unwrap();
        assert_eq!(greeting[0], 5);
        let mut methods = vec![0; greeting[1] as usize];
        stream.read_exact(&mut methods).unwrap();
        assert!(methods.contains(&0));
        stream.write_all(&[5, 0]).unwrap();
        let mut request = [0; 5];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request[..4], &[5, 1, 0, 3]);
        let mut host = vec![0; request[4] as usize];
        stream.read_exact(&mut host).unwrap();
        assert_eq!(host, b"gateway.invalid");
        let mut port = [0; 2];
        stream.read_exact(&mut port).unwrap();
        assert_eq!(u16::from_be_bytes(port), 8080);
        stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
    }
    let mut stream = BufReader::new(stream);
    if !socks {
        assert!(headers(&mut stream).starts_with("CONNECT gateway.invalid:8080 HTTP/1.1\r\n"));
        stream
            .get_mut()
            .write_all(b"HTTP/1.1 200 OK\r\n\r\n")
            .unwrap();
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
    let response = r#"{"id":"proxy-test","pin":false,"url":"https://example.invalid/proxy-test"}"#;
    write!(stream.get_mut(), "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
}

fn check_proxy(socks_variable: Option<&str>, empty_all_proxy: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "upload::proxy_tests::upload_child",
        "--nocapture",
    ]);
    command.env(CHILD, "1");
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
    // Lower-priority proxies must not win over the configured working proxy.
    command.env("HTTP_PROXY", "http://127.0.0.1:0");
    if let Some(variable) = socks_variable {
        command.env(variable, format!("socks5h://{address}"));
        command.env("HTTPS_PROXY", "http://127.0.0.1:0");
        if variable == "ALL_PROXY" {
            command.env("all_proxy", "socks5h://127.0.0.1:0");
        }
    } else {
        command.env("HTTPS_PROXY", format!("http://{address}"));
    }
    if empty_all_proxy {
        command.env("ALL_PROXY", "").env("all_proxy", "");
    }
    let socks = socks_variable.is_some();
    let server = thread::spawn(move || serve(listener, socks));
    let output = command.output().unwrap();
    let server_result = server.join();
    assert!(
        output.status.success(),
        "upload subprocess failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    server_result.unwrap();
}

#[test]
fn all_proxy_socks5h_takes_precedence() {
    check_proxy(Some("ALL_PROXY"), false);
}

#[test]
fn lowercase_all_proxy_supports_socks5h() {
    check_proxy(Some("all_proxy"), false);
}

#[test]
fn unset_all_proxy_falls_back_to_https_proxy() {
    check_proxy(None, false);
}

#[test]
fn empty_all_proxy_falls_back_to_https_proxy() {
    check_proxy(None, true);
}
