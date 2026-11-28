//! The Fonts tab driven through its `Tab` methods on the screen map's
//! machine and on the large one: every row of the tab's nodes, Space and
//! Activate missing and the changes they return, the action bar's
//! button and its count, Try live's command, the picture's switch and
//! the preview, the filters, the shell's search handed to the tab, the
//! detail and the install of a font of your own, the clicks on the places
//! the tab returns, F1's text and Find's items, and that a key that
//! filters or searches reads no folder and builds nothing again on 3,000
//! fonts.
//!
//! References:
//! - ADR-1206 D9: the tab implements `Tab` and `Searchable` over the
//!   shared state, the action and the commands.
//! - ADR-1206 D3: the navigation state holds the search, and each tab
//!   keeps the indexes that match.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   Activate missing, its count and its message, and fonts turned off on
//!   purpose staying off.
//! - ADR-1206 D10: nothing scans the font folders on a keypress.

mod common;

use std::path::PathBuf;

use common::{
    DEJAVU, act, apply, bar, click, every_state, machines, open_search, rows_of, screen,
    search_key, search_type, viewer,
};
use vtium_config_core::batch::Change as Batch;
use vtium_config_core::document::Edit;
use vtium_config_core::found;
use vtium_config_core::found::fonts::Filter;
use vtium_config_core::preview::Shows;
use vtium_config_core::ui::{Action, Change, Command, Effect, Op, Request, Shared};
use vtium_config_tab_fonts::list::Sel;
use vtium_config_tab_fonts::states::{self, Item};
use vtium_config_tab_fonts::{Fonts, words};
use vtium_config_widgets::contract;
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{Searchable, Tab};

/// The screen map's machine.
fn map() -> Shared {
    vtium_config_core::fixture::padnoir::shared()
}

/// Highlights the font whose file name is `name`, through Find.
fn go_to(tab: &mut Fonts, shared: &Shared, name: &str) {
    screen(tab, shared, 100, 30, true);
    let Some(at) = tab
        .find_items(shared)
        .into_iter()
        .find(|i| i.name == name)
        .map(|i| i.index)
    else {
        panic!("no font {name}");
    };
    assert!(
        tab.update(shared, Action::item("enter.typing.find", at))
            .is_empty()
    );
}

/// The document changes `commands` make, by their edits.
fn edits(commands: &[Command]) -> Vec<Edit> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::Change(Change::Document(d)) => Some(d.edit.clone()),
            _ => None,
        })
        .collect()
}

/// The message line `commands` say, where they say one.
fn said(commands: &[Command]) -> Option<String> {
    commands.iter().find_map(|c| match c {
        Command::Change(Change::Say(m)) => Some(m.line.clone()),
        _ => None,
    })
}

#[test]
fn every_row_of_the_tab_s_nodes_acts_or_does_nothing_and_only_space_and_a_change_the_document() {
    for (machine, build) in machines() {
        for node in ["fonts", "fonts.filter", "fonts.font"] {
            let Some(n) = contract::node(node) else {
                panic!("no node {node}");
            };
            for row in n.rows() {
                let mut shared = build();
                every_state(&mut shared);
                let mut tab = Fonts::new(true);
                screen(&tab, &shared, 100, 30, true);
                // open the level the node is
                match node {
                    "fonts.filter" => {
                        act(&mut tab, &shared, "enter.view.fonts");
                    }
                    "fonts.font" => {
                        act(&mut tab, &shared, "end.view");
                        act(&mut tab, &shared, "enter.view.fonts");
                    }
                    _ => {
                        act(&mut tab, &shared, "down.view");
                        act(&mut tab, &shared, "down.view");
                        act(&mut tab, &shared, "down.view");
                    }
                }
                assert_eq!(tab.node(), node, "{machine}: {node} did not open");
                let made = tab.update(&shared, Action::of(row.id));
                let changes = edits(&made);
                let allowed = matches!(row.id, "space.view.resources" | "shift-a.view.fonts");
                assert!(
                    changes.is_empty() || allowed,
                    "{machine}: {} on {node} changed the document: {changes:?}",
                    row.id
                );
                screen(&tab, &shared, 100, 30, true);
            }
        }
    }
}

