//! The Effects tab driven by its events: its list in groups, the filter
//! line and its list of groups, an item's detail, and Space through the
//! controls the resource tabs share. `tui.rs` says how the tests drive the
//! TUI, and `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;
use vtium_config::batch::{Change, State};
use vtium_config::tui::desk::Op;
use vtium_config::tui::input::Key;
use vtium_config::tui::model::{App, Effect, Event, View};
use vtium_config::tui::nav;

/// The app on the Effects tab at 80×25.
fn effects() -> App {
    let mut a = app(80, 25);
    press(&mut a, Key::Char('2'));
    assert_eq!(a.nav.view, View::Effects);
    a
}

/// Moves the highlight to the item `name`, from the filter line down.
fn to(a: &mut App, name: &str) {
    press(a, Key::Home);
    for _ in 0..20 {
        if highlighted(a, name) {
            return;
        }
        press(a, Key::Down);
    }
    panic!("never reached {name}:\n{}", text(a));
}

/// Whether the highlighted item is `name`.
fn highlighted(a: &App, name: &str) -> bool {
    a.effects
        .highlighted(&a.shared)
        .is_some_and(|item| match item {
            vtium_config::tui::tabs::effects::Effect::Vtlook(i) => a
                .shared
                .inputs
                .effects
                .vtlooks
                .get(i)
                .is_some_and(|v| v.name == name || (name == "No filter" && i == 0)),
            other => vtium_config::tui::tabs::effects::Effects::resource(&a.shared, other)
                .is_some_and(|r| r.name() == name),
        })
}

#[test]
fn the_list_shows_no_filter_then_the_presets_then_the_administrator_s_vtlooks() {
    let a = effects();
    let screen = text(&a);
    let at = |s: &str| screen.find(s).unwrap_or_else(|| panic!("{s}:\n{screen}"));
    assert!(at(" Show: All effects") < at(" vtlooks"), "{screen}");
    assert!(at("No filter") < at("amber"), "{screen}");
    assert!(at("amber") < at("sepia"), "{screen}");
    assert!(at(" vtlooks") < at(" Color maps"), "{screen}");
    // the preset green is hidden by the administrator's green, which does
    // not read
    assert!(
        screen.contains("preset, hidden by the administrator's vtlook of this name"),
        "{screen}"
    );
    assert!(
        screen.contains("administrator's, does not read"),
        "{screen}"
    );
    assert!(screen.contains("sepia.cube"), "{screen}");
    assert!(screen.contains("active, trusted"), "{screen}");
    assert!(
        screen.contains("not active, fails the trust rule"),
        "{screen}"
    );
    assert!(screen.contains("not usable"), "{screen}");
    assert!(screen.contains("No shaders are installed."), "{screen}");
    assert_eq!(nav::node(&a), "effects");
}

#[test]
fn the_filter_line_opens_the_groups_and_shows_one() {
    let mut a = effects();
    press(&mut a, Key::Enter);
    assert_eq!(nav::node(&a), "effects.filter");
    // the action keys do nothing while the list of groups is open
    assert!(press(&mut a, Key::Char('s')).is_empty());
    press(&mut a, Key::Down);
    press(&mut a, Key::Down);
    press(&mut a, Key::Enter);
    assert_eq!(nav::node(&a), "effects");
    let screen = text(&a);
    assert!(screen.contains(" Show: Color maps"), "{screen}");
    assert!(!screen.contains("amber"), "{screen}");
    assert!(screen.contains("warm.cube"), "{screen}");
    // Esc closes the list of groups and keeps the group shown
    press(&mut a, Key::Enter);
    press(&mut a, Key::Escape);
    assert!(text(&a).contains(" Show: Color maps"));
}

