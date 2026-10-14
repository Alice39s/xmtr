//! The result panel printed when a run ends: the finished step line, the IPMap link set apart at
//! the top, notes, then every route as a table.

use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};

use super::theme::{ACCENT, MUTED, border_text, muted};
use super::{Options, progress, steps};
use crate::board::State;
use crate::model::{Hop, Route, UploadAccepted};

#[derive(Debug)]
pub enum Upload {
    Accepted(UploadAccepted),
    Skipped,
    Failed(String),
}

/// What a finished run reports.
#[derive(Debug)]
pub struct Summary {
    pub routes: Vec<Route>,
    pub upload: Upload,
    pub skipped: Vec<String>,
    pub failures: Vec<String>,
    /// Addresses without a reverse DNS or AS answer in time.
    pub unanswered: usize,
}

/// A laid out result panel.
pub struct Rendered {
    pub lines: Vec<Line<'static>>,
    /// Panel width including the border.
    pub width: u16,
    pub top_right: Line<'static>,
    pub bottom_right: Line<'static>,
    /// The link, when it is wider than the panel and goes on its own line below it.
    pub link_below: Option<String>,
}

impl Rendered {
    pub fn height(&self) -> u16 {
        u16::try_from(self.lines.len() + 2).unwrap_or(u16::MAX)
    }
}

const INDENT: &str = " ";
const CALLOUT: &str = "▍ ";
const GAP: usize = 2;
const HOP_WIDTH: usize = 3;
const ASN_WIDTH: usize = 8;
const HOST_MIN: usize = 16;
/// Characters of a truncated host name kept before its address.
const NAME_MIN: usize = 4;
/// Consecutive silent hops that are shown as one line.
const SILENT_RUN: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Metric {
    Loss,
    Sent,
    Last,
    Avg,
    Best,
    Worst,
    StDev,
}

impl Metric {
    const ALL: [Self; 7] = [
        Self::Loss,
        Self::Sent,
        Self::Last,
        Self::Avg,
        Self::Best,
        Self::Worst,
        Self::StDev,
    ];
    /// Dropped in this order when a table is too wide for the panel.
    const OPTIONAL: [Self; 4] = [Self::StDev, Self::Worst, Self::Best, Self::Last];

    const fn header(self) -> &'static str {
        match self {
            Self::Loss => "Loss%",
            Self::Sent => "Snt",
            Self::Last => "Last",
            Self::Avg => "Avg",
            Self::Best => "Best",
            Self::Worst => "Wrst",
            Self::StDev => "StDev",
        }
    }

    const fn width(self) -> usize {
        match self {
            Self::Sent => 4,
            _ => 6,
        }
    }

    fn cell(self, hop: &Hop) -> Span<'static> {
        let ms = |value: Option<f64>| value.map_or_else(|| "-".to_owned(), |v| format!("{v:.1}"));
        let text = match self {
            Self::Loss => format!("{:.1}", hop.loss_pct),
            Self::Sent => hop.sent.to_string(),
            Self::Last => ms(hop.last_ms),
            Self::Avg => ms(hop.avg_ms),
            Self::Best => ms(hop.best_ms),
            Self::Worst => ms(hop.worst_ms),
            Self::StDev => ms(hop.stdev_ms),
        };
        let text = format!("{}{text:>width$}", " ".repeat(GAP), width = self.width());
        match self {
            // A hop that never answered is already muted as `???`; its loss needs no alarm.
            Self::Loss if hop.hosts.is_empty() => muted(text),
            Self::Loss if hop.loss_pct >= 100.0 => {
                Span::styled(text, Style::new().fg(Color::LightRed))
            }
            Self::Loss if hop.loss_pct > 0.0 => Span::styled(text, Style::new().fg(Color::Yellow)),
            Self::Loss | Self::Sent => muted(text),
            _ => Span::raw(text),
        }
    }
}

struct Table {
    host: usize,
    metrics: Vec<Metric>,
}

impl Table {
    fn width_with(host: usize, metrics: &[Metric]) -> usize {
        INDENT.len()
            + 2
            + HOP_WIDTH
            + GAP
            + host
            + GAP
            + ASN_WIDTH
            + metrics
                .iter()
                .map(|metric| GAP + metric.width())
                .sum::<usize>()
    }

