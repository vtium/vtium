//! The scaled-copy cache: each display keeps the pictures it draws at
//! another size, or turned, so a picture is not scaled again every frame
//! (ADR-1100 D5). It is the renderer's own, outside the image budget, so
//! no copy ever evicts a stored image.

use std::collections::BTreeMap;

/// A whole picture as one display draws it: the pixels it shows, the part
/// of them it draws, the size it is drawn at and the display's turn. The
/// image's key, the number of the frame shown and that frame's pixel
/// revision name the pixels (ADR-1102 D3): the store never reuses an
/// image's key, and gives a frame a new pixel revision whenever a command
/// changes what it shows. The image's size is kept with them all the same.
/// Nothing about where the picture sits on the screen is in the key, so a
/// picture that moves, scrolls off the top or is cut into pieces keeps its
/// copy.
// `Copies::drop_older` finds a frame's copies as one range of keys, so
// `image`, `frame` and `pixel_revision` stay the first three fields, which
// the derived order compares first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub(crate) image: u64,
    /// The frame shown, 1 for the root frame and for a still picture.
    pub(crate) frame: u32,
    pub(crate) pixel_revision: u64,
    pub(crate) image_size: (u32, u32),
    /// The source rectangle: x, y, width and height in the image's pixels.
    pub(crate) src: (u32, u32, u32, u32),
    /// The size the picture is drawn at, in the grid's pixels, unturned.
    pub(crate) size: (u32, u32),
    pub(crate) turn: u8,
}

impl Key {
    /// The bytes a copy of this picture holds.
    pub(crate) fn bytes(&self) -> u64 {
        u64::from(self.size.0)
            .saturating_mul(u64::from(self.size.1))
            .saturating_mul(4)
    }
}

/// A key of a picture's kept pixels whose image, animation frame and pixel
/// revision come first in its order, so the keys of one frame's older
/// revisions are one range of keys.
pub(crate) trait Name: Ord + Copy {
    /// The range of keys of this key's image and frame at an older pixel
    /// revision: from the first key of the frame to the first of this key's
    /// revision.
    fn older(&self) -> (Self, Self);
}

impl Name for Key {
    fn older(&self) -> (Key, Key) {
        let first = Key {
            image: self.image,
            frame: self.frame,
            pixel_revision: 0,
            image_size: (0, 0),
            src: (0, 0, 0, 0),
            size: (0, 0),
            turn: 0,
        };
        let end = Key {
            pixel_revision: self.pixel_revision,
            ..first
        };
        (first, end)
    }
}

/// What the cache did, since the renderer was made, and what it holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PictureCounts {
    /// Times a picture was scaled or turned, into the cache or for one
    /// frame alone.
    pub scaled: u64,
    /// Of those, copies the cache kept.
    pub kept: u64,
    /// Of those, pictures drawn without being kept, because the copy would
    /// pass the cache's bound.
    pub uncached: u64,
    /// Copies evicted to make room for another.
    pub evicted: u64,
    /// Runs of display pixels a picture was drawn in, each one display
    /// line of one picture across the cells drawn there together.
    pub lines: u64,
    /// Bytes the kept copies hold.
    pub bytes: u64,
    /// Copies kept.
    pub copies: usize,
}

#[derive(Debug)]
struct Entry<V> {
    /// The picture's pixels: on the CPU path the picture in display order,
    /// turned for the display; on the GPU path its textures.
    value: V,
    /// The bytes it counts against the bound.
    bytes: u64,
    /// The frame that last placed it.
    placed: u64,
}

/// A display's kept pictures, at most `limit` bytes of them.
///
/// The CPU path keeps its scaled copies here, and the GPU path its picture
/// textures, under one rule (ADR-1303 D2). A value the cache lets go, by
/// eviction, by an older revision's drop, by [`Copies::idle`] or by
/// [`Copies::clear`], waits in [`Copies::take_gone`] for its owner, which
/// frees a texture in GL.
#[derive(Debug)]
pub(crate) struct Copies<K = Key, V = Vec<u32>> {
    entries: BTreeMap<K, Entry<V>>,
    limit: u64,
    frame: u64,
    counts: PictureCounts,
    gone: Vec<V>,
}

impl<K: Name, V> Copies<K, V> {
    pub(crate) fn new(limit: u64) -> Copies<K, V> {
        Copies {
            entries: BTreeMap::new(),
            limit,
            frame: 0,
            counts: PictureCounts::default(),
            gone: Vec::new(),
        }
    }