#[test]
fn a_vtlook_s_detail_shows_its_filters_files_and_looks() {
    let mut a = effects();
    to(&mut a, "sepia");
    press(&mut a, Key::Enter);
    assert_eq!(nav::node(&a), "effects.effect");
    let screen = text(&a);
    assert!(screen.contains(" vtlook sepia"), "{screen}");
    assert!(
        screen.contains("File: /etc/vtium/looks/sepia.vtlook"),
        "{screen}"
    );
    assert!(screen.contains("cube: file sepia.cube"), "{screen}");
    assert!(
        screen.contains("/etc/vtium/looks/sepia.cube, active"),
        "{screen}"
    );
    assert!(screen.contains("Used by look proof."), "{screen}");
    // Down scrolls the lines under the title, which stays
    press(&mut a, Key::Down);
    let scrolled = text(&a);
    assert!(scrolled.contains(" vtlook sepia"), "{scrolled}");
    assert!(!scrolled.contains("File: /etc"), "{scrolled}");
    press(&mut a, Key::Escape);
    assert_eq!(nav::node(&a), "effects");
    // No filter's detail says it is built in
    to(&mut a, "No filter");
    press(&mut a, Key::Enter);
    let screen = text(&a);
    assert!(
        screen.contains("File: none, it is built into vtium"),
        "{screen}"
    );
    assert!(screen.contains("It has no filter"), "{screen}");
}

#[test]
fn a_color_map_s_detail_shows_its_marks_and_the_check_it_fails() {
    let mut a = effects();
    to(&mut a, "loose.cube");
    press(&mut a, Key::Enter);
    let screen = text(&a);
    assert!(screen.contains(" color map loose.cube"), "{screen}");
    assert!(screen.contains("Active: not active"), "{screen}");
    assert!(screen.contains("the file belongs to user 1000"), "{screen}");
    assert!(screen.contains("No look uses it."), "{screen}");
}

#[test]
fn space_activates_a_color_map_through_the_desk_and_the_mark_follows() {
    let mut a = effects();
    to(&mut a, "warm.cube");
    let out = press(&mut a, Key::Char(' '));
    let op = Op::Resource {
        path: "/etc/vtium/looks/warm.cube".into(),
        change: Change::Activate,
    };
    assert_eq!(out, [Effect::Desk(op.clone())]);
    // the desk's answer marks the color map and says what changed
    let mut states = active();
    let done = vtium_config::tui::fixture::resource_op(
        &mut states,
        std::path::Path::new("/etc/vtium/looks/warm.cube"),
        Change::Activate,
    );
    a.event(Event::Desked(op, Ok(done)));
    let warm = a
        .shared
        .inputs
        .effects
        .maps
        .iter()
        .find(|m| m.name() == "warm.cube")
        .map(|m| m.state);
    assert_eq!(warm, Some(State::Activates));
    let screen = text(&a);
    assert!(screen.contains("activates on install, trusted"), "{screen}");
    assert!(
        screen.contains("The draft activates this color map."),
        "{screen}"
    );
    // a vtlook takes no Space
    to(&mut a, "amber");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
}

#[test]
fn space_asks_before_it_deactivates_a_color_map_that_a_look_uses() {
    let mut a = effects();
    to(&mut a, "sepia.cube");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert_eq!(nav::node(&a), "any.question.deactivate");
    let screen = text(&a);
    assert!(screen.contains("Look proof uses it"), "{screen}");
    // Esc goes back without acting
    press(&mut a, Key::Escape);
    assert_eq!(nav::node(&a), "effects");
    press(&mut a, Key::Char(' '));
    let out = press(&mut a, Key::Char('y'));
    assert_eq!(
        out,
        [Effect::Desk(Op::Resource {
            path: "/etc/vtium/looks/sepia.cube".into(),
            change: Change::Deactivate,
        })]
    );
}

#[test]
fn space_on_a_color_map_vtium_cannot_use_says_why() {
    let mut a = effects();
    to(&mut a, "loose.cube");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert!(text(&a).contains("fails the trust rule"));
    to(&mut a, "mine.cube");
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    let screen = text(&a);
    assert!(screen.contains("in your own folder"), "{screen}");
    // a refusal from the desk goes on the message line
    a.event(Event::Desked(
        Op::Resource {
            path: "/x.cube".into(),
            change: Change::Activate,
        },
        Err("the file is not active".into()),
    ));
    assert!(text(&a).contains("the file is not active"));
}
