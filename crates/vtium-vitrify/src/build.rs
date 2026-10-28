//! A set from a frit's parsed sources (ADR-1200 D1, D2, D4, D13): the
//! first source sets the cell, a bitmap font at its own cell and an
//! outline font at each `size=`; each later one fills only the codepoints
//! the earlier ones lack. A bitmap font is scaled by the largest whole
//! factor at which its cell fits the strike's, placed on the strike's
//! bottom row and centred across the cell, and one whose cell does not fit
//! at factor 1 fills nothing; an outline font is rasterised at the cell.
//! Each cell holds a strike a pixel format: 1-bit glyphs from bitmap
//! fonts, R8 from outlines and premultiplied RGBA from color glyphs.

use std::collections::{BTreeMap, BTreeSet};

use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder, VERSION, builtin};
use vtium_font::sprite::Range;
use vtium_font::{
    Cell, Coverage, Face, RangeCoverage, Sha256, SpritePolicy, Style, Width, sha256_hex,
};

use crate::child::Parser;
use crate::frit::{Frit, Source};
use crate::glyphs::Glyphs;
use crate::outline::{Outline, OutlineRequest};
use crate::sources::{Input, Resolved};

/// One source's glyphs at its own cell: 1-bit, each row padded to a byte,
/// one or two cells wide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub cell: Cell,
    /// The baseline's row from the cell's top, where the font gives one:
    /// a built-in set's strike does, a PSF font does not.
    pub baseline: Option<u32>,
    pub glyphs: Vec<(Width, Vec<u8>)>,
    /// Each codepoint and its glyph.
    pub map: BTreeMap<u32, u32>,
    /// What was decoded, for the provenance.
    pub format: String,
}

impl Font {
    /// A parsed bitmap font: every glyph one cell wide.
    pub fn from_glyphs(g: Glyphs) -> Font {
        let size = g.glyph_bytes().max(1);
        Font {
            cell: Cell {
                width: g.width,
                height: g.height,
            },
            baseline: None,
            glyphs: g
                .bits
                .chunks(size)
                .map(|rows| (Width::Narrow, rows.to_vec()))
                .collect(),
            map: g.map.into_iter().collect(),
            format: g.format,
        }
    }

    /// A built-in set's glyphs at its smallest cell, read through the
    /// set's own face.
    pub fn from_builtin(name: &str) -> Result<Font, String> {
        let set = builtin(name).ok_or_else(|| format!("this build holds no built-in {name}"))?;
        let cell = *set
            .cells()
            .first()
            .ok_or_else(|| format!("the built-in {name} has no strike"))?;
        let baseline = set
            .strikes()
            .find(|s| s.cell == cell && s.style == Style::default())
            .map(|s| s.baseline);
        let face = vtium_font::set::SetFace::new(set, cell.height)
            .ok_or_else(|| format!("the built-in {name} has no strike"))?;
        let mut font = Font {
            cell,
            baseline,
            glyphs: Vec::new(),
            map: BTreeMap::new(),
            format: format!("builtin:{name}"),
        };
        for c in (0..=0x10_ffff_u32).filter_map(char::from_u32) {
            if !face.has_glyph(c) {
                continue;
            }
            let Some(glyph) = face.draw_unscaled(&[c], Style::default()) else {
                continue;
            };
            let row = glyph.width().div_ceil(8) as usize;
            let mut bits = vec![0u8; row.saturating_mul(glyph.height() as usize)];
            for y in 0..glyph.height() {
                for x in 0..glyph.width() {
                    if glyph
                        .pixel(x, y)
                        .and_then(|p| p.first())
                        .is_some_and(|&a| a >= 0x80)
                    {
                        set_bit(&mut bits, row, x, y);
                    }
                }
            }
            let g = u32::try_from(font.glyphs.len()).map_err(|_| "too many glyphs".to_owned())?;
            font.glyphs.push((glyph.cells(), bits));
            font.map.insert(u32::from(c), g);
        }
        Ok(font)
    }
}

