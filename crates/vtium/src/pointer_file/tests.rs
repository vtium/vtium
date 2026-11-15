use super::*;
use crate::scratch::Scratch;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;
use std::time::Duration;

/// Holds the tests that load a pointer file one at a time, since every
/// loader shares the one table and the one queue of heights.
static LOADING: Mutex<()> = Mutex::new(());

/// Takes [`LOADING`] and empties the table and the queue of heights.
fn loading() -> std::sync::MutexGuard<'static, ()> {
    let held = LOADING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    reset();
    held
}

/// Empties the table and the queue of heights, as a console starts.
fn reset() {
    let mut s = store();
    s.pictures = None;
    s.svg = false;
    s.asked.clear();
    s.waiting.clear();
    s.forget.clear();
    s.wake = None;
}

/// Reads `bytes`, the helper's answer to `ask`, as the console reads it:
/// each frame through the helper client's frame reader, then [`Reply`].
fn read_answer(bytes: &[u8], ask: &Ask) -> Result<Answer, String> {
    let mut r = bytes;
    let mut reply = Reply::new(ask);
    while let Some(event) = crate::helper::frame(&mut r) {
        match event {
            crate::helper::Event::Pointer(kind, payload) => reply.frame(kind, &payload)?,
            crate::helper::Event::Done => return Ok(reply.finish()),
            crate::helper::Event::Sandbox(_) => {}
            other => return Err(format!("{other:?}")),
        }
    }
    Err("the answer ends before its done frame".to_owned())
}

/// The font helper's answer to `ask` from its own serve code, in this
/// process, read as the console reads it: the stand-in checks what the
/// real helper checks, because it is the real helper's code.
fn helper_answer(ask: &Ask) -> Result<Answer, String> {
    let mut out = Vec::new();
    vtium_config::serve::serve(
        std::io::Cursor::new(ask.encode()),
        &mut out,
        &vtium_config::serve::Paths::default(),
        Duration::from_secs(5),
    );
    read_answer(&out, ask)
}

/// Sends `loader`'s asks to the helper's own code one at a time, as the
/// console's loop does, until it has nothing to ask; what it logged.
fn load(loader: &mut Loader) -> Log {
    while let Some(ask) = loader.next_ask() {
        loader.take(helper_answer(&ask));
    }
    loader.take_log()
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

fn white(w: u32, h: u32) -> Vec<u8> {
    vec![255u8; (w * h * 4) as usize]
}

/// The done frame, kind 2, that ends every answer.
fn done() -> Vec<u8> {
    frame_bytes(2, &[])
}

/// `frames` followed by the done frame, read as the answer to `ask`.
fn answered(ask: &Ask, frames: &[Vec<u8>]) -> Result<Answer, String> {
    let mut bytes: Vec<u8> = frames.concat();
    bytes.extend(done());
    read_answer(&bytes, ask)
}

// The console refuses each wrong picture a helper may send, as the real
// helper never sends: a picture too large, rows of the wrong length, a
// hotspot outside, a color above its alpha, a shape or a height it did
// not ask for, a size of 0, and a picture in which no pixel shows. Each
// leaves the shape with vtium's own picture.
#[test]
fn the_console_refuses_each_wrong_picture() {
    let text = shape_code(PointerShape::Text);
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
        ("blank", picture_frame(text, 16, (2, 2), (0, 0), &[0; 16])),
    ] {
        let got = answered(&text_ask(), &[frame]).unwrap();
        assert!(matches!(got, Answer::Bad(_)), "{what}: {got:?}");
    }
    let theme = Ask {
        svg: false,
        heights: Vec::new(),
        ..text_ask()
    };
    let zero = picture_frame(text, 0, (2, 2), (0, 0), &white(2, 2));
    assert!(matches!(answered(&theme, &[zero]), Ok(Answer::Bad(_))));
    // a good one passes
    let good = picture_frame(text, 16, (2, 2), (1, 1), &white(2, 2));
    assert!(matches!(
        answered(&text_ask(), std::slice::from_ref(&good)),
        Ok(Answer::Pictures(p)) if p.len() == 1
    ));
    // a no-picture frame for another shape, a frame past its kind's bound,
    // of an unknown kind, or cut short refuses the whole answer
    let mut other = Vec::new();
    put(&mut other, 0);
    put(&mut other, 1);
    assert!(answered(&text_ask(), &[frame_bytes(FRAME_NO_POINTER, &other)]).is_err());
    let mut huge = Vec::new();
    put(&mut huge, FRAME_POINTER);
    put(&mut huge, PICTURE_BYTES_MAX + 1);
    assert!(read_answer(&huge, &text_ask()).is_err());
    assert!(read_answer(&frame_bytes(99, &[]), &text_ask()).is_err());
    assert!(read_answer(&good[..good.len() - 9], &text_ask()).is_err());
}

