//! Reports for the xmtr developers, sent to GlitchTip through the Sentry SDK.
//!
//! One run in a hundred that uses the network anyway sends the xmtr version, the operating system
//! with its distribution, and the CPU architecture. Errors and crashes are held back until the
//! person at the terminal agrees to send them; without a terminal to ask they are never sent.
//! `DO_NOT_TRACK` turns both off, as <https://consoledonottrack.com> describes.

use std::env;
use std::io::{self, IsTerminal};
use std::mem;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::panic::PanicHookInfo;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use regex::{Captures, Regex};
use sentry::integrations::panic::PanicIntegration;
use sentry::protocol::{
    Context, DeviceContext, EnvelopeItem, Event, Frame, OsContext, SpanStatus, Stacktrace,
};
use sentry::transports::UreqHttpTransportOptions;
use sentry::types::Dsn;
use sentry::{
    ClientInitGuard, ClientOptions, Envelope, Transaction, TransactionContext, Transport,
    TransportOptions,
};

use crate::cli::{self, Cli, Command, DEFAULT_API};
use crate::dns::{DnsSpec, Host};
use crate::live::Console;
use crate::upload;

/// Where reports go, fixed when xmtr is built: release builds take it from the `XMTR_TELEMETRY_DSN`
/// secret, and builds without it send nothing.
const DSN: Option<&str> = option_env!("XMTR_TELEMETRY_DSN");
/// One run in this many sends its version and system.
const SAMPLE: u128 = 100;
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest a report holds up the end of a run.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(3);

/// True when `DO_NOT_TRACK` holds anything but an empty value, `0` or `false`.
fn do_not_track(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        !matches!(value.trim(), "" | "0") && !value.eq_ignore_ascii_case("false")
    })
}

/// What a run may send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Policy {
    /// The sampled version and system; only for runs that use the network anyway.
    usage: bool,
    /// Errors and crashes, after asking at the terminal.
    ask: bool,
}

fn policy(cli: &Cli, do_not_track: bool, interactive: bool) -> Option<Policy> {
    if do_not_track {
        return None;
    }
    let policy = Policy {
        usage: cli.command.is_some() || !cli.no_upload,
        ask: interactive,
    };
    (policy.usage || policy.ask).then_some(policy)
}

/// The name of the run in usage reports.
fn command_name(cli: &Cli) -> &'static str {
    match &cli.command {
        Some(Command::Manage(cli::SelfCommand::Update { .. })) => "self update",
        Some(Command::Manage(cli::SelfCommand::Uninstall)) => "self uninstall",
        None => "trace",
    }
}

/// The operating system or distribution, its version and edition.
#[derive(Debug, Clone, PartialEq, Eq)]
struct System {
    name: String,
    version: Option<String>,
    edition: Option<String>,
}

#[cfg(not(target_os = "macos"))]
fn system() -> System {
    let info = os_info::get();
    System {
        name: info.os_type().to_string(),
        version: (*info.version() != os_info::Version::Unknown).then(|| info.version().to_string()),
        edition: info.edition().map(str::to_owned),
    }
}

#[cfg(target_os = "macos")]
fn system() -> System {
    use sysctl::Sysctl;
    System {
        name: "macOS".to_owned(),
        // Without the version the report still names the system.
        version: sysctl::Ctl::new("kern.osproductversion")
            .and_then(|ctl| ctl.value_string())
            .ok(),
        edition: None,
    }
}

/// Looking the system up may start programs such as `lsb_release`, so it happens only for reports
/// that are sent.
static SYSTEM: LazyLock<System> = LazyLock::new(system);

fn os() -> Context {
    OsContext {
        name: Some(SYSTEM.name.clone()),
        version: SYSTEM.version.clone(),
        build: SYSTEM.edition.clone(),
        ..OsContext::default()
    }
    .into()
}

/// The name of a usage report. GlitchTip keeps only a count per transaction name and operation, so
/// the name carries the release, the system and the architecture the count is about.
fn usage_name(release: &str, system: &str, version: Option<&str>, arch: &str) -> String {
    match version {
        Some(version) => format!("{release} · {system} {version} · {arch}"),
        None => format!("{release} · {system} · {arch}"),
    }
}

fn device() -> Context {
    DeviceContext {
        arch: Some(env::consts::ARCH.into()),
        ..DeviceContext::default()
    }
    .into()
}

