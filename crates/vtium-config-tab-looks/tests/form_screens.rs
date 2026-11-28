//! The screens of the Looks tab's forms, each drawn at 100×30 and 80×25
//! on the screen map's machine and on the large machine: a new look's
//! card with its name being typed, the new font set's form, the text
//! role's list box, the name being typed in the form, and the list on a
//! font set marked for removal. Every screen is checked for rows of the
//! terminal's width, for what it must still show at 80×25, and for the
//! same text without color, where the highlight stays in reverse video
//! and each mark is a word.
//!
//! Each screen's text is kept under `tests/snapshots/`, with a grid of
//! the attributes that show where no color does, and
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tab-looks --test form_screens`
//! writes the snapshots again.
//!
//! References:
//! - The catalog's screens "Looks: n makes a look and types its name", "Looks: the form for a new font set", "Looks: a role's list box in the new font set's form", "Looks: typing in the new font set's form" and "Looks: r marks a font set for removal".
//! - ADR-1206 D10: 100×30, 80×25, `NO_COLOR` and a monochrome console, on the screen map's machine and a large one.

mod common;

use common::{
    SIZES, act, apply, check_rows, check_without_color, go_to, machines, rows_of, screen, snapshot,
    snapshot_text, type_in,
};
use vtium_config_core::ui::Shared;
use vtium_config_tab_looks::Looks;
use vtium_config_tab_looks::list::Item;

/// Draws `name`'s screen four times, sets it up with `set`, checks each,
/// and compares the four with the snapshot `name`. `must` lists what the
/// screen still shows at 80×25.
fn check_screen(name: &str, must: &[&str], set: impl Fn(&mut Looks, &mut Shared)) {
    let mut text = String::new();
    for (machine, build) in machines() {
        for (w, h) in SIZES {
            let mut shared = build();
            let mut tab = Looks::new(true);
            screen(&tab, &shared, w, h, true);
            set(&mut tab, &mut shared);
            let with = screen(&tab, &shared, w, h, true);
            check_rows(&with, w, h);
            let mut mono_shared = build();
            let mut mono = Looks::new(false);
            screen(&mono, &mono_shared, w, h, false);
            set(&mut mono, &mut mono_shared);
            let without = screen(&mono, &mono_shared, w, h, false);
            check_without_color(&with, &without, true);
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

/// Opens the new font set's form from the first font set.
fn open_form(tab: &mut Looks, shared: &mut Shared) {
    go_to(tab, shared, |i| matches!(i, Item::FontSet(_)));
    act(tab, shared, "n.view.looks");
}

#[test]
fn n_makes_a_look_and_types_its_name() {
    check_screen(
        "looks-new-look",
        &["new-look", "Name", "Font set", "[Enter] Keep"],
        |tab, shared| {
            go_to(tab, shared, |i| matches!(i, Item::Look(_)));
            let made = act(tab, shared, "n.view.looks");
            apply(shared, &made);
        },
    );
}

#[test]
fn the_form_for_a_new_font_set() {
    check_screen(
        "looks-new-frit",
        &[
            "New font set",
            "Text font",
            "Box drawing",
            "Fallback",
            "Add this font set",
            "row order",
            "[Enter]",
        ],
        open_form,
    );
}

#[test]
fn a_role_s_list_box_in_the_new_font_set_s_form() {
    check_screen(
        "looks-new-frit-list",
        &["New font set", "Text font", "┌", "[Enter] Pick"],
        |tab, shared| {
            open_form(tab, shared);
            screen(tab, shared, 100, 30, true);
            act(tab, shared, "list.system-fonts");
        },
    );
}

#[test]
fn typing_in_the_new_font_set_s_form() {
    check_screen(
        "looks-new-frit-typing",
        &["mono-work", "Add this font set", "[Enter] Keep"],
        |tab, shared| {
            open_form(tab, shared);
            act(tab, shared, "end.layer.new-frit");
            act(tab, shared, "up.layer.new-frit");
            act(tab, shared, "enter.layer.new-frit");
            type_in(tab, shared, "char.typing", "mono-work");
        },
    );
}

#[test]
fn r_marks_a_font_set_for_removal() {
    check_screen(
        "looks-font-set-removed",
        &["removed at the next save"],
        |tab, shared| {
            go_to(
                tab,
                shared,
                |i| matches!(i, Item::FontSet(n) if n != "proof"),
            );
            let made = act(tab, shared, "r.view.looks");
            apply(shared, &made);
        },
    );
}
