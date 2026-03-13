use chrono::NaiveDateTime;
use regex::Regex;
use std::sync::LazyLock;

use super::LogParser;
use crate::model::{LogEvent, LogLevel};

// ── Timestamp + Level patterns ──────────────────────────────────────────────

// "2026-03-12 10:10:01.123 [INFO] message"  or  "2026-03-12 10:10:01 [INFO] message"
static RE_TS_BRACKET_LEVEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?)\s+\[(\w+)\]\s*(.*)")
        .unwrap()
});

// "2026-03-12T10:10:01.123 INFO message"  or  "2026-03-12 10:10:01 INFO message"
static RE_TS_BARE_LEVEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?)\s+(TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|FATAL)\s+(.*)",
    )
    .unwrap()
});

// "[INFO] message"  (no timestamp)
static RE_BRACKET_LEVEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\w+)\]\s*(.*)").unwrap());

// "INFO: message"  or  "ERROR: message"
static RE_LEVEL_COLON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(TRACE|DEBUG|INFO|WARN(?:ING)?|ERROR|FATAL):\s*(.*)").unwrap());

// ── Request ID extraction ───────────────────────────────────────────────────

// RequestId=abc-123  or  request_id:abc-123  or  traceId=abc-123
static RE_REQUEST_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:RequestId=|request_id:|traceId=)([\w\-.:]+)").unwrap());

// [abc-123-def] at start of message (UUID-like or alphanumeric-dash pattern, ≥ 5 chars)
static RE_BRACKET_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[([\w\-]{5,})\]\s*(.*)").unwrap());

// ── Thread ID extraction ────────────────────────────────────────────────────

// [Thread 14] or [thread-pool-1]
static RE_THREAD_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[Thread\s+(\S+)\]").unwrap());

static RE_THREAD_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(thread[\w\-]*\d+)\]").unwrap());

// ── Timestamp parsing helpers ───────────────────────────────────────────────

fn parse_timestamp(s: &str) -> Option<NaiveDateTime> {
    let normalized = s.replace('T', " ");
    let trimmed = normalized.trim();

    NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.3f")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f"))
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

fn parse_level(s: &str) -> LogLevel {
    match s.to_uppercase().as_str() {
        "TRACE" => LogLevel::Trace,
        "DEBUG" => LogLevel::Debug,
        "INFO" => LogLevel::Info,
        "WARN" | "WARNING" => LogLevel::Warn,
        "ERROR" | "ERR" => LogLevel::Error,
        "FATAL" | "CRITICAL" | "CRIT" => LogLevel::Fatal,
        _ => LogLevel::Unknown,
    }
}

// ── Request/Thread ID extraction from message ───────────────────────────────

struct ExtractedIds {
    request_id: Option<String>,
    trace_id: Option<String>,
    thread_id: Option<String>,
    clean_message: String,
}

fn extract_ids(message: &str) -> ExtractedIds {
    let mut request_id = None;
    let mut trace_id = None;
    let mut clean = message.to_string();

    // Extract RequestId=..., request_id:..., traceId=...
    if let Some(caps) = RE_REQUEST_ID.captures(message) {
        let id = caps[1].to_string();
        let full_match = caps.get(0).unwrap().as_str();

        if full_match.to_lowercase().starts_with("traceid") {
            trace_id = Some(id);
        } else {
            request_id = Some(id);
        }

        clean = clean.replace(full_match, "").trim().to_string();
    }

    // Try [bracket-id] at start of message (only if no request_id found yet)
    if request_id.is_none() && trace_id.is_none() {
        if let Some(caps) = RE_BRACKET_ID.captures(&clean) {
            request_id = Some(caps[1].to_string());
            clean = caps[2].to_string();
        }
    }

    // Extract thread ID
    let thread_id = RE_THREAD_ID
        .captures(message)
        .map(|c| c[1].to_string())
        .or_else(|| RE_THREAD_NAME.captures(message).map(|c| c[1].to_string()));

    if let Some(ref tid_match) = RE_THREAD_ID.find(message) {
        clean = clean.replace(tid_match.as_str(), "").trim().to_string();
    }

    ExtractedIds {
        request_id,
        trace_id,
        thread_id,
        clean_message: clean.trim().to_string(),
    }
}

// ── PlainTextParser ─────────────────────────────────────────────────────────

/// Parser for common plain-text log formats.
pub struct PlainTextParser;

impl PlainTextParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlainTextParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LogParser for PlainTextParser {
    fn parse_line(&self, line: &str, line_number: usize) -> Option<LogEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let (timestamp, level, raw_message) = parse_timestamp_and_level(trimmed);
        let ids = extract_ids(&raw_message);

