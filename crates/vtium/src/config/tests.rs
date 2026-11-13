use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use super::keys::*;
use super::*;
use crate::scratch::Scratch;

/// [`load`] over files held in memory, by path.
fn load_with(args: &[&str], files: &[(&str, &[u8])]) -> Loaded {
    let files: BTreeMap<PathBuf, Vec<u8>> = files
        .iter()
        .map(|(p, b)| (PathBuf::from(p), b.to_vec()))
        .collect();
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    load(&args, &mut |p: &Path| Ok(files.get(p).cloned()))
}

/// The main file alone.
fn main_file(text: &str) -> Loaded {
    load_with(&[], &[("/etc/vtium/vtium.conf", text.as_bytes())])
}

fn problems(l: &Loaded) -> Vec<&Problem> {
    l.reports.iter().map(|r| &r.problem).collect()
}

fn line(n: usize) -> Origin {
    Origin::File {
        path: "/etc/vtium/vtium.conf".into(),
        line: n,
    }
}

// The five guest configurations of tier 3, read from tools/tier3/guest/,
// which installs each as /etc/vtium/vtium.conf.
const TIER3_2D: &str = include_str!("../../../../tools/tier3/guest/vtium.conf");
const TIER3_FONT32: &str = include_str!("../../../../tools/tier3/guest/vtium-font32.conf");
const TIER3_GL: &str = include_str!("../../../../tools/tier3/guest/vtium-gl.conf");
const TIER3_MIN_PSF: &str = include_str!("../../../../tools/tier3/guest/vtium-minimal-psf.conf");
const TIER3_MIN: &str = include_str!("../../../../tools/tier3/guest/vtium-minimal.conf");
const TIER3_POWER: &str = include_str!("../../../../tools/tier3/guest/vtium-power.conf");

// covers: platform.config
#[test]
fn tier3_guest_configurations() {
    // Each file, its font-engine, font-name and font-size, and hwaccel.
    type Case<'a> = (&'a str, Option<&'a str>, Option<&'a str>, u64, bool);
    let cases: [Case; 5] = [
        (TIER3_2D, None, Some("tier3-text"), 16, false),
        (TIER3_FONT32, None, Some("tier3-text"), 32, false),
        (TIER3_GL, None, Some("tier3-text"), 16, true),
        (
            TIER3_MIN_PSF,
            Some("psf"),
            Some("/usr/share/kbd/consolefonts/default8x16.psfu.gz"),
            0,
            false,
        ),
        // no font-name: vconsole's font, else the built-ins (ADR-1200 D9)
        (TIER3_MIN, None, None, 0, false),
    ];
    for (text, engine, name, size, hwaccel) in cases {
        let l = main_file(text);
        assert_eq!(l.reports, [], "{text}");
        let c = &l.config;
        assert_eq!(c.text(&FONT_ENGINE), engine, "{text}");
        assert_eq!(c.text(&FONT_NAME), name, "{text}");
        assert_eq!(c.number(&FONT_SIZE), Some(size), "{text}");
        assert!(!c.switch(&BLINK), "{text}");
        // a configuration that leaves hwaccel unset takes the build's
        // default (ADR-1500 D8)
        assert_eq!(c.switch(&HWACCEL), hwaccel || HWACCEL_DEFAULT, "{text}");
        assert_eq!(c.text(&RENDER_TIMER), Some("log"), "{text}");
    }
}

// covers: platform.config
#[test]
fn tier3_command_lines() {
    // vtium@.service's ExecStart, which tier 3's guest starts on tty2.
    let l = load_with(&["--vt=tty2", "--no-switchvt"], &[]);
    assert_eq!(l.reports, []);
    assert_eq!(l.config.text(&VT), Some("tty2"));
    assert!(!l.config.switch(&SWITCHVT));

    // tools/vt/run-vt.sh's, with the demo as the session.
    let l = load_with(
        &[
            "--vt=tty5",
            "--no-switchvt",
            "--configdir",
            "/review/config",
            "--oneshot",
            "--font-name=Monoid Nerd Font Mono",
            "--login",
            "--",
            "/usr/bin/env",
            "DEMO_LABEL=vtium dev",
            "/bin/bash",
        ],
        &[("/review/config/vtium.conf", b"font-size=24\n")],
    );
    assert_eq!(l.reports, []);
    let c = &l.config;
    assert_eq!(c.text(&VT), Some("tty5"));
    assert_eq!(c.text(&CONFIGDIR), Some("/review/config"));
    assert!(c.switch(&ONESHOT));
    assert_eq!(c.text(&FONT_NAME), Some("Monoid Nerd Font Mono"));
    assert_eq!(c.number(&FONT_SIZE), Some(24));
    assert_eq!(
        c.command(&LOGIN),
        Some(
            &[
                OsString::from("/usr/bin/env"),
                "DEMO_LABEL=vtium dev".into(),
                "/bin/bash".into()
            ][..]
        )
    );
}

#[test]
fn defaults() {
    let c = Config::default();
    assert_eq!(c.text(&CONFIGDIR), Some("/etc/vtium"));
    assert_eq!(c.text(&TERM), Some("xterm-vtium"));
    assert_eq!(c.number(&SB_SIZE), Some(1000));
    assert_eq!(c.number(&SB_SIZE_CAP), None);
    assert_eq!(c.number(&SYNC_TIMEOUT), Some(200));
    assert_eq!(c.number(&DPMS_TIMEOUT), Some(600));
    assert_eq!(c.number(&FONT_SIZE), Some(0));
    // unset: vconsole's font, else the built-ins
    assert_eq!(c.text(&FONT_NAME), None);
    assert_eq!(c.text(&RENDER_TIMER), Some("none"));
    assert_eq!(c.text(&GLYPH_CACHE), None);
    assert_eq!(c.text(&DISPLAY_DEPTH), Some("auto"));
    assert_eq!(c.command(&LOGIN), None);
    for (key, on) in [
        (&SWITCHVT, true),
        (&RESET_ENV, true),
        (&BACKSPACE_DELETE, true),
        (&BLINK, true),
        (&ONESHOT, false),
        (&HWACCEL, HWACCEL_DEFAULT),
        (&USE_ORIGINAL_MODE, false),
        (&HELP, false),
    ] {
        assert_eq!(c.switch(key), on, "{}", key.name);
    }
    assert_eq!(c.origin(&SB_SIZE), Some(&Origin::Default));
}

#[test]
fn command_line_wins_over_the_file() {
    let l = load_with(
        &["--sb-size=300"],
        &[(
            "/etc/vtium/vtium.conf",
            b"sb-size=100\nfont-size=12\nterm=xterm\n",
        )],
    );
    assert_eq!(l.reports, []);
    let c = &l.config;
    assert_eq!(c.number(&SB_SIZE), Some(300));
    assert_eq!(c.origin(&SB_SIZE), Some(&Origin::CommandLine { arg: 1 }));
    assert_eq!(c.number(&FONT_SIZE), Some(12));
    assert_eq!(c.text(&TERM), Some("xterm"));
    // Within one source the last line wins.
    assert_eq!(
        main_file("sb-size=5\nsb-size=6\n").config.number(&SB_SIZE),
        Some(6)
    );
}

// A relative or empty font-frits is reported and ignored, so the
// directory stays where the sources before it put it: the default under
// vtium.conf's, and vtium.conf's under the command line's.
#[test]
fn a_relative_or_empty_font_frits_leaves_the_directory_set_before_it() {
    let l = load_with(
        &["--font-frits=fonts"],
        &[("/etc/vtium/vtium.conf", b"font-frits=/srv/frits\n")],
    );
    assert_eq!(l.config.text(&FONT_FRITS), Some("/srv/frits"));
    assert_eq!(
        problems(&l),
        [&Problem::BadValue {
            key: "font-frits",
            value: "fonts".into(),
            why: NOT_ABSOLUTE,
        }]
    );
    let l = main_file("font-frits=\n");
    assert_eq!(l.config.text(&FONT_FRITS), Some(FRITS));
    let origins: Vec<&Origin> = l.reports.iter().map(|r| &r.origin).collect();
    assert_eq!(origins, [&line(1)]);
    let l = load_with(&["--font-frits=fonts"], &[]);
    assert_eq!(l.config.text(&FONT_FRITS), Some(FRITS));
    assert_eq!(l.reports.len(), 1, "{:?}", l.reports);
}

// ADR-1500 D4, ADR-1403 D2: with no `--configdir`, vtium reads
// `/etc/vtium/vtium.conf` and nothing else: never the seat's file beside
// it, and never `/etc/kmscon`, even where a file is there.
#[test]
fn vtium_conf_is_the_one_file_read() {
    let mut read = Vec::new();
    let l = load(&[], &mut |p: &Path| {
        read.push(p.to_owned());
        let other = p.starts_with("/etc/kmscon") || p.ends_with("seat0.seat.conf");
        Ok(other.then(|| b"sb-size=7\n".to_vec()))
    });
    assert_eq!(read, [PathBuf::from("/etc/vtium/vtium.conf")]);
    assert_eq!(l.reports, []);
    assert_eq!(l.config.number(&SB_SIZE), Some(1000));
    assert_eq!(l.path, Path::new("/etc/vtium/vtium.conf"));
    assert_eq!(l.file, File::Missing);
}

