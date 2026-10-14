//! Live status drawn on stderr while xmtr works: a rounded panel with the phases of the run, one
//! progress bar per route and the progress of the current phase. It lives in an inline viewport
//! below the prompt and is erased before the results are printed.

use std::io::{self, Stderr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::{cursor, execute, terminal};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};

use crate::board::{Board, Phase, Row, Stage, State};
use crate::convergence::STABLE_ROUNDS;
use crate::runner::protocol_name;

const FRAME: Duration = Duration::from_millis(80);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;
/// Panel rows besides the route rows: two borders, the phase line, two spacers and the footer.
const CHROME_ROWS: u16 = 6;
const MAX_ROUTE_ROWS: u16 = 12;
const BAR_MAX: usize = 20;
const BAR_MIN: usize = 6;
const STATUS_WIDTH: usize = 18;
const PROTOCOL_WIDTH: usize = 11;
/// The panel spans the route rows on wide terminals, but never less than this, so the phase line and
/// footer fit.
const PANEL_MIN_WIDTH: u16 = 76;

/// What the panel shows besides the board.
#[derive(Debug, Clone)]
pub struct Options {
    pub lookup: bool,
    pub upload: Option<String>,
    pub min_rounds: usize,
    pub time_limit: Duration,
}

pub struct Live {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Result<()>>,
}

impl Live {
    /// Starts drawing `board` on stderr. Ctrl-C erases the panel and exits with status 130.
    pub fn start(board: Arc<Board>, options: Options) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let handle = thread::spawn({
            let stop = Arc::clone(&stop);
            move || run(&board, &options, &stop)
        });
        Self { stop, handle }
    }

    /// Erases the panel and gives the terminal back.
    pub fn finish(self) -> Result<()> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle
            .join()
            .map_err(|_| anyhow::anyhow!("the live view panicked"))?
    }
}

type Term = Terminal<CrosstermBackend<Stderr>>;

/// An inline viewport and the area it occupies on screen.
struct Panel {
    terminal: Term,
    area: Rect,
    rows: usize,
}

fn open(rows: usize) -> Result<Panel> {
    let height = u16::try_from(rows).unwrap_or(MAX_ROUTE_ROWS) + CHROME_ROWS;
    let terminal = Terminal::with_options(
        CrosstermBackend::new(io::stderr()),
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
    .context("opening the live view")?;
    Ok(Panel {
        terminal,
        area: Rect::default(),
        rows,
    })
}

/// Clears the viewport and leaves the cursor where it started, so later output takes its place.
fn close(view: &mut Panel) -> Result<()> {
    view.terminal.clear()?;
    view.terminal.set_cursor_position(view.area.as_position())?;
    view.terminal.show_cursor()?;
    Ok(())
}

fn restore() {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stderr(), cursor::Show);
}

/// Route rows that fit: every route on a tall terminal, fewer plus a summary row on a short one.
fn route_rows(count: usize) -> usize {
    let height = terminal::size().map_or(24, |(_, rows)| rows);
    let fit = height
        .saturating_sub(CHROME_ROWS + 1)
        .clamp(1, MAX_ROUTE_ROWS);
    count.clamp(1, usize::from(fit))
}

fn ctrl_c_pressed() -> Result<bool> {
    while event::poll(Duration::ZERO)? {
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn run(board: &Board, options: &Options, stop: &AtomicBool) -> Result<()> {
    // Raw mode keeps typed keys from scribbling over the panel; Ctrl-C then arrives as a key.
    terminal::enable_raw_mode().context("switching the terminal to raw mode")?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous_hook(info);
    }));
    let result = draw_until(board, options, stop);
    restore();
    result
}

fn draw_until(board: &Board, options: &Options, stop: &AtomicBool) -> Result<()> {
    let started = Instant::now();
    let mut view = open(route_rows(board.snapshot().rows.len()))?;
    let mut tick = 0usize;
    loop {
        let state = board.snapshot();
        let rows = route_rows(state.rows.len());
        if rows != view.rows {
            close(&mut view)?;
            view = open(rows)?;
        }
        let elapsed = started.elapsed();
        let mut area = Rect::default();
        view.terminal.draw(|frame| {
            area = frame.area();
            render(frame, &state, options, elapsed, tick);
        })?;
        view.area = area;
        if stop.load(Ordering::SeqCst) {
            return close(&mut view);
        }
        if ctrl_c_pressed()? {
            close(&mut view)?;
            restore();
            std::process::exit(130);
        }
        thread::sleep(FRAME);
        tick += 1;
    }
}

