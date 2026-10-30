//! The run-time model: what each event while vtium runs does to a start outcome, explored to every state it can reach (ADR-1401 D5, D8, D9; a reload is vtium/vtium#86).
//!
//! One small machine stands for every file: heads `H1` and `H2`, and the default head `DEF` the display guard may light. Displays `D1` and `D2` are claimed by `H1` and `H2`, and `DU` by no rule. Keyboards `K1` and `K2` are claimed by `H1` and `H2`, `KU` by no rule, and `KI` sent to `ignore`. Pointer `P1` is claimed by `H1`, and `PU` by no rule. A partly broken file keeps `H1` alone, so `D2` and `K2` fall through to no rule; a file with no head keeps the `ignore` rule alone; the shipped rules send every display to `DEF` and every keyboard and pointer to the focus.
//!
//! A state is the devices present, the keyboards the rescue key has moved, where the focus rests and which configuration runs. The guards run after every event, as D9 says, so the anti-lockout guard firing and letting go and the display guard firing are outcomes of the events, never events of their own. The state space is finite, 2^9 device sets by 2^4 rescued sets by 4 focus places by 2 configurations, and the exploration runs breadth first until no event reaches a new state. Every invariant here is a property of one state, so a sequence of any length reaches only states the exploration checked.

use std::collections::{BTreeMap, BTreeSet};

use super::{Ways, paths, ways};
use crate::outcome::{
    Fallback, Landing, Outcome, Posture, Read, Rest, Runs, Stated, SwitchKey, Trusted, resolve,
};

const H1: u8 = 1;
const H2: u8 = 2;
const DEF: u8 = 4;
const ORDER: [u8; 3] = [H1, H2, DEF];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Display,
    Keyboard,
    Pointer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum To {
    Head(u8),
    Focus,
    Ignore,
    /// No rule matches it.
    None,
}

struct Device {
    name: &'static str,
    bit: u16,
    kind: Kind,
}

const DEVICES: [Device; 9] = [
    Device {
        name: "D1",
        bit: 1,
        kind: Kind::Display,
    },
    Device {
        name: "D2",
        bit: 2,
        kind: Kind::Display,
    },
    Device {
        name: "DU",
        bit: 4,
        kind: Kind::Display,
    },
    Device {
        name: "K1",
        bit: 8,
        kind: Kind::Keyboard,
    },
    Device {
        name: "K2",
        bit: 16,
        kind: Kind::Keyboard,
    },
    Device {
        name: "KU",
        bit: 32,
        kind: Kind::Keyboard,
    },
    Device {
        name: "KI",
        bit: 64,
        kind: Kind::Keyboard,
    },
    Device {
        name: "P1",
        bit: 128,
        kind: Kind::Pointer,
    },
    Device {
        name: "PU",
        bit: 256,
        kind: Kind::Pointer,
    },
];

const EVERY: u16 = 511;

/// Where each device's rule sends it, in [`DEVICES`]' order, and the heads the running text declares.
#[derive(Clone, Copy)]
struct Scene {
    heads: u8,
    to: [To; 9],
}

impl Scene {
    fn first(&self) -> Option<u8> {
        ORDER.into_iter().find(|&h| h != DEF && self.heads & h != 0)
    }

    fn to(&self, i: usize) -> To {
        self.to.get(i).copied().unwrap_or(To::None)
    }
}

