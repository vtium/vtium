//! The store: one terminal's images and placements, its budget and its
//! eviction order (ADR-1100, D2, D4, D5).
//!
//! A store has two buffers, main and alternate, each with its own images,
//! kitty ids and placements, and one budget and one use clock over both.
//! A placement is anchored to the line holding its top row, named by its
//! [`LineKey`], and a column; the store never orders lines by their key.
//! Two kinds of kitty placement have no line of their own: a relative
//! placement follows its parent, and a virtual placement lies on no line
//! (`store/relative.rs`).
//! The terminal tells the store which line holds each screen row when an
//! event needs it, through [`Rows`]: the screen's rows top to bottom and
//! the newest scrollback rows above them.
//!
//! Every map is ordered, and whatever reaches the screen, a reply or an
//! eviction is taken in key order, stack order or use order, never from a
//! hash. Every walk over placements adds what it visits to the store's
//! work count, which the terminal adds to its input call's work.

mod anim;
mod edit;
mod events;
mod frame;
mod iterm2;
pub(crate) mod kitty;
mod relative;
mod reserve;
mod runs;
mod sixel;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use crate::bounds::{
    BOX_CELLS_MAX, CELL_MAX, IMAGE_OVERHEAD, IMAGE_SIDE_MAX, IMAGES_PER_BUFFER, PLACEMENT_OVERHEAD,
    PLACEMENTS_PER_BUFFER,
};
use crate::image::{ImageKey, LineKey, NewImage, Pixels, Place, PlacementKey, Refusal};
use crate::rules::{Kept, Layer, rules};

use anim::Anim;
pub use anim::{Animation, FramePixels, Run};
pub(crate) use anim::{Compose, Control, NewFrame, layer_cost};
pub use edit::Edit;
pub use frame::FramePlacement;
use relative::{Drawn, Located, Spot};
pub(crate) use relative::{Link, Placing};
pub use reserve::Reservation;
pub use runs::{ChangedFloating, FloatingRegion};

/// The lines of one buffer's screen, as the terminal holds them when an
/// event reaches the store: `screen` the screen's rows top to bottom, and
/// `above` the scrollback's rows above it, newest first, at least
/// [`Store::reach`] of them when the scrollback has that many.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rows<'a> {
    pub screen: &'a [LineKey],
    pub above: &'a [LineKey],
}

impl Rows<'_> {
    /// The line at screen row `row`, negative rows counting up into the
    /// scrollback from -1.
    fn at(&self, row: i64) -> Option<LineKey> {
        if row >= 0 {
            self.screen.get(usize::try_from(row).ok()?).copied()
        } else {
            let k = row.checked_neg()?.checked_sub(1)?;
            self.above.get(usize::try_from(k).ok()?).copied()
        }
    }

    fn height(&self) -> i64 {
        i64::try_from(self.screen.len()).unwrap_or(i64::MAX)
    }
}

/// Both buffers' lines, for a request that may evict: an image placed on
/// a buffer's screen rows is on the screen's own lines whichever buffer is
/// shown.
#[derive(Clone, Copy, Debug, Default)]
pub struct Screens<'a> {
    pub main: Rows<'a>,
    pub alt: Rows<'a>,
}

impl<'a> Screens<'a> {
    fn of(&self, b: usize) -> Rows<'a> {
        if b == MAIN { self.main } else { self.alt }
    }
}

/// A box the store changed, in cells: `rows` rows from the row `row` rows
/// below the line `line`, `cols` columns from `col`. The terminal marks the
/// cells under it. `row` is 0 for a placement on its own line; a relative
/// placement (ADR-1100, D13) names the line its first ancestor with a line
/// is anchored on, and `row` is then the rows from that line to its box's
/// top, negative when the box starts above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub line: LineKey,
    pub row: i64,
    pub col: u32,
    pub cols: u32,
    pub rows: u32,
}

const MAIN: usize = 0;
const ALT: usize = 1;

/// An image the store holds.
#[derive(Clone, Debug, PartialEq)]
struct Image {
    protocol: Kept,
    id: u32,
    number: u32,
    width: u32,
    height: u32,
    pixels: Pixels,
    opaque: bool,
    /// Its charge: its pixels, [`IMAGE_OVERHEAD`], and
    /// [`PLACEMENT_OVERHEAD`] for each placement past its first.
    bytes: u64,
    /// The part of `bytes` the budget has yet to grant: pieces a screen
    /// event cut, until the event settles.
    owed: u64,
    /// The use clock's value at the last stream event that touched it.
    used_at: u64,
    /// The use clock's value when it was transmitted.
    sent_at: u64,
    transient: bool,
    /// It arrived as palette indices, kitty's `f=8` or `f=16`, which take no
    /// frames and no composition (ADR-1100, D15).
    indexed: bool,
    placements: u32,
    /// Held while an operation that may evict works for it.
    hold: u32,
    /// Its root frame's pixel revision: the store's revision when a command
    /// last changed its own pixels (ADR-1102, D3).
    revision: u64,
    /// Its animation, once a command gave it frames or set its playing
    /// (ADR-1102); `None` for a picture no animation command named.
    anim: Option<Box<Anim>>,
}

/// A placement: a box of cells showing part of an image, anchored to a
/// line, or following another placement, or on no line at all (`spot`).
/// The picture is never cut: `ox`, `oy`, `dw` and `dh` place the whole
/// drawn picture relative to the box's top left, in pixels of the cell
/// size `cw` by `ch` it was made in, and the source rectangle `sx`, `sy`,
/// `sw`, `sh` is what the whole picture shows.
#[derive(Clone, Debug, PartialEq)]
struct Placement {
    image: ImageKey,
    protocol: Kept,
    spot: Spot,
    /// On the line its spot names, or waiting for a scroll's lines to
    /// move; always false for a placement with no line of its own.
    attached: bool,
    /// The column of its box's left edge, for a placement on a line.
    col: u32,
    /// The columns and rows cut off its box's left and top since it was
    /// placed, so its origin, the cell that held its box's top left as it
    /// was placed, is `lost_cols` left of `col` and `lost_rows` above its
    /// line. A relative placement's offset counts from its parent's origin.
    lost_cols: u32,
    lost_rows: u32,
    cols: u32,
    rows: u32,
    ox: i64,
    oy: i64,
    dw: u64,
    dh: u64,
    sx: u32,
    sy: u32,
    sw: u32,
    sh: u32,
    cw: u32,
    ch: u32,
    z: i32,
    layer: Layer,
    seq: u64,
    id: u32,
    revision: u64,
}

