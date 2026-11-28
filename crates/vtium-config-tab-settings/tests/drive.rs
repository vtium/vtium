//! The Settings tab driven through its `Tab` methods, on the screen map's
//! machine and on the large machine: every key reachable and settable,
//! the list box's pick as one change that Undo takes back, a typed value
//! and its refusal, the `?` search by name and by description, F1's whole
//! help, Find's items and its request, the clicks on the places the tab
//! returns, and every row of each of the tab's nodes.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", "The list box", "F1, help for
//!   what is highlighted" and "Search, on ?".
//! - ADR-1206 D2, D9: the tab returns commands, and the shell holds it as
//!   a trait object.

mod common;

use common::{
    act, click, machines, open_search, repeat, screen, screen_searching, search_key, search_type,
    snapshot, type_in,
};
use vtium_config_core::commands::value_text;
use vtium_config_core::document::{ConfEdit, Done, Edit};
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Change, Command, Shared};
use vtium_config_tab_settings::Settings;
use vtium_config_widgets::contract;
use vtium_config_widgets::input::Key;
use vtium_config_widgets::keys::{self as widget_keys, Code};
use vtium_config_widgets::search::{Outcome, Search};
use vtium_config_widgets::tab::{Searchable, Tab};
use vtium_settings::keys::{self, Kind};

/// Goes from the list of sections to the key numbered `at`.
fn go_to(tab: &mut Settings, shared: &Shared, at: usize) {
    screen(tab, shared, 100, 30, true);
    let entry = tab.table().entries[at];
    let first = tab.table().sections[entry.section].keys.start;
    repeat(tab, shared, "down.view", entry.section);
    act(tab, shared, "enter.view.settings");
    repeat(tab, shared, "down.layer.settings-section", at - first);
    screen(tab, shared, 100, 30, true);
}

/// The one top-level edit that `commands` holds: the key and its value.
fn top_edit(commands: &[Command]) -> Option<(&'static str, Option<String>)> {
    let [Command::Change(Change::Document(change))] = commands else {
        return None;
    };
    let Edit::Conf(edits) = &change.edit else {
        return None;
    };
    match edits.as_slice() {
        [ConfEdit::Top { key, value }] => Some((key, value.clone())),
        _ => None,
    }
}

/// Makes the commands' changes on `shared`, as the shell's one change
/// function does.
fn apply(shared: &mut Shared, commands: Vec<Command>) -> Vec<Result<Done, String>> {
    commands
        .into_iter()
        .filter_map(|c| match c {
            Command::Change(Change::Document(change)) => Some(shared.change(change)),
            _ => None,
        })
        .collect()
}

/// A value of `key`'s kind that vtium.conf holds and that differs from
/// `now`.
fn sample(key: &keys::Key, now: &str) -> String {
    let pick = |a: String, b: String| if a == now { b } else { a };
    match key.kind {
        Kind::Number { min, max, .. } => pick(min.to_string(), max.to_string()),
        Kind::Path => pick("/tmp/sample".into(), "/tmp/other".into()),
        Kind::Paths => pick("/tmp/a:/tmp/b".into(), "/tmp/c".into()),
        Kind::Rgb => pick("1,2,3".into(), "4,5,6".into()),
        Kind::Mode => pick("1024x768".into(), "800x600".into()),
        Kind::Command => pick("/bin/sh".into(), "/bin/bash".into()),
        Kind::KeyBinding => pick("<Ctrl><Alt>F12".into(), "<Ctrl><Alt>F11".into()),
        Kind::Char => pick("x".into(), "y".into()),
        Kind::Size { .. } => pick("8x8".into(), "9x9".into()),
        Kind::Cell => pick("8x16".into(), "9x18".into()),
        Kind::DecoColor => pick("fg".into(), "1".into()),
        _ => pick("sample".into(), "other".into()),
    }
}