// ADR-1403 D5: a head's section wins over its look's, its look's over
// the top level, and a section wins over the command line as well; a key
// no section sets keeps the top level's value
#[test]
#[expect(clippy::expect_used, reason = "a test's set-up fails loudly")]
fn a_head_s_sections_win_over_the_top_level_and_the_command_line() {
    let text = "font-size=12\nfont-engine=pango\nversion = 1\n\
                [look big]\nfont-size = 14\nfont-engine = freetype\n\
                [head large]\nlook = big\nlogin = /bin/large\n\
                [head small]\n";
    let rules = vtium_settings::sections(text.as_bytes())
        .expect("a flow")
        .parsed
        .config;
    let own = |head: &str, args: &[&str]| {
        let top = load_with(args, &[("/etc/vtium/vtium.conf", text.as_bytes())]).config;
        let h = rules.head(head).expect("the head");
        let settings: Vec<vtium_settings::Setting> = rules
            .settings(h)
            .into_iter()
            .map(|(_, s)| s.clone())
            .collect();
        let (config, reports) = top.for_head(Path::new("/etc/vtium/vtium.conf"), &settings);
        assert_eq!(reports, []);
        config
    };
    let large = own("large", &[]);
    assert_eq!(large.number(&FONT_SIZE), Some(14));
    assert_eq!(large.text(&FONT_ENGINE), Some("freetype"));
    assert_eq!(
        large.origin(&FONT_SIZE),
        Some(&Origin::File {
            path: "/etc/vtium/vtium.conf".into(),
            line: 5
        })
    );
    assert_eq!(
        large.command(&LOGIN),
        Some(&[OsString::from("/bin/large")][..])
    );
    let small = own("small", &[]);
    assert_eq!(small.number(&FONT_SIZE), Some(12));
    assert_eq!(small.text(&FONT_ENGINE), Some("pango"));
    // the command line wins over the top level, and loses to a section
    let large = own("large", &["--font-size=10"]);
    assert_eq!(large.number(&FONT_SIZE), Some(14));
    let small = own("small", &["--font-size=10"]);
    assert_eq!(small.number(&FONT_SIZE), Some(10));
}

// ADR-1403 D5, ADR-1200 D16: only a look sets `cell`, and within one
// look its `font-size`, other than 0, wins over its `cell`, so the head's
// configuration keeps a look's cell only where that look sets no size
#[test]
#[expect(clippy::expect_used, reason = "a test's set-up fails loudly")]
fn a_look_s_cell_reaches_its_head_unless_the_look_sets_a_size() {
    let text = "font-size=12\nversion = 1\n\
                [look cell]\ncell = 12x24\n\
                [look both]\ncell = 12x24\nfont-size = 32\n\
                [look auto]\ncell = 12x24\nfont-size = 0\n\
                [head a]\nlook = cell\n[head b]\nlook = both\n[head c]\nlook = auto\n";
    let rules = vtium_settings::sections(text.as_bytes())
        .expect("a flow")
        .parsed
        .config;
    let top = load_with(&[], &[("/etc/vtium/vtium.conf", text.as_bytes())]).config;
    assert_eq!(top.size(&CELL), None, "the top level has no cell");
    let own = |head: &str| {
        let h = rules.head(head).expect("the head");
        let settings: Vec<vtium_settings::Setting> = rules
            .settings(h)
            .into_iter()
            .map(|(_, s)| s.clone())
            .collect();
        let (config, reports) = top.for_head(Path::new("/etc/vtium/vtium.conf"), &settings);
        assert_eq!(reports, []);
        config
    };
    assert_eq!(own("a").size(&CELL), Some((12, 24)));
    assert_eq!(own("a").number(&FONT_SIZE), Some(12));
    assert_eq!(own("b").size(&CELL), None, "the look's size wins");
    assert_eq!(own("b").number(&FONT_SIZE), Some(32));
    assert_eq!(own("c").size(&CELL), Some((12, 24)), "0 sets no size");
}

// ADR-1403 D5: `cell` is a look's alone, so the top level and the command
// line refuse it and name where it goes
#[test]
fn the_top_level_and_the_command_line_refuse_a_cell() {
    let l = main_file("cell=12x24\n");
    assert_eq!(problems(&l), [&Problem::SectionOnly("cell")]);
    assert_eq!(l.config.size(&CELL), None);
    for args in [&["--cell=12x24"][..], &["--cell", "12x24"]] {
        let l = load_with(args, &[]);
        assert_eq!(problems(&l), [&Problem::SectionOnly("cell")], "{args:?}");
        assert_eq!(l.config.size(&CELL), None);
    }
    assert_eq!(
        Problem::SectionOnly("cell").to_string(),
        "cell can be set only in a [look NAME] section, ignored"
    );
}

// ADR-1200 D25: font-paths is a list of absolute folders, each that is
// not left out and reported; unset, the helper searches the build's list
#[test]
fn font_paths_keeps_its_absolute_folders_in_order() {
    let l = main_file("font-paths=/srv/fonts:fonts:/usr/share/fonts\n");
    assert_eq!(
        l.config.text(&FONT_PATHS),
        Some("/srv/fonts:/usr/share/fonts")
    );
    assert_eq!(
        problems(&l),
        [&Problem::BadValue {
            key: "font-paths",
            value: "fonts".into(),
            why: NOT_ABSOLUTE,
        }]
    );
    assert_eq!(
        crate::helper::font_dirs(&l.config),
        ["--font-dir", "/srv/fonts", "--font-dir", "/usr/share/fonts"].map(OsString::from)
    );
    let unset = main_file("");
    assert_eq!(unset.config.text(&FONT_PATHS), None);
    assert!(crate::helper::font_dirs(&unset.config).is_empty());
    // no absolute folder at all leaves the key unset
    assert_eq!(main_file("font-paths=a:b\n").config.text(&FONT_PATHS), None);
}

#[test]
fn configdir_names_the_file_read() {
    let mut read = Vec::new();
    let args = [OsString::from("-c"), OsString::from("/x")];
    load(&args, &mut |p: &Path| {
        read.push(p.to_owned());
        Ok(None)
    });
    assert_eq!(read, [PathBuf::from("/x/vtium.conf")]);
    let l = load_with(&["-c/y"], &[]);
    assert_eq!(l.config.text(&CONFIGDIR), Some("/y"));
    // It cannot be set from a file.
    let l = main_file("configdir=/z\n");
    assert_eq!(problems(&l), [&Problem::CommandLineOnly("configdir")]);
    assert_eq!(l.config.text(&CONFIGDIR), Some("/etc/vtium"));
}

#[test]
fn an_unknown_key_is_reported_once() {
    let l = load_with(
        &["--frobnicate", "--frobnicate"],
        &[("/etc/vtium/vtium.conf", b"frob=1\nfrob=2\nwarble\nfrob=3\n")],
    );
    assert_eq!(
        l.reports,
        [
            Report {
                origin: Origin::CommandLine { arg: 1 },
                problem: Problem::UnknownKey("--frobnicate".into()),
            },
            Report {
                origin: line(1),
                problem: Problem::UnknownKey("frob".into()),
            },
            Report {
                origin: line(3),
                problem: Problem::UnknownKey("warble".into()),
            },
        ]
    );
}

// the operator on #145, 2026-10-28: display-dpi states the density, from
// vtium-platform's coarsest believed to its finest; unset, it is measured
#[test]
fn display_dpi_is_unset_or_a_believed_density() {
    use vtium_platform::display::{DPI_MAX, DPI_MIN};
    let bounds = match DISPLAY_DPI.kind {
        Kind::Number { min, max, .. } => Some((min as f64, max as f64)),
        _ => None,
    };
    assert_eq!(bounds, Some((DPI_MIN, DPI_MAX)), "vtium-platform's bounds");
    assert_eq!(main_file("").config.number(&DISPLAY_DPI), None);
    assert_eq!(
        main_file("display-dpi=109\n").config.number(&DISPLAY_DPI),
        Some(109)
    );
    for (text, used) in [("display-dpi=5\n", 20), ("display-dpi=4000\n", 1000)] {
        let l = main_file(text);
        assert_eq!(l.config.number(&DISPLAY_DPI), Some(used), "{text}");
        assert!(
            matches!(
                problems(&l)[..],
                [Problem::Clamped {
                    key: "display-dpi",
                    ..
                }]
            ),
            "{text}"
        );
    }
}

#[test]
fn a_number_out_of_bounds_is_clamped_and_reported() {
    let l = main_file("sb-size=200000\nfont-size=99999\nsb-size-cap=99999999999999999999999\n");
    assert_eq!(l.config.number(&SB_SIZE), Some(100_000));
    assert_eq!(l.config.number(&FONT_SIZE), Some(2048));
    assert_eq!(l.config.number(&SB_SIZE_CAP), Some(100_000));
    assert_eq!(
        problems(&l),
        [
            &Problem::Clamped {
                key: "sb-size",
                value: "200000".into(),
                used: 100_000
            },
            &Problem::Clamped {
                key: "font-size",
                value: "99999".into(),
                used: 2048
            },
            &Problem::Clamped {
                key: "sb-size-cap",
                value: "99999999999999999999999".into(),
                used: 100_000
            },
        ]
    );
    let l = load_with(&["--sync-timeout=10001"], &[]);
    assert_eq!(l.config.number(&SYNC_TIMEOUT), Some(10_000));
    assert_eq!(l.reports.len(), 1);
}

