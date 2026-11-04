use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Result, anyhow};
use trippy_core::{
    Builder, Extension, Extensions, IcmpExtensionParseMode, MultipathStrategy, Port, PortDirection,
    PrivilegeMode, ProbeStatus, Protocol, Round, State,
};

use crate::board::{Board, Stage};
use crate::convergence::Convergence;
use crate::model;
use crate::plan::Job;

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const MAX_SAMPLES: usize = 64;
const UDP_SOURCE_PORT_BASE: u16 = 41000;
const SEQUENCE_BLOCK: u16 = 1536;
/// Destination port of the TCP fallback; `--port` applies to `--protocol tcp`.
pub const FALLBACK_TCP_PORT: u16 = 80;
/// Rounds the first protocol has to reach the target before its fallback starts beside it.
const FALLBACK_AFTER_ROUNDS: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub privilege: PrivilegeMode,
    pub min_rounds: usize,
    pub max_rounds: usize,
    pub time_limit: Duration,
    pub interval: Duration,
    pub max_ttl: u8,
    /// Destination port of `--protocol tcp`.
    pub tcp_port: u16,
}

#[derive(Debug)]
pub struct Outcome {
    pub label: String,
    pub result: Result<model::Route>,
}

/// One tracer to run: the first protocol of a job, or the fallback started beside it.
#[derive(Debug, Clone, Copy)]
struct Probe {
    job: usize,
    protocol: Protocol,
    fallback: bool,
    /// Board row the probe reports to.
    row: usize,
}

pub const fn protocol_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Icmp => "icmp",
        Protocol::Udp => "udp",
        Protocol::Tcp => "tcp",
    }
}

fn job_label(job: &Job, protocol: Protocol) -> String {
    let family = if job.addr.is_ipv4() { "IPv4" } else { "IPv6" };
    if job.target == job.addr.to_string() {
        format!(
            "{} {family}/{}",
            job.target,
            protocol_name(protocol).to_uppercase()
        )
    } else {
        format!(
            "{} ({}) {family}/{}",
            job.target,
            job.addr,
            protocol_name(protocol).to_uppercase()
        )
    }
}

/// Rounds the tracer itself runs. trippy cannot be stopped from outside, so a tracer that outlives its
/// route keeps probing until this many rounds; capping it at what fits in the time limit keeps such
/// leftovers short.
fn tracer_rounds(settings: &Settings) -> usize {
    let interval = settings.interval.max(Duration::from_millis(100));
    let fit = settings.time_limit.as_nanos().div_ceil(interval.as_nanos()) + 1;
    settings
        .max_rounds
        .min(usize::try_from(fit).unwrap_or(usize::MAX))
}

/// Classic UDP probes change the destination port with every probe, so a router that balances
/// flows over parallel links sends each hop's probe down a different link and the route mixes
/// hops of several paths. Dublin keeps every probe of a round on one flow and identifies probes
/// by the IPv4 identifier or the IPv6 payload length, so NAT checksum changes do not hide replies.
/// Setting those fields needs raw sockets.
/// trippy has no flow-stable mode for ICMP or TCP.
const fn multipath_strategy(protocol: Protocol, privilege: PrivilegeMode) -> MultipathStrategy {
    match (protocol, privilege) {
        (Protocol::Udp, PrivilegeMode::Privileged) => MultipathStrategy::Dublin,
        _ => MultipathStrategy::Classic,
    }
}

