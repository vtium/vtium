//! The rules that decide where the console looks for a font file named by
//! its path, which paths it takes, and which kind of source it reads the
//! file with. The console applies them when OSC 50 or `font-name` names a
//! font by its path. vtium-config's Fonts tab applies the same rules
//! before its `t` sends a path, so the two never disagree on a font.
//!
//! The rules read only the path's text and the value of `font-paths`, and
//! open no file. Whether the font is active and passes the trust rule
//! stays with each program, because the console reads its batch and
//! vtium-config reads the marks of its own scan.
//!
//! References:
//! - ADR-1200 D5, "A qualifying font": the first two checks, which these
//!   rules make, and their order.
//! - ADR-1200 D9: a font path takes the source its extension names.
//! - ADR-1200 D25: `font-paths` replaces the system font folders.

use std::path::{Component, Path, PathBuf};

use crate::batch;
use crate::keys;

/// The system font folders, as fontconfig's default configuration names
/// them: where packages install fonts, and where the machine's
/// administrator installs them. The console looks in them for fonts when
/// `font-paths` names no absolute folder.
///
/// References:
/// - ADR-1200 D25: `font-paths` replaces this list.
pub const SYSTEM_FONTS: [&str; 2] = ["/usr/share/fonts", "/usr/local/share/fonts"];

/// Whether `path` is a path the console takes as the text of a font
/// switch. Such a path starts with `/` and is in the form that rebuilding
/// it from its components gives, so it has no `.` component, no `//` and
/// no `/` at its end. It has no `..` component, no control character, no
/// `#`, and no space at either end.
///
/// A path in another form names the same file, but the console's answer
/// to the query would spell it another way. The font set reader ends a
/// line at `#` and trims the spaces around a value, so a path with either
/// would reach the font helper as another file.
pub fn plain(path: &str) -> bool {
    let file = Path::new(path);
    path.starts_with('/')
        && file.components().collect::<PathBuf>().as_os_str() == file.as_os_str()
        && !file.components().any(|c| c == Component::ParentDir)
        && !path.chars().any(char::is_control)
        && !path.contains('#')
        && path.trim() == path
}

/// The folders where the console looks for a font file named by its path,
/// for `font_paths`, the value of the `font-paths` key, or `None` where
/// vtium.conf does not set it. They are the absolute folders that
/// `font_paths` names, separated by `:`, or the [`SYSTEM_FONTS`] when it
/// names none. vtium's own font folder and kbd's console-font folders
/// follow them. Root installs fonts in each of these folders, and a fresh
/// batch already activates the fonts in the last two.
///
/// The console reports each folder of `font-paths` that is not absolute
/// and leaves it out, and it leaves the key unset when no folder remains.
/// So `font-paths = a:b`, `font-paths =` and `font-paths = fonts` all give
/// the system font folders.
pub fn folders(font_paths: Option<&str>) -> Vec<PathBuf> {
    let configured: Vec<PathBuf> = font_paths
        .unwrap_or_default()
        .split(':')
        .filter(|folder| keys::absolute(folder))
        .map(PathBuf::from)
        .collect();
    let configured = if configured.is_empty() {
        SYSTEM_FONTS.iter().map(PathBuf::from).collect()
    } else {
        configured
    };
    configured
        .into_iter()
        .chain(std::iter::once(PathBuf::from(batch::PACKAGE_FONTS)))
        .chain(batch::CONSOLE_FONT_FOLDERS.iter().map(PathBuf::from))
        .collect()
}

/// Whether the file at `path` sits in one of `folders`, or in a folder
/// below one. Whole components are compared, so
/// `/usr/share/fonts-old/x.ttf` is not under `/usr/share/fonts`, and a
/// folder itself is not a file inside it.
pub fn inside(path: &Path, folders: &[PathBuf]) -> bool {
    folders
        .iter()
        .any(|dir| path != dir && path.starts_with(dir))
}

