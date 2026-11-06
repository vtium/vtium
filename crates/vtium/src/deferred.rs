//! The deferred pipeline's glyphs (ADR-1301 D2): the per-boot memory
//! store of what the helper generated, and the face that draws a cell from
//! it, falling back per character to the built-in font (#141).
//!
//! The store keys a glyph by its cluster's text and its style, bold,
//! italic, both or neither, at a cell (ADR-1200 D20), the text one
//! codepoint or a cluster of more, so two cells holding the same cluster
//! in one style share its glyph however their clusters were made. A
//! style's glyphs come by ask in sets of their own (ADR-1200 D21), and
//! until a style's glyph arrives the built-in font draws it in its own
//! synthetic style: a bitmap glyph widened for bold and sheared for
//! italic (ADR-1302 D4). The lookup for a cell is ADR-1200 D22's: the cluster's glyph in
//! the store; the cluster's NFC form, where it precomposes to one
//! codepoint the store holds; the base's glyph, the store's or the
//! built-in font's, with each nonspacing mark's glyph overstruck on it and
//! each spacing mark's beside it (ADR-1302 D14), the default ignorables
//! and the rest of an emoji sequence left out; the
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

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::font::DisplayFace;

use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder, StrikeInfo};
use vtium_font::spacing::{Band, Fit, SpacedCell};
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

/// What a glyph is keyed by at a cell and in a style (ADR-1200 D20): its
/// cluster's text, one codepoint or more.
pub type Text = Vec<char>;

/// What the displays show in one style: each character, and each cluster
/// of more than one, each sorted and once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub style: Style,
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

/// `c`'s full canonical decomposition, `c` itself when it has none: a
/// Hangul syllable by rule, the CJK compatibility ideographs' left out.
fn decompose(c: char, out: &mut Vec<char>) {
    let u = u32::from(c);
    let s = u.wrapping_sub(HANGUL_S);
    if s < HANGUL_S_COUNT {
        let lv = s / HANGUL_T_COUNT;
        let parts = [
            HANGUL_L.checked_add(lv / HANGUL_V_COUNT),
            HANGUL_V.checked_add(lv % HANGUL_V_COUNT),
            (s % HANGUL_T_COUNT != 0)
                .then(|| HANGUL_T.checked_add(s % HANGUL_T_COUNT))
                .flatten(),
        ];
        out.extend(parts.into_iter().flatten().filter_map(char::from_u32));
        return;
    }
    match data::DECOMPOSE.binary_search_by_key(&u, |&(cp, _, _)| cp) {
        Ok(i) => {
            for part in data::DECOMPOSE
                .get(i)
                .map(|&(_, a, b)| [a, b])
                .into_iter()
                .flatten()
                .filter(|&p| p != 0)
                .filter_map(char::from_u32)
            {
                decompose(part, out);
            }
        }
        Err(_) => out.push(c),
    }
}