/// `display-wait`: 10 s unset, 0 to not wait, at most 600 s, or
/// `forever`, and no other word.
#[test]
fn the_wait_for_a_display_is_read_and_bounded() {
    assert_eq!(Config::default().number(&DISPLAY_WAIT), Some(10));
    let l = main_file("display-wait=0\n");
    assert_eq!(l.config.number(&DISPLAY_WAIT), Some(0));
    assert_eq!(l.reports, []);
    let l = main_file("display-wait=forever\n");
    assert_eq!(l.config.text(&DISPLAY_WAIT), Some("forever"));
    assert_eq!(l.config.number(&DISPLAY_WAIT), None);
    assert_eq!(l.reports, []);
    let l = main_file("display-wait=never\n");
    assert_eq!(l.config.number(&DISPLAY_WAIT), Some(10), "the default kept");
    assert_eq!(
        problems(&l),
        [&Problem::NotANumber {
            key: "display-wait",
            value: "never".into()
        }]
    );
    assert_eq!(
        problems(&l).first().map(ToString::to_string).as_deref(),
        Some("display-wait=\"never\" is not a whole number or forever, ignored")
    );
    let l = load_with(&["--display-wait=601"], &[]);
    assert_eq!(l.config.number(&DISPLAY_WAIT), Some(600));
    assert_eq!(
        problems(&l),
        [&Problem::Clamped {
            key: "display-wait",
            value: "601".into(),
            used: 600
        }]
    );
}

#[test]
fn a_value_that_cannot_be_read_keeps_the_one_before() {
    let l = load_with(
        &["--font-size=-3", "--render-timer=loud"],
        &[(
            "/etc/vtium/vtium.conf",
            b"font-size=12\nfont-size=12px\nfont-size=\nrender-timer=osd\n",
        )],
    );
    assert_eq!(l.config.number(&FONT_SIZE), Some(12));
    assert_eq!(l.config.text(&RENDER_TIMER), Some("osd"));
    assert_eq!(
        problems(&l),
        [
            &Problem::NotANumber {
                key: "font-size",
                value: "-3".into()
            },
            &Problem::NotAChoice {
                key: "render-timer",
                value: "loud".into()
            },
            &Problem::NotANumber {
                key: "font-size",
                value: "12px".into()
            },
            &Problem::NotANumber {
                key: "font-size",
                value: "".into()
            },
        ]
    );
}

#[test]
fn switches() {
    let l = main_file("no-blink\nhwaccel\nblink=1\nfont-size\nno-font-size=3\n");
    assert!(!l.config.switch(&BLINK));
    assert!(l.config.switch(&HWACCEL));
    assert_eq!(
        problems(&l),
        [
            &Problem::TakesNoValue("blink"),
            &Problem::NeedsValue("font-size"),
            &Problem::UnknownKey("no-font-size".into()),
        ]
    );
    let l = load_with(&["--no-reset-env", "--blink=no", "-h"], &[]);
    assert!(!l.config.switch(&RESET_ENV));
    assert!(l.config.switch(&HELP));
    assert_eq!(problems(&l), [&Problem::TakesNoValue("blink")]);
}

#[test]
fn comments_blanks_and_colors() {
    let l = main_file(
        "  font-name =\tDejaVu Sans Mono  # the console's font\r\n\
         # a comment\n\
         \n\
         term=#a0b1c2 # a color is not a comment\n\
         sb-size=10 # a comment after a blank\n\
         hwaccel # on\n",
    );
    assert_eq!(l.reports, []);
    assert_eq!(l.config.text(&FONT_NAME), Some("DejaVu Sans Mono"));
    assert_eq!(l.config.text(&TERM), Some("#a0b1c2"));
    assert_eq!(l.config.number(&SB_SIZE), Some(10));
    assert!(l.config.switch(&HWACCEL));
    // ADR-1403 D4: a '#' glued to a value stays in it, so the value does not
    // read and the key keeps its default
    let l = main_file("sb-size=10#comment\n");
    assert_eq!(l.config.number(&SB_SIZE), Some(1000));
    assert_eq!(
        problems(&l),
        [&Problem::NotANumber {
            key: "sb-size",
            value: "10#comment".into()
        }]
    );
    // A '#' right after the '=' is in the value too, six hex digits or not.
    assert_eq!(
        main_file("term=#a0b1c\n").config.text(&TERM),
        Some("#a0b1c")
    );
    // '\#' after a blank is a literal '#'.
    assert_eq!(main_file("term=a \\#b\n").config.text(&TERM), Some("a #b"));
}

#[test]
fn login_commands() {
    let l = main_file("login=/bin/sh -c \"echo \\\"hi\\\"\" 'a b' c\\ d \"\"\n");
    assert_eq!(l.reports, []);
    assert_eq!(
        l.config.command(&LOGIN),
        Some(
            &[
                OsString::from("/bin/sh"),
                "-c".into(),
                "echo \"hi\"".into(),
                "a b".into(),
                "c d".into()
            ][..]
        )
    );
    for (text, why) in [
        ("login='/bin/sh\n", "a quote is not closed"),
        ("login=/bin/sh \\\n", "the command ends with a backslash"),
        ("login=\"\"\n", "no command"),
    ] {
        let l = main_file(text);
        assert_eq!(
            problems(&l),
            [&Problem::BadCommand { key: "login", why }],
            "{text}"
        );
        assert_eq!(l.config.command(&LOGIN), None);
    }
    let l = load_with(&["--login", "--"], &[]);
    assert_eq!(problems(&l), [&Problem::LoginWithoutCommand]);
    let l = load_with(&["--login=/bin/sh"], &[]);
    assert_eq!(problems(&l), [&Problem::TakesNoValue("login")]);
    let l = load_with(&["-l", "--", "/bin/sh"], &[]);
    assert_eq!(
        l.config.command(&LOGIN),
        Some(&[OsString::from("/bin/sh")][..])
    );
}

#[test]
fn stray_arguments_never_reach_the_command() {
    let l = load_with(&["/bin/sh", "--login", "x", "--", "/bin/login"], &[]);
    assert_eq!(
        l.config.command(&LOGIN),
        Some(&[OsString::from("/bin/login")][..])
    );
    assert_eq!(
        problems(&l),
        [
            &Problem::StrayArgument("/bin/sh".into()),
            &Problem::StrayArgument("x".into())
        ]
    );
    let l = load_with(&["--", "/bin/sh"], &[]);
    assert_eq!(l.config.command(&LOGIN), None);
    assert_eq!(problems(&l), [&Problem::StrayArgument("/bin/sh".into())]);
}

#[test]
fn option_values() {
    let l = load_with(&["--font-name", "Hack", "-t", "linux", "--sb-size"], &[]);
    assert_eq!(l.config.text(&FONT_NAME), Some("Hack"));
    assert_eq!(l.config.text(&TERM), Some("linux"));
    assert_eq!(problems(&l), [&Problem::NeedsValue("sb-size")]);
    // A name is matched whole, as no prefix of another.
    let l = load_with(&["--font-nam=x", "-x"], &[]);
    assert_eq!(
        problems(&l),
        [
            &Problem::UnknownKey("--font-nam".into()),
            &Problem::UnknownKey("-x".into())
        ]
    );
}

#[test]
fn text_is_bounded() {
    let long = format!("font-name={}\n", "a".repeat(TEXT_MAX + 1));
    let l = main_file(&long);
    assert_eq!(problems(&l), [&Problem::TooLong("font-name")]);
    let l = main_file(&format!("font-name={}\n", "a".repeat(TEXT_MAX)));
    assert_eq!(l.reports, []);
    let l = load_with(&["--term=x\u{1b}[2J"], &[]);
    assert_eq!(problems(&l), [&Problem::ControlCharacter("term")]);
    assert_eq!(l.config.text(&TERM), Some("xterm-vtium"));
}

#[test]
fn files_are_bounded() {
    // Past FILE_MAX nothing is read: the key on the last line is lost.
    let mut big = format!("#{}\n", "-".repeat(62))
        .repeat(FILE_MAX / 64)
        .into_bytes();
    assert_eq!(big.len(), FILE_MAX);
    big.extend_from_slice(b"no-blink\n");
    let l = load_with(&[], &[("/etc/vtium/vtium.conf", &big)]);
    assert!(l.config.switch(&BLINK));
    assert_eq!(problems(&l), [&Problem::FileTooLong]);

    let mut text = format!("#{}\n", "x".repeat(LINE_MAX)).into_bytes();
    text.extend_from_slice(b"sb-size=7\0\nterm=\xff\n=3\nsb-size=8\n");
    let l = load_with(&[], &[("/etc/vtium/vtium.conf", &text)]);
    assert_eq!(l.config.number(&SB_SIZE), Some(8));
    assert_eq!(
        problems(&l),
        [
            &Problem::LineTooLong,
            &Problem::Nul,
            &Problem::NotUtf8,
            &Problem::NoKey
        ]
    );
}

#[test]
fn reports_are_bounded() {
    let text: String = (0..REPORTS_MAX + 10).map(|n| format!("k{n}\n")).collect();
    let l = main_file(&text);
    assert_eq!(l.reports.len(), REPORTS_MAX + 1);
    assert_eq!(
        l.reports.last().map(|r| &r.problem),
        Some(&Problem::More(10))
    );
}

#[test]
fn an_unreadable_file_is_reported() {
    let l = load(&[], &mut |_: &Path| {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    });
    assert_eq!(
        l.reports,
        [Report {
            origin: line(0),
            problem: Problem::Unreadable(io::ErrorKind::PermissionDenied),
        }]
    );
    assert_eq!(l.config, Config::default());
}

#[test]
fn reports_read_as_lines() {
    let l = main_file("font-size=9999\nsb-size=x\u{7}y\n");
    let text: Vec<String> = l.reports.iter().map(ToString::to_string).collect();
    assert_eq!(
        text,
        [
            "/etc/vtium/vtium.conf:1: font-size=\"9999\" is out of range, so 2048 is used",
            "/etc/vtium/vtium.conf:2: sb-size=\"x\\u{7}y\" is not a whole number, ignored",
        ]
    );
}

