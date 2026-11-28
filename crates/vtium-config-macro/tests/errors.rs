//! Each error the parser reports: the first one only, on one line, naming
//! the line and what the line should hold.
//!
//! References:
//! - ADR-1208 D1: the limits on a line, a file, a count and a pause.
//! - ADR-1208 D2: the error that names the file, the line and what the
//!   line should hold, and Set Live as the first step.

use vtium_config_macro::bounds::{FILE_MAX, LINE_MAX, STEPS_MAX};
use vtium_config_macro::{Parser, parse, parse_bytes};

/// The line and the message of the first error in a macro.
fn err(text: &str) -> (usize, String) {
    err_bytes(text.as_bytes())
}

fn err_bytes(data: &[u8]) -> (usize, String) {
    match parse_bytes(data) {
        Ok(m) => panic!("{:?} parsed as {m:?}", String::from_utf8_lossy(data)),
        Err(e) => (e.line(), e.message().to_owned()),
    }
}

/// The message of the error in a one-line macro, checked to be on line 1.
fn msg(line: &str) -> String {
    let (n, m) = err(line);
    assert_eq!(n, 1, "{line:?}: {m}");
    m
}

const STEPS: &str = "A step is a key, Type, Click, Expect, Sleep, Screenshot or Set Live.";
const KEYS: &str = "A key is Enter, Escape, Tab, Space, Backspace, an arrow, PageUp, PageDown, Home, End, F1 to F12, or one character.";

#[test]
fn the_error_names_the_file_and_the_line() {
    let e = parse("Enter\n# two\nClik \"Save\"\nNope\n").unwrap_err();
    assert_eq!(e.line(), 3);
    assert_eq!(
        e.to_string(),
        format!("line 3: Clik is not a step. {STEPS}")
    );
    assert_eq!(
        e.in_file("cullet-11.vtmacro").to_string(),
        format!("cullet-11.vtmacro:3: Clik is not a step. {STEPS}")
    );
}

#[test]
fn an_unknown_word_is_not_a_step() {
    assert_eq!(msg("Clik"), format!("Clik is not a step. {STEPS}"));
    assert_eq!(msg("Enterr 3"), format!("Enterr is not a step. {STEPS}"));
    // A long word is cut short in the message, which stays one line.
    let long = "x".repeat(100);
    assert_eq!(
        msg(&long),
        format!("{}… is not a step. {STEPS}", "x".repeat(32))
    );
}

#[test]
fn a_word_in_the_wrong_case_says_so() {
    assert_eq!(
        msg("type \"a\""),
        "type is not a step. Steps and key names start with a capital letter, as in Type."
    );
    assert_eq!(
        msg("enter"),
        "enter is not a step. Steps and key names start with a capital letter, as in Enter."
    );
    assert_eq!(
        msg("ESC"),
        "ESC is not a step. Steps and key names start with a capital letter, as in Esc."
    );
}

#[test]
fn steps_of_vhs_that_macros_lack_do_not_parse() {
    for word in [
        "Output", "Require", "Hide", "Show", "Wait", "Source", "Env", "Copy", "Paste",
    ] {
        assert_eq!(
            msg(&format!("{word} x")),
            format!("{word} is a step of VHS's tapes that macros do not have. {STEPS}")
        );
    }
    assert_eq!(
        msg("Set FontSize 14"),
        "Set takes only Live in a macro, as in Set Live."
    );
    assert_eq!(
        msg("Set"),
        "Set takes only Live in a macro, as in Set Live."
    );
    assert_eq!(
        msg("Set Live now"),
        "Set takes only Live in a macro, as in Set Live."
    );
}

#[test]
fn a_key_name_must_be_one_the_format_has() {
    assert_eq!(
        msg("Delete"),
        format!("Delete is not a key that macros can press. {KEYS}")
    );
    assert_eq!(
        msg("Ctrl+Insert"),
        format!("Insert is not a key that macros can press. {KEYS}")
    );
    assert_eq!(msg("Ctrl+Foo"), format!("Foo is not a key. {KEYS}"));
    assert_eq!(msg("F13"), format!("F13 is not a step. {STEPS}"));
    assert_eq!(msg("Alt+F0"), format!("F0 is not a key. {KEYS}"));
}