/// Removes what could identify the user, their machine or their network from report text: IP
/// addresses, the hosts they traced or resolve through, their home directory, user names in paths,
/// and credentials in URLs.
#[derive(Debug, Default)]
struct Redactor {
    names: Option<Regex>,
    home: Option<String>,
}

static CREDENTIALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b([a-z][a-z0-9+.-]*://)[^/@\s]+@").expect("the pattern is valid")
});
static USER_DIRECTORY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(/home/|/Users/|\\Users\\)[^/\\\s]+").expect("the pattern is valid")
});
/// Words that may hold an address: an IP address, with a port, an IPv6 zone or punctuation after it.
static WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9A-Za-z:.%]+").expect("the pattern is valid"));

/// True when `text` is an IP address, with or without a port or an IPv6 zone.
fn is_address(text: &str) -> bool {
    text.parse::<IpAddr>().is_ok()
        || text.parse::<SocketAddr>().is_ok()
        || text
            .split_once('%')
            .is_some_and(|(address, _)| address.parse::<Ipv6Addr>().is_ok())
}

fn redact_addresses(text: &str) -> String {
    WORD.replace_all(text, |captures: &Captures<'_>| {
        let word = &captures[0];
        let bare = word.trim_end_matches(['.', ':']);
        if is_address(bare) {
            format!("[ip]{}", &word[bare.len()..])
        } else {
            word.to_owned()
        }
    })
    .into_owned()
}

impl Redactor {
    /// Hides `names` besides the addresses, paths and credentials every report loses.
    fn new(names: impl IntoIterator<Item = String>, home: Option<String>) -> Self {
        let names: Vec<String> = names
            .into_iter()
            .filter(|name| name.len() > 2 && !is_address(name))
            .map(|name| regex::escape(&name))
            .collect();
        Self {
            names: (!names.is_empty()).then(|| {
                Regex::new(&format!(r"(?i)\b(?:{})\b", names.join("|")))
                    .expect("escaped names form a valid pattern")
            }),
            home: home.filter(|home| home.len() > 1),
        }
    }

    /// The hosts `cli` names that are not public services xmtr knows: the targets, a resolver
    /// given by name, a gateway other than the default, and proxies from the environment.
    fn for_run(cli: &Cli) -> Self {
        let mut names = cli.targets.clone();
        if let DnsSpec::Server {
            host: Host::Name(name),
            ..
        } = &cli.dns
        {
            names.push(name.clone());
        }
        if cli.api != DEFAULT_API
            && let Ok(uri) = cli.api.parse::<ureq::http::Uri>()
            && let Some(host) = uri.host()
        {
            names.push(host.to_owned());
        }
        names.extend(upload::proxy_host());
        let home = env::home_dir().map(|home| home.to_string_lossy().into_owned());
        Self::new(names, home)
    }

    fn text(&self, text: &str) -> String {
        let mut text = CREDENTIALS
            .replace_all(text, "${1}[credentials]@")
            .into_owned();
        if let Some(names) = &self.names {
            text = names.replace_all(&text, "[host]").into_owned();
        }
        if let Some(home) = &self.home {
            text = text.replace(home.as_str(), "~");
        }
        text = USER_DIRECTORY.replace_all(&text, "${1}[user]").into_owned();
        redact_addresses(&text)
    }

    fn option(&self, text: &mut Option<String>) {
        if let Some(value) = text {
            *value = self.text(value);
        }
    }

    fn stacktrace(&self, stacktrace: Option<&mut Stacktrace>) {
        for frame in stacktrace.into_iter().flat_map(|trace| &mut trace.frames) {
            let Frame {
                filename, abs_path, ..
            } = frame;
            self.option(filename);
            self.option(abs_path);
        }
    }

