use super::*;
use crate::{Canvas, Config, DrawState, Renderer};
use vtium_font::set::{SetFace, builtin};
use vtium_term::{Age, Cursor, Damage, Frame, Glyph, Line, LineId, Palette};

const FIGURE: &str =
    "vtium: heads.conf:5: rule names undefined head \"desc\"; running the shipped rules";

fn font(name: &str, height: u32) -> Font<SetFace> {
    let set = builtin(name).expect("a built-in set");
    Font::new(SetFace::new(set, height).expect("a cell"))
}

fn display(width: u32, height: u32) -> Surface {
    Surface {
        width,
        height,
        orientation: Orientation::Normal,
        depth: Depth::Color,
    }
}

/// Rows of `cols` cells of `c`.
fn lines(cols: usize, rows: usize, c: char) -> Vec<Line> {
    (0..rows)
        .map(|r| {
            let mut line = Line::new(LineId::new(r as u64 + 1), cols);
            for col in 0..cols {
                line.write(
                    col,
                    Glyph::Char(c),
                    vtium_term::Width::Narrow,
                    &vtium_term::Style::DEFAULT,
                    None,
                );
            }
            line
        })
        .collect()
}

fn frame<'a>(rows: &'a [Line], damage: &'a Damage, palette: &'a Palette, since: Age) -> Frame<'a> {
    Frame::new(
        rows.iter().collect(),
        damage,
        palette,
        since,
        Cursor::default(),
        false,
    )
}

fn palette() -> Palette {
    Palette::new(
        [Rgb::default(); 256],
        Rgb::new(200, 200, 200),
        Rgb::new(0, 0, 0),
    )
}

// ADR-1401 D7: the notice is vtium's own, drawn over the display's
// frame and never through the terminal's cells
#[test]
fn a_notice_draws_above_the_grid_and_leaves_the_terminals_cells_unchanged() {
    let (w, h) = (320, 240);
    let rows = lines(40, 30, 'x');
    let before = rows.clone();
    let mut damage = Damage::new(40, 30);
    let palette = palette();
    let mut r = Renderer::new(font("unscii-8", 8), w, h, Config::default());
    let mut canvas = Canvas::new(w, h, 0);
    r.draw(
        &frame(&rows, &damage, &palette, Age::NEVER),
        &mut canvas,
        0,
        &DrawState::default(),
    );
    let since = damage.checkpoint();
    let grid = canvas.clone();

    let area = Notice::new(FIGURE)
        .draw(
            &font("unscii-8", 8),
            display(w, h),
            Colors::NOTICE,
            None,
            &mut canvas,
        )
        .expect("a notice fits 320x240");
    assert_eq!((area.x, area.y, area.width), (0, 0, w));
    assert!(area.height > 0 && area.height < h, "{area:?}");
    let c = Colors::NOTICE;
    let drawn = [pixel(c.ink), pixel(c.paper), pixel(c.edge)];
    for y in 0..h {
        for x in 0..w {
            let now = canvas.pixel(x, y).expect("inside");
            if y < area.height {
                assert!(drawn.contains(&now), "the box at {x},{y}: {now:06x}");
            } else {
                assert_eq!(Some(now), grid.pixel(x, y), "outside the box at {x},{y}");
            }
        }
    }
    assert_eq!(canvas.pixel(0, 0), Some(pixel(c.edge)));
    assert_eq!(rows, before, "the terminal's cells");
    // the renderer's own record of the cells is untouched: a frame
    // that changes nothing draws nothing, and the box stays
    let shown = canvas.clone();
    let drawn = r.draw(
        &frame(&rows, &damage, &palette, since),
        &mut canvas,
        1,
        &DrawState::default(),
    );
    assert!(drawn.damage.is_empty(), "{:?}", drawn.damage);
    assert_eq!(canvas, shown);
}

fn text(lines: &[Vec<(char, Width)>]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.iter().map(|&(c, _)| c).collect())
        .collect()
}

fn narrow(_: char) -> Width {
    Width::Narrow
}

#[test]
fn a_long_message_wraps_at_its_spaces_to_the_box() {
    // 38 cells: 320 pixels of 8, less the edge and half a cell each side
    assert_eq!(
        text(&wrap(FIGURE, usize::MAX, |_| 38, narrow)),
        [
            "vtium: heads.conf:5: rule names",
            "undefined head \"desc\"; running the",
            "shipped rules",
        ]
    );
    assert_eq!(
        text(&wrap("one\n\ntwo", usize::MAX, |_| 8, narrow)),
        ["one", "", "two"]
    );
    // a continuation line starts at its first word: a run of spaces at
    // the break goes with it
    assert_eq!(
        text(&wrap("abcd  efgh", usize::MAX, |_| 4, narrow)),
        ["abcd", "efgh"]
    );
    assert_eq!(
        text(&wrap("ab   cd", usize::MAX, |_| 3, narrow)),
        ["ab", "cd"]
    );
    // a break on the last space leaves no empty line after it
    assert_eq!(
        text(&wrap("abcd efgh ", usize::MAX, |_| 4, narrow)),
        ["abcd", "efgh"]
    );
}

#[test]
fn a_word_longer_than_the_box_breaks_inside_it() {
    assert_eq!(
        text(&wrap("abcdefghij kl", usize::MAX, |_| 4, narrow)),
        ["abcd", "efgh", "ij", "kl"]
    );
    // a wide character that does not fit goes to the next line, and one
    // wider than the box takes a line of its own
    let wide = |c: char| {
        if c == '\u{6f22}' {
            Width::Wide
        } else {
            Width::Narrow
        }
    };
    assert_eq!(
        text(&wrap("ab\u{6f22}", usize::MAX, |_| 3, wide)),
        ["ab", "\u{6f22}"]
    );
    assert_eq!(
        text(&wrap("\u{6f22}a", usize::MAX, |_| 1, wide)),
        ["\u{6f22}", "a"]
    );
}

