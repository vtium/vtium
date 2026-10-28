//! The SGR and color stacks, driven through the byte stream: XTPUSHSGR
//! and XTPOPSGR, XTPUSHCOLORS, XTPOPCOLORS and XTREPORTCOLORS (ctlseqs),
//! and kitty's OSC 30001 and 30101 (`color-stack`), each stack at its
//! bound, empty, and across DECSTR and RIS.
//!
//! The C reference ignored every one of these sequences; the crate's
//! README lists what vtium does and where it departs from xterm.

mod common;

use common::term;
use vtium_term::{Attrs, COLOR_STACK_MAX, Color, Palette, Rgb, SGR_STACK_MAX, Style, Terminal};

fn pen(t: &Terminal) -> Style {
    *t.screen().pen()
}

fn reply(t: &mut Terminal, query: &[u8]) -> Vec<u8> {
    t.take_replies();
    t.input(query);
    t.take_replies()
}

/// A pen with every part SGR sets differing from the default.
const EVERY_PART: &[u8] = b"\x1b[1;2;3;4:3;5;7;9;38;5;100;48;2;1;2;3;58:5:200m";

/// A pen with every part SGR sets differing from both the default and
/// [`EVERY_PART`].
const OTHER_PARTS: &[u8] = b"\x1b[0;21;31;44;58:5:9m";

// ctlseqs, XTPUSHSGR and XTPOPSGR: with no parameters every video
// attribute is pushed, and the pop restores them; `CSI # p` and `CSI # q`
// are the same functions.
// covers: sgr.stack
#[test]
fn a_push_of_everything_and_its_pop_restore_the_pen() {
    for (push, pop) in [(&b"\x1b[#{"[..], &b"\x1b[#}"[..]), (b"\x1b[#p", b"\x1b[#q")] {
        let mut t = term(8, 2, EVERY_PART);
        let saved = pen(&t);
        t.input(push);
        t.input(OTHER_PARTS);
        assert_ne!(pen(&t), saved);
        t.input(pop);
        assert_eq!(pen(&t), saved, "{push:?} {pop:?}");
        t.input(b"\x1b[m");
        t.input(pop);
        assert_eq!(pen(&t), Style::DEFAULT, "an empty stack pops nothing");
    }
}

// ctlseqs, XTPUSHSGR: each parameter names the part of the pen its pop
// restores, by its SGR code: 1 bold, 2 faint, 3 italic, 4 underlined,
// 5 blink, 7 inverse, 9 crossed-out, 21 doubly underlined, 30 the
// foreground and 31 the background, with xterm's older 10 and 11 for 30
// and 31. The other parts keep what was set after the push.
#[test]
fn a_push_of_named_parts_restores_only_those() {
    let base = pen(&term(8, 2, EVERY_PART));
    let other = pen(&term(8, 2, OTHER_PARTS));
    let attr = |a: Attrs| move |p: &Style| p.attrs.contains(a);
    type Part = Box<dyn Fn(&Style) -> String>;
    let parts: [(&str, Part); 10] = [
        ("1", Box::new(move |p| format!("{}", attr(Attrs::BOLD)(p)))),
        ("2", Box::new(move |p| format!("{}", attr(Attrs::DIM)(p)))),
        (
            "3",
            Box::new(move |p| format!("{}", attr(Attrs::ITALIC)(p))),
        ),
        (
            "4",
            Box::new(|p| format!("{:?} {:?}", p.attrs.underline(), p.underline_color)),
        ),
        ("5", Box::new(move |p| format!("{}", attr(Attrs::BLINK)(p)))),
        (
            "7",
            Box::new(move |p| format!("{}", attr(Attrs::INVERSE)(p))),
        ),
        (
            "9",
            Box::new(move |p| format!("{}", attr(Attrs::STRIKE)(p))),
        ),
        (
            "21",
            Box::new(|p| format!("{:?} {:?}", p.attrs.underline(), p.underline_color)),
        ),
        ("30", Box::new(|p| format!("{:?}", p.fg))),
        ("31", Box::new(|p| format!("{:?}", p.bg))),
    ];
    for (code, part) in &parts {
        let mut t = term(8, 2, EVERY_PART);
        t.input(format!("\x1b[{code}#{{").as_bytes());
        t.input(OTHER_PARTS);
        t.input(b"\x1b[#}");
        let p = pen(&t);
        assert_eq!(part(&p), part(&base), "{code} restores its part");
        // 4 and 21 both name the underline, which vtium holds as one part
        let underline = |c: &str| c == "4" || c == "21";
        for (other_code, other_part) in &parts {
            if other_code == code || (underline(code) && underline(other_code)) {
                continue;
            }
            assert_eq!(
                other_part(&p),
                other_part(&other),
                "{code} leaves {other_code}"
            );
        }
    }
    // 10 and 11 are xterm's older codes for 30 and 31, and two parts in
    // one push restore both
    let mut t = term(8, 2, EVERY_PART);
    t.input(b"\x1b[10;11#{");
    t.input(OTHER_PARTS);
    t.input(b"\x1b[#}");
    assert_eq!((pen(&t).fg, pen(&t).bg), (base.fg, base.bg));
    assert_eq!(pen(&t).attrs, other.attrs);
    // a parameter naming no part pushes a pen whose pop restores nothing,
    // as in xterm
    let mut t = term(8, 2, EVERY_PART);
    t.input(b"\x1b[0;6;99#{");
    t.input(OTHER_PARTS);
    t.input(b"\x1b[#}");
    assert_eq!(pen(&t), other);
}

