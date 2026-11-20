//! What the Looks tab says: the tab's name, its groups, a look's card and
//! its lists, a font set's detail, the density its preview uses, the
//! caption of the console's own preview and the tab's key line.

use super::*;

/// The Looks tab's name in the tab bar.
pub const LOOKS: &str = "Looks";

/// The caption of the console's own preview, which the console draws in
/// the font and size it uses now.
pub const CONSOLE_SIZE_CAPTION: &str = "The console draws this in its current font, so t shows a font set here. A size takes effect when vtium starts again.";

/// The size that lets vtium choose.
pub const AUTOMATIC: &str = "automatic";

pub fn points(n: u32) -> String {
    format!("{n} pt")
}

pub fn density(dpi: u32, from: impl Display) -> String {
    format!("The preview uses {dpi} dpi, {from}.")
}

pub const AUTOMATIC_PICTURE: &str =
    "Automatic lets vtium pick the size for each display, so there is no preview.";

pub const DPI_FROM_CONF: &str = "set by display-dpi in vtium.conf";

pub const DPI_FROM_EDID: &str = "measured from the display";

pub const DPI_ASSUMED: &str = "assumed, because no display reports its size";

/// The headings of the tab's groups, in the list's order. The color
/// schemes and the pointer themes come after the font sets.
pub const LOOKS_GROUP: &str = "Looks";
pub const FONT_SETS_GROUP: &str = "Font sets";

/// The row of the looks group when the draft has no look.
pub const NO_LOOKS: &str = "No looks yet";
pub const NO_LOOKS_NOTE: &str = "press n to make one";

/// `items` as a list in a sentence: "a", "a and b", "a, b, and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, last] => format!("{first} and {last}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// What the sentence under the list says of the highlighted look: its
/// font set, its size, where `None` leaves each to vtium.conf, and the
/// terminals that draw it.
pub fn look_about(name: &str, frit: Option<&str>, size: Option<&str>, heads: &[String]) -> String {
    let frit = match frit {
        Some(f) => format!("the font set {f}"),
        None => "vtium.conf's font set".to_owned(),
    };
    let size = match size {
        None => "at vtium.conf's size".to_owned(),
        Some("0") => "at the size vtium picks for each display".to_owned(),
        Some(n) => format!("at {}", n.parse().map_or_else(|_| n.to_owned(), points)),
    };
    let heads = match heads {
        [] => "No terminal uses it.".to_owned(),
        [one] => format!("Terminal {one} uses it."),
        _ => format!("Terminals {} use it.", and_list(heads)),
    };
    format!("{name} uses {frit} {size}. {heads}")
}

/// The look that every terminal with no `look =` line draws.
pub const DEFAULT_LOOK: &str = "default";

/// The name `n` gives a new look, with a number after it when a look of
/// that name exists.
pub const NEW_LOOK_NAME: &str = "new-look";

/// What the message line says once `n` has made a new look.
pub fn look_added(name: impl Display) -> String {
    format!("Made the look {name}. Type its name, then press Enter.")
}

/// The question `r` asks before it deletes a look. The terminals that
/// draw the look `default` then use vtium.conf's top-level settings, and
/// those that draw any other look go back to the look `default`.
pub fn look_delete_question(name: &str, heads: &[String]) -> String {
    let (after_one, after_many) = if name == DEFAULT_LOOK {
        (
            "then uses vtium.conf's top-level settings",
            "then use vtium.conf's top-level settings",
        )
    } else {
        (
            "then goes back to the default look",
            "then go back to the default look",
        )
    };
    let users = match heads {
        [] => String::new(),
        [one] => format!(" Terminal {one} uses it, and {after_one}."),
        _ => format!(" Terminals {} use it, and {after_many}.", and_list(heads)),
    };
    format!("Delete the look {name}?{users} y deletes it, and any other key keeps it.")
}

/// What the message line says when `t` finds no font set to try: the
/// look names none, and vtium.conf names none either.
pub const TRY_NO_FONT: &str =
    "This look names no font set, and vtium.conf names none either, so t has nothing to try.";

/// What the message line says when `t` is pressed on the row that says
/// there are no looks yet.
pub const TRY_NO_LOOK: &str =
    "There is no look yet, so t has nothing to try. Press n to make a look.";

/// What the message line says when `t` cannot try a font set that the
/// console does not read, with why.
pub fn try_refused(name: impl Display, why: impl Display) -> String {
    format!("The console does not read the font set {name}, so t cannot try it: {why}")
}