    /// The event without personal data, with the system it happened on.
    fn event(&self, mut event: Event<'static>) -> Event<'static> {
        event.server_name = None;
        event.user = None;
        event.request = None;
        event.breadcrumbs.values.clear();
        event.extra.clear();
        self.option(&mut event.message);
        self.option(&mut event.culprit);
        if let Some(entry) = &mut event.logentry {
            entry.message = self.text(&entry.message);
            entry.params.clear();
        }
        for value in event.tags.values_mut() {
            *value = self.text(value);
        }
        for exception in &mut event.exception.values {
            self.option(&mut exception.value);
            self.stacktrace(exception.stacktrace.as_mut());
        }
        for thread in &mut event.threads.values {
            self.stacktrace(thread.stacktrace.as_mut());
        }
        event.contexts.insert("os".to_owned(), os());
        event.contexts.insert("device".to_owned(), device());
        event
    }
}

/// Sends usage reports right away and holds errors until the user decides about them. Nothing
/// else leaves the machine.
struct Gate {
    inner: Box<dyn Transport>,
    held: Mutex<Vec<Envelope>>,
}

impl Gate {
    fn held(&self) -> usize {
        self.held
            .lock()
            .expect("the gate lock is never poisoned")
            .len()
    }

    /// Sends the held errors, or drops them when `send` is false.
    fn release(&self, send: bool) {
        let held = mem::take(&mut *self.held.lock().expect("the gate lock is never poisoned"));
        if send {
            for envelope in held {
                self.inner.send_envelope(envelope);
            }
        }
    }
}

impl Transport for Gate {
    fn send_envelope(&self, envelope: Envelope) {
        if envelope
            .items()
            .any(|item| matches!(item, EnvelopeItem::Transaction(_)))
        {
            self.inner.send_envelope(envelope);
        } else if envelope.event().is_some() {
            self.held
                .lock()
                .expect("the gate lock is never poisoned")
                .push(envelope);
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        self.inner.flush(timeout)
    }

    fn shutdown(&self, timeout: Duration) -> bool {
        self.inner.shutdown(timeout)
    }
}

/// Asks about the held errors, or the crash, and sends them when the user agrees.
fn settle(gate: &Gate, console: Option<Console>, crash: bool) {
    if gate.held() == 0 {
        return;
    }
    // A terminal that cannot be read answers no.
    let send = console.is_some_and(|console| console.offer_report(crash).unwrap_or(false));
    gate.release(send);
    if send {
        gate.flush(FLUSH_TIMEOUT);
    }
}

/// Only the first panic of a run is reported; others follow from it.
static PANICKED: AtomicBool = AtomicBool::new(false);

fn report_panic(info: &PanicHookInfo<'_>, gate: &Gate, console: Option<Console>) {
    if PANICKED.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut event = PanicIntegration::new().event_from_panic_info(info);
    if let Some(location) = info.location() {
        event.culprit = Some(format!("{}:{}", location.file(), location.line()));
    }
    sentry::capture_event(event);
    settle(gate, console, true);
}

struct Active {
    gate: Arc<Gate>,
    console: Option<Console>,
    run: Option<Transaction>,
    _guard: ClientInitGuard,
}

/// Reports of one run. Inactive when `DO_NOT_TRACK` is set or the run can send nothing.
pub struct Telemetry(Option<Active>);

impl Telemetry {
    pub fn start(cli: &Cli, console: Console) -> Self {
        // An empty value is what CI builds without access to the secret get.
        let Some(dsn) = DSN.filter(|dsn| !dsn.is_empty()) else {
            return Self(None);
        };
        // Parsed before anything else, so every build with a broken DSN fails its smoke test.
        let dsn: Dsn = dsn.parse().expect("XMTR_TELEMETRY_DSN is a valid DSN");
        let interactive =
            cli::interactive(cli, io::stdin().is_terminal(), io::stderr().is_terminal());
        let opted_out = do_not_track(env::var("DO_NOT_TRACK").ok().as_deref());
        Self(
            policy(cli, opted_out, interactive)
                .map(|policy| Active::start(cli, console, policy, dsn)),
        )
    }

    /// Asks whether to send the errors captured so far, and sends them when the user agrees.
    pub fn ask(&self) {
        if let Some(active) = &self.0 {
            settle(&active.gate, active.console, false);
        }
    }

