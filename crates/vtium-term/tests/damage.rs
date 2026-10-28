//! Change tracking, as a property: after any change, every cell a display
//! would draw differently is marked changed for it (aaronsb/libtsm#48).
//!
//! Each step takes the frame a display drew, makes one change through the
//! byte stream or the screen's own calls (the pen, the modes, resizing,
//! the view, the selection), and compares the frame after it cell by cell.
//! A cell that differs and is not marked fails the test, which prints the
//! seed, the step and the change. The rows the marks may spare are the
//! point of aaronsb/libtsm#34: a region scroll marks only the region, and
//! this test holds that sparing honest.

mod common;

use common::{Rng, SEQUENCES, TEXT};
use vtium_term::{
    Attrs, Color, CursorShape, Glyph, LinkId, Palette, Rgb, Screen, Style, Terminal,
    UnderlineStyle, Width, WidthMethod,
};

/// One cell as a display draws it.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    glyph: String,
    width: Width,
    fg: Rgb,
    bg: Rgb,
    underline: Option<Rgb>,
    attrs: Attrs,
    default_bg: bool,
    link: Option<LinkId>,
    selected: bool,
}

fn palette() -> Palette {
    let mut colors = [Rgb::default(); 256];
    for (i, c) in colors.iter_mut().enumerate() {
        let i = i as u8;
        *c = Rgb::new(i, i.wrapping_mul(3), i.wrapping_mul(7));
    }
    Palette::new(colors, Rgb::new(0xe5, 0xe5, 0xe5), Rgb::new(0, 0, 0))
}

/// The view as a display draws it, and which cells the frame marks
/// changed since `since`.
fn draw(screen: &Screen, since: vtium_term::Age) -> Vec<Vec<(Drawn, bool)>> {
    let palette = palette();
    let frame = screen.frame(&palette, since);
    (0..frame.rows())
        .map(|r| {
            (0..frame.cols())
                .map(|c| {
                    let cell = frame.cell(r, c).unwrap();
                    let glyph = match cell.glyph {
                        Glyph::Empty => String::new(),
                        Glyph::Char(ch) => ch.into(),
                        Glyph::Cluster(s) => s.into(),
                    };
                    let drawn = Drawn {
                        glyph,
                        width: cell.width,
                        fg: cell.fg,
                        bg: cell.bg,
                        underline: cell.underline_color,
                        attrs: cell.attrs,
                        default_bg: cell.default_bg,
                        link: cell.link,
                        selected: cell.selected,
                    };
                    (drawn, cell.changed)
                })
                .collect()
        })
        .collect()
}

fn random_style(rng: &mut Rng) -> Style {
    let color = |rng: &mut Rng| match rng.below(3) {
        0 => Color::Default,
        1 => Color::Indexed(rng.next() as u8),
        _ => Color::Rgb(Rgb::new(rng.next() as u8, 1, 2)),
    };
    let mut style = Style {
        fg: color(rng),
        bg: color(rng),
        underline_color: color(rng),
        ..Style::DEFAULT
    };
    for flag in [Attrs::BOLD, Attrs::INVERSE, Attrs::PROTECTED, Attrs::STRIKE] {
        if rng.below(3) == 0 {
            style.attrs.insert(flag);
        }
    }
    if rng.below(4) == 0 {
        style.attrs.set_underline(UnderlineStyle::Curly);
    }
    style
}

/// One change, described for a failure's message.
fn change(t: &mut Terminal, rng: &mut Rng) -> String {
    let s = t.screen_mut();
    let (cols, rows) = (s.cols(), s.rows());
    match rng.below(24) {
        0 => {
            s.set_pen(random_style(rng));
            "pen".into()
        }
        1 => {
            let link = (rng.below(2) == 0).then(|| LinkId::new(rng.below(4) as u32));
            s.set_link(link);
            format!("link {link:?}")
        }
        2 => {
            let on = rng.below(2) == 0;
            s.set_reverse_video(on);
            format!("reverse {on}")
        }
        3 => {
            let on = rng.below(2) == 0;
            s.set_alternate(on);
            format!("alternate {on}")
        }
        4 => {
            let (c, r) = (1 + rng.below(12), 1 + rng.below(8));
            s.resize(c, r);
            format!("resize {c}x{r}")
        }
        5 => {
            let n = rng.below(4);
            s.scroll_view_up(n);
            format!("view up {n}")
        }
        6 => {
            let n = rng.below(4);
            s.scroll_view_down(n);
            format!("view down {n}")
        }
        7 => {
            s.reset_view();
            "view reset".into()
        }
        8 => {
            let (x, y) = (rng.below(cols), rng.below(rows));
            s.select_start(x, y);
            format!("select start {x},{y}")
        }
        9 => {
            let (x, y) = (rng.below(cols), rng.below(rows));
            s.select_to(x, y);
            format!("select to {x},{y}")
        }
        10 => {
            let (x, y) = (rng.below(cols), rng.below(rows));
            s.select_word(x, y);
            format!("select word {x},{y}")
        }
        11 => {
            s.clear_selection();
            "select clear".into()
        }
        12 => {
            let on = rng.below(2) == 0;
            s.set_insert_mode(on);
            format!("insert {on}")
        }
        13 => {
            let on = rng.below(2) == 0;
            s.set_auto_wrap(on);
            format!("autowrap {on}")
        }
        14 => {
            let on = rng.below(2) == 0;
            s.set_origin_mode(on);
            format!("origin {on}")
        }
        15 => {
            let m = if rng.below(2) == 0 {
                WidthMethod::Unicode
            } else {
                WidthMethod::Wcwidth
            };
            s.set_width_method(m);
            format!("width {m:?}")
        }
        16 => {
            let n = rng.below(4);
            s.set_scrollback_max(n);
            format!("scrollback {n}")
        }
        17 => {
            s.set_cursor_style(CursorShape::Bar, false);
            s.set_cursor_visible(rng.below(2) == 0);
            "cursor".into()
        }
        _ => {
            let mut bytes = Vec::new();
            for _ in 0..1 + rng.below(3) {
                let piece = if rng.below(2) == 0 {
                    rng.pick(SEQUENCES)
                } else {
                    rng.pick(TEXT)
                };
                bytes.extend_from_slice(piece.as_bytes());
            }
            t.input(&bytes);
            format!("bytes {:?}", String::from_utf8_lossy(&bytes))
        }
    }
}

