//! The VTs a head runs on, its `vt` key (ADR-1403 D4), and where each
//! device lands on each VT by the four steps of ADR-1401 D3's placement
//! procedure. The tests named `path_test_N` are ADR-1403 D7's path tests,
//! each checked at the level of vtium-settings: where the rules place each
//! device on each VT.

use vtium_settings::{
    By, Class, Config, Disposition, Edid, Identity, Kind, Place, Placed, Problem, Scope, VT_MAX,
    parse, reading, set_key,
};

fn config(text: &str) -> Config {
    match parse(text) {
        Ok(p) => p.config,
        Err(e) => panic!("{e:?}\n{text}"),
    }
}

/// `text` with `policy = closed` among its top-level keys.
fn closed(text: &str) -> String {
    format!("policy = closed\n{text}")
}

fn keyboard(path: &str) -> Identity {
    let mut d = Identity::input()
        .with_class(Class::Keyboard)
        .with_class(Class::Key);
    d.path = Some(path.to_owned());
    d
}

fn monitor(connector: &str, serial: &str) -> Identity {
    let mut d = Identity::display();
    d.card = Some("card0".to_owned());
    d.connector = Some(connector.to_owned());
    d.edid = Some(Edid {
        maker: "AAA".to_owned(),
        product: 1,
        serial: serial.to_owned(),
    });
    d
}

/// The index of the head named `name`.
fn head(config: &Config, name: &str) -> Place {
    match config.heads.iter().position(|h| h.name == name) {
        Some(i) => Place::Head(i),
        None => panic!("no head {name}"),
    }
}

fn elsewhere(config: &Config, name: &str) -> Place {
    match head(config, name) {
        Place::Head(i) => Place::Elsewhere(i),
        other => other,
    }
}

/// Where `id` lands on VT `vt`, before the guards.
fn on(config: &Config, id: &Identity, vt: u16) -> Place {
    config.walk_on(id, vt).place
}

// covers: platform.heads
#[test]
fn a_heads_vt_names_one_vt_or_several_and_none_means_every_vt() {
    // a tab between two numbers is a blank, as a space is
    let c = config(
        "version = 1\n[head one]\nvt = 3\n[head two]\nvt = 4\t 6\n[head every]\n[head ends]\nvt = 1 63\n",
    );
    let names: Vec<(&str, &[u16])> = c
        .heads
        .iter()
        .map(|h| (h.name.as_str(), h.vts.as_slice()))
        .collect();
    assert_eq!(
        names,
        [
            ("one", &[3][..]),
            ("two", &[4, 6][..]),
            ("every", &[][..]),
            ("ends", &[1, 63][..]),
        ]
    );
    let every = &c.heads[2];
    assert!((1..=VT_MAX).all(|vt| every.runs_on(vt)));
    assert!((1..=VT_MAX).all(|vt| !every.names_vt(vt)));
    let two = &c.heads[1];
    assert!(two.runs_on(4) && two.runs_on(6) && !two.runs_on(5));
}

/// A `vt` line, a test of the error it holds, and the error's message.
type Case = (&'static str, fn(&Problem) -> bool, &'static str);

