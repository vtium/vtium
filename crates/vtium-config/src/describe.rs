//! What a font file is, as the font helper reads it in its sandbox: each
//! face's family, style and version, whether it is a bitmap or an outline
//! font, whether it is monospaced, the scripts its character map covers,
//! its bitmap strikes, and the well-known groups it belongs to, such as a
//! Nerd Fonts build or a color emoji font. vtium-config never parses a
//! font in its own process. It asks the helper for this description
//! ([`crate::serve::DescribeAsk`]) and shows what comes back, so the Fonts
//! tab can sort the fonts it finds and the Looks tab can list the sizes a
//! bitmap font supplies.
//!
//! The description travels as a frame of its own. [`Description::encode`]
//! writes it and [`Description::decode`] reads it back, checking every
//! count against its bound before anything is allocated. The decoder also
//! replaces each control character in a name, because a font file is
//! someone else's bytes and its names reach the person's terminal.
//!
//! References:
//! - ADR-1205 D3: vtium-config reads a font's family, coverage and sizes
//!   only through the sandboxed font helper, and the Fonts tab filters by
//!   bitmap or outline, monospaced, scripts and well-known groups.
//! - ADR-1205 D7: a bitmap font supplies its strikes, and an outline font
//!   any size.

use std::collections::BTreeMap;

use crate::frit::Cover;

/// How many faces of a collection a description holds at most. A real
/// collection holds a few dozen.
pub const FACES_MAX: usize = 64;
/// The most bytes a name in a description holds: a family, a style or a
/// version. A longer name is cut at a character's edge.
pub const TEXT_BYTES_MAX: usize = 256;
/// The most bitmap strikes a face lists.
pub const STRIKES_MAX: usize = 64;
/// The most ranges of codepoints a face's coverage lists. A face whose
/// coverage takes more ranges lists the first ones and says that the list
/// is cut.
pub const RANGES_MAX: usize = 4096;

/// The kind of file a description is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A TrueType or OpenType font, or a collection of them. Its faces
    /// may be outlines, bitmap strikes in an OpenType file, or both.
    OpenType,
    /// A PSF console font, as kbd installs them.
    Psf,
    /// A BDF bitmap font.
    Bdf,
    /// A font in the `.hex` format of GNU Unifont's tools.
    Hex,
}

impl Format {
    fn code(self) -> u32 {
        match self {
            Format::OpenType => 1,
            Format::Psf => 2,
            Format::Bdf => 3,
            Format::Hex => 4,
        }
    }

    fn of(code: u32) -> Option<Format> {
        Some(match code {
            1 => Format::OpenType,
            2 => Format::Psf,
            3 => Format::Bdf,
            4 => Format::Hex,
            _ => return None,
        })
    }

    /// The format's name, as a listing shows it.
    pub const fn name(self) -> &'static str {
        match self {
            Format::OpenType => "OpenType",
            Format::Psf => "PSF",
            Format::Bdf => "BDF",
            Format::Hex => "hex",
        }
    }
}

/// A well-known group a font belongs to, which the Fonts tab filters by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// A Nerd Fonts build: its family says so, or it maps the icon ranges
    /// that Nerd Fonts patches in.
    NerdFonts,
    /// It maps the four Powerline arrows, U+E0B0 to U+E0B3, that shell
    /// prompts draw.
    Powerline,
    /// It holds color glyphs, in a COLR, CBDT or sbix table, for emoji.
    ColorEmoji,
    /// It maps at least half of the box drawing characters, U+2500 to
    /// U+257F.
    BoxDrawing,
}

impl Group {
    /// Every group, in the order of its bit.
    pub const ALL: [Group; 4] = [
        Group::NerdFonts,
        Group::Powerline,
        Group::ColorEmoji,
        Group::BoxDrawing,
    ];

    /// The group's name, as a listing shows it.
    pub const fn name(self) -> &'static str {
        match self {
            Group::NerdFonts => "Nerd Fonts",
            Group::Powerline => "Powerline",
            Group::ColorEmoji => "color emoji",
            Group::BoxDrawing => "box drawing",
        }
    }

    fn bit(self) -> u32 {
        match self {
            Group::NerdFonts => 1,
            Group::Powerline => 2,
            Group::ColorEmoji => 4,
            Group::BoxDrawing => 8,
        }
    }
}

/// One bitmap strike: the size of a character cell it is drawn for, in
/// pixels. For a strike in an OpenType file, the height is its pixels to
/// the em and the width its widest glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Strike {
    pub width: u32,
    pub height: u32,
}

