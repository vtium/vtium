//! The configuration as paths, for the Flow view as rows ([`super::rows`]).
//! A path joins input pieces and display pieces through one terminal (a
//! head), with an optional font set. The person composes a new path from
//! pieces in the editor, adds it to the document, and revises a path to
//! change which pieces it joins: revising moves the path out of the
//! document and back into the editor.
//!
//! A piece is a selector, as a rule writes it: `edid:…` or `*` for a
//! display, `path:…`, `id:…`, `serial:…` or `class:keyboard` for an input.
//! [`read`] reads the paths out of a draft's flow, and [`plan`] writes a
//! document of paths back as the draft's changes: a head per path, a rule
//! per piece, an input in two paths written as roaming, and the rules
//! ordered most specific first. Every change goes through
//! [`super::edit::Change`], so through `src/ops.rs`.

use super::edit::Change;
use super::flow::{Device, Flow, Kind, Rule, Special, Target};
use super::rows::{Sort, port_words, sort};
use super::words;

/// The piece `any pointer`, which the draft writes as one rule for each
/// class of pointer.
pub const ANY_POINTER: &str = "class:pointer";

/// The classes `any pointer` stands for.
pub const POINTER_CLASSES: [&str; 4] = [
    "class:mouse",
    "class:touchpad",
    "class:trackball",
    "class:pointingstick",
];

/// One piece of a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Piece {
    Display(String),
    Input(String),
    /// A font set, by its frit's name; `None` for the default font set.
    Font(Option<String>),
}

/// A path: its terminal's name once it has one, its pieces, and its font
/// set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path {
    /// The head's name: the draft's for a path read from it, kept when the
    /// path is taken out; `None` for a path never added.
    pub name: Option<String>,
    pub displays: Vec<String>,
    pub inputs: Vec<String>,
    /// The frit the path's look draws with; `None` for the default.
    pub font: Option<String>,
}

/// What picking a piece did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    Added,
    Removed,
    /// Another font set took the place of this one.
    Replaced(Option<String>),
}

impl Path {
    /// Whether the path has at least one input and one display.
    pub fn complete(&self) -> bool {
        !self.displays.is_empty() && !self.inputs.is_empty()
    }

    /// Whether the path has no piece at all.
    pub fn is_empty(&self) -> bool {
        self.displays.is_empty() && self.inputs.is_empty() && self.font.is_none()
    }

    /// Whether `piece` is in the path.
    pub fn has(&self, piece: &Piece) -> bool {
        match piece {
            Piece::Display(s) => self.displays.contains(s),
            Piece::Input(s) => self.inputs.contains(s),
            Piece::Font(f) => self.font == *f && f.is_some(),
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
        match piece {
            Piece::Display(s) => toggle(&mut self.displays, s),
            Piece::Input(s) => toggle(&mut self.inputs, s),
            Piece::Font(f) if self.font == f => {
                self.font = None;
                Picked::Removed
            }
            Piece::Font(f) => match std::mem::replace(&mut self.font, f) {
                None => Picked::Added,
                old => Picked::Replaced(old),
            },
        }
    }

    /// Puts `new` in the place of the piece `old`, keeping its place.
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

    /// Whether `other` has the same displays and inputs, in any order.
    fn same_pieces(&self, other: &Path) -> bool {
        let set = |v: &[String]| {
            let mut v = v.to_vec();
            v.sort();
            v.dedup();
            v
        };
        set(&self.displays) == set(&other.displays) && set(&self.inputs) == set(&other.inputs)
    }

    /// What the new path still needs, or that it is ready, in words.
    pub fn status(&self) -> &'static str {
        match (self.inputs.is_empty(), self.displays.is_empty()) {
            (true, true) => words::PATH_EMPTY,
            (false, true) => words::PATH_NEEDS_DISPLAY,
            (true, false) => words::PATH_NEEDS_INPUT,
            (false, false) => words::PATH_COMPLETE,
        }
    }
}

