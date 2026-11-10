//! The configuration as paths, for the Flow view ([`super::flowview`]).
//! A path joins input parts and display parts through one terminal (a
//! head), with a font set, its size, and the terminal's settings. The
//! person composes a new path from parts in the editor, adds it to the
//! configuration, and revises a path to change which parts it joins:
//! revising moves the path out of the configuration and back into the
//! editor. What allocates nothing, its name, font set, size and terminal
//! settings, can change in place.
//!
//! An input that roams types into whichever terminal is active, among
//! every terminal of the configuration, so the configuration holds the
//! roaming inputs once, beside its paths ([`Doc::roaming`]). An input
//! picked into a second path, or into the first path of a configuration,
//! roams, as vtium's shipped default has its keyboards and pointers roam.
//!
//! A part is a selector, as a rule writes it: `edid:…` or `*` for a
//! display, `path:…`, `id:…`, `serial:…` or `class:keyboard` for an input.
//! [`read`] reads the configuration out of a draft's flow, and [`plan`]
//! writes one back as the draft's changes: a head per path, a rule per
//! part, and the rules placed most specific first. Every change goes
//! through [`super::edit::Change`], so through `src/ops.rs`.

use super::edit::Change;
use super::flow::{Device, Flow, Kind, Rule, Special, Target};
use super::flowview::{Sort, port_words, sort};
use super::words;

/// The part `any keyboard`: every input with keys, written as a rule for
/// full keyboards and one for the inputs with some keys, as vtium's
/// shipped default writes them (ADR-1403 D8). Its own word is no class of
/// vtium's, so a lone `class:keyboard` rule reads back as a part of its own
/// and a change elsewhere never adds `class:key` to it.
pub const ANY_KEYBOARD: &str = "class:keyboard+key";
const KEYBOARD_CLASSES: [&str; 2] = ["class:keyboard", "class:key"];

/// The part `any pointer`, written as one rule for each class of pointer
/// the shipped default lists.
pub const ANY_POINTER: &str = "class:pointer";
pub const POINTER_CLASSES: [&str; 5] = [
    "class:mouse",
    "class:touchpad",
    "class:touchscreen",
    "class:trackball",
    "class:pointingstick",
];

/// The classes in the order the shipped default lists their rules.
const CLASS_ORDER: [&str; 7] = [
    "class:keyboard",
    "class:mouse",
    "class:touchpad",
    "class:touchscreen",
    "class:trackball",
    "class:pointingstick",
    "class:key",
];

/// The rules one part writes.
pub fn expand(part: &str) -> Vec<&str> {
    match part {
        ANY_KEYBOARD => KEYBOARD_CLASSES.to_vec(),
        ANY_POINTER => POINTER_CLASSES.to_vec(),
        p => vec![p],
    }
}

/// The terminal settings a path carries that allocate nothing: how many
/// consoles, the login, and what happens when its screen is unplugged.
/// `isolated` is kept as the draft has it.
pub const SETTINGS: [&str; 4] = ["consoles", "login", "when-display-lost", "isolated"];

/// One part of a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Piece {
    Display(String),
    Input(String),
    /// A font set, by its frit's name; `None` for the default font set.
    Font(Option<String>),
    /// The active terminal: a path with it in its terminal's place sends
    /// its inputs roaming, to whichever terminal is active.
    Active,
    /// No input: nothing types into the path's terminal. vtium-config
    /// writes no input rule for the terminal and `isolated = yes` in its
    /// section, so roaming inputs stay out of it too.
    NoInput,
    /// No terminal: a path to it turns its displays off and sets its
    /// inputs aside, as the rules `display MATCH -> off` and
    /// `input MATCH -> ignore` do.
    NoTerminal,
    /// A new terminal the person made, by its name.
    Terminal(String),
}

/// A path: its terminal's name once it has one, its parts, its font set
/// and size, and its terminal's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path {
    /// The head's name: the draft's for a path read from it, kept when the
    /// path is revised, or typed in the editor; `None` for a path that has
    /// none yet.
    pub name: Option<String>,
    pub displays: Vec<String>,
    /// The inputs sent to this terminal alone. In the editor, every input
    /// picked, some of which roam once the path is added.
    pub inputs: Vec<String>,
    /// The frit the path's look draws with; `None` for the default.
    pub font: Option<String>,
    /// The size in points; `None` for the automatic size.
    pub size: Option<u32>,
    /// The terminal's settings, each a key of [`SETTINGS`] and its value.
    pub settings: Vec<(String, String)>,
    /// The path's terminal is the active terminal: its inputs roam, and it
    /// has no display, no name and no font set of its own.
    pub active: bool,
    /// The path has "no input" in its inputs' place: nothing types into
    /// its terminal.
    pub no_input: bool,
    /// The path's terminal is "no terminal": its displays are off and its
    /// inputs are set aside.
    pub none: bool,
}

/// What picking a part did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    Added,
    /// `any keyboard` came in, and `any pointer` with it.
    AddedWithPointers,
    Removed,
    /// Another font set took the place of this one.
    Replaced(Option<String>),
}

impl Path {
    /// Whether the path can be added to the configuration `doc`: the
    /// active terminal with inputs and no display; or a terminal with a
    /// display, and an input of its own or roaming inputs to reach it.
    pub fn complete(&self, doc: &Doc) -> bool {
        [
            words::PATH_COMPLETE,
            words::PATH_COMPLETE_DISPLAYS,
            words::KIOSK_COMPLETE,
            words::ACTIVE_COMPLETE,
            words::NONE_COMPLETE,
        ]
        .contains(&self.status(doc))
    }

    /// Whether the path has nothing in it at all.
    pub fn is_empty(&self) -> bool {
        *self == Path::default()
    }

    /// Whether `piece` is in the path.
    pub fn has(&self, piece: &Piece) -> bool {
        match piece {
            Piece::Display(s) => self.displays.contains(s),
            Piece::Input(s) => self.inputs.contains(s),
            Piece::Font(f) => self.font == *f && f.is_some(),
            Piece::Active => self.active,
            Piece::NoInput => self.no_input,
            Piece::NoTerminal => self.none,
            Piece::Terminal(n) => self.name.as_deref() == Some(n.as_str()),
        }
    }

    /// The value of the setting `key`, empty when the path leaves it at its
    /// default.
    pub fn setting(&self, key: &str) -> String {
        self.settings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    /// The value the path writes for the setting `key`: `isolated = yes`
    /// for a path with "no input", else the setting as the path has it.
    fn wanted(&self, key: &str) -> String {
        if key == "isolated" && self.no_input {
            return "yes".to_owned();
        }
        self.setting(key)
    }

    /// Sets the setting `key`, or leaves it at its default when `value` is
    /// empty.
    pub fn set(&mut self, key: &str, value: &str) {
        self.settings.retain(|(k, _)| k != key);
        if !value.is_empty() {
            self.settings.push((key.to_owned(), value.to_owned()));
        }
    }

    /// Picks `piece` into the path, or takes it out when it is there. A
    /// second font set takes the first one's place.
    pub fn pick(&mut self, piece: Piece) -> Picked {
        let toggle = |list: &mut Vec<String>, s: String| {
            if let Some(at) = list.iter().position(|x| *x == s) {
                list.remove(at);
                Picked::Removed
            } else {
                list.push(s);
                Picked::Added
            }
        };
        let flag = |on: &mut bool| {
            *on = !*on;
            if *on { Picked::Added } else { Picked::Removed }
        };
        match piece {
            Piece::Display(s) => toggle(&mut self.displays, s),
            // an input takes the place of "no input", and the other way round
            Piece::Input(s) => {
                self.no_input = false;
                toggle(&mut self.inputs, s)
            }
            Piece::NoInput => {
                self.inputs.clear();
                flag(&mut self.no_input)
            }
            // the terminal's place holds one terminal part at a time
            Piece::NoTerminal => {
                self.active = false;
                self.no_input = false;
                self.name = None;
                flag(&mut self.none)
            }
            Piece::Terminal(n) if self.name.as_deref() == Some(n.as_str()) => {
                self.name = None;
                Picked::Removed
            }
            Piece::Terminal(n) => {
                self.active = false;
                self.none = false;
                match self.name.replace(n) {
                    None => Picked::Added,
                    Some(_) => Picked::Replaced(None),
                }
            }
            Piece::Font(f) if self.font == f => {
                self.font = None;
                Picked::Removed
            }
            Piece::Font(f) => match std::mem::replace(&mut self.font, f) {
                None => Picked::Added,
                old => Picked::Replaced(old),
            },
            Piece::Active => {
                self.none = false;
                self.name = None;
                flag(&mut self.active)
            }
        }
    }

    /// Picks `piece` as the person does from PARTS: as [`Path::pick`],
    /// except that `any keyboard` brings `any pointer` along into a path
    /// that has no pointer, as vtium's shipped default sends every
    /// keyboard and every pointer together.
    pub fn pick_part(&mut self, flow: &Flow, piece: Piece) -> Picked {
        let keyboard = piece == Piece::Input(ANY_KEYBOARD.to_owned());
        let done = self.pick(piece);
        let pointer = self
            .inputs
            .iter()
            .any(|s| sort_of(flow, &Piece::Input(s.clone())) == Sort::Pointer);
        if keyboard && done == Picked::Added && !pointer {
            self.inputs.push(ANY_POINTER.to_owned());
            return Picked::AddedWithPointers;
        }
        done
    }

    /// Puts `new` in the place of the part `old`, keeping its place.
    pub fn replace(&mut self, old: &Piece, new: Piece) {
        let swap = |list: &mut Vec<String>, o: &str, n: String| {
            if list.contains(&n) {
                list.retain(|x| x != o);
            } else if let Some(x) = list.iter_mut().find(|x| *x == o) {
                *x = n;
            } else {
                list.push(n);
            }
        };
        match (old, new) {
            (Piece::Display(o), Piece::Display(n)) => swap(&mut self.displays, o, n),
            (Piece::Input(o), Piece::Input(n)) => swap(&mut self.inputs, o, n),
            (_, Piece::Font(f)) => self.font = f,
            _ => {}
        }
    }

    /// What the new path still needs to join the configuration `doc`, or
    /// that it is ready, in words.
    pub fn status(&self, doc: &Doc) -> &'static str {
        let (no_input, no_display) = (self.inputs.is_empty(), self.displays.is_empty());
        if self.active {
            return match (no_input, no_display) {
                (_, false) => words::ACTIVE_NO_DISPLAY,
                (true, true) => words::ACTIVE_NEEDS_INPUT,
                (false, true) => words::ACTIVE_COMPLETE,
            };
        }
        if self.none {
            return if no_input && no_display {
                words::NONE_EMPTY
            } else {
                words::NONE_COMPLETE
            };
        }
        match (no_input && !self.no_input, no_display) {
            (true, true) => words::PATH_EMPTY,
            (false, true) => words::PATH_NEEDS_DISPLAY,
            _ if self.no_input => words::KIOSK_COMPLETE,
            // a terminal with displays alone: roaming inputs, or the
            // policy, reach it when it is active
            (true, false) if doc.roaming.is_empty() => words::PATH_COMPLETE_DISPLAYS,
            _ => words::PATH_COMPLETE,
        }
    }
}