// covers: platform.heads
#[test]
fn a_bad_vt_is_an_error_at_its_line_that_drops_the_head_with_its_rules() {
    let cases: [Case; 7] = [
        (
            "vt = 0",
            |p| matches!(p, Problem::BadVt(v) if v.raw() == "0"),
            "This head's `vt` names \"0\", which is not a VT. A VT is a number from 1 to 63, and `vt = 3 4` names two of them.",
        ),
        (
            "vt = 64",
            |p| matches!(p, Problem::BadVt(v) if v.raw() == "64"),
            "This head's `vt` names \"64\", which is not a VT. A VT is a number from 1 to 63, and `vt = 3 4` names two of them.",
        ),
        (
            "vt = 3 tty4",
            |p| matches!(p, Problem::BadVt(v) if v.raw() == "tty4"),
            "This head's `vt` names \"tty4\", which is not a VT. A VT is a number from 1 to 63, and `vt = 3 4` names two of them.",
        ),
        (
            "vt = 3,4",
            |p| matches!(p, Problem::BadVt(v) if v.raw() == "3,4"),
            "This head's `vt` names \"3,4\", which is not a VT. A VT is a number from 1 to 63, and `vt = 3 4` names two of them.",
        ),
        (
            "vt = 99999999999999999999",
            |p| matches!(p, Problem::BadVt(_)),
            "This head's `vt` names \"99999999999999999999\", which is not a VT. A VT is a number from 1 to 63, and `vt = 3 4` names two of them.",
        ),
        (
            "vt =",
            |p| matches!(p, Problem::BadVt(v) if v.raw().is_empty()),
            "This head's `vt` names no VT. Give it a VT number from 1 to 63, or remove the line so that the head runs on every VT.",
        ),
        (
            "vt = 3 4 3",
            |p| matches!(p, Problem::RepeatedVt(3)),
            "This head's `vt` names VT 3 twice. Remove one of the two.",
        ),
    ];
    for (line, problem, message) in cases {
        let text = format!(
            "version = 1\n[head a]\n{line}\n[head b]\n[rules]\ndisplay * -> a\ninput * -> b\n"
        );
        let r = reading(text.as_bytes());
        let errors: Vec<_> = r.errors.errors.iter().collect();
        assert_eq!(errors.len(), 1, "{line}: {errors:?}");
        let error = errors[0];
        assert_eq!(error.line, Some(3), "{line}");
        assert!(problem(&error.problem), "{line}: {:?}", error.problem);
        assert_eq!(error.to_string(), message, "{line}");
        assert_eq!(r.scopes, [Scope::Head], "{line}");
        // the head goes with the rule naming it; head b and its rule stay
        let c = &r.parsed.config;
        let heads: Vec<&str> = c.heads.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(heads, ["b"], "{line}");
        assert_eq!(c.rules.len(), 1, "{line}");
        assert_eq!(c.rules[0].kind, Kind::Input, "{line}");
    }
}

// covers: platform.heads
#[test]
fn a_heads_vt_is_its_own_key_and_set_once() {
    // the head's `vt` is not the top level's `vt`, the VT vtium opens, so
    // it sets nothing vtium reads as that key
    let c = config("version = 1\n[head a]\nvt = 3 4\n");
    assert!(c.heads[0].settings.is_empty(), "{:?}", c.heads[0].settings);
    assert_eq!(c.heads[0].vts, [3, 4]);
    let twice = reading(b"version = 1\n[head a]\nvt = 3\nvt = 4\n");
    assert!(
        twice
            .errors
            .errors
            .iter()
            .any(|e| e.line == Some(4) && e.problem == Problem::DuplicateKey("vt")),
        "{:?}",
        twice.errors
    );
    let look = reading(b"version = 1\n[look big]\nvt = 3\n");
    assert!(
        look.errors
            .errors
            .iter()
            .any(|e| e.line == Some(3) && matches!(e.problem, Problem::NotHere { key: "vt", .. })),
        "{:?}",
        look.errors
    );
}

#[test]
fn the_section_writer_writes_vt_and_leaves_its_line_byte_for_byte() {
    use vtium_settings::Of;
    let text = "version = 1\n[head a]\nvt   =  3\t4   # the desk's VTs\nconsoles = 1\n";
    // an edit of another key leaves the `vt` line as it is
    let edited = set_key(text, Of::Head("a"), "consoles", Some("2")).expect("consoles");
    assert_eq!(
        edited,
        "version = 1\n[head a]\nvt   =  3\t4   # the desk's VTs\nconsoles = 2\n"
    );
    // setting `vt` replaces its value and keeps the blanks and the comment
    let set = set_key(text, Of::Head("a"), "vt", Some("5 6")).expect("vt");
    assert_eq!(
        set,
        "version = 1\n[head a]\nvt   =  5 6   # the desk's VTs\nconsoles = 1\n"
    );
    assert_eq!(config(&set).heads[0].vts, [5, 6]);
    // a head with no `vt` gains the line below its heading
    let added = set_key("version = 1\n[head b]\n", Of::Head("b"), "vt", Some("3")).expect("add");
    assert_eq!(added, "version = 1\n[head b]\nvt = 3\n");
    // and unsetting comments it out, so the head runs on every VT again
    let unset = set_key(&set, Of::Head("a"), "vt", None).expect("unset");
    assert!(config(&unset).heads[0].vts.is_empty(), "{unset}");
}

