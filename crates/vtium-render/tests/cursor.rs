//! The cursor's shapes, as the C reference drew them (libtsm's
//! `frame_cursor_cell`, kmscon's `bbulk_draw`): a block swaps the cell's
//! colors, an underline toggles a single underline in the foreground, and
//! a bar draws U+258F in place of the character. Each case is checked
//! against the same screen without a cursor, styled to look as the cursor
//! should.

mod common;

use common::{Screen, arrow_over, rgb, style, unifont};
use vtium_render::{Arrow, Canvas, Config, DrawState, Orientation, PointerShape, Renderer};
use vtium_term::{Age, CursorShape, Rgb, Style, UnderlineStyle};

const FG: &str = "e0e0e0";
const BG: &str = "203040";

fn draw(s: &Screen, cursor_on: bool, pointer: Option<(u32, u32)>) -> Canvas {
    draw_with(s, cursor_on, pointer, Config::default())
}

fn draw_with(s: &Screen, cursor_on: bool, pointer: Option<(u32, u32)>, config: Config) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on,
        background: s.palette.bg(),
        pointer,
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// "ab中c" in `st`, the cursor at `col` in `shape` with `color`.
fn screen(st: Style, col: usize, shape: CursorShape, color: Option<Rgb>) -> Screen {
    let mut s = Screen::new(6, 1, rgb("c0c0c0"), rgb("101010"));
    s.text(0, 0, "ab", st);
    s.put(2, 0, '中', true, st);
    s.put(4, 0, 'c', false, st);
    s.cursor.visible = true;
    s.cursor.col = col;
    s.cursor.shape = shape;
    s.cursor.color = color;
    s
}

fn swapped(st: Style) -> Style {
    Style {
        fg: st.bg,
        bg: st.fg,
        ..st
    }
}

// covers: cursor.style
#[test]
fn a_block_swaps_the_cells_colors() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 1, CursorShape::Block, None);
    let mut want = screen(st, 1, CursorShape::Block, None);
    want.put(1, 0, 'b', false, swapped(st));
    assert!(draw(&s, true, None) == draw(&want, false, None));
    // a hidden or blinked-off cursor draws the cell as it is
    assert!(draw(&s, false, None) == draw(&screen(st, 1, CursorShape::Block, None), false, None));
    let mut hidden = screen(st, 1, CursorShape::Block, None);
    hidden.cursor.visible = false;
    assert!(draw(&hidden, true, None) == draw(&s, false, None));
}

// On the trailing half of a wide character the cursor covers the whole
// character, from its leading cell.
// covers: cursor.style
#[test]
fn a_block_on_a_trailing_half_covers_the_wide_character() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 3, CursorShape::Block, None);
    let mut want = screen(st, 3, CursorShape::Block, None);
    want.put(2, 0, '中', true, swapped(st));
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

// OSC 12 sets the block's color; the character keeps the cell's
// background as its color.
// covers: cursor.color
#[test]
fn a_colored_block_takes_its_color() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 0, CursorShape::Block, Some(rgb("ff8000")));
    let mut want = screen(st, 0, CursorShape::Block, None);
    want.put(0, 0, 'a', false, style(rgb(BG), rgb("ff8000")));
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

// covers: cursor.style
#[test]
fn an_underline_cursor_toggles_a_single_underline() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 4, CursorShape::Underline, None);
    let mut want = screen(st, 4, CursorShape::Underline, None);
    let mut ul = st;
    ul.attrs.set_underline(UnderlineStyle::Single);
    want.put(4, 0, 'c', false, ul);
    assert!(draw(&s, true, None) == draw(&want, false, None));

    // on underlined text it takes the underline away
    let mut curly = st;
    curly.attrs.set_underline(UnderlineStyle::Curly);
    let s = screen(curly, 4, CursorShape::Underline, None);
    let mut want = screen(curly, 4, CursorShape::Underline, None);
    want.put(4, 0, 'c', false, st);
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

// covers: cursor.style
#[test]
fn a_bar_replaces_the_character_with_a_left_eighth_block() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 0, CursorShape::Bar, None);
    let mut want = screen(st, 0, CursorShape::Bar, None);
    want.put(0, 0, '▏', false, st);
    assert!(draw(&s, true, None) == draw(&want, false, None));

    // on a wide character: the bar in the first cell, the second empty
    let s = screen(st, 2, CursorShape::Bar, None);
    let mut want = screen(st, 2, CursorShape::Bar, None);
    want.put(2, 0, '▏', false, st);
    want.put(3, 0, ' ', false, st);
    assert!(draw(&s, true, None) == draw(&want, false, None));

    // in OSC 12's color
    let s = screen(st, 0, CursorShape::Bar, Some(rgb("00ff00")));
    let mut want = screen(st, 0, CursorShape::Bar, None);
    want.put(0, 0, '▏', false, style(rgb("00ff00"), rgb(BG)));
    assert!(draw(&s, true, None) == draw(&want, false, None));
}

