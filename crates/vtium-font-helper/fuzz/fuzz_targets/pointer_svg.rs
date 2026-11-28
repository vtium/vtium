//! Any bytes as an SVG pointer file through the drawing the font helper
//! runs for the console's pointer (ADR-1309 D4). The first byte picks the
//! cell height, 1 to 128, and the second the shape whose symbol is drawn.
//! No panic, and each picture is at most 64 pixels a side, holds exactly
//! its pixels, has its hotspot inside it, and has no color above its alpha.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_font_helper::pointer_svg::draw;

const SHAPES: [&str; 10] = [
    "default",
    "text",
    "pointer",
    "crosshair",
    "wait",
    "progress",
    "not-allowed",
    "move",
    "ew-resize",
    "ns-resize",
];

fuzz_target!(|input: &[u8]| {
    let [height, shape, bytes @ ..] = input else {
        return;
    };
    let cell = u32::from(height % 128) + 1;
    let id = SHAPES[usize::from(*shape) % SHAPES.len()];
    let Ok(p) = draw(bytes, id, cell) else {
        return;
    };
    assert!((1..=64).contains(&p.width) && (1..=64).contains(&p.height));
    assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
    assert!(p.hot.0 < p.width && p.hot.1 < p.height);
    for px in p.rgba.chunks(4) {
        assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
    }
});
