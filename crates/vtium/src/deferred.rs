//! The deferred pipeline's glyphs (ADR-1301 D2): the per-boot memory
//! store of what the helper generated, and the face that draws a cell from
//! it, falling back per character to the built-in font (#141).
//!
//! The store keys a glyph by its cluster's text, its style, bold, italic,
//! both or neither, and its script, normal, superscript or subscript, at a
//! cell (ADR-1200 D20, ADR-1202 D11), the text one codepoint or a cluster
//! of more, so two cells holding the same cluster in one style and script
//! share its glyph however their clusters were made. A style's glyphs and
//! a script's come by ask in sets of their own (ADR-1200 D21), and until a
//! style's glyph arrives the built-in font draws it in its own synthetic
//! style: a bitmap glyph widened for bold and sheared for italic
//! (ADR-1302 D4). Until a superscript or subscript glyph arrives, the
//! built-in font draws the cell's script glyph, as it draws any glyph on
//! its way (ADR-1200 D7, D22 step 4), and never the frit's normal glyph
//! made small ([`DeferredFace`]'s [`Face::script`]). The lookup chain below
//! runs within the cell's variant: a superscript cluster takes its NFC
//! form's superscript glyph, else its base's with its marks' laid over it,
//! each part from the store in superscript, and the built-in font's only
//! for a part the store lacks (ADR-1202 D11). The lookup for a cell is ADR-1200 D22's: the cluster's glyph in
//! the store; the cluster's NFC form, where it precomposes to one
//! codepoint the store holds; the base's glyph, the store's or the
//! built-in font's, with each nonspacing mark's glyph overstruck on it and
//! each spacing mark's beside it (ADR-1302 D14), each part keeping the ink
//! it carries past the cell (ADR-1306), the default ignorables
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
    Cell, Decorations, Face, Format, Glyph, Narrowing, Origin, RangeCoverage, Script, ScriptFrom,
    SpritePolicy, Style, Variant, Width,
};

mod data;
#[cfg(feature = "bidi")]
mod joined;
#[cfg(feature = "bidi")]
pub use joined::{presentation, presented};

/// The sides a letter of a joining script joins its neighbours on, read in
/// its script's order (ADR-1004 D6): its glyph is keyed with U+200D on
/// each.
#[cfg(feature = "bidi")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sides {
    before: bool,
    after: bool,
}

#[cfg(feature = "bidi")]
impl Sides {
    /// `inner`, a letter and its marks, keyed with U+200D on each side.
    fn key(self, mut inner: Text) -> Text {
        const ZWJ: char = '\u{200d}';
        if self.before {
            inner.insert(0, ZWJ);
        }
        if self.after {
            inner.push(ZWJ);
        }
        inner
    }

    /// Whether the letter joins neither side.
    fn none(self) -> bool {
        !self.before && !self.after
    }
}

/// The most marks a cluster may hold for [`DeferredFace::held_part`] to
/// look for the parts of it the store holds: each set of marks left out is
/// one lookup, 2 to the power of this at most.
const HELD_PART_MARKS: usize = 6;

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

/// What a glyph is keyed by at a cell and in a variant (ADR-1200 D20): its
/// cluster's text, one codepoint or more.
pub type Text = Vec<char>;

/// What the displays show in one variant, a style and a script: each
/// character, and each cluster of more than one, each sorted and once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub variant: Variant,
    pub chars: Vec<char>,
    pub clusters: Vec<Text>,
}

/// Adds to `all` what `frame` shows, as the console reads it to ask the
/// helper for the glyphs a display draws, in each variant a cell draws its
/// glyph in, bold, italic, both or neither, and normal, superscript or
/// subscript (ADR-1200 D21, ADR-1202 D11): each character,
/// every character of a cluster among them, and each cluster whole, and
/// what a right-to-left row draws ([`vtium_render::joined`]): each joined
/// letter's key, each lam-alef ligature's, a mirrored cluster as it draws,
/// and the presentation form each key draws where no font holds the joined
/// form (ADR-1004 D6, D7). A variant `all` lacks is added after the others.
/// A build without the bidi feature reads the frame in `Head::visible`
/// alone, so the minspec build carries none of this.
#[cfg(feature = "bidi")]
pub fn read_shown(frame: &vtium_term::Frame<'_>, all: &mut Vec<Shown>) {
    use vtium_render::joined;
    use vtium_term::Glyph;
    let mut drawn = Vec::new();
    for row in 0..frame.rows() {
        for (col, cell) in frame.cells(row).enumerate() {
            let variant = variant_of(cell.attrs);
            if !all.iter().any(|s| s.variant == variant) {
                all.push(Shown {
                    variant,
                    ..Shown::default()
                });
            }
            let Some(shown) = all.iter_mut().find(|s| s.variant == variant) else {
                continue;
            };
            match cell.glyph {
                Glyph::Char(c) => shown.chars.push(c),
                Glyph::Cluster(text) => {
                    shown.chars.extend(text.chars());
                    shown.clusters.push(text.chars().collect());
                }
                Glyph::Empty => {}
            }
            // the cell's own text, and a lam-alef ligature's too, since the
            // cursor standing on it draws its letters apart
            drawn.clear();
            joined::text(&cell, &mut drawn);
            let mut texts = vec![drawn.clone()];
            if joined::span(frame, (col, row), &cell, None) == joined::Span::Pair
                && let Some(lam) = frame.cell(row, col.saturating_add(1))
            {
                let mut pair = Vec::new();
                joined::pair_text(&cell, &lam, &mut pair);
                texts.push(pair);
            }
            for text in texts.into_iter().filter(|t| t.len() > 1) {
                shown.chars.extend(presentation(&text));
                shown
                    .chars
                    .extend(text.iter().filter(|&&c| c != joined::ZWJ));
                shown.clusters.push(text);
            }
        }
    }
}

/// The variant a cell with `attrs` draws its glyph in: its style, bold,
/// italic, both or neither, and its script, normal, superscript (SGR 73)
/// or subscript (SGR 74), as the renderer draws it.
pub fn variant_of(attrs: vtium_term::Attrs) -> Variant {
    use vtium_term::{Attrs, Script as Position};
    Variant {
        style: Style {
            bold: attrs.contains(Attrs::BOLD),
            italic: attrs.contains(Attrs::ITALIC),
        },
        script: match attrs.script() {
            Position::Baseline => Script::Baseline,
            Position::Super => Script::Super,
            Position::Sub => Script::Sub,
        },
    }
}

/// Each of `all`'s characters and clusters sorted and once, white space
/// left out, as the console asks the helper for them.
#[cfg(feature = "bidi")]
pub fn settle(all: &mut [Shown]) {
    for shown in all {
        shown.chars.retain(|c| !c.is_whitespace());
        shown.chars.sort_unstable();
        shown.chars.dedup();
        shown.clusters.sort_unstable();
        shown.clusters.dedup();
    }
}

/// Whether `c` is a combining mark, drawn over its base.
fn is_mark(c: char) -> bool {
    within(&data::MARKS, c)
}

/// Whether `c` is a default ignorable, drawn as nothing.
fn is_ignorable(c: char) -> bool {
    within(&data::IGNORABLES, c)
}

