//! Where the box of a glyph whose ink passes its cells holds ink, by the
//! cells of its slot (ADR-1306 D3, D9), from which the ink grid sets the
//! line bits of the cells the glyph's ink reaches.

use super::gl::Size;

/// Where a glyph's box holds ink, by the cells of its slot (ADR-1306 D3):
/// for each cell of the slot, which of its rows of pixels and which of its
/// columns hold any coverage. A cell's line bits follow from it, so they
/// are worked out once, when the glyph takes its box.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BoxLines {
    /// The slot's cells across.
    across: usize,
    /// The cell's width and height.
    cell: (usize, usize),
    /// For each cell of the slot, a row at a time, whether each of its
    /// rows of pixels holds ink.
    rows: Vec<bool>,
    /// For each cell of the slot, whether each of its columns holds ink.
    columns: Vec<bool>,
}

impl BoxLines {
    /// The lines of the box whose coverage is `pixels`, `slot` in size, a
    /// line after the other, in cells of `cell`.
    pub(crate) fn new(pixels: &[u8], slot: Size, cell: Size) -> BoxLines {
        let n = |v: u32| usize::try_from(v).unwrap_or(0);
        let (cw, ch) = (n(cell.width), n(cell.height));
        let across = n(slot.width).checked_div(cw).unwrap_or(0);
        let down = n(slot.height).checked_div(ch).unwrap_or(0);
        let cells = across.saturating_mul(down);
        let mut lines = BoxLines {
            across,
            cell: (cw, ch),
            rows: vec![false; cells.saturating_mul(ch)],
            columns: vec![false; cells.saturating_mul(cw)],
        };
        let stride = n(slot.width);
        for (y, line) in pixels.chunks(stride.max(1)).enumerate() {
            let (by, py) = (y.checked_div(ch), y.checked_rem(ch));
            let (Some(by), Some(py)) = (by, py) else {
                continue;
            };
            for (x, &a) in line.iter().enumerate() {
                let (Some(bx), Some(px)) = (x.checked_div(cw), x.checked_rem(cw)) else {
                    continue;
                };
                if a == 0 {
                    continue;
                }
                let i = by.saturating_mul(across).saturating_add(bx);
                if let Some(r) = lines.rows.get_mut(i.saturating_mul(ch).saturating_add(py)) {
                    *r = true;
                }
                if let Some(c) = lines
                    .columns
                    .get_mut(i.saturating_mul(cw).saturating_add(px))
                {
                    *c = true;
                }
            }
        }
        lines
    }

    /// Whether the slot's cell at `bx`, `by` holds ink on each of its
    /// display lines, its rows of pixels or, `down`, its columns, ORed
    /// into `out`; nothing for a cell outside the slot.
    pub(super) fn or_into(&self, (bx, by): (i64, i64), down: bool, out: &mut [bool]) {
        let (Ok(bx), Ok(by)) = (usize::try_from(bx), usize::try_from(by)) else {
            return;
        };
        if bx >= self.across {
            return;
        }
        let (lines, n) = match down {
            true => (&self.columns, self.cell.0),
            false => (&self.rows, self.cell.1),
        };
        let at = by
            .saturating_mul(self.across)
            .saturating_add(bx)
            .saturating_mul(n);
        let Some(cell) = lines.get(at..at.saturating_add(n)) else {
            return;
        };
        for (o, &inked) in out.iter_mut().zip(cell) {
            *o |= inked;
        }
    }
}
