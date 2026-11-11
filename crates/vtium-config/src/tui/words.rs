//! The TUI's words, in one place so they can be reviewed and reworded
//! together (ADR-1201 D3). They are English, as ADR-1402 keeps what
//! vtium's tools print in a terminal. A choice between wordings is two
//! entries, never one sentence built from pieces.

use std::fmt::Display;

/// The views' names, in the order Tab steps through them (ADR-1201 D3).
pub const FLOW: &str = "Flow";
pub const LOOKS: &str = "Looks";
pub const FONTS: &str = "Fonts";
pub const SETTINGS: &str = "Settings";
pub const HELP: &str = "Help";

/// The actions beside the views in the tab bar (ADR-1403 D7), each in its
/// long and its short form.
/// The actions the tab bar shows beside the Looks, Fonts and Settings
/// views (ADR-1403 D7, What carries over); the Help view shows none.
pub const ACTIONS: [(&str, &str); 2] = [("Save", "Save"), ("Install", "Install")];

/// How the TUI previews a change, and whether the live trial works, at
/// the right of the status line (ADR-1201 D4).
pub const PREVIEW_ASKING: &str = "checking this terminal";
pub const PREVIEW_PICTURE: &str = "image preview";
pub const PREVIEW_TEXT: &str = "text preview";
/// The short forms, for a status line whose left side does not fit beside
/// the long ones.
pub const PREVIEW_PICTURE_SHORT: &str = "image";
pub const PREVIEW_TEXT_SHORT: &str = "text";
pub const TRIAL_KEY: &str = "t: try live";
pub const TRIAL_NONE: &str = "no live trial";

/// Why the live trial does not work, on the message line.
pub const OPS_OFF: &str = "Live trial is off: this vtium console has allow-font-ops turned off, so t cannot try a font on it. To turn it on, press o in the Fonts view, then install.";
pub const NOT_VTIUM: &str = "Live trial is off: this terminal is not a vtium console, so t cannot try a font on it. You can still install fonts.";

/// The live trial (ADR-1201 D4).
pub const TRIAL_OFF: &str = "The live trial works only in a vtium console with allow-font-ops on.";
pub fn trial_countdown(font: impl Display, left: u32) -> String {
    format!(
        "Trying {font} on the console for {left} s. Press k to keep it, or any other key to put the font back."
    )
}
pub fn trial_kept(font: impl Display) -> String {
    format!(
        "The console keeps {font} until you quit vtium-config. Press Enter to put it in the draft, then install to keep it for good."
    )
}
pub fn trial_back(font: impl Display) -> String {
    format!("The console is back to {font}.")
}
pub fn trial_same(font: impl Display) -> String {
    format!("The console already uses {font}.")
}
pub fn trial_refused(font: impl Display) -> String {
    format!("The console refused {font}. Its log says why.")
}

/// The line beside the detailed specimen (ADR-1201 D4).
pub fn detail_caption(
    font: impl Display,
    points: u32,
    summary: Option<&crate::specimen::Summary>,
    (w, h): (u32, u32),
    assumed: bool,
) -> String {
    let marks = "Characters in grey come from the built-in font, and a box marks a character no font can display. Press s to see the console.";
    let Some(s) = summary else {
        return format!("{font} at {points} pt. {marks}");
    };
    let display = if assumed {
        format!("on a {w}x{h} display (assumed)")
    } else {
        format!("on this {w}x{h} display")
    };
    let grid = s
        .grid
        .map(|(c, r)| format!(", and the console will be {c}x{r} characters {display}"))
        .unwrap_or_default();
    format!(
        "{font} at {points} pt: each character cell is {}x{} pixels{grid}. The font can display {} of the {} characters in the preview. {marks}",
        s.cell.width, s.cell.height, s.covered, s.of
    )
}

/// The line beside a picture of the console (ADR-1201 D4).
pub fn caption(c: &super::shot::Caption) -> String {
    let (w, h) = c.display;
    let display = if c.assumed {
        format!("a {w}x{h} display (assumed, because no display was found)")
    } else {
        format!("this {w}x{h} display")
    };
    format!(
        "{} at {} pt on {display}: each character cell is {}x{} pixels, and the console will be {} characters wide and {} lines high.",
        c.font, c.points, c.cell.width, c.cell.height, c.grid.0, c.grid.1
    )
}

/// A font's state, in the font list.
pub const READS: &str = "reads";
pub const BUILT_IN: &str = "built in";
pub fn does_not_read(why: impl Display) -> String {
    format!("cannot be read: {why}")
}
pub const KEPT: &str = "not used: the console kept its font (see its log)";
pub fn sources_missing(n: usize) -> String {
    if n == 1 {
        "1 font file missing".to_owned()
    } else {
        format!("{n} font files missing")
    }
}
pub const DRAWS_NOTHING: &str = "no font file on this machine";
pub const SOURCES_UNKNOWN: &str = "font files not checked";

/// Marks the choice that an install writes.
pub const TO_SAVE: &str = "to install";
/// Marks what vtium.conf names now.
pub const SAVED: &str = "installed";
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
pub fn remove_unread(name: impl Display) -> String {
    format!("{name}.vtfrit could not be read when vtium-config started")
}

/// The size that lets vtium choose.
pub const AUTOMATIC: &str = "automatic";
pub fn points(n: u32) -> String {
    format!("{n} pt")
}
/// A size's line when the terminal draws no pictures.
pub const NO_PICTURES: &str =
    "This terminal does not support images, so here is a written description instead.";
