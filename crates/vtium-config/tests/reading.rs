//! What each error of a head file drops, what survives, and how the
//! reading classifies the file for the outcome model (ADR-1401 D3, D7).

use vtium_config::{
    FILE_MAX, HEADS_MAX, Kept, Line, Policy, PolicyLine, Problem, Reading, Rest, Runs, Scope,
    Stated, Target, Trusted, reading, resolve,
};

fn read(text: &str) -> Reading {
    reading(text.as_bytes())
}

fn heads(r: &Reading) -> Vec<&str> {
    r.parsed
        .config
        .heads
        .iter()
        .map(|h| h.name.as_str())
        .collect()
}

fn rule_lines(r: &Reading) -> Vec<usize> {
    r.parsed.config.rules.iter().map(|r| r.line).collect()
}

fn problems(r: &Reading) -> Vec<(Option<usize>, Problem)> {
    r.errors
        .errors
        .iter()
        .map(|e| (e.line, e.problem.clone()))
        .collect()
}

fn trusted(stated: Stated, rest: Rest) -> Trusted {
    Trusted { stated, rest }
}

// covers: platform.heads.rules
#[test]
fn a_bad_rule_drops_that_rule_alone() {
    let r = read(
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:printer -> desk\ninput class:keyboard -> desk\n",
    );
    assert_eq!(heads(&r), ["desk"]);
    assert_eq!(rule_lines(&r), [4, 6]);
    assert!(matches!(
        problems(&r).as_slice(),
        [(Some(5), Problem::BadToken(_))]
    ));
    assert_eq!(r.scopes, [Scope::Line]);
    assert_eq!(r.trusted(), trusted(Stated::Unset, Rest::Partly));
    assert_eq!(
        resolve(&vtium_config::Read::Trusted(r.trusted())).runs,
        Runs::Survivors
    );
}

// covers: platform.heads.rules
#[test]
fn an_error_in_a_heads_section_drops_the_head_with_every_rule_naming_it() {
    let r = read(
        "version = 1\n[head desk]\n[head panel]\nconsoles = two\nlogin = /bin/sh\n[rules]\ndisplay edid:AAA:0x0001:X -> panel\ndisplay * -> desk\ninput * -> panel\n",
    );
    assert_eq!(heads(&r), ["desk"]);
    // the rules naming panel go with it, and no rule names an undefined head
    assert_eq!(rule_lines(&r), [8]);
    assert_eq!(r.parsed.config.rules[0].target, Target::Head(0));
    assert!(matches!(
        problems(&r).as_slice(),
        [(
            Some(4),
            Problem::BadValue {
                key: "consoles",
                ..
            }
        )]
    ));
    assert_eq!(r.scopes, [Scope::Head]);
    assert_eq!(r.kept(), Kept::Heads);
    assert_eq!(r.trusted().rest, Rest::Partly);
    // a heading that is no name drops the head the same way
    let r = read("version = 1\n[head a/b]\n[head c]\n[rules]\ninput * -> a/b\ndisplay * -> c\n");
    assert_eq!(heads(&r), ["c"]);
    assert_eq!(rule_lines(&r), [6]);
    assert!(matches!(
        problems(&r).as_slice(),
        [(Some(2), Problem::BadName(_))]
    ));
}

// ADR-1401 D3, "One of each": the first section of a head's name stays
// covers: platform.heads.rules
#[test]
fn a_second_section_of_a_heads_name_is_dropped_and_the_first_stays() {
    let r = read(
        "version = 1\n[head a]\nconsoles = 2\n[head a]\nconsoles = 3\nfrob = x\n[rules]\ndisplay * -> a\n",
    );
    assert_eq!(heads(&r), ["a"]);
    assert_eq!(r.parsed.config.heads[0].consoles, 2);
    assert_eq!(rule_lines(&r), [8]);
    assert!(matches!(
        problems(&r).as_slice(),
        [(Some(4), Problem::DuplicateHead(_))]
    ));
    assert_eq!(r.scopes, [Scope::Section]);
}