// covers: color.palette.named
#[test]
fn palette_names_a_palette_vtium_term_holds() {
    let mut choices = match PALETTE.kind {
        Kind::Choice(c) => c.to_vec(),
        _ => Vec::new(),
    };
    choices.sort_unstable();
    let mut names: Vec<&str> = vtium_term::PALETTE_NAMES.to_vec();
    names.push("custom");
    names.sort_unstable();
    assert_eq!(choices, names);
    for name in vtium_term::PALETTE_NAMES {
        let l = main_file(&format!("palette={name}\n"));
        assert_eq!(l.reports, [], "{name}");
        assert_eq!(
            Some(l.config.palette()),
            vtium_term::Palette::named(name),
            "{name}"
        );
    }
    let l = load_with(&["--palette=base16-light"], &[]);
    let p = l.config.palette();
    assert_eq!(p.bg(), vtium_term::Rgb::new(0xd8, 0xd8, 0xd8));
    assert_eq!(p.color(1), vtium_term::Rgb::new(0xab, 0x46, 0x42));
}

#[test]
fn legacy_is_drawn_when_no_source_names_a_palette() {
    let legacy = vtium_term::Palette::named("legacy");
    assert_eq!(Some(Config::default().palette()), legacy);
    assert_eq!(Some(main_file("").config.palette()), legacy);
}

#[test]
fn a_palette_vtium_does_not_hold_is_reported_and_leaves_the_value_below() {
    let l = main_file("palette=frob\n");
    assert_eq!(
        problems(&l),
        [&Problem::NotAChoice {
            key: "palette",
            value: "frob".into()
        }]
    );
    assert_eq!(
        Some(l.config.palette()),
        vtium_term::Palette::named("legacy")
    );
    let l = load_with(
        &["--palette=Nord"],
        &[("/etc/vtium/vtium.conf", b"palette=vga\n")],
    );
    assert_eq!(l.reports.len(), 1);
    assert_eq!(Some(l.config.palette()), vtium_term::Palette::named("vga"));
}

// covers: color.palette.named
#[test]
fn custom_takes_the_palette_keys() {
    // Unset, they hold legacy's colors.
    let l = main_file("palette=custom\n");
    assert_eq!(
        Some(l.config.palette()),
        vtium_term::Palette::named("legacy")
    );
    let mut text = String::from("palette=custom\n");
    let names = [
        "black",
        "red",
        "green",
        "yellow",
        "blue",
        "magenta",
        "cyan",
        "light-grey",
        "dark-grey",
        "light-red",
        "light-green",
        "light-yellow",
        "light-blue",
        "light-magenta",
        "light-cyan",
        "white",
    ];
    for (i, name) in names.iter().enumerate() {
        text.push_str(&format!("palette-{name}={i},{},7\n", 100 + i));
    }
    text.push_str("palette-foreground=1,2,3\npalette-background=4,5,6\n");
    let l = main_file(&text);
    assert_eq!(l.reports, []);
    let p = l.config.palette();
    for i in 0..16u8 {
        assert_eq!(p.color(i), vtium_term::Rgb::new(i, 100 + i, 7), "color {i}");
    }
    assert_eq!(p.fg(), vtium_term::Rgb::new(1, 2, 3));
    assert_eq!(p.bg(), vtium_term::Rgb::new(4, 5, 6));
    assert_eq!(p.color(16), vtium_term::Rgb::new(0, 0, 0));
    assert_eq!(p.color(232), vtium_term::Rgb::new(8, 8, 8));
    // The keys color nothing under another palette.
    let l = main_file("palette=nord\npalette-red=1,2,3\n");
    assert_eq!(Some(l.config.palette()), vtium_term::Palette::named("nord"));
}

#[test]
fn read_bytes_is_bounded() {
    let dir = Scratch::new("settings");
    let path = dir.join("big.conf");
    std::fs::write(&path, vec![b'#'; FILE_MAX * 2]).ok();
    let got = read_bytes(&path).ok().flatten().map(|b| b.len());
    assert_eq!(got, Some(FILE_MAX + 1));
    assert_eq!(read_bytes(&dir.join("absent.conf")).ok(), Some(None));
}

#[test]
fn colors() {
    let l = main_file("palette=custom\npalette-red=10, 20,30\npalette-background=1,2,3\n");
    assert_eq!(l.reports, []);
    assert_eq!(l.config.text(&PALETTE), Some("custom"));
    assert_eq!(l.config.rgb(&PALETTE_RED), Some([10, 20, 30]));
    assert_eq!(l.config.rgb(&PALETTE_BACKGROUND), Some([1, 2, 3]));
    // legacy's colors.
    assert_eq!(l.config.rgb(&PALETTE_BLUE), Some([0, 0, 238]));
    assert_eq!(l.config.rgb(&PALETTE_FOREGROUND), Some([229, 229, 229]));
    let l =
        main_file("palette-red=300,0,0\npalette-red=1,2\npalette-red=a,b,c\npalette-red=1,2,3,4\n");
    assert_eq!(l.config.rgb(&PALETTE_RED), Some([205, 0, 0]));
    let whys: Vec<&str> = l
        .reports
        .iter()
        .filter_map(|r| match &r.problem {
            Problem::BadValue { why, .. } => Some(*why),
            _ => None,
        })
        .collect();
    assert_eq!(
        whys,
        [
            "each value must be 255 or less",
            "a color is three values, r,g,b",
            "each value must be a whole number",
            "a color is three values, r,g,b"
        ]
    );
}

fn mode(gpu: Option<&str>, connector: Option<&str>, w: u32, h: u32, rate: Option<u32>) -> ModeSpec {
    ModeSpec {
        gpu: gpu.map(Into::into),
        connector: connector.map(Into::into),
        width: w,
        height: h,
        rate,
    }
}

#[test]
fn mode_lines_add_up_and_the_command_line_replaces_them() {
    let l = load_with(
        &[],
        &[(
            "/etc/vtium/vtium.conf",
            b"mode=1920x1080\nmode=[HDMI-A-1]3840x2160@60\nmode=pci-0000:00:02.0[eDP-1]1366x768\n",
        )],
    );
    assert_eq!(l.reports, []);
    assert_eq!(
        l.config.modes(&MODE),
        [
            mode(None, None, 1920, 1080, None),
            mode(None, Some("HDMI-A-1"), 3840, 2160, Some(60)),
            mode(Some("pci-0000:00:02.0"), Some("eDP-1"), 1366, 768, None),
        ]
    );
    let l = load_with(
        &["--mode=800x600", "--mode", "[DP-1]1024x768@75"],
        &[("/etc/vtium/vtium.conf", b"mode=1920x1080\n")],
    );
    assert_eq!(
        l.config.modes(&MODE),
        [
            mode(None, None, 800, 600, None),
            mode(None, Some("DP-1"), 1024, 768, Some(75)),
        ]
    );
}

#[test]
fn a_mode_that_does_not_read_is_reported() {
    for (text, why) in [
        ("mode=0x768\n", "a resolution is WxH, each from 1 to 65535"),
        ("mode=1366\n", "a resolution is WxH, each from 1 to 65535"),
        ("mode=[HDMI-A-1 1366x768\n", "a '[' has no ']'"),
        (
            "mode=[]1366x768\n",
            "the connector name in brackets is empty",
        ),
        (
            "mode=1366x768@x\n",
            "a refresh rate is a whole number of Hz",
        ),
        (
            "mode=70000x768\n",
            "a resolution is WxH, each from 1 to 65535",
        ),
    ] {
        let l = main_file(text);
        assert_eq!(l.config.modes(&MODE), [], "{text}");
        assert!(
            matches!(&l.reports[..], [Report { problem: Problem::BadValue { why: w, .. }, .. }] if *w == why),
            "{text}: {:?}",
            l.reports
        );
    }
}

#[test]
fn the_most_specific_mode_wins() {
    let modes = [
        mode(None, None, 800, 600, None),
        mode(None, Some("HDMI-A-1"), 1024, 768, None),
        mode(Some("pci-1"), Some("HDMI-A-1"), 1280, 800, None),
    ];
    let w = |gpu, c| ModeSpec::find(&modes, gpu, c).map(|m| m.width);
    assert_eq!(w(Some("pci-1"), "HDMI-A-1"), Some(1280));
    assert_eq!(w(Some("pci-2"), "HDMI-A-1"), Some(1024));
    assert_eq!(w(None, "HDMI-A-1"), Some(1024));
    assert_eq!(w(Some("pci-1"), "DP-1"), Some(800));
    assert_eq!(ModeSpec::find(&modes[1..], None, "DP-1"), None);
}

#[test]
fn keyboard_and_mouse() {
    let l = main_file(
        "xkb-layout=de\nxkb-variant=nodeadkeys\nxkb-options=ctrl:nocaps\nxkb-repeat-delay=1500\nxkb-repeat-rate=30\nno-mouse\n",
    );
    assert_eq!(l.config.text(&XKB_LAYOUT), Some("de"));
    assert_eq!(l.config.text(&XKB_VARIANT), Some("nodeadkeys"));
    assert_eq!(l.config.text(&XKB_OPTIONS), Some("ctrl:nocaps"));
    assert_eq!(l.config.number(&XKB_REPEAT_DELAY), Some(999));
    assert_eq!(l.config.number(&XKB_REPEAT_RATE), Some(30));
    assert!(!l.config.switch(&MOUSE));
    assert_eq!(
        problems(&l),
        [&Problem::Clamped {
            key: "xkb-repeat-delay",
            value: "1500".into(),
            used: 999
        }]
    );
    let d = Config::default();
    assert!(d.switch(&MOUSE));
    assert_eq!(d.number(&XKB_REPEAT_DELAY), Some(250));
    assert_eq!(d.number(&XKB_REPEAT_RATE), Some(50));
    assert_eq!(d.text(&PALETTE), Some("legacy"));
}

