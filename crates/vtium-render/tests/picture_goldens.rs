//! vtium's own goldens of a few frames with pictures (issue #30): a picture
//! at its own size, the same zoomed, on a turned display, and text over a
//! picture under a picture, and a selection over a picture. Each screen is drawn, written to
//! `$CARGO_TARGET_TMPDIR/pictures/<name>.ppm`, and compared with
//! `tests/pictures/<name>.ppm`. While `tests/pictures/provisional.tsv`
//! lists a golden, the maintainer has not reviewed it: a difference is
//! printed and judges nothing. A golden the list does not name judges its
//! screen exactly. No test writes a golden; a golden is a drawn screen
//! copied into `tests/pictures/` in a commit of its own.
#![cfg(feature = "images")]

mod common;

use std::path::{Path, PathBuf};

use common::{Screen, rgb, style, unifont};
use vtium_font::{Face, Font, Unifont};
use vtium_image::{FramePlacement, LineKey, NewImage, Place, Protocol, Rows, Screens, Store};
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
use vtium_term::{Age, CursorShape};

/// The screen's lines and a store holding `pictures`: each a protocol, a
/// kitty id, a size, its pixels' rule and where it goes.
type Picture = (Protocol, u32, (u32, u32), fn(u32, u32) -> u32, Place);

fn placements(cols: usize, rows: usize, pictures: &[Picture]) -> Vec<FramePlacement> {
    let keys: Vec<LineKey> = (1..=rows as u64).map(LineKey::new).collect();
    let view = Rows {
        screen: &keys,
        above: &[],
    };
    let screens = Screens {
        main: view,
        alt: Rows::default(),
    };
    let mut store = Store::new();
    store.set_limit(1 << 26);
    for &(protocol, id, (w, h), px, place) in pictures {
        let pixels = (0..h).flat_map(|y| (0..w).map(move |x| px(x, y))).collect();
        let new = NewImage {
            protocol,
            width: w,
            height: h,
            pixels,
            id,
            number: 0,
            transient: false,
            opaque: None,
        };
        let image = store.add_image(new, None, screens).unwrap();
        store.place(image, place, screens, cols).unwrap();
    }
    store.frame(view, cols)
}

/// A sunset: bands of warm colors down, a blue sea below, a disc of sun.
fn sunset(x: u32, y: u32) -> u32 {
    let (dx, dy) = (x as i32 - 24, y as i32 - 26);
    if dx * dx + dy * dy < 100 {
        return 0xffff_e070;
    }
    if y >= 30 {
        let v = 0x40 + (y - 30) * 6;
        return 0xff00_0000 | (v / 4) << 16 | (v / 2) << 8 | v;
    }
    0xff00_0000 | (0xff - y * 2) << 16 | (0x60 + y * 3) << 8 | (0x40 + y * 2)
}

/// A checker of half-transparent red, premultiplied, and clear squares.
fn veil(x: u32, y: u32) -> u32 {
    if (x / 4 + y / 4).is_multiple_of(2) {
        0x8070_0000
    } else {
        0
    }
}

fn text(s: &mut Screen) {
    let st = style(rgb("e0e0e0"), rgb("102030"));
    s.text(0, 0, "$ cat sunset.six", st);
    s.text(0, 4, "text beside it", st);
    s.text(0, 5, "$ _", st);
}

fn draw<F: Face>(
    font: Font<F>,
    s: &Screen,
    p: Vec<FramePlacement>,
    (w, h): (u32, u32),
    o: Orientation,
) -> Canvas {
    let config = Config {
        orientation: o,
        ..Config::default()
    };
    let mut r = Renderer::new(font, w, h, config);
    let mut c = Canvas::new(w, h, 0);
    let st = DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&s.frame(Age::NEVER).with_placements(p), &mut c, 0, &st);
    c
}

