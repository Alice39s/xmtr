//! Uploaded traces: the admin link and the delete command printed after an upload, and
//! `xmtr traces`, which lists the uploads kept on this machine and deletes them.

#[path = "traces/store.rs"]
pub mod store;

use std::fmt::{self, Display};
use std::path::Path;
use std::sync::LazyLock;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use regex::Regex;
use ureq::Agent;
use url::Url;

use self::store::Record;
use crate::cli::{DeleteArgs, TracesArgs, TracesCommand};
use crate::dns::Dns;
use crate::model::UploadAccepted;
use crate::output::Stdout;
use crate::upload::{self, printable};

/// Labels in front of trace details are padded to the width of `trace-id`, so the values line up.
const LABEL_WIDTH: usize = 8;

/// `name` padded to the label width, with the space before the value.
pub fn label(name: &str) -> String {
    format!("{name:<LABEL_WIDTH$} ")
}

/// The admin link and the command that deletes the trace, each behind its label, for the fields
/// the gateway returned as plain URLs.
pub fn admin_lines(trace: &UploadAccepted) -> Vec<(String, String)> {
    let link = trace
        .admin_link()
        .map(|url| (label("admin"), url.to_owned()));
    let delete = trace
        .delete_command()
        .map(|command| (label("delete"), command));
    link.into_iter().chain(delete).collect()
}

/// Printed under the admin link and the delete command; `listed` adds where to find them again.
pub const fn admin_warning(listed: bool) -> &'static str {
    if listed {
        "Anyone with the admin link can delete this trace. Run xmtr traces to see these links again."
    } else {
        "Anyone with the admin link can delete this trace."
    }
}

/// The admin lines and the warning under them as plain text; none when the gateway returned no
/// admin link.
pub fn admin_text(trace: &UploadAccepted, listed: bool) -> Vec<String> {
    let mut lines: Vec<String> = admin_lines(trace)
        .into_iter()
        .map(|(label, value)| label + &value)
        .collect();
    if !lines.is_empty() {
        lines.push(admin_warning(listed).to_owned());
    }
    lines
}

/// Adds an accepted upload of `targets` to the gateway at `api` to the list of uploads.
pub fn save(trace: &UploadAccepted, api: &str, targets: &[String]) -> Result<()> {
    let record = Record {
        trace: trace.clone(),
        uploaded_at: humantime::format_rfc3339_seconds(SystemTime::now()).to_string(),
        api: api.to_owned(),
        targets: targets.to_vec(),
    };
    store::append(&store::path()?, &record)
}

/// Runs `xmtr traces`; `api` is the gateway for a trace the list holds no gateway for.
pub fn run(args: &TracesArgs, api: &str) -> Result<()> {
    let path = store::path()?;
    match &args.command {
        Some(TracesCommand::Delete(delete)) => {
            // The gateway is resolved and reached like for an upload.
            let dns = Dns::new(&delete.dns)?;
            let connect = |api: &str| upload::agent(api, &dns, upload::UPLOAD_TIMEOUT);
            let deletion = self::delete(connect, &path, delete, api)?;
            Stdout::new().line(deletion);
        }
        Some(TracesCommand::List(_)) | None => list(&path, args.json())?,
    }
    Ok(())
}

/// Warns about the lines of the list at `path` that hold no record.
fn warn_unreadable(path: &Path, list: &store::List) {
    if list.unreadable.is_empty() {
        return;
    }
    let numbers: Vec<String> = list.unreadable.iter().map(usize::to_string).collect();
    let noun = if numbers.len() == 1 { "line" } else { "lines" };
    eprintln!(
        "warning: skipped unreadable {noun} {} of {}",
        numbers.join(", "),
        path.display()
    );
}

/// Prints the list at `path`, newest first.
fn list(path: &Path, json: bool) -> Result<()> {
    let list = store::read(path)?;
    warn_unreadable(path, &list);
    let mut records = list.records;
    records.reverse();
    let mut stdout = Stdout::new();
    if json {
        stdout.line(serde_json::to_string_pretty(&records).expect("records always encode"));
    } else if records.is_empty() {
        eprintln!("No uploads in {}", path.display());
    } else {
        stdout.line(listing(&records));
    }
    Ok(())
}