#[test]
fn space_activates_a_font_that_is_not_active_as_one_change() {
    let mut shared = map();
    every_state(&mut shared);
    let mut tab = Fonts::new(true);
    go_to(&mut tab, &shared, "ter-u16n.otb");
    let made = act(&mut tab, &shared, "space.view.resources");
    assert_eq!(
        edits(&made),
        [Edit::Active {
            path: PathBuf::from("/usr/share/fonts/misc/ter-u16n.otb"),
            change: Some(Batch::Activate),
        }]
    );
    let steps = shared.documents.history().steps().len();
    apply(&mut shared, made);
    assert_eq!(shared.documents.history().steps().len(), steps + 1);
    screen(&tab, &shared, 100, 30, true);
    let at = tab.known(&shared).position(Item::File(1));
    assert!(at.is_some_and(|at| tab.known(&shared).active(at)));
}

#[test]
fn space_on_a_font_turned_off_on_purpose_turns_it_on_and_on_an_active_one_asks_first() {
    let mut shared = map();
    every_state(&mut shared);
    let mut tab = Fonts::new(true);
    // the document deactivates DejaVu, so Space takes the document's own
    // change back, to what the batch file holds
    go_to(&mut tab, &shared, "DejaVuSansMono.ttf");
    let made = act(&mut tab, &shared, "space.view.resources");
    assert_eq!(
        edits(&made),
        [Edit::Active {
            path: PathBuf::from(DEJAVU),
            change: None,
        }]
    );
    apply(&mut shared, made);
    screen(&tab, &shared, 100, 30, true);
    // DejaVu is active again, and the look big uses it, so Space asks
    let made = act(&mut tab, &shared, "space.view.resources");
    let [Command::Request(Request::Ask { question, then })] = made.as_slice() else {
        panic!("{made:?}");
    };
    assert!(
        question.contains("Deactivate DejaVuSansMono.ttf? big uses it"),
        "{question}"
    );
    let made = tab.update(&shared, then.clone());
    assert_eq!(
        edits(&made),
        [Edit::Active {
            path: PathBuf::from(DEJAVU),
            change: Some(Batch::Deactivate),
        }]
    );
}

#[test]
fn space_says_why_it_cannot_change_a_font_of_your_own_one_that_fails_trust_or_one_not_installed() {
    let mut shared = map();
    every_state(&mut shared);
    let mut tab = Fonts::new(true);
    for (name, word) in [
        ("Mine-Regular.ttf", "in your own folder"),
        ("NotoColorEmoji.ttf", "fails the trust rule"),
        ("JetBrains Mono", "is not installed"),
    ] {
        go_to(&mut tab, &shared, name);
        let made = act(&mut tab, &shared, "space.view.resources");
        assert!(edits(&made).is_empty(), "{name}: {made:?}");
        let line = said(&made).unwrap_or_default();
        assert!(line.contains(word), "{name}: {line}");
    }
    let mut shared = map();
    let mut tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    let made = act(&mut tab, &shared, "space.view.resources");
    assert_eq!(said(&made).as_deref(), Some(words::SPACE_ON_FILTER));
    shared.message = None;
}