#[test]
fn modifiers_need_a_key_and_come_once_each() {
    assert_eq!(
        msg("Ctrl"),
        "Ctrl is a modifier and needs a key after it, as in Ctrl+S."
    );
    assert_eq!(
        msg("Ctrl+"),
        "Ctrl+ has no key after its last +. Write a key after it, as in Ctrl+S."
    );
    assert_eq!(
        msg("Ctrl+Alt+"),
        "Ctrl+Alt+ has no key after its last +. Write a key after it, as in Ctrl+S."
    );
    assert_eq!(msg("Ctrl+Ctrl+S"), "Ctrl+Ctrl+S names Ctrl more than once.");
    assert_eq!(
        msg("Super+X"),
        "Super is not a modifier. A key takes Ctrl, Alt and Shift, as in Ctrl+S."
    );
    assert_eq!(
        msg("ctrl+X"),
        "ctrl is not a modifier. A key takes Ctrl, Alt and Shift, as in Ctrl+S."
    );
}

#[test]
fn a_count_runs_from_1_to_100() {
    let m = "A key takes only a count from 1 to 100 after it, as in Down 3.";
    for line in [
        "Down 0",
        "Down 101",
        "Down 3x",
        "Down -1",
        "Down +3",
        "Down 3 4",
        "Down 99999999999999999999",
        "Enter # a note",
    ] {
        assert_eq!(msg(line), m, "{line}");
    }
}

#[test]
fn quoted_text_must_be_quoted_closed_and_alone() {
    assert_eq!(
        msg("Type cullet"),
        "Type needs its text in quotes, as in Type \"cullet\"."
    );
    assert_eq!(
        msg("Type"),
        "Type needs its text in quotes, as in Type \"cullet\"."
    );
    assert_eq!(
        msg("Type \"cullet"),
        "The text after Type has no closing quote."
    );
    assert_eq!(
        msg("Type 'say \"hi\""),
        "The text after Type has no closing quote."
    );
    assert_eq!(
        msg("Type \"a\" \"b\""),
        "Only the text in quotes may follow Type."
    );
    assert_eq!(msg("Type \"\""), "The text after Type is empty.");
    assert_eq!(
        msg("Click Save"),
        "Click needs a label in quotes, as in Click \"Save\"."
    );
    assert_eq!(
        msg("Click"),
        "Click needs a label in quotes, as in Click \"Save\"."
    );
    assert_eq!(
        msg("Click Mouse Save"),
        "Click Mouse needs a label in quotes, as in Click Mouse \"Save\"."
    );
    assert_eq!(
        msg("Click \"Save\" now"),
        "Only the text in quotes may follow Click."
    );
    assert_eq!(msg("Click ''"), "The label after Click is empty.");
    assert_eq!(
        msg("Click Mouse ''"),
        "The label after Click Mouse is empty."
    );
    assert_eq!(msg("Expect ''"), "The text after Expect is empty.");
    assert_eq!(
        msg("Expect Not \"a"),
        "The text after Expect Not has no closing quote."
    );
}

#[test]
fn a_label_reads_as_the_screen_does() {
    let tail = "The screen is read with one space between words, so it would never match.";
    assert_eq!(
        msg("Click \" Save\""),
        format!(
            "The label after Click has a space at its start or end, or two spaces in a row. {tail}"
        )
    );
    assert_eq!(
        msg("Click Mouse \"Save \""),
        format!(
            "The label after Click Mouse has a space at its start or end, or two spaces in a row. {tail}"
        )
    );
    assert_eq!(
        msg("Expect \"11  pt\""),
        format!(
            "The text after Expect has a space at its start or end, or two spaces in a row. {tail}"
        )
    );
    assert_eq!(
        msg("Expect Not \"a\tb\""),
        "The text after Expect Not holds a tab. Write the words with one space between each."
    );
}

#[test]
fn type_text_holds_no_tab() {
    assert_eq!(
        msg("Type \"a\tb\""),
        "The text after Type holds a tab. Press Tab as a key step instead."
    );
}

#[test]
fn text_and_labels_have_a_longest() {
    let t = "a".repeat(1001);
    assert_eq!(
        msg(&format!("Type \"{t}\"")),
        "The text after Type is longer than 1000 characters, the most one step types."
    );
    assert!(parse(&format!("Type \"{}\"", "a".repeat(1000))).is_ok());
    let l = "a".repeat(257);
    assert_eq!(
        msg(&format!("Click \"{l}\"")),
        "The label after Click is longer than 256 characters, the most a label holds."
    );
    assert_eq!(
        msg(&format!("Expect \"{l}\"")),
        "The text after Expect is longer than 256 characters, the most a label holds."
    );
    assert!(parse(&format!("Click \"{}\"", "é".repeat(256))).is_ok());
}

