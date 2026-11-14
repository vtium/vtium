//! `make tier1-play CASE=NAME`: a case played in the reviewer's terminal
//! one screen at a time, the live path of its picture.
//!
//! The player replays the case once through [`shoot::replay`], the
//! replay its picture comes from, with the same input and the same cuts.
//! At each block it shows the screen's place in the case, what the
//! terminal wrote back and the test lines the block printed, then writes
//! the live path's bytes for that block ([`vtium_conformance::live`]) and
//! waits for a key. The terminal it runs in draws each screen, its
//! pictures included, and the last screen is the case's picture. A
//! scenario plays through `conformance scenarios --play`'s player
//! ([`vtium_conformance::scenario::play`]).

use std::io::Write;

use vtium_conformance::live;
use vtium_conformance::runner::{Case, Corpus};
use vtium_conformance::scenario::play::Step;
use vtium_conformance::scenario::transcript;
use vtium_conformance::view::View;

use crate::shoot;

/// Plays `case` of `set`, named `label` in its heading, for a reviewer:
/// writes each screen to `out` and calls `wait` after it. Returns whether
/// the reviewer played it to its end.
pub fn case(
    set: &Corpus,
    case: &Case,
    label: &str,
    out: &mut dyn Write,
    wait: &mut dyn FnMut() -> Step,
) -> Result<bool, String> {
    let input = set.input(case, View::Screen)?;
    let (printed, screens) = shoot::replay(&input, None);
    let printed = printed?;
    let blocks = transcript::blocks(&printed);
    let total = input.cuts.len().saturating_add(1);
    let write = |out: &mut dyn Write, bytes: &[u8]| -> Result<(), String> {
        out.write_all(bytes).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())
    };
    write(
        out,
        format!(
            "\x1b[1mcase {}\x1b[0m  {label}, {} by {}, {} screens\n",
            case.name, case.cols, case.rows, total
        )
        .as_bytes(),
    )?;
    for (k, screen) in &screens {
        let at = input.cuts.get(*k).copied().unwrap_or(input.data.len());
        let mut text = format!(
            "\n\x1b[1mscreen {} of {total}\x1b[0m, after byte {at} of {}\n",
            k + 1,
            input.data.len()
        );
        if let Some(b) = blocks.get(*k) {
            for line in b.effects.iter().chain(&b.tests) {
                text.push_str(&format!("\x1b[36m< {line}\x1b[0m\n"));
            }
            for line in b
                .body
                .iter()
                .filter(|l| l.starts_with("cursor ") || l.starts_with("flags "))
            {
                text.push_str(&format!("{line}\n"));
            }
        }
        text.push_str("\x1b[2m(any key: next screen, q: stop)\x1b[0m");
        text.push_str(&format!("\r\n{}\r\n", "─".repeat(screen.cols)));
        let mut bytes = text.into_bytes();
        bytes.extend_from_slice(&screen.live);
        write(out, &bytes)?;
        let step = wait();
        let mut after = live::below(screen.rows);
        after.extend_from_slice(format!("{}\n", "─".repeat(screen.cols)).as_bytes());
        write(out, &after)?;
        if step == Step::Quit {
            return Ok(false);
        }
    }
    write(
        out,
        format!("\nThe case {} ends here.\n", case.name).as_bytes(),
    )?;
    Ok(true)
}
