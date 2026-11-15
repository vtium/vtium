use super::*;
use crate::scratch::Scratch;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

/// A round through the font helper's own serve code, in this process, as
/// the helper answers: the stand-in checks what the real helper checks,
/// because it is the real helper's code.
fn helper_round(asks: &[Ask]) -> Result<Vec<Answer>, String> {
    let input: Vec<u8> = asks.iter().flat_map(Ask::encode).collect();
    let mut out = Vec::new();
    vtium_config::serve::serve(
        std::io::Cursor::new(input),
        &mut out,
        &vtium_config::serve::Paths::default(),
        Duration::from_secs(5),
    );
    answers(&mut &out[..], asks)
}

/// A directory of the test's own whose folders pass the ownership check
/// that `trust` makes, removed when the test ends.
fn scratch(name: &str) -> (Scratch, Trust) {
    let dir = Scratch::new(&format!("pointer-file-{name}"));
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&dir).unwrap());
    let trust = Trust::owner(uid).below(&dir);
    (dir, trust)
}

fn write(path: &Path, bytes: &[u8], mode: u32) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A cursor file with one opaque white image at each of `sizes`, each as
/// many pixels a side as its size, its hotspot at its centre.
fn cursor(sizes: &[u32]) -> Vec<u8> {
    let n = u32::try_from(sizes.len()).unwrap();
    let mut out = b"Xcur".to_vec();
    for v in [16, 0x1_0000, n] {
        put(&mut out, v);
    }
    let mut at = 16 + 12 * n;
    for &s in sizes {
        for v in [0xfffd_0002, s, at] {
            put(&mut out, v);
        }
        at += 36 + s * s * 4;
    }
    for &s in sizes {
        for v in [36, 0xfffd_0002, s, 1, s, s, s / 2, s / 2, 0] {
            put(&mut out, v);
        }
        for _ in 0..s * s {
            put(&mut out, 0xffff_ffff);
        }
    }
    out
}

// The console's ask is the layout the helper reads: vtium-config's own
// encoding of the same ask gives the same bytes.
#[test]
fn the_consoles_ask_is_the_helpers_layout() {
    for (svg, size, heights) in [(true, None, vec![16, 20]), (false, Some(24), vec![])] {
        let ask = Ask {
            svg,
            shape: PointerShape::EwResize,
            size,
            heights: heights.clone(),
            bytes: Arc::new(b"bytes".to_vec()),
        };
        let theirs = vtium_config::serve::PointerAsk {
            file: if svg {
                vtium_config::serve::PointerFile::Svg
            } else {
                vtium_config::serve::PointerFile::Xcursor
            },
            shape: PointerShape::EwResize,
            size,
            heights,
            bytes: b"bytes".to_vec(),
        };
        assert_eq!(ask.encode(), theirs.encode());
    }
}

// A regular file is an SVG file, a folder that holds `cursors` is a theme,
// and a folder without one or a missing path is neither.
#[test]
fn the_path_names_a_file_a_theme_or_neither() {
    let (dir, _) = scratch("locate");
    let (file, theme, default) = (dir.join("p.svg"), dir.join("Adwaita"), dir.join("default"));
    write(&file, b"<svg/>", 0o644);
    std::fs::create_dir_all(theme.join("cursors")).unwrap();
    std::fs::create_dir_all(&default).unwrap();
    std::fs::write(default.join("index.theme"), "Inherits=Adwaita\n").unwrap();
    assert_eq!(locate(&file), Located::Svg(file.clone()));
    assert_eq!(locate(&theme), Located::Theme(theme.clone()));
    assert_eq!(locate(&default), Located::Unusable(default.clone()));
    assert_eq!(
        locate(&dir.join("none")),
        Located::Unusable(dir.join("none"))
    );
    assert_eq!(
        unusable_line(&default),
        format!(
            "The pointer file {} is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures.",
            default.display()
        )
    );
}

// The names vtium tries for each shape, in order.
#[test]
fn each_shape_tries_its_names_in_order() {
    assert_eq!(theme_names(PointerShape::Default), ["default", "left_ptr"]);
    assert_eq!(theme_names(PointerShape::Move), ["all-scroll", "fleur"]);
    assert_eq!(
        theme_names(PointerShape::EwResize),
        ["ew-resize", "sb_h_double_arrow", "col-resize"]
    );
    assert!(
        PointerShape::ALL
            .iter()
            .all(|&s| !theme_names(s).contains(&"hand1"))
    );
}

