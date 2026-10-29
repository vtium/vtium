//! The font: which frit or built-in font vtium draws, its faces at each
//! height, its zoom steps and the automatic size, with the lookup's
//! fallback.
//!
//! The font is `font-name`, else vconsole's (ADR-1200 D9). A name is a
//! frit's (`NAME.vtfrit` in the frits' directory), else a kbd name, a path
//! or vconsole's font becomes a one-source frit made on the fly
//! ([`frit_named`]). A frit is generated at run time (ADR-1301 D2), in
//! the cell it states ([`frit_cell`], #145), else for a frit of bitmap
//! sources its fonts' own cell, which the helper tells before the first
//! frame (the operator, 2026-10-28), else the display's: the
//! built-in font draws in that cell at once, and the helper's glyphs
//! replace it, character by character, as they arrive ([`Chosen::Frit`],
//! [`crate::deferred`], [`crate::helper`]). The built-ins are Unscii-8
//! for cells under 16 pixels and unifont from 16, but in a zh, ja or ko
//! locale, which Unscii-8 does not cover (D7). OSC 50 switches among the
//! frits and the built-ins while vtium runs (D5).
//!
//! A font is looked up at the size asked for. When none is in use yet, a
//! failure tries [`FALLBACK_SIZE`] once, and a second failure is the
//! error; with a font in use, as at a zoom, a failure is the error and
//! that font stays. The C reference retried at 16 for as long as the
//! lookup failed (`src/terminal.c:921-939` at kmscon b1f7a7c5,
//! aaronsb/kmscon#63).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use vtium_font::set::{SetFace, builtin, builtin_name};
use vtium_font::sprite::Range;
use vtium_font::{CELL_SIDE_MAX, Cell, Face, Missing, Source, SpritePolicy, Unifont};

use crate::config::keys::{
    FONT_ENGINE, FONT_NAME, GLYPH_SOURCE, GLYPH_SOURCE_BLOCK, GLYPH_SOURCE_BOX,
    GLYPH_SOURCE_BRAILLE, GLYPH_SOURCE_LEGACY, GLYPH_SOURCE_OCTANTS, GLYPH_SOURCE_SEXTANTS, Key,
    MISSING,
};
use crate::config::{Config, Origin};
use crate::deferred::{DeferredFace, Shared};
use crate::helper::{self, FritText};
use crate::layout::Zoom;

/// Each sprite range and the key that sets its source alone.
pub const RANGE_KEYS: [(Range, &Key); 6] = [
    (Range::BoxDrawing, &GLYPH_SOURCE_BOX),
    (Range::BlockElements, &GLYPH_SOURCE_BLOCK),
    (Range::Braille, &GLYPH_SOURCE_BRAILLE),
    (Range::Sextants, &GLYPH_SOURCE_SEXTANTS),
    (Range::Octants, &GLYPH_SOURCE_OCTANTS),
    (Range::LegacyComputing, &GLYPH_SOURCE_LEGACY),
];

fn source(text: &str) -> Option<Source> {
    match text {
        "font" => Some(Source::Font),
        "sprites" => Some(Source::Sprites),
        "auto" => Some(Source::Auto),
        _ => None,
    }
}

/// A font's sprite policy from the configuration: `glyph-source` for all
/// six ranges, else the face's own `default`, then each range's own key
/// over it (vtium-font's README, "#21 builds a font's policy").
pub fn policy(config: &Config, default: SpritePolicy) -> SpritePolicy {
    let base = config
        .text(&GLYPH_SOURCE)
        .and_then(source)
        .map_or(default, SpritePolicy::all);
    RANGE_KEYS.iter().fold(base, |p, &(range, key)| {
        match config.text(key).and_then(source) {
            Some(s) => p.with(range, s),
            None => p,
        }
    })
}

/// A frit's sprite policy from its `text` (ADR-1200 D1): `glyph-source=`
/// for all six ranges, else `default`, then each range's own
/// `glyph-source-<range>=` over it, read in the frit's line syntax as the
/// configuration's keys are named. A value or a range vtium does not know
/// changes nothing here; the helper refuses the frit naming its line.
pub fn frit_policy(text: &str, default: SpritePolicy) -> SpritePolicy {
    let lines = || {
        text.split('\n').filter_map(|line| {
            let (key, value) = line.split('#').next()?.split_once('=')?;
            Some((key.trim(), source(value.trim())?))
        })
    };
    let base = lines()
        .rfind(|&(key, _)| key == GLYPH_SOURCE.name)
        .map_or(default, |(_, s)| SpritePolicy::all(s));
    lines().fold(base, |p, (key, s)| {
        match RANGE_KEYS.iter().find(|(_, k)| k.name == key) {
            Some(&(range, _)) => p.with(range, s),
            None => p,
        }
    })
}

/// What a character the font lacks draws as, from `missing`.
pub fn missing(config: &Config) -> Missing {
    match config.text(&MISSING) {
        Some("hex") => Missing::Hex,
        _ => Missing::Replacement,
    }
}

/// The size tried once when the first lookup fails.
pub const FALLBACK_SIZE: u32 = 16;

/// Looks a font up at `size` with `find`, which gives the font at a size
/// or why there is none; `in_use` says whether a font is in use. Returns
/// the font and the size it was found at.
pub fn select<F, E>(
    size: u32,
    in_use: bool,
    mut find: impl FnMut(u32) -> Result<F, E>,
) -> Result<(F, u32), E> {
    match find(size) {
        Ok(font) => Ok((font, size)),
        Err(e) if in_use => Err(e),
        Err(_) => find(FALLBACK_SIZE).map(|f| (f, FALLBACK_SIZE)),
    }
}

/// The height `font-size=0` picks for a display `width` by `height`, for
/// a font whose smallest height is `smallest`: at most 120 columns,
/// taking a cell as half as wide as it is high, and at least 40 rows, and
/// never under 16 (`src/terminal.c:899-918`); but where 16-pixel cells
/// would leave fewer than 80 by 25, the font's own smallest height, so a
/// 320x240 display draws an 8x8 font as 40 by 30 cells (vtium/vtium#118).
pub fn auto_size(width: u32, height: u32, smallest: u32) -> u32 {
    if width == 0 || height == 0 {
        return FALLBACK_SIZE;
    }
    let by_width = width.saturating_mul(2) / 120;
    let size = by_width.min(height / 40).max(16);
    if width < 80 * 8 || height < 25 * 16 {
        size.min(smallest.max(1))
    } else {
        size
    }
}

/// The frits' directory, set at build time with `VTIUM_FONT_FRITS` as
/// vtium-vitrify's is.
pub fn default_frits() -> PathBuf {
    PathBuf::from(option_env!("VTIUM_FONT_FRITS").unwrap_or("/etc/vtium/fonts"))
}

/// Whether `name` is a frit's name (ADR-1200 D1): 1 to 64 characters of
/// `A-Z a-z 0-9 . _ + -`, not starting with `.`.
fn frit_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// The name a frit made on the fly goes to the helper by when the name it
/// was given is not a frit's name, as a path or a family is: the helper
/// refuses a request naming any other (ADR-1200 D1).
pub const GENERATED: &str = "generated";

