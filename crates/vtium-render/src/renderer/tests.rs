use vtium_font::{Font, Unifont};
use vtium_term::{Age, Cursor, Damage, Frame, Glyph, Line, LineId, Palette, Rgb, Style, Width};

use super::{Config, DrawState, PartialRow, Placement, Renderer};
use crate::{Canvas, Orientation, Rect, TestInk};

/// The border's color around the grid, and the cells' ink.
const BORDER: u32 = 0x10_10_10;
const INK: u32 = 0xff_ff_ff;

/// Rows of `cols` cells, blank but for a full block at each of `at`.
fn lines(cols: usize, rows: usize, at: &[(usize, usize)]) -> Vec<Line> {
    (0..rows)
        .map(|r| {
            let mut line = Line::new(LineId::new(r as u64 + 1), cols);
            for &(c, _) in at.iter().filter(|&&(_, row)| row == r) {
                line.write(
                    c,
                    Glyph::Char('\u{2588}'),
                    Width::Narrow,
                    &Style::DEFAULT,
                    None,
                );
            }
            line
        })
        .collect()
}

/// The renderer's drawing, on a display `w` by `h` turned as `o`, of a
/// grid of `cols` by `rows` cells with a full block at each of `at`.
fn drawn(
    o: Orientation,
    size: (u32, u32),
    grid: (usize, usize),
    at: &[(usize, usize)],
) -> (Renderer<Unifont>, Canvas) {
    drawn_placed(o, Placement::default(), size, grid, at)
}

/// [`drawn`], with the grid placed as `placement` says.
fn drawn_placed(
    o: Orientation,
    placement: Placement,
    (w, h): (u32, u32),
    (cols, rows): (usize, usize),
    at: &[(usize, usize)],
) -> (Renderer<Unifont>, Canvas) {
    let config = Config {
        orientation: o,
        placement,
        ..Config::default()
    };
    let mut r = Renderer::new(Font::new(Unifont::new(16)), w, h, config);
    let mut canvas = Canvas::new(w, h, 0);
    let rows_ = lines(cols, rows, at);
    let damage = Damage::new(cols, rows);
    let palette = Palette::new(
        [Rgb::default(); 256],
        Rgb::new(0xff, 0xff, 0xff),
        Rgb::new(0x10, 0x10, 0x10),
    );
    let frame = Frame::new(
        rows_.iter().collect(),
        &damage,
        &palette,
        Age::NEVER,
        Cursor {
            visible: false,
            ..Cursor::default()
        },
        false,
    );
    let state = DrawState {
        background: Rgb::new(0x10, 0x10, 0x10),
        ..DrawState::default()
    };
    r.draw(&frame, &mut canvas, 0, &state);
    (r, canvas)
}

/// The smallest rectangle that holds every pixel of `canvas` in `ink`.
fn lit(canvas: &Canvas, ink: u32) -> Option<Rect> {
    let (w, h) = (canvas.width(), canvas.height());
    let px: Vec<(u32, u32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| canvas.pixel(x, y) == Some(ink))
        .collect();
    let x0 = px.iter().map(|p| p.0).min()?;
    let y0 = px.iter().map(|p| p.1).min()?;
    let x1 = px.iter().map(|p| p.0).max()?;
    let y1 = px.iter().map(|p| p.1).max()?;
    Some(Rect {
        x: x0,
        y: y0,
        width: x1 - x0 + 1,
        height: y1 - y0 + 1,
    })
}

/// The border's columns left and right of the lit grid, then its lines
/// above and below, on a display of `size` drawn with every cell of a
/// grid of 6 by 2 lit, placed as `placement` says.
fn margins(o: Orientation, placement: Placement, size: (u32, u32)) -> [u32; 4] {
    let (gw, gh) = (6 * 8, 2 * 16);
    let all: Vec<(usize, usize)> = (0..2).flat_map(|r| (0..6).map(move |c| (c, r))).collect();
    let (_, canvas) = drawn_placed(o, placement, size, (6, 2), &all);
    let (w, h) = size;
    let grid = lit(&canvas, INK).expect("the cells draw");
    assert_eq!(grid.area(), u64::from(gw * gh), "{o:?}: every cell lit");
    let border = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| canvas.pixel(x, y) == Some(BORDER))
        .count();
    assert_eq!(border as u32, w * h - gw * gh, "{o:?}: the border");
    [
        grid.x,
        w - grid.x - grid.width,
        grid.y,
        h - grid.y - grid.height,
    ]
}

