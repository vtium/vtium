//! Whole screens against the C reference: each screen of
//! `reference/screens.tsv`, drawn through vtium-term's frame with unifont,
//! must give the pixels `reference/refcells.c` drew with the C reference's
//! glyphs, sprites and blending (`reference/README.md`), but for the dashed
//! underline, which is the render crate's departure.

mod common;

use common::{Screen, digest, digest_pixels, parse_screens, reference_screens_tsv, rgb, unifont};
use vtium_font::Decorations;
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::Age;

fn want(name: &str) -> u64 {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/reference/cells.tsv");
    let tsv = std::fs::read_to_string(path).expect("cells.tsv");
    tsv.lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f[0] == "screen" && f[1] == name)
        .map(|f| u64::from_str_radix(f[2], 16).expect("hex"))
        .unwrap_or_else(|| panic!("no digest for {name}"))
}

/// `screen` drawn in unifont's 8 by 16 cells.
fn draw(screen: &Screen) -> Canvas {
    let (w, h) = (screen.cols as u32 * 8, screen.rows as u32 * 16);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut canvas = Canvas::new(w, h, 0);
    let state = DrawState {
        background: screen.palette.bg(),
        ..DrawState::default()
    };
    r.draw(&screen.frame(Age::NEVER), &mut canvas, 0, &state);
    canvas
}

/// The screens of screens.tsv, the dashed cells' underline taken off when
/// `bare`.
fn screens(bare: bool) -> Vec<(String, Screen)> {
    let tsv: Vec<String> = reference_screens_tsv()
        .lines()
        .map(|l| {
            let mut f: Vec<&str> = l.split('\t').collect();
            if bare && f[0] == "cell" && f[8] == "5" {
                f[8] = "0";
            }
            f.join("\t")
        })
        .collect();
    parse_screens(&tsv.join("\n"))
}

/// The dashed cells of screen `name`: column, row, cells and line color.
fn dashed(name: &str) -> Vec<(u32, u32, u32, u32)> {
    let mut screen = String::new();
    let mut out = Vec::new();
    for line in reference_screens_tsv().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "screen" => screen = f[1].to_owned(),
            "cell" if screen == name && f[8] == "5" => {
                let color = if f[10] == "-" { f[5] } else { f[10] };
                let c = rgb(color);
                out.push((
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    f[4].parse().unwrap(),
                    u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b),
                ));
            }
            _ => {}
        }
    }
    out
}

/// `bare`'s pixels with a dash over columns `[x0, x1)` of each of `cells`'
/// 8-pixel cells, on unifont's underline.
fn with_dash(bare: &Canvas, cells: &[(u32, u32, u32, u32)], x0: u32, x1: u32) -> Vec<u32> {
    let m = Decorations::for_height(16);
    let w = bare.width();
    let mut px = bare.pixels().to_vec();
    for &(col, row, n, color) in cells {
        for y in m.underline..m.underline + m.underline_thickness {
            for c in col..col + n {
                for x in x0..x1 {
                    px[((row * 16 + y) * w + c * 8 + x) as usize] = color;
                }
            }
        }
    }
    px
}

// covers: platform.compositor
#[test]
fn screens_match_the_c_reference() {
    let screens = screens(false);
    assert_eq!(screens.len(), 3);
    for (name, screen) in &screens {
        let canvas = draw(screen);
        if dashed(name).is_empty() {
            assert_eq!(digest(&canvas), want(name), "screen {name}");
        } else {
            assert_ne!(
                digest(&canvas),
                want(name),
                "screen {name} keeps the C dash"
            );
        }
    }
}

// A screen with a dashed underline is the C reference's but for the dash:
// drawn with no dash, then given the C reference's over each cell's middle
// two thirds (columns 1 to 6 of 8), it is the C reference's screen; given
// vtium's from the cell's left edge for five eighths (columns 0 to 4), it
// is what vtium draws.
// covers: platform.compositor
#[test]
fn a_dashed_screen_differs_from_the_c_reference_only_in_its_dashes() {
    let drawn = screens(false);
    let mut checked = 0;
    for (name, bare) in &screens(true) {
        let cells = dashed(name);
        if cells.is_empty() {
            continue;
        }
        let bare = draw(bare);
        let (w, h) = (bare.width(), bare.height());
        let c_dash = with_dash(&bare, &cells, 1, 7);
        assert_eq!(digest_pixels(w, h, &c_dash), want(name), "screen {name}");
        let (_, screen) = drawn.iter().find(|(n, _)| n == name).unwrap();
        assert_eq!(
            with_dash(&bare, &cells, 0, 5),
            draw(screen).pixels(),
            "screen {name}"
        );
        checked += 1;
    }
    assert_eq!(checked, 1);
}
