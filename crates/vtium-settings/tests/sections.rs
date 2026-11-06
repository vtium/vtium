//! vtium.conf's section writer (ADR-1403 D7, D8): each edit changes only
//! the lines it needs, edits a draft whatever the check says of it, and
//! refuses only what it cannot place; a new rule goes where what it names
//! puts it; a section is removed or renamed with what names it.

use vtium_settings::{
    Kind, Of, SectionEdit, add_rule, add_section, move_rule, parse_sections, placement,
    remove_rule, remove_section, rename_section, rule_lines, set_key, set_rule,
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

/// The rules of `text`, each as its line's text, as the writer numbers
/// them.
fn rules(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    rule_lines(text)
        .iter()
        .map(|n| lines.get(n - 1).copied().unwrap_or("").to_owned())
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
fn the_writer_refuses_only_what_it_cannot_place() {
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
    // a value out of bounds is written, and the check reports it
    let out = set_key(FILE, Of::Look("big"), "font-size", Some("9999")).expect("the edit");
    assert_eq!(
        changed(FILE, &out),
        [(
            6,
            "font-size = 14   # the desk's\n".to_owned(),
            "font-size = 9999   # the desk's\n".to_owned()
        )]
    );
    assert!(parse_sections(&out).is_err());
    // sections vtium reads only in part are edited all the same
    let broken = FILE.replace("look = big", "look = big\nfrob = 1");
    let out = set_key(&broken, Of::Look("big"), "font-size", Some("16")).expect("the edit");
    assert_eq!(changed(&broken, &out).len(), 1);
    let out = add_rule(&broken, "input * -> desk", None).expect("the edit");
    assert_eq!(rules(&out).len(), 6);
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
    // a rule may send to a head the draft does not declare yet
    let out = add_rule(FILE, "input * -> nowhere", None).expect("the edit");
    assert_eq!(
        rules(&out).get(4).map(String::as_str),
        Some("input * -> nowhere")
    );
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
    assert_eq!(
        moved,
        FILE.replace(
            "display * -> desk\ninput path",
            "display * -> desk\ninput * -> ignore\ninput path"
        )
        .replacen("roaming\ninput * -> ignore\n", "roaming\n", 1)
    );
    // a moved rule keeps its blanks and its comment
    let mine = "input path:pci-0000:15:00.0-usb-0:3.3 -> desk   # my keyboard\n";
    assert_eq!(
        move_rule(FILE, 3, 6).expect("the move"),
        FILE.replace(mine, "")
            .replace("input * -> ignore\n", &format!("input * -> ignore\n{mine}"))
    );
    let back = move_rule(&moved, 3, 6).expect("the move");
    assert_eq!(back, FILE);
    assert_eq!(move_rule(FILE, 2, 3).expect("no move"), FILE);
    assert_eq!(move_rule(FILE, 6, 1), Err(SectionEdit::NoRule(6)));
    let removed = remove_rule(FILE, 1).expect("the removal");
    assert_eq!(
        removed,
        FILE.replace("display edid:AAA:0x0001:LARGE00001 -> desk\n", "")
    );
    // removing the only rule naming a head is still a file vtium reads
    assert_eq!(
        remove_rule(FILE, 3).expect("the removal"),
        FILE.replace(
            "input path:pci-0000:15:00.0-usb-0:3.3 -> desk   # my keyboard\n",
            ""
        )
    );
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

// The writer finds `[rules]` as the reader reads a heading, so blanks
// inside the brackets name the same section.
#[test]
fn a_section_goes_before_rules_however_its_heading_is_spaced() {
    let text = "version = 1\n[head desk]\n[ rules ]\ndisplay * -> desk\n";
    assert_eq!(
        add_section(text, Of::Head("panel")).expect("the head"),
        "version = 1\n[head desk]\n[head panel]\n\n[ rules ]\ndisplay * -> desk\n"
    );
    let no_rules = "version = 1\n[head desk]\n";
    assert_eq!(
        add_rule(no_rules, "display * -> desk", None).expect("the rule"),
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n"
    );
    let spaced = "version = 1\n[head desk]\n[\trules ]\n";
    assert_eq!(
        add_rule(spaced, "display * -> desk", None).expect("the rule"),
        "version = 1\n[head desk]\n[\trules ]\ndisplay * -> desk\n"
    );
    // a file holding a policy line alone gains its version with a section
    assert_eq!(
        add_section("policy = closed\n", Of::Head("desk")).expect("the head"),
        "policy = closed\nversion = 1\n\n[head desk]\n"
    );
}

const TWO: &str = "\
version = 1

[look big]
font-name = monoid

[head desk]
look = big
consoles = 2

[head panel]
look = big

# Each display and each input tries these rules.
[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display * -> panel
input class:keyboard -> desk   # the keyboard
";

#[test]
fn a_removed_head_keeps_its_rules_and_the_comments_before_the_next_heading() {
    let out = remove_section(TWO, Of::Head("panel")).expect("the edit");
    assert_eq!(out, TWO.replace("[head panel]\nlook = big\n\n", ""));
    assert_eq!(rules(&out)[1], "display * -> panel");
    // the check reports the rule that now sends nowhere
    assert!(parse_sections(&out).is_err());
    // and the rule is still the writer's to fix
    let fixed = set_rule(&out, 2, "display * -> desk").expect("the edit");
    assert!(parse_sections(&fixed).is_ok());
    assert_eq!(
        remove_section(TWO, Of::Head("nobody")),
        Err(SectionEdit::NoSection("nobody".to_owned()))
    );
}

#[test]
fn a_removed_look_leaves_its_heads_naming_it() {
    let out = remove_section(TWO, Of::Look("big")).expect("the edit");
    assert_eq!(out, TWO.replace("[look big]\nfont-name = monoid\n\n", ""));
}

#[test]
fn a_renamed_head_takes_every_rule_that_sends_to_it() {
    let out = rename_section(TWO, Of::Head("desk"), "office").expect("the edit");
    assert_eq!(
        changed(TWO, &out),
        [
            (6, "[head desk]\n".to_owned(), "[head office]\n".to_owned()),
            (
                15,
                "display edid:AAA:0x0001:LARGE00001 -> desk\n".to_owned(),
                "display edid:AAA:0x0001:LARGE00001 -> office\n".to_owned()
            ),
            (
                17,
                "input class:keyboard -> desk   # the keyboard\n".to_owned(),
                "input class:keyboard -> office   # the keyboard\n".to_owned()
            ),
        ]
    );
    assert!(parse_sections(&out).is_ok());
    assert_eq!(
        rename_section(TWO, Of::Head("desk"), "panel"),
        Err(SectionEdit::Exists("panel".to_owned()))
    );
    assert_eq!(
        rename_section(TWO, Of::Head("desk"), "roaming"),
        Err(SectionEdit::BadName("roaming".to_owned()))
    );
}

#[test]
fn a_renamed_look_takes_every_head_that_names_it() {
    let out = rename_section(TWO, Of::Look("big"), "large").expect("the edit");
    assert_eq!(
        changed(TWO, &out),
        [
            (3, "[look big]\n".to_owned(), "[look large]\n".to_owned()),
            (7, "look = big\n".to_owned(), "look = large\n".to_owned()),
            (11, "look = big\n".to_owned(), "look = large\n".to_owned()),
        ]
    );
}

#[test]
fn a_section_named_twice_is_not_the_writer_s_to_guess() {
    let twice = TWO.replace("[head panel]", "[head desk]");
    assert_eq!(
        remove_section(&twice, Of::Head("desk")),
        Err(SectionEdit::Ambiguous("desk".to_owned()))
    );
}

#[test]
fn a_rule_vtium_drops_is_still_numbered() {
    // rule 2 sends to a head no section declares, so vtium drops it; the
    // writer numbers it all the same, as the commands do
    let text = TWO.replace("display * -> panel", "display * -> gone");
    assert_eq!(rule_lines(&text), [15, 16, 17]);
    let out = remove_rule(&text, 2).expect("the edit");
    assert_eq!(
        rules(&out),
        [
            "display edid:AAA:0x0001:LARGE00001 -> desk",
            "input class:keyboard -> desk   # the keyboard"
        ]
    );
}