/// The pixels of the `w` by `h` block at `x`, `y`.
fn block(c: &Canvas, x: u32, y: u32, w: u32, h: u32) -> Vec<u32> {
    (y..y + h)
        .flat_map(|y| (x..x + w).map(move |x| (x, y)))
        .map(|(x, y)| c.pixel(x, y).expect("inside"))
        .collect()
}

// legible down to minspec's floor: each line in the built-in font's
// glyphs, one a cell, in the box's colors
#[test]
fn the_notice_draws_each_line_in_the_fonts_glyphs() {
    for (w, h, name, ch, lines) in [(320, 240, "unscii-8", 8, 3), (640, 480, "unifont", 16, 2)] {
        let font = font(name, ch);
        let mut canvas = Canvas::new(w, h, 0);
        let area = Notice::new(FIGURE)
            .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
            .expect("it fits");
        let (edge, cw) = ((ch / 8).max(1), 8);
        let inset = (edge + cw / 2, edge + ch / 4);
        assert_eq!(area.height, 2 * inset.1 + lines * ch, "{name}");
        let cols = (w - 2 * inset.0) / cw;
        let c = Colors::NOTICE;
        for (row, line) in wrap(FIGURE, usize::MAX, |_| cols, narrow)
            .iter()
            .enumerate()
        {
            for (col, &(ch_, _)) in line.iter().enumerate() {
                let g = font
                    .glyph(&[ch_], Width::Narrow, Style::default())
                    .expect("a glyph");
                let want: Vec<u32> = g
                    .pixels()
                    .iter()
                    .map(|&a| mask(c.ink, c.paper, a))
                    .collect();
                let (x, y) = (inset.0 + col as u32 * cw, inset.1 + row as u32 * ch);
                assert_eq!(block(&canvas, x, y, cw, ch), want, "{name} {ch_:?}");
            }
        }
    }
}

/// A character the built-in font lacks draws as a hollow box as wide
/// as its cells: a wide one across two cells, a narrow one in one,
/// beside letters drawn from the font.
///
/// References:
/// - ADR-1204 D1: the hollow box for a character the built-in font
///   lacks.
#[test]
fn a_character_the_built_in_font_lacks_draws_as_a_hollow_box() {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let font = Font::new(vtium_font::set::own::face(cell).expect("a strike"));
    let (w, h) = (320, 240);
    let mut canvas = Canvas::new(w, h, 0);
    Notice::new("a\u{4e2d}\u{21d4}b")
        .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
        .expect("it fits");
    let c = Colors::NOTICE;
    let inset = (2 + 4, 2 + 4);
    let at = |col: u32, glyph: &vtium_font::Glyph| {
        let want: Vec<u32> = glyph
            .pixels()
            .iter()
            .map(|&a| mask(c.ink, c.paper, a))
            .collect();
        let x = inset.0 + col * 8;
        assert_eq!(
            block(&canvas, x, inset.1, glyph.width(), 16),
            want,
            "column {col}"
        );
    };
    let letter = |ch| {
        font.glyph(&[ch], Width::Narrow, Style::default())
            .expect("a glyph")
    };
    let boxed = |w| hollow_box(cell, w).expect("a box");
    at(0, &letter('a'));
    at(1, &boxed(Width::Wide));
    at(3, &boxed(Width::Narrow));
    at(4, &letter('b'));
}

/// Under a policy that draws box drawing from the font, a box-drawing
/// character the built-in font lacks still draws as its sprite, not
/// as a hollow box: Terminus has no U+2504, the dashed line.
///
/// References:
/// - ADR-1204 D1: the sprites draw the characters Terminus lacks.
#[test]
fn a_box_drawing_character_the_font_lacks_draws_as_its_sprite() {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let face = vtium_font::set::own::face(cell).expect("a strike");
    let policy = vtium_font::SpritePolicy::all(vtium_font::Source::Font);
    let font = Font::new(face).with_policy(policy);
    assert!(!font.has_glyph('\u{2504}'), "Terminus lacks U+2504");
    let (w, h) = (320, 240);
    let mut canvas = Canvas::new(w, h, 0);
    Notice::new("\u{2504}")
        .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
        .expect("it fits");
    let c = Colors::NOTICE;
    let thick = vtium_font::sprite::light_line(16);
    let sprite = vtium_font::sprite::draw('\u{2504}', 8, 16, thick).expect("a sprite");
    let want: Vec<u32> = sprite
        .pixels()
        .iter()
        .map(|&a| mask(c.ink, c.paper, a))
        .collect();
    assert_eq!(block(&canvas, 2 + 4, 2 + 4, 8, 16), want);
}

/// Every character the font lacks and draws as a narrow box shares one
/// box, and every wide one shares another, so a notice quoting many
/// such characters draws two boxes at most.
#[test]
fn the_hollow_boxes_are_drawn_once() {
    let cell = vtium_font::Cell {
        width: 8,
        height: 16,
    };
    let font = Font::new(vtium_font::set::own::face(cell).expect("a strike"));
    let glyphs = Glyphs::new(&font, Depth::default(), "\u{4e2d}\u{6587}\u{21d4}\u{21d2}a");
    let same = |a, b| match (glyphs.glyph(a), glyphs.glyph(b)) {
        (Some(x), Some(y)) => std::ptr::eq(x, y),
        _ => false,
    };
    assert!(same('\u{4e2d}', '\u{6587}'), "the wide box");
    assert!(same('\u{21d4}', '\u{21d2}'), "the narrow box");
    assert!(!same('\u{4e2d}', '\u{21d4}'), "a wide box and a narrow one");
    assert_eq!(glyphs.width('\u{4e2d}'), Width::Wide);
}