#[test]
fn the_power_keys_are_unbound_until_set() {
    let d = Config::default();
    for key in [&KEY_REBOOT, &KEY_POWER_OFF, &KEY_SUSPEND] {
        assert_eq!(d.chords(key), [], "{}", key.name);
    }
    let l = load_with(
        &["--key-suspend=<Logo>s"],
        &[(
            "/etc/vtium/vtium.conf",
            b"key-reboot=<Ctrl><Alt>Delete\nkey-power-off=<Ctrl><Alt>End,<Logo>p\nkey-suspend=<Logo>z\n",
        )],
    );
    assert_eq!(l.reports, []);
    let chord = |mods, keysym| crate::grab::Chord {
        mods,
        keysyms: vec![keysym],
    };
    assert_eq!(l.config.chords(&KEY_REBOOT), [chord(4 | 8, 0xffff)]);
    assert_eq!(
        l.config.chords(&KEY_POWER_OFF),
        [chord(4 | 8, 0xff57), chord(16, 0x70)]
    );
    // The command line wins over the file.
    assert_eq!(l.config.chords(&KEY_SUSPEND), [chord(16, 0x73)]);
    // An empty value unbinds what a file before it bound.
    let l = load_with(
        &["--key-reboot="],
        &[("/etc/vtium/vtium.conf", b"key-reboot=<Ctrl><Alt>Delete\n")],
    );
    assert_eq!(l.reports, []);
    assert_eq!(l.config.chords(&KEY_REBOOT), []);
}

#[test]
fn tier3_power_configuration_binds_each_power_key() {
    let l = main_file(TIER3_POWER);
    assert_eq!(l.reports, []);
    for key in [&KEY_REBOOT, &KEY_POWER_OFF, &KEY_SUSPEND] {
        assert_eq!(l.config.chords(key).len(), 1, "{}", key.name);
    }
}

#[test]
fn a_chord_that_does_not_read_is_reported_and_leaves_the_value_below() {
    let l = load_with(
        &["--key-reboot=<Ctrl><Alt>delete"],
        &[("/etc/vtium/vtium.conf", b"key-reboot=<Ctrl><Alt>Delete\n")],
    );
    assert_eq!(
        problems(&l),
        [&Problem::BadValue {
            key: "key-reboot",
            value: "<Ctrl><Alt>delete".into(),
            why: "a key name in the wrong case; key names are case-sensitive",
        }]
    );
    assert_eq!(
        l.config.chords(&KEY_REBOOT),
        [crate::grab::Chord {
            mods: 4 | 8,
            keysyms: vec![0xffff]
        }]
    );
}

/// `read_file` on what is not a regular file: a FIFO with no writer, a
/// directory, a character device. Each is reported and none blocks.
#[test]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test's set-up, and a read that blocked"
)]
fn only_a_regular_file_is_read() {
    let dir = Scratch::new("notafile");
    let fifo = dir.join("vtium.conf");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    for path in [fifo.clone(), dir.to_path_buf(), PathBuf::from("/dev/null")] {
        let (tx, rx) = std::sync::mpsc::channel();
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(read_file(&p).map_err(|e| e.kind()));
        });
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap_or_else(|_| panic!("{} blocked", path.display()));
        assert_eq!(got, Err(io::ErrorKind::InvalidInput), "{}", path.display());
    }
    let args: Vec<OsString> = ["-c".into(), dir.as_os_str().to_owned()].into();
    let l = load(&args, &mut read_file);
    assert_eq!(
        l.reports,
        [Report {
            origin: Origin::File {
                path: fifo,
                line: 0
            },
            problem: Problem::NotAFile,
        }]
    );
}

#[test]
fn login_without_its_command_is_reported() {
    for args in [&["--login"][..], &["--login", "--oneshot"], &["-l"]] {
        let l = load_with(args, &[]);
        assert_eq!(problems(&l), [&Problem::LoginWithoutCommand], "{args:?}");
        assert_eq!(l.config.command(&LOGIN), None);
    }
}

#[test]
fn check_config_is_found_wherever_the_options_put_it() {
    for args in [
        &["check-config"][..],
        &["-c", "/x", "check-config"],
        &["check-config", "-c", "/x"],
        &["--oneshot", "check-config", "--no-blink"],
    ] {
        let l = load_with(args, &[]);
        assert!(l.check, "{args:?}");
        assert_eq!(l.reports, [], "{args:?}");
    }
    // An option's value and the command's words are not the subcommand.
    let l = load_with(&["-c", "check-config"], &[]);
    assert!(!l.check);
    assert_eq!(l.config.text(&CONFIGDIR), Some("check-config"));
    let l = load_with(&["--login", "--", "check-config"], &[]);
    assert!(!l.check);
    assert_eq!(
        l.config.command(&LOGIN),
        Some(&[OsString::from("check-config")][..])
    );
}

#[test]
fn a_stray_argument_or_a_login_without_its_command_refuses_the_start() {
    for args in [
        &["x"][..],
        &["--", "/bin/sh"],
        &["--login"],
        &["--login", "--"],
    ] {
        let l = load_with(args, &[]);
        assert!(l.refuses_start(), "{args:?}");
    }
    for args in [
        &["--frob"][..],
        &["--sb-size=x"],
        &["--login", "--", "/bin/sh"],
    ] {
        let l = load_with(args, &[]);
        assert!(!l.refuses_start(), "{args:?}");
    }
    // A file's report never refuses the start.
    assert!(!main_file("x\nlogin=\n").refuses_start());
}

#[test]
fn a_command_word_that_is_not_utf8_is_passed_as_given() {
    use std::os::unix::ffi::OsStringExt;
    let word = OsString::from_vec(b"/bin/\xff".to_vec());
    let args = [
        OsString::from("--login"),
        OsString::from("--"),
        word.clone(),
    ];
    let l = load(&args, &mut |_: &Path| Ok(None));
    assert_eq!(l.reports, []);
    assert_eq!(l.config.command(&LOGIN), Some(&[word][..]));
}

/// The table's own rules, which the parser leans on: `clamp` panics
/// when a bound's `min` passes its `max`.
#[test]
fn the_key_table_keeps_its_rules() {
    use std::collections::BTreeSet;
    let mut names = BTreeSet::new();
    let mut shorts = BTreeSet::new();
    for k in &KEYS {
        assert!(names.insert(k.name), "{} twice", k.name);
        assert!(
            !k.name.starts_with("no-"),
            "{} reads as a switch's no-",
            k.name
        );
        if let Some(c) = k.short {
            assert!(shorts.insert(c), "-{c} twice");
        }
        if let Kind::Number { min, max, word, .. } = k.kind {
            assert!(min <= max, "{}: {min} > {max}", k.name);
            assert!(
                word.is_none_or(|w| w.parse::<u64>().is_err()),
                "{}: a word that reads as a number",
                k.name
            );
        }
        // A default reads back as itself, inside its bound, with no
        // report.
        let text = match k.default {
            keys::Default::Number(n) => Some(n.to_string()),
            keys::Default::Text(t) => Some(t.to_owned()),
            keys::Default::Rgb([r, g, b]) => Some(format!("{r},{g},{b}")),
            keys::Default::Switch(_) => {
                assert_eq!(k.kind, Kind::Switch, "{}", k.name);
                None
            }
            keys::Default::Command(_) => {
                assert_eq!(k.kind, Kind::Command, "{}", k.name);
                None
            }
            keys::Default::Unset => None,
        };
        if let Some(text) = text {
            let mut reports = Reports::default();
            let parsed = value::parse(k, &text, &Origin::Default, &mut reports);
            assert_eq!(reports.finish(), [], "{}={text}", k.name);
            assert_eq!(parsed, value::default(k), "{}", k.name);
        }
    }
}

#[test]
fn blink_text_and_blink_decorations_take_their_values() {
    let d = main_file("");
    assert_eq!(d.config.text(&BLINK_TEXT), Some("partial"));
    assert!(!d.config.switch(&BLINK_DECORATIONS));
    let l = load_with(
        &["--blink-text=off"],
        &[(
            "/etc/vtium/vtium.conf",
            b"blink-text=partial\nblink-decorations\n",
        )],
    );
    assert_eq!(l.config.text(&BLINK_TEXT), Some("off"));
    assert!(l.config.switch(&BLINK_DECORATIONS));
    assert_eq!(problems(&l), [] as [&Problem; 0]);
    let l = main_file("blink-text=half\nblink-decorations=1\n");
    assert_eq!(l.config.text(&BLINK_TEXT), Some("partial"));
    assert_eq!(
        problems(&l),
        [
            &Problem::NotAChoice {
                key: "blink-text",
                value: "half".into()
            },
            &Problem::TakesNoValue("blink-decorations"),
        ]
    );
}

#[test]
fn program_blank_is_on_until_turned_off() {
    assert!(Config::default().switch(&PROGRAM_BLANK));
    let l = main_file("no-program-blank\n");
    assert!(!l.config.switch(&PROGRAM_BLANK));
    assert!(problems(&l).is_empty());
    let l = load_with(&["--no-program-blank"], &[]);
    assert!(!l.config.switch(&PROGRAM_BLANK));
}