// With smooth scrolling on, at rest, the grid's last row touches the
// text's bottom edge, so the spare lines lie above its first row.
// Across the rows the grid stays centred, with an odd spare pixel on
// the right or the bottom of the display. A grid of 6 by 2 cells of 8
// by 16 pixels leaves 3 spare lines and 5 spare columns, which the
// display turned on its side swaps.
//
// References:
// - ADR-1310 D10: the grid flush to the text's bottom edge while smooth scrolling is on, and centred across the rows.
#[test]
fn the_grid_at_rest_touches_the_text_s_bottom_edge() {
    let (gw, gh) = (6 * 8, 2 * 16);
    for partial in [PartialRow::History, PartialRow::Blank, PartialRow::Fade] {
        for (o, size, want) in [
            (Orientation::Normal, (gw + 5, gh + 3), [2, 3, 3, 0]),
            (Orientation::UpsideDown, (gw + 5, gh + 3), [2, 3, 0, 3]),
            (Orientation::Right, (gh + 3, gw + 5), [0, 3, 2, 3]),
            (Orientation::Left, (gh + 3, gw + 5), [3, 0, 2, 3]),
        ] {
            let got = margins(o, Placement::Bottom(partial), size);
            assert_eq!(got, want, "{o:?} {partial:?}: left, right, top, bottom");
        }
    }
}

// With smooth scrolling off, the default, the grid is centred on both
// axes in the display's own pixels: of s spare lines the top gets
// half, rounded down, and the bottom the rest, and the spare columns
// split the same way. At every turn the odd pixel lies on the
// display's right or bottom edge, and every spare line and column
// shows the border's color.
//
// References:
// - ADR-1310 D12, "With smooth scrolling off": the grid centred on both axes in the display's own pixels.
#[test]
fn the_grid_with_smooth_scrolling_off_is_centred_on_both_axes() {
    let (gw, gh) = (6 * 8, 2 * 16);
    assert_eq!(Config::default().placement, Placement::Centred);
    for (o, size, want) in [
        (Orientation::Normal, (gw + 5, gh + 3), [2, 3, 1, 2]),
        (Orientation::UpsideDown, (gw + 5, gh + 3), [2, 3, 1, 2]),
        (Orientation::Right, (gh + 3, gw + 5), [1, 2, 2, 3]),
        (Orientation::Left, (gh + 3, gw + 5), [1, 2, 2, 3]),
    ] {
        assert_eq!(
            margins(o, Placement::Centred, size),
            want,
            "{o:?}: left, right, top, bottom"
        );
    }
}

// grid_origin names the pixel, as the text reads, where the drawn grid
// starts, at every turn and in both placements, with an odd margin on
// both axes. The login picture is fitted to this origin, so a picture
// on a display turned left with an odd margin starts on the pixel
// where the renderer draws the grid's first cell.
//
// References:
// - ADR-1310 D10 and D12: where the grid sits with smooth scrolling on and off.
// - ADR-1100 D17: the login picture fitted to the grid on the display.
#[test]
fn grid_origin_is_where_the_grid_is_drawn_as_the_text_reads() {
    use super::grid_origin;
    let (gw, gh) = (6 * 8, 2 * 16);
    for placement in [Placement::Centred, Placement::Bottom(PartialRow::History)] {
        for o in [
            Orientation::Normal,
            Orientation::UpsideDown,
            Orientation::Right,
            Orientation::Left,
        ] {
            let size = match o.turned() {
                true => (gh + 3, gw + 5),
                false => (gw + 5, gh + 3),
            };
            let [left, right, top, bottom] = margins(o, placement, size);
            let drawn = match o {
                Orientation::Normal => (left, top),
                Orientation::UpsideDown => (right, bottom),
                Orientation::Right => (top, right),
                Orientation::Left => (bottom, left),
            };
            assert_eq!(
                grid_origin(placement, o, size, (gw, gh)),
                drawn,
                "{placement:?}, {o:?}"
            );
        }
    }
    // turned left and centred, 3 spare columns and 5 spare lines: the
    // display's odd line lies at its bottom, which is the text's left
    let left = grid_origin(
        Placement::Centred,
        Orientation::Left,
        (gh + 3, gw + 5),
        (gw, gh),
    );
    assert_eq!(left, (3, 1));
}

