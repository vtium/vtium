//! The palette the `palette` key names: one of vtium-term's built-in
//! palettes, `custom`, whose sixteen colors and two defaults the
//! `palette-<color>` keys set, or a palette file, a color scheme in
//! Ghostty's theme format that vtium-settings reads.
//!
//! vtium reads each palette file once, the first time a configuration
//! names it, and keeps what it read for as long as it runs: the palette,
//! or nothing when the file failed or no folder holds it. A file that
//! fails costs one read and one log line, however many heads name it. A
//! failed or missing file counts as missing, and whoever named it draws
//! the palette below: a look's falls back to the top level's, and the top
//! level's to `legacy`. vtium reads `palette` only when it starts, so a
//! file edited while vtium runs takes effect at the next start, as any
//! change to `palette` does.
//!
//! References:
//! - ADR-1205 D10: palette files, their two folders, and what vtium draws when one fails.
//! - ADR-1403 D5: a look's `palette` wins over the top level's.

use std::path::Path;
use std::sync::Mutex;

use vtium_settings::Trust;
use vtium_settings::palette::{self, CUSTOM};
use vtium_term::{Palette, Rgb};

use super::Config;
use super::keys::{self, Key};

/// The sixteen colors' keys, in palette order.
const PALETTE_KEYS: [&Key; 16] = [
    &keys::PALETTE_BLACK,
    &keys::PALETTE_RED,
    &keys::PALETTE_GREEN,
    &keys::PALETTE_YELLOW,
    &keys::PALETTE_BLUE,
    &keys::PALETTE_MAGENTA,
    &keys::PALETTE_CYAN,
    &keys::PALETTE_LIGHT_GREY,
    &keys::PALETTE_DARK_GREY,
    &keys::PALETTE_LIGHT_RED,
    &keys::PALETTE_LIGHT_GREEN,
    &keys::PALETTE_LIGHT_YELLOW,
    &keys::PALETTE_LIGHT_BLUE,
    &keys::PALETTE_LIGHT_MAGENTA,
    &keys::PALETTE_LIGHT_CYAN,
    &keys::PALETTE_WHITE,
];

/// The palette files vtium has read, by name, each with its palette or
/// `None` for one that failed or that no folder holds.
#[derive(Debug, Default)]
pub(crate) struct Schemes {
    read: Vec<(String, Option<Palette>)>,
}

impl Schemes {
    /// The palette file `name`'s palette, read from `folders` under
    /// `trust` the first time it is asked for. A file that fails, or that
    /// no folder holds, gives `None`, and `log` gets one line for it, the
    /// first time only.
    pub(crate) fn get(
        &mut self,
        name: &str,
        folders: &[&Path],
        trust: &Trust,
        log: &mut dyn FnMut(String),
    ) -> Option<Palette> {
        if let Some((_, palette)) = self.read.iter().find(|(n, _)| n == name) {
            return palette.clone();
        }
        const INSTEAD: &str = "The heads that name it draw the color scheme that vtium.conf's top level names, or legacy when the top level names it.";
        let palette = match palette::read(name, folders, trust) {
            Ok(Some(scheme)) => Some(scheme.palette),
            Ok(None) => {
                let names: Vec<String> = folders.iter().map(|f| f.display().to_string()).collect();
                log(format!(
                    "Color scheme {name}: there is no file {name}{} in {}. {INSTEAD}",
                    palette::SUFFIX,
                    names.join(" or "),
                ));
                None
            }
            Err(failure) => {
                log(failure.log_line(INSTEAD));
                None
            }
        };
        self.read.push((name.to_owned(), palette.clone()));
        palette
    }
}

/// The palette files the console has read since it started.
static SCHEMES: Mutex<Schemes> = Mutex::new(Schemes { read: Vec::new() });

/// The palette file `name`'s palette, read once from the two palette
/// folders as the account vtium runs as; `None` when it fails or no
/// folder holds it, logged the first time.
fn file_palette(name: &str) -> Option<Palette> {
    let folders = palette::FOLDERS.map(Path::new);
    let mut schemes = SCHEMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    schemes.get(name, &folders, &crate::font::trust(), &mut |line| {
        log::warn!(target: "palette", "{line}");
    })
}

impl Config {
    /// The palette `palette` names, with `legacy` in place of a palette
    /// file that fails or that no folder holds. A name the key does not
    /// take is reported as it is read and leaves the value below it, so
    /// `legacy` is drawn when no source names a palette.
    pub fn palette(&self) -> Palette {
        self.palette_or(Palette::default)
    }

    /// The palette `palette` names, with `instead()` in place of a
    /// palette file that fails or that no folder holds.
    pub fn palette_or(&self, instead: impl FnOnce() -> Palette) -> Palette {
        self.palette_with(instead, &mut file_palette)
    }

