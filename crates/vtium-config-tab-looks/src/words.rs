//! What the Looks tab says on screen. Each string follows the rules for
//! strings on screen: the topic first, the reader's words, one line where
//! a paragraph was, and every key to press in brackets.
//!
//! References:
//! - crates/AGENTS.md, "Strings on screen", rules 1 to 8.
//! - ADR-1403 D11, "One set of marks for a font": the marks' words.

use crate::list::Group;

/// The tab's name on the top row.
pub const LOOKS: &str = "Looks";

/// A group's heading in the list.
pub fn group(g: Group) -> &'static str {
    match g {
        Group::Looks => "Looks",
        Group::FontSets => "Font sets",
        Group::Schemes => "Color schemes",
        Group::Pointers => "Pointer themes",
    }
}

/// The looks group's one row while the document has no look.
pub const NO_LOOKS: &str = "No look yet";

/// What a look is, beside the row that says the document has none.
pub const NO_LOOKS_NOTE: &str =
    "A look puts a font set, a size, a color scheme and a vtlook together for a terminal.";

/// Enter on the row that says the document has no look.
pub const NO_LOOK_TO_OPEN: &str = "The configuration has no look to open yet.";

/// The pointer theme of vtium's own pictures.
pub const POINTER_OWN: &str = "vtium's own pictures";

/// A font set the document removes, which Save removes.
pub const REMOVED_AT_SAVE: &str = "removed at the next save";

/// A font set only the document adds.
pub const NEW_NOT_SAVED: &str = "new, not saved";

/// The card's lines' labels.
pub const LINE_NAME: &str = "Name";
pub const LINE_FONT_SET: &str = "Font set";
pub const LINE_SIZE: &str = "Size";
pub const LINE_PALETTE: &str = "Color scheme";
pub const LINE_VTLOOK: &str = "vtlook";
pub const LINE_POINTER: &str = "Pointer theme";
pub const LINE_FONTS: &str = "Fonts";

/// The mark at the right of the highlighted line that Left and Right step.
pub const STEPS: &str = "◂ ▸";

/// The line under the card about when a size applies.
pub const SIZE_APPLIES: &str = "A size applies when vtium starts again.";

/// The heading of a look's fonts.
pub const FONTS_BY_ROLE: &str = "Fonts by role";

/// The role of a font set's own sources.
pub const ROLE_TEXT: &str = "Text";

/// The role of a layer's fonts, by the layer's name.
pub fn layer_role(layer: &str) -> String {
    let mut chars = layer.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    })
}

/// How a font set names a font, as a row shows it: a family by its name
/// and a file by its path, with the kind before the colon left out.
pub fn named(spelling: &str) -> String {
    let rest = spelling.split_once(':').map_or(spelling, |(_, r)| r);
    vtium_config_widgets::canvas::printable(rest.trim())
}

/// Every font is active.
pub const ALL_ACTIVE: &str = "● all active";

/// How many of a look's fonts are not active, beside the card.
pub fn fonts_not_active(n: usize) -> String {
    match n {
        1 => "1 font not active".to_owned(),
        n => format!("{n} fonts not active"),
    }
}

/// The card's line that offers Activate.
pub fn activate_line(n: usize) -> String {
    format!("{} · [A] Activate", fonts_not_active(n))
}

/// Under the card beside the list, where Enter shows which fonts.
pub const ENTER_FOR_WHICH: &str = "[Enter] says which and why";

/// Beside a font that is missing or fails the trust rule.
pub const SAYS_WHY: &str = "[Enter] says why";

/// Beside a font that Activate activates.
pub const ACTIVATES_IT: &str = "[A] activates it";

/// The card's line where its font set names no font file.
pub const NO_FONT_FILES: &str = "The font set names no font file vtium-config can check.";

/// The card's line for a font built into vtium.
pub fn built_in_text(name: &str) -> String {
    format!("{ROLE_TEXT}  vtium's built-in font {name}  ● always active")
}

/// The title of what a look still misses.
pub fn misses_title(look: &str) -> String {
    format!("{look} › What it still misses")
}

/// One line of what a look misses.
pub fn miss(role: &str, font: &str, why: &str) -> String {
    format!("{role}: {font} {why}")
}

