//! The cursor's shapes: a block swaps the cell's
//! colors, an underline toggles a single underline in the foreground, and
//! a bar draws over the cell's left edge, the character under it as it draws
//! without a cursor. Each case is checked
//! against the same screen without a cursor, styled to look as the cursor
//! should.

mod common;

use common::{Screen, arrow_over, rgb, style, unifont};
use vtium_render::{Arrow, Canvas, Config, DrawState, Orientation, PointerShape, Renderer, Target};
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

/// What a bar cursor at `col` of `s` should draw: the cells as they draw
/// with no cursor, and over them, in the cursor's cell, the bar's pixels,
/// which are the pixels where U+258F, LEFT ONE EIGHTH BLOCK, drawn in the
/// bar's color on a blank cell, differs from that blank cell. Also gives
/// how many of the bar's pixels there are.
fn letter_under_bar(s: &Screen, col: usize, cells: u32, bar: Rgb) -> (Canvas, u32) {
    let mut want = draw(s, false, None);
    let st = style(bar, rgb(BG));
    let mut marked = Screen::new(s.cols, 1, rgb("c0c0c0"), rgb("101010"));
    let mut blank = Screen::new(s.cols, 1, rgb("c0c0c0"), rgb("101010"));
    for k in 0..cells as usize {
        marked.put(col + k, 0, ' ', false, st);
        blank.put(col + k, 0, ' ', false, st);
    }
    marked.put(col, 0, '\u{258f}', false, st);
    let (marked, blank) = (draw(&marked, false, None), draw(&blank, false, None));
    let x0 = col as u32 * 8;
    let mut n = 0;
    for y in 0..16 {
        for x in x0..x0 + cells * 8 {
            let m = marked.pixel(x, y).unwrap();
            if m != blank.pixel(x, y).unwrap() {
                want.write(x, y, &[m]);
                n += 1;
            }
        }
    }
    (want, n)
}

/// Whether the cursor's cell in `got` shows the letter's own foreground
/// somewhere outside the bar, as `want` has it.
fn shows_letter(got: &Canvas, col: usize, cells: u32, fg: Rgb) -> bool {
    let fg = u32::from(fg.r) << 16 | u32::from(fg.g) << 8 | u32::from(fg.b);
    let x0 = col as u32 * 8;
    (0..16).any(|y| (x0 + 1..x0 + cells * 8).any(|x| got.pixel(x, y) == Some(fg)))
}

// The maintainer's look at a steady bar cursor on a real VT, 2026-11-14:
// the bar never hides the letter in its cell. The letter draws as it does
// with no cursor, and the bar draws over it along the cell's left edge,
// one eighth of the cell wide, as the underline cursor draws over the
// cell's bottom.
// covers: cursor.style
#[test]
fn a_bar_draws_over_the_letter_along_the_cells_left_edge() {
    let st = style(rgb(FG), rgb(BG));
    for (col, cells, color) in [
        (0, 1, None),
        (4, 1, None),
        // on a wide character, along the left edge of its first cell
        (2, 2, None),
        (3, 2, None),
        // in OSC 12's color
        (0, 1, Some(rgb("00ff00"))),
    ] {
        let s = screen(st, col, CursorShape::Bar, color);
        let lead = if col == 3 { 2 } else { col };
        let (want, n) = letter_under_bar(&s, lead, cells, color.unwrap_or(rgb(FG)));
        let got = draw(&s, true, None);
        assert_eq!(
            n, 16,
            "column {col}: the bar is a pixel wide and a cell tall"
        );
        assert!(
            got == want,
            "column {col}, {color:?}: the letter and the bar over it"
        );
        assert!(
            shows_letter(&got, lead, cells, rgb(FG)),
            "column {col}: the letter shows"
        );
    }
}

// A blinking bar keeps its letter in both phases of its blink: with the
// bar shown, the letter lies under it, and with the bar hidden, the cell
// draws as it does with no cursor.
// covers: cursor.style
#[test]
fn a_blinking_bar_keeps_its_letter_in_both_phases() {
    let st = style(rgb(FG), rgb(BG));
    let mut s = screen(st, 0, CursorShape::Bar, None);
    s.cursor.blink = true;
    let (want, _) = letter_under_bar(&s, 0, 1, rgb(FG));
    assert!(draw(&s, true, None) == want, "the bar shown");
    let mut plain = screen(st, 0, CursorShape::Bar, None);
    plain.cursor.visible = false;
    assert!(
        draw(&s, false, None) == draw(&plain, true, None),
        "the bar hidden"
    );
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
