//! Every path through [`resolve`], its table, and the invariants over all of them (ADR-1401 D7).
//!
//! A path's outcome leaves the switches the running text writes open, so each invariant is checked under the extremes of what that text can write ([`Ways`]): nothing, every switch on, every switch off. Each check is monotone in each switch, so a text that writes some on and some off lies between the extremes. Keyboards are present on every path, and a rule may strand every one of them, so a keyboard reaches a lit head only where the shipped rules run or a guard or the rescue key gives it a way.

use std::collections::BTreeSet;

use super::*;
use crate::config::Config;
use crate::error::{Quoted, Untrusted};
use crate::identity::{Class, Identity};
use crate::parse::parse;
use crate::place::Place;

mod runtime;

/// Every path: the three verdicts that read nothing, and each disposition with each rest of a trusted file.
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

/// What the running text writes of the switches, at its extremes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ways {
    /// Nothing: each switch takes the run's value for unset.
    Unset,
    /// Every switch on, as an operator's own head under deny writes them.
    Granted,
    /// Every switch off.
    Revoked,
    /// The operator's own way in under deny: the first head writes `focus`, `vt-keys` and `rescue` on, the guards `anti-lockout` and `rescue-key` on, and nothing else is written.
    Admin,
}

impl Ways {
    fn name(self) -> &'static str {
        match self {
            Ways::Unset => "unset",
            Ways::Granted => "granted",
            Ways::Revoked => "revoked",
            Ways::Admin => "admin",
        }
    }
}

/// The extremes a path's text can write: none where nothing of the file runs or nothing survives.
fn ways(read: &Read) -> &'static [Ways] {
    match read {
        Read::Trusted(Trusted {
            rest: Rest::Valid | Rest::Partly,
            ..
        }) => &[Ways::Unset, Ways::Granted, Ways::Revoked, Ways::Admin],
        Read::Trusted(Trusted {
            rest: Rest::Headless,
            ..
        }) => &[Ways::Unset, Ways::Granted, Ways::Revoked],
        Read::Trusted(Trusted {
            rest: Rest::Empty | Rest::Broken,
            ..
        })
        | Read::Missing
        | Read::Untrusted
        | Read::Inaccessible => &[Ways::Unset],
    }
}

/// What the text writes of `key` on some head: a file with no head writes no head's right.
fn written(read: &Read, ways: Ways, key: SwitchKey) -> Option<bool> {
    let headless = matches!(
        read,
        Read::Trusted(Trusted {
            rest: Rest::Headless,
            ..
        })
    );
    let head_right = match key {
        SwitchKey::Focus | SwitchKey::VtKeys | SwitchKey::Rescue => true,
        SwitchKey::AntiLockout | SwitchKey::RescueKey | SwitchKey::NeverDark => false,
    };
    match ways {
        Ways::Unset => None,
        Ways::Granted | Ways::Revoked | Ways::Admin if headless && head_right => None,
        Ways::Admin if key == SwitchKey::NeverDark => None,
        Ways::Granted | Ways::Admin => Some(true),
        Ways::Revoked => Some(false),
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
        self.outcome
            .switch(key)
            .of(written(&self.read, self.ways, key))
    }

    /// Whether a head's display is lit whatever the rules place: by the shipped rules or the display guard.
    fn lit(&self) -> bool {
        match self.outcome.lit {
            Lit::Rules => self.on(SwitchKey::NeverDark),
            Lit::Notice => false,
        }
    }

    /// Whether the console line has a lit display to show on.
    fn shown(&self) -> bool {
        match self.outcome.lit {
            Lit::Rules => self.on(SwitchKey::NeverDark),
            Lit::Notice => true,
        }
    }

    /// Whether some head takes the focus.
    fn lands(&self) -> bool {
        let first = match self.outcome.landing {
            Landing::AsSet => false,
            Landing::FirstSurvivor => true,
        };
        first || self.on(SwitchKey::Focus)
    }

    /// Whether a keyboard the rescue key moves lands on a lit head.
    fn rescued(&self) -> bool {
        self.lit() && self.lands() && self.on(SwitchKey::RescueKey) && self.on(SwitchKey::Rescue)
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

    /// Whether the file's own valid text runs and writes every way off: what "anything written explicitly wins" lets a file do.
    fn revokes_as_written(&self) -> bool {
        self.ways == Ways::Revoked && self.outcome.runs == Runs::File
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

/// A trusted file with errors: partly broken while a usable head survives, broken once none does. The `default` line vtium cannot read is one of the errors.
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
            Stated::Unset | Stated::Allow | Stated::Deny,
            Rest::Valid | Rest::Headless | Rest::Empty,
        ) => None,
    }
}

