//! What pictures cost a frame on the minspec display, 1366x768 in
//! unifont's 8x16 cells: a full redraw with no picture, under one
//! full-screen picture at its own size, under one scaled to the screen,
//! and with a picture of ten cells; and a frame where only the ten-cell
//! picture moves. They are machine figures and are printed, not pinned,
//! and the test is left out of the gates:
//!
//! `cargo test --release -p vtium-render --features images --test picture_figures -- --ignored --nocapture`
#![cfg(feature = "images")]

mod common;

use std::time::Duration;

use common::{Screen, rgb, style, unifont};
use vtium_image::{FramePlacement, LineKey, NewImage, Place, Protocol, Rows, Screens, Store};
use vtium_render::{Config, DrawState, Mapped, Renderer};
use vtium_term::Age;

const W: u32 = 1366;

/// A case's placements for each frame.
type Placements<'a> = Box<dyn Fn(usize) -> Vec<FramePlacement> + 'a>;
const H: u32 = 768;

fn text(s: &mut Screen) {
    let words = "the quick brown fox jumps over a lazy dog; fn main() { println!(\"hi\"); } ";
    let st = style(rgb("c0c0c0"), rgb("101010"));
    for row in 0..s.rows {
        let line: String = words.chars().cycle().skip(row * 7).take(s.cols).collect();
        s.text(0, row, &line, st);
    }
}

/// A store holding one picture `w` by `h` made in cells of `cell`, placed
/// at `col`, `row` of a screen `cols` by `rows`.
fn picture(
    (cols, rows): (usize, usize),
    cell: (u32, u32),
    (w, h): (u32, u32),
    at: (u32, u32),
) -> Vec<FramePlacement> {
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
    store.set_limit(1 << 30);
    store.set_cell(cell.0, cell.1).unwrap();
    let pixels = (0..h)
        .flat_map(|y| (0..w).map(move |x| 0xff00_0000 | (x & 0xff) << 16 | (y & 0xff) << 8 | 0x40))
        .collect();
    let new = NewImage {
        protocol: Protocol::SIXEL,
        width: w,
        height: h,
        pixels,
        id: 0,
        number: 0,
        transient: false,
        opaque: None,
    };
    let image = store.add_image(new, None, screens).unwrap();
    let place = Place {
        col: at.0,
        row: at.1,
        ..Place::default()
    };
    store.place(image, place, screens, cols).unwrap();
    store.frame(view, cols)
}

/// The median and the largest of `t`.
fn summary(mut t: Vec<Duration>) -> String {
    t.sort();
    format!(
        "median {:.3} ms, max {:.3} ms",
        t[t.len() / 2].as_secs_f64() * 1e3,
        t[t.len() - 1].as_secs_f64() * 1e3
    )
}

#[test]
#[ignore = "machine figures, run by hand: see the file's documentation"]
fn picture_figures_at_1366x768() {
    let mut buffer = vec![0u8; W as usize * 4 * H as usize];
    let probe = Renderer::new(unifont(), W, H, Config::default());
    let (cols, rows) = probe.grid_size();
    let mut s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("101010"));
    text(&mut s);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    // a full redraw of every cell, 20 frames into a buffer drawn whole
    let full = |placements: &dyn Fn(usize) -> Vec<FramePlacement>, buffer: &mut [u8]| {
        let mut r = Renderer::new(unifont(), W, H, Config::default());
        let mut times = Vec::new();
        let mut first = Duration::ZERO;
        for i in 0..21 {
            let mut m = Mapped::new(buffer, W, H, W as usize * 4).unwrap();
            let d = r.draw(
                &s.frame(Age::NEVER).with_placements(placements(i)),
                &mut m,
                0,
                &st,
            );
            if i == 0 {
                first = d.stats.cpu;
            } else {
                times.push(d.stats.cpu);
            }
        }
        (first, times, r.picture_counts())
    };
    let gw = cols as u32 * 8;
    let gh = rows as u32 * 16;
    let cases: [(&str, Placements); 5] = [
        ("no picture", Box::new(|_| Vec::new())),
        (
            "one full-screen picture at its own size",
            Box::new(|_| picture((cols, rows), (8, 16), (gw, gh), (0, 0))),
        ),
        (
            "one full-screen picture made in 4x8 cells, scaled to the screen",
            Box::new(|_| picture((cols, rows), (4, 8), (gw / 2, gh / 2), (0, 0))),
        ),
        (
            "text over one full-screen kitty picture below text",
            Box::new(|_| {
                let mut p = picture((cols, rows), (8, 16), (gw, gh), (0, 0));
                for f in &mut p {
                    f.layer = vtium_image::Layer::BelowText;
                }
                p
            }),
        ),
        (
            "a ten-cell picture, five cells by two",
            Box::new(|_| picture((cols, rows), (8, 16), (40, 32), (80, 20))),
        ),
    ];
    for (name, placements) in &cases {
        let (first, times, counts) = full(placements.as_ref(), &mut buffer);
        println!(
            "full redraw, {name}: first frame {:.3} ms; then {} (scaled {}, kept {})",
            first.as_secs_f64() * 1e3,
            summary(times),
            counts.scaled,
            counts.kept
        );
    }
    // the ten-cell picture moves a column each frame over static text, into
    // a buffer that holds the frame before: only its old and new cells draw
    let mut r = Renderer::new(unifont(), W, H, Config::default());
    let mut since = Age::NEVER;
    let mut times = Vec::new();
    let mut cells = 0;
    for i in 0..21u32 {
        let mut m = Mapped::new(&mut buffer, W, H, W as usize * 4).unwrap();
        let p = picture((cols, rows), (8, 16), (40, 32), (40 + i, 20));
        let d = r.draw(
            &s.frame(since).with_placements(p),
            &mut m,
            u32::from(i > 0),
            &st,
        );
        if i > 0 {
            times.push(d.stats.cpu);
            cells = d.stats.cells;
        }
        since = s.damage.checkpoint();
    }
    println!(
        "a ten-cell picture moving a column a frame: {} ({cells} cells drawn a frame)",
        summary(times)
    );
}
