//! Whole screens against their digests: each screen of
//! `reference/screens.tsv`, drawn through vtium-term's frame with unifont,
//! must give the pixels its digest in `reference/cells.tsv` covers
//! (`reference/README.md`). A dashed underline is the one exception: the
//! stored digest covers the dash's earlier shape, so the dashed screen is
//! checked with that dash put back.

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

// When a digest differs, the test writes every `screen` line as vtium draws
// it to `digests/screens.tsv` under the test's scratch directory, a dashed
// screen's with the dash the digest covers, and fails naming that file and
// how many lines differ.
// covers: platform.compositor
#[test]
fn every_screen_matches_its_digest() {
    let bare = screens(true);
    let screens = screens(false);
    assert_eq!(screens.len(), 3);
    let mut differ = Vec::new();
    let mut drawn = String::new();
    for (name, screen) in &screens {
        let canvas = draw(screen);
        let cells = dashed(name);
        let got = if cells.is_empty() {
            digest(&canvas)
        } else {
            assert_ne!(
                digest(&canvas),
                want(name),
                "screen {name} draws the stored digest's dash"
            );
            let (_, b) = bare.iter().find(|(n, _)| n == name).unwrap();
            let b = draw(b);
            digest_pixels(b.width(), b.height(), &with_dash(&b, &cells, 1, 7))
        };
        if got != want(name) {
            differ.push(name.clone());
        }
        drawn.push_str(&format!("screen\t{name}\t{got:016x}\n"));
    }
    if !differ.is_empty() {
        let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("digests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("screens.tsv");
        std::fs::write(&path, drawn).unwrap();
        panic!(
            "{} of {} screen digests differ from tests/reference/cells.tsv ({}); \
             vtium's digests for every screen line are in {}",
            differ.len(),
            screens.len(),
            differ.join(", "),
            path.display()
        );
    }
}

// A screen with a dashed underline matches its digest but for the dash:
// drawn with no dash, then given the stored digest's dash over each cell's
// middle two thirds (columns 1 to 6 of 8), it matches the digest; given
// vtium's dash from the cell's left edge for five eighths (columns 0 to
// 4), it is what vtium draws.
// covers: platform.compositor
#[test]
fn a_dashed_screen_matches_its_digest_but_for_its_dashes() {
    let drawn = screens(false);
    let mut checked = 0;
    for (name, bare) in &screens(true) {
        let cells = dashed(name);
        if cells.is_empty() {
            continue;
        }
        let bare = draw(bare);
        let (w, h) = (bare.width(), bare.height());
        let stored_dash = with_dash(&bare, &cells, 1, 7);
        assert_eq!(
            digest_pixels(w, h, &stored_dash),
            want(name),
            "screen {name}"
        );
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
