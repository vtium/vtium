//! The store: one terminal's images and placements, its budget and its
//! eviction order (ADR-1100, D2, D4, D5).
//!
//! A store has two buffers, main and alternate, each with its own images,
//! kitty ids and placements, and one budget and one use clock over both.
//! A placement is anchored to the line holding its top row, named by its
//! [`LineKey`], and a column; the store never orders lines by their key.
//! The terminal tells the store which line holds each screen row when an
//! event needs it, through [`Rows`]: the screen's rows top to bottom and
//! the newest scrollback rows above them.
//!
//! Every map is ordered, and whatever reaches the screen, a reply or an
//! eviction is taken in key order, stack order or use order, never from a
//! hash. Every walk over placements adds what it visits to the store's
//! work count, which the terminal adds to its input call's work.

mod edit;
mod events;
mod frame;
pub(crate) mod kitty;
mod reserve;
mod sixel;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::bounds::{
    BOX_CELLS_MAX, CELL_MAX, IMAGE_OVERHEAD, IMAGE_SIDE_MAX, IMAGES_PER_BUFFER, PLACEMENT_OVERHEAD,
    PLACEMENTS_PER_BUFFER,
};
use crate::image::{ImageKey, LineKey, NewImage, Pixels, Place, PlacementKey, Refusal};
use crate::rules::{Kept, Layer, rules};

pub use edit::Edit;
pub use frame::FramePlacement;
pub use reserve::Reservation;

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

/// A box the store changed, in cells: `rows` rows from the line `line`,
/// `cols` columns from `col`. The terminal marks the cells under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub line: LineKey,
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
    placements: u32,
    /// Held while an operation that may evict works for it.
    hold: u32,
    revision: u64,
}