/// A walk that names no VT keeps today's placement: every head runs on
/// every VT, whatever its `vt` says.
#[test]
fn a_walk_that_names_no_vt_reads_every_head_as_running() {
    let c = config(
        "version = 1\n[head three]\nvt = 3\n[head four]\nvt = 4\n[rules]\ndisplay * -> three\ninput * -> four\n",
    );
    let dp = monitor("DP-8", "S1");
    let kb = keyboard("usb-0:1");
    assert_eq!(c.walk(&dp).place, head(&c, "three"));
    assert_eq!(c.walk(&kb).place, head(&c, "four"));
}

/// Test 1 of ADR-1403 D7: one display, one keyboard, three terminals.
const THREE_TERMINALS: &str = "\
version = 1
[head a]
vt = 3
[head b]
vt = 4
[head c]
vt = 5
[rules]
display edid:AAA:0x0001:S1 -> a
input path:usb-0:1 -> a
display edid:AAA:0x0001:S1 -> b
input path:usb-0:1 -> b
display edid:AAA:0x0001:S1 -> c
input path:usb-0:1 -> c
";

// covers: platform.heads.rules
#[test]
fn path_test_1_one_display_and_one_keyboard_serve_a_terminal_on_each_vt() {
    let c = config(THREE_TERMINALS);
    let dp = monitor("DP-8", "S1");
    let kb = keyboard("usb-0:1");
    // step 3 notes head a's rules on VT 4, and step 2 places by head b's
    assert_eq!(
        c.walk_on(&dp, 4),
        Placed {
            place: head(&c, "b"),
            by: By::Rule(11),
        }
    );
    assert_eq!(
        c.walk_on(&kb, 4),
        Placed {
            place: head(&c, "b"),
            by: By::Rule(12),
        }
    );
    for (vt, name) in [(3, "a"), (4, "b"), (5, "c")] {
        assert_eq!(on(&c, &dp, vt), head(&c, name), "VT {vt}");
        assert_eq!(on(&c, &kb, vt), head(&c, name), "VT {vt}");
        let placement = c.place_on(&[dp.clone(), kb.clone()], None, vt);
        assert_eq!(placement.focus, Some(head(&c, name)), "VT {vt}");
        assert!(!placement.anti_lockout && !placement.never_dark, "VT {vt}");
    }
}

// covers: platform.heads.rules
#[test]
fn path_test_2_a_switch_moves_each_display_to_its_own_terminal_on_the_new_vt() {
    let c = config(
        "\
version = 1
[head left3]
vt = 3
[head right3]
vt = 3
[head left4]
vt = 4
[head right4]
vt = 4
[rules]
display connector:card0/DP-1 -> left3
display connector:card0/DP-2 -> right3
display connector:card0/DP-1 -> left4
display connector:card0/DP-2 -> right4
input * -> roaming
",
    );
    let left = monitor("DP-1", "L");
    let right = monitor("DP-2", "R");
    // step 1: the left display's rules never match the right display
    assert_eq!(on(&c, &left, 3), head(&c, "left3"));
    assert_eq!(on(&c, &right, 3), head(&c, "right3"));
    // steps 3 and 2: on VT 4, the VT 3 rules are noted and read past
    assert_eq!(on(&c, &left, 4), head(&c, "left4"));
    assert_eq!(on(&c, &right, 4), head(&c, "right4"));
}

// covers: platform.heads.rules
#[test]
fn path_test_3_a_terminal_with_no_vt_shows_on_every_vt() {
    let c = config(
        "version = 1\n[head both]\n[rules]\ndisplay * -> both\ninput class:keyboard -> roaming\n",
    );
    let left = monitor("DP-1", "L");
    let right = monitor("DP-2", "R");
    let kb = keyboard("usb-0:1");
    for vt in [1, 3, 4, VT_MAX] {
        assert_eq!(on(&c, &left, vt), head(&c, "both"), "VT {vt}");
        assert_eq!(on(&c, &right, vt), head(&c, "both"), "VT {vt}");
        assert_eq!(on(&c, &kb, vt), Place::Roaming, "VT {vt}");
        let placement = c.place_on(&[left.clone(), right.clone(), kb.clone()], None, vt);
        assert_eq!(placement.focus, Some(head(&c, "both")), "VT {vt}");
    }
}

