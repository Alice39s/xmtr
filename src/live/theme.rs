//! Colors, glyphs and small widgets shared by the live views and the result panel.
//!
//! Four palettes cover dark and light backgrounds, each with the 16 ANSI colors the terminal theme
//! defines, or with fixed RGB colors on terminals that announce true color.

use std::env;
use std::sync::OnceLock;
use std::time::Duration;

use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use terminal_colorsaurus::{QueryOptions, ThemeMode};

use crate::cli::ThemeChoice;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// How long to wait for the terminal to report its background color.
const QUERY_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub name: &'static str,
    pub accent: Color,
    pub muted: Color,
    pub good: Color,
    pub bad: Color,
    pub warn: Color,
    /// The moving highlight on a progress bar.
    pub glint: Color,
}

pub const DARK_ANSI: Palette = Palette {
    name: "dark-ansi",
    accent: Color::Cyan,
    muted: Color::DarkGray,
    good: Color::Green,
    bad: Color::LightRed,
    warn: Color::Yellow,
    glint: Color::White,
};

/// Yellow and bright cyan wash out on a light background; blue, magenta and black stay readable.
pub const LIGHT_ANSI: Palette = Palette {
    name: "light-ansi",
    accent: Color::Blue,
    muted: Color::DarkGray,
    good: Color::Green,
    bad: Color::Red,
    warn: Color::Magenta,
    glint: Color::Black,
};

pub const DARK: Palette = Palette {
    name: "dark",
    accent: Color::Rgb(86, 182, 214),
    muted: Color::Rgb(118, 124, 138),
    good: Color::Rgb(126, 206, 150),
    bad: Color::Rgb(242, 116, 122),
    warn: Color::Rgb(230, 192, 120),
    glint: Color::Rgb(255, 255, 255),
};

pub const LIGHT: Palette = Palette {
    name: "light",
    accent: Color::Rgb(0, 105, 160),
    muted: Color::Rgb(120, 124, 134),
    good: Color::Rgb(22, 128, 60),
    bad: Color::Rgb(192, 40, 36),
    warn: Color::Rgb(168, 98, 0),
    glint: Color::Rgb(0, 0, 0),
};

static PALETTE: OnceLock<Palette> = OnceLock::new();

/// The palette in use; the dark ANSI one until `select` picks another.
pub fn palette() -> &'static Palette {
    PALETTE.get_or_init(|| DARK_ANSI)
}

/// Picks the palette before anything is drawn; later calls keep the first choice. `auto` asks the
/// terminal for its background color, then falls back to `COLORFGBG` and finally to a dark
/// background; `COLORTERM` decides between RGB and ANSI colors.
pub fn select(choice: ThemeChoice) -> &'static Palette {
    PALETTE.get_or_init(|| match choice {
        ThemeChoice::Dark => DARK,
        ThemeChoice::Light => LIGHT,
        ThemeChoice::DarkAnsi => DARK_ANSI,
        ThemeChoice::LightAnsi => LIGHT_ANSI,
        ThemeChoice::Auto => {
            let light = background_is_light();
            match (light, true_color()) {
                (false, false) => DARK_ANSI,
                (true, false) => LIGHT_ANSI,
                (false, true) => DARK,
                (true, true) => LIGHT,
            }
        }
    })
}

fn background_is_light() -> bool {
    let mut options = QueryOptions::default();
    options.timeout = QUERY_TIMEOUT;
    match terminal_colorsaurus::theme_mode(options) {
        Ok(mode) => mode == ThemeMode::Light,
        Err(_) => env::var("COLORFGBG").is_ok_and(|value| colorfgbg_is_light(&value)),
    }
}

/// `COLORFGBG` is `fg;bg` or `fg;default;bg` with ANSI color numbers; 7 and 9 to 15 are light.
fn colorfgbg_is_light(value: &str) -> bool {
    value
        .rsplit(';')
        .next()
        .and_then(|bg| bg.parse::<u8>().ok())
        .is_some_and(|bg| bg == 7 || (9..=15).contains(&bg))
}

fn true_color() -> bool {
    env::var("COLORTERM").is_ok_and(|value| value == "truecolor" || value == "24bit")
}

pub fn spinner(tick: usize) -> Span<'static> {
    paint(SPINNER[tick % SPINNER.len()], palette().accent)
}

pub fn paint(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(text.into(), Style::new().fg(color))
}

pub fn muted(text: impl Into<String>) -> Span<'static> {
    paint(text, palette().muted)
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
                    Style::new()
                        .fg(palette().glint)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                paint("━", color)
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
                paint("━", palette().accent)
            } else {
                muted("─")
            }
        })
        .collect()
}

/// The left margin inside a panel.
pub const INDENT: &str = " ";
/// The colored bar in front of a headline.
pub const CALLOUT: &str = "▍ ";

/// A line set apart by a bar in `color`.
pub fn callout(color: Color, spans: Vec<Span<'static>>) -> Line<'static> {
    let mut all = vec![
        Span::raw(INDENT),
        Span::styled(CALLOUT, Style::new().fg(color)),
    ];
    all.extend(spans);
    Line::from(all)
}

/// `text` wrapped to `width` columns, but never narrower than 16.
pub fn wrapped(text: &str, width: usize) -> Vec<String> {
    textwrap::wrap(text, width.max(16))
        .into_iter()
        .map(String::from)
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
        .border_style(Style::new().fg(palette().muted))
        .title_top(Line::from(vec![
            Span::raw(" "),
            "xmtr".fg(palette().accent).bold(),
            Span::raw(" "),
        ]))
        .title_top(top_right.right_aligned())
        .title_bottom(bottom_right.right_aligned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_background_from_colorfgbg() {
        assert!(colorfgbg_is_light("0;15"));
        assert!(colorfgbg_is_light("0;default;7"));
        assert!(!colorfgbg_is_light("15;0"));
        assert!(!colorfgbg_is_light("7;8"));
        assert!(!colorfgbg_is_light("garbage"));
    }
}
