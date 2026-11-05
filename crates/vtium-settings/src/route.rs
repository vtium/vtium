//! Whether a configuration wires a complete route (ADR-1403 D3): a display
//! present that its first matching rule sends to a head, and a keyboard
//! present that reaches that head, by an input rule whose target is the
//! head, or by roaming into it when it can be the active head.
//!
//! `vtium check-config` and vtium-config's checked install call
//! [`complete_route`]; the console's start never does, since missing
//! hardware never makes a file invalid at start (ADR-1401 D7). With no
//! display or no keyboard present, or when the caller reads no devices,
//! the check judges the rules alone: some display rule sends displays to a
//! head that some input rule sends keyboards to, directly or by roaming,
//! with a `class:keyboard`, `*` or device rule.

use crate::config::{Config, Rule, Target};
use crate::error::Quoted;
use crate::identity::{Class, Identity, Kind};
use crate::matching::{Match, Token};
use crate::place::Place;

/// How [`complete_route`] judged the configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Judged {
    /// By the devices present.
    Devices,
    /// By the rules alone, since no display is present.
    NoDisplay,
    /// By the rules alone, since no keyboard is present.
    NoKeyboard,
    /// By the rules alone, since the caller read no devices, as `vtium
    /// check-config --no-devices` asks.
    RulesAlone,
}

/// The first thing a complete route lacks, in the order a person wires
/// one: a display sent to a head, then a keyboard that reaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// No display present is sent to a head: the first display falls to
    /// the rule numbered `rule`, which sends it to `off`, or with `None`
    /// to no rule, which leaves it off.
    NoDisplay {
        display: Quoted,
        rule: Option<usize>,
    },
    /// The head shows the display, but no keyboard present reaches it.
    NoKeyboard { head: Quoted, display: Quoted },
    /// The head shows a display, and keyboards roam, but the head is
    /// isolated, so roaming keyboards never reach it.
    Isolated { head: Quoted },
    /// By the rules alone: no display rule sends displays to a head.
    NoDisplayRule,
    /// By the rules alone: the rule numbered `rule` sends displays to the
    /// head, but no input rule sends keyboards to it or to `roaming`.
    NoKeyboardRule { head: Quoted, rule: usize },
}

/// What [`complete_route`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub judged: Judged,
    /// `None` when the configuration wires a complete route.
    pub missing: Option<Missing>,
}

impl Route {
    /// Whether the configuration wires a complete route.
    pub fn complete(&self) -> bool {
        self.missing.is_none()
    }
}

/// Whether a rule's match names one device: by `edid`, `connector`,
/// `connector-path`, `serial`, `path`, `bluetooth` or `name` (ADR-1403
/// D7). Any other match, `*` or one of `class`, `id`, `card` and `udev`
/// alone, is general: it names a kind of device.
pub(crate) fn names_one_device(matcher: &Match) -> bool {
    matcher.tokens().iter().any(|t| {
        matches!(
            t,
            Token::Edid(_)
                | Token::Connector { .. }
                | Token::ConnectorPath(_)
                | Token::Serial(_)
                | Token::Path(_)
                | Token::Bluetooth(_)
                | Token::Name(_)
        )
    })
}

/// The number a rule shows, its place in `[rules]` from 1, found by its
/// line.
fn number(config: &Config, line: usize) -> Option<usize> {
    config
        .rules
        .iter()
        .position(|r| r.line == line)
        .map(|i| i.saturating_add(1))
}

/// Whether an earlier rule of its kind takes every device the rule at
/// `index` matches, so no device reaches it.
fn shadowed(rules: &[Rule], index: usize) -> bool {
    let (Some(earlier), Some(rule)) = (rules.get(..index), rules.get(index)) else {
        return false;
    };
    earlier
        .iter()
        .any(|e| e.kind == rule.kind && rule.matcher.within(&e.matcher))
}

/// A display as a message names it: its connector, else its monitor's
/// serial, else its place among the displays.
fn display_name(id: &Identity, place: usize) -> Quoted {
    match (&id.connector, &id.edid) {
        (Some(connector), _) => Quoted::new(connector),
        (None, Some(edid)) => Quoted::new(&edid.serial),
        (None, None) => Quoted::new(&format!("display {}", place.saturating_add(1))),
    }
}

