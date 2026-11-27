//! The font a configuration names, and the frit, built-in font or font
//! file vtium finds for a name, as at the start and at OSC 50.

use std::path::{Path, PathBuf};

use vtium_font::Cell;
use vtium_settings::batch;
use vtium_settings::cullet::{self, Rule};
use vtium_settings::fontpath::{self, path_source};
use vtium_settings::mounts::Mounts;

use crate::config::keys::{FONT_ENGINE, FONT_NAME, FONT_PATHS};
use crate::config::{Config, Origin};
use crate::helper::FritText;

use super::frits::frit_line;
use super::{Came, Chosen, DEFAULT_SET, Sources, Wanted, frit};

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
                one_source(name)?
            }
            None => return Err("there is no font set of that name".to_owned()),
        },
    };
    Chosen::frit(frit, sized, look)
}

/// The folders where installed outline fonts are found when `font-paths`
/// is unset, as fontconfig's default configuration names them for the
/// system. vtium-settings holds the list, and vtium-config's font helper
/// searches the same folders.
///
/// References:
/// - ADR-1200 D25: `font-paths` replaces this list.
pub const FONT_DIRS: [&str; 2] = fontpath::SYSTEM_FONTS;

/// The folders a font that OSC 50 names by its path must sit in, or below.
/// They are the folders of `font-paths`, else [`FONT_DIRS`], then the
/// package's font folder and kbd's console-font folders. Root installs
/// fonts in each of them, and a fresh batch already activates the fonts in
/// the last two. The configuration has already left out each folder of
/// `font-paths` that is not an absolute path. vtium-settings holds the
/// rule, so vtium-config's Fonts tab searches the same folders.
///
/// References:
/// - ADR-1200 D5, "A qualifying font": the second check and its folders.
pub fn font_folders(config: &Config) -> Vec<PathBuf> {
    fontpath::folders(config.text(&FONT_PATHS))
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

/// The one-source frit vtium makes for the font file at `path`, named by
/// the path, as the start makes it for a path that `font-name` gives.
/// vtium then reads the frit's `source=` line back in the frit's line
/// syntax, as the font helper reads it: the line ends at `#`, and the
/// value loses the spaces at either end. The frit is used only when that
/// value names the same path. [`qualifying`]'s first check already
/// refuses every path the reader would change, so this is a second line
/// of defence. If the two rules ever differ, vtium refuses the font
/// instead of checking one file while the helper reads another.
///
/// References:
/// - ADR-1200 D5: vtium builds a frit with the one source, named by the
///   path, as it does at the start for a path.
pub(super) fn one_source(path: &str) -> Result<FritText, String> {
    let frit = Wanted {
        name: path.to_owned(),
        came: Came::Config,
    }
    .on_the_fly();
    let source = path_source(path);
    match frit_line(&frit.text, "source") {
        Ok(Some((1, read))) if read == source => Ok(frit),
        _ => Err("the font set vtium would make for this path would name another file".to_owned()),
    }
}

/// Whether `path`, the text OSC 50 carries, names a qualifying font: one
/// that root already installed and trusts. vtium makes four checks, in
/// this order, and the error says which one failed:
///
/// 1. The path is plain: it starts with `/`, is in its plain form and has
///    no `..` component. It holds no control character and no `#`, and
///    has no whitespace at either end. A path in another form, such as
///    one with a `/` at its end, names the same file as the batch's line,
///    but the query would report that other spelling. The frit reader
///    ends a line at `#` and trims the spaces around a value, so a path
///    with either would reach the font helper as another file.
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
    if !fontpath::plain(path) {
        return Err("a font's path must be absolute and plain: no `.`, `..` or empty component, no `/` at its end, no control character, no `#`, and no space at either end".to_owned());
    }
    if !fontpath::inside(file, folders) {
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
