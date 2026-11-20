//! The Looks tab's group for the color schemes. A color scheme sets the
//! terminal's 16 colors and its default foreground and background. The
//! group lists the nine schemes built into vtium first, which are always
//! active and need no file, then the scheme files of the package's folder
//! and of the administrator's folder. Each file is a resource, so the
//! group shows its active mark and trust mark and takes Space on it with
//! the controls the resource tabs share. When the administrator's folder
//! holds a file of a packaged file's name, vtium reads the
//! administrator's file, so the packaged one is marked as hidden.
//!
//! The TUI reads the files when it starts, outside the state machine,
//! with [`super::super::super::effects_found::find_palettes`]. It reads
//! each file's colors for the preview, whether or not the file is active,
//! through the gate and the strict reader that vtium itself uses, so the
//! preview and the detail show the error or the refusal vtium would log.
//! A color scheme's preview is its
//! 16 colors beside text in its foreground on its background
//! ([`preview`]).
//!
//! References:
//! - ADR-1205 D1: the Looks tab lists the color schemes.
//! - ADR-1205 D4: a scheme file is a resource under the trust rule, and
//!   vtium uses it only while the batch lists it.
//! - ADR-1205 D10: the built-in schemes, the two folders, the
//!   administrator's file winning on a name, and the preview.
//! - ADR-1403 D10, its note of 2026-11-19: Space does nothing on a
//!   built-in scheme, and `n`, `r`, `t` and `p` do nothing among the
//!   color schemes.

use std::path::{Path, PathBuf};

use vtium_term::{PALETTE_NAMES, Palette};

use super::super::super::canvas::{Row, span};
use super::super::super::color::{Back, Normal, Style};
use super::super::super::effects_found::SchemeFile;
use super::super::super::listbox::Item;
use super::super::super::model::Shared;
use super::super::super::words;
use crate::batch::{Places, State, Use};

/// A color scheme of the group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// A scheme built into vtium, by its name.
    BuiltIn(&'static str),
    /// A scheme file, by its place in [`super::super::super::effects_found::Effects::palettes`].
    File(usize),
}

/// The schemes of the group, in its order: the built-in ones, then the
/// files.
pub fn entries(shared: &Shared) -> Vec<Scheme> {
    PALETTE_NAMES
        .iter()
        .map(|n| Scheme::BuiltIn(n))
        .chain((0..shared.inputs.effects.palettes.len()).map(Scheme::File))
        .collect()
}

/// The scheme file at `at`.
pub fn file(shared: &Shared, at: usize) -> Option<&SchemeFile> {
    shared.inputs.effects.palettes.get(at)
}

/// Whether the terminal shows 24-bit color, so the preview draws the
/// scheme's own colors.
pub fn truecolor(shared: &Shared) -> bool {
    shared.inputs.flow_colors.truecolor
}

/// The scheme's name and colors.
fn named(shared: &Shared, scheme: Scheme) -> Option<(String, Result<Palette, String>)> {
    match scheme {
        Scheme::BuiltIn(name) => Palette::named(name).map(|p| (name.to_owned(), Ok(p))),
        Scheme::File(at) => file(shared, at).map(|f| (f.name.clone(), f.colors.clone())),
    }
}

/// `scheme` as an item of the list: its name, and its marks.
pub fn item(shared: &Shared, scheme: Scheme) -> Item {
    match scheme {
        Scheme::BuiltIn(name) => Item::new(name, words::PALETTE_ALWAYS),
        Scheme::File(at) => file(shared, at).map_or_else(Item::default, |f| {
            let mut marks = f.resource.row();
            if f.colors.is_err() {
                marks.push_str(", ");
                marks.push_str(words::PALETTE_BROKEN);
            }
            Item::new(f.name.clone(), marks)
        }),
    }
}

/// The looks of vtium.conf's text `conf` that use the scheme file at
/// `path`, by the rule the command line's warning follows, with the
/// folders the group read the files from.
pub fn uses(shared: &Shared, path: &Path) -> Vec<Use> {
    let conf = shared.conf_text().unwrap_or_default();
    let folders: Vec<&Path> = shared
        .inputs
        .effects
        .palette_folders
        .iter()
        .map(PathBuf::as_path)
        .collect();
    let none = |_: &str| Vec::new();
    crate::batch::uses(
        path,
        conf,
        &Places {
            frits: None,
            palettes: &folders,
            vtlook: &none,
        },
    )
}

/// The looks of the draft that draw the built-in scheme `name`, the top
/// level of vtium.conf first, as the command line's warning counts the
/// users of a file.
fn built_in_users(shared: &Shared, name: &str) -> Vec<String> {
    crate::batch::drawing_palette(shared.conf_text().unwrap_or_default(), name)
        .iter()
        .map(|look| words::look_name(look.as_deref()))
        .collect()
}