    /// The widest host column wanted, narrowed and stripped of optional metrics until it fits.
    /// The table for a panel `room` cells wide: full host names with as many metrics as fit,
    /// otherwise every metric that leaves the host column [`HOST_MIN`] cells, truncating names.
    fn fit(routes: &[Route], room: usize) -> Self {
        let longest = routes
            .iter()
            .flat_map(|route| &route.hops)
            .flat_map(|hop| hop.hosts.iter().map(host_text))
            .map(|text| text.chars().count())
            .max()
            .unwrap_or(3)
            .max(4);
        let mut options = vec![Metric::ALL.to_vec()];
        for drop in Metric::OPTIONAL {
            let mut fewer = options.last().cloned().unwrap_or_default();
            fewer.retain(|metric| *metric != drop);
            options.push(fewer);
        }
        let left = |metrics: &[Metric]| room.saturating_sub(Self::width_with(0, metrics));
        // Every metric stays while names lose at most a quarter; past that, metrics go first.
        let enough = HOST_MIN.max(longest * 3 / 4).min(longest);
        let chosen = options
            .iter()
            .find(|metrics| left(metrics) >= enough)
            .or_else(|| options.iter().find(|metrics| left(metrics) >= HOST_MIN))
            .or_else(|| options.last())
            .cloned()
            .unwrap_or_default();
        Self {
            host: longest.min(left(&chosen)).max(4),
            metrics: chosen,
        }
    }

    fn width(&self) -> usize {
        Self::width_with(self.host, &self.metrics)
    }

    fn header(&self) -> Line<'static> {
        let mut text = format!(
            "{INDENT}  {:>HOP_WIDTH$}{gap}{:<host$}{gap}{:<ASN_WIDTH$}",
            "Hop",
            "Host",
            "ASN",
            gap = " ".repeat(GAP),
            host = self.host
        );
        for metric in &self.metrics {
            text.push_str(&format!(
                "{}{:>width$}",
                " ".repeat(GAP),
                metric.header(),
                width = metric.width()
            ));
        }
        Line::from(muted(text))
    }

    /// The rows of a route; runs of silent hops shrink to one line.
    fn route(&self, route: &Route) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut hops = route.hops.iter().peekable();
        while let Some(hop) = hops.next() {
            if !hop.hosts.is_empty() {
                lines.extend(self.rows(hop));
                continue;
            }
            let mut last = hop;
            let mut count = 1;
            while let Some(next) = hops.next_if(|next| next.hosts.is_empty()) {
                last = next;
                count += 1;
            }
            if count < SILENT_RUN {
                let run = route
                    .hops
                    .iter()
                    .filter(|candidate| (hop.ttl..=last.ttl).contains(&candidate.ttl));
                lines.extend(run.flat_map(|silent| self.rows(silent)));
            } else {
                lines.push(Line::from(muted(format!(
                    "{INDENT}  {:>HOP_WIDTH$}{gap}\u{22ef} {count} hops without reply, up to TTL {}",
                    hop.ttl,
                    last.ttl,
                    gap = " ".repeat(GAP)
                ))));
            }
        }
        lines
    }

    fn rows(&self, hop: &Hop) -> Vec<Line<'static>> {
        let gap = " ".repeat(GAP);
        let mut lines = Vec::new();
        let hosts: Vec<_> = if hop.hosts.is_empty() {
            vec![None]
        } else {
            hop.hosts.iter().map(Some).collect()
        };
        for (index, host) in hosts.into_iter().enumerate() {
            let mut spans = vec![Span::raw(INDENT), Span::raw("  ")];
            spans.push(if index == 0 {
                muted(format!("{:>HOP_WIDTH$}", hop.ttl))
            } else {
                Span::raw(" ".repeat(HOP_WIDTH))
            });
            spans.push(Span::raw(gap.clone()));
            spans.extend(self.host_cell(host));
            if index == 0 {
                let asn = hop
                    .hosts
                    .first()
                    .and_then(|host| host.asn)
                    .map_or_else(|| "-".to_owned(), |asn| format!("AS{asn}"));
                let style = if asn == "-" {
                    Style::new().fg(MUTED)
                } else {
                    Style::new()
                };
                spans.push(Span::styled(format!("{gap}{asn:<ASN_WIDTH$}"), style));
                spans.extend(self.metrics.iter().map(|metric| metric.cell(hop)));
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    fn host_cell(&self, host: Option<&crate::model::Host>) -> Vec<Span<'static>> {
        let Some(host) = host else {
            return vec![muted(format!("{:<width$}", "???", width = self.host))];
        };
        let text = host_text(host);
        let ip = format!(" ({})", host.ip);
        let name_room = self.host.saturating_sub(ip.chars().count());
        match &host.hostname {
            // Names give way first: the address is what the hop is.
            Some(name) if text.chars().count() > self.host && name_room >= NAME_MIN => {
                let cut: String = name.chars().take(name_room - 1).collect();
                vec![Span::raw(format!("{cut}\u{2026}")), muted(ip)]
            }
            _ if text.chars().count() > self.host => {
                let cut: String = text.chars().take(self.host.saturating_sub(1)).collect();
                vec![Span::raw(format!("{cut}\u{2026}"))]
            }
            Some(name) => {
                let pad = " ".repeat(self.host - text.chars().count());
                vec![Span::raw(name.clone()), muted(format!("{ip}{pad}"))]
            }
            None => {
                let pad = " ".repeat(self.host - text.chars().count());
                vec![Span::raw(format!("{}{pad}", host.ip))]
            }
        }
    }
}