/// One face of a font file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    /// The family, as the font names it: its typographic family where it
    /// has one, else its family. A bitmap font that names none takes its
    /// file's name.
    pub family: String,
    /// The style, such as `Regular` or `Bold Italic`; empty when the font
    /// states none.
    pub style: String,
    /// The version the font states, where it states one.
    pub version: Option<String>,
    /// Whether the face has outlines, so it draws at any size. A face
    /// without them draws only at its strikes.
    pub outline: bool,
    /// Whether every printable ASCII character advances by the same width,
    /// as a terminal's text font needs.
    pub monospaced: bool,
    /// The scripts the face covers, by their names in Unicode's
    /// Scripts.txt, in [`SCRIPTS`]' order.
    pub scripts: Vec<&'static str>,
    /// The groups it belongs to.
    pub groups: Vec<Group>,
    /// Its bitmap strikes, smallest first.
    pub strikes: Vec<Strike>,
    /// How many codepoints it maps.
    pub codepoints: u32,
    /// The codepoints it maps, as ranges, at most [`RANGES_MAX`] of them.
    pub coverage: Vec<Cover>,
    /// Whether `coverage` stops at [`RANGES_MAX`] before the face's last
    /// codepoint.
    pub coverage_cut: bool,
}

/// What the helper says of one font file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description {
    pub format: Format,
    /// Its faces: one for a font, each face for a collection.
    pub faces: Vec<Face>,
}

/// A script and the letters that show a font covers it: the font covers
/// the script when it maps at least one in `share` of these codepoints.
/// The letters are a script's basic alphabet, so a Latin font with a few
/// Greek letters for mathematics does not count as covering Greek.
pub struct ScriptLetters {
    pub name: &'static str,
    pub letters: &'static [Cover],
    pub share: u32,
}

/// The scripts a description can name, each by its name in Unicode's
/// Scripts.txt. Han takes one in four of its first block, so a font with
/// one of the national character sets, such as GB 2312 or JIS X 0208,
/// counts. Hangul takes its compatibility letters, which every Korean
/// character set holds.
pub const SCRIPTS: [ScriptLetters; 27] = [
    script("Latin", &[(0x41, 0x5a), (0x61, 0x7a)], 2),
    script(
        "Greek",
        &[(0x391, 0x3a1), (0x3a3, 0x3a9), (0x3b1, 0x3c9)],
        2,
    ),
    script("Cyrillic", &[(0x410, 0x44f)], 2),
    script("Armenian", &[(0x531, 0x556), (0x561, 0x586)], 2),
    script("Hebrew", &[(0x5d0, 0x5ea)], 2),
    script("Arabic", &[(0x621, 0x64a)], 2),
    script("Devanagari", &[(0x905, 0x939)], 2),
    script("Bengali", &[(0x985, 0x9b9)], 2),
    script("Gurmukhi", &[(0xa05, 0xa39)], 2),
    script("Gujarati", &[(0xa85, 0xab9)], 2),
    script("Tamil", &[(0xb85, 0xbb9)], 2),
    script("Telugu", &[(0xc05, 0xc39)], 2),
    script("Kannada", &[(0xc85, 0xcb9)], 2),
    script("Malayalam", &[(0xd05, 0xd39)], 2),
    script("Sinhala", &[(0xd85, 0xdc6)], 2),
    script("Thai", &[(0xe01, 0xe2e)], 2),
    script("Lao", &[(0xe81, 0xeae)], 2),
    script("Tibetan", &[(0xf40, 0xf6c)], 2),
    script("Myanmar", &[(0x1000, 0x102a)], 2),
    script("Georgian", &[(0x10d0, 0x10fa)], 2),
    script("Ethiopic", &[(0x1200, 0x1248)], 2),
    script("Khmer", &[(0x1780, 0x17a2)], 2),
    script("Hiragana", &[(0x3041, 0x3096)], 2),
    script("Katakana", &[(0x30a1, 0x30fa)], 2),
    script("Hangul", &[(0x3131, 0x3163)], 2),
    script("Han", &[(0x4e00, 0x4fff)], 4),
    script("Braille", &[(0x2800, 0x28ff)], 2),
];

const fn script(name: &'static str, letters: &'static [Cover], share: u32) -> ScriptLetters {
    ScriptLetters {
        name,
        letters,
        share,
    }
}

