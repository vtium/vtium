//! The configuration as paths. A path joins display parts and input parts
//! through one terminal, a head of vtium.conf, with the look the terminal
//! draws with and the terminal's settings. [`read`] reads the paths out of
//! a draft's flow, with the inputs that roam, which type into whichever
//! terminal is active, and the displays and inputs sent to Nowhere.
//! [`Path`] is also the path that NEW PATH builds, which Space fills part
//! by part.
//!
//! A part is a match, as a rule writes it: `edid:…` or `*` for a display,
//! `path:…`, `id:…`, `serial:…` or `class:keyboard` for an input. Two
//! parts stand for several rules each: All keyboards writes a rule for
//! full keyboards and one for the inputs with some keys, and All mice and
//! touchpads one for each class of pointer, as vtium's shipped default
//! writes them.
//!
//! References:
//! - ADR-1403 D7, "A path": what a path holds, roaming as a path, and a
//!   display in one path for each VT.
//! - ADR-1403 D8: the rules and the head a path is in vtium.conf.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use vtium_config_core::flow::{Device, Flow, Kind, Special, Target};

use crate::inputs::{Sort, port_words, sort};
use crate::words;

/// The part All keyboards: every input with keys, written as a rule for
/// full keyboards and one for the inputs with some keys. Its own word is
/// no class of vtium's, so a lone `class:keyboard` rule reads back as a
/// part of its own.
pub const ANY_KEYBOARD: &str = "class:keyboard+key";
const KEYBOARD_CLASSES: [&str; 2] = ["class:keyboard", "class:key"];

/// The part All mice and touchpads, written as one rule for each class of
/// pointer the shipped default lists.
pub const ANY_POINTER: &str = "class:pointer";
const POINTER_CLASSES: [&str; 5] = [
    "class:mouse",
    "class:touchpad",
    "class:touchscreen",
    "class:trackball",
    "class:pointingstick",
];

/// The terminal settings a path carries, as a head's keys.
const SETTINGS: [&str; 6] = [
    "consoles",
    "login",
    "when-display-lost",
    "switch-on-enter",
    "isolated",
    "vt",
];

/// The rules one part writes.
pub fn expand(part: &str) -> Vec<&str> {
    match part {
        ANY_KEYBOARD => KEYBOARD_CLASSES.to_vec(),
        ANY_POINTER => POINTER_CLASSES.to_vec(),
        p => vec![p],
    }
}

/// One part of a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Piece {
    Display(String),
    Input(String),
    /// A look, by the name of its `[look NAME]` section; `None` for the
    /// default look, which a head that names no look draws with.
    Look(Option<String>),
    /// The active terminal: a path with it in its terminal's place sends
    /// its inputs roaming, to whichever terminal is active.
    Active,
    /// No input: nothing types into the path's terminal, roaming inputs
    /// included.
    NoInput,
    /// Nowhere: a path to it turns its displays off and sets its inputs
    /// aside.
    NoTerminal,
    /// A new terminal the person made, by its name.
    Terminal(String),
}

/// A path: its terminal's name once it has one, its parts, its look, and
/// its terminal's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path {
    /// The head's name: the draft's for a path read from it, or typed in
    /// NEW PATH; `None` for a path that has none yet.
    pub name: Option<String>,
    pub displays: Vec<String>,
    /// The inputs sent to this terminal alone. In NEW PATH, every input
    /// picked, some of which roam once the path is added.
    pub inputs: Vec<String>,
    /// The look the path's terminal draws with, by its name; `None` for
    /// the default look.
    pub look: Option<String>,
    /// The terminal's settings, each a head's key and its value.
    pub settings: Vec<(String, String)>,
    /// The path's terminal is the active terminal: its inputs roam.
    pub active: bool,
    /// The path has No input in its inputs' place.
    pub no_input: bool,
    /// The path's terminal is Nowhere.
    pub none: bool,
}

/// What picking a part did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    Added,
    /// All keyboards came in, and All mice and touchpads with it.
    AddedWithPointers,
    Removed,
    /// Another look took the place of this one.
    Replaced(Option<String>),
}