/// What the message line says when `t` cannot try a font set that this
/// machine does not have.
pub fn try_absent(name: impl Display) -> String {
    format!("This machine has no font set named {name}, so t cannot try it.")
}

/// What the sentence under the list says of the highlighted item.
pub fn item_about(name: impl Display, detail: impl Display) -> String {
    format!("{name}: {detail}.")
}

/// The names of a look card's lines, in order. A vtlook's name is always
/// written in lowercase, as the Effects tab writes it.
pub const CARD_LINES: [&str; 6] = [
    "Name",
    "Font set",
    "Size",
    "Color scheme",
    "vtlook",
    "Pointer theme",
];

/// What a card's line shows where the look leaves the key to vtium.conf,
/// with the value vtium.conf gives it.
pub fn from_top(value: impl Display) -> String {
    format!("{value}, from vtium.conf")
}

/// The first choice of a card's list: the value of vtium.conf's top level,
/// which leaves the look's own key unset.
pub fn top_choice(what: &str, value: impl Display) -> String {
    format!("The {what} vtium.conf sets ({value})")
}

/// What vtium.conf sets for a key it leaves unset, in a card's first
/// choice.
pub const TOP_NONE: &str = "none";

/// What the font set line shows where neither the look nor vtium.conf
/// names a font set.
pub const FRIT_NONE: &str =
    "none, so vtium uses the font that vconsole.conf names, or else its built-in font";

/// The note of the font set list's first choice where vtium.conf names no
/// font set.
pub const FRIT_NONE_NOTE: &str =
    "vtium uses the font that vconsole.conf names, or else its built-in font";

/// The text role where neither the look nor vtium.conf names a font set.
pub const ROLE_TEXT_NONE: &str = "the font that vconsole.conf names, or else vtium's built-in font";

pub const TOP_FRIT: &str = "font set";
pub const TOP_SIZE: &str = "size";
pub const TOP_PALETTE: &str = "color scheme";
pub const TOP_VTLOOK: &str = "vtlook";

/// The marks of a color scheme in the card's list.
pub const PALETTE_BUILT_IN: &str = "built in";

/// The color scheme `custom` in the card's list, and its note.
pub const PALETTE_CUSTOM: &str = "custom";
pub const PALETTE_CUSTOM_NOTE: &str = "the colors that vtium.conf's palette-0 to palette-15 set";

/// The note of a value that the look sets and the line's usual choices
/// lack, such as a color scheme file or a size typed into vtium.conf.
pub const CHOICE_OWN: &str = "what this look sets now";

/// The note of a font set that the look names and this machine lacks.
pub const CHOICE_ABSENT: &str =
    "what this look sets now, and this machine has no font set of that name";

/// The note of a font set that the look names and the console does not
/// read, with why.
pub fn choice_refused(why: impl Display) -> String {
    format!("what this look sets now, and the console does not read it: {why}")
}

/// The mark of a vtlook that does not read, with why, in the card's list
/// and on its line.
pub fn vtlook_broken(why: impl Display) -> String {
    format!(
        "vtium cannot read this file, so the terminal draws No filter until the file is fixed: {why}"
    )
}

/// The note of an active color scheme file in a look card's list of
/// color schemes.
pub const PALETTE_FILE_NOTE: &str = "scheme file";

/// The pointer theme line: every look draws the pointer theme vtium.conf
/// sets, which a look cannot change yet.
pub fn pointer_line(theme: Option<&str>) -> String {
    let theme = theme.unwrap_or("vtium's own pictures");
    format!("{theme}, from vtium.conf. A look cannot pick a pointer theme of its own yet.")
}

/// The sentence under a look's card.
pub const CARD_HINT: &str = "Enter on a line shows its choices or lets you type the name, and Left and Right step through the choices. Esc keeps the changes for Save.";

/// The sentence under a card's list.
pub const CARD_LIST_HINT: &str =
    "Enter picks the highlighted choice, and Esc goes back to the card without a change.";

/// The sentence under a card while its name is typed.
pub const CARD_TYPING_HINT: &str =
    "Type the look's name, then press Enter. Esc keeps the name it had.";

/// The heading of what a look's font set fills by role and what it misses.
pub const ROLES_HEADING: &str = "Its fonts, by role:";
pub const MISSES_HEADING: &str = "What it still misses:";

/// A role's line: the role, then what fills it.
pub fn role_line(role: &str, what: &str) -> String {
    format!("{role}: {what}")
}