/// Why a font that is installed and trusted is not active.
pub const MISS_NOT_ACTIVE: &str = "is installed and ○ not active. [A] activates it.";

/// Why a font turned off on purpose stays off.
pub const MISS_TURNED_OFF: &str =
    "is ○ turned off on purpose, so Activate leaves it off. Space in the Fonts tab turns it on.";

/// Why a font that is not installed is missing.
pub const MISS_NOT_INSTALLED: &str = "is ✗ missing: it is not installed.";

/// Why a font that fails the trust rule is never activated.
pub fn miss_trust(check: &str) -> String {
    format!("is ○ not active: it fails trust, so vtium never activates it. {check}")
}

/// A look that misses no font.
pub const MISSES_NOTHING: &str = "It misses nothing: every font it uses is active.";

/// What `[A]` does on the screen of what a look misses.
pub fn activate_hint(n: usize) -> String {
    format!(
        "[A] activates the {} that can be activated.",
        plural(n, "font")
    )
}

/// `1 font` or `N fonts`.
fn plural(n: usize, what: &str) -> String {
    match n {
        1 => format!("1 {what}"),
        n => format!("{n} {what}s"),
    }
}

/// A size in points.
pub fn points(n: u32) -> String {
    format!("{n} pt")
}

/// A size of 0, which vtium picks for each display.
pub const AUTOMATIC: &str = "automatic";

/// A value vtium.conf leaves unset.
pub const NONE: &str = "none";

/// The choice, and the line, that leave the look's key to vtium.conf.
pub fn from_conf(value: &str) -> String {
    format!("{value}, from vtium.conf")
}

/// A value the look sets that the usual choices lack.
pub fn own_choice(value: &str) -> String {
    format!("{value}, the look's own")
}

/// A font built into vtium, as a choice.
pub fn built_in_font(name: &str) -> String {
    format!("{name}, built in")
}

/// The built-in vtlook.
pub const NO_FILTER: &str = "No filter";

/// A vtlook that does not read, as a choice.
pub fn does_not_read(name: &str) -> String {
    format!("{name}, does not read")
}

/// The name of the change that sets or unsets a look's key.
pub fn change_name(key: &str, value: Option<&str>, look: &str) -> String {
    match value {
        Some(v) => format!("{key} = {v} in {look}"),
        None => format!("{key} from vtium.conf in {look}"),
    }
}

/// The name of the change that renames a look.
pub fn renamed(old: &str, new: &str) -> String {
    format!("look {old} renamed {new}")
}

/// The card's title line when its look is gone, as after an undo.
pub fn look_gone(name: &str) -> String {
    format!("The look {name} is no longer in the configuration. [Esc] goes back.")
}

/// Which terminals draw a look.
pub fn drawn_by(terminals: &[String]) -> String {
    match terminals {
        [] => "used by no terminal".to_owned(),
        [one] => format!("used by the terminal {one}"),
        many => format!("used by the terminals {}", many.join(", ")),
    }
}

/// Which looks use a font set.
pub fn used_by(looks: &[String]) -> String {
    match looks {
        [] => "used by no look".to_owned(),
        many => format!("used by {}", many.join(", ")),
    }
}

/// The looks that use a resource, after its label.
pub fn looks(looks: &[String]) -> String {
    match looks {
        [] => "no look".to_owned(),
        many => many.join(", "),
    }
}

/// A user of a resource: a look by its name, or vtium.conf's top level.
pub fn user(look: Option<&str>) -> String {
    look.map_or_else(|| "vtium.conf".to_owned(), str::to_owned)
}

/// The preview's titles.
pub const SAMPLE_TITLE: &str = "as the console draws it";
pub const PICTURE_TITLE: &str = "picture of the console";

/// The title of a color scheme's box of colors.
pub fn colors_title(title: &str) -> String {
    format!("{}'s colors", title.trim_start_matches("Color scheme "))
}

/// A line of text in a scheme's foreground on its background.
pub const SAMPLE_LINE: &str = " ~/notes $ ls -l todo.txt error done ";

/// The sample line's words without color.
pub fn sample_words(fg: &str, bg: &str) -> String {
    format!("{} in {fg} on {bg}", SAMPLE_LINE.trim())
}

/// A scheme's foreground and background.
pub fn fg_bg(fg: &str, bg: &str) -> String {
    format!("foreground {fg}, background {bg}")
}

