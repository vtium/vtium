//! Frits (ADR-1200 D1): `/etc/vtium/fonts/<name>.vtfrit`, in
//! vtium.conf's line syntax, read strictly. One `key=value` a line, blanks
//! around the key and the value dropped, `#` to the line's end a comment.
//! An unknown key, a malformed line, a value out of range or a key this
//! build does not build yet refuses the whole frit, naming the file and
//! the line, since the helper runs unattended and a half-read frit draws
//! the wrong glyphs.

use std::fmt;
use std::path::{Path, PathBuf};

use vtium_font::spacing::{Fit, LINE_SPACING_MAX};
use vtium_font::sprite::Range;
use vtium_font::{Cell, Style};

/// The most bytes a frit holds.
pub const FRIT_BYTES_MAX: usize = 64 * 1024;
/// The most bytes one line holds.
pub const FRIT_LINE_MAX: usize = 4096;
/// The most sources a frit names.
pub const FRIT_SOURCES_MAX: usize = 16;
/// The smallest and largest side of the cell `cell=` states, in pixels,
/// from the key table, whose look's `cell` takes the same bounds.
pub use vtium_settings::keys::{FRIT_CELL_MAX, FRIT_CELL_MIN};
/// The smallest and largest size `size=` names, in points, from the key
/// table, which the console reads it within too.
pub use vtium_settings::keys::{FRIT_SIZE_MAX, FRIT_SIZE_MIN};
/// The heights `cp-size=` picks among a CPI font's, as `setfont`'s `-8`,
/// `-14` and `-16` do.
pub const CP_SIZES: [u32; 3] = [8, 14, 16];

/// Set names a frit may not take: the built-in sets'.
pub const RESERVED: [&str; 2] = ["unifont", "unscii-8"];

/// Why a frit was refused: its file, the line from 1 (0 for the whole
/// file) and the reason.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FritError {
    pub file: PathBuf,
    pub line: usize,
    pub why: String,
}

impl fmt::Display for FritError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.file.display(), self.why)
        } else {
            write!(f, "{}:{}: {}", self.file.display(), self.line, self.why)
        }
    }
}

impl std::error::Error for FritError {}

/// Where a source's glyphs come from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Origin {
    /// A kbd console font by name, found as `setfont` finds it.
    Kbd(String),
    /// A font by family, matched on the machine that draws it, as
    /// fontconfig matches a family: case and blanks ignored.
    Font(String),
    /// A bitmap font file by its absolute path.
    File(PathBuf),
    /// A BDF font file by its absolute path, read by vtium-config's own
    /// reader (ADR-1200 D14).
    Bdf(PathBuf),
    /// An outline font file, TrueType or OpenType or a collection of
    /// them, by its absolute path.
    Outline(PathBuf),
    /// `FONT=` and `FONT_UNIMAP=` from vconsole.conf and the kernel's
    /// command line.
    Vconsole,
    /// The font the kernel holds for a VT.
    Console(u16),
    /// A built-in font: `unifont` or `unscii-8`.
    Builtin(String),
}

/// A range of codepoints a source is limited to, both ends included.
pub type Cover = (u32, u32);

/// One `source=` line and the keys that follow it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Source {
    /// The value as the frit spelled it.
    pub spelling: String,
    pub origin: Origin,
    /// `unimap=`: a kbd unimap name or an absolute path.
    pub unimap: Option<String>,
    /// `cover=`: the union of its lines' ranges, sorted and merged; empty
    /// for every glyph the source holds.
    pub cover: Vec<Cover>,
    /// `cp-size=`: the height of the CPI font's three to take.
    pub cp_size: Option<u32>,
    /// `style=`: the style the source draws, regular unless the frit names
    /// another (ADR-1200 D1, D21).
    pub style: Style,
    /// `fit=`: how each of the source's sets uses a row made taller by
    /// line spacing (ADR-1302 D13), in the frit's order.
    pub fits: Vec<SetFit>,
    /// The `source=` line, from 1.
    pub line: usize,
}

/// One `fit=` line: the set it is for and its value. The set is the
/// whole source when the line follows the `source=` line, or the ranges
/// of the `cover=` line it follows.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SetFit {
    /// The `cover=` line's ranges, or `None` for the whole source.
    pub cover: Option<Vec<Cover>>,
    pub fit: Fit,
    /// The `source=` or `cover=` line the set comes from, from 1.
    pub set_line: usize,
}

impl Source {
    /// Whether `cover=` lets the source give `c`.
    pub fn covers(&self, c: u32) -> bool {
        if self.cover.is_empty() {
            return true;
        }
        let after = self.cover.partition_point(|&(a, _)| a <= c);
        after
            .checked_sub(1)
            .and_then(|i| self.cover.get(i))
            .is_some_and(|&(_, b)| c <= b)
    }

    /// How the source's set holding `c` uses a taller row: the `fit=` of
    /// a `cover=` line whose ranges hold `c`, else the source's own
    /// `fit=`, else `None`, and `c` takes its default
    /// ([`Fit::default_for`]).
    pub fn fit(&self, c: u32) -> Option<Fit> {
        let in_cover = |cover: &[Cover]| cover.iter().any(|&(a, b)| (a..=b).contains(&c));
        let by_cover = self
            .fits
            .iter()
            .find(|f| f.cover.as_deref().is_some_and(in_cover));
        by_cover
            .or_else(|| self.fits.iter().find(|f| f.cover.is_none()))
            .map(|f| f.fit)
    }
}