fn spinner(tick: usize) -> Span<'static> {
    Span::styled(SPINNER[tick % SPINNER.len()], Style::new().fg(ACCENT))
}

fn muted(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::new().fg(MUTED))
}

/// A determinate bar; `glint` lights one filled cell to show the work is still moving.
fn bar(width: usize, ratio: f64, color: Color, glint: Option<usize>) -> Vec<Span<'static>> {
    let filled = ((width as f64) * ratio.clamp(0.0, 1.0)).round() as usize;
    (0..width)
        .map(|index| {
            if index >= filled {
                muted("─")
            } else if glint == Some(index) {
                Span::styled(
                    "━",
                    Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("━", Style::new().fg(color))
            }
        })
        .collect()
}

/// A bar for work of unknown length: a short bright segment sweeping across a dim track.
fn sweep(width: usize, tick: usize) -> Vec<Span<'static>> {
    let head = tick % (width + 4);
    (0..width)
        .map(|index| {
            if index + 4 > head && index <= head {
                Span::styled("━", Style::new().fg(ACCENT))
            } else {
                muted("─")
            }
        })
        .collect()
}

/// How far a route is from being stable: rounds against the rounds a stable route needs, or the time
/// limit, whichever is closer.
fn route_ratio(row: &Row, options: &Options) -> f64 {
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
        Stage::Done if row.reached => "✓".green(),
        Stage::Done => "✗".light_red(),
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
            vec!["reached".green(), muted(format!(" · round {}", row.rounds))]
        }
        Stage::Tracing => vec![muted(format!("round {}", row.rounds))],
        Stage::Done => {
            let time = format!(" · {:.1}s", row.elapsed.as_secs_f64());
            if row.reached {
                vec![muted(format!("{} rounds{time}", row.rounds))]
            } else {
                vec!["not reached".light_red(), muted(time)]
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
        Stage::Resolving | Stage::Queued => bar(width, 0.0, ACCENT, None),
        Stage::Done if row.reached => bar(width, 1.0, Color::Green, None),
        Stage::Done => bar(width, 1.0, Color::LightRed, None),
        Stage::Tracing => {
            let ratio = route_ratio(row, options);
            let filled = ((width as f64) * ratio).round() as usize;
            let glint = (filled > 0).then(|| (tick + index * 3) % (filled + 4));
            let color = if row.fallback { Color::Yellow } else { ACCENT };
            bar(width, ratio, color, glint)
        }
    }
}

/// Column widths for a panel `width` cells wide inside its borders.
struct Columns {
    target: usize,
    bar: usize,
}

impl Columns {
    /// Cells a route row needs.
    const fn width(&self) -> usize {
        let bar = if self.bar > 0 { self.bar + 2 } else { 0 };
        3 + self.target + 1 + PROTOCOL_WIDTH + 1 + bar + STATUS_WIDTH
    }
}

fn columns(width: usize, rows: &[Row]) -> Columns {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepState {
    Done,
    Active,
    Waiting,
}

fn step(label: &str, state: StepState, detail: Option<String>, tick: usize) -> Vec<Span<'static>> {
    let detail = detail.map(|detail| muted(format!(" {detail}")));
    let mut spans = match state {
        StepState::Done => vec!["✓ ".green(), Span::raw(label.to_owned())],
        StepState::Active => vec![
            spinner(tick),
            Span::raw(" "),
            Span::styled(label.to_owned(), Style::new().fg(ACCENT).bold()),
        ],
        StepState::Waiting => return vec![muted(format!("○ {label}"))],
    };
    spans.extend(detail);
    spans
}

