//! The pointer themes found: vtium's own pictures, the package's
//! `pointers.svg`, and each desktop cursor theme in the system's icon
//! folders, which is a folder that holds a `cursors` folder. A cursor
//! theme is activated by its folder, and every file in the folder is
//! active while the batch lists the folder.
//!
//! References:
//! - ADR-1205 D6: the installed pointer themes and their folders.
//! - ADR-1205 D4, its notes of 2026-11-19: a pointer theme is activated by its folder, and vtium's own pictures are active on a fresh install.

use std::path::{Path, PathBuf};

use super::{Kind, Origin, Places, Resource, counted};

/// The system's icon folders, where the desktop's cursor themes are
/// installed.
pub const ICONS: [&str; 2] = ["/usr/share/icons", "/usr/local/share/icons"];

/// Where the pointer themes are found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folders {
    /// vtium's own pictures.
    pub svg: PathBuf,
    /// The icon folders, each read for cursor themes.
    pub icons: Vec<PathBuf>,
}

impl Folders {
    /// This machine's: the package's pointer file and [`ICONS`].
    pub fn system() -> Folders {
        Folders {
            svg: PathBuf::from(vtium_settings::batch::PACKAGE_POINTERS),
            icons: ICONS.iter().map(PathBuf::from).collect(),
        }
    }
}

/// Whether the theme at `path` is a cursor theme's folder: vtium reads a
/// pointer file as SVG by its suffix, and any other path as a folder.
pub fn is_folder(path: &Path) -> bool {
    path.extension().is_none_or(|e| e != "svg")
}

/// The cursor themes in `dir`, by path: each folder in it that holds a
/// `cursors` folder. A link to a folder is not listed.
fn themes(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) && p.join("cursors").is_dir()
        })
        .collect();
    out.sort();
    out
}

/// The pointer themes at `places`: vtium's own pictures, where they are
/// installed, then the cursor themes of each icon folder, each with its
/// trust mark and its activation's origin from `origin`.
pub(super) fn scan(places: &Places, origin: &dyn Fn(&Path) -> Origin) -> Vec<Resource> {
    counted(|s| &mut s.pointers);
    let folders = &places.pointers;
    let own = folders
        .svg
        .symlink_metadata()
        .is_ok()
        .then(|| folders.svg.clone());
    own.into_iter()
        .chain(folders.icons.iter().flat_map(|d| themes(d)))
        .map(|path| Resource {
            trust: crate::batch::trusted(&path, places.euid),
            origin: origin(&path),
            path,
            kind: Kind::PointerTheme,
            own: false,
            hidden: false,
        })
        .collect()
}