#[test]
fn every_key_today_s_tab_shows_is_reachable_and_settable() {
    for (machine, build) in machines() {
        let count = Settings::new(true).table().entries.len();
        assert!(count > 100, "only {count} keys");
        for at in 0..count {
            let mut shared = build();
            let mut tab = Settings::new(true);
            go_to(&mut tab, &shared, at);
            let key = tab.key().expect("a key is highlighted");
            act(&mut tab, &shared, "enter.layer.settings-section");
            let made = match tab.node() {
                "settings.section.list" => {
                    screen(&tab, &shared, 100, 30, true);
                    let first = act(&mut tab, &shared, "down.layer.list");
                    assert!(first.is_empty());
                    let made = act(&mut tab, &shared, "enter.layer.list");
                    // the pick moved off the value the key has now, unless
                    // that was the last choice
                    made
                }
                "settings.section.typing" => {
                    let now = value_text(key, Some(shared.document().text())).0;
                    repeat(
                        &mut tab,
                        &shared,
                        "backspace.typing",
                        now.chars().count() + 40,
                    );
                    type_in(&mut tab, &shared, "char.typing", &sample(key, &now));
                    act(&mut tab, &shared, "enter.typing.settings")
                }
                other => panic!("{}: Enter opened {other}", key.name),
            };
            let edit = top_edit(&made);
            assert_eq!(
                edit.as_ref().map(|e| e.0),
                Some(key.name),
                "{machine}: {} made {made:?}",
                key.name
            );
            for done in apply(&mut shared, made) {
                assert!(
                    done.is_ok(),
                    "{machine}: {} was refused: {done:?}",
                    key.name
                );
            }
            assert_eq!(tab.node(), "settings.section", "{}", key.name);
        }
    }
}

#[test]
fn a_pick_is_one_change_that_undo_takes_back() {
    for (_, build) in machines() {
        let mut shared = build();
        let mut tab = Settings::new(true);
        let at = tab
            .table()
            .entries
            .iter()
            .position(|e| e.key.name == "glyph-source")
            .expect("glyph-source");
        go_to(&mut tab, &shared, at);
        act(&mut tab, &shared, "enter.layer.settings-section");
        assert_eq!(tab.node(), "settings.section.list");
        repeat(&mut tab, &shared, "end.layer.list", 1);
        let made = act(&mut tab, &shared, "enter.layer.list");
        assert_eq!(
            top_edit(&made),
            Some(("glyph-source", Some("auto".to_owned())))
        );
        let steps = shared.documents.history().made();
        assert_eq!(apply(&mut shared, made), [Ok(Done::Made)]);
        assert_eq!(shared.documents.history().made(), steps + 1);
        let now = |s: &Shared| value_text(&keys::GLYPH_SOURCE, Some(s.document().text()));
        assert_eq!(now(&shared), ("auto".to_owned(), true));
        shared.undo();
        assert_eq!(now(&shared), ("unset".to_owned(), false));
        // unset leaves the key at its default, as one change too
        act(&mut tab, &shared, "enter.layer.settings-section");
        repeat(&mut tab, &shared, "end.layer.list", 1);
        let made = act(&mut tab, &shared, "enter.layer.list");
        apply(&mut shared, made);
        act(&mut tab, &shared, "enter.layer.settings-section");
        repeat(&mut tab, &shared, "home.layer.list", 1);
        let made = act(&mut tab, &shared, "enter.layer.list");
        assert_eq!(top_edit(&made), Some(("glyph-source", None)));
        assert_eq!(apply(&mut shared, made), [Ok(Done::Made)]);
        assert!(!now(&shared).1);
        // Esc closes the list box and changes nothing
        act(&mut tab, &shared, "enter.layer.settings-section");
        assert!(act(&mut tab, &shared, "esc.layer.list").is_empty());
        assert_eq!(tab.node(), "settings.section");
    }
}