// covers: platform.heads.guards
#[test]
fn a_guards_section_is_dropped_with_its_lines_and_the_rest_runs() {
    let r = read(
        "version = 1\n[head a]\n[rules]\ndisplay * -> a\n[guards]\nnever-dark = off\nanti-lockout = off\n",
    );
    assert_eq!(heads(&r), ["a"]);
    assert_eq!(rule_lines(&r), [4]);
    assert_eq!(
        r.parsed.config.policy,
        vtium_config::Policy::Open,
        "the policy alone sets the guards"
    );
    assert_eq!(
        problems(&r),
        [(Some(5), Problem::Renamed(vtium_config::Renamed::Guards))]
    );
    assert_eq!(r.scopes, [Scope::Section]);
}

// ADR-1401 D7 level 2: a trusted file always fails to its policy
// covers: platform.heads.policy
#[test]
fn a_missing_or_unknown_version_drops_all_but_the_policy() {
    for (text, version) in [
        (
            "policy = closed\n[head kiosk]\n[rules]\ndisplay * -> kiosk\n",
            None,
        ),
        (
            "version = 2\npolicy = closed\n[head kiosk]\n[rules]\ndisplay * -> kiosk\n",
            Some("2"),
        ),
    ] {
        let r = read(text);
        match (&r.errors.errors[0].problem, version) {
            (Problem::MissingVersion, None) => {}
            (Problem::UnknownVersion(v), Some(want)) if v.raw() == want => {}
            (other, _) => panic!("{text:?}: {other:?}"),
        }
        assert!(
            heads(&r).is_empty() && rule_lines(&r).is_empty(),
            "{text:?}"
        );
        assert_eq!(r.parsed.config.policy, Policy::Closed, "{text:?}");
        assert_eq!(
            r.trusted(),
            trusted(Stated::Closed, Rest::Broken),
            "{text:?}"
        );
        assert_eq!(
            resolve(&vtium_config::Read::Trusted(r.trusted())).runs,
            Runs::Off
        );
        // the same file under open runs the shipped rules, with its line
        let open = read(&text.replace("policy = closed", "policy = open"));
        assert_eq!(
            open.trusted(),
            trusted(Stated::Open, Rest::Broken),
            "{text:?}"
        );
        let outcome = resolve(&vtium_config::Read::Trusted(open.trusted()));
        assert_eq!(
            (outcome.runs, outcome.line),
            (Runs::Builtin, Some(Line::Skipped))
        );
    }
    let mut many = String::from("version = 1\npolicy = closed\n");
    for i in 0..=HEADS_MAX {
        many.push_str(&format!("[head h{i}]\n"));
    }
    let r = read(&many);
    assert_eq!(r.scopes, [Scope::File]);
    assert_eq!(r.trusted(), trusted(Stated::Closed, Rest::Broken));
}

// The maintainer's call while away: bytes vtium cannot read drop their
// line, section or head, as the outcome model scopes them
// covers: platform.heads.rules
#[test]
fn a_line_vtium_cannot_read_drops_only_what_its_place_takes() {
    let text = b"version = 1\npolicy = closed\n[head desk]\n[head panel]\nlogin = a\x1bb\n[head \xffx]\nconsoles = 2\n[rules]\ndisplay * -> desk\ninput name:\xff -> desk\ninput * -> desk\n";
    let r = reading(text);
    assert_eq!(
        heads(&r),
        ["desk"],
        "panel and the unreadable heading are dropped"
    );
    assert_eq!(rule_lines(&r), [9, 11]);
    assert_eq!(
        problems(&r),
        [
            (Some(5), Problem::ControlCharacter('\u{1b}')),
            (Some(6), Problem::NotUtf8),
            (Some(10), Problem::NotUtf8),
        ]
    );
    let mut scopes = r.scopes.clone();
    scopes.sort();
    assert_eq!(scopes, [Scope::Head, Scope::Section, Scope::Line]);
    // the closed policy stands
    assert_eq!(r.trusted(), trusted(Stated::Closed, Rest::Partly));
    assert_eq!(
        resolve(&vtium_config::Read::Trusted(r.trusted())).runs,
        Runs::Survivors
    );
}

