//! The set writer's core: a set described by plain numbers in, its bytes
//! out (ADR-1200 D2). [`SetBuilder`](super::SetBuilder) puts vtium-font's
//! types on it, and `build.rs` includes this file to pack the built-in
//! sets.
//!
//! The bytes depend only on what is given: strikes are sorted, each
//! strike's map is sorted, glyphs are numbered in the order the map first
//! reaches them, and every chunk is deflated at one fixed level, so the
//! same set gives the same bytes on every host. A build without the
//! `deflate` feature, the console's, stores every chunk raw.

use crate::bounds::{
    CELL_SIDE_MAX, CLUSTER_SCALARS_MAX, SET_BYTES_MAX, SET_CHUNK_BYTES, SET_CHUNKS_MAX,
    SET_CLUSTERS_MAX, SET_GLYPHS_MAX, SET_META_BYTES_MAX, SET_RUNS_MAX, SET_STRIKES_MAX,
};
use crate::set::format::{
    CHUNK_DEFLATED, CHUNK_RECORD, CLUS, CLUSTER_RECORD, CMAP, COVERAGE_RECORD, FORMAT_BITS,
    FORMAT_R8, FORMAT_RGBA, GLYF, MAGIC, MAJOR, META, MINOR, POLY, PRELUDE_BYTES, RUN_RECORD,
    RUN_WIDE, SECTION_BYTES, SOURCE_AUTO, STRIKE_RECORD, STRK, glyph_bytes,
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
    pub cell_width: u16,
    pub cell_height: u16,
    pub baseline: u16,
    pub underline: u16,
    pub underline_thickness: u16,
    pub strike: u16,
    pub strike_thickness: u16,
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
                if want > SET_CHUNK_BYTES {
                    return Err("a glyph larger than a chunk");
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
/// from 0, and its offset in it.
fn chunks(stream: &[Vec<u8>], compress: bool) -> Result<(Vec<Chunk>, Vec<Place>), WriteError> {
    let limit = usize::try_from(SET_CHUNK_BYTES).map_err(|_| "a chunk past usize")?;
    let mut raws: Vec<Vec<u8>> = Vec::new();
    let mut raw: Vec<u8> = Vec::new();
    let mut at = Vec::with_capacity(stream.len());
    for glyph in stream {
        if raw.len().saturating_add(glyph.len()) > limit {
            raws.push(std::mem::take(&mut raw));
        }
        at.push((u32_of(raws.len())?, u32_of(raw.len())?));
        raw.extend_from_slice(glyph);
    }
    if !raw.is_empty() {
        raws.push(raw);
    }
    let chunks = raws
        .into_iter()
        .map(|raw| {
            let decoded = u32_of(raw.len())?;
            if let Some(packed) = compress.then(|| deflate(&raw)).flatten()
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
fn clusters(order: &[&RawStrike]) -> Result<Option<Vec<u8>>, WriteError> {
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
                if u32_of(bytes.len())? != want || want > SET_CHUNK_BYTES {
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
    Ok(Some(out))
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

    let key = |s: &RawStrike| (s.cell_height, s.cell_width, s.style, s.format);
    let mut order: Vec<&RawStrike> = set.strikes.iter().collect();
    order.sort_by_key(|s| key(s));
    if order.is_empty() || u32_of(order.len())? > SET_STRIKES_MAX {
        return Err("no strikes, or more than SET_STRIKES_MAX");
    }
    if order
        .windows(2)
        .any(|w| matches!(w, [a, b] if key(a) == key(b)))
    {
        return Err("two strikes of one style, cell size and format");
    }
    for s in &order {
        if !matches!(s.format, FORMAT_BITS | FORMAT_R8 | FORMAT_RGBA) {
            return Err("an unknown pixel format");
        }
        if s.style > 3 {
            return Err("an unknown style");
        }
        let (w, h) = (u32::from(s.cell_width), u32::from(s.cell_height));
        if w == 0 || h == 0 || w > CELL_SIDE_MAX || h > CELL_SIDE_MAX {
            return Err("a cell 0 pixels across or past CELL_SIDE_MAX");
        }
        if !order
            .iter()
            .any(|r| r.style == 0 && (r.cell_width, r.cell_height) == (s.cell_width, s.cell_height))
        {
            return Err("a cell size with no regular strike");
        }
    }

    let laid: Vec<Laid> = order.iter().map(|s| lay_out(s)).collect::<Result<_, _>>()?;
    // one cell's strikes, a pixel format each, map codepoints apart
    let mut taken: Vec<((u16, u16, u8), u32)> = order
        .iter()
        .flat_map(|s| {
            let k = (s.cell_height, s.cell_width, s.style);
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
            0,
            s.cell_width,
            s.cell_height,
            s.baseline,
            s.underline,
            s.underline_thickness,
            s.strike,
            s.strike_thickness,
            0,
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
    if u32_of(all.len())? > SET_CHUNKS_MAX {
        return Err("more chunks than SET_CHUNKS_MAX");
    }
    let mut glyf = Vec::new();
    put32(&mut glyf, u32_of(all.len())?);
    put32(&mut glyf, CHUNK_RECORD);
    put_chunks(&mut glyf, &all)?;
    let clus = clusters(&order)?;

    let mut sections: Vec<([u8; 4], &[u8])> = vec![
        (META, meta.as_bytes()),
        (STRK, &strk),
        (CMAP, &cmap),
        (POLY, &poly),
        (GLYF, &glyf),
    ];
    if let Some(clus) = &clus {
        sections.push((CLUS, clus));
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