// The console reads a pointer file under the ownership check, and says why
// it does not: too long, or writable by others with the command that
// fixes it.
#[test]
fn a_file_vtium_does_not_read_is_logged_with_why() {
    let (dir, trust) = scratch("read");
    let (long, open) = (dir.join("long"), dir.join("open"));
    write(&long, &[0; 32], 0o644);
    write(&open, b"x", 0o666);
    assert_eq!(
        read(&long, &trust, 16),
        Err(format!(
            "vtium does not read the pointer file {}, because it is longer than 1 MiB.",
            long.display()
        ))
    );
    let why = read(&open, &trust, 16).unwrap_err();
    assert!(
        why.contains("other users can change") && why.contains("chmod go-w"),
        "{why}"
    );
    assert_eq!(read(&dir.join("none"), &trust, 16), Ok(None));
}

/// The ask `answers` checks frames against: the text shape at 16.
fn text_ask() -> Ask {
    Ask {
        svg: true,
        shape: PointerShape::Text,
        size: None,
        heights: vec![16],
        bytes: Arc::new(Vec::new()),
    }
}

fn frame_bytes(kind: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put(&mut out, kind);
    put(&mut out, u32::try_from(payload.len()).unwrap());
    out.extend_from_slice(payload);
    out
}

/// A picture frame's bytes for the text shape.
fn picture_frame(
    shape: u32,
    size: u32,
    (w, h): (u32, u32),
    hot: (u32, u32),
    rgba: &[u8],
) -> Vec<u8> {
    let mut p = Vec::new();
    for v in [shape, size, w, h, hot.0, hot.1] {
        put(&mut p, v);
    }
    p.extend_from_slice(rgba);
    frame_bytes(FRAME_POINTER, &p)
}

// The console refuses each wrong picture a helper may send, as the real
// helper never sends: a picture too large, rows of the wrong length, a
// hotspot outside, a color above its alpha, a shape or a height it did
// not ask for. Each leaves the shape with vtium's own picture.
#[test]
fn the_console_refuses_each_wrong_picture() {
    let text = shape_code(PointerShape::Text);
    let white = |w: u32, h: u32| vec![255u8; (w * h * 4) as usize];
    let mut over = white(2, 2);
    over[0] = 0;
    over[3] = 10;
    for (what, frame) in [
        (
            "too large",
            picture_frame(text, 16, (65, 1), (0, 0), &white(65, 1)),
        ),
        (
            "rows",
            picture_frame(text, 16, (2, 2), (0, 0), &white(2, 1)),
        ),
        (
            "hotspot",
            picture_frame(text, 16, (2, 2), (2, 0), &white(2, 2)),
        ),
        ("alpha", picture_frame(text, 16, (2, 2), (0, 0), &over)),
        ("shape", picture_frame(0, 16, (2, 2), (0, 0), &white(2, 2))),
        (
            "height",
            picture_frame(text, 17, (2, 2), (0, 0), &white(2, 2)),
        ),
    ] {
        let mut bytes = frame;
        bytes.extend(frame_bytes(FRAME_DONE, &[]));
        let got = answers(&mut &bytes[..], &[text_ask()]).unwrap();
        assert!(
            matches!(got.as_slice(), [Answer::Refused(_)]),
            "{what}: {got:?}"
        );
    }
    // a good one passes
    let mut good = picture_frame(text, 16, (2, 2), (1, 1), &white(2, 2));
    good.extend(frame_bytes(FRAME_DONE, &[]));
    assert!(matches!(
        answers(&mut &good[..], &[text_ask()]).unwrap().as_slice(),
        [Answer::Pictures(p)] if p.len() == 1
    ));
    // a frame past its kind's bound, of an unknown kind, or cut short
    // refuses the whole answer
    let mut huge = Vec::new();
    put(&mut huge, FRAME_POINTER);
    put(&mut huge, PICTURE_BYTES_MAX + 1);
    assert!(answers(&mut &huge[..], &[text_ask()]).is_err());
    assert!(answers(&mut &frame_bytes(99, &[])[..], &[text_ask()]).is_err());
    assert!(answers(&mut &good[..good.len() - 9], &[text_ask()]).is_err());
}

