//! The two kinds of kitty placement that have no line of their own
//! (ADR-1100, D2 and D13).
//!
//! A relative placement follows another placement, its parent: its box's
//! top left is its parent's origin plus an offset in cells, where a
//! placement's origin is the cell that held its box's top left as it was
//! placed. It is worked out from the parent each time it is needed, so a
//! scroll, the scrollback, a rewrap, a region scroll and a replacement
//! that move the parent move it too, and the store never moves it itself.
//! A chain of relative placements ends at a placement on a line, or at a
//! virtual placement, which lies on no line and is drawn only where a
//! program writes placeholder cells into the text.
//!
//! A relative placement lives as long as its parent: whatever removes the
//! parent removes its children, and theirs in turn, and a child's image
//! left with no placement goes with it, as the kitty spec asks. A chain is
//! at most [`RELATIVE_DEPTH_MAX`] relative placements deep, and none makes
//! a cycle.

use std::collections::BTreeMap;

use super::{Buffer, Region, Rows, Store};
use crate::bounds::RELATIVE_DEPTH_MAX;
use crate::image::{LineKey, PlacementKey, Refusal};

/// Where a placement is, or what it follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spot {
    /// Anchored to the line holding its box's top row, at its column.
    Line(LineKey),
    /// A relative placement: its box's top left is `cols` columns right and
    /// `rows` rows down from its parent's origin, negative values left and
    /// up.
    Child {
        parent: PlacementKey,
        cols: i32,
        rows: i32,
    },
    /// A virtual placement, which placeholder cells show: on no line.
    Virtual,
}

/// What a kitty put asks a placement to be, for
/// [`Store::place_at`](super::Store::place_at).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    /// At the screen cell the put names.
    Cell,
    /// A relative placement of the placement of kitty image id `image`
    /// whose placement id is `placement`, `cols` columns and `rows` rows
    /// from its origin. A `placement` of 0 names that image's placement
    /// made last among those without a placement id.
    Parent {
        image: u32,
        placement: u32,
        cols: i32,
        rows: i32,
    },
    /// A virtual placement, for Unicode placeholders.
    Virtual,
}

/// Why the store refused a placement: one of its usual refusals, or one of
/// the three a relative placement can meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placing {
    Refused(Refusal),
    /// The parent does not exist (kitty's ENOPARENT).
    NoParent,
    /// The placement would follow itself (kitty's ECYCLE).
    Cycle,
    /// The chain would pass [`RELATIVE_DEPTH_MAX`] (kitty's ETOODEEP).
    TooDeep,
}

impl From<Refusal> for Placing {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}

impl Placing {
    /// The refusal a caller that makes no relative placement sees.
    pub(super) fn refusal(self) -> Refusal {
        match self {
            Self::Refused(refusal) => refusal,
            Self::NoParent | Self::Cycle | Self::TooDeep => Refusal::Invalid,
        }
    }
}

/// Where a placement's box is, as its chain of parents gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Located {
    /// The box's top left is `row` rows below the line `line` holds,
    /// negative above it, at column `col`, negative left of the screen.
    /// `root` is the first placement up the chain that is on a line, the
    /// one anchored on `line`, and `revision` the latest revision along the
    /// chain.
    At {
        root: PlacementKey,
        line: LineKey,
        row: i64,
        col: i64,
        revision: u64,
    },
    /// It is the virtual placement `root` or follows it: only placeholder
    /// runs give it a place. Its box's top left is `row` rows and `col`
    /// columns from the origin the runs of `root` give, and `revision` is
    /// the latest revision along the chain.
    Floating {
        root: PlacementKey,
        row: i64,
        col: i64,
        revision: u64,
    },
}

/// Where a relative placement draws on the screen in use, for the store's
/// own decisions: eviction, ED 2 and kitty's deletes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Drawn {
    /// Its box's top left: the screen row, when the line its root is on is
    /// among the rows the event gave, or when the runs handed with the
    /// decision show its virtual root, and the column.
    At { top: Option<i64>, col: i64 },
    /// It follows a virtual placement that no run handed with the decision
    /// shows: it has no cells the store knows.
    Nowhere,
}

