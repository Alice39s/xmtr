//! The numbered step line at the top of both panels: `✓ 1. Resolve ─ ⠹ 2. Trace 3/5 ─ ○ 3. Lookup`.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::Options;
use super::theme::{muted, paint, palette, spinner};
use crate::board::{Phase, Stage, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Done,
    /// Running now; drawn with the spinner.
    Active,
    /// Where the run ended up; drawn with a dot.
    Current,
    Failed,
    Waiting,
}

#[derive(Debug, Clone)]
pub struct Step {
    label: &'static str,
    mark: Mark,
    detail: Option<String>,
}

const SEPARATOR: &str = " ─ ";

fn mark(phase: Phase, current: Phase) -> Mark {
    match phase.cmp(&current) {
        std::cmp::Ordering::Less => Mark::Done,
        std::cmp::Ordering::Equal => Mark::Active,
        std::cmp::Ordering::Greater => Mark::Waiting,
    }
}

/// The steps of a run, with counts for the step that is running. `upload_failed` marks the upload
/// step once the run has reached its result.
pub fn steps(state: &State, options: &Options, upload_failed: bool) -> Vec<Step> {
    let current = state.phase;
    let step = |label, phase, detail: Option<String>| Step {
        label,
        mark: mark(phase, current),
        detail: detail.filter(|_| phase == current),
    };
    let done = state
        .rows
        .iter()
        .filter(|row| row.stage == Stage::Done)
        .count();
    let (answered, total) = state.lookup;
    let mut steps = vec![
        step("Resolve", Phase::Resolve, None),
        step(
            "Trace",
            Phase::Trace,
            Some(format!("{done}/{}", state.rows.len())),
        ),
    ];
    if options.lookup {
        steps.push(step(
            "Lookup",
            Phase::Lookup,
            (total > 0).then(|| format!("{answered}/{total}")),
        ));
    }
    if options.upload.is_some() {
        let mut upload = step("Upload", Phase::Upload, None);
        if upload_failed {
            upload.mark = Mark::Failed;
        }
        steps.push(upload);
    }
    let mut result = step("Result", Phase::Result, None);
    if result.mark == Mark::Active {
        result.mark = Mark::Current;
    }
    steps.push(result);
    steps
}

fn spans(number: usize, step: &Step, short: bool, tick: usize) -> Vec<Span<'static>> {
    let icon = match step.mark {
        Mark::Done => paint("✓", palette().good),
        Mark::Active => spinner(tick),
        Mark::Current => paint("●", palette().accent),
        Mark::Failed => paint("✗", palette().bad),
        Mark::Waiting => muted("○"),
    };
    let text = if short {
        format!("{number}")
    } else {
        format!("{number}. {}", step.label)
    };
    let label = match step.mark {
        Mark::Done => Span::raw(text),
        Mark::Active | Mark::Current => {
            Span::styled(text, Style::new().fg(palette().accent).bold())
        }
        Mark::Failed => Span::styled(text, Style::new().fg(palette().bad).bold()),
        Mark::Waiting => muted(text),
    };
    let mut spans = vec![icon, Span::raw(" "), label];
    if let Some(detail) = step.detail.as_ref().filter(|_| !short) {
        spans.push(muted(format!(" {detail}")));
    }
    spans
}

fn line(steps: &[Step], tick: usize, short: impl Fn(&Step) -> bool) -> Line<'static> {
    let mut all = vec![Span::raw(" ")];
    for (index, step) in steps.iter().enumerate() {
        if index > 0 {
            all.push(muted(SEPARATOR));
        }
        all.extend(spans(index + 1, step, short(step), tick));
    }
    Line::from(all)
}

/// The step line for `width` cells: every label when they fit, otherwise only the labels of steps
/// that need attention, with numbers for the rest.
pub fn render(steps: &[Step], width: usize, tick: usize) -> Line<'static> {
    let full = line(steps, tick, |_| false);
    if full.width() <= width {
        return full;
    }
    line(steps, tick, |step| {
        matches!(step.mark, Mark::Done | Mark::Waiting)
    })
}

/// Cells the full step line takes.
pub fn width(steps: &[Step]) -> usize {
    line(steps, 0, |_| false).width()
}