// A cursor theme: each shape takes the first of its names that vtium reads
// and the helper draws. A symlink is followed, a file others can write is
// skipped with a warning, a file the helper cannot read is skipped with a
// warning and the next name is tried, and the shapes with no file get one
// info line.
#[test]
fn a_theme_takes_each_shapes_first_drawable_file_in_order() {
    let (dir, trust) = scratch("theme");
    let cursors = dir.join("cursors");
    std::fs::create_dir_all(&cursors).unwrap();
    std::fs::set_permissions(&cursors, std::fs::Permissions::from_mode(0o755)).unwrap();
    // default: only the X name
    write(&cursors.join("left_ptr"), &cursor(&[24, 32]), 0o644);
    // text: a symlink to the X name's file
    write(&cursors.join("xterm"), &cursor(&[24]), 0o644);
    std::os::unix::fs::symlink("xterm", cursors.join("text")).unwrap();
    // pointer: the first file is writable by others, the second serves
    write(&cursors.join("pointer"), &cursor(&[24]), 0o666);
    write(&cursors.join("hand2"), &cursor(&[30]), 0o644);
    // wait: the first file is not a cursor file, the second serves
    write(&cursors.join("wait"), b"not a cursor", 0o644);
    write(&cursors.join("watch"), &cursor(&[48]), 0o644);
    // every other shape but ns-resize, by its first name
    for name in [
        "crosshair",
        "progress",
        "not-allowed",
        "all-scroll",
        "ew-resize",
    ] {
        write(&cursors.join(name), &cursor(&[24]), 0o644);
    }
    let (table, log) = load_theme(&dir, &trust, None, &mut helper_round);
    assert_eq!(table.sizes_of(PointerShape::Default), [24, 32]);
    assert_eq!(table.sizes_of(PointerShape::Text), [24]);
    assert_eq!(table.sizes_of(PointerShape::Pointer), [30]);
    assert_eq!(table.sizes_of(PointerShape::Wait), [48]);
    assert!(!table.has(PointerShape::NsResize));
    assert_eq!(log.warnings.len(), 2, "{log:?}");
    assert!(
        log.warnings[0].contains("pointer") && log.warnings[0].contains("chmod go-w"),
        "{log:?}"
    );
    assert!(
        log.warnings[1].starts_with("The font helper cannot draw the wait shape from")
            && log.warnings[1].contains("Xcur"),
        "{log:?}"
    );
    assert_eq!(
        log.infos,
        [format!(
            "The pointer file {} has no picture for the ns-resize shape, so vtium draws its own pictures for it.",
            dir.display()
        )]
    );
    // a renderer picks among the sizes kept; pointer-size keeps one
    assert_eq!(
        table.pick(PointerShape::Default, 31).map(Arrow::width),
        Some(32)
    );
    let (sized, _) = load_theme(&dir, &trust, Some(24), &mut helper_round);
    assert_eq!(sized.sizes_of(PointerShape::Default), [24]);
}

// An SVG file: vtium's shipped file draws every shape at each height asked;
// a file that lacks shapes logs them once; a symbol the helper cannot draw
// is logged once and not asked for again.
#[test]
fn an_svg_file_draws_at_each_height_asked_and_logs_once() {
    let (dir, trust) = scratch("svg");
    let shipped =
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/install/pointers.svg")).unwrap();
    let path = dir.join("pointers.svg");
    write(&path, &shipped, 0o644);
    let mut svg = SvgFile::read(&path, &trust).unwrap();
    let log = svg.draw(&[16], &mut helper_round);
    assert_eq!(log, Log::default());
    assert!(
        PointerShape::ALL
            .iter()
            .all(|&s| svg.table().sizes_of(s) == [16])
    );
    assert!(svg.table().lacks(20));
    svg.draw(&[20], &mut helper_round);
    assert!(!svg.table().lacks(20));
    // a file with the default shape alone, and a wait symbol with no hotspot
    let small = dir.join("small.svg");
    write(
        &small,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><rect width=\"8\" height=\"16\" fill=\"#fff\"/></symbol><symbol id=\"wait\" viewBox=\"0 0 8 16\"/></svg>",
        0o644,
    );
    let mut s = SvgFile::read(&small, &trust).unwrap();
    let log = s.draw(&[16], &mut helper_round);
    assert_eq!(
        log.warnings,
        [format!(
            "The font helper cannot draw the wait shape from {}, because its symbol has no data-hotspot. vtium draws its own picture for that shape.",
            small.display()
        )]
    );
    assert_eq!(
        log.infos,
        [format!(
            "The pointer file {} has no picture for the text, pointer, crosshair, progress, not-allowed, move, ew-resize and ns-resize shapes, so vtium draws its own pictures for them.",
            small.display()
        )]
    );
    assert_eq!(s.draw(&[20], &mut helper_round), Log::default());
    assert_eq!(s.table().sizes_of(PointerShape::Default), [16, 20]);
}
