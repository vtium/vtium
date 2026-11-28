//! What the Looks tab's forms and questions say on screen: the new look,
//! the new font set's form and its roles' lists, the removal of a font
//! set, and the questions before a look is deleted or something a look
//! uses is deactivated. Each string follows the rules for strings on
//! screen: the topic first, the reader's words, one line where a paragraph
//! was, and every key to press in brackets.
//!
//! References:
//! - crates/AGENTS.md, "Strings on screen", rules 1 to 8.

/// The name a new look takes before the person types one, and the base of
/// the next free name after it.
pub const NEW_LOOK_NAME: &str = "new-look";

/// The name of the change that makes a look.
pub fn look_made(name: &str) -> String {
    format!("make the look {name}")
}

/// What the message line says once `n` has made a look.
pub fn type_look_name(name: &str) -> String {
    format!("Made the look {name}. Type its name, then press [Enter].")
}

/// `r` on the row that says the configuration has no look.
pub const NO_LOOK_TO_DELETE: &str = "The configuration has no look to delete.";

/// The question before `r` deletes the look `name`, which names the
/// terminals that draw it. The terminals that draw the look `default`
/// then use vtium.conf's top-level settings, and the others the default
/// look.
pub fn delete_question(name: &str, terminals: &[String]) -> String {
    let then = if name == "default" {
        "vtium.conf's top-level settings"
    } else {
        "the default look"
    };
    match terminals {
        [] => format!("Delete the look {name}? No terminal uses it."),
        [one] => format!("Delete the look {name}? The terminal {one} then takes {then}."),
        many => format!(
            "Delete the look {name}? The terminals {} then take {then}.",
            many.join(", ")
        ),
    }
}

/// The name of the change that deletes a look.
pub fn look_deleted(name: &str) -> String {
    format!("delete the look {name}")
}

/// The question before Space deactivates the color scheme or pointer
/// theme `name`, which the looks `looks` use. A `None` among them is
/// vtium.conf's top level, which every look that names no other draws.
pub fn deactivate_question(name: &str, looks: &[Option<String>]) -> String {
    let named: Vec<&str> = looks.iter().filter_map(Option::as_deref).collect();
    if looks.iter().any(Option::is_none) {
        return format!(
            "Deactivate {name}? vtium.conf names it, and looks that name no other draw without it."
        );
    }
    match named.as_slice() {
        [one] => format!("Deactivate {name}? The look {one} draws without it once saved."),
        many => format!(
            "Deactivate {name}? The looks {} draw without it once saved.",
            many.join(", ")
        ),
    }
}

/// The names of `r`'s changes to a font set.
pub fn remove_name(name: &str) -> String {
    format!("remove the font set {name}")
}

pub fn keep_name(name: &str) -> String {
    format!("keep the font set {name}")
}

/// What the message line says after `r` on a font set.
pub fn removal_marked(name: &str) -> String {
    format!("Font set {name} is removed at the next save. [r] again takes the mark back.")
}

pub fn removal_taken_back(name: &str) -> String {
    format!("Font set {name} stays: the mark for removal is taken back.")
}

pub fn new_set_taken_back(name: &str) -> String {
    format!("The new font set {name} is taken back out of the configuration.")
}

/// Why `r` cannot remove a font set.
pub fn cannot_remove_built_in(name: &str) -> String {
    format!("{name} is a font built into vtium, not a font set, so it cannot be removed.")
}

pub fn cannot_remove(name: &str) -> String {
    format!("No font set named {name} is installed or added, so there is nothing to remove.")
}

/// The form's title.
pub const FORM_TITLE: &str = "New font set";

/// The form's rows' labels, one for each field in order.
pub const FIELD_TEXT: &str = "Text font";
pub const FIELD_BOX: &str = "Box drawing";
pub const FIELD_SCRIPTS: &str = "Scripts";
pub const FIELD_SYMBOLS: &str = "Symbols and icons";
pub const FIELD_EMOJI: &str = "Emoji";
pub const FIELD_FALLBACK: &str = "Fallback";
pub const FIELD_NAME: &str = "Name";

/// The form's last row, which adds the font set.
pub const ADD: &str = "Add this font set";

/// The one line under the form.
pub const ROW_ORDER: &str =
    "vtium tries the fonts in row order; each one supplies what those above lack.";

/// A role that holds nothing.
pub const NONE_CHOSEN: &str = "none";

/// The first choice of a role's list, which leaves the role empty.
pub const LEAVE_EMPTY: &str = "None";

/// The last choice of a role's list, which lets the person type a font.
pub const OTHER: &str = "Other…";

/// A font built into vtium, beside its name in the text font's list.
pub const BUILT_IN: &str = "built in";

/// Where box drawing comes from, as the `glyph-source-box=` line chooses.
pub const BOX_DEFAULT: &str = "vtium's own lines, unless the text font has them all";
pub const BOX_OWN: &str = "vtium's own lines";
pub const BOX_FONT: &str = "the text font's own lines";
pub const BOX_AUTO: &str = "the text font's own lines, and vtium's where the font lacks one";

/// The name row while the person has typed no name.
pub fn name_from(made: &str) -> String {
    if made.is_empty() {
        "made from the text font".to_owned()
    } else {
        format!("{made}, made from the text font")
    }
}

/// The text font's pick, with the bold and italic faces it brings, as
/// `Mono with bold, italic`.
pub fn with_styles(face: &str, styles: &[&str]) -> String {
    if styles.is_empty() {
        face.to_owned()
    } else {
        format!("{face} with {}", styles.join(", "))
    }
}

/// What the message line says while a font is typed after Other….
pub const TYPE_FONT: &str = "Type a family, such as JetBrains Mono, or a font set file's name for it, such as kbd:ter-v16n.";

/// What the message line says while the name is typed.
pub const TYPE_NAME: &str =
    "Type the font set's name: letters, digits, '.', '_', '+' or '-', up to 64 of them.";

/// Why the form cannot add the font set.
pub const NEEDS_TEXT: &str = "A font set needs a text font. [Enter] on Text font lists them.";

pub const BAD_NAME: &str = "A font set's name has 1 to 64 letters, digits, '.', '_', '+' or '-', and is not a built-in font's.";

pub fn replaces_binary(name: &str) -> String {
    format!("The installed {name}.vtfrit is not text, so a new font set cannot replace it.")
}

pub fn does_not_read(why: &str) -> String {
    format!("This font set does not read: {why}")
}

/// The name of the change that adds a font set.
pub fn added(name: &str) -> String {
    format!("add the font set {name}")
}

/// F1's text on the form: what a font set is and what each role holds.
pub const ABOUT_FONT_SETS: &str = "A font set names a font for each role, and vtium tries the roles in row order: each later font supplies the characters the fonts before it lack.

Text font: the monospaced font most text draws in. Its bold and italic faces come with it where they are active.
Box drawing: where the lines of boxes and tables come from.
Scripts: fonts for writing systems the text font lacks, such as Arabic or Han. A font set can hold several.
Symbols and icons: a Nerd Fonts or Powerline font for the icons that shell prompts draw.
Emoji: a color emoji font.
Fallback: any font, tried last for what the others lack.
Name: the font set's name. Left empty, it is made from the text font.

A role's list offers only the active fonts that can fill it. To use another font, activate it in the Fonts tab, or pick Other… and type its family. Add this font set puts the font set into the configuration as one change, and Save installs it.";

/// F1's heading over a choice's own facts.
pub const THIS_CHOICE: &str = "The highlighted choice:";

/// The scripts a font adds, in F1's text.
pub fn covers(scripts: &[&str]) -> String {
    format!("covers {}", scripts.join(", "))
}
