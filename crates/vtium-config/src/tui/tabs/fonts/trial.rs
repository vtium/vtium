//! Whether the console can take a font file that the Fonts tab's `t`
//! tries. The console switches to a single font file only when root
//! already installed and trusts it, so vtium-config makes the console's
//! checks first and sends nothing for a font that would fail one. The
//! console makes the same checks again when it gets the path, so these
//! checks only save the person a trial that the console would refuse.
//!
//! vtium-config checks the installed vtium.conf, which the console runs
//! with, and the marks that the scan gives each font: its folder, its
//! place in the batch, and its trust mark.
//!
//! References:
//! - ADR-1200 D5, "A qualifying font": the console's four checks, in the
//!   order the console makes them.
//! - ADR-1200 D5, "Trying one font from vtium-config's Fonts tab": `t`
//!   makes the checks before it sends anything, and says why on a font
//!   that fails one.

use std::path::{Component, Path, PathBuf};

use crate::batch::State;
use crate::discover::{LOCAL_FONTS, PACKAGE_FONTS, font_paths};
use crate::onscreen::text;
use crate::tui::fonts_found::FontFile;
use crate::tui::words;

/// The folders where the console looks for a font that OSC 50 names by
/// its path, for `conf`, the installed vtium.conf's text: the folders of
/// `font-paths` where vtium.conf sets it, else the system's two font
/// folders, then vtium's own font folder and kbd's console-font folders.
/// A font may sit in one of them or in a folder below one.
fn searched(conf: Option<&str>) -> Vec<PathBuf> {
    let set = conf
        .and_then(|c| vtium_settings::setting(c, vtium_settings::keys::FONT_PATHS.name))
        .is_some();
    let configured = if set {
        font_paths(conf)
    } else {
        vec![PathBuf::from(PACKAGE_FONTS), PathBuf::from(LOCAL_FONTS)]
    };
    configured
        .into_iter()
        .chain(std::iter::once(PathBuf::from(
            vtium_settings::batch::PACKAGE_FONTS,
        )))
        .chain(
            vtium_settings::batch::CONSOLE_FONT_FOLDERS
                .iter()
                .map(PathBuf::from),
        )
        .collect()
}

/// Whether `path` is a path the console takes as the text of a font
/// switch: absolute, in the form that rebuilding it from its components
/// gives, with no `..` component, no control character, no `#`, and no
/// space at either end. A path in another form names the same file, but
/// the console's answer to the query would spell it another way.
fn plain(path: &str) -> bool {
    let file = Path::new(path);
    path.starts_with('/')
        && file.components().collect::<PathBuf>().as_os_str() == file.as_os_str()
        && !file.components().any(|c| c == Component::ParentDir)
        && !path.chars().any(char::is_control)
        && !path.contains('#')
        && path.trim() == path
}

