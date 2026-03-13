use chrono::NaiveDateTime;
use std::time::Duration;

use super::span::Span;

/// A complete execution trace for a single request.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Trace {
    pub id: String,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub root: Span,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
    pub total_duration: Option<Duration>,
    pub has_error: bool,
    pub error_count: usize,
    pub span_count: usize,
}

impl Trace {
    /// Build a `Trace` from a root span, computing all aggregate metrics.
    /// Propagates errors bottom-up and derives counts from the tree.
    pub fn from_root_span(mut root: Span, request_id: Option<String>) -> Self {
        root.propagate_errors();

        let span_count = root.span_count();
        let error_count = root.error_count();
        let has_error = error_count > 0;
        let start_time = root.start_time;
        let end_time = root.end_time;
        let total_duration = root.duration();
        let id = request_id.clone().unwrap_or_else(|| root.id.to_string());

        Self {
            id,
            request_id,
            trace_id: None,
            root,
            start_time,
            end_time,
            total_duration,
            has_error,
            error_count,
            span_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::error::{ErrorDetail, ErrorType};
    use chrono::NaiveDate;

    fn make_dt(h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 12)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    #[test]
    fn from_root_span_no_errors() {
        let mut root = Span::new("Controller");
        root.start_time = Some(make_dt(10, 0, 0));
        root.end_time = Some(make_dt(10, 0, 1));
        root.add_child(Span::new("GetUser"));
        root.add_child(Span::new("GetCart"));

        let trace = Trace::from_root_span(root, Some("req-1".into()));

        assert_eq!(trace.id, "req-1");
        assert_eq!(trace.request_id, Some("req-1".into()));
        assert_eq!(trace.span_count, 3);
        assert_eq!(trace.error_count, 0);
        assert!(!trace.has_error);
        assert_eq!(trace.total_duration.unwrap().as_secs(), 1);
    }

    #[test]
    fn from_root_span_with_deep_error() {
        let mut root = Span::new("Controller");
        root.start_time = Some(make_dt(10, 0, 0));
        root.end_time = Some(make_dt(10, 0, 5));

        let mut service = Span::new("CreateInvoice");
        let mut provider = Span::new("GetProvider");
        provider.error = Some(ErrorDetail::new("No provider found", ErrorType::Throw));
        service.add_child(provider);

        root.add_child(Span::new("GetUser"));
        root.add_child(Span::new("GetCart"));
        root.add_child(service);

        let trace = Trace::from_root_span(root, Some("abc-123".into()));

        assert_eq!(trace.span_count, 5);
        assert_eq!(trace.error_count, 1);
        assert!(trace.has_error);
        // Root should have has_error propagated
        assert!(trace.root.has_error);
        // CreateInvoice should have has_error propagated
        assert!(trace.root.children[2].has_error);
    }

    #[test]
    fn from_root_span_multiple_errors() {
        let mut root = Span::new("Controller");

        let mut child_a = Span::new("ServiceA");
        child_a.error = Some(ErrorDetail::new("fail a", ErrorType::Exception));

        let mut child_b = Span::new("ServiceB");
        child_b.error = Some(ErrorDetail::new("fail b", ErrorType::Timeout));

        root.add_child(child_a);
        root.add_child(child_b);

        let trace = Trace::from_root_span(root, None);

        assert_eq!(trace.error_count, 2);
        assert_eq!(trace.span_count, 3);
        assert!(trace.has_error);
    }

    #[test]
    fn from_root_span_generates_id_from_uuid_when_no_request_id() {
        let root = Span::new("Orphan");
        let trace = Trace::from_root_span(root, None);

        assert!(trace.request_id.is_none());
        assert!(!trace.id.is_empty());
    }

    #[test]
    fn from_root_span_no_duration_without_timestamps() {
        let root = Span::new("NoDuration");
        let trace = Trace::from_root_span(root, None);
        assert!(trace.total_duration.is_none());
        assert!(trace.start_time.is_none());
        assert!(trace.end_time.is_none());
    }
}
