// `unsafe` lives in one module, `fork`, which allows it site by site
// (ADR-1101 D1); every other module forbids it.
#![deny(unsafe_code)]
// A program's bytes reach every decoder here, and nothing they hold can
// panic the helper (docs/architecture.md, "Determinism").
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-image, the image helper (ADR-1101): the program vtium starts as
//! the session's user to read the files and shared-memory objects a
//! program names, and to decode PNG, zlib and the formats iTerm2's
//! pictures arrive in, for the console, which runs as root and does
//! neither (ADR-1100 D7).
//!
//! - [`serve`]: the helper's loop, one request at a time on descriptor 3,
//!   each carried out by a worker it forks.
//! - [`job`]: one request as a worker carries it out, split at the lock
//!   into [`job::open`] and [`job::finish`].
//! - [`files`]: the file rules, which files a worker may open and how a
//!   checked file is removed.
//! - [`inflate`] and [`decode`]: zlib, PNG, JPEG and GIF, each bounded by
//!   the request before it allocates.
//! - [`sandbox`]: the seccomp filters and the landlock domain.
//! - [`channel`]: packets with descriptors, and the worker's outcome.
//! - [`fork`]: the one module that holds `unsafe`.
//! - [`bounds`]: the helper's named constants.
//!
//! The wire to vtium and the conversion into the store's format live in
//! vtium-image, which both ends link.

pub mod bounds;
pub mod channel;
pub mod decode;
pub mod files;
pub mod fork;
pub mod inflate;
pub mod job;
pub mod sandbox;
pub mod serve;