fn scene(o: &Outcome, rest: Rest) -> Scene {
    use To::{Focus, Head, Ignore, None as Unmatched};
    let full = [
        Head(H1),
        Head(H2),
        Unmatched,
        Head(H1),
        Head(H2),
        Unmatched,
        Ignore,
        Head(H1),
        Unmatched,
    ];
    let partly = [
        Head(H1),
        Unmatched,
        Unmatched,
        Head(H1),
        Unmatched,
        Unmatched,
        Ignore,
        Head(H1),
        Unmatched,
    ];
    let headless = [
        Unmatched, Unmatched, Unmatched, Unmatched, Unmatched, Unmatched, Ignore, Unmatched,
        Unmatched,
    ];
    match o.runs {
        Runs::Builtin => Scene {
            heads: DEF,
            to: [
                Head(DEF),
                Head(DEF),
                Head(DEF),
                Focus,
                Focus,
                Focus,
                Focus,
                Focus,
                Focus,
            ],
        },
        Runs::Off => Scene {
            heads: 0,
            to: [Unmatched; 9],
        },
        Runs::File | Runs::Survivors => match rest {
            Rest::Valid => Scene {
                heads: H1 | H2,
                to: full,
            },
            Rest::Partly => Scene {
                heads: H1,
                to: partly,
            },
            Rest::Headless => Scene {
                heads: 0,
                to: headless,
            },
            Rest::Empty | Rest::Broken => Scene {
                heads: 0,
                to: [Unmatched; 9],
            },
        },
    }
}

/// What the running text writes of `key` on `head`, `None` for a guard.
fn written(ways: Ways, head: Option<u8>, key: SwitchKey) -> Option<bool> {
    let head_right = match key {
        SwitchKey::Focus | SwitchKey::VtKeys | SwitchKey::Rescue => true,
        SwitchKey::AntiLockout | SwitchKey::RescueKey | SwitchKey::NeverDark => false,
    };
    match ways {
        Ways::Unset => None,
        Ways::Granted => Some(true),
        Ways::Revoked => Some(false),
        Ways::Admin if head_right => (head == Some(H1)).then_some(true),
        Ways::Admin if key == SwitchKey::NeverDark => None,
        Ways::Admin => Some(true),
    }
}

/// A configuration running: its outcome, its rules and what its text writes.
#[derive(Clone, Copy)]
struct Config {
    o: Outcome,
    scene: Scene,
    ways: Ways,
}

impl Config {
    /// A switch in the run; the default head is the compiled-in `[head console]`, with every right on.
    fn on(&self, head: Option<u8>, key: SwitchKey) -> bool {
        head == Some(DEF) || self.o.switch(key).of(written(self.ways, head, key))
    }

    fn takes(&self, h: u8) -> bool {
        self.on(Some(h), SwitchKey::Focus)
            || (self.o.landing == Landing::FirstSurvivor && Some(h) == self.scene.first())
    }
}

fn bit(n: usize) -> u64 {
    u32::try_from(n)
        .ok()
        .and_then(|n| 1u64.checked_shl(n))
        .unwrap_or(0)
}

fn slot(h: u8) -> usize {
    ORDER.iter().position(|&x| x == h).unwrap_or(0)
}

/// An input delivering to a head.
fn delivers(i: usize, h: u8) -> u64 {
    bit(i
        .saturating_sub(3)
        .saturating_mul(3)
        .saturating_add(slot(h)))
}

/// A keyboard switching the VT.
fn switches(i: usize) -> u64 {
    bit(i.saturating_add(15))
}

/// A head's session.
fn session(h: u8) -> u64 {
    bit(slot(h).saturating_add(22))
}

/// A display showing a head.
fn shows(d: usize, h: u8) -> u64 {
    bit(d
        .saturating_mul(3)
        .saturating_add(slot(h))
        .saturating_add(25))
}

fn describe(bits: u64) -> String {
    let mut out = Vec::new();
    for (i, d) in DEVICES.iter().enumerate().skip(3) {
        for h in ORDER {
            if bits & delivers(i, h) != 0 {
                out.push(format!("{}->{}", d.name, head_name(h)));
            }
        }
        if d.kind == Kind::Keyboard && bits & switches(i) != 0 {
            out.push(format!("{}->VT", d.name));
        }
    }
    for h in ORDER {
        if bits & session(h) != 0 {
            out.push(format!("session {}", head_name(h)));
        }
        for (d, display) in DEVICES.iter().enumerate().take(3) {
            if bits & shows(d, h) != 0 {
                out.push(format!("{} shows {}", display.name, head_name(h)));
            }
        }
    }
    out.join(" ")
}