#[test]
fn key_paste_binds_shift_insert_until_set() {
    let chord = |mods, keysym| crate::grab::Chord {
        mods,
        keysyms: vec![keysym],
    };
    assert_eq!(Config::default().chords(&KEY_PASTE), [chord(1, 0xff63)]);
    let l = main_file("key-paste=<Ctrl><Shift>V,<Shift>Insert\n");
    assert_eq!(l.reports, []);
    assert_eq!(
        l.config.chords(&KEY_PASTE),
        [chord(4 | 1, 0x56), chord(1, 0xff63)]
    );
    let l = main_file("key-paste=\n");
    assert_eq!(l.reports, []);
    assert_eq!(l.config.chords(&KEY_PASTE), []);
}

/// The combinations of a key binding, as vtium.conf writes them.
fn written(l: &Loaded, key: &Key) -> Vec<String> {
    l.config
        .chords(key)
        .iter()
        .map(crate::grab::written)
        .collect()
}

// ADR-1005 D10: a key vtium.conf binds to another combination takes that
// combination alone, and an empty value takes none; with no vtium.conf
// each key keeps its built-in default.
#[test]
fn the_key_bindings_take_what_vtium_conf_binds_and_nothing_when_empty() {
    let l = main_file("key-scroll-up = <Alt>k\nkey-reset =\n");
    assert_eq!(l.reports, []);
    assert_eq!(written(&l, &KEY_SCROLL_UP), ["<Alt>k"]);
    assert_eq!(written(&l, &KEY_RESET), Vec::<String>::new());
    assert_eq!(written(&l, &KEY_SCROLL_DOWN), ["<Shift>Down"]);
    let none = load_with(&[], &[]);
    assert_eq!(none.reports, []);
    assert_eq!(written(&none, &KEY_RESET), ["<Ctrl><Shift>Delete"]);
    assert_eq!(written(&none, &KEY_ZOOM_IN), ["<Ctrl>plus", "<Ctrl>equal"]);
}

// ADR-1005 D10: a combination two keys bind goes to the key that comes
// first in the key order, and the report names both keys, at the line of
// the key the file set: the later key's line when the file set both.
#[test]
fn a_combination_bound_twice_is_reported_and_goes_to_the_first_key() {
    let l = main_file("key-zoom-in = <Shift>Up,<Alt>z\n");
    assert_eq!(
        l.reports,
        [Report {
            origin: line(1),
            problem: Problem::BoundTwice {
                chord: "<Shift>Up".into(),
                first: "key-scroll-up",
                then: "key-zoom-in",
            },
        }]
    );
    assert_eq!(written(&l, &KEY_SCROLL_UP), ["<Shift>Up"]);
    assert_eq!(written(&l, &KEY_ZOOM_IN), ["<Alt>z"]);
    // The earlier key set in the file takes a later key's default.
    let l = main_file("\nkey-console-next = <Ctrl>minus\n");
    assert_eq!(
        l.reports,
        [Report {
            origin: line(2),
            problem: Problem::BoundTwice {
                chord: "<Ctrl>minus".into(),
                first: "key-console-next",
                then: "key-zoom-out",
            },
        }]
    );
    assert_eq!(written(&l, &KEY_CONSOLE_NEXT), ["<Ctrl>minus"]);
    assert_eq!(written(&l, &KEY_ZOOM_OUT), Vec::<String>::new());
    // Both set: the report is at the later key's line.
    let l = main_file("key-reset = <Alt>r\nkey-paste = <Alt>r\n");
    assert_eq!(
        l.reports
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "/etc/vtium/vtium.conf:1: <Alt>r is bound by both key-paste and key-reset. key-paste keeps it, because vtium checks key-paste first. To use it for key-reset, remove it from key-paste, or give key-reset another combination"
        ]
    );
    assert_eq!(written(&l, &KEY_PASTE), ["<Alt>r"]);
    assert_eq!(written(&l, &KEY_RESET), Vec::<String>::new());
}

// ADR-1005 D10: Ctrl+Alt+F1 to F12 and the XF86Switch_VT keys switch the
// VT, the rescue key among them, and no key binds them: a binding of one
// is reported with its file and line and dropped, and the key keeps the
// rest of its value.
#[test]
fn a_binding_of_a_key_that_switches_the_vt_is_reported_and_dropped() {
    let l =
        main_file("key-zoom-in = <Ctrl>plus\nkey-reset = <Ctrl><Alt>F2,<Alt>r,XF86Switch_VT_3\n");
    assert_eq!(
        l.reports
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "/etc/vtium/vtium.conf:2: key-reset: <Ctrl><Alt>F2 switches the VT, so vtium ignores this combination. No setting can bind Ctrl+Alt+F1 to F12 or the XF86Switch_VT keys",
            "/etc/vtium/vtium.conf:2: key-reset: XF86Switch_VT_3 switches the VT, so vtium ignores this combination. No setting can bind Ctrl+Alt+F1 to F12 or the XF86Switch_VT keys",
        ]
    );
    assert_eq!(written(&l, &KEY_RESET), ["<Alt>r"]);
}

// ADR-1005 D10: a mapping that does not read is reported with its file and
// line, and the key keeps its built-in default.
#[test]
fn a_bad_mapping_is_reported_with_its_file_and_line() {
    let l = main_file("\n\nkey-page-up = <Hyper>Prior\nkey-focus-next = <Ctrl><Logo>NoSuchKey\n");
    assert_eq!(
        l.reports
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "/etc/vtium/vtium.conf:3: key-page-up=\"<Hyper>Prior\": a modifier other than Shift, Lock, Control, Ctrl, Alt or Logo, ignored",
            "/etc/vtium/vtium.conf:4: key-focus-next=\"<Ctrl><Logo>NoSuchKey\": a key name xkb does not know, ignored",
        ]
    );
    assert_eq!(written(&l, &KEY_PAGE_UP), ["<Shift>Prior"]);
    assert_eq!(written(&l, &KEY_FOCUS_NEXT), ["<Ctrl><Logo>Up"]);
}

// ADR-1005 D10: vtium reads the same keys from a vtium.conf a person
// edited by hand, with spaces and a comment, and from the same file as
// `vtium-config set` writes it (vtium-config's test
// `set_writes_vtiums_own_keys_as_a_person_writes_them` holds that text).
#[test]
fn vtium_config_set_and_a_hand_edit_give_the_same_keys() {
    let by_set = "font-size=12\nversion = 1\nkey-scroll-up=<Alt>k\nkey-reset=\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";
    let by_hand = "font-size=12\n# my keys\nkey-scroll-up = <Alt>k\nkey-reset =\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";
    let set = main_file(by_set);
    let hand = main_file(by_hand);
    assert_eq!(
        (set.reports.as_slice(), hand.reports.as_slice()),
        (&[][..], &[][..])
    );
    for (key, _) in crate::grab::ORDER {
        assert_eq!(
            set.config.chords(key),
            hand.config.chords(key),
            "{}",
            key.name
        );
    }
    assert_eq!(written(&hand, &KEY_SCROLL_UP), ["<Alt>k"]);
    assert_eq!(written(&hand, &KEY_RESET), Vec::<String>::new());
}

// The maintainer renamed the key bindings to more accepted terms
// (ADR-1005 D10, 2026-11-07): vtium reads the four names that shipped
// before, grab-paste, grab-reboot, grab-power-off and grab-suspend, as
// their new keys, and reports each line that uses one, with the name to
// write in its place.
#[test]
fn an_old_key_binding_name_is_read_as_its_new_name_and_reported() {
    let l = main_file("\ngrab-paste = <Alt>p\n");
    assert_eq!(
        l.reports,
        [Report {
            origin: line(2),
            problem: Problem::Renamed {
                old: "grab-paste",
                new: "key-paste",
            },
        }]
    );
    assert_eq!(
        l.reports
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "/etc/vtium/vtium.conf:2: grab-paste is now called key-paste. vtium reads grab-paste as key-paste for now; rename it"
        ]
    );
    assert_eq!(written(&l, &KEY_PASTE), ["<Alt>p"]);
    for (old, new) in [
        ("grab-reboot", &KEY_REBOOT),
        ("grab-power-off", &KEY_POWER_OFF),
        ("grab-suspend", &KEY_SUSPEND),
    ] {
        let l = main_file(&format!("{old}=<Logo>q\n"));
        assert_eq!(written(&l, new), ["<Logo>q"], "{old}");
        assert_eq!(l.reports.len(), 1, "{old}");
    }
    // The command line reads the old names too.
    let l = load_with(&["--grab-reboot=<Ctrl><Alt>Delete"], &[]);
    assert_eq!(written(&l, &KEY_REBOOT), ["<Ctrl><Alt>Delete"]);
    assert_eq!(
        problems(&l),
        [&Problem::Renamed {
            old: "grab-reboot",
            new: "key-reboot",
        }]
    );
    // The seventeen keys that never shipped have no old names.
    let l = main_file("grab-reset = <Alt>r\n");
    assert_eq!(problems(&l), [&Problem::UnknownKey("grab-reset".into())]);
}

// When a file sets a key binding by its old name and by its new one, in
// either order, the new name wins and the old one is reported.
#[test]
fn the_new_name_wins_over_the_old_one() {
    for text in [
        "key-paste = <Alt>n\ngrab-paste = <Alt>o\n",
        "grab-paste = <Alt>o\nkey-paste = <Alt>n\n",
    ] {
        let l = main_file(text);
        assert_eq!(written(&l, &KEY_PASTE), ["<Alt>n"], "{text:?}");
        assert_eq!(
            problems(&l),
            [&Problem::Renamed {
                old: "grab-paste",
                new: "key-paste",
            }],
            "{text:?}"
        );
    }
    let l = load_with(&["--key-paste=<Alt>n", "--grab-paste=<Alt>o"], &[]);
    assert_eq!(written(&l, &KEY_PASTE), ["<Alt>n"]);
}

