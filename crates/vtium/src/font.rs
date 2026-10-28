//! The font: which set vtium draws, its faces at each height, its zoom
//! steps and the automatic size, with the lookup's fallback.
//!
//! A configured font that names a frit (`NAME.vtfrit` in the frits'
//! directory) is generated at run time (ADR-1201 D1): the built-in font
//! for the cell size draws at once, and the helper's glyphs replace it,
//! character by character, as they arrive ([`Chosen::Frit`],
//! [`crate::deferred`], [`crate::helper`]). Any other configured font
//! names a vitrified set (ADR-1200), which is loaded from the root-owned
//! store after the store's check, or vtium draws its built-in sets and
//! logs the command that would build the missing one ([`wanted`],
//! [`load`], D9). The built-ins
//! are Unscii-8 for cells under 16 pixels and unifont from 16, but in a
//! zh, ja or ko locale, which Unscii-8 does not cover (D7). OSC 50
//! switches among the built sets while vtium runs (D5).
//!
//! A font is looked up at the size asked for. When none is in use yet, a
//! failure tries [`FALLBACK_SIZE`] once, and a second failure is the
//! error; with a font in use, as at a zoom, a failure is the error and
//! that font stays. The C reference retried at 16 for as long as the
//! lookup failed (`src/terminal.c:921-939` at kmscon b1f7a7c5,
//! aaronsb/kmscon#63).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use vtium_font::set::{Set, SetFace, Store, builtin, builtin_name};
use vtium_font::sprite::Range;
use vtium_font::{Face, Missing, Source, SpritePolicy, Unifont};

use crate::config::keys::{
    FONT_ENGINE, FONT_NAME, GLYPH_SOURCE, GLYPH_SOURCE_BLOCK, GLYPH_SOURCE_BOX,
    GLYPH_SOURCE_BRAILLE, GLYPH_SOURCE_LEGACY, GLYPH_SOURCE_OCTANTS, GLYPH_SOURCE_SEXTANTS, Key,
    MISSING,
};
use crate::config::{Config, Origin};
use crate::deferred::{DeferredFace, Shared};
use crate::helper;
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

/// The store's directory, set at build time with `VTIUM_FONT_STORE` as
/// vtium-vitrify's is.
pub fn default_store() -> PathBuf {
    PathBuf::from(option_env!("VTIUM_FONT_STORE").unwrap_or("/var/lib/vtium/fonts"))
}

/// The frits' directory, set at build time with `VTIUM_FONT_FRITS` as
/// vtium-vitrify's is.
pub fn default_frits() -> PathBuf {
    PathBuf::from(option_env!("VTIUM_FONT_FRITS").unwrap_or("/etc/vtium/fonts"))
}

/// The frit `name` names in `frits`: `NAME.vtfrit`, a regular file, for a
/// name of 1 to 64 characters of `A-Z a-z 0-9 . _ + -` not starting with
/// `.` (ADR-1200 D1), so no name reaches outside the directory.
pub fn frit(frits: &Path, name: &str) -> Option<PathBuf> {
    let valid = (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'));
    let path = frits.join(format!("{name}.vtfrit"));
    (valid && std::fs::metadata(&path).is_ok_and(|m| m.is_file())).then_some(path)
}

/// Where vtium finds sets and frits: the store and the owner its check
/// expects, the frits' directory, the helper that generates a frit's
/// glyphs, and the files that name the font when the configuration does
/// not.
#[derive(Clone, Debug)]
pub struct Sources {
    pub store: PathBuf,
    pub uid: u32,
    pub frits: PathBuf,
    /// The helper; `None` for the system's, [`helper::Command::system`].
    pub helper: Option<helper::Command>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
}

impl Default for Sources {
    /// The system's: the store owned by root, the frits' directory, the
    /// installed helper, `/etc/vconsole.conf` and `/proc/cmdline`.
    fn default() -> Sources {
        Sources {
            store: default_store(),
            uid: 0,
            frits: default_frits(),
            helper: None,
            vconsole_conf: PathBuf::from("/etc/vconsole.conf"),
            kernel_cmdline: PathBuf::from("/proc/cmdline"),
        }
    }
}

/// Where the font's name came from, which picks the command that builds
/// its set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Came {
    /// `font-name`: a path, or a kbd font's name.
    Config,
    /// `font-name` under `font-engine=freetype` or `pango`: an outline
    /// font's family, which only a frit names.
    Family,
    /// `vconsole.font=` on the kernel command line, or `FONT=` in
    /// vconsole.conf.
    Vconsole,
}

