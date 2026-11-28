//! The Effects tab driven through its `Tab` methods on the screen map's
//! machine and on the large machine: every row of the tab's nodes in the
//! key table, the moves and the scrolling of 200 vtlooks, the search, the
//! list of groups, the detail, Space and the question before a
//! deactivation, the clicks, F1, Find, and the preview's source and place.
//!
//! References:
//! - ADR-1206 D2: update returns commands, which the shell applies.
//! - ADR-1403 D10 and D11: the nodes `effects`, `effects.filter` and
//!   `effects.effect`, and the rows the D11 overlay gives them.
//! - ADR-1205 D1, D8: Space activates or deactivates, and asks first when
//!   a look uses the resource.

mod common;

use common::{SIZES, act, click, machines, repeat, rows_of, screen, type_in};
use vtium_config_core::batch::Change as BatchChange;
use vtium_config_core::document::{Change as DocChange, Edit};
use vtium_config_core::fixture;
use vtium_config_core::found::effects::Group;
use vtium_config_core::preview::Applied;
use vtium_config_core::ui::{Action, Change, Command, Request, Shared};
use vtium_config_tab_effects::Effects;
use vtium_config_tab_effects::list::{self, Item};
use vtium_config_widgets::contract;
use vtium_config_widgets::tab::Tab;

/// The place in the list of every group of the first item named `name`.
fn index_of(tab: &Effects, shared: &Shared, name: &str) -> usize {
    tab.find_items(shared)
        .iter()
        .position(|i| i.name == name)
        .unwrap_or_else(|| panic!("no item {name}"))
}

/// The tab drawn at 100×30 with the item `name` highlighted.
fn on(shared: &Shared, name: &str) -> Effects {
    let mut tab = Effects::new(true);
    screen(&tab, shared, 100, 30, true);
    let at = index_of(&tab, shared, name);
    repeat(&mut tab, shared, "down.view", at + 1);
    tab
}

/// Makes each change in `made` on `shared`, as the shell does.
fn apply(shared: &mut Shared, made: Vec<Command>) {
    for c in made {
        if let Command::Change(Change::Document(change)) = c {
            assert!(shared.change(change).is_ok());
        }
    }
}

/// The edit a single command makes to the active resources.
fn active_edit(made: &[Command]) -> Option<Option<BatchChange>> {
    match made {
        [
            Command::Change(Change::Document(DocChange {
                edit: Edit::Active { change, .. },
                ..
            })),
        ] => Some(*change),
        _ => None,
    }
}

#[test]
fn every_row_of_the_tab_s_nodes_is_answered_without_a_panic() {
    for (machine, build) in machines() {
        for (w, h) in SIZES {
            for node in ["effects", "effects.filter", "effects.effect"] {
                let Some(n) = contract::node(node) else {
                    panic!("no node {node}");
                };
                assert!(n.rows().count() > 10, "{node} has too few rows");
                for row in n.rows() {
                    let shared = build();
                    let mut tab = Effects::new(true);
                    screen(&tab, &shared, w, h, true);
                    match node {
                        "effects.filter" => {
                            act(&mut tab, &shared, "enter.view.effects");
                        }
                        "effects.effect" => {
                            act(&mut tab, &shared, "down.view");
                            act(&mut tab, &shared, "enter.view.effects");
                        }
                        _ => {}
                    }
                    assert_eq!(tab.node(), node, "{machine}: not on {node}");
                    let made = tab.update(&shared, Action::of(row.id));
                    for c in &made {
                        assert!(
                            matches!(
                                c,
                                Command::Change(Change::Say(_) | Change::Document(_))
                                    | Command::Request(Request::Ask { .. })
                            ),
                            "{node} {}: {c:?}",
                            row.id
                        );
                    }
                    assert!(
                        ["effects", "effects.filter", "effects.effect", "any.search"]
                            .contains(&tab.node()),
                        "{node} {} left the tab's nodes for {}",
                        row.id,
                        tab.node()
                    );
                    let drawn = screen(&tab, &shared, w, h, true);
                    assert_eq!(rows_of(&drawn).len(), usize::from(h));
                }
            }
        }
    }
}

