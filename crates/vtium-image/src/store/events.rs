//! What each screen event does to placements (ADR-1100, D3 and D8).
//!
//! The terminal calls these as the events happen, with the lines of the
//! screen in use. Removing part of a placement splits it into the pieces
//! left over, each a placement of the same picture with a smaller box; the
//! picture itself is never cut.

use super::{ALT, MAIN, Placement, Rows, Screens, Store};
use crate::bounds::{PLACEMENT_OVERHEAD, PLACEMENTS_PER_BUFFER};
use crate::image::{LineKey, PlacementKey};
use crate::rules::{RegionScroll, rules};

/// Where a piece is anchored: a line, or the screen row whose line holds
/// it once a scroll's lines have moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anchor {
    Line(LineKey),
    Row(i64),
}

/// A piece of a placement: box rows `r0` to `r1` and columns `c0` to `c1`,
/// counted from the placement's box.
#[derive(Clone, Copy, Debug)]
struct Piece {
    r0: u32,
    r1: u32,
    c0: u32,
    c1: u32,
    anchor: Anchor,
}

impl Piece {
    fn area(&self) -> u64 {
        let rows = u64::from(self.r1.saturating_sub(self.r0)).saturating_add(1);
        let cols = u64::from(self.c1.saturating_sub(self.c0)).saturating_add(1);
        rows.saturating_mul(cols)
    }

    /// The piece's box and picture offset, cut from `p`.
    fn cut(&self, p: &Placement) -> (u32, u32, u32, i64, i64) {
        let cols = self.c1.saturating_sub(self.c0).saturating_add(1);
        let rows = self.r1.saturating_sub(self.r0).saturating_add(1);
        let ox =
            p.ox.saturating_sub(i64::from(self.c0).saturating_mul(i64::from(p.cw)));
        let oy =
            p.oy.saturating_sub(i64::from(self.r0).saturating_mul(i64::from(p.ch)));
        (p.col.saturating_add(self.c0), cols, rows, ox, oy)
    }

    /// Whether any of the picture lies inside the piece's box.
    fn shows(&self, p: &Placement) -> bool {
        let (_, cols, rows, ox, oy) = self.cut(p);
        let bw = i64::from(cols).saturating_mul(i64::from(p.cw));
        let bh = i64::from(rows).saturating_mul(i64::from(p.ch));
        let dw = i64::try_from(p.dw).unwrap_or(i64::MAX);
        let dh = i64::try_from(p.dh).unwrap_or(i64::MAX);
        cols > 0
            && rows > 0
            && ox < bw
            && ox.saturating_add(dw) > 0
            && oy < bh
            && oy.saturating_add(dh) > 0
    }
}

impl Store {
    /// Cells `x0` to `x1` of screen row `y` were written or erased: the
    /// placements whose rules say text cuts them lose those cells (D3).
    pub fn cut(&mut self, y: usize, x0: usize, x1: usize, rows: Rows<'_>, cols: usize) {
        let to_u32 = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        if !self.has_cuttable() {
            return;
        }
        self.cut_where(to_u32(y), to_u32(x0), to_u32(x1), rows, cols, |p| {
            rules(p.protocol.0).cut_by_text
        });
    }

