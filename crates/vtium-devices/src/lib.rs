#![forbid(unsafe_code)]
// A listed device's facts come from udev, sysfs and the device, which the
// machine's owner and the device's maker control: nothing in them can
// panic the console or `vtium check-config`.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! The devices present as the head file's rules see them (ADR-1401 D4,
//! ADR-1403 D9).
//!
//! vtium-platform reads a display's identity and an input's identity from
//! udev, sysfs, the device and the KMS connector. This crate maps each into
//! the [`Identity`](vtium_settings::Identity) that vtium-settings's rules
//! match: [`display`] and [`unnamed_display`] for a display, with its
//! [`Card`], and [`input`] for an input device. [`devices`] maps a whole
//! listing of a seat's devices the same way.
//!
//! The console's start places each device it opens through these
//! functions, and `vtium check-config` places the devices it lists through
//! [`devices`], so a rule matches the same device in both. vtium-config
//! links this crate too, so that its flow view places each device by the
//! same functions.

mod identity;
mod listing;

pub use identity::{Card, display, input, unnamed_display, why_unnamed};
pub use listing::{SEAT, UDEV_CONTROL, devices, udev_daemon_runs};