// The console bounds what one answer brings. A cursor theme's shape keeps
// at most POINTER_THEME_PIXELS_MAX pixels: four pictures of 64 × 64 pass,
// and a fifth refuses the file. With pointer-size set, a second picture
// refuses the whole answer, as do more pictures of an SVG file than the
// heights asked.
#[test]
fn the_console_bounds_the_pictures_of_one_answer() {
    let text = shape_code(PointerShape::Text);
    let theme = Ask {
        svg: false,
        heights: Vec::new(),
        ..text_ask()
    };
    let big = |size| picture_frame(text, size, (64, 64), (0, 0), &white(64, 64));
    let four: Vec<Vec<u8>> = (1..=4).map(big).collect();
    assert!(matches!(
        answered(&theme, &four),
        Ok(Answer::Pictures(p)) if p.len() == 4
    ));
    let five: Vec<Vec<u8>> = (1..=5).map(big).collect();
    let got = answered(&theme, &five).unwrap();
    assert!(
        matches!(&got, Answer::Bad(why) if why.contains("16384 pixels")),
        "{got:?}"
    );
    let sized = Ask {
        size: Some(24),
        ..theme
    };
    let small = |size| picture_frame(text, size, (2, 2), (0, 0), &white(2, 2));
    assert!(answered(&sized, &[small(24)]).is_ok());
    let why = answered(&sized, &[small(24), small(32)]).unwrap_err();
    assert!(why.contains("more than 1 pictures"), "{why}");
    let why = answered(&text_ask(), &[small(16), small(16)]).unwrap_err();
    assert!(why.contains("more than 1 pictures"), "{why}");
}

// A cursor theme: each shape takes the first of its names that vtium reads
// and the helper draws, one file at a time. A symlink is followed, a file
// others can write is skipped with a warning, a file the helper cannot
// read is skipped with a warning and the next name is tried, and the
// shapes with no file get one info line. The table goes to the displays
// once the last shape is answered, and the loader then ends.
#[test]
fn a_theme_takes_each_shapes_first_drawable_file_in_order() {
    let _held = loading();
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
    let mut loader = Loader::start(&dir, None, trust.clone()).unwrap();
    let log = load(&mut loader);
    assert!(loader.ended());
    let table = loader.table();
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
    assert!(current().is_none());
    assert!(loader.publish());
    assert!(!loader.publish());
    assert_eq!(current().unwrap().sizes_of(PointerShape::Wait), [48]);
    let mut sized = Loader::start(&dir, Some(24), trust).unwrap();
    load(&mut sized);
    assert_eq!(sized.table().sizes_of(PointerShape::Default), [24]);
}

