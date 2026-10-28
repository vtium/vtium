//! The colors a program sets, drawn: the selection's colors (OSC 17 and
//! 19) in place of its inversion, and a configured decoration color that
//! names a palette index following the palette a program changes (OSC 4),
//! as the C reference resolved it on every frame. Every expected color is
//! a literal.

mod common;

use std::collections::BTreeSet;
use std::ops::Range;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DecoColor, DecorationColors, DrawState, Renderer};
use vtium_term::{Age, Attrs, Rgb, Special, UnderlineStyle};

fn state(s: &Screen) -> DrawState {
    DrawState {
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// The colors of the cell at `col`, the most used first.
fn colors(c: &Canvas, col: u32) -> Vec<u32> {
    let px: Vec<u32> = (0..16)
        .flat_map(|y| (col * 8..col * 8 + 8).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect();
    let set: BTreeSet<u32> = px.iter().copied().collect();
    let mut by_use: Vec<u32> = set.into_iter().collect();
    by_use.sort_by_key(|&p| core::cmp::Reverse(px.iter().filter(|&&q| q == p).count()));
    by_use
}

// ctlseqs, OSC 17 and 19: the selected cells draw on the selection's
// background in its foreground; the cells around them keep theirs.
// covers: color.selection
#[test]
fn the_selection_draws_in_its_colors() {
    let plain = style(rgb("ff8040"), rgb("21a0c3"));
    let mut s = Screen::new(4, 1, rgb("c0c0c0"), rgb("101010"));
    s.text(0, 0, "xxx", plain);
    s.palette
        .set_special(Special::SelectionBg, Some(rgb("0000ff")));
    s.palette
        .set_special(Special::SelectionFg, Some(rgb("00ff00")));
    let mut r = Renderer::new(unifont(), 32, 16, Config::default());
    let mut c = Canvas::new(32, 16, 0);
    let frame = s
        .frame(Age::NEVER)
        .with_selection(vec![Range { start: 1, end: 2 }]);
    r.draw(&frame, &mut c, 0, &state(&s));
    assert_eq!(colors(&c, 0), [0x21_a0_c3, 0xff_80_40]);
    assert_eq!(colors(&c, 1), [0x00_00_ff, 0x00_ff_00]);
    assert_eq!(colors(&c, 2), [0x21_a0_c3, 0xff_80_40]);
}

// The C reference resolved the configured decoration colors through the
// palette on every frame (`kmscon_conf_resolve_deco`): an underline color
// configured as index 196 follows a change to 196.
#[test]
fn a_palette_decoration_color_follows_the_palette() {
    let mut underlined = style(rgb("ff8040"), rgb("21a0c3"));
    underlined.attrs.set_underline(UnderlineStyle::Single);
    underlined.attrs.insert(Attrs::STRIKE);
    let mut s = Screen::new(2, 1, rgb("c0c0c0"), rgb("101010"));
    s.text(0, 0, " ", underlined);
    let config = Config {
        decorations: DecorationColors {
            underline: DecoColor::Indexed(196),
            strike: DecoColor::Indexed(7),
            force: false,
        },
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), 16, 16, config);
    let mut c = Canvas::new(16, 16, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(&s));
    // index 196 is (196, 76, 92) and 7 is (7, 21, 49) in the test palette
    let first: BTreeSet<u32> = colors(&c, 0).into_iter().collect();
    assert_eq!(
        first,
        BTreeSet::from([0x21_a0_c3, 0xc4_4c_5c, 0x07_15_31]),
        "{first:x?}"
    );
    s.palette.set_color(196, Rgb::new(0, 0xff, 0));
    s.damage.mark_all();
    r.draw(&s.frame(Age::NEVER), &mut c, 1, &state(&s));
    let second: BTreeSet<u32> = colors(&c, 0).into_iter().collect();
    assert_eq!(
        second,
        BTreeSet::from([0x21_a0_c3, 0x00_ff_00, 0x07_15_31]),
        "{second:x?}"
    );
}