/// `antialias=` (ADR-1301 D4): `auto` follows the grid, `no` draws every
/// outline glyph crisp at any size.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Antialias {
    #[default]
    Auto,
    No,
}

/// `hinting=` (ADR-1200 D1): how the helper fits the frit's outline
/// glyphs to the pixel grid. Bitmap sources take no hinting.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum Hinting {
    /// Up and down only, as FreeType's light load
    /// (`FT_LOAD_TARGET_LIGHT`) hints: stems and the tops and bottoms of letters land on whole rows, and
    /// each glyph keeps the width and side bearings its outline gives it.
    #[default]
    Light,
    /// Both ways: the font's own instructions, or the automatic hinter for
    /// a font that has none, may also move a glyph's stems sideways onto
    /// whole columns.
    Full,
}

/// The ranges `glyph-source-<range>=` names, by the configuration's
/// keys' names.
pub const GLYPH_SOURCE_RANGES: [(&str, Range); 6] = [
    ("box", Range::BoxDrawing),
    ("block", Range::BlockElements),
    ("braille", Range::Braille),
    ("sextants", Range::Sextants),
    ("octants", Range::Octants),
    ("legacy", Range::LegacyComputing),
];

/// A parsed frit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frit {
    pub name: String,
    pub sources: Vec<Source>,
    /// `size=`: an outline frit's starting size in points, from which a
    /// zoom steps; a bitmap frit's is left aside (ADR-1200 D1, D17).
    pub size: Option<u32>,
    /// `cell=`: the cell the console draws in and the helper generates
    /// for (#145), which the console reads before its first frame.
    pub cell: Option<Cell>,
    pub antialias: Antialias,
    pub hinting: Hinting,
    /// `line-spacing=`: the height added to each row, a whole percentage
    /// of the text's height from 0 to 100 (ADR-1302 D13); `None` when the
    /// frit states none, which adds nothing.
    pub line_spacing: Option<u32>,
    /// `glyph-source=`, for every range (`None`), and each
    /// `glyph-source-<range>=`, in the frit's order: the #85 sprite policy,
    /// the configuration's keys over it.
    pub glyph_source: Vec<(Option<Range>, vtium_font::Source)>,
    pub license: Option<String>,
    /// The frit's bytes, verbatim.
    pub text: String,
    /// The file the frit was read from, for messages.
    pub file: PathBuf,
    /// What the frit carries that does nothing, each naming its line:
    /// `compress=`.
    pub warnings: Vec<FritError>,
}

/// The style `style=`'s value names (ADR-1200 D1): `regular`, `bold`,
/// `italic` or `bold-italic`, spelled exactly so.
pub fn style(value: &str) -> Option<Style> {
    let (bold, italic) = match value {
        "regular" => (false, false),
        "bold" => (true, false),
        "italic" => (false, true),
        "bold-italic" => (true, true),
        _ => return None,
    };
    Some(Style { bold, italic })
}

/// Whether `name` is a set name: 1 to 64 characters of `A-Z a-z 0-9 . _
/// + -`, not starting with `.` (ADR-1200 D1).
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