// ctlseqs: "The stack is limited to 10 levels." As in xterm, a push past
// the bound keeps nothing and its pop restores nothing, so pushes and pops
// still pair: the pops after them restore the pens the first ten pushes
// kept, newest first.
// covers: sgr.stack
#[test]
fn a_push_past_the_sgr_bound_keeps_nothing() {
    assert_eq!(SGR_STACK_MAX, 10);
    let mut t = term(8, 2, b"");
    for i in 1..=SGR_STACK_MAX + 2 {
        t.input(format!("\x1b[38;5;{i}m\x1b[#{{").as_bytes());
    }
    t.input(b"\x1b[38;5;99m");
    for _ in 0..2 {
        t.input(b"\x1b[#}");
        assert_eq!(pen(&t).fg, Color::Indexed(99));
    }
    for i in (1..=SGR_STACK_MAX).rev() {
        t.input(b"\x1b[#}");
        assert_eq!(pen(&t).fg, Color::Indexed(u8::try_from(i).unwrap()));
    }
    t.input(b"\x1b[38;5;99m\x1b[#}");
    assert_eq!(pen(&t).fg, Color::Indexed(99), "the stack is empty");
}

/// A terminal whose palette differs from the default in each kind of
/// color the stack keeps.
fn colored() -> Terminal {
    term(
        4,
        2,
        b"\x1b[31mx\x1b]4;1;#010101;200;#020202\x07\x1b]10;#030303;#040404;#050505;#060606;#070707\x07\x1b]17;#080808\x07\x1b]19;#090909\x07",
    )
}

// ctlseqs, XTPUSHCOLORS and XTPOPCOLORS: the push keeps the 256 palette
// colors and the dynamic ones, the defaults and the cursor's, pointer's
// and selection's colors, and the pop restores them all and recolors
// every cell.
// covers: color.stack
#[test]
fn xtpushcolors_and_xtpopcolors_restore_the_palette() {
    let mut t = colored();
    let saved = t.palette().clone();
    assert_ne!(saved, Palette::default());
    t.input(b"\x1b[#P");
    t.input(b"\x1b]4;1;#ffffff;200;#ffffff\x07\x1b]10;#ffffff;#ffffff;#ffffff;#ffffff;#ffffff\x07\x1b]117\x07\x1b]119\x07");
    assert_ne!(t.palette(), &saved);
    let since = t.screen_mut().checkpoint();
    t.input(b"\x1b[#Q");
    assert_eq!(t.palette(), &saved);
    let palette = t.palette().clone();
    let frame = t.screen().frame(&palette, since);
    assert!(frame.row_changed(0), "the pop recolors the screen");
    assert_eq!(frame.cell(0, 0).unwrap().fg, Rgb::new(1, 1, 1));
    // a pop that restores the colors already drawn marks nothing
    t.input(b"\x1b[#P");
    let since = t.screen_mut().checkpoint();
    t.input(b"\x1b[#Q");
    let palette = t.palette().clone();
    assert!(!t.screen().frame(&palette, since).row_changed(0));
}

