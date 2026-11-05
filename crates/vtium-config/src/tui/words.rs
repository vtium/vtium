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
pub const ACTIONS: [(&str, &str); 6] = [
    ("View", "View"),
    ("Install", "Install"),
    ("Revert", "Revert"),
    ("Restore", "Restore"),
    ("New", "New"),
    ("Delete draft", "Delete"),
];

/// An action this build does not do yet, on the message line.
pub fn not_built(what: impl Display) -> String {
    format!("{what} is not built yet.")
}
pub const CONNECT: &str = "Connecting tags";
pub const READ_DEVICES: &str = "Reading the devices again";
pub const VIEW_DRAFT: &str = "View, the draft's pager,";
pub const REVERT: &str = "Revert";
pub const RESTORE: &str = "Restore";
pub const NEW_CONFIGURATION: &str = "New, which starts a new configuration,";
pub const DELETE_DRAFT: &str = "Delete draft";
pub const NEW_NOT_BUILT: &str =
    "Making a new item in the flow is not built yet; the Fonts view makes a new frit.";

/// What live preview is doing, at the right of the first line.
pub const LIVE_ASKING: &str = "asking the terminal";
pub const LIVE_ON: &str = "live preview: on";
pub const LIVE_OFF: &str = "live preview: off";

/// Why live preview is off, on the message line.
pub const OPS_OFF: &str = "allow-font-ops is off in this vtium console, so fonts are not tried live. In the Fonts view, o turns it on when you install.";
pub const OPS_ON_PENDING: &str = "allow-font-ops is turned on when you install.";
pub const NOT_VTIUM: &str =
    "This terminal is not a vtium console, so fonts are not tried live. Installing still works.";

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
pub const SOURCES_UNKNOWN: &str = "sources not checked";

/// Marks the choice that an install writes.
pub const TO_SAVE: &str = "to install";
/// Marks what vtium.conf names now.
pub const SAVED: &str = "installed";
/// Marks a frit an install removes.
pub const TO_REMOVE: &str = "to remove";

/// The fonts view's remove key, `r`.
pub fn remove_pending(name: impl Display) -> String {
    format!("{name} is removed when you install; r again keeps it.")
}
pub fn remove_dropped(name: impl Display) -> String {
    format!("{name} is kept.")
}
pub fn remove_refused(why: impl Display) -> String {
    format!("Not removed: {why}")
}
pub fn remove_replaced(name: impl Display) -> String {
    format!("{name} is replaced by a new frit in this install")
}
pub fn remove_unread(name: impl Display) -> String {
    format!("{name}.vtfrit could not be read when the tool started")
}

/// The size that lets vtium choose.
pub const AUTOMATIC: &str = "automatic";
pub fn points(n: u32) -> String {
    format!("{n} pt")
}
/// A size's line when the terminal draws no pictures.
pub const NO_PICTURES: &str =
    "This terminal draws no pictures, so a size shows only once it is installed.";
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

/// The Looks view, which holds the sizes until looks are built.
pub const LOOKS_SIZES: &str = "The sizes the console's font takes, in points. A look of its own comes with vtium.conf's look sections.";

/// The new-frit view: its fields, and what it says.
pub const NEW_FIELD_NAMES: [&str; 6] = ["Name", "Source 1", "Source 2", "Source 3", "Cell", "Size"];
pub const NEW_ADD: &str = "Add the frit to the install";
pub const NEW_HINT: &str = "A source is a font's family, as JetBrains Mono, or a source as a frit writes it, as kbd:ter-v16n. Each later source fills what the ones before it lack. Cell is WIDTHxHEIGHT and Size is points; leave them empty for the fonts' own. An empty name is made from the first source.";
pub const NEW_BAD_NAME: &str =
    "the name is 1 to 64 letters, digits, '.', '_', '+' and '-', and not a built-in font's";
