//! What blending a partly transparent picture costs a frame on the minspec
//! display, 1366x768 in unifont's 8x16 cells: a full redraw under one
//! full-screen picture at its own size, every pixel of it partly
//! transparent, above the text and below it; and, for scale, the same
//! picture opaque. They are machine figures and are printed, not pinned,
//! and the test is left out of the gates:
//!
//! `cargo test --release -p vtium-render --features images --test picture_blend_figures -- --ignored --nocapture`
#![cfg(feature = "images")]

mod common;

use std::time::Duration;

use common::{Screen, rgb, style, unifont};
use vtium_image::{
    FramePlacement, Layer, LineKey, NewImage, Place, Protocol, Rows, Screens, Store,
};
use vtium_render::{Config, DrawState, Mapped, Renderer};
use vtium_term::Age;

const W: u32 = 1366;
const H: u32 = 768;
const FRAMES: usize = 40;

/// The alpha of each pixel of a picture, by its column and row.
type Alpha = fn(u32, u32) -> u32;

fn text(s: &mut Screen) {
    let words = "the quick brown fox jumps over a lazy dog; fn main() { println!(\"hi\"); } ";
    let st = style(rgb("c0c0c0"), rgb("101010"));
    for row in 0..s.rows {
        let line: String = words.chars().cycle().skip(row * 7).take(s.cols).collect();
        s.text(0, row, &line, st);
    }
}

/// One full-screen kitty picture `w` by `h` in `layer`, each pixel
/// premultiplied at the alpha `alpha(x, y)`.
fn picture(
    (cols, rows): (usize, usize),
    (w, h): (u32, u32),
    layer: Layer,
    alpha: Alpha,
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
    store.set_cell(8, 16).unwrap();
    let pixels = (0..h)
        .flat_map(|y| {
            (0..w).map(move |x| {
                let a = alpha(x, y);
                let c = |v: u32| (v * a + 127) / 255;
                a << 24 | c(x & 0xff) << 16 | c(y & 0xff) << 8 | c(0xc0)
            })
        })
        .collect();
    let new = NewImage {
        protocol: Protocol::KITTY,
        width: w,
        height: h,
        pixels,
        id: 1,
        number: 0,
        transient: false,
        opaque: None,
    };
    let image = store.add_image(new, None, screens).unwrap();
    store.place(image, Place::default(), screens, cols).unwrap();
    let mut p = store.frame(view, cols);
    for f in &mut p {
        f.layer = layer;
    }
    p
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

/// Every alpha from 1 to 254 in turn across the picture.
fn partial(x: u32, y: u32) -> u32 {
    1 + (x + y) % 254
}

fn opaque(_: u32, _: u32) -> u32 {
    255
}

#[test]
#[ignore = "machine figures, run by hand: see the file's documentation"]
fn picture_blend_figures_at_1366x768() {
    let mut buffer = vec![0u8; W as usize * 4 * H as usize];
    let probe = Renderer::new(unifont(), W, H, Config::default());
    let (cols, rows) = probe.grid_size();
    let mut s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("101010"));
    text(&mut s);
    let st = DrawState {
        background: s.palette.bg(),
        ..DrawState::default()
    };
    let (gw, gh) = (cols as u32 * 8, rows as u32 * 16);
    let cases: [(&str, Layer, Alpha); 4] = [
        (
            "partly transparent, above the text",
            Layer::AboveText,
            partial,
        ),
        (
            "partly transparent, below the text",
            Layer::BelowText,
            partial,
        ),
        ("opaque, above the text", Layer::AboveText, opaque),
        ("no picture", Layer::AboveText, opaque),
    ];
    for (name, layer, alpha) in cases {
        let placements = if name == "no picture" {
            Vec::new()
        } else {
            picture((cols, rows), (gw, gh), layer, alpha)
        };
        let mut r = Renderer::new(unifont(), W, H, Config::default());
        let mut times = Vec::new();
        for i in 0..=FRAMES {
            let mut m = Mapped::new(&mut buffer, W, H, W as usize * 4).unwrap();
            let frame = s.frame(Age::NEVER).with_placements(placements.clone());
            let d = r.draw(&frame, &mut m, 0, &st);
            if i > 0 {
                times.push(d.stats.cpu);
            }
        }
        println!("full redraw, {name}: {}", summary(times));
    }
}
