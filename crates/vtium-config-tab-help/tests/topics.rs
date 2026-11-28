//! What each topic says. A tab's topic gives every labelled row of its
//! nodes, which these tests read from the TSV files themselves, ADR-1403's
//! two tables and the overlay under vtium-config-widgets' `contract/`, so
//! a row the tables gain or lose fails here until the tab's topics follow.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet: the topics, and a
//!   tab's topic as its keys and what each does.
//! - ADR-1403 D7, "Help that matches the code": the help says only what
//!   the contract says.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use vtium_config_tab_help::topics::{self, Entry, Kind, TOPICS};
use vtium_config_widgets::contract;

/// The repository's root.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The rows of a TSV file, each a map from its header's names.
fn tsv(path: &str) -> Vec<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(root().join(path)).expect("the table");
    let mut lines = text.lines();
    let head: Vec<String> = lines
        .next()
        .expect("a header")
        .split('\t')
        .map(str::to_owned)
        .collect();
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            head.iter()
                .cloned()
                .zip(l.split('\t').map(str::to_owned))
                .collect()
        })
        .collect()
}

/// A table with the overlay's rows put in place by id, and the ids that go
/// taken out.
fn merged(
    base: &str,
    over: &str,
    gone: &BTreeSet<String>,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = tsv(base)
        .into_iter()
        .chain(tsv(over))
        .map(|r| (r["id"].clone(), r))
        .collect();
    out.retain(|id, _| !gone.contains(id));
    out
}

/// The labelled rows of each node, as the TSV files give them: each id in
/// the node's `keys` column whose row is supported and has a label.
fn labelled_from_the_tables() -> BTreeMap<String, BTreeSet<String>> {
    let gone = tsv("crates/vtium-config-widgets/contract/d11-gone.tsv");
    let gone_of = |kind: &str| -> BTreeSet<String> {
        gone.iter()
            .filter(|g| g["kind"] == kind)
            .map(|g| g["id"].clone())
            .collect()
    };
    let rows = merged(
        "docs/architecture/platform/ADR-1403-keys.tsv",
        "crates/vtium-config-widgets/contract/d11-keys.tsv",
        &gone_of("row"),
    );
    let nodes = merged(
        "docs/architecture/platform/ADR-1403-tree.tsv",
        "crates/vtium-config-widgets/contract/d11-tree.tsv",
        &gone_of("node"),
    );
    nodes
        .into_iter()
        .map(|(id, node)| {
            let keys = node["keys"]
                .split(", ")
                .filter(|k| {
                    rows.get(*k).is_some_and(|r| {
                        r["status"] == "supported" && !r["label"].is_empty() && r["label"] != "-"
                    })
                })
                .map(str::to_owned)
                .collect();
            (id, keys)
        })
        .collect()
}

/// Every topic's entries, with the screens' About text.
fn entries() -> Vec<Vec<Entry>> {
    topics::entries(common::ABOUT)
}

#[test]
fn each_tab_s_topic_gives_every_labelled_row_of_its_nodes_once() {
    let tables = labelled_from_the_tables();
    let entries = entries();
    for (topic, list) in TOPICS.iter().zip(&entries) {
        let Kind::Tab(nodes) = topic.kind else {
            continue;
        };
        let want: BTreeSet<String> = nodes.iter().flat_map(|n| tables[*n].clone()).collect();
        let shown = topics::rows_shown(list);
        let got: BTreeSet<String> = shown.iter().map(|r| (*r).to_owned()).collect();
        assert_eq!(
            got.len(),
            shown.len(),
            "{}: a row shows twice: {shown:?}",
            topic.name
        );
        let missing: Vec<_> = want.difference(&got).collect();
        let extra: Vec<_> = got.difference(&want).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{}: missing {missing:?}, not in the tables {extra:?}",
            topic.name
        );
    }
}

