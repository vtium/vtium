//! The Flow page: where the highlight is, which row of the document is at
//! the viewer's top, and NEW PATH, the path being built; with the moves
//! the keys and the mouse make on it.
//!
//! [`Model`] is what the page is drawn from, made from the shared state
//! each time: the open document's flow, read as paths, and the parts.
//! [`State`] is the tab's own. [`State::settle`] puts the highlight back on
//! its box after the layout changes, as after an undo or a device that
//! came, and keeps that box at the same height in the viewer, so the page
//! does not jump. The tab's update and its draw both settle a copy of the
//! state on the layout of the moment, so a key and a click land on what
//! the viewer shows.
//!
//! References:
//! - ADR-1403 D7, "One document with three sections": the document
//!   scrolls just enough to keep the highlighted box in sight, PgUp and
//!   PgDn scroll by a window, and a relayout keeps the highlighted box at
//!   its height.
//! - ADR-1403 D7, "The keys": the arrows move from box to box.

pub mod colors;
pub mod draw;
pub mod layout;

use vtium_config_core::flow::Flow;
use vtium_config_core::ui::Shared;

use self::layout::{Boxed, Ctx, Layout, Obj};
use crate::edit::plan::{self, Revising};
use crate::parts::{self, Part};
use crate::paths::{self, Doc, Own, Path};

/// What the page is drawn from: the open document's flow and its
/// configuration as paths, and whether that is vtium's built-in default.
#[derive(Clone, Debug)]
pub struct Model {
    pub flow: Flow,
    /// The configuration as CONFIGURATION shows it: without the path NEW
    /// PATH is revising.
    pub doc: Doc,
    /// The whole configuration the draft holds, the path being revised
    /// included.
    pub full: Doc,
    pub default: bool,
    /// vtium-config can follow devices as they come and go, through udev.
    pub hotplug: bool,
    /// The match each connected device is picked by.
    pub own: Own,
    /// The parts the flow makes now, enumerated once for the model.
    pub fresh: Vec<Part>,
}

impl Model {
    /// The page's model from the shared state: the open document's
    /// vtium.conf and the devices present, with `revising`, the path NEW
    /// PATH is revising, left out of CONFIGURATION.
    pub fn of(shared: &Shared, revising: Option<&Revising>) -> Model {
        let flow =
            vtium_config_core::flowmodel::build(shared.document().text(), &shared.present, None);
        let full = paths::read(&flow);
        let doc = plan::shown(&full, revising);
        let own = Own::of(&flow);
        Model {
            fresh: parts::enumerate(&flow, &own, &doc),
            own,
            default: paths::is_default(&full),
            hotplug: shared.present.udev,
            flow,
            doc,
            full,
        }
    }

    /// The parts PARTS shows: `known`, the parts the tab holds, with what
    /// the flow holds now merged in.
    pub fn parts(&self, known: &[Part]) -> Vec<Part> {
        let mut parts = known.to_vec();
        parts::merge(&mut parts, &self.flow, &self.own, &self.fresh);
        parts
    }

    /// The page laid out `cols` columns wide, with NEW PATH holding `new`
    /// and PARTS showing `parts`.
    pub fn layout(&self, new: &Path, parts: &[Part], cols: usize) -> Layout {
        layout::layout(&Ctx {
            flow: &self.flow,
            doc: &self.doc,
            new,
            parts,
            own: &self.own,
            cols,
            default: self.default,
            hotplug: self.hotplug,
        })
    }
}

/// Where the highlight is on the page, which row of the document is at the
/// viewer's top, NEW PATH, and the parts PARTS keeps while the tab lives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// The highlighted box; `None` until the first layout.
    pub at: Option<Obj>,
    /// The document's row at the viewer's top.
    pub top: usize,
    /// How far below the viewer's top the highlighted box is, which a new
    /// layout keeps so the page does not jump.
    pub offset: isize,
    /// The document's row the highlighted box was last on, to find its
    /// neighbour when it goes.
    pub last_y: usize,
    /// The path NEW PATH holds.
    pub new: Path,
    /// The parts PARTS shows: a part whose device left stays, not
    /// connected, for as long as the tab lives.
    pub parts: Vec<Part>,
    /// The path NEW PATH holds for revising, which CONFIGURATION leaves
    /// out while the draft still has it.
    pub revising: Option<Revising>,
}

