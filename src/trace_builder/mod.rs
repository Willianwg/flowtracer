pub mod stack;

use chrono::NaiveDateTime;

use crate::grouper::RequestGroup;
use crate::model::span::Span;
use crate::model::trace::Trace;

use self::stack::build_span_tree;

/// Transform a `RequestGroup` into a fully-formed `Trace`.
///
/// Pipeline:
/// 1. Build span tree from classified events (sibling heuristic)
/// 2. Infer missing `end_time` values from children/events
/// 3. Compute aggregate metrics via `Trace::from_root_span`
pub fn build_trace(group: RequestGroup) -> Trace {
    let group_id = group.id.clone();
    let mut root = build_span_tree(group.events);
    infer_end_times(&mut root);
    Trace::from_root_span(root, Some(group_id))
}

/// Convenience: build traces from multiple groups.
pub fn build_traces(groups: Vec<RequestGroup>) -> Vec<Trace> {
    groups.into_iter().map(build_trace).collect()
}

/// Recursively infer `end_time` for spans that have `start_time` but no
/// explicit `end_time`. Uses the latest timestamp found among children
/// and attached log events.
fn infer_end_times(span: &mut Span) {
    for child in &mut span.children {
        infer_end_times(child);
    }

    if span.start_time.is_some() && span.end_time.is_none() {
        span.end_time = find_latest_timestamp(span);
    }
}