/// The configuration as paths: its paths, one per head in the file's
/// order; the inputs that roam, which type into whichever terminal is
/// active; and how many of its rules belong to neither.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    pub paths: Vec<Path>,
    pub roaming: Vec<String>,
    /// The path to "no terminal": the displays that stay off, and the
    /// inputs that are set aside.
    pub off: Vec<String>,
    pub ignored: Vec<String>,
    pub unfit: usize,
    /// The draft has no rule and no terminal yet, so its first path takes
    /// the shipped default's shape: its inputs roam.
    pub empty: bool,
    /// The labels the person gave parts: each part's kind, its match as the
    /// part holds it, and its label. A label belongs to the part, not to a
    /// path, so revising a path leaves it as it is.
    pub labels: Vec<(Kind, String, String)>,
}

impl Doc {
    /// The label of the part of `kind` that matches by `sel`.
    pub fn label(&self, kind: Kind, sel: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(k, s, _)| *k == kind && s == sel)
            .map(|(_, _, l)| l.as_str())
    }

    /// Gives the part of `kind` that matches by `sel` the label `label`, or
    /// takes its label away when `label` is empty.
    pub fn set_label(&mut self, kind: Kind, sel: &str, label: &str) {
        match self
            .labels
            .iter_mut()
            .find(|(k, s, _)| *k == kind && s == sel)
        {
            Some(entry) if !label.is_empty() => label.clone_into(&mut entry.2),
            Some(_) => self.labels.retain(|(k, s, _)| !(*k == kind && s == sel)),
            None if !label.is_empty() => {
                self.labels.push((kind, sel.to_owned(), label.to_owned()));
            }
            None => {}
        }
    }

    /// The path of the terminal `name`.
    pub fn path(&self, name: &str) -> Option<&Path> {
        self.paths.iter().find(|p| p.name.as_deref() == Some(name))
    }

    /// Whether the input part `sel`, picked into a new path, roams once the
    /// path is added: it roams already, another path has it, or the new
    /// path is the configuration's first.
    pub fn would_roam(&self, sel: &str) -> bool {
        (self.empty && self.paths.is_empty())
            || self.roaming.iter().any(|r| r == sel)
            || self.paths.iter().any(|p| p.inputs.iter().any(|i| i == sel))
    }

    /// The configuration with `new` added: each input of `new` that roams
    /// by [`Doc::would_roam`], or every input of a path to the active
    /// terminal, goes to the roaming inputs, out of the path that had it,
    /// and the rest stay with `new`.
    pub fn with(&self, mut new: Path) -> Doc {
        if new.none {
            return self.with_none(new);
        }
        let mut doc = self.clone();
        let first = (doc.empty && doc.paths.is_empty()) || new.active;
        let mut own = Vec::new();
        for i in std::mem::take(&mut new.inputs) {
            if doc.roaming.contains(&i) {
                continue;
            }
            let mut held = false;
            for p in &mut doc.paths {
                if let Some(at) = p.inputs.iter().position(|x| *x == i) {
                    p.inputs.remove(at);
                    held = true;
                }
            }
            if held || first {
                doc.roaming.push(i);
            } else if !own.contains(&i) {
                own.push(i);
            }
        }
        new.inputs = own;
        if !new.active {
            doc.paths.push(new);
        }
        doc
    }

    /// The configuration with the path to no terminal `new` added: its
    /// displays join the displays that stay off, and its inputs the inputs
    /// set aside.
    fn with_none(&self, new: Path) -> Doc {
        let mut doc = self.clone();
        for d in new.displays {
            if !doc.off.contains(&d) {
                doc.off.push(d);
            }
        }
        for i in new.inputs {
            if !doc.ignored.contains(&i) {
                doc.ignored.push(i);
            }
        }
        doc
    }

    /// The configuration without its path to no terminal, and that path,
    /// for the editor.
    pub fn revise_none(&self) -> Option<(Doc, Path)> {
        if self.off.is_empty() && self.ignored.is_empty() {
            return None;
        }
        let mut rest = self.clone();
        let taken = Path {
            displays: std::mem::take(&mut rest.off),
            inputs: std::mem::take(&mut rest.ignored),
            none: true,
            ..Path::default()
        };
        Some((rest, taken))
    }

    /// The configuration without the path of the terminal `name`, and that
    /// path, which goes into the editor with its name kept: "Revise this
    /// path". The roaming inputs stay where they are.
    pub fn revise(&self, name: &str) -> Option<(Doc, Path)> {
        let at = self
            .paths
            .iter()
            .position(|p| p.name.as_deref() == Some(name))?;
        let mut rest = self.clone();
        let taken = rest.paths.remove(at);
        Some((rest, taken))
    }

    /// The configuration without its roaming inputs, and the path of the
    /// active terminal that holds them, for the editor.
    pub fn revise_active(&self) -> Option<(Doc, Path)> {
        if self.roaming.is_empty() {
            return None;
        }
        let mut rest = self.clone();
        let inputs = std::mem::take(&mut rest.roaming);
        Some((
            rest,
            Path {
                inputs,
                active: true,
                ..Path::default()
            },
        ))
    }
}

/// Whether the rule `rule` belongs to the configuration as paths: it
/// sends to a head, roams, turns a display off or sets an input aside.
/// Only a rule whose target the draft does not define fits no path.
fn fits(flow: &Flow, rule: &Rule) -> bool {
    matches!(flow.target(rule), Target::Head(_) | Target::Special(_))
}

/// Collapses the rules a part writes back into the part: the keyboard
/// classes into `any keyboard`, the pointer classes into `any pointer`.
fn collapse(inputs: &mut Vec<String>, part: &str, classes: &[&str]) {
    if !classes.iter().all(|c| inputs.iter().any(|s| s == c)) {
        return;
    }
    let at = inputs
        .iter()
        .position(|s| classes.contains(&s.as_str()))
        .unwrap_or(0);
    inputs.retain(|s| !classes.contains(&s.as_str()));
    inputs.insert(at.min(inputs.len()), part.to_owned());
}

/// Each selector once, in the order it first comes.
fn once(list: &mut Vec<String>) {
    let mut seen = Vec::new();
    list.retain(|s| {
        let fresh = !seen.contains(s);
        seen.push(s.clone());
        fresh
    });
}

/// The configuration of a draft's flow, as paths and roaming inputs.
pub fn read(flow: &Flow) -> Doc {
    let mut roaming: Vec<String> = flow
        .rules_of(Kind::Input)
        .filter(|(_, r)| flow.target(r) == Target::Special(Special::Roaming))
        .map(|(_, r)| r.selector.clone())
        .collect();
    once(&mut roaming);
    collapse(&mut roaming, ANY_POINTER, &POINTER_CLASSES);
    collapse(&mut roaming, ANY_KEYBOARD, &KEYBOARD_CLASSES);
    let to = |kind: Kind, special: Special| -> Vec<String> {
        let mut out: Vec<String> = flow
            .rules_of(kind)
            .filter(|(_, r)| flow.target(r) == Target::Special(special))
            .map(|(_, r)| r.selector.clone())
            .collect();
        once(&mut out);
        collapse(&mut out, ANY_POINTER, &POINTER_CLASSES);
        collapse(&mut out, ANY_KEYBOARD, &KEYBOARD_CLASSES);
        out
    };
    let (off, ignored) = (
        to(Kind::Display, Special::Off),
        to(Kind::Input, Special::Ignore),
    );
    let mut paths = Vec::new();
    for (h, head) in flow.heads.iter().enumerate() {
        let of = |kind: Kind| -> Vec<String> {
            flow.rules_of(kind)
                .filter(|(_, r)| flow.target(r) == Target::Head(h))
                .map(|(_, r)| r.selector.clone())
                .collect()
        };
        let mut inputs = of(Kind::Input);
        once(&mut inputs);
        collapse(&mut inputs, ANY_POINTER, &POINTER_CLASSES);
        collapse(&mut inputs, ANY_KEYBOARD, &KEYBOARD_CLASSES);
        let look = flow.look_of(h).and_then(|l| flow.looks.get(l));
        let mut settings: Vec<(String, String)> = head
            .keys
            .iter()
            .filter(|(k, _)| SETTINGS.contains(&k.as_str()))
            .cloned()
            .collect();
        if head.consoles != 1 {
            settings.insert(0, ("consoles".to_owned(), head.consoles.to_string()));
        }
        // a terminal no input rule names that is isolated has "no input"
        let no_input =
            inputs.is_empty() && settings.iter().any(|(k, v)| k == "isolated" && v == "yes");
        if no_input {
            settings.retain(|(k, _)| k != "isolated");
        }
        paths.push(Path {
            name: Some(head.name.clone()),
            displays: of(Kind::Display),
            inputs,
            font: look.map(|l| l.frit.clone()).filter(|f| !f.is_empty()),
            size: look.map(|l| l.points).filter(|p| *p > 0),
            settings,
            no_input,
            ..Path::default()
        });
    }
    let unfit = flow.rules.iter().filter(|r| !fits(flow, r)).count();
    // a label's line writes the first rule's match of the part, so the
    // first class of the pointers, or of the keyboards, reads back as
    // "every pointer" or "every keyboard", unless a rule of the
    // configuration names that one class alone
    let has = |part: &str| {
        paths
            .iter()
            .flat_map(|p| &p.inputs)
            .chain(&roaming)
            .chain(&ignored)
            .any(|s| s == part)
    };
    let wholes: Vec<(&str, &str)> = [ANY_POINTER, ANY_KEYBOARD]
        .into_iter()
        .filter_map(|part| expand(part).first().map(|first| (*first, part)))
        .filter(|(first, _)| !has(first))
        .collect();
    let labels = flow
        .labels
        .iter()
        .map(|(kind, sel, label)| {
            let sel = match wholes.iter().find(|(first, _)| first == sel) {
                Some((_, part)) if *kind == Kind::Input => (*part).to_owned(),
                _ => sel.clone(),
            };
            (*kind, sel, label.clone())
        })
        .collect();
    Doc {
        paths,
        roaming,
        off,
        ignored,
        unfit,
        empty: flow.rules.is_empty() && flow.heads.is_empty(),
        labels,
    }
}

