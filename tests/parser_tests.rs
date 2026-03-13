use flowtracer::{LogLevel, LogParser, PlainTextParser};

fn parse(line: &str) -> Option<flowtracer::LogEvent> {
    PlainTextParser::new().parse_line(line, 1)
}

// ── Timestamp formats ──────────────────────────────────────────────────

#[test]
fn ts_space_separated_with_brackets() {
    let event = parse("2026-03-12 10:10:01 [INFO] hello").unwrap();
    assert!(event.timestamp.is_some());
    assert_eq!(event.level, LogLevel::Info);
    assert_eq!(event.message, "hello");
}

#[test]
fn ts_space_separated_with_millis() {
    let event = parse("2026-03-12 10:10:01.500 [WARN] slow query").unwrap();
    assert!(event.timestamp.is_some());
    assert_eq!(event.level, LogLevel::Warn);
    let ts = event.timestamp.unwrap();
    assert_eq!(ts.and_utc().timestamp_millis() % 1000, 500);
}

#[test]
fn ts_t_separated_iso8601() {
    let event = parse("2026-03-12T10:10:01 [DEBUG] init").unwrap();
    assert!(event.timestamp.is_some());
    assert_eq!(event.level, LogLevel::Debug);
}

#[test]
fn ts_bare_level_without_brackets() {
    let event = parse("2026-03-12 10:10:01 INFO starting up").unwrap();
    assert!(event.timestamp.is_some());
    assert_eq!(event.level, LogLevel::Info);
}

#[test]
fn ts_bare_error_with_millis() {
    let event = parse("2026-03-12 10:10:01.123 ERROR disk failure").unwrap();
    assert!(event.timestamp.is_some());
    assert_eq!(event.level, LogLevel::Error);
    assert_eq!(event.message, "disk failure");
}

// ── Level-only formats (no timestamp) ──────────────────────────────────

#[test]
fn bracket_level_no_timestamp() {
    let event = parse("[ERROR] something broke").unwrap();
    assert!(event.timestamp.is_none());
    assert_eq!(event.level, LogLevel::Error);
    assert_eq!(event.message, "something broke");
}

#[test]
fn level_colon_format() {
    let event = parse("INFO: application started").unwrap();
    assert!(event.timestamp.is_none());
    assert_eq!(event.level, LogLevel::Info);
    assert_eq!(event.message, "application started");
}

#[test]
fn level_colon_debug() {
    let event = parse("DEBUG: cache hit ratio 95%").unwrap();
    assert!(event.timestamp.is_none());
    assert_eq!(event.level, LogLevel::Debug);
}

#[test]
fn fatal_level() {
    let event = parse("[FATAL] out of memory").unwrap();
    assert!(event.timestamp.is_none());
    assert_eq!(event.level, LogLevel::Fatal);
}

// ── Request ID extraction ──────────────────────────────────────────────

#[test]
fn request_id_equals_format() {
    let event = parse("2026-03-12 10:10:01 [INFO] RequestId=abc-123 hello").unwrap();
    assert_eq!(event.request_id, Some("abc-123".to_string()));
}

#[test]
fn request_id_colon_format() {
    let event = parse("2026-03-12 10:10:01 [INFO] request_id:xyz-789 doing stuff").unwrap();
    assert_eq!(event.request_id, Some("xyz-789".to_string()));
}

#[test]
fn trace_id_extraction() {
    let event = parse("2026-03-12 10:10:01 [INFO] traceId=trace-456 processing").unwrap();
    assert_eq!(event.trace_id, Some("trace-456".to_string()));
}

#[test]
fn bracket_id_at_start() {
    let event =
        parse("2026-03-12 10:10:01 [INFO] [a1b2c3d4-e5f6-7890-abcd-ef1234567890] hello").unwrap();
    assert_eq!(
        event.request_id,
        Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890".to_string())
    );
}

#[test]
fn no_request_id_when_absent() {
    let event = parse("2026-03-12 10:10:01 [INFO] just a message").unwrap();
    assert!(event.request_id.is_none());
}

// ── Thread ID extraction ───────────────────────────────────────────────

#[test]
fn thread_id_pattern() {
    let event = parse("2026-03-12 10:10:01 [INFO] [Thread 14] working").unwrap();
    assert_eq!(event.thread_id, Some("14".to_string()));
}

#[test]
fn thread_name_pattern() {
    let event = parse("2026-03-12 10:10:01 [INFO] [thread-pool-3] processing").unwrap();
    assert_eq!(event.thread_id, Some("thread-pool-3".to_string()));
}

// ── Edge cases ─────────────────────────────────────────────────────────

#[test]
fn empty_line_returns_none() {
    assert!(parse("").is_none());
}

#[test]
fn whitespace_only_returns_none() {
    assert!(parse("   ").is_none());
}

#[test]
fn unrecognized_line_returns_unknown_level() {
    let event = parse("some random unstructured text").unwrap();
    assert_eq!(event.level, LogLevel::Unknown);
    assert_eq!(event.message, "some random unstructured text");
}

// ── Fixture file parsing ───────────────────────────────────────────────

#[test]
fn parse_all_fixture_lines() {
    let content = std::fs::read_to_string("tests/fixtures/plain_logs.txt").unwrap();
    let parser = PlainTextParser::new();

    let events: Vec<_> = content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| parser.parse_line(line, i + 1))
        .collect();

    assert_eq!(events.len(), 8, "Fixture has 8 non-empty lines");

    assert_eq!(events[0].request_id, Some("abc-123".to_string()));
    assert_eq!(events[0].level, LogLevel::Info);

    assert_eq!(events[4].level, LogLevel::Error);
    assert_eq!(events[4].request_id, Some("abc-123".to_string()));

    assert_eq!(events[5].request_id, Some("def-456".to_string()));
}

#[test]
fn parse_multi_request_fixture() {
    let content = std::fs::read_to_string("tests/fixtures/multi_request.txt").unwrap();
    let parser = PlainTextParser::new();

    let events: Vec<_> = content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| parser.parse_line(line, i + 1))
        .collect();

    assert_eq!(events.len(), 13);

    let req001_count = events
        .iter()
        .filter(|e| e.request_id.as_deref() == Some("req-001"))
        .count();
    let req002_count = events
        .iter()
        .filter(|e| e.request_id.as_deref() == Some("req-002"))
        .count();
    let req003_count = events
        .iter()
        .filter(|e| e.request_id.as_deref() == Some("req-003"))
        .count();

    assert_eq!(req001_count, 4);
    assert_eq!(req002_count, 5);
    assert_eq!(req003_count, 4);
}