// ctlseqs, XTREPORTCOLORS: `CSI ? Pu ; Ps # Q`, the current entry and the
// palettes stored, the form XTPOPCOLORS takes.
// covers: color.stack
#[test]
fn xtreportcolors_reports_the_entry_and_the_palettes_stored() {
    let mut t = term(4, 1, b"");
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?0;0#Q");
    t.input(b"\x1b[#P\x1b[#P");
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?2;2#Q");
    t.input(b"\x1b[#Q");
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?1;2#Q");
    // its parameters are ignored
    assert_eq!(reply(&mut t, b"\x1b[5;7#R"), b"\x1b[?1;2#Q");
}

// ctlseqs, XTPUSHCOLORS and XTPOPCOLORS: a parameter of 1 to 10 stores
// the palette in that entry, or restores it from that entry, which
// becomes the current one; 0 pushes and pops; several parameters act in
// turn; a parameter past 10 does nothing.
#[test]
fn a_color_stack_entry_is_stored_and_restored_by_number() {
    let red = |t: &Terminal| t.palette().color(1);
    let mut t = term(
        4,
        1,
        b"\x1b]4;1;#030303\x07\x1b[3#P\x1b]4;1;#050505\x07\x1b[5#P",
    );
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?5;5#Q");
    t.input(b"\x1b]4;1;#ffffff\x07\x1b[3#Q");
    assert_eq!(red(&t), Rgb::new(3, 3, 3));
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?2;5#Q");
    // an entry never stored restores nothing and moves nothing
    t.input(b"\x1b[#Q");
    assert_eq!(red(&t), Rgb::new(3, 3, 3));
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?2;5#Q");
    t.input(b"\x1b[5#Q");
    assert_eq!(red(&t), Rgb::new(5, 5, 5));
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?4;5#Q");
    // 0 pushes, a parameter past 10 does nothing, and each acts in turn
    t.input(b"\x1b]4;1;#070707\x07\x1b[0;11;1#P");
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?1;5#Q");
    t.input(b"\x1b]4;1;#ffffff\x07\x1b[5#Q");
    assert_eq!(red(&t), Rgb::new(7, 7, 7), "the 0 stored entry 5");
    t.input(b"\x1b[11#Q");
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?4;5#Q");
}

// ctlseqs, XTPUSHCOLORS: entries 1 through 10. A push with the stack full
// keeps nothing and moves nothing, as in xterm; a pop of an empty stack
// does nothing.
// covers: color.stack
#[test]
fn a_push_past_the_color_bound_keeps_nothing() {
    assert_eq!(COLOR_STACK_MAX, 10);
    let mut t = term(4, 1, b"");
    t.input(b"\x1b[#Q");
    assert_eq!(t.palette(), &Palette::default());
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?0;0#Q");
    for i in 1..=COLOR_STACK_MAX + 2 {
        t.input(format!("\x1b]4;1;rgb:{i:02x}/00/00\x07\x1b[#P").as_bytes());
    }
    let max = COLOR_STACK_MAX;
    assert_eq!(
        reply(&mut t, b"\x1b[#R"),
        format!("\x1b[?{max};{max}#Q").into_bytes()
    );
    for i in (1..=COLOR_STACK_MAX).rev() {
        t.input(b"\x1b[#Q");
        assert_eq!(
            t.palette().color(1),
            Rgb::new(u8::try_from(i).unwrap(), 0, 0)
        );
    }
    t.input(b"\x1b]4;1;#ffffff\x07\x1b[#Q");
    assert_eq!(t.palette().color(1), Rgb::new(255, 255, 255));
    assert_eq!(
        reply(&mut t, b"\x1b[#R"),
        format!("\x1b[?0;{max}#Q").into_bytes()
    );
}