    /// [`Config::palette_or`], with `file` giving a palette file's palette.
    pub(crate) fn palette_with(
        &self,
        instead: impl FnOnce() -> Palette,
        file: &mut dyn FnMut(&str) -> Option<Palette>,
    ) -> Palette {
        match self.text(&keys::PALETTE) {
            Some(CUSTOM) => {
                let rgb = |k: &Key| {
                    let [r, g, b] = self.rgb(k).unwrap_or_default();
                    Rgb::new(r, g, b)
                };
                Palette::custom(
                    PALETTE_KEYS.map(rgb),
                    rgb(&keys::PALETTE_FOREGROUND),
                    rgb(&keys::PALETTE_BACKGROUND),
                )
            }
            None => Palette::default(),
            Some(name) => match Palette::named(name) {
                Some(built_in) => built_in,
                None => file(name).unwrap_or_else(instead),
            },
        }
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use super::*;
    use crate::scratch::Scratch;

    fn config(text: &str) -> Config {
        crate::config::load(&[], &mut |_: &Path| Ok(Some(text.as_bytes().to_vec()))).config
    }

    /// A palette folder under a scratch directory with the file `name`
    /// holding `text`, and the trust its owner gets.
    fn folder(dir: &Scratch, files: &[(&str, &str)]) -> Trust {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
        for (name, text) in files {
            let path = dir.join(format!("{name}.palette"));
            std::fs::write(&path, text).unwrap_or_default();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        }
        let me = std::fs::metadata(&**dir).map_or(0, |m| m.uid());
        Trust::owner(me).below(dir)
    }

    // The top level and the command line take a built-in palette's name
    // or a file's name in `palette`, and refuse a value that breaks the
    // name rule, which leaves the value below it.
    #[test]
    fn the_top_level_palette_takes_a_built_in_or_a_file_s_name() {
        for value in ["nord", "catppuccin-mocha"] {
            let loaded = crate::config::load(&[], &mut |_: &Path| {
                Ok(Some(format!("palette = {value}\n").into_bytes()))
            });
            assert_eq!(loaded.reports.len(), 0, "{value}");
            assert_eq!(loaded.config.text(&keys::PALETTE), Some(value));
        }
        let loaded = crate::config::load(&["--palette=../nord".into()], &mut |_: &Path| {
            Ok(Some(b"palette = vga\n".to_vec()))
        });
        assert_eq!(loaded.reports.len(), 1);
        assert_eq!(loaded.config.text(&keys::PALETTE), Some("vga"));
    }

    // A palette file that reads cleanly is the palette; one that fails, and
    // a name no folder holds, give the palette below it, and vtium logs one
    // line for each file the first time, whatever asks for it again.
    // covers: color.palette.named
    #[test]
    fn a_palette_file_is_read_once_and_a_failed_one_logs_once() {
        let dir = Scratch::new("palette-console");
        let trust = folder(
            &dir,
            &[
                ("dracula", "background = #282a36\npalette = 1=#ff5555\n"),
                ("broken", "background = #282a36\nthis line is wrong\n"),
            ],
        );
        let folders = [&*dir];
        let mut schemes = Schemes::default();
        let mut logged = Vec::new();
        let mut file = |name: &str| schemes.get(name, &folders, &trust, &mut |l| logged.push(l));
        let top = config("palette = dracula\n").palette_with(Palette::default, &mut file);
        assert_eq!(top.bg(), Rgb::new(0x28, 0x2a, 0x36));
        assert_eq!(top.color(1), Rgb::new(0xff, 0x55, 0x55));
        let nord = || Palette::named("nord").unwrap_or_default();
        for _ in 0..3 {
            assert_eq!(
                config("palette = broken\n").palette_with(nord, &mut file),
                nord()
            );
            assert_eq!(
                config("palette = absent\n").palette_with(nord, &mut file),
                nord()
            );
        }
        // the built-in palettes and custom read no file
        assert_eq!(
            config("palette = vga\n").palette_with(nord, &mut file),
            Palette::named("vga").unwrap_or_default()
        );
        let [broken, absent] = logged.as_slice() else {
            panic!("{logged:?}");
        };
        assert!(
            broken.starts_with(&format!(
                "Color scheme broken ({}:2): This line is not a setting.",
                dir.join("broken.palette").display()
            )),
            "{broken}"
        );
        assert_eq!(
            absent,
            &format!(
                "Color scheme absent: there is no file absent.palette in {}. The heads that name it draw the color scheme that vtium.conf's top level names, or legacy when the top level names it.",
                dir.display()
            )
        );
    }
}
