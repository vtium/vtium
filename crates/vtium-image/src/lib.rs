#![forbid(unsafe_code)]
//! Inline images.
//!
//! Decodes sixel (DCS) and the kitty graphics protocol (APC), and keeps
//! the image store: images, their placements on lines, and the memory
//! budget that bounds them. Everything decoded here comes from the PTY,
//! so every size and count is bounded before allocation.
//!
//! Not part of the minspec build.
