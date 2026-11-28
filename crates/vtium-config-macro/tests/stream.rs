//! A macro read as a stream, as `--macro -` reads standard input: the step
//! count and the line length bound it, in place of a file's 1 MiB, and the
//! parser keeps one line of it at a time.
//!
//! References:
//! - ADR-1208 D1: the bounds, and the stream bound by steps and lines.

use std::io::{self, BufReader, Read};

use vtium_config_macro::bounds::{FILE_MAX, LINE_MAX, STEPS_MAX};
use vtium_config_macro::{Parser, Step, parse_bytes};

/// Every step of a stream, up to and including its first error.
fn read_all(data: &[u8]) -> Vec<Result<Step, String>> {
    let mut p = Parser::new();
    let mut r = data;
    let mut out = Vec::new();
    while let Some(s) = p.read(&mut r) {
        out.push(s.map_err(|e| e.to_string()));
    }
    out
}

#[test]
fn a_stream_is_not_bound_by_the_size_of_a_file() {
    // 300 lines of 4,000 bytes pass a file's 1 MiB.
    let row = format!("#{}\n", "a".repeat(3999));
    let text = row.repeat(300);
    assert!(text.len() > FILE_MAX);
    assert!(parse_bytes(text.as_bytes()).is_err());
    let steps = read_all(text.as_bytes());
    assert_eq!(steps.len(), 300);
    assert!(steps.iter().all(Result::is_ok));
    // Line by line too.
    let mut p = Parser::new();
    for _ in 0..300 {
        p.line(row.trim_end().as_bytes()).unwrap();
    }
}

#[test]
fn a_stream_holds_at_most_steps_max_lines() {
    let text = "Enter\n".repeat(STEPS_MAX + 5);
    let steps = read_all(text.as_bytes());
    assert_eq!(steps.len(), STEPS_MAX + 1);
    assert_eq!(
        steps.last().unwrap().as_ref().unwrap_err(),
        "line 10001: The macro has more than 10000 lines, the most a macro holds."
    );
}

/// A reader of `left` bytes of `a`, counting what it hands out.
struct Endless {
    left: usize,
    given: usize,
}

impl Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.left);
        buf[..n].fill(b'a');
        self.left -= n;
        self.given += n;
        Ok(n)
    }
}

#[test]
fn a_stream_line_longer_than_line_max_stops_without_reading_on() {
    let mut src = Endless {
        left: 64 * FILE_MAX,
        given: 0,
    };
    let mut p = Parser::new();
    let e = {
        let mut r = BufReader::with_capacity(16, &mut src);
        p.read(&mut r).unwrap().unwrap_err()
    };
    assert_eq!(
        e.to_string(),
        "line 1: This line is longer than 4096 bytes, the most a line holds."
    );
    assert!(src.given <= LINE_MAX + 16, "read {} bytes", src.given);
    // After an error the stream ends.
    assert!(p.read(&mut &b"Enter\n"[..]).is_none());
}

#[test]
fn a_stream_reads_the_same_steps_as_the_file() {
    let text = "\u{feff}# first\r\nEnter\n\nType `it's`\nDown 3";
    let whole = parse_bytes(text.as_bytes()).unwrap();
    let steps: Vec<Step> = read_all(text.as_bytes())
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(steps, whole.steps());
    // A line of exactly LINE_MAX bytes and a CRLF ending reads.
    let long = format!("#{}\r\n", "a".repeat(LINE_MAX - 1));
    assert_eq!(read_all(long.as_bytes()).len(), 1);
    assert!(read_all(long.as_bytes())[0].is_ok());
    // Nothing at all is no step.
    assert!(read_all(b"").is_empty());
}