pub fn new_added(name: impl Display) -> String {
    format!("{name} is added to the install; Enter installs it.")
}
pub fn new_replaces_binary(name: impl Display) -> String {
    format!(
        "the installed {name}.vtfrit is not UTF-8 text, so a new frit does not replace it; remove it with vtium config remove-frit {name} first"
    )
}
pub fn new_refused(why: impl Display) -> String {
    format!("The frit does not read: {why}")
}

/// The install view, which installs the changes made in the views.
pub const NOTHING_TO_SAVE: &str = "Nothing is changed, so there is nothing to install.";
pub const SAVE_RUNS: &str = "Enter runs this command, which asks for your password once:";
pub const SAVE_AS_USER: &str = "The tool's directories are not the system's, so Enter installs the draft as you, with no sudo, by this command:";
pub fn draft_under(dir: impl Display) -> String {
    format!("DRAFT is a new directory under {dir}.")
}
pub const SAVING: &str =
    "Installing. The command runs in the terminal; the screen comes back after.";
pub fn saved(files: usize) -> String {
    if files == 1 {
        "Installed 1 file.".to_owned()
    } else {
        format!("Installed {files} files.")
    }
}
pub fn removed_file(name: impl Display) -> String {
    format!("removed, the old file kept as {name}.vtfrit.bak")
}
pub const COMMAND_SAID: &str = "The command said:";
pub fn save_failed(why: impl Display) -> String {
    format!("Nothing was installed: {why}")
}

/// The keys, on the last line of each view.
pub const KEYS_FLOW: &str = "Tab ←→ columns  ↑↓ move  Enter open  Space tag  c connect  f follow  d devices  ? route  v view  i install  r revert  o restore  n new  x delete  q leave";
pub const KEYS_ROUTE: &str = "↑↓ route  Enter open the item  q back to the flow";
pub const KEYS_LOOKS: &str = "Enter keep  Tab next  i install  q leave, undo";
pub const KEYS_FONTS: &str = "Enter keep  n new frit  r remove  Tab next  i install  q leave, undo";
pub const KEYS_SETTINGS: &str = "Enter change  Tab next  i install  q leave, undo";
pub const KEYS_HELP: &str = "←→ Help, About  ↑↓ scroll  Tab next  q leave";
pub const KEYS_NEW: &str = "type a field  Enter next  Esc back  Ctrl-C leave, undo";
pub const KEYS_EDITING: &str = "Enter keep  Up Down drop  Ctrl-C leave, undo";
pub const KEYS_SAVE: &str = "Enter install  Esc back  q leave, undo";

/// The screen below the TUI's minimum size (ADR-1201 D3).
pub fn too_small(cols: usize, rows: usize) -> String {
    format!(
        "vtium-config needs a terminal of at least 80 by 25 cells, and this one is {cols} by {rows}."
    )
}
pub const TOO_SMALL_COMMANDS: &str =
    "Make the terminal larger, or use the commands, which work at any size:";
pub const TOO_SMALL_KEYS: &str =
    "q leaves. u puts the console's font back when a preview left the grid too small.";

/// The status line (ADR-1403 D7).
pub const NO_DRAFT: &str = "no draft";
pub fn draft_state(name: impl Display, changes: usize, problems: usize) -> String {
    let changes = match changes {
        0 => "no changes".to_owned(),
        1 => "1 change".to_owned(),
        n => format!("{n} changes"),
    };
    let problems = match problems {
        0 => "no problems".to_owned(),
        1 => "1 problem".to_owned(),
        n => format!("{n} problems"),
    };
    format!("draft {name} · {changes} · {problems}")
}
pub fn tagged(names: &[String]) -> String {
    format!("tagged: {}", names.join(", "))
}
pub const FLOW_NOT_READ: &str = "This build does not read vtium.conf's heads, looks and rules yet, so the flow has nothing to draw.";
pub const NOTHING_TO_FOLLOW: &str = "No line leaves this item toward the heads.";

