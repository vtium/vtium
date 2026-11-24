//! The font: which frit or built-in font vtium draws, its faces at each
//! height, its zoom steps and the automatic size, with the lookup's
//! fallback.
//!
//! The font is `font-name`, else vconsole's (ADR-1200 D9), else the
//! default set `terminus` where it is installed (ADR-1204 D2). A name is a
//! frit's (`NAME.vtfrit` in the frits' directory), else a kbd name, a path
//! or vconsole's font becomes a one-source frit made on the fly
//! ([`frit_named`]). A frit is generated at run time (ADR-1301 D2), in
//! the cell it states ([`frit_cell`], #145), else the cell its first
//! source gives, which the helper tells at the height the console draws
//! at, before the first frame and at each zoom of an outline frit: a
//! bitmap source's own cell (the operator, 2026-10-28), or an outline
//! source's cell at that height (the maintainer, 2026-11-01); else the
//! display's: the
//! built-in font draws in that cell at once, and the helper's glyphs
//! replace it, character by character, as they arrive ([`Chosen::Frit`],
//! [`crate::deferred`], [`crate::helper`]). The built-ins are Unscii-8
//! for cells under 16 pixels and unifont from 16, but in a zh, ja or ko
//! locale, which Unscii-8 does not cover (D7). OSC 50 switches among the
//! frits, the built-ins, the head's frit made on the fly and the fonts
//! root installed and trusts, given by their paths, while vtium runs, each
//! by the name its query reports ([`by_name`], D5).
//!
//! A font is looked up at the size asked for. When none is in use yet, a
//! failure tries [`FALLBACK_SIZE`] once, and a second failure is the
//! error; with a font in use, as at a zoom, a failure is the error and
//! that font stays, so a font that cannot be found never holds the start
//! in a loop.

use std::path::{Path, PathBuf};

use vtium_font::set::{SetFace, builtin, builtin_name, layer};
use vtium_font::spacing::{Band, LINE_SPACING_MAX};
use vtium_font::sprite::Range;
use vtium_font::{CELL_SIDE_MAX, Cell, Face, Missing, Source, SpritePolicy, Unifont};
use vtium_settings::cullet::{self, Rule};
use vtium_settings::mounts::Mounts;
use vtium_settings::{Trust, batch};

use crate::config::keys::{
    FONT_ENGINE, FONT_FRITS, FONT_NAME, FONT_PATHS, GLYPH_SOURCE, GLYPH_SOURCE_BLOCK,
    GLYPH_SOURCE_BOX, GLYPH_SOURCE_BRAILLE, GLYPH_SOURCE_LEGACY, GLYPH_SOURCE_OCTANTS,
    GLYPH_SOURCE_SEXTANTS, Key, MISSING,
};
use crate::config::{Config, Origin};
use crate::deferred::{DeferredFace, Shared};
use crate::helper::{self, FritText, OwnCell};
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
/// never under 16; but where 16-pixel cells
/// would leave fewer than 80 by 25, the font's own smallest height, so a
/// 320x240 display draws an 8x8 font as 40 by 30 cells (vtium/vtium#118).
pub fn auto_size(width: u32, height: u32, smallest: u32) -> u32 {
    if width == 0 || height == 0 {
        return FALLBACK_SIZE;
    }
    let by_width = width.saturating_mul(2) / 120;
    let size = by_width.min(height / 40).max(16);
    if small_display(width, height) {
        size.min(smallest.max(1))
    } else {
        size
    }
}

/// Whether a display `width` by `height` is small: 16-pixel cells, 8
/// pixels wide, would leave it fewer than 80 columns or 25 rows. There
/// [`auto_size`] takes the font's own smallest height, and with no font
/// named vtium draws the built-in font's 8×8 strike in place of the
/// default set, so a 320×240 panel shows 40 columns by 30 rows. A display
/// whose size is not known yet, 0 by 0, is not small.
///
/// References:
/// - ADR-1200 D17: the small-display rule.
/// - ADR-1204 D2: an 8-pixel cell draws the built-in font's 8×8 strike,
///   because Terminus has no strike under 12 pixels.
pub fn small_display(width: u32, height: u32) -> bool {
    width != 0 && height != 0 && (width < 80 * 8 || height < 25 * 16)
}

/// Whether vtium draws the default set on a head whose displays have
/// `sizes`, each a width and height in pixels. The head counts as small
/// when its smallest width and its smallest height make a small display
/// ([`small_display`]), as the automatic size takes them, and there vtium
/// draws the built-in font's 8×8 strike instead. With no display known yet
/// the default set draws.
///
/// References:
/// - ADR-1204 D2: on a display where the small-display rule picks 8-pixel
///   rows, vtium draws the built-in font's 8×8 strike in place of
///   `terminus`, which has no strike under 12 pixels.
pub fn default_set_fits(sizes: impl IntoIterator<Item = (u32, u32)>) -> bool {
    let smallest = sizes
        .into_iter()
        .reduce(|(w, h), (w2, h2)| (w.min(w2), h.min(h2)));
    smallest.is_none_or(|(w, h)| !small_display(w, h))
}

/// The font set vtium draws when no font is named: bitmap Terminus at
/// every strike from 12 to 32 pixels, then GNU Unifont, which every vtium
/// build installs. It sits in the catalog's `console` folder.
///
/// References:
/// - ADR-1204 D2: `terminus` is the default set, after the font that a
///   head's look, `font-name` or vconsole names, and before the built-in
///   font.
pub const DEFAULT_SET: &str = "terminus";

/// The frits' directory vtium is built with, `font-frits`'s default, set
/// at build time with `VTIUM_FONT_FRITS`: the key table's, which
/// vtium-config reads too.
pub const FRITS: &str = vtium_settings::keys::FRITS;

/// The frits' directory `config` names, `font-frits`.
pub fn frits(config: &Config) -> PathBuf {
    PathBuf::from(config.text(&FONT_FRITS).unwrap_or(FRITS))
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

/// The category folders of the catalog of font sets, in the order vtium
/// searches them. vtium-config searches the same folders.
///
/// References:
/// - ADR-1204 D3 and D9: the six categories and their folders.
pub const CATEGORIES: [&str; 6] = ["console", "classic", "code", "nerd", "scripts", "small"];

/// The frit `name` names in `frits`: `NAME.vtfrit`, a regular file, in the
/// frits' directory itself, where a person's own sets live, else in the
/// first category folder that holds it. Only a frit's name is looked up,
/// so no name reaches outside the directory.
///
/// References:
/// - ADR-1204 D9: the directory itself first, then each category folder.
pub fn frit(frits: &Path, name: &str) -> Option<PathBuf> {
    if !frit_name(name) {
        return None;
    }
    let file = format!("{name}.vtfrit");
    std::iter::once(frits.join(&file))
        .chain(CATEGORIES.iter().map(|c| frits.join(c).join(&file)))
        .find(|path| std::fs::metadata(path).is_ok_and(|m| m.is_file()))
}

/// The smallest and largest side of a frit's `cell=` and a look's
/// `cell`, from the key table.
pub use crate::config::keys::{FRIT_CELL_MAX, FRIT_CELL_MIN};
/// The smallest and largest size a frit's `size=` names, in points, from
/// the key table, which vtium-config reads it within too.
use crate::config::keys::{FRIT_SIZE_MAX, FRIT_SIZE_MIN};

/// The ownership check a frit passes before vtium reads it, the check
/// vtium.conf passes (ADR-1401 D10, ADR-1403 D3): the file is root's, or
/// for a vtium that runs as another user, as the tests and a run by hand
/// do, that user's own, and every directory above it is root's or that
/// user's; neither the group nor other users can write the file or any
/// of those directories.
pub fn trust() -> Trust {
    Trust::running_as(rustix::process::geteuid().as_raw())
}

/// The frit at `path`'s text, once the ownership check `trust` makes
/// passes ([`trust`]): a regular file of at most `FILE_MAX` bytes, as
/// vtium-config's `FRIT_BYTES_MAX`, in UTF-8. The error is why vtium does
/// not read it, worded to follow "because" and ending in a full stop: for
/// a frit the check refuses, the path that fails and the command that
/// mends it, in the words vtium.conf's refusal uses
/// ([`vtium_settings::Untrusted::reason`]).
pub fn frit_text(path: &Path, trust: &Trust) -> Result<String, String> {
    trusted_text(path, trust)?.ok_or_else(|| "the file is missing.".to_owned())
}

/// The text of the file at `path`, read as [`frit_text`] reads a frit,
/// under the same ownership check; `None` when there is no such file. A
/// layer is read this way, so that a layer passes the check its frit
/// passes.
fn trusted_text(path: &Path, trust: &Trust) -> Result<Option<String>, String> {
    let inspected = match vtium_settings::inspect(path, trust) {
        Ok(Some(inspected)) => inspected,
        Ok(None) => return Ok(None),
        Err(rejected) => return Err(unread(&rejected)),
    };
    inspected.trusted.map_err(|why| why.reason().to_string())?;
    let bytes = inspected.bytes;
    if bytes.len() > crate::config::FILE_MAX {
        return Err(format!(
            "the file is longer than {} bytes.",
            crate::config::FILE_MAX
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "the file is not UTF-8 text.".to_owned())
}

/// The folder that holds the layers of every frit in the frits'
/// directory `frits`: its `layers/` folder, wherever in the directory the
/// frit itself is.
///
/// References:
/// - ADR-1204 D5 and D9: the layers live in the `layers/` folder of the
///   frits' directory.
pub fn layers_dir(frits: &Path) -> PathBuf {
    frits.join(layer::DIR)
}

/// The file of the layer `name` in the layers' folder `dir`.
fn layer_file(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}{}", layer::SUFFIX))
}

/// The frit `name`'s `text` with the sources of each layer it names in
/// place of the layer's `layer=` line, and a sentence for each `layer=`
/// line that adds nothing. vtium reads each layer from the layers' folder
/// of the frits' directory `frits`, under the ownership check `trust` that
/// the frit passed. It draws the set without a layer that is missing, that
/// the check refuses, whose structure is wrong, or that would take the set
/// past its 16 sources or 64 KiB. The glyph helper then reads one list of
/// sources, as it reads a frit with no layers, and it checks the layers'
/// values as it checks the frit's own.
///
/// References:
/// - ADR-1204 D5: the splice in place, and the ownership check a layer
///   passes.
pub fn with_layers(name: &str, frits: &Path, text: &str, trust: &Trust) -> (String, Vec<String>) {
    let dir = layers_dir(frits);
    let mut read = |layer: &str| trusted_text(&layer_file(&dir, layer), trust);
    let expanded = layer::expand(text, &mut read);
    let notes = expanded
        .notes
        .iter()
        .map(|note| layer_note(name, &dir, note))
        .collect();
    (expanded.text, notes)
}

/// The sentence that says why the `layer=` line `note` names, in the frit
/// `name` whose layers are in `dir`, adds nothing to the set. The start
/// and OSC 50 log it once a boot, and `vtium check-config` prints it.
pub fn layer_note(name: &str, dir: &Path, note: &layer::Note) -> String {
    let (n, layer) = (note.line, &note.layer);
    let file = || quoted(&layer_file(dir, layer));
    match &note.problem {
        layer::Problem::BadName => format!(
            "font set {name}: line {n}: layer={layer} does not name a layer. {} vtium draws the font set without it.",
            layer::NAME_RULE
        ),
        layer::Problem::Missing => format!(
            "font set {name}: line {n}: the layer {layer} is missing, because there is no file {}. vtium draws the font set without that layer.",
            file()
        ),
        layer::Problem::Unread(why) => format!(
            "font set {name}: line {n}: vtium does not read the layer {layer} from {}, because {why} vtium draws the font set without that layer.",
            file()
        ),
        layer::Problem::Invalid(e) => format!(
            "font set {name}: line {n}: the layer {layer} in {} has an error: {e}. vtium draws the font set without that layer.",
            file()
        ),
        layer::Problem::Twice(first) => format!(
            "font set {name}: line {n}: the layer {layer} is already on line {first}, so vtium leaves this line out."
        ),
        layer::Problem::RoleTaken { role, by, at } => format!(
            "font set {name}: line {n}: the layer {layer} is a second {role} layer, after {by} on line {at}. A font set holds one {role} layer, so vtium draws the font set without {layer}."
        ),
        layer::Problem::TooManySources(count) => format!(
            "font set {name}: line {n}: the layer {layer} would give the font set {count} sources, and a font set holds {} at most. vtium draws the font set without that layer.",
            layer::SET_SOURCES_MAX
        ),
        layer::Problem::TooLarge => format!(
            "font set {name}: line {n}: the layer {layer} would make the font set larger than {} bytes. vtium draws the font set without that layer.",
            layer::SET_BYTES_MAX
        ),
        layer::Problem::TooManyLayers => format!(
            "font set {name}: line {n}: the font set has more than {} layer= lines, so vtium does not read the layer {layer}.",
            layer::LAYERS_MAX
        ),
    }
}

/// The most sentences about layers that vtium logs in a boot.
const LAYER_NOTES_MAX: usize = 64;

/// Logs `note`, a sentence about a layer, once a boot: the start and each
/// OSC 50 switch to a frit read it again, and the log says it once.
pub fn log_layer_note(note: &str) {
    static LOGGED: std::sync::Mutex<std::collections::BTreeSet<String>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let mut logged = LOGGED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if logged.len() < LAYER_NOTES_MAX && logged.insert(note.to_owned()) {
        log::warn!("{note}");
    }
}

/// Why a frit that cannot be opened is not read, worded as [`frit_text`]
/// words it.
fn unread(rejected: &vtium_settings::Rejected) -> String {
    match rejected.errors.first().map(|e| &e.problem) {
        Some(vtium_settings::Problem::NotAFile) => "the file is not a regular file.".to_owned(),
        Some(vtium_settings::Problem::Unreadable(kind)) => {
            format!("reading it failed ({}).", std::io::Error::from(*kind))
        }
        _ => "the file cannot be read.".to_owned(),
    }
}

/// `path` in double quotes, escaped as vtium-settings quotes a path.
pub fn quoted(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}

/// The sentence that says vtium does not read the frit `name` at `path`,
/// and `reason`, why, as [`frit_text`] words it. The start logs it, OSC
/// 50's refusal quotes it and `vtium check-config` prints it.
pub fn refusal(name: &str, path: &Path, reason: &str) -> String {
    format!(
        "vtium does not read the font set {name} from {}, because {reason}",
        quoted(path)
    )
}

/// The value of the one `key=` line in a frit's `text`, with its line
/// number: read in the frit's line syntax, one `key=value` a line and `#`
/// to the line's end a comment, and nothing else of it, since the console
/// parses no font. `None` when there is none; an error naming the line
/// for a second.
fn frit_line<'a>(text: &'a str, key: &str) -> Result<Option<(usize, &'a str)>, String> {
    let mut found = None;
    // a frit with its layers in place names the frit's own lines
    let own = layer::lines(text);
    let lines = text.split('\n').zip(own.into_iter().map(|line| match line {
        layer::Line::Set(n) => n,
        layer::Line::Layer { at, .. } => at,
    }));
    for (line, n) in lines {
        let body = line.split('#').next().unwrap_or_default();
        let Some((k, value)) = body.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        if found.is_some() {
            return Err(format!("line {n}: {key}= appears twice"));
        }
        found = Some((n, value.trim()));
    }
    Ok(found)
}

/// A frit's `size=`, in points: digits only, no leading zero,
/// [`FRIT_SIZE_MIN`] to [`FRIT_SIZE_MAX`], as vtium-config reads it.
fn frit_points(s: &str) -> Option<u32> {
    s.parse::<u32>()
        .ok()
        .filter(|v| (FRIT_SIZE_MIN..=FRIT_SIZE_MAX).contains(v) && !s.starts_with(['0', '+']))
}

/// The cell a frit's `text` states, `cell=WIDTHxHEIGHT` (#145); `None`
/// when it states none; an error naming the line for a value
/// vtium-config refuses, or a second `cell=`.
pub fn frit_cell(text: &str) -> Result<Option<Cell>, String> {
    let Some((n, value)) = frit_line(text, "cell")? else {
        return Ok(None);
    };
    let parsed =
        crate::config::keys::parse_cell(value).map(|(width, height)| Cell { width, height });
    parsed.map(Some).ok_or_else(|| {
        format!(
            "line {n}: cell={value} is not WIDTHxHEIGHT in pixels, each from {FRIT_CELL_MIN} to {FRIT_CELL_MAX}"
        )
    })
}

/// The size a frit's `text` starts at, `size=` (ADR-1200 D1), in points;
/// `None` when it states none; an error naming the line for a value
/// vtium-config refuses, or a second `size=`.
pub fn frit_size(text: &str) -> Result<Option<u32>, String> {
    let Some((n, value)) = frit_line(text, "size")? else {
        return Ok(None);
    };
    frit_points(value).map(Some).ok_or_else(|| {
        format!(
            "line {n}: size={value} is not a size in points from {FRIT_SIZE_MIN} to {FRIT_SIZE_MAX}"
        )
    })
}

/// The line spacing a frit's `text` states, `line-spacing=` (ADR-1302
/// D13), a whole percentage of the text's height; `None` when it states
/// none; an error naming the line for a value vtium-config refuses, or a
/// second `line-spacing=`.
pub fn frit_line_spacing(text: &str) -> Result<Option<u32>, String> {
    let Some((n, value)) = frit_line(text, "line-spacing")? else {
        return Ok(None);
    };
    value
        .parse::<u32>()
        .ok()
        .filter(|&v| {
            v <= LINE_SPACING_MAX
                && !value.starts_with('+')
                && (value == "0" || !value.starts_with('0'))
        })
        .map(Some)
        .ok_or_else(|| {
            format!(
                "line {n}: line-spacing={value} is not a whole percentage from 0 to {LINE_SPACING_MAX}"
            )
        })
}

/// Points in an inch: a point is 1/72 inch.
const POINTS_PER_INCH: f64 = 72.0;

/// The pixel height of `points` on a display of `dpi` dots per inch, at
/// least 1 and at most [`CELL_SIDE_MAX`] (the operator's decision on
/// vtium/vtium#145).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is held inside 1..=CELL_SIDE_MAX, a u32, before the cast, and NaN casts to 0"
)]
pub fn points_to_pixels(points: u32, dpi: f64) -> u32 {
    // points × DPI ÷ 72, to the nearest pixel, a half away from zero
    let pixels = (f64::from(points) * dpi / POINTS_PER_INCH).round();
    (pixels.clamp(1.0, f64::from(CELL_SIDE_MAX)) as u32).max(1)
}

