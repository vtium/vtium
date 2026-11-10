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
pub const ON: &str = "on";
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
pub const KEYS_FLOW: &str = "←→ columns  ↑↓ move  Tab next view  Enter open  Space tag  c connect  f follow  d devices  ? route  v view  i install  r revert  o restore  n new  x delete  q quit";
pub const KEYS_ROUTE: &str = "↑↓ route  Enter open item  q back";
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
pub const FLOW_NOT_READ: &str = "The Flow view is empty: no draft of vtium.conf is open. Quit and start vtium config to open one.";
pub const NOTHING_TO_FOLLOW: &str = "Nothing to follow: this item has no line to a head.";

/// The flow's items (ADR-1403 D7).
pub const NOT_FOUND: &str = "in a rule, not connected";
pub const NOT_FOUND_SHORT: &str = "not connected";
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
pub const NO_SUCH_LOOK: &str = "! look not found";
pub const NO_SUCH_LOOK_SHORT: &str = "! missing";
pub fn roaming(active: Option<&str>) -> String {
    match active {
        Some(head) => format!("types into {head} (active head)"),
        None => "types into the active head".to_owned(),
    }
}
pub const IGNORE_DOES: &str = "vtium never opens it";
pub const OFF_DOES: &str = "vtium turns it off";
pub fn policy(value: impl Display) -> String {
    format!("policy {value}")
}
pub const POLICY_DOES: &str = "places each device no rule matches";
pub const NEW_RULE: &str = "+ new rule…";
pub const NEW_HEAD: &str = "+ new head…";
pub const NEW_LOOK: &str = "+ new look…";
pub const NEW_SHORT: &str = "+ new…";
pub fn look_size(points: u32) -> String {
    if points == 0 {
        "automatic size".to_owned()
    } else {
        format!("{points} pt")
    }
}
pub const MISSING: &str = "missing";
pub const NO_HEAD_USES: &str = "not used by any head";
pub const NO_SUCH_HEAD: &str = "! head not found";
/// Under a rule whose pattern matches none of the connected displays or
/// devices, and the short form for a column too narrow to hold the long
/// one whole on two rows.
pub const MATCHES_NO_DISPLAY: &str = "matches no connected display";
pub const MATCHES_NO_DEVICE: &str = "matches no connected device";
pub const MATCHES_NONE_SHORT: &str = "none connected";
pub fn shadowed(by: usize) -> String {
    format!("rule {by} matches first")
}
pub const NEEDS_UDEV: &str = "needs udev";
pub fn out_of_sight(above: usize, below: usize) -> String {
    match (above, below) {
        (0, b) => format!("↓ {b} more"),
        (a, 0) => format!("↑ {a} more"),
        (a, b) => format!("↑ {a} more ↓ {b} more"),
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
        "shared settings   {}   ({defaults} others at their defaults)",
        set.join("   ")
    )
}

