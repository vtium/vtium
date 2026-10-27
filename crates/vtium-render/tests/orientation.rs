//! The four orientations draw the same picture turned: each turned screen,
//! turned back, is the unturned screen pixel for pixel. Its screen has a
//! wide character cut at the last column the display holds, which shows
//! its left half in every orientation (aaronsb/kmscon#44).

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
use vtium_term::{Age, Attrs, CursorShape, UnderlineStyle};

/// 21 columns, one more than the displays below hold, with a wide
/// character in the last column they show.
fn screen() -> Screen {
    let mut s = Screen::new(21, 3, rgb("c0c0c0"), rgb("101010"));
    let plain = style(rgb("ffffff"), rgb("203040"));
    s.text(0, 0, "Turn me", plain);
    let mut ul = style(rgb("ffff00"), rgb("400000"));
    ul.attrs.set_underline(UnderlineStyle::Curly);
    ul.attrs.insert(Attrs::STRIKE);
    s.put(8, 0, '中', true, ul);
    s.put(19, 0, '国', true, plain);
    s.put(19, 1, '字', true, ul);
    s.text(0, 2, "│▌⠿ abc", style(rgb("00ff80"), rgb("101010")));
    s.cursor.visible = true;
    s.cursor.row = 1;
    s.cursor.col = 3;
    s.cursor.shape = CursorShape::Underline;
    s
}

const W: u32 = 20 * 8 + 6;
const H: u32 = 3 * 16 + 4;

fn draw(s: &Screen, o: Orientation) -> Canvas {
    let (w, h) = if o == Orientation::Right || o == Orientation::Left {
        (H, W)
    } else {
        (W, H)
    };
    let config = Config {
        orientation: o,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), w, h, config);
    assert_eq!(r.grid_size(), (20, 3));
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on: true,
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// `c`, drawn turned by `o`, turned back.
fn unturn(c: &Canvas, o: Orientation) -> Canvas {
    let mut out = Canvas::new(W, H, 0);
    for y in 0..H {
        let row: Vec<u32> = (0..W)
            .map(|x| {
                let (tx, ty) = match o {
                    Orientation::Normal => (x, y),
                    Orientation::UpsideDown => (W - 1 - x, H - 1 - y),
                    Orientation::Right => (H - 1 - y, x),
                    Orientation::Left => (y, W - 1 - x),
                };
                c.pixel(tx, ty).unwrap()
            })
            .collect();
        vtium_render::Target::write(&mut out, 0, y, &row);
    }
    out
}

// covers: platform.compositor
#[test]
fn every_orientation_draws_the_same_picture_turned() {
    let s = screen();
    let normal = draw(&s, Orientation::Normal);
    for o in [
        Orientation::Right,
        Orientation::UpsideDown,
        Orientation::Left,
    ] {
        assert!(unturn(&draw(&s, o), o) == normal, "{o:?}");
    }
}

// The cut glyph's cell holds the glyph's left half: the same pixels as the
// first cell of the glyph drawn whole on a display one column wider.
// covers: platform.compositor
#[test]
fn a_wide_glyph_cut_at_the_margin_shows_its_left_half() {
    let s = screen();
    let cut = draw(&s, Orientation::Normal);
    let mut r = Renderer::new(unifont(), W + 8, H, Config::default());
    let mut whole = Canvas::new(W + 8, H, 0);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut whole, 0, &st);
    // the grid starts 3 pixels in and 2 down on both displays
    for row in [0u32, 1] {
        for y in 0..16 {
            for x in 0..8 {
                let a = cut.pixel(3 + 19 * 8 + x, 2 + row * 16 + y).unwrap();
                let b = whole.pixel(3 + 19 * 8 + x, 2 + row * 16 + y).unwrap();
                assert_eq!(a, b, "row {row} pixel {x},{y}");
            }
        }
    }
    let lit = (0..16)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .filter(|&(x, y)| cut.pixel(3 + 19 * 8 + x, 2 + y).unwrap() == 0xffffff)
        .count();
    assert!(lit > 10, "the half glyph is drawn");
}

