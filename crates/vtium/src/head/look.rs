//! The vtlook a head draws with: the one its configuration names, read
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
//! not an error of the vtlook.
//!
//! A build without the `filters` feature reads no vtlook. It logs once
//! for each head whose configuration names one, and draws the built-in
//! vtlook, as the build is meant to.
//!
//! References:
//! - ADR-1304 D5: a head's vtlook comes from its look, else the top level, else the built-in one.
//! - ADR-1304 D2: the table is built on the CPU once for each head.
//! - ADR-1304 D6: any error drops the head to the built-in vtlook, with one log line, and no retry.
//! - ADR-1304 D7: a display that does not meet the requirements draws the built-in vtlook, with one log line.
//! - ADR-1304 D10: a build without `filters` logs once and draws the built-in vtlook.
//! - ADR-1304 D12: the built-in vtlook reads no file.

use crate::config::{Config, keys};
use vtium_settings::vtlook;

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
                let (w, h) = display.size;
                let depth = match display.depth {
                    vtlook::Depth::One => "1 bit",
                    vtlook::Depth::Gray => "8-bit gray",
                    vtlook::Depth::Sixteen => "16 bits",
                    vtlook::Depth::TwentyFour => "24 bits",
                };
                let line = format!(
                    "vtlook {}: display {} ({w}x{h}, {depth}) draws the built-in vtlook: requires {why}",
                    look.name, display.name
                );
                (None, Some(line))
            }
        }
    }
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

/// Reads the vtlook `name` for head `head` from the looks folder, as the
/// account vtium runs as reads it, with the line to log, if any.
#[cfg(feature = "filters")]
fn read(head: &str, name: &str) -> (Chosen, Option<String>) {
    read_in(
        head,
        name,
        std::path::Path::new(vtlook::LOOKS),
        &crate::font::trust(),
    )
}

/// What a build without frame filters does with the vtlook `name`: it
/// reads nothing and says so.
#[cfg(not(feature = "filters"))]
fn read(head: &str, name: &str) -> (Chosen, Option<String>) {
    (Chosen::default(), Some(vtlook::no_filters(head, name)))
}

/// Reads the vtlook `name` from the folder `dir` under `trust`: the vtlook
/// read, or the built-in one with the line that says why.
#[cfg(feature = "filters")]
fn read_in(
    head: &str,
    name: &str,
    dir: &std::path::Path,
    trust: &vtium_settings::Trust,
) -> (Chosen, Option<String>) {
    let look = match vtlook::read(name, dir, trust) {
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
    match vtlook::table(&look) {
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
        let (chosen, said) = read_in("console", "plain", &dir, &trust);
        assert_eq!(said, None);
        assert!(chosen.vtlook.is_some_and(|l| !l.holds_filters()));
        write(
            "crt",
            "version = 1\n[requires]\nmin-depth = 12\n[color sepia]\n[spatial scanlines]\nperiod = 12\n",
        );
        let (chosen, said) = read_in("console", "crt", &dir, &trust);
        assert_eq!(chosen, Chosen::default());
        assert_eq!(
            said,
            Some(format!(
                "vtlook crt ({}:3): requires: checking: min-depth = 12, needs any, 1, 8, 16 or 24; 2 more errors; head console draws the built-in vtlook instead",
                dir.join("crt.vtlook").display()
            ))
        );
        let (chosen, said) = read_in("desk", "gone", &dir, &trust);
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
        let (plain, said) = read_in("console", "plain", &dir, &trust);
        assert_eq!((plain.table.is_none(), said), (true, None));
        let (chosen, said) = read_in("console", "amber", &dir, &trust);
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
}
