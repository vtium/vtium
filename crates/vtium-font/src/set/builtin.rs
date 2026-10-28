//! The built-in sets (ADR-1200 D7): `unifont` and `unscii-8`, packed by
//! `build.rs` and compiled into the binary, trusted by construction and
//! never read from disk. Each is read once, the first time it is asked
//! for, by the same reader as a set from the store.

use std::sync::{Arc, OnceLock};

use super::read::Set;

/// A built-in set's slot: read once; `None` only if the build packed a set
/// its own reader refuses, which the tests hold against.
type Slot = OnceLock<Option<Arc<Set>>>;

#[allow(dead_code)]
fn load(slot: &'static Slot, bytes: &'static [u8]) -> Option<&'static Arc<Set>> {
    slot.get_or_init(|| Set::from_static(bytes).ok().map(Arc::new))
        .as_ref()
}

/// The built-in unifont set, with the `unifont` feature.
pub(crate) fn unifont() -> Option<&'static Arc<Set>> {
    #[cfg(feature = "unifont")]
    {
        static SLOT: Slot = OnceLock::new();
        load(
            &SLOT,
            include_bytes!(concat!(env!("OUT_DIR"), "/unifont.vtglass")),
        )
    }
    #[cfg(not(feature = "unifont"))]
    None
}

/// The built-in Unscii-8 set, with the `unscii` feature.
pub(crate) fn unscii_8() -> Option<&'static Arc<Set>> {
    #[cfg(feature = "unscii")]
    {
        static SLOT: Slot = OnceLock::new();
        load(
            &SLOT,
            include_bytes!(concat!(env!("OUT_DIR"), "/unscii-8.vtglass")),
        )
    }
    #[cfg(not(feature = "unscii"))]
    None
}

/// The built-in set named `name`, as the manifest and OSC 50 spell it:
/// `unifont` or `unscii-8`. `None` for any other name, or one this build
/// leaves out.
pub fn builtin(name: &str) -> Option<Arc<Set>> {
    match name {
        "unifont" => unifont().cloned(),
        "unscii-8" => unscii_8().cloned(),
        _ => None,
    }
}

/// The name of the built-in set ADR-1200 D7 draws for cells
/// `cell_height` pixels high in `locale` (`LANG`'s form, such as
/// `zh_CN.UTF-8`): `unscii-8` under 16 pixels, `unifont` from 16, and
/// `unifont` in a zh, ja or ko locale whatever the height, since Unscii-8
/// holds no CJK.
pub fn builtin_name(cell_height: u32, locale: &str) -> &'static str {
    let language = locale.split(['_', '.', '@']).next().unwrap_or_default();
    if cell_height < 16 && !matches!(language, "zh" | "ja" | "ko") {
        "unscii-8"
    } else {
        "unifont"
    }
}
