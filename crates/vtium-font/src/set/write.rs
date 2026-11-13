//! The set writer's core: a set described by plain numbers in, its bytes
//! out (ADR-1200 D2). [`SetBuilder`](super::SetBuilder) puts vtium-font's
//! types on it, and `build.rs` includes this file to pack the built-in
//! sets.
//!
//! The bytes depend only on what is given: strikes are sorted, each
//! strike's map is sorted, glyphs are numbered in the order the map first
//! reaches them, and every chunk is deflated at one fixed level, so the
//! same set gives the same bytes on every host built with the same
//! features: the console's writer, without the `deflate` feature, stores
//! raw a compress=yes strike that vtium-config's deflates.

use crate::bounds::{
    CELL_SIDE_MAX, CLUSTER_SCALARS_MAX, SET_BYTES_MAX, SET_CHUNK_BYTES, SET_CHUNKS_MAX,
    SET_CLUSTERS_MAX, SET_GLYPHS_MAX, SET_INKS_MAX, SET_META_BYTES_MAX, SET_RUNS_MAX,
    SET_STRIKES_MAX,
};
use crate::set::format::{
    CHUNK_DEFLATED, CHUNK_RECORD, CLUS, CLUSTER_RECORD, CMAP, COVERAGE_RECORD, FORMAT_BITS,
    FORMAT_R8, FORMAT_RGBA, GLYF, INK_CLUSTER, INK_GLYPH, INK_RECORD, INKX, InkRect, MAGIC, MAJOR,
    META, MINOR, POLY, PRELUDE_BYTES, RUN_RECORD, RUN_WIDE, SCRIPT_SUB, SECTION_BYTES, SOURCE_AUTO,
    STRIKE_RECORD, STRK, glyph_bytes,
};
use crate::set::pack::deflate;
use std::collections::BTreeMap;

use crate::sprite::spans;

/// One strike as the writer takes it.
pub(crate) struct RawStrike {
    /// Style bits: `STYLE_BOLD`, `STYLE_ITALIC`.
    pub style: u8,
    /// `FORMAT_BITS`, `FORMAT_R8` or `FORMAT_RGBA`.
    pub format: u8,
    /// `SCRIPT_BASELINE`, `SCRIPT_SUPER` or `SCRIPT_SUB` (ADR-1202 D11).
    pub script: u16,
    pub cell_width: u16,
    pub cell_height: u16,
    pub baseline: u16,
    pub underline: u16,
    pub underline_thickness: u16,
    pub strike: u16,
    pub strike_thickness: u16,
    /// The overline's top row, as thick as the underline (ADR-1202 D8).
    pub overline: u16,
    /// Whether to store a chunk deflated where that makes it smaller.
    pub compress: bool,
    /// Each glyph: whether it is two cells wide, and its bytes in the
    /// strike's format.
    pub glyphs: Vec<(bool, Vec<u8>)>,
    /// Each codepoint the strike maps and the glyph it draws, in any order.
    pub map: Vec<(u32, u32)>,
    /// Each cluster the strike maps, its scalar values, and the glyph it
    /// draws, in any order.
    pub clusters: Vec<(Vec<u32>, u32)>,
    /// The ink box of each glyph whose ink passes its cells, by the
    /// glyph's number in `glyphs`, in any order (`INKX`).
    pub inks: Vec<(u32, RawInk)>,
}