    /// Cuts cells `x0` to `x1` of screen row `y` out of every placement of
    /// the buffer in use that `cuts` chooses. The placements covering the
    /// row are anchored at most their tallest's rows less one lines above
    /// it.
    pub(super) fn cut_where(
        &mut self,
        y: u32,
        x0: u32,
        x1: u32,
        rows: Rows<'_>,
        cols: usize,
        cuts: impl Fn(&Placement) -> bool,
    ) {
        let b = self.active();
        let Some(buf) = self.buffers.get(b) else {
            return;
        };
        let y64 = i64::from(y);
        if buf.placements.is_empty() || y64 >= rows.height() || x0 > x1 {
            return;
        }
        let last_col = u32::try_from(cols.saturating_sub(1)).unwrap_or(u32::MAX);
        let x1 = x1.min(last_col);
        // the placements a cut can reach are anchored at most the tallest
        // of them, less one row, above the row
        let (_, tallest) = buf.tallest();
        let mut hits: Vec<(PlacementKey, u32)> = Vec::new();
        let mut visited = 0usize;
        for k in 0..tallest {
            let Some(line) = rows.at(y64.saturating_sub(i64::from(k))) else {
                break;
            };
            visited = visited.saturating_add(1);
            for &key in buf.by_line.get(&line).into_iter().flatten() {
                visited = visited.saturating_add(1);
                let Some(p) = buf.placements.get(&key) else {
                    continue;
                };
                if cuts(p) && p.rows > k && p.col <= x1 && p.col.saturating_add(p.cols) > x0 {
                    hits.push((key, k));
                }
            }
        }
        self.count(visited);
        for (key, k) in hits {
            let Some(p) = self.placement(b, key) else {
                continue;
            };
            let (col, cols) = (p.col, p.cols);
            let ec0 = x0.max(col).saturating_sub(col);
            let ec1 = x1
                .min(col.saturating_add(cols).saturating_sub(1))
                .saturating_sub(col);
            self.cut_box(
                b,
                key,
                y64.saturating_sub(i64::from(k)),
                (k, k),
                (ec0, ec1),
                rows,
            );
        }
    }

    /// Removes box rows `er` and columns `ec` of placement `key`, whose top
    /// is at screen row `top`: what is left above stays on its line, what
    /// is left beside and below is anchored on the screen's lines, and a
    /// piece below the screen has no line to hold it and goes. The pieces
    /// come in D4's order: above, left, right, below.
    fn cut_box(
        &mut self,
        b: usize,
        key: PlacementKey,
        top: i64,
        (er0, er1): (u32, u32),
        (ec0, ec1): (u32, u32),
        rows: Rows<'_>,
    ) {
        let Some(p) = self.placement(b, key) else {
            return;
        };
        let (pcols, prows, anchor) = (p.cols, p.rows, p.anchor);
        let last = pcols.saturating_sub(1);
        let mid = rows.at(top.saturating_add(i64::from(er0)));
        let below_row = top.saturating_add(i64::from(er1)).saturating_add(1);
        let mut pieces = Vec::with_capacity(4);
        if er0 > 0 {
            let r1 = er0.saturating_sub(1);
            pieces.push(Piece {
                r0: 0,
                r1,
                c0: 0,
                c1: last,
                anchor: Anchor::Line(anchor),
            });
        }
        if let Some(mid) = mid {
            if ec0 > 0 {
                let c1 = ec0.saturating_sub(1);
                pieces.push(Piece {
                    r0: er0,
                    r1: er1,
                    c0: 0,
                    c1,
                    anchor: Anchor::Line(mid),
                });
            }
            if ec1.saturating_add(1) < pcols {
                let c0 = ec1.saturating_add(1);
                pieces.push(Piece {
                    r0: er0,
                    r1: er1,
                    c0,
                    c1: last,
                    anchor: Anchor::Line(mid),
                });
            }
        }
        if er1.saturating_add(1) < prows && below_row < rows.height() {
            if let Some(line) = rows.at(below_row) {
                let (r0, r1) = (er1.saturating_add(1), prows.saturating_sub(1));
                pieces.push(Piece {
                    r0,
                    r1,
                    c0: 0,
                    c1: last,
                    anchor: Anchor::Line(line),
                });
            }
        }
        self.split(b, key, &pieces);
    }