pub fn density(dpi: u32, from: impl Display) -> String {
    format!("The preview uses {dpi} dpi, {from}.")
}
pub fn no_picture(why: impl Display) -> String {
    format!("No preview image of the console: {why}")
}
pub fn size_summary(s: &crate::specimen::Summary) -> String {
    let grid = s
        .grid
        .map(|(c, r)| format!(" the console will be {c}x{r} characters, and"))
        .unwrap_or_else(|| " and".to_owned());
    format!(
        "With this font and size, each character cell is {}x{} pixels,{grid} the font can display {} of the {} characters in the preview.",
        s.cell.width, s.cell.height, s.covered, s.of
    )
}
pub fn no_summary(why: impl Display) -> String {
    format!("This size cannot be used: {why}")
}
pub const AUTOMATIC_PICTURE: &str =
    "Automatic lets vtium pick the size for each display, so there is no preview.";
pub const DPI_FROM_CONF: &str = "set by display-dpi in vtium.conf";
pub const DPI_FROM_EDID: &str = "measured from the display";
pub const DPI_ASSUMED: &str = "assumed, because no display reports its size";

/// The settings view.
pub fn default_value(value: impl Display) -> String {
    format!("{value} (default)")
}
pub const SETTINGS_HINT: &str = "Press Enter to turn a setting on or off, pick the next choice, or type a value. Leave a value empty to use the default.";
pub const EDITING: &str = "Type the value and press Enter. Leave it empty to use the default.";
/// What a key binding's editor says (ADR-1005 D10): an empty value binds
/// no key, so the program gets the key, and the word [`BINDING_DEFAULT`]
/// returns the key binding to its default.
pub const EDITING_BINDING: &str = "Type the key combinations, separated by commas, and press Enter. Leave it empty to bind no key, so programs get it, or type default to use the default.";
/// The word that returns a key binding to its default in its editor.
pub const BINDING_DEFAULT: &str = "default";
/// A key binding that binds no key, as the Settings view and the base
/// card show it.
pub const NO_KEY: &str = "none";
pub const OFF: &str = "off";

/// The Looks view, which holds the sizes until looks are built.
pub const LOOKS_SIZES: &str = "Font sizes for the console, in points.";

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

/// The install view, which installs the changes made in the views.
pub fn saved(files: usize) -> String {
    if files == 1 {
        "Installed 1 file.".to_owned()
    } else {
        format!("Installed {files} files.")
    }
}

/// The keys, on the last line of each view.
pub const KEYS_LOOKS: &str = "Enter choose  Tab next view  i install  q quit without installing";
pub const KEYS_FONTS: &str =
    "Enter choose  t try live  s preview  n new  r remove  i install  q quit";
pub const KEYS_SETTINGS: &str = "Enter change  Tab next view  i install  q quit without installing";
pub const KEYS_HELP: &str = "←→ Help, About  ↑↓ scroll  Tab next view  q quit";
pub const KEYS_NEW: &str =
    "type into a field  Enter next field  Esc back  Ctrl-C quit without installing";
pub const KEYS_EDITING: &str = "Enter keep  ↑↓ cancel  Ctrl-C quit without installing";

/// The screen below the TUI's minimum size (ADR-1201 D3).
pub fn too_small(cols: usize, rows: usize) -> String {
    format!(
        "This terminal is too small: vtium-config needs at least 80x25 characters, and this terminal is {cols}x{rows}."
    )
}
pub const TOO_SMALL_COMMANDS: &str =
    "Make this terminal larger, or use these commands, which work at any size:";
pub const TOO_SMALL_KEYS: &str = "Press q to quit. If a font you tried made this terminal too small, press u to put back the font you had before.";

/// The status line (ADR-1403 D7).
pub const NO_DRAFT: &str = "no draft";
pub fn draft_state(name: impl Display, changes: usize, unsaved: usize, problems: usize) -> String {
    let mut changes = match changes {
        0 => "no changes".to_owned(),
        1 => "1 change".to_owned(),
        n => format!("{n} changes"),
    };
    if unsaved > 0 {
        changes = format!("{changes} · {unsaved} not saved");
    }
    let problems = match problems {
        0 => "no problems".to_owned(),
        1 => "1 problem".to_owned(),
        n => format!("{n} problems"),
    };
    format!("draft {name} · {changes} · {problems}")
}
/// What `s` says once the unsaved changes are in the draft.
pub fn saved_draft(n: usize, name: &str) -> String {
    match n {
        1 => format!("Saved 1 change into draft {name}."),
        n => format!("Saved {n} changes into draft {name}."),
    }
}
pub const NOTHING_TO_SAVE: &str = "Nothing to save: the draft already holds every change.";
/// The question `q` asks while changes are not saved (ADR-1403 D7).
pub fn quit_question(n: usize) -> String {
    let changes = match n {
        1 => "1 change is".to_owned(),
        n => format!("{n} changes are"),
    };
    format!(
        "{changes} not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back."
    )
}
pub const KEYS_QUIT: &str = "y save and quit  n quit without saving  Esc back";
/// A question that throws the draft away, with the unsaved changes it
/// also throws away.
pub fn losing_unsaved(question: String, n: usize) -> String {
    match n {
        0 => question,
        1 => format!("{question} This includes 1 change that is not saved."),
        n => format!("{question} This includes {n} changes that are not saved."),
    }
}

pub fn consoles(n: u32) -> String {
    if n == 1 {
        "1 console".to_owned()
    } else {
        format!("{n} consoles")
    }
}
pub const LOOK: &str = "look";
pub fn roaming(active: Option<&str>) -> String {
    match active {
        Some(head) => format!("types into {head} (active head)"),
        None => "types into the active head".to_owned(),
    }
}
pub fn policy(value: impl Display) -> String {
    format!("policy {value}")
}
pub fn shadowed(by: usize) -> String {
    format!("rule {by} matches first")
}

/// The card Enter opens below the columns.
pub const SENDS_TO: &str = "sends to";
pub const MATCHES: &str = "matches";