#[test]
fn the_large_machine_s_list_scrolls_to_its_last_item_and_back() {
    let shared = fixture::large::shared();
    let effects = &shared.found.effects;
    assert_eq!(effects.vtlooks.len(), 201);
    let all = list::items(effects, Group::All).len();
    for (w, h) in SIZES {
        let mut tab = Effects::new(true);
        screen(&tab, &shared, w, h, true);
        act(&mut tab, &shared, "end.view");
        let last = list::items(effects, Group::All).last().copied();
        assert_eq!(tab.highlighted(&shared), last);
        let text = rows_of(&screen(&tab, &shared, w, h, true)).join("\n");
        assert!(text.contains(&format!("–{all} of {all}")), "{text}");
        // the highlighted row stays in sight on the way back up
        let seen = |tab: &Effects| {
            let at = tab.highlighted(&shared);
            tab.places(&shared)
                .items
                .iter()
                .any(|(_, i)| list::items(effects, Group::All).get(*i).copied() == at)
        };
        assert!(seen(&tab), "the last item is not in sight");
        for _ in 0..40 {
            act(&mut tab, &shared, "up.view");
            screen(&tab, &shared, w, h, true);
            assert!(seen(&tab), "the highlighted item left the window");
        }
        assert!(text.contains("Shader shader-11.glsl"), "{text}");
        act(&mut tab, &shared, "pgup.view");
        act(&mut tab, &shared, "home.view");
        assert_eq!(tab.highlighted(&shared), None);
        let text = rows_of(&screen(&tab, &shared, w, h, true)).join("\n");
        assert!(text.contains("1–"), "{text}");
        assert!(text.contains("No filter"), "{text}");
    }
}

#[test]
fn the_search_filters_the_large_list_as_the_person_types() {
    let shared = fixture::large::shared();
    let mut tab = Effects::new(true);
    screen(&tab, &shared, 80, 25, true);
    act(&mut tab, &shared, "question.view");
    assert_eq!(tab.node(), "any.search");
    assert_eq!(tab.search().map(|s| s.help()), Some("name"));
    type_in(&mut tab, &shared, "char.typing.search", "^admin");
    let Some(search) = tab.search() else {
        panic!("no search");
    };
    assert_eq!(search.matches().len(), 90);
    let first = tab.highlighted(&shared);
    assert!(matches!(first, Some(Item::Vtlook(_))));
    act(&mut tab, &shared, "down.typing.search");
    assert_ne!(tab.highlighted(&shared), first);
    act(&mut tab, &shared, "enter.typing.search");
    assert_eq!(tab.node(), "effects");
    let text = rows_of(&screen(&tab, &shared, 80, 25, true)).join("\n");
    assert!(text.contains("matching ^admin"), "{text}");
    assert!(!text.contains("look-001"), "{text}");
    let Some(search) = tab.search() else {
        panic!("the filter is not kept");
    };
    let places = tab.places(&shared).items;
    assert!(!places.is_empty());
    assert!(
        places.iter().all(|(_, at)| search.is_match(*at)),
        "the list shows an item the search does not match"
    );
    act(&mut tab, &shared, "esc.view");
    assert!(tab.search().is_none());
}

#[test]
fn the_list_of_groups_shows_one_group() {
    for (machine, build) in machines() {
        let shared = build();
        let mut tab = Effects::new(true);
        screen(&tab, &shared, 100, 30, true);
        act(&mut tab, &shared, "enter.view.effects");
        assert_eq!(tab.node(), "effects.filter");
        repeat(&mut tab, &shared, "down.layer.list", 2);
        act(&mut tab, &shared, "enter.layer.list");
        assert_eq!(tab.node(), "effects");
        assert_eq!(tab.group(), Group::ColorMaps, "{machine}");
        act(&mut tab, &shared, "down.view");
        assert_eq!(tab.highlighted(&shared), Some(Item::Map(0)), "{machine}");
        let text = rows_of(&screen(&tab, &shared, 100, 30, true)).join("\n");
        assert!(text.contains("Show: Color maps ▾"), "{text}");
        assert!(!text.contains("No filter"), "{text}");
        // Esc closes the box and changes nothing
        act(&mut tab, &shared, "up.view");
        act(&mut tab, &shared, "enter.view.effects");
        act(&mut tab, &shared, "esc.layer.list");
        assert_eq!(tab.group(), Group::ColorMaps);
        assert_eq!(tab.node(), "effects");
    }
}

#[test]
fn the_detail_scrolls_under_its_title_and_esc_goes_back_to_the_same_item() {
    let shared = fixture::large::shared();
    let mut tab = on(&shared, "look-004");
    act(&mut tab, &shared, "enter.view.effects");
    let first = rows_of(&screen(&tab, &shared, 80, 25, true));
    repeat(&mut tab, &shared, "down.layer.detail", 50);
    let scrolled = rows_of(&screen(&tab, &shared, 80, 25, true));
    assert_ne!(first, scrolled);
    assert_eq!(first.get(2), scrolled.get(2), "the title moved");
    repeat(&mut tab, &shared, "up.layer.detail", 50);
    assert_eq!(rows_of(&screen(&tab, &shared, 80, 25, true)), first);
    act(&mut tab, &shared, "esc.layer");
    assert_eq!(tab.node(), "effects");
    assert_eq!(
        tab.highlighted(&shared),
        Some(Item::Vtlook(index_of(&tab, &shared, "look-004")))
    );
}