#[test]
fn the_help_topic_gives_the_help_tab_s_own_keys_first() {
    let entries = entries();
    let own: Vec<&str> = entries[5]
        .iter()
        .filter_map(|e| match e {
            Entry::Key { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(
        own,
        [
            "up.view.help",
            "down.view.help",
            "home.view.help",
            "end.view.help",
            "pgup.view.help",
            "pgdn.view.help",
            "left.view.help",
            "right.view.help",
        ]
    );
    // Left and Right mean the same, so they share one line
    assert!(entries[5].iter().any(|e| matches!(
        e,
        Entry::Key { keys, .. } if keys == &["[Left]", "[Right]"]
    )));
}

#[test]
fn the_keys_that_work_everywhere_are_on_five_tabs_or_six() {
    let got: BTreeSet<&str> = topics::everywhere_rows().iter().map(|r| r.id).collect();
    let want: BTreeSet<&str> = [
        "ctrl-s.view",
        "ctrl-z.view",
        "ctrl-y.view",
        "q.view",
        "ctrl-c.any",
        "tab.view",
        "shift-tab.view",
        "period.view",
        "alt-f.view",
        "alt-a.view",
        "alt-tab.view",
        "alt-p.view",
        "f10.view",
        "ctrl-f.view",
        "f1.view",
        "question.view",
        "1.view",
        "2.view",
        "3.view",
        "4.view",
        "5.view",
        "6.view",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}

#[test]
fn menus_and_save_says_what_each_menu_holds_and_gives_every_key_that_works_everywhere() {
    let entries = entries();
    let menus = &entries[6];
    for node in ["any.menu.file", "any.menu.actions"] {
        let what = contract::node(node).expect("the menu's node").what;
        assert!(menus.contains(&Entry::Text(what.to_owned())), "{node}");
    }
    let shown: BTreeSet<&str> = topics::rows_shown(menus).into_iter().collect();
    let all: BTreeSet<&str> = topics::everywhere_rows().iter().map(|r| r.id).collect();
    assert_eq!(shown, all);
    // each with what it does, in full
    for e in menus {
        if let Entry::Key { rows, means, .. } = e {
            assert_eq!(*means, contract::row(rows[0]).expect("its row").means);
        }
    }
}

#[test]
fn mouse_gives_each_click_and_how_to_select_text_while_vtium_config_has_the_mouse() {
    let entries = entries();
    let mouse = &entries[7];
    let shown = topics::rows_shown(mouse);
    for row in [
        "click-menu.view",
        "click-tab.view",
        "click-row.view",
        "click-button.any",
        "click-facts.view",
    ] {
        assert!(shown.contains(&row), "{row}");
    }
    assert!(
        mouse.contains(&Entry::Text(
            "To select text to copy while vtium-config has the mouse, hold [Shift] and drag."
                .to_owned()
        ))
    );
    // a click is no key to press, so it is not in brackets
    assert!(mouse.iter().any(|e| matches!(
        e,
        Entry::Key { keys, .. } if keys == &["Click on a menu's name"]
    )));
}

#[test]
fn commands_give_every_form_that_vtium_config_s_usage_lists() {
    let entries = entries();
    let usage = vtium_config_core::help::usage();
    let forms: Vec<(&str, &str)> = entries[8]
        .iter()
        .filter_map(|e| match e {
            Entry::Command { synopsis, does } => Some((*synopsis, *does)),
            _ => None,
        })
        .collect();
    let all: usize = vtium_config_core::help::topics()
        .map(|t| t.forms.len())
        .sum();
    assert_eq!(forms.len(), all);
    for (synopsis, does) in forms {
        assert!(usage.contains(synopsis), "{synopsis}");
        assert!(usage.contains(does), "{does}");
    }
}

#[test]
fn about_gives_the_name_the_version_and_what_vtium_config_does() {
    let entries = entries();
    assert_eq!(
        entries[9],
        [
            Entry::Text("vtium-config VERSION".to_owned()),
            Entry::Text(
                "vtium-config sets up the vtium console: its fonts and frits, its settings and heads, checked before they are saved."
                    .to_owned()
            ),
        ]
    );
    // the logo's rows, which style themselves, stay rows of the logo
    let logo = topics::entries("\x1b[1mvtium\x1b[0m\nvtium-config 1\n");
    assert_eq!(logo[9][0], Entry::Art("\x1b[1mvtium\x1b[0m".to_owned()));
}

#[test]
fn a_key_is_written_in_brackets_in_the_action_bar_s_form() {
    let row = |id: &str| contract::row(id).expect("the row");
    assert_eq!(topics::keys_of(row("ctrl-s.view")), ["[Ctrl+S]"]);
    assert_eq!(
        topics::keys_of(row("alt-tab.view")),
        [
            "[Alt+N]", "[Alt+E]", "[Alt+L]", "[Alt+O]", "[Alt+S]", "[Alt+H]"
        ]
    );
    assert_eq!(topics::keys_of(row("pgdn.view.help")), ["[PgDn]"]);
    assert_eq!(topics::keys_of(row("char.typing.search")), ["any letter"]);
}

#[test]
fn a_level_s_topic_is_its_tab_s() {
    for (node, topic) in [
        ("fonts", Some(0)),
        ("fonts.font", Some(0)),
        ("effects.effect", Some(1)),
        ("looks.look.list", Some(2)),
        ("looks-resources", Some(2)),
        ("looks.palette", Some(2)),
        ("flow.card", Some(3)),
        ("settings.section", Some(4)),
        ("help", Some(5)),
        ("any.menu.file", None),
        ("no.such.node", None),
    ] {
        assert_eq!(topics::for_node(node), topic, "{node}");
    }
}

#[test]
fn the_search_matches_every_line_by_its_topic_s_name_and_its_words() {
    let entries = entries();
    let items = topics::items(&entries);
    for (topic, list) in entries.iter().enumerate() {
        for (entry, e) in list.iter().enumerate() {
            let lines = match e {
                Entry::Grid(cells) => cells.len(),
                Entry::Text(_) | Entry::Key { .. } | Entry::Command { .. } => 1,
                Entry::Heading(_) | Entry::Art(_) | Entry::Blank => 0,
            };
            let found = items
                .iter()
                .filter(|i| i.topic == topic && i.entry == entry)
                .count();
            assert_eq!(found, lines, "topic {topic}, entry {entry}: {e:?}");
        }
    }
    for item in &items {
        assert!(item.text.starts_with(TOPICS[item.topic].name), "{item:?}");
    }
    assert!(
        items
            .iter()
            .any(|i| i.topic == 0 && i.text.contains("[Space]") && i.text.contains("activates"))
    );
}
