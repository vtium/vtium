//! SGR, driven through the byte stream: each attribute set and cleared,
//! the palette and extended colors in their semicolon and colon forms,
//! the underline styles and color, and the forms vtium ignores.
//!
//! Each expected pen is the C reference's, or the specification's where
//! vtium departs from it (the crate's README lists each departure).

mod common;

use common::term;
use vtium_term::{Attrs, Color, Rgb, Style, Terminal, UnderlineStyle};

fn pen(t: &Terminal) -> Style {
    *t.screen().pen()
}

fn pen_after(bytes: &[u8]) -> Style {
    pen(&term(8, 2, bytes))
}

/// Sets and clears one attribute.
fn toggles(on: &[u8], off: &[u8], attr: Attrs) {
    let mut t = term(8, 2, on);
    assert!(pen(&t).attrs.contains(attr), "{on:?} sets it");
    t.input(off);
    assert!(!pen(&t).attrs.contains(attr), "{off:?} clears it");
    t.input(on);
    t.input(b"\x1b[m");
    assert_eq!(pen(&t), Style::DEFAULT, "SGR 0 clears it");
}

// ECMA-48 8.3.117: SGR 1 bold, 22 normal intensity, 0 everything.
// covers: sgr.bold
#[test]
fn bold_is_set_and_cleared() {
    toggles(b"\x1b[1m", b"\x1b[22m", Attrs::BOLD);
    let p = pen_after(b"\x1b[1;2m\x1b[22m");
    assert!(!p.attrs.contains(Attrs::BOLD) && !p.attrs.contains(Attrs::DIM));
}

// covers: sgr.dim
#[test]
fn dim_is_set_and_cleared() {
    toggles(b"\x1b[2m", b"\x1b[22m", Attrs::DIM);
}

// covers: sgr.italic
#[test]
fn italic_is_set_and_cleared() {
    toggles(b"\x1b[3m", b"\x1b[23m", Attrs::ITALIC);
}

// covers: sgr.blink
#[test]
fn blink_is_set_and_cleared() {
    toggles(b"\x1b[5m", b"\x1b[25m", Attrs::BLINK);
}

// covers: sgr.inverse
#[test]
fn inverse_is_set_and_cleared() {
    toggles(b"\x1b[7m", b"\x1b[27m", Attrs::INVERSE);
}

// covers: sgr.strike
#[test]
fn strike_is_set_and_cleared() {
    toggles(b"\x1b[9m", b"\x1b[29m", Attrs::STRIKE);
}

// covers: sgr.underline
#[test]
fn underline_is_set_and_cleared() {
    let mut t = term(8, 2, b"\x1b[4m");
    assert_eq!(pen(&t).attrs.underline(), UnderlineStyle::Single);
    t.input(b"\x1b[24m");
    assert_eq!(pen(&t).attrs.underline(), UnderlineStyle::None);
}

// kitty underlines: SGR 4:0 to 4:5, an empty style none, a style past 5
// single, as Ghostty draws it; more than one sub-parameter is not the form.
// covers: sgr.underline-style
#[test]
fn underline_styles_take_a_sub_parameter() {
    for (seq, style) in [
        (&b"\x1b[4:0m"[..], UnderlineStyle::None),
        (b"\x1b[4:1m", UnderlineStyle::Single),
        (b"\x1b[4:2m", UnderlineStyle::Double),
        (b"\x1b[4:3m", UnderlineStyle::Curly),
        (b"\x1b[4:4m", UnderlineStyle::Dotted),
        (b"\x1b[4:5m", UnderlineStyle::Dashed),
        (b"\x1b[4:9m", UnderlineStyle::Single),
        (b"\x1b[4:m", UnderlineStyle::None),
    ] {
        assert_eq!(pen_after(seq).attrs.underline(), style, "{seq:?}");
    }
    assert_eq!(
        pen_after(b"\x1b[4:3m\x1b[4:1:2m").attrs.underline(),
        UnderlineStyle::Curly
    );
}

// ECMA-48 8.3.117: SGR 21 doubly underlined.
// covers: sgr.double-underline
#[test]
fn sgr_21_double_underlines() {
    assert_eq!(
        pen_after(b"\x1b[21m").attrs.underline(),
        UnderlineStyle::Double
    );
    assert_eq!(
        pen_after(b"\x1b[21;24m").attrs.underline(),
        UnderlineStyle::None
    );
}

// kitty underlines: SGR 58 sets the underline color in every form 38
// takes, and 59 returns it to the foreground's.
// covers: sgr.underline-color
#[test]
fn sgr_58_and_59_set_the_underline_color() {
    let rgb = |r, g, b| Color::Rgb(Rgb::new(r, g, b));
    assert_eq!(
        pen_after(b"\x1b[58;5;196m").underline_color,
        Color::Indexed(196)
    );
    assert_eq!(
        pen_after(b"\x1b[58;2;10;20;30m").underline_color,
        rgb(10, 20, 30)
    );
    assert_eq!(
        pen_after(b"\x1b[58:2::40:50:60m").underline_color,
        rgb(40, 50, 60)
    );
    assert_eq!(
        pen_after(b"\x1b[58:5:9m").underline_color,
        Color::Indexed(9)
    );
    assert_eq!(
        pen_after(b"\x1b[58;5;1;59m").underline_color,
        Color::Default
    );
}

