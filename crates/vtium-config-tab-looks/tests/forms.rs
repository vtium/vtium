//! The Looks tab's forms and questions driven through its `Tab` methods
//! on the screen map's machine and the large one: every row of the new
//! font set's form's nodes; a new font set made as one change; the fonts
//! each role offers; Other… and the typed name; `n` making a look; `r` on
//! a look and on a font set; Space before deactivating what a look uses;
//! F1 on the form; the clicks on the places the form returns; and that no
//! key reads a folder.
//!
//! References:
//! - ADR-1206 D2: a tab's update returns commands, and the shell makes each change in one place.
//! - ADR-1201 D3, "A new font set", and ADR-1205 D4: a role offers only the active fonts that fit it.
//! - ADR-1403 D8, "Every font set change is kept at once": what `r` keeps.

mod common;

use common::{act, apply, click, go_to, machines, one_change, screen, type_in};
use vtium_config_core::batch::Change as Batch;
use vtium_config_core::document::{Change, ConfEdit, Edit, FontSetEdit};
use vtium_config_core::found;
use vtium_config_core::ui::{Action, Change as UiChange, Command, Request, Shared};
use vtium_config_tab_looks::Looks;
use vtium_config_tab_looks::forms::new_frit::{self, Field, Value};
use vtium_config_tab_looks::list::{self, Item};
use vtium_config_widgets::contract;
use vtium_config_widgets::tab::{Choices, Tab};

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

/// The question `commands` ask, and the action its yes hands back.
fn asked(commands: &[Command]) -> (String, Action) {
    match commands {
        [Command::Request(Request::Ask { question, then })] => (question.clone(), then.clone()),
        _ => panic!("not one question: {commands:?}"),
    }
}

/// The tab, drawn at 100×30, with the new font set's form open.
fn form_open(shared: &Shared) -> Looks {
    let mut tab = Looks::new(true);
    screen(&tab, shared, 100, 30, true);
    go_to(&mut tab, shared, |i| matches!(i, Item::FontSet(_)));
    let made = act(&mut tab, shared, "n.view.looks");
    assert!(made.is_empty(), "{made:?}");
    screen(&tab, shared, 100, 30, true);
    assert_eq!(tab.node(), "looks.new-frit");
    tab
}

/// Moves the form's highlight to the row of `field`.
fn to_field(tab: &mut Looks, shared: &Shared, field: Field) {
    act(tab, shared, "home.layer.new-frit");
    let at = Field::ALL.iter().position(|f| *f == field).unwrap_or(0);
    for _ in 0..at {
        act(tab, shared, "down.layer.new-frit");
    }
}

/// Opens the list box of `field` and keeps its choice `name`, found by
/// stepping down from the first.
fn keep_choice(tab: &mut Looks, shared: &Shared, field: Field, name: &str) {
    to_field(tab, shared, field);
    act(tab, shared, "list.system-fonts");
    screen(tab, shared, 100, 30, true);
    assert_eq!(tab.node(), "looks.new-frit.list");
    let frit = tab.form().map(|f| f.frit.clone()).unwrap_or_default();
    let c = new_frit::choices(shared, field, &frit);
    let at = c
        .choices
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name} is not among {:?}", c.choices));
    act(tab, shared, "home.layer.list");
    for _ in 0..at {
        act(tab, shared, "down.layer.list");
    }
    let made = act(tab, shared, "enter.layer.list");
    assert!(
        made.iter()
            .all(|c| !matches!(c, Command::Change(UiChange::Document(_)))),
        "a pick in the form changes the document: {made:?}"
    );
}

/// A way to reach a state of the form by its keys.
type Reach<'a> = Box<dyn Fn(&mut Looks) + 'a>;

