//! The system's keyboard layout, as the rules' names an unconfigured vtium
//! types with (ADR-1400's L2 layer and D3).
//!
//! vtium takes the layout from the console keymap first, then from
//! Debian's keyboard file, then from localed:
//!
//! 1. `KEYMAP=` in `/etc/vconsole.conf`, the console keymap that systemd's
//!    vconsole setup loads (vconsole.conf(5)). vtium translates the kbd
//!    keymap's name into the X11 rules' names through systemd's table
//!    `/usr/share/systemd/kbd-model-map`, the table localed uses for the
//!    same translation (localectl(1), "set-keymap": the console keymap is
//!    converted "to the closest matching X11 keyboard mapping"). Reading
//!    both files needs no bus.
//! 2. When vconsole.conf sets no `KEYMAP=`, or the table has no row for
//!    it, `/etc/default/keyboard`, where Debian and Ubuntu keep the
//!    console's layout for console-setup. It holds the X11 rules' names
//!    already, as `XKBMODEL`, `XKBLAYOUT`, `XKBVARIANT` and `XKBOPTIONS`
//!    (keyboard(5), keyboard-configuration 1.242~deb13u1), so vtium takes
//!    them as they are. The file is one shell-style assignment a line, with
//!    `#` starting a comment, and setupcon(1) sources it with the shell, so
//!    vtium reads a value bare or in single or double quotes, an `export`
//!    before it, and the last assignment of a name. An empty value is unset,
//!    and a file whose `XKBLAYOUT` is unset gives no layout.
//! 3. Otherwise a build with the `logind` feature reads localed's X11
//!    layout ([`crate::logind::layout`]). Any other build, minspec among
//!    them, uses xkb's default names.
//!
//! vtium.conf's `xkb-*` keys win over all three
//! ([`super::KeymapConfig::follow`]).
//! When vconsole.conf names a keymap the table cannot translate, vtium logs
//! one line that names it.
//!
//! vtium reads the layout at its start. A build with `logind` reads it again,
//! in the same order, each time localed announces a new layout while vtium
//! runs ([`crate::logind::LayoutFollow`]). [`find`] reads the sources and
//! logs nothing, so that each caller logs what it needs.

use std::convert::Infallible;
use std::path::Path;

use super::RuleNames;

/// The file whose `KEYMAP=` names the console keymap, under the root.
const VCONSOLE: &str = "etc/vconsole.conf";

/// Debian's and Ubuntu's keyboard file, under the root (keyboard(5)).
const KEYBOARD: &str = "etc/default/keyboard";

/// systemd's table from console keymaps to X11 layouts, under the root.
const KBD_MODEL_MAP: &str = "usr/share/systemd/kbd-model-map";

/// The system's keyboard layout, read from the machine's own files and, in a
/// build with the `logind` feature, from localed.
pub fn system_layout() -> RuleNames {
    layout_in(Path::new("/"), localed())
}

/// Where a build reads localed's layout: only a build with `logind` has it.
fn localed() -> Option<fn() -> RuleNames> {
    #[cfg(feature = "logind")]
    {
        Some(crate::logind::layout)
    }
    #[cfg(not(feature = "logind"))]
    {
        None
    }
}

/// Where the system's keyboard layout came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The console keymap of this name in vconsole.conf, through the table.
    ConsoleKeymap(String),
    /// Debian's keyboard file.
    KeyboardFile,
    /// localed's X11 layout.
    Localed,
    /// None of them: xkb's default names, which are empty.
    Default,
}

/// The system's keyboard layout, where it came from, and the console
/// keymap that the table could not translate, if there was one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) names: RuleNames,
    pub(crate) origin: Origin,
    pub(crate) untranslated: Option<String>,
}

/// The layout from the files under `root`, else from `localed` when the
/// build has it, else xkb's default names, which are empty. It logs
/// nothing, so that vtium can read it again while it runs and log only what
/// changed. `Err` when localed is asked and cannot answer.
pub(crate) fn find<E>(
    root: &Path,
    localed: Option<impl FnOnce() -> Result<RuleNames, E>>,
) -> Result<Found, E> {
    let vconsole = std::fs::read_to_string(root.join(VCONSOLE)).unwrap_or_default();
    let mut untranslated = None;
    if let Some(keymap) = keymap(&vconsole) {
        let table = std::fs::read_to_string(root.join(KBD_MODEL_MAP)).unwrap_or_default();
        if let Some(names) = translate(&table, &keymap) {
            return Ok(Found {
                names,
                origin: Origin::ConsoleKeymap(keymap),
                untranslated,
            });
        }
        untranslated = Some(keymap);
    }
    let keyboard = std::fs::read_to_string(root.join(KEYBOARD)).unwrap_or_default();
    let (names, origin) = if let Some(names) = xkb_names(&keyboard) {
        (names, Origin::KeyboardFile)
    } else if let Some(read) = localed {
        (read()?, Origin::Localed)
    } else {
        (RuleNames::default(), Origin::Default)
    };
    Ok(Found {
        names,
        origin,
        untranslated,
    })
}

