//! Messages printed outside the live views: notes beside the route tables, problems that end a
//! run, and the update offer after it. On a terminal they use the palette, glyphs and panel frame of
//! the live views; elsewhere they are plain text lines.

use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::backend::IntoCrossterm;
use ratatui::buffer::Buffer;
use ratatui::crossterm::cursor::MoveToColumn;
use ratatui::crossterm::queue;
use ratatui::crossterm::style::{Print, PrintStyledContent, StyledContent};
use ratatui::crossterm::terminal::{self, Clear, ClearType};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::theme::{
    self, CALLOUT, INDENT, border_text, callout, muted, paint, palette, panel_block, spinner,
    wrapped,
};
use super::{FRAME, progress, stderr_is_plain};
use crate::cli::{ProgressChoice, ThemeChoice};

/// A remark about a run, shown with its routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// A target or address left out of the run, and why.
    Skipped(String),
    /// A route that produced no result, and why.
    Failed(String),
    /// Addresses without a reverse DNS or AS answer in time.
    Unanswered(usize),
}

impl Note {
    fn unanswered(count: usize) -> String {
        format!("no reverse DNS or AS answer in time for {count} addresses")
    }

    fn plain(&self) -> String {
        match self {
            Self::Skipped(text) => format!("skip {text}"),
            Self::Failed(text) => format!("error {text}"),
            Self::Unanswered(count) => format!("note: {}", Self::unanswered(*count)),
        }
    }

    /// The note behind its icon, wrapped to `inner` columns.
    pub fn lines(&self, inner: usize) -> Vec<Line<'static>> {
        let (icon, text, style) = match self {
            Self::Skipped(text) => (
                paint("!", palette().warn),
                format!("skipped {text}"),
                Style::new(),
            ),
            Self::Failed(text) => (paint("✗", palette().bad), text.clone(), Style::new()),
            Self::Unanswered(count) => (
                muted("·"),
                Self::unanswered(*count),
                Style::new().fg(palette().muted),
            ),
        };
        wrapped(&text, inner.saturating_sub(INDENT.len() + 2))
            .into_iter()
            .enumerate()
            .map(|(index, part)| {
                let lead = if index == 0 {
                    icon.clone()
                } else {
                    Span::raw(" ")
                };
                Line::from(vec![
                    Span::raw(INDENT),
                    lead,
                    Span::raw(" "),
                    Span::styled(part, style),
                ])
            })
            .collect()
    }
}

/// Reads the reply to a `[Y/n]` question: an empty line, `y` or `yes` means yes, `n` or `no`
/// means no, in any letter case.
pub fn answer(reply: &str) -> Option<bool> {
    match reply.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Some(true),
        "n" | "no" => Some(false),
        _ => None,
    }
}

/// Columns for messages: one less than the terminal, so the cursor never wraps to the next row.
fn columns() -> usize {
    terminal::size()
        .map_or(80, |(width, _)| usize::from(width))
        .saturating_sub(1)
        .max(1)
}

/// Writes to stderr; like `eprintln!`, a stderr that cannot be written to is fatal.
fn emit(write: impl FnOnce(&mut io::StderrLock<'static>) -> io::Result<()>) {
    let mut out = io::stderr().lock();
    write(&mut out)
        .and_then(|()| out.flush())
        .unwrap_or_else(|error| panic!("failed printing to stderr: {error}"));
}

fn write_styled(out: &mut impl Write, style: Style, text: &str) -> io::Result<()> {
    queue!(
        out,
        PrintStyledContent(StyledContent::new(style.into_crossterm(), text))
    )
}

/// Writes the cells of `buffer` with their styles, one row per line.
fn write_buffer(out: &mut impl Write, buffer: &Buffer) -> io::Result<()> {
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        let mut run = String::new();
        let mut style = Style::new();
        // Cells covered by the right half of a wide character.
        let mut covered = 0;
        for x in area.left()..area.right() {
            if covered > 0 {
                covered -= 1;
                continue;
            }
            let cell = &buffer[Position::new(x, y)];
            covered = cell.symbol().width().saturating_sub(1);
            if cell.style() != style {
                write_styled(out, style, &run)?;
                run.clear();
                style = cell.style();
            }
            run.push_str(cell.symbol());
        }
        write_styled(out, style, &run)?;
        queue!(out, Print("\n"))?;
    }
    Ok(())
}

