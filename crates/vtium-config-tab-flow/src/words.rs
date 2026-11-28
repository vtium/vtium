//! The words the Flow tab shows. Each fact the page shows takes one line,
//! and a key it names for pressing is written in brackets. A sentence that
//! says more than a line holds keeps its full text for F1.
//!
//! References:
//! - crates/AGENTS.md, strings on screen, rules 7 and 8: one line where a
//!   paragraph was, and keys in brackets.
//! - ADR-1403 D7: the page's headings, the words of the parts and of
//!   NEW PATH's status, and the part makers' templates.

/// The tab's name on the top row.
pub const FLOW: &str = "Flow";

pub const CONFIGURATION: &str = "CONFIGURATION";
pub const NEW_PATH: &str = "NEW PATH";
pub const PARTS: &str = "PARTS";
pub const DISPLAYS: &str = "DISPLAYS";
pub const TERMINALS: &str = "TERMINALS";
pub const INPUTS: &str = "INPUTS";
pub const LOOKS: &str = "LOOKS";

/// The policy, beside CONFIGURATION's heading.
pub fn policy(policy: &str) -> &'static str {
    match policy {
        "closed" => "policy closed: vtium ignores a keyboard no rule names",
        _ => "policy open: a keyboard no rule names types into the active terminal",
    }
}

pub const NO_PATHS: &str =
    "No paths yet. Pick an input and a display in PARTS, then [a] adds the path.";

pub const BUILT_IN_DEFAULT: &str =
    "vtium's built-in default: all displays show the console, and every input types into it.";

/// The rules that belong to no path.
pub fn unfit_rules(n: usize) -> String {
    match n {
        1 => "1 rule belongs to no path. [v] shows it in the file.".to_owned(),
        n => format!("{n} rules belong to no path. [v] shows them in the file."),
    }
}

pub const ROAMING: &str = "ROAMING";
pub const ROAMING_ABOUT: &str = "These inputs type into the active terminal.";
pub const ROAMING_STUB: &str = "roaming inputs";
pub const ROAMING_WORD: &str = "roaming";

