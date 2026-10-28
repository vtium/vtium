//! The demonstration set (vtium/vtium#142): vtium's test card, drawn by
//! the renderer in memory for each display profile.
//!
//! [`card`] writes the card's byte stream, [`profiles`] holds the display
//! profiles and how each arrays the card, [`draw`] draws a profile's
//! screens and its collage, [`png`] reads and writes them, [`docs`] lays
//! out `docs/specimens/`, and [`check`] writes it or judges it.

pub mod card;
pub mod check;
pub mod docs;
pub mod draw;
pub mod png;
pub mod profiles;
