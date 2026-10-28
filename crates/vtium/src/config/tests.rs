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

// The five guest configurations of tier 3, read from tools/tier3/guest/,
// which installs each as /etc/kmscon/kmscon.conf.
const TIER3_2D: &str = include_str!("../../../../tools/tier3/guest/kmscon.conf");
const TIER3_FONT32: &str = include_str!("../../../../tools/tier3/guest/kmscon-font32.conf");
const TIER3_GL: &str = include_str!("../../../../tools/tier3/guest/kmscon-gl.conf");
const TIER3_MIN_PSF: &str = include_str!("../../../../tools/tier3/guest/kmscon-minimal-psf.conf");
const TIER3_MIN: &str = include_str!("../../../../tools/tier3/guest/kmscon-minimal.conf");
const TIER3_POWER: &str = include_str!("../../../../tools/tier3/guest/kmscon-power.conf");

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
    // `.seat.conf` is 10 bytes: a 245-byte name makes a 255-byte file
    // name, the most one holds.
    let long = "s".repeat(246);
    for seat in ["../etc/shadow", "/etc/x", "a/b", ".", "", &long] {
        let mut read = Vec::new();
        let l = load(&[], Some(seat), &mut |p: &Path| {
            read.push(p.to_owned());
            Ok(None)
        });
        assert_eq!(read, [PathBuf::from("/etc/kmscon/kmscon.conf")], "{seat}");
        assert_eq!(problems(&l), [&Problem::BadSeatName(seat.to_owned())]);
    }
    for seat in ["seat0", "seat_1-a", &long[1..]] {
        let mut read = Vec::new();
        let l = load(&[], Some(seat), &mut |p: &Path| {
            read.push(p.to_owned());
            Ok(None)
        });
        assert_eq!(l.reports, [], "{seat}");
        assert_eq!(
            read.get(1),
            Some(&PathBuf::from(format!("/etc/kmscon/{seat}.seat.conf")))
        );
    }
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
    let l = load_with(&["--display-wait=601"], None, &[]);
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
    assert_eq!(
        l.config.command(&LOGIN),
        Some(&[OsString::from("/bin/sh")][..])
    );
}

#[test]
fn stray_arguments_never_reach_the_command() {
    let l = load_with(&["/bin/sh", "--login", "x", "--", "/bin/login"], None, &[]);
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

// covers: colour.palette.named
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
    let l = load_with(&["--palette=base16-light"], None, &[]);
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
        None,
        &[("/etc/kmscon/kmscon.conf", b"palette=vga\n")],
    );
    assert_eq!(l.reports.len(), 1);
    assert_eq!(Some(l.config.palette()), vtium_term::Palette::named("vga"));
}

// covers: colour.palette.named
#[test]
fn custom_takes_the_palette_keys() {
    // Unset, they hold the C reference's defaults, which are legacy's.
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
        assert_eq!(
            p.color(i),
            vtium_term::Rgb::new(i, 100 + i, 7),
            "colour {i}"
        );
    }
    assert_eq!(p.fg(), vtium_term::Rgb::new(1, 2, 3));
    assert_eq!(p.bg(), vtium_term::Rgb::new(4, 5, 6));
    assert_eq!(p.color(16), vtium_term::Rgb::new(0, 0, 0));
    assert_eq!(p.color(232), vtium_term::Rgb::new(8, 8, 8));
    // The keys colour nothing under another palette.
    let l = main_file("palette=nord\npalette-red=1,2,3\n");
    assert_eq!(Some(l.config.palette()), vtium_term::Palette::named("nord"));
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

