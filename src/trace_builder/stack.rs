use crate::model::event::{ClassifiedEvent, EventKind};
use crate::model::span::Span;

/// True if the span name looks like an outgoing HTTP request (e.g. "POST https://...").
fn is_http_request_span(name: &str) -> bool {
    let name = name.trim();
    let method_prefix = name.starts_with("GET ")
        || name.starts_with("POST ")
        || name.starts_with("PUT ")
        || name.starts_with("PATCH ")
        || name.starts_with("DELETE ")
        || name.starts_with("HEAD ");
    method_prefix && name.contains("http")
}

/// Build a tree of `Span`s from a sequence of classified events.
///
/// Uses a stack-based algorithm with the **sibling heuristic**: consecutive
/// `Entry` events without an `Exit` between them are treated as siblings
/// (children of the same parent) rather than nested. This reflects the
/// common pattern in real-world logs where functions only log on entry.
///
/// When `Exit` events are present, proper nesting is respected.
pub fn build_span_tree(events: Vec<ClassifiedEvent>) -> Span {
    if events.is_empty() {
        return Span::new("(empty)");
    }

    let mut root: Option<Span> = None;
    let mut stack: Vec<Span> = Vec::new();
    let mut prev_was_entry = false;

    for event in events {
        match event.kind {
            EventKind::Entry => {
                let name = event
                    .function_name
                    .clone()
                    .unwrap_or_else(|| "unknown".into());
                let mut span = Span::new(&name);
                span.start_time = event.event.timestamp;

                if root.is_none() {
                    root = Some(span);
                    prev_was_entry = true;
                    continue;
                }

                // Sibling heuristic: if previous event was also Entry (no Exit
                // in between) and stack is non-empty, pop the current top and
                // commit it — then push the new entry at the same level.
                // Also: consecutive outgoing HTTP requests (Start before End) are
                // concurrent/siblings; Log events between them clear prev_was_entry,
                // so we treat HTTP-after-HTTP on stack as sibling explicitly.
                let top_is_http = stack
                    .last()
                    .map(|s| is_http_request_span(&s.name))
                    .unwrap_or(false);
                let new_is_http = is_http_request_span(&name);
                if (prev_was_entry && !stack.is_empty())
                    || (new_is_http && top_is_http)
                {
                    let sibling = stack.pop().unwrap();
                    commit_span(sibling, &mut stack, &mut root);
                }

                stack.push(span);
                prev_was_entry = true;
            }

            EventKind::Exit => {
                prev_was_entry = false;
                let name = event.function_name.clone().unwrap_or_default();

                // Search stack from top for matching span
                if let Some(idx) = stack.iter().rposition(|s| s.name == name) {
                    stack[idx].end_time = event.event.timestamp;

                    // Pop everything above idx — they become children of stack[idx]
                    while stack.len() > idx + 1 {
                        let child = stack.pop().unwrap();
                        stack.last_mut().unwrap().children.push(child);
                    }

                    // Pop the matched span and commit to its parent
                    let closed = stack.pop().unwrap();
                    commit_span(closed, &mut stack, &mut root);
                } else if let Some(ref mut r) = root {
                    if r.name == name {
                        r.end_time = event.event.timestamp;
                        // Collapse any remaining stack entries into root
                        while let Some(child) = stack.pop() {
                            r.children.push(child);
                        }
                    }
                }
            }

            EventKind::Error => {
                prev_was_entry = false;
                ensure_root(&mut root);
                if let Some(top) = stack.last_mut() {
                    top.error = event.error_detail;
                    top.has_error = true;
                } else if let Some(ref mut r) = root {
                    r.error = event.error_detail;
                    r.has_error = true;
                }
            }

            EventKind::Log => {
                prev_was_entry = false;
                ensure_root(&mut root);
                if let Some(top) = stack.last_mut() {
                    top.events.push(event.event);
                } else if let Some(ref mut r) = root {
                    r.events.push(event.event);
                }
            }
        }
    }

    // Finalize: commit remaining stack entries bottom-up
    while stack.len() > 1 {
        let child = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(child);
    }
    if let Some(last) = stack.pop() {
        if let Some(ref mut r) = root {
            r.children.push(last);
        }
    }

    root.unwrap_or_else(|| Span::new("(empty)"))
}

/// Lazily create a synthetic root span when events arrive before any Entry.
fn ensure_root(root: &mut Option<Span>) {
    if root.is_none() {
        *root = Some(Span::new("(empty)"));
    }
}

