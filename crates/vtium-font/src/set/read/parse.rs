//! Each section's check: a set is parsed as untrusted input, every count
//! against its bound and every table against the others, before any glyph
//! is drawn from it (ADR-1200 D2).

use std::cmp::Ordering;

use super::{
    Chunk, Cursor, Located, PixelFormat, Run, SetError, Span, Strike, StrikeInfo, add, mul,
};
use crate::bounds::{
    CELL_SIDE_MAX, CLUSTER_SCALARS_MAX, SET_CHUNK_BYTES, SET_CHUNKS_MAX, SET_CLUSTERS_MAX,
    SET_GLYPHS_MAX, SET_INKS_MAX, SET_RECORD_BYTES_MAX, SET_RUNS_MAX, SET_STRIKES_MAX,
};
use crate::face::{Cell, Decorations, Style};
use crate::policy::{Coverage, RangeCoverage, Source, SpritePolicy};
use crate::script::Script;
use crate::set::format::{
    CHUNK_DEFLATED, CHUNK_RECORD, CLUSTER_RECORD, COVERAGE_RECORD, INK_CLUSTER, INK_GLYPH,
    INK_RECORD, INK_RECORD_MIN, RUN_RECORD, RUN_WIDE, SCRIPT_BASELINE, SCRIPT_SUB, SCRIPT_SUPER,
    SOURCE_AUTO, SOURCE_FONT, SOURCE_SPRITES, STRIKE_RECORD, STYLE_BOLD, STYLE_ITALIC,
    TABLE_HEAD_BYTES, glyph_bytes, valid_name,
};
use crate::sprite::Range;

/// `META`: UTF-8 `key=value` lines, each ended, with one valid `name`.
pub(super) fn parse_meta(bytes: &[u8]) -> Result<String, SetError> {
    let text = std::str::from_utf8(bytes).map_err(|_| SetError::Malformed("META is not UTF-8"))?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(SetError::Malformed("META's last line is not ended"));
    }
    let mut names = 0u32;
    for line in text.lines() {
        match line.split_once('=') {
            Some((key, value)) if !key.is_empty() && !line.contains('\r') => {
                if key == "name" {
                    if !valid_name(value) {
                        return Err(SetError::Malformed("META's name is not a set name"));
                    }
                    names = names.saturating_add(1);
                }
            }
            _ => return Err(SetError::Malformed("a META line that is not key=value")),
        }
    }
    if names != 1 {
        return Err(SetError::Malformed("META names the set other than once"));
    }
    Ok(text.to_owned())
}

/// A decoration line from `STRK`: inside the cell and at least a row
/// thick.
fn line(at: u16, thickness: u16, height: u32) -> Result<(u32, u32), SetError> {
    let (at, thickness) = (u32::from(at), u32::from(thickness));
    if thickness == 0 || at.saturating_add(thickness) > height {
        return Err(SetError::Malformed("a decoration outside its cell"));
    }
    Ok((at, thickness))
}

