//! The flow of a draft with the devices present placed on it (ADR-1403
//! D7, D8): [`build`] makes the Flow view's model, [`tui::flow::Flow`],
//! from a draft's text and the devices [`present`] reads, and [`text`]
//! and [`json`] print it for `vtium config flow`. [`routes`] lists the
//! complete routes an item is part of, which the route card and `vtium
//! config route` show.
//!
//! Each device is placed by the first rule that matches it, through
//! vtium-settings's own walk over the rules, and the devices are mapped
//! into the identities the rules match by vtium-devices, the functions
//! the console and `vtium check-config` place them with, so the view, the
//! check and the console agree on where a device goes.

use std::fmt::Write as _;
use std::path::Path;

use vtium_platform::listing::{self, Roots};
use vtium_settings::{By, Identity, Kind as IdKind, Policy, Warning, WhenDisplayLost, split};

use crate::ops::RuleText;
use crate::tui::flow::{
    Base, Device, Draft, Flow, Head, Item, Kind, Look, Rule, RuleState, Special, Target, general,
};

/// The devices present, and whether a udev daemon runs, which the keys
/// `path`, `serial`, `card`, `connector` and `udev` need.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Present {
    pub devices: Vec<Identity>,
    pub udev: bool,
}

/// The devices of seat0 under `roots`, read from sysfs and udev's database
/// without privilege, opening nothing under `/dev`, and whether the udev
/// daemon whose control socket is `control` runs.
pub fn present(roots: &Roots, control: &Path) -> Present {
    Present {
        devices: vtium_devices::devices(&listing::list(roots, vtium_devices::SEAT)),
        udev: vtium_devices::udev_daemon_runs(control),
    }
}

/// A rule of `[rules]` as the file writes it, whether or not vtium keeps
/// it: its line and its words.
struct Written {
    line: usize,
    rule: RuleText,
}

/// Every rule line of `text`'s `[rules]` sections, in order, numbered as
/// the section writer and the commands number them, including the lines
/// vtium cannot read, which keep their words as best they read.
fn written_rules(text: &str) -> Vec<Written> {
    let lines: Vec<&str> = text.lines().collect();
    vtium_settings::rule_lines(text)
        .into_iter()
        .map(|line| {
            let body = lines
                .get(line.saturating_sub(1))
                .copied()
                .unwrap_or_default();
            let rule = RuleText::read(body).unwrap_or_else(|| {
                let content = body.split(" #").next().unwrap_or_default().trim();
                let (head, target) = content.rsplit_once("->").unwrap_or((content, ""));
                let mut words = head.split_whitespace();
                RuleText {
                    kind: words.next().unwrap_or("input").to_owned(),
                    selector: words.collect::<Vec<_>>().join(" "),
                    target: target.trim().to_owned(),
                }
            });
            Written { line, rule }
        })
        .collect()
}

/// A display's name: its connector, else `display`.
fn display_name(id: &Identity) -> String {
    id.connector.clone().unwrap_or_else(|| "display".to_owned())
}

/// An input's name: the name it reports, else `input`.
fn input_name(id: &Identity) -> String {
    id.name.clone().unwrap_or_else(|| "input".to_owned())
}

