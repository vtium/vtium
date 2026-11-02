//! JPEGs that panicked the decoder the helper used to link, `zune-jpeg`
//! 0.4.21, each a fuzz target's crash input minimized with `cargo fuzz
//! tmin` (`tests/fixtures/jpeg/README.md` gives their origin). A hostile
//! JPEG must give a picture or a refusal, never a panic: the helper's
//! release build aborts on a panic, which kills the worker and counts a
//! crash against the console (ADR-1101 D8).

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]

use vtium_image_helper::decode::{self, Bounds};

/// The bounds the jpeg fuzz target decodes under.
const BOUNDS: Bounds = Bounds {
    largest: (512, 512),
    max_bytes: 1 << 20,
};

fn decodes_or_refuses(data: &[u8]) {
    if let Ok(p) = decode::jpeg(data, BOUNDS) {
        assert_eq!(
            p.rgba.len() as u64,
            u64::from(p.width) * u64::from(p.height) * 4
        );
    }
}

/// 0.4.21's scalar IDCT added two coefficients without wrapping, so a
/// progressive JPEG with large coefficients overflowed (`src/idct/scalar.rs`
/// line 128), a panic wherever overflow is checked, as in tests.
#[test]
fn a_progressive_jpeg_whose_coefficients_overflow_does_not_panic() {
    decodes_or_refuses(include_bytes!("fixtures/jpeg/idct-overflow.jpg"));
}

/// 0.4.21 asserted that a horizontally subsampled component's buffers had
/// sizes the file can contradict (`src/worker.rs` line 412), a panic in
/// every build, the helper's release build included.
#[test]
fn a_jpeg_whose_subsampling_contradicts_its_buffers_does_not_panic() {
    decodes_or_refuses(include_bytes!("fixtures/jpeg/upsample-assert.jpg"));
}
