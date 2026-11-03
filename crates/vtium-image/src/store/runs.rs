//! Kitty's Unicode placeholders on the store's side (ADR-1100, D13).
//!
//! A virtual placement lies on no line: a program shows it by writing
//! placeholder cells into the text, and vtium-term, which owns the cells,
//! reads them into *runs*. A run is a stretch of side-by-side placeholder
//! cells on one row that name one image and one placement, lie on one row
//! of the picture and show consecutive columns of it. The store never sees
//! a cell; it takes runs at two times:
//!
//! - **For a frame,** [`Store::frame_with_runs`] turns each run on the rows
//!   the frame shows into an ordinary frame entry: the part of the
//!   virtual placement's picture, fitted to its grid of cells, that lies
//!   under the run's cells. A relative placement whose parent is virtual
//!   draws from the origin the parent's runs give.
//! - **For a decision,** a kitty delete by cell, row or column or of
//!   every placement on the screen, ED 2, or a reservation that may evict,
//!   [`Store::with_runs`] holds the runs read
//!   from the screen's own lines while the decision runs, so a virtual
//!   parent's children are deleted and ranked by where they draw there.
//!
//! The store tells vtium-term which images' placeholder cells to mark
//! changed through [`Store::take_changed_virtual`].

use std::collections::BTreeMap;

use super::relative::Located;
use super::{ALT, Buffer, Placement, Rows, Screens, Spot, Store};
use crate::bounds::IMAGES_PER_BUFFER;
use crate::image::{LineKey, PlaceholderRun, PlacementKey};
use crate::rules::rules;

#[cfg(test)]
mod tests;

/// The first key a frame entry made from a run takes; each entry of one
/// frame takes the next. A placement's own key is counted up from 0 and
/// never reaches it.
pub(super) const RUN_KEY_FIRST: u64 = 1 << 63;

/// The virtual placements of one buffer by the kitty ids that runs name
/// them by.
pub(super) struct Virtuals {
    /// By image id and placement id.
    named: BTreeMap<(u32, u32), PlacementKey>,
    /// By image id: the one made last.
    last: BTreeMap<u32, (u64, PlacementKey)>,
}

impl Virtuals {
    /// The index of `buf`'s virtual placements, in one pass over its
    /// placements.
    pub(super) fn of(buf: &Buffer) -> Self {
        let mut named = BTreeMap::new();
        let mut last: BTreeMap<u32, (u64, PlacementKey)> = BTreeMap::new();
        for (&k, p) in &buf.placements {
            if p.spot != Spot::Virtual {
                continue;
            }
            let Some(image) = buf.images.get(&p.image) else {
                continue;
            };
            if !rules(image.protocol.0).ids || image.id == 0 {
                continue;
            }
            if p.id != 0 {
                named.insert((image.id, p.id), k);
            }
            let newer = last.get(&image.id).is_none_or(|&(seq, _)| p.seq > seq);
            if newer {
                last.insert(image.id, (p.seq, k));
            }
        }
        Self { named, last }
    }

    /// The virtual placement `run` names: the one of its image and
    /// placement id, or with no placement id the one of its image made
    /// last, as the spec leaves the choice to the terminal.
    pub(super) fn find(&self, run: &PlaceholderRun) -> Option<PlacementKey> {
        match run.placement.filter(|&p| p != 0) {
            Some(p) => self.named.get(&(run.image, p)).copied(),
            None => self.last.get(&run.image).map(|&(_, k)| k),
        }
    }
}

/// The origin of each virtual placement that `runs` show: the least row
/// and the least column over its runs' cells, each cell's own row and
/// column in the picture taken off first (D13). `row_of` gives a run's
/// line's row, or `None` for a line the rows given do not hold.
pub(super) fn origins(
    virtuals: &Virtuals,
    runs: &[PlaceholderRun],
    row_of: impl Fn(LineKey) -> Option<i64>,
) -> BTreeMap<PlacementKey, (i64, i64)> {
    let mut out: BTreeMap<PlacementKey, (i64, i64)> = BTreeMap::new();
    for run in runs {
        let (Some(key), Some(row)) = (virtuals.find(run), row_of(run.line)) else {
            continue;
        };
        let top = row.saturating_sub(i64::from(run.row));
        let left = i64::from(run.col).saturating_sub(i64::from(run.column));
        out.entry(key)
            .and_modify(|o| *o = (o.0.min(top), o.1.min(left)))
            .or_insert((top, left));
    }
    out
}

/// Virtual placement `p` as the entry run `run` draws, with the run's first
/// cell at screen column `run.col`, on a screen `cols` wide: the part of
/// the run's cells inside the placement's grid, its picture moved by the
/// cells of the grid above and left of the run. `None` when no part of the
/// run lies in the grid or on the screen, or when the fitted picture
/// covers none of its cells; the picture lies inside its grid, so a run on
/// a picture row past the grid's rows covers none.
pub(super) fn run_placement(p: &Placement, run: &PlaceholderRun, cols: usize) -> Option<Placement> {
    let on_screen = usize::try_from(run.col)
        .ok()
        .and_then(|c| cols.checked_sub(c))
        .and_then(|left| u32::try_from(left).ok())?;
    let cells = run
        .cells
        .min(on_screen)
        .min(p.cols.saturating_sub(run.column));
    if cells == 0 {
        return None;
    }
    let (cw, ch) = (i64::from(p.cw), i64::from(p.ch));
    let ox =
        p.ox.saturating_sub(i64::from(run.column).saturating_mul(cw));
    let oy = p.oy.saturating_sub(i64::from(run.row).saturating_mul(ch));
    let (dw, dh) = (
        i64::try_from(p.dw).unwrap_or(i64::MAX),
        i64::try_from(p.dh).unwrap_or(i64::MAX),
    );
    let width = i64::from(cells).saturating_mul(cw);
    let covers = ox < width && ox.saturating_add(dw) > 0 && oy < ch && oy.saturating_add(dh) > 0;
    covers.then(|| Placement {
        col: run.col,
        cols: cells,
        rows: 1,
        ox,
        oy,
        ..p.clone()
    })
}