/// The icon ranges Nerd Fonts patches into a font: Seti UI with Custom,
/// Devicons, Codicons and Font Awesome. A font that maps at least half of
/// them is a Nerd Fonts build whatever its family says.
const NERD_ICONS: [Cover; 4] = [
    (0xe5fa, 0xe6b7),
    (0xe700, 0xe8ef),
    (0xea60, 0xec1e),
    (0xf000, 0xf2ff),
];
/// The four Powerline arrows.
const POWERLINE: Cover = (0xe0b0, 0xe0b3);
/// Box drawing.
const BOX_DRAWING: Cover = (0x2500, 0x257f);
/// The emoji blocks from Miscellaneous Symbols and Pictographs to Symbols
/// and Pictographs Extended-A, one of which a color emoji font maps.
const EMOJI: Cover = (0x1f300, 0x1faff);

/// How many of `ranges`' codepoints `mapped` holds, and how many the
/// ranges hold.
fn share_of(mapped: &[u32], ranges: &[Cover]) -> (u32, u32) {
    let mut held = 0u32;
    let mut all = 0u32;
    for &(lo, hi) in ranges {
        all = all.saturating_add(hi.saturating_sub(lo).saturating_add(1));
        let start = mapped.partition_point(|&c| c < lo);
        let end = mapped.partition_point(|&c| c <= hi);
        held = held.saturating_add(u32::try_from(end.saturating_sub(start)).unwrap_or(u32::MAX));
    }
    (held, all)
}

/// The scripts `mapped`, sorted, covers.
fn scripts_of(mapped: &[u32]) -> Vec<&'static str> {
    SCRIPTS
        .iter()
        .filter(|s| {
            let (held, all) = share_of(mapped, s.letters);
            held > 0 && held.saturating_mul(s.share) >= all
        })
        .map(|s| s.name)
        .collect()
}

/// The groups a face with the codepoints `mapped`, sorted, the family
/// `family` and color glyphs or not belongs to.
fn groups_of(mapped: &[u32], family: &str, color: bool) -> Vec<Group> {
    let mut out = Vec::new();
    let lower = family.to_lowercase();
    let named =
        lower.contains("nerd font") || [" nf", " nfm", " nfp"].iter().any(|s| lower.ends_with(s));
    let (icons, all) = share_of(mapped, &NERD_ICONS);
    if named || icons.saturating_mul(2) >= all {
        out.push(Group::NerdFonts);
    }
    let (arrows, four) = share_of(mapped, &[POWERLINE]);
    if arrows == four {
        out.push(Group::Powerline);
    }
    if color && share_of(mapped, &[EMOJI]).0 > 0 {
        out.push(Group::ColorEmoji);
    }
    let (boxes, all) = share_of(mapped, &[BOX_DRAWING]);
    if boxes.saturating_mul(2) >= all {
        out.push(Group::BoxDrawing);
    }
    out
}

/// `mapped`, sorted and without repeats, as ranges: at most
/// [`RANGES_MAX`] of them, and whether the list stops early.
fn ranges_of(mapped: &[u32]) -> (Vec<Cover>, bool) {
    let mut out: Vec<Cover> = Vec::new();
    for &c in mapped {
        if let Some((_, hi)) = out.last_mut()
            && hi.checked_add(1) == Some(c)
        {
            *hi = c;
        } else if out.len() == RANGES_MAX {
            return (out, true);
        } else {
            out.push((c, c));
        }
    }
    (out, false)
}

/// `text` as a description holds it: each control character and each
/// character that turns text around or hides it replaced by U+FFFD, and
/// cut at a character's edge to [`TEXT_BYTES_MAX`] bytes.
pub fn clean(text: &str) -> String {
    let hidden = |c: char| {
        c.is_control()
            || matches!(
                c,
                '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'
            )
    };
    let mut out = String::new();
    for c in text.trim().chars() {
        let c = if hidden(c) { '\u{fffd}' } else { c };
        if out.len().saturating_add(c.len_utf8()) > TEXT_BYTES_MAX {
            break;
        }
        out.push(c);
    }
    out
}

/// A face as the helper describes it, from what it read.
struct Read<'a> {
    family: &'a str,
    style: &'a str,
    version: Option<&'a str>,
    outline: bool,
    monospaced: bool,
    color: bool,
    strikes: Vec<Strike>,
    mapped: Vec<u32>,
}

impl Read<'_> {
    fn face(mut self) -> Face {
        self.mapped.sort_unstable();
        self.mapped.dedup();
        let (coverage, coverage_cut) = ranges_of(&self.mapped);
        self.strikes.sort_unstable();
        self.strikes.dedup();
        self.strikes.truncate(STRIKES_MAX);
        Face {
            family: clean(self.family),
            style: clean(self.style),
            version: self.version.map(clean).filter(|v| !v.is_empty()),
            outline: self.outline,
            monospaced: self.monospaced,
            scripts: scripts_of(&self.mapped),
            groups: groups_of(&self.mapped, self.family, self.color),
            strikes: self.strikes,
            codepoints: u32::try_from(self.mapped.len()).unwrap_or(u32::MAX),
            coverage,
            coverage_cut,
        }
    }
}

