use std::fs::File;
use std::io::{self, BufRead, BufReader, Read as _};
use std::path::PathBuf;

use super::LogInput;

const BINARY_CHECK_BYTES: usize = 8192;

/// Reads log lines sequentially from one or more files.
#[derive(Debug)]
pub struct FileInput {
    paths: Vec<PathBuf>,
}

impl FileInput {
    pub fn new(paths: Vec<PathBuf>) -> anyhow::Result<Self> {
        for path in &paths {
            if !path.exists() {
                anyhow::bail!("file not found: {}", path.display());
            }
            if path.is_dir() {
                anyhow::bail!("expected a file, got a directory: {}", path.display());
            }
            if is_likely_binary(path)? {
                anyhow::bail!(
                    "file appears to be binary (not a text log): {}",
                    path.display()
                );
            }
        }
        Ok(Self { paths })
    }
}

/// Heuristic: read the first N bytes and check for null bytes or a high
/// ratio of non-text bytes. This mirrors what `file(1)` and `git` do.
fn is_likely_binary(path: &PathBuf) -> anyhow::Result<bool> {
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; BINARY_CHECK_BYTES];
    let n = file.read(&mut buf)?;
    if n == 0 {
        return Ok(false);
    }
    let buf = &buf[..n];

    let null_count = buf.iter().filter(|&&b| b == 0).count();
    if null_count > 0 {
        return Ok(true);
    }

    let non_text = buf
        .iter()
        .filter(|&&b| b < 0x07 || (b > 0x0D && b < 0x20 && b != 0x1B))
        .count();

    Ok(non_text as f64 / n as f64 > 0.30)
}

impl LogInput for FileInput {
    fn lines(self: Box<Self>) -> Box<dyn Iterator<Item = io::Result<String>>> {
        Box::new(FileLineIterator::new(self.paths))
    }
}

/// Iterator that chains lines from multiple files in order.
struct FileLineIterator {
    paths: Vec<PathBuf>,
    current_index: usize,
    current_reader: Option<io::Lines<BufReader<File>>>,
}

impl FileLineIterator {
    fn new(paths: Vec<PathBuf>) -> Self {
        let mut iter = Self {
            paths,
            current_index: 0,
            current_reader: None,
        };
        iter.advance_file();
        iter
    }

    fn advance_file(&mut self) {
        self.current_reader = None;
        while self.current_index < self.paths.len() {
            match File::open(&self.paths[self.current_index]) {
                Ok(file) => {
                    self.current_reader = Some(BufReader::new(file).lines());
                    return;
                }
                Err(_) => {
                    self.current_index += 1;
                }
            }
        }
    }
}

impl Iterator for FileLineIterator {
    type Item = io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut reader) = self.current_reader {
                match reader.next() {
                    Some(line) => return Some(line),
                    None => {
                        self.current_index += 1;
                        self.advance_file();
                    }
                }
            } else {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn temp_file_with(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn read_lines_from_single_file() {
        let f = temp_file_with("line1\nline2\nline3");
        let input = FileInput::new(vec![f.path().to_path_buf()]).unwrap();
        let lines: Vec<String> = Box::new(input)
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn read_lines_from_multiple_files() {
        let f1 = temp_file_with("alpha\nbeta");
        let f2 = temp_file_with("gamma\ndelta");
        let input = FileInput::new(vec![f1.path().to_path_buf(), f2.path().to_path_buf()]).unwrap();
        let lines: Vec<String> = Box::new(input)
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(lines, vec!["alpha", "beta", "gamma", "delta"]);
    }

    #[test]
    fn empty_file_returns_no_lines() {
        let f = temp_file_with("");
        let input = FileInput::new(vec![f.path().to_path_buf()]).unwrap();
        let lines: Vec<String> = Box::new(input)
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn nonexistent_file_returns_error() {
        let result = FileInput::new(vec![PathBuf::from("nonexistent_file.log")]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("file not found"));
    }

    #[test]
    fn directory_path_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = FileInput::new(vec![dir.path().to_path_buf()]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("directory"));
    }

    #[test]
    fn binary_file_returns_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&[0x00, 0x01, 0x02, 0xFF, 0xFE, 0x00, 0x89, 0x50])
            .unwrap();
        f.flush().unwrap();
        let result = FileInput::new(vec![f.path().to_path_buf()]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("binary"));
    }

    #[test]
    fn text_file_is_not_binary() {
        let f = temp_file_with("2026-03-12 10:10:01 [INFO] Hello world\n");
        let result = FileInput::new(vec![f.path().to_path_buf()]);
        assert!(result.is_ok());
    }

    #[test]
    fn empty_file_is_not_binary() {
        let f = temp_file_with("");
        let result = FileInput::new(vec![f.path().to_path_buf()]);
        assert!(result.is_ok());
    }
}