// covers: platform.heads.rules
#[test]
fn path_test_4_each_keyboard_types_into_its_own_terminal_on_the_new_vt() {
    let c = config(
        "\
version = 1
[head left3]
vt = 3
[head right3]
vt = 3
[head left4]
vt = 4
[head right4]
vt = 4
[rules]
display connector:card0/DP-1 -> left3
display connector:card0/DP-2 -> right3
display connector:card0/DP-1 -> left4
display connector:card0/DP-2 -> right4
input path:usb-0:1 -> left3
input path:usb-0:2 -> right3
input path:usb-0:1 -> left4
input path:usb-0:2 -> right4
",
    );
    let a = keyboard("usb-0:1");
    let b = keyboard("usb-0:2");
    assert_eq!(on(&c, &a, 3), head(&c, "left3"));
    assert_eq!(on(&c, &b, 3), head(&c, "right3"));
    assert_eq!(on(&c, &a, 4), head(&c, "left4"));
    assert_eq!(on(&c, &b, 4), head(&c, "right4"));
}

/// A keyboard tied to a VT 3 terminal, and on VT 4 a terminal of its own
/// display that no keyboard reaches.
const TIED_KEYBOARD: &str = "\
version = 1
[head three]
vt = 3
[head four]
vt = 4
[rules]
display connector:card0/DP-1 -> three
display connector:card0/DP-1 -> four
input path:usb-0:1 -> three
";

// covers: platform.heads.rules
#[test]
fn path_test_5_a_keyboard_whose_only_terminal_is_on_another_vt_types_nowhere() {
    let c = config(TIED_KEYBOARD);
    let dp = monitor("DP-1", "L");
    let kb = keyboard("usb-0:1");
    // step 4 after step 3: the keyboard stays with its terminal
    assert_eq!(
        c.walk_on(&kb, 4),
        Placed {
            place: elsewhere(&c, "three"),
            by: By::Rule(9),
        }
    );
    // the policy does not place it, and no guard moves it to VT 4's head
    let placement = c.place_on(&[dp.clone(), kb.clone()], None, 4);
    assert_eq!(placement.placed[0].place, head(&c, "four"));
    assert_eq!(placement.placed[1].place, elsewhere(&c, "three"));
    assert!(!placement.anti_lockout);
    // under open, vtium watches it for the VT keys alone, and the rescue key
    // works
    let stays = placement.placed[1];
    assert_eq!(c.disposition(&stays, &kb), Disposition::RescueOnly);
    assert!(c.rescue(&stays));
    // under closed it stays closed, and the rescue key is off
    let shut = config(&closed(TIED_KEYBOARD));
    let stays = shut.walk_on(&kb, 4);
    assert_eq!(stays.place, elsewhere(&shut, "three"));
    assert_eq!(shut.disposition(&stays, &kb), Disposition::Closed);
    assert!(!shut.rescue(&stays));
    // on VT 3 it types into its terminal
    assert_eq!(on(&c, &kb, 3), head(&c, "three"));
}

// covers: platform.heads.rules
#[test]
fn path_test_6_a_display_whose_terminals_are_on_another_vt_shows_no_terminal() {
    let text = "\
version = 1
[head three]
vt = 3
[head four]
vt = 4
[rules]
display connector:card0/DP-1 -> three
display connector:card0/DP-2 -> four
input * -> roaming
";
    let left = monitor("DP-1", "L");
    let right = monitor("DP-2", "R");
    let kb = keyboard("usb-0:1");
    // on VT 4 the display of VT 3's terminal shows the notice that names
    // head three under open, and stays blank under closed
    for (c, left_on_4) in [
        (config(text), Place::Unused(Some(0))),
        (config(&closed(text)), Place::Elsewhere(0)),
    ] {
        let placement = c.place_on(&[left.clone(), right.clone(), kb.clone()], None, 4);
        assert_eq!(placement.placed[0].place, left_on_4);
        assert_eq!(placement.placed[1].place, head(&c, "four"));
        assert!(!placement.never_dark);
        let alone = c.place_on(&[left.clone(), kb.clone()], None, 4);
        assert_eq!(alone.placed[0].place, left_on_4);
        assert!(!alone.never_dark);
    }
}

