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

/// The scheme file that vtium reads for the color scheme `name`: the
/// administrator's file where it hides the package's file of that name.
pub fn file_named<'a>(shared: &'a Shared, name: &str) -> Option<&'a SchemeFile> {
    shared
        .inputs
        .effects
        .palettes
        .iter()
        .find(|f| f.name == name && !f.resource.hidden)
}

/// Whether the console reads the scheme file `f` now: the installed batch
/// lists it, and it passes the trust rule. A file that fails either check
/// counts as missing, and the console draws `legacy` in its place.
///
/// References:
/// - ADR-1205 D4: a palette file is a resource under the trust rule, and
///   the console uses it only while the batch holds it.
pub fn console_reads(f: &SchemeFile) -> bool {
    matches!(
        f.resource.state,
        State::Always | State::Active | State::Deactivates
    ) && f.resource.trust.is_ok()
        && !f.resource.own
}

/// What `t` on the look `look` says when its color scheme `scheme` is a
/// file the console does not read now, so the trial would show colors the
/// console never uses: it offers to activate a file the batch does not
/// list, says that a file the draft activates waits for an install and a
/// new login, and says why the console cannot use a file that fails the
/// trust rule. `None` when the console reads the scheme, or when it is not
/// a file. vtium reads the batch again at each new login, so an
/// activation reaches the console only then.
///
/// References:
/// - ADR-1205 D4: a palette file draws only while the batch holds it, and
///   activating it is an act of trust; its note of 2026-11-19: a change to
///   the batch takes effect at the next login.
pub fn refuse_trial(
    shared: &Shared,
    look: &str,
    scheme: &str,
) -> Option<Vec<super::super::Command>> {
    use super::super::super::desk::Op;
    use super::super::{Change, Command, Request};
    use crate::batch::Change as BatchChange;
    let f = file_named(shared, scheme)?;
    if PALETTE_NAMES.contains(&scheme) || console_reads(f) {
        return None;
    }
    let say = |line: String| Some(vec![Command::Change(Change::Say(line))]);
    if let Err(why) = &f.resource.trust {
        return say(words::scheme_unusable(scheme, look, why));
    }
    if f.resource.own {
        return say(words::scheme_unusable(scheme, look, words::SCHEME_OWN));
    }
    if f.resource.state == State::Activates {
        return say(words::scheme_drafted(scheme, look));
    }
    Some(vec![Command::Request(Request::Ask {
        question: words::scheme_inactive_question(scheme, look, f.resource.path.display()),
        then: super::super::super::model::Then::Desks(vec![Op::Resource {
            path: f.resource.path.clone(),
            change: BatchChange::Activate,
        }]),
    })])
}

/// How this terminal shows the preview's colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// In the scheme's own 24-bit colors.
    Truecolor,
    /// In the terminal's eight basic colors, the nearest by number.
    Basic,
    /// Not at all, because colors are off in this terminal, as with
    /// `NO_COLOR`, so a block of color would show blank.
    Off,
}

/// How this terminal shows the preview's colors.
pub fn shown(shared: &Shared) -> Shown {
    if !shared.inputs.color {
        Shown::Off
    } else if shared.inputs.flow_colors.truecolor {
        Shown::Truecolor
    } else {
        Shown::Basic
    }
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
            out.push(words::detail_file(Some(&crate::onscreen::path(&r.path))));
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
            values(palette, shown(shared))
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
/// whose file does not read, nor while colors are off in this terminal.
pub fn preview(shared: &Shared, scheme: Scheme) -> Vec<Row> {
    match (named(shared, scheme), shown(shared)) {
        (_, Shown::Off) => Vec::new(),
        (Some((_, Ok(palette))), shown) => swatches(&palette, shown == Shown::Truecolor),
        _ => Vec::new(),
    }
}

/// The sentence that says how the preview of `scheme` shows its colors,
/// when it cannot show the scheme's own: nothing for a scheme whose file
/// does not read, which has no preview.
pub fn preview_note(shared: &Shared, scheme: Scheme) -> Option<&'static str> {
    if !matches!(named(shared, scheme), Some((_, Ok(_)))) {
        return None;
    }
    match shown(shared) {
        Shown::Truecolor => None,
        Shown::Basic => Some(words::PALETTE_NO_TRUECOLOR),
        Shown::Off => Some(words::PALETTE_NO_COLOR),
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
/// While colors are off in this terminal, the values stand alone.
fn values(palette: &Palette, shown: Shown) -> Vec<Row> {
    let truecolor = shown == Shown::Truecolor;
    let mut out = Vec::new();
    for first in (0u8..16).step_by(4) {
        let mut row: Row = vec![span(" ", Style::PLAIN)];
        for i in first..first.saturating_add(4) {
            let c = palette.color(i);
            row.push(span(format!(" {i:>2} "), Style::PLAIN));
            if shown != Shown::Off {
                row.push(span("  ", filled(c, c, usize::from(i), truecolor)));
            }
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
    if shown != Shown::Off {
        out.push(Vec::new());
        out.extend(swatches(palette, truecolor));
    }
    out
}

/// `c` as `#rrggbb`.
fn hex(c: vtium_term::Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}
