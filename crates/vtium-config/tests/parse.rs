//! The head file's parser: what makes a file invalid, each error named
//! with its line, and what a valid file says (ADR-1401 D3, D7).

use vtium_config::{Problem, parse};

/// The invalid file of ADR-1401's figure: line 6 names `desc`, a head no
/// section declares.
const INVALID_FIGURE: &str = "\
version = 1
[head desk]
profile = desk
[rules]
display * -> desk
input path:pci-0000:15:00.0-usb-0:3.3 -> desc
input class:keyboard -> focus
";

#[test]
fn a_rule_naming_an_undefined_head_rejects_the_file_at_its_line() {
    let rejected = parse(INVALID_FIGURE).expect_err("a rule names an undefined head");
    assert_eq!(rejected.errors.len(), 1, "{:?}", rejected.errors);
    let error = &rejected.errors[0];
    assert_eq!(error.line, Some(6));
    assert!(matches!(&error.problem, Problem::UndefinedHead(name) if name.raw() == "desc"));
    assert_eq!(
        error.to_string(),
        "line 6: rule names undefined head \"desc\""
    );
}

/// A file, the line its one error names, and a test of the error.
type Case = (&'static str, Option<usize>, fn(&Problem) -> bool);

/// Each kind of invalid file D7 names, and the line its error names.
#[test]
fn each_kind_of_invalid_file_is_rejected_whole_at_its_line() {
    let cases: &[Case] = &[
        ("", None, |p| matches!(p, Problem::MissingVersion)),
        ("# only a comment\n", None, |p| {
            matches!(p, Problem::MissingVersion)
        }),
        ("[head a]\n", Some(1), |p| {
            matches!(p, Problem::MissingVersion)
        }),
        (
            "version = 2\n",
            Some(1),
            |p| matches!(p, Problem::UnknownVersion(v) if v.raw() == "2"),
        ),
        ("version = 1\nnonsense\n", Some(2), |p| {
            matches!(p, Problem::Syntax(_))
        }),
        ("version = 1\n[head a\n", Some(2), |p| {
            matches!(p, Problem::Syntax(_))
        }),
        (
            "version = 1\n[head a]\ncolour = red\n",
            Some(3),
            |p| matches!(p, Problem::UnknownKey(k) if k.raw() == "colour"),
        ),
        ("version = 1\n[head a]\nprofile\n", Some(3), |p| {
            matches!(p, Problem::Syntax(_))
        }),
        ("version = 1\n[screens]\n", Some(2), |p| {
            matches!(p, Problem::UnknownSection(_))
        }),
        ("version = 1\n[head a]\n[head a]\n", Some(3), |p| {
            matches!(p, Problem::DuplicateHead(_))
        }),
        ("version = 1\n[head focus]\n", Some(2), |p| {
            matches!(p, Problem::ReservedName(_))
        }),
        ("version = 1\n[head dark]\n", Some(2), |p| {
            matches!(p, Problem::ReservedName(_))
        }),
        ("version = 1\n[head a/b]\n", Some(2), |p| {
            matches!(p, Problem::BadName(_))
        }),
        ("version = 1\n[head a]\nprofile = ../x\n", Some(3), |p| {
            matches!(p, Problem::BadName(_))
        }),
        ("version = 1\n[head a]\nconsoles = 0\n", Some(3), |p| {
            matches!(
                p,
                Problem::OutOfRange {
                    key: "consoles",
                    ..
                }
            )
        }),
        ("version = 1\n[head a]\nconsoles = two\n", Some(3), |p| {
            matches!(
                p,
                Problem::BadValue {
                    key: "consoles",
                    ..
                }
            )
        }),
        ("version = 1\n[head a]\nfont-size = 2049\n", Some(3), |p| {
            matches!(
                p,
                Problem::OutOfRange {
                    key: "font-size",
                    ..
                }
            )
        }),
        ("version = 1\n[head a]\non-loss = never\n", Some(3), |p| {
            matches!(p, Problem::BadValue { key: "on-loss", .. })
        }),
        (
            "version = 1\n[head a]\non-loss = hangup 4294967296\n",
            Some(3),
            |p| matches!(p, Problem::OutOfRange { key: "on-loss", .. }),
        ),
        ("version = 1\n[head a]\nrescue = no\n", Some(3), |p| {
            matches!(p, Problem::BadValue { key: "rescue", .. })
        }),
        (
            "version = 1\n[head a]\nconsoles = 2\nconsoles = 3\n",
            Some(4),
            |p| matches!(p, Problem::DuplicateKey("consoles")),
        ),
        (
            "version = 1\n[guards]\nnever-dark = maybe\n",
            Some(3),
            |p| {
                matches!(
                    p,
                    Problem::BadValue {
                        key: "never-dark",
                        ..
                    }
                )
            },
        ),
        ("version = 1\n[guards]\nlockout = off\n", Some(3), |p| {
            matches!(p, Problem::UnknownKey(_))
        }),
        ("version = 1\n[rules]\n[rules]\n", Some(3), |p| {
            matches!(p, Problem::DuplicateSection("rules"))
        }),
        ("version = 1\n[rules]\nscreen * -> dark\n", Some(3), |p| {
            matches!(p, Problem::Syntax(_))
        }),
        ("version = 1\n[rules]\ndisplay * dark\n", Some(3), |p| {
            matches!(p, Problem::Syntax(_))
        }),
        ("version = 1\n[rules]\ndisplay -> dark\n", Some(3), |p| {
            matches!(p, Problem::Syntax(_))
        }),
        (
            "version = 1\n[rules]\ndisplay * class:key -> dark\n",
            Some(3),
            |p| matches!(p, Problem::Syntax(_)),
        ),
        ("version = 1\n[rules]\ndisplay * -> focus\n", Some(3), |p| {
            matches!(
                p,
                Problem::WrongKind {
                    target: "focus",
                    ..
                }
            )
        }),
        (
            "version = 1\n[rules]\ndisplay * -> ignore\n",
            Some(3),
            |p| {
                matches!(
                    p,
                    Problem::WrongKind {
                        target: "ignore",
                        ..
                    }
                )
            },
        ),
        ("version = 1\n[rules]\ninput * -> dark\n", Some(3), |p| {
            matches!(p, Problem::WrongKind { target: "dark", .. })
        }),
        (
            "version = 1\n[rules]\ninput colour:red -> focus\n",
            Some(3),
            |p| matches!(p, Problem::UnknownMatchKey(_)),
        ),
        (
            "version = 1\n[rules]\ninput keyboard -> focus\n",
            Some(3),
            |p| matches!(p, Problem::UnknownMatchKey(_)),
        ),
        (
            "version = 1\n[rules]\ninput class:printer -> focus\n",
            Some(3),
            |p| matches!(p, Problem::BadToken(_)),
        ),
        (
            "version = 1\n[rules]\ninput id:zzzz:0001 -> focus\n",
            Some(3),
            |p| matches!(p, Problem::BadToken(_)),
        ),
        (
            "version = 1\n[rules]\ndisplay edid:aaa:0x1:S -> dark\n",
            Some(3),
            |p| matches!(p, Problem::BadToken(_)),
        ),
        (
            "version = 1\n[rules]\ninput serial:aaaa:0002 -> focus\n",
            Some(3),
            |p| matches!(p, Problem::BadToken(_)),
        ),
        (
            "version = 1\n[rules]\ninput udev:=x -> focus\n",
            Some(3),
            |p| matches!(p, Problem::BadToken(_)),
        ),
        (
            "version = 1\n[rules]\ndisplay connector:DP-8 -> dark\n",
            Some(3),
            |p| matches!(p, Problem::BadToken(_)),
        ),
        (
            "version = 1\n[rules]\ninput path: -> focus\n",
            Some(3),
            |p| matches!(p, Problem::BadToken(_)),
        ),
        (
            "version = 1\n[rules]\ninput name:a name:b name:c name:d name:e name:f name:g name:h name:i -> focus\n",
            Some(3),
            |p| matches!(p, Problem::TooManyTokens),
        ),
    ];
    for (text, line, problem) in cases {
        let rejected = match parse(text) {
            Ok(p) => panic!("{text:?} parsed: {p:?}"),
            Err(r) => r,
        };
        assert_eq!(rejected.errors.len(), 1, "{text:?}: {:?}", rejected.errors);
        assert_eq!(rejected.errors[0].line, *line, "{text:?}");
        assert!(
            problem(&rejected.errors[0].problem),
            "{text:?}: {:?}",
            rejected.errors[0]
        );
    }
}

#[test]
fn a_file_past_a_bound_is_rejected_whole() {
    let long = format!("version = 1\n# {}\n", "x".repeat(vtium_config::LINE_MAX));
    let r = parse(&long).expect_err("a line past LINE_MAX");
    assert_eq!(
        (r.errors[0].line, &r.errors[0].problem),
        (Some(2), &Problem::LineTooLong)
    );

    let mut heads = String::from("version = 1\n");
    for i in 0..=vtium_config::HEADS_MAX {
        heads.push_str(&format!("[head h{i}]\n"));
    }
    let r = parse(&heads).expect_err("a head past HEADS_MAX");
    assert_eq!(r.errors.len(), 1);
    assert_eq!(r.errors[0].line, Some(vtium_config::HEADS_MAX + 2));
    assert_eq!(r.errors[0].problem, Problem::TooManyHeads);

    let mut rules = String::from("version = 1\n[rules]\n");
    for _ in 0..=vtium_config::RULES_MAX {
        rules.push_str("input * -> focus\n");
    }
    let r = parse(&rules).expect_err("a rule past RULES_MAX");
    assert_eq!(r.errors.len(), 1);
    assert_eq!(r.errors[0].line, Some(vtium_config::RULES_MAX + 3));
    assert_eq!(r.errors[0].problem, Problem::TooManyRules);
}

#[test]
fn every_error_of_a_file_is_named_with_its_line_in_order() {
    let text = "\
version = 1
[head desk]
consoles = 0
[rules]
display * -> nowhere
input class:printer -> focus
[head desk]
";
    let r = parse(text).expect_err("four errors");
    let lines: Vec<_> = r.errors.iter().map(|e| e.line).collect();
    assert_eq!(
        lines,
        [Some(3), Some(5), Some(6), Some(7)],
        "{:?}",
        r.errors
    );
    assert!(matches!(&r.errors[1].problem, Problem::UndefinedHead(n) if n.raw() == "nowhere"));
}

#[test]
fn errors_past_the_bound_are_counted() {
    let mut text = String::from("version = 1\n");
    for _ in 0..vtium_config::ERRORS_MAX + 5 {
        text.push_str("nonsense\n");
    }
    let r = parse(&text).expect_err("every line an error");
    assert_eq!(r.errors.len(), vtium_config::ERRORS_MAX);
    assert_eq!(r.more, 5);
}

#[test]
fn text_an_error_quotes_is_escaped_and_cut() {
    // a bidirectional override passes the file checks, being no control
    // character, and the error shows it escaped
    let r = parse("version = 1\n[head a\u{202e}b]\n").expect_err("a bad name");
    let shown = r.errors[0].to_string();
    assert!(shown.contains("\"a\\u{202e}b\""), "{shown}");
    assert!(!shown.contains('\u{202e}'), "{shown}");

    let long = "n".repeat(200);
    let r = parse(&format!("version = 1\n[head {long}]\n")).expect_err("a name past NAME_MAX");
    let shown = r.errors[0].to_string();
    let quoted = format!("\"{}...\"", "n".repeat(vtium_config::QUOTE_MAX));
    assert!(shown.contains(&quoted), "{shown}");
}

/// The example file of ADR-1401 D3.
const EXAMPLE: &str = "\
# /etc/vtium/heads.conf for the machine under Evidence; the EDID values are illustrative
version = 1

[head desk]
profile = desk

[head panel]
profile = small
consoles = 2
on-loss = hangup 30s

[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display edid:BBB:0x0002:SMALL00001 -> panel
input path:pci-0000:15:00.0-usb-0:3.3 -> desk
input path:pci-0000:15:00.0-usb-0:3.4.4 -> desk
input path:pci-0000:15:00.0-usb-0:3.2 -> panel
input path:pci-0000:15:00.0-usb-0:3.4.2 -> focus
# needs the example udev rule that sets VTIUM_HEAD (D4)
input udev:VTIUM_HEAD=panel -> panel
display * -> desk
input class:keyboard -> focus
input class:mouse -> focus
input * -> ignore

[guards]
# each guard is on; turning one off is a choice to risk a lockout
anti-lockout = on
rescue-key = on
never-dark = on
";

#[test]
fn the_records_example_file_is_valid_and_reads_every_key() {
    use vtium_config::{Edid, Kind, Match, OnLoss, Target, Token};
    let parsed = parse(EXAMPLE).expect("the record's example is valid");
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert!(parsed.notes.is_empty(), "{:?}", parsed.notes);
    let c = parsed.config;
    let names: Vec<_> = c.heads.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["desk", "panel"]);
    assert_eq!(c.heads[0].profile, "desk");
    assert_eq!(
        (c.heads[0].consoles, c.heads[0].on_loss),
        (1, OnLoss::Nohup)
    );
    assert_eq!(c.heads[1].profile, "small");
    assert_eq!(
        (c.heads[1].consoles, c.heads[1].on_loss),
        (2, OnLoss::HangupAfter(30))
    );
    assert_eq!(c.rules.len(), 11);
    assert_eq!(c.rules[0].line, 13);
    assert_eq!(c.rules[0].kind, Kind::Display);
    assert_eq!(
        c.rules[0].matcher,
        Match::All(vec![Token::Edid(Edid {
            maker: "AAA".into(),
            product: 1,
            serial: "LARGE00001".into(),
        })])
    );
    assert_eq!(c.rules[0].target, Target::Head(0));
    assert_eq!(c.rules[4].target, Target::Head(1));
    assert_eq!(c.rules[5].target, Target::Focus);
    assert_eq!(
        c.rules[6].matcher,
        Match::All(vec![Token::Udev {
            name: "VTIUM_HEAD".into(),
            value: "panel".into(),
        }])
    );
    assert_eq!(c.rules[10].target, Target::Ignore);
}

#[test]
fn blanks_and_comments_are_dropped_anywhere() {
    let text = "  # lead\n\tversion\t=\t1   # the format\n\n[ head  a ]\n  consoles=3#three\n[rules]\n display\t*  ->   a # all\n";
    let c = parse(text).expect("valid").config;
    assert_eq!(c.heads[0].name, "a");
    assert_eq!(c.heads[0].consoles, 3);
    assert_eq!(c.rules[0].line, 7);
}

#[test]
fn a_rule_may_name_a_head_declared_after_it() {
    let c = parse("version = 1\n[rules]\ndisplay * -> late\n[head late]\n")
        .expect("valid")
        .config;
    assert_eq!(c.rules[0].target, vtium_config::Target::Head(0));
}

#[test]
fn on_loss_takes_seconds_or_a_suffix() {
    use vtium_config::OnLoss;
    for (value, want) in [
        ("nohup", OnLoss::Nohup),
        ("hangup", OnLoss::Hangup),
        ("hangup 30", OnLoss::HangupAfter(30)),
        ("hangup 30s", OnLoss::HangupAfter(30)),
        ("hangup   5m", OnLoss::HangupAfter(300)),
        ("hangup 1h", OnLoss::HangupAfter(3600)),
        ("hangup 0", OnLoss::HangupAfter(0)),
    ] {
        let text = format!("version = 1\n[head a]\non-loss = {value}\n");
        let c = parse(&text)
            .unwrap_or_else(|e| panic!("{value}: {e:?}"))
            .config;
        assert_eq!(c.heads[0].on_loss, want, "{value}");
    }
    for value in [
        "hangup 5x",
        "hangup 1 2",
        "hangup s",
        "hangup -1",
        "nohup 5",
        "hangup 1193047h",
    ] {
        let text = format!("version = 1\n[head a]\non-loss = {value}\n");
        assert!(parse(&text).is_err(), "{value} parsed");
    }
}

#[test]
fn consoles_past_the_cap_get_the_cap_with_a_warning() {
    use vtium_config::{CONSOLES_MAX, Warning};
    for value in ["51", "99999999999999999999"] {
        let p = parse(&format!("version = 1\n[head a]\nconsoles = {value}\n")).expect("valid");
        assert_eq!(p.config.heads[0].consoles, CONSOLES_MAX);
        assert_eq!(p.warnings, [Warning::ConsolesCapped { line: 3 }]);
    }
    let p = parse("version = 1\n[head a]\nconsoles = 50\n").expect("valid");
    assert_eq!(p.config.heads[0].consoles, 50);
    assert!(p.warnings.is_empty());
}

#[test]
fn a_rule_an_earlier_one_shadows_is_a_warning() {
    use vtium_config::Warning;
    let text = "\
version = 1
[head a]
[rules]
input class:keyboard -> a
input class:keyboard name:Numpad -> focus
display * -> a
display edid:AAA:0x0001:X -> dark
input name:Numpad -> ignore
";
    let p = parse(text).expect("valid");
    assert_eq!(
        p.warnings,
        [
            Warning::Shadowed { line: 5, by: 4 },
            Warning::Shadowed { line: 7, by: 6 },
        ]
    );
}

#[test]
fn guards_turned_off_weak_matches_and_rescue_off_are_notes() {
    use vtium_config::Note;
    let text = "\
version = 1
[head kiosk]
rescue = off
[rules]
input id:aaaa:0001 -> kiosk
input name:Keyboard* id:aaaa:0002 -> kiosk
input id:aaaa:0003 path:pci-0000:15:00.0-usb-0:3.3 -> kiosk
input name:Token -> focus
[guards]
anti-lockout = off
rescue-key = on
never-dark = off
";
    let p = parse(text).expect("valid");
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(
        p.notes,
        [
            Note::RescueOff { line: 3 },
            Note::WeakMatch { line: 5 },
            Note::WeakMatch { line: 6 },
            Note::GuardOff {
                line: 10,
                guard: "anti-lockout"
            },
            Note::GuardOff {
                line: 12,
                guard: "never-dark"
            },
        ]
    );
    let g = p.config.guards;
    assert!(!g.anti_lockout && g.rescue_key && !g.never_dark);
    assert!(!p.config.heads[0].rescue);
}

#[test]
fn the_keys_that_need_udevs_database_are_listed_by_line() {
    let text = "\
version = 1
[head a]
[rules]
input path:pci-0000:15:00.0-usb-0:3.3 serial:aaaa:0002:N -> a
input class:keyboard name:K id:aaaa:0001 bluetooth:aa:bb -> a
display card:pci-* -> a
display connector:pci-0000:03:00.0/DP-* edid:AAA:0x0001:S connector-path:mst:1-2 -> a
input udev:VTIUM_HEAD=a -> a
";
    let c = parse(text).expect("valid").config;
    assert_eq!(
        c.needs_udev(),
        [
            (4, "path"),
            (4, "serial"),
            (6, "card"),
            (7, "connector"),
            (8, "udev"),
        ]
    );
}

#[test]
fn a_heads_font_is_its_own_over_its_profile_over_the_configuration() {
    use vtium_config::Font;
    let text = "\
version = 1
[head plain]
[head named]
profile = desk
[head own]
profile = desk
font-name = terminus
font-size = 12
";
    let c = parse(text).expect("valid").config;
    let base = Font {
        name: Some("monospace".into()),
        size: Some(10),
    };
    assert_eq!(c.heads[0].font(&base), base);
    assert_eq!(
        c.heads[1].font(&base),
        Font {
            name: Some("desk".into()),
            size: Some(10),
        }
    );
    assert_eq!(
        c.heads[2].font(&base),
        Font {
            name: Some("terminus".into()),
            size: Some(12),
        }
    );
    assert_eq!(c.heads[0].font(&Font::default()), Font::default());
}

#[test]
fn font_size_max_is_vtium_fonts_cell_bound() {
    assert_eq!(vtium_config::FONT_SIZE_MAX, vtium_font::CELL_SIDE_MAX);
}

#[test]
fn a_head_keeps_its_login_as_written() {
    let c = parse("version = 1\n[head a]\nlogin = /sbin/agetty -o '-p -- \\u' $TERM\n")
        .expect("valid")
        .config;
    assert_eq!(
        c.heads[0].login.as_deref(),
        Some("/sbin/agetty -o '-p -- \\u' $TERM")
    );
}

#[test]
fn the_errors_kept_are_the_first_by_line() {
    // the undefined head is found only once every head is known, after
    // the errors of every later line
    let mut text = String::from("version = 1\n[rules]\ninput * -> nowhere\n");
    for _ in 0..vtium_config::ERRORS_MAX + 6 {
        text.push_str("nonsense\n");
    }
    let r = parse(&text).expect_err("errors");
    assert_eq!(r.errors.len(), vtium_config::ERRORS_MAX);
    assert_eq!(r.more, 7);
    assert_eq!(r.errors[0].line, Some(3));
    assert!(matches!(&r.errors[0].problem, Problem::UndefinedHead(n) if n.raw() == "nowhere"));
    let lines: Vec<_> = r.errors.iter().map(|e| e.line).collect();
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted);
    assert_eq!(lines.last(), Some(&Some(vtium_config::ERRORS_MAX + 2)));
}

#[test]
fn warnings_and_notes_past_the_bound_are_counted() {
    use vtium_config::{ERRORS_MAX, Note, Warning};
    let mut text = String::from("version = 1\n[head a]\n[rules]\ninput * -> a\n");
    for i in 0..ERRORS_MAX + 3 {
        // shadowed by line 4, and a weak match: one warning and one note
        text.push_str(&format!("input name:k{i} -> a\n"));
    }
    let p = parse(&text).expect("valid");
    assert_eq!(p.warnings.len(), ERRORS_MAX);
    assert_eq!(p.more_warnings, 3);
    assert_eq!(p.warnings[0], Warning::Shadowed { line: 5, by: 4 });
    assert_eq!(p.notes.len(), ERRORS_MAX);
    assert_eq!(p.more_notes, 3);
    assert_eq!(p.notes[0], Note::WeakMatch { line: 5 });
}