#[test]
fn activate_missing_is_one_change_that_leaves_fonts_turned_off_on_purpose_off() {
    for (machine, build) in machines() {
        let mut shared = build();
        every_state(&mut shared);
        let mut tab = Fonts::new(true);
        screen(&tab, &shared, 100, 30, true);
        let made = act(&mut tab, &shared, "shift-a.view.fonts");
        let changes = edits(&made);
        let [Edit::All(all)] = changes.as_slice() else {
            panic!("{machine}: {made:?}");
        };
        assert_eq!(all.len(), 1, "{machine}: {all:?}");
        let [Edit::Active { path, change }] = all.as_slice() else {
            panic!("{all:?}");
        };
        assert_eq!(*change, Some(Batch::Activate));
        assert!(
            !path.ends_with("DejaVuSansMono.ttf") && !path.ends_with("Family0004-Regular.ttf"),
            "{machine}: a font turned off on purpose was activated"
        );
        let line = said(&made).unwrap_or_default();
        assert!(line.contains("1 stays off on purpose"), "{machine}: {line}");
        assert!(
            line.contains("cannot be activated ([F1])"),
            "{machine}: {line}"
        );
        assert!(line.chars().count() <= 80, "{line}");
        // the message comes first, so a refusal would replace it
        assert!(matches!(
            made.first(),
            Some(Command::Change(Change::Say(_)))
        ));
        apply(&mut shared, made);
        screen(&tab, &shared, 100, 30, true);
        assert_eq!(tab.known(&shared).counts.not_active, 0, "{machine}");
        // Undo takes it back as one step
        shared.undo();
        screen(&tab, &shared, 100, 30, true);
        assert_eq!(tab.known(&shared).counts.not_active, 1, "{machine}");
    }
}

#[test]
fn enter_on_the_configuration_s_row_does_what_a_does() {
    let mut shared = map();
    every_state(&mut shared);
    let mut tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    act(&mut tab, &shared, "down.view");
    assert_eq!(tab.sel(), Sel::Activate);
    let by_enter = act(&mut tab, &shared, "enter.view.fonts");
    let by_a = act(&mut tab, &shared, "shift-a.view.fonts");
    assert_eq!(by_enter, by_a);
    assert_eq!(edits(&by_enter).len(), 1);
}

#[test]
fn the_bar_counts_activate_missing_while_a_font_waits_and_drops_it_otherwise() {
    let mut shared = map();
    let tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    let labels = |tab: &Fonts, shared: &Shared| -> Vec<String> {
        bar(tab, shared)
            .buttons
            .iter()
            .map(|b| format!("[{}] {}", b.key, b.label))
            .collect()
    };
    assert!(
        !labels(&tab, &shared).iter().any(|l| l.starts_with("[A]")),
        "{:?}",
        labels(&tab, &shared)
    );
    assert!(
        !tab.bar(&shared)
            .iter()
            .any(|r| r.id == "shift-a.view.fonts")
    );
    // the change counts at once, before the tab is drawn again
    every_state(&mut shared);
    assert!(
        tab.bar(&shared)
            .iter()
            .any(|r| r.id == "shift-a.view.fonts")
    );
    assert!(
        tab.bar_labels(&shared)
            .contains(&("shift-a.view.fonts", "Activate missing (1)".to_owned())),
        "{:?}",
        tab.bar_labels(&shared)
    );
    let rows = rows_of(&screen(&tab, &shared, 100, 30, true));
    assert!(labels(&tab, &shared).contains(&"[A] Activate missing (1)".to_owned()));
    let last = rows.last().cloned().unwrap_or_default();
    assert!(last.contains("[A] Activate missing (1)"), "{last}");
    // at 80 columns the bar moves it behind More, so it stays reachable
    let rows = rows_of(&screen(&tab, &shared, 80, 25, true));
    let last = rows.last().cloned().unwrap_or_default();
    assert!(last.contains("[.] More"), "{last}");
}

#[test]
fn space_s_button_says_deactivate_only_on_an_active_font_file() {
    let shared = map();
    let mut tab = Fonts::new(true);
    let space = |tab: &Fonts, shared: &Shared| {
        tab.bar_labels(shared)
            .into_iter()
            .find(|(id, _)| *id == "space.view.resources")
            .map(|(_, label)| label)
    };
    go_to(&mut tab, &shared, "DejaVuSansMono.ttf");
    assert_eq!(space(&tab, &shared).as_deref(), Some(words::DEACTIVATE));
    go_to(&mut tab, &shared, "ter-u16n.otb");
    assert_eq!(space(&tab, &shared), None, "ter-u16n.otb is not active");
    go_to(&mut tab, &shared, "unifont");
    assert_eq!(space(&tab, &shared), None, "a built-in font is not a file");
}