/// `s` with `p` and a selection across rows 1 to 4 from column 8 to 15,
/// over the sunset's left half and the text beside it: the selection
/// tints the picture (the maintainer, 2026-11-01).
fn selected(s: &Screen, p: Vec<FramePlacement>) -> Canvas {
    let mut r = Renderer::new(unifont(), 160, 96, Config::default());
    let mut c = Canvas::new(160, 96, 0);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let selection = vec![0..0, 8..16, 8..16, 8..16, 0..12];
    r.draw(
        &s.frame(Age::NEVER)
            .with_placements(p)
            .with_selection(selection),
        &mut c,
        0,
        &st,
    );
    c
}

/// The screens, by name.
fn screens() -> Vec<(&'static str, Canvas)> {
    let mut s = Screen::new(20, 6, rgb("c0c0c0"), rgb("000000"));
    text(&mut s);
    let one = [(
        Protocol::SIXEL,
        0,
        (48, 48),
        sunset as fn(u32, u32) -> u32,
        Place {
            col: 12,
            row: 1,
            ..Place::default()
        },
    )];
    let mut over = Screen::new(20, 6, rgb("c0c0c0"), rgb("000000"));
    let st = style(rgb("ffffff"), rgb("203040"));
    for row in 0..6 {
        over.text(0, row, "text over a picture", st);
    }
    over.cursor.visible = true;
    over.cursor.row = 3;
    over.cursor.col = 6;
    over.cursor.shape = CursorShape::Bar;
    let under_and_over = [
        (
            Protocol::KITTY,
            1,
            (48, 48),
            sunset as fn(u32, u32) -> u32,
            Place {
                col: 2,
                row: 1,
                cols: 10,
                rows: 4,
                z: -1,
                ..Place::default()
            },
        ),
        (
            Protocol::SIXEL,
            0,
            (32, 32),
            veil as fn(u32, u32) -> u32,
            Place {
                col: 5,
                row: 2,
                x: 4,
                ..Place::default()
            },
        ),
    ];
    vec![
        (
            "picture-1x",
            draw(
                unifont(),
                &s,
                placements(20, 6, &one),
                (160, 96),
                Orientation::Normal,
            ),
        ),
        (
            "picture-zoomed",
            draw(
                Font::new(Unifont::new(32)),
                &s,
                placements(20, 6, &one),
                (320, 192),
                Orientation::Normal,
            ),
        ),
        (
            "picture-turned",
            draw(
                unifont(),
                &s,
                placements(20, 6, &one),
                (96, 160),
                Orientation::Right,
            ),
        ),
        (
            "text-over-picture",
            draw(
                unifont(),
                &over,
                placements(20, 6, &under_and_over),
                (160, 96),
                Orientation::Normal,
            ),
        ),
        (
            "selection-over-picture",
            selected(&s, placements(20, 6, &one)),
        ),
    ]
}

fn ppm(c: &Canvas) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", c.width(), c.height()).into_bytes();
    for &p in c.pixels() {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    out
}

fn here(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

#[test]
fn picture_screens_against_their_goldens() {
    let listed =
        std::fs::read_to_string(here("tests/pictures/provisional.tsv")).unwrap_or_default();
    let provisional: Vec<&str> = listed
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter_map(|l| l.split('\t').next())
        .collect();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pictures");
    std::fs::create_dir_all(&out).unwrap();
    for (name, canvas) in screens() {
        let drawn = ppm(&canvas);
        let actual = out.join(format!("{name}.ppm"));
        std::fs::write(&actual, &drawn).unwrap();
        let golden = here(&format!("tests/pictures/{name}.ppm"));
        let judged = !provisional.contains(&name);
        match std::fs::read(&golden) {
            Ok(g) if g == drawn => {}
            Ok(_) if judged => panic!(
                "{name} differs from its golden; drawn: {}",
                actual.display()
            ),
            Err(e) if judged => panic!("{name} has no golden ({e}); drawn: {}", actual.display()),
            _ => println!(
                "{name}: provisional, differs from its golden or has none; drawn: {}",
                actual.display()
            ),
        }
    }
    for name in &provisional {
        assert!(
            screens().iter().any(|(n, _)| n == name),
            "{name} is listed and no screen draws it"
        );
    }
}