fn host_text(host: &crate::model::Host) -> String {
    match &host.hostname {
        Some(name) => format!("{name} ({})", host.ip),
        None => host.ip.clone(),
    }
}

fn route_title(route: &Route) -> Line<'static> {
    let mut spans = vec![Span::raw(INDENT), Span::raw(route.target.clone()).bold()];
    if route.target != route.destination {
        spans.push(muted(" → "));
        spans.push(Span::raw(route.destination.clone()));
    }
    spans.push(muted(format!(
        "  {} · IPv{} · {} rounds · {:.1}s · ",
        route.protocol.to_uppercase(),
        route.family,
        route.rounds,
        route.duration_ms as f64 / 1000.0
    )));
    spans.push(if route.reached {
        "reached".green()
    } else {
        "not reached".light_red()
    });
    Line::from(spans)
}

fn callout(color: Color, spans: Vec<Span<'static>>) -> Line<'static> {
    let mut all = vec![
        Span::raw(INDENT),
        Span::styled(CALLOUT, Style::new().fg(color)),
    ];
    all.extend(spans);
    Line::from(all)
}

fn wrapped(text: &str, width: usize) -> Vec<String> {
    textwrap::wrap(text, width.max(16))
        .into_iter()
        .map(String::from)
        .collect()
}

/// The callout at the top: the link when the upload worked, otherwise what happened to the upload.
fn headline(summary: &Summary, inner: usize) -> (Vec<Line<'static>>, Option<String>) {
    let reached = summary.routes.iter().filter(|route| route.reached).count();
    let routes = summary.routes.len();
    let noun = if routes == 1 { "route" } else { "routes" };
    let counts = muted(format!(" · {routes} {noun} · {reached} reached"));
    match &summary.upload {
        Upload::Accepted(accepted) => {
            let fits =
                CALLOUT.chars().count() + INDENT.len() + accepted.url.chars().count() <= inner;
            let link = if fits {
                Span::styled(
                    accepted.url.clone(),
                    Style::new()
                        .fg(ACCENT)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )
            } else {
                muted("The link is printed below this panel.")
            };
            let mut id = vec![muted("trace-id "), Span::raw(accepted.id.clone())];
            if accepted.pin {
                id.push(muted(" · "));
                id.push("readers need your PIN".yellow());
            }
            let lines = vec![
                callout(ACCENT, vec!["Uploaded to IPMap".bold(), counts]),
                callout(ACCENT, vec![link]),
                callout(ACCENT, id),
            ];
            (lines, (!fits).then(|| accepted.url.clone()))
        }
        Upload::Skipped => (
            vec![callout(MUTED, vec!["Kept on this machine".bold(), counts])],
            None,
        ),
        Upload::Failed(error) => {
            let mut lines = vec![callout(
                Color::LightRed,
                vec!["Upload failed".light_red().bold(), counts],
            )];
            let width = inner.saturating_sub(INDENT.len() + CALLOUT.chars().count());
            lines.extend(
                wrapped(error, width)
                    .into_iter()
                    .map(|part| callout(Color::LightRed, vec![muted(part)])),
            );
            (lines, None)
        }
    }
}

fn notes(summary: &Summary, inner: usize) -> Vec<Line<'static>> {
    let width = inner.saturating_sub(INDENT.len() + 2);
    let note = |icon: Span<'static>, text: &str, style: Style| {
        wrapped(text, width)
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
            .collect::<Vec<_>>()
    };
    let mut lines = Vec::new();
    for skipped in &summary.skipped {
        lines.extend(note(
            "!".yellow(),
            &format!("skipped {skipped}"),
            Style::new(),
        ));
    }
    for failure in &summary.failures {
        lines.extend(note("✗".light_red(), failure, Style::new()));
    }
    if summary.unanswered > 0 {
        let text = format!(
            "no reverse DNS or AS answer in time for {} addresses",
            summary.unanswered
        );
        lines.extend(note(muted("·"), &text, Style::new().fg(MUTED)));
    }
    lines
}

