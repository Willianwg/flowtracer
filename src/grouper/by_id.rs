use std::collections::HashMap;

use crate::model::ClassifiedEvent;

use super::RequestGroup;

/// Grouping key selector — determines which field to use as the grouping ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdField {
    Request,
    Trace,
    Thread,
}

/// Extract the grouping key from an event based on the selected field.
fn extract_key(event: &ClassifiedEvent, field: IdField) -> Option<&str> {
    match field {
        IdField::Request => event.event.request_id.as_deref(),
        IdField::Trace => event.event.trace_id.as_deref(),
        IdField::Thread => event.event.thread_id.as_deref(),
    }
}

/// Group events by a specific ID field into `RequestGroup`s.
///
/// Events that lack the chosen ID are placed into an "orphan" group
/// (only included in the output if non-empty).
pub fn group_by_id(events: Vec<ClassifiedEvent>, field: IdField) -> Vec<RequestGroup> {
    let mut buckets: HashMap<String, Vec<ClassifiedEvent>> = HashMap::new();
    let mut orphans: Vec<ClassifiedEvent> = Vec::new();
    let mut insertion_order: Vec<String> = Vec::new();

    for event in events {
        if let Some(key) = extract_key(&event, field) {
            let key = key.to_string();
            if !buckets.contains_key(&key) {
                insertion_order.push(key.clone());
            }
            buckets.entry(key).or_default().push(event);
        } else {
            orphans.push(event);
        }
    }

    let mut groups: Vec<RequestGroup> = insertion_order
        .into_iter()
        .filter_map(|id| {
            buckets
                .remove(&id)
                .map(|events| RequestGroup { id, events })
        })
        .collect();

    if !orphans.is_empty() {
        groups.push(RequestGroup {
            id: "orphan".to_string(),
            events: orphans,
        });
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, LogEvent, LogLevel};

    fn make_event(message: &str, request_id: Option<&str>) -> ClassifiedEvent {
        ClassifiedEvent {
            event: LogEvent {
                timestamp: None,
                level: LogLevel::Info,
                message: message.to_string(),
                request_id: request_id.map(String::from),
                trace_id: None,
                thread_id: None,
                source_location: None,
                raw_line: message.to_string(),
                line_number: 1,
            },
            kind: EventKind::Log,
            function_name: None,
            error_detail: None,
        }
    }

    fn make_event_with_trace_id(message: &str, trace_id: Option<&str>) -> ClassifiedEvent {
        ClassifiedEvent {
            event: LogEvent {
                timestamp: None,
                level: LogLevel::Info,
                message: message.to_string(),
                request_id: None,
                trace_id: trace_id.map(String::from),
                thread_id: None,
                source_location: None,
                raw_line: message.to_string(),
                line_number: 1,
            },
            kind: EventKind::Log,
            function_name: None,
            error_detail: None,
        }
    }

    fn make_event_with_thread_id(message: &str, thread_id: Option<&str>) -> ClassifiedEvent {
        ClassifiedEvent {
            event: LogEvent {
                timestamp: None,
                level: LogLevel::Info,
                message: message.to_string(),
                request_id: None,
                trace_id: None,
                thread_id: thread_id.map(String::from),
                source_location: None,
                raw_line: message.to_string(),
                line_number: 1,
            },
            kind: EventKind::Log,
            function_name: None,
            error_detail: None,
        }
    }

    #[test]
    fn group_by_request_id_two_groups() {
        let events = vec![
            make_event("e1", Some("abc-123")),
            make_event("e2", Some("abc-123")),
            make_event("e3", Some("def-456")),
            make_event("e4", Some("abc-123")),
            make_event("e5", Some("def-456")),
        ];

        let groups = group_by_id(events, IdField::Request);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "abc-123");
        assert_eq!(groups[0].events.len(), 3);
        assert_eq!(groups[1].id, "def-456");
        assert_eq!(groups[1].events.len(), 2);
    }

    #[test]
    fn group_by_request_id_preserves_order_within_group() {
        let events = vec![
            make_event("first", Some("abc")),
            make_event("second", Some("abc")),
            make_event("third", Some("abc")),
        ];

        let groups = group_by_id(events, IdField::Request);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].events[0].event.message, "first");
        assert_eq!(groups[0].events[1].event.message, "second");
        assert_eq!(groups[0].events[2].event.message, "third");
    }

    #[test]
    fn group_by_request_id_orphans() {
        let events = vec![
            make_event("e1", Some("abc-123")),
            make_event("orphan1", None),
            make_event("orphan2", None),
        ];

        let groups = group_by_id(events, IdField::Request);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "abc-123");
        assert_eq!(groups[0].events.len(), 1);
        assert_eq!(groups[1].id, "orphan");
        assert_eq!(groups[1].events.len(), 2);
    }

    #[test]
    fn group_by_request_id_all_orphans() {
        let events = vec![make_event("e1", None), make_event("e2", None)];

        let groups = group_by_id(events, IdField::Request);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "orphan");
        assert_eq!(groups[0].events.len(), 2);
    }

    #[test]
    fn group_by_request_id_empty() {
        let groups = group_by_id(vec![], IdField::Request);
        assert!(groups.is_empty());
    }

    #[test]
    fn group_by_trace_id() {
        let events = vec![
            make_event_with_trace_id("e1", Some("trace-a")),
            make_event_with_trace_id("e2", Some("trace-b")),
            make_event_with_trace_id("e3", Some("trace-a")),
        ];

        let groups = group_by_id(events, IdField::Trace);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "trace-a");
        assert_eq!(groups[0].events.len(), 2);
        assert_eq!(groups[1].id, "trace-b");
        assert_eq!(groups[1].events.len(), 1);
    }

    #[test]
    fn group_by_thread_id() {
        let events = vec![
            make_event_with_thread_id("e1", Some("14")),
            make_event_with_thread_id("e2", Some("15")),
            make_event_with_thread_id("e3", Some("14")),
        ];

        let groups = group_by_id(events, IdField::Thread);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "14");
        assert_eq!(groups[0].events.len(), 2);
        assert_eq!(groups[1].id, "15");
        assert_eq!(groups[1].events.len(), 1);
    }

    #[test]
    fn group_preserves_insertion_order() {
        let events = vec![
            make_event("e1", Some("second")),
            make_event("e2", Some("first")),
            make_event("e3", Some("second")),
        ];

        let groups = group_by_id(events, IdField::Request);
        assert_eq!(groups[0].id, "second");
        assert_eq!(groups[1].id, "first");
    }
}