/// `STRK`: each strike's style, format, script, cell, metrics and glyph
/// count, sorted by cell height, width, script, style and format with no
/// two alike. A set of format 2.0 or 2.1 holds 0 where the script is, so
/// its strikes are normal ones (ADR-1202 D11).
pub(super) fn parse_strikes(bytes: &[u8]) -> Result<Vec<Strike>, SetError> {
    let mut c = Cursor(bytes);
    let (count, size) = c.table(SET_STRIKES_MAX, "SET_STRIKES_MAX", STRIKE_RECORD)?;
    if count == 0 {
        return Err(SetError::Malformed("a set with no strikes"));
    }
    let mut strikes: Vec<Strike> = Vec::new();
    let mut keys: Vec<(u16, u16, u16, u8, u8)> = Vec::new();
    for _ in 0..count {
        let mut r = c.record(size)?;
        let style = r.u8()?;
        let format = r.u8()?;
        let script_code = r.u16()?;
        let (w, h) = (r.u16()?, r.u16()?);
        let baseline = r.u16()?;
        let (underline, underline_thickness) = (r.u16()?, r.u16()?);
        let (strike, strike_thickness) = (r.u16()?, r.u16()?);
        let reserved2 = r.u16()?;
        let glyph_count = r.u32()?;
        if reserved2 != 0 {
            return Err(SetError::Malformed("a reserved field that is not 0"));
        }
        let script = match script_code {
            SCRIPT_BASELINE => Script::Baseline,
            SCRIPT_SUPER => Script::Super,
            SCRIPT_SUB => Script::Sub,
            _ => return Err(SetError::Malformed("an unknown script")),
        };
        if style & !(STYLE_BOLD | STYLE_ITALIC) != 0 {
            return Err(SetError::Malformed("an unknown style"));
        }
        let code = format;
        let format = PixelFormat::from_code(format)?;
        let cell = Cell {
            width: u32::from(w),
            height: u32::from(h),
        };
        if cell.width == 0 || cell.height == 0 {
            return Err(SetError::Malformed("a cell 0 pixels across"));
        }
        if cell.width > CELL_SIDE_MAX || cell.height > CELL_SIDE_MAX {
            return Err(SetError::Bound("CELL_SIDE_MAX"));
        }
        if u32::from(baseline) > cell.height {
            return Err(SetError::Malformed("a baseline below its cell"));
        }
        if glyph_count > SET_GLYPHS_MAX {
            return Err(SetError::Bound("SET_GLYPHS_MAX"));
        }
        let (underline, underline_thickness) = line(underline, underline_thickness, cell.height)?;
        let (strike, strike_thickness) = line(strike, strike_thickness, cell.height)?;
        keys.push((h, w, script_code, style, code));
        strikes.push(Strike {
            info: StrikeInfo {
                style: Style {
                    bold: style & STYLE_BOLD != 0,
                    italic: style & STYLE_ITALIC != 0,
                },
                format,
                script,
                cell,
                baseline: u32::from(baseline),
                decorations: Decorations {
                    underline,
                    underline_thickness,
                    strike,
                    strike_thickness,
                },
                glyph_count,
            },
            runs: Vec::new(),
            spans: Vec::new(),
            stored: 0,
            coverage: Coverage::new(|_| RangeCoverage {
                covered: 0,
                total: 0,
            }),
            chunks: (0, 0),
            clusters: (0, 0),
        });
    }
    c.end()?;
    if keys.windows(2).any(|w| matches!(w, [a, b] if a >= b)) {
        return Err(SetError::Malformed("strikes out of order or repeated"));
    }
    Ok(strikes)
}

/// `CMAP`: each strike's runs, sorted and apart, every codepoint a
/// character. A run brings in the next glyphs or reuses earlier ones of
/// its width whole, and the runs bring in exactly the strike's glyphs.
pub(super) fn parse_cmap(bytes: &[u8], strikes: &mut [Strike]) -> Result<(), SetError> {
    let mut c = Cursor(bytes);
    let (count, size) = c.table(SET_STRIKES_MAX, "SET_STRIKES_MAX", RUN_RECORD)?;
    if usize::try_from(count).ok() != Some(strikes.len()) {
        return Err(SetError::Malformed("CMAP's strikes are not STRK's"));
    }
    let mut total = 0u32;
    for strike in strikes.iter_mut() {
        let n = c.u32()?;
        total = total.saturating_add(n);
        if total > SET_RUNS_MAX {
            return Err(SetError::Bound("SET_RUNS_MAX"));
        }
        // the runs' bytes are there before any is held, and the map is
        // allocated once, exactly as large as the counts declare
        let declared = usize::try_from(n).map_err(|_| SetError::Truncated)?;
        if c.0.len() < declared.saturating_mul(size) {
            return Err(SetError::Truncated);
        }
        let mut runs: Vec<Run> = Vec::with_capacity(declared);
        let mut spans: Vec<Span> = Vec::with_capacity(declared);
        let (mut next, mut stored) = (0u32, 0u64);
        for _ in 0..n {
            let mut r = c.record(size)?;
            let (first, len, glyph, flags) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
            if flags & !RUN_WIDE != 0 {
                return Err(SetError::Malformed("an unknown run flag"));
            }
            let wide = flags & RUN_WIDE != 0;
            let last = len
                .checked_sub(1)
                .and_then(|l| first.checked_add(l))
                .ok_or(SetError::Malformed("an empty run or one past u32"))?;
            if last > 0x10_ffff || (first <= 0xdfff && last >= 0xd800) {
                return Err(SetError::Malformed(
                    "a run of codepoints that are not characters",
                ));
            }
            if runs
                .last()
                .is_some_and(|p| first.wrapping_sub(p.first) < p.len || first < p.first)
            {
                return Err(SetError::Malformed("runs out of order or overlapping"));
            }
            let glyph_stored = strike.size(wide).ok_or(SetError::Truncated)?;
            if glyph == next {
                spans.push(Span {
                    glyph,
                    len,
                    wide,
                    stored,
                });
                stored = add(stored, mul(u64::from(len), glyph_stored)?)?;
                next = next
                    .checked_add(len)
                    .ok_or(SetError::Bound("SET_GLYPHS_MAX"))?;
                if next > strike.info.glyph_count {
                    return Err(SetError::Malformed("a run past its strike's glyphs"));
                }
            } else if glyph.checked_add(len).is_some_and(|end| end <= next) {
                reuses(&spans, glyph, len, wide)?;
            } else {
                return Err(SetError::Malformed(
                    "a run whose glyphs are neither new nor earlier",
                ));
            }
            runs.push(Run { first, len, glyph });
        }
        if next != strike.info.glyph_count {
            return Err(SetError::Malformed(
                "a strike's glyph count is not its map's",
            ));
        }
        strike.runs = runs;
        strike.spans = spans;
        strike.stored = stored;
    }
    c.end()?;
    apart(strikes)
}