#[test]
fn expect_takes_one_of_its_forms() {
    let forms = "Expect takes a text in quotes, Not and a text, At and a place, Saved, Unsaved, or Exited and a status, as in Expect \"11 pt\".";
    assert_eq!(msg("Expect"), forms);
    assert_eq!(msg("Expect Done"), forms);
    assert_eq!(
        msg("Expect Not"),
        "Expect Not needs a text in quotes, as in Expect Not \"problems\"."
    );
    assert_eq!(
        msg("Expect Not At fonts"),
        "Expect Not needs a text in quotes, as in Expect Not \"problems\"."
    );
    let at = "Expect At needs a place in the TUI, as in Expect At looks.look. A place is lowercase words joined by dots, at most 64 bytes.";
    for line in [
        "Expect At",
        "Expect At Looks",
        "Expect At looks..look",
        "Expect At .looks",
        "Expect At looks look",
    ] {
        assert_eq!(msg(line), at, "{line}");
    }
    assert_eq!(msg(&format!("Expect At {}", "a".repeat(65))), at);
    assert_eq!(
        msg("Expect Saved now"),
        "Expect Saved takes nothing after it."
    );
    assert_eq!(
        msg("Expect Unsaved 1"),
        "Expect Unsaved takes nothing after it."
    );
    let exited = "Expect Exited needs a status from 0 to 255, as in Expect Exited 0.";
    for line in [
        "Expect Exited",
        "Expect Exited 256",
        "Expect Exited -1",
        "Expect Exited 0 1",
        "Expect Exited x",
    ] {
        assert_eq!(msg(line), exited, "{line}");
    }
}

#[test]
fn sleep_lasts_from_1_ms_to_60_s() {
    let form = "Sleep takes a number of seconds or a whole number of milliseconds, as in Sleep 2, Sleep 0.5s or Sleep 500ms.";
    for line in [
        "Sleep",
        "Sleep 0.5ms",
        "Sleep .5",
        "Sleep 2.",
        "Sleep 1.2.3",
        "Sleep 2 s",
        "Sleep 2m",
        "Sleep ms",
        "Sleep 2s 3s",
        "Sleep -1s",
    ] {
        assert_eq!(msg(line), form, "{line}");
    }
    let long = "This Sleep lasts longer than 60 seconds, the longest a macro waits.";
    for line in [
        "Sleep 61s",
        "Sleep 61",
        "Sleep 60.001",
        "Sleep 60001ms",
        "Sleep 99999999999999999999s",
        "Sleep 99999999999999999999.5",
    ] {
        assert_eq!(msg(line), long, "{line}");
    }
    assert_eq!(
        msg("Sleep 0ms"),
        "This Sleep lasts no time. A Sleep lasts at least 1 ms."
    );
    for line in ["Sleep 0s", "Sleep 0", "Sleep 0.000"] {
        assert_eq!(
            msg(line),
            "This Sleep lasts no time. A Sleep lasts at least 1 ms.",
            "{line}"
        );
    }
    assert_eq!(
        msg("Sleep 0.0005"),
        "This Sleep is finer than a millisecond. Write at most three digits after the point, as in Sleep 0.25."
    );
}

#[test]
fn a_pace_after_a_steps_name_is_refused() {
    let tail = "A macro sets no speed for a step, because the player sets its own pace.";
    assert_eq!(
        msg("Type@500ms \"cullet\""),
        format!("Type@500ms gives the step a speed. {tail} Write Type without @500ms.")
    );
    assert_eq!(
        msg("Down@100ms 3"),
        format!("Down@100ms gives the step a speed. {tail} Write Down without @100ms.")
    );
    assert_eq!(
        msg("Ctrl+S@1s"),
        format!("Ctrl+S@1s gives the step a speed. {tail} Write Ctrl+S without @1s.")
    );
    // Ctrl and @ is a key, and @ alone is one too.
    assert!(parse("Ctrl+@\n@\n").is_ok());
}

#[test]
fn screenshot_names_a_file_of_its_own() {
    let form = "Screenshot needs a name for its file, as in Screenshot after-save. A name is up to 64 letters, digits, -, _ and ., and does not start with a dot.";
    for line in [
        "Screenshot",
        "Screenshot .hidden",
        "Screenshot a/b",
        "Screenshot ../x",
        "Screenshot a b",
        "Screenshot \"a\"",
        "Screenshot é",
    ] {
        assert_eq!(msg(line), form, "{line}");
    }
    assert_eq!(msg(&format!("Screenshot {}", "a".repeat(65))), form);
    let own = "would overwrite a file that debug mode writes itself. Choose another name.";
    assert_eq!(
        msg("Screenshot failure"),
        format!("Screenshot failure {own}")
    );
    for name in ["step-0007", "step-one", "step-", "live"] {
        assert_eq!(
            msg(&format!("Screenshot {name}")),
            format!("Screenshot {name} {own}")
        );
    }
    // A name with a character a name cannot hold gets the form, even when
    // it starts like debug mode's own.
    assert_eq!(msg("Screenshot step-../x"), form);
}

