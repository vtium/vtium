//! The terminal state's bounds: one source for the constants the code uses
//! and for the table in this crate's README.
//!
//! Each bound is a [`Bound`] row holding its value, what happens past it,
//! and where it comes from; the public constant takes its value from the
//! row. The README table is generated from [`BOUNDS`], and
//! `readme_table_matches_bounds` fails when the committed copy differs.
//! `VTIUM_TERM_BLESS=1 cargo test -p vtium-term readme_table` rewrites it.

/// One bound and its account, a row of the README table.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Bound {
    name: &'static str,
    value: usize,
    what: &'static str,
    past: &'static str,
    cite: &'static str,
}

const COLUMNS: Bound = Bound {
    name: "COLUMNS_MAX",
    value: 8192,
    what: "Cells a line holds, and columns a screen's damage tracks",
    past: "A line is made or widened to at most this many cells, and damage to at most this many columns; the grid's width comes from the display and the font, never from the stream",
    cite: "vtium's own: a line's style and link tables are indexed in 16 bits and compact at `TABLE_GROWTH` times the line's cells, which this keeps inside 65535; a 15360-pixel display in a 4-pixel font is 3840 columns",
};

const ROWS: Bound = Bound {
    name: "ROWS_MAX",
    value: 2048,
    what: "Rows a screen's damage tracks",
    past: "Damage is made or resized to at most this many rows",
    cite: "vtium's own: an 8640-pixel display in a 6-pixel font is 1440 rows; the bound keeps a resize from the stream (vtium/vtium#44) from allocating without limit",
};

const CLUSTER: Bound = Bound {
    name: "CLUSTER_MAX",
    value: 32,
    what: "Codepoints one grapheme cluster keeps",
    past: "Codepoints past it are dropped, and the cluster keeps its first 32",
    cite: "kmscon ADR 102, \"Cells and clusters\": \"at most 32 codepoints each (extenders past 32 are dropped)\"",
};

const CLUSTER_BYTES: Bound = Bound {
    name: "CLUSTER_BYTES_PER_CELL",
    value: 6,
    what: "Bytes of grapheme clusters a line keeps, per cell of the line: each cluster's codepoints in UTF-8 and a length byte, so a base and two combining marks a cell on average",
    past: "A cluster that would pass it keeps the longest run of its first codepoints that fits, and at least its first codepoint",
    cite: "vtium's own: it holds the worst case of the default 1,000-line scrollback inside the minspec peak (ADR-1001, \"Memory\")",
};

const SB_SMALL: Bound = Bound {
    name: "SB_SIZE_CAP_SMALL",
    value: 10_000,
    what: "Scrollback lines `sb-size` may ask for on a machine with less than 2 GiB of memory",
    past: "`sb-size` is clamped to the cap",
    cite: "The operator's caps, vtium/vtium#10 (2026-10-26)",
};

const SB_MEDIUM: Bound = Bound {
    name: "SB_SIZE_CAP_MEDIUM",
    value: 30_000,
    what: "Scrollback lines `sb-size` may ask for with 2 GiB of memory to less than 4 GiB",
    past: "`sb-size` is clamped to the cap",
    cite: "The operator's caps, vtium/vtium#10 (2026-10-26)",
};

const SB_LARGE: Bound = Bound {
    name: "SB_SIZE_CAP_LARGE",
    value: 100_000,
    what: "Scrollback lines `sb-size` may ask for with 4 GiB of memory or more",
    past: "`sb-size` is clamped to the cap",
    cite: "The operator's caps, vtium/vtium#10 (2026-10-26)",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 7] = [
    COLUMNS,
    ROWS,
    CLUSTER,
    CLUSTER_BYTES,
    SB_SMALL,
    SB_MEDIUM,
    SB_LARGE,
];

/// Cells a line holds, and columns a screen's damage tracks. The grid's
/// width comes from the display and the font, so no program reaches this
/// bound.
pub const COLUMNS_MAX: usize = COLUMNS.value;

/// Rows a screen's damage tracks.
pub const ROWS_MAX: usize = ROWS.value;

/// Codepoints one grapheme cluster keeps; those past it are dropped.
pub const CLUSTER_MAX: usize = CLUSTER.value;

/// Bytes of grapheme clusters a line keeps per cell of the line, length
/// bytes included.
pub const CLUSTER_BYTES_PER_CELL: usize = CLUSTER_BYTES.value;

/// The bytes one cluster takes in its line's arena at most: a length byte
/// and [`CLUSTER_MAX`] codepoints of four UTF-8 bytes.
pub(crate) const CLUSTER_BYTES_MAX: usize = 1 + CLUSTER_MAX * 4;

/// A line's tables compact once they reach this many times what the line
/// can name: its cells, or for clusters, its cluster bytes.
pub(crate) const TABLE_GROWTH: usize = 2;

// A style or link index is 16 bits, 0 reserved: a full table at the widest
// line must fit.
const _: () = assert!(COLUMNS_MAX * TABLE_GROWTH < u16::MAX as usize);
// A cluster's offset in its line's arena is 21 bits of the cell: a full
// arena at the widest line, plus the cluster that triggers compaction,
// must fit.
const _: () =
    assert!(COLUMNS_MAX * TABLE_GROWTH * CLUSTER_BYTES_PER_CELL + CLUSTER_BYTES_MAX < 1 << 21);
// A cluster's length fits its length byte.
const _: () = assert!(CLUSTER_BYTES_MAX - 1 <= u8::MAX as usize);

/// The most scrollback lines below 2 GiB of memory.
pub const SB_SIZE_CAP_SMALL: usize = SB_SMALL.value;

/// The most scrollback lines from 2 GiB to below 4 GiB.
pub const SB_SIZE_CAP_MEDIUM: usize = SB_MEDIUM.value;

/// The most scrollback lines from 4 GiB.
pub const SB_SIZE_CAP_LARGE: usize = SB_LARGE.value;

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
        if std::env::var_os("VTIUM_TERM_BLESS").is_some() {
            let fresh = format!("{}{want}{}", &readme[..start], &readme[end..]);
            std::fs::write(path, fresh).expect("rewrite README.md");
            return;
        }
        panic!(
            "README.md's bounds table differs from src/bounds.rs; \
             rerun with VTIUM_TERM_BLESS=1 to rewrite it. Generated:\n{want}"
        );
    }
}
