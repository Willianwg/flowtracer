use std::io::Write;
use std::time::Duration;

use crate::model::error::ErrorDetail;
use crate::model::span::Span;
use crate::model::trace::Trace;

use super::colors::ColorConfig;
use super::Renderer;

const DEFAULT_TERMINAL_WIDTH: usize = 80;
const MAX_MESSAGE_LEN: usize = 10240;
const MAX_REQUEST_BODY_DISPLAY: usize = 512;
const REQUEST_BODY_PREFIX: &str = "Request Body:";

const BRANCH_MID: &str = "├── ";
const BRANCH_END: &str = "└── ";
const BRANCH_PIPE: &str = "│   ";
const BRANCH_SPACE: &str = "    ";

pub struct TreeRenderer {
    pub colors: ColorConfig,
    pub terminal_width: usize,
}

#[allow(dead_code)]
impl TreeRenderer {
    pub fn new(colors: ColorConfig) -> Self {
        let terminal_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(DEFAULT_TERMINAL_WIDTH);

        Self {
            colors,
            terminal_width,
        }
    }

    pub fn with_width(colors: ColorConfig, width: usize) -> Self {
        Self {
            colors,
            terminal_width: width,
        }
    }

    fn render_header(&self, trace: &Trace, w: &mut dyn Write) -> anyhow::Result<()> {
        let id_label = format!("Trace {}", trace.id);
        let duration_label = format_duration(trace.total_duration);

        let status_label = if trace.has_error {
            format!(
                "❌ {} error{}",
                trace.error_count,
                if trace.error_count == 1 { "" } else { "s" }
            )
        } else {
            "✅ ok".to_string()
        };

        let fixed_parts_len = id_label.len() + 2 + duration_label.len() + 2 + status_label.len();
        let dash_count = if self.terminal_width > fixed_parts_len + 4 {
            self.terminal_width - fixed_parts_len - 4
        } else {
            6
        };
        let dashes: String = "─".repeat(dash_count);

        if trace.has_error {
            self.colors.write_error(w, &id_label)?;
        } else {
            self.colors.write_bold(w, &id_label)?;
        }
        write!(w, "  ")?;
        self.colors.write_dim(w, &dashes)?;
        write!(w, "  ")?;
        self.colors.write_duration(w, &duration_label)?;
        write!(w, "  ")?;
        if trace.has_error {
            self.colors.write_error(w, &status_label)?;
        } else {
            self.colors.write_success(w, &status_label)?;
        }
        writeln!(w)?;

        Ok(())
    }

    fn render_span(
        &self,
        span: &Span,
        w: &mut dyn Write,
        prefix: &str,
        is_last: bool,
        is_root: bool,
    ) -> anyhow::Result<()> {
        let connector = if is_root {
            ""
        } else if is_last {
            BRANCH_END
        } else {
            BRANCH_MID
        };

        let name = truncate_name(
            &span.name,
            self.terminal_width,
            prefix.len() + connector.len(),
        );
        let duration_str = format_duration(span.duration());

        if is_root {
            writeln!(w)?;
        }

        write!(w, "{}", prefix)?;

        if span.has_error && !is_root {
            self.colors
                .write_error(w, &format!("{}❌ {}", connector, name))?;
        } else if span.has_error && is_root {
            self.colors.write_error(w, &format!("❌ {}", name))?;
        } else {
            self.colors.write_dim(w, connector)?;
            write!(w, "{}", name)?;
        }

        if !duration_str.is_empty() {
            let current_len =
                prefix.len() + connector.len() + name.len() + (if span.has_error { 3 } else { 0 });
            let padding = if self.terminal_width > current_len + duration_str.len() + 2 {
                self.terminal_width - current_len - duration_str.len()
            } else {
                2
            };
            write!(w, "{}", " ".repeat(padding))?;
            self.colors.write_duration(w, &duration_str)?;
        }

        writeln!(w)?;

        if let Some(ref error) = span.error {
            let child_prefix = if is_root {
                format!("{}    ", prefix)
            } else if is_last {
                format!("{}{}", prefix, BRANCH_SPACE)
            } else {
                format!("{}{}", prefix, BRANCH_PIPE)
            };
            self.render_error_inline(error, w, &child_prefix, span.children.is_empty())?;
            // Show request body only below the EXCEPTION line (span with direct error), not for every HTTP span with propagated error
            if is_http_request_span(&span.name) {
                if let Some(body) = get_request_body_for_display(span) {
                    let body_prefix = if span.children.is_empty() {
                        format!("{}{}", prefix, BRANCH_SPACE)
                    } else {
                        format!("{}{}", prefix, BRANCH_PIPE)
                    };
                    write!(w, "{}", body_prefix)?;
                    self.colors.write_dim(w, "Request body: ")?;
                    self.colors.write_dim(w, &body)?;
                    writeln!(w)?;
                }
            }
        }

        let child_prefix = if is_root {
            format!("{}   ", prefix)
        } else if is_last {
            format!("{}{}", prefix, BRANCH_SPACE)
        } else {
            format!("{}{}", prefix, BRANCH_PIPE)
        };

        let child_count = span.children.len();
        for (i, child) in span.children.iter().enumerate() {
            let child_is_last = i == child_count - 1 && span.error.is_none();
            self.render_span(child, w, &child_prefix, child_is_last, false)?;
        }

        Ok(())
    }

