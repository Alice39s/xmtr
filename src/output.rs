use comfy_table::presets::{ASCII_BORDERS_ONLY_CONDENSED, UTF8_BORDERS_ONLY};
use comfy_table::{CellAlignment, ColumnConstraint, ContentArrangement, Table, Width};

use unicode_width::UnicodeWidthStr;

use crate::model::{Hop, Route};

/// The narrowest the Host column gets in a table: an IPv4 address. Below that the hops are listed one
/// per line instead.
const HOST_MIN: u16 = 15;

fn ms(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.1}"))
}

/// The route header: target, destination, protocol, rounds, time and status.
fn header(route: &Route) -> String {
    let status = if route.reached {
        "reached"
    } else {
        "not reached"
    };
    format!(
        "{} -> {} [{} IPv{}] {} rounds, {:.1}s, {status}",
        route.target,
        route.destination,
        route.protocol.to_uppercase(),
        route.family,
        route.rounds,
        route.duration_ms as f64 / 1000.0,
    )
}

/// Cuts `text` to `width` cells, ending with an ellipsis when something was cut.
fn cut(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut kept = String::new();
    for c in text.chars() {
        if kept.width() + c.to_string().width() + 1 > width {
            break;
        }
        kept.push(c);
    }
    if width > 0 {
        kept.push('…');
    }
    kept
}

