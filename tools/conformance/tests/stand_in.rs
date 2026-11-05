//! The image store judged against the image cases' goldens without the
//! sixel handler (ADR-1100, D12 step 1).
//!
//! Each `img-` case draws its sixels as real sixel strings, which only the
//! sixel handler reads. A sixel string of one solid color is a picture the
//! replay's `image` and `place` commands can make as well: this test
//! replaces each such string with those two commands, at the cursor the
//! string was sent at, and replays the case. The store's events then judge
//! the screen events' rules against the case's goldens, which the output
//! must match exactly. A case whose sixels are not all one solid color is
//! left to the sixel handler.

#![cfg(feature = "images")]

use std::path::Path;

use vtium_conformance::replay::{self, Input};
use vtium_conformance::runner::{Corpus, mask};
use vtium_conformance::view::View;

const TOOL: &str = env!("CARGO_MANIFEST_DIR");

/// A solid sixel: its size and its color as premultiplied AARRGGBB.
struct Solid {
    width: u32,
    height: u32,
    fill: u32,
}

/// The sixel string `body` (from after `ESC P` to before `ESC \`) as a
/// solid picture, or `None` when it has a transparent background, more
/// than one color, or a pixel it leaves unset.
fn solid(body: &[u8]) -> Option<Solid> {
    let q = body.iter().position(|&b| b == b'q')?;
    let params: Vec<&[u8]> = body.get(..q)?.split(|&b| b == b';').collect();
    if params.get(1).is_some_and(|p| *p == b"1") {
        return None;
    }
    let data = body.get(q + 1..)?;
    let text = std::str::from_utf8(data).ok()?;
    let raster = text.strip_prefix("\"1;1;")?;
    let end = raster.find(|c: char| !c.is_ascii_digit() && c != ';')?;
    let mut dims = raster.get(..end)?.split(';').map(|n| n.parse::<u32>().ok());
    let (width, height) = (dims.next()??, dims.next()??);
    let rest = raster.get(end..)?;
    let rest = rest.strip_prefix("#1;2;")?;
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != ';')?;
    let mut rgb = rest.get(..end)?.split(';').map(|n| n.parse::<u32>().ok());
    let pct = |p: u32| (p * 255 + 50) / 100;
    let (r, g, b) = (pct(rgb.next()??), pct(rgb.next()??), pct(rgb.next()??));
    let bands = rest.get(end..)?.strip_prefix("#1")?;
    // every pixel of the raster set, in color 1
    let mut set = vec![false; (width * height) as usize];
    let (mut x, mut band) = (0u32, 0u32);
    for c in bands.bytes() {
        match c {
            b'-' => {
                band += 1;
                x = 0;
            }
            b'?'..=b'~' => {
                let bits = c - b'?';
                for k in 0..6 {
                    let y = band * 6 + k;
                    if bits & (1 << k) != 0 && x < width && y < height {
                        set[(y * width + x) as usize] = true;
                    }
                }
                x += 1;
            }
            _ => return None,
        }
    }
    set.iter().all(|&s| s).then_some(Solid {
        width,
        height,
        fill: 0xff00_0000 | r << 16 | g << 8 | b,
    })
}

/// The case's stream with every sixel string replaced by `image` and
/// `place` at the cursor, or `None` when one is not solid.
fn stand_in(stream: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = stream;
    let mut n = 40;
    while let Some(at) = rest.windows(2).position(|w| w == b"\x1bP") {
        out.extend_from_slice(&rest[..at]);
        let body = &rest[at + 2..];
        let end = body.windows(2).position(|w| w == b"\x1b\\")?;
        let s = solid(&body[..end])?;
        out.extend_from_slice(
            format!(
                "\x1b_tsm-test;image;n={n};w={};h={};fill={:08x}\x1b\\\x1b_tsm-test;place;img={n}\x1b\\",
                s.width, s.height, s.fill
            )
            .as_bytes(),
        );
        n += 1;
        rest = &body[end + 2..];
    }
    out.extend_from_slice(rest);
    Some(out)
}

// covers: image.pipeline
#[test]
fn the_store_meets_the_image_goldens_with_its_solid_sixels_stood_in() {
    let corpus = Corpus::open(&Path::new(TOOL).join("corpus")).unwrap_or_else(|e| panic!("{e}"));
    let mut skipped = Vec::new();
    let mut failures = Vec::new();
    for case in corpus.cases.iter().filter(|c| c.name.starts_with("img-")) {
        for (view, dir) in [(View::Screen, "golden"), (View::Frame, "golden-frame")] {
            let input = corpus.input(case, view).unwrap_or_else(|e| panic!("{e}"));
            let data = stand_in(&input.data).filter(|_| input.cuts.is_empty());
            let Some(data) = data else {
                skipped.push(format!("{} {dir}", case.name));
                continue;
            };
            let got = replay::run(&Input { data, ..input });
            let got = match got {
                Ok(out) => mask(&out),
                Err(f) => {
                    failures.push(format!(
                        "{} {dir}: exit {}: {}",
                        case.name, f.code, f.message
                    ));
                    continue;
                }
            };
            let want = mask(&corpus.golden(case, dir).unwrap_or_else(|e| panic!("{e}")));
            if got != want {
                failures.push(format!(
                    "{} {dir}:\n--- golden\n{want}--- vtium\n{got}",
                    case.name
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // every case is judged but the one whose sixels have holes and a clear
    // background, which waits for the sixel handler
    assert_eq!(
        skipped,
        ["img-sixel-over golden", "img-sixel-over golden-frame"]
    );
}

#[test]
fn a_sixel_with_a_hole_or_a_clear_background_is_not_stood_in() {
    let solid_blue = b"0;0;0q\"1;1;2;6#1;2;0;0;100#1~~";
    assert!(solid(solid_blue).is_some());
    assert_eq!(solid(solid_blue).map(|s| s.fill), Some(0xff00_00ff));
    assert!(solid(b"0;1;0q\"1;1;2;6#1;2;0;0;100#1~~").is_none());
    assert!(solid(b"0;0;0q\"1;1;2;6#1;2;0;0;100#1~}").is_none());
}
