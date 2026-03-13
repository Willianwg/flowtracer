use regex::Regex;
use std::sync::LazyLock;

pub struct PatternMatch {
    pub name: Option<String>,
}

// ── ENTRY patterns ──────────────────────────────────────────────────────────

static ENTRY_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // ASP.NET Core: Executing endpoint 'Namespace.Controller.Action (Assembly)'
        Regex::new(r"(?i)^Executing\s+endpoint\s+'([^']+)'").unwrap(),
        // ASP.NET Core: Executing action method Namespace.Controller.Action (Assembly) - Validation state: Valid
        Regex::new(r"(?i)^Executing\s+action\s+method\s+([^\s-]+(?:\s+\([^)]+\))?)").unwrap(),
        // ASP.NET Core: Route matched with {action = "GetById", controller = "Cart"}. Executing controller action ... on controller Full.Controller.Name (Api).
        Regex::new(r"(?i)on\s+controller\s+([^\s(]+(?:\s+\([^)]+\))?)").unwrap(),
        // Generic
        Regex::new(r"(?i)^Executing\s+(?:method\s+)?(\S+)").unwrap(),
        Regex::new(r"(?i)^Enter(?:ing)?\s+(\S+)").unwrap(),
        Regex::new(r"(?i)^Starting\s+(\S+)").unwrap(),
        Regex::new(r"(?i)^Handling\s+(\S+)").unwrap(),
        Regex::new(r"(?i)^Processing\s+(\S+)").unwrap(),
        Regex::new(r"(?i)^Calling\s+(\S+)").unwrap(),
        Regex::new(r"^-->\s+(\S+)").unwrap(),
        // Kestrel: Request starting HTTP/1.1 GET http://localhost:5000/carts/...
        Regex::new(r"(?i)^Request\s+starting\s+\S+\s+((?:GET|POST|PUT|PATCH|DELETE|HEAD)\s+\S+)").unwrap(),
        // HttpClient (outgoing): Start processing HTTP request GET https://... (only this one as Entry to avoid duplicate span with "Sending")
        Regex::new(r"(?i)^Start\s+processing\s+HTTP\s+request\s+((?:GET|POST|PUT|PATCH|DELETE|HEAD)\s+\S+)").unwrap(),
    ]
});

// ── EXIT patterns ───────────────────────────────────────────────────────────

static EXIT_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // ASP.NET Core: Executed endpoint 'Namespace.Controller.Action (Api)'
        Regex::new(r"(?i)^Executed\s+endpoint\s+'([^']+)'").unwrap(),
        // ASP.NET Core: Executed action Api.Controllers.Carts.CartController.GetById (Api) in 375.6283ms
        Regex::new(r"(?i)^Executed\s+action\s+([^\s]+(?:\s+\([^)]+\))?)\s+in\s+").unwrap(),
        // ASP.NET Core: Executed action method ... returned result ... in 334ms
        Regex::new(r"(?i)^Executed\s+action\s+method\s+([^\s,]+(?:\s+\([^)]+\))?)").unwrap(),
        // Generic
        Regex::new(r"(?i)^(\S+)\s+completed").unwrap(),
        Regex::new(r"(?i)^(\S+)\s+finished").unwrap(),
        Regex::new(r"(?i)^Exiting\s+(\S+)").unwrap(),
        Regex::new(r"^<--\s+(\S+)").unwrap(),
        Regex::new(r"(?i)^Completed\s+(\S+)").unwrap(),
        // Kestrel: Request finished HTTP/1.1 GET ... - 200 ... 458.9821ms
        Regex::new(r"(?i)^Request\s+finished\s+\S+\s+((?:GET|POST|PUT|PATCH|DELETE|HEAD)\s+\S+)").unwrap(),
        // HttpClient (outgoing): End processing HTTP request after 223.2738ms - 200
        // (Only this one; "Received HTTP response headers" is NOT used as Exit to avoid two Exits per call closing the parent span)
        Regex::new(r"(?i)^End\s+processing\s+(HTTP\s+request)\s+after\s+[\d.]+\s*ms\s+-\s+\d+").unwrap(),
    ]
});

