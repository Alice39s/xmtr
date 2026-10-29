//! The live view for terminals too small for the panel: finished steps and routes scroll by as
//! single lines, and one status line at the bottom is redrawn in place. Every line is cut to the
//! terminal width, dropping the least useful parts first, so nothing wraps.

use std::io::{self, Write};

use ratatui::backend::IntoCrossterm;
use ratatui::crossterm::cursor::MoveToColumn;
use ratatui::crossterm::queue;
use ratatui::crossterm::style::{Print, PrintStyledContent, StyledContent};
use ratatui::crossterm::terminal::{self, Clear, ClearType};
use ratatui::style::Stylize;
use ratatui::text::Span;

use super::Options;
use super::events::{self, Event, Tone};
use super::progress::route_ratio;
use super::theme::{bar, muted, paint, palette, spinner, sweep};
use crate::board::{Phase, Stage, State};

const BAR_WIDTH: usize = 10;
const ELLIPSIS: &str = "…";

/// One part of a line; parts with a higher `drop` rank go first when the line is too long.
struct Part {
    drop: u8,
    spans: Vec<Span<'static>>,
}

const fn part(drop: u8, spans: Vec<Span<'static>>) -> Part {
    Part { drop, spans }
}

fn width_of(spans: &[Span<'static>]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// Cuts `spans` to `width` cells, ending with an ellipsis when something was cut.
fn truncate(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    if width_of(&spans) <= width {
        return spans;
    }
    let room = width.saturating_sub(1);
    let mut used = 0;
    let mut kept = Vec::new();
    'outer: for span in spans {
        let mut text = String::new();
        for c in span.content.chars() {
            let cell = Span::raw(c.to_string()).width();
            if used + cell > room {
                kept.push(Span::styled(text, span.style));
                break 'outer;
            }
            used += cell;
            text.push(c);
        }
        kept.push(Span::styled(text, span.style));
    }
    if width > 0 {
        kept.push(muted(ELLIPSIS));
    }
    kept
}

/// Joins the parts with single spaces, leaving out the highest ranks until the line fits `width`.
fn fit(mut parts: Vec<Part>, width: usize) -> Vec<Span<'static>> {
    let total = |parts: &[Part]| {
        parts
            .iter()
            .map(|part| width_of(&part.spans))
            .sum::<usize>()
            + parts.len().saturating_sub(1)
    };
    while total(&parts) > width {
        let Some(worst) = parts
            .iter()
            .enumerate()
            .filter(|(_, part)| part.drop > 0)
            .max_by_key(|(index, part)| (part.drop, *index))
            .map(|(index, _)| index)
        else {
            break;
        };
        parts.remove(worst);
    }
    let mut spans = Vec::new();
    for (index, part) in parts.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.extend(part.spans);
    }
    truncate(spans, width)
}

/// How much of the trace is done, counting running routes by their own progress.
fn trace_ratio(state: &State, options: &Options) -> f64 {
    if state.rows.is_empty() {
        return 0.0;
    }
    let sum: f64 = state
        .rows
        .iter()
        .map(|row| match row.stage {
            Stage::Done => 1.0,
            Stage::Tracing => route_ratio(row, options),
            Stage::Resolving | Stage::Queued => 0.0,
        })
        .sum();
    sum / state.rows.len() as f64
}

/// The status line: step, counter, bar, details and elapsed time, in that order of importance.
pub fn status(
    state: &State,
    options: &Options,
    elapsed: f64,
    tick: usize,
    width: usize,
) -> Vec<Span<'static>> {
    let (number, steps) = events::number(state.phase, options);
    let accent = palette().accent;
    let mut parts = vec![
        part(0, vec![spinner(tick)]),
        part(0, vec![paint(format!("{number}/{steps}"), accent).bold()]),
        part(3, vec![Span::raw(events::label(state.phase)).bold()]),
    ];
    match state.phase {
        Phase::Resolve => {
            parts.push(part(
                4,
                vec![muted(events::plural(state.rows.len(), "target"))],
            ));
        }
        Phase::Trace => {
            let (done, total) = events::traced(state);
            let tracing = state
                .rows
                .iter()
                .filter(|row| row.stage == Stage::Tracing)
                .count();
            let reached = state.rows.iter().filter(|row| row.reached).count();
            parts.push(part(1, vec![Span::raw(format!("{done}/{total}"))]));
            parts.push(part(
                5,
                bar(BAR_WIDTH, trace_ratio(state, options), accent, None),
            ));
            parts.push(part(
                6,
                vec![muted(format!("{tracing} tracing · {reached} reached"))],
            ));
        }
        Phase::Lookup => {
            let (answered, total) = state.lookup;
            let ratio = if total == 0 {
                0.0
            } else {
                answered as f64 / total as f64
            };
            parts.push(part(1, vec![Span::raw(format!("{answered}/{total}"))]));
            parts.push(part(5, bar(BAR_WIDTH, ratio, accent, None)));
        }
        Phase::Upload => {
            let host = options.upload.clone().unwrap_or_default();
            parts.push(part(6, vec![muted(host)]));
            parts.push(part(5, sweep(BAR_WIDTH, tick)));
        }
        Phase::Result => {}
    }
    parts.push(part(2, vec![muted(format!("{elapsed:.1}s"))]));
    fit(parts, width)
}