/// The flow's items (ADR-1403 D7).
pub const NOT_FOUND: &str = "configured, not found";
pub const NOT_FOUND_SHORT: &str = "not found";
pub const BY_POLICY: &str = "placed by the policy";
pub const BY_POLICY_SHORT: &str = "by policy";
pub fn consoles(n: u32) -> String {
    if n == 1 {
        "1 console".to_owned()
    } else {
        format!("{n} consoles")
    }
}
pub const ACTIVE_HEAD: &str = "the active head";
pub const ACTIVE: &str = "active";
pub const LOOK: &str = "look";
pub const DEFAULT: &str = "default";
pub const NO_SUCH_LOOK: &str = "! no such look";
pub const NO_SUCH_LOOK_SHORT: &str = "! missing";
pub fn roaming(active: Option<&str>) -> String {
    match active {
        Some(head) => format!("types into {head}, active"),
        None => "types into the active head".to_owned(),
    }
}
pub const IGNORE_DOES: &str = "vtium never opens it";
pub const OFF_DOES: &str = "vtium turns it off";
pub fn policy(value: impl Display) -> String {
    format!("policy {value}")
}
pub const POLICY_DOES: &str = "takes what no rule matches";
pub const NEW_RULE: &str = "+ new rule…";
pub const NEW_HEAD: &str = "+ new head…";
pub const NEW_LOOK: &str = "+ new look…";
pub const NEW_SHORT: &str = "+ new…";
pub fn look_size(points: u32) -> String {
    if points == 0 {
        "auto size".to_owned()
    } else {
        format!("{points} pt")
    }
}
pub const MISSING: &str = "missing";
pub const NO_HEAD_USES: &str = "no head uses it";
pub const NO_HEAD_USES_SHORT: &str = "unused";
pub const NO_SUCH_HEAD: &str = "sends to a head that does not exist";
pub const MATCHES_NO_DISPLAY: &str = "matches no display present";
pub const MATCHES_NO_DEVICE: &str = "matches no device present";
pub fn shadowed(by: usize) -> String {
    format!("rule {by} matches first")
}
pub const NEEDS_UDEV: &str = "needs udev";
pub fn out_of_sight(above: usize, below: usize) -> String {
    match (above, below) {
        (0, b) => format!("↓ {b} more"),
        (a, 0) => format!("↑ {a} above"),
        (a, b) => format!("↑ {a} above ↓ {b} more"),
    }
}
pub fn base_row(set: &[(String, String)], defaults: usize) -> String {
    let set: Vec<String> = set
        .iter()
        .map(|(k, v)| {
            if v.is_empty() {
                k.clone()
            } else {
                format!("{k} {v}")
            }
        })
        .collect();
    format!(
        "base   {}   ({defaults} keys at their defaults)",
        set.join("   ")
    )
}

/// The card Enter opens below the columns.
pub const SENDS_TO: &str = "sends to";
pub const MATCHES: &str = "matches";
pub const OR_TYPE: &str = " or type a selector";
pub const AT_DEFAULT: &str = "default";
pub fn rule_card(
    n: usize,
    line: usize,
    kind: impl Display,
    selector: impl Display,
    target: impl Display,
) -> String {
    format!("rule {n}, line {line}   {kind} {selector} -> {target}")
}
pub fn placed_by(n: usize, selector: impl Display, target: impl Display) -> String {
    format!("placed by rule {n}: {selector} -> {target}")
}
pub fn named_by(n: usize) -> String {
    format!("configured, not found: rule {n} names it, and nothing plugged in has it")
}
pub fn placed_by_policy(policy: impl Display) -> String {
    format!("no rule matches it, so the policy, {policy}, places it")
}
pub fn match_keys(keys: &[String]) -> String {
    if keys.is_empty() {
        "no match keys, since it is not plugged in".to_owned()
    } else {
        format!("match keys: {}", keys.join("  "))
    }
}
pub fn head_card(name: impl Display, consoles: u32, active: bool) -> String {
    let n = self::consoles(consoles);
    if active {
        format!("head {name} · {n} · the active head")
    } else {
        format!("head {name} · {n}")
    }
}
pub fn look_card(name: impl Display, frit: impl Display, points: u32) -> String {
    format!("look {name} · frit {frit} · {}", look_size(points))
}
pub fn look_fonts(fonts: &[&str]) -> String {
    if fonts.is_empty() {
        "its frit lists no font".to_owned()
    } else {
        format!("draws with {}", fonts.join(", "))
    }
}
pub fn look_users(heads: &[&str]) -> String {
    if heads.is_empty() {
        "no head uses it; the Looks view edits it".to_owned()
    } else {
        format!("used by {}; the Looks view edits it", heads.join(", "))
    }
}
pub const BASE_CARD: &str = "base: the top-level keys, which every head inherits";
pub fn at_defaults(n: usize) -> String {
    format!("{n} keys at their defaults; the Settings view lists every key")
}