#[test]
fn every_row_of_the_form_s_nodes_acts_and_stays_on_the_tab_s_nodes() {
    let ours = |n: &str| n.starts_with("looks") || n == "any.search";
    for (machine, build) in machines() {
        let shared = build();
        let states: Vec<(&str, Reach<'_>)> = vec![
            ("looks.new-frit", Box::new(|_: &mut Looks| {})),
            (
                "looks.new-frit.list",
                Box::new(|t: &mut Looks| {
                    act(t, &shared, "list.system-fonts");
                }),
            ),
            (
                "looks.new-frit.typing",
                Box::new(|t: &mut Looks| {
                    act(t, &shared, "end.layer.new-frit");
                    act(t, &shared, "up.layer.new-frit");
                    act(t, &shared, "enter.layer.new-frit");
                }),
            ),
        ];
        for (node, reach) in &states {
            let rows: Vec<&'static str> = contract::node(node)
                .map(|n| n.rows().map(|r| r.id).collect())
                .unwrap_or_default();
            assert!(!rows.is_empty(), "{node} has no rows");
            for row in rows {
                let mut tab = form_open(&shared);
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
fn a_new_font_set_made_in_the_form_is_one_change_of_the_document() {
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = form_open(&shared);
    keep_choice(&mut tab, &shared, Field::Text, "Cullet Mono");
    keep_choice(&mut tab, &shared, Field::Symbols, "DejaVu Sans Mono");
    keep_choice(&mut tab, &shared, Field::Box, "the text font's own lines");
    keep_choice(&mut tab, &shared, Field::Fallback, "Cullet Mono");
    act(&mut tab, &shared, "end.layer.new-frit");
    let made = act(&mut tab, &shared, "enter.layer.new-frit");
    assert_eq!(made.len(), 1, "not one command: {made:?}");
    let change = one_change(&made);
    let Edit::FontSet {
        frit,
        edit: FontSetEdit::Keep(text),
    } = &change.edit
    else {
        panic!("not a new font set: {change:?}");
    };
    assert_eq!(frit, "cullet-mono");
    let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(
        lines,
        [
            "name=cullet-mono",
            "source=outline:/usr/share/vtium/fonts/cullet-mono.otf",
            "source=outline:/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "source=outline:/usr/share/vtium/fonts/cullet-mono.otf",
            "glyph-source-box=font",
        ]
    );
    assert!(tab.form().is_none(), "the form stays open");
    apply(&mut shared, &made);
    assert!(
        shared
            .document()
            .content()
            .font_sets
            .kept
            .contains_key("cullet-mono")
    );
    assert_eq!(
        tab.highlighted(&shared),
        Some(Item::FontSet("cullet-mono".to_owned()))
    );
}

#[test]
fn each_role_offers_only_the_active_fonts_that_fit_it() {
    for (machine, build) in machines() {
        let shared = build();
        let frit = new_frit::NewFrit::default();
        for field in Field::ALL {
            let c = new_frit::choices(&shared, field, &frit);
            for choice in &c.choices {
                let Value::Pick(pick) = &choice.value else {
                    continue;
                };
                let Some(path) = pick
                    .lines
                    .first()
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, p)| p)
                else {
                    continue;
                };
                if choice.note == "built in" {
                    continue;
                }
                let font = shared
                    .found
                    .fonts
                    .file(std::path::Path::new(path))
                    .unwrap_or_else(|| panic!("{machine}: {path} was not found"));
                let r = &font.resource;
                assert!(
                    vtium_config_tab_looks::card::active(&shared, r) && r.trust.is_ok() && !r.own,
                    "{machine}: {field:?} offers {path}, which is not active"
                );
            }
            if field != Field::Box {
                assert_eq!(
                    c.choices.last().map(|c| &c.value),
                    Some(&Value::Other),
                    "{machine}: {field:?}"
                );
            }
        }
    }
    let shared = vtium_config_core::fixture::padnoir::shared();
    let frit = new_frit::NewFrit::default();
    let names = |field| -> Vec<String> {
        new_frit::choices(&shared, field, &frit)
            .choices
            .into_iter()
            .map(|c| c.name)
            .collect()
    };
    assert_eq!(
        names(Field::Text),
        [
            "DejaVu Sans Mono",
            "Cullet Mono",
            "unifont",
            "unscii-8",
            "Other…"
        ]
    );
    assert_eq!(names(Field::Emoji), ["None", "Other…"]);
    assert_eq!(
        names(Field::Symbols),
        ["None", "DejaVu Sans Mono", "Cullet Mono", "Other…"]
    );
}

#[test]
fn scripts_take_several_fonts_and_enter_on_a_picked_one_takes_it_out() {
    let shared = vtium_config_core::fixture::large::shared();
    let mut tab = form_open(&shared);
    let frit = new_frit::NewFrit::default();
    let c = new_frit::choices(&shared, Field::Scripts, &frit);
    let fonts: Vec<String> = c
        .choices
        .iter()
        .filter(|c| matches!(c.value, Value::Pick(_)))
        .map(|c| c.name.clone())
        .take(2)
        .collect();
    assert_eq!(fonts.len(), 2, "{:?}", c.choices);
    keep_choice(&mut tab, &shared, Field::Scripts, &fonts[0]);
    keep_choice(&mut tab, &shared, Field::Scripts, &fonts[1]);
    let shown = |tab: &Looks| {
        tab.form()
            .map(|f| f.frit.shown(Field::Scripts))
            .unwrap_or_default()
    };
    assert_eq!(shown(&tab), format!("{}, {}", fonts[0], fonts[1]));
    keep_choice(&mut tab, &shared, Field::Scripts, &fonts[0]);
    assert_eq!(shown(&tab), fonts[1]);
}

#[test]
fn a_role_s_long_list_offers_the_search_and_keeps_a_match() {
    let shared = vtium_config_core::fixture::large::shared();
    let mut tab = form_open(&shared);
    act(&mut tab, &shared, "list.system-fonts");
    screen(&tab, &shared, 80, 25, true);
    act(&mut tab, &shared, "question.layer.list");
    assert_eq!(tab.node(), "any.search");
    assert!(tab.search().is_some());
    type_in(&mut tab, &shared, "char.typing", "Family 0044");
    act(&mut tab, &shared, "enter.typing");
    assert_eq!(tab.node(), "looks.new-frit.list");
    act(&mut tab, &shared, "enter.layer.list");
    assert_eq!(tab.node(), "looks.new-frit");
    let shown = tab
        .form()
        .map(|f| f.frit.shown(Field::Text))
        .unwrap_or_default();
    assert!(shown.starts_with("Family 0044"), "{shown}");
}

#[test]
fn other_lets_the_person_type_a_font_and_the_name_is_typed() {
    let shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = form_open(&shared);
    keep_choice(&mut tab, &shared, Field::Text, "DejaVu Sans Mono");
    to_field(&mut tab, &shared, Field::Fallback);
    act(&mut tab, &shared, "list.system-fonts");
    screen(&tab, &shared, 100, 30, true);
    let made = act(&mut tab, &shared, "end.layer.list");
    assert!(made.is_empty());
    let made = act(&mut tab, &shared, "enter.layer.list");
    assert_eq!(tab.node(), "looks.new-frit.typing");
    assert_eq!(said(&made).len(), 1, "{made:?}");
    type_in(&mut tab, &shared, "char.typing", "Terminus");
    act(&mut tab, &shared, "enter.typing.new-frit");
    assert_eq!(tab.node(), "looks.new-frit");
    assert_eq!(tab.form().map(|f| f.field()), Some(Some(Field::Name)));
    act(&mut tab, &shared, "enter.layer.new-frit");
    type_in(&mut tab, &shared, "char.typing", "mono-work");
    act(&mut tab, &shared, "enter.typing.new-frit");
    let frit = tab.form().map(|f| f.frit.clone()).unwrap_or_default();
    assert_eq!(frit.name(), "mono-work");
    assert!(
        frit.text().contains("source=font:Terminus\n"),
        "{}",
        frit.text()
    );
    // Esc drops what is typed, and keeps the name it had
    act(&mut tab, &shared, "up.layer.new-frit");
    act(&mut tab, &shared, "enter.layer.new-frit");
    type_in(&mut tab, &shared, "char.typing", "x");
    act(&mut tab, &shared, "esc.typing");
    assert_eq!(tab.form().map(|f| f.frit.name()), Some("mono-work".into()));
}

#[test]
fn the_form_refuses_a_font_set_without_a_text_font_or_with_a_bad_name() {
    let shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = form_open(&shared);
    act(&mut tab, &shared, "end.layer.new-frit");
    let made = act(&mut tab, &shared, "enter.layer.new-frit");
    assert!(
        said(&made).iter().any(|l| l.contains("needs a text font")),
        "{made:?}"
    );
    assert!(
        made.iter()
            .all(|c| !matches!(c, Command::Change(UiChange::Document(_))))
    );
    assert_eq!(tab.node(), "looks.new-frit");
    keep_choice(&mut tab, &shared, Field::Text, "Cullet Mono");
    to_field(&mut tab, &shared, Field::Name);
    act(&mut tab, &shared, "enter.layer.new-frit");
    type_in(&mut tab, &shared, "char.typing", "unifont");
    act(&mut tab, &shared, "enter.typing.new-frit");
    let made = act(&mut tab, &shared, "enter.layer.new-frit");
    assert!(said(&made).iter().any(|l| l.contains("name")), "{made:?}");
    assert!(tab.form().is_some());
}

#[test]
fn esc_leaves_the_form_and_makes_nothing() {
    for (machine, build) in machines() {
        let shared = build();
        let mut tab = form_open(&shared);
        let before = tab.highlighted(&shared);
        keep_choice(&mut tab, &shared, Field::Fallback, "Other…");
        act(&mut tab, &shared, "esc.typing");
        let made = act(&mut tab, &shared, "esc.layer");
        assert!(made.is_empty(), "{machine}: {made:?}");
        assert_eq!(tab.node(), "looks", "{machine}");
        assert_eq!(tab.highlighted(&shared), before, "{machine}");
        act(&mut tab, &shared, "n.view.looks");
        assert_eq!(
            tab.form().map(|f| f.frit.clone()),
            Some(new_frit::NewFrit::default())
        );
    }
}

#[test]
fn n_makes_a_look_as_one_change_and_types_its_name() {
    for (machine, build) in machines() {
        let mut shared = build();
        let mut tab = Looks::new(true);
        screen(&tab, &shared, 100, 30, true);
        go_to(&mut tab, &shared, |i| matches!(i, Item::Look(_)));
        let made = act(&mut tab, &shared, "n.view.looks");
        let change = one_change(&made);
        assert!(
            matches!(&change.edit, Edit::Conf(e) if matches!(e.as_slice(),
            [ConfEdit::LookAdd { name, frit: None, size: None, vtlook: None }] if name == "new-look")),
            "{machine}: {change:?}"
        );
        assert_eq!(said(&made).len(), 1, "{machine}: {made:?}");
        assert_eq!(tab.node(), "looks.look.typing", "{machine}");
        apply(&mut shared, &made);
        screen(&tab, &shared, 100, 30, true);
        for _ in 0.."new-look".len() {
            act(&mut tab, &shared, "backspace.typing");
        }
        type_in(&mut tab, &shared, "char.typing", "Evening");
        let made = act(&mut tab, &shared, "enter.typing");
        let change = one_change(&made);
        assert!(
            matches!(&change.edit, Edit::Conf(e) if matches!(e.as_slice(),
            [ConfEdit::LookRename { name, new }] if name == "new-look" && new == "Evening")),
            "{machine}: {change:?}"
        );
        apply(&mut shared, &made);
        assert!(list::look(shared.document().text(), "Evening").is_some());
        act(&mut tab, &shared, "esc.layer.look");
        let made = act(&mut tab, &shared, "n.view.looks");
        apply(&mut shared, &made);
        assert!(
            list::look(shared.document().text(), "new-look").is_some(),
            "{machine}"
        );
        act(&mut tab, &shared, "esc.typing");
        act(&mut tab, &shared, "esc.layer.look");
        let made = act(&mut tab, &shared, "n.view.looks");
        assert!(
            matches!(&one_change(&made).edit, Edit::Conf(e) if matches!(e.as_slice(),
            [ConfEdit::LookAdd { name, .. }] if name == "new-look-2")),
            "{machine}: {made:?}"
        );
    }
}

#[test]
fn a_new_look_starts_on_no_filter_where_the_top_level_sets_a_vtlook() {
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    let done = shared.change(Change::new(
        "vtlook = amber",
        Edit::Conf(vec![ConfEdit::Top {
            key: "vtlook",
            value: Some("amber".to_owned()),
        }]),
    ));
    assert!(done.is_ok(), "{done:?}");
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    let made = act(&mut tab, &shared, "n.view.looks");
    assert!(
        matches!(&one_change(&made).edit, Edit::Conf(e) if matches!(e.as_slice(),
        [ConfEdit::LookAdd { vtlook: Some(v), .. }] if v == "default")),
        "{made:?}"
    );
}

#[test]
fn r_asks_before_it_deletes_a_look_and_only_the_yes_deletes_it() {
    for (machine, build) in machines() {
        let mut shared = build();
        let mut tab = Looks::new(true);
        screen(&tab, &shared, 100, 30, true);
        go_to(&mut tab, &shared, |i| matches!(i, Item::Look(_)));
        let Some(Item::Look(name)) = tab.highlighted(&shared) else {
            panic!("{machine}: no look");
        };
        let made = act(&mut tab, &shared, "r.view.looks");
        let (question, then) = asked(&made);
        assert!(
            question.starts_with(&format!("Delete the look {name}?")),
            "{question}"
        );
        for terminal in list::users(shared.document().text(), &name) {
            assert!(question.contains(&terminal), "{machine}: {question}");
        }
        let yes = tab.update(&shared, then);
        let change = one_change(&yes);
        assert!(
            matches!(&change.edit, Edit::Conf(e) if matches!(e.as_slice(),
            [ConfEdit::LookRm(n)] if *n == name)),
            "{machine}: {change:?}"
        );
        apply(&mut shared, &yes);
        assert!(list::look(shared.document().text(), &name).is_none());
    }
}

#[test]
fn the_questions_fit_the_message_line_at_80_columns() {
    let shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    go_to(&mut tab, &shared, |i| matches!(i, Item::Look(_)));
    let (question, _) = asked(&act(&mut tab, &shared, "r.view.looks"));
    assert_eq!(
        question,
        "Delete the look big? The terminal desk then takes the default look."
    );
    let (question, _) = asked(&pointer_used(&shared).1);
    assert!(
        vtium_config_widgets::canvas::cells(&question) <= 80,
        "{question}"
    );
}

#[test]
fn r_marks_a_font_set_for_removal_and_r_again_takes_the_mark_back() {
    for (machine, build) in machines() {
        let mut shared = build();
        let mut tab = Looks::new(true);
        screen(&tab, &shared, 100, 30, true);
        go_to(
            &mut tab,
            &shared,
            |i| matches!(i, Item::FontSet(n) if n != "proof"),
        );
        let Some(Item::FontSet(name)) = tab.highlighted(&shared) else {
            panic!("{machine}: no font set");
        };
        let made = act(&mut tab, &shared, "r.view.looks");
        assert!(
            matches!(&one_change(&made).edit, Edit::FontSet { frit, edit: FontSetEdit::Remove } if *frit == name),
            "{machine}: {made:?}"
        );
        assert_eq!(said(&made).len(), 1);
        apply(&mut shared, &made);
        assert!(
            shared
                .document()
                .content()
                .font_sets
                .removed
                .contains(&name)
        );
        assert_eq!(tab.highlighted(&shared), Some(Item::FontSet(name.clone())));
        let made = act(&mut tab, &shared, "r.view.looks");
        assert!(
            matches!(
                &one_change(&made).edit,
                Edit::FontSet {
                    edit: FontSetEdit::Forget,
                    ..
                }
            ),
            "{machine}: {made:?}"
        );
        apply(&mut shared, &made);
        assert!(
            !shared
                .document()
                .content()
                .font_sets
                .removed
                .contains(&name)
        );
    }
    // a font set only the document adds is taken back out
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    let done = shared.change(Change::new(
        "add mine",
        Edit::FontSet {
            frit: "mine".to_owned(),
            edit: FontSetEdit::Keep("source=builtin:unifont\n".to_owned()),
        },
    ));
    assert!(done.is_ok(), "{done:?}");
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    go_to(&mut tab, &shared, |i| {
        *i == Item::FontSet("mine".to_owned())
    });
    let made = act(&mut tab, &shared, "r.view.looks");
    apply(&mut shared, &made);
    assert!(!list::font_sets(&shared).contains(&"mine".to_owned()));
}

/// The screen map's machine with its first look drawing vtium's own
/// pointer pictures, an active theme, and what Space says on that theme.
fn pointer_used(shared: &Shared) -> (Shared, Vec<Command>) {
    let mut shared2 = vtium_config_core::fixture::padnoir::shared();
    let look = list::looks(shared.document().text())[0].name.clone();
    let done = shared2.change(Change::new(
        "pointer",
        Edit::Conf(vec![ConfEdit::LookKey {
            name: look,
            key: "pointer-theme",
            value: Some("/usr/share/vtium/pointers.svg".to_owned()),
        }]),
    ));
    assert!(done.is_ok(), "{done:?}");
    let mut tab = Looks::new(true);
    screen(&tab, &shared2, 100, 30, true);
    go_to(&mut tab, &shared2, |i| *i == Item::Pointer(0));
    let made = act(&mut tab, &shared2, "space.view.resources");
    (shared2, made)
}

#[test]
fn space_asks_before_it_deactivates_what_a_look_uses() {
    let shared = vtium_config_core::fixture::padnoir::shared();
    // nothing uses the theme yet, so Space deactivates it at once
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    go_to(&mut tab, &shared, |i| *i == Item::Pointer(0));
    let made = act(&mut tab, &shared, "space.view.resources");
    one_change(&made);
    let (mut shared, made) = pointer_used(&shared);
    let (question, then) = asked(&made);
    assert!(
        question.starts_with("Deactivate vtium's own pictures?") && question.contains("big"),
        "{question}"
    );
    let mut tab = Looks::new(true);
    screen(&tab, &shared, 100, 30, true);
    let yes = tab.update(&shared, then);
    let change = one_change(&yes);
    assert!(
        matches!(
            &change.edit,
            Edit::Active {
                change: Some(Batch::Deactivate),
                ..
            }
        ),
        "{change:?}"
    );
    apply(&mut shared, &yes);
    let r = &shared.found.pointers[0];
    assert!(!vtium_config_tab_looks::card::active(&shared, r));
}

#[test]
fn f1_on_the_form_says_what_a_font_set_is_and_each_role_holds() {
    let shared = vtium_config_core::fixture::padnoir::shared();
    let mut tab = form_open(&shared);
    let text = tab.full_text(&shared).unwrap_or_default();
    for label in [
        "Text font",
        "Box drawing",
        "Scripts",
        "Symbols and icons",
        "Emoji",
        "Fallback",
    ] {
        assert!(text.contains(&format!("{label}:")), "{label}: {text}");
    }
    act(&mut tab, &shared, "list.system-fonts");
    let text = tab.full_text(&shared).unwrap_or_default();
    assert!(
        text.contains("/usr/share/fonts/TTF/DejaVuSansMono.ttf"),
        "{text}"
    );
    act(&mut tab, &shared, "esc.layer.list");
    act(&mut tab, &shared, "end.layer.new-frit");
    act(&mut tab, &shared, "up.layer.new-frit");
    act(&mut tab, &shared, "enter.layer.new-frit");
    assert!(tab.full_text(&shared).is_none());
}

#[test]
fn clicks_highlight_a_row_then_open_it_and_keep_a_choice() {
    for (machine, build) in machines() {
        let shared = build();
        let mut tab = form_open(&shared);
        let places = tab.places(&shared);
        assert_eq!(places.form.len(), new_frit::ROWS, "{machine}");
        let (rect, row) = places.form[5];
        assert_eq!(row, 5);
        assert!(click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y).is_empty());
        assert_eq!(tab.form().map(|f| f.row), Some(5));
        assert_eq!(tab.node(), "looks.new-frit", "{machine}");
        click(&mut tab, &shared, "click-row.view", rect.x + 3, rect.y);
        assert_eq!(tab.node(), "looks.new-frit.list", "{machine}");
        screen(&tab, &shared, 100, 30, true);
        let list = tab
            .places(&shared)
            .list
            .unwrap_or_else(|| panic!("no list box"));
        let (r, choice) = list.rows[1];
        click(&mut tab, &shared, "click-choice.layer.list", r.x + 2, r.y);
        click(&mut tab, &shared, "click-choice.layer.list", r.x + 2, r.y);
        assert_eq!(tab.node(), "looks.new-frit", "{machine}");
        let frit = tab.form().map(|f| f.frit.clone()).unwrap_or_default();
        let c = new_frit::choices(&shared, Field::Fallback, &frit);
        assert_eq!(c.current(), Some(choice), "{machine}");
    }
}

#[test]
fn the_form_s_keys_read_no_folder() {
    for (_, build) in machines() {
        let shared = build();
        let before = found::scans();
        let mut tab = form_open(&shared);
        for row in [
            "list.system-fonts",
            "down.layer.list",
            "enter.layer.list",
            "down.layer.new-frit",
            "list.system-fonts",
            "end.layer.list",
            "esc.layer.list",
        ] {
            act(&mut tab, &shared, row);
            screen(&tab, &shared, 100, 30, true);
        }
        screen(&tab, &shared, 80, 25, true);
        assert_eq!(found::scans(), before);
    }
}