/// The frit `name` names in `frits`: `NAME.vtfrit`, a regular file, for a
/// frit's name, so no name reaches outside the directory.
pub fn frit(frits: &Path, name: &str) -> Option<PathBuf> {
    let path = frits.join(format!("{name}.vtfrit"));
    (frit_name(name) && std::fs::metadata(&path).is_ok_and(|m| m.is_file())).then_some(path)
}

/// The smallest and largest side of a frit's `cell=`, vtium-vitrify's
/// `FRIT_CELL_MIN` and `FRIT_CELL_MAX`.
pub const FRIT_CELL_MIN: u32 = 6;
pub const FRIT_CELL_MAX: u32 = 96;

/// The frit at `path`'s text: a regular file of at most `FILE_MAX`
/// bytes, as vtium-vitrify's `FRIT_BYTES_MAX`, in UTF-8.
pub fn frit_text(path: &Path) -> Result<String, String> {
    let bytes = crate::config::read_file(path)
        .map_err(|e| e.to_string())?
        .ok_or("it is gone")?;
    if bytes.len() > crate::config::FILE_MAX {
        return Err(format!(
            "it is longer than {} bytes",
            crate::config::FILE_MAX
        ));
    }
    String::from_utf8(bytes).map_err(|_| "it is not UTF-8".to_owned())
}

/// The value of the one `key=` line in a frit's `text`, with its line
/// number: read in the frit's line syntax, one `key=value` a line and `#`
/// to the line's end a comment, and nothing else of it, since the console
/// parses no font. `None` when there is none; an error naming the line
/// for a second.
fn frit_line<'a>(text: &'a str, key: &str) -> Result<Option<(usize, &'a str)>, String> {
    let mut found = None;
    for (line, n) in text.split('\n').zip(1usize..) {
        let body = line.split('#').next().unwrap_or_default();
        let Some((k, value)) = body.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        if found.is_some() {
            return Err(format!("line {n}: {key}= given twice"));
        }
        found = Some((n, value.trim()));
    }
    Ok(found)
}

/// One side of a frit's cell, or its size, in pixels: digits only, no
/// leading zero, [`FRIT_CELL_MIN`] to [`FRIT_CELL_MAX`], as vtium-vitrify
/// reads it.
fn frit_side(s: &str) -> Option<u32> {
    s.parse::<u32>()
        .ok()
        .filter(|v| (FRIT_CELL_MIN..=FRIT_CELL_MAX).contains(v) && !s.starts_with(['0', '+']))
}

/// The cell a frit's `text` states, `cell=WIDTHxHEIGHT` (#145); `None`
/// when it states none; an error naming the line for a value
/// vtium-vitrify refuses, or a second `cell=`.
pub fn frit_cell(text: &str) -> Result<Option<Cell>, String> {
    let Some((n, value)) = frit_line(text, "cell")? else {
        return Ok(None);
    };
    let parsed = value.split_once('x').and_then(|(w, h)| {
        Some(Cell {
            width: frit_side(w)?,
            height: frit_side(h)?,
        })
    });
    parsed.map(Some).ok_or_else(|| {
        format!(
            "line {n}: cell={value} is not WIDTHxHEIGHT in pixels, each {FRIT_CELL_MIN} to {FRIT_CELL_MAX}"
        )
    })
}

/// The size a frit's `text` starts at, `size=` (ADR-1200 D1), in points;
/// `None` when it states none; an error naming the line for a value
/// vtium-vitrify refuses, or a second `size=`.
pub fn frit_size(text: &str) -> Result<Option<u32>, String> {
    let Some((n, value)) = frit_line(text, "size")? else {
        return Ok(None);
    };
    frit_side(value).map(Some).ok_or_else(|| {
        format!(
            "line {n}: size={value} is not a size in points, {FRIT_CELL_MIN} to {FRIT_CELL_MAX}"
        )
    })
}

/// Points in an inch: a point is 1/72 inch.
const POINTS_PER_INCH: f64 = 72.0;

/// The pixel height of `points` on a display of `dpi` dots per inch, at
/// least 1 and at most [`CELL_SIDE_MAX`] (the operator's decision on
/// vtium/vtium#145).
pub fn points_to_pixels(points: u32, dpi: f64) -> u32 {
    // points × DPI ÷ 72, to the nearest pixel, a half away from zero
    let pixels = (f64::from(points) * dpi / POINTS_PER_INCH).round();
    // `as` saturates, and takes NaN to 0
    (pixels as u32).clamp(1, CELL_SIDE_MAX)
}

/// Whether a frit's `text` names an outline source, `outline:` or a
/// family, `font:`, so a zoom steps it by 1 pixel (ADR-1200 D17).
fn frit_outline(text: &str) -> bool {
    text.split('\n').any(|line| {
        line.split('#')
            .next()
            .and_then(|body| body.split_once('='))
            .is_some_and(|(k, v)| {
                let v = v.trim();
                k.trim() == "source" && (v.starts_with("outline:") || v.starts_with("font:"))
            })
    })
}

/// What `vtium check-config` says of the cell the frit `name`'s `text`
/// states, where that needs no font parsed (#145): an unreadable
/// `cell=`, a cell smaller than the built-in Unscii-8's 8x8, which the
/// built-in is cropped to until the glyphs land, and a `builtin:` source
/// whose own cell does not fit it, which gives nothing. Whether a font
/// file's glyphs fit is vtium-vitrify's to check.
pub fn frit_warnings(name: &str, text: &str) -> Vec<String> {
    let cell = match frit_cell(text) {
        Ok(Some(cell)) => cell,
        Ok(None) => return Vec::new(),
        Err(why) => {
            return vec![format!(
                "the frit {name}: {why}; vtium draws its built-in font"
            )];
        }
    };
    let mut out = Vec::new();
    if cell.width < 8 || cell.height < 8 {
        out.push(format!(
            "the frit {name}: its {}x{} cell is smaller than the built-in font's 8x8, which is cropped to it until the glyphs are generated",
            cell.width, cell.height
        ));
    }
    for (line, n) in text.split('\n').zip(1usize..) {
        let body = line.split('#').next().unwrap_or_default();
        let Some((key, value)) = body.split_once('=') else {
            continue;
        };
        let own = match (key.trim(), value.trim()) {
            ("source", "builtin:unscii-8") => (8, 8),
            ("source", "builtin:unifont") => (8, 16),
            _ => continue,
        };
        if own.0 > cell.width || own.1 > cell.height {
            out.push(format!(
                "the frit {name}: line {n}: source={}'s {}x{} cell does not fit its {}x{} cell, so it gives nothing",
                value.trim(),
                own.0,
                own.1,
                cell.width,
                cell.height
            ));
        }
    }
    out
}

/// A frit's cell at `height` (#145): the cell it states, scaled by the
/// whole factor `height` reaches, at least 1; or with none stated,
/// `height` tall and as wide as `builtin`, the built-in font's cell at
/// `height`, never shorter than it (ADR-1200 D17).
fn frit_cell_at(stated: Option<Cell>, height: u32, builtin: Cell) -> Cell {
    match stated {
        Some(c) => {
            let most = CELL_SIDE_MAX
                .checked_div(c.width.max(c.height))
                .unwrap_or(1);
            let k = height
                .checked_div(c.height)
                .unwrap_or(1)
                .clamp(1, most.max(1));
            Cell {
                width: c.width.saturating_mul(k),
                height: c.height.saturating_mul(k),
            }
        }
        None => Cell {
            width: builtin.width,
            height: height.max(builtin.height),
        },
    }
}