    /// Replaces placement `key` by `pieces`: it becomes the first piece
    /// that shows any of the picture, keeping its key and its place in the
    /// stack, and the others are new placements directly above it, each
    /// charged [`PLACEMENT_OVERHEAD`] to settle after the event. Past the
    /// buffer's placement count the largest pieces stay, largest first.
    /// A placement no piece of which shows anything is removed.
    fn split(&mut self, b: usize, key: PlacementKey, pieces: &[Piece]) {
        let Some(o) = self.placement(b, key).cloned() else {
            return;
        };
        let mut shown: Vec<Piece> = pieces.iter().filter(|pc| pc.shows(&o)).copied().collect();
        let count = self.buffers.get(b).map_or(0, |buf| buf.placements.len());
        let room = PLACEMENTS_PER_BUFFER.saturating_sub(u64::try_from(count).unwrap_or(u64::MAX));
        let keep = usize::try_from(room.saturating_add(1)).unwrap_or(usize::MAX);
        if shown.len() > keep {
            shown.sort_by_key(|pc| core::cmp::Reverse(pc.area()));
            shown.truncate(keep);
        }
        if shown.is_empty() {
            self.free_placement(b, key, true);
            return;
        }
        self.changed.push(o.region());
        let mut prev = key;
        for (i, pc) in shown.iter().enumerate() {
            let (col, cols, rows, ox, oy) = pc.cut(&o);
            let revision = self.bump();
            let k = if i == 0 {
                key
            } else {
                let k = PlacementKey(self.next_placement);
                self.next_placement = self.next_placement.saturating_add(1);
                let piece = Placement {
                    attached: false,
                    ..o.clone()
                };
                if let Some(buf) = self.buffers.get_mut(b) {
                    buf.put(k, piece);
                    buf.stack_after(k, prev);
                }
                if let Some(image) = self.image_mut(b, o.image) {
                    image.placements = image.placements.saturating_add(1);
                    image.bytes = image.bytes.saturating_add(PLACEMENT_OVERHEAD);
                    image.owed = image.owed.saturating_add(PLACEMENT_OVERHEAD);
                }
                self.owed = self.owed.saturating_add(PLACEMENT_OVERHEAD);
                prev = k;
                k
            };
            let Some(buf) = self.buffers.get_mut(b) else {
                return;
            };
            buf.count_height(k, false);
            if let Some(p) = buf.placements.get_mut(&k) {
                p.col = col;
                p.cols = cols;
                p.rows = rows;
                p.ox = ox;
                p.oy = oy;
                p.revision = revision;
            }
            buf.count_height(k, true);
            match pc.anchor {
                Anchor::Line(line) => {
                    buf.attach(k, line);
                    if let Some(p) = buf.placements.get(&k) {
                        self.changed.push(p.region());
                    }
                }
                Anchor::Row(row) => {
                    buf.detach(k);
                    self.pending.push((b, k, row));
                }
            }
        }
    }