// covers: platform.heads.policy
#[test]
fn a_file_past_its_bound_is_dropped_whole_with_the_policy_read_from_what_was_read() {
    let mut text =
        String::from("version = 1\npolicy = closed\n[head a]\n[rules]\ndisplay * -> a\n");
    while text.len() <= FILE_MAX {
        text.push_str("# padding past the bound\n");
    }
    let r = read(&text);
    assert_eq!(problems(&r), [(None, Problem::TooLarge)]);
    assert!(heads(&r).is_empty());
    assert_eq!(r.policies, [PolicyLine::Closed]);
    assert_eq!(r.trusted(), trusted(Stated::Closed, Rest::Broken));
}

// the last complete line within the bound is read whole, its CRLF ending
// included, and nothing past the bound is read
// covers: platform.heads.policy
#[test]
fn a_crlf_policy_line_ending_at_the_bound_is_read_and_nothing_past_it() {
    let last = "policy = closed\r\n";
    let mut text = String::from("version = 1\r\n");
    // comment lines of 64 bytes, then one that fills the rest
    while text.len() + 64 < FILE_MAX - last.len() {
        text.push_str(&format!("#{}\r\n", "x".repeat(61)));
    }
    let rest = FILE_MAX - last.len() - text.len();
    text.push_str(&format!("#{}\r\n", "y".repeat(rest - 3)));
    text.push_str(last);
    assert_eq!(text.len(), FILE_MAX);
    text.push_str("policy = open\r\n");
    let r = read(&text);
    assert_eq!(problems(&r), [(None, Problem::TooLarge)]);
    assert_eq!(r.policies, [PolicyLine::Closed]);
    assert_eq!(r.trusted(), trusted(Stated::Closed, Rest::Broken));
    // a line whose newline is the first byte past the bound is cut, and
    // so not read; a policy line cut so makes the file closed and broken
    // (ADR-1401 D7, the maintainer's answer of 2026-11-01)
    let cut = format!("{} \r\n", text.get(..FILE_MAX - 2).unwrap_or_default());
    assert_eq!(cut.as_bytes().get(FILE_MAX), Some(&b'\n'));
    let r = read(&format!("{cut}policy = open\r\n"));
    let line = cut.matches('\n').count();
    assert_eq!(
        problems(&r),
        [
            (None, Problem::TooLarge),
            (
                Some(line),
                Problem::UnreadablePolicy(vtium_config::PolicyFault::Cut)
            )
        ]
    );
    assert_eq!(
        r.policies,
        [PolicyLine::Unreadable],
        "the cut line is the policy line"
    );
    assert_eq!(r.trusted().stated, Stated::Unreadable);
}

// a byte-order mark counts toward the bound: a file past it is dropped
// whole whatever its first bytes are
// covers: platform.heads.policy
#[test]
fn a_file_past_the_bound_with_a_byte_order_mark_is_dropped_whole() {
    let mut bytes = b"\xef\xbb\xbfversion = 1\n[head a]\n".to_vec();
    while bytes.len() <= FILE_MAX {
        bytes.extend_from_slice(b"# padding\n");
    }
    bytes.truncate(FILE_MAX + 1);
    let r = reading(&bytes);
    assert_eq!(problems(&r), [(None, Problem::TooLarge)]);
    assert!(heads(&r).is_empty());
}