#[test]
fn a_screenshot_name_is_used_once() {
    assert_eq!(
        err("Screenshot a\nEnter\n# c\nScreenshot b\nScreenshot a\n"),
        (
            5,
            "Screenshot a uses the name of the Screenshot on line 1, so its file would overwrite that one. Choose another name.".to_owned()
        )
    );
    // A stream checks the names too.
    let mut p = Parser::new();
    p.line(b"Screenshot x").unwrap();
    assert_eq!(p.line(b"Screenshot x").unwrap_err().line(), 2);
}

#[test]
fn set_live_after_a_step_fails() {
    let m = "Set Live must be the first step. Only comments and blank lines may come before it.";
    assert_eq!(err("Enter\nSet Live\n"), (2, m.to_owned()));
    assert_eq!(err("# c\n\nSet Live\nSet Live\n"), (4, m.to_owned()));
}

#[test]
fn a_line_holds_no_control_character_and_is_utf8() {
    assert_eq!(
        msg("Enter\u{7}"),
        "This line holds a control character other than a tab."
    );
    assert_eq!(
        msg("# bell \u{7}"),
        "This line holds a control character other than a tab."
    );
    assert_eq!(
        msg("Type \"a\rb\""),
        "This line holds a control character other than a tab."
    );
    assert_eq!(
        err_bytes(b"Enter\nType \"\xff\"\n"),
        (2, "This line is not UTF-8 text.".to_owned())
    );
    // A byte-order mark counts only at the start of the file.
    assert_eq!(
        err("Enter\n\u{feff}Down\n").1,
        format!("\u{feff}Down is not a step. {STEPS}")
    );
}

#[test]
fn a_line_holds_at_most_line_max_bytes() {
    let ok = format!("#{}", "a".repeat(LINE_MAX - 1));
    assert!(parse(&ok).is_ok());
    let long = format!("Enter\n#{}\n", "a".repeat(LINE_MAX));
    assert_eq!(
        err(&long),
        (
            2,
            "This line is longer than 4096 bytes, the most a line holds.".to_owned()
        )
    );
    // A carriage return before the newline is not part of the line.
    assert!(parse(&format!("{ok}\r\n")).is_ok());
}

#[test]
fn a_macro_holds_at_most_steps_max_lines() {
    let ok = "\n".repeat(STEPS_MAX);
    assert_eq!(parse(&ok).unwrap().steps().len(), STEPS_MAX);
    let over = format!("{ok}Enter\n");
    assert_eq!(
        err(&over),
        (
            STEPS_MAX + 1,
            "The macro has more than 10000 lines, the most a macro holds.".to_owned()
        )
    );
}

#[test]
fn a_macro_holds_at_most_file_max_bytes() {
    // Lines of 4000 bytes and a newline each; the line that crosses the
    // bound is the one named.
    let row = format!("#{}\n", "a".repeat(3999));
    let rows = FILE_MAX / row.len();
    let mut text = row.repeat(rows);
    assert!(parse(&text).is_ok());
    text.push_str(&row);
    assert_eq!(
        err(&text),
        (
            rows + 1,
            "The macro is larger than 1 MiB, the most a macro holds.".to_owned()
        )
    );
    // A huge single line stops at the bound without reading the rest.
    let huge = vec![b'a'; 64 * FILE_MAX];
    assert_eq!(
        err_bytes(&huge),
        (
            1,
            "The macro is larger than 1 MiB, the most a macro holds.".to_owned()
        )
    );
}

#[test]
fn the_parser_reads_a_macro_line_by_line() {
    // As `--macro -` does: each line parses as it arrives, with its number.
    let mut p = Parser::new();
    assert!(p.line(b"# streamed").is_ok());
    assert!(p.line(b"Enter").is_ok());
    let e = p.line(b"Set Live").unwrap_err();
    assert_eq!(e.line(), 3);
    let e = p.line(b"Clik").unwrap_err();
    assert_eq!(
        e.to_string(),
        format!("line 4: Clik is not a step. {STEPS}")
    );
}