// A display 4,320 lines high with rows 41 pixels high holds 105 rows
// and leaves 15 spare lines. With smooth scrolling off, 7 lie above
// the grid and 8 below, at every turn.
//
// References:
// - ADR-1310 D12, "With smooth scrolling off": the example of the 4,320-line display.
#[test]
fn a_4320_line_display_of_41_pixel_rows_shows_7_lines_above_and_8_below() {
    for o in [
        Orientation::Normal,
        Orientation::UpsideDown,
        Orientation::Right,
        Orientation::Left,
    ] {
        let (along, across) = (4320u32, 64u32);
        let (w, h) = match o.turned() {
            true => (along, across),
            false => (across, along),
        };
        let config = Config {
            orientation: o,
            ..Config::default()
        };
        let mut r = Renderer::new(Font::new(Unifont::new(16)), w, h, config);
        r.layout.ch = 41;
        r.layout.max_rows = 105;
        let g = r.layout.grid_of(8, 105);
        let (lo, hi) = match o.turned() {
            true => (g.off_x, i64::from(w) - g.max_x),
            false => (g.off_y, i64::from(h) - g.max_y),
        };
        assert_eq!(
            (lo, hi),
            (7, 8),
            "{o:?}: the display's own top or left, then bottom or right"
        );
    }
}

// The renderer's rectangle of a cell is where a frame of the grid's
// size draws that cell, at every turn, with spare lines and columns
// left over; a cell outside the grid has none.
#[test]
fn a_cell_s_rectangle_is_where_the_frame_draws_it() {
    for (o, size) in [
        (Orientation::Normal, (6 * 8 + 5, 2 * 16 + 3)),
        (Orientation::UpsideDown, (6 * 8 + 5, 2 * 16 + 3)),
        (Orientation::Right, (2 * 16 + 3, 6 * 8 + 5)),
        (Orientation::Left, (2 * 16 + 3, 6 * 8 + 5)),
    ] {
        for at in [(0, 0), (5, 0), (0, 1), (3, 1)] {
            let (r, canvas) = drawn(o, size, (6, 2), &[at]);
            assert_eq!(
                r.cell_rect((6, 2), at.0, at.1),
                lit(&canvas, INK),
                "{o:?} {at:?}"
            );
        }
        let (r, _) = drawn(o, size, (6, 2), &[]);
        assert_eq!(
            r.cell_rect((6, 2), 6, 0),
            None,
            "{o:?}: past the last column"
        );
        assert_eq!(r.cell_rect((6, 2), 0, 2), None, "{o:?}: past the last row");
        assert_eq!(r.cell_rect((0, 2), 0, 0), None, "{o:?}: an empty grid");
    }
}

/// A terminal of 6 by 4 cells that ran `bytes`, and its screen drawn on
/// a display with 7 spare lines above the grid, with the rows of
/// history above the screen that a build with `scroll-pan` asks for.
fn spare_lines_drawn(bytes: &[u8]) -> Canvas {
    spare_lines_placed(bytes, BOTTOM)
}

/// The grid placed at the bottom, with the rows of history in the
/// spare lines: smooth scrolling on, under `partial-row`'s default.
const BOTTOM: Placement = Placement::Bottom(PartialRow::History);