// ── ERROR patterns ──────────────────────────────────────────────────────────

static ERROR_PATTERN_EXCEPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:Exception|Error|PANIC|FATAL):\s*(.+)").unwrap());

static ERROR_PATTERN_THROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)throw\s+(?:new\s+)?(\w+)(?:\((.+)\))?").unwrap());

static ERROR_PATTERN_FAILED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)failed\s+to\s+.+?:\s*(.+)").unwrap());

static ERROR_PATTERN_NOT_FOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)No\s+\w+\s+found.+").unwrap());

/// Try to match the message against all ENTRY patterns.
/// Returns the captured function name on success.
pub fn match_entry(message: &str) -> Option<PatternMatch> {
    for re in ENTRY_PATTERNS.iter() {
        if let Some(caps) = re.captures(message) {
            return Some(PatternMatch {
                name: Some(caps[1].to_string()),
            });
        }
    }
    None
}

/// Try to match the message against all EXIT patterns.
/// Returns the captured function name on success.
pub fn match_exit(message: &str) -> Option<PatternMatch> {
    for re in EXIT_PATTERNS.iter() {
        if let Some(caps) = re.captures(message) {
            return Some(PatternMatch {
                name: Some(caps[1].to_string()),
            });
        }
    }
    None
}

/// Result of matching a message against error patterns.
pub struct ErrorPatternMatch {
    pub message: String,
    pub is_throw: bool,
    pub is_exception: bool,
    pub exception_name: Option<String>,
}