impl Frit {
    /// Reads and parses the frit at `path`.
    pub fn load(path: &Path) -> Result<Frit, FritError> {
        let err = |why: String| FritError {
            file: path.to_owned(),
            line: 0,
            why,
        };
        let bytes = crate::sources::read_regular(path, FRIT_BYTES_MAX as u64).map_err(err)?;
        let stem = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.strip_suffix(".vtfrit").unwrap_or(n))
            .unwrap_or_default();
        Frit::parse(&bytes, path, stem)
    }

    /// Parses a frit's bytes; `file` names it in errors and `stem` is the
    /// set's name when the frit gives none.
    pub fn parse(bytes: &[u8], file: &Path, stem: &str) -> Result<Frit, FritError> {
        let err = |line: usize, why: &str| FritError {
            file: file.to_owned(),
            line,
            why: why.to_owned(),
        };
        if bytes.len() > FRIT_BYTES_MAX {
            return Err(err(
                0,
                &format!("the file is too large ({FRIT_BYTES_MAX} bytes at most)"),
            ));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| err(0, "not UTF-8"))?;
        let mut name = None;
        let mut sources: Vec<Source> = Vec::new();
        let mut compress = false;
        let mut size = None;
        let mut cell = None;
        let mut antialias = None;
        let mut hinting = None;
        let mut line_spacing = None;
        // the set a `fit=` line is for: the last `source=` or `cover=`
        // line, its line and its ranges (`None` for a whole source)
        let mut set: Option<(usize, Option<Vec<Cover>>)> = None;
        let mut glyph_source = Vec::new();
        let mut license = None;
        let mut warnings = Vec::new();
        // the sources a `style=` line has named, by their place
        let mut styled: Vec<usize> = Vec::new();
        for (line, n) in text.split('\n').zip(1usize..) {
            if line.len() > FRIT_LINE_MAX {
                return Err(err(n, "the line is too long"));
            }
            if line.contains('\0') {
                return Err(err(n, "the line contains a NUL byte"));
            }
            let body = line.split('#').next().unwrap_or_default().trim();
            if body.is_empty() {
                continue;
            }
            let Some((key, value)) = body.split_once('=') else {
                return Err(err(n, "the line is not key=value"));
            };
            let (key, value) = (key.trim(), value.trim());
            if value.is_empty() {
                return Err(err(n, &format!("{key}= has no value")));
            }
            if value.chars().any(char::is_control) {
                return Err(err(n, &format!("{key}= contains a control character")));
            }
            let once = |seen: bool| {
                if seen {
                    Err(err(n, &format!("{key}= appears twice")))
                } else {
                    Ok(())
                }
            };
            let current = |sources: &mut Vec<Source>| -> Result<usize, FritError> {
                sources
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| err(n, &format!("{key}= comes before any source= line")))
            };
            match key {
                "name" => {
                    once(name.is_some())?;
                    name = Some(value.to_owned());
                }
                "source" => {
                    if sources.len() >= FRIT_SOURCES_MAX {
                        return Err(err(
                            n,
                            &format!("too many sources (at most {FRIT_SOURCES_MAX})"),
                        ));
                    }
                    let origin = origin(value).map_err(|why| err(n, &why))?;
                    sources.push(Source {
                        spelling: value.to_owned(),
                        origin,
                        unimap: None,
                        cover: Vec::new(),
                        cp_size: None,
                        style: Style::default(),
                        fits: Vec::new(),
                        line: n,
                    });
                    set = Some((n, None));
                }
                "unimap" => {
                    let i = current(&mut sources)?;
                    let source = sources
                        .get_mut(i)
                        .ok_or_else(|| err(n, "there is no source line"))?;
                    once(source.unimap.is_some())?;
                    if matches!(
                        source.origin,
                        Origin::Builtin(_)
                            | Origin::Console(_)
                            | Origin::Outline(_)
                            | Origin::Bdf(_)
                    ) {
                        return Err(err(n, "unimap= is for a source without its own map"));
                    }
                    if value.contains('/') && !value.starts_with('/') {
                        return Err(err(
                            n,
                            "unimap= takes a kbd unimap name or an absolute path",
                        ));
                    }
                    source.unimap = Some(value.to_owned());
                }
                "cover" => {
                    let i = current(&mut sources)?;
                    let ranges = cover(value).map_err(|why| err(n, &why))?;
                    set = Some((n, Some(ranges.clone())));
                    if let Some(source) = sources.get_mut(i) {
                        source.cover = merged([std::mem::take(&mut source.cover), ranges].concat());
                    }
                }
                "cp-size" => {
                    let i = current(&mut sources)?;
                    let source = sources
                        .get_mut(i)
                        .ok_or_else(|| err(n, "there is no source line"))?;
                    once(source.cp_size.is_some())?;
                    if !matches!(
                        source.origin,
                        Origin::Kbd(_) | Origin::Font(_) | Origin::File(_) | Origin::Vconsole
                    ) {
                        return Err(err(
                            n,
                            "cp-size= is for a CPI font: a kbd:, font:, file: or vconsole source",
                        ));
                    }
                    source.cp_size = Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|s| CP_SIZES.contains(s) && !value.starts_with(['0', '+']))
                            .ok_or_else(|| err(n, "cp-size= is 8, 14 or 16"))?,
                    );
                }
                "style" => {
                    let i = current(&mut sources)?;
                    let source = sources
                        .get_mut(i)
                        .ok_or_else(|| err(n, "there is no source line"))?;
                    if styled.contains(&i) {
                        return Err(err(n, "style= appears twice for one source"));
                    }
                    styled.push(i);
                    source.style = style(value).ok_or_else(|| {
                        err(n, "style= takes regular, bold, italic or bold-italic")
                    })?;
                }
                "compress" => {
                    // ADR-1200 D1: the key changes nothing that is drawn,
                    // and refusing it would leave the console on the
                    // built-in font, so it is accepted with a warning
                    once(compress)?;
                    compress = true;
                    warnings.push(err(n, "compress= has no effect here, and is ignored"));
                }
                "license" => {
                    once(license.is_some())?;
                    license = Some(value.to_owned());
                }
                "size" => {
                    once(size.is_some())?;
                    size = Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|s| {
                                (FRIT_SIZE_MIN..=FRIT_SIZE_MAX).contains(s)
                                    && !value.starts_with(['0', '+'])
                            })
                            .ok_or_else(|| {
                                err(
                                    n,
                                    &format!(
                                        "size= takes a size in points from {FRIT_SIZE_MIN} to {FRIT_SIZE_MAX}"
                                    ),
                                )
                            })?,
                    );
                }
                "cell" => {
                    once(cell.is_some())?;
                    cell = Some(parse_cell(value).ok_or_else(|| {
                        err(
                            n,
                            &format!(
                                "cell= takes WIDTHxHEIGHT in pixels, each from {FRIT_CELL_MIN} to {FRIT_CELL_MAX}"
                            ),
                        )
                    })?);
                }
                "antialias" => {
                    once(antialias.is_some())?;
                    antialias = Some(match value {
                        "auto" => Antialias::Auto,
                        "no" => Antialias::No,
                        _ => return Err(err(n, "antialias= takes auto or no")),
                    });
                }
                "line-spacing" => {
                    once(line_spacing.is_some())?;
                    line_spacing = Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|&v| {
                                v <= LINE_SPACING_MAX
                                    && !value.starts_with('+')
                                    && (value == "0" || !value.starts_with('0'))
                            })
                            .ok_or_else(|| {
                                err(
                                    n,
                                    &format!(
                                        "line-spacing= takes a whole percentage from 0 to {LINE_SPACING_MAX}"
                                    ),
                                )
                            })?,
                    );
                }
                "fit" => {
                    let i = current(&mut sources)?;
                    let fit = Fit::parse(value)
                        .ok_or_else(|| err(n, "fit= takes fill, natural or room"))?;
                    let (set_line, cover) = set.clone().unwrap_or_default();
                    let source = sources
                        .get_mut(i)
                        .ok_or_else(|| err(n, "there is no source line"))?;
                    if source.fits.iter().any(|f| f.set_line == set_line) {
                        return Err(err(n, "fit= appears twice for one set"));
                    }
                    source.fits.push(SetFit {
                        cover,
                        fit,
                        set_line,
                    });
                }
                "hinting" => {
                    once(hinting.is_some())?;
                    hinting = Some(match value {
                        "light" => Hinting::Light,
                        "full" => Hinting::Full,
                        _ => return Err(err(n, "hinting= takes light or full")),
                    });
                }
                k if k == "glyph-source" || k.starts_with("glyph-source-") => {
                    let range = match k.strip_prefix("glyph-source-") {
                        None => None,
                        Some(r) => Some(
                            GLYPH_SOURCE_RANGES
                                .iter()
                                .find(|&&(name, _)| name == r)
                                .map(|&(_, range)| range)
                                .ok_or_else(|| {
                                    err(
                                        n,
                                        &format!(
                                            "{k}= names no range: use box, block, braille, sextants, octants or legacy"
                                        ),
                                    )
                                })?,
                        ),
                    };
                    if glyph_source.iter().any(|&(r, _)| r == range) {
                        return Err(err(n, &format!("{k}= appears twice")));
                    }
                    let source = match value {
                        "font" => vtium_font::Source::Font,
                        "sprites" => vtium_font::Source::Sprites,
                        "auto" => vtium_font::Source::Auto,
                        _ => return Err(err(n, &format!("{k}= takes font, sprites or auto"))),
                    };
                    glyph_source.push((range, source));
                }
                _ => return Err(err(n, &format!("unknown key {key}"))),
            }
        }
        let name = name.unwrap_or_else(|| stem.to_owned());
        if !valid_name(&name) {
            return Err(err(
                0,
                &format!(
                    "{name:?} cannot be a font set's name: use 1 to 64 of A-Z a-z 0-9 . _ + -, not starting with ."
                ),
            ));
        }
        if RESERVED.contains(&name.as_str()) {
            return Err(err(
                0,
                &format!("{name} is the name of a built-in font set"),
            ));
        }
        if sources.is_empty() {
            return Err(err(0, "there is no source= line"));
        }
        Ok(Frit {
            name,
            sources,
            size,
            cell,
            antialias: antialias.unwrap_or_default(),
            hinting: hinting.unwrap_or_default(),
            line_spacing,
            glyph_source,
            license,
            text: text.to_owned(),
            file: file.to_owned(),
            warnings,
        })
    }
}