/// The built-in face that draws in `cell` (#145): unifont at the largest
/// whole factor whose cell fits, else the smaller strike, Unscii-8 outside
/// a zh, ja or ko locale, cropped where even it does not fit.
fn builtin_in(cell: Cell, locale: &str) -> Option<Box<dyn Face>> {
    let k = (cell.width / 8).min(cell.height / 16);
    Chosen::Builtin(None).face(if k == 0 { 8 } else { k.saturating_mul(16) }, locale)
}

/// Where vtium finds frits: the frits' directory, the helper that
/// generates a frit's glyphs, and the files that name the font when the
/// configuration does not.
#[derive(Clone, Debug)]
pub struct Sources {
    pub frits: PathBuf,
    /// The helper; `None` for the system's, [`helper::Command::system`].
    pub helper: Option<helper::Command>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
}

impl Default for Sources {
    /// The system's: the frits' directory, the installed helper,
    /// `/etc/vconsole.conf` and `/proc/cmdline`.
    fn default() -> Sources {
        Sources {
            frits: default_frits(),
            helper: None,
            vconsole_conf: PathBuf::from("/etc/vconsole.conf"),
            kernel_cmdline: PathBuf::from("/proc/cmdline"),
        }
    }
}

/// Where the font's name came from, which picks the source of the frit
/// made on the fly for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Came {
    /// `font-name`: a path, or a kbd font's name.
    Config,
    /// `font-name` under `font-engine=freetype` or `pango`: an outline
    /// font's family, a `font:` source.
    Family,
    /// `vconsole.font=` on the kernel command line, or `FONT=` in
    /// vconsole.conf.
    Vconsole,
}

/// The font a name asks for, and where the name came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub name: String,
    pub came: Came,
}

impl Wanted {
    /// The one source of the frit made on the fly for the name (ADR-1200
    /// D9): `vconsole` for vconsole's font, `font:` for an outline family,
    /// `file:` for a path, else `kbd:`.
    fn source(&self) -> String {
        match self.came {
            Came::Vconsole => "vconsole".to_owned(),
            Came::Family => format!("font:{}", self.name),
            Came::Config if self.name.starts_with('/') => format!("file:{}", self.name),
            Came::Config => format!("kbd:{}", self.name),
        }
    }
}