#[test]
fn a_notice_taller_than_the_display_is_cut_to_whole_lines() {
    let mut canvas = Canvas::new(320, 240, 0);
    let area = Notice::new("x\n".repeat(100))
        .draw(
            &font("unscii-8", 8),
            display(320, 240),
            Colors::NOTICE,
            None,
            &mut canvas,
        )
        .expect("it fits");
    // 29 lines of 8 between the edges and the quarter-cell insets
    assert_eq!(area.height, 6 + 29 * 8);
    // a display that holds no line draws nothing
    let mut small = Canvas::new(16, 8, 7);
    let none = Notice::new("x").draw(
        &font("unscii-8", 8),
        display(16, 8),
        Colors::NOTICE,
        None,
        &mut small,
    );
    assert_eq!(none, None);
    assert!(small.pixels().iter().all(|&p| p == 7));
}

fn report(happened: &[&str], items: &[&str], more: usize) -> NoticeReport {
    NoticeReport {
        happened: happened.iter().map(|s| (*s).to_owned()).collect(),
        items: items.iter().map(|s| (*s).to_owned()).collect(),
        more,
        label: "More:".to_owned(),
        next: "next step".to_owned(),
        drawing: None,
    }
}

/// `report` laid out in `cols` columns and `rows` lines of a font of
/// 8x16 cells whose characters are each one cell wide.
fn laid(report: &NoticeReport, cols: u32, rows: usize) -> Laid {
    arrange(report, Room { cols, rows }, (8, 16), |_| cols, narrow)
}

/// A stand-in for the panic screen's drawing, the sizes of the
/// maintainer's: a full one of 32 columns by 20 rows, and a small one of
/// 21 by 10.
fn drawing() -> Drawing {
    let art = |cols: usize, rows: usize, c: char| -> &'static str {
        let line: String = std::iter::repeat_n(c, cols).collect();
        String::leak(format!("{line}\n").repeat(rows))
    };
    Drawing::text(art(32, 20, 'F'), art(21, 10, 'S'))
}

/// The panic screen's text, in the console's words, with its file's
/// first `errors` of three.
fn panic_report(errors: usize, drawing: Option<Drawing>) -> NoticeReport {
    let all = [
        "/etc/vtium/vtium.conf:4: `consoles` cannot be \"0\": it takes 1 or more.",
        "/etc/vtium/vtium.conf:5: `isolated` cannot be \"maybe\". Change it to one of `yes`, `no`.",
        "/etc/vtium/vtium.conf:7: This rule sends devices to head \"k\\u{202e}x\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it.",
    ];
    NoticeReport {
        happened: vec![
            "vtium.conf has errors and counts as closed: all input is off.".to_owned(),
            "No display head can run because of the errors in /etc/vtium/vtium.conf, listed below."
                .to_owned(),
        ],
        items: all.iter().take(errors).map(|s| (*s).to_owned()).collect(),
        more: 0,
        label: "More errors:".to_owned(),
        next: "This console takes no input. For the full report, run vtium check-config /etc/vtium/vtium.conf over ssh or from other boot media.".to_owned(),
        drawing,
    }
}

/// The report's whole text wrapped to `cols` columns: every line it
/// shows with no drawing and no limit on its lines.
fn whole(report: &NoticeReport, cols: u32) -> Vec<String> {
    text(&wrap(&report.text(), usize::MAX, |_| cols, narrow))
}

// ADR-1401 D7: on an 80x25 screen, 78 columns by 24 lines inside the
// box, the full drawing goes to the left of the text, which keeps 44
// columns beside it and shows every line, its three errors among them
#[test]
fn the_full_drawing_goes_left_of_the_whole_text_on_80x25() {
    let report = panic_report(3, Some(drawing()));
    let got = laid(&report, 78, 24);
    let full = drawing().full;
    assert_eq!(
        got.drawing,
        Some(Shown {
            art: Art::Text(full),
            cells: (32, 20),
            at: (0, 0),
        })
    );
    assert_eq!(got.text, (34, 0));
    assert_eq!(text(&got.lines), whole(&report, 44));
    assert_eq!(got.rows, 20.max(got.lines.len() as u32));
    // with 39 columns left beside it, the full drawing goes above the
    // text instead where it fits, and else the small one
    let narrower = arrange(
        &report,
        Room { cols: 73, rows: 24 },
        (8, 16),
        |_| 73,
        narrow,
    );
    assert_ne!(narrower.text.0, 34, "{narrower:?}");
    assert!(
        narrower
            .drawing
            .is_none_or(|d| d.at.1 == 0 && narrower.text.0 == 0)
    );
}

// ADR-1401 D7: where the full drawing fits neither beside the text nor
// above it, the small one goes centered above the text, an empty row
// between them, as on a 40x25 screen: 38 columns by 24 lines
#[test]
fn the_small_drawing_goes_centered_above_the_text_on_40x25() {
    let report = panic_report(1, Some(drawing()));
    let got = laid(&report, 38, 24);
    assert_eq!(
        got.drawing,
        Some(Shown {
            art: Art::Text(drawing().small),
            cells: (21, 10),
            at: (8, 0),
        })
    );
    assert_eq!(got.text, (0, 11));
    assert_eq!(text(&got.lines), whole(&report, 38));
    assert_eq!(got.rows, 11 + got.lines.len() as u32);
    // a screen tall enough takes the full drawing above the text
    let tall = laid(&report, 38, 48);
    assert_eq!(
        tall.drawing.map(|d| (d.cells, d.at)),
        Some(((32, 20), (3, 0)))
    );
    assert_eq!(tall.text, (0, 21));
    assert_eq!(text(&tall.lines), whole(&report, 38));
}

// ADR-1401 D7: the text comes first. On a 40x25 screen the three
// errors leave no room for even the small drawing, and on 40x15,
// minspec's 320x240 screen in an 8x16 font, one error leaves none:
// each shows the text alone, exactly as a report with no drawing
#[test]
fn the_text_shows_alone_where_no_drawing_fits_beside_it() {
    for (errors, cols, rows) in [(3, 38, 24), (1, 38, 14), (3, 38, 14)] {
        let with = panic_report(errors, Some(drawing()));
        let without = panic_report(errors, None);
        let got = laid(&with, cols, rows);
        assert_eq!(got.drawing, None, "{errors} errors on {cols}x{rows}");
        assert_eq!(got.text, (0, 0));
        assert_eq!(text(&got.lines), text(&laid(&without, cols, rows).lines));
    }
    // where all of the text fits, all of it shows
    assert_eq!(
        text(&laid(&panic_report(3, Some(drawing())), 38, 24).lines),
        whole(&panic_report(3, None), 38)
    );
}

