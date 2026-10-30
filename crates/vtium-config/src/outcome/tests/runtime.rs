//! The run-time model: what each event while vtium runs does to a start outcome, explored to every state it can reach (ADR-1401 D5, D8, D9; a reload is vtium/vtium#86).
//!
//! One small machine stands for every file: heads `H1` and `H2`, and the default head `DEF` the display guard may light. Displays `D1` and `D2` are claimed by `H1` and `H2`, and `DU` by no rule. Keyboards `K1` and `K2` are claimed by `H1` and `H2`, `KU` by no rule, and `KI` sent to `ignore`. Pointer `P1` is claimed by `H1`, and `PU` by no rule. A partly broken file keeps `H1` alone, so `D2` and `K2` fall through to no rule; a file with no head keeps the `ignore` rule alone; the shipped rules send every display to `DEF` and every keyboard and pointer roaming.
//!
//! Each head the running text declares is isolated or not, as the text writes it or the policy leaves it, and every such assignment is a start of its own; the operator's own way in under a closed policy is a head written `isolated = no`.
//!
//! A state is the devices present, the keyboards the rescue key has moved and which head is the active head; a reload changes the configuration only for a valid trusted file, which the events here never read. The guards run after every event, as D9 says, so the anti-lockout guard firing and letting go and the display guard firing are outcomes of the events, never events of their own. The state space is finite, 2^9 device sets by 2^4 rescued sets by 4 places for the active head, and the exploration runs breadth first until no event reaches a new state. Every invariant here is a property of one state, so a sequence of any length reaches only states the exploration checked.

use std::collections::{BTreeMap, BTreeSet};