#[test]
fn t_returns_the_font_s_path_for_the_trial_or_says_why_the_console_would_refuse_it() {
    let mut shared = map();
    every_state(&mut shared);
    let mut tab = Fonts::new(true);
    go_to(&mut tab, &shared, "cullet-mono.otf");
    let made = act(&mut tab, &shared, "t.view.fonts");
    assert_eq!(
        made,
        [Command::Change(Change::Try(
            "/usr/share/vtium/fonts/cullet-mono.otf".to_owned()
        ))]
    );
    go_to(&mut tab, &shared, "ter-u16n.otb");
    let made = act(&mut tab, &shared, "t.view.fonts");
    assert_eq!(said(&made), Some(words::try_inactive("ter-u16n.otb")));
    go_to(&mut tab, &shared, "unifont");
    let made = act(&mut tab, &shared, "t.view.fonts");
    assert_eq!(made, [Command::Change(Change::Try("unifont".to_owned()))]);
}

#[test]
fn p_switches_the_picture_or_says_this_terminal_shows_none() {
    let mut shared = map();
    let mut tab = Fonts::new(true);
    go_to(&mut tab, &shared, "DejaVuSansMono.ttf");
    assert!(!tab.picture());
    assert!(act(&mut tab, &shared, "p.view.fonts").is_empty());
    assert!(tab.picture());
    let preview = tab.preview(&shared);
    assert_eq!(
        preview.as_ref().and_then(|p| p.font.as_deref()),
        Some(DEJAVU)
    );
    assert!(matches!(
        preview.as_ref().map(|p| p.shows),
        Some(Shows::Font { detail: false })
    ));
    assert_eq!(preview.as_ref().and_then(|p| p.in_document.clone()), None);
    // the picture goes inside the box under the list
    let view = viewer(&shared, &tab, 100, 30);
    let Some(place) = preview.and_then(|p| p.place) else {
        panic!("no place for the picture at 100×30");
    };
    assert!(
        place.x > view.x && place.y > view.y + 2,
        "{place:?} in {view:?}"
    );
    assert!(
        place.x + place.width < view.x + view.width,
        "{place:?} in {view:?}"
    );
    assert!(
        place.y + place.height < view.y + view.height,
        "{place:?} in {view:?}"
    );
    act(&mut tab, &shared, "p.view.fonts");
    assert!(matches!(
        tab.preview(&shared).map(|p| p.shows),
        Some(Shows::Font { detail: true })
    ));
    // the filter line previews nothing
    act(&mut tab, &shared, "home.view");
    assert!(tab.preview(&shared).is_none());
    shared.console.live = vtium_config_core::ui::Live::NotVtium;
    shared.console.graphics = false;
    let made = act(&mut tab, &shared, "p.view.fonts");
    assert_eq!(said(&made).as_deref(), Some(words::P_NO_IMAGES));
}

#[test]
fn a_filter_from_the_list_box_keeps_the_fonts_that_pass_it() {
    let shared = map();
    let mut tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    act(&mut tab, &shared, "enter.view.fonts");
    assert_eq!(tab.node(), "fonts.filter");
    screen(&tab, &shared, 100, 30, true);
    act(&mut tab, &shared, "down.layer.list");
    act(&mut tab, &shared, "enter.layer.list");
    assert_eq!(tab.filter(), Filter::Bitmap);
    assert_eq!(tab.node(), "fonts");
    let rows = rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(rows.contains("ter-u16n.otb"), "{rows}");
    assert!(!rows.contains("DejaVuSansMono.ttf"), "{rows}");
    // Esc closes the box and leaves the filter
    act(&mut tab, &shared, "enter.view.fonts");
    act(&mut tab, &shared, "esc.layer.list");
    assert_eq!(tab.filter(), Filter::Bitmap);
    assert_eq!(tab.node(), "fonts");
}

