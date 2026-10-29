//! `vtium check-config`: the configuration read as vtium would read it at
//! start, with every warning, and nothing applied. It opens no VT,
//! display, input device or tty, and reads only the configuration files,
//! so it runs unprivileged (the operator's decision on vtium/vtium#21).

use std::path::Path;

use crate::config::{Config, Loaded, keys};
use crate::font;

/// The subcommand's name, which stands among the options.
pub const SUBCOMMAND: &str = "check-config";

/// The subcommand's help.
pub const HELP: &str = "Usage: vtium check-config [OPTION]...\n\n\
Reads the configuration as vtium would start with these options:\n\
kmscon.conf and the seat's file in the configuration directory, then the\n\
options. It prints each warning and exits 1 when there is any, 0 when\n\
there is none, and notes on standard output the density font sizes in\n\
points convert at, which the start reads from each display.\n\
It only checks: it opens no VT, display or input device, applies\n\
nothing, and reads nothing but the configuration files and the frit\n\
font-name names.\n";

/// What a value means beyond its own key, which the start reports too.
pub fn semantics(config: &Config) -> Vec<String> {
    // Any user may read PID 1's status; `SigCgt` says what it catches.
    let status = std::fs::read_to_string("/proc/1/status").map_err(|e| e.to_string());
    let mut out = semantics_under(config, &status, vtium_platform::power::power_off_signal());
    out.extend(frit(config, &font::default_frits()));
    out
}

/// What the frit `font-name` names in `frits` says of its cell, as
/// [`font::frit_warnings`] reads it; nothing when the name is no frit's.
fn frit(config: &Config, frits: &Path) -> Vec<String> {
    let Some(wanted) = font::wanted(config, "", "").filter(|w| w.came == font::Came::Config) else {
        return Vec::new();
    };
    let Some(path) = font::frit(frits, &wanted.name) else {
        return Vec::new();
    };
    match font::frit_text(&path) {
        Ok(text) => font::frit_warnings(&wanted.name, &text),
        Err(why) => vec![format!(
            "the frit {} cannot be read ({why}); vtium draws its built-in font",
            wanted.name
        )],
    }
}

/// Whether PID 1 catches `signal`, from the text of `/proc/1/status`:
/// the `SigCgt` line is a hex mask with bit `signal - 1` set for a caught
/// signal. `Err` names why the text does not answer.
fn init_catches(status: &str, signal: i32) -> Result<bool, String> {
    let mask = status
        .lines()
        .find_map(|line| line.strip_prefix("SigCgt:"))
        .ok_or("it has no SigCgt line")?;
    let mask = u64::from_str_radix(mask.trim(), 16)
        .map_err(|_| format!("its SigCgt line is not a hex mask: {:?}", mask.trim()))?;
    let bit = u32::try_from(signal).ok().and_then(|n| n.checked_sub(1));
    Ok(bit
        .and_then(|b| mask.checked_shr(b))
        .is_some_and(|m| m & 1 == 1))
}

/// [`semantics`] where `status` is what reading `/proc/1/status` gave (the
/// text, or the error's message) and `signal` is the power-off signal.
fn semantics_under(config: &Config, status: &Result<String, String>, signal: i32) -> Vec<String> {
    let mut out = Vec::new();
    if config.switch(&keys::HWACCEL) {
        out.push("hwaccel is not built yet; vtium draws in software".to_owned());
    }
    if config.switch(&keys::USE_ORIGINAL_MODE) && !config.modes(&keys::MODE).is_empty() {
        out.push("mode is set, so use-original-mode is off".to_owned());
    }
    if !config.chords(&keys::GRAB_POWER_OFF).is_empty() {
        let caught = match status {
            Ok(text) => init_catches(text, signal).map_err(|why| format!("/proc/1/status: {why}")),
            Err(why) => Err(format!("/proc/1/status could not be read: {why}")),
        };
        match caught {
            Ok(true) => {}
            Ok(false) => out.push(format!(
                "grab-power-off is bound, and PID 1 does not catch SIGRTMIN+4 (signal {signal}, \
                 from SigCgt in /proc/1/status): it powers nothing off"
            )),
            Err(why) => out.push(format!(
                "grab-power-off is bound, and whether PID 1 catches SIGRTMIN+4 (signal {signal}) \
                 is unknown, since {why}"
            )),
        }
    }
    out
}