/// Test 7 of ADR-1403 D7: a keyboard tied to VT 3, and a broad rule after
/// it.
const BROAD_AFTER: &str = "\
version = 1
[head three]
vt = 3
[head four]
vt = 4
[head every]
[rules]
display connector:card0/DP-1 -> three
display connector:card0/DP-1 -> four
input path:usb-0:1 -> three
input class:keyboard -> roaming
";

// covers: platform.heads.rules
#[test]
fn path_test_7_a_broad_rule_after_a_tied_keyboard_leaves_it_alone() {
    let c = config(BROAD_AFTER);
    let tied = keyboard("usb-0:1");
    let other = keyboard("usb-0:2");
    // the exception in step 2: `roaming` leaves the tied keyboard alone
    assert_eq!(on(&c, &tied, 4), elsewhere(&c, "three"));
    // a keyboard no head ties is the broad rule's
    assert_eq!(on(&c, &other, 4), Place::Roaming);
    // a head with no `vt`, `ignore` and `off` leave it alone too
    for target in ["every", "ignore"] {
        let text = BROAD_AFTER.replace("class:keyboard -> roaming", &format!("* -> {target}"));
        let c = config(&text);
        assert_eq!(on(&c, &tied, 4), elsewhere(&c, "three"), "{target}");
    }
    let dark =
        config(&BROAD_AFTER.replace("display connector:card0/DP-1 -> four", "display * -> off"));
    let dp = monitor("DP-1", "L");
    assert_eq!(on(&dark, &dp, 4), elsewhere(&dark, "three"));
    // only a rule whose head's `vt` names VT 4 places it there
    let named = config(&format!("{BROAD_AFTER}input * -> four\n"));
    assert_eq!(on(&named, &tied, 4), head(&named, "four"));
}

/// Test 8 of ADR-1403 D7: terminals on VT 3 and VT 4 that share DP-8 and
/// a keyboard.
fn shared_terminals() -> String {
    "\
version = 1
[head three]
vt = 3
[head four]
vt = 4
[rules]
display connector:card0/DP-8 -> three
input path:usb-0:1 -> three
display connector:card0/DP-8 -> four
input path:usb-0:1 -> four
"
    .to_owned()
}

// covers: platform.heads.rules
#[test]
fn path_test_8_two_terminals_share_a_display_and_a_keyboard() {
    let c = config(&shared_terminals());
    let dp = monitor("DP-8", "S");
    let kb = keyboard("usb-0:1");
    for (vt, name) in [(3, "three"), (4, "four")] {
        let placement = c.place_on(&[dp.clone(), kb.clone()], None, vt);
        let places: Vec<Place> = placement.placed.iter().map(|p| p.place).collect();
        assert_eq!(places, [head(&c, name), head(&c, name)], "VT {vt}");
        assert_eq!(placement.focus, Some(head(&c, name)), "VT {vt}");
    }
    // on a VT where neither runs, both stay with the first terminal noted
    assert_eq!(on(&c, &dp, 5), elsewhere(&c, "three"));
    assert_eq!(on(&c, &kb, 5), elsewhere(&c, "three"));
}

/// Only a head that runs on the VT placed for can be the active head.
#[test]
fn the_active_head_is_only_a_head_of_this_vt() {
    let c = config(
        "version = 1\n[head three]\nvt = 3\n[head four]\nvt = 4\n[rules]\ndisplay * -> four\ninput * -> roaming\n",
    );
    let dp = monitor("DP-1", "L");
    let kb = keyboard("usb-0:1");
    let placement = c.place_on(&[dp, kb], Some(head(&c, "three")), 4);
    assert_eq!(placement.placed[0].place, head(&c, "four"));
    assert_eq!(placement.focus, Some(head(&c, "four")));
}

// The maintainer's table (2026-11-10) decides what a connected display that
// no path catches on a VT shows. The tests below check each row.

/// desk runs on VT 3 and holds DP-8; here runs on VT 4 with no display
/// rule, and holds a keyboard.
const NO_DISPLAY_HERE: &str = "\
version = 1
[head desk]
vt = 3
[head here]
vt = 4
[rules]
display connector:card0/DP-8 -> desk
input path:usb-0:1 -> here
";

/// The places of `placement`'s devices, in order.
fn places(placement: &vtium_settings::Placement) -> Vec<Place> {
    placement.placed.iter().map(|p| p.place).collect()
}