/// Every concurrent tracer gets its own ICMP identifier, UDP source port and sequence range,
/// so replies are never attributed to the wrong route.
fn build_tracer(
    job: &Job,
    probe: &Probe,
    slot: u16,
    settings: &Settings,
) -> Result<trippy_core::Tracer> {
    let identifier = u16::try_from(std::process::id() % u32::from(u16::MAX))
        .unwrap_or(1)
        .wrapping_mul(97)
        .wrapping_add(slot.wrapping_mul(13))
        .max(1);
    let port_direction = match probe.protocol {
        Protocol::Icmp => PortDirection::None,
        Protocol::Udp => PortDirection::FixedSrc(Port(UDP_SOURCE_PORT_BASE + slot)),
        Protocol::Tcp if probe.fallback => PortDirection::FixedDest(Port(FALLBACK_TCP_PORT)),
        Protocol::Tcp => PortDirection::FixedDest(Port(settings.tcp_port)),
    };
    Builder::new(job.addr)
        .multipath_strategy(multipath_strategy(probe.protocol, settings.privilege))
        .privilege_mode(settings.privilege)
        .protocol(probe.protocol)
        .trace_identifier(identifier)
        .port_direction(port_direction)
        .initial_sequence(33434 + (slot % 20) * SEQUENCE_BLOCK)
        .max_rounds(Some(tracer_rounds(settings)))
        .max_ttl(settings.max_ttl)
        .min_round_duration(settings.interval)
        .max_round_duration(settings.interval.max(Duration::from_millis(1000)))
        // RFC 4950 label stacks show which hops forwarded the probe inside an MPLS tunnel, where
        // every hop reports the round trip through the end of the tunnel.
        .icmp_extension_parse_mode(IcmpExtensionParseMode::Enabled)
        .build()
        .map_err(|error| anyhow!("{error}"))
}

/// What the replies of one host at one TTL said: trippy keeps the round trips and the ICMP
/// extensions per TTL only, mixing the hosts that answered there.
#[derive(Debug, Default, Clone, PartialEq)]
struct HostReplies {
    best: Option<Duration>,
    mpls: Vec<model::MplsLabel>,
}

type HostLog = HashMap<(u8, IpAddr), HostReplies>;

/// The core state and address metadata published by the same completed round.
struct CompletedTrace {
    state: State,
    replies: HostLog,
}

impl CompletedTrace {
    fn new(state: State) -> Self {
        Self {
            state,
            replies: HostLog::new(),
        }
    }

    fn publish(&mut self, state: State, round: &Round<'_>) {
        record_round(&mut self.replies, round);
        self.state = state;
    }

    fn route(
        &self,
        job: &Job,
        protocol: Protocol,
        started: SystemTime,
        elapsed: Duration,
    ) -> model::Route {
        route_from_state(
            job,
            protocol,
            &self.state,
            &self.replies,
            self.state.round_count(State::default_flow_id()),
            started,
            elapsed,
        )
    }
}

/// The MPLS label stack in a reply's ICMP extensions, top entry first.
fn mpls_stack(extensions: &Extensions) -> Option<Vec<model::MplsLabel>> {
    extensions
        .extensions
        .iter()
        .find_map(|extension| match extension {
            Extension::Mpls(stack) => Some(
                stack
                    .members
                    .iter()
                    .map(|member| model::MplsLabel {
                        label: member.label,
                        tc: member.exp,
                        bottom: member.bos != 0,
                        ttl: member.ttl,
                    })
                    .collect(),
            ),
            Extension::Unknown(_) => None,
        })
}

/// Adds the replies of a finished round to `log`.
fn record_round(log: &mut HostLog, round: &Round<'_>) {
    for probe in round.probes {
        let ProbeStatus::Complete(reply) = probe else {
            continue;
        };
        let entry = log.entry((reply.ttl.0, reply.host)).or_default();
        if let Ok(rtt) = reply.received.duration_since(reply.sent) {
            entry.best = Some(entry.best.map_or(rtt, |best| best.min(rtt)));
        }
        if let Some(stack) = reply.extensions.as_ref().and_then(mpls_stack) {
            entry.mpls = stack;
        }
    }
}

fn responders(state: &State) -> Vec<(u8, IpAddr)> {
    state
        .hops()
        .iter()
        .flat_map(|hop| hop.addrs().map(move |addr| (hop.ttl(), *addr)))
        .collect()
}

fn reached(state: &State, target: IpAddr) -> bool {
    state
        .hops()
        .iter()
        .any(|hop| hop.addrs().any(|addr| *addr == target))
}