/// Whether the rule `rule` belongs to a path: it sends to a head, or it is
/// an input rule that roams while the draft has a head to roam among.
fn fits(flow: &Flow, rule: &Rule) -> bool {
    match flow.target(rule) {
        Target::Head(_) => true,
        Target::Special(Special::Roaming) => !flow.heads.is_empty(),
        _ => false,
    }
}

/// The font set the head `h` draws with: its look's frit, `None` for the
/// default.
fn font_of(flow: &Flow, h: usize) -> Option<String> {
    flow.look_of(h)
        .and_then(|l| flow.looks.get(l))
        .map(|l| l.frit.clone())
        .filter(|f| !f.is_empty())
}

/// The paths of a draft's flow, one per head in the file's order, and how
/// many of its rules belong to no path. A roaming input is in every path.
pub fn read(flow: &Flow) -> (Vec<Path>, usize) {
    let roaming: Vec<String> = flow
        .rules_of(Kind::Input)
        .filter(|(_, r)| flow.target(r) == Target::Special(Special::Roaming))
        .map(|(_, r)| r.selector.clone())
        .collect();
    let mut out = Vec::new();
    for (h, head) in flow.heads.iter().enumerate() {
        let of = |kind: Kind| -> Vec<String> {
            flow.rules_of(kind)
                .filter(|(_, r)| flow.target(r) == Target::Head(h))
                .map(|(_, r)| r.selector.clone())
                .collect()
        };
        let mut inputs = of(Kind::Input);
        inputs.extend(roaming.iter().cloned());
        let mut seen = Vec::new();
        inputs.retain(|s| {
            let fresh = !seen.contains(s);
            seen.push(s.clone());
            fresh
        });
        // the four pointer classes read as `any pointer`
        if POINTER_CLASSES
            .iter()
            .all(|c| inputs.iter().any(|s| s == c))
        {
            let at = inputs
                .iter()
                .position(|s| POINTER_CLASSES.contains(&s.as_str()))
                .unwrap_or(0);
            inputs.retain(|s| !POINTER_CLASSES.contains(&s.as_str()));
            inputs.insert(at.min(inputs.len()), ANY_POINTER.to_owned());
        }
        out.push(Path {
            name: Some(head.name.clone()),
            displays: of(Kind::Display),
            inputs,
            font: font_of(flow, h),
        });
    }
    let unfit = flow.rules.iter().filter(|r| !fits(flow, r)).count();
    (out, unfit)
}

