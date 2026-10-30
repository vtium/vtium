//! The deferred pipeline's glyphs (ADR-1301 D2): the per-boot memory
//! store of what the helper generated, and the face that draws a cell from
//! it, falling back per character to the built-in font (#141).
//!
//! The store keys a glyph by its cluster's text at a cell (ADR-1200 D20),
//! one codepoint or a cluster of more, in the regular style, so two cells
//! holding the same cluster share its glyph however their clusters were
//! made. The lookup for a cell is ADR-1200 D22's: the cluster's glyph in
//! the store; the cluster's NFC form, where it precomposes to one
//! codepoint the store holds; the base's glyph, the store's or the
//! built-in font's, with each combining mark's glyph overstruck on it, the
//! default ignorables and the rest of an emoji sequence left out; the
//! built-in font for the cell size; and [`vtium_font::Font`]'s replacement
//! chain through this same face, U+25A1 from the frit, else from the
//! built-in. Asking the helper is the console's ([`crate::helper`]).
//!
//! Nothing here is written to disk. The store holds only the cells the
//! displays draw at ([`Generated::retain`]), within a limit taken from the
//! machine's memory ([`store_limit`]): a fill stops at it, and a request
//! past it compacts the store ([`Generated::make_room`], ADR-1200 D19):
//! the glyphs least recently on the screen leave, each set they leave is
//! written again without them, and the store counts the bytes written, so
//! the limit bounds the memory the sets take. An evicted glyph is asked
//! for again when the screen shows it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder, StrikeInfo};
use vtium_font::sprite::Range;
use vtium_font::{
    Cell, Decorations, Face, Format, Glyph, RangeCoverage, SpritePolicy, Style, Width,
};

mod data;

const GIB: u64 = 1 << 30;

/// The store's bytes on a machine with `ram` bytes, as the scrollback takes
/// its own (vtium-term's `sb_size_cap`): 16 MiB below 2 GiB, 64 MiB from 2
/// GiB, 128 MiB from 4 GiB.
pub fn store_limit(ram: u64) -> u64 {
    match ram {
        r if r >= 4 * GIB => 128 << 20,
        r if r >= 2 * GIB => 64 << 20,
        _ => 16 << 20,
    }
}

/// What a glyph is keyed by at a cell (ADR-1200 D20): its cluster's text,
/// one codepoint or more, in the regular style.
pub type Text = Vec<char>;

/// What the displays show: each character, and each cluster of more than
/// one, each sorted and once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub chars: Vec<char>,
    pub clusters: Vec<Text>,
}

/// Whether `c` is a combining mark, drawn over its base.
fn is_mark(c: char) -> bool {
    within(&data::MARKS, c)
}

/// Whether `c` is a default ignorable, drawn as nothing.
fn is_ignorable(c: char) -> bool {
    within(&data::IGNORABLES, c)
}

fn within(ranges: &[(u32, u32)], c: char) -> bool {
    let c = u32::from(c);
    let i = ranges.partition_point(|&(_, last)| last < c);
    ranges.get(i).is_some_and(|&(first, _)| first <= c)
}

/// `c`'s canonical combining class.
fn class(c: char) -> u8 {
    let c = u32::from(c);
    let i = data::CLASSES.partition_point(|&(_, last, _)| last < c);
    match data::CLASSES.get(i) {
        Some(&(first, _, class)) if first <= c => class,
        _ => 0,
    }
}

const HANGUL_S: u32 = 0xac00;
const HANGUL_L: u32 = 0x1100;
const HANGUL_V: u32 = 0x1161;
const HANGUL_T: u32 = 0x11a7;
const HANGUL_V_COUNT: u32 = 21;
const HANGUL_T_COUNT: u32 = 28;
const HANGUL_S_COUNT: u32 = 19 * HANGUL_V_COUNT * HANGUL_T_COUNT;

/// The primary composite of `a` and `b`, a Hangul syllable by rule.
fn compose(a: char, b: char) -> Option<char> {
    let (a, b) = (u32::from(a), u32::from(b));
    let l = a.wrapping_sub(HANGUL_L);
    let v = b.wrapping_sub(HANGUL_V);
    if l < 19 && v < HANGUL_V_COUNT {
        let lv = l.checked_mul(HANGUL_V_COUNT)?.checked_add(v)?;
        return char::from_u32(HANGUL_S.checked_add(lv.checked_mul(HANGUL_T_COUNT)?)?);
    }
    let s = a.wrapping_sub(HANGUL_S);
    let t = b.wrapping_sub(HANGUL_T);
    if s < HANGUL_S_COUNT && s % HANGUL_T_COUNT == 0 && (1..HANGUL_T_COUNT).contains(&t) {
        return char::from_u32(a.checked_add(t)?);
    }
    let i = data::COMPOSE
        .binary_search_by(|&(x, y, _)| (x, y).cmp(&(a, b)))
        .ok()?;
    data::COMPOSE
        .get(i)
        .and_then(|&(_, _, c)| char::from_u32(c))
}

/// The one codepoint `cluster`'s NFC form is, when it is one: each run of
/// its marks of a class but 0 in canonical order, then each composed into
/// the base in turn, as none is left between to block the next.
fn precomposed(cluster: &[char]) -> Option<char> {
    let (&base, rest) = cluster.split_first()?;
    if rest.is_empty() {
        return None;
    }
    let mut marks = rest.to_vec();
    for run in marks.split_mut(|&c| class(c) == 0) {
        run.sort_by_key(|&c| class(c));
    }
    marks.into_iter().try_fold(base, compose)
}

/// One set the helper sent, or its rebuild: its cell, the face it draws
/// through, each text it maps with when the screen last showed it, and its
/// bytes.
#[derive(Debug)]
struct Entry {
    cell: Cell,
    face: SetFace,
    /// Sorted by text.
    glyphs: Vec<(Text, u64)>,
    bytes: u64,
}