    pub(crate) fn set_limit(&mut self, limit: u64) {
        self.limit = limit;
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.saturating_add(1);
    }

    /// A placement of the frame being drawn shows `key`: whether its copy
    /// is kept, which keeps it from eviction for the frame.
    pub(crate) fn place(&mut self, key: &K) -> bool {
        let frame = self.frame;
        self.entries
            .get_mut(key)
            .map(|e| e.placed = frame)
            .is_some()
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|e| &e.value)
    }

    /// Makes room for `bytes` under `key`, and says whether they fit. It
    /// first drops the copies of the same animation frame at an older pixel
    /// revision that no placement in this draw shows, since the store
    /// never shows a frame's older pixels again. Then it evicts the copies
    /// that no placement in this draw shows, least recently placed first
    /// and then by key, and only when that makes the copy fit.
    pub(crate) fn room(&mut self, key: &K, bytes: u64) -> bool {
        self.drop_older(key);
        let frame = self.frame;
        let free = |c: &Copies<K, V>| c.limit.saturating_sub(c.counts.bytes);
        if bytes <= free(self) {
            return true;
        }
        let mut victims: Vec<(u64, K)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.placed != frame)
            .map(|(k, e)| (e.placed, *k))
            .collect();
        let evictable = victims.iter().fold(0u64, |n, (_, k)| {
            n.saturating_add(self.entries.get(k).map_or(0, |e| e.bytes))
        });
        if bytes > free(self).saturating_add(evictable) {
            return false;
        }
        victims.sort_unstable();
        for (_, k) in victims {
            if bytes <= free(self) {
                break;
            }
            self.remove(&k);
            self.counts.evicted = self.counts.evicted.saturating_add(1);
        }
        true
    }

    /// Lets `key`'s entry go, its value to [`Copies::take_gone`].
    fn remove(&mut self, key: &K) {
        if let Some(e) = self.entries.remove(key) {
            self.counts.bytes = self.counts.bytes.saturating_sub(e.bytes);
            self.gone.push(e.value);
        }
    }

    /// Drops the copies of `key`'s image and animation frame at an older
    /// pixel revision, except those a placement in this draw shows.
    fn drop_older(&mut self, key: &K) {
        let (first, end) = key.older();
        let frame = self.frame;
        let older: Vec<K> = self
            .entries
            .range(first..end)
            .filter(|(_, e)| e.placed != frame)
            .map(|(k, _)| *k)
            .collect();
        for k in older {
            self.remove(&k);
        }
    }

    /// Keeps `value`, `bytes` of it, as `key`'s, which [`Copies::room`]
    /// made room for.
    pub(crate) fn keep(&mut self, key: K, value: V, bytes: u64) {
        self.counts.scaled = self.counts.scaled.saturating_add(1);
        self.counts.kept = self.counts.kept.saturating_add(1);
        self.counts.bytes = self.counts.bytes.saturating_add(bytes);
        let placed = self.frame;
        if let Some(old) = self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                placed,
            },
        ) {
            self.counts.bytes = self.counts.bytes.saturating_sub(old.bytes);
            self.gone.push(old.value);
        }
    }

    /// A picture is scaled for this frame alone.
    pub(crate) fn uncached(&mut self) {
        self.counts.scaled = self.counts.scaled.saturating_add(1);
        self.counts.uncached = self.counts.uncached.saturating_add(1);
    }

    /// Lets go every entry no frame has placed in the last `frames`
    /// frames, this one included, so pictures a display stopped showing
    /// give their room back though the bound is not reached (the GPU
    /// path's textures, ADR-1303 D9).
    #[cfg(feature = "gpu")]
    pub(crate) fn idle(&mut self, frames: u64) {
        let since = self.frame.saturating_sub(frames);
        let idle: Vec<K> = self
            .entries
            .iter()
            .filter(|(_, e)| e.placed <= since)
            .map(|(k, _)| *k)
            .collect();
        for k in idle {
            self.remove(&k);
        }
    }

    /// Lets go every entry.
    #[cfg(feature = "gpu")]
    pub(crate) fn clear(&mut self) {
        let all: Vec<K> = self.entries.keys().copied().collect();
        for k in all {
            self.remove(&k);
        }
    }

    /// The values the cache let go since the last call.
    pub(crate) fn take_gone(&mut self) -> Vec<V> {
        core::mem::take(&mut self.gone)
    }

    pub(crate) fn counts(&self) -> PictureCounts {
        PictureCounts {
            copies: self.entries.len(),
            ..self.counts
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(image: u64, side: u32) -> Key {
        Key {
            image,
            frame: 1,
            pixel_revision: 1,
            image_size: (side, side),
            src: (0, 0, side, side),
            size: (side, side),
            turn: 0,
        }
    }

    fn keep(c: &mut Copies, k: Key) -> bool {
        let fits = c.room(&k, k.bytes());
        if fits {
            c.keep(
                k,
                vec![0; usize::try_from(k.bytes() / 4).unwrap_or(0)],
                k.bytes(),
            );
        }
        fits
    }

    #[test]
    fn a_copy_this_frame_places_is_never_evicted() {
        // room for two 4x4 copies
        let mut c = Copies::new(128);
        c.begin_frame();
        assert!(keep(&mut c, key(1, 4)));
        assert!(keep(&mut c, key(2, 4)));
        // both are this frame's: a third does not fit, and nothing goes
        assert!(!keep(&mut c, key(3, 4)));
        assert_eq!(c.counts().evicted, 0);
        c.begin_frame();
        assert!(c.place(&key(2, 4)));
        // copy 1 is not placed now and goes; copy 2 stays
        assert!(keep(&mut c, key(3, 4)));
        assert_eq!(c.counts().evicted, 1);
        assert!(c.get(&key(1, 4)).is_none());
        assert!(c.get(&key(2, 4)).is_some());
    }

    #[test]
    fn a_copy_that_only_this_frames_copies_could_make_room_for_evicts_nothing() {
        let mut c = Copies::new(128);
        c.begin_frame();
        assert!(keep(&mut c, key(1, 4)));
        c.begin_frame();
        assert!(keep(&mut c, key(2, 4)));
        c.begin_frame();
        assert!(c.place(&key(1, 4)));
        // 96 bytes: copy 2's 64 are not enough, and copy 1 is this frame's
        let wide = Key {
            size: (6, 4),
            ..key(3, 4)
        };
        assert!(!keep(&mut c, wide));
        assert_eq!(c.counts().evicted, 0);
        assert!(c.get(&key(2, 4)).is_some());
    }

    #[test]
    fn nothing_is_evicted_for_a_copy_that_would_not_fit_anyway() {
        let mut c = Copies::new(128);
        c.begin_frame();
        assert!(keep(&mut c, key(1, 4)));
        c.begin_frame();
        // 6x6 is 144 bytes: past the bound alone
        assert!(!keep(&mut c, key(2, 6)));
        assert_eq!(c.counts().evicted, 0);
        assert!(c.get(&key(1, 4)).is_some());
    }

    // ADR-1102 D3: a frame's older pixel revision never shows again, so its
    // copy goes once the newer one is made, unless this frame still shows it
    #[test]
    fn a_newer_pixel_revision_of_a_frame_drops_the_older_copy() {
        let at = |frame: u32, pixel_revision: u64| Key {
            frame,
            pixel_revision,
            ..key(1, 4)
        };
        let mut c = Copies::new(1000);
        c.begin_frame();
        assert!(keep(&mut c, at(2, 5)));
        assert!(keep(&mut c, at(3, 5)));
        assert!(keep(&mut c, key(2, 4)));
        c.begin_frame();
        assert!(c.place(&at(2, 5)));
        // frame 2 at revision 5 is this frame's: it stays beside revision 6
        assert!(keep(&mut c, at(2, 6)));
        assert!(c.get(&at(2, 5)).is_some());
        c.begin_frame();
        assert!(keep(&mut c, at(2, 9)));
        assert!(c.get(&at(2, 5)).is_none());
        assert!(c.get(&at(2, 6)).is_none());
        // another frame, and another image, keep theirs
        assert!(c.get(&at(3, 5)).is_some());
        assert!(c.get(&key(2, 4)).is_some());
        assert_eq!((c.counts().copies, c.counts().bytes), (3, 3 * 64));
        assert_eq!(c.counts().evicted, 0);
    }

    #[test]
    fn the_bound_holds_whatever_is_kept() {
        let mut c = Copies::new(1000);
        for frame in 0..50u64 {
            c.begin_frame();
            for i in 0..7 {
                let side = 3 + u32::try_from((frame * 7 + i) % 9).unwrap_or(0);
                keep(&mut c, key(frame * 7 + i, side));
                assert!(c.counts().bytes <= 1000);
            }
        }
        assert!(c.counts().evicted > 0);
    }
}