#[test]
fn decoration_colors_read_fg_an_index_or_rgb() {
    use vtium_render::DecoColor;
    use vtium_term::Rgb;
    let d = Config::default();
    assert_eq!(d.deco_color(&UNDERLINE_COLOR), DecoColor::Foreground);
    assert_eq!(d.deco_color(&STRIKE_COLOR), DecoColor::Foreground);
    assert!(!d.switch(&FORCE_DECORATION_COLORS));
    let l =
        main_file("underline-color=#Ff8000 # orange\nstrike-color=007\nforce-decoration-colors\n");
    assert_eq!(l.reports, []);
    assert_eq!(
        l.config.deco_color(&UNDERLINE_COLOR),
        DecoColor::Rgb(Rgb::new(255, 128, 0))
    );
    // Decimal digits only: a leading 0 is not octal.
    assert_eq!(l.config.deco_color(&STRIKE_COLOR), DecoColor::Indexed(7));
    assert!(l.config.switch(&FORCE_DECORATION_COLORS));
    for bad in [
        "256", "1234", "", "#12345", "#1234567", "#12345g", "0x10", "-1", "FG", "1,2,3",
    ] {
        let l = load_with(
            &[&format!("--underline-color={bad}"), "--strike-color=fg"],
            &[],
        );
        assert_eq!(
            problems(&l),
            [&Problem::BadValue {
                key: "underline-color",
                value: bad.into(),
                why: "use fg, a palette number from 0 to 255, or #rrggbb",
            }],
            "{bad:?}"
        );
        assert_eq!(l.config.deco_color(&UNDERLINE_COLOR), DecoColor::Foreground);
    }
}

#[test]
fn pointer_char_takes_one_character() {
    assert_eq!(Config::default().char(&POINTER_CHAR), None);
    let l = main_file("pointer-char=✛\n");
    assert_eq!(l.reports, []);
    assert_eq!(l.config.char(&POINTER_CHAR), Some('✛'));
    for bad in ["", "ab", "\u{7f}", " ", "\u{3000}", "\u{a0}"] {
        let l = load_with(&[&format!("--pointer-char={bad}")], &[]);
        assert_eq!(
            problems(&l),
            [&Problem::BadValue {
                key: "pointer-char",
                value: bad.into(),
                why: "give one character that is not a control character or a space",
            }],
            "{bad:?}"
        );
        assert_eq!(l.config.char(&POINTER_CHAR), None);
    }
}

/// `sb-memory`: the scrollback's byte budget, fenced from 1 MiB to 1 GiB
/// (vtium/vtium#131); a quarter of the machine's memory caps it at start.
#[test]
fn sb_memory_is_fenced_from_1_mib_to_1_gib() {
    super::budget::set_test_memory(Some(8 << 30));
    assert_eq!(
        Config::default().number(&SB_MEMORY),
        Some(vtium_term::SB_MEMORY_DEFAULT as u64)
    );
    for (asked, used) in [
        ("1048576", 1 << 20),
        ("6291456", 6 << 20),
        ("1073741824", 1 << 30),
    ] {
        let l = main_file(&format!("sb-memory={asked}\n"));
        assert_eq!(l.reports, [], "{asked}");
        assert_eq!(l.config.number(&SB_MEMORY), Some(used));
    }
    for (asked, used) in [
        ("0", 1 << 20),
        ("1048575", 1 << 20),
        ("1073741825", 1 << 30),
        ("99999999999999999999999", 1 << 30),
    ] {
        let l = load_with(&[&format!("--sb-memory={asked}")], &[]);
        assert_eq!(l.config.number(&SB_MEMORY), Some(used), "{asked}");
        assert_eq!(
            problems(&l),
            [&Problem::Clamped {
                key: "sb-memory",
                value: asked.into(),
                used
            }]
        );
    }
    for (asked, used) in [
        ("512K", 1 << 20),
        ("1024k", 1 << 20),
        ("6M", 6 << 20),
        ("6m", 6 << 20),
        ("1G", 1 << 30),
    ] {
        let l = main_file(&format!("sb-memory={asked}\n"));
        assert_eq!(l.config.number(&SB_MEMORY), Some(used), "{asked}");
        assert_eq!(l.reports.len(), usize::from(asked == "512K"), "{asked}");
    }
    let l = main_file("sb-memory=2G\n");
    assert_eq!(l.config.number(&SB_MEMORY), Some(1 << 30));
    assert_eq!(
        problems(&l),
        [&Problem::Clamped {
            key: "sb-memory",
            value: "2G".into(),
            used: 1 << 30
        }]
    );
    for bad in ["M", "6T", "6MB", "-6M", "6 M", "0x6M"] {
        let l = main_file(&format!("sb-memory={bad}\n"));
        assert_eq!(
            problems(&l),
            [&Problem::NotANumber {
                key: "sb-memory",
                value: bad.into()
            }],
            "{bad}"
        );
    }
    // A key counted in anything else takes no suffix.
    assert_eq!(
        problems(&main_file("sb-size=2K\n")),
        [&Problem::NotANumber {
            key: "sb-size",
            value: "2K".into()
        }]
    );
    assert_eq!(
        problems(&main_file("sb-memory=0\n"))
            .first()
            .map(ToString::to_string)
            .as_deref(),
        Some("sb-memory=\"0\" is out of range, so 1048576 is used")
    );
}

// On the 512 MB minspec machine a quarter of the memory caps `sb-memory`:
// a value past it, or past 1 GiB, is reported once, naming the value
// asked and the value used.
#[test]
fn sb_memory_past_the_machines_quarter_is_reported_once() {
    super::budget::set_test_memory(Some(512 << 20));
    for asked in ["200M", "2G"] {
        let l = main_file(&format!("sb-memory={asked}\n"));
        assert_eq!(l.config.number(&SB_MEMORY), Some(128 << 20), "{asked}");
        assert_eq!(
            problems(&l),
            [&Problem::Clamped {
                key: "sb-memory",
                value: asked.into(),
                used: 128 << 20
            }]
        );
        assert_eq!(crate::check::warnings(&l).len(), 1, "{asked}");
    }
    let l = main_file("sb-memory=128M\n");
    assert_eq!(l.reports, []);
    // Unread, the memory holds `sb-memory` to its default.
    super::budget::set_test_memory(None);
    let l = main_file("sb-memory=6M\n");
    assert_eq!(
        l.config.number(&SB_MEMORY),
        Some(vtium_term::SB_MEMORY_DEFAULT as u64)
    );
    assert_eq!(crate::check::warnings(&l).len(), 2);
}

/// `display-buffers` takes `1`, `2` or `3` and nothing else: a value
/// outside them is reported and ignored, keeping the value the sources
/// before it gave, never clamped into range. Unset, it is the count the
/// build names (ADR-1500 D8), and for a build that names none the accessor
/// gives none and the console's rule by its features decides (ADR-1401
/// D2).
#[test]
fn display_buffers_takes_one_two_or_three() {
    // unset, the count the build names, if it names one (ADR-1500 D8)
    assert_eq!(Config::default().display_buffers(), DISPLAY_BUFFERS_BUILD);
    for (text, n) in [("1", 1), ("2", 2), ("3", 3)] {
        let l = main_file(&format!("display-buffers={text}\n"));
        assert_eq!(l.config.display_buffers(), Some(n), "{text}");
        assert_eq!(l.reports, [], "{text}");
    }
    for bad in ["0", "4", "02", "two", "", "-1"] {
        let l = main_file(&format!("display-buffers=2\ndisplay-buffers={bad}\n"));
        assert_eq!(l.config.display_buffers(), Some(2), "{bad:?}");
        assert_eq!(
            problems(&l),
            [&Problem::NotAChoice {
                key: "display-buffers",
                value: bad.into()
            }],
            "{bad:?}"
        );
    }
    assert_eq!(
        main_file("display-buffers=4\n")
            .reports
            .first()
            .map(ToString::to_string)
            .as_deref(),
        Some(
            "/etc/vtium/vtium.conf:1: display-buffers=\"4\" is not a value this setting takes. It takes 1, 2, or 3. Because of this error, vtium ignores this setting."
        )
    );
    // The command line wins over the file, in either form of the option.
    for args in [&["--display-buffers=1"][..], &["--display-buffers", "1"]] {
        let l = load_with(args, &[("/etc/vtium/vtium.conf", b"display-buffers=3\n")]);
        assert_eq!(l.config.display_buffers(), Some(1), "{args:?}");
        assert_eq!(l.reports, [], "{args:?}");
    }
    let l = load_with(
        &["--display-buffers=5"],
        &[("/etc/vtium/vtium.conf", b"display-buffers=3\n")],
    );
    assert_eq!(l.config.display_buffers(), Some(3), "the file's value kept");
    assert_eq!(l.reports.len(), 1);
}