use super::paths;
use crate::outcome::{
    Fallback, Landing, Outcome, Read, Rescue, Rest, Runs, Stated, SwitchKey, Trusted, Unplaced,
    resolve,
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
    Roaming,
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
    use To::{Head, Ignore, None as Unmatched, Roaming};
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
                Roaming,
                Roaming,
                Roaming,
                Roaming,
                Roaming,
                Roaming,
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

/// What the running text writes of each head's `isolated`: `H1`'s, then `H2`'s, `None` for unset.
type Isolation = (Option<bool>, Option<bool>);

/// Every assignment of `isolated` to the heads `scene` declares.
fn isolations(scene: &Scene) -> Vec<Isolation> {
    let each = [None, Some(false), Some(true)];
    let h1: &[Option<bool>] = if scene.heads & H1 != 0 {
        &each
    } else {
        &[None]
    };
    let h2: &[Option<bool>] = if scene.heads & H2 != 0 {
        &each
    } else {
        &[None]
    };
    h1.iter()
        .flat_map(|&a| h2.iter().map(move |&b| (a, b)))
        .collect()
}

fn written(iso: Isolation, h: u8) -> Option<bool> {
    match h {
        H1 => iso.0,
        H2 => iso.1,
        _ => None,
    }
}

/// A configuration running: its outcome, its rules and what its text writes.
#[derive(Clone, Copy)]
struct Config {
    o: Outcome,
    scene: Scene,
    iso: Isolation,
}

impl Config {
    /// Whether `h` is isolated; the default head is the compiled-in `[head console]`, never isolated.
    fn isolated(&self, h: u8) -> bool {
        h != DEF && self.o.switch(SwitchKey::Isolated).of(written(self.iso, h))
    }

    fn guard(&self, key: SwitchKey) -> bool {
        self.o.switch(key).of(None)
    }

    /// Whether `h` may be the active head while the heads `lit` are lit.
    fn takes(&self, h: u8, lit: u8) -> bool {
        let first_lit = ORDER
            .into_iter()
            .find(|&x| x != DEF && self.scene.heads & x != 0 && lit & x != 0);
        !self.isolated(h) || (self.o.landing == Landing::FirstLitSurvivor && Some(h) == first_lit)
    }

    /// Whether the rescue key reaches a keyboard its rule gives to `h`.
    fn rescuable(&self, h: u8) -> bool {
        !self.isolated(h) || self.o.rescue == Rescue::Forced
    }

    /// Whether an input of `kind` that no running rule places roams.
    fn unplaced_roams(&self, kind: Kind) -> bool {
        match self.o.unplaced {
            Unplaced::Roaming => true,
            // the shipped rules' tail: the keyboard, pointer and key classes, every input here
            Unplaced::Shipped => matches!(kind, Kind::Keyboard | Kind::Pointer),
            Unplaced::Ignore => false,
        }
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

fn head_name(h: u8) -> &'static str {
    match h {
        H1 => "H1",
        H2 => "H2",
        _ => "DEF",
    }
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

/// A state once the guards have run.
#[derive(Clone, Copy)]
struct View {
    heads: u8,
    lit: u8,
    active: Option<u8>,
    roams: u16,
    claims: [Option<u8>; 9],
    anti_lockout: bool,
    display_guard: bool,
    reach: u64,
}

/// Runs the placement and the guards over a state (ADR-1401 D5, D9).
fn settle(c: &Config, present: u16, rescued: u16, active: Option<u8>) -> View {
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
    // the display guard is always on
    let mut display_guard = false;
    if let (0, Some(boot)) = (lit, boot) {
        match c.o.fallback {
            Fallback::FirstHead => {
                let h = c.scene.first().unwrap_or(DEF);
                display_guard = true;
                lit = h;
                boot_head = Some(h);
                shown |= shows(boot, h);
            }
            // the boot display for the notice alone, and dark for a valid closed file with no head
            Fallback::NoticeOnly => display_guard = c.o.line.is_some() || c.scene.heads != 0,
        }
    }
    let heads = c.scene.heads | (lit & DEF);
    let lit_taker = || {
        boot_head
            .filter(|&h| lit & h != 0 && c.takes(h, lit))
            .or_else(|| ORDER.into_iter().find(|&h| lit & h != 0 && c.takes(h, lit)))
    };
    let mut roams = 0u16;
    let mut claims = [None; 9];
    for ((i, d), claim) in DEVICES.iter().enumerate().zip(claims.iter_mut()) {
        if d.kind == Kind::Display || present & d.bit == 0 || off {
            continue;
        }
        if rescued & d.bit != 0 {
            roams |= d.bit;
            continue;
        }
        match c.scene.to(i) {
            To::Head(h) => *claim = Some(h),
            To::Roaming => roams |= d.bit,
            To::Ignore => {}
            To::None if c.unplaced_roams(d.kind) => roams |= d.bit,
            To::None => {}
        }
    }
    let mut active = active.filter(|&h| heads & h != 0 && c.takes(h, lit));
    if active.is_none() {
        active = lit_taker();
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
            || (roams & bit != 0 && active.is_some_and(|f| lit & f != 0))
    });
    let anti_lockout = !off && !keyboards.is_empty() && !reaches && c.guard(SwitchKey::AntiLockout);
    if anti_lockout {
        for &i in &keyboards {
            roams |= DEVICES.get(i).map_or(0, |d| d.bit);
            if let Some(claim) = claims.get_mut(i) {
                *claim = None;
            }
        }
        active = lit_taker();
    }
    let mut reach = shown;
    for ((i, d), claim) in DEVICES.iter().enumerate().zip(claims.iter()) {
        let to = claim.or(if roams & d.bit != 0 { active } else { None });
        if let Some(h) = to {
            reach |= delivers(i, h);
            // an isolated head's VT keys go to its session
            if d.kind == Kind::Keyboard && !c.isolated(h) {
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
        lit,
        active,
        roams,
        claims,
        anti_lockout,
        display_guard,
        reach,
    }
}

/// What a run may reach under `start`: by a closed file's text alone (`by_text`), or by the start outcome's own policy.
fn allowed(start: &Config, by_text: bool) -> u64 {
    let o = &start.o;
    if o.runs == Runs::Off {
        return 0;
    }
    let isolated = |h: u8| {
        if by_text {
            written(start.iso, h).unwrap_or(true)
        } else {
            start.isolated(h)
        }
    };
    let guard = |k: SwitchKey| !by_text && start.guard(k);
    let open = !by_text && o.policy == crate::outcome::Policy::Open;
    // the open policy's display guard may light the default head; a closed file never grants it
    let heads = start.scene.heads | if open { DEF } else { 0 };
    let takes = |h: u8| {
        h == DEF
            || !isolated(h)
            || (!by_text && o.landing == Landing::FirstLitSurvivor && start.scene.heads & h != 0)
    };
    let mut out = 0u64;
    for i in 0..3 {
        if let To::Head(h) = start.scene.to(i) {
            out |= shows(i, h);
        }
        // the open policy's display guard may give the boot display, any display present, to the first head or the default head
        if open {
            out |= shows(i, start.scene.first().unwrap_or(DEF));
        }
    }
    for (i, d) in DEVICES.iter().enumerate().skip(3) {
        let keyboard = d.kind == Kind::Keyboard;
        let to = start.scene.to(i);
        let rescuable = match to {
            To::Head(h) => !isolated(h) || (!by_text && o.rescue == Rescue::Forced),
            To::Roaming | To::Ignore | To::None => true,
        };
        let roams = to == To::Roaming
            || (to == To::None && !by_text && start.unplaced_roams(d.kind))
            || (keyboard && guard(SwitchKey::AntiLockout))
            || (keyboard && guard(SwitchKey::RescueKey) && rescuable);
        for h in ORDER {
            if heads & h == 0 || !(to == To::Head(h) || (roams && takes(h))) {
                continue;
            }
            out |= delivers(i, h);
            if keyboard && (h == DEF || !isolated(h)) {
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
    /// The chord that moves the active head, pressed on a roaming keyboard.
    Chord,
    /// A reload reads this file.
    Reload(Read),
}

const RELOADS: [Read; 6] = [
    Read::Missing,
    Read::Untrusted,
    Read::Inaccessible,
    Read::Trusted(Trusted {
        stated: Stated::Open,
        rest: Rest::Broken,
    }),
    Read::Trusted(Trusted {
        stated: Stated::Closed,
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
        Event::Chord => "active-head chord",
        Event::Reload(_) => "reload",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct State {
    present: u16,
    rescued: u16,
    active: Option<u8>,
}

/// What an exploration found.
#[derive(Default)]
struct Found {
    starts: usize,
    states: usize,
    depth: usize,
    text_breaches: Vec<String>,
    policy_breaches: Vec<String>,
    anti_lockout: usize,
    let_go: usize,
    display_guard: usize,
    effective: BTreeMap<&'static str, usize>,
}

fn explore(read: &Read, iso: Isolation, found: &mut Found) {
    let o = resolve(read);
    let rest = match read {
        Read::Trusted(t) => t.rest,
        Read::Missing | Read::Untrusted | Read::Inaccessible => Rest::Empty,
    };
    let readable_closed = matches!(
        read,
        Read::Trusted(Trusted {
            stated: Stated::Closed,
            ..
        })
    );
    let c = Config {
        o,
        scene: scene(&o, rest),
        iso,
    };
    let by_text = allowed(&c, true);
    let by_policy = allowed(&c, false);
    let events = events();
    let first = settle(&c, EVERY, 0, None);
    let mut seen = BTreeSet::new();
    let mut queue = std::collections::VecDeque::new();
    let s0 = State {
        present: EVERY,
        rescued: 0,
        active: first.active,
    };
    seen.insert(s0);
    queue.push_back((s0, 0usize));
    found.starts = found.starts.saturating_add(1);
    while let Some((s, depth)) = queue.pop_front() {
        found.depth = found.depth.max(depth);
        let v = settle(&c, s.present, s.rescued, s.active);
        found.states = found.states.saturating_add(1);
        found.anti_lockout = found
            .anti_lockout
            .saturating_add(usize::from(v.anti_lockout));
        found.display_guard = found
            .display_guard
            .saturating_add(usize::from(v.display_guard));
        let at = || format!("{read:?} {iso:?} {s:?}");
        if readable_closed && v.reach & !by_text != 0 {
            found
                .text_breaches
                .push(format!("{}: {}", at(), describe(v.reach & !by_text)));
        }
        if v.reach & !by_policy != 0 {
            found
                .policy_breaches
                .push(format!("{}: {}", at(), describe(v.reach & !by_policy)));
        }
        for e in &events {
            let mut next = s;
            match *e {
                Event::Toggle(i) => next.present ^= DEVICES.get(i).map_or(0, |d| d.bit),
                Event::Rescue(i) => {
                    let bit = DEVICES.get(i).map_or(0, |d| d.bit);
                    let reached = v
                        .claims
                        .get(i)
                        .copied()
                        .flatten()
                        .is_none_or(|h| c.rescuable(h));
                    if s.present & bit != 0 && c.guard(SwitchKey::RescueKey) && reached {
                        next.rescued |= bit;
                    }
                }
                Event::Chord => {
                    let typed = DEVICES
                        .iter()
                        .any(|d| d.kind == Kind::Keyboard && v.roams & d.bit != 0);
                    if let (true, Some(f)) = (typed, v.active) {
                        let after = ORDER.iter().position(|&h| h == f).unwrap_or(0);
                        next.active = ORDER
                            .iter()
                            .cycle()
                            .skip(after.saturating_add(1))
                            .take(ORDER.len())
                            .copied()
                            .find(|&h| v.heads & h != 0 && c.takes(h, v.lit));
                    }
                }
                // only a valid trusted file is applied, and these are none
                Event::Reload(r) => {
                    let new = c.o.reload(&r);
                    if new != c.o {
                        let breach = format!("{}: a reload of {r:?} runs {new:?}", at());
                        if readable_closed {
                            found.text_breaches.push(breach.clone());
                        }
                        found.policy_breaches.push(breach);
                    }
                }
            }
            let settled = settle(&c, next.present, next.rescued, next.active);
            next.active = settled.active;
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

/// Every start outcome with every assignment of its heads' isolation, explored once for every test here.
fn explore_all() -> &'static Found {
    static FOUND: std::sync::OnceLock<Found> = std::sync::OnceLock::new();
    FOUND.get_or_init(|| {
        let mut found = Found::default();
        for read in paths() {
            let o = resolve(&read);
            let rest = match read {
                Read::Trusted(t) => t.rest,
                Read::Missing | Read::Untrusted | Read::Inaccessible => Rest::Empty,
            };
            for iso in isolations(&scene(&o, rest)) {
                explore(&read, iso, &mut found);
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
fn no_event_opens_what_a_readable_closed_policy_did_not_grant() {
    let found = explore_all();
    assert!(
        found.text_breaches.is_empty(),
        "{} states reach what the closed file did not grant: {:#?}",
        found.text_breaches.len(),
        firsts(&found.text_breaches)
    );
}

#[test]
fn a_valid_closed_file_that_declares_nothing_is_dark_and_silent() {
    let read = Read::Trusted(Trusted {
        stated: Stated::Closed,
        rest: Rest::Empty,
    });
    let o = resolve(&read);
    let c = Config {
        o,
        scene: scene(&o, Rest::Empty),
        iso: (None, None),
    };
    let v = settle(&c, EVERY, 0, None);
    assert_eq!(o.line, None);
    assert!(v.reach == 0 && !v.display_guard, "{}", describe(v.reach));
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
}

#[test]
fn the_exploration_reaches_every_event_and_guard() {
    let found = explore_all();
    for kind in [
        "display lost or returned",
        "keyboard added or removed",
        "pointer added or removed",
        "rescue key",
        "active-head chord",
    ] {
        assert!(
            found.effective.get(kind).is_some_and(|&n| n > 0),
            "{kind} never changes a state"
        );
    }
    assert!(found.anti_lockout > 0 && found.let_go > 0 && found.display_guard > 0);
    // a reload of a file vtium cannot run whole changes nothing
    assert!(!found.effective.contains_key("reload"));
    println!(
        "{} starts, {} states, depth {}, anti-lockout firing in {}, letting go {} times, display guard firing in {}, effective events {:?}",
        found.starts,
        found.states,
        found.depth,
        found.anti_lockout,
        found.let_go,
        found.display_guard,
        found.effective
    );
}
