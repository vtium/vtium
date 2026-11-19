//! The palette reader's rules, one test for each: the format, the keys,
//! the colors, the ignored keys, the later line winning, every error, the
//! bound, and the two folders.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]

use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::scratch::Scratch;

fn legacy() -> Palette {
    Palette::default()
}

/// The palette `text` reads as, or the test fails with its errors.
fn colors(text: &str) -> Palette {
    match parse(text.as_bytes()) {
        Ok((palette, _)) => palette,
        Err(e) => panic!("{text:?}: {e:?}"),
    }
}

/// The errors `text` reads with: each line and reason, and the count past
/// the bound.
fn faults(text: &[u8]) -> (Vec<(Option<usize>, Reason)>, usize) {
    match parse(text) {
        Ok(_) => panic!("{text:?} reads cleanly"),
        Err((faults, more)) => (
            faults.into_iter().map(|f| (f.line, f.reason)).collect(),
            more,
        ),
    }
}

// An empty file, a comment and blank lines set nothing, so every color
// keeps legacy's, the 16 and the cube and gray ramp above them.
// covers: color.palette.named
#[test]
fn a_file_that_sets_nothing_is_legacy() {
    assert_eq!(colors(""), legacy());
    assert_eq!(colors("# Dracula\n\n   \n  # indented comment\n"), legacy());
}

// `palette = N=COLOR` sets color N, and the colors the file leaves unset,
// 16 to 255 among them, keep legacy's.
// covers: color.palette.named
#[test]
fn palette_sets_the_color_it_numbers() {
    let p = colors("palette = 0=#1d1f21\npalette = 15=#ffffff\npalette = 200=#010203\n");
    assert_eq!(p.color(0), Rgb::new(0x1d, 0x1f, 0x21));
    assert_eq!(p.color(15), Rgb::new(0xff, 0xff, 0xff));
    assert_eq!(p.color(200), Rgb::new(1, 2, 3));
    for i in (1..=14).chain(16..=199).chain(201..=255) {
        assert_eq!(p.color(i), legacy().color(i), "color {i}");
    }
    assert_eq!((p.fg(), p.bg()), (legacy().fg(), legacy().bg()));
    // blanks around the inner `=` are allowed, as around the outer one
    let p = colors("palette=3 = #0a0b0c\n");
    assert_eq!(p.color(3), Rgb::new(10, 11, 12));
}

// `foreground` and `background` set the default colors.
// covers: color.palette.named
#[test]
fn foreground_and_background_set_the_defaults() {
    let p = colors("foreground = #f8f8f2\nbackground = 282a36\n");
    assert_eq!(p.fg(), Rgb::new(0xf8, 0xf8, 0xf2));
    assert_eq!(p.bg(), Rgb::new(0x28, 0x2a, 0x36));
    assert_eq!(p.color(1), legacy().color(1));
}

// The cursor's and the selection's colors are set by their keys, and
// `cell-foreground` and `cell-background` leave each unset, so vtium
// draws by inverting the cell.
// covers: color.palette.named
#[test]
fn cursor_and_selection_colors_and_the_cell_words() {
    let p = colors(
        "cursor-color = #ff0000\nselection-background = #00ff00\nselection-foreground = #0000ff\n",
    );
    assert_eq!(p.special(Special::Cursor), Some(Rgb::new(255, 0, 0)));
    assert_eq!(p.special(Special::SelectionBg), Some(Rgb::new(0, 255, 0)));
    assert_eq!(p.special(Special::SelectionFg), Some(Rgb::new(0, 0, 255)));
    for word in ["cell-foreground", "cell-background"] {
        let p = colors(&format!(
            "cursor-color = #ff0000\ncursor-color = {word}\nselection-background = {word}\nselection-foreground = {word}\n"
        ));
        for which in [Special::Cursor, Special::SelectionBg, Special::SelectionFg] {
            assert_eq!(p.special(which), None, "{word} {which:?}");
        }
    }
    // the pointer's colors are no key of a palette file
    assert_eq!(colors("").special(Special::PointerFg), None);
}