fn narrow_hop(hop: &Hop, width: usize) -> String {
    let host = hop.hosts.first().map_or_else(
        || "???".to_owned(),
        |host| host.hostname.clone().unwrap_or_else(|| host.ip.clone()),
    );
    let more = hop.hosts.len().saturating_sub(1);
    let host = if more > 0 {
        format!("{host} +{more}")
    } else {
        host
    };
    let avg = hop
        .avg_ms
        .map_or_else(String::new, |avg| format!("{avg:.1}ms"));
    let loss = if hop.loss_pct > 0.0 && !hop.hosts.is_empty() {
        format!("{:.0}%", hop.loss_pct)
    } else {
        String::new()
    };
    let ttl = format!("{:>3}", hop.ttl);
    // Space for the measurements first, then as much of the host as still fits.
    let numbers: Vec<&str> = [avg.as_str(), loss.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    let mut tail = numbers.join(" ");
    let mut room = width.saturating_sub(ttl.width() + 2 + tail.width());
    if room < 6 && !loss.is_empty() {
        tail = avg.clone();
        room = width.saturating_sub(ttl.width() + 2 + tail.width());
    }
    if room < 6 {
        tail.clear();
        room = width.saturating_sub(ttl.width() + 1);
    }
    let host = cut(&host, room);
    if tail.is_empty() {
        cut(&format!("{ttl} {host}"), width)
    } else {
        let pad = room.saturating_sub(host.width());
        cut(&format!("{ttl} {host}{} {tail}", " ".repeat(pad)), width)
    }
}

/// The most detailed route header that fits `width`; the status is the last thing to go.
fn narrow_header(route: &Route, width: usize) -> String {
    let status = if route.reached {
        "reached"
    } else {
        "not reached"
    };
    let protocol = format!("{} v{}", route.protocol.to_uppercase(), route.family);
    let seconds = route.duration_ms as f64 / 1000.0;
    let candidates = [
        header(route),
        format!(
            "{} {protocol}, {} rounds, {seconds:.1}s, {status}",
            route.target, route.rounds
        ),
        format!("{} {protocol} {status}", route.target),
        format!("{} {status}", route.target),
    ];
    candidates
        .iter()
        .find(|candidate| candidate.width() <= width)
        .cloned()
        .unwrap_or_else(|| cut(&candidates[3], width))
}

/// A route for a terminal `width` cells wide, rendered the way `mtr --report-wide` does: the full
/// table when it fits with only the Host column wrapped, otherwise one short line per hop with the
/// host, the average round trip and the loss. `usize::MAX` gives the full table for pipes and logs.
pub fn render_for(route: &Route, width: usize) -> String {
    let table = table(route, u16::try_from(width).ok()).to_string();
    if table.lines().all(|line| line.width() <= width) {
        return format!("{}\n{table}", narrow_header(route, width));
    }
    let mut lines = vec![narrow_header(route, width)];
    lines.extend(route.hops.iter().map(|hop| narrow_hop(hop, width)));
    lines.join("\n")
}

/// The hop table with alternate ECMP hosts on extra lines, at most `width` cells wide when one is
/// given. Only the Host column wraps, and not below `HOST_MIN` cells, so a table that does not fit
/// comes back wider than `width`.
fn table(route: &Route, width: Option<u16>) -> Table {
    let mut table = Table::new();
    table
        .load_preset(if cfg!(windows) {
            ASCII_BORDERS_ONLY_CONDENSED
        } else {
            UTF8_BORDERS_ONLY
        })
        .set_content_arrangement(ContentArrangement::Dynamic)
        // The width comes from the caller; comfy-table would otherwise wrap to the size of the
        // terminal it sees on stdout.
        .force_no_tty()
        .set_header([
            "Hop", "Host", "ASN", "Loss%", "Snt", "Last", "Avg", "Best", "Wrst", "StDev",
        ]);
    if let Some(width) = width {
        table.set_width(width);
    }
    let mut host_width = 0;
    for hop in &route.hops {
        let host = if hop.hosts.is_empty() {
            "???".to_owned()
        } else {
            hop.hosts
                .iter()
                .map(|host| match &host.hostname {
                    Some(name) => format!("{name} ({})", host.ip),
                    None => host.ip.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        host_width = host
            .lines()
            .map(UnicodeWidthStr::width)
            .fold(host_width, usize::max);
        let asn = hop
            .hosts
            .first()
            .and_then(|host| host.asn)
            .map_or_else(|| "-".to_owned(), |asn| format!("AS{asn}"));
        table.add_row([
            hop.ttl.to_string(),
            host,
            asn,
            format!("{:.1}", hop.loss_pct),
            hop.sent.to_string(),
            ms(hop.last_ms),
            ms(hop.avg_ms),
            ms(hop.best_ms),
            ms(hop.worst_ms),
            ms(hop.stdev_ms),
        ]);
    }
    let host_min = u16::try_from(host_width).map_or(HOST_MIN, |host| host.min(HOST_MIN));
    for (index, column) in table.column_iter_mut().enumerate() {
        // Constraint widths include the cell padding.
        let padding = column.padding_width();
        column.set_constraint(if index == 1 {
            ColumnConstraint::LowerBoundary(Width::Fixed(host_min + padding))
        } else {
            ColumnConstraint::ContentWidth
        });
        if index != 1 && index != 2 {
            column.set_cell_alignment(CellAlignment::Right);
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Host;

    fn hop(ttl: u8, ip: Option<&str>, avg: Option<f64>, loss: f64) -> Hop {
        Hop {
            ttl,
            hosts: ip
                .map(|ip| Host {
                    ip: ip.to_owned(),
                    hostname: None,
                    asn: None,
                    replies: 1,
                })
                .into_iter()
                .collect(),
            sent: 5,
            recv: 4,
            loss_pct: loss,
            last_ms: avg,
            avg_ms: avg,
            best_ms: avg,
            worst_ms: avg,
            stdev_ms: avg.map(|_| 0.0),
            samples_ms: Vec::new(),
        }
    }

    fn route() -> Route {
        Route {
            target: "example.org".to_owned(),
            destination: "192.0.2.10".to_owned(),
            protocol: "icmp",
            family: 4,
            source_addr: None,
            started_at: String::new(),
            duration_ms: 5200,
            rounds: 5,
            reached: true,
            hops: vec![
                hop(1, Some("10.63.0.2"), Some(0.1), 20.0),
                hop(2, None, None, 100.0),
                hop(3, Some("192.0.2.10"), Some(23.45), 0.0),
            ],
        }
    }

    #[test]
    fn narrow_terminals_get_one_line_per_hop() {
        let text = render_for(&route(), 30);
        assert_eq!(
            text,
            "example.org ICMP v4 reached\n  1 10.63.0.2        0.1ms 20%\n  2 ???\n  3 192.0.2.10          23.4ms"
        );
        for width in 4..40 {
            for line in render_for(&route(), width).lines() {
                assert!(line.width() <= width, "{width}: {line}");
            }
        }
        assert_eq!(render_for(&route(), 200), full(&route()));
    }

    fn full(route: &Route) -> String {
        render_for(route, usize::MAX)
    }

    fn widest(text: &str) -> usize {
        text.lines().map(UnicodeWidthStr::width).max().unwrap_or(0)
    }

    #[test]
    fn keeps_every_column_until_the_hosts_would_be_cut() {
        let natural = widest(&full(&route()));
        assert_eq!(render_for(&route(), natural), full(&route()));
        let narrow = render_for(&route(), natural - 1);
        assert!(!narrow.contains("StDev"), "{narrow}");
        assert!(widest(&narrow) < natural, "{narrow}");
    }

    #[test]
    fn wraps_long_host_names_inside_the_host_column() {
        let mut route = route();
        route.hops[0].hosts[0].hostname =
            Some("ae-12.edge-router-03.backbone.example.net".to_owned());
        let text = render_for(&route, 90);
        assert!(widest(&text) <= 90, "{text}");
        let header = text
            .lines()
            .find(|line| line.contains("Hop"))
            .unwrap_or_default();
        for column in [
            "Host", "ASN", "Loss%", "Snt", "Last", "Avg", "Best", "Wrst", "StDev",
        ] {
            assert!(header.contains(column), "{column}: {text}");
        }
        assert!(text.contains("23.4"), "{text}");
        assert!(text.contains("(10.63.0.2)"), "{text}");
        assert!(widest(&full(&route)) > 90);
    }
}
