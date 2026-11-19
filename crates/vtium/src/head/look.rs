//! What a head's look gives it beyond its font: its palette and its
//! vtlook.
//!
//! The palette comes first. A head's palette is the one its look's
//! `palette` names, else the top level's, else `legacy`. A palette file
//! the look names that fails, or that no folder holds, leaves the head on
//! the top level's palette, and vtium logs one line for the file. The
//! palette is the head's configured palette: each console of the head
//! starts with it, and the color resets return to it.
//!
//! The vtlook a head draws with is the one its configuration names, read
//! when the head is made, or the built-in vtlook.
//!
//! A head's configuration gives `vtlook` from the head's look section,
//! else from vtium.conf's top level. With neither, or with `default`, the
//! head draws the built-in vtlook, and vtium reads no file. Otherwise
//! vtium reads the vtlook from the looks folder. When the vtlook fails to
//! read, at any line, the head draws the built-in vtlook and vtium logs
//! one line that names the first error and counts the rest. vtium does
//! not read the vtlook again until it reads its configuration again, so a
//! broken vtlook costs one read and one log line.
//!
//! A vtlook that reads cleanly has its color filters merged into one color
//! table once, when vtium reads it, and every display of the head draws
//! through that table. A table vtium cannot build puts the head on the
//! built-in vtlook, with the same kind of log line as a read that fails.
//!
//! Each display is checked against the vtlook's `[requires]` section. A
//! display that does not meet it draws the built-in vtlook alone, while
//! the head's other displays keep the look, and vtium logs one line that
//! names the display and the first requirement it does not meet. That is
//! not an error of the vtlook. vtium checks a display again at each
//! modeset that may change its size, refresh or depth, and each time the
//! head moves between the GPU path and the CPU path. A display that meets
//! the requirements again draws the vtlook again, with no reread.
//!
//! A build without the `filters` feature reads no vtlook. It logs once
//! for each head whose configuration names one, and draws the built-in
//! vtlook, as the build is meant to.
//!
//! References:
//! - ADR-1205 D10: a look's palette, the top level's in place of a file that fails, and the head's configured palette.
//! - ADR-1304 D5: a head's vtlook comes from its look, else the top level, else the built-in one.
//! - ADR-1304 D2: the table is built on the CPU once for each head.
//! - ADR-1304 D6: any error drops the head to the built-in vtlook, with one log line, and no retry.
//! - ADR-1304 D7: a display that does not meet the requirements draws the built-in vtlook, with one log line, and vtium checks it again when its mode, its depth or the head's path changes.
//! - ADR-1304 D10: a build without `filters` logs once and draws the built-in vtlook.
//! - ADR-1304 D12: the built-in vtlook reads no file.

use crate::config::{Config, keys};
use crate::term::Term;
use vtium_settings::vtlook;
use vtium_term::Palette;

/// The configured palette of a head whose configuration is `config`: the
/// palette its look names, else the top level's. `term` is the head's
/// first console, made from the top level's configuration, so it holds
/// the top level's palette, which takes the place of a palette file of
/// the look's that fails or that no folder holds. `term` takes the
/// head's palette as the one it is configured with.
pub(crate) fn palette(config: &Config, term: &mut Term) -> Palette {
    palette_with(config, term, &mut Config::palette_file)
}

/// [`palette`], with `file` giving a palette file's palette, or `None`
/// for one that fails or that no folder holds.
pub(crate) fn palette_with(
    config: &Config,
    term: &mut Term,
    file: &mut dyn FnMut(&str) -> Option<Palette>,
) -> Palette {
    let top = term.terminal.palette().clone();
    let palette = config.palette_with(|| top, file);
    term.terminal.set_palette(palette.clone());
    palette
}