/// A glyph's ink box: its first column's offset from its first cell's
/// left edge and its first row's offset from its row's top, each 0 or
/// negative, its width and height, and its bytes in the strike's format.
pub(crate) struct RawInk {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

/// A set as the writer takes it.
pub(crate) struct RawSet {
    /// The provenance, in order; each key 1 or more characters with no `=`
    /// or line break, each value with no line break.
    pub meta: Vec<(String, String)>,
    /// Each sprite range's source, in `Range::ALL`'s order.
    pub policy: [u8; 6],
    pub strikes: Vec<RawStrike>,
}

/// Why a set could not be written.
pub(crate) type WriteError = &'static str;

/// A run of codepoints whose glyphs are consecutive.
struct Run {
    first: u32,
    len: u32,
    glyph: u32,
    wide: bool,
}

/// A strike laid out: its runs, its glyph bytes in index order, and each
/// range's coverage.
struct Laid {
    glyph_count: u32,
    /// Each of the strike's glyphs' number as written, `None` for one the
    /// map never reaches.
    renumbered: Vec<Option<u32>>,
    runs: Vec<Run>,
    stream: Vec<Vec<u8>>,
    coverage: [(u32, u32); 6],
}

fn u32_of(n: usize) -> Result<u32, WriteError> {
    u32::try_from(n).map_err(|_| "a count past u32")
}

fn u64_of(n: usize) -> Result<u64, WriteError> {
    u64::try_from(n).map_err(|_| "a count past u64")
}

fn checked(a: u32, b: u32) -> Result<u32, WriteError> {
    a.checked_add(b).ok_or("a count past u32")
}

/// The strike's glyphs renumbered in the order its sorted map first
/// reaches them, its runs, and its coverage of each sprite range. Glyphs
/// the map never reaches are dropped. A run either brings in new glyphs or
/// reuses earlier ones whole, never both, as the reader requires.
fn lay_out(strike: &RawStrike) -> Result<Laid, WriteError> {
    let mut map = strike.map.clone();
    map.sort_unstable();
    if map.windows(2).any(|w| matches!(w, [a, b] if a.0 == b.0)) {
        return Err("a codepoint mapped twice");
    }
    let mut renumbered: Vec<Option<u32>> = vec![None; strike.glyphs.len()];
    let mut stream: Vec<Vec<u8>> = Vec::new();
    let mut runs: Vec<Run> = Vec::new();
    // whether the last run brought in new glyphs
    let mut last_new = false;
    for &(cp, old) in &map {
        if char::from_u32(cp).is_none() {
            return Err("a codepoint that is not a character");
        }
        let old_index = usize::try_from(old).map_err(|_| "a glyph index past usize")?;
        let slot = renumbered
            .get_mut(old_index)
            .ok_or("a codepoint mapped to a glyph the strike lacks")?;
        let &(wide, ref bytes) = strike
            .glyphs
            .get(old_index)
            .ok_or("a codepoint mapped to a glyph the strike lacks")?;
        let (index, new) = match *slot {
            Some(i) => (i, false),
            None => {
                let i = u32_of(stream.len())?;
                let cells = if wide { 2 } else { 1 };
                let want = glyph_bytes(
                    strike.format,
                    u32::from(strike.cell_width),
                    u32::from(strike.cell_height),
                    cells,
                )
                .ok_or("an unknown pixel format")?;
                if u32_of(bytes.len())? != want {
                    return Err("a glyph whose bytes do not fit its cells and format");
                }
                stream.push(bytes.clone());
                *slot = Some(i);
                (i, true)
            }
        };
        match runs.last_mut() {
            Some(r)
                if r.wide == wide
                    && last_new == new
                    && checked(r.first, r.len)? == cp
                    && checked(r.glyph, r.len)? == index =>
            {
                r.len = checked(r.len, 1)?;
            }
            _ => {
                runs.push(Run {
                    first: cp,
                    len: 1,
                    glyph: index,
                    wide,
                });
                last_new = new;
            }
        }
    }
    let glyph_count = u32_of(stream.len())?;
    if glyph_count > SET_GLYPHS_MAX {
        return Err("more glyphs in a strike than SET_GLYPHS_MAX");
    }
    let mut coverage = [(0u32, 0u32); 6];
    for (n, range) in coverage.iter_mut().zip(spans::ALL) {
        for &(first, last) in range {
            for cp in (first..=last).filter(|&cp| char::from_u32(cp).is_some()) {
                n.1 = checked(n.1, 1)?;
                if map.binary_search_by_key(&cp, |&(c, _)| c).is_ok() {
                    n.0 = checked(n.0, 1)?;
                }
            }
        }
    }
    Ok(Laid {
        glyph_count,
        renumbered,
        runs,
        stream,
        coverage,
    })
}

/// Where a glyph lies once cut into chunks: its chunk and its offset in it.
type Place = (u32, u32);

/// A chunk as stored: its flags and its bytes, and its decoded length.
struct Chunk {
    flags: u32,
    stored: Vec<u8>,
    decoded: u32,
}

/// A strike's glyph bytes cut into chunks of at most `SET_CHUNK_BYTES` on
/// glyph boundaries, each deflated when `compress` asks, the build
/// deflates and that makes it smaller; with each glyph's chunk, counted
/// from 0, and its offset in it. A glyph larger than `SET_CHUNK_BYTES`
/// takes a chunk of its own, exactly its bytes and never deflated
/// (ADR-1302 D6), so a chunk's decoded bytes past the bound are always
/// bytes the set holds.
fn chunks(stream: &[Vec<u8>], compress: bool) -> Result<(Vec<Chunk>, Vec<Place>), WriteError> {
    let limit = usize::try_from(SET_CHUNK_BYTES).map_err(|_| "a chunk past usize")?;
    // each chunk's bytes, and whether they are one glyph past the bound
    let mut raws: Vec<(Vec<u8>, bool)> = Vec::new();
    let mut raw: Vec<u8> = Vec::new();
    let mut at = Vec::with_capacity(stream.len());
    for glyph in stream {
        if glyph.len() > limit {
            if !raw.is_empty() {
                raws.push((std::mem::take(&mut raw), false));
            }
            at.push((u32_of(raws.len())?, 0));
            raws.push((glyph.clone(), true));
            continue;
        }
        if raw.len().saturating_add(glyph.len()) > limit {
            raws.push((std::mem::take(&mut raw), false));
        }
        at.push((u32_of(raws.len())?, u32_of(raw.len())?));
        raw.extend_from_slice(glyph);
    }
    if !raw.is_empty() {
        raws.push((raw, false));
    }
    let chunks = raws
        .into_iter()
        .map(|(raw, alone)| {
            let decoded = u32_of(raw.len())?;
            if let Some(packed) = (compress && !alone).then(|| deflate(&raw)).flatten()
                && packed.len() < raw.len()
            {
                return Ok(Chunk {
                    flags: CHUNK_DEFLATED,
                    stored: packed,
                    decoded,
                });
            }
            Ok(Chunk {
                flags: 0,
                stored: raw,
                decoded,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok((chunks, at))
}

/// Each chunk's record, its offset from the start of `out`, the section
/// so far, then the chunks' bytes.
fn put_chunks(out: &mut Vec<u8>, all: &[Chunk]) -> Result<(), WriteError> {
    let mut at = u64::from(CHUNK_RECORD)
        .checked_mul(u64_of(all.len())?)
        .and_then(|n| n.checked_add(u64_of(out.len()).ok()?))
        .ok_or("a count past u64")?;
    for c in all {
        put64(out, at);
        put32(out, u32_of(c.stored.len())?);
        put32(out, c.decoded);
        put32(out, c.flags);
        at = at
            .checked_add(u64_of(c.stored.len())?)
            .ok_or("a count past u64")?;
    }
    for c in all {
        out.extend_from_slice(&c.stored);
    }
    Ok(())
}

/// `CLUS`, or `None` when no strike maps a cluster: each strike's clusters
/// sorted by key, their glyphs stored once each, strike after strike.
fn clusters(order: &[&RawStrike]) -> Result<Option<(Vec<u8>, usize)>, WriteError> {
    let mut records = Vec::new();
    let mut pool: Vec<u32> = Vec::new();
    let mut all: Vec<Chunk> = Vec::new();
    for (i, s) in order.iter().enumerate() {
        if s.clusters.is_empty() {
            continue;
        }
        let strike = u16::try_from(i).map_err(|_| "a strike past u16")?;
        let mut sorted: Vec<&(Vec<u32>, u32)> = s.clusters.iter().collect();
        sorted.sort_unstable();
        if sorted.windows(2).any(|w| matches!(w, [a, b] if a.0 == b.0)) {
            return Err("a cluster mapped twice");
        }
        // each glyph stored once, in the order the sorted keys reach it
        let mut stored: BTreeMap<u32, usize> = BTreeMap::new();
        let mut stream: Vec<Vec<u8>> = Vec::new();
        for &(_, g) in &sorted {
            if !stored.contains_key(g) {
                let i = usize::try_from(*g).map_err(|_| "a glyph index past usize")?;
                let (wide, bytes) = s
                    .glyphs
                    .get(i)
                    .ok_or("a cluster mapped to a glyph the strike lacks")?;
                let want = glyph_bytes(
                    s.format,
                    u32::from(s.cell_width),
                    u32::from(s.cell_height),
                    if *wide { 2 } else { 1 },
                )
                .ok_or("an unknown pixel format")?;
                if u32_of(bytes.len())? != want {
                    return Err("a glyph whose bytes do not fit its cells and format");
                }
                stored.insert(*g, stream.len());
                stream.push(bytes.clone());
            }
        }
        let (chunks, at) = self::chunks(&stream, s.compress)?;
        let first = u32_of(all.len())?;
        for (key, g) in sorted {
            if key.is_empty() || u32_of(key.len())? > CLUSTER_SCALARS_MAX {
                return Err("a cluster empty or longer than CLUSTER_SCALARS_MAX");
            }
            if key.iter().any(|&c| char::from_u32(c).is_none()) {
                return Err("a cluster holding a codepoint that is not a character");
            }
            let &(chunk, offset) = stored
                .get(g)
                .and_then(|&n| at.get(n))
                .ok_or("a cluster mapped to a glyph the strike lacks")?;
            let wide = usize::try_from(*g)
                .ok()
                .and_then(|g| s.glyphs.get(g))
                .is_some_and(|&(w, _)| w);
            put16(&mut records, strike);
            put16(
                &mut records,
                u16::try_from(key.len()).map_err(|_| "a key past u16")?,
            );
            put32(&mut records, u32_of(pool.len())?);
            put32(&mut records, checked(first, chunk)?);
            put32(&mut records, offset);
            put32(&mut records, if wide { RUN_WIDE } else { 0 });
            pool.extend_from_slice(key);
        }
        all.extend(chunks);
    }
    if records.is_empty() {
        return Ok(None);
    }
    let count = order.iter().map(|s| s.clusters.len()).sum::<usize>();
    if u32_of(count)? > SET_CLUSTERS_MAX {
        return Err("more clusters than SET_CLUSTERS_MAX");
    }
    if u32_of(all.len())? > SET_CHUNKS_MAX {
        return Err("more cluster chunks than SET_CHUNKS_MAX");
    }
    let mut out = Vec::new();
    for v in [
        u32_of(count)?,
        CLUSTER_RECORD,
        u32_of(pool.len())?,
        u32_of(all.len())?,
        CHUNK_RECORD,
    ] {
        put32(&mut out, v);
    }
    out.extend(records);
    for c in pool {
        put32(&mut out, c);
    }
    put_chunks(&mut out, &all)?;
    Ok(Some((out, all.len())))
}

/// Checks `ink` as the box of a glyph of strike `s`, `wide` or not: in
/// an R8 or 1-bit strike, or of a color emoji in one cell, reaching at
/// most 1 pixel past it on each side and inside its row (ADR-1302 D5);
/// its bytes the box's, inside a chunk, spanning the glyph's cells, past
/// them on at least one side and inside their reach (ADR-1302 D1, D6,
/// ADR-1306 D1, D3).
fn check_ink(s: &RawStrike, wide: bool, ink: &RawInk) -> Result<(), WriteError> {
    let cell = (i64::from(s.cell_width), i64::from(s.cell_height));
    let cells = (cell.0.saturating_mul(if wide { 2 } else { 1 }), cell.1);
    let boxed = InkRect {
        left: i64::from(ink.left),
        top: i64::from(ink.top),
        width: i64::from(ink.width),
        height: i64::from(ink.height),
    };
    if s.format == FORMAT_RGBA && !boxed.is_emoji_edge(cell, wide) {
        return Err("an ink box in a color strike past 1 pixel beside a one-cell glyph");
    }
    if !boxed.in_reach(cell, cells) {
        return Err("an ink box outside its glyph's reach, or not past its cells");
    }
    let want = glyph_bytes(s.format, ink.width, ink.height, 1).ok_or("an unknown pixel format")?;
    if u32_of(ink.bytes.len())? != want {
        return Err("an ink box whose bytes do not fit its width and format");
    }
    if want > SET_CHUNK_BYTES {
        return Err("an ink box larger than a chunk");
    }
    Ok(())
}

/// An `INKX` record as the writer lays it out: its strike, kind and
/// number, then the box's left, width, chunk, offset, top and height.
type InkRecord = (u16, u16, u32, (i32, u32, u32, u32, i32, u32));

/// `INKX`, or `None` when no glyph holds an ink box: each strike's boxes
/// stored once each, with a record for each glyph `GLYF` numbers and each
/// cluster `CLUS` holds that draws a glyph with a box, sorted by strike,
/// kind and number.
fn inks(order: &[&RawStrike], laid: &[Laid]) -> Result<Option<(Vec<u8>, usize)>, WriteError> {
    let mut records: Vec<InkRecord> = Vec::new();
    let mut all: Vec<Chunk> = Vec::new();
    // the number of the strike's first cluster in `CLUS`
    let mut cluster_base = 0u32;
    for (i, (s, l)) in order.iter().zip(laid).enumerate() {
        let strike = u16::try_from(i).map_err(|_| "a strike past u16")?;
        let mut sorted: Vec<&(Vec<u32>, u32)> = s.clusters.iter().collect();
        sorted.sort_unstable();
        let mut inks: Vec<&(u32, RawInk)> = s.inks.iter().collect();
        inks.sort_unstable_by_key(|(g, _)| *g);
        if inks.windows(2).any(|w| matches!(w, [a, b] if a.0 == b.0)) {
            return Err("a glyph with two ink boxes");
        }
        let mut stream: Vec<Vec<u8>> = Vec::new();
        let mut boxed: Vec<(u32, usize, &RawInk)> = Vec::new();
        for &&(g, ref ink) in &inks {
            let gi = usize::try_from(g).map_err(|_| "a glyph index past usize")?;
            let &(wide, _) = s
                .glyphs
                .get(gi)
                .ok_or("an ink box for a glyph the strike lacks")?;
            check_ink(s, wide, ink)?;
            let mapped = l.renumbered.get(gi).copied().flatten().is_some();
            let clustered = sorted.iter().any(|(_, cg)| *cg == g);
            if mapped || clustered {
                boxed.push((g, stream.len(), ink));
                stream.push(ink.bytes.clone());
            }
        }
        let (chunks, at) = self::chunks(&stream, s.compress)?;
        let first = u32_of(all.len())?;
        for &(g, n, ink) in &boxed {
            let &(chunk, start) = at.get(n).ok_or("an ink box lost")?;
            let chunk = checked(first, chunk)?;
            // the record names the box's row at the top of its glyph's row
            let row = glyph_bytes(s.format, ink.width, 1, 1).ok_or("an unknown pixel format")?;
            let above = ink
                .top
                .unsigned_abs()
                .checked_mul(row)
                .ok_or("an ink box past u32")?;
            let offset = checked(start, above)?;
            let at = (ink.left, ink.width, chunk, offset, ink.top, ink.height);
            let gi = usize::try_from(g).map_err(|_| "a glyph index past usize")?;
            if let Some(index) = l.renumbered.get(gi).copied().flatten() {
                records.push((strike, INK_GLYPH, index, at));
            }
            for (k, (_, cg)) in sorted.iter().enumerate() {
                if *cg == g {
                    let index = checked(cluster_base, u32_of(k)?)?;
                    records.push((strike, INK_CLUSTER, index, at));
                }
            }
        }
        all.extend(chunks);
        cluster_base = checked(cluster_base, u32_of(s.clusters.len())?)?;
    }
    if records.is_empty() {
        return Ok(None);
    }
    records.sort_unstable_by_key(|r| (r.0, r.1, r.2));
    if u32_of(records.len())? > SET_INKS_MAX {
        return Err("more ink boxes than SET_INKS_MAX");
    }
    let mut out = Vec::new();
    for v in [
        u32_of(records.len())?,
        INK_RECORD,
        u32_of(all.len())?,
        CHUNK_RECORD,
    ] {
        put32(&mut out, v);
    }
    for (strike, kind, index, (left, width, chunk, offset, top, height)) in records {
        put16(&mut out, strike);
        put16(&mut out, kind);
        put32(&mut out, index);
        out.extend_from_slice(&left.to_le_bytes());
        put32(&mut out, width);
        put32(&mut out, chunk);
        put32(&mut out, offset);
        out.extend_from_slice(&top.to_le_bytes());
        put32(&mut out, height);
    }
    put_chunks(&mut out, &all)?;
    Ok(Some((out, all.len())))
}

/// Refuses a set whose `GLYF`, `CLUS` and `INKX` chunks, `counts`,
/// together pass `max`, `SET_CHUNKS_MAX` in a set [`encode`] writes, which
/// the reader holds the whole set to.
fn chunks_within(counts: &[usize], max: u32) -> Result<(), WriteError> {
    let total = counts.iter().try_fold(0usize, |n, &c| n.checked_add(c));
    match total {
        Some(n) if u32_of(n)? <= max => Ok(()),
        _ => Err("more chunks over GLYF, CLUS and INKX than SET_CHUNKS_MAX"),
    }
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// The set's bytes.
pub(crate) fn encode(set: &RawSet) -> Result<Vec<u8>, WriteError> {
    encode_within(set, SET_CHUNKS_MAX)
}

/// [`encode`], the set's chunks over `GLYF`, `CLUS` and `INKX` together
/// held to `chunks_max`. Only a test asks for a bound under
/// `SET_CHUNKS_MAX`: a set past that bound holds over 2 GiB of glyphs.
fn encode_within(set: &RawSet, chunks_max: u32) -> Result<Vec<u8>, WriteError> {
    let mut meta = String::new();
    for (key, value) in &set.meta {
        if key.is_empty() || key.contains(['=', '\n', '\r']) || value.contains(['\n', '\r']) {
            return Err("a META key or value with a line break or a misplaced '='");
        }
        meta.push_str(key);
        meta.push('=');
        meta.push_str(value);
        meta.push('\n');
    }
    if u32_of(meta.len())? > SET_META_BYTES_MAX {
        return Err("META larger than SET_META_BYTES_MAX");
    }
    if set.policy.iter().any(|&s| s > SOURCE_AUTO) {
        return Err("an unknown sprite source");
    }

    let key = |s: &RawStrike| (s.cell_height, s.cell_width, s.script, s.style, s.format);
    let mut order: Vec<&RawStrike> = set.strikes.iter().collect();
    order.sort_by_key(|s| key(s));
    if order.is_empty() || u32_of(order.len())? > SET_STRIKES_MAX {
        return Err("no strikes, or more than SET_STRIKES_MAX");
    }
    if order
        .windows(2)
        .any(|w| matches!(w, [a, b] if key(a) == key(b)))
    {
        return Err("two strikes of one script, style, cell size and format");
    }
    for s in &order {
        if !matches!(s.format, FORMAT_BITS | FORMAT_R8 | FORMAT_RGBA) {
            return Err("an unknown pixel format");
        }
        if s.style > 3 {
            return Err("an unknown style");
        }
        if s.script > SCRIPT_SUB {
            return Err("an unknown script");
        }
        let (w, h) = (u32::from(s.cell_width), u32::from(s.cell_height));
        if w == 0 || h == 0 || w > CELL_SIDE_MAX || h > CELL_SIDE_MAX {
            return Err("a cell 0 pixels across or past CELL_SIDE_MAX");
        }
    }

    let laid: Vec<Laid> = order.iter().map(|s| lay_out(s)).collect::<Result<_, _>>()?;
    // one cell's strikes, a pixel format each, map codepoints apart
    let mut taken: Vec<((u16, u16, u16, u8), u32)> = order
        .iter()
        .flat_map(|s| {
            let k = (s.cell_height, s.cell_width, s.script, s.style);
            s.map.iter().map(move |&(cp, _)| (k, cp))
        })
        .collect();
    taken.sort_unstable();
    if taken.windows(2).any(|w| matches!(w, [a, b] if a == b)) {
        return Err("two strikes at a cell map one codepoint");
    }
    let total_runs = laid
        .iter()
        .try_fold(0u32, |n, l| checked(n, u32_of(l.runs.len())?))?;
    if total_runs > SET_RUNS_MAX {
        return Err("more runs than SET_RUNS_MAX");
    }

    let mut strk = Vec::new();
    put32(&mut strk, u32_of(order.len())?);
    put32(&mut strk, STRIKE_RECORD);
    for (s, l) in order.iter().zip(&laid) {
        strk.extend_from_slice(&[s.style, s.format]);
        for v in [
            s.script,
            s.cell_width,
            s.cell_height,
            s.baseline,
            s.underline,
            s.underline_thickness,
            s.strike,
            s.strike_thickness,
            s.overline,
        ] {
            put16(&mut strk, v);
        }
        put32(&mut strk, l.glyph_count);
    }

    let mut cmap = Vec::new();
    put32(&mut cmap, u32_of(order.len())?);
    put32(&mut cmap, RUN_RECORD);
    for l in &laid {
        put32(&mut cmap, u32_of(l.runs.len())?);
        for r in &l.runs {
            for v in [r.first, r.len, r.glyph, if r.wide { RUN_WIDE } else { 0 }] {
                put32(&mut cmap, v);
            }
        }
    }

    let mut poly = set.policy.to_vec();
    put16(&mut poly, 0);
    put32(&mut poly, u32_of(order.len())?);
    put32(&mut poly, COVERAGE_RECORD);
    for l in &laid {
        for (covered, total) in l.coverage {
            put32(&mut poly, covered);
            put32(&mut poly, total);
        }
    }

    let mut all: Vec<Chunk> = Vec::new();
    for (s, l) in order.iter().zip(&laid) {
        all.extend(chunks(&l.stream, s.compress)?.0);
    }
    let mut glyf = Vec::new();
    put32(&mut glyf, u32_of(all.len())?);
    put32(&mut glyf, CHUNK_RECORD);
    put_chunks(&mut glyf, &all)?;
    let clus = clusters(&order)?;
    let inkx = inks(&order, &laid)?;
    chunks_within(
        &[
            all.len(),
            clus.as_ref().map_or(0, |c| c.1),
            inkx.as_ref().map_or(0, |c| c.1),
        ],
        chunks_max,
    )?;

    let mut sections: Vec<([u8; 4], &[u8])> = vec![
        (META, meta.as_bytes()),
        (STRK, &strk),
        (CMAP, &cmap),
        (POLY, &poly),
        (GLYF, &glyf),
    ];
    if let Some((clus, _)) = &clus {
        sections.push((CLUS, clus));
    }
    if let Some((inkx, _)) = &inkx {
        sections.push((INKX, inkx));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    put16(&mut out, MAJOR);
    put16(&mut out, MINOR);
    put32(&mut out, u32_of(sections.len())?);
    let mut offset = u64::from(PRELUDE_BYTES)
        .checked_add(u64::from(SECTION_BYTES).saturating_mul(u64_of(sections.len())?))
        .ok_or("a count past u64")?;
    for (tag, bytes) in &sections {
        let len = u64_of(bytes.len())?;
        out.extend_from_slice(tag);
        put64(&mut out, offset);
        put64(&mut out, len);
        offset = offset.checked_add(len).ok_or("a count past u64")?;
    }
    for (_, bytes) in &sections {
        out.extend_from_slice(bytes);
    }
    if u64::try_from(out.len()).map_err(|_| "a count past u64")? > u64::from(SET_BYTES_MAX) {
        return Err("a set larger than SET_BYTES_MAX");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{RawInk, RawSet, RawStrike, chunks_within, encode, encode_within};
    use crate::bounds::SET_CHUNKS_MAX;
    use crate::set::format::FORMAT_R8;

    // the writer holds the set's chunks over its three chunked sections to
    // SET_CHUNKS_MAX, as the reader does
    #[test]
    fn the_chunks_of_all_sections_together_are_bounded() {
        let max = SET_CHUNKS_MAX as usize;
        assert!(chunks_within(&[max - 2, 1, 1], SET_CHUNKS_MAX).is_ok());
        assert!(
            chunks_within(&[max - 2, 1, 2], SET_CHUNKS_MAX).is_err(),
            "INKX past the bound"
        );
        assert!(chunks_within(&[max, 0, 0], SET_CHUNKS_MAX).is_ok());
        assert!(
            chunks_within(&[usize::MAX, 1, 0], SET_CHUNKS_MAX).is_err(),
            "a count past usize"
        );
    }

    /// One R8 strike in a cell 90 pixels wide and 100 high, its glyphs
    /// 9,000 bytes each, so no two share a 16 KiB chunk: `a`, `b` and `c`
    /// in three `GLYF` chunks, a cluster drawing `a`'s glyph in one `CLUS`
    /// chunk, and `b`'s ink box, 100 columns from 10 left of its cell, in
    /// one `INKX` chunk: five chunks in all.
    fn five_chunks() -> RawSet {
        let glyph = |v: u8| (false, vec![v; 90 * 100]);
        RawSet {
            meta: Vec::new(),
            policy: [0; 6],
            strikes: vec![RawStrike {
                style: 0,
                format: FORMAT_R8,
                script: 0,
                cell_width: 90,
                cell_height: 100,
                baseline: 80,
                underline: 90,
                underline_thickness: 1,
                strike: 50,
                strike_thickness: 1,
                overline: 0,
                compress: false,
                glyphs: vec![glyph(1), glyph(2), glyph(3)],
                map: vec![(0x61, 0), (0x62, 1), (0x63, 2)],
                clusters: vec![(vec![0x65, 0x301], 0)],
                inks: vec![(
                    1,
                    RawInk {
                        left: -10,
                        top: 0,
                        width: 100,
                        height: 100,
                        bytes: vec![2; 100 * 100],
                    },
                )],
            }],
        }
    }

    /// `five_chunks` in a cell 9 pixels wide, its three glyphs of 900
    /// bytes sharing a chunk, each a different pattern, so deflate and
    /// the chunks' boundaries both show in its bytes.
    fn patterned(compress: bool) -> RawSet {
        let mut set = five_chunks();
        for strike in &mut set.strikes {
            strike.compress = compress;
            strike.cell_width = 9;
            for (i, (_, bytes)) in (3u8..).zip(strike.glyphs.iter_mut()) {
                *bytes = (0..900u16)
                    .map(|p| u8::try_from(p.wrapping_mul(u16::from(i)) % 251).unwrap_or(0))
                    .collect();
            }
            for (_, ink) in &mut strike.inks {
                *ink = RawInk {
                    left: -1,
                    top: 0,
                    width: 11,
                    height: 100,
                    bytes: vec![2; 11 * 100],
                };
            }
        }
        set
    }

    /// `set`, written by this format, as format 2.0 wrote it: the header's
    /// and META's minor version 0, and each `INKX` record cut to 24 bytes,
    /// without the box's top and height, with the chunks' offsets from the
    /// section's start moved back by the bytes cut. `INKX` is the last
    /// section, so no other section moves.
    /// A set's prelude and every section but META, each as its tag and its
    /// bytes in table order. The built-in sets' META names the crate's
    /// version (build.rs's `tool` key), so a hash of this moves only when
    /// what the set draws moves, never at a version bump.
    #[expect(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        reason = "a test fails by panicking"
    )]
    fn without_meta(set: &[u8]) -> Vec<u8> {
        let word = |i: usize| u32::from_le_bytes(set[i..i + 4].try_into().unwrap());
        let long = |i: usize| u64::from_le_bytes(set[i..i + 8].try_into().unwrap()) as usize;
        let mut kept = set[..16].to_vec();
        for k in 0..word(12) as usize {
            let entry = 16 + 20 * k;
            if &set[entry..entry + 4] == b"META" {
                continue;
            }
            let (at, len) = (long(entry + 4), long(entry + 12));
            kept.extend_from_slice(&set[entry..entry + 4]);
            kept.extend_from_slice(&set[at..at + len]);
        }
        kept
    }

    #[expect(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        reason = "a test fails by panicking"
    )]
    fn as_format_2_0(set: &[u8]) -> Vec<u8> {
        let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let mut out = set.to_vec();
        out[10..12].copy_from_slice(&0u16.to_le_bytes());
        // a set the builder writes names its version in META too
        let this = format!("format={}.{}", super::MAJOR, super::MINOR);
        if let Some(meta) = out.windows(10).position(|w| w == this.as_bytes()) {
            out[meta..meta + 10].copy_from_slice(b"format=2.0");
        }
        let sections = word(&out, 12) as usize;
        let Some(entry) = (0..sections)
            .map(|k| 16 + 20 * k)
            .find(|&e| &out[e..e + 4] == b"INKX")
        else {
            return out;
        };
        let at = u64::from_le_bytes(out[entry + 4..entry + 12].try_into().unwrap()) as usize;
        let inkx = out[at..].to_vec();
        let (count, chunks) = (word(&inkx, 0) as usize, word(&inkx, 8) as usize);
        assert_eq!(word(&inkx, 4), 32);
        let mut cut = inkx[..16].to_vec();
        cut[4..8].copy_from_slice(&24u32.to_le_bytes());
        for r in 0..count {
            let record = &inkx[16 + r * 32..16 + r * 32 + 32];
            assert_eq!(&record[24..28], &0i32.to_le_bytes(), "a box of its row");
            cut.extend_from_slice(&record[..24]);
        }
        let table = 16 + count * 32;
        for c in 0..chunks {
            let mut record = inkx[table + c * 20..table + c * 20 + 20].to_vec();
            let from = u64::from_le_bytes(record[..8].try_into().unwrap()) - (count * 8) as u64;
            record[..8].copy_from_slice(&from.to_le_bytes());
            cut.extend_from_slice(&record);
        }
        cut.extend_from_slice(&inkx[table + chunks * 20..]);
        out.truncate(at);
        out.extend_from_slice(&cut);
        let len = (cut.len() as u64).to_le_bytes();
        out[entry + 12..entry + 20].copy_from_slice(&len);
        out
    }

    // a set whose glyphs all fit a chunk is laid out as format 2.0 lays it
    // out (ADR-1302 D6), so carrying glyphs that pass a chunk added no
    // version: the SHA-256 of each set below is that of the format 2.0
    // writer's bytes, and each set is taken back to format 2.0 first, so
    // the test also shows that formats 2.1 and 2.2 differ only in the
    // version and the two fields each INKX record gains: a set whose
    // strikes are all normal carries 0 in the script field, where 2.0 and
    // 2.1 reserved it (ADR-1202 D11)
    #[test]
    fn a_set_with_no_glyph_past_a_chunk_is_written_as_before() {
        let sha = |bytes: &[u8]| crate::sha256::hex(&crate::sha256::Sha256::digest(bytes));
        let written = |set: &RawSet| encode(set).map(|bytes| sha(&as_format_2_0(&bytes)));
        let raw = "d1b3685dc145be370440b69d2935969f87711863f6ff28a7c7a636a92887c770";
        assert_eq!(written(&patterned(false)).as_deref(), Ok(raw));
        // deflated only where the build deflates (`deflate`)
        let packed = if cfg!(feature = "deflate") {
            "e0d026403e03ab1292cffe34b12bd20c9fbd1414cf01f83218339dc2bc7f870a"
        } else {
            raw
        };
        assert_eq!(written(&patterned(true)).as_deref(), Ok(packed));
        assert_eq!(
            written(&five_chunks()).as_deref(),
            Ok("f9589d871394767b3c8ea121db80c43e99e7bb566e08a12b5a14aacefcc17ca3")
        );
        // the built-in sets, which build.rs writes with this writer, each
        // chunk deflated, hashed without META (`without_meta`)
        #[cfg(feature = "unifont")]
        assert_eq!(
            sha(&without_meta(&as_format_2_0(include_bytes!(concat!(
                env!("OUT_DIR"),
                "/unifont.vtglass"
            ))))),
            "b75720a67660fefb8d42112da52280dbd07e4ed82243667360ee7f7fc8d891c3"
        );
        #[cfg(feature = "unscii")]
        assert_eq!(
            sha(&without_meta(&as_format_2_0(include_bytes!(concat!(
                env!("OUT_DIR"),
                "/unscii-8.vtglass"
            ))))),
            "9fcf711cafe672b88df8b067382df45b0bd67db64d3d8836ba1a71161fc82592"
        );
    }

    // encode refuses a set whose GLYF, CLUS and INKX chunks together pass
    // the chunk bound, though GLYF's alone are within it (ADR-1200 D2:
    // every count is bounded): with the bound at 4, the five chunks are
    // refused, and with it at 5 they are written. A set past
    // SET_CHUNKS_MAX itself would hold over 2 GiB of glyphs, so the test
    // lowers the bound
    #[test]
    fn encode_refuses_a_set_whose_sections_together_pass_the_chunk_bound() {
        let set = five_chunks();
        assert!(encode(&set).is_ok(), "within SET_CHUNKS_MAX");
        assert!(encode_within(&set, 5).is_ok(), "five chunks, a bound of 5");
        assert_eq!(
            encode_within(&set, 4).err(),
            Some("more chunks over GLYF, CLUS and INKX than SET_CHUNKS_MAX"),
            "five chunks, a bound of 4"
        );
    }
}
