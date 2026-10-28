//! `sb-memory` on this machine: the key's fence holds it from 1 MiB to
//! 1 GiB, and a quarter of the machine's memory caps it at start
//! (vtium/vtium#131). The start and `vtium check-config` report the cap
//! through [`sb_memory_warning`], and the seat takes [`sb_memory`].

use super::{Config, keys};

/// The scrollback's byte budget for `config` on this machine.
pub fn sb_memory(config: &Config) -> usize {
    sb_memory_on(config, physical_memory()).0
}

/// The warning when a quarter of this machine's memory lowers
/// `sb-memory`, naming the key, the value asked and the value used.
pub fn sb_memory_warning(config: &Config) -> Option<String> {
    sb_memory_on(config, physical_memory()).1
}

/// The budget `config` asks for on a machine with `ram` bytes, and the
/// warning when a quarter of `ram` lowers it.
fn sb_memory_on(config: &Config, ram: u64) -> (usize, Option<String>) {
    let asked = config
        .number(&keys::SB_MEMORY)
        .map_or(vtium_term::SB_MEMORY_DEFAULT, |n| {
            usize::try_from(n).unwrap_or(usize::MAX)
        });
    let used = vtium_term::sb_memory(asked, ram);
    let warning = (used != asked).then(|| {
        format!("sb-memory={asked} is more than a quarter of the machine's memory, {used} used")
    });
    (used, warning)
}

/// The machine's memory in bytes, from `/proc/meminfo`; 0 when unread,
/// which takes the least budget.
fn physical_memory() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
            let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            kib.checked_mul(1024)
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::*;

    fn config(conf: &str) -> Config {
        let args = [OsString::from("vtium")];
        let conf = conf.as_bytes().to_vec();
        super::super::load(&args, Some("seat0"), &mut |p: &Path| {
            Ok((p == Path::new("/etc/kmscon/kmscon.conf")).then(|| conf.clone()))
        })
        .config
    }

    // A quarter of the machine's memory caps `sb-memory`, with one warning
    // naming the key, the value asked and the value used.
    #[test]
    fn a_quarter_of_the_memory_caps_sb_memory_with_a_warning() {
        let ram = 512 << 20;
        let default = vtium_term::SB_MEMORY_DEFAULT;
        assert_eq!(sb_memory_on(&config(""), ram), (default, None));
        assert_eq!(
            sb_memory_on(&config("sb-memory=128M\n"), ram),
            (128 << 20, None)
        );
        assert_eq!(
            sb_memory_on(&config("sb-memory=1G\n"), ram),
            (
                128 << 20,
                Some(
                    "sb-memory=1073741824 is more than a quarter of the machine's memory, 134217728 used"
                        .to_owned()
                )
            )
        );
        assert_eq!(
            sb_memory_on(&config("sb-memory=1G\n"), 8 << 30),
            (1 << 30, None)
        );
    }
}