fn head_name(h: u8) -> &'static str {
    match h {
        H1 => "H1",
        H2 => "H2",
        _ => "DEF",
    }
}

/// A state once the guards have run.
#[derive(Clone, Copy)]
struct View {
    heads: u8,
    focus: Option<u8>,
    follows: u16,
    claims: [Option<u8>; 9],
    anti_lockout: bool,
    never_dark: bool,
    reach: u64,
}

/// Runs the placement and the guards over a state (ADR-1401 D5, D9).
fn settle(c: &Config, present: u16, rescued: u16, focus: Option<u8>) -> View {
    let off = c.o.runs == Runs::Off;
    let mut lit = 0u8;
    let mut shown = 0u64;
    let mut boot: Option<usize> = None;
    let mut boot_head = None;
    for (i, d) in DEVICES.iter().enumerate().take(3) {
        if present & d.bit == 0 {
            continue;
        }
        let first = boot.is_none();
        boot = boot.or(Some(i));
        if let To::Head(h) = c.scene.to(i) {
            lit |= h;
            shown |= shows(i, h);
            if first {
                boot_head = Some(h);
            }
        }
    }
    let mut never_dark = false;
    if let (0, Some(boot), true) = (lit, boot, c.on(None, SwitchKey::NeverDark)) {
        match c.o.fallback {
            Fallback::FirstHead => {
                let h = c.scene.first().unwrap_or(DEF);
                never_dark = true;
                lit = h;
                boot_head = Some(h);
                shown |= shows(boot, h);
            }
            // the boot display for the notice alone, and dark when the run shows none
            Fallback::NoticeOnly => never_dark = c.o.line.is_some(),
        }
    }
    let heads = c.scene.heads | (lit & DEF);
    let lit_taker = || {
        boot_head
            .filter(|&h| lit & h != 0 && c.takes(h))
            .or_else(|| ORDER.into_iter().find(|&h| lit & h != 0 && c.takes(h)))
    };
    let mut follows = 0u16;
    let mut claims = [None; 9];
    for ((i, d), claim) in DEVICES.iter().enumerate().zip(claims.iter_mut()) {
        if d.kind == Kind::Display || present & d.bit == 0 || off {
            continue;
        }
        if rescued & d.bit != 0 {
            follows |= d.bit;
            continue;
        }
        match c.scene.to(i) {
            To::Head(h) => *claim = Some(h),
            To::Focus => follows |= d.bit,
            To::Ignore => {}
            To::None if c.o.posture == Posture::Allow => follows |= d.bit,
            To::None => {}
        }
    }
    let mut focus = focus.filter(|&h| heads & h != 0 && c.takes(h));
    if focus.is_none() {
        focus = lit_taker();
    }
    let keyboards: Vec<usize> = DEVICES
        .iter()
        .enumerate()
        .filter(|(_, d)| d.kind == Kind::Keyboard && present & d.bit != 0)
        .map(|(i, _)| i)
        .collect();
    let reaches = keyboards.iter().any(|&i| {
        let bit = DEVICES.get(i).map_or(0, |d| d.bit);
        claims
            .get(i)
            .copied()
            .flatten()
            .is_some_and(|h| lit & h != 0)
            || (follows & bit != 0 && focus.is_some_and(|f| lit & f != 0))
    });
    let anti_lockout =
        !off && !keyboards.is_empty() && !reaches && c.on(None, SwitchKey::AntiLockout);
    if anti_lockout {
        for &i in &keyboards {
            follows |= DEVICES.get(i).map_or(0, |d| d.bit);
            if let Some(claim) = claims.get_mut(i) {
                *claim = None;
            }
        }
        focus = lit_taker();
    }
    let mut reach = shown;
    for ((i, d), claim) in DEVICES.iter().enumerate().zip(claims.iter()) {
        let to = claim.or(if follows & d.bit != 0 { focus } else { None });
        if let Some(h) = to {
            reach |= delivers(i, h);
            if d.kind == Kind::Keyboard && c.on(Some(h), SwitchKey::VtKeys) {
                reach |= switches(i);
            }
        }
    }
    for h in ORDER {
        if lit & h != 0 {
            reach |= session(h);
        }
    }
    View {
        heads,
        focus,
        follows,
        claims,
        anti_lockout,
        never_dark,
        reach,
    }
}

