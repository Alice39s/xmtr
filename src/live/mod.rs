//! Live status drawn on stderr while xmtr works, in an inline viewport below the prompt: a rounded
//! panel with numbered steps, one progress bar per route and the progress of the current step. When
//! the run ends, the panel gives way to the result panel, or is erased when the caller prints the
//! results itself.

mod progress;
mod result;
mod steps;
mod theme;

use std::io::{self, Stderr};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::crossterm::{cursor, execute, terminal};
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};

pub use self::result::{Summary, Upload};
use crate::board::{Board, Phase, State};

const FRAME: Duration = Duration::from_millis(80);
const MAX_ROUTE_ROWS: u16 = 12;

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
}

impl Live {
    /// Starts drawing `board` on stderr. Ctrl-C erases the panel and exits with status 130. Fails
    /// when the terminal cannot host the panel, for example when it does not report the cursor
    /// position.
    pub fn start(board: Arc<Board>, options: Options) -> Result<Self> {
        // Raw mode keeps typed keys from scribbling over the panel; Ctrl-C then arrives as a key.
        terminal::enable_raw_mode().context("switching the terminal to raw mode")?;
        let panel = match Panel::open(route_rows(board.snapshot().rows.len())) {
            Ok(panel) => panel,
            Err(error) => {
                restore();
                return Err(error);
            }
        };
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous_hook(info);
        }));
        let (done, finished) = mpsc::channel();
        let handle = thread::spawn(move || {
            let result = draw_until(panel, &board, &options, &finished);
            restore();
            result
        });
        Ok(Self { done, handle })
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

fn draw_until(
    mut panel: Panel,
    board: &Board,
    options: &Options,
    finished: &Receiver<Option<Summary>>,
) -> Result<Option<String>> {
    let started = Instant::now();
    let mut tick = 0usize;
    loop {
        let mut state = board.snapshot();
        let rows = route_rows(state.rows.len());
        if rows != panel.rows {
            panel.close()?;
            panel = Panel::open(rows)?;
        }
        let elapsed = started.elapsed().as_secs_f64();
        panel.draw(&state, options, elapsed, tick)?;
        match finished.try_recv() {
            Ok(Some(summary)) => {
                state.phase = Phase::Result;
                let link = panel.print_result(&summary, &state, options, elapsed)?;
                panel.close()?;
                return Ok(link);
            }
            Ok(None) | Err(TryRecvError::Disconnected) => {
                panel.close()?;
                return Ok(None);
            }
            Err(TryRecvError::Empty) => {}
        }
        if ctrl_c_pressed()? {
            panel.close()?;
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
