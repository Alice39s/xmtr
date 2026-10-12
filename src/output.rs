use comfy_table::presets::{ASCII_BORDERS_ONLY_CONDENSED, UTF8_BORDERS_ONLY};
use comfy_table::{CellAlignment, ContentArrangement, Table};

use crate::model::Route;

fn ms(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.1}"))
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
    let status = if route.reached {
        "reached"
    } else {
        "not reached"
    };
    format!(
        "{} -> {} [{} IPv{}] {} rounds, {:.1}s, {status}\n{table}",
        route.target,
        route.destination,
        route.protocol.to_uppercase(),
        route.family,
        route.rounds,
        route.duration_ms as f64 / 1000.0,
    )
}
