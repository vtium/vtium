//! The words of the Flow tab's editing: the terminal's card, the part
//! makers and pickers, labels, and what adding and revising a path say.
//! Each message is one line at 80 columns, and each key it names is in
//! brackets.
//!
//! References:
//! - ADR-1403 D7: the sentences that adding, revising and the part makers
//!   say.
//! - ADR-1403 D11, "The action bar": keys written in brackets.

use vtium_config_widgets::canvas::fit;

use crate::words::terminal;

/// A name or a value as a message quotes it, cut so the line stays one
/// line at 80 columns.
fn quoted(text: &str) -> String {
    fit(text, 18)
}

/// `text` with its first letter a capital.
fn capital(text: &str) -> String {
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

// adding, clearing and revising a path
pub const REVISE_WHERE: &str = "Highlight a path in CONFIGURATION, then [e] revises it.";
pub const EDITOR_BUSY: &str =
    "NEW PATH holds parts: [a] adds them or [Esc] clears them, then [e] again.";
pub const ACTIVE_REVISED: &str =
    "Revising the roaming inputs: change them, then [a] adds them back.";
pub const NONE_REVISED: &str = "Revising the path to Nowhere: change it, then [a] adds it back.";
pub const REVISED_DROPPED: &str = "Took the path you were revising out of the configuration.";
pub const SAME_ACTIVE: &str = "These inputs roam already, so nothing changed.";
pub const SAME_NONE: &str = "These displays are off and these inputs aside already: no change.";
pub const ACTIVE_BACK: &str = "The roaming inputs are in CONFIGURATION.";
pub const NONE_BACK: &str = "The path to Nowhere is in CONFIGURATION.";

/// What `e` says once the path of the terminal `head` is in NEW PATH.
pub fn path_revised(head: &str) -> String {
    format!(
        "Revising {}'s path: change a part, then [a] adds it back.",
        quoted(head)
    )
}

/// What `a` says once it added the path of `head`.
pub fn path_added(head: &str) -> String {
    format!("Added the path of {}.", terminal(&quoted(head)))
}

/// What `a` says when the configuration holds the path already.
pub fn same_path(head: &str) -> String {
    format!(
        "{} has this path already: nothing changed.",
        capital(&terminal(&quoted(head)))
    )
}

/// Why `a` refuses a path whose parts another path holds with another
/// look.
pub fn path_other_look(head: &str) -> String {
    format!(
        "{} holds them with another look: [e] revises it.",
        capital(&terminal(&quoted(head)))
    )
}

/// Why a display in the path to Nowhere cannot join another path.
pub fn display_off(display: &str) -> String {
    format!(
        "{} is off in the path to Nowhere: revise that path first.",
        quoted(display)
    )
}

/// Why a terminal cannot take the name `name`, taken by another.
pub fn name_taken(name: &str) -> String {
    format!(
        "The name {} is another terminal's: choose another name.",
        quoted(name)
    )
}

/// Why no terminal can take the name `name`.
pub fn name_invalid(name: &str) -> String {
    format!(
        "{:?} cannot name a terminal: use letters, digits, . _ + or -.",
        quoted(name)
    )
}

/// The name of the change that adds the path of `head`, which Undo and
/// the File menu name.
pub fn adding(head: &str) -> String {
    format!("adding the path of {}", terminal(head))
}

/// The name of the change that takes a revised path out.
pub const REMOVING: &str = "removing the path being revised";

// the terminal's card
pub const NAME: &str = "Name";
pub const LOOK: &str = "Look";
pub const VTS: &str = "VTs";
pub const CONSOLES: &str = "Consoles";
pub const LOGIN: &str = "Login";
pub const WHEN_UNPLUGGED: &str = "When its screen is unplugged";
pub const SWITCH: &str = "Switch on Enter";
pub const DISPLAY: &str = "Display";
pub const INPUT: &str = "Input";
pub const NEW_TERMINAL: &str = "THE NEW PATH'S TERMINAL";
pub const TERMINAL: &str = "TERMINAL";
pub const KEEP_SESSIONS: &str = "keep the sessions running";
pub const END_SESSIONS: &str = "end the sessions";
pub const SWITCH_OFF: &str = "off";
pub const SWITCH_ON: &str = "on";
pub const LOGIN_OTHER: &str = "Other\u{2026}";
pub const CONNECTED: &str = "connected";
pub const DEFAULT_LOOK: &str = "default look";
pub const NEW_PATH_NOTE: &str =
    "[Space] on a part in PARTS joins it; [Enter] on a part here takes it out.";
pub const VTS_READ_ONLY: &str = "The VTs change only when you revise the path: [e].";
pub const NAME_HINT: &str = "[Enter] types a new name";
pub const TYPING_HINT: &str = "[Enter] keeps it, [Esc] cancels";
pub const LOGIN_ABOUT: &str = "Login: the program each console of this terminal starts.";
pub const SWITCH_ABOUT: &str = "Switch on Enter: [Enter] on its own keyboard switches to its VT.";
pub const SWITCH_CLOSED: &str = "No effect while the policy is closed.";
pub const SWITCH_ISOLATED: &str = "No effect: this terminal is isolated, so it switches no VT.";
pub const SWITCH_EVERY_VT: &str = "No effect while it runs on every VT: press [e] and choose a VT.";
pub const SWITCH_EVERY_VT_NEW: &str = "No effect while it runs on every VT: choose a VT above.";
pub const SWITCH_PAST_12: &str = "No effect: [Enter] switches only to VTs 1 to 12.";
pub const LOGIN_EMPTY: &str = "The login command is empty: type a program's full path.";
pub const LOGIN_HASH: &str = "The login command holds #, which vtium.conf reads as a comment.";

/// The first choice of the Login line, which leaves the head's `login`
/// unset, naming the command vtium runs then.
pub fn login_default(command: &str) -> String {
    format!("Default: {command}")
}

/// Why the login program `program` cannot run: not a full path.
pub fn login_not_absolute(program: &str) -> String {
    format!(
        "The login program {} is not a full path: start it with /.",
        quoted(program)
    )
}

/// Why the login program `program` cannot run: it does not exist.
pub fn login_missing(program: &str) -> String {
    format!(
        "The login program {} does not exist on this machine.",
        quoted(program)
    )
}

/// Why the login program `program` cannot run: not an executable file.
pub fn login_not_runnable(program: &str) -> String {
    format!(
        "The login program {} is not an executable file.",
        quoted(program)
    )
}

/// The Look line of a terminal that names a look the draft does not
/// hold.
pub fn missing_look(name: &str) -> String {
    format!("{name} (not in the draft)")
}

/// What the Look line says while its look is not in the draft.
pub fn missing_look_about(name: &str) -> String {
    format!(
        "The draft has no look {}: vtium draws the top of vtium.conf.",
        quoted(name)
    )
}

/// What a card's line says once it is set, and the name of that change.
pub fn card_set(line: &str, value: &str, head: &str) -> String {
    format!(
        "{line} is now {} for {}.",
        quoted(value),
        terminal(&quoted(head))
    )
}

/// The name of the change a card's line makes.
pub fn setting(line: &str, value: &str, head: &str) -> String {
    format!("setting {line} to {value} for {}", terminal(head))
}

// what the terminal draws
pub const SUBFLOW_TITLE: &str = "WHAT THIS TERMINAL DRAWS";
pub const FONT_SET_FROM_CONF: &str = "Font set from vtium.conf";
pub const DEFAULT_LOOK_BOX: &str = "default look (built in)";
pub const NO_DISPLAY_BOX: &str = "no display";
pub const NO_VTLOOK: &str = "Effects: No filter, the built-in vtlook, which changes no color.";

/// The font set box of the subflow.
pub fn font_set_box(frit: &str) -> String {
    format!("Font set {frit}")
}

/// The look box of the subflow.
pub fn look_box(name: &str) -> String {
    format!("look {name}")
}

/// The effects line of the subflow, for the vtlook `name`: read, broken,
/// or not on this machine.
pub fn vtlook_line(name: &str, read: Option<bool>) -> String {
    match read {
        Some(true) => format!("Effects: the vtlook {name}. The Effects tab shows its filters."),
        Some(false) => {
            format!("Effects: the vtlook {name} has an error, so vtium draws the built-in one.")
        }
        None => format!("Effects: no vtlook {name} here, so vtium draws the built-in one."),
    }
}

// the part makers
pub const OR_CHOOSE_BY: &str = "OR CHOOSE BY";
pub const NEW_TERMINAL_CHOICE: &str = "New terminal\u{2026}";
pub const OTHER: &str = "Other\u{2026}";
pub const TYPE_A_VALUE: &str = "type a value";
pub const ADVANCED: &str = "(advanced)";
pub const NO_VALUES: &str = "No device connected now gives a value: [Enter] on Other… types one.";
pub const TYPED_EMPTY: &str = "The value is empty: type one, such as id:046d:c31c.";

/// A maker's heading and its question, for the stage named `stage`.
pub fn maker_heading(stage: &str) -> (String, &'static str) {
    match stage {
        "Display" => (
            "ADD A DISPLAY".to_owned(),
            "Which displays should this include?",
        ),
        "Input" => (
            "ADD AN INPUT".to_owned(),
            "Which inputs should this include?",
        ),
        "Terminal" => ("ADD A TERMINAL".to_owned(), "Where should this path go?"),
        other => (
            format!("ADD A {}", other.to_uppercase()),
            "Which look should the terminal use?",
        ),
    }
}

/// A picker's line under its heading.
pub fn picker_about(key: &str, label: &str) -> String {
    if key == "path" {
        return "each port lists what is plugged into it now".to_owned();
    }
    let value = label.trim_start_matches("This ").to_lowercase();
    format!("each {value} lists what matches it now")
}

/// What making a part says, as `Added DP-9: [Space] puts it in NEW
/// PATH.`
pub fn part_made(title: &str) -> String {
    format!("Added {}: [Space] puts it in NEW PATH.", quoted(title))
}

/// A typed value that holds a mark a rule's match cannot.
pub fn typed_bad_mark(typed: &str) -> String {
    format!(
        "The value {} holds # or ->: type the value alone.",
        quoted(typed)
    )
}

/// A typed value vtium.conf cannot read, by the picker's match key: what
/// it is not, and what one looks like.
pub fn typed_refused(key: &str) -> String {
    let (what, example) = match key {
        "path" => ("USB port is not a port path", "pci-0000:00:14.0-usb-0:3.4"),
        "serial" => ("serial number is not valid", "046d:c52b:0123ABCD"),
        "id" => ("model is not valid", "046d:c31c"),
        "bluetooth" => ("Bluetooth address is not valid", "00:1a:7d:da:71:13"),
        "name" => ("device name is not valid", "Logitech?USB?Receiver"),
        "class" => ("device type is not one vtium knows", "touchscreen"),
        "udev" => ("udev property is not valid", "ID_INPUT_JOYSTICK=1"),
        "edid" => ("EDID is not valid", "AAA:0x0001:LARGE00001"),
        "connector" => ("connector is not valid", "card1/HDMI-A-1"),
        "connector-path" => ("hub port is not valid", "card1/DP-1-1"),
        _ => ("graphics card is not valid", "pci-0000:03:00.0"),
    };
    format!("This {what}: one looks like {example}.")
}

// labels
pub const LABEL_HINT: &str = "[Enter] keeps it; blank removes it";
pub const LABEL_REMOVED: &str = "This part has no label now.";
pub const LABEL_HASH: &str = "A label cannot start with #, which vtium.conf reads as a comment.";

/// What setting a label says.
pub fn label_set(label: &str) -> String {
    format!("The label is now \u{201c}{}\u{201d}.", quoted(label))
}

/// The name of the change that labels the part `part`.
pub fn labelling(part: &str, label: &str) -> String {
    if label.is_empty() {
        format!("removing the label of {part}")
    } else {
        format!("labelling {part} \u{201c}{label}\u{201d}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_config_widgets::canvas::cells;

    /// Every message is one line at 80 columns: the frame draws it from
    /// the viewer's second column. Names that may be long are cut.
    #[test]
    fn each_message_fits_one_line_at_80_columns() {
        let long = "Logitech USB Receiver Keyboard Number 12";
        let mut lines: Vec<String> = [
            REVISE_WHERE,
            EDITOR_BUSY,
            ACTIVE_REVISED,
            NONE_REVISED,
            REVISED_DROPPED,
            SAME_ACTIVE,
            SAME_NONE,
            ACTIVE_BACK,
            NONE_BACK,
            VTS_READ_ONLY,
            LOGIN_ABOUT,
            SWITCH_ABOUT,
            SWITCH_EVERY_VT,
            LOGIN_EMPTY,
            LOGIN_HASH,
            NO_VALUES,
            TYPED_EMPTY,
            LABEL_REMOVED,
            LABEL_HASH,
            NO_VTLOOK,
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        lines.extend([
            path_revised(long),
            path_added(long),
            same_path(long),
            path_other_look(long),
            display_off(long),
            name_taken(long),
            name_invalid(long),
            login_not_absolute(long),
            login_missing(long),
            login_not_runnable(long),
            missing_look_about(long),
            card_set(SWITCH, long, long),
            part_made(long),
            typed_bad_mark(long),
            label_set(long),
            vtlook_line("crt-amber", Some(false)),
            vtlook_line("crt-amber", None),
        ]);
        for key in [
            "path",
            "serial",
            "id",
            "bluetooth",
            "name",
            "class",
            "udev",
            "edid",
            "connector",
            "connector-path",
            "card",
        ] {
            lines.push(typed_refused(key));
        }
        let long: Vec<String> = lines
            .into_iter()
            .filter(|l| cells(l) > 79)
            .map(|l| format!("{} cells: {l}", cells(&l)))
            .collect();
        assert!(long.is_empty(), "{long:#?}");
    }
}