/// The characters `c` decomposes to canonically, `c` among them: a cell
/// whose cluster holds any of them may draw from `c`'s glyph (step 2).
pub fn decomposition(c: char) -> Vec<char> {
    let mut out = vec![c];
    decompose(c, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}

/// The one codepoint `cluster`'s NFC form is, when it is one: the cluster
/// decomposed canonically, each run of marks of a class but 0 put in
/// canonical order, then each composed into the base in turn, as none is
/// left between to block the next.
fn precomposed(cluster: &[char]) -> Option<char> {
    if cluster.len() < 2 {
        return None;
    }
    let mut chars = Vec::new();
    for &c in cluster {
        decompose(c, &mut chars);
    }
    let (&base, rest) = chars.split_first()?;
    let mut marks = rest.to_vec();
    for run in marks.split_mut(|&c| class(c) == 0) {
        run.sort_by_key(|&c| class(c));
    }
    marks.into_iter().try_fold(base, compose)
}

/// Whether `c` is a spacing mark (General_Category Mc), and if it is,
/// whether its body stands before its letter.
fn spacing_side(c: char) -> Option<bool> {
    let c = u32::from(c);
    let i = data::SPACING.partition_point(|&(_, last, _)| last < c);
    match data::SPACING.get(i) {
        Some(&(first, _, before)) if first <= c => Some(before),
        _ => None,
    }
}

/// `base` with each of `marks` overstruck on it, the base and each mark
/// centred across the widest of them, so a mark two cells wide, as U+20E3
/// is in unifont, makes the glyph two cells wide.
fn overstrike(base: &Glyph, marks: &[Glyph]) -> Option<Glyph> {
    if marks.is_empty() {
        return Some(base.clone());
    }
    let parts: Vec<&Glyph> = std::iter::once(base).chain(marks).collect();
    let w = parts.iter().map(|g| g.width()).max()?;
    let h = base.height();
    let wide = parts.iter().any(|g| g.cells() == Width::Wide);
    let mut pixels = vec![0u8; usize::try_from(w.checked_mul(h)?).ok()?];
    for part in parts {
        let dx = w.saturating_sub(part.width()) / 2;
        for (y, row) in (0..h).zip(part.rows()) {
            for (x, &p) in (dx..w).zip(row) {
                let at = usize::try_from(y.checked_mul(w)?.checked_add(x)?).ok()?;
                if let Some(px) = pixels.get_mut(at) {
                    *px = (*px).max(p);
                }
            }
        }
    }
    let cells = if wide { Width::Wide } else { base.cells() };
    Glyph::from_pixels(w, h, Format::Mask, pixels).map(|g| g.with_cells(cells))
}

/// Whether the font already draws `mark` where it belongs against `letter`,
/// the side it stands on given by `before`. The mark's box must be as wide
/// as the letter's, and then either its ink wraps the letter's ink, as GNU
/// Unifont draws Myanmar's medial ra round its consonant, lying both left
/// and right of it, or over its whole height, past it above or below, and
/// past it on the mark's own side; or the outer edge of its ink lies
/// in the quarter of the box on its own side: a mark before its letter
/// starts in the left quarter, and a mark after it ends in the right
/// quarter. Unifont draws its Devanagari, Bengali, Tamil and Malayalam
/// vowel signs that way, each in a box as wide as its letter's with its
/// ink reaching the box's edge on its side, so that laid on the letter it
/// lands beside it, whatever the letter's shape. A mark drawn in the middle
/// of its box, or on the other side, is not placed.
fn drawn_beside(letter: &Glyph, mark: &Glyph, before: bool) -> bool {
    let w = mark.width();
    if w != letter.width() {
        return false;
    }
    let Some((first, last)) = ink_columns(mark) else {
        return false;
    };
    if let (Some((top, bottom)), Some((from, to)), Some((high, low))) =
        (ink_rows(mark), ink_columns(letter), ink_rows(letter))
    {
        // left and right of the letter's ink
        let across = first < from && last > to;
        // over its whole height, past it at the top or the bottom, and past
        // it on the mark's own side
        let round = top <= high
            && bottom >= low
            && (top < high || bottom > low)
            && if before { first < from } else { last > to };
        if across || round {
            return true;
        }
    }
    let quarter = i64::from(w / 4);
    if before {
        first < quarter
    } else {
        last >= i64::from(w).saturating_sub(quarter)
    }
}

/// The first and last rows of `g` that hold ink.
fn ink_rows(g: &Glyph) -> Option<(i64, i64)> {
    let inked: Vec<bool> = g.rows().map(|r| r.iter().any(|&p| p > 0)).collect();
    let first = inked.iter().position(|&i| i)?;
    let last = inked.iter().rposition(|&i| i)?;
    Some((i64::try_from(first).ok()?, i64::try_from(last).ok()?))
}

/// The first and last columns of `g` that hold ink.
fn ink_columns(g: &Glyph) -> Option<(i64, i64)> {
    let inked = |x: u32| g.rows().any(|r| r.get(x as usize).is_some_and(|&p| p > 0));
    let first = (0..g.width()).find(|&x| inked(x))?;
    let last = (0..g.width()).rev().find(|&x| inked(x))?;
    Some((i64::from(first), i64::from(last)))
}

/// `letter` with each of `marks` standing beside it (ADR-1302 D14): a
/// mark after the letter has its first column of ink in the column after
/// the last ink so far, and a mark before it, which the flag says, its
/// last in the column before the first. The run of ink is centred in the
/// letter's cells, or, where it is wider than they are, scaled sideways
/// as one to fill them, so no ink is cut.
fn beside(letter: Glyph, marks: &[(Glyph, bool)]) -> Option<Glyph> {
    if marks.is_empty() {
        return Some(letter);
    }
    let w = i64::from(letter.width());
    let (mut lo, mut hi) = ink_columns(&letter).unwrap_or((0, w.checked_sub(1)?));
    let mut placed: Vec<(&Glyph, i64)> = vec![(&letter, 0)];
    for (mark, before) in marks {
        let Some((first, last)) = ink_columns(mark) else {
            continue;
        };
        let x = if *before {
            let x = lo.checked_sub(1)?.checked_sub(last)?;
            lo = x.checked_add(first)?;
            x
        } else {
            let x = hi.checked_add(1)?.checked_sub(first)?;
            hi = x.checked_add(last)?;
            x
        };
        placed.push((mark, x));
    }
    let span = hi.checked_sub(lo)?.checked_add(1)?;
    let (canvas, shift) = if span <= w {
        (w, w.checked_sub(span)?.checked_div(2)?.checked_sub(lo)?)
    } else {
        (span, lo.checked_neg()?)
    };
    let h = letter.height();
    let stride = usize::try_from(canvas).ok()?;
    let mut pixels = vec![0u8; stride.checked_mul(usize::try_from(h).ok()?)?];
    for (part, x) in placed {
        let x = x.checked_add(shift)?;
        for (row, line) in part.rows().zip(pixels.chunks_exact_mut(stride)) {
            for (from, &p) in (x..).zip(row) {
                if let Some(px) = usize::try_from(from).ok().and_then(|i| line.get_mut(i)) {
                    *px = (*px).max(p);
                }
            }
        }
    }
    if canvas > w {
        pixels = scaled(&pixels, stride, usize::try_from(w).ok()?)?;
    }
    Glyph::from_pixels(letter.width(), h, Format::Mask, pixels)
        .map(|g| g.with_cells(letter.cells()))
}

/// Mask `rows`, `from` pixels wide, scaled sideways to `to` pixels wide,
/// `to` no wider: each pixel the coverage of the span of the row it takes.
fn scaled(rows: &[u8], from: usize, to: usize) -> Option<Vec<u8>> {
    let (f, t) = (u64::try_from(from).ok()?, u64::try_from(to).ok()?);
    let mut out = Vec::with_capacity(rows.len().checked_div(from)?.checked_mul(to)?);
    for row in rows.chunks_exact(from) {
        for j in 0..t {
            // pixel j spans [j * from, (j + 1) * from) of the row's
            // from * to units, and source pixel i spans [i * to, (i + 1) * to)
            let (start, end) = (j.checked_mul(f)?, j.checked_add(1)?.checked_mul(f)?);
            let mut sum = 0u64;
            for (i, &p) in (0u64..).zip(row) {
                let (a, b) = (i.checked_mul(t)?, i.checked_add(1)?.checked_mul(t)?);
                let overlap = end.min(b).saturating_sub(start.max(a));
                sum = sum.checked_add(u64::from(p).checked_mul(overlap)?)?;
            }
            out.push(u8::try_from(sum.checked_div(f)?).ok()?);
        }
    }
    Some(out)
}

/// One set the helper sent, or its rebuild: its cell and the rows line
/// spacing adds to it, the style of its strikes, the face it draws
/// through, each text it maps with when the screen last showed it, and its
/// bytes.
#[derive(Debug)]
struct Entry {
    cell: Cell,
    band: Band,
    style: Style,
    face: SetFace,
    /// Sorted by text.
    glyphs: Vec<(Text, u64)>,
    bytes: u64,
}

/// The glyph `face` draws for `text` in `style`: a codepoint's from the
/// set's map, a cluster's from its cluster map, at the face's scale or at
/// 1.
fn draw_text(face: &SetFace, text: &[char], style: Style, unscaled: bool) -> Option<Glyph> {
    match text {
        [c] if unscaled => face.draw_unscaled(&[*c], style),
        [c] => face.draw(&[*c], style),
        _ => face.cluster(text, style),
    }
}

impl Entry {
    /// The cell and band it was generated for, which it is held by.
    fn at(&self) -> SpacedCell {
        SpacedCell {
            cell: self.cell,
            band: self.band,
        }
    }

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
            && let Some((face, bytes)) = rewrite(&self.face, self.cell, self.style, &self.glyphs)
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
/// with only `kept`'s glyphs at `cell` in `style`, as the helper writes an
/// answer:
/// a strike a pixel format, its chunks raw, a codepoint in its map and a
/// cluster in its cluster map. Each glyph keeps its format: RGBA stays
/// RGBA, and a mask whose every pixel is 0 or 255 in a set with a 1-bit
/// strike stays 1-bit, every other mask R8. The new face and its bytes;
/// `None` when a glyph cannot be drawn or the set cannot be written.
fn rewrite(
    face: &SetFace,
    cell: Cell,
    style: Style,
    kept: &[(Text, u64)],
) -> Option<(SetFace, u64)> {
    let set = face.set();
    let strikes: Vec<StrikeInfo> = set
        .strikes()
        .filter(|s| s.cell == cell && s.style == style)
        .collect();
    let one_bit = strikes.iter().any(|s| s.format == PixelFormat::Bits);
    let formats = [PixelFormat::Bits, PixelFormat::R8, PixelFormat::Rgba];
    let mut builders = formats.map(|format| {
        let mut s = StrikeBuilder::new(format, cell, style).compress(false);
        if let Some(info) = strikes.iter().find(|s| s.format == format) {
            s = s.baseline(info.baseline).decorations(info.decorations);
        }
        (s, BTreeMap::<(bool, Vec<u8>), u32>::new())
    });
    for (text, _) in kept {
        let glyph = draw_text(face, text, style, true)?;
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

/// The store, shared between the console and its displays' faces, which
/// draw on their heads' render threads: each lock is held for one lookup
/// or one change, never across a frame.
pub type Shared = Arc<Mutex<Generated>>;

/// The store behind `shared`, locked. A panic while it was held ends the
/// process (ADR-1401 D10), so a poisoned lock is never met outside a test.
pub fn lock(shared: &Shared) -> MutexGuard<'_, Generated> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

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
    /// A checked set of glyphs at `at`, a cell and the rows line spacing
    /// adds to it (ADR-1302 D13), in `style`, mapping the codepoints of
    /// `runs` and the clusters its cluster map holds at the cell in that
    /// style, and taking `bytes`, without the glyphs the store holds at
    /// `at` in `style` (ADR-1200 D18): a set that carries some is written again without them, as
    /// compaction writes one ([`rewrite`]), and counts the bytes written.
    /// It adds nothing when it carries nothing new, or when it cannot be
    /// written again. `None` when `set` has no face at the cell.
    pub fn fresh(
        &self,
        (at, style): (SpacedCell, Style),
        set: Arc<Set>,
        runs: &[(u32, u32)],
        bytes: u64,
    ) -> Option<Fresh> {
        let clusters: Vec<Text> = set
            .clusters()
            .filter(|&(of_cell, of, _)| of_cell == at.cell && of == style)
            .map(|(_, _, key)| key)
            .collect();
        let face = SetFace::new(set, at.cell.height)?;
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
            .filter(|t| self.held(at, style, t))
            .cloned()
            .collect();
        let mut entry = Entry {
            cell: at.cell,
            band: at.band,
            style,
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

    /// Whether the store holds at `at`, in any style, the glyph of a
    /// cluster `c` begins.
    fn begins(&self, at: SpacedCell, c: char) -> bool {
        self.entries.iter().filter(|e| e.at() == at).any(|e| {
            let i = e
                .glyphs
                .partition_point(|(t, _)| t.as_slice() <= [c].as_slice());
            e.glyphs
                .get(i)
                .is_some_and(|(t, _)| t.len() > 1 && t.first() == Some(&c))
        })
    }

    /// Whether the store holds `text`'s glyph at `at` in `style`.
    fn held(&self, at: SpacedCell, style: Style, text: &[char]) -> bool {
        self.entry(at, style, text).is_some()
    }

    /// Whether the store holds `text`'s glyph at `at` in any style.
    fn held_any(&self, at: SpacedCell, text: &[char]) -> bool {
        self.entries
            .iter()
            .any(|e| e.at() == at && e.find(text).is_ok())
    }

    /// The entry holding `text`'s glyph at `at` in `style`.
    fn entry(&self, at: SpacedCell, style: Style, text: &[char]) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.at() == at && e.style == style && e.find(text).is_ok())
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
    /// go too. The texts no longer held, by cell, band and style; nothing
    /// is redrawn, and each is asked for again when the screen shows it.
    pub fn make_room(&mut self, overrun: u64) -> Vec<(SpacedCell, Style, Vec<Text>)> {
        let mut out: Vec<(SpacedCell, Style, Vec<Text>)> = Vec::new();
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
                let (at, style) = (e.at(), e.style);
                if empty {
                    self.entries.remove(i);
                }
                match out
                    .iter_mut()
                    .find(|(of_at, of, _)| (*of_at, *of) == (at, style))
                {
                    Some((_, _, texts)) => texts.extend(lost),
                    None => out.push((at, style, lost)),
                }
            }
            short = short.saturating_sub(held.saturating_sub(self.bytes()));
        }
        for (_, _, texts) in &mut out {
            texts.sort_unstable();
            texts.dedup();
        }
        out.retain(|(_, _, texts)| !texts.is_empty());
        out
    }

    /// The screen shows `shown` at `at`: the glyphs of its characters
    /// and its clusters in its style are the most recently shown.
    pub fn touch(&mut self, at: SpacedCell, shown: &Shown) {
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        let singles = shown.chars.iter().map(std::slice::from_ref);
        let texts: Vec<&[char]> = singles
            .chain(shown.clusters.iter().map(Vec::as_slice))
            .collect();
        for e in self
            .entries
            .iter_mut()
            .filter(|e| e.at() == at && e.style == shown.style)
        {
            for &t in &texts {
                if let Ok(i) = e.find(t)
                    && let Some((_, used)) = e.glyphs.get_mut(i)
                {
                    *used = tick;
                }
            }
        }
    }

    /// Drops the glyphs of every cell and band but `cells`.
    pub fn retain(&mut self, cells: &[SpacedCell]) {
        self.entries.retain(|e| cells.contains(&e.at()));
    }

    /// Whether any glyph at `at` is held.
    pub fn holds(&self, at: SpacedCell) -> bool {
        self.entries.iter().any(|e| e.at() == at)
    }

    /// The underline and strike-through the helper's strikes at `at`
    /// report (ADR-1302 D3): every answer at one cell and band reports the
    /// frit's first source's, so the first set held there gives them.
    /// `None` while no set at `at` is held.
    fn decorations(&self, at: SpacedCell) -> Option<Decorations> {
        let e = self.entries.iter().find(|e| e.at() == at)?;
        Some(e.face.decorations())
    }

    /// `text`'s glyph at `at` in `style`, from the store: a set of that
    /// style's, drawn as it is.
    fn draw(&self, at: SpacedCell, text: &[char], style: Style) -> Option<Glyph> {
        let face = &self.entry(at, style, text)?.face;
        match text {
            [_] => face.draw(text, style),
            _ => face.cluster(text, style),
        }
    }
}

/// A display's face while the font is a frit: the generated glyph where
/// the helper sent one, else the built-in font's, in the frit's cell
/// (#145), a cluster found as the module says. The built-in fonts in a
/// look's cell, and in a cell made taller by line spacing, draw through it
/// with a store that stays empty. Its cell never moves when glyphs arrive.
/// A built-in glyph is centred in a larger cell and cropped to a smaller
/// one, so both kinds come at the cell's size and the renderer stores them
/// alike.
///
/// Where line spacing adds rows above and below the text band (ADR-1302
/// D13), the helper sends glyphs already placed in the whole cell, and a
/// built-in glyph is fitted to the text band and placed in the whole cell
/// by its default fit ([`Fit::default_for`]): a line or block character
/// stretches, by repeating its top and bottom rows, and the rest keeps its
/// size with blank rows above and below.
pub struct DeferredFace {
    builtin: DisplayFace,
    cell: Cell,
    band: Band,
    step: u32,
    generated: Shared,
}

impl DeferredFace {
    /// `builtin` drawing in `cell`, which a zoom moves by `step`.
    pub fn new(builtin: DisplayFace, cell: Cell, step: u32, generated: Shared) -> DeferredFace {
        DeferredFace::spaced(builtin, cell, Band::NONE, step, generated)
    }

    /// `builtin` drawing in `text`, the text band of a cell that line
    /// spacing makes taller by `band` (ADR-1302 D13), which a zoom moves
    /// by `step`.
    pub fn spaced(
        builtin: DisplayFace,
        text: Cell,
        band: Band,
        step: u32,
        generated: Shared,
    ) -> DeferredFace {
        DeferredFace {
            builtin,
            cell: Cell {
                width: text.width,
                height: text.height.saturating_add(band.added()),
            },
            band,
            step,
            generated,
        }
    }

    /// The cell and band the store holds the face's glyphs by.
    fn at(&self) -> SpacedCell {
        SpacedCell {
            cell: self.cell,
            band: self.band,
        }
    }

    /// The cell's text band, the cell less the rows line spacing adds.
    fn text(&self) -> Cell {
        Cell {
            width: self.cell.width,
            height: self.cell.height.saturating_sub(self.band.added()),
        }
    }

    fn generated_has(&self, c: char) -> bool {
        lock(&self.generated).held_any(self.at(), &[c])
    }

    /// `c`'s glyph in `style`: the store's in that style, else the built-in
    /// font's in the cell, which draws the style itself.
    fn single(&self, c: char, style: Style) -> Option<Glyph> {
        let held = lock(&self.generated).draw(self.at(), &[c], style);
        let fit = Fit::default_for(u32::from(c), is_mark(c));
        held.or_else(|| self.fit(self.builtin.draw(&[c], style)?, fit))
    }

    /// A cluster the store does not hold drawn without its own glyph
    /// (ADR-1200 D22, steps 2 to 4): its NFC form's, else its base's with
    /// its combining marks; the default ignorables and the rest of the
    /// cluster left out. A color base takes no mark, and a color mark is
    /// left out.
    ///
    /// A nonspacing mark is overstruck on the base ([`overstrike`]). A
    /// spacing mark (ADR-1302 D14), a two-part vowel sign as its canonical
    /// parts, is overstruck too where the font already draws it in place
    /// against its letter ([`drawn_beside`]), as GNU Unifont draws the
    /// Indic vowel signs and Myanmar's medial ra; any
    /// other stands beside the letter ([`beside`]), after it, or before it
    /// where its body stands before its letter, as the vowel sign i does.
    fn unshaped(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        if let Some(c) = precomposed(cluster)
            && let Some(g) = lock(&self.generated).draw(self.at(), &[c], style)
        {
            return Some(g);
        }
        let (&base, rest) = cluster.split_first()?;
        let glyph = self.single(base, style)?;
        if glyph.format() != Format::Mask {
            return Some(glyph);
        }
        let mask = |c| self.single(c, style).filter(|g| g.format() == Format::Mask);
        let mut over: Vec<Glyph> = Vec::new();
        let mut spacing: Vec<(Glyph, bool)> = Vec::new();
        for &c in rest.iter().filter(|&&c| is_mark(c) && !is_ignorable(c)) {
            if spacing_side(c).is_none() {
                over.extend(mask(c));
                continue;
            }
            let mut parts = Vec::new();
            decompose(c, &mut parts);
            for part in parts {
                match spacing_side(part) {
                    Some(before) => spacing.extend(mask(part).map(|g| (g, before))),
                    None => over.extend(mask(part)),
                }
            }
        }
        let mut letter = overstrike(&glyph, &over)?;
        let mut stand: Vec<(Glyph, bool)> = Vec::new();
        for (mark, before) in spacing {
            if drawn_beside(&letter, &mark, before) {
                letter = overstrike(&letter, std::slice::from_ref(&mark))?;
            } else {
                stand.push((mark, before));
            }
        }
        beside(letter, &stand)
    }

    /// The built-in's `glyph` at the cell's size: centred in the text
    /// band, padded or cropped, the spare row or column after it, and then
    /// placed in the whole cell as `fit` says (ADR-1302 D13).
    fn fit(&self, glyph: Glyph, fit: Fit) -> Option<Glyph> {
        let boxed = self.boxed(glyph)?;
        vtium_font::spacing::place(&boxed, self.band, fit, true)
    }

    /// The built-in's `glyph` at the text band's size: centred, padded or
    /// cropped, the spare row or column after it.
    fn boxed(&self, glyph: Glyph) -> Option<Glyph> {
        let cells = glyph.cells();
        let text = self.text();
        let width = text.width.checked_mul(cells.cells())?;
        let height = text.height;
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
            || lock(&self.generated).begins(self.at(), c)
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
                let held = lock(&self.generated).draw(self.at(), cluster, style);
                held.or_else(|| self.unshaped(cluster, style))
            }
        }
    }

    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.fit(self.builtin.last_resort(style)?, Fit::Natural)
    }

    fn band(&self) -> Band {
        self.band
    }

    /// The lines the helper's strikes at the cell report, once a set there
    /// is held: an outline first source's own, measured from the font's
    /// `post` and `OS/2` tables, and a bitmap first source's the lines for
    /// the cell's height. The helper moves a font's line that would pass
    /// the cell inside it, and the set reader refuses a strike whose line
    /// passes its cell, so a line held here always lies in the cell's rows.
    /// Before any set at the cell is held: the built-in font's in its own
    /// cell, and in another the lines for the cell's height. Line spacing
    /// moves the built-in font's lines down with its text band (ADR-1302
    /// D13); the helper's strikes report them moved already.
    fn decorations(&self) -> Decorations {
        if let Some(lines) = lock(&self.generated).decorations(self.at()) {
            return lines;
        }
        let text = self.text();
        let lines = if self.builtin.cell() == text {
            self.builtin.decorations()
        } else {
            Decorations::for_height(text.height)
        };
        Decorations {
            underline: lines.underline.saturating_add(self.band.above),
            strike: lines.strike.saturating_add(self.band.above),
            ..lines
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

    /// [`CELL`] with no line spacing.
    const AT: SpacedCell = SpacedCell::plain(CELL);

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

    // ADR-1302 D13: the store holds glyphs by the cell and the rows line
    // spacing adds to it, so a set generated for one band never draws for
    // the same cell size at another
    #[test]
    fn a_set_held_at_one_band_does_not_draw_at_another() {
        let spaced = SpacedCell {
            cell: CELL,
            band: Band { above: 2, below: 2 },
        };
        let mut g = Generated::default();
        let set = solid(CELL, &['A']);
        let fresh = g
            .fresh((spaced, Style::default()), set, &[(0x41, 1)], 1)
            .unwrap();
        assert!(g.add(fresh).is_some());
        assert!(g.held(spaced, Style::default(), &['A']));
        assert!(!g.held(AT, Style::default(), &['A']));
        assert!(g.draw(AT, &['A'], Style::default()).is_none());
        assert!(g.holds(spaced) && !g.holds(AT));
        g.retain(&[AT]);
        assert!(!g.holds(spaced), "the band no display draws at leaves");
    }

    /// Adds `set` at `cell` to `store` as the helper does: what it adds.
    fn add(
        store: &mut Generated,
        cell: Cell,
        set: Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
    ) -> Option<SetFace> {
        let fresh = store.fresh(
            (SpacedCell::plain(cell), Style::default()),
            set,
            &runs,
            bytes,
        )?;
        store.add(fresh)
    }

    /// The screen showing `chars`, no cluster.
    fn shown(chars: &[char]) -> Shown {
        Shown {
            style: Style::default(),
            chars: chars.to_vec(),
            clusters: Vec::new(),
        }
    }

    /// Each of `chars` as a text of its own.
    fn texts(chars: &[char]) -> Vec<Text> {
        chars.iter().map(|&c| vec![c]).collect()
    }

    fn face(generated: &Shared) -> DeferredFace {
        DeferredFace::new(Box::new(Unifont::new(16)), CELL, 16, Arc::clone(generated))
    }

    fn covered(g: &Glyph) -> usize {
        (0..g.height())
            .flat_map(|y| (0..g.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| g.pixel(x, y).unwrap()[0] > 0)
            .count()
    }

    // ADR-1200 D20, D21: the store keys a glyph by its style as well as
    // its text. An italic glyph the helper sent draws for italic alone;
    // regular draws the built-in font's until a regular one comes, and an
    // italic glyph not yet held draws in the built-in font's own synthetic
    // italic, sheared about its baseline (ADR-1302 D4)
    // covers: sgr.italic
    #[test]
    fn a_style_draws_its_own_glyph_else_the_built_in_fonts_in_that_style() {
        let italic = Style {
            bold: false,
            italic: true,
        };
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, italic);
        let g = s.glyph(Width::Narrow, vec![99; (CELL.width * CELL.height) as usize]);
        s.map('A', g);
        let set =
            Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap());
        let generated = Shared::default();
        {
            let mut store = lock(&generated);
            let fresh = store.fresh((AT, italic), set, &[(0x41, 1)], 1).unwrap();
            assert!(store.add(fresh).is_some());
            assert!(store.held(AT, italic, &['A']));
            assert!(!store.held(AT, Style::default(), &['A']));
        }
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let a = f.draw(&['A'], italic).unwrap();
        assert!(all(&a, 99), "the helper's italic A");
        assert_eq!(
            f.draw(&['A'], Style::default()),
            unifont.draw(&['A'], Style::default()),
            "regular A is not held"
        );
        let b = f.draw(&['B'], italic).unwrap();
        assert_eq!(Some(b.clone()), unifont.draw(&['B'], italic));
        assert_ne!(
            b.pixels(),
            unifont.draw(&['B'], Style::default()).unwrap().pixels(),
            "the built-in font's B leans"
        );
    }

    #[test]
    fn the_built_in_font_draws_until_the_glyphs_come() {
        let generated = Shared::default();
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!(a, unifont.draw(&['A'], Style::default()).unwrap());
        add(
            &mut lock(&generated),
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
            &mut lock(&generated),
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
            &mut lock(&generated),
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
        DeferredFace::new(Box::new(Unifont::new(16)), cell, 16, Arc::clone(generated))
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

    /// A set at `cell` mapping `chars` to one solid glyph, whose strike
    /// reports `lines`, as the helper writes the font's own lines in a
    /// strike's header.
    fn with_lines(cell: Cell, chars: &[char], lines: Decorations) -> Arc<Set> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).decorations(lines);
        let g = s.glyph(
            Width::Narrow,
            vec![255; (cell.width * cell.height) as usize],
        );
        for &c in chars {
            s.map(c, g);
        }
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    // #241: the lines of a frit the helper generates from an outline font
    // come from the strike's header, which carries the font's own: in
    // tier 3's 10x19 cell, DejaVu Sans Mono at 16 points puts its
    // underline on row 15 and its strike-through on row 11, where the
    // cell's height alone gives rows 18 and 9
    #[test]
    fn a_generated_frits_lines_come_from_the_strike_header() {
        let generated = Shared::default();
        let cell = Cell {
            width: 10,
            height: 19,
        };
        let f = face_in(&generated, cell);
        assert_eq!(f.decorations(), Decorations::for_height(19));
        let font = Decorations {
            underline: 15,
            underline_thickness: 1,
            strike: 11,
            strike_thickness: 1,
        };
        add(
            &mut lock(&generated),
            cell,
            with_lines(cell, &['A'], font),
            vec![(0x41, 1)],
            1,
        );
        assert_eq!(f.decorations(), font);
        // a set at another cell says nothing about this one
        let other = face_in(&generated, CELL);
        assert_eq!(other.decorations(), Unifont::new(16).decorations());
    }

    // #241: a bitmap frit's strikes report the lines of a face with no
    // metrics of its own, as the helper writes them for a bitmap first
    // source, so its lines stay what they were before its glyphs came,
    // in the built-in font's own cell and in any other
    #[test]
    fn a_bitmap_frits_lines_are_unchanged() {
        for cell in [
            CELL,
            Cell {
                width: 10,
                height: 19,
            },
            Cell {
                width: 16,
                height: 32,
            },
        ] {
            let generated = Shared::default();
            let f = face_in(&generated, cell);
            let before = f.decorations();
            add(
                &mut lock(&generated),
                cell,
                with_lines(cell, &['A'], Decorations::for_height(cell.height)),
                vec![(0x41, 1)],
                1,
            );
            assert_eq!(f.decorations(), before, "{cell:?}");
        }
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
            &mut lock(&generated),
            CELL,
            solid(CELL, &['A']),
            vec![(0x41, 1)],
            1,
        );
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!((a.width(), a.height()), (10, 20));
        assert!(covered(&a) < 10 * 20);
        add(
            &mut lock(&generated),
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
        g.touch(AT, &shown(&['A']));
        assert_eq!(g.make_room(1), vec![(AT, Style::default(), texts(&['B']))]);
        assert_eq!(g.bytes(), 10);
        assert_eq!(g.make_room(1), vec![(AT, Style::default(), texts(&['A']))]);
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
            let mut g = lock(&generated);
            put(&mut g, answer(&[], &[a.clone(), b, c.clone(), w.clone()]))
                + put(&mut g, answer(&[d.clone(), e], &[]))
        };
        assert_eq!(lock(&generated).bytes(), before);
        // shown: D, then A, then C and the wide one; B and E never
        lock(&generated).touch(AT, &shown(&['D']));
        lock(&generated).touch(AT, &shown(&['A']));
        lock(&generated).touch(AT, &shown(&['C', wide]));
        let f = face(&generated);
        let kept = ['A', 'C', wide, 'D'];
        let drawn = kept.map(|k| f.draw(&[k], Style::default()).unwrap());
        // the store after: the same answers written without B and E
        let after = answer(&[], &[a, c, w]).len() as u64 + answer(&[d], &[]).len() as u64;
        assert!(after < before);
        let gone = lock(&generated).make_room(before - after);
        assert_eq!(gone, vec![(AT, Style::default(), texts(&['B', 'E']))]);
        assert_eq!(lock(&generated).bytes(), after);
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
        g.touch(AT, &shown(&['A', 'D']));
        assert_eq!(g.make_room(1), vec![(AT, Style::default(), texts(&['E']))]);
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
        let first = put(&mut lock(&generated), answer(&[], &[a, b.clone()]));
        let f = face(&generated);
        let drawn = ['A', 'B'].map(|k| f.draw(&[k], Style::default()).unwrap());
        // the frame's B differs from the held one, so a B drawn from it shows
        let frame = answer(&[], &[r8('B', 9), c.clone()]);
        let sent = Arc::new(Set::from_bytes(frame.clone()).unwrap());
        let added = add(
            &mut lock(&generated),
            CELL,
            Arc::clone(&sent),
            vec![(0x42, 2)],
            frame.len() as u64,
        )
        .unwrap();
        assert!(added.has_glyph('C') && !added.has_glyph('B'));
        let rebuilt = answer(&[], &[c]).len() as u64;
        assert_eq!(lock(&generated).bytes(), first + rebuilt);
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
        assert!(add(&mut lock(&generated), CELL, held, vec![(0x42, 2)], bytes).is_none());
        assert_eq!(lock(&generated).bytes(), first + rebuilt);
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
            &mut lock(&generated),
            CELL,
            with_clusters(&['e'], &[(&ACUTE, 7)]),
            vec![(0x65, 1)],
            1,
        );
        let f = face(&generated);
        assert!(all(&f.draw(&ACUTE, Style::default()).unwrap(), 7));
        assert!(all(&f.draw(&['e'], Style::default()).unwrap(), 255));
    }

    // ADR-1200 D20: the store compares a cluster's text whole: with e held,
    // e U+0301 is new, and with e U+0301 held, e U+0300 is
    #[test]
    fn the_store_keys_a_cluster_by_its_whole_text() {
        let mut g = Generated::default();
        add(&mut g, CELL, with_clusters(&['e'], &[]), vec![(0x65, 1)], 1);
        assert!(g.held(AT, Style::default(), &['e']) && !g.held(AT, Style::default(), &ACUTE));
        let fresh = g
            .fresh(
                (AT, Style::default()),
                with_clusters(&['e'], &[(&ACUTE, 7)]),
                &[(0x65, 1)],
                1,
            )
            .unwrap();
        assert_eq!(fresh.texts().cloned().collect::<Vec<_>>(), [ACUTE.to_vec()]);
        assert!(g.add(fresh).is_some());
        let grave = ['e', '\u{300}'];
        assert!(g.held(AT, Style::default(), &ACUTE) && !g.held(AT, Style::default(), &grave));
        let fresh = g
            .fresh(
                (AT, Style::default()),
                with_clusters(&[], &[(&grave, 9)]),
                &[],
                1,
            )
            .unwrap();
        assert_eq!(fresh.texts().cloned().collect::<Vec<_>>(), [grave.to_vec()]);
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
        // a precomposed base is decomposed first: ê and dot below is ệ, and
        // ANGSTROM SIGN with acute composes through Å
        assert_eq!(precomposed(&['\u{ea}', '\u{323}']), Some('\u{1ec7}'));
        assert_eq!(precomposed(&['\u{212b}', '\u{301}']), Some('\u{1fa}'));
        assert_eq!(precomposed(&['\u{ac00}', '\u{11a8}']), Some('\u{ac01}'));
        assert_eq!(precomposed(&['e']), None);
        assert_eq!(precomposed(&['q', '\u{301}']), None);
    }

    // ADR-1200 D22 step 2: a cluster the store does not hold draws its NFC
    // form's glyph where the store holds that
    #[test]
    fn a_cluster_draws_its_precomposed_form_from_the_store() {
        let generated = Shared::default();
        add(
            &mut lock(&generated),
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
    fn a_mark_wider_than_its_base_is_drawn_whole() {
        let generated = Shared::default();
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let regular = Style::default();
        let keycap = unifont.draw(&['\u{20e3}'], regular).unwrap();
        assert_eq!(keycap.cells(), Width::Wide, "unifont's keycap is two cells");
        let one = unifont.draw(&['1'], regular).unwrap();
        let drawn = f.draw(&['1', '\u{fe0f}', '\u{20e3}'], regular).unwrap();
        assert_eq!((drawn.width(), drawn.cells()), (16, Width::Wide));
        // the keycap whole, and the 1 centred in it
        assert!({
            (0..16).all(|y| {
                (0..16).all(|x| {
                    let k = keycap.pixel(x, y).unwrap()[0];
                    let o = x
                        .checked_sub(4)
                        .filter(|&x| x < 8)
                        .map_or(0, |x| one.pixel(x, y).unwrap()[0]);
                    drawn.pixel(x, y).unwrap()[0] == k.max(o)
                })
            })
        });
    }

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
            &mut lock(&generated),
            CELL,
            with_clusters(&['q'], &[]),
            vec![(0x71, 1)],
            1,
        );
        assert!(all(&f.draw(&['q', '\u{301}'], regular).unwrap(), 255));
        let held = f.draw(&['q', '\u{301}'], regular).unwrap();
        assert_eq!(held.cells(), Width::Narrow);
    }

    /// A glyph at `cell` from rows of `#` and `.`, `#` fully inked, the rows
    /// not given blank.
    fn drawn(cell: Cell, rows: &[&str]) -> Vec<u8> {
        let mut px = vec![0u8; (cell.width * cell.height) as usize];
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                if c == '#' {
                    px[y * cell.width as usize + x] = 255;
                }
            }
        }
        px
    }

    /// `g`'s rows as `#` where inked and `.` where not.
    fn picture(g: &Glyph) -> Vec<String> {
        g.rows()
            .map(|r| r.iter().map(|&p| if p > 0 { '#' } else { '.' }).collect())
            .collect()
    }

    /// A set at [`CELL`] mapping each of `glyphs`' characters to its rows,
    /// one cell wide, as a frit's bitmap or outline source gives a
    /// character the helper draws alone.
    fn letters(glyphs: &[(char, &[&str])]) -> Arc<Set> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        for &(c, rows) in glyphs {
            let g = s.glyph(Width::Narrow, drawn(CELL, rows));
            s.map(c, g);
        }
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    /// A face whose store holds `glyphs` at [`CELL`].
    fn face_with(glyphs: &[(char, &[&str])]) -> DeferredFace {
        let generated = Shared::default();
        let set = letters(glyphs);
        let runs = glyphs.iter().map(|&(c, _)| (u32::from(c), 1)).collect();
        add(&mut lock(&generated), CELL, set, runs, 1);
        face(&generated)
    }

    const KA: char = '\u{915}';
    const AA: char = '\u{93e}';
    const I: char = '\u{93f}';

    // ka as a font draws it alone: the headline, the stem and the bowl,
    // its ink in the 5 columns at the cell's left
    const KA_ROWS: [&str; 13] = [
        "........", "........", "........", "#####...", "...#....", "...#....", ".#.#....",
        ".#.#....", ".#.#....", ".#.#....", "...#....", "...#....", "...#....",
    ];

    // the vowel sign aa as a font draws it alone, centred in its cell: a
    // piece of the headline and a stem, 2 columns of ink
    const AA_ROWS: [&str; 13] = [
        "........", "........", "........", "..##....", "...#....", "...#....", "...#....",
        "...#....", "...#....", "...#....", "...#....", "...#....", "...#....",
    ];

    // #235: a spacing mark (General_Category Mc) stands beside its letter.
    // Overstruck, the vowel sign aa lands on ka's stem; drawn through the
    // fallback it stands after ka, its ink starting in the column after
    // ka's last, and the two are centred in the cell
    #[test]
    fn a_spacing_mark_stands_after_its_letter() {
        let f = face_with(&[(KA, &KA_ROWS), (AA, &AA_ROWS)]);
        let both = f.draw(&[KA, AA], Style::default()).unwrap();
        assert_eq!((both.width(), both.cells()), (8, Width::Narrow));
        let want = [
            "........", "........", "........", "#######.", "...#..#.", "...#..#.", ".#.#..#.",
            ".#.#..#.", ".#.#..#.", ".#.#..#.", "...#..#.", "...#..#.", "...#..#.", "........",
            "........", "........",
        ];
        assert_eq!(picture(&both), want);
    }

    // #235: a spacing mark whose body stands before its letter, as the
    // vowel sign i does (Indic_Positional_Category Left), stands before it
    #[test]
    fn a_pre_base_spacing_mark_stands_before_its_letter() {
        let i_rows: [&str; 13] = [
            "........", "........", "........", "...##...", "...#....", "...#....", "...#....",
            "...#....", "...#....", "...#....", "...#....", "...#....", "...#....",
        ];
        let f = face_with(&[(KA, &KA_ROWS), (I, &i_rows)]);
        let both = f.draw(&[KA, I], Style::default()).unwrap();
        let want = [
            "........", "........", "........", "#######.", "#....#..", "#....#..", "#..#.#..",
            "#..#.#..", "#..#.#..", "#..#.#..", "#....#..", "#....#..", "#....#..", "........",
            "........", "........",
        ];
        assert_eq!(picture(&both), want);
    }

    // #235, ADR-1302 D14: a letter and its spacing marks that need more
    // room than the letter's cells scale sideways together to fit them,
    // so no ink is cut: the vowel sign's stem ends in the cell's last
    // column, and the letter's headline still starts in its first
    #[test]
    fn a_letter_and_spacing_marks_wider_than_the_cells_scale_to_fit() {
        let wide_ka: [&str; 13] = [
            "........", "........", "........", "########", "......#.", "......#.", ".####.#.",
            "#....##.", "#....##.", ".####.#.", "......#.", "......#.", "......#.",
        ];
        let f = face_with(&[(KA, &wide_ka), (AA, &AA_ROWS)]);
        let both = f.draw(&[KA, AA], Style::default()).unwrap();
        assert_eq!((both.width(), both.cells()), (8, Width::Narrow));
        let ink = |x: u32, y: u32| both.pixel(x, y).unwrap()[0] > 0;
        // the headline runs the whole cell
        assert!((0..8).all(|x| ink(x, 3)));
        // the vowel sign's stem in the last column, alone at the right
        assert!((4..13).all(|y| ink(7, y)), "{:?}", picture(&both));
        // the letter's stem stays left of it, with a blank column between
        assert!((4..13).all(|y| !ink(6, y)), "{:?}", picture(&both));
        assert!((4..13).all(|y| ink(5, y)), "{:?}", picture(&both));
        // the 10 columns of ink scale to 8, so the cell's last column takes
        // eight tenths of the stem's column and two tenths of the blank one
        // before it: 255 * 8 / 10, rounded down
        assert!(
            (4..13).all(|y| both.pixel(7, y).unwrap()[0] == 204),
            "{:?}",
            (4..13)
                .map(|y| both.pixel(7, y).unwrap()[0])
                .collect::<Vec<_>>()
        );
    }

    /// The vowel sign i drawn in the middle of its cell, columns 4 and 5,
    /// away from a letter in columns 0 to 2.
    const I_MID_ROWS: [&str; 13] = [
        "........", "........", "........", "....##..", "....#...", "....#...", "....#...",
        "....#...", "....#...", "....#...", "....#...", "....#...", "....#...",
    ];

    // a narrow letter, its ink in columns 0 to 2
    const NARROW_ROWS: [&str; 13] = [
        "........", "........", "........", "###.....", ".#......", ".#......", ".#......",
        ".#......", ".#......", ".#......", ".#......", ".#......", ".#......",
    ];

    // #235: a spacing mark whose font draws it clear of the letter but on
    // the wrong side, or in the middle of its box, still stands on its own
    // side: the vowel sign i, drawn after the letter's ink, goes before it
    #[test]
    fn a_pre_base_mark_drawn_after_its_letter_moves_before_it() {
        let f = face_with(&[(KA, &NARROW_ROWS), (I, &I_MID_ROWS)]);
        let both = f.draw(&[KA, I], Style::default()).unwrap();
        let want = [
            "........", "........", "........", ".#####..", ".#..#...", ".#..#...", ".#..#...",
            ".#..#...", ".#..#...", ".#..#...", ".#..#...", ".#..#...", ".#..#...", "........",
            "........", "........",
        ];
        assert_eq!(picture(&both), want);
    }

    // #235: and the vowel sign aa, drawn at the left of its box, before a
    // letter at the right of its own, goes after it
    #[test]
    fn a_mark_drawn_before_its_letter_moves_after_it() {
        let aa_left: [&str; 13] = [
            "........", "........", "........", "##......", ".#......", ".#......", ".#......",
            ".#......", ".#......", ".#......", ".#......", ".#......", ".#......",
        ];
        let ka_right: [&str; 13] = [
            "........", "........", "........", ".....###", "......#.", "......#.", "......#.",
            "......#.", "......#.", "......#.", "......#.", "......#.", "......#.",
        ];
        let f = face_with(&[(KA, &ka_right), (AA, &aa_left)]);
        let both = f.draw(&[KA, AA], Style::default()).unwrap();
        let want = [
            "........", "........", "........", ".#####..", "..#..#..", "..#..#..", "..#..#..",
            "..#..#..", "..#..#..", "..#..#..", "..#..#..", "..#..#..", "..#..#..", "........",
            "........", "........",
        ];
        assert_eq!(picture(&both), want);
    }

    /// How many clusters of a letter in `letters` and a sign in `signs`
    /// the fallback draws as the plain overstrike of their Unifont glyphs,
    /// a two-part sign as its canonical parts; it panics on any other.
    fn overstruck_in_unifont(sets: &[(std::ops::RangeInclusive<u32>, &[u32])]) -> usize {
        let generated = Shared::default();
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let regular = Style::default();
        let mut drawn = 0;
        for (letters, signs) in sets {
            for letter in letters
                .clone()
                .filter_map(char::from_u32)
                .filter(|&c| unifont.has_glyph(c))
            {
                for sign in signs.iter().filter_map(|&c| char::from_u32(c)) {
                    let mut parts = Vec::new();
                    decompose(sign, &mut parts);
                    let mut over = unifont.draw(&[letter], regular).unwrap();
                    for part in parts {
                        let glyph = unifont.draw(&[part], regular).unwrap();
                        over = overstrike(&over, &[glyph]).unwrap();
                    }
                    let both = f.draw(&[letter, sign], regular).unwrap();
                    let at = format!("U+{:04X} U+{:04X}", u32::from(letter), u32::from(sign));
                    assert_eq!(both, over, "{at}");
                    assert_eq!(both.cells(), Width::Wide, "{at}");
                    drawn += 1;
                }
            }
        }
        drawn
    }

    // #235: GNU Unifont draws each Devanagari, Bengali, Tamil and
    // Malayalam vowel sign in a box as wide as its letter's, its ink
    // reaching the box's edge on its own side, so laid on the letter it
    // lands beside it. The fallback keeps that placement for every
    // consonant, those whose headline has a gap, such as tha, dha, bha and
    // ha, among them, and for every spacing mark of the four scripts, the
    // stemless loops of Tamil's e and ai and Malayalam's e among them, a
    // two-part sign as its canonical parts
    #[test]
    fn a_spacing_mark_the_font_draws_beside_its_letter_stays_there() {
        let devanagari: &[u32] = &[
            0x903, 0x93b, 0x93e, 0x93f, 0x940, 0x949, 0x94a, 0x94b, 0x94c, 0x94e, 0x94f,
        ];
        let bengali: &[u32] = &[
            0x982, 0x983, 0x9be, 0x9bf, 0x9c0, 0x9c7, 0x9c8, 0x9cb, 0x9cc, 0x9d7,
        ];
        let tamil: &[u32] = &[
            0xbbe, 0xbbf, 0xbc1, 0xbc2, 0xbc6, 0xbc7, 0xbc8, 0xbca, 0xbcb, 0xbcc, 0xbd7,
        ];
        let malayalam: &[u32] = &[
            0xd02, 0xd03, 0xd3e, 0xd3f, 0xd40, 0xd46, 0xd47, 0xd48, 0xd4a, 0xd4b, 0xd4c, 0xd57,
        ];
        let drawn = overstruck_in_unifont(&[
            (0x915..=0x939, devanagari),
            (0x995..=0x9b9, bengali),
            (0xb95..=0xbb9, tamil),
            (0xd15..=0xd39, malayalam),
        ]);
        // each range's 37 codepoints, unifont drawing even the unassigned
        assert_eq!(drawn, 37 * 11 + 37 * 10 + 37 * 11 + 37 * 12);
    }

    // #235, ADR-1302 D14: Unifont draws Myanmar's medial ra as a bracket
    // round its consonant, over and under it and on its left. A mark whose
    // ink wraps its letter's is the font's own placement, so medial ra on
    // every consonant draws as the plain overstrike
    #[test]
    fn a_spacing_mark_the_font_draws_round_its_letter_stays_there() {
        let drawn = overstruck_in_unifont(&[(0x1000..=0x1020, &[0x103c])]);
        assert_eq!(drawn, 33);
    }

    // #235, ADR-1302 D14: a mark wraps its letter's ink only when it passes
    // it on the mark's own side. Unifont's Myanmar tall aa (U+102B) and
    // medial ya (U+103B) span their consonant's height but sit on the wrong
    // side of it, so on every consonant they stand beside it instead of
    // overstriking it
    #[test]
    fn a_mark_spanning_its_letter_on_the_wrong_side_stands_beside_it() {
        let generated = Shared::default();
        let f = face(&generated);
        let unifont = Unifont::new(16);
        let regular = Style::default();
        let mut checked = 0;
        for letter in (0x1000..=0x1020).filter_map(char::from_u32) {
            for sign in [0x102b, 0x103b].into_iter().filter_map(char::from_u32) {
                let alone = unifont.draw(&[letter], regular).unwrap();
                let mark = unifont.draw(&[sign], regular).unwrap();
                let over = overstrike(&alone, &[mark]).unwrap();
                let both = f.draw(&[letter, sign], regular).unwrap();
                let at = format!("U+{:04X} U+{:04X}", u32::from(letter), u32::from(sign));
                assert_ne!(both, over, "{at} is the plain overstrike");
                checked += 1;
            }
        }
        assert_eq!(checked, 33 * 2);
    }

    // #235: the font has placed a mark only in a box as wide as its
    // letter's. Unifont's vowel sign aa, 16 pixels wide, on a frit's
    // letter 8 pixels wide, stands after the letter in the letter's cell
    // instead of being laid on it across two cells
    #[test]
    fn a_mark_in_a_box_wider_than_its_letters_stands_beside_it() {
        let f = face_with(&[(KA, &KA_ROWS)]);
        let sign = Unifont::new(16).draw(&[AA], Style::default()).unwrap();
        assert_eq!(sign.width(), 16, "unifont's aa is two cells");
        let both = f.draw(&[KA, AA], Style::default()).unwrap();
        assert_eq!((both.width(), both.cells()), (8, Width::Narrow));
        // the vowel sign's stem ends in the cell's last column
        assert!(
            (4..13).all(|y| both.pixel(7, y).unwrap()[0] > 0),
            "{:?}",
            picture(&both)
        );
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
            &mut lock(&generated),
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
            &mut lock(&generated),
            CELL,
            with_clusters(&['e'], &[(&ACUTE, 7), (&grave, 9)]),
            vec![(0x65, 1)],
            // more than the set written again takes, so one pass frees it
            1 << 20,
        );
        let shown = Shown {
            style: Style::default(),
            chars: vec!['e'],
            clusters: vec![ACUTE.to_vec()],
        };
        lock(&generated).touch(AT, &shown);
        assert_eq!(
            lock(&generated).make_room(1),
            vec![(AT, Style::default(), vec![grave.to_vec()])]
        );
        let f = face(&generated);
        assert!(all(&f.draw(&ACUTE, Style::default()).unwrap(), 7));
        assert!(!lock(&generated).held(AT, Style::default(), &grave));
        // a frame of the held cluster and a new one adds the new one alone
        let fresh = lock(&generated)
            .fresh(
                (AT, Style::default()),
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
            &mut lock(&generated),
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
        assert!(lock(&generated).holds(SpacedCell::plain(other)));
        lock(&generated).retain(&[AT]);
        assert!(!lock(&generated).holds(SpacedCell::plain(other)));
    }
}