#[test]
fn colours() {
    let l = main_file("palette=custom\npalette-red=10, 20,30\npalette-background=1,2,3\n");
    assert_eq!(l.reports, []);
    assert_eq!(l.config.text(&PALETTE), Some("custom"));
    assert_eq!(l.config.rgb(&PALETTE_RED), Some([10, 20, 30]));
    assert_eq!(l.config.rgb(&PALETTE_BACKGROUND), Some([1, 2, 3]));
    // The C reference's default colours.
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
            "each value is at most 255",
            "three values are needed, r,g,b",
            "each value is a whole number",
            "three values are needed, r,g,b"
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
        Some("seat0"),
        &[
            (
                "/etc/kmscon/kmscon.conf",
                b"mode=1920x1080\nmode=[HDMI-A-1]3840x2160@60\n",
            ),
            (
                "/etc/kmscon/seat0.seat.conf",
                b"mode=pci-0000:00:02.0[eDP-1]1366x768\n",
            ),
        ],
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
        None,
        &[("/etc/kmscon/kmscon.conf", b"mode=1920x1080\n")],
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
        ("mode=0x768\n", "a size is WxH, each from 1 to 65535"),
        ("mode=1366\n", "a size is WxH, each from 1 to 65535"),
        ("mode=[HDMI-A-1 1366x768\n", "a '[' with no ']'"),
        ("mode=[]1366x768\n", "an empty name in brackets"),
        ("mode=1366x768@x\n", "a rate is a whole number of Hz"),
        ("mode=70000x768\n", "a size is WxH, each from 1 to 65535"),
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
    for key in [&GRAB_REBOOT, &GRAB_POWER_OFF, &GRAB_SUSPEND] {
        assert_eq!(d.chords(key), [], "{}", key.name);
    }
    let l = load_with(
        &["--grab-suspend=<Logo>s"],
        None,
        &[(
            "/etc/kmscon/kmscon.conf",
            b"grab-reboot=<Ctrl><Alt>Delete\ngrab-power-off=<Ctrl><Alt>End,<Logo>p\ngrab-suspend=<Logo>z\n",
        )],
    );
    assert_eq!(l.reports, []);
    let chord = |mods, keysym| crate::grab::Chord {
        mods,
        keysyms: vec![keysym],
    };
    assert_eq!(l.config.chords(&GRAB_REBOOT), [chord(4 | 8, 0xffff)]);
    assert_eq!(
        l.config.chords(&GRAB_POWER_OFF),
        [chord(4 | 8, 0xff57), chord(16, 0x70)]
    );
    // The command line wins over the file.
    assert_eq!(l.config.chords(&GRAB_SUSPEND), [chord(16, 0x73)]);
    // An empty value unbinds what a file before it bound.
    let l = load_with(
        &["--grab-reboot="],
        None,
        &[(
            "/etc/kmscon/kmscon.conf",
            b"grab-reboot=<Ctrl><Alt>Delete\n",
        )],
    );
    assert_eq!(l.reports, []);
    assert_eq!(l.config.chords(&GRAB_REBOOT), []);
}

#[test]
fn tier3_power_configuration_binds_each_power_key() {
    let l = main_file(TIER3_POWER);
    assert_eq!(l.reports, []);
    for key in [&GRAB_REBOOT, &GRAB_POWER_OFF, &GRAB_SUSPEND] {
        assert_eq!(l.config.chords(key).len(), 1, "{}", key.name);
    }
}

#[test]
fn a_chord_that_does_not_read_is_reported_and_leaves_the_value_below() {
    let l = load_with(
        &["--grab-reboot=<Ctrl><Alt>delete"],
        None,
        &[(
            "/etc/kmscon/kmscon.conf",
            b"grab-reboot=<Ctrl><Alt>Delete\n",
        )],
    );
    assert_eq!(
        problems(&l),
        [&Problem::BadValue {
            key: "grab-reboot",
            value: "<Ctrl><Alt>delete".into(),
            why: "a key name in the wrong case; key names are case-sensitive",
        }]
    );
    assert_eq!(
        l.config.chords(&GRAB_REBOOT),
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
    let dir = std::env::temp_dir().join(format!("vtium-notafile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a directory");
    let fifo = dir.join("kmscon.conf");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    for path in [fifo.clone(), dir.clone(), PathBuf::from("/dev/null")] {
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
    let args: Vec<OsString> = ["-c".into(), dir.clone().into_os_string()].into();
    let l = load(&args, None, &mut read_file);
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
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn login_without_its_command_is_reported() {
    for args in [&["--login"][..], &["--login", "--oneshot"], &["-l"]] {
        let l = load_with(args, None, &[]);
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
        let l = load_with(args, None, &[]);
        assert!(l.check, "{args:?}");
        assert_eq!(l.reports, [], "{args:?}");
    }
    // An option's value and the command's words are not the subcommand.
    let l = load_with(&["-c", "check-config"], None, &[]);
    assert!(!l.check);
    assert_eq!(l.config.text(&CONFIGDIR), Some("check-config"));
    let l = load_with(&["--login", "--", "check-config"], None, &[]);
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
        let l = load_with(args, None, &[]);
        assert!(l.refuses_start(), "{args:?}");
    }
    for args in [
        &["--frob"][..],
        &["--sb-size=x"],
        &["--login", "--", "/bin/sh"],
    ] {
        let l = load_with(args, None, &[]);
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
    let l = load(&args, None, &mut |_: &Path| Ok(None));
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
    assert_eq!(d.config.text(&BLINK_TEXT), Some("full"));
    assert!(!d.config.switch(&BLINK_DECORATIONS));
    let l = load_with(
        &["--blink-text=off"],
        None,
        &[(
            "/etc/kmscon/kmscon.conf",
            b"blink-text=partial\nblink-decorations\n",
        )],
    );
    assert_eq!(l.config.text(&BLINK_TEXT), Some("off"));
    assert!(l.config.switch(&BLINK_DECORATIONS));
    assert_eq!(problems(&l), [] as [&Problem; 0]);
    let l = main_file("blink-text=half\nblink-decorations=1\n");
    assert_eq!(l.config.text(&BLINK_TEXT), Some("full"));
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
    let l = load_with(&["--no-program-blank"], None, &[]);
    assert!(!l.config.switch(&PROGRAM_BLANK));
}