/// The glyph `face` draws for `text`: a codepoint's from the set's map, a
/// cluster's from its cluster map, at the face's scale or at 1.
fn draw_text(face: &SetFace, text: &[char], unscaled: bool) -> Option<Glyph> {
    let style = Style::default();
    match text {
        [c] if unscaled => face.draw_unscaled(&[*c], style),
        [c] => face.draw(&[*c], style),
        _ => face.cluster(text, style),
    }
}

impl Entry {
    fn find(&self, text: &[char]) -> Result<usize, usize> {
        self.glyphs
            .binary_search_by(|(t, _)| t.as_slice().cmp(text))
    }

    /// Each glyph's share of the set's bytes, rounded up: what dropping
    /// it is taken to free when the glyphs to evict are chosen.
    fn share(&self) -> u64 {
        let n = u64::try_from(self.glyphs.len()).unwrap_or(u64::MAX).max(1);
        self.bytes.div_ceil(n).max(1)
    }

    /// Drops `gone`, sorted, and writes the set again without them: the
    /// texts it no longer holds, every one of them when the set cannot be
    /// written again or holds nothing after, and the entry then left to be
    /// removed.
    fn compact(&mut self, gone: &[Text]) -> (Vec<Text>, bool) {
        self.glyphs.retain(|(t, _)| gone.binary_search(t).is_err());
        if !self.glyphs.is_empty()
            && let Some((face, bytes)) = rewrite(&self.face, self.cell, &self.glyphs)
        {
            self.face = face;
            self.bytes = bytes;
            return (gone.to_vec(), false);
        }
        let mut all = gone.to_vec();
        all.extend(self.glyphs.iter().map(|(t, _)| t.clone()));
        all.sort_unstable();
        (all, true)
    }
}

/// The set `face` draws, written again through vtium-font's set writer
/// with only `kept`'s glyphs at `cell`, as the helper writes an answer:
/// a strike a pixel format, its chunks raw, a codepoint in its map and a
/// cluster in its cluster map. Each glyph keeps its format: RGBA stays
/// RGBA, and a mask whose every pixel is 0 or 255 in a set with a 1-bit
/// strike stays 1-bit, every other mask R8. The new face and its bytes;
/// `None` when a glyph cannot be drawn or the set cannot be written.
fn rewrite(face: &SetFace, cell: Cell, kept: &[(Text, u64)]) -> Option<(SetFace, u64)> {
    let set = face.set();
    let strikes: Vec<StrikeInfo> = set
        .strikes()
        .filter(|s| s.cell == cell && s.style == Style::default())
        .collect();
    let one_bit = strikes.iter().any(|s| s.format == PixelFormat::Bits);
    let formats = [PixelFormat::Bits, PixelFormat::R8, PixelFormat::Rgba];
    let mut builders = formats.map(|format| {
        let mut s = StrikeBuilder::new(format, cell, Style::default()).compress(false);
        if let Some(info) = strikes.iter().find(|s| s.format == format) {
            s = s.baseline(info.baseline).decorations(info.decorations);
        }
        (s, BTreeMap::<(bool, Vec<u8>), u32>::new())
    });
    for (text, _) in kept {
        let glyph = draw_text(face, text, true)?;
        let pixels = glyph.pixels();
        let (slot, bytes) = match glyph.format() {
            Format::Rgba => (2, pixels.to_vec()),
            Format::Mask if one_bit && pixels.iter().all(|&p| p == 0 || p == 0xff) => {
                (0, pack(&glyph)?)
            }
            Format::Mask => (1, pixels.to_vec()),
        };
        let (strike, placed) = builders.get_mut(slot)?;
        let wide = glyph.cells() == Width::Wide;
        // texts that shared a glyph share it again
        let index = match placed.get(&(wide, bytes.clone())) {
            Some(&i) => i,
            None => {
                let i = strike.glyph(glyph.cells(), bytes.clone());
                placed.insert((wide, bytes), i);
                i
            }
        };
        match text.as_slice() {
            [c] => strike.map(*c, index),
            cluster => strike.cluster(cluster, index),
        }
    }
    let mut builder = SetBuilder::new(set.name()).policy(set.policy());
    for (key, value) in set.meta().filter(|(k, _)| !matches!(*k, "name" | "format")) {
        builder = builder.meta(key, value);
    }
    for (strike, _) in builders {
        if !strike.is_empty() {
            builder = builder.strike(strike);
        }
    }
    let bytes = builder.write().ok()?;
    let len = u64::try_from(bytes.len()).ok()?;
    let set = Set::from_bytes(bytes).ok()?;
    Some((SetFace::new(Arc::new(set), cell.height)?, len))
}

/// A mask of 0 and 255 as 1-bit rows padded to a byte, the first pixel
/// the high bit.
fn pack(glyph: &Glyph) -> Option<Vec<u8>> {
    let row = usize::try_from(glyph.width().div_ceil(8)).ok()?;
    let mut out = Vec::with_capacity(row.checked_mul(usize::try_from(glyph.height()).ok()?)?);
    for line in glyph.rows() {
        let at = out.len();
        out.resize(at.checked_add(row)?, 0);
        for (x, _) in line.iter().enumerate().filter(|&(_, &p)| p != 0) {
            *out.get_mut(at.checked_add(x / 8)?)? |= 0x80 >> (x % 8);
        }
    }
    Some(out)
}

/// The helper's glyphs this boot.
#[derive(Debug, Default)]
pub struct Generated {
    entries: Vec<Entry>,
    /// Counts [`Generated::touch`]es: the store's clock.
    tick: u64,
}

/// The store, shared between the console and its displays' faces.
pub type Shared = Rc<RefCell<Generated>>;

/// A set the helper sent, without the glyphs the store held at its cell
/// when it was made ([`Generated::fresh`]): what adding it takes.
#[derive(Debug)]
pub struct Fresh(Entry);

impl Fresh {
    /// The bytes the store counts for it.
    pub fn bytes(&self) -> u64 {
        self.0.bytes
    }

    /// The texts it adds, in order.
    pub fn texts(&self) -> impl Iterator<Item = &Text> + '_ {
        self.0.glyphs.iter().map(|(t, _)| t)
    }
}