/// [`spare_lines_drawn`], with the grid placed as `placement` says.
fn spare_lines_placed(bytes: &[u8], placement: Placement) -> Canvas {
    let mut t = vtium_term::Terminal::new(6, 4, 1000);
    t.screen_mut().set_scrollback_max(100);
    t.input(bytes);
    let (w, h) = (6 * 8, 4 * 16 + 7);
    let config = Config {
        placement,
        ..Config::default()
    };
    let mut r = Renderer::new(Font::new(Unifont::new(16)), w, h, config);
    // a glyph whose ink reaches 6 pixels into the rows above and below
    let pixels = [255u8; 2 * 28];
    r.keep_glyph_for_test(
        TALL,
        TestInk {
            left: 3,
            top: -6,
            width: 2,
            height: 28,
            pixels: &pixels,
        },
    );
    let palette = t.palette().clone();
    #[cfg(feature = "scroll-pan")]
    let frame = {
        let reach = vtium_term::Reach {
            above: r.rows_above_asked((6, 4)),
            ..vtium_term::Reach::default()
        };
        t.screen().frame_reaching(&palette, Age::NEVER, reach)
    };
    #[cfg(not(feature = "scroll-pan"))]
    let frame = t.screen().frame(&palette, Age::NEVER);
    let state = DrawState {
        background: palette.bg(),
        ..DrawState::default()
    };
    let mut c = Canvas::new(w, h, 0x00ab_cdef);
    r.draw(&frame, &mut c, 0, &state);
    c
}

// The spare lines above the grid show the bottom of the row of history
// above the screen in a build with `scroll-pan`, drawn as that line
// draws on the screen beside the same neighbours; a build without it
// shows the border's color there.
//
// References:
// - ADR-1310 D10, "Every build": a build without `scroll-pan` keeps no row above, so its spare lines show the border's color.
#[test]
fn the_spare_lines_show_the_row_above_or_the_border() {
    // a red line, then four more, so the red line lies above the screen
    let red = b"\x1b[41mAAAAAA\x1b[0m\r\nB";
    let history = spare_lines_drawn(&[red.as_slice(), b"\r\nC\r\nD\r\nE"].concat());
    let strip: Vec<u32> = (0..7)
        .flat_map(|y| (0..48).map(move |x| (x, y)))
        .map(|(x, y)| history.pixel(x, y).unwrap_or_default())
        .collect();
    #[cfg(feature = "scroll-pan")]
    {
        // the red line on the screen's first row, with the same line
        // below it: its last 7 lines, from the grid's 16th line
        let screen = spare_lines_drawn(red);
        let want: Vec<u32> = (16..23)
            .flat_map(|y| (0..48).map(move |x| (x, y)))
            .map(|(x, y)| screen.pixel(x, y).unwrap_or_default())
            .collect();
        assert_eq!(strip, want, "the spare lines show the red line's bottom");
    }
    #[cfg(not(feature = "scroll-pan"))]
    {
        let bg = vtium_term::Terminal::new(6, 4, 0).palette().bg();
        let border = crate::blend::pixel(bg);
        assert!(
            strip.iter().all(|&p| p == border),
            "the spare lines show the border's color"
        );
    }
}

/// A glyph whose ink reaches 6 pixels into the rows above and below.
const TALL: char = '\u{a6}';