/// `path`'s text, read as a configuration file is, a regular file of at
/// most `FILE_MAX` bytes; empty when there is none.
pub fn read_text(path: &Path) -> String {
    crate::config::read_file(path)
        .ok()
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// The font vconsole names, as systemd-vconsole-setup reads it: `FONT=` in
/// `conf`, with the kernel command line's `vconsole.font=` over it; the
/// last assignment of each wins.
pub fn vconsole_font(conf: &str, cmdline: &str) -> Option<String> {
    let unquote = |v: &str| {
        let v = v.trim();
        ['"', '\'']
            .iter()
            .find_map(|&q| v.strip_prefix(q).and_then(|v| v.strip_suffix(q)))
            .unwrap_or(v)
            .to_owned()
    };
    let mut font = None;
    for line in conf.lines().map(str::trim) {
        if let Some(("FONT", value)) = line.split_once('=').map(|(k, v)| (k.trim(), v)) {
            font = Some(unquote(value));
        }
    }
    for word in cmdline.split_whitespace() {
        if let Some(v) = word.strip_prefix("vconsole.font=") {
            font = Some(v.to_owned());
        }
    }
    font.filter(|f| !f.is_empty())
}

/// The font the configuration asks for (ADR-1200 D9): `font-name`, set in
/// a file or on the command line, unless `font-engine` names the built-in
/// unifont or the C reference's 8x16; unset, vconsole's font; else none,
/// and vtium draws its built-in font.
pub fn wanted(config: &Config, vconsole_conf: &str, cmdline: &str) -> Option<Wanted> {
    if matches!(config.text(&FONT_ENGINE), Some("unifont" | "8x16")) {
        return None;
    }
    let named = !matches!(config.origin(&FONT_NAME), None | Some(Origin::Default));
    match config.text(&FONT_NAME) {
        Some(name) if named && !name.is_empty() => Some(Wanted {
            name: name.to_owned(),
            came: match config.text(&FONT_ENGINE) {
                Some("freetype" | "pango") => Came::Family,
                _ => Came::Config,
            },
        }),
        _ => vconsole_font(vconsole_conf, cmdline).map(|name| Wanted {
            name,
            came: Came::Vconsole,
        }),
    }
}

/// The frit `wanted` names (ADR-1200 D9): the frit of that name in
/// `frits`; else a kbd name, a path, an outline family or vconsole's font
/// as a one-source frit made on the fly, named as it was named. The error
/// is why the frit of that name cannot be read.
pub fn frit_named(frits: &Path, wanted: &Wanted) -> Result<FritText, String> {
    if let Some(path) = frit(frits, &wanted.name) {
        return FritText::read(&path);
    }
    Ok(FritText {
        name: wanted.name.clone(),
        text: format!("source={}\n", wanted.source()),
    })
}

/// The built-in font `name` names, as its own name spells it.
pub fn builtin_named(name: &str) -> Option<Chosen> {
    ["unifont", "unscii-8"]
        .into_iter()
        .find(|b| *b == name)
        .map(|b| Chosen::Builtin(Some(b)))
}

/// The font named `name` for OSC 50 (ADR-1200 D5): a built-in, or a frit
/// in the frits' directory. The font keeps the name as the built-in or
/// the frit's file spells it, never the request's bytes.
pub fn by_name(sources: &Sources, name: &str) -> Result<Chosen, String> {
    if let Some(builtin) = builtin_named(name) {
        return Ok(builtin);
    }
    let path = frit(&sources.frits, name).ok_or("no frit is named so")?;
    Chosen::frit(FritText::read(&path)?, false)
}

/// The font vtium draws.
#[derive(Clone, Debug)]
pub enum Chosen {
    /// The built-in sets: the one named, or with `None`, Unscii-8 for
    /// cells under 16 pixels and unifont from 16, but unifont alone in a
    /// zh, ja or ko locale (ADR-1200 D7).
    Builtin(Option<&'static str>),
    /// A frit, generated at run time (ADR-1301 D2), in the cell it
    /// states, else the display's (#145): drawn in the built-in font, each
    /// character the helper has sent drawn from `generated` in its place.
    Frit {
        /// The name the font goes by, as it was given.
        name: String,
        /// The frit as the helper is handed it, under a frit's name.
        frit: FritText,
        /// `cell=`, unless `font-size` is configured.
        cell: Option<Cell>,
        /// `size=`, unless `font-size` is configured.
        size: Option<u32>,
        /// Its fonts' own cell, as the helper tells it for a frit of
        /// bitmap sources that states no cell or size.
        own: Option<Cell>,
        /// A source is an outline font, so a zoom steps by 1 pixel.
        outline: bool,
        generated: Shared,
    },
}

/// Heights `step` and its whole multiples, from `min` and under `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rung {
    pub step: u32,
    pub min: u32,
    pub end: u32,
}

impl Chosen {
    /// The frit `frit`, its cell and size read from its text before the
    /// first frame (#145), unless `sized`, a configured `font-size`, wins
    /// over them (ADR-1200 D16, D17). Its `size=` counts for an outline
    /// frit only: points do not apply to bitmap fonts (the operator's
    /// decision on vtium/vtium#145). A name that is not a frit's goes to
    /// the helper as [`GENERATED`]. The error is why they cannot be read.
    pub fn frit(mut frit: FritText, sized: bool) -> Result<Chosen, String> {
        let cell = frit_cell(&frit.text)?;
        let size = frit_size(&frit.text)?;
        let outline = frit_outline(&frit.text);
        let name = frit.name.clone();
        if !frit_name(&name) {
            GENERATED.clone_into(&mut frit.name);
        }
        Ok(Chosen::Frit {
            name,
            outline,
            frit,
            cell: cell.filter(|_| !sized),
            size: size.filter(|_| !sized && outline),
            own: None,
            generated: Shared::default(),
        })
    }

    /// The cell a frit states, `cell=`, in pixels (ADR-1200 D17).
    fn stated(&self) -> Option<Cell> {
        match self {
            Chosen::Frit { cell, .. } => *cell,
            Chosen::Builtin(_) => None,
        }
    }

    /// Whether the font is a frit that takes its fonts' own cell, asked of
    /// the helper (the operator, 2026-10-28): its sources bitmap, by its
    /// text, and no cell stated or size configured.
    pub fn wants_own_cell(&self) -> bool {
        matches!(
            self,
            Chosen::Frit {
                cell: None,
                own: None,
                outline: false,
                ..
            }
        )
    }

    /// Takes `cell`, the frit's fonts' own, as the helper told it.
    pub fn set_own_cell(&mut self, cell: Cell) {
        if let Chosen::Frit { own, .. } = self {
            *own = Some(cell);
        }
    }

    /// The cell a bitmap frit's heights are whole multiples of: the one it
    /// states, else its fonts' own.
    fn base_cell(&self) -> Option<Cell> {
        match self {
            Chosen::Frit { own, .. } => self.stated().or(*own),
            Chosen::Builtin(_) => None,
        }
    }

    /// Whether the font is a frit a zoom steps by 1 pixel.
    fn outline(&self) -> bool {
        matches!(self, Chosen::Frit { outline: true, .. })
    }

    /// The built-in font drawn at `height` in `locale`.
    fn builtin_at(&self, height: u32, locale: &str) -> &'static str {
        match self {
            Chosen::Builtin(Some(name)) => name,
            Chosen::Builtin(None) | Chosen::Frit { .. } => builtin_name(height, locale),
        }
    }

    /// The font's name at `height` in `locale`, as OSC 50 spells it: the
    /// frit's, else the built-in's.
    pub fn name(&self, height: u32, locale: &str) -> &str {
        match self {
            Chosen::Frit { name, .. } => name,
            Chosen::Builtin(_) => self.builtin_at(height, locale),
        }
    }

    /// The face at `height`: a frit's, in its cell at that height, drawn
    /// in the built-in font until the helper's glyphs land; else the
    /// built-in's strike of that height, else the tallest under it scaled
    /// by the largest whole factor that fits. The built-in unifont draws
    /// through its own face.
    pub fn face(&self, height: u32, locale: &str) -> Option<Box<dyn Face>> {
        if let Chosen::Frit { generated, .. } = self {
            let at = Chosen::Builtin(None).face(height, locale)?;
            let stated = self.base_cell();
            let (cell, step) = match (stated, self.outline()) {
                // D17, the C reference's outline increase_step of 1: the
                // stated cell at its own height, else D17's width
                (Some(c), true) if c.height == height => (c, 1),
                (_, true) => (frit_cell_at(None, height, at.cell()), 1),
                // D17, the C reference's PSF increase_step of the font's
                // height: the stated or own cell's whole multiples
                (Some(c), false) => (frit_cell_at(Some(c), height, at.cell()), c.height),
                // D17: with no cell stated, the display's
                (None, false) => (frit_cell_at(None, height, at.cell()), at.size_step()),
            };
            let builtin = builtin_in(cell, locale)?;
            return Some(Box::new(DeferredFace::new(
                builtin,
                cell,
                step,
                Rc::clone(generated),
            )));
        }
        match self.builtin_at(height, locale) {
            "unifont" => Some(Box::new(Unifont::new(height))),
            name => SetFace::new(builtin(name)?, height).map(|f| Box::new(f) as Box<dyn Face>),
        }
    }

    /// The heights a zoom steps through (ADR-1200 D17): an outline frit's
    /// every height from the built-in's smallest, a bitmap frit's stated
    /// or own cell's height and its whole multiples. The built-ins, and a bitmap
    /// frit with no cell stated, step through Unscii-8's 8 under 16, where
    /// it is drawn, and unifont's multiples of 16 from 16.
    fn rungs(&self, locale: &str) -> Vec<Rung> {
        let all = |step| Rung {
            step,
            min: step,
            end: u32::MAX,
        };
        // D17, the C reference's outline increase_step of 1
        if self.outline() {
            return vec![Rung {
                step: 1,
                min: Chosen::Builtin(None).smallest(locale),
                end: u32::MAX,
            }];
        }
        // D17, the C reference's PSF increase_step of the font's height
        if let Some(cell) = self.base_cell() {
            return vec![all(cell.height)];
        }
        match self {
            Chosen::Builtin(Some("unscii-8")) => vec![all(8)],
            Chosen::Builtin(Some(_)) => vec![all(16)],
            Chosen::Builtin(None) | Chosen::Frit { .. }
                if builtin_name(8, locale) == "unscii-8" =>
            {
                vec![
                    Rung {
                        step: 8,
                        min: 8,
                        end: 16,
                    },
                    all(16),
                ]
            }
            Chosen::Builtin(None) | Chosen::Frit { .. } => vec![all(16)],
        }
    }

    /// The height `font-size=0` picks for a display `width` by `height` at
    /// `dpi`: the height of the cell a frit states, or an outline frit's
    /// `size=` in points at `dpi`, which no display's size moves (#145,
    /// ADR-1200 D17); else [`auto_size`] for the font's smallest height,
    /// which a frit's own cell takes at the largest whole multiple not
    /// over it, at least the cell itself, as the C reference scaled a PSF
    /// font (the operator, 2026-10-28).
    pub fn auto_height(&self, width: u32, height: u32, locale: &str, dpi: f64) -> u32 {
        if let Some(cell) = self.stated() {
            return cell.height;
        }
        match self {
            Chosen::Frit {
                size: Some(points), ..
            } => points_to_pixels(*points, dpi),
            _ => self.snap(auto_size(width, height, self.smallest(locale))),
        }
    }

    /// The height the font draws when `height` is asked for: a bitmap
    /// frit's stated or own cell's whole multiple at or under it, at least
    /// the cell itself (#145, ADR-1200 D17); else `height`.
    pub fn snap(&self, height: u32) -> u32 {
        match self.base_cell() {
            Some(cell) if !self.outline() => frit_cell_at(Some(cell), height, cell).height,
            _ => height,
        }
    }

    /// The height a zoom asks for from `size` at `dpi` (ADR-1200 D17): an
    /// outline frit's next whole point in its direction whose pixels
    /// differ from `size`, as the C reference's outline engines stepped by
    /// 1 in their own units (the operator's decision on vtium/vtium#145);
    /// else the next of its rungs. `None` past [`ZOOM_IN_MAX`]
    /// in, or below the smallest out.
    pub fn zoom(&self, size: u32, dir: Zoom, locale: &str, dpi: f64) -> Option<u32> {
        if !self.outline() {
            return zoom(&self.rungs(locale), size, dir);
        }
        // the whole points at or under `size`; `as` saturates
        let under = (f64::from(size) * POINTS_PER_INCH / dpi).floor() as u32;
        match dir {
            Zoom::In if size > ZOOM_IN_MAX => None,
            Zoom::In => (under.saturating_add(1)..=under.saturating_add(POINTS_SCAN))
                .map(|p| points_to_pixels(p, dpi))
                .find(|&h| h > size),
            Zoom::Out => (under.saturating_sub(POINTS_SCAN)..=under)
                .rev()
                .map(|p| points_to_pixels(p, dpi))
                .find(|&h| h < size)
                .filter(|&h| h >= self.smallest(locale)),
        }
    }

    /// The smallest height the font draws.
    pub fn smallest(&self, locale: &str) -> u32 {
        self.rungs(locale)
            .iter()
            .map(|r| r.min)
            .min()
            .unwrap_or(FALLBACK_SIZE)
    }
}

