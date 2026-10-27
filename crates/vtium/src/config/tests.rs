use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use super::keys::*;
use super::*;

/// [`load`] over files held in memory, by path.
fn load_with(args: &[&str], seat: Option<&str>, files: &[(&str, &[u8])]) -> Loaded {
    let files: BTreeMap<PathBuf, Vec<u8>> = files
        .iter()
        .map(|(p, b)| (PathBuf::from(p), b.to_vec()))
        .collect();
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    load(&args, seat, &mut |p: &Path| Ok(files.get(p).cloned()))
}

/// The main file alone.
fn main_file(text: &str) -> Loaded {
    load_with(&[], None, &[("/etc/kmscon/kmscon.conf", text.as_bytes())])
}

fn problems(l: &Loaded) -> Vec<&Problem> {
    l.reports.iter().map(|r| &r.problem).collect()
}

fn line(n: usize) -> Origin {
    Origin::File {
        path: "/etc/kmscon/kmscon.conf".into(),
        line: n,
    }
}

// The five guest configurations of tier 3 (kmscon `fork/tier3/guest/`,
// on `fork/tier3-vtium` at bde2fd92): their keys as tier 3 installs them,
// and the first line of each header comment.
const TIER3_2D: &str = "\
# kmscon config for tier 3 runs on the 2D path (drm2d with bbulk).
font-engine=pango
font-name=DejaVu Sans Mono
font-size=16
no-blink
render-timer=log
";
const TIER3_FONT32: &str = "\
# kmscon config for the tier 3 variant with large cells: the 2D path (drm2d
font-engine=pango
font-name=DejaVu Sans Mono
font-size=32
no-blink
render-timer=log
";
const TIER3_GL: &str = "\
# kmscon config for tier 3 runs on the GL path (drm3d with gltex).
font-engine=pango
font-name=DejaVu Sans Mono
font-size=16
no-blink
hwaccel
render-timer=log
";
const TIER3_MIN_PSF: &str = "\
# kmscon config for tier 3's 2d-potato-min-psf variant: the minimal build
font-engine=psf
font-name=/usr/share/kbd/consolefonts/default8x16.psfu.gz
no-blink
render-timer=log
";
const TIER3_MIN: &str = "\
# kmscon config for tier 3's 2d-potato-min variant: the minimal build
no-blink
render-timer=log
";

// covers: platform.config
#[test]
fn tier3_guest_configurations() {
    // Each file, its font-engine, font-name and font-size, and hwaccel.
    type Case<'a> = (&'a str, Option<&'a str>, Option<&'a str>, u64, bool);
    let cases: [Case; 5] = [
        (TIER3_2D, Some("pango"), Some("DejaVu Sans Mono"), 16, false),
        (
            TIER3_FONT32,
            Some("pango"),
            Some("DejaVu Sans Mono"),
            32,
            false,
        ),
        (TIER3_GL, Some("pango"), Some("DejaVu Sans Mono"), 16, true),
        (
            TIER3_MIN_PSF,
            Some("psf"),
            Some("/usr/share/kbd/consolefonts/default8x16.psfu.gz"),
            0,
            false,
        ),
        (TIER3_MIN, None, Some("monospace"), 0, false),
    ];
    for (text, engine, name, size, hwaccel) in cases {
        let l = main_file(text);
        assert_eq!(l.reports, [], "{text}");
        let c = &l.config;
        assert_eq!(c.text(&FONT_ENGINE), engine, "{text}");
        assert_eq!(c.text(&FONT_NAME), name, "{text}");
        assert_eq!(c.number(&FONT_SIZE), Some(size), "{text}");
        assert!(!c.switch(&BLINK), "{text}");
        assert_eq!(c.switch(&HWACCEL), hwaccel, "{text}");
        assert_eq!(c.text(&RENDER_TIMER), Some("log"), "{text}");
    }
}

// covers: platform.config
#[test]
fn tier3_command_lines() {
    // kmsconvt@.service's ExecStart, which tier 3's guest starts on tty2.
    let l = load_with(&["--vt=tty2", "--no-switchvt"], None, &[]);
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
        None,
        &[("/review/config/kmscon.conf", b"font-size=24\n")],
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
                "/usr/bin/env".to_owned(),
                "DEMO_LABEL=vtium dev".into(),
                "/bin/bash".into()
            ][..]
        )
    );
}