pub const HELP_VIEWS: [(&str, &str, &str); 5] = [
    (
        FLOW,
        "shows how the console is set up, from the displays to the input devices: each display, the rule that sends it to a head (a screen with its own consoles), the head and its look (its font and size), and each input device with the rule that sends its typing to a head.",
        KEYS_FLOW,
    ),
    (
        LOOKS,
        "lists the console's font sizes, automatic first, with a preview of each. Later it will manage the looks, each a font set and a size.",
        KEYS_LOOKS,
    ),
    (
        FONTS,
        "lists the font sets (frits) and the built-in fonts, with any font files each is missing. It shows each as an image of the console, or with s as a preview of every character it can display. On a vtium console, t tries a font live. Press n to make a new font set.",
        KEYS_FONTS,
    ),
    (
        SETTINGS,
        "lists every setting at the top of vtium.conf, with its value and what it does.",
        KEYS_SETTINGS,
    ),
    (
        HELP,
        "shows this page, and About, with vtium-config's name and version.",
        KEYS_HELP,
    ),
];
pub const HELP_TABS: &str = "Tab moves to the next view, and Shift-Tab moves to the one before, wherever the selection is. The arrow keys move inside a view. While a card, a list, the draft's text, the form for a new font set, or a value you are typing is open, Tab does nothing until you close it with Esc. While a question waits for your answer, Tab answers no.";
pub const HELP_COMMANDS: &str = "Commands, which work in a terminal of any size:";

/// What leaving shows while the console's font comes back.
pub const LEAVING: &str = "Restoring the console's font…";

/// Why vtium.conf cannot take an edit.
pub fn conf_edit(e: &vtium_settings::ConfEdit) -> String {
    match e {
        vtium_settings::ConfEdit::BadKey(key) => format!(
            "There is no setting named {key:?}. A setting's name uses lowercase letters, digits and -."
        ),
        vtium_settings::ConfEdit::BadValue { key, value } => format!(
            "{key} cannot be set to {value:?}: vtium would read it back as a different value."
        ),
    }
}

/// The Flow view's changes to the draft (ADR-1403 D7, D8).
pub const NAME: &str = "name";
pub const FRIT: &str = "font set";
pub const SIZE: &str = "size";
pub const NO_DESK: &str = "This is a sample setup with no draft, so changes here are not saved.";
pub const NO_HOTPLUG: &str = "not watching for new devices";
pub const NOT_DONE: &str = "Nothing changed.";
pub const FORM_RULE_NEEDS: &str = "A new rule needs a match and a target.";
pub const FORM_NAME_NEEDED: &str = "A new head or look needs a name.";
/// `a`, `a and b`, `a, b and c`.
fn list(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.collect();
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}
pub const CONNECT_TWO_TARGETS: &str = "Cannot connect: two heads or targets are tagged. Tag one head, or one of roaming, ignore and off.";
pub const CONNECT_TWO_LOOKS: &str = "Cannot connect: two looks are tagged. Tag one.";
pub const CONNECT_TWO_RULES: &str = "Cannot connect: two rules are tagged. Tag one.";
pub const CONNECT_TWO_KINDS: &str = "Cannot connect a display and an input device together. Tag displays or input devices, not both.";
pub const CONNECT_RULE_ONE_DEVICE: &str = "Cannot connect: a rule takes one device. Tag one.";
pub const CONNECT_NO_HEAD: &str =
    "Cannot connect: no head is tagged. Tag a head, or roaming, ignore or off.";
pub const CONNECT_LOOK_NO_HEAD: &str = "Cannot connect: a look connects to a head. Tag a head too.";
pub const CONNECT_NOTHING: &str = "Nothing is tagged. Press Space to tag the selected item.";
pub fn connect_display_to(target: &str) -> String {
    format!("A display cannot be sent to {target}. Send it to a head or to off.")
}
pub fn connect_input_to(target: &str) -> String {
    format!("An input device cannot be sent to {target}. Send it to a head, roaming or ignore.")
}
pub fn connect_look_to(target: &str) -> String {
    format!("A look connects to a head, not to {target}.")
}
pub fn connect_no_key(device: &str) -> String {
    format!("No rule can name {device}: it reports no name, path or other key to match.")
}
pub const INSTALLING: &str = "Installing. sudo may ask for your password.";
pub const NOTHING_TO_INSTALL: &str =
    "Nothing to install: the draft is the same as the installed vtium.conf.";
pub fn install_question(added: usize, removed: usize, frits: usize) -> String {
    match frits {
        0 => format!("Install the draft? {}.", lines(added, removed)),
        1 => format!(
            "Install the draft? {}, and 1 font set.",
            lines(added, removed)
        ),
        n => format!(
            "Install the draft? {}, and {n} font sets.",
            lines(added, removed)
        ),
    }
}
/// The first half of Install's question for a draft that wires no
/// complete route, and of the refusal without a terminal (ADR-1403 D7,
/// D8).
pub const NO_ROUTE: &str = "This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches.";
/// What `vtium config install` says, with no terminal to ask and no
/// `--force`, of a draft that wires no complete route (ADR-1403 D8).
pub fn no_route_force() -> String {
    format!("{NO_ROUTE} To install it anyway, run vtium config install --force.")
}
/// What `vtium config install` says when the person answers no.
pub const NO_ROUTE_NOT_INSTALLED: &str =
    "Nothing installed: you answered no. The draft keeps your changes.";