/// A row count as the layout's signed offsets hold it.
fn signed(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

impl State {
    /// Puts the highlight back on its box in `layout`, or on the box
    /// nearest where it was when the box is gone, and scrolls so the
    /// highlighted box is at the same height in a viewer `window` rows
    /// tall, or as near it as keeps the box in sight.
    pub fn settle(&mut self, layout: &Layout, window: usize) {
        let found = self.at.as_ref().and_then(|o| layout.find(o));
        let b = match found {
            Some(b) => Some(b),
            None => {
                let last = self.last_y;
                layout
                    .reading()
                    .into_iter()
                    .min_by_key(|b| (b.y.abs_diff(last), b.x))
            }
        };
        let Some(b) = b else {
            self.at = None;
            self.top = 0;
            return;
        };
        // on the first layout the viewer starts at the document's top, so
        // the headings above the first box show
        if self.at.is_none() {
            self.offset = signed(b.y);
        }
        self.at = Some(b.obj.clone());
        let y = b.y;
        let wanted = signed(y).saturating_sub(self.offset).max(0);
        let mut top = usize::try_from(wanted).unwrap_or(0);
        top = top.min(layout.height.saturating_sub(window));
        // the whole box in sight, its top first where the viewer is
        // shorter than a box
        if b.bottom() > top.saturating_add(window) {
            top = b.bottom().saturating_sub(window);
        }
        if y < top {
            top = y;
        }
        self.top = top;
        self.last_y = y;
        self.offset = signed(y).saturating_sub(signed(top));
    }

    /// Moves the highlight to `to`, scrolling only as far as keeps it in
    /// sight, line by line.
    pub fn go(&mut self, layout: &Layout, to: Obj, window: usize) {
        if let Some(b) = layout.find(&to) {
            self.offset = signed(b.y).saturating_sub(signed(self.top));
        }
        self.at = Some(to);
        self.settle(layout, window);
    }

    /// Scrolls by a window, down or up. When the highlighted box is
    /// scrolled away, the highlight moves to the box with the most rows in
    /// sight, the first in reading order going down and the last going up,
    /// and the viewer then shows that box whole.
    pub fn page(&mut self, layout: &Layout, down: bool, window: usize) {
        let most = layout.height.saturating_sub(window);
        self.top = if down {
            self.top.saturating_add(window).min(most)
        } else {
            self.top.saturating_sub(window)
        };
        let end = self.top.saturating_add(window);
        let rows_seen = |b: &Boxed| b.bottom().min(end).saturating_sub(b.y.max(self.top));
        let here = self.at.as_ref().and_then(|o| layout.find(o));
        if !here.is_some_and(|b| b.y >= self.top && b.bottom() <= end) {
            let mut order = layout.reading();
            if !down {
                order.reverse();
            }
            let best = order.iter().map(|b| rows_seen(b)).max().unwrap_or(0);
            if best > 0
                && let Some(b) = order.into_iter().find(|b| rows_seen(b) == best)
            {
                self.at = Some(b.obj.clone());
            }
        }
        if let Some(b) = self.at.as_ref().and_then(|o| layout.find(o)) {
            self.offset = signed(b.y).saturating_sub(signed(self.top));
        }
        self.settle(layout, window);
    }
}

/// A move of the highlight from box to box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    Left,
    Right,
    First,
    Last,
}

/// The box a move goes to from `at`: Right and Left to the box after or
/// before it in reading order, along a line and then on to the next, so
/// Right from the first box reaches every box; First and Last to the first
/// and the last box. Up and Down go to the line above or below, to its box
/// nearest in column. In CONFIGURATION, whose boxes stand in three
/// columns, they keep to the column first, so Down from a terminal reaches
/// the next terminal.
pub fn step(layout: &Layout, at: &Obj, to: Move) -> Option<Obj> {
    let order = layout.reading();
    let here = layout.find(at)?;
    let pick = |b: Option<&&Boxed>| b.map(|b| b.obj.clone());
    let overlaps = |b: &Boxed| b.x < here.right() && here.x < b.right();
    let centre = |b: &Boxed| b.x.saturating_add(b.w / 2);
    let at_order = order.iter().position(|b| b.obj == *at).unwrap_or(0);
    match to {
        Move::Right => pick(order.get(at_order.saturating_add(1))),
        Move::Left => pick(at_order.checked_sub(1).and_then(|i| order.get(i))),
        Move::Down | Move::Up => {
            let down = to == Move::Down;
            let candidates: Vec<&&Boxed> = order
                .iter()
                .filter(|b| if down { b.y > here.y } else { b.y < here.y })
                .collect();
            let config = |b: &Boxed| matches!(b.obj, Obj::Doc(..) | Obj::Roam(_) | Obj::Nothing(_));
            if config(here)
                && let Some(b) = candidates
                    .iter()
                    .filter(|b| config(b) && overlaps(b))
                    .min_by_key(|b| (b.y.abs_diff(here.y), centre(b).abs_diff(centre(here))))
            {
                return Some(b.obj.clone());
            }
            let line = candidates.iter().map(|b| b.y.abs_diff(here.y)).min();
            pick(
                candidates
                    .into_iter()
                    .filter(|b| Some(b.y.abs_diff(here.y)) == line)
                    .min_by_key(|b| centre(b).abs_diff(centre(here))),
            )
        }
        Move::First => pick(order.first()),
        Move::Last => pick(order.last()),
    }
}
