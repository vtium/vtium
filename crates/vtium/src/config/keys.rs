//! The key table, which lives in vtium-settings so that vtium-config shows
//! and edits vtium.conf by the table the console reads it by (ADR-1201
//! D1). Its defaults that name what vtium is built with are vtium-settings'
//! own constants, each held equal here, when vtium is compiled, to the
//! constant vtium uses.

pub use vtium_settings::keys::*;

/// Whether `a` and `b` are the same bytes, in a constant.
const fn same(a: &[u8], b: &[u8]) -> bool {
    match (a, b) {
        ([], []) => true,
        ([x, a @ ..], [y, b @ ..]) => *x == *y && same(a, b),
        _ => false,
    }
}

const _: () = assert!(
    same(FRITS.as_bytes(), crate::font::FRITS.as_bytes()),
    "font-frits' default"
);
const _: () = assert!(
    same(HELPER_PROGRAM.as_bytes(), crate::helper::PROGRAM.as_bytes()),
    "font-helper's default"
);
const _: () = assert!(
    same(
        IMAGE_PROGRAM.as_bytes(),
        crate::head::images::PROGRAM.as_bytes()
    ),
    "image-helper's default"
);
const _: () = assert!(
    same(TERM_NAME.as_bytes(), crate::termcap::NAME.as_bytes()),
    "term's default"
);
