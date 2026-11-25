//! The test pattern that stands in for a row of text. Each character cell
//! gets a background and a foreground color and one of a few bitmap
//! shapes, all chosen from the row's id and the cell's column, so a row
//! drawn twice comes out the same and two rows differ. A cell's shape is a
//! bitmask a line, as a bitmap font's glyph is, so drawing a row costs
//! about what drawing a row of text from a glyph store costs.
//!
//! The text look draws a row a person can follow by eye instead: the row's
//! number in digits at its start, then a run of letters that repeats along
//! the row, shifted by one cell from each row to the next. The digits and
//! letters are bitmask glyphs of the tool's own, drawn the same way as the
//! random shapes, so the look changes what a row shows and not what drawing
//! it costs.

use crate::Geometry;

/// The border's color, which fills the pixels right of the last cell and
/// the strip under the grid.
pub const BORDER: u32 = 0x0010_1418;

const BACKGROUNDS: [u32; 2] = [0x0000_0000, 0x0018_1820];
const FOREGROUNDS: [u32; 6] = [
    0x00d0_d0d0,
    0x00e0_6060,
    0x0060_d060,
    0x0060_80e0,
    0x00e0_c050,
    0x00c0_60c0,
];
/// The random shapes a cell can hold, the first a blank cell.
const SHAPES: usize = 8;

/// The glyphs of the text look, 5 columns by 7 lines, each line a string
/// in which `#` lights a pixel: the digits 0 to 9, then the letters of
/// [`LETTERS`].
const GLYPHS: [[&str; 7]; 15] = [
    [
        ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
    ],
    [
        "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
    ],
    [
        ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
    ],
    [
        "####.", "....#", "....#", ".###.", "....#", "....#", "####.",
    ],
    [
        "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
    ],
    [
        "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
    ],
    [
        ".###.", "#....", "#....", "####.", "#...#", "#...#", ".###.",
    ],
    [
        "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
    ],
    [
        ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
    ],
    [
        ".###.", "#...#", "#...#", ".####", "....#", "....#", ".###.",
    ],
    [
        "#...#", "#...#", "#...#", ".#.#.", ".#.#.", "..#..", "..#..",
    ],
    [
        "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
    ],
    [
        ".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.",
    ],
    [
        "#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
    ],
    [
        "#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#",
    ],
];
/// The letters of the text look's repeating run, after the digits in
/// [`GLYPHS`], and a blank cell between runs.
const LETTERS: usize = 5;
/// The digits of the row number at the start of each row of the text look.
const DIGITS: u32 = 6;

/// What the pattern draws in each row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    /// Random shapes and colors, the timings' default.
    #[default]
    Random,
    /// The row's number, then a run of letters that repeats along the row.
    Text,
}

impl Look {
    pub fn name(self) -> &'static str {
        match self {
            Look::Random => "random",
            Look::Text => "text",
        }
    }

    pub fn parse(name: &str) -> Option<Look> {
        [Look::Random, Look::Text]
            .into_iter()
            .find(|l| l.name() == name)
    }
}

/// The pattern for one cell size: its shapes, a bitmask for each line.
pub struct Pattern {
    geometry: Geometry,
    look: Look,
    /// `SHAPES` random shapes, then the text look's glyphs, each of
    /// `cell_height` lines; bit x of a line lights column x of the cell.
    shapes: Vec<u64>,
}

