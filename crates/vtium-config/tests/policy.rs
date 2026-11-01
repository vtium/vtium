//! A policy line vtium cannot read makes the head file closed and broken
//! (ADR-1401 D7, level 2, the maintainer's ruling of 2026-11-01 and his
//! answer of the same day on its five cases).
//!
//! A policy line is a line above the first section whose bytes, after its
//! leading spaces and tabs, begin with `policy` in any case. vtium reads
//! one that says `policy = open` or `policy = closed`; any other policy
//! line drops the whole file, whatever heads it declares, under the closed
//! policy, and the boot display shows the panic screen, whose errors name
//! the line.

use vtium_config::{
    Drops, FILE_MAX, LINE_MAX, Line, Policy, PolicyFault, PolicyLine, Problem, Reading, Rest, Runs,
    Stated, Trusted, reading, resolve,
};

/// A file that declares a usable head, so only the policy line can break
/// it.
const BODY: &str = "[head a]\n[rules]\ndisplay * -> a\ninput * -> a\n";

fn outcome(r: &Reading) -> vtium_config::Outcome {
    resolve(&vtium_config::Read::Trusted(r.trusted()))
}

/// Checks that `bytes` read as closed and broken for a fault `is` accepts, the error
/// naming line `line`: nothing runs, under the closed policy, and the boot
/// display shows the panic screen.
fn closed_and_broken(bytes: &[u8], line: usize, is: impl Fn(&PolicyFault) -> bool) {
    let shown = String::from_utf8_lossy(bytes.get(..200).unwrap_or(bytes)).into_owned();
    let r = reading(bytes);
    let o = outcome(&r);
    assert_eq!(
        (o.runs, o.policy, o.line),
        (Runs::Off, Policy::Closed, Some(Line::ClosedOff)),
        "{shown:?}"
    );
    assert_eq!(r.trusted().stated, Stated::Unreadable, "{shown:?}");
    assert!(r.policies.contains(&PolicyLine::Unreadable), "{shown:?}");
    assert_eq!(r.parsed.config.policy, Policy::Closed, "{shown:?}");
    let named = r
        .errors
        .errors
        .iter()
        .find(|e| e.line == Some(line))
        .unwrap_or_else(|| panic!("{shown:?}: no error names line {line}: {:?}", r.errors));
    assert!(
        matches!(&named.problem, Problem::UnreadablePolicy(f) if is(f)),
        "{shown:?}: {named:?}"
    );
    assert_eq!(named.drops, Some(Drops::Closed), "{shown:?}");
}

// covers: platform.heads.policy
#[test]
fn a_policy_line_with_a_byte_that_is_not_utf8_makes_the_file_closed_and_broken() {
    // 0xff is a byte no UTF-8 text holds
    let bytes = [&b"version = 1\npolicy = clo\xffsed\n"[..], BODY.as_bytes()].concat();
    closed_and_broken(&bytes, 2, |f| *f == PolicyFault::NotUtf8);
    // under open too: the line meant `open`, and still closes the file
    closed_and_broken(b"version = 1\npolicy = op\xffen\n[head a]\n", 2, |f| {
        *f == PolicyFault::NotUtf8
    });
}

// covers: platform.heads.policy
#[test]
fn a_policy_line_with_a_control_character_makes_the_file_closed_and_broken() {
    for control in ['\u{1b}', '\u{7f}', '\u{85}', '\r'] {
        let text = format!("version = 1\npolicy = open{control} \n{BODY}");
        closed_and_broken(text.as_bytes(), 2, |f| {
            *f == PolicyFault::ControlCharacter(control)
        });
    }
}

// covers: platform.heads.policy
#[test]
fn a_policy_line_with_an_unknown_value_makes_the_file_closed_and_broken() {
    for value in ["opne", "maybe", "Closed", "open closed", ""] {
        let text = format!("policy = {value}\nversion = 1\n{BODY}");
        closed_and_broken(
            text.as_bytes(),
            1,
            |f| matches!(f, PolicyFault::Value(v) if v.raw() == value),
        );
    }
}

// covers: platform.heads.policy
#[test]
fn a_policy_key_in_another_case_makes_the_file_closed_and_broken() {
    for (line, key) in [
        ("Policy = closed", "Policy"),
        ("POLICY = open", "POLICY"),
        ("  pOlIcY = closed", "pOlIcY"),
        ("\tPolicy=open", "Policy"),
    ] {
        let text = format!("version = 1\n{line}\n{BODY}");
        closed_and_broken(
            text.as_bytes(),
            2,
            |f| matches!(f, PolicyFault::Key(k) if k.raw() == key),
        );
    }
    // a comment that begins with `#` is no policy line
    let r = reading(format!("version = 1\n# Policy = opne\n{BODY}").as_bytes());
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(outcome(&r).runs, Runs::File);
}