/// Lays out the result panel for a terminal `columns` cells wide.
pub fn render(
    summary: &Summary,
    state: &State,
    options: &Options,
    elapsed: f64,
    columns: u16,
) -> Rendered {
    let failed = matches!(summary.upload, Upload::Failed(_));
    let steps = steps::steps(state, options, failed);
    let link = match &summary.upload {
        Upload::Accepted(accepted) => {
            accepted.url.chars().count() + INDENT.len() + CALLOUT.chars().count()
        }
        _ => 0,
    };
    // One spare cell keeps the last column off the border.
    let wide = Table::fit(&summary.routes, usize::MAX / 2).width() + 1;
    let wanted = [
        steps::width(&steps),
        link,
        wide,
        usize::from(progress::MIN_WIDTH) - 2,
    ]
    .into_iter()
    .max()
    .unwrap_or_default();
    let width = columns.min(u16::try_from(wanted + 2).unwrap_or(u16::MAX));
    let inner = usize::from(width.saturating_sub(2));
    let table = Table::fit(&summary.routes, inner.saturating_sub(1));

    let mut lines = vec![steps::render(&steps, inner, 0), Line::raw("")];
    let (headline, link_below) = headline(summary, inner);
    lines.extend(headline);
    let notes = notes(summary, inner);
    if !notes.is_empty() {
        lines.push(Line::raw(""));
        lines.extend(notes);
    }
    for route in &summary.routes {
        lines.push(Line::raw(""));
        lines.push(route_title(route));
        lines.push(table.header());
        lines.extend(table.route(route));
    }
    // Long reports scroll the headline away, so the link also closes the panel when it fits.
    let bottom_right = match &summary.upload {
        Upload::Accepted(accepted) if accepted.url.chars().count() + 6 <= inner => {
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    accepted.url.clone(),
                    Style::new().fg(ACCENT).add_modifier(Modifier::UNDERLINED),
                ),
                Span::raw(" "),
            ])
        }
        _ => border_text(&format!("v{}", env!("CARGO_PKG_VERSION"))),
    };
    Rendered {
        lines,
        width,
        top_right: border_text(&format!("{elapsed:.1}s")),
        bottom_right,
        link_below,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::{Paragraph, Widget};

    use super::*;
    use crate::board::Phase;
    use crate::live::tests::{options, screen};
    use crate::live::theme::panel_block;
    use crate::model::Host;

    const URL: &str = "https://xmsl.dev/ipmap?plat=xmtr&trace-id=103115083070058496-nBjPEnny";

    fn hop(ttl: u8, hosts: Vec<Host>, loss: f64) -> Hop {
        Hop {
            ttl,
            hosts,
            sent: 5,
            recv: 5,
            loss_pct: loss,
            last_ms: Some(1.25),
            avg_ms: Some(1.5),
            best_ms: Some(1.0),
            worst_ms: Some(2.0),
            stdev_ms: Some(0.25),
            samples_ms: Vec::new(),
        }
    }

    fn host(ip: &str, hostname: Option<&str>, asn: Option<u32>) -> Host {
        Host {
            ip: ip.to_owned(),
            hostname: hostname.map(str::to_owned),
            asn,
            replies: 5,
        }
    }

    fn summary(upload: Upload) -> Summary {
        Summary {
            routes: vec![Route {
                target: "one.one.one.one".to_owned(),
                destination: "1.1.1.1".to_owned(),
                protocol: "icmp",
                family: 4,
                started_at: String::new(),
                duration_ms: 5100,
                rounds: 5,
                reached: true,
                hops: vec![
                    hop(1, vec![host("192.168.1.1", None, None)], 0.0),
                    hop(2, Vec::new(), 100.0),
                    hop(
                        3,
                        vec![
                            host("1.1.1.1", Some("one.one.one.one"), Some(13335)),
                            host("1.0.0.1", None, Some(13335)),
                        ],
                        20.0,
                    ),
                ],
            }],
            upload,
            skipped: vec!["example.org: no address in the requested family".to_owned()],
            failures: Vec::new(),
            unanswered: 2,
        }
    }

    fn draw(summary: &Summary, columns: u16) -> (String, Rendered) {
        let state = State {
            phase: Phase::Result,
            rows: Vec::new(),
            lookup: (4, 4),
        };
        let rendered = render(summary, &state, &options(), 7.5, columns);
        let area = Rect::new(0, 0, rendered.width, rendered.height());
        let mut buffer = Buffer::empty(area);
        Paragraph::new(rendered.lines.clone())
            .block(panel_block(
                rendered.top_right.clone(),
                rendered.bottom_right.clone(),
            ))
            .render(area, &mut buffer);
        (screen(&buffer), rendered)
    }

    fn accepted() -> Upload {
        Upload::Accepted(UploadAccepted {
            id: "103115083070058496-nBjPEnny".to_owned(),
            pin: true,
            url: URL.to_owned(),
        })
    }

    #[test]
    fn puts_the_link_first_and_the_tables_after() {
        let (screen, rendered) = draw(&summary(accepted()), 120);
        let lines: Vec<&str> = screen.lines().collect();
        assert!(
            lines[1].contains("\u{2713} 4. Upload \u{2500} \u{25cf} 5. Result"),
            "{screen}"
        );
        assert!(
            lines[3].contains("Uploaded to IPMap \u{b7} 1 route \u{b7} 1 reached"),
            "{screen}"
        );
        assert!(lines[4].contains(URL), "{screen}");
        assert!(lines[5].contains("readers need your PIN"), "{screen}");
        assert!(screen.contains("skipped example.org"), "{screen}");
        assert!(
            screen.contains("one.one.one.one \u{2192} 1.1.1.1"),
            "{screen}"
        );
        assert!(screen.contains("one.one.one.one (1.1.1.1)"), "{screen}");
        assert!(screen.contains("AS13335"), "{screen}");
        assert!(screen.contains("???"), "{screen}");
        assert!(screen.contains("StDev"), "{screen}");
        assert!(
            lines.last().is_some_and(|line| line.contains(URL)),
            "{screen}"
        );
        assert_eq!(rendered.link_below, None);
    }

    #[test]
    fn narrow_terminals_drop_columns_and_move_the_link_out() {
        let (screen, rendered) = draw(&summary(accepted()), 64);
        assert_eq!(rendered.width, 64);
        assert!(!screen.contains("StDev"), "{screen}");
        assert!(screen.contains("Avg"), "{screen}");
        assert!(screen.contains("printed below"), "{screen}");
        assert_eq!(rendered.link_below.as_deref(), Some(URL));
    }

    #[test]
    fn long_host_names_shrink_before_their_address() {
        let table = Table {
            host: 20,
            metrics: Vec::new(),
        };
        let cell = table.host_cell(Some(&host("1.1.1.1", Some("one.one.one.one"), None)));
        let text: String = cell.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "one.one.o\u{2026} (1.1.1.1)");
    }

    #[test]
    fn runs_of_silent_hops_take_one_line() {
        let mut summary = summary(Upload::Skipped);
        let route = &mut summary.routes[0];
        let last = route.hops.pop().expect("a final hop");
        for ttl in 3..=6 {
            route.hops.push(hop(ttl, Vec::new(), 100.0));
        }
        route.hops.push(Hop { ttl: 7, ..last });
        let (screen, _) = draw(&summary, 120);
        assert!(
            screen.contains("5 hops without reply, up to TTL 6"),
            "{screen}"
        );
        assert!(!screen.contains("???"), "{screen}");
        assert!(screen.contains("Kept on this machine"), "{screen}");
    }

    #[test]
    fn a_failed_upload_marks_its_step_and_shows_the_error() {
        let failed = Upload::Failed("upload rejected with HTTP 503: maintenance".to_owned());
        let (screen, _) = draw(&summary(failed), 100);
        assert!(screen.contains("\u{2717} 4. Upload"), "{screen}");
        assert!(screen.contains("Upload failed"), "{screen}");
        assert!(screen.contains("HTTP 503: maintenance"), "{screen}");
    }
}