/// The description of the font file whose bytes are `bytes` and whose
/// file name is `name`. The helper tells the format by the file's first
/// bytes, after it decompresses a compressed file, and by the name only
/// for a `.hex` font, which has no magic of its own. `Err` says why the
/// file is not a font that vtium reads.
pub fn describe(bytes: &[u8], name: &str) -> Result<Description, String> {
    if is_sfnt(bytes) {
        return opentype(bytes);
    }
    let (plain, _) = crate::decode::decompress(bytes)?;
    let stem = stem(name);
    if plain.starts_with(b"STARTFONT") {
        return bdf(bytes, &plain, stem);
    }
    if plain.starts_with(&[0x36, 0x04]) || plain.starts_with(&[0x72, 0xb5, 0x4a, 0x86]) {
        return psf(&plain, stem);
    }
    if name.ends_with(".hex") {
        let font = crate::hex::font(&plain)?;
        let face = bitmap_face(&font, stem, "", None, true);
        return Ok(Description {
            format: Format::Hex,
            faces: vec![face],
        });
    }
    Err("this file is not a font that vtium reads: not OpenType, PSF, BDF or .hex".to_owned())
}

/// The file's name less its directory, its compression's suffix and its
/// format's suffix: `ter-v16n` for `/x/ter-v16n.psf.gz`.
fn stem(name: &str) -> &str {
    let mut stem = name.rsplit('/').next().unwrap_or(name);
    for suffix in [".gz", ".bz2", ".xz", ".zst"] {
        stem = stem.strip_suffix(suffix).unwrap_or(stem);
    }
    for suffix in [".psfu", ".psf", ".bdf", ".hex"] {
        stem = stem.strip_suffix(suffix).unwrap_or(stem);
    }
    stem
}

/// Whether `bytes` start as a TrueType or OpenType font or collection.
fn is_sfnt(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some([0, 1, 0, 0] | b"OTTO" | b"ttcf" | b"true")
    )
}

/// A bitmap font's one face, at its own cell.
fn bitmap_face(
    font: &crate::font::Font,
    family: &str,
    style: &str,
    version: Option<&str>,
    monospaced: bool,
) -> Face {
    Read {
        family,
        style,
        version,
        outline: false,
        monospaced,
        color: false,
        strikes: vec![Strike {
            width: font.cell.width,
            height: font.cell.height,
        }],
        mapped: font.map.keys().copied().collect(),
    }
    .face()
}

/// A PSF font, named by its file. A font with no Unicode table of its own
/// takes its codepoints from the unimap its frit names, which the file
/// alone does not tell, so its description gives its cell from the PSF
/// header and no coverage.
fn psf(plain: &[u8], stem: &str) -> Result<Description, String> {
    let face = match crate::decode::font(plain, None, None) {
        Ok(glyphs) => bitmap_face(
            &crate::font::Font::from_glyphs(glyphs),
            stem,
            "",
            None,
            true,
        ),
        Err(why) => {
            let (width, height) = psf_cell(plain).ok_or(why)?;
            Read {
                family: stem,
                style: "",
                version: None,
                outline: false,
                monospaced: true,
                color: false,
                strikes: vec![Strike { width, height }],
                mapped: Vec::new(),
            }
            .face()
        }
    };
    Ok(Description {
        format: Format::Psf,
        faces: vec![face],
    })
}

/// The cell a PSF header states: PSF1's height at byte 3 and its width of
/// 8, or PSF2's height and width at bytes 24 and 28.
fn psf_cell(plain: &[u8]) -> Option<(u32, u32)> {
    let word = |at: usize| Some(u32::from_le_bytes(plain.get(at..at + 4)?.try_into().ok()?));
    let cell = if plain.starts_with(&[0x36, 0x04]) {
        (8, u32::from(*plain.get(3)?))
    } else {
        (word(28)?, word(24)?)
    };
    (cell.0 > 0 && cell.1 > 0).then_some(cell)
}

/// The properties of a BDF font that a description takes, read before its
/// first glyph: `FAMILY_NAME`, `WEIGHT_NAME`, `SLANT` and `SPACING`, each
/// with its quotes dropped. A line that is not UTF-8 is passed over.
fn bdf_properties(plain: &[u8]) -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    for line in plain.split(|&b| b == b'\n') {
        let Ok(line) = std::str::from_utf8(line).map(str::trim) else {
            continue;
        };
        if line.starts_with("ENDPROPERTIES") || line.starts_with("CHARS ") {
            break;
        }
        for key in ["FAMILY_NAME", "WEIGHT_NAME", "SLANT", "SPACING"] {
            if let Some(value) = line.strip_prefix(key)
                && value.starts_with(char::is_whitespace)
            {
                out.insert(key, value.trim().trim_matches('"').to_owned());
            }
        }
    }
    out
}

