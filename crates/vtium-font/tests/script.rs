//! Superscript and subscript glyphs as a [`Font`] draws them for the
//! console's own sources (ADR-1202): a bitmap face's glyph scaled to two
//! thirds with each pixel fully on or off (D4), a sprite drawn at the small
//! size (D6), bold and italic added to the small glyph (D7), and each
//! script glyph made from the face that draws the cell's normal glyph
//! (D15).

#![cfg(feature = "unifont")]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]

use vtium_font::script::Rect;
use vtium_font::spacing::Band;
use vtium_font::{
    Cell, Face, Font, Glyph, Kind, Origin, Script, Style, Unifont, Variant, Width, sprite,
};

fn variant(script: Script, style: Style) -> Variant {
    Variant { style, script }
}

const BOLD: Style = Style {
    bold: true,
    italic: false,
};

fn unifont() -> Font<Unifont> {
    Font::new(Unifont::new(16))
}

fn script_glyph<F: Face>(font: &Font<F>, c: char, script: Script, style: Style) -> Glyph {
    font.drawn_variant(&[c], Width::Narrow, variant(script, style))
        .unwrap()
        .glyph
}

/// Whether the pixel at `x`, `y` holds ink.
fn ink(g: &Glyph, x: i64, y: i64) -> bool {
    match (u32::try_from(x), u32::try_from(y)) {
        (Ok(x), Ok(y)) => g.pixel(x, y).is_some_and(|p| p[0] >= 128),
        _ => false,
    }
}

/// The number of strokes, 8-connected ink, and of holes, 4-connected
/// background that does not touch the glyph's edge.
fn shape(g: &Glyph) -> (usize, usize) {
    let (w, h) = (i64::from(g.width()), i64::from(g.height()));
    let mut seen = vec![false; (w * h) as usize];
    let (mut strokes, mut holes) = (0, 0);
    for start in 0..w * h {
        if seen[start as usize] {
            continue;
        }
        let lit = ink(g, start % w, start / w);
        let mut stack = vec![start];
        seen[start as usize] = true;
        let mut edge = false;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            edge |= x == 0 || y == 0 || x == w - 1 || y == h - 1;
            for (dx, dy) in [
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                if !lit && dx != 0 && dy != 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = ny * w + nx;
                if !seen[j as usize] && ink(g, nx, ny) == lit {
                    seen[j as usize] = true;
                    stack.push(j);
                }
            }
        }
        if lit {
            strokes += 1;
        } else if !edge {
            holes += 1;
        }
    }
    (strokes, holes)
}

// D4, measured as the record measured it: of unifont's 94 printable ASCII
// characters and é, É, Å, ², α and β, the on-or-off scale to 5x11 keeps
// the strokes and holes of all but 9, where a gap one pixel high closes
// covers: sgr.script
#[test]
fn the_bitmap_scale_keeps_the_shape_of_91_of_100_unifont_glyphs() {
    let font = unifont();
    let chars = ('!'..='~').chain(['é', 'É', 'Å', '²', 'α', 'β']);
    let mut changed = Vec::new();
    for c in chars {
        let normal = font.glyph(&[c], Width::Narrow, Style::default()).unwrap();
        let small = script_glyph(&font, c, Script::Super, Style::default());
        assert!(
            small.pixels().iter().all(|&p| p == 0 || p == 255),
            "{c}: only on and off"
        );
        if shape(&normal) != shape(&small) {
            changed.push(c);
        }
    }
    assert_eq!(
        changed,
        ['!', '%', '&', '?', '@', 'a', 'i', 'j', 'Å'],
        "the glyphs whose shape changed"
    );
}

// D6: a sprite in a script cell is drawn at the small box's size, with the
// light line of that height, not drawn full size and scaled
// covers: sgr.script
#[test]
fn a_sprite_is_drawn_at_the_small_size() {
    let font = unifont();
    for c in ['─', '┼', '█', '░', '⣿'] {
        // two thirds of 8x16 is 5x11, one column in
        let small = sprite::draw(c, 5, 11, sprite::light_line(11)).unwrap();
        for (script, top) in [(Script::Super, 0), (Script::Sub, 5)] {
            let g = script_glyph(&font, c, script, Style::default());
            assert_eq!((g.width(), g.height()), (8, 16));
            for y in 0..16 {
                for x in 0..8 {
                    let want = if (1..6).contains(&x) && (top..top + 11).contains(&y) {
                        small.pixel(x - 1, y - top).unwrap()[0]
                    } else {
                        0
                    };
                    assert_eq!(g.pixel(x, y).unwrap()[0], want, "{c} {script:?} at {x},{y}");
                }
            }
        }
    }
}