/// Whether `config` wires a complete route, judged by `devices`, the
/// devices present, or by the rules alone when `devices` is `None` or
/// holds no display or no keyboard (ADR-1403 D3). Placement is the
/// first-match walk without the guards, which act only while vtium runs.
pub fn complete_route(config: &Config, devices: Option<&[Identity]>) -> Route {
    let Some(devices) = devices else {
        return by_rules(config, Judged::RulesAlone);
    };
    let displays: Vec<&Identity> = devices.iter().filter(|d| d.kind == Kind::Display).collect();
    let keyboards: Vec<&Identity> = devices
        .iter()
        .filter(|d| d.kind == Kind::Input && d.counts_as_keyboard())
        .collect();
    if displays.is_empty() {
        return by_rules(config, Judged::NoDisplay);
    }
    if keyboards.is_empty() {
        return by_rules(config, Judged::NoKeyboard);
    }
    // each head a display present is sent to, with the first such display
    let mut shown: Vec<(usize, Quoted)> = Vec::new();
    for (i, d) in displays.iter().enumerate() {
        if let Place::Head(h) = config.walk(d).place
            && !shown.iter().any(|(s, _)| *s == h)
        {
            shown.push((h, display_name(d, i)));
        }
    }
    let route = |missing| Route {
        judged: Judged::Devices,
        missing,
    };
    if shown.is_empty() {
        let first = displays.first().map(|d| {
            let rule = match config.walk(d).by {
                crate::place::By::Rule(line) => number(config, line),
                _ => None,
            };
            Missing::NoDisplay {
                display: display_name(d, 0),
                rule,
            }
        });
        return route(first);
    }
    let mut reached = Vec::new();
    let mut roams = false;
    for k in &keyboards {
        match config.walk(k).place {
            Place::Head(h) => reached.push(h),
            Place::Roaming => roams = true,
            _ => {}
        }
    }
    let complete = shown
        .iter()
        .any(|(h, _)| reached.contains(h) || (roams && !config.isolated(*h)));
    if complete {
        return route(None);
    }
    let missing = shown.first().map(|(h, display)| {
        let head = Quoted::new(config.heads.get(*h).map_or("", |x| x.name.as_str()));
        if roams && config.isolated(*h) {
            Missing::Isolated { head }
        } else {
            Missing::NoKeyboard {
                head,
                display: display.clone(),
            }
        }
    });
    route(missing)
}

/// The complete route judged by the rules alone (ADR-1403 D3).
fn by_rules(config: &Config, judged: Judged) -> Route {
    let route = |missing| Route { judged, missing };
    // each head a display rule that some display can reach sends displays
    // to, with that rule's number
    let mut shown: Vec<(usize, usize)> = Vec::new();
    for (i, r) in config.rules.iter().enumerate() {
        if let (Kind::Display, Target::Head(h)) = (r.kind, r.target)
            && !shadowed(&config.rules, i)
            && !shown.iter().any(|(s, _)| *s == h)
        {
            shown.push((h, i.saturating_add(1)));
        }
    }
    let Some(&(first, rule)) = shown.first() else {
        return route(Some(Missing::NoDisplayRule));
    };
    // a keyboard of no particular make walks the rules as any keyboard
    // does, so `*`, `class:keyboard` and the policy are its; a rule naming
    // one device reaches that device
    let keyboard = Identity::input().with_class(Class::Keyboard);
    let mut reached = Vec::new();
    let mut roams = false;
    match config.walk(&keyboard).place {
        Place::Head(h) => reached.push(h),
        Place::Roaming => roams = true,
        _ => {}
    }
    for (i, r) in config.rules.iter().enumerate() {
        if r.kind == Kind::Input && names_one_device(&r.matcher) && !shadowed(&config.rules, i) {
            match r.target {
                Target::Head(h) => reached.push(h),
                Target::Roaming => roams = true,
                Target::Ignore | Target::Off => {}
            }
        }
    }
    let complete = shown
        .iter()
        .any(|(h, _)| reached.contains(h) || (roams && !config.isolated(*h)));
    if complete {
        return route(None);
    }
    let head = Quoted::new(config.heads.get(first).map_or("", |x| x.name.as_str()));
    route(Some(if roams && config.isolated(first) {
        Missing::Isolated { head }
    } else {
        Missing::NoKeyboardRule { head, rule }
    }))
}
