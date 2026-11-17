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
//! A build without the `filters` feature reads no vtlook. It logs once
//! for each head whose configuration names one, and draws the built-in
//! vtlook, as the build is meant to.
//!
//! References:
//! - ADR-1304 D5: a head's vtlook comes from its look, else the top level, else the built-in one.
//! - ADR-1304 D6: any error drops the head to the built-in vtlook, with one log line, and no retry.
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
    match vtlook::read(name, dir, trust) {
        Ok(look) => (Chosen { vtlook: Some(look) }, None),
        Err(reports) => (Chosen::default(), vtlook::log_line(&reports, head)),
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
                Some("This build of vtium has no frame filters, so head console draws the vtlook default (built in) and not vtlook amber.".to_owned())
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
                "vtlook crt ({}:3): requires: checking: min-depth = 12, needs any, 1, 8, 16 or 24; 2 more errors; head console draws the vtlook default (built in)",
                dir.join("crt.vtlook").display()
            ))
        );
        let (chosen, said) = read_in("desk", "gone", &dir, &trust);
        assert_eq!(chosen, Chosen::default());
        assert!(
            said.as_deref()
                .is_some_and(|s| s.contains("reading: found no such file")
                    && s.ends_with("head desk draws the vtlook default (built in)")),
            "{said:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
