//! The Settings tab's screens, each drawn at 100×30 and 80×25 on the
//! screen map's machine and on the large machine: the sections, a
//! section's keys, a key's values in the list box, a value being typed, a
//! changed value, and the search for `*bright`. Every screen is checked
//! for rows of the terminal's width, for what it must still show at
//! 80×25, and for the same text without color, where the highlight stays
//! in reverse video and each mark is a word or an attribute.
//!
//! Each screen's text is kept under `tests/snapshots/`, with a grid of
//! the attributes that show where no color does. A test fails when a
//! drawn screen differs from its snapshot, and
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-settings --test screens`
//! writes the snapshots again.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Settings bullet, and the
//!   catalog's Settings screens.
//! - ADR-1206 D10: 100×30, 80×25, `NO_COLOR` and a monochrome console,
//!   on the screen map's machine and a large one.

mod common;

use common::{
    SIZES, act, check_rows, check_without_color, machines, repeat, rows_of, screen, snapshot,
    snapshot_text, type_in,
};
use vtium_config_core::ui::{Change, Command, Shared};
use vtium_config_tab_settings::Settings;

/// Goes from the list of sections to the key `name`, with the keys'
/// pane in focus, drawing first so the moves act on a drawn screen.
fn go_to_key(tab: &mut Settings, shared: &Shared, name: &str, w: u16, h: u16) {
    screen(tab, shared, w, h, true);
    let table = tab.table();
    let at = table
        .entries
        .iter()
        .position(|e| e.key.name == name)
        .unwrap_or_default();
    let entry = table.entries[at];
    let first = table.sections[entry.section].keys.start;
    repeat(tab, shared, "down.view", entry.section);
    act(tab, shared, "enter.view.settings");
    repeat(tab, shared, "down.layer.settings-section", at - first);
    assert_eq!(tab.key().map(|k| k.name), Some(name));
    screen(tab, shared, w, h, true);
}

/// Draws `name`'s screen four times, sets it up with `set`, checks each,
/// and compares the four with the snapshot `name`. `must` lists what the
/// screen still shows at 80×25.
fn check_screen(name: &str, must: &[&str], set: impl Fn(&mut Settings, &mut Shared, u16, u16)) {
    let mut text = String::new();
    for (machine, build) in machines() {
        for (w, h) in SIZES {
            let mut shared = build();
            let mut tab = Settings::new(true);
            set(&mut tab, &mut shared, w, h);
            let with = screen(&tab, &shared, w, h, true);
            check_rows(&with, w, h);
            let mut mono = Settings::new(false);
            let mut mono_shared = build();
            set(&mut mono, &mut mono_shared, w, h);
            let without = screen(&mono, &mono_shared, w, h, false);
            check_without_color(&with, &without);
            let all = rows_of(&with).join("\n");
            for m in must {
                assert!(
                    all.contains(m),
                    "{name} on {machine} at {w}×{h} lacks {m:?}\n{all}"
                );
            }
            text.push_str(&format!("== {machine} {w}x{h} ==\n"));
            text.push_str(&snapshot_text(&without));
        }
    }
    snapshot(name, &text);
}

#[test]
fn the_sections_sit_at_the_left_and_the_first_section_s_keys_at_the_right() {
    check_screen(
        "settings-sections",
        &[
            "Settings",
            "Fonts and text",
            "Running vtium",
            "font-name",
            "[Enter]",
        ],
        |tab, shared, w, h| {
            screen(tab, shared, w, h, true);
        },
    );
}

#[test]
fn a_section_s_keys_take_the_focus_with_the_key_s_line_at_the_foot() {
    check_screen(
        "settings-section",
        &[
            "font-name: the console's font.",
            "Fonts and text",
            "default",
        ],
        |tab, shared, w, h| go_to_key(tab, shared, "font-name", w, h),
    );
}

#[test]
fn a_key_s_values_open_in_the_list_box_under_its_line() {
    check_screen(
        "settings-values",
        &[
            "unset (default)",
            "sprites",
            "auto",
            "glyph-source: who draws",
        ],
        |tab, shared, w, h| {
            go_to_key(tab, shared, "glyph-source", w, h);
            act(tab, shared, "enter.layer.settings-section");
        },
    );
}

#[test]
fn a_typed_value_shows_in_its_line_with_the_cursor_after_it() {
    check_screen(
        "settings-typing",
        &["proof_", "[Enter]"],
        |tab, shared, w, h| {
            go_to_key(tab, shared, "font-name", w, h);
            act(tab, shared, "enter.layer.settings-section");
            type_in(tab, shared, "char.typing", "proof");
        },
    );
}

#[test]
fn a_changed_value_is_marked_by_a_word() {
    check_screen(
        "settings-changed",
        &["auto", "changed"],
        |tab, shared, w, h| {
            go_to_key(tab, shared, "glyph-source", w, h);
            act(tab, shared, "enter.layer.settings-section");
            screen(tab, shared, w, h, true);
            repeat(tab, shared, "end.layer.list", 1);
            let made = act(tab, shared, "enter.layer.list");
            for command in made {
                if let Command::Change(Change::Document(change)) = command {
                    assert!(shared.change(change).is_ok());
                }
            }
            act(tab, shared, "up.layer.settings-section");
        },
    );
}

#[test]
fn the_search_for_bright_keeps_display_depth_by_its_description() {
    check_screen(
        "settings-search",
        &[
            "display-depth",
            "matching *bright",
            "brightness",
            "? *bright",
        ],
        |tab, shared, w, h| {
            screen(tab, shared, w, h, true);
            act(tab, shared, "question.view");
            type_in(tab, shared, "char.typing.search", "*bright");
        },
    );
}