/// One cell's strikes of one style in one script, a pixel format each,
/// map codepoints apart, so a codepoint draws from one strike whichever is
/// searched first.
fn apart(strikes: &[Strike]) -> Result<(), SetError> {
    let mut runs: Vec<(Cell, Style, Script, u32, u32)> = Vec::new();
    for s in strikes {
        for r in &s.runs {
            let last = r.first.saturating_add(r.len.saturating_sub(1));
            runs.push((s.info.cell, s.info.style, s.info.script, r.first, last));
        }
    }
    let key = |r: &(Cell, Style, Script, u32, u32)| {
        (r.0.height, r.0.width, r.2, r.1.bold, r.1.italic, r.3)
    };
    runs.sort_unstable_by_key(key);
    let overlap = runs
        .windows(2)
        .any(|w| matches!(w, [a, b] if (a.0, a.1, a.2) == (b.0, b.1, b.2) && b.3 <= a.4));
    if overlap {
        return Err(SetError::Malformed(
            "two strikes at a cell map one codepoint",
        ));
    }
    Ok(())
}

/// Checks that glyphs `[glyph, glyph + len)`, brought in by earlier runs,
/// are all `wide` or all narrow as the reusing run says.
fn reuses(spans: &[Span], glyph: u32, len: u32, wide: bool) -> Result<(), SetError> {
    let end = glyph.saturating_add(len);
    let from = spans
        .partition_point(|s| s.glyph <= glyph)
        .saturating_sub(1);
    let ok = spans
        .get(from..)
        .unwrap_or_default()
        .iter()
        .take_while(|s| s.glyph < end)
        .all(|s| s.wide == wide);
    if ok {
        Ok(())
    } else {
        Err(SetError::Malformed(
            "a run that reuses glyphs of another width",
        ))
    }
}

/// `POLY`: the sprite policy, and each strike's coverage of each range,
/// which must be what its map covers.
pub(super) fn parse_poly(bytes: &[u8], strikes: &mut [Strike]) -> Result<SpritePolicy, SetError> {
    let mut c = Cursor(bytes);
    let sources = c.array::<6>()?;
    if c.u16()? != 0 {
        return Err(SetError::Malformed("a reserved field that is not 0"));
    }
    let mut policy = SpritePolicy::SPRITES;
    for (range, code) in Range::ALL.into_iter().zip(sources) {
        let source = match code {
            SOURCE_FONT => Source::Font,
            SOURCE_SPRITES => Source::Sprites,
            SOURCE_AUTO => Source::Auto,
            _ => return Err(SetError::Malformed("an unknown sprite source")),
        };
        policy = policy.with(range, source);
    }
    let (count, size) = c.table(SET_STRIKES_MAX, "SET_STRIKES_MAX", COVERAGE_RECORD)?;
    if usize::try_from(count).ok() != Some(strikes.len()) {
        return Err(SetError::Malformed("POLY's strikes are not STRK's"));
    }
    for strike in strikes.iter_mut() {
        let mut r = c.record(size)?;
        let coverage = Coverage::new(|range| {
            RangeCoverage::count(range, |ch| strike.run(u32::from(ch)).is_some())
        });
        for range in Range::ALL {
            let n = coverage.get(range);
            if (r.u32()?, r.u32()?) != (n.covered, n.total) {
                return Err(SetError::Malformed("POLY's coverage is not CMAP's"));
            }
        }
        strike.coverage = coverage;
    }
    c.end()?;
    Ok(policy)
}

