//! What the Flow tab says: the tab's name and actions, the document of
//! paths, the parts and their cards, the questions of Revert, Restore and
//! New, and the file view.

use super::*;

/// The Flow tab's name in the tab bar.
pub const FLOW: &str = "Flow";

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

/// The Flow view's changes to the draft (ADR-1403 D7, D8).
pub const NAME: &str = "name";

pub const FRIT: &str = "font set";

pub const SIZE: &str = "size";

pub const NO_HOTPLUG: &str = "not watching for new devices";

pub const FORM_RULE_NEEDS: &str = "A new rule needs a match and a target.";

pub const FORM_NAME_NEEDED: &str = "A new head or look needs a name.";

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

pub const NO_HISTORY: &str =
    "Nothing to restore: no install has replaced a vtium.conf yet, so there is no earlier file.";

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

/// The Flow view's sections, boxes and sentences (ADR-1403 D7).
pub const FLOW_PATHS: &str = "CONFIGURATION";

pub const FLOW_NEW_PATH: &str = "NEW PATH";

pub const FLOW_PIECES: &str = "PARTS";

pub const FLOW_DISPLAYS: &str = "DISPLAYS";

pub const FLOW_TERMINALS: &str = "TERMINALS";

pub const FLOW_INPUTS: &str = "INPUTS";

pub const FLOW_LOOKS: &str = "LOOKS";

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

/// The font set of a look that names none, which vtium.conf's top-level
/// `font-name` sets.
pub const FONT_SET_FROM_CONF: &str = "from vtium.conf";

pub const DEFAULT_LOOK_BOX: &str = "default look (built in)";

pub const NO_DISPLAY_BOX: &str = "no display";

pub fn look_box(name: &str) -> String {
    format!("look {name}")
}

/// A look that a head's look line names and the draft does not hold, as
/// the terminal's card, its subflow and its box in a path show it.
pub fn missing_look(name: &str) -> String {
    format!("{name} (missing)")
}

/// What the card's Look line says while its cursor is on a terminal whose
/// look the draft does not hold.
pub fn look_missing_about(name: &str) -> String {
    format!(
        "The draft has no look named {name}. Until you choose another look, vtium draws this terminal with the settings at the top of vtium.conf."
    )
}

pub const SUBFLOW_REQUIRES: &str = "Filters and requirements: vtium-config does not read a look's vtlook file yet, so this card names the look and shows no filters.";

pub const EVERY_OF_TYPE: &str = "every input of that type";

pub const BY_CONNECTOR: &str = "by its connector";

pub const NOT_PLUGGED: &str = "Not connected";

pub const EVERY_INPUT_ON_PORT: &str = "every input on that port";

/// The default look, as the Flow view names it: the look of a head that
/// names none.
pub const DEFAULT_LOOK_PART: &str = "default look";

/// A look's words on its terminal's box and in PARTS.
pub fn look_words(look: &str) -> String {
    format!("look {look}")
}

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

pub const DEFAULT_LOOK_DETAIL: &str = "Fonts and settings that vtium.conf sets";

pub const IN_NO_PATH: &str = "in no path";

/// `text` with its first letter capital, to start a sentence.
fn capitalized(text: &str) -> String {
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
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

pub const KEEP_SESSIONS: &str = "keep the sessions running";

pub const END_SESSIONS: &str = "end the sessions";

/// The Switch on Enter line of a terminal's card, the head key
/// `switch-on-enter` (ADR-1401, "Enter on a head's own keyboard"): its
/// label, its two choices, what it does, and why it does nothing.
pub const SWITCH_LABEL: &str = "Switch on Enter";

pub const SWITCH_OFF: &str = "off";

pub const SWITCH_ON: &str = "on";

pub const SWITCH_ABOUT: &str = "Enter on this terminal's own keyboard switches to its VT, from a VT where this terminal does not run.";

pub const SWITCH_CLOSED: &str =
    "It has no effect now, because the policy is closed. It works only with policy = open.";

pub const SWITCH_ISOLATED: &str =
    "It has no effect now, because this terminal is isolated, so its keyboards switch no VT.";

/// On an added path's card, where e revises the path, and in NEW PATH,
/// whose card has the line VTs.
pub const SWITCH_EVERY_VT: &str = "It has no effect now, because this terminal runs on every VT. Press e and choose a VT on the line VTs.";

pub const SWITCH_EVERY_VT_NEW: &str = "It has no effect now, because this terminal runs on every VT. Choose a VT on the line VTs above.";

pub const SWITCH_PAST_12: &str = "It has no effect now, because Enter switches only to VTs 1 to 12, and this terminal runs on none of them.";

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

pub const REVISE_WHERE: &str = "Highlight a path in CONFIGURATION, then press e to revise it.";

pub const EDITOR_CLEARED: &str = "Cleared the new path.";

pub const EDITOR_BUSY: &str = "The new path already holds parts. Add it with a, or clear it with Esc, before you revise another path.";

pub const IN_DOCUMENT: &str =
    "This part is in a path in the configuration. Press e to revise the path.";

pub const OTHERS_CANNOT: &str =
    "These inputs are neither keyboards nor pointers, so they cannot be part of a path.";

pub const DEFAULT_FONT_PREVIEW: &str = "This look draws with the font set vtium.conf sets. The Looks tab shows each font set's preview.";

pub const PREVIEW_PATH_NOT_HERE: &str =
    "Previewing a path needs a vtium console, and this terminal is not one.";

pub const PREVIEW_PATH_LATER: &str = "Previewing a whole path needs the console's control socket, which vtium does not have yet. Enter on a font set shows its preview.";

pub const LIVE_ON: &str = "Live trial is on: in the Looks tab, t tries a font set on this console.";

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

pub fn path_other_look(head: &str) -> String {
    format!(
        "{} already has these parts with another look. Press e on it to revise its path.",
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

/// Space on a display that vtium cannot match as itself: it reports no
/// EDID, and without a udev daemon vtium knows no connector to match.
pub fn display_no_match(display: &str) -> String {
    format!(
        "The display {display} reports no EDID, and without a udev daemon vtium has no other way to tell it apart, so vtium-config cannot pick it as itself. To show a terminal on it, open the Display template, choose All displays, and press Space on it."
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

pub fn look_picked(look: &str) -> String {
    format!("The new path uses the look {look}.")
}

pub fn look_instead(look: &str, old: &str) -> String {
    format!("The new path uses the look {look} in place of {old}.")
}

pub fn look_default() -> String {
    "The new path uses the default look.".to_owned()
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

/// Esc in the Flow view while NEW PATH is empty.
pub const NOTHING_TO_CLEAR: &str = "NEW PATH is empty, so Esc has nothing to clear.";

/// Space on NEW PATH's terminal box.
pub const NEW_TERMINAL_SPACE: &str =
    "The new terminal is already in NEW PATH. Enter opens its card.";
