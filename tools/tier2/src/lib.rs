//! Tier 2: what the renderer draws in memory, compared pixel for pixel
//! with goldens.
//!
//! [`ppm`], [`tol`] and [`compare`] read and judge the images as tier 3
//! does, and [`report`] writes what a run found.

pub mod compare;
pub mod ppm;
pub mod report;
pub mod tol;
