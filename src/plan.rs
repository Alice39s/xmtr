use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr, UdpSocket};

use anyhow::{Context, Result, bail};
use trippy_core::{PrivilegeMode, Protocol};
use trippy_privilege::Privilege;

use crate::cli::{Cli, ProtocolChoice};
use crate::dns::Dns;

/// One (target, address, protocol) combination to trace.
#[derive(Debug, Clone)]
pub struct Job {
    pub target: String,
    pub addr: IpAddr,
    pub protocol: Protocol,
    /// Protocol to try next when this one does not reach the target.
    pub fallback: Option<Protocol>,
}

#[derive(Debug)]
pub struct Plan {
    pub jobs: Vec<Job>,
    pub privilege: PrivilegeMode,
    pub skipped: Vec<String>,
}

/// Picks raw sockets when allowed. Platforms whose ICMP datagram sockets can set the IP header
/// (macOS) also run unprivileged; elsewhere trippy needs root, Administrator or `CAP_NET_RAW`.
pub fn privilege_mode(cli: &Cli) -> Result<PrivilegeMode> {
    let privilege = Privilege::acquire_privileges().context("checking raw socket privileges")?;
    let unprivileged_supported = !privilege.needs_privileges();
    if cli.unprivileged {
        if !unprivileged_supported {
            bail!(
                "unprivileged mode is not supported on this platform; run xmtr as root or Administrator"
            );
        }
        return Ok(PrivilegeMode::Unprivileged);
    }
    if privilege.has_privileges() {
        return Ok(PrivilegeMode::Privileged);
    }
    if unprivileged_supported {
        return Ok(PrivilegeMode::Unprivileged);
    }
    bail!(
        "raw sockets need elevated privileges: run xmtr as root or Administrator \
         (on Linux `sudo setcap cap_net_raw+ep $(which xmtr)` also works)"
    )
}

/// True when the OS has a route to `addr`; connecting a UDP socket sends no packet.
fn has_route(addr: IpAddr) -> bool {
    let bind: SocketAddr = match addr {
        IpAddr::V4(_) => (IpAddr::from([0, 0, 0, 0]), 0).into(),
        IpAddr::V6(_) => (IpAddr::from([0u16; 8]), 0).into(),
    };
    UdpSocket::bind(bind)
        .and_then(|socket| socket.connect((addr, 33434)))
        .is_ok()
}

/// First IPv4 and first IPv6 address of `target`.
fn resolve(target: &str, dns: &Dns) -> Result<Vec<IpAddr>> {
    let addrs: BTreeSet<IpAddr> = dns.resolve(target)?.into_iter().collect();
    let v4 = addrs.iter().find(|addr| addr.is_ipv4()).copied();
    let v6 = addrs.iter().find(|addr| addr.is_ipv6()).copied();
    Ok([v4, v6].into_iter().flatten().collect())
}

fn protocols(
    choice: ProtocolChoice,
    privilege: PrivilegeMode,
) -> Result<(Protocol, Option<Protocol>)> {
    let unprivileged = privilege == PrivilegeMode::Unprivileged;
    Ok(match choice {
        ProtocolChoice::Auto if unprivileged => (Protocol::Icmp, Some(Protocol::Udp)),
        ProtocolChoice::Auto => (Protocol::Icmp, Some(Protocol::Tcp)),
        ProtocolChoice::Icmp => (Protocol::Icmp, None),
        ProtocolChoice::Udp => (Protocol::Udp, None),
        ProtocolChoice::Tcp if unprivileged => {
            bail!("TCP tracing needs raw sockets; run as root or drop --unprivileged")
        }
        ProtocolChoice::Tcp => (Protocol::Tcp, None),
    })
}

/// Expands targets into jobs: hostnames with A and AAAA records are traced over both families at once,
/// families without a local route are skipped.
pub fn build(cli: &Cli, dns: &Dns) -> Result<Plan> {
    let privilege = privilege_mode(cli)?;
    let (protocol, fallback) = protocols(cli.protocol, privilege)?;
    let mut jobs = Vec::new();
    let mut skipped = Vec::new();

    for target in &cli.targets {
        let addrs = match resolve(target, dns) {
            Ok(addrs) => addrs,
            Err(error) => {
                skipped.push(format!("{error:#}"));
                continue;
            }
        };
        let wanted: Vec<IpAddr> = addrs
            .into_iter()
            .filter(|addr| (!cli.ipv4 || addr.is_ipv4()) && (!cli.ipv6 || addr.is_ipv6()))
            .collect();
        if wanted.is_empty() {
            skipped.push(format!("{target}: no address in the requested family"));
            continue;
        }
        for addr in wanted {
            if !has_route(addr) {
                skipped.push(format!("{target} ({addr}): no route on this machine"));
                continue;
            }
            jobs.push(Job {
                target: target.clone(),
                addr,
                protocol,
                fallback,
            });
        }
    }
    if jobs.is_empty() {
        bail!("nothing to trace: {}", skipped.join("; "));
    }
    Ok(Plan {
        jobs,
        privilege,
        skipped,
    })
}
