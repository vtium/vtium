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

mod add;
mod anim;
mod edit;
mod events;
mod frame;
mod geometry;
mod iterm2;
pub(crate) mod kitty;
mod margins;
mod place;
mod relative;
mod reserve;
mod room;
mod runs;
mod sixel;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use crate::bounds::CELL_MAX;
#[cfg(doc)]
use crate::bounds::{IMAGE_OVERHEAD, PLACEMENT_OVERHEAD};
use crate::image::{ImageKey, LineKey, Pixels, PlacementKey, Refusal};
use crate::rules::{Kept, Layer, rules};

use anim::Anim;
pub use anim::{Animation, FramePixels, Run};
pub(crate) use anim::{Compose, Control, NewFrame, layer_cost};
pub use edit::Edit;
pub use frame::{FramePlacement, LinePlaces, SingleSize};
use geometry::Geometry;
use kitty::Indices;
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
    /// The palette indices it arrived as, kitty's `f=8` or `f=16`, from
    /// which its pixels are made again whenever the terminal's colors
    /// change. Such a picture takes no frames and no composition.
    ///
    /// References:
    /// - ADR-1100 D15: the kept indices, and the refused frames.
    indices: Option<Indices>,
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

    /// Stacks a placement as a new arrival (D4): on top, but a kitty
    /// picture goes beneath the first placement from the bottom whose image
    /// has a higher kitty id. A kitty picture sent without an id has the
    /// id 0, as the kitty specification gives it, so it goes beneath every
    /// kitty picture with an id. Returns the placements passed.
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

    /// The kitty id the id rule compares a placement by: its image's id,
    /// 0 for a kitty image sent without one, or none for a protocol
    /// without ids.
    fn id_of(&self, p: &Placement) -> Option<u32> {
        let image = self.images.get(&p.image)?;
        rules(p.protocol.0).ids.then_some(image.id)
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

    /// Whether the alternate buffer is the buffer in use.
    pub fn in_alternate(&self) -> bool {
        self.alt
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

    /// The width and height of image `key` of the buffer in use.
    pub(crate) fn image_size(&self, key: ImageKey) -> Option<(u32, u32)> {
        let image = self.image(self.active(), key)?;
        Some((image.width, image.height))
    }

    /// Whether image `key`, in either buffer, arrived as palette indices.
    pub fn indexed(&self, key: ImageKey) -> bool {
        self.buffers
            .iter()
            .any(|b| b.images.get(&key).is_some_and(|i| i.indices.is_some()))
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
}

/// Whether every pixel is opaque: alpha 0xff.
fn all_opaque(pixels: &[u32]) -> bool {
    pixels.iter().all(|&px| px >> 24 == 0xff)
}