/// What a run may reach under `start`: by the file's text alone under deny (`by_text`), or by the start outcome's own policy.
fn allowed(start: &Config, by_text: bool) -> u64 {
    let o = &start.o;
    if o.runs == Runs::Off {
        return 0;
    }
    let on = |h: Option<u8>, k: SwitchKey| {
        if by_text {
            written(start.ways, h, k).unwrap_or(Posture::Deny.unset(k))
        } else {
            start.on(h, k)
        }
    };
    let posture = if by_text { Posture::Deny } else { o.posture };
    // the allow posture's display guard may light the default head; a deny file never grants it
    let heads = start.scene.heads | if posture == Posture::Allow { DEF } else { 0 };
    let takes = |h: u8| {
        h == DEF
            || on(Some(h), SwitchKey::Focus)
            || (!by_text && o.landing == Landing::FirstSurvivor && Some(h) == start.scene.first())
    };
    let mut out = 0u64;
    for i in 0..3 {
        if let To::Head(h) = start.scene.to(i) {
            out |= shows(i, h);
        }
        // the allow posture's display guard may give the boot display, any display present, to the first head or the default head
        if posture == Posture::Allow {
            out |= shows(i, start.scene.first().unwrap_or(DEF));
        }
    }
    for (i, d) in DEVICES.iter().enumerate().skip(3) {
        let keyboard = d.kind == Kind::Keyboard;
        let to = start.scene.to(i);
        let rescuable = match to {
            To::Head(h) => on(Some(h), SwitchKey::Rescue),
            To::Focus | To::Ignore | To::None => true,
        };
        let follows = to == To::Focus
            || (to == To::None && posture == Posture::Allow)
            || (keyboard && on(None, SwitchKey::AntiLockout))
            || (keyboard && on(None, SwitchKey::RescueKey) && rescuable);
        for h in ORDER {
            if heads & h == 0 || !(to == To::Head(h) || (follows && takes(h))) {
                continue;
            }
            out |= delivers(i, h);
            if keyboard && (h == DEF || on(Some(h), SwitchKey::VtKeys)) {
                out |= switches(i);
            }
        }
    }
    for h in ORDER {
        if heads & h != 0 {
            out |= session(h);
        }
    }
    out
}

#[derive(Clone, Copy, Debug)]
enum Event {
    /// A device added or removed, by its place in [`DEVICES`].
    Toggle(usize),
    /// The rescue key pressed on a keyboard.
    Rescue(usize),
    /// The focus chord pressed on a keyboard following the focus.
    Chord,
    /// A reload reads this file.
    Reload(Read),
}

const RELOADS: [Read; 6] = [
    Read::Missing,
    Read::Untrusted,
    Read::Inaccessible,
    Read::Trusted(Trusted {
        stated: Stated::Allow,
        rest: Rest::Broken,
    }),
    Read::Trusted(Trusted {
        stated: Stated::Deny,
        rest: Rest::Broken,
    }),
    Read::Trusted(Trusted {
        stated: Stated::Unset,
        rest: Rest::Partly,
    }),
];

fn events() -> Vec<Event> {
    let mut out: Vec<Event> = (0..DEVICES.len()).map(Event::Toggle).collect();
    out.extend((3..7).map(Event::Rescue));
    out.push(Event::Chord);
    out.extend(RELOADS.map(Event::Reload));
    out
}

