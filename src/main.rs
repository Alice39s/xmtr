mod board;
mod cli;
mod convergence;
mod dns;
mod live;
mod manage;
mod model;
mod output;
mod plan;
mod runner;
mod update;
mod upload;

use std::collections::HashMap;
use std::fmt::Display;
use std::io::{IsTerminal, Write};
use std::net::IpAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use ratatui::crossterm::style::Stylize;

use crate::board::Board;
use crate::cli::{Cli, Command, SelfCommand};
use crate::dns::Dns;
use crate::live::{Console, Live, Note};
use crate::runner::Settings;

const DEFAULT_PARALLEL: usize = 8;

/// True when no hop of the route answered a single probe.
fn silent(route: &model::Route) -> bool {
    route.hops.iter().all(|hop| hop.recv == 0)
}

/// Lines for stdout. A reader that went away, such as `xmtr ... | head -1`, closes stdout; later lines
/// are then dropped instead of aborting the run.
struct Stdout {
    open: bool,
}

impl Stdout {
    const fn new() -> Self {
        Self { open: true }
    }

    fn line(&mut self, text: impl Display) {
        if self.open && writeln!(std::io::stdout().lock(), "{text}").is_err() {
            self.open = false;
        }
    }

    /// Prints the IPMap link on stdout, or on stderr when stdout is closed so the stored trace can still
    /// be found.
    fn link(&mut self, url: &str) {
        self.line(url);
        if !self.open {
            eprintln!("{url}");
        }
    }
}

/// Everything a run produced, printed once the live view is gone.
struct Report {
    skipped: Vec<String>,
    failures: Vec<String>,
    unanswered: usize,
    body: model::Upload,
    upload: Option<Result<model::UploadAccepted>>,
}

fn settings(cli: &Cli, privilege: trippy_core::PrivilegeMode) -> Settings {
    Settings {
        privilege,
        min_rounds: usize::from(cli.min_rounds.min(cli.max_rounds)),
        max_rounds: usize::from(cli.max_rounds),
        time_limit: cli.time_limit,
        interval: cli.interval,
        max_ttl: cli.max_ttl,
        tcp_port: cli.port,
    }
}

/// The public source address of the routes is looked up only for uploads that share their origin.
fn sends_source(cli: &Cli) -> bool {
    !cli.hide_origin && !cli.no_upload
}

/// Resolves, traces, annotates and uploads, recording each step on `board`.
fn work(cli: &Cli, board: &Board) -> Result<Report> {
    let dns = Dns::new(&cli.dns)?;
    let plan = plan::build(cli, &dns)?;
    board.start_trace(&plan.jobs);
    let parallel = cli.parallel.map_or(
        DEFAULT_PARALLEL.min(runner::route_count(&plan.jobs)),
        usize::from,
    );
    let (outcomes, sources) = thread::scope(|scope| {
        // The source addresses are looked up while the routes are traced.
        let lookup = sends_source(cli)
            .then(|| scope.spawn(|| upload::source::lookup(&plan.jobs, &cli.api, &dns)));
        let outcomes = runner::run_all(&plan.jobs, parallel, &settings(cli, plan.privilege), board);
        let sources: HashMap<IpAddr, IpAddr> = lookup.map_or_else(HashMap::new, |lookup| {
            lookup
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        });
        (outcomes, sources)
    });

    let mut routes = Vec::new();
    let mut failures = Vec::new();
    for outcome in outcomes {
        match outcome.result {
            Ok(route) if silent(&route) => {
                failures.push(format!("{}: no probe was answered", outcome.label));
            }
            Ok(mut route) => {
                let destination: IpAddr = route
                    .destination
                    .parse()
                    .expect("routes are traced to IP addresses");
                route.source_addr = sources.get(&destination).copied();
                routes.push(route);
            }
            Err(error) => failures.push(format!("{}: {error:#}", outcome.label)),
        }
    }
    routes.sort_by_key(|route| {
        (
            route.target.clone(),
            route.family,
            !route.reached,
            route.protocol,
        )
    });

    let mut unanswered = 0;
    if !cli.no_rdns && !routes.is_empty() {
        board.start_lookup();
        unanswered = dns.annotate(&mut routes, |answered, total| {
            board.set_lookup(answered, total);
        });
    }
    let body = model::Upload {
        schema: 1,
        client: model::Client::current(),
        share_origin: !cli.hide_origin,
        pin: cli.pin.clone(),
        routes,
    };
    let upload = (!cli.no_upload && !body.routes.is_empty()).then(|| {
        board.start_upload();
        upload::upload(&cli.api, &body, &dns)
    });
    Ok(Report {
        skipped: plan.skipped,
        failures,
        unanswered,
        body,
        upload,
    })
}

/// Columns the route tables may use: the terminal width, or no limit when the output is not a
/// terminal, so logs and pipes keep the full table.
fn table_width(terminal: bool) -> usize {
    if !terminal {
        return usize::MAX;
    }
    ratatui::crossterm::terminal::size().map_or(usize::MAX, |(width, _)| usize::from(width))
}

/// What the run left out or could not do; `--quiet` keeps only the routes that failed.
fn notes(cli: &Cli, report: &Report) -> Vec<Note> {
    let mut notes = Vec::new();
    if !cli.quiet {
        notes.extend(report.skipped.iter().cloned().map(Note::Skipped));
    }
    notes.extend(report.failures.iter().cloned().map(Note::Failed));
    if report.unanswered > 0 && !cli.quiet {
        notes.push(Note::Unanswered(report.unanswered));
    }
    notes
}

