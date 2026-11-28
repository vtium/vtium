//! The Flow tab's editing, driven through its `Tab` methods on the screen
//! map's machine and on the large machine: a terminal's card and its list
//! box, NEW PATH's card, the part makers and pickers, a part's label, and
//! adding and revising a path. Each screen is drawn at 100×30 and 80×25,
//! in color and without, and kept as a snapshot. The commands a change
//! returns are applied as the shell applies them, and the vtium.conf they
//! write is compared with what today's Flow tab writes for the same keys.
//!
//! References:
//! - ADR-1403 D7, "Composing and adding a path", "Templates and the part
//!   makers", "Labels" and "Editing an added path, and Revise".
//! - ADR-1403 D8, "What the Flow view writes".
//! - ADR-1403 D11, "The list box".
//! - ADR-1206 D10: each screen at 100×30 and 80×25, readable without color.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]

mod common;

use common::*;
use ratatui_core::style::Modifier;
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Change, Command, Request, Shared};
use vtium_config_tab_flow::page::layout::{Obj, Slot};
use vtium_config_tab_flow::parts::Stage;
use vtium_config_tab_flow::paths::Piece;
use vtium_config_tab_flow::{FlowTab, Level};
use vtium_config_widgets::contract;
use vtium_config_widgets::input::Key;
use vtium_config_widgets::tab::Tab;

