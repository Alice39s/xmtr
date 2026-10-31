//! Progress told as one line per change, for the views that cannot redraw a panel: the line view
//! prints these lines above its status line, the plain view prints nothing else.

use trippy_core::Protocol;

use super::Options;
use crate::board::{Phase, Row, Stage, State};
use crate::runner::{FALLBACK_TCP_PORT, protocol_name};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A step started.
    Step(Phase),
    /// The first protocol had not reached the target; this fallback row traces beside it.
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

/// What happened between two snapshots of the board, in the order it happened. `before` is `None`
/// for the first snapshot. Several steps can pass between two snapshots; each one still gets its
/// line, and a lookup that ended gets its summary. An `after` in `Phase::Result` marks the end of
/// the run: it closes a running lookup but starts no step.
pub fn between(before: Option<&State>, after: &State, options: &Options) -> Vec<Event> {
    let from = before.map(|before| before.phase);
    let ended = after.phase == Phase::Result;
    let started =
        |step: Phase| !ended && from.is_none_or(|from| step > from) && step <= after.phase;
    // The lookup ran either when `before` was taken or entirely between the two snapshots.
    let lookup_ended = after.phase > Phase::Lookup
        && (from == Some(Phase::Lookup)
            || (!ended && from.is_none_or(|from| from < Phase::Lookup)));
    let steps = steps(options);
    let mut events: Vec<Event> = steps
        .iter()
        .filter(|step| **step <= Phase::Trace && started(**step))
        .map(|step| Event::Step(*step))
        .collect();
    // Before the trace starts the rows are the targets, not the routes, so nothing is compared.
    if after.phase >= Phase::Trace {
        let traced = before.filter(|before| before.phase >= Phase::Trace);
        for row in &after.rows {
            let old = traced.and_then(|before| before.rows.iter().find(|old| old.id == row.id));
            if row.fallback && old.is_none() {
                events.push(Event::Fallback(row.clone()));
            }
            if row.stage == Stage::Done && !old.is_some_and(|old| old.stage == Stage::Done) {
                events.push(Event::Route(row.clone()));
            }
        }
    }
    for step in steps.into_iter().filter(|step| *step > Phase::Trace) {
        if started(step) {
            events.push(Event::Step(step));
        }
        if step == Phase::Lookup && lookup_ended {
            let (answered, total) = after.lookup;
            events.push(Event::Lookup { answered, total });
        }
    }
    events
}

/// The protocol of a row, with the port of the TCP fallback: `ICMP`, `UDP` or `TCP:80`.
pub fn probe(row: &Row) -> String {
    let name = protocol_name(row.protocol).to_uppercase();
    if row.fallback && row.protocol == Protocol::Tcp {
        format!("{name}:{FALLBACK_TCP_PORT}")
    } else {
        name
    }
}

