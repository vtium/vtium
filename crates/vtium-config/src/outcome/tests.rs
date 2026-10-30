//! Every path through [`resolve`], its table, and the invariants over all of them (ADR-1401 D7).
//!
//! A path's outcome leaves a head's `isolated`, the one switch the running text writes, open, so each invariant is checked under the extremes of what that text can write ([`Ways`]): nothing, every head shared, every head isolated. Each check is monotone in each head's isolation, so a text that isolates some heads lies between the extremes. Keyboards are present on every path, and a rule may strand every one of them, so a keyboard reaches a lit head only where the shipped rules run or the anti-lockout guard or the rescue key gives it a way.

use std::collections::BTreeSet;

use super::*;
use crate::config::Config;
use crate::error::{Quoted, Untrusted};
use crate::identity::{Class, Identity};
use crate::parse::parse;
use crate::place::Place;

mod runtime;

/// Every path: the three verdicts that read nothing, and each policy with each rest of a trusted file.
fn paths() -> Vec<Read> {
    let seeds = [
        Read::Missing,
        Read::Untrusted,
        Read::Inaccessible,
        Read::Trusted(Trusted {
            stated: Stated::Unset,
            rest: Rest::Valid,
        }),
    ];
    let mut out = Vec::new();
    for seed in seeds {
        match seed {
            Read::Missing | Read::Untrusted | Read::Inaccessible => out.push(seed),
            Read::Trusted(_) => {
                for &stated in Stated::ALL {
                    for &rest in Rest::ALL {
                        out.push(Read::Trusted(Trusted { stated, rest }));
                    }
                }
            }
        }
    }
    out
}

/// What the running text writes of its heads' `isolated`, at its extremes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ways {
    /// Nothing: each head takes the policy's value.
    Unset,
    /// Every head `isolated = no`, as an operator's own head under closed writes it.
    Shared,
    /// Every head `isolated = yes`.
    Isolated,
}

impl Ways {
    fn name(self) -> &'static str {
        match self {
            Ways::Unset => "unset",
            Ways::Shared => "shared",
            Ways::Isolated => "isolated",
        }
    }
}

/// The extremes a path's text can write: none where no head of the file runs.
fn ways(read: &Read) -> &'static [Ways] {
    match read {
        Read::Trusted(Trusted {
            rest: Rest::Valid | Rest::Partly,
            ..
        }) => &[Ways::Unset, Ways::Shared, Ways::Isolated],
        Read::Trusted(Trusted {
            rest: Rest::Headless | Rest::Empty | Rest::Broken,
            ..
        })
        | Read::Missing
        | Read::Untrusted
        | Read::Inaccessible => &[Ways::Unset],
    }
}

fn written(ways: Ways) -> Option<bool> {
    match ways {
        Ways::Unset => None,
        Ways::Shared => Some(false),
        Ways::Isolated => Some(true),
    }
}

/// Whether `key`'s value in `o` opens a way: a guard on, or a head not isolated.
fn opens(o: &Outcome, key: SwitchKey, written: Option<bool>) -> bool {
    let value = o.switch(key).of(written);
    match key {
        SwitchKey::AntiLockout | SwitchKey::RescueKey => value,
        SwitchKey::Isolated => !value,
    }
}

/// Whether `key` opens a way under `policy` where the file writes nothing.
fn grants(policy: Policy, key: SwitchKey) -> bool {
    match key {
        SwitchKey::AntiLockout | SwitchKey::RescueKey => policy.unset(key),
        SwitchKey::Isolated => !policy.unset(key),
    }
}

/// One path with one extreme of its text.
struct Run {
    read: Read,
    ways: Ways,
    outcome: Outcome,
}

impl Run {
    fn on(&self, key: SwitchKey) -> bool {
        self.outcome.switch(key).of(written(self.ways))
    }

    /// Whether a head's display is lit whatever the rules place: by the shipped rules or the display guard, which under closed lights no head of its own.
    fn lit(&self) -> bool {
        match (self.outcome.lit, self.outcome.fallback) {
            (Lit::Rules, Fallback::FirstHead) => true,
            (Lit::Rules, Fallback::NoticeOnly) | (Lit::Notice, _) => false,
        }
    }

