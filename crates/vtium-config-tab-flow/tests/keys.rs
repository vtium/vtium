//! The Flow tab driven through its `Tab` methods on the screen map's
//! machine and on the large machine: every row of the key table on each
//! node the tab answers, the clicks on the boxes it draws, Find's matches,
//! F1's text, and the commands each returns. A row the shell answers, such
//! as Ctrl+S or Tab, reaches the tab as nothing it acts on, so the tab
//! returns no command and stays as it was.
//!
//! References:
//! - ADR-1403 D7, "The keys" and the interface tree's Flow nodes.
//! - ADR-1403 D11, "The keys that change": `r`, `n`, `v` and `o` on the
//!   Flow page, and `?`, which does nothing there.
//! - ADR-1206 D2: a tab's update returns effects, changes and requests.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]

mod common;

use common::*;
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Change, Command, Effect, Op, Request, Shared};
use vtium_config_tab_flow::page::layout::{Obj, Slot};
use vtium_config_tab_flow::{FILE_LETTERS, FlowTab, Level};
use vtium_config_widgets::contract;
use vtium_config_widgets::tab::Tab;

/// The two machines every key is pressed on.
fn machines() -> [(&'static str, Shared); 2] {
    [
        ("workstation", fixture::padnoir::shared()),
        ("large", fixture::large::shared()),
    ]
}

/// A tab drawn once at 100×30, its state settled by an action it does
/// not act on.
fn tab(shared: &Shared) -> FlowTab {
    let mut tab = FlowTab::new(true);
    let _ = screen(shared, &tab, 100, 30, true);
    assert!(press(shared, &mut tab, "question.view.flow").is_empty());
    tab
}

/// A tab on the node `node`: the page, a part's detail on its label or
/// on its first choice, a part of a path, or the other inputs.
fn tab_on(name: &str, shared: &Shared, node: &str) -> FlowTab {
    let mut t = tab(shared);
    let (part, section) = match (node, name) {
        ("flow", _) => return t,
        ("flow.detail" | "flow.detail.choices", "workstation") => ("DP-8", "PARTS"),
        ("flow.detail" | "flow.detail.choices", _) => ("Keyboard 07", "PARTS"),
        ("flow.info", "workstation") => ("All displays", "CONFIGURATION"),
        ("flow.info", _) => ("Mouse 42", "CONFIGURATION"),
        ("flow.others", _) => ("1 other input", "PARTS"),
        _ => panic!("no way to {node}"),
    };
    assert!(go_to(shared, &mut t, part, section), "{name}: {part}");
    press(shared, &mut t, "enter.view.flow");
    if node == "flow.detail.choices" {
        press(shared, &mut t, "down.layer.flow-detail");
    }
    assert_eq!(t.node(), node, "{name}");
    t
}

/// The rows of `node` that the tab acts on, each with what it returns.
#[derive(Debug, PartialEq)]
enum Want {
    /// A message on the message line.
    Say,
    /// The message line cleared, as a move does.
    Quiet,
    /// This request.
    Request(Request),
    /// This effect.
    Effect(Effect),
    /// Nothing returned, and the tab on another node.
    Node(&'static str),
    /// Nothing returned, and nothing changed.
    Nothing,
}

/// What `commands` are, as a [`Want`] compares them.
fn got(commands: &[Command], before: &str, after: &str) -> Want {
    match commands {
        [Command::Change(Change::Say(_))] => Want::Say,
        [Command::Change(Change::Quiet)] => Want::Quiet,
        [Command::Request(r)] => Want::Request(r.clone()),
        [Command::Effect(e)] => Want::Effect(e.clone()),
        [] if before != after => Want::Node(leak(after)),
        [] => Want::Nothing,
        other => panic!("unexpected commands {other:?}"),
    }
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

/// What the row `row` of `node` returns.
fn want(node: &str, row: &str) -> Want {
    match (node, row) {
        // the File menu's letters: Revert to saved… and New… ask first,
        // Show changes shows them, and History lists the versions
        ("flow", "r.view.flow" | "n.view.flow") => {
            Want::Request(Request::Open("any.question.confirm"))
        }
        ("flow", "v.view.flow") => Want::Request(Request::Open("any.changes")),
        ("flow", "o.view.flow") => Want::Request(Request::Open("any.menu.file.history")),
        ("flow", "space.view.flow" | "esc.view.flow" | "t.view.flow" | "p.view.flow") => Want::Say,
        // the first box of either machine is a display of a path, whose
        // information opens
        ("flow", "enter.view.flow") => Want::Request(Request::Open("flow.info")),
        ("flow", "d.view.flow") => Want::Effect(Effect::Desk(Op::Devices)),
        (
            "flow",
            "up.view.flow"
            | "down.view.flow"
            | "left.view.flow"
            | "right.view.flow"
            | "home.view.flow"
            | "end.view.flow"
            | "pgup.view.flow"
            | "pgdn.view.flow"
            | "wheel-up.view.flow"
            | "wheel-down.view.flow",
        ) => Want::Quiet,
        (
            "flow.detail",
            "down.layer.flow-detail"
            | "end.layer.flow-detail"
            | "pgdn.layer.flow-detail"
            | "wheel-down.layer",
        ) => Want::Node("flow.detail.choices"),
        ("flow.detail", "esc.layer.flow-detail")
        | ("flow.detail.choices" | "flow.info" | "flow.others", "esc.layer") => {
            Want::Request(Request::Close)
        }
        (
            "flow.detail.choices",
            "up.layer.flow-choices"
            | "home.layer.flow-choices"
            | "pgup.layer.flow-choices"
            | "wheel-up.layer",
        ) => Want::Node("flow.detail"),
        // Down and End move along the choices, on the same node
        _ => Want::Nothing,
    }
}

#[test]
fn the_tab_names_itself_and_takes_its_keys_from_the_key_table() {
    let shared = fixture::padnoir::shared();
    let tab = tab(&shared);
    assert_eq!(
        (tab.name(), tab.letter(), tab.node()),
        ("Flow", 'o', "flow")
    );
    assert_eq!(tab.bar(), contract::buttons("flow"));
    assert_eq!(tab.key_line(), contract::labelled("flow"));
    assert!(tab.searchable().is_none(), "Flow offers no ? search");
    assert!(contract::search("flow").is_empty());
    assert!(tab.menu_items(&shared).is_empty());
    for (row, node) in FILE_LETTERS {
        assert!(contract::row(row).is_some(), "{row} is in the key table");
        assert!(contract::node(node).is_some(), "{node} is in the tree");
    }
}

#[test]
fn every_row_of_each_node_returns_what_the_key_table_says() {
    for node in [
        "flow",
        "flow.detail",
        "flow.detail.choices",
        "flow.info",
        "flow.others",
    ] {
        let rows: Vec<&str> = contract::node(node)
            .expect("the node")
            .rows()
            .map(|r| r.id)
            .collect();
        assert!(!rows.is_empty(), "{node}");
        for (name, shared) in machines() {
            if node == "flow.others" && name == "large" {
                // the large machine has no other inputs to fold
                continue;
            }
            for row in &rows {
                let mut t = tab_on(name, &shared, node);
                let before = (t.state().clone(), t.level().clone());
                let row: &'static str = leak(row);
                let commands = t.update(&shared, Action::of(row));
                let got = got(&commands, node, t.node());
                let want = want(node, row);
                if want == Want::Nothing && got == Want::Nothing {
                    let after = (t.state().clone(), t.level().clone());
                    // a move that has nowhere to go changes nothing, and
                    // every other row the tab leaves to the shell
                    if !row.contains("flow-choices") && !row.starts_with("wheel") {
                        assert_eq!(before, after, "{name} {node}: {row} changed the tab");
                    }
                }
                assert_eq!(got, want, "{name} {node}: {row}");
            }
        }
    }
}

#[test]
fn the_arrows_move_from_box_to_box_and_pgdn_scrolls_by_a_window() {
    let shared = fixture::large::shared();
    let mut t = tab(&shared);
    let first = t.state().at.clone();
    press(&shared, &mut t, "right.view.flow");
    assert_ne!(t.state().at, first);
    press(&shared, &mut t, "left.view.flow");
    assert_eq!(t.state().at, first);
    // Down from a display of a path keeps to the displays' column
    press(&shared, &mut t, "down.view.flow");
    assert!(
        matches!(t.state().at, Some(Obj::Doc(_, Slot::Display(_)))),
        "{:?}",
        t.state().at
    );
    press(&shared, &mut t, "pgdn.view.flow");
    assert_eq!(t.state().top, usize::from(viewer(100, 30).height));
    press(&shared, &mut t, "pgup.view.flow");
    assert_eq!(t.state().top, 0);
    press(&shared, &mut t, "end.view.flow");
    let last = t.state().at.clone();
    assert!(
        matches!(last, Some(Obj::Pal(_))),
        "End goes to the last box: {last:?}"
    );
    press(&shared, &mut t, "home.view.flow");
    assert_eq!(t.state().at, first);
}

#[test]
fn a_click_highlights_a_box_and_a_second_click_opens_it() {
    let shared = fixture::padnoir::shared();
    let mut t = tab(&shared);
    let buf = screen(&shared, &t, 100, 30, true);
    let (x, y) = find(&buf, "panel").expect("the terminal panel on screen");
    assert_eq!(click(&shared, &mut t, "click-box.view.flow", x, y).len(), 1);
    assert_eq!(
        t.state().at,
        Some(Obj::Doc("panel".to_owned(), Slot::Terminal))
    );
    // a terminal's card is the editing's, so a second click opens nothing
    // yet; on a display of a path it opens its information
    // a click on the box's bottom border lands on the box too
    let (x, y) = find(&buf, "pci-0:3.3").expect("the panel's input");
    click(&shared, &mut t, "click-box.view.flow", x, y + 2);
    assert_eq!(
        t.state().at,
        Some(Obj::Doc(
            "panel".to_owned(),
            Slot::Input("path:pci-0:3.3".to_owned())
        ))
    );
    assert_eq!(t.node(), "flow");
    let opened = click(&shared, &mut t, "click-box.view.flow", x, y);
    assert_eq!(opened, [Command::Request(Request::Open("flow.info"))]);
    assert_eq!(t.node(), "flow.info");
    // a click on no box, or outside the viewer, does nothing
    let mut t = tab(&shared);
    let at = t.state().at.clone();
    assert!(click(&shared, &mut t, "click-box.view.flow", 99, 8).is_empty());
    assert!(click(&shared, &mut t, "click-box.view.flow", 5, 29).is_empty());
    assert_eq!(t.state().at, at);
}

#[test]
fn space_picks_a_part_into_new_path_and_takes_it_out() {
    let mut shared = examples::roaming();
    let mut t = tab(&shared);
    assert!(go_to(&shared, &mut t, "ThinkPad Extra Buttons", "PARTS"));
    let said = press(&shared, &mut t, "space.view.flow");
    with_message(&mut shared, &said);
    assert_eq!(t.state().new.inputs.len(), 1);
    let line = shared.message.as_ref().map(|m| m.line.as_str());
    assert_eq!(
        line,
        Some("Picked the ThinkPad Extra Buttons. Now pick a display.")
    );
    // the picked part's box in NEW PATH
    assert!(text_of(&screen(&shared, &t, 100, 30, true)).contains("✓ ThinkPad Extra Buttons"));
    press(&shared, &mut t, "space.view.flow");
    assert!(t.state().new.is_empty(), "Space again takes it out");
    // Esc clears NEW PATH, and says so; with nothing in it, says that
    press(&shared, &mut t, "space.view.flow");
    let cleared = press(&shared, &mut t, "esc.view.flow");
    assert!(t.state().new.is_empty());
    with_message(&mut shared, &cleared);
    assert_eq!(
        shared.message.as_ref().map(|m| m.line.as_str()),
        Some("Cleared NEW PATH.")
    );
}

#[test]
fn find_lists_every_part_and_goes_to_the_one_chosen() {
    let shared = fixture::large::shared();
    let mut t = tab(&shared);
    let items = t.find_items(&shared);
    for n in 0..30 {
        let name = format!("Keyboard {n:02}");
        assert!(
            items
                .iter()
                .any(|i| i.name == name && i.place == "Flow › PARTS"),
            "{name} is not among Find's items"
        );
    }
    assert!(
        items
            .iter()
            .all(|i| !i.name.is_empty() && !i.fact.is_empty())
    );
    assert!(go_to(&shared, &mut t, "Mouse 59", "PARTS"));
    let text = text_of(&screen(&shared, &t, 100, 30, true));
    assert!(
        text.contains("┃ Mouse 59"),
        "the match is highlighted in sight:\n{text}"
    );
    // Find's match closes a level open over the page
    let mut t = tab_on("large", &shared, "flow.detail");
    assert!(go_to(&shared, &mut t, "Mouse 59", "PARTS"));
    assert_eq!(t.level(), &Level::Page);
}

#[test]
fn f1_gives_the_full_text_of_what_is_highlighted() {
    let shared = fixture::padnoir::shared();
    let mut t = tab(&shared);
    let text = t.full_text(&shared).expect("the highlighted box's text");
    assert!(
        text.starts_with("All displays: every display, connected now or later."),
        "{text}"
    );
    let t2 = tab_on("workstation", &shared, "flow.others");
    let text = t2.full_text(&shared).expect("the list's text");
    assert!(text.contains("Power Button"), "{text}");
    press(&shared, &mut t, "enter.view.flow");
    assert!(
        t.full_text(&shared)
            .expect("the part's text")
            .contains("Matched by: *")
    );
}

#[test]
fn a_device_that_comes_is_a_new_part_and_one_that_goes_stays_not_connected() {
    let mut shared = fixture::padnoir::shared();
    let mut t = tab(&shared);
    let held = t
        .state()
        .parts
        .iter()
        .find(|p| p.title == "HDMI-A-2")
        .expect("HDMI-A-2's part");
    assert!(held.connected);
    // HDMI-A-2 is unplugged and a mouse comes
    shared
        .present
        .devices
        .retain(|d| d.connector.as_deref() != Some("HDMI-A-2"));
    let mut mouse = vtium_settings::Identity::input().with_class(vtium_settings::Class::Mouse);
    mouse.name = Some("Logitech USB Receiver".to_owned());
    mouse.path = Some("pci-0000:15:00.0-usb-0:3.4:1.0".to_owned());
    shared.present.devices.push(mouse);
    press(&shared, &mut t, "question.view.flow");
    let names: Vec<String> = t.state().parts.iter().map(|p| p.title.clone()).collect();
    assert!(
        names.iter().any(|n| n == "Logitech USB Receiver"),
        "{names:?}"
    );
    let gone = t
        .state()
        .parts
        .iter()
        .find(|p| p.title == "HDMI-A-2")
        .expect("HDMI-A-2 stays");
    assert!(!gone.connected);
}