impl Generated {
    /// A checked set of glyphs at `cell`, mapping the codepoints of `runs`
    /// and the clusters its cluster map holds at `cell`, and taking
    /// `bytes`, without the glyphs the store holds at `cell` (ADR-1200
    /// D18): a set that carries some is written again without them, as
    /// compaction writes one ([`rewrite`]), and counts the bytes written.
    /// It adds nothing when it carries nothing new, or when it cannot be
    /// written again. `None` when `set` has no face at `cell`.
    pub fn fresh(
        &self,
        cell: Cell,
        set: Arc<Set>,
        runs: &[(u32, u32)],
        bytes: u64,
    ) -> Option<Fresh> {
        let clusters: Vec<Text> = set
            .clusters()
            .filter(|(at, style, _)| *at == cell && *style == Style::default())
            .map(|(_, _, key)| key)
            .collect();
        let face = SetFace::new(set, cell.height)?;
        let mut glyphs: Vec<(Text, u64)> = runs
            .iter()
            .flat_map(|&(first, n)| (0..n).map(move |i| first.checked_add(i)))
            .filter_map(|c| char::from_u32(c?))
            .map(|c| vec![c])
            .chain(clusters)
            .map(|t| (t, self.tick))
            .collect();
        glyphs.sort_unstable();
        glyphs.dedup_by(|a, b| a.0 == b.0);
        let held: Vec<Text> = glyphs
            .iter()
            .map(|(t, _)| t)
            .filter(|t| self.held(cell, t))
            .cloned()
            .collect();
        let mut entry = Entry {
            cell,
            face,
            glyphs,
            bytes,
        };
        if !held.is_empty() && entry.compact(&held).1 {
            entry.glyphs.clear();
            entry.bytes = 0;
        }
        Some(Fresh(entry))
    }

    /// Adds `fresh`: the face it draws through, for the caller to ask what
    /// it holds; `None` when it adds no glyph.
    pub fn add(&mut self, fresh: Fresh) -> Option<SetFace> {
        let Fresh(entry) = fresh;
        if entry.glyphs.is_empty() {
            return None;
        }
        let face = entry.face.clone();
        self.entries.push(entry);
        Some(face)
    }

    /// Whether the store holds at `cell` the glyph of a cluster `c` begins.
    fn begins(&self, cell: Cell, c: char) -> bool {
        self.entries.iter().filter(|e| e.cell == cell).any(|e| {
            let at = e
                .glyphs
                .partition_point(|(t, _)| t.as_slice() <= [c].as_slice());
            e.glyphs
                .get(at)
                .is_some_and(|(t, _)| t.len() > 1 && t.first() == Some(&c))
        })
    }

    /// Whether the store holds `text`'s glyph at `cell`.
    fn held(&self, cell: Cell, text: &[char]) -> bool {
        self.entry(cell, text).is_some()
    }

    /// The entry holding `text`'s glyph at `cell`.
    fn entry(&self, cell: Cell, text: &[char]) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.cell == cell && e.find(text).is_ok())
    }

    /// The bytes held: each set's as the helper sent it or as compaction
    /// wrote it again.
    pub fn bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.bytes).sum()
    }

    /// Frees at least `overrun` bytes for an ask at a full store: drops
    /// the glyphs least recently shown, across every set, until their
    /// share of their sets' bytes covers it, and writes each set they
    /// leave again without them ([`rewrite`]). When the rewritten sets
    /// free less than their share, the next least recently shown glyphs
    /// go too. The texts no longer held, by cell; nothing is redrawn, and
    /// each is asked for again when the screen shows it.
    pub fn make_room(&mut self, overrun: u64) -> Vec<(Cell, Vec<Text>)> {
        let mut out: Vec<(Cell, Vec<Text>)> = Vec::new();
        let mut short = overrun;
        while short > 0 && !self.entries.is_empty() {
            let held = self.bytes();
            let mut oldest: Vec<(u64, usize, &Text)> = self
                .entries
                .iter()
                .enumerate()
                .flat_map(|(i, e)| e.glyphs.iter().map(move |(t, used)| (*used, i, t)))
                .collect();
            oldest.sort_unstable();
            let mut gone: Vec<Vec<Text>> = vec![Vec::new(); self.entries.len()];
            let mut freed = 0u64;
            for (_, i, t) in oldest {
                if freed >= short {
                    break;
                }
                let (Some(list), Some(e)) = (gone.get_mut(i), self.entries.get(i)) else {
                    continue;
                };
                list.push(t.clone());
                freed = freed.saturating_add(e.share());
            }
            if freed == 0 {
                // no set maps a text, so none draws a glyph
                self.entries.clear();
                break;
            }
            for (i, mut list) in gone.into_iter().enumerate().rev() {
                let Some(e) = self.entries.get_mut(i).filter(|_| !list.is_empty()) else {
                    continue;
                };
                list.sort_unstable();
                let (lost, empty) = e.compact(&list);
                let cell = e.cell;
                if empty {
                    self.entries.remove(i);
                }
                match out.iter_mut().find(|(at, _)| *at == cell) {
                    Some((_, texts)) => texts.extend(lost),
                    None => out.push((cell, lost)),
                }
            }
            short = short.saturating_sub(held.saturating_sub(self.bytes()));
        }
        for (_, texts) in &mut out {
            texts.sort_unstable();
            texts.dedup();
        }
        out.retain(|(_, texts)| !texts.is_empty());
        out
    }

    /// The screen shows `shown` at `cell`: the glyphs of its characters
    /// and its clusters are the most recently shown.
    pub fn touch(&mut self, cell: Cell, shown: &Shown) {
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        let singles = shown.chars.iter().map(std::slice::from_ref);
        let texts: Vec<&[char]> = singles
            .chain(shown.clusters.iter().map(Vec::as_slice))
            .collect();
        for e in self.entries.iter_mut().filter(|e| e.cell == cell) {
            for &t in &texts {
                if let Ok(i) = e.find(t)
                    && let Some((_, used)) = e.glyphs.get_mut(i)
                {
                    *used = tick;
                }
            }
        }
    }

    /// Drops the glyphs of every cell but `cells`.
    pub fn retain(&mut self, cells: &[Cell]) {
        self.entries.retain(|e| cells.contains(&e.cell));
    }

    /// Whether any glyph at `cell` is held.
    pub fn holds(&self, cell: Cell) -> bool {
        self.entries.iter().any(|e| e.cell == cell)
    }

    /// `text`'s glyph at `cell`, from the store.
    fn draw(&self, cell: Cell, text: &[char], style: Style) -> Option<Glyph> {
        let face = &self.entry(cell, text)?.face;
        match text {
            [_] => face.draw(text, style),
            _ => face.cluster(text, style),
        }
    }
}

