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

/// The size line's note: the card offers vtium-config's usual sizes.
pub const SIZES_USUAL: &str = "vtium-config offers its usual sizes, because it cannot yet ask the font helper which sizes these fonts supply.";

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