impl Path {
    /// Whether the path can be added to the configuration `doc`.
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
            Piece::Look(l) => self.look == *l && l.is_some(),
            Piece::Active => self.active,
            Piece::NoInput => self.no_input,
            Piece::NoTerminal => self.none,
            Piece::Terminal(n) => self.name.as_deref() == Some(n.as_str()),
        }
    }

    /// The value of the setting `key`, empty when the path leaves it at
    /// its default.
    pub fn setting(&self, key: &str) -> String {
        self.settings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    /// The VTs the path's terminal runs on, none for every VT.
    pub fn vts(&self) -> Vec<u16> {
        self.setting("vt")
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect()
    }

    /// The VT on which this path's terminal and `other`'s both run, the
    /// smallest, or `Some(None)` when both run on every VT; `None` when
    /// they share no VT, so the two may hold the same display and inputs.
    pub fn shared_vt(&self, other: &Path) -> Option<Option<u16>> {
        let (mine, theirs) = (self.vts(), other.vts());
        match (mine.is_empty(), theirs.is_empty()) {
            (true, true) => Some(None),
            (true, false) => theirs.iter().min().copied().map(Some),
            (false, true) => mine.iter().min().copied().map(Some),
            (false, false) => mine
                .iter()
                .filter(|v| theirs.contains(v))
                .min()
                .copied()
                .map(Some),
        }
    }

    /// Picks `piece` into the path, or takes it out when it is there. A
    /// second look takes the first one's place, and the terminal's place
    /// holds one terminal part at a time.
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
            // an input takes the place of No input, and the other way round
            Piece::Input(s) => {
                self.no_input = false;
                toggle(&mut self.inputs, s)
            }
            Piece::NoInput => {
                self.inputs.clear();
                flag(&mut self.no_input)
            }
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
            Piece::Look(l) if self.look == l => {
                self.look = None;
                Picked::Removed
            }
            Piece::Look(l) => match std::mem::replace(&mut self.look, l) {
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
    /// except that All keyboards brings All mice and touchpads along into
    /// a path that has no pointer, as vtium's shipped default sends every
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

    /// What the path still needs to join the configuration `doc`, or that
    /// it is ready, in words.
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
/// order; the inputs that roam; the path to Nowhere; and how many of its
/// rules belong to none of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    pub paths: Vec<Path>,
    pub roaming: Vec<String>,
    /// The path to Nowhere: the displays that stay off, and the inputs set
    /// aside.
    pub off: Vec<String>,
    pub ignored: Vec<String>,
    pub unfit: usize,
    /// The draft has no rule and no terminal yet, so its first path takes
    /// the shipped default's shape: its inputs roam.
    pub empty: bool,
    /// The labels the person gave parts: each part's kind, its match as
    /// the part holds it, and its label.
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

    /// The path of the terminal `name`.
    pub fn path(&self, name: &str) -> Option<&Path> {
        self.paths.iter().find(|p| p.name.as_deref() == Some(name))
    }

    /// Whether the input part `sel`, picked into the new path `new`, roams
    /// once the path is added: it roams already, another path whose
    /// terminal shares a VT with `new`'s has it, or the new path is the
    /// configuration's first.
    pub fn would_roam(&self, sel: &str, new: &Path) -> bool {
        (self.empty && self.paths.is_empty())
            || self.roaming.iter().any(|r| r == sel)
            || self
                .paths
                .iter()
                .any(|p| p.inputs.iter().any(|i| i == sel) && new.shared_vt(p).is_some())
    }
}

/// Collapses the rules a part writes back into the part: the keyboard
/// classes into All keyboards, the pointer classes into All mice and
/// touchpads.
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

/// Each match once, in the order it first comes, with the classes
/// collapsed into the parts that write them.
fn tidy(mut list: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    list.retain(|s| {
        let fresh = !seen.contains(s);
        seen.push(s.clone());
        fresh
    });
    collapse(&mut list, ANY_POINTER, &POINTER_CLASSES);
    collapse(&mut list, ANY_KEYBOARD, &KEYBOARD_CLASSES);
    list
}

/// The configuration of a draft's flow, as paths and roaming inputs.
pub fn read(flow: &Flow) -> Doc {
    let to = |kind: Kind, special: Special| -> Vec<String> {
        tidy(
            flow.rules_of(kind)
                .filter(|(_, r)| flow.target(r) == Target::Special(special))
                .map(|(_, r)| r.selector.clone())
                .collect(),
        )
    };
    let roaming = to(Kind::Input, Special::Roaming);
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
        let inputs = tidy(of(Kind::Input));
        let mut settings: Vec<(String, String)> = head
            .keys
            .iter()
            .filter(|(k, _)| SETTINGS.contains(&k.as_str()))
            .cloned()
            .collect();
        if head.consoles != 1 {
            settings.insert(0, ("consoles".to_owned(), head.consoles.to_string()));
        }
        // a terminal no input rule names that is isolated has No input
        let no_input =
            inputs.is_empty() && settings.iter().any(|(k, v)| k == "isolated" && v == "yes");
        if no_input {
            settings.retain(|(k, _)| k != "isolated");
        }
        paths.push(Path {
            name: Some(head.name.clone()),
            displays: of(Kind::Display),
            inputs,
            look: (head.look != "default").then(|| head.look.clone()),
            settings,
            no_input,
            ..Path::default()
        });
    }
    let unfit = flow
        .rules
        .iter()
        .filter(|r| !matches!(flow.target(r), Target::Head(_) | Target::Special(_)))
        .count();
    // a label's line writes the first rule's match of the part, so the
    // first class of the pointers, or of the keyboards, reads back as the
    // whole part, unless a rule of the configuration names that one class
    // alone
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

/// Whether `doc` is vtium's built-in default: its paths and roaming
/// inputs are the shipped example's. The example is read once.
pub fn is_default(doc: &Doc) -> bool {
    static SHIPPED: OnceLock<Doc> = OnceLock::new();
    let shipped = SHIPPED.get_or_init(|| {
        read(&vtium_config_core::flowmodel::build(
            vtium_config_core::drafts::EXAMPLE,
            &vtium_config_core::flowmodel::Present::default(),
            None,
        ))
    });
    doc.paths == shipped.paths && doc.roaming == shipped.roaming
}

/// The name a new terminal takes: `console`, the shipped default's, while
/// the configuration has no terminal; else `terminal-N`, the smallest N no
/// head and no path has.
pub fn new_name(flow: &Flow, doc: &Doc) -> String {
    let taken = |n: &str| flow.heads.iter().any(|h| h.name == n) || doc.path(n).is_some();
    if flow.heads.is_empty() && doc.paths.is_empty() {
        return "console".to_owned();
    }
    (1usize..)
        .map(|n| format!("terminal-{n}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| "terminal".to_owned())
}

/// The connected devices of `kind` a match names.
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

/// The match a connected device is picked by from PARTS: its serial when
/// it has one, else its id when no other input shares it, else its path;
/// for a display, its EDID, else its connector.
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

/// The match each connected device is picked by from PARTS, as
/// [`own_key`] gives it, worked out once for a flow. Finding a device by
/// its own match then takes one lookup, so laying out a page with dozens
/// of devices stays quick on a small machine.
#[derive(Clone, Debug, Default)]
pub struct Own {
    displays: BTreeMap<String, usize>,
    inputs: BTreeMap<String, usize>,
}

impl Own {
    /// The own match of every connected device of `flow`.
    pub fn of(flow: &Flow) -> Own {
        let index = |kind: Kind| -> BTreeMap<String, usize> {
            let mut out = BTreeMap::new();
            for (i, d) in flow.devices(kind).iter().enumerate() {
                if d.found {
                    out.entry(own_key(flow, kind, d)).or_insert(i);
                }
            }
            out
        };
        Own {
            displays: index(Kind::Display),
            inputs: index(Kind::Input),
        }
    }

    /// The connected device of `kind` whose own match is `sel`.
    pub fn device<'a>(&self, flow: &'a Flow, kind: Kind, sel: &str) -> Option<&'a Device> {
        let map = match kind {
            Kind::Display => &self.displays,
            Kind::Input => &self.inputs,
        };
        map.get(sel).and_then(|i| flow.devices(kind).get(*i))
    }
}

/// A part's title and the line under it, in a reader's words.
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
            .map_or_else(|| selector.to_owned(), |d| d.name.clone())
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    match (kind, key) {
        (Kind::Display, "*") => pair(words::ANY_DISPLAY, words::EVERY_DISPLAY),
        (Kind::Input, "*") => pair(words::ANY_INPUT_TITLE, words::EVERY_INPUT),
        (Kind::Display, "connector") => {
            let c = value.rsplit('/').next().unwrap_or(value);
            pair(c, words::BY_CONNECTOR)
        }
        (Kind::Display, "edid") => {
            let serial = value.rsplit(':').next().unwrap_or(value);
            match dev {
                Some(d) => (d.name.clone(), format!("{serial} · by EDID")),
                None => (format!("display {serial}"), words::NOT_PLUGGED.to_owned()),
            }
        }
        (Kind::Input, "class") if selector == ANY_POINTER => {
            pair(words::ANY_POINTER_TITLE, words::EVERY_POINTER)
        }
        (Kind::Input, "class") if selector == ANY_KEYBOARD => {
            pair(words::ANY_KEYBOARD_TITLE, words::EVERY_KEYBOARD)
        }
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
                if on.is_empty() && !value.contains("usb") {
                    (gone(), words::NOT_PLUGGED.to_owned())
                } else {
                    (
                        format!("{what} on {}", port_words(value)),
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
                .filter(|d| d.found && crate::parts::matches(d, selector));
            match (dev.or_else(|| on.next()), on.next()) {
                (Some(d), None) => (d.name.clone(), selector.to_owned()),
                _ => (selector.to_owned(), String::new()),
            }
        }
    }
}

/// The steps a part of sort `s` narrows through, broad to narrow, each its
/// title and its match: All for its sort; each port of a connected device
/// of that sort; each device by its own match, its id and its serial. A
/// display narrows from All displays to each connector and each display
/// by its EDID.
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

/// The sort a part's match is about, for its narrowing steps.
pub fn sort_of(flow: &Flow, piece: &Piece) -> Sort {
    match piece {
        Piece::Display(_) => Sort::Screen,
        Piece::Look(_)
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
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use vtium_config_core::flowmodel::{Present, build};

    #[test]
    fn the_keyboard_and_pointer_classes_read_back_as_one_part_each() {
        let text = "version = 1\n[head console]\n[rules]\ndisplay * -> console\n\
                    input class:keyboard -> roaming\ninput class:key -> roaming\n\
                    input class:mouse -> roaming\ninput class:touchpad -> roaming\n\
                    input class:touchscreen -> roaming\ninput class:trackball -> roaming\n\
                    input class:pointingstick -> roaming\n";
        let doc = read(&build(text, &Present::default(), None));
        assert_eq!(doc.roaming, [ANY_KEYBOARD, ANY_POINTER]);
        assert_eq!(doc.paths[0].displays, ["*"]);
        assert!(is_default(&doc) || !doc.paths.is_empty());
    }

    #[test]
    fn the_shipped_example_is_the_built_in_default() {
        let flow = build(
            vtium_config_core::drafts::EXAMPLE,
            &Present::default(),
            None,
        );
        assert!(is_default(&read(&flow)));
    }

    #[test]
    fn two_terminals_on_other_vts_share_no_vt() {
        let on = |vt: &str| Path {
            settings: vec![("vt".to_owned(), vt.to_owned())],
            ..Path::default()
        };
        assert_eq!(on("1").shared_vt(&on("2")), None);
        assert_eq!(on("1 2").shared_vt(&on("2")), Some(Some(2)));
        assert_eq!(Path::default().shared_vt(&Path::default()), Some(None));
    }

    #[test]
    fn all_keyboards_brings_the_pointers_along() {
        let flow = Flow::default();
        let mut p = Path::default();
        assert_eq!(
            p.pick_part(&flow, Piece::Input(ANY_KEYBOARD.to_owned())),
            Picked::AddedWithPointers
        );
        assert_eq!(p.inputs, [ANY_KEYBOARD, ANY_POINTER]);
        assert_eq!(
            p.pick(Piece::Input(ANY_KEYBOARD.to_owned())),
            Picked::Removed
        );
    }
}