    /// A scroll of rows `top` to `bottom` of the screen in use by `n`, up
    /// or down, before the lines move (D3, D8). A whole-screen scroll up
    /// that `into_scrollback` sends into the scrollback carries placements
    /// with their lines. In a region, and at the screen's top edge without
    /// a scrollback, a placement whose rules split it keeps the parts
    /// outside the band where they are, moves the part inside, and clips
    /// what leaves; one whose rules move it whole moves only when wholly
    /// inside and otherwise keeps its place on the screen. What moves to a
    /// new line waits for [`scroll_done`](Self::scroll_done).
    pub fn scroll(
        &mut self,
        band: (usize, usize),
        n: usize,
        up: bool,
        into_scrollback: bool,
        rows: Rows<'_>,
    ) {
        let (top, bottom) = band;
        let b = self.active();
        if !self
            .buffers
            .get(b)
            .is_some_and(|buf| !buf.placements.is_empty())
            || bottom < top
        {
            return;
        }
        let as_i64 = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
        let (top, bottom) = (as_i64(top), as_i64(bottom));
        let span = bottom.saturating_sub(top).saturating_add(1);
        let n = as_i64(n).min(span);
        let carry = up && top == 0 && b == MAIN && into_scrollback;
        let lo = if carry { i64::MIN } else { top };
        let hi = if bottom.saturating_add(1) == rows.height() {
            i64::MAX
        } else {
            bottom
        };
        let clip = if up { hi } else { bottom };
        let d = if up { n.saturating_neg() } else { n };
        let tops = self.tops(b, rows);
        for (key, r0) in tops {
            let Some(p) = self.placement(b, key) else {
                continue;
            };
            let (prows, last, anchor, scroll) = (
                i64::from(p.rows),
                p.cols.saturating_sub(1),
                p.anchor,
                rules(p.protocol.0).region_scroll,
            );
            let r1 = r0.saturating_add(prows).saturating_sub(1);
            if r1 < top || r0 > bottom {
                continue;
            }
            if scroll == RegionScroll::WholeOnly && (r0 < lo || r1 > hi) {
                // across an edge: it keeps its place, re-anchored to the
                // line that then holds its top row
                if r0 >= top || carry {
                    if let Some(buf) = self.buffers.get_mut(b) {
                        buf.detach(key);
                    }
                    self.pending.push((b, key, r0));
                }
                continue;
            }
            let to_u32 = |v: i64| u32::try_from(v).unwrap_or(0);
            let mut pieces = Vec::with_capacity(3);
            if r0 < lo {
                let r1 = to_u32(lo.saturating_sub(1).saturating_sub(r0));
                pieces.push(Piece {
                    r0: 0,
                    r1,
                    c0: 0,
                    c1: last,
                    anchor: Anchor::Line(anchor),
                });
            }
            let ia = r0.max(lo).saturating_sub(r0);
            let ib = r1.min(hi).saturating_sub(r0);
            let moved = r0.saturating_add(d);
            let na = if moved.saturating_add(ia) < lo {
                lo.saturating_sub(moved)
            } else {
                ia
            };
            let nb = if moved.saturating_add(ib) > clip {
                clip.saturating_sub(moved)
            } else {
                ib
            };
            if na <= nb {
                let anchor = if na == 0 {
                    Anchor::Line(anchor)
                } else {
                    Anchor::Row(moved.saturating_add(na))
                };
                pieces.push(Piece {
                    r0: to_u32(na),
                    r1: to_u32(nb),
                    c0: 0,
                    c1: last,
                    anchor,
                });
            }
            if r1 > hi && hi.saturating_add(1) < rows.height() {
                if let Some(line) = rows.at(hi.saturating_add(1)) {
                    let r0 = to_u32(hi.saturating_add(1).saturating_sub(r0));
                    let r1 = to_u32(prows.saturating_sub(1));
                    pieces.push(Piece {
                        r0,
                        r1,
                        c0: 0,
                        c1: last,
                        anchor: Anchor::Line(line),
                    });
                }
            }
            self.split(b, key, &pieces);
        }
    }

    /// After a scroll's lines moved: each placement it moved is anchored to
    /// the line now at its row, or goes when no line is there. Rows above
    /// the screen are the scrollback's only on the main screen.
    pub fn scroll_done(&mut self, rows: Rows<'_>) {
        for (b, key, row) in core::mem::take(&mut self.pending) {
            let line = if row >= 0 || b == MAIN {
                rows.at(row)
            } else {
                None
            };
            match line {
                Some(line) => {
                    if let Some(buf) = self.buffers.get_mut(b) {
                        buf.attach(key, line);
                    }
                    if let Some(p) = self.placement(b, key) {
                        self.changed.push(p.region());
                    }
                }
                None => self.free_placement(b, key, true),
            }
        }
    }