/// Every warning the start would give for `loaded`: the configuration's
/// reports, then [`semantics`].
pub fn warnings(loaded: &Loaded) -> Vec<String> {
    let mut out: Vec<String> = loaded.reports.iter().map(ToString::to_string).collect();
    out.extend(semantics(&loaded.config));
    out.extend(crate::config::budget::memory_warning());
    out
}

/// What the subcommand reports beyond the warnings, on standard output
/// and outside the exit status: the density points convert at,
/// `display-dpi`'s, else each display's, which only the start reads, since
/// the subcommand opens no display (the operator's decision on
/// vtium/vtium#145).
pub fn notes(config: &Config) -> Vec<String> {
    let density = match config.number(&keys::DISPLAY_DPI) {
        Some(dpi) => format!(
            "display-dpi={dpi}: every display converts points at {dpi} DPI, its measured density aside"
        ),
        None => "each display converts points at the density it measures from its physical size, else at 96 DPI with a warning naming it; the start logs which".to_owned(),
    };
    vec![density]
}

/// The subcommand on the configuration `loaded`, which the command line
/// named it in: the lines to print and the exit status.
pub fn run(loaded: &Loaded) -> (Vec<String>, i32) {
    if loaded.config.switch(&keys::HELP) {
        return (vec![HELP.trim_end().to_owned()], 0);
    }
    let lines = warnings(loaded);
    let status = i32::from(!lines.is_empty());
    (lines, status)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::config;

    fn run_with(args: &[&str], conf: &str) -> (Vec<String>, i32) {
        let args: Vec<OsString> = [SUBCOMMAND]
            .iter()
            .chain(args)
            .map(OsString::from)
            .collect();
        let conf = conf.as_bytes().to_vec();
        let loaded = config::load(&args, Some("seat0"), &mut |p: &Path| {
            Ok((p == Path::new("/etc/kmscon/kmscon.conf")).then(|| conf.clone()))
        });
        assert!(loaded.check);
        run(&loaded)
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
                "argument 2: unknown key \"--bogus\", ignored",
                "/etc/kmscon/kmscon.conf:1: sb-size=\"200000\" is out of bounds, 100000 used",
                "/etc/kmscon/kmscon.conf:2: unknown key \"frob\", ignored",
            ]
        );
    }

    #[test]
    fn what_the_values_mean_together_is_checked_too() {
        let (lines, status) = run_with(&[], "mode=1024x768\nuse-original-mode\n");
        assert_eq!(status, 1);
        assert_eq!(lines, ["mode is set, so use-original-mode is off"]);
    }

    #[test]
    fn a_palette_vtium_does_not_hold_is_reported() {
        let (lines, status) = run_with(&["--palette=frob"], "palette=nord\n");
        assert_eq!(status, 1);
        assert_eq!(
            lines,
            [
                "argument 2: palette=\"frob\" is none of legacy, custom, nord, solarized, \
                 solarized-black, solarized-white, soft-black, base16-dark, base16-light, vga, \
                 ignored"
            ]
        );
        for name in vtium_term::PALETTE_NAMES.iter().chain(&["custom"]) {
            let conf = format!("palette={name}\n");
            assert_eq!(run_with(&[], &conf), (Vec::new(), 0), "{name}");
        }
    }

    #[test]
    fn a_key_not_built_yet_warns() {
        let (lines, status) = run_with(&[], "hwaccel\n");
        assert_eq!(status, 1);
        assert_eq!(lines, ["hwaccel is not built yet; vtium draws in software"]);
        assert_eq!(run_with(&[], "no-hwaccel\n"), (Vec::new(), 0));
    }

    fn status(cgt: &str) -> Result<String, String> {
        Ok(format!(
            "Name:\tinit\nSigIgn:\t0000000000001000\nSigCgt:\t{cgt}\nCapInh:\t0\n"
        ))
    }

    fn power_off_warnings(conf: &str, status: &Result<String, String>) -> Vec<String> {
        let args = [OsString::from(SUBCOMMAND)];
        let conf = conf.as_bytes().to_vec();
        let loaded = config::load(&args, None, &mut |_: &Path| Ok(Some(conf.clone())));
        semantics_under(&loaded.config, status, 38)
    }

    const BOUND: &str = "grab-power-off=<Ctrl><Alt>End\n";

    #[test]
    fn grab_power_off_is_silent_where_init_catches_the_signal() {
        // Both masks have bit 37 (signal 38) set.
        for cgt in ["00000021fdc1fefb", "0000002000014a03"] {
            assert_eq!(
                power_off_warnings(BOUND, &status(cgt)),
                [] as [&str; 0],
                "{cgt}"
            );
        }
    }

    #[test]
    fn grab_power_off_warns_where_init_catches_nothing_of_it() {
        assert_eq!(
            power_off_warnings(BOUND, &status("0000000000000000")),
            [
                "grab-power-off is bound, and PID 1 does not catch SIGRTMIN+4 (signal 38, \
                 from SigCgt in /proc/1/status): it powers nothing off"
            ]
        );
        // Bit 38 (signal 39) is a neighbour, not the signal.
        assert_eq!(
            power_off_warnings(BOUND, &status("0000004000000000")).len(),
            1
        );
    }

    #[test]
    fn grab_power_off_names_why_an_unreadable_status_does_not_answer() {
        let unreadable = Err("Permission denied (os error 13)".to_owned());
        assert_eq!(
            power_off_warnings(BOUND, &unreadable),
            [
                "grab-power-off is bound, and whether PID 1 catches SIGRTMIN+4 (signal 38) is \
                 unknown, since /proc/1/status could not be read: Permission denied (os error 13)"
            ]
        );
        let no_line = Ok("Name:\tinit\n".to_owned());
        let says = |lines: Vec<String>, what: &str| lines.iter().all(|l| l.contains(what));
        assert!(says(
            power_off_warnings(BOUND, &no_line),
            "it has no SigCgt line"
        ));
        assert!(says(
            power_off_warnings(BOUND, &status("zz")),
            "not a hex mask"
        ));
        assert_eq!(power_off_warnings(BOUND, &no_line).len(), 1);
        assert_eq!(power_off_warnings(BOUND, &status("zz")).len(), 1);
    }

    #[test]
    fn an_unbound_grab_power_off_checks_nothing() {
        let unbound = "grab-reboot=<Ctrl><Alt>Delete\n";
        assert_eq!(power_off_warnings(unbound, &status("0")), [] as [&str; 0]);
        assert_eq!(
            power_off_warnings(unbound, &Err("x".to_owned())),
            [] as [&str; 0]
        );
    }

    // #145: a frit's cell that does not suit it, where no font need be
    // parsed to tell
    #[test]
    #[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
    fn the_frit_font_name_names_is_checked() {
        let dir = std::env::temp_dir().join(format!("vtium-check-frit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("small.vtfrit"),
            "source=builtin:unifont\ncell=8x12\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("fine.vtfrit"),
            "source=outline:/f.ttf\ncell=10x20\n",
        )
        .unwrap();
        let config = |name: &str| {
            let args: Vec<OsString> = [SUBCOMMAND, "--font-name", name]
                .iter()
                .map(OsString::from)
                .collect();
            config::load(&args, Some("seat0"), &mut |_: &Path| Ok(None)).config
        };
        assert_eq!(
            frit(&config("small"), &dir),
            [
                "the frit small: line 1: source=builtin:unifont's 8x16 cell does not fit its 8x12 cell, so it gives nothing"
            ]
        );
        assert!(frit(&config("fine"), &dir).is_empty());
        assert!(frit(&config("ter-v16n"), &dir).is_empty(), "no frit");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // the operator on #145, 2026-10-28: the density points convert at,
    // display-dpi's, else each display's measured or assumed, which only
    // the start can read, since check-config opens no display
    #[test]
    fn the_density_source_is_noted_outside_the_status() {
        let note = |args: &[&str]| {
            let args: Vec<OsString> = [SUBCOMMAND]
                .iter()
                .chain(args)
                .map(OsString::from)
                .collect();
            notes(&config::load(&args, None, &mut |_: &Path| Ok(None)).config)
        };
        assert_eq!(
            note(&["--display-dpi=109"]),
            [
                "display-dpi=109: every display converts points at 109 DPI, its measured density aside"
            ]
        );
        assert_eq!(
            note(&[]),
            [
                "each display converts points at the density it measures from its physical size, else at 96 DPI with a warning naming it; the start logs which"
            ]
        );
        assert_eq!(run_with(&["--display-dpi=109"], ""), (Vec::new(), 0));
    }

    #[test]
    fn help_says_it_only_checks() {
        let (lines, status) = run_with(&["--help"], "");
        assert_eq!(status, 0);
        assert_eq!(lines, [HELP.trim_end()]);
        assert!(HELP.contains("It only checks"));
    }
}