/// The detail of `scheme`: its title, its file or that it is built in,
/// its marks, the check it fails, the looks that use it, and its colors,
/// one paragraph each, then the preview's rows.
pub fn detail(shared: &Shared, scheme: Scheme) -> (Vec<String>, Vec<Row>) {
    let Some((name, colors)) = named(shared, scheme) else {
        return (Vec::new(), Vec::new());
    };
    let mut out = vec![words::palette_title(&name)];
    match scheme {
        Scheme::BuiltIn(_) => {
            out.push(words::detail_file(None));
            out.push(words::detail_state(words::active_mark(State::Always)));
            out.push(words::detail_users(&built_in_users(shared, &name)));
        }
        Scheme::File(at) => {
            let Some(f) = file(shared, at) else {
                return (Vec::new(), Vec::new());
            };
            let r = &f.resource;
            out.push(words::detail_file(Some(&r.path.display().to_string())));
            out.push(words::detail_state(words::active_mark(r.state)));
            out.push(words::detail_trust(
                r.kind,
                r.trust.as_ref().err().map(String::as_str),
            ));
            if r.hidden {
                out.push(words::palette_hidden(&name));
            }
            let users: Vec<String> = uses(shared, &r.path)
                .iter()
                .map(|u| words::look_name(u.look.as_deref()))
                .collect();
            out.push(words::detail_users(&users));
        }
    }
    let rows = match &colors {
        Ok(palette) => {
            out.push(words::PALETTE_COLORS.to_owned());
            values(palette, truecolor(shared))
        }
        Err(why) => {
            out.push(words::palette_unread(why));
            Vec::new()
        }
    };
    (out, rows)
}

/// The rows of the list's preview of `scheme`: its 16 colors and a line
/// of text in its foreground on its background; nothing for a scheme
/// whose file does not read.
pub fn preview(shared: &Shared, scheme: Scheme) -> Vec<Row> {
    match named(shared, scheme) {
        Some((_, Ok(palette))) => swatches(&palette, truecolor(shared)),
        _ => Vec::new(),
    }
}

/// The style of a cell with its text in `fg` on `bg`, both 24-bit colors
/// where `truecolor` says the terminal shows them. The fill also names
/// one of the eight normal colors, the one of the color at `index`, which
/// a terminal without 24-bit color shows in its own shade.
fn filled(fg: vtium_term::Rgb, bg: vtium_term::Rgb, index: usize, truecolor: bool) -> Style {
    let basic = match index % 8 {
        0 => Normal::Black,
        1 => Normal::Red,
        2 => Normal::Green,
        3 => Normal::Yellow,
        4 => Normal::Blue,
        5 => Normal::Magenta,
        6 => Normal::Cyan,
        _ => Normal::White,
    };
    Style {
        rgb: truecolor.then_some([fg.r, fg.g, fg.b]),
        back: Some(Back {
            basic,
            rgb: truecolor.then_some([bg.r, bg.g, bg.b]),
        }),
        ..Style::PLAIN
    }
}

/// The 16 colors as two rows of eight blocks, then a line of sample text
/// in the scheme's foreground on its background, with words in its
/// colors.
fn swatches(palette: &Palette, truecolor: bool) -> Vec<Row> {
    let fg = palette.fg();
    let bg = palette.bg();
    let mut out = Vec::new();
    for (first, label) in [(0u8, words::PALETTE_NORMAL), (8, words::PALETTE_BRIGHT)] {
        let mut row: Row = vec![span(format!(" {label:<7}"), Style::PLAIN)];
        for i in first..first.saturating_add(8) {
            let c = palette.color(i);
            row.push(span("    ", filled(c, c, usize::from(i), truecolor)));
            row.push(span(" ", Style::PLAIN));
        }
        out.push(row);
    }
    let mut sample: Row = vec![span(format!(" {:<7}", words::PALETTE_TEXT), Style::PLAIN)];
    let on = |text: &str, color: vtium_term::Rgb| span(text, filled(color, bg, 0, truecolor));
    sample.push(on(" ", fg));
    for (text, index) in words::PALETTE_SAMPLE {
        let color = index.map_or(fg, |i| palette.color(i));
        sample.push(on(text, color));
    }
    sample.push(on(" ", fg));
    out.push(sample);
    out
}

/// The scheme's colors by value, four to a row, each beside a block of
/// its color, then its foreground and background, then the preview.
fn values(palette: &Palette, truecolor: bool) -> Vec<Row> {
    let mut out = Vec::new();
    for first in (0u8..16).step_by(4) {
        let mut row: Row = vec![span(" ", Style::PLAIN)];
        for i in first..first.saturating_add(4) {
            let c = palette.color(i);
            row.push(span(format!(" {i:>2} "), Style::PLAIN));
            row.push(span("  ", filled(c, c, usize::from(i), truecolor)));
            row.push(span(format!(" {} ", hex(c)), Style::PLAIN));
        }
        out.push(row);
    }
    let (fg, bg) = (palette.fg(), palette.bg());
    out.push(vec![span(
        format!(
            " {} {}, {} {}",
            words::PALETTE_FOREGROUND,
            hex(fg),
            words::PALETTE_BACKGROUND,
            hex(bg)
        ),
        Style::PLAIN,
    )]);
    out.push(Vec::new());
    out.extend(swatches(palette, truecolor));
    out
}

/// `c` as `#rrggbb`.
fn hex(c: vtium_term::Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}