#[test]
fn the_search_matches_a_font_s_family_and_its_file_name() {
    let shared = map();
    let mut tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    let Some(s) = tab.searchable() else {
        panic!("the tab offers no search");
    };
    assert_eq!(s.fields(), ["family", "file"]);
    // `?` on the list is the shell's, and opens no search of the tab's
    act(&mut tab, &shared, "question.view");
    assert!(tab.filtered_by().is_none() && tab.search().is_none());
    let mut search = open_search(&mut tab, &shared);
    assert_eq!(
        tab.node(),
        "fonts",
        "the shell answers the search row's node"
    );
    // Terminus is ter-u16n.otb's family, which its file name does not say
    search_type(&mut tab, &shared, &mut search, "terminus");
    assert_eq!(tab.sel(), Sel::Item(Item::File(1)));
    let rows = rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(
        rows.contains("ter-u16n.otb") && !rows.contains("DejaVu"),
        "{rows}"
    );
    search_key(&mut tab, &shared, &mut search, Key::plain(Code::Enter));
    assert!(tab.filtered_by().is_some_and(|s| !s.typing()));
    // a kept filter's Esc reaches the shell's search, which clears it
    search_key(&mut tab, &shared, &mut search, Key::plain(Code::Escape));
    assert!(tab.filtered_by().is_none());
    let rows = rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(rows.contains("DejaVu"), "{rows}");
}

#[test]
fn a_kept_filter_moves_the_highlight_among_the_matches_and_resumes_on_it() {
    let shared = map();
    let mut tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    let mut search = open_search(&mut tab, &shared);
    search_type(&mut tab, &shared, &mut search, "mono");
    search_key(&mut tab, &shared, &mut search, Key::plain(Code::Enter));
    let matches = search.matches().to_vec();
    assert!(matches.len() > 2, "{matches:?}");
    let first = tab.highlighted();
    assert_eq!(first, matches.first().copied());
    // the list's own keys reach the tab while the filter is kept
    act(&mut tab, &shared, "down.view");
    let moved = tab.highlighted();
    assert_ne!(moved, first);
    assert!(moved.is_some_and(|at| matches.contains(&at)), "{moved:?}");
    // `?` resumes the search on the font the list highlights now
    search_key(&mut tab, &shared, &mut search, Key::char('?'));
    assert_eq!(search.highlighted(), moved);
    assert_eq!(tab.highlighted(), moved);
    assert!(tab.filtered_by().is_some_and(Search::typing));
    // the search closing lists every font again with the highlight kept
    tab.filtered(&shared, None);
    assert_eq!(tab.highlighted(), moved);
    assert!(tab.filtered_by().is_none());
}

#[test]
fn a_font_s_detail_scrolls_and_enter_installs_only_a_font_of_your_own() {
    let shared = map();
    let mut tab = Fonts::new(true);
    go_to(&mut tab, &shared, "Mine-Regular.ttf");
    assert_eq!(
        tab.system_font(&shared),
        Ok(PathBuf::from(vtium_config_core::fixture::found::OWN_FONT))
    );
    assert_eq!(
        tab.own_font(&shared),
        Some(PathBuf::from(vtium_config_core::fixture::found::OWN_FONT))
    );
    act(&mut tab, &shared, "enter.view.fonts");
    assert_eq!(tab.node(), "fonts.font");
    assert!(tab.bar(&shared).iter().any(|r| r.id == "enter.layer.font"));
    let made = act(&mut tab, &shared, "enter.layer.font");
    assert!(matches!(
        made.as_slice(),
        [Command::Change(Change::Say(_)), Command::Effect(Effect::Desk(Op::SystemFont(p)))]
            if p == &PathBuf::from(vtium_config_core::fixture::found::OWN_FONT)
    ));
    act(&mut tab, &shared, "esc.layer");
    assert_eq!(tab.node(), "fonts");
    go_to(&mut tab, &shared, "NotoColorEmoji.ttf");
    assert_eq!(tab.own_font(&shared), None, "not a font of your own");
    act(&mut tab, &shared, "enter.view.fonts");
    assert!(!tab.bar(&shared).iter().any(|r| r.id == "enter.layer.font"));
    let made = act(&mut tab, &shared, "enter.layer.font");
    assert_eq!(said(&made).as_deref(), Some(words::SYSTEM_FONT_NOT_YOURS));
    // a detail taller than the viewer scrolls, and stops at its last row
    screen(&tab, &shared, 80, 25, true);
    for _ in 0..40 {
        act(&mut tab, &shared, "down.layer.detail");
    }
    let rows = rows_of(&screen(&tab, &shared, 80, 25, true)).join("\n");
    assert!(
        rows.contains("NotoColorEmoji.ttf  Noto Color Emoji"),
        "{rows}"
    );
    assert!(rows.contains("can write them."), "{rows}");
}