fn event_kind(e: &Event) -> &'static str {
    match e {
        Event::Toggle(i) => match DEVICES.get(*i).map(|d| d.kind) {
            Some(Kind::Display) => "display lost or returned",
            Some(Kind::Keyboard) => "keyboard added or removed",
            Some(Kind::Pointer) | None => "pointer added or removed",
        },
        Event::Rescue(_) => "rescue key",
        Event::Chord => "focus chord",
        Event::Reload(_) => "reload",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct State {
    present: u16,
    rescued: u16,
    focus: Option<u8>,
    /// 0 for the start configuration, 1 for the shipped rules a reload put in its place.
    config: u8,
}

/// What an exploration found.
#[derive(Default)]
struct Found {
    states: usize,
    depth: usize,
    text_breaches: Vec<String>,
    policy_breaches: Vec<String>,
    /// Policy breaches in states where a reload put the shipped rules in place of an open start.
    reloaded_open: usize,
    anti_lockout: usize,
    let_go: usize,
    never_dark: usize,
    effective: BTreeMap<&'static str, usize>,
}

fn explore(read: &Read, ways: Ways, found: &mut Found) {
    let o = resolve(read);
    let rest = match read {
        Read::Trusted(t) => t.rest,
        Read::Missing | Read::Untrusted | Read::Inaccessible => Rest::Empty,
    };
    let readable_deny = matches!(
        read,
        Read::Trusted(Trusted {
            stated: Stated::Deny,
            ..
        })
    );
    let start = Config {
        o,
        scene: scene(&o, rest),
        ways,
    };
    let shipped = resolve(&Read::Missing);
    let configs = [
        start,
        Config {
            o: shipped,
            scene: scene(&shipped, Rest::Empty),
            ways,
        },
    ];
    let by_text = allowed(&start, true);
    let by_policy = allowed(&start, false);
    let events = events();
    let first = settle(&start, EVERY, 0, None);
    let mut seen = BTreeSet::new();
    let mut queue = std::collections::VecDeque::new();
    let s0 = State {
        present: EVERY,
        rescued: 0,
        focus: first.focus,
        config: 0,
    };
    seen.insert(s0);
    queue.push_back((s0, 0usize));
    while let Some((s, depth)) = queue.pop_front() {
        found.depth = found.depth.max(depth);
        let c = configs.get(usize::from(s.config)).unwrap_or(&start);
        let v = settle(c, s.present, s.rescued, s.focus);
        found.states = found.states.saturating_add(1);
        found.anti_lockout = found
            .anti_lockout
            .saturating_add(usize::from(v.anti_lockout));
        found.never_dark = found.never_dark.saturating_add(usize::from(v.never_dark));
        let at = || format!("{read:?} {ways:?} {s:?}");
        if readable_deny && v.reach & !by_text != 0 {
            found
                .text_breaches
                .push(format!("{}: {}", at(), describe(v.reach & !by_text)));
        }
        if v.reach & !by_policy != 0 {
            if s.config == 1 && o.posture == Posture::Allow {
                found.reloaded_open = found.reloaded_open.saturating_add(1);
            } else {
                found
                    .policy_breaches
                    .push(format!("{}: {}", at(), describe(v.reach & !by_policy)));
            }
        }
        for e in &events {
            let mut next = s;
            match *e {
                Event::Toggle(i) => next.present ^= DEVICES.get(i).map_or(0, |d| d.bit),
                Event::Rescue(i) => {
                    let bit = DEVICES.get(i).map_or(0, |d| d.bit);
                    let rescue = match v.claims.get(i).copied().flatten() {
                        Some(h) => c.on(Some(h), SwitchKey::Rescue),
                        None => true,
                    };
                    if s.present & bit != 0
                        && c.o.runs != Runs::Off
                        && c.on(None, SwitchKey::RescueKey)
                        && rescue
                    {
                        next.rescued |= bit;
                    }
                }
                Event::Chord => {
                    let typed = DEVICES
                        .iter()
                        .any(|d| d.kind == Kind::Keyboard && v.follows & d.bit != 0);
                    if let (true, Some(f)) = (typed, v.focus) {
                        let after = ORDER.iter().position(|&h| h == f).unwrap_or(0);
                        next.focus = ORDER
                            .iter()
                            .cycle()
                            .skip(after.saturating_add(1))
                            .take(ORDER.len())
                            .copied()
                            .find(|&h| v.heads & h != 0 && c.takes(h));
                    }
                }
                Event::Reload(r) => {
                    let new = c.o.reload(&r);
                    if new != c.o && new.runs == Runs::Builtin {
                        next.config = 1;
                    } else if new != c.o {
                        found
                            .policy_breaches
                            .push(format!("{}: a reload of {r:?} runs {new:?}", at()));
                    }
                }
            }
            let c2 = configs.get(usize::from(next.config)).unwrap_or(&start);
            let settled = settle(c2, next.present, next.rescued, next.focus);
            next.focus = settled.focus;
            if next != s {
                let n = found.effective.entry(event_kind(e)).or_default();
                *n = n.saturating_add(1);
            }
            if v.anti_lockout && !settled.anti_lockout {
                found.let_go = found.let_go.saturating_add(1);
            }
            if seen.insert(next) {
                queue.push_back((next, depth.saturating_add(1)));
            }
        }
    }
}

/// Every start outcome with every extreme of its text, explored once for every test here.
fn explore_all() -> &'static Found {
    static FOUND: std::sync::OnceLock<Found> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        let mut found = Found::default();
        for read in paths() {
            for &w in ways(&read) {
                explore(&read, w, &mut found);
            }
        }
        found
    })
}