// covers: platform.heads.policy
#[test]
fn a_policy_line_past_the_line_bound_makes_the_file_closed_and_broken() {
    let long = format!("policy = closed # {}", "x".repeat(LINE_MAX));
    assert!(long.len() > LINE_MAX);
    closed_and_broken(format!("version = 1\n{long}\n{BODY}").as_bytes(), 2, |f| {
        *f == PolicyFault::LineTooLong
    });
    let long = format!("policy = open # {}", "x".repeat(LINE_MAX));
    closed_and_broken(format!("version = 1\n{long}\n{BODY}").as_bytes(), 2, |f| {
        *f == PolicyFault::LineTooLong
    });
}

// covers: platform.heads.policy
#[test]
fn a_policy_line_the_file_bound_cuts_makes_the_file_closed_and_broken() {
    /// The file's lines up to the one that starts `read` bytes before the
    /// bound, then `policy`, then the body.
    fn straddling(read: usize, policy: &str) -> (String, usize) {
        // comment lines of 64 bytes, then one that fills the rest
        let mut text = String::from("version = 1\n");
        while text.len() + 64 <= FILE_MAX - read {
            text.push_str(&format!("#{}\n", "x".repeat(62)));
        }
        while text.len() < FILE_MAX - read {
            text.insert(text.len() - 1, 'y');
        }
        let line = text.matches('\n').count() + 1;
        text.push_str(policy);
        text.push_str(BODY);
        assert!(text.len() > FILE_MAX);
        (text, line)
    }
    for policy in [
        "policy = open\n",
        "policy = closed\n",
        "  POLICY = closed\n",
    ] {
        let (text, line) = straddling(10, policy);
        closed_and_broken(text.as_bytes(), line, |f| *f == PolicyFault::Cut);
    }
    // vtium judges a cut line by the bytes it read within the bound
    // (ADR-1401 D7, the maintainer's answer of 2026-11-01), so a line cut
    // before its sixth letter is no policy line
    let (text, _) = straddling(4, "policy = closed\n");
    let r = reading(text.as_bytes());
    assert_eq!(r.trusted().stated, Stated::Unset);
    assert_eq!(outcome(&r).runs, Runs::Builtin);
    // a file the bound cuts in a comment keeps its readable policy
    let mut text = String::from("version = 1\npolicy = open\n");
    while text.len() <= FILE_MAX {
        text.push_str("# padding past the bound\n");
    }
    let r = reading(text.as_bytes());
    assert_eq!(r.trusted().stated, Stated::Open);
    assert_eq!(outcome(&r).runs, Runs::Builtin);
}

// covers: platform.heads.policy
#[test]
fn a_line_that_begins_with_policy_and_names_another_key_makes_the_file_closed_and_broken() {
    for (line, key) in [
        ("policyx = open", Some("policyx")),
        ("policy", None),
        ("policy closed", None),
        ("policy: closed", None),
        ("policy-mode = 1", Some("policy-mode")),
    ] {
        let text = format!("version = 1\n{line}\n{BODY}");
        closed_and_broken(text.as_bytes(), 2, |f| match (f, key) {
            (PolicyFault::Key(k), Some(key)) => k.raw() == key,
            (PolicyFault::NotKeyValue, None) => true,
            _ => false,
        });
    }
    // `policies` does not begin with `policy`, so the rule's own words
    // leave it a top-level line of no key, which drops that line alone; its
    // error names the keys the top level takes and picks neither, since
    // vtium does not guess what an invalid file meant (the maintainer's
    // answer of 2026-11-01)
    let r = reading(format!("version = 1\npolicies = closed\n{BODY}").as_bytes());
    let o = outcome(&r);
    assert_eq!(
        (o.runs, o.policy, o.line),
        (Runs::Survivors, Policy::Open, Some(Line::Partly))
    );
    let [error] = r.errors.errors.as_slice() else {
        panic!("{:?}", r.errors);
    };
    assert_eq!(error.line, Some(2));
    let shown = error.to_string();
    assert!(
        shown.contains("`version`") && shown.contains("`policy`"),
        "{shown}"
    );
}

// a policy line vtium cannot read wins over a readable one, in any order
// covers: platform.heads.policy
#[test]
fn a_policy_line_vtium_cannot_read_beside_a_readable_one_still_closes_and_breaks_the_file() {
    for top in [
        "policy = open\npolicy = opne\n",
        "policy = opne\npolicy = open\n",
        "policy = closed\nPolicy = closed\n",
    ] {
        let text = format!("version = 1\n{top}{BODY}");
        let r = reading(text.as_bytes());
        let o = outcome(&r);
        assert_eq!((o.runs, o.policy), (Runs::Off, Policy::Closed), "{top:?}");
    }
}

