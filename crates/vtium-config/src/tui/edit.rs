//! What the Flow view changes in the draft (ADR-1403 D7): the fields of
//! the card Enter opens, the changes that connecting tags makes, and the
//! deletes that ask first. Each change is one of [`crate::ops`]'s
//! operations, the ones the `vtium config` commands call, and it also
//! names the command that makes the same change, which the view shows.

use vtium_settings::Of;
use vtium_settings::keys::{self, Kind as KeyKind};

use super::flow::{Device, Flow, Item, Kind, New, Special, Target, general};
use crate::ops::{self, RuleChange};
use crate::outcome::Fail;

/// One change to the draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Adds a rule, `KIND SELECTOR -> TARGET`, where a new rule goes.
    RuleAdd(String),
    /// Changes rule `number`'s selector, its target, or both.
    RuleSet {
        number: usize,
        selector: Option<String>,
        target: Option<String>,
    },
    /// Moves rule `number` before rule `before`; one past the last rule
    /// moves it last.
    RuleMove {
        number: usize,
        before: usize,
    },
    RuleRm(usize),
    HeadAdd {
        name: String,
        look: Option<String>,
    },
    /// Sets a head's key, or leaves it at its default with `None`.
    HeadKey {
        name: String,
        key: &'static str,
        value: Option<String>,
    },
    HeadRename {
        name: String,
        new: String,
    },
    HeadRm(String),
    LookAdd {
        name: String,
        frit: Option<String>,
        size: Option<String>,
    },
    LookKey {
        name: String,
        key: &'static str,
        value: Option<String>,
    },
    LookRename {
        name: String,
        new: String,
    },
    /// Deletes a look, setting the heads that use it to the default look.
    LookRm(String),
    /// Sets a top-level key, or leaves it at its default with `None`.
    Top {
        key: &'static str,
        value: Option<String>,
    },
}

/// The look keys the look card edits, each with the option of `vtium
/// config look set` that sets it.
pub const LOOK_KEYS: [(&str, &str); 5] = [
    ("font-name", "--frit"),
    ("font-size", "--size"),
    ("font-engine", "--engine"),
    ("font-hinting", "--hinting"),
    ("font-line-spacing", "--line-spacing"),
];

/// The head keys the head card edits, after its look.
pub const HEAD_KEYS: [&str; 4] = ["consoles", "login", "when-display-lost", "isolated"];

