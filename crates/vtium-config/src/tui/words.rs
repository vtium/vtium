//! The TUI's words, in one place so they can be reviewed and reworded
//! together (ADR-1201 D3). They are English, as ADR-1402 keeps what
//! vtium's tools print in a terminal. A choice between wordings is two
//! entries, never one sentence built from pieces.

use std::fmt::Display;

/// The title on the first line.
pub const TITLE: &str = "VTium Vitrifier";

/// The views' names, in the order Tab steps through them.
pub const FONTS: &str = "Fonts";
pub const SIZES: &str = "Sizes";
pub const SETTINGS: &str = "Settings";
pub const HEADS: &str = "Heads";
pub const NEW: &str = "New";
pub const SAVE: &str = "Save";

/// What live preview is doing, at the right of the first line.
pub const LIVE_ASKING: &str = "asking the terminal";
pub const LIVE_ON: &str = "live preview: on";
pub const LIVE_OFF: &str = "live preview: off";

/// Why live preview is off, on the message line.
pub const OPS_OFF: &str = "allow-font-ops is off in this vtium console, so fonts are not tried live. Press o to turn it on when you save.";
pub const OPS_ON_PENDING: &str = "allow-font-ops is turned on when you save.";
pub const NOT_VTIUM: &str =
    "This terminal is not a vtium console, so fonts are not tried live. Saving still works.";

/// A font's state, in the font list.
pub const READS: &str = "reads";
pub const BUILT_IN: &str = "built in";
pub fn does_not_read(why: impl Display) -> String {
    format!("does not read: {why}")
}
pub const KEPT: &str = "kept by the console, see its log";
pub fn sources_missing(n: usize) -> String {
    if n == 1 {
        "1 source missing".to_owned()
    } else {
        format!("{n} sources missing")
    }
}
pub const DRAWS_NOTHING: &str = "no source on this machine";

/// Marks the choice that a save writes.
pub const TO_SAVE: &str = "to save";
/// Marks what vtium.conf names now.
pub const SAVED: &str = "saved";

/// The size that lets vtium choose.
pub const AUTOMATIC: &str = "automatic";
pub fn points(n: u32) -> String {
    format!("{n} pt")
}
/// A size's line when the terminal draws no pictures.
pub const NO_PICTURES: &str =
    "This terminal draws no pictures, so a size shows only once it is saved.";
pub fn density(dpi: u32, from: impl Display) -> String {
    format!("Pictures are drawn at {dpi} dpi, {from}.")
}
pub fn no_picture(why: impl Display) -> String {
    format!("No picture of this size: {why}")
}
pub fn size_summary(s: &crate::specimen::Summary) -> String {
    let grid = s
        .grid
        .map(|(c, r)| format!(", the display holds {c}x{r} cells,"))
        .unwrap_or_default();
    format!(
        "At this size the cell is {}x{} pixels{grid} and the font draws {} of the specimen's {} characters itself.",
        s.cell.width, s.cell.height, s.covered, s.of
    )
}
pub fn no_summary(why: impl Display) -> String {
    format!("This size cannot be drawn: {why}")
}
pub const AUTOMATIC_PICTURE: &str =
    "Automatic lets vtium pick the size from the display, so it has no picture.";
pub const DPI_FROM_CONF: &str = "from vtium.conf's display-dpi";
pub const DPI_FROM_EDID: &str = "measured from the display";
pub const DPI_ASSUMED: &str = "assumed, since no display reports its size";

/// The settings view.
pub fn default_value(value: impl Display) -> String {
    format!("{value} (default)")
}
pub const SETTINGS_HINT: &str = "Enter turns a switch on or off, steps a choice, or types a value; an empty value is the default.";
pub const EDITING: &str = "Type the value; Enter keeps it, an empty value is the default.";
pub const ON: &str = "on";
pub const OFF: &str = "off";

/// The heads view.
pub const NO_HEAD_FILE: &str =
    "There is no head file, so vtium runs one head on every display, drawn with font-name.";
pub const HEAD_FILE_ERRORS: &str =
    "The head file holds errors, so it cannot be edited here. Run vtium check-config to see them.";
pub fn head_profile(profile: impl Display) -> String {
    format!("profile {profile}")
}
pub const HEADS_HINT: &str = "Left and Right choose the profile a head draws with.";

/// The new-frit view: its fields, and what it says.
pub const NEW_FIELD_NAMES: [&str; 6] = ["Name", "Source 1", "Source 2", "Source 3", "Cell", "Size"];
pub const NEW_ADD: &str = "Add the frit to the save";
pub const NEW_HINT: &str = "A source is a font's family, as JetBrains Mono, or a source as a frit writes it, as kbd:ter-v16n. Each later source fills what the ones before it lack. Cell is WIDTHxHEIGHT and Size is points; leave them empty for the fonts' own. An empty name is made from the first source.";
pub const NEW_BAD_NAME: &str =
    "the name is 1 to 64 letters, digits, '.', '_', '+' and '-', and not a built-in font's";
pub fn new_added(name: impl Display) -> String {
    format!("{name} is added to the save; Enter saves it.")
}
pub fn new_replaces_binary(name: impl Display) -> String {
    format!(
        "the installed {name}.vtfrit is not UTF-8 text, so a new frit does not replace it; remove it with vtium config remove-frit {name} first"
    )
}
pub fn new_refused(why: impl Display) -> String {
    format!("The frit does not read: {why}")
}

/// The save view.
pub const NOTHING_TO_SAVE: &str = "Nothing is changed, so there is nothing to save.";
pub const SAVE_RUNS: &str = "Enter runs this command, which asks for your password once:";
pub const SAVE_AS_USER: &str = "The tool's directories are not the system's, so Enter installs the draft as you, with no sudo, by this command:";
pub fn draft_under(dir: impl Display) -> String {
    format!("DRAFT is a new directory under {dir}.")
}
pub const SAVING: &str = "Saving. The command runs in the terminal; the screen comes back after.";
pub fn saved(files: usize) -> String {
    if files == 1 {
        "Saved 1 file.".to_owned()
    } else {
        format!("Saved {files} files.")
    }
}
pub const COMMAND_SAID: &str = "The command said:";
pub fn save_failed(why: impl Display) -> String {
    format!("Nothing was saved: {why}")
}

/// The keys, on the last line of each view.
pub const KEYS_FONTS: &str = "Enter keep  Tab next  s save  q leave, undo";
pub const KEYS_SIZES: &str = "Enter keep  Tab next  s save  q leave, undo";
pub const KEYS_SETTINGS: &str = "Enter change  Tab next  s save  q leave, undo";
pub const KEYS_HEADS: &str = "Left Right profile  Tab next  s save  q leave";
pub const KEYS_NEW: &str = "type a field  Enter next  Tab next view  Ctrl-C leave, undo";
pub const KEYS_EDITING: &str = "Enter keep  Up Down drop  Ctrl-C leave, undo";
pub const KEYS_SAVE: &str = "Enter save  Backspace back  q leave, undo";
pub const KEYS_SMALL: &str = "Enter keep  q back";

/// What leaving shows while the console's font comes back.
pub const LEAVING: &str = "Putting the console's font back.";

/// Why vtium.conf cannot take an edit.
pub fn conf_edit(e: &vtium_settings::ConfEdit) -> String {
    match e {
        vtium_settings::ConfEdit::BadKey(key) => format!(
            "vtium.conf has no key named {key:?}: a key is lowercase letters, digits and -."
        ),
        vtium_settings::ConfEdit::BadValue { key, value } => format!(
            "{value:?} cannot be written for {key}, since vtium would read it back as something else."
        ),
    }
}