fn round_ms(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Converts trippy's per-hop state and the per-host replies into the upload model, stopping at the
/// hop that answered for the target.
fn route_from_state(
    job: &Job,
    protocol: Protocol,
    state: &State,
    replies: &HostLog,
    rounds: usize,
    started: SystemTime,
    elapsed: Duration,
) -> model::Route {
    let target_ttl = state
        .hops()
        .iter()
        .find(|hop| hop.addrs().any(|addr| *addr == job.addr))
        .map(trippy_core::Hop::ttl);
    let hops = state
        .hops()
        .iter()
        .filter(|hop| hop.total_sent() > 0 && target_ttl.is_none_or(|ttl| hop.ttl() <= ttl))
        .map(|hop| {
            let answered = hop.total_recv() > 0;
            let mut samples: Vec<f64> = hop
                .samples()
                .iter()
                .filter(|sample| !sample.is_zero())
                .take(MAX_SAMPLES)
                .map(|sample| round_ms(sample.as_secs_f64() * 1000.0))
                .collect();
            samples.reverse();
            model::Hop {
                ttl: hop.ttl(),
                hosts: hop
                    .addrs_with_counts()
                    .map(|(addr, count)| {
                        let host = replies.get(&(hop.ttl(), *addr));
                        model::Host {
                            ip: addr.to_string(),
                            hostname: None,
                            asn: None,
                            replies: *count,
                            best_ms: host
                                .and_then(|host| host.best)
                                .map(|best| round_ms(best.as_secs_f64() * 1000.0)),
                            mpls: host.map(|host| host.mpls.clone()).unwrap_or_default(),
                        }
                    })
                    .collect(),
                sent: hop.total_sent(),
                recv: hop.total_recv(),
                loss_pct: (hop.loss_pct() * 100.0).round() / 100.0,
                last_ms: hop.last_ms().map(round_ms),
                avg_ms: answered.then(|| round_ms(hop.avg_ms())),
                best_ms: hop.best_ms().map(round_ms),
                worst_ms: hop.worst_ms().map(round_ms),
                stdev_ms: answered.then(|| round_ms(hop.stddev_ms())),
                samples_ms: samples,
            }
        })
        .collect();
    model::Route {
        target: job.target.clone(),
        destination: job.addr.to_string(),
        protocol: protocol_name(protocol),
        family: if job.addr.is_ipv4() { 4 } else { 6 },
        source_addr: None,
        started_at: humantime::format_rfc3339_millis(started).to_string(),
        duration_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        rounds,
        reached: target_ttl.is_some(),
        hops,
    }
}

/// Runs one tracer until the path is stable, the round limit or the time limit is hit, telling
/// `watch` the completed rounds and whether the target answered after every poll.
fn trace(
    job: &Job,
    probe: &Probe,
    slot: u16,
    settings: &Settings,
    board: &Board,
    watch: &mut dyn FnMut(usize, bool),
) -> Result<model::Route> {
    let started = SystemTime::now();
    let clock = Instant::now();
    let tracer = build_tracer(job, probe, slot, settings)?;
    let completed = Arc::new(Mutex::new(CompletedTrace::new(tracer.snapshot())));
    let published = Arc::clone(&completed);
    let core = tracer.clone();
    let (tracer, handle) = tracer
        .spawn_with(move |round| {
            let state = core.snapshot();
            published
                .lock()
                .expect("completed trace snapshot lock poisoned while publishing a round")
                .publish(state, round);
        })
        .map_err(|error| anyhow!("{error}"))?;
    let flow = State::default_flow_id();
    let mut convergence = Convergence::default();

    loop {
        thread::sleep(POLL_INTERVAL);
        if let Some(error) = tracer.snapshot().error() {
            return Err(anyhow!("{error}"));
        }
        let (rounds, is_reached, hosts) = {
            let completed = completed.lock().map_err(|_| {
                anyhow!("completed trace snapshot lock poisoned while reading progress")
            })?;
            let state = &completed.state;
            (
                state.round_count(flow),
                reached(state, job.addr),
                responders(state),
            )
        };
        convergence.observe(rounds, hosts);
        board.update(probe.row, |row| {
            row.rounds = rounds;
            row.reached = is_reached;
        });
        watch(rounds, is_reached);
        let finished = handle.is_finished();
        if finished
            || convergence.is_stable(settings.min_rounds, is_reached)
            || clock.elapsed() >= settings.time_limit
        {
            if finished {
                handle
                    .join()
                    .map_err(|_| anyhow!("tracer thread panicked"))?
                    .map_err(|error| anyhow!("{error}"))?;
            }
            // A tracer that is still running ends on its own after `tracer_rounds` rounds.
            if let Some(error) = tracer.snapshot().error() {
                return Err(anyhow!("{error}"));
            }
            let completed = completed.lock().map_err(|_| {
                anyhow!("completed trace snapshot lock poisoned while building the route")
            })?;
            return Ok(completed.route(job, probe.protocol, started, clock.elapsed()));
        }
    }
}

/// Routes `jobs` can produce: one per job and one more for each job with a fallback.
pub fn route_count(jobs: &[Job]) -> usize {
    jobs.iter()
        .map(|job| 1 + usize::from(job.fallback.is_some()))
        .sum()
}

/// Probes waiting for a tracer, fallbacks first, and the number of probes running.
#[derive(Debug, Default)]
struct Queue {
    fallbacks: VecDeque<Probe>,
    first: VecDeque<Probe>,
    running: usize,
}

/// Hands probes to the workers. A worker with nothing to take waits while a running probe may still
/// add its fallback.
#[derive(Debug)]
struct Scheduler {
    queue: Mutex<Queue>,
    changed: Condvar,
}

/// A probe taken from the scheduler. Dropping it, also while its worker panics, frees its place.
struct Lease<'a> {
    probe: Probe,
    scheduler: &'a Scheduler,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        self.scheduler.lock().running -= 1;
        self.scheduler.changed.notify_all();
    }
}