/// The most bytes a chunk of `GLYF` or `CLUS` may hold: `SET_CHUNK_BYTES`,
/// or the largest glyph a strike holds where that is larger, since such a
/// glyph takes a chunk of its own (ADR-1302 D6). Each strike's cell is
/// inside `CELL_SIDE_MAX`, so this is too.
fn largest_chunk(strikes: &[Strike]) -> u32 {
    strikes
        .iter()
        .filter_map(|s| s.size(true))
        .filter_map(|n| u32::try_from(n).ok())
        .fold(SET_CHUNK_BYTES, u32::max)
}

/// A chunk table of `count` records of `size` bytes, read from `c`, its
/// first chunk `at` bytes into its section, `section` its offset in the
/// file and its length: chunks packed in order and filling the section,
/// each within `SET_CHUNK_BYTES`, except a raw chunk of at most `largest`
/// bytes, which its section's reader holds to one glyph.
fn chunk_table(
    c: &mut Cursor<'_>,
    (count, size): (u32, usize),
    mut at: u64,
    section: (u64, u64),
    largest: u32,
) -> Result<Vec<Chunk>, SetError> {
    let mut chunks = Vec::new();
    for _ in 0..count {
        let mut r = c.record(size)?;
        let offset = r.u64()?;
        let (stored, decoded, flags) = (r.u32()?, r.u32()?, r.u32()?);
        if flags & !CHUNK_DEFLATED != 0 {
            return Err(SetError::Malformed("an unknown chunk flag"));
        }
        let deflated = flags & CHUNK_DEFLATED != 0;
        if offset != at {
            return Err(SetError::Malformed("chunks not packed in order"));
        }
        if stored == 0 || decoded == 0 {
            return Err(SetError::Malformed("an empty chunk"));
        }
        // only a raw chunk passes the bound, so its decoded bytes are bytes
        // the set holds
        if (stored > SET_CHUNK_BYTES || decoded > SET_CHUNK_BYTES)
            && (deflated || stored.max(decoded) > largest)
        {
            return Err(SetError::Bound("SET_CHUNK_BYTES"));
        }
        if !deflated && stored != decoded {
            return Err(SetError::Malformed("a raw chunk whose lengths differ"));
        }
        at = add(at, u64::from(stored))?;
        chunks.push(Chunk {
            offset: add(section.0, offset)?,
            stored,
            decoded,
            deflated,
            strike: 0,
            start: 0,
        });
    }
    if at != section.1 {
        return Err(SetError::Malformed("a section's length is not its chunks'"));
    }
    Ok(chunks)
}

/// `GLYF`'s index: chunks packed in order after it and filling the
/// section, each within `SET_CHUNK_BYTES` but a raw chunk holding exactly
/// one glyph larger than that, and each strike's glyph bytes cut into
/// whole chunks on glyph boundaries, strike after strike.
pub(super) fn parse_index(
    bytes: &[u8],
    glyf: (u64, u64),
    strikes: &mut [Strike],
) -> Result<Vec<Chunk>, SetError> {
    let mut c = Cursor(bytes);
    let table = c.table(SET_CHUNKS_MAX, "SET_CHUNKS_MAX", CHUNK_RECORD)?;
    let at = add(
        u64::from(TABLE_HEAD_BYTES),
        mul(
            u64::from(table.0),
            u64::try_from(table.1).map_err(|_| SetError::Truncated)?,
        )?,
    )?;
    let mut chunks = chunk_table(&mut c, table, at, glyf, largest_chunk(strikes))?;
    c.end()?;
    let mut next = 0usize;
    for (i, strike) in strikes.iter_mut().enumerate() {
        let first = next;
        let mut pos = 0u64;
        while pos < strike.stored {
            if !strike.boundary(pos) {
                return Err(SetError::Malformed("a chunk that splits a glyph"));
            }
            let chunk = chunks
                .get_mut(next)
                .ok_or(SetError::Malformed("fewer chunk bytes than glyph bytes"))?;
            if chunk.decoded > SET_CHUNK_BYTES
                && strike.glyph_at(pos) != Some(u64::from(chunk.decoded))
            {
                return Err(SetError::Malformed(
                    "a chunk past SET_CHUNK_BYTES that is not one glyph",
                ));
            }
            chunk.strike = i;
            chunk.start = pos;
            pos = add(pos, u64::from(chunk.decoded))?;
            next = next.saturating_add(1);
        }
        if pos != strike.stored {
            return Err(SetError::Malformed("a chunk across two strikes"));
        }
        strike.chunks = (first, next);
    }
    if next != chunks.len() {
        return Err(SetError::Malformed("more chunk bytes than glyph bytes"));
    }
    Ok(chunks)
}

