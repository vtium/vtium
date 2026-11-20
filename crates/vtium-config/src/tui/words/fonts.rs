//! What the Fonts tab says: the tab's name, what each font's line notes,
//! the removal of a frit, the new-frit form, the caption of the console's
//! own preview and the tab's key lines.

use super::*;

/// The Fonts tab's name in the tab bar.
pub const FONTS: &str = "Fonts";

/// The line above the preview in a vtium console, where the console draws
/// the preview's characters itself (ADR-1201 D4): in the Fonts view, the
/// font it draws them in, which `t` changes; in the Looks view, the size,
/// which the TUI cannot change on the console.
pub fn console_font_caption(font: Option<&str>) -> String {
    let font = match font {
        Some(f) => format!("{f}, the font it uses now"),
        None => "the font it uses now".to_owned(),
    };
    format!(
        "The console draws these characters in {font}. Press t to try the highlighted font on the console, k to keep it, and s to save it in the draft."
    )
}

/// A font's state, in the font list.
pub const READS: &str = "reads";

pub const BUILT_IN: &str = "built in";

pub fn does_not_read(why: impl Display) -> String {
    format!("cannot be read: {why}")
}

pub fn sources_missing(n: usize) -> String {
    if n == 1 {
        "1 font file missing".to_owned()
    } else {
        format!("{n} font files missing")
    }
}

pub const DRAWS_NOTHING: &str = "no font file on this machine";

pub const SOURCES_UNKNOWN: &str = "font files not checked";

/// Marks a frit an install removes.
pub const TO_REMOVE: &str = "to remove";

/// The fonts view's remove key, `r`.
pub fn remove_pending(name: impl Display) -> String {
    format!("{name} will be removed when you install. Press r again to keep it.")
}

pub fn remove_dropped(name: impl Display) -> String {
    format!("{name} will be kept.")
}

pub fn remove_refused(name: impl Display, why: impl Display) -> String {
    format!("{name} cannot be removed: {why}")
}

pub fn remove_replaced(name: impl Display) -> String {
    format!("this install replaces {name} with a new font set")
}

/// The new-frit view: its fields, and what it says.
pub const NEW_FIELD_NAMES: [&str; 6] = [
    "Name",
    "Font 1",
    "Font 2",
    "Font 3",
    "Character cell",
    "Size",
];

pub const NEW_ADD: &str = "Add this font set to the install";

pub const NEW_HINT: &str = "A font set (frit) combines up to three fonts. Name each font by its family, such as JetBrains Mono, or as a frit file names it, such as kbd:ter-v16n. Each later font supplies the characters the fonts before it lack. Character cell is WIDTHxHEIGHT in pixels and Size is in points; leave both empty to use the fonts' own size. If you leave Name empty, the name comes from the first font.";

pub const NEW_BAD_NAME: &str = "A name has 1 to 64 letters, digits, '.', '_', '+' or '-', and cannot be a built-in font's name.";

pub fn new_added(name: impl Display) -> String {
    format!("{name} is added to the install. Press Enter to install it.")
}

pub fn new_replaces_binary(name: impl Display) -> String {
    format!(
        "A new font set cannot replace {name}.vtfrit, because the installed file is not UTF-8 text. Remove it first with vtium config remove-frit {name}."
    )
}

pub fn new_refused(why: impl Display) -> String {
    format!("This font set cannot be read: {why}")
}

pub const KEYS_FONTS: &str =
    "Enter choose  t try live  s preview  n new  r remove  i install  q quit";

/// The Fonts view's keys in a vtium console, where `s` saves the font the
/// console shows (ADR-1201 D4).
pub const KEYS_FONTS_LIVE: &str =
    "Enter choose  t try live  s save  n new  r remove  i install  q quit";

pub const KEYS_NEW: &str =
    "type into a field  Enter next field  Esc back  Ctrl-C quit without installing";

/// What the message line says when `n` in the Fonts tab opens the form for
/// a new font set, which belongs to the Looks tab.
pub const NEW_FRIT_IN_LOOKS: &str = "The form for a new font set is part of the Looks tab, so Esc or adding the font set goes to the Looks tab.";