/// A display's face while the font is a frit: the generated glyph where
/// the helper sent one, else the built-in font's, in the frit's cell
/// (#145), a cluster found as the module says. Its cell never moves when
/// glyphs arrive. A built-in glyph is centred in a larger cell and cropped
/// to a smaller one, so both kinds come at the cell's size and the
/// renderer stores them alike.
pub struct DeferredFace {
    builtin: Box<dyn Face>,
    cell: Cell,
    step: u32,
    generated: Shared,
}

impl DeferredFace {
    /// `builtin` drawing in `cell`, which a zoom moves by `step`.
    pub fn new(builtin: Box<dyn Face>, cell: Cell, step: u32, generated: Shared) -> DeferredFace {
        DeferredFace {
            builtin,
            cell,
            step,
            generated,
        }
    }

    fn generated_has(&self, c: char) -> bool {
        self.generated.borrow().held(self.cell, &[c])
    }

    /// `c`'s glyph: the store's, else the built-in font's in the cell.
    fn single(&self, c: char, style: Style) -> Option<Glyph> {
        let generated = self.generated.borrow();
        match generated.draw(self.cell, &[c], style) {
            Some(g) => Some(g),
            None => self.fit(self.builtin.draw(&[c], style)?),
        }
    }

    /// A cluster the store does not hold drawn without its own glyph
    /// (ADR-1200 D22, steps 2 to 4): its NFC form's, else its base's with
    /// each combining mark's overstruck, centred across it; the default
    /// ignorables and the rest of the cluster left out. A color base takes
    /// no mark.
    fn unshaped(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        if let Some(c) = precomposed(cluster)
            && let Some(g) = self.generated.borrow().draw(self.cell, &[c], style)
        {
            return Some(g);
        }
        let (&base, rest) = cluster.split_first()?;
        let glyph = self.single(base, style)?;
        if glyph.format() != Format::Mask {
            return Some(glyph);
        }
        let marks: Vec<Glyph> = rest
            .iter()
            .filter(|&&c| is_mark(c) && !is_ignorable(c))
            .filter_map(|&c| self.single(c, style))
            .filter(|g| g.format() == Format::Mask)
            .collect();
        if marks.is_empty() {
            return Some(glyph);
        }
        let (w, h) = (glyph.width(), glyph.height());
        let mut pixels = glyph.pixels().to_vec();
        for mark in &marks {
            let dx = w.saturating_sub(mark.width()) / 2;
            for (y, row) in (0..h).zip(mark.rows()) {
                for (x, &p) in (dx..w).zip(row) {
                    let at = usize::try_from(y.checked_mul(w)?.checked_add(x)?).ok()?;
                    if let Some(px) = pixels.get_mut(at) {
                        *px = (*px).max(p);
                    }
                }
            }
        }
        Glyph::from_pixels(w, h, Format::Mask, pixels).map(|g| g.with_cells(glyph.cells()))
    }

    /// The built-in's `glyph` at the cell's size: centred, padded or
    /// cropped, the spare row or column after it.
    fn fit(&self, glyph: Glyph) -> Option<Glyph> {
        let cells = glyph.cells();
        let width = self.cell.width.checked_mul(cells.cells())?;
        let height = self.cell.height;
        if (glyph.width(), glyph.height()) == (width, height) {
            return Some(glyph);
        }
        let bpp = usize::try_from(glyph.format().bytes_per_pixel()).ok()?;
        let offset = |to: u32, from: u32| i64::from(to).saturating_sub(i64::from(from)) / 2;
        let (dx, dy) = (offset(width, glyph.width()), offset(height, glyph.height()));
        let stride = usize::try_from(width).ok()?.checked_mul(bpp)?;
        let mut pixels = vec![0u8; stride.checked_mul(usize::try_from(height).ok()?)?];
        for (y, row) in pixels.chunks_exact_mut(stride).enumerate() {
            let Some(sy) = i64::try_from(y).ok().map(|y| y.saturating_sub(dy)) else {
                continue;
            };
            let Some(src) = u32::try_from(sy)
                .ok()
                .and_then(|sy| glyph.rows().nth(usize::try_from(sy).ok()?))
            else {
                continue;
            };
            for (x, px) in row.chunks_exact_mut(bpp).enumerate() {
                let sx = i64::try_from(x).ok().map(|x| x.saturating_sub(dx));
                let from = sx.and_then(|sx| usize::try_from(sx).ok()).and_then(|sx| {
                    src.get(sx.checked_mul(bpp)?..sx.checked_mul(bpp)?.checked_add(bpp)?)
                });
                if let Some(from) = from {
                    px.copy_from_slice(from);
                }
            }
        }
        Glyph::from_pixels(width, height, glyph.format(), pixels).map(|g| g.with_cells(cells))
    }
}

impl Face for DeferredFace {
    fn cell(&self) -> Cell {
        self.cell
    }

    fn size_step(&self) -> u32 {
        self.step
    }

    /// Whether the store or the built-in font holds `c`, or the store a
    /// cluster `c` begins: the renderer asks by a cluster's first
    /// character before it draws the cluster.
    fn has_glyph(&self, c: char) -> bool {
        self.generated_has(c)
            || self.builtin.has_glyph(c)
            || self.generated.borrow().begins(self.cell, c)
    }

