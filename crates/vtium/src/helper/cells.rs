//! What the console knows of each cell size's glyphs: those the store holds,
//! those asked for, and those the font lacks.

use std::collections::BTreeSet;
use std::time::Instant;
use vtium_term::ClusterWidth;

use vtium_font::set::SetFace;
use vtium_font::spacing::SpacedCell;
use vtium_font::{Doubled, Variant};

use crate::deferred::Text;

use super::Helper;
use super::wire::Kind;

/// An answer the console takes: glyphs now in the store at `cell`, drawn
/// through `face`, for `chars`, sorted: each codepoint it brought with
/// those it decomposes to, and each character of each cluster, so the
/// cells showing one, or a cluster that composes to one, redraw.
#[derive(Debug)]
pub struct Answer {
    pub at: SpacedCell,
    pub face: SetFace,
    pub chars: Vec<char>,
}

/// One bit for each Unicode codepoint, [`BITS_BYTES`] once a bit is set
/// and nothing before, so a record that never holds a codepoint costs
/// nothing.
pub(super) struct Bits(Vec<u64>);

/// The bytes a [`Bits`] takes once a bit is set: 136 KiB.
const BITS_BYTES: usize = 0x11_0000 / 8;

impl Bits {
    pub(super) fn new() -> Bits {
        Bits(Vec::new())
    }

    pub(super) fn has(&self, c: u32) -> bool {
        self.0
            .get((c / 64) as usize)
            .is_some_and(|w| w & (1 << (c % 64)) != 0)
    }

    pub(super) fn put(&mut self, c: u32, on: bool) {
        if on && self.0.is_empty() {
            self.0 = vec![0; BITS_BYTES / 8];
        }
        if let Some(w) = self.0.get_mut((c / 64) as usize) {
            if on {
                *w |= 1 << (c % 64);
            } else {
                *w &= !(1 << (c % 64));
            }
        }
    }

    /// The codepoints set, as ranges.
    pub(super) fn ranges(&self) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for (i, &w) in self.0.iter().enumerate() {
            if w == 0 {
                continue;
            }
            let base = u32::try_from(i).unwrap_or(0).saturating_mul(64);
            for b in (0..64).filter(|b| w & (1 << b) != 0) {
                let c = base.saturating_add(b);
                match out.last_mut() {
                    Some((_, last)) if last.checked_add(1) == Some(c) => *last = c,
                    _ => out.push((c, c)),
                }
            }
        }
        out
    }
}

#[cfg(feature = "bidi")]
/// Whether `part` is a part of the cluster `key` the helper shapes when a
/// mark of `key` has no glyph: `key` with one or more of its marks or
/// default ignorables left out, everything else kept in order (ADR-1200
/// D22 step 3).
pub(super) fn part_of(part: &[char], key: &[char]) -> bool {
    if part.len() >= key.len() || part.is_empty() {
        return false;
    }
    let mut rest = part.iter().peekable();
    for &c in key {
        if rest.peek() == Some(&&c) {
            rest.next();
        } else if !crate::deferred::droppable(c) {
            return false;
        }
    }
    rest.next().is_none()
}

/// Each codepoint of `runs`, `None` past `u32`.
pub(super) fn codepoints(runs: &[(u32, u32)]) -> impl Iterator<Item = Option<u32>> + '_ {
    runs.iter()
        .flat_map(|&(first, n)| (0..n).map(move |i| first.checked_add(i)))
}

/// Where a cell's fill stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fill {
    /// Never started.
    None,
    /// Not started yet: the console holds the fill until it knows that the
    /// cell is the one the frit draws in ([`Helper::hold_fill`]).
    Held,
    Active,
    /// Ended at the store's limit or by a display gone.
    Stopped,
    Done,
}

/// What the console knows of one cell's clusters, as of its codepoints.
#[derive(Default)]
pub(super) struct Clusters {
    pub(super) held: BTreeSet<Text>,
    /// At most [`CLUSTERS_LACKED_MAX`].
    pub(super) absent: BTreeSet<Text>,
    pub(super) pending: BTreeSet<Text>,
}