fn route_name(row: &Row) -> String {
    format!("{} v{} {}", row.target, row.family, probe(row))
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
                "{} v{}: ICMP has not reached the target, adding {}",
                row.target,
                row.family,
                probe(row)
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

    use super::*;
    use crate::live::tests::options;

    fn row(stage: Stage, fallback: bool) -> Row {
        Row {
            id: usize::from(fallback),
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

    const SUMMARY: Event = Event::Lookup {
        answered: 3,
        total: 4,
    };

    #[test]
    fn reports_steps_fallbacks_and_finished_routes_once() {
        let options = options();
        let start = state(Phase::Resolve, vec![row(Stage::Resolving, false)]);
        assert_eq!(
            between(None, &start, &options),
            [Event::Step(Phase::Resolve)]
        );
        let tracing = state(Phase::Trace, vec![row(Stage::Tracing, false)]);
        assert_eq!(
            between(Some(&start), &tracing, &options),
            [Event::Step(Phase::Trace)]
        );
        let fallback = state(
            Phase::Trace,
            vec![row(Stage::Tracing, false), row(Stage::Queued, true)],
        );
        assert_eq!(
            between(Some(&tracing), &fallback, &options),
            [Event::Fallback(row(Stage::Queued, true))]
        );
        let done = state(
            Phase::Trace,
            vec![row(Stage::Done, false), row(Stage::Done, true)],
        );
        assert_eq!(
            between(Some(&fallback), &done, &options),
            [
                Event::Route(row(Stage::Done, false)),
                Event::Route(row(Stage::Done, true))
            ]
        );
        assert!(between(Some(&done), &done, &options).is_empty());
        let lookup = state(Phase::Lookup, vec![row(Stage::Done, true)]);
        let upload = state(Phase::Upload, vec![row(Stage::Done, true)]);
        assert_eq!(
            between(Some(&lookup), &upload, &options),
            [SUMMARY, Event::Step(Phase::Upload)]
        );
    }

    #[test]
    fn rows_below_a_new_fallback_row_keep_their_lines() {
        let other = Row {
            id: 2,
            target: "192.0.2.1".to_owned(),
            ..row(Stage::Done, false)
        };
        let before = state(
            Phase::Trace,
            vec![row(Stage::Tracing, false), other.clone()],
        );
        let after = state(
            Phase::Trace,
            vec![row(Stage::Tracing, false), row(Stage::Queued, true), other],
        );
        assert_eq!(
            between(Some(&before), &after, &options()),
            [Event::Fallback(row(Stage::Queued, true))]
        );
    }

    #[test]
    fn steps_passed_between_two_snapshots_keep_their_lines() {
        let options = options();
        let tracing = state(Phase::Trace, vec![row(Stage::Tracing, false)]);
        let upload = state(Phase::Upload, vec![row(Stage::Done, false)]);
        assert_eq!(
            between(Some(&tracing), &upload, &options),
            [
                Event::Route(row(Stage::Done, false)),
                Event::Step(Phase::Lookup),
                SUMMARY,
                Event::Step(Phase::Upload)
            ]
        );
        let start = state(Phase::Resolve, vec![row(Stage::Resolving, false)]);
        let traced = state(Phase::Lookup, vec![row(Stage::Done, false)]);
        assert_eq!(
            between(Some(&start), &traced, &options),
            [
                Event::Step(Phase::Trace),
                Event::Route(row(Stage::Done, false)),
                Event::Step(Phase::Lookup)
            ]
        );
    }

    #[test]
    fn the_end_of_the_run_closes_the_lookup() {
        let lookup = state(Phase::Lookup, vec![row(Stage::Done, false)]);
        let result = state(Phase::Result, vec![row(Stage::Done, false)]);
        assert_eq!(between(Some(&lookup), &result, &options()), [SUMMARY]);
        let no_lookup = Options {
            lookup: false,
            upload: None,
            ..options()
        };
        let tracing = state(Phase::Trace, vec![row(Stage::Done, false)]);
        assert!(between(Some(&tracing), &result, &no_lookup).is_empty());
        // A run that ends before its lookup starts has nothing to close.
        assert!(between(Some(&tracing), &result, &options()).is_empty());
    }

    #[test]
    fn describes_events_as_plain_text() {
        let state = state(Phase::Trace, vec![row(Stage::Done, false)]);
        let (phase, tone, text) =
            describe(&Event::Route(row(Stage::Done, false)), &state, &options());
        assert_eq!((phase, tone), (Phase::Trace, Tone::Good));
        assert_eq!(text, "1.1.1.1 v4 ICMP: reached, 7 rounds, 5.2s");
        let (_, tone, text) = describe(
            &Event::Fallback(row(Stage::Queued, true)),
            &state,
            &options(),
        );
        assert_eq!(tone, Tone::Warn);
        assert_eq!(
            text,
            "1.1.1.1 v4: ICMP has not reached the target, adding TCP:80"
        );
        let (_, _, text) = describe(&Event::Route(row(Stage::Done, true)), &state, &options());
        assert_eq!(text, "1.1.1.1 v4 TCP:80: reached, 7 rounds, 5.2s");
        assert_eq!(number(Phase::Upload, &options()), (4, 5));
        assert_eq!(plural(1, "address"), "1 address");
        assert_eq!(plural(2, "address"), "2 addresses");
    }
}