/// The source of the one-source font set that the console makes for the
/// font file at `path`, chosen by the file's extension with case ignored.
/// An OpenType or TrueType file or collection, `.ttf`, `.otf`, `.ttc`,
/// `.otc` or `.otb`, takes `outline:`, a BDF font `bdf:` and a `.hex` font
/// `hex:`. A console font, `.psf` or `.psfu`, and a file of any other name
/// take `file:`, which reads PSF, CPI and raw fonts. A PSF or BDF font's
/// extension may be followed by `.gz`, `.bz2`, `.xz` or `.zst`, and the
/// rule looks past that.
///
/// The rule reads the path's text alone and opens no font file. The font
/// helper then reads the file with the reader of that kind, so a file
/// whose name does not match its contents is refused there.
///
/// References:
/// - ADR-1200 D1: the source forms, and which kinds of font each reads.
/// - ADR-1200 D9: a path becomes a one-source font set.
pub fn path_source(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or_default();
    let name = name.to_ascii_lowercase();
    let mut extensions = name
        .split('.')
        .skip(1)
        .collect::<Vec<&str>>()
        .into_iter()
        .rev();
    let mut extension = extensions.next();
    if extension.is_some_and(|e| ["gz", "bz2", "xz", "zst"].contains(&e)) {
        extension = extensions.next();
    }
    let kind = match extension {
        Some("ttf" | "otf" | "ttc" | "otc" | "otb") => "outline",
        Some("bdf") => "bdf",
        Some("hex") => "hex",
        _ => "file",
    };
    format!("{kind}:{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_path_is_absolute_and_spelled_one_way() {
        assert!(plain("/usr/share/fonts/TTF/DejaVuSansMono.ttf"));
        for path in [
            "usr/share/fonts/a.ttf",
            "/usr/share/fonts/C#Mono.ttf",
            "/usr/share/fonts/TTF/../TTF/a.ttf",
            "/usr/share/fonts/TTF/./a.ttf",
            "/usr/share/fonts/TTF//a.ttf",
            "/usr/share/fonts/TTF/",
            "/usr/share/fonts/a\u{1b}b.ttf",
            "/usr/share/fonts/a.ttf ",
            " /usr/share/fonts/a.ttf",
        ] {
            assert!(!plain(path), "{path:?}");
        }
    }

    // vtium's own font folder can move at build time, so the test takes
    // it from the batch's constant
    const TAIL: [&str; 5] = [
        batch::PACKAGE_FONTS,
        "/usr/share/kbd/consolefonts",
        "/usr/lib/kbd/consolefonts",
        "/lib/kbd/consolefonts",
        "/usr/share/consolefonts",
    ];

    fn shown(folders: Vec<PathBuf>) -> Vec<String> {
        folders.iter().map(|p| p.display().to_string()).collect()
    }

    #[test]
    fn font_paths_replaces_the_system_font_folders() {
        let mut want = vec!["/opt/fonts", "/srv/f"];
        want.extend(TAIL);
        assert_eq!(shown(folders(Some("/opt/fonts:fonts:/srv/f"))), want);
    }

    #[test]
    fn font_paths_with_no_absolute_folder_gives_the_system_font_folders() {
        let mut want = vec!["/usr/share/fonts", "/usr/local/share/fonts"];
        want.extend(TAIL);
        for value in [None, Some("a:b"), Some(""), Some("fonts")] {
            assert_eq!(shown(folders(value)), want, "{value:?}");
        }
    }

    #[test]
    fn a_file_is_inside_a_folder_by_whole_components() {
        let dirs = folders(None);
        assert!(inside(Path::new("/usr/share/fonts/TTF/a.ttf"), &dirs));
        assert!(inside(Path::new("/usr/share/fonts/a.ttf"), &dirs));
        assert!(!inside(Path::new("/usr/share/fonts-old/a.ttf"), &dirs));
        assert!(!inside(Path::new("/usr/share/fonts"), &dirs));
        assert!(!inside(Path::new("/opt/a.ttf"), &dirs));
    }

    #[test]
    fn the_source_follows_the_extension() {
        for (path, kind) in [
            ("/f/a.ttf", "outline"),
            ("/f/a.OTF", "outline"),
            ("/f/a.ttc", "outline"),
            ("/f/a.otc", "outline"),
            ("/f/a.otb", "outline"),
            ("/f/a.bdf", "bdf"),
            ("/f/a.bdf.gz", "bdf"),
            ("/f/a.hex", "hex"),
            ("/f/a.psfu.gz", "file"),
            ("/f/a.psf", "file"),
            ("/f/a", "file"),
            ("/f/a.ttf.d/b", "file"),
        ] {
            assert_eq!(path_source(path), format!("{kind}:{path}"), "{path}");
        }
    }
}