/// Every match key `id` offers, the strongest first, as `vtium config flow
/// --json` gives them (ADR-1401 D12).
pub fn keys(id: &Identity) -> Vec<String> {
    let mut out = Vec::new();
    match id.kind {
        IdKind::Display => {
            if let Some(e) = &id.edid {
                out.push(format!("edid:{}:{:#06x}:{}", e.maker, e.product, e.serial));
            }
            if let Some(p) = &id.connector_path {
                out.push(format!("connector-path:{p}"));
            }
            if let (Some(card), Some(c)) = (&id.card, &id.connector) {
                out.push(format!("connector:{card}/{c}"));
            }
            if let Some(card) = &id.card {
                out.push(format!("card:{card}"));
            }
        }
        IdKind::Input => {
            if let Some(p) = &id.path {
                out.push(format!("path:{p}"));
            }
            if let Some(s) = &id.serial {
                out.push(format!(
                    "serial:{:04x}:{:04x}:{}",
                    s.vendor, s.product, s.serial
                ));
            }
            if let Some(b) = &id.unique {
                out.push(format!("bluetooth:{b}"));
            }
            if let Some((v, p)) = id.id {
                out.push(format!("id:{v:04x}:{p:04x}"));
            }
            for class in vtium_settings::Class::ALL {
                if id.classes.has(class) {
                    out.push(format!("class:{}", class.name()));
                }
            }
        }
    }
    out
}

/// What tells a device apart under its name: a display's EDID serial,
/// else its connector's path; an input's port, else its classes.
fn detail(id: &Identity) -> String {
    match id.kind {
        IdKind::Display => id
            .edid
            .as_ref()
            .map(|e| e.serial.clone())
            .or_else(|| id.connector_path.clone())
            .unwrap_or_default(),
        IdKind::Input => id.path.clone().unwrap_or_else(|| {
            vtium_settings::Class::ALL
                .into_iter()
                .filter(|c| id.classes.has(*c))
                .map(|c| c.name())
                .collect::<Vec<_>>()
                .join(", ")
        }),
    }
}

/// The base card: the keys `text`'s top level sets, each once with the
/// value its last line gives, and how many keys of the table are left at
/// their defaults.
fn base(text: &str) -> Base {
    let mut set: Vec<(String, String)> = Vec::new();
    // the keys a line sets by name, which an old name never replaces
    let mut renamed_over: Vec<String> = Vec::new();
    for (_, line) in split(text.as_bytes()).top_lines() {
        let Some(read) = vtium_settings::key_line(line) else {
            continue;
        };
        let key = String::from_utf8_lossy(read.key).into_owned();
        if key == "version" {
            continue;
        }
        // a key binding set by its old name shows under its new name, and
        // its own line wins (ADR-1005 D10)
        let (key, old) = match vtium_settings::keys::renamed(&key) {
            Some((_, k)) => (k.name.to_owned(), true),
            None => (key, false),
        };
        if old && renamed_over.contains(&key) {
            continue;
        }
        let value = read
            .value
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .unwrap_or_default();
        if !old {
            renamed_over.push(key.clone());
        }
        set.retain(|(k, _)| *k != key);
        set.push((key, value));
    }
    let in_files = vtium_settings::keys::KEYS
        .iter()
        .filter(|k| k.scope == vtium_settings::keys::Scope::Anywhere)
        .count();
    let counted = set
        .iter()
        .filter(|(k, _)| {
            let name = k.strip_prefix("no-").unwrap_or(k);
            vtium_settings::keys::find(name)
                .is_some_and(|k| k.scope == vtium_settings::keys::Scope::Anywhere)
        })
        .count();
    Base {
        defaults: in_files.saturating_sub(counted),
        set,
    }
}

/// `when-display-lost` as the file writes it.
fn when_lost(w: WhenDisplayLost) -> String {
    match w {
        WhenDisplayLost::Keep => "keep".to_owned(),
        WhenDisplayLost::Hangup => "hangup".to_owned(),
        WhenDisplayLost::HangupAfter(s) => format!("hangup {s}"),
    }
}