#[test]
fn a_value_vtium_conf_cannot_hold_keeps_the_entry_open_and_says_why() {
    let shared = fixture::padnoir::shared();
    let mut tab = Settings::new(true);
    let index = |tab: &Settings, name: &str| {
        tab.table()
            .entries
            .iter()
            .position(|e| e.key.name == name)
            .expect("the key is shown")
    };
    let at = index(&tab, "font-frits");
    go_to(&mut tab, &shared, at);
    act(&mut tab, &shared, "enter.layer.settings-section");
    assert_eq!(tab.node(), "settings.section.typing");
    repeat(&mut tab, &shared, "backspace.typing", 40);
    type_in(&mut tab, &shared, "char.typing", "fonts");
    let made = act(&mut tab, &shared, "enter.typing.settings");
    assert!(
        matches!(made.as_slice(), [Command::Change(Change::Say(m))] if m.line.contains("font-frits")),
        "{made:?}"
    );
    assert_eq!(tab.node(), "settings.section.typing");
    // q is typed, and F1 has nothing to open while a value is typed
    tab.update(&shared, Action::key("q.typing", Key::Char('q')));
    assert_eq!(tab.full_text(&shared), None);
    assert!(act(&mut tab, &shared, "esc.typing").is_empty());
    assert_eq!(tab.node(), "settings.section");
    // an empty value goes back to the default
    let mut tab = Settings::new(true);
    let at = index(&tab, "font-size");
    go_to(&mut tab, &shared, at);
    act(&mut tab, &shared, "enter.layer.settings-section");
    repeat(&mut tab, &shared, "backspace.typing", 10);
    let made = act(&mut tab, &shared, "enter.typing.settings");
    assert_eq!(top_edit(&made), Some(("font-size", None)));
}

/// A key of the search row with no modifier.
fn plain(code: Code) -> widget_keys::Key {
    widget_keys::Key::plain(code)
}

#[test]
fn the_search_matches_a_description_word_as_well_as_a_key_s_name() {
    for (_, build) in machines() {
        let shared = build();
        let mut tab = Settings::new(true);
        screen(&tab, &shared, 100, 30, true);
        let mut search = Some(open_search(&tab));
        search_type(&mut tab, &shared, &mut search, "*bright");
        assert_eq!(tab.key().map(|k| k.name), Some("display-depth"));
        let names = |tab: &Settings, search: &Option<Search>| -> Vec<&'static str> {
            let s = search.as_ref().expect("the search is open");
            s.matches()
                .iter()
                .map(|&m| tab.table().entries[m].key.name)
                .collect()
        };
        assert!(
            names(&tab, &search).contains(&"display-depth"),
            "{:?}",
            names(&tab, &search)
        );
        assert!(
            !keys::DISPLAY_DEPTH.name.contains("bright"),
            "display-depth matched by its name"
        );
        // a key's name
        for _ in 0..7 {
            search_key(&mut tab, &shared, &mut search, &plain(Code::Backspace));
        }
        search_type(&mut tab, &shared, &mut search, "^font-n");
        assert_eq!(names(&tab, &search), ["font-name"]);
        // Enter keeps the filter, and the section's keys act on it
        search_key(&mut tab, &shared, &mut search, &plain(Code::Enter));
        assert_eq!(tab.node(), "settings.section");
        assert_eq!(tab.key().map(|k| k.name), Some("font-name"));
        // `?` resumes the kept filter on the tab's key
        let resumed = search_key(&mut tab, &shared, &mut search, &widget_keys::Key::char('?'));
        assert_eq!(resumed, Outcome::Resumed);
        assert_eq!(
            search.as_ref().and_then(Search::highlighted),
            Searchable::highlighted(&tab)
        );
        search_key(&mut tab, &shared, &mut search, &plain(Code::Enter));
        // the first Esc clears the filter, the second goes to the sections
        search_key(&mut tab, &shared, &mut search, &plain(Code::Escape));
        assert!(search.is_none());
        assert_eq!(tab.node(), "settings.section");
        assert_eq!(tab.key().map(|k| k.name), Some("font-name"));
        act(&mut tab, &shared, "esc.layer.settings-section");
        assert_eq!(tab.node(), "settings");
        // Backspace on an empty entry closes the row
        let mut search = Some(open_search(&tab));
        search_key(&mut tab, &shared, &mut search, &plain(Code::Backspace));
        assert!(search.is_none());
    }
}