// kitty, `color-stack`: OSC 30001 pushes the colors and OSC 30101 pops
// them, ended by BEL or ST. They push and pop the stack XTPUSHCOLORS and
// XTPOPCOLORS do.
// covers: color.stack
#[test]
fn osc_30001_and_30101_push_and_pop_the_colors() {
    let mut t = colored();
    let saved = t.palette().clone();
    t.input(b"\x1b]30001\x1b\\\x1b]104\x07\x1b]110;111\x07");
    assert_ne!(t.palette(), &saved);
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?1;1#Q");
    t.input(b"\x1b]30101\x07");
    assert_eq!(t.palette(), &saved);
    t.input(b"\x1b[#P\x1b]104\x07\x1b]30101\x1b\\");
    assert_eq!(
        t.palette(),
        &saved,
        "XTPUSHCOLORS and OSC 30101 share the stack"
    );
    t.input(b"\x1b]30001\x07\x1b]104\x07\x1b[#Q");
    assert_eq!(
        t.palette(),
        &saved,
        "OSC 30001 and XTPOPCOLORS share the stack"
    );
    // with anything after the number they are other strings
    t.input(b"\x1b]30001;1\x07\x1b]30001;\x07");
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?0;1#Q");
}

// RIS returns the terminal to its initial state, so it empties both
// stacks, as foot empties its color stack. DECSTR keeps them, as xterm's
// soft reset and foot's do (`ReallyReset` in xterm 412's `charproc.c`).
#[test]
fn ris_empties_the_stacks_and_decstr_keeps_them() {
    let mut t = colored();
    let saved_palette = t.palette().clone();
    t.input(b"\x1b[1;31m\x1b[#{\x1b[#P\x1b[!p");
    assert_eq!(pen(&t), Style::DEFAULT);
    t.input(b"\x1b[#}\x1b[#Q");
    assert_eq!(pen(&t).fg, Color::Indexed(1), "DECSTR keeps the SGR stack");
    assert_eq!(t.palette(), &saved_palette, "DECSTR keeps the color stack");

    t.input(b"\x1b[#{\x1b[#P\x1bc");
    assert_eq!(reply(&mut t, b"\x1b[#R"), b"\x1b[?0;0#Q");
    t.input(b"\x1b[#}\x1b[#Q");
    assert_eq!(pen(&t), Style::DEFAULT, "RIS empties the SGR stack");
    assert_eq!(
        t.palette(),
        &Palette::default(),
        "RIS empties the color stack"
    );
}

// A program that pushes without popping, or pops without pushing, holds
// no more than each stack's bound: 100,000 of each leave the stacks
// working as at their bound.
#[test]
fn a_flood_of_pushes_and_pops_stays_at_the_bounds() {
    let mut t = term(4, 1, b"\x1b[3m\x1b]4;1;#010101\x07");
    let n = 100_000;
    t.input(&b"\x1b[#{\x1b[#P\x1b]30001\x07".repeat(n));
    let max = COLOR_STACK_MAX;
    assert_eq!(
        reply(&mut t, b"\x1b[#R"),
        format!("\x1b[?{max};{max}#Q").into_bytes()
    );
    t.input(b"\x1b[m\x1b]104\x07");
    t.input(&b"\x1b[#}\x1b[#Q".repeat(n));
    assert!(pen(&t).attrs.contains(Attrs::ITALIC));
    assert_eq!(t.palette().color(1), Rgb::new(1, 1, 1));
    assert_eq!(
        reply(&mut t, b"\x1b[#R"),
        format!("\x1b[?0;{max}#Q").into_bytes()
    );
    // the stacks are empty, and work again
    t.input(b"\x1b[m\x1b[#}");
    assert_eq!(pen(&t), Style::DEFAULT);
    t.input(b"\x1b[1m\x1b[#{\x1b[m\x1b[#}");
    assert!(pen(&t).attrs.contains(Attrs::BOLD));
}

// The C reference ignored each of these sequences; none of them moves the
// cursor or prints.
#[test]
fn the_stacks_print_nothing() {
    let t = term(
        8,
        1,
        b"a\x1b[#{\x1b[#}\x1b[#p\x1b[#q\x1b[#P\x1b[#Q\x1b]30001\x07\x1b]30101\x07b",
    );
    assert_eq!(common::rows(t.screen()), ["ab"]);
    assert_eq!(t.screen().cursor(), (2, 0));
    assert!(t.replies().is_empty());
}
