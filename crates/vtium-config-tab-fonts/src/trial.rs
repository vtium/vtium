//! Try live on a font: the font's path, which the console takes in place
//! of a font set's name, or why the console would refuse it. The trial
//! itself, its countdown and `k` that keeps it, is the shell's, so the
//! tab only returns the path to try.
//!
//! The console takes only a font that root has installed, activated and
//! trusts, at a path it spells one way, inside the folders it reads fonts
//! from. So the tab sends no path the console would refuse, and says why
//! instead, in one line with the rest behind F1.
//!
//! References:
//! - ADR-1200 D5, "Trying one font from vtium-config's Fonts tab": `t`,
//!   the path in place of a font set's name, and the console's checks.
//! - ADR-1206 D2: a tab returns commands, and the shell runs the trial.

use vtium_config_core::fontuse::State;
use vtium_config_core::found::fonts::FontFile;
use vtium_settings::fontpath;

use crate::words;

/// Why the console would refuse a font: the message line, and the text
/// F1 opens behind it, where there is more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub line: String,
    pub more: Option<String>,
}

fn refused(line: String, more: Option<&str>) -> Refused {
    Refused {
        line,
        more: more.map(str::to_owned),
    }
}

/// The path of `f` to try on the console, whose state is `state` and
/// which the installed batch holds where `installed` is set, with the
/// document's vtium.conf text `conf`; or why the console would refuse it.
pub fn path_to_try(
    conf: &str,
    f: &FontFile,
    state: &State,
    installed: bool,
) -> Result<String, Refused> {
    let name = vtium_config_widgets::canvas::printable(&f.resource.name());
    if f.resource.own {
        return Err(refused(words::try_own(&name), None));
    }
    let Some(path) = f.path().to_str().filter(|p| fontpath::plain(p)) else {
        return Err(refused(
            words::try_unplain(&name),
            Some(words::TRY_UNPLAIN_MORE),
        ));
    };
    let font_paths = vtium_settings::setting(conf, vtium_settings::keys::FONT_PATHS.name);
    if !fontpath::inside(f.path(), &fontpath::folders(font_paths.as_deref())) {
        return Err(refused(
            words::try_outside(&name),
            Some(words::TRY_OUTSIDE_MORE),
        ));
    }
    match state {
        State::FailsTrust { check, .. } => Err(Refused {
            line: words::try_untrusted(&name),
            more: Some(check.clone()),
        }),
        State::Missing => Err(refused(words::try_missing(&name), None)),
        State::NotActive | State::TurnedOff(_) => Err(refused(words::try_inactive(&name), None)),
        State::Active if !installed => Err(refused(words::try_not_saved(&name), None)),
        State::Active => Ok(path.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vtium_config_core::discover::Place;
    use vtium_config_core::found::{Kind, Origin, Resource};

    use super::*;

    fn file(path: &str, own: bool) -> FontFile {
        FontFile {
            resource: Resource {
                path: PathBuf::from(path),
                kind: Kind::Font,
                trust: Ok(()),
                own,
                hidden: false,
                origin: Origin::BatchFile,
            },
            place: Place::System,
            duplicate: None,
            described: None,
            asked: false,
        }
    }

    const DEJAVU: &str = "/usr/share/fonts/TTF/DejaVuSansMono.ttf";

    #[test]
    fn an_installed_active_trusted_font_is_sent_by_its_path() {
        let got = path_to_try("", &file(DEJAVU, false), &State::Active, true);
        assert_eq!(got, Ok(DEJAVU.to_owned()));
    }

    #[test]
    fn a_font_the_console_would_refuse_says_why() {
        let f = file(DEJAVU, false);
        let line = |s: &State, installed| path_to_try("", &f, s, installed).map_err(|r| r.line);
        assert_eq!(
            line(&State::NotActive, false),
            Err(words::try_inactive("DejaVuSansMono.ttf"))
        );
        assert_eq!(
            line(&State::Active, false),
            Err(words::try_not_saved("DejaVuSansMono.ttf"))
        );
        let own = file("/home/u/.local/share/fonts/Mine.ttf", true);
        assert_eq!(
            path_to_try("", &own, &State::Active, true).map_err(|r| r.line),
            Err(words::try_own("Mine.ttf"))
        );
        let odd = file("/usr/share/fonts/C#Mono.ttf", false);
        assert_eq!(
            path_to_try("", &odd, &State::Active, true).map_err(|r| r.line),
            Err(words::try_unplain("C#Mono.ttf"))
        );
    }

    #[test]
    fn font_paths_replaces_the_system_s_folders() {
        let conf = "font-paths = /opt/fonts\n";
        let f = file(DEJAVU, false);
        assert_eq!(
            path_to_try(conf, &f, &State::Active, true).map_err(|r| r.line),
            Err(words::try_outside("DejaVuSansMono.ttf"))
        );
        let opt = file("/opt/fonts/a/Mono.ttf", false);
        assert_eq!(
            path_to_try(conf, &opt, &State::Active, true),
            Ok("/opt/fonts/a/Mono.ttf".to_owned())
        );
    }
}
