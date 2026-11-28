//! Any input as a macro, read whole and line by line.
//!
//! A macro may come over ssh from another machine, so its parser must
//! never panic. A macro that parses writes canonical text that parses back
//! into the same macro, and that text is its own canonical text. A macro
//! that fails names a line the input has, in one line with no control
//! character. Reading the input line by line, and as a stream through
//! the parser's bounded reader, stops at the same line with the same
//! message. A stream has no bound on its size in bytes, so the three agree
//! for any input no larger than a file may be.
//!
//! References:
//! - ADR-1208 D7: the parser treats its input as untrusted, holds to the
//!   limits, and a fuzz target runs it.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_config_macro::bounds::FILE_MAX;
use vtium_config_macro::{Parser, parse, parse_bytes};

fuzz_target!(|data: &[u8]| {
    let whole = parse_bytes(data);
    match &whole {
        Ok(m) => {
            let text = m.to_string();
            assert_eq!(parse(&text).as_ref(), Ok(m), "{text:?}");
            assert_eq!(parse(&text).map(|m| m.to_string()).as_deref(), Ok(text.as_str()));
        }
        Err(e) => {
            let lines = data.split(|&b| b == b'\n').count();
            assert!((1..=lines).contains(&e.line()), "line {} of {lines}", e.line());
            assert!(!e.to_string().chars().any(char::is_control), "{e}");
        }
    }
    // Line by line, as `--macro -` reads standard input.
    let body = data.strip_suffix(b"\n").unwrap_or(data);
    let mut parser = Parser::new();
    let mut first = None;
    if !data.is_empty() {
        for line in body.split(|&b| b == b'\n') {
            if let Err(e) = parser.line(line) {
                first = Some(e);
                break;
            }
        }
    }
    // As a stream, through the reader that bounds each line.
    let mut parser = Parser::new();
    let mut reader = data;
    let mut steps = Vec::new();
    let mut streamed = None;
    while let Some(step) = parser.read(&mut reader) {
        match step {
            Ok(s) => steps.push(s),
            Err(e) => {
                streamed = Some(e);
                break;
            }
        }
    }
    if data.len() <= FILE_MAX {
        if let Ok(m) = &whole {
            assert_eq!(m.steps(), steps.as_slice());
        }
        assert_eq!(whole.err(), first.clone());
        assert_eq!(first, streamed);
    }
});