// A color is `#rrggbb`, `rrggbb`, or an X11 color name in any case and
// with any spaces, and a value may be quoted, as Ghostty quotes one.
#[test]
fn a_color_is_hex_with_or_without_the_hash_or_an_x11_name() {
    let want = Rgb::new(0x2f, 0x4f, 0x4f);
    for value in [
        "#2f4f4f",
        "#2F4F4F",
        "2f4f4f",
        "DarkSlateGray",
        "dark slate gray",
        "darkslategrey",
        "\"#2f4f4f\"",
        "\"dark slate gray\"",
    ] {
        assert_eq!(
            colors(&format!("foreground = {value}\n")).fg(),
            want,
            "{value}"
        );
    }
}

// Keys vtium does not read are ignored, each listed once at its first
// line, and a `config-file` or `theme` line is never followed.
#[test]
fn other_keys_are_ignored_and_listed_once() {
    let text = "cursor-text = #000000\nfont-family = \"Fira Code\"\ncursor-text = #ffffff\nconfig-file = /etc/passwd\ntheme = dracula\nforeground = #010203\n";
    let Ok((palette, ignored)) = parse(text.as_bytes()) else {
        panic!("reads cleanly");
    };
    let listed: Vec<(&str, usize)> = ignored.iter().map(|i| (i.key.as_str(), i.line)).collect();
    assert_eq!(
        listed,
        [
            ("cursor-text", 1),
            ("font-family", 2),
            ("config-file", 4),
            ("theme", 5)
        ]
    );
    assert_eq!(palette.fg(), Rgb::new(1, 2, 3));
    assert_eq!(palette.color(0), legacy().color(0));
}

// When a key appears twice, the later line wins, for each color number
// apart.
// covers: color.palette.named
#[test]
fn the_later_line_wins() {
    let p = colors(
        "foreground = #111111\npalette = 1=#aa0000\npalette = 2=#00aa00\nforeground = #222222\npalette = 1=#bb0000\n",
    );
    assert_eq!(p.fg(), Rgb::new(0x22, 0x22, 0x22));
    assert_eq!(p.color(1), Rgb::new(0xbb, 0, 0));
    assert_eq!(p.color(2), Rgb::new(0, 0xaa, 0));
}

// Carriage returns and a byte-order mark read as nothing.
#[test]
fn crlf_and_a_byte_order_mark_read() {
    let p = colors("\u{feff}foreground = #010203\r\n# comment\r\nbackground = #040506\r\n");
    assert_eq!((p.fg(), p.bg()), (Rgb::new(1, 2, 3), Rgb::new(4, 5, 6)));
}

// A line that is not `key = value` is an error, and any error makes the
// whole file count as missing, whatever lines read cleanly.
#[test]
fn a_line_that_is_not_key_value_is_an_error() {
    for line in [
        "foreground #ffffff",
        "= #ffffff",
        "fore ground = #ffffff",
        "[colors]",
    ] {
        let text = format!("background = #000000\n{line}\n");
        assert_eq!(
            faults(text.as_bytes()),
            (vec![(Some(2), Reason::NotKeyValue)], 0),
            "{line}"
        );
    }
}

// A color vtium cannot read is an error, in every key that takes one.
#[test]
fn a_color_vtium_cannot_read_is_an_error() {
    for (key, value) in [
        ("foreground", "#fff"),
        ("background", "#1234567"),
        ("cursor-color", "nosuchcolor"),
        ("selection-background", ""),
        ("selection-foreground", "#gggggg"),
        ("foreground", "#DarkSlateGray"),
    ] {
        let text = format!("{key} = {value}\n");
        let (got, _) = faults(text.as_bytes());
        assert_eq!(
            got,
            [(
                Some(1),
                Reason::BadColor {
                    key: match key {
                        "foreground" => "foreground",
                        "background" => "background",
                        "cursor-color" => "cursor-color",
                        "selection-background" => "selection-background",
                        _ => "selection-foreground",
                    },
                    value: Quoted::new(value),
                }
            )],
            "{text:?}"
        );
    }
    let (got, _) = faults(b"palette = 4=reddish\n");
    assert_eq!(
        got,
        [(
            Some(1),
            Reason::BadColor {
                key: "palette",
                value: Quoted::new("reddish")
            }
        )]
    );
}