/// A finished step or route as one line.
pub fn event_line(
    event: &Event,
    state: &State,
    options: &Options,
    width: usize,
) -> Vec<Span<'static>> {
    let (phase, tone, text) = events::describe(event, state, options);
    let spans = match tone {
        Tone::Step => {
            let (number, steps) = events::number(phase, options);
            vec![
                paint("▸", palette().accent),
                Span::raw(" "),
                paint(format!("{number}/{steps}"), palette().accent).bold(),
                Span::raw(" "),
                Span::raw(text).bold(),
            ]
        }
        Tone::Good => vec![paint("✓", palette().good), Span::raw(" "), Span::raw(text)],
        Tone::Bad => vec![paint("✗", palette().bad), Span::raw(" "), Span::raw(text)],
        Tone::Warn => vec![paint("↻", palette().warn), Span::raw(" "), Span::raw(text)],
    };
    truncate(spans, width)
}

fn queue_spans(out: &mut impl Write, spans: Vec<Span<'static>>) -> io::Result<()> {
    queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
    for span in spans {
        let style = span.style.into_crossterm();
        queue!(
            out,
            PrintStyledContent(StyledContent::new(style, span.content.into_owned()))
        )?;
    }
    Ok(())
}

/// Columns to use: one less than the terminal, so the cursor never wraps to the next row.
fn columns() -> usize {
    terminal::size()
        .map_or(80, |(width, _)| usize::from(width))
        .saturating_sub(1)
        .max(1)
}

#[derive(Default)]
pub struct LineView {
    before: Option<State>,
}

impl LineView {
    pub fn draw(
        &mut self,
        state: &State,
        options: &Options,
        elapsed: f64,
        tick: usize,
    ) -> io::Result<()> {
        let width = columns();
        let mut out = io::stderr().lock();
        for event in events::between(self.before.as_ref(), state, options) {
            queue_spans(&mut out, event_line(&event, state, options, width))?;
            queue!(out, Print("\r\n"))?;
        }
        self.before = Some(state.clone());
        queue_spans(&mut out, status(state, options, elapsed, tick, width))?;
        out.flush()
    }

    /// Erases the status line, leaving the cursor at the start of the empty row.
    pub fn close(&mut self) -> io::Result<()> {
        let mut out = io::stderr().lock();
        queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        out.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use trippy_core::Protocol;

    use super::*;
    use crate::board::Row;
    use crate::live::tests::options;

    fn text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    fn tracing_state() -> State {
        let row = |stage| Row {
            target: "example.org".to_owned(),
            family: 6,
            protocol: Protocol::Icmp,
            fallback: false,
            stage,
            rounds: 2,
            reached: false,
            started: None,
            elapsed: Duration::ZERO,
        };
        State {
            phase: Phase::Trace,
            rows: vec![row(Stage::Done), row(Stage::Tracing), row(Stage::Queued)],
            lookup: (0, 0),
        }
    }

    #[test]
    fn status_drops_details_before_the_counter() {
        let state = tracing_state();
        let wide = text(&status(&state, &options(), 3.25, 0, 80));
        assert!(wide.contains("2/5 Trace 1/3"), "{wide}");
        assert!(wide.contains("1 tracing · 0 reached 3.2s"), "{wide}");
        let narrow = text(&status(&state, &options(), 3.25, 0, 14));
        assert_eq!(narrow, "⠋ 2/5 1/3 3.2s");
        assert_eq!(text(&status(&state, &options(), 3.25, 0, 5)), "⠋ 2/5");
        assert_eq!(text(&status(&state, &options(), 3.25, 0, 4)), "⠋ 2…");
        for width in 1..60 {
            let line = status(&state, &options(), 3.25, 0, width);
            assert!(width_of(&line) <= width, "{width}: {}", text(&line));
        }
    }

    #[test]
    fn event_lines_are_cut_with_an_ellipsis() {
        let state = tracing_state();
        let event = Event::Route(state.rows[0].clone());
        let line = text(&event_line(&event, &state, &options(), 20));
        assert_eq!(line, "✗ example.org v6 IC…");
    }
}
