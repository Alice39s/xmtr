//! Colors, glyphs and small widgets shared by the live panel and the result panel.

use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};

pub const ACCENT: Color = Color::Cyan;
pub const MUTED: Color = Color::DarkGray;
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner(tick: usize) -> Span<'static> {
    Span::styled(SPINNER[tick % SPINNER.len()], Style::new().fg(ACCENT))
}

pub fn muted(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::new().fg(MUTED))
}

/// A determinate bar; `glint` lights one filled cell to show the work is still moving.
pub fn bar(width: usize, ratio: f64, color: Color, glint: Option<usize>) -> Vec<Span<'static>> {
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
pub fn sweep(width: usize, tick: usize) -> Vec<Span<'static>> {
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

/// Text set into a border, padded and muted.
pub fn border_text(text: &str) -> Line<'static> {
    Line::from(muted(format!(" {text} ")))
}

/// The rounded frame both panels share: the name top left, `top_right` and `bottom_right` in the
/// border.
pub fn panel_block(top_right: Line<'static>, bottom_right: Line<'static>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(MUTED))
        .title_top(Line::from(vec![
            Span::raw(" "),
            "xmtr".fg(ACCENT).bold(),
            Span::raw(" "),
        ]))
        .title_top(top_right.right_aligned())
        .title_bottom(bottom_right.right_aligned())
}
