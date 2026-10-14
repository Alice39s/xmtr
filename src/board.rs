//! Run state shared between the workers that trace, look up and upload, and the live view that
//! draws it.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use trippy_core::Protocol;

use crate::plan::Job;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Resolve,
    Trace,
    Lookup,
    Upload,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The target's addresses are not known yet.
    Resolving,
    Queued,
    Tracing,
    Done,
}

/// One row of the view: a target before it resolves, then one traced address.
#[derive(Debug, Clone)]
pub struct Row {
    pub target: String,
    pub family: u8,
    pub protocol: Protocol,
    /// The first protocol did not reach the target and this is the fallback.
    pub fallback: bool,
    pub stage: Stage,
    pub rounds: usize,
    pub reached: bool,
    pub started: Option<Instant>,
    pub elapsed: Duration,
}

impl Row {
    fn resolving(target: &str) -> Self {
        Self {
            target: target.to_owned(),
            family: 0,
            protocol: Protocol::Icmp,
            fallback: false,
            stage: Stage::Resolving,
            rounds: 0,
            reached: false,
            started: None,
            elapsed: Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub phase: Phase,
    pub rows: Vec<Row>,
    /// Addresses answered by reverse DNS and AS lookups, and how many were asked.
    pub lookup: (usize, usize),
}

#[derive(Debug)]
pub struct Board {
    state: Mutex<State>,
}

impl Board {
    pub fn new(targets: &[String]) -> Self {
        Self {
            state: Mutex::new(State {
                phase: Phase::Resolve,
                rows: targets
                    .iter()
                    .map(|target| Row::resolving(target))
                    .collect(),
                lookup: (0, 0),
            }),
        }
    }

    /// A worker that panicked while holding the lock leaves plain data behind, which is still fine
    /// to show.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn snapshot(&self) -> State {
        self.lock().clone()
    }

    /// Replaces the resolving targets with the planned jobs and starts the trace phase.
    pub fn start_trace(&self, jobs: &[Job]) {
        let mut state = self.lock();
        state.phase = Phase::Trace;
        state.rows = jobs
            .iter()
            .map(|job| Row {
                target: job.target.clone(),
                family: if job.addr.is_ipv4() { 4 } else { 6 },
                protocol: job.protocol,
                stage: Stage::Queued,
                ..Row::resolving(&job.target)
            })
            .collect();
    }

    pub fn update(&self, index: usize, change: impl FnOnce(&mut Row)) {
        if let Some(row) = self.lock().rows.get_mut(index) {
            change(row);
        }
    }

    pub fn start_lookup(&self) {
        self.lock().phase = Phase::Lookup;
    }

    pub fn set_lookup(&self, answered: usize, total: usize) {
        self.lock().lookup = (answered, total);
    }

    pub fn start_upload(&self) {
        self.lock().phase = Phase::Upload;
    }
}
