//! Each row of the grammar: the line parses into the step it names, and
//! the step writes back as its canonical line.
//!
//! References:
//! - ADR-1208 D2: the grammar's rows and their examples.

use std::time::Duration;

use vtium_config_macro::{
    Character, Count, Expectation, Key, KeyPress, KeyStep, Label, Macro, Mods, Name, Node, Pause,
    Step, Text, parse,
};

/// The one step of a one-line macro.
fn one(line: &str) -> Step {
    let m = parse(line).unwrap_or_else(|e| panic!("{line:?}: {e}"));
    assert_eq!(m.steps().len(), 1, "{line:?}");
    m.steps()[0].clone()
}

/// The canonical line of a step.
fn line(step: &Step) -> String {
    step.to_string()
}

fn key(mods: Mods, key: Key, count: u8) -> Step {
    Step::Key(KeyStep::new(KeyPress { mods, key }, Count::new(count).unwrap()).unwrap())
}

fn ch(c: char) -> Key {
    Key::Char(Character::new(c).unwrap())
}

const CTRL: Mods = Mods {
    ctrl: true,
    alt: false,
    shift: false,
};
const ALT: Mods = Mods {
    ctrl: false,
    alt: true,
    shift: false,
};
const SHIFT: Mods = Mods {
    ctrl: false,
    alt: false,
    shift: true,
};

#[test]
fn every_named_key_parses_and_writes_back() {
    for (name, k) in Key::NAMED {
        assert_eq!(one(name), key(Mods::NONE, k, 1), "{name}");
        assert_eq!(line(&one(name)), name);
    }
    assert_eq!(Key::NAMED.len(), 25);
}

#[test]
fn esc_is_escape_and_writes_as_escape() {
    assert_eq!(one("Esc"), key(Mods::NONE, Key::Escape, 1));
    assert_eq!(line(&one("Esc")), "Escape");
}

#[test]
fn one_character_is_a_key() {
    assert_eq!(one("q"), key(Mods::NONE, ch('q'), 1));
    assert_eq!(one("é"), key(Mods::NONE, ch('é'), 1));
    assert_eq!(one("+"), key(Mods::NONE, ch('+'), 1));
    assert_eq!(one("\""), key(Mods::NONE, ch('"'), 1));
    assert_eq!(line(&one("q")), "q");
    // A byte-order mark is no key: the parser drops one at the start of a
    // file, so a key of it would not survive its own canonical line.
    assert!(Character::new('\u{feff}').is_none());
    assert!(parse("\u{feff}\u{feff}").is_err());
}

#[test]
fn keys_take_modifiers_in_any_combination() {
    assert_eq!(one("Ctrl+S"), key(CTRL, ch('S'), 1));
    assert_eq!(one("Alt+L"), key(ALT, ch('L'), 1));
    assert_eq!(one("Shift+Tab"), key(SHIFT, Key::Tab, 1));
    let all = Mods {
        ctrl: true,
        alt: true,
        shift: true,
    };
    assert_eq!(one("Ctrl+Alt+Shift+F5"), key(all, Key::F5, 1));
    // The macro may name them in any order, and the canonical line writes
    // Ctrl, Alt, Shift.
    assert_eq!(one("Shift+Alt+Ctrl+F5"), key(all, Key::F5, 1));
    assert_eq!(line(&one("Alt+Ctrl+X")), "Ctrl+Alt+X");
    // A + after the last modifier is the key itself.
    assert_eq!(one("Ctrl++"), key(CTRL, ch('+'), 1));
    // The crate keeps a key as written; the player maps Shift+a.
    assert_eq!(one("Shift+a"), key(SHIFT, ch('a'), 1));
    // A key the TUI may not read still parses; the player refuses it.
    assert_eq!(
        one("Ctrl+Alt+Up"),
        key(
            Mods {
                ctrl: true,
                alt: true,
                shift: false
            },
            Key::Up,
            1
        )
    );
    // # is a comment alone, and a key after a modifier.
    assert_eq!(one("Ctrl+#"), key(CTRL, ch('#'), 1));
}

#[test]
fn a_count_presses_the_key_that_many_times() {
    assert_eq!(one("Down 3"), key(Mods::NONE, Key::Down, 3));
    assert_eq!(one("Backspace 5"), key(Mods::NONE, Key::Backspace, 5));
    assert_eq!(one("Tab 2"), key(Mods::NONE, Key::Tab, 2));
    assert_eq!(one("x 100"), key(Mods::NONE, ch('x'), 100));
    assert_eq!(one("Ctrl+Z 4"), key(CTRL, ch('Z'), 4));
    assert_eq!(line(&one("Down 3")), "Down 3");
    // A count of 1 is the canonical line without it.
    assert_eq!(one("Down 1"), one("Down"));
    assert_eq!(line(&one("Down 1")), "Down");
}

