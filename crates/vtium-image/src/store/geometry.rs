//! The box and the picture a placement takes: how [`Store::place`] fits
//! an image's pixels to the cells a placement names.

use crate::bounds::BOX_CELLS_MAX;
use crate::image::{Place, Refusal};

#[cfg(doc)]
use super::Store;

/// A placement's box and picture, as [`Store::place`] fits them.
pub(super) struct Geometry {
    pub(super) cols: u32,
    pub(super) rows: u32,
    pub(super) ox: i64,
    pub(super) oy: i64,
    pub(super) dw: u64,
    pub(super) dh: u64,
    pub(super) sw: u32,
    pub(super) sh: u32,
}

impl Geometry {
    /// The source cut to the image, and the box and the picture in it: at
    /// the `drawn` size when one is given, at the source's pixel size,
    /// fitted and centred in a box of both sides, or one side given and the
    /// other following the source's aspect. A
    /// picture scaled to a sliver keeps a pixel; one scaled past the
    /// largest box is refused.
    pub(super) fn fit(
        place: &Place,
        drawn: Option<(u32, u32)>,
        (width, height): (u32, u32),
        (cw, ch): (u32, u32),
    ) -> Result<Self, Refusal> {
        let cut = |avail: u32, want: u32| {
            if want != 0 && want < avail {
                want
            } else {
                avail
            }
        };
        let sw = cut(width.saturating_sub(place.src_x), place.src_width);
        let sh = cut(height.saturating_sub(place.src_y), place.src_height);
        let (sw64, sh64) = (u64::from(sw).max(1), u64::from(sh).max(1));
        let (cw64, ch64) = (u64::from(cw), u64::from(ch));
        let (mut ox, mut oy) = (i64::from(place.x), i64::from(place.y));
        let scale = |a: u64, num: u64, den: u64| {
            a.saturating_mul(num)
                .saturating_add(den / 2)
                .checked_div(den)
                .unwrap_or(0)
        };
        let (mut cols, mut rows, mut dw, mut dh);
        match (drawn, place.cols, place.rows) {
            // a drawn size the protocol worked out, at the box's top left
            (Some((w, h)), _, _) => {
                (cols, rows) = (0, 0);
                (dw, dh) = (u64::from(w), u64::from(h));
            }
            (None, 0, 0) => {
                (cols, rows) = (0, 0);
                (dw, dh) = (u64::from(sw), u64::from(sh));
            }
            (None, c, 0) => {
                (cols, rows) = (c, 0);
                dw = u64::from(c).saturating_mul(cw64);
                dh = scale(dw, sh64, sw64);
            }
            (None, 0, r) => {
                (cols, rows) = (0, r);
                dh = u64::from(r).saturating_mul(ch64);
                dw = scale(dh, sw64, sh64);
            }
            (None, c, r) => {
                (cols, rows) = (c, r);
                let bw = u64::from(c)
                    .saturating_mul(cw64)
                    .saturating_sub(u64::from(place.x));
                let bh = u64::from(r)
                    .saturating_mul(ch64)
                    .saturating_sub(u64::from(place.y));
                if bw.saturating_mul(sh64) <= bh.saturating_mul(sw64) {
                    dw = bw;
                    dh = scale(bw, sh64, sw64);
                } else {
                    dh = bh;
                    dw = scale(bh, sw64, sh64);
                }
                ox = ox.saturating_add(i64::try_from(bw.saturating_sub(dw) / 2).unwrap_or(0));
                oy = oy.saturating_add(i64::try_from(bh.saturating_sub(dh) / 2).unwrap_or(0));
            }
        }
        dw = dw.max(1);
        dh = dh.max(1);
        if dw > BOX_CELLS_MAX.saturating_mul(cw64) || dh > BOX_CELLS_MAX.saturating_mul(ch64) {
            return Err(Refusal::Invalid);
        }
        let span = |o: i64, d: u64, c: u64| {
            let end = u64::try_from(o).unwrap_or(0).saturating_add(d);
            u32::try_from(end.div_ceil(c.max(1))).unwrap_or(u32::MAX)
        };
        if cols == 0 {
            cols = span(ox, dw, cw64);
        }
        if rows == 0 {
            rows = span(oy, dh, ch64);
        }
        Ok(Self {
            cols,
            rows,
            ox,
            oy,
            dw,
            dh,
            sw,
            sh,
        })
    }
}