/// A BDF font: its family from `FAMILY_NAME`, else its file's name, and
/// its style from its weight and slant. It is monospaced unless its
/// `SPACING` says proportional.
fn bdf(bytes: &[u8], plain: &[u8], stem: &str) -> Result<Description, String> {
    let (font, version) = crate::bdf::read(bytes, true)?;
    let props = bdf_properties(plain);
    let family = props
        .get("FAMILY_NAME")
        .map_or(stem, String::as_str)
        .to_owned();
    let slant = match props.get("SLANT").map(String::as_str) {
        Some("I") => "Italic",
        Some("O") => "Oblique",
        _ => "",
    };
    let style = [props.get("WEIGHT_NAME").map_or("", String::as_str), slant]
        .iter()
        .filter(|w| !w.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let monospaced = !matches!(props.get("SPACING").map(String::as_str), Some("P" | "p"));
    Ok(Description {
        format: Format::Bdf,
        faces: vec![bitmap_face(
            &font,
            &family,
            &style,
            version.as_deref(),
            monospaced,
        )],
    })
}

/// An OpenType font or collection: each face, at most [`FACES_MAX`].
fn opentype(bytes: &[u8]) -> Result<Description, String> {
    let fonts = swash::FontDataRef::new(bytes).ok_or("not a TrueType or OpenType font")?;
    let mut faces = Vec::new();
    for index in 0..fonts.len().min(FACES_MAX) {
        let Some(font) = fonts.get(index) else {
            continue;
        };
        faces.push(opentype_face(bytes, index, font));
    }
    if faces.is_empty() {
        return Err("this OpenType file holds no face".to_owned());
    }
    Ok(Description {
        format: Format::OpenType,
        faces,
    })
}

/// One face of an OpenType file.
fn opentype_face(bytes: &[u8], index: usize, font: swash::FontRef<'_>) -> Face {
    use swash::StringId;
    let tag = |t: &[u8; 4]| font.table(u32::from_be_bytes(*t)).is_some();
    let strings = font.localized_strings();
    let text = |ids: &[StringId]| {
        ids.iter()
            .find_map(|&id| strings.find_by_id(id, Some("en")))
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    let family = text(&[StringId::TypographicFamily, StringId::Family]);
    let style = text(&[StringId::TypographicSubFamily, StringId::SubFamily]);
    let version = text(&[StringId::Version]);
    let mut mapped = Vec::new();
    let mut glyphs = Vec::new();
    font.charmap().enumerate(|c, g| {
        let control = c < 0x20 || (0x7f..0xa0).contains(&c);
        if g != 0 && !control && char::from_u32(c).is_some() {
            mapped.push(c);
            if glyphs.len() < OUTLINE_PROBES {
                glyphs.push(g);
            }
        }
    });
    let charmap = font.charmap();
    let advances = font.glyph_metrics(&[]);
    let mut widths = (0x21u32..0x7f)
        .map(|c| charmap.map(c))
        .filter(|&g| g != 0)
        .map(|g| advances.advance_width(g).to_bits());
    let first = widths.next();
    let even = first.is_some() && widths.all(|w| Some(w) == first);
    Read {
        family: &family,
        style: &style,
        version: Some(&version),
        outline: tag(b"CFF ") || tag(b"CFF2") || glyf_outlines(bytes, index, &glyphs),
        monospaced: font.metrics(&[]).is_monospace || even,
        color: tag(b"COLR") || tag(b"CBDT") || tag(b"sbix"),
        strikes: strikes(bytes, index),
        mapped,
    }
    .face()
}

/// How many of a face's mapped glyphs [`glyf_outlines`] looks at.
const OUTLINE_PROBES: usize = 64;

/// Whether the face at `index` of an OpenType file draws `glyphs`, some
/// of its mapped glyphs, from outlines in its `glyf` table. An OpenType
/// bitmap font, such as Terminus's `.otb` files, carries a `glyf` table
/// whose glyphs are all empty, so the table alone does not make an
/// outline font: at least one of these glyphs must hold an outline.
fn glyf_outlines(bytes: &[u8], index: usize, glyphs: &[u16]) -> bool {
    use skrifa::raw::{FileRef, TableProvider, types::GlyphId};
    let Ok(file) = FileRef::new(bytes) else {
        return false;
    };
    let Some(Ok(font)) = file.fonts().nth(index) else {
        return false;
    };
    let (Ok(glyf), Ok(loca)) = (font.glyf(), font.loca(None)) else {
        return false;
    };
    glyphs
        .iter()
        .any(|&g| matches!(loca.get_glyf(GlyphId::from(g), &glyf), Ok(Some(_))))
}

/// The bitmap strikes of the face at `index` of an OpenType file: each
/// size its EBLC or CBLC table lists, and each strike of its sbix table.
fn strikes(bytes: &[u8], index: usize) -> Vec<Strike> {
    use skrifa::raw::{FileRef, TableProvider};
    let Ok(file) = FileRef::new(bytes) else {
        return Vec::new();
    };
    let Some(Ok(font)) = file.fonts().nth(index) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut take = |sizes: &[skrifa::raw::tables::bitmap::BitmapSize]| {
        for size in sizes.iter().take(STRIKES_MAX) {
            out.push(Strike {
                width: u32::from(size.hori().width_max()),
                height: u32::from(size.ppem_y()),
            });
        }
    };
    if let Ok(eblc) = font.eblc() {
        take(eblc.bitmap_sizes());
    }
    if let Ok(cblc) = font.cblc() {
        take(cblc.bitmap_sizes());
    }
    if let Ok(sbix) = font.sbix() {
        for strike in sbix.strikes().iter().take(STRIKES_MAX).flatten() {
            let ppem = u32::from(strike.ppem());
            out.push(Strike {
                width: ppem,
                height: ppem,
            });
        }
    }
    out
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn put_text(out: &mut Vec<u8>, text: &str) {
    put32(out, len32(text.len()));
    out.extend_from_slice(text.as_bytes());
}

/// Little-endian fields read from a frame's payload, each count checked
/// against its bound before anything is allocated for it.
struct Fields<'a>(&'a [u8]);

impl Fields<'_> {
    fn u32(&mut self) -> Result<u32, String> {
        let (head, rest) = self
            .0
            .split_first_chunk::<4>()
            .ok_or("a font description cut short")?;
        self.0 = rest;
        Ok(u32::from_le_bytes(*head))
    }

    fn count(&mut self, max: usize, what: &str) -> Result<usize, String> {
        let n = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        if n > max {
            return Err(format!("a font description with {what} past {max}"));
        }
        Ok(n)
    }

    fn text(&mut self) -> Result<String, String> {
        let n = self.count(TEXT_BYTES_MAX, "a name")?;
        let (bytes, rest) = self
            .0
            .split_at_checked(n)
            .ok_or("a font description cut short")?;
        self.0 = rest;
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "a font description's name that is not UTF-8")?;
        Ok(clean(text))
    }
}