/// The disposition a trusted file gives: deny as written, else open.
fn disposition(read: &Read) -> Option<Posture> {
    let Read::Trusted(t) = read else { return None };
    Some(match t.stated {
        Stated::Deny => Posture::Deny,
        Stated::Unset | Stated::Allow | Stated::Unreadable => Posture::Allow,
    })
}

/// How far an input no rule names in particular reaches: the shipped rules give the focus to the keyboard, pointer and key classes and ignore the rest, while an open run gives it to every input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Inputs {
    Ignored,
    Shipped,
    Focus,
}

fn inputs(o: &Outcome) -> Inputs {
    match (o.runs, o.posture) {
        (Runs::Builtin, Posture::Allow | Posture::Deny) => Inputs::Shipped,
        (Runs::Off, Posture::Allow | Posture::Deny) => Inputs::Ignored,
        (Runs::File | Runs::Survivors, Posture::Allow) => Inputs::Focus,
        (Runs::File | Runs::Survivors, Posture::Deny) => Inputs::Ignored,
    }
}

fn granted_inputs(posture: Posture) -> Inputs {
    match posture {
        Posture::Allow => Inputs::Focus,
        Posture::Deny => Inputs::Ignored,
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
    let mut out =
        String::from("read\tdefault\trest\truns\tposture\trescue\tlanding\tline\tlit\tfallback");
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
            o.posture.name(),
            o.rescue.name(),
            o.landing.name(),
            o.line.map_or("-", Line::text),
            o.lit.name(),
            o.fallback.name(),
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
                    Stated::Unset | Stated::Allow | Stated::Deny,
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
        if run.outcome.posture != Posture::Allow || run.reaches() {
            continue;
        }
        assert!(
            run.revokes_as_written(),
            "{:?} under {:?} is open and locks out",
            run.read,
            run.ways
        );
        explicit.insert(run.read);
    }
    // contradiction: a valid open file that writes every way off locks out, since anything written explicitly wins
    let want: BTreeSet<Read> = [Stated::Unset, Stated::Allow]
        .into_iter()
        .flat_map(|stated| {
            [Rest::Valid, Rest::Headless]
                .into_iter()
                .map(move |rest| Read::Trusted(Trusted { stated, rest }))
        })
        .collect();
    assert_eq!(explicit, want);
}

#[test]
fn a_lockout_needs_a_trusted_file_that_wrote_deny() {
    for run in runs() {
        if run.reaches() {
            continue;
        }
        let deny = matches!(
            run.read,
            Read::Trusted(Trusted {
                stated: Stated::Deny,
                ..
            })
        );
        assert!(
            deny || run.revokes_as_written(),
            "{:?} under {:?} locks out without a deny",
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
    /// Closes a right its disposition grants.
    Closes,
}

#[test]
fn a_degraded_run_is_never_more_open_than_builtin_and_never_closes_what_its_disposition_grants() {
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
                o.switch(key).of(None) || !granted.unset(key),
                "{read:?} closes {} its disposition grants",
                key.name()
            );
        }
    }
    // contradiction: the shipped rules ignore the inputs no class names, which an open disposition gives the focus, so an open degraded run breaks one half for them: partly broken runs its survivors open, broken runs the shipped rules
    let want: BTreeSet<(Read, Half)> = paths()
        .into_iter()
        .filter_map(|read| match (degraded(&read), disposition(&read)) {
            (Some(Degraded::Partly), Some(Posture::Allow)) => Some((read, Half::MoreOpen)),
            (Some(Degraded::Broken), Some(Posture::Allow)) => Some((read, Half::Closes)),
            (Some(_), Some(Posture::Deny)) | (None, _) | (_, None) => None,
        })
        .collect();
    assert_eq!(broken, want);
}

#[test]
fn a_degraded_run_keeps_the_disposition_its_file_states() {
    for read in paths() {
        let (Some(_), Some(granted)) = (degraded(&read), disposition(&read)) else {
            continue;
        };
        assert_eq!(resolve(&read).posture, granted, "{read:?}");
    }
}

#[test]
fn every_partly_broken_run_leaves_a_rescued_keyboard_somewhere_to_land_or_names_why_not() {
    for run in runs() {
        if degraded(&run.read) != Some(Degraded::Partly) {
            continue;
        }
        // a partly broken deny run names why not: a readable deny is sticky, so the rescue goes only where the file grants it
        assert!(
            run.rescued() || run.outcome.rescue == Rescue::Sticky,
            "{:?} under {:?}: a rescued keyboard lands nowhere lit",
            run.read,
            run.ways
        );
    }
}

