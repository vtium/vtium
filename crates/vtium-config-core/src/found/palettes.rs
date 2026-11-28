//! The color schemes found. A color scheme sets the terminal's 16 colors
//! and its default foreground and background. vtium builds some schemes
//! in, which are always active and need no file, and reads the others from
//! `NAME.palette` files in the administrator's folder and the package's.
//! When both folders hold a file of one name, vtium reads the
//! administrator's, so the packaged one is marked as hidden.
//!
//! Each file's colors are read when the file is found, whether or not the
//! file is active, through the gate and the strict reader that vtium
//! itself uses. So a preview shows the colors vtium would draw, and the
//! detail shows the error or the refusal vtium would log.
//!
//! References:
//! - ADR-1205 D10: the built-in schemes, the two folders, the administrator's file winning on a name, and the strict reader vtium uses.
//! - ADR-1205 D4, its note of 2026-11-19: one gate for every kind of resource.

use std::path::{Path, PathBuf};

use vtium_settings::palette::{Fault, Reason, SUFFIX};

use super::{Kind, Origin, Places, Resource, counted, files, stem};

/// A color scheme file: its name, which a look's `palette` names, the
/// file with its marks, and its colors, or the first error that makes
/// vtium treat the file as missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub name: String,
    pub resource: Resource,
    pub colors: Result<vtium_term::Palette, String>,
}

/// The color schemes found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Palettes {
    /// The schemes built into vtium, by name, which are always active.
    pub built_in: Vec<&'static str>,
    /// The scheme files: the package's first and then the
    /// administrator's, as the Effects tab lists presets before the
    /// administrator's vtlooks.
    pub files: Vec<Scheme>,
    /// The folders the files were found in, the administrator's first,
    /// which the rule that names the looks using a file reads.
    pub folders: Vec<PathBuf>,
}

impl Palettes {
    /// The scheme file vtium reads for the color scheme `name`: the
    /// administrator's where it hides the package's file of that name.
    pub fn file_named(&self, name: &str) -> Option<&Scheme> {
        self.files
            .iter()
            .find(|f| f.name == name && !f.resource.hidden)
    }
}

/// Why vtium never reads a file whose name a look cannot name.
const BAD_NAME: &str = "The name before .palette is not one a look can name. A color scheme's name holds 1 to 64 of the letters A to Z and a to z, the digits 0 to 9, and the characters . _ + and -.";

/// The colors of the scheme file `file`, named `name`, read as vtium reads
/// them: through a gate whose batch lists only this file, under the trust
/// rule of the user `euid`. So an inactive file still previews, while a
/// file vtium refuses, such as a symbolic link, a FIFO or a file others
/// can write, shows the refusal vtium logs. The gate opens the file
/// without following a link and without waiting on a FIFO, so a file here
/// can never hang the TUI. `Err` holds the first error, file and line
/// first. A file that takes a built-in scheme's name is never read, and a
/// name a look cannot name is an error too.
pub fn colors(name: &str, file: &Path, euid: u32) -> Result<vtium_term::Palette, String> {
    use vtium_settings::batch::{Batch, Entry};
    use vtium_settings::cullet::{Check, Gate, Rule};
    let at = |why: &dyn std::fmt::Display| format!("{}: {why}", file.display());
    let gate = Gate::new(
        Rule::running_as(euid),
        None,
        Batch::new(vec![Entry::Path(file.to_path_buf())]),
    );
    let folder = file.parent().unwrap_or(Path::new("/"));
    match vtium_settings::palette::read(name, &[folder], &gate) {
        Ok(Some(scheme)) => Ok(scheme.palette),
        Ok(None) if !vtium_settings::palette::named(name) => Err(at(&BAD_NAME)),
        Ok(None) => Err(at(&Reason::Refused(Check::Missing))),
        Err(failure) => Err(match failure.faults.first() {
            Some(Fault { reason, .. }) => format!("{}: {reason}", failure.at()),
            None => at(&Reason::NotKeyValue),
        }),
    }
}

/// The color schemes at `places`: the built-in ones, then the files of
/// `places.palettes`, each with its trust mark, its activation's origin
/// from `origin`, and its colors. A file that an earlier folder's file of
/// the same name hides is marked so.
pub(super) fn scan(places: &Places, origin: &dyn Fn(&Path) -> Origin) -> Palettes {
    counted(|s| &mut s.palettes);
    let folders = &places.palettes;
    let mut out = Vec::new();
    for (at, dir) in folders.iter().enumerate().rev() {
        for file in files(dir, SUFFIX) {
            let name = stem(&file, SUFFIX);
            let hidden = folders
                .get(..at)
                .unwrap_or_default()
                .iter()
                .any(|d| d.join(format!("{name}{SUFFIX}")).symlink_metadata().is_ok());
            out.push(Scheme {
                colors: colors(&name, &file, places.euid),
                resource: Resource {
                    trust: crate::batch::trusted(&file, places.euid),
                    origin: origin(&file),
                    path: file,
                    kind: Kind::ColorScheme,
                    own: false,
                    hidden,
                },
                name,
            });
        }
    }
    Palettes {
        built_in: vtium_term::PALETTE_NAMES.to_vec(),
        files: out,
        folders: folders.clone(),
    }
}