/// Find the latest timestamp in a span's children and events.
fn find_latest_timestamp(span: &Span) -> Option<NaiveDateTime> {
    let child_max = span
        .children
        .iter()
        .filter_map(|c| c.end_time.or(c.start_time))
        .max();

    let event_max = span.events.iter().filter_map(|e| e.timestamp).max();

    match (child_max, event_max) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier;
    use crate::model::error::ErrorType;
    use crate::model::event::{ClassifiedEvent, EventKind, LogEvent, LogLevel};
    use chrono::NaiveDateTime;

    fn ts(s: &str) -> Option<NaiveDateTime> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
    }

    fn entry(name: &str, timestamp: Option<NaiveDateTime>) -> ClassifiedEvent {
        ClassifiedEvent {
            event: LogEvent {
                timestamp,
                level: LogLevel::Info,
                message: format!("Executing {name}"),
                request_id: None,
                trace_id: None,
                thread_id: None,
                source_location: None,
                raw_line: format!("[INFO] Executing {name}"),
                line_number: 1,
            },
            kind: EventKind::Entry,
            function_name: Some(name.to_string()),
            error_detail: None,
        }
    }

    fn error_event(message: &str, timestamp: Option<NaiveDateTime>) -> ClassifiedEvent {
        use crate::model::error::ErrorDetail;
        ClassifiedEvent {
            event: LogEvent {
                timestamp,
                level: LogLevel::Error,
                message: message.to_string(),
                request_id: None,
                trace_id: None,
                thread_id: None,
                source_location: None,
                raw_line: format!("[ERROR] {message}"),
                line_number: 1,
            },
            kind: EventKind::Error,
            function_name: None,
            error_detail: Some(ErrorDetail::new(message, ErrorType::Unknown)),
        }
    }

    // ── build_trace integration ─────────────────────────────────────

    #[test]
    fn build_trace_from_group() {
        let group = RequestGroup {
            id: "abc-123".to_string(),
            events: vec![
                entry("CreateOrderController", ts("2026-03-12 10:10:01")),
                entry("GetUser", ts("2026-03-12 10:10:02")),
                entry("GetCart", ts("2026-03-12 10:10:03")),
                entry("CreateInvoice", ts("2026-03-12 10:10:04")),
                error_event("No provider found", ts("2026-03-12 10:10:05")),
            ],
        };

        let trace = build_trace(group);

        assert_eq!(trace.id, "abc-123");
        assert_eq!(trace.request_id, Some("abc-123".into()));
        assert_eq!(trace.span_count, 4);
        assert_eq!(trace.error_count, 1);
        assert!(trace.has_error);

        assert_eq!(trace.root.name, "CreateOrderController");
        assert_eq!(trace.root.children.len(), 3);
        assert!(!trace.root.children[0].has_error); // GetUser
        assert!(!trace.root.children[1].has_error); // GetCart
        assert!(trace.root.children[2].has_error); // CreateInvoice
    }

    #[test]
    fn build_trace_no_errors() {
        let group = RequestGroup {
            id: "def-456".to_string(),
            events: vec![
                entry("ListProductsController", ts("2026-03-12 10:10:06")),
                entry("GetProducts", ts("2026-03-12 10:10:07")),
            ],
        };

        let trace = build_trace(group);

        assert_eq!(trace.id, "def-456");
        assert_eq!(trace.span_count, 2);
        assert_eq!(trace.error_count, 0);
        assert!(!trace.has_error);
    }

    #[test]
    fn build_trace_empty_group() {
        let group = RequestGroup {
            id: "empty".to_string(),
            events: vec![],
        };

        let trace = build_trace(group);

        assert_eq!(trace.root.name, "(empty)");
        assert_eq!(trace.span_count, 1);
        assert!(!trace.has_error);
    }

    // ── Error propagation ───────────────────────────────────────────

    #[test]
    fn error_propagates_to_root() {
        let group = RequestGroup {
            id: "req-1".to_string(),
            events: vec![
                entry("Controller", ts("2026-03-12 10:10:01")),
                entry("Service", ts("2026-03-12 10:10:02")),
                error_event("DB down", ts("2026-03-12 10:10:03")),
            ],
        };

        let trace = build_trace(group);

        assert!(trace.has_error);
        assert!(trace.root.has_error);
        assert!(trace.root.children[0].has_error);
    }

    // ── Duration inference ──────────────────────────────────────────

    #[test]
    fn infer_end_time_from_last_child() {
        let group = RequestGroup {
            id: "req-2".to_string(),
            events: vec![
                entry("Controller", ts("2026-03-12 10:10:01")),
                entry("GetUser", ts("2026-03-12 10:10:02")),
                entry("GetCart", ts("2026-03-12 10:10:04")),
            ],
        };

        let trace = build_trace(group);

        // Root start_time = 10:10:01, end_time inferred from last child = 10:10:04
        assert_eq!(trace.root.start_time, ts("2026-03-12 10:10:01"));
        assert_eq!(trace.root.end_time, ts("2026-03-12 10:10:04"));
        assert!(trace.total_duration.is_some());
        assert_eq!(trace.total_duration.unwrap().as_secs(), 3);
    }

    #[test]
    fn no_duration_when_no_timestamps() {
        let group = RequestGroup {
            id: "req-3".to_string(),
            events: vec![entry("Controller", None), entry("GetUser", None)],
        };

        let trace = build_trace(group);

        assert!(trace.total_duration.is_none());
    }

    // ── build_traces batch ──────────────────────────────────────────

    #[test]
    fn build_traces_multiple_groups() {
        let groups = vec![
            RequestGroup {
                id: "abc-123".to_string(),
                events: vec![
                    entry("ControllerA", ts("2026-03-12 10:10:01")),
                    entry("ServiceA", ts("2026-03-12 10:10:02")),
                ],
            },
            RequestGroup {
                id: "def-456".to_string(),
                events: vec![entry("ControllerB", ts("2026-03-12 10:10:03"))],
            },
        ];

        let traces = build_traces(groups);

        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].id, "abc-123");
        assert_eq!(traces[0].root.name, "ControllerA");
        assert_eq!(traces[1].id, "def-456");
        assert_eq!(traces[1].root.name, "ControllerB");
    }

    // ── Full pipeline: parse → classify → group → build ─────────────

    #[test]
    fn full_pipeline_with_fixture_data() {
        use crate::grouper::{group_events, GroupConfig};
        use crate::parser::plain::PlainTextParser;
        use crate::parser::LogParser;

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

        let parser = PlainTextParser::new();
        let events: Vec<_> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| parser.parse_line(l, i + 1))
            .collect();

        let classified = classifier::classify_all(events);
        let config = GroupConfig::default();
        let groups = group_events(classified, &config);
        let traces = build_traces(groups);

        assert_eq!(traces.len(), 2);

        // First trace: abc-123 with error
        let t1 = &traces[0];
        assert_eq!(t1.id, "abc-123");
        assert!(t1.has_error);
        assert_eq!(t1.error_count, 1);
        assert_eq!(t1.root.name, "CreateOrderController");
        assert_eq!(t1.root.children.len(), 3);
        assert!(t1.root.children[2].has_error); // CreateInvoice

        // Second trace: def-456 no error
        let t2 = &traces[1];
        assert_eq!(t2.id, "def-456");
        assert!(!t2.has_error);
        assert_eq!(t2.root.name, "ListProductsController");
    }
}