// An SVG file: vtium's shipped file draws every shape at each height a
// display wants; a file that lacks shapes logs them once; a symbol the
// helper cannot draw is logged once and not asked for again.
#[test]
fn an_svg_file_draws_at_each_height_asked_and_logs_once() {
    let _held = loading();
    let (dir, trust) = scratch("svg");
    let shipped =
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/install/pointers.svg")).unwrap();
    let path = dir.join("pointers.svg");
    write(&path, &shipped, 0o644);
    let mut svg = Loader::start(&path, None, trust.clone()).unwrap();
    // nothing is asked until a display wants a height
    assert_eq!(svg.next_ask(), None);
    want(16);
    want(16);
    assert_eq!(load(&mut svg), Log::default());
    assert!(
        PointerShape::ALL
            .iter()
            .all(|&s| svg.table().sizes_of(s) == [16])
    );
    want(20);
    load(&mut svg);
    assert_eq!(svg.table().sizes_of(PointerShape::Wait), [16, 20]);
    assert!(!svg.ended());
    // a file with the default shape alone, and a wait symbol with no hotspot
    reset();
    let small = dir.join("small.svg");
    write(
        &small,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><rect width=\"8\" height=\"16\" fill=\"#fff\"/></symbol><symbol id=\"wait\" viewBox=\"0 0 8 16\"/></svg>",
        0o644,
    );
    let mut s = Loader::start(&small, None, trust).unwrap();
    want(16);
    let log = load(&mut s);
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
    want(20);
    assert_eq!(load(&mut s), Log::default());
    assert_eq!(s.table().sizes_of(PointerShape::Default), [16, 20]);
}

// More heights than one ask carries are all drawn, over two asks of each
// shape, and none is lost. Past SVG_HEIGHTS_KEPT heights the oldest height
// is dropped, from the table too, and drawn again when a display wants it
// again.
#[test]
fn every_height_wanted_is_drawn_and_the_oldest_makes_room() {
    let _held = loading();
    let (dir, trust) = scratch("heights");
    let path = dir.join("one.svg");
    write(
        &path,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><rect width=\"8\" height=\"16\" fill=\"#fff\"/></symbol></svg>",
        0o644,
    );
    let mut svg = Loader::start(&path, None, trust).unwrap();
    let heights: Vec<u32> = (8..).take(SVG_HEIGHTS_KEPT).collect();
    for &h in &heights {
        want(h);
    }
    load(&mut svg);
    assert_eq!(svg.table().sizes_of(PointerShape::Default), heights);
    want(40);
    load(&mut svg);
    let kept = svg.table().sizes_of(PointerShape::Default);
    assert_eq!(kept.len(), SVG_HEIGHTS_KEPT);
    assert!(!kept.contains(&8) && kept.contains(&40), "{kept:?}");
    want(8);
    load(&mut svg);
    let kept = svg.table().sizes_of(PointerShape::Default);
    assert!(kept.contains(&8) && !kept.contains(&9), "{kept:?}");
}

// When the font helper fails during an ask, the loader gives the file up:
// it logs one line, keeps what it has, asks for nothing more, and a new
// height starts no helper.
#[test]
fn a_failed_helper_gives_the_file_up() {
    let _held = loading();
    let (dir, trust) = scratch("failed");
    let shipped =
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/install/pointers.svg")).unwrap();
    let path = dir.join("pointers.svg");
    write(&path, &shipped, 0o644);
    let mut svg = Loader::start(&path, None, trust).unwrap();
    want(16);
    let first = svg.next_ask().unwrap();
    svg.take(helper_answer(&first));
    assert!(svg.next_ask().is_some());
    svg.take(Err("the font helper gave no answer for 10 s".to_owned()));
    assert!(svg.ended());
    assert_eq!(svg.next_ask(), None);
    want(20);
    assert_eq!(svg.next_ask(), None);
    assert!(store().waiting.is_empty());
    assert_eq!(svg.table().sizes_of(PointerShape::Default), [16]);
    assert_eq!(
        svg.take_log().warnings,
        [format!(
            "The font helper cannot draw the pointer file {}, so vtium draws its own pictures for the shapes it has no picture for yet. The reason: the font helper gave no answer for 10 s.",
            path.display()
        )]
    );
}