// aaronsb/libtsm#48: the frame's changed set covers every cell that
// differs, through every kind of change.
#[test]
fn a_frames_changed_set_covers_every_cell_that_differs() {
    for seed in 0..common::seeds(80) {
        covers_every_change(seed);
    }
}

// Seed 218 found an edit that ends a row's wrap, IL here, turning the pad
// in its last column into an empty narrow cell unmarked (ADR-1003).
#[test]
fn an_ended_wrap_marks_the_pad_it_empties() {
    covers_every_change(218);
}

/// Runs the property's 100 random changes from `seed`.
fn covers_every_change(seed: u64) {
    {
        let mut rng = Rng(seed);
        let mut t = Terminal::new(1 + rng.below(12), 1 + rng.below(8), 6);
        t.screen_mut().set_scrollback_max(rng.below(6));
        for step in 0..100 {
            let seen = t.screen_mut().checkpoint();
            let before = draw(t.screen(), seen);
            let what = change(&mut t, &mut rng);
            let after = draw(t.screen(), seen);
            if before.len() != after.len()
                || before.first().map(Vec::len) != after.first().map(Vec::len)
            {
                let all = after.iter().flatten().all(|&(_, changed)| changed);
                assert!(
                    all,
                    "seed {seed}, step {step}, {what}: a resize left cells unmarked"
                );
                continue;
            }
            for (r, (b, a)) in before.iter().zip(&after).enumerate() {
                for (c, ((was, _), (now, changed))) in b.iter().zip(a).enumerate() {
                    assert!(
                        was == now || *changed,
                        "seed {seed}, step {step}, {what}: cell {r},{c} went from {was:?} to {now:?} unmarked"
                    );
                }
            }
        }
    }
}

// A change that alters nothing visible may mark cells, but a display that
// drew after it sees nothing changed until the next change.
#[test]
fn after_a_checkpoint_nothing_is_changed() {
    let mut t = Terminal::new(8, 4, 10);
    t.input("ab\u{4E2D}\x1b[2;3r\n\n\n".as_bytes());
    let seen = t.screen_mut().checkpoint();
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
    // Moving the cursor and changing the pen mark nothing (ADR-1001).
    t.input(b"\x1b[3;3H\x1b[A\x1b[2C");
    t.screen_mut().set_pen(Style {
        bg: Color::Indexed(2),
        ..Style::DEFAULT
    });
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
}

// Clearing the selection marks every cell, since its cells drew inverted;
// with none to clear it marks nothing, as the host clears it at each key.
#[test]
fn clearing_no_selection_marks_nothing() {
    let mut t = Terminal::new(8, 4, 10);
    t.input(b"ab");
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().clear_selection();
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
    t.screen_mut().select_start(0, 0);
    let seen = t.screen_mut().checkpoint();
    t.screen_mut().clear_selection();
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| c));
}

// A palette the host sets while text is on screen recolors every cell, so
// a display that drew before it sees every cell changed.
#[test]
fn a_new_palette_marks_every_cell() {
    let mut t = Terminal::new(8, 4, 10);
    t.input(b"ab\x1b[31mcd");
    let seen = t.screen_mut().checkpoint();
    let mut changed = palette();
    changed.set_color(1, Rgb::new(1, 2, 3));
    t.set_palette(changed.clone());
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| c));
    // The same palette again changes nothing a display draws.
    let seen = t.screen_mut().checkpoint();
    t.set_palette(changed);
    assert!(draw(t.screen(), seen).iter().flatten().all(|&(_, c)| !c));
}