impl Font {
    /// The SHA-256 of the glyphs as read: the cell, then each glyph's
    /// width and rows, then each codepoint and its glyph. A built-in
    /// source is hashed this way, so a build whose built-in changed
    /// rebuilds the sets that take it.
    pub fn digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(&self.cell.width.to_le_bytes());
        h.update(&self.cell.height.to_le_bytes());
        h.update(&self.baseline.unwrap_or(u32::MAX).to_le_bytes());
        h.update(&(self.glyphs.len() as u64).to_le_bytes());
        for (width, bits) in &self.glyphs {
            h.update(&[u8::from(*width == Width::Wide)]);
            h.update(&(bits.len() as u64).to_le_bytes());
            h.update(bits);
        }
        for (c, g) in &self.map {
            h.update(&c.to_le_bytes());
            h.update(&g.to_le_bytes());
        }
        h.finish()
    }
}

fn bit(bits: &[u8], row: usize, x: u32, y: u32) -> bool {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    bits.get(at).is_some_and(|b| b & (0x80 >> (x % 8)) != 0)
}

fn set_bit(bits: &mut [u8], row: usize, x: u32, y: u32) {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    if let Some(b) = bits.get_mut(at) {
        *b |= 0x80 >> (x % 8);
    }
}

/// `glyph`, `width` cells wide in a font of `from`'s cell, scaled by
/// `scale` into a glyph of the same width at `cell`, its top `y0` rows
/// down, centred across.
fn place(glyph: &[u8], width: Width, from: Cell, cell: Cell, scale: u32, y0: u32) -> Vec<u8> {
    let cells = width.cells();
    let (src_w, dst_w) = (from.width * cells, cell.width * cells);
    let (src_row, dst_row) = (src_w.div_ceil(8) as usize, dst_w.div_ceil(8) as usize);
    let x0 = (dst_w - src_w * scale) / 2;
    let mut out = vec![0u8; dst_row * cell.height as usize];
    for y in 0..from.height * scale {
        for x in 0..src_w * scale {
            if bit(glyph, src_row, x / scale, y / scale) {
                set_bit(&mut out, dst_row, x0 + x, y0 + y);
            }
        }
    }
    out
}

/// The build key (ADR-1200 D4): the SHA-256 of the format version, the
/// tool's version, the frit's bytes and the SHA-256 of everything read for
/// each source, in order, or that the source was missing, so a set
/// rebuilds when any of them changes, a missing font that appears and a
/// tool that builds differently included.
pub fn build_key(frit: &Frit, sources: &[Option<Resolved>]) -> [u8; 32] {
    build_key_for(frit, sources, env!("CARGO_PKG_VERSION"))
}

/// [`build_key`] for the tool at version `tool`.
fn build_key_for(frit: &Frit, sources: &[Option<Resolved>], tool: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"vtium-vitrify build key\0");
    h.update(&VERSION.0.to_le_bytes());
    h.update(&VERSION.1.to_le_bytes());
    h.update(&(tool.len() as u64).to_le_bytes());
    h.update(tool.as_bytes());
    // a tool built to scale color bitmaps another way builds other bytes
    let scale = crate::resample::METHOD.name();
    h.update(&(scale.len() as u64).to_le_bytes());
    h.update(scale.as_bytes());
    h.update(&(frit.text.len() as u64).to_le_bytes());
    h.update(frit.text.as_bytes());
    for s in sources {
        let Some(s) = s else {
            h.update(b"missing\0");
            continue;
        };
        h.update(&s.sha256);
        for extra in [s.unimap_sha256, s.def_uni_sha256] {
            match extra {
                Some(sha) => {
                    h.update(&[1]);
                    h.update(&sha);
                }
                None => h.update(&[0]),
            }
        }
    }
    h.finish()
}

/// A source's glyphs as parsed: a bitmap font at its own cell, or an
/// outline font rasterised at the set's cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    Bitmap(Font),
    Outline(Outline),
}

impl Parsed {
    /// What was read, for the provenance.
    fn format(&self) -> &str {
        match self {
            Parsed::Bitmap(f) => &f.format,
            Parsed::Outline(o) => &o.format,
        }
    }
}

/// A source found and parsed: what it read, and its glyphs.
pub struct Part<'a> {
    pub source: &'a Source,
    pub resolved: &'a Resolved,
    pub parsed: Parsed,
}