/// Install's question for a draft whose only problem is that it wires no
/// complete route (ADR-1403 D7).
pub fn no_route_question(added: usize, removed: usize, frits: usize) -> String {
    let install = install_question(added, removed, frits);
    let counted = install
        .strip_prefix("Install the draft? ")
        .unwrap_or(&install);
    format!(
        "{NO_ROUTE} After the install, nobody at this machine may be able to type into vtium. Install it anyway? {counted}"
    )
}
pub const NOT_PREPARED: &str = "The install is not ready yet. Press i again.";
pub fn install_failed(why: &str) -> String {
    format!("Not installed: {why}")
}
pub fn installed(name: &str) -> String {
    format!("Installed draft {name}. The console uses it after vtium restarts.")
}
pub fn revert_question(added: usize, removed: usize) -> String {
    format!(
        "Throw away the draft's changes? {} will be lost.",
        lost(added, removed)
    )
}
pub const REVERTED: &str = "The draft now matches the installed vtium.conf.";
pub const NEW_QUESTION: &str =
    "Start a new configuration? The draft's heads, rules and looks will be lost.";
pub const NEW_DONE: &str = "The draft starts over, with every setting at its default and one look, named default. Send a display and a keyboard to a head before you install.";
pub fn deleted_draft(next: &str) -> String {
    format!("Draft deleted. Draft {next} starts again from the installed vtium.conf.")
}
pub const NO_HISTORY: &str = "Nothing to restore: vtium-config has not installed anything yet.";
pub fn restore_question(n: usize, added: usize, removed: usize) -> String {
    format!(
        "Replace the draft with earlier file {n}? {} will be lost.",
        lost(added, removed)
    )
}
pub const RESTORED: &str = "The draft is now the file you chose. Install it to put it back in use.";
pub fn no_history_file(n: usize) -> String {
    format!("There is no earlier file {n}")
}
pub const RESTORE_TITLE: &str = "Restore an earlier vtium.conf, newest first";
pub const PAGER_TITLE: &str = "The draft (read only)";
/// `3 lines added and 1 removed`, as a question says what is lost.
fn lost(added: usize, removed: usize) -> String {
    match (added, removed) {
        (0, 0) => "No lines".to_owned(),
        (a, 0) => format!("{a} {} added", if a == 1 { "line" } else { "lines" }),
        (0, r) => format!("{r} {} removed", if r == 1 { "line" } else { "lines" }),
        (a, r) => format!(
            "{a} {} added and {r} removed",
            if a == 1 { "line" } else { "lines" }
        ),
    }
}
/// `3 lines added, 1 removed`.
fn lines(added: usize, removed: usize) -> String {
    crate::drafts::lines(added, removed)
}
pub fn came_went(came: &[String], went: &[String]) -> String {
    let came = (!came.is_empty()).then(|| format!("connected: {}", list(came.iter().cloned())));
    let went = (!went.is_empty()).then(|| format!("disconnected: {}", list(went.iter().cloned())));
    match (came, went) {
        (Some(c), Some(w)) => format!("{c}; {w}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => "no device changes".to_owned(),
    }
}
pub const KEYS_TYPING: &str = "type the value  Enter keep  Esc cancel";
pub const KEYS_CONFIRM: &str = "y yes  any other key no";
pub const KEYS_LEAVE: &str = "i install  l quit anyway  any other key back";
pub const KEYS_PAGER: &str =
    "s save  i install  j k ↑↓ lines  h l ←→ sideways  g G top bottom  ^D ^U half page  q back";
pub const KEYS_RESTORE: &str = "↑↓ choose  Enter restore  q back";
pub fn leave_question(added: &[String], removed: &[String]) -> String {
    let mut lost = Vec::new();
    if !added.is_empty() {
        lost.push(format!("the new font sets {}", list(added.iter().cloned())));
    }
    if !removed.is_empty() {
        lost.push(format!("the removal of {}", list(removed.iter().cloned())));
    }
    format!(
        "Quit without installing? You will lose {}. Press i to install, l to quit anyway, or any other key to go back.",
        list(lost.into_iter())
    )
}
pub const NOT_LEFT: &str = "Nothing is lost.";
/// The short form of [`frits_waiting`], for a status line too narrow for
/// the long one.
pub fn frits_waiting_short(n: usize) -> String {
    format!("{n} to install")
}
pub fn frits_waiting(n: usize) -> String {
    if n == 1 {
        "1 font set to install".to_owned()
    } else {
        format!("{n} font sets to install")
    }
}

/// The Flow view's sections, boxes and sentences (ADR-1403 D7).
pub const FLOW_PATHS: &str = "CONFIGURATION";
pub const FLOW_NEW_PATH: &str = "NEW PATH";
pub const FLOW_PIECES: &str = "PARTS";
pub const FLOW_DISPLAYS: &str = "DISPLAYS";
pub const FLOW_TERMINALS: &str = "TERMINALS";
pub const FLOW_INPUTS: &str = "INPUTS";
pub const FLOW_FONT_SETS: &str = "FONT SETS";
pub const NO_PATHS: &str =
    "No paths yet. Pick an input and a display in PARTS, then press a to add the path.";
pub fn unfit_rules(n: usize) -> String {
    match n {
        1 => "1 rule belongs to no path. v shows it in the file.".to_owned(),
        n => format!("{n} rules belong to no path. v shows them in the file."),
    }
}
pub const FLOW_ROAMING: &str = "ROAMING";
pub const FLOW_ROAMING_ABOUT: &str = "These inputs type into the active terminal.";
pub const FLOW_ROAMING_STUB: &str = "roaming inputs";
pub const ROAMING_WORD: &str = "roaming";
pub const REVISE_ROAMING_NOTE: &str = "To change the roaming inputs, press e. They go into NEW PATH with \u{201c}Active terminal\u{201d}, and a adds them back.";
pub const IN_ROAMING: &str = "This input roams. Press e to revise the roaming inputs.";
pub const PATH_EMPTY: &str = "Pick an input and a display to make a path.";
pub const PATH_NEEDS_DISPLAY: &str = "Add a display to finish this path.";
pub const PATH_COMPLETE: &str = "This path is complete. Press a to add it.";
/// The special parts' names.
pub const NO_INPUT: &str = "No input";
/// A part picked into the path to no terminal.
pub fn picked_aside(title: &str, display: bool) -> String {
    if display {
        format!("Picked {title}. It will stay off.")
    } else {
        format!("Picked {title}. It will be set aside.")
    }
}
pub const NO_TERMINAL_ABOUT: &str = "Nowhere is where a path sends what vtium leaves alone: its displays stay off and its inputs are ignored.";
pub const NONE_REVISED: &str =
    "The path to Nowhere is in NEW PATH. Change its parts, then press a to add it back.";
pub const EMPTY_SPACE: &str =
    "This place in the new path is empty. Press Enter to choose a part for it.";
pub const NO_INPUT_NEEDS_TERMINAL: &str = "\u{201c}No input\u{201d} belongs to a path with a terminal of its own. Take \u{201c}Active terminal\u{201d} or \u{201c}Nowhere\u{201d} out of this path first.";
/// Space on a template.
pub fn template_space(stage: &str) -> String {
    format!(
        "The {stage} template adds {}s. Press Enter to add one.",
        stage.to_lowercase()
    )
}
/// Enter on a special part.
pub fn special_about(title: &str) -> String {
    match title {
        NO_INPUT => "\u{201c}No input\u{201d} means no keyboard or mouse reaches the terminal, not even a roaming one. In vtium.conf, the terminal's section gets isolated = yes.".to_owned(),
        NO_TERMINAL => NO_TERMINAL_ABOUT.to_owned(),
        ACTIVE_TERMINAL => ACTIVE_ABOUT.to_owned(),
        name => format!("{name} is a new terminal. Press Space to put it in the new path. There, press Enter on it to set its name, font set and size."),
    }
}
/// A part made from a template in PARTS.
pub fn part_made(title: &str) -> String {
    format!("Added {title}. Press Space to put it in the new path.")
}
/// A template's layer and a filter's picker.
pub fn template_heading(stage: &str) -> String {
    match stage {
        "Input" => "ADD AN INPUT",
        "Display" => "ADD A DISPLAY",
        "Terminal" => "ADD A TERMINAL",
        _ => "ADD A FONT SET",
    }
    .to_owned()
}
/// The line under a part maker's heading, by the template's name.
pub fn template_about(stage: &str) -> &'static str {
    match stage {
        "Input" => "Which inputs should this include?",
        "Display" => "Which displays should this include?",
        "Terminal" => "Where should this path go?",
        _ => "Which font set should the terminal use?",
    }
}
/// A template box's second line, by the template's name.
pub fn template_add(stage: &str) -> String {
    format!("Add {}\u{2026} [Enter]", stage.to_lowercase())
}
/// The small heading above the choices that match by one key.
pub const MATCH_BY: &str = "OR CHOOSE BY";
/// A picker's heading: the choice's label in capitals.
pub fn picker_heading(what: &str) -> String {
    what.to_uppercase()
}
/// The line under a picker's heading, by the match key it lists.
pub fn picker_about(key: &str) -> String {
    let value = match key {
        "path" => return "Each port lists what is plugged into it now.".to_owned(),
        "serial" => "device",
        "id" => "model",
        "bluetooth" => "address",
        "name" => "name",
        "class" => "type",
        "udev" => "property",
        "edid" => "monitor",
        "connector" => "connector",
        "connector-path" => "hub port",
        _ => "card",
    };
    format!("Each {value} lists what matches it now.")
}
pub const PICKER_TYPE: &str = "Other\u{2026}";
pub const PICKER_TYPE_DETAIL: &str = "type a value";
/// The key line while a picker's value is typed.
pub const KEYS_TYPING_VALUE: &str = "type a value  Enter add  Esc cancel";
/// A typed value vtium.conf cannot read, by the picker's match key: what
/// it is not, and what one looks like.
pub fn typed_refused(key: &str) -> String {
    let (what, example) = match key {
        "path" => (
            "USB port is not a valid port path. A port path",
            "pci-0000:00:14.0-usb-0:3.4",
        ),
        "serial" => (
            "device is not a valid serial number. A serial number",
            "046d:c52b:0123ABCD",
        ),
        "id" => ("model is not a valid model. A model", "046d:c31c"),
        "bluetooth" => (
            "Bluetooth device is not a valid address. An address",
            "00:1a:7d:da:71:13",
        ),
        "name" => (
            "device name is not valid. A name, with ? for each space,",
            "Logitech?USB?Receiver",
        ),
        "class" => (
            "device type is not one vtium knows. A device type",
            "keyboard, mouse or touchscreen",
        ),
        "udev" => (
            "udev property is not valid. A udev property",
            "ID_INPUT_JOYSTICK=1",
        ),
        "edid" => (
            "monitor is not a valid EDID. An EDID",
            "AAA:0x0001:LARGE00001",
        ),
        "connector" => ("connector is not valid. A connector", "card1/HDMI-A-1"),
        "connector-path" => ("hub port is not valid. A hub port", "card1/DP-1-1"),
        _ => (
            "graphics card is not valid. A graphics card",
            "pci-0000:03:00.0",
        ),
    };
    format!("This {what} looks like {example}.")
}
/// The special parts' descriptions, each on the part's box and in its
/// maker.
pub const NO_INPUT_DETAIL: &str = "Display only, with no keyboard or mouse";
pub const NO_TERMINAL_DETAIL: &str = "Displays off, inputs ignored";
pub const NEW_TERMINAL_LABEL: &str = "New terminal\u{2026}";
pub const NEW_TERMINAL_DETAIL: &str = "Separate terminal with its own consoles and look";
pub const TURNED_OFF: &str = "turned off";
pub const SET_ASIDE: &str = "set aside";
/// An empty place in NEW PATH.
pub const EMPTY_PLACE: &str = "Choose\u{2026} [Enter]";
/// The empty terminal place in NEW PATH, with the name a new terminal takes.
pub fn new_terminal_named(name: &str) -> String {
    format!("new: {name}")
}
/// A typed match that is empty.
pub const TYPED_EMPTY: &str =
    "The value is empty. Type one, such as id:046d:c31c, then press Enter.";