/// A `cell=` value, read by the key table's [`vtium_settings::keys::parse_cell`],
/// which a look's `cell` is read by too.
pub fn parse_cell(value: &str) -> Option<Cell> {
    vtium_settings::keys::parse_cell(value).map(|(width, height)| Cell { width, height })
}

/// A `source=` value's origin.
fn origin(value: &str) -> Result<Origin, String> {
    if value == "vconsole" {
        return Ok(Origin::Vconsole);
    }
    let Some((kind, rest)) = value.split_once(':') else {
        return Err(format!(
            "source={value} is not one of font:, kbd:, file:, bdf:, outline:, vconsole, console: or builtin:"
        ));
    };
    match kind {
        "font" => {
            if rest.trim().is_empty() || rest.contains('/') {
                return Err("font: takes a family name, with no /".to_owned());
            }
            Ok(Origin::Font(rest.to_owned()))
        }
        "kbd" if !rest.is_empty() && !rest.contains('\0') => {
            if rest.split('/').any(|c| c == "..") {
                return Err("a kbd: name cannot contain ..".to_owned());
            }
            Ok(Origin::Kbd(rest.to_owned()))
        }
        "file" | "bdf" | "outline" => {
            let path = PathBuf::from(rest);
            if !path.is_absolute() {
                return Err(format!("{kind}: takes an absolute path"));
            }
            Ok(match kind {
                "file" => Origin::File(path),
                "bdf" => Origin::Bdf(path),
                _ => Origin::Outline(path),
            })
        }
        "console" => rest
            .parse::<u16>()
            .ok()
            .filter(|&vt| (1..=63).contains(&vt) && !rest.starts_with('0'))
            .map(Origin::Console)
            .ok_or_else(|| "console: takes a VT number from 1 to 63".to_owned()),
        "builtin" if RESERVED.contains(&rest) => Ok(Origin::Builtin(rest.to_owned())),
        "builtin" => Err("builtin: takes unifont or unscii-8".to_owned()),
        _ => Err(format!(
            "source={value} is not one of font:, kbd:, file:, bdf:, outline:, vconsole, console: or builtin:"
        )),
    }
}

/// The ASCII graphic characters, `cover=ascii`.
const ASCII: Cover = (0x20, 0x7e);
/// Latin-1's beyond ASCII, which `cover=latin-1` adds.
const LATIN_1: Cover = (0xa0, 0xff);

