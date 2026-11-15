//! The pointer pictures the pointer file screens draw with, made as the
//! console makes them from a pointer file.
//!
//! The `pointer-file-` screens draw the pointer from a pointer file
//! instead of from vtium's compiled-in pictures. Each picture goes the way
//! a picture from the font helper goes: the helper's own code draws it,
//! the SVG drawing for an SVG file and the Xcursor reader for a cursor
//! theme's file, and [`Arrow::from_rgba`] checks it as the console checks
//! the helper's answer. The renderer then holds the pictures in a
//! [`PointerPictures`] table and picks the one its cells need.
//!
//! Two files serve the screens. vtium's own `pointers.svg` comes from this
//! tree, as vtium installs it. Adwaita's `default` cursor is a third
//! party's file, so tier 2 pins it by its URL and SHA-256, fetches it into
//! a cache outside the tree, `$XDG_CACHE_HOME/vtium/tier2/pointers`, and
//! never commits it. `make tier2-fonts` fetches it with the fonts.
//!
//! References:
//! - ADR-1309, "Verification": the twelve screens and the pinned Adwaita file, its URL and its SHA-256.
//! - ADR-1503 D9: a test input that vtium does not ship is pinned by its URL and SHA-256 and fetched into a cache.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vtium_render::{Arrow, PictureSizes, PointerPictures};
use vtium_term::PointerShape;

/// vtium's own pointer file, as `make install` installs it to
/// `/usr/share/vtium/pointers.svg`.
const SHIPPED: &[u8] = include_bytes!("../../../crates/vtium/install/pointers.svg");

/// Adwaita 51.0's `default` cursor file, pinned: the name tier 2 keeps it
/// under in its cache, its URL in GNOME's repository at the tag `51.0`,
/// and the SHA-256 of its bytes.
pub const ADWAITA_DEFAULT: (&str, &str, &str) = (
    "adwaita-51.0-default",
    "https://gitlab.gnome.org/GNOME/adwaita-icon-theme/-/raw/51.0/Adwaita/cursors/default",
    "47f579498d72b12d828aa40e7e484c5ebd6b75ae04293cf5c96532c21c3e1594",
);

/// The nominal size the Adwaita screen asks for, as `pointer-size=24`
/// asks for it.
pub const ADWAITA_SIZE: u32 = 24;

/// The pointer file a screen draws its pointer from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerFile {
    /// vtium's own `pointers.svg`, each of its ten shapes drawn for the
    /// screen's cell height.
    Shipped,
    /// Adwaita's `default` cursor at nominal size [`ADWAITA_SIZE`], as the
    /// default shape, with the nine other shapes left to vtium's own
    /// pictures.
    Adwaita,
}

/// The cache the pinned pointer files are fetched into and read from:
/// `pointers` beside the outline fonts' cache.
pub fn cache_dir() -> PathBuf {
    crate::fonts::cache_dir().with_file_name("pointers")
}

/// Fetches the pinned pointer files into `dir` when `dir` lacks them or
/// holds other bytes, and checks each against its SHA-256. A file that
/// does not match is removed, and the fetch fails naming it.
pub fn fetch(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (file, url, sha256) = ADWAITA_DEFAULT;
    if crate::fonts::fault(dir, file, sha256).is_some() {
        crate::fonts::download(dir, file, url)?;
    }
    if let Some(why) = crate::fonts::fault(dir, file, sha256) {
        let _ = std::fs::remove_file(dir.join(file));
        return Err(why);
    }
    Ok(())
}

/// The bytes of Adwaita's pinned `default` file in `dir`, or why they
/// cannot be read, with the command that fetches them.
fn adwaita(dir: &Path) -> Result<Vec<u8>, String> {
    let (file, _, sha256) = ADWAITA_DEFAULT;
    if let Some(why) = crate::fonts::fault(dir, file, sha256) {
        return Err(format!(
            "{why}; `make tier2-fonts` fetches the pinned pointer file"
        ));
    }
    std::fs::read(dir.join(file)).map_err(|e| format!("{file}: {e}"))
}

/// Checks a picture from the helper's code as the console checks the
/// helper's answer, and keeps it in `table` as `shape`'s picture.
fn keep(
    table: &mut PointerPictures,
    shape: PointerShape,
    p: &vtium_config::serve::PointerPicture,
) -> Result<(), String> {
    let arrow = Arrow::from_rgba(p.width, p.height, p.hot, &p.rgba)
        .map_err(|e| format!("the {} shape: {e}", shape.name()))?;
    table.insert(shape, p.size, arrow);
    Ok(())
}

/// The pictures of `file` for a renderer whose cells are `cell_height`
/// pixels tall.
pub fn pictures(file: PointerFile, cell_height: u32) -> Result<Arc<PointerPictures>, String> {
    match file {
        PointerFile::Shipped => {
            let mut table = PointerPictures::new(PictureSizes::Svg);
            for shape in PointerShape::ALL {
                let p = vtium_config::pointer_svg::draw(SHIPPED, shape.name(), cell_height)
                    .map_err(|e| format!("pointers.svg has no {} shape: {e:?}", shape.name()))?;
                keep(&mut table, shape, &p)?;
            }
            Ok(Arc::new(table))
        }
        PointerFile::Adwaita => {
            let bytes = adwaita(&cache_dir())?;
            let kept = vtium_config::xcursor::read(&bytes, Some(ADWAITA_SIZE))
                .map_err(|e| format!("Adwaita's default cursor fails, because {e}"))?;
            let mut table = PointerPictures::new(PictureSizes::Theme {
                size: Some(ADWAITA_SIZE),
            });
            for p in &kept {
                keep(&mut table, PointerShape::Default, p)?;
            }
            if table.sizes_of(PointerShape::Default) != [ADWAITA_SIZE] {
                return Err(format!(
                    "Adwaita's default cursor kept the sizes {:?}, not {ADWAITA_SIZE} alone",
                    table.sizes_of(PointerShape::Default)
                ));
            }
            Ok(Arc::new(table))
        }
    }
}
