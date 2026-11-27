//! A frit, vtium's font set file: its name and where vtium finds it, the
//! ownership check it passes before vtium reads it, its layers, and the
//! lines it states, `cell=`, `size=` and `line-spacing=`, with the
//! warnings about them.

use std::path::{Path, PathBuf};

use vtium_font::set::{layer, scope};
use vtium_font::spacing::LINE_SPACING_MAX;
use vtium_font::{CELL_SIDE_MAX, Cell};
use vtium_settings::Trust;

use crate::config::Config;
use crate::config::keys::FONT_FRITS;

use super::{FRIT_CELL_MAX, FRIT_CELL_MIN};

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
pub(super) fn frit_name(name: &str) -> bool {
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
/// line that adds nothing and for each line of a placed layer that vtium
/// leaves out. vtium reads each layer from the layers' folder
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
/// - ADR-1510 D2: a layer's unknown or misplaced key is reported with the
///   layer's file and line, and ignored alone.
pub fn with_layers(name: &str, frits: &Path, text: &str, trust: &Trust) -> (String, Vec<String>) {
    let dir = layers_dir(frits);
    let mut read = |layer: &str| trusted_text(&layer_file(&dir, layer), trust);
    let expanded = layer::expand(text, &mut read);
    let notes = expanded
        .notes
        .iter()
        .map(|note| layer_note(name, &dir, note))
        .chain(
            expanded
                .ignored
                .iter()
                .map(|ignored| layer_ignored(name, &dir, ignored)),
        )
        .collect();
    (expanded.text, notes)
}

/// The sentence that says vtium leaves out a line of a layer of the frit
/// `name`, whose layers are in `dir`, because the layer cannot hold that
/// line: a key vtium does not know, a key of the whole set, or a source's
/// key with no source above it. The rest of the layer and of the set
/// still loads. The sentence names the layer's file and the line in it,
/// as `vtium-config check` does.
fn layer_ignored(name: &str, dir: &Path, ignored: &layer::IgnoredIn) -> String {
    let layer::IgnoredIn { layer, ignored, .. } = ignored;
    format!(
        "font set {name}: line {} of the layer {layer} in {}: {} vtium ignores this line and reads the rest of the font set.",
        ignored.line,
        quoted(&layer_file(dir, layer)),
        ignored.why
    )
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
pub(super) fn frit_line<'a>(text: &'a str, key: &str) -> Result<Option<(usize, &'a str)>, String> {
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
pub(super) fn points_under(pixels: u32, dpi: f64) -> u32 {
    let points = (f64::from(pixels) * POINTS_PER_INCH / dpi).floor();
    points.clamp(0.0, f64::from(u32::MAX)) as u32
}

/// Whether a frit's `text` names an outline source, `outline:` or a
/// family, `font:`, so a zoom steps it by 1 pixel (ADR-1200 D17).
pub(super) fn frit_outline(text: &str) -> bool {
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
/// family, and a `strike=` line describes that source. Such a source is
/// one font at several heights, so its cell changes with the height it
/// draws at.
///
/// A `strike=` line describes the first source when it follows that
/// source's line and no `layer=` line comes between them. A first source
/// that is a built-in font (`builtin:`) or the system's console font
/// (`vconsole` or `console:`) takes no strikes. The font helper ignores a
/// `strike=` line in either case, so the console does not count one
/// either. Both ask vtium-font's `scope` module, so both read the same
/// set.
///
/// This asks whether a zoom asks the helper for bitmap strikes, which is
/// narrower than whether the helper keeps a `strike=` line: the helper
/// keeps one under an `outline:` or `font:` source too.
///
/// References:
/// - ADR-1204 D2: the `strike=` key, and the strike each height draws.
/// - ADR-1510 D2: the helper ignores a key where it does not apply, and reads the rest of the set.
pub(super) fn frit_struck(text: &str) -> bool {
    let mut current = scope::CurrentSource::default();
    let mut first = true;
    for line in text.split('\n') {
        let Some((k, v)) = line.split('#').next().and_then(|body| body.split_once('=')) else {
            continue;
        };
        let (key, value) = (k.trim(), v.trim());
        let describes = current.read(key, value);
        match key {
            // only the first source counts
            "source" if !first => return false,
            "source" if scope::may_be_outline(value) || !scope::takes_strikes(value) => {
                return false;
            }
            "source" => first = false,
            "strike" if matches!(describes, scope::Describes::Source(_)) => return true,
            _ => {}
        }
    }
    false
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
pub(super) fn frit_cell_at(stated: Option<Cell>, height: u32, builtin: Cell) -> Cell {
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