/// The heading of a scheme's colors.
pub const COLORS_HEADING: &str = "Its 16 colors, then its foreground and background";

/// The marks of a resource, in the words of ADR-1403 D11's table.
pub const ACTIVE: &str = "● active";
pub const NOT_ACTIVE: &str = "○ not active";
pub const NOT_ACTIVE_DOT: &str = "○ not active · ";
pub const TURNED_OFF: &str = "○ turned off on purpose";
pub const FAILS_TRUST: &str = "fails trust";
pub const NOT_USABLE_YET: &str = "not usable yet";
pub const ALWAYS_ACTIVE: &str = "● always active";
pub const TRUSTED: &str = " · trusted";

/// The labels of a detail's facts.
pub const FILE: &str = "File";
pub const FOLDER: &str = "Folder";
pub const STATE: &str = "State";
pub const TRUST: &str = "Trust";
pub const USED_BY: &str = "Used by";

/// A scheme built into vtium, after its label.
pub const BUILT_IN: &str = "built in";

/// A pointer theme's folder, activated with every file in it.
pub const WHOLE_FOLDER: &str = "          every file in it is active while the folder is";

/// A resource that fails the trust rule.
pub const FAILS_COUNTS_MISSING: &str = "vtium treats a theme that fails the trust rule as missing.";

/// The titles of the details.
pub fn font_set_title(name: &str) -> String {
    format!("Font set {name}")
}

pub fn scheme_title(name: &str) -> String {
    format!("Color scheme {name}")
}

pub fn pointer_title(name: &str) -> String {
    format!("Pointer theme {name}")
}

/// A font set the console reads.
pub const READS: &str = "The console reads it without errors.";

/// Why a font set or a scheme does not read.
pub fn does_not_read_why(why: &str) -> String {
    format!("It does not read: {why}")
}

/// A font set that the document neither has nor adds.
pub const NO_SUCH_SET: &str = "No font set of this name is installed or added.";

/// A font set no look uses, whose fonts the walk did not reach.
pub const NOT_CHECKED: &str = "  No look uses it, so vtium-config does not check its fonts.";

/// The heading of what a font set still misses.
pub const STILL_MISSES: &str = "What it still misses";

/// Space on a built-in scheme.
pub fn always_active(name: &str) -> String {
    format!("{name} is built in and always active.")
}

/// Space on a resource vtium cannot use.
pub fn cannot_activate(name: &str) -> String {
    format!("{name} cannot be activated: it fails the trust rule ([F1]).")
}

/// The names of Space's changes.
pub fn activated(name: &str) -> String {
    format!("activate {name}")
}

pub fn deactivated(name: &str) -> String {
    format!("deactivate {name}")
}

/// What Find says each kind of item is.
pub const KIND_LOOK: &str = "look";
pub const KIND_FONT_SET: &str = "font set";
pub const KIND_SCHEME: &str = "color scheme";
pub const KIND_POINTER: &str = "pointer theme";

/// The list's right when the filter matches nothing.
pub fn no_match(entry: &str) -> String {
    format!("Nothing matches {entry}.")
}

/// F1's heading of a line's choices.
pub const ITS_CHOICES: &str = "Its choices:";

/// Why `t` cannot try a look or a font set.
pub const TRY_NOT_VTIUM: &str = "Try live is off: this terminal is not a vtium console.";
pub const TRY_OPS_OFF: &str = "Try live is off: the console's allow-font-ops is off.";
pub const TRY_ASKING: &str = "Try live waits for this terminal to say what it is.";

pub fn try_after_save(name: &str) -> String {
    format!("{name} can be tried after [Ctrl+S] Save, which installs it.")
}

pub fn try_does_not_read(name: &str) -> String {
    format!("The console cannot try {name}: the font set does not read ([Enter] says why).")
}

/// `w`'s question, and what it says where the document already lets
/// programs resize the grid.
pub const WINDOW_OPS_QUESTION: &str = "Let programs resize the grid? This sets allow-window-ops = on: a program can then set the console's rows and columns, and the font size follows.";
pub const WINDOW_OPS_ON: &str = "Programs can already resize the grid: allow-window-ops is on.";

/// The name of the change `w`'s yes makes.
pub const WINDOW_OPS_STEP: &str = "allow-window-ops = on";
