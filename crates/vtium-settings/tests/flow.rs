//! vtium.conf as one file (ADR-1403 D3, D4, D5): the top level and the
//! sections split at the first heading with the whole file's line numbers,
//! each part read its own way, the looks and heads with their keys, and
//! the level a head's key comes from.

use vtium_settings::keys::Level;
use vtium_settings::{
    Config, Drops, Kind, LOOKS_MAX, Policy, Problem, Reading, Renamed, Syntax, TEMPLATE, Target,
    Warning, key_line, parse, parse_sections, sections, setting, split, value_for,
};

/// D4's example with a login for one head: two looks, two heads and five
/// rules below vtium's own keys. Each line's number is on its right.
const FILE: &str = "\
# /etc/vtium/vtium.conf: two displays, one keyboard.
font-size=12
xkb-layout=us
mouse
version = 1
policy = open

# A head's look: the look, which names a frit with font-name.
[look big]
font-name = monoid
font-size = 14

[look small]
font-name = dejavu

[head desk]
look = big
login = /bin/login -p

[head panel]
look = small
consoles = 2

[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display edid:BBB:0x0002:SMALL00001 -> panel
display * -> desk
input class:keyboard -> roaming
input * -> ignore
";

fn read(text: &str) -> Reading {
    sections(text.as_bytes()).expect("the file holds a flow")
}

/// `FILE` with the line numbered `n` replaced by `line`.
fn with_line(n: usize, line: &str) -> String {
    FILE.lines()
        .enumerate()
        .map(|(i, l)| if i + 1 == n { line } else { l })
        .map(|l| format!("{l}\n"))
        .collect()
}

/// A look as these tests compare it: its name, its heading's line, and
/// each key it sets with its value and line.
type LookRow<'a> = (&'a str, usize, Vec<(&'a str, &'a str, usize)>);

/// Each error's line, what it is, and what it drops.
fn errors(r: &Reading) -> Vec<(Option<usize>, Problem, Option<Drops>)> {
    r.errors
        .errors
        .iter()
        .map(|e| (e.line, e.problem.clone(), e.drops))
        .collect()
}

#[test]
fn the_whole_file_reads_in_its_two_parts_with_its_line_numbers() {
    let parts = split(FILE.as_bytes());
    assert_eq!(parts.first_section, Some(9));
    let keys: Vec<(usize, String)> = parts
        .key_lines()
        .filter_map(|(n, l)| key_line(l).map(|k| (n, String::from_utf8_lossy(k.key).into_owned())))
        .collect();
    assert_eq!(
        keys,
        [
            (2, "font-size".to_owned()),
            (3, "xkb-layout".to_owned()),
            (4, "mouse".to_owned()),
        ]
    );
    assert_eq!(setting(FILE, "font-size").as_deref(), Some("12"));

    let r = read(FILE);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let c = &r.parsed.config;
    assert_eq!(c.policy, Policy::Open);
    let looks: Vec<LookRow> = c
        .looks
        .iter()
        .map(|l| {
            let set = l
                .settings
                .iter()
                .map(|s| (s.key.name, s.value.as_str(), s.line))
                .collect();
            (l.name.as_str(), l.line, set)
        })
        .collect();
    assert_eq!(
        looks,
        [
            (
                "big",
                9,
                vec![("font-name", "monoid", 10), ("font-size", "14", 11)]
            ),
            ("small", 13, vec![("font-name", "dejavu", 14)]),
        ]
    );
    let heads: Vec<(&str, usize, &str, usize, u32)> = c
        .heads
        .iter()
        .map(|h| {
            (
                h.name.as_str(),
                h.line,
                h.profile.as_str(),
                h.profile_line,
                h.consoles,
            )
        })
        .collect();
    assert_eq!(
        heads,
        [("desk", 16, "big", 17, 1), ("panel", 20, "small", 21, 2)]
    );
    let desk = c.head("desk").expect("desk");
    assert_eq!(desk.login.as_deref(), Some("/bin/login -p"));
    assert_eq!(
        desk.settings
            .iter()
            .map(|s| (s.key.name, s.line))
            .collect::<Vec<_>>(),
        [("login", 18)]
    );
    let rules: Vec<(usize, Kind, Target)> =
        c.rules.iter().map(|r| (r.line, r.kind, r.target)).collect();
    assert_eq!(
        rules,
        [
            (25, Kind::Display, Target::Head(0)),
            (26, Kind::Display, Target::Head(1)),
            (27, Kind::Display, Target::Head(0)),
            (28, Kind::Input, Target::Roaming),
            (29, Kind::Input, Target::Ignore),
        ]
    );
}