/// Add a finished span to its parent: either the top of the stack or root.
fn commit_span(span: Span, stack: &mut [Span], root: &mut Option<Span>) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(span);
    } else if let Some(ref mut r) = root {
        r.children.push(span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::error::{ErrorDetail, ErrorType};
    use crate::model::event::LogEvent;
    use crate::model::event::LogLevel;
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

    fn exit(name: &str, timestamp: Option<NaiveDateTime>) -> ClassifiedEvent {
        ClassifiedEvent {
            event: LogEvent {
                timestamp,
                level: LogLevel::Info,
                message: format!("{name} completed"),
                request_id: None,
                trace_id: None,
                thread_id: None,
                source_location: None,
                raw_line: format!("[INFO] {name} completed"),
                line_number: 1,
            },
            kind: EventKind::Exit,
            function_name: Some(name.to_string()),
            error_detail: None,
        }
    }

    fn error_event(message: &str, timestamp: Option<NaiveDateTime>) -> ClassifiedEvent {
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

    fn log_event(message: &str, timestamp: Option<NaiveDateTime>) -> ClassifiedEvent {
        ClassifiedEvent {
            event: LogEvent {
                timestamp,
                level: LogLevel::Info,
                message: message.to_string(),
                request_id: None,
                trace_id: None,
                thread_id: None,
                source_location: None,
                raw_line: format!("[INFO] {message}"),
                line_number: 1,
            },
            kind: EventKind::Log,
            function_name: None,
            error_detail: None,
        }
    }

    // ── Empty input ─────────────────────────────────────────────────

    #[test]
    fn empty_events_returns_empty_span() {
        let span = build_span_tree(vec![]);
        assert_eq!(span.name, "(empty)");
        assert!(span.children.is_empty());
    }

    // ── Sequential entries without exits → siblings ─────────────────

    #[test]
    fn sequential_entries_become_siblings_under_root() {
        let events = vec![
            entry("CreateOrderController", ts("2026-03-12 10:10:01")),
            entry("GetUser", ts("2026-03-12 10:10:02")),
            entry("GetCart", ts("2026-03-12 10:10:03")),
            entry("CreateInvoice", ts("2026-03-12 10:10:04")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.name, "CreateOrderController");
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.children[0].name, "GetUser");
        assert_eq!(root.children[1].name, "GetCart");
        assert_eq!(root.children[2].name, "CreateInvoice");

        // All children are leaf nodes (no further nesting)
        for child in &root.children {
            assert!(child.children.is_empty());
        }
    }

    // ── Acceptance criteria from roadmap ─────────────────────────────

    #[test]
    fn acceptance_criteria_entries_with_error() {
        let events = vec![
            entry("CreateOrderController", ts("2026-03-12 10:10:01")),
            entry("GetUser", ts("2026-03-12 10:10:02")),
            entry("GetCart", ts("2026-03-12 10:10:03")),
            entry("CreateInvoice", ts("2026-03-12 10:10:04")),
            error_event(
                "No provider found with name \"paypau\"",
                ts("2026-03-12 10:10:05"),
            ),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.name, "CreateOrderController");
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.children[0].name, "GetUser");
        assert!(!root.children[0].has_error);
        assert_eq!(root.children[1].name, "GetCart");
        assert!(!root.children[1].has_error);
        assert_eq!(root.children[2].name, "CreateInvoice");
        assert!(root.children[2].has_error);
        assert!(root.children[2].error.is_some());
    }

    // ── Entry/Exit pairs → proper nesting ───────────────────────────

    #[test]
    fn entry_exit_pairs_nest_correctly() {
        let events = vec![
            entry("A", ts("2026-03-12 10:10:01")),
            entry("B", ts("2026-03-12 10:10:02")),
            exit("B", ts("2026-03-12 10:10:03")),
            entry("C", ts("2026-03-12 10:10:04")),
            exit("C", ts("2026-03-12 10:10:05")),
            exit("A", ts("2026-03-12 10:10:06")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.name, "A");
        assert_eq!(root.end_time, ts("2026-03-12 10:10:06"));
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].name, "B");
        assert_eq!(root.children[0].end_time, ts("2026-03-12 10:10:03"));
        assert_eq!(root.children[1].name, "C");
        assert_eq!(root.children[1].end_time, ts("2026-03-12 10:10:05"));
    }

    // ── Error attributed to the last entry before it ────────────────

    #[test]
    fn error_goes_to_last_entry() {
        let events = vec![
            entry("Controller", ts("2026-03-12 10:10:01")),
            entry("ServiceA", ts("2026-03-12 10:10:02")),
            entry("ServiceB", ts("2026-03-12 10:10:03")),
            error_event("Something broke", ts("2026-03-12 10:10:04")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.children.len(), 2);
        assert!(root.children[0].error.is_none()); // ServiceA
        assert!(root.children[1].error.is_some()); // ServiceB — last entry
        assert_eq!(root.children[1].name, "ServiceB");
    }

    // ── Error on root when no children ──────────────────────────────

    #[test]
    fn error_on_root_when_only_root() {
        let events = vec![
            entry("Controller", ts("2026-03-12 10:10:01")),
            error_event("Root failed", ts("2026-03-12 10:10:02")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.name, "Controller");
        assert!(root.children.is_empty());
        assert!(root.has_error);
        assert!(root.error.is_some());
    }

    // ── Log events go to current span ───────────────────────────────

    #[test]
    fn log_events_added_to_current_span() {
        let events = vec![
            entry("Controller", ts("2026-03-12 10:10:01")),
            entry("GetUser", ts("2026-03-12 10:10:02")),
            log_event("Cache miss for user", ts("2026-03-12 10:10:03")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "GetUser");
        assert_eq!(root.children[0].events.len(), 1);
        assert_eq!(root.children[0].events[0].message, "Cache miss for user");
    }

    #[test]
    fn log_event_on_root_when_no_children() {
        let events = vec![
            entry("Controller", ts("2026-03-12 10:10:01")),
            log_event("Initializing", ts("2026-03-12 10:10:02")),
        ];

        let root = build_span_tree(events);

        assert!(root.children.is_empty());
        assert_eq!(root.events.len(), 1);
    }

    // ── Single entry ────────────────────────────────────────────────

    #[test]
    fn single_entry_becomes_root() {
        let events = vec![entry("Controller", ts("2026-03-12 10:10:01"))];

        let root = build_span_tree(events);

        assert_eq!(root.name, "Controller");
        assert!(root.children.is_empty());
        assert_eq!(root.start_time, ts("2026-03-12 10:10:01"));
    }

    // ── Mixed: some with exits, some without ────────────────────────

    #[test]
    fn mixed_exit_and_sequential() {
        let events = vec![
            entry("A", ts("2026-03-12 10:10:01")),
            entry("B", ts("2026-03-12 10:10:02")),
            exit("B", ts("2026-03-12 10:10:03")),
            entry("C", ts("2026-03-12 10:10:04")),
            entry("D", ts("2026-03-12 10:10:05")),
            error_event("fail", ts("2026-03-12 10:10:06")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.name, "A");
        assert_eq!(root.children.len(), 3);
        assert_eq!(root.children[0].name, "B");
        assert!(root.children[0].end_time.is_some()); // properly exited
        assert_eq!(root.children[1].name, "C");
        assert_eq!(root.children[2].name, "D");
        assert!(root.children[2].error.is_some());
    }

    // ── Timestamps preserved ────────────────────────────────────────

    #[test]
    fn start_times_are_preserved() {
        let events = vec![
            entry("A", ts("2026-03-12 10:10:01")),
            entry("B", ts("2026-03-12 10:10:02")),
            entry("C", ts("2026-03-12 10:10:03")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.start_time, ts("2026-03-12 10:10:01"));
        assert_eq!(root.children[0].start_time, ts("2026-03-12 10:10:02"));
        assert_eq!(root.children[1].start_time, ts("2026-03-12 10:10:03"));
    }

    // ── Only log/error events (no entries) ──────────────────────────

    #[test]
    fn only_error_creates_empty_root_with_error() {
        let events = vec![error_event("crash", ts("2026-03-12 10:10:01"))];

        let root = build_span_tree(events);

        // No Entry → empty root gets the error
        assert_eq!(root.name, "(empty)");
        assert!(root.has_error);
    }

    #[test]
    fn only_logs_creates_empty_root_with_events() {
        let events = vec![
            log_event("line 1", ts("2026-03-12 10:10:01")),
            log_event("line 2", ts("2026-03-12 10:10:02")),
        ];

        let root = build_span_tree(events);

        assert_eq!(root.name, "(empty)");
        assert_eq!(root.events.len(), 2);
    }
}