// D7: bold is the small glyph widened by a pixel, each pixel lit when it
// or its left neighbour is, in unifont
// covers: sgr.script
#[test]
fn bold_is_added_after_the_glyph_is_made_small() {
    let font = unifont();
    for c in ['l', 'H', 'm', '|', 'x'] {
        for script in [Script::Super, Script::Sub] {
            let regular = script_glyph(&font, c, script, Style::default());
            let bold = script_glyph(&font, c, script, BOLD);
            for y in 0..16 {
                for x in 0..8 {
                    let want = ink(&regular, x, y) || ink(&regular, x - 1, y);
                    assert_eq!(ink(&bold, x, y), want, "{c} {script:?} at {x},{y}");
                }
            }
        }
    }
}

/// A 1-bit face of 8x16 cells whose every glyph is a vertical stroke in
/// column `column`, which says its glyphs are 1-bit, and whose cell has
/// line spacing `band` around a 16-row text band.
struct Stroke {
    column: u32,
    band: Band,
}

impl Face for Stroke {
    fn cell(&self) -> Cell {
        Cell {
            width: 8,
            height: 16 + self.band.added(),
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, c: char) -> bool {
        c == 'x'
    }
    fn draw(&self, _cluster: &[char], _style: Style) -> Option<Glyph> {
        let mut px = vec![0u8; (8 * self.cell().height) as usize];
        for y in self.band.above..self.band.above + 16 {
            px[(y * 8 + self.column) as usize] = 255;
        }
        Glyph::from_pixels(8, self.cell().height, vtium_font::Format::Mask, px)
    }
    fn band(&self) -> Band {
        self.band
    }
    fn origin(&self, _cluster: &[char], _style: Style) -> Origin {
        Origin {
            kind: Kind::Bitmap,
            own: Some(Rect {
                left: 0,
                top: self.band.above,
                width: 8,
                height: 16,
            }),
            ..Origin::default()
        }
    }
}

// D15: a script glyph comes from the face that draws the cell's normal
// glyph, its own stroke made small, and never from the built-in font
// covers: sgr.script
#[test]
fn a_face_draws_its_own_script_glyph() {
    let font = Font::new(Stroke {
        column: 6,
        band: Band::NONE,
    });
    let g = script_glyph(&font, 'x', Script::Super, Style::default());
    // column 6 goes to small column 3, one column in, rows 0 to 10
    for y in 0..16 {
        for x in 0..8 {
            assert_eq!(ink(&g, x, y), x == 5 && y < 11, "at {x},{y}");
        }
    }
}

// D1: in a face with line spacing, a subscript's bottom is the text band's
// bottom row and a superscript's top the band's top row, not the cell's
// covers: sgr.script
#[test]
fn a_script_glyph_sits_against_the_faces_text_band() {
    let font = Font::new(Stroke {
        column: 3,
        band: Band { above: 3, below: 2 },
    });
    let rows = |script| -> Vec<i64> {
        let g = script_glyph(&font, 'x', script, Style::default());
        (0..21).filter(|&y| (0..8).any(|x| ink(&g, x, y))).collect()
    };
    assert_eq!(rows(Script::Super), (3..14).collect::<Vec<_>>());
    assert_eq!(rows(Script::Sub), (8..19).collect::<Vec<_>>());
}

// D4, D9: a wide character's script glyph is two thirds of the wide
// glyph, centred across its two cells, every pixel on or off
// covers: sgr.script
#[test]
fn a_wide_script_glyph_is_centred_across_its_cells() {
    let font = unifont();
    let g = font
        .drawn_variant(&['中'], Width::Wide, variant(Script::Sub, Style::default()))
        .unwrap()
        .glyph;
    assert_eq!((g.width(), g.height(), g.cells()), (16, 16, Width::Wide));
    assert!(g.pixels().iter().all(|&p| p == 0 || p == 255));
    let cols: Vec<i64> = (0..16)
        .filter(|&x| (0..16).any(|y| ink(&g, x, y)))
        .collect();
    assert!(cols.iter().all(|x| (2..13).contains(x)), "{cols:?}");
    let rows: Vec<i64> = (0..16)
        .filter(|&y| (0..16).any(|x| ink(&g, x, y)))
        .collect();
    assert!(rows.iter().all(|y| (5..16).contains(y)), "{rows:?}");
}