#[test]
fn the_search_opens_on_the_tab_s_highlight_and_a_kept_filter_s_keys_move_among_its_matches() {
    let shared = fixture::padnoir::shared();
    let mut tab = Settings::new(true);
    screen(&tab, &shared, 100, 30, true);
    // on the list of sections, the highlighted section's first key
    repeat(&mut tab, &shared, "down.view", 1);
    let first = tab.table().sections[1].keys.start;
    assert_eq!(Searchable::highlighted(&tab), Some(first));
    assert_eq!(open_search(&tab).highlighted(), Some(first));
    // on the keys, the highlighted key
    act(&mut tab, &shared, "enter.view.settings");
    repeat(&mut tab, &shared, "down.layer.settings-section", 2);
    assert_eq!(Searchable::highlighted(&tab), Some(first + 2));
    // the tab keeps the copy it is handed, and lists only what matches
    let mut search = Some(open_search(&tab));
    search_type(&mut tab, &shared, &mut search, "*color");
    search_key(&mut tab, &shared, &mut search, &plain(Code::Enter));
    let matches = search
        .as_ref()
        .map(|s| s.matches().to_vec())
        .unwrap_or_default();
    assert!(matches.len() > 1, "{matches:?}");
    let lit = |tab: &Settings| {
        tab.table()
            .entries
            .iter()
            .position(|e| tab.key().is_some_and(|k| k.name == e.key.name))
    };
    let shell = search.as_ref().and_then(Search::highlighted);
    assert_eq!(lit(&tab), shell);
    let at = matches.iter().position(|&m| Some(m) == shell);
    let next = at.and_then(|at| matches.get(at + 1)).copied();
    assert!(next.is_some(), "{matches:?} past {shell:?}");
    // with the filter kept, Down moves the tab's highlight to the next
    // match, which the shell's search does not see
    act(&mut tab, &shared, "down.layer.settings-section");
    assert_eq!(lit(&tab), next);
    assert_eq!(Searchable::highlighted(&tab), next);
    assert_eq!(search.as_ref().and_then(Search::highlighted), shell);
    // a click on another section that keeps a match puts the copy's
    // highlight on the section's first match, so Down moves on from there
    screen(&tab, &shared, 100, 30, true);
    let (row, section) = *tab
        .places(&shared)
        .sections
        .iter()
        .find(|(_, s)| *s != tab.section())
        .expect("another section keeps a match");
    let first = matches
        .iter()
        .position(|&m| tab.table().entries[m].section == section)
        .expect("the section keeps a match");
    click(&mut tab, &shared, "click-row.view", row.x + 2, row.y);
    assert_eq!(tab.section(), section);
    assert_eq!(lit(&tab), matches.get(first).copied());
    act(&mut tab, &shared, "down.layer.settings-section");
    assert_eq!(lit(&tab), matches.get(first + 1).copied());
    let next = lit(&tab);
    // the tab's own search is only a list box's
    assert!(tab.search().is_none(), "the ? search is the shell's");
    // `None` clears the filter, and the highlight stays
    tab.filtered(&shared, None);
    assert_eq!(lit(&tab), next);
    let text = common::rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(!text.contains("matching"), "{text}");
    // `?` reaches the tab as a row that does nothing: the shell opens the
    // search
    assert!(act(&mut tab, &shared, "question.layer").is_empty());
    assert!(tab.search().is_none());
}

#[test]
fn a_search_that_matches_nothing_says_so_and_enter_opens_nothing() {
    let shared = fixture::padnoir::shared();
    let mut tab = Settings::new(true);
    screen(&tab, &shared, 80, 25, true);
    let mut search = Some(open_search(&tab));
    search_type(&mut tab, &shared, &mut search, "^qqzz");
    assert_eq!(tab.key(), None);
    let text = common::rows_of(&screen_searching(
        &tab,
        &shared,
        search.as_ref(),
        80,
        25,
        true,
    ))
    .join("\n");
    assert!(
        text.contains("No setting matches. [Esc] clears the search."),
        "{text}"
    );
    search_key(&mut tab, &shared, &mut search, &plain(Code::Enter));
    assert!(act(&mut tab, &shared, "enter.layer.settings-section").is_empty());
    assert_eq!(tab.node(), "settings.section");
    assert_eq!(tab.full_text(&shared), None);
}

#[test]
fn up_and_down_in_the_search_move_through_the_matches_across_sections() {
    let shared = fixture::large::shared();
    let mut tab = Settings::new(true);
    screen(&tab, &shared, 80, 25, true);
    let mut search = Some(open_search(&tab));
    search_type(&mut tab, &shared, &mut search, "*color");
    let first = tab.section();
    let mut sections = vec![first];
    for _ in 0..40 {
        search_key(&mut tab, &shared, &mut search, &plain(Code::Down));
        sections.push(tab.section());
    }
    let key = tab.key().expect("a match is highlighted");
    assert!(
        key.name.contains("color") || key.what.to_lowercase().contains("color"),
        "{}",
        key.name
    );
    assert!(sections.iter().any(|&s| s != first), "{sections:?}");
}

