//! The renderer's bounds.

/// Bytes each glyph pool may hold in the performance profile, gltex's
/// per-pool budget in the C reference.
pub const POOL_BYTES_PERFORMANCE: u64 = 64 * 1024 * 1024;

/// Bytes the row scratch may hold; a taller row is composed in bands.
pub const ROW_SCRATCH_MAX: usize = 1024 * 1024;