/// The length of `CLUS`'s fixed head.
pub(super) const CLUSTER_HEAD_BYTES: u64 = 20;

/// `CLUS`'s head from its first 20 bytes, each count within its bound:
/// its clusters and their record size, its key pool's length, its chunks
/// and their record size; and how many bytes come before its chunks'
/// bytes.
pub(super) fn cluster_head(bytes: &[u8]) -> Result<([u32; 5], u64), SetError> {
    let head: &[u8; 20] = bytes.first_chunk().ok_or(SetError::Truncated)?;
    let [count, size, pool, chunks, chunk_size] = words(head);
    if count > SET_CLUSTERS_MAX {
        return Err(SetError::Bound("SET_CLUSTERS_MAX"));
    }
    if chunks > SET_CHUNKS_MAX {
        return Err(SetError::Bound("SET_CHUNKS_MAX"));
    }
    if size > SET_RECORD_BYTES_MAX || chunk_size > SET_RECORD_BYTES_MAX {
        return Err(SetError::Bound("SET_RECORD_BYTES_MAX"));
    }
    if size < CLUSTER_RECORD
        || chunk_size < CHUNK_RECORD
        || pool > count.saturating_mul(CLUSTER_SCALARS_MAX)
    {
        return Err(SetError::Malformed("a malformed cluster"));
    }
    // each product is under 2^32 by the bounds above
    let len = [(count, size), (pool, 4), (chunks, chunk_size)]
        .iter()
        .fold(CLUSTER_HEAD_BYTES, |n, &(a, b)| {
            n.saturating_add(u64::from(a).saturating_mul(u64::from(b)))
        });
    Ok(([count, size, pool, chunks, chunk_size], len))
}

/// Five little-endian u32s.
fn words(b: &[u8; 20]) -> [u32; 5] {
    let [
        a0,
        a1,
        a2,
        a3,
        b0,
        b1,
        b2,
        b3,
        c0,
        c1,
        c2,
        c3,
        d0,
        d1,
        d2,
        d3,
        e0,
        e1,
        e2,
        e3,
    ] = *b;
    [
        u32::from_le_bytes([a0, a1, a2, a3]),
        u32::from_le_bytes([b0, b1, b2, b3]),
        u32::from_le_bytes([c0, c1, c2, c3]),
        u32::from_le_bytes([d0, d1, d2, d3]),
        u32::from_le_bytes([e0, e1, e2, e3]),
    ]
}

/// `CLUS`'s records and key pool as stored, checked once and then read in
/// place.
#[derive(Default)]
pub(super) struct ClusterMap {
    bytes: Box<[u8]>,
    /// A record's size.
    size: usize,
    /// Where the key pool starts in `bytes`.
    pool: usize,
    /// The set's number for `CLUS`'s first chunk.
    first: usize,
}

impl ClusterMap {
    /// Record `i`: its strike and key length, its key's place in the pool,
    /// its glyph's chunk and offset, and its flags.
    fn record(&self, i: usize) -> Option<[usize; 6]> {
        let r = self.bytes.get(i.checked_mul(self.size)?..)?.first_chunk()?;
        let [head, key, chunk, offset, flags] = words(r).map(|w| w as usize);
        Some([head & 0xffff, head >> 16, key, chunk, offset, flags])
    }

    /// Record `i`'s key, scalar value by scalar value.
    fn key(&self, i: usize) -> Option<impl Iterator<Item = u32> + '_> {
        let [_, len, key, ..] = self.record(i)?;
        let start = self.pool.checked_add(key.checked_mul(4)?)?;
        let bytes = self
            .bytes
            .get(start..start.checked_add(len.checked_mul(4)?)?)?;
        Some(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&w| u32::from_le_bytes(w)),
        )
    }

    /// Record `i`'s key as characters; `None` past the records, the
    /// parser having checked each scalar value.
    pub(super) fn key_of(&self, i: usize) -> Option<Vec<char>> {
        self.key(i)?.map(char::from_u32).collect()
    }

    /// Where the glyph of the cluster `cluster` lies, among records
    /// `[lo, hi)`, sorted by key.
    pub(super) fn find(
        &self,
        (mut lo, mut hi): (usize, usize),
        cluster: &[char],
    ) -> Option<Located> {
        while lo < hi {
            let mid = lo.midpoint(hi);
            match self.key(mid)?.cmp(cluster.iter().map(|&c| u32::from(c))) {
                Ordering::Less => lo = mid.checked_add(1)?,
                Ordering::Greater => hi = mid,
                Ordering::Equal => {
                    let [.., chunk, offset, flags] = self.record(mid)?;
                    return Some(Located {
                        chunk: self.first.checked_add(chunk)?,
                        offset: offset as u64,
                        wide: flags != 0,
                        key: (INK_CLUSTER, u32::try_from(mid).ok()?),
                    });
                }
            }
        }
        None
    }
}

