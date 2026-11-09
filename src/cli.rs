use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::dns::DnsSpec;

pub const DEFAULT_API: &str = "https://stage-info-api.xmsl.dev";

/// Trace several targets in parallel with trippy, then publish the routes to xmsl.dev IPMap.
#[derive(Debug, Parser)]
#[command(
    name = "xmtr",
    version,
    about,
    long_about = None,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Hostnames or IP addresses to trace.
    #[arg(required = true, value_name = "TARGET")]
    pub targets: Vec<String>,

    /// Probe protocol; `auto` starts with ICMP and, when ICMP has not reached the target after 4
    /// rounds, also traces TCP port 80, or UDP with unprivileged sockets.
    #[arg(short = 'p', long, value_enum, default_value_t = ProtocolChoice::Auto)]
    pub protocol: ProtocolChoice,

    /// Trace IPv4 addresses only.
    #[arg(short = '4', long, conflicts_with = "ipv6")]
    pub ipv4: bool,

    /// Trace IPv6 addresses only.
    #[arg(short = '6', long)]
    pub ipv6: bool,

    /// Rounds to run before a route may count as stable.
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u16).range(1..))]
    pub min_rounds: u16,

    /// Hard limit on rounds per route.
    #[arg(short = 'c', long, default_value_t = 30, value_parser = clap::value_parser!(u16).range(1..))]
    pub max_rounds: u16,

    /// Time limit per route, at least 1s and longer than `--interval`.
    #[arg(short = 't', long, default_value = "45s", value_parser = duration_at_least_1s)]
    pub time_limit: Duration,

    /// Minimum time between rounds, such as `1s` or `500ms`; at least 100ms.
    #[arg(short = 'i', long, default_value = "1s", value_parser = interval)]
    pub interval: Duration,

    /// Maximum TTL.
    #[arg(short = 'm', long, default_value_t = 30, value_parser = clap::value_parser!(u8).range(1..=254))]
    pub max_ttl: u8,

    /// Destination port for `--protocol tcp`; the TCP probes of `auto` go to port 80.
    #[arg(long, default_value_t = 443, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: u16,

    /// Routes traced at the same time; defaults to the number of routes, at most 8.
    #[arg(short = 'j', long, value_parser = clap::value_parser!(u16).range(1..=64))]
    pub parallel: Option<u16>,

    /// Use unprivileged ICMP/UDP sockets (macOS) instead of raw sockets.
    #[arg(short = 'u', long)]
    pub unprivileged: bool,

    /// Resolver for target hostnames, the upload, reverse DNS and AS lookups: `auto`, `cloudflare`,
    /// `google`, `quad9`, an IP address, or a URL such as `https://cloudflare`, `tls://dns.google`,
    /// `quic://dns.adguard-dns.com` or `tcp://9.9.9.9`.
    #[arg(long, default_value = "auto", value_name = "RESOLVER")]
    pub dns: DnsSpec,

    /// Skip reverse DNS and AS lookups.
    #[arg(long)]
    pub no_rdns: bool,

    /// Print the routes without uploading them.
    #[arg(long)]
    pub no_upload: bool,

    /// Upload the routes without the public address their probes left from, which IPMap shows as the
    /// start of each route. The first hops are still uploaded and may show the network the trace
    /// started from.
    #[arg(long)]
    pub hide_origin: bool,

    /// Protect the trace with a four-digit PIN (digits 0-8, no 4); readers enter it after opening the link.
    #[arg(long, value_name = "PIN", value_parser = pin)]
    pub pin: Option<String>,

    /// Gateway that stores uploaded traces.
    #[arg(long, env = "XMTR_API", default_value = DEFAULT_API)]
    pub api: String,

    /// Print the upload payload as JSON on stdout; the route tables go to stderr.
    #[arg(long)]
    pub json: bool,

    /// Print only the IPMap link.
    #[arg(short = 'q', long)]
    pub quiet: bool,

    /// Progress view on stderr: `panel`, a status `line` for small terminals, or `plain` text lines
    /// for logs; `auto` picks one from the terminal.
    #[arg(long, value_enum, default_value_t = ProgressChoice::Auto, value_name = "VIEW")]
    pub progress: ProgressChoice,

    /// Colors for the progress and result views; `auto` asks the terminal for its background and
    /// uses RGB colors when `COLORTERM` announces true color.
    #[arg(long, value_enum, default_value_t = ThemeChoice::Auto)]
    pub theme: ThemeChoice,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Update or remove this installation.
    #[command(name = "self", subcommand)]
    Manage(SelfCommand),
    /// List the traces uploaded from this machine, or delete one.
    Traces(TracesArgs),
}

