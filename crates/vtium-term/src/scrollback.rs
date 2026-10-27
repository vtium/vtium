//! How many lines the scrollback keeps: `sb-size`, capped by the machine's
//! memory.

use crate::bounds::{SB_SIZE_CAP_LARGE, SB_SIZE_CAP_MEDIUM, SB_SIZE_CAP_SMALL};

/// `sb-size` when the configuration does not set it, as the C reference's
/// `kmscon.conf(5)` gives it.
pub const SB_SIZE_DEFAULT: usize = 1000;

const GIB: u64 = 1 << 30;

/// The most scrollback lines a machine with `ram` bytes of memory
/// accepts: 10,000 below 2 GiB, 30,000 from 2 GiB, 100,000 from 4 GiB.
pub fn sb_size_cap(ram: u64) -> usize {
    if ram >= 4 * GIB {
        SB_SIZE_CAP_LARGE
    } else if ram >= 2 * GIB {
        SB_SIZE_CAP_MEDIUM
    } else {
        SB_SIZE_CAP_SMALL
    }
}

/// The scrollback lines to keep for a requested `sb-size` on a machine
/// with `ram` bytes: the request, clamped to the cap.
pub fn sb_size(requested: usize, ram: u64) -> usize {
    requested.min(sb_size_cap(ram))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_steps_at_2_and_4_gib() {
        assert_eq!(sb_size_cap(512 << 20), 10_000);
        assert_eq!(sb_size_cap(2 * GIB - 1), 10_000);
        assert_eq!(sb_size_cap(2 * GIB), 30_000);
        assert_eq!(sb_size_cap(4 * GIB - 1), 30_000);
        assert_eq!(sb_size_cap(4 * GIB), 100_000);
        assert_eq!(sb_size_cap(u64::MAX), 100_000);
    }

    #[test]
    fn a_request_past_the_cap_is_clamped() {
        assert_eq!(sb_size(SB_SIZE_DEFAULT, 512 << 20), 1000);
        assert_eq!(sb_size(50_000, 512 << 20), 10_000);
        assert_eq!(sb_size(50_000, 3 * GIB), 30_000);
        assert_eq!(sb_size(usize::MAX, 8 * GIB), 100_000);
        assert_eq!(sb_size(0, 8 * GIB), 0);
    }
}