/// `CLUS`: its clusters, each of a strike the set holds, sorted by strike
/// and then by key with no key twice in a strike, each key 1 to
/// `CLUSTER_SCALARS_MAX` scalar values of the pool, and each glyph inside
/// one of the section's own chunks, numbered from `first`, the count of
/// the set's chunks before them; with those chunks.
pub(super) fn parse_clusters(
    bytes: &[u8],
    clus: (u64, u64),
    strikes: &mut [Strike],
    first: usize,
) -> Result<(ClusterMap, Vec<Chunk>), SetError> {
    let ([count, size, pool, chunks, chunk_size], len) = cluster_head(bytes)?;
    let (count, size) = (count as usize, size as usize);
    let records = count.saturating_mul(size);
    let end = records
        .saturating_add((pool as usize).saturating_mul(4))
        .saturating_add(20);
    let body = bytes.get(20..end).ok_or(SetError::Truncated)?;
    let mut c = Cursor(bytes.get(end..).unwrap_or_default());
    let chunks = chunk_table(
        &mut c,
        (chunks, chunk_size as usize),
        len,
        clus,
        largest_chunk(strikes),
    )?;
    let map = ClusterMap {
        bytes: body.into(),
        size,
        pool: records,
        first,
    };
    let scalars = body.get(records..).unwrap_or_default();
    let mut ok = scalars
        .as_chunks::<4>()
        .0
        .iter()
        .all(|&w| char::from_u32(u32::from_le_bytes(w)).is_some());
    for i in 0..count {
        let [strike, len, _, chunk, offset, flags] = map.record(i).ok_or(SetError::Truncated)?;
        if len == 0 || len > CLUSTER_SCALARS_MAX as usize {
            return Err(SetError::Bound("CLUSTER_SCALARS_MAX"));
        }
        let s = strikes.get_mut(strike);
        let fits = s
            .as_ref()
            .and_then(|s| s.size(flags == 1))
            .zip(chunks.get(chunk))
            .is_some_and(|(n, ch)| {
                // a chunk past the bound holds exactly this one glyph
                if ch.decoded > SET_CHUNK_BYTES {
                    offset == 0 && n == u64::from(ch.decoded)
                } else {
                    (offset as u64).saturating_add(n) <= u64::from(ch.decoded)
                }
            });
        // after the record before it: a later strike, or a greater key
        let after = match i.checked_sub(1).and_then(|p| Some((p, map.record(p)?[0]))) {
            None => true,
            Some((p, before)) if before == strike => {
                map.key(p).zip(map.key(i)).is_some_and(|(a, b)| a.lt(b))
            }
            Some((_, before)) => before < strike,
        };
        ok &= flags <= 1 && fits && after && map.key(i).is_some();
        if let Some(s) = s {
            s.clusters = (
                if s.clusters.1 == 0 { i } else { s.clusters.0 },
                i.saturating_add(1),
            );
        }
    }
    if !ok {
        return Err(SetError::Malformed("a malformed cluster"));
    }
    Ok((map, chunks))
}

impl Strike {
    /// Whether a glyph starts at offset `pos` of the strike's glyph bytes.
    fn boundary(&self, pos: u64) -> bool {
        self.glyph_at(pos).is_some()
    }

    /// The bytes of the glyph that starts at offset `pos` of the strike's
    /// glyph bytes; `None` when no glyph starts there.
    fn glyph_at(&self, pos: u64) -> Option<u64> {
        let span = self
            .spans
            .partition_point(|s| s.stored <= pos)
            .checked_sub(1)
            .and_then(|i| self.spans.get(i))?;
        let (size, rel) = (self.size(span.wide)?, pos.checked_sub(span.stored)?);
        (rel.checked_rem(size) == Some(0)
            && rel
                .checked_div(size)
                .is_some_and(|k| k < u64::from(span.len)))
        .then_some(size)
    }
}