    /// Ends the run: sends its usage report when sampled and asks about errors not yet settled.
    pub fn finish(self) {
        let Some(active) = self.0 else {
            return;
        };
        if let Some(run) = active.run {
            run.set_status(if active.gate.held() == 0 {
                SpanStatus::Ok
            } else {
                SpanStatus::InternalError
            });
            sentry::configure_scope(|scope| {
                scope.set_context("os", os());
                scope.set_context("device", device());
            });
            run.finish();
        }
        settle(&active.gate, active.console, false);
    }
}

impl Active {
    fn start(cli: &Cli, console: Console, policy: Policy, dsn: Dsn) -> Self {
        let sampled = policy.usage
            && sentry::types::random_uuid()
                .as_u128()
                .is_multiple_of(SAMPLE);
        let redactor = Redactor::for_run(cli);
        let mut options = ClientOptions::new()
            .default_integrations(false)
            .shutdown_timeout(FLUSH_TIMEOUT)
            .before_send(move |event| Some(redactor.event(event)))
            .traces_sample_rate(if sampled { 1.0 } else { 0.0 });
        options.dsn = Some(dsn);
        options.release = sentry::release_name!();
        let transport =
            TransportOptions::try_from_client_options(&options).expect("the options hold a DSN");
        let gate = Arc::new(Gate {
            inner: Box::new(
                UreqHttpTransportOptions::from(transport)
                    .with_agent(upload::proxied_agent(SEND_TIMEOUT))
                    .build(),
            ),
            held: Mutex::new(Vec::new()),
        });
        let guard = sentry::init(options.transport(Arc::clone(&gate)));
        let console = policy.ask.then_some(console);
        let hook_gate = Arc::clone(&gate);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            report_panic(info, &hook_gate, console);
        }));
        let run = sampled.then(|| {
            let name = usage_name(
                &sentry::release_name!().unwrap_or_default(),
                &SYSTEM.name,
                SYSTEM.version.as_deref(),
                env::consts::ARCH,
            );
            sentry::start_transaction(TransactionContext::new(&name, command_name(cli)))
        });
        Self {
            gate,
            console,
            run,
            _guard: guard,
        }
    }
}

