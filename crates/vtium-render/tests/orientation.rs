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
    r.draw(&s.frame(Age::NEVER), &mut c, &st);
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
    r.draw(&s.frame(Age::NEVER), &mut whole, &st);
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