#[test]
fn f1_on_a_key_opens_its_whole_help_with_its_default_and_its_value_now() {
    let shared = fixture::padnoir::shared();
    let mut tab = Settings::new(true);
    let at = tab
        .table()
        .entries
        .iter()
        .position(|e| e.key.name == "font-name")
        .expect("font-name");
    go_to(&mut tab, &shared, at);
    let text = tab.full_text(&shared).expect("F1 has text on a key");
    assert!(text.starts_with("Settings › Fonts and text › font-name\n"));
    for paragraph in keys::FONT_NAME.paragraphs() {
        assert!(text.contains(paragraph), "{paragraph}");
    }
    assert!(text.contains("Default    unset"), "{text}");
    assert!(text.contains("Now        unset, default"), "{text}");
    snapshot("settings-about-key", &(text + "\n"));
}

#[test]
fn f1_on_a_section_whose_description_the_foot_cut_opens_it_whole() {
    let shared = fixture::padnoir::shared();
    let tab = Settings::new(true);
    screen(&tab, &shared, 80, 25, true);
    let text = tab
        .full_text(&shared)
        .expect("the description is cut at 80 columns");
    assert!(text.starts_with("Settings › Fonts and text\n\n"));
    assert!(text.ends_with(keys::SECTIONS[0].about));
}

#[test]
fn find_lists_every_key_and_its_enter_highlights_the_key_in_its_section() {
    for (_, build) in machines() {
        let shared = build();
        let mut tab = Settings::new(true);
        let items = tab.find_items(&shared);
        assert_eq!(items.len(), tab.table().entries.len());
        let item = items
            .iter()
            .find(|i| i.name == "display-depth")
            .expect("display-depth is found");
        assert_eq!(item.place, "Settings › Displays");
        assert_eq!(item.fact, "auto, default");
        assert!(
            item.description
                .as_deref()
                .is_some_and(|d| d.contains("brightness"))
        );
        tab.update(&shared, Action::item("enter.typing.find", item.index));
        assert_eq!(tab.node(), "settings.section");
        assert_eq!(tab.key().map(|k| k.name), Some("display-depth"));
    }
}

#[test]
fn clicks_land_on_the_places_the_tab_returns() {
    for (_, build) in machines() {
        let mut shared = build();
        let mut tab = Settings::new(true);
        screen(&tab, &shared, 80, 25, true);
        let places = tab.places(&shared);
        assert_eq!(places.sections.len(), tab.table().sections.len());
        // a section that holds a switch, whose list box the clicks open
        let switch = tab
            .table()
            .entries
            .iter()
            .position(|e| e.key.kind == Kind::Switch && e.section > 0)
            .expect("a switch");
        let (row, section) = places.sections[tab.table().entries[switch].section];
        assert!(click(&mut tab, &shared, "click-row.view", row.x + 2, row.y).is_empty());
        assert_eq!((tab.section(), tab.node()), (section, "settings"));
        click(&mut tab, &shared, "click-row.view", row.x + 2, row.y);
        assert_eq!(tab.node(), "settings.section");
        screen(&tab, &shared, 80, 25, true);
        // a key: the first click highlights it, the second opens it
        let places = tab.places(&shared);
        let (row, at) = places.keys[1];
        click(&mut tab, &shared, "click-row.view", row.x + 1, row.y);
        assert_eq!(
            tab.key().map(|k| k.name),
            Some(tab.table().entries[at].key.name)
        );
        // a key with values, so its list box opens on the second click
        tab.update(&shared, Action::item("click-row.view", switch));
        screen(&tab, &shared, 80, 25, true);
        let (row, _) = *tab
            .places(&shared)
            .keys
            .iter()
            .find(|(_, k)| *k == switch)
            .expect("the switch shows");
        click(&mut tab, &shared, "click-row.view", row.x + 1, row.y);
        assert_eq!(tab.node(), "settings.section.list");
        screen(&tab, &shared, 80, 25, true);
        let list = tab.places(&shared).list.expect("the list box shows");
        let (on, _) = list.rows[0];
        let (off, _) = list.rows[1];
        // the choice not picked: the first click highlights, the second keeps
        let target = if tab
            .key()
            .is_some_and(|k| value_text(k, Some(shared.document().text())).0 == "on")
        {
            off
        } else {
            on
        };
        assert!(
            click(
                &mut tab,
                &shared,
                "click-choice.layer.list",
                target.x + 1,
                target.y
            )
            .is_empty()
        );
        let made = click(
            &mut tab,
            &shared,
            "click-choice.layer.list",
            target.x + 1,
            target.y,
        );
        assert!(top_edit(&made).is_some(), "{made:?}");
        assert_eq!(apply(&mut shared, made), [Ok(Done::Made)]);
    }
}

