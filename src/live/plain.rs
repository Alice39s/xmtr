//! Progress as plain text lines on stderr, for logs, pipes and terminals that cannot move the
//! cursor: one line per step and finished route, tagged with the step number, and a reminder of the
//! current state when nothing has changed for a while. No colors and no escape sequences.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use super::Options;
use super::events::{self, Event};
use crate::board::{Phase, Stage, State};

/// Silence after which the current state is printed again, so a log shows the run is alive.
const HEARTBEAT: Duration = Duration::from_secs(10);

pub struct PlainView {
    before: Option<State>,
    last_line: Instant,
}

impl PlainView {
    pub fn new() -> Self {
        Self {
            before: None,
            last_line: Instant::now(),
        }
    }

    pub fn draw(&mut self, state: &State, options: &Options, elapsed: f64) -> io::Result<()> {
        let lines: Vec<String> = events::between(self.before.as_ref(), state)
            .iter()
            .map(|event| line(event, state, options))
            .collect();
        self.before = Some(state.clone());
        let mut out = io::stderr().lock();
        if lines.is_empty() {
            if self.last_line.elapsed() < HEARTBEAT {
                return Ok(());
            }
            let (number, steps) = events::number(state.phase, options);
            writeln!(
                out,
                "[{number}/{steps}] {} ({elapsed:.0}s)",
                still(state, options)
            )?;
        }
        for line in lines {
            writeln!(out, "{line}")?;
        }
        self.last_line = Instant::now();
        out.flush()
    }
}

fn line(event: &Event, state: &State, options: &Options) -> String {
    let (phase, _, text) = events::describe(event, state, options);
    let (number, steps) = events::number(phase, options);
    format!("[{number}/{steps}] {text}")
}

/// What the run is still doing.
fn still(state: &State, options: &Options) -> String {
    match state.phase {
        Phase::Resolve => "Resolve: still resolving".to_owned(),
        Phase::Trace => {
            let (done, total) = events::traced(state);
            let tracing = state
                .rows
                .iter()
                .filter(|row| row.stage == Stage::Tracing)
                .count();
            format!("Trace: {done}/{total} done, {tracing} tracing")
        }
        Phase::Lookup => {
            let (answered, total) = state.lookup;
            format!("Lookup: {answered}/{total} answered")
        }
        Phase::Upload => format!(
            "Upload: waiting for {}",
            options.upload.as_deref().unwrap_or_default()
        ),
        Phase::Result => "Result".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::tests::options;

    #[test]
    fn lines_are_tagged_with_the_step_and_free_of_escapes() {
        let state = State {
            phase: Phase::Upload,
            rows: Vec::new(),
            lookup: (0, 0),
        };
        let text = line(&Event::Step(Phase::Upload), &state, &options());
        assert_eq!(text, "[4/5] Upload: info-api.example");
        assert_eq!(
            still(&state, &options()),
            "Upload: waiting for info-api.example"
        );
        assert!(!text.contains('\u{1b}'));
    }
}
