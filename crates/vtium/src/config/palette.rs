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
//! A palette file is a resource, so vtium reads it only when it passes the
//! trust rule and the batch lists it ([`palette::Gate`]). vtium reads the
//! batch file and the mount table once, the first time a palette file is
//! asked for. With no batch file, the batch is the list of a fresh
//! install, which holds every palette the package ships, so those draw
//! from the first start.
//!
//! References:
//! - ADR-1205 D10: palette files, their two folders, what vtium draws when one fails, and the package's palettes being active on a fresh install.
//! - ADR-1205 D4: the trust rule and the batch a palette file passes.
//! - ADR-1403 D5: a look's `palette` wins over the top level's.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use vtium_settings::Trust;
use vtium_settings::batch::{self, Batch};
use vtium_settings::cullet::{Gate, Rule};
use vtium_settings::mounts::Mounts;
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
    /// The palette file `name`'s palette, read from `folders` through
    /// `gate` the first time it is asked for. A file that fails, or that
    /// no folder holds, gives `None`, and `log` gets one line for it, the
    /// first time only.
    pub(crate) fn get(
        &mut self,
        name: &str,
        folders: &[&Path],
        gate: &Gate,
        log: &mut dyn FnMut(String),
    ) -> Option<Palette> {
        if let Some((_, palette)) = self.read.iter().find(|(n, _)| n == name) {
            return palette.clone();
        }
        const INSTEAD: &str = "In its place, the heads draw the color scheme that vtium.conf's top level names. When the top level names this color scheme, or one that fails too, the heads draw legacy.";
        let palette = match palette::read(name, folders, gate) {
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

/// The batch at `file`, read under `trust` as vtium reads it at start, with
/// what vtium found reading it. When the file is missing or vtium cannot
/// use it, the batch is the list of a fresh install: the font files the
/// package ships, the console font that vconsole.conf names, and every
/// palette in `package_palettes`, the folder of the palettes the package
/// ships.
///
/// References:
/// - ADR-1205 D4, its notes of 2026-11-18 and 2026-11-19: the batch file, and the list of a fresh install in its place.
/// - ADR-1205 D10: the package's palettes are active on a fresh install.
fn batch_at(file: &Path, trust: &Trust, package_palettes: &Path) -> batch::Read {
    let places = batch::Places {
        file: file.to_path_buf(),
        palettes: package_palettes.to_path_buf(),
        ..batch::Places::system()
    };
    Batch::system(&places, trust)
}

/// The gate a palette file passes for a vtium that reads under `trust`:
/// the trust rule, this machine's mount table, and the batch at
/// [`batch::BATCH_FILE`]. A mount table vtium cannot read is empty, so
/// every file fails the rule, as the rule asks when vtium cannot tell
/// where a file is mounted.
///
/// vtium logs once what it found reading the batch file: a missing file as
/// information, since a machine upgraded from an earlier vtium has none,
/// and a file it cannot use, or a line it ignores, as a warning.
fn system_gate(trust: &Trust) -> Gate {
    let read = batch_at(
        Path::new(batch::BATCH_FILE),
        trust,
        Path::new(palette::PACKAGE_PALETTES),
    );
    for finding in &read.findings {
        match finding {
            batch::Finding::Missing { .. } => log::info!(target: "batch", "{finding}"),
            _ => log::warn!(target: "batch", "{finding}"),
        }
    }
    Gate::new(
        Rule::new(trust.clone()),
        Some(Mounts::system().unwrap_or_default()),
        read.batch,
    )
}

/// The console's gate, made the first time a palette file is asked for.
static GATE: OnceLock<Gate> = OnceLock::new();

/// The palette file `name`'s palette, read once from the two palette
/// folders as the account vtium runs as, through the console's gate;
/// `None` when it fails or no folder holds it, logged the first time.
fn file_palette(name: &str) -> Option<Palette> {
    let folders = palette::FOLDERS.map(Path::new);
    let gate = GATE.get_or_init(|| system_gate(&crate::font::trust()));
    let mut schemes = SCHEMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    schemes.get(name, &folders, gate, &mut |line| {
        log::warn!(target: "palette", "{line}");
    })
}

impl Config {
    /// The batch at `file`, read under `trust` as vtium reads it at start,
    /// with the list of a fresh install in its place when the file is
    /// missing or vtium cannot use it; `package_palettes` is the folder of
    /// the palettes the package ships. check-config reads the batch here,
    /// as the console does.
    pub(crate) fn batch_at(file: &Path, trust: &Trust, package_palettes: &Path) -> batch::Read {
        batch_at(file, trust, package_palettes)
    }

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

    /// The palette file `name`'s palette, read once from the two palette
    /// folders, as [`Config::palette_or`] reads it; `None` when the file
    /// fails or no folder holds it, logged the first time.
    pub(crate) fn palette_file(name: &str) -> Option<Palette> {
        file_palette(name)
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
    /// holding `text`, and the gate its owner's files pass, with each of
    /// the files active.
    fn folder(dir: &Scratch, files: &[(&str, &str)]) -> Gate {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
        let mut active = Vec::new();
        for (name, text) in files {
            let path = dir.join(format!("{name}.palette"));
            std::fs::write(&path, text).unwrap_or_default();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
            active.push(batch::Entry::Path(path));
        }
        let me = std::fs::metadata(&**dir).map_or(0, |m| m.uid());
        Gate::new(
            Rule::new(Trust::owner(me)).below(dir),
            Some(Mounts::system().unwrap_or_default()),
            Batch::new(active),
        )
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
        let gate = folder(
            &dir,
            &[
                ("dracula", "background = #282a36\npalette = 1=#ff5555\n"),
                ("broken", "background = #282a36\nthis line is wrong\n"),
            ],
        );
        let folders = [&*dir];
        let mut schemes = Schemes::default();
        let mut logged = Vec::new();
        let mut file = |name: &str| schemes.get(name, &folders, &gate, &mut |l| logged.push(l));
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
                "Color scheme absent: there is no file absent.palette in {}. In its place, the heads draw the color scheme that vtium.conf's top level names. When the top level names this color scheme, or one that fails too, the heads draw legacy.",
                dir.display()
            )
        );
    }

    // A head whose look names a palette file that fails draws the top
    // level's palette, not legacy, and vtium logs one line for the file
    // however many heads name it.
    // covers: color.palette.named
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_head_whose_look_s_palette_file_fails_draws_the_top_level_s() {
        let dir = Scratch::new("palette-head");
        let gate = folder(
            &dir,
            &[("broken", "background = #282a36\nthis line is wrong\n")],
        );
        let folders = [&*dir];
        let mut schemes = Schemes::default();
        let mut logged = Vec::new();
        let top = config("palette = solarized\n");
        let look = config("palette = broken\n");
        let solarized = Palette::named("solarized").unwrap_or_default();
        let clock: crate::term::Clock = std::rc::Rc::new(std::time::Instant::now);
        for _ in 0..2 {
            let mut term = crate::head::terminal(&top, (0, 0), 0, std::rc::Rc::clone(&clock));
            let drawn = crate::head::look::palette_with(&look, &mut term, &mut |name| {
                schemes.get(name, &folders, &gate, &mut |l| logged.push(l))
            });
            assert_eq!(drawn, solarized);
            assert_eq!(term.terminal.palette(), &solarized);
        }
        let [line] = logged.as_slice() else {
            panic!("{logged:?}");
        };
        assert!(line.starts_with("Color scheme broken ("), "{line}");
    }

    // With no batch file, vtium uses the list of a fresh install, which
    // holds the palettes the package ships, so a shipped palette draws from
    // the first start. A palette in the administrator's folder that the
    // list does not hold is not active, and vtium logs why.
    // covers: color.palette.named
    #[test]
    fn a_shipped_palette_draws_with_no_batch_file() {
        let dir = Scratch::new("palette-fresh");
        let _ = std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o755));
        let (etc, share) = (dir.join("etc"), dir.join("share"));
        for d in [&etc, &share] {
            let _ = std::fs::create_dir(d);
            let _ = std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755));
        }
        for (d, name) in [(&share, "kiln"), (&etc, "mine")] {
            let path = d.join(format!("{name}.palette"));
            std::fs::write(&path, "background = #282a36\n").unwrap_or_default();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        }
        let me = std::fs::metadata(&*dir).map_or(0, |m| m.uid());
        let trust = Trust::owner(me).below(&dir);
        let read = batch_at(&etc.join("batch.conf"), &trust, &share);
        assert!(
            matches!(read.findings[..], [batch::Finding::Missing { .. }]),
            "{:?}",
            read.findings
        );
        let gate = Gate::new(
            Rule::new(Trust::owner(me)).below(&dir),
            Some(Mounts::system().unwrap_or_default()),
            read.batch,
        );
        let folders = [etc.as_path(), share.as_path()];
        let mut logged = Vec::new();
        let mut schemes = Schemes::default();
        let kiln = schemes.get("kiln", &folders, &gate, &mut |l| logged.push(l));
        assert_eq!(kiln.map(|p| p.bg()), Some(Rgb::new(0x28, 0x2a, 0x36)));
        assert_eq!(logged, Vec::<String>::new());
        assert_eq!(
            schemes.get("mine", &folders, &gate, &mut |l| logged.push(l)),
            None
        );
        let [line] = logged.as_slice() else {
            panic!("{logged:?}");
        };
        assert!(
            line.starts_with(&format!(
                "Color scheme mine ({}): The color scheme is not active,",
                etc.join("mine.palette").display()
            )),
            "{line}"
        );
    }
}