/// The flow of the draft `text` with the devices `present` placed on it,
/// and `draft`, how it stands against the installed file.
pub fn build(text: &str, present: &Present, draft: Option<Draft>) -> Flow {
    let reading = vtium_settings::sections(text.as_bytes());
    let config = reading
        .as_ref()
        .map(|r| r.parsed.config.clone())
        .unwrap_or_else(vtium_settings::Config::builtin);
    let warnings: Vec<Warning> = reading
        .as_ref()
        .map(|r| r.parsed.warnings.clone())
        .unwrap_or_default();
    let written = if reading.is_some() {
        written_rules(text)
    } else {
        written_rules(vtium_settings::TEMPLATE)
    };
    let index_of_line = |line: usize| written.iter().position(|w| w.line == line);
    let needs_udev: Vec<usize> = config.needs_udev().into_iter().map(|(l, _)| l).collect();

    // each device present, placed by its first matching rule
    let mut displays = Vec::new();
    let mut inputs = Vec::new();
    for id in &present.devices {
        let rule = match config.walk(id).by {
            By::Rule(line) => index_of_line(line),
            _ => None,
        };
        let (name, list) = match id.kind {
            IdKind::Display => (display_name(id), &mut displays),
            IdKind::Input => (input_name(id), &mut inputs),
        };
        list.push(Device {
            name,
            detail: detail(id),
            keys: keys(id),
            found: true,
            rule,
        });
    }
    let first_match_of = |i: usize| displays.iter().chain(&inputs).any(|d| d.rule == Some(i));

    let rules: Vec<Rule> = written
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let kind = if w.rule.kind == "display" {
                Kind::Display
            } else {
                Kind::Input
            };
            let kept = config.rules.iter().any(|r| r.line == w.line);
            let error = reading.as_ref().and_then(|r| {
                r.errors
                    .errors
                    .iter()
                    .find(|e| e.line == Some(w.line))
                    .map(|e| e.problem.to_string())
            });
            let shadowed = warnings.iter().find_map(|warning| match warning {
                Warning::Shadowed { line, by } if *line == w.line => index_of_line(*by),
                _ => None,
            });
            let state = match (kept, error) {
                (false, error) => {
                    RuleState::Error(error.unwrap_or_else(|| "vtium ignores this rule".to_owned()))
                }
                _ if !present.udev && needs_udev.contains(&w.line) => RuleState::NeedsUdev,
                _ if shadowed.is_some() => RuleState::Shadowed(shadowed.unwrap_or_default()),
                _ if first_match_of(i) => RuleState::Matches,
                _ => RuleState::MatchesNothing,
            };
            Rule {
                kind,
                line: w.line,
                selector: w.rule.selector.clone(),
                target: w.rule.target.clone(),
                state,
            }
        })
        .collect();

    // a rule that names one device stands for it while it is unplugged
    for (i, rule) in rules.iter().enumerate() {
        if general(&rule.selector) {
            continue;
        }
        let list = match rule.kind {
            Kind::Display => &mut displays,
            Kind::Input => &mut inputs,
        };
        let matcher = config.rules.iter().find(|r| r.line == rule.line);
        let named = present
            .devices
            .iter()
            .any(|id| matcher.is_some_and(|r| r.kind == id.kind && r.matcher.matches(id)));
        if !named {
            let value = rule
                .selector
                .split_once(':')
                .map_or(rule.selector.as_str(), |(_, v)| v);
            let missing = Device {
                name: value.to_owned(),
                detail: String::new(),
                keys: vec![rule.selector.clone()],
                found: false,
                rule: Some(i),
            };
            list.push(missing);
        }
    }

    let heads = config
        .heads
        .iter()
        .map(|h| {
            let mut keys = Vec::new();
            if let Some(login) = &h.login {
                keys.push(("login".to_owned(), login.clone()));
            }
            if h.when_display_lost != WhenDisplayLost::Keep {
                keys.push((
                    "when-display-lost".to_owned(),
                    when_lost(h.when_display_lost),
                ));
            }
            if let Some(isolated) = h.isolated {
                keys.push((
                    "isolated".to_owned(),
                    if isolated { "yes" } else { "no" }.to_owned(),
                ));
            }
            Head {
                name: h.name.clone(),
                consoles: h.consoles,
                look: h.look.clone(),
                keys,
            }
        })
        .collect();
    let looks = config
        .looks
        .iter()
        .map(|l| {
            let value = |key: &str| {
                l.settings
                    .iter()
                    .find(|s| s.key.name == key)
                    .map(|s| (s.value.clone(), s.line))
            };
            let frit = value("font-name");
            let problem = warnings.iter().find_map(|w| match w {
                Warning::MissingFrit { line, .. }
                    if frit.as_ref().is_some_and(|(_, l)| l == line) =>
                {
                    Some(w.to_string())
                }
                _ => None,
            });
            Look {
                name: l.name.clone(),
                frit: frit.map(|(f, _)| f).unwrap_or_default(),
                points: value("font-size")
                    .and_then(|(v, _)| v.parse().ok())
                    .unwrap_or(0),
                keys: l
                    .settings
                    .iter()
                    .filter(|s| !matches!(s.key.name, "font-name" | "font-size"))
                    .map(|s| (s.key.name.to_owned(), s.value.clone()))
                    .collect(),
                fonts: Vec::new(),
                problem,
            }
        })
        .collect();
    Flow {
        base: base(text),
        displays,
        inputs,
        rules,
        heads,
        looks,
        active: None,
        policy: match config.policy {
            Policy::Open => "open".to_owned(),
            Policy::Closed => "closed".to_owned(),
        },
        draft,
        read: true,
        labels: config
            .labels
            .iter()
            .map(|l| {
                let kind = match l.kind {
                    vtium_settings::Kind::Display => Kind::Display,
                    vtium_settings::Kind::Input => Kind::Input,
                };
                (kind, l.selector.clone(), l.text.clone())
            })
            .collect(),
    }
}