/// The name a new terminal takes: `terminal-N`, the smallest N no head
/// and no path has. A head's name holds no space.
pub fn new_name(flow: &Flow, doc: &[Path]) -> String {
    let taken = |n: &str| {
        flow.heads.iter().any(|h| h.name == n) || doc.iter().any(|p| p.name.as_deref() == Some(n))
    };
    (1usize..)
        .map(|n| format!("terminal-{n}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| "terminal".to_owned())
}

/// How specific a selector is, most specific first: one device by its
/// serial, EDID, Bluetooth address or name; then a port or a connector;
/// then an id, which names every device that reports it; then a class or
/// a card; then the catch-alls. It is the order in which `ops` places a
/// new rule, before the first wider rule of its kind (ADR-1403 D7).
pub fn rank(selector: &str) -> u8 {
    let key = selector.split(':').next().unwrap_or_default();
    match key {
        "serial" | "edid" | "bluetooth" | "name" | "connector-path" => 0,
        "path" | "connector" => 1,
        "id" => 2,
        "class" | "card" | "udev" => 3,
        _ => 4,
    }
}

/// What adding the new path does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adding {
    /// It is a new path.
    New,
    /// The document already has this path, in the terminal named: nothing
    /// changes.
    Same(String),
}

/// Whether `new` may be added to the document `doc`, or why not.
pub fn check_add(flow: &Flow, doc: &[Path], new: &Path) -> Result<Adding, String> {
    if !new.complete() {
        return Err(new.status().to_owned());
    }
    let name = |p: &Path| p.name.clone().unwrap_or_default();
    if let Some(n) = &new.name
        && !valid_name(n)
    {
        return Err(words::name_invalid(n));
    }
    if let Some(same) = doc.iter().find(|p| p.same_pieces(new)) {
        return if same.font == new.font {
            Ok(Adding::Same(name(same)))
        } else {
            Err(words::path_other_font(&name(same)))
        };
    }
    if let Some(n) = &new.name
        && doc.iter().any(|p| p.name.as_deref() == Some(n.as_str()))
    {
        return Err(words::name_taken(n));
    }
    for d in &new.displays {
        if let Some(p) = doc.iter().find(|p| p.displays.contains(d)) {
            let (title, _) = title(flow, Kind::Display, d);
            return Err(words::display_taken(&title, &name(p)));
        }
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

/// The document without the path of the terminal `name`, and that path,
/// which goes into the editor with its name kept: "Revise this path".
pub fn revise(doc: &[Path], name: &str) -> Option<(Vec<Path>, Path)> {
    let at = doc.iter().position(|p| p.name.as_deref() == Some(name))?;
    let mut rest = doc.to_vec();
    let taken = rest.remove(at);
    Some((rest, taken))
}

/// The draft's changes that make its heads, looks and path rules the
/// document `doc`: the old path rules go, the heads no path has go, each
/// path's head is added or has its look set, and each piece's rule is
/// added, most specific first. An input in two paths is written once, as
/// roaming. Rules that belong to no path stay as they are.
pub fn plan(flow: &Flow, doc: &[Path]) -> Vec<Change> {
    let mut out = Vec::new();
    // the old path rules, from the last, so the numbers stay right
    for (i, r) in flow.rules.iter().enumerate().rev() {
        if fits(flow, r) {
            out.push(Change::RuleRm(i.saturating_add(1)));
        }
    }
    for head in &flow.heads {
        if !doc
            .iter()
            .any(|p| p.name.as_deref() == Some(head.name.as_str()))
        {
            out.push(Change::HeadRm(head.name.clone()));
        }
    }
    // a look for each font set picked: one that names its frit, else a new
    // look of the frit's name
    let mut new_looks: Vec<String> = Vec::new();
    let mut look_for = |frit: &str, out: &mut Vec<Change>| -> String {
        if let Some(l) = flow.looks.iter().find(|l| l.frit == frit) {
            return l.name.clone();
        }
        if !new_looks.iter().any(|n| n == frit) {
            new_looks.push(frit.to_owned());
            out.push(Change::LookAdd {
                name: frit.to_owned(),
                frit: Some(frit.to_owned()),
                size: None,
            });
        }
        frit.to_owned()
    };
    let mut heads = Vec::new();
    for p in doc {
        let Some(name) = p.name.clone() else {
            continue;
        };
        let look = p.font.as_deref().map(|f| look_for(f, &mut out));
        match flow.heads.iter().find(|h| h.name == name) {
            None => heads.push(Change::HeadAdd { name, look }),
            Some(h) => {
                let now = (h.look != "default").then(|| h.look.clone());
                if now != look {
                    heads.push(Change::HeadKey {
                        name,
                        key: "look",
                        value: look,
                    });
                }
            }
        }
    }
    out.extend(heads);
    // the rules: each piece once, an input in two paths roaming
    let mut rules: Vec<(u8, String)> = Vec::new();
    let mut push = |line: String, sel: &str| {
        if !rules.iter().any(|(_, l)| *l == line) {
            rules.push((rank(sel), line));
        }
    };
    for p in doc {
        let Some(name) = &p.name else {
            continue;
        };
        for d in &p.displays {
            push(format!("display {d} -> {name}"), d);
        }
        for i in &p.inputs {
            let paths = doc.iter().filter(|q| q.inputs.contains(i)).count();
            let target = if paths > 1 { "roaming" } else { name.as_str() };
            let sels: Vec<&str> = if i == ANY_POINTER {
                POINTER_CLASSES.to_vec()
            } else {
                vec![i.as_str()]
            };
            for s in sels {
                push(format!("input {s} -> {target}"), s);
            }
        }
    }
    rules.sort_by_key(|(r, _)| *r);
    out.extend(rules.into_iter().map(|(_, l)| Change::RuleAdd(l)));
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
        (Kind::Input, "*") => ("any input".to_owned(), "every input".to_owned()),
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
        (Kind::Input, "class") => (format!("any {value}"), format!("every {value}")),
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
        _ => match dev {
            Some(d) => (d.name.clone(), selector.to_owned()),
            None => (selector.to_owned(), String::new()),
        },
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
        Sort::Keyboard => add("class:keyboard".to_owned()),
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
        Piece::Font(_) => Sort::Other,
        Piece::Input(s) if s == "class:keyboard" => Sort::Keyboard,
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
        }
    }

    #[test]
    fn a_draft_reads_as_one_path_per_terminal() {
        let (doc, unfit) = read(&flow());
        assert_eq!(
            doc,
            [
                path(
                    Some("desk"),
                    &[EDID_A],
                    &["serial:046d:c52b:0", "id:046d:c31c"],
                    Some("monoid")
                ),
                path(Some("panel"), &[EDID_B], &["id:046d:c31c"], None),
            ]
        );
        assert_eq!(unfit, 1, "the catch-all to ignore belongs to no path");
    }

    #[test]
    fn a_path_is_complete_with_an_input_and_a_display() {
        let mut p = Path::default();
        assert_eq!(p.status(), words::PATH_EMPTY);
        p.pick(Piece::Input(KBD_PATH.into()));
        assert!(!p.complete());
        assert_eq!(p.status(), words::PATH_NEEDS_DISPLAY);
        p.pick(Piece::Display(EDID_A.into()));
        assert!(p.complete());
        assert_eq!(p.status(), words::PATH_COMPLETE);
        // a second pick takes the piece out again
        assert_eq!(p.pick(Piece::Input(KBD_PATH.into())), Picked::Removed);
        assert_eq!(p.status(), words::PATH_NEEDS_INPUT);
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
    fn adding_a_path_the_document_has_changes_nothing() {
        let f = flow();
        let (doc, _) = read(&f);
        // the same pieces in another order
        let same = path(
            None,
            &[EDID_A],
            &["id:046d:c31c", "serial:046d:c52b:0"],
            Some("monoid"),
        );
        assert_eq!(check_add(&f, &doc, &same), Ok(Adding::Same("desk".into())));
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
        let (doc, _) = read(&f);
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
                "any keyboard",
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
            ["any display", "DP-8", "HDMI-A-2", "DP-8", "HDMI-A-2"]
        );
    }

    /// The draft whose flow [`flow`]'s heads, looks and rules stand for.
    const TEXT: &str = "version = 1\npolicy = open\n[look big]\nfont-name = monoid\nfont-size = 14\n[head desk]\nlook = big\n[head panel]\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ndisplay edid:BBB:0x0002:SMALL00001 -> panel\ninput serial:046d:c52b:0 -> desk\ninput id:046d:c31c -> roaming\ninput * -> ignore\n";

    fn apply(text: &str, changes: &[Change]) -> String {
        changes.iter().fold(text.to_owned(), |t, c| {
            c.apply(&t)
                .unwrap_or_else(|e| panic!("{c:?}: {}", e.text()))
        })
    }

    fn rule_lines(text: &str) -> Vec<String> {
        let at = text.find("[rules]\n").expect("rules") + 8;
        text[at..].lines().map(str::to_owned).collect()
    }

    #[test]
    fn the_rules_are_written_most_specific_first() {
        let f = crate::flowmodel::build(TEXT, &crate::flowmodel::Present::default(), None);
        let (mut doc, _) = read(&f);
        assert_eq!(doc.len(), 2);
        let name = new_name(&f, &doc);
        assert_eq!(name, "terminal-1");
        doc.push(path(
            Some(&name),
            &["*"],
            &["class:keyboard", PORT_33, "serial:2dc8:5200:9", ANY_POINTER],
            Some("dejavu"),
        ));
        let text = apply(TEXT, &plan(&f, &doc));
        let lines = rule_lines(&text);
        assert_eq!(
            lines,
            [
                "input serial:046d:c52b:0 -> desk",
                "input serial:2dc8:5200:9 -> terminal-1",
                format!("input {PORT_33} -> terminal-1").as_str(),
                "input id:046d:c31c -> roaming",
                "input class:keyboard -> terminal-1",
                "input class:mouse -> terminal-1",
                "input class:touchpad -> terminal-1",
                "input class:trackball -> terminal-1",
                "input class:pointingstick -> terminal-1",
                "input * -> ignore",
                "display edid:AAA:0x0001:LARGE00001 -> desk",
                "display edid:BBB:0x0002:SMALL00001 -> panel",
                "display * -> terminal-1",
            ]
        );
        // within each kind, from the most specific rule to the widest
        for kind in ["display ", "input "] {
            let ranks: Vec<u8> = lines
                .iter()
                .filter_map(|l| l.strip_prefix(kind))
                .map(|l| rank(l.split_whitespace().next().unwrap_or_default()))
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
        // read back, the draft is the same document, except that the
        // receiver, which roams, types into the new terminal too: vtium.conf
        // says only that it roams, which is into whichever terminal is active
        let back = crate::flowmodel::build(&text, &crate::flowmodel::Present::default(), None);
        let read_back = read(&back).0;
        assert_eq!(read_back[..2], doc[..2]);
        assert_eq!(
            read_back[2].inputs,
            [
                "serial:2dc8:5200:9",
                PORT_33,
                "class:keyboard",
                ANY_POINTER,
                "id:046d:c31c"
            ]
        );
        assert_eq!(read_back[2].font.as_deref(), Some("dejavu"));
        // and writing it again changes nothing
        assert_eq!(apply(&text, &plan(&back, &read_back)), text);
    }

    #[test]
    fn revising_a_path_returns_it_to_the_editor_and_frees_its_pieces() {
        let f = crate::flowmodel::build(TEXT, &crate::flowmodel::Present::default(), None);
        let (doc, _) = read(&f);
        let (rest, taken) = revise(&doc, "desk").expect("desk's path");
        assert_eq!(taken, doc[0]);
        let text = apply(TEXT, &plan(&f, &rest));
        assert!(!text.contains("[head desk]"), "{text}");
        // the receiver is in one path now, so it no longer roams
        assert_eq!(
            rule_lines(&text),
            [
                "input id:046d:c31c -> panel",
                "input * -> ignore",
                "display edid:BBB:0x0002:SMALL00001 -> panel",
            ]
        );
        // adding it back puts the draft as it was, the receiver roaming again
        let back = crate::flowmodel::build(&text, &crate::flowmodel::Present::default(), None);
        let (mut doc2, _) = read(&back);
        assert_eq!(check_add(&back, &doc2, &taken), Ok(Adding::New));
        doc2.push(taken);
        let again = apply(&text, &plan(&back, &doc2));
        assert!(again.contains("input id:046d:c31c -> roaming"), "{again}");
        assert!(again.contains("[head desk]\nlook = big\n"), "{again}");
    }

    #[test]
    fn a_port_drops_its_usb_interface() {
        assert_eq!(port_of(KBD_PATH), PORT_33);
        assert_eq!(port_of("path:acpi-LNXPWRBN:00"), "path:acpi-LNXPWRBN:00");
        assert_eq!(rank("serial:1"), 0);
        assert_eq!(rank(PORT_33), 1);
        assert_eq!(rank("id:046d:c31c"), 2);
        assert_eq!(rank("class:keyboard"), 3);
        assert_eq!(rank("*"), 4);
    }
}