        Some(LogEvent {
            timestamp,
            level,
            message: ids.clean_message,
            request_id: ids.request_id,
            trace_id: ids.trace_id,
            thread_id: ids.thread_id,
            source_location: None,
            raw_line: line.to_string(),
            line_number,
        })
    }
}

/// Try each pattern in priority order and return (timestamp, level, message).
fn parse_timestamp_and_level(line: &str) -> (Option<NaiveDateTime>, LogLevel, String) {
    // Pattern 1: timestamp [LEVEL] message
    if let Some(caps) = RE_TS_BRACKET_LEVEL.captures(line) {
        let ts = parse_timestamp(&caps[1]);
        let level = parse_level(&caps[2]);
        return (ts, level, caps[3].to_string());
    }

    // Pattern 2: timestamp LEVEL message
    if let Some(caps) = RE_TS_BARE_LEVEL.captures(line) {
        let ts = parse_timestamp(&caps[1]);
        let level = parse_level(&caps[2]);
        return (ts, level, caps[3].to_string());
    }

    // Pattern 3: [LEVEL] message (no timestamp)
    if let Some(caps) = RE_BRACKET_LEVEL.captures(line) {
        let level = parse_level(&caps[1]);
        if level != LogLevel::Unknown {
            return (None, level, caps[2].to_string());
        }
    }

    // Pattern 4: LEVEL: message
    if let Some(caps) = RE_LEVEL_COLON.captures(line) {
        let level = parse_level(&caps[1]);
        return (None, level, caps[2].to_string());
    }

    // Fallback: unknown level, entire line is the message
    (None, LogLevel::Unknown, line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Option<LogEvent> {
        PlainTextParser::new().parse_line(line, 1)
    }

    // ── Empty / whitespace ──────────────────────────────────────────────

    #[test]
    fn empty_line_returns_none() {
        assert!(parse("").is_none());
        assert!(parse("   ").is_none());
        assert!(parse("\t\n").is_none());
    }

    // ── Pattern 1: YYYY-MM-DD HH:MM:SS [LEVEL] message ─────────────────

    #[test]
    fn ts_space_bracket_level() {
        let e = parse("2026-03-12 10:10:01 [INFO] Executing CreateOrderController").unwrap();
        assert_eq!(e.level, LogLevel::Info);
        assert_eq!(e.message, "Executing CreateOrderController");
        assert!(e.timestamp.is_some());
        let ts = e.timestamp.unwrap();
        assert_eq!(
            ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-03-12 10:10:01"
        );
    }

    #[test]
    fn ts_space_bracket_level_with_millis() {
        let e = parse("2026-03-12 10:10:01.456 [ERROR] Something broke").unwrap();
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.message, "Something broke");
        let ts = e.timestamp.unwrap();
        assert_eq!(ts.format("%H:%M:%S%.3f").to_string(), "10:10:01.456");
    }

    #[test]
    fn ts_t_separator_bracket_level() {
        let e = parse("2026-03-12T10:10:01 [WARN] Low memory").unwrap();
        assert_eq!(e.level, LogLevel::Warn);
        assert_eq!(e.message, "Low memory");
        assert!(e.timestamp.is_some());
    }

    // ── Pattern 2: YYYY-MM-DD HH:MM:SS LEVEL message ───────────────────

    #[test]
    fn ts_bare_level() {
        let e = parse("2026-03-12 10:10:01 INFO Starting server").unwrap();
        assert_eq!(e.level, LogLevel::Info);
        assert_eq!(e.message, "Starting server");
        assert!(e.timestamp.is_some());
    }

    #[test]
    fn ts_bare_level_error_with_millis() {
        let e = parse("2026-03-12T10:10:01.999 ERROR Connection refused").unwrap();
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.message, "Connection refused");
    }

    #[test]
    fn ts_bare_level_warning_full() {
        let e = parse("2026-03-12 10:10:01 WARNING Disk almost full").unwrap();
        assert_eq!(e.level, LogLevel::Warn);
        assert_eq!(e.message, "Disk almost full");
    }

    // ── Pattern 3: [LEVEL] message (no timestamp) ──────────────────────

    #[test]
    fn bracket_level_no_timestamp() {
        let e = parse("[INFO] Executing handler").unwrap();
        assert!(e.timestamp.is_none());
        assert_eq!(e.level, LogLevel::Info);
        assert_eq!(e.message, "Executing handler");
    }

    #[test]
    fn bracket_level_fatal() {
        let e = parse("[FATAL] Out of memory").unwrap();
        assert_eq!(e.level, LogLevel::Fatal);
        assert_eq!(e.message, "Out of memory");
    }

    // ── Pattern 4: LEVEL: message ───────────────────────────────────────

    #[test]
    fn level_colon() {
        let e = parse("ERROR: Database connection lost").unwrap();
        assert!(e.timestamp.is_none());
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.message, "Database connection lost");
    }

    #[test]
    fn level_colon_debug() {
        let e = parse("DEBUG: Cache hit for key=user:42").unwrap();
        assert_eq!(e.level, LogLevel::Debug);
        assert_eq!(e.message, "Cache hit for key=user:42");
    }

    // ── Fallback: unrecognized format ───────────────────────────────────

    #[test]
    fn unrecognized_line_returns_unknown_level() {
        let e = parse("some random log line without pattern").unwrap();
        assert_eq!(e.level, LogLevel::Unknown);
        assert_eq!(e.message, "some random log line without pattern");
        assert!(e.timestamp.is_none());
    }

    // ── Request ID extraction ───────────────────────────────────────────

    #[test]
    fn extract_request_id_equals() {
        let e = parse("2026-03-12 10:10:01 [INFO] RequestId=abc-123 Executing GetUser").unwrap();
        assert_eq!(e.request_id, Some("abc-123".into()));
        assert_eq!(e.message, "Executing GetUser");
    }

    #[test]
    fn extract_request_id_colon() {
        let e = parse("[INFO] request_id:req-999 Processing order").unwrap();
        assert_eq!(e.request_id, Some("req-999".into()));
        assert_eq!(e.message, "Processing order");
    }

    #[test]
    fn extract_trace_id() {
        let e = parse("2026-03-12 10:10:01 [INFO] traceId=4bf92f35 Handling request").unwrap();
        assert_eq!(e.trace_id, Some("4bf92f35".into()));
        assert!(e.request_id.is_none());
        assert_eq!(e.message, "Handling request");
    }

    #[test]
    fn extract_bracket_id_at_start() {
        let e = parse("[INFO] [abc-123-def] Executing CreateOrder").unwrap();
        assert_eq!(e.request_id, Some("abc-123-def".into()));
        assert_eq!(e.message, "Executing CreateOrder");
    }

    #[test]
    fn no_request_id_when_absent() {
        let e = parse("[INFO] Just a regular log message").unwrap();
        assert!(e.request_id.is_none());
        assert!(e.trace_id.is_none());
    }

    // ── Thread ID extraction ────────────────────────────────────────────

    #[test]
    fn extract_thread_id_pattern() {
        let e = parse("2026-03-12 10:10:01 [INFO] [Thread 14] Processing").unwrap();
        assert_eq!(e.thread_id, Some("14".into()));
    }

    #[test]
    fn extract_thread_name_pattern() {
        let e = parse("[INFO] [thread-pool-3] Handling request").unwrap();
        assert_eq!(e.thread_id, Some("thread-pool-3".into()));
    }

    // ── Preserves raw_line and line_number ───────────────────────────────

    #[test]
    fn preserves_raw_line() {
        let raw = "2026-03-12 10:10:01 [INFO] RequestId=x Hello";
        let e = parse(raw).unwrap();
        assert_eq!(e.raw_line, raw);
        assert_eq!(e.line_number, 1);
    }

    // ── Fixture integration ─────────────────────────────────────────────

    #[test]
    fn parse_fixture_lines() {
        let parser = PlainTextParser::new();
        let lines = [
            "2026-03-12 10:10:01 [INFO] RequestId=abc-123 Executing CreateOrderController",
            "2026-03-12 10:10:02 [INFO] RequestId=abc-123 Executing GetUser",
            "2026-03-12 10:10:03 [INFO] RequestId=abc-123 Executing GetCart",
            "2026-03-12 10:10:04 [INFO] RequestId=abc-123 Executing CreateInvoice",
            "2026-03-12 10:10:05 [ERROR] RequestId=abc-123 No provider found with name \"paypau\"",
            "2026-03-12 10:10:06 [INFO] RequestId=def-456 Executing ListProductsController",
            "2026-03-12 10:10:07 [INFO] RequestId=def-456 Executing GetProducts",
            "2026-03-12 10:10:08 [INFO] RequestId=def-456 Completed successfully",
        ];

        let events: Vec<LogEvent> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| parser.parse_line(l, i + 1))
            .collect();

        assert_eq!(events.len(), 8);

        // First event
        assert_eq!(events[0].request_id, Some("abc-123".into()));
        assert_eq!(events[0].level, LogLevel::Info);
        assert_eq!(events[0].message, "Executing CreateOrderController");
        assert!(events[0].timestamp.is_some());

        // Error event
        assert_eq!(events[4].level, LogLevel::Error);
        assert_eq!(events[4].request_id, Some("abc-123".into()));
        assert!(events[4].message.contains("No provider found"));

        // Second request
        assert_eq!(events[5].request_id, Some("def-456".into()));
        assert_eq!(events[7].message, "Completed successfully");
    }
}
