//! Text blink: in the hidden phase a blinking cell, SGR 5 or 6, draws as a
//! blank cell in its own colours, except under the cursor, which shows it.

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::{Age, Attrs, CursorShape, Style};

const FG: &str = "e0e0e0";
const BG: &str = "203040";

fn draw(s: &Screen, cursor_on: bool, blink_hidden: bool) -> Canvas {
    let (w, h) = (s.cols as u32 * 8, s.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on,
        blink_hidden,
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    c
}

/// Six cells from `cells`, `(col, char, wide, style)`, with a block cursor
/// at column 1 when `cursor`.
fn screen(cells: &[(usize, char, bool, Style)], cursor: bool) -> Screen {
    let mut s = Screen::new(6, 1, rgb("c0c0c0"), rgb("101010"));
    for &(col, c, wide, st) in cells {
        s.put(col, 0, c, wide, st);
    }
    s.cursor.visible = cursor;
    s.cursor.col = 1;
    s.cursor.shape = CursorShape::Block;
    s
}

// covers: sgr.blink
#[test]
fn blinking_text_hides_as_blank_cells_but_under_the_cursor() {
    let plain = style(rgb(FG), rgb(BG));
    let mut blink = plain;
    blink.attrs.insert(Attrs::BLINK);
    let mut rapid = plain;
    rapid.attrs.insert(Attrs::RAPID_BLINK);
    let swapped = style(rgb(BG), rgb(FG));
    let s = screen(
        &[
            (0, 'a', false, blink),
            (1, 'b', false, blink),
            (2, '中', true, rapid),
            (4, 'c', false, plain),
        ],
        true,
    );

    // shown: the text as it is, the cursor's cell swapped
    let shown = screen(
        &[
            (0, 'a', false, plain),
            (1, 'b', false, swapped),
            (2, '中', true, plain),
            (4, 'c', false, plain),
        ],
        false,
    );
    assert!(draw(&s, true, false) == draw(&shown, false, false), "shown");

    // hidden: blank cells in their own colours, but the cursor's
    let blank = screen(
        &[
            (0, ' ', false, plain),
            (1, 'b', false, swapped),
            (2, ' ', false, plain),
            (3, ' ', false, plain),
            (4, 'c', false, plain),
        ],
        false,
    );
    let hidden = draw(&s, true, true);
    assert!(hidden == draw(&blank, false, false), "hidden");
    let bg = 0x20_30_40;
    for col in [0, 2, 3] {
        for y in 0..16 {
            for x in col * 8..col * 8 + 8 {
                assert_eq!(hidden.pixel(x, y), Some(bg), "column {col} at {x},{y}");
            }
        }
    }
    let lit = |c: &Canvas, col: u32| {
        (0..16)
            .flat_map(|y| (col * 8..col * 8 + 8).map(move |x| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) == Some(bg))
            .count()
    };
    assert!(lit(&hidden, 1) > 10, "the cursor's cell shows its b");

    // with the cursor off, its cell hides too
    let mut off = blank;
    off.put(1, 0, ' ', false, plain);
    assert!(
        draw(&s, false, true) == draw(&off, false, false),
        "cursor off"
    );
}