/// The largest height a zoom in starts from, as the C reference refused
/// above 150 pixels.
pub const ZOOM_IN_MAX: u32 = 150;

/// The points an outline frit's zoom looks through for a height that
/// differs: a pixel is at most 3.6 points at vtium-platform's coarsest
/// density believed, 20 DPI, and rounding moves it by under one more.
const POINTS_SCAN: u32 = 5;

/// The height a zoom asks for from `size`, the next of `rungs` in its
/// direction; `None` past [`ZOOM_IN_MAX`] in, or below the smallest out.
pub fn zoom(rungs: &[Rung], size: u32, dir: Zoom) -> Option<u32> {
    let heights = rungs.iter().filter(|r| r.step > 0);
    match dir {
        Zoom::In if size > ZOOM_IN_MAX => None,
        Zoom::In => heights
            .filter_map(|r| {
                let next = size
                    .checked_div(r.step)?
                    .checked_add(1)?
                    .checked_mul(r.step)?;
                let next = next.max(r.min);
                (next < r.end).then_some(next)
            })
            .min(),
        Zoom::Out => heights
            .filter_map(|r| {
                // the multiple under `size`, and the last under `end`
                let floor = |n: u32| n.checked_div(r.step)?.checked_mul(r.step);
                let below = floor(size.checked_sub(1)?)?.min(floor(r.end.saturating_sub(1))?);
                (below >= r.min).then_some(below)
            })
            .max(),
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn config(args: &[&str]) -> Config {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        crate::config::load(&args, None, &mut |_: &std::path::Path| Ok(None)).config
    }

    #[test]
    fn the_policy_is_the_faces_unless_a_key_sets_it() {
        let named = SpritePolicy::all(Source::Font).with(Range::Octants, Source::Sprites);
        assert_eq!(policy(&config(&[]), named), named);
        assert_eq!(
            policy(&config(&["--glyph-source=sprites"]), named),
            SpritePolicy::SPRITES
        );
        let one = policy(&config(&["--glyph-source-braille=auto"]), named);
        assert_eq!(one, named.with(Range::Braille, Source::Auto));
        // A range's own key wins over the key for all six.
        let both = policy(
            &config(&["--glyph-source=font", "--glyph-source-legacy=sprites"]),
            SpritePolicy::SPRITES,
        );
        assert_eq!(
            both,
            SpritePolicy::all(Source::Font).with(Range::LegacyComputing, Source::Sprites)
        );
    }

    // ADR-1200 D1: a frit's glyph-source= lines are its sprite policy, over
    // the face's, the configuration's keys over both
    #[test]
    fn a_frits_glyph_source_is_its_policy_under_the_keys() {
        let face = SpritePolicy::SPRITES;
        assert_eq!(frit_policy("source=kbd:x\n", face), face);
        let frit = "source=kbd:x\nglyph-source=font # all six\nglyph-source-braille = auto\n";
        let own = SpritePolicy::all(Source::Font).with(Range::Braille, Source::Auto);
        assert_eq!(frit_policy(frit, face), own);
        // a value or a range vtium does not know is the helper's to refuse
        assert_eq!(
            frit_policy("glyph-source=x\nglyph-source-y=font\n", face),
            face
        );
        assert_eq!(
            policy(
                &config(&["--glyph-source-braille=sprites"]),
                frit_policy(frit, face)
            ),
            own.with(Range::Braille, Source::Sprites)
        );
    }

    #[test]
    fn missing_is_box_or_hex() {
        assert_eq!(missing(&config(&[])), Missing::Replacement);
        assert_eq!(missing(&config(&["--missing=hex"])), Missing::Hex);
        assert_eq!(missing(&config(&["--missing=box"])), Missing::Replacement);
    }

    #[test]
    fn the_automatic_size() {
        assert_eq!(auto_size(1366, 768, 8), 19);
        assert_eq!(auto_size(1280, 800, 8), 20);
        assert_eq!(auto_size(3840, 2160, 8), 54);
        assert_eq!(auto_size(640, 480, 8), 16);
        assert_eq!(auto_size(640, 400, 8), 16, "80 by 25 at 16 pixels");
        assert_eq!(auto_size(0, 768, 8), 16);
    }

    // vtium/vtium#118: where 16-pixel cells leave fewer than 80 by 25, the
    // font's own smallest height.
    #[test]
    fn a_small_display_takes_the_fonts_own_height() {
        assert_eq!(auto_size(320, 240, 8), 8, "40 by 30 cells of 8x8");
        assert_eq!(auto_size(480, 320, 8), 8);
        assert_eq!(auto_size(632, 480, 8), 8, "79 columns at 16");
        assert_eq!(auto_size(640, 384, 8), 8, "24 rows at 16");
        assert_eq!(auto_size(320, 240, 12), 12);
        // A font no smaller than 16 stays at 16, as unifont does in a
        // CJK locale.
        assert_eq!(auto_size(320, 240, 16), 16);
        assert_eq!(auto_size(320, 240, 24), 16);
    }

    fn rungs(steps: &[u32]) -> Vec<Rung> {
        steps
            .iter()
            .map(|&step| Rung {
                step,
                min: step,
                end: u32::MAX,
            })
            .collect()
    }

    fn cell(width: u32, height: u32) -> Cell {
        Cell { width, height }
    }

    fn frit_font(stated: Option<Cell>) -> Chosen {
        let cell = stated.map_or_else(String::new, |c| format!("cell={}x{}\n", c.width, c.height));
        frit_of(&format!("source=kbd:x\n{cell}"), false)
    }

    // #145: the console reads a frit's cell as text, before its first frame
    #[test]
    fn a_frits_cell_is_read_from_its_text() {
        assert_eq!(frit_cell("source=kbd:x\n"), Ok(None));
        assert_eq!(
            frit_cell("# c\nsource=outline:/f.ttf\n cell = 10x20 # the cell\n"),
            Ok(Some(cell(10, 20)))
        );
        for (text, why) in [
            ("cell=10\n", "line 1: cell=10 is not WIDTHxHEIGHT"),
            ("\ncell=5x16\n", "line 2: cell=5x16 is not"),
            ("cell=8x97\n", "is not"),
            ("cell=08x16\n", "is not"),
            ("cell=+8x16\n", "is not"),
            ("cell=8x16\ncell=8x16\n", "line 2: cell= given twice"),
        ] {
            let e = frit_cell(text).expect_err(text);
            assert!(e.contains(why), "{text:?}: {e}");
        }
        // a comment holding cell= states nothing
        assert_eq!(frit_cell("# cell=1x1\n"), Ok(None));
    }

    // #145: the built-in draws in the stated cell, the helper's glyphs at
    // that cell, a zoom by its whole multiples
    #[test]
    fn a_stated_cell_is_the_faces_at_each_of_its_multiples() {
        let f = frit_font(Some(cell(10, 20)));
        let face = |h| f.face(h, "C").expect("a face");
        assert_eq!(face(20).cell(), cell(10, 20));
        assert_eq!(face(16).cell(), cell(10, 20), "never smaller than stated");
        assert_eq!(face(45).cell(), cell(20, 40));
        assert_eq!(face(20).size_step(), 20);
        assert_eq!((f.snap(16), f.snap(45)), (20, 40));
        assert_eq!(frit_font(None).snap(19), 19);
        assert_eq!(
            f.rungs("C"),
            [Rung {
                step: 20,
                min: 20,
                end: u32::MAX
            }]
        );
        assert_eq!(zoom(&f.rungs("C"), 20, Zoom::In), Some(40));
        assert_eq!(zoom(&f.rungs("C"), 20, Zoom::Out), None);
        // the display moves no stated cell
        assert_eq!(f.auto_height(3840, 2160, "C", 96.0), 20);
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 20);
    }

    fn frit_of(text: &str, sized: bool) -> Chosen {
        let frit = FritText {
            name: "f".into(),
            text: text.into(),
        };
        Chosen::frit(frit, sized).expect("a frit")
    }

    // ADR-1200 D17: at 72 DPI, the C reference's fixed density, an
    // outline frit zooms by 1 pixel, as its outline engines' increase_step
    // did, from its stated cell, each height but the stated one as wide as
    // the built-in font there
    #[test]
    fn an_outline_frit_at_72_dpi_zooms_by_one_pixel() {
        for text in [
            "source=outline:/f.ttf\ncell=10x20\n",
            "source=font:DejaVu Sans Mono\ncell=10x20\n",
            "source=kbd:x\nsource=outline:/f.ttf\ncell=10x20\n",
        ] {
            let f = frit_of(text, false);
            assert_eq!(f.zoom(20, Zoom::In, "C", 72.0), Some(21), "{text}");
            assert_eq!(f.zoom(20, Zoom::Out, "C", 72.0), Some(19), "{text}");
            assert_eq!(f.snap(21), 21, "{text}");
            let cell_at = |h| f.face(h, "C").expect("a face").cell();
            assert_eq!(cell_at(20), cell(10, 20), "{text}");
            assert_eq!(cell_at(21), cell(8, 21), "{text}");
            assert_eq!(cell_at(33), cell(16, 33), "{text}");
            assert_eq!(f.face(21, "C").expect("a face").size_step(), 1);
        }
        // down to the built-in font's smallest, which it draws in meanwhile
        let f = frit_of("source=outline:/f.ttf\n", false);
        assert_eq!(f.zoom(9, Zoom::Out, "C", 72.0), Some(8));
        assert_eq!(f.zoom(8, Zoom::Out, "C", 72.0), None);
    }

    // the operator's decision on #145, 2026-10-28: a bitmap frit with no
    // cell stated asks for its fonts' own cell, and takes it at the largest
    // whole multiple not over the automatic height, at least once, as the C
    // reference scaled a PSF font; it zooms by the cell's height
    #[test]
    fn a_frits_own_cell_scales_by_whole_factors_under_the_automatic_height() {
        for text in ["source=kbd:x\n", "source=kbd:x\nsize=16\n"] {
            assert!(frit_of(text, false).wants_own_cell(), "{text}");
        }
        for text in [
            "source=kbd:x\ncell=8x16\n",
            "source=outline:/f.ttf\n",
            "source=kbd:x\nsource=font:Mono\n",
        ] {
            assert!(!frit_of(text, false).wants_own_cell(), "{text}");
        }
        let mut f = frit_of("source=kbd:x\n", false);
        f.set_own_cell(cell(8, 16));
        assert!(!f.wants_own_cell());
        // 1366x768's automatic 19, 1920x1080's 27 and 3840x2160's 54
        assert_eq!(f.auto_height(1366, 768, "C", 96.0), 16);
        assert_eq!(f.auto_height(1920, 1080, "C", 96.0), 16);
        assert_eq!(f.auto_height(3840, 2160, "C", 96.0), 48);
        // at least the cell, under #118's floor too
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 16);
        assert_eq!(f.face(16, "C").expect("a face").cell(), cell(8, 16));
        assert_eq!(f.face(48, "C").expect("a face").cell(), cell(24, 48));
        assert_eq!(zoom(&f.rungs("C"), 16, Zoom::In), Some(32));
        assert_eq!(zoom(&f.rungs("C"), 16, Zoom::Out), None);
        let mut f = frit_of("source=kbd:x\n", false);
        f.set_own_cell(cell(8, 8));
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 8);
        assert_eq!(f.auto_height(640, 480, "C", 96.0), 16);
        assert_eq!(f.face(16, "C").expect("a face").cell(), cell(16, 16));
    }

    // ADR-1200 D1, D17: an outline frit's size= alone is its starting
    // size, the cell as wide as the built-in font at its height; points do
    // not apply to a bitmap frit, whose size= the console leaves aside (the
    // operator's decision on #145, 2026-10-28)
    #[test]
    fn size_alone_starts_an_outline_frit_and_cell_wins_over_it() {
        let f = frit_of("source=outline:/f.ttf\nsize=24\n", false);
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 32);
        assert_eq!(f.face(32, "C").expect("a face").cell(), cell(16, 32));
        let f = frit_of("source=kbd:x\nsize=24\n", false);
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 8, "the display's");
        // the cell is cell='s; size= is the glyph's size in it
        let f = frit_of("source=outline:/f.ttf\ncell=10x20\nsize=16\n", false);
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 20);
        assert_eq!(f.face(20, "C").expect("a face").cell(), cell(10, 20));
        // a configured font-size wins over both
        let f = frit_of("source=outline:/f.ttf\ncell=10x20\nsize=16\n", true);
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 8);
        for (text, why) in [
            ("size=24\nsize=32\n", "line 2: size= given twice"),
            (
                "size=5\n",
                "line 1: size=5 is not a size in points, 6 to 96",
            ),
            ("size=016\n", "line 1: size=016 is not"),
        ] {
            let frit = FritText {
                name: "f".into(),
                text: text.into(),
            };
            let e = Chosen::frit(frit, false).expect_err(text);
            assert!(e.contains(why), "{text:?}: {e}");
        }
    }

    // the operator's decision on #145, 2026-10-28: an outline frit's size=
    // is points, pixels = points x DPI / 72; a 27-inch 2560x1440 monitor is
    // about 109 DPI, and a display that measures none is assumed 96
    #[test]
    fn an_outline_frits_size_is_points_at_the_displays_density() {
        let f = frit_of("source=outline:/f.ttf\nsize=12\n", false);
        assert_eq!(f.auto_height(2560, 1440, "C", 108.86), 18);
        assert_eq!(f.auto_height(2560, 1440, "C", 96.0), 16);
        assert_eq!(f.auto_height(320, 240, "C", 300.0), 50);
        // cell= stays in pixels, whatever the density
        let f = frit_of("source=outline:/f.ttf\ncell=10x20\nsize=12\n", false);
        assert_eq!(f.auto_height(2560, 1440, "C", 300.0), 20);
    }

    // the operator's decision on #145, 2026-10-28: an outline frit zooms 1
    // pt at a time, converted to pixels, to the next point whose pixels
    // differ
    #[test]
    fn an_outline_frit_zooms_by_one_point() {
        let f = frit_of("source=outline:/f.ttf\nsize=12\n", false);
        // 12 pt at 96 DPI is 16 px; 13 pt is 17.3, 14 pt 18.7 and 11 pt 14.7
        assert_eq!(f.zoom(16, Zoom::In, "C", 96.0), Some(17));
        assert_eq!(f.zoom(17, Zoom::In, "C", 96.0), Some(19));
        assert_eq!(f.zoom(16, Zoom::Out, "C", 96.0), Some(15));
        // at 300 DPI a point is over 4 pixels: 12 pt is 50, 13 pt 54.2
        assert_eq!(f.zoom(50, Zoom::In, "C", 300.0), Some(54));
        assert_eq!(f.zoom(50, Zoom::Out, "C", 300.0), Some(46));
        // at 50 DPI 15 pt is 10.4 px, so in from 10 is 16 pt's 11
        assert_eq!(f.zoom(10, Zoom::In, "C", 50.0), Some(11));
        // down to the built-in font's smallest, and not in past 150
        assert_eq!(f.zoom(9, Zoom::Out, "C", 96.0), Some(8));
        assert_eq!(f.zoom(8, Zoom::Out, "C", 96.0), None);
        assert_eq!(f.zoom(150, Zoom::In, "C", 96.0), Some(151));
        assert_eq!(f.zoom(151, Zoom::In, "C", 96.0), None);
        // a bitmap frit and the built-ins step by their cells, at any density
        let b = Chosen::Builtin(None);
        assert_eq!(b.zoom(16, Zoom::In, "C", 300.0), Some(32));
        let mut k = frit_of("source=kbd:x\n", false);
        k.set_own_cell(cell(8, 16));
        assert_eq!(k.zoom(16, Zoom::In, "C", 300.0), Some(32));
    }

    // #145: under 16 pixels the smaller strike, Unscii-8, draws
    #[test]
    fn a_small_stated_cell_draws_unscii_8_in_it() {
        let f = frit_font(Some(cell(7, 12)));
        let face = f.face(12, "C").expect("a face");
        assert_eq!(face.cell(), cell(7, 12));
        let a = face
            .draw(&['A'], vtium_font::Style::default())
            .expect("a glyph");
        assert_eq!((a.width(), a.height()), (7, 12));
        let unscii = Chosen::Builtin(Some("unscii-8"))
            .face(8, "C")
            .expect("unscii-8")
            .draw(&['A'], vtium_font::Style::default())
            .expect("a glyph");
        // Unscii-8's 8x8, two rows down, its last column cropped
        for y in 0..8 {
            for x in 0..7 {
                assert_eq!(a.pixel(x, y + 2), unscii.pixel(x, y), "{x},{y}");
            }
        }
    }

    // ADR-1200 D17: with no stated cell, the display's: the C reference's
    // automatic height, #118's floor first, as wide as the built-in font
    // is at that height
    #[test]
    fn with_no_stated_cell_the_cell_comes_from_the_display() {
        let f = frit_font(None);
        let at = |w, h| {
            let height = f.auto_height(w, h, "C", 96.0);
            f.face(height, "C").expect("a face").cell()
        };
        assert_eq!(at(320, 240), cell(8, 8), "40 by 30");
        assert_eq!(at(640, 480), cell(8, 16), "80 by 30");
        assert_eq!(at(1366, 768), cell(8, 19), "unifont's 8 at 19");
        assert_eq!(at(1920, 1080), cell(8, 27), "unifont's 8 at 27");
        assert_eq!(at(3840, 2160), cell(24, 54), "unifont's 24 at 54");
        assert_eq!(f.face(27, "C").expect("a face").size_step(), 16);
        // unifont alone in a zh, ja or ko locale: 16 at the floor
        let height = f.auto_height(320, 240, "ja_JP.UTF-8", 96.0);
        assert_eq!(
            f.face(height, "ja_JP.UTF-8").expect("a face").cell(),
            cell(8, 16)
        );
    }

    #[test]
    fn check_config_names_a_cell_that_does_not_suit_its_frit() {
        assert!(frit_warnings("f", "source=kbd:x\n").is_empty());
        assert!(frit_warnings("f", "source=builtin:unifont\ncell=8x16\n").is_empty());
        assert_eq!(
            frit_warnings("f", "cell=8\n"),
            [
                "the frit f: line 1: cell=8 is not WIDTHxHEIGHT in pixels, each 6 to 96; vtium draws its built-in font"
            ]
        );
        assert_eq!(
            frit_warnings(
                "f",
                "source=outline:/f.ttf\nsource=builtin:unifont\nsource=builtin:unscii-8\ncell=6x12\n"
            ),
            [
                "the frit f: its 6x12 cell is smaller than the built-in font's 8x8, which is cropped to it until the glyphs are generated",
                "the frit f: line 2: source=builtin:unifont's 8x16 cell does not fit its 6x12 cell, so it gives nothing",
                "the frit f: line 3: source=builtin:unscii-8's 8x8 cell does not fit its 6x12 cell, so it gives nothing",
            ]
        );
    }

    #[test]
    fn zoom_steps_through_the_strikes_and_their_multiples() {
        let one = rungs(&[16]);
        assert_eq!(zoom(&one, 16, Zoom::In), Some(32));
        assert_eq!(zoom(&one, 32, Zoom::Out), Some(16));
        assert_eq!(zoom(&one, 19, Zoom::In), Some(32));
        assert_eq!(zoom(&one, 19, Zoom::Out), Some(16));
        // Two strikes: 12 and 16 and their multiples.
        let two = rungs(&[12, 16]);
        let mut up = vec![12];
        while let Some(next) = zoom(&two, *up.last().unwrap(), Zoom::In) {
            up.push(next);
        }
        assert_eq!(
            up[..9],
            [12, 16, 24, 32, 36, 48, 60, 64, 72],
            "each strike's multiples, in order"
        );
        assert_eq!(zoom(&two, 24, Zoom::Out), Some(16));
        assert_eq!(zoom(&two, 16, Zoom::Out), Some(12));
    }

    #[test]
    fn zoom_stops_at_the_smallest_and_past_150() {
        let two = rungs(&[12, 16]);
        assert_eq!(zoom(&two, 12, Zoom::Out), None);
        assert_eq!(zoom(&two, 8, Zoom::Out), None);
        assert_eq!(zoom(&rungs(&[16]), 150, Zoom::In), Some(160));
        assert_eq!(zoom(&rungs(&[16]), 151, Zoom::In), None);
    }

    // ADR-1200 D7: Unscii-8 under 16 pixels and unifont from 16, but in a
    // zh, ja or ko locale.
    #[test]
    fn the_builtins_step_from_unscii_8_to_unifont() {
        let b = Chosen::Builtin(None);
        let r = b.rungs("en_US.UTF-8");
        assert_eq!(zoom(&r, 8, Zoom::In), Some(16));
        assert_eq!(zoom(&r, 16, Zoom::In), Some(32));
        assert_eq!(zoom(&r, 16, Zoom::Out), Some(8));
        assert_eq!(zoom(&r, 12, Zoom::Out), Some(8));
        assert_eq!(zoom(&r, 8, Zoom::Out), None);
        assert_eq!(b.smallest("en_US.UTF-8"), 8);
        assert_eq!((b.name(8, "C"), b.name(16, "C")), ("unscii-8", "unifont"));
        let cell = |h: u32, locale: &str| {
            let c = b.face(h, locale).expect("a face").cell();
            (c.width, c.height)
        };
        assert_eq!(cell(8, "C"), (8, 8));
        assert_eq!(cell(16, "C"), (8, 16));
        assert_eq!(cell(24, "C"), (8, 16));
        assert_eq!(cell(32, "C"), (16, 32));
        // In a CJK locale, unifont at every size.
        let r = b.rungs("ja_JP.UTF-8");
        assert_eq!(zoom(&r, 16, Zoom::Out), None);
        assert_eq!(b.smallest("zh_CN.UTF-8"), 16);
        assert_eq!(cell(8, "ko_KR.UTF-8"), (8, 16));
        assert_eq!(b.name(8, "ko_KR.UTF-8"), "unifont");
        // One built-in named keeps to its own steps.
        let unscii = Chosen::Builtin(Some("unscii-8"));
        assert_eq!(zoom(&unscii.rungs("C"), 8, Zoom::In), Some(16));
        assert_eq!(unscii.face(16, "C").expect("a face").cell().width, 16);
        let unifont = Chosen::Builtin(Some("unifont"));
        assert_eq!(unifont.smallest("C"), 16);
    }

    #[test]
    fn vconsole_names_the_font_with_the_kernel_over_it() {
        assert_eq!(vconsole_font("", ""), None);
        assert_eq!(
            vconsole_font("KEYMAP=us\nFONT=ter-v16n\n", "").as_deref(),
            Some("ter-v16n")
        );
        assert_eq!(
            vconsole_font("FONT=\"lat2-16\"\n# FONT=x\nFONT = 'eurlatgr'\n", "").as_deref(),
            Some("eurlatgr")
        );
        assert_eq!(
            vconsole_font("FONT=ter-v16n\n", "quiet vconsole.font=ter-v32n rw").as_deref(),
            Some("ter-v32n")
        );
        assert_eq!(vconsole_font("FONT=\n", ""), None);
    }

    #[test]
    fn the_font_named_is_the_configurations_then_vconsoles() {
        let conf = "FONT=ter-v16n\n";
        assert_eq!(
            wanted(&config(&[]), conf, ""),
            Some(Wanted {
                name: "ter-v16n".into(),
                came: Came::Vconsole
            })
        );
        assert_eq!(wanted(&config(&[]), "", ""), None, "the built-ins");
        let path = "/usr/share/kbd/consolefonts/ter-v16n.psf.gz";
        let named = wanted(
            &config(&["--font-engine=psf", &format!("--font-name={path}")]),
            conf,
            "",
        )
        .expect("named");
        assert_eq!((named.name.as_str(), named.came), (path, Came::Config));
        let family = wanted(
            &config(&["--font-engine=pango", "--font-name=DejaVu Sans Mono"]),
            "",
            "",
        )
        .expect("named");
        assert_eq!(family.came, Came::Family);
        // `font-engine=unifont` or `8x16` draws the built-ins.
        for engine in ["unifont", "8x16"] {
            let c = config(&[&format!("--font-engine={engine}"), "--font-name=x"]);
            assert_eq!(wanted(&c, conf, ""), None, "{engine}");
        }
    }

    // ADR-1200 D9: a name no frit bears is a one-source frit made on the
    // fly, named as it was named: kbd:, file:, font: or vconsole
    #[test]
    fn a_name_no_frit_bears_is_a_one_source_frit() {
        let none = Path::new("/nonexistent/vtium-test-frits");
        let made = |name: &str, came| {
            frit_named(
                none,
                &Wanted {
                    name: name.into(),
                    came,
                },
            )
        };
        let text = |name: &str, came| made(name, came).map(|f| (f.name, f.text));
        assert_eq!(
            text("Lat2-Terminus16", Came::Config),
            Ok((
                "Lat2-Terminus16".into(),
                "source=kbd:Lat2-Terminus16\n".into()
            ))
        );
        assert_eq!(
            text("/fonts/my font.psf", Came::Config),
            Ok((
                "/fonts/my font.psf".into(),
                "source=file:/fonts/my font.psf\n".into()
            ))
        );
        assert_eq!(
            text("lat9w-16", Came::Vconsole),
            Ok(("lat9w-16".into(), "source=vconsole\n".into()))
        );
        assert_eq!(
            text("DejaVu Sans Mono", Came::Family),
            Ok((
                "DejaVu Sans Mono".into(),
                "source=font:DejaVu Sans Mono\n".into()
            ))
        );
    }

    #[test]
    fn the_size_asked_for() {
        assert_eq!(select(24, false, |s| Ok::<_, ()>(s * 2)), Ok((48, 24)));
    }

    #[test]
    fn with_no_font_in_use_a_failure_falls_back_once() {
        let mut asked = Vec::new();
        let found = select(99, false, |s| {
            asked.push(s);
            if s == FALLBACK_SIZE {
                Ok("font")
            } else {
                Err("no 99")
            }
        });
        assert_eq!(found, Ok(("font", FALLBACK_SIZE)));
        assert_eq!(asked, [99, FALLBACK_SIZE]);
    }

    #[test]
    fn a_lookup_that_always_fails_is_an_error_after_the_fallback() {
        let mut asked = Vec::new();
        let found = select(99, false, |s| {
            asked.push(s);
            Err::<(), _>(s)
        });
        assert_eq!(found, Err(FALLBACK_SIZE));
        assert_eq!(asked, [99, FALLBACK_SIZE]);
    }

    #[test]
    fn with_a_font_in_use_a_failure_is_the_error() {
        let mut asked = Vec::new();
        let found = select(99, true, |s| {
            asked.push(s);
            Err::<(), _>(s)
        });
        assert_eq!(found, Err(99));
        assert_eq!(asked, [99]);
    }
}