    /// Whether the console line has a lit display to show on: the display guard lights the first head under open, and the boot display for the notice of a failed run under closed.
    fn shown(&self) -> bool {
        match (self.outcome.lit, self.outcome.fallback) {
            (Lit::Rules, Fallback::FirstHead) | (Lit::Notice, _) => true,
            (Lit::Rules, Fallback::NoticeOnly) => self.outcome.line.is_some(),
        }
    }

    /// Whether some head may be the active head.
    fn lands(&self) -> bool {
        !self.on(SwitchKey::Isolated)
    }

    /// Whether a keyboard the rescue key moves lands on a lit head.
    fn rescued(&self) -> bool {
        self.lit() && self.lands() && self.on(SwitchKey::RescueKey)
    }

    /// Whether some keyboard reaches a lit head however the rules place the keyboards.
    fn reaches(&self) -> bool {
        let shipped = match self.outcome.runs {
            Runs::Builtin => true,
            Runs::File | Runs::Survivors | Runs::Off => false,
        };
        let guard = self.lit() && self.lands() && self.on(SwitchKey::AntiLockout);
        (shipped && self.lit()) || guard || self.rescued()
    }

    /// Whether the file's own valid text runs and isolates every head: what "anything written explicitly wins" lets a file do.
    fn isolates_as_written(&self) -> bool {
        self.ways == Ways::Isolated && self.outcome.runs == Runs::File
    }
}

fn runs() -> Vec<Run> {
    paths()
        .into_iter()
        .flat_map(|read| {
            ways(&read).iter().map(move |&ways| Run {
                read,
                ways,
                outcome: resolve(&read),
            })
        })
        .collect()
}

/// A trusted file with errors: partly broken while a usable head survives, broken once none does. The `policy` line vtium cannot read is one of the errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Degraded {
    Partly,
    Broken,
}

fn degraded(read: &Read) -> Option<Degraded> {
    let Read::Trusted(t) = read else { return None };
    match (t.stated, t.rest) {
        (Stated::Unreadable, Rest::Valid) | (_, Rest::Partly) => Some(Degraded::Partly),
        (Stated::Unreadable, Rest::Headless | Rest::Empty) | (_, Rest::Broken) => {
            Some(Degraded::Broken)
        }
        (
            Stated::Unset | Stated::Open | Stated::Closed,
            Rest::Valid | Rest::Headless | Rest::Empty,
        ) => None,
    }
}

/// The policy a trusted file gives: closed as written, else open.
fn disposition(read: &Read) -> Option<Policy> {
    let Read::Trusted(t) = read else { return None };
    Some(match t.stated {
        Stated::Closed => Policy::Closed,
        Stated::Unset | Stated::Open | Stated::Unreadable => Policy::Open,
    })
}

/// How far an input no rule names in particular reaches: the shipped rules let the keyboard, pointer and key classes roam and ignore the rest, while an open run lets every input roam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Inputs {
    Ignored,
    Shipped,
    Roaming,
}

fn inputs(o: &Outcome) -> Inputs {
    match o.unplaced {
        Unplaced::Roaming => Inputs::Roaming,
        Unplaced::Shipped => Inputs::Shipped,
        Unplaced::Ignore => Inputs::Ignored,
    }
}

fn granted_inputs(policy: Policy) -> Inputs {
    match policy {
        Policy::Open => Inputs::Roaming,
        Policy::Closed => Inputs::Ignored,
    }
}

fn read_name(read: &Read) -> [&'static str; 3] {
    match read {
        Read::Missing => ["missing", "-", "-"],
        Read::Untrusted => ["untrusted", "-", "-"],
        Read::Inaccessible => ["inaccessible", "-", "-"],
        Read::Trusted(t) => ["trusted", t.stated.name(), t.rest.name()],
    }
}

const TABLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/outcomes.tsv");