#[test]
fn space_activates_a_color_map_as_one_change_and_again_takes_it_back() {
    let mut shared = fixture::padnoir::shared();
    let mut tab = on(&shared, "warm.cube");
    let made = act(&mut tab, &shared, "space.view.resources");
    assert_eq!(active_edit(&made), Some(Some(BatchChange::Activate)));
    apply(&mut shared, made);
    let r = list::resource(&shared.found.effects, Item::Map(1)).cloned();
    let Some(r) = r else { panic!("no warm.cube") };
    assert!(list::active(&shared, &r));
    let made = act(&mut tab, &shared, "space.view.resources");
    // the draft drops its change, since the installed batch keeps it off
    assert_eq!(active_edit(&made), Some(None));
}

#[test]
fn space_on_a_color_map_a_look_uses_asks_first_and_yes_deactivates_it() {
    let mut shared = fixture::padnoir::shared();
    let text = format!(
        "{}[look evening]\nvtlook = sepia\n",
        fixture::padnoir::DRAFT
    );
    assert!(
        shared
            .change(DocChange::new("evening", Edit::Text(text)))
            .is_ok()
    );
    let mut tab = on(&shared, "sepia.cube");
    let made = act(&mut tab, &shared, "space.view.resources");
    let [Command::Request(Request::Ask { question, then })] = made.as_slice() else {
        panic!("no question: {made:?}");
    };
    assert!(
        question.contains("evening uses it through the vtlook sepia"),
        "{question}"
    );
    assert_eq!(then.row, "y.question");
    let made = tab.update(&shared, then.clone());
    assert_eq!(active_edit(&made), Some(Some(BatchChange::Deactivate)));
}

#[test]
fn space_says_why_it_does_nothing_on_a_vtlook_the_filter_line_and_a_file_it_cannot_use() {
    let shared = fixture::padnoir::shared();
    let says = |made: Vec<Command>| match made.as_slice() {
        [Command::Change(Change::Say(m))] => m.clone(),
        _ => panic!("says nothing: {made:?}"),
    };
    let mut tab = Effects::new(true);
    screen(&tab, &shared, 100, 30, true);
    assert!(
        says(act(&mut tab, &shared, "space.view.resources"))
            .line
            .contains("[Space]")
    );
    let mut tab = on(&shared, "amber");
    assert!(
        says(act(&mut tab, &shared, "space.view.resources"))
            .line
            .contains("vtlook")
    );
    let mut tab = on(&shared, "loose.cube");
    let m = says(act(&mut tab, &shared, "space.view.resources"));
    assert!(
        m.line.contains("trust rule") && m.line.contains("[F1]"),
        "{}",
        m.line
    );
    assert!(m.more.is_some_and(|t| t.contains("belongs to user 1000")));
    let mut tab = on(&shared, "mine.cube");
    let m = says(act(&mut tab, &shared, "space.view.resources"));
    assert!(m.line.contains("your own folder"), "{}", m.line);
}

#[test]
fn a_click_highlights_a_row_and_a_second_click_opens_it() {
    let shared = fixture::padnoir::shared();
    let mut tab = Effects::new(true);
    screen(&tab, &shared, 100, 30, true);
    let places = tab.places(&shared);
    let Some(&(rect, at)) = places.items.get(1) else {
        panic!("no rows");
    };
    click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y);
    assert_eq!(
        tab.highlighted(&shared),
        list::items(&shared.found.effects, Group::All)
            .get(at)
            .copied()
    );
    click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y);
    assert_eq!(tab.node(), "effects.effect");
    act(&mut tab, &shared, "esc.layer");
    // the filter line: one click highlights it, a second opens the groups
    let filter = tab.places(&shared).filter;
    click(&mut tab, &shared, "click-row.view", filter.x + 2, filter.y);
    assert_eq!(tab.highlighted(&shared), None);
    click(&mut tab, &shared, "click-row.view", filter.x + 2, filter.y);
    assert_eq!(tab.node(), "effects.filter");
    screen(&tab, &shared, 100, 30, true);
    let Some(boxed) = tab.places(&shared).groups else {
        panic!("no list box");
    };
    let Some(&(row, _)) = boxed.rows.get(3) else {
        panic!("no shaders choice");
    };
    click(&mut tab, &shared, "click-row.view", row.x + 2, row.y);
    click(&mut tab, &shared, "click-row.view", row.x + 2, row.y);
    assert_eq!(tab.group(), Group::Shaders);
}

