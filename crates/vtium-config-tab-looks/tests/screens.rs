//! The Looks tab's screens, each drawn at 100×30 and 80×25 on the screen
//! map's machine and on the large machine: the list on a look, with the
//! console's own preview, filtered by the search, on a font set, a color
//! scheme and a pointer theme; a look's card, each of its lines' list
//! boxes, and what the look still misses; and the three details. Every
//! screen is checked for rows of the terminal's width, for what it must
//! still show at 80×25, and for the same text without color, where the
//! highlight stays in reverse video and each mark is a word.
//!
//! Each screen's text is kept under `tests/snapshots/`, with a grid of
//! the attributes that show where no color does. A test fails when a
//! drawn screen differs from its snapshot, and
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-looks --test screens`
//! writes the snapshots again.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Looks bullet, and the catalog's Looks screens.
//! - ADR-1206 D10: 100×30, 80×25, `NO_COLOR` and a monochrome console, on the screen map's machine and a large one.

mod common;

use common::{
    SIZES, act, check_rows, check_without_color, go_to, machines, rows_of, screen, snapshot,
    snapshot_text, type_in,
};
use vtium_config_core::ui::{Live, Shared};
use vtium_config_tab_looks::Looks;
use vtium_config_tab_looks::list::{Item, Scheme};

/// Draws `name`'s screen four times, sets it up with `set`, checks each,
/// and compares the four with the snapshot `name`. `must` lists what the
/// screen still shows at 80×25, and `lit` whether it has a highlight.
fn check_screen(
    name: &str,
    must: &[&str],
    lit: bool,
    set: impl Fn(&mut Looks, &mut Shared, u16, u16),
) {
    let mut text = String::new();
    for (machine, build) in machines() {
        for (w, h) in SIZES {
            let mut shared = build();
            let mut tab = Looks::new(true);
            screen(&tab, &shared, w, h, true);
            set(&mut tab, &mut shared, w, h);
            let with = screen(&tab, &shared, w, h, true);
            check_rows(&with, w, h);
            let mut mono_shared = build();
            let mut mono = Looks::new(false);
            screen(&mono, &mono_shared, w, h, false);
            set(&mut mono, &mut mono_shared, w, h);
            let without = screen(&mono, &mono_shared, w, h, false);
            check_without_color(&with, &without, lit);
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

/// The first look of the machine.
fn a_look(i: &Item) -> bool {
    matches!(i, Item::Look(_))
}

/// Opens the first look's card and highlights its line `line`.
fn card_on(tab: &mut Looks, shared: &Shared, line: usize) {
    go_to(tab, shared, a_look);
    act(tab, shared, "enter.view.looks");
    for _ in 0..line {
        act(tab, shared, "down.layer.look");
    }
}

#[test]
fn the_list_shows_the_look_s_card_beside_it() {
    check_screen(
        "looks-list",
        &[
            "Looks",
            "Font sets",
            "Font set",
            "Size",
            "Fonts",
            "as the console draws it",
        ],
        true,
        |tab, shared, _, _| {
            go_to(tab, shared, a_look);
        },
    );
}

#[test]
fn in_a_vtium_console_the_preview_is_the_console_s_own_text() {
    check_screen(
        "looks-list-console",
        &["as the console draws it", "Bold text"],
        true,
        |tab, shared, _, _| {
            shared.console.live = Live::On;
            go_to(tab, shared, a_look);
        },
    );
}

#[test]
fn the_search_filters_the_list_by_name() {
    check_screen(
        "looks-search",
        &["Color schemes", "solarized"],
        true,
        |tab, shared, _, _| {
            act(tab, shared, "question.view");
            type_in(tab, shared, "char.typing", "^sol");
        },
    );
}

#[test]
fn a_font_set_s_facts_sit_beside_the_list() {
    check_screen(
        "looks-font-set",
        &["Font set", "Fonts by role"],
        true,
        |tab, shared, _, _| {
            go_to(tab, shared, |i| matches!(i, Item::FontSet(_)));
        },
    );
}

#[test]
fn a_color_scheme_s_facts_and_colors_sit_beside_the_list() {
    check_screen(
        "looks-schemes",
        &["Color scheme", "always active", "#"],
        true,
        |tab, shared, _, _| {
            go_to(tab, shared, |i| {
                matches!(i, Item::Scheme(Scheme::BuiltIn("vga")))
            });
        },
    );
}

#[test]
fn a_pointer_theme_s_facts_sit_beside_the_list() {
    check_screen(
        "looks-pointers",
        &["Pointer theme", "State"],
        true,
        |tab, shared, _, _| {
            go_to(tab, shared, |i| matches!(i, Item::Pointer(_)));
        },
    );
}

#[test]
fn a_look_s_card_shows_its_lines_and_its_fonts_by_role() {
    check_screen(
        "looks-card",
        &["Name", "Font set", "Pointer theme", "Fonts by role"],
        true,
        |tab, shared, _, _| card_on(tab, shared, 2),
    );
}

#[test]
fn each_line_of_the_card_opens_its_list_box() {
    for (name, line, row, must) in [
        ("looks-card-font-set", 1, "list.font-sets", "proof"),
        ("looks-card-size", 2, "list.font-sizes", "automatic"),
        ("looks-card-scheme", 3, "list.palettes", "vga"),
        ("looks-card-vtlook", 4, "list.look-effects", "No filter"),
        (
            "looks-card-pointer",
            5,
            "list.pointer-themes",
            "vtium's own pictures",
        ),
    ] {
        check_screen(name, &[must, "┌"], true, |tab, shared, _, _| {
            card_on(tab, shared, line);
            act(tab, shared, row);
        });
    }
}

#[test]
fn what_a_look_still_misses_opens_behind_enter() {
    check_screen(
        "looks-missing",
        &["What it still misses"],
        false,
        |tab, shared, _, _| {
            card_on(tab, shared, 6);
            act(tab, shared, "enter.layer.look");
        },
    );
}

#[test]
fn the_details_of_a_font_set_a_scheme_and_a_pointer_theme() {
    for (name, must, pick) in [
        (
            "looks-font-set-detail",
            "Fonts by role",
            (|i: &Item| matches!(i, Item::FontSet(_))) as fn(&Item) -> bool,
        ),
        ("looks-scheme-detail", "Its 16 colors", |i: &Item| {
            matches!(i, Item::Scheme(Scheme::File(_)))
        }),
        ("looks-pointer-detail", "Used by", |i: &Item| {
            matches!(i, Item::Pointer(_))
        }),
    ] {
        check_screen(name, &[must], false, |tab, shared, _, _| {
            go_to(tab, shared, pick);
            act(tab, shared, "enter.view.looks");
        });
    }
}
