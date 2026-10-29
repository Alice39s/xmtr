//! The panel drawn while the run is in progress: the step line, one progress bar per route and the
//! progress of the current step.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme::{bar, border_text, muted, paint, palette, panel_block, spinner, sweep};
use super::{Options, steps};
use crate::board::{Phase, Row, Stage, State};
use crate::convergence::STABLE_ROUNDS;
use crate::runner::protocol_name;

/// Panel rows besides the route rows: two borders, the step line, two spacers and the footer.
pub const CHROME_ROWS: u16 = 6;
/// The panel spans the route rows on wide terminals, but never less than this, so the step line and
/// footer fit.
pub const MIN_WIDTH: u16 = 76;
const BAR_MAX: usize = 20;
const BAR_MIN: usize = 6;
const STATUS_WIDTH: usize = 18;
const PROTOCOL_WIDTH: usize = 11;

/// How far a route is from being stable: rounds against the rounds a stable route needs, or the time
/// limit, whichever is closer.
pub fn route_ratio(row: &Row, options: &Options) -> f64 {
    let needed = (options.min_rounds + STABLE_ROUNDS) as f64;
    let rounds = row.rounds as f64 / needed;
    let time = row.started.map_or(0.0, |started| {
        started.elapsed().as_secs_f64() / options.time_limit.as_secs_f64()
    });
    rounds.max(time).min(0.95)
}

fn icon(row: &Row, tick: usize) -> Span<'static> {
    match row.stage {
        Stage::Resolving | Stage::Tracing => spinner(tick),
        Stage::Queued => muted("·"),
        Stage::Done if row.reached => paint("✓", palette().good),
        Stage::Done => paint("✗", palette().bad),
    }
}

fn protocol(row: &Row) -> String {
    if row.stage == Stage::Resolving {
        return String::new();
    }
    let name = protocol_name(row.protocol).to_uppercase();
    if row.fallback {
        format!("v{} ICMP→{name}", row.family)
    } else {
        format!("v{} {name}", row.family)
    }
}

fn status(row: &Row) -> Vec<Span<'static>> {
    match row.stage {
        Stage::Resolving => vec![muted("resolving")],
        Stage::Queued => vec![muted("queued")],
        Stage::Tracing if row.reached => {
            vec![
                paint("reached", palette().good),
                muted(format!(" · round {}", row.rounds)),
            ]
        }
        Stage::Tracing => vec![muted(format!("round {}", row.rounds))],
        Stage::Done => {
            let time = format!(" · {:.1}s", row.elapsed.as_secs_f64());
            if row.reached {
                vec![muted(format!("{} rounds{time}", row.rounds))]
            } else {
                vec![paint("not reached", palette().bad), muted(time)]
            }
        }
    }
}

fn route_bar(
    row: &Row,
    width: usize,
    options: &Options,
    tick: usize,
    index: usize,
) -> Vec<Span<'static>> {
    match row.stage {
        Stage::Resolving | Stage::Queued => bar(width, 0.0, palette().accent, None),
        Stage::Done if row.reached => bar(width, 1.0, palette().good, None),
        Stage::Done => bar(width, 1.0, palette().bad, None),
        Stage::Tracing => {
            let ratio = route_ratio(row, options);
            let filled = ((width as f64) * ratio).round() as usize;
            let glint = (filled > 0).then(|| (tick + index * 3) % (filled + 4));
            let color = if row.fallback {
                palette().warn
            } else {
                palette().accent
            };
            bar(width, ratio, color, glint)
        }
    }
}

/// Column widths for a panel `width` cells wide inside its borders.
pub struct Columns {
    target: usize,
    pub bar: usize,
}

impl Columns {
    /// Cells a route row needs.
    const fn width(&self) -> usize {
        let bar = if self.bar > 0 { self.bar + 2 } else { 0 };
        3 + self.target + 1 + PROTOCOL_WIDTH + 1 + bar + STATUS_WIDTH
    }
}

