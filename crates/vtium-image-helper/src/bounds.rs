//! The helper's bounds and named constants, each with its source.

/// How long the helper waits for a request before it exits, the idle
/// interval ADR-1301 D2 sets for vtium's helpers, the font helper's
/// `HELPER_IDLE_MS`.
pub const HELPER_IDLE_MS: u64 = 60_000;

/// How many requests in a row from one console, of one medium, flags and
/// picture size, make a stream (ADR-1101 D4). The helper counts requests,
/// never time.
pub const STREAM_AFTER: u32 = 3;

/// How long a stream lasts with no request before it ends (ADR-1101 D4),
/// on the helper's idle timer.
pub const STREAM_IDLE_MS: u64 = 1_000;

/// How many pooled reply files a stream keeps (ADR-1101 D4's second way).
pub const REPLY_POOL_FILES: usize = 2;

/// The longest shared-memory name, without its leading `/`: the kernel's
/// longest file name in `/dev/shm`, `NAME_MAX`.
pub const SHM_NAME_MAX: usize = 255;

/// What a temporary file's resolved path must contain (kitty's spec).
pub const TEMP_MARK: &str = "tty-graphics-protocol";

/// The directory shared-memory objects live in.
pub const SHM_DIR: &str = "/dev/shm";

/// The most bytes the PNG decoder may allocate beside the picture it
/// writes, for its rows and chunks: the request's `max_bytes`, but at
/// most this, the png crate's own default. It keeps a hostile PNG from
/// making the decoder allocate more than the request allows.
pub const PNG_WORK_MAX: u64 = 64 << 20;

/// The descriptor the helper's socket to vtium is on (ADR-1101 D3).
pub const SOCKET_FD: i32 = 3;
