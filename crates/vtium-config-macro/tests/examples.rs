//! The two example macros of the record parse, step for step, and write
//! back as the same text.
//!
//! References:
//! - ADR-1208 D2, "Two macros": the macro that sets Cullet Mono at 11 pt
//!   and the test that expects the built-in font's refusal.

use vtium_config_macro::{
    Character, Count, Expectation, Key, KeyPress, KeyStep, Label, Mods, Node, Step, parse,
};

const CULLET_11: &str = r#"# Make the console draw Cullet Mono at 11 pt, and save it.
# Run it with: vtium config tui --macro cullet-11.vtmacro --live
Set Live
Alt+L
Expect At looks
Click "console"
Enter
Click "Font set"
Enter
Click "cullet-mono"
Enter
Click "Size"
Enter
Click "11 pt"
Enter
Expect "cullet-mono"
Expect "11 pt"
Ctrl+S
Enter
Expect Saved
"#;

const REFUSE_BUILTIN: &str = r#"# The built-in font is always active, so Space on it changes nothing.
Alt+N
Expect At fonts
Click "built-in"
Space
Expect "always active"
Expect Saved
"#;

fn key(mods: Mods, key: Key) -> Step {
    Step::Key(KeyStep::new(KeyPress { mods, key }, Count::ONE).unwrap())
}

fn letter(mods: Mods, c: char) -> Step {
    key(mods, Key::Char(Character::new(c).unwrap()))
}

fn click(l: &str) -> Step {
    Step::Click(Label::new(l).unwrap())
}

fn shows(t: &str) -> Step {
    Step::Expect(Expectation::Shows(Label::new(t).unwrap()))
}

fn at(n: &str) -> Step {
    Step::Expect(Expectation::At(Node::new(n).unwrap()))
}

const ALT: Mods = Mods {
    ctrl: false,
    alt: true,
    shift: false,
};
const CTRL: Mods = Mods {
    ctrl: true,
    alt: false,
    shift: false,
};

#[test]
fn the_cullet_macro_parses_step_for_step() {
    let m = parse(CULLET_11).unwrap();
    let steps = m.steps();
    assert!(
        matches!(&steps[0], Step::Comment(c) if c.as_str() == " Make the console draw Cullet Mono at 11 pt, and save it.")
    );
    assert!(matches!(&steps[1], Step::Comment(_)));
    let enter = key(Mods::NONE, Key::Enter);
    let rest = [
        Step::SetLive,
        letter(ALT, 'L'),
        at("looks"),
        click("console"),
        enter.clone(),
        click("Font set"),
        enter.clone(),
        click("cullet-mono"),
        enter.clone(),
        click("Size"),
        enter.clone(),
        click("11 pt"),
        enter.clone(),
        shows("cullet-mono"),
        shows("11 pt"),
        letter(CTRL, 'S'),
        enter,
        Step::Expect(Expectation::Saved),
    ];
    assert_eq!(&steps[2..], rest);
    assert!(m.is_live());
    // Line 18 is the Ctrl+S that saves, the line a live run's journal names.
    assert_eq!(
        m.numbered().find(|(n, _)| *n == 18).map(|(_, s)| s.clone()),
        Some(letter(CTRL, 'S'))
    );
    assert_eq!(m.to_string(), CULLET_11);
}

#[test]
fn the_cullet_macro_without_set_live_rehearses() {
    let rehearsal = CULLET_11.replace("Set Live\n", "");
    let m = parse(&rehearsal).unwrap();
    assert!(!m.is_live());
    assert_eq!(m.steps().len(), 19);
}

#[test]
fn the_refusal_macro_parses_step_for_step() {
    let m = parse(REFUSE_BUILTIN).unwrap();
    let rest = [
        letter(ALT, 'N'),
        at("fonts"),
        click("built-in"),
        key(Mods::NONE, Key::Space),
        shows("always active"),
        Step::Expect(Expectation::Saved),
    ];
    assert_eq!(&m.steps()[1..], rest);
    assert!(!m.is_live());
    // The Expect that the record's failure report names is on line 6.
    assert_eq!(
        m.numbered().find(|(n, _)| *n == 6).map(|(_, s)| s.clone()),
        Some(shows("always active"))
    );
    assert_eq!(m.to_string(), REFUSE_BUILTIN);
}