/// The cells and baselines a set whose first source is `first` is built
/// at: a bitmap font's own cell, its baseline at the cell's bottom, or
/// each cell an outline font was rasterised at.
fn cells(frit: &Frit, first: &Parsed) -> Result<Vec<(Cell, u32)>, String> {
    match first {
        Parsed::Bitmap(font) => {
            if !frit.sizes.is_empty() && frit.sizes != [font.cell.height] {
                return Err(format!(
                    "size= is for a set whose first source is an outline font; this one's is a \
                     bitmap font, its cell {}x{}",
                    font.cell.width, font.cell.height
                ));
            }
            Ok(vec![(font.cell, font.baseline.unwrap_or(font.cell.height))])
        }
        Parsed::Outline(o) => Ok(o
            .strikes
            .iter()
            .map(|s| {
                let cell = Cell {
                    width: s.width,
                    height: s.height,
                };
                (cell, s.baseline)
            })
            .collect()),
    }
}

/// Parses each source of `frit` found (`resolved`, in the frit's order) in
/// the parser's child, a built-in as the tool holds it, and assembles the
/// set's bytes. An outline font that comes first is rasterised at each
/// `size=`, letting it set the cell; a later one at the cells the first
/// set, within its `cover=`.
pub fn parse_and_assemble(
    frit: &Frit,
    resolved: &[Option<Resolved>],
    key: &[u8; 32],
    parser: &Parser,
) -> Result<Vec<u8>, String> {
    let mut parts: Vec<Part<'_>> = Vec::new();
    let mut missing = Vec::new();
    for (source, resolved) in frit.sources.iter().zip(resolved) {
        let Some(resolved) = resolved else {
            missing.push(source);
            continue;
        };
        let parsed = match &resolved.input {
            Input::Font(request) => parser
                .parse(request)
                .map(|g| Parsed::Bitmap(Font::from_glyphs(g))),
            Input::Builtin(font) => Ok(Parsed::Bitmap(font.clone())),
            Input::Outline(request) => {
                let at: Vec<(u32, u32)> = match parts.first() {
                    Some(first) => cells(frit, &first.parsed)?
                        .iter()
                        .map(|(c, _)| (c.width, c.height))
                        .collect(),
                    None if frit.sizes.is_empty() => {
                        return Err(format!(
                            "source={}: an outline font that sets the cell is drawn at the \
                             heights size= names, and the frit names none",
                            source.spelling
                        ));
                    }
                    None => frit.sizes.iter().map(|&h| (0, h)).collect(),
                };
                let request = OutlineRequest {
                    font: std::sync::Arc::clone(&request.font),
                    family: request.family.clone(),
                    cells: at,
                    cover: source.cover.clone(),
                };
                parser.rasterise(&request).map(Parsed::Outline)
            }
        }
        .map_err(|e| format!("source={}: {e}", source.spelling))?;
        parts.push(Part {
            source,
            resolved,
            parsed,
        });
    }
    assemble(frit, &parts, &missing, key)
}

/// One cell's strikes, a pixel format each, and the codepoints they map.
struct Strikes {
    bits: StrikeBuilder,
    r8: StrikeBuilder,
    rgba: StrikeBuilder,
    mapped: BTreeSet<u32>,
}

impl Strikes {
    fn new(cell: Cell, baseline: u32, compress: bool) -> Strikes {
        let strike = |format| {
            let s = StrikeBuilder::new(format, cell, Style::default()).baseline(baseline);
            if compress { s } else { s.compress(false) }
        };
        Strikes {
            bits: strike(PixelFormat::Bits),
            r8: strike(PixelFormat::R8),
            rgba: strike(PixelFormat::Rgba),
            mapped: BTreeSet::new(),
        }
    }
}

