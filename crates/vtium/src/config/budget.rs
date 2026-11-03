//! `sb-memory` on this machine (vtium/vtium#131): the key's fence holds it
//! from 1 MiB to 1 GiB, and [`sb_memory_cap`] lowers the top to a quarter
//! of the machine's memory, or to the default when the memory cannot be
//! read. Reading the key clamps it to that cap, so a value past either is
//! reported once; [`memory_warning`] reports the memory left unread.

use std::cell::Cell;

use super::{Config, Origin, keys};

/// Where the machine's memory is read.
const MEMINFO: &str = "/proc/meminfo";

/// The machine's memory in bytes, from [`MEMINFO`]'s `MemTotal`; `None`
/// when it cannot be read.
pub fn physical_memory() -> Option<u64> {
    MEMORY.with(|m| {
        if let Some(known) = m.get() {
            return known;
        }
        let read = std::fs::read_to_string(MEMINFO)
            .ok()
            .and_then(|text| mem_total(&text));
        m.set(Some(read));
        read
    })
}

thread_local! {
    /// The machine's memory once read: it does not change while vtium
    /// runs. A test sets it in place of the machine's.
    static MEMORY: Cell<Option<Option<u64>>> = const { Cell::new(None) };
}

/// `MemTotal` in bytes from `/proc/meminfo`'s text.
fn mem_total(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    kib.checked_mul(1024)
}

/// The most `sb-memory` may be on a machine with `ram` bytes: a quarter
/// of it inside the key's fence, or the default when `ram` is unknown.
fn cap_for(ram: Option<u64>) -> u64 {
    let cap = ram.map_or(vtium_term::SB_MEMORY_DEFAULT, vtium_term::sb_memory_cap);
    u64::try_from(cap).unwrap_or(u64::MAX)
}

/// The most `sb-memory` may be on this machine.
pub fn sb_memory_cap() -> u64 {
    cap_for(physical_memory())
}

/// Whether the configuration sets `sb-memory` itself, in a file or on the
/// command line. Left to its default, the budget follows each console's
/// grid (ADR-1001, "Memory").
pub fn sb_memory_set(config: &Config) -> bool {
    !matches!(
        config.origin(&keys::SB_MEMORY),
        None | Some(Origin::Default)
    )
}

/// The scrollback's byte budget `config` gives on this machine: `sb-memory`
/// as set, inside its cap; or, left to its default, the cap, the most the
/// budget that follows each console's grid may be.
pub fn sb_memory(config: &Config) -> usize {
    let asked = config
        .number(&keys::SB_MEMORY)
        .unwrap_or(vtium_term::SB_MEMORY_DEFAULT as u64);
    let most = if sb_memory_set(config) {
        asked.min(sb_memory_cap())
    } else {
        sb_memory_cap()
    };
    usize::try_from(most).unwrap_or(usize::MAX)
}

/// The warning when the machine's memory cannot be read, which holds
/// `sb-memory` to its default.
pub fn memory_warning() -> Option<String> {
    physical_memory().is_none().then(|| {
        format!(
            "cannot read the machine's memory from {MEMINFO}; sb-memory is at most {}",
            vtium_term::SB_MEMORY_DEFAULT
        )
    })
}

/// Sets the machine's memory a test sees, `None` for unread.
#[cfg(test)]
pub(super) fn set_test_memory(ram: Option<u64>) {
    MEMORY.with(|m| m.set(Some(ram)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memtotal_is_read_in_kib() {
        let text = "MemTotal:        524288 kB\nMemFree:  1 kB\n";
        assert_eq!(mem_total(text), Some(512 << 20));
        assert_eq!(mem_total("MemFree: 1 kB\n"), None);
        assert_eq!(mem_total("MemTotal: lots kB\n"), None);
    }

    // A quarter of the machine's memory, inside the fence; the default when
    // the memory cannot be read.
    #[test]
    fn the_cap_is_a_quarter_of_the_memory_or_the_default() {
        assert_eq!(cap_for(Some(512 << 20)), 128 << 20);
        assert_eq!(cap_for(Some(64 << 30)), 1 << 30);
        assert_eq!(cap_for(Some(0)), 1 << 20);
        assert_eq!(cap_for(None), vtium_term::SB_MEMORY_DEFAULT as u64);
    }

    #[test]
    fn unread_memory_holds_the_default_with_a_warning_naming_meminfo() {
        set_test_memory(None);
        assert_eq!(sb_memory(&Config::default()), vtium_term::SB_MEMORY_DEFAULT);
        // a build with images prices placeholder cells into a screen row,
        // so its default is lower (ADR-1001, "Placeholder cells")
        #[cfg(feature = "images")]
        let most = "4653056";
        #[cfg(not(feature = "images"))]
        let most = "5242880";
        assert_eq!(
            memory_warning(),
            Some(format!(
                "cannot read the machine's memory from /proc/meminfo; sb-memory is at most {most}"
            ))
        );
        set_test_memory(Some(512 << 20));
        assert_eq!(memory_warning(), None);
    }
}
