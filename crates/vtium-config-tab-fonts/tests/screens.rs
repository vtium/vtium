//! The Fonts tab's screens, each drawn at 100×30 and 80×25 on the screen
//! map's machine and on the large machine: the list with a font's facts
//! beside it, every one of the five states, the configuration's own row,
//! the list of filters, the list through a filter, the search and a
//! search that matches nothing, a font's detail, a font of your own, and
//! a font no file is installed for. Every screen is checked for rows of
//! the terminal's width, for what it must still show at 80×25, and for
//! the same text without color, where the highlight stays in reverse
//! video and each mark is a word.
//!
//! Each screen's text is kept under `tests/snapshots/`, with a grid of
//! the attributes that show where no color does. A test fails when a
//! drawn screen differs from its snapshot, and
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-fonts --test screens`
//! writes the snapshots again.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Fonts bullet, and the
//!   catalog's Fonts screens.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   the marks, which read the same without color.
//! - ADR-1206 D10: 100×30, 80×25, `NO_COLOR` and a monochrome console,
//!   on the screen map's machine and a large one.

mod common;

use common::{
    SIZES, act, check_rows, check_without_color, every_state, machines, open_search, rows_of,
    screen, search_type, snapshot, snapshot_text,
};
use vtium_config_core::ui::{Action, Shared};
use vtium_config_tab_fonts::Fonts;
use vtium_config_widgets::tab::Tab;

/// Highlights the font whose file or family is `name`, as Find's Enter
/// does, after a first draw at `w`×`h`.
fn go_to(tab: &mut Fonts, shared: &Shared, name: &str, w: u16, h: u16) {
    screen(tab, shared, w, h, true);
    let at = tab
        .find_items(shared)
        .into_iter()
        .find(|i| i.name == name || i.description.as_deref() == Some(name))
        .map(|i| i.index);
    let Some(at) = at else {
        panic!("no font {name}");
    };
    assert!(
        tab.update(shared, Action::item("enter.typing.find", at))
            .is_empty()
    );
    screen(tab, shared, w, h, true);
}

/// The name of the font that stands for `map` on the screen map's
/// machine and for `large` on the large one.
fn pick<'a>(shared: &Shared, map: &'a str, large: &'a str) -> &'a str {
    if shared.document().name() == vtium_config_core::fixture::large::NAME {
        large
    } else {
        map
    }
}

/// Draws `name`'s screen four times, sets it up with `set`, checks each,
/// and compares the four with the snapshot `name`. `must` lists what the
/// screen still shows at 80×25 on the screen map's machine, and `lit`
/// whether it highlights a row.
fn check_screen(
    name: &str,
    must: &[&str],
    lit: bool,
    set: impl Fn(&mut Fonts, &mut Shared, u16, u16),
) {
    let mut text = String::new();
    for (machine, build) in machines() {
        for (w, h) in SIZES {
            let mut shared = build();
            let mut tab = Fonts::new(true);
            set(&mut tab, &mut shared, w, h);
            let with = screen(&tab, &shared, w, h, true);
            check_rows(&with, w, h);
            let mut mono = Fonts::new(false);
            let mut mono_shared = build();
            set(&mut mono, &mut mono_shared, w, h);
            let without = screen(&mono, &mono_shared, w, h, false);
            check_without_color(&with, &without, lit);
            let all = rows_of(&with).join("\n");
            if machine == "map" {
                for m in must {
                    assert!(
                        all.contains(m),
                        "{name} on {machine} at {w}×{h} lacks {m:?}\n{all}"
                    );
                }
            }
            text.push_str(&format!("== {machine} {w}x{h} ==\n"));
            text.push_str(&snapshot_text(&without));
        }
    }
    snapshot(name, &text);
}

#[test]
fn the_list_shows_the_highlighted_font_s_facts_beside_it() {
    check_screen(
        "fonts-list",
        &[
            "Show: All fonts ▾",
            "System fonts",
            "DejaVuSansMono.ttf",
            "● active",
            "DejaVu Sans Mono Regular",
            "folder  /usr/share",
            "[Space] Deactivate",
        ],
        true,
        |tab, shared, w, h| {
            let name = pick(shared, "DejaVuSansMono.ttf", "Family0004-Regular.ttf");
            go_to(tab, shared, name, w, h);
        },
    );
}

#[test]
fn each_of_the_five_states_shows_its_mark_in_words() {
    check_screen(
        "fonts-states",
        &[
            "○ not active",
            "○ turned off",
            "not usable yet",
            "trust",
            "Activate missing fonts (1)",
        ],
        true,
        |tab, shared, w, h| {
            every_state(shared);
            let name = pick(shared, "ter-u16n.otb", "Family0005-Regular.ttf");
            go_to(tab, shared, name, w, h);
        },
    );
}

#[test]
fn the_configuration_s_row_lists_what_activate_missing_activates() {
    check_screen(
        "fonts-activate-row",
        &[
            "○ not all active",
            "Activates:",
            "Stays off on purpose:",
            "Cannot be activated:",
        ],
        true,
        |tab, shared, w, h| {
            every_state(shared);
            screen(tab, shared, w, h, true);
            act(tab, shared, "down.view");
        },
    );
}

#[test]
fn enter_on_the_filter_line_lists_the_filters_in_the_list_box() {
    check_screen(
        "fonts-filter",
        &["All fonts", "Monospaced fonts", "Fonts for Latin"],
        true,
        |tab, shared, w, h| {
            screen(tab, shared, w, h, true);
            act(tab, shared, "enter.view.fonts");
        },
    );
}

#[test]
fn a_filter_keeps_the_fonts_that_pass_it_and_counts_them() {
    check_screen(
        "fonts-filtered",
        &["Show: Monospaced fonts ▾", "Terminus"],
        true,
        |tab, shared, w, h| {
            screen(tab, shared, w, h, true);
            act(tab, shared, "enter.view.fonts");
            screen(tab, shared, w, h, true);
            for _ in 0..3 {
                act(tab, shared, "down.layer.list");
            }
            act(tab, shared, "enter.layer.list");
            screen(tab, shared, w, h, true);
            act(tab, shared, "down.view");
            act(tab, shared, "down.view");
            act(tab, shared, "down.view");
        },
    );
}

#[test]
fn the_search_keeps_the_fonts_whose_family_or_file_matches() {
    check_screen(
        "fonts-search",
        &["matching mono", "? mono", "cullet-mono.otf"],
        true,
        |tab, shared, w, h| {
            screen(tab, shared, w, h, true);
            let mut search = open_search(tab, shared);
            search_type(tab, shared, &mut search, "mono");
        },
    );
}

#[test]
fn a_search_that_matches_nothing_says_so() {
    check_screen(
        "fonts-search-empty",
        &["No font's family or file name matches $vga.", "0 of"],
        true,
        |tab, shared, w, h| {
            screen(tab, shared, w, h, true);
            let mut search = open_search(tab, shared);
            search_type(tab, shared, &mut search, "$vga");
        },
    );
}

#[test]
fn a_font_s_detail_names_the_check_it_fails() {
    check_screen(
        "fonts-detail",
        &[
            "NotoColorEmoji.ttf  Noto Color Emoji Regular, version 2.37",
            "Trust       fails:",
            "Duplicate",
            "The trust rule:",
        ],
        false,
        |tab, shared, w, h| {
            let name = pick(shared, "NotoColorEmoji.ttf", "Local-0013.ttf");
            go_to(tab, shared, name, w, h);
            act(tab, shared, "enter.view.fonts");
        },
    );
}

#[test]
fn a_font_of_your_own_says_why_vtium_cannot_use_it_yet() {
    check_screen(
        "fonts-own",
        &[
            "Mine-Regular.ttf  Mine Regular",
            "not usable yet",
            "vtium uses only system fonts",
            "your folder",
        ],
        false,
        |tab, shared, w, h| {
            let name = pick(shared, "Mine-Regular.ttf", "Mine-0002.ttf");
            go_to(tab, shared, name, w, h);
            act(tab, shared, "enter.view.fonts");
        },
    );
}

#[test]
fn a_font_no_file_is_installed_for_is_listed_as_missing() {
    check_screen(
        "fonts-missing",
        &[
            "Not installed",
            "JetBrains Mono",
            "✗ missing",
            "named   font:JetBrains Mono",
            "not installed",
        ],
        true,
        |tab, shared, w, h| {
            let name = pick(shared, "JetBrains Mono", "proof");
            go_to(tab, shared, name, w, h);
        },
    );
}

#[test]
fn without_color_each_of_the_five_states_is_its_word() {
    let mut shared = vtium_config_core::fixture::padnoir::shared();
    every_state(&mut shared);
    let tab = Fonts::new(false);
    let rows = rows_of(&screen(&tab, &shared, 100, 30, false)).join("\n");
    for word in [
        "● active",
        "○ not active",
        "○ turned off on purpose",
        "○ not active · fails trust",
        "not usable yet",
        "✗ missing",
        "● always active",
    ] {
        assert!(rows.contains(word), "no {word:?} without color\n{rows}");
    }
}
