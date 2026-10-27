//! Rectangular areas: DECERA, DECSERA, DECFRA and DECCRA, and the
//! attribute changes of DECCARA and DECRARA in the extent DECSACE selects.
//!
//! A rectangle's corners count from 1, as DEC STD 070 and xterm's ctlseqs
//! give them, from the top margin in origin mode; an omitted corner is the
//! screen's edge, or the margin, and a corner past it is the edge. Each
//! operation is linear in the rectangle's cells, and DECCRA holds one
//! source line at a time.
//!
//! A wide character the rectangle's edge cuts goes whole, as an edit's
//! does (`edit.rs`); DECCARA and DECRARA change it whole. An operation that
//! reaches a row's last column ends the row's wrap, as EL 0 does, and one
//! that replaces the main screen's first row whole ends the scrollback's
//! newest line's (`edit.rs`); DECCARA and DECRARA change no text and keep
//! both.

use super::{Erase, Screen};
use crate::cell::{Content, Width};
use crate::line::Glyph;
use crate::style::Style;

/// A rectangle of the screen: rows `top..=bottom` and columns
/// `left..=right`, inside the screen, never inverted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    top: usize,
    left: usize,
    bottom: usize,
    right: usize,
}

/// DECSACE: which cells DECCARA and DECRARA change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Extent {
    /// The cells from the first corner to the second in reading order,
    /// each row after the first from the screen's first column and each
    /// before the last to its last, where a character was printed (DECSACE
    /// 0 and 1).
    #[default]
    Stream,
    /// Every cell of the rectangle (DECSACE 2).
    Rectangle,
}

/// What a cell shows, owned, so a cell can be rewritten from its own line.
enum Shown {
    Empty,
    Char(char),
    Cluster,
}

impl Screen {
    /// The rectangle with corners `top`, `left`, `bottom` and `right`,
    /// counted from 1 and 0 when omitted; `None` when it is inverted.
    pub(crate) fn rect(
        &self,
        top: usize,
        left: usize,
        bottom: usize,
        right: usize,
    ) -> Option<Rect> {
        let (first, last) = self.rect_rows();
        let last_col = self.cols.saturating_sub(1);
        let col = |n: usize, default: usize| n.checked_sub(1).map_or(default, |n| n.min(last_col));
        let rect = Rect {
            top: self.rect_row(top, first),
            left: col(left, 0),
            bottom: self.rect_row(bottom, last),
            right: col(right, last_col),
        };
        (rect.top <= rect.bottom && rect.left <= rect.right).then_some(rect)
    }

    /// The rows a rectangle may take: the screen's, or the region's in
    /// origin mode.
    fn rect_rows(&self) -> (usize, usize) {
        if self.flags.origin {
            (self.top, self.bottom)
        } else {
            (0, self.rows.saturating_sub(1))
        }
    }

    /// Row `n` of a rectangle, counted from 1 and 0 when omitted, which
    /// is `default`.
    fn rect_row(&self, n: usize, default: usize) -> usize {
        let (first, last) = self.rect_rows();
        n.checked_sub(1)
            .map_or(default, |n| first.saturating_add(n).min(last))
    }

    /// DECSACE's extent.
    pub(crate) fn attribute_extent(&self) -> Extent {
        self.extent
    }

    pub(crate) fn set_attribute_extent(&mut self, extent: Extent) {
        self.extent = extent;
    }

    /// DECERA, and with `Erase::Unprotected` DECSERA: erases the
    /// rectangle's cells as ED erases them.
    pub(crate) fn erase_rect(&mut self, rect: Rect, which: Erase) {
        for y in rect.top..=rect.bottom {
            self.erase_region((rect.left, y), (rect.right, y), which);
        }
        self.end_rect_wraps(rect, false);
    }

    /// DECFRA: fills the rectangle with `c`, one cell wide, in the pen and
    /// the hyperlink printed text takes.
    pub(crate) fn fill_rect(&mut self, rect: Rect, c: char) {
        self.segment = None;
        let (pen, link) = (self.pen, self.link);
        for y in rect.top..=rect.bottom {
            self.split_wide(y, rect.left);
            self.split_wide(y, rect.right.saturating_add(1));
            if let Some(line) = self.line_mut(y) {
                for x in rect.left..=rect.right {
                    line.write(x, Glyph::Char(c), Width::Narrow, &pen, link);
                }
            }
            self.mark_cells(y, rect.left..rect.right.saturating_add(1));
        }
        self.end_rect_wraps(rect, true);
    }

