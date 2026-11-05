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

/// How the TUI previews a change, and whether the live trial works, at
/// the right of the status line (ADR-1201 D4).
pub const PREVIEW_ASKING: &str = "asking the terminal";
pub const PREVIEW_PICTURE: &str = "picture";
pub const PREVIEW_TEXT: &str = "text";
pub const TRIAL_KEY: &str = "t: live";
pub const TRIAL_NONE: &str = "no live";

/// Why the live trial does not work, on the message line.
pub const OPS_OFF: &str = "allow-font-ops is off in this vtium console, so t cannot try a font on the console. In the Fonts view, o turns it on when you install.";
pub const NOT_VTIUM: &str =
    "This terminal is not a vtium console, so t cannot try a font on it. Installing still works.";

/// The live trial (ADR-1201 D4).
pub const TRIAL_OFF: &str = "The live trial works only in a vtium console with allow-font-ops on.";
pub fn trial_countdown(font: impl Display, left: u32) -> String {
    format!(
        "Trying {font} on the console for {left} s: press k to keep it, any other key puts the font back."
    )
}
pub fn trial_kept(font: impl Display) -> String {
    format!(
        "The console keeps {font} until vtium-config leaves. Enter puts it in the draft, and Install makes it the configured font."
    )
}
pub fn trial_back(font: impl Display) -> String {
    format!("The console is back in {font}.")
}
pub fn trial_same(font: impl Display) -> String {
    format!("The console already draws {font}.")
}
pub fn trial_refused(font: impl Display) -> String {
    format!("The console refused {font}; see its log.")
}

/// The line beside the detailed specimen (ADR-1201 D4).
pub fn detail_caption(font: impl Display, points: u32) -> String {
    format!(
        "The detailed specimen of {font} at {points} pt: white characters come from the font, grey ones from the built-in font, and an empty box marks one neither draws. s shows the console again."
    )
}