/// The outcome table: one row a path, its inputs, its outcome, each switch's rule, and the extremes of its text under which a lockout can occur.
fn table() -> String {
    let mut out = String::from(
        "read\tpolicy\trest\truns\teffective\trescue\tline\tlit\tfallback\tdark-notice\tunplaced",
    );
    for key in SwitchKey::ALL {
        out.push('\t');
        out.push_str(key.name());
    }
    out.push_str("\tlockout\n");
    for read in paths() {
        let o = resolve(&read);
        let mut row: Vec<&str> = read_name(&read).to_vec();
        row.extend([
            o.runs.name(),
            o.policy.name(),
            o.rescue.name(),
            o.line.map_or("-", Line::text),
            o.lit.name(),
            o.fallback.name(),
            o.dark_notice().map_or("-", Line::text),
            o.unplaced.name(),
        ]);
        row.extend(SwitchKey::ALL.iter().map(|&k| o.switch(k).name()));
        let locked: Vec<&str> = ways(&read)
            .iter()
            .map(|&ways| Run {
                read,
                ways,
                outcome: o,
            })
            .filter(|r| !r.reaches())
            .map(|r| r.ways.name())
            .collect();
        let locked = if locked.is_empty() {
            "-".to_owned()
        } else {
            locked.join(",")
        };
        row.push(&locked);
        out.push_str(&row.join("\t"));
        out.push('\n');
    }
    out
}

#[test]
fn outcome_table_matches_resolve() {
    let want = table();
    if std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
        std::fs::write(TABLE, &want).unwrap_or_default();
        return;
    }
    let committed = std::fs::read_to_string(TABLE).unwrap_or_default();
    assert_eq!(
        committed, want,
        "outcomes.tsv differs from resolve; VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config outcome_table rewrites it"
    );
}

#[test]
fn every_path_yields_exactly_one_outcome() {
    let paths = paths();
    let distinct: BTreeSet<Read> = paths.iter().copied().collect();
    let trusted = Stated::ALL.len().saturating_mul(Rest::ALL.len());
    assert_eq!(paths.len(), trusted.saturating_add(3));
    assert_eq!(distinct.len(), paths.len(), "a path is listed twice");
    for read in &paths {
        assert_eq!(resolve(read), resolve(read), "{read:?} answers twice");
    }
}

#[test]
fn an_untrusted_files_content_is_never_consulted() {
    // `Read::Untrusted` and `Read::Inaccessible` carry nothing, which the module's compile_fail example proves: the outcome is a constant of the verdict
    let skipped = resolve(&Read::Untrusted);
    assert_eq!(resolve(&Read::Inaccessible), skipped);
    assert_eq!(skipped, builtin(Some(Line::Skipped)));
}

#[test]
fn missing_and_valid_paths_are_silent_and_every_other_shows_one_line_on_a_lit_display() {
    for run in runs() {
        let silent = match run.read {
            Read::Missing => true,
            Read::Untrusted | Read::Inaccessible => false,
            Read::Trusted(t) => match (t.stated, t.rest) {
                (
                    Stated::Unset | Stated::Open | Stated::Closed,
                    Rest::Valid | Rest::Headless | Rest::Empty,
                ) => true,
                (Stated::Unreadable, _) | (_, Rest::Partly | Rest::Broken) => false,
            },
        };
        match run.outcome.line {
            None => assert!(silent, "{:?} shows no line", run.read),
            Some(line) => {
                assert!(!silent, "{:?} shows {line:?}", run.read);
                assert!(
                    run.shown(),
                    "{:?} under {:?} shows {line:?} on no lit display",
                    run.read,
                    run.ways
                );
            }
        }
    }
}

#[test]
fn no_open_outcome_locks_out() {
    let mut explicit = BTreeSet::new();
    for run in runs() {
        if run.outcome.policy != Policy::Open || run.reaches() {
            continue;
        }
        assert!(
            run.isolates_as_written(),
            "{:?} under {:?} is open and locks out",
            run.read,
            run.ways
        );
        explicit.insert(run.read);
    }
    // contradiction: a valid open file that isolates every head can lock out, since anything written explicitly wins
    let want: BTreeSet<Read> = [Stated::Unset, Stated::Open]
        .into_iter()
        .map(|stated| {
            Read::Trusted(Trusted {
                stated,
                rest: Rest::Valid,
            })
        })
        .collect();
    assert_eq!(explicit, want);
}

