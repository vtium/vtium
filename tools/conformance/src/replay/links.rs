//! The links of the view (INTERFACE-vtium.md, "Links"): the `link`
//! command prints the URI of a cell's OSC 8 link, as Ctrl+click copies it,
//! and the `hover` command puts the pointer over a cell, with Ctrl held or
//! not, and prints the cells of the link under it and the overlay row that
//! shows the link's URI (ADR-1002).

use core::fmt::Write;

use vtium_term::{Age, Terminal};

use super::view_cell;
use crate::{Failure, usage};

/// Prints `test link HEX`, the URI of the link at the view's cell `at`,
/// or `test link none`.
pub(super) fn link(t: &Terminal, (col, row): (u64, u64), out: &mut String) -> Result<(), Failure> {
    let screen = t.screen();
    let (Some(x), Some(y)) = view_cell(screen, col, row) else {
        return Err(usage("test command link: col or row past the screen"));
    };
    out.push_str("test link ");
    match screen.link_at(x, y).and_then(|l| screen.link_uri(l)) {
        Some(uri) => push_hex(out, uri.as_bytes()),
        None => out.push_str("none"),
    }
    out.push('\n');
    Ok(())
}

/// Puts the pointer over the view's cell `at`, or takes it away, with
/// `ctrl` held or not, then prints the `test hover` and `test overlay`
/// lines.
pub(super) fn hover(
    t: &mut Terminal,
    at: Option<(u64, u64)>,
    ctrl: bool,
    out: &mut String,
) -> Result<(), Failure> {
    let at = match at {
        Some((col, row)) => match view_cell(t.screen(), col, row) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => return Err(usage("test command hover: col or row past the screen")),
        },
        None => None,
    };
    t.screen_mut().set_pointer(at, ctrl);
    hovered(t, out);
    Ok(())
}

/// Appends `bytes` as lowercase hex, two digits a byte.
fn push_hex(out: &mut String, bytes: &[u8]) {
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
}

/// Prints the `test hover` line, the cells of the view the hovered link
/// takes as runs `ROW:FIRST-LAST`, and the `test overlay` line, the row
/// that shows the link's URI while Ctrl is held and the URI (ADR-1002,
/// "Drawing").
fn hovered(t: &Terminal, out: &mut String) {
    let screen = t.screen();
    let frame = screen.frame(t.palette(), Age::NEVER);
    let mut runs: Vec<String> = Vec::new();
    for row in 0..frame.rows() {
        let mut first: Option<usize> = None;
        let hovered: Vec<bool> = frame.cells(row).map(|c| c.hovered).collect();
        for (col, &h) in hovered.iter().chain(&[false]).enumerate() {
            match (h, first) {
                (true, None) => first = Some(col),
                (false, Some(f)) => {
                    runs.push(format!("{row}:{f}-{}", col.saturating_sub(1)));
                    first = None;
                }
                _ => {}
            }
        }
    }
    out.push_str("test hover ");
    if runs.is_empty() {
        out.push_str("none");
    } else {
        out.push_str(&runs.join(","));
    }
    out.push('\n');
    out.push_str("test overlay ");
    match frame.overlay() {
        Some(o) => {
            let _ = write!(out, "{} ", o.row);
            push_hex(out, o.uri.as_bytes());
        }
        None => out.push_str("none"),
    }
    out.push('\n');
}
