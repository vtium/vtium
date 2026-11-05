//! vtium.conf's section writer (ADR-1403 D7, D8): each edit changes only
//! the lines it needs, refuses sections vtium reads only in part, and
//! reads its own result before handing it back; a new rule goes where
//! what it names puts it.

use vtium_settings::{
    Kind, Of, Problem, SectionEdit, add_rule, add_section, move_rule, parse_sections, placement,
    remove_rule, set_key, set_rule,
};

const FILE: &str = "\
font-size=12
version = 1

[look big]
font-name = monoid
font-size = 14   # the desk's

[head desk]
look = big

[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display * -> desk
input path:pci-0000:15:00.0-usb-0:3.3 -> desk   # my keyboard
input class:keyboard -> roaming
input * -> ignore
";

/// The lines of `a` and `b` that differ, as (line number, a's, b's).
fn changed(a: &str, b: &str) -> Vec<(usize, String, String)> {
    let a: Vec<&str> = a.split_inclusive('\n').collect();
    let b: Vec<&str> = b.split_inclusive('\n').collect();
    (0..a.len().max(b.len()))
        .filter_map(|i| {
            let (x, y) = (a.get(i).copied(), b.get(i).copied());
            (x != y).then(|| {
                (
                    i + 1,
                    x.unwrap_or("").to_owned(),
                    y.unwrap_or("").to_owned(),
                )
            })
        })
        .collect()
}

/// The rules of `text`, each as its line's text.
fn rules(text: &str) -> Vec<String> {
    let config = parse_sections(text).expect("valid").expect("a flow").config;
    let lines: Vec<&str> = text.lines().collect();
    config
        .rules
        .iter()
        .map(|r| lines.get(r.line - 1).copied().unwrap_or("").to_owned())
        .collect()
}

#[test]
fn a_look_s_key_is_set_in_place_with_its_comment_kept() {
    let out = set_key(FILE, Of::Look("big"), "font-size", Some("16")).expect("the edit");
    assert_eq!(
        changed(FILE, &out),
        [(
            6,
            "font-size = 14   # the desk's\n".to_owned(),
            "font-size = 16   # the desk's\n".to_owned()
        )]
    );
    let login = set_key(FILE, Of::Head("desk"), "login", Some("/bin/login -p")).expect("the edit");
    // a key the head does not set goes directly below its heading
    assert_eq!(
        login,
        FILE.replace("[head desk]\n", "[head desk]\nlogin = /bin/login -p\n")
    );
    let again = set_key(&login, Of::Head("desk"), "login", Some("/bin/sh -l")).expect("the edit");
    assert_eq!(changed(&login, &again).len(), 1);
    let unset = set_key(FILE, Of::Look("big"), "font-name", None).expect("the edit");
    assert_eq!(
        changed(FILE, &unset),
        [(
            5,
            "font-name = monoid\n".to_owned(),
            "#font-name = monoid\n".to_owned()
        )]
    );
    assert_eq!(
        set_key(FILE, Of::Look("big"), "vtlook", None).expect("nothing to unset"),
        FILE
    );
}

#[test]
fn the_writer_refuses_what_vtium_would_refuse() {
    assert_eq!(
        set_key(FILE, Of::Look("big"), "font-frits", Some("/x")),
        Err(SectionEdit::BadKey("font-frits".to_owned()))
    );
    assert_eq!(
        set_key(FILE, Of::Head("desk"), "font-size", Some("3")),
        Err(SectionEdit::BadKey("font-size".to_owned()))
    );
    assert_eq!(
        set_key(FILE, Of::Look("small"), "font-size", Some("3")),
        Err(SectionEdit::NoSection("small".to_owned()))
    );
    for bad in ["a #b", "two\nlines", ""] {
        assert!(
            matches!(
                set_key(FILE, Of::Look("big"), "font-name", Some(bad)),
                Err(SectionEdit::BadValue { .. })
            ),
            "{bad:?}"
        );
    }
    // a value out of bounds would drop the look, so nothing is handed back
    match set_key(FILE, Of::Look("big"), "font-size", Some("9999")) {
        Err(SectionEdit::Refused(r)) => assert!(matches!(
            r.errors.first().map(|e| &e.problem),
            Some(Problem::OutOfRange { .. })
        )),
        other => panic!("{other:?}"),
    }
    // sections vtium reads only in part are not the writer's to edit
    let broken = FILE.replace("look = big", "look = big\nfrob = 1");
    assert!(matches!(
        set_key(&broken, Of::Look("big"), "font-size", Some("16")),
        Err(SectionEdit::Unreadable(_))
    ));
    assert!(matches!(
        add_rule(&broken, "input * -> desk", None),
        Err(SectionEdit::Unreadable(_))
    ));
}

/// Where each new rule goes among `FILE`'s five rules: before the rule of
/// this number, 6 for after the last.
#[test]
fn a_new_rule_goes_where_what_it_names_puts_it() {
    let table: [(&str, usize); 8] = [
        // a rule naming one device goes before the first general rule of its kind
        ("input serial:046d:c31c:ABC -> desk", 4),
        ("input name:Logitech* -> desk", 4),
        ("display connector:card0/DP-1 -> desk", 2),
        ("display connector-path:mst:1-8 -> off", 2),
        // a general rule goes before the first `*` of its kind
        ("input class:mouse -> roaming", 5),
        ("input * -> roaming", 5),
        ("display card:pci-0000:03:00.0 -> desk", 2),
        ("input id:046d:c31c -> roaming", 5),
    ];
    let config = parse_sections(FILE).expect("valid").expect("a flow").config;
    for (rule, before) in table {
        let out = add_rule(FILE, rule, None).expect(rule);
        let mut want = rules(FILE);
        want.insert(before - 1, rule.to_owned());
        assert_eq!(rules(&out), want, "{rule}");
        assert_eq!(out.lines().count(), FILE.lines().count() + 1, "{rule}");
        let (kind, matcher) = rule_of(rule);
        assert_eq!(
            placement(&config.rules, kind, &matcher),
            before - 1,
            "{rule}"
        );
    }
    // with nothing of the sort to go before, a rule goes after the last
    let devices_only = "version = 1\n[head desk]\n[rules]\ninput path:a -> desk\n";
    let out = add_rule(devices_only, "input path:b -> desk", None).expect("the edit");
    assert_eq!(
        rules(&out),
        ["input path:a -> desk", "input path:b -> desk"]
    );
    let out = add_rule(devices_only, "display * -> desk", None).expect("the edit");
    assert_eq!(rules(&out), ["input path:a -> desk", "display * -> desk"]);
}

/// The kind and match of a rule's text, read through a file that holds it.
fn rule_of(rule: &str) -> (Kind, vtium_settings::Match) {
    let text = format!("version = 1\n[head desk]\n[rules]\n{rule}\n");
    let config = parse_sections(&text)
        .expect("valid")
        .expect("a flow")
        .config;
    let r = config.rules.into_iter().next().expect("the rule");
    (r.kind, r.matcher)
}

#[test]
fn a_rule_goes_before_the_rule_it_is_told_to() {
    let out = add_rule(FILE, "input * -> desk", Some(1)).expect("the edit");
    assert_eq!(
        rules(&out).first().map(String::as_str),
        Some("input * -> desk")
    );
    let out = add_rule(FILE, "input * -> desk", Some(6)).expect("the edit");
    assert_eq!(
        rules(&out).last().map(String::as_str),
        Some("input * -> desk")
    );
    assert_eq!(
        add_rule(FILE, "input * -> desk", Some(7)),
        Err(SectionEdit::NoRule(7))
    );
    assert_eq!(
        add_rule(FILE, "input * -> desk", Some(0)),
        Err(SectionEdit::NoRule(0))
    );
    assert!(matches!(
        add_rule(FILE, "input * desk", None),
        Err(SectionEdit::BadRule(_))
    ));
    assert!(matches!(
        add_rule(FILE, "input * -> nowhere", None),
        Err(SectionEdit::Refused(_))
    ));
}

#[test]
fn a_rule_is_replaced_moved_and_removed_on_its_own_line() {
    let out = set_rule(FILE, 3, "input path:pci-0000:15:00.0-usb-0:3.4 -> desk").expect("the edit");
    assert_eq!(
        changed(FILE, &out),
        [(
            14,
            "input path:pci-0000:15:00.0-usb-0:3.3 -> desk   # my keyboard\n".to_owned(),
            "input path:pci-0000:15:00.0-usb-0:3.4 -> desk   # my keyboard\n".to_owned()
        )]
    );
    let moved = move_rule(FILE, 5, 3).expect("the move");
    assert_eq!(
        rules(&moved),
        [
            "display edid:AAA:0x0001:LARGE00001 -> desk",
            "display * -> desk",
            "input * -> ignore",
            "input path:pci-0000:15:00.0-usb-0:3.3 -> desk   # my keyboard",
            "input class:keyboard -> roaming",
        ]
    );
    let back = move_rule(&moved, 3, 6).expect("the move");
    assert_eq!(back, FILE);
    assert_eq!(move_rule(FILE, 2, 3).expect("no move"), FILE);
    assert_eq!(move_rule(FILE, 6, 1), Err(SectionEdit::NoRule(6)));
    let removed = remove_rule(FILE, 1).expect("the removal");
    assert_eq!(changed(FILE, &removed).first().map(|c| c.0), Some(12));
    assert_eq!(rules(&removed).len(), 4);
    // removing the only rule naming a head is still a file vtium reads
    assert!(remove_rule(FILE, 3).is_ok());
}

#[test]
fn a_section_is_added_where_its_kind_goes() {
    let head = add_section(FILE, Of::Head("panel")).expect("the head");
    assert!(
        head.contains("look = big\n\n[head panel]\n\n[rules]\n"),
        "{head}"
    );
    let look = add_section(FILE, Of::Look("small")).expect("the look");
    assert!(
        look.contains("# the desk's\n\n[look small]\n\n[head desk]\n"),
        "{look}"
    );
    assert_eq!(
        add_section(FILE, Of::Head("desk")),
        Err(SectionEdit::Exists("desk".to_owned()))
    );
    for bad in ["roaming", "off", "focus", "a b", ""] {
        assert_eq!(
            add_section(FILE, Of::Head(bad)),
            Err(SectionEdit::BadName(bad.to_owned())),
            "{bad}"
        );
    }
    // a kmscon.conf gains its version with its first section
    let kmscon = "font-size=12\nbell\n";
    let out = add_section(kmscon, Of::Head("console")).expect("the head");
    assert_eq!(out, "font-size=12\nbell\nversion = 1\n\n[head console]\n");
    let out = add_rule(kmscon, "input * -> roaming", None).expect("the rule");
    assert_eq!(
        out,
        "font-size=12\nbell\nversion = 1\n[rules]\ninput * -> roaming\n"
    );
}