#[test]
fn a_lockout_needs_a_trusted_file_that_wrote_closed() {
    for run in runs() {
        if run.reaches() {
            continue;
        }
        let closed = matches!(
            run.read,
            Read::Trusted(Trusted {
                stated: Stated::Closed,
                ..
            })
        );
        assert!(
            closed || run.isolates_as_written(),
            "{:?} under {:?} locks out without a closed policy",
            run.read,
            run.ways
        );
    }
}

/// Which half of the degraded-run invariant a path breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Half {
    /// More open than the shipped rules.
    MoreOpen,
    /// Closes a right its policy grants.
    Closes,
}

#[test]
fn a_degraded_run_is_never_more_open_than_builtin_and_never_closes_what_its_policy_grants() {
    let mut broken = BTreeSet::new();
    for read in paths() {
        let (Some(_), Some(granted)) = (degraded(&read), disposition(&read)) else {
            continue;
        };
        let o = resolve(&read);
        if inputs(&o) > Inputs::Shipped {
            broken.insert((read, Half::MoreOpen));
        }
        if inputs(&o) < granted_inputs(granted) {
            broken.insert((read, Half::Closes));
        }
        for &key in SwitchKey::ALL {
            assert!(
                opens(&o, key, None) || !grants(granted, key),
                "{read:?} closes {} its policy grants",
                key.name()
            );
        }
    }
    // the shipped rules ignore the inputs no class names, which an open policy lets roam: every open degraded run, partly broken or broken, keeps to the shipped rules and closes them, and none is more open
    let want: BTreeSet<(Read, Half)> = paths()
        .into_iter()
        .filter_map(|read| match (degraded(&read), disposition(&read)) {
            (Some(_), Some(Policy::Open)) => Some((read, Half::Closes)),
            (Some(_), Some(Policy::Closed)) | (None, _) | (_, None) => None,
        })
        .collect();
    assert_eq!(broken, want);
}

#[test]
fn a_degraded_run_keeps_the_policy_its_file_states() {
    for read in paths() {
        let (Some(_), Some(granted)) = (degraded(&read), disposition(&read)) else {
            continue;
        };
        assert_eq!(resolve(&read).policy, granted, "{read:?}");
    }
}

#[test]
fn every_partly_broken_run_leaves_a_rescued_keyboard_somewhere_to_land_or_names_why_not() {
    for run in runs() {
        if degraded(&run.read) != Some(Degraded::Partly) {
            continue;
        }
        // a partly broken closed run names why not: a readable closed policy is sticky, so the rescue key stays off
        assert!(
            run.rescued() || run.outcome.rescue == Rescue::Sticky,
            "{:?} under {:?}: a rescued keyboard lands nowhere lit",
            run.read,
            run.ways
        );
    }
}

#[test]
fn a_readable_closed_policy_is_sticky_within_the_file() {
    // no other `policy` line and no broken line moves a readable closed policy, and a second `policy` line beside it leaves the file partly broken at best; a trusted file always fails to its policy
    for policies in sequences(PolicyLine::ALL, 3) {
        if !policies.contains(&PolicyLine::Closed) {
            continue;
        }
        for scopes in sequences(Scope::ALL, 2) {
            for &kept in Kept::ALL {
                let t = Trusted::of(&policies, &scopes, kept);
                assert_eq!(t.stated, Stated::Closed, "{policies:?} {scopes:?} {kept:?}");
                if policies.len() > 1 {
                    assert!(
                        matches!(t.rest, Rest::Partly | Rest::Broken),
                        "{policies:?} {scopes:?} {kept:?} reads as {:?}",
                        t.rest
                    );
                }
            }
        }
    }
    // and no rest of a closed file runs past what a valid closed file grants
    let valid = resolve(&Read::Trusted(Trusted {
        stated: Stated::Closed,
        rest: Rest::Valid,
    }));
    for &rest in Rest::ALL {
        let o = resolve(&Read::Trusted(Trusted {
            stated: Stated::Closed,
            rest,
        }));
        assert_eq!(o.policy, Policy::Closed, "{rest:?}");
        assert_ne!(o.rescue, Rescue::Forced, "{rest:?}");
        assert_eq!(o.fallback, Fallback::NoticeOnly, "{rest:?}");
        for &key in SwitchKey::ALL {
            for written in [None, Some(false), Some(true)] {
                assert!(
                    !opens(&o, key, written) || opens(&valid, key, written),
                    "{rest:?} opens {} past a valid closed file",
                    key.name()
                );
            }
        }
    }
}