/// Records `error` for the report the user is asked about at the end of the run.
pub fn capture(error: &anyhow::Error) {
    sentry::integrations::anyhow::capture_anyhow(error);
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use sentry::protocol::{Exception, Transaction, User};

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from([&["xmtr"], args].concat()).unwrap()
    }

    #[test]
    fn usage_reports_are_named_after_release_system_and_architecture() {
        assert_eq!(
            usage_name("xmtr@0.4.0", "Debian", Some("13.0.0"), "x86_64"),
            "xmtr@0.4.0 · Debian 13.0.0 · x86_64"
        );
        assert_eq!(
            usage_name("xmtr@0.4.0", "Android", None, "aarch64"),
            "xmtr@0.4.0 · Android · aarch64"
        );
    }

    #[test]
    fn names_the_system() {
        let system = system();
        assert!(!system.name.is_empty(), "{system:?}");
        assert!(
            system
                .version
                .as_ref()
                .is_none_or(|version| !version.is_empty()),
            "{system:?}"
        );
    }

    #[test]
    fn do_not_track_turns_telemetry_off() {
        for on in ["1", "true", "yes", " 1 "] {
            assert!(do_not_track(Some(on)), "{on:?}");
        }
        for off in [None, Some(""), Some("0"), Some("false"), Some("FALSE")] {
            assert!(!do_not_track(off), "{off:?}");
        }
    }

    #[test]
    fn usage_needs_the_network_and_errors_need_someone_to_ask() {
        let trace = cli(&["1.1.1.1"]);
        let offline = cli(&["--no-upload", "1.1.1.1"]);
        let update = cli(&["self", "update"]);
        assert_eq!(policy(&trace, true, true), None);
        assert_eq!(
            policy(&trace, false, false),
            Some(Policy {
                usage: true,
                ask: false
            })
        );
        assert_eq!(
            policy(&offline, false, true),
            Some(Policy {
                usage: false,
                ask: true
            })
        );
        assert_eq!(policy(&offline, false, false), None);
        assert_eq!(
            policy(&update, false, false),
            Some(Policy {
                usage: true,
                ask: false
            })
        );
        assert_eq!(command_name(&trace), "trace");
        assert_eq!(command_name(&update), "self update");
    }

    #[test]
    fn removes_addresses_but_keeps_names_that_only_look_like_them() {
        let redactor = Redactor::default();
        for (text, expected) in [
            (
                "1.1.1.1 v4 ICMP: no probe was answered",
                "[ip] v4 ICMP: no probe was answered",
            ),
            ("connecting to 192.0.2.7:443.", "connecting to [ip]."),
            ("route to [2001:db8::1]:443", "route to [[ip]]:443"),
            ("bind fe80::1%en0 failed", "bind [ip] failed"),
            ("target 2606:4700::1111:", "target [ip]:"),
            ("std::io::Error: deadline", "std::io::Error: deadline"),
            ("xmtr 0.3.3 on 10.2", "xmtr 0.3.3 on 10.2"),
            ("cafe::face", "[ip]"),
        ] {
            assert_eq!(redactor.text(text), expected, "{text}");
        }
    }

    #[test]
    fn removes_named_hosts_paths_and_credentials() {
        let redactor = Redactor::new(
            ["internal.example.org", "1.1.1.1", "a"].map(str::to_owned),
            Some("/srv/people/alice".to_owned()),
        );
        assert_eq!(
            redactor.text("Internal.Example.org: no address; a is fine"),
            "[host]: no address; a is fine"
        );
        assert_eq!(
            redactor.text("reading /srv/people/alice/.config/xmtr and /home/bob/x"),
            "reading ~/.config/xmtr and /home/[user]/x"
        );
        assert_eq!(
            redactor.text(r"C:\Users\carol\xmtr.exe and /Users/dave/bin"),
            r"C:\Users\[user]\xmtr.exe and /Users/[user]/bin"
        );
        assert_eq!(
            redactor.text("proxy socks5h://me:secret@203.0.113.9:1080 refused"),
            "proxy socks5h://[credentials]@[ip] refused"
        );
    }

    #[test]
    fn hides_the_hosts_a_run_names() {
        let run = cli(&[
            "--dns",
            "https://doh.corp.example/dns-query",
            "--api",
            "https://gateway.corp.example",
            "host.corp.example",
            "192.0.2.1",
        ]);
        let text = Redactor::for_run(&run).text(
            "host.corp.example via doh.corp.example, upload to https://gateway.corp.example/api",
        );
        assert_eq!(text, "[host] via [host], upload to https://[host]/api");
        let preset = cli(&["--dns", "google", "1.1.1.1"]);
        assert_eq!(
            Redactor::for_run(&preset).text("uploading to stage-info-api.xmsl.dev via google"),
            "uploading to stage-info-api.xmsl.dev via google"
        );
    }

    #[test]
    fn events_lose_personal_fields_and_gain_the_system() {
        let event = Event {
            server_name: Some("alice-laptop".into()),
            user: Some(User {
                username: Some("alice".to_owned()),
                ..User::default()
            }),
            culprit: Some("/home/alice/src/main.rs:3".to_owned()),
            exception: vec![Exception {
                ty: "Error".to_owned(),
                value: Some("connecting to 198.51.100.4".to_owned()),
                ..Exception::default()
            }]
            .into(),
            ..Event::default()
        };
        let event = Redactor::default().event(event);
        assert_eq!(event.server_name, None);
        assert_eq!(event.user, None);
        assert_eq!(event.culprit.as_deref(), Some("/home/[user]/src/main.rs:3"));
        assert_eq!(
            event.exception.values[0].value.as_deref(),
            Some("connecting to [ip]")
        );
        assert!(event.contexts.contains_key("os"));
        assert!(event.contexts.contains_key("device"));
    }

    type Sent = Arc<Mutex<Vec<Envelope>>>;

    struct Recorder(Sent);

    impl Transport for Recorder {
        fn send_envelope(&self, envelope: Envelope) {
            self.0.lock().unwrap().push(envelope);
        }
    }

    fn gate() -> (Gate, Sent) {
        let sent = Sent::default();
        let gate = Gate {
            inner: Box::new(Recorder(Arc::clone(&sent))),
            held: Mutex::new(Vec::new()),
        };
        (gate, sent)
    }

    fn sent(recorder: &Sent) -> usize {
        recorder.lock().unwrap().len()
    }

    #[test]
    fn the_gate_sends_usage_at_once_and_errors_only_when_released() {
        let (gate, recorder) = gate();
        let mut usage = Envelope::new();
        usage.add_item(Transaction::default());
        gate.send_envelope(usage);
        assert_eq!(sent(&recorder), 1);

        gate.send_envelope(Event::default().into());
        gate.send_envelope(Event::default().into());
        gate.send_envelope(Envelope::new());
        assert_eq!((gate.held(), sent(&recorder)), (2, 1));
        gate.release(false);
        assert_eq!((gate.held(), sent(&recorder)), (0, 1));

        gate.send_envelope(Event::default().into());
        gate.release(true);
        assert_eq!((gate.held(), sent(&recorder)), (0, 2));
    }
}
