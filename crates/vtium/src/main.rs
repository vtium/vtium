//! vtium: a terminal for the Linux virtual console.
//!
//! Wires the platform (display, VT, seat, input, pty) to the terminal
//! state and the renderer. Nothing runs yet: the first working build is
//! the minspec vertical slice.

fn main() {
    eprintln!("vtium {}: not yet implemented", env!("CARGO_PKG_VERSION"));
    std::process::exit(1);
}