// A `palette` value that is not `N=COLOR` is an error, and so is an index
// past 255.
#[test]
fn a_bad_palette_line_and_an_index_past_255_are_errors() {
    for value in [
        "#ffffff",
        "x=#ffffff",
        "-1=#ffffff",
        "=#ffffff",
        "1.5=#ffffff",
    ] {
        let text = format!("palette = {value}\n");
        assert_eq!(
            faults(text.as_bytes()).0,
            [(Some(1), Reason::BadEntry(Quoted::new(value)))],
            "{value}"
        );
    }
    for index in ["256", "1000", "99999999999999999999"] {
        let text = format!("palette = {index}=#ffffff\n");
        assert_eq!(
            faults(text.as_bytes()).0,
            [(Some(1), Reason::PastLastColor(Quoted::new(index)))],
            "{index}"
        );
    }
    assert_eq!(
        colors("palette = 255=#010101\n").color(255),
        Rgb::new(1, 1, 1)
    );
}

// A file of PALETTE_FILE_MAX bytes reads, and one byte more is an error.
#[test]
fn a_file_over_16_kib_is_an_error() {
    assert_eq!(PALETTE_FILE_MAX, 16 * 1024);
    let line = "foreground = #010203\n";
    let mut text = String::from(line);
    while text.len() < PALETTE_FILE_MAX {
        text.push('#');
        let room = PALETTE_FILE_MAX.saturating_sub(text.len()).min(70);
        text.push_str(&"x".repeat(room.saturating_sub(1)));
        text.push('\n');
    }
    text.truncate(PALETTE_FILE_MAX);
    assert_eq!(text.len(), PALETTE_FILE_MAX);
    assert_eq!(colors(&text).fg(), Rgb::new(1, 2, 3));
    text.push('\n');
    assert_eq!(faults(text.as_bytes()), (vec![(None, Reason::TooLarge)], 0));
}

// Bytes that are not UTF-8 are an error at the line that holds them.
#[test]
fn bytes_that_are_not_utf8_are_an_error() {
    assert_eq!(
        faults(b"foreground = #010203\nbackground = \xff\n"),
        (vec![(Some(2), Reason::NotText)], 0)
    );
}

// The errors are kept in the file's order up to ERRORS_MAX, and the rest
// counted.
#[test]
fn errors_past_the_bound_are_counted() {
    let text = "nonsense\n".repeat(ERRORS_MAX.saturating_add(3));
    let (got, more) = faults(text.as_bytes());
    assert_eq!(got.len(), ERRORS_MAX);
    assert_eq!(more, 3);
    assert_eq!(got.first().map(|f| f.0), Some(Some(1)));
    assert_eq!(got.last().map(|f| f.0), Some(Some(ERRORS_MAX)));
}

/// Two palette folders under a scratch directory, the administrator's
/// and the package's, and the trust their owner gets.
struct Folders {
    _dir: Scratch,
    admin: PathBuf,
    package: PathBuf,
    trust: Trust,
}

impl Folders {
    fn new(name: &str) -> Folders {
        let dir = Scratch::new(name);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (admin, package) = (dir.join("etc"), dir.join("share"));
        for d in [&admin, &package] {
            std::fs::create_dir(d).unwrap();
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&*dir).unwrap());
        let trust = Trust::owner(uid).below(&dir);
        Folders {
            _dir: dir,
            admin,
            package,
            trust,
        }
    }

    fn write(&self, folder: &Path, name: &str, text: &[u8]) -> PathBuf {
        let path = folder.join(format!("{name}{SUFFIX}"));
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        path
    }

    fn read(&self, name: &str) -> Result<Option<Scheme>, Failure> {
        read(name, &[&self.admin, &self.package], &self.trust)
    }
}

// A name is looked up in the administrator's folder first, then the
// package's; a name no folder holds, and a name no file can take, are
// missing.
// covers: color.palette.named
#[test]
fn the_administrator_s_file_wins_on_a_name() {
    let f = Folders::new("palette-folders");
    f.write(&f.package, "dracula", b"foreground = #000001\n");
    let only = f.read("dracula").unwrap().unwrap();
    assert_eq!(only.palette.fg(), Rgb::new(0, 0, 1));
    assert_eq!(only.file, f.package.join("dracula.palette"));
    let mine = f.write(&f.admin, "dracula", b"foreground = #000002\n");
    let read = f.read("dracula").unwrap().unwrap();
    assert_eq!((read.palette.fg(), read.file), (Rgb::new(0, 0, 2), mine));
    assert_eq!(f.read("absent"), Ok(None));
    assert_eq!(f.read("../share/dracula"), Ok(None));
    assert_eq!(f.read(""), Ok(None));
}