pub fn columns(width: usize, rows: &[Row]) -> Columns {
    let longest = rows
        .iter()
        .map(|row| row.target.chars().count())
        .max()
        .unwrap_or(0);
    let target = longest.clamp(8, 28);
    // The bar also takes two cells of spacing before the status.
    let fixed = Columns { target, bar: 0 }.width() + 2;
    let bar = width.saturating_sub(fixed).min(BAR_MAX);
    Columns {
        target,
        bar: if bar < BAR_MIN { 0 } else { bar },
    }
}

fn route_line(
    row: &Row,
    columns: &Columns,
    options: &Options,
    tick: usize,
    index: usize,
) -> Line<'static> {
    let target: String = row.target.chars().take(columns.target).collect();
    let mut spans = vec![
        Span::raw(" "),
        icon(row, tick),
        Span::raw(" "),
        Span::raw(format!("{target:<width$} ", width = columns.target)),
        muted(format!("{:<PROTOCOL_WIDTH$} ", protocol(row))),
    ];
    if columns.bar > 0 {
        spans.extend(route_bar(row, columns.bar, options, tick, index));
        spans.push(Span::raw("  "));
    }
    spans.extend(status(row));
    Line::from(spans)
}

/// Rows that fit in `room` lines; running routes come first when not all fit, the rest is counted.
fn visible(rows: &[Row], room: usize) -> (Vec<(usize, &Row)>, usize) {
    if rows.len() <= room {
        return (rows.iter().enumerate().collect(), 0);
    }
    let shown = room.saturating_sub(1);
    let mut order: Vec<(usize, &Row)> = rows.iter().enumerate().collect();
    order.sort_by_key(|(index, row)| {
        let rank = match row.stage {
            Stage::Tracing | Stage::Resolving => 0,
            Stage::Queued => 1,
            Stage::Done => 2,
        };
        (rank, *index)
    });
    order.truncate(shown);
    order.sort_by_key(|(index, _)| *index);
    (order, rows.len() - shown)
}

fn footer(state: &State, options: &Options, tick: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    match state.phase {
        Phase::Resolve => spans.push(muted(format!("Resolving {} targets", state.rows.len()))),
        Phase::Trace => {
            let count = |stage: Stage| state.rows.iter().filter(|row| row.stage == stage).count();
            let reached = state.rows.iter().filter(|row| row.reached).count();
            let mut parts = vec![format!("{} tracing", count(Stage::Tracing))];
            let queued = count(Stage::Queued);
            if queued > 0 {
                parts.push(format!("{queued} queued"));
            }
            parts.push(format!("{reached} reached"));
            spans.push(muted(parts.join(" · ")));
        }
        Phase::Lookup => {
            let (answered, total) = state.lookup;
            spans.push(Span::raw("Reverse DNS and AS  "));
            let ratio = if total == 0 {
                0.0
            } else {
                answered as f64 / total as f64
            };
            spans.extend(bar(BAR_MAX, ratio, palette().accent, None));
            spans.push(muted(format!("  {answered}/{total} addresses")));
        }
        Phase::Upload => {
            let host = options.upload.as_deref().unwrap_or_default();
            spans.push(Span::raw("Uploading to "));
            spans.push(Span::raw(host.to_owned()).bold());
            spans.push(Span::raw("  "));
            spans.extend(sweep(BAR_MAX, tick));
        }
        Phase::Result => {}
    }
    Line::from(spans)
}