// The pointer drawn in software: the arrow a cell tall, its tip on the
// pointer's pixel kept inside the grid, blended over the cells it covers,
// which show through around it.
// covers: platform.compositor
#[test]
fn the_pointer_is_an_arrow_over_the_cells() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 0, CursorShape::Block, None);
    let without = draw(&s, false, None);
    let arrow = Arrow::new(16, Orientation::Normal);
    assert_eq!(
        (arrow.width(), arrow.height(), arrow.hot()),
        (11, 16, (1, 1))
    );
    let at = |x: u32, y: u32| arrow.pixels()[(y * 11 + x) as usize];
    // at 20,9 the arrow's corner is at 19,8; far past the grid the tip is
    // kept on the grid's last pixel, 47,15, and the arrow runs off the
    // display
    for (pointer, (ax, ay)) in [((20, 9), (19, 8)), ((10_000, 10_000), (46, 14))] {
        let with = draw(&s, false, Some(pointer));
        let mut lit = 0;
        for y in 0..16 {
            for x in 0..48 {
                let under = without.pixel(x, y).unwrap();
                let inside = (ax..ax + 11).contains(&x) && (ay..ay + 16).contains(&y);
                let want = match inside.then(|| at(x - ax, y - ay)) {
                    Some(src) if src != 0 => {
                        lit += 1;
                        arrow_over(under, src)
                    }
                    _ => under,
                };
                assert_eq!(with.pixel(x, y).unwrap(), want, "{pointer:?} at {x},{y}");
            }
        }
        assert!(lit > 1, "{pointer:?}: the arrow shows");
    }
    // the tip's fill is white, whatever is under it
    let with = draw(&s, false, Some((20, 9)));
    assert_eq!(with.pixel(20, 10), Some(0xff_ff_ff));
}

// `pointer-char`: the glyph drawn as the pointer, filled and outlined as
// the arrow is, on the cell under the pointer wherever in the cell the
// pointer is; the cursor plane takes it at the same place.
#[test]
fn a_glyph_pointer_sits_on_the_cell_under_the_pointer() {
    let st = style(rgb(FG), rgb(BG));
    let s = screen(st, 0, CursorShape::Block, None);
    let without = draw(&s, false, None);
    let config = Config {
        pointer: PointerShape::Glyph('+'),
        ..Config::default()
    };
    let glyph = unifont()
        .glyph(
            &['+'],
            vtium_font::Width::Narrow,
            vtium_font::Style::default(),
        )
        .unwrap();
    let image = Arrow::glyph(&glyph, Orientation::Normal);
    let (iw, ih) = (image.width(), image.height());
    assert_eq!((iw, ih, image.hot()), (10, 18, (1, 1)));
    let at = |x: u32, y: u32| image.pixels()[(y * iw + x) as usize];
    let with = draw_with(&s, false, Some((20, 9)), config);
    // cell 2 starts at 16,0, the image's corner one pixel up and left
    let (ax, ay) = (15_i64, -1_i64);
    let mut lit = 0;
    for y in 0..16_u32 {
        for x in 0..48_u32 {
            let under = without.pixel(x, y).unwrap();
            let (ix, iy) = (i64::from(x) - ax, i64::from(y) - ay);
            let inside = (0..i64::from(iw)).contains(&ix) && (0..i64::from(ih)).contains(&iy);
            let want = match inside.then(|| at(ix as u32, iy as u32)) {
                Some(src) if src != 0 => {
                    lit += 1;
                    arrow_over(under, src)
                }
                _ => under,
            };
            assert_eq!(with.pixel(x, y).unwrap(), want, "at {x},{y}");
        }
    }
    assert!(lit > 1, "the glyph shows");
    // anywhere in the cell, the same frame
    assert!(draw_with(&s, false, Some((23, 15)), config) == with);
    // the cursor plane's place is the cell's corner too
    let (w, h) = (48, 16);
    let mut r = Renderer::new(unifont(), w, h, config);
    r.draw(
        &s.frame(Age::NEVER),
        &mut Canvas::new(w, h, 0),
        0,
        &DrawState::default(),
    );
    assert_eq!(r.arrow(), &image);
    assert_eq!(r.pointer_at((23, 15)), Some((16, 0)));
}

// A character the font lacks would draw a fallback (the missing box, U+FFFD,
// '?'), and a blank one would light nothing: either keeps the arrow.
#[test]
fn a_glyph_pointer_the_font_cannot_show_keeps_the_arrow() {
    let arrow = Arrow::new(16, Orientation::Normal);
    for c in ['\u{10FFFD}', ' ', '\u{3000}', '\u{a0}'] {
        let config = Config {
            pointer: PointerShape::Glyph(c),
            ..Config::default()
        };
        let r = Renderer::new(unifont(), 48, 16, config);
        assert_eq!(r.arrow(), &arrow, "{c:?}");
    }
    let config = Config {
        pointer: PointerShape::Glyph('+'),
        ..Config::default()
    };
    assert_ne!(Renderer::new(unifont(), 48, 16, config).arrow(), &arrow);
}
