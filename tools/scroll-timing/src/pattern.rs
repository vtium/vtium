//! The test pattern that stands in for a row of text. Each character cell
//! gets a background and a foreground color and one of a few bitmap
//! shapes, all chosen from the row's id and the cell's column, so a row
//! drawn twice comes out the same and two rows differ. A cell's shape is a
//! bitmask a line, as a bitmap font's glyph is, so drawing a row costs
//! about what drawing a row of text from a glyph store costs.

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
/// The shapes a cell can hold, the first a blank cell.
const SHAPES: usize = 8;

/// The pattern for one cell size: its shapes, a bitmask for each line.
pub struct Pattern {
    geometry: Geometry,
    /// `SHAPES` shapes of `cell_height` lines each; bit x of a line lights
    /// column x of the cell.
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
    /// The pattern for `geometry`'s cell, which is at most 64 pixels wide.
    pub fn new(geometry: Geometry) -> Pattern {
        let (w, h) = (geometry.cell_width.min(64), geometry.cell_height);
        let mut shapes = vec![0u64; SHAPES * h as usize];
        for (shape, lines) in shapes.chunks_mut(h as usize).enumerate().skip(1) {
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
        Pattern { geometry, shapes }
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
        let cells: Vec<(u32, u32, usize)> = (0..cols)
            .map(|col| {
                let h = mix(row, col);
                let bg = BACKGROUNDS[(h & 1) as usize];
                let fg = FOREGROUNDS[((h >> 1) % FOREGROUNDS.len() as u64) as usize];
                let shape = ((h >> 8) % SHAPES as u64) as usize;
                (bg, fg, shape)
            })
            .collect();
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