/// A stand-in sprite 80 by 80 in two colors: every pixel on its
/// diagonal red, and the rest blue.
#[cfg(feature = "images")]
fn sprite() -> Sprite {
    static PALETTE: [Rgb; 2] = [Rgb::new(0, 0, 0xff), Rgb::new(0xff, 0, 0)];
    let samples: Vec<u8> = (0..80)
        .flat_map(|y| (0..80).flat_map(move |x| if x == y { [0xff, 0, 0] } else { [0, 0, 0xff] }))
        .collect();
    Sprite::new(80, 80, &PALETTE, samples.leak())
}

/// The stand-in sprite cropped as the panic screen's is for the small
/// drawing's place: 50 by 44 pixels from its pixel at 15, 6.
#[cfg(feature = "images")]
fn monitor() -> Sprite {
    sprite().crop(15, 6, 50, 44)
}

// ADR-1401 D7: a build with image support draws the sprite in the
// place of the drawing in text, at the largest whole-number scale that
// fits the cells the text would take: in 8×16 cells, the whole sprite
// at three times its size in the full drawing's 32 by 20 cells, 30 by
// 15 cells, and the 50×44 crop at three times its size in the small
// one's 21 by 10, 19 by 9 cells
#[cfg(feature = "images")]
#[test]
fn the_sprite_takes_the_drawings_place_at_a_whole_scale() {
    let (sprite, monitor) = (sprite(), monitor());
    assert!(sprite.is_whole());
    assert!(monitor.is_whole());
    assert_eq!(monitor.size(), (50, 44));
    let with = Some(drawing().with_sprite(sprite, monitor));
    let report = panic_report(3, with);
    let got = laid(&report, 78, 24);
    assert_eq!(
        got.drawing,
        Some(Shown {
            art: Art::Sprite(sprite, 3),
            cells: (30, 15),
            at: (0, 0),
        })
    );
    assert_eq!(got.text, (32, 0));
    assert_eq!(text(&got.lines), whole(&report, 46));
    let one = panic_report(1, with);
    let got = laid(&one, 38, 24);
    assert_eq!(
        got.drawing,
        Some(Shown {
            art: Art::Sprite(monitor, 3),
            cells: (19, 9),
            at: (9, 0),
        })
    );
    assert_eq!(got.text, (0, 10));
    // too small for even the small one, the text shows alone
    assert_eq!(laid(&one, 38, 14).drawing, None);
    // in 4x6 cells the small drawing's 84x60 pixels cannot hold the
    // sprite at its own size, and the full one's 128x120 hold it once
    let tiny = arrange(
        &one,
        Room {
            cols: 200,
            rows: 100,
        },
        (4, 6),
        |_| 200,
        narrow,
    );
    assert_eq!(
        tiny.drawing.map(|d| (d.art, d.cells)),
        Some((Art::Sprite(sprite, 1), (20, 14)))
    );
    let none = Drawing::text("", "").with_sprite(sprite, monitor);
    assert_eq!(none.art(false, (8, 16)), None);
}

// a crop is the rectangle of its sprite's pixels it names, and a
// crop that runs past the sprite's edge is cut there
#[cfg(feature = "images")]
#[test]
fn a_crop_draws_its_rectangle_of_the_sprite() {
    let (sprite, monitor) = (sprite(), monitor());
    for (x, y) in [(0, 0), (49, 0), (0, 43), (49, 43), (0, 9), (34, 43)] {
        assert_eq!(monitor.get(x, y), sprite.get(x + 15, y + 6), "{x},{y}");
    }
    assert_eq!(monitor.get(50, 0), None);
    assert_eq!(monitor.get(0, 44), None);
    let past = sprite.crop(70, 75, 20, 20);
    assert_eq!(past.size(), (10, 5));
    assert!(past.is_whole());
    assert_eq!(past.get(9, 4), sprite.get(79, 79));
    assert_eq!(sprite.crop(80, 0, 4, 4).size(), (0, 4));
    assert_eq!(monitor.crop(1, 2, 3, 4).get(0, 0), sprite.get(16, 8));
}

// ADR-1401 D7: on 40×25 with one error, the small drawing's place
// draws the sprite cropped to its monitor, each pixel a 3×3 square,
// centered in its 19 by 9 cells; on 80×25 the full one's place draws
// the whole sprite (the test after this one)
#[cfg(feature = "images")]
#[test]
fn the_small_place_draws_the_cropped_sprite() {
    let (w, h) = (320, 400);
    let font = font("unifont", 16);
    let mut canvas = Canvas::new(w, h, 0);
    let (sprite, monitor) = (sprite(), monitor());
    Notice::report(panic_report(
        1,
        Some(drawing().with_sprite(sprite, monitor)),
    ))
    .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
    .expect("it fits");
    // the box's inset of 6 pixels, then column 9 of 8 pixels; 19 by 9
    // cells hold 152×144 pixels, so 1 pixel across and 6 down
    let (x0, y0) = (6 + 9 * 8 + 1, 6 + 6);
    // the stand-in's diagonal crosses the crop at 0,9, which the whole
    // sprite has blue
    assert_ne!(monitor.get(0, 9), sprite.get(0, 9));
    for (sx, sy) in [(0, 0), (0, 9), (1, 10), (34, 43), (49, 0), (49, 43)] {
        let want = pixel(monitor.get(sx, sy).expect("a pixel"));
        assert_eq!(
            block(&canvas, x0 + sx * 3, y0 + sy * 3, 3, 3),
            vec![want; 9],
            "{sx},{sy}"
        );
    }
    // the paper around it is the box's
    for (x, y) in [(x0 - 1, y0), (x0 + 150, y0), (x0, y0 - 1), (x0, y0 + 132)] {
        assert_eq!(
            canvas.pixel(x, y),
            Some(pixel(Colors::NOTICE.paper)),
            "{x},{y}"
        );
    }
}

