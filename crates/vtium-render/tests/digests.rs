//! Whole screens against their digests: each screen of
//! `digests/screens.tsv`, drawn through vtium-term's frame with unifont,
//! must give the pixels its digest in `digests/cells.tsv` covers
//! (`digests/README.md`). A dashed underline is the one exception: the
//! stored digest covers the dash's earlier shape, so the dashed screen is
//! checked with that dash put back.

mod common;

use common::{Screen, digest, digest_pixels, digest_screens_tsv, parse_screens, rgb, unifont};
use vtium_font::Decorations;
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::Age;

fn want(name: &str) -> u64 {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/digests/cells.tsv");
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
    let tsv: Vec<String> = digest_screens_tsv()
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

/// A dashed cell of a screen: its column, row, width in cells, background
/// and line color, as `0x00RRGGBB`.
type Dashed = (u32, u32, u32, u32, u32);

fn hex(color: &str) -> u32 {
    let c = rgb(color);
    u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
}

/// The dashed cells of screen `name`.
fn dashed(name: &str) -> Vec<Dashed> {
    let mut screen = String::new();
    let mut out = Vec::new();
    for line in digest_screens_tsv().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "screen" => screen = f[1].to_owned(),
            "cell" if screen == name && f[8] == "5" => {
                let color = if f[10] == "-" { f[5] } else { f[10] };
                out.push((
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    f[4].parse().unwrap(),
                    hex(f[6]),
                    hex(color),
                ));
            }
            _ => {}
        }
    }
    out
}

/// `bare`'s pixels with a dash over columns `[x0, x1)` of each of `cells`'
/// 8-pixel cells, on unifont's underline. The dash skips ink the way vtium
/// draws every underline: it leaves out each pixel that has glyph ink in
/// itself or in the pixel on either side of it, along the same pixel row and
/// inside the same character cell. `bare` draws no underline, so a pixel of
/// it in the underline's rows that differs from the cell's background is
/// glyph ink.
fn with_dash(bare: &Canvas, cells: &[Dashed], x0: u32, x1: u32) -> Vec<u32> {
    let m = Decorations::for_height(16);
    let w = bare.width();
    let ink = bare.pixels();
    let mut px = ink.to_vec();
    for &(col, row, n, bg, color) in cells {
        let (left, right) = (col * 8, (col + n) * 8);
        for y in m.underline..m.underline + m.underline_thickness {
            let line = (row * 16 + y) * w;
            for c in col..col + n {
                for x in (x0..x1).map(|x| c * 8 + x) {
                    let near_ink = (x.saturating_sub(1)..=x + 1)
                        .filter(|&i| (left..right).contains(&i))
                        .any(|i| ink[(line + i) as usize] != bg);
                    if !near_ink {
                        px[(line + x) as usize] = color;
                    }
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
            "{} of {} screen digests differ from tests/digests/cells.tsv ({}); \
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
// 4), it is what vtium draws. Both dashes leave a gap around the glyph's
// ink, as vtium's underlines do.
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