/// The card Enter opens below the columns.
pub const SENDS_TO: &str = "sends to";
pub const MATCHES: &str = "matches";
pub const OR_TYPE: &str = " or type a match";
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
    format!("not connected: rule {n} names this device, and no connected device matches it")
}
pub fn placed_by_policy(policy: impl Display) -> String {
    format!("placed by the policy, {policy}, because no rule matches it")
}
pub fn match_keys(keys: &[String]) -> String {
    if keys.is_empty() {
        "no match keys: the device is not connected".to_owned()
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
    format!("look {name} · font set {frit} · {}", look_size(points))
}
pub fn look_fonts(fonts: &[&str]) -> String {
    if fonts.is_empty() {
        "its font set lists no font".to_owned()
    } else {
        format!("fonts: {}", fonts.join(", "))
    }
}
pub fn look_users(heads: &[&str]) -> String {
    if heads.is_empty() {
        "not used by any head. Edit it in the Looks view.".to_owned()
    } else {
        format!("used by {}. Edit it in the Looks view.", heads.join(", "))
    }
}
pub const BASE_CARD: &str = "shared settings: the keys at the top of vtium.conf, which every head uses unless it sets its own";
pub fn at_defaults(n: usize) -> String {
    format!("{n} keys at their defaults. The Settings view lists every key.")
}

/// The route card (ADR-1403 D7).
pub fn route_title(name: impl Display) -> String {
    format!("Route card: {name}")
}
pub fn route_of(k: usize, n: usize, when: Option<&str>) -> String {
    match when {
        Some(head) => format!("route {k} of {n}, when {head} is the active head"),
        None => format!("route {k} of {n}"),
    }
}
pub const SECTION_DISPLAY: &str = "display";
pub const SECTION_DISPLAY_RULE: &str = "display rule";
pub const SECTION_HEAD: &str = "head";
pub const SECTION_LOOK: &str = "look";
pub const SECTION_INPUT_RULE: &str = "input rule";
pub const SECTION_INPUT: &str = "input device";
pub const HEAD_NOT_USED: &str = "Not used: no rule sends anything here.";
pub const LOOK_NOT_USED: &str = "Not used: no head uses this look.";
pub const BASE_DOES: &str =
    "The keys at the top of vtium.conf, which every head uses unless it sets its own.";
pub const NEW_DOES: &str = "Press Enter to add a new item to this column.";

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
                "No rule matches this device, so the policy, {}, places it.",
                flow.policy
            );
        };
        let n = r.saturating_add(1);
        if !d.found {
            return format!("This device is not connected. Rule {n} names it.");
        }
        match (flow.target(rule), kind) {
            (Target::Head(h), Kind::Display) => {
                format!("This display shows head {}.", head_name(h))
            }
            (Target::Head(h), Kind::Input) => {
                format!("This device types into head {}.", head_name(h))
            }
            (Target::Special(Special::Off), _) => "vtium turns this display off.".to_owned(),
            (Target::Special(Special::Ignore), _) => "vtium never opens this device.".to_owned(),
            (Target::Special(Special::Roaming), _) => roams(flow),
            (Target::Missing, _) => format!(
                "Rule {n} sends this device to {target}, but there is no head named {target}.",
                target = rule.target
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
                    "Rule {n} sends {} to {target}, but there is no head named {target}.",
                    rule.selector,
                    target = rule.target
                );
            }
            match &rule.state {
                RuleState::Error(e) => {
                    return format!(
                        "Rule {n} sends {} to {}, but it has an error: {e}.",
                        rule.selector, rule.target
                    );
                }
                RuleState::Shadowed(by) => {
                    return format!(
                        "Rule {n} never matches: rule {} comes first and matches everything rule {n} would.",
                        by.saturating_add(1)
                    );
                }
                RuleState::NeedsUdev => {
                    return format!(
                        "Rule {n} never matches: its match needs udev, and this machine does not run udev."
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
                    "Rule {n} sends {} to {}. No connected device matches it.",
                    rule.selector, rule.target
                )
            } else {
                format!(
                    "Rule {n} sends {} to {}. It matches {}.",
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
                format!("Head {} is not on any connected display.", head_name(h))
            } else {
                format!("Head {} shows on {}.", head_name(h), shows.join(" and "))
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
            let uses = match users.as_slice() {
                [one] => format!("Head {one} uses look {look}"),
                _ => format!("Heads {} use look {look}", users.join(" and ")),
            };
            match flow.look_of(h).and_then(|l| flow.looks.get(l)) {
                None if look == DEFAULT => format!("{uses}."),
                None => format!(
                    "Head {} uses the default look, because vtium.conf has no look named {look}.",
                    head_name(h)
                ),
                Some(l) => look_said(l, &uses),
            }
        }
        Item::Look(l) => match flow.looks.get(l) {
            Some(look) if look.problem.is_some() || look.fonts.iter().any(|(_, here)| !here) => {
                look_said(look, &format!("Look {} is not used by any head", look.name))
            }
            _ => LOOK_NOT_USED.to_owned(),
        },
        Item::Special(_) if unused(item) => HEAD_NOT_USED.to_owned(),
        Item::Special(Special::Roaming) => match flow.active.as_deref() {
            Some(active) => {
                format!("A device sent here types into the active head, which is {active} now.")
            }
            None => "A device sent here types into the active head.".to_owned(),
        },
        Item::Special(Special::Ignore) => "vtium never opens a device sent here.".to_owned(),
        Item::Special(Special::Off) => "vtium turns off a display sent here.".to_owned(),
        Item::Policy => format!(
            "The policy, {}, places each device that no rule matches.",
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
        Some(problem) => format!("{said}, but the look has a problem: {problem}."),
        None if !missing.is_empty() => format!(
            "{said}, but {} is not installed on this machine. Font set {} needs it.",
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
        .map(|n| format!("; into {n} when {n} is the active head"))
        .collect();
    match flow.active.as_deref() {
        Some(active) => format!(
            "This device roams: it types into the active head, which is {active} now{}.",
            others.concat()
        ),
        None => "This device roams: it types into whichever head is active.".to_owned(),
    }
}

/// The Help view's pages (ADR-1201 D3).
pub const HELP_PAGE: &str = "Help";
pub const ABOUT_PAGE: &str = "About";
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
pub const HELP_ROUTE: &str = "The route card: in the Flow view, press ? to see what the selected item does and every route through it, from a display to an input device.";
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
pub const OR_TYPE_VALUE: &str = "type…";
pub const NO_DESK: &str = "This is a sample setup with no draft, so changes here are not saved.";
pub const NO_HOTPLUG: &str = "not watching for new devices";
pub const NOT_DONE: &str = "Nothing changed.";
/// A rule moved up from the first place of its kind, or down from the
/// last.
pub const fn rule_first(kind: super::flow::Kind) -> &'static str {
    match kind {
        super::flow::Kind::Display => "This rule is already the first display rule.",
        super::flow::Kind::Input => "This rule is already the first input rule.",
    }
}
pub const fn rule_last(kind: super::flow::Kind) -> &'static str {
    match kind {
        super::flow::Kind::Display => "This rule is already the last display rule.",
        super::flow::Kind::Input => "This rule is already the last input rule.",
    }
}
pub const FORM_RULE_NEEDS: &str = "A new rule needs a match and a target.";
pub const FORM_NAME_NEEDED: &str = "A new head or look needs a name.";
pub fn delete_rule(n: usize, line: &str) -> String {
    format!("Delete rule {n} ({line})?")
}
pub fn delete_placeholder(device: &str, n: usize, line: &str) -> String {
    format!("{device} is not connected. Delete rule {n} ({line}), which names it?")
}
pub fn delete_head(name: &str, rules: &[usize]) -> String {
    match rules {
        [] => format!("Delete head {name}? No rule sends to it."),
        [n] => format!(
            "Delete head {name}? Rule {n} sends to it and will not work until you change or delete it."
        ),
        _ => format!(
            "Delete head {name}? Rules {} send to it and will not work until you change or delete them.",
            list(rules.iter().map(usize::to_string))
        ),
    }
}
pub fn delete_look(name: &str, heads: &[String]) -> String {
    match heads {
        [] => format!("Delete look {name}? No head uses it."),
        [h] => format!("Delete look {name}? Head {h} uses it and will switch to the default look."),
        _ => format!(
            "Delete look {name}? Heads {} use it and will switch to the default look.",
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
pub const CONNECT_TWO_TARGETS: &str = "Cannot connect: two heads or targets are tagged. Tag one head, or one of roaming, ignore and off.";
pub const CONNECT_TWO_LOOKS: &str = "Cannot connect: two looks are tagged. Tag one.";
pub const CONNECT_TWO_RULES: &str = "Cannot connect: two rules are tagged. Tag one.";
pub const CONNECT_TWO_KINDS: &str = "Cannot connect a display and an input device together. Tag displays or input devices, not both.";
pub const CONNECT_RULE_ONE_DEVICE: &str = "Cannot connect: a rule takes one device. Tag one.";
pub const CONNECT_NO_HEAD: &str =
    "Cannot connect: no head is tagged. Tag a head, or roaming, ignore or off.";
pub const CONNECT_LOOK_NO_HEAD: &str = "Cannot connect: a look connects to a head. Tag a head too.";
pub const CONNECT_NOTHING: &str = "Nothing is tagged. Press Space to tag the selected item.";
pub const CONNECT_NO_CHANGE: &str = "Nothing to change: the tagged items are already joined.";
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
pub fn delete_draft_question(added: usize, removed: usize) -> String {
    format!("Delete the draft? {} will be lost.", lost(added, removed))
}
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
pub const KEYS_CARD: &str = "↑↓ field  ←→ pick  Enter choose  t try live  Esc close  q quit";
pub const KEYS_TYPING: &str = "type the value  Enter keep  Esc cancel";
pub const KEYS_CONFIRM: &str = "y yes  any other key no";
pub const KEYS_LEAVE: &str = "i install  l quit anyway  any other key back";
pub const KEYS_PAGER: &str =
    "j k ↑↓ lines  h l ←→ sideways  g G top bottom  ^D ^U half page  q back";
/// The file view's keys in the Flow view as paths, which also saves and
/// installs.
pub const ROWS_KEYS_PAGER: &str =
    "s save  i install  j k ↑↓ lines  h l ←→ sideways  g G top bottom  ^D ^U half page  q back";
pub const KEYS_RESTORE: &str = "↑↓ choose  Enter restore  q back";
pub const fn new_card(new: super::flow::New) -> &'static str {
    match new {
        super::flow::New::DisplayRule => "new display rule: a match and the head it sends to",
        super::flow::New::InputRule => "new input rule: a match and its target",
        super::flow::New::Head => "new head: a name and a look",
        super::flow::New::Look => "new look: a name, a font set and a size",
    }
}
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

/// The Flow view as paths, the try-out `VTIUM_CONFIG_FLOW=rows`.
pub const ROWS_PATHS: &str = "CONFIGURATION";
pub const ROWS_NEW_PATH: &str = "NEW PATH";
pub const ROWS_PIECES: &str = "PARTS";
pub const ROWS_DISPLAYS: &str = "DISPLAYS";
pub const ROWS_TERMINALS: &str = "TERMINALS";
pub const ROWS_INPUTS: &str = "INPUTS";
pub const ROWS_FONT_SETS: &str = "FONT SETS";
pub const NO_PATHS: &str =
    "No paths yet. Pick an input and a display in PARTS, then press a to add the path.";
pub fn unfit_rules(n: usize) -> String {
    match n {
        1 => "1 rule belongs to no path. v shows it in the file.".to_owned(),
        n => format!("{n} rules belong to no path. v shows them in the file."),
    }
}
pub const ROWS_ROAMING: &str = "ROAMING";
pub const ROWS_ROAMING_ABOUT: &str = "These inputs type into whichever terminal is active.";
pub const ROWS_ROAMING_STUB: &str = "roaming inputs";
pub const ROAMING_WORD: &str = "roaming";
pub const REVISE_ROAMING_NOTE: &str = "To change the roaming inputs, press e. They go into NEW PATH with the active terminal, and a adds them back.";
pub const IN_ROAMING: &str = "This input roams. Press e to revise the roaming inputs.";
pub const PATH_EMPTY: &str = "Pick an input and a display to make a path.";
pub const PATH_NEEDS_DISPLAY: &str = "Add a display to finish this path.";
pub const PATH_NEEDS_INPUT: &str = "Add an input to finish this path.";
pub const PATH_COMPLETE: &str = "This path is complete. Press a to add it.";
pub const ACTIVE_TERMINAL: &str = "the active terminal";
pub const ACTIVE_DETAIL: &str = "whichever is in use";
pub const ACTIVE_PART_DETAIL: &str = "makes a path's inputs roam";
pub const ACTIVE_COMPLETE: &str =
    "This path is complete: its inputs type into whichever terminal is active. Press a to add it.";
pub const ACTIVE_NEEDS_INPUT: &str = "Pick an input to type into whichever terminal is active.";
pub const ACTIVE_NO_DISPLAY: &str = "The active terminal has no display of its own, since it is one of the other terminals at a time. Take the displays out of this path.";
pub const ACTIVE_ABOUT: &str = "The active terminal is whichever terminal is in use. An input in its path roams: it types into the terminal you switch to.";
pub const ACTIVE_PICKED: &str = "This path's inputs will type into whichever terminal is active.";
pub const ACTIVE_UNPICKED: &str = "This path's inputs will type into a terminal of their own.";
pub const SAME_ACTIVE: &str = "These inputs roam already, so nothing changed.";
pub const ACTIVE_BACK: &str = "The roaming inputs are back in CONFIGURATION.";
pub const ACTIVE_REVISED: &str =
    "The roaming inputs are in NEW PATH now. Change them, then press a to add them back.";
pub const REVISED_DROPPED: &str = "Took the path you were revising out of the configuration.";
pub const ANY_DISPLAY: &str = "any display";
pub const EVERY_DISPLAY: &str = "every display";
pub const ANY_POINTER_TITLE: &str = "any pointer";
pub const EVERY_POINTER: &str = "every mouse, touchpad and trackball";
pub const BY_CONNECTOR: &str = "by its connector";
pub const NOT_PLUGGED: &str = "not plugged in";
pub const EVERY_INPUT_ON_PORT: &str = "every input on that port";
pub const DEFAULT_FONT_SET: &str = "default font set";
pub const DEFAULT_FONT_SET_DETAIL: &str = "the font vtium.conf sets";
pub const FONT_SET: &str = "font set";
pub const IN_NO_PATH: &str = "in no path";
pub const ROWS_ROAMS: &str = "roams";
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
pub const ROAMS_SENTENCE: &str = "It roams: it types into whichever terminal is active.";
pub const REVISE_NOTE: &str =
    "Which displays and inputs this path joins change only when you revise it: press e.";
pub const NO_FONT_SET_KEYS: &str =
    "This path uses the default font set, which has no settings of its own here.";
pub const CHOOSE_DISPLAY: &str = "CHOOSE WHICH DISPLAYS THIS PART MATCHES";
pub const CHOOSE_INPUT: &str = "CHOOSE WHICH INPUTS THIS PART MATCHES";
pub const CHOOSE_ABOUT: &str =
    "The choices go from the widest to the narrowest. Enter picks one into the new path.";
pub const NEW_TERMINAL: &str = "THE NEW PATH'S TERMINAL";
pub const AUTOMATIC_SIZE: &str = "automatic";
pub const KEEP_SESSIONS: &str = "keep the sessions running";
pub const END_SESSIONS: &str = "end the sessions";
pub const SYSTEM_LOGIN: &str = "the system's login";
pub const DISCONNECT: &str = "disconnect it";
pub const CONNECTED: &str = "connected";
pub const EDITOR_NOTE: &str =
    "Space on a part in PARTS connects it to this path. Enter on a part here disconnects it.";
pub const BUILT_IN_DEFAULT: &str = "This is vtium's built-in default: every display shows the terminal console, and every keyboard and pointer types into it.";
pub const ENTER_TO_TYPE: &str = "Enter types a new name";
pub const ROWS_KEYS: &str = "↑↓←→ move  Space pick  Enter open  a add path  e revise  v view file  Esc clear  i install  o restore  n new  p preview  t trial  PgUp PgDn scroll  q quit";
pub const ROWS_KEYS_EDITOR_CARD: &str = "↑↓ move  ←→ choose  Enter set  Esc back  q quit";
pub const ROWS_KEYS_CARD: &str =
    "↑↓ move  ←→ choose  Enter set  e revise this path  Esc back  q quit";
pub const ROWS_KEYS_INFO: &str = "e revise  Esc back  q quit";
pub const ROWS_KEYS_CHOOSE: &str = "↑↓ move  Enter choose  Esc back  q quit";
pub const ROWS_SAVE: &str = "The draft keeps each change as you make it, so there is nothing to save. Press i to install it.";
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
pub const PREVIEW_PATH_LATER: &str =
    "Previewing a whole path is not built in this try-out. Enter on a font set shows its preview.";
pub const LIVE_ON: &str = "Live trial is on: in the Fonts view, t tries a font on this console.";
pub const ROWS_COMPLETE: &str = "every device is in a path";
pub const ROWS_OTHERS_TITLE: &str = "OTHER INPUTS";
pub const ROWS_OTHERS_ABOUT: &str = "These inputs are neither keyboards nor pointers, so none of them types into a terminal. Each line says which rule catches it.";
pub const ROWS_NO_RULE: &str = "no rule names it";
/// The Flow view's actions in the tab bar, long and short.
pub const ROWS_ACTIONS: [(&str, &str); 5] = [
    ("v View file", "View file"),
    ("s Save", "Save"),
    ("i Install", "Install"),
    ("r Revert", "Revert"),
    ("n New", "New"),
];
pub fn rows_other_inputs(n: usize) -> String {
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
pub fn display_taken(display: &str, head: &str) -> String {
    format!(
        "The display {display} is already {}. A display can be in one path only.",
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
        (true, true) => format!("{what} They will type into whichever terminal is active. {next}"),
        (true, false) => format!("{what} It will type into whichever terminal is active. {next}"),
        _ => format!("{what} {next}"),
    }
}
/// What waits, as the status line counts it: `1 keyboard and 1 display
/// in no path yet`.
pub fn rows_waiting(keyboards: usize, pointers: usize, displays: usize) -> String {
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
        [] => return ROWS_COMPLETE.to_owned(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("{list} in no path yet")
}
/// The policy in a few words, under the status line.
pub fn rows_policy(policy: &str) -> String {
    match policy {
        "closed" => "policy closed: vtium ignores a keyboard that no rule names".to_owned(),
        _ => "policy open: a keyboard that no rule names types into the active terminal".to_owned(),
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
            "Throw away the draft's changes? 3 lines added and 1 removed will be lost."
        );
        assert_eq!(
            super::NEW_QUESTION,
            "Start a new configuration? The draft's heads, rules and looks will be lost."
        );
    }
}
