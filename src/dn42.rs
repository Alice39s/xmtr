use std::collections::HashMap;
use std::net::IpAddr;

use anyhow::{Context, Result};
use dn42_ipdb::{Bundle, Cache, IpLookup, Observation};
use serde::Deserialize;

use crate::cli::{Cli, Mode};
use crate::model::Route;

pub fn open(cli: &Cli) -> Result<Option<Bundle>> {
    if cli.mode != Mode::Dn42 || cli.no_rdns {
        return Ok(None);
    }
    if let Some(directory) = &cli.dn42_db {
        return Bundle::open(directory)
            .with_context(|| format!("opening DN42 bundle {}", directory.display()))
            .map(Some);
    }
    let root = dirs::cache_dir()
        .context("locating the cache directory; pass --dn42-db DIRECTORY")?
        .join("xmtr/dn42-ipdb");
    let cache = Cache::new(&root).context("configuring the DN42 cache")?;
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

#[derive(Default, Deserialize)]
struct Extension {
    #[serde(default)]
    observations: Vec<Observation>,
}

#[derive(Deserialize)]
struct Record {
    autonomous_system_number: Option<u32>,
    #[serde(default)]
    dn42: Extension,
}

#[derive(Default)]
struct Names {
    asn: Option<u32>,
    hostname: Option<String>,
}

fn names(found: IpLookup) -> Result<Names> {
    let mut names = Names::default();
    for (key, matched) in found.records {
        let record: Record = serde_json::from_value(matched.data)
            .with_context(|| format!("reading DN42 {key} record for {}", found.ip))?;
        if key == "asn" {
            names.asn = record.autonomous_system_number;
        }
        if names.hostname.is_none() {
            names.hostname = record.dn42.observations.iter().find_map(|observation| {
                (observation.kind == "ptr"
                    && observation.status == "observed"
                    && observation
                        .target
                        .parse::<IpAddr>()
                        .ok()
                        .map(|ip| ip.to_canonical())
                        == Some(found.ip))
                .then_some(observation.value.as_deref())
                .flatten()
                .map(|name| name.trim_end_matches('.'))
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
            });
        }
    }
    Ok(names)
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
        annotations.insert(ip, names(found)?);
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
            let found = names(bundle.lookup_ip(ip.parse().unwrap()).unwrap()).unwrap();
            assert_eq!(found.asn, Some(4_242_420_001), "{ip}");
            assert_eq!(
                found.hostname.as_deref(),
                Some("resolver.test.dn42"),
                "{ip}"
            );
        }
        for ip in ["172.20.0.2", "fd42:1234::2"] {
            let found = names(bundle.lookup_ip(ip.parse().unwrap()).unwrap()).unwrap();
            assert_eq!(found.asn, Some(4_242_420_001));
            assert!(
                found.hostname.is_none(),
                "a prefix must not inherit another host's PTR"
            );
        }
        let absent = names(bundle.lookup_ip("203.0.113.1".parse().unwrap()).unwrap()).unwrap();
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
    fn invalid_asn_record_is_an_error() {
        let found = IpLookup {
            ip: "172.20.0.1".parse().unwrap(),
            records: [(
                "asn".to_owned(),
                Match {
                    network: "172.20.0.0/24".to_owned(),
                    prefix_length: 24,
                    data: json!({"autonomous_system_number": "invalid"}),
                },
            )]
            .into(),
        };
        assert!(names(found).is_err());
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