// The administrator's file wins on a name even when it has errors, so it
// counts as missing and never lets the package's file through.
#[test]
fn a_broken_administrator_s_file_hides_the_package_s() {
    let f = Folders::new("palette-broken");
    f.write(&f.package, "kiln", b"foreground = #000001\n");
    let mine = f.write(&f.admin, "kiln", b"foreground = nope\n");
    let failure = f.read("kiln").unwrap_err();
    assert_eq!(failure.file, mine);
    assert_eq!(failure.faults.len(), 1);
}

// A file that takes the name of a built-in palette or `custom` is never
// read, and is reported.
#[test]
fn a_file_named_like_a_built_in_palette_is_never_read() {
    let f = Folders::new("palette-shadow");
    let legacy_file = f.write(&f.package, "legacy", b"foreground = #000001\n");
    let custom_file = f.write(&f.admin, "custom", b"foreground = #000001\n");
    assert_eq!(
        f.read("legacy").unwrap_err().faults[0].reason,
        Reason::Shadows("legacy")
    );
    let found: Vec<PathBuf> = shadowing(&[&f.admin, &f.package])
        .into_iter()
        .map(|failure| failure.file)
        .collect();
    assert_eq!(found, [custom_file, legacy_file]);
}

// The ownership walk refuses a file others can write, and a file larger
// than the bound is refused before it is read whole.
#[test]
fn the_walk_refuses_a_writable_file_and_a_large_one() {
    let f = Folders::new("palette-walk");
    let open = f.write(&f.admin, "open", b"foreground = #000001\n");
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(matches!(
        f.read("open").unwrap_err().faults[0].reason,
        Reason::Untrusted(Untrusted::Writable { .. })
    ));
    f.write(
        &f.admin,
        "big",
        &vec![b'#'; PALETTE_FILE_MAX.saturating_add(1)],
    );
    assert_eq!(
        f.read("big").unwrap_err().faults[0].reason,
        Reason::TooLarge
    );
}

// What a person reads: the reasons in plain words, and the log line with
// the scheme, the file and line, the count of the rest, and what vtium
// draws instead.
#[test]
fn the_log_line_names_the_scheme_the_line_and_the_rest() {
    let failure = Failure {
        name: "kiln".to_owned(),
        file: PathBuf::from("/etc/vtium/palettes/kiln.palette"),
        faults: vec![
            Fault {
                line: Some(3),
                reason: Reason::BadColor {
                    key: "foreground",
                    value: Quoted::new("\u{1b}[31m"),
                },
            },
            Fault {
                line: Some(4),
                reason: Reason::NotKeyValue,
            },
        ],
        more: 1,
    };
    assert_eq!(
        failure.log_line("Head desk draws the color scheme legacy instead."),
        "Color scheme kiln (/etc/vtium/palettes/kiln.palette:3): The color \"\\u{1b}[31m\" of foreground is not one vtium can read. A color is #rrggbb, rrggbb or an X11 color name such as DarkSlateGray. 2 more errors. Head desk draws the color scheme legacy instead."
    );
    assert_eq!(
        Reason::PastLastColor(Quoted::new("256")).to_string(),
        "The color number \"256\" is past 255, the last of the 256 colors."
    );
}

// A key longer than a quote shows is still listed once, however many
// lines set it, and two keys that share their first characters are two
// keys. The fuzz target found the first case listed twice.
#[test]
fn a_long_ignored_key_is_listed_once() {
    let long = "k".repeat(crate::bounds::QUOTE_MAX.saturating_add(10));
    let other = format!("{long}x");
    let text = format!("{long} = 1\n{long} = 2\n{other} = 3\n");
    let Ok((_, ignored)) = parse(text.as_bytes()) else {
        panic!("reads cleanly");
    };
    let listed: Vec<(&str, usize)> = ignored.iter().map(|i| (i.key.as_str(), i.line)).collect();
    assert_eq!(listed, [(long.as_str(), 1), (other.as_str(), 3)]);
    assert!(ignored[0].quoted().to_string().ends_with("...\""));
}