fn phase_line(state: &State, options: &Options, tick: usize) -> Line<'static> {
    let done = state
        .rows
        .iter()
        .filter(|row| row.stage == Stage::Done)
        .count();
    let (answered, total) = state.lookup;
    let mut steps = vec![(Phase::Resolve, "Resolve", None)];
    let traced = (state.phase >= Phase::Trace).then(|| format!("{done}/{}", state.rows.len()));
    steps.push((Phase::Trace, "Trace", traced));
    if options.lookup {
        let looked =
            (state.phase >= Phase::Lookup && total > 0).then(|| format!("{answered}/{total}"));
        steps.push((Phase::Lookup, "Lookup", looked));
    }
    if options.upload.is_some() {
        steps.push((Phase::Upload, "Upload", None));
    }
    let mut spans = vec![Span::raw(" ")];
    for (position, (phase, label, detail)) in steps.into_iter().enumerate() {
        if position > 0 {
            spans.push(muted("  ──  "));
        }
        let state = match phase.cmp(&state.phase) {
            std::cmp::Ordering::Less => StepState::Done,
            std::cmp::Ordering::Equal => StepState::Active,
            std::cmp::Ordering::Greater => StepState::Waiting,
        };
        spans.extend(step(label, state, detail, tick));
    }
    Line::from(spans)
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
            spans.extend(bar(BAR_MAX, ratio, ACCENT, None));
            spans.push(muted(format!("  {answered}/{total} addresses")));
        }
        Phase::Upload => {
            let host = options.upload.as_deref().unwrap_or_default();
            spans.push(Span::raw("Uploading to "));
            spans.push(Span::raw(host.to_owned()).bold());
            spans.push(Span::raw("  "));
            spans.extend(sweep(BAR_MAX, tick));
        }
    }
    Line::from(spans)
}

fn render(frame: &mut Frame<'_>, state: &State, options: &Options, elapsed: Duration, tick: usize) {
    let full = frame.area();
    let content = columns(usize::from(full.width.saturating_sub(2)), &state.rows).width();
    let area = Rect {
        width: full.width.min(
            u16::try_from(content + 2)
                .unwrap_or(u16::MAX)
                .max(PANEL_MIN_WIDTH),
        ),
        ..full
    };
    let inner_width = usize::from(area.width.saturating_sub(2));
    let room = usize::from(area.height.saturating_sub(CHROME_ROWS));
    let columns = columns(inner_width, &state.rows);
    let (shown, hidden) = visible(&state.rows, room);

    let mut lines = vec![phase_line(state, options, tick), Line::raw("")];
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
        " stable after {} rounds · limit {} ",
        options.min_rounds,
        humantime::format_duration(options.time_limit)
    );
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(MUTED))
        .title_top(Line::from(vec![
            Span::raw(" "),
            Span::styled("xmtr", Style::new().fg(ACCENT).bold()),
            Span::raw(" "),
        ]))
        .title_top(Line::from(muted(format!(" {:.1}s ", elapsed.as_secs_f64()))).right_aligned())
        .title_bottom(Line::from(muted(limits)).right_aligned());
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Position;
    use trippy_core::Protocol;

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

    fn options() -> Options {
        Options {
            lookup: true,
            upload: Some("info-api.example".to_owned()),
            min_rounds: 5,
            time_limit: Duration::from_secs(45),
        }
    }

    fn draw(state: &State, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        terminal
            .draw(|frame| render(frame, state, &options(), Duration::from_secs(3), 0))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[Position::new(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn shows_phases_routes_and_lookup_progress() {
        let state = State {
            phase: Phase::Lookup,
            rows: vec![
                row("1.1.1.1", Stage::Done, true),
                row("example.org", Stage::Done, false),
            ],
            lookup: (3, 8),
        };
        let screen = draw(&state, 80, 2 + CHROME_ROWS);
        assert!(screen.contains("✓ Resolve"), "{screen}");
        assert!(screen.contains("✓ Trace 2/2"), "{screen}");
        assert!(screen.contains("Lookup 3/8"), "{screen}");
        assert!(screen.contains("○ Upload"), "{screen}");
        assert!(screen.contains("3 rounds · 5.2s"), "{screen}");
        assert!(screen.contains("not reached"), "{screen}");
        assert!(screen.contains("3/8 addresses"), "{screen}");
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
