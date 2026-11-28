//! The Looks tab driven through its `Tab` methods on the screen map's
//! machine and the large one: every row of each of its nodes, a kept pick
//! of each card line's list box as one change, the sizes a bitmap font
//! supplies, Left and Right, the typed name, Activate on a look, Space on
//! a color scheme, `t`, `p`, `w` and its question, the search, F1, Find,
//! and the clicks on the places the tab returns.
//!
//! References:
//! - ADR-1206 D2: a tab's update returns commands, and the shell makes each change in one place.
//! - ADR-1403 D11, "The list box", "Search, on ?", "F1, help for what is highlighted", and "Fonts the configuration uses but has not activated".
//! - ADR-1205 D7: a look offers the sizes its fonts supply.

mod common;

use common::{
    act, apply, click, go_to, machines, one_change, open_search, screen, search_key, search_type,
    type_in,
};
use std::path::PathBuf;

use vtium_config_core::batch::{Change as Batch, Folders, Start, View};
use vtium_config_core::document::{Change, ConfEdit, Edit, FontSetEdit};
use vtium_config_core::found;
use vtium_config_core::preview::Shows;
use vtium_config_core::ui::shared::FontReads;
use vtium_config_core::ui::{Action, Carries, Change as UiChange, Command, Live, Request, Shared};
use vtium_config_tab_looks::Looks;
use vtium_config_tab_looks::card::{self, Line};
use vtium_config_tab_looks::list::{self, Item, Scheme};
use vtium_config_widgets::contract;
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::tab::{Choices, Searchable, Tab};

/// The first look.
fn a_look(i: &Item) -> bool {
    matches!(i, Item::Look(_))
}

/// The tab, drawn once at 100×30, with its first look's card open on the
/// line `line`.
fn card_on(shared: &Shared, line: usize) -> Looks {
    let mut tab = Looks::new(true);
    screen(&tab, shared, 100, 30, true);
    go_to(&mut tab, shared, a_look);
    act(&mut tab, shared, "enter.view.looks");
    for _ in 0..line {
        act(&mut tab, shared, "down.layer.look");
    }
    screen(&tab, shared, 100, 30, true);
    tab
}

/// The look the card shows, as the document holds it now.
fn shown_look(tab: &Looks, shared: &Shared) -> list::Look {
    let name = tab.card().map(|c| c.name.clone()).unwrap_or_default();
    list::look(shared.document().text(), &name).unwrap_or_default()
}

/// The message lines `commands` put on the message line.
fn said(commands: &[Command]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::Change(UiChange::Say(m)) => Some(m.line.clone()),
            _ => None,
        })
        .collect()
}

/// A machine whose first look draws the font set `term`, whose text font
/// is the screen map's Terminus strike, 8×16 pixels, which nothing
/// activates, and whose second font is Noto Color Emoji, which fails the
/// trust rule.
fn bitmap_machine() -> (Shared, String) {
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    let look = list::looks(shared.document().text())[0].name.clone();
    let done = shared.change(Change::new(
        "term",
        Edit::All(vec![
            Edit::FontSet {
                frit: "term".to_owned(),
                edit: FontSetEdit::Keep(
                    "source=font:Terminus\nsource=font:Noto Color Emoji\n".to_owned(),
                ),
            },
            Edit::Conf(vec![ConfEdit::LookKey {
                name: look.clone(),
                key: "font-name",
                value: Some("term".to_owned()),
            }]),
        ]),
    ));
    assert!(done.is_ok(), "{done:?}");
    (shared, look)
}

/// A way to reach a state of the tab by its keys.
type Reach<'a> = Box<dyn Fn(&mut Looks) + 'a>;