/// The match a label's line writes for the part of `kind` that matches by
/// `sel`: exactly the match of the part's first rule, as "every keyboard"
/// writes `class:keyboard`.
pub fn label_match(kind: Kind, sel: &str) -> String {
    match kind {
        Kind::Display => sel.to_owned(),
        Kind::Input => expand(sel).first().copied().unwrap_or(sel).to_owned(),
    }
}

/// One line of `[labels]`: `display MATCH = LABEL` or
/// `input MATCH = LABEL`, with each `#` the comment rule would read
/// escaped.
fn label_line(kind: Kind, sel: &str, label: &str) -> String {
    let word = match kind {
        Kind::Display => "display",
        Kind::Input => "input",
    };
    format!("{word} {sel} = {}", vtium_settings::escape_label(label))
}

/// The `[labels]` lines `doc` says, given the rules in the order the draft
/// will hold them: a label whose part a rule uses goes where that rule
/// is, and the labels of parts no rule uses come after, in the order the
/// person gave them.
fn labels_plan(doc: &Doc, rules: &[(Line, bool)]) -> Vec<String> {
    let mut placed: Vec<(usize, usize, String)> = doc
        .labels
        .iter()
        .enumerate()
        .map(|(i, (kind, sel, label))| {
            let written = label_match(*kind, sel);
            let at = rules
                .iter()
                .position(|(l, _)| l.kind == *kind && l.selector == written)
                .unwrap_or(usize::MAX);
            (at, i, label_line(*kind, &written, label))
        })
        .collect();
    placed.sort_by_key(|(at, i, _)| (*at, *i));
    placed.into_iter().map(|(_, _, line)| line).collect()
}

/// Whether `flow` is vtium's built-in default: its paths and roaming
/// inputs are the shipped example's.
pub fn is_default(flow: &Flow) -> bool {
    let shipped = read(&crate::flowmodel::build(
        crate::drafts::EXAMPLE,
        &crate::flowmodel::Present::default(),
        None,
    ));
    let doc = read(flow);
    doc.paths == shipped.paths && doc.roaming == shipped.roaming
}

/// The name a new terminal takes: `console`, the shipped default's, while
/// the configuration has no terminal; else `terminal-N`, the smallest N no
/// head and no path has. A head's name holds no space.
pub fn new_name(flow: &Flow, doc: &Doc) -> String {
    new_name_besides(flow, doc, &[])
}

/// The name a new terminal takes, as [`new_name`] gives it, besides the
/// names in `made`, the new terminals the person made in PARTS.
pub fn new_name_besides(flow: &Flow, doc: &Doc, made: &[String]) -> String {
    let taken = |n: &str| {
        flow.heads.iter().any(|h| h.name == n)
            || doc.path(n).is_some()
            || made.iter().any(|m| m == n)
    };
    if flow.heads.is_empty() && doc.paths.is_empty() && !taken("console") {
        return "console".to_owned();
    }
    (1usize..)
        .map(|n| format!("terminal-{n}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| "terminal".to_owned())
}

/// How specific the selector of a rule of `kind` is, by ADR-1403 D8's
/// ranks, from 1 for one device to 7 for the catch-all. vtium-settings
/// gives the rank ([`vtium_settings::Match::rank`]), so vtium-config and
/// the section writer keep one order. The parts' own words, All keyboards
/// and All mice and touchpads, rank as the classes they stand for, and a
/// selector vtium cannot read ranks with the catch-all.
pub fn rank(kind: Kind, selector: &str) -> u8 {
    let selector = expand(selector).first().copied().unwrap_or(selector);
    let word = match kind {
        Kind::Display => "display",
        Kind::Input => "input",
    };
    let text = format!("version = 1\n[head h]\n[rules]\n{word} {selector} -> h\n");
    vtium_settings::parse_sections(&text)
        .ok()
        .flatten()
        .and_then(|p| p.config.rules.first().map(|r| r.matcher.rank()))
        .unwrap_or(vtium_settings::Match::RANK_ANY)
}

/// Where a rule goes among the rules [`plan`] writes: the display rules
/// before the input rules, each kind most specific first, and the classes
/// in the order the shipped default lists them.
fn order(kind: Kind, selector: &str) -> (u8, u8, usize) {
    let class = CLASS_ORDER
        .iter()
        .position(|c| *c == selector)
        .unwrap_or(CLASS_ORDER.len());
    let at = match kind {
        Kind::Display => 0,
        Kind::Input => 1,
    };
    (at, rank(kind, selector), class)
}

/// What adding the new path does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adding {
    /// It is a new path.
    New,
    /// The configuration already holds this path, in the terminal named:
    /// nothing changes.
    Same(String),
    /// Every input of the path to the active terminal roams already.
    SameActive,
    /// Every display of the path to no terminal is off already, and every
    /// input set aside.
    SameNone,
}

/// Whether `new` may be added to the configuration `doc`, or why not. A
/// path whose displays, font set and size a path of `doc` has, and whose
/// inputs that path or the roaming inputs all have, is already held, and
/// adding it changes nothing.
pub fn check_add(flow: &Flow, doc: &Doc, new: &Path) -> Result<Adding, String> {
    if !new.complete(doc) {
        return Err(new.status(doc).to_owned());
    }
    if new.active {
        return Ok(if new.inputs.iter().all(|i| doc.roaming.contains(i)) {
            Adding::SameActive
        } else {
            Adding::New
        });
    }
    let name = |p: &Path| p.name.clone().unwrap_or_default();
    let taken = |d: &str| {
        let (title, _) = title(flow, Kind::Display, d);
        if let Some(p) = doc.paths.iter().find(|p| p.displays.iter().any(|x| x == d)) {
            return Some(words::display_taken(&title, &name(p)));
        }
        (!new.none && doc.off.iter().any(|x| x == d)).then(|| words::display_off(&title))
    };
    if new.none {
        if let Some(why) = new.displays.iter().find_map(|d| taken(d)) {
            return Err(why);
        }
        let held = new.displays.iter().all(|d| doc.off.contains(d))
            && new.inputs.iter().all(|i| doc.ignored.contains(i));
        return Ok(if held { Adding::SameNone } else { Adding::New });
    }
    if let Some(n) = &new.name
        && !valid_name(n)
    {
        return Err(words::name_invalid(n));
    }
    let set = |v: &[String]| {
        let mut v = v.to_vec();
        v.sort();
        v.dedup();
        v
    };
    let held = doc.paths.iter().find(|p| {
        set(&p.displays) == set(&new.displays)
            && p.no_input == new.no_input
            && new
                .inputs
                .iter()
                .all(|i| p.inputs.contains(i) || doc.roaming.contains(i))
    });
    if let Some(held) = held {
        return if held.font == new.font && held.size == new.size {
            Ok(Adding::Same(name(held)))
        } else {
            Err(words::path_other_font(&name(held)))
        };
    }
    if let Some(n) = &new.name
        && doc.path(n).is_some()
    {
        return Err(words::name_taken(n));
    }
    if let Some(why) = new.displays.iter().find_map(|d| taken(d)) {
        return Err(why);
    }
    Ok(Adding::New)
}

/// Whether `name` may name a terminal: 1 to 64 of `A-Z a-z 0-9 . _ + -`,
/// and none of the words a rule sends to that are not terminals.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
        && !Special::ALL.iter().any(|s| s.name() == name)
}

/// The look a path's font set and size draw with: one of the draft's that
/// names that frit at that size, else a new look of the frit's name, with
/// the size after it when it has one; `None` for the default font set at
/// the automatic size.
fn look_for(
    flow: &Flow,
    font: Option<&str>,
    size: Option<u32>,
    added: &mut Vec<Change>,
) -> Option<String> {
    if font.is_none() && size.is_none() {
        return None;
    }
    let frit = font.unwrap_or_default();
    let points = size.unwrap_or(0);
    if let Some(l) = flow
        .looks
        .iter()
        .find(|l| l.frit == frit && l.points == points)
    {
        return Some(l.name.clone());
    }
    let name = match (font, size) {
        (Some(f), None) => f.to_owned(),
        (Some(f), Some(s)) => format!("{f}-{s}"),
        (None, Some(s)) => format!("size-{s}"),
        (None, None) => String::new(),
    };
    let fresh = added
        .iter()
        .all(|c| !matches!(c, Change::LookAdd { name: n, .. } if *n == name));
    if fresh && !flow.looks.iter().any(|l| l.name == name) {
        added.push(Change::LookAdd {
            name: name.clone(),
            frit: font.map(str::to_owned),
            size: size.map(|s| s.to_string()),
        });
    }
    Some(name)
}

/// One rule as [`plan`] writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    kind: Kind,
    selector: String,
    target: String,
}

impl Line {
    fn new(kind: Kind, selector: &str, target: &str) -> Line {
        Line {
            kind,
            selector: selector.to_owned(),
            target: target.to_owned(),
        }
    }

    fn of(rule: &Rule) -> Line {
        Line::new(rule.kind, &rule.selector, &rule.target)
    }

