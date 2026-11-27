//! Placing an image on a screen, at a cell, beside a parent or as a
//! virtual placement.

use crate::bounds::{BOX_CELLS_MAX, PLACEMENT_OVERHEAD};
use crate::image::{ImageKey, Place, PlacementKey, Refusal};
use crate::rules::{Kept, rules};

use super::{Geometry, Link, Placement, Placing, Screens, Spot, Store};

impl Store {
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
}