/// The route card (ADR-1403 D7).
pub fn route_title(name: impl Display) -> String {
    format!("Route card: {name}")
}
pub fn route_of(k: usize, n: usize, when: Option<&str>) -> String {
    match when {
        Some(head) => format!("route {k} of {n}, when {head} is active"),
        None => format!("route {k} of {n}"),
    }
}
pub const SECTION_DISPLAY: &str = "display";
pub const SECTION_DISPLAY_RULE: &str = "display rule";
pub const SECTION_HEAD: &str = "head";
pub const SECTION_LOOK: &str = "look";
pub const SECTION_INPUT_RULE: &str = "input rule";
pub const SECTION_INPUT: &str = "input device";
pub const HEAD_NOT_USED: &str = "not used: no rule sends anything here";
pub const LOOK_NOT_USED: &str = "not used: no head uses this look";
pub const BASE_DOES: &str = "The top-level keys, which every head inherits.";
pub const NEW_DOES: &str = "Opening it makes a new item of its column.";

/// What an item does, the route card's first line.
pub fn route_sentence(flow: &super::flow::Flow, item: super::flow::Item) -> String {
    use super::flow::{Item, Kind, Special, Target};
    let head_name = |h: usize| {
        flow.heads
            .get(h)
            .map(|h| h.name.clone())
            .unwrap_or_default()
    };
    let device = |d: Option<&super::flow::Device>, kind: Kind| -> String {
        let Some(d) = d else {
            return String::new();
        };
        let Some((r, rule)) = d.rule.and_then(|r| flow.rules.get(r).map(|rule| (r, rule))) else {
            return format!(
                "No rule matches it, so the policy, {}, places it.",
                flow.policy
            );
        };
        let n = r.saturating_add(1);
        if !d.found {
            return format!(
                "It is configured, not found: rule {n} names it, and nothing plugged in has it."
            );
        }
        match (flow.target(rule), kind) {
            (Target::Head(h), Kind::Display) => format!("It shows {}.", head_name(h)),
            (Target::Head(h), Kind::Input) => format!("It types into {}.", head_name(h)),
            (Target::Special(Special::Off), _) => "vtium turns it off.".to_owned(),
            (Target::Special(Special::Ignore), _) => "vtium never opens it.".to_owned(),
            (Target::Special(Special::Roaming), _) => roams(flow),
            (Target::Missing, _) => format!(
                "Rule {n} sends it to {}, a head that does not exist.",
                rule.target
            ),
        }
    };
    let unused = |i: Item| flow.follows(i).is_empty();
    match item {
        Item::Display(d) => device(flow.displays.get(d), Kind::Display),
        Item::Input(d) => device(flow.inputs.get(d), Kind::Input),
        Item::Rule(r) => {
            let Some(rule) = flow.rules.get(r) else {
                return String::new();
            };
            let n = r.saturating_add(1);
            if flow.target(rule) == Target::Missing {
                return format!(
                    "Rule {n} sends {} to {}, a head that does not exist.",
                    rule.selector, rule.target
                );
            }
            let caught: Vec<&str> = flow
                .devices(rule.kind)
                .iter()
                .filter(|d| d.found && d.rule == Some(r))
                .map(|d| d.name.as_str())
                .collect();
            if caught.is_empty() {
                format!(
                    "Rule {n} sends {} to {}, and it matches no device present.",
                    rule.selector, rule.target
                )
            } else {
                format!(
                    "Rule {n} sends {} to {}, and it matches {}.",
                    rule.selector,
                    rule.target,
                    caught.join(", ")
                )
            }
        }
        Item::Head(h) | Item::HeadLook(h) if unused(Item::Head(h)) && unused(Item::HeadLook(h)) => {
            HEAD_NOT_USED.to_owned()
        }
        Item::Head(h) => {
            let shows: Vec<&str> = flow
                .displays
                .iter()
                .filter(|d| {
                    d.found
                        && d.rule
                            .and_then(|r| flow.rules.get(r))
                            .is_some_and(|r| flow.target(r) == Target::Head(h))
                })
                .map(|d| d.name.as_str())
                .collect();
            if shows.is_empty() {
                format!("{} shows on no display present.", head_name(h))
            } else {
                format!("{} shows on {}.", head_name(h), shows.join(" and "))
            }
        }
        Item::HeadLook(h) => {
            let look = flow
                .heads
                .get(h)
                .map(|h| h.look.clone())
                .unwrap_or_default();
            let users: Vec<&str> = flow
                .heads
                .iter()
                .filter(|x| x.look == look)
                .map(|x| x.name.as_str())
                .collect();
            format!("Look {look} draws {}.", users.join(" and "))
        }
        Item::Look(_) => LOOK_NOT_USED.to_owned(),
        Item::Special(_) if unused(item) => HEAD_NOT_USED.to_owned(),
        Item::Special(Special::Roaming) => match flow.active.as_deref() {
            Some(active) => {
                format!("A device sent here types into the active head, now {active}.")
            }
            None => "A device sent here types into the active head.".to_owned(),
        },
        Item::Special(Special::Ignore) => "vtium never opens a device sent here.".to_owned(),
        Item::Special(Special::Off) => "vtium turns off a display sent here.".to_owned(),
        Item::Policy => format!(
            "The policy, {}, places each device no rule matches.",
            flow.policy
        ),
        Item::Base => BASE_DOES.to_owned(),
        Item::New(_) => NEW_DOES.to_owned(),
    }
}