/// A `cover=` value's ranges: `ascii`, `latin-1`, a named set of
/// [`crate::cover`], or `U+XXXX` or `U+XXXX-U+YYYY`.
fn cover(value: &str) -> Result<Vec<Cover>, String> {
    match value {
        "ascii" => return Ok(vec![ASCII]),
        "latin-1" => return Ok(vec![ASCII, LATIN_1]),
        _ => {}
    }
    if let Some((_, set)) = crate::cover::SETS.iter().find(|(name, _)| *name == value) {
        return Ok(set.to_vec());
    }
    let point = |s: &str| {
        let hex = s.strip_prefix("U+")?;
        if !(4..=6).contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16)
            .ok()
            .filter(|&c| c <= 0x10_ffff)
    };
    let (a, b) = match value.split_once('-') {
        Some((a, b)) => (point(a), point(b)),
        None => (point(value), point(value)),
    };
    match (a, b) {
        (Some(a), Some(b)) if a <= b => Ok(vec![(a, b)]),
        _ => Err(format!(
            "cover={value} is not one of ascii, latin-1, gb2312, gbk, big5, jis-x-0208, ks-x-1001, U+XXXX or U+XXXX-U+YYYY"
        )),
    }
}

/// `ranges` sorted, with those that overlap or touch merged.
fn merged(mut ranges: Vec<Cover>) -> Vec<Cover> {
    ranges.sort_unstable();
    let mut out: Vec<Cover> = Vec::with_capacity(ranges.len());
    for (a, b) in ranges {
        match out.last_mut() {
            Some((_, end)) if a <= end.saturating_add(1) => *end = (*end).max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Frit, FritError> {
        Frit::parse(text.as_bytes(), Path::new("/r/t.vtfrit"), "t")
    }

    fn refusal(text: &str) -> (usize, String) {
        let e = parse(text).expect_err("refused");
        (e.line, e.why)
    }

    // a bound's error gives the number, never the name of the constant
    // that holds it
    #[test]
    fn a_bounds_error_names_the_number_not_the_constant() {
        let many: String = (0..=FRIT_SOURCES_MAX).map(|_| "source=kbd:a\n").collect();
        let (_, why) = refusal(&many);
        assert_eq!(
            why,
            format!("too many sources (at most {FRIT_SOURCES_MAX})")
        );
        let big = "#".repeat(FRIT_BYTES_MAX.saturating_add(1));
        let (_, why) = refusal(&big);
        assert_eq!(
            why,
            format!("the file is too large ({FRIT_BYTES_MAX} bytes at most)")
        );
        assert!(!why.contains("_MAX"), "{why}");
    }

    // a source takes the style its `style=` line names, regular unless one
    // does (ADR-1200 D1)
    #[test]
    fn style_names_the_style_of_the_source_above_it() {
        let r = parse(
            "source=kbd:a\n\
             source=kbd:b\n\
             style=bold\n\
             source=kbd:c\n\
             style=italic\n\
             source=kbd:d\n\
             cover=ascii\n\
             style=bold-italic\n\
             source=kbd:e\n\
             style=regular\n",
        )
        .expect("parses");
        let styles: Vec<(bool, bool)> = r
            .sources
            .iter()
            .map(|s| (s.style.bold, s.style.italic))
            .collect();
        assert_eq!(
            styles,
            [
                (false, false),
                (true, false),
                (false, true),
                (true, true),
                (false, false)
            ]
        );
    }

    #[test]
    fn every_key_is_read() {
        let r = parse(
            "# a comment\n\
             name = term-16 # trailing comment\n\
             source=kbd:ter-v16n\n\
             unimap=lat1u\n\
             cover=ascii\n\
             cover=U+2500-U+257F\n\
             fit=natural\n\
             style=regular\n\
             source=file:/usr/share/fonts/x.cp.gz\n\
             cp-size=14\n\
             source=vconsole\n\
             source=console:3\n\
             source=builtin:unifont\n\
             source=font:Terminus 16\n\
             style=regular\n\
             source=bdf:/usr/share/fonts/f.bdf.gz\n\
             source=outline:/usr/share/fonts/noto/NotoColorEmoji.ttf\n\
             cover=U+1F600-U+1F64F\n\
             license=OFL-1.1\n\
             size=12\n\
             cell=10x20\n\
             antialias=no\n\
             hinting=full\n\
             line-spacing=25\n\
             glyph-source=font\n\
             glyph-source-braille = sprites\n\
             glyph-source-legacy=auto\n",
        )
        .expect("parses");
        assert_eq!(r.name, "term-16");
        assert_eq!(r.license.as_deref(), Some("OFL-1.1"));
        let origins: Vec<&Origin> = r.sources.iter().map(|s| &s.origin).collect();
        assert_eq!(
            origins,
            [
                &Origin::Kbd("ter-v16n".into()),
                &Origin::File("/usr/share/fonts/x.cp.gz".into()),
                &Origin::Vconsole,
                &Origin::Console(3),
                &Origin::Builtin("unifont".into()),
                &Origin::Font("Terminus 16".into()),
                &Origin::Bdf("/usr/share/fonts/f.bdf.gz".into()),
                &Origin::Outline("/usr/share/fonts/noto/NotoColorEmoji.ttf".into()),
            ]
        );
        let cp: Vec<Option<u32>> = r.sources.iter().map(|s| s.cp_size).collect();
        assert_eq!(cp, [None, Some(14), None, None, None, None, None, None]);
        assert_eq!(r.sources[5].line, 14);
        assert_eq!(r.line_spacing, Some(25));
        assert_eq!(r.sources[0].fit(0x2500), Some(Fit::Natural));
        assert_eq!(r.sources[0].fit(0x41), None);
        assert_eq!(r.sources[7].cover, [(0x1f600, 0x1f64f)]);
        assert_eq!(r.size, Some(12));
        assert_eq!(
            r.cell,
            Some(Cell {
                width: 10,
                height: 20
            })
        );
        assert_eq!(r.antialias, Antialias::No);
        assert_eq!(r.hinting, Hinting::Full);
        assert_eq!(
            r.glyph_source,
            [
                (None, vtium_font::Source::Font),
                (Some(Range::Braille), vtium_font::Source::Sprites),
                (Some(Range::LegacyComputing), vtium_font::Source::Auto),
            ]
        );
        assert_eq!(r.warnings, [], "nothing here does nothing");
        assert_eq!(r.sources[0].unimap.as_deref(), Some("lat1u"));
        assert_eq!(r.sources[0].cover, [(0x20, 0x7e), (0x2500, 0x257f)]);
        assert!(r.sources[0].covers(0x41) && !r.sources[0].covers(0xe9));
        assert!(r.sources[1].covers(0xe9));
    }

    #[test]
    fn the_name_defaults_to_the_stem_and_the_options_to_auto() {
        let r = parse("source=kbd:x\n").expect("parses");
        assert_eq!(r.name, "t");
        assert_eq!((r.size, r.cell, r.antialias), (None, None, Antialias::Auto));
        // ADR-1200 D1: light hinting where the frit states none
        assert_eq!(r.hinting, Hinting::Light);
        assert_eq!(r.glyph_source, []);
        assert_eq!(r.text, "source=kbd:x\n");
        // ADR-1302 D13: no spacing, and every set takes its default fit
        assert_eq!(r.line_spacing, None);
        assert_eq!(r.sources[0].fits, []);
    }

    // ADR-1302 D13, ADR-1200 D1: `line-spacing=` is a whole percentage
    // from 0 to 100
    #[test]
    fn line_spacing_is_a_whole_percentage_from_0_to_100() {
        for n in [0u32, 1, 10, 99, 100] {
            let r = parse(&format!("source=kbd:x\nline-spacing={n}\n")).expect("parses");
            assert_eq!(r.line_spacing, Some(n), "{n}");
        }
    }

    // ADR-1302 D13, ADR-1200 D1: `fit=` sets how the set on the `source=`
    // or `cover=` line above it uses the taller row; a `cover=` line's set
    // wins over its source's, and a codepoint in no set with a fit takes
    // the default
    #[test]
    fn fit_names_the_fit_of_the_set_above_it() {
        let r = parse(
            "source=kbd:a\n\
             fit=natural\n\
             cover=U+2500-U+257F\n\
             fit=fill\n\
             cover=ascii\n\
             source=kbd:b\n\
             cover=U+0600-U+06FF\n\
             fit=room\n\
             source=kbd:c\n",
        )
        .expect("parses");
        let a = &r.sources[0];
        assert_eq!(a.fit(0x2502), Some(Fit::Fill));
        // ascii's cover line has no fit of its own: the source's
        assert_eq!(a.fit(0x41), Some(Fit::Natural));
        assert_eq!(a.fit(0x2588), Some(Fit::Natural));
        let b = &r.sources[1];
        assert_eq!(b.fit(0x0628), Some(Fit::Room));
        assert_eq!(b.fit(0x41), None);
        assert_eq!(r.sources[2].fits, []);
    }

    #[test]
    fn each_refusal_names_its_line() {
        for (text, line, why) in [
            ("source=kbd:x\nbogus=1\n", 2, "unknown key bogus"),
            ("source=kbd:x\njust words\n", 2, "the line is not key=value"),
            ("unimap=x\n", 1, "before any source="),
            ("cover=ascii\n", 1, "before any source="),
            (
                "source=kbd:x\ncompress=yes\ncompress=no\n",
                3,
                "appears twice",
            ),
            ("source=kbd:x\nname=a\nname=b\n", 3, "appears twice"),
            ("source=kbd:x\ncell=8x16\ncell=8x16\n", 3, "appears twice"),
            ("source=kbd:x\ncell=16\n", 2, "WIDTHxHEIGHT"),
            ("source=kbd:x\ncell=5x16\n", 2, "WIDTHxHEIGHT"),
            ("source=kbd:x\ncell=8x97\n", 2, "WIDTHxHEIGHT"),
            ("source=kbd:x\ncell=08x16\n", 2, "WIDTHxHEIGHT"),
            ("source=file:relative.psf\n", 1, "absolute path"),
            ("source=console:0\n", 1, "1 to 63"),
            ("source=console:64\n", 1, "1 to 63"),
            ("source=builtin:terminus\n", 1, "unifont or unscii-8"),
            ("source=bdf:x.bdf\n", 1, "absolute path"),
            ("source=bdf:/x.bdf\nunimap=x\n", 2, "its own map"),
            ("source=outline:x.ttf\n", 1, "absolute path"),
            ("source=outline:/x.ttf\nunimap=x\n", 2, "its own map"),
            ("source=x\n", 1, "not one of"),
            ("source=kbd:../x\n", 1, ".."),
            ("source=kbd:x\nsize=5\n", 2, "in points from 6 to 96"),
            ("source=kbd:x\nsize=97\n", 2, "in points from 6 to 96"),
            ("source=kbd:x\nsize=016\n", 2, "in points"),
            ("source=kbd:x\nsize=12.5\n", 2, "in points"),
            ("source=kbd:x\nsize=16\nsize=24\n", 3, "appears twice"),
            ("source=kbd:x\nlicense=a\nlicense=b\n", 3, "appears twice"),
            ("cp-size=8\n", 1, "before any source="),
            ("source=kbd:x\ncp-size=12\n", 2, "8, 14 or 16"),
            ("source=kbd:x\ncp-size=016\n", 2, "8, 14 or 16"),
            ("source=kbd:x\ncp-size=8\ncp-size=8\n", 3, "appears twice"),
            ("source=bdf:/x.bdf\ncp-size=8\n", 2, "for a CPI font"),
            ("source=builtin:unifont\ncp-size=8\n", 2, "for a CPI font"),
            ("source=console:1\ncp-size=8\n", 2, "for a CPI font"),
            ("source=outline:/x.ttf\ncp-size=8\n", 2, "for a CPI font"),
            ("source=kbd:x\nantialias=yes\n", 2, "auto or no"),
            ("source=kbd:x\nhinting=none\n", 2, "light or full"),
            ("source=kbd:x\nline-spacing=101\n", 2, "from 0 to 100"),
            ("source=kbd:x\nline-spacing=-1\n", 2, "from 0 to 100"),
            ("source=kbd:x\nline-spacing=010\n", 2, "from 0 to 100"),
            ("source=kbd:x\nline-spacing=+10\n", 2, "from 0 to 100"),
            ("source=kbd:x\nline-spacing=12.5\n", 2, "from 0 to 100"),
            ("source=kbd:x\nline-spacing=10%\n", 2, "from 0 to 100"),
            (
                "source=kbd:x\nline-spacing=10\nline-spacing=10\n",
                3,
                "appears twice",
            ),
            ("fit=fill\n", 1, "before any source="),
            ("source=kbd:x\nfit=stretch\n", 2, "fill, natural or room"),
            ("source=kbd:x\nfit=Fill\n", 2, "fill, natural or room"),
            (
                "source=kbd:x\nfit=fill\nfit=natural\n",
                3,
                "appears twice for one set",
            ),
            (
                "source=kbd:x\ncover=ascii\nfit=fill\nstyle=bold\nfit=room\n",
                5,
                "appears twice for one set",
            ),
            ("source=kbd:x\nhinting=Light\n", 2, "light or full"),
            (
                "source=kbd:x\nhinting=light\nhinting=full\n",
                3,
                "appears twice",
            ),
            (
                "source=kbd:x\nantialias=no\nantialias=no\n",
                3,
                "appears twice",
            ),
            (
                "source=kbd:x\nstyle=oblique\n",
                2,
                "regular, bold, italic or bold-italic",
            ),
            ("source=kbd:x\nstyle=Bold\n", 2, "regular, bold, italic"),
            (
                "source=kbd:x\nstyle=bold\nstyle=italic\n",
                3,
                "appears twice",
            ),
            ("style=bold\n", 1, "before any source="),
            (
                "source=kbd:x\nglyph-source=fonts\n",
                2,
                "font, sprites or auto",
            ),
            (
                "source=kbd:x\nglyph-source-box=x\n",
                2,
                "font, sprites or auto",
            ),
            (
                "source=kbd:x\nglyph-source-boxes=font\n",
                2,
                "names no range",
            ),
            ("source=kbd:x\nglyph-source-=font\n", 2, "names no range"),
            (
                "source=kbd:x\nglyph-source=font\nglyph-source=auto\n",
                3,
                "appears twice",
            ),
            (
                "source=kbd:x\nglyph-source-box=font\nglyph-source-box=auto\n",
                3,
                "appears twice",
            ),
            ("source=kbd:x\ncover=GBK\n", 2, "not one of"),
            ("source=kbd:x\ncover=U+9FFF-U+4E00\n", 2, "not one of"),
            ("source=kbd:x\ncover=U+41\n", 2, "not one of"),
            ("source=font: \n", 1, "family name"),
            ("source=font:a/b\n", 1, "family name"),
            // the store's keys are not a frit's (ADR-1200 D1's key table)
            (
                "source=font:x\npackage=terminus-font\n",
                2,
                "unknown key package",
            ),
            (
                "source=font:x\nsha256=00000000000000000000000000000000000000000000000000000000000000ff\n",
                2,
                "unknown key sha256",
            ),
            ("source=kbd:x\nunimap=a/b\n", 2, "absolute path"),
            ("source=builtin:unifont\nunimap=x\n", 2, "its own map"),
            ("source=kbd:x\nlicense=\n", 2, "has no value"),
            ("source=file:/a\tb.psf\n", 1, "control character"),
            ("name=unifont\nsource=kbd:x\n", 0, "built-in"),
            (
                "name=.hidden\nsource=kbd:x\n",
                0,
                "cannot be a font set's name",
            ),
            ("name=a b\nsource=kbd:x\n", 0, "cannot be a font set's name"),
            ("# nothing\n", 0, "there is no source= line"),
        ] {
            let (l, w) = refusal(text);
            assert_eq!(l, line, "{text:?}: {w}");
            assert!(w.contains(why), "{text:?}: {w}");
        }
    }

    // ADR-1200 D1: compress= has nothing to act on, so a frit carrying it
    // is accepted, with one warning naming the file and the line.
    #[test]
    fn compress_is_accepted_with_one_warning_naming_its_line() {
        for value in ["yes", "no", "zstd"] {
            let r = parse(&format!("source=kbd:x\n\ncompress={value}\n")).expect("accepted");
            let lines: Vec<String> = r.warnings.iter().map(ToString::to_string).collect();
            assert_eq!(
                lines,
                ["/r/t.vtfrit:3: compress= has no effect here, and is ignored"],
                "compress={value}"
            );
        }
        assert_eq!(parse("source=kbd:x\n").expect("parses").warnings, []);
    }

    /// The codepoints `cover=VALUE` lets a source give.
    fn count(value: &str) -> u32 {
        let r = parse(&format!("source=kbd:x\ncover={value}\n")).expect("parses");
        r.sources[0].cover.iter().map(|&(a, b)| b - a + 1).sum()
    }

    // ADR-1200 D1: each named set is its standard's repertoire as its
    // Unicode mapping file gives it (tools/cover-tables/provenance.tsv),
    // so a regenerated table that moves fails here
    #[test]
    fn the_named_sets_hold_their_files_codepoints() {
        for (name, codepoints) in [
            ("ascii", 95),
            ("latin-1", 95 + 96),
            ("gb2312", 7_445),
            ("gbk", 21_920),
            ("big5", 13_704),
            ("jis-x-0208", 6_879),
            ("ks-x-1001", 8_224),
        ] {
            assert_eq!(count(name), codepoints, "{name}");
        }
        let r = parse(
            "source=kbd:x\ncover=jis-x-0208\nsource=kbd:y\ncover=big5\ncover=ascii\n\
             source=kbd:z\ncover=gbk\n",
        )
        .expect("parses");
        let [jis, big5, gbk] = [0, 1, 2].map(|i| &r.sources[i]);
        assert!(jis.covers(0x3042) && jis.covers(0x4e9c) && !jis.covers(0x41));
        assert!(!jis.covers(0xac00), "no Hangul in JIS X 0208");
        assert!(big5.covers(0x41) && big5.covers(0x7e) && !big5.covers(0x7f));
        assert!(big5.covers(0x4e00) && !big5.covers(0xac00));
        assert!(gbk.covers(0x9fa5) && gbk.covers(0x20ac) && !gbk.covers(0xac00));
        let r =
            parse("source=kbd:x\ncover=gb2312\nsource=kbd:y\ncover=ks-x-1001\n").expect("parses");
        let [gb2312, ks] = [0, 1].map(|i| &r.sources[i]);
        // 啊 opens GB 2312's hanzi; 丂 is GBK's alone
        assert!(gb2312.covers(0x554a) && !gb2312.covers(0x4e02) && !gb2312.covers(0x41));
        // 가 is KS X 1001's; 갂 is UHC's alone
        assert!(ks.covers(0xac00) && !ks.covers(0xac02) && ks.covers(0x4f3d));
        // the ranges are merged and sorted
        assert!(
            big5.cover.windows(2).all(|w| w[0].1 + 1 < w[1].0),
            "{:?}",
            &big5.cover[..4]
        );
    }

    // src/cover.rs is what tools/cover-tables wrote: an edit by hand that
    // keeps each set's count still moves its SHA-256, which the generator
    // writes beside it and its check compares
    #[test]
    fn the_named_sets_are_the_tables_the_generator_wrote() {
        let digest =
            vtium_font::sha256_hex(&vtium_font::Sha256::digest(include_bytes!("cover.rs")));
        assert_eq!(digest, include_str!("cover.rs.sha256").trim());
    }

    #[test]
    fn a_sources_cover_lines_are_one_sorted_union() {
        let r = parse(
            "source=kbd:x\ncover=U+2500-U+257F\ncover=U+0045-U+0090\ncover=ascii\ncover=U+257F\n\
             cover=U+2580-U+259F\ncover=U+00A0\n",
        )
        .expect("parses");
        assert_eq!(
            r.sources[0].cover,
            [(0x20, 0x90), (0xa0, 0xa0), (0x2500, 0x259f)]
        );
        let s = &r.sources[0];
        assert!(s.covers(0x20) && s.covers(0x90) && !s.covers(0x91) && !s.covers(0x1f));
        assert!(s.covers(0xa0) && !s.covers(0xa1) && s.covers(0x259f) && !s.covers(0x25a0));
    }

    #[test]
    fn a_line_past_its_bound_and_a_nul_are_refused() {
        let long = format!("source=kbd:{}\n", "x".repeat(FRIT_LINE_MAX));
        assert_eq!(refusal(&long).0, 1);
        assert_eq!(refusal("source=kbd:x\0\n").0, 1);
        let many = "source=kbd:x\n".repeat(FRIT_SOURCES_MAX + 1);
        assert_eq!(refusal(&many).0, FRIT_SOURCES_MAX + 1);
        let e = Frit::parse(&[0xff], Path::new("/r/t.vtfrit"), "t").expect_err("refused");
        assert_eq!(e.why, "not UTF-8");
    }

    #[test]
    fn the_error_names_the_file_and_line() {
        let e = parse("source=kbd:x\nbogus=1\n").expect_err("refused");
        assert_eq!(e.to_string(), "/r/t.vtfrit:2: unknown key bogus");
    }
}