#[cfg(feature = "bidi")]
/// Whether a part of a cluster may leave `c` out: a combining mark or a
/// default ignorable, which the helper leaves out of a cluster's part when
/// the font draws no glyph for it (ADR-1200 D22 step 3).
pub fn droppable(c: char) -> bool {
    is_mark(c) || is_ignorable(c)
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

/// Parts of a glyph laid together with the ink each carries past its cells
/// (ADR-1200 D22 step 3, ADR-1306 D1): a canvas over the reach of a glyph
/// `width` by `height` pixels that spans `cells` cells, from `x0`, `y0`
/// relative to the glyph's top left, so that a mark the font sets above
/// the row keeps its ink when the cluster is built from its parts.
struct Layers {
    width: u32,
    height: u32,
    cells: Width,
    x0: i64,
    y0: i64,
    stride: usize,
    pixels: Vec<u8>,
}

impl Layers {
    fn new(width: u32, height: u32, cells: Width) -> Option<Layers> {
        let cell = i64::from(width.checked_div(cells.cells())?);
        let across = cell.checked_mul(i64::from(vtium_font::REACH_CELLS))?;
        let down = i64::from(height).checked_mul(i64::from(vtium_font::REACH_ROWS))?;
        let stride = i64::from(width).checked_add(across.checked_mul(2)?)?;
        let rows = i64::from(height).checked_add(down.checked_mul(2)?)?;
        Some(Layers {
            width,
            height,
            cells,
            x0: across.checked_neg()?,
            y0: down.checked_neg()?,
            stride: usize::try_from(stride).ok()?,
            pixels: vec![0; usize::try_from(stride.checked_mul(rows)?).ok()?],
        })
    }

    /// `part` laid with its cells' left edge at column `dx`: its ink box
    /// where it has one, else its own pixels; ink past the reach is cut.
    fn lay(&mut self, part: &Glyph, dx: i64) {
        let (left, top, w, px) = match part.ink() {
            Some(ink) => (ink.left(), ink.top(), ink.width(), ink.pixels()),
            None => (0, 0, part.width(), part.pixels()),
        };
        let Ok(w) = usize::try_from(w) else { return };
        let x0 = i64::from(left).saturating_add(dx).saturating_sub(self.x0);
        let y0 = i64::from(top).saturating_sub(self.y0);
        for (y, row) in (y0..).zip(px.chunks_exact(w.max(1))) {
            let Ok(y) = usize::try_from(y) else { continue };
            let Some(line) = self
                .pixels
                .get_mut(y.saturating_mul(self.stride)..)
                .and_then(|l| l.get_mut(..self.stride))
            else {
                continue;
            };
            for (x, &p) in (x0..).zip(row) {
                if let Some(at) = usize::try_from(x).ok().and_then(|x| line.get_mut(x)) {
                    *at = (*at).max(p);
                }
            }
        }
    }

    /// The glyph the parts make: its own pixels inside its cells, and an
    /// ink box over its cells and the ink past them, where there is any.
    fn glyph(self) -> Option<Glyph> {
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        // the box: the cells, and every row and column that holds ink
        let (mut l, mut t, mut r, mut b) = (0i64, 0i64, w, h);
        for (y, line) in self.pixels.chunks_exact(self.stride.max(1)).enumerate() {
            let inked = line.iter().position(|&p| p > 0);
            let last = line.iter().rposition(|&p| p > 0);
            if let (Some(first), Some(last)) = (inked, last) {
                let y = i64::try_from(y).ok()?.checked_add(self.y0)?;
                (t, b) = (t.min(y), b.max(y.checked_add(1)?));
                l = l.min(i64::try_from(first).ok()?.checked_add(self.x0)?);
                r = r.max(
                    i64::try_from(last)
                        .ok()?
                        .checked_add(1)?
                        .checked_add(self.x0)?,
                );
            }
        }
        let cut = |x0: i64, y0: i64, bw: i64, bh: i64| -> Option<Vec<u8>> {
            let mut out = Vec::with_capacity(usize::try_from(bw.checked_mul(bh)?).ok()?);
            for y in y0..y0.checked_add(bh)? {
                let row = usize::try_from(y.checked_sub(self.y0)?).ok()?;
                let from = usize::try_from(x0.checked_sub(self.x0)?).ok()?;
                let line = self.pixels.get(row.checked_mul(self.stride)?..)?;
                out.extend_from_slice(
                    line.get(from..from.checked_add(usize::try_from(bw).ok()?)?)?,
                );
            }
            Some(out)
        };
        let own = cut(0, 0, w, h)?;
        let glyph =
            Glyph::from_pixels(self.width, self.height, Format::Mask, own)?.with_cells(self.cells);
        if (l, t, r, b) == (0, 0, w, h) {
            return Some(glyph);
        }
        let (bw, bh) = (r.checked_sub(l)?, b.checked_sub(t)?);
        let boxed = cut(l, t, bw, bh)?;
        let at = (i32::try_from(l).ok()?, i32::try_from(t).ok()?);
        let size = (u32::try_from(bw).ok()?, u32::try_from(bh).ok()?);
        glyph.clone().with_ink(at, size, boxed).or(Some(glyph))
    }
}

/// `base` with each of `marks` overstruck on it, the base and each mark
/// centred across the widest of them, so a mark two cells wide, as U+20E3
/// is in unifont, makes the glyph two cells wide. Each part keeps the ink
/// it carries past its cells, cut to the reach (ADR-1200 D22 step 3).
fn overstrike(base: &Glyph, marks: &[Glyph]) -> Option<Glyph> {
    if marks.is_empty() {
        return Some(base.clone());
    }
    let parts: Vec<&Glyph> = std::iter::once(base).chain(marks).collect();
    let w = parts.iter().map(|g| g.width()).max()?;
    let wide = parts.iter().any(|g| g.cells() == Width::Wide);
    let cells = if wide { Width::Wide } else { base.cells() };
    let mut layers = Layers::new(w, base.height(), cells)?;
    for part in parts {
        layers.lay(part, i64::from(w.saturating_sub(part.width()) / 2));
    }
    layers.glyph()
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
/// letter's cells, each part keeping the ink it carries past its cells.
/// Where the run is wider than the cells, no ink is cut. A run of 1-bit
/// parts, such as bitmap glyphs, comes whole, as wide as its ink, for
/// [`Glyph::fit`] to squeeze into the cells it is drawn in by OR pooling,
/// as it squeezes any bitmap glyph wider than its cells. A run with an
/// outline part is narrowed to the cells by coverage
/// ([`Glyph::narrowed`]).
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
    if canvas == w {
        let mut layers = Layers::new(letter.width(), h, letter.cells())?;
        for (part, x) in placed {
            layers.lay(part, x.checked_add(shift)?);
        }
        return layers.glyph();
    }
    let one_bit = placed
        .iter()
        .all(|(part, _)| part.pixels().iter().all(|&p| p == 0 || p == 255));
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
    let run = Glyph::from_pixels(u32::try_from(canvas).ok()?, h, Format::Mask, pixels)?
        .with_cells(letter.cells());
    if one_bit {
        return Some(run);
    }
    run.narrowed(letter.width(), Narrowing::Coverage)
        .map(|g| g.with_cells(letter.cells()))
}

/// One set the helper sent, or its rebuild: its cell and the rows line
/// spacing adds to it, the style and script of its strikes, the face it draws
/// through, each text it maps with when the screen last showed it, and its
/// bytes.
#[derive(Debug)]
struct Entry {
    cell: Cell,
    band: Band,
    variant: Variant,
    face: SetFace,
    /// Sorted by text.
    glyphs: Vec<(Text, u64)>,
    bytes: u64,
}

/// The glyph `face` draws for `text` in `variant`: a codepoint's from the
/// set's map, a cluster's from its cluster map, at the face's scale or at
/// a scale of 1. A superscript or subscript glyph comes from a strike of
/// its script alone, at the face's scale, which is 1 for the helper's
/// sets.
fn draw_text(face: &SetFace, text: &[char], variant: Variant, unscaled: bool) -> Option<Glyph> {
    let style = variant.style;
    match text {
        _ if variant.script != Script::Baseline => face.draw_variant(text, variant),
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
            && let Some((face, bytes)) = rewrite(&self.face, self.cell, self.variant, &self.glyphs)
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
/// with only `kept`'s glyphs at `cell` in `variant`, as the helper writes an
/// answer:
/// a strike a pixel format, its chunks raw, a codepoint in its map and a
/// cluster in its cluster map. Each glyph keeps its format: RGBA stays
/// RGBA, and a mask whose every pixel, and every pixel of its ink box, is
/// 0 or 255 in a set with a 1-bit strike stays 1-bit, every other mask R8.
/// Each mask keeps its ink box past its cells (ADR-1306 D3), so the set
/// written again draws every kept glyph as the helper's set drew it. The
/// new face and its bytes; `None` when a glyph cannot be drawn or the set
/// cannot be written.
fn rewrite(
    face: &SetFace,
    cell: Cell,
    variant: Variant,
    kept: &[(Text, u64)],
) -> Option<(SetFace, u64)> {
    let set = face.set();
    let Variant { style, script } = variant;
    let strikes: Vec<StrikeInfo> = set
        .strikes()
        .filter(|s| s.cell == cell && s.style == style && s.script == script)
        .collect();
    let one_bit = strikes.iter().any(|s| s.format == PixelFormat::Bits);
    let formats = [PixelFormat::Bits, PixelFormat::R8, PixelFormat::Rgba];
    let mut builders = formats.map(|format| {
        let mut s = StrikeBuilder::new(format, cell, style)
            .script(script)
            .compress(false);
        if let Some(info) = strikes.iter().find(|s| s.format == format) {
            s = s.baseline(info.baseline).decorations(info.decorations);
        }
        (s, BTreeMap::<(bool, Vec<u8>, Option<InkBytes>), u32>::new())
    });
    let binary = |pixels: &[u8]| pixels.iter().all(|&p| p == 0 || p == 0xff);
    for (text, _) in kept {
        let glyph = draw_text(face, text, variant, true)?;
        let pixels = glyph.pixels();
        let ink = glyph.ink();
        let (slot, bytes, ink) = match glyph.format() {
            Format::Rgba => (2, pixels.to_vec(), None),
            Format::Mask if one_bit && binary(pixels) && ink.is_none_or(|i| binary(i.pixels())) => {
                let ink = match ink {
                    Some(i) => Some(ink_bytes(i, pack(i.pixels(), i.width())?)),
                    None => None,
                };
                (0, pack(pixels, glyph.width())?, ink)
            }
            Format::Mask => (
                1,
                pixels.to_vec(),
                ink.map(|i| ink_bytes(i, i.pixels().to_vec())),
            ),
        };
        let (strike, placed) = builders.get_mut(slot)?;
        let wide = glyph.cells() == Width::Wide;
        // texts that shared a glyph, its pixels and its ink, share it again
        let key = (wide, bytes, ink);
        let index = match placed.get(&key) {
            Some(&i) => i,
            None => {
                let (_, bytes, ink) = &key;
                let i = strike.glyph(glyph.cells(), bytes.clone());
                if let Some((at, size, boxed)) = ink {
                    strike.ink(i, *at, *size, boxed.clone());
                }
                placed.insert(key, i);
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

/// An ink box as a strike writes it ([`StrikeBuilder::ink`]): its place
/// from the glyph's first cell, its size, and its bytes in the strike's
/// format.
type InkBytes = ((i32, i32), (u32, u32), Vec<u8>);

/// `ink`'s place and size, with `bytes`, its pixels in the strike's format.
fn ink_bytes(ink: &vtium_font::Ink, bytes: Vec<u8>) -> InkBytes {
    ((ink.left(), ink.top()), (ink.width(), ink.height()), bytes)
}

/// A mask of 0 and 255, `width` pixels a row, as 1-bit rows padded to a
/// byte, the first pixel the high bit.
fn pack(pixels: &[u8], width: u32) -> Option<Vec<u8>> {
    let width = usize::try_from(width).ok().filter(|&w| w > 0)?;
    let row = width.div_ceil(8);
    let mut out = Vec::with_capacity(row.checked_mul(pixels.len().checked_div(width)?)?);
    for line in pixels.chunks(width) {
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
    /// adds to it (ADR-1302 D13), in `variant`, mapping the codepoints of
    /// `runs` and the clusters its cluster map holds at the cell in that
    /// variant, and taking `bytes`, without the glyphs the store holds at
    /// `at` in `variant` (ADR-1200 D18): a set that carries some is written again without them, as
    /// compaction writes one ([`rewrite`]), and counts the bytes written.
    /// It adds nothing when it carries nothing new, or when it cannot be
    /// written again. `None` when `set` has no face at the cell.
    pub fn fresh(
        &self,
        (at, variant): (impl Into<SpacedCell>, Variant),
        set: Arc<Set>,
        runs: &[(u32, u32)],
        bytes: u64,
    ) -> Option<Fresh> {
        let at: SpacedCell = at.into();
        let clusters: Vec<Text> = set
            .strike_clusters()
            .filter(|(of, _)| {
                of.cell == at.cell && of.style == variant.style && of.script == variant.script
            })
            .map(|(_, key)| key)
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
            .filter(|t| self.held(at, variant, t))
            .cloned()
            .collect();
        let mut entry = Entry {
            cell: at.cell,
            band: at.band,
            variant,
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

    /// Whether the store holds at `at`, in any variant, the glyph of a
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

    /// Whether the store holds `text`'s glyph at `at` in `variant`.
    fn held(&self, at: SpacedCell, variant: Variant, text: &[char]) -> bool {
        self.entry(at, variant, text).is_some()
    }

    /// Whether the store holds `text`'s glyph at `at` in `variant`, for
    /// the tests of the console's record of what it holds.
    #[cfg(test)]
    pub(crate) fn holds_variant(&self, at: SpacedCell, variant: Variant, text: &[char]) -> bool {
        self.held(at, variant, text)
    }

    /// Whether the store holds `text`'s glyph at `at` in any variant.
    fn held_any(&self, at: SpacedCell, text: &[char]) -> bool {
        self.entries
            .iter()
            .any(|e| e.at() == at && e.find(text).is_ok())
    }

    /// The entry holding `text`'s glyph at `at` in `variant`.
    fn entry(&self, at: SpacedCell, variant: Variant, text: &[char]) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.at() == at && e.variant == variant && e.find(text).is_ok())
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
    /// go too. The texts no longer held, by cell, band and variant; nothing
    /// is redrawn, and each is asked for again when the screen shows it.
    pub fn make_room(&mut self, overrun: u64) -> Vec<(SpacedCell, Variant, Vec<Text>)> {
        let mut out: Vec<(SpacedCell, Variant, Vec<Text>)> = Vec::new();
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
                let (at, variant) = (e.at(), e.variant);
                if empty {
                    self.entries.remove(i);
                }
                match out
                    .iter_mut()
                    .find(|(of_at, of, _)| (*of_at, *of) == (at, variant))
                {
                    Some((_, _, texts)) => texts.extend(lost),
                    None => out.push((at, variant, lost)),
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
    /// and its clusters in its variant are the most recently shown.
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
            .filter(|e| e.at() == at && e.variant == shown.variant)
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

    /// The lines the helper's normal strikes at `at` report (ADR-1302 D3):
    /// every answer for normal glyphs at one cell and band reports the
    /// frit's first source's, so the first such set held there gives them.
    /// `None` while no set of normal glyphs at `at` is held, since a
    /// superscript or subscript set reports the lines of its own script.
    fn decorations(&self, at: SpacedCell) -> Option<Decorations> {
        let e = self
            .entries
            .iter()
            .find(|e| e.at() == at && e.variant.script == Script::Baseline)?;
        Some(e.face.decorations())
    }

    /// The lines the helper's strikes of `script` at `at` report: the
    /// lines of the frit's first source at the small glyph's size, where
    /// its own box lies (ADR-1202 D8). Every answer in one script at one
    /// cell and band reports the same lines, so the first such set held
    /// there gives them. `None` while none is held.
    fn script_decorations(&self, at: SpacedCell, script: Script) -> Option<Decorations> {
        let e = self
            .entries
            .iter()
            .find(|e| e.at() == at && e.variant.script == script)?;
        Some(e.face.script_decorations(script))
    }

    /// How many cells the store's normal glyph for `c` at `at` spans, as
    /// its set's map marks it, without drawing it; `None` when the store
    /// holds no such glyph.
    fn advance(&self, at: SpacedCell, c: char) -> Option<Width> {
        self.entry(at, Variant::default(), &[c])?.face.advance(c)
    }

    /// `text`'s glyph at `at` in `variant`, from the store: a set of that
    /// variant's, drawn as it is.
    fn draw(&self, at: SpacedCell, text: &[char], variant: Variant) -> Option<Glyph> {
        let face = &self.entry(at, variant, text)?.face;
        draw_text(face, text, variant, false)
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

    /// `c`'s glyph in `variant`: the store's in that variant, else the
    /// built-in font's in the cell, which draws the style and the script
    /// itself.
    fn single(&self, c: char, variant: Variant) -> Option<Glyph> {
        let held = lock(&self.generated).draw(self.at(), &[c], variant);
        held.or_else(|| self.built_in_variant(c, variant))
    }

    /// `cluster`'s glyph in `variant` (ADR-1200 D22): the store's glyph for
    /// the whole cluster in the variant, else, for a joined letter's key or
    /// lam and alef, their joined forms (ADR-1004 D6), else the cluster
    /// built from its parts ([`DeferredFace::unshaped`]), each part from the
    /// store in the variant or else from the built-in font.
    fn draw_in(&self, cluster: &[char], variant: Variant) -> Option<Glyph> {
        match cluster {
            [] => None,
            &[c] => self.single(c, variant),
            _ => {
                let held = lock(&self.generated).draw(self.at(), cluster, variant);
                #[cfg(feature = "bidi")]
                if let Some(key) = joined::Key::of(cluster) {
                    return held.or_else(|| self.joined(&key, variant));
                }
                #[cfg(feature = "bidi")]
                return held.or_else(|| self.unshaped(Sides::default(), cluster, variant));
                #[cfg(not(feature = "bidi"))]
                held.or_else(|| self.unshaped(cluster, variant))
            }
        }
    }

    /// `c`'s glyph in `variant` from the built-in font, fitted to the cell:
    /// the normal glyph, or the built-in font's own superscript or
    /// subscript glyph, made small as the built-in font's (ADR-1202 D4,
    /// D11).
    fn built_in_variant(&self, c: char, variant: Variant) -> Option<Glyph> {
        let normal = self.built_in(c, variant.style)?;
        if variant.script == Script::Baseline {
            return Some(normal);
        }
        let font = vtium_font::Font::new(Builtins(self));
        Some(font.drawn_variant(&[c], normal.cells(), variant)?.glyph)
    }

    /// `c`'s glyph in `style` from the built-in font, fitted to the cell.
    fn built_in(&self, c: char, style: Style) -> Option<Glyph> {
        let fit = Fit::default_for(u32::from(c), is_mark(c));
        self.fit(self.builtin.draw(&[c], style)?, fit)
    }

    /// The built-in font's origin for `cluster`'s first character in
    /// `style`, or for its last resort when `cluster` is empty, moved where
    /// [`DeferredFace::fit`] puts the built-in glyph in the text band
    /// (ADR-1202 D1, D4); the default, a smooth glyph over the text band,
    /// when the built-in font draws nothing for it.
    fn built_in_origin(&self, cluster: &[char], style: Style) -> Origin {
        let first = cluster.get(..1).unwrap_or_default();
        let glyph = match *first {
            [] => self.builtin.last_resort(style),
            _ => self.builtin.draw(first, style),
        };
        let Some(glyph) = glyph else {
            return Origin::default();
        };
        let text = self.text();
        let to = (
            text.width.saturating_mul(glyph.cells().cells()),
            text.height,
        );
        self.builtin.origin(first, style).boxed(
            (glyph.width(), glyph.height()),
            to,
            self.band.above,
        )
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
    ///
    /// The letter is joined on `sides` (ADR-1004 D6): it takes the store's
    /// part of its key ([`DeferredFace::held_part`]) or its joined form
    /// ([`DeferredFace::letter`]), and no NFC form, which would draw it
    /// unjoined. A build without the bidi feature joins nothing.
    fn unshaped(
        &self,
        #[cfg(feature = "bidi")] sides: Sides,
        cluster: &[char],
        variant: Variant,
    ) -> Option<Glyph> {
        #[cfg(not(feature = "bidi"))]
        if let Some(c) = precomposed(cluster)
            && let Some(g) = lock(&self.generated).draw(self.at(), &[c], variant)
        {
            return Some(g);
        }
        #[cfg(feature = "bidi")]
        if sides.none()
            && let Some(c) = precomposed(cluster)
            && let Some(g) = lock(&self.generated).draw(self.at(), &[c], variant)
        {
            return Some(g);
        }
        let (&base, rest) = cluster.split_first()?;
        let marks: Vec<char> = rest
            .iter()
            .copied()
            .filter(|&c| is_mark(c) && !is_ignorable(c))
            .collect();
        #[cfg(not(feature = "bidi"))]
        let (glyph, marks) = match self.held_part(base, &marks, variant) {
            Some(held) => held,
            None => (self.single(base, variant)?, marks),
        };
        #[cfg(feature = "bidi")]
        let (glyph, marks) = match self.held_part(sides, base, &marks, variant) {
            Some(held) => held,
            None => (self.letter(sides, base, variant)?, marks),
        };
        if glyph.format() != Format::Mask {
            return Some(glyph);
        }
        let mask = |c| {
            self.single(c, variant)
                .filter(|g| g.format() == Format::Mask)
        };
        let mut over: Vec<Glyph> = Vec::new();
        let mut spacing: Vec<(Glyph, bool)> = Vec::new();
        for c in marks {
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

    /// `base`'s own glyph joined on `sides`: the base's glyph where it
    /// joins nothing ([`DeferredFace::single`]), else a joined letter's
    /// form ([`DeferredFace::form`]).
    #[cfg(feature = "bidi")]
    fn letter(&self, sides: Sides, base: char, variant: Variant) -> Option<Glyph> {
        if sides.none() {
            self.single(base, variant)
        } else {
            self.form(sides, base, variant)
        }
    }

    /// The store's glyph for the most of a cluster it holds, `base` joined
    /// on `sides` with as many of `marks` as the helper shaped with it, and
    /// the marks left out, which the caller lays over it (ADR-1200 D22 step
    /// 3): a cluster whose mark the font set lacks, as lam with shadda,
    /// fatha and superscript alef, draws the rest as the font shapes it,
    /// its marks where the font puts them. The helper sends that part under
    /// its own key when it shapes a cluster whose mark no glyph draws. `None` when the store holds no such part
    /// with at least one mark, or the cluster has more than
    /// [`HELD_PART_MARKS`] marks.
    fn held_part(
        &self,
        #[cfg(feature = "bidi")] sides: Sides,
        base: char,
        marks: &[char],
        variant: Variant,
    ) -> Option<(Glyph, Vec<char>)> {
        let n = marks.len();
        if !(2..=HELD_PART_MARKS).contains(&n) {
            return None;
        }
        let store = lock(&self.generated);
        // the fewest marks left out first, at least one, at most all but one
        for out in 1..n {
            for set in 0u32..1 << n {
                if set.count_ones() as usize != out {
                    continue;
                }
                let left = |i: usize| set >> i & 1 == 1;
                let mut text = vec![base];
                text.extend((0..n).filter(|&i| !left(i)).filter_map(|i| marks.get(i)));
                #[cfg(feature = "bidi")]
                let text = sides.key(text);
                let Some(glyph) = store.draw(self.at(), &text, variant) else {
                    continue;
                };
                let rest = (0..n).filter(|&i| left(i)).filter_map(|i| marks.get(i));
                return Some((glyph, rest.copied().collect()));
            }
        }
        None
    }

    /// The built-in's `glyph` at the cell's size: centred in the text
    /// band, padded or cropped, the spare row or column after it, and then
    /// placed in the whole cell as `fit` says (ADR-1302 D13).
    fn fit(&self, glyph: Glyph, fit: Fit) -> Option<Glyph> {
        let boxed = self.boxed(glyph)?;
        vtium_font::spacing::place(&boxed, self.band, fit, true)
    }

    /// The built-in's `glyph` at the text band's height, centred, its rows
    /// padded or cut, the spare row after it, and padded to the text
    /// band's width of its cells, centred, the spare column after it. No
    /// column of a glyph wider than its cells is cut here: it keeps its
    /// width, and [`Glyph::fit`] squeezes it, or lets it reach a pixel past
    /// the cells it is drawn in (ADR-1200 D7, ADR-1302 D14).
    fn boxed(&self, glyph: Glyph) -> Option<Glyph> {
        let cells = glyph.cells();
        let text = self.text();
        let width = text.width.checked_mul(cells.cells())?;
        let height = text.height;
        if (glyph.width(), glyph.height()) == (width, height) {
            return Some(glyph);
        }
        let bpp = usize::try_from(glyph.format().bytes_per_pixel()).ok()?;
        let dy = i64::from(height).saturating_sub(i64::from(glyph.height())) / 2;
        let stride = usize::try_from(glyph.width()).ok()?.checked_mul(bpp)?;
        let mut pixels = vec![0u8; stride.checked_mul(usize::try_from(height).ok()?)?];
        for (y, row) in pixels.chunks_exact_mut(stride).enumerate() {
            let src = i64::try_from(y)
                .ok()
                .and_then(|y| usize::try_from(y.saturating_sub(dy)).ok())
                .and_then(|sy| glyph.rows().nth(sy));
            if let Some(src) = src {
                row.copy_from_slice(src);
            }
        }
        let rows = Glyph::from_pixels(glyph.width(), height, glyph.format(), pixels)?;
        if rows.width() < width {
            rows.fit(width, cells)
        } else {
            Some(rows.with_cells(cells))
        }
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
    /// In a build with the bidi feature U+200D counts as held too: a key
    /// that starts with it is a joined letter's ([`joined`]), which draws
    /// at least the letter's own glyph.
    fn has_glyph(&self, c: char) -> bool {
        #[cfg(feature = "bidi")]
        if c == '\u{200d}' {
            return true;
        }
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

    /// The store's glyph's cells, else the built-in font's: the glyph
    /// [`Face::draw`] gives `c`, measured without drawing it. A built-in
    /// glyph keeps its cells when it is fitted to the face's cell.
    fn advance(&self, c: char) -> Option<Width> {
        lock(&self.generated)
            .advance(self.at(), c)
            .or_else(|| self.builtin.advance(c))
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.draw_in(cluster, style.into())
    }

    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.fit(self.builtin.last_resort(style)?, Fit::Natural)
    }

    fn band(&self) -> Band {
        self.band
    }

    /// For a character the store lacks, and for the last resort, the
    /// built-in font's origin, moved where [`DeferredFace::fit`] puts the
    /// built-in glyph in the text band (ADR-1202 D1, D4). The store's
    /// glyphs come from the helper, and a longer cluster draws from several
    /// glyphs, so their origin is the default, a smooth glyph over the text
    /// band.
    fn origin(&self, cluster: &[char], style: Style) -> Origin {
        match *cluster {
            [] => self.built_in_origin(cluster, style),
            [c] if lock(&self.generated)
                .draw(self.at(), &[c], style.into())
                .is_none() =>
            {
                self.built_in_origin(cluster, style)
            }
            _ => Origin::default(),
        }
    }

    /// The face's built-in font alone, fitted to its cell as it draws a
    /// character the store lacks, whatever the store holds.
    fn builtins(&self) -> Option<Box<dyn Face + '_>> {
        Some(Box::new(Builtins(self)))
    }

    /// A superscript or subscript cell's glyph (ADR-1202 D11): the one the
    /// helper sent for the cluster in its variant, drawn as it came, small
    /// and placed in the whole cell; else, while it is on its way or when
    /// the frit lacks the character, the built-in font's script glyph,
    /// which also draws the character's normal glyph meanwhile (ADR-1200
    /// D7, D22 step 4). The frit's normal glyph is never made small here,
    /// since only the helper's source draws a frit's script glyph
    /// (ADR-1202 D15).
    ///
    /// A cluster runs ADR-1200 D22's chain within the cell's script
    /// (ADR-1202 D11): its own glyph in the script, else its NFC form's,
    /// else its base's with each mark's laid over it, each from the store
    /// in the script, and the built-in font's script glyph only for a part
    /// the store lacks or that is still on its way.
    fn script(&self, cluster: &[char], variant: Variant) -> ScriptFrom {
        if variant.script == Script::Baseline {
            return ScriptFrom::Normal;
        }
        let made = match cluster {
            [_] => lock(&self.generated).draw(self.at(), cluster, variant),
            _ => self.draw_in(cluster, variant),
        };
        match made {
            Some(glyph) => ScriptFrom::Made(glyph),
            None => ScriptFrom::Builtins,
        }
    }

    /// The built-in font's glyph for a single character, in place of the
    /// font set's own that would draw as the last step of the fallback
    /// (ADR-1302 D14, ADR-1200 D7): for a character whose Emoji_Presentation
    /// is No, which takes one cell, as ❤ does, its text glyph before a
    /// color glyph in one cell; and for any character a built-in glyph no
    /// wider than `width`, before a squeeze. A cluster of several
    /// characters takes none.
    fn fallback(&self, cluster: &[char], style: Style, width: u32, color: bool) -> Option<Glyph> {
        let &[c] = cluster else {
            return None;
        };
        if !self.builtin.has_glyph(c) {
            return None;
        }
        let glyph = self.built_in(c, style)?;
        let text = color && vtium_term::cluster_width(&[c]) == 1;
        (glyph.format() == Format::Mask && (text || glyph.width() <= width)).then_some(glyph)
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

    /// A superscript or subscript cell's lines (ADR-1202 D8): the lines
    /// the helper's strike of that script reports, once one is held at
    /// the cell. While it is on its way, the cell's normal lines go
    /// through D4's map, the text band their own box
    /// ([`vtium_font::script::lines::of_face`]), and the strike's lines
    /// replace them when it arrives. Before any set of the frit's at the
    /// cell is held, the built-in font's own script lines in its own cell,
    /// moved down with its text band, as its glyphs are drawn then.
    fn script_decorations(&self, script: Script) -> Decorations {
        if script == Script::Baseline {
            return self.decorations();
        }
        let at = self.at();
        let held = {
            let store = lock(&self.generated);
            match store.script_decorations(at, script) {
                Some(lines) => return lines,
                None => store.holds(at),
            }
        };
        if !held && self.builtin.cell() == self.text() {
            let lines = self.builtin.script_decorations(script);
            return Decorations {
                underline: lines.underline.saturating_add(self.band.above),
                strike: lines.strike.saturating_add(self.band.above),
                overline: lines.overline.saturating_add(self.band.above),
                ..lines
            };
        }
        vtium_font::script::lines::of_face(self, script)
    }
}

/// A [`DeferredFace`]'s built-in font alone, in the face's cell: what the
/// console draws its notices and focus mark with (ADR-1401 D5, D7), so a
/// notice draws in one font on one baseline whatever the helper sent for
/// the characters the terminal shows. It draws a cluster as its first
/// character, and its lines are the built-in font's as the face places
/// them.
struct Builtins<'a>(&'a DeferredFace);

impl Face for Builtins<'_> {
    fn cell(&self) -> Cell {
        self.0.cell
    }

    fn size_step(&self) -> u32 {
        self.0.step
    }

    fn has_glyph(&self, c: char) -> bool {
        self.0.builtin.has_glyph(c)
    }

    fn coverage(&self, range: Range) -> RangeCoverage {
        self.0.builtin.coverage(range)
    }

    fn sprite_policy(&self) -> SpritePolicy {
        self.0.builtin.sprite_policy()
    }

    fn advance(&self, c: char) -> Option<Width> {
        self.0.builtin.advance(c)
    }

    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        self.0.built_in(*cluster.first()?, style)
    }

    fn last_resort(&self, style: Style) -> Option<Glyph> {
        self.0.last_resort(style)
    }

    fn band(&self) -> Band {
        self.0.band
    }

    /// The built-in font's origin, as it draws the cluster's first
    /// character, so the placeholder for a superscript or subscript glyph
    /// on its way is made small as the built-in font's own (ADR-1202 D4,
    /// D11).
    fn origin(&self, cluster: &[char], style: Style) -> Origin {
        self.0.built_in_origin(cluster, style)
    }

    /// The built-in font's strike, so its superscripts and subscripts come
    /// from a smaller multiple of it as its own face's do (ADR-1202 D3).
    fn strike(&self) -> Option<(Cell, u32)> {
        self.0.builtin.strike()
    }

    fn draw_multiple(&self, cluster: &[char], style: Style, multiple: u32) -> Option<Glyph> {
        self.0.builtin.draw_multiple(cluster, style, multiple)
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
            .fresh((spaced, Variant::default()), set, &[(0x41, 1)], 1)
            .unwrap();
        assert!(g.add(fresh).is_some());
        assert!(g.held(spaced, Variant::default(), &['A']));
        assert!(!g.held(AT, Variant::default(), &['A']));
        assert!(g.draw(AT, &['A'], Variant::default()).is_none());
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
            (SpacedCell::plain(cell), Variant::default()),
            set,
            &runs,
            bytes,
        )?;
        store.add(fresh)
    }

    /// The screen showing `chars`, no cluster.
    fn shown(chars: &[char]) -> Shown {
        Shown {
            variant: Variant::default(),
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

    /// The superscript and subscript variants of the regular style.
    const SUPER: Variant = Variant {
        style: Style {
            bold: false,
            italic: false,
        },
        script: Script::Super,
    };
    const SUB: Variant = Variant {
        script: Script::Sub,
        ..SUPER
    };

    /// A set at `cell` of one strike of `variant`'s style and script,
    /// whose every glyph in `chars` is `value` throughout.
    fn filled(cell: Cell, variant: Variant, value: u8, chars: &[char]) -> Arc<Set> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, variant.style).script(variant.script);
        let g = s.glyph(
            Width::Narrow,
            vec![value; (cell.width * cell.height) as usize],
        );
        for &c in chars {
            s.map(c, g);
        }
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    // ADR-1202 D11, ADR-1200 D7 and D22 step 4: while the helper's
    // superscript glyph is on its way, the cell draws the built-in font's
    // superscript glyph, the placeholder every glyph on its way has, and
    // not the frit's normal glyph made small, though the store holds it;
    // the helper's glyph replaces the placeholder when it arrives, in its
    // own script alone, and leaves the normal glyph as it was
    // covers: sgr.script
    #[test]
    fn a_script_cell_draws_the_built_in_script_glyph_until_its_own_arrives() {
        let generated: Shared = Arc::default();
        let normal = filled(CELL, Variant::default(), 255, &['A']);
        let fresh = lock(&generated)
            .fresh((AT, Variant::default()), normal, &[(0x41, 1)], 1)
            .unwrap();
        assert!(lock(&generated).add(fresh).is_some());
        let font = Font::new(face(&generated));
        let unifont = Font::new(Unifont::new(16));
        let draw =
            |f: &Font<DeferredFace>, v| f.drawn_variant(&['A'], Width::Narrow, v).unwrap().glyph;
        let placeholder = unifont
            .drawn_variant(&['A'], Width::Narrow, SUPER)
            .unwrap()
            .glyph;
        assert_eq!(draw(&font, SUPER), placeholder, "the built-in font's");
        let frit_small = draw(&font, Variant::default())
            .scripted(Script::Super, &Origin::default(), Band::NONE)
            .unwrap();
        assert_ne!(
            draw(&font, SUPER),
            frit_small,
            "not the frit's normal glyph made small"
        );
        // the helper's superscript glyph arrives
        let sup = filled(CELL, SUPER, 77, &['A']);
        let fresh = lock(&generated)
            .fresh((AT, SUPER), sup, &[(0x41, 1)], 1)
            .unwrap();
        assert!(lock(&generated).add(fresh).is_some());
        assert!(
            draw(&font, SUPER).pixels().iter().all(|&p| p == 77),
            "the helper's"
        );
        assert!(
            draw(&font, Variant::default())
                .pixels()
                .iter()
                .all(|&p| p == 255)
        );
        let sub = unifont
            .drawn_variant(&['A'], Width::Narrow, SUB)
            .unwrap()
            .glyph;
        assert_eq!(draw(&font, SUB), sub, "the subscript still on its way");
    }

    /// A set at `cell` of one 1-bit strike of `variant`'s style and script
    /// mapping each of `glyphs`' characters to a glyph of rows `rows`, as a
    /// bitmap frit's helper answers.
    fn bitmap_rows(cell: Cell, variant: Variant, glyphs: &[(char, (u32, u32))]) -> Arc<Set> {
        let mut s =
            StrikeBuilder::new(PixelFormat::Bits, cell, variant.style).script(variant.script);
        for &(c, (top, bottom)) in glyphs {
            let rows: Vec<u8> = (0..cell.height)
                .map(|y| {
                    if (top..bottom).contains(&y) {
                        0b0011_1100
                    } else {
                        0
                    }
                })
                .collect();
            let g = s.glyph(Width::Narrow, rows);
            s.map(c, g);
        }
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    /// The rows of `g` with ink.
    fn inked_rows(g: &Glyph) -> Vec<u32> {
        (0..g.height())
            .filter(|&y| (0..g.width()).any(|x| g.pixel(x, y).unwrap()[0] > 0))
            .collect()
    }

    // ADR-1202 D11, D15 and ADR-1200 D22: a superscript cluster that the
    // store holds no glyph for whole, e and its combining acute, runs the
    // lookup chain within its script. A bitmap frit's helper answers no
    // cluster, so the cell draws the frit's superscript e with the frit's
    // superscript acute laid over it; once the frit's superscript é
    // arrives, its NFC form draws. The built-in font draws neither, since
    // the frit has both characters
    // covers: sgr.script
    #[test]
    fn a_superscript_cluster_draws_from_the_frits_superscript_parts() {
        let generated: Shared = Arc::default();
        let font = Font::new(face(&generated));
        let cluster = ['e', '\u{301}'];
        let unifont = Font::new(Unifont::new(16))
            .drawn_variant(&cluster, Width::Narrow, SUPER)
            .unwrap()
            .glyph;
        // the frit's superscript e on rows 6 to 9, its acute on rows 0 to 1
        let parts = bitmap_rows(CELL, SUPER, &[('e', (6, 10)), ('\u{301}', (0, 2))]);
        let fresh = lock(&generated)
            .fresh((AT, SUPER), parts, &[(0x65, 1), (0x301, 1)], 1)
            .unwrap();
        assert!(lock(&generated).add(fresh).is_some());
        let drawn = font
            .drawn_variant(&cluster, Width::Narrow, SUPER)
            .unwrap()
            .glyph;
        assert_eq!(
            inked_rows(&drawn),
            [0, 1, 6, 7, 8, 9],
            "the frit's e and acute"
        );
        assert_ne!(drawn, unifont, "not the built-in font's");
        // the frit's superscript é arrives, on rows 3 to 4
        let nfc = bitmap_rows(CELL, SUPER, &[('\u{e9}', (3, 5))]);
        let fresh = lock(&generated)
            .fresh((AT, SUPER), nfc, &[(0xe9, 1)], 1)
            .unwrap();
        assert!(lock(&generated).add(fresh).is_some());
        let drawn = font
            .drawn_variant(&cluster, Width::Narrow, SUPER)
            .unwrap()
            .glyph;
        assert_eq!(inked_rows(&drawn), [3, 4], "the frit's é");
    }

    // ADR-1202 D11, ADR-1200 D19: compaction writes a superscript set
    // again as superscript strikes, so the glyph it keeps still draws in
    // superscript and never for a normal cell
    #[test]
    fn compaction_keeps_a_glyphs_script() {
        let mut g = Generated::default();
        let set = filled(CELL, SUPER, 77, &['A', 'C']);
        // counted larger than the set written again holds, so dropping one
        // glyph frees enough
        let fresh = g
            .fresh((AT, SUPER), set, &[(0x41, 1), (0x43, 1)], 1 << 20)
            .unwrap();
        assert!(g.add(fresh).is_some());
        g.touch(
            AT,
            &Shown {
                variant: SUPER,
                chars: vec!['A'],
                clusters: Vec::new(),
            },
        );
        // C was shown least recently, so it leaves and A's set is written
        // again without it
        assert_eq!(g.make_room(1), vec![(AT, SUPER, texts(&['C']))]);
        assert!(g.held(AT, SUPER, &['A']) && !g.held(AT, SUPER, &['C']));
        let a = g.draw(AT, &['A'], SUPER).unwrap();
        assert!(a.pixels().iter().all(|&p| p == 77));
        assert!(g.draw(AT, &['A'], Variant::default()).is_none());
    }

    // ADR-1202 D11: the store keys a glyph by its script, so a superscript
    // set's glyph is held and drawn only in that script
    #[test]
    fn the_store_keys_a_glyph_by_its_script() {
        let mut g = Generated::default();
        let fresh = g
            .fresh(
                (AT, SUPER),
                filled(CELL, SUPER, 77, &['A']),
                &[(0x41, 1)],
                1,
            )
            .unwrap();
        assert!(g.add(fresh).is_some());
        assert!(g.held(AT, SUPER, &['A']));
        assert!(!g.held(AT, Variant::default(), &['A']));
        assert!(!g.held(AT, SUB, &['A']));
        assert!(g.draw(AT, &['A'], Variant::default()).is_none());
        assert!(
            g.draw(AT, &['A'], SUPER)
                .is_some_and(|a| a.pixels()[0] == 77)
        );
        // what the screen shows in superscript is the most recently shown,
        // so compaction drops the normal B and keeps the superscript A
        let fresh = g
            .fresh(
                (AT, Variant::default()),
                solid(CELL, &['B']),
                &[(0x42, 1)],
                1,
            )
            .unwrap();
        assert!(g.add(fresh).is_some());
        g.touch(
            AT,
            &Shown {
                variant: SUPER,
                chars: vec!['A'],
                clusters: Vec::new(),
            },
        );
        assert_eq!(
            g.make_room(1),
            vec![(AT, Variant::default(), texts(&['B']))]
        );
        assert!(
            g.draw(AT, &['A'], SUPER)
                .is_some_and(|a| a.pixels()[0] == 77)
        );
    }

    // ADR-1401 D5, D7: the console draws a notice and the focus mark from
    // the built-in fonts in the head's cell, whatever the helper sent. The
    // store holds solid glyphs for `f`, `p`, `s` and the filled circle, as
    // a frit's glyphs for characters the terminal shows; the overlay over
    // that face draws the same pixels as over a face whose store is empty,
    // and the glyphs the store holds never reach it.
    #[test]
    fn a_notice_and_the_mark_draw_from_the_built_in_fonts_alone() {
        use crate::term::{Overlay, OverlayColors};
        use vtium_render::{Canvas, Config as RenderConfig, Mark, Orientation, Renderer};
        let chars = ['f', 'p', 's', '\u{25cf}'];
        let generated = Shared::default();
        let runs = chars.iter().map(|&c| (u32::from(c), 1)).collect();
        assert!(add(&mut lock(&generated), CELL, solid(CELL, &chars), runs, 1).is_some());
        let held = face(&generated);
        assert!(
            all(&held.draw(&['s'], Style::default()).unwrap(), 255),
            "s is held"
        );
        let overlay = Overlay::new(
            Some("59.9 fps 12.345 ms"),
            &[],
            Some(Mark::Focus),
            OverlayColors::default(),
        );
        let drawn = |face: DeferredFace| {
            let face: DisplayFace = Box::new(face);
            let renderer = Renderer::new(Font::new(face), 320, 96, RenderConfig::default());
            let mut canvas = Canvas::new(320, 96, 0);
            let areas = overlay.draw(&renderer, (40, 6), Orientation::Normal, &mut canvas);
            assert_eq!(areas.len(), 2, "the box and the mark");
            canvas
        };
        let built_in = drawn(face(&Shared::default()));
        let over_held = drawn(held);
        assert!(
            built_in.pixels() == over_held.pixels(),
            "the notice and the mark draw from the built-in fonts alone"
        );
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
            let fresh = store
                .fresh((AT, italic.into()), set, &[(0x41, 1)], 1)
                .unwrap();
            assert!(store.add(fresh).is_some());
            assert!(store.held(AT, italic.into(), &['A']));
            assert!(!store.held(AT, Variant::default(), &['A']));
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
            overline: 0,
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

    // ADR-1202 D8: a superscript cell's lines are the D4 map of the
    // cell's normal lines while the frit's superscript strike is on its
    // way, the strike's own lines once it is held, and a superscript set
    // never gives the normal cell its lines
    // covers: sgr.script
    #[test]
    fn a_script_cells_lines_come_from_its_scripts_strike() {
        let generated = Shared::default();
        let cell = Cell {
            width: 10,
            height: 19,
        };
        let f = face_in(&generated, cell);
        let normal = Decorations {
            underline: 15,
            underline_thickness: 1,
            strike: 11,
            strike_thickness: 1,
            overline: 0,
        };
        add(
            &mut lock(&generated),
            cell,
            with_lines(cell, &['A'], normal),
            vec![(0x41, 1)],
            1,
        );
        let mapped =
            vtium_font::script::lines::mapped(normal, Script::Super, (0, 19), 19, Band::NONE);
        assert_eq!(f.script_decorations(Script::Super), mapped);
        assert_ne!(mapped, normal);
        let sup = Decorations {
            underline: 10,
            underline_thickness: 1,
            strike: 6,
            strike_thickness: 1,
            overline: 0,
        };
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default())
            .script(Script::Super)
            .decorations(sup);
        let g = s.glyph(Width::Narrow, vec![255; 190]);
        s.map('A', g);
        let set =
            Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap());
        let variant = Variant {
            style: Style::default(),
            script: Script::Super,
        };
        let mut store = lock(&generated);
        let fresh = store
            .fresh((SpacedCell::plain(cell), variant), set, &[(0x41, 1)], 1)
            .unwrap();
        store.add(fresh);
        drop(store);
        assert_eq!(f.script_decorations(Script::Super), sup);
        assert_eq!(f.decorations(), normal, "the normal cell's lines stay");
        // a store holding only the superscript set gives the normal cell
        // the lines it had before any set came
        let alone = Shared::default();
        let g = face_in(&alone, cell);
        let before = g.decorations();
        let set = lock(&generated)
            .entries
            .iter()
            .find(|e| e.variant.script == Script::Super)
            .map(|e| Arc::clone(e.face.set()))
            .unwrap();
        let mut store = lock(&alone);
        let fresh = store
            .fresh((SpacedCell::plain(cell), variant), set, &[(0x41, 1)], 1)
            .unwrap();
        store.add(fresh);
        drop(store);
        assert_eq!(g.decorations(), before);
        assert_eq!(g.script_decorations(Script::Super), sup);
        assert_eq!(
            f.script_decorations(Script::Sub),
            vtium_font::script::lines::mapped(normal, Script::Sub, (0, 19), 19, Band::NONE)
        );
    }

    // ADR-1202 D8: before any set of the frit's is held, a script cell's
    // lines are the built-in font's own, as its glyphs are: unifont at 2x
    // takes the lines of unifont at 1x placed with the strike
    // covers: sgr.script
    #[test]
    fn before_the_frits_glyphs_a_script_cells_lines_are_the_built_in_fonts() {
        let generated = Shared::default();
        let cell = Cell {
            width: 16,
            height: 32,
        };
        let f = DeferredFace::new(Box::new(Unifont::new(32)), cell, 16, Arc::clone(&generated));
        let unifont = Unifont::new(32);
        for script in [Script::Super, Script::Sub] {
            assert_eq!(
                f.script_decorations(script),
                unifont.script_decorations(script)
            );
        }
        assert_eq!(f.script_decorations(Script::Sub).overline, 16);
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
        assert_eq!(
            g.make_room(1),
            vec![(AT, Variant::default(), texts(&['B']))]
        );
        assert_eq!(g.bytes(), 10);
        assert_eq!(
            g.make_room(1),
            vec![(AT, Variant::default(), texts(&['A']))]
        );
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
        assert_eq!(gone, vec![(AT, Variant::default(), texts(&['B', 'E']))]);
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
        assert_eq!(
            g.make_room(1),
            vec![(AT, Variant::default(), texts(&['E']))]
        );
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

    /// A set at [`CELL`] in `format`, 1-bit or R8, where `A` is solid,
    /// `n` is a body inside its cell, and `o` is the same body with a horn
    /// whose ink reaches one column into the cell after it (ADR-1306 D3),
    /// as Iosevka Term draws U+01A1 at 15x38: its ink box is the cell and
    /// that column, the horn's pixel in it gray in R8.
    fn horned(format: PixelFormat) -> Arc<Set> {
        let (w, h) = (CELL.width as usize, CELL.height as usize);
        let body = |x: usize, y: usize| (2..6).contains(&x) && (6..14).contains(&y);
        let horn = if format == PixelFormat::Bits {
            255
        } else {
            128
        };
        let own: Vec<u8> = (0..h)
            .flat_map(|y| (0..w).map(move |x| if body(x, y) { 255 } else { 0 }))
            .collect();
        let boxed: Vec<u8> = (0..h)
            .flat_map(|y| {
                (0..=w).map(move |x| match x {
                    _ if body(x, y) => 255,
                    8 if (6..8).contains(&y) => horn,
                    _ => 0,
                })
            })
            .collect();
        // a 1-bit strike holds its rows packed, the first pixel the high bit
        let bytes = |px: &[u8], width: usize| -> Vec<u8> {
            if format != PixelFormat::Bits {
                return px.to_vec();
            }
            let row = width.div_ceil(8);
            let mut out = vec![0u8; row * h];
            for (y, line) in px.chunks(width).enumerate() {
                for (x, _) in line.iter().enumerate().filter(|&(_, &p)| p != 0) {
                    out[y * row + x / 8] |= 0x80 >> (x % 8);
                }
            }
            out
        };
        let mut s = StrikeBuilder::new(format, CELL, Style::default());
        let solid = s.glyph(Width::Narrow, bytes(&vec![255; w * h], w));
        s.map('A', solid);
        let plain = s.glyph(Width::Narrow, bytes(&own, w));
        s.map('n', plain);
        let horned = s.glyph(Width::Narrow, bytes(&own, w));
        s.ink(
            horned,
            (0, 0),
            (CELL.width + 1, CELL.height),
            bytes(&boxed, w + 1),
        );
        s.map('o', horned);
        Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap())
    }

    /// `g`'s ink box past its cell, as its place, size and pixels.
    fn ink_of(g: &Glyph) -> Option<InkBytes> {
        g.ink().map(|i| ink_bytes(i, i.pixels().to_vec()))
    }

    const HORNED_RUNS: [(u32, u32); 3] = [(0x41, 1), (0x6e, 1), (0x6f, 1)];

    // ADR-1200 D18, ADR-1306 D3: a helper's answer that carries a glyph the
    // store already holds is written again without it, and the glyphs it
    // keeps keep their ink past the cell. The store draws the same glyph
    // for `o` whether its set came first or after a set that brought `A`,
    // so the frame does not depend on the order the answers arrived in.
    #[test]
    fn an_answer_written_again_without_held_glyphs_keeps_their_ink() {
        for format in [PixelFormat::R8, PixelFormat::Bits] {
            let mut first = Generated::default();
            add(&mut first, CELL, horned(format), HORNED_RUNS.to_vec(), 1);
            let mut after = Generated::default();
            add(&mut after, CELL, solid(CELL, &['A']), vec![(0x41, 1)], 1);
            let fresh = after
                .fresh((AT, Variant::default()), horned(format), &HORNED_RUNS, 1)
                .unwrap();
            assert_eq!(
                fresh.texts().cloned().collect::<Vec<_>>(),
                texts(&['n', 'o']),
                "{format:?}: the answer is written again without A"
            );
            assert!(after.add(fresh).is_some());
            let whole = first.draw(AT, &['o'], Variant::default()).unwrap();
            let again = after.draw(AT, &['o'], Variant::default()).unwrap();
            assert!(
                ink_of(&whole).is_some(),
                "{format:?}: the helper's o has a horn"
            );
            assert_eq!(
                ink_of(&again),
                ink_of(&whole),
                "{format:?}: o keeps its horn"
            );
            assert_eq!(
                again.pixels(),
                whole.pixels(),
                "{format:?}: o inside its cell"
            );
            // n shares o's pixels inside the cell, and has no ink past it
            let n = after.draw(AT, &['n'], Variant::default()).unwrap();
            assert_eq!(ink_of(&n), None, "{format:?}: n takes no horn from o");
            assert_eq!(n.pixels(), whole.pixels(), "{format:?}: n inside its cell");
        }
    }

    // ADR-1200 D19, ADR-1306 D3: compaction writes a set again without the
    // glyphs least recently shown, and the glyphs it keeps keep their ink
    #[test]
    fn compaction_keeps_the_ink_of_the_glyphs_it_keeps() {
        let mut g = Generated::default();
        add(
            &mut g,
            CELL,
            horned(PixelFormat::R8),
            HORNED_RUNS.to_vec(),
            3,
        );
        let whole = ink_of(&g.draw(AT, &['o'], Variant::default()).unwrap());
        g.touch(
            AT,
            &Shown {
                variant: Variant::default(),
                chars: vec!['o'],
                clusters: Vec::new(),
            },
        );
        let left = g.make_room(1);
        assert!(
            left.iter().any(|(_, _, gone)| gone.contains(&vec!['A'])),
            "A leaves the store: {left:?}"
        );
        assert!(g.held(AT, Variant::default(), &['o']));
        assert!(whole.is_some());
        assert_eq!(
            ink_of(&g.draw(AT, &['o'], Variant::default()).unwrap()),
            whole
        );
    }

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
        assert!(g.held(AT, Variant::default(), &['e']) && !g.held(AT, Variant::default(), &ACUTE));
        let fresh = g
            .fresh(
                (AT, Variant::default()),
                with_clusters(&['e'], &[(&ACUTE, 7)]),
                &[(0x65, 1)],
                1,
            )
            .unwrap();
        assert_eq!(fresh.texts().cloned().collect::<Vec<_>>(), [ACUTE.to_vec()]);
        assert!(g.add(fresh).is_some());
        let grave = ['e', '\u{300}'];
        assert!(g.held(AT, Variant::default(), &ACUTE) && !g.held(AT, Variant::default(), &grave));
        let fresh = g
            .fresh(
                (AT, Variant::default()),
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

    const LAM: char = '\u{644}';
    const SHADDA: char = '\u{651}';
    const SUPERSCRIPT_ALEF: char = '\u{670}';

    // ADR-1200 D22 step 3, ADR-1306 D1: a cluster the store does not hold
    // whole, because one of its marks, superscript alef, has no glyph in
    // the font set, is built from its parts, and keeps each part's ink past
    // the cell. The store holds lam, inside its cell, and shadda, which the
    // font sets 3 rows above the row, in its ink box; superscript alef
    // comes from the built-in font, inside the cell.
    #[test]
    fn a_cluster_built_from_its_parts_keeps_their_ink_above_the_row() {
        let generated = Shared::default();
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let lam = s.glyph(
            Width::Narrow,
            drawn(CELL, &["", "", "", "...#....", "...#....", "...#...."]),
        );
        s.map(LAM, lam);
        let shadda = s.glyph(Width::Narrow, vec![0; 128]);
        // the box: 4 rows above the cell, then the cell, 8 columns across
        let mut boxed = vec![0u8; 8 * 20];
        for y in 1..3 {
            boxed[y * 8 + 2..y * 8 + 6].fill(255);
        }
        s.ink(shadda, (0, -4), (8, 20), boxed);
        s.map(SHADDA, shadda);
        let set =
            Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap());
        let runs = vec![(u32::from(LAM), 1), (u32::from(SHADDA), 1)];
        add(&mut lock(&generated), CELL, set, runs, 1);
        let f = face(&generated);
        let regular = Style::default();
        // shadda alone keeps its ink above the row
        let alone = f.draw(&[LAM, SHADDA], regular).unwrap();
        let built = f.draw(&[LAM, SHADDA, SUPERSCRIPT_ALEF], regular).unwrap();
        // a pixel of `g`'s ink at column `x`, row `y` from its cell's top left
        let at = |g: &Glyph, x: i32, y: i32| {
            let ink = g.ink().unwrap();
            let (bx, by) = (x - ink.left(), y - ink.top());
            let width = i32::try_from(ink.width()).unwrap();
            ink.pixels()[usize::try_from(by * width + bx).unwrap()]
        };
        for g in [&alone, &built] {
            // the shadda's ink above the row
            let ink = g.ink().unwrap();
            // the box holds the cell and the shadda's two rows above it
            assert_eq!(
                (ink.left(), ink.top(), ink.width(), ink.height()),
                (0, -3, 8, 19)
            );
            assert!((2..6).all(|x| at(g, x, -3) == 255 && at(g, x, -2) == 255));
            assert_eq!(at(g, 3, 3), 255, "the lam inside the cell");
        }
        // the built-in font's superscript alef lies inside the cell
        let alef = Unifont::new(16).draw(&[SUPERSCRIPT_ALEF], regular).unwrap();
        for (y, row) in (0..).zip(alef.rows()) {
            for (x, &p) in (0..).zip(row) {
                if p > 0 {
                    assert!(at(&built, x, y) > 0, "the alef at {x},{y}");
                }
            }
        }
    }

    // ADR-1200 D22 step 3: where the store holds the cluster without the
    // mark the font set lacks, as the helper shaped it whole, that glyph
    // draws, its shadda 3 rows above the row where the font puts it, and
    // the missing superscript alef from the built-in font lies over it
    #[test]
    fn a_cluster_draws_the_part_the_font_shaped_and_the_missing_mark_over_it() {
        let generated = Shared::default();
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let lam = s.glyph(
            Width::Narrow,
            drawn(CELL, &["", "", "", "...#....", "...#...."]),
        );
        s.map(LAM, lam);
        let shaped = s.glyph(
            Width::Narrow,
            drawn(CELL, &["", "", "", "...#....", "...#...."]),
        );
        let mut boxed = vec![0u8; 8 * 19];
        boxed[2..6].fill(255);
        boxed[8 + 2..8 + 6].fill(255);
        boxed[(3 + 3) * 8 + 3] = 255;
        boxed[(3 + 4) * 8 + 3] = 255;
        s.ink(shaped, (0, -3), (8, 19), boxed);
        s.cluster(&[LAM, SHADDA], shaped);
        let set =
            Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap());
        add(
            &mut lock(&generated),
            CELL,
            set,
            vec![(u32::from(LAM), 1)],
            1,
        );
        let f = face(&generated);
        let regular = Style::default();
        let built = f.draw(&[LAM, SHADDA, SUPERSCRIPT_ALEF], regular).unwrap();
        // the shaped part's shadda keeps its two rows above the row
        let ink = built.ink().unwrap();
        assert_eq!(
            (ink.left(), ink.top(), ink.width(), ink.height()),
            (0, -3, 8, 19)
        );
        assert!(ink.pixels()[2..6].iter().all(|&p| p == 255));
        // and the built-in font's superscript alef lies over it in the cell
        let alef = Unifont::new(16).draw(&[SUPERSCRIPT_ALEF], regular).unwrap();
        for (y, row) in alef.rows().enumerate() {
            for (x, &p) in row.iter().enumerate() {
                if p > 0 {
                    assert!(ink.pixels()[(y + 3) * 8 + x] > 0, "the alef at {x},{y}");
                }
            }
        }
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

    // vtium/vtium#159: the face measures a character by the store's
    // glyph for it, else the built-in font's, without drawing either: the
    // store holds a wide `W`, which the built-in font draws narrow, and the
    // set's chunk stays undecoded. The built-in fonts alone, which the
    // console draws its notices with, measure `W` by the built-in font's
    #[test]
    fn the_face_measures_by_the_store_else_the_built_in_font() {
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default()).compress(true);
        let g = s.glyph(
            Width::Wide,
            vec![255; (2 * CELL.width * CELL.height) as usize],
        );
        s.map('W', g);
        let set =
            Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap());
        let generated = Shared::default();
        add(
            &mut lock(&generated),
            CELL,
            Arc::clone(&set),
            vec![(0x57, 1)],
            1,
        );
        let f = face(&generated);
        assert_eq!(f.advance('W'), Some(Width::Wide), "the store's glyph");
        assert_eq!(f.advance('\u{4e00}'), Some(Width::Wide), "unifont's glyph");
        assert_eq!(f.advance('A'), Some(Width::Narrow), "unifont's glyph");
        assert_eq!(f.advance('\u{10ffff}'), None);
        assert_eq!(set.loaded_chunks(), 0, "measuring drew the store's glyph");
        let builtins = f.builtins().unwrap();
        assert_eq!(builtins.advance('W'), Some(Width::Narrow));
        assert_eq!(builtins.advance('\u{4e00}'), Some(Width::Wide));
        assert_eq!(builtins.advance('\u{10ffff}'), None);
        for c in ['W', '\u{4e00}', 'A', '\u{10ffff}'] {
            let drawn = |face: &dyn Face| face.draw(&[c], Style::default()).map(|g| g.cells());
            assert_eq!(f.advance(c), drawn(&f), "{c:?}");
            assert_eq!(builtins.advance(c), drawn(&*builtins), "{c:?}");
        }
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

    /// The rows of `g`'s ink box, as `#` and `.`.
    fn boxed_picture(g: &Glyph) -> Vec<String> {
        let ink = g.ink().unwrap();
        ink.pixels()
            .chunks_exact(ink.width() as usize)
            .map(|r| r.iter().map(|&p| if p > 0 { '#' } else { '.' }).collect())
            .collect()
    }

    // covers: text.overhang
    // #235, ADR-1302 D14: a bitmap letter and its spacing marks that need
    // at most 2 pixels more than the letter's cells keep their size, so no
    // ink is cut: their 10 columns are centred on the 8-pixel cell and
    // reach 1 pixel into the cell on each side. The letter's headline
    // starts in the cell before, and the vowel sign's stem stands in the
    // cell after, a blank column inside the cell between it and the
    // letter's stem. The cluster stays one bit
    #[test]
    fn a_letter_and_spacing_marks_a_little_wider_than_the_cells_keep_their_size() {
        let wide_ka: [&str; 13] = [
            "........", "........", "........", "########", "......#.", "......#.", ".####.#.",
            "#....##.", "#....##.", ".####.#.", "......#.", "......#.", "......#.",
        ];
        let f = face_with(&[(KA, &wide_ka), (AA, &AA_ROWS)]);
        let both = Font::new(f)
            .glyph(&[KA, AA], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!((both.width(), both.cells()), (8, Width::Narrow));
        let ink = both.ink().unwrap();
        assert_eq!((ink.left(), ink.width()), (-1, 10));
        assert!(ink.pixels().iter().all(|&p| p == 0 || p == 255));
        let rows = boxed_picture(&both);
        assert_eq!(rows[3], "##########");
        for row in &rows[4..13] {
            assert_eq!(&row[6..], "#..#", "{rows:?}");
        }
    }

    // covers: text.overhang
    // ADR-1302 D14: a bitmap letter and its spacing marks wider than the
    // cell and 2 pixels more are squeezed by OR pooling to the cell and 2
    // pixels, centred: ka with the vowel sign aa twice takes 12 columns of
    // ink, squeezed to 10 in an 8-pixel cell, one bit. The glyph
    // path squeezes the same run of ink to the same pixels, as one piece
    // of code squeezes both
    #[test]
    fn a_letter_and_spacing_marks_much_wider_than_the_cells_are_pooled() {
        let wide_ka: [&str; 13] = [
            "........", "........", "........", "########", "......#.", "......#.", ".####.#.",
            "#....##.", "#....##.", ".####.#.", "......#.", "......#.", "......#.",
        ];
        let f = face_with(&[(KA, &wide_ka), (AA, &AA_ROWS)]);
        let run = f.draw(&[KA, AA, AA], Style::default()).unwrap();
        let both = Font::new(f)
            .glyph(&[KA, AA, AA], Width::Narrow, Style::default())
            .unwrap();
        let ink = both.ink().unwrap();
        assert!(run.width() > 10, "the run is {} wide", run.width());
        assert_eq!((ink.left(), ink.width()), (-1, 10));
        assert!(
            both.pixels()
                .iter()
                .chain(ink.pixels())
                .all(|&p| p == 0 || p == 255)
        );
        assert_eq!(Some(both), run.fit(8, Width::Narrow));
    }

    // ADR-1302 D14: an outline letter and its spacing marks wider than the
    // cells scale sideways together to fill them, each pixel taking the
    // coverage of what lies under it. The vowel sign's stem stands in the last
    // column, which takes eight tenths of the stem's column, 255 * 8 / 10
    // rounded down, and no ink past the cell
    #[test]
    fn an_outline_letter_and_spacing_marks_scale_by_coverage() {
        let wide_ka: [&str; 13] = [
            "........", "........", "........", "########", "......#.", "......#.", ".####.#.",
            "#....##.", "#....##.", ".####.#.", "......#.", "......#.", "......#.",
        ];
        let mut s = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        // one pixel of partial coverage makes the letter an outline glyph
        let mut gray = drawn(CELL, &wide_ka);
        gray[0] = 9;
        let g = s.glyph(Width::Narrow, gray);
        s.map(KA, g);
        let a = s.glyph(Width::Narrow, drawn(CELL, &AA_ROWS));
        s.map(AA, a);
        let set =
            Arc::new(Set::from_bytes(SetBuilder::new("t").strike(s).write().unwrap()).unwrap());
        let generated = Shared::default();
        let runs = vec![(u32::from(KA), 1), (u32::from(AA), 1)];
        add(&mut lock(&generated), CELL, set, runs, 1);
        let both = Font::new(face(&generated))
            .glyph(&[KA, AA], Width::Narrow, Style::default())
            .unwrap();
        assert!(both.ink().is_none());
        assert!(
            (4..13).all(|y| both.pixel(7, y).unwrap()[0] == 204),
            "{:?}",
            picture(&both)
        );
    }

    // ADR-1302 D14: with no glyph in the store, the console draws ka
    // with aa (का) from the built-in GNU Unifont, which draws both 16
    // pixels wide while the terminal gives the cluster one 8-pixel cell.
    // The pair is squeezed into the cell, two columns into one, so the
    // vowel sign's stem, in column 14 of Unifont's 16, stands in the
    // cell's last column on rows 4 to 12, and ka's own stem, in column 8,
    // in column 4 left of it
    #[test]
    fn ka_with_aa_from_the_built_in_font_keeps_the_stem_in_one_cell() {
        let font = Font::new(face(&Shared::default()));
        let both = font
            .glyph(&[KA, AA], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!((both.width(), both.cells()), (8, Width::Narrow));
        let ink = |x: u32, y: u32| both.pixel(x, y).unwrap()[0] > 0;
        assert!((4..13).all(|y| ink(7, y)), "{:?}", picture(&both));
        assert!((4..13).all(|y| ink(4, y)), "{:?}", picture(&both));
        assert!((0..8).all(|x| ink(x, 3)), "{:?}", picture(&both));
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

    /// A face whose store holds `c` as a color glyph one cell wide, every
    /// pixel opaque red, and `wide` as a mask two cells wide, every pixel
    /// inked.
    fn color_and_wide(c: char, wide: char) -> DeferredFace {
        let mut color = StrikeBuilder::new(PixelFormat::Rgba, CELL, Style::default());
        let red = [255u8, 0, 0, 255].repeat((CELL.width * CELL.height) as usize);
        let g = color.glyph(Width::Narrow, red);
        color.map(c, g);
        let mut mask = StrikeBuilder::new(PixelFormat::R8, CELL, Style::default());
        let g = mask.glyph(
            Width::Wide,
            vec![255; (CELL.width * 2 * CELL.height) as usize],
        );
        mask.map(wide, g);
        let bytes = SetBuilder::new("t")
            .strike(color)
            .strike(mask)
            .write()
            .unwrap();
        let set = Arc::new(Set::from_bytes(bytes).unwrap());
        let generated = Shared::default();
        let runs = vec![(u32::from(c), 1), (u32::from(wide), 1)];
        add(&mut lock(&generated), CELL, set, runs, 1);
        face(&generated)
    }

    // covers: text.overhang
    // ADR-1302 D14, step 2 of the fallback order: ❤ (U+2764) presents as
    // text by default (Emoji_Presentation=No) and takes one cell, so the
    // built-in font's text glyph draws it before the font set's color
    // glyph, which would have to draw in one cell. The text glyph is
    // GNU Unifont's heart, 16 pixels wide, squeezed 2:1 into the cell
    #[test]
    fn a_text_glyph_comes_before_a_color_glyph_in_one_cell() {
        let heart = '\u{2764}';
        let font = Font::new(color_and_wide(heart, 'A'));
        let drawn = font
            .glyph(&[heart], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!(drawn.format(), Format::Mask);
        let unifont = Font::new(Unifont::new(16))
            .glyph(&[heart], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!(drawn, unifont);
        // an emoji that presents as emoji by default, drawn in one cell as
        // wcwidth gives it, keeps the font set's color glyph
        let grin = '\u{1f600}';
        let font = Font::new(color_and_wide(grin, 'A'));
        let drawn = font
            .glyph(&[grin], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!(drawn.format(), Format::Rgba);
    }

    // covers: text.overhang
    // ADR-1302 D14, step 3 of the fallback order: a built-in glyph that
    // fits the cell comes before a squeeze of the font set's own glyph
    // wider than its cells. The set's A is two cells wide; the built-in
    // font's A, 8 pixels wide, draws instead
    #[test]
    fn a_built_in_glyph_that_fits_comes_before_a_squeeze() {
        let font = Font::new(color_and_wide('\u{2764}', 'A'));
        let drawn = font.glyph(&['A'], Width::Narrow, Style::default()).unwrap();
        let unifont = Font::new(Unifont::new(16))
            .glyph(&['A'], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!(drawn, unifont);
    }

    // #235, ADR-1302 D14: the font has placed a mark only in a box as wide
    // as its letter's. Unifont's vowel sign aa, 16 pixels wide, on a
    // frit's letter 8 pixels wide, stands after the letter instead of being
    // laid on it across two cells. The two take 9 columns, a pixel more
    // than the cell, so they keep their size, and the odd column, the top
    // stroke's last, goes past the cell's right edge
    #[test]
    fn a_mark_in_a_box_wider_than_its_letters_stands_beside_it() {
        let f = face_with(&[(KA, &KA_ROWS)]);
        let sign = Unifont::new(16).draw(&[AA], Style::default()).unwrap();
        assert_eq!(sign.width(), 16, "unifont's aa is two cells");
        let both = Font::new(f)
            .glyph(&[KA, AA], Width::Narrow, Style::default())
            .unwrap();
        assert_eq!((both.width(), both.cells()), (8, Width::Narrow));
        let ink = both.ink().unwrap();
        assert_eq!((ink.left(), ink.width()), (0, 9));
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

    /// U+10FFFD, which the built-in font lacks, and a skin tone modifier
    /// after it: a cluster a set holds while it does not hold the base
    /// alone.
    const TONED: [char; 2] = ['\u{10fffd}', '\u{1f3fb}'];

    /// A terminal of four cells by one whose first cell shows U+10FFFD
    /// alone, and a store whose only set holds [`TONED`]: the face says it
    /// has U+10FFFD, through the cluster, yet draws nothing for it alone,
    /// so the font draws the built-in font's U+25A1 for it.
    fn lone_base_of_a_held_cluster() -> (Shared, vtium_term::Terminal) {
        let generated = Shared::default();
        add(
            &mut lock(&generated),
            CELL,
            with_clusters(&[], &[(&TONED, 7)]),
            vec![],
            1,
        );
        let f = face(&generated);
        assert!(f.has_glyph(TONED[0]));
        assert!(f.draw(&TONED[..1], Style::default()).is_none());
        let mut term = vtium_term::Terminal::new(4, 1, 0);
        assert_eq!(term.input("\u{10fffd}".as_bytes()), 4);
        (generated, term)
    }

    /// Adds to `generated` a set that holds a solid U+25A1, as the helper
    /// does once a later set brings one.
    fn bring_u25a1(generated: &Shared) {
        add(
            &mut lock(generated),
            CELL,
            solid(CELL, &['\u{25a1}']),
            vec![(0x25a1, 1)],
            1,
        );
    }

    // vtium/vtium#320: the cell that shows U+10FFFD alone draws the missing
    // glyph though the face says it has U+10FFFD. A later set that brings
    // U+25A1 replaces that drawing on the frame after it arrives, as a
    // renderer made after the arrival draws it.
    #[test]
    fn a_later_u25a1_redraws_the_lone_base_of_a_held_cluster() {
        use vtium_render::{Canvas, Config as RenderConfig, DrawState, Renderer};
        let (generated, mut term) = lone_base_of_a_held_cluster();
        let (w, h) = (CELL.width * 4, CELL.height);
        let state = DrawState::default();
        let mut r = Renderer::new(Font::new(face(&generated)), w, h, RenderConfig::default());
        let mut canvas = Canvas::new(w, h, 0);
        r.draw(
            &term.screen().frame(term.palette(), vtium_term::Age::NEVER),
            &mut canvas,
            0,
            &state,
        );
        let before = canvas.clone();
        let since = term.screen_mut().checkpoint();
        bring_u25a1(&generated);
        r.glyphs_changed(&['\u{25a1}']);
        assert!(r.redraws_missing(), "the arrival owes no frame");
        r.draw(
            &term.screen().frame(term.palette(), since),
            &mut canvas,
            1,
            &state,
        );
        let mut fresh = Canvas::new(w, h, 0);
        Renderer::new(Font::new(face(&generated)), w, h, RenderConfig::default()).draw(
            &term.screen().frame(term.palette(), vtium_term::Age::NEVER),
            &mut fresh,
            0,
            &state,
        );
        assert!(before != fresh, "the set's U+25A1 moves no pixel");
        assert!(
            canvas == fresh,
            "U+10FFFD still shows the built-in U+25A1 drawn before the set arrived"
        );
    }

    // vtium/vtium#320 on the GPU path: the frame after the arrival uploads
    // the lone U+10FFFD's new drawing
    #[cfg(feature = "gpu")]
    #[test]
    fn on_the_gpu_path_a_later_u25a1_redraws_the_lone_base_of_a_held_cluster() {
        use vtium_render::gpu::{Call, Config as GpuConfig, Gl, Pipeline, Size, StandIn};
        use vtium_render::{Config as RenderConfig, DrawState, Renderer};
        let (generated, mut term) = lone_base_of_a_held_cluster();
        let display = Size::new(CELL.width * 4, CELL.height);
        let mut gl = StandIn::new();
        let mut pipeline = Pipeline::new(&mut gl, display, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&display).unwrap();
        let mut r = Renderer::new(
            Font::new(face(&generated)),
            display.width,
            display.height,
            RenderConfig::default(),
        );
        // the textures each frame fills: the glyphs it draws anew
        let mut frame =
            |r: &mut Renderer<DeferredFace>, term: &vtium_term::Terminal, since, age| {
                gl.clear_calls();
                let frame = term.screen().frame(term.palette(), since);
                r.draw_gpu(
                    &mut gl,
                    &mut pipeline,
                    &frame,
                    &buffer,
                    age,
                    &DrawState::default(),
                    &[],
                )
                .unwrap();
                gl.calls()
                    .iter()
                    .filter(|c| matches!(c, Call::FillTexture(_)))
                    .count()
            };
        frame(&mut r, &term, vtium_term::Age::NEVER, 0);
        let since = term.screen_mut().checkpoint();
        bring_u25a1(&generated);
        r.glyphs_changed(&['\u{25a1}']);
        assert_eq!(
            frame(&mut r, &term, since, 1),
            1,
            "U+10FFFD drawn from the set"
        );
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
            variant: Variant::default(),
            chars: vec!['e'],
            clusters: vec![ACUTE.to_vec()],
        };
        lock(&generated).touch(AT, &shown);
        assert_eq!(
            lock(&generated).make_room(1),
            vec![(AT, Variant::default(), vec![grave.to_vec()])]
        );
        let f = face(&generated);
        assert!(all(&f.draw(&ACUTE, Style::default()).unwrap(), 7));
        assert!(!lock(&generated).held(AT, Variant::default(), &grave));
        // a frame of the held cluster and a new one adds the new one alone
        let fresh = lock(&generated)
            .fresh(
                (AT, Variant::default()),
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