impl Store {
    /// The origins of buffer `b`'s virtual placements that the runs held
    /// for the decision under way show on `rows`, the screen's own lines;
    /// empty when no runs are held.
    pub(super) fn decision_origins(
        &mut self,
        b: usize,
        rows: Rows<'_>,
    ) -> BTreeMap<PlacementKey, (i64, i64)> {
        let (Some(runs), Some(buf)) = (self.runs.get(b), self.buffers.get(b)) else {
            return BTreeMap::new();
        };
        if runs.is_empty() || buf.children.is_empty() {
            return BTreeMap::new();
        }
        let virtuals = Virtuals::of(buf);
        let screen: BTreeMap<LineKey, i64> = rows.screen.iter().copied().zip(0i64..).collect();
        let out = origins(&virtuals, runs, |line| screen.get(&line).copied());
        let visited = buf
            .placements
            .len()
            .saturating_add(runs.len())
            .saturating_add(screen.len());
        self.count(visited);
        out
    }

    /// Runs `f` over the store with `runs`, the placeholder runs vtium-term
    /// read from the screen's own lines of the main and the alternate
    /// buffer, held for the decisions `f` makes that take a virtual parent's
    /// children by the cells they draw on: a kitty delete by cell, row or
    /// column or of every placement on the screen (`d=a`), ED 2, and a
    /// reservation that may evict (D13). The store
    /// takes at most one run for each cell of `screens`' screen rows, `cols`
    /// wide, counts each run it takes as input work, and keeps none past
    /// the call. vtium-term reads the runs only while
    /// [`has_virtual_children`](Self::has_virtual_children) says a buffer
    /// needs them.
    pub fn with_runs<R>(
        &mut self,
        runs: [&[PlaceholderRun]; 2],
        screens: Screens<'_>,
        cols: usize,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let mut taken = 0usize;
        for (b, given) in runs.into_iter().enumerate() {
            let cap = screens.of(b).screen.len().saturating_mul(cols);
            let held: Vec<PlaceholderRun> = given.iter().take(cap).copied().collect();
            taken = taken.saturating_add(held.len());
            if let Some(slot) = self.runs.get_mut(b) {
                *slot = held;
            }
        }
        self.count(taken);
        let out = f(self);
        self.runs = [Vec::new(), Vec::new()];
        out
    }

    /// Whether the alternate buffer, or with `alt` false the main one,
    /// holds a relative placement whose chain of parents ends at a virtual
    /// placement, whose cells only runs give. Every such chain starts with
    /// a child of the virtual placement itself, and the buffer counts those
    /// children as they are linked and unlinked, so the answer costs no
    /// walk.
    pub fn has_virtual_children(&self, alt: bool) -> bool {
        let b = if alt { ALT } else { super::MAIN };
        self.buffers.get(b).is_some_and(|buf| {
            debug_assert_eq!(
                buf.virtual_children > 0,
                buf.children
                    .values()
                    .flatten()
                    .any(|&k| matches!(buf.locate(k), Some(Located::Floating { .. }))),
                "the count of virtual placements' children follows the chains"
            );
            buf.virtual_children > 0
        })
    }

    /// The buffer, `true` for the alternate, and the kitty image id of each
    /// virtual placement made, replaced or deleted since the last call,
    /// and of each image with one that was sent again, each once, in order:
    /// vtium-term marks changed every placeholder cell of each (D13).
    pub fn take_changed_virtual(&mut self) -> Vec<(bool, u32)> {
        core::mem::take(&mut self.changed_virtual)
            .into_iter()
            .collect()
    }

    /// Notes that virtual placement `p` of buffer `b` changed, for
    /// [`take_changed_virtual`](Self::take_changed_virtual).
    pub(super) fn virtual_changed(&mut self, b: usize, p: &Placement) {
        if p.spot != Spot::Virtual {
            return;
        }
        let id = self
            .buffers
            .get(b)
            .and_then(|buf| buf.images.get(&p.image))
            .map_or(0, |i| i.id);
        self.note_virtual(b, id);
    }

    /// Notes that the placeholder cells of kitty image `id` in buffer `b`
    /// changed. The notes hold at most as many images as both buffers can
    /// hold at once, twice [`IMAGES_PER_BUFFER`], so they stay bounded when
    /// nobody takes them; one event changes at most the images it touches,
    /// so a store whose changes are taken after every event never meets the
    /// bound.
    pub(super) fn note_virtual(&mut self, b: usize, id: u32) {
        let bound = usize::try_from(IMAGES_PER_BUFFER.saturating_mul(2)).unwrap_or(usize::MAX);
        if id != 0 && self.changed_virtual.len() < bound {
            self.changed_virtual.insert((b == ALT, id));
        }
    }
}
