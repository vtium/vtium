//! The shell's tests: the App driven with two stand-in tabs over the
//! screen map's machine and the large machine, at 100×30 and at 80×25. A
//! key or a click goes in, and the test reads the state, the screen and
//! the effects that come out. Each row of ADR-1403 D11's key table that
//! the shell owns has a test here, named by the row.
//!
//! The screens are kept as snapshots under `tests/snapshots/`, each with
//! a grid of the attributes every cell keeps without color.
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tui --test app`
//! writes them again.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use common::{Machine, SIZES, alt, click, draw, keys, locate, plain, press, text};
use vtium_config_core::ui::{Action, Carries, Effect};
use vtium_config_tui::app::update::{Route, route};
use vtium_config_tui::nav::menus::Which;
use vtium_config_widgets::keys::{Code, Key};

const MAP: (u16, u16) = (100, 30);

fn count(app: &vtium_config_tui::app::App) -> usize {
    app.shared().document().count()
}

fn message(app: &vtium_config_tui::app::App) -> Option<String> {
    app.shared().message.as_ref().map(|m| m.line.clone())
}

fn last(log: &common::Log) -> Option<Action> {
    log.borrow().last().cloned()
}

// The top row's Alt letters, F10 and the menus.

#[test]
fn alt_f_view_opens_the_file_menu_in_either_case() {
    for key in [alt('f'), alt('F')] {
        let (mut app, ..) = common::app(Machine::Map, MAP);
        press(&mut app, key);
        assert_eq!(app.node(), "any.menu.file");
        assert_eq!(app.nav().menu().map(|m| m.which), Some(Which::File));
    }
}

#[test]
fn alt_a_view_opens_the_actions_menu_with_the_tab_s_own_items_after_its_own() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Alt-L", "Alt-A"]);
    assert_eq!(app.node(), "any.menu.actions");
    let screen = text(&app);
    for item in [
        "Try live · still asking this terminal",
        "Read devices again",
        "Install as a system font… · on a font of your own",
        "Picture of the console",
    ] {
        assert!(screen.contains(item), "{item:?} is missing:\n{screen}");
    }
}

#[test]
fn alt_tab_view_goes_to_the_tab_of_its_letter_in_either_case_and_not_from_a_level() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, alt('l'));
    assert_eq!(app.nav().tab(), 1);
    press(&mut app, alt('N'));
    assert_eq!(app.nav().tab(), 0);
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "fonts.font");
    press(&mut app, alt('l'));
    assert_eq!(app.nav().tab(), 0, "an Alt letter acts only on a tab");
}

#[test]
fn f10_view_opens_the_menu_bar_on_file_and_f10_layer_menu_closes_it() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, plain(Code::F(10)));
    assert_eq!(app.node(), "any.menu.file");
    press(&mut app, plain(Code::F(10)));
    assert_eq!(app.node(), "fonts");
}

#[test]
fn esc_layer_menu_closes_the_submenu_first_then_the_menu() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Alt-F", "o"]);
    assert_eq!(app.node(), "any.menu.file.open");
    press(&mut app, Key::ESCAPE);
    assert_eq!(app.node(), "any.menu.file");
    press(&mut app, Key::ESCAPE);
    assert_eq!(app.node(), "fonts");
}

#[test]
fn updown_layer_menu_moves_past_separators_and_greyed_items() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    // the screen map's document has a step neither to undo nor to redo,
    // so New is the first item that acts
    press(&mut app, alt('f'));
    let at = |app: &vtium_config_tui::app::App| app.nav().menu().and_then(|m| m.highlight);
    assert_eq!(at(&app), Some(0), "New");
    press(&mut app, plain(Code::Down));
    assert_eq!(at(&app), Some(1), "Open");
    press(&mut app, plain(Code::Down));
    assert_eq!(at(&app), Some(7), "Show changes, past the greyed items");
    press(&mut app, plain(Code::Down));
    assert_eq!(at(&app), Some(8), "Edit as text");
    press(&mut app, plain(Code::Down));
    assert_eq!(at(&app), Some(10), "Revert to saved, past the separator");
    press(&mut app, plain(Code::Down));
    assert_eq!(at(&app), Some(11), "History");
    press(&mut app, plain(Code::Down));
    assert_eq!(at(&app), Some(13), "Quit");
    press(&mut app, plain(Code::Down));
    assert_eq!(at(&app), Some(0), "round to New");
    press(&mut app, plain(Code::Up));
    assert_eq!(at(&app), Some(13));
}

#[test]
fn leftright_layer_menu_opens_a_submenu_or_the_menu_beside() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, alt('f'));
    press(&mut app, plain(Code::Down));
    press(&mut app, plain(Code::Right));
    assert_eq!(
        app.node(),
        "any.menu.file.open",
        "Right opens Open's submenu"
    );
    press(&mut app, plain(Code::Left));
    assert_eq!(app.node(), "any.menu.file", "Left closes it");
    press(&mut app, plain(Code::Down));
    press(&mut app, plain(Code::Down));
    press(&mut app, plain(Code::Right));
    assert_eq!(
        app.node(),
        "any.menu.actions",
        "Right on Quit moves to Actions"
    );
    press(&mut app, plain(Code::Left));
    assert_eq!(app.node(), "any.menu.file");
}

#[test]
fn letters_layer_menu_file_choose_their_items_and_a_greyed_one_does_nothing() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Alt-F", "s"]);
    assert_eq!(
        app.node(),
        "any.menu.file",
        "Save is greyed, so s does nothing"
    );
    assert_eq!(message(&app), None);
    let effects = press(&mut app, Key::char('q'));
    assert_eq!(effects, [Effect::Exit(0)], "q chooses Quit");
}

#[test]
fn letters_layer_menu_actions_choose_their_items_and_a_greyed_one_does_nothing() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    // this terminal has not answered yet, and the font is a system font
    keys(&mut app, &["Alt-A", "t", "i"]);
    assert_eq!(app.node(), "any.menu.actions");
    assert_eq!(app.leaving(), None);
    assert_eq!(
        press(&mut app, Key::char('r')),
        [Effect::Desk(vtium_config_core::ui::Op::Devices)]
    );
    assert_eq!(app.node(), "fonts");
}

#[test]
fn enter_layer_menu_chooses_the_highlighted_item() {
    let (mut app, fonts, _) = common::app(Machine::Map, MAP);
    press(&mut app, Key::char(' '));
    assert_eq!(last(&fonts).map(|a| a.row), Some("space.view.resources"));
    let before = count(&app);
    press(&mut app, alt('f'));
    press(&mut app, plain(Code::Down));
    press(&mut app, plain(Code::Down));
    assert_eq!(app.nav().menu().and_then(|m| m.highlight), Some(3), "Undo");
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "fonts");
    assert_eq!(count(&app), before - 1);
}

#[test]
fn enter_layer_menu_history_opens_the_sheet_of_the_version_chosen() {
    for machine in Machine::BOTH {
        let (mut app, ..) = common::app(machine, MAP);
        keys(&mut app, &["Alt-F", "h"]);
        assert_eq!(app.node(), "any.menu.file.history");
        let screen = text(&app);
        // each entry is a time on the hour, and none is greyed
        assert!(screen.contains("│ 2026-"), "{screen}");
        assert!(!screen.contains(":00 · not built"), "{screen}");
        press(&mut app, plain(Code::Enter));
        assert_eq!(app.node(), "any.history.version", "{}", machine.name());
    }
}

#[test]
fn click_menu_view_opens_the_menu_clicked() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    let (x, y) = locate(&app, "Actions");
    click(&mut app, x, y);
    assert_eq!(app.node(), "any.menu.actions");
    let (x, y) = locate(&app, "File");
    click(&mut app, x, y);
    assert_eq!(
        app.node(),
        "any.menu.file",
        "a click on the other menu opens it"
    );
}

#[test]
fn click_item_layer_menu_chooses_the_item_and_click_outside_closes() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, Key::char(' '));
    press(&mut app, alt('f'));
    let (x, y) = locate(&app, "Undo mark 1");
    click(&mut app, x, y);
    assert_eq!(app.node(), "fonts");
    assert_eq!(
        message(&app).as_deref(),
        Some("Undid mark 1. [Ctrl+Y] redoes it.")
    );
    press(&mut app, alt('f'));
    click(&mut app, 90, 20);
    assert_eq!(app.node(), "fonts");
}

#[test]
fn click_tab_view_goes_to_the_tab() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    let (x, y) = locate(&app, "Looks");
    click(&mut app, x + 2, y);
    assert_eq!(app.nav().tab(), 1);
}

#[test]
fn click_row_view_reaches_the_tab_with_the_cell_in_the_viewer() {
    let (mut app, fonts, _) = common::app(Machine::Map, MAP);
    click(&mut app, 5, 4);
    let action = last(&fonts).expect("the tab is handed the click");
    assert_eq!(action.row, "click-row.view");
    // the viewer starts under the top row and its rule
    assert_eq!(action.carries, common::viewer_click(5, 2));
}

#[test]
fn click_button_any_does_what_the_button_s_key_does() {
    let (mut app, fonts, _) = common::app(Machine::Map, MAP);
    let (x, y) = locate(&app, "[Space] Activate");
    click(&mut app, x + 9, y);
    assert_eq!(last(&fonts).map(|a| a.row), Some("space.view.resources"));
    let (x, y) = locate(&app, "[?] Search");
    click(&mut app, x, y);
    assert_eq!(app.node(), "any.search");
    keys(&mut app, &["Esc"]);
    let (x, y) = locate(&app, "[.] More");
    click(&mut app, x, y);
    assert_eq!(app.nav().menu().map(|m| m.which), Some(Which::More));
    keys(&mut app, &["Esc"]);
    let (x, y) = locate(&app, "[q] Quit");
    assert_eq!(click(&mut app, x + 4, y), [Effect::Exit(0)]);
}

#[test]
fn click_facts_view_on_the_count_not_saved_opens_show_changes() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    let (x, y) = locate(&app, "not saved");
    click(&mut app, x, y);
    assert_eq!(app.node(), "any.changes");
}

// Undo and redo.

#[test]
fn ctrl_z_view_undoes_the_last_change_and_says_so_and_ctrl_y_view_redoes_it() {
    for machine in Machine::BOTH {
        let (mut app, ..) = common::app(machine, MAP);
        let start = count(&app);
        press(&mut app, Key::char(' '));
        assert_eq!(count(&app), start + 1);
        press(&mut app, Key::ctrl('z'));
        assert_eq!(count(&app), start);
        assert_eq!(
            message(&app).as_deref(),
            Some("Undid mark 1. [Ctrl+Y] redoes it.")
        );
        press(&mut app, Key::ctrl('z'));
        assert_eq!(message(&app).as_deref(), Some("Nothing to undo."));
        press(&mut app, Key::ctrl('y'));
        assert_eq!(count(&app), start + 1);
        assert_eq!(
            message(&app).as_deref(),
            Some("Redid mark 1. [Ctrl+Z] undoes it.")
        );
        press(&mut app, Key::ctrl('y'));
        assert_eq!(message(&app).as_deref(), Some("Nothing to redo."));
    }
}

#[test]
fn ctrl_z_layer_and_ctrl_y_layer_act_and_what_is_open_stays_open() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    let start = count(&app);
    press(&mut app, Key::char(' '));
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "fonts.font");
    press(&mut app, Key::ctrl('z'));
    assert_eq!(count(&app), start);
    assert_eq!(app.node(), "fonts.font");
    press(&mut app, Key::ctrl('y'));
    assert_eq!(count(&app), start + 1);
    assert_eq!(app.node(), "fonts.font");
}

#[test]
fn ctrl_z_typing_and_ctrl_y_typing_do_nothing_in_the_search_row() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, Key::char(' '));
    let before = count(&app);
    press(&mut app, Key::char('?'));
    assert_eq!(app.node(), "any.search");
    press(&mut app, Key::ctrl('z'));
    press(&mut app, Key::ctrl('y'));
    assert_eq!(count(&app), before);
    assert_eq!(app.node(), "any.search");
}

#[test]
fn ctrl_z_layer_menu_and_ctrl_y_layer_menu_do_nothing_in_a_menu() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, Key::char(' '));
    let before = count(&app);
    press(&mut app, alt('f'));
    press(&mut app, Key::ctrl('z'));
    assert_eq!(count(&app), before, "Ctrl+Z undid in a menu");
    assert_eq!(app.node(), "any.menu.file");
    press(&mut app, alt('f'));
    keys(&mut app, &["Esc"]);
    press(&mut app, Key::ctrl('z'));
    press(&mut app, alt('f'));
    press(&mut app, Key::ctrl('y'));
    assert_eq!(count(&app), before - 1, "Ctrl+Y redid in a menu");
}

#[test]
fn ctrl_z_and_ctrl_y_in_save_s_screens_are_the_shell_s_and_do_nothing() {
    for id in [
        "ctrl-z.layer.save",
        "ctrl-z.layer.save-changed",
        "ctrl-z.layer.save-settle",
        "ctrl-y.layer.save",
        "ctrl-y.layer.save-changed",
        "ctrl-y.layer.save-settle",
        "ctrl-z.typing",
        "ctrl-y.typing",
        "ctrl-z.layer.menu",
        "ctrl-y.layer.menu",
    ] {
        assert_eq!(route(id), Route::Nothing, "{id}");
    }
    assert_eq!(route("ctrl-z.view"), Route::Undo);
    assert_eq!(route("ctrl-z.layer"), Route::Undo);
    assert_eq!(route("ctrl-y.view"), Route::Redo);
    assert_eq!(route("ctrl-y.layer"), Route::Redo);
}

#[test]
fn the_file_menu_names_the_step_undo_and_redo_take_and_greys_them_with_none() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, alt('f'));
    let screen = text(&app);
    assert!(screen.contains("Undo · nothing to undo"), "{screen}");
    assert!(screen.contains("Redo · nothing to redo"), "{screen}");
    keys(&mut app, &["Esc", "Space", "Alt-F"]);
    let screen = text(&app);
    assert!(screen.contains("Undo mark 1"), "{screen}");
    keys(&mut app, &["u", "Alt-F"]);
    let screen = text(&app);
    assert!(screen.contains("Redo mark 1"), "{screen}");
    assert!(screen.contains("Undo · nothing to undo"), "{screen}");
}

#[test]
fn the_bar_offers_undo_and_redo_only_while_there_is_a_step() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    // Undo and Redo sit on the bar or behind More, by the width
    let keys_on = |app: &vtium_config_tui::app::App| -> Vec<String> {
        vtium_config_tui::view::layout(app)
            .bar
            .buttons
            .iter()
            .map(|b| format!("[{}] {}", b.key, b.label))
            .collect()
    };
    let has = |app: &vtium_config_tui::app::App, b: &str| keys_on(app).iter().any(|k| k == b);
    assert!(!has(&app, "[Ctrl+Z] Undo"), "{:?}", keys_on(&app));
    assert!(!has(&app, "[Ctrl+Y] Redo"), "{:?}", keys_on(&app));
    press(&mut app, Key::char(' '));
    assert!(has(&app, "[Ctrl+Z] Undo"), "{:?}", keys_on(&app));
    assert!(!has(&app, "[Ctrl+Y] Redo"), "{:?}", keys_on(&app));
    press(&mut app, Key::ctrl('z'));
    assert!(has(&app, "[Ctrl+Y] Redo"), "{:?}", keys_on(&app));
    assert!(!has(&app, "[Ctrl+Z] Undo"), "{:?}", keys_on(&app));
}

// Quitting.

#[test]
fn q_view_quits_at_once_and_the_farewell_counts_the_changes_not_saved() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    assert_eq!(press(&mut app, Key::char('q')), [Effect::Exit(0)]);
    assert_eq!(
        app.farewell().as_deref(),
        Some("vtium-config: workstation has 1 change, not saved. vtium config tui opens it again.")
    );
    assert!(
        press(&mut app, Key::char('q')).is_empty(),
        "nothing after leaving"
    );
    let (mut app, ..) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Space", "Space", "q"]);
    assert_eq!(
        app.farewell().as_deref(),
        Some(
            "vtium-config: workstation has 3 changes, not saved. vtium config tui opens them again."
        )
    );
}

#[test]
fn a_document_with_nothing_unsaved_leaves_without_a_line() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    // the stand-in Fonts tab's n puts the installed file's text back
    keys(&mut app, &["n"]);
    assert_eq!(count(&app), 0);
    assert_eq!(press(&mut app, Key::char('q')), [Effect::Exit(0)]);
    assert_eq!(app.farewell(), None);
}

#[test]
fn q_layer_quits_from_a_level_too() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "fonts.font");
    assert_eq!(press(&mut app, Key::char('q')), [Effect::Exit(0)]);
}

#[test]
fn q_typing_types_the_letter_and_ctrl_c_any_still_quits() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    keys(&mut app, &["?", "q"]);
    assert_eq!(app.leaving(), None);
    assert_eq!(
        app.nav().search().map(|s| s.entry().to_owned()).as_deref(),
        Some("q")
    );
    assert_eq!(press(&mut app, Key::ctrl('c')), [Effect::Exit(0)]);
}

#[test]
fn file_quit_quits_at_once() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, alt('f'));
    let (x, y) = locate(&app, "Quit ");
    assert_eq!(click(&mut app, x, y), [Effect::Exit(0)]);
}

#[test]
fn ctrl_c_any_quits_and_ctrl_c_question_cancels_the_question() {
    let (mut app, _, looks) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Alt-L", "n"]);
    assert_eq!(app.node(), "any.question.confirm");
    assert!(text(&app).contains("Make a new look?"));
    assert!(press(&mut app, Key::ctrl('c')).is_empty());
    assert_eq!(app.node(), "looks");
    assert_eq!(press(&mut app, Key::ctrl('c')), [Effect::Exit(0)]);
    assert_eq!(looks.borrow().len(), 1, "the tab heard only n");
}

#[test]
fn a_signal_quits_with_its_status() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    let effects = app.event(vtium_config_tui::app::Event::Signal(15));
    assert_eq!(effects, [Effect::Exit(143)]);
}

// The rows that are the same everywhere.

#[test]
fn tab_shift_tab_and_the_digits_go_to_the_tabs_on_a_tab_and_not_from_a_level() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, plain(Code::Tab));
    assert_eq!(app.nav().tab(), 1);
    press(&mut app, plain(Code::Tab));
    assert_eq!(app.nav().tab(), 0, "round to the first");
    keys(&mut app, &["Shift-Tab"]);
    assert_eq!(app.nav().tab(), 1);
    keys(&mut app, &["1"]);
    assert_eq!(app.nav().tab(), 0);
    keys(&mut app, &["6"]);
    assert_eq!(app.nav().tab(), 0, "no sixth tab here");
    keys(&mut app, &["Enter", "Tab"]);
    assert_eq!(app.nav().tab(), 0);
    assert_eq!(app.node(), "fonts.font");
}

#[test]
fn esc_on_a_level_closes_it_and_tells_the_tab_and_each_tab_keeps_its_own() {
    let (mut app, fonts, _) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Enter", "Alt-L"]);
    assert_eq!(app.nav().tab(), 0, "Alt waits until the level closes");
    keys(&mut app, &["Esc"]);
    assert_eq!(app.node(), "fonts");
    assert_eq!(last(&fonts).map(|a| a.row), Some("esc.layer"));
    keys(&mut app, &["Enter", "Esc", "Alt-L", "Enter", "Alt-N"]);
    assert_eq!(app.node(), "looks.look");
}

#[test]
fn a_question_s_yes_hands_the_tab_its_action_and_no_changes_nothing() {
    let (mut app, _, looks) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Alt-L", "n", "x", "q", "Tab"]);
    assert_eq!(app.node(), "any.question.confirm", "another key leaves it");
    assert_eq!(app.leaving(), None, "q waits for the answer");
    keys(&mut app, &["Esc"]);
    assert_eq!(app.node(), "looks");
    keys(&mut app, &["n", "y"]);
    assert_eq!(
        last(&looks).map(|a| a.carries),
        Some(Carries::Item(1)),
        "the yes hands the tab the question's action"
    );
    assert_eq!(message(&app).as_deref(), Some("Made the look new-1."));
}

#[test]
fn question_view_opens_the_search_only_where_the_node_names_fields() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, Key::char('?'));
    assert_eq!(app.node(), "any.search");
    keys(&mut app, &["c", "u", "l"]);
    assert_eq!(
        app.nav().search().map(|s| s.matches().to_vec()),
        Some(vec![1])
    );
    assert!(text(&app).contains("? cul_"), "{}", text(&app));
    keys(&mut app, &["Esc"]);
    assert_eq!(app.node(), "fonts");
    keys(&mut app, &["Enter", "?"]);
    assert_eq!(
        app.node(),
        "fonts.font",
        "a font's detail names no fields to search"
    );
    keys(&mut app, &["Esc", "Alt-L", "?"]);
    assert_eq!(app.node(), "looks", "Looks names no fields to search");
}

#[test]
fn period_view_opens_more_with_what_the_bar_leaves_out_and_an_item_does_what_its_key_does() {
    let (mut app, fonts, _) = common::app(Machine::Map, (80, 25));
    press(&mut app, Key::char('.'));
    assert_eq!(app.nav().menu().map(|m| m.which), Some(Which::More));
    let screen = text(&app);
    assert!(screen.contains("Try live"), "{screen}");
    let (x, y) = locate(&app, "Try live");
    click(&mut app, x, y);
    assert_eq!(last(&fonts).map(|a| a.row), Some("t.view.fonts"));
    assert_eq!(
        message(&app).as_deref(),
        Some("Try live waits for this terminal's answers. Try again in a moment.")
    );
}

#[test]
fn ctrl_s_is_not_built_yet_and_says_so() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    keys(&mut app, &["Ctrl-S"]);
    assert_eq!(message(&app).as_deref(), Some("Save is not built yet."));
}

#[test]
fn the_file_menu_greys_each_item_not_built_yet() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    press(&mut app, alt('f'));
    let screen = text(&app);
    assert!(screen.contains("Save… · not built yet"), "{screen}");
    for item in ["Show changes", "Edit as text…", "Revert to saved…"] {
        assert!(
            !screen.contains(&format!("{item} · ")),
            "{item:?} acts:\n{screen}"
        );
    }
}

#[test]
fn below_the_minimum_size_the_screen_says_so_and_q_quits() {
    let (mut app, ..) = common::app(Machine::Map, (60, 20));
    assert_eq!(app.node(), "any.too-small");
    assert!(text(&app).starts_with("vtium-config needs 80×25; this screen is 60×20."));
    keys(&mut app, &["Tab"]);
    assert_eq!(app.nav().tab(), 0);
    assert_eq!(press(&mut app, Key::char('q')), [Effect::Exit(0)]);
}

#[test]
fn a_tab_s_effects_go_to_the_loop_and_its_requests_to_the_navigation_module() {
    let (mut app, ..) = common::app(Machine::Map, MAP);
    assert!(press(&mut app, plain(Code::Enter)).is_empty());
    assert_eq!(app.nav().levels(0), ["fonts.font"]);
}

// The screens.

/// Draws the screen that `setup` reaches, on both machines at both sizes,
/// checks that each row is the screen's width, that no mark is carried by
/// color alone, and that `musts` show at both sizes, and compares the four
/// screens with the snapshot `name`.
fn screens(name: &str, setup: &dyn Fn(&mut vtium_config_tui::app::App), musts: &[&str]) {
    let mut out = String::new();
    for machine in Machine::BOTH {
        for size in SIZES {
            let (mut app, ..) = common::app(machine, size);
            setup(&mut app);
            let color = draw(&app);
            let (mut mono, ..) = common::app_mono(machine, size);
            setup(&mut mono);
            let mono = draw(&mono);
            let what = format!("{name} on {} at {}×{}", machine.name(), size.0, size.1);
            common::rows_fit(&color, &what);
            common::marks_survive_without_color(&color, &mono, &what);
            let screen = common::rows(&color).join("\n");
            for must in musts {
                assert!(
                    screen.contains(must),
                    "{what}: {must:?} is missing:\n{screen}"
                );
            }
            out.push_str(&format!("== {what}\n"));
            out.push_str(&common::snapshot_text(&color));
        }
    }
    common::snapshot(name, &out);
}

#[test]
fn the_frame_around_a_tab_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-fonts",
        &|_| {},
        &["File", "Actions", "Fonts", "Looks", "[q] Quit"],
    );
}

#[test]
fn the_file_menu_after_a_change_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-file-menu",
        &|app| {
            keys(app, &["Space", "Alt-F"]);
        },
        &[
            "Undo mark 1",
            "Ctrl+Z",
            "Redo · nothing to redo",
            "Quit",
            "History",
        ],
    );
}

#[test]
fn the_file_menu_s_open_submenu_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-file-open",
        &|app| {
            keys(app, &["Alt-F", "o"]);
        },
        &["Manage configurations…"],
    );
}

#[test]
fn the_file_menu_s_history_submenu_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-file-history",
        &|app| {
            keys(app, &["Alt-F", "h"]);
        },
        &["History"],
    );
}

#[test]
fn the_actions_menu_on_looks_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-actions-menu",
        &|app| {
            keys(app, &["Alt-L", "Alt-A"]);
        },
        &["Try live", "Picture of the console"],
    );
}

#[test]
fn the_message_after_undo_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-undo-message",
        &|app| {
            keys(app, &["Space", "Ctrl-Z"]);
        },
        &["Undid mark 1. [Ctrl+Y] redoes it."],
    );
}

#[test]
fn a_question_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-question",
        &|app| {
            keys(app, &["Alt-L", "n"]);
        },
        &["Make a new look?", "[y] Yes", "[Esc] Cancel"],
    );
}

#[test]
fn more_s_menu_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-more",
        &|app| {
            keys(app, &["Space", "."]);
        },
        &["[q] Quit"],
    );
}

#[test]
fn the_search_row_at_both_sizes_with_both_fixtures() {
    screens(
        "shell-search",
        &|app| {
            keys(app, &["?", "n", "o"]);
        },
        &["? no_"],
    );
}

// The loop and the terminal.

#[test]
fn the_loop_draws_after_each_turn_and_leaves_on_q_with_the_farewell() {
    use vtium_config_tui::app::Size;
    use vtium_config_tui::port::{Wake, drive};
    let (app, ..) = common::app(Machine::Map, MAP);
    let mut port = common::Script::new(
        Size::grid(100, 30),
        vec![
            vec![Wake::Bytes(b" ".to_vec())],
            vec![Wake::Bytes(b"q".to_vec())],
        ],
    );
    let left = drive(&mut port, app).expect("the loop runs");
    assert_eq!(left.status, 0);
    assert_eq!(
        left.line.as_deref(),
        Some(
            "vtium-config: workstation has 2 changes, not saved. vtium config tui opens them again."
        )
    );
    let written = String::from_utf8_lossy(&port.written);
    assert!(written.contains("Terminus"), "the first frame is drawn");
    // ratatui writes only the cells that change, so the frame after Space
    // writes the count's new digit
    let first = written.find("Terminus").expect("the first frame");
    assert!(
        written.get(first..).is_some_and(|w| w.contains('2')),
        "{written:?}"
    );
}

#[test]
fn the_loop_leaves_when_the_terminal_hangs_up() {
    use vtium_config_tui::app::Size;
    use vtium_config_tui::port::drive;
    let (app, ..) = common::app(Machine::Map, MAP);
    let mut port = common::Script::new(Size::grid(100, 30), Vec::new());
    assert_eq!(drive(&mut port, app).expect("the loop runs").status, 129);
}

#[test]
fn mouse_reporting_is_on_while_the_tui_runs_and_off_after_q() {
    let pty = common::Pty::open();
    let reader = pty.reader();
    let mut tty = pty.tty();
    let (app, ..) = common::app(Machine::Map, MAP);
    pty.type_keys(b"q");
    let left = vtium_config_tui::port::drive(&mut tty, app).expect("the loop runs");
    drop(tty);
    drop(pty);
    let bytes = reader.join().expect("the reader");
    assert_eq!(left.status, 0);
    let on = common::at(&bytes, vtium_config_tui::port::MOUSE_ON).expect("reporting on");
    let off = common::last_at(&bytes, vtium_config_tui::port::MOUSE_OFF).expect("reporting off");
    assert!(on < off, "on at start, off on leaving");
    assert!(
        bytes.ends_with(vtium_config_tui::port::LEAVE),
        "reporting stays off"
    );
}

#[test]
fn mouse_reporting_is_off_while_the_tui_steps_aside() {
    use vtium_config_tui::port::{MOUSE_OFF, MOUSE_ON, Port};
    let pty = common::Pty::open();
    let reader = pty.reader();
    let mut tty = pty.tty();
    let mut aside = pty.writer();
    tty.step_aside(&mut || std::io::Write::write_all(&mut aside, b"ASIDE"))
        .expect("the step aside");
    drop(tty);
    drop(aside);
    drop(pty);
    let bytes = reader.join().expect("the reader");
    let aside_at = common::at(&bytes, b"ASIDE").expect("the step aside wrote");
    let before = bytes.get(..aside_at).expect("before");
    let after = bytes.get(aside_at..).expect("after");
    let last_on = common::last_at(before, MOUSE_ON).expect("on at start");
    let last_off = common::last_at(before, MOUSE_OFF).expect("off for the step aside");
    assert!(
        last_on < last_off,
        "reporting is off while the TUI steps aside"
    );
    let back_on = common::at(after, MOUSE_ON).expect("on again after");
    let back_off = common::last_at(after, MOUSE_OFF).expect("off on leaving");
    assert!(back_on < back_off);
}