/// The panel for a problem: `title` in the error color, then `details` and `notes`.
fn problem_panel(title: &str, details: &[String], notes: &[Note], columns: usize) -> Buffer {
    let width = columns.min(usize::from(progress::MIN_WIDTH));
    let inner = width.saturating_sub(2);
    let text = inner.saturating_sub(INDENT.len() + CALLOUT.chars().count());
    let bad = palette().bad;
    let mut lines: Vec<Line<'static>> = wrapped(title, text)
        .into_iter()
        .map(|part| callout(bad, vec![paint(part, bad).bold()]))
        .collect();
    lines.extend(
        details
            .iter()
            .flat_map(|detail| wrapped(detail, text))
            .map(|part| callout(bad, vec![muted(part)])),
    );
    if !notes.is_empty() {
        lines.push(Line::raw(""));
        lines.extend(notes.iter().flat_map(|note| note.lines(inner)));
    }
    let area = Rect::new(
        0,
        0,
        u16::try_from(width).unwrap_or(u16::MAX),
        u16::try_from(lines.len() + 2).unwrap_or(u16::MAX),
    );
    let mut buffer = Buffer::empty(area);
    Paragraph::new(lines)
        .block(panel_block(
            Line::raw(""),
            border_text(&format!("v{}", env!("CARGO_PKG_VERSION"))),
        ))
        .render(area, &mut buffer);
    buffer
}

/// A problem as plain text: the notes, then the error with its causes.
fn problem_text(error: &anyhow::Error, notes: &[Note]) -> Vec<String> {
    let mut lines: Vec<String> = notes.iter().map(Note::plain).collect();
    lines.push(format!("error: {error:#}"));
    lines
}

/// Where messages go: stderr in the style of the live views, or plain text lines.
#[derive(Debug, Clone, Copy)]
pub struct Console {
    styled: bool,
    theme: ThemeChoice,
}

impl Console {
    /// Styled on a terminal unless `--quiet` or `--progress plain` asked for less, as the live
    /// views are.
    pub fn new(quiet: bool, progress: ProgressChoice, theme: ThemeChoice) -> Self {
        Self {
            styled: !quiet && progress != ProgressChoice::Plain && !stderr_is_plain(),
            theme,
        }
    }

    /// Picks the palette before the first styled message, unless a live view already did.
    fn palette(self) {
        if self.styled {
            theme::select(self.theme);
        }
    }