/// Where a roaming device types: the active head, then each other head
/// that can be active.
fn roams(flow: &super::flow::Flow) -> String {
    let others: Vec<String> = flow
        .activatable()
        .into_iter()
        .filter_map(|h| flow.heads.get(h).map(|h| h.name.clone()))
        .filter(|n| Some(n.as_str()) != flow.active.as_deref())
        .map(|n| format!("; into {n} when {n} is active"))
        .collect();
    match flow.active.as_deref() {
        Some(active) => format!(
            "It types into {active}, the active head, by roaming{}.",
            others.concat()
        ),
        None => "It types into whichever head is active, by roaming.".to_owned(),
    }
}

/// The Help view's pages (ADR-1201 D3).
pub const HELP_PAGE: &str = "Help";
pub const ABOUT_PAGE: &str = "About";
pub const HELP_VIEWS: [(&str, &str, &str); 5] = [
    (
        FLOW,
        "draws the configuration from the displays to the input devices: each display, the rule that sends it to a head, the head and its look, and each input device with the rule that sends its typing to a head.",
        KEYS_FLOW,
    ),
    (
        LOOKS,
        "manages the looks, each a frit and its size; until looks are built it offers the console's sizes, automatic first, and shows each.",
        KEYS_LOOKS,
    ),
    (
        FONTS,
        "lists the frits and the built-in fonts with what each lacks, tries each on a vtium console, and makes a new frit with n.",
        KEYS_FONTS,
    ),
    (
        SETTINGS,
        "lists every top-level key of vtium.conf, with its value and what it does.",
        KEYS_SETTINGS,
    ),
    (
        HELP,
        "holds this page, and About, with the tool's name and version.",
        KEYS_HELP,
    ),
];
pub const HELP_ROUTE: &str = "In the Flow view, ? opens the route card of the selected item: what it does, and every route from a display to an input device that it is part of.";
pub const HELP_COMMANDS: &str = "The commands, which work at any size:";

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