#[test]
fn f1_gives_the_highlighted_vtlook_s_error_whole() {
    let shared = fixture::padnoir::shared();
    let mut tab = Effects::new(true);
    screen(&tab, &shared, 80, 25, true);
    assert_eq!(tab.full_text(&shared), None);
    let at = tab
        .find_items(&shared)
        .iter()
        .position(|i| i.name == "green" && i.fact.contains("error"))
        .unwrap_or_default();
    repeat(&mut tab, &shared, "down.view", at + 1);
    let Some(text) = tab.full_text(&shared) else {
        panic!("no full text");
    };
    assert!(
        text.contains("needs gamma, contrast, tint, color-blind or cube"),
        "{text}"
    );
}

#[test]
fn find_lists_every_effect_and_its_enter_highlights_one() {
    for (machine, build) in machines() {
        let shared = build();
        let effects = &shared.found.effects;
        let mut tab = Effects::new(true);
        screen(&tab, &shared, 100, 30, true);
        let items = tab.find_items(&shared);
        assert_eq!(
            items.len(),
            effects.vtlooks.len() + effects.maps.len() + effects.shaders.len(),
            "{machine}"
        );
        assert!(items.iter().all(|i| i.place == "Effects"));
        // from a single group, Find's Enter shows every group again
        act(&mut tab, &shared, "enter.view.effects");
        act(&mut tab, &shared, "down.layer.list");
        act(&mut tab, &shared, "enter.layer.list");
        let last = items.len() - 1;
        tab.update(&shared, Action::item("enter.typing.find", last));
        assert_eq!(tab.group(), Group::All);
        assert_eq!(
            tab.highlighted(&shared),
            list::items(effects, Group::All).get(last).copied()
        );
    }
}

#[test]
fn the_tab_offers_the_search_by_name() {
    let shared = fixture::padnoir::shared();
    let tab = Effects::new(true);
    screen(&tab, &shared, 100, 30, true);
    let Some(s) = tab.searchable() else {
        panic!("not searchable");
    };
    assert_eq!(s.fields(), &["name"]);
    assert_eq!(
        s.count(),
        list::items(&shared.found.effects, Group::All).len()
    );
    assert_eq!(s.text(0, 0).as_deref(), Some("No filter"));
    assert_eq!(tab.letter(), 'e');
    assert_eq!(tab.name(), "Effects");
}

#[test]
fn the_preview_applies_the_highlighted_item_in_the_picture_s_frame() {
    let shared = fixture::padnoir::shared();
    let tab = on(&shared, "amber");
    let wanted = tab.preview(&shared, None);
    assert!(matches!(wanted.effect, Applied::Vtlook(v) if v.name == "amber"));
    let Some(place) = wanted.place else {
        panic!("no place");
    };
    assert!(place.width > 20 && place.height >= 2, "{place:?}");
    assert!(place.x > vtium_config_tab_effects::LIST_CELLS);
    // the frame's inside holds no text, so the picture covers nothing drawn
    let drawn = screen(&tab, &shared, 100, 30, true);
    for y in place.y..place.y + place.height {
        for x in place.x..place.x + place.width {
            assert_eq!(drawn.cell((x, y)).map(|c| c.symbol()), Some(" "));
        }
    }
    let mut tab = tab;
    act(&mut tab, &shared, "enter.view.effects");
    let wanted = tab.preview(&shared, None);
    assert!(matches!(wanted.effect, Applied::Nothing));
    assert!(wanted.place.is_none());
    // a shader has no picture, since the picture is drawn on the CPU
    let shared = fixture::large::shared();
    let tab = on(&shared, "shader-00.vtshader");
    let wanted = tab.preview(&shared, None);
    assert!(matches!(wanted.effect, Applied::Shader(_)));
    assert!(wanted.place.is_none());
}

#[test]
fn a_planned_filter_is_marked_planned_without_color() {
    let shared = fixture::large::shared();
    let mut tab = Effects::new(false);
    screen(&tab, &shared, 80, 25, false);
    let at = index_of(&tab, &shared, "look-004");
    repeat(&mut tab, &shared, "down.view", at + 1);
    let text = rows_of(&screen(&tab, &shared, 80, 25, false));
    let row = text
        .iter()
        .find(|r| r.starts_with("  look-004 "))
        .unwrap_or_else(|| panic!("no row\n{}", text.join("\n")));
    assert!(row.contains("planned"), "{row}");
    let all = text.join("\n");
    assert!(all.contains("scanlines (planned)"), "{all}");
    assert!(all.contains("planned, ADR-1304 slice 4"), "{all}");
    act(&mut tab, &shared, "enter.view.effects");
    let all = rows_of(&screen(&tab, &shared, 80, 25, false)).join("\n");
    assert!(all.contains("planned, ADR-1304 slice 4"), "{all}");
}
