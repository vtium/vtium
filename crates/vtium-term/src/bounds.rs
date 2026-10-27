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
    what: "Cells a line holds, and columns a screen and its damage have",
    past: "A line is made or widened to at most this many cells, and a screen and its damage to at most this many columns; the grid's width comes from the display and the font, never from the stream",
    cite: "vtium's own: a line's style and link tables are indexed in 16 bits and compact at `TABLE_GROWTH` times the line's cells, which this keeps inside 65535; a 15360-pixel display in a 4-pixel font is 3840 columns",
};

const ROWS: Bound = Bound {
    name: "ROWS_MAX",
    value: 2048,
    what: "Rows a screen and its damage have",
    past: "A screen and its damage are made or resized to at most this many rows",
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
    what: "Bytes of grapheme clusters a line keeps, per cell of the width it was written at (a line trimmed for the scrollback keeps what its cells name): each cluster's codepoints in UTF-8 and a length byte, so a base and two combining marks a cell on average",
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

const SB_MAX: Bound = Bound {
    name: "SB_SIZE_MAX",
    value: 100_000,
    what: "Scrollback lines any cap may allow, the configured override included",
    past: "A configured cap is clamped to it",
    cite: "vtium's own: the largest of the operator's caps, where the worst case a program can write is 531 MiB (ADR-1001, \"Memory\"); the operator asked for the cap to be configurable (2026-10-26)",
};

const REPLIES_PENDING: Bound = Bound {
    name: "REPLIES_PENDING_MAX",
    value: 65_536,
    what: "Bytes of replies and input reports the terminal holds before the host takes them",
    past: "The input call returns after the byte whose reply reached it, and takes nothing until the host has taken the replies, so a program that queries without reading cannot grow them",
    cite: "vtium's own, at `PTY_REPLY_HIGH` (64 KiB), where the C reference stopped reading a child that queried without reading (pty.c:68)",
};

const VERSION: Bound = Bound {
    name: "VERSION_MAX",
    value: 64,
    what: "Bytes of the text XTVERSION replies, which the host sets",
    past: "`Terminal::set_version` refuses the text, and the reply keeps the one before",
    cite: "`TSM_VTE_VERSION_MAX`, libtsm.h:1497",
};

const TERMCAP_ENTRIES: Bound = Bound {
    name: "TERMCAP_ENTRIES_MAX",
    value: 1024,
    what: "Capabilities in the table XTGETTCAP answers from",
    past: "`Termcap::new` refuses the table",
    cite: "`TSM_VTE_TERMCAP_MAX`, libtsm.h:1537",
};

const TERMCAP_NAME: Bound = Bound {
    name: "TERMCAP_NAME_MAX",
    value: 32,
    what: "Bytes of a capability's name, in the table and in a request",
    past: "`Termcap::new` refuses the table; a requested name past it ends the reply's list",
    cite: "`TSM_VTE_TERMCAP_NAME_MAX`, libtsm.h:1538",
};

const TERMCAP_VALUE: Bound = Bound {
    name: "TERMCAP_VALUE_MAX",
    value: 256,
    what: "Bytes of a capability's value",
    past: "`Termcap::new` refuses the table",
    cite: "`TSM_VTE_TERMCAP_VALUE_MAX`, libtsm.h:1539",
};

const XTGETTCAP_NAMES_BOUND: Bound = Bound {
    name: "XTGETTCAP_NAMES",
    value: 16,
    what: "Names one XTGETTCAP request is answered for",
    past: "The reply's list ends after the sixteenth, with the `;` before the seventeenth",
    cite: "`XTGETTCAP_NAMES`, tsm-vte-int.h:36; vtium-parse's `XTGETTCAP_MAX` holds sixteen names of `TERMCAP_NAME_MAX` bytes",
};

const XTGETTCAP_REPLY: Bound = Bound {
    name: "XTGETTCAP_REPLY_MAX",
    value: 4096,
    what: "Bytes of one XTGETTCAP reply, its DCS and ST included",
    past: "The list ends before the pair that would pass it; a reply of exactly this many bytes is sent",
    cite: "`XTGETTCAP_REPLY_MAX`, tsm-vte.c:131; the C reference reserved a byte for a `;` the last pair does not take, so it refused a reply of exactly 4096 bytes",
};

const CLIPBOARD: Bound = Bound {
    name: "CLIPBOARD_MAX",
    value: 512 * 1024,
    what: "Bytes of selection one OSC 52 sets, decoded, which the terminal holds until the host takes them; a later set replaces one the host has not taken",
    past: "The set is dropped whole, and the selection keeps what it held",
    cite: "vtium's own: the largest power of two whose base64, 699,052 bytes, fits vtium-parse's `OSC_MAX` with its `52;` and targets, so one byte over still reaches the check; tmux drops a sequence past 1 MiB, its `input-buffer-size` (`tmux.h:3509` at `df302c5`), and Ghostty an OSC 52 past 8 MiB (`src/terminal/osc.zig:365` at `a732580`)",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 16] = [
    COLUMNS,
    ROWS,
    CLUSTER,
    CLUSTER_BYTES,
    SB_SMALL,
    SB_MEDIUM,
    SB_LARGE,
    SB_MAX,
    REPLIES_PENDING,
    VERSION,
    TERMCAP_ENTRIES,
    TERMCAP_NAME,
    TERMCAP_VALUE,
    XTGETTCAP_NAMES_BOUND,
    XTGETTCAP_REPLY,
    CLIPBOARD,
];

/// Cells a line holds, and columns a screen and its damage have. The
/// grid's width comes from the display and the font, so no program reaches
/// this bound.
pub const COLUMNS_MAX: usize = COLUMNS.value;

/// Rows a screen and its damage have.
pub const ROWS_MAX: usize = ROWS.value;

/// Codepoints one grapheme cluster keeps; those past it are dropped.
pub const CLUSTER_MAX: usize = CLUSTER.value;

/// Bytes of grapheme clusters a line keeps per cell of the width it was
/// written at, length bytes included.
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

/// The most scrollback lines any cap allows, a configured one included.
pub const SB_SIZE_MAX: usize = SB_MAX.value;

/// The most bytes of replies held before the host takes them.
pub const REPLIES_PENDING_MAX: usize = REPLIES_PENDING.value;

/// The most bytes of XTVERSION's text.
pub const VERSION_MAX: usize = VERSION.value;

/// The most capabilities in XTGETTCAP's table.
pub const TERMCAP_ENTRIES_MAX: usize = TERMCAP_ENTRIES.value;

/// The most bytes of a capability's name.
pub const TERMCAP_NAME_MAX: usize = TERMCAP_NAME.value;

/// The most bytes of a capability's value.
pub const TERMCAP_VALUE_MAX: usize = TERMCAP_VALUE.value;

/// The most names one XTGETTCAP request is answered for.
pub const XTGETTCAP_NAMES: usize = XTGETTCAP_NAMES_BOUND.value;

/// The most bytes of one XTGETTCAP reply.
pub const XTGETTCAP_REPLY_MAX: usize = XTGETTCAP_REPLY.value;

/// The most bytes of selection one OSC 52 sets, decoded: 512 KiB.
pub const CLIPBOARD_MAX: usize = CLIPBOARD.value;

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