/// The words of a rule's state, as the view and `flow` print them; empty
/// for a rule that matches a device present.
pub fn state_words(flow: &Flow, rule: &Rule) -> String {
    match &rule.state {
        RuleState::Matches => String::new(),
        RuleState::MatchesNothing if general(&rule.selector) => match rule.kind {
            Kind::Display => "matches no connected display".to_owned(),
            Kind::Input => "matches no connected device".to_owned(),
        },
        RuleState::MatchesNothing => "in a rule, not connected".to_owned(),
        RuleState::Shadowed(by) => format!("rule {} matches first", by.saturating_add(1)),
        RuleState::NeedsUdev => "needs udev".to_owned(),
        RuleState::Error(why) => why.clone(),
    }
    .to_owned()
        + match flow.target(rule) {
            Target::Missing if !matches!(rule.state, RuleState::Error(_)) => "! head not found",
            _ => "",
        }
}

/// The first line of `flow` and of the status line: the draft's name and
/// state against the installed file, and its problems.
pub fn state_line(name: &str, state: &crate::drafts::State, problems: usize) -> String {
    let problems = match problems {
        0 => "no problems".to_owned(),
        1 => "1 problem".to_owned(),
        n => format!("{n} problems"),
    };
    format!("draft {name}: {} · {problems}", state.words())
}