#[test]
fn clicks_land_on_the_places_the_tab_returns() {
    let shared = map();
    let mut tab = Fonts::new(true);
    let drawn = rows_of(&screen(&tab, &shared, 100, 30, true));
    let places = tab.places(&shared);
    let Some(&(rect, sel)) = places
        .rows
        .iter()
        .find(|(_, s)| *s == Sel::Item(Item::File(1)))
    else {
        panic!("no row of ter-u16n.otb in {places:?}");
    };
    // the place is where the view drew the font's row
    let row = drawn.get(usize::from(rect.y)).cloned().unwrap_or_default();
    assert!(row.contains("ter-u16n.otb"), "{row}");
    assert!(click(&mut tab, &shared, "click-row.view", rect.x + 4, rect.y).is_empty());
    assert_eq!(tab.sel(), sel);
    // a second click on the highlighted row opens it, as Enter does
    click(&mut tab, &shared, "click-row.view", rect.x + 4, rect.y);
    assert_eq!(tab.node(), "fonts.font");
    act(&mut tab, &shared, "esc.layer");
    // the filters' list box takes a click on a choice
    act(&mut tab, &shared, "up.view");
    act(&mut tab, &shared, "up.view");
    act(&mut tab, &shared, "up.view");
    act(&mut tab, &shared, "enter.view.fonts");
    screen(&tab, &shared, 100, 30, true);
    let Some(boxed) = tab.places(&shared).filters else {
        panic!("no list box");
    };
    let Some(&(row, _)) = boxed.rows.get(3) else {
        panic!("{boxed:?}");
    };
    // the first click highlights the choice, and a click on the
    // highlighted choice keeps it
    click(
        &mut tab,
        &shared,
        "click-choice.layer.list",
        row.x + 2,
        row.y,
    );
    assert_eq!(tab.filter(), Filter::All);
    click(
        &mut tab,
        &shared,
        "click-choice.layer.list",
        row.x + 2,
        row.y,
    );
    assert_eq!(tab.filter(), Filter::Monospaced);
    assert_eq!(tab.node(), "fonts");
}

#[test]
fn f1_opens_the_facts_the_pane_cut_and_find_lists_every_font() {
    for (machine, build) in machines() {
        let shared = build();
        let mut tab = Fonts::new(true);
        screen(&tab, &shared, 80, 25, true);
        let items = tab.find_items(&shared);
        assert_eq!(items.len(), tab.known(&shared).items.len(), "{machine}");
        assert!(items.iter().all(|i| i.place.starts_with("Fonts › ")));
        let Some(first) = items.iter().find(|i| i.name.ends_with(".ttf")) else {
            panic!("{machine}: no .ttf");
        };
        assert!(
            tab.update(&shared, Action::item("enter.typing.find", first.index))
                .is_empty()
        );
        screen(&tab, &shared, 80, 25, true);
        let Some(text) = tab.full_text(&shared) else {
            panic!("{machine}: no full text at 80 columns");
        };
        assert!(text.contains("folder  /usr/"), "{machine}: {text}");
        assert!(!text.contains('…'), "{machine}: {text}");
    }
}