/// What waits, under CONFIGURATION's paths: `1 keyboard and 2 pointers are
/// in no path yet.`
pub fn waiting(keyboards: usize, pointers: usize, displays: usize) -> String {
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
    let one = keyboards.saturating_add(pointers).saturating_add(displays) == 1;
    let list = match parts.as_slice() {
        [] => return EVERY_DEVICE_PLACED.to_owned(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    let verb = if one { "is" } else { "are" };
    format!("{list} {verb} in no path yet.")
}

pub const EVERY_DEVICE_PLACED: &str = "Every connected device is in a path.";

pub const NO_HOTPLUG: &str = "vtium-config is not watching for new devices: [d] reads them again.";

pub const NO_TERMINAL: &str = "Nowhere";
pub const NO_TERMINAL_DETAIL: &str = "displays off, inputs ignored";
pub const ACTIVE_TERMINAL: &str = "Active terminal";
pub const ACTIVE_DETAIL: &str = "the terminal on screen";
pub const NO_INPUT: &str = "No input";
pub const NO_INPUT_DETAIL: &str = "display only, no keyboard or mouse";
pub const NEW_TERMINAL_DETAIL: &str = "a terminal of its own";

pub const ANY_DISPLAY: &str = "All displays";
pub const EVERY_DISPLAY: &str = "every display, connected now or later";
pub const ANY_POINTER_TITLE: &str = "All mice and touchpads";
pub const EVERY_POINTER: &str = "mice, touchpads, trackballs";
pub const ANY_KEYBOARD_TITLE: &str = "All keyboards";
pub const EVERY_KEYBOARD: &str = "every keyboard, connected now or later";
pub const ANY_INPUT_TITLE: &str = "All input devices";
pub const EVERY_INPUT: &str = "keyboards, pointers and every other input";
pub const EVERY_OF_TYPE: &str = "every input of that type";
pub const EVERY_INPUT_ON_PORT: &str = "every input on that port";
pub const BY_CONNECTOR: &str = "by its connector";
pub const NOT_PLUGGED: &str = "not connected";
pub const TURNED_OFF: &str = "turned off";
pub const SET_ASIDE: &str = "set aside";
pub const IN_NO_PATH: &str = "in no path";
pub const EMPTY_PLACE: &str = "Choose… [Enter]";
/// An empty place's second line where its box is too narrow for
/// [`EMPTY_PLACE`], so the key keeps its brackets whole.
pub const EMPTY_PLACE_SHORT: &str = "[Enter]";
pub const LOOK: &str = "look";
pub const DEFAULT_LOOK_PART: &str = "default look";
pub const DEFAULT_LOOK_DETAIL: &str = "what vtium.conf sets";

/// The second line of a template's box.
pub fn template_add(stage: &str) -> String {
    format!("Add {}… [Enter]", stage.to_lowercase())
}

/// The second line of NEW PATH's empty terminal: the name it will take.
pub fn new_terminal_named(name: &str) -> String {
    format!("new: {name}")
}

/// A look on a terminal's second line and on a look's box.
pub fn look_words(look: &str) -> String {
    format!("look {look}")
}

/// A look a terminal names that the draft does not hold.
pub fn missing_look(name: &str) -> String {
    format!("{name} (missing)")
}

pub const EVERY_VT: &str = "every VT";

/// The VTs a terminal runs on: `VT 3`, `VTs 3 and 4`, `every VT`.
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

/// A terminal as a sentence names it: `the terminal desk`, or a new
/// terminal by its name alone, `terminal-1`.
pub fn terminal(head: &str) -> String {
    if head.starts_with("terminal-") {
        head.to_owned()
    } else {
        format!("the terminal {head}")
    }
}

/// Where a part is: `in the terminal desk`.
pub fn in_terminal(head: &str) -> String {
    format!("in {}", terminal(head))
}

/// An input that types into several terminals.
pub fn roams_among(n: usize) -> String {
    format!("roams between {n} terminals")
}

/// The box of the other inputs.
pub fn other_inputs(n: usize) -> String {
    match n {
        1 => "1 other input".to_owned(),
        n => format!("{n} other inputs"),
    }
}

pub const OTHERS_TITLE: &str = "OTHER INPUTS";
pub const OTHERS_ABOUT: &str = "neither keyboards nor pointers, so none types into a terminal";
pub const OTHERS_CRUMB: &str = "other inputs";
pub const NO_RULE: &str = "no rule names it";

/// The rows the other inputs' list leaves out, at its foot.
pub fn more_others(n: usize) -> String {
    format!("… and {n} more")
}

/// The rule that catches one of the other inputs.
pub fn caught_by(rule: usize, selector: &str, target: &str) -> String {
    format!("rule {rule}: {selector} -> {target}")
}

// A part's detail.
pub const DISPLAY_PART: &str = "DISPLAY PART";
pub const INPUT_PART: &str = "INPUT PART";
pub const LABEL_FIELD: &str = "Label";
pub const NO_LABEL: &str = "none, [Enter] adds one";
/// What a display's label is for, and what an input's is.
pub const DISPLAY_LABEL_ABOUT: &str = "shown on the display when you identify the displays";
pub const INPUT_LABEL_ABOUT: &str = "shown in vtium-config";
pub const CHOOSE_DISPLAY: &str = "CHOOSE WHICH DISPLAYS THIS PART MATCHES";
pub const CHOOSE_INPUT: &str = "CHOOSE WHICH INPUTS THIS PART MATCHES";
pub const CHOOSE_ABOUT: &str = "from the widest to the narrowest; [Enter] picks one";
pub const NO_CHOICES: &str = "No narrower match: no device of this kind is connected.";

// A display or an input of a path, read-only.
pub const DISPLAY: &str = "DISPLAY";
pub const INPUT: &str = "INPUT";
pub const MATCHED_BY: &str = "Matched by";
pub const WHERE_IT_GOES: &str = "Where it goes";
pub const ROAMS: &str = "roams: it types into the active terminal";
pub const REVISE_NOTE: &str = "What a path joins changes only when you revise it: [e].";
pub const REVISE_ROAMING_NOTE: &str =
    "[e] takes the roaming inputs into NEW PATH, and [a] adds them back.";

// Messages: each one line at 80 columns. F1 opens a longer one's text.

pub const NOTHING_TO_CLEAR: &str = "NEW PATH is empty, so [Esc] has nothing to clear.";
pub const NEW_PATH_CLEARED: &str = "Cleared NEW PATH.";
pub const EMPTY_SPACE: &str = "This place in NEW PATH is empty: [Enter] chooses a part for it.";

/// Space on a template.
pub fn template_space(stage: &str) -> String {
    format!(
        "The {stage} template adds {}s: [Enter] adds one.",
        stage.to_lowercase()
    )
}

pub const NO_INPUT_ABOUT: &str =
    "No input: no keyboard or mouse reaches the terminal, not even a roaming one.";
pub const NO_INPUT_MORE: &str = "\u{201c}No input\u{201d} means no keyboard or mouse reaches the terminal, not even a roaming one. In vtium.conf, the terminal's section gets isolated = yes.";
pub const NO_TERMINAL_ABOUT: &str = "Nowhere: its displays stay off and its inputs are ignored.";
pub const ACTIVE_ABOUT: &str =
    "Active terminal: an input in its path types into the terminal on screen.";
pub const ACTIVE_MORE: &str = "\u{201c}Active terminal\u{201d} is the terminal in use. An input in its path roams: it types into the terminal you switch to.";

/// Enter on a new terminal in PARTS.
pub fn new_terminal_about(name: &str) -> String {
    format!("{name} is a new terminal: [Space] puts it in NEW PATH.")
}

pub const OTHERS_CANNOT: &str =
    "These inputs are neither keyboards nor pointers, so no path takes them.";
pub const IN_DOCUMENT: &str = "This part is in a path of the configuration: [e] revises the path.";
pub const IN_ROAMING: &str = "This input roams: [e] revises the roaming inputs.";
pub const NEW_TERMINAL_SPACE: &str =
    "The new terminal is in NEW PATH already: [Enter] opens its card.";
pub const ACTIVE_NO_DISPLAY: &str =
    "Active terminal has no display of its own: take the displays out first.";
pub const ACTIVE_NO_DISPLAY_MORE: &str = "\u{201c}Active terminal\u{201d} has no display of its own, since it is one of the other terminals at a time. Take the displays out of this path.";
pub const NO_INPUT_NEEDS_TERMINAL: &str =
    "No input needs a terminal of its own: take out Active terminal or Nowhere.";
pub const DEFAULT_FONT_PREVIEW: &str = "This look draws with the font set vtium.conf sets.";

/// A display picked on a VT where a path holds it already, in a line, and
/// the full sentence behind it.
pub fn display_taken(display: &str, head: &str, vt: Option<u16>) -> (String, String) {
    let on = vt.map_or_else(|| EVERY_VT.to_owned(), |vt| format!("VT {vt}"));
    let held = in_terminal(head);
    (
        format!("{display} is already {held} on {on}: one path for each VT."),
        format!(
            "The display {display} is already {held} on {on}. A display can be in one path for each VT."
        ),
    )
}

/// A display with no key vtium can match it by.
pub fn display_no_match(display: &str) -> String {
    format!("vtium cannot tell {display} apart: pick All displays in its place.")
}

/// The full text behind [`display_no_match`].
pub fn display_no_match_more(display: &str) -> String {
    format!(
        "The display {display} reports no EDID, and without a udev daemon vtium has no other way to tell it apart, so vtium-config cannot pick it as itself. To show a terminal on it, open the Display template, choose All displays, and press Space on it."
    )
}

/// A part taken out of NEW PATH.
pub fn piece_out(title: &str) -> String {
    format!("Took {title} out of NEW PATH.")
}

/// A special part picked into NEW PATH.
pub fn special_picked(title: &str) -> String {
    format!("Picked {title} into NEW PATH.")
}

pub const ACTIVE_PICKED: &str = "NEW PATH's inputs will type into the active terminal.";
pub const ACTIVE_UNPICKED: &str = "NEW PATH's inputs will type into a terminal of their own.";

/// A display or an input picked into a path to Nowhere.
pub fn picked_aside(title: &str, display: bool) -> String {
    if display {
        format!("Picked {title}: it will stay off.")
    } else {
        format!("Picked {title}: it will be set aside.")
    }
}

pub fn look_picked(look: &str) -> String {
    format!("NEW PATH uses the look {look}.")
}

pub fn look_instead(look: &str, old: &str) -> String {
    format!("NEW PATH uses the look {look} in place of {old}.")
}

pub const LOOK_DEFAULT: &str = "NEW PATH uses the default look.";

/// A display or an input picked into NEW PATH: what was picked, with All
/// mice and touchpads when it came along with All keyboards, whether it
/// will roam, and what the path needs next. The line is short, and the
/// full sentence is what F1 opens.
pub fn picked(
    title: &str,
    display: bool,
    next: Next,
    pointers: bool,
    roams: bool,
) -> (String, String) {
    let what = match (display, pointers) {
        (true, _) => format!("Picked the display {}.", title.trim_start_matches("the ")),
        (false, true) => format!("Picked {title}, and {ANY_POINTER_TITLE} with it."),
        (false, false) => format!("Picked {title}."),
    };
    let (short, long) = match next {
        Next::Display => ("Now pick a display.", "Now pick a display, or more inputs."),
        Next::Input => ("Now pick an input.", "Now pick an input, or more displays."),
        Next::Complete => (
            "[a] adds the path.",
            "This path is complete: [a] adds it, or pick more parts.",
        ),
    };
    let (roam, roam_long) = match (roams, pointers) {
        (true, true) => (
            " They will roam.",
            " They will type into the active terminal.",
        ),
        (true, false) => (" It will roam.", " It will type into the active terminal."),
        _ => ("", ""),
    };
    (
        format!("{what}{roam} {short}"),
        format!("{what}{roam_long} {long}"),
    )
}

/// What a path in NEW PATH needs next, after a display or an input is
/// picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    Display,
    Input,
    Complete,
}

// NEW PATH's status, beside its heading.
pub const PATH_EMPTY: &str = "Pick an input and a display, then [a] adds the path.";
pub const PATH_NEEDS_DISPLAY: &str = "Add a display to finish this path.";
pub const PATH_COMPLETE: &str = "This path is complete: [a] adds it.";
pub const PATH_COMPLETE_DISPLAYS: &str =
    "Complete: only roaming inputs type into it, while it is active. [a] adds it.";
pub const KIOSK_COMPLETE: &str = "Complete: nothing types into its terminal. [a] adds it.";
pub const NONE_EMPTY: &str = "Pick the displays to turn off or the inputs to set aside.";
pub const NONE_COMPLETE: &str =
    "Complete: its displays stay off and its inputs are set aside. [a] adds it.";
pub const ACTIVE_COMPLETE: &str =
    "Complete: its inputs type into the active terminal. [a] adds it.";
pub const ACTIVE_NEEDS_INPUT: &str = "Pick an input to type into the active terminal.";

// `t` and `p`, which say what the terminal allows.
pub const LIVE_ON: &str =
    "Live trial is on: [t] in the Looks tab tries a font set on this console.";
pub const OPS_OFF: &str = "Live trial is off: this vtium console has allow-font-ops off.";
pub const OPS_OFF_MORE: &str = "Live trial is off: this vtium console has allow-font-ops turned off, so t cannot try a font on it. To turn it on, change allow-font-ops in the Settings tab, save, and restart vtium and vtium-config.";
pub const NOT_VTIUM: &str = "Live trial is off: this terminal is not a vtium console.";
pub const TRIAL_ASKING: &str =
    "Live trial is not ready: vtium-config is still asking what this terminal is.";
pub const PREVIEW_NOT_HERE: &str =
    "Previewing a path needs a vtium console, and this terminal is not one.";
pub const PREVIEW_ASKING: &str =
    "Previewing a path needs a vtium console: vtium-config is still asking.";
pub const PREVIEW_LATER: &str =
    "Previewing a whole path needs the console's control socket, which vtium lacks.";
pub const PREVIEW_LATER_MORE: &str = "Previewing a whole path needs the console's control socket, which vtium does not have yet. Enter on a font set shows its preview.";

/// The place Find gives a part: the section of the page it is in.
pub fn place(section: &str) -> String {
    format!("{FLOW} › {section}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_config_widgets::canvas::cells;

    /// Every message is one line at 80 columns: the frame draws it from
    /// the viewer's second column.
    #[test]
    fn each_message_fits_one_line_at_80_columns() {
        let picked = picked(
            "the AT Translated Set 2 keyboard",
            false,
            Next::Complete,
            false,
            true,
        );
        let taken = display_taken("HDMI-A-1", "console", Some(12));
        for line in [
            NO_PATHS,
            NO_HOTPLUG,
            NOTHING_TO_CLEAR,
            NEW_PATH_CLEARED,
            EMPTY_SPACE,
            NO_INPUT_ABOUT,
            NO_TERMINAL_ABOUT,
            ACTIVE_ABOUT,
            OTHERS_CANNOT,
            IN_DOCUMENT,
            IN_ROAMING,
            NEW_TERMINAL_SPACE,
            ACTIVE_NO_DISPLAY,
            NO_INPUT_NEEDS_TERMINAL,
            DEFAULT_FONT_PREVIEW,
            ACTIVE_PICKED,
            ACTIVE_UNPICKED,
            LIVE_ON,
            OPS_OFF,
            NOT_VTIUM,
            TRIAL_ASKING,
            PREVIEW_NOT_HERE,
            PREVIEW_ASKING,
            PREVIEW_LATER,
            &template_space("Terminal"),
            &new_terminal_about("terminal-12"),
            &display_no_match("HDMI-A-1"),
            &picked.0,
            &taken.0,
            &unfit_rules(12),
        ] {
            assert!(cells(line) <= 79, "{} cells: {line}", cells(line));
        }
    }
}