/// The flow as `vtium config flow` prints it, under `first`, the state
/// line.
pub fn text(flow: &Flow, first: &str, missing: &[String]) -> String {
    let mut out = format!("{first}\n");
    let device = |out: &mut String, d: &Device| {
        let mut line = format!("  {}", d.name);
        if !d.detail.is_empty() {
            let _ = write!(line, " ({})", d.detail);
        }
        if !d.found {
            line.push_str(" * in a rule, not connected");
        }
        match d.rule {
            Some(r) => {
                let _ = write!(line, " -> rule {}", r.saturating_add(1));
            }
            None => {
                let _ = write!(line, " -> policy {}", flow.policy);
            }
        }
        let _ = writeln!(out, "{line}");
    };
    let rules = |out: &mut String, kind: Kind| {
        for (i, r) in flow.rules_of(kind) {
            let words = state_words(flow, r);
            let mut line = format!(
                "  {} {} {} -> {}",
                i.saturating_add(1),
                match kind {
                    Kind::Display => "display",
                    Kind::Input => "input",
                },
                r.selector,
                r.target
            );
            if !words.is_empty() {
                let _ = write!(line, " ({words})");
            }
            let _ = writeln!(out, "{line}");
        }
    };
    out.push_str("displays\n");
    for d in &flow.displays {
        device(&mut out, d);
    }
    out.push_str("display rules\n");
    rules(&mut out, Kind::Display);
    out.push_str("heads\n");
    for h in &flow.heads {
        let consoles = if h.consoles == 1 {
            "1 console".to_owned()
        } else {
            format!("{} consoles", h.consoles)
        };
        let mut line = format!("  {}: {consoles}, look {}", h.name, h.look);
        for (k, v) in &h.keys {
            let _ = write!(line, ", {k} {v}");
        }
        let _ = writeln!(out, "{line}");
    }
    out.push_str("looks\n");
    for (i, l) in flow.looks.iter().enumerate() {
        let mut line = format!("  {}:", l.name);
        if !l.frit.is_empty() {
            let _ = write!(line, " frit {}", l.frit);
        }
        if l.points > 0 {
            let _ = write!(line, " {} pt", l.points);
        }
        if l.frit.is_empty() && l.points == 0 {
            line.push_str(" the font set at the top of vtium.conf");
        }
        if !flow.used(i) {
            line.push_str(" (no head uses it)");
        }
        if let Some(p) = &l.problem {
            let _ = write!(line, " ({p})");
        }
        let _ = writeln!(out, "{line}");
    }
    out.push_str("input rules\n");
    rules(&mut out, Kind::Input);
    out.push_str("input devices\n");
    for d in &flow.inputs {
        device(&mut out, d);
    }
    let _ = writeln!(out, "policy {}", flow.policy);
    for m in missing {
        let _ = writeln!(out, "missing: {m}");
    }
    out
}

/// `s` as a JSON string.
pub fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A list of JSON values.
fn list(items: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

/// The JSON of the rule state's kind.
fn state_kind(state: &RuleState) -> &'static str {
    match state {
        RuleState::Matches => "matches",
        RuleState::MatchesNothing => "matches-nothing",
        RuleState::Shadowed(_) => "shadowed",
        RuleState::NeedsUdev => "needs-udev",
        RuleState::Error(_) => "error",
    }
}