#[test]
fn an_error_in_a_section_never_drops_a_top_level_key() {
    // a value out of bounds in a look is an error there, never clamped (D5)
    let text = with_line(11, "font-size = 9999");
    let r = read(&text);
    assert_eq!(
        errors(&r),
        [(
            Some(11),
            Problem::OutOfRange {
                key: "font-size",
                value: q("9999"),
            },
            Some(Drops::Section)
        )]
    );
    let c = &r.parsed.config;
    assert_eq!(
        c.looks.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
        ["small"]
    );
    // the head that names it stays, and draws with the default look
    let desk = c.head("desk").expect("desk stays");
    assert_eq!(c.look(desk), None);
    assert!(r.parsed.warnings.is_empty(), "{:?}", r.parsed.warnings);
    assert_eq!(setting(&text, "font-size").as_deref(), Some("12"));
    assert_eq!(
        split(text.as_bytes()).key_lines().count(),
        split(FILE.as_bytes()).key_lines().count()
    );
}

#[test]
fn a_top_level_mistake_never_drops_a_section() {
    for line in ["font-sise=3", "font-size=many", "\u{a0}bell", "[[["] {
        let text = with_line(3, line);
        let parts = split(text.as_bytes());
        if line.starts_with('[') {
            // a heading ends the top level wherever it sits
            assert_eq!(parts.first_section, Some(3));
            continue;
        }
        let r = read(&text);
        assert!(r.errors.is_empty(), "{line:?}: {:?}", r.errors);
        assert_eq!(r.parsed.config.heads.len(), 2, "{line:?}");
    }
}

#[test]
fn a_file_with_no_section_and_no_policy_line_has_no_flow() {
    assert!(sections(b"font-size=12\nbell\nversion = 1\n").is_none());
    assert_eq!(parse_sections("font-size=12\n"), Ok(None));
    // a policy line alone is the flow's, so a closed policy is never lost
    let r = read("policy = closed\n");
    assert_eq!(r.parsed.config.policy, Policy::Closed);
    assert_eq!(
        errors(&r),
        [(None, Problem::MissingVersion, Some(Drops::File))]
    );
}

#[test]
fn a_file_with_a_section_needs_its_version() {
    let text = with_line(5, "# no version");
    let r = read(&text);
    assert_eq!(
        errors(&r),
        [(None, Problem::MissingVersion, Some(Drops::File))]
    );
    assert!(r.parsed.config.heads.is_empty());
    assert!(r.parsed.config.looks.is_empty());
    assert_eq!(setting(&text, "font-size").as_deref(), Some("12"));
    let r = read(&with_line(5, "version = 2"));
    assert!(matches!(
        errors(&r).as_slice(),
        [(Some(5), Problem::UnknownVersion(_), Some(Drops::File))]
    ));
}

#[test]
fn a_policy_line_is_read_above_the_first_heading_and_is_no_key_line() {
    let text = with_line(6, "Policy = closed");
    assert!(split(text.as_bytes()).key_lines().all(|(n, _)| n != 6));
    let r = read(&text);
    assert_eq!(r.parsed.config.policy, Policy::Closed);
    assert_eq!(
        errors(&r).first().map(|e| (e.0, e.2)),
        Some((Some(6), Some(Drops::Closed)))
    );
    // below the first heading it is a line of its section, dropped alone
    let text = format!("{FILE}policy = closed\n");
    let r = read(&text);
    assert_eq!(
        errors(&r),
        [(Some(30), Problem::PolicyInSection, Some(Drops::Rule))]
    );
    assert_eq!(r.parsed.config.policy, Policy::Open);
}