    fn order(&self) -> (u8, u8, usize) {
        order(self.kind, &self.selector)
    }

    fn text(&self) -> String {
        let kind = match self.kind {
            Kind::Display => "display",
            Kind::Input => "input",
        };
        format!("{kind} {} -> {}", self.selector, self.target)
    }
}

/// The heads and looks that make the draft's heads the paths of `doc`:
/// the heads no path has go, and each path's head is added or has its
/// look and settings set.
fn heads_plan(flow: &Flow, doc: &Doc) -> Vec<Change> {
    let mut looks = Vec::new();
    let mut heads = Vec::new();
    for p in &doc.paths {
        let Some(name) = p.name.clone() else {
            continue;
        };
        let look = look_for(flow, p.font.as_deref(), p.size, &mut looks);
        match flow.heads.iter().find(|h| h.name == name) {
            None => {
                heads.push(Change::HeadAdd {
                    name: name.clone(),
                    look,
                });
                for key in SETTINGS {
                    let value = p.wanted(key);
                    if !value.is_empty() {
                        heads.push(Change::HeadKey {
                            name: name.clone(),
                            key,
                            value: Some(value),
                        });
                    }
                }
            }
            Some(h) => {
                let now = (h.look != "default").then(|| h.look.clone());
                if now != look {
                    heads.push(Change::HeadKey {
                        name: name.clone(),
                        key: "look",
                        value: look,
                    });
                }
                // the settings the path changed in place
                for key in SETTINGS {
                    let have = match key {
                        "consoles" if h.consoles != 1 => h.consoles.to_string(),
                        "consoles" => String::new(),
                        _ => h
                            .keys
                            .iter()
                            .find(|(k, _)| k == key)
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default(),
                    };
                    let want = p.wanted(key);
                    if have != want {
                        heads.push(Change::HeadKey {
                            name: name.clone(),
                            key,
                            value: Some(want).filter(|v| !v.is_empty()),
                        });
                    }
                }
            }
        }
    }
    let gone = flow
        .heads
        .iter()
        .filter(|h| doc.path(&h.name).is_none())
        .map(|h| Change::HeadRm(h.name.clone()));
    gone.chain(looks).chain(heads).collect()
}

/// The draft's rules as `doc` says them, in order, each marked when it is
/// new. The rules the draft has and `doc` still says stay where they are,
/// with the rules that belong to no path, so a configuration that changed
/// nothing writes nothing. Each new rule goes most specific first: before
/// the first rule of its kind that is wider, else after the last rule of
/// its kind. The first path of a configuration with no rules also brings
/// the shipped default's last rule, which sets aside every input no rule
/// names.
fn rules_plan(flow: &Flow, doc: &Doc) -> Vec<(Line, bool)> {
    let mut want: Vec<Line> = Vec::new();
    let mut push = |l: Line| {
        if !want.contains(&l) {
            want.push(l);
        }
    };
    for p in &doc.paths {
        let Some(name) = &p.name else {
            continue;
        };
        for d in &p.displays {
            push(Line::new(Kind::Display, d, name));
        }
        for i in &p.inputs {
            // an input two paths name roams, should a path name it twice
            let paths = doc.paths.iter().filter(|q| q.inputs.contains(i)).count();
            let target = if paths > 1 { "roaming" } else { name.as_str() };
            for s in expand(i) {
                push(Line::new(Kind::Input, s, target));
            }
        }
    }
    for r in &doc.roaming {
        for s in expand(r) {
            push(Line::new(Kind::Input, s, "roaming"));
        }
    }
    for d in &doc.off {
        push(Line::new(Kind::Display, d, "off"));
    }
    for i in &doc.ignored {
        for s in expand(i) {
            push(Line::new(Kind::Input, s, "ignore"));
        }
    }
    if flow.rules.is_empty() && !doc.paths.is_empty() {
        push(Line::new(Kind::Input, "*", "ignore"));
    }
    want.sort_by_key(Line::order);
    let mut out: Vec<(Line, bool)> = flow
        .rules
        .iter()
        .filter(|r| !fits(flow, r) || want.contains(&Line::of(r)))
        .map(|r| (Line::of(r), false))
        .collect();
    for n in want {
        if out.iter().any(|(l, _)| *l == n) {
            continue;
        }
        let wider = out
            .iter()
            .position(|(l, _)| l.kind == n.kind && l.order() > n.order());
        let after = out
            .iter()
            .rposition(|(l, _)| l.kind == n.kind)
            .map(|i| i.saturating_add(1));
        let at = wider.or(after).unwrap_or(match n.kind {
            Kind::Display => 0,
            Kind::Input => out.len(),
        });
        out.insert(at, (n, true));
    }
    out
}

/// The draft's changes that make its heads, looks and rules the
/// configuration `doc`: the rules `doc` no longer says go, the heads no
/// path has go, each path's head is added or has its look set, and each
/// new rule is added where [`rules_plan`] puts it. A draft that already
/// says all this is left as it is.
pub fn plan(flow: &Flow, doc: &Doc) -> Vec<Change> {
    let mut out = Vec::new();
    let rules = rules_plan(flow, doc);
    let kept: Vec<&Line> = rules.iter().filter(|(_, n)| !n).map(|(l, _)| l).collect();
    // the rules that go, from the last, so the numbers stay right
    for (i, r) in flow.rules.iter().enumerate().rev() {
        if !kept.contains(&&Line::of(r)) {
            out.push(Change::RuleRm(i.saturating_add(1)));
        }
    }
    out.extend(heads_plan(flow, doc));
    for (i, (l, added)) in rules.iter().enumerate() {
        if *added {
            out.push(Change::RuleAddBefore {
                line: l.text(),
                before: i.saturating_add(1),
            });
        }
    }
    // the labels last, so their section stays after `[rules]`
    let labels = labels_plan(doc, &rules);
    let now: Vec<String> = flow
        .labels
        .iter()
        .map(|(kind, sel, label)| label_line(*kind, sel, label))
        .collect();
    if labels != now {
        out.push(Change::Labels(labels));
    }
    out
}

/// The connected devices of `kind` a selector names.
fn named<'a>(flow: &'a Flow, kind: Kind, selector: &'a str) -> impl Iterator<Item = &'a Device> {
    flow.devices(kind)
        .iter()
        .filter(move |d| d.found && d.keys.iter().any(|k| k == selector))
}

/// A path's port, without the USB interface after it:
/// `pci-0:14.0-usb-0:3.3:1.0` is on `pci-0:14.0-usb-0:3.3`.
pub fn port_of(path: &str) -> String {
    match path.rsplit_once(':') {
        Some((port, iface))
            if path.contains("-usb-")
                && port.contains("-usb-")
                && port.rsplit_once(':').is_some()
                && iface.contains('.') =>
        {
            port.to_owned()
        }
        _ => path.to_owned(),
    }
}

/// The key a connected device is picked by from the palette: its serial
/// when it has one, else its id when no other input shares it, else its
/// path; for a display, its EDID, else its connector.
pub fn own_key(flow: &Flow, kind: Kind, dev: &Device) -> String {
    let key = |prefix: &str| dev.keys.iter().find(|k| k.starts_with(prefix)).cloned();
    let first = || dev.keys.first().cloned().unwrap_or_default();
    match kind {
        Kind::Display => key("edid:")
            .or_else(|| key("connector:"))
            .unwrap_or_else(first),
        Kind::Input => key("serial:")
            .or_else(|| {
                key("id:").filter(|id| {
                    flow.inputs
                        .iter()
                        .filter(|d| d.found && d.keys.contains(id))
                        .count()
                        == 1
                })
            })
            .or_else(|| key("path:"))
            .unwrap_or_else(first),
    }
}

/// A piece's title and the detail under it, in a reader's words.
pub fn title(flow: &Flow, kind: Kind, selector: &str) -> (String, String) {
    let (key, value) = selector.split_once(':').unwrap_or((selector, ""));
    let dev = named(flow, kind, selector).next();
    let gone = || {
        flow.devices(kind)
            .iter()
            .find(|d| {
                !d.found
                    && d.rule
                        .and_then(|r| flow.rules.get(r))
                        .is_some_and(|r| r.selector == selector)
            })
            .map(|d| d.name.clone())
            .unwrap_or_else(|| selector.to_owned())
    };
    match (kind, key) {
        (Kind::Display, "*") => (
            words::ANY_DISPLAY.to_owned(),
            words::EVERY_DISPLAY.to_owned(),
        ),
        (Kind::Input, "*") => (
            words::ANY_INPUT_TITLE.to_owned(),
            "what no other rule names".to_owned(),
        ),
        (Kind::Display, "connector") => {
            let c = value.rsplit('/').next().unwrap_or(value);
            (c.to_owned(), words::BY_CONNECTOR.to_owned())
        }
        (Kind::Display, "edid") => {
            let serial = value.rsplit(':').next().unwrap_or(value);
            match dev {
                Some(d) => (d.name.clone(), format!("{serial} · by EDID")),
                None => (format!("display {serial}"), words::NOT_PLUGGED.to_owned()),
            }
        }
        (Kind::Input, "class") if selector == ANY_POINTER => (
            words::ANY_POINTER_TITLE.to_owned(),
            words::EVERY_POINTER.to_owned(),
        ),
        (Kind::Input, "class") if selector == ANY_KEYBOARD => (
            words::ANY_KEYBOARD_TITLE.to_owned(),
            words::EVERY_KEYBOARD.to_owned(),
        ),
        (Kind::Input, "class") => (format!("every {value}"), words::EVERY_OF_TYPE.to_owned()),
        (Kind::Input, "path") => match dev {
            Some(d) => (d.name.clone(), port_words(&d.detail)),
            None => {
                let on: Vec<&Device> = flow
                    .inputs
                    .iter()
                    .filter(|d| d.found && d.keys.iter().any(|k| k.starts_with(selector)))
                    .collect();
                let what = if !on.is_empty() && on.iter().all(|d| sort(d) == Sort::Keyboard) {
                    "keyboards"
                } else {
                    "inputs"
                };
                let port = port_words(value);
                if on.is_empty() && !value.contains("usb") {
                    (gone(), words::NOT_PLUGGED.to_owned())
                } else {
                    (
                        format!("{what} on {port}"),
                        words::EVERY_INPUT_ON_PORT.to_owned(),
                    )
                }
            }
        },
        (Kind::Input, "id") => match dev {
            Some(d) => (format!("{} (by id)", d.name), format!("id {value}")),
            None => (
                format!("devices with id {value}"),
                words::NOT_PLUGGED.to_owned(),
            ),
        },
        (Kind::Input, "serial") => match dev {
            Some(d) => (format!("{} (by serial)", d.name), port_words(&d.detail)),
            None => (gone(), words::NOT_PLUGGED.to_owned()),
        },
        // a name, a card, a hub port, a typed match: the one device it
        // matches by its name, else the match as it is written
        _ => {
            let mut on = flow
                .devices(kind)
                .iter()
                .filter(|d| d.found && super::parts::matches(d, selector));
            match (dev.or_else(|| on.next()), on.next()) {
                (Some(d), None) => (d.name.clone(), selector.to_owned()),
                _ => (selector.to_owned(), String::new()),
            }
        }
    }
}