/// A typed match that holds a mark a rule's match cannot.
pub fn typed_bad_mark(typed: &str) -> String {
    format!("The value {typed} holds # or ->, which a match cannot hold. Type the value alone.")
}
pub const NO_TERMINAL: &str = "Nowhere";
pub const SAME_NONE: &str =
    "These displays are off and these inputs set aside already, so nothing changed.";
pub const NONE_BACK: &str = "The path to Nowhere is in CONFIGURATION.";
pub fn special_picked(title: &str) -> String {
    format!("Picked {title} into the new path.")
}
/// A path with displays and a terminal and no input of its own.
pub const PATH_COMPLETE_DISPLAYS: &str = "This path is complete. Only roaming inputs type into its terminal, while it is active. Press a to add it.";
/// A path with "no input".
pub const KIOSK_COMPLETE: &str =
    "This path is complete. Nothing types into its terminal. Press a to add it.";
/// A path to "no terminal" with nothing in it yet.
pub const NONE_EMPTY: &str = "Pick the displays to turn off or the inputs to set aside.";
/// A path to "no terminal" that can be added.
pub const NONE_COMPLETE: &str =
    "This path is complete. Its displays stay off and its inputs are set aside. Press a to add it.";
pub const ACTIVE_TERMINAL: &str = "Active terminal";
pub const ACTIVE_DETAIL: &str = "Terminal on screen, following you as you switch";
pub const ACTIVE_COMPLETE: &str =
    "This path is complete: its inputs type into the active terminal. Press a to add it.";