#[test]
fn a_look_takes_the_key_table_s_look_keys_and_its_vtlook() {
    let good = "version = 1\n[look all]\nfont-engine = pango\nfont-name = monoid\nfont-size = 0\nfont-hinting = full\nfont-line-spacing = 100\ndisplay-depth = gray16\ndisplay-dpi = 20\nvtlook = amber\n";
    let r = read(good);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let look = r.parsed.config.looks.first().expect("the look");
    assert_eq!(look.settings.len(), 7);
    assert_eq!(
        (look.vtlook.as_deref(), look.vtlook_line),
        (Some("amber"), 10)
    );

    let cases: [(&str, Problem); 9] = [
        (
            "mouse = yes",
            Problem::NotHere {
                key: "mouse",
                section: Level::Look,
                levels: vtium_settings::keys::Levels::TOP,
            },
        ),
        (
            "login = /bin/sh",
            Problem::NotHere {
                key: "login",
                section: Level::Look,
                levels: vtium_settings::keys::Levels::TOP_HEAD,
            },
        ),
        (
            "font-size = 9999",
            Problem::OutOfRange {
                key: "font-size",
                value: q("9999"),
            },
        ),
        (
            "display-dpi = 19",
            Problem::OutOfRange {
                key: "display-dpi",
                value: q("19"),
            },
        ),
        (
            "font-engine = cairo",
            Problem::BadValue {
                key: "font-engine",
                value: q("cairo"),
            },
        ),
        (
            "font-size = big",
            Problem::BadValue {
                key: "font-size",
                value: q("big"),
            },
        ),
        (
            "vtlook = a/b",
            Problem::BadValue {
                key: "vtlook",
                value: q("a/b"),
            },
        ),
        ("frob = 1", Problem::UnknownKey(q("frob"))),
        ("font-name", Problem::Syntax(Syntax::NotKeyValue)),
    ];
    for (line, problem) in cases {
        let text = format!("version = 1\n[look x]\n{line}\n[head h]\nlook = x\n");
        let r = read(&text);
        assert_eq!(
            errors(&r),
            [(Some(3), problem, Some(Drops::Section))],
            "{line}"
        );
        // the look drops, and its head stays with the default look
        assert!(r.parsed.config.looks.is_empty(), "{line}");
        assert_eq!(r.parsed.config.heads.len(), 1, "{line}");
    }
    let twice = read("version = 1\n[look x]\nfont-size = 1\nfont-size = 2\n");
    assert_eq!(
        errors(&twice),
        [(
            Some(4),
            Problem::DuplicateKey("font-size"),
            Some(Drops::Section)
        )]
    );
}

#[test]
fn a_head_takes_its_keys_and_the_key_table_s_head_keys() {
    let cases: [(&str, Problem); 4] = [
        (
            "font-size = 12",
            Problem::NotHere {
                key: "font-size",
                section: Level::Head,
                levels: vtium_settings::keys::Levels::TOP_LOOK,
            },
        ),
        ("profile = big", Problem::Renamed(Renamed::Profile)),
        (
            "look = no/such",
            Problem::BadValue {
                key: "look",
                value: q("no/such"),
            },
        ),
        (
            "login = ",
            Problem::BadValue {
                key: "login",
                value: q(""),
            },
        ),
    ];
    for (line, problem) in cases {
        let text = format!("version = 1\n[head h]\n{line}\n[rules]\ndisplay * -> h\n");
        let r = read(&text);
        assert_eq!(
            errors(&r),
            [(Some(3), problem, Some(Drops::Head))],
            "{line}"
        );
        // the head drops with every rule naming it
        assert!(r.parsed.config.heads.is_empty(), "{line}");
        assert!(r.parsed.config.rules.is_empty(), "{line}");
    }
    let long = format!("version = 1\n[head h]\nlogin = {}\n", "x".repeat(4096));
    assert_eq!(
        errors(&read(&long)),
        [(Some(3), Problem::TooLong("login"), Some(Drops::Head))]
    );
}

#[test]
fn a_section_vtium_conf_does_not_take_drops_alone() {
    let cases: [(&str, Problem); 4] = [
        ("[profile big]", Problem::Renamed(Renamed::ProfileSection)),
        ("[look]", Problem::Syntax(Syntax::LookOneName)),
        ("[look a b]", Problem::Syntax(Syntax::LookOneName)),
        ("[frob]", Problem::NotASection(q("frob"))),
    ];
    for (heading, problem) in cases {
        let text = format!("version = 1\n{heading}\nfont-size = 1\n[head h]\n");
        let r = read(&text);
        assert_eq!(
            errors(&r),
            [(Some(2), problem, Some(Drops::Section))],
            "{heading}"
        );
        assert_eq!(r.parsed.config.heads.len(), 1, "{heading}");
    }
    // the head file's reader still knows no look
    assert!(parse("version = 1\n[look x]\n").is_err());
}

