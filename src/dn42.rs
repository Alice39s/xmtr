use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use dn42_ipdb::{Bundle, Cache, IpLookup, Observation};
use serde::Deserialize;
use serde_json::Value;

use crate::cli::{Cli, Mode};
use crate::model::Route;
use crate::user_dir::{self, Kind};

/// A release is about 30 MB; the limit covers the wait for another update and every download.
const REFRESH_LIMIT: Duration = Duration::from_secs(300);

/// True for a host name under `.dn42`, which reaches DN42 and carries a certificate of the DN42 CA.
pub fn is_dn42_name(host: &str) -> bool {
    host.trim_end_matches('.')
        .rsplit_once('.')
        .is_some_and(|(name, tld)| !name.is_empty() && tld.eq_ignore_ascii_case("dn42"))
}

pub fn open(cli: &Cli) -> Result<Option<Bundle>> {
    if cli.mode != Mode::Dn42 || cli.no_rdns {
        return Ok(None);
    }
    if let Some(directory) = &cli.dn42_db {
        return Bundle::open(directory)
            .with_context(|| format!("opening DN42 bundle {}", directory.display()))
            .map(Some);
    }
    let root = user_dir::dir(Kind::Cache)?
        .context("root has no home directory in the user database; pass --dn42-db DIRECTORY")?
        .join("xmtr")
        .join("dn42-ipdb");
    let cache = Cache::new(&root)
        .and_then(|cache| cache.with_timeout(REFRESH_LIMIT))
        .context("configuring the DN42 cache")?;
    let bundle = if cli.dn42_refresh {
        cache
            .refresh()
            .context("refreshing the DN42 database")?
            .bundle
    } else {
        cache.open_cached().with_context(|| {
            format!(
                "opening DN42 cache {}; use --dn42-refresh to download a verified bundle or --dn42-db DIRECTORY",
                root.display()
            )
        })?
    };
    Ok(Some(bundle))
}

#[derive(Default)]
struct Names {
    asn: Option<u32>,
    hostname: Option<String>,
}