// `xmtr traces` without a subcommand lists.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true, flatten_help = true)]
pub struct TracesArgs {
    #[command(subcommand)]
    pub command: Option<TracesCommand>,

    #[command(flatten)]
    pub list: ListArgs,
}

impl TracesArgs {
    /// True when the command deletes a trace, which takes the network.
    pub const fn deletes(&self) -> bool {
        matches!(self.command, Some(TracesCommand::Delete(_)))
    }

    /// True when the list is printed as JSON.
    pub const fn json(&self) -> bool {
        match &self.command {
            Some(TracesCommand::List(list)) => list.json,
            Some(TracesCommand::Delete(_)) => false,
            None => self.list.json,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum TracesCommand {
    /// List the uploads, newest first, with their links and delete commands (the default).
    List(ListArgs),
    /// Delete a trace from the gateway and from the list.
    Delete(DeleteArgs),
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Print the complete records as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
    /// Trace ID, IPMap link or admin link; an admin link carries the admin key.
    #[arg(value_name = "TRACE")]
    pub trace: String,

    /// Admin key of the trace, when neither the list nor an admin link has it.
    #[arg(long, value_name = "KEY")]
    pub admin_key: Option<String>,

    /// Gateway that stores the trace; defaults to the gateway the list recorded for it, then to
    /// `XMTR_API` and the default gateway.
    #[arg(long, value_name = "URL")]
    pub api: Option<String>,

    /// Resolver for the gateway, with the values `xmtr --dns` takes.
    #[arg(long, default_value = "auto", value_name = "RESOLVER")]
    pub dns: DnsSpec,
}

#[derive(Debug, Subcommand)]
pub enum SelfCommand {
    /// Replace this binary with the latest release.
    Update {
        /// Install even when this version is already the latest.
        #[arg(long)]
        force: bool,
        /// Release tag to install instead of the latest, such as `v0.2.0` or `0.2.0`.
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,
    },
    /// Delete this binary.
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProgressChoice {
    Auto,
    Panel,
    Line,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ThemeChoice {
    Auto,
    Dark,
    Light,
    DarkAnsi,
    LightAnsi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProtocolChoice {
    Auto,
    Icmp,
    Udp,
    Tcp,
}

fn duration_at_least(value: &str, min: Duration) -> Result<Duration, String> {
    let duration = humantime::parse_duration(value)
        .map_err(|error| format!("{error}; use a duration such as 45s, 500ms or 2m"))?;
    if duration < min {
        return Err(format!(
            "must be at least {}",
            humantime::format_duration(min)
        ));
    }
    Ok(duration)
}

fn duration_at_least_1s(value: &str) -> Result<Duration, String> {
    duration_at_least(value, Duration::from_secs(1))
}

fn interval(value: &str) -> Result<Duration, String> {
    duration_at_least(value, Duration::from_millis(100))
}

/// True when someone at the terminal can answer questions: stdin and stderr are terminals, and
/// neither `--json` nor `--quiet` keeps the output for programs.
pub const fn interactive(cli: &Cli, stdin_terminal: bool, stderr_terminal: bool) -> bool {
    let json = match &cli.command {
        Some(Command::Traces(traces)) => traces.json(),
        _ => cli.json,
    };
    stdin_terminal && stderr_terminal && !json && !cli.quiet
}

/// The subcommands in the order of [`Command`], each with a command that gives it options of its own.
const SUBCOMMANDS: [(&str, &str); 2] = [
    ("self", "xmtr self update --force"),
    ("traces", "xmtr traces delete --dns quad9 <TRACE>"),
];

/// Arguments that each parse on their own but do not make a run: a subcommand read as the first
/// target, or a time limit that no round fits in.
pub fn conflict(cli: &Cli) -> Option<String> {
    subcommand_as_target(cli).or_else(|| {
        // trippy reports a round only when it ends, after at least one interval.
        (cli.interval >= cli.time_limit).then(|| {
            format!(
                "--time-limit {} must be longer than --interval {}; a round takes at least one interval",
                humantime::format_duration(cli.time_limit),
                humantime::format_duration(cli.interval)
            )
        })
    })
}

/// Options in front of a subcommand make clap read the subcommand and its arguments as targets,
/// so `xmtr --dns quad9 traces delete <TRACE>` would trace hosts named `traces` and `delete`.
fn subcommand_as_target(cli: &Cli) -> Option<String> {
    let first = cli.targets.first()?;
    let (name, example) = SUBCOMMANDS.iter().find(|(name, _)| name == first)?;
    Some(format!(
        "{name} is a subcommand and must come before any option; put its options after it, such as {example}"
    ))
}

fn pin(value: &str) -> Result<String, String> {
    if value.len() == 4
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'3' | b'5'..=b'8'))
    {
        Ok(value.to_owned())
    } else {
        Err("PIN must be four digits from 0-8 without 4, such as 2580".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn accepts_pins_from_0_to_8_without_4() {
        for good in ["0000", "2580", "8888", "1235"] {
            assert_eq!(pin(good), Ok(good.to_owned()));
        }
        for bad in ["1234", "9000", "123", "12345", "12a3"] {
            assert!(pin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_durations_below_the_minimum() {
        assert_eq!(interval("500ms"), Ok(Duration::from_millis(500)));
        assert!(interval("0s").is_err());
        assert!(duration_at_least_1s("0s").is_err());
        assert!(interval("0.5").is_err());
    }

    #[test]
    fn traces_lists_by_default_and_deletes_with_a_key_and_gateway() {
        let parse = |args: &[&str]| Cli::try_parse_from([&["xmtr", "traces"], args].concat());
        let traces = |cli: Cli| match cli.command {
            Some(Command::Traces(traces)) => traces,
            command => panic!("{command:?}"),
        };
        let list = traces(parse(&[]).unwrap());
        assert!(list.command.is_none() && !list.json() && !list.deletes());
        assert!(traces(parse(&["--json"]).unwrap()).json());
        assert!(traces(parse(&["list", "--json"]).unwrap()).json());
        let delete = traces(
            parse(&[
                "delete",
                "103115083070058496-nBjPEnny",
                "--admin-key",
                "1b4e28ba-2fa1-4d2b-883f-0016d3cca427",
                "--api",
                "http://127.0.0.1:8787",
                "--dns",
                "quad9",
            ])
            .unwrap(),
        );
        assert!(delete.deletes() && !delete.json());
        let Some(TracesCommand::Delete(args)) = delete.command else {
            panic!("{delete:?}");
        };
        assert_eq!(args.trace, "103115083070058496-nBjPEnny");
        assert_eq!(
            args.admin_key.as_deref(),
            Some("1b4e28ba-2fa1-4d2b-883f-0016d3cca427")
        );
        assert_eq!(args.api.as_deref(), Some("http://127.0.0.1:8787"));
        assert_eq!(args.dns, "quad9".parse().unwrap());
        assert!(parse(&["--dns", "quad9"]).is_err());
        assert!(parse(&["--json", "delete", "1"]).is_err());
        assert!(parse(&["delete"]).is_err());
    }

    #[test]
    fn rejects_a_subcommand_given_as_the_first_target() {
        let command = Cli::command();
        let names: Vec<&str> = command
            .get_subcommands()
            .map(clap::Command::get_name)
            .collect();
        assert_eq!(names, SUBCOMMANDS.map(|(name, _)| name));
        let parse = |args: &[&str]| Cli::try_parse_from([&["xmtr"], args].concat()).unwrap();
        let traces = parse(&["--dns", "quad9", "traces", "delete", "1-a"]);
        assert!(traces.command.is_none());
        assert_eq!(
            conflict(&traces).as_deref(),
            Some(
                "traces is a subcommand and must come before any option; put its options after it, such as xmtr traces delete --dns quad9 <TRACE>"
            )
        );
        let manage = conflict(&parse(&["--no-rdns", "self", "update"])).unwrap();
        assert!(manage.starts_with("self is a subcommand"), "{manage}");
        // Only the first target can be a misplaced subcommand, and a subcommand in its place parses.
        assert_eq!(conflict(&parse(&["--no-rdns", "1.1.1.1", "traces"])), None);
        assert_eq!(conflict(&parse(&["traces", "delete", "1-a"])), None);
    }

    #[test]
    fn rejects_a_time_limit_that_fits_no_round() {
        let parse = |args: &[&str]| Cli::try_parse_from([&["xmtr"], args, &["192.0.2.1"]].concat());
        for args in [
            &["-i", "10s", "-t", "2s"][..],
            &["-i", "2s", "-t", "2s"],
            &["-t", "1s"],
        ] {
            assert!(conflict(&parse(args).unwrap()).is_some(), "{args:?}");
        }
        assert_eq!(
            conflict(&parse(&["-i", "500ms", "-t", "1s"]).unwrap()),
            None
        );
        assert_eq!(conflict(&parse(&[]).unwrap()), None);
    }
}
