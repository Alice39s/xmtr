//! Progress told as one line per change, for the views that cannot redraw a panel: the line view
//! prints these lines above its status line, the plain view prints nothing else.

use super::Options;
use crate::board::{Phase, Row, Stage, State};
use crate::runner::protocol_name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A step started.
    Step(Phase),
    /// The first protocol did not reach the target; the row now traces with its fallback.
    Fallback(Row),
    /// A route finished.
    Route(Row),
    /// Reverse DNS and AS lookups ended with this many of the addresses answered.
    Lookup { answered: usize, total: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Step,
    Good,
    Bad,
    Warn,
}

/// The steps this run shows, in order.
fn steps(options: &Options) -> Vec<Phase> {
    let mut steps = vec![Phase::Resolve, Phase::Trace];
    if options.lookup {
        steps.push(Phase::Lookup);
    }
    if options.upload.is_some() {
        steps.push(Phase::Upload);
    }
    steps.push(Phase::Result);
    steps
}

/// The 1-based number of `phase` and the step count, such as `(2, 4)`.
pub fn number(phase: Phase, options: &Options) -> (usize, usize) {
    let steps = steps(options);
    let position = steps.iter().position(|step| *step == phase).unwrap_or(0);
    (position + 1, steps.len())
}

pub const fn label(phase: Phase) -> &'static str {
    match phase {
        Phase::Resolve => "Resolve",
        Phase::Trace => "Trace",
        Phase::Lookup => "Lookup",
        Phase::Upload => "Upload",
        Phase::Result => "Result",
    }
}

/// What happened between two snapshots of the board. `before` is `None` for the first snapshot.
pub fn between(before: Option<&State>, after: &State) -> Vec<Event> {
    let mut events = Vec::new();
    let Some(before) = before else {
        events.push(Event::Step(after.phase));
        return events;
    };
    if before.phase == Phase::Lookup && after.phase != Phase::Lookup {
        let (answered, total) = after.lookup;
        events.push(Event::Lookup { answered, total });
    }
    // Before the trace starts the rows are the targets, not the routes, so nothing is compared.
    if after.phase >= Phase::Trace {
        let traced = before.phase >= Phase::Trace;
        for (index, row) in after.rows.iter().enumerate() {
            let old = traced.then(|| before.rows.get(index)).flatten();
            if row.fallback && !old.is_some_and(|old| old.fallback) {
                events.push(Event::Fallback(row.clone()));
            }
            if row.stage == Stage::Done && !old.is_some_and(|old| old.stage == Stage::Done) {
                events.push(Event::Route(row.clone()));
            }
        }
    }
    if after.phase != before.phase && after.phase != Phase::Result {
        events.push(Event::Step(after.phase));
    }
    events
}

fn route_name(row: &Row) -> String {
    format!(
        "{} v{} {}",
        row.target,
        row.family,
        protocol_name(row.protocol).to_uppercase()
    )
}

/// The step the event belongs to, its tone and its text without any decoration.
pub fn describe(event: &Event, state: &State, options: &Options) -> (Phase, Tone, String) {
    match event {
        Event::Step(phase) => {
            let detail = match phase {
                Phase::Resolve => plural(state.rows.len(), "target"),
                Phase::Trace => plural(state.rows.len(), "route"),
                Phase::Lookup => "reverse DNS and AS numbers".to_owned(),
                Phase::Upload => options.upload.clone().unwrap_or_default(),
                Phase::Result => String::new(),
            };
            (*phase, Tone::Step, format!("{}: {detail}", label(*phase)))
        }
        Event::Fallback(row) => (
            Phase::Trace,
            Tone::Warn,
            format!(
                "{} v{}: ICMP did not reach the target, trying {}",
                row.target,
                row.family,
                protocol_name(row.protocol).to_uppercase()
            ),
        ),
        Event::Route(row) if row.reached => (
            Phase::Trace,
            Tone::Good,
            format!(
                "{}: reached, {}, {:.1}s",
                route_name(row),
                plural(row.rounds, "round"),
                row.elapsed.as_secs_f64()
            ),
        ),
        Event::Route(row) => (
            Phase::Trace,
            Tone::Bad,
            format!(
                "{}: not reached, {:.1}s",
                route_name(row),
                row.elapsed.as_secs_f64()
            ),
        ),
        Event::Lookup { answered, total } => (
            Phase::Lookup,
            Tone::Good,
            format!(
                "Lookup: {answered} of {} answered",
                plural(*total, "address")
            ),
        ),
    }
}

pub fn plural(count: usize, noun: &str) -> String {
    match (count, noun.ends_with('s')) {
        (1, _) => format!("1 {noun}"),
        (_, true) => format!("{count} {noun}es"),
        (_, false) => format!("{count} {noun}s"),
    }
}

/// Routes done and all routes, for the trace counter.
pub fn traced(state: &State) -> (usize, usize) {
    let done = state
        .rows
        .iter()
        .filter(|row| row.stage == Stage::Done)
        .count();
    (done, state.rows.len())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use trippy_core::Protocol;

    use super::*;
    use crate::live::tests::options;

    fn row(stage: Stage, fallback: bool) -> Row {
        Row {
            target: "1.1.1.1".to_owned(),
            family: 4,
            protocol: if fallback {
                Protocol::Tcp
            } else {
                Protocol::Icmp
            },
            fallback,
            stage,
            rounds: 7,
            reached: stage == Stage::Done,
            started: None,
            elapsed: Duration::from_millis(5200),
        }
    }

    fn state(phase: Phase, rows: Vec<Row>) -> State {
        State {
            phase,
            rows,
            lookup: (3, 4),
        }
    }

    #[test]
    fn reports_steps_fallbacks_and_finished_routes_once() {
        let start = state(Phase::Resolve, vec![row(Stage::Resolving, false)]);
        assert_eq!(between(None, &start), [Event::Step(Phase::Resolve)]);
        let tracing = state(Phase::Trace, vec![row(Stage::Tracing, false)]);
        assert_eq!(between(Some(&start), &tracing), [Event::Step(Phase::Trace)]);
        let fallback = state(Phase::Trace, vec![row(Stage::Done, true)]);
        assert_eq!(
            between(Some(&tracing), &fallback),
            [
                Event::Fallback(row(Stage::Done, true)),
                Event::Route(row(Stage::Done, true))
            ]
        );
        assert!(between(Some(&fallback), &fallback).is_empty());
        let lookup = state(Phase::Lookup, vec![row(Stage::Done, true)]);
        let upload = state(Phase::Upload, vec![row(Stage::Done, true)]);
        assert_eq!(
            between(Some(&lookup), &upload),
            [
                Event::Lookup {
                    answered: 3,
                    total: 4
                },
                Event::Step(Phase::Upload)
            ]
        );
    }

    #[test]
    fn describes_events_as_plain_text() {
        let state = state(Phase::Trace, vec![row(Stage::Done, false)]);
        let (phase, tone, text) =
            describe(&Event::Route(row(Stage::Done, false)), &state, &options());
        assert_eq!((phase, tone), (Phase::Trace, Tone::Good));
        assert_eq!(text, "1.1.1.1 v4 ICMP: reached, 7 rounds, 5.2s");
        assert_eq!(number(Phase::Upload, &options()), (4, 5));
        assert_eq!(plural(1, "address"), "1 address");
        assert_eq!(plural(2, "address"), "2 addresses");
    }
}