#[test]
fn the_tab_draws_only_into_the_viewer_it_is_given() {
    use ratatui_core::backend::TestBackend;
    use ratatui_core::layout::Rect;
    use ratatui_core::terminal::Terminal;
    for (machine, build) in machines() {
        let mut shared = build();
        every_state(&mut shared);
        let tab = Fonts::new(true);
        let area = Rect::new(3, 2, 90, 24);
        let Ok(mut terminal) = Terminal::new(TestBackend::new(100, 30));
        let drawn = terminal.draw(|f| {
            for cell in f.buffer_mut().content.iter_mut() {
                cell.set_symbol("#");
            }
            tab.draw(&shared, f, area);
        });
        assert!(drawn.is_ok());
        let buf = terminal.backend().buffer();
        for y in 0..30u16 {
            for x in 0..100u16 {
                if !area.contains((x, y).into()) {
                    let s = buf.cell((x, y)).map_or("", |c| c.symbol());
                    assert_eq!(s, "#", "{machine}: the tab drew at {x},{y}");
                }
            }
        }
        let _ = viewer(&shared, &tab, 100, 30);
    }
}

#[test]
fn a_filter_or_a_search_on_3000_fonts_reads_no_folder_and_builds_nothing_again() {
    let shared = vtium_config_core::fixture::large::shared();
    assert_eq!(
        shared.found.fonts.files().len(),
        vtium_config_core::fixture::found::LARGE_FONTS
    );
    let mut tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    let scans = found::scans();
    let builds = states::builds();
    act(&mut tab, &shared, "enter.view.fonts");
    screen(&tab, &shared, 100, 30, true);
    act(&mut tab, &shared, "down.layer.list");
    act(&mut tab, &shared, "down.layer.list");
    act(&mut tab, &shared, "enter.layer.list");
    screen(&tab, &shared, 100, 30, true);
    let mut search = open_search(&mut tab, &shared);
    for c in "family 00".chars() {
        search_key(&mut tab, &shared, &mut search, Key::char(c));
        screen(&tab, &shared, 100, 30, true);
    }
    for _ in 0..5 {
        search_key(&mut tab, &shared, &mut search, Key::plain(Code::Backspace));
        search_key(&mut tab, &shared, &mut search, Key::plain(Code::Down));
        screen(&tab, &shared, 100, 30, true);
    }
    search_key(&mut tab, &shared, &mut search, Key::plain(Code::Enter));
    for _ in 0..50 {
        act(&mut tab, &shared, "down.view");
    }
    screen(&tab, &shared, 80, 25, true);
    assert_eq!(found::scans(), scans, "a key read a folder");
    assert_eq!(
        states::builds(),
        builds,
        "a key built the fonts' states again"
    );
}

#[test]
fn a_change_to_the_document_builds_the_states_again_once() {
    let mut shared = map();
    let tab = Fonts::new(true);
    screen(&tab, &shared, 100, 30, true);
    let builds = states::builds();
    every_state(&mut shared);
    screen(&tab, &shared, 100, 30, true);
    screen(&tab, &shared, 100, 30, true);
    assert_eq!(states::builds(), builds + 1);
}

#[test]
fn a_change_to_a_font_no_look_uses_shows_on_its_row() {
    let mut shared = map();
    let mut tab = Fonts::new(true);
    // nothing activates ter-u16n.otb, and no look uses it
    go_to(&mut tab, &shared, "ter-u16n.otb");
    let made = act(&mut tab, &shared, "space.view.resources");
    apply(&mut shared, made);
    let rows = rows_of(&screen(&tab, &shared, 100, 30, true));
    let row = rows
        .iter()
        .find(|r| r.starts_with("   ter-u16n.otb"))
        .cloned();
    assert!(
        row.as_deref().is_some_and(|r| r.contains("● active")),
        "{row:?}"
    );
    // vtium-fonts's list activates cullet-mono.otf, and the document turns
    // it off
    go_to(&mut tab, &shared, "cullet-mono.otf");
    let made = act(&mut tab, &shared, "space.view.resources");
    apply(&mut shared, made);
    let rows = rows_of(&screen(&tab, &shared, 100, 30, true));
    let row = rows
        .iter()
        .find(|r| r.starts_with("   cullet-mono.otf"))
        .cloned();
    assert!(
        row.as_deref()
            .is_some_and(|r| r.contains("○ turned off on purpose")),
        "{row:?}"
    );
}
