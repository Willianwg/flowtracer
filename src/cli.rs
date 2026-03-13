use clap::Parser;
use std::path::PathBuf;

const AFTER_HELP: &str = "\x1b[1mExamples:\x1b[0m
  flowtracer app.log                    Analyze a log file
  flowtracer app.log worker.log         Analyze multiple files
  cat app.log | flowtracer              Read from stdin (pipe)
  dotnet run | flowtracer -w            Watch mode: stay open, update as pipe receives data
  flowtracer -w app.log                 Watch mode: tail -f style, update as file grows
  flowtracer -r abc-123 app.log         Show only request abc-123
  flowtracer -e app.log                 Show only traces with errors
  flowtracer -n 5 app.log               Show last 5 traces
  flowtracer --no-color app.log         Disable colored output
  flowtracer --time-threshold 2000 app.log  Custom temporal grouping gap";

/// Reconstruct and visualize execution traces from application logs.
///
/// FlowTracer reads log files (or stdin) and rebuilds the execution flow
/// per request, displaying a visual tree in the terminal with errors
/// clearly highlighted and propagated.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "flowtracer",
    version,
    about,
    long_about = "FlowTracer reads application logs and reconstructs the execution flow per \
request. It displays a visual tree in the terminal with errors clearly \
highlighted and propagated from the point of failure up to the root span.\n\n\
Supports automatic grouping by request ID, trace ID, thread ID, or \
temporal proximity when no identifiers are present.",
    after_help = AFTER_HELP,
)]
pub struct Cli {
    /// Log file(s) to analyze. Reads from stdin if omitted.
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Filter by request ID
    #[arg(short = 'r', long = "request", value_name = "ID")]
    pub request_id: Option<String>,

    /// Show only traces that contain errors
    #[arg(short = 'e', long = "errors-only")]
    pub errors_only: bool,

    /// Disable colored output
    #[arg(long = "no-color")]
    pub no_color: bool,

    /// Show only the last N traces
    #[arg(short = 'n', long = "last", value_name = "N")]
    pub last: Option<usize>,

    /// Time threshold (ms) for temporal grouping heuristic
    #[arg(long = "time-threshold", value_name = "MS", default_value = "500")]
    pub time_threshold: u64,

    /// Watch mode: stay open and refresh output as new data arrives (stdin or file)
    #[arg(short = 'w', long = "watch")]
    pub watch: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_no_args() {
        let cli = Cli::parse_from(["flowtracer"]);
        assert!(cli.files.is_empty());
        assert!(cli.request_id.is_none());
        assert!(!cli.errors_only);
        assert!(!cli.no_color);
        assert!(cli.last.is_none());
        assert_eq!(cli.time_threshold, 500);
    }

    #[test]
    fn parse_single_file() {
        let cli = Cli::parse_from(["flowtracer", "app.log"]);
        assert_eq!(cli.files, vec![PathBuf::from("app.log")]);
    }

    #[test]
    fn parse_multiple_files() {
        let cli = Cli::parse_from(["flowtracer", "app.log", "worker.log"]);
        assert_eq!(
            cli.files,
            vec![PathBuf::from("app.log"), PathBuf::from("worker.log")]
        );
    }

    #[test]
    fn parse_request_id_short() {
        let cli = Cli::parse_from(["flowtracer", "-r", "abc-123", "app.log"]);
        assert_eq!(cli.request_id, Some("abc-123".to_string()));
    }

    #[test]
    fn parse_request_id_long() {
        let cli = Cli::parse_from(["flowtracer", "--request", "abc-123"]);
        assert_eq!(cli.request_id, Some("abc-123".to_string()));
    }

    #[test]
    fn parse_errors_only() {
        let cli = Cli::parse_from(["flowtracer", "-e"]);
        assert!(cli.errors_only);
    }

    #[test]
    fn parse_no_color() {
        let cli = Cli::parse_from(["flowtracer", "--no-color"]);
        assert!(cli.no_color);
    }

    #[test]
    fn parse_last_n() {
        let cli = Cli::parse_from(["flowtracer", "-n", "10"]);
        assert_eq!(cli.last, Some(10));
    }

    #[test]
    fn parse_time_threshold() {
        let cli = Cli::parse_from(["flowtracer", "--time-threshold", "1000"]);
        assert_eq!(cli.time_threshold, 1000);
    }

    #[test]
    fn parse_watch() {
        let cli = Cli::parse_from(["flowtracer", "-w", "app.log"]);
        assert!(cli.watch);
        let cli = Cli::parse_from(["flowtracer", "--watch"]);
        assert!(cli.watch);
    }

    #[test]
    fn parse_all_flags_combined() {
        let cli = Cli::parse_from([
            "flowtracer",
            "-r",
            "req-42",
            "-e",
            "--no-color",
            "-n",
            "5",
            "--time-threshold",
            "200",
            "-w",
            "server.log",
            "worker.log",
        ]);
        assert_eq!(cli.request_id, Some("req-42".to_string()));
        assert!(cli.errors_only);
        assert!(cli.no_color);
        assert_eq!(cli.last, Some(5));
        assert_eq!(cli.time_threshold, 200);
        assert!(cli.watch);
        assert_eq!(
            cli.files,
            vec![PathBuf::from("server.log"), PathBuf::from("worker.log")]
        );
    }
}
