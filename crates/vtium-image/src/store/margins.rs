//! A region scroll between left and right margins (ADR-1100, D3 and D6).
//!
//! While DECLRMM is set, a region scroll moves only the cells inside the
//! rectangle that the four margins bound, and the screen's lines stay where
//! they are. A placement whose rules split it moves the part of it inside
//! the rectangle, keeps the parts outside it where they are, and loses what
//! the scroll pushes out. A placement whose rules move it whole moves only
//! when it lies wholly inside the rectangle, and otherwise keeps its place.

use super::events::{Anchor, Piece};
use super::{Rows, Spot, Store};
use crate::rules::{RegionScroll, rules};

impl Store {
    /// A scroll of the rectangle of rows `top` to `bottom` and columns
    /// `left` to `right` of the screen in use by `n` rows, up or down,
    /// before its cells move. The lines do not move, so what moves waits
    /// for [`scroll_done`](Self::scroll_done), which anchors it to the line
    /// at its new row once the cells have moved. Nothing enters the
    /// scrollback.
    pub fn scroll_in(
        &mut self,
        (top, bottom): (usize, usize),
        (left, right): (usize, usize),
        n: usize,
        up: bool,
        rows: Rows<'_>,
    ) {
        let b = self.active();
        if !self
            .buffers
            .get(b)
            .is_some_and(|buf| !buf.placements.is_empty())
            || bottom < top
            || right < left
        {
            return;
        }
        let as_i64 = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
        let to_u32 = |v: i64| u32::try_from(v).unwrap_or(0);
        let (lo, bottom) = (as_i64(top), as_i64(bottom));
        let (left, right) = (as_i64(left), as_i64(right));
        let span = bottom.saturating_sub(lo).saturating_add(1);
        let n = as_i64(n).min(span);
        // a box reaching past the screen's last row is inside a band that
        // ends there, as in a scroll of whole lines
        let hi = if bottom.saturating_add(1) == rows.height() {
            i64::MAX
        } else {
            bottom
        };
        let clip = if up { hi } else { bottom };
        let d = if up { n.saturating_neg() } else { n };
        for (key, r0) in self.tops(b, rows) {
            let Some(p) = self.placement(b, key) else {
                continue;
            };
            let Spot::Line(anchor) = p.spot else {
                continue;
            };
            let (prows, last, scroll) = (
                i64::from(p.rows),
                p.cols.saturating_sub(1),
                rules(p.protocol.0).region_scroll,
            );
            let c0 = i64::from(p.col);
            let c1 = c0.saturating_add(i64::from(last));
            let r1 = r0.saturating_add(prows).saturating_sub(1);
            if r1 < lo || r0 > bottom || c1 < left || c0 > right {
                continue;
            }
            let inside = r0 >= lo && r1 <= hi && c0 >= left && c1 <= right;
            if scroll == RegionScroll::WholeOnly && !inside {
                // across an edge: it keeps its place, and its line stays
                continue;
            }
            // the box rows inside the band, and where they move to, clipped
            // at the band's edges
            let ia = r0.max(lo).saturating_sub(r0);
            let ib = r1.min(hi).saturating_sub(r0);
            let moved = r0.saturating_add(d);
            let na = ia.max(lo.saturating_sub(moved));
            let nb = ib.min(clip.saturating_sub(moved));
            let band_line = rows.at(r0.saturating_add(ia));
            // the box columns inside the rectangle
            let ca = to_u32(left.max(c0).saturating_sub(c0));
            let cb = to_u32(right.min(c1).saturating_sub(c0));
            let mut pieces = Vec::with_capacity(5);
            if r0 < lo {
                pieces.push(Piece {
                    r0: 0,
                    r1: to_u32(lo.saturating_sub(1).saturating_sub(r0)),
                    c0: 0,
                    c1: last,
                    anchor: Anchor::Line(anchor),
                });
            }
            if ca > 0
                && let Some(line) = band_line
            {
                pieces.push(Piece {
                    r0: to_u32(ia),
                    r1: to_u32(ib),
                    c0: 0,
                    c1: ca.saturating_sub(1),
                    anchor: Anchor::Line(line),
                });
            }
            if na <= nb {
                pieces.push(Piece {
                    r0: to_u32(na),
                    r1: to_u32(nb),
                    c0: ca,
                    c1: cb,
                    anchor: Anchor::Row(moved.saturating_add(na)),
                });
            }
            if cb < last
                && let Some(line) = band_line
            {
                pieces.push(Piece {
                    r0: to_u32(ia),
                    r1: to_u32(ib),
                    c0: cb.saturating_add(1),
                    c1: last,
                    anchor: Anchor::Line(line),
                });
            }
            if r1 > hi
                && let Some(line) = rows.at(hi.saturating_add(1))
            {
                pieces.push(Piece {
                    r0: to_u32(hi.saturating_add(1).saturating_sub(r0)),
                    r1: to_u32(prows.saturating_sub(1)),
                    c0: 0,
                    c1: last,
                    anchor: Anchor::Line(line),
                });
            }
            self.split(b, key, &pieces);
        }
    }
}