/// The steps a piece of sort `s` narrows through, broad to narrow, each its
/// title and its selector: any; each port of a connected device of that
/// sort; each device by its own key, its id and its serial. A display
/// narrows from any display to each connector and each display by its
/// EDID.
pub fn narrow(flow: &Flow, s: Sort) -> Vec<(String, String)> {
    let mut sels: Vec<String> = Vec::new();
    let mut add = |sel: String| {
        if !sel.is_empty() && !sels.contains(&sel) {
            sels.push(sel);
        }
    };
    let key = |d: &Device, prefix: &str| d.keys.iter().find(|k| k.starts_with(prefix)).cloned();
    match s {
        Sort::Screen => {
            add("*".to_owned());
            let shown: Vec<&Device> = flow.displays.iter().filter(|d| d.found).collect();
            for d in &shown {
                add(key(d, "connector:").unwrap_or_default());
            }
            for d in &shown {
                add(key(d, "edid:").unwrap_or_default());
            }
            return sels
                .into_iter()
                .map(|s| (title(flow, Kind::Display, &s).0, s))
                .collect();
        }
        Sort::Keyboard => add(ANY_KEYBOARD.to_owned()),
        Sort::Pointer => add(ANY_POINTER.to_owned()),
        Sort::Other => {}
    }
    let these: Vec<&Device> = flow
        .inputs
        .iter()
        .filter(|d| d.found && sort(d) == s)
        .collect();
    for d in &these {
        if let Some(p) = key(d, "path:") {
            add(port_of(&p));
        }
    }
    for d in &these {
        add(own_key(flow, Kind::Input, d));
        add(key(d, "id:").unwrap_or_default());
        add(key(d, "serial:").unwrap_or_default());
    }
    sels.into_iter()
        .map(|s| (title(flow, Kind::Input, &s).0, s))
        .collect()
}

