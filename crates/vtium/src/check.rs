//! `vtium check-config`: the configuration read as vtium would read it at
//! start, with every warning, and nothing applied. It opens no VT,
//! display, input device or tty, and reads only the configuration files,
//! so it runs unprivileged (the operator's decision on vtium/vtium#21).

use std::ffi::OsString;
use std::io;
use std::path::Path;

use crate::config::{self, Config, Loaded, keys};

/// The subcommand's name, the first argument.
pub const SUBCOMMAND: &str = "check-config";

/// The subcommand's help.
pub const HELP: &str = "Usage: vtium check-config [OPTION]...\n\n\
Reads the configuration as vtium would start with these options:\n\
kmscon.conf and the seat's file in the configuration directory, then the\n\
options. It prints each warning and exits 1 when there is any, 0 when\n\
there is none. It only checks: it opens no VT, display or input device,\n\
applies nothing, and reads nothing but the configuration files.\n";

/// What a value means beyond its own key, which the start reports too.
pub fn semantics(config: &Config) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(name) = config
        .text(&keys::PALETTE)
        .filter(|p| !matches!(*p, "legacy" | "custom"))
    {
        out.push(format!("palette={name} is not built yet; legacy is used"));
    }
    if config.switch(&keys::HWACCEL) {
        out.push("hwaccel is not built yet; vtium draws in software".to_owned());
    }
    if config.switch(&keys::USE_ORIGINAL_MODE) && !config.modes(&keys::MODE).is_empty() {
        out.push("mode is set, so use-original-mode is off".to_owned());
    }
    out
}

/// Every warning the start would give for `loaded`: the configuration's
/// reports, then [`semantics`].
pub fn warnings(loaded: &Loaded) -> Vec<String> {
    let mut out: Vec<String> = loaded.reports.iter().map(ToString::to_string).collect();
    out.extend(semantics(&loaded.config));
    out
}

/// Runs the subcommand on `args` (after its name) with `read` for the
/// files: the lines to print and the exit status.
pub fn run(
    args: &[OsString],
    read: &mut dyn FnMut(&Path) -> io::Result<Option<Vec<u8>>>,
) -> (Vec<String>, i32) {
    let loaded = config::load(args, Some("seat0"), read);
    if loaded.config.switch(&keys::HELP) {
        return (vec![HELP.trim_end().to_owned()], 0);
    }
    let lines = warnings(&loaded);
    let status = i32::from(!lines.is_empty());
    (lines, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with(args: &[&str], conf: &str) -> (Vec<String>, i32) {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let conf = conf.as_bytes().to_vec();
        run(&args, &mut |p: &Path| {
            Ok((p == Path::new("/etc/kmscon/kmscon.conf")).then(|| conf.clone()))
        })
    }

    #[test]
    fn a_clean_configuration_exits_zero() {
        let (lines, status) = run_with(&[], "font-size=16\nno-blink\n");
        assert_eq!((lines, status), (Vec::<String>::new(), 0));
    }

    #[test]
    fn each_warning_is_printed_and_the_status_is_one() {
        let (lines, status) = run_with(&["--bogus"], "sb-size=200000\nfrob=1\n");
        assert_eq!(status, 1);
        assert_eq!(
            lines,
            [
                "argument 1: unknown key \"--bogus\", ignored",
                "/etc/kmscon/kmscon.conf:1: sb-size=\"200000\" is out of bounds, 100000 used",
                "/etc/kmscon/kmscon.conf:2: unknown key \"frob\", ignored",
            ]
        );
    }

    #[test]
    fn what_the_values_mean_together_is_checked_too() {
        let (lines, status) = run_with(&[], "palette=nord\nmode=1024x768\nuse-original-mode\n");
        assert_eq!(status, 1);
        assert_eq!(
            lines,
            [
                "palette=nord is not built yet; legacy is used",
                "mode is set, so use-original-mode is off",
            ]
        );
    }

    #[test]
    fn a_key_not_built_yet_warns() {
        let (lines, status) = run_with(&[], "hwaccel\n");
        assert_eq!(status, 1);
        assert_eq!(lines, ["hwaccel is not built yet; vtium draws in software"]);
        assert_eq!(run_with(&[], "no-hwaccel\n"), (Vec::new(), 0));
    }

    #[test]
    fn help_says_it_only_checks() {
        let (lines, status) = run_with(&["--help"], "");
        assert_eq!(status, 0);
        assert_eq!(lines, [HELP.trim_end()]);
        assert!(HELP.contains("It only checks"));
    }
}
