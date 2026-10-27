//! How many lines the scrollback keeps: `sb-size`, capped by default by the
//! machine's memory, or by a cap the configuration sets (its key is wired
//! with the configuration, vtium/vtium#21).

use crate::bounds::{SB_SIZE_CAP_LARGE, SB_SIZE_CAP_MEDIUM, SB_SIZE_CAP_SMALL, SB_SIZE_MAX};

/// `sb-size` when the configuration does not set it, as the C reference's
/// `kmscon.conf(5)` gives it.
pub const SB_SIZE_DEFAULT: usize = 1000;

const GIB: u64 = 1 << 30;

/// The most scrollback lines `sb-size` may ask for. By default it follows
/// the machine's `ram`: 10,000 lines below 2 GiB, 30,000 from 2 GiB,
/// 100,000 from 4 GiB. A configured `cap` replaces that default, higher or
/// lower, clamped to [`SB_SIZE_MAX`].
pub fn sb_size_cap(ram: u64, cap: Option<usize>) -> usize {
    match cap {
        Some(cap) => cap.min(SB_SIZE_MAX),
        None if ram >= 4 * GIB => SB_SIZE_CAP_LARGE,
        None if ram >= 2 * GIB => SB_SIZE_CAP_MEDIUM,
        None => SB_SIZE_CAP_SMALL,
    }
}

/// The scrollback lines to keep for a requested `sb-size` on a machine
/// with `ram` bytes and an optional configured `cap`: the request, clamped
/// to the cap.
pub fn sb_size(requested: usize, ram: u64, cap: Option<usize>) -> usize {
    requested.min(sb_size_cap(ram, cap))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_cap_steps_at_2_and_4_gib() {
        assert_eq!(sb_size_cap(512 << 20, None), 10_000);
        assert_eq!(sb_size_cap(2 * GIB - 1, None), 10_000);
        assert_eq!(sb_size_cap(2 * GIB, None), 30_000);
        assert_eq!(sb_size_cap(4 * GIB - 1, None), 30_000);
        assert_eq!(sb_size_cap(4 * GIB, None), 100_000);
        assert_eq!(sb_size_cap(u64::MAX, None), 100_000);
    }

    #[test]
    fn a_request_past_the_cap_is_clamped() {
        assert_eq!(sb_size(SB_SIZE_DEFAULT, 512 << 20, None), 1000);
        assert_eq!(sb_size(50_000, 512 << 20, None), 10_000);
        assert_eq!(sb_size(50_000, 3 * GIB, None), 30_000);
        assert_eq!(sb_size(usize::MAX, 8 * GIB, None), 100_000);
        assert_eq!(sb_size(0, 8 * GIB, None), 0);
    }

    // The operator, 2026-10-26: "10000 lines is fine but it should be
    // configurable". A configured cap wins over the memory's default in
    // both directions, and never passes SB_SIZE_MAX.
    #[test]
    fn a_configured_cap_wins_both_ways_inside_the_hard_bound() {
        assert_eq!(sb_size_cap(512 << 20, Some(50_000)), 50_000);
        assert_eq!(sb_size(50_000, 512 << 20, Some(50_000)), 50_000);
        assert_eq!(sb_size_cap(8 * GIB, Some(2_000)), 2_000);
        assert_eq!(sb_size(50_000, 8 * GIB, Some(2_000)), 2_000);
        assert_eq!(sb_size_cap(512 << 20, Some(0)), 0);
        assert_eq!(sb_size_cap(512 << 20, Some(usize::MAX)), SB_SIZE_MAX);
        assert_eq!(sb_size(usize::MAX, 8 * GIB, Some(usize::MAX)), SB_SIZE_MAX);
    }
}