    fn render_error_inline(
        &self,
        error: &ErrorDetail,
        w: &mut dyn Write,
        prefix: &str,
        is_only_child: bool,
    ) -> anyhow::Result<()> {
        let connector = if is_only_child {
            BRANCH_END
        } else {
            BRANCH_MID
        };
        let error_text = format!("{}: {}", error.error_type, truncate_message(&error.message));

        write!(w, "{}{}", prefix, connector)?;
        self.colors.write_lightning(w, &error_text)?;
        writeln!(w)?;

        Ok(())
    }

    fn render_error_summary(&self, trace: &Trace, w: &mut dyn Write) -> anyhow::Result<()> {
        if !trace.has_error {
            return Ok(());
        }

        let errors = collect_errors(&trace.root);

        if errors.is_empty() {
            return Ok(());
        }

        writeln!(w)?;

        let header = "─── Error Summary ";
        let remaining = if self.terminal_width > header.len() {
            self.terminal_width - header.len()
        } else {
            6
        };
        let border_end: String = "─".repeat(remaining);

        self.colors
            .write_dim(w, &format!("{}{}", header, border_end))?;
        writeln!(w)?;

        for (span_name, error) in &errors {
            let line = format!(
                "  ⚡ {} │ {} → \"{}\"",
                error.error_type,
                span_name,
                truncate_message(&error.message)
            );
            self.colors.write_error(w, &line)?;
            writeln!(w)?;
        }

        let full_border: String = "─".repeat(self.terminal_width);
        self.colors.write_dim(w, &full_border)?;
        writeln!(w)?;

        Ok(())
    }
}

impl Renderer for TreeRenderer {
    fn render(&self, trace: &Trace, w: &mut dyn Write) -> anyhow::Result<()> {
        self.render_header(trace, w)?;
        self.render_span(&trace.root, w, "", true, true)?;
        self.render_error_summary(trace, w)?;
        Ok(())
    }
}

/// Render multiple traces, separated by blank lines.
pub fn render_all(
    traces: &[Trace],
    renderer: &TreeRenderer,
    w: &mut dyn Write,
) -> anyhow::Result<()> {
    for (i, trace) in traces.iter().enumerate() {
        if i > 0 {
            writeln!(w)?;
        }
        renderer.render(trace, w)?;
    }
    Ok(())
}

fn collect_errors(span: &Span) -> Vec<(&str, &ErrorDetail)> {
    let mut out = Vec::new();
    collect_errors_inner(span, &mut out);
    out
}

fn collect_errors_inner<'a>(span: &'a Span, out: &mut Vec<(&'a str, &'a ErrorDetail)>) {
    if let Some(ref error) = span.error {
        out.push((&span.name, error));
    }
    for child in &span.children {
        collect_errors_inner(child, out);
    }
}

fn format_duration(duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let millis = d.as_millis();
            if millis < 1000 {
                format!("{}ms", millis)
            } else if millis < 60_000 {
                let secs = millis as f64 / 1000.0;
                format!("{:.1}s", secs)
            } else {
                let mins = millis / 60_000;
                let secs = (millis % 60_000) / 1000;
                format!("{}m{}s", mins, secs)
            }
        }
        None => String::new(),
    }
}

fn truncate_message(msg: &str) -> &str {
    if msg.len() > MAX_MESSAGE_LEN {
        let end = msg
            .char_indices()
            .nth(MAX_MESSAGE_LEN)
            .map(|(i, _)| i)
            .unwrap_or(msg.len());
        &msg[..end]
    } else {
        msg
    }
}

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