// ADR-1401 D3: a readable `policy = closed` wins over every other
// readable line, a second readable line is a duplicate, and a policy line
// vtium cannot read makes the file closed and broken
// covers: platform.heads.policy
#[test]
fn every_policy_line_is_read_wherever_it_sits() {
    let body = "[head a]\n[rules]\ndisplay * -> a\n";
    let cases = [
        (
            "policy = closed\npolicy = open\n",
            Stated::Closed,
            Rest::Partly,
        ),
        (
            "policy = open\npolicy = closed\n",
            Stated::Closed,
            Rest::Partly,
        ),
        ("policy = open\npolicy = open\n", Stated::Open, Rest::Partly),
        // the rest apart from the policy lines is valid
        ("policy = maybe\n", Stated::Unreadable, Rest::Valid),
        ("policy = open\n", Stated::Open, Rest::Valid),
        ("policy = closed\n", Stated::Closed, Rest::Valid),
        ("", Stated::Unset, Rest::Valid),
    ];
    for (top, stated, rest) in cases {
        let r = read(&format!("version = 1\n{top}{body}"));
        assert_eq!(r.trusted(), trusted(stated, rest), "{top:?}");
        assert_eq!(heads(&r), ["a"], "{top:?}");
    }
    // inside a section it states no policy, and drops only its line
    for text in [
        "version = 1\n[head a]\npolicy = closed\n[rules]\ndisplay * -> a\n",
        "version = 1\n[head a]\n[rules]\npolicy = closed\ndisplay * -> a\n",
    ] {
        let r = read(text);
        assert_eq!(r.policies, [PolicyLine::InSection], "{text:?}");
        assert!(matches!(
            problems(&r).as_slice(),
            [(_, Problem::PolicyInSection)]
        ));
        assert_eq!(heads(&r), ["a"], "{text:?}");
        assert_eq!(r.parsed.config.policy, Policy::Open, "{text:?}");
        assert_eq!(
            r.trusted(),
            trusted(Stated::Unset, Rest::Partly),
            "{text:?}"
        );
    }
    // beside a readable top-level line it leaves that line's policy standing
    let r = read(
        "version = 1
policy = open
[head a]
policy = closed
[rules]
display * -> a
",
    );
    assert_eq!(r.trusted(), trusted(Stated::Open, Rest::Partly));
    let closed = read(&format!("version = 1\npolicy = closed\n{body}"));
    assert_eq!(closed.parsed.config.policy, Policy::Closed);
}

// The maintainer's call while away: a valid open file with rules but no
// head runs its rules, and the display guard lights the default head
// covers: platform.heads.policy
#[test]
fn a_valid_open_file_with_rules_and_no_head_runs_its_rules() {
    let r = read("version = 1\n[rules]\ninput class:joystick -> ignore\n");
    assert!(r.errors.is_empty());
    assert_eq!(r.kept(), Kept::Headless);
    assert_eq!(r.trusted(), trusted(Stated::Unset, Rest::Headless));
    let outcome = resolve(&vtium_config::Read::Trusted(r.trusted()));
    assert_eq!((outcome.runs, outcome.line), (Runs::NoHead, None));
    // with nothing at all it is the shipped rules, silently
    let empty = read("version = 1\n");
    assert_eq!(empty.trusted(), trusted(Stated::Unset, Rest::Empty));
    assert_eq!(
        resolve(&vtium_config::Read::Trusted(empty.trusted())).runs,
        Runs::Builtin
    );
}

// the strict parse and the reading agree on a valid file, and the parse
// refuses a file with any error
#[test]
fn the_strict_parse_is_the_reading_of_a_file_with_no_error() {
    let valid = "version = 1\npolicy = closed\n[head a]\nisolated = no\n[rules]\ndisplay * -> a\n";
    let r = read(valid);
    assert!(r.errors.is_empty());
    assert_eq!(vtium_config::parse(valid), Ok(r.parsed));
    let partly = "version = 1\n[head a]\n[rules]\ndisplay * -> a\ninput frob:x -> a\n";
    let r = read(partly);
    assert_eq!(rule_lines(&r), [4]);
    assert_eq!(vtium_config::parse(partly), Err(r.errors));
}