#[test]
fn every_row_of_every_node_acts_and_stays_on_the_tab_s_nodes() {
    let nodes = [
        "looks",
        "looks-resources",
        "looks.look",
        "looks.look.list",
        "looks.look.typing",
        "looks.font-set",
        "looks.palette",
        "looks.pointer",
    ];
    let ours = |n: &str| nodes.contains(&n) || n == "any.search";
    for (machine, build) in machines() {
        let shared = build();
        // each state of the tab, reached by its keys
        let states: Vec<(&str, Reach<'_>)> = vec![
            (
                "looks",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, a_look);
                }),
            ),
            (
                "looks-resources",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, |i| matches!(i, Item::Scheme(_)));
                }),
            ),
            (
                "looks.look",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, a_look);
                    act(t, &shared, "enter.view.looks");
                }),
            ),
            (
                "looks.look.list",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, a_look);
                    act(t, &shared, "enter.view.looks");
                    act(t, &shared, "down.layer.look");
                    act(t, &shared, "list.font-sets");
                }),
            ),
            (
                "looks.look.typing",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, a_look);
                    act(t, &shared, "enter.view.looks");
                    act(t, &shared, "enter.layer.look");
                }),
            ),
            (
                "looks.font-set",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, |i| matches!(i, Item::FontSet(_)));
                    act(t, &shared, "enter.view.looks");
                }),
            ),
            (
                "looks.palette",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, |i| matches!(i, Item::Scheme(_)));
                    act(t, &shared, "enter.view.looks");
                }),
            ),
            (
                "looks.pointer",
                Box::new(|t: &mut Looks| {
                    go_to(t, &shared, |i| matches!(i, Item::Pointer(_)));
                    act(t, &shared, "enter.view.looks");
                }),
            ),
        ];
        for (node, reach) in &states {
            let rows: Vec<&'static str> = contract::node(node)
                .map(|n| n.rows().map(|r| r.id).collect())
                .unwrap_or_default();
            assert!(!rows.is_empty(), "{node} has no rows");
            for row in rows {
                let mut tab = Looks::new(true);
                screen(&tab, &shared, 100, 30, true);
                reach(&mut tab);
                screen(&tab, &shared, 100, 30, true);
                assert_eq!(tab.node(), *node, "{machine}: reaching {node}");
                let made = tab.update(&shared, Action::of(row));
                screen(&tab, &shared, 80, 25, true);
                assert!(
                    ours(tab.node()),
                    "{machine}: {row} on {node} left for {}",
                    tab.node()
                );
                for c in &made {
                    if let Command::Change(UiChange::Document(d)) = c {
                        let mut copy = build();
                        let done = copy.change(d.clone());
                        assert!(
                            done.is_ok(),
                            "{machine}: {row} on {node} made {d:?}: {done:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn each_card_line_s_list_box_keeps_a_pick_as_one_change() {
    for (machine, build) in machines() {
        for line in [
            Line::FontSet,
            Line::Size,
            Line::Palette,
            Line::Vtlook,
            Line::Pointer,
        ] {
            let mut shared = build();
            let at = Line::ALL.iter().position(|l| *l == line).unwrap_or(0);
            let mut tab = card_on(&shared, at);
            let row = match line {
                Line::FontSet => "list.font-sets",
                Line::Size => "list.font-sizes",
                Line::Palette => "list.palettes",
                Line::Vtlook => "list.look-effects",
                _ => "list.pointer-themes",
            };
            act(&mut tab, &shared, row);
            assert_eq!(tab.node(), "looks.look.list", "{machine} {line:?}");
            let look = shown_look(&tab, &shared);
            let choices = card::choices(&shared, &look, line);
            // a choice other than the look's own, the first where the look
            // sets its own value and the last otherwise
            let target = match choices.current {
                Some(0) => choices.count() - 1,
                _ => 0,
            };
            let from = choices.current.unwrap_or(0);
            let (key, n) = if target > from {
                ("down.layer.list", target - from)
            } else {
                ("up.layer.list", from - target)
            };
            for _ in 0..n {
                assert!(act(&mut tab, &shared, key).is_empty());
            }
            let made = act(&mut tab, &shared, "enter.layer.list");
            assert_eq!(made.len(), 1, "{machine} {line:?}: {made:?}");
            let change = one_change(&made);
            match &change.edit {
                Edit::Conf(edits) => assert!(
                    matches!(edits.as_slice(), [ConfEdit::LookKey { name, key, value }]
                        if *name == look.name && Some(*key) == line.key() && *value == choices.keep(target)),
                    "{machine} {line:?}: {edits:?}"
                ),
                other => panic!("{machine} {line:?}: {other:?}"),
            }
            assert_eq!(tab.node(), "looks.look");
            apply(&mut shared, &made);
            let now = card::choices(&shared, &shown_look(&tab, &shared), line);
            assert_eq!(now.current, Some(target), "{machine} {line:?}");
            // Esc closes a list box and changes nothing
            act(&mut tab, &shared, row);
            assert!(act(&mut tab, &shared, "esc.layer.list").is_empty());
            assert_eq!(tab.node(), "looks.look");
        }
    }
}

#[test]
fn a_bitmap_text_font_offers_only_the_sizes_its_strikes_draw() {
    let (shared, _) = bitmap_machine();
    let mut tab = card_on(&shared, 2);
    act(&mut tab, &shared, "list.font-sizes");
    let look = shown_look(&tab, &shared);
    let c = card::choices(&shared, &look, Line::Size);
    let texts: Vec<&str> = c.choices.iter().map(|c| c.text.as_str()).collect();
    // the strike is 16 pixels high, which 12 pt draws at 96 dpi; the look
    // keeps its own 11 pt as a choice of its own
    assert_eq!(
        texts,
        [
            "12 pt, from vtium.conf",
            "automatic",
            "12 pt",
            "11 pt, the look's own"
        ],
    );
    // an outline font offers the usual sizes
    let shared = vtium_config_core::fixture::padnoir::shared();
    let look = list::looks(shared.document().text())[0].clone();
    let c = card::choices(&shared, &look, Line::Size);
    assert_eq!(c.count(), 2 + vtium_config_tab_looks::sizes::USUAL.len());
}

#[test]
fn left_and_right_step_a_line_as_one_change() {
    for (machine, build) in machines() {
        let mut shared = build();
        let mut tab = card_on(&shared, 3);
        let before = card::choices(&shared, &shown_look(&tab, &shared), Line::Palette).current;
        let made = act(&mut tab, &shared, "right.layer.look");
        one_change(&made);
        apply(&mut shared, &made);
        let after = card::choices(&shared, &shown_look(&tab, &shared), Line::Palette).current;
        assert_eq!(after, before.map(|b| b + 1), "{machine}");
        let made = act(&mut tab, &shared, "left.layer.look");
        one_change(&made);
        apply(&mut shared, &made);
        let back = card::choices(&shared, &shown_look(&tab, &shared), Line::Palette).current;
        assert_eq!(back, before, "{machine}");
        // the name has no choices to step
        let mut tab = card_on(&shared, 0);
        assert!(act(&mut tab, &shared, "right.layer.look").is_empty());
    }
}

#[test]
fn a_typed_name_renames_the_look_as_one_change_or_says_why_not() {
    for (machine, build) in machines() {
        let mut shared = build();
        let mut tab = card_on(&shared, 0);
        act(&mut tab, &shared, "enter.layer.look");
        assert_eq!(tab.node(), "looks.look.typing");
        act(&mut tab, &shared, "backspace.typing");
        type_in(&mut tab, &shared, "char.typing", "-evening");
        let made = act(&mut tab, &shared, "enter.typing");
        let change = one_change(&made);
        assert!(
            matches!(&change.edit, Edit::Conf(e) if matches!(e.as_slice(), [ConfEdit::LookRename { .. }])),
            "{machine}"
        );
        apply(&mut shared, &made);
        assert_eq!(tab.node(), "looks.look");
        assert!(
            shown_look(&tab, &shared).name.ends_with("-evening"),
            "{machine}"
        );
        // a name with a space is refused: the entry stays and says why
        act(&mut tab, &shared, "enter.layer.look");
        type_in(&mut tab, &shared, "char.typing", " x");
        let made = act(&mut tab, &shared, "enter.typing");
        assert_eq!(said(&made).len(), 1, "{machine}: {made:?}");
        assert_eq!(tab.node(), "looks.look.typing");
        assert!(act(&mut tab, &shared, "esc.typing").is_empty());
        assert_eq!(tab.node(), "looks.look");
    }
}

#[test]
fn a_look_whose_fonts_are_not_all_active_says_so_and_activates_them_as_one_change() {
    let (mut shared, look) = bitmap_machine();
    let mut tab = card_on(&shared, 0);
    let all = common::rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(all.contains("1 font not active · [A] Activate"), "{all}");
    assert!(all.contains("○ not active"), "{all}");
    assert!(all.contains("fails trust"), "{all}");
    let made = act(&mut tab, &shared, "shift-a.layer.look");
    let change = one_change(&made);
    let Edit::All(edits) = &change.edit else {
        panic!("{change:?}");
    };
    // the Terminus strike only: Noto Color Emoji fails the trust rule
    assert_eq!(
        edits.as_slice(),
        [Edit::Active {
            path: "/usr/share/fonts/misc/ter-u16n.otb".into(),
            change: Some(Batch::Activate),
        }]
    );
    assert_eq!(said(&made).len(), 1);
    apply(&mut shared, &made);
    let all = common::rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(!all.contains("not active · [A] Activate"), "{all}");
    assert!(all.contains("○ not all active"), "{all}");
    // the list's row keeps the mark of the font that cannot be activated
    act(&mut tab, &shared, "esc.layer.look");
    let all = common::rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(
        all.contains(&format!("{look} · 1 font not active")),
        "{all}"
    );
}

#[test]
fn a_look_s_fonts_are_judged_against_the_batch_the_shared_state_holds() {
    let (mut shared, _) = bitmap_machine();
    let tab = card_on(&shared, 0);
    let all = common::rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(all.contains("1 font not active · [A] Activate"), "{all}");
    // the loop reads the batch again, which now activates the Terminus
    // strike, and the card judges against it without a change of the
    // document
    let Some(reads) = shared.in_use.reads().cloned() else {
        panic!("the fixture holds no batch");
    };
    let start = Start {
        fresh: vtium_settings::batch::Batch::default(),
        folders: Folders {
            package: PathBuf::new(),
            admin: PathBuf::new(),
        },
        trust: vtium_settings::Trust::running_as(0),
    };
    let view = View::new(Some(b"/usr/share/fonts/misc/ter-u16n.otb\n"), &start);
    shared.read_fonts(FontReads { view, ..reads });
    let all = common::rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(!all.contains("not active · [A] Activate"), "{all}");
    assert!(all.contains("● active"), "{all}");
}

#[test]
fn what_a_look_misses_says_why_for_each_font() {
    let (shared, _) = bitmap_machine();
    let mut tab = card_on(&shared, 6);
    act(&mut tab, &shared, "enter.layer.look");
    let all = common::rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
    assert!(
        all.contains("Text: ter-u16n.otb is installed and ○ not active."),
        "{all}"
    );
    assert!(
        all.contains("NotoColorEmoji.ttf is ○ not active: it fails trust"),
        "{all}"
    );
    assert!(all.contains("[A] activates the 1 font"), "{all}");
    assert!(act(&mut tab, &shared, "esc.layer.look").is_empty());
    assert_eq!(tab.node(), "looks.look");
}

#[test]
fn space_activates_a_scheme_file_and_a_built_in_scheme_is_always_active() {
    let shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    go_to(&mut tab, &shared, |i| {
        matches!(i, Item::Scheme(Scheme::BuiltIn("vga")))
    });
    assert_eq!(tab.node(), "looks-resources");
    let made = act(&mut tab, &shared, "space.view.resources");
    assert_eq!(said(&made), ["vga is built in and always active."]);
    // the pointer theme Adwaita is trusted and not active
    let at = go_to(&mut tab, &shared, |i| matches!(i, Item::Pointer(_)));
    let adwaita = list::items(&shared)
        .iter()
        .skip(at)
        .position(|i| i.name(&shared) == "Adwaita")
        .unwrap_or(0);
    for _ in 0..adwaita {
        act(&mut tab, &shared, "down.view");
    }
    let made = act(&mut tab, &shared, "space.view.resources");
    let change = one_change(&made);
    assert_eq!(
        change.edit,
        Edit::Active {
            path: "/usr/share/icons/Adwaita".into(),
            change: Some(Batch::Activate),
        }
    );
}

#[test]
fn t_tries_a_look_on_a_vtium_console_and_says_why_not_elsewhere() {
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    go_to(&mut tab, &shared, a_look);
    shared.console.live = Live::NotVtium;
    let made = act(&mut tab, &shared, "t.view.looks");
    assert_eq!(said(&made).len(), 1);
    shared.console.live = Live::On;
    let made = act(&mut tab, &shared, "t.view.looks");
    assert!(matches!(made.as_slice(), [Command::Change(UiChange::TryLook(n))] if n == "big"));
}

#[test]
fn p_switches_the_preview_and_the_preview_names_its_font() {
    let shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    go_to(&mut tab, &shared, a_look);
    let p = tab.preview(&shared).unwrap_or_else(|| panic!("no preview"));
    assert_eq!(p.font.as_deref(), Some("proof"));
    assert!(
        matches!(p.shows, Shows::Size { points: 11 }),
        "{:?}",
        p.shows
    );
    assert!(p.place.is_some());
    act(&mut tab, &shared, "p.view.looks");
    assert!(
        tab.preview(&shared)
            .is_some_and(|p| matches!(p.shows, Shows::Font { detail: false }))
    );
    go_to(&mut tab, &shared, |i| matches!(i, Item::Pointer(_)));
    assert!(tab.preview(&shared).is_none());
}

#[test]
fn w_asks_first_and_only_a_yes_lets_programs_resize_the_grid() {
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    let made = act(&mut tab, &shared, "window-ops.view.looks");
    let [Command::Request(Request::Ask { then, .. })] = made.as_slice() else {
        panic!("{made:?}");
    };
    assert!(
        made.iter()
            .all(|c| !matches!(c, Command::Change(UiChange::Document(_))))
    );
    let yes = tab.update(&shared, then.clone());
    let change = one_change(&yes);
    assert!(
        matches!(&change.edit, Edit::Conf(e) if matches!(e.as_slice(),
        [ConfEdit::Top { key: "allow-window-ops", value: Some(v) }] if v == "on"))
    );
    apply(&mut shared, &yes);
    let made = act(&mut tab, &shared, "window-ops.view.looks");
    assert_eq!(said(&made).len(), 1, "{made:?}");
}

#[test]
fn the_shell_s_search_filters_by_name_and_clearing_it_lists_every_item() {
    for (machine, build) in machines() {
        let shared = build();
        let mut tab = Looks::new(true);
        screen(&tab, &shared, 100, 30, true);
        go_to(&mut tab, &shared, |i| matches!(i, Item::FontSet(_)));
        let at = list::items(&shared)
            .iter()
            .position(|i| matches!(i, Item::FontSet(_)));
        assert_eq!(Searchable::highlighted(&tab), at, "{machine}");
        let mut search = Some(open_search(&tab));
        assert_eq!(search.as_ref().and_then(|s| s.highlighted()), at);
        search_type(&mut tab, &shared, &mut search, "^sol");
        let kept = search
            .as_ref()
            .map(|s| s.matches().to_vec())
            .unwrap_or_default();
        let items = list::items(&shared);
        assert!(!kept.is_empty(), "{machine}");
        assert!(
            kept.iter()
                .all(|&i| items[i].name(&shared).starts_with("sol")),
            "{machine}"
        );
        // the shell draws its own row, so the tab gives no search of its own
        assert!(tab.search().is_none(), "{machine}");
        search_key(&mut tab, &shared, &mut search, Key::plain(Code::Enter));
        assert_eq!(tab.node(), "looks-resources", "{machine}");
        let shown: Vec<usize> = tab.places(&shared).items.iter().map(|p| p.1).collect();
        assert_eq!(shown, kept, "{machine}: the list shows only the matches");
        // with the filter kept, the list's own keys move among the matches
        act(&mut tab, &shared, "end.view");
        assert_eq!(Searchable::highlighted(&tab), kept.last().copied());
        act(&mut tab, &shared, "home.view");
        assert_eq!(Searchable::highlighted(&tab), kept.first().copied());
        tab.filtered(&shared, None);
        screen(&tab, &shared, 100, 30, true);
        assert!(
            tab.places(&shared).items.len() > kept.len(),
            "{machine}: clearing lists every item again"
        );
        assert_eq!(Searchable::highlighted(&tab), kept.first().copied());
    }
}

#[test]
fn another_tab_s_request_highlights_a_font_set_or_a_look_by_its_name() {
    for (machine, build) in machines() {
        let shared = build();
        let items = list::items(&shared);
        let at = |want: &Item| items.iter().position(|i| i == want);
        let mut tab = Looks::new(true);
        screen(&tab, &shared, 100, 30, true);
        // a font set, with a look's card open over the list
        let Some(Item::FontSet(set)) = items
            .iter()
            .rev()
            .find(|i| matches!(i, Item::FontSet(_)))
            .cloned()
        else {
            panic!("{machine} has no font set");
        };
        go_to(&mut tab, &shared, a_look);
        act(&mut tab, &shared, "enter.view.looks");
        tab.show(&shared, &set);
        assert!(tab.card().is_none(), "{machine}: the card stays open");
        assert_eq!(tab.node(), "looks");
        assert_eq!(
            Searchable::highlighted(&tab),
            at(&Item::FontSet(set.clone())),
            "{machine}"
        );
        // a look, where no font set has the name
        let Some(Item::Look(look)) = items.iter().find(|i| matches!(i, Item::Look(_))).cloned()
        else {
            panic!("{machine} has no look");
        };
        tab.show(&shared, &look);
        assert_eq!(Searchable::highlighted(&tab), at(&Item::Look(look)));
        // a name the list does not hold leaves the highlight where it is
        let before = Searchable::highlighted(&tab);
        tab.show(&shared, "no-such-font-set");
        assert_eq!(Searchable::highlighted(&tab), before, "{machine}");
        // a name the kept filter leaves out is not listed either
        let mut search = Some(open_search(&tab));
        search_type(&mut tab, &shared, &mut search, "^sol");
        search_key(&mut tab, &shared, &mut search, Key::plain(Code::Enter));
        let kept = Searchable::highlighted(&tab);
        tab.show(&shared, &set);
        assert_eq!(Searchable::highlighted(&tab), kept, "{machine}");
    }
}

#[test]
fn f1_gives_the_full_text_and_find_lists_every_item() {
    for (machine, build) in machines() {
        let shared = build();
        let mut tab = card_on(&shared, 2);
        let text = tab.full_text(&shared).unwrap_or_default();
        assert!(
            text.contains("Its choices:") && text.contains("automatic"),
            "{machine}: {text}"
        );
        act(&mut tab, &shared, "enter.layer.look");
        act(&mut tab, &shared, "list.font-sizes");
        act(&mut tab, &shared, "esc.layer.list");
        act(&mut tab, &shared, "up.layer.look");
        act(&mut tab, &shared, "up.layer.look");
        act(&mut tab, &shared, "enter.layer.look");
        assert_eq!(tab.node(), "looks.look.typing");
        assert!(tab.full_text(&shared).is_none());
        let items = tab.find_items(&shared);
        let all = list::items(&shared);
        assert_eq!(items.len(), all.len(), "{machine}");
        assert!(
            items
                .iter()
                .any(|i| i.description.as_deref() == Some("look"))
        );
        let font_set = items
            .iter()
            .find(|i| i.description.as_deref() == Some("font set"));
        let at = font_set.map_or(0, |i| i.index);
        let made = tab.update(
            &shared,
            Action {
                row: "enter.typing.find",
                carries: Carries::Item(at),
            },
        );
        assert!(made.is_empty());
        assert_eq!(tab.node(), "looks");
        assert!(matches!(tab.highlighted(&shared), Some(Item::FontSet(_))));
    }
}

#[test]
fn clicks_highlight_then_open_and_keep_a_list_box_s_pick() {
    for (machine, build) in machines() {
        let shared = build();
        let mut tab = Looks::new(true);
        screen(&tab, &shared, 100, 30, true);
        let places = tab.places(&shared);
        let (rect, item) = places.items[1];
        assert!(click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y).is_empty());
        assert_eq!(
            tab.highlighted(&shared),
            list::items(&shared).get(item).cloned()
        );
        go_to(&mut tab, &shared, a_look);
        screen(&tab, &shared, 100, 30, true);
        let (rect, _) = tab.places(&shared).items[0];
        click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y);
        assert_eq!(tab.node(), "looks.look", "{machine}");
        screen(&tab, &shared, 100, 30, true);
        let (rect, row) = tab.places(&shared).card[3];
        assert_eq!(row, 3);
        click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y);
        click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y);
        assert_eq!(tab.node(), "looks.look.list", "{machine}");
        screen(&tab, &shared, 100, 30, true);
        let list = tab
            .places(&shared)
            .list
            .unwrap_or_else(|| panic!("no list box"));
        let (r, _) = list.rows[1];
        assert!(click(&mut tab, &shared, "click-choice.layer.list", r.x + 2, r.y).is_empty());
        let made = click(&mut tab, &shared, "click-choice.layer.list", r.x + 2, r.y);
        one_change(&made);
        assert_eq!(tab.node(), "looks.look");
    }
}

#[test]
fn keys_read_no_folder() {
    for (_, build) in machines() {
        let shared = build();
        let before = found::scans();
        let mut tab = Looks::new(true);
        screen(&tab, &shared, 100, 30, true);
        for row in [
            "down.view",
            "down.view",
            "end.view",
            "home.view",
            "enter.view.looks",
        ] {
            act(&mut tab, &shared, row);
            screen(&tab, &shared, 100, 30, true);
        }
        act(&mut tab, &shared, "list.font-sizes");
        screen(&tab, &shared, 80, 25, true);
        assert_eq!(found::scans(), before);
    }
}

#[test]
fn the_font_set_line_offers_no_font_set_the_document_removes() {
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    let done = shared.change(Change::new(
        "remove tiny5",
        Edit::FontSet {
            frit: "tiny5".to_owned(),
            edit: FontSetEdit::Remove,
        },
    ));
    assert!(done.is_ok(), "{done:?}");
    let look = list::looks(shared.document().text())[0].clone();
    let texts: Vec<String> = card::choices(&shared, &look, Line::FontSet)
        .choices
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert!(texts.iter().any(|t| t == "minspec-16"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "tiny5"), "{texts:?}");
}
