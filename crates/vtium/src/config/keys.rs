//! The key table, which lives in vtium-settings so that vtium-config shows
//! and edits vtium.conf by the table the console reads it by (ADR-1201
//! D1). Its defaults that name what vtium is built with are vtium-settings'
//! own constants, which vtium's `font::FRITS`, `helper::PROGRAM`,
//! `head::images::PROGRAM` and `termcap::NAME` take.

pub use vtium_settings::keys::*;
