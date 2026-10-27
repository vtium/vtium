//! The drawn-glyph cache (ADR-1300 D1 to D4).
//!
//! A glyph is cached under its text (the cluster's codepoints), bold,
//! italic and its width in cells, so the same text in the same style is
//! drawn by the font once however often the screen re-creates it. Glyphs
//! live in four pools, masks and colour glyphs each one or two cells wide,
//! and each pool has its own least-recently-used order, a glyph count and a
//! byte ceiling. Recency is the drawn frame. A glyph that would evict one
//! the frame draws, or that alone passes its pool's ceiling, is drawn
//! without being cached, so a full cache costs a font call and never a
//! blank cell.
//!
//! The table is open addressing with a fixed hash, and nothing iterates it
//! to draw (docs/architecture.md, "Determinism").

use vtium_font::{Face, Font, Style, Width};

use crate::bounds::POOL_BYTES_PERFORMANCE;
use crate::stored::Stored;
use crate::target::Orientation;

/// The cache's budgets, chosen by the `glyph-cache` configuration key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Twice the screen's cells of narrow glyphs and once of wide, each
    /// pool's bytes at most two screens of its format.
    Minspec,
    /// Twice minspec's glyphs, each pool's bytes at most 64 MiB, as the C
    /// reference's GPU renderer allowed.
    Performance,
}

impl Default for Profile {
    /// `performance` in a build with the `performance-cache` feature, else
    /// `minspec`.
    fn default() -> Profile {
        if cfg!(feature = "performance-cache") {
            Profile::Performance
        } else {
            Profile::Minspec
        }
    }
}

/// How many glyphs and bytes each pool may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budgets {
    /// Glyphs in each one-cell pool.
    pub narrow: u32,
    /// Glyphs in each two-cell pool.
    pub wide: u32,
    /// Bytes in each mask pool.
    pub mask_bytes: u64,
    /// Bytes in each colour pool.
    pub colour_bytes: u64,
}

impl Budgets {
    /// `profile`'s budgets for a display `width` by `height` pixels that
    /// holds `cells` cells.
    pub fn for_profile(profile: Profile, cells: u32, width: u32, height: u32) -> Budgets {
        let screen = u64::from(width).saturating_mul(u64::from(height));
        match profile {
            Profile::Minspec => Budgets {
                narrow: cells.saturating_mul(2),
                wide: cells,
                mask_bytes: screen.saturating_mul(2),
                colour_bytes: screen.saturating_mul(8),
            },
            Profile::Performance => Budgets {
                narrow: cells.saturating_mul(4),
                wide: cells.saturating_mul(2),
                mask_bytes: POOL_BYTES_PERFORMANCE,
                colour_bytes: POOL_BYTES_PERFORMANCE,
            },
        }
    }

    /// `n` glyphs in every pool, with the performance profile's byte
    /// ceilings: the sizes the cache-invariance test draws at.
    pub fn glyphs(n: u32) -> Budgets {
        Budgets {
            narrow: n,
            wide: n,
            mask_bytes: POOL_BYTES_PERFORMANCE,
            colour_bytes: POOL_BYTES_PERFORMANCE,
        }
    }

    fn pool(&self, pool: usize) -> (u32, u64) {
        let count = if pool & WIDE != 0 {
            self.wide
        } else {
            self.narrow
        };
        let bytes = if pool & COLOUR != 0 {
            self.colour_bytes
        } else {
            self.mask_bytes
        };
        (count, bytes)
    }
}

/// How glyphs are stored. Both are on by default; turned off, they let
/// tier 2 check that each changes no pixel (ADR-1300, "Verification").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Storage {
    /// A mask of only 0 and 255 is stored at 1 bit a pixel.
    pub bits: bool,
    /// A bitmap font's glyph is stored at the font's own size and scaled
    /// while drawn.
    pub unscaled: bool,
}

impl Default for Storage {
    fn default() -> Storage {
        Storage {
            bits: true,
            unscaled: true,
        }
    }
}

const NIL: u32 = u32::MAX;
const WIDE: usize = 1;
const COLOUR: usize = 2;