impl Description {
    /// The description's bytes, as [`crate::serve::FRAME_DESCRIPTION`]
    /// carries them.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put32(&mut out, self.format.code());
        put32(&mut out, len32(self.faces.len()));
        for face in &self.faces {
            put_text(&mut out, &face.family);
            put_text(&mut out, &face.style);
            put32(&mut out, u32::from(face.version.is_some()));
            put_text(&mut out, face.version.as_deref().unwrap_or_default());
            let flags = u32::from(face.outline)
                | u32::from(face.monospaced) << 1
                | u32::from(face.coverage_cut) << 2;
            put32(&mut out, flags);
            put32(
                &mut out,
                face.groups.iter().fold(0, |bits, g| bits | g.bit()),
            );
            put32(&mut out, face.codepoints);
            put32(&mut out, len32(face.scripts.len()));
            for name in &face.scripts {
                let at = SCRIPTS.iter().position(|s| s.name == *name).unwrap_or(0);
                put32(&mut out, len32(at));
            }
            put32(&mut out, len32(face.strikes.len()));
            for s in &face.strikes {
                put32(&mut out, s.width);
                put32(&mut out, s.height);
            }
            put32(&mut out, len32(face.coverage.len()));
            for &(lo, hi) in &face.coverage {
                put32(&mut out, lo);
                put32(&mut out, hi);
            }
        }
        out
    }

    /// Reads a description from a frame's payload. Each count is checked
    /// against its bound, each script and group against the known ones,
    /// and each range against the last, so what the helper sends cannot
    /// make vtium-config allocate past the bounds or show a control
    /// character.
    pub fn decode(payload: &[u8]) -> Result<Description, String> {
        let mut f = Fields(payload);
        let format = Format::of(f.u32()?).ok_or("a font description of an unknown format")?;
        let n = f.count(FACES_MAX, "faces")?;
        let mut faces = Vec::with_capacity(n);
        for _ in 0..n {
            let family = f.text()?;
            let style = f.text()?;
            let has_version = f.u32()?;
            let version = f.text()?;
            let version = match has_version {
                0 => None,
                1 => Some(version),
                _ => return Err("a font description with a malformed version".to_owned()),
            };
            let flags = f.u32()?;
            if flags & !7 != 0 {
                return Err("a font description with unknown flags".to_owned());
            }
            let bits = f.u32()?;
            let groups: Vec<Group> = Group::ALL
                .into_iter()
                .filter(|g| bits & g.bit() != 0)
                .collect();
            if groups.iter().fold(0, |b, g| b | g.bit()) != bits {
                return Err("a font description with an unknown group".to_owned());
            }
            let codepoints = f.u32()?;
            let ns = f.count(SCRIPTS.len(), "scripts")?;
            let mut scripts = Vec::with_capacity(ns);
            for _ in 0..ns {
                let at = usize::try_from(f.u32()?).unwrap_or(usize::MAX);
                let script = SCRIPTS
                    .get(at)
                    .ok_or("a font description with an unknown script")?;
                scripts.push(script.name);
            }
            let nk = f.count(STRIKES_MAX, "strikes")?;
            let mut strikes = Vec::with_capacity(nk);
            for _ in 0..nk {
                strikes.push(Strike {
                    width: f.u32()?,
                    height: f.u32()?,
                });
            }
            let nr = f.count(RANGES_MAX, "ranges")?;
            let mut coverage: Vec<Cover> = Vec::with_capacity(nr);
            for _ in 0..nr {
                let (lo, hi) = (f.u32()?, f.u32()?);
                let after = coverage.last().is_none_or(|&(_, last)| lo > last);
                if lo > hi || hi > 0x10ffff || !after {
                    return Err("a font description with a malformed range".to_owned());
                }
                coverage.push((lo, hi));
            }
            faces.push(Face {
                family,
                style,
                version,
                outline: flags & 1 != 0,
                monospaced: flags & 2 != 0,
                coverage_cut: flags & 4 != 0,
                scripts,
                groups,
                strikes,
                codepoints,
                coverage,
            });
        }
        if !f.0.is_empty() {
            return Err("a font description with bytes after its last face".to_owned());
        }
        Ok(Description { format, faces })
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use vtium_testfont::ttf;

    #[test]
    fn an_outline_font_is_described_by_its_names_coverage_and_groups() {
        let mut font = ttf::test_mono();
        font.cmap.extend(
            (0x2500..=0x257f)
                .map(|c| (c, 2))
                .chain((0xe0b0..=0xe0b3).map(|c| (c, 2))),
        );
        let d = describe(&font.build_with_version("Version 2.5"), "/f/test.ttf").unwrap();
        assert_eq!(d.format, Format::OpenType);
        let face = &d.faces[0];
        assert_eq!(face.family, "Test Mono");
        assert_eq!(face.version.as_deref(), Some("Version 2.5"));
        assert!(face.outline);
        assert!(face.monospaced, "A and W both advance 600");
        assert_eq!(
            face.groups,
            [Group::Powerline, Group::ColorEmoji, Group::BoxDrawing]
        );
        // two Latin letters are not an alphabet, and one ideograph is not Han
        assert!(face.scripts.is_empty(), "{:?}", face.scripts);
        assert_eq!(face.codepoints, 6 + 128 + 4);
        assert!(face.coverage.contains(&(0x2500, 0x257f)));
    }

    /// A font whose glyphs are all empty and whose pictures are bitmap
    /// strikes, as an OpenType bitmap font or a color emoji font is, draws
    /// only at its strikes.
    #[test]
    fn a_font_of_bitmap_strikes_alone_is_no_outline_font() {
        let d = describe(&ttf::test_emoji().build(), "emoji.ttf").unwrap();
        let face = &d.faces[0];
        assert!(!face.outline);
        assert_eq!(
            face.strikes.iter().map(|s| s.height).collect::<Vec<_>>(),
            [40]
        );
        assert_eq!(face.groups, [Group::ColorEmoji]);
    }

    #[test]
    fn a_font_covers_a_script_when_it_maps_half_its_letters() {
        let latin: Vec<u32> = (0x41..=0x5a).chain(0x61..=0x6d).collect();
        assert_eq!(scripts_of(&latin), ["Latin"]);
        let few: Vec<u32> = (0x41..=0x50).collect();
        assert!(scripts_of(&few).is_empty());
        let han: Vec<u32> = (0x4e00..0x4e80).collect();
        assert_eq!(scripts_of(&han), ["Han"]);
    }

    #[test]
    fn a_nerd_fonts_build_is_known_by_its_name_or_its_icons() {
        assert_eq!(
            groups_of(&[], "JetBrainsMono Nerd Font", false),
            [Group::NerdFonts]
        );
        assert_eq!(groups_of(&[], "Hack NFM", false), [Group::NerdFonts]);
        let icons: Vec<u32> = NERD_ICONS.iter().flat_map(|&(a, b)| a..=b).collect();
        assert_eq!(groups_of(&icons, "Plain", false), [Group::NerdFonts]);
        assert!(groups_of(&[0xf000], "Plain", false).is_empty());
    }

    #[test]
    fn a_psf_font_is_one_strike_named_by_its_file() {
        let psf = crate::decode::tests::psf2(8, 16, 256, Some(&[(65, 'A')]));
        let d = describe(&psf, "/usr/share/kbd/consolefonts/ter-v16n.psf.gz").unwrap();
        assert_eq!(d.format, Format::Psf);
        let face = &d.faces[0];
        assert_eq!(face.family, "ter-v16n");
        assert!(!face.outline);
        assert_eq!(
            face.strikes,
            [Strike {
                width: 8,
                height: 16
            }]
        );
        assert_eq!(face.coverage, [(0x41, 0x41)]);
        // a font with no table of its own still gives its cell
        let bare = crate::decode::tests::psf2(9, 18, 256, None);
        let d = describe(&bare, "x.psfu").unwrap();
        assert_eq!(
            d.faces[0].strikes,
            [Strike {
                width: 9,
                height: 18
            }]
        );
        assert_eq!(d.faces[0].codepoints, 0);
    }

    #[test]
    fn a_bdf_font_gives_its_family_style_and_spacing() {
        let bdf = "STARTFONT 2.1\nFONT -x-Small-Bold-R-Normal--8-80-75-75-P-50-ISO10646-1\nSIZE 8 75 75\nFONTBOUNDINGBOX 5 8 0 -1\nSTARTPROPERTIES 4\nFAMILY_NAME \"Small\"\nWEIGHT_NAME \"Bold\"\nSPACING \"P\"\nFONT_VERSION \"1.2\"\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nSWIDTH 500 0\nDWIDTH 5 0\nBBX 5 8 0 -1\nBITMAP\n00\n00\n00\n00\n00\n00\n00\n00\nENDCHAR\nENDFONT\n";
        let d = describe(bdf.as_bytes(), "small.bdf").unwrap();
        let face = &d.faces[0];
        assert_eq!(
            (face.family.as_str(), face.style.as_str()),
            ("Small", "Bold")
        );
        assert_eq!(face.version.as_deref(), Some("1.2"));
        assert!(!face.monospaced);
        assert_eq!(
            face.strikes,
            [Strike {
                width: 5,
                height: 8
            }]
        );
    }

    #[test]
    fn a_file_that_is_no_font_is_refused() {
        let e = describe(b"hello", "notes.txt").unwrap_err();
        assert!(e.contains("not a font that vtium reads"), "{e}");
    }

    #[test]
    fn a_description_reads_back_as_itself() {
        let d = describe(&ttf::test_mono().build(), "t.ttf").unwrap();
        assert_eq!(Description::decode(&d.encode()), Ok(d));
    }

    #[test]
    fn a_name_with_control_characters_is_cleaned_both_ways() {
        let mut font = ttf::test_mono();
        font.family = "Evil\u{1b}[2J\u{202e}Font".to_owned();
        let d = describe(&font.build(), "t.ttf").unwrap();
        assert_eq!(d.faces[0].family, "Evil\u{fffd}[2J\u{fffd}Font");
        // a helper that sent the raw name is cleaned on vtium-config's side
        let mut raw = d.clone();
        raw.faces[0].family = "a\u{7}b".to_owned();
        assert_eq!(
            Description::decode(&raw.encode()).unwrap().faces[0].family,
            "a\u{fffd}b"
        );
    }

    #[test]
    fn a_description_past_its_bounds_is_refused() {
        let d = describe(&ttf::test_mono().build(), "t.ttf").unwrap();
        let mut bytes = d.encode();
        // the face count
        bytes[4..8].copy_from_slice(&u32::try_from(FACES_MAX + 1).unwrap().to_le_bytes());
        assert!(Description::decode(&bytes).is_err());
        let mut bytes = d.encode();
        bytes.push(0);
        assert!(Description::decode(&bytes).is_err());
        assert!(Description::decode(&bytes[..bytes.len() - 3]).is_err());
    }
}