/// Closed, with nothing defined: nothing shows on any display.
#[test]
fn table_closed_with_nothing_defined_leaves_every_display_blank() {
    let c = config(&closed("version = 1\n"));
    assert!(!c.uses_builtin_on(4));
    let placement = c.place_on(&[monitor("DP-1", "L"), keyboard("usb-0:1")], None, 4);
    assert_eq!(places(&placement), [Place::Off, Place::Ignore]);
    assert!(!placement.never_dark);
}

/// Closed, with some paths: a display no path catches on this VT stays
/// blank, whether it belongs to another VT or to no terminal.
#[test]
fn table_closed_with_some_paths_leaves_the_other_displays_blank() {
    let c = config(&closed(NO_DISPLAY_HERE));
    let dp8 = monitor("DP-8", "S");
    let dp9 = monitor("DP-9", "F");
    let placement = c.place_on(&[dp8.clone(), dp9.clone(), keyboard("usb-0:1")], None, 4);
    assert_eq!(
        places(&placement),
        [elsewhere(&c, "desk"), Place::Off, head(&c, "here")]
    );
    assert!(!placement.never_dark);
    // on VT 3, desk shows on DP-8, and DP-9 stays blank
    let three = c.place_on(&[dp8, dp9], None, 3);
    assert_eq!(places(&three), [head(&c, "desk"), Place::Off]);
}

/// Open, with nothing defined: the built-in default terminal on every
/// display, which every keyboard types into.
#[test]
fn table_open_with_nothing_defined_runs_the_built_in_default() {
    let c = config("version = 1\n");
    assert!(c.uses_builtin_on(4));
    assert_eq!(c.heads_on_vt(4), Vec::<usize>::new());
    let mut pointer = Identity::input().with_class(Class::Mouse);
    pointer.path = Some("usb-0:9".to_owned());
    let devices = [
        monitor("DP-1", "L"),
        monitor("DP-2", "R"),
        keyboard("usb-0:1"),
        pointer,
    ];
    let placement = c.place_on(&devices, None, 4);
    let got: Vec<(Place, By)> = placement.placed.iter().map(|p| (p.place, p.by)).collect();
    assert_eq!(
        got,
        [
            (Place::DefaultHead, By::Builtin),
            (Place::DefaultHead, By::Builtin),
            (Place::Roaming, By::Builtin),
            (Place::Roaming, By::Builtin),
        ]
    );
    assert_eq!(placement.focus, Some(Place::DefaultHead));
    assert!(!placement.never_dark && !placement.anti_lockout);
    // a file with a rule defines a path, so the built-in default does not run
    assert!(!config("version = 1\n[rules]\ninput * -> roaming\n").uses_builtin_on(4));
    assert!(!config(NO_DISPLAY_HERE).uses_builtin_on(5));
}

/// Open, with some paths: each display that no path catches on this VT
/// shows its own notice, naming the terminal and VT it is configured for,
/// or no terminal. No head's console shows on it.
#[test]
fn table_open_with_some_paths_gives_each_other_display_its_notice() {
    let c = config(NO_DISPLAY_HERE);
    let dp8 = monitor("DP-8", "S");
    let dp9 = monitor("DP-9", "F");
    let kb = keyboard("usb-0:1");
    let placement = c.place_on(&[dp8.clone(), dp9.clone(), kb.clone()], None, 4);
    assert_eq!(
        places(&placement),
        [
            Place::Unused(Some(0)),
            Place::Unused(None),
            head(&c, "here")
        ]
    );
    assert!(matches!(placement.placed[0].by, By::Rule(_)));
    assert_eq!(placement.placed[1].by, By::NoMatch);
    assert_eq!(placement.focus, None);
    assert!(!placement.never_dark);
    // a VT where no head runs shows the notices too
    let five = c.place_on(&[dp8.clone(), dp9.clone(), kb.clone()], None, 5);
    assert_eq!(
        places(&five),
        [
            Place::Unused(Some(0)),
            Place::Unused(None),
            elsewhere(&c, "here")
        ]
    );
    // a wildcard path to a head whose `vt` names VT 4 catches both
    // displays, DP-8 too by the exception in step 2
    let wild = config(&format!("{NO_DISPLAY_HERE}display * -> here\n"));
    let placement = wild.place_on(&[dp8, dp9, kb], None, 4);
    let here = head(&wild, "here");
    assert_eq!(places(&placement), [here, here, here]);
}

