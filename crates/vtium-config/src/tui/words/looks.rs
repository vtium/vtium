//! What the Looks tab says: the tab's name, its groups, a look's card and
//! its lists, a font set's detail, the density its preview uses, the
//! caption of the console's own preview and the tab's key line.

use super::*;

/// The Looks tab's name in the tab bar.
pub const LOOKS: &str = "Looks";

pub const CONSOLE_SIZE_CAPTION: &str = "These characters are drawn at the console's current size. A size you choose takes effect when vtium starts on this console again.";

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

/// What a look's row and card say where the look leaves a key to
/// vtium.conf's top level.
pub const LOOK_FRIT_TOP: &str = "vtium.conf's font set";
pub const LOOK_SIZE_TOP: &str = "vtium.conf's size";

/// A look's row in the list: its font set, its size, and the heads that
/// draw it.
pub fn look_row(frit: &str, size: &str, heads: &[String]) -> String {
    match heads {
        [] => format!("{frit}, {size}, no terminal uses it"),
        [one] => format!("{frit}, {size}, used by terminal {one}"),
        _ => format!("{frit}, {size}, used by terminals {}", heads.join(", ")),
    }
}

/// The name `n` gives a new look, with a number after it when a look of
/// that name exists.
pub const NEW_LOOK_NAME: &str = "look";

/// What the message line says once `n` has made a new look.
pub fn look_added(name: impl Display) -> String {
    format!("Made the look {name}. Type its name, then press Enter.")
}

/// The question `r` asks before it deletes a look.
pub fn look_delete_question(name: &str, heads: &[String]) -> String {
    match heads {
        [] => format!("Delete the look {name}? y deletes it, and any other key keeps it."),
        [one] => format!(
            "Delete the look {name}? Terminal {one} uses it and goes back to the default look. y deletes it, and any other key keeps it."
        ),
        _ => format!(
            "Delete the look {name}? Terminals {} use it and go back to the default look. y deletes it, and any other key keeps it.",
            heads.join(", ")
        ),
    }
}

/// What the message line says when `t` has nothing to try.
pub const TRY_NO_FONT: &str =
    "This look names no font set, and vtium.conf names none either, so there is nothing to try.";

/// What the sentence under the list says of the highlighted item.
pub fn item_about(name: impl Display, detail: impl Display) -> String {
    format!("{name}: {detail}.")
}

/// The names of a look card's lines, in order.
pub const CARD_LINES: [&str; 6] = [
    "Name",
    "Font set",
    "Size",
    "Color scheme",
    "Vtlook",
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

/// What a card's line shows for a key that neither the look nor vtium.conf
/// sets, which vtium chooses itself.
pub const TOP_UNSET: &str = "vtium's own choice";

pub const TOP_FRIT: &str = "font set";
pub const TOP_SIZE: &str = "size";
pub const TOP_PALETTE: &str = "color scheme";

/// The size line's note: the card offers vtium-config's usual sizes.
pub const SIZES_USUAL: &str = "vtium-config offers its usual sizes, because it cannot yet ask the font helper which sizes these fonts supply.";

/// The marks of a color scheme in the card's list.
pub const PALETTE_BUILT_IN: &str = "built in";

/// The mark of a vtlook that does not read, with why, in the card's list
/// and on its line.
pub fn vtlook_broken(why: impl Display) -> String {
    format!("does not read, so the terminal draws No filter until it is fixed: {why}")
}

/// The pointer theme line: every look draws the pointer theme vtium.conf
/// sets, which a look cannot change yet.
pub fn pointer_line(theme: Option<&str>) -> String {
    let theme = theme.unwrap_or("vtium's own pictures");
    format!("{theme}, from vtium.conf. A look cannot pick a pointer theme of its own yet.")
}

/// The sentence under a look's card.
pub const CARD_HINT: &str = "Enter on a line shows its choices, and Left and Right step through them. Each change waits for Save. Esc goes back to the list and keeps the changes.";

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
pub const ROLE_BOX: &str = "Box drawing";
pub const ROLE_OTHER: &str = "Other fonts, in order";
pub const ROLE_LAYERS: &str = "Layers for scripts, symbols, emoji and fallback";
pub const ROLE_BOX_OWN: &str = "vtium's own lines and blocks";
pub const ROLE_BOX_FONT: &str = "a font, as the font set's glyph-source line chooses";
pub const ROLE_NONE: &str = "no font";

/// What fills the text role of a built-in font.
pub fn built_in_font(name: impl Display) -> String {
    format!("the built-in font {name}")
}

/// A font that the font helper found missing on this machine.
pub fn font_missing(spelling: impl Display) -> String {
    format!("{spelling} (missing)")
}

/// The roles line of a font set whose file vtium-config could not read.
pub const ROLES_UNREAD: &str = "vtium-config could not read the font set's file.";

pub const MISSES_NOTHING: &str = "Nothing: this machine has every font it names.";
pub const MISSES_NOT_ASKED: &str = "Not known yet: the font helper has not checked its fonts.";

/// A font the font set names that this machine lacks or refuses, and why.
pub fn misses(spelling: impl Display, why: impl Display) -> String {
    format!("{spelling}: {why}")
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

/// The keys, on the last line of each view.
pub const KEYS_LOOKS: &str = "Enter choose  Tab next view  i install  q quit without installing";

/// What a card says once its look has left the draft.
pub fn look_gone(name: impl Display) -> String {
    format!("The draft no longer has the look {name}. Esc goes back to the list.")
}