pub const ROLE_TEXT: &str = "Text";
pub const ROLE_BOLD: &str = "Text, bold";
pub const ROLE_ITALIC: &str = "Text, italic";
pub const ROLE_BOLD_ITALIC: &str = "Text, bold italic";
pub const ROLE_BOX: &str = "Box drawing";
pub const ROLE_OTHER: &str = "More fonts, in the order vtium tries them";
pub const ROLE_LAYERS: &str = "Scripts, symbols, emoji, and fallback";
pub const ROLE_LAYER_FONT: &str = "A font from a layer";
pub const ROLE_NONE: &str = "no font";

/// Box drawing where the font set chooses nothing: vtium's default.
pub const ROLE_BOX_DEFAULT: &str = "vtium's own lines, unless the text font has them all";
pub const ROLE_BOX_OWN: &str = "vtium's own lines";
pub const ROLE_BOX_FONT: &str = "the text font's own lines";
pub const ROLE_BOX_AUTO: &str = "the text font's own lines, and vtium's where the font lacks one";

/// The layers a font set adds, by name.
pub fn role_layers(names: &[String]) -> String {
    match names {
        [one] => format!("the fonts of the layer {one}"),
        _ => format!("the fonts of the layers {}", and_list(names)),
    }
}

/// A font a font set names, in the words a person uses for it.
pub fn font_named(origin: &crate::frit::Origin) -> String {
    use crate::frit::Origin;
    match origin {
        Origin::Font(family) => format!("the font family {family}"),
        Origin::Kbd(name) => format!("the console font {name}"),
        Origin::File(p) | Origin::Bdf(p) | Origin::Hex(p) | Origin::Outline(p) => {
            format!("the font file {}", p.display())
        }
        Origin::Vconsole => "the font that vconsole.conf names".to_owned(),
        Origin::Console(vt) => format!("the font of VT {vt}"),
        Origin::Builtin(name) => built_in_font(name),
    }
}

/// What fills the text role of a built-in font.
pub fn built_in_font(name: impl Display) -> String {
    format!("the built-in font {name}")
}

/// A font that the font helper found missing on this machine.
pub fn font_missing(font: impl Display) -> String {
    format!("{font}, not on this machine")
}

/// A font that this machine has and the font helper cannot use.
pub fn font_refused(font: impl Display) -> String {
    format!("{font}, which the font helper cannot use")
}

/// The roles line of a font set whose file vtium-config could not read.
pub const ROLES_UNREAD: &str = "vtium-config could not read the font set's file.";

pub const MISSES_NOTHING: &str = "Nothing: this machine has every font it names.";
pub const MISSES_NOT_ASKED: &str = "Not known yet: the font helper has not checked its fonts.";

/// A font the font set names for `role` that this machine lacks, and why.
pub fn misses(role: impl Display, font: impl Display, why: impl Display) -> String {
    format!("{role}: {font} is not on this machine: {why}.")
}

/// A font the font set names for `role` that this machine has and the
/// font helper cannot use, and why.
pub fn misses_refused(role: impl Display, font: impl Display, why: impl Display) -> String {
    format!("{role}: this machine has {font}, but the font helper cannot use it: {why}.")
}

pub fn misses_all(why: impl Display) -> String {
    format!("Every font it names: {why}")
}

pub fn misses_unknown(why: impl Display) -> String {
    format!("Not known: the font helper did not answer. {why}")
}

/// The first line of a font set's detail.
pub fn font_set_title(name: impl Display) -> String {
    format!("Font set {name}")
}

pub const READS_WHOLE: &str = "The console reads it without errors.";
pub const BUILT_IN_WHOLE: &str = "It is built into vtium, so it needs no file.";

pub fn does_not_read_whole(why: impl Display) -> String {
    format!("The console does not read it: {why}")
}

/// The sentence under a font set's detail.
pub const FONT_SET_HINT: &str = "Up and Down scroll the detail, and Esc goes back to the list.";

/// What a card says once its look has left the draft.
pub fn look_gone(name: impl Display) -> String {
    format!("The draft no longer has the look {name}. Esc goes back to the list.")
}

/// The heading of the Looks tab's group of color schemes.
pub const COLOR_SCHEMES_GROUP: &str = "Color schemes";

/// The heading of the Looks tab's group of pointer themes.
pub const POINTERS_GROUP: &str = "Pointer themes";

/// The marks of a color scheme built into vtium.
pub const PALETTE_ALWAYS: &str = "built in, always active";

