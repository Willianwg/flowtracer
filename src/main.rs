mod classifier;
mod cli;
mod grouper;
mod input;
mod model;
mod parser;
mod renderer;
mod trace_builder;

use std::io::{self, BufRead, BufWriter, Write};

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

/// Runs the full pipeline (parse → classify → group → build → render) on the given lines
/// and writes to `writer`. Returns `true` if any output was written, `false` if there was
/// nothing to show (empty input, no structured events, or no traces after filters).
fn process_lines_and_render(
    lines: &[String],
    args: &Cli,
    writer: &mut impl Write,
) -> Result<bool> {
    if lines.is_empty() {
        return Ok(false);
    }

    let log_parser = PlainTextParser::new();
    let mut events = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(event) = log_parser.parse_line(line, i + 1) {
            events.push(event);
        }
    }

    if events.is_empty() {
        return Ok(false);
    }

    let classified = classify_all(events);

    let has_structured_events = classified.iter().any(|e| {
        matches!(
            e.kind,
            EventKind::Entry | EventKind::Exit | EventKind::Error
        )
    });

    if !has_structured_events {
        for event in &classified {
            writeln!(writer, "{}", event.event.raw_line)?;
        }
        writer.flush()?;
        return Ok(true);
    }

    let config = GroupConfig {
        time_threshold_ms: args.time_threshold,
    };
    let mut groups = group_events(classified, &config);

    if let Some(ref request_id) = args.request_id {
        groups.retain(|g| g.id == *request_id);
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
        return Ok(false);
    }

    let colors = ColorConfig::new(!args.no_color);
    let tree_renderer = TreeRenderer::new(colors);
    render_all(&traces, &tree_renderer, writer)?;
    writer.flush()?;
    Ok(true)
}

fn run_once(args: &Cli) -> Result<()> {
    let source = build_input(args.files.clone())?;
    let lines: Vec<String> = source
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(anyhow::Error::from)?;

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    let rendered = process_lines_and_render(&lines, args, &mut writer)?;

    if !rendered {
        if lines.is_empty() {
            eprintln!("No log entries found.");
        } else {
            eprintln!("No traces found matching the given filters.");
        }
    }

    Ok(())
}

/// Watch mode: read from stdin line by line, accumulate buffer, re-run pipeline and re-render on each update.
fn run_watch_stdin(args: &Cli) -> Result<()> {
    use crossterm::cursor::MoveTo;
    use crossterm::terminal::{Clear, ClearType};
    use crossterm::ExecutableCommand;

    let stdin = io::stdin();
    let mut lines_iter = stdin.lock().lines();
    let mut buffer: Vec<String> = Vec::new();
    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    loop {
        match lines_iter.next() {
            Some(Ok(line)) => {
                buffer.push(line);
                writer.execute(Clear(ClearType::All))?;
                writer.execute(MoveTo(0, 0))?;
                let rendered = process_lines_and_render(&buffer, args, &mut writer)?;
                if !rendered {
                    writeln!(writer, "Waiting for log data... (Ctrl+C to exit)")?;
                }
                writer.flush()?;
            }
            Some(Err(e)) => return Err(e.into()),
            None => break,
        }
    }

    Ok(())
}

/// Watch mode: tail -f style on a single file. Poll for new content and re-render.
fn run_watch_file(args: &Cli) -> Result<()> {
    use crossterm::cursor::MoveTo;
    use crossterm::terminal::{Clear, ClearType};
    use crossterm::ExecutableCommand;
    use std::fs::File;
    use std::io::BufRead;

    let path = args
        .files
        .first()
        .expect("watch file mode requires at least one file");
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }
    if path.is_dir() {
        anyhow::bail!("expected a file, got a directory: {}", path.display());
    }

    let mut last_line_count = 0usize;
    let poll_interval = std::time::Duration::from_millis(500);
    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    loop {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                std::thread::sleep(poll_interval);
                continue;
            }
        };
        let reader = io::BufReader::new(file);
        let lines: Vec<String> = reader.lines().collect::<Result<Vec<_>, _>>()?;
        let current_count = lines.len();
        if current_count != last_line_count {
            last_line_count = current_count;
            writer.execute(Clear(ClearType::All))?;
            writer.execute(MoveTo(0, 0))?;
            let rendered = process_lines_and_render(&lines, args, &mut writer)?;
            if !rendered {
                if lines.is_empty() {
                    writeln!(writer, "Watching {} (Ctrl+C to exit)", path.display())?;
                } else {
                    writeln!(writer, "Waiting for structured log data... (Ctrl+C to exit)")?;
                }
            }
            writer.flush()?;
        }
        std::thread::sleep(poll_interval);
    }
}

fn run() -> Result<()> {
    let args = Cli::parse();

    if args.watch {
        if args.files.is_empty() {
            run_watch_stdin(&args)
        } else {
            run_watch_file(&args)
        }
    } else {
        run_once(&args)
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