/// `font-hinting` takes `light` or `full` as written, any other value
/// reported and ignored. Unset, the accessor gives none and the frit's
/// `hinting=` decides, `light` where it says nothing (ADR-1200 D1).
#[test]
fn font_hinting_takes_light_or_full() {
    assert_eq!(Config::default().font_hinting(), None);
    for (text, h) in [("light", Hinting::Light), ("full", Hinting::Full)] {
        let l = main_file(&format!("font-hinting={text}\n"));
        assert_eq!(l.config.font_hinting(), Some(h), "{text}");
        assert_eq!(l.reports, [], "{text}");
    }
    for bad in ["Light", "FULL", "none", "slight", "", "1"] {
        let l = main_file(&format!("font-hinting=full\nfont-hinting={bad}\n"));
        assert_eq!(l.config.font_hinting(), Some(Hinting::Full), "{bad:?}");
        assert_eq!(
            problems(&l),
            [&Problem::NotAChoice {
                key: "font-hinting",
                value: bad.into()
            }],
            "{bad:?}"
        );
    }
    for args in [&["--font-hinting=light"][..], &["--font-hinting", "light"]] {
        let l = load_with(args, &[("/etc/vtium/vtium.conf", b"font-hinting=full\n")]);
        assert_eq!(l.config.font_hinting(), Some(Hinting::Light), "{args:?}");
        assert_eq!(l.reports, [], "{args:?}");
    }
}

/// `font-line-spacing` is a whole percentage of the text's height from 0
/// to 100: past 100 it is clamped and reported, as every number key is,
/// and a value that is no whole number is reported and ignored. Unset,
/// the accessor gives none and the frit's `line-spacing=` decides, 0
/// where it states none (ADR-1302 D13).
#[test]
fn font_line_spacing_is_a_percentage_from_0_to_100() {
    assert_eq!(Config::default().font_line_spacing(), None);
    for n in [0, 1, 25, 100] {
        let l = main_file(&format!("font-line-spacing={n}\n"));
        assert_eq!(l.config.font_line_spacing(), Some(n), "{n}");
        assert_eq!(l.reports, [], "{n}");
    }
    let l = main_file("font-line-spacing=101\n");
    assert_eq!(l.config.font_line_spacing(), Some(100));
    assert_eq!(
        problems(&l),
        [&Problem::Clamped {
            key: "font-line-spacing",
            value: "101".into(),
            used: 100
        }]
    );
    let l = main_file("font-line-spacing=99999999999999999999999\n");
    assert_eq!(l.config.font_line_spacing(), Some(100));
    for bad in ["-1", "12%", "1.5", "", "none"] {
        let l = main_file(&format!("font-line-spacing=20\nfont-line-spacing={bad}\n"));
        assert_eq!(l.config.font_line_spacing(), Some(20), "{bad:?}");
        assert_eq!(
            problems(&l),
            [&Problem::NotANumber {
                key: "font-line-spacing",
                value: bad.into()
            }],
            "{bad:?}"
        );
    }
    for args in [
        &["--font-line-spacing=0"][..],
        &["--font-line-spacing", "0"],
    ] {
        let l = load_with(
            args,
            &[("/etc/vtium/vtium.conf", b"font-line-spacing=40\n")],
        );
        assert_eq!(l.config.font_line_spacing(), Some(0), "{args:?}");
        assert_eq!(l.reports, [], "{args:?}");
    }
}

#[test]
fn the_image_keys_take_auto_a_size_and_bytes() {
    let d = main_file("");
    assert_eq!(d.config.number(&IMAGE_STORAGE_LIMIT), None);
    assert_eq!(d.config.text(&IMAGE_STORAGE_LIMIT), Some("auto"));
    assert_eq!(d.config.size(&MAX_GRAPHIC_SIZE), Some((10_000, 10_000)));
    assert_eq!(d.config.number(&NUM_COLOR_REGISTERS), Some(1024));
    assert!(d.config.switch(&PRIVATE_COLOR_REGISTERS));
    assert!(d.config.switch(&SIXEL));
    assert!(d.config.switch(&KITTY_GRAPHICS));
    let l = main_file(
        "image-storage-limit=64M\nmax-graphic-size=1920x65535\nnum-color-registers=1\nno-sixel\n",
    );
    assert_eq!(l.config.number(&IMAGE_STORAGE_LIMIT), Some(64 << 20));
    assert_eq!(l.config.size(&MAX_GRAPHIC_SIZE), Some((1920, 65_535)));
    // below its bound, clamped and reported
    assert_eq!(l.config.number(&NUM_COLOR_REGISTERS), Some(2));
    assert!(!l.config.switch(&SIXEL));
    assert!(matches!(
        &problems(&l)[..],
        [Problem::Clamped { used: 2, .. }]
    ));
    // 1 TiB at most, and `0` is a limit of its own
    let l = main_file("image-storage-limit=2048G\n");
    assert_eq!(l.config.number(&IMAGE_STORAGE_LIMIT), Some(1 << 40));
    let l = main_file("image-storage-limit=0\n");
    assert_eq!(l.config.number(&IMAGE_STORAGE_LIMIT), Some(0));
    // a size is two sides from 1 to 65535; a bad one keeps the default
    for bad in ["0x10", "10x", "65536x1", "10", "10x10x10", "+5x5", "auto"] {
        let l = main_file(&format!("max-graphic-size={bad}\n"));
        assert_eq!(
            l.config.size(&MAX_GRAPHIC_SIZE),
            Some((10_000, 10_000)),
            "{bad}"
        );
        assert!(
            matches!(&problems(&l)[..], [Problem::BadValue { .. }]),
            "{bad}: {:?}",
            l.reports
        );
    }
}

// ADR-1004 D5: `text-direction` reads `ltr`, `rtl` or `auto`, `ltr` when
// unset; a value it does not take is reported and leaves the default.
#[test]
fn text_direction_reads_its_three_values_and_reports_any_other() {
    assert_eq!(main_file("").config.text(&TEXT_DIRECTION), Some("ltr"));
    for value in ["ltr", "rtl", "auto"] {
        let l = main_file(&format!("text-direction = {value}\n"));
        assert_eq!(l.reports, [], "{value}");
        assert_eq!(l.config.text(&TEXT_DIRECTION), Some(value));
    }
    let l = main_file("text-direction = up\n");
    assert_eq!(l.config.text(&TEXT_DIRECTION), Some("ltr"));
    assert!(
        matches!(
            &problems(&l)[..],
            [Problem::NotAChoice {
                key: "text-direction",
                ..
            }]
        ),
        "{:?}",
        l.reports
    );
}

// ADR-1004 D5: each console's terminal starts in the direction
// `text-direction` gives, in a build with the bidi feature.
#[cfg(feature = "bidi")]
#[test]
#[expect(clippy::disallowed_methods, reason = "a test's clock is the host's")]
fn each_console_starts_in_the_direction_text_direction_gives() {
    use vtium_term::LineFlags;
    for (value, want) in [
        ("ltr", LineFlags::empty()),
        ("rtl", LineFlags::RTL),
        ("auto", LineFlags::DETECT),
    ] {
        let l = main_file(&format!("text-direction = {value}\n"));
        let clock = std::rc::Rc::new(std::time::Instant::now);
        let term = crate::head::terminal(&l.config, (0, 0), 0, clock);
        assert_eq!(term.terminal.screen().paragraph(), want, "{value}");
    }
}

/// vtium goes silent only on an `osd` line that says so in a form vtium
/// can read. While the top level also holds an `osd` line whose value
/// vtium cannot read, vtium runs as `off` in place of `silent`, so the
/// notices still show, and a report on the `silent` line says why. Any
/// other reading applies as read: the last value vtium can read wins, and
/// a misspelled key leaves `osd` at the value the other lines give.
///
/// References:
/// - ADR-1401 D7, "`osd = silent` in a file with errors": each case here.
#[test]
fn an_osd_line_vtium_cannot_read_keeps_osd_silent_from_applying() {
    let osd = |args: &[&str], text: &str| {
        let l = load_with(args, &[("/etc/vtium/vtium.conf", text.as_bytes())]);
        let said: Vec<String> = l.reports.iter().map(ToString::to_string).collect();
        (l.config.text(&OSD).map(str::to_owned), said)
    };
    let not_silent = "/etc/vtium/vtium.conf:1: osd=silent does not apply, because another osd line has a value vtium cannot read. vtium runs as osd=off instead, so its notices still show. Correct the other osd line to hide them.";
    for text in [
        "osd = silent\nosd = slient\n",
        "osd = slient\nosd = silent\n",
        "osd = silent\nosd =\n",
    ] {
        let (value, said) = osd(&[], text);
        assert_eq!(value.as_deref(), Some("off"), "{text:?}");
        let line = if text.starts_with("osd = silent") {
            not_silent.to_owned()
        } else {
            not_silent.replace(".conf:1:", ".conf:2:")
        };
        assert_eq!(said.last(), Some(&line), "{text:?}: {said:#?}");
        assert_eq!(said.len(), 2, "{text:?}: the bad line and why: {said:#?}");
    }
    // the command line's mistake counts too, beside the file's silent
    let (value, said) = osd(&["--osd=slient"], "osd = silent\n");
    assert_eq!(value.as_deref(), Some("off"), "{said:#?}");
    for (text, want) in [
        ("osd = silent\n", "silent"),
        ("osd = full\nosd = silent\n", "silent"),
        ("osd = silent\nosd = full\n", "full"),
        ("osd = slient\nosd = full\n", "full"),
        ("osb = silent\n", "minimal"),
        ("osb = silent\nosd = silent\n", "silent"),
    ] {
        let (value, said) = osd(&[], text);
        assert_eq!(value.as_deref(), Some(want), "{text:?}: {said:#?}");
        assert!(
            said.iter().all(|s| !s.contains("does not apply")),
            "{text:?}: {said:#?}"
        );
    }
}