#[test]
fn type_takes_double_or_single_quotes_and_no_escapes() {
    assert_eq!(
        one("Type \"cullet\""),
        Step::Type(Text::new("cullet").unwrap())
    );
    assert_eq!(
        one("Type 'say \"hi\"'"),
        Step::Type(Text::new("say \"hi\"").unwrap())
    );
    assert_eq!(one("Type \"it's\""), Step::Type(Text::new("it's").unwrap()));
    // A backslash is a character like any other.
    assert_eq!(one("Type \"a\\n\""), Step::Type(Text::new("a\\n").unwrap()));
    // Spaces inside the quotes are keys, kept as written.
    assert_eq!(
        one("Type \"  two  \""),
        Step::Type(Text::new("  two  ").unwrap())
    );
    // The canonical line uses double quotes unless the text holds one.
    assert_eq!(line(&one("Type 'cullet'")), "Type \"cullet\"");
    assert_eq!(line(&one("Type 'say \"hi\"'")), "Type 'say \"hi\"'");
}

#[test]
fn backticks_quote_a_text_as_in_vhs() {
    assert_eq!(one("Type `it's`"), Step::Type(Text::new("it's").unwrap()));
    assert_eq!(
        one("Type `say \"hi\", it's`"),
        Step::Type(Text::new("say \"hi\", it's").unwrap())
    );
    assert_eq!(
        one("Click `Font set`"),
        Step::Click(Label::new("Font set").unwrap())
    );
    assert_eq!(
        one("Expect Not `it's \"off\"`"),
        Step::Expect(Expectation::ShowsNot(Label::new("it's \"off\"").unwrap()))
    );
    // The canonical line uses backticks only for a text that holds both
    // other quotes.
    assert_eq!(line(&one("Type `cullet`")), "Type \"cullet\"");
    assert_eq!(line(&one("Type `say \"hi\"`")), "Type 'say \"hi\"'");
    assert_eq!(
        line(&one("Type `say \"hi\", it's`")),
        "Type `say \"hi\", it's`"
    );
    // A text that holds all three quotes cannot be written.
    assert!(Text::new("`'\"").is_none());
    assert!(Label::new("`'\"").is_none());
}

#[test]
fn click_takes_a_label() {
    assert_eq!(
        one("Click \"Looks\""),
        Step::Click(Label::new("Looks").unwrap())
    );
    assert_eq!(
        one("Click \"cullet-mono\""),
        Step::Click(Label::new("cullet-mono").unwrap())
    );
    assert_eq!(
        one("Click 'Font set'"),
        Step::Click(Label::new("Font set").unwrap())
    );
    assert_eq!(line(&one("Click 'Font set'")), "Click \"Font set\"");
}

#[test]
fn click_mouse_clicks_at_the_labels_cell() {
    assert_eq!(
        one("Click Mouse \"Save\""),
        Step::ClickMouse(Label::new("Save").unwrap())
    );
    assert_eq!(line(&one("Click   Mouse   'Save'")), "Click Mouse \"Save\"");
    // A label that reads Mouse is still a plain click when quoted.
    assert_eq!(
        one("Click \"Mouse\""),
        Step::Click(Label::new("Mouse").unwrap())
    );
}

#[test]
fn expect_checks_text_and_state() {
    assert_eq!(
        one("Expect \"11 pt\""),
        Step::Expect(Expectation::Shows(Label::new("11 pt").unwrap()))
    );
    assert_eq!(
        one("Expect Not \"problems\""),
        Step::Expect(Expectation::ShowsNot(Label::new("problems").unwrap()))
    );
    assert_eq!(
        one("Expect At looks.look"),
        Step::Expect(Expectation::At(Node::new("looks.look").unwrap()))
    );
    assert_eq!(
        one("Expect At fonts"),
        Step::Expect(Expectation::At(Node::new("fonts").unwrap()))
    );
    assert_eq!(
        one("Expect At any.question.system-font"),
        Step::Expect(Expectation::At(
            Node::new("any.question.system-font").unwrap()
        ))
    );
    assert_eq!(one("Expect Saved"), Step::Expect(Expectation::Saved));
    assert_eq!(one("Expect Unsaved"), Step::Expect(Expectation::Unsaved));
    assert_eq!(one("Expect Exited 0"), Step::Expect(Expectation::Exited(0)));
    assert_eq!(
        one("Expect Exited 255"),
        Step::Expect(Expectation::Exited(255))
    );
    for l in [
        "Expect \"11 pt\"",
        "Expect Not \"problems\"",
        "Expect At looks.look",
        "Expect Saved",
        "Expect Unsaved",
        "Expect Exited 2",
    ] {
        assert_eq!(line(&one(l)), l);
    }
}

#[test]
fn sleep_takes_milliseconds_or_seconds() {
    let ms = |n| Step::Sleep(Pause::from_millis(n).unwrap());
    assert_eq!(one("Sleep 500ms"), ms(500));
    assert_eq!(one("Sleep 2s"), ms(2000));
    assert_eq!(one("Sleep 60s"), ms(60_000));
    assert_eq!(one("Sleep 1ms"), ms(1));
    let Step::Sleep(p) = one("Sleep 2s") else {
        panic!()
    };
    assert_eq!(p.duration(), Duration::from_secs(2));
    // Whole seconds write in seconds, the rest in milliseconds.
    assert_eq!(line(&one("Sleep 2000ms")), "Sleep 2s");
    assert_eq!(line(&one("Sleep 1500ms")), "Sleep 1500ms");
}