/// The flow as `vtium config flow --json` prints it: one object whose
/// `version` is 1, with the draft's state, so a script can rely on its
/// fields. `problems` in the draft is the count the state line gives, and
/// the top-level `problems` lists every line the check printed. Rules and the devices' `rule` are numbered from 1, as the
/// commands take them.
pub fn json(
    flow: &Flow,
    name: &str,
    state: &crate::drafts::State,
    checked: &crate::drafts::Checked,
    missing: &[String],
) -> String {
    let problems = &checked.problems;
    let device = |d: &Device| {
        format!(
            "{{\"name\":{},\"detail\":{},\"keys\":{},\"found\":{},\"rule\":{}}}",
            quoted(&d.name),
            quoted(&d.detail),
            list(d.keys.iter().map(|k| quoted(k))),
            d.found,
            d.rule
                .map_or_else(|| "null".to_owned(), |r| r.saturating_add(1).to_string())
        )
    };
    let rules = flow.rules.iter().enumerate().map(|(i, r)| {
        format!(
            "{{\"number\":{},\"line\":{},\"kind\":{},\"selector\":{},\"target\":{},\"state\":{},\"words\":{}}}",
            i.saturating_add(1),
            r.line,
            quoted(match r.kind {
                Kind::Display => "display",
                Kind::Input => "input",
            }),
            quoted(&r.selector),
            quoted(&r.target),
            quoted(state_kind(&r.state)),
            quoted(&state_words(flow, r))
        )
    });
    let heads = flow.heads.iter().map(|h| {
        format!(
            "{{\"name\":{},\"consoles\":{},\"look\":{},\"keys\":{}}}",
            quoted(&h.name),
            h.consoles,
            quoted(&h.look),
            list(h.keys.iter().map(|(k, v)| format!(
                "{{\"key\":{},\"value\":{}}}",
                quoted(k),
                quoted(v)
            )))
        )
    });
    let looks = flow.looks.iter().enumerate().map(|(i, l)| {
        format!(
            "{{\"name\":{},\"frit\":{},\"points\":{},\"used\":{},\"problem\":{}}}",
            quoted(&l.name),
            quoted(&l.frit),
            l.points,
            flow.used(i),
            l.problem
                .as_deref()
                .map_or_else(|| "null".to_owned(), quoted)
        )
    });
    let base = list(
        flow.base
            .set
            .iter()
            .map(|(k, v)| format!("{{\"key\":{},\"value\":{}}}", quoted(k), quoted(v))),
    );
    let (added, removed) = state.counts();
    format!(
        "{{\"version\":1,\"draft\":{{\"name\":{},\"state\":{},\"added\":{added},\"removed\":{removed},\"problems\":{}}},\"policy\":{},\"base\":{{\"set\":{base},\"defaults\":{}}},\"displays\":{},\"rules\":{},\"heads\":{},\"looks\":{},\"inputs\":{},\"problems\":{},\"missing\":{}}}\n",
        quoted(name),
        quoted(state.kind()),
        checked.count(),
        quoted(&flow.policy),
        flow.base.defaults,
        list(flow.displays.iter().map(device)),
        list(rules),
        list(heads),
        list(looks),
        list(flow.inputs.iter().map(device)),
        list(problems.iter().map(|p| quoted(p))),
        list(missing.iter().map(|m| quoted(m))),
    )
}

/// One complete route of the flow: a display, its rule, the head that
/// rule sends it to, an input rule that reaches the head and an input
/// device it places, each by its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub display: usize,
    pub display_rule: usize,
    pub head: usize,
    pub input_rule: usize,
    pub input: usize,
}

impl Route {
    /// Whether `item` is on the route.
    pub fn has(&self, flow: &Flow, item: Item) -> bool {
        match item {
            Item::Display(d) => d == self.display,
            Item::Input(d) => d == self.input,
            Item::Rule(r) => r == self.display_rule || r == self.input_rule,
            Item::Head(h) | Item::HeadLook(h) => h == self.head,
            Item::Look(l) => flow.look_of(self.head) == Some(l),
            Item::Special(Special::Roaming) => flow
                .rules
                .get(self.input_rule)
                .is_some_and(|r| flow.target(r) == Target::Special(Special::Roaming)),
            Item::Base => true,
            Item::Special(_) | Item::Policy | Item::New(_) => false,
        }
    }
}

/// Every complete route of `flow`: each display a rule sends to a head,
/// with each input device whose rule sends it to that head, or to
/// `roaming`, which reaches every head (ADR-1401 D5).
pub fn routes(flow: &Flow) -> Vec<Route> {
    let mut out = Vec::new();
    for (display, d) in flow.displays.iter().enumerate().filter(|(_, d)| d.found) {
        let Some(display_rule) = d.rule else { continue };
        let Some(Target::Head(head)) = flow.rules.get(display_rule).map(|r| flow.target(r)) else {
            continue;
        };
        for (input, i) in flow.inputs.iter().enumerate().filter(|(_, i)| i.found) {
            let Some(input_rule) = i.rule else { continue };
            let reaches = match flow.rules.get(input_rule).map(|r| flow.target(r)) {
                Some(Target::Head(h)) => h == head,
                Some(Target::Special(Special::Roaming)) => true,
                _ => false,
            };
            if reaches {
                out.push(Route {
                    display,
                    display_rule,
                    head,
                    input_rule,
                    input,
                });
            }
        }
    }
    out
}