impl Buffer {
    /// Where placement `key`'s box is. `None` for a placement the buffer
    /// lacks, or a chain longer than any the store makes.
    pub(super) fn locate(&self, key: PlacementKey) -> Option<Located> {
        let p = self.placements.get(&key)?;
        let mut revision = p.revision;
        let (mut parent, mut col, mut row) = match p.spot {
            Spot::Line(line) => {
                return Some(Located::At {
                    root: key,
                    line,
                    row: 0,
                    col: i64::from(p.col),
                    revision,
                });
            }
            Spot::Virtual => {
                return Some(Located::Floating {
                    root: key,
                    row: 0,
                    col: 0,
                    revision,
                });
            }
            Spot::Child { parent, cols, rows } => (parent, i64::from(cols), i64::from(rows)),
        };
        for _ in 0..RELATIVE_DEPTH_MAX {
            let q = self.placements.get(&parent)?;
            revision = revision.max(q.revision);
            match q.spot {
                Spot::Line(line) => {
                    return Some(Located::At {
                        root: parent,
                        line,
                        row: row.saturating_sub(i64::from(q.lost_rows)),
                        col: i64::from(q.col)
                            .saturating_sub(i64::from(q.lost_cols))
                            .saturating_add(col),
                        revision,
                    });
                }
                Spot::Virtual => {
                    return Some(Located::Floating {
                        root: parent,
                        row,
                        col,
                        revision,
                    });
                }
                Spot::Child {
                    parent: next,
                    cols,
                    rows,
                } => {
                    col = col.saturating_add(i64::from(cols));
                    row = row.saturating_add(i64::from(rows));
                    parent = next;
                }
            }
        }
        None
    }

    /// The box placement `key` covers, for the terminal to mark: a part
    /// left of the screen is cut off. `None` for a virtual placement and
    /// those that follow one.
    pub(super) fn region(&self, key: PlacementKey) -> Option<Region> {
        let p = self.placements.get(&key)?;
        if let Some(region) = p.own_region() {
            return Some(region);
        }
        let Some(Located::At { line, row, col, .. }) = self.locate(key) else {
            return None;
        };
        let left = u32::try_from(col.min(0).unsigned_abs()).unwrap_or(u32::MAX);
        Some(Region {
            line,
            row,
            col: u32::try_from(col.max(0)).unwrap_or(u32::MAX),
            cols: p.cols.saturating_sub(left),
            rows: p.rows,
        })
    }

    /// Placement `key` and every relative placement that follows it, its
    /// children before their own children.
    pub(super) fn family(&self, key: PlacementKey) -> Vec<PlacementKey> {
        let mut out = vec![key];
        let mut i = 0usize;
        while let Some(&k) = out.get(i) {
            if out.len() > self.placements.len() {
                break;
            }
            out.extend(self.children.get(&k).into_iter().flatten().copied());
            i = i.saturating_add(1);
        }
        out
    }

    /// Takes relative placement `key` out of its parent's list of children.
    pub(super) fn unlink(&mut self, key: PlacementKey) {
        let Some(Spot::Child { parent, .. }) = self.placements.get(&key).map(|p| p.spot) else {
            return;
        };
        if let Some(list) = self.children.get_mut(&parent) {
            list.retain(|&k| k != key);
            if list.is_empty() {
                self.children.remove(&parent);
            }
        }
    }

    /// The levels of relative placements below placement `key`: 0 when it
    /// has no children.
    fn height(&self, key: PlacementKey) -> u64 {
        let mut level = vec![key];
        let mut height = 0u64;
        while height <= RELATIVE_DEPTH_MAX {
            let next: Vec<PlacementKey> = level
                .iter()
                .flat_map(|k| self.children.get(k).into_iter().flatten().copied())
                .collect();
            if next.is_empty() {
                break;
            }
            height = height.saturating_add(1);
            level = next;
        }
        height
    }

    /// The scrollback rows above the screen a frame or an event must be
    /// given: the tallest placement on a line, less one, or the furthest a
    /// relative placement draws below the line its root is anchored on,
    /// less one.
    pub(super) fn reach(&self) -> usize {
        let (tallest, _) = self.tallest();
        let mut most = i64::from(tallest).saturating_sub(1);
        for &k in self.children.values().flatten() {
            let (Some(Located::At { row, .. }), Some(p)) =
                (self.locate(k), self.placements.get(&k))
            else {
                continue;
            };
            most = most.max(row.saturating_add(i64::from(p.rows)).saturating_sub(1));
        }
        usize::try_from(most.max(0)).unwrap_or(usize::MAX)
    }
}

impl Buffer {
    /// The most rows any relative placement starts above the line its root
    /// is anchored on, 0 when none does.
    pub(super) fn reach_below(&self) -> usize {
        let mut most = 0i64;
        for &k in self.children.values().flatten() {
            if let Some(Located::At { row, .. }) = self.locate(k) {
                most = most.max(row.saturating_neg());
            }
        }
        usize::try_from(most).unwrap_or(usize::MAX)
    }
}