#[test]
fn sleep_takes_a_bare_number_of_seconds_as_vhs_does() {
    let ms = |n| Step::Sleep(Pause::from_millis(n).unwrap());
    assert_eq!(one("Sleep 2"), ms(2000));
    assert_eq!(one("Sleep 0.5"), ms(500));
    assert_eq!(one("Sleep 0.5s"), ms(500));
    assert_eq!(one("Sleep 1.25"), ms(1250));
    assert_eq!(one("Sleep 0.001"), ms(1));
    assert_eq!(one("Sleep 60"), ms(60_000));
    assert_eq!(one("Sleep 59.999s"), ms(59_999));
    assert_eq!(line(&one("Sleep 2")), "Sleep 2s");
    assert_eq!(line(&one("Sleep 0.5")), "Sleep 500ms");
}

#[test]
fn screenshot_takes_a_file_name() {
    assert_eq!(
        one("Screenshot after-save"),
        Step::Screenshot(Name::new("after-save").unwrap())
    );
    assert_eq!(
        one("Screenshot v1.2_b"),
        Step::Screenshot(Name::new("v1.2_b").unwrap())
    );
    assert_eq!(line(&one("Screenshot after-save")), "Screenshot after-save");
    // A name may start like debug mode's own files without being one.
    assert!(Name::new("failures").is_some());
    assert!(Name::new("lively").is_some());
    assert!(Name::new("step").is_some());
    // Debug mode's own names are not screenshot names.
    for own in ["step-one", "step-0007", "step-", "failure", "live"] {
        assert!(Name::new(own).is_none(), "{own}");
    }
}

#[test]
fn set_live_comes_first() {
    assert_eq!(one("Set Live"), Step::SetLive);
    let m = parse("# a live run\n\nSet Live\nEnter\n").unwrap();
    assert_eq!(m.steps()[2], Step::SetLive);
    assert!(m.is_live());
    assert!(!parse("Enter\n").unwrap().is_live());
}

#[test]
fn comments_and_blank_lines_are_kept() {
    let m = parse("# the Looks tab\n\n   # indented\n \t \nEnter\n#\n").unwrap();
    let steps = m.steps();
    assert!(matches!(&steps[0], Step::Comment(c) if c.as_str() == " the Looks tab"));
    assert_eq!(steps[1], Step::Blank);
    assert!(matches!(&steps[2], Step::Comment(c) if c.as_str() == " indented"));
    assert_eq!(steps[3], Step::Blank);
    assert!(matches!(&steps[5], Step::Comment(c) if c.as_str().is_empty()));
    assert_eq!(m.to_string(), "# the Looks tab\n\n# indented\n\nEnter\n#\n");
    // A comment keeps its words as written, a tab and trailing spaces too.
    assert!(matches!(one("#\tx  "), Step::Comment(c) if c.as_str() == "\tx  "));
    // A count or a step after # is part of the comment.
    assert!(matches!(one("# 3"), Step::Comment(_)));
}

#[test]
fn numbered_gives_each_acting_step_its_line() {
    let m = parse("# c\n\nEnter\n# c\nDown 2\n").unwrap();
    let lines: Vec<usize> = m.numbered().map(|(n, _)| n).collect();
    assert_eq!(lines, [3, 5]);
}

#[test]
fn lines_take_spaces_and_tabs_around_words_and_crlf_endings() {
    assert_eq!(one("  Down \t 3  "), key(Mods::NONE, Key::Down, 3));
    assert_eq!(one("\tType \"a\"\t"), Step::Type(Text::new("a").unwrap()));
    let m = parse("Enter\r\nDown\r\n").unwrap();
    assert_eq!(
        m.steps(),
        [
            key(Mods::NONE, Key::Enter, 1),
            key(Mods::NONE, Key::Down, 1)
        ]
    );
    // A last line with no newline, and a byte-order mark on the first.
    assert_eq!(parse("\u{feff}Enter\nDown").unwrap().steps().len(), 2);
}

#[test]
fn an_empty_macro_parses_and_writes_nothing() {
    let m = parse("").unwrap();
    assert!(m.steps().is_empty());
    assert_eq!(m.to_string(), "");
    assert_eq!(parse("\n").unwrap().steps(), [Step::Blank]);
}

#[test]
fn the_macro_writes_one_line_for_each_step() {
    let text = "Set Live\nCtrl+Alt+X\nDown 3\nType 'say \"hi\"'\nClick \"Save\"\nClick Mouse \"Save\"\nExpect At looks.look\nSleep 500ms\nScreenshot a\n# c\n\n";
    let m: Macro = text.parse().unwrap();
    assert_eq!(m.to_string(), text);
}