// ADR-1401 D7: each pixel of the sprite is a square of the scale's
// side, so it stays sharp, and the sprite is centered in its cells
#[cfg(feature = "images")]
#[test]
fn the_notice_draws_each_sprite_pixel_as_a_whole_square() {
    let (w, h) = (640, 400);
    let font = font("unifont", 16);
    let mut canvas = Canvas::new(w, h, 0);
    let sprite = sprite();
    Notice::report(panic_report(
        3,
        Some(drawing().with_sprite(sprite, monitor())),
    ))
    .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
    .expect("it fits");
    // 30 by 15 cells of 8x16 hold 240x240 pixels: no space across,
    // and 0 down
    let (x0, y0) = (2 + 4, 2 + 4);
    for (sx, sy) in [(0, 0), (1, 0), (0, 1), (40, 40), (79, 79), (79, 0)] {
        let want = pixel(sprite.get(sx, sy).expect("a pixel"));
        assert_eq!(
            block(&canvas, x0 + sx * 3, y0 + sy * 3, 3, 3),
            vec![want; 9],
            "{sx},{sy}"
        );
    }
    // the paper around it is the box's
    assert_eq!(
        canvas.pixel(x0 + 240, y0),
        Some(pixel(Colors::NOTICE.paper))
    );
    assert_eq!(
        canvas.pixel(x0, y0 + 240),
        Some(pixel(Colors::NOTICE.paper))
    );
}

// a sprite whose samples end early, or whose pixel matches no entry
// of its palette, draws nothing there and is not whole
#[cfg(feature = "images")]
#[test]
fn a_sprite_draws_only_what_its_palette_names() {
    static PALETTE: [Rgb; 1] = [Rgb::new(1, 2, 3)];
    let short = Sprite::new(2, 2, &PALETTE, &[1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2]);
    assert!(!short.is_whole());
    assert_eq!(short.get(1, 0), Some(Rgb::new(1, 2, 3)));
    assert_eq!(short.get(0, 1), Some(Rgb::new(1, 2, 3)));
    assert_eq!(short.get(1, 1), None);
    assert_eq!(short.get(2, 0), None);
    let unnamed = Sprite::new(2, 1, &PALETTE, &[1, 2, 3, 1, 2, 4]);
    assert!(!unnamed.is_whole());
    assert_eq!(unnamed.entry(0, 0), Some(0));
    assert_eq!(unnamed.get(1, 0), None);
    assert!(Sprite::new(2, 1, &PALETTE, &[1, 2, 3, 1, 2, 3]).is_whole());
    assert!(!Sprite::new(2, 1, &PALETTE, &[1, 2, 3, 1, 2, 3, 1, 2, 3]).is_whole());
}

// ADR-1401 D7: the notice draws the drawing's characters in its font,
// each in its cell, with the text beside it
#[test]
fn the_notice_draws_the_drawing_in_the_fonts_glyphs() {
    let (w, h) = (640, 400);
    let font = font("unifont", 16);
    let mut canvas = Canvas::new(w, h, 0);
    Notice::report(panic_report(3, Some(drawing())))
        .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
        .expect("it fits");
    let (cw, ch) = (8, 16);
    let inset = (2 + cw / 2, 2 + ch / 4);
    let c = Colors::NOTICE;
    let cell = |ch_: char| -> Vec<u32> {
        let g = font
            .glyph(&[ch_], Width::Narrow, Style::default())
            .expect("a glyph");
        g.pixels()
            .iter()
            .map(|&a| mask(c.ink, c.paper, a))
            .collect()
    };
    for (col, row) in [(0, 0), (31, 0), (0, 19), (31, 19)] {
        assert_eq!(
            block(&canvas, inset.0 + col * cw, inset.1 + row * ch, cw, ch),
            cell('F'),
            "{col},{row}"
        );
    }
    // the gap is paper, and the text's first letter is past it
    assert_eq!(
        block(&canvas, inset.0 + 32 * cw, inset.1, 2 * cw, ch),
        vec![pixel(c.paper); 2 * 8 * 16]
    );
    assert_eq!(
        block(&canvas, inset.0 + 34 * cw, inset.1, cw, ch),
        cell('v')
    );
}

// ADR-1401 D7: a report too tall for its display drops its items from
// the last and counts each one dropped, so what happened, the count and
// its next step still show
#[test]
fn a_report_too_tall_for_its_display_counts_the_items_it_drops() {
    let three = report(&["happened"], &["error one", "error two", "error three"], 0);
    let font = font("unscii-8", 8);
    // 4 lines of 8 between the edges and the quarter-cell insets
    let (w, h) = (320, 6 + 4 * 8);
    let drawn = |notice: Notice| {
        let mut canvas = Canvas::new(w, h, 0);
        notice
            .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
            .expect("it fits");
        canvas
    };
    // the count's digit is in no line of the report's own text
    assert_eq!(
        drawn(Notice::report(three.clone())),
        drawn(Notice::new("happened\nerror one\nMore: 2\nnext step"))
    );
    assert_eq!(
        drawn(Notice::new(three.text())),
        drawn(Notice::new("happened\nerror one\nerror two\nerror three")),
        "any other notice is cut"
    );
    let lines = |r: &NoticeReport, room| text(&laid(r, 38, room).lines);
    // with room for every item, nothing goes and nothing is counted
    assert_eq!(
        lines(&three, 9),
        [
            "happened",
            "error one",
            "error two",
            "error three",
            "next step"
        ]
    );
    // a count the report already has grows by each item dropped
    let more = report(&["happened", "why"], &["error one", "error two"], 2);
    assert_eq!(
        lines(&more, 9),
        [
            "happened",
            "why",
            "error one",
            "error two",
            "More: 2",
            "next step"
        ]
    );
    assert_eq!(
        lines(&more, 5),
        ["happened", "why", "error one", "More: 3", "next step"]
    );
    assert_eq!(lines(&more, 4), ["happened", "why", "More: 4", "next step"]);
    // a next step that wraps keeps every line of its own
    let long = NoticeReport {
        next: ["word"; 10].join(" "),
        ..three.clone()
    };
    assert_eq!(
        lines(&long, 4),
        [
            "happened",
            "More: 3",
            "word word word word word word word",
            "word word word"
        ]
    );
    // with room for less than the parts that stay, the rest is cut
    assert_eq!(lines(&three, 2), ["happened", "More: 3"]);
    // a report with no items is its parts that stay
    assert_eq!(
        lines(&report(&["happened"], &[], 0), 3),
        ["happened", "next step"]
    );
}

