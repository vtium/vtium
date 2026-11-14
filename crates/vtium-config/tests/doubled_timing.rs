//! How long the font helper's engine takes to draw a screen of text at
//! each double size, beside the normal size: every printable ASCII
//! character of DejaVu Sans Mono at its 9x16 cell, the most characters a
//! screen of double-size lines in one style asks for at once. It is a
//! measurement, run by hand in a release build:
//!
//! ```text
//! cargo test --release -p vtium-config --test doubled_timing -- --ignored --nocapture
//! ```
#![expect(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "a measurement that fails by panicking and prints what it measured"
)]

use std::time::{Duration, Instant};

use vtium_config::serve::{Engine, FritText, Paths};
use vtium_font::spacing::SpacedCell;
use vtium_font::{Cell, Doubled, Variant};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/DejaVuSansMono-ascii.ttf"
);

/// The fastest of `runs` runs of `f`, which leaves out the runs another
/// process on the machine slowed.
#[expect(
    clippy::disallowed_methods,
    reason = "the measurement times the drawing"
)]
fn fastest(runs: usize, mut f: impl FnMut()) -> Duration {
    (0..runs)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
#[ignore = "a measurement, run by hand in a release build"]
fn measure_a_screen_of_double_size_text() {
    let frit = FritText {
        name: "timing".to_owned(),
        text: format!("source=outline:{FONT}\n"),
    };
    let engine = Engine::load(&frit, &Paths::default(), None).unwrap();
    let at = SpacedCell::plain(Cell {
        width: 9,
        height: 16,
    });
    let want: Vec<u32> = (0x21..=0x7e).collect();
    let normal = fastest(5, || {
        engine
            .draw_variant(at, Variant::default(), &want, (&[], false))
            .unwrap()
            .unwrap();
    });
    println!("normal size: {} glyphs in {normal:?}", want.len());
    for doubled in [Doubled::Width, Doubled::Height] {
        let took = fastest(5, || {
            engine
                .draw_doubled(at, (Variant::default(), doubled), &want)
                .unwrap()
                .unwrap();
        });
        println!("{doubled:?}: {} glyphs in {took:?}", want.len());
    }
}