/// A 64-bit mix of a row's id and a column, the splitmix64 finalizer.
fn mix(row: i64, col: u32) -> u64 {
    let mut z = (row as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(u64::from(col).wrapping_mul(0xbf58_476d_1ce4_e5b9));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Pattern {
    /// The random pattern for `geometry`'s cell, which is at most 64 pixels
    /// wide.
    pub fn new(geometry: Geometry) -> Pattern {
        Pattern::with_look(geometry, Look::Random)
    }

    /// The pattern of `look` for `geometry`'s cell.
    pub fn with_look(geometry: Geometry, look: Look) -> Pattern {
        let (w, h) = (geometry.cell_width.min(64), geometry.cell_height);
        let mut shapes = vec![0u64; (SHAPES + GLYPHS.len()) * h as usize];
        for (shape, lines) in shapes
            .chunks_mut(h as usize)
            .enumerate()
            .take(SHAPES)
            .skip(1)
        {
            for (y, line) in lines.iter_mut().enumerate() {
                let y = y as u32;
                let (top, bottom) = (h / 5, h - h / 5);
                if y < top || y >= bottom {
                    continue;
                }
                for x in w / 8..w - w / 8 {
                    let lit = match shape {
                        1 => x == w / 2 || y == h / 2,
                        2 => x == w / 8 || x == w - w / 8 - 1 || y == top || y == bottom - 1,
                        3 => (x + y).is_multiple_of(3),
                        4 => y.is_multiple_of(2),
                        5 => x < w / 2,
                        6 => (x * h) / w.max(1) == y - top || y == bottom - 1,
                        _ => (x ^ y) & 2 == 0,
                    };
                    if lit {
                        *line |= 1 << x;
                    }
                }
            }
        }
        // Each glyph's 5 by 7 grid, scaled into the cell's inner box, the
        // same box the random shapes use.
        let (left, right) = (w / 8, w - w / 8);
        let (top, bottom) = (h / 5, h - h / 5);
        for (g, glyph) in GLYPHS.iter().enumerate() {
            let lines = &mut shapes[(SHAPES + g) * h as usize..][..h as usize];
            for y in top..bottom {
                let gy = ((y - top) * 7 / (bottom - top).max(1)) as usize;
                for x in left..right {
                    let gx = ((x - left) * 5 / (right - left).max(1)) as usize;
                    if glyph[gy].as_bytes()[gx] == b'#' {
                        lines[y as usize] |= 1 << x;
                    }
                }
            }
        }
        Pattern {
            geometry,
            look,
            shapes,
        }
    }

    /// The background, the foreground and the shape of column `col` of row
    /// `row`.
    fn cell(&self, row: i64, col: u32) -> (u32, u32, usize) {
        match self.look {
            Look::Random => {
                let h = mix(row, col);
                let bg = BACKGROUNDS[(h & 1) as usize];
                let fg = FOREGROUNDS[((h >> 1) % FOREGROUNDS.len() as u64) as usize];
                let shape = ((h >> 8) % SHAPES as u64) as usize;
                (bg, fg, shape)
            }
            Look::Text => {
                // Rows alternate between the two backgrounds, so the eye
                // sees where one row ends and the next begins.
                let bg = BACKGROUNDS[row.rem_euclid(2) as usize];
                if col < DIGITS {
                    let place = 10_i64.pow(DIGITS - 1 - col);
                    let digit = row.rem_euclid(10_i64.pow(DIGITS)) / place % 10;
                    (bg, FOREGROUNDS[4], SHAPES + digit as usize)
                } else if col == DIGITS {
                    (bg, FOREGROUNDS[0], 0)
                } else {
                    let run = LETTERS as i64 + 1;
                    let at = (i64::from(col - DIGITS - 1) + row).rem_euclid(run) as usize;
                    let shape = if at == LETTERS { 0 } else { SHAPES + 10 + at };
                    (bg, FOREGROUNDS[0], shape)
                }
            }
        }
    }

    /// Draws text row `row` into `dst`, whose first byte is the row's top
    /// left pixel and whose lines are `stride` bytes apart, as wide as the
    /// screen. Returns the bytes written.
    pub fn draw_row(&self, dst: &mut [u8], stride: usize, row: i64) -> u64 {
        self.draw_lines(dst, stride, row, 0, self.geometry.cell_height)
    }

    /// Draws `count` pixel lines of text row `row`, from its line `first`
    /// down, into `dst`, whose first byte is the left pixel of line `first`
    /// and whose lines are `stride` bytes apart. A smooth scroll draws a
    /// row in part this way where only some of its lines come into view.
    /// Returns the bytes written.
    pub fn draw_lines(
        &self,
        dst: &mut [u8],
        stride: usize,
        row: i64,
        first: u32,
        count: u32,
    ) -> u64 {
        let g = self.geometry;
        let (cw, ch) = (g.cell_width as usize, g.cell_height as usize);
        let cols = g.cols();
        let line_bytes = g.line_bytes();
        // The colors and shape of each cell, chosen once for the row.
        let cells: Vec<(u32, u32, usize)> = (0..cols).map(|col| self.cell(row, col)).collect();
        let first = first as usize;
        let end = (first + count as usize).min(ch);
        let mut written = 0;
        for (i, y) in (first..end).enumerate() {
            let Some(line) = dst.get_mut(i * stride..i * stride + line_bytes) else {
                break;
            };
            let (cells_part, rest) = line.split_at_mut(cols as usize * cw * 4);
            for (&(bg, fg, shape), cell) in cells.iter().zip(cells_part.chunks_exact_mut(cw * 4)) {
                let mask = self.shapes[shape * ch + y];
                for (x, px) in cell.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let color = if mask >> x & 1 == 1 { fg } else { bg };
                    *px = color.to_le_bytes();
                }
            }
            for px in rest.as_chunks_mut::<4>().0 {
                *px = BORDER.to_le_bytes();
            }
            written += line_bytes as u64;
        }
        written
    }

    /// Fills `lines` lines of `dst` with the border's color, each as wide
    /// as the screen and `stride` bytes apart. Returns the bytes written.
    pub fn draw_border(&self, dst: &mut [u8], stride: usize, lines: u32) -> u64 {
        let line_bytes = self.geometry.line_bytes();
        let mut written = 0;
        for y in 0..lines as usize {
            let Some(line) = dst.get_mut(y * stride..y * stride + line_bytes) else {
                break;
            };
            for px in line.as_chunks_mut::<4>().0 {
                *px = BORDER.to_le_bytes();
            }
            written += line_bytes as u64;
        }
        written
    }
}
