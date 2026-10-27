//! The parser's bounds: one source for the constants the code uses and for
//! the table in this crate's README.
//!
//! Each bound is a [`Bound`] row holding its value, what happens past it,
//! and the C reference lines that set it; the public constant takes its
//! value from the row. The README table is generated from [`BOUNDS`], and
//! `readme_table_matches_bounds` fails when the committed copy differs.
//! `VTIUM_PARSE_BLESS=1 cargo test -p vtium-parse readme_table` rewrites it.

/// One bound and its account, a row of the README table.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Bound {
    name: &'static str,
    value: usize,
    what: &'static str,
    past: &'static str,
    cite: &'static str,
}

const PARAMS: Bound = Bound {
    name: "PARAMS_MAX",
    value: 16,
    what: "Parameters a CSI sequence or DCS string keeps",
    past: "Dropped, and the sequence still dispatches with 16",
    cite: "`CSI_ARG_MAX`, tsm-vte-int.h:25; tsm-vte.c:1283, 1293-1294, 2542-2543",
};

const SUBPARAMS: Bound = Bound {
    name: "SUBPARAMS_MAX",
    value: 8,
    what: "Colon sub-parameters one parameter keeps",
    past: "The colon and every digit up to the next `;` are dropped, and the sequence still dispatches",
    cite: "`CSI_SUBARG_MAX`, tsm-vte-int.h:28; tsm-vte.c:1298-1308",
};

const VALUE: Bound = Bound {
    name: "PARAM_VALUE_CAP",
    value: 0xffff,
    what: "A parameter or sub-parameter takes digits while it is at most this",
    past: "Further digits are dropped, so a value reaches at most 655359",
    cite: "tsm-vte.c:1314-1316",
};

const OSC: Bound = Bound {
    name: "OSC_MAX",
    value: 1 << 20,
    what: "Bytes an OSC string keeps, as UTF-8, in a buffer that doubles from 4 KiB to at most this size and is released after a string past 64 KiB: an OSC 52 payload at vtium-term's `CLIPBOARD_MAX` with room to spare, and 500 times an OSC 8 URI at the 2083 bytes VTE and iTerm2 allow",
    past: "The character that would pass it and everything after it are dropped as they arrive, so the kept bytes are a prefix, and the string still dispatches, marked truncated; vtium-term acts on no truncated string, so it is dropped whole",
    cite: "vtium's own, at tmux's `input-buffer-size` (`tmux.h:3509` at `df302c5`), which drops a longer sequence; xterm ignores a string past `maxStringParse`, 20000 bytes, or 600000 with sixel or ReGIS (`main.h:277-283` at `5c75ead`); Ghostty keeps 2048 bytes of an OSC string and 8 MiB of an OSC 52 (`src/terminal/osc.zig:361, 365` at `a732580`) and drops a longer one; foot grows its buffer with no bound short of half the address space (`osc.c:1708-1723` at `152487e`). The C reference kept 127 (`OSC_MAX_LEN`, tsm-vte-int.h:31; tsm-vte.c:2912-2921)",
};

const XTGETTCAP: Bound = Bound {
    name: "XTGETTCAP_MAX",
    value: 16 * (2 * 32 + 1) - 1,
    what: "Bytes an XTGETTCAP payload keeps: 16 hex-encoded names of 32 bytes and the `;` between each two",
    past: "The rest is dropped and the request reaches `xtgettcap` with no payload, which the C reference answers `DCS 0 + r ST`",
    cite: "`XTGETTCAP_MAX_LEN`, tsm-vte-int.h:33-38; libtsm.h:1538; tsm-vte.c:3087-3091, 3149-3152",
};

const INTERMEDIATES: Bound = Bound {
    name: "INTERMEDIATES_MAX",
    value: 2,
    what: "Intermediate bytes an ESC, CSI or DCS sequence keeps, in order",
    past: "The sequence dispatches nothing (vt100.net); the C reference kept intermediates as a set of flags with no count bound",
    cite: "vt100.net, collect: \"If more than two intermediate characters arrive, the parser can just flag this so that the dispatch can be turned into a null operation\"",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 6] = [PARAMS, SUBPARAMS, VALUE, OSC, XTGETTCAP, INTERMEDIATES];

/// Parameters a CSI sequence or DCS string keeps; those past it are
/// dropped and the sequence still dispatches.
pub const PARAMS_MAX: usize = PARAMS.value;

/// Colon sub-parameters one parameter keeps; past it the colon and every
/// digit up to the next `;` are dropped.
pub const SUBPARAMS_MAX: usize = SUBPARAMS.value;

/// A parameter or sub-parameter takes digits while it is at most this, so
/// a value reaches at most `PARAM_VALUE_CAP * 10 + 9`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a constant, 0xffff, which fits u32"
)]
pub const PARAM_VALUE_CAP: u32 = VALUE.value as u32;

/// Bytes an OSC string keeps, as UTF-8, 1 MiB. The character that would
/// pass it and everything after it are dropped.
pub const OSC_MAX: usize = OSC.value;

/// Bytes an XTGETTCAP payload keeps. Past it the request is flagged.
pub const XTGETTCAP_MAX: usize = XTGETTCAP.value;

/// Intermediate bytes an ESC, CSI or DCS sequence keeps. With more, the
/// sequence dispatches nothing.
pub const INTERMEDIATES_MAX: usize = INTERMEDIATES.value;

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    const BEGIN: &str = "<!-- bounds table: generated from src/bounds.rs -->\n";
    const END: &str = "<!-- end of bounds table -->\n";

    /// The README table, generated from [`BOUNDS`].
    fn table() -> String {
        let mut t = String::from(
            "| Bound | Value | What it bounds | Past it | Citation |\n|---|---|---|---|---|\n",
        );
        for b in &BOUNDS {
            t += &format!(
                "| `{}` | {} | {} | {} | {} |\n",
                b.name, b.value, b.what, b.past, b.cite
            );
        }
        t
    }

    #[test]
    fn readme_table_matches_bounds() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");
        let readme = std::fs::read_to_string(path).expect("README.md");
        let start = readme
            .find(BEGIN)
            .expect("README.md lacks the begin marker")
            + BEGIN.len();
        let end = readme.find(END).expect("README.md lacks the end marker");
        let want = table();
        if readme[start..end] == want {
            return;
        }
        if std::env::var_os("VTIUM_PARSE_BLESS").is_some() {
            let fresh = format!("{}{want}{}", &readme[..start], &readme[end..]);
            std::fs::write(path, fresh).expect("rewrite README.md");
            return;
        }
        panic!(
            "README.md's bounds table differs from src/bounds.rs; \
             rerun with VTIUM_PARSE_BLESS=1 to rewrite it. Generated:\n{want}"
        );
    }

    // tsm-vte-int.h:25, 28, 33-38; libtsm.h:1538
    #[test]
    fn values_are_the_c_references() {
        assert_eq!(PARAMS_MAX, 16);
        assert_eq!(SUBPARAMS_MAX, 8);
        assert_eq!(PARAM_VALUE_CAP, 0xffff);
        assert_eq!(XTGETTCAP_MAX, 1039);
    }

    // tmux.h:3509, input-buffer-size; the C reference kept 127.
    #[test]
    fn osc_max_is_tmuxs_input_buffer() {
        assert_eq!(OSC_MAX, 1_048_576);
    }

    // vt100.net, collect; the C reference has no such bound.
    #[test]
    fn intermediates_max_is_vt100_nets() {
        assert_eq!(INTERMEDIATES_MAX, 2);
    }
}
