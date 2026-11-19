//! The Settings tab driven headlessly through the TUI's real loop: the
//! sections, their keys, the values offered as a list and a value typed
//! into the draft. `tui.rs` says how the tests drive the TUI, and
//! `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;

/// The keys that pick `word` from the highlighted key's list of values,
/// wherever the list opens.
fn pick_value(name: &str, word: &str) -> Vec<&'static [u8]> {
    let key = vtium_settings::keys::find(name).expect("a key");
    let at = vtium_config::tui::tabs::settings::fixed_values(key)
        .and_then(|words| words.iter().position(|w| *w == word))
        .unwrap_or_else(|| panic!("{name} does not take {word}"));
    let mut keys: Vec<&'static [u8]> = vec![b"\r", b"\x1b[H"];
    keys.extend(std::iter::repeat_n(DOWN, at));
    keys.push(b"\r");
    keys
}

/// The keys that go from the Settings view's list of sections to the
/// key `name`: the first section, Down to the key's section, Enter to open
/// it, and Down to the key. Esc closes the section again.
fn to_setting(name: &str) -> Vec<&'static [u8]> {
    use vtium_config::tui::tabs::settings::sections;
    let (section, key) = sections()
        .iter()
        .enumerate()
        .find_map(|(s, sec)| {
            sec.keys()
                .iter()
                .position(|k| k.name == name)
                .map(|k| (s, k))
        })
        .unwrap_or_else(|| panic!("no section holds {name}"));
    let mut keys: Vec<&'static [u8]> = vec![b"\x1b[H"];
    keys.extend(std::iter::repeat_n(DOWN, section));
    keys.push(b"\r");
    keys.extend(std::iter::repeat_n(DOWN, key));
    keys
}

/// An open Settings section and a key's list of values, at 80×25 and
/// 100×30, each a provisional golden until the maintainer reviews it.
#[test]
fn a_settings_section_and_a_list_of_values_at_80x25_and_100x30() {
    let scripts: [(&str, &[&[u8]]); 2] = [
        ("settings-section", &[b"\t", b"\r"]),
        ("settings-values", &[b"\t", b"\r", b"\x1b[F", b"\r"]),
    ];
    for (cols, rows) in [(80, 25), (100, 30)] {
        for (name, keys) in scripts {
            let (screen, bytes) = reach_from(model::View::Fonts, cols, rows, keys);
            let lines: Vec<&str> = screen.lines().collect();
            assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
            assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
            assert!(lines[rows - 1].contains(" quit"), "{name}: {screen}");
            golden(&format!("{name}-{cols}x{rows}"), &screen, &bytes);
        }
    }
}

/// The Settings view opens on the sections of vtium.conf's keys, each
/// with how many settings it holds. Enter opens a section, which shows its
/// keys under the titles of their groups, and Esc goes back to the
/// sections.
///
/// References:
/// - ADR-1201 D3, the Settings bullet: the sections come from the key
///   table, and Enter and Esc move between them and their keys.
#[test]
fn the_settings_view_opens_on_the_sections_and_enter_shows_a_section_s_groups() {
    let (screen, _) = reach_from(model::View::Fonts, 80, 25, &[b"\t"]);
    for title in ["Fonts and text", "Colors", "Keyboard and mouse"] {
        assert!(screen.contains(title), "{title}:\n{screen}");
    }
    assert!(screen.contains(" settings"), "{screen}");
    assert!(screen.contains(words::SECTIONS_HINT.split(' ').next().unwrap_or_default()));
    let (screen, _) = reach_from(model::View::Fonts, 80, 25, &[b"\t", b"\r"]);
    assert!(screen.contains("Settings › Fonts and text"), "{screen}");
    assert!(screen.contains(" Font\n"), "the group's title: {screen}");
    assert!(screen.contains("font-name"), "{screen}");
    assert!(!screen.contains("Colors"), "{screen}");
    let (screen, _) = reach_from(model::View::Fonts, 80, 25, &[b"\t", DOWN, b"\r", b"\x1b"]);
    assert!(screen.contains("Fonts and text"), "{screen}");
    assert!(!screen.contains("Settings ›"), "{screen}");
}

/// ADR-1005 D10: the settings view lists the seventeen keys vtium takes
/// for itself, each with its built-in default from the console's key
/// table, and shows a key's default on its row.
#[test]
fn the_settings_view_lists_vtiums_own_keys_with_their_built_in_defaults() {
    let want = [
        ("key-scroll-up", "<Shift>Up"),
        ("key-scroll-down", "<Shift>Down"),
        ("key-page-up", "<Shift>Prior"),
        ("key-page-down", "<Shift>Next"),
        ("key-prompt-up", "<Ctrl><Shift>Up"),
        ("key-prompt-down", "<Ctrl><Shift>Down"),
        ("key-zoom-in", "<Ctrl>plus,<Ctrl>equal"),
        ("key-zoom-out", "<Ctrl>minus"),
        ("key-rotate-cw", "<Logo>plus,<Logo>equal"),
        ("key-rotate-ccw", "<Logo>minus"),
        ("key-reset", "<Ctrl><Shift>Delete"),
        ("key-console-next", "<Ctrl><Logo>Right"),
        ("key-console-previous", "<Ctrl><Logo>Left"),
        ("key-console-open", "<Ctrl><Logo>Return"),
        ("key-console-close", "<Ctrl><Logo>BackSpace"),
        ("key-focus-next", "<Ctrl><Logo>Up"),
        ("key-focus-previous", "<Ctrl><Logo>Down"),
    ];
    let listed: Vec<&vtium_settings::keys::Key> = vtium_config::tui::tabs::settings::sections()
        .iter()
        .flat_map(|s| s.keys())
        .collect();
    for (name, default) in want {
        let key = listed
            .iter()
            .find(|k| k.name == name)
            .unwrap_or_else(|| panic!("{name} is not listed"));
        assert_eq!(
            vtium_config::commands::value_text(key, None),
            (default.to_owned(), false),
            "{name}"
        );
    }
    let mut keys: Vec<&[u8]> = vec![b"\t"];
    keys.extend(to_setting("key-reset"));
    let mut c = console().keys(&keys);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("key-reset"), "{screen}");
    assert!(screen.contains("<Ctrl><Shift>Delete (default)"), "{screen}");
    assert!(screen.contains("Reset the console on screen"), "{screen}");
}