/// A glyph's text: one codepoint held in place, or a cluster's.
#[derive(Debug)]
enum Text {
    One(char),
    Many(Box<[char]>),
}

impl Text {
    fn chars(&self) -> &[char] {
        match self {
            Text::One(c) => core::slice::from_ref(c),
            Text::Many(cs) => cs,
        }
    }
}

#[derive(Debug)]
struct Entry {
    text: Text,
    style: Style,
    wide: bool,
    hash: u64,
    glyph: Stored,
    pool: usize,
    /// The drawn frame that last used it.
    used: u64,
    /// Toward the pool's most recently used.
    prev: u32,
    /// Toward the pool's least recently used.
    next: u32,
}

#[derive(Clone, Copy, Debug)]
struct Pool {
    /// Most recently used.
    head: u32,
    /// Least recently used: the next to go.
    tail: u32,
    count: u32,
    bytes: u64,
}

const EMPTY_POOL: Pool = Pool {
    head: NIL,
    tail: NIL,
    count: 0,
    bytes: 0,
};

/// Each codepoint folded in with one multiply by the golden ratio's
/// 64-bit constant, after the style; the high half folded into the low
/// bits a slot is chosen by.
fn hash(text: &[char], style: Style, wide: bool) -> u64 {
    const K: u64 = 0x9e37_79b9_7f4a_7c15;
    let flags = u64::from(style.bold) | u64::from(style.italic) << 1 | u64::from(wide) << 2;
    let h = text.iter().fold(flags, |h, &c| {
        (h.rotate_left(21) ^ u64::from(u32::from(c))).wrapping_mul(K)
    });
    h ^ h >> 32
}

/// Glyph lookups since the counts were last taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheCounts {
    /// Glyphs the font drew.
    pub misses: u32,
    /// Of those, the glyphs drawn without being cached.
    pub uncached: u32,
}

/// The drawn glyphs a renderer keeps.
#[derive(Debug)]
pub(crate) struct GlyphCache {
    budgets: Budgets,
    storage: Storage,
    orientation: Orientation,
    /// Entry indices by hash, `NIL` when free; a power of two long.
    slots: Vec<u32>,
    entries: Vec<Option<Entry>>,
    free: Vec<u32>,
    live: usize,
    pools: [Pool; 4],
    /// The glyph drawn uncached, kept until the next.
    spare: Option<Stored>,
    frame: u64,
    counts: CacheCounts,
}

impl GlyphCache {
    pub(crate) fn new(budgets: Budgets, storage: Storage, orientation: Orientation) -> GlyphCache {
        GlyphCache {
            budgets,
            storage,
            orientation,
            slots: vec![NIL; 16],
            entries: Vec::new(),
            free: Vec::new(),
            live: 0,
            pools: [EMPTY_POOL; 4],
            spare: None,
            frame: 0,
            counts: CacheCounts::default(),
        }
    }

    /// Starts a drawn frame: glyphs used from now on are this frame's.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// The counts since they were last taken, and resets them.
    pub(crate) fn take_counts(&mut self) -> CacheCounts {
        core::mem::take(&mut self.counts)
    }

    /// Bytes of glyph pixels the cache holds.
    pub(crate) fn pixel_bytes(&self) -> u64 {
        self.pools.iter().fold(0, |n, p| n.saturating_add(p.bytes))
    }

    /// The glyph for `text` in `style`, `width` cells wide: cached, drawn
    /// and cached, or drawn uncached. `None` when the font draws nothing.
    pub(crate) fn get<F: Face>(
        &mut self,
        font: &Font<F>,
        text: &[char],
        width: Width,
        style: Style,
    ) -> Option<&Stored> {
        let wide = width == Width::Wide;
        let h = hash(text, style, wide);
        if let Some(i) = self.find(h, text, style, wide) {
            self.touch(i);
            return self.entry(i).map(|e| &e.glyph);
        }
        self.counts.misses = self.counts.misses.saturating_add(1);
        let (glyph, scale) = if self.storage.unscaled {
            font.glyph_unscaled(text, width, style)?
        } else {
            (font.glyph(text, width, style)?, 1)
        };
        let glyph = Stored::new(&glyph, scale, self.orientation, self.storage.bits);
        let pool = if glyph.is_rgba() { COLOUR } else { 0 } | if wide { WIDE } else { 0 };
        if !self.make_room(pool, glyph.bytes()) {
            self.counts.uncached = self.counts.uncached.saturating_add(1);
            return Some(self.spare.insert(glyph));
        }
        let text = match *text {
            [c] => Text::One(c),
            _ => Text::Many(text.into()),
        };
        let i = self.insert(Entry {
            text,
            style,
            wide,
            hash: h,
            glyph,
            pool,
            used: self.frame,
            prev: NIL,
            next: NIL,
        })?;
        self.entry(i).map(|e| &e.glyph)
    }