    /// A line left the scrollback past `sb-size`: its placements lose that
    /// row and go on from `next`, the line after it (D8).
    pub fn discard_line(&mut self, line: LineKey, next: Option<LineKey>) {
        let b = MAIN;
        let keys = self.anchored(b, line);
        for key in keys {
            let Some(p) = self.placement(b, key) else {
                continue;
            };
            let (prows, last) = (p.rows, p.cols.saturating_sub(1));
            match next {
                Some(next) if prows >= 2 => {
                    let r1 = prows.saturating_sub(1);
                    let piece = Piece {
                        r0: 1,
                        r1,
                        c0: 0,
                        c1: last,
                        anchor: Anchor::Line(next),
                    };
                    self.split(b, key, &[piece]);
                }
                _ => self.free_placement(b, key, true),
            }
        }
    }

    /// A line was freed, by ED 3, by a scroll with nowhere to keep it, or
    /// by a rewrap: the placements anchored on it go, in either buffer.
    pub fn free_line(&mut self, line: LineKey) {
        for b in [MAIN, ALT] {
            for key in self.anchored(b, line) {
                self.free_placement(b, key, true);
            }
        }
    }

    fn anchored(&self, b: usize, line: LineKey) -> Vec<PlacementKey> {
        self.buffers
            .get(b)
            .and_then(|buf| buf.by_line.get(&line))
            .cloned()
            .unwrap_or_default()
    }

    /// ED 2, after its erase cut what text cuts: every placement with a row
    /// on the screen's own lines whose rules say ED 2 removes it goes, its
    /// image kept as its rules say (D8).
    pub fn erase_screen(&mut self, rows: Rows<'_>) {
        let b = self.active();
        let height = rows.height();
        let tops = self.tops(b, rows);
        for (key, top) in tops {
            let Some(p) = self.placement(b, key) else {
                continue;
            };
            let on = top.saturating_add(i64::from(p.rows)) > 0 && top < height;
            if on && rules(p.protocol.0).erased_by_clear_screen {
                self.free_placement(b, key, true);
            }
        }
    }

    /// Every line the main buffer's placements are anchored on, each with
    /// their columns.
    pub fn main_anchor_lines(&self) -> Vec<(LineKey, Vec<u32>)> {
        let Some(buf) = self.buffers.get(MAIN) else {
            return Vec::new();
        };
        buf.by_line
            .iter()
            .map(|(&line, keys)| {
                let cols = keys
                    .iter()
                    .filter_map(|k| buf.placements.get(k).map(|p| p.col))
                    .collect();
                (line, cols)
            })
            .collect()
    }

    /// The main buffer's placements anchored on `line`, each with its
    /// column, for a rewrap to carry (ADR-1003).
    pub fn anchors_on(&self, line: LineKey) -> Vec<(PlacementKey, u32)> {
        let Some(buf) = self.buffers.get(MAIN) else {
            return Vec::new();
        };
        buf.by_line
            .get(&line)
            .into_iter()
            .flatten()
            .filter_map(|k| buf.placements.get(k).map(|p| (*k, p.col)))
            .collect()
    }

    /// A rewrap carried the main buffer's placement `key` to `to`, a line
    /// and a column, or to no line, when it goes.
    pub fn move_anchor(&mut self, key: PlacementKey, to: Option<(LineKey, u32)>) {
        match to {
            Some((line, col)) => {
                let revision = self.bump();
                let Some(buf) = self.buffers.get_mut(MAIN) else {
                    return;
                };
                let Some(p) = buf.placements.get_mut(&key) else {
                    return;
                };
                let old = p.region();
                p.col = col;
                p.revision = revision;
                buf.attach(key, line);
                self.changed.push(old);
                if let Some(p) = self.placement(MAIN, key) {
                    self.changed.push(p.region());
                }
            }
            None => self.free_placement(MAIN, key, true),
        }
    }

    /// Grants what the pieces a screen event cut owe, or drops the smallest
    /// of them (D2). The terminal calls it after each screen event, so the
    /// state follows the events and never how the input was chunked.
    pub fn settle_event(&mut self, screens: Screens<'_>) {
        if self.owed > 0 {
            self.settle(screens);
        }
    }
}
