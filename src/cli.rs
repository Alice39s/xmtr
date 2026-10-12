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

    /// Probe protocol; `auto` starts with ICMP and adds TCP or UDP when ICMP does not reach the target.
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

    /// Time limit per route.
    #[arg(short = 't', long, default_value = "45s", value_parser = humantime::parse_duration)]
    pub time_limit: Duration,

    /// Minimum time between rounds.
    #[arg(short = 'i', long, default_value = "1s", value_parser = humantime::parse_duration)]
    pub interval: Duration,

    /// Maximum TTL.
    #[arg(short = 'm', long, default_value_t = 30, value_parser = clap::value_parser!(u8).range(1..=254))]
    pub max_ttl: u8,

    /// Destination port for TCP probes.
    #[arg(long, default_value_t = 443)]
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

    /// Do not attach this machine's public address as the route origin.
    #[arg(long)]
    pub hide_origin: bool,

    /// Gateway that stores uploaded traces.
    #[arg(long, env = "XMTR_API", default_value = DEFAULT_API)]
    pub api: String,

    /// Print the upload payload as JSON on stdout.
    #[arg(long)]
    pub json: bool,

    /// Print only the IPMap link.
    #[arg(short = 'q', long)]
    pub quiet: bool,
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
        /// Release tag to install instead of the latest, such as `v0.2.0`.
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,
    },
    /// Delete this binary.
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProtocolChoice {
    Auto,
    Icmp,
    Udp,
    Tcp,
}