/// ADR-1005 D10: in the settings view an empty value of a key binding is
/// D10's empty value, no key, written `key-reset=`; the editor starts from
/// the key's combinations, so Enter alone keeps them; and the word
/// `default` returns the key to its default.
#[test]
fn the_settings_view_empties_a_key_binding_and_default_returns_it() {
    let erase = [b"\x7f".as_slice(); 19];
    let mut keys: Vec<&[u8]> = vec![b"\t"];
    keys.extend(to_setting("key-reset"));
    keys.push(b"\r");
    keys.extend(erase);
    keys.push(b"\r");
    let mut d = desk();
    let mut c = console().keys(&[keys.as_slice(), &[b"q".as_slice(), b"y"]].concat());
    assert_eq!(run_desk(&mut c, &mut d), 0, "{:?}", c.seen);
    assert!(d.draft.contains("\nkey-reset=\n"), "{}", d.draft);
    // Enter again opens the empty value, and `default` returns the key to
    // its default: its line is commented out
    let mut c = console().keys(
        &[
            keys.as_slice(),
            &[b"\r".as_slice(), b"default", b"\r", b"q", b"y"],
        ]
        .concat(),
    );
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0, "{:?}", c.seen);
    assert!(d.draft.contains("\n#key-reset=\n"), "{}", d.draft);
    // Enter alone keeps the default's combinations as they are
    let mut c =
        console().keys(&[&keys[..keys.len() - 20], &[b"\r".as_slice(), b"q", b"y"]].concat());
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(!d.draft.contains("key-reset="), "{}", d.draft);
}

#[test]
fn a_setting_shows_its_default_and_what_it_does() {
    let mut keys: Vec<&[u8]> = vec![b"\t"];
    keys.extend(to_setting("font-size"));
    let mut c = console().keys(&keys);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("font-size"), "{screen}");
    assert!(screen.contains("0 (default)"), "{screen}");
    assert!(screen.contains("The font size in points"), "{screen}");
}

/// Enter types a value for a key that takes one, and a second Enter
/// keeps it. Enter on a switch or a choice opens its values as a list, and
/// Enter there picks one. Each change goes into the draft at once, and
/// typed text takes `q` and `i` as letters.
#[test]
fn a_typed_value_a_switch_and_a_choice_go_into_the_draft() {
    let mut d = desk();
    let mut keys: Vec<&[u8]> = vec![b"\t"];
    keys.extend(to_setting("font-hinting"));
    keys.extend(pick_value("font-hinting", "light"));
    keys.push(b"\x1b");
    keys.extend(to_setting("font-line-spacing"));
    keys.extend([b"\r".as_slice(), b"10", b"\r", b"\x1b"]);
    keys.extend(to_setting("allow-font-ops"));
    keys.extend(pick_value("allow-font-ops", "off"));
    keys.push(b"\x1b");
    keys.extend(to_setting("display-dpi"));
    keys.extend([b"\r".as_slice(), b"qi", b"\r", b"q", b"y"]);
    let mut c = console().keys(&keys);
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert_eq!(
        d.draft,
        // a new key goes after the last line of the top level, never into a
        // section (ADR-1403 D8)
        "# mine\nfont-name=unifont\nversion = 1\nfont-hinting=light\nfont-line-spacing=10\nno-allow-font-ops\ndisplay-dpi=qi\n[look proof]\nfont-name = proof\n[head desk]\nlook = proof\n[head panel]\n[rules]\ndisplay * -> desk\n"
    );
}

#[test]
fn tab_does_nothing_while_a_value_is_typed_in_settings() {
    // Settings, down to font-line-spacing, Enter starts typing its value;
    // Tab and Shift-Tab between the two digits change nothing
    let mut keys: Vec<&[u8]> = vec![TAB];
    keys.extend(to_setting("font-line-spacing"));
    keys.extend([
        b"\r".as_slice(),
        b"1",
        TAB,
        BACKTAB,
        b"0",
        b"\r",
        b"q",
        b"y",
    ]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(d.draft.contains("\nfont-line-spacing=10\n"), "{}", d.draft);
}

#[test]
fn an_edit_vtium_conf_cannot_take_is_said_in_words() {
    use vtium_config::tui::words::conf_edit;
    use vtium_settings::ConfEdit;
    let key = conf_edit(&ConfEdit::BadKey("Font".into()));
    assert!(key.contains("Font") && !key.contains("BadKey"), "{key}");
    let value = conf_edit(&ConfEdit::BadValue {
        key: "font-name".into(),
        value: "a#b".into(),
    });
    assert!(
        value.contains("font-name") && value.contains("a#b") && !value.contains("BadValue"),
        "{value}"
    );
}
