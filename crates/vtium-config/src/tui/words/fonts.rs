//! What the Fonts tab says: the tab's name, its filters and groups, each
//! font's marks, a font's detail, the sentences under the list, the
//! question before a font is installed as a system font, and the sizes a
//! look's card offers from its text font. The words of the font sets,
//! which the Looks tab lists, live here too: what each font set's line
//! notes, the removal of a font set and the new-frit form.

use super::*;
use crate::describe::Strike;
use crate::discover::Place;
use crate::tui::fonts_found::{Filter, Supplied, Unknown};

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

pub const KEYS_NEW: &str =
    "type into a field  Enter next field  Esc back  Ctrl-C quit without installing";

/// The filter line's choice that shows every font.
pub const FONTS_ALL: &str = "All fonts";

/// The filter line, which names the filter the list shows.
pub fn fonts_filter(shown: &str) -> String {
    format!("Show: {shown}")
}

/// The name of a filter, as the filter line and its list show it.
pub fn filter_name(filter: Filter) -> String {
    match filter {
        Filter::All => FONTS_ALL.to_owned(),
        Filter::Bitmap => "Bitmap fonts".to_owned(),
        Filter::Outline => "Outline fonts".to_owned(),
        Filter::Monospaced => "Monospaced fonts".to_owned(),
        Filter::Proportional => "Proportional fonts".to_owned(),
        Filter::Group(g) => format!("{} fonts", g.name()),
        Filter::Script(s) => format!("Fonts for {}", s.replace('_', " ")),
    }
}

/// The heading of each kind of folder's group in the list.
pub fn place_heading(place: Place) -> &'static str {
    match place {
        Place::System => "System fonts",
        Place::Vtium => "vtium's own fonts",
        Place::Console => "Console fonts (kbd)",
        Place::FontPaths => "Folders that font-paths names",
        Place::User => {
            "Your own fonts, which vtium cannot use until you install them as system fonts"
        }
    }
}

/// A font's mark while the font helper has not described it.
pub const DESCRIBING: &str = "describing…";

/// A font's mark when the font helper cannot read it as a font.
pub const NOT_A_FONT: &str = "not a font vtium reads";

/// A font's mark when no description came.
pub const NOT_DESCRIBED: &str = "not described";

/// The mark of a font that a package also installed.
pub const DUPLICATE: &str = "duplicate";

/// A face as a list names it: its family and its style.
pub fn face_name(family: &str, style: &str) -> String {
    if style.is_empty() {
        family.to_owned()
    } else {
        format!("{family} {style}")
    }
}

/// The sentence under the list while the font helper describes fonts.
pub fn fonts_waiting(n: usize) -> String {
    if n == 1 {
        "The font helper is still describing 1 font, and only All fonts shows it until then."
            .to_owned()
    } else {
        format!(
            "The font helper is still describing {n} fonts, and only All fonts shows them until then."
        )
    }
}

/// The sentence under the list when the filter keeps no font.
pub const FONTS_NONE: &str = "No font passes this filter.";

/// The sentence under the list when the scan found no font file.
pub const FONTS_EMPTY: &str = "vtium-config found no font files in the folders it looks in.";

/// The sentence under the list that says what the keys do.
pub const FONTS_HINT: &str = "Space activates or deactivates the highlighted font. Enter opens a font's detail, or the filters on the filter line. p switches the preview.";

/// The sentence under the list of filters.
pub const FONTS_FILTER_HINT: &str = "Enter shows the fonts that pass the highlighted filter. A filter other than All fonts leaves out each font the font helper has not described.";

/// The sentence under a font's detail.
pub const FONTS_DETAIL_HINT: &str = "Up and Down scroll the detail, and Esc goes back to the list.";

/// The sentence under the detail of a font in the person's own folder.
pub const FONTS_OWN_HINT: &str = "Press Enter to install this font as a system font. vtium-config shows the command and asks before it runs it. Esc goes back to the list.";

/// What Space says on a font in the person's own folder.
pub fn own_font(path: &str) -> String {
    format!(
        "This font is in your own folder, so vtium cannot use it. Press Enter to open its detail, then Enter again to install it as a system font. {path}"
    )
}

/// What `t` says in the Fonts tab.
pub const TRY_A_FONT_SET: &str = "vtium can try a font set on this console, but not a single font file. Make a font set from this font in the Looks tab, then press t there.";

/// The lines of a font's detail.
pub fn font_title(name: &str) -> String {
    format!("font {name}")
}

pub fn font_folder(place: Place) -> String {
    format!("Folder: {}", place.words())
}

pub fn font_face(name: &str, version: Option<&str>) -> String {
    match version {
        Some(v) => format!("Face: {name}, version {v}"),
        None => format!("Face: {name}"),
    }
}

pub fn font_kind(format: &str, outline: bool, monospaced: bool) -> String {
    let drawn = if outline { "outline" } else { "bitmap" };
    let spacing = if monospaced {
        "monospaced"
    } else {
        "proportional"
    };
    format!("  {format}, {drawn}, {spacing}")
}

pub fn font_covers(codepoints: u32, scripts: &[&str]) -> String {
    let scripts: Vec<String> = scripts.iter().map(|s| s.replace('_', " ")).collect();
    if scripts.is_empty() {
        format!("  Covers {codepoints} characters, and no script in full.")
    } else {
        format!("  Covers {codepoints} characters: {}.", scripts.join(", "))
    }
}