    /// The built-in font's, so the sprites draw the same ranges before the
    /// generated glyphs arrive and after.
    fn coverage(&self, range: Range) -> RangeCoverage {
        self.builtin.coverage(range)
    }

    fn sprite_policy(&self) -> SpritePolicy {
        self.builtin.sprite_policy()
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        match cluster {
            [] => None,
            &[c] => self.single(c, style),
            _ => {
                let held = self.generated.borrow().draw(self.cell, cluster, style);
                held.or_else(|| self.unshaped(cluster, style))
            }
        }
    }

    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.fit(self.builtin.last_resort(style)?)
    }

    /// The built-in font's in its own cell; in another, the lines for the
    /// cell's height.
    fn decorations(&self) -> Decorations {
        if self.builtin.cell() == self.cell {
            self.builtin.decorations()
        } else {
            Decorations::for_height(self.cell.height)
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use vtium_font::{Font, Unifont};

    const CELL: Cell = Cell {
        width: 8,
        height: 16,
    };

    /// A set at `cell` whose every glyph in `chars` is solid.
    pub(crate) fn solid(cell: Cell, chars: &[char]) -> Arc<Set> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default());
        let g = s.glyph(
            Width::Narrow,
            vec![255; (cell.width * cell.height) as usize],
        );
        for &c in chars {
            s.map(c, g);
        }
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    /// Adds `set` at `cell` to `store` as the helper does: what it adds.
    fn add(
        store: &mut Generated,
        cell: Cell,
        set: Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
    ) -> Option<SetFace> {
        let fresh = store.fresh(cell, set, &runs, bytes)?;
        store.add(fresh)
    }

    /// The screen showing `chars`, no cluster.
    fn shown(chars: &[char]) -> Shown {
        Shown {
            chars: chars.to_vec(),
            clusters: Vec::new(),
        }
    }

    /// Each of `chars` as a text of its own.
    fn texts(chars: &[char]) -> Vec<Text> {
        chars.iter().map(|&c| vec![c]).collect()
    }

    fn face(generated: &Shared) -> DeferredFace {
        DeferredFace::new(Box::new(Unifont::new(16)), CELL, 16, Rc::clone(generated))
    }

    fn covered(g: &Glyph) -> usize {
        (0..g.height())
            .flat_map(|y| (0..g.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| g.pixel(x, y).unwrap()[0] > 0)
            .count()
    }

    #[test]
    fn the_built_in_font_draws_until_the_glyphs_come() {
        let generated = Shared::default();
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!(a, unifont.draw(&['A'], Style::default()).unwrap());
        add(
            &mut generated.borrow_mut(),
            CELL,
            solid(CELL, &['A']),
            vec![(0x41, 1)],
            1,
        );
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!(covered(&a), 8 * 16);
    }

    // #141: a codepoint the frit lacks draws in the built-in font, one the
    // built-in lacks too in the replacement glyph
    #[test]
    fn a_character_the_frit_lacks_draws_in_the_built_in_font() {
        let generated = Shared::default();
        add(
            &mut generated.borrow_mut(),
            CELL,
            solid(CELL, &['A']),
            vec![(0x41, 1)],
            1,
        );
        let f = face(&generated);
        let unifont = Unifont::new(16);
        assert_eq!(
            f.draw(&['B'], Style::default()),
            unifont.draw(&['B'], Style::default())
        );
        assert!(f.has_glyph('A') && f.has_glyph('B'));
        // U+10FFFD is in neither: the font's replacement chain draws its
        // U+25A1, from the frit once the frit has one
        let font = Font::new(face(&generated));
        let lacking = font.glyph(&['\u{10fffd}'], Width::Narrow, Style::default());
        let unifont_font = Font::new(Unifont::new(16));
        assert_eq!(
            lacking,
            unifont_font.glyph(&['\u{10fffd}'], Width::Narrow, Style::default())
        );
        add(
            &mut generated.borrow_mut(),
            CELL,
            solid(CELL, &['\u{25a1}']),
            vec![(0x25a1, 1)],
            1,
        );
        let lacking = font
            .glyph(&['\u{10fffd}'], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!(covered(&lacking), 8 * 16);
    }

    /// The face drawing unifont at 16 in `cell`.
    fn face_in(generated: &Shared, cell: Cell) -> DeferredFace {
        DeferredFace::new(Box::new(Unifont::new(16)), cell, 16, Rc::clone(generated))
    }

    /// Whether `g` is `of` moved right by `dx` and down by `dy`, nothing
    /// set outside it.
    fn moved(g: &Glyph, of: &Glyph, dx: i64, dy: i64) -> bool {
        (0..g.height()).all(|y| {
            (0..g.width()).all(|x| {
                let from = u32::try_from(i64::from(x) - dx)
                    .ok()
                    .zip(u32::try_from(i64::from(y) - dy).ok())
                    .and_then(|(x, y)| of.pixel(x, y));
                g.pixel(x, y).unwrap()[0] == from.map_or(0, |p| p[0])
            })
        })
    }

    // #145: the built-in draws in the frit's cell until the glyphs land,
    // centred in a larger one
    #[test]
    fn the_built_in_font_is_centred_in_a_larger_cell() {
        let generated = Shared::default();
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let f = face_in(&generated, cell);
        let unifont = Unifont::new(16);
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!((a.width(), a.height()), (10, 20));
        assert!(moved(
            &a,
            &unifont.draw(&['A'], Style::default()).unwrap(),
            1,
            2
        ));
        let wide = f.draw(&['\u{4e2d}'], Style::default()).unwrap();
        assert_eq!(
            (wide.width(), wide.height(), wide.cells()),
            (20, 20, Width::Wide)
        );
        assert_eq!(f.decorations(), vtium_font::Decorations::for_height(20));
    }

    #[test]
    fn the_built_in_font_is_cropped_to_a_smaller_cell() {
        let generated = Shared::default();
        let cell = Cell {
            width: 8,
            height: 12,
        };
        let a = face_in(&generated, cell)
            .draw(&['A'], Style::default())
            .unwrap();
        assert_eq!((a.width(), a.height()), (8, 12));
        let unifont = Unifont::new(16).draw(&['A'], Style::default()).unwrap();
        assert!(moved(&a, &unifont, 0, -2));
    }

    // #145: the glyphs arrive at the frit's cell, and the cell stays
    #[test]
    fn the_cell_never_moves_when_the_glyphs_arrive() {
        let generated = Shared::default();
        let cell = Cell {
            width: 10,
            height: 20,
        };
        let f = face_in(&generated, cell);
        assert_eq!((f.cell(), f.size_step()), (cell, 16));
        // glyphs at the built-in's own cell are not the frit's
        add(
            &mut generated.borrow_mut(),
            CELL,
            solid(CELL, &['A']),
            vec![(0x41, 1)],
            1,
        );
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!((a.width(), a.height()), (10, 20));
        assert!(covered(&a) < 10 * 20);
        add(
            &mut generated.borrow_mut(),
            cell,
            solid(cell, &['A']),
            vec![(0x41, 1)],
            1,
        );
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!(covered(&a), 10 * 20);
        assert_eq!(f.cell(), cell);
    }

    #[test]
    fn the_store_limit_follows_the_machines_memory() {
        assert_eq!(store_limit(512 << 20), 16 << 20);
        assert_eq!(store_limit(2 << 30), 64 << 20);
        assert_eq!(store_limit(32 << 30), 128 << 20);
    }

    #[test]
    fn the_glyph_least_recently_on_the_screen_leaves_first() {
        let mut g = Generated::default();
        add(&mut g, CELL, solid(CELL, &['A']), vec![(0x41, 1)], 10);
        add(&mut g, CELL, solid(CELL, &['B']), vec![(0x42, 1)], 20);
        assert_eq!(g.bytes(), 30);
        // A is on the screen, B is not
        g.touch(CELL, &shown(&['A']));
        assert_eq!(g.make_room(1), vec![(CELL, texts(&['B']))]);
        assert_eq!(g.bytes(), 10);
        assert_eq!(g.make_room(1), vec![(CELL, texts(&['A']))]);
        assert_eq!(g.make_room(1), vec![]);
        assert_eq!(g.bytes(), 0);
    }

    /// `seed`'s R8 glyph `cells` wide at [`CELL`], its coverage between 1
    /// and 254, so no pixel of it is 1-bit.
    fn shade(cells: Width, seed: u32) -> Vec<u8> {
        let n = CELL.width * CELL.height * if cells == Width::Wide { 2 } else { 1 };
        (0..n)
            .map(|i| u8::try_from((i * 7 + seed * 13) % 254 + 1).unwrap())
            .collect()
    }

    /// `seed`'s 1-bit glyph at [`CELL`]: a byte a row.
    fn bits(seed: u32) -> Vec<u8> {
        (0..CELL.height)
            .map(|i| u8::try_from((i * 17 + seed) % 256).unwrap())
            .collect()
    }

    type Glyphs<'a> = &'a [(char, Width, Vec<u8>)];

    /// A set at [`CELL`] as the helper writes one: `one_bit` in a 1-bit
    /// strike, `r8` in an R8 one, chunks raw, the baseline at the cell's
    /// bottom.
    fn answer(one_bit: Glyphs, r8: Glyphs) -> Vec<u8> {
        let mut set = SetBuilder::new("t");
        for (format, glyphs) in [(PixelFormat::Bits, one_bit), (PixelFormat::R8, r8)] {
            let mut s = StrikeBuilder::new(format, CELL, Style::default())
                .baseline(CELL.height)
                .compress(false);
            for (c, width, bytes) in glyphs {
                let g = s.glyph(*width, bytes.clone());
                s.map(*c, g);
            }
            if !s.is_empty() {
                set = set.strike(s);
            }
        }
        set.write().unwrap()
    }

    /// Adds `bytes`, a set at [`CELL`], to `store`: its size.
    fn put(store: &mut Generated, bytes: Vec<u8>) -> u64 {
        let set = Arc::new(Set::from_bytes(bytes.clone()).unwrap());
        let runs = vtium_font::set::SetFace::new(Arc::clone(&set), CELL.height)
            .map(|f| {
                (0..=0x10ffff_u32)
                    .filter(|&c| char::from_u32(c).is_some_and(|c| f.has_glyph(c)))
                    .map(|c| (c, 1))
                    .collect()
            })
            .unwrap();
        add(store, CELL, set, runs, bytes.len() as u64).unwrap();
        bytes.len() as u64
    }

    // #145, ADR-1200 D19: an ask at a full store compacts it, the glyphs
    // least recently shown leaving first, and the store counts the
    // rebuilt sets' real bytes
    #[test]
    fn a_full_store_compacts_the_least_recently_shown_glyphs() {
        let wide = '\u{4e2d}';
        let r8 = |c, cells, seed| (c, cells, shade(cells, seed));
        let (a, b, c) = (
            r8('A', Width::Narrow, 1),
            r8('B', Width::Narrow, 2),
            r8('C', Width::Narrow, 3),
        );
        let w = r8(wide, Width::Wide, 4);
        let (d, e) = (('D', Width::Narrow, bits(5)), ('E', Width::Narrow, bits(6)));
        let generated = Shared::default();
        let before = {
            let mut g = generated.borrow_mut();
            put(&mut g, answer(&[], &[a.clone(), b, c.clone(), w.clone()]))
                + put(&mut g, answer(&[d.clone(), e], &[]))
        };
        assert_eq!(generated.borrow().bytes(), before);
        // shown: D, then A, then C and the wide one; B and E never
        generated.borrow_mut().touch(CELL, &shown(&['D']));
        generated.borrow_mut().touch(CELL, &shown(&['A']));
        generated.borrow_mut().touch(CELL, &shown(&['C', wide]));
        let f = face(&generated);
        let kept = ['A', 'C', wide, 'D'];
        let drawn = kept.map(|k| f.draw(&[k], Style::default()).unwrap());
        // the store after: the same answers written without B and E
        let after = answer(&[], &[a, c, w]).len() as u64 + answer(&[d], &[]).len() as u64;
        assert!(after < before);
        let gone = generated.borrow_mut().make_room(before - after);
        assert_eq!(gone, vec![(CELL, texts(&['B', 'E']))]);
        assert_eq!(generated.borrow().bytes(), after);
        for (k, glyph) in kept.into_iter().zip(drawn) {
            assert_eq!(f.draw(&[k], Style::default()), Some(glyph), "{k}");
        }
        // B and E draw in the built-in font until they are asked for again
        for k in ['B', 'E'] {
            assert!(!f.generated_has(k));
            assert_eq!(
                f.draw(&[k], Style::default()),
                Unifont::new(16).draw(&[k], Style::default())
            );
        }
    }

    // ADR-1200 D13, D19: compaction keeps each glyph in its own format
    #[test]
    fn a_1_bit_glyph_stays_1_bit_in_the_rebuilt_set() {
        let mut g = Generated::default();
        let a = ('A', Width::Narrow, shade(Width::Narrow, 1));
        let d = ('D', Width::Narrow, bits(5));
        let e = ('E', Width::Narrow, bits(6));
        put(&mut g, answer(&[d.clone(), e], std::slice::from_ref(&a)));
        g.touch(CELL, &shown(&['A', 'D']));
        assert_eq!(g.make_room(1), vec![(CELL, texts(&['E']))]);
        let strikes: Vec<_> = g.entries[0]
            .face
            .set()
            .strikes()
            .map(|s| (s.format, s.glyph_count))
            .collect();
        assert_eq!(strikes.len(), 2);
        assert!(strikes.contains(&(PixelFormat::Bits, 1)));
        assert!(strikes.contains(&(PixelFormat::R8, 1)));
        assert_eq!(g.bytes(), answer(&[d], &[a]).len() as u64);
    }

    // ADR-1200 D18, D19: a frame that carries glyphs the store holds is
    // written again without them, and the store counts the set it wrote
    #[test]
    fn a_frame_of_held_and_new_glyphs_adds_only_the_new() {
        let r8 = |c, seed| (c, Width::Narrow, shade(Width::Narrow, seed));
        let (a, b, c) = (r8('A', 1), r8('B', 2), r8('C', 3));
        let generated = Shared::default();
        let first = put(&mut generated.borrow_mut(), answer(&[], &[a, b.clone()]));
        let f = face(&generated);
        let drawn = ['A', 'B'].map(|k| f.draw(&[k], Style::default()).unwrap());
        // the frame's B differs from the held one, so a B drawn from it shows
        let frame = answer(&[], &[r8('B', 9), c.clone()]);
        let sent = Arc::new(Set::from_bytes(frame.clone()).unwrap());
        let added = add(
            &mut generated.borrow_mut(),
            CELL,
            Arc::clone(&sent),
            vec![(0x42, 2)],
            frame.len() as u64,
        )
        .unwrap();
        assert!(added.has_glyph('C') && !added.has_glyph('B'));
        let rebuilt = answer(&[], &[c]).len() as u64;
        assert_eq!(generated.borrow().bytes(), first + rebuilt);
        for (k, glyph) in ['A', 'B'].into_iter().zip(drawn) {
            assert_eq!(f.draw(&[k], Style::default()), Some(glyph), "{k}");
        }
        let from_frame = SetFace::new(sent, CELL.height).unwrap();
        assert_eq!(
            f.draw(&['C'], Style::default()),
            from_frame.draw(&['C'], Style::default())
        );
        // a frame of held glyphs alone adds nothing
        let held = answer(&[], &[b, r8('C', 3)]);
        let bytes = held.len() as u64;
        let held = Arc::new(Set::from_bytes(held).unwrap());
        assert!(
            add(
                &mut generated.borrow_mut(),
                CELL,
                held,
                vec![(0x42, 2)],
                bytes
            )
            .is_none()
        );
        assert_eq!(generated.borrow().bytes(), first + rebuilt);
    }

    /// A set at [`CELL`] mapping each of `chars` to a solid glyph and each
    /// of `clusters` to a glyph of its own shade.
    fn with_clusters(chars: &[char], clusters: &[(&[char], u8)]) -> Arc<Set> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let n = (CELL.width * CELL.height) as usize;
        let solid = s.glyph(Width::Narrow, vec![255; n]);
        for &c in chars {
            s.map(c, solid);
        }
        for &(key, shade) in clusters {
            let g = s.glyph(Width::Narrow, vec![shade; n]);
            s.cluster(key, g);
        }
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    /// Whether every pixel of `g` is `shade`.
    fn all(g: &Glyph, shade: u8) -> bool {
        g.pixels().iter().all(|&p| p == shade)
    }

    const ACUTE: [char; 2] = ['e', '\u{301}'];

    // ADR-1200 D20, D22 step 1: a cluster's glyph is keyed by its whole
    // text, apart from its first codepoint's
    #[test]
    fn a_cluster_draws_its_own_generated_glyph() {
        let generated = Shared::default();
        add(
            &mut generated.borrow_mut(),
            CELL,
            with_clusters(&['e'], &[(&ACUTE, 7)]),
            vec![(0x65, 1)],
            1,
        );
        let f = face(&generated);
        assert!(all(&f.draw(&ACUTE, Style::default()).unwrap(), 7));
        assert!(all(&f.draw(&['e'], Style::default()).unwrap(), 255));
    }

    #[test]
    fn a_clusters_nfc_form_is_found_in_canonical_order() {
        assert_eq!(precomposed(&ACUTE), Some('\u{e9}'));
        // a with dot below and circumflex, in either order, is U+1EAD
        assert_eq!(precomposed(&['a', '\u{323}', '\u{302}']), Some('\u{1ead}'));
        assert_eq!(precomposed(&['a', '\u{302}', '\u{323}']), Some('\u{1ead}'));
        // two marks of one class: the second is blocked, and stays
        assert_eq!(precomposed(&['a', '\u{301}', '\u{300}']), None);
        // Hangul by rule: L V is LV, LV T is LVT
        assert_eq!(precomposed(&['\u{1100}', '\u{1161}']), Some('\u{ac00}'));
        assert_eq!(
            precomposed(&['\u{1100}', '\u{1161}', '\u{11a8}']),
            Some('\u{ac01}')
        );
        // a composition exclusion: U+0958 is not composed again
        assert_eq!(precomposed(&['\u{915}', '\u{93c}']), None);
        assert_eq!(precomposed(&['e']), None);
        assert_eq!(precomposed(&['q', '\u{301}']), None);
    }

    // ADR-1200 D22 step 2: a cluster the store does not hold draws its NFC
    // form's glyph where the store holds that
    #[test]
    fn a_cluster_draws_its_precomposed_form_from_the_store() {
        let generated = Shared::default();
        add(
            &mut generated.borrow_mut(),
            CELL,
            with_clusters(&['\u{e9}'], &[]),
            vec![(0xe9, 1)],
            1,
        );
        let f = face(&generated);
        assert!(all(&f.draw(&ACUTE, Style::default()).unwrap(), 255));
    }

    // ADR-1200 D22 step 3, ADR-1301 D1: with neither its own glyph nor
    // its NFC form held, a cluster draws unshaped: its base with each
    // combining mark overstruck, each from the store or the built-in
    // font; a default ignorable and the rest of an emoji sequence are not
    // drawn
    #[test]
    fn a_base_draws_with_its_marks_overstruck() {
        let generated = Shared::default();
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let regular = Style::default();
        let q = unifont.draw(&['q'], regular).unwrap();
        let acute = unifont.draw(&['\u{301}'], regular).unwrap();
        let both = f.draw(&['q', '\u{301}'], regular).unwrap();
        let over: Vec<u8> = q
            .pixels()
            .iter()
            .zip(acute.pixels())
            .map(|(&a, &b)| a.max(b))
            .collect();
        assert_eq!(both.pixels(), over.as_slice());
        assert_ne!(both.pixels(), q.pixels());
        // a variation selector is a default ignorable, not a mark
        assert_eq!(f.draw(&['q', '\u{fe0f}'], regular), Some(q.clone()));
        let family = ['\u{1f468}', '\u{200d}', '\u{1f469}'];
        assert_eq!(f.draw(&family, regular), f.draw(&family[..1], regular));
        // the base from the store, the mark from the built-in font
        add(
            &mut generated.borrow_mut(),
            CELL,
            with_clusters(&['q'], &[]),
            vec![(0x71, 1)],
            1,
        );
        assert!(all(&f.draw(&['q', '\u{301}'], regular).unwrap(), 255));
        let held = f.draw(&['q', '\u{301}'], regular).unwrap();
        assert_eq!(held.cells(), Width::Narrow);
    }

    // a cluster whose first character neither the store nor the built-in
    // font holds still draws its own glyph through the font, which asks by
    // that character first
    #[test]
    fn a_cluster_draws_though_no_font_holds_its_first_character() {
        let generated = Shared::default();
        let lead = '\u{10fffd}';
        let cluster = [lead, '\u{301}'];
        let f = face(&generated);
        assert!(!f.has_glyph(lead));
        add(
            &mut generated.borrow_mut(),
            CELL,
            with_clusters(&[], &[(&cluster, 7)]),
            vec![],
            1,
        );
        assert!(f.has_glyph(lead));
        assert!(!f.has_glyph('\u{10fffc}'));
        let font = Font::new(face(&generated));
        let drawn = font
            .glyph(&cluster, Width::Narrow, Style::default())
            .unwrap();
        assert!(all(&drawn, 7));
    }

    // ADR-1200 D19: compaction keys a cluster as the store does, drops the
    // one least recently shown and writes the one shown again
    #[test]
    fn compaction_keeps_the_clusters_shown_and_drops_the_rest() {
        let generated = Shared::default();
        let grave = ['e', '\u{300}'];
        add(
            &mut generated.borrow_mut(),
            CELL,
            with_clusters(&['e'], &[(&ACUTE, 7), (&grave, 9)]),
            vec![(0x65, 1)],
            // more than the set written again takes, so one pass frees it
            1 << 20,
        );
        let shown = Shown {
            chars: vec!['e'],
            clusters: vec![ACUTE.to_vec()],
        };
        generated.borrow_mut().touch(CELL, &shown);
        assert_eq!(
            generated.borrow_mut().make_room(1),
            vec![(CELL, vec![grave.to_vec()])]
        );
        let f = face(&generated);
        assert!(all(&f.draw(&ACUTE, Style::default()).unwrap(), 7));
        assert!(!generated.borrow().held(CELL, &grave));
        // a frame of the held cluster and a new one adds the new one alone
        let fresh = generated
            .borrow()
            .fresh(
                CELL,
                with_clusters(&[], &[(&ACUTE, 3), (&grave, 9)]),
                &[],
                100,
            )
            .unwrap();
        assert_eq!(fresh.texts().cloned().collect::<Vec<_>>(), [grave.to_vec()]);
    }

    #[test]
    fn glyphs_at_another_cell_are_not_drawn_and_retain_drops_them() {
        let generated = Shared::default();
        let other = Cell {
            width: 16,
            height: 32,
        };
        add(
            &mut generated.borrow_mut(),
            other,
            solid(other, &['A']),
            vec![(0x41, 1)],
            1,
        );
        let f = face(&generated);
        assert_eq!(
            f.draw(&['A'], Style::default()),
            Unifont::new(16).draw(&['A'], Style::default())
        );
        assert!(generated.borrow().holds(other));
        generated.borrow_mut().retain(&[CELL]);
        assert!(!generated.borrow().holds(other));
    }
}
