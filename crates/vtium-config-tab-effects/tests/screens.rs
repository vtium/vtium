//! The Effects tab's screens, each drawn at 100×30 and 80×25 on the
//! screen map's machine and on the large machine: the list on its filter
//! line, a vtlook's chain beside the list, a color map's facts, a shader's
//! facts, the list of groups in the list box, a vtlook's detail, a
//! vtlook that does not read, a color map's detail, a shader's detail, and
//! the search. Every screen is checked for rows of the terminal's width,
//! for what it must still show at 80×25, and for the same text without
//! color, where the highlight stays in reverse video and each mark is a
//! word.
//!
//! Each screen's text is kept under `tests/snapshots/`, with a grid of
//! the attributes that show where no color does. A test fails when a
//! drawn screen differs from its snapshot, and
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-effects --test screens`
//! writes the snapshots again.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Effects bullet, and the
//!   catalog's Effects screens.
//! - ADR-1206 D10: 100×30, 80×25, `NO_COLOR` and a monochrome console,
//!   on the screen map's machine and a large one.

mod common;

use common::{
    SIZES, Searching, act, check_rows, check_without_color, machines, repeat, rows_of, screen,
    screen_searching, snapshot, snapshot_text,
};
use vtium_config_core::ui::Shared;
use vtium_config_tab_effects::Effects;
use vtium_config_widgets::tab::Tab;

/// Highlights the first item whose name is `name`, with Down from the
/// filter line, drawing first so the moves act on a drawn screen.
fn highlight(tab: &mut Effects, shared: &Shared, name: &str, w: u16, h: u16) {
    screen(tab, shared, w, h, true);
    let at = tab
        .find_items(shared)
        .iter()
        .position(|i| i.name == name)
        .unwrap_or_else(|| panic!("no item {name}"));
    repeat(tab, shared, "down.view", at + 1);
}

/// Draws the screen `set` leaves on each machine at each size, checks it,
/// and compares the screens without color with the snapshot `name`.
/// `must` gives, for each machine, what the screen shows at every size.
fn check_screen(
    name: &str,
    must: &[(&str, &[&str])],
    set: impl Fn(&mut Effects, &mut Shared, &str, u16, u16),
) {
    check_search_screen(name, must, |tab, shared, machine, w, h, _| {
        set(tab, shared, machine, w, h);
    });
}

/// Checks the screen `set` leaves as [`check_screen`] does, with the
/// search row of the shell's search that `set` opens.
fn check_search_screen(
    name: &str,
    must: &[(&str, &[&str])],
    set: impl Fn(&mut Effects, &mut Shared, &str, u16, u16, &mut Searching),
) {
    let mut text = String::new();
    for (machine, build) in machines() {
        for (w, h) in SIZES {
            let mut shared = build();
            let mut tab = Effects::new(true);
            let mut shell = Searching::default();
            set(&mut tab, &mut shared, machine, w, h, &mut shell);
            let with = screen_searching(&tab, &shared, w, h, true, shell.row(&tab));
            check_rows(&with, w, h);
            let mut mono = Effects::new(false);
            let mut mono_shared = build();
            let mut mono_shell = Searching::default();
            set(&mut mono, &mut mono_shared, machine, w, h, &mut mono_shell);
            let without = screen_searching(&mono, &mono_shared, w, h, false, mono_shell.row(&mono));
            check_without_color(&with, &without);
            let all = rows_of(&without).join("\n");
            for (m, strings) in must {
                if *m != machine && *m != "*" {
                    continue;
                }
                for s in *strings {
                    assert!(
                        all.contains(s),
                        "{name} on {machine} at {w}×{h} lacks {s:?}\n{all}"
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
fn the_list_opens_on_its_filter_line_with_the_three_groups() {
    check_screen(
        "effects-list",
        &[
            (
                "*",
                &["Show: All ▾", "vtlooks", "No filter", "built in", "[Enter]"],
            ),
            (
                "map",
                &[
                    "Color maps",
                    "sepia.cube",
                    "● active",
                    "○ not active",
                    "error",
                    "Shaders  none installed",
                    "the sample screen, with no filter",
                ],
            ),
            ("large", &["of 253"]),
        ],
        |tab, shared, _, w, h| {
            screen(tab, shared, w, h, true);
        },
    );
}

#[test]
fn a_vtlook_s_chain_reads_left_to_right_beside_the_list() {
    check_screen(
        "effects-vtlook",
        &[
            ("*", &["tint", "used by"]),
            (
                "map",
                &[
                    "frame → spatial: none → turn",
                    "amber  preset",
                    "color 255,176,0 · amount 1",
                    "amber, on the sample screen",
                ],
            ),
            (
                "large",
                &[
                    "look-004",
                    "scanlines (planned)",
                    "planned, ADR-1304 slice 4",
                ],
            ),
        ],
        |tab, shared, machine, w, h| {
            let name = if machine == "map" {
                "amber"
            } else {
                "look-004"
            };
            highlight(tab, shared, name, w, h);
        },
    );
}

#[test]
fn a_color_map_s_facts_sit_beside_the_list() {
    check_screen(
        "effects-map",
        &[
            ("*", &["Color map", "file", "state", "used by"]),
            (
                "map",
                &[
                    "Color map sepia.cube",
                    "● active · trusted",
                    "sepia.cube, on the sample screen",
                ],
            ),
            ("large", &["Color map map-00.cube"]),
        ],
        |tab, shared, machine, w, h| {
            let name = if machine == "map" {
                "sepia.cube"
            } else {
                "map-00.cube"
            };
            highlight(tab, shared, name, w, h);
        },
    );
}

#[test]
fn the_filter_line_opens_the_groups_in_the_list_box() {
    check_screen(
        "effects-groups",
        &[(
            "*",
            &[
                "All effects",
                "vtlooks (look files)",
                "Color maps",
                "Shaders",
            ],
        )],
        |tab, shared, _, w, h| {
            screen(tab, shared, w, h, true);
            act(tab, shared, "enter.view.effects");
            assert_eq!(tab.node(), "effects.filter");
        },
    );
}

#[test]
fn a_vtlook_s_detail_lists_its_layers_and_what_is_planned() {
    check_screen(
        "effects-detail-vtlook",
        &[
            (
                "*",
                &[
                    "vtlook",
                    "File",
                    "Color layer, in order",
                    "Requires",
                    "planned, ADR-1304 slice 7",
                ],
            ),
            (
                "map",
                &[
                    "vtlook amber",
                    "/usr/share/vtium/looks/amber.vtlook",
                    "without errors",
                    "Spatial layer   none",
                    "To change a preset",
                ],
            ),
            (
                "large",
                &[
                    "vtlook look-004",
                    "Spatial layer, in order",
                    "scanlines",
                    "planned, ADR-1304 slice 4",
                ],
            ),
        ],
        |tab, shared, machine, w, h| {
            let name = if machine == "map" {
                "amber"
            } else {
                "look-004"
            };
            highlight(tab, shared, name, w, h);
            act(tab, shared, "enter.view.effects");
            assert_eq!(tab.node(), "effects.effect");
        },
    );
}

#[test]
fn a_vtlook_that_does_not_read_gives_its_error_whole() {
    check_screen(
        "effects-detail-error",
        &[
            (
                "*",
                &[
                    "does not read: 1 error",
                    "draws the built-in vtlook instead",
                ],
            ),
            (
                "map",
                &[
                    "vtlook green",
                    "administrator's",
                    "hides the preset of this name",
                    "needs gamma,",
                    "color-blind or cube",
                ],
            ),
            ("large", &["vtlook look-014", "this build of vtium has no"]),
        ],
        |tab, shared, machine, w, h| {
            screen(tab, shared, w, h, true);
            let items = tab.find_items(shared);
            let at = if machine == "map" {
                items
                    .iter()
                    .position(|i| i.name == "green" && i.fact.contains("error"))
            } else {
                items.iter().position(|i| i.name == "look-014")
            };
            repeat(tab, shared, "down.view", at.unwrap_or(0) + 1);
            act(tab, shared, "enter.view.effects");
        },
    );
}

#[test]
fn a_color_map_s_detail_gives_its_state_and_its_trust() {
    check_screen(
        "effects-detail-map",
        &[
            ("*", &["Color map", "File", "State"]),
            (
                "map",
                &[
                    "Color map loose.cube",
                    "fails trust",
                    "belongs to user 1000",
                ],
            ),
            (
                "large",
                &["Color map map-01.cube", "○ not active · trusted"],
            ),
        ],
        |tab, shared, machine, w, h| {
            let name = if machine == "map" {
                "loose.cube"
            } else {
                "map-01.cube"
            };
            highlight(tab, shared, name, w, h);
            act(tab, shared, "enter.view.effects");
        },
    );
}

#[test]
fn a_shader_waits_for_the_spatial_layer_as_planned() {
    check_screen(
        "effects-shader",
        &[
            ("map", &["Shaders  none installed"]),
            (
                "large",
                &[
                    "Shader shader-03.glsl",
                    "planned (slice 4)",
                    "planned, ADR-1304 slice 11: Ghostty's",
                    "planned, ADR-1304 slice 4",
                ],
            ),
        ],
        |tab, shared, machine, w, h| {
            if machine == "map" {
                screen(tab, shared, w, h, true);
                act(tab, shared, "end.view");
            } else {
                highlight(tab, shared, "shader-03.glsl", w, h);
            }
        },
    );
}

#[test]
fn the_search_keeps_the_items_whose_names_match() {
    check_search_screen(
        "effects-search",
        &[
            ("*", &["matching"]),
            ("map", &["matching *green", "green"]),
            ("large", &["matching ^admin", "admin-011"]),
        ],
        |tab, shared, machine, w, h, shell| {
            screen(tab, shared, w, h, true);
            shell.open(tab, shared);
            let typed = if machine == "map" {
                "*green"
            } else {
                "^admin-01"
            };
            shell.type_in(tab, shared, typed);
        },
    );
}