/// The layout [`find`] gives, with the lines that say where it came from
/// logged. localed's reader logs its own line, and gives empty names when
/// localed cannot answer.
pub(crate) fn layout_in(root: &Path, localed: Option<fn() -> RuleNames>) -> RuleNames {
    let asks_localed = localed.is_some();
    let found = match find(
        root,
        localed.map(|read| move || Ok::<_, Infallible>(read())),
    ) {
        Ok(found) => found,
        Err(never) => match never {},
    };
    if let Some(keymap) = &found.untranslated {
        log::info!(
            target: "input",
            "The console keymap {keymap} in /{VCONSOLE} has no row in /{KBD_MODEL_MAP}, so vtium cannot translate it to a keyboard layout. {}",
            if asks_localed {
                "vtium takes the layout from /etc/default/keyboard instead, else from localed."
            } else {
                "vtium takes the layout from /etc/default/keyboard instead, else uses xkb's default layout unless vtium.conf sets one."
            }
        );
    }
    let names = &found.names;
    match &found.origin {
        Origin::ConsoleKeymap(keymap) => log::info!(
            target: "input",
            "the keyboard layout comes from the console keymap {keymap}: layout {:?}, model {:?}, variant {:?}, options {:?}",
            names.layout, names.model, names.variant, names.options
        ),
        Origin::KeyboardFile => log::info!(
            target: "input",
            "the keyboard layout comes from /{KEYBOARD}: layout {:?}, model {:?}, variant {:?}, options {:?}",
            names.layout, names.model, names.variant, names.options
        ),
        Origin::Localed | Origin::Default => {}
    }
    found.names
}

/// The X11 rules' names in the text of /etc/default/keyboard, or `None`
/// when it sets no `XKBLAYOUT`. A name it does not set is empty, which is
/// xkb's default.
fn xkb_names(text: &str) -> Option<RuleNames> {
    let value = |name: &str| shell_value(text, name).unwrap_or_default();
    let layout = shell_value(text, "XKBLAYOUT")?;
    Some(RuleNames {
        model: value("XKBMODEL"),
        layout,
        variant: value("XKBVARIANT"),
        options: value("XKBOPTIONS"),
    })
}

/// The value the last assignment of `name` in the shell-style `text` gives
/// it, without its quotes; `None` when no line sets it to anything. A line
/// may start with `export`, and `#` starts a comment line.
fn shell_value(text: &str, name: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.strip_prefix("export ").map_or(line, str::trim_start))
        .filter_map(|line| line.strip_prefix(name)?.strip_prefix('='))
        .map(|value| {
            let value = value.trim();
            let quoted = |q: char| value.len() >= 2 && value.starts_with(q) && value.ends_with(q);
            if quoted('"') || quoted('\'') {
                value
                    .get(1..value.len().saturating_sub(1))
                    .unwrap_or_default()
            } else {
                value
            }
            .to_owned()
        })
        .next_back()
        .filter(|value| !value.is_empty())
}

/// The value of the last `KEYMAP=` in the text of vconsole.conf, without
/// quotes; `None` when no line sets it to anything. The file is a list of
/// shell-style assignments, with `#` starting a comment line
/// (vconsole.conf(5)).
fn keymap(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("KEYMAP="))
        .map(|value| {
            value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_owned()
        })
        .next_back()
        .filter(|value| !value.is_empty())
}

/// The X11 rules' names of the console keymap `keymap` in the text of
/// kbd-model-map: the first row whose first column is the keymap. Each row
/// reads `consolelayout xlayout xmodel xvariant xoptions [bcp47]`, separated
/// by white space, with `-` for an empty column and `#` starting a comment
/// line, as the file's own header says.
fn translate(table: &str, keymap: &str) -> Option<RuleNames> {
    let column = |c: Option<&str>| match c {
        Some("-") | None => String::new(),
        Some(c) => c.to_owned(),
    };
    table
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(str::split_whitespace)
        .find_map(|mut fields| {
            if fields.next()? != keymap {
                return None;
            }
            let layout = column(fields.next());
            let model = column(fields.next());
            let variant = column(fields.next());
            let options = column(fields.next());
            (!layout.is_empty()).then_some(RuleNames {
                model,
                layout,
                variant,
                options,
            })
        })
}

