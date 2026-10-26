//! The platform layer.
//!
//! Everything that touches the kernel and the session: DRM/KMS and fbdev
//! displays, the virtual terminal (VT_PROCESS switching, keyboard mode),
//! seats and sessions (logind, libseat), keyboard input through
//! xkbcommon, the pty and the child process, and the event loop that
//! serves them without ever blocking.
//!
//! The only crate that needs `unsafe` (ioctls and FFI); each use is kept
//! small and documented where it happens.