/// The item `words` names, as `vtium config route` takes it: `display
/// NAME`, `rule N`, `head NAME`, `look NAME` or `input NAME`.
pub fn item(flow: &Flow, words: &[String]) -> Option<Item> {
    let [kind, name] = words else { return None };
    match kind.as_str() {
        "display" => flow
            .displays
            .iter()
            .position(|d| d.name == *name)
            .map(Item::Display),
        "input" => flow
            .inputs
            .iter()
            .position(|d| d.name == *name)
            .map(Item::Input),
        "rule" => name
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .filter(|i| *i < flow.rules.len())
            .map(Item::Rule),
        "head" => flow
            .heads
            .iter()
            .position(|h| h.name == *name)
            .map(Item::Head),
        "look" => flow.looks.iter().position(|l| l.name == *name).map(|l| {
            flow.heads
                .iter()
                .position(|h| h.look == *name)
                .map_or(Item::Look(l), Item::HeadLook)
        }),
        _ => None,
    }
}

/// One line of a route: its section, its words, and whether it is the
/// item the card is for.
pub fn route_lines(
    flow: &Flow,
    route: &Route,
    selected: Item,
) -> Vec<(&'static str, String, bool)> {
    let rule = |i: usize| {
        flow.rules.get(i).map_or_else(String::new, |r| {
            format!(
                "rule {}, {} -> {}",
                i.saturating_add(1),
                r.selector,
                r.target
            )
        })
    };
    let head = flow.heads.get(route.head);
    let look = flow.look_of(route.head).and_then(|l| flow.looks.get(l));
    vec![
        (
            "display",
            flow.displays
                .get(route.display)
                .map_or_else(String::new, |d| d.name.clone()),
            selected == Item::Display(route.display),
        ),
        (
            "display rule",
            rule(route.display_rule),
            selected == Item::Rule(route.display_rule),
        ),
        (
            "head",
            head.map_or_else(String::new, |h| h.name.clone()),
            selected == Item::Head(route.head),
        ),
        (
            "look",
            look.map_or_else(
                || head.map_or_else(String::new, |h| h.look.clone()),
                |l| l.name.clone(),
            ),
            matches!(selected, Item::HeadLook(h) if h == route.head)
                || matches!(selected, Item::Look(l) if flow.look_of(route.head) == Some(l)),
        ),
        (
            "input rule",
            rule(route.input_rule),
            selected == Item::Rule(route.input_rule),
        ),
        (
            "input device",
            flow.inputs
                .get(route.input)
                .map_or_else(String::new, |d| d.name.clone()),
            selected == Item::Input(route.input),
        ),
    ]
}