// ADR-1401 D7, the maintainer's answer of 2026-11-01: a second readable
// line is a key given twice, which drops that line alone
// covers: platform.heads.policy
#[test]
fn a_second_readable_policy_line_is_a_duplicate_that_drops_only_itself() {
    for (top, policy) in [
        ("policy = open\npolicy = open\n", Policy::Open),
        ("policy = open\npolicy = closed\n", Policy::Closed),
        ("policy = closed\npolicy = open\n", Policy::Closed),
    ] {
        let r = reading(format!("version = 1\n{top}{BODY}").as_bytes());
        let stated = match policy {
            Policy::Open => Stated::Open,
            Policy::Closed => Stated::Closed,
        };
        assert_eq!(
            r.trusted(),
            Trusted {
                stated,
                rest: Rest::Partly
            },
            "{top:?}"
        );
        let o = outcome(&r);
        assert_eq!(
            (o.runs, o.policy, o.line),
            (Runs::Survivors, policy, Some(Line::Partly)),
            "{top:?}"
        );
        assert!(
            matches!(
                r.errors.errors.as_slice(),
                [e] if e.line == Some(3)
                    && e.problem == vtium_config::Problem::DuplicateKey("policy")
            ),
            "{top:?}: {:?}",
            r.errors
        );
    }
}

// ADR-1401 D7, the maintainer's answer of 2026-11-01: the rule names the
// lines above the first section, so a `policy` line inside one
// still drops only its own line, and a bad line there drops what any bad
// line in its place drops
// covers: platform.heads.policy
#[test]
fn a_policy_line_inside_a_section_is_still_ignored() {
    let r = reading(b"version = 1\n[head a]\npolicy = closed\n[rules]\ndisplay * -> a\n");
    assert_eq!(r.policies, [PolicyLine::InSection]);
    let o = outcome(&r);
    assert_eq!(
        (o.runs, o.policy, o.line),
        (Runs::Survivors, Policy::Open, Some(Line::Partly))
    );
    // in the rules, a line that begins with `policy` is a bad rule, and a
    // bad byte in it drops that rule alone
    for bytes in [
        &b"version = 1\n[head a]\n[rules]\ndisplay * -> a\nPolicy = closed\n"[..],
        &b"version = 1\n[head a]\n[rules]\ndisplay * -> a\npolicy = clo\xffsed\n"[..],
    ] {
        let r = reading(bytes);
        let o = outcome(&r);
        assert_eq!(
            (o.runs, o.policy, o.line),
            (Runs::Survivors, Policy::Open, Some(Line::Partly)),
            "{:?}",
            String::from_utf8_lossy(bytes)
        );
        assert_eq!(r.parsed.config.heads.len(), 1);
    }
}

// ADR-1401 D7: the policy line's error is always kept, so a check and the
// panic screen name the line that closes the file, however many errors
// come before it
// covers: platform.heads.policy
#[test]
fn a_policy_lines_error_is_kept_past_the_error_bound() {
    let mut text = String::from("version = 1\n");
    for i in 0..70 {
        text.push_str(&format!("junk{i}\n"));
    }
    let line = text.matches('\n').count() + 1;
    let bad = format!("{text}policy = op\u{1b}en\n{BODY}");
    closed_and_broken(bad.as_bytes(), line, |f| {
        *f == PolicyFault::ControlCharacter('\u{1b}')
    });
    let r = reading(bad.as_bytes());
    assert_eq!(r.errors.errors.len(), vtium_config::ERRORS_MAX);
    // every error past the bound is counted, the one the policy line's
    // error took the place of among them
    assert_eq!(r.errors.errors.len() + r.errors.more, 71);
    // the same for a policy line the file bound cuts
    let mut cut = text.clone();
    while cut.len() + 30 <= FILE_MAX - 10 {
        cut.push_str("# padding past the bound ..\n");
    }
    while cut.len() < FILE_MAX - 10 {
        cut.insert(cut.len() - 1, 'y');
    }
    let line = cut.matches('\n').count() + 1;
    cut.push_str("policy = closed\n");
    cut.push_str(BODY);
    closed_and_broken(cut.as_bytes(), line, |f| *f == PolicyFault::Cut);
    // and when an error found once every line is read, such as a missing
    // `version`, sorts ahead of it
    let mut late = String::new();
    for i in 0..63 {
        late.push_str(&format!("junk{i}\n"));
    }
    late.push_str("policy = opne\n");
    late.push_str(BODY);
    closed_and_broken(
        late.as_bytes(),
        64,
        |f| matches!(f, PolicyFault::Value(v) if v.raw() == "opne"),
    );
}