/// The two machines each screen is drawn on.
fn machines() -> [(&'static str, Shared); 2] {
    [
        ("workstation", fixture::padnoir::shared()),
        ("large", fixture::large::shared()),
    ]
}

/// The machine `name`, fresh: the screen map's or the large one.
fn machine_named(name: &str) -> Shared {
    match name {
        "workstation" => fixture::padnoir::shared(),
        _ => fixture::large::shared(),
    }
}

/// A tab drawn once at `cols`×`rows`, its state settled.
fn tab_at(shared: &Shared, cols: u16, rows: u16) -> FlowTab {
    let mut tab = FlowTab::new(true);
    let _ = screen(shared, &tab, cols, rows, true);
    assert!(press(shared, &mut tab, "question.view.flow").is_empty());
    tab
}

/// Makes the document changes among `commands` as the shell does, puts
/// their message on the message line, and says how many changes it made.
fn apply(shared: &mut Shared, commands: &[Command]) -> usize {
    let mut made = 0;
    for c in commands {
        match c {
            Command::Change(Change::Document(d)) => {
                shared.change(d.clone()).expect("the change is made");
                made += 1;
            }
            Command::Change(Change::Say(m)) => shared.message = Some(m.clone()),
            _ => {}
        }
    }
    made
}

/// The name of the first box Find lists of the kind `fact` in `section`.
fn first(shared: &Shared, tab: &FlowTab, fact: &str, section: &str) -> String {
    let place = format!("Flow › {section}");
    tab.find_items(shared)
        .into_iter()
        .find(|i| i.fact == fact && i.place == place)
        .map(|i| i.name)
        .unwrap_or_else(|| panic!("a {fact} in {section}"))
}

/// Presses each row of `rows` in turn, the commands of each made as the
/// shell makes them.
fn keys(shared: &mut Shared, tab: &mut FlowTab, rows: &[&'static str]) {
    for row in rows {
        let out = tab.update(shared, Action::of(row));
        apply(shared, &out);
    }
}

/// Types `text` into the value being typed.
fn type_text(shared: &Shared, tab: &mut FlowTab, text: &str) {
    for c in text.chars() {
        tab.update(shared, Action::key("char.typing", Key::Char(c)));
    }
}

/// A tab on the card of the first terminal of a path in CONFIGURATION.
fn on_card(shared: &Shared, cols: u16, rows: u16) -> FlowTab {
    let mut t = tab_at(shared, cols, rows);
    let head = first(shared, &t, "terminal", "CONFIGURATION");
    assert!(go_to(shared, &mut t, &head, "CONFIGURATION"));
    let out = press(shared, &mut t, "enter.view.flow");
    assert_eq!(out, [Command::Request(Request::Open("flow.card"))]);
    t
}

/// A tab on the maker of the template before the first part of the kind
/// `fact` in PARTS.
fn on_maker(shared: &Shared, fact: &str, cols: u16, rows: u16) -> FlowTab {
    let mut t = tab_at(shared, cols, rows);
    let part = first(shared, &t, fact, "PARTS");
    assert!(go_to(shared, &mut t, &part, "PARTS"));
    press(shared, &mut t, "left.view.flow");
    let out = press(shared, &mut t, "enter.view.flow");
    assert_eq!(out, [Command::Request(Request::Open("flow.maker"))]);
    t
}

/// Moves the highlight to `want`, going right from the page's first box
/// in reading order.
fn go_obj(shared: &Shared, tab: &mut FlowTab, want: &Obj) {
    press(shared, tab, "home.view.flow");
    for _ in 0..2000 {
        if tab.state().at.as_ref() == Some(want) {
            return;
        }
        press(shared, tab, "right.view.flow");
    }
    panic!("no box {want:?}");
}

/// A tab on NEW PATH's card, after New terminal… from NEW PATH's empty
/// terminal place.
fn on_new_card(shared: &mut Shared, cols: u16, rows: u16) -> FlowTab {
    let mut t = tab_at(shared, cols, rows);
    go_obj(shared, &mut t, &Obj::New(Slot::Empty(Stage::Terminal)));
    keys(
        shared,
        &mut t,
        &["enter.view.flow", "enter.layer.flow-maker"],
    );
    assert_eq!(t.node(), "flow.new-card");
    t
}

/// Draws the screen at `cols`×`rows` in color and without, checks both,
/// and keeps the text as the snapshot `name-COLSxROWS`.
fn shot(shared: &Shared, tab: &mut FlowTab, name: &str, cols: u16, rows: u16) -> String {
    let buf = screen(shared, tab, cols, rows, true);
    check_rows(&buf, cols, rows);
    tab.set_color(false);
    let plain = screen(shared, tab, cols, rows, false);
    tab.set_color(true);
    check_no_color(&plain);
    assert!(
        has_modifier(&plain, Modifier::REVERSED) || has_modifier(&plain, Modifier::BOLD),
        "{name}: without color the highlight keeps an attribute"
    );
    assert_eq!(text_of(&buf), text_of(&plain), "{name}: color adds no text");
    snapshot(&format!("{name}-{cols}x{rows}"), &buf);
    text_of(&buf)
}

#[test]
fn a_terminal_s_card_shows_its_lines_and_what_it_draws() {
    for (name, shared) in machines() {
        for (cols, rows) in SIZES {
            let mut t = on_card(&shared, cols, rows);
            let text = shot(&shared, &mut t, &format!("card-{name}"), cols, rows);
            for must in [
                "TERMINAL",
                "Name",
                "Look",
                "VTs",
                "Consoles",
                "Login",
                "Switch on Enter",
                "WHAT THIS TERMINAL DRAWS",
                "[e]",
            ] {
                assert!(text.contains(must), "{name} {cols}x{rows}: {must}\n{text}");
            }
            // the Login line shows its two choices in place, and what it
            // means under the card
            for _ in 0..4 {
                press(&shared, &mut t, "down.layer.flow-card");
            }
            let text = shot(&shared, &mut t, &format!("card-login-{name}"), cols, rows);
            assert!(
                text.contains("● Default: /bin/login") && text.contains("○ Other…"),
                "{text}"
            );
            assert!(text.contains("Login: the program"), "{text}");
            // Right steps to Other…, where Enter types a command, and Esc
            // leaves the login as it was
            press(&shared, &mut t, "right.layer.flow-card");
            press(&shared, &mut t, "enter.layer.flow-card");
            assert_eq!(t.node(), "flow.card.typing", "{name}");
            press(&shared, &mut t, "esc.typing");
            press(&shared, &mut t, "left.layer.flow-card");
            for _ in 0..2 {
                press(&shared, &mut t, "down.layer.flow-card");
            }
            let text = shot(&shared, &mut t, &format!("card-switch-{name}"), cols, rows);
            assert!(text.contains("● off") && text.contains("○ on"), "{text}");
            assert!(full_text_of(&shared, &t).contains("Switch on Enter"));
        }
    }
}

/// F1's text for the tab.
fn full_text_of(shared: &Shared, t: &FlowTab) -> String {
    t.full_text(shared).expect("F1's text")
}

#[test]
fn a_card_s_name_is_typed_and_a_taken_name_is_refused() {
    for (name, mut shared) in machines() {
        for (cols, rows) in SIZES {
            let mut t = on_card(&shared, cols, rows);
            press(&shared, &mut t, "enter.layer.flow-card");
            assert_eq!(t.node(), "flow.card.typing");
            type_text(&shared, &mut t, "x");
            let text = shot(&shared, &mut t, &format!("card-typing-{name}"), cols, rows);
            assert!(text.contains("x_"), "{text}");
            // Esc keeps the name as it was and changes nothing
            assert_eq!(
                press(&shared, &mut t, "esc.typing"),
                [Command::Change(Change::Quiet)]
            );
            assert_eq!(t.node(), "flow.card");
        }
        let mut t = on_card(&shared, 100, 30);
        let before = shared.document().text().to_owned();
        press(&shared, &mut t, "enter.layer.flow-card");
        for _ in 0..64 {
            press(&shared, &mut t, "backspace.typing");
        }
        type_text(&shared, &mut t, "roaming");
        let out = press(&shared, &mut t, "enter.typing");
        assert_eq!(
            apply(&mut shared, &out),
            0,
            "{name}: a refused name changes nothing"
        );
        assert_eq!(
            t.node(),
            "flow.card.typing",
            "{name}: the entry stays to be fixed"
        );
        assert_eq!(shared.document().text(), before);
    }
}

#[test]
fn a_card_s_list_box_keeps_its_pick_as_one_change() {
    for (name, mut shared) in machines() {
        for (cols, rows) in SIZES {
            let mut t = on_card(&shared, cols, rows);
            for _ in 0..3 {
                press(&shared, &mut t, "down.layer.flow-card");
            }
            press(&shared, &mut t, "enter.layer.flow-card");
            assert_eq!(t.node(), "flow.card.list", "{name}");
            press(&shared, &mut t, "down.layer.list");
            press(&shared, &mut t, "down.layer.list");
            let text = shot(&shared, &mut t, &format!("card-list-{name}"), cols, rows);
            assert!(text.contains("│") && text.contains("6"), "{text}");
            // Esc closes the list box and changes nothing
            assert!(press(&shared, &mut t, "esc.layer.list").is_empty());
            assert_eq!(t.node(), "flow.card");
        }
        let mut t = on_card(&shared, 100, 30);
        let head = first(&shared, &t, "terminal", "CONFIGURATION");
        for _ in 0..3 {
            press(&shared, &mut t, "down.layer.flow-card");
        }
        press(&shared, &mut t, "enter.layer.flow-card");
        press(&shared, &mut t, "down.layer.list");
        press(&shared, &mut t, "down.layer.list");
        let before = shared.document().text().to_owned();
        let out = press(&shared, &mut t, "enter.layer.list");
        // the pick is a change the shell makes: until it does, the
        // document is as it was
        assert_eq!(shared.document().text(), before, "{name}");
        assert_eq!(apply(&mut shared, &out), 1, "{name}: one change");
        assert_eq!(t.node(), "flow.card");
        let after = shared.document().text().to_owned();
        let section = after
            .split(&format!("[head {head}]"))
            .nth(1)
            .and_then(|s| s.split("\n[").next())
            .expect("the head's section");
        assert!(section.contains("consoles = 3"), "{name}: {section}");
        // the card shows the value the document now holds
        let text = text_of(&screen(&shared, &t, 100, 30, true));
        assert!(text.contains("Consoles                       3"), "{text}");
    }
}

#[test]
fn an_added_card_keeps_its_vts_for_revise() {
    let mut shared = fixture::padnoir::shared();
    let mut t = on_card(&shared, 100, 30);
    press(&shared, &mut t, "down.layer.flow-card");
    press(&shared, &mut t, "down.layer.flow-card");
    let before = shared.document().text().to_owned();
    let out = press(&shared, &mut t, "enter.layer.flow-card");
    assert_eq!(apply(&mut shared, &out), 0);
    assert_eq!(t.node(), "flow.card");
    assert!(
        shared
            .message
            .as_ref()
            .is_some_and(|m| m.line.contains("[e]"))
    );
    assert_eq!(shared.document().text(), before);
    // e on the card revises its path
    let out = press(&shared, &mut t, "e.layer.flow-card");
    assert_eq!(out.first(), Some(&Command::Request(Request::Close)));
    assert_eq!(t.level(), &Level::Page);
    assert_eq!(t.state().new.name.as_deref(), Some("desk"));
}

#[test]
fn the_part_makers_list_their_choices_and_open_a_picker() {
    for (name, shared) in machines() {
        for (cols, rows) in SIZES {
            let mut t = on_maker(&shared, "display", cols, rows);
            let text = shot(
                &shared,
                &mut t,
                &format!("maker-display-{name}"),
                cols,
                rows,
            );
            for must in [
                "ADD A DISPLAY",
                "All displays",
                "OR CHOOSE BY",
                "Graphics card",
            ] {
                assert!(text.contains(must), "{must}\n{text}");
            }
            // This monitor lists each EDID with its display
            press(&shared, &mut t, "down.layer.flow-maker");
            press(&shared, &mut t, "enter.layer.flow-maker");
            assert_eq!(t.node(), "flow.maker.picker");
            let text = shot(&shared, &mut t, &format!("picker-{name}"), cols, rows);
            assert!(
                text.contains("THIS MONITOR") && text.contains("edid:"),
                "{text}"
            );
            assert!(full_text_of(&shared, &t).contains("edid:"));
            press(&shared, &mut t, "end.layer.list");
            press(&shared, &mut t, "enter.layer.list");
            assert_eq!(t.node(), "flow.maker.picker.typing");
            type_text(&shared, &mut t, "AAA:");
            let text = shot(
                &shared,
                &mut t,
                &format!("picker-typing-{name}"),
                cols,
                rows,
            );
            assert!(text.contains("AAA:_"), "{text}");
            press(&shared, &mut t, "esc.typing");
            assert_eq!(t.node(), "flow.maker.picker");
            press(&shared, &mut t, "esc.layer.list");
            assert_eq!(t.node(), "flow.maker");
            let mut t = on_maker(&shared, "input", cols, rows);
            let text = shot(&shared, &mut t, &format!("maker-input-{name}"), cols, rows);
            for must in [
                "ADD AN INPUT",
                "No input",
                "OR CHOOSE BY",
                "udev property",
                "(advanced)",
            ] {
                assert!(text.contains(must), "{must}\n{text}");
            }
            for _ in 0..4 {
                press(&shared, &mut t, "down.layer.flow-maker");
            }
            press(&shared, &mut t, "enter.layer.flow-maker");
            let text = shot(&shared, &mut t, &format!("picker-usb-{name}"), cols, rows);
            assert!(
                text.contains("USB PORT") && text.contains("path:"),
                "{text}"
            );
        }
    }
}

#[test]
fn a_choice_makes_a_part_and_a_typed_value_is_checked() {
    let mut shared = fixture::padnoir::shared();
    let mut t = on_maker(&shared, "input", 100, 30);
    // All keyboards is a part PARTS holds already: the highlight goes to it
    keys(
        &mut shared,
        &mut t,
        &["down.layer.flow-maker", "enter.layer.flow-maker"],
    );
    assert_eq!(t.level(), &Level::Page);
    assert_eq!(
        t.state().at,
        Some(Obj::Pal(Slot::Input(
            vtium_config_tab_flow::paths::ANY_KEYBOARD.to_owned()
        )))
    );
    // a model typed after Other… makes a part of its own, and a bad one
    // is refused in one line and stays to be fixed
    let mut t = on_maker(&shared, "input", 100, 30);
    for _ in 0..6 {
        press(&shared, &mut t, "down.layer.flow-maker");
    }
    keys(
        &mut shared,
        &mut t,
        &[
            "enter.layer.flow-maker",
            "end.layer.list",
            "enter.layer.list",
        ],
    );
    type_text(&shared, &mut t, "nonsense");
    let out = press(&shared, &mut t, "enter.typing");
    assert!(
        matches!(&out[..], [Command::Change(Change::Say(m))] if m.line.contains("model is not valid")),
        "{out:?}"
    );
    assert_eq!(t.node(), "flow.maker.picker.typing");
    for _ in 0..8 {
        press(&shared, &mut t, "backspace.typing");
    }
    type_text(&shared, &mut t, "1234:abcd");
    let out = press(&shared, &mut t, "enter.typing");
    apply(&mut shared, &out);
    assert_eq!(t.level(), &Level::Page);
    assert!(
        t.state()
            .parts
            .iter()
            .any(|p| p.piece == Piece::Input("id:1234:abcd".into()))
    );
    assert!(
        shared
            .message
            .as_ref()
            .is_some_and(|m| m.line.contains("[Space]"))
    );
}

#[test]
fn new_path_s_card_names_the_terminal_and_sets_its_vts() {
    for name in ["workstation", "large"] {
        for (cols, rows) in SIZES {
            let mut shared = machine_named(name);
            let mut t = on_new_card(&mut shared, cols, rows);
            let text = shot(&shared, &mut t, &format!("new-card-{name}"), cols, rows);
            for must in [
                "THE NEW PATH'S TERMINAL",
                "Name",
                "VTs",
                "terminal-",
                "[Space]",
            ] {
                assert!(text.contains(must), "{must}\n{text}");
            }
            // the VTs open the list box, and a pick changes NEW PATH alone
            let before = shared.document().text().to_owned();
            keys(
                &mut shared,
                &mut t,
                &[
                    "down.layer.flow-card",
                    "down.layer.flow-card",
                    "enter.layer.flow-card",
                    "down.layer.list",
                    "down.layer.list",
                    "enter.layer.list",
                ],
            );
            assert_eq!(shared.document().text(), before);
            assert_eq!(t.state().new.setting("vt"), "2");
        }
    }
}

#[test]
fn a_part_s_label_is_typed_and_set_as_one_change() {
    for (name, mut shared) in machines() {
        for (cols, rows) in SIZES {
            let mut t = tab_at(&shared, cols, rows);
            let part = first(&shared, &t, "display", "PARTS");
            assert!(go_to(&shared, &mut t, &part, "PARTS"));
            press(&shared, &mut t, "enter.view.flow");
            press(&shared, &mut t, "enter.layer.flow-detail");
            assert_eq!(t.node(), "flow.detail.typing");
            type_text(&shared, &mut t, "Big screen");
            let text = shot(&shared, &mut t, &format!("label-typing-{name}"), cols, rows);
            assert!(
                text.contains("Big screen_") && text.contains("[Enter] keeps it; blank removes it"),
                "{text}"
            );
            assert_eq!(
                press(&shared, &mut t, "esc.typing"),
                [Command::Change(Change::Quiet)]
            );
            assert_eq!(t.node(), "flow.detail");
        }
        let mut t = tab_at(&shared, 100, 30);
        let part = first(&shared, &t, "display", "PARTS");
        assert!(go_to(&shared, &mut t, &part, "PARTS"));
        keys(
            &mut shared,
            &mut t,
            &["enter.view.flow", "enter.layer.flow-detail"],
        );
        type_text(&shared, &mut t, "Big screen");
        let out = press(&shared, &mut t, "enter.typing");
        assert_eq!(apply(&mut shared, &out), 1, "{name}");
        assert_eq!(t.node(), "flow.detail");
        assert!(
            shared.document().text().contains("= Big screen\n"),
            "{name}"
        );
        for (cols, rows) in SIZES {
            let text = shot(&shared, &mut t, &format!("label-set-{name}"), cols, rows);
            assert!(text.contains("The label is now “Big screen”."), "{text}");
        }
        // a label that starts with # is refused
        press(&shared, &mut t, "enter.layer.flow-detail");
        for _ in 0..12 {
            press(&shared, &mut t, "backspace.typing");
        }
        type_text(&shared, &mut t, "#x");
        let out = press(&shared, &mut t, "enter.typing");
        assert_eq!(apply(&mut shared, &out), 0);
        assert_eq!(t.node(), "flow.detail.typing");
    }
}

#[test]
fn revise_takes_a_path_into_new_path_and_a_puts_it_back_unchanged() {
    for (name, mut shared) in machines() {
        for (cols, rows) in SIZES {
            let mut t = tab_at(&shared, cols, rows);
            let head = first(&shared, &t, "terminal", "CONFIGURATION");
            assert!(go_to(&shared, &mut t, &head, "CONFIGURATION"));
            let out = press(&shared, &mut t, "e.view.flow");
            apply(&mut shared, &out);
            assert_eq!(t.state().new.name.as_deref(), Some(head.as_str()));
            let text = shot(&shared, &mut t, &format!("revise-{name}"), cols, rows);
            assert!(text.contains("Revising"), "{text}");
            // e again while NEW PATH holds the path says so and changes
            // nothing
            let before = (t.state().new.clone(), t.state().revising.clone());
            let out = press(&shared, &mut t, "e.view.flow");
            assert!(
                matches!(&out[..], [Command::Change(Change::Say(_))]),
                "{out:?}"
            );
            assert_eq!((t.state().new.clone(), t.state().revising.clone()), before);
        }
        let before = shared.document().text().to_owned();
        let mut t = tab_at(&shared, 100, 30);
        let head = first(&shared, &t, "terminal", "CONFIGURATION");
        assert!(go_to(&shared, &mut t, &head, "CONFIGURATION"));
        keys(&mut shared, &mut t, &["e.view.flow"]);
        let out = press(&shared, &mut t, "a.view.flow");
        assert_eq!(
            apply(&mut shared, &out),
            0,
            "{name}: adding it back changes nothing"
        );
        assert_eq!(shared.document().text(), before, "{name}");
        assert!(t.state().new.is_empty());
        assert_eq!(t.state().at, Some(Obj::Doc(head.clone(), Slot::Terminal)));
    }
}

#[test]
fn esc_on_a_revised_path_takes_it_out_as_one_change() {
    let mut shared = fixture::padnoir::shared();
    let mut t = tab_at(&shared, 100, 30);
    assert!(go_to(&shared, &mut t, "panel", "CONFIGURATION"));
    keys(&mut shared, &mut t, &["e.view.flow"]);
    let out = press(&shared, &mut t, "esc.view.flow");
    assert_eq!(apply(&mut shared, &out), 1);
    let text = shared.document().text();
    assert!(
        !text.contains("[head panel]") && !text.contains("-> panel"),
        "{text}"
    );
    assert!(t.state().new.is_empty() && t.state().revising.is_none());
    assert!(
        shared
            .message
            .as_ref()
            .is_some_and(|m| m.line.starts_with("Took the path"))
    );
}

/// The configuration the comparisons with today's Flow tab start from,
/// on the screen map's devices: DP-8 in `desk` on VT 1, and `panel` on VT
/// 3 with an input of its own, so HDMI-A-2 is free to place.
const START: &str = "version = 1\n[head desk]\nvt = 1\n[head panel]\nvt = 3\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n";

/// The screen map's devices with [`START`] open.
fn start() -> Shared {
    machine("workstation", START, fixture::padnoir::present())
}

// What today's Flow tab writes for each comparison's keys, from
// vtium-config's tui::paths on the same configuration and devices.

/// NEW PATH: a new terminal on VT 2, HDMI-A-2 and the keyboard by its
/// USB port, then `a`. The port picker reads the keyboard's path
/// `pci-0000:15:00.0-usb-0:3.3` as a USB interface on the port
/// `pci-0000:15:00.0-usb-0`, today as in the rebuilt tab.
const ADDED: &str = "version = 1\n[head desk]\nvt = 1\n[head panel]\nvt = 3\n[head terminal-1]\nvt = 2\n\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ndisplay edid:BBB:0x0002:SMALL00001 -> terminal-1\ninput path:pci-0:3.3 -> panel\ninput path:pci-0000:15:00.0-usb-0 -> terminal-1\ninput class:keyboard -> roaming\n";

/// `panel` revised: its input taken out, HDMI-A-2 picked, then `a`.
const REVISED: &str = "version = 1\n[head desk]\nvt = 1\n[head panel]\nvt = 3\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ndisplay edid:BBB:0x0002:SMALL00001 -> panel\ninput class:keyboard -> roaming\n";

/// `desk`'s card: Consoles set to 3 in its list box.
const CONSOLES: &str = "version = 1\n[head desk]\nconsoles = 3\nvt = 1\n[head panel]\nvt = 3\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n";

/// DP-8's detail: the label Big screen.
const LABEL: &str = "version = 1\n[head desk]\nvt = 1\n[head panel]\nvt = 3\n[rules]\ndisplay edid:AAA:0x0001:LARGE00001 -> desk\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n\n# The person's own names for displays and inputs. vtium shows a\n# display's label when vtium-config identifies the displays.\n[labels]\ndisplay edid:AAA:0x0001:LARGE00001 = Big screen\n";

#[test]
fn a_path_added_through_new_path_writes_what_today_s_flow_tab_writes() {
    let mut shared = start();
    let mut t = tab_at(&shared, 100, 30);
    // the terminal first, through the terminal maker, with VT 2 on its card
    go_obj(&shared, &mut t, &Obj::New(Slot::Empty(Stage::Terminal)));
    keys(
        &mut shared,
        &mut t,
        &["enter.view.flow", "enter.layer.flow-maker"],
    );
    assert_eq!(t.node(), "flow.new-card");
    keys(
        &mut shared,
        &mut t,
        &[
            "down.layer.flow-card",
            "down.layer.flow-card",
            "enter.layer.flow-card",
            "down.layer.list",
            "down.layer.list",
            "enter.layer.list",
            "esc.layer",
        ],
    );
    assert_eq!(t.level(), &Level::Page);
    // HDMI-A-2 by Space, and the keyboard through the input maker's USB
    // port picker
    assert!(go_to(&shared, &mut t, "HDMI-A-2", "PARTS"));
    keys(&mut shared, &mut t, &["space.view.flow"]);
    go_obj(&shared, &mut t, &Obj::New(Slot::Empty(Stage::Input)));
    keys(&mut shared, &mut t, &["enter.view.flow"]);
    for _ in 0..4 {
        keys(&mut shared, &mut t, &["down.layer.flow-maker"]);
    }
    keys(
        &mut shared,
        &mut t,
        &["enter.layer.flow-maker", "enter.layer.list"],
    );
    assert_eq!(t.level(), &Level::Page);
    let before = shared.document().text().to_owned();
    let out = press(&shared, &mut t, "a.view.flow");
    assert_eq!(
        shared.document().text(),
        before,
        "the shell makes the change"
    );
    assert_eq!(apply(&mut shared, &out), 1, "{out:?}");
    assert_eq!(shared.document().text(), ADDED);
    assert!(t.state().new.is_empty());
    assert_eq!(
        t.state().at,
        Some(Obj::Doc("terminal-1".into(), Slot::Terminal))
    );
    // Undo takes the whole path back as one step
    shared.undo();
    assert_eq!(shared.document().text(), START);
}

#[test]
fn a_revised_path_writes_what_today_s_flow_tab_writes() {
    let mut shared = start();
    let mut t = tab_at(&shared, 100, 30);
    assert!(go_to(&shared, &mut t, "panel", "CONFIGURATION"));
    keys(&mut shared, &mut t, &["e.view.flow", "enter.view.flow"]);
    assert_eq!(t.node(), "flow.new-card");
    // the card's last line is the path's input, which Enter takes out
    keys(
        &mut shared,
        &mut t,
        &["end.layer.flow-card", "enter.layer.flow-card", "esc.layer"],
    );
    assert!(t.state().new.inputs.is_empty());
    assert!(go_to(&shared, &mut t, "HDMI-A-2", "PARTS"));
    keys(&mut shared, &mut t, &["space.view.flow"]);
    let out = press(&shared, &mut t, "a.view.flow");
    assert_eq!(apply(&mut shared, &out), 1, "{out:?}");
    assert_eq!(shared.document().text(), REVISED);
}

#[test]
fn a_card_s_change_and_a_label_write_what_today_s_flow_tab_writes() {
    let mut shared = start();
    let mut t = tab_at(&shared, 100, 30);
    assert!(go_to(&shared, &mut t, "desk", "CONFIGURATION"));
    keys(
        &mut shared,
        &mut t,
        &[
            "enter.view.flow",
            "down.layer.flow-card",
            "down.layer.flow-card",
            "down.layer.flow-card",
            "enter.layer.flow-card",
            "down.layer.list",
            "down.layer.list",
            "enter.layer.list",
        ],
    );
    assert_eq!(shared.document().text(), CONSOLES);
    let mut shared = start();
    let mut t = tab_at(&shared, 100, 30);
    assert!(go_to(&shared, &mut t, "DP-8", "PARTS"));
    keys(
        &mut shared,
        &mut t,
        &["enter.view.flow", "enter.layer.flow-detail"],
    );
    type_text(&shared, &mut t, "Big screen");
    keys(&mut shared, &mut t, &["enter.typing"]);
    assert_eq!(shared.document().text(), LABEL);
}

/// A tab on the node `node` of the editing, on the machine `shared`.
fn on_node(shared: &mut Shared, node: &str) -> FlowTab {
    let mut t = match node {
        "flow.card" | "flow.card.list" | "flow.card.typing" => on_card(shared, 100, 30),
        "flow.new-card" => on_new_card(shared, 100, 30),
        _ if node.starts_with("flow.maker") => on_maker(shared, "display", 100, 30),
        _ => {
            let mut t = tab_at(shared, 100, 30);
            let part = first(shared, &t, "display", "PARTS");
            assert!(go_to(shared, &mut t, &part, "PARTS"));
            press(shared, &mut t, "enter.view.flow");
            t
        }
    };
    let rows: &[&'static str] = match node {
        "flow.card.list" => &["down.layer.flow-card", "enter.layer.flow-card"],
        "flow.card.typing" => &["enter.layer.flow-card"],
        "flow.maker.picker" => &["down.layer.flow-maker", "enter.layer.flow-maker"],
        "flow.maker.picker.typing" => &[
            "down.layer.flow-maker",
            "enter.layer.flow-maker",
            "end.layer.list",
            "enter.layer.list",
        ],
        "flow.detail.typing" => &["enter.layer.flow-detail"],
        _ => &[],
    };
    keys(shared, &mut t, rows);
    assert_eq!(t.node(), node);
    t
}

/// Whether the row `row` is one the shell answers, which reaches the tab
/// as nothing it acts on: quitting, the tabs, Undo and Redo, F1, Find,
/// the action bar's More and the key line, and the keys that do nothing
/// while a value is typed.
fn shell_row(row: &str) -> bool {
    let verb = row.split('.').next().unwrap_or("");
    matches!(
        verb,
        "q" | "tab"
            | "shift-tab"
            | "ctrl-c"
            | "ctrl-z"
            | "ctrl-y"
            | "f1"
            | "ctrl-f"
            | "period"
            | "key-line"
            | "action"
    ) || (row.ends_with(".typing") && matches!(verb, "click" | "wheel"))
        || (verb == "question" && row != "question.layer.list")
}

#[test]
fn every_row_of_each_editing_node_does_what_the_key_table_says() {
    for node in [
        "flow.card",
        "flow.card.list",
        "flow.card.typing",
        "flow.new-card",
        "flow.maker",
        "flow.maker.picker",
        "flow.maker.picker.typing",
        "flow.detail.typing",
    ] {
        let rows: Vec<&'static str> = contract::node(node)
            .expect("the node")
            .rows()
            .map(|r| r.id)
            .collect();
        assert!(!rows.is_empty(), "{node}");
        for name in ["workstation", "large"] {
            for row in &rows {
                let mut shared = machine_named(name);
                let mut t = on_node(&mut shared, node);
                let before = shared.document().text().to_owned();
                let out = t.update(&shared, Action::of(row));
                if shell_row(row) {
                    assert!(out.is_empty(), "{name} {node}: {row} returned {out:?}");
                    assert_eq!(t.node(), node, "{name} {node}: {row}");
                }
                // only Enter keeps a change, and none of these rows' own
                // action changes the document without a command
                assert_eq!(shared.document().text(), before, "{name} {node}: {row}");
                let esc = row.starts_with("esc");
                if esc {
                    assert_ne!(t.node(), node, "{name} {node}: {row} leaves it");
                }
            }
        }
    }
}