/// One block per record, separated by blank lines: when and what was traced, the trace ID, the
/// IPMap link, the admin link and the delete command.
fn listing(records: &[Record]) -> String {
    let blocks: Vec<String> = records
        .iter()
        .map(|record| {
            let mut heading = format!("{}  {}", record.uploaded_at, record.targets.join(", "));
            if record.trace.pin {
                heading.push_str("  PIN");
            }
            let mut lines = vec![
                heading,
                label("trace-id") + &record.trace.id,
                record.trace.url.clone(),
            ];
            lines.extend(
                admin_lines(&record.trace)
                    .into_iter()
                    .map(|(label, value)| label + &value),
            );
            lines.join("\n")
        })
        .collect();
    blocks.join("\n\n")
}

/// A trace named on the command line: its ID, and the admin key an admin link carries.
#[derive(Debug, PartialEq, Eq)]
struct Named {
    id: String,
    admin_key: Option<String>,
}

/// A trace ID as the gateway issues it: a snowflake ID and an 8-letter view key.
static TRACE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{1,20}-[A-Za-z]{8}$").expect("the pattern is valid"));

/// Reads a trace ID, an IPMap link such as `https://xmsl.dev/xmtr/<ID>`, or a link with
/// `trace-id` and `admin-key` parameters such as the admin link; a link may leave out
/// `https://`. The error does not repeat `trace`, which may hold an admin key.
fn named(trace: &str) -> Result<Named> {
    const UNKNOWN: &str = "TRACE is not a trace ID, IPMap link or admin link";
    if TRACE_ID.is_match(trace) {
        return Ok(Named {
            id: trace.to_owned(),
            admin_key: None,
        });
    }
    let link = if trace.contains("://") {
        Url::parse(trace)
    } else {
        Url::parse(&format!("https://{trace}"))
    };
    let Some(link) = link
        .ok()
        .filter(|link| matches!(link.scheme(), "http" | "https"))
    else {
        bail!(UNKNOWN);
    };
    let parameter = |name: &str| {
        link.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    let id = parameter("trace-id").or_else(|| {
        link.path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .map(str::to_owned)
    });
    match id {
        Some(id) if TRACE_ID.is_match(&id) => Ok(Named {
            id,
            admin_key: parameter("admin-key"),
        }),
        _ => bail!(UNKNOWN),
    }
}

/// What `xmtr traces delete` did.
#[derive(Debug, PartialEq, Eq)]
struct Deletion {
    id: String,
    /// False when the gateway had no such trace.
    on_gateway: bool,
    /// True when the list held the trace.
    listed: bool,
}

impl Display for Deletion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = &self.id;
        match (self.on_gateway, self.listed) {
            (true, true) => write!(f, "Deleted {id} and removed it from the trace list"),
            (true, false) => write!(f, "Deleted {id}"),
            (false, _) => write!(
                f,
                "The gateway has no trace {id}; removed it from the trace list"
            ),
        }
    }
}

/// Deletes the trace `args` names from its gateway and from the list at `path`, with the agent
/// `connect` makes for the gateway. The admin key comes from `--admin-key`, the admin link or the
/// list, the gateway from `--api`, the list or `api`.
fn delete(
    connect: impl FnOnce(&str) -> Result<Agent>,
    path: &Path,
    args: &DeleteArgs,
    api: &str,
) -> Result<Deletion> {
    let named = named(&args.trace)?;
    let given_key = args.admin_key.clone().or(named.admin_key);
    // With the admin key and the gateway on the command line the list is not needed; a list that
    // cannot be read then only keeps the trace in it.
    let list = match store::read(path) {
        Ok(list) => {
            warn_unreadable(path, &list);
            Some(list)
        }
        Err(error) if given_key.is_some() && args.api.is_some() => {
            eprintln!("warning: {error:#}");
            None
        }
        Err(error) => return Err(error),
    };
    let record = list.as_ref().and_then(|list| {
        list.records
            .iter()
            .find(|record| record.trace.id == named.id)
    });
    let Some(admin_key) =
        given_key.or_else(|| record.and_then(|record| record.trace.admin_key.clone()))
    else {
        bail!(
            "no admin key for {}; pass --admin-key or the admin link",
            named.id
        );
    };
    let api = args
        .api
        .as_deref()
        .or(record.map(|record| record.api.as_str()))
        .unwrap_or(api);
    let on_gateway = upload::delete(&connect(api)?, api, &named.id, &admin_key)?;
    let listed = list.is_some()
        && store::remove(path, &named.id).with_context(|| {
            let done = if on_gateway { "deleted" } else { "not found" };
            format!(
                "{} {done} on the gateway; removing it from the trace list",
                named.id
            )
        })?;
    if !on_gateway && !listed {
        bail!(
            "the gateway at {} has no trace {}",
            printable(api),
            named.id
        );
    }
    Ok(Deletion {
        id: named.id,
        on_gateway,
        listed,
    })
}

