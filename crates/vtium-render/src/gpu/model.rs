//! The stand-in's model of the cell shader (`glsl/cell.frag`), which
//! [`StandIn`](super::StandIn) draws the cell program's quads with, so that
//! a test can compare the cells the GPU path composes with the CPU path's
//! drawing of the same screen.
//!
//! The model reads what the GPU path hands the shader, the quads, the
//! glyphs' textures, the boxes, the ink grid and the uniforms, the way the
//! shader reads them, and does each blend with the CPU path's own
//! arithmetic (`src/blend.rs`), which the shader does in whole numbers.
//! Tier 2 judges the shader itself on a real GL against the CPU path; the
//! model judges what the GPU path gives it, where no GL is at hand.

use vtium_term::Rgb;

use super::gl::{Format, Quad, Size};
use crate::blend::{argb_over, mask, over_bg, pixel, tint_over};

/// A texture's pixels as the stand-in holds them.
#[derive(Clone, Copy, Debug)]
pub(super) struct Texels<'a> {
    pub(super) format: Format,
    pub(super) size: Size,
    pub(super) bytes: &'a [u8],
}

impl Texels<'_> {
    /// Channel `ch` of the texel at `x`, `y`, as `texelFetch` reads it; 0
    /// outside the texture.
    fn byte(&self, (x, y): (i64, i64), ch: usize) -> u8 {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return 0;
        };
        let size = |v: u32| usize::try_from(v).unwrap_or(0);
        let (w, h) = (size(self.size.width), size(self.size.height));
        if x >= w || y >= h {
            return 0;
        }
        let bpp = self.format.bytes();
        let at = y
            .saturating_mul(w)
            .saturating_add(x)
            .saturating_mul(bpp)
            .saturating_add(ch.min(bpp.saturating_sub(1)));
        self.bytes.get(at).copied().unwrap_or(0)
    }

    fn rgb(&self, at: (i64, i64)) -> Rgb {
        Rgb::new(self.byte(at, 0), self.byte(at, 1), self.byte(at, 2))
    }

    /// A premultiplied color texel as `0xAARRGGBB`.
    fn argb(&self, at: (i64, i64)) -> u32 {
        u32::from_be_bytes([
            self.byte(at, 3),
            self.byte(at, 0),
            self.byte(at, 1),
            self.byte(at, 2),
        ])
    }
}

/// What a draw of the cell program binds.
#[derive(Clone, Copy, Debug)]
pub(super) struct Inputs<'a> {
    pub(super) masks: Texels<'a>,
    pub(super) colors: Texels<'a>,
    pub(super) sprites: Texels<'a>,
    pub(super) boxes: Texels<'a>,
    pub(super) ink: Texels<'a>,
    /// The cell's width and height.
    pub(super) cell: (i64, i64),
    /// The sprites across the sprites' texture.
    pub(super) sprite_columns: i64,
    /// A box slot's width and height, and the slots across the texture.
    pub(super) box_slot: (i64, i64, i64),
    /// The shader's `REACH_SIDE` and `REACH_ROWS`.
    pub(super) bound: (i64, i64),
    /// The shader is built with `INK`, so a quad flagged as receiving ink
    /// draws it.
    pub(super) with_ink: bool,
}

/// The job a quad draws, as the shader reads it.
#[derive(Clone, Copy, Debug)]
struct Job {
    col: i64,
    row: i64,
    cells: i64,
    marked: bool,
    fg: Rgb,
}

const RECEIVES: u32 = 1;
const MARKED: u32 = 2;
const DOWN: u32 = 4;

fn rgb(c: u32) -> Rgb {
    let [_, r, g, b] = c.to_be_bytes();
    Rgb::new(r, g, b)
}

fn int(v: impl TryInto<i64>) -> i64 {
    v.try_into().unwrap_or(i64::MAX)
}

/// The pixel `0x00RRGGBB` the cell shader writes at the target's pixel
/// `x`, `y` for `q`.
pub(super) fn cell(q: &Quad, i: &Inputs<'_>, (x, y): (i64, i64)) -> u32 {
    let [dx, dy, dw, _] = q.dst.map(i64::from);
    let [sx, sy, col, row] = q.src.map(i64::from);
    let [p0, p1, p2, p3] = q.params;
    let local = (x.saturating_sub(dx), y.saturating_sub(dy));
    let at = (sx.saturating_add(local.0), sy.saturating_add(local.1));
    let (kind, flags, ul, st) = (p0 >> 24, p1 >> 24, p2 >> 24, p3 >> 24);
    let (fg, bg) = (rgb(p0), rgb(p1));
    let (cw, ch) = i.cell;
    let job = Job {
        col,
        row,
        cells: dw.checked_div(cw.max(1)).unwrap_or(0),
        marked: flags & MARKED != 0,
        fg,
    };
    let px = (
        col.saturating_mul(cw).saturating_add(local.0),
        row.saturating_mul(ch).saturating_add(local.1),
    );
    let a = if kind == 1 { i.masks.byte(at, 0) } else { 0 };
    let mut one = ul == 0 && st == 0;
    let mut recv = 0;
    if i.with_ink && flags & RECEIVES != 0 && line_inked(i, &job, px, flags & DOWN != 0) {
        recv = received(i, &job, px);
        one = one && recv >> 24 == 0 && (a == 0 || a == 255);
    }
    if one {
        return match kind {
            1 => mask(fg, bg, a),
            2 => over_bg(bg, i.colors.argb(at)),
            _ => pixel(bg),
        };
    }
    let pos = (local.0.checked_rem(cw.max(1)).unwrap_or(0), local.1);
    let sprite = |k: u32| {
        let k = int(k.saturating_sub(1));
        let across = i.sprite_columns.max(1);
        let (col, row) = (k.checked_rem(across), k.checked_div(across));
        let (col, row) = (col.unwrap_or(0), row.unwrap_or(0));
        let x = col.saturating_mul(cw).saturating_add(pos.0);
        let y = row.saturating_mul(ch).saturating_add(pos.1);
        i.sprites.byte((x, y), 0)
    };
    let mut p = 0;
    if ul != 0 {
        p = tint_over(p, sprite(ul), rgb(p2));
    }
    p = argb_over(p, recv);
    match kind {
        1 => p = tint_over(p, a, fg),
        2 => p = argb_over(p, i.colors.argb(at)),
        _ => {}
    }
    if st != 0 {
        p = tint_over(p, sprite(st), rgb(p3));
    }
    over_bg(bg, p)
}