/// What the console knows of one cell in one variant, a style and a
/// script: the codepoints and clusters the store holds, those the frit
/// lacks, those asked for and not yet answered, and, for the regular
/// normal variant, its fill.
/// Its four codepoint records take [`BITS_BYTES`] each once one is set,
/// and a variant other than regular normal never fills, so its `brought`
/// stays empty: at most 544 KiB for the regular normal record and 408 KiB
/// for each other variant's.
pub(super) struct CellState {
    /// The cell and the rows line spacing adds to it (ADR-1302 D13): one
    /// cell size can carry two bands on two heads, each with glyphs of
    /// its own.
    pub(super) at: SpacedCell,
    pub(super) variant: Variant,
    /// The double size its glyphs are drawn at, on a line of double size,
    /// or `None` for the normal size. The glyphs of each size are asked
    /// for and held apart.
    pub(super) doubled: Option<Doubled>,
    pub(super) held: Bits,
    pub(super) absent: Bits,
    pub(super) pending: Bits,
    pub(super) clusters: Clusters,
    pub(super) fill: Fill,
    /// The codepoints the fill asked for last has brought, held or not.
    pub(super) brought: Bits,
    /// The fill's first codepoints, sent again with a restart.
    pub(super) first: Vec<u32>,
    /// When the fill was asked for, and when its first glyphs came.
    pub(super) started: Option<Instant>,
    pub(super) first_glyphs: Option<Instant>,
    /// The console has decided its one early ask for bold printable ASCII
    /// at the cell ([`Helper::ask_bold_early`]): it sent the ask, or the
    /// store was at its limit. Kept in the regular normal record alone.
    pub(super) bold_early: bool,
    /// The early bold ask fell due while the last start of a search ran,
    /// and waits until the search ends ([`Helper::ask_bold_waiting`]).
    pub(super) bold_waits: bool,
}

impl CellState {
    /// The store holds `text`'s glyph at the cell.
    pub(super) fn hold(&mut self, text: Text) {
        match *text.as_slice() {
            [c] => self.held.put(u32::from(c), true),
            _ => {
                self.clusters.held.insert(text);
            }
        }
    }

    /// The store no longer holds `text`'s glyph at the cell.
    pub(super) fn forget(&mut self, text: Text) {
        match *text.as_slice() {
            [c] => self.held.put(u32::from(c), false),
            _ => {
                self.clusters.held.remove(&text);
            }
        }
    }

    pub(super) fn new(at: SpacedCell, (variant, doubled): Kind) -> CellState {
        CellState {
            at,
            variant,
            doubled,
            held: Bits::new(),
            absent: Bits::new(),
            pending: Bits::new(),
            clusters: Clusters::default(),
            fill: Fill::None,
            brought: Bits::new(),
            first: Vec::new(),
            started: None,
            first_glyphs: None,
            bold_early: false,
            bold_waits: false,
        }
    }
}

/// An ask the helper has not answered whole: its cell and variant, the
/// codepoints and clusters it has yet to send, and when it was sent.
pub(super) struct Ask {
    pub(super) at: SpacedCell,
    pub(super) variant: Variant,
    /// The double size the ask's glyphs are drawn at, or `None`.
    pub(super) doubled: Option<Doubled>,
    /// The cells the terminal gives a cluster with a spacing mark or a
    /// virama conjunct, which the helper places the clusters in (ADR-1006
    /// D8).
    pub(super) cluster_width: ClusterWidth,
    pub(super) want: BTreeSet<u32>,
    pub(super) clusters: BTreeSet<Text>,
    pub(super) sent: Instant,
}

impl Helper {
    /// Keeps the fill of `at`'s coverage, `first` first, without sending
    /// it, until [`Helper::start_held_fills`] starts it. The console holds
    /// the fill at a cell it draws in while it waits for the frit's own
    /// cell. A bitmap font may give no glyph at that cell, and the helper
    /// refuses a fill that brings no glyph, which would make the console
    /// give the font up although the font's own cell is about to come.
    /// The asks still go, so the frit draws what it can at the cell and the
    /// built-in font draws the rest.
    ///
    /// References:
    /// - ADR-1200 D17: no answer in time leaves the display's cell in use,
    ///   and an answer that comes after the wait is used from the next
    ///   frame.
    /// - ADR-1200 D18: the fill brings the frit's coverage at a cell while
    ///   nothing is asked.
    pub fn hold_fill(&mut self, at: SpacedCell, first: &[char]) {
        if self.failed.is_some() {
            return;
        }
        let Some(state) = self.state(at, Variant::default()) else {
            return;
        };
        if state.fill == Fill::None {
            state.fill = Fill::Held;
            state.first = first.iter().map(|&c| u32::from(c)).collect();
        }
    }

    /// Whether a fill waits for [`Helper::start_held_fills`].
    pub fn holds_fills(&self) -> bool {
        self.cells.iter().any(|c| c.fill == Fill::Held)
    }

    /// Starts each fill [`Helper::hold_fill`] kept, but at the cells of
    /// `waiting`, where the console still waits for the frit's own cell.
    /// A cell no display draws at any more was dropped with its fill
    /// ([`Helper::retain`]), so a fill held at the cell drawn while the
    /// console waited never starts once the frit's own cell replaced it.
    pub fn start_held_fills(&mut self, waiting: &[SpacedCell], now: Instant) {
        let held: Vec<(SpacedCell, Vec<char>)> = self
            .cells
            .iter()
            .filter(|c| c.fill == Fill::Held && !waiting.contains(&c.at))
            .map(|c| {
                (
                    c.at,
                    c.first.iter().filter_map(|&u| char::from_u32(u)).collect(),
                )
            })
            .collect();
        for (at, first) in held {
            self.fill(at.cell, at.band, &first, now);
        }
    }
}