#[cfg(test)]
pub mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use super::store::tests::{record, scratch};
    use super::*;
    use crate::model::Shell;

    pub const ID: &str = "103115083070058496-nBjPEnny";
    pub const KEY: &str = "1b4e28ba-2fa1-4d2b-883f-0016d3cca427";

    /// An upload accepted by a gateway that returns the admin key and its links.
    pub fn accepted() -> UploadAccepted {
        UploadAccepted {
            id: ID.to_owned(),
            pin: false,
            url: format!("https://xmsl.dev/xmtr/{ID}"),
            admin_key: Some(KEY.to_owned()),
            admin_url: Some(format!(
                "https://xmsl.dev/ipmap?plat=xmtr&trace-id={ID}&admin-key={KEY}"
            )),
            admin_api_url: Some(format!(
                "https://stage-info-api.xmsl.dev/api/v1/traces/{ID}?admin-key={KEY}"
            )),
        }
    }

    /// The delete line of [`accepted`], quoted for the shells of this system.
    fn delete_line() -> String {
        let url = format!("https://stage-info-api.xmsl.dev/api/v1/traces/{ID}?admin-key={KEY}");
        format!("delete   {}", Shell::NATIVE.delete_command(&url).unwrap())
    }

    #[test]
    fn lines_up_the_admin_link_and_delete_command_with_the_trace_id() {
        assert_eq!(
            admin_text(&accepted(), true),
            [
                format!("admin    https://xmsl.dev/ipmap?plat=xmtr&trace-id={ID}&admin-key={KEY}"),
                delete_line(),
                "Anyone with the admin link can delete this trace. Run xmtr traces to see these links again."
                    .to_owned(),
            ]
        );
        assert_eq!(
            admin_text(&accepted(), false).last().map(String::as_str),
            Some("Anyone with the admin link can delete this trace.")
        );
        assert_eq!(label("trace-id"), "trace-id ");
    }

    #[test]
    fn prints_nothing_for_gateways_without_admin_links() {
        let trace = UploadAccepted {
            admin_key: None,
            admin_url: None,
            admin_api_url: None,
            ..accepted()
        };
        assert!(admin_text(&trace, true).is_empty());
    }

    #[test]
    fn lists_each_upload_with_its_links() {
        let mut old = record("1-a");
        old.trace.admin_key = None;
        old.trace.admin_url = None;
        old.trace.admin_api_url = None;
        old.trace.pin = true;
        old.targets = vec!["example.com".to_owned()];
        assert_eq!(
            listing(&[record(ID), old]),
            format!(
                "2026-11-09T04:12:33Z  1.1.1.1, cloudflare.com
trace-id {ID}
https://xmsl.dev/xmtr/{ID}
admin    https://xmsl.dev/ipmap?plat=xmtr&trace-id={ID}&admin-key={KEY}
{}

2026-11-09T04:12:33Z  example.com  PIN
trace-id 1-a
https://xmsl.dev/xmtr/1-a",
                delete_line()
            )
        );
    }

    #[test]
    fn reads_trace_ids_and_links() {
        let id = |id: &str| Named {
            id: id.to_owned(),
            admin_key: None,
        };
        assert_eq!(named(ID).unwrap(), id(ID));
        assert_eq!(
            named(&format!("https://xmsl.dev/xmtr/{ID}")).unwrap(),
            id(ID)
        );
        assert_eq!(
            named(&format!("https://xmsl.dev/xmtr/{ID}/")).unwrap(),
            id(ID)
        );
        assert_eq!(
            named(&format!("https://xmsl.dev/ipmap?plat=xmtr&trace-id={ID}")).unwrap(),
            id(ID)
        );
        let admin = Named {
            id: ID.to_owned(),
            admin_key: Some(KEY.to_owned()),
        };
        assert_eq!(
            named(accepted().admin_url.as_deref().unwrap()).unwrap(),
            admin
        );
        assert_eq!(
            named(&format!(
                "xmsl.dev/ipmap?plat=xmtr&trace-id={ID}&admin-key={KEY}"
            ))
            .unwrap(),
            admin
        );
        assert_eq!(named(&format!("xmsl.dev/xmtr/{ID}")).unwrap(), id(ID));
    }

    #[test]
    fn rejects_other_input_without_repeating_it() {
        for trace in [
            "https://xmsl.dev".to_owned(),
            "1-a".to_owned(),
            String::new(),
            "..".to_owned(),
            KEY.to_owned(),
            format!("xmsl.dev/ipmap?plat=xmtr&trace-id=1-a&admin-key={KEY}"),
            format!("https://xmsl.dev/ipmap?trace-id=%1b%5b31mX&admin-key={KEY}"),
            format!("ftp://xmsl.dev/xmtr/{ID}"),
        ] {
            let error = named(&trace).unwrap_err().to_string();
            assert_eq!(
                error, "TRACE is not a trace ID, IPMap link or admin link",
                "{trace}"
            );
        }
    }

    /// Answers one request on a port of this machine with `status` and a problem `body`, and
    /// returns the gateway address and the request head.
    fn gateway(status: &str, body: &str) -> (String, JoinHandle<String>) {
        server(status, "application/problem+json", body)
    }

    /// Answers one request on a port of this machine with `status` and a `body` of
    /// `content_type`, and returns the server address and the request head.
    fn server(status: &str, content_type: &str, body: &str) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut stream = BufReader::new(stream);
            let mut head = String::new();
            while !head.ends_with("\r\n\r\n") {
                assert_ne!(stream.read_line(&mut head).unwrap(), 0);
            }
            stream.get_mut().write_all(response.as_bytes()).unwrap();
            head
        });
        (api, server)
    }

    /// Connects to the test servers on this machine whatever the proxy settings.
    fn direct(_: &str) -> Result<Agent> {
        Ok(upload::direct_agent())
    }

    fn delete_args(trace: &str, admin_key: Option<&str>, api: Option<&str>) -> DeleteArgs {
        DeleteArgs {
            trace: trace.to_owned(),
            admin_key: admin_key.map(str::to_owned),
            api: api.map(str::to_owned),
            dns: crate::dns::DnsSpec::Auto,
        }
    }

    #[test]
    fn deletes_a_listed_trace_with_its_key_on_its_gateway() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let (api, server) = gateway("200 OK", r#"{"ok":true,"deleted":true}"#);
        let mut listed = record(ID);
        listed.api = api;
        store::append(&path, &record("1-a")).unwrap();
        store::append(&path, &listed).unwrap();
        let deletion = delete(
            direct,
            &path,
            &delete_args(&format!("https://xmsl.dev/xmtr/{ID}"), None, None),
            "http://192.0.2.1:9",
        )
        .unwrap();
        assert_eq!(
            deletion,
            Deletion {
                id: ID.to_owned(),
                on_gateway: true,
                listed: true,
            }
        );
        assert_eq!(
            deletion.to_string(),
            format!("Deleted {ID} and removed it from the trace list")
        );
        let head = server.join().unwrap().to_ascii_lowercase();
        assert!(
            head.starts_with(&format!(
                "delete /api/v1/traces/{} http/1.1\r\n",
                ID.to_ascii_lowercase()
            )),
            "{head}"
        );
        assert!(
            head.contains(&format!("\r\nx-trace-admin-key: {KEY}\r\n")),
            "{head}"
        );
        assert_eq!(store::read(&path).unwrap().records, [record("1-a")]);
    }

    #[test]
    fn an_admin_link_deletes_a_trace_the_list_does_not_hold() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let (api, server) = gateway("200 OK", r#"{"ok":true}"#);
        let deletion = delete(
            direct,
            &path,
            &delete_args(accepted().admin_url.as_deref().unwrap(), None, Some(&api)),
            "http://192.0.2.1:9",
        )
        .unwrap();
        assert_eq!(deletion.to_string(), format!("Deleted {ID}"));
        assert!(server.join().unwrap().contains(KEY));
    }

    #[test]
    fn a_trace_the_gateway_lost_leaves_the_list() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let (api, server) = gateway("404 Not Found", r#"{"title":"Not Found"}"#);
        let mut listed = record(ID);
        listed.api = api.clone();
        store::append(&path, &listed).unwrap();
        let deletion = delete(direct, &path, &delete_args(ID, None, None), &api).unwrap();
        assert!(!deletion.on_gateway && deletion.listed);
        assert_eq!(store::read(&path).unwrap(), store::List::default());
        server.join().unwrap();

        let (api, server) = gateway("404 Not Found", r#"{"title":"Not Found"}"#);
        let error = delete(
            direct,
            &path,
            &delete_args(ID, Some(KEY), Some(&format!("{api}/\u{1b}[2J"))),
            &api,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("the gateway at {api}/\u{fffd}[2J has no trace {ID}")
        );
        assert!(!format!("{error:#}").contains(KEY));
        server.join().unwrap();
    }

    #[test]
    fn a_404_from_another_server_keeps_the_trace_listed() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        store::append(&path, &record(ID)).unwrap();
        for content_type in ["text/plain; charset=UTF-8", "text/html"] {
            let (api, server) = server("404 Not Found", content_type, "404 Not Found");
            let error =
                delete(direct, &path, &delete_args(ID, None, Some(&api)), &api).unwrap_err();
            assert_eq!(
                error.to_string(),
                "deletion rejected with HTTP 404: Not Found"
            );
            server.join().unwrap();
        }
        assert_eq!(store::read(&path).unwrap().records, [record(ID)]);

        let (api, server) = server(
            "404 Not Found",
            "Application/Problem+JSON; charset=utf-8",
            r#"{"title":"Not Found"}"#,
        );
        let deletion = delete(direct, &path, &delete_args(ID, None, Some(&api)), &api).unwrap();
        assert!(!deletion.on_gateway && deletion.listed);
        server.join().unwrap();
    }

    #[test]
    fn a_rejected_key_keeps_the_trace_listed() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let (api, server) = gateway(
            "403 Forbidden",
            r#"{"title":"Forbidden","detail":"The admin key does not match this trace\u001b[2J","adminKeyRejected":true}"#,
        );
        store::append(&path, &record(ID)).unwrap();
        let error = delete(
            direct,
            &path,
            &delete_args(ID, Some("00000000-0000-4000-8000-000000000000"), Some(&api)),
            &api,
        )
        .unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            "deletion rejected with HTTP 403: The admin key does not match this trace\u{fffd}[2J"
        );
        assert!(
            server
                .join()
                .unwrap()
                .contains("00000000-0000-4000-8000-000000000000")
        );
        assert_eq!(store::read(&path).unwrap().records, [record(ID)]);
    }

    #[test]
    fn unreadable_lines_in_the_list_do_not_stop_a_deletion() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let (api, server) = gateway("200 OK", r#"{"ok":true}"#);
        let mut listed = record(ID);
        listed.api = api;
        std::fs::write(&path, "{not json\n").unwrap();
        store::append(&path, &listed).unwrap();
        let deletion = delete(
            direct,
            &path,
            &delete_args(ID, None, None),
            "http://192.0.2.1:9",
        )
        .unwrap();
        assert!(deletion.on_gateway && deletion.listed);
        assert!(server.join().unwrap().contains(KEY));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json\n");
    }

    #[test]
    fn a_list_that_cannot_be_read_needs_the_key_and_gateway_on_the_command_line() {
        let dir = scratch();
        // A directory in place of the list cannot be read as one.
        let path = dir.path().join("traces.jsonl");
        std::fs::create_dir(&path).unwrap();
        let (api, server) = gateway("200 OK", r#"{"ok":true}"#);
        let admin_link = accepted().admin_url.unwrap();
        let deletion = delete(
            direct,
            &path,
            &delete_args(&admin_link, None, Some(&api)),
            "http://192.0.2.1:9",
        )
        .unwrap();
        assert_eq!(deletion.to_string(), format!("Deleted {ID}"));
        server.join().unwrap();

        let error = delete(
            direct,
            &path,
            &delete_args(&admin_link, None, None),
            "http://192.0.2.1:9",
        )
        .unwrap_err();
        assert!(error.to_string().starts_with("reading "), "{error:#}");
    }

    #[test]
    fn needs_an_admin_key_before_asking_the_gateway() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let error = delete(
            direct,
            &path,
            &delete_args(ID, None, None),
            "http://192.0.2.1:9",
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("no admin key for {ID}; pass --admin-key or the admin link")
        );
    }
}