/// `word` quoted for a shell where it needs it: in single quotes, each
/// quote in it closed, escaped and opened again.
fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._+-=:,@".contains(&b));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// The font a name asks for, and where the name came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub name: String,
    pub came: Came,
}

impl Wanted {
    /// How a set built from the font is spelled as its frit's first
    /// source, as `vtium-vitrify create` writes the one-source frit; none
    /// for a family, whose frit names its file.
    fn source(&self) -> Option<String> {
        match self.came {
            Came::Vconsole => Some("vconsole".to_owned()),
            Came::Config if self.name.starts_with('/') => Some(format!("file:{}", self.name)),
            Came::Config => Some(format!("kbd:{}", self.name)),
            Came::Family => None,
        }
    }

    /// How to build the set: the command, `--force` rebuilding one the
    /// store holds, each word quoted for a shell.
    pub fn command(&self, force: bool) -> String {
        let force = if force { "--force " } else { "" };
        let name = quote(&self.name);
        match self.came {
            Came::Vconsole => format!("sudo vtium-vitrify create {force}--vconsole"),
            Came::Config if self.name.starts_with('/') => {
                format!("sudo vtium-vitrify create {force}--file {name}")
            }
            Came::Config => format!("sudo vtium-vitrify create {force}--kbd {name}"),
            Came::Family => format!(
                "a frit naming the file of {name}, `source=outline:PATH` (outline sources come with vtium/vtium#23), built with sudo vtium-vitrify create {force}FRIT, and font-name set to the set's name"
            ),
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
/// and vtium draws its built-in sets.
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

/// The set `wanted` finds in the store at `sources`, looked up by set name
/// and then by its first source as the frit spelled it (ADR-1200 D9), and
/// checked as D3 checks it: the set's name, and the set. The error is the
/// reason and whether the store holds the set, which then needs a rebuild.
pub fn load(sources: &Sources, wanted: &Wanted) -> Result<(String, Arc<Set>), (String, bool)> {
    let store = Store::open(&sources.store, sources.uid).map_err(|e| (e.to_string(), false))?;
    let manifest = store.manifest().map_err(|e| (e.to_string(), false))?;
    let source = wanted.source();
    let row = manifest
        .row(&wanted.name)
        .or_else(|| {
            manifest
                .rows()
                .iter()
                .find(|r| Some(&r.source) == source.as_ref())
        })
        .ok_or_else(|| ("the store holds no set built from it".to_owned(), false))?;
    let set = store.load_row(row).map_err(|e| (e.to_string(), true))?;
    Ok((row.name.clone(), set))
}

/// The set named `name` for OSC 50 (ADR-1200 D5): a built-in, or a set the
/// store's manifest names, checked as D3 checks it. The set keeps the name
/// as the built-in or the manifest spells it, never the request's bytes.
pub fn by_name(sources: &Sources, name: &str) -> Result<Chosen, String> {
    if let Some(builtin) = ["unifont", "unscii-8"].into_iter().find(|b| *b == name) {
        return Ok(Chosen::Builtin(Some(builtin)));
    }
    let store = Store::open(&sources.store, sources.uid).map_err(|e| e.to_string())?;
    let manifest = store.manifest().map_err(|e| e.to_string())?;
    let row = manifest
        .row(name)
        .ok_or_else(|| "the store holds no set of that name".to_owned())?;
    let set = store.load_row(row).map_err(|e| e.to_string())?;
    Ok(Chosen::Set {
        name: row.name.clone(),
        set,
    })
}

/// The font vtium draws.
#[derive(Clone, Debug)]
pub enum Chosen {
    /// The built-in sets: the one named, or with `None`, Unscii-8 for
    /// cells under 16 pixels and unifont from 16, but unifont alone in a
    /// zh, ja or ko locale (ADR-1200 D7).
    Builtin(Option<&'static str>),
    /// A set from the store.
    Set { name: String, set: Arc<Set> },
    /// A frit, generated at run time (ADR-1201 D1): drawn as
    /// `Builtin(None)` is, each character the helper has sent drawn from
    /// `generated` in its place.
    Frit {
        name: String,
        path: PathBuf,
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
    /// The built-in set drawn at `height` in `locale`.
    fn builtin_at(&self, height: u32, locale: &str) -> Option<&'static str> {
        match self {
            Chosen::Builtin(Some(name)) => Some(name),
            Chosen::Builtin(None) | Chosen::Frit { .. } => Some(builtin_name(height, locale)),
            Chosen::Set { .. } => None,
        }
    }

    /// The set's name at `height` in `locale`, as `vtium-vitrify list`
    /// and OSC 50 spell it.
    pub fn name(&self, height: u32, locale: &str) -> &str {
        match self {
            Chosen::Set { name, .. } | Chosen::Frit { name, .. } => name,
            _ => self.builtin_at(height, locale).unwrap_or("unifont"),
        }
    }

    /// The face at `height`: the strike of that height, else one whose
    /// whole multiple it is, else the tallest under it scaled by the
    /// largest whole factor that fits (ADR-1200 D9). The built-in unifont
    /// draws through its own face.
    pub fn face(&self, height: u32, locale: &str) -> Option<Box<dyn Face>> {
        if let Chosen::Frit { generated, .. } = self {
            let builtin = Chosen::Builtin(None).face(height, locale)?;
            return Some(Box::new(DeferredFace::new(builtin, Rc::clone(generated))));
        }
        let set = match (self, self.builtin_at(height, locale)) {
            (_, Some("unifont")) => return Some(Box::new(Unifont::new(height))),
            (_, Some(name)) => builtin(name)?,
            (Chosen::Set { set, .. }, None) => set.clone(),
            (Chosen::Builtin(_) | Chosen::Frit { .. }, None) => return None,
        };
        SetFace::new(set, height).map(|f| Box::new(f) as Box<dyn Face>)
    }

    /// The heights a zoom steps through: each strike's height and its
    /// whole multiples. The built-ins step through Unscii-8's 8 under 16,
    /// where it is drawn, and unifont's multiples of 16 from 16.
    pub fn rungs(&self, locale: &str) -> Vec<Rung> {
        let all = |step| Rung {
            step,
            min: step,
            end: u32::MAX,
        };
        match self {
            Chosen::Set { set, .. } => set.cells().iter().map(|c| all(c.height)).collect(),
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

    /// The smallest height the font draws.
    pub fn smallest(&self, locale: &str) -> u32 {
        self.rungs(locale)
            .iter()
            .map(|r| r.min)
            .min()
            .unwrap_or(FALLBACK_SIZE)
    }
}

/// The height a zoom asks for from `size`, the next of `rungs` in its
/// direction; `None` past 150 in, or below the smallest out.
pub fn zoom(rungs: &[Rung], size: u32, dir: Zoom) -> Option<u32> {
    let heights = rungs.iter().filter(|r| r.step > 0);
    match dir {
        Zoom::In if size > 150 => None,
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
        assert_eq!(named.source(), Some(format!("file:{path}")));
        assert_eq!(
            named.command(false),
            format!("sudo vtium-vitrify create --file {path}")
        );
        let kbd = wanted(&config(&["--font-name=Lat2-Terminus16"]), "", "").expect("named");
        assert_eq!(kbd.source().as_deref(), Some("kbd:Lat2-Terminus16"));
        assert_eq!(
            kbd.command(true),
            "sudo vtium-vitrify create --force --kbd Lat2-Terminus16"
        );
        let v = wanted(&config(&[]), conf, "").expect("vconsole");
        assert_eq!(v.source().as_deref(), Some("vconsole"));
        assert_eq!(v.command(false), "sudo vtium-vitrify create --vconsole");
        // An outline font's family under freetype or pango: a frit names
        // its file, and the suggestion quotes the family.
        let family = wanted(
            &config(&["--font-engine=pango", "--font-name=DejaVu Sans Mono"]),
            "",
            "",
        )
        .expect("named");
        assert_eq!(family.came, Came::Family);
        assert_eq!(family.source(), None);
        let says = family.command(false);
        assert!(says.contains("of 'DejaVu Sans Mono'"), "{says}");
        assert!(says.contains("source=outline:PATH"), "{says}");
        assert!(!says.contains("--kbd"), "{says}");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(
            wanted(&config(&["--font-name=/fonts/my font.psf"]), "", "")
                .expect("named")
                .command(false),
            "sudo vtium-vitrify create --file '/fonts/my font.psf'"
        );
        // `font-engine=unifont` or `8x16` draws the built-ins.
        for engine in ["unifont", "8x16"] {
            let c = config(&[&format!("--font-engine={engine}"), "--font-name=x"]);
            assert_eq!(wanted(&c, conf, ""), None, "{engine}");
        }
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