// With smooth scrolling off, a centred grid draws no row of history:
// the spare lines show the border's color, and a history row whose ink
// reaches down leaves none in the screen's top row, so the display
// equals the one a terminal with no history draws. With smooth
// scrolling on, the same ink shows in the screen's top row.
//
// References:
// - ADR-1310 D10, "Ink at the top edge": ink from above only while smooth scrolling is on.
// - ADR-1310 D12, "With smooth scrolling off": the border's color in every spare line.
#[test]
fn a_centred_grid_takes_no_ink_from_the_row_above() {
    let tall: String = core::iter::repeat_n(TALL, 6).collect();
    let history = format!("{tall}\r\nB\r\nC\r\nD\r\nE");
    let alone = b"B\r\nC\r\nD\r\nE";
    let centred = spare_lines_placed(history.as_bytes(), Placement::Centred);
    assert!(
        centred == spare_lines_placed(alone, Placement::Centred),
        "the history row leaves no pixel on a centred display"
    );
    let bg = vtium_term::Terminal::new(6, 4, 0).palette().bg();
    let border = crate::blend::pixel(bg);
    for y in (0..3).chain(4 * 16 + 3..4 * 16 + 7) {
        for x in 0..48 {
            assert_eq!(centred.pixel(x, y), Some(border), "the bar at {x}, {y}");
        }
    }
    #[cfg(feature = "scroll-pan")]
    {
        let bottom = spare_lines_placed(history.as_bytes(), BOTTOM);
        let top_row = |c: &Canvas| -> Vec<u32> {
            (7..13)
                .flat_map(|y| (0..48).map(move |x| (x, y)))
                .map(|(x, y)| c.pixel(x, y).unwrap_or_default())
                .collect()
        };
        assert_ne!(
            top_row(&bottom),
            top_row(&spare_lines_placed(alone, BOTTOM)),
            "with smooth scrolling on, the row above sends its ink into the top row"
        );
    }
}

// With smooth scrolling on, `partial-row` sets what the 7 spare lines
// above the grid show at rest. Under `blank` they show the border's
// color. Under `fade` the line at the screen's edge shows the border's
// color and the line next to the grid's first row shows the row
// above's own pixels, with the lines between mixed from one toward the
// other. The grid itself draws the same pixels under every value. A
// build without `scroll-pan` shows the border's color under each one.
//
// References:
// - ADR-1310 D12, "The key `partial-row`": the three values.
// - ADR-1310 D12, "In a build without `scroll-pan`": the border's color under every value.
#[test]
fn the_spare_lines_blank_and_fade_as_partial_row_says() {
    let red = b"\x1b[41mAAAAAA\x1b[0m\r\nB\r\nC\r\nD\r\nE";
    let drawn = |p| spare_lines_placed(red, Placement::Bottom(p));
    let (history, blank, fade) = (
        drawn(PartialRow::History),
        drawn(PartialRow::Blank),
        drawn(PartialRow::Fade),
    );
    let line = |c: &Canvas, y: u32| -> Vec<u32> {
        (0..48).map(|x| c.pixel(x, y).unwrap_or_default()).collect()
    };
    let bg = vtium_term::Terminal::new(6, 4, 0).palette().bg();
    let border = vec![crate::blend::pixel(bg); 48];
    for y in 7..4 * 16 + 7 {
        assert_eq!(
            line(&blank, y),
            line(&history, y),
            "blank: the grid's line {y}"
        );
        assert_eq!(
            line(&fade, y),
            line(&history, y),
            "fade: the grid's line {y}"
        );
    }
    for y in 0..7 {
        assert_eq!(line(&blank, y), border, "blank: the spare line {y}");
    }
    assert_eq!(
        line(&fade, 0),
        border,
        "fade: the line at the screen's edge"
    );
    #[cfg(feature = "scroll-pan")]
    {
        assert_ne!(
            line(&history, 6),
            border,
            "the red row shows in the spare lines"
        );
        assert_eq!(
            line(&fade, 6),
            line(&history, 6),
            "fade: the line next to the grid"
        );
        let (mid, own) = (line(&fade, 3), line(&history, 3));
        assert!(
            mid != own && mid != border,
            "fade: a line between mixes the two"
        );
    }
    #[cfg(not(feature = "scroll-pan"))]
    for y in 0..7 {
        assert_eq!(line(&history, y), border, "history: the spare line {y}");
        assert_eq!(line(&fade, y), border, "fade: the spare line {y}");
    }
}

