use comfy_table::presets::{ASCII_BORDERS_ONLY_CONDENSED, UTF8_BORDERS_ONLY};
use comfy_table::{CellAlignment, ContentArrangement, Table};

use unicode_width::UnicodeWidthStr;

use crate::model::{Hop, Route};

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

/// A route for a terminal `width` cells wide: the full table when it fits, otherwise one short line
/// per hop with the host, the average round trip and the loss.
pub fn render_for(route: &Route, width: usize) -> String {
    let table = render(route);
    if table.lines().all(|line| line.width() <= width) {
        return table;
    }
    let mut lines = vec![narrow_header(route, width)];
    lines.extend(route.hops.iter().map(|hop| narrow_hop(hop, width)));
    lines.join("\n")
}

/// Renders a route the way `mtr --report-wide` does, with alternate ECMP hosts on extra lines.
pub fn render(route: &Route) -> String {
    let mut table = Table::new();
    table
        .load_preset(if cfg!(windows) {
            ASCII_BORDERS_ONLY_CONDENSED
        } else {
            UTF8_BORDERS_ONLY
        })
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header([
            "Hop", "Host", "ASN", "Loss%", "Snt", "Last", "Avg", "Best", "Wrst", "StDev",
        ]);
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
    for column in [0, 3, 4, 5, 6, 7, 8, 9] {
        if let Some(column) = table.column_mut(column) {
            column.set_cell_alignment(CellAlignment::Right);
        }
    }
    format!("{}\n{table}", header(route))
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
        assert_eq!(render_for(&route(), 200), render(&route()));
    }
}
