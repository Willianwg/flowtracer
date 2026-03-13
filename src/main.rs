mod classifier;
mod cli;
mod grouper;
mod input;
mod model;
mod parser;
mod renderer;
mod trace_builder;

use std::io::{self, Write};

use anyhow::Result;
use clap::Parser;

use classifier::classify_all;
use cli::Cli;
use grouper::{group_events, GroupConfig};
use input::build_input;
use model::EventKind;
use parser::plain::PlainTextParser;
use parser::LogParser;
use renderer::colors::ColorConfig;
use renderer::tree::{render_all, TreeRenderer};
use trace_builder::build_traces;

fn run() -> Result<()> {
    let args = Cli::parse();

    let source = build_input(args.files)?;
    let lines = source.lines();

    let log_parser = PlainTextParser::new();
    let mut events = Vec::new();
    for (i, line) in lines.enumerate() {
        let line = line?;
        if let Some(event) = log_parser.parse_line(&line, i + 1) {
            events.push(event);
        }
    }

    if events.is_empty() {
        eprintln!("No log entries found.");
        return Ok(());
    }

    let classified = classify_all(events);

    let has_structured_events = classified.iter().any(|e| {
        matches!(
            e.kind,
            EventKind::Entry | EventKind::Exit | EventKind::Error
        )
    });

    if !has_structured_events {
        eprintln!(
            "Warning: no recognized log patterns found. \
             Showing raw log lines without trace structure."
        );
        let stdout = io::stdout();
        let mut writer = io::BufWriter::new(stdout.lock());
        for event in &classified {
            writeln!(writer, "{}", event.event.raw_line)?;
        }
        writer.flush()?;
        return Ok(());
    }

    let config = GroupConfig {
        time_threshold_ms: args.time_threshold,
    };
    let mut groups = group_events(classified, &config);

    if let Some(ref request_id) = args.request_id {
        let available_ids: Vec<String> = groups.iter().map(|g| g.id.clone()).collect();

        groups.retain(|g| g.id == *request_id);

        if groups.is_empty() {
            eprintln!("Request ID \"{}\" not found.", request_id);
            if !available_ids.is_empty() {
                eprintln!("Available request IDs:");
                for id in &available_ids {
                    eprintln!("  - {}", id);
                }
            }
            return Ok(());
        }
    }

    let mut traces = build_traces(groups);

    if args.errors_only {
        traces.retain(|t| t.has_error);
    }

    if let Some(n) = args.last {
        if traces.len() > n {
            traces = traces.split_off(traces.len() - n);
        }
    }

    if traces.is_empty() {
        eprintln!("No traces found matching the given filters.");
        return Ok(());
    }

    let colors = ColorConfig::new(!args.no_color);
    let tree_renderer = TreeRenderer::new(colors);

    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());
    render_all(&traces, &tree_renderer, &mut writer)?;
    writer.flush()?;

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