/// The origin ASN and the PTR name of `found.ip`. Each field is read on its own, so a record
/// in another shape leaves only the fields it cannot fill empty.
fn names(found: IpLookup) -> Names {
    let mut names = Names::default();
    for (key, matched) in found.records {
        if key == "asn" {
            names.asn = matched
                .data
                .get("autonomous_system_number")
                .and_then(Value::as_u64)
                .and_then(|asn| u32::try_from(asn).ok());
        }
        if names.hostname.is_none() {
            names.hostname = matched
                .data
                .pointer("/dn42/observations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|observation| ptr_name(observation, found.ip));
        }
    }
    names
}

/// The name of an observed PTR record for exactly `ip`.
fn ptr_name(observation: &Value, ip: IpAddr) -> Option<String> {
    let observation = Observation::deserialize(observation).ok()?;
    let target = observation.target.parse::<IpAddr>().ok()?.to_canonical();
    if observation.kind != "ptr" || observation.status != "observed" || target != ip {
        return None;
    }
    let name = observation.value?;
    let name = name.trim_end_matches('.');
    (!name.is_empty()).then(|| name.to_owned())
}

pub fn annotate(
    bundle: &Bundle,
    routes: &mut [Route],
    progress: impl Fn(usize, usize),
) -> Result<()> {
    let mut addresses: Vec<IpAddr> = routes
        .iter()
        .flat_map(|route| &route.hops)
        .flat_map(|hop| &hop.hosts)
        .map(|host| {
            host.ip
                .parse()
                .with_context(|| format!("reading hop address {}", host.ip))
        })
        .collect::<Result<_>>()?;
    addresses.sort_unstable();
    addresses.dedup();
    let total = addresses.len();
    let mut annotations = HashMap::new();
    progress(0, total);
    for ip in addresses {
        let found = bundle
            .lookup_ip(ip)
            .with_context(|| format!("looking up {ip} in DN42 IPDB"))?;
        annotations.insert(ip, names(found));
        progress(annotations.len(), total);
    }
    for host in routes
        .iter_mut()
        .flat_map(|route| &mut route.hops)
        .flat_map(|hop| &mut hop.hosts)
    {
        let ip = host
            .ip
            .parse::<IpAddr>()
            .expect("hop addresses were validated above");
        let found = &annotations[&ip];
        host.asn = found.asn;
        host.hostname.clone_from(&found.hostname);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use dn42_ipdb::Match;
    use serde_json::json;

    fn fixtures() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/dn42")
    }

    #[test]
    fn reads_real_mmdb_records_for_both_families() {
        let bundle = Bundle::open(fixtures()).unwrap();
        for ip in ["172.20.0.1", "fd42:1234::1", "::ffff:172.20.0.1"] {
            let found = names(bundle.lookup_ip(ip.parse().unwrap()).unwrap());
            assert_eq!(found.asn, Some(4_242_420_001), "{ip}");
            assert_eq!(
                found.hostname.as_deref(),
                Some("resolver.test.dn42"),
                "{ip}"
            );
        }
        for ip in ["172.20.0.2", "fd42:1234::2"] {
            let found = names(bundle.lookup_ip(ip.parse().unwrap()).unwrap());
            assert_eq!(found.asn, Some(4_242_420_001));
            assert!(
                found.hostname.is_none(),
                "a prefix must not inherit another host's PTR"
            );
        }
        let absent = names(bundle.lookup_ip("203.0.113.1".parse().unwrap()).unwrap());
        assert!(absent.asn.is_none() && absent.hostname.is_none());
    }

    #[test]
    fn annotates_repeated_hops_and_reports_each_address_once() {
        use crate::model::{Hop, Host};
        use std::cell::RefCell;

        let hosts = ["172.20.0.1", "fd42:1234::1", "172.20.0.1", "203.0.113.1"]
            .map(|ip| Host {
                ip: ip.to_owned(),
                hostname: None,
                asn: None,
                replies: 1,
                best_ms: Some(1.0),
                mpls: Vec::new(),
            })
            .to_vec();
        let mut routes = [Route {
            target: "172.20.0.1".to_owned(),
            destination: "172.20.0.1".to_owned(),
            protocol: "icmp",
            family: 4,
            source_addr: None,
            started_at: String::new(),
            duration_ms: 1000,
            rounds: 1,
            reached: true,
            hops: vec![Hop {
                ttl: 1,
                hosts,
                sent: 1,
                recv: 1,
                loss_pct: 0.0,
                last_ms: Some(1.0),
                avg_ms: Some(1.0),
                best_ms: Some(1.0),
                worst_ms: Some(1.0),
                stdev_ms: Some(0.0),
                samples_ms: vec![1.0],
            }],
        }];
        let progress = RefCell::new(Vec::new());
        annotate(
            &Bundle::open(fixtures()).unwrap(),
            &mut routes,
            |done, total| {
                progress.borrow_mut().push((done, total));
            },
        )
        .unwrap();
        assert_eq!(*progress.borrow(), [(0, 3), (1, 3), (2, 3), (3, 3)]);
        for host in &routes[0].hops[0].hosts[..3] {
            assert_eq!(host.asn, Some(4_242_420_001));
            assert_eq!(host.hostname.as_deref(), Some("resolver.test.dn42"));
        }
        assert!(routes[0].hops[0].hosts[3].asn.is_none());
        assert!(routes[0].hops[0].hosts[3].hostname.is_none());
        let json = serde_json::to_value(&routes[0]).unwrap();
        assert_eq!(json["hops"][0]["hosts"][0]["asn"], 4_242_420_001u32);
        assert_eq!(
            json["hops"][0]["hosts"][0]["hostname"],
            "resolver.test.dn42"
        );
    }

    #[test]
    fn names_under_dn42_are_dn42_names() {
        for name in ["xmtr.ip-api.dn42", "XMTR.IP-API.DN42.", "hat.dn42"] {
            assert!(is_dn42_name(name), "{name}");
        }
        for name in [
            "dn42",
            ".dn42",
            "dn42.example",
            "example.dn42x",
            "172.20.0.53",
            "",
        ] {
            assert!(!is_dn42_name(name), "{name}");
        }
    }

    #[test]
    fn fields_in_another_shape_stay_empty() {
        let record = |data| Match {
            network: "172.20.0.0/24".to_owned(),
            prefix_length: 24,
            data,
        };
        let ptr = json!({
            "target": "172.20.0.1",
            "kind": "ptr",
            "value": "router.test.dn42.",
            "status": "observed",
            "method": "dns",
            "source": "registry",
            "authority": "test.dn42",
            "observed_at": "2026-11-17T00:00:00Z",
        });
        let found = IpLookup {
            ip: "172.20.0.1".parse().unwrap(),
            records: [
                (
                    "asn".to_owned(),
                    record(json!({
                        "autonomous_system_number": "invalid",
                        "dn42": {"observations": "invalid"},
                    })),
                ),
                (
                    "city".to_owned(),
                    record(json!({
                        "dn42": {"observations": [{"target": "172.20.0.1", "kind": "ptr"}, ptr]},
                    })),
                ),
            ]
            .into(),
        };
        let names = names(found);
        assert_eq!(names.asn, None);
        assert_eq!(names.hostname.as_deref(), Some("router.test.dn42"));
    }

    #[test]
    fn bundle_is_loaded_only_for_dn42_annotations() {
        for args in [
            vec!["xmtr", "127.0.0.1"],
            vec!["xmtr", "--mode", "dn42", "--no-rdns", "127.0.0.1"],
        ] {
            assert!(open(&Cli::try_parse_from(args).unwrap()).unwrap().is_none());
        }
        let mut cli = Cli::try_parse_from(["xmtr", "--mode", "dn42", "127.0.0.1"]).unwrap();
        cli.dn42_db = Some(fixtures());
        assert!(open(&cli).unwrap().is_some());
        cli.dn42_db = Some(fixtures().join("missing"));
        assert!(open(&cli).is_err());
    }
}