/// The length of `INKX`'s fixed head.
const INK_HEAD_BYTES: usize = 16;

/// `INKX`'s records as stored, checked once and then read in place.
#[derive(Default)]
pub(super) struct InkMap {
    bytes: Box<[u8]>,
    size: usize,
    count: usize,
    /// The set's number for `INKX`'s first chunk.
    first: usize,
}

/// Where an ink box lies, and its place and size.
pub(super) struct InkAt {
    pub chunk: usize,
    /// Where the box's row at the top of its glyph's row lies in its
    /// chunk; its first row lies `-top` rows before it.
    pub offset: u64,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    /// `None` for a record written without the field, whose box is its
    /// strike's cell's height.
    pub height: Option<u32>,
}

/// An `INKX` record's fields: its strike, kind and number, its left and
/// width, its chunk and offset, and, where the record holds them, its top
/// and height.
type InkRecord = (u16, u16, u32, i32, u32, u32, u32, Option<(i32, u32)>);

impl InkMap {
    /// Record `i`. A record shorter than [`INK_RECORD`] was written before
    /// the box had a top and a height, and gives none.
    fn record(&self, i: usize) -> Option<InkRecord> {
        let r = self
            .bytes
            .get(i.checked_mul(self.size)?..)?
            .get(..self.size)?;
        let [s0, s1, k0, k1, g @ ..] = r else {
            return None;
        };
        let w = |at: usize| -> Option<[u8; 4]> { g.get(at..at.checked_add(4)?)?.try_into().ok() };
        let rows = match (w(20), w(24)) {
            (Some(top), Some(height)) if self.size >= INK_RECORD as usize => {
                Some((i32::from_le_bytes(top), u32::from_le_bytes(height)))
            }
            _ => None,
        };
        Some((
            u16::from_le_bytes([*s0, *s1]),
            u16::from_le_bytes([*k0, *k1]),
            u32::from_le_bytes(w(0)?),
            i32::from_le_bytes(w(4)?),
            u32::from_le_bytes(w(8)?),
            u32::from_le_bytes(w(12)?),
            u32::from_le_bytes(w(16)?),
            rows,
        ))
    }

    /// The ink box of strike `strike`'s glyph or cluster `key`.
    pub(super) fn find(&self, strike: usize, key: (u16, u32)) -> Option<InkAt> {
        let want = (u16::try_from(strike).ok()?, key.0, key.1);
        let (mut lo, mut hi) = (0, self.count);
        while lo < hi {
            let mid = lo.midpoint(hi);
            let (s, k, g, left, width, chunk, offset, rows) = self.record(mid)?;
            match (s, k, g).cmp(&want) {
                Ordering::Less => lo = mid.checked_add(1)?,
                Ordering::Greater => hi = mid,
                Ordering::Equal => {
                    return Some(InkAt {
                        chunk: self.first.checked_add(chunk as usize)?,
                        offset: u64::from(offset),
                        left,
                        top: rows.map_or(0, |r| r.0),
                        width,
                        height: rows.map(|r| r.1),
                    });
                }
            }
        }
        None
    }
}