fn report(cli: &Cli, console: Console, report: Report) -> Result<ExitCode> {
    let notes = notes(cli, &report);
    if report.body.routes.is_empty() {
        console.problem(&anyhow::anyhow!("no route could be traced"), None, &notes);
        return Ok(ExitCode::FAILURE);
    }
    console.notes(&notes);
    let mut stdout = Stdout::new();
    // With --json the tables go to stderr so stdout holds only the JSON document.
    if !cli.quiet {
        let width = if cli.json {
            table_width(std::io::stderr().is_terminal())
        } else {
            table_width(std::io::stdout().is_terminal())
        };
        for route in &report.body.routes {
            let table = output::render_for(route, width);
            if cli.json {
                eprintln!("{table}\n");
            } else {
                stdout.line(format_args!("{table}\n"));
            }
        }
    }
    if cli.json {
        stdout.line(serde_json::to_string_pretty(&report.body)?);
    }
    match report.upload {
        None => Ok(ExitCode::SUCCESS),
        Some(Ok(accepted)) => {
            if !cli.quiet {
                stdout.line(format_args!("trace-id {}", accepted.id));
            }
            stdout.link(&accepted.url);
            if !cli.quiet && accepted.pin {
                stdout.line("Readers also need the PIN you set.");
            }
            Ok(ExitCode::SUCCESS)
        }
        Some(Err(error)) => {
            console.problem(&error, Some("Upload failed"), &[]);
            Ok(ExitCode::from(2))
        }
    }
}

fn live_options(cli: &Cli) -> Result<live::Options> {
    let upload = if cli.no_upload {
        None
    } else {
        let uri: ureq::http::Uri = cli
            .api
            .parse()
            .with_context(|| format!("reading the gateway address {}", cli.api))?;
        Some(uri.host().unwrap_or(&cli.api).to_owned())
    };
    Ok(live::Options {
        lookup: !cli.no_rdns,
        upload,
        min_rounds: usize::from(cli.min_rounds.min(cli.max_rounds)),
        time_limit: cli.time_limit,
    })
}

/// Hands the report to the live view, which prints it as the result panel.
fn show_result(cli: &Cli, live: Live, report: Report) -> Result<ExitCode> {
    let notes = notes(cli, &report);
    let (upload, code) = match report.upload {
        None => (live::Upload::Skipped, ExitCode::SUCCESS),
        Some(Ok(accepted)) => (live::Upload::Accepted(accepted), ExitCode::SUCCESS),
        Some(Err(error)) => (
            live::Upload::Failed(format!("{error:#}")),
            ExitCode::from(2),
        ),
    };
    let link_below = live.finish(Some(live::Summary {
        routes: report.body.routes.clone(),
        upload,
        notes,
    }))?;
    if let Some(link) = link_below {
        // Plain text the terminal can wrap, beside the panel on stderr.
        eprintln!("{}", link.as_str().with(live::accent()).underlined());
    }
    if cli.json {
        Stdout::new().line(serde_json::to_string_pretty(&report.body)?);
    }
    Ok(code)
}

/// Shows the result in the panel when the live view has one, and prints it otherwise.
fn finish(
    cli: &Cli,
    console: Console,
    live: Option<Live>,
    result: Result<Report>,
) -> Result<ExitCode> {
    let Some(live) = live else {
        return report(cli, console, result?);
    };
    // The panel runs only with stdout on the terminal, so its result replaces the tables printed
    // there; with `--json` the document still follows on stdout.
    match result {
        Ok(report) if live.shows_result() && !report.body.routes.is_empty() => {
            show_result(cli, live, report)
        }
        result => {
            live.finish(None)?;
            report(cli, console, result?)
        }
    }
}

fn run(cli: &Cli, console: Console) -> Result<ExitCode> {
    let board = Arc::new(Board::new(&cli.targets));
    let live = if cli.quiet {
        None
    } else {
        match Live::start(
            Arc::clone(&board),
            live_options(cli)?,
            cli.progress,
            cli.theme,
        ) {
            Ok(live) => Some(live),
            Err(error) => {
                console.remark(&format!("showing results without the live view: {error:#}"));
                None
            }
        }
    };
    let result = work(cli, &board);
    let traced = result
        .as_ref()
        .is_ok_and(|report| !report.body.routes.is_empty());
    let code = finish(cli, console, live, result)?;
    if traced
        && update::wanted(
            cli,
            std::io::stdin().is_terminal(),
            std::io::stderr().is_terminal(),
        )
    {
        return update::offer(console, code);
    }
    Ok(code)
}

fn main() -> ExitCode {
    // Package managers check a command-line tool by starting it without arguments, so a bare `xmtr`
    // prints the help and succeeds.
    if std::env::args_os().len() == 1 {
        return match Cli::command().print_help() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let cli = Cli::parse();
    if let Some(message) = cli::conflict(&cli) {
        Cli::command()
            .error(clap::error::ErrorKind::ArgumentConflict, message)
            .exit();
    }
    let console = Console::new(cli.quiet, cli.progress, cli.theme);
    let result = match &cli.command {
        Some(Command::Manage(SelfCommand::Update { force, tag })) => {
            manage::update(*force, tag.as_deref()).map(|()| ExitCode::SUCCESS)
        }
        Some(Command::Manage(SelfCommand::Uninstall)) => {
            manage::uninstall().map(|()| ExitCode::SUCCESS)
        }
        None => run(&cli, console),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            console.problem(&error, None, &[]);
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_address_is_sent_only_with_an_upload_that_shares_its_origin() {
        let parse = |args: &[&str]| Cli::try_parse_from([&["xmtr"], args, &["1.1.1.1"]].concat());
        assert!(sends_source(&parse(&[]).unwrap()));
        for args in [
            &["--hide-origin"][..],
            &["--no-upload"],
            &["--hide-origin", "--no-upload"],
        ] {
            assert!(!sends_source(&parse(args).unwrap()), "{args:?}");
        }
    }
}