// ctlseqs: 30-37 and 40-47 the eight colors, 90-97 and 100-107 the
// bright eight, 39 and 49 the defaults.
// covers: sgr.palette
#[test]
fn palette_colors_are_set_and_returned_to_the_default() {
    let p = pen_after(b"\x1b[31;42m");
    assert_eq!((p.fg, p.bg), (Color::Indexed(1), Color::Indexed(2)));
    let p = pen_after(b"\x1b[97;100m");
    assert_eq!((p.fg, p.bg), (Color::Indexed(15), Color::Indexed(8)));
    let p = pen_after(b"\x1b[37;47;39;49m");
    assert_eq!((p.fg, p.bg), (Color::Default, Color::Default));
}

// ctlseqs; ITU T.416: 38 and 48 with 5 and an index, or 2 and RGB, in the
// semicolon form and the colon forms with and without a color space. A
// semicolon component keeps its low byte and an index past 255 is black,
// as the C reference's goldens record; a colon form past 255 is refused.
// covers: sgr.extended-color
#[test]
fn extended_colors_take_both_forms() {
    let rgb = |r, g, b| Color::Rgb(Rgb::new(r, g, b));
    assert_eq!(pen_after(b"\x1b[38;5;200m").fg, Color::Indexed(200));
    assert_eq!(pen_after(b"\x1b[48;2;1;2;3m").bg, rgb(1, 2, 3));
    assert_eq!(pen_after(b"\x1b[38:5:17m").fg, Color::Indexed(17));
    assert_eq!(pen_after(b"\x1b[38:2::1:2:3m").fg, rgb(1, 2, 3));
    assert_eq!(pen_after(b"\x1b[38:2:7:1:2:3m").fg, rgb(1, 2, 3));
    assert_eq!(pen_after(b"\x1b[38:2:4:5:6m").fg, rgb(4, 5, 6));
    assert_eq!(pen_after(b"\x1b[38;2;257;2;3m").fg, rgb(1, 2, 3));
    assert_eq!(pen_after(b"\x1b[38;5;300m").fg, rgb(0, 0, 0));
    assert_eq!(pen_after(b"\x1b[38:5:300m").fg, Color::Default);
    assert_eq!(pen_after(b"\x1b[38:2::1:2:300m").fg, Color::Default);
    // A semicolon form carries on after its values.
    let p = pen_after(b"\x1b[38;5;3;1;48;2;4;5;6;3m");
    assert_eq!((p.fg, p.bg), (Color::Indexed(3), rgb(4, 5, 6)));
    assert!(p.attrs.contains(Attrs::BOLD) && p.attrs.contains(Attrs::ITALIC));
    // A color that is no form leaves the pen; the values after it are
    // read as attributes, as in the C reference.
    let p = pen_after(b"\x1b[38;5m");
    assert_eq!(p.fg, Color::Default);
    assert!(p.attrs.contains(Attrs::BLINK));
}

// ECMA-48 5.4.2 e and 8.3.117: an empty SGR parameter is its default, 0,
// wherever it falls; the C reference skipped one at the end.
#[test]
fn an_empty_sgr_parameter_resets() {
    assert_eq!(pen_after(b"\x1b[1;m"), Style::DEFAULT);
    assert_eq!(pen_after(b"\x1b[1m\x1b[;3m").attrs, {
        let mut a = Attrs::empty();
        a.insert(Attrs::ITALIC);
        a
    });
}

// ADR-1000: SGR 38:6, 48:6 and 58:6 (WezTerm's RGBA), 221 and 222
// (kitty's), and ECMA-48's fonts, Fraktur, spacing, frames and ideogram
// marks are ignored: the pen stays as it was.
// covers: sgr.rgba, sgr.bold-faint-reset, sgr.ecma-other
#[test]
fn the_ignored_sgr_forms_leave_the_pen() {
    let start = b"\x1b[1;2;3;31m";
    let before = pen_after(start);
    for seq in [
        &b"\x1b[38:6::1:2:3:4m"[..],
        b"\x1b[48:6::1:2:3:4m",
        b"\x1b[58:6::1:2:3:4m",
        b"\x1b[221m",
        b"\x1b[222m",
        b"\x1b[10;11;12;13;14;15;16;17;18;19;20m",
        b"\x1b[26m",
        b"\x1b[50;51;52;54m",
        b"\x1b[60;61;62;63;64;65m",
    ] {
        let mut t = term(8, 2, start);
        t.input(seq);
        assert_eq!(pen(&t), before, "{seq:?}");
    }
}

// vtium-parse's README: a private marker or an intermediate makes another
// function, so `CSI > 4 m` (XTMODKEYS) and `CSI 0 % m` are not SGR.
#[test]
fn a_marker_or_an_intermediate_is_not_sgr() {
    let start = b"\x1b[1m";
    for seq in [&b"\x1b[>4;1m"[..], b"\x1b[?4m", b"\x1b[0%m"] {
        let mut t = term(8, 2, start);
        t.input(seq);
        assert!(pen(&t).attrs.contains(Attrs::BOLD), "{seq:?}");
    }
}

// SGR 0 leaves DECSCA's protection, which is not SGR's.
#[test]
fn sgr_0_keeps_the_protection() {
    let mut t = term(8, 2, b"");
    let mut protected = Style::DEFAULT;
    protected.attrs.insert(Attrs::PROTECTED);
    protected.attrs.insert(Attrs::BOLD);
    t.screen_mut().set_pen(protected);
    t.input(b"\x1b[0m");
    assert!(pen(&t).attrs.contains(Attrs::PROTECTED));
    assert!(!pen(&t).attrs.contains(Attrs::BOLD));
}