pub const ACTIVE_NEEDS_INPUT: &str = "Pick an input to type into the active terminal.";
pub const ACTIVE_NO_DISPLAY: &str = "\u{201c}Active terminal\u{201d} has no display of its own, since it is one of the other terminals at a time. Take the displays out of this path.";
pub const ACTIVE_ABOUT: &str = "\u{201c}Active terminal\u{201d} is the terminal in use. An input in its path roams: it types into the terminal you switch to.";
pub const ACTIVE_PICKED: &str = "This path's inputs will type into the active terminal.";
pub const ACTIVE_UNPICKED: &str = "This path's inputs will type into a terminal of their own.";
pub const SAME_ACTIVE: &str = "These inputs roam already, so nothing changed.";
pub const ACTIVE_BACK: &str = "The roaming inputs are back in CONFIGURATION.";
pub const ACTIVE_REVISED: &str =
    "The roaming inputs are in NEW PATH now. Change them, then press a to add them back.";
pub const REVISED_DROPPED: &str = "Took the path you were revising out of the configuration.";
/// The whole-kind parts' labels and descriptions, each on the part's box
/// and in its maker.
pub const ANY_DISPLAY: &str = "All displays";
pub const EVERY_DISPLAY: &str = "Every display, connected now or later";
pub const ANY_POINTER_TITLE: &str = "All mice and touchpads";
pub const ANY_KEYBOARD_TITLE: &str = "All keyboards";
pub const ANY_INPUT_TITLE: &str = "All input devices";
pub const EVERY_POINTER: &str = "Mice, touchpads and trackballs";
pub const EVERY_KEYBOARD: &str = "Every keyboard, connected now or later";
pub const EVERY_INPUT: &str = "Keyboards, pointers and every other kind of input";
/// The terminal's subflow on its card (ADR-1403 D7).
pub const SUBFLOW_TITLE: &str = "WHAT THIS TERMINAL DRAWS";
pub const FONT_SET_WORD: &str = "Font set";
pub const DEFAULT_LOOK_BOX: &str = "default look (built in)";
pub const NO_DISPLAY_BOX: &str = "no display";
pub fn look_box(name: &str) -> String {
    format!("look {name}")
}
pub const SUBFLOW_REQUIRES: &str = "Filters and requirements: vtium-config does not read a look's vtlook file yet, so this card names the look and shows no filters.";
pub const EVERY_OF_TYPE: &str = "every input of that type";
pub const BY_CONNECTOR: &str = "by its connector";
pub const NOT_PLUGGED: &str = "Not connected";
pub const EVERY_INPUT_ON_PORT: &str = "every input on that port";
pub const DEFAULT_FONT_SET: &str = "default font set";
/// The VTs a terminal runs on, as its box and its card say them (ADR-1403
/// D4, D7): `every VT` for a terminal with no `vt`, else `VT 3` or
/// `VTs 3 and 4`.
pub fn vt_words(vts: &[u16]) -> String {
    match vts {
        [] => EVERY_VT.to_owned(),
        [one] => format!("VT {one}"),
        [rest @ .., last] => {
            let rest: Vec<String> = rest.iter().map(u16::to_string).collect();
            format!("VTs {} and {last}", rest.join(", "))
        }
    }
}
pub const EVERY_VT: &str = "every VT";
pub const VTS_LABEL: &str = "VTs";
pub const DEFAULT_FONT_SET_DETAIL: &str = "Font that vtium.conf sets";
pub const FONT_SET: &str = "font set";
pub const IN_NO_PATH: &str = "in no path";
/// `text` with its first letter capital, to start a sentence.
fn capitalized(text: &str) -> String {
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}
/// A terminal named in a sentence: `the terminal desk`, or for a name
/// vtium-config gave it, `terminal-1` alone.
pub fn terminal(head: &str) -> String {
    if head.starts_with("terminal-") {
        head.to_owned()
    } else {
        format!("the terminal {head}")
    }
}
pub fn in_terminal(head: &str) -> String {
    format!("in {}", terminal(head))
}
pub fn roams_among(n: usize) -> String {
    format!("roams between {n} terminals")
}
pub const ROAMS_SENTENCE: &str = "It roams: it types into the active terminal.";
pub const REVISE_NOTE: &str =
    "Which displays and inputs this path joins change only when you revise it: press e.";