/// Either policy: a display a rule sends to `off` stays blank on every VT,
/// even where no head of the VT uses it; a path to Nowhere blanks a display
/// this way (the maintainer, 2026-11-10: "Off for every VT
/// (Recommended)").
#[test]
fn table_a_display_a_rule_turns_off_stays_blank_on_every_vt() {
    let text = "\
version = 1
[head here]
vt = 4
[rules]
display connector:card0/DP-1 -> off
display connector:card0/DP-2 -> here
input * -> here
";
    let off = monitor("DP-1", "L");
    let used = monitor("DP-2", "R");
    for c in [config(text), config(&closed(text))] {
        for vt in [3, 4] {
            let placement = c.place_on(&[off.clone(), used.clone()], None, vt);
            assert_eq!(placement.placed[0].place, Place::Off, "VT {vt}");
            assert!(matches!(placement.placed[0].by, By::Rule(_)), "VT {vt}");
        }
    }
    // a walk that names no VT never gives a notice
    let c = config(text);
    let placement = c.place(&[off, monitor("DP-3", "N")], None);
    assert!(
        placement
            .placed
            .iter()
            .all(|p| !matches!(p.place, Place::Unused(_)))
    );
}

/// [`TIED_KEYBOARD`] with `switch-on-enter = yes` on head three, under
/// `policy` and with head three's `isolated` as `isolated` says.
fn switching(policy: &str, isolated: &str, switch: &str) -> String {
    format!(
        "version = 1
policy = {policy}
[head three]
vt = 3
isolated = {isolated}
switch-on-enter = {switch}
[head four]
vt = 4
[rules]
display connector:card0/DP-1 -> three
display connector:card0/DP-1 -> four
input path:usb-0:1 -> three
"
    )
}

/// ADR-1401 D3, "Enter on a head's own keyboard": a head's
/// `switch-on-enter` is off unless the file sets it, and it switches only
/// under the open policy, for a head that is not isolated. A keyboard that
/// stays with such a head on another VT is opened for Enter and the VT
/// keys; one of a head that is not isolated is opened for the VT keys alone,
/// under either policy, and one of an isolated head under closed is never
/// opened.
#[test]
fn switch_on_enter_is_off_unless_set_and_switches_only_under_open_for_a_head_not_isolated() {
    let kb = keyboard("usb-0:1");
    let unset = config(TIED_KEYBOARD);
    assert!(!unset.heads.iter().any(|h| h.switch_on_enter));
    for (policy, isolated, switch, on, disposition) in [
        ("open", "no", "yes", true, Disposition::EnterAndRescue),
        ("open", "no", "no", false, Disposition::RescueOnly),
        ("open", "yes", "yes", false, Disposition::RescueOnly),
        // under closed a path places it, so it is opened for the VT keys
        // alone (D3, "Who switches VTs"), unless its head is isolated
        ("closed", "no", "yes", false, Disposition::RescueOnly),
        ("closed", "yes", "yes", false, Disposition::Closed),
    ] {
        let c = config(&switching(policy, isolated, switch));
        let three = c
            .heads
            .iter()
            .position(|h| h.name == "three")
            .expect("three");
        assert_eq!(
            c.switch_on_enter(three),
            on,
            "policy {policy}, isolated {isolated}, switch-on-enter {switch}"
        );
        let stays = c.walk_on(&kb, 4);
        assert_eq!(stays.place, Place::Elsewhere(three));
        assert_eq!(
            c.disposition(&stays, &kb),
            disposition,
            "policy {policy}, isolated {isolated}, switch-on-enter {switch}"
        );
    }
}

/// `switch-on-enter` takes yes or no; any other value is an error at its
/// line that drops the head (ADR-1401 D7).
#[test]
fn switch_on_enter_takes_yes_or_no() {
    let text = "version = 1\n[head a]\nvt = 3\nswitch-on-enter = maybe\n";
    let errors = reading(text.as_bytes()).errors;
    assert!(
        errors.errors.iter().any(|e| e.line == Some(4)
            && matches!(
                e.problem,
                Problem::BadValue {
                    key: "switch-on-enter",
                    ..
                }
            )),
        "{errors:?}"
    );
}