/// The sort a piece's selector is about, for its narrowing steps.
pub fn sort_of(flow: &Flow, piece: &Piece) -> Sort {
    match piece {
        Piece::Display(_) => Sort::Screen,
        Piece::Font(_)
        | Piece::Active
        | Piece::NoInput
        | Piece::NoTerminal
        | Piece::Terminal(_) => Sort::Other,
        Piece::Input(s) if s == ANY_KEYBOARD || KEYBOARD_CLASSES.contains(&s.as_str()) => {
            Sort::Keyboard
        }
        Piece::Input(s) if s == ANY_POINTER || POINTER_CLASSES.contains(&s.as_str()) => {
            Sort::Pointer
        }
        Piece::Input(s) => flow
            .inputs
            .iter()
            .find(|d| d.found && d.keys.iter().any(|k| k == s || k.starts_with(s.as_str())))
            .map_or(Sort::Keyboard, sort),
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
pub(crate) mod tests {
    use super::super::flow::{Head, Look, RuleState};
    use super::*;

    fn dev(name: &str, detail: &str, keys: &[&str], found: bool, rule: Option<usize>) -> Device {
        Device {
            name: name.into(),
            detail: detail.into(),
            keys: keys.iter().map(|k| (*k).to_owned()).collect(),
            found,
            rule,
        }
    }

    fn rule(kind: Kind, selector: &str, target: &str) -> Rule {
        Rule {
            kind,
            line: 1,
            selector: selector.into(),
            target: target.into(),
            state: RuleState::Matches,
        }
    }

    pub(crate) const KBD_PATH: &str = "path:pci-0000:00:14.0-usb-0:3.3:1.0";
    pub(crate) const PORT_33: &str = "path:pci-0000:00:14.0-usb-0:3.3";
    pub(crate) const EDID_A: &str = "edid:AAA:0x0001:LARGE00001";
    pub(crate) const EDID_B: &str = "edid:BBB:0x0002:SMALL00001";

    /// Two terminals, desk and panel; a keyboard and a mouse on one
    /// receiver that roam between them by its id; a keyboard no rule names;
    /// a power button; a keyboard a rule names that is not plugged in; and a
    /// catch-all rule that belongs to no path.
    pub(crate) fn flow() -> Flow {
        use Kind::{Display, Input};
        Flow {
            displays: vec![
                dev(
                    "DP-8",
                    "LARGE00001",
                    &[EDID_A, "connector:card1/DP-8"],
                    true,
                    Some(0),
                ),
                dev(
                    "HDMI-A-2",
                    "SMALL00001",
                    &[EDID_B, "connector:card1/HDMI-A-2"],
                    true,
                    Some(1),
                ),
            ],
            inputs: vec![
                dev("046d:c52b:0", "", &[], false, Some(2)),
                dev(
                    "Logitech USB Receiver",
                    "pci-0000:00:14.0-usb-0:3.3:1.0",
                    &[KBD_PATH, "id:046d:c31c", "class:keyboard"],
                    true,
                    Some(3),
                ),
                dev(
                    "Logitech USB Receiver Mouse",
                    "pci-0000:00:14.0-usb-0:3.3:1.1",
                    &[
                        "path:pci-0000:00:14.0-usb-0:3.3:1.1",
                        "id:046d:c31c",
                        "class:mouse",
                    ],
                    true,
                    Some(3),
                ),
                dev(
                    "Power Button",
                    "acpi-LNXPWRBN:00",
                    &["path:acpi-LNXPWRBN:00", "class:key"],
                    true,
                    Some(4),
                ),
                dev(
                    "8BitDo Keyboard",
                    "pci-0000:00:14.0-usb-0:3.4:1.0",
                    &[
                        "path:pci-0000:00:14.0-usb-0:3.4:1.0",
                        "id:2dc8:5200",
                        "class:keyboard",
                    ],
                    true,
                    None,
                ),
            ],
            rules: vec![
                rule(Display, EDID_A, "desk"),
                rule(Display, EDID_B, "panel"),
                rule(Input, "serial:046d:c52b:0", "desk"),
                rule(Input, "id:046d:c31c", "roaming"),
                rule(Input, "*", "ignore"),
            ],
            heads: vec![
                Head {
                    name: "desk".into(),
                    consoles: 1,
                    look: "big".into(),
                    keys: Vec::new(),
                },
                Head {
                    name: "panel".into(),
                    consoles: 1,
                    look: "default".into(),
                    keys: Vec::new(),
                },
            ],
            looks: vec![Look {
                name: "big".into(),
                frit: "monoid".into(),
                points: 14,
                keys: Vec::new(),
                fonts: Vec::new(),
                problem: None,
            }],
            policy: "open".into(),
            read: true,
            ..Flow::default()
        }
    }

    fn path(name: Option<&str>, displays: &[&str], inputs: &[&str], font: Option<&str>) -> Path {
        Path {
            name: name.map(str::to_owned),
            displays: displays.iter().map(|s| (*s).to_owned()).collect(),
            inputs: inputs.iter().map(|s| (*s).to_owned()).collect(),
            font: font.map(str::to_owned),
            ..Path::default()
        }
    }

    fn sized(mut p: Path, size: u32) -> Path {
        p.size = Some(size);
        p
    }

    #[test]
    fn a_draft_reads_as_paths_and_its_roaming_inputs() {
        let doc = read(&flow());
        assert_eq!(
            doc.paths,
            [
                sized(
                    path(
                        Some("desk"),
                        &[EDID_A],
                        &["serial:046d:c52b:0"],
                        Some("monoid")
                    ),
                    14
                ),
                path(Some("panel"), &[EDID_B], &[], None),
            ]
        );
        // the receiver roams, so it is drawn once, not on each path
        assert_eq!(doc.roaming, ["id:046d:c31c"]);
        // the catch-all to ignore reads as the path to no terminal
        assert_eq!(doc.ignored, ["*"]);
        assert_eq!(doc.unfit, 0);
    }

    #[test]
    fn a_path_is_complete_with_an_input_and_a_display() {
        let mut p = Path::default();
        assert_eq!(p.status(&Doc::default()), words::PATH_EMPTY);
        p.pick(Piece::Input(KBD_PATH.into()));
        assert!(!p.complete(&Doc::default()));
        assert_eq!(p.status(&Doc::default()), words::PATH_NEEDS_DISPLAY);
        p.pick(Piece::Display(EDID_A.into()));
        assert!(p.complete(&Doc::default()));
        assert_eq!(p.status(&Doc::default()), words::PATH_COMPLETE);
        // a second pick takes the piece out again, and a terminal with a
        // display alone is a path roaming inputs reach
        assert_eq!(p.pick(Piece::Input(KBD_PATH.into())), Picked::Removed);
        assert!(p.complete(&Doc::default()));
        assert_eq!(p.status(&Doc::default()), words::PATH_COMPLETE_DISPLAYS);
    }

    #[test]
    fn a_second_font_set_takes_the_first_one_s_place() {
        let mut p = Path::default();
        assert_eq!(p.pick(Piece::Font(Some("monoid".into()))), Picked::Added);
        assert_eq!(
            p.pick(Piece::Font(Some("dejavu".into()))),
            Picked::Replaced(Some("monoid".into()))
        );
        assert_eq!(p.font.as_deref(), Some("dejavu"));
        assert_eq!(p.pick(Piece::Font(Some("dejavu".into()))), Picked::Removed);
        assert_eq!(p.font, None);
    }

    #[test]
    fn any_keyboard_brings_any_pointer_into_a_path_without_one() {
        let f = flow();
        let mut p = Path::default();
        assert_eq!(
            p.pick_part(&f, Piece::Input(ANY_KEYBOARD.into())),
            Picked::AddedWithPointers
        );
        assert_eq!(p.inputs, [ANY_KEYBOARD, ANY_POINTER]);
        // taking any keyboard out leaves the pointers
        assert_eq!(
            p.pick_part(&f, Piece::Input(ANY_KEYBOARD.into())),
            Picked::Removed
        );
        assert_eq!(p.inputs, [ANY_POINTER]);
        // a path that has a pointer already takes the keyboards alone
        let mouse = "path:pci-0000:00:14.0-usb-0:3.3:1.1";
        let mut p = Path::default();
        p.pick_part(&f, Piece::Input(mouse.into()));
        assert_eq!(
            p.pick_part(&f, Piece::Input(ANY_KEYBOARD.into())),
            Picked::Added
        );
        assert_eq!(p.inputs, [mouse, ANY_KEYBOARD]);
    }

    #[test]
    fn adding_a_path_the_document_has_changes_nothing() {
        let f = flow();
        let doc = read(&f);
        // the same pieces in another order, the roaming receiver among them
        let same = sized(
            path(
                None,
                &[EDID_A],
                &["id:046d:c31c", "serial:046d:c52b:0"],
                Some("monoid"),
            ),
            14,
        );
        assert_eq!(check_add(&f, &doc, &same), Ok(Adding::Same("desk".into())));
        // fewer inputs through the same display are held too
        let fewer = sized(path(None, &[EDID_A], &["id:046d:c31c"], Some("monoid")), 14);
        assert_eq!(check_add(&f, &doc, &fewer), Ok(Adding::Same("desk".into())));
        // the same pieces with another font set conflict
        let other = path(
            None,
            &[EDID_A],
            &["id:046d:c31c", "serial:046d:c52b:0"],
            None,
        );
        let why = check_add(&f, &doc, &other).expect_err("a conflict");
        assert!(why.starts_with("The terminal desk already has"), "{why}");
    }

    #[test]
    fn a_display_is_in_one_path_only() {
        let f = flow();
        let doc = read(&f);
        let p = path(None, &[EDID_B], &[KBD_PATH], None);
        let why = check_add(&f, &doc, &p).expect_err("a conflict");
        assert_eq!(
            why,
            "The display HDMI-A-2 is already in the terminal panel. A display can be in one path only."
        );
        // an input already in a path may join another, and will roam
        let p = path(None, &["connector:card1/DP-9"], &["id:046d:c31c"], None);
        assert_eq!(check_add(&f, &doc, &p), Ok(Adding::New));
        // a name another terminal has, or one a terminal cannot take
        let p = path(Some("desk"), &["connector:card1/DP-9"], &[KBD_PATH], None);
        assert_eq!(
            check_add(&f, &doc, &p),
            Err(
                "The name desk is already the name of another terminal. Choose another name."
                    .to_owned()
            )
        );
        let p = path(
            Some("my desk"),
            &["connector:card1/DP-9"],
            &[KBD_PATH],
            None,
        );
        assert!(check_add(&f, &doc, &p).is_err_and(|e| e.starts_with("A terminal's name")));
        // an incomplete path says what it needs
        let p = path(None, &[], &[KBD_PATH], None);
        assert_eq!(
            check_add(&f, &doc, &p),
            Err(words::PATH_NEEDS_DISPLAY.to_owned())
        );
    }

    #[test]
    fn any_keyboard_narrows_to_a_port_then_to_one_keyboard() {
        let f = flow();
        let steps = narrow(&f, Sort::Keyboard);
        let titles: Vec<&str> = steps.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            titles,
            [
                "All keyboards",
                "inputs on USB 3.3",
                "keyboards on USB 3.4",
                "Logitech USB Receiver",
                "Logitech USB Receiver (by id)",
                "8BitDo Keyboard (by id)",
            ]
        );
        assert_eq!(steps[1].1, PORT_33);
        // a narrowed piece takes its place in the new path
        let mut p = path(None, &[], &["class:keyboard"], None);
        p.replace(
            &Piece::Input("class:keyboard".into()),
            Piece::Input(PORT_33.into()),
        );
        assert_eq!(p.inputs, [PORT_33]);
        let screens: Vec<String> = narrow(&f, Sort::Screen)
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        assert_eq!(
            screens,
            ["All displays", "DP-8", "HDMI-A-2", "DP-8", "HDMI-A-2"]
        );
    }

    /// The draft whose flow [`flow`]'s heads, looks and rules stand for.
    pub(crate) const TEXT: &str = "version = 1\npolicy = open\n[look big]\nfont-name = monoid\nfont-size = 14\n[head desk]\nlook = big\n[head panel]\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ndisplay edid:BBB:0x0002:SMALL00001 -> panel\ninput serial:046d:c52b:0 -> desk\ninput id:046d:c31c -> roaming\ninput * -> ignore\n";

    fn apply(text: &str, changes: &[Change]) -> String {
        changes.iter().fold(text.to_owned(), |t, c| {
            c.apply(&t)
                .unwrap_or_else(|e| panic!("{c:?}: {}", e.text()))
        })
    }

    fn build(text: &str) -> Flow {
        crate::flowmodel::build(text, &crate::flowmodel::Present::default(), None)
    }

    /// The rule lines of `text`, each as it reads, without comments.
    fn rules_of(text: &str) -> Vec<String> {
        build(text)
            .rules
            .iter()
            .map(|r| Line::of(r).text())
            .collect()
    }

    #[test]
    fn the_rules_are_written_most_specific_first() {
        let f = build(TEXT);
        let doc = read(&f);
        assert_eq!(doc.paths.len(), 2);
        let name = new_name(&f, &doc);
        assert_eq!(name, "terminal-1");
        let new = path(
            Some(&name),
            &["*"],
            &[ANY_KEYBOARD, PORT_33, "serial:2dc8:5200:9", ANY_POINTER],
            Some("dejavu"),
        );
        let doc = doc.with(new.clone());
        let text = apply(TEXT, &plan(&f, &doc));
        let lines = rules_of(&text);
        // serial, then id, then port, then class, then the catch-alls
        assert_eq!(
            lines,
            [
                "display edid:AAA:0x0001:LARGE00001 -> desk",
                "display edid:BBB:0x0002:SMALL00001 -> panel",
                "display * -> terminal-1",
                "input serial:046d:c52b:0 -> desk",
                "input serial:2dc8:5200:9 -> terminal-1",
                // a port before a model (ADR-1403 D8)
                format!("input {PORT_33} -> terminal-1").as_str(),
                "input id:046d:c31c -> roaming",
                "input class:keyboard -> terminal-1",
                "input class:mouse -> terminal-1",
                "input class:touchpad -> terminal-1",
                "input class:touchscreen -> terminal-1",
                "input class:trackball -> terminal-1",
                "input class:pointingstick -> terminal-1",
                "input class:key -> terminal-1",
                "input * -> ignore",
            ]
        );
        // within each kind, from the most specific rule to the widest
        for kind in ["display ", "input "] {
            let ranks: Vec<u8> = lines
                .iter()
                .filter_map(|l| l.strip_prefix(kind))
                .map(|l| {
                    let k = if kind == "display " {
                        Kind::Display
                    } else {
                        Kind::Input
                    };
                    rank(k, l.split_whitespace().next().unwrap_or_default())
                })
                .collect();
            assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{kind}{ranks:?}");
        }
        assert!(
            text.contains("[head terminal-1]\nlook = dejavu\n"),
            "{text}"
        );
        assert!(
            text.contains("[look dejavu]\nfont-name = dejavu\n"),
            "{text}"
        );
        // read back, the draft is the same configuration
        let back = build(&text);
        let read_back = read(&back);
        assert_eq!(read_back.paths[..2], doc.paths[..2]);
        assert_eq!(
            read_back.paths[2].inputs,
            ["serial:2dc8:5200:9", PORT_33, ANY_KEYBOARD, ANY_POINTER]
        );
        assert_eq!(read_back.roaming, doc.roaming);
        assert_eq!(read_back.paths[2].font.as_deref(), Some("dejavu"));
        // and writing it again changes nothing
        assert_eq!(plan(&back, &read_back), []);
    }

    #[test]
    fn an_input_picked_into_a_second_path_roams_and_is_held_once() {
        let f = build(TEXT);
        let doc = read(&f);
        // desk's own keyboard, picked into a new path too
        let own = "serial:046d:c52b:0";
        assert!(doc.would_roam(own));
        assert!(doc.would_roam("id:046d:c31c"));
        assert!(!doc.would_roam(KBD_PATH));
        let new = path(
            Some("terminal-1"),
            &["connector:card1/DP-9"],
            &[own, KBD_PATH],
            None,
        );
        let doc = doc.with(new);
        assert_eq!(doc.roaming, ["id:046d:c31c", own]);
        assert!(doc.paths[0].inputs.is_empty());
        assert_eq!(doc.paths[2].inputs, [KBD_PATH]);
        let text = apply(TEXT, &plan(&f, &doc));
        assert!(text.contains(&format!("input {own} -> roaming")), "{text}");
        assert!(!text.contains(&format!("input {own} -> desk")), "{text}");
        // read back in the file's order, most specific first
        assert_eq!(read(&build(&text)).roaming, [own, "id:046d:c31c"]);
        // the first path of an empty configuration roams its inputs
        assert!(
            Doc {
                empty: true,
                ..Doc::default()
            }
            .would_roam(KBD_PATH)
        );
    }

    #[test]
    fn revising_a_path_returns_it_to_the_editor_and_frees_its_pieces() {
        let f = build(TEXT);
        let doc = read(&f);
        let (rest, taken) = doc.revise("desk").expect("desk's path");
        assert_eq!(taken, doc.paths[0]);
        let text = apply(TEXT, &plan(&f, &rest));
        assert!(!text.contains("[head desk]"), "{text}");
        assert_eq!(
            rules_of(&text),
            [
                "display edid:BBB:0x0002:SMALL00001 -> panel",
                "input id:046d:c31c -> roaming",
                "input * -> ignore",
            ]
        );
        // adding it back puts the draft as it was, the receiver roaming
        let back = build(&text);
        let doc2 = read(&back);
        assert_eq!(check_add(&back, &doc2, &taken), Ok(Adding::New));
        let again = apply(&text, &plan(&back, &doc2.with(taken)));
        assert!(again.contains("input id:046d:c31c -> roaming"), "{again}");
        assert!(
            again.contains("input serial:046d:c52b:0 -> desk"),
            "{again}"
        );
        assert!(again.contains("[head desk]\nlook = big\n"), "{again}");
        // the roaming inputs revised go into the editor with the active
        // terminal, and the path is complete without a display
        let (rest, taken) = doc.revise_active().expect("roams");
        assert!(rest.roaming.is_empty());
        assert_eq!(taken.inputs, ["id:046d:c31c"]);
        assert!(taken.active && taken.complete(&rest));
        assert_eq!(taken.status(&rest), words::ACTIVE_COMPLETE);
        let text = apply(TEXT, &plan(&f, &rest));
        assert!(!text.contains("id:046d:c31c"), "{text}");
    }

    fn default_doc() -> (Vec<Path>, Vec<String>) {
        (
            vec![path(Some("console"), &["*"], &[], None)],
            vec![ANY_KEYBOARD.to_owned(), ANY_POINTER.to_owned()],
        )
    }

    #[test]
    fn the_shipped_default_is_one_path_and_its_roaming_inputs() {
        let f = build(crate::drafts::EXAMPLE);
        let doc = read(&f);
        assert_eq!((doc.paths.clone(), doc.roaming.clone()), default_doc());
        assert_eq!(doc.ignored, ["*"], "the catch-all reads as a path");
        assert_eq!(doc.unfit, 0);
        assert!(is_default(&f));
        // writing it back changes nothing
        assert_eq!(plan(&f, &doc), []);
        // composing any keyboard and any display, with nothing else, is held
        let mut new = Path::default();
        new.pick_part(&f, Piece::Input(ANY_KEYBOARD.into()));
        new.pick_part(&f, Piece::Display("*".into()));
        assert_eq!(
            check_add(&f, &doc, &new),
            Ok(Adding::Same("console".into()))
        );
        assert!(!is_default(&flow()));
    }

    #[test]
    fn composing_the_default_from_parts_writes_the_shipped_default() {
        // n starts a configuration with no terminal and no rule
        let empty = crate::ops::new_text(crate::drafts::EXAMPLE);
        let f = build(&empty);
        let doc = read(&f);
        assert!(doc.paths.is_empty());
        // the person picks any keyboard and any display, and nothing else
        let mut new = Path::default();
        new.pick_part(&f, Piece::Input(ANY_KEYBOARD.into()));
        new.pick_part(&f, Piece::Display("*".into()));
        new.name = Some(new_name(&f, &doc));
        assert_eq!(new.name.as_deref(), Some("console"));
        assert_eq!(check_add(&f, &doc, &new), Ok(Adding::New));
        let text = apply(&empty, &plan(&f, &doc.with(new)));
        // the same rules as the shipped default, pointers and all, in its
        // order
        assert_eq!(rules_of(&text), rules_of(crate::drafts::EXAMPLE));
        let back = build(&text);
        let read_back = read(&back);
        assert_eq!(
            (read_back.paths.clone(), read_back.roaming.clone()),
            default_doc()
        );
        assert!(is_default(&back));
        // adding it once more changes nothing
        let mut again = Path::default();
        again.pick_part(&back, Piece::Input(ANY_KEYBOARD.into()));
        again.pick_part(&back, Piece::Display("*".into()));
        assert_eq!(
            check_add(&back, &read_back, &again),
            Ok(Adding::Same("console".into()))
        );
    }

    #[test]
    fn an_added_path_s_name_size_and_settings_change_in_place() {
        let f = build(TEXT);
        let mut doc = read(&f);
        doc.paths[0].size = Some(16);
        doc.paths[0].set("consoles", "3");
        doc.paths[1].name = Some("small".into());
        let text = apply(TEXT, &plan(&f, &doc));
        let back = build(&text);
        let read_back = read(&back);
        assert_eq!(read_back.paths[0].size, Some(16));
        assert_eq!(read_back.paths[0].font.as_deref(), Some("monoid"));
        assert_eq!(read_back.paths[0].setting("consoles"), "3");
        assert!(
            text.contains("[look monoid-16]\nfont-name = monoid\nfont-size = 16\n"),
            "{text}"
        );
        // a size alone moves no rule
        let mut sized_only = read(&f);
        sized_only.paths[1].size = Some(10);
        let changes = plan(&f, &sized_only);
        assert!(
            changes.iter().all(|c| !matches!(
                c,
                Change::RuleRm(_) | Change::RuleAdd(_) | Change::RuleAddBefore { .. }
            )),
            "{changes:?}"
        );
        // the renamed terminal keeps its display, and the receiver roams on
        assert_eq!(read_back.paths[1].name.as_deref(), Some("small"));
        assert_eq!(read_back.paths[1].displays, [EDID_B]);
        assert_eq!(read_back.roaming, ["id:046d:c31c"]);
        assert!(!text.contains("[head panel]"), "{text}");
    }

    /// Revises each item `text`'s configuration shows and adds it straight
    /// back, and says what the draft's text then is, each item by its name.
    fn round_trips(text: &str) -> Vec<(String, Result<String, String>)> {
        let f = build(text);
        let doc = read(&f);
        let mut out = Vec::new();
        let mut taken: Vec<(String, Option<(Doc, Path)>)> = doc
            .paths
            .iter()
            .filter_map(|p| p.name.clone())
            .map(|n| (n.clone(), doc.revise(&n)))
            .collect();
        if !doc.roaming.is_empty() {
            taken.push(("the active terminal".to_owned(), doc.revise_active()));
        }
        if !doc.off.is_empty() || !doc.ignored.is_empty() {
            taken.push(("no terminal".to_owned(), doc.revise_none()));
        }
        for (name, t) in taken {
            let (rest, path) = t.expect("revised");
            let back =
                check_add(&f, &rest, &path).map(|_| apply(text, &plan(&f, &rest.with(path))));
            out.push((name, back));
        }
        out
    }

    #[test]
    fn whatever_revise_takes_out_adds_straight_back_unchanged() {
        // one display, a font set and size, a roaming input
        let multi = TEXT.replace(
            "display edid:BBB:0x0002:SMALL00001 -> panel\n",
            "display edid:BBB:0x0002:SMALL00001 -> panel\ndisplay connector:card1/DP-9 -> panel\n",
        );
        // a narrowed any input: keyboards on one USB port
        let narrowed = TEXT.replace(
            "input serial:046d:c52b:0 -> desk\n",
            &format!("input serial:046d:c52b:0 -> desk\ninput {PORT_33} -> panel\n"),
        );
        let default = crate::drafts::EXAMPLE;
        let mut seen = Vec::new();
        for text in [TEXT, multi.as_str(), narrowed.as_str(), default] {
            for (name, back) in round_trips(text) {
                assert_eq!(back.as_deref(), Ok(text), "{name}");
                seen.push(name);
            }
        }
        for want in [
            "desk",
            "panel",
            "console",
            "the active terminal",
            "no terminal",
        ] {
            assert!(seen.iter().any(|s| s == want), "{want}: {seen:?}");
        }
    }

    /// A kiosk: desk shows a display and nothing types into it; panel
    /// shows a display, and no input rule names it and no roaming input
    /// reaches it; a display stays off; a keyboard and every input no rule
    /// names are set aside.
    const NOTHING: &str = "version = 1\npolicy = open\n[head desk]\nisolated = yes\n[head panel]\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ndisplay edid:BBB:0x0002:SMALL00001 -> panel\ndisplay connector:card1/DP-9 -> off\ninput serial:046d:c52b:0 -> ignore\ninput * -> ignore\n";

    #[test]
    fn every_rule_reads_as_a_path_and_the_nothing_parts_round_trip() {
        let doc = read(&build(NOTHING));
        assert_eq!(doc.unfit, 0, "every rule reads as a path");
        let desk = doc.path("desk").expect("desk");
        assert!(desk.no_input && desk.inputs.is_empty());
        assert_eq!(desk.setting("isolated"), "", "no input is the isolated key");
        let panel = doc.path("panel").expect("panel");
        assert!(!panel.no_input && panel.inputs.is_empty());
        assert_eq!(doc.off, ["connector:card1/DP-9"]);
        assert_eq!(doc.ignored, ["serial:046d:c52b:0", "*"]);
        let mut seen = Vec::new();
        for (name, back) in round_trips(NOTHING) {
            assert_eq!(back.as_deref(), Ok(NOTHING), "{name}");
            seen.push(name);
        }
        assert_eq!(seen, ["desk", "panel", "no terminal"]);
        // the display-only head of round 3 adds back without roaming inputs
        let (rest, panel) = doc.revise("panel").expect("panel");
        assert!(rest.roaming.is_empty() && panel.complete(&rest));
        assert_eq!(panel.status(&rest), words::PATH_COMPLETE_DISPLAYS);
    }

    #[test]
    fn no_input_writes_isolated_and_no_terminal_writes_off_and_ignore() {
        let empty = crate::ops::new_text(crate::drafts::EXAMPLE);
        let f = build(&empty);
        let doc = read(&f);
        // a kiosk: a display, a terminal, and no input
        let mut kiosk = path(Some("kiosk"), &[EDID_A], &[KBD_PATH], None);
        assert_eq!(kiosk.pick(Piece::NoInput), Picked::Added);
        assert!(kiosk.inputs.is_empty(), "no input takes the inputs' place");
        assert_eq!(kiosk.status(&doc), words::KIOSK_COMPLETE);
        let doc = doc.with(kiosk);
        // a display turned off and a keyboard set aside
        let mut none = path(
            None,
            &["connector:card1/DP-9"],
            &["serial:046d:c52b:0"],
            None,
        );
        none.pick(Piece::NoTerminal);
        assert_eq!(none.status(&doc), words::NONE_COMPLETE);
        let f2 = build(&apply(&empty, &plan(&f, &doc)));
        let doc2 = read(&f2);
        assert_eq!(check_add(&f2, &doc2, &none), Ok(Adding::New));
        let text = apply(&empty, &plan(&f, &doc.with(none.clone())));
        assert!(text.contains("[head kiosk]\nisolated = yes\n"), "{text}");
        assert_eq!(
            rules_of(&text),
            [
                "display edid:AAA:0x0001:LARGE00001 -> kiosk",
                "display connector:card1/DP-9 -> off",
                "input serial:046d:c52b:0 -> ignore",
                "input * -> ignore",
            ]
        );
        let back = read(&build(&text));
        assert!(back.path("kiosk").expect("kiosk").no_input);
        assert_eq!(back.off, ["connector:card1/DP-9"]);
        assert_eq!(back.ignored, ["serial:046d:c52b:0", "*"]);
        assert_eq!(check_add(&build(&text), &back, &none), Ok(Adding::SameNone));
        // a display that is off cannot join a terminal's path
        let p = path(Some("t"), &["connector:card1/DP-9"], &[], None);
        assert!(check_add(&build(&text), &back, &p).is_err_and(|e| e.contains("turned off")));
    }

    #[test]
    fn a_keyboard_placed_in_its_own_path_leaves_the_every_keyboard_terminal() {
        use vtium_settings::{Class, Identity};
        let text = "version = 1\n[head desk]\n[head panel]\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ndisplay edid:BBB:0x0002:SMALL00001 -> panel\ninput class:keyboard -> desk\ninput class:key -> desk\n";
        let mut kbd = Identity::input().with_class(Class::Keyboard);
        kbd.name = Some("8BitDo Keyboard".to_owned());
        kbd.id = Some((0x2dc8, 0x5200));
        let present = crate::flowmodel::Present {
            devices: vec![kbd],
            udev: true,
        };
        let goes = |text: &str| -> String {
            let f = crate::flowmodel::build(text, &present, None);
            let r = f.inputs[0].rule.expect("a rule catches it");
            f.rules[r].target.clone()
        };
        assert_eq!(goes(text), "desk", "every keyboard types into desk");
        // revise panel and place the keyboard in it
        let f = build(text);
        let (rest, mut panel) = read(&f).revise("panel").expect("panel");
        panel.pick(Piece::Input("id:2dc8:5200".into()));
        let after = apply(text, &plan(&f, &rest.with(panel)));
        assert_eq!(
            rules_of(&after)[2..],
            [
                "input id:2dc8:5200 -> panel",
                "input class:keyboard -> desk",
                "input class:key -> desk",
            ]
        );
        assert_eq!(goes(&after), "panel");
    }

    #[test]
    fn a_port_drops_its_usb_interface() {
        assert_eq!(port_of(KBD_PATH), PORT_33);
        assert_eq!(port_of("path:acpi-LNXPWRBN:00"), "path:acpi-LNXPWRBN:00");
        // ADR-1403 D8's ranks, as vtium-settings gives them
        assert_eq!(rank(Kind::Input, "serial:046d:c52b:1"), 1);
        assert_eq!(rank(Kind::Display, "edid:AAA:0x0001:LARGE00001"), 1);
        assert_eq!(rank(Kind::Input, PORT_33), 2);
        assert_eq!(rank(Kind::Display, "connector:pci-0000:03:00.0/DP-8"), 2);
        assert_eq!(rank(Kind::Display, "card:pci-0000:03:00.0"), 3);
        assert_eq!(rank(Kind::Input, "id:046d:c31c"), 4);
        assert_eq!(rank(Kind::Input, "name:Power*"), 5);
        assert_eq!(rank(Kind::Input, "class:keyboard"), 6);
        assert_eq!(rank(Kind::Input, ANY_KEYBOARD), 6);
        assert_eq!(rank(Kind::Input, ANY_POINTER), 6);
        assert_eq!(rank(Kind::Input, "*"), 7);
    }
    // ADR-1403 D4, D8: labelling one part writes the `[labels]` section,
    // with its comment, and that part's line, and changes nothing else; a
    // second label adds its one line
    #[test]
    fn labelling_one_part_changes_only_its_label_line() {
        for text in [TEXT, crate::drafts::EXAMPLE, crate::tui::fixture::DRAFT] {
            let f = build(text);
            let mut doc = read(&f);
            doc.set_label(Kind::Input, "serial:046d:c52b:0", "Desk keyboard");
            let one = super::super::edit::apply_all(text, &plan(&f, &doc)).expect("the label");
            let added = one
                .strip_prefix(text)
                .expect("nothing before the section changes");
            assert_eq!(
                added,
                "\n# The person's own names for displays and inputs. vtium shows a\n# display's label when vtium-config identifies the displays.\n[labels]\ninput serial:046d:c52b:0 = Desk keyboard\n"
            );
            let back = build(&one);
            let mut doc = read(&back);
            doc.set_label(Kind::Display, "edid:BBB:0x0002:SMALL00001", "Panel");
            let two = apply(&one, &plan(&back, &doc));
            assert_eq!(
                crate::drafts::counts(&one, &two),
                (1, 0),
                "{}",
                crate::drafts::diff("a", &one, "b", &two)
            );
        }
        // a draft with no section runs the compiled-in flow, which a label
        // writes out first, so the draft keeps its meaning and the label
        // reads back; a second label then adds one line
        let plain = "# a comment\nfont-name=monoid\n";
        let f = build(plain);
        let mut doc = read(&f);
        doc.set_label(Kind::Input, "serial:046d:c52b:0", "Desk keyboard");
        let one = super::super::edit::apply_all(plain, &plan(&f, &doc)).expect("the label");
        assert!(one.starts_with(plain), "{one}");
        assert_eq!(rules_of(&one), rules_of(crate::drafts::EXAMPLE));
        let back = build(&one);
        assert_eq!(
            read(&back).label(Kind::Input, "serial:046d:c52b:0"),
            Some("Desk keyboard")
        );
        let mut doc = read(&back);
        doc.set_label(Kind::Display, "edid:BBB:0x0002:SMALL00001", "Panel");
        let two = super::super::edit::apply_all(&one, &plan(&back, &doc)).expect("the label");
        assert_eq!(crate::drafts::counts(&one, &two), (1, 0));
        // a draft that holds the try-out's longer comment gets the short one
        let tried = format!(
            "{TEXT}\n{}[labels]\ninput serial:046d:c52b:0 = Desk keyboard\n",
            crate::ops::LABELS_COMMENT_TRIED
        );
        let f = build(&tried);
        let mut doc = read(&f);
        doc.set_label(Kind::Input, "serial:046d:c52b:0", "Desk board");
        assert_eq!(
            apply(&tried, &plan(&f, &doc)).strip_prefix(TEXT),
            Some(
                "\n# The person's own names for displays and inputs. vtium shows a\n# display's label when vtium-config identifies the displays.\n[labels]\ninput serial:046d:c52b:0 = Desk board\n"
            )
        );
    }

    #[test]
    fn labels_round_trip_through_the_labels_section_in_the_rules_order() {
        let f = build(TEXT);
        let mut doc = read(&f);
        assert!(doc.labels.is_empty());
        doc.set_label(Kind::Input, "serial:046d:c52b:0", "Desk keyboard #1");
        doc.set_label(Kind::Display, "edid:BBB:0x0002:SMALL00001", "Panel screen");
        // a part made from a template choice, in no rule
        doc.set_label(Kind::Input, ANY_KEYBOARD, "Every board");
        doc.set_label(Kind::Input, "id:046d:c31c", "Receiver");
        let text = apply(TEXT, &plan(&f, &doc));
        // the rules stay as they are, and the section comes after them
        assert_eq!(rules_of(&text), rules_of(TEXT));
        let tail = text.strip_prefix(TEXT).expect("the rules stay as they are");
        assert_eq!(
            tail,
            format!(
                "\n{}[labels]\ndisplay edid:BBB:0x0002:SMALL00001 = Panel screen\ninput serial:046d:c52b:0 = Desk keyboard \\#1\ninput id:046d:c31c = Receiver\ninput class:keyboard = Every board\n",
                crate::ops::LABELS_COMMENT
            )
        );
        // the comment above the section wraps at 78 columns
        assert!(
            crate::ops::LABELS_COMMENT
                .lines()
                .all(|l| l.chars().count() <= 78)
        );
        // reading the draft back gives each part its label
        let back = read(&build(&text));
        assert_eq!(
            back.label(Kind::Input, "serial:046d:c52b:0"),
            Some("Desk keyboard #1")
        );
        assert_eq!(back.label(Kind::Input, ANY_KEYBOARD), Some("Every board"));
        assert_eq!(
            back.label(Kind::Display, "edid:BBB:0x0002:SMALL00001"),
            Some("Panel screen")
        );
        assert_eq!(
            back.label(Kind::Display, "edid:AAA:0x0001:LARGE00001"),
            None
        );
        assert!(
            plan(&build(&text), &back).is_empty(),
            "nothing more to write"
        );
        // revise and add back keep the labels byte for byte
        let mut seen = 0;
        for (name, b) in round_trips(&text) {
            assert_eq!(b.as_deref(), Ok(text.as_str()), "{name}");
            seen += 1;
        }
        assert!(seen >= 3);
        // clearing a label removes its line; clearing every label, the section
        let mut fewer = back.clone();
        fewer.set_label(Kind::Input, "id:046d:c31c", "");
        let less = apply(&text, &plan(&build(&text), &fewer));
        assert!(!less.contains("Receiver") && less.contains("Every board"));
        for (k, s) in [
            (Kind::Input, "serial:046d:c52b:0"),
            (Kind::Display, "edid:BBB:0x0002:SMALL00001"),
            (Kind::Input, ANY_KEYBOARD),
        ] {
            fewer.set_label(k, s, "");
        }
        assert_eq!(apply(&less, &plan(&build(&less), &fewer)), TEXT);
    }
}