/// The whole points at or under `pixels` on a display of `dpi` dots per
/// inch, held to a u32.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is held inside 0..=u32::MAX before the cast, and NaN casts to 0"
)]
fn points_under(pixels: u32, dpi: f64) -> u32 {
    let points = (f64::from(pixels) * POINTS_PER_INCH / dpi).floor();
    points.clamp(0.0, f64::from(u32::MAX)) as u32
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

/// Whether a frit's `text` gives a bitmap font with strikes as its first
/// source: the first `source=` line names neither an outline file nor a
/// family, and a `strike=` line follows it before the next `source=` or
/// `layer=` line. Such a source is one font at several heights, so its
/// cell changes with the height it draws at.
///
/// A first source that is a built-in font (`builtin:`) or the system's
/// console font (`vconsole` or `console:`) takes no strikes. The font
/// helper ignores a `strike=` line under such a source, so the console
/// does not count one either, and both read the same set.
///
/// References:
/// - ADR-1204 D2: the `strike=` key, and the strike each height draws.
/// - ADR-1510 D2: the helper ignores a key where it does not apply, and reads the rest of the set.
fn frit_struck(text: &str) -> bool {
    let mut keys = text.split('\n').filter_map(|line| {
        let (k, v) = line.split('#').next()?.split_once('=')?;
        Some((k.trim(), v.trim()))
    });
    let Some((_, first)) = keys.by_ref().find(|(k, _)| *k == "source") else {
        return false;
    };
    let unstruck = ["outline:", "font:", "builtin:", "console:"];
    !(unstruck.iter().any(|kind| first.starts_with(kind)) || first == "vconsole")
        && keys
            .take_while(|(k, _)| *k != "source" && *k != "layer")
            .any(|(k, _)| k == "strike")
}

/// What `vtium check-config` says of the cell the frit `name`'s `text`
/// states, where that needs no font parsed (#145): an unreadable
/// `cell=`, a cell smaller than the built-in Unscii-8's 8x8, which the
/// built-in is cropped to until the glyphs land, and a `builtin:` source
/// whose own cell does not fit it, which gives nothing. Whether a font
/// file's glyphs fit is vtium-config's to check.
pub fn frit_warnings(name: &str, text: &str) -> Vec<String> {
    if let Err(why) = frit_line_spacing(text) {
        return vec![format!(
            "font set {name}: {why}, so vtium uses its built-in font"
        )];
    }
    let cell = match frit_cell(text) {
        Ok(Some(cell)) => cell,
        Ok(None) => return Vec::new(),
        Err(why) => {
            return vec![format!(
                "font set {name}: {why}, so vtium uses its built-in font"
            )];
        }
    };
    let mut out = Vec::new();
    if cell.width < 8 || cell.height < 8 {
        out.push(format!(
            "font set {name}: its {}x{} character cell is smaller than the built-in font's 8x8, so the built-in font is cut to fit until the font set's characters are ready",
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
                "font set {name}: line {n}: the characters of source={} ({}x{}) do not fit the {}x{} character cell, so vtium does not use that source",
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

/// The height of the largest whole multiple of `cell` no taller than
/// `height`, and at least `cell`'s own height: the height a bitmap font of
/// that cell draws at when `height` is asked for.
pub fn whole_height(cell: Cell, height: u32) -> u32 {
    frit_cell_at(Some(cell), height, cell).height
}

/// A face a display draws with: [`Send`], as its head's render thread
/// draws it (ADR-1401 D2).
pub type DisplayFace = Box<dyn Face + Send>;

/// The built-in strike that draws in `cell` (#145, ADR-1200 D7), and the
/// height it draws at. With no `name`, it is unifont at the largest whole
/// factor whose cell fits in `cell`; where even unifont's 8x16 does not
/// fit, it is the smaller strike, Unscii-8 at 8x8, but unifont in a zh, ja
/// or ko locale. The built-in that `name` names keeps its own strike, at
/// the largest whole factor whose cell fits, at least once. The face cuts
/// a strike that does not fit ([`DeferredFace`]).
fn builtin_strike(name: Option<&'static str>, cell: Cell, locale: &str) -> (&'static str, u32) {
    let factor = |w: u32, h: u32| {
        let across = cell.width.checked_div(w).unwrap_or(0);
        across.min(cell.height.checked_div(h).unwrap_or(0))
    };
    let height = match name {
        Some("unscii-8") => factor(8, 8).max(1).saturating_mul(8),
        Some(_) => factor(8, 16).max(1).saturating_mul(16),
        None => match factor(8, 16) {
            0 => 8,
            k => k.saturating_mul(16),
        },
    };
    (name.unwrap_or_else(|| builtin_name(height, locale)), height)
}

/// Whether a built-in glyph `drawn` pixels wide, in a cluster of
/// `cells` cells of `cell`, is squeezed to fit them, the last step of the
/// fallback (ADR-1302 D14): when it is exactly twice their width, or
/// wider than them and 2 pixels more.
pub fn squeezed(drawn: u32, cell: Cell, cells: u32) -> bool {
    let w = cell.width.saturating_mul(cells);
    drawn == w.saturating_mul(2) || drawn > w.saturating_add(2)
}

/// Whether the built-in fonts, drawing in `cell` under `locale`, squeeze
/// `c` to fit its cells ([`squeezed`]); false where they lack it.
pub fn builtin_squeezes(c: char, cell: Cell, locale: &str) -> bool {
    let Some(face) = builtin_in(None, cell, locale) else {
        return false;
    };
    let cells = u32::from(vtium_term::cluster_width(&[c]).max(1));
    face.has_glyph(c)
        && face
            .draw(&[c], vtium_font::Style::default())
            .is_some_and(|g| squeezed(g.width(), cell, cells))
}

/// The built-in face that draws in `cell`: the strike
/// [`builtin_strike`] chooses, at its height.
fn builtin_in(name: Option<&'static str>, cell: Cell, locale: &str) -> Option<DisplayFace> {
    let (strike, height) = builtin_strike(name, cell, locale);
    Chosen::Builtin {
        name: Some(strike),
        cell: None,
    }
    .face(height, locale)
}

#[cfg(test)]
thread_local! {
    /// How many times this thread has called [`own_face`], so that a test
    /// can check which frames build the face.
    pub static OWN_FACES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The face vtium draws its own messages in for a grid of `cell`: the
/// notices, the on-screen display's line and dot, and the panic screen.
/// It is the built-in font, "vtium built-in", whose strike the cell's
/// height chooses at the largest whole multiple that fits, centred in the
/// cell where the two differ. It draws with no font file and no font
/// helper, whatever font the head draws its terminal in.
///
/// References:
/// - ADR-1204 D1: vtium's own messages always draw in the built-in font,
///   and which strike draws which cell.
pub fn own_face(cell: Cell) -> Option<DisplayFace> {
    #[cfg(test)]
    OWN_FACES.set(OWN_FACES.get().saturating_add(1));
    let face = vtium_font::set::own::face(cell)?;
    if face.cell() == cell {
        return Some(Box::new(face));
    }
    Some(Box::new(DeferredFace::new(
        Box::new(face),
        cell,
        cell.height,
        Shared::default(),
    )))
}

/// Where vtium finds frits: the frits' directory, the helper that
/// generates a frit's glyphs, and the files that name the font when the
/// configuration does not.
#[derive(Clone, Debug)]
pub struct Sources {
    pub frits: PathBuf,
    /// The ownership check each frit passes before vtium reads it,
    /// [`trust`] but in a test whose frits sit under a directory others
    /// can write, such as `/tmp`.
    pub trust: Trust,
    /// The helper; `None` for the one `font-helper` names
    /// ([`helper::Launcher::configured`]).
    pub helper: Option<helper::Command>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
}

impl Sources {
    /// The system's, as `config` names them: the frits' directory,
    /// `font-frits`; the helper `font-helper` names; `/etc/vconsole.conf`
    /// and `/proc/cmdline`.
    pub fn configured(config: &Config) -> Sources {
        Sources {
            frits: frits(config),
            ..Sources::default()
        }
    }
}

impl Default for Sources {
    /// The system's with every key at its default: the frits' directory
    /// vtium is built with, read under [`trust`], the helper `font-helper` names,
    /// `/etc/vconsole.conf` and `/proc/cmdline`.
    fn default() -> Sources {
        Sources {
            frits: PathBuf::from(FRITS),
            trust: trust(),
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
    /// No font is named, so the name is the default set's,
    /// [`DEFAULT_SET`]. vtium draws it only where a set of that name is
    /// installed, and never makes a set on the fly for it.
    Default,
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
    /// The default set's name never reaches this, because [`frit_named`]
    /// refuses it when no set bears it; it reads as a kbd name.
    fn source(&self) -> String {
        match self.came {
            Came::Vconsole => "vconsole".to_owned(),
            Came::Family => format!("font:{}", self.name),
            Came::Config if self.name.starts_with('/') => format!("file:{}", self.name),
            Came::Config | Came::Default => format!("kbd:{}", self.name),
        }
    }
}

/// `path`'s text, read as a configuration file is, a regular file of at
/// most `FILE_MAX` bytes; empty when there is none.
pub fn read_text(path: &Path) -> String {
    crate::config::read_bytes(path)
        .ok()
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// What the console reads its resources through for this login: the gate
/// and the folder of the palettes the package ships.
struct Held {
    gate: std::sync::Arc<cullet::Gate>,
    palettes: PathBuf,
}

thread_local! {
    /// The console's gate for this login: the batch, the trust rule and
    /// the mount table that every resource the console reads passes. The
    /// console reads the batch and the mount table when it starts and again
    /// at each new login ([`load_batch`]), and every reader takes the gate from here: the
    /// font helpers, the color schemes, the color maps and the pointer
    /// files. The console reads its resources on its main loop's thread, so
    /// the gate is that thread's; each test, which runs on a thread of its
    /// own, holds a gate of its own.
    static HELD: std::cell::RefCell<Option<Held>> = const { std::cell::RefCell::new(None) };
}

/// What [`HELD`] holds, read the first time from this machine's places,
/// as a start reads them.
fn held<T>(f: impl Fn(&Held) -> T) -> T {
    if let Some(t) = HELD.with_borrow(|h| h.as_ref().map(&f)) {
        return t;
    }
    let _ = start_batch(&batch::Places::system(), &trust(), Rule::new(trust()));
    HELD.with_borrow(|h| h.as_ref().map(&f)).unwrap_or_else(|| {
        // `start_batch` always leaves something held; this stands in only
        // to keep the function free of a panic
        f(&Held {
            gate: std::sync::Arc::new(cullet::Gate::new(
                Rule::new(trust()),
                None,
                batch::Batch::default(),
            )),
            palettes: PathBuf::from(vtium_settings::palette::PACKAGE_PALETTES),
        })
    })
}

/// Reads the batch the way a start does, at `places` under `trust` with
/// `rule` for each resource, unless the console already holds one; what
/// vtium found reading the batch file, for the caller to log, or `None`
/// when the console already held a batch.
pub fn start_batch(places: &batch::Places, trust: &Trust, rule: Rule) -> Option<Found> {
    if HELD.with_borrow(Option::is_some) {
        return None;
    }
    load_batch(places, trust, rule)
}

/// The console's gate for this login. The first call before any
/// [`load_batch`] reads this machine's batch, as a start does.
///
/// References:
/// - ADR-1205 D4: one gate for every kind of resource, and one batch for each login.
pub fn gate() -> std::sync::Arc<cullet::Gate> {
    held(|h| std::sync::Arc::clone(&h.gate))
}

/// The folder of the palettes the package ships, as the places the batch
/// was last read from name it: [`vtium_settings::palette::PACKAGE_PALETTES`],
/// or a test's.
pub fn package_palettes() -> PathBuf {
    held(|h| h.palettes.clone())
}

/// Reads the batch at `places` under `trust`, the check vtium.conf passes,
/// and this machine's mount table, and makes them the console's gate, with
/// `rule`, the trust rule each resource passes. vtium calls this at the
/// start and at each new login. When the batch differs from the one the
/// console held, which it does at the first read, the result is what vtium
/// found reading the batch file, for the caller to log. The result is
/// `None` when the batch is the same, so a login that changes nothing logs
/// nothing.
///
/// References:
/// - ADR-1205 D4, its note of 2026-11-19: the console reads the batch again at each new login, so an activation takes effect at the next login.
pub fn load_batch(places: &batch::Places, trust: &Trust, rule: Rule) -> Option<Found> {
    load_batch_with(places, trust, rule, Mounts::system().unwrap_or_default())
}

/// [`load_batch`], judging filesystems by `mounts`. The gate takes the
/// mount table of every login, even one whose batch is the same, because
/// the kernel can give a mount ID to a new mount once the old mount is
/// gone. A gate that kept an older table would then judge the new
/// filesystem by the old mount's line.
fn load_batch_with(
    places: &batch::Places,
    trust: &Trust,
    rule: Rule,
    mounts: Mounts,
) -> Option<Found> {
    let read = batch::Batch::system(places, trust);
    let (same, kept) = HELD.with_borrow(|held| {
        let Some(h) = held else {
            return (false, false);
        };
        let same = h.gate.batch == read.batch && h.palettes == places.palettes;
        (same, same && h.gate.mounts.as_ref() == Some(&mounts))
    });
    if kept {
        return None;
    }
    let more = read.more_line();
    HELD.set(Some(Held {
        gate: std::sync::Arc::new(cullet::Gate::new(rule, Some(mounts), read.batch)),
        palettes: places.palettes.clone(),
    }));
    (!same).then_some(Found {
        findings: read.findings,
        more,
    })
}

/// What vtium found reading the batch file, for the caller to log.
#[derive(Debug)]
pub struct Found {
    /// The findings, in the file's order, at most the first
    /// [`vtium_settings::ERRORS_MAX`] lines vtium ignores.
    pub findings: Vec<batch::Finding>,
    /// The line that counts the ignored lines past those listed, when
    /// there are more ([`batch::Read::more_line`]).
    pub more: Option<String>,
}

impl Found {
    /// Logs the findings: a missing file as information, since a machine
    /// upgraded from an earlier vtium has none, and a file vtium cannot
    /// use, or a line it ignores, as a warning. The count of the ignored
    /// lines past those listed comes last, as a warning too.
    pub fn log(&self) {
        for finding in &self.findings {
            match finding {
                batch::Finding::Missing { .. } => log::info!(target: "batch", "{finding}"),
                _ => log::warn!(target: "batch", "{finding}"),
            }
        }
        if let Some(more) = &self.more {
            log::warn!(target: "batch", "{more}");
        }
    }
}

/// The font the configuration asks for (ADR-1200 D9): `font-name`, set in
/// a file or on the command line; unset, vconsole's font; else the
/// default set, [`DEFAULT_SET`]. When `font-engine` names the built-in
/// unifont or the 8x16 font, none, and vtium draws its built-in font.
///
/// References:
/// - ADR-1204 D2: the default set comes after vconsole's font.
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
        _ => Some(batch::vconsole(vconsole_conf, cmdline).font.map_or_else(
            || Wanted {
                name: DEFAULT_SET.to_owned(),
                came: Came::Default,
            },
            |name| Wanted {
                name,
                came: Came::Vconsole,
            },
        )),
    }
}

/// The frit `wanted` names (ADR-1200 D9): the frit of that name in the
/// frits' directory, read under the sources' ownership check; else a kbd name, a path, an outline family or vconsole's font
/// as a one-source frit made on the fly, named as it was named. The error
/// is why the frit of that name cannot be read, or, for the default set,
/// that no set of its name is installed: vtium then draws its built-in
/// font.
pub fn frit_named(sources: &Sources, wanted: &Wanted) -> Result<FritText, String> {
    if let Some(path) = frit(&sources.frits, &wanted.name) {
        return FritText::read(&path, &sources.frits, &sources.trust);
    }
    if wanted.came == Came::Default {
        return Err(format!(
            "No font is named, and the default font set {} is not installed.",
            wanted.name
        ));
    }
    Ok(wanted.on_the_fly())
}

impl Wanted {
    /// The one-source frit vtium makes on the fly for this name, named as
    /// it was named (ADR-1200 D9).
    fn on_the_fly(&self) -> FritText {
        FritText {
            name: self.name.clone(),
            text: format!("source={}\n", self.source()),
        }
    }
}

/// The built-in font `name` names, as its own name spells it, in `look`,
/// the cell the head's look states, where it states one (ADR-1403 D5).
pub fn builtin_named(name: &str, look: Option<Cell>) -> Option<Chosen> {
    ["unifont", "unscii-8"]
        .into_iter()
        .find(|b| *b == name)
        .map(|b| Chosen::Builtin {
            name: Some(b),
            cell: look,
        })
}

/// The font named `name` for OSC 50 (ADR-1200 D5): a built-in; else a
/// frit in the frits' directory; else `made`, the frit vtium made on the
/// fly for the font the configuration gives the head (D9), when `name` is
/// the name it was made under. The order is D9's, so a name means the same
/// font at a switch as at the start: a frit in the directory wins over a
/// frit made on the fly under its name. A name that holds `/` is a font
/// file's path. When the file qualifies, which [`qualifying`] checks with
/// `folders` and `gate`, vtium makes a one-source frit for it, named by
/// the path, as the start does for a path that `font-name` gives. Each
/// draws in `look`, the cell the head's look states, and a frit with its
/// own cell unless `sized`, as [`Chosen::frit`] has it, so the name OSC
/// 50's query reports for the font a head starts in switches back to that
/// font at the size it had. The font keeps the name as the built-in, the
/// frit's file or the configuration spells it, or the path as the request
/// gave it once it qualifies.
pub fn by_name(
    sources: &Sources,
    name: &str,
    sized: bool,
    look: Option<Cell>,
    made: Option<&FritText>,
    folders: &[PathBuf],
    gate: &cullet::Gate,
) -> Result<Chosen, String> {
    if let Some(builtin) = builtin_named(name, look) {
        return Ok(builtin);
    }
    let frit = match frit(&sources.frits, name) {
        Some(path) => FritText::read(&path, &sources.frits, &sources.trust)?,
        None => match made.filter(|made| made.name == name) {
            Some(made) => made.clone(),
            None if name.contains('/') => {
                qualifying(name, folders, gate)?;
                Wanted {
                    name: name.to_owned(),
                    came: Came::Config,
                }
                .on_the_fly()
            }
            None => return Err("there is no font set of that name".to_owned()),
        },
    };
    Chosen::frit(frit, sized, look)
}

/// The folders where installed outline fonts are found when `font-paths`
/// is unset, as fontconfig's default configuration names them for the
/// system. vtium-config's font helper searches the same list.
///
/// References:
/// - ADR-1200 D25: `font-paths` replaces this list.
pub const FONT_DIRS: [&str; 2] = ["/usr/share/fonts", "/usr/local/share/fonts"];

/// The folders a font that OSC 50 names by its path must sit in, or below.
/// They are the folders of `font-paths`, else [`FONT_DIRS`], then the
/// package's font folder and kbd's console-font folders. Root installs
/// fonts in each of them, and a fresh batch already activates the fonts in
/// the last two. The configuration has already left out each folder of
/// `font-paths` that is not an absolute path.
///
/// References:
/// - ADR-1200 D5, "A qualifying font": the second check and its folders.
pub fn font_folders(config: &Config) -> Vec<PathBuf> {
    let configured: Vec<PathBuf> = match config.text(&FONT_PATHS) {
        Some(paths) => paths.split(':').map(PathBuf::from).collect(),
        None => FONT_DIRS.iter().map(PathBuf::from).collect(),
    };
    configured
        .into_iter()
        .chain(std::iter::once(PathBuf::from(batch::PACKAGE_FONTS)))
        .chain(batch::CONSOLE_FONT_FOLDERS.iter().map(PathBuf::from))
        .collect()
}

/// Checks the folder `dir` under the trust rule `rule`, judging its
/// filesystem by `mounts`, or by the system's mount table when `None`.
/// The folder passes when its owner and every folder above it are trusted
/// by `rule`, neither the group nor other users can write any of them,
/// none of them is a symbolic link, and the filesystem stays mounted.
/// vtium opens each folder on the way, and nothing inside `dir`.
///
/// vtium-settings checks a folder on its own only through the gate of a
/// pointer theme, which the batch must list first. So a gate whose batch
/// lists only this folder judges the folder under the trust rule.
/// `vtium check-config` judges a folder of `font-paths` with this check,
/// as root, and OSC 50 judges the folder of a font it is given by path, so
/// the two never disagree on a folder.
///
/// References:
/// - ADR-1205 D4: the trust rule on a resource and every folder above it.
/// - ADR-1200 D5, "A qualifying font": the fourth check.
pub fn trusted_folder(
    dir: &Path,
    rule: Rule,
    mounts: Option<Mounts>,
) -> Result<(), cullet::Refused> {
    let listed = batch::Batch::new(vec![batch::Entry::Path(dir.to_path_buf())]);
    cullet::Gate::new(rule, mounts, listed).theme(dir)
}

/// Whether `path`, the text OSC 50 carries, names a qualifying font: one
/// that root already installed and trusts. vtium makes four checks, in
/// this order, and the error says which one failed:
///
/// 1. The path is plain: it starts with `/`, has no `..` component and
///    holds no control character.
/// 2. The file sits in one of `folders` ([`font_folders`]), or in a folder
///    below one. vtium compares whole components, so
///    `/usr/share/fonts-old/x.ttf` is not under `/usr/share/fonts`.
/// 3. The batch that `gate` holds lists the file, so the font is active.
/// 4. The folder that holds the file passes the trust rule of `gate`
///    ([`trusted_folder`]).
///
/// The checks read the path's text, the batch and the folders, and open
/// no font file. The font helper opens the file later through its own
/// gate, which checks the batch and the trust rule again on the file it
/// opened.
///
/// References:
/// - ADR-1200 D5, "A qualifying font": the four checks and their order.
/// - ADR-1205 D4: the trust rule and the batch.
pub fn qualifying(path: &str, folders: &[PathBuf], gate: &cullet::Gate) -> Result<(), String> {
    let file = Path::new(path);
    let plain = path.starts_with('/')
        && !file
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        && !path.chars().any(char::is_control);
    if !plain {
        return Err(
            "a font's path must start with `/`, and hold no `..` and no control character"
                .to_owned(),
        );
    }
    let searched = folders
        .iter()
        .any(|dir| file != dir && file.starts_with(dir));
    if !searched {
        return Err(
            "the file is not in a folder that vtium searches for fonts: those of font-paths, the package's font folder and kbd's console-font folders".to_owned(),
        );
    }
    if !gate.holds(file) {
        return Err(format!(
            "the font is not active, because {} does not list it",
            batch::BATCH_FILE
        ));
    }
    let dir = file.parent().unwrap_or(file);
    trusted_folder(dir, gate.rule.clone(), gate.mounts.clone()).map_err(|refused| {
        format!(
            "the folder {} fails the trust rule, because {}",
            dir.display(),
            refused.check
        )
    })
}

/// The font vtium draws.
#[derive(Clone, Debug)]
pub enum Chosen {
    /// The built-in sets (ADR-1200 D7): the one `name` names, or with
    /// `None`, Unscii-8 for cells under 16 pixels and unifont from 16, but
    /// unifont alone in a zh, ja or ko locale. With a `cell`, the cell the
    /// head's look states (ADR-1403 D5), they draw in that cell, fitted to
    /// it as they draw in a frit's cell, and a zoom steps by its whole
    /// multiples (D17); with none, in their own cell at the height drawn.
    Builtin {
        name: Option<&'static str>,
        cell: Option<Cell>,
    },
    /// A frit, generated at run time (ADR-1301 D2), in the cell it
    /// states, else the display's (#145): drawn in the built-in font, each
    /// character the helper has sent drawn from `generated` in its place.
    Frit {
        /// The name the font goes by, as it was given.
        name: String,
        /// The frit as the helper is handed it, under a frit's name.
        frit: FritText,
        /// The look's `cell`, else the frit's `cell=` unless `font-size`
        /// is configured.
        cell: Option<Cell>,
        /// `size=`, unless `font-size` is configured or the look states a
        /// cell.
        size: Option<u32>,
        /// Its bitmap first source's own cell, as the helper tells it for a
        /// frit that states no cell, with no `font-size` configured.
        own: Option<Cell>,
        /// Its outline first source's cell at each height the helper was
        /// asked at, for a frit that states no cell; for a bitmap first
        /// source with strikes, the cell of the strike each height draws.
        heights: std::collections::BTreeMap<u32, Cell>,
        /// Its first source is an outline font, so a zoom steps by 1
        /// pixel: guessed from its text until the helper's answer says.
        outline: bool,
        /// Its first source is a bitmap font with strikes, read from its
        /// text, so with no cell stated it asks the helper for its cell at
        /// each height, as an outline frit does, and keeps each answer in
        /// `heights` by the height asked at.
        struck: bool,
        /// A `font-size` is configured, which sets the cell's height of a
        /// frit with strikes ([`Chosen::face_spaced`]).
        sized: bool,
        /// `line-spacing=`, a whole percentage of the text's height
        /// (ADR-1302 D13), which vtium.conf's `font-line-spacing`
        /// overrides.
        line_spacing: Option<u32>,
        /// Its `fit=` lines, which place the built-in glyphs drawn in its
        /// cell (ADR-1302 D13).
        fits: std::sync::Arc<crate::deferred::FritFits>,
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
    /// The built-in sets in their own cell, Unscii-8 or unifont by height.
    pub const BUILTINS: Chosen = Chosen::Builtin {
        name: None,
        cell: None,
    };

    /// The built-in sets, Unscii-8 or unifont by height, in `look`, the
    /// cell the head's look states, where it states one (ADR-1403 D5).
    pub fn builtins(look: Option<Cell>) -> Chosen {
        Chosen::Builtin {
            name: None,
            cell: look,
        }
    }

    /// The frit `frit`, its cell and size read from its text before the
    /// first frame (#145), unless `sized`, a configured `font-size`, wins
    /// over them (ADR-1200 D16, D17), or `look`, the cell the head's look
    /// states, which is more specific than the frit and wins over its
    /// `cell=` and `size=` alike (ADR-1403 D5). Its `size=` counts for an
    /// outline frit only: points do not apply to bitmap fonts (the
    /// operator's decision on vtium/vtium#145). A name that is not a
    /// frit's goes to the helper as [`GENERATED`]. The error is why they
    /// cannot be read.
    pub fn frit(mut frit: FritText, sized: bool, look: Option<Cell>) -> Result<Chosen, String> {
        let cell = frit_cell(&frit.text)?;
        let size = frit_size(&frit.text)?;
        let line_spacing = frit_line_spacing(&frit.text)?;
        let fits = std::sync::Arc::new(crate::deferred::FritFits::of(&frit.text));
        let outline = frit_outline(&frit.text);
        let struck = frit_struck(&frit.text);
        let name = frit.name.clone();
        if !frit_name(&name) {
            GENERATED.clone_into(&mut frit.name);
        }
        Ok(Chosen::Frit {
            name,
            outline,
            frit,
            cell: look.or(cell.filter(|_| !sized)),
            size: size.filter(|_| !sized && outline && look.is_none()),
            own: None,
            heights: std::collections::BTreeMap::new(),
            struck,
            sized,
            line_spacing,
            fits,
            generated: Shared::default(),
        })
    }

    /// The cell the font draws in, whatever the height asked: the look's
    /// `cell`, else a frit's `cell=` (ADR-1200 D17, ADR-1403 D5).
    fn stated(&self) -> Option<Cell> {
        match self {
            Chosen::Frit { cell, .. } | Chosen::Builtin { cell, .. } => *cell,
        }
    }

    /// Whether the font is a frit that asks the helper for its first
    /// source's cell at `height`: one that states no cell, and either an
    /// outline frit or a bitmap frit with strikes with no answer at
    /// `height` yet, whether or not a `font-size` is configured, or another
    /// bitmap frit with no answer yet and no `font-size` configured, which
    /// `sized` says. Each height of a font with strikes can draw another
    /// strike, so its cell depends on the height, as an outline font's
    /// does.
    ///
    /// References:
    /// - ADR-1200 D17: an outline frit asks at each height (the maintainer,
    ///   2026-11-01), and a bitmap frit at its own size (the operator,
    ///   2026-10-28).
    /// - ADR-1204 D2: the strike for the automatic height, for a configured
    ///   font size and for each zoom step.
    pub fn wants_own_cell(&self, height: u32, sized: bool) -> bool {
        match self {
            Chosen::Frit {
                cell: None,
                outline: true,
                heights,
                ..
            }
            | Chosen::Frit {
                cell: None,
                struck: true,
                heights,
                ..
            } => !heights.contains_key(&height),
            Chosen::Frit {
                cell: None,
                own: None,
                outline: false,
                ..
            } => !sized,
            _ => false,
        }
    }

    /// Takes `answer`, the cell the helper told for the frit's first
    /// source, which also says whether that source is a bitmap or an
    /// outline font, whatever the frit's text suggested, as the answer at
    /// the cell's own height ([`Chosen::set_own_cell_at`]).
    pub fn set_own_cell(&mut self, answer: OwnCell, sized: bool) {
        let height = match answer {
            OwnCell::Bitmap(cell) | OwnCell::Outline(cell) => cell.height,
        };
        self.set_own_cell_at(answer, height, sized);
    }

    /// Takes `answer`, the cell the helper told for the frit's first
    /// source when asked at `height`, which also says whether that source
    /// is a bitmap or an outline font, whatever the frit's text suggested.
    /// A bitmap source's own cell counts only where no `font-size` is
    /// configured, as `sized` says. A bitmap source with strikes answers
    /// with the cell of the tallest strike no taller than `height`, which
    /// then holds at every height from the strike's own up to `height`.
    pub fn set_own_cell_at(&mut self, answer: OwnCell, height: u32, sized: bool) {
        if let Chosen::Frit {
            cell: stated,
            own,
            heights,
            outline,
            struck,
            ..
        } = self
        {
            match answer {
                OwnCell::Bitmap(cell) if *struck && stated.is_none() => {
                    *outline = false;
                    for at in [height, whole_height(cell, height), cell.height] {
                        heights.insert(at, cell);
                    }
                }
                OwnCell::Bitmap(cell) => {
                    *outline = false;
                    if !sized {
                        *own = Some(cell);
                    }
                }
                OwnCell::Outline(cell) => {
                    *outline = true;
                    heights.insert(cell.height, cell);
                }
            }
        }
    }

    /// The cell a bitmap frit's heights, and the built-ins' in a look's
    /// cell, are whole multiples of, and the cell an outline frit states:
    /// the one stated, else a bitmap first source's own.
    fn base_cell(&self) -> Option<Cell> {
        match self {
            Chosen::Frit { own, .. } => self.stated().or(*own),
            Chosen::Builtin { .. } => self.stated(),
        }
    }

    /// Whether the font is a frit a zoom steps by 1 pixel, whose first
    /// source is an outline font.
    pub fn outline(&self) -> bool {
        matches!(self, Chosen::Frit { outline: true, .. })
    }

    /// Whether the font is a frit that states no cell and whose first
    /// source is a bitmap font with strikes, so each height it draws at
    /// takes the cell of the strike the helper names for that height.
    pub fn struck(&self) -> bool {
        matches!(
            self,
            Chosen::Frit {
                cell: None,
                outline: false,
                struck: true,
                ..
            }
        )
    }

    /// The cell of the strike that a frit with strikes draws at `height`,
    /// as the helper answered for that height. `None` before the answer,
    /// and for any other font.
    pub fn strike_at(&self, height: u32) -> Option<Cell> {
        match self {
            Chosen::Frit { heights, .. } if self.struck() => heights.get(&height).copied(),
            _ => None,
        }
    }

    /// The built-in font drawn at `height` in `locale`: in a look's cell,
    /// the strike that [`builtin_strike`] chooses for the cell at that
    /// height, which is what the face draws.
    fn builtin_at(&self, height: u32, locale: &str) -> &'static str {
        match self {
            Chosen::Builtin {
                name,
                cell: Some(look),
            } => builtin_strike(*name, frit_cell_at(Some(*look), height, *look), locale).0,
            Chosen::Builtin {
                name: Some(name),
                cell: None,
            } => name,
            Chosen::Builtin {
                name: None,
                cell: None,
            }
            | Chosen::Frit { .. } => builtin_name(height, locale),
        }
    }

    /// The font's name at `height` in `locale`, as OSC 50 spells it: the
    /// frit's, else the built-in's.
    pub fn name(&self, height: u32, locale: &str) -> &str {
        match self {
            Chosen::Frit { name, .. } => name,
            Chosen::Builtin { .. } => self.builtin_at(height, locale),
        }
    }

    /// The face at `height`: a frit's, in its cell at that height, drawn
    /// in the built-in font until the helper's glyphs land; the built-ins
    /// in a look's cell, fitted to that cell's whole multiple at that
    /// height; else the built-in's strike of that height, else the tallest
    /// under it scaled by the largest whole factor that fits. The built-in
    /// unifont draws through its own face. A frit's own `line-spacing=`
    /// makes the cell taller where it applies ([`Chosen::face_spaced`]).
    pub fn face(&self, height: u32, locale: &str) -> Option<DisplayFace> {
        self.face_spaced(height, locale, None)
    }

    /// The rows line spacing adds to a cell whose text is `text` pixels
    /// high (ADR-1302 D13): `spacing`, vtium.conf's `font-line-spacing`,
    /// else a frit's `line-spacing=`, else none. A cell the look or the
    /// frit states, `cell` or `cell=`, is exact and takes none; the cell
    /// vtium works out takes it: a frit's fonts' own cell, the automatic
    /// rule's, and the cell a configured `font-size` gives in place of a
    /// frit's `cell=`.
    fn band(&self, text: u32, spacing: Option<u32>) -> Band {
        if self.stated().is_some() {
            return Band::NONE;
        }
        let own = match self {
            Chosen::Frit { line_spacing, .. } => *line_spacing,
            Chosen::Builtin { .. } => None,
        };
        Band::of(text, spacing.or(own).unwrap_or(0))
    }

    /// [`Chosen::face`], with `spacing`, vtium.conf's
    /// `font-line-spacing`, over a frit's own `line-spacing=`: the cell is
    /// taller by the rows `Chosen::band` gives, split above and below
    /// the text band, and each glyph is placed in it (ADR-1302 D13). The
    /// text band is the cell [`Chosen::face`] would draw in, so `height`
    /// stays the text's height.
    pub fn face_spaced(
        &self,
        height: u32,
        locale: &str,
        spacing: Option<u32>,
    ) -> Option<DisplayFace> {
        if let Chosen::Builtin {
            name,
            cell: Some(look),
        } = self
        {
            // D7: the built-ins fitted to the look's cell, as to a frit's
            // stated cell, and to its whole multiples at a zoom
            let cell = frit_cell_at(Some(*look), height, *look);
            return Some(Box::new(DeferredFace::new(
                builtin_in(*name, cell, locale)?,
                cell,
                look.height,
                Shared::default(),
            )));
        }
        if let Chosen::Frit {
            generated,
            heights,
            fits,
            sized,
            ..
        } = self
        {
            let at = Chosen::BUILTINS.face(height, locale)?;
            let stated = self.base_cell();
            let (cell, step) = match (stated, self.outline()) {
                // A bitmap source with strikes: the cell of the strike the
                // helper names for this height, or its whole multiple
                // above the tallest strike. A configured font size keeps
                // its own height, as wide as that strike's whole multiple,
                // and the helper centres the strike in it.
                // References: ADR-1204 D2 decides the strike each height
                // draws.
                (None, false) if self.struck() => match heights.get(&height) {
                    Some(c) => {
                        let whole = frit_cell_at(Some(*c), height, *c);
                        if *sized {
                            (
                                Cell {
                                    width: whole.width,
                                    height: height.max(whole.height),
                                },
                                1,
                            )
                        } else {
                            (whole, c.height)
                        }
                    }
                    None => (frit_cell_at(None, height, at.cell()), at.size_step()),
                },
                // D17, an outline frit steps by 1: the stated cell at its
                // own height, else the helper's answer
                // at this height, else the built-in font's width
                (Some(c), true) if c.height == height => (c, 1),
                (_, true) => match heights.get(&height) {
                    Some(c) => (*c, 1),
                    None => (frit_cell_at(None, height, at.cell()), 1),
                },
                // D17, a bitmap frit steps by the font's height: the
                // stated or own cell's whole multiples
                (Some(c), false) => (frit_cell_at(Some(c), height, at.cell()), c.height),
                // D17: with no cell stated, the display's
                (None, false) => (frit_cell_at(None, height, at.cell()), at.size_step()),
            };
            let builtin = builtin_in(None, cell, locale)?;
            let band = self.band(cell.height, spacing);
            return Some(Box::new(
                DeferredFace::spaced(builtin, cell, band, step, std::sync::Arc::clone(generated))
                    .with_fits(std::sync::Arc::clone(fits)),
            ));
        }
        let face: DisplayFace = match self.builtin_at(height, locale) {
            "unifont" => Box::new(Unifont::new(height)),
            name => Box::new(SetFace::new(builtin(name)?, height)?),
        };
        let band = self.band(face.cell().height, spacing);
        // with right-to-left text the built-in font draws through the
        // lookup that draws a joined letter's presentation form (ADR-1004
        // D6), as it does in a look's cell
        if band.is_none() && !cfg!(feature = "bidi") {
            return Some(face);
        }
        // D13: the built-in font in a taller cell, placed by the face that
        // places a frit's built-in glyphs, with a store that stays empty
        let (cell, step) = (face.cell(), face.size_step());
        Some(Box::new(DeferredFace::spaced(
            face,
            cell,
            band,
            step,
            Shared::default(),
        )))
    }

    /// The heights a zoom steps through (ADR-1200 D17): an outline frit's
    /// every height from the built-in's smallest, a bitmap frit's stated
    /// or own cell's height and its whole multiples, and the built-ins' in
    /// a look's cell the same. The built-ins in their own cell, and a
    /// bitmap frit with no cell, step through Unscii-8's 8 under 16, where
    /// it is drawn, and unifont's multiples of 16 from 16.
    fn rungs(&self, locale: &str) -> Vec<Rung> {
        let all = |step| Rung {
            step,
            min: step,
            end: u32::MAX,
        };
        // D17, an outline frit steps by 1
        if self.outline() {
            return vec![Rung {
                step: 1,
                min: Chosen::BUILTINS.smallest(locale),
                end: u32::MAX,
            }];
        }
        // D17, a bitmap frit steps by the font's height
        if let Some(cell) = self.base_cell() {
            return vec![all(cell.height)];
        }
        match self {
            Chosen::Builtin {
                name: Some("unscii-8"),
                ..
            } => vec![all(8)],
            Chosen::Builtin { name: Some(_), .. } => vec![all(16)],
            Chosen::Builtin { name: None, .. } | Chosen::Frit { .. }
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
            Chosen::Builtin { name: None, .. } | Chosen::Frit { .. } => vec![all(16)],
        }
    }

    /// The height `font-size=0` picks for a display `width` by `height` at
    /// `dpi`: the height of the cell a frit states, or an outline frit's
    /// `size=` in points at `dpi`, which no display's size moves (#145,
    /// ADR-1200 D17); else [`auto_size`] for the font's smallest height,
    /// which a frit's own cell takes at the largest whole multiple not
    /// over it, at least the cell itself, so a bitmap font keeps its own
    /// pixels whole (the operator, 2026-10-28).
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
    /// the cell itself (#145, ADR-1200 D17); with strikes and no
    /// `font-size` configured, the whole multiple of the strike the helper
    /// named for `height` (ADR-1204 D2); else `height`.
    pub fn snap(&self, height: u32) -> u32 {
        if let Chosen::Frit { sized: false, .. } = self
            && let Some(strike) = self.strike_at(height)
        {
            return whole_height(strike, height);
        }
        match self.base_cell() {
            Some(cell) if !self.outline() => frit_cell_at(Some(cell), height, cell).height,
            _ => height,
        }
    }

    /// The height a zoom asks for from `size` at `dpi` (ADR-1200 D17): an
    /// outline frit's next whole point in its direction whose pixels
    /// differ from `size` (the operator's decision on vtium/vtium#145);
    /// else the next of its rungs. `None` past [`ZOOM_IN_MAX`]
    /// in, or below the smallest out.
    pub fn zoom(&self, size: u32, dir: Zoom, locale: &str, dpi: f64) -> Option<u32> {
        if !self.outline() {
            return zoom(&self.rungs(locale), size, dir);
        }
        let under = points_under(size, dpi);
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

/// The largest height a zoom in starts from.
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
    use crate::scratch::Scratch;
    use vtium_font::Width;

    /// With nothing installed, no vtium.conf, no font sets, no font files
    /// and no font helper, vtium draws its terminal in the compiled-in GNU
    /// Unifont and Unscii-8, and its notices in its built-in font, "vtium
    /// built-in", in the terminal's cell. At 16 pixels the notice's letters
    /// differ from the terminal's Unifont, so the notice draws from neither
    /// the terminal's font nor Unifont. The terminal moves to the built-in
    /// font with card 5b of ADR-1204, which removes Unifont and Unscii-8
    /// from the binary.
    ///
    /// References:
    /// - ADR-1204 D1: vtium's own messages always draw in the built-in
    ///   font, the acceptance check's unit test, and the note of 2026-11-15
    ///   on cards 5a and 5b.
    #[test]
    fn with_nothing_installed_a_notice_draws_in_the_built_in_font() {
        use crate::term::{Overlay, OverlayColors};
        use vtium_render::{
            Canvas, Config as RenderConfig, Notice, Orientation, Renderer, Surface,
        };
        // an empty vtium.conf, an empty vconsole.conf and an empty kernel
        // command line name no font, so vtium asks for the default set;
        // with no font sets installed it draws its built-in fonts
        let config = crate::config::load(&[], &mut |_: &Path| Ok(None)).config;
        let default = wanted(&config, "", "").expect("the default set");
        let none = Sources {
            frits: PathBuf::from("/nonexistent/vtium-test-frits"),
            ..Sources::default()
        };
        assert!(frit_named(&none, &default).is_err());
        let text = "Keyboard /dev/input/event0 was not found.";
        let overlay = Overlay::new(None, &[text], None, OverlayColors::default());
        for (height, grid) in [(8, (40, 30)), (16, (40, 15))] {
            let face = Chosen::BUILTINS.face(height, "C").expect("a built-in face");
            let cell = face.cell();
            let renderer = Renderer::new(
                vtium_font::Font::new(face),
                320,
                240,
                RenderConfig::default(),
            );
            let mut drawn = Canvas::new(320, 240, 0);
            overlay.draw(&renderer, grid, Orientation::Normal, &mut drawn);
            let surface = Surface {
                width: 320,
                height: 240,
                orientation: Orientation::Normal,
                depth: renderer.depth(),
            };
            let colors = OverlayColors::default().notice;
            let in_font = |font: &vtium_font::Font<DisplayFace>| {
                let mut c = Canvas::new(320, 240, 0);
                Notice::new(text).draw(font, surface, colors, None, &mut c);
                c
            };
            let own = vtium_font::Font::new(own_face(cell).expect("the built-in font"));
            assert!(
                drawn.pixels() == in_font(&own).pixels(),
                "{height}: vtium built-in"
            );
            if height == 16 {
                let terminal = vtium_font::Font::new(Chosen::BUILTINS.face(16, "C").unwrap());
                assert!(drawn.pixels() != in_font(&terminal).pixels(), "not Unifont");
            }
        }
    }

    // ADR-1302 D14: a glyph is squeezed when it is exactly twice its
    // cells' width, or wider than them and 2 pixels more; the built-in
    // GNU Unifont squeezes its 16-pixel Devanagari into one 8x16 cell,
    // and neither a Latin letter nor, in tiny5's 6x9 cell, Unscii-8's
    #[test]
    fn a_built_in_glyph_is_squeezed_past_its_cells_and_two_pixels() {
        let c816 = Cell {
            width: 8,
            height: 16,
        };
        assert!(squeezed(16, c816, 1));
        assert!(squeezed(11, c816, 1));
        assert!(!squeezed(10, c816, 1));
        assert!(!squeezed(9, c816, 1));
        assert!(!squeezed(8, c816, 1));
        assert!(!squeezed(16, c816, 2));
        assert!(squeezed(32, c816, 2));
        assert!(builtin_squeezes('\u{915}', c816, "hi_IN"));
        assert!(!builtin_squeezes('A', c816, "hi_IN"));
        assert!(!builtin_squeezes('\u{4e00}', c816, "zh_CN"));
        let tiny5 = Cell {
            width: 6,
            height: 9,
        };
        assert!(!builtin_squeezes('A', tiny5, "C"));
    }

    fn config(args: &[&str]) -> Config {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        crate::config::load(&args, &mut |_: &std::path::Path| Ok(None)).config
    }

    // font-frits names the frits' directory, its default the one vtium is
    // built with
    #[test]
    fn font_frits_names_the_frits_directory() {
        assert_eq!(Sources::configured(&config(&[])).frits, Path::new(FRITS));
        let sources = Sources::configured(&config(&["--font-frits=/srv/frits"]));
        assert_eq!(sources.frits, Path::new("/srv/frits"));
        assert!(sources.helper.is_none(), "the helper font-helper names");
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
            ("cell=8x16\ncell=8x16\n", "line 2: cell= appears twice"),
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
        Chosen::frit(frit, sized, None).expect("a frit")
    }

    // ADR-1200 D17: at 72 DPI, where a point is a pixel, an outline frit
    // zooms by 1 pixel, from its stated cell, each height but the stated
    // one as wide as the built-in font there
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
    // whole multiple not over the automatic height, at least once; it zooms
    // by the cell's height
    #[test]
    fn a_frits_own_cell_scales_by_whole_factors_under_the_automatic_height() {
        for text in ["source=kbd:x\n", "source=kbd:x\nsize=16\n"] {
            assert!(frit_of(text, false).wants_own_cell(16, false), "{text}");
            assert!(!frit_of(text, true).wants_own_cell(16, true), "{text}");
        }
        assert!(!frit_of("source=kbd:x\ncell=8x16\n", false).wants_own_cell(16, false));
        let mut f = frit_of("source=kbd:x\n", false);
        f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
        assert!(!f.wants_own_cell(16, false));
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
        f.set_own_cell(OwnCell::Bitmap(cell(8, 8)), false);
        assert_eq!(f.auto_height(320, 240, "C", 96.0), 8);
        assert_eq!(f.auto_height(640, 480, "C", 96.0), 16);
        assert_eq!(f.face(16, "C").expect("a face").cell(), cell(16, 16));
    }

    // overhang part 1 (the maintainer, 2026-11-01): an outline frit with
    // no cell stated asks the helper at each height it draws at, a
    // configured font-size included, and takes each answer at its own
    // height alone; at a height with no answer it is as wide as the
    // built-in font, and it zooms by 1 pixel
    #[test]
    fn an_outline_frit_takes_the_helpers_cell_at_each_height_asked() {
        let mut f = frit_of("source=outline:/f.ttf\n", true);
        assert!(f.outline());
        assert!(f.wants_own_cell(16, true), "a configured size asks too");
        f.set_own_cell(OwnCell::Outline(cell(9, 16)), true);
        assert!(!f.wants_own_cell(16, true));
        assert!(f.wants_own_cell(17, true), "another height asks again");
        assert_eq!(f.face(16, "C").expect("a face").cell(), cell(9, 16));
        assert_eq!(f.face(17, "C").expect("a face").cell(), cell(8, 17));
        f.set_own_cell(OwnCell::Outline(cell(12, 22)), true);
        assert_eq!(f.face(22, "C").expect("a face").cell(), cell(12, 22));
        assert_eq!(f.face(16, "C").expect("a face").cell(), cell(9, 16));
        assert_eq!(f.snap(17), 17, "no whole multiples");
        // an outline frit that states a cell asks nothing
        let stated = frit_of("source=outline:/f.ttf\ncell=10x20\n", false);
        assert!(!stated.wants_own_cell(20, false));
    }

    // A frit whose first source is a bitmap font with strikes asks the
    // helper for its cell at each height, with a font size configured or
    // not, and keeps each answer for the heights it holds at: the height
    // asked at, the strike's own, and the whole multiple between them.
    // With no font size the cell is the strike's cell or its whole
    // multiple. A configured font size keeps its height, as wide as the
    // strike's whole multiple, which the helper centres in the cell. The
    // helper's answers below are those of Terminus's strikes.
    //
    // References:
    // - ADR-1204 D2: the strike for the automatic height, for a configured
    //   font size and for each zoom step.
    #[test]
    fn a_frit_with_strikes_takes_the_strike_of_each_height() {
        let text =
            "source=bdf:/16.bdf\nsha256=0\nstrike=bdf:/12.bdf\nstrike=bdf:/32.bdf\nlayer=unifont\n";
        // the tallest Terminus strike no taller than each height
        let strike = |h: u32| match h {
            12..14 => cell(6, 12),
            14..16 => cell(7, 14),
            16..18 => cell(8, 16),
            18..20 => cell(10, 18),
            20..22 => cell(10, 20),
            22..24 => cell(11, 22),
            24..28 => cell(12, 24),
            28..32 => cell(14, 28),
            _ => cell(16, 32),
        };
        for (sized, rows) in [
            (
                true,
                [
                    (12, cell(6, 12)),
                    (16, cell(8, 16)),
                    (18, cell(10, 18)),
                    (20, cell(10, 20)),
                    (23, cell(11, 23)),
                    (24, cell(12, 24)),
                    (28, cell(14, 28)),
                    (32, cell(16, 32)),
                    (40, cell(16, 40)),
                    (64, cell(32, 64)),
                ],
            ),
            (
                false,
                [
                    (12, cell(6, 12)),
                    (16, cell(8, 16)),
                    (18, cell(10, 18)),
                    (20, cell(10, 20)),
                    (23, cell(11, 22)),
                    (24, cell(12, 24)),
                    (28, cell(14, 28)),
                    (32, cell(16, 32)),
                    (40, cell(16, 32)),
                    (64, cell(32, 64)),
                ],
            ),
        ] {
            let mut f = frit_of(text, sized);
            assert!(f.struck() && !f.outline(), "{text}");
            for (height, want) in rows {
                assert!(f.wants_own_cell(height, sized), "{height}, sized {sized}");
                assert_eq!(f.strike_at(height), None);
                f.set_own_cell_at(OwnCell::Bitmap(strike(height)), height, sized);
                assert!(!f.wants_own_cell(height, sized));
                assert_eq!(f.strike_at(height), Some(strike(height)));
                let drawn = f.snap(height);
                assert_eq!(drawn, want.height, "{height}, sized {sized}");
                let face = f.face(drawn, "C").expect("a face");
                assert_eq!(face.cell(), want, "{height}, sized {sized}");
            }
        }
        // a height with no answer yet draws as wide as the built-in font
        let f = frit_of(text, true);
        assert_eq!(f.face(24, "C").expect("a face").cell(), cell(8, 24));
        // a stated cell wins, and asks nothing
        let stated = frit_of(&format!("{text}cell=10x20\n"), false);
        assert!(!stated.struck() && !stated.wants_own_cell(20, false));
    }

    // Only a first source that is a bitmap font followed by its own
    // strike= lines makes a frit with strikes. A bitmap source with no
    // strikes, a later source's strikes, and an outline source's strikes
    // keep the rules they have.
    #[test]
    fn only_a_bitmap_first_source_with_strikes_is_struck() {
        for (text, struck) in [
            ("source=bdf:/16.bdf\nstrike=bdf:/12.bdf\n", true),
            (
                "# strike=bdf:/12.bdf\nsource=psf:/16.psf\nversion=1\nstrike=bdf:/12.bdf # a comment\n",
                true,
            ),
            ("source=kbd:x\n", false),
            (
                "source=kbd:x\nsource=bdf:/16.bdf\nstrike=bdf:/12.bdf\n",
                false,
            ),
            ("source=kbd:x\nlayer=unifont\nstrike=bdf:/12.bdf\n", false),
            ("source=outline:/f.ttf\nstrike=bdf:/12.bdf\n", false),
            ("source=font:Terminus\nstrike=bdf:/12.bdf\n", false),
            ("strike=bdf:/12.bdf\n", false),
            // the helper ignores a strike= under a built-in font or the
            // system's console font, so the set has no strikes
            ("source=builtin:unifont\nstrike=kbd:ter-v16n\n", false),
            ("source=builtin:unscii-8\nstrike=bdf:/12.bdf\n", false),
            ("source=vconsole\nstrike=kbd:ter-v16n\n", false),
            ("source=console:2\nstrike=kbd:ter-v16n\n", false),
        ] {
            assert_eq!(frit_struck(text), struck, "{text}");
        }
        // a bitmap frit with no strikes asks once, and not with a font size
        let mut f = frit_of("source=kbd:x\n", false);
        f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
        assert!(!f.struck() && !f.wants_own_cell(24, false));
        assert!(!frit_of("source=kbd:x\n", true).wants_own_cell(24, true));
    }

    // overhang part 1: the helper's answer, not the frit's text, says what
    // kind of font the first source is. A font: source the text reads as
    // an outline font that resolves to a bitmap font answers with its own
    // cell, and the frit then zooms by that cell's height; a source the
    // text reads as a bitmap font answered as an outline font zooms by 1
    #[test]
    fn the_helpers_answer_decides_whether_a_frit_is_outline() {
        let mut f = frit_of("source=font:Terminus\n", false);
        assert!(f.outline(), "the text's guess");
        assert!(f.wants_own_cell(16, false));
        f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
        assert!(!f.outline());
        assert!(!f.wants_own_cell(16, false));
        assert_eq!(f.face(32, "C").expect("a face").cell(), cell(16, 32));
        assert_eq!(zoom(&f.rungs("C"), 16, Zoom::In), Some(32));
        // with font-size configured the bitmap answer's cell does not count
        let mut f = frit_of("source=font:Terminus\n", true);
        f.set_own_cell(OwnCell::Bitmap(cell(8, 16)), true);
        assert!(!f.outline());
        assert!(!f.wants_own_cell(19, true));
        assert_eq!(f.face(19, "C").expect("a face").cell(), cell(8, 19));
        let mut f = frit_of("source=kbd:missing\nsource=outline:/f.ttf\n", false);
        assert!(f.outline(), "the text names an outline source");
        let mut f2 = frit_of("source=kbd:missing\n", false);
        f2.set_own_cell(OwnCell::Outline(cell(9, 16)), false);
        assert!(f2.outline());
        assert_eq!(f2.face(16, "C").expect("a face").cell(), cell(9, 16));
        f.set_own_cell(OwnCell::Outline(cell(9, 16)), false);
        assert_eq!(f.face(16, "C").expect("a face").cell(), cell(9, 16));
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
            ("size=24\nsize=32\n", "line 2: size= appears twice"),
            (
                "size=5\n",
                "line 1: size=5 is not a size in points from 6 to 96",
            ),
            ("size=016\n", "line 1: size=016 is not"),
        ] {
            let frit = FritText {
                name: "f".into(),
                text: text.into(),
            };
            let e = Chosen::frit(frit, false, None).expect_err(text);
            assert!(e.contains(why), "{text:?}: {e}");
        }
    }

    /// The rows of `c`'s glyph in `face`.
    fn glyph_rows(face: &DisplayFace, c: char) -> Vec<Vec<u8>> {
        let g = face
            .draw(&[c], vtium_font::Style::default())
            .expect("a glyph");
        g.rows().map(<[u8]>::to_vec).collect()
    }

    // ADR-1302 D13: the frit's fit= reaches the glyphs the console draws
    // itself in the frit's cell. Box drawing fills the taller cell by
    // default; under the frit's fit=natural for U+2500 to U+257F a box line
    // keeps the text band's rows, with the added rows blank
    #[test]
    fn a_frits_fit_places_the_built_in_glyphs_in_its_cell() {
        let natural = frit_of(
            "source=kbd:x\nsource=kbd:y\ncover=U+2500-U+257F\nfit=natural\nline-spacing=20\n",
            false,
        );
        let plain = frit_of("source=kbd:x\nline-spacing=20\n", false);
        let at = |f: &Chosen| f.face_spaced(16, "C", None).expect("a face");
        // 20% of 16 is 3.2: 3 rows, 2 above and 1 below
        let inked = |f: &Chosen| -> Vec<usize> {
            glyph_rows(&at(f), '\u{2502}')
                .iter()
                .enumerate()
                .filter(|(_, row)| row.iter().any(|&p| p != 0))
                .map(|(y, _)| y)
                .collect()
        };
        assert_eq!(inked(&plain), (0..19).collect::<Vec<_>>());
        assert_eq!(inked(&natural), (2..18).collect::<Vec<_>>());
    }

    // ADR-1302 D13: a frit's `line-spacing=`, and vtium.conf's
    // `font-line-spacing` over it, make the cell vtium works out taller by
    // a whole percentage of the text's height, split above and below the
    // text band with the odd pixel above; the text band stays the cell the
    // face had without spacing
    #[test]
    fn line_spacing_makes_the_cell_taller_split_above_and_below() {
        let plain = frit_of("source=kbd:x\n", false);
        let spaced = frit_of("source=kbd:x\nline-spacing=20\n", false);
        let at = |f: &Chosen, spacing| f.face_spaced(16, "C", spacing).expect("a face");
        assert_eq!(at(&plain, None).cell(), cell(8, 16));
        assert_eq!(at(&plain, None).band(), Band::NONE);
        // 20% of 16 is 3.2: 3 rows, 2 above and 1 below
        let face = at(&spaced, None);
        assert_eq!(face.cell(), cell(8, 19));
        assert_eq!(face.band(), Band { above: 2, below: 1 });
        // vtium.conf's font-line-spacing wins over the frit's, 0 included
        assert_eq!(at(&spaced, Some(50)).cell(), cell(8, 24));
        assert_eq!(at(&spaced, Some(0)).cell(), cell(8, 16));
        assert_eq!(at(&plain, Some(25)).band(), Band { above: 2, below: 2 });
        // the built-in fonts with no frit take it too
        let face = Chosen::BUILTINS
            .face_spaced(16, "C", Some(25))
            .expect("a face");
        assert_eq!(
            (face.cell(), face.band()),
            (cell(8, 20), Band { above: 2, below: 2 })
        );
        // a letter keeps its rows in the text band, blank above and below,
        // and a box line, drawn by the built-in font, stretches to fill
        let unspaced = Chosen::BUILTINS.face(16, "C").expect("a face");
        let a = glyph_rows(&unspaced, 'A');
        let blank = vec![0u8; a[0].len()];
        let mut want = vec![blank.clone(); 2];
        want.extend(a.iter().cloned());
        want.extend(vec![blank; 2]);
        assert_eq!(glyph_rows(&face, 'A'), want);
        let line = glyph_rows(&unspaced, '\u{2502}');
        let mut want = vec![line[0].clone(); 2];
        want.extend(line.iter().cloned());
        want.extend(vec![line[15].clone(); 2]);
        assert_eq!(glyph_rows(&face, '\u{2502}'), want);
        // the lines move down with the text band
        let (u, d) = (unspaced.decorations(), face.decorations());
        assert_eq!((d.underline, d.strike), (u.underline + 2, u.strike + 2));
        for (text, why) in [
            (
                "line-spacing=101\n",
                "line 1: line-spacing=101 is not a whole percentage from 0 to 100",
            ),
            (
                "line-spacing=5\nline-spacing=5\n",
                "line 2: line-spacing= appears twice",
            ),
        ] {
            let frit = FritText {
                name: "f".into(),
                text: text.into(),
            };
            let e = Chosen::frit(frit, false, None).expect_err(text);
            assert!(e.contains(why), "{text:?}: {e}");
        }
    }

    // ADR-1302 D13: a cell the frit or the look states is exact and takes
    // no spacing; where a configured font-size overrides the frit's cell=,
    // the cell vtium works out takes it
    #[test]
    fn a_stated_cell_takes_no_spacing_unless_font_size_overrides_it() {
        let text = "source=kbd:x\ncell=10x20\nline-spacing=50\n";
        let stated = frit_of(text, false);
        let face = stated.face_spaced(20, "C", Some(50)).expect("a face");
        assert_eq!((face.cell(), face.band()), (cell(10, 20), Band::NONE));
        let sized = frit_of(text, true);
        let face = sized.face_spaced(20, "C", None).expect("a face");
        assert_eq!(face.cell().height, 30);
        assert_eq!(face.band(), Band { above: 5, below: 5 });
        let look = Chosen::frit(
            FritText {
                name: "f".into(),
                text: text.into(),
            },
            true,
            Some(cell(12, 24)),
        )
        .expect("a frit");
        let face = look.face_spaced(24, "C", Some(50)).expect("a face");
        assert_eq!((face.cell(), face.band()), (cell(12, 24), Band::NONE));
        let builtins = Chosen::builtins(Some(cell(12, 24)));
        let face = builtins.face_spaced(24, "C", Some(50)).expect("a face");
        assert_eq!((face.cell(), face.band()), (cell(12, 24), Band::NONE));
    }

    // ADR-1403 D5, ADR-1200 D16: the look is more specific than its frit,
    // so a cell the look states wins over the frit's cell= and size=, and
    // the frit asks the helper for no cell of its own
    #[test]
    fn a_looks_cell_wins_over_its_frits_cell_and_size() {
        let look = Some(cell(12, 24));
        for text in [
            "source=outline:/f.ttf\ncell=10x20\nsize=16\n",
            "source=outline:/f.ttf\nsize=16\n",
            "source=kbd:x\ncell=10x20\n",
            "source=kbd:x\n",
        ] {
            let frit = FritText {
                name: "f".into(),
                text: text.into(),
            };
            let f = Chosen::frit(frit, false, look).expect("a frit");
            assert_eq!(f.auto_height(320, 240, "C", 96.0), 24, "{text:?}");
            assert_eq!(
                f.face(24, "C").expect("a face").cell(),
                cell(12, 24),
                "{text:?}"
            );
            assert!(!f.wants_own_cell(24, false), "{text:?}");
        }
    }

    // ADR-1200 D7: a look's 6x16 cell is 16 pixels tall but narrower than
    // unifont's 8 columns, so the built-ins draw Unscii-8 in it, centered
    // down the cell and on it, its first column in the cell before it, and
    // OSC 50's query names the strike that draws
    #[test]
    fn a_looks_cell_too_narrow_for_unifont_draws_and_names_unscii_8() {
        let font = Chosen::builtins(Some(cell(6, 16)));
        assert_eq!(font.name(16, "C"), "unscii-8");
        let a = vtium_font::Font::new(font.face(16, "C").expect("a face"))
            .glyph(&['A'], Width::Narrow, vtium_font::Style::default())
            .expect("a glyph");
        assert_eq!((a.width(), a.height()), (6, 16));
        let unscii = Chosen::Builtin {
            name: Some("unscii-8"),
            cell: None,
        }
        .face(8, "C")
        .expect("unscii-8")
        .draw(&['A'], vtium_font::Style::default())
        .expect("a glyph");
        for y in 0..8 {
            for x in 0..6 {
                assert_eq!(a.pixel(x, y + 4), unscii.pixel(x + 1, y), "{x},{y}");
            }
        }
        // Unscii-8's first column lies in the cell before, in the glyph's
        // ink box where it holds ink
        let first = (0..8).any(|y| unscii.pixel(0, y).is_some_and(|p| p[0] > 0));
        assert_eq!(a.ink().map(|i| i.left()), first.then_some(-1));
        // a zoom to twice the cell, 12x32, fits unifont's 8x16 once, so
        // unifont draws there, and is named
        assert_eq!(font.name(32, "C"), "unifont");
        assert_eq!(font.face(32, "C").expect("a face").cell(), cell(12, 32));
        // a zh, ja or ko locale draws unifont at every size, and names it
        assert_eq!(font.name(16, "ja_JP.UTF-8"), "unifont");
    }

    // ADR-1200 D7, D17, ADR-1403 D5: the built-in fonts in a look's cell
    // draw in that cell whatever the display's size, fitted to it as in a
    // frit's stated cell, and a zoom steps by the cell's whole multiples;
    // a built-in font named keeps to its own strike in the cell
    #[test]
    fn the_built_in_fonts_draw_in_a_looks_cell() {
        let look = Some(cell(12, 24));
        let plain = |name| {
            Chosen::Builtin { name, cell: None }
                .face(16, "C")
                .expect("a face")
                .draw(&['A'], vtium_font::Style::default())
                .expect("a glyph")
        };
        for (font, own) in [
            (Chosen::builtins(look), plain(None)),
            (
                builtin_named("unifont", look).expect("unifont"),
                plain(Some("unifont")),
            ),
            (
                builtin_named("unscii-8", look).expect("unscii-8"),
                Chosen::Builtin {
                    name: Some("unscii-8"),
                    cell: None,
                }
                .face(8, "C")
                .expect("unscii-8")
                .draw(&['A'], vtium_font::Style::default())
                .expect("a glyph"),
            ),
        ] {
            let name = font.name(24, "C").to_owned();
            assert_eq!(font.auto_height(1920, 1080, "C", 96.0), 24, "{name}");
            assert_eq!(font.snap(30), 24, "{name}");
            let face = font.face(24, "C").expect("a face");
            assert_eq!(face.cell(), cell(12, 24), "{name}");
            let a = face
                .draw(&['A'], vtium_font::Style::default())
                .expect("a glyph");
            assert_eq!((a.width(), a.height()), (12, 24), "{name}");
            // the font's own glyph, centered in the larger cell
            let (dx, dy) = ((12 - own.width()) / 2, (24 - own.height()) / 2);
            for y in 0..own.height() {
                for x in 0..own.width() {
                    assert_eq!(a.pixel(x + dx, y + dy), own.pixel(x, y), "{name} {x},{y}");
                }
            }
            assert_eq!(font.zoom(24, Zoom::In, "C", 96.0), Some(48), "{name}");
            assert_eq!(font.zoom(24, Zoom::Out, "C", 96.0), None, "{name}");
            assert_eq!(
                font.face(48, "C").expect("a face").cell(),
                cell(24, 48),
                "{name}"
            );
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
        let b = Chosen::BUILTINS;
        assert_eq!(b.zoom(16, Zoom::In, "C", 300.0), Some(32));
        let mut k = frit_of("source=kbd:x\n", false);
        k.set_own_cell(OwnCell::Bitmap(cell(8, 16)), false);
        assert_eq!(k.zoom(16, Zoom::In, "C", 300.0), Some(32));
    }

    // #145: under 16 pixels the smaller strike, Unscii-8, draws
    #[test]
    fn a_small_stated_cell_draws_unscii_8_in_it() {
        let f = frit_font(Some(cell(7, 12)));
        let face = f.face(12, "C").expect("a face");
        assert_eq!(face.cell(), cell(7, 12));
        let a = vtium_font::Font::new(face)
            .glyph(&['A'], Width::Narrow, vtium_font::Style::default())
            .expect("a glyph");
        assert_eq!((a.width(), a.height()), (7, 12));
        // the 8th column, past the cell's right edge, is blank
        assert!(a.ink().is_none());
        let unscii = Chosen::Builtin {
            name: Some("unscii-8"),
            cell: None,
        }
        .face(8, "C")
        .expect("unscii-8")
        .draw(&['A'], vtium_font::Style::default())
        .expect("a glyph");
        // Unscii-8's 8x8, two rows down, its first 7 columns in the cell
        for y in 0..8 {
            for x in 0..7 {
                assert_eq!(a.pixel(x, y + 2), unscii.pixel(x, y), "{x},{y}");
            }
        }
    }

    // covers: text.overhang
    // ADR-1200 D7, ADR-1302 D14: Tiny5 Mono's 6x9 cell is narrower than
    // Unscii-8's 8x8, so the built-in glyph keeps all 8 of its columns,
    // centred on the cell: its first column lies in the cell before it and
    // its 8th, which Unscii-8 leaves blank, in the cell after it. It is
    // never squeezed, since it is at most 2 pixels wider than the cell. It
    // sits in the top 8 rows, its baseline on Tiny5 Mono's, and the 9th row
    // is blank.
    #[test]
    fn unscii_8_in_a_narrower_cell_keeps_every_column() {
        let f = frit_font(Some(cell(6, 9)));
        let face = vtium_font::Font::new(f.face(9, "C").expect("a face"));
        assert_eq!(face.cell(), cell(6, 9));
        let builtin = Chosen::Builtin {
            name: Some("unscii-8"),
            cell: None,
        }
        .face(8, "C")
        .expect("unscii-8");
        let rows = |px: &[u8], w: usize| -> Vec<String> {
            px.chunks_exact(w)
                .map(|r| r.iter().map(|&b| if b > 0 { '#' } else { '.' }).collect())
                .collect()
        };
        let draw = |c| {
            face.glyph(&[c], Width::Narrow, vtium_font::Style::default())
                .expect("a glyph")
        };
        let m = draw('M');
        assert_eq!((m.width(), m.height()), (6, 9));
        let ink = m.ink().expect("an ink box");
        assert_eq!((ink.left(), ink.top(), ink.width()), (-1, 0, 7));
        assert_eq!(
            rows(ink.pixels(), 7),
            [
                "##...##", "###.###", "#######", "##.#.##", "##...##", "##...##", "##...##",
                ".......", "......."
            ],
            "Unscii-8's M, its first column in the cell before"
        );
        for c in ['A', 'M', 'g', 'y', '#', '|'] {
            let (g, own) = (draw(c), builtin.draw(&[c], vtium_font::Style::default()));
            let own = own.expect("a glyph");
            for y in 0..8 {
                for x in 0..6 {
                    assert_eq!(g.pixel(x, y), own.pixel(x + 1, y), "{c} at {x},{y}");
                }
            }
            assert!(
                (0..6).all(|x| g.pixel(x, 8) == Some(&[0][..])),
                "{c}'s 9th row"
            );
            assert!(g.pixels().iter().all(|&p| p == 0 || p == 255), "{c}");
        }
    }

    // ADR-1200 D17: with no stated cell, the display's: the automatic
    // height, #118's floor first, as wide as the built-in font
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

    // A font set's `size=` is read within the size bounds that vtium-config
    // reads it within, never the character cell's, at both ends.
    #[test]
    fn a_font_sets_size_is_read_within_the_size_bounds() {
        use crate::config::keys::{FRIT_SIZE_MAX, FRIT_SIZE_MIN};
        for (size, read) in [
            (FRIT_SIZE_MIN - 1, None),
            (FRIT_SIZE_MIN, Some(FRIT_SIZE_MIN)),
            (FRIT_SIZE_MAX, Some(FRIT_SIZE_MAX)),
            (FRIT_SIZE_MAX + 1, None),
        ] {
            let got = frit_size(&format!("size={size}\n"));
            match read {
                Some(points) => assert_eq!(got, Ok(Some(points)), "{size}"),
                None => assert_eq!(
                    got,
                    Err(format!(
                        "line 1: size={size} is not a size in points from {FRIT_SIZE_MIN} to {FRIT_SIZE_MAX}"
                    )),
                    "{size}"
                ),
            }
        }
    }

    // ADR-1302 D13: check-config names a frit's line-spacing= that
    // vtium-config refuses, for which vtium draws its built-in font
    #[test]
    fn check_config_names_a_line_spacing_the_frit_reader_refuses() {
        assert!(frit_warnings("f", "source=kbd:x\nline-spacing=0\n").is_empty());
        assert!(frit_warnings("f", "source=kbd:x\nline-spacing=100\n").is_empty());
        assert_eq!(
            frit_warnings("f", "source=kbd:x\nline-spacing=120\n"),
            [
                "font set f: line 2: line-spacing=120 is not a whole percentage from 0 to 100, so vtium uses its built-in font"
            ]
        );
        for bad in ["010", "+5", "5%", "-1", "1.5"] {
            assert_eq!(
                frit_warnings("f", &format!("line-spacing={bad}\n")).len(),
                1,
                "{bad}"
            );
        }
    }

    #[test]
    fn check_config_names_a_cell_that_does_not_suit_its_frit() {
        assert!(frit_warnings("f", "source=kbd:x\n").is_empty());
        assert!(frit_warnings("f", "source=builtin:unifont\ncell=8x16\n").is_empty());
        assert_eq!(
            frit_warnings("f", "cell=8\n"),
            [
                "font set f: line 1: cell=8 is not WIDTHxHEIGHT in pixels, each from 6 to 96, so vtium uses its built-in font"
            ]
        );
        assert_eq!(
            frit_warnings(
                "f",
                "source=outline:/f.ttf\nsource=builtin:unifont\nsource=builtin:unscii-8\ncell=6x12\n"
            ),
            [
                "font set f: its 6x12 character cell is smaller than the built-in font's 8x8, so the built-in font is cut to fit until the font set's characters are ready",
                "font set f: line 2: the characters of source=builtin:unifont (8x16) do not fit the 6x12 character cell, so vtium does not use that source",
                "font set f: line 3: the characters of source=builtin:unscii-8 (8x8) do not fit the 6x12 character cell, so vtium does not use that source",
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
        let b = Chosen::BUILTINS;
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
        let unscii = Chosen::Builtin {
            name: Some("unscii-8"),
            cell: None,
        };
        assert_eq!(zoom(&unscii.rungs("C"), 8, Zoom::In), Some(16));
        assert_eq!(unscii.face(16, "C").expect("a face").cell().width, 16);
        let unifont = Chosen::Builtin {
            name: Some("unifont"),
            cell: None,
        };
        assert_eq!(unifont.smallest("C"), 16);
    }

    /// Each font file the shipped font set or layer `rel`, under
    /// vtium-config's `frits` folder, names by its path.
    fn shipped_paths(rel: &str) -> Vec<PathBuf> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vtium-config/frits")
            .join(rel);
        std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter_map(|l| {
                l.strip_prefix("source=")
                    .or_else(|| l.strip_prefix("strike="))
            })
            .filter_map(|v| v.split_once(':').map(|(_, p)| PathBuf::from(p)))
            .collect()
    }

    /// The batch [`batch::Batch::system`] builds from the batch file
    /// `file`, vconsole.conf at `vconsole` and the kernel command line at
    /// `cmdline`, read under `trust`.
    fn system_batch(file: &Path, vconsole: &Path, cmdline: &Path, trust: &Trust) -> batch::Read {
        let places = batch::Places {
            file: file.to_path_buf(),
            vconsole_conf: vconsole.to_path_buf(),
            cmdline: cmdline.to_path_buf(),
            ..batch::Places::system()
        };
        batch::Batch::system(&places, trust)
    }

    // With no batch file, the
    // batch is the list of a fresh install. It holds every font file the
    // default set terminus and its fallback layer name, and the console
    // font vconsole.conf names, so the default set draws with no setup. A
    // batch file that root wrote replaces the list.
    //
    // References:
    // - ADR-1205 D4, its note of 2026-11-18: with no batch file, the batch is the list of a fresh install.
    #[test]
    fn with_no_batch_file_the_default_set_and_the_console_font_are_active() {
        let d = Scratch::new("fresh-batch");
        let vconsole = d.join("vconsole.conf");
        std::fs::write(
            &vconsole,
            "FONT=/usr/share/kbd/consolefonts/ter-v16n.psf.gz\n",
        )
        .unwrap();
        let file = d.join("batch.conf");
        let read = system_batch(&file, &vconsole, &d.join("cmdline"), &trust());
        assert_eq!(
            read.findings,
            [batch::Finding::Missing { path: file.clone() }]
        );
        let default_set = shipped_paths("console/terminus.vtfrit");
        let fallback = shipped_paths("layers/unifont.vtlayer");
        assert_eq!((default_set.len(), fallback.len()), (18, 1));
        for path in default_set.iter().chain(&fallback) {
            assert!(read.batch.holds(path), "{} is not active", path.display());
        }
        assert!(
            read.batch
                .holds(Path::new("/usr/share/kbd/consolefonts/ter-v16n.psf.gz"))
        );
        assert!(
            !read
                .batch
                .holds(Path::new("/usr/share/fonts/TTF/DejaVuSansMono.ttf"))
        );
        // a batch file replaces the fresh list
        std::fs::write(&file, "/usr/share/fonts/TTF/DejaVuSansMono.ttf\n").unwrap();
        let me = rustix::process::geteuid().as_raw();
        let read = system_batch(
            &file,
            &vconsole,
            &d.join("cmdline"),
            &Trust::running_as(me).below(&d),
        );
        assert_eq!(read.findings, []);
        assert!(
            read.batch
                .holds(Path::new("/usr/share/fonts/TTF/DejaVuSansMono.ttf"))
        );
        assert!(!read.batch.holds(&default_set[0]));
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
        assert_eq!(
            wanted(&config(&[]), "", ""),
            Some(Wanted {
                name: "terminus".into(),
                came: Came::Default
            }),
            "with no font named, the default set"
        );
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
        let none = Sources {
            frits: PathBuf::from("/nonexistent/vtium-test-frits"),
            ..Sources::default()
        };
        let made = |name: &str, came| {
            frit_named(
                &none,
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

    /// With no font named, vtium draws the default set `terminus` from the
    /// catalog's console folder. Where no set of that name is installed,
    /// vtium makes no set on the fly for the name, as it does for a kbd
    /// font's name: the read is refused with a line that names the set,
    /// and vtium draws its built-in font.
    ///
    /// References:
    /// - ADR-1204 D2: the order in which vtium chooses a font, with
    ///   `terminus` after vconsole's font and before the built-in font.
    #[test]
    fn with_no_font_named_vtium_draws_terminus_where_it_is_installed() {
        let dir = Scratch::new("font-default");
        let default = wanted(&config(&[]), "", "").expect("the default set");
        assert_eq!(default.name, DEFAULT_SET);
        let sources = Sources {
            frits: dir.to_path_buf(),
            trust: trust().below(&dir),
            ..Sources::default()
        };
        let why = frit_named(&sources, &default).expect_err("no terminus is installed");
        assert_eq!(
            why,
            "No font is named, and the default font set terminus is not installed."
        );
        std::fs::create_dir_all(dir.join("console")).expect("the folder");
        let text = "source=bdf:/usr/share/vtium/fonts/ter-u16n.bdf.gz\n";
        std::fs::write(dir.join("console/terminus.vtfrit"), text).expect("the set");
        let frit = frit_named(&sources, &default).expect("terminus");
        assert_eq!((frit.name.as_str(), frit.text.as_str()), ("terminus", text));
    }

    /// A display where 16-pixel rows leave fewer than 80 columns by 25 rows
    /// is small. On a small display vtium draws the built-in font's 8×8
    /// strike in place of the default set, so a 320×240 panel keeps 40
    /// columns by 30 rows.
    ///
    /// References:
    /// - ADR-1204 D2: an 8-pixel cell draws the built-in font's 8×8 strike.
    /// - ADR-1200 D17: the small-display rule.
    #[test]
    fn a_small_display_keeps_the_built_in_font_with_no_font_named() {
        for (w, h) in [(320, 240), (480, 320), (632, 480), (640, 384)] {
            assert!(small_display(w, h), "{w}x{h}");
            assert_eq!(auto_size(w, h, 8), 8, "{w}x{h}");
            assert!(!default_set_fits([(w, h)]), "{w}x{h}");
        }
        for (w, h) in [(640, 400), (640, 480), (1920, 1080), (0, 0)] {
            assert!(!small_display(w, h), "{w}x{h}");
            assert!(default_set_fits([(w, h)]), "{w}x{h}");
        }
        // a head's displays count by the smallest width and the smallest
        // height among them, as the automatic size takes them
        assert!(!default_set_fits([(1920, 1080), (320, 240)]));
        assert!(!default_set_fits([(1920, 240), (320, 1080)]));
        assert!(default_set_fits([(1920, 1080), (640, 480)]));
        // with no display known yet, the default set draws
        assert!(default_set_fits([]));
    }

    // A set is found by its name in the frits' directory itself, where a
    // person's own sets live, and then in each category folder of the
    // catalog. A name in the directory itself wins over a category folder,
    // two category folders are searched in the catalog's order, and a name
    // in a folder that is not a category, or in the layers' folder, is not
    // a set.
    //
    // References:
    // - ADR-1204 D9: the category folders and the order vtium searches.
    #[test]
    fn a_set_is_found_in_the_frits_directory_and_then_in_its_category_folder() {
        let dir = Scratch::new("font-catalog");
        for (folder, name) in [
            ("console", "terminus"),
            ("code", "both"),
            ("small", "both"),
            ("code", "mine"),
            ("", "mine"),
            ("elsewhere", "stray"),
            ("layers", "layered"),
        ] {
            std::fs::create_dir_all(dir.join(folder)).expect("the folder");
            std::fs::write(
                dir.join(folder).join(format!("{name}.vtfrit")),
                "source=kbd:x\n",
            )
            .expect("the set");
        }
        assert_eq!(
            frit(&dir, "terminus"),
            Some(dir.join("console/terminus.vtfrit"))
        );
        assert_eq!(frit(&dir, "mine"), Some(dir.join("mine.vtfrit")));
        assert_eq!(frit(&dir, "both"), Some(dir.join("code/both.vtfrit")));
        assert_eq!(frit(&dir, "stray"), None);
        assert_eq!(frit(&dir, "layered"), None);
        assert_eq!(frit(&dir, "absent"), None);
        assert_eq!(frit(&dir, "../console/terminus"), None);
    }

    // ADR-1401 D10, ADR-1403 D3: vtium reads a frit, at the start and for
    // OSC 50, only once the ownership check vtium.conf passes trusts it. A
    // frit its group can write, and a frit in a directory its group can
    // write, are refused, each with the path that fails, its mode and the
    // command that mends it.
    #[test]
    fn a_frit_is_read_only_when_the_ownership_check_trusts_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("font-trust");
        let open = dir.join("open");
        std::fs::create_dir_all(&open).expect("the directories");
        let mode = |path: &Path, mode: u32| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
                .expect("the mode");
        };
        for path in [
            dir.join("t.vtfrit"),
            dir.join("g.vtfrit"),
            open.join("t.vtfrit"),
        ] {
            std::fs::write(&path, "source=kbd:x\n").expect("the frit");
            mode(&path, 0o644);
        }
        mode(&dir, 0o755);
        mode(&dir.join("g.vtfrit"), 0o664);
        mode(&open, 0o775);
        let sources = |frits: &Path| Sources {
            frits: frits.to_path_buf(),
            trust: trust().below(&dir),
            ..Sources::default()
        };
        let at_start = |frits: &Path, name: &str| {
            let wanted = Wanted {
                name: name.into(),
                came: Came::Config,
            };
            frit_named(&sources(frits), &wanted).map(|frit| frit.text)
        };
        let by_osc_50 = |frits: &Path, name: &str| {
            by_name(&sources(frits), name, false, None, None, &[], &no_paths()).map(|chosen| {
                match chosen {
                    Chosen::Frit { frit, .. } => frit.text,
                    Chosen::Builtin { .. } => String::new(),
                }
            })
        };
        let results = [
            at_start(&dir, "t"),
            at_start(&dir, "g"),
            at_start(&open, "t"),
            by_osc_50(&dir, "t"),
            by_osc_50(&dir, "g"),
            by_osc_50(&open, "t"),
        ];
        let read = Ok("source=kbd:x\n".to_owned());
        let refused = |name: &str, file: &Path, failed: &Path, mode: &str| {
            Err(format!(
                "vtium does not read the font set {name} from \"{file}\", because other users can change \"{failed}\" (mode {mode}). Run `chmod go-w {failed}` so that only its owner can change it.",
                file = file.display(),
                failed = failed.display(),
            ))
        };
        let writable = refused("g", &dir.join("g.vtfrit"), &dir.join("g.vtfrit"), "664");
        let in_open = refused("t", &open.join("t.vtfrit"), &open, "775");
        assert_eq!(
            results,
            [
                read.clone(),
                writable.clone(),
                in_open.clone(),
                read,
                writable,
                in_open
            ]
        );
    }

    // The console puts each layer's sources in place of the set's layer=
    // line, reads each layer from the layers' folder of the frits'
    // directory under the ownership check the frit passes, and draws the
    // set without a layer it does not read, with one sentence for each.
    //
    // References:
    // - ADR-1204 D5: the splice in place and the ownership check.
    #[test]
    fn a_frits_layers_go_in_place_under_the_frits_ownership_check() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("font-layers");
        let layers = dir.join("layers");
        std::fs::create_dir_all(&layers).expect("the layers' folder");
        let mode = |path: &Path, mode: u32| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
                .expect("the mode");
        };
        let write = |path: &Path, text: &str| {
            std::fs::write(path, text).expect("the file");
            mode(path, 0o644);
        };
        write(
            &dir.join("set.vtfrit"),
            "cell=8x16\nsource=kbd:text\nlayer=symbols\nlayer=theirs\nlayer=gone\nlayer=emoji\nline-spacing=10\n",
        );
        write(
            &layers.join("symbols.vtlayer"),
            "role=symbols\nsource=font:Symbols\ncover=U+E000-U+F8FF\n",
        );
        write(
            &layers.join("emoji.vtlayer"),
            "role=emoji\nsource=font:Emoji\n",
        );
        write(
            &layers.join("theirs.vtlayer"),
            "role=script\nsource=font:Theirs\n",
        );
        mode(&layers.join("theirs.vtlayer"), 0o664);
        mode(&dir, 0o755);
        mode(&layers, 0o755);
        let sources = Sources {
            frits: dir.to_path_buf(),
            trust: trust().below(&dir),
            ..Sources::default()
        };
        let wanted = Wanted {
            name: "set".into(),
            came: Came::Config,
        };
        let frit = frit_named(&sources, &wanted).expect("the frit");
        assert_eq!(
            frit.text,
            "cell=8x16\nsource=kbd:text\nlayer=symbols\n\nsource=font:Symbols\ncover=U+E000-U+F8FF\nlayer=\nlayer=\nlayer=\nlayer=emoji\n\nsource=font:Emoji\nlayer=\nline-spacing=10\n"
        );
        // a key of the set after a layer is named by the frit's own line
        assert_eq!(frit_line_spacing(&frit.text), Ok(Some(10)));
        let after = with_layers(
            "set",
            &dir,
            "source=kbd:text\nlayer=symbols\ncell=8\n",
            &sources.trust,
        )
        .0;
        assert!(
            frit_cell(&after).unwrap_err().starts_with("line 3: "),
            "{after}"
        );
        let by_osc_50 = match by_name(&sources, "set", false, None, None, &[], &no_paths())
            .expect("the frit")
        {
            Chosen::Frit { frit, .. } => frit.text,
            Chosen::Builtin { .. } => String::new(),
        };
        assert_eq!(by_osc_50, frit.text);
        let (_, notes) =
            FritText::read_noting(&dir.join("set.vtfrit"), &dir, &sources.trust).expect("the frit");
        // a frit elsewhere takes its layers from the frits' directory, not
        // from a layers/ folder beside it
        let elsewhere = Scratch::new("font-layers-elsewhere");
        mode(&elsewhere, 0o755);
        write(
            &elsewhere.join("draft.vtfrit"),
            "source=kbd:text\nlayer=emoji\n",
        );
        let (draft, missing) = FritText::read_noting(
            &elsewhere.join("draft.vtfrit"),
            &dir,
            &trust().below(&elsewhere),
        )
        .expect("the draft");
        assert!(missing.is_empty(), "{missing:?}");
        assert!(draft.text.contains("source=font:Emoji\n"), "{}", draft.text);
        let theirs = layers.join("theirs.vtlayer");
        assert_eq!(
            notes,
            [
                format!(
                    "font set set: line 4: vtium does not read the layer theirs from {}, because other users can change {} (mode 664). Run `chmod go-w {}` so that only its owner can change it. vtium draws the font set without that layer.",
                    quoted(&theirs),
                    quoted(&theirs),
                    theirs.display()
                ),
                format!(
                    "font set set: line 5: the layer gone is missing, because there is no file {}. vtium draws the font set without that layer.",
                    quoted(&layers.join("gone.vtlayer"))
                ),
            ]
        );
    }

    // ADR-1200 D5, D9: OSC 50 finds a name as the start does: a built-in,
    // then a frit in the frits' directory, then the frit made on the fly
    // for the head's configured font under its own name; a frit in the
    // directory wins over a frit made on the fly under the same name, any
    // other name is refused, and so is a path in no folder vtium searches
    // for fonts
    #[test]
    fn by_name_finds_a_frit_made_on_the_fly_after_the_frits_directory() {
        let dir = Scratch::new("font-by-name");
        std::fs::write(dir.join("t.vtfrit"), "source=kbd:x\n").expect("the frit");
        let sources = Sources {
            frits: dir.to_path_buf(),
            trust: trust().below(&dir),
            ..Sources::default()
        };
        let made = |name: &str| FritText {
            name: name.to_owned(),
            text: "source=vconsole\n".to_owned(),
        };
        let found = |name: &str, made: Option<&FritText>| {
            by_name(&sources, name, false, None, made, &[], &no_paths()).map(
                |chosen| match chosen {
                    Chosen::Frit { name, frit, .. } => (name, frit.text),
                    Chosen::Builtin { name, .. } => {
                        (name.unwrap_or_default().to_owned(), String::new())
                    }
                },
            )
        };
        let vconsole = made("ter-124n");
        let named = |name: &str, text: &str| Ok((name.to_owned(), text.to_owned()));
        let refused = Err("there is no font set of that name".to_owned());
        let results = [
            found("ter-124n", Some(&vconsole)),
            found("ter-124n", None),
            found("ter-v16n", Some(&vconsole)),
            found("t", Some(&made("t"))),
            found("unifont", Some(&made("unifont"))),
            found("/etc/passwd", None),
        ];
        assert_eq!(
            results,
            [
                named("ter-124n", "source=vconsole\n"),
                refused.clone(),
                refused.clone(),
                named("t", "source=kbd:x\n"),
                named("unifont", ""),
                Err(OUTSIDE.to_owned()),
            ]
        );
    }

    /// Sets the permission bits of `path` to `bits`.
    fn chmod(path: &Path, bits: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).expect("the mode");
    }

    /// Writes `text` to the file at `path`.
    fn put(path: &Path, text: &str) {
        std::fs::write(path, text).expect("the file");
    }

    /// A gate whose batch lists nothing, under the rule of the account the
    /// test runs as.
    fn no_paths() -> cullet::Gate {
        cullet::Gate::new(Rule::new(trust()), None, batch::Batch::default())
    }

    /// Why vtium refuses a font path at each of ADR-1200 D5's four checks,
    /// the fourth less the folder and the trust rule's own words.
    const NOT_PLAIN: &str =
        "a font's path must start with `/`, and hold no `..` and no control character";
    const OUTSIDE: &str = "the file is not in a folder that vtium searches for fonts: those of font-paths, the package's font folder and kbd's console-font folders";
    fn inactive() -> String {
        format!(
            "the font is not active, because {} does not list it",
            batch::BATCH_FILE
        )
    }
    fn untrusted(dir: &Path) -> String {
        format!(
            "the folder {} fails the trust rule, because ",
            dir.display()
        )
    }

    /// A font file `fonts/sub/a.ttf` in a scratch folder `name`, the
    /// folders vtium searches for fonts with `fonts` among them, and a gate
    /// whose batch lists the file under the rule of the account the test
    /// runs as, below the scratch folder.
    fn font_path(name: &str) -> (Scratch, PathBuf, Vec<PathBuf>, cullet::Gate) {
        let dir = Scratch::new(name);
        let sub = dir.join("fonts/sub");
        std::fs::create_dir_all(&sub).expect("the folders");
        chmod(&dir, 0o755);
        chmod(&dir.join("fonts"), 0o755);
        chmod(&sub, 0o755);
        let file = sub.join("a.ttf");
        put(&file, "a font");
        chmod(&file, 0o644);
        let folders = vec![PathBuf::from("/usr/share/fonts"), dir.join("fonts")];
        let gate = listing(&dir, &[&file]);
        (dir, file, folders, gate)
    }

    /// A gate whose batch lists `files`, under the rule of the account the
    /// test runs as, with the folders above `dir` left unchecked.
    fn listing(dir: &Path, files: &[&Path]) -> cullet::Gate {
        let entries = files
            .iter()
            .map(|f| batch::Entry::Path(f.to_path_buf()))
            .collect();
        cullet::Gate::new(
            Rule::new(trust()).below(dir),
            Some(Mounts::system().unwrap_or_default()),
            batch::Batch::new(entries),
        )
    }

    // ADR-1200 D5: a font path that passes the four checks is a qualifying
    // font, and OSC 50 takes it as a one-source frit named by the path, as
    // the start makes one for a path that font-name gives.
    #[test]
    fn osc_50_takes_a_font_path_that_passes_every_check() {
        let (dir, file, folders, gate) = font_path("font-path-qualifies");
        let path = file.to_str().unwrap();
        assert_eq!(qualifying(path, &folders, &gate), Ok(()));
        let sources = Sources {
            frits: dir.join("frits"),
            trust: trust().below(&dir),
            ..Sources::default()
        };
        let chosen = by_name(&sources, path, false, None, None, &folders, &gate);
        let (name, text) = match chosen.expect("the font file qualifies") {
            Chosen::Frit { name, frit, .. } => (name, frit.text),
            Chosen::Builtin { .. } => (String::new(), String::new()),
        };
        assert_eq!(name, path, "the name the query reports");
        assert_eq!(text, format!("source=file:{path}\n"));
        let at_start = frit_named(
            &sources,
            &Wanted {
                name: path.to_owned(),
                came: Came::Config,
            },
        )
        .map(|frit| frit.text);
        assert_eq!(at_start, Ok(text), "the frit the start makes for the path");
    }

    // ADR-1200 D5: vtium makes the four checks in the record's order, so a
    // path that fails them all is refused at the first, and each check put
    // right moves the refusal on to the next.
    #[test]
    fn a_font_path_is_refused_at_the_first_check_it_fails() {
        let (dir, file, folders, _) = font_path("font-path-order");
        let open = dir.join("fonts/open");
        std::fs::create_dir_all(&open).expect("the folder");
        chmod(&open, 0o777);
        let inside = open.join("b.ttf");
        put(&inside, "a font");
        let relative = inside.strip_prefix("/").unwrap().to_str().unwrap();
        let elsewhere = &[PathBuf::from("/usr/share/fonts")];
        let none = listing(&dir, &[]);
        let listed = listing(&dir, &[&inside]);
        let refusal = |path: &str, folders: &[PathBuf], gate: &cullet::Gate| {
            qualifying(path, folders, gate).unwrap_err()
        };
        let path = inside.to_str().unwrap();
        assert_eq!(refusal(relative, elsewhere, &none), NOT_PLAIN, "check 1");
        assert_eq!(refusal(path, elsewhere, &none), OUTSIDE, "check 2");
        assert_eq!(refusal(path, &folders, &none), inactive(), "check 3");
        let fourth = refusal(path, &folders, &listed);
        assert!(fourth.starts_with(&untrusted(&open)), "check 4: {fourth}");
        chmod(&open, 0o755);
        assert_eq!(qualifying(path, &folders, &listed), Ok(()), "all four");
        assert_eq!(
            qualifying(file.to_str().unwrap(), &folders, &listing(&dir, &[&file])),
            Ok(())
        );
    }

    // ADR-1200 D5, the first check: a path that does not start with `/`,
    // holds `..` or holds a control character is refused before vtium
    // looks at anything else.
    #[test]
    fn a_font_path_that_is_not_plain_fails_the_first_check() {
        let (dir, file, folders, _) = font_path("font-path-plain");
        let path = file.to_str().unwrap();
        let relative = path.trim_start_matches('/');
        let up = format!("{}/../sub/a.ttf", dir.join("fonts/sub").display());
        let control = format!("{path}\u{7}");
        let gate = listing(
            &dir,
            &[
                Path::new(relative),
                Path::new(&up),
                Path::new(&control),
                Path::new("fonts/sub/a.ttf"),
            ],
        );
        for bad in [relative, &up, &control, "fonts/sub/a.ttf", "./a.ttf"] {
            assert_eq!(
                qualifying(bad, &folders, &gate),
                Err(NOT_PLAIN.to_owned()),
                "{bad:?}"
            );
        }
    }

    // ADR-1200 D5, the second check: the file sits in a folder vtium
    // searches for fonts, or below one, compared by whole components; the
    // folder itself is no font file.
    #[test]
    fn a_font_path_outside_the_folders_vtium_searches_fails_the_second_check() {
        let (dir, _, folders, _) = font_path("font-path-outside");
        let old = dir.join("fonts-old");
        std::fs::create_dir_all(&old).expect("the folder");
        chmod(&old, 0o755);
        let beside = old.join("a.ttf");
        put(&beside, "a font");
        let fonts = dir.join("fonts");
        let gate = listing(&dir, &[&beside, &fonts]);
        for outside in [beside.as_path(), &fonts, Path::new("/etc/passwd")] {
            assert_eq!(
                qualifying(outside.to_str().unwrap(), &folders, &gate),
                Err(OUTSIDE.to_owned()),
                "{}",
                outside.display()
            );
        }
        // a relative folder matches no absolute path
        let relative = [PathBuf::from(fonts.strip_prefix("/").unwrap())];
        let file = fonts.join("sub/a.ttf");
        assert_eq!(
            qualifying(file.to_str().unwrap(), &relative, &listing(&dir, &[&file])),
            Err(OUTSIDE.to_owned())
        );
    }

    // ADR-1200 D5, the third check: a font the batch does not list is not
    // active, whatever folder holds it.
    #[test]
    fn a_font_path_the_batch_does_not_list_fails_the_third_check() {
        let (dir, file, folders, _) = font_path("font-path-inactive");
        let other = dir.join("fonts/sub/b.ttf");
        put(&other, "a font");
        let gate = listing(&dir, &[&other]);
        assert_eq!(
            qualifying(file.to_str().unwrap(), &folders, &gate),
            Err(inactive())
        );
    }

    // ADR-1200 D5, the fourth check, on a symbolic link: a path through a
    // folder that is a symbolic link is refused, though the link's target
    // would pass.
    #[test]
    fn a_font_path_through_a_symbolic_link_fails_the_fourth_check() {
        let (dir, _, folders, _) = font_path("font-path-link");
        let link = dir.join("fonts/link");
        std::os::unix::fs::symlink(dir.join("fonts/sub"), &link).expect("the link");
        let file = link.join("a.ttf");
        let why =
            qualifying(file.to_str().unwrap(), &folders, &listing(&dir, &[&file])).unwrap_err();
        assert_eq!(
            why,
            format!(
                "{}{} is a symbolic link, and vtium uses no resource reached through one.",
                untrusted(&link),
                link.display()
            )
        );
    }

    // ADR-1200 D5, the fourth check, on a folder others can write: anyone
    // could put a file in it, so its fonts are refused.
    #[test]
    fn a_font_in_a_folder_others_can_write_fails_the_fourth_check() {
        let (dir, file, folders, gate) = font_path("font-path-writable");
        let sub = dir.join("fonts/sub");
        chmod(&sub, 0o775);
        let why = qualifying(file.to_str().unwrap(), &folders, &gate).unwrap_err();
        assert!(why.starts_with(&untrusted(&sub)), "{why}");
        assert!(why.contains("chmod go-w"), "{why}");
    }

    // ADR-1200 D5, the fourth check, on a folder a user owns: judged as
    // root, a font in a folder that a user owns, such as one in a home
    // folder, is refused on the folder's owner.
    #[test]
    fn a_font_in_a_folder_a_user_owns_fails_the_fourth_check() {
        let (dir, file, folders, _) = font_path("font-path-owner");
        let me = rustix::process::geteuid().as_raw();
        if me == 0 {
            return;
        }
        let gate = cullet::Gate::new(
            Rule::new(Trust::ROOT).below(&dir),
            Some(Mounts::system().unwrap_or_default()),
            batch::Batch::new(vec![batch::Entry::Path(file.clone())]),
        );
        let why = qualifying(file.to_str().unwrap(), &folders, &gate).unwrap_err();
        assert!(why.starts_with(&untrusted(&dir.join("fonts/sub"))), "{why}");
        assert!(why.contains("chown root:root"), "{why}");
    }

    // ADR-1200 D5, the fourth check, on the filesystem: a folder on a
    // filesystem vtium cannot show stays mounted is refused.
    #[test]
    fn a_font_on_a_filesystem_not_known_to_stay_mounted_fails_the_fourth_check() {
        let (dir, file, folders, _) = font_path("font-path-mounts");
        let gate = cullet::Gate::new(
            Rule::new(trust()).below(&dir),
            Some(Mounts::default()),
            batch::Batch::new(vec![batch::Entry::Path(file.clone())]),
        );
        let why = qualifying(file.to_str().unwrap(), &folders, &gate).unwrap_err();
        assert!(why.starts_with(&untrusted(&dir.join("fonts/sub"))), "{why}");
    }

    // ADR-1200 D5, the second check's folders: font-paths, else the
    // build's list, which vtium-config's font helper searches too, then
    // the package's font folder and kbd's console-font folders; the
    // configuration leaves out a folder of font-paths that is not absolute.
    #[test]
    fn the_folders_a_font_path_must_sit_in_follow_font_paths() {
        assert_eq!(FONT_DIRS, vtium_config::sources::FONT_DIRS);
        let tail = [
            batch::PACKAGE_FONTS,
            "/usr/share/kbd/consolefonts",
            "/usr/lib/kbd/consolefonts",
            "/lib/kbd/consolefonts",
            "/usr/share/consolefonts",
        ];
        let folders = |args: &[&str]| -> Vec<String> {
            font_folders(&config(args))
                .iter()
                .map(|p| p.display().to_string())
                .collect()
        };
        let mut built: Vec<&str> = FONT_DIRS.to_vec();
        built.extend(tail);
        assert_eq!(folders(&[]), built);
        let mut configured = vec!["/opt/fonts", "/srv/f"];
        configured.extend(tail);
        assert_eq!(
            folders(&["--font-paths=/opt/fonts:fonts:/srv/f"]),
            configured
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

    // Each login reads the mount table again, though the batch is the same.
    // A mount ID can be reused after an unmount, so a gate that kept the
    // table of the first login would judge a filesystem mounted later under
    // that ID by the old mount's line, an NFS share as ext4. The login
    // that finds only the mount table changed logs nothing.
    //
    // References:
    // - ADR-1205 D4: the trust rule judges the filesystem a resource sits on.
    #[test]
    fn each_login_judges_by_the_mount_table_read_again() {
        let dir = Scratch::new("batch-mounts");
        let me = rustix::process::geteuid().as_raw();
        let places = batch::Places {
            file: dir.join("batch.conf"),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            palettes: dir.to_path_buf(),
            ..batch::Places::system()
        };
        let trust = Trust::owner(me).below(&dir);
        let sys = Path::new("/nonexistent-sys");
        let ext4 = Mounts::parse("40 22 8:17 / /mnt rw - ext4 /dev/sdb1 rw\n", sys);
        let nfs = Mounts::parse("40 22 0:52 / /mnt rw - nfs4 host:/share rw\n", sys);
        let rule = || Rule::new(trust.clone());
        assert!(load_batch_with(&places, &trust, rule(), ext4.clone()).is_some());
        assert_eq!(gate().mounts.as_ref(), Some(&ext4));
        assert!(
            load_batch_with(&places, &trust, rule(), nfs.clone()).is_none(),
            "the batch is the same, so there is nothing to log"
        );
        assert_eq!(gate().mounts.as_ref(), Some(&nfs));
    }

    // A batch file of more bad lines than vtium lists gives the first
    // ERRORS_MAX as findings and the line that counts the rest, for the log.
    //
    // References:
    // - ADR-1205 D4, its note of 2026-11-19: the console names each batch line it ignores.
    #[test]
    fn a_batch_past_the_error_bound_logs_the_count_of_the_rest() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new("batch-more");
        std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = dir.join("batch.conf");
        std::fs::write(&file, "relative\n".repeat(vtium_settings::ERRORS_MAX + 2)).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let me = rustix::process::geteuid().as_raw();
        let places = batch::Places {
            file: file.clone(),
            vconsole_conf: PathBuf::from("/dev/null"),
            cmdline: PathBuf::from("/dev/null"),
            palettes: dir.to_path_buf(),
            ..batch::Places::system()
        };
        let trust = Trust::owner(me).below(&dir);
        let found = load_batch_with(&places, &trust, Rule::new(trust.clone()), Mounts::default())
            .expect("the first read logs");
        assert_eq!(found.findings.len(), vtium_settings::ERRORS_MAX);
        assert_eq!(
            found.more,
            Some(format!(
                "{}: vtium also ignores 2 more lines of this file. Only the first {} are listed.",
                file.display(),
                vtium_settings::ERRORS_MAX
            ))
        );
    }
}