/// `INKX` (ADR-1302 D6, ADR-1306 D3): its boxes, sorted by strike, kind
/// and number with none twice, each of a glyph or cluster the set holds in
/// an R8 or 1-bit strike, spanning its glyph's cells, past them on one
/// side at least and inside their reach, its bytes inside one of the
/// section's own chunks, numbered from `first`; with those chunks. A
/// record of [`INK_RECORD_MIN`] bytes, written before the box had a top and
/// a height, gives a box of the cell's height.
pub(super) fn parse_inks(
    bytes: &[u8],
    inkx: (u64, u64),
    strikes: &[Strike],
    clusters: &ClusterMap,
    first: usize,
) -> Result<(InkMap, Vec<Chunk>), SetError> {
    let head: &[u8; 16] = bytes.first_chunk().ok_or(SetError::Truncated)?;
    let word = |i: usize| -> u32 {
        head.as_chunks::<4>()
            .0
            .get(i)
            .map_or(u32::MAX, |&w| u32::from_le_bytes(w))
    };
    let (count, size, chunk_count, chunk_size) = (word(0), word(1), word(2), word(3));
    if count > SET_INKS_MAX {
        return Err(SetError::Bound("SET_INKS_MAX"));
    }
    if chunk_count > SET_CHUNKS_MAX {
        return Err(SetError::Bound("SET_CHUNKS_MAX"));
    }
    if size > SET_RECORD_BYTES_MAX || chunk_size > SET_RECORD_BYTES_MAX {
        return Err(SetError::Bound("SET_RECORD_BYTES_MAX"));
    }
    if size < INK_RECORD_MIN || chunk_size < CHUNK_RECORD {
        return Err(SetError::Malformed("a malformed ink box"));
    }
    let (count, size) = (count as usize, size as usize);
    let records = count.saturating_mul(size);
    let end = records.saturating_add(INK_HEAD_BYTES);
    let body = bytes.get(INK_HEAD_BYTES..end).ok_or(SetError::Truncated)?;
    let mut c = Cursor(bytes.get(end..).ok_or(SetError::Truncated)?);
    let at = u64::try_from(end)
        .map_err(|_| SetError::Truncated)?
        .saturating_add(u64::from(chunk_count).saturating_mul(u64::from(chunk_size)));
    let chunks = chunk_table(
        &mut c,
        (chunk_count, chunk_size as usize),
        at,
        inkx,
        SET_CHUNK_BYTES,
    )?;
    let map = InkMap {
        bytes: body.into(),
        size,
        count,
        first,
    };
    let cluster_count = clusters.pool.checked_div(clusters.size).unwrap_or(0);
    let mut before: Option<(u16, u16, u32)> = None;
    for i in 0..count {
        let (strike, kind, glyph, left, width, chunk, offset, rows) =
            map.record(i).ok_or(SetError::Truncated)?;
        let s = strikes
            .get(usize::from(strike))
            .ok_or(SetError::Malformed("an ink box of a strike the set lacks"))?;
        if s.info.format == PixelFormat::Rgba {
            return Err(SetError::Malformed("an ink box in a color strike"));
        }
        let wide = match kind {
            INK_GLYPH if glyph < s.info.glyph_count => s
                .spans
                .iter()
                .find(|sp| glyph.checked_sub(sp.glyph).is_some_and(|k| k < sp.len))
                .map(|sp| sp.wide),
            INK_CLUSTER if (glyph as usize) < cluster_count => clusters
                .record(glyph as usize)
                .filter(|r| r[0] == usize::from(strike))
                .map(|r| r[5] != 0),
            _ => None,
        }
        .ok_or(SetError::Malformed(
            "an ink box of a glyph the strike lacks",
        ))?;
        let cell = (i64::from(s.info.cell.width), i64::from(s.info.cell.height));
        let cells = (cell.0.saturating_mul(if wide { 2 } else { 1 }), cell.1);
        let (top, height) = rows.unwrap_or((0, s.info.cell.height));
        let boxed = crate::set::format::InkRect {
            left: i64::from(left),
            top: i64::from(top),
            width: i64::from(width),
            height: i64::from(height),
        };
        if !boxed.in_reach(cell, cells) {
            return Err(SetError::Malformed(
                "an ink box past its glyph's reach, or not past its cells",
            ));
        }
        let (start, n) = ink_span(s.info.format.code(), (top, width, height), offset)
            .ok_or(SetError::Malformed("an ink box outside its chunk"))?;
        let fits = chunks
            .get(chunk as usize)
            .is_some_and(|ch| start.saturating_add(u64::from(n)) <= u64::from(ch.decoded));
        if !fits {
            return Err(SetError::Malformed("an ink box outside its chunk"));
        }
        if before.is_some_and(|b| b >= (strike, kind, glyph)) {
            return Err(SetError::Malformed("ink boxes out of order, or one twice"));
        }
        before = Some((strike, kind, glyph));
    }
    Ok((map, chunks))
}

/// Where a box `(top, width, height)` of `format` starts in its chunk, its
/// row at the top of its glyph's row lying at `offset`, and how many bytes
/// it holds; `None` when it would start before the chunk or pass `u32`.
pub(super) fn ink_span(
    format: u8,
    (top, width, height): (i32, u32, u32),
    offset: u32,
) -> Option<(u64, u32)> {
    let row = glyph_bytes(format, width, 1, 1)?;
    if top > 0 {
        return None;
    }
    let above = u64::from(top.unsigned_abs()).checked_mul(u64::from(row))?;
    let start = u64::from(offset).checked_sub(above)?;
    Some((start, glyph_bytes(format, width, height, 1)?))
}