    fn entry(&self, i: u32) -> Option<&Entry> {
        self.entries.get(usize::try_from(i).ok()?)?.as_ref()
    }

    fn entry_mut(&mut self, i: u32) -> Option<&mut Entry> {
        self.entries.get_mut(usize::try_from(i).ok()?)?.as_mut()
    }

    fn mask(&self) -> usize {
        self.slots.len().wrapping_sub(1)
    }

    fn home(&self, hash: u64) -> usize {
        // the low bits of a 64-bit hash; the table is never 2^32 long
        usize::try_from(hash & 0xffff_ffff).unwrap_or(0) & self.mask()
    }

    fn find(&self, h: u64, text: &[char], style: Style, wide: bool) -> Option<u32> {
        let mut at = self.home(h);
        loop {
            let i = *self.slots.get(at)?;
            if i == NIL {
                return None;
            }
            let e = self.entry(i)?;
            if e.hash == h && e.style == style && e.wide == wide && e.text.chars() == text {
                return Some(i);
            }
            at = at.wrapping_add(1) & self.mask();
        }
    }

    /// Whether pool `pool` can take a glyph of `bytes` without evicting one
    /// this frame draws, evicting older glyphs to make room.
    fn make_room(&mut self, pool: usize, bytes: usize) -> bool {
        let (count, ceiling) = self.budgets.pool(pool);
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        if count == 0 || bytes > ceiling {
            return false;
        }
        loop {
            let Some(p) = self.pools.get(pool) else {
                return false;
            };
            if p.count < count && p.bytes.saturating_add(bytes) <= ceiling {
                return true;
            }
            let victim = p.tail;
            match self.entry(victim) {
                Some(e) if e.used != self.frame => self.evict(victim),
                _ => return false,
            }
        }
    }

    fn insert(&mut self, e: Entry) -> Option<u32> {
        if self.live.saturating_add(1).saturating_mul(2) > self.slots.len() {
            self.grow();
        }
        let (h, pool, bytes) = (e.hash, e.pool, e.glyph.bytes());
        let i = match self.free.pop() {
            Some(i) => {
                *self.entries.get_mut(usize::try_from(i).ok()?)? = Some(e);
                i
            }
            None => {
                let i = u32::try_from(self.entries.len())
                    .ok()
                    .filter(|&i| i != NIL)?;
                self.entries.push(Some(e));
                i
            }
        };
        self.place(i, h);
        self.live = self.live.saturating_add(1);
        let p = self.pools.get_mut(pool)?;
        p.count = p.count.saturating_add(1);
        p.bytes = p
            .bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        self.push_front(i);
        Some(i)
    }

    /// Puts entry `i` in the first free slot from its home.
    fn place(&mut self, i: u32, h: u64) {
        let mut at = self.home(h);
        while let Some(s) = self.slots.get_mut(at) {
            if *s == NIL {
                *s = i;
                return;
            }
            at = at.wrapping_add(1) & self.mask();
        }
    }

    fn grow(&mut self) {
        let len = self.slots.len().saturating_mul(2);
        self.slots = vec![NIL; len];
        for i in 0..self.entries.len() {
            let Ok(i) = u32::try_from(i) else { break };
            if let Some(h) = self.entry(i).map(|e| e.hash) {
                self.place(i, h);
            }
        }
    }

