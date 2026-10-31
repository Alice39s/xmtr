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

/// One row of the view: a target before it resolves, then one traced address and protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Identifies the row while fallback rows are inserted above later rows.
    pub id: usize,
    pub target: String,
    pub family: u8,
    pub protocol: Protocol,
    /// Traces the fallback protocol beside the first protocol, which had not reached the target.
    pub fallback: bool,
    pub stage: Stage,
    pub rounds: usize,
    pub reached: bool,
    pub started: Option<Instant>,
    pub elapsed: Duration,
}

impl Row {
    fn resolving(id: usize, target: &str) -> Self {
        Self {
            id,
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
                    .enumerate()
                    .map(|(id, target)| Row::resolving(id, target))
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
            .enumerate()
            .map(|(id, job)| Row {
                family: if job.addr.is_ipv4() { 4 } else { 6 },
                protocol: job.protocol,
                stage: Stage::Queued,
                ..Row::resolving(id, &job.target)
            })
            .collect();
    }

    /// Adds a queued row for the fallback of row `parent` right below it and returns its id.
    pub fn add_fallback(&self, parent: usize, protocol: Protocol) -> usize {
        let mut state = self.lock();
        // Ids count up from 0 and rows are never removed, so the count is a free id.
        let id = state.rows.len();
        let position = state
            .rows
            .iter()
            .position(|row| row.id == parent)
            .expect("a fallback belongs to a row on the board");
        let parent = &state.rows[position];
        let row = Row {
            family: parent.family,
            protocol,
            fallback: true,
            stage: Stage::Queued,
            ..Row::resolving(id, &parent.target)
        };
        state.rows.insert(position + 1, row);
        id
    }

    pub fn update(&self, id: usize, change: impl FnOnce(&mut Row)) {
        if let Some(row) = self.lock().rows.iter_mut().find(|row| row.id == id) {
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