// the notice and the mark turn with the display, as the grid does
#[test]
fn every_orientation_draws_the_same_notice_and_mark_turned() {
    const UNTOUCHED: u32 = 0x0012_3456;
    let font = font("unscii-8", 8);
    let (w, h) = (320, 240);
    let draw = |o: Orientation, dw: u32, dh: u32| {
        let mut c = Canvas::new(dw, dh, UNTOUCHED);
        let d = Surface {
            orientation: o,
            ..display(dw, dh)
        };
        let a = Notice::new(FIGURE)
            .draw(&font, d, Colors::NOTICE, None, &mut c)
            .expect("it fits");
        // the view's top right cell, where the display draws it
        let cell = match o {
            Orientation::Normal => (312, 0),
            Orientation::UpsideDown => (0, 232),
            Orientation::Right => (232, 312),
            Orientation::Left => (0, 0),
        };
        let cell = Rect {
            x: cell.0,
            y: cell.1,
            width: 8,
            height: 8,
        };
        let b = Mark::Focus
            .draw(&font, d, cell, Colors::NOTICE, &mut c)
            .expect("it fits");
        (c, a, b)
    };
    let (normal, _, _) = draw(Orientation::Normal, w, h);
    for o in [
        Orientation::UpsideDown,
        Orientation::Right,
        Orientation::Left,
    ] {
        let (dw, dh) = if o.turned() { (h, w) } else { (w, h) };
        let (turned, a, b) = draw(o, dw, dh);
        for y in 0..h {
            for x in 0..w {
                let (tx, ty) = match o {
                    Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
                    Orientation::Right => (h - 1 - y, x),
                    Orientation::Left => (y, w - 1 - x),
                    Orientation::Normal => (x, y),
                };
                assert_eq!(turned.pixel(tx, ty), normal.pixel(x, y), "{o:?} {x},{y}");
            }
        }
        // the areas returned are exactly the pixels written
        let inside = |r: Rect, x: u32, y: u32| {
            x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
        };
        for y in 0..dh {
            for x in 0..dw {
                let written = turned.pixel(x, y) != Some(UNTOUCHED);
                assert_eq!(written, inside(a, x, y) || inside(b, x, y), "{o:?} {x},{y}");
            }
        }
    }
}

// vtium/vtium#135: at 1 bit the box is lit where its paper is light, its
// text and edge unlit, and every pixel black or white
#[test]
fn a_1_bit_display_draws_the_notice_in_black_and_white() {
    let mut canvas = Canvas::new(320, 240, 0x0080_8080);
    let d = Surface {
        depth: Depth::One,
        ..display(320, 240)
    };
    // a rounded corner and a diagonal, sprites of partial coverage
    let area = Notice::new(format!("{FIGURE} \u{256d}\u{2571}"))
        .draw(&font("unscii-8", 8), d, Colors::NOTICE, None, &mut canvas)
        .expect("it fits");
    let box_ = block(&canvas, 0, 0, area.width, area.height);
    assert!(box_.iter().all(|&p| p == 0 || p == 0x00ff_ffff));
    assert_eq!(canvas.pixel(0, 0), Some(0), "the edge");
    assert_eq!(canvas.pixel(1, 1), Some(0x00ff_ffff), "the paper");
    assert!(box_.iter().filter(|&&p| p == 0).count() > 2 * 320 + 2 * area.height as usize);
    // text on its paper's side of the cut still shows, as a cell's does
    let pale = Colors {
        ink: Rgb::new(0xc0, 0xc0, 0xc0),
        paper: Rgb::new(0xff, 0xff, 0xff),
        edge: Rgb::new(0xe0, 0xe0, 0xe0),
    };
    let mut canvas = Canvas::new(320, 240, 0);
    Notice::new("x").draw(&font("unscii-8", 8), d, pale, None, &mut canvas);
    assert_eq!(canvas.pixel(0, 0), Some(0), "the edge");
    assert!(block(&canvas, 5, 3, 8, 8).contains(&0), "the text");
}

// ADR-1401 D5: every head shows whether it holds the focus, filled where
// it does and hollow where it does not, legible at 8 pixels
#[test]
fn the_mark_shows_whether_its_head_holds_the_focus() {
    for (name, ch) in [("unscii-8", 8), ("unifont", 16), ("unifont", 32)] {
        let font = font(name, ch);
        let ink = |mark: Mark| {
            assert!(font.face().has_glyph(mark.char()), "{name} {mark:?}");
            let mut canvas = Canvas::new(320, 240, 0);
            let cell = font.cell();
            let cell = Rect {
                x: 320 - cell.width,
                y: 0,
                width: cell.width,
                height: cell.height,
            };
            let area = mark
                .draw(&font, display(320, 240), cell, Colors::NOTICE, &mut canvas)
                .expect("it fits");
            assert_eq!(area, cell);
            block(&canvas, area.x, 0, area.width, area.height)
                .iter()
                .filter(|&&p| p == pixel(Colors::NOTICE.ink))
                .count()
        };
        let (held, not) = (ink(Mark::Focus), ink(Mark::NoFocus));
        assert!(
            not > 0 && held > not,
            "{name} at {ch}: {held} against {not}"
        );
    }
}