pub fn render(frame: &mut Frame<'_>, state: &State, options: &Options, elapsed: f64, tick: usize) {
    let full = frame.area();
    let content = columns(usize::from(full.width.saturating_sub(2)), &state.rows).width();
    let area = Rect {
        width: full.width.min(
            u16::try_from(content + 2)
                .unwrap_or(u16::MAX)
                .max(MIN_WIDTH),
        ),
        ..full
    };
    let inner_width = usize::from(area.width.saturating_sub(2));
    let room = usize::from(area.height.saturating_sub(CHROME_ROWS));
    let columns = columns(inner_width, &state.rows);
    let (shown, hidden) = visible(&state.rows, room);

    let steps = steps::steps(state, options, false);
    let mut lines = vec![steps::render(&steps, inner_width, tick), Line::raw("")];
    lines.extend(
        shown
            .iter()
            .map(|(index, row)| route_line(row, &columns, options, tick, *index)),
    );
    if hidden > 0 {
        lines.push(Line::from(muted(format!("   +{hidden} more"))));
    }
    lines.push(Line::raw(""));
    lines.push(footer(state, options, tick));

    let limits = format!(
        "stable after {} rounds · limit {}",
        options.min_rounds,
        humantime::format_duration(options.time_limit)
    );
    let block = panel_block(border_text(&format!("{elapsed:.1}s")), border_text(&limits));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use trippy_core::Protocol;

    use super::*;
    use crate::live::tests::{options, screen};

    fn row(target: &str, stage: Stage, reached: bool) -> Row {
        Row {
            target: target.to_owned(),
            family: 4,
            protocol: Protocol::Icmp,
            fallback: false,
            stage,
            rounds: 3,
            reached,
            started: None,
            elapsed: Duration::from_millis(5200),
        }
    }

    fn draw(state: &State, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        terminal
            .draw(|frame| render(frame, state, &options(), 3.0, 0))
            .expect("draw");
        screen(terminal.backend().buffer())
    }

    #[test]
    fn shows_numbered_steps_routes_and_lookup_progress() {
        let state = State {
            phase: Phase::Lookup,
            rows: vec![
                row("1.1.1.1", Stage::Done, true),
                row("example.org", Stage::Done, false),
            ],
            lookup: (3, 8),
        };
        let screen = draw(&state, 80, 2 + CHROME_ROWS);
        assert!(screen.contains("✓ 1. Resolve ─ ✓ 2. Trace ─"), "{screen}");
        assert!(screen.contains("3. Lookup 3/8"), "{screen}");
        assert!(screen.contains("○ 4. Upload ─ ○ 5. Result"), "{screen}");
        assert!(screen.contains("3 rounds · 5.2s"), "{screen}");
        assert!(screen.contains("not reached"), "{screen}");
        assert!(screen.contains("3/8 addresses"), "{screen}");
    }

    #[test]
    fn narrow_panels_keep_only_the_running_step_label() {
        let state = State {
            phase: Phase::Trace,
            rows: vec![row("1.1.1.1", Stage::Tracing, false)],
            lookup: (0, 0),
        };
        let screen = draw(&state, 50, 1 + CHROME_ROWS);
        assert!(screen.contains("✓ 1 ─"), "{screen}");
        assert!(
            screen.contains("2. Trace 0/1 ─ ○ 3 ─ ○ 4 ─ ○ 5"),
            "{screen}"
        );
    }

    #[test]
    fn counts_routes_that_do_not_fit() {
        let mut rows: Vec<Row> = (0..6)
            .map(|n| row(&format!("10.0.0.{n}"), Stage::Done, true))
            .collect();
        rows[4].stage = Stage::Tracing;
        let state = State {
            phase: Phase::Trace,
            rows,
            lookup: (0, 0),
        };
        let screen = draw(&state, 80, 3 + CHROME_ROWS);
        assert!(screen.contains("10.0.0.0"), "{screen}");
        assert!(screen.contains("10.0.0.4"), "{screen}");
        assert!(!screen.contains("10.0.0.1"), "{screen}");
        assert!(screen.contains("+4 more"), "{screen}");
    }

    #[test]
    fn narrow_terminals_drop_the_bars() {
        assert_eq!(columns(40, &[row("1.1.1.1", Stage::Done, true)]).bar, 0);
        assert_eq!(
            columns(78, &[row("1.1.1.1", Stage::Done, true)]).bar,
            BAR_MAX
        );
    }
}