pub fn font_groups(groups: &[&str]) -> String {
    format!("  Belongs to: {}.", groups.join(", "))
}

pub fn font_strikes(strikes: &[String]) -> String {
    if strikes.is_empty() {
        "  Sizes: any size, since it has outlines.".to_owned()
    } else {
        format!("  Sizes: {}.", strikes.join(", "))
    }
}

pub fn font_outline_strikes(strikes: &[String]) -> String {
    format!(
        "  Sizes: any size, since it has outlines, and bitmaps for {}.",
        strikes.join(", ")
    )
}

pub fn font_not_described(why: &str) -> String {
    format!("The font helper cannot describe this file: {why}")
}

pub const FONT_DESCRIBING: &str = "The font helper is still describing this file.";

pub fn font_duplicate(of: &str) -> String {
    format!("Duplicate: {of}. vtium uses the package's file while it is active.")
}

pub const FONT_OWN: &str = "This font is in your own folder, so vtium cannot use it. Install it as a system font, then activate it.";

/// The question before a font of the person's is installed as a system
/// font. The font's detail above it shows the exact command.
pub fn system_font_question(target: &str) -> String {
    format!(
        "Install this font as the system font {target}? The command shown above runs and asks for your password."
    )
}

/// The same question when another file of the same name is installed.
/// The font's detail above it shows both files and the command.
pub fn system_font_replace_question(target: &str) -> String {
    format!(
        "Replace the installed file {target} with yours? The command shown above runs and asks for your password."
    )
}

/// The lines a font's detail adds while the question before its install
/// as a system font is open: both files for a file that replaces another,
/// then the command.
pub const SYSTEM_FONT_CLASH: &str = "A different file of this name is already installed.";

pub fn system_font_installed_file(facts: &str) -> String {
    format!("Installed: {facts}")
}

pub fn system_font_yours(facts: &str) -> String {
    format!("Yours: {facts}")
}

pub fn system_font_command(source: &str, target: &str) -> String {
    format!("To install {source} as {target}, vtium-config runs this command:")
}

/// What the message line says when the same file is already installed.
pub fn system_font_identical(target: &str) -> String {
    format!(
        "This font is already installed as {target}, and the two files are the same. Nothing changed."
    )
}

/// What the message line says after the install, with what the
/// privileged step printed.
pub fn system_font_installed(target: &str) -> String {
    format!(
        "{target} is installed as a system font. Installing does not activate it: press Space on it in the list to activate it."
    )
}

pub fn system_font_failed(why: &str) -> String {
    format!("The font was not installed: {why}")
}

pub const SYSTEM_FONT_NOT_YOURS: &str = "Only a font in your own folder is installed as a system font. This font is a system font already.";

/// What a look's card says beside its size, from its text font.
pub fn sizes_note(supplied: &Supplied) -> String {
    match supplied {
        Supplied::Any { family } => format!(
            "The text font, {family}, has outlines, so the look can take any size."
        ),
        Supplied::Strikes { family, strikes } => {
            let shown: Vec<String> = strikes.iter().map(strike_cell).collect();
            format!(
                "The text font, {family}, is a bitmap font, so the look offers its sizes: {}.",
                shown.join(", ")
            )
        }
        Supplied::Unknown(why) => match why {
            Unknown::NoTextFont => {
                "vtium-config offers its usual sizes, because the font set names no text font."
                    .to_owned()
            }
            Unknown::BuiltIn => {
                "vtium-config offers its usual sizes, because the text font is built into vtium or is the console's own font."
                    .to_owned()
            }
            Unknown::Describing => {
                "vtium-config offers its usual sizes until the font helper has described the text font."
                    .to_owned()
            }
            Unknown::NotFound(spelling) => format!(
                "vtium-config offers its usual sizes, because it found no font file for the text font {spelling}."
            ),
            Unknown::NotAFont(spelling) => format!(
                "vtium-config offers its usual sizes, because the font helper cannot read the text font {spelling}."
            ),
        },
    }
}

/// A bitmap strike as the card and a font's detail show it: its cell in
/// pixels.
pub fn strike_cell(strike: &Strike) -> String {
    format!("{}×{}", strike.width, strike.height)
}

/// A size of the card's list that draws a bitmap strike.
pub fn strike_size(points: u32, drawn: &Strike) -> String {
    format!("{} pt, the {} strike", points, strike_cell(drawn))
}

/// The Fonts tab's note when vtium-config cannot read the draft's batch.
pub fn fonts_unread(why: &str) -> String {
    format!(
        "vtium-config cannot read which resources the draft keeps active, so every font shows as not active. {why}"
    )
}

/// The Fonts tab's note when vtium-config cannot read the mount table.
pub fn fonts_unjudged(why: &str) -> String {
    format!(
        "vtium-config cannot read the mount table, so it cannot judge the fonts it finds and lists none. {why}"
    )
}

/// What the message line says when Enter in a font's detail finds that
/// the desk has readied no font to install.
pub const NO_FONT_READY: &str = "No font is ready to install as a system font.";

/// Why a font helper's answer is missing.
pub const HELPER_SILENT: &str = "the font helper did not answer";

/// Why a font of the person's is not installed: the font helper cannot
/// read the draft's copy as a font.
pub fn system_font_not_a_font(source: &str, why: &str) -> String {
    format!(
        "{source}: vtium-config cannot install this file as a system font, because the font helper cannot read it as a font: {why}"
    )
}
