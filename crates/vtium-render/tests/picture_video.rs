//! What a picture replaced every frame costs, as a video player's output
//! is: each frame a new full-screen opaque picture over the last, which
//! goes with it, timing `Renderer::draw` alone. One case is a sixel at its
//! own size, the other a kitty picture half the grid's size fitted to the
//! whole grid, so it is scaled twice over. Each runs at 1366x768,
//! 1920x1080 and 7680x2160, unturned and turned a quarter right, in
//! unifont's 8x16 cells. They are machine figures, printed and not pinned,
//! and the test is left out of the gates:
//!
//! `cargo test --release -p vtium-render --features images --test picture_video -- --ignored --nocapture`
#![cfg(feature = "images")]

mod common;

use std::time::Duration;

use common::{Screen, rgb, unifont};
use vtium_image::{LineKey, NewImage, Place, Protocol, Rows, Screens, Store};
use vtium_render::{Canvas, Config, DrawState, Orientation, Renderer};
use vtium_term::Age;

const FRAMES: usize = 7;

fn median(mut t: Vec<Duration>) -> f64 {
    t.sort();
    t[t.len() / 2].as_secs_f64() * 1e3
}

/// The median time of `Renderer::draw` over `FRAMES` frames on a
/// `display` turned as `o` says, the first frame left out. Each frame
/// places a new picture over the last: a sixel the grid's size, or with
/// `kitty`, a kitty picture of half the grid's size fitted to the whole
/// grid, with the same id. Also the times a picture was scaled.
fn video(display: (u32, u32), o: Orientation, kitty: bool) -> (f64, u64) {
    let config = Config {
        orientation: o,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), display.0, display.1, config);
    let (cols, rows) = r.grid_size();
    let s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("000000"));
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
    store.set_limit(1 << 31);
    let (gw, gh) = (cols as u32 * 8, rows as u32 * 16);
    let (w, h) = if kitty { (gw / 2, gh / 2) } else { (gw, gh) };
    let mut c = Canvas::new(display.0, display.1, 0);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let mut times = Vec::new();
    for frame in 0..FRAMES as u32 {
        let pixels = (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| 0xff00_0000 | ((x + frame) & 0xff) << 16 | (y & 0xff) << 8)
            })
            .collect();
        let new = NewImage {
            protocol: if kitty {
                Protocol::KITTY
            } else {
                Protocol::SIXEL
            },
            width: w,
            height: h,
            pixels,
            id: u32::from(kitty),
            number: 0,
            transient: false,
        };
        let image = store.add_image(new, None, screens).unwrap();
        let (bc, br) = if kitty {
            (cols as u32, rows as u32)
        } else {
            (0, 0)
        };
        let place = Place {
            cols: bc,
            rows: br,
            ..Place::default()
        };
        store.place(image, place, screens, cols).unwrap();
        let frame_p = s.frame(Age::NEVER).with_placements(store.frame(view, cols));
        let d = r.draw(&frame_p, &mut c, 1, &st);
        if frame > 0 {
            times.push(d.stats.cpu);
        }
    }
    (median(times), r.picture_counts().scaled)
}

#[test]
#[ignore = "machine figures, run by hand: see the file's documentation"]
fn a_picture_replaced_every_frame() {
    for display in [(1366, 768), (1920, 1080), (7680, 2160)] {
        for o in [Orientation::Normal, Orientation::Right] {
            for kitty in [false, true] {
                let (ms, scaled) = video(display, o, kitty);
                let kind = if kitty {
                    "kitty fitted 2x"
                } else {
                    "sixel own size"
                };
                println!(
                    "{}x{} {:?} {kind}: median {ms:.3} ms a frame (scaled {scaled})",
                    display.0, display.1, o
                );
            }
        }
    }
}
