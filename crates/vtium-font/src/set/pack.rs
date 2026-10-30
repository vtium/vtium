//! The set writer's deflate, with the `deflate` feature: vtium-vitrify
//! enables it, and `build.rs` includes this file to deflate the built-in
//! sets. The console builds without it, so it carries an inflate and no
//! deflate (ADR-1200 D19).

/// The deflate level the writer uses: fixed, so a set's bytes depend only
/// on its inputs and the tool's version.
const DEFLATE_LEVEL: u8 = 9;

/// `raw` as a raw deflate stream (RFC 1951).
pub(crate) fn deflate(raw: &[u8]) -> Option<Vec<u8>> {
    Some(miniz_oxide::deflate::compress_to_vec(raw, DEFLATE_LEVEL))
}