impl Placement {
    /// The box of a placement on a line; `None` for one with no line of its
    /// own, whose box [`Buffer::region`] works out.
    fn own_region(&self) -> Option<Region> {
        match self.spot {
            Spot::Line(line) => Some(Region {
                line,
                row: 0,
                col: self.col,
                cols: self.cols,
                rows: self.rows,
            }),
            Spot::Child { .. } | Spot::Virtual => None,
        }
    }

    fn area(&self) -> u64 {
        u64::from(self.cols).saturating_mul(u64::from(self.rows))
    }
}

/// One buffer's images and placements.
#[derive(Clone, Debug, Default, PartialEq)]
struct Buffer {
    images: BTreeMap<ImageKey, Image>,
    placements: BTreeMap<PlacementKey, Placement>,
    /// The placements anchored on each line, for lookups only.
    by_line: BTreeMap<LineKey, Vec<PlacementKey>>,
    /// Each placement's relative placements, its children, in the order
    /// they were made (D13); every relative placement is in it once.
    children: BTreeMap<PlacementKey, Vec<PlacementKey>>,
    /// The relative placements whose parent is a virtual placement: every
    /// chain that ends at a virtual placement starts with one (D13).
    virtual_children: usize,
    /// The stack at each layer and z, bottom to top (D4).
    stacks: BTreeMap<(Layer, i32), Vec<PlacementKey>>,
    /// How many placements are each height in rows, all of them and those
    /// text cuts, so the tallest is known without a walk and a buffer with
    /// nothing text cuts answers a cut at once.
    heights: BTreeMap<u32, u32>,
    cut_heights: BTreeMap<u32, u32>,
}

impl Buffer {
    fn attach(&mut self, key: PlacementKey, line: LineKey) {
        self.detach(key);
        if let Some(p) = self.placements.get_mut(&key) {
            p.spot = Spot::Line(line);
            p.attached = true;
        }
        self.by_line.entry(line).or_default().push(key);
    }

    fn detach(&mut self, key: PlacementKey) {
        let Some(p) = self.placements.get_mut(&key) else {
            return;
        };
        if !core::mem::take(&mut p.attached) {
            return;
        }
        let Spot::Line(line) = p.spot else {
            return;
        };
        if let Some(list) = self.by_line.get_mut(&line) {
            list.retain(|&k| k != key);
            if list.is_empty() {
                self.by_line.remove(&line);
            }
        }
    }

    fn unstack(&mut self, key: PlacementKey) {
        let Some(p) = self.placements.get(&key) else {
            return;
        };
        let depth = (p.layer, p.z);
        if let Some(stack) = self.stacks.get_mut(&depth) {
            stack.retain(|&k| k != key);
            if stack.is_empty() {
                self.stacks.remove(&depth);
            }
        }
    }

    /// Stacks a placement as a new arrival (D4): on top, but a picture
    /// whose image has a kitty id goes beneath the first placement from the
    /// bottom whose image has a higher one. Returns the placements passed.
    fn stack_arrival(&mut self, key: PlacementKey) -> u64 {
        let Some(p) = self.placements.get(&key) else {
            return 0;
        };
        let depth = (p.layer, p.z);
        let id = self.id_of(p);
        let stack = self.stacks.entry(depth).or_default();
        let passed = u64::try_from(stack.len()).unwrap_or(u64::MAX);
        let at = id.and_then(|id| {
            stack.iter().position(|q| {
                self.placements
                    .get(q)
                    .and_then(|q| {
                        let image = self.images.get(&q.image)?;
                        Some(rules(q.protocol.0).ids && image.id > id)
                    })
                    .unwrap_or(false)
            })
        });
        match at {
            Some(i) => stack.insert(i, key),
            None => stack.push(key),
        }
        passed
    }

    /// The kitty id the id rule compares a placement by, if it has one.
    fn id_of(&self, p: &Placement) -> Option<u32> {
        let image = self.images.get(&p.image)?;
        (rules(p.protocol.0).ids && image.id != 0).then_some(image.id)
    }

    /// Stacks `key` directly above `after`, as a piece of it.
    fn stack_after(&mut self, key: PlacementKey, after: PlacementKey) {
        let Some(p) = self.placements.get(&key) else {
            return;
        };
        let stack = self.stacks.entry((p.layer, p.z)).or_default();
        match stack.iter().position(|&k| k == after) {
            Some(i) => stack.insert(i.saturating_add(1), key),
            None => stack.push(key),
        }
    }

    /// The tallest placement's rows, and the tallest one text cuts.
    fn tallest(&self) -> (u32, u32) {
        let top = |m: &BTreeMap<u32, u32>| m.keys().next_back().copied().unwrap_or(0);
        (top(&self.heights), top(&self.cut_heights))
    }

    /// Counts placement `key`'s height in, or with `inward` false out. Only
    /// a placement on a line is counted: one with no line of its own is
    /// found through its parent or not at all, never by walking lines.
    fn count_height(&mut self, key: PlacementKey, inward: bool) {
        let Some(p) = self.placements.get(&key) else {
            return;
        };
        if !matches!(p.spot, Spot::Line(_)) {
            return;
        }
        let (rows, cuts) = (p.rows, rules(p.protocol.0).cut_by_text);
        let step = |m: &mut BTreeMap<u32, u32>| {
            let n = m.entry(rows).or_insert(0);
            *n = if inward {
                n.saturating_add(1)
            } else {
                n.saturating_sub(1)
            };
            if *n == 0 {
                m.remove(&rows);
            }
        };
        step(&mut self.heights);
        if cuts {
            step(&mut self.cut_heights);
        }
    }

    /// Adds or replaces a placement, keeping the height counts, and the
    /// count of virtual placements' children when a replacement makes a
    /// parent virtual or no longer virtual.
    fn put(&mut self, key: PlacementKey, p: Placement) {
        self.count_height(key, false);
        let was = self.is_virtual(key);
        let now = p.spot == Spot::Virtual;
        self.placements.insert(key, p);
        self.count_height(key, true);
        let kids = self.children.get(&key).map_or(0, Vec::len);
        if was && !now {
            self.virtual_children = self.virtual_children.saturating_sub(kids);
        } else if now && !was {
            self.virtual_children = self.virtual_children.saturating_add(kids);
        }
    }

    fn is_virtual(&self, key: PlacementKey) -> bool {
        self.placements
            .get(&key)
            .is_some_and(|p| p.spot == Spot::Virtual)
    }