    /// DECCRA: copies `source` with each cell's style and hyperlink to the
    /// rectangle of its size whose top left is row `top`, column `left`,
    /// counted as a rectangle's corners are and clipped to the screen, or
    /// the region in origin mode. The copy reads the source as it was
    /// before any of it is written. Half a wide character the source's
    /// edge cuts, or the target's clips, arrives as an empty cell in its
    /// style.
    pub(crate) fn copy_rect(&mut self, source: Rect, top: usize, left: usize) {
        let (first, last) = self.rect_rows();
        let last_col = self.cols.saturating_sub(1);
        let top = self.rect_row(top, first);
        let left = left.saturating_sub(1).min(last_col);
        let height = source.bottom.saturating_sub(source.top);
        let width = source.right.saturating_sub(source.left);
        let target = Rect {
            top,
            left,
            bottom: top.saturating_add(height).min(last),
            right: left.saturating_add(width).min(last_col),
        };
        self.segment = None;
        let rows = target.bottom.saturating_sub(target.top);
        // Rows go in the order that reads each source row before the copy
        // writes over it.
        let down = target.top > source.top;
        for i in 0..=rows {
            let k = if down { rows.saturating_sub(i) } else { i };
            let y = target.top.saturating_add(k);
            let Some(from) = self.line(source.top.saturating_add(k)).cloned() else {
                continue;
            };
            self.split_wide(y, target.left);
            self.split_wide(y, target.right.saturating_add(1));
            let Some(line) = self.line_mut(y) else {
                continue;
            };
            for (x, sx) in (target.left..=target.right).zip(source.left..) {
                let cell = from.cell(sx);
                let cut = match cell.width() {
                    Width::Wide => x == target.right,
                    Width::WideTail => x == target.left,
                    Width::Narrow => false,
                };
                let style = from.style(cell);
                let link = from.link(cell);
                if cut {
                    line.write(x, Glyph::Empty, Width::Narrow, &style, link);
                } else {
                    line.write(x, from.glyph(cell), cell.width(), &style, link);
                }
            }
            self.mark_cells(y, target.left..target.right.saturating_add(1));
        }
        self.end_rect_wraps(target, true);
    }

    /// DECCARA and DECRARA: gives each cell of the rectangle, in DECSACE's
    /// extent, the style `change` makes of its own. Marks the cells whose
    /// style changed.
    pub(crate) fn change_attrs(&mut self, rect: Rect, mut change: impl FnMut(Style) -> Style) {
        self.segment = None;
        let last_col = self.cols.saturating_sub(1);
        let stream = self.extent == Extent::Stream;
        // Neighbouring cells mostly share a style.
        let mut last: Option<(Style, Style)> = None;
        let mut cluster = String::new();
        for y in rect.top..=rect.bottom {
            let (mut from, mut to) = if stream {
                (
                    if y == rect.top { rect.left } else { 0 },
                    if y == rect.bottom {
                        rect.right
                    } else {
                        last_col
                    },
                )
            } else {
                (rect.left, rect.right)
            };
            let Some(line) = self.line_mut(y) else {
                continue;
            };
            if from > 0 && line.cell(from).width() == Width::WideTail {
                from = from.saturating_sub(1);
            }
            if line.cell(to).width() == Width::Wide
                && line.cell(to.saturating_add(1)).width() == Width::WideTail
            {
                to = to.saturating_add(1);
            }
            let mut changed: Option<(usize, usize)> = None;
            for x in from..=to {
                let cell = line.cell(x);
                if stream && cell.content() == Content::Empty && cell.width() != Width::WideTail {
                    continue;
                }
                let old = line.style(cell);
                let new = match last {
                    Some((was, now)) if was == old => now,
                    _ => change(old),
                };
                last = Some((old, new));
                if new == old {
                    continue;
                }
                let shown = match line.glyph(cell) {
                    Glyph::Empty => Shown::Empty,
                    Glyph::Char(c) => Shown::Char(c),
                    Glyph::Cluster(s) => {
                        cluster.clear();
                        cluster.push_str(s);
                        Shown::Cluster
                    }
                };
                let glyph = match shown {
                    Shown::Empty => Glyph::Empty,
                    Shown::Char(c) => Glyph::Char(c),
                    Shown::Cluster => Glyph::Cluster(&cluster),
                };
                let link = line.link(cell);
                line.write(x, glyph, cell.width(), &new, link);
                changed = Some(changed.map_or((x, x), |(a, _)| (a, x)));
            }
            if let Some((a, b)) = changed {
                self.mark_cells(y, a..b.saturating_add(1));
            }
        }
    }

    /// Ends the wrap of each row of `rect` when it reaches the last
    /// column, and, when `replaced` says each of its rows was written
    /// whole and it spans the first row, the scrollback's newest line's.
    fn end_rect_wraps(&mut self, rect: Rect, replaced: bool) {
        let last_col = self.cols.saturating_sub(1);
        if rect.right < last_col {
            return;
        }
        self.unwrap_rows(rect.top..=rect.bottom);
        if replaced && rect.top == 0 && rect.left == 0 {
            self.unwrap_scrollback();
        }
    }
}