/// The line beside a picture of the console (ADR-1201 D4).
pub fn caption(c: &super::shot::Caption) -> String {
    let (w, h) = c.display;
    let display = if c.assumed {
        format!("an assumed {w}x{h} display, since none is found,")
    } else {
        format!("this {w}x{h} display")
    };
    format!(
        "{} at {} pt on {display} draws cells of {}x{} pixels, a grid of {} columns by {} rows.",
        c.font, c.points, c.cell.width, c.cell.height, c.grid.0, c.grid.1
    )
}

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
pub const NO_PICTURES: &str = "This terminal draws no pictures, so a change is described in words.";
pub fn density(dpi: u32, from: impl Display) -> String {
    format!("Pictures are drawn at {dpi} dpi, {from}.")
}
pub fn no_picture(why: impl Display) -> String {
    format!("No picture of the console: {why}")
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
pub fn saved(files: usize) -> String {
    if files == 1 {
        "Installed 1 file.".to_owned()
    } else {
        format!("Installed {files} files.")
    }
}

/// The keys, on the last line of each view.
pub const KEYS_FLOW: &str = "Tab ←→ columns  ↑↓ move  Enter open  Space tag  c connect  f follow  d devices  ? route  v view  i install  r revert  o restore  n new  x delete  q leave";
pub const KEYS_ROUTE: &str = "↑↓ route  Enter open the item  q back to the flow";
pub const KEYS_LOOKS: &str = "Enter keep  Tab next  i install  q leave, undo";
pub const KEYS_FONTS: &str =
    "Enter keep  t try live  s specimen  n new frit  r remove  Tab next  i install  q leave, undo";
pub const KEYS_SETTINGS: &str = "Enter change  Tab next  i install  q leave, undo";
pub const KEYS_HELP: &str = "←→ Help, About  ↑↓ scroll  Tab next  q leave";
pub const KEYS_NEW: &str = "type a field  Enter next  Esc back  Ctrl-C leave, undo";
pub const KEYS_EDITING: &str = "Enter keep  Up Down drop  Ctrl-C leave, undo";

/// The screen below the TUI's minimum size (ADR-1201 D3).
pub fn too_small(cols: usize, rows: usize) -> String {
    format!(
        "vtium-config needs a terminal of at least 80 by 25 cells, and this one is {cols} by {rows}."
    )
}
pub const TOO_SMALL_COMMANDS: &str =
    "Make the terminal larger, or use the commands, which work at any size:";
pub const TOO_SMALL_KEYS: &str = "q leaves. u puts back the console's font from when vtium-config started, when a font it tried left the grid too small.";

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
    use super::flow::{Item, Kind, RuleState, Special, Target};
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
            match &rule.state {
                RuleState::Error(e) => {
                    return format!(
                        "Rule {n} sends {} to {}, but the check finds an error in it: {e}.",
                        rule.selector, rule.target
                    );
                }
                RuleState::Shadowed(by) => {
                    return format!(
                        "Rule {n} sends {} to {}, but rule {} comes first and matches everything it would, so it matches nothing.",
                        rule.selector,
                        rule.target,
                        by.saturating_add(1)
                    );
                }
                RuleState::NeedsUdev => {
                    return format!(
                        "Rule {n} sends {} to {}, but its selector needs udev, which this machine does not run, so it matches nothing.",
                        rule.selector, rule.target
                    );
                }
                RuleState::Matches | RuleState::MatchesNothing => {}
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
            let users = users.join(" and ");
            match flow.look_of(h).and_then(|l| flow.looks.get(l)) {
                None if look == DEFAULT => format!("Look {look} draws {users}."),
                None => format!(
                    "Head {} names look {look}, which the file does not have, so it draws with the default look.",
                    head_name(h)
                ),
                Some(l) => look_said(l, &format!("Look {look} draws {users}")),
            }
        }
        Item::Look(l) => match flow.looks.get(l) {
            Some(look) if look.problem.is_some() || look.fonts.iter().any(|(_, here)| !here) => {
                look_said(
                    look,
                    &format!("Look {} is not used: no head uses it", look.name),
                )
            }
            _ => LOOK_NOT_USED.to_owned(),
        },
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

/// What a look does, `said`, with what is wrong with it: the problem the
/// check finds, else the fonts its frit names that this machine lacks.
fn look_said(look: &super::flow::Look, said: &str) -> String {
    let missing: Vec<&str> = look
        .fonts
        .iter()
        .filter(|(_, here)| !here)
        .map(|(f, _)| f.as_str())
        .collect();
    match &look.problem {
        Some(problem) => format!("{said}, but the check finds a problem in it: {problem}."),
        None if !missing.is_empty() => format!(
            "{said}, but this machine lacks {}, which its frit {} names.",
            missing.join(" and "),
            look.frit
        ),
        None => format!("{said}."),
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
        "lists the frits and the built-in fonts with what each lacks, shows each as a picture of the console or, with s, as a specimen of every character it covers, tries one on a vtium console with t, and makes a new frit with n.",
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

/// The Flow view's changes to the draft (ADR-1403 D7, D8).
pub const NAME: &str = "name";
pub const FRIT: &str = "frit";
pub const SIZE: &str = "size";
pub const OR_TYPE_VALUE: &str = "type…";
pub const NO_DESK: &str = "This view shows a sample flow and has no draft, so it changes nothing.";
pub const NO_HOTPLUG: &str = "cannot follow hotplug";
pub const NOT_DONE: &str = "Nothing changed.";
pub const RULE_FIRST: &str = "This rule is the first of its kind already.";
pub const RULE_LAST: &str = "This rule is the last of its kind already.";
pub const FORM_RULE_NEEDS: &str = "A new rule needs what it matches and where it sends it.";
pub const FORM_NAME_NEEDED: &str = "A new head or look needs a name.";
pub fn delete_rule(n: usize, line: &str) -> String {
    format!("Delete rule {n}, {line}?")
}
pub fn delete_placeholder(device: &str, n: usize, line: &str) -> String {
    format!("{device} is not plugged in. Delete rule {n}, {line}, which names it?")
}
pub fn delete_head(name: &str, rules: &[usize]) -> String {
    match rules {
        [] => format!("Delete head {name}? No rule sends to it."),
        [n] => format!(
            "Delete head {name}? Rule {n} still sends to it and stays, broken, until you point it elsewhere or delete it."
        ),
        _ => format!(
            "Delete head {name}? Rules {} still send to it and stay, broken, until you point them elsewhere or delete them.",
            list(rules.iter().map(usize::to_string))
        ),
    }
}
pub fn delete_look(name: &str, heads: &[String]) -> String {
    match heads {
        [] => format!("Delete look {name}? No head uses it."),
        [h] => format!("Delete look {name}? The head {h} uses it and goes to the default look."),
        _ => format!(
            "Delete look {name}? The heads {} use it and go to the default look.",
            list(heads.iter().cloned())
        ),
    }
}
/// `a`, `a and b`, `a, b and c`.
fn list(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.collect();
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}
pub const CONNECT_TWO_TARGETS: &str =
    "The tags name two heads or targets; tag one head, or one of roaming, ignore and off.";
pub const CONNECT_TWO_LOOKS: &str = "The tags name two looks; tag one.";
pub const CONNECT_TWO_RULES: &str = "The tags name two rules; tag one.";
pub const CONNECT_TWO_KINDS: &str =
    "The tags mix displays and input devices; a path holds one kind.";
pub const CONNECT_RULE_ONE_DEVICE: &str = "A rule takes one tagged device; tag one.";
pub const CONNECT_NO_HEAD: &str =
    "The tags name no head to connect to; tag a head, or roaming, ignore or off.";
pub const CONNECT_LOOK_NO_HEAD: &str = "A look needs a head to connect to; tag a head too.";
pub const CONNECT_NOTHING: &str = "Nothing is tagged; Space tags the selected item.";
pub const CONNECT_NO_CHANGE: &str =
    "The tags describe the draft as it is, so connecting changes nothing.";
pub fn connect_display_to(target: &str) -> String {
    format!("A display cannot be sent to {target}; displays go to a head or off.")
}
pub fn connect_input_to(target: &str) -> String {
    format!("An input device cannot be sent to {target}; inputs go to a head, roaming or ignore.")
}
pub fn connect_look_to(target: &str) -> String {
    format!("A look goes with a head, not with {target}.")
}
pub fn connect_no_key(device: &str) -> String {
    format!("{device} offers no key a rule could name it by.")
}
pub const INSTALLING: &str = "Installing; sudo may ask for your password.";
pub const NOTHING_TO_INSTALL: &str = "The draft is the installed file; nothing to install.";
pub fn install_question(added: usize, removed: usize, frits: usize) -> String {
    match frits {
        0 => format!("Install the draft? {}.", lines(added, removed)),
        1 => format!("Install the draft? {}, and 1 frit.", lines(added, removed)),
        n => format!(
            "Install the draft? {}, and {n} frits.",
            lines(added, removed)
        ),
    }
}
pub const NOT_PREPARED: &str = "Install has nothing ready; press i again.";
pub fn install_failed(why: &str) -> String {
    format!("Nothing is installed: {why}")
}
pub fn installed(name: &str) -> String {
    format!("Installed draft {name}. It takes effect when vtium next reads its configuration.")
}
pub fn revert_question(added: usize, removed: usize) -> String {
    format!(
        "Throw away the draft's changes? {} are lost.",
        lost(added, removed)
    )
}
pub const REVERTED: &str = "The draft is the installed file again.";
pub const NEW_QUESTION: &str =
    "Start a new configuration? The draft's heads, rules and looks are lost.";
pub const NEW_DONE: &str = "The draft starts over, with every key at its default and one look, default. Wire a display and a keyboard to a head before installing.";
pub fn delete_draft_question(added: usize, removed: usize) -> String {
    format!("Delete the draft? {} are lost.", lost(added, removed))
}
pub fn deleted_draft(next: &str) -> String {
    format!("The draft is deleted; draft {next} starts again from the installed file.")
}
pub const NO_HISTORY: &str =
    "Nothing has been installed from vtium-config yet, so there is nothing to restore.";
pub fn restore_question(n: usize, added: usize, removed: usize) -> String {
    format!(
        "Replace the draft with installed file {n}? {} are lost.",
        lost(added, removed)
    )
}
pub const RESTORED: &str = "The draft is the installed file you chose. Install puts it back.";
pub fn no_history_file(n: usize) -> String {
    format!("the installed history has no file {n}")
}
pub const RESTORE_TITLE: &str = "Restore: the installed history, newest first";
pub const PAGER_TITLE: &str = "The draft, read only";
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
    let came = (!came.is_empty()).then(|| format!("{} came", list(came.iter().cloned())));
    let went = (!went.is_empty()).then(|| format!("{} went", list(went.iter().cloned())));
    match (came, went) {
        (Some(c), Some(w)) => format!("{c}; {w}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => "the devices are as they were".to_owned(),
    }
}
pub const KEYS_CARD: &str = "↑↓ field  ←→ pick  Enter choose  t try live  Esc close  q leave";
pub const KEYS_TYPING: &str = "type the value  Enter keep  Esc drop";
pub const KEYS_CONFIRM: &str = "y yes  any other key no";
pub const KEYS_PAGER: &str =
    "j k ↑↓ lines  h l ←→ sideways  g G top bottom  ^D ^U half page  q back";
pub const KEYS_RESTORE: &str = "↑↓ choose  Enter restore  q back";
pub const fn new_card(new: super::flow::New) -> &'static str {
    match new {
        super::flow::New::DisplayRule => {
            "new display rule: what it matches and the head it sends to"
        }
        super::flow::New::InputRule => "new input rule: what it matches and where it sends it",
        super::flow::New::Head => "new head: its name and its look",
        super::flow::New::Look => "new look: its name, its frit and its size",
    }
}
pub fn leave_question(added: &[String], removed: &[String]) -> String {
    let mut lost = Vec::new();
    if !added.is_empty() {
        lost.push(format!("the new frits {}", list(added.iter().cloned())));
    }
    if !removed.is_empty() {
        lost.push(format!("the removal of {}", list(removed.iter().cloned())));
    }
    format!(
        "Leave without installing? You lose {}. i installs, l leaves anyway, any other key goes back.",
        list(lost.into_iter())
    )
}
pub const NOT_LEFT: &str = "Back to the view; nothing is lost.";
pub fn frits_waiting(n: usize) -> String {
    if n == 1 {
        "1 frit to install".to_owned()
    } else {
        format!("{n} frits to install")
    }
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
            "Throw away the draft's changes? 3 lines added and 1 removed are lost."
        );
        assert_eq!(
            super::NEW_QUESTION,
            "Start a new configuration? The draft's heads, rules and looks are lost."
        );
    }
}
