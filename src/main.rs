mod cli;
mod convergence;
mod dns;
mod manage;
mod model;
mod output;
mod plan;
mod runner;
mod upload;

use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Command, SelfCommand};
use crate::dns::Dns;
use crate::runner::{Progress, Settings};

const DEFAULT_PARALLEL: usize = 8;
/// Width of the progress line; kept under 80 columns so it never wraps.
const PROGRESS_WIDTH: usize = 78;

/// One status line such as `tracing 3 | done 2/5 | reached 2 | 1.1.1.1 IPv4/ICMP r4`, rewritten in place
/// with a carriage return, which every terminal understands.
fn print_progress(progress: &[Mutex<Progress>]) {
    let entries: Vec<Progress> = progress
        .iter()
        .filter_map(|entry| entry.lock().ok().map(|entry| entry.clone()))
        .collect();
    let done = entries.iter().filter(|entry| entry.done).count();
    let reached = entries.iter().filter(|entry| entry.reached).count();
    let active: Vec<&Progress> = entries
        .iter()
        .filter(|entry| !entry.done && !entry.label.is_empty())
        .collect();
    let mut line = format!(
        "tracing {} | done {done}/{} | reached {reached}",
        active.len(),
        entries.len()
    );
    if let Some(first) = active.first() {
        line.push_str(&format!(" | {} r{}", first.label, first.rounds));
    }
    let line: String = line.chars().take(PROGRESS_WIDTH).collect();
    let mut stderr = std::io::stderr();
    let _ = write!(stderr, "\r{line:<PROGRESS_WIDTH$}");
    let _ = stderr.flush();
}

/// True when no hop of the route answered a single probe.
fn silent(route: &model::Route) -> bool {
    route.hops.iter().all(|hop| hop.recv == 0)
}

fn run(cli: &Cli) -> Result<ExitCode> {
    let dns = Dns::new(&cli.dns)?;
    let plan = plan::build(cli, &dns)?;
    if !cli.quiet {
        for skipped in &plan.skipped {
            eprintln!("skip {skipped}");
        }
    }
    let settings = Settings {
        privilege: plan.privilege,
        min_rounds: usize::from(cli.min_rounds.min(cli.max_rounds)),
        max_rounds: usize::from(cli.max_rounds),
        time_limit: cli.time_limit,
        interval: cli.interval,
        max_ttl: cli.max_ttl,
        tcp_port: cli.port,
    };
    let parallel = cli
        .parallel
        .map_or(DEFAULT_PARALLEL.min(plan.jobs.len()), usize::from);
    let progress: Vec<Mutex<Progress>> = plan
        .jobs
        .iter()
        .map(|_| {
            Mutex::new(Progress {
                label: String::new(),
                rounds: 0,
                hops: 0,
                reached: false,
                done: false,
            })
        })
        .collect();

    let show_progress = !cli.quiet && std::io::stderr().is_terminal();
    let finished = AtomicBool::new(false);
    let outcomes = thread::scope(|scope| {
        if show_progress {
            scope.spawn(|| {
                while !finished.load(Ordering::SeqCst) {
                    print_progress(&progress);
                    thread::sleep(Duration::from_millis(250));
                }
                eprint!("\r{:PROGRESS_WIDTH$}\r", "");
            });
        }
        let outcomes = runner::run_all(&plan.jobs, parallel, &settings, &progress);
        finished.store(true, Ordering::SeqCst);
        outcomes
    });

    let mut routes = Vec::new();
    for outcome in outcomes {
        match outcome.result {
            Ok(route) if silent(&route) => {
                eprintln!("error {}: no probe was answered", outcome.label);
            }
            Ok(route) => routes.push(route),
            Err(error) => eprintln!("error {}: {error:#}", outcome.label),
        }
    }
    if routes.is_empty() {
        anyhow::bail!("no route could be traced");
    }
    routes.sort_by_key(|route| (route.target.clone(), route.family, !route.reached));

    if !cli.no_rdns {
        let unanswered = dns.annotate(&mut routes);
        if unanswered > 0 && !cli.quiet {
            eprintln!("note: no reverse DNS or AS answer in time for {unanswered} addresses");
        }
    }
    // With --json the tables go to stderr so stdout holds only the JSON document.
    if !cli.quiet {
        for route in &routes {
            if cli.json {
                eprintln!("{}\n", output::render(route));
            } else {
                println!("{}\n", output::render(route));
            }
        }
    }

    let body = model::Upload {
        schema: 1,
        client: model::Client::current(),
        share_origin: !cli.hide_origin,
        pin: cli.pin.clone(),
        routes,
    };
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&body)?);
    }
    if cli.no_upload {
        return Ok(ExitCode::SUCCESS);
    }
    match upload::upload(&cli.api, &body, &dns) {
        Ok(accepted) => {
            if cli.quiet {
                println!("{}", accepted.url);
            } else {
                println!("trace-id {}\n{}", accepted.id, accepted.url);
                if accepted.pin {
                    println!("Readers also need the PIN you set.");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            Ok(ExitCode::from(2))
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Some(Command::Manage(SelfCommand::Update { force, tag })) => {
            manage::update(*force, tag.as_deref()).map(|()| ExitCode::SUCCESS)
        }
        Some(Command::Manage(SelfCommand::Uninstall)) => {
            manage::uninstall().map(|()| ExitCode::SUCCESS)
        }
        None => run(&cli),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
