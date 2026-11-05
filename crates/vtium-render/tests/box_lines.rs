//! Box drawing and block elements at a large cell, after the font's glyphs
//! and lines have arrived from the glyph helper (vtium/vtium#310). A frit
//! (a font set file) of an outline font such as DejaVu Sans Mono draws a
//! 20x38 cell. Once the helper has measured it, the font's underline is a
//! row thick, but vtium's sprites keep a light line that grows with the
//! cell: 2 pixels at 38 rows, heavy lines twice that, double lines two
//! light strokes a light stroke apart, every pixel solid.

mod common;

use std::cell::Cell as Shared;
use std::rc::Rc;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Decorations, Face, Font, Format, Glyph, SpritePolicy, Style};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::Age;

const CELL: Cell = Cell {
    width: 20,
    height: 38,
};

/// A 20x38 outline face that holds every character, as DejaVu Sans Mono
/// holds box drawing, and draws each one soft: every pixel half covered.
/// It draws the sprite ranges as sprites, as a frit does unless its
/// `glyph-source` lines say otherwise.
struct Outline {
    lines: Rc<Shared<Decorations>>,
}

impl Face for Outline {
    fn cell(&self) -> Cell {
        CELL
    }
    fn size_step(&self) -> u32 {
        1
    }
    fn has_glyph(&self, _: char) -> bool {
        true
    }
    fn sprite_policy(&self) -> SpritePolicy {
        SpritePolicy::SPRITES
    }
    fn draw(&self, _: &[char], _: Style) -> Option<Glyph> {
        let n = usize::try_from(CELL.width * CELL.height).ok()?;
        Glyph::from_pixels(CELL.width, CELL.height, Format::Mask, vec![0x80; n])
    }
    fn decorations(&self) -> Decorations {
        self.lines.get()
    }
}

/// The lines the glyph helper measures from DejaVu Sans Mono's `post`
/// table at this cell: the underline and strike-through a row thick.
const MEASURED: Decorations = Decorations {
    underline: 34,
    underline_thickness: 1,
    strike: 18,
    strike_thickness: 1,
};

/// The characters, one a cell, in the order [`lit_runs`] reads them.
const ROW: &str = "\u{2500}\u{2501}\u{2550}\u{2502}\u{2503}\u{2551}\u{2588}\u{2580}";

/// The lengths of the runs of foreground pixels along the line through the
/// middle of cell `col`: down its middle column for a horizontal line, and
/// across its middle row for a vertical one.
fn lit_runs(canvas: &Canvas, col: u32, vertical: bool, fg: u32) -> Vec<u32> {
    let (x0, y0) = (col * CELL.width, 0);
    let along: Vec<u32> = if vertical {
        (0..CELL.width)
            .map(|x| canvas.pixel(x0 + x, y0 + CELL.height / 2).unwrap())
            .collect()
    } else {
        (0..CELL.height)
            .map(|y| canvas.pixel(x0 + CELL.width / 2, y0 + y).unwrap())
            .collect()
    };
    let mut runs = Vec::new();
    let mut n = 0;
    for p in along.into_iter().chain([u32::MAX]) {
        if p == fg {
            n += 1;
        } else if n > 0 {
            runs.push(n);
            n = 0;
        }
    }
    runs
}

#[test]
fn box_lines_at_a_large_cell_keep_the_cells_weight_after_the_fonts_lines_arrive() {
    let (fg, bg) = (rgb("e5e5e5"), rgb("000000"));
    let (fg_px, bg_px) = (0x00e5_e5e5, 0x0000_0000);
    // before the helper answers, the font gives the lines for the cell's
    // height
    let lines = Rc::new(Shared::new(Decorations::for_height(CELL.height)));
    let font = Font::new(Outline {
        lines: Rc::clone(&lines),
    });
    let cols = ROW.chars().count();
    let (w, h) = (CELL.width * u32::try_from(cols).unwrap(), CELL.height);
    let mut r = Renderer::new(font, w, h, Config::default());
    let mut s = Screen::new(cols, 1, fg, bg);
    s.text(0, 0, ROW, style(fg, bg));
    let state = DrawState {
        background: bg,
        ..DrawState::default()
    };
    let mut canvas = Canvas::new(w, h, 0);
    r.draw(&s.frame(Age::NEVER), &mut canvas, 0, &state);
    // the helper's glyphs arrive, with the font's measured lines
    let since = s.damage.checkpoint();
    lines.set(MEASURED);
    let all: Vec<char> = ROW.chars().collect();
    r.glyphs_changed(&all);
    r.draw(&s.frame(since), &mut canvas, 1, &state);

    let solid = canvas.pixels().iter().all(|&p| p == fg_px || p == bg_px);
    assert!(
        solid,
        "a sprite pixel is neither the foreground nor the background"
    );
    // light, heavy and double horizontal lines, read down the cell
    assert_eq!(lit_runs(&canvas, 0, false, fg_px), [2], "U+2500");
    assert_eq!(lit_runs(&canvas, 1, false, fg_px), [4], "U+2501");
    assert_eq!(lit_runs(&canvas, 2, false, fg_px), [2, 2], "U+2550");
    // and vertical, read across it
    assert_eq!(lit_runs(&canvas, 3, true, fg_px), [2], "U+2502");
    assert_eq!(lit_runs(&canvas, 4, true, fg_px), [4], "U+2503");
    assert_eq!(lit_runs(&canvas, 5, true, fg_px), [2, 2], "U+2551");
    // the double line's strokes are a light line apart
    let x = 2 * CELL.width + CELL.width / 2;
    let rows: Vec<u32> = (0..CELL.height)
        .filter(|&y| canvas.pixel(x, y) == Some(fg_px))
        .collect();
    assert_eq!(
        rows.last().unwrap() - rows.first().unwrap() + 1,
        6,
        "U+2550"
    );
    // the full block fills the cell, and the upper half block its top 19
    // rows
    assert_eq!(lit_runs(&canvas, 6, false, fg_px), [38], "U+2588");
    assert_eq!(lit_runs(&canvas, 6, true, fg_px), [20], "U+2588");
    assert_eq!(lit_runs(&canvas, 7, false, fg_px), [19], "U+2580");
    assert_eq!(
        canvas.pixel(7 * CELL.width + CELL.width / 2, 0),
        Some(fg_px),
        "U+2580 starts at the cell's top"
    );
}