/// `word` quoted for a shell where it needs it.
fn shell(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:,=@+".contains(c))
    {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

impl Change {
    /// The draft `text` with the change made, through the operations the
    /// commands call.
    pub fn apply(&self, text: &str) -> Result<String, Fail> {
        match self {
            Change::RuleAdd(line) => ops::rule_add(text, line, None),
            Change::RuleSet {
                number,
                selector,
                target,
            } => ops::rule_set(
                text,
                *number,
                &RuleChange::Parts {
                    selector: selector.clone(),
                    target: target.clone(),
                },
            ),
            Change::RuleMove { number, before } => {
                let last = vtium_settings::rule_lines(text).len().saturating_add(1);
                let to = if *before >= last {
                    ops::To::Last
                } else {
                    ops::To::Before(*before)
                };
                ops::rule_move(text, *number, to)
            }
            Change::RuleRm(n) => ops::rule_rm(text, *n),
            Change::HeadAdd { name, look } => ops::head_add(text, name, look.as_deref()),
            Change::HeadKey { name, key, value } => {
                ops::section_keys(text, Of::Head(name), &[(key, value.as_deref())])
            }
            Change::HeadRename { name, new } => ops::rename(text, Of::Head(name), new),
            Change::HeadRm(name) => ops::head_rm(text, name),
            Change::LookAdd { name, frit, size } => {
                ops::look_add(text, name, frit.as_deref(), size.as_deref())
            }
            Change::LookKey { name, key, value } => {
                ops::section_keys(text, Of::Look(name), &[(key, value.as_deref())])
            }
            Change::LookRename { name, new } => ops::rename(text, Of::Look(name), new),
            Change::LookRm(name) => ops::look_rm(text, name, true),
            Change::Top {
                key,
                value: Some(v),
            } => ops::set_top(text, key, v),
            Change::Top { key, value: None } => ops::unset_top(text, key),
        }
    }

    /// The `vtium config` command that makes the same change (ADR-1403
    /// D8).
    pub fn command(&self) -> String {
        let c = "vtium config";
        match self {
            Change::RuleAdd(line) => format!("{c} rule add {}", shell(line)),
            Change::RuleSet {
                number,
                selector,
                target,
            } => {
                let mut out = format!("{c} rule set {number}");
                if let Some(s) = selector {
                    out.push_str(&format!(" --selector {}", shell(s)));
                }
                if let Some(t) = target {
                    out.push_str(&format!(" --target {}", shell(t)));
                }
                out
            }
            Change::RuleMove { number, before } => {
                format!("{c} rule move {number} --before {before}")
            }
            Change::RuleRm(n) => format!("{c} rule rm {n}"),
            Change::HeadAdd {
                name,
                look: Some(l),
            } => format!("{c} head add {} --look {}", shell(name), shell(l)),
            Change::HeadAdd { name, look: None } => format!("{c} head add {}", shell(name)),
            Change::HeadKey { name, key, value } => format!(
                "{c} head set {} --{key} {}",
                shell(name),
                shell(value.as_deref().unwrap_or_default())
            ),
            Change::HeadRename { name, new } => {
                format!("{c} head set {} --name {}", shell(name), shell(new))
            }
            Change::HeadRm(name) => format!("{c} head rm {}", shell(name)),
            Change::LookAdd { name, frit, size } => {
                let mut out = format!("{c} look add {}", shell(name));
                if let Some(f) = frit {
                    out.push_str(&format!(" --frit {}", shell(f)));
                }
                if let Some(s) = size {
                    out.push_str(&format!(" --size {}", shell(s)));
                }
                out
            }
            Change::LookKey { name, key, value } => {
                let option = LOOK_KEYS
                    .iter()
                    .find(|(k, _)| k == key)
                    .map_or(*key, |(_, o)| *o);
                format!(
                    "{c} look set {} {option} {}",
                    shell(name),
                    shell(value.as_deref().unwrap_or_default())
                )
            }
            Change::LookRename { name, new } => {
                format!("{c} look set {} --name {}", shell(name), shell(new))
            }
            Change::LookRm(name) => format!("{c} look rm {} --force", shell(name)),
            Change::Top {
                key,
                value: Some(v),
            } => format!("{c} set {key} {}", shell(v)),
            Change::Top { key, value: None } => format!("{c} set --unset {key}"),
        }
    }
}

/// What a card's field changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    RuleTarget(usize),
    RuleSelector(usize),
    HeadLook(String),
    HeadKey(String, &'static str),
    HeadName(String),
    LookKey(String, &'static str),
    LookName(String),
    Top(&'static keys::Key),
    /// A field of a `new…` form, by its place in the form.
    Form(usize),
    /// The card's actions: move, add, delete.
    Actions,
}

/// One field of a card: its label, its value now, the values it offers to
/// pick from, and whether a value may be typed instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub label: String,
    pub current: String,
    pub options: Vec<String>,
    pub typed: bool,
    pub what: What,
}

/// The actions a card's last row offers.
pub const UP: &str = "move up";
pub const DOWN: &str = "move down";
pub const DELETE: &str = "delete";
pub const ADD: &str = "add";

/// The sizes a look offers, in points, after automatic (0), as the Looks
/// view offers them.
fn sizes() -> Vec<String> {
    std::iter::once(0)
        .chain(super::model::SIZES)
        .map(|s| s.to_string())
        .collect()
}

/// The targets a rule of `kind` may take: the heads, then the targets of
/// its kind that are not heads.
pub fn targets(flow: &Flow, kind: Kind) -> Vec<String> {
    flow.heads
        .iter()
        .map(|h| h.name.clone())
        .chain(
            Special::ALL
                .iter()
                .filter(|s| s.kind() == kind)
                .map(|s| s.name().to_owned()),
        )
        .collect()
}

/// The selectors a rule of `kind` offers: the match keys of the devices
/// present of that kind, the strongest first, then `*`.
fn selectors(flow: &Flow, kind: Kind, first: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = first.map(str::to_owned).into_iter().collect();
    for dev in flow.devices(kind).iter().filter(|d| d.found) {
        for key in &dev.keys {
            if !out.contains(key) {
                out.push(key.clone());
            }
        }
    }
    if !out.iter().any(|s| s == "*") {
        out.push("*".to_owned());
    }
    out
}

/// The look names a head may take: the file's looks and `default`.
fn look_names(flow: &Flow) -> Vec<String> {
    let mut out: Vec<String> = flow.looks.iter().map(|l| l.name.clone()).collect();
    if !out.iter().any(|l| l == "default") {
        out.push("default".to_owned());
    }
    out
}

/// The look `item` shows, by its index in the flow's looks.
pub fn look_at(flow: &Flow, item: Item) -> Option<usize> {
    match item {
        Item::HeadLook(h) => flow.look_of(h),
        Item::Look(l) => Some(l),
        _ => None,
    }
}

/// A form's fields for the `new…` item `new`, with the values typed or
/// picked so far.
fn form(flow: &Flow, new: New, values: &[String], frits: &[String]) -> Vec<Field> {
    let value = |i: usize| values.get(i).cloned().unwrap_or_default();
    let field = |label: &str, i: usize, options: Vec<String>, typed: bool| Field {
        label: label.to_owned(),
        current: value(i),
        options,
        typed,
        what: What::Form(i),
    };
    let mut out = match new {
        New::DisplayRule | New::InputRule => {
            let kind = if new == New::DisplayRule {
                Kind::Display
            } else {
                Kind::Input
            };
            vec![
                field(super::words::MATCHES, 0, selectors(flow, kind, None), true),
                field(super::words::SENDS_TO, 1, targets(flow, kind), true),
            ]
        }
        New::Head => vec![
            field(super::words::NAME, 0, Vec::new(), true),
            field(super::words::LOOK, 1, look_names(flow), false),
        ],
        New::Look => vec![
            field(super::words::NAME, 0, Vec::new(), true),
            field(super::words::FRIT, 1, frits.to_vec(), true),
            field(super::words::SIZE, 2, sizes(), true),
        ],
    };
    out.push(Field {
        label: String::new(),
        current: String::new(),
        options: vec![ADD.to_owned()],
        typed: false,
        what: What::Actions,
    });
    out
}

/// The fields of the card of `item`, which a form's `values` fill for a
/// `new…` item; `frits` are the frits a look may name. An item the
/// machine supplies, such as a display found, has none.
pub fn fields(flow: &Flow, item: Item, values: &[String], frits: &[String]) -> Vec<Field> {
    let actions = |options: &[&str]| Field {
        label: String::new(),
        current: String::new(),
        options: options.iter().map(|o| (*o).to_owned()).collect(),
        typed: false,
        what: What::Actions,
    };
    match item {
        Item::Rule(r) => {
            let Some(rule) = flow.rules.get(r) else {
                return Vec::new();
            };
            let mut targets = targets(flow, rule.kind);
            if !targets.contains(&rule.target) {
                targets.push(rule.target.clone());
            }
            vec![
                Field {
                    label: super::words::SENDS_TO.to_owned(),
                    current: rule.target.clone(),
                    options: targets,
                    typed: true,
                    what: What::RuleTarget(r),
                },
                Field {
                    label: super::words::MATCHES.to_owned(),
                    current: rule.selector.clone(),
                    options: selectors(flow, rule.kind, Some(&rule.selector)),
                    typed: true,
                    what: What::RuleSelector(r),
                },
                actions(&[UP, DOWN, DELETE]),
            ]
        }
        Item::Head(h) => {
            let Some(head) = flow.heads.get(h) else {
                return Vec::new();
            };
            let value = |k: &str| {
                if k == "consoles" {
                    return head.consoles.to_string();
                }
                head.keys
                    .iter()
                    .find(|(n, _)| n == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            let mut out = vec![
                Field {
                    label: super::words::LOOK.to_owned(),
                    current: head.look.clone(),
                    options: look_names(flow),
                    typed: false,
                    what: What::HeadLook(head.name.clone()),
                },
                Field {
                    label: super::words::NAME.to_owned(),
                    current: head.name.clone(),
                    options: Vec::new(),
                    typed: true,
                    what: What::HeadName(head.name.clone()),
                },
            ];
            for key in HEAD_KEYS {
                let options = match key {
                    "consoles" => (1..=6).map(|n| n.to_string()).collect(),
                    "when-display-lost" => vec!["keep".to_owned(), "hangup".to_owned()],
                    "isolated" => vec!["yes".to_owned(), "no".to_owned()],
                    _ => Vec::new(),
                };
                out.push(Field {
                    label: key.to_owned(),
                    current: value(key),
                    options,
                    typed: true,
                    what: What::HeadKey(head.name.clone(), key),
                });
            }
            out.push(actions(&[DELETE]));
            out
        }
        Item::HeadLook(_) | Item::Look(_) => {
            let Some(look) = look_at(flow, item).and_then(|l| flow.looks.get(l)) else {
                return Vec::new();
            };
            let mut out = vec![Field {
                label: super::words::NAME.to_owned(),
                current: look.name.clone(),
                options: Vec::new(),
                typed: true,
                what: What::LookName(look.name.clone()),
            }];
            for (key, _) in LOOK_KEYS {
                let (current, options) = match key {
                    "font-name" => (look.frit.clone(), frits.to_vec()),
                    "font-size" => (look.points.to_string(), sizes()),
                    _ => {
                        let options = match keys::find(key).map(|k| k.kind) {
                            Some(KeyKind::Choice(words)) => {
                                words.iter().map(|w| (*w).to_owned()).collect()
                            }
                            _ => Vec::new(),
                        };
                        (String::new(), options)
                    }
                };
                out.push(Field {
                    label: key.to_owned(),
                    current,
                    options,
                    typed: true,
                    what: What::LookKey(look.name.clone(), key),
                });
            }
            out.push(actions(&[DELETE]));
            out
        }
        Item::Display(d) | Item::Input(d) => {
            let devices = if matches!(item, Item::Display(_)) {
                &flow.displays
            } else {
                &flow.inputs
            };
            // a device not found stands for its rule, which deleting it
            // deletes
            match devices.get(d) {
                Some(Device { found: false, .. }) => vec![actions(&[DELETE])],
                _ => Vec::new(),
            }
        }
        Item::Base => super::model::setting_keys()
            .into_iter()
            .map(|k| {
                let current = flow
                    .base
                    .set
                    .iter()
                    .find(|(n, _)| n == k.name)
                    .map(|(_, v)| {
                        if v.is_empty() {
                            "on".to_owned()
                        } else {
                            v.clone()
                        }
                    })
                    .unwrap_or_default();
                let options = match k.kind {
                    KeyKind::Switch => vec!["on".to_owned(), "off".to_owned()],
                    KeyKind::Choice(words) => words.iter().map(|w| (*w).to_owned()).collect(),
                    _ => Vec::new(),
                };
                Field {
                    label: k.name.to_owned(),
                    current,
                    options,
                    typed: !matches!(k.kind, KeyKind::Switch),
                    what: What::Top(k),
                }
            })
            .collect(),
        Item::New(new) => form(flow, new, values, frits),
        Item::Special(_) | Item::Policy => Vec::new(),
    }
}

/// The change picking `value` in `field` makes, on the card of `item`; an
/// empty value leaves a key at its default. `None` for a field whose pick
/// changes no file, such as a form's.
pub fn pick(flow: &Flow, field: &Field, value: &str) -> Option<Change> {
    let unset = || Some(value.to_owned()).filter(|v| !v.is_empty());
    Some(match &field.what {
        What::RuleTarget(r) => Change::RuleSet {
            number: r.saturating_add(1),
            selector: None,
            target: Some(value.to_owned()),
        },
        What::RuleSelector(r) => Change::RuleSet {
            number: r.saturating_add(1),
            selector: Some(value.to_owned()),
            target: None,
        },
        What::HeadLook(name) => Change::HeadKey {
            name: name.clone(),
            key: "look",
            value: Some(value.to_owned()),
        },
        What::HeadKey(name, key) => Change::HeadKey {
            name: name.clone(),
            key,
            value: unset(),
        },
        What::HeadName(name) => Change::HeadRename {
            name: name.clone(),
            new: value.to_owned(),
        },
        What::LookKey(name, key) => Change::LookKey {
            name: name.clone(),
            key,
            value: unset(),
        },
        What::LookName(name) => Change::LookRename {
            name: name.clone(),
            new: value.to_owned(),
        },
        What::Top(k) => Change::Top {
            key: k.name,
            value: unset(),
        },
        What::Form(_) | What::Actions => {
            let _ = flow;
            return None;
        }
    })
}

/// The change a `new…` form's values make when the user adds it, or why
/// it cannot be added yet.
pub fn add(new: New, values: &[String]) -> Result<Change, String> {
    let value = |i: usize| {
        values
            .get(i)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    match new {
        New::DisplayRule | New::InputRule => {
            let kind = if new == New::DisplayRule {
                "display"
            } else {
                "input"
            };
            match (value(0), value(1)) {
                (Some(s), Some(t)) => Ok(Change::RuleAdd(format!("{kind} {s} -> {t}"))),
                _ => Err(super::words::FORM_RULE_NEEDS.to_owned()),
            }
        }
        New::Head => match value(0) {
            Some(name) => Ok(Change::HeadAdd {
                name,
                look: value(1),
            }),
            None => Err(super::words::FORM_NAME_NEEDED.to_owned()),
        },
        New::Look => match value(0) {
            Some(name) => Ok(Change::LookAdd {
                name,
                frit: value(1),
                size: value(2),
            }),
            None => Err(super::words::FORM_NAME_NEEDED.to_owned()),
        },
    }
}

/// The rule a device not found stands for, by its index.
pub fn placeholder_rule(flow: &Flow, item: Item) -> Option<usize> {
    let dev = match item {
        Item::Display(d) => flow.displays.get(d),
        Item::Input(d) => flow.inputs.get(d),
        _ => None,
    }?;
    (!dev.found).then_some(dev.rule).flatten()
}

/// The first rule that names the device `dev` of `kind` alone: a rule
/// whose selector names one device and is one of the device's match keys.
fn own_rule(flow: &Flow, kind: Kind, dev: &Device) -> Option<usize> {
    flow.rules_of(kind)
        .find(|(_, r)| !general(&r.selector) && dev.keys.contains(&r.selector))
        .map(|(i, _)| i)
}

/// A tagged item, sorted by what it is.
struct Tags {
    devices: Vec<(Kind, Device)>,
    rules: Vec<usize>,
    /// Heads and the targets that are not heads, by name, each with the
    /// kind of rule that may send to it, `None` for a head.
    targets: Vec<(String, Option<Kind>)>,
    looks: Vec<String>,
}

fn sort(flow: &Flow, tags: &[Item]) -> Tags {
    let mut out = Tags {
        devices: Vec::new(),
        rules: Vec::new(),
        targets: Vec::new(),
        looks: Vec::new(),
    };
    for tag in tags {
        match *tag {
            Item::Display(d) => {
                if let Some(dev) = flow.displays.get(d) {
                    out.devices.push((Kind::Display, dev.clone()));
                }
            }
            Item::Input(d) => {
                if let Some(dev) = flow.inputs.get(d) {
                    out.devices.push((Kind::Input, dev.clone()));
                }
            }
            Item::Rule(r) => out.rules.push(r),
            Item::Head(h) => {
                if let Some(head) = flow.heads.get(h) {
                    out.targets.push((head.name.clone(), None));
                }
            }
            Item::Special(s) => out.targets.push((s.name().to_owned(), Some(s.kind()))),
            Item::HeadLook(_) | Item::Look(_) => {
                if let Some(l) = look_at(flow, *tag).and_then(|l| flow.looks.get(l)) {
                    out.looks.push(l.name.clone());
                }
            }
            Item::Base | Item::Policy | Item::New(_) => {}
        }
    }
    out
}

/// The changes `c` makes of the tagged items `tags` (ADR-1403 D7), or what
/// is missing or does not fit.
pub fn connect(flow: &Flow, tags: &[Item]) -> Result<Vec<Change>, String> {
    use super::words as w;
    let t = sort(flow, tags);
    if t.targets.len() > 1 {
        return Err(w::CONNECT_TWO_TARGETS.to_owned());
    }
    if t.looks.len() > 1 {
        return Err(w::CONNECT_TWO_LOOKS.to_owned());
    }
    if t.rules.len() > 1 {
        return Err(w::CONNECT_TWO_RULES.to_owned());
    }
    let target = t.targets.first();
    let rule = t
        .rules
        .first()
        .and_then(|r| flow.rules.get(*r).map(|x| (*r, x)));
    // every device and the rule are of one kind, and the target takes it
    let mut kinds: Vec<Kind> = t.devices.iter().map(|(k, _)| *k).collect();
    kinds.extend(rule.map(|(_, r)| r.kind));
    kinds.dedup();
    if kinds.len() > 1 {
        return Err(w::CONNECT_TWO_KINDS.to_owned());
    }
    let kind = kinds.first().copied();
    if let (Some((name, Some(takes))), Some(kind)) = (target, kind)
        && *takes != kind
    {
        return Err(match kind {
            Kind::Display => w::connect_display_to(name),
            Kind::Input => w::connect_input_to(name),
        });
    }
    let mut out = Vec::new();
    match (rule, t.devices.as_slice()) {
        (Some((r, rule)), devices) => {
            if devices.len() > 1 {
                return Err(w::CONNECT_RULE_ONE_DEVICE.to_owned());
            }
            let selector = devices.first().and_then(|(_, dev)| {
                let key = dev.keys.first()?;
                // a rule that already matches the device stays as it is
                let matches = dev.found && dev.rule == Some(r) || dev.keys.contains(&rule.selector);
                (!matches).then(|| key.clone())
            });
            let target = target.map(|(n, _)| n.clone()).filter(|n| *n != rule.target);
            if selector.is_some() || target.is_some() {
                out.push(Change::RuleSet {
                    number: r.saturating_add(1),
                    selector,
                    target,
                });
            }
        }
        (None, []) => {}
        (None, devices) => {
            let Some((name, _)) = target else {
                return Err(w::CONNECT_NO_HEAD.to_owned());
            };
            for (kind, dev) in devices {
                match own_rule(flow, *kind, dev) {
                    Some(r) => {
                        if flow.rules.get(r).is_some_and(|x| x.target != *name) {
                            out.push(Change::RuleSet {
                                number: r.saturating_add(1),
                                selector: None,
                                target: Some(name.clone()),
                            });
                        }
                    }
                    None => {
                        let Some(key) = dev.keys.first() else {
                            return Err(w::connect_no_key(&dev.name));
                        };
                        let word = match kind {
                            Kind::Display => "display",
                            Kind::Input => "input",
                        };
                        out.push(Change::RuleAdd(format!("{word} {key} -> {name}")));
                    }
                }
            }
        }
    }
    if let Some(look) = t.looks.first() {
        match target {
            Some((head, None)) => {
                let now = flow.heads.iter().find(|h| h.name == *head).map(|h| &h.look);
                if now != Some(look) {
                    out.push(Change::HeadKey {
                        name: head.clone(),
                        key: "look",
                        value: Some(look.clone()),
                    });
                }
            }
            Some((other, Some(_))) => return Err(w::connect_look_to(other)),
            None => return Err(w::CONNECT_LOOK_NO_HEAD.to_owned()),
        }
    }
    if out.is_empty() && t.rules.is_empty() && t.devices.is_empty() && t.looks.is_empty() {
        return Err(w::CONNECT_NOTHING.to_owned());
    }
    if out.is_empty() && rule.is_none() && target.is_none() {
        return Err(w::CONNECT_NO_HEAD.to_owned());
    }
    Ok(out)
}

/// The rule numbers that send to the head `name`, as the delete's question
/// names them.
pub fn rules_to(flow: &Flow, name: &str) -> Vec<usize> {
    flow.rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.target == name && matches!(flow.target(r), Target::Head(_)))
        .map(|(i, _)| i.saturating_add(1))
        .collect()
}
