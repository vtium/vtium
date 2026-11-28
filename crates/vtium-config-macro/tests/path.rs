//! The key path that names a failure: every key pressed from the start,
//! written in the macro's own syntax, so it plays back as a macro.
//!
//! References:
//! - ADR-1208 D6: the failure report and its keys from the start.

use vtium_config_macro::{
    Character, Count, Key, KeyPath, KeyPress, KeyStep, Label, Mods, Step, Text, failure_name,
    failure_report, parse,
};

fn ch(mods: Mods, c: char) -> KeyPress {
    KeyPress {
        mods,
        key: Key::Char(Character::new(c).unwrap()),
    }
}

const ALT: Mods = Mods {
    ctrl: false,
    alt: true,
    shift: false,
};

fn path(keys: &[KeyPress]) -> KeyPath {
    let mut p = KeyPath::new();
    for k in keys {
        p.push(*k);
    }
    p
}

fn record_path() -> KeyPath {
    path(&[
        ch(ALT, 'N'),
        KeyPress::plain(Key::Down),
        KeyPress::plain(Key::Down),
        KeyPress::plain(Key::Space),
    ])
}

#[test]
fn a_key_path_writes_its_keys_with_one_space_between() {
    assert_eq!(record_path().to_string(), "Alt+N Down Down Space");
    assert_eq!(record_path().keys().len(), 4);
    assert_eq!(KeyPath::new().to_string(), "");
}

#[test]
fn a_key_path_reads_back_from_its_text() {
    let p = record_path();
    assert_eq!(p.to_string().parse::<KeyPath>().unwrap(), p);
    let odd = path(&[
        ch(Mods::NONE, '#'),
        ch(Mods::NONE, '+'),
        ch(
            Mods {
                ctrl: true,
                alt: false,
                shift: false,
            },
            '+',
        ),
        KeyPress::plain(Key::Escape),
    ]);
    assert_eq!(odd.to_string(), "# + Ctrl++ Escape");
    assert_eq!(odd.to_string().parse::<KeyPath>().unwrap(), odd);
    assert_eq!("".parse::<KeyPath>().unwrap(), KeyPath::new());
    assert_eq!(
        "Up  Clik".parse::<KeyPath>().unwrap_err(),
        "Clik is not a key in a key path."
    );
    assert_eq!(
        "Up Ctrl+".parse::<KeyPath>().unwrap_err(),
        "Ctrl+ is not a key in a key path."
    );
    // A digit is a key of its own, not a count.
    assert_eq!("Down 3".parse::<KeyPath>().unwrap().keys().len(), 2);
}

#[test]
fn failure_name_reads_as_the_scenario_tests_name_one() {
    let p: KeyPath = "Up Up Enter Down r q l".parse().unwrap();
    assert_eq!(
        failure_name("vtium 80×25", &p),
        "vtium 80×25: Up Up Enter Down r q l"
    );
    assert_eq!(
        failure_name("vtium 80×25", &KeyPath::new()),
        "vtium 80×25: no keys"
    );
}

#[test]
fn failure_report_names_the_file_the_line_the_step_and_the_keys() {
    let step = Step::Expect(vtium_config_macro::Expectation::Shows(
        Label::new("always active").unwrap(),
    ));
    let report = failure_report(
        "refuse-builtin.vtmacro",
        6,
        &step,
        "the screen does not show it.",
        &record_path(),
    );
    assert_eq!(
        report,
        "refuse-builtin.vtmacro:6: Expect \"always active\": the screen does not show it.\nKeys from the start: Alt+N Down Down Space"
    );
    let none = failure_report(
        "a.vtmacro",
        1,
        &step,
        "the screen does not show it.",
        &KeyPath::new(),
    );
    assert!(none.ends_with("\nKeys from the start: none."), "{none}");
}

#[test]
fn a_key_path_plays_back_as_a_macro() {
    let p: KeyPath = "Alt+N Down Down Down # Space".parse().unwrap();
    let m = p.to_macro().unwrap();
    // Repeats become a count, and # alone types as text.
    assert_eq!(m.to_string(), "Alt+N\nDown 3\nType \"#\"\nSpace\n");
    let again = parse(&m.to_string()).unwrap();
    assert_eq!(again, m);
    // A run longer than the highest count splits.
    let long = path(&vec![KeyPress::plain(Key::Up); 250]);
    assert_eq!(
        long.to_macro().unwrap().to_string(),
        "Up 100\nUp 100\nUp 50\n"
    );
    let steps = long.to_macro().unwrap();
    assert_eq!(
        steps.steps()[2],
        Step::Key(KeyStep::new(KeyPress::plain(Key::Up), Count::new(50).unwrap()).unwrap())
    );
    assert!(matches!(&m.steps()[2], Step::Type(t) if *t == Text::new("#").unwrap()));
}

#[test]
fn a_key_path_too_long_for_one_macro_says_so() {
    // Alternating keys never merge, so each takes a line of its own.
    let mut p = KeyPath::new();
    for i in 0..=vtium_config_macro::bounds::STEPS_MAX {
        p.push(KeyPress::plain(if i % 2 == 0 {
            Key::Up
        } else {
            Key::Down
        }));
    }
    let e = p.to_macro().unwrap_err();
    assert_eq!(
        e.message(),
        "The macro has more than 10000 lines, the most a macro holds."
    );
}
