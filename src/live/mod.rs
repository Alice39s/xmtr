//! Live status drawn on stderr while xmtr works, in one of three views:
//!
//! - the panel: an inline viewport below the prompt with numbered steps, one progress bar per route
//!   and the progress of the current step; when the run ends it gives way to the result panel, or is
//!   erased when the caller prints the results itself;
//! - the line view, for terminals too small for the panel or without cursor position reports:
//!   finished steps and routes as single lines and one status line redrawn in place;
//! - the plain view, for pipes, logs and `TERM=dumb`: one tagged text line per step and route.

mod events;
mod line;
mod plain;
mod progress;
mod result;
mod steps;
mod theme;

use std::env;
use std::io::{self, IsTerminal, Stderr};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui::backend::{CrosstermBackend, IntoCrossterm};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::crossterm::{cursor, execute, terminal};
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};

use self::line::LineView;
use self::plain::PlainView;
pub use self::result::{Summary, Upload};
use crate::board::{Board, Phase, State};
use crate::cli::{ProgressChoice, ThemeChoice};

const FRAME: Duration = Duration::from_millis(80);
const MAX_ROUTE_ROWS: u16 = 12;
/// The smallest terminal the panel is drawn in: a route row without its bar, inside the borders.
const PANEL_MIN_WIDTH: u16 = 44;
/// The panel chrome, one route row and the row the cursor returns to.
const PANEL_MIN_HEIGHT: u16 = progress::CHROME_ROWS + 2;

/// What the panel shows besides the board.
#[derive(Debug, Clone)]
pub struct Options {
    pub lookup: bool,
    pub upload: Option<String>,
    pub min_rounds: usize,
    pub time_limit: Duration,
}

pub struct Live {
    done: Sender<Option<Summary>>,
    handle: JoinHandle<Result<Option<String>>>,
    panel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Panel,
    Line,
    Plain,
}

/// The view for `choice`; `auto` takes the panel when the terminal is big enough, the line view on
/// smaller terminals and plain lines when stderr is not a terminal or `TERM=dumb`.
fn pick(choice: ProgressChoice) -> Kind {
    match choice {
        ProgressChoice::Panel => Kind::Panel,
        ProgressChoice::Line => Kind::Line,
        ProgressChoice::Plain => Kind::Plain,
        ProgressChoice::Auto => {
            if !io::stderr().is_terminal() || env::var("TERM").is_ok_and(|term| term == "dumb") {
                return Kind::Plain;
            }
            match terminal::size() {
                Ok((width, height)) if width >= PANEL_MIN_WIDTH && height >= PANEL_MIN_HEIGHT => {
                    Kind::Panel
                }
                _ => Kind::Line,
            }
        }
    }
}

enum View {
    Panel(Panel),
    Line(LineView),
    Plain(PlainView),
}

impl Live {
    /// Starts showing `board` on stderr in the view `progress` asks for. In the panel and line
    /// views Ctrl-C erases the live output and exits with status 130. Fails when a view chosen
    /// explicitly cannot run, for example a panel on a terminal that does not report the cursor
    /// position; `auto` then falls back to the line view.
    pub fn start(
        board: Arc<Board>,
        options: Options,
        progress: ProgressChoice,
        theme: ThemeChoice,
    ) -> Result<Self> {
        let kind = pick(progress);
        let raw = kind != Kind::Plain && io::stderr().is_terminal();
        if kind != Kind::Plain {
            // Asks the terminal for its background, which needs the terminal to itself.
            theme::select(theme);
        }
        if raw {
            // Raw mode keeps typed keys from scribbling over the view; Ctrl-C then arrives as a key.
            terminal::enable_raw_mode().context("switching the terminal to raw mode")?;
        }
        let view = match kind {
            Kind::Panel => match Panel::open(route_rows(board.snapshot().rows.len())) {
                Ok(panel) => View::Panel(panel),
                Err(_) if progress == ProgressChoice::Auto => View::Line(LineView::default()),
                Err(error) => {
                    restore();
                    return Err(error);
                }
            },
            Kind::Line => View::Line(LineView::default()),
            Kind::Plain => View::Plain(PlainView::new()),
        };
        if matches!(view, View::Line(_)) {
            execute!(io::stderr(), cursor::Hide)?;
        }
        let panel = matches!(view, View::Panel(_));
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous_hook(info);
        }));
        let (done, finished) = mpsc::channel();
        let handle = thread::spawn(move || {
            let result = draw_until(view, raw, &board, &options, &finished);
            if raw {
                restore();
            }
            result
        });
        Ok(Self {
            done,
            handle,
            panel,
        })
    }

    /// True when the view shows the result panel; otherwise the caller prints the results.
    pub const fn shows_result(&self) -> bool {
        self.panel
    }

    /// Ends the live view: with a summary the result panel takes its place, without one it is
    /// erased. Returns a link the result panel had no room for.
    pub fn finish(self, summary: Option<Summary>) -> Result<Option<String>> {
        self.done
            .send(summary)
            .context("the live view stopped early")?;
        self.handle
            .join()
            .map_err(|_| anyhow::anyhow!("the live view panicked"))?
    }
}

/// The accent color of the palette in use, for text printed beside the panel.
pub fn accent() -> ratatui::crossterm::style::Color {
    theme::palette().accent.into_crossterm()
}