// A row of history above the screen joins its letters as a row of the
// screen does: in the spare lines, a lam-alef pair draws its ligature
// and an Arabic word draws its joined forms, as the same line draws on
// the screen's first row beside the same neighbours.
//
// References:
// - ADR-1310 D10, "Every build": every way of drawing shows the same pixels in the spare lines.
// - ADR-1004 D6 and D7: the lam-alef ligature and the joined forms.
#[cfg(all(feature = "scroll-pan", feature = "bidi"))]
#[test]
fn the_spare_lines_join_the_letters_of_the_row_above() {
    let line = "\u{644}\u{627} \u{628}\u{64a}\u{62a}";
    let screen = spare_lines_drawn(line.as_bytes());
    let history = spare_lines_drawn(format!("{line}\r\nB\r\nC\r\nD\r\nE").as_bytes());
    let rows = |c: &Canvas, from: u32| -> Vec<u32> {
        (from..from + 7)
            .flat_map(|y| (0..48).map(move |x| (x, y)))
            .map(|(x, y)| c.pixel(x, y).unwrap_or_default())
            .collect()
    };
    let border = rows(&spare_lines_drawn(b""), 0);
    let want = rows(&screen, 16);
    assert_ne!(want, border, "the line's bottom shows ink");
    assert_eq!(rows(&history, 0), want, "the spare lines join the letters");
}

// When the scrollback is cleared, the rows of history above the screen
// go, and the next frame drawn into the same buffer puts the border's
// color back where they were, as a frame drawn afresh shows: on a
// display whose spare lines are taller than a row, no history pixel
// stays in them.
//
// References:
// - ADR-1310 D10, "A strip taller than a row": the border's color shows only above the oldest line.
#[cfg(feature = "scroll-pan")]
#[test]
fn clearing_the_scrollback_puts_the_border_back_in_the_spare_lines() {
    let mut t = vtium_term::Terminal::new(6, 4, 1000);
    t.screen_mut().set_scrollback_max(100);
    for i in 0..20 {
        t.input(format!("\x1b[4{}m{i:02}xxxx\x1b[0m\r\n", 1 + i % 6).as_bytes());
    }
    // 4 rows and 7 lines of spare lines above the grid's 4 rows
    let (w, h) = (6 * 8, 8 * 16 + 7);
    let strip = h - 4 * 16;
    let palette = t.palette().clone();
    let state = DrawState {
        background: palette.bg(),
        ..DrawState::default()
    };
    let draw = |t: &vtium_term::Terminal, r: &mut Renderer<Unifont>, c: &mut Canvas, since, age| {
        let reach = vtium_term::Reach {
            above: r.rows_above_asked((6, 4)),
            ..vtium_term::Reach::default()
        };
        let frame = t.screen().frame_reaching(&palette, since, reach);
        r.draw(&frame, c, age, &state);
    };
    let config = Config {
        placement: BOTTOM,
        ..Config::default()
    };
    let mut r = Renderer::new(Font::new(Unifont::new(16)), w, h, config);
    let mut kept = Canvas::new(w, h, 0x00ab_cdef);
    draw(&t, &mut r, &mut kept, Age::NEVER, 0);
    let border = crate::blend::pixel(palette.bg());
    let lit = |c: &Canvas| {
        (0..strip)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) != Some(border))
            .count()
    };
    assert!(lit(&kept) > 0, "the spare lines show the history");
    let since = t.screen_mut().checkpoint();
    t.input(b"\x1b[3J");
    draw(&t, &mut r, &mut kept, since, 1);
    let mut fresh = Canvas::new(w, h, 0x00ab_cdef);
    let mut afresh = Renderer::new(Font::new(Unifont::new(16)), w, h, config);
    draw(&t, &mut afresh, &mut fresh, Age::NEVER, 0);
    assert_eq!(lit(&fresh), 0, "a frame drawn afresh shows the border");
    assert_eq!(lit(&kept), 0, "no history pixel stays in the spare lines");
    assert!(
        kept == fresh,
        "the buffer shows what a frame drawn afresh shows"
    );
}