pub const CHOOSE_DISPLAY: &str = "CHOOSE WHICH DISPLAYS THIS PART MATCHES";
pub const CHOOSE_INPUT: &str = "CHOOSE WHICH INPUTS THIS PART MATCHES";
pub const CHOOSE_ABOUT: &str =
    "The choices go from the widest to the narrowest. Enter picks one into the new path.";
pub const NEW_TERMINAL: &str = "THE NEW PATH'S TERMINAL";
pub const AUTOMATIC_SIZE: &str = "automatic";
pub const KEEP_SESSIONS: &str = "keep the sessions running";
pub const END_SESSIONS: &str = "end the sessions";
/// The Login line of a terminal's card: what it sets, its two choices,
/// and what is wrong with a command the person types.
pub const LOGIN_ABOUT: &str = "The program each console of this terminal starts.";
/// The first choice, which leaves the head's `login` unset and names the
/// command vtium runs then.
pub fn login_default(command: &str) -> String {
    format!("Default: {command}")
}
pub const LOGIN_OTHER: &str = "Other\u{2026}";
pub const LOGIN_EMPTY: &str = "The login command is empty. Type the full path of a program, such as /bin/login -p, or choose Default.";
pub const LOGIN_HASH: &str = "The login command holds #, which vtium.conf reads as the start of a comment. Type the command without #.";
pub fn login_not_absolute(program: &str) -> String {
    format!(
        "The login program {program} is not a full path. Type a path that starts with /, such as /bin/login."
    )
}
pub fn login_missing(program: &str) -> String {
    format!(
        "The login program {program} does not exist. Type the full path of a program that is installed."
    )
}
pub fn login_not_runnable(program: &str) -> String {
    format!(
        "The login program {program} cannot run, because it is not an executable file. Type the full path of a program."
    )
}
pub const DISCONNECT: &str = "disconnect it";
pub const CONNECTED: &str = "connected";
pub const EDITOR_NOTE: &str =
    "Space on a part in PARTS connects it to this path. Enter on a part here disconnects it.";
pub const BUILT_IN_DEFAULT: &str = "This is vtium's built-in default: all displays show the terminal console, and all keyboards, mice and touchpads type into it.";
pub const ENTER_TO_TYPE: &str = "Enter types a new name";
pub const KEYS_FLOW: &str = "↑↓←→ move  Space pick  Enter open  a add path  e revise  v view file  Esc clear  i install  o restore  n new  p preview  t trial  PgUp PgDn scroll  q quit";
pub const KEYS_FLOW_EDITOR_CARD: &str = "↑↓ move  ←→ choose  Enter set  Esc back  q quit";
pub const KEYS_FLOW_CARD: &str =
    "↑↓ move  ←→ choose  Enter set  e revise this path  Esc back  q quit";
pub const KEYS_FLOW_INFO: &str = "e revise  Esc back  q quit";
pub const KEYS_FLOW_CHOOSE: &str = "↑↓ move  Enter choose  Esc back  q quit";
pub const REVISE_WHERE: &str = "Highlight a path in CONFIGURATION, then press e to revise it.";
pub const EDITOR_CLEARED: &str = "Cleared the new path.";
pub const EDITOR_BUSY: &str = "The new path already holds parts. Add it with a, or clear it with Esc, before you revise another path.";
pub const IN_DOCUMENT: &str =
    "This part is in a path in the configuration. Press e to revise the path.";
pub const OTHERS_CANNOT: &str =
    "These inputs are neither keyboards nor pointers, so they cannot be part of a path.";
pub const DEFAULT_FONT_PREVIEW: &str = "The default font set is the font vtium.conf sets. The Fonts view shows each font set's preview.";
pub const PREVIEW_PATH_NOT_HERE: &str =
    "Previewing a path needs a vtium console, and this terminal is not one.";
pub const PREVIEW_PATH_LATER: &str = "Previewing a whole path needs the console's control socket, which vtium does not have yet. Enter on a font set shows its preview.";
pub const LIVE_ON: &str = "Live trial is on: in the Fonts view, t tries a font on this console.";
pub const FLOW_COMPLETE: &str = "All devices are in a path";
pub const FLOW_OTHERS_TITLE: &str = "OTHER INPUTS";
pub const FLOW_OTHERS_ABOUT: &str = "These inputs are neither keyboards nor pointers, so none of them types into a terminal. Each line says which rule catches it.";
pub const FLOW_NO_RULE: &str = "no rule names it";
/// The Flow view's actions in the tab bar, long and short.
pub const FLOW_ACTIONS: [(&str, &str); 5] = [
    ("v View file", "View file"),
    ("s Save", "Save"),
    ("i Install", "Install"),
    ("r Revert", "Revert"),
    ("n New", "New"),
];
pub fn flow_other_inputs(n: usize) -> String {
    match n {
        1 => "1 other input".to_owned(),
        n => format!("{n} other inputs"),
    }
}
pub fn path_other_font(head: &str) -> String {
    format!(
        "{} already has these parts with another font set. Press e on it to revise its path.",
        capitalized(&terminal(head))
    )
}
pub fn name_taken(name: &str) -> String {
    format!("The name {name} is already the name of another terminal. Choose another name.")
}
pub fn name_invalid(name: &str) -> String {
    format!(
        "A terminal's name is 1 to 64 letters, digits, dots, dashes, underscores or plus signs, so {name:?} cannot be one. Choose another name."
    )
}
/// A display already turned off, picked into a path with a terminal.
pub fn display_off(display: &str) -> String {
    format!(
        "The display {display} is turned off by the path to Nowhere. Revise that path to use the display here."
    )
}