/// Extract "Request Body: ..." from the first matching log event; returns truncated body for display.
fn get_request_body_for_display(span: &Span) -> Option<String> {
    for event in &span.events {
        if event.message.starts_with(REQUEST_BODY_PREFIX) {
            let body = event.message[REQUEST_BODY_PREFIX.len()..].trim();
            let display = if body.len() > MAX_REQUEST_BODY_DISPLAY {
                format!("{}…", &body[..body.char_indices().nth(MAX_REQUEST_BODY_DISPLAY).map(|(i, _)| i).unwrap_or(body.len())])
            } else {
                body.to_string()
            };
            return Some(display);
        }
    }
    None
}

fn truncate_name(name: &str, terminal_width: usize, prefix_len: usize) -> String {
    let max_name_len = if terminal_width > prefix_len + 15 {
        terminal_width - prefix_len - 15
    } else {
        20
    };

    let char_count = name.chars().count();
    if char_count > max_name_len {
        let truncated: String = name.chars().take(max_name_len - 1).collect();
        format!("{}…", truncated)
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::error::{ErrorDetail, ErrorType};
    use crate::model::span::Span;
    use crate::model::trace::Trace;
    use chrono::NaiveDate;

    fn make_dt(h: u32, m: u32, s: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 12)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    fn make_renderer() -> TreeRenderer {
        TreeRenderer::with_width(ColorConfig::new(false), 80)
    }

    fn render_to_string(trace: &Trace) -> String {
        let renderer = make_renderer();
        let mut buf = Vec::new();
        renderer.render(trace, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    // ── format_duration ─────────────────────────────────────────────

    #[test]
    fn format_duration_millis() {
        assert_eq!(format_duration(Some(Duration::from_millis(12))), "12ms");
        assert_eq!(format_duration(Some(Duration::from_millis(999))), "999ms");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Some(Duration::from_millis(1500))), "1.5s");
        assert_eq!(format_duration(Some(Duration::from_millis(59999))), "60.0s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(Some(Duration::from_secs(90))), "1m30s");
    }

    #[test]
    fn format_duration_none() {
        assert_eq!(format_duration(None), "");
    }

    // ── Snapshot: trace without errors ───────────────────────────────

    #[test]
    fn snapshot_trace_no_errors() {
        let mut root = Span::new("ListProductsController");
        root.start_time = Some(make_dt(10, 0, 0));
        root.end_time = Some(make_dt(10, 0, 0)); // 200ms
        root.end_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_milli_opt(10, 0, 0, 200)
                .unwrap(),
        );

        let mut get_products = Span::new("GetProducts");
        get_products.start_time = Some(make_dt(10, 0, 0));
        get_products.end_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_milli_opt(10, 0, 0, 150)
                .unwrap(),
        );

        root.add_child(get_products);

        let trace = Trace::from_root_span(root, Some("def-456".into()));
        let output = render_to_string(&trace);

        insta::assert_snapshot!(output);
    }

    // ── Snapshot: trace with 1 error ────────────────────────────────

    #[test]
    fn snapshot_trace_with_error() {
        let mut root = Span::new("CreateOrderController");
        root.start_time = Some(make_dt(10, 10, 1));
        root.end_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_milli_opt(10, 10, 1, 12)
                .unwrap(),
        );

        let mut get_user = Span::new("GetUser");
        get_user.start_time = Some(make_dt(10, 10, 1));
        get_user.end_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_milli_opt(10, 10, 1, 3)
                .unwrap(),
        );

        let mut get_cart = Span::new("GetCart");
        get_cart.start_time = Some(make_dt(10, 10, 1));
        get_cart.end_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_milli_opt(10, 10, 1, 2)
                .unwrap(),
        );

        let mut create_invoice = Span::new("CreateInvoice");
        create_invoice.start_time = Some(make_dt(10, 10, 1));
        create_invoice.end_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_milli_opt(10, 10, 1, 5)
                .unwrap(),
        );

        let mut get_provider = Span::new("GetProvider");
        get_provider.error = Some(ErrorDetail::new(
            "No provider found with name \"paypau\"",
            ErrorType::Throw,
        ));

        create_invoice.add_child(get_provider);
        root.add_child(get_user);
        root.add_child(get_cart);
        root.add_child(create_invoice);

        let trace = Trace::from_root_span(root, Some("abc-123".into()));
        let output = render_to_string(&trace);

        insta::assert_snapshot!(output);
    }

    // ── Snapshot: trace with multiple errors ────────────────────────

    #[test]
    fn snapshot_trace_multiple_errors() {
        let mut root = Span::new("BatchProcessor");
        root.start_time = Some(make_dt(10, 0, 0));
        root.end_time = Some(make_dt(10, 0, 5));

        let mut service_a = Span::new("ServiceA");
        service_a.error = Some(ErrorDetail::new("Connection refused", ErrorType::Exception));

        let mut service_b = Span::new("ServiceB");
        service_b.error = Some(ErrorDetail::new("Request timed out", ErrorType::Timeout));

        root.add_child(Span::new("HealthCheck"));
        root.add_child(service_a);
        root.add_child(service_b);

        let trace = Trace::from_root_span(root, Some("batch-1".into()));
        let output = render_to_string(&trace);

        insta::assert_snapshot!(output);
    }

    // ── No-color: no ANSI escape sequences ──────────────────────────

    #[test]
    fn no_color_output_has_no_escape_sequences() {
        let mut root = Span::new("Controller");
        root.start_time = Some(make_dt(10, 0, 0));
        root.end_time = Some(make_dt(10, 0, 1));

        let mut child = Span::new("FailingService");
        child.error = Some(ErrorDetail::new("boom", ErrorType::Throw));
        root.add_child(child);

        let trace = Trace::from_root_span(root, Some("req-1".into()));

        let renderer = TreeRenderer::with_width(ColorConfig::new(false), 80);
        let mut buf = Vec::new();
        renderer.render(&trace, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(
            !output.contains("\x1b["),
            "Output should not contain ANSI escape sequences when color is disabled"
        );
    }

    // ── Color mode: produces ANSI escape sequences ──────────────────

    #[test]
    fn color_output_has_escape_sequences() {
        let mut root = Span::new("Controller");
        root.start_time = Some(make_dt(10, 0, 0));
        root.end_time = Some(make_dt(10, 0, 1));

        let mut child = Span::new("FailingService");
        child.error = Some(ErrorDetail::new("boom", ErrorType::Throw));
        root.add_child(child);

        let trace = Trace::from_root_span(root, Some("req-1".into()));

        let renderer = TreeRenderer::with_width(ColorConfig::new(true), 80);
        let mut buf = Vec::new();
        renderer.render(&trace, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(
            output.contains("\x1b["),
            "Output should contain ANSI escape sequences when color is enabled"
        );
    }

    // ── Duration alignment ──────────────────────────────────────────

    #[test]
    fn duration_aligned_right() {
        let mut root = Span::new("Controller");
        root.start_time = Some(make_dt(10, 0, 0));
        root.end_time = Some(make_dt(10, 0, 1));

        let trace = Trace::from_root_span(root, Some("req-1".into()));
        let output = render_to_string(&trace);

        let lines: Vec<&str> = output.lines().collect();
        let header = lines[0];
        assert!(header.contains("1.0s"), "Header should contain duration");
    }

    // ── render_all separates traces ─────────────────────────────────

    #[test]
    fn render_all_separates_with_blank_line() {
        let root1 = Span::new("ControllerA");
        let root2 = Span::new("ControllerB");

        let trace1 = Trace::from_root_span(root1, Some("req-1".into()));
        let trace2 = Trace::from_root_span(root2, Some("req-2".into()));

        let renderer = make_renderer();
        let mut buf = Vec::new();
        render_all(&[trace1, trace2], &renderer, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(
            output.contains("\n\n"),
            "Multiple traces should be separated by blank lines"
        );
    }

    // ── Truncation of long names ────────────────────────────────────

    #[test]
    fn truncate_long_span_name() {
        let result = truncate_name("VeryLongControllerNameThatExceedsEverything", 40, 4);
        assert!(result.chars().count() <= 21); // max_name_len = 40-4-15 = 21
        assert!(result.ends_with('…'));
    }

    #[test]
    fn short_name_not_truncated() {
        let result = truncate_name("GetUser", 80, 4);
        assert_eq!(result, "GetUser");
    }

    // ── Empty trace ─────────────────────────────────────────────────

    #[test]
    fn renders_empty_trace() {
        let root = Span::new("(empty)");
        let trace = Trace::from_root_span(root, Some("empty".into()));
        let output = render_to_string(&trace);

        assert!(output.contains("Trace empty"));
        assert!(output.contains("(empty)"));
    }
}