#[test]
fn a_second_look_of_one_name_drops_alone_and_past_the_bound_the_sections_drop() {
    let r = read("version = 1\n[look x]\nfont-size = 1\n[look x]\nfont-size = 2\n");
    assert_eq!(
        errors(&r),
        [(
            Some(4),
            Problem::DuplicateLook(q("x")),
            Some(Drops::Section)
        )]
    );
    let kept = &r.parsed.config.looks;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept.first().map(|l| l.line), Some(2));

    let mut many = String::from("version = 1\n");
    for i in 0..=LOOKS_MAX {
        many.push_str(&format!("[look l{i}]\n"));
    }
    let r = read(&many);
    assert_eq!(
        errors(&r),
        [(
            Some(LOOKS_MAX + 2),
            Problem::TooManyLooks,
            Some(Drops::File)
        )]
    );
    assert!(r.parsed.config.looks.is_empty());
}

#[test]
fn a_head_naming_a_look_with_no_section_is_warned_and_draws_the_default_look() {
    let r = read("version = 1\n[head h]\nlook = gone\n[head d]\nlook = default\n");
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(
        r.parsed.warnings,
        [Warning::NoLook {
            line: 3,
            head: q("h"),
            look: q("gone"),
        }]
    );
    // a `[look default]` section is the default look every other head takes
    let r = read("version = 1\n[look default]\nfont-size = 9\n[head h]\n");
    let c = &r.parsed.config;
    let h = c.head("h").expect("h");
    assert_eq!(c.look(h).map(|l| l.name.as_str()), Some("default"));
}

#[test]
fn a_head_s_section_wins_over_its_look_s_and_its_look_s_over_the_top_level() {
    let text = format!("login=/bin/sh\n{FILE}");
    assert_eq!(
        value_for(&text, "desk", "font-size"),
        Some(("14".to_owned(), Level::Look))
    );
    assert_eq!(
        value_for(&text, "panel", "font-size"),
        Some(("12".to_owned(), Level::Top))
    );
    assert_eq!(
        value_for(&text, "panel", "font-name"),
        Some(("dejavu".to_owned(), Level::Look))
    );
    assert_eq!(
        value_for(&text, "desk", "login"),
        Some(("/bin/login -p".to_owned(), Level::Head))
    );
    assert_eq!(
        value_for(&text, "panel", "login"),
        Some(("/bin/sh".to_owned(), Level::Top))
    );
    assert_eq!(value_for(&text, "panel", "xkb-variant"), None);
    let c = parse_sections(&text)
        .expect("valid")
        .expect("a flow")
        .config;
    let desk = c.head("desk").expect("desk");
    let levels: Vec<(Level, &str)> = c
        .settings(desk)
        .into_iter()
        .map(|(level, s)| (level, s.key.name))
        .collect();
    assert_eq!(
        levels,
        [
            (Level::Head, "login"),
            (Level::Look, "font-name"),
            (Level::Look, "font-size"),
        ]
    );
}

#[test]
fn the_template_reads_the_same_as_vtium_conf_s_sections() {
    let parsed = parse_sections(TEMPLATE)
        .expect("the template's sections are valid")
        .expect("the template holds a flow");
    assert_eq!(parsed.config, Config::builtin());
    assert!(parsed.warnings.is_empty());
}

/// `text` as an error quotes it: the quote an unknown key of that name
/// gets.
fn q(text: &str) -> vtium_settings::Quoted {
    let r = read(&format!("version = 1\n[head h]\n{text} = 1\n"));
    match r.errors.errors.into_iter().next().map(|e| e.problem) {
        Some(Problem::UnknownKey(q)) => q,
        other => panic!("{text:?}: {other:?}"),
    }
}

// The fuzz target's find: the section reader drops one byte-order mark,
// as the split does, so a second one is a byte of line 1 in both parts.
#[test]
fn the_section_reader_and_the_split_drop_one_byte_order_mark_alike() {
    let bytes = b"\xef\xbb\xbf\xef\xbb\xbf[,\nQNQ[\n[";
    assert_eq!(split(bytes).first_section, Some(3));
    let keys: Vec<usize> = split(bytes).key_lines().map(|(n, _)| n).collect();
    assert_eq!(keys, [1, 2]);
    let r = sections(bytes).expect("a flow");
    assert!(
        r.errors
            .errors
            .iter()
            .all(|e| e.line.is_none_or(|n| n >= 3)),
        "{:?}",
        r.errors
    );
}