/// The first breach of each start, so a failure names every start it reaches.
fn firsts(breaches: &[String]) -> Vec<&String> {
    let mut starts = BTreeSet::new();
    breaches
        .iter()
        .filter(|b| starts.insert(b.split(" State").next().unwrap_or_default()))
        .collect()
}

#[test]
fn no_event_opens_what_a_readable_deny_did_not_grant() {
    let found = explore_all();
    assert!(
        found.text_breaches.is_empty(),
        "{} states reach what the deny file did not grant: {:#?}",
        found.text_breaches.len(),
        firsts(&found.text_breaches)
    );
}

#[test]
fn a_valid_deny_file_that_declares_nothing_is_dark_and_silent() {
    let read = Read::Trusted(Trusted {
        stated: Stated::Deny,
        rest: Rest::Empty,
    });
    let o = resolve(&read);
    let c = Config {
        o,
        scene: scene(&o, Rest::Empty),
        ways: Ways::Unset,
    };
    let v = settle(&c, EVERY, 0, None);
    assert_eq!(o.line, None);
    assert!(v.reach == 0 && !v.never_dark, "{}", describe(v.reach));
}

#[test]
fn no_event_opens_past_its_start_outcome() {
    let found = explore_all();
    assert!(
        found.policy_breaches.is_empty(),
        "{} states reach past their start outcome: {:#?}",
        found.policy_breaches.len(),
        firsts(&found.policy_breaches)
    );
    // contradiction: a reload that finds the file gone puts the shipped rules in place of a running open file, which may give an input the focus the file closed
    assert!(found.reloaded_open > 0);
}

#[test]
fn the_exploration_reaches_every_event_and_guard() {
    let found = explore_all();
    for kind in [
        "display lost or returned",
        "keyboard added or removed",
        "pointer added or removed",
        "rescue key",
        "focus chord",
        "reload",
    ] {
        assert!(
            found.effective.get(kind).is_some_and(|&n| n > 0),
            "{kind} never changes a state"
        );
    }
    assert!(found.anti_lockout > 0 && found.let_go > 0 && found.never_dark > 0);
    println!(
        "{} states, depth {}, anti-lockout firing in {}, letting go {} times, never-dark firing in {}, effective events {:?}, open reloads to the shipped rules {}",
        found.states,
        found.depth,
        found.anti_lockout,
        found.let_go,
        found.never_dark,
        found.effective,
        found.reloaded_open
    );
}