    /// Adds relative placement `key` to its parent `parent`'s children.
    fn link(&mut self, key: PlacementKey, parent: PlacementKey) {
        self.children.entry(parent).or_default().push(key);
        if self.is_virtual(parent) {
            self.virtual_children = self.virtual_children.saturating_add(1);
        }
    }

    /// Forgets placement `key`'s list of children, as it goes.
    fn drop_children(&mut self, key: PlacementKey) {
        let kids = self.children.remove(&key).map_or(0, |list| list.len());
        if self.is_virtual(key) {
            self.virtual_children = self.virtual_children.saturating_sub(kids);
        }
    }

    /// Removes a placement, keeping the height counts.
    fn take(&mut self, key: PlacementKey) -> Option<Placement> {
        self.count_height(key, false);
        self.placements.remove(&key)
    }

    fn find_id(&self, id: u32) -> Option<ImageKey> {
        self.images
            .iter()
            .find(|(_, image)| rules(image.protocol.0).ids && image.id == id)
            .map(|(&k, _)| k)
    }
}

/// One terminal's image store.
#[derive(Clone, Debug)]
pub struct Store {
    buffers: [Buffer; 2],
    /// The buffer in use: the alternate one while the alternate screen is.
    alt: bool,
    /// The budget, `image-storage-limit`, over both buffers.
    limit: u64,
    /// What the budget has granted.
    used: u64,
    /// What pieces cut by screen events owe, not yet granted.
    owed: u64,
    /// What the frame allowance has granted: every animation's charge and
    /// the frame reservations not yet spent (ADR-1102, D1).
    frame_used: u64,
    clock: u64,
    next_image: u64,
    next_placement: u64,
    next_seq: u64,
    revision: u64,
    /// The cell size in pixels new placements measure their pixels in.
    cell: (u32, u32),
    /// Placements a scroll moved, waiting for its lines to move: each
    /// buffer, key and the screen row to anchor at.
    pending: Vec<(usize, PlacementKey, i64)>,
    /// The image a replacement reserves room to free, never evicted
    /// meanwhile.
    keep: Option<(usize, ImageKey)>,
    /// The image being freed, whose placements give back no charge of
    /// their own.
    dying: Option<ImageKey>,
    work: u64,
    changed: Vec<Region>,
    /// The pixels of the image freed last, for a handler to fill again: one
    /// buffer at most, charged to the budget at 4 bytes a pixel, freed
    /// before any image when a reservation needs room, and on a reset or a
    /// buffer's clearing.
    spare: Option<Pixels>,
    /// An image is being evicted for room, so its pixels go.
    evicting: bool,
    /// The ledger of the reservations the store granted and has not seen
    /// spent: each one's number and the bytes it holds of the budget,
    /// which `used` counts.
    reservations: BTreeMap<u64, u64>,
    /// The number the next reservation gets.
    next_reservation: u64,
    /// The placeholder runs vtium-term read from each buffer's screen lines
    /// for the decision under way, held only through
    /// [`with_runs`](Self::with_runs) (ADR-1100, D13).
    runs: [Vec<crate::image::PlaceholderRun>; 2],
    /// The buffer and kitty image id of each virtual placement changed
    /// since vtium-term last took them, for it to mark that image's
    /// placeholder cells (D13).
    changed_virtual: BTreeSet<(bool, u32)>,
    /// The buffer and the box of each relative placement that follows a
    /// virtual placement and changed since vtium-term last took them, and
    /// the buffers, main then alternate, whose boxes passed the notes'
    /// bound and changed whole (D13).
    changed_floating: BTreeSet<(bool, FloatingRegion)>,
    floating_whole: [bool; 2],
}