/// The vtlook a head draws with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Chosen {
    /// The vtlook read, `None` for the built-in one.
    #[cfg(feature = "filters")]
    pub(crate) vtlook: Option<vtlook::Vtlook>,
    /// The vtlook's color filters merged into one table, which each
    /// display's renderer takes; `None` for a vtlook with no color filter.
    #[cfg(feature = "filters")]
    pub(crate) table: Option<std::sync::Arc<vtium_render::filter::Table>>,
}

/// What a display is, as a vtlook's requirements judge it.
#[cfg(feature = "filters")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Display<'a> {
    /// The connector's name, such as DP-1.
    pub(crate) name: &'a str,
    /// The mode's width and height, in the mode's own pixels, not turned.
    pub(crate) size: (u32, u32),
    pub(crate) depth: vtlook::Depth,
    /// The density vtium sizes fonts with.
    pub(crate) dpi: u32,
    /// The head draws on the GPU path.
    pub(crate) gpu: bool,
}

#[cfg(feature = "filters")]
impl Chosen {
    /// The color table display `display` draws with: the vtlook's, when
    /// the display meets the vtlook's requirements. A display that does
    /// not draws the built-in vtlook, `None`, with the line to log, which
    /// names the display and the first requirement it does not meet.
    pub(crate) fn table_for(
        &self,
        display: &Display<'_>,
    ) -> (
        Option<std::sync::Arc<vtium_render::filter::Table>>,
        Option<String>,
    ) {
        let Some(look) = &self.vtlook else {
            return (None, None);
        };
        match unmet(&look.requires, display) {
            None => (self.table.clone(), None),
            Some(why) => {
                let line = format!(
                    "vtlook {}: display {} draws the built-in vtlook: requires {why}",
                    look.name,
                    described(display)
                );
                (None, Some(line))
            }
        }
    }

    /// The line vtium logs when display `display`, which drew the built-in
    /// vtlook because it did not meet the vtlook's requirements, meets
    /// them again and draws the vtlook. `None` with no vtlook.
    pub(crate) fn met_again(&self, display: &Display<'_>) -> Option<String> {
        let look = self.vtlook.as_ref()?;
        Some(format!(
            "vtlook {}: display {} meets the vtlook's requirements again and draws the vtlook",
            look.name,
            described(display)
        ))
    }
}

/// Display `display` as a log line names it: its connector, its mode and
/// its depth, such as `DP-1 (1280x800, 1 bit)`.
#[cfg(feature = "filters")]
fn described(display: &Display<'_>) -> String {
    let (w, h) = display.size;
    let depth = match display.depth {
        vtlook::Depth::One => "1 bit",
        vtlook::Depth::Gray => "8-bit gray",
        vtlook::Depth::Sixteen => "16 bits",
        vtlook::Depth::TwentyFour => "24 bits",
    };
    format!("{} ({w}x{h}, {depth})", display.name)
}

/// The first of `requires` that `display` does not meet, as the log line
/// names it, such as `min-depth 24`; `None` when it meets them all. A
/// key at `any` is always met. A size is met on each side. `min-fps` counts
/// as met here: the measured run that judges it is the frame-rate floor's.
#[cfg(feature = "filters")]
fn unmet(requires: &vtlook::Requires, display: &Display<'_>) -> Option<String> {
    use vtlook::{Depth, Gpu};
    let depth = |d: Depth| match d {
        Depth::One => "1",
        Depth::Gray => "8",
        Depth::Sixteen => "16",
        Depth::TwentyFour => "24",
    };
    if let Some(d) = requires.min_depth.filter(|&d| display.depth < d) {
        return Some(format!("min-depth {}", depth(d)));
    }
    if let Some(d) = requires.max_depth.filter(|&d| display.depth > d) {
        return Some(format!("max-depth {}", depth(d)));
    }
    let (w, h) = display.size;
    if let Some(s) = requires.min_size.filter(|s| w < s.width || h < s.height) {
        return Some(format!("min-size {}x{}", s.width, s.height));
    }
    if let Some(s) = requires.max_size.filter(|s| w > s.width || h > s.height) {
        return Some(format!("max-size {}x{}", s.width, s.height));
    }
    if let Some(d) = requires.min_dpi.filter(|&d| display.dpi < d) {
        return Some(format!("min-dpi {d}"));
    }
    if let Some(d) = requires.max_dpi.filter(|&d| display.dpi > d) {
        return Some(format!("max-dpi {d}"));
    }
    match (requires.gpu, display.gpu) {
        (Gpu::Yes, false) => Some("gpu yes".to_owned()),
        (Gpu::No, true) => Some("gpu no".to_owned()),
        _ => None,
    }
}