    fn evict(&mut self, i: u32) {
        self.unlink(i);
        let Some(e) = self
            .entries
            .get_mut(usize::try_from(i).unwrap_or(usize::MAX))
            .and_then(Option::take)
        else {
            return;
        };
        if let Some(p) = self.pools.get_mut(e.pool) {
            p.count = p.count.saturating_sub(1);
            p.bytes = p
                .bytes
                .saturating_sub(u64::try_from(e.glyph.bytes()).unwrap_or(0));
        }
        self.free.push(i);
        self.live = self.live.saturating_sub(1);
        // take it out of the table, moving later entries of its run back
        let mut hole = self.home(e.hash);
        while self.slots.get(hole).is_some_and(|&s| s != i) {
            hole = hole.wrapping_add(1) & self.mask();
        }
        let mut at = hole;
        loop {
            at = at.wrapping_add(1) & self.mask();
            let s = self.slots.get(at).copied().unwrap_or(NIL);
            if s == NIL {
                break;
            }
            let Some(home) = self.entry(s).map(|e| self.home(e.hash)) else {
                break;
            };
            // `s` may fill the hole when its home is not between the hole
            // and its slot, cyclically
            let dist = |from: usize, to: usize| to.wrapping_sub(from) & self.mask();
            if dist(home, at) >= dist(hole, at) {
                if let Some(slot) = self.slots.get_mut(hole) {
                    *slot = s;
                }
                hole = at;
            }
        }
        if let Some(slot) = self.slots.get_mut(hole) {
            *slot = NIL;
        }
    }

    fn unlink(&mut self, i: u32) {
        let Some((pool, prev, next)) = self.entry(i).map(|e| (e.pool, e.prev, e.next)) else {
            return;
        };
        match self.entry_mut(prev) {
            Some(p) => p.next = next,
            None => {
                if let Some(p) = self.pools.get_mut(pool) {
                    p.head = next;
                }
            }
        }
        match self.entry_mut(next) {
            Some(n) => n.prev = prev,
            None => {
                if let Some(p) = self.pools.get_mut(pool) {
                    p.tail = prev;
                }
            }
        }
    }

    fn push_front(&mut self, i: u32) {
        let Some(pool) = self.entry(i).map(|e| e.pool) else {
            return;
        };
        let Some(head) = self.pools.get(pool).map(|p| p.head) else {
            return;
        };
        if let Some(e) = self.entry_mut(i) {
            e.prev = NIL;
            e.next = head;
        }
        match self.entry_mut(head) {
            Some(h) => h.prev = i,
            None => {
                if let Some(p) = self.pools.get_mut(pool) {
                    p.tail = i;
                }
            }
        }
        if let Some(p) = self.pools.get_mut(pool) {
            p.head = i;
        }
    }