/// Try to match the message against known error patterns.
/// Returns extracted error information if a pattern matches.
pub fn match_error(message: &str) -> Option<ErrorPatternMatch> {
    // "throw new SomeError(details)" or "throw SomeError"
    if let Some(caps) = ERROR_PATTERN_THROW.captures(message) {
        let exception_name = Some(caps[1].to_string());
        let detail_msg = caps
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| message.to_string());
        return Some(ErrorPatternMatch {
            message: detail_msg,
            is_throw: true,
            is_exception: false,
            exception_name,
        });
    }

    // "SomeException: actual message" or "Error: actual message"
    if let Some(caps) = ERROR_PATTERN_EXCEPTION.captures(message) {
        let detail_msg = caps[1].trim().to_string();
        let is_exception = message.to_lowercase().contains("exception");
        return Some(ErrorPatternMatch {
            message: detail_msg,
            is_throw: false,
            is_exception,
            exception_name: None,
        });
    }

    // "failed to do something: reason"
    if let Some(caps) = ERROR_PATTERN_FAILED.captures(message) {
        return Some(ErrorPatternMatch {
            message: caps[1].trim().to_string(),
            is_throw: false,
            is_exception: false,
            exception_name: None,
        });
    }

    // "No <thing> found ..."
    if ERROR_PATTERN_NOT_FOUND.is_match(message) {
        return Some(ErrorPatternMatch {
            message: message.to_string(),
            is_throw: false,
            is_exception: false,
            exception_name: None,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ENTRY patterns ──────────────────────────────────────────────

    #[test]
    fn entry_executing() {
        let m = match_entry("Executing CreateOrderController").unwrap();
        assert_eq!(m.name.unwrap(), "CreateOrderController");
    }

    #[test]
    fn entry_executing_method() {
        let m = match_entry("Executing method PaymentService.GetProvider").unwrap();
        assert_eq!(m.name.unwrap(), "PaymentService.GetProvider");
    }

    #[test]
    fn entry_enter() {
        let m = match_entry("Enter GetUser").unwrap();
        assert_eq!(m.name.unwrap(), "GetUser");
    }

    #[test]
    fn entry_entering() {
        let m = match_entry("Entering ValidateInput").unwrap();
        assert_eq!(m.name.unwrap(), "ValidateInput");
    }

    #[test]
    fn entry_starting() {
        let m = match_entry("Starting OrderProcessing").unwrap();
        assert_eq!(m.name.unwrap(), "OrderProcessing");
    }

    #[test]
    fn entry_handling() {
        let m = match_entry("Handling CreateOrder").unwrap();
        assert_eq!(m.name.unwrap(), "CreateOrder");
    }

    #[test]
    fn entry_processing() {
        let m = match_entry("Processing PaymentRequest").unwrap();
        assert_eq!(m.name.unwrap(), "PaymentRequest");
    }

    #[test]
    fn entry_calling() {
        let m = match_entry("Calling ExternalAPI").unwrap();
        assert_eq!(m.name.unwrap(), "ExternalAPI");
    }

    #[test]
    fn entry_arrow() {
        let m = match_entry("--> HandleRequest").unwrap();
        assert_eq!(m.name.unwrap(), "HandleRequest");
    }

    #[test]
    fn entry_case_insensitive() {
        let m = match_entry("executing getUser").unwrap();
        assert_eq!(m.name.unwrap(), "getUser");
    }

    #[test]
    fn entry_no_match() {
        assert!(match_entry("Just a regular log line").is_none());
        assert!(match_entry("Completed successfully").is_none());
    }

    // ── EXIT patterns ───────────────────────────────────────────────

    #[test]
    fn exit_completed() {
        let m = match_exit("GetUser completed").unwrap();
        assert_eq!(m.name.unwrap(), "GetUser");
    }

    #[test]
    fn exit_finished() {
        let m = match_exit("OrderProcessing finished").unwrap();
        assert_eq!(m.name.unwrap(), "OrderProcessing");
    }

    #[test]
    fn exit_exiting() {
        let m = match_exit("Exiting CreateOrder").unwrap();
        assert_eq!(m.name.unwrap(), "CreateOrder");
    }

    #[test]
    fn exit_arrow() {
        let m = match_exit("<-- HandleRequest").unwrap();
        assert_eq!(m.name.unwrap(), "HandleRequest");
    }

    #[test]
    fn exit_completed_prefix() {
        let m = match_exit("Completed successfully").unwrap();
        assert_eq!(m.name.unwrap(), "successfully");
    }

    #[test]
    fn exit_no_match() {
        assert!(match_exit("Executing GetUser").is_none());
        assert!(match_exit("Some random log").is_none());
    }

    // ── ERROR patterns ──────────────────────────────────────────────

    #[test]
    fn error_exception_colon() {
        let m = match_error("NullPointerException: Cannot invoke method on null").unwrap();
        assert_eq!(m.message, "Cannot invoke method on null");
        assert!(m.is_exception);
        assert!(!m.is_throw);
    }

    #[test]
    fn error_error_colon() {
        let m = match_error("Error: Connection refused").unwrap();
        assert_eq!(m.message, "Connection refused");
        assert!(!m.is_exception);
    }

    #[test]
    fn error_panic() {
        let m = match_error("PANIC: thread main panicked at 'index out of bounds'").unwrap();
        assert_eq!(m.message, "thread main panicked at 'index out of bounds'");
    }

    #[test]
    fn error_throw_new() {
        let m = match_error("throw new ValidationError(invalid input)").unwrap();
        assert!(m.is_throw);
        assert_eq!(m.exception_name.unwrap(), "ValidationError");
        assert_eq!(m.message, "invalid input");
    }

    #[test]
    fn error_throw_simple() {
        let m = match_error("throw RuntimeError").unwrap();
        assert!(m.is_throw);
        assert_eq!(m.exception_name.unwrap(), "RuntimeError");
    }

    #[test]
    fn error_failed_to() {
        let m = match_error("failed to connect to database: timeout after 30s").unwrap();
        assert_eq!(m.message, "timeout after 30s");
    }

    #[test]
    fn error_not_found() {
        let m = match_error("No provider found with name \"paypau\"").unwrap();
        assert_eq!(m.message, "No provider found with name \"paypau\"");
    }

    #[test]
    fn error_no_match() {
        assert!(match_error("Everything is fine").is_none());
        assert!(match_error("Executing GetUser").is_none());
    }
}