// ADR-1401 D7: text quoted from a file is escaped first, so it cannot
// add a line to the notice or hide a character in it
#[test]
fn quote_escapes_what_a_file_could_hide() {
    assert_eq!(quote("desc"), "\"desc\"");
    assert_eq!(quote("a\nb"), "\"a\\nb\"");
    assert_eq!(quote("\u{1b}[2J"), "\"\\u{1b}[2J\"");
    assert_eq!(quote("\u{202e}csed"), "\"\\u{202e}csed\"");
    assert_eq!(quote("x\u{200b}y"), "\"x\\u{200b}y\"");
    assert_eq!(quote("say \"hi\" \\"), "\"say \\\"hi\\\" \\\\\"");
    assert_eq!(quote("caf\u{e9} \u{6f22}"), "\"caf\u{e9} \u{6f22}\"");
}

/// The renderer's drawing of `w` by `h` turned as `o`, its cells blank
/// but for a full block in the top right one, and that cell's display
/// rectangle as the renderer placed it.
fn top_right_cell(font: &Font<SetFace>, w: u32, h: u32, o: Orientation) -> (Canvas, Rect) {
    let cell = font.cell();
    let (vw, vh) = if o.turned() { (h, w) } else { (w, h) };
    let (cols, rows) = ((vw / cell.width) as usize, (vh / cell.height) as usize);
    let mut rows_ = lines(cols, rows, ' ');
    rows_[0].write(
        cols - 1,
        Glyph::Char('\u{2588}'),
        vtium_term::Width::Narrow,
        &vtium_term::Style::DEFAULT,
        None,
    );
    let damage = Damage::new(cols, rows);
    let palette = palette();
    let config = Config {
        orientation: o,
        ..Config::default()
    };
    let mut r = Renderer::new(font.clone(), w, h, config);
    let mut canvas = Canvas::new(w, h, 0);
    r.draw(
        &frame(&rows_, &damage, &palette, Age::NEVER),
        &mut canvas,
        0,
        &DrawState::default(),
    );
    let lit: Vec<(u32, u32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| canvas.pixel(x, y) != Some(0))
        .collect();
    let (x0, y0) = (
        lit.iter().map(|p| p.0).min().expect("lit"),
        lit.iter().map(|p| p.1).min().expect("lit"),
    );
    let (x1, y1) = (
        lit.iter().map(|p| p.0).max().expect("lit"),
        lit.iter().map(|p| p.1).max().expect("lit"),
    );
    let rect = Rect {
        x: x0,
        y: y0,
        width: x1 - x0 + 1,
        height: y1 - y0 + 1,
    };
    (canvas, rect)
}

// the mark takes the cell it is given, wherever the renderer placed the
// grid, and touches nothing beside it
#[test]
fn the_mark_fills_the_grid_cell_it_is_given() {
    for (name, ch, w, h, o) in [
        ("unscii-8", 8, 1366, 768, Orientation::Normal),
        ("unscii-8", 8, 1366, 768, Orientation::Right),
        ("unifont", 16, 640, 480, Orientation::Normal),
        ("unifont", 16, 1366, 768, Orientation::UpsideDown),
        ("unifont", 32, 1366, 768, Orientation::Left),
    ] {
        let font = font(name, ch);
        let (grid, cell) = top_right_cell(&font, w, h, o);
        let mut canvas = grid.clone();
        let d = Surface {
            orientation: o,
            ..display(w, h)
        };
        let area = Mark::Focus
            .draw(&font, d, cell, Colors::NOTICE, &mut canvas)
            .expect("it fits");
        assert_eq!(area, cell, "{name} {w}x{h} {o:?}");
        let c = Colors::NOTICE;
        for y in 0..h {
            for x in 0..w {
                let now = canvas.pixel(x, y).expect("inside");
                let inside = x >= cell.x
                    && y >= cell.y
                    && x < cell.x + cell.width
                    && y < cell.y + cell.height;
                if inside {
                    assert!(
                        now == pixel(c.ink) || now == pixel(c.paper),
                        "{name} {w}x{h} {o:?}: the mark at {x},{y}"
                    );
                } else {
                    assert_eq!(
                        Some(now),
                        grid.pixel(x, y),
                        "{name} {w}x{h} {o:?}: beside it at {x},{y}"
                    );
                }
            }
        }
    }
}

// widths at which the notice's text could run under the mark, and a
// height whose 31 spare lines above the grid put the mark across the
// notice's first and second lines
#[test]
fn the_notice_keeps_its_text_clear_of_the_mark() {
    let filler = "m".repeat(400);
    for (name, ch, w, h) in [
        ("unscii-8", 8, 320, 240),
        ("unscii-8", 8, 1050, 480),
        ("unifont", 16, 324, 480),
        ("unifont", 16, 900, 480),
        ("unifont", 32, 600, 480),
        ("unifont", 32, 1080, 480),
        ("unifont", 32, 1080, 511),
    ] {
        let font = font(name, ch);
        let (grid, cell) = top_right_cell(&font, w, h, Orientation::Normal);
        for text in [FIGURE, filler.as_str()] {
            let mut canvas = grid.clone();
            let area = Notice::new(text)
                .draw(
                    &font,
                    display(w, h),
                    Colors::NOTICE,
                    Some(cell),
                    &mut canvas,
                )
                .expect("it fits");
            let ink = pixel(Colors::NOTICE.ink);
            let under = block(&canvas, cell.x, cell.y, cell.width, cell.height);
            assert!(
                !under.contains(&ink),
                "{name} {w}x{h}: text under the mark at {cell:?}"
            );
            let shown = block(&canvas, 0, 0, w, area.height);
            assert!(shown.contains(&ink), "{name} {w}x{h}: the text");
            // the mark, drawn last, shows whole over the box
            let mark = Mark::Focus
                .draw(&font, display(w, h), cell, Colors::NOTICE, &mut canvas)
                .expect("it fits");
            assert_eq!(mark, cell);
        }
    }
}

