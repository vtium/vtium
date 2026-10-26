#![forbid(unsafe_code)]
//! The escape-sequence parser.
//!
//! Turns the byte stream a program writes into actions: printable
//! characters, C0/C1 controls, and complete CSI, ESC, OSC, DCS and APC
//! sequences with their parameters, sub-parameters and intermediates.
//!
//! The state machine follows the DEC/ANSI parser described at
//! vt100.net, with every string bounded: the PTY byte stream crosses a
//! privilege boundary uninvited, so no sequence can grow memory without
//! limit. APC strings are delivered (the kitty graphics protocol rides on
//! them), and a sequence with more intermediates than it may carry is
//! flagged, never run as another command.
//!
//! No I/O and no allocation beyond fixed bounds, so it can be fuzzed
//! directly.