/// The look section's own `vtlook` as a setting of the head, laid over the
/// top level's `vtlook` the way the look's other keys are: `settings`
/// gains it unless it already holds one.
pub(crate) fn with_look_vtlook(
    settings: &mut Vec<vtium_settings::Setting>,
    look: Option<&vtium_settings::Look>,
) {
    let Some((value, line)) = look.and_then(|l| Some((l.vtlook.clone()?, l.vtlook_line))) else {
        return;
    };
    if settings.iter().all(|s| s.key.name != keys::VTLOOK.name) {
        settings.push(vtium_settings::Setting {
            key: &keys::VTLOOK,
            value,
            line,
        });
    }
}

/// Chooses head `head`'s vtlook from its configuration `config`, and logs
/// why when the head draws the built-in vtlook though `config` names
/// another. A head with no name, the notice screen, has no vtlook.
pub(crate) fn choose(head: &str, config: &Config) -> Chosen {
    let Some(name) = vtlook::chosen(config.text(&keys::VTLOOK)).filter(|_| !head.is_empty()) else {
        return Chosen::default();
    };
    let (chosen, said) = read(head, name);
    if let Some(said) = said {
        log::warn!(target: "look", "{said}");
    }
    chosen
}

/// Reads the vtlook `name` for head `head`, as the account vtium runs as
/// reads it, with the line to log, if any. vtium looks in the
/// administrator's folder first and then in the package's, so a preset
/// such as `amber` draws with no copy in the administrator's folder, and
/// a copy there takes its place. A `cube` filter's `.cube` file is in the
/// administrator's folder, and vtium reads it only when the batch lists it
/// and it passes the trust rule.
///
/// References:
/// - ADR-1304 D4, its note of 2026-11-19: the two folders, and the administrator's vtlook winning on a name.
/// - ADR-1304 D9: the `.cube` file sits beside the administrator's vtlooks.
/// - ADR-1205 D4: the batch and the trust rule a `.cube` file passes.
#[cfg(feature = "filters")]
fn read(head: &str, name: &str) -> (Chosen, Option<String>) {
    let gate = cube_gate();
    let cubes = vtlook::Cubes {
        dir: std::path::Path::new(vtlook::LOOKS),
        gate,
    };
    read_in(
        head,
        name,
        &vtlook::folders(),
        &crate::font::trust(),
        &cubes,
    )
}

/// The console's gate for a vtlook's `.cube` file, made the first time a
/// vtlook is read: the batch file read as the start reads it, with the
/// list of a fresh install in its place when the file is missing or vtium
/// cannot use it, and the mount table. A mount table vtium cannot read is
/// empty, so every `.cube` file fails the trust rule and counts as missing.
/// The palette reader logs what vtium finds reading the batch file, so this
/// logs nothing.
///
/// References:
/// - ADR-1205 D4: a `.cube` file counts as missing unless the batch lists it and it passes the trust rule.
#[cfg(feature = "filters")]
fn cube_gate() -> &'static vtium_settings::cullet::Gate {
    static GATE: std::sync::OnceLock<vtium_settings::cullet::Gate> = std::sync::OnceLock::new();
    GATE.get_or_init(|| {
        let trust = crate::font::trust();
        let read = Config::batch_at(
            std::path::Path::new(vtium_settings::batch::BATCH_FILE),
            &trust,
            std::path::Path::new(vtium_settings::palette::PACKAGE_PALETTES),
        );
        vtium_settings::cullet::Gate::new(
            vtium_settings::cullet::Rule::new(trust),
            Some(vtium_settings::mounts::Mounts::system().unwrap_or_default()),
            read.batch,
        )
    })
}