/// Two stores compare by what they hold; the work an input call counted
/// is the call's, and the boxes not yet taken are the terminal's to mark;
/// a spare buffer counts by its size, which its charge follows.
impl PartialEq for Store {
    fn eq(&self, other: &Self) -> bool {
        self.buffers == other.buffers
            && self.alt == other.alt
            && self.limit == other.limit
            && self.used == other.used
            && self.owed == other.owed
            && self.frame_used == other.frame_used
            && self.clock == other.clock
            && self.next_image == other.next_image
            && self.next_placement == other.next_placement
            && self.next_seq == other.next_seq
            && self.revision == other.revision
            && self.cell == other.cell
            && self.pending == other.pending
            && self.keep == other.keep
            && self.dying == other.dying
            && self.spare_bytes() == other.spare_bytes()
            && self.reservations == other.reservations
            && self.next_reservation == other.next_reservation
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    /// An empty store whose budget is 0, so it stores no image until the
    /// host sets one, and whose cell is 8 by 16 pixels.
    pub fn new() -> Self {
        Self {
            buffers: [Buffer::default(), Buffer::default()],
            alt: false,
            limit: 0,
            used: 0,
            owed: 0,
            frame_used: 0,
            clock: 0,
            next_image: 0,
            next_placement: 0,
            next_seq: 0,
            revision: 0,
            cell: (8, 16),
            pending: Vec::new(),
            keep: None,
            dying: None,
            work: 0,
            changed: Vec::new(),
            spare: None,
            evicting: false,
            reservations: BTreeMap::new(),
            next_reservation: 0,
            runs: [Vec::new(), Vec::new()],
            changed_virtual: BTreeSet::new(),
            changed_floating: BTreeSet::new(),
            floating_whole: [false; 2],
        }
    }

    fn active(&self) -> usize {
        if self.alt { ALT } else { MAIN }
    }

    /// Whether either buffer holds an image or a placement.
    pub fn is_empty(&self) -> bool {
        self.buffers.iter().all(|b| b.images.is_empty())
    }

    /// Whether the buffer in use has a placement that text cuts: without
    /// one, a write or an erase has nothing to tell the store.
    pub fn has_cuttable(&self) -> bool {
        self.buffers
            .get(self.active())
            .is_some_and(|b| !b.cut_heights.is_empty())
    }

    /// Whether the buffer in use has a placement.
    pub fn has_placements(&self) -> bool {
        self.buffers
            .get(self.active())
            .is_some_and(|b| !b.placements.is_empty())
    }

    /// Whether the main buffer has a placement, which a rewrap carries.
    pub fn main_has_placements(&self) -> bool {
        self.buffers
            .get(MAIN)
            .is_some_and(|b| !b.placements.is_empty())
    }

    /// The scrollback rows above the screen a [`Rows`] must hold for the
    /// buffer in use: the tallest placement's rows less one, or more when a
    /// relative placement draws further below the line its first ancestor
    /// with a line is anchored on.
    pub fn reach(&self) -> usize {
        self.buffers.get(self.active()).map_or(0, Buffer::reach)
    }

    /// The scrollback rows above the main screen a [`Rows`] must hold for
    /// the main buffer, whichever buffer is in use.
    pub fn main_reach(&self) -> usize {
        self.buffers.get(MAIN).map_or(0, Buffer::reach)
    }

    /// Switches to the alternate buffer or back, emptying the alternate
    /// buffer both ways (D8).
    pub fn set_alternate(&mut self, on: bool) {
        if self.alt != on {
            self.clear(ALT);
            self.alt = on;
        }
    }

    /// RIS: both buffers are emptied. vtium-term resets each handler first,
    /// and a handler's reset gives back every reservation it holds, so a
    /// build with debug assertions checks that the ledger is empty: a
    /// reservation still in it is one a handler dropped without spending,
    /// whose bytes the budget would count for the store's whole life.
    pub fn reset(&mut self) {
        debug_assert!(
            self.reservations.is_empty(),
            "a handler kept or dropped a reservation past RIS: {:?}",
            self.reservations
        );
        self.clear(MAIN);
        self.clear(ALT);
    }

    /// Sets the budget, `image-storage-limit`, in bytes. A smaller limit
    /// evicts nothing at once: the next reservation evicts down to it.
    pub fn set_limit(&mut self, bytes: u64) {
        self.limit = bytes;
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// The bytes the store's images hold, the charges pieces owe included.
    pub fn bytes(&self) -> u64 {
        self.used.saturating_add(self.owed)
    }

    /// Sets the cell size placements made from now on measure their
    /// pixels in. A side of 0 or past [`CELL_MAX`] is refused.
    pub fn set_cell(&mut self, width: u32, height: u32) -> Result<(), Refusal> {
        let ok = |s: u32| s > 0 && u64::from(s) <= CELL_MAX;
        if !ok(width) || !ok(height) {
            return Err(Refusal::Invalid);
        }
        self.cell = (width, height);
        Ok(())
    }

    pub fn cell(&self) -> (u32, u32) {
        self.cell
    }

    /// Whether pieces a screen event cut owe a charge the budget has yet to
    /// grant.
    pub fn owes(&self) -> bool {
        self.owed > 0
    }

    /// Adds a handler's work to the count.
    pub fn add_work(&mut self, work: u64) {
        self.work = self.work.saturating_add(work);
    }

    /// What the store's walks visited since the last
    /// [`take_work`](Self::take_work).
    pub fn work(&self) -> u64 {
        self.work
    }

    /// The images both buffers hold.
    pub fn image_count(&self) -> usize {
        self.buffers.iter().map(|b| b.images.len()).sum()
    }

    /// The placements both buffers hold.
    pub fn placement_count(&self) -> usize {
        self.buffers.iter().map(|b| b.placements.len()).sum()
    }

    /// What the store's walks visited since the last call.
    pub fn take_work(&mut self) -> u64 {
        core::mem::take(&mut self.work)
    }

    /// The boxes placements left or entered since the last call.
    pub fn take_changed(&mut self) -> Vec<Region> {
        core::mem::take(&mut self.changed)
    }

    /// A count that moves on every change to a placement or an image, which
    /// a frame entry's `revision` is stamped from.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn tick(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }

    fn bump(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }

    fn count(&mut self, n: usize) {
        self.work = self
            .work
            .saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
    }

    /// Stores an image in the buffer in use, reserving its whole cost first
    /// (D5): what `reservation` already holds, which a handler reserved as
    /// its payload arrived, and the rest now. An image of a kitty id already
    /// in the buffer replaces it, with its placements, once the new one has
    /// its bytes; refused, the old one stays, and so does nothing of the
    /// reservation.
    pub fn add_image(
        &mut self,
        new: NewImage,
        reservation: Option<Reservation>,
        screens: Screens<'_>,
    ) -> Result<ImageKey, Refusal> {
        let b = self.active();
        // the image a reservation held for its replacement, when it is
        // still in the buffer in use with the id; it is held no more. A copy
        // of a reservation already spent holds nothing, so the image
        // reserves its whole cost
        // a frame reservation holds the frame allowance, which pays for no
        // image: it is given back, and the image reserves its whole cost
        let reservation = match reservation {
            Some(r) if r.frames => {
                self.unreserve(r);
                None
            }
            other => other,
        };
        let spent = reservation.and_then(|r| self.spend(&r).map(|bytes| (r, bytes)));
        let (mut reserved, held) = match spent {
            Some((r, bytes)) => {
                self.hold(r.buffer, r.old, false);
                let still = r
                    .old
                    .filter(|&o| r.buffer == b && self.image(b, o).is_some_and(|i| i.id == new.id));
                (bytes, still)
            }
            None => (0, None),
        };
        let refuse = |s: &mut Self, reserved: u64, why: Refusal| {
            s.used = s.used.saturating_sub(reserved);
            Err(why)
        };
        let side = |s: u32| s > 0 && u64::from(s) <= IMAGE_SIDE_MAX;
        let n = u64::from(new.width).saturating_mul(u64::from(new.height));
        let bytes = n.checked_mul(4).and_then(|b| b.checked_add(IMAGE_OVERHEAD));
        let fits =
            side(new.width) && side(new.height) && u64::try_from(new.pixels.len()).ok() == Some(n);
        let Some(bytes) = bytes.filter(|_| fits) else {
            return refuse(self, reserved, Refusal::Invalid);
        };
        let old = match held {
            Some(o) => Some(o),
            None if rules(new.protocol).ids && new.id != 0 => {
                let count = self.buffers.get(b).map_or(0, |buf| buf.images.len());
                self.count(count);
                self.buffers.get(b).and_then(|buf| buf.find_id(new.id))
            }
            None => None,
        };
        if reserved > bytes {
            self.used = self.used.saturating_sub(reserved.saturating_sub(bytes));
            reserved = bytes;
        }
        self.hold(b, old, true);
        self.settle(screens);
        let made = match old {
            Some(_) => Ok(()),
            None => self.make_image_room(b, screens),
        };
        let added = made.and_then(|()| {
            self.reserve_bytes(bytes.saturating_sub(reserved), old.map(|o| (b, o)), screens)
        });
        self.hold(b, old, false);
        let added = match added {
            Ok(added) => added,
            Err(why) => return refuse(self, reserved, why),
        };
        let credit = reserved.saturating_add(added);
        let key = ImageKey(self.next_image);
        self.next_image = self.next_image.saturating_add(1);
        let stamp = self.tick();
        let revision = self.bump();
        let opaque = match new.opaque {
            Some(opaque) => {
                debug_assert_eq!(
                    opaque,
                    all_opaque(&new.pixels),
                    "the handler's opacity disagrees with the pixels"
                );
                opaque
            }
            None => {
                self.count(new.pixels.len());
                all_opaque(&new.pixels)
            }
        };
        if let Some(o) = old {
            // the old image's bytes beyond what was reserved pass to the new
            // one; freeing it gives back only what is left
            if let Some(image) = self.image_mut(b, o) {
                image.bytes = image.bytes.saturating_sub(bytes.saturating_sub(credit));
            }
            self.free_image(b, o);
        }
        let image = Image {
            protocol: Kept(new.protocol),
            id: new.id,
            number: new.number,
            width: new.width,
            height: new.height,
            pixels: Pixels::new(new.pixels),
            opaque,
            bytes,
            owed: 0,
            used_at: stamp,
            sent_at: stamp,
            transient: new.transient,
            indexed: false,
            placements: 0,
            hold: 0,
            revision,
            anim: None,
        };
        if let Some(buf) = self.buffers.get_mut(b) {
            buf.images.insert(key, image);
        }
        Ok(key)
    }

    /// Places `image` of the buffer in use on its screen, as `place` says,
    /// anchored to the line at `place.row` of `screens`'s rows for that
    /// buffer, on a screen `cols` wide. A new placement past the image's
    /// first reserves its charge. A picture whose rules say a new picture
    /// covers older ones cell by cell first removes the older pictures of
    /// its kind under every cell it covers fully with opaque pixels, and
    /// then those of them under its box left with no pixel set
    /// (`store/sixel.rs`).
    pub fn place(
        &mut self,
        image: ImageKey,
        place: Place,
        screens: Screens<'_>,
        cols: usize,
    ) -> Result<PlacementKey, Refusal> {
        self.place_at(image, place, Link::Cell, screens, cols)
            .map_err(Placing::refusal)
    }

    /// Places `image` as [`place`](Self::place) does, at the screen cell
    /// `place` names, or as `link` says otherwise: as a relative placement
    /// of a parent, or as a virtual placement on no line (D13). A relative
    /// or virtual placement ignores `place.col` and `place.row`.
    pub(crate) fn place_at(
        &mut self,
        image: ImageKey,
        place: Place,
        link: Link,
        screens: Screens<'_>,
        cols: usize,
    ) -> Result<PlacementKey, Placing> {
        let b = self.active();
        let rows = screens.of(b);
        let (cw, ch) = self.cell;
        let img = self.image(b, image).ok_or(Refusal::NotFound)?;
        let (protocol, width, height) = (img.protocol.0, img.width, img.height);
        let line = match link {
            Link::Cell | Link::Drawn { .. } => {
                let cols_ok = usize::try_from(place.col).is_ok_and(|c| c < cols);
                let row = usize::try_from(place.row).ok();
                let line = row.and_then(|r| rows.screen.get(r)).copied();
                Some(line.filter(|_| cols_ok).ok_or(Refusal::Invalid)?)
            }
            Link::Parent { .. } | Link::Virtual => None,
        };
        if place.x >= cw
            || place.y >= ch
            || place.src_x >= width
            || place.src_y >= height
            || u64::from(place.cols) > BOX_CELLS_MAX
            || u64::from(place.rows) > BOX_CELLS_MAX
        {
            return Err(Refusal::Invalid.into());
        }
        let drawn = match link {
            Link::Drawn { width, height } => Some((width, height)),
            _ => None,
        };
        let geometry = Geometry::fit(&place, drawn, (width, height), (cw, ch))?;
        let replaced = if rules(protocol).ids && place.id != 0 {
            let count = self.buffers.get(b).map_or(0, |buf| buf.placements.len());
            self.count(count);
            self.buffers.get(b).and_then(|buf| {
                buf.placements
                    .iter()
                    .find(|(_, p)| p.image == image && p.id == place.id)
                    .map(|(&k, _)| k)
            })
        } else {
            None
        };
        let spot = match (link, line) {
            (Link::Cell | Link::Drawn { .. }, Some(line)) => Spot::Line(line),
            (
                Link::Parent {
                    image,
                    placement,
                    cols,
                    rows,
                },
                _,
            ) => {
                let parent = self.find_parent(b, image, placement)?;
                self.check_chain(b, parent, replaced)?;
                Spot::Child { parent, cols, rows }
            }
            _ => Spot::Virtual,
        };
        // a placement past the image's first carries its charge
        let charged = replaced.is_none() && self.image(b, image).is_some_and(|i| i.placements > 0);
        if replaced.is_none() {
            self.hold(b, Some(image), true);
            let room = self.make_placement_room(b, image, screens);
            self.hold(b, Some(image), false);
            room?;
        }
        // the image being placed is held through the cut, so a cut that
        // removes its own last placement leaves it to the new one
        self.hold(b, Some(image), true);
        if rules(protocol).covered_by_same {
            self.cover(b, image, &place, &geometry, rows, cols);
        }
        self.hold(b, Some(image), false);
        // the cut took the image's other placements: the new one is its
        // first again, and the image pays for it
        if charged && self.image(b, image).is_some_and(|i| i.placements == 0) {
            if let Some(img) = self.image_mut(b, image) {
                img.bytes = img.bytes.saturating_sub(PLACEMENT_OVERHEAD);
            }
            self.give_back(b, image, PLACEMENT_OVERHEAD);
        }
        let (z, layer) = (rules(protocol).depth)(place.z);
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let revision = self.bump();
        let placement = Placement {
            image,
            protocol: Kept(protocol),
            spot,
            attached: false,
            col: if line.is_some() { place.col } else { 0 },
            lost_cols: 0,
            lost_rows: 0,
            cols: geometry.cols,
            rows: geometry.rows,
            ox: geometry.ox,
            oy: geometry.oy,
            dw: geometry.dw,
            dh: geometry.dh,
            sx: place.src_x,
            sy: place.src_y,
            sw: geometry.sw,
            sh: geometry.sh,
            cw,
            ch,
            z,
            layer,
            seq,
            id: place.id,
            revision,
        };
        self.virtual_changed(b, &placement);
        let key = match replaced {
            Some(k) => {
                // the old box and every box that follows it change
                self.mark_family(b, k, true);
                if let Some(old) = self.placement(b, k).cloned() {
                    self.virtual_changed(b, &old);
                }
                if let Some(buf) = self.buffers.get_mut(b) {
                    buf.unstack(k);
                    buf.detach(k);
                    buf.unlink(k);
                    buf.put(k, placement);
                }
                k
            }
            None => {
                let k = PlacementKey(self.next_placement);
                self.next_placement = self.next_placement.saturating_add(1);
                if let Some(buf) = self.buffers.get_mut(b) {
                    buf.put(k, placement);
                }
                if let Some(img) = self.image_mut(b, image) {
                    img.placements = img.placements.saturating_add(1);
                }
                k
            }
        };
        let passed = match self.buffers.get_mut(b) {
            Some(buf) => {
                match spot {
                    Spot::Line(line) => buf.attach(key, line),
                    Spot::Child { parent, .. } => buf.link(key, parent),
                    Spot::Virtual => {}
                }
                buf.stack_arrival(key)
            }
            None => 0,
        };
        self.work = self.work.saturating_add(passed);
        let stamp = self.tick();
        if let Some(img) = self.image_mut(b, image) {
            img.used_at = stamp;
        }
        self.mark_family(b, key, true);
        Ok(key)
    }

    /// Marks image `key` of the buffer in use as one that arrived as
    /// palette indices (ADR-1100, D15).
    pub(crate) fn set_indexed(&mut self, key: ImageKey) {
        let b = self.active();
        if let Some(image) = self.image_mut(b, key) {
            image.indexed = true;
        }
    }

    /// Whether image `key`, in either buffer, arrived as palette indices.
    pub fn indexed(&self, key: ImageKey) -> bool {
        self.buffers
            .iter()
            .any(|b| b.images.get(&key).is_some_and(|i| i.indexed))
    }

    fn image(&self, b: usize, key: ImageKey) -> Option<&Image> {
        self.buffers.get(b)?.images.get(&key)
    }

    fn image_mut(&mut self, b: usize, key: ImageKey) -> Option<&mut Image> {
        self.buffers.get_mut(b)?.images.get_mut(&key)
    }

    fn placement(&self, b: usize, key: PlacementKey) -> Option<&Placement> {
        self.buffers.get(b)?.placements.get(&key)
    }

    fn hold(&mut self, b: usize, image: Option<ImageKey>, on: bool) {
        if let Some(img) = image.and_then(|k| self.image_mut(b, k)) {
            img.hold = if on {
                img.hold.saturating_add(1)
            } else {
                img.hold.saturating_sub(1)
            };
        }
    }

    // The budget.

    /// Reserves `bytes`, counting what `keep`, an image a replacement will
    /// free, already holds, and returns what it added. A request that
    /// cannot fit once everything that may go has gone evicts nothing and
    /// is refused; otherwise images go in the eviction order until it fits.
    fn reserve_bytes(
        &mut self,
        bytes: u64,
        keep: Option<(usize, ImageKey)>,
        screens: Screens<'_>,
    ) -> Result<u64, Refusal> {
        let credit = keep
            .and_then(|(b, k)| self.image(b, k))
            .map_or(0, |i| i.bytes.saturating_sub(i.owed));
        let need = bytes.saturating_sub(credit);
        self.keep = keep;
        let total = |s: &Self| s.used.saturating_sub(credit).saturating_add(bytes);
        let mut ok = true;
        // the spare buffer goes before any image
        if total(self) > self.limit {
            self.drop_spare();
        }
        if total(self) > self.limit {
            let room = self
                .used
                .saturating_sub(credit)
                .saturating_sub(self.evictable(screens));
            ok = bytes <= self.limit && room.saturating_add(bytes) <= self.limit;
            while ok && total(self) > self.limit {
                ok = self.evict_one(None, screens);
            }
        }
        self.keep = None;
        if !ok {
            return Err(Refusal::NoSpace);
        }
        self.used = self.used.saturating_add(need);
        Ok(need)
    }

    /// Gives back `bytes` an image no longer holds: what its pieces owed
    /// first, and the rest, which the budget granted, to the budget.
    fn give_back(&mut self, b: usize, key: ImageKey, bytes: u64) {
        let owed = self.image(b, key).map_or(0, |i| i.owed.min(bytes));
        if let Some(img) = self.image_mut(b, key) {
            img.owed = img.owed.saturating_sub(owed);
        }
        self.owed = self.owed.saturating_sub(owed);
        self.used = self.used.saturating_sub(bytes.saturating_sub(owed));
    }

    /// Grants what the pieces a screen event cut owe. When the budget
    /// refuses, the smallest placements of the images that owe go, one,
    /// then two, then four a round, as if their cells were erased (D2).
    fn settle(&mut self, screens: Screens<'_>) {
        let mut drop = 1usize;
        while self.owed > 0 {
            let owed = self.owed;
            if self.reserve_bytes(owed, None, screens).is_ok() {
                // images evicted took their pieces' debts with them
                self.used = self.used.saturating_sub(owed.saturating_sub(self.owed));
                for buf in &mut self.buffers {
                    for image in buf.images.values_mut() {
                        image.owed = 0;
                    }
                }
                self.owed = 0;
                return;
            }
            let mut small: Vec<(u64, usize, PlacementKey)> = Vec::new();
            for (b, buf) in self.buffers.iter().enumerate() {
                for (&k, p) in &buf.placements {
                    let owes = buf
                        .images
                        .get(&p.image)
                        .is_some_and(|i| i.owed > 0 && i.hold == 0);
                    if owes {
                        small.push((p.area(), b, k));
                    }
                }
            }
            let visited = self.placement_count().saturating_add(small.len());
            self.count(visited);
            if small.is_empty() {
                return;
            }
            small.sort_unstable();
            for &(_, b, k) in small.iter().take(drop) {
                if self.owed == 0 {
                    break;
                }
                let owes = self
                    .placement(b, k)
                    .and_then(|p| self.image(b, p.image))
                    .is_some_and(|i| i.owed > 0);
                if owes {
                    self.free_placement(b, k, true);
                }
            }
            drop = drop.saturating_mul(2);
        }
    }

    // Eviction.

    /// Each image's place in the eviction order: whether it is pinned, and
    /// whether it is placed on the screen's own lines. A relative placement
    /// counts by the rows it draws on, and one that follows a virtual
    /// placement, like a virtual placement itself, counts as on the
    /// screen's own lines (D13).
    fn marks(&mut self, screens: Screens<'_>) -> [BTreeMap<ImageKey, (bool, bool)>; 2] {
        let mut out = [BTreeMap::new(), BTreeMap::new()];
        let keep = self.keep;
        for (b, marks) in out.iter_mut().enumerate() {
            let rows = screens.of(b);
            let height = rows.height();
            let tops = self.tops(b, rows);
            let drawn = self.drawn(b, rows, &tops);
            let Some(buf) = self.buffers.get(b) else {
                continue;
            };
            let visited = buf
                .images
                .len()
                .saturating_mul(2)
                .saturating_add(buf.placements.len());
            for (&k, image) in &buf.images {
                let kept = keep == Some((b, k));
                marks.insert(k, (image.hold > 0 || kept, false));
            }
            for (k, p) in &buf.placements {
                let reaches = |top: i64| top.saturating_add(i64::from(p.rows)) > 0 && top < height;
                let on = match p.spot {
                    Spot::Line(_) => tops.get(k).is_some_and(|&top| reaches(top)),
                    Spot::Child { .. } => match drawn.get(k) {
                        Some(Drawn::At { top, .. }) => top.is_some_and(reaches),
                        Some(Drawn::Nowhere) | None => true,
                    },
                    Spot::Virtual => true,
                };
                if !on {
                    continue;
                }
                if let Some(m) = marks.get_mut(&p.image) {
                    if rules(p.protocol.0).evict_on_screen {
                        m.1 = true;
                    } else {
                        m.0 = true;
                    }
                }
            }
            self.count(visited);
        }
        out
    }

    /// The bytes of every image that may be evicted.
    fn evictable(&mut self, screens: Screens<'_>) -> u64 {
        let marks = self.marks(screens);
        let mut bytes = 0u64;
        for (buf, marks) in self.buffers.iter().zip(&marks) {
            for (k, image) in &buf.images {
                if !marks.get(k).is_some_and(|m| m.0) {
                    bytes = bytes.saturating_add(image.bytes);
                }
            }
        }
        bytes
    }

    /// The image evicted next, from buffer `only` or from both (D5): images
    /// with no placement, transient ones first, then those placed only in
    /// the scrollback, each least recently used first; then those placed
    /// on the screen's own lines whose rules allow it, oldest transmission
    /// first.
    fn victim(&mut self, only: Option<usize>, screens: Screens<'_>) -> Option<(usize, ImageKey)> {
        let marks = self.marks(screens);
        let mut best: Option<((u8, u64), usize, ImageKey)> = None;
        for (b, (buf, marks)) in self.buffers.iter().zip(&marks).enumerate() {
            if only.is_some_and(|o| o != b) {
                continue;
            }
            for (&k, image) in &buf.images {
                let (pinned, on_screen) = marks.get(&k).copied().unwrap_or_default();
                if pinned {
                    continue;
                }
                let order = if on_screen {
                    (3, image.sent_at)
                } else if image.placements > 0 {
                    (2, image.used_at)
                } else if image.transient {
                    (0, image.used_at)
                } else {
                    (1, image.used_at)
                };
                if best.is_none_or(|(o, _, _)| order < o) {
                    best = Some((order, b, k));
                }
            }
        }
        best.map(|(_, b, k)| (b, k))
    }

    fn evict_one(&mut self, only: Option<usize>, screens: Screens<'_>) -> bool {
        match self.victim(only, screens) {
            Some((b, k)) => {
                // an image evicted for room keeps no spare buffer
                self.evicting = true;
                self.free_image(b, k);
                self.evicting = false;
                true
            }
            None => false,
        }
    }

    /// Room for one more image in buffer `b`, past [`IMAGES_PER_BUFFER`]
    /// by evicting from it alone.
    fn make_image_room(&mut self, b: usize, screens: Screens<'_>) -> Result<(), Refusal> {
        while self.buffers.get(b).is_some_and(|buf| {
            u64::try_from(buf.images.len()).unwrap_or(u64::MAX) >= IMAGES_PER_BUFFER
        }) {
            if !self.evict_one(Some(b), screens) {
                return Err(Refusal::NoSpace);
            }
        }
        Ok(())
    }

    /// Room for one more placement of `image` in buffer `b`, and its charge
    /// when the image already has one.
    fn make_placement_room(
        &mut self,
        b: usize,
        image: ImageKey,
        screens: Screens<'_>,
    ) -> Result<(), Refusal> {
        while self.buffers.get(b).is_some_and(|buf| {
            u64::try_from(buf.placements.len()).unwrap_or(u64::MAX) >= PLACEMENTS_PER_BUFFER
        }) {
            if !self.evict_one(Some(b), screens) {
                return Err(Refusal::NoSpace);
            }
        }
        if self.image(b, image).is_some_and(|i| i.placements > 0) {
            self.settle(screens);
            self.reserve_bytes(PLACEMENT_OVERHEAD, None, screens)?;
            if let Some(img) = self.image_mut(b, image) {
                img.bytes = img.bytes.saturating_add(PLACEMENT_OVERHEAD);
            }
        }
        Ok(())
    }

    // Freeing.

    /// Removes a placement, and every relative placement that follows it,
    /// their children in turn (D13). One past its image's first gives its
    /// charge back; with `free_image`, an image whose rules free it with its
    /// last placement goes with it. A relative placement that goes with its
    /// parent takes its image when that image is left with no placement, as
    /// the kitty spec asks.
    fn free_placement(&mut self, b: usize, key: PlacementKey, free_image: bool) {
        let family = match self.buffers.get(b) {
            Some(buf) if buf.placements.contains_key(&key) => buf.family(key),
            _ => return,
        };
        let visited = family.len();
        self.count(visited);
        // each box is worked out while every parent is still there
        self.note_floating(b, &family);
        let boxes: Vec<(PlacementKey, Option<Region>)> = family
            .into_iter()
            .map(|k| (k, self.buffers.get(b).and_then(|buf| buf.region(k))))
            .collect();
        for (i, (k, region)) in boxes.into_iter().enumerate() {
            let Some(image) = self.remove_one(b, k, region) else {
                continue;
            };
            let Some(img) = self.image(b, image) else {
                continue;
            };
            let bare = img.placements == 0 && img.hold == 0;
            let freed = if i == 0 {
                free_image && rules(img.protocol.0).freed_with_last_placement
            } else {
                self.dying != Some(image)
            };
            if bare && freed {
                self.free_image(b, image);
            }
        }
    }

    /// Removes placement `key` alone, whose box was `region`, and returns
    /// its image's key. One past its image's first gives its charge back.
    fn remove_one(
        &mut self,
        b: usize,
        key: PlacementKey,
        region: Option<Region>,
    ) -> Option<ImageKey> {
        let buf = self.buffers.get_mut(b)?;
        buf.detach(key);
        buf.unstack(key);
        buf.unlink(key);
        buf.drop_children(key);
        let p = buf.take(key)?;
        self.virtual_changed(b, &p);
        self.pending.retain(|&(pb, pk, _)| (pb, pk) != (b, key));
        self.changed.extend(region);
        self.bump();
        let dying = self.dying == Some(p.image);
        let image = self.image_mut(b, p.image)?;
        image.placements = image.placements.saturating_sub(1);
        if image.placements > 0 && !dying {
            image.bytes = image.bytes.saturating_sub(PLACEMENT_OVERHEAD);
            self.give_back(b, p.image, PLACEMENT_OVERHEAD);
        }
        Some(p.image)
    }

    /// Reports every placement of image `key` of buffer `b` as changed, as
    /// when the pixels they show change: the box of each placement on a
    /// line or following one, the placeholder cells of the image's virtual
    /// placements, and where the runs place each placement that follows a
    /// virtual one.
    fn mark_image(&mut self, b: usize, key: ImageKey) {
        let Some(buf) = self.buffers.get(b) else {
            return;
        };
        let visited = buf.placements.len();
        let mut keys = Vec::new();
        let mut boxes = Vec::new();
        let mut placeholders = false;
        for (&k, p) in &buf.placements {
            if p.image != key {
                continue;
            }
            placeholders |= p.spot == Spot::Virtual;
            boxes.extend(buf.region(k));
            keys.push(k);
        }
        let id = buf.images.get(&key).map_or(0, |i| i.id);
        self.changed.extend(boxes);
        if placeholders {
            self.note_virtual(b, id);
        }
        self.note_floating(b, &keys);
        self.count(visited);
    }

    /// Removes an image and every placement of it, and gives its bytes
    /// back in one release.
    fn free_image(&mut self, b: usize, key: ImageKey) {
        let Some(buf) = self.buffers.get(b) else {
            return;
        };
        let keys: Vec<PlacementKey> = buf
            .placements
            .iter()
            .filter(|(_, p)| p.image == key)
            .map(|(&k, _)| k)
            .collect();
        let visited = buf.placements.len();
        self.count(visited);
        let before = self.dying.replace(key);
        for k in keys {
            self.free_placement(b, k, false);
        }
        self.dying = before;
        let Some(image) = self
            .buffers
            .get_mut(b)
            .and_then(|buf| buf.images.remove(&key))
        else {
            return;
        };
        self.bump();
        self.owed = self.owed.saturating_sub(image.owed);
        self.used = self
            .used
            .saturating_sub(image.bytes.saturating_sub(image.owed));
        // its frames go with it and give back their allowance (ADR-1102, D1)
        self.frame_used = self.frame_used.saturating_sub(Anim::charge_of(&image));
        self.keep_spare(image.pixels);
    }

    /// The bytes the spare buffer holds of the budget: its pixels at 4
    /// bytes each.
    fn spare_bytes(&self) -> u64 {
        self.spare.as_ref().map_or(0, |p| {
            u64::try_from(p.as_slice().len())
                .unwrap_or(u64::MAX)
                .saturating_mul(4)
        })
    }

    /// Keeps `pixels` as the spare buffer, charged to the budget, in place
    /// of the one before, when the budget has room for it.
    fn keep_spare(&mut self, pixels: Pixels) {
        self.drop_spare();
        if self.evicting {
            return;
        }
        self.spare = Some(pixels);
        let bytes = self.spare_bytes();
        if self.used.saturating_add(bytes) > self.limit {
            self.spare = None;
        } else {
            self.used = self.used.saturating_add(bytes);
        }
    }

    /// Frees the spare buffer and gives its bytes back.
    fn drop_spare(&mut self) {
        self.used = self.used.saturating_sub(self.spare_bytes());
        self.spare = None;
    }

    /// Empties buffer `b`, giving back every byte its images hold.
    fn clear(&mut self, b: usize) {
        self.drop_spare();
        let keys: Vec<PlacementKey> = self
            .buffers
            .get(b)
            .map(|buf| buf.placements.keys().copied().collect())
            .unwrap_or_default();
        self.note_floating(b, &keys);
        let Some(buf) = self.buffers.get_mut(b) else {
            return;
        };
        let buf = core::mem::take(buf);
        for (&k, p) in &buf.placements {
            self.changed.extend(buf.region(k));
            if p.spot == Spot::Virtual
                && let Some(image) = buf.images.get(&p.image)
            {
                self.note_virtual(b, image.id);
            }
        }
        for image in buf.images.values() {
            self.owed = self.owed.saturating_sub(image.owed);
            self.used = self
                .used
                .saturating_sub(image.bytes.saturating_sub(image.owed));
            self.frame_used = self.frame_used.saturating_sub(Anim::charge_of(image));
        }
        self.pending.retain(|&(pb, _, _)| pb != b);
        self.bump();
    }

    /// Each placement of buffer `b` that `rows` reaches, and its top's
    /// screen row: the screen's rows from 0, the scrollback's above it from
    /// -1. A placement anchored deeper in the scrollback is not listed.
    fn tops(&mut self, b: usize, rows: Rows<'_>) -> BTreeMap<PlacementKey, i64> {
        let mut out = BTreeMap::new();
        let Some(buf) = self.buffers.get(b) else {
            return out;
        };
        if buf.placements.is_empty() {
            return out;
        }
        let mut visited = 0usize;
        let screen = rows.screen.iter().zip(0i64..);
        let above = rows.above.iter().zip((1i64..).map(i64::wrapping_neg));
        for (line, row) in screen.chain(above) {
            visited = visited.saturating_add(1);
            for &k in buf.by_line.get(line).into_iter().flatten() {
                out.insert(k, row);
            }
        }
        self.count(visited.saturating_add(out.len()));
        out
    }
}

/// A placement's box and picture, as [`Store::place`] fits them.
struct Geometry {
    cols: u32,
    rows: u32,
    ox: i64,
    oy: i64,
    dw: u64,
    dh: u64,
    sw: u32,
    sh: u32,
}

impl Geometry {
    /// The source cut to the image, and the box and the picture in it: at
    /// the `drawn` size when one is given, at the source's pixel size,
    /// fitted and centred in a box of both sides, or one side given and the
    /// other following the source's aspect. A
    /// picture scaled to a sliver keeps a pixel; one scaled past the
    /// largest box is refused.
    fn fit(
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

/// Whether every pixel is opaque: alpha 0xff.
fn all_opaque(pixels: &[u32]) -> bool {
    pixels.iter().all(|&px| px >> 24 == 0xff)
}