    fn write(self, out: &mut impl Write, spans: &[Span<'static>]) -> io::Result<()> {
        for span in spans {
            if self.styled {
                write_styled(out, span.style, &span.content)?;
            } else {
                write!(out, "{}", span.content)?;
            }
        }
        Ok(())
    }

    fn lines(self, lines: &[Line<'static>]) {
        emit(|out| {
            for line in lines {
                self.write(out, &line.spans)?;
                writeln!(out)?;
            }
            Ok(())
        });
    }

    /// Prints the notes of a run.
    pub fn notes(self, notes: &[Note]) {
        if !self.styled {
            for note in notes {
                eprintln!("{}", note.plain());
            }
            return;
        }
        self.palette();
        let lines: Vec<Line<'static>> = notes
            .iter()
            .flat_map(|note| note.lines(columns()))
            .collect();
        self.lines(&lines);
    }

    /// Prints a remark that is neither a note nor a problem.
    pub fn remark(self, text: &str) {
        if !self.styled {
            eprintln!("note: {text}");
            return;
        }
        self.palette();
        self.lines(&[Line::from(vec![
            paint("!", palette().warn),
            Span::raw(" "),
            Span::raw(text.to_owned()),
        ])]);
    }

    /// Prints a problem that ends the run, with the `notes` that led to it. On a terminal this is
    /// a panel headed by `title`, or by the error with its causes below; otherwise the notes and an
    /// `error:` line.
    pub fn problem(self, error: &anyhow::Error, title: Option<&str>, notes: &[Note]) {
        if !self.styled {
            for line in problem_text(error, notes) {
                eprintln!("{line}");
            }
            return;
        }
        self.palette();
        let panel = match title {
            Some(title) => problem_panel(title, &[format!("{error:#}")], notes, columns()),
            None => {
                let causes: Vec<String> = error.chain().skip(1).map(ToString::to_string).collect();
                problem_panel(&error.to_string(), &causes, notes, columns())
            }
        };
        emit(|out| write_buffer(out, &panel));
    }

    /// Runs `work` on its own thread and returns its result, or `None` once `limit` has passed.
    /// A styled console shows `label` behind a spinner meanwhile.
    pub fn wait<T: Send + 'static>(
        self,
        label: &str,
        limit: Option<Duration>,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        self.palette();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            // The receiver is gone once the wait gave up; the result is no longer needed then.
            let _ = sender.send(work());
        });
        let started = Instant::now();
        let mut tick = 0;
        let result = loop {
            if self.styled {
                let line = [spinner(tick), Span::raw(" "), muted(label.to_owned())];
                emit(|out| {
                    queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
                    self.write(out, &line)
                });
            }
            let left = limit.map_or(FRAME, |limit| limit.saturating_sub(started.elapsed()));
            match receiver.recv_timeout(left.min(FRAME)) {
                Ok(value) => break Some(value),
                Err(RecvTimeoutError::Timeout) if left <= FRAME && limit.is_some() => break None,
                Err(RecvTimeoutError::Timeout) => tick += 1,
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("{label}: the worker thread panicked")
                }
            }
        };
        if self.styled {
            emit(|out| queue!(out, MoveToColumn(0), Clear(ClearType::CurrentLine)));
        }
        result
    }

    /// Asks a yes or no question on stderr and reads the reply from stdin until it is one of the
    /// replies [`answer`] understands. Enter alone answers yes; the end of input answers no.
    pub fn ask(self, question: &str) -> io::Result<bool> {
        self.palette();
        let prompt = [
            paint("?", palette().accent).bold(),
            Span::raw(" "),
            Span::raw(question.to_owned()).bold(),
            muted(" [Y/n] "),
        ];
        loop {
            emit(|out| self.write(out, &prompt));
            let mut reply = String::new();
            if io::stdin().read_line(&mut reply)? == 0 {
                eprintln!();
                return Ok(false);
            }
            if let Some(yes) = answer(&reply) {
                return Ok(yes);
            }
        }
    }

    /// Announces release `latest`, with the command that installs it when xmtr does not replace
    /// itself.
    pub fn offer(self, current: &str, latest: &str, command: Option<&str>) {
        self.palette();
        let mut lines = vec![Line::from(vec![
            paint("↑", palette().accent).bold(),
            Span::raw(" "),
            Span::raw("Update available").bold(),
            muted(format!(" · {current} → {latest}")),
        ])];
        if let Some(command) = command {
            lines.push(Line::from(vec![
                muted("  Run "),
                paint(command.to_owned(), palette().accent).bold(),
            ]));
        }
        self.lines(&lines);
    }

    /// Reports that `exe` now holds release `version`, with the command that restores raw socket
    /// access where the replaced binary may have had it.
    pub fn updated(self, exe: &Path, version: &str, setcap: Option<&str>) {
        self.palette();
        let mut lines = vec![Line::from(vec![
            paint("✓", palette().good),
            Span::raw(" "),
            Span::raw(format!("Updated {} to {version}", exe.display())),
        ])];
        if let Some(setcap) = setcap {
            lines.push(Line::from(vec![
                paint("!", palette().warn),
                Span::raw(" If you granted raw sockets with setcap, grant them again: "),
                paint(setcap.to_owned(), palette().accent).bold(),
            ]));
        }
        self.lines(&lines);
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Context, anyhow};

    use super::*;
    use crate::live::tests::screen;

    #[test]
    fn reads_yes_no_replies_with_enter_as_yes() {
        for yes in ["", "\n", "y\n", "Y", " yes ", "YES\r\n"] {
            assert_eq!(answer(yes), Some(true), "{yes:?}");
        }
        for no in ["n\n", "N", "no", " No\r\n"] {
            assert_eq!(answer(no), Some(false), "{no:?}");
        }
        for unclear in ["maybe", "yy", "0", "nope"] {
            assert_eq!(answer(unclear), None, "{unclear:?}");
        }
    }

    fn privileges() -> anyhow::Error {
        Err::<(), _>(anyhow!("operation not permitted"))
            .context("checking raw socket privileges")
            .unwrap_err()
    }

    #[test]
    fn a_problem_panel_heads_with_the_error_and_lists_causes_and_notes() {
        let notes = [
            Note::Skipped("example.invalid: no address".to_owned()),
            Note::Failed("1.1.1.1 v4 ICMP: no probe was answered".to_owned()),
        ];
        let error = privileges();
        let causes: Vec<String> = error.chain().skip(1).map(ToString::to_string).collect();
        let panel = problem_panel(&error.to_string(), &causes, &notes, 120);
        let screen = screen(&panel);
        let lines: Vec<&str> = screen.lines().collect();
        assert_eq!(panel.area.width, progress::MIN_WIDTH, "{screen}");
        assert!(lines[0].starts_with("╭ xmtr "), "{screen}");
        assert!(
            lines[1].contains("▍ checking raw socket privileges"),
            "{screen}"
        );
        assert!(lines[2].contains("▍ operation not permitted"), "{screen}");
        assert!(
            lines[4].contains("! skipped example.invalid: no address"),
            "{screen}"
        );
        assert!(
            lines[5].contains("✗ 1.1.1.1 v4 ICMP: no probe was answered"),
            "{screen}"
        );
        assert!(
            lines[6].ends_with(&format!(" v{} ╯", env!("CARGO_PKG_VERSION"))),
            "{screen}"
        );
        let title = &panel[Position::new(4, 1)];
        assert_eq!(title.fg, palette().bad);
        assert!(title.modifier.contains(ratatui::style::Modifier::BOLD));
    }

    #[test]
    fn a_problem_panel_wraps_long_errors_on_narrow_terminals() {
        let long = "raw sockets need elevated privileges: run xmtr as root or Administrator";
        let panel = problem_panel("Upload failed", &[long.to_owned()], &[], 40);
        let screen = screen(&panel);
        assert_eq!(panel.area.width, 40);
        assert!(
            screen
                .lines()
                .nth(1)
                .is_some_and(|line| line.contains("Upload failed"))
        );
        assert!(screen.lines().count() > 4, "{screen}");
        assert!(screen.contains("Administrator"), "{screen}");
    }

    #[test]
    fn styled_output_keeps_colors_and_wide_characters() {
        let panel = problem_panel("名前解決に失敗しました", &[], &[], 40);
        let mut out = Vec::new();
        write_buffer(&mut out, &panel).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("名前解決に失敗しました"), "{text:?}");
        assert!(text.contains('\u{1b}'), "{text:?}");
        assert_eq!(text.matches('\n').count(), 3, "{text:?}");
        let rows: Vec<&str> = text.lines().collect();
        let width = |row: &str| {
            let mut plain = String::new();
            let mut escape = false;
            for c in row.chars() {
                match c {
                    '\u{1b}' => escape = true,
                    'm' if escape => escape = false,
                    _ if escape => {}
                    _ => plain.push(c),
                }
            }
            plain.width()
        };
        assert!(rows.iter().all(|row| width(row) == 40), "{rows:?}");
    }

    #[test]
    fn plain_problems_print_the_notes_and_the_error_chain() {
        let notes = [
            Note::Skipped("example.invalid: no address".to_owned()),
            Note::Failed("1.1.1.1 v4 ICMP: no probe was answered".to_owned()),
            Note::Unanswered(3),
        ];
        assert_eq!(
            problem_text(&privileges(), &notes),
            [
                "skip example.invalid: no address",
                "error 1.1.1.1 v4 ICMP: no probe was answered",
                "note: no reverse DNS or AS answer in time for 3 addresses",
                "error: checking raw socket privileges: operation not permitted",
            ]
        );
    }

    #[test]
    fn waiting_gives_up_after_the_limit() {
        let console = Console {
            styled: false,
            theme: ThemeChoice::DarkAnsi,
        };
        let started = Instant::now();
        let result = console.wait("sleeping", Some(Duration::from_millis(200)), || {
            thread::sleep(Duration::from_secs(5));
        });
        assert_eq!(result, None);
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(200), "{waited:?}");
        assert!(waited < Duration::from_secs(1), "{waited:?}");
        assert_eq!(
            console.wait("answering", Some(Duration::from_secs(3)), || 7),
            Some(7)
        );
        assert_eq!(console.wait("answering", None, || 8), Some(8));
    }
}