/// The path that `t` sends the console for `file`, or the sentence that
/// says why the console would not take it. `conf` is the installed
/// vtium.conf's text. vtium-config makes the console's checks in the
/// console's order: the path is plain, the file sits in a folder where the
/// console looks for fonts, the font is active, and it passes the trust
/// rule. A font in the person's own folder fails the second check, and
/// the sentence then says how to install it as a system font.
pub fn path_to_try(conf: Option<&str>, file: &FontFile) -> Result<String, String> {
    let name = text(&file.resource.name());
    if file.resource.own {
        return Err(words::try_own_font(&name));
    }
    let Some(path) = file.path().to_str().filter(|p| plain(p)) else {
        return Err(words::try_unplain(&name));
    };
    let inside = searched(conf)
        .iter()
        .any(|dir| file.path() != dir && file.path().starts_with(dir));
    if !inside {
        return Err(words::try_outside_folders(&name));
    }
    match file.resource.state {
        State::Inactive => return Err(words::try_inactive(&name)),
        State::Activates => return Err(words::try_activates(&name)),
        State::Always | State::Active | State::Deactivates => {}
    }
    if let Err(why) = &file.resource.trust {
        return Err(words::try_untrusted(&name, text(why)));
    }
    Ok(path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::{Folder, Found, Place};

    /// A font file found at `path` in a system folder, in the batch as
    /// `state`, with the trust mark `trust`.
    fn file(path: &str, state: State, trust: Result<(), String>) -> FontFile {
        FontFile::of(Found {
            path: PathBuf::from(path),
            folder: Folder {
                path: PathBuf::from("/usr/share/fonts"),
                place: Place::System,
            },
            trust,
            state,
            duplicate: None,
        })
    }

    const DEJAVU: &str = "/usr/share/fonts/TTF/DejaVuSansMono.ttf";

    #[test]
    fn an_active_trusted_font_in_a_system_folder_is_sent_by_its_path() {
        let f = file(DEJAVU, State::Active, Ok(()));
        assert_eq!(path_to_try(None, &f), Ok(DEJAVU.to_owned()));
        // the draft's deactivation takes effect only once it is installed,
        // so the console still holds the font in its batch
        let f = file(DEJAVU, State::Deactivates, Ok(()));
        assert_eq!(path_to_try(None, &f), Ok(DEJAVU.to_owned()));
    }

    #[test]
    fn a_path_the_console_would_spell_another_way_is_not_sent() {
        for path in [
            "/usr/share/fonts/C#Mono.ttf",
            "/usr/share/fonts/TTF/../TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/./DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF//DejaVuSansMono.ttf",
            "/usr/share/fonts/a\u{1b}b.ttf",
            "/usr/share/fonts/a.ttf ",
        ] {
            let f = file(path, State::Active, Ok(()));
            let name = text(&f.resource.name());
            assert_eq!(
                path_to_try(None, &f),
                Err(words::try_unplain(name)),
                "{path:?}"
            );
        }
    }

    #[test]
    fn a_path_that_is_not_utf_8_is_not_sent() {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::OsStr::from_bytes(b"/usr/share/fonts/\xff.ttf");
        let f = FontFile::of(Found {
            path: PathBuf::from(path),
            folder: Folder {
                path: PathBuf::from("/usr/share/fonts"),
                place: Place::System,
            },
            trust: Ok(()),
            state: State::Active,
            duplicate: None,
        });
        assert_eq!(
            path_to_try(None, &f),
            Err(words::try_unplain("\u{fffd}.ttf"))
        );
    }

    #[test]
    fn font_paths_replaces_the_system_s_folders() {
        let conf = "font-paths = /opt/fonts\n";
        let f = file(DEJAVU, State::Active, Ok(()));
        assert_eq!(
            path_to_try(Some(conf), &f),
            Err(words::try_outside_folders("DejaVuSansMono.ttf"))
        );
        let f = file("/opt/fonts/a/Mono.ttf", State::Active, Ok(()));
        assert_eq!(
            path_to_try(Some(conf), &f),
            Ok("/opt/fonts/a/Mono.ttf".to_owned())
        );
        // vtium's own fonts and kbd's console fonts stay where the console
        // looks
        let f = file(
            "/usr/share/kbd/consolefonts/a.psfu.gz",
            State::Active,
            Ok(()),
        );
        assert!(path_to_try(Some(conf), &f).is_ok());
        // whole components are compared
        let f = file("/opt/fonts-old/Mono.ttf", State::Active, Ok(()));
        assert_eq!(
            path_to_try(Some(conf), &f),
            Err(words::try_outside_folders("Mono.ttf"))
        );
    }

    #[test]
    fn a_font_the_console_does_not_hold_as_active_is_not_sent() {
        let f = file(DEJAVU, State::Inactive, Ok(()));
        assert_eq!(
            path_to_try(None, &f),
            Err(words::try_inactive("DejaVuSansMono.ttf"))
        );
        let f = file(DEJAVU, State::Activates, Ok(()));
        assert_eq!(
            path_to_try(None, &f),
            Err(words::try_activates("DejaVuSansMono.ttf"))
        );
    }

    #[test]
    fn a_font_that_fails_the_trust_rule_is_not_sent_and_says_why() {
        let why = "/usr/share/fonts/TTF: other users can change the folder.";
        let f = file(DEJAVU, State::Active, Err(why.to_owned()));
        assert_eq!(
            path_to_try(None, &f),
            Err(words::try_untrusted("DejaVuSansMono.ttf", why))
        );
    }
}