/// Every sequence of at most `n` of `of`.
fn sequences<T: Copy>(of: &[T], n: usize) -> Vec<Vec<T>> {
    let mut all = vec![Vec::new()];
    let mut last = vec![Vec::new()];
    for _ in 0..n {
        last = last
            .iter()
            .flat_map(|s: &Vec<T>| {
                of.iter().map(move |&t| {
                    let mut s = s.clone();
                    s.push(t);
                    s
                })
            })
            .collect();
        all.extend(last.iter().cloned());
    }
    all
}

/// Every order of `items`.
fn orders<T: Copy>(items: &[T]) -> Vec<Vec<T>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (i, &first) in items.iter().enumerate() {
        let others: Vec<T> = items
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, &t)| t)
            .collect();
        for mut order in orders(&others) {
            order.insert(0, first);
            out.push(order);
        }
    }
    out
}

#[test]
fn no_outcome_depends_on_where_a_top_level_key_sits() {
    // `resolve` takes no line, so only `Trusted::of` could read an order
    for policies in sequences(PolicyLine::ALL, 3) {
        for scopes in sequences(Scope::ALL, 2) {
            for &kept in Kept::ALL {
                let want = Trusted::of(&policies, &scopes, kept);
                for d in orders(&policies) {
                    for s in orders(&scopes) {
                        assert_eq!(
                            Trusted::of(&d, &s, kept),
                            want,
                            "{d:?} {s:?} {kept:?} differs from {policies:?} {scopes:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_policy_is_read_from_the_policy_lines() {
    let of = |policies: &[PolicyLine], scopes: &[Scope]| Trusted::of(policies, scopes, Kept::Heads);
    assert_eq!(of(&[], &[]).stated, Stated::Unset);
    assert_eq!(
        of(&[PolicyLine::Closed], &[]),
        Trusted {
            stated: Stated::Closed,
            rest: Rest::Valid
        }
    );
    assert_eq!(of(&[PolicyLine::Open], &[]).stated, Stated::Open);
    // a readable closed policy wins, and the duplicate leaves the file partly broken
    for lines in [
        [PolicyLine::Closed, PolicyLine::Closed],
        [PolicyLine::Open, PolicyLine::Closed],
        [PolicyLine::Closed, PolicyLine::Unknown],
        [PolicyLine::InSection, PolicyLine::Closed],
    ] {
        let want = Trusted {
            stated: Stated::Closed,
            rest: Rest::Partly,
        };
        assert_eq!(of(&lines, &[]), want, "{lines:?}");
    }
    // with no readable closed policy, conflicting or unreadable lines state nothing
    for lines in [
        [PolicyLine::Open, PolicyLine::Open],
        [PolicyLine::Open, PolicyLine::Unknown],
    ] {
        assert_eq!(of(&lines, &[]).stated, Stated::Unreadable, "{lines:?}");
    }
    assert_eq!(of(&[PolicyLine::InSection], &[]).stated, Stated::Unreadable);
    // a structure vtium cannot trust drops the file, and a readable closed policy still stands
    assert_eq!(
        of(&[PolicyLine::Closed], &[Scope::File]),
        Trusted {
            stated: Stated::Closed,
            rest: Rest::Broken
        }
    );
    assert_eq!(
        Trusted::of(&[], &[Scope::Line], Kept::Headless).rest,
        Rest::Broken
    );
}

#[test]
fn every_problem_has_its_scope() {
    let q = || Quoted::new("x");
    let untrusted = Untrusted::Owner { path: q(), uid: 1 };
    let cases = [
        (Problem::NotAFile, Within::Top, Scope::Trust),
        (
            Problem::Unreadable(std::io::ErrorKind::PermissionDenied),
            Within::Top,
            Scope::Trust,
        ),
        (Problem::Untrusted(untrusted), Within::Top, Scope::Trust),
        (Problem::TooLarge, Within::Top, Scope::File),
        (Problem::NotUtf8, Within::Head, Scope::Head),
        (Problem::NotUtf8, Within::Top, Scope::Line),
        (Problem::ControlCharacter('\r'), Within::Rules, Scope::Line),
        (
            Problem::ControlCharacter('\r'),
            Within::Header,
            Scope::Section,
        ),
        (Problem::MissingVersion, Within::Top, Scope::File),
        (Problem::UnknownVersion(q()), Within::Top, Scope::File),
        (Problem::LineTooLong, Within::Top, Scope::File),
        (Problem::LineTooLong, Within::Rules, Scope::File),
        (Problem::TooManyHeads, Within::Header, Scope::File),
        (Problem::TooManyRules, Within::Rules, Scope::File),
        (
            Problem::Syntax("a line outside a section"),
            Within::Top,
            Scope::Line,
        ),
        (
            Problem::Syntax("a section header with no closing `]`"),
            Within::Header,
            Scope::Section,
        ),
        (
            Problem::Syntax("`[head NAME]` takes one name"),
            Within::Header,
            Scope::Section,
        ),
        (
            Problem::Syntax("expected `key = value`"),
            Within::Head,
            Scope::Head,
        ),
        (
            Problem::Syntax("a rule is `KIND MATCH -> TARGET`"),
            Within::Rules,
            Scope::Line,
        ),
        // a `[guards]` section is unknown, since the head file has none
        (Problem::UnknownSection(q()), Within::Header, Scope::Section),
        (
            Problem::DuplicateSection("rules"),
            Within::Header,
            Scope::Section,
        ),
        (Problem::UnknownKey(q()), Within::Top, Scope::Line),
        (Problem::UnknownKey(q()), Within::Head, Scope::Head),
        (Problem::DuplicateKey("policy"), Within::Top, Scope::Line),
        (Problem::DuplicateKey("login"), Within::Head, Scope::Head),
        (Problem::BadName(q()), Within::Header, Scope::Head),
        (Problem::BadName(q()), Within::Head, Scope::Head),
        (Problem::ReservedName(q()), Within::Header, Scope::Head),
        (Problem::DuplicateHead(q()), Within::Header, Scope::Head),
        (
            Problem::BadValue {
                key: "policy",
                value: q(),
            },
            Within::Top,
            Scope::Line,
        ),
        (
            Problem::BadValue {
                key: "isolated",
                value: q(),
            },
            Within::Head,
            Scope::Head,
        ),
        (
            Problem::OutOfRange {
                key: "consoles",
                value: q(),
            },
            Within::Head,
            Scope::Head,
        ),
        (Problem::UnknownMatchKey(q()), Within::Rules, Scope::Line),
        (Problem::BadToken(q()), Within::Rules, Scope::Line),
        (Problem::UndefinedHead(q()), Within::Rules, Scope::Line),
        (
            Problem::WrongKind {
                target: "dark",
                kind: crate::identity::Kind::Input,
            },
            Within::Rules,
            Scope::Line,
        ),
        (Problem::TooManyTokens, Within::Rules, Scope::Line),
    ];
    for (problem, within, want) in cases {
        assert_eq!(scope(&problem, within), want, "{problem:?} {within:?}");
    }
}

#[test]
fn the_shipped_rules_close_inputs_an_open_file_gives_the_focus() {
    // the device level of the degraded-run contradiction: a joystick
    let joystick = Identity::input().with_class(Class::Joystick);
    assert_eq!(Config::builtin().walk(&joystick).place, Place::Ignore);
    let open = parse("version = 1\n").map(|p| p.config).unwrap_or_default();
    assert_eq!(open.walk(&joystick).place, Place::Focus);
}
