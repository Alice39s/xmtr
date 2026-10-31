use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

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
    stdin_terminal && stderr_terminal && !cli.json && !cli.quiet
}

/// Option combinations that each parse on their own but cannot produce a route together.
pub fn conflict(cli: &Cli) -> Option<String> {
    // trippy reports a round only when it ends, after at least one interval.
    (cli.interval >= cli.time_limit).then(|| {
        format!(
            "--time-limit {} must be longer than --interval {}; a round takes at least one interval",
            humantime::format_duration(cli.time_limit),
            humantime::format_duration(cli.interval)
        )
    })
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