/// What the route card says of `item` when no route passes through it:
/// that it is broken, as a rule vtium drops or one that sends to a head
/// that does not exist, or that it is not used, and why.
pub fn unrouted(flow: &Flow, item: Item) -> String {
    if let Item::Rule(r) = item
        && let Some(rule) = flow.rules.get(r)
    {
        let words = state_words(flow, rule);
        let broken =
            matches!(rule.state, RuleState::Error(_)) || flow.target(rule) == Target::Missing;
        if broken {
            return format!("broken: {words}");
        }
        if !words.is_empty() {
            return format!("not used: {words}");
        }
    }
    "not used: no complete route goes through it".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_settings::{Class, Edid};

    fn monitor(connector: &str, serial: &str) -> Identity {
        let mut id = Identity::display();
        id.connector = Some(connector.to_owned());
        id.edid = Some(Edid {
            maker: "AAA".to_owned(),
            product: 1,
            serial: serial.to_owned(),
        });
        id
    }

    fn keyboard(name: &str) -> Identity {
        let mut id = Identity::input().with_class(Class::Keyboard);
        id.name = Some(name.to_owned());
        id
    }

    const DESK: &str = "font-size=12\nmouse\nversion = 1\n[look big]\nfont-name = monoid\nfont-size = 14\n[look spare]\n[head desk]\nlook = big\n[rules]\ndisplay edid:AAA:0x0001:LARGE -> desk\ndisplay * -> desk\ninput serial:046d:c31c:X -> desk\ninput class:keyboard -> roaming\n";

    // ADR-1005 D10: the base card shows a key binding set empty as no key,
    // shows a key binding set by its old name under its new name, and a
    // key binding typed empty there is D10's empty value, while the word
    // `default` returns it to its default.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_base_card_shows_and_edits_key_bindings() {
        use crate::tui::edit::{Change, fields, pick};
        use crate::tui::flow::Item;
        let text =
            "grab-paste=<Alt>p\nkey-reset=\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\n";
        let flow = build(
            text,
            &Present {
                devices: Vec::new(),
                udev: true,
            },
            None,
        );
        let all = fields(&flow, Item::Base, &[], &[]);
        let field = |name: &str| all.iter().find(|f| f.label == name).cloned();
        let reset = field("key-reset").expect("listed");
        assert_eq!(reset.current, "none");
        assert_eq!(
            field("key-paste").map(|f| f.current).as_deref(),
            Some("<Alt>p")
        );
        assert!(field("grab-paste").is_none());
        assert_eq!(
            pick(&flow, &reset, ""),
            Some(Change::Top {
                key: "key-reset",
                value: Some(String::new())
            })
        );
        assert_eq!(
            pick(&flow, &reset, "default"),
            Some(Change::Top {
                key: "key-reset",
                value: None
            })
        );
        let size = field("font-size").expect("listed");
        assert_eq!(
            pick(&flow, &size, ""),
            Some(Change::Top {
                key: "font-size",
                value: None
            })
        );
    }

    #[test]
    fn devices_are_placed_by_their_first_rule_and_a_missing_one_stays() {
        let present = Present {
            devices: vec![monitor("DP-8", "LARGE"), keyboard("Keyboard A")],
            udev: true,
        };
        let flow = build(DESK, &present, None);
        assert_eq!(
            flow.displays,
            [Device {
                name: "DP-8".to_owned(),
                detail: "LARGE".to_owned(),
                keys: vec!["edid:AAA:0x0001:LARGE".to_owned()],
                found: true,
                rule: Some(0),
            }]
        );
        assert_eq!(
            flow.inputs,
            [
                Device {
                    name: "Keyboard A".to_owned(),
                    detail: "keyboard".to_owned(),
                    keys: vec!["class:keyboard".to_owned()],
                    found: true,
                    rule: Some(3),
                },
                Device {
                    name: "046d:c31c:X".to_owned(),
                    detail: String::new(),
                    keys: vec!["serial:046d:c31c:X".to_owned()],
                    found: false,
                    rule: Some(2),
                },
            ]
        );
        let states: Vec<&RuleState> = flow.rules.iter().map(|r| &r.state).collect();
        assert_eq!(
            states,
            [
                &RuleState::Matches,
                &RuleState::MatchesNothing,
                &RuleState::MatchesNothing,
                &RuleState::Matches
            ]
        );
        assert_eq!(
            flow.base.set,
            [
                ("font-size".to_owned(), "12".to_owned()),
                ("mouse".to_owned(), String::new())
            ]
        );
        let r = routes(&flow);
        assert_eq!(
            r,
            [Route {
                display: 0,
                display_rule: 0,
                head: 0,
                input_rule: 3,
                input: 0
            }]
        );
        assert_eq!(
            item(&flow, &["look".to_owned(), "spare".to_owned()]),
            Some(Item::Look(1))
        );
        assert_eq!(
            unrouted(&flow, Item::Rule(1)),
            "not used: matches no connected display"
        );
        assert_eq!(item(&flow, &["rule".to_owned(), "0".to_owned()]), None);
    }
}