#[test]
fn a_readable_deny_is_sticky_within_the_file() {
    // no other `default` line and no error moves a readable deny
    for defaults in sequences(DefaultLine::ALL, 3) {
        if !defaults.contains(&DefaultLine::Deny) {
            continue;
        }
        for scopes in sequences(Scope::ALL, 2) {
            for &kept in Kept::ALL {
                let t = Trusted::of(&defaults, &scopes, kept);
                assert_eq!(t.stated, Stated::Deny, "{defaults:?} {scopes:?} {kept:?}");
            }
        }
    }
    // and no rest of a deny file runs past what a valid deny file grants
    let valid = resolve(&Read::Trusted(Trusted {
        stated: Stated::Deny,
        rest: Rest::Valid,
    }));
    for &rest in Rest::ALL {
        let o = resolve(&Read::Trusted(Trusted {
            stated: Stated::Deny,
            rest,
        }));
        assert_eq!(o.posture, Posture::Deny, "{rest:?}");
        assert_ne!(o.rescue, Rescue::Forced, "{rest:?}");
        assert_eq!(o.landing, Landing::AsSet, "{rest:?}");
        assert_eq!(o.fallback, Fallback::Notice, "{rest:?}");
        // `never-dark` lights a display and grants no input: a degraded deny run keeps its notice on a lit display
        for &key in SwitchKey::ALL
            .iter()
            .filter(|&&k| k != SwitchKey::NeverDark)
        {
            for written in [None, Some(false), Some(true)] {
                assert!(
                    !o.switch(key).of(written) || valid.switch(key).of(written),
                    "{rest:?} opens {} past a valid deny file",
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
    for defaults in sequences(DefaultLine::ALL, 3) {
        for scopes in sequences(Scope::ALL, 2) {
            for &kept in Kept::ALL {
                let want = Trusted::of(&defaults, &scopes, kept);
                for d in orders(&defaults) {
                    for s in orders(&scopes) {
                        assert_eq!(
                            Trusted::of(&d, &s, kept),
                            want,
                            "{d:?} {s:?} {kept:?} differs from {defaults:?} {scopes:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_disposition_is_read_from_the_default_lines() {
    let of =
        |defaults: &[DefaultLine], scopes: &[Scope]| Trusted::of(defaults, scopes, Kept::Heads);
    assert_eq!(of(&[], &[]).stated, Stated::Unset);
    assert_eq!(
        of(&[DefaultLine::Deny], &[]),
        Trusted {
            stated: Stated::Deny,
            rest: Rest::Valid
        }
    );
    assert_eq!(of(&[DefaultLine::Allow], &[]).stated, Stated::Allow);
    for (lines, want) in [
        ([DefaultLine::Deny, DefaultLine::Deny], Stated::Deny),
        ([DefaultLine::Allow, DefaultLine::Deny], Stated::Deny),
        ([DefaultLine::Deny, DefaultLine::Unknown], Stated::Deny),
        ([DefaultLine::InSection, DefaultLine::Deny], Stated::Deny),
        ([DefaultLine::Allow, DefaultLine::Allow], Stated::Unreadable),
        (
            [DefaultLine::Allow, DefaultLine::Unknown],
            Stated::Unreadable,
        ),
    ] {
        assert_eq!(of(&lines, &[]).stated, want, "{lines:?}");
    }
    assert_eq!(
        of(&[DefaultLine::InSection], &[]).stated,
        Stated::Unreadable
    );
    // a structure or format vtium cannot trust drops the file, and a readable deny still stands
    assert_eq!(
        of(&[DefaultLine::Deny], &[Scope::File]),
        Trusted {
            stated: Stated::Deny,
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
            Problem::Syntax("expected `key = value`"),
            Within::Guards,
            Scope::Line,
        ),
        (
            Problem::Syntax("a rule is `KIND MATCH -> TARGET`"),
            Within::Rules,
            Scope::Line,
        ),
        (Problem::UnknownSection(q()), Within::Header, Scope::Section),
        (
            Problem::DuplicateSection("rules"),
            Within::Header,
            Scope::Section,
        ),
        (Problem::UnknownKey(q()), Within::Top, Scope::Line),
        (Problem::UnknownKey(q()), Within::Head, Scope::Head),
        (Problem::UnknownKey(q()), Within::Guards, Scope::Line),
        (Problem::DuplicateKey("default"), Within::Top, Scope::Line),
        (Problem::DuplicateKey("login"), Within::Head, Scope::Head),
        (
            Problem::DuplicateKey("rescue-key"),
            Within::Guards,
            Scope::Line,
        ),
        (Problem::BadName(q()), Within::Header, Scope::Head),
        (Problem::BadName(q()), Within::Head, Scope::Head),
        (Problem::ReservedName(q()), Within::Header, Scope::Head),
        (Problem::DuplicateHead(q()), Within::Header, Scope::Head),
        (
            Problem::BadValue {
                key: "default",
                value: q(),
            },
            Within::Top,
            Scope::Line,
        ),
        (
            Problem::BadValue {
                key: "rescue",
                value: q(),
            },
            Within::Head,
            Scope::Head,
        ),
        (
            Problem::BadValue {
                key: "never-dark",
                value: q(),
            },
            Within::Guards,
            Scope::Line,
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