#[test]
fn defaults() {
    let c = Config::default();
    assert_eq!(c.text(&CONFIGDIR), Some("/etc/kmscon"));
    assert_eq!(c.text(&TERM), Some("kmscon"));
    assert_eq!(c.number(&SB_SIZE), Some(1000));
    assert_eq!(c.number(&SB_SIZE_CAP), None);
    assert_eq!(c.number(&SYNC_TIMEOUT), Some(200));
    assert_eq!(c.number(&DPMS_TIMEOUT), Some(600));
    assert_eq!(c.number(&FONT_SIZE), Some(0));
    assert_eq!(c.text(&FONT_NAME), Some("monospace"));
    assert_eq!(c.text(&RENDER_TIMER), Some("none"));
    assert_eq!(c.text(&GLYPH_CACHE), None);
    assert_eq!(c.command(&LOGIN), None);
    for (key, on) in [
        (&SWITCHVT, true),
        (&RESET_ENV, true),
        (&BACKSPACE_DELETE, true),
        (&BLINK, true),
        (&ONESHOT, false),
        (&HWACCEL, false),
        (&USE_ORIGINAL_MODE, false),
        (&HELP, false),
    ] {
        assert_eq!(c.switch(key), on, "{}", key.name);
    }
    assert_eq!(c.origin(&SB_SIZE), Some(&Origin::Default));
}

#[test]
fn command_line_wins_over_the_seat_file_over_the_main_file() {
    let l = load_with(
        &["--sb-size=300"],
        Some("seat0"),
        &[
            (
                "/etc/kmscon/kmscon.conf",
                b"sb-size=100\nfont-size=12\nterm=xterm\n",
            ),
            (
                "/etc/kmscon/seat0.seat.conf",
                b"sb-size=200\nfont-size=14\n",
            ),
        ],
    );
    assert_eq!(l.reports, []);
    let c = &l.config;
    assert_eq!(c.number(&SB_SIZE), Some(300));
    assert_eq!(c.origin(&SB_SIZE), Some(&Origin::CommandLine { arg: 1 }));
    assert_eq!(c.number(&FONT_SIZE), Some(14));
    assert_eq!(c.text(&TERM), Some("xterm"));
    // Within one source the last line wins.
    assert_eq!(
        main_file("sb-size=5\nsb-size=6\n").config.number(&SB_SIZE),
        Some(6)
    );
}

#[test]
fn configdir_names_the_files_read() {
    let mut read = Vec::new();
    let args = [OsString::from("-c"), OsString::from("/x")];
    load(&args, Some("seat1"), &mut |p: &Path| {
        read.push(p.to_owned());
        Ok(None)
    });
    assert_eq!(
        read,
        [
            PathBuf::from("/x/kmscon.conf"),
            PathBuf::from("/x/seat1.seat.conf")
        ]
    );
    let l = load_with(&["-c/y"], None, &[]);
    assert_eq!(l.config.text(&CONFIGDIR), Some("/y"));
    // It cannot be set from a file.
    let l = main_file("configdir=/z\n");
    assert_eq!(problems(&l), [&Problem::CommandLineOnly("configdir")]);
    assert_eq!(l.config.text(&CONFIGDIR), Some("/etc/kmscon"));
}

#[test]
fn a_seat_name_that_names_no_file_is_not_read() {
    let mut read = Vec::new();
    let l = load(&[], Some("../etc/shadow"), &mut |p: &Path| {
        read.push(p.to_owned());
        Ok(None)
    });
    assert_eq!(read, [PathBuf::from("/etc/kmscon/kmscon.conf")]);
    assert_eq!(
        problems(&l),
        [&Problem::BadSeatName("../etc/shadow".to_owned())]
    );
}

