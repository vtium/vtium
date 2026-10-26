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
    value: 128 - 1,
    what: "Bytes an OSC string keeps, as UTF-8: `OSC_MAX_LEN` (128) less the NUL that ends it",
    past: "A character that would pass it is dropped whole, a shorter one after it may still fit, and the string still dispatches",
    cite: "`OSC_MAX_LEN`, tsm-vte-int.h:31; tsm-vte.c:2912-2921, 3006",
};

const XTGETTCAP: Bound = Bound {
    name: "XTGETTCAP_MAX",
    value: 16 * (2 * 32 + 1) - 1,
    what: "Bytes an XTGETTCAP payload keeps: 16 hex-encoded names of 32 bytes and the `;` between each two",
    past: "The rest is dropped and the request is flagged `overflowed`, which the C reference answers `DCS 0 + r ST`",
    cite: "`XTGETTCAP_MAX_LEN`, tsm-vte-int.h:33-38; libtsm.h:1538; tsm-vte.c:3087-3091, 3149-3152",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 5] = [PARAMS, SUBPARAMS, VALUE, OSC, XTGETTCAP];

/// Parameters a CSI sequence or DCS string keeps; those past it are
/// dropped and the sequence still dispatches.
pub const PARAMS_MAX: usize = PARAMS.value;

/// Colon sub-parameters one parameter keeps; past it the colon and every
/// digit up to the next `;` are dropped.
pub const SUBPARAMS_MAX: usize = SUBPARAMS.value;

/// A parameter or sub-parameter takes digits while it is at most this, so
/// a value reaches at most `PARAM_VALUE_CAP * 10 + 9`.
pub const PARAM_VALUE_CAP: i32 = VALUE.value as i32;

/// Bytes an OSC string keeps, as UTF-8. A character that would pass it is
/// dropped whole.
pub const OSC_MAX: usize = OSC.value;

/// Bytes an XTGETTCAP payload keeps. Past it the request is flagged.
pub const XTGETTCAP_MAX: usize = XTGETTCAP.value;

#[cfg(test)]
mod tests {
    use super::*;

    const BEGIN: &str = "<!-- bounds table: generated from src/bounds.rs -->\n";
    const END: &str = "<!-- end of bounds table -->\n";

    /// The README table, generated from [`BOUNDS`].
    fn table() -> String {
        let mut t = String::from(
            "| Bound | Value | What it bounds | Past it | C reference |\n|---|---|---|---|---|\n",
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

    // tsm-vte-int.h:25, 28, 31, 33-38; libtsm.h:1538
    #[test]
    fn values_are_the_c_references() {
        assert_eq!(PARAMS_MAX, 16);
        assert_eq!(SUBPARAMS_MAX, 8);
        assert_eq!(PARAM_VALUE_CAP, 0xffff);
        assert_eq!(OSC_MAX, 127);
        assert_eq!(XTGETTCAP_MAX, 1039);
    }
}
