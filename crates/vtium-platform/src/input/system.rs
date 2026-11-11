//! The system's keyboard layout, as the rules' names an unconfigured vtium
//! types with (ADR-1400's L2 layer and D3).
//!
//! vtium takes the layout from the console keymap first, then from localed:
//!
//! 1. `KEYMAP=` in `/etc/vconsole.conf`, the console keymap that systemd's
//!    vconsole setup loads (vconsole.conf(5)). vtium translates the kbd
//!    keymap's name into the X11 rules' names through systemd's table
//!    `/usr/share/systemd/kbd-model-map`, the table localed uses for the
//!    same translation (localectl(1), "set-keymap": the console keymap is
//!    converted "to the closest matching X11 keyboard mapping"). Reading
//!    both files needs no bus.
//! 2. When vconsole.conf sets no `KEYMAP=`, or the table has no row for
//!    it, a build with the `logind` feature reads localed's X11 layout
//!    ([`crate::logind::layout`]). Any other build, minspec among them,
//!    uses xkb's default names.
//!
//! vtium.conf's `xkb-*` keys win over both ([`super::KeymapConfig::follow`]).
//! When vconsole.conf names a keymap the table cannot translate, vtium logs
//! one line that names it.

use std::path::Path;

use super::RuleNames;

/// The file whose `KEYMAP=` names the console keymap, under the root.
const VCONSOLE: &str = "etc/vconsole.conf";

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

/// The layout from the files under `root`, else from `localed` when the
/// build has it, else xkb's default names, which are empty.
pub(crate) fn layout_in(root: &Path, localed: Option<fn() -> RuleNames>) -> RuleNames {
    let vconsole = std::fs::read_to_string(root.join(VCONSOLE)).unwrap_or_default();
    if let Some(keymap) = keymap(&vconsole) {
        let table = std::fs::read_to_string(root.join(KBD_MODEL_MAP)).unwrap_or_default();
        if let Some(names) = translate(&table, &keymap) {
            log::info!(
                target: "input",
                "the keyboard layout comes from the console keymap {keymap}: layout {:?}, model {:?}, variant {:?}, options {:?}",
                names.layout, names.model, names.variant, names.options
            );
            return names;
        }
        log::info!(
            target: "input",
            "The console keymap {keymap} in /{VCONSOLE} has no row in /{KBD_MODEL_MAP}, so vtium cannot translate it to a keyboard layout. {}",
            if localed.is_some() {
                "vtium takes the layout from localed instead."
            } else {
                "vtium uses xkb's default layout unless vtium.conf sets one."
            }
        );
    }
    localed.map(|read| read()).unwrap_or_default()
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

    use std::path::PathBuf;

    use super::*;
    use crate::input::KeymapConfig;

    /// A few rows of systemd 262's kbd-model-map, with its header.
    const TABLE: &str = "\
# consolelayout\t\txlayout\txmodel\t\txvariant\txoptions\t\t\t\t\tbcp47
de\t\t\tde\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\t\t\tde-DE,de-AT,de
de-latin1\t\tde\tpc105\t\t-\t\tterminate:ctrl_alt_bksp\t\t\t\t-
de-latin1-nodeadkeys\tde\tpc105\t\tnodeadkeys\tterminate:ctrl_alt_bksp\t\t\t\t-
us\t\t\tus\tpc105+inet\t-\t\tterminate:ctrl_alt_bksp\t\t\t\ten-US,en
";

    /// A root of its own for one test, holding `vconsole` as
    /// etc/vconsole.conf when given, and the table.
    fn root(name: &str, vconsole: Option<&str>) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("vtium-system-layout-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
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
        root
    }

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

    #[test]
    fn only_a_build_with_logind_asks_localed() {
        assert_eq!(localed().is_some(), cfg!(feature = "logind"));
    }
}