    /// Marks entry `i` used by the frame. Recency is the frame, so a glyph
    /// the frame already used is already among its pool's newest.
    fn touch(&mut self, i: u32) {
        let frame = self.frame;
        match self.entry_mut(i) {
            Some(e) if e.used != frame => e.used = frame,
            _ => return,
        }
        self.unlink(i);
        self.push_front(i);
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use vtium_font::Psf;

    /// A 256-glyph 8x16 font without a table, glyph i's rows all i.
    fn font() -> Font<Psf> {
        let mut b = vec![0x72, 0xb5, 0x4a, 0x86];
        for v in [0u32, 32, 0, 256, 16, 16, 8] {
            b.extend(v.to_le_bytes());
        }
        for i in 0..=255u8 {
            b.extend([i; 16]);
        }
        Font::new(Psf::parse(&b).unwrap())
    }

    fn get(c: &mut GlyphCache, f: &Font<Psf>, ch: char) -> bool {
        c.get(f, &[ch], Width::Narrow, Style::default()).is_some()
    }

    #[test]
    fn a_hit_draws_nothing_and_a_miss_draws_once() {
        let f = font();
        let mut c = GlyphCache::new(Budgets::glyphs(8), Storage::default(), Orientation::Normal);
        c.begin_frame();
        for ch in ['a', 'b', 'a', 'a', 'b'] {
            assert!(get(&mut c, &f, ch));
        }
        assert_eq!(
            c.take_counts(),
            CacheCounts {
                misses: 2,
                uncached: 0
            }
        );
        let bold = Style {
            bold: true,
            italic: false,
        };
        assert!(c.get(&f, &['a'], Width::Narrow, bold).is_some());
        assert!(c.get(&f, &['a'], Width::Wide, Style::default()).is_some());
        assert!(
            c.get(&f, &['a', '\u{301}'], Width::Narrow, Style::default())
                .is_some()
        );
        assert_eq!(c.take_counts().misses, 3);
    }

    #[test]
    fn a_full_pool_evicts_only_glyphs_older_than_the_frame() {
        let f = font();
        let mut c = GlyphCache::new(Budgets::glyphs(2), Storage::default(), Orientation::Normal);
        c.begin_frame();
        for ch in ['a', 'b', 'c'] {
            assert!(get(&mut c, &f, ch));
        }
        // 'c' would have evicted a glyph this frame drew
        assert_eq!(
            c.take_counts(),
            CacheCounts {
                misses: 3,
                uncached: 1
            }
        );
        c.begin_frame();
        assert!(get(&mut c, &f, 'b'));
        assert!(get(&mut c, &f, 'c'));
        assert_eq!(
            c.take_counts(),
            CacheCounts {
                misses: 1,
                uncached: 0
            }
        );
        // 'a' went, 'b' stayed
        assert!(get(&mut c, &f, 'b'));
        assert_eq!(c.take_counts().misses, 0);
        c.begin_frame();
        assert!(get(&mut c, &f, 'a'));
        assert_eq!(c.take_counts().misses, 1);
    }

    #[test]
    fn a_cache_of_nothing_draws_every_glyph() {
        let f = font();
        let mut c = GlyphCache::new(Budgets::glyphs(0), Storage::default(), Orientation::Normal);
        c.begin_frame();
        for _ in 0..3 {
            assert!(get(&mut c, &f, 'a'));
        }
        assert_eq!(
            c.take_counts(),
            CacheCounts {
                misses: 3,
                uncached: 3
            }
        );
        assert_eq!(c.pixel_bytes(), 0);
    }

    #[test]
    fn a_byte_ceiling_bounds_a_pool() {
        let f = font();
        let budgets = Budgets {
            mask_bytes: 40,
            ..Budgets::glyphs(100)
        };
        let mut c = GlyphCache::new(budgets, Storage::default(), Orientation::Normal);
        // each glyph is 16 bytes at 1 bit
        for (i, ch) in ('A'..='Z').enumerate() {
            c.begin_frame();
            assert!(get(&mut c, &f, ch), "{i}");
            assert!(c.pixel_bytes() <= 40);
        }
        assert_eq!(c.pixel_bytes(), 32);
        let wide = Budgets {
            mask_bytes: 8,
            ..Budgets::glyphs(100)
        };
        let mut c = GlyphCache::new(wide, Storage::default(), Orientation::Normal);
        c.begin_frame();
        assert!(get(&mut c, &f, 'a'));
        assert_eq!(c.take_counts().uncached, 1);
    }

    // Many inserts and evictions through a small table keep every glyph
    // findable: the backward shift never strands an entry.
    #[test]
    fn the_table_survives_churn() {
        let f = font();
        let mut c = GlyphCache::new(Budgets::glyphs(37), Storage::default(), Orientation::Normal);
        for round in 0..50u32 {
            c.begin_frame();
            for k in 0..40u32 {
                let ch = char::from_u32(0x20 + (round * 7 + k * 13) % 200).unwrap();
                assert!(get(&mut c, &f, ch));
            }
            c.take_counts();
            for k in 0..37u32 {
                let ch = char::from_u32(0x20 + (round * 7 + k * 13) % 200).unwrap();
                c.begin_frame();
                let before = c.take_counts();
                assert_eq!(before.misses, 0);
                assert!(get(&mut c, &f, ch));
                assert_eq!(c.take_counts().misses, 0, "round {round} glyph {k}");
            }
        }
        assert!(c.live <= 37);
    }
}
