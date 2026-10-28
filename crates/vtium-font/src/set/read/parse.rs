//! Each section's check: a set is parsed as untrusted input, every count
//! against its bound and every table against the others, before any glyph
//! is drawn from it (ADR-1200 D2).

use super::{Chunk, Cursor, PixelFormat, Run, SetError, Span, Strike, StrikeInfo, add, mul};
use crate::bounds::{
    CELL_SIDE_MAX, SET_CHUNK_BYTES, SET_CHUNKS_MAX, SET_GLYPHS_MAX, SET_RUNS_MAX, SET_STRIKES_MAX,
};
use crate::face::{Cell, Decorations, Style};
use crate::policy::{Coverage, RangeCoverage, Source, SpritePolicy};
use crate::set::format::{
    CHUNK_DEFLATED, CHUNK_RECORD, COVERAGE_RECORD, RUN_RECORD, RUN_WIDE, SOURCE_AUTO, SOURCE_FONT,
    SOURCE_SPRITES, STRIKE_RECORD, STYLE_BOLD, STYLE_ITALIC, TABLE_HEAD_BYTES, valid_name,
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

/// `STRK`: each strike's style, format, cell, metrics and glyph count,
/// sorted by cell height, width and style with no two alike, and a
/// regular strike for every cell size.
pub(super) fn parse_strikes(bytes: &[u8]) -> Result<Vec<Strike>, SetError> {
    let mut c = Cursor(bytes);
    let (count, size) = c.table(SET_STRIKES_MAX, "SET_STRIKES_MAX", STRIKE_RECORD)?;
    if count == 0 {
        return Err(SetError::Malformed("a set with no strikes"));
    }
    let mut strikes: Vec<Strike> = Vec::new();
    let mut keys: Vec<(u16, u16, u8)> = Vec::new();
    for _ in 0..count {
        let mut r = c.record(size)?;
        let style = r.u8()?;
        let format = r.u8()?;
        let reserved = r.u16()?;
        let (w, h) = (r.u16()?, r.u16()?);
        let baseline = r.u16()?;
        let (underline, underline_thickness) = (r.u16()?, r.u16()?);
        let (strike, strike_thickness) = (r.u16()?, r.u16()?);
        let reserved2 = r.u16()?;
        let glyph_count = r.u32()?;
        if reserved != 0 || reserved2 != 0 {
            return Err(SetError::Malformed("a reserved field that is not 0"));
        }
        if style & !(STYLE_BOLD | STYLE_ITALIC) != 0 {
            return Err(SetError::Malformed("an unknown style"));
        }
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
        keys.push((h, w, style));
        strikes.push(Strike {
            info: StrikeInfo {
                style: Style {
                    bold: style & STYLE_BOLD != 0,
                    italic: style & STYLE_ITALIC != 0,
                },
                format,
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
        });
    }
    c.end()?;
    if keys.windows(2).any(|w| matches!(w, [a, b] if a >= b)) {
        return Err(SetError::Malformed("strikes out of order or repeated"));
    }
    if keys.iter().any(|&(h, w, _)| !keys.contains(&(h, w, 0))) {
        return Err(SetError::Malformed("a cell size with no regular strike"));
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
            if glyph_stored > u64::from(SET_CHUNK_BYTES) {
                return Err(SetError::Bound("SET_CHUNK_BYTES"));
            }
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
    c.end()
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

/// `GLYF`'s index: chunks packed in order after it and filling the
/// section, each within `SET_CHUNK_BYTES`, and each strike's glyph bytes
/// cut into whole chunks on glyph boundaries, strike after strike.
pub(super) fn parse_index(
    bytes: &[u8],
    glyf: (u64, u64),
    strikes: &mut [Strike],
) -> Result<Vec<Chunk>, SetError> {
    let mut c = Cursor(bytes);
    let (count, size) = c.table(SET_CHUNKS_MAX, "SET_CHUNKS_MAX", CHUNK_RECORD)?;
    let size64 = u64::try_from(size).map_err(|_| SetError::Truncated)?;
    let mut at = add(u64::from(TABLE_HEAD_BYTES), mul(u64::from(count), size64)?)?;
    let mut chunks = Vec::new();
    for _ in 0..count {
        let mut r = c.record(size)?;
        let offset = r.u64()?;
        let (stored, decoded, flags) = (r.u32()?, r.u32()?, r.u32()?);
        let sha256 = r.array::<32>()?;
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
        if stored > SET_CHUNK_BYTES || decoded > SET_CHUNK_BYTES {
            return Err(SetError::Bound("SET_CHUNK_BYTES"));
        }
        if !deflated && stored != decoded {
            return Err(SetError::Malformed("a raw chunk whose lengths differ"));
        }
        at = add(at, u64::from(stored))?;
        chunks.push(Chunk {
            offset: add(glyf.0, offset)?,
            stored,
            decoded,
            deflated,
            sha256,
            strike: 0,
            start: 0,
        });
    }
    c.end()?;
    if at != glyf.1 {
        return Err(SetError::Malformed("GLYF's length is not its chunks'"));
    }
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

impl Strike {
    /// Whether a glyph starts at offset `pos` of the strike's glyph bytes.
    fn boundary(&self, pos: u64) -> bool {
        let Some(span) = self
            .spans
            .partition_point(|s| s.stored <= pos)
            .checked_sub(1)
            .and_then(|i| self.spans.get(i))
        else {
            return false;
        };
        let (Some(size), Some(rel)) = (self.size(span.wide), pos.checked_sub(span.stored)) else {
            return false;
        };
        rel.checked_rem(size) == Some(0)
            && rel
                .checked_div(size)
                .is_some_and(|k| k < u64::from(span.len))
    }
}