#[test]
fn a_name_s_list_box_ends_with_other_which_lets_the_person_type_one() {
    let shared = fixture::padnoir::shared();
    let mut tab = Settings::new(true);
    let at = tab
        .table()
        .entries
        .iter()
        .position(|e| matches!(e.key.kind, Kind::Name(_)))
        .expect("a key that takes a name");
    go_to(&mut tab, &shared, at);
    act(&mut tab, &shared, "enter.layer.settings-section");
    screen(&tab, &shared, 100, 30, true);
    repeat(&mut tab, &shared, "end.layer.list", 1);
    assert!(act(&mut tab, &shared, "enter.layer.list").is_empty());
    assert_eq!(tab.node(), "settings.section.typing");
}

/// The nodes the tab is on, each reached from a fresh tab by `rows`.
const NODES: [(&str, &[&str]); 4] = [
    ("settings", &[]),
    ("settings.section", &["enter.view.settings"]),
    (
        "settings.section.typing",
        &["enter.view.settings", "enter.layer.settings-section"],
    ),
    (
        "settings.section.list",
        &[
            "enter.view.settings",
            "down.layer.settings-section",
            "down.layer.settings-section",
            "down.layer.settings-section",
            "down.layer.settings-section",
            "down.layer.settings-section",
            "down.layer.settings-section",
            "down.layer.settings-section",
            "down.layer.settings-section",
            "enter.layer.settings-section",
        ],
    ),
];

#[test]
fn every_row_of_each_node_acts_or_does_nothing_and_only_a_pick_changes_the_document() {
    for (_, build) in machines() {
        for (node, path) in NODES {
            let rows: Vec<&'static str> = contract::node(node)
                .expect("the tab's node")
                .rows()
                .map(|r| r.id)
                .collect();
            for row in rows {
                let shared = build();
                let mut tab = Settings::new(true);
                screen(&tab, &shared, 80, 25, true);
                for step in path {
                    act(&mut tab, &shared, step);
                }
                screen(&tab, &shared, 80, 25, true);
                assert_eq!(tab.node(), node);
                assert_eq!(
                    tab.bar(&shared).iter().map(|r| r.id).collect::<Vec<_>>(),
                    contract::buttons(node)
                        .iter()
                        .map(|r| r.id)
                        .collect::<Vec<_>>()
                );
                for action in [Action::of(row), Action::key(row, Key::Char('a'))] {
                    let made = tab.update(&shared, action);
                    let changes = made
                        .iter()
                        .filter(|c| matches!(c, Command::Change(Change::Document(_))))
                        .count();
                    let picks = row.starts_with("enter.layer.list")
                        || row.starts_with("enter.typing.settings");
                    assert!(
                        changes == 0 || picks,
                        "{node}: {row} changed the document: {made:?}"
                    );
                    screen(&tab, &shared, 80, 25, true);
                }
            }
        }
    }
}

#[test]
fn the_shell_holds_the_tab_as_a_trait_object_with_its_search() {
    let shared = fixture::padnoir::shared();
    let tab: Box<dyn Tab<Shared, Action, Command>> = Box::new(Settings::new(true));
    assert_eq!(
        (tab.name(), tab.letter(), tab.node()),
        ("Settings", 's', "settings")
    );
    let search = tab.searchable().expect("Settings offers the search");
    assert_eq!(search.fields(), ["key", "description"]);
    assert_eq!(search.count(), Settings::new(true).table().entries.len());
    assert!(!contract::search(tab.node()).is_empty());
    assert!(!contract::search("settings.section").is_empty());
    assert!(tab.menu_items(&shared).is_empty());
}