/// A display that a path on the same VT holds (ADR-1403 D7): the topic,
/// the display, first, then the terminal that holds it and the VT where
/// the two clash, `None` when both run on every VT.
pub fn display_taken(display: &str, head: &str, vt: Option<u16>) -> String {
    let on = match vt {
        Some(vt) => format!("VT {vt}"),
        None => "every VT".to_owned(),
    };
    format!(
        "The display {display} is already {} on {on}. A display can be in one path for each VT.",
        in_terminal(head)
    )
}
pub fn same_path(head: &str) -> String {
    format!(
        "This path is already {}, so nothing changed.",
        in_terminal(head)
    )
}
pub fn path_added(head: &str) -> String {
    format!("Added the path of {}.", terminal(head))
}
pub fn path_revised(head: &str) -> String {
    format!(
        "The path of {} is in NEW PATH now. Change its parts, then press a to add it back.",
        terminal(head)
    )
}
pub fn changed_in_draft(head: &str) -> String {
    format!("Changed {} in the draft.", terminal(head))
}
pub fn piece_out(title: &str) -> String {
    format!("Took {title} out of the new path.")
}
pub fn font_picked(font: &str) -> String {
    format!("The new path uses the font set {font}.")
}
pub fn font_instead(font: &str, old: &str) -> String {
    format!("The new path uses the font set {font} in place of {old}.")
}
pub fn font_default() -> String {
    "The new path uses the default font set.".to_owned()
}
/// What picking a display or an input says: what was picked, with `any
/// pointer` when it came along with `any keyboard`; that it will roam,
/// when it will; and what the new path needs next.
pub fn picked(
    title: &str,
    display: bool,
    new: &crate::tui::paths::Path,
    pointers: bool,
    roams: bool,
) -> String {
    let what = match (display && !title.starts_with("any "), pointers) {
        (true, _) => format!("Picked the display {}.", title.trim_start_matches("the ")),
        (false, true) => format!("Picked {title}, and {ANY_POINTER_TITLE} with it."),
        (false, false) => format!("Picked {title}."),
    };
    let next = match (new.inputs.is_empty(), new.displays.is_empty()) {
        (false, true) => "Now pick a display, or more inputs.",
        (true, false) => "Now pick an input, or more displays.",
        _ => "This path is complete: press a to add it, or pick more parts.",
    };
    match (roams, pointers) {
        (true, true) => format!("{what} They will type into the active terminal. {next}"),
        (true, false) => format!("{what} It will type into the active terminal. {next}"),
        _ => format!("{what} {next}"),
    }
}
/// What waits, as the status line counts it: `1 keyboard and 1 display
/// in no path yet`.
pub fn flow_waiting(keyboards: usize, pointers: usize, displays: usize) -> String {
    let count = |n: usize, one: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {one}s")),
    };
    let parts: Vec<String> = [
        count(keyboards, "keyboard"),
        count(pointers, "pointer"),
        count(displays, "display"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let list = match parts.as_slice() {
        [] => return FLOW_COMPLETE.to_owned(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("{list} in no path yet")
}
/// The policy in a few words, under the status line.
pub fn flow_policy(policy: &str) -> String {
    match policy {
        "closed" => "policy closed: vtium ignores a keyboard that no rule names".to_owned(),
        _ => "policy open: a keyboard that no rule names types into the active terminal".to_owned(),
    }
}
/// A part's detail: its heading, and its label's line.
pub const DISPLAY_PART: &str = "DISPLAY PART";
pub const INPUT_PART: &str = "INPUT PART";
pub const LABEL_FIELD: &str = "Label";
pub const NO_LABEL: &str = "none, press Enter to add one";
pub const LABEL_ABOUT: &str = "shown on the console's on-screen display";
pub const LABEL_TYPING: &str =
    "Enter keeps it. Clear it to remove the label. Esc leaves it as it was.";
pub const LABEL_HASH: &str = "This label starts with #, which vtium.conf reads as the start of a comment. Start the label with another character.";
pub const LABEL_REMOVED: &str =
    "This part has no label now. vtium-config took it out of the draft.";

/// What the status line says once a part's label is written.
pub fn label_set(label: &str) -> String {
    format!("This part's label is now \u{201c}{label}\u{201d}. vtium-config wrote it to the draft.")
}

#[cfg(test)]
mod tests {
    // ADR-1403 D7's fixed word: Install, Revert and New each call the
    // working copy the draft
    #[test]
    fn install_revert_and_new_name_the_draft() {
        assert_eq!(
            super::install_question(3, 1, 0),
            "Install the draft? 3 lines added, 1 removed."
        );
        assert_eq!(
            super::revert_question(3, 1),
            "Throw away the draft's changes? 3 lines added and 1 removed will be lost."
        );
        assert_eq!(
            super::NEW_QUESTION,
            "Start a new configuration? The draft's heads, rules and looks will be lost."
        );
    }

    // ADR-1403 D7 and D8 word Install's question and the refusal for a
    // draft that wires no complete route
    #[test]
    fn install_asks_before_it_installs_a_draft_with_no_complete_path() {
        assert_eq!(
            super::no_route_question(3, 1, 0),
            "This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches. After the install, nobody at this machine may be able to type into vtium. Install it anyway? 3 lines added, 1 removed."
        );
        assert_eq!(
            super::no_route_force(),
            "This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches. To install it anyway, run vtium config install --force."
        );
    }
}