impl Scheduler {
    fn new(first: impl IntoIterator<Item = Probe>) -> Self {
        Self {
            queue: Mutex::new(Queue {
                first: first.into_iter().collect(),
                ..Queue::default()
            }),
            changed: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The next probe, or `None` once nothing waits and nothing runs.
    fn next(&self) -> Option<Lease<'_>> {
        let mut queue = self.lock();
        loop {
            if let Some(probe) = queue
                .fallbacks
                .pop_front()
                .or_else(|| queue.first.pop_front())
            {
                queue.running += 1;
                return Some(Lease {
                    probe,
                    scheduler: self,
                });
            }
            if queue.running == 0 {
                return None;
            }
            queue = self
                .changed
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Queues a fallback ahead of the jobs that have not started.
    fn add_fallback(&self, probe: Probe) {
        self.lock().fallbacks.push_back(probe);
        self.changed.notify_one();
    }
}

/// Traces all jobs with up to `parallel` tracers at once. When the first protocol of a job has not
/// reached the target after `FALLBACK_AFTER_ROUNDS` rounds, or ends without reaching it, the
/// fallback protocol is traced beside it, and both routes are kept.
pub fn run_all(jobs: &[Job], parallel: usize, settings: &Settings, board: &Board) -> Vec<Outcome> {
    let slots = AtomicUsize::new(0);
    schedule(jobs, parallel, board, |probe, watch| {
        // Every tracer takes a new slot: one that outlives its route keeps probing with it.
        let slot = u16::try_from(slots.fetch_add(1, Ordering::SeqCst) % 1000).unwrap_or(0);
        trace(&jobs[probe.job], probe, slot, settings, board, watch)
    })
}

/// Runs the probes of `jobs` with `trace` on up to `parallel` workers, adding fallbacks as
/// `run_all` describes.
fn schedule(
    jobs: &[Job],
    parallel: usize,
    board: &Board,
    trace: impl Fn(&Probe, &mut dyn FnMut(usize, bool)) -> Result<model::Route> + Sync,
) -> Vec<Outcome> {
    let scheduler = Scheduler::new(jobs.iter().enumerate().map(|(index, job)| Probe {
        job: index,
        protocol: job.protocol,
        fallback: false,
        row: index,
    }));
    let outcomes = Mutex::new(Vec::new());

    thread::scope(|scope| {
        for _ in 0..parallel.min(route_count(jobs)).max(1) {
            scope.spawn(|| {
                while let Some(lease) = scheduler.next() {
                    let probe = lease.probe;
                    let job = &jobs[probe.job];
                    let mut fallback = job.fallback.filter(|_| !probe.fallback);
                    let mut start_fallback = || {
                        if let Some(protocol) = fallback.take() {
                            scheduler.add_fallback(Probe {
                                job: probe.job,
                                protocol,
                                fallback: true,
                                row: board.add_fallback(probe.row, protocol),
                            });
                        }
                    };
                    let started = Instant::now();
                    board.update(probe.row, |row| {
                        row.stage = Stage::Tracing;
                        row.started = Some(started);
                    });
                    let result = trace(&probe, &mut |rounds, reached| {
                        if !reached && rounds >= FALLBACK_AFTER_ROUNDS {
                            start_fallback();
                        }
                    });
                    if matches!(&result, Ok(route) if !route.reached) {
                        start_fallback();
                    }
                    board.update(probe.row, |row| {
                        row.stage = Stage::Done;
                        row.elapsed = started.elapsed();
                    });
                    outcomes
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(Outcome {
                            label: job_label(job, probe.protocol),
                            result,
                        });
                }
            });
        }
    });
    outcomes
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn settings(time_limit: Duration, interval: Duration) -> Settings {
        Settings {
            privilege: PrivilegeMode::Privileged,
            min_rounds: 5,
            max_rounds: 30,
            time_limit,
            interval,
            max_ttl: 30,
            tcp_port: 443,
        }
    }

    fn reply(ttl: u8, host: &str, rtt_ms: u64, labels: &[u32]) -> ProbeStatus {
        use trippy_core::{
            IcmpPacketType, MplsLabelStack, MplsLabelStackMember, ProbeComplete, RoundId, Sequence,
            TimeToLive, TraceId,
        };
        let sent = SystemTime::UNIX_EPOCH;
        let extensions = (!labels.is_empty()).then(|| Extensions {
            extensions: vec![Extension::Mpls(MplsLabelStack {
                members: labels
                    .iter()
                    .enumerate()
                    .map(|(index, label)| MplsLabelStackMember {
                        label: *label,
                        exp: 0,
                        bos: u8::from(index + 1 == labels.len()),
                        ttl: 1,
                    })
                    .collect(),
            })],
        });
        ProbeStatus::Complete(ProbeComplete {
            sequence: Sequence(33434),
            identifier: TraceId(1),
            src_port: Port(0),
            dest_port: Port(0),
            ttl: TimeToLive(ttl),
            round: RoundId(0),
            sent,
            host: host.parse().unwrap(),
            received: sent + Duration::from_millis(rtt_ms),
            icmp_packet_type: IcmpPacketType::NotApplicable,
            tos: None,
            expected_udp_checksum: None,
            actual_udp_checksum: None,
            extensions,
        })
    }

    fn record(log: &mut HostLog, probes: &[ProbeStatus]) {
        record_round(
            log,
            &Round::new(
                probes,
                trippy_core::TimeToLive(8),
                trippy_core::CompletionReason::TargetFound,
            ),
        );
    }

    #[test]
    fn keeps_each_hosts_best_round_trip_and_latest_label_stack() {
        let mut log = HostLog::new();
        record(
            &mut log,
            &[
                reply(5, "192.0.2.1", 40, &[24001, 16]),
                reply(5, "192.0.2.2", 90, &[]),
                ProbeStatus::NotSent,
            ],
        );
        record(
            &mut log,
            &[
                reply(5, "192.0.2.1", 35, &[]),
                reply(5, "192.0.2.2", 95, &[30002]),
            ],
        );
        let first = &log[&(5, "192.0.2.1".parse().unwrap())];
        assert_eq!(first.best, Some(Duration::from_millis(35)));
        assert_eq!(
            first.mpls,
            [
                model::MplsLabel {
                    label: 24001,
                    tc: 0,
                    bottom: false,
                    ttl: 1
                },
                model::MplsLabel {
                    label: 16,
                    tc: 0,
                    bottom: true,
                    ttl: 1
                },
            ]
        );
        let second = &log[&(5, "192.0.2.2".parse().unwrap())];
        assert_eq!(second.best, Some(Duration::from_millis(90)));
        assert_eq!(second.mpls.len(), 1);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn publishes_round_state_and_host_metadata_together() {
        let job = job("192.0.2.2", None);
        let mut core = Builder::new(job.addr).build().unwrap().snapshot();
        let mut completed = CompletedTrace::new(core.clone());
        let report = |completed: &CompletedTrace| {
            completed.route(&job, Protocol::Icmp, SystemTime::UNIX_EPOCH, Duration::ZERO)
        };
        let first = [reply(1, "192.0.2.1", 40, &[100])];
        let first_round = Round::new(
            &first,
            trippy_core::TimeToLive(1),
            trippy_core::CompletionReason::RoundTimeLimitExceeded,
        );
        core.update_from_round(&first_round);
        assert_eq!(core.round_count(State::default_flow_id()), 1);
        let before_publish = report(&completed);
        assert_eq!(before_publish.rounds, 0);
        assert!(before_publish.hops.is_empty());

        completed.publish(core.clone(), &first_round);
        let first_report = report(&completed);
        assert_eq!(first_report.rounds, 1);
        assert_eq!(first_report.hops.len(), 1);
        assert_eq!(first_report.hops[0].hosts[0].best_ms, Some(40.0));
        assert_eq!(first_report.hops[0].hosts[0].mpls[0].label, 100);

        let mut second = [
            reply(1, "192.0.2.1", 20, &[200]),
            reply(2, "192.0.2.2", 30, &[300]),
        ];
        for probe in &mut second {
            if let ProbeStatus::Complete(reply) = probe {
                reply.round = trippy_core::RoundId(1);
            }
        }
        let second_round = Round::new(
            &second,
            trippy_core::TimeToLive(2),
            trippy_core::CompletionReason::TargetFound,
        );
        core.update_from_round(&second_round);
        assert_eq!(core.round_count(State::default_flow_id()), 2);
        let before_publish = report(&completed);
        assert_eq!(before_publish.rounds, 1);
        assert_eq!(before_publish.hops.len(), 1);
        assert!(!before_publish.reached);
        assert_eq!(before_publish.hops[0].hosts[0].best_ms, Some(40.0));
        assert_eq!(before_publish.hops[0].hosts[0].mpls[0].label, 100);

        completed.publish(core, &second_round);
        let final_report = report(&completed);
        assert_eq!(final_report.rounds, 2);
        assert!(final_report.reached);
        assert_eq!(final_report.hops.len(), 2);
        for (hop, ip, best_ms, label) in [
            (&final_report.hops[0], "192.0.2.1", 20.0, 200),
            (&final_report.hops[1], "192.0.2.2", 30.0, 300),
        ] {
            assert_eq!(hop.hosts[0].ip, ip);
            assert_eq!(hop.hosts[0].best_ms, Some(best_ms));
            assert_eq!(hop.hosts[0].mpls[0].label, label);
        }
    }

    #[test]
    fn uses_nat_compatible_flow_stable_udp_only_with_raw_sockets() {
        for (protocol, privilege, strategy) in [
            (
                Protocol::Udp,
                PrivilegeMode::Privileged,
                MultipathStrategy::Dublin,
            ),
            (
                Protocol::Udp,
                PrivilegeMode::Unprivileged,
                MultipathStrategy::Classic,
            ),
            (
                Protocol::Icmp,
                PrivilegeMode::Privileged,
                MultipathStrategy::Classic,
            ),
            (
                Protocol::Tcp,
                PrivilegeMode::Privileged,
                MultipathStrategy::Classic,
            ),
        ] {
            assert_eq!(multipath_strategy(protocol, privilege), strategy);
        }
    }

    #[test]
    fn udp_tracers_keep_their_source_ports_and_sequence_ranges_in_both_families() {
        let settings = settings(Duration::from_secs(45), Duration::from_secs(1));
        let probe = Probe {
            job: 0,
            protocol: Protocol::Udp,
            fallback: false,
            row: 0,
        };
        for target in ["192.0.2.1", "2001:db8::1"] {
            let job = job(target, None);
            for (slot, source_port, sequence) in [
                (0, 41000, 33434),
                (1, 41001, 34970),
                (19, 41019, 62618),
                (20, 41020, 33434),
                (999, 41999, 62618),
            ] {
                let tracer = build_tracer(&job, &probe, slot, &settings).unwrap();
                assert_eq!(tracer.multipath_strategy(), MultipathStrategy::Dublin);
                assert_eq!(
                    tracer.port_direction(),
                    PortDirection::FixedSrc(Port(source_port))
                );
                assert_eq!(tracer.initial_sequence().0, sequence);
            }
        }
    }

    #[test]
    fn round_budget_keeps_the_partial_round_and_final_poll() {
        for (limit, interval, expected) in [
            (Duration::from_secs(45), Duration::from_secs(1), 30),
            (Duration::from_secs(1), Duration::from_millis(100), 11),
            (Duration::from_millis(1001), Duration::from_millis(100), 12),
            (Duration::from_secs(1), Duration::from_secs(2), 2),
            (Duration::MAX, Duration::MAX, 2),
        ] {
            assert_eq!(tracer_rounds(&settings(limit, interval)), expected);
        }
    }

    #[test]
    fn round_budget_caps_durations_larger_than_usize_rounds() {
        for limit in [
            // Exceeds usize only on 32-bit targets.
            Duration::from_secs(u64::from(u32::MAX)),
            // Exceeds usize on 64-bit targets too.
            Duration::from_secs(u64::MAX),
            Duration::MAX,
        ] {
            assert_eq!(
                tracer_rounds(&settings(limit, Duration::from_millis(100))),
                30
            );
        }
    }

    fn job(target: &str, fallback: Option<Protocol>) -> Job {
        Job {
            target: target.to_owned(),
            addr: target.parse().unwrap(),
            protocol: Protocol::Icmp,
            fallback,
            local: IpAddr::from([192, 0, 2, 100]),
        }
    }

    fn board(jobs: &[Job]) -> Board {
        let targets: Vec<String> = jobs.iter().map(|job| job.target.clone()).collect();
        let board = Board::new(&targets);
        board.start_trace(jobs);
        board
    }

    /// The route a tracer returns for `probe` after `rounds` rounds.
    fn route(jobs: &[Job], probe: &Probe, rounds: usize, reached: bool) -> model::Route {
        let job = &jobs[probe.job];
        model::Route {
            target: job.target.clone(),
            destination: job.addr.to_string(),
            protocol: protocol_name(probe.protocol),
            family: 4,
            source_addr: None,
            started_at: String::new(),
            duration_ms: 0,
            rounds,
            reached,
            hops: Vec::new(),
        }
    }

    /// Target, protocol, fallback flag and stage of every row, in the order the view shows them.
    fn rows(board: &Board) -> Vec<(String, Protocol, bool, Stage)> {
        board
            .snapshot()
            .rows
            .into_iter()
            .map(|row| (row.target, row.protocol, row.fallback, row.stage))
            .collect()
    }

    fn row(target: &str, protocol: Protocol, fallback: bool) -> (String, Protocol, bool, Stage) {
        (target.to_owned(), protocol, fallback, Stage::Done)
    }

    #[test]
    fn the_tcp_fallback_goes_to_port_80_and_tcp_tracing_to_the_chosen_port() {
        let job = job("192.0.2.1", Some(Protocol::Tcp));
        let settings = settings(Duration::from_secs(45), Duration::from_secs(1));
        let port = |fallback| {
            let probe = Probe {
                job: 0,
                protocol: Protocol::Tcp,
                fallback,
                row: 0,
            };
            build_tracer(&job, &probe, 3, &settings)
                .unwrap()
                .port_direction()
        };
        assert_eq!(port(true), PortDirection::FixedDest(Port(80)));
        assert_eq!(port(false), PortDirection::FixedDest(Port(443)));
    }

    #[test]
    fn the_fallback_starts_beside_a_first_protocol_without_the_target_after_4_rounds() {
        let jobs = [job("192.0.2.1", Some(Protocol::Tcp))];
        let board = board(&jobs);
        let (started, fallback_started) = mpsc::channel();
        let fallback_started = Mutex::new(fallback_started);
        let outcomes = schedule(&jobs, 2, &board, |probe, watch| {
            if probe.fallback {
                started.send(()).unwrap();
                return Ok(route(&jobs, probe, 7, true));
            }
            for round in 1..FALLBACK_AFTER_ROUNDS {
                watch(round, false);
            }
            assert_eq!(board.snapshot().rows.len(), 1, "no fallback before round 4");
            watch(FALLBACK_AFTER_ROUNDS, false);
            fallback_started
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .expect("the fallback starts while ICMP is still tracing");
            // ICMP reaching the target later keeps the fallback route.
            watch(FALLBACK_AFTER_ROUNDS + 2, true);
            Ok(route(&jobs, probe, 9, true))
        });
        let mut labels: Vec<&str> = outcomes
            .iter()
            .map(|outcome| outcome.label.as_str())
            .collect();
        labels.sort_unstable();
        assert_eq!(labels, ["192.0.2.1 IPv4/ICMP", "192.0.2.1 IPv4/TCP"]);
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.result.as_ref().is_ok_and(|route| route.reached))
        );
        assert_eq!(
            rows(&board),
            [
                row("192.0.2.1", Protocol::Icmp, false),
                row("192.0.2.1", Protocol::Tcp, true)
            ]
        );
    }

    #[test]
    fn a_first_protocol_that_reaches_the_target_by_round_4_runs_alone() {
        let jobs = [job("192.0.2.1", Some(Protocol::Tcp))];
        let board = board(&jobs);
        let outcomes = schedule(&jobs, 2, &board, |probe, watch| {
            assert!(!probe.fallback, "a reached target needs no fallback");
            for round in 1..FALLBACK_AFTER_ROUNDS {
                watch(round, false);
            }
            for round in FALLBACK_AFTER_ROUNDS..10 {
                watch(round, true);
            }
            Ok(route(&jobs, probe, 9, true))
        });
        assert_eq!(outcomes.len(), 1);
        assert_eq!(rows(&board), [row("192.0.2.1", Protocol::Icmp, false)]);
    }

    #[test]
    fn one_tracer_runs_each_fallback_after_its_first_protocol_and_before_later_jobs() {
        let jobs = [
            job("192.0.2.1", Some(Protocol::Udp)),
            job("192.0.2.2", Some(Protocol::Udp)),
        ];
        let board = board(&jobs);
        let order = Mutex::new(Vec::new());
        let outcomes = schedule(&jobs, 1, &board, |probe, watch| {
            order.lock().unwrap().push((probe.job, probe.protocol));
            // Ends without the target before round 4, as a short time limit does.
            watch(2, false);
            Ok(route(&jobs, probe, 2, false))
        });
        assert_eq!(
            order.into_inner().unwrap(),
            [
                (0, Protocol::Icmp),
                (0, Protocol::Udp),
                (1, Protocol::Icmp),
                (1, Protocol::Udp)
            ]
        );
        assert_eq!(outcomes.len(), 4);
        assert_eq!(
            rows(&board),
            [
                row("192.0.2.1", Protocol::Icmp, false),
                row("192.0.2.1", Protocol::Udp, true),
                row("192.0.2.2", Protocol::Icmp, false),
                row("192.0.2.2", Protocol::Udp, true)
            ]
        );
    }

    #[test]
    fn parallel_limits_the_running_tracers_fallbacks_included() {
        let jobs: Vec<Job> = (1..=4)
            .map(|n| job(&format!("192.0.2.{n}"), Some(Protocol::Tcp)))
            .collect();
        assert_eq!(route_count(&jobs), 8);
        assert_eq!(route_count(&[job("192.0.2.1", None)]), 1);
        let board = board(&jobs);
        let running = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let outcomes = schedule(&jobs, 3, &board, |probe, watch| {
            peak.fetch_max(running.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
            for round in 1..=6 {
                watch(round, false);
                thread::sleep(Duration::from_millis(5));
            }
            running.fetch_sub(1, Ordering::SeqCst);
            Ok(route(&jobs, probe, 6, false))
        });
        assert_eq!(outcomes.len(), 8);
        assert!(peak.into_inner() <= 3);
        let rows = rows(&board);
        for (pair, n) in rows.chunks(2).zip(1..) {
            let target = format!("192.0.2.{n}");
            assert_eq!(
                pair,
                [
                    row(&target, Protocol::Icmp, false),
                    row(&target, Protocol::Tcp, true)
                ]
            );
        }
    }

    /// macOS gives every user ICMP sockets, so this traces loopback for real.
    #[cfg(target_os = "macos")]
    #[test]
    fn loopback_answers_icmp_and_starts_no_fallback() {
        let jobs = [Job {
            local: IpAddr::from([127, 0, 0, 1]),
            ..job("127.0.0.1", Some(Protocol::Udp))
        }];
        let board = board(&jobs);
        let settings = Settings {
            privilege: PrivilegeMode::Unprivileged,
            min_rounds: 1,
            max_rounds: 10,
            time_limit: Duration::from_secs(10),
            interval: Duration::from_millis(100),
            max_ttl: 4,
            tcp_port: 443,
        };
        let outcomes = run_all(&jobs, 2, &settings, &board);
        assert_eq!(outcomes.len(), 1);
        let route = outcomes[0].result.as_ref().unwrap();
        assert_eq!((route.protocol, route.reached), ("icmp", true));
        assert!(!route.hops.is_empty());
        assert!(
            route
                .hops
                .iter()
                .flat_map(|hop| &hop.hosts)
                .all(|host| host.best_ms.is_some())
        );
        assert_eq!(rows(&board), [row("127.0.0.1", Protocol::Icmp, false)]);
    }
}
