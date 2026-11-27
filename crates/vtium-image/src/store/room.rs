//! The room the store keeps under its budget: the holds that keep an
//! image from eviction, the bytes reserved for an image to come, the
//! settling of what each screen still shows, the eviction, the frees of
//! placements and images, and the spare buffer.

use std::collections::BTreeMap;

use crate::bounds::{IMAGES_PER_BUFFER, PLACEMENT_OVERHEAD, PLACEMENTS_PER_BUFFER};
use crate::image::{ImageKey, Pixels, PlacementKey, Refusal};
use crate::rules::rules;

use super::{Anim, Drawn, Region, Rows, Screens, Spot, Store};

impl Store {
    pub(super) fn hold(&mut self, b: usize, image: Option<ImageKey>, on: bool) {
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
    pub(super) fn reserve_bytes(
        &mut self,
        bytes: u64,
        keep: Option<(usize, ImageKey)>,
        screens: Screens<'_>,
    ) -> Result<u64, Refusal> {
        let credit = keep
            .and_then(|(b, k)| self.image(b, k))
            .map_or(0, |i| i.bytes.saturating_sub(i.owed));
        self.reserve_bytes_with(bytes, keep, credit, screens)
    }

    /// Reserves `bytes` beside `keep`, an image that stays and keeps its
    /// bytes counted: nothing it holds counts toward the request, and it
    /// is not evicted to make room (ADR-1100, D16).
    pub(super) fn reserve_bytes_beside(
        &mut self,
        bytes: u64,
        keep: Option<(usize, ImageKey)>,
        screens: Screens<'_>,
    ) -> Result<u64, Refusal> {
        self.reserve_bytes_with(bytes, keep, 0, screens)
    }

    /// Reserves `bytes`, of which `credit` is already held by `keep`.
    fn reserve_bytes_with(
        &mut self,
        bytes: u64,
        keep: Option<(usize, ImageKey)>,
        credit: u64,
        screens: Screens<'_>,
    ) -> Result<u64, Refusal> {
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
    pub(super) fn give_back(&mut self, b: usize, key: ImageKey, bytes: u64) {
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
    pub(super) fn settle(&mut self, screens: Screens<'_>) {
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
    pub(super) fn victim(
        &mut self,
        only: Option<usize>,
        screens: Screens<'_>,
    ) -> Option<(usize, ImageKey)> {
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
    pub(super) fn make_image_room(
        &mut self,
        b: usize,
        screens: Screens<'_>,
    ) -> Result<(), Refusal> {
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
    pub(super) fn make_placement_room(
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
    pub(super) fn free_placement(&mut self, b: usize, key: PlacementKey, free_image: bool) {
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
    pub(super) fn mark_image(&mut self, b: usize, key: ImageKey) {
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
    pub(super) fn free_image(&mut self, b: usize, key: ImageKey) {
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
    pub(super) fn spare_bytes(&self) -> u64 {
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
    pub(super) fn clear(&mut self, b: usize) {
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
    pub(super) fn tops(&mut self, b: usize, rows: Rows<'_>) -> BTreeMap<PlacementKey, i64> {
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