/// The set `frit` describes, from the fonts of its sources found, in
/// order, the first setting the cell: its bytes. The sources left out are
/// named in the provenance.
pub fn assemble(
    frit: &Frit,
    parts: &[Part<'_>],
    missing: &[&Source],
    key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let main = parts
        .first()
        .ok_or("no source of the frit is on this machine")?;
    let mut set = SetBuilder::new(&frit.name)
        .meta("build-key", &sha256_hex(key))
        .meta("tool", concat!("vtium-vitrify ", env!("CARGO_PKG_VERSION")));
    let mut coverage = None;
    for (i, &(cell, baseline)) in cells(frit, &main.parsed)?.iter().enumerate() {
        let mut strikes = Strikes::new(cell, baseline, frit.compress);
        for (j, part) in parts.iter().enumerate() {
            match &part.parsed {
                Parsed::Bitmap(font) => fill(&mut strikes, font, part.source, cell, baseline),
                Parsed::Outline(o) => {
                    if let Some(s) = o.strikes.get(i) {
                        fill_outline(&mut strikes, s, part.source);
                    }
                }
            }
            // The sprite policy is the first font's: the sprites win over
            // a fill font's glyphs, so a set draws a range from its glyphs
            // only where its first font covers the range whole (the
            // operator's decision, 2026-10-28).
            if j == 0 {
                let mapped = &strikes.mapped;
                coverage.get_or_insert_with(|| {
                    Coverage::new(|range: Range| {
                        RangeCoverage::count(range, |c| mapped.contains(&u32::from(c)))
                    })
                });
            }
        }
        if strikes.mapped.is_empty() {
            return Err(format!(
                "the sources give the set no glyph at {}x{}",
                cell.width, cell.height
            ));
        }
        for strike in [strikes.bits, strikes.r8, strikes.rgba] {
            if !strike.is_empty() {
                set = set.strike(strike);
            }
        }
    }
    if let Some(coverage) = coverage {
        set = set.policy(SpritePolicy::default_for(&coverage));
    }
    for line in frit.text.lines() {
        set = set.meta("frit", line);
    }
    for part in parts {
        set = set
            .meta("source", &part.source.spelling)
            .meta("source-sha256", &sha256_hex(&part.resolved.sha256))
            .meta("source-format", part.parsed.format());
        if let Some(sha) = part.resolved.unimap_sha256 {
            set = set.meta("source-unimap-sha256", &sha256_hex(&sha));
        }
        if let Parsed::Outline(o) = &part.parsed {
            set = set.meta("source-family", &o.family);
            if !o.license.is_empty() {
                set = set.meta("source-license", &o.license);
            }
        }
    }
    for source in missing {
        set = set.meta("source-missing", &source.spelling);
    }
    if parts.iter().any(|p| matches!(p.parsed, Parsed::Outline(_))) {
        set = set.meta("color-scale", crate::resample::METHOD.name());
    }
    if let Some(license) = &frit.license {
        set = set.meta("license", license);
    }
    set.write().map_err(|e| e.to_string())
}

/// Adds an outline font's glyphs at one cell for the codepoints `source`
/// covers and the cell lacks: R8 glyphs to the R8 strike, color glyphs
/// to the RGBA strike.
fn fill_outline(strikes: &mut Strikes, outline: &crate::outline::Strike, source: &Source) {
    let mut placed: BTreeMap<(bool, u32), u32> = BTreeMap::new();
    for &(c, g) in &outline.map {
        let Some(ch) = char::from_u32(c) else {
            continue;
        };
        let Some(glyph) = outline.glyphs.get(g as usize) else {
            continue;
        };
        if strikes.mapped.contains(&c) || !source.covers(c) {
            continue;
        }
        let strike = if glyph.color {
            &mut strikes.rgba
        } else {
            &mut strikes.r8
        };
        let index = *placed.entry((glyph.color, g)).or_insert_with(|| {
            let width = if glyph.wide {
                Width::Wide
            } else {
                Width::Narrow
            };
            strike.glyph(width, glyph.pixels.clone())
        });
        strike.map(ch, index);
        strikes.mapped.insert(c);
    }
}

/// Adds `font`'s glyphs for the codepoints `source` covers and the cell
/// lacks to the 1-bit strike, placed at `cell`: its baseline on the
/// strike's `baseline` (ADR-1200 D1), kept inside the cell; a font that
/// gives no baseline on the cell's bottom row.
fn fill(strikes: &mut Strikes, font: &Font, source: &Source, cell: Cell, baseline: u32) {
    let scale = (cell.width / font.cell.width.max(1)).min(cell.height / font.cell.height.max(1));
    if scale == 0 {
        return;
    }
    let lowest = cell.height - font.cell.height * scale;
    let y0 = font
        .baseline
        .map_or(lowest, |b| baseline.saturating_sub(b * scale).min(lowest));
    let mut placed: BTreeMap<u32, u32> = BTreeMap::new();
    for (&c, &g) in &font.map {
        let Some(ch) = char::from_u32(c) else {
            continue;
        };
        if strikes.mapped.contains(&c) || !source.covers(c) {
            continue;
        }
        let index = match placed.get(&g) {
            Some(&i) => i,
            None => {
                let Some((width, bits)) = font.glyphs.get(g as usize) else {
                    continue;
                };
                let i = strikes
                    .bits
                    .glyph(*width, place(bits, *width, font.cell, cell, scale, y0));
                placed.insert(g, i);
                i
            }
        };
        strikes.bits.map(ch, index);
        strikes.mapped.insert(c);
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    // The operator's decision (2026-10-28): the sprites win over a fill
    // font's glyphs, so a set whose first font lacks box drawing, blocks,
    // shades and Braille draws those ranges as sprites with a built-in
    // after it, as it does alone.
    #[test]
    fn a_fills_glyphs_leave_the_sprite_policy_the_first_fonts() {
        use crate::sources::{Input, Resolved};
        use vtium_font::sprite::Range;
        // an 8x8 font holding `A` alone
        let main = Font::from_glyphs(Glyphs {
            width: 8,
            height: 8,
            count: 1,
            bits: vec![0xff; 8],
            map: vec![(0x41, 0)],
            format: "psf2".into(),
        });
        let unscii = Font::from_builtin("unscii-8").expect("unscii-8");
        let policy = |text: &[u8], fonts: Vec<Font>| {
            let frit = Frit::parse(text, std::path::Path::new("/t.vtfrit"), "t").unwrap();
            let resolved: Vec<Resolved> = fonts
                .iter()
                .map(|f| Resolved {
                    input: Input::Builtin(f.clone()),
                    sha256: f.digest(),
                    unimap_sha256: None,
                    def_uni_sha256: None,
                })
                .collect();
            let parts: Vec<Part<'_>> = frit
                .sources
                .iter()
                .zip(&resolved)
                .zip(fonts)
                .map(|((source, resolved), f)| Part {
                    source,
                    resolved,
                    parsed: Parsed::Bitmap(f),
                })
                .collect();
            let bytes = assemble(&frit, &parts, &[], &[0; 32]).unwrap();
            vtium_font::set::Set::from_bytes(bytes).unwrap().policy()
        };
        let alone = policy(b"source=file:/a.psf\n", vec![main.clone()]);
        let filled = policy(
            b"source=file:/a.psf\nsource=builtin:unscii-8\n",
            vec![main, unscii],
        );
        assert_eq!(filled, alone);
        for range in [Range::BoxDrawing, Range::BlockElements, Range::Braille] {
            assert_eq!(
                filled.source(range),
                vtium_font::Source::Sprites,
                "{range:?}"
            );
        }
    }

    #[test]
    fn a_smaller_glyph_is_scaled_onto_the_baseline_and_centred() {
        // a 2x2 glyph, its top-left pixel set, into a 6x6 cell: scale 2,
        // 1 column from the left, 2 rows from the top
        let from = Cell {
            width: 2,
            height: 2,
        };
        let cell = Cell {
            width: 6,
            height: 6,
        };
        let out = place(&[0x80, 0x00], Width::Narrow, from, cell, 2, 2);
        assert_eq!(out, [0, 0, 0b0110_0000, 0b0110_0000, 0, 0]);
        // two cells wide: 8 pixels into 12, 2 columns from the left
        let wide = place(&[0x80, 0x00], Width::Wide, from, cell, 2, 2);
        assert_eq!(wide.len(), 2 * 6);
        assert_eq!(&wide[4..8], &[0b0011_0000, 0, 0b0011_0000, 0]);
    }

    #[test]
    fn the_build_key_holds_the_tool_version() {
        let frit =
            crate::frit::Frit::parse(b"source=kbd:x\n", std::path::Path::new("/f.vtfrit"), "f")
                .unwrap();
        assert_ne!(
            build_key_for(&frit, &[], "0.0.0"),
            build_key_for(&frit, &[], "0.0.1")
        );
        assert_eq!(
            build_key(&frit, &[]),
            build_key_for(&frit, &[], env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn a_builtin_reads_as_a_font() {
        let f = Font::from_builtin("unscii-8").expect("unscii-8");
        assert_eq!((f.cell.width, f.cell.height), (8, 8));
        assert!(f.map.contains_key(&0x41));
        let u = Font::from_builtin("unifont").expect("unifont");
        assert_eq!((u.cell.width, u.cell.height), (8, 16));
        let wide = u.map[&0x4e00];
        assert_eq!(u.glyphs[wide as usize].0, Width::Wide);
        assert_eq!(u.glyphs[wide as usize].1.len(), 2 * 16);
    }
}