/// The shader's `texels`: the texels of a cell's entry in the ink grid,
/// two for the glyph it sends ink from and a texel for each 32 of its
/// line bits, as many as the larger of the cell's width and height.
fn texels(i: &Inputs<'_>) -> i64 {
    let (cw, ch) = i.cell;
    2i64.saturating_add(cw.max(ch).saturating_add(31) / 32)
}

/// The shader's `take`: the ink of the glyph whose first cell is at
/// column `k` in row `r` over `p` at the pixel `at`.
fn take(p: u32, i: &Inputs<'_>, (k, r): (i64, i64), job: &Job, at: (i64, i64), gap: i64) -> u32 {
    let (cols, lines) = (int(i.ink.size.width), int(i.ink.size.height));
    let entry = r.saturating_mul(texels(i));
    if k < 0 || r < 0 || k >= cols || entry.saturating_add(1) >= lines {
        return p;
    }
    let reach = i64::from(i.ink.byte((k, entry), 0));
    let span = i64::from(i.ink.byte((k, entry), 1));
    if reach == 0 || span == 0 {
        return p;
    }
    if gap >= 0 && job.col.saturating_sub(k.saturating_add(span)) != gap {
        return p;
    }
    let c0 = k.saturating_sub(reach & 3);
    let c1 = k
        .saturating_add(span)
        .saturating_sub(1)
        .saturating_add(reach >> 2 & 3);
    let r0 = r.saturating_sub(reach >> 4 & 3);
    let r1 = r.saturating_add(reach >> 6 & 3);
    let last = job.col.saturating_add(job.cells).saturating_sub(1);
    if job.row < r0 || job.row > r1 || last < c0 || job.col > c1 {
        return p;
    }
    let (cw, ch) = i.cell;
    if at.0 < c0.saturating_mul(cw)
        || at.0 >= c1.saturating_add(1).saturating_mul(cw)
        || at.1 < r0.saturating_mul(ch)
        || at.1 >= r1.saturating_add(1).saturating_mul(ch)
    {
        return p;
    }
    let slot = i64::from(i.ink.byte((k, entry), 2)) | i64::from(i.ink.byte((k, entry), 3)) << 8;
    let (sw, sh, across) = i.box_slot;
    let (side, rows) = i.bound;
    let t = (
        at.0.saturating_sub(k.saturating_mul(cw))
            .saturating_add(side.saturating_mul(cw)),
        at.1.saturating_sub(r.saturating_mul(ch))
            .saturating_add(rows.saturating_mul(ch)),
    );
    if t.0 < 0 || t.1 < 0 || t.0 >= sw || t.1 >= sh {
        return p;
    }
    let from = (
        slot.checked_rem(across).unwrap_or(0).saturating_mul(sw),
        slot.checked_div(across).unwrap_or(0).saturating_mul(sh),
    );
    let a = i
        .boxes
        .byte((from.0.saturating_add(t.0), from.1.saturating_add(t.1)), 0);
    let c = if job.marked {
        job.fg
    } else {
        i.ink.rgb((k, entry.saturating_add(1)))
    };
    tint_over(p, a, c)
}

/// The shader's `received`, in ADR-1306 D4's order.
fn received(i: &Inputs<'_>, job: &Job, at: (i64, i64)) -> u32 {
    let (side, rows) = i.bound;
    let end = job.col.saturating_add(job.cells);
    let mut p = 0;
    for below in [false, true] {
        for k in job.col.saturating_sub(side).saturating_sub(1)..end.saturating_add(side) {
            for n in 1..=rows {
                let r = if below {
                    job.row.saturating_add(n)
                } else {
                    job.row
                        .saturating_sub(rows)
                        .saturating_sub(1)
                        .saturating_add(n)
                };
                p = take(p, i, (k, r), job, at, -1);
            }
        }
    }
    for gap in (0..side).rev() {
        for k in job.col.saturating_sub(side).saturating_sub(1)..job.col {
            p = take(p, i, (k, job.row), job, at, gap);
        }
        p = take(p, i, (end.saturating_add(gap), job.row), job, at, -1);
    }
    p
}

/// The shader's `line_inked`: the bit the ink grid holds for the display
/// line through the pixel `at`, in the entry of the cell `at` lies in.
fn line_inked(i: &Inputs<'_>, job: &Job, at: (i64, i64), down: bool) -> bool {
    let (cw, ch) = i.cell;
    let k = at.0.checked_div(cw).unwrap_or(0);
    let n = match down {
        true => at.0.checked_rem(cw).unwrap_or(0),
        false => at.1.saturating_sub(job.row.saturating_mul(ch)),
    };
    let texel = job
        .row
        .saturating_mul(texels(i))
        .saturating_add(2)
        .saturating_add(n / 32);
    let bit = n % 32;
    let byte = i
        .ink
        .byte((k, texel), usize::try_from(bit / 8).unwrap_or(0));
    byte >> (bit % 8) & 1 != 0
}