// the bound of quote: no control character passes, and look-alikes do
#[test]
fn quote_lets_no_control_character_through() {
    for c in ('\u{0}'..='\u{1f}')
        .chain(['\u{7f}'])
        .chain('\u{80}'..='\u{9f}')
    {
        let q = quote(&format!("a{c}b"));
        assert!(
            !q.chars().any(char::is_control),
            "{:?} gave {q:?}",
            u32::from(c)
        );
    }
    for text in ["\u{2800}", "e\u{301}", "d\u{435}sc"] {
        assert_eq!(quote(text), format!("\"{text}\""));
    }
}

/// A face with no circles: it holds `?` alone, drawn solid.
struct Bare;

impl Face for Bare {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 8,
            height: 8,
        }
    }
    fn size_step(&self) -> u32 {
        8
    }
    fn has_glyph(&self, c: char) -> bool {
        c == '?'
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
        (cluster == ['?'])
            .then(|| vtium_font::Glyph::from_pixels(8, 8, Format::Mask, vec![255; 64]))
            .flatten()
    }
}

// a font without the circles still tells the focus apart: a filled
// box where the head holds it and an empty one where it does not,
// never the font's stand-in for a character it lacks
#[test]
fn a_font_without_the_circles_draws_the_mark_as_boxes() {
    let font = Font::new(Bare);
    assert!(!font.has_glyph(Mark::Focus.char()) && !font.has_glyph(Mark::NoFocus.char()));
    let cell = Rect {
        x: 312,
        y: 0,
        width: 8,
        height: 8,
    };
    let draw = |mark: Mark| {
        let mut canvas = Canvas::new(320, 240, 0);
        mark.draw(&font, display(320, 240), cell, Colors::NOTICE, &mut canvas)
            .expect("it fits");
        block(&canvas, 312, 0, 8, 8)
            .iter()
            .map(|&p| u8::from(p == pixel(Colors::NOTICE.ink)))
            .collect::<Vec<u8>>()
    };
    #[rustfmt::skip]
    let filled = [
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
    ];
    #[rustfmt::skip]
    let empty = [
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 1, 0, 0, 1, 0, 0,
        0, 0, 1, 0, 0, 1, 0, 0,
        0, 0, 1, 1, 1, 1, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
    ];
    assert_eq!(draw(Mark::Focus), filled);
    assert_eq!(draw(Mark::NoFocus), empty);
}

// vtium/vtium#135: the notice's glyphs take the 1-bit stand-ins a
// cell's do, so a shade keeps its pattern where the cut would empty it
#[test]
fn a_1_bit_notice_draws_a_shade_as_its_1_bit_pattern() {
    let font = font("unscii-8", 8);
    let d = Surface {
        depth: Depth::One,
        ..display(320, 240)
    };
    let mut canvas = Canvas::new(320, 240, 0);
    Notice::new("\u{2591}")
        .draw(&font, d, Colors::NOTICE, None, &mut canvas)
        .expect("it fits");
    let thick = vtium_font::sprite::light_line(8);
    let want: Vec<u32> = vtium_font::sprite::one_bit('\u{2591}', 8, 8, thick)
        .expect("a 1-bit shade")
        .pixels()
        .iter()
        .map(|&a| if a >= 128 { 0 } else { 0x00ff_ffff })
        .collect();
    assert!(want.contains(&0) && want.contains(&0x00ff_ffff));
    assert_eq!(block(&canvas, 5, 3, 8, 8), want);
}

/// A face of a narrow `x` and a wide U+4E00, solid, that counts the
/// glyphs it draws. It knows each glyph's width without drawing it,
/// as a face that keeps the widths in its map does.
struct Counting(std::cell::Cell<u32>);

const WIDE: char = '\u{4e00}';

impl Face for Counting {
    fn cell(&self) -> vtium_font::Cell {
        vtium_font::Cell {
            width: 8,
            height: 8,
        }
    }
    fn size_step(&self) -> u32 {
        8
    }
    fn has_glyph(&self, c: char) -> bool {
        c == 'x' || c == WIDE
    }
    fn advance(&self, c: char) -> Option<Width> {
        match c {
            'x' => Some(Width::Narrow),
            WIDE => Some(Width::Wide),
            _ => None,
        }
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
        self.0.set(self.0.get() + 1);
        match cluster {
            ['x'] => vtium_font::Glyph::from_pixels(8, 8, Format::Mask, vec![255; 64]),
            [WIDE] => vtium_font::Glyph::from_pixels(16, 8, Format::Mask, vec![255; 128])
                .map(|g| g.with_cells(Width::Wide)),
            _ => None,
        }
    }
}

// vtium/vtium#159: the notice measures its text by the face's advance
// and draws each character it shows once, however often it appears,
// so its cost follows the characters and not the text; the wide
// character takes two cells, so the `x` after it starts three cells
// from the line's start
#[test]
fn a_notice_measures_by_the_advance_and_draws_each_character_once() {
    let draw = |text: String| {
        let font = Font::new(Counting(std::cell::Cell::new(0)));
        let mut canvas = Canvas::new(320, 240, 0);
        Notice::new(text)
            .draw(&font, display(320, 240), Colors::NOTICE, None, &mut canvas)
            .expect("it fits");
        (font.face().0.get(), canvas)
    };
    let (draws, _) = draw(format!("x{WIDE}x").repeat(100));
    assert_eq!(draws, 2, "one draw for each character");
    let (_, canvas) = draw(format!("x{WIDE}x"));
    // the text starts an eighth of the cell plus half a cell across and
    // an eighth plus a quarter down: at 5 and 3
    let ink = pixel(Colors::NOTICE.ink);
    assert!(block(&canvas, 5, 3, 32, 8).iter().all(|&p| p == ink));
    assert!(block(&canvas, 37, 3, 1, 8).iter().all(|&p| p != ink));
}
