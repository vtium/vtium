//! Tier 2: what the renderer draws in memory, compared pixel for pixel
//! with goldens.
//!
//! [`screens`] holds the screens and where each one's golden lives,
//! [`draw`] draws them through `vtium-term` and `vtium-render`, [`ppm`],
//! [`tol`] and [`compare`] read and judge the images as tier 3 does, and
//! [`runner`] runs them all and writes the [`report`].

pub mod compare;
pub mod draw;
pub mod ppm;
pub mod report;
pub mod runner;
pub mod screens;
pub mod tol;