/// The mark of a color scheme file that vtium cannot read.
pub const PALETTE_BROKEN: &str = "has an error";

/// The title of a color scheme's detail.
pub fn palette_title(name: impl Display) -> String {
    format!("Color scheme {name}")
}

/// The line of a hidden color scheme file's detail.
pub fn palette_hidden(name: impl Display) -> String {
    format!(
        "The administrator's folder holds a color scheme named {name} too, so vtium reads that file in place of this one."
    )
}

/// The line above a color scheme's colors in its detail.
pub const PALETTE_COLORS: &str = "The scheme's 16 colors, then its foreground and background:";

/// The line of a color scheme's detail, and the note of its choice on a
/// look's card, when vtium cannot use its file, with the first error, its
/// file and line first.
pub fn palette_unread(why: impl Display) -> String {
    format!(
        "vtium cannot use this file, so a look that names this color scheme renders with the one vtium.conf names: {why}"
    )
}

/// Why vtium cannot use a scheme file whose name a look cannot name: the
/// name is what comes before `.palette`, and a look names a scheme by it.
pub const PALETTE_BAD_NAME: &str = "The name before .palette is not one a look can name. A color scheme's name holds 1 to 64 of the letters A to Z and a to z, the digits 0 to 9, and the characters . _ + and -.";

/// The labels of the preview's rows: the normal colors 0 to 7, the
/// bright colors 8 to 15, and the sample text.
pub const PALETTE_NORMAL: &str = "0-7";
pub const PALETTE_BRIGHT: &str = "8-15";
pub const PALETTE_TEXT: &str = "Text";

/// The preview's sample text, each word with the color it is drawn in:
/// a number from the 16 colors, or `None` for the scheme's foreground.
pub const PALETTE_SAMPLE: [(&str, Option<u8>); 8] = [
    ("~/notes", Some(4)),
    (" $ ", None),
    ("ls", Some(2)),
    (" -l ", None),
    ("todo.txt", Some(3)),
    (" ", None),
    ("error", Some(1)),
    (" done", Some(10)),
];

/// The names of the scheme's foreground and background in its detail.
pub const PALETTE_FOREGROUND: &str = "foreground";
pub const PALETTE_BACKGROUND: &str = "background";

/// The sentence under a color scheme's preview in a terminal that does
/// not report 24-bit color, where the preview fills each block with the
/// nearest of the terminal's eight basic colors.
pub const PALETTE_NO_TRUECOLOR: &str = "This terminal does not report 24-bit color, so the preview shows the terminal's own eight colors in place of the scheme's. Enter opens the scheme's detail, which gives each color's value.";

/// The sentence in place of a color scheme's preview while colors are
/// off in this terminal, as with NO_COLOR, where every block would show
/// blank.
pub const PALETTE_NO_COLOR: &str = "Colors are off in this terminal, so the preview cannot show the scheme's colors. Enter opens the scheme's detail, which gives each color's value.";

/// The title of a pointer theme's detail.
pub fn pointer_title(name: impl Display) -> String {
    format!("Pointer theme {name}")
}

/// The line of a pointer theme's detail that names its folder.
pub fn pointer_folder(path: impl Display) -> String {
    format!("Folder: {path}. Every file in the folder is active while the folder is.")
}

/// The line of a pointer theme's detail that says whether the looks draw
/// it: every look draws the theme vtium.conf's `pointer-theme` names,
/// because a look cannot pick one of its own yet.
pub fn pointer_users(named: bool) -> String {
    if named {
        "Every look uses it, because vtium.conf's pointer-theme names it. A look cannot pick a pointer theme of its own yet.".to_owned()
    } else {
        "No look uses it. Every look uses the pointer theme that vtium.conf's pointer-theme names, because a look cannot pick one of its own yet.".to_owned()
    }
}

/// Space on a color scheme built into vtium.
pub const BUILT_IN_SCHEME_SPACE: &str =
    "A color scheme built into vtium is always active, so Space has nothing to change.";

/// Enter or `r` on the looks group's row while the draft has no look.
pub const NO_LOOK_YET: &str = "The draft has no look yet. Press n to make one.";

/// `p` on a font set vtium cannot read, whose preview stays empty.
pub const NO_PREVIEW_TO_SWITCH: &str =
    "vtium cannot read this font set, so it has no preview to switch.";

/// Enter on the pointer theme line of a look's card.
pub const LOOK_POINTER_NOT_YET: &str = "A look cannot pick a pointer theme of its own yet.";
