pub mod file;
pub mod stdin;

use std::io;

/// Trait that abstracts log line sources (files, stdin, etc.).
///
/// Returns an iterator of `io::Result<String>` so callers can handle
/// I/O errors per-line without loading everything into memory.
pub trait LogInput {
    fn lines(self: Box<Self>) -> Box<dyn Iterator<Item = io::Result<String>>>;
}

/// Build the appropriate `LogInput` based on CLI arguments.
///
/// - If `files` is non-empty, reads from those files sequentially.
/// - Otherwise, reads from stdin.
pub fn build_input(files: Vec<std::path::PathBuf>) -> anyhow::Result<Box<dyn LogInput>> {
    if files.is_empty() {
        Ok(Box::new(stdin::StdinInput::new()))
    } else {
        Ok(Box::new(file::FileInput::new(files)?))
    }
}