#[cfg(test)]
mod tests {
    #![expect(clippy::panic, reason = "a test fails by panicking")]

    use super::*;
    use crate::input::KeymapConfig;
    use crate::scratch::Scratch;

    /// A few rows of systemd 262's kbd-model-map, with its header.
    const TABLE: &str = "\
# consolelayout\t\txlayout\txmodel\t\txvariant\txoptions\t\t\t\t\tbcp47
de\t\t\tde\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\t\t\tde-DE,de-AT,de
de-latin1\t\tde\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\t\t\t-
de-latin1-nodeadkeys\tde\tpc105\t\tnodeadkeys\tterminate:ctrl_alt_bksp\t\t\t\t-
us\t\t\tus\tpc105+inet\t-\t\tterminate:ctrl_alt_bksp\t\t\t\ten-US,en
";

    /// A root of its own for one test, holding `vconsole` as
    /// etc/vconsole.conf when given, and the table. The test removes it
    /// when it ends.
    fn root(name: &str, vconsole: Option<&str>) -> Scratch {
        root_with(name, vconsole, None)
    }

    /// [`root`], with `keyboard` as etc/default/keyboard when given.
    fn root_with(name: &str, vconsole: Option<&str>, keyboard: Option<&str>) -> Scratch {
        let root = Scratch::new(&format!("system-layout-{name}"));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{e}"));
            }
            std::fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
        };
        write(KBD_MODEL_MAP, TABLE);
        if let Some(text) = vconsole {
            write(VCONSOLE, text);
        }
        if let Some(text) = keyboard {
            write(KEYBOARD, text);
        }
        root
    }

    /// /etc/default/keyboard as Debian's installer writes it, for a German
    /// keyboard without dead keys (keyboard(5), its EXAMPLES).
    const DEBIAN: &str = "\
# KEYBOARD CONFIGURATION FILE

# Consult the keyboard(5) manual page.

XKBMODEL=\"pc105\"
XKBLAYOUT=\"de\"
XKBVARIANT=\"nodeadkeys\"
XKBOPTIONS=\"\"