// Cells 208 pixels high make an unturned row of 1400 pixels 1.1 MiB, past
// the row scratch's 1 MiB, so its decorated span is composed in bands; a
// display turned a quarter composes the same row in one strip 208 pixels
// wide. Turned back, the two are the same.
// covers: platform.compositor
#[test]
fn a_row_too_tall_for_the_scratch_is_composed_in_bands() {
    let mut s = Screen::new(13, 2, rgb("c0c0c0"), rgb("101010"));
    let mut ul = style(rgb("ffff00"), rgb("400000"));
    ul.attrs.set_underline(UnderlineStyle::Curly);
    ul.attrs.insert(Attrs::STRIKE);
    s.text(0, 0, "Banded", ul);
    s.put(7, 0, '中', true, ul);
    s.text(0, 1, "plain", style(rgb("ffffff"), rgb("203040")));
    let font = || vtium_font::Font::new(vtium_font::Unifont::new(208));
    let (w, h) = (13 * 104 + 48, 2 * 208 + 6);
    let ds = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let mut normal = Renderer::new(font(), w, h, Config::default());
    assert!(normal.row_scratch_bytes() <= vtium_render::ROW_SCRATCH_MAX);
    assert!(w as usize * 208 * 4 > vtium_render::ROW_SCRATCH_MAX);
    let mut a = Canvas::new(w, h, 0);
    normal.draw(&s.frame(Age::NEVER), &mut a, 0, &ds);
    let config = Config {
        orientation: Orientation::Right,
        ..Config::default()
    };
    let mut right = Renderer::new(font(), h, w, config);
    let mut b = Canvas::new(h, w, 0);
    right.draw(&s.frame(Age::NEVER), &mut b, 0, &ds);
    for y in 0..h {
        for x in 0..w {
            assert_eq!(a.pixel(x, y), b.pixel(h - 1 - y, x), "{x},{y}");
        }
    }
}

// The pointer on a display turned right: the font's I turned a quarter,
// in the default colours, in a box 16 wide and 8 high. Six columns and two
// rows on a display 36 by 54 put the grid at 2,3 to 34,51; the pointer at
// 20,9 in the grid's pixels centres its box at 25,23, so it spans 17 to 33
// across and 19 to 27 down, and the glyph's pixel gx,gy lands at
// 17 + 15 - gy, 19 + gx.
// covers: platform.compositor
#[test]
fn the_pointer_turns_with_the_display() {
    let s = Screen::new(6, 2, rgb("c0c0c0"), rgb("101010"));
    let config = Config {
        orientation: Orientation::Right,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), 36, 54, config);
    assert_eq!(r.grid_size(), (6, 2));
    let mut c = Canvas::new(36, 54, 0);
    let st = DrawState {
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        pointer: Some((20, 9)),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    let i = unifont()
        .glyph(
            &['I'],
            vtium_font::Width::Narrow,
            vtium_font::Style::default(),
        )
        .unwrap();
    assert_eq!((i.width(), i.height()), (8, 16));
    let (fg, bg) = (0xc0_c0_c0, 0x10_10_10);
    let mut lit = 0;
    for y in 0..54 {
        for x in 0..36 {
            let want = if (17..33).contains(&x) && (19..27).contains(&y) {
                let (gx, gy) = (y - 19, 15 - (x - 17));
                match i.pixel(gx, gy).unwrap() {
                    [0] => bg,
                    [255] => {
                        lit += 1;
                        fg
                    }
                    other => panic!("a bitmap glyph's coverage: {other:?}"),
                }
            } else {
                bg
            };
            assert_eq!(c.pixel(x, y), Some(want), "{x},{y}");
        }
    }
    assert!(lit > 10, "the I has pixels");
}