impl Store {
    /// The parent a relative placement names in buffer `b`: the placement
    /// of the image of kitty id `id` whose placement id is `placement`, or
    /// with a `placement` of 0 the one of that image made last among those
    /// with no placement id, the spec leaving the choice open (D13).
    pub(super) fn find_parent(
        &mut self,
        b: usize,
        id: u32,
        placement: u32,
    ) -> Result<PlacementKey, Placing> {
        let Some(buf) = self.buffers.get(b) else {
            return Err(Placing::NoParent);
        };
        let visited = buf.images.len().saturating_add(buf.placements.len());
        let found = buf.find_id(id).and_then(|image| {
            buf.placements
                .iter()
                .filter(|(_, p)| p.image == image && p.id == placement)
                .map(|(&k, _)| k)
                .next_back()
        });
        self.count(visited);
        found.ok_or(Placing::NoParent)
    }

    /// Whether a relative placement of `parent`, which replaces placement
    /// `replaced` when that is given, may be made in buffer `b`: it may not
    /// follow itself, through `parent` or any placement `parent` follows,
    /// and neither it nor any placement that follows it may lie more than
    /// [`RELATIVE_DEPTH_MAX`] relative placements below the chain's root.
    pub(super) fn check_chain(
        &mut self,
        b: usize,
        parent: PlacementKey,
        replaced: Option<PlacementKey>,
    ) -> Result<(), Placing> {
        let Some(buf) = self.buffers.get(b) else {
            return Err(Placing::NoParent);
        };
        // the new placement's own level, and one more for each parent up
        // the chain that is itself relative
        let mut depth = 1u64;
        let mut at = parent;
        let mut visited = 0usize;
        loop {
            visited = visited.saturating_add(1);
            if replaced == Some(at) {
                self.count(visited);
                return Err(Placing::Cycle);
            }
            let Some(Spot::Child { parent: next, .. }) = buf.placements.get(&at).map(|p| p.spot)
            else {
                break;
            };
            depth = depth.saturating_add(1);
            if depth > RELATIVE_DEPTH_MAX.saturating_add(1) {
                break;
            }
            at = next;
        }
        let below = replaced.map_or(0, |r| buf.height(r));
        self.count(visited.saturating_add(usize::try_from(below).unwrap_or(usize::MAX)));
        if depth.saturating_add(below) > RELATIVE_DEPTH_MAX {
            return Err(Placing::TooDeep);
        }
        Ok(())
    }

    /// Reports the boxes of every relative placement that follows placement
    /// `key`, and with `with_self` its own, for the terminal to mark: once
    /// before a change that moves them, and once after it.
    pub(super) fn mark_family(&mut self, b: usize, key: PlacementKey, with_self: bool) {
        let Some(buf) = self.buffers.get(b) else {
            return;
        };
        let family = buf.family(key);
        let visited = family.len();
        let boxes: Vec<Region> = family
            .into_iter()
            .filter(|&k| with_self || k != key)
            .filter_map(|k| buf.region(k))
            .collect();
        self.changed.extend(boxes);
        self.count(visited);
    }

    /// Where each relative placement of buffer `b` draws, from the screen
    /// rows of the placements on lines that `tops` gives, and, for one that
    /// follows a virtual placement, from the origin the runs handed with
    /// the decision give on `rows`, the screen's own lines (D13).
    pub(super) fn drawn(
        &mut self,
        b: usize,
        rows: Rows<'_>,
        tops: &BTreeMap<PlacementKey, i64>,
    ) -> BTreeMap<PlacementKey, Drawn> {
        let mut out = BTreeMap::new();
        let origins = self.decision_origins(b, rows);
        let Some(buf) = self.buffers.get(b) else {
            return out;
        };
        for &k in buf.children.values().flatten() {
            let drawn = match buf.locate(k) {
                Some(Located::At { root, row, col, .. }) => Drawn::At {
                    top: tops.get(&root).map(|t| t.saturating_add(row)),
                    col,
                },
                Some(Located::Floating { root, row, col, .. }) => match origins.get(&root) {
                    Some(&(top, left)) => Drawn::At {
                        top: Some(top.saturating_add(row)),
                        col: left.saturating_add(col),
                    },
                    None => Drawn::Nowhere,
                },
                None => Drawn::Nowhere,
            };
            out.insert(k, drawn);
        }
        let visited = out.len();
        self.count(visited);
        out
    }
}