#[test]
fn an_unknown_key_is_reported_once() {
    let l = load_with(
        &["--frobnicate", "--frobnicate"],
        Some("seat0"),
        &[
            ("/etc/kmscon/kmscon.conf", b"frob=1\nfrob=2\nwarble\n"),
            ("/etc/kmscon/seat0.seat.conf", b"frob=3\n"),
        ],
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
    let l = load_with(&["--sync-timeout=10001"], None, &[]);
    assert_eq!(l.config.number(&SYNC_TIMEOUT), Some(10_000));
    assert_eq!(l.reports.len(), 1);
}

#[test]
fn a_value_that_cannot_be_read_keeps_the_one_before() {
    let l = load_with(
        &["--font-size=-3", "--render-timer=loud"],
        None,
        &[(
            "/etc/kmscon/kmscon.conf",
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
    let l = load_with(&["--no-reset-env", "--blink=no", "-h"], None, &[]);
    assert!(!l.config.switch(&RESET_ENV));
    assert!(l.config.switch(&HELP));
    assert_eq!(problems(&l), [&Problem::TakesNoValue("blink")]);
}

#[test]
fn comments_blanks_and_colours() {
    let l = main_file(
        "  font-name =\tDejaVu Sans Mono  # the console's font\r\n\
         # a comment\n\
         \n\
         term=#a0b1c2 # a colour is not a comment\n\
         sb-size=10#comment\n\
         hwaccel # on\n",
    );
    assert_eq!(l.reports, []);
    assert_eq!(l.config.text(&FONT_NAME), Some("DejaVu Sans Mono"));
    assert_eq!(l.config.text(&TERM), Some("#a0b1c2"));
    assert_eq!(l.config.number(&SB_SIZE), Some(10));
    assert!(l.config.switch(&HWACCEL));
    // Not six hex digits, so the '#' starts a comment and the value is empty.
    assert_eq!(main_file("term=#a0b1c\n").config.text(&TERM), Some(""));
}

#[test]
fn login_commands() {
    let l = main_file("login=/bin/sh -c \"echo \\\"hi\\\"\" 'a b' c\\ d \"\"\n");
    assert_eq!(l.reports, []);
    assert_eq!(
        l.config.command(&LOGIN),
        Some(
            &[
                "/bin/sh".to_owned(),
                "-c".into(),
                "echo \"hi\"".into(),
                "a b".into(),
                "c d".into()
            ][..]
        )
    );
    for (text, why) in [
        ("login='/bin/sh\n", "a quote is not closed"),
        ("login=/bin/sh \\\n", "a backslash ends the command"),
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
    let l = load_with(&["--login", "--"], None, &[]);
    assert_eq!(problems(&l), [&Problem::LoginWithoutCommand]);
    let l = load_with(&["--login=/bin/sh"], None, &[]);
    assert_eq!(problems(&l), [&Problem::TakesNoValue("login")]);
    let l = load_with(&["-l", "--", "/bin/sh"], None, &[]);
    assert_eq!(l.config.command(&LOGIN), Some(&["/bin/sh".to_owned()][..]));
}

#[test]
fn stray_arguments_never_reach_the_command() {
    let l = load_with(&["/bin/sh", "--login", "x", "--", "/bin/login"], None, &[]);
    assert_eq!(
        l.config.command(&LOGIN),
        Some(&["/bin/login".to_owned()][..])
    );
    assert_eq!(
        problems(&l),
        [
            &Problem::StrayArgument("/bin/sh".into()),
            &Problem::StrayArgument("x".into())
        ]
    );
    let l = load_with(&["--", "/bin/sh"], None, &[]);
    assert_eq!(l.config.command(&LOGIN), None);
    assert_eq!(problems(&l), [&Problem::StrayArgument("/bin/sh".into())]);
}

#[test]
fn option_values() {
    let l = load_with(
        &["--font-name", "Hack", "-t", "linux", "--sb-size"],
        None,
        &[],
    );
    assert_eq!(l.config.text(&FONT_NAME), Some("Hack"));
    assert_eq!(l.config.text(&TERM), Some("linux"));
    assert_eq!(problems(&l), [&Problem::NeedsValue("sb-size")]);
    // A name is matched whole, as no prefix of another.
    let l = load_with(&["--font-nam=x", "-x"], None, &[]);
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
    let l = load_with(&["--term=x\u{1b}[2J"], None, &[]);
    assert_eq!(problems(&l), [&Problem::ControlCharacter("term")]);
    assert_eq!(l.config.text(&TERM), Some("kmscon"));
}

#[test]
fn files_are_bounded() {
    // Past FILE_MAX nothing is read: the key on the last line is lost.
    let mut big = format!("#{}\n", "-".repeat(62))
        .repeat(FILE_MAX / 64)
        .into_bytes();
    assert_eq!(big.len(), FILE_MAX);
    big.extend_from_slice(b"hwaccel\n");
    let l = load_with(&[], None, &[("/etc/kmscon/kmscon.conf", &big)]);
    assert!(!l.config.switch(&HWACCEL));
    assert_eq!(problems(&l), [&Problem::FileTooLong]);

    let mut text = format!("#{}\n", "x".repeat(LINE_MAX)).into_bytes();
    text.extend_from_slice(b"sb-size=7\0\nterm=\xff\n=3\nsb-size=8\n");
    let l = load_with(&[], None, &[("/etc/kmscon/kmscon.conf", &text)]);
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
    let l = load(&[], None, &mut |_: &Path| {
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
            "/etc/kmscon/kmscon.conf:1: font-size=\"9999\" is out of bounds, 2048 used",
            "/etc/kmscon/kmscon.conf:2: sb-size=\"x\\u{7}y\" is not a whole number, ignored",
        ]
    );
}

#[test]
fn read_file_is_bounded() {
    let dir = std::env::temp_dir().join(format!("vtium-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("big.conf");
    std::fs::write(&path, vec![b'#'; FILE_MAX * 2]).ok();
    let got = read_file(&path).ok().flatten().map(|b| b.len());
    assert_eq!(got, Some(FILE_MAX + 1));
    assert_eq!(read_file(&dir.join("absent.conf")).ok(), Some(None));
    std::fs::remove_dir_all(&dir).ok();
}