type Term = Terminal<CrosstermBackend<Stderr>>;

/// An inline viewport and the area it occupies on screen.
struct Panel {
    terminal: Term,
    area: Rect,
    rows: usize,
}

impl Panel {
    fn open(rows: usize) -> Result<Self> {
        let height = u16::try_from(rows).unwrap_or(MAX_ROUTE_ROWS) + progress::CHROME_ROWS;
        let terminal = Terminal::with_options(
            CrosstermBackend::new(io::stderr()),
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )
        .context("opening the live view")?;
        Ok(Self {
            terminal,
            area: Rect::default(),
            rows,
        })
    }

    fn draw(&mut self, state: &State, options: &Options, elapsed: f64, tick: usize) -> Result<()> {
        let mut area = Rect::default();
        synchronized(|| {
            self.terminal.draw(|frame| {
                area = frame.area();
                progress::render(frame, state, options, elapsed, tick);
            })?;
            Ok(())
        })?;
        self.area = area;
        Ok(())
    }

    /// Prints the result panel above the viewport, which scrolls into the terminal's history.
    /// Returns the link when it is too wide for the panel, for the caller to print as plain text
    /// the terminal can wrap.
    fn print_result(
        &mut self,
        summary: &Summary,
        state: &State,
        options: &Options,
        elapsed: f64,
    ) -> Result<Option<String>> {
        let rendered = result::render(summary, state, options, elapsed, self.area.width);
        let block = theme::panel_block(rendered.top_right.clone(), rendered.bottom_right.clone());
        let width = rendered.width;
        let height = rendered.height();
        let lines = rendered.lines;
        let mut area = Rect::default();
        synchronized(|| {
            self.terminal.insert_before(height, |buffer| {
                let area = Rect {
                    width: width.min(buffer.area.width),
                    ..buffer.area
                };
                Paragraph::new(lines).block(block).render(area, buffer);
            })?;
            // The viewport moved down; take its new place before erasing it.
            self.terminal.draw(|frame| area = frame.area())?;
            Ok(())
        })?;
        self.area = area;
        Ok(rendered.link_below)
    }

    /// Clears the viewport and leaves the cursor where it started, so later output takes its place.
    fn close(&mut self) -> Result<()> {
        self.terminal.clear()?;
        self.terminal.set_cursor_position(self.area.as_position())?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}

/// Runs `paint` inside a synchronized update, so terminals show the whole repaint at once instead
/// of a half drawn panel.
fn synchronized(paint: impl FnOnce() -> Result<()>) -> Result<()> {
    execute!(io::stderr(), BeginSynchronizedUpdate)?;
    let painted = paint();
    execute!(io::stderr(), EndSynchronizedUpdate)?;
    painted
}

fn restore() {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stderr(), cursor::Show);
}

/// Route rows that fit: every route on a tall terminal, fewer plus a summary row on a short one.
fn route_rows(count: usize) -> usize {
    let height = terminal::size().map_or(24, |(_, rows)| rows);
    let fit = height
        .saturating_sub(progress::CHROME_ROWS + 1)
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

impl View {
    fn draw(&mut self, state: &State, options: &Options, elapsed: f64, tick: usize) -> Result<()> {
        match self {
            Self::Panel(panel) => {
                let rows = route_rows(state.rows.len());
                if rows != panel.rows {
                    panel.close()?;
                    *panel = Panel::open(rows)?;
                }
                panel.draw(state, options, elapsed, tick)
            }
            Self::Line(line) => Ok(line.draw(state, options, elapsed, tick)?),
            Self::Plain(plain) => Ok(plain.draw(state, options, elapsed)?),
        }
    }

    fn close(&mut self) -> Result<()> {
        match self {
            Self::Panel(panel) => panel.close(),
            Self::Line(line) => Ok(line.close()?),
            Self::Plain(_) => Ok(()),
        }
    }
}

fn draw_until(
    mut view: View,
    raw: bool,
    board: &Board,
    options: &Options,
    finished: &Receiver<Option<Summary>>,
) -> Result<Option<String>> {
    let started = Instant::now();
    let mut tick = 0usize;
    loop {
        let mut state = board.snapshot();
        let elapsed = started.elapsed().as_secs_f64();
        view.draw(&state, options, elapsed, tick)?;
        match finished.try_recv() {
            Ok(Some(summary)) => {
                if let View::Panel(panel) = &mut view {
                    state.phase = Phase::Result;
                    let link = panel.print_result(&summary, &state, options, elapsed)?;
                    panel.close()?;
                    return Ok(link);
                }
                view.close()?;
                return Ok(None);
            }
            Ok(None) | Err(TryRecvError::Disconnected) => {
                view.close()?;
                return Ok(None);
            }
            Err(TryRecvError::Empty) => {}
        }
        if raw && ctrl_c_pressed()? {
            view.close()?;
            restore();
            std::process::exit(130);
        }
        thread::sleep(FRAME);
        tick += 1;
    }
}

#[cfg(test)]
pub mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Position;

    use super::*;

    pub fn options() -> Options {
        Options {
            lookup: true,
            upload: Some("info-api.example".to_owned()),
            min_rounds: 5,
            time_limit: Duration::from_secs(45),
        }
    }

    /// The text of a buffer, one line per row.
    pub fn screen(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[Position::new(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