// a heading past the line bound still starts its section, as a heading
// with a bad byte does, so a `policy` line after it is inside a section
// and drops only itself (ADR-1401 D7)
// covers: platform.heads.policy
#[test]
fn a_heading_past_the_line_bound_still_ends_the_top_level() {
    let heading = format!("[head a]{}", " ".repeat(LINE_MAX));
    let r = reading(format!("version = 1\n{heading}\npolicy = opne\n").as_bytes());
    assert_eq!(r.policies, [PolicyLine::InSection]);
    let o = outcome(&r);
    assert_eq!(
        (o.runs, o.policy, o.line),
        (Runs::Builtin, Policy::Open, Some(Line::Skipped))
    );
}

// ADR-1401 D7: a line that shows as `policy = closed` but begins with a
// character that does not show is no policy line, since vtium does not
// guess what an invalid file meant; its error shows that character,
// escaped, so the owner can see why
// covers: platform.heads.policy
#[test]
fn a_hidden_character_before_policy_is_shown_in_its_error() {
    for (hidden, escaped) in [
        ('\u{a0}', "\\u{a0}"),
        ('\u{200b}', "\\u{200b}"),
        ('\u{feff}', "\\u{feff}"),
        ('\u{3000}', "\\u{3000}"),
    ] {
        let text = format!("version = 1\n{hidden}policy = closed\n{BODY}");
        let r = reading(text.as_bytes());
        let o = outcome(&r);
        assert_eq!(
            (o.runs, o.policy),
            (Runs::Survivors, Policy::Open),
            "{escaped}"
        );
        let [error] = r.errors.errors.as_slice() else {
            panic!("{escaped}: {:?}", r.errors);
        };
        assert_eq!(error.line, Some(2));
        let shown = error.to_string();
        assert!(shown.contains(escaped), "{shown}");
        assert!(!shown.contains(hidden), "{shown}");
        assert!(
            shown.contains("`version`") && shown.contains("`policy`"),
            "{shown}"
        );
    }
}

// the file bound cutting a `policy` line inside a section closes nothing:
// the rule names the lines above the first section, so the file is
// dropped whole under its own policy, open here, and the shipped default
// runs (ADR-1401 D7)
// covers: platform.heads.policy
#[test]
fn a_policy_line_the_bound_cuts_inside_a_section_closes_nothing() {
    let mut text = String::from("version = 1\n[head a]\n");
    while text.len() + 30 <= FILE_MAX - 10 {
        text.push_str("# padding past the bound ..\n");
    }
    while text.len() < FILE_MAX - 10 {
        text.insert(text.len() - 1, 'y');
    }
    text.push_str("policy = closed\n");
    let r = reading(text.as_bytes());
    assert!(
        !r.policies.contains(&PolicyLine::Unreadable),
        "{:?}",
        r.policies
    );
    let o = outcome(&r);
    assert_eq!(
        (o.runs, o.policy, o.line),
        (Runs::Builtin, Policy::Open, Some(Line::Skipped))
    );
}

// the text a policy line's error quotes is escaped, so a format character
// in the key or the value never reaches the screen (ADR-1401 D7)
// covers: platform.heads.policy
#[test]
fn a_policy_lines_error_escapes_what_it_quotes() {
    for (line, hidden, escaped) in [
        ("policy\u{202e}x = open", '\u{202e}', "\\u{202e}"),
        ("policy = op\u{200b}en", '\u{200b}', "\\u{200b}"),
    ] {
        let r = reading(format!("version = 1\n{line}\n{BODY}").as_bytes());
        assert_eq!(outcome(&r).runs, Runs::Off, "{escaped}");
        let shown = r
            .errors
            .errors
            .iter()
            .find(|e| matches!(e.problem, Problem::UnreadablePolicy(_)))
            .map(ToString::to_string)
            .unwrap_or_default();
        assert!(shown.contains(escaped), "{shown}");
        assert!(!shown.contains(hidden), "{shown}");
    }
}

// the quote starts just before the hidden character, so a long key never
// cuts it off before the character the error says it shows
// covers: platform.heads.policy
#[test]
fn a_hidden_character_past_the_quote_bound_is_still_shown() {
    let key = format!("{}\u{a0}policy", "x".repeat(70));
    let r = reading(format!("version = 1\n{key} = closed\n{BODY}").as_bytes());
    let [error] = r.errors.errors.as_slice() else {
        panic!("{:?}", r.errors);
    };
    let shown = error.to_string();
    assert!(shown.contains("\\u{a0}"), "{shown}");
    assert!(!shown.contains('\u{a0}'), "{shown}");
}