/// What a build without frame filters does with the vtlook `name`: it
/// reads nothing and says so.
#[cfg(not(feature = "filters"))]
fn read(head: &str, name: &str) -> (Chosen, Option<String>) {
    (Chosen::default(), Some(vtlook::no_filters(head, name)))
}

/// Reads the vtlook `name` from the first of `folders` that holds it,
/// under `trust`, and each `cube` filter's file through `cubes`: the
/// vtlook read, or the built-in one with the line that says why.
#[cfg(feature = "filters")]
fn read_in(
    head: &str,
    name: &str,
    folders: &[&std::path::Path],
    trust: &vtium_settings::Trust,
    cubes: &vtlook::Cubes,
) -> (Chosen, Option<String>) {
    let look = match vtlook::find(name, folders, trust) {
        Ok(look) => look,
        Err(failure) => return (Chosen::default(), vtlook::log_line(&failure, head)),
    };
    if look.color.is_empty() {
        let chosen = Chosen {
            vtlook: Some(look),
            table: None,
        };
        return (chosen, None);
    }
    match vtlook::table_with(&look, cubes) {
        Ok(table) => {
            let chosen = Chosen {
                vtlook: Some(look),
                table: Some(std::sync::Arc::new(table)),
            };
            (chosen, None)
        }
        Err(failure) => (Chosen::default(), vtlook::log_line(&failure, head)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> Config {
        crate::config::load(&[], &mut |_: &std::path::Path| {
            Ok(Some(text.as_bytes().to_vec()))
        })
        .config
    }

    // With no vtlook named, or `default`, the head draws the built-in
    // vtlook and vtium reads and logs nothing.
    #[test]
    fn the_built_in_vtlook_reads_no_file() {
        for text in ["", "vtlook = default\n"] {
            let c = config(text);
            assert_eq!(vtlook::chosen(c.text(&keys::VTLOOK)), None, "{text:?}");
            assert_eq!(choose("console", &c), Chosen::default());
        }
        // the notice screen has no vtlook whatever the file names
        assert_eq!(choose("", &config("vtlook = amber\n")), Chosen::default());
    }

    // A look's own vtlook wins over the top level's, and a head whose look
    // names none takes the top level's, through the heads a start makes.
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_look_s_vtlook_wins_over_the_top_level_s() {
        let text = "vtlook = top\nversion = 1\n[look warm]\nvtlook = amber\n[look plain]\n[head a]\nlook = warm\n[head b]\nlook = plain\n";
        let path = std::path::Path::new("/etc/vtium/vtium.conf");
        let top = crate::config::load(&[], &mut |p: &std::path::Path| {
            Ok((p == path).then(|| text.as_bytes().to_vec()))
        })
        .config;
        let rules = vtium_settings::sections(text.as_bytes())
            .map(|s| s.parsed.config)
            .unwrap_or_default();
        let clock: crate::term::Clock = std::rc::Rc::new(std::time::Instant::now);
        let caps = crate::head::session::Caps { head: 4, total: 8 };
        let heads = crate::head::session::heads(&rules, &top, ((100, 100), 1 << 20), caps, &clock);
        let vtlooks: Vec<Option<String>> = heads
            .iter()
            .map(|h| {
                top.for_head(path, &h.settings)
                    .0
                    .text(&keys::VTLOOK)
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(vtlooks, [Some("amber".to_owned()), Some("top".to_owned())]);
    }

    // A look's own palette wins over the top level's, and a head whose
    // look names none takes the top level's. The palette is the head's
    // configured one: its first console starts with it, and so does each
    // console the head opens later.
    // covers: color.palette.named
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_look_s_palette_wins_over_the_top_level_s() {
        let text = "palette = solarized\nversion = 1\n[look warm]\npalette = nord\n[look plain]\n[head a]\nlook = warm\n[head b]\nlook = plain\n";
        let path = std::path::Path::new("/etc/vtium/vtium.conf");
        let top = crate::config::load(&[], &mut |p: &std::path::Path| {
            Ok((p == path).then(|| text.as_bytes().to_vec()))
        })
        .config;
        let rules = vtium_settings::sections(text.as_bytes())
            .map(|s| s.parsed.config)
            .unwrap_or_default();
        let clock: crate::term::Clock = std::rc::Rc::new(std::time::Instant::now);
        let caps = crate::head::session::Caps { head: 4, total: 8 };
        let heads = crate::head::session::heads(&rules, &top, ((100, 100), 1 << 20), caps, &clock);
        let named = |n: &str| Palette::named(n).unwrap_or_default();
        let drawn: Vec<Palette> = heads
            .iter()
            .map(|h| h.console.term.terminal.palette().clone())
            .collect();
        assert_eq!(drawn, [named("nord"), named("solarized")]);
        let configured: Vec<&Palette> = heads.iter().map(|h| &h.palette).collect();
        assert_eq!(configured, [&named("nord"), &named("solarized")]);
        // a console the head opens later starts with the head's palette,
        // though its terminal is made from the top level's configuration
        let later = crate::head::terminal(&top, (0, 0), 0, std::rc::Rc::clone(&clock));
        let opened = heads.first().map(|h| h.fresh_console(9, later));
        assert_eq!(
            opened.map(|c| c.term.terminal.palette().clone()),
            Some(named("nord"))
        );
    }

    // A look whose palette file no folder holds draws the top level's
    // palette, not legacy: the head, its first console and each console it
    // opens later. config/palette.rs tests a file that fails.
    // covers: color.palette.named
    #[test]
    #[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
    fn a_look_s_failing_palette_file_draws_the_top_level_s_palette() {
        let text = "palette = solarized\nversion = 1\n[look odd]\npalette = no-such-scheme-zz\n[head a]\nlook = odd\n";
        let path = std::path::Path::new("/etc/vtium/vtium.conf");
        let top = crate::config::load(&[], &mut |p: &std::path::Path| {
            Ok((p == path).then(|| text.as_bytes().to_vec()))
        })
        .config;
        let rules = vtium_settings::sections(text.as_bytes())
            .map(|s| s.parsed.config)
            .unwrap_or_default();
        let clock: crate::term::Clock = std::rc::Rc::new(std::time::Instant::now);
        let caps = crate::head::session::Caps { head: 4, total: 8 };
        let heads = crate::head::session::heads(&rules, &top, ((100, 100), 1 << 20), caps, &clock);
        let solarized = Palette::named("solarized").unwrap_or_default();
        assert_eq!(heads.len(), 1);
        let Some(head) = heads.first() else { return };
        assert_eq!(head.palette, solarized);
        assert_eq!(head.console.term.terminal.palette(), &solarized);
        let later = crate::head::terminal(&top, (0, 0), 0, std::rc::Rc::clone(&clock));
        assert_eq!(
            head.fresh_console(9, later).term.terminal.palette(),
            &solarized
        );
    }

    // A build without frame filters reads no vtlook and says so once.
    #[cfg(not(feature = "filters"))]
    #[test]
    fn a_build_without_filters_logs_one_line_and_draws_the_built_in_vtlook() {
        assert_eq!(
            read("console", "amber"),
            (
                Chosen::default(),
                Some("This build of vtium has no frame filters, so head console draws the built-in vtlook instead of vtlook amber.".to_owned())
            )
        );
    }

    // A vtlook that reads cleanly is the head's; one that fails at any
    // line leaves the head on the built-in vtlook, with one line naming
    // the first error and counting the rest.
    #[cfg(feature = "filters")]
    #[test]
    fn a_vtlook_that_fails_leaves_the_head_on_the_built_in_vtlook() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = std::env::temp_dir().join(format!("vtium-look-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
        let me = std::fs::metadata(&dir).map_or(0, |m| m.uid());
        let trust = vtium_settings::Trust::owner(me).below(&dir);
        let write = |name: &str, text: &str| {
            let path = dir.join(format!("{name}.vtlook"));
            std::fs::write(&path, text).unwrap_or_default();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        };
        write("plain", "version = 1\n[requires]\nmin-depth = 8\n");
        let (chosen, said) = read_in(
            "console",
            "plain",
            &[dir.as_path()],
            &trust,
            &Cubes::active(&dir, &[]).gate(),
        );
        assert_eq!(said, None);
        assert!(chosen.vtlook.is_some_and(|l| !l.holds_filters()));
        write(
            "crt",
            "version = 1\n[requires]\nmin-depth = 12\n[color sepia]\n[spatial scanlines]\nperiod = 12\n",
        );
        let (chosen, said) = read_in(
            "console",
            "crt",
            &[dir.as_path()],
            &trust,
            &Cubes::active(&dir, &[]).gate(),
        );
        assert_eq!(chosen, Chosen::default());
        assert_eq!(
            said,
            Some(format!(
                "vtlook crt ({}:3): requires: checking: min-depth = 12, needs any, 1, 8, 16 or 24; 2 more errors; head console draws the built-in vtlook instead",
                dir.join("crt.vtlook").display()
            ))
        );
        let (chosen, said) = read_in(
            "desk",
            "gone",
            &[dir.as_path()],
            &trust,
            &Cubes::active(&dir, &[]).gate(),
        );
        assert_eq!(chosen, Chosen::default());
        assert!(
            said.as_deref()
                .is_some_and(|s| s.contains("reading: There is no file gone.vtlook in")
                    && s.ends_with("head desk draws the built-in vtlook instead")),
            "{said:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A vtlook's color filters are merged into its table once, when vtium
    // reads it; a vtlook with none has no table. Each display that meets
    // the requirements takes the table; one that does not draws the
    // built-in vtlook, and the line names the display, its mode and depth,
    // and the first requirement it misses.
    #[cfg(feature = "filters")]
    #[test]
    fn each_display_that_meets_the_requirements_takes_the_vtlook_s_table() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = std::env::temp_dir().join(format!("vtium-look-table-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
        let me = std::fs::metadata(&dir).map_or(0, |m| m.uid());
        let trust = vtium_settings::Trust::owner(me).below(&dir);
        let write = |name: &str, text: &str| {
            let path = dir.join(format!("{name}.vtlook"));
            std::fs::write(&path, text).unwrap_or_default();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        };
        write(
            "amber",
            "version = 1\n[requires]\nmin-depth = 16\nmax-size = 1920x1080\nmin-dpi = 90\nmax-dpi = 200\ngpu = any\n[color tint]\ncolor = 255,176,0\n",
        );
        write("plain", "version = 1\n");
        let (plain, said) = read_in(
            "console",
            "plain",
            &[dir.as_path()],
            &trust,
            &Cubes::active(&dir, &[]).gate(),
        );
        assert_eq!((plain.table.is_none(), said), (true, None));
        let (chosen, said) = read_in(
            "console",
            "amber",
            &[dir.as_path()],
            &trust,
            &Cubes::active(&dir, &[]).gate(),
        );
        assert_eq!(said, None);
        let want = chosen
            .vtlook
            .as_ref()
            .map(vtlook::table)
            .and_then(Result::ok);
        assert_eq!(chosen.table.as_deref(), want.as_ref());
        assert!(want.is_some());
        let display = Display {
            name: "DP-1",
            size: (1280, 800),
            depth: vtlook::Depth::TwentyFour,
            dpi: 96,
            gpu: true,
        };
        let (table, line) = chosen.table_for(&display);
        assert_eq!((table.as_deref(), line), (want.as_ref(), None));
        let missed = |d: Display<'_>| chosen.table_for(&d);
        let cases = [
            (
                Display {
                    depth: vtlook::Depth::One,
                    ..display
                },
                "vtlook amber: display DP-1 (1280x800, 1 bit) draws the built-in vtlook: requires min-depth 16",
            ),
            (
                Display {
                    size: (2560, 1080),
                    ..display
                },
                "vtlook amber: display DP-1 (2560x1080, 24 bits) draws the built-in vtlook: requires max-size 1920x1080",
            ),
            (
                Display { dpi: 72, ..display },
                "vtlook amber: display DP-1 (1280x800, 24 bits) draws the built-in vtlook: requires min-dpi 90",
            ),
            (
                Display {
                    dpi: 220,
                    ..display
                },
                "vtlook amber: display DP-1 (1280x800, 24 bits) draws the built-in vtlook: requires max-dpi 200",
            ),
        ];
        for (d, want_line) in cases {
            assert_eq!(missed(d), (None, Some(want_line.to_owned())), "{d:?}");
        }
        // the path, and the ends of each pair, inclusive
        let mut requires = vtlook::Requires {
            gpu: vtlook::Gpu::Yes,
            ..vtlook::Requires::default()
        };
        let cpu = Display {
            gpu: false,
            ..display
        };
        assert_eq!(unmet(&requires, &cpu).as_deref(), Some("gpu yes"));
        requires.gpu = vtlook::Gpu::No;
        assert_eq!(unmet(&requires, &display).as_deref(), Some("gpu no"));
        let edges = vtlook::Requires {
            min_depth: Some(vtlook::Depth::TwentyFour),
            max_depth: Some(vtlook::Depth::Sixteen),
            min_size: Some(vtlook::Size {
                width: 1280,
                height: 800,
            }),
            ..vtlook::Requires::default()
        };
        assert_eq!(unmet(&edges, &display).as_deref(), Some("max-depth 16"));
        let edges = vtlook::Requires {
            max_depth: None,
            ..edges
        };
        assert_eq!(unmet(&edges, &display), None);
        let small = Display {
            size: (1280, 799),
            ..display
        };
        assert_eq!(unmet(&edges, &small).as_deref(), Some("min-size 1280x800"));
        // the built-in vtlook has no table on any display
        assert_eq!(Chosen::default().table_for(&display), (None, None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The `.cube` files a test's vtlooks may read: those named in
    /// `active` in the folder `dir`, under a trust rule that takes the
    /// test's own user's files below `dir`, and this machine's mount table.
    #[cfg(feature = "filters")]
    struct Cubes {
        dir: std::path::PathBuf,
        active: vtium_settings::cullet::Gate,
    }

    #[cfg(feature = "filters")]
    impl Cubes {
        fn active(dir: &std::path::Path, active: &[&str]) -> Cubes {
            use std::os::unix::fs::MetadataExt;
            use vtium_settings::batch::{Batch, Entry};
            let me = std::fs::metadata(dir).map_or(0, |m| m.uid());
            Cubes {
                dir: dir.to_path_buf(),
                active: vtium_settings::cullet::Gate::new(
                    vtium_settings::cullet::Rule::new(vtium_settings::Trust::owner(me)).below(dir),
                    Some(vtium_settings::mounts::Mounts::system().unwrap_or_default()),
                    Batch::new(active.iter().map(|n| Entry::Path(dir.join(n))).collect()),
                ),
            }
        }

        fn gate(&self) -> vtlook::Cubes<'_> {
            vtlook::Cubes {
                dir: &self.dir,
                gate: &self.active,
            }
        }
    }

    // A vtlook whose cube filter names an active .cube file that passes
    // the trust rule draws through the file's table. One that names a
    // file the batch does not list draws the built-in vtlook, and the line
    // names the file and says why vtium treats it as missing.
    #[cfg(feature = "filters")]
    #[test]
    fn a_vtlook_draws_an_active_cube_and_not_an_inactive_one() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("vtium-look-cube-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
        let write = |name: &str, text: &str| {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap_or_default();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        };
        write(
            "warm.cube",
            "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 0.95 0\n1 0.95 0\n0 0 0.85\n1 0 0.85\n0 0.95 0.85\n1 0.95 0.85\n",
        );
        write(
            "film.vtlook",
            "version = 1\n[color cube]\nfile = warm.cube\n",
        );
        let cubes = Cubes::active(&dir, &["warm.cube"]);
        let me = std::fs::metadata(&dir).map_or(0, |m| std::os::unix::fs::MetadataExt::uid(&m));
        let trust = vtium_settings::Trust::owner(me).below(&dir);
        let (chosen, said) = read_in("console", "film", &[dir.as_path()], &trust, &cubes.gate());
        assert_eq!(said, None);
        let cube = std::fs::read(dir.join("warm.cube")).unwrap_or_default();
        let want = vtlook::read_cube(&cube).ok().and_then(|cube| {
            let map = vtium_render::filter::Map::cube(std::sync::Arc::new(cube));
            vtium_render::filter::Table::build(&[map]).ok()
        });
        assert!(want.is_some());
        assert_eq!(chosen.table.as_deref(), want.as_ref());
        let inactive = Cubes::active(&dir, &["other.cube"]);
        let (chosen, said) = read_in(
            "console",
            "film",
            &[dir.as_path()],
            &trust,
            &inactive.gate(),
        );
        assert_eq!(chosen, Chosen::default());
        assert_eq!(
            said,
            Some(format!(
                "vtlook film ({}): color filter 1 of 1, cube: reading: vtium treats the file as missing, because it is not active: {} does not list it.; head console draws the built-in vtlook instead",
                dir.join("warm.cube").display(),
                vtium_settings::batch::BATCH_FILE
            ))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A head reads a vtlook from the administrator's folder, and failing
    // that from the package's: a preset with no copy in the
    // administrator's folder draws, and a copy there takes its place.
    #[cfg(feature = "filters")]
    #[test]
    fn a_preset_draws_with_no_copy_and_the_administrator_s_copy_wins() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let top = std::env::temp_dir().join(format!("vtium-look-presets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (etc, share) = (top.join("etc"), top.join("share"));
        for dir in [&top, &etc, &share] {
            std::fs::create_dir_all(dir).unwrap_or_default();
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
        }
        let me = std::fs::metadata(&top).map_or(0, |m| m.uid());
        let trust = vtium_settings::Trust::owner(me).below(&top);
        let write = |dir: &std::path::Path, text: &str| {
            let path = dir.join("amber.vtlook");
            std::fs::write(&path, text).unwrap_or_default();
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        };
        let folders = [etc.as_path(), share.as_path()];
        let filters = |chosen: &Chosen| {
            chosen.vtlook.as_ref().map(|l| {
                l.color
                    .iter()
                    .map(|f| f.declaration.name.to_owned())
                    .collect::<Vec<_>>()
            })
        };
        write(&share, "version = 1\n[color tint]\ncolor = 255,176,0\n");
        let (chosen, said) = read_in(
            "console",
            "amber",
            &folders,
            &trust,
            &Cubes::active(&etc, &[]).gate(),
        );
        assert_eq!(said, None);
        assert_eq!(filters(&chosen), Some(vec!["tint".to_owned()]));
        assert!(chosen.table.is_some());
        write(&etc, "version = 1\n[color gamma]\n");
        let (chosen, said) = read_in(
            "console",
            "amber",
            &folders,
            &trust,
            &Cubes::active(&etc, &[]).gate(),
        );
        assert_eq!(said, None);
        assert_eq!(filters(&chosen), Some(vec!["gamma".to_owned()]));
        let _ = std::fs::remove_dir_all(&top);
    }
}