BACKSPACE=\"guess\"
";

    /// localed's stand-in: a French layout, on a model the table's German
    /// rows do not name.
    fn french() -> RuleNames {
        RuleNames {
            model: "pc104".to_owned(),
            layout: "fr".to_owned(),
            variant: "azerty".to_owned(),
            options: "compose:ralt".to_owned(),
        }
    }

    fn names(layout: &str, model: &str, variant: &str, options: &str) -> RuleNames {
        RuleNames {
            model: model.to_owned(),
            layout: layout.to_owned(),
            variant: variant.to_owned(),
            options: options.to_owned(),
        }
    }

    // covers: platform.system-layout
    #[test]
    fn the_console_keymap_comes_first_through_the_table() {
        let root = root(
            "first",
            Some("# the console\nFONT=ter-124n\nKEYMAP=\"de-latin1\"\n"),
        );
        let want = names("de", "pc105", "", "terminate:ctrl_alt_bksp");
        assert_eq!(layout_in(&root, Some(french)), want);
        assert_eq!(layout_in(&root, None), want);
        let root = super::tests::root("variant", Some("KEYMAP=de-latin1-nodeadkeys\n"));
        assert_eq!(
            layout_in(&root, None),
            names("de", "pc105", "nodeadkeys", "terminate:ctrl_alt_bksp")
        );
    }

    // covers: platform.system-layout
    #[test]
    fn a_keymap_the_table_lacks_falls_back_to_localed_or_xkbs_default() {
        let root = root("unknown", Some("KEYMAP=dvorak-programmer\n"));
        assert_eq!(layout_in(&root, Some(french)), french());
        assert_eq!(layout_in(&root, None), RuleNames::default());
    }

    // covers: platform.system-layout
    #[test]
    fn without_a_keymap_localed_gives_the_layout() {
        for vconsole in [None, Some("FONT=ter-124n\n"), Some("KEYMAP=\n")] {
            let root = root("none", vconsole);
            assert_eq!(layout_in(&root, Some(french)), french(), "{vconsole:?}");
            assert_eq!(layout_in(&root, None), RuleNames::default(), "{vconsole:?}");
        }
    }

    // covers: platform.system-layout
    #[test]
    fn vtium_confs_keys_win_over_the_console_keymap_and_localed() {
        let root = root("conf", Some("KEYMAP=de-latin1\n"));
        let set = KeymapConfig {
            layout: "us".to_owned(),
            options: "ctrl:nocaps".to_owned(),
            ..KeymapConfig::default()
        };
        let from_keymap = set.clone().follow(&layout_in(&root, Some(french)));
        assert_eq!(
            (
                from_keymap.layout.as_str(),
                from_keymap.model.as_str(),
                from_keymap.options.as_str()
            ),
            ("us", "pc105", "ctrl:nocaps")
        );
        let root = super::tests::root("conf-localed", None);
        let from_localed = set.follow(&layout_in(&root, Some(french)));
        assert_eq!(
            (
                from_localed.layout.as_str(),
                from_localed.variant.as_str(),
                from_localed.options.as_str()
            ),
            ("us", "", "ctrl:nocaps")
        );
    }

    // covers: platform.system-layout
    #[test]
    fn debians_keyboard_file_gives_the_layout_without_a_console_keymap() {
        for vconsole in [None, Some("FONT=ter-124n\n")] {
            let root = root_with("debian", vconsole, Some(DEBIAN));
            let want = names("de", "pc105", "nodeadkeys", "");
            assert_eq!(layout_in(&root, Some(french)), want, "{vconsole:?}");
            assert_eq!(layout_in(&root, None), want, "{vconsole:?}");
        }
        // a keymap the table lacks falls through to the keyboard file too
        let root = root_with(
            "debian-unknown",
            Some("KEYMAP=dvorak-programmer\n"),
            Some(DEBIAN),
        );
        assert_eq!(
            layout_in(&root, Some(french)),
            names("de", "pc105", "nodeadkeys", "")
        );
    }

    // covers: platform.system-layout
    #[test]
    fn the_console_keymap_wins_over_the_keyboard_file() {
        let root = root_with("both", Some("KEYMAP=us\n"), Some(DEBIAN));
        assert_eq!(
            layout_in(&root, Some(french)),
            names("us", "pc105+inet", "", "terminate:ctrl_alt_bksp")
        );
    }

    // covers: platform.system-layout
    #[test]
    fn vtium_confs_keys_win_over_the_keyboard_file_and_the_console_keymap() {
        let set = KeymapConfig {
            layout: "fr".to_owned(),
            variant: "bepo".to_owned(),
            ..KeymapConfig::default()
        };
        for (name, vconsole) in [("conf-debian", None), ("conf-both", Some("KEYMAP=us\n"))] {
            let root = root_with(name, vconsole, Some(DEBIAN));
            let got = set.clone().follow(&layout_in(&root, None));
            assert_eq!(
                (got.layout.as_str(), got.variant.as_str()),
                ("fr", "bepo"),
                "{vconsole:?}"
            );
        }
    }

    // covers: platform.system-layout
    #[test]
    fn an_empty_layout_in_the_keyboard_file_falls_through_to_localed() {
        for text in [
            "XKBMODEL=\"pc105\"\nXKBLAYOUT=\"\"\n",
            "XKBMODEL=pc105\nXKBLAYOUT=\n",
            "# XKBLAYOUT=\"de\"\n",
            "XKBLAYOUT=\"de\"\nXKBLAYOUT=''\n",
        ] {
            let root = root_with("debian-empty", None, Some(text));
            assert_eq!(layout_in(&root, Some(french)), french(), "{text:?}");
            assert_eq!(layout_in(&root, None), RuleNames::default(), "{text:?}");
        }
    }

    // The file is read as the shell that setupcon sources it with reads
    // it: quoted or bare values, comments, an `export`, and the last
    // assignment winning.
    #[test]
    fn the_keyboard_file_reads_as_the_shell_reads_it() {
        let text = "  # a comment\nexport XKBLAYOUT='fr'\nXKBVARIANT=bepo\nXKBOPTIONS=\"ctrl:nocaps,compose:ralt\"\nXKBLAYOUT=\"fr\"\n";
        assert_eq!(
            xkb_names(text),
            Some(names("fr", "", "bepo", "ctrl:nocaps,compose:ralt"))
        );
    }

    #[test]
    fn only_a_build_with_logind_asks_localed() {
        assert_eq!(localed().is_some(), cfg!(feature = "logind"));
    }
}