/// A placement: a box of cells anchored to a line, showing part of an
/// image. The picture is never cut: `ox`, `oy`, `dw` and `dh` place the
/// whole drawn picture relative to the box's top left, in pixels of the
/// cell size `cw` by `ch` it was made in, and the source rectangle `sx`,
/// `sy`, `sw`, `sh` is what the whole picture shows.
#[derive(Clone, Debug, PartialEq)]
struct Placement {
    image: ImageKey,
    protocol: Kept,
    anchor: LineKey,
    /// Anchored at `anchor`, or waiting for a scroll's lines to move.
    attached: bool,
    col: u32,
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
    fn region(&self) -> Region {
        Region {
            line: self.anchor,
            col: self.col,
            cols: self.cols,
            rows: self.rows,
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
            p.anchor = line;
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
        let line = p.anchor;
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

    /// Counts placement `key`'s height in, or with `inward` false out.
    fn count_height(&mut self, key: PlacementKey, inward: bool) {
        let Some(p) = self.placements.get(&key) else {
            return;
        };
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

    /// Adds or replaces a placement, keeping the height counts.
    fn put(&mut self, key: PlacementKey, p: Placement) {
        self.count_height(key, false);
        self.placements.insert(key, p);
        self.count_height(key, true);
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
    /// buffer in use: the tallest placement's rows less one.
    pub fn reach(&self) -> usize {
        let (tallest, _) = self
            .buffers
            .get(self.active())
            .map(Buffer::tallest)
            .unwrap_or_default();
        usize::try_from(tallest.saturating_sub(1)).unwrap_or(usize::MAX)
    }

    /// The scrollback rows above the main screen a [`Rows`] must hold for
    /// the main buffer, whichever buffer is in use.
    pub fn main_reach(&self) -> usize {
        let (tallest, _) = self
            .buffers
            .get(MAIN)
            .map(Buffer::tallest)
            .unwrap_or_default();
        usize::try_from(tallest.saturating_sub(1)).unwrap_or(usize::MAX)
    }

    /// Switches to the alternate buffer or back, emptying the alternate
    /// buffer both ways (D8).
    pub fn set_alternate(&mut self, on: bool) {
        if self.alt != on {
            self.clear(ALT);
            self.alt = on;
        }
    }

    /// RIS: both buffers are emptied.
    pub fn reset(&mut self) {
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
            placements: 0,
            hold: 0,
            revision,
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
        let b = self.active();
        let rows = screens.of(b);
        let (cw, ch) = self.cell;
        let img = self.image(b, image).ok_or(Refusal::NotFound)?;
        let (protocol, width, height) = (img.protocol.0, img.width, img.height);
        let cols_ok = usize::try_from(place.col).is_ok_and(|c| c < cols);
        let row = usize::try_from(place.row).ok();
        let line = row.and_then(|r| rows.screen.get(r)).copied();
        let Some(line) = line.filter(|_| cols_ok) else {
            return Err(Refusal::Invalid);
        };
        if place.x >= cw
            || place.y >= ch
            || place.src_x >= width
            || place.src_y >= height
            || u64::from(place.cols) > BOX_CELLS_MAX
            || u64::from(place.rows) > BOX_CELLS_MAX
        {
            return Err(Refusal::Invalid);
        }
        let geometry = Geometry::fit(&place, (width, height), (cw, ch))?;
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
            anchor: line,
            attached: false,
            col: place.col,
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
        let key = match replaced {
            Some(k) => {
                if let Some(old) = self.placement(b, k) {
                    self.changed.push(old.region());
                }
                if let Some(buf) = self.buffers.get_mut(b) {
                    buf.unstack(k);
                    buf.detach(k);
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
                buf.attach(key, line);
                buf.stack_arrival(key)
            }
            None => 0,
        };
        self.work = self.work.saturating_add(passed);
        let stamp = self.tick();
        if let Some(img) = self.image_mut(b, image) {
            img.used_at = stamp;
        }
        if let Some(p) = self.placement(b, key) {
            self.changed.push(p.region());
        }
        Ok(key)
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
    /// whether it is placed on the screen's own lines.
    fn marks(&mut self, screens: Screens<'_>) -> [BTreeMap<ImageKey, (bool, bool)>; 2] {
        let mut out = [BTreeMap::new(), BTreeMap::new()];
        let keep = self.keep;
        for (b, marks) in out.iter_mut().enumerate() {
            let tops = self.tops(b, screens.of(b));
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
                let reaches = tops
                    .get(k)
                    .is_some_and(|&top| top.saturating_add(i64::from(p.rows)) > 0);
                if !reaches {
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

    /// Removes a placement. One past its image's first gives its charge
    /// back; with `free_image`, an image whose rules free it with its last
    /// placement goes with it.
    fn free_placement(&mut self, b: usize, key: PlacementKey, free_image: bool) {
        let Some(buf) = self.buffers.get_mut(b) else {
            return;
        };
        buf.detach(key);
        buf.unstack(key);
        let Some(p) = buf.take(key) else {
            return;
        };
        self.pending.retain(|&(pb, pk, _)| (pb, pk) != (b, key));
        self.changed.push(p.region());
        self.bump();
        let dying = self.dying == Some(p.image);
        let Some(image) = self.image_mut(b, p.image) else {
            return;
        };
        image.placements = image.placements.saturating_sub(1);
        let left = image.placements;
        let held = image.hold > 0;
        if left > 0 && !dying {
            image.bytes = image.bytes.saturating_sub(PLACEMENT_OVERHEAD);
            self.give_back(b, p.image, PLACEMENT_OVERHEAD);
        }
        if free_image && left == 0 && !held && rules(p.protocol.0).freed_with_last_placement {
            self.free_image(b, p.image);
        }
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
        let Some(buf) = self.buffers.get_mut(b) else {
            return;
        };
        let buf = core::mem::take(buf);
        for p in buf.placements.values() {
            self.changed.push(p.region());
        }
        for image in buf.images.values() {
            self.owed = self.owed.saturating_sub(image.owed);
            self.used = self
                .used
                .saturating_sub(image.bytes.saturating_sub(image.owed));
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
    /// the source's pixel size, fitted and centred in a box of both sides,
    /// or one side given and the other following the source's aspect. A
    /// picture scaled to a sliver keeps a pixel; one scaled past the
    /// largest box is refused.
    fn fit(
        place: &Place,
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
        match (place.cols, place.rows) {
            (0, 0) => {
                (cols, rows) = (0, 0);
                (dw, dh) = (u64::from(sw), u64::from(sh));
            }
            (c, 0) => {
                (cols, rows) = (c, 0);
                dw = u64::from(c).saturating_mul(cw64);
                dh = scale(dw, sh64, sw64);
            }
            (0, r) => {
                (cols, rows) = (0, r);
                dh = u64::from(r).saturating_mul(ch64);
                dw = scale(dh, sw64, sh64);
            }
            (c, r) => {
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
