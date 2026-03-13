use std::io::{self, BufRead};

use super::LogInput;

/// Reads log lines from standard input (for pipe usage).
pub struct StdinInput;

impl StdinInput {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdinInput {
    fn default() -> Self {
        Self::new()
    }
}

impl LogInput for StdinInput {
    fn lines(self: Box<Self>) -> Box<dyn Iterator<Item = io::Result<String>>> {
        let stdin = io::stdin();
        let reader = stdin.lock();
        let lines: Vec<io::Result<String>> = reader.lines().collect();
        Box::new(lines.into_iter())
    }
}
