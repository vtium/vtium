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
    cite: "ADR-1001, \"The line\": a cluster keeps at most 32 codepoints, its extenders past them dropped",
};

const CLUSTER_BYTES: Bound = Bound {
    name: "CLUSTER_BYTES_PER_CELL",
    value: 6,
    what: "Bytes of grapheme clusters a line keeps, per cell of the width it was written at (a line trimmed for the scrollback keeps what its cells name): each cluster's codepoints in UTF-8 and a length byte, so a base and two combining marks a cell on average",
    past: "A cluster that would pass it keeps the longest run of its first codepoints that fits, and at least its first codepoint",
    cite: "vtium's own: it bounds the worst a row can cost, which `sb-memory`'s default holds inside the minspec peak (ADR-1001, \"Memory\")",
};

const PLACEHOLDER: Bound = Bound {
    name: "PLACEHOLDER_MARKS",
    value: 3,
    what: "Codepoints a cluster that starts with U+10EEEE, the kitty graphics protocol's Unicode placeholder, keeps after it whatever the line's cluster budget, in a build with images: its picture row, its picture column and the top byte of its image id. A build without images keeps such a cluster within the budget, as any other",
    past: "Codepoints past the third are dropped, so the cluster takes at most 17 bytes of its line's arena with its length byte; the line's budget of `CLUSTER_BYTES_PER_CELL` counts every other cluster",
    cite: "ADR-1001, \"Placeholder cells\" (the maintainer, 2026-11-02); ADR-1100, D13",
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

const SB_MEMORY_MIN_BOUND: Bound = Bound {
    name: "SB_MEMORY_MIN",
    value: 1 << 20,
    what: "The least byte budget `sb-memory` sets the scrollback",
    past: "A smaller `sb-memory` is clamped up to it, with a warning",
    cite: "The operator's fence, vtium/vtium#131 (2026-10-28): about 138 rows at the 7,560-byte worst case of a build without images (about 100 at the 10,450-byte worst case of a build with images), a screen of hostile rows on a minspec display, and thousands of plain lines",
};

const SB_MEMORY_MAX_BOUND: Bound = Bound {
    name: "SB_MEMORY_MAX",
    value: 1 << 30,
    what: "The most byte budget `sb-memory` may set the scrollback; a quarter of the machine's memory caps it lower, and never below `SB_MEMORY_MIN`",
    past: "A larger `sb-memory` is clamped to the lesser of it and a quarter of the machine's memory, with a warning",
    cite: "The operator's fence, vtium/vtium#131 (2026-10-28): 128 MiB on the 512 MB minspec board, scaled by memory as `sb_size_cap` scales lines",
};

const REPLIES_PENDING: Bound = Bound {
    name: "REPLIES_PENDING_MAX",
    value: 65_536,
    what: "Bytes of replies and input reports the terminal holds before the host takes them",
    past: "The input call returns after the byte whose reply reached it, and takes nothing until the host has taken the replies, so a program that queries without reading cannot grow them",
    cite: "vtium's own, at vtium-platform's `REPLY_HIGH` (64 KiB), past which vtium stops reading a program that queries without reading",
};

const VERSION: Bound = Bound {
    name: "VERSION_MAX",
    value: 64,
    what: "Bytes of the text XTVERSION replies, which the host sets",
    past: "`Terminal::set_version` refuses the text, and the reply keeps the one before",
    cite: "vtium's own: XTVERSION's text is a program's name and version, which 64 bytes hold with room to spare",
};

const TERMCAP_ENTRIES: Bound = Bound {
    name: "TERMCAP_ENTRIES_MAX",
    value: 1024,
    what: "Capabilities in the table XTGETTCAP answers from",
    past: "`Termcap::new` refuses the table",
    cite: "vtium's own: over five times the 182 names XTGETTCAP answers from vtium's terminfo entry",
};

const TERMCAP_NAME: Bound = Bound {
    name: "TERMCAP_NAME_MAX",
    value: 32,
    what: "Bytes of a capability's name, in the table and in a request",
    past: "`Termcap::new` refuses the table; a requested name past it ends the reply's list",
    cite: "vtium's own: the longest name in vtium's terminfo entry takes 7 bytes",
};

const TERMCAP_VALUE: Bound = Bound {
    name: "TERMCAP_VALUE_MAX",
    value: 256,
    what: "Bytes of a capability's value",
    past: "`Termcap::new` refuses the table",
    cite: "vtium's own: the longest value in vtium's terminfo entry, `initc`'s, takes 93 bytes",
};

const XTGETTCAP_NAMES_BOUND: Bound = Bound {
    name: "XTGETTCAP_NAMES",
    value: 16,
    what: "Names one XTGETTCAP request is answered for",
    past: "The reply's list ends after the sixteenth, with the `;` before the seventeenth",
    cite: "vtium's own: vtium-parse's `XTGETTCAP_MAX` holds sixteen names of `TERMCAP_NAME_MAX` bytes",
};

const XTGETTCAP_REPLY: Bound = Bound {
    name: "XTGETTCAP_REPLY_MAX",
    value: 4096,
    what: "Bytes of one XTGETTCAP reply, its DCS and ST included",
    past: "The list ends before the pair that would pass it; a reply of exactly this many bytes is sent",
    cite: "vtium's own: a page of memory, which holds sixteen of the longest pairs vtium's entry has",
};

const CLIPBOARD: Bound = Bound {
    name: "CLIPBOARD_MAX",
    value: 512 * 1024,
    what: "Bytes of selection one OSC 52 sets, decoded, which the terminal holds until the host takes them; a later set replaces one the host has not taken",
    past: "The set is dropped whole, and the selection keeps what it held",
    cite: "vtium's own: the largest power of two whose base64, 699,052 bytes, fits vtium-parse's `OSC_MAX` with its `52;` and targets, so one byte over still reaches the check; tmux drops a sequence past 1 MiB, its `input-buffer-size` (`tmux.h:3509` at `df302c5`), and Ghostty an OSC 52 past 8 MiB (`src/terminal/osc.zig:365` at `a732580`)",
};

const SGR_STACK: Bound = Bound {
    name: "SGR_STACK_MAX",
    value: 10,
    what: "Pens XTPUSHSGR keeps",
    past: "A push keeps nothing and is counted, and the pop that pairs with it restores nothing, so pushes and pops still pair",
    cite: "ctlseqs, XTPUSHSGR: \"The stack is limited to 10 levels\"; `MAX_SAVED_SGR` and `xtermPushSGR` in xterm 412's `ptyx.h` and `misc.c`",
};

const COLOR_STACK: Bound = Bound {
    name: "COLOR_STACK_MAX",
    value: 10,
    what: "Palettes XTPUSHCOLORS and kitty's OSC 30001 keep, in one stack",
    past: "A push with the stack full keeps nothing and moves nothing, and a parameter past it names no entry",
    cite: "ctlseqs, XTPUSHCOLORS: entries 1 through 10; `MAX_SAVED_SGR` and `xtermPushColors` in xterm 412's `ptyx.h` and `misc.c`. kitty's `color-stack` gives no depth",
};

const LINKS_BYTES: Bound = Bound {
    name: "LINKS_BYTES_MAX",
    value: 256 * 1024,
    what: "Bytes the link store charges for the OSC 8 links it holds, each distinct link once: its id and URI bytes and 64 for its slot, its index entry and allocation",
    past: "The store sweeps the links no line names; with less than a quarter of the bound then free, the scrollback's oldest lines lose their links until a quarter is; when the screens alone hold the bound, the open is refused and the text prints unlinked",
    cite: "ADR-1002, \"Link storage\": about 2,300 links of 50-byte URIs, which keeps the worst default scrollback under the minspec peak; VTE caps a screen at 2^20-2 links (`src/vtedefines.hh:101-119` at `5d75dcf`)",
};

const INPUT_WORK: Bound = Bound {
    name: "INPUT_WORK_MAX",
    value: 1 << 24,
    what: "Image work one input call does, in a build with images: each placement or image a scroll, an ED 2, an eviction or a handler's search visits, and each pixel a handler decodes",
    past: "The input call stops after the byte that passed the bound and returns how many bytes it took; the next call takes the rest, and the resulting state is the same. That byte may lie inside a run of sixel or kitty string data, because an image handler takes a run only as far as the bound allows",
    cite: "ADR-1100, D5: 2^24 a call, so one input call's image work stays bounded however much image data one read brings",
};

const IMAGE_NOTE_REASONS: Bound = Bound {
    name: "IMAGE_NOTE_REASONS_MAX",
    value: 64,
    what: "Reasons, each with its protocol, that a terminal in a build with images counts the notes of, where an image handler says why it dropped what a program sent",
    past: "A note with a new reason is counted nowhere and reaches no log; the reasons already counted keep their counts",
    cite: "vtium's own: vtium-image's handlers write about 30 fixed reasons, so they never reach it; it keeps a handler that writes reasons of its own from growing the table",
};

const IMAGE_NOTE_BYTES: Bound = Bound {
    name: "IMAGE_NOTE_BYTES_MAX",
    value: 160,
    what: "Bytes of one note's reason the terminal keeps, in a build with images",
    past: "The reason is cut at the last character boundary at or before it",
    cite: "vtium's own: vtium-image's handlers write no reason longer than 70 bytes",
};

const BIDI_PARAGRAPH_ROWS: Bound = Bound {
    name: "BIDI_PARAGRAPH_ROWS_MAX",
    value: 500,
    what: "Rows of the cursor's paragraph (the rows a wrap joins) that a change of one of its properties reaches, in a build with the bidi feature: SM and RM of BDSM, 2500 and 2501, SCP and SPD set the property on every row of the paragraph on the screen, then on its rows in the scrollback, newest first, until the paragraph's rows they reached number this many",
    past: "The paragraph's older rows in the scrollback keep the values they had, so the work one sequence does stays bounded however long the paragraph is",
    cite: "ADR-1004 D3: 500 rows a paragraph, as the Terminal Working Group's BiDi recommendation suggests",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 26] = [
    COLUMNS,
    ROWS,
    CLUSTER,
    CLUSTER_BYTES,
    PLACEHOLDER,
    SB_SMALL,
    SB_MEDIUM,
    SB_LARGE,
    SB_MAX,
    SB_MEMORY_MIN_BOUND,
    SB_MEMORY_MAX_BOUND,
    REPLIES_PENDING,
    VERSION,
    TERMCAP_ENTRIES,
    TERMCAP_NAME,
    TERMCAP_VALUE,
    XTGETTCAP_NAMES_BOUND,
    XTGETTCAP_REPLY,
    CLIPBOARD,
    LINKS_BYTES,
    SGR_STACK,
    COLOR_STACK,
    INPUT_WORK,
    IMAGE_NOTE_REASONS,
    IMAGE_NOTE_BYTES,
    BIDI_PARAGRAPH_ROWS,
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

/// Codepoints a placeholder cluster, one that starts with U+10EEEE, keeps
/// after the placeholder whatever its line's cluster budget.
pub(crate) const PLACEHOLDER_MARKS: usize = PLACEHOLDER.value;

/// The bytes a placeholder cluster takes in its line's arena at most: a
/// length byte, the placeholder's 4 UTF-8 bytes and [`PLACEHOLDER_MARKS`]
/// codepoints of up to 4 bytes each.
pub(crate) const PLACEHOLDER_BYTES_MAX: usize = 1 + 4 + PLACEHOLDER_MARKS * 4;

/// A line's tables compact once they reach this many times what the line
/// can name: its cells, or for clusters, its cluster bytes.
pub(crate) const TABLE_GROWTH: usize = 2;

// A style or link index is 16 bits, 0 reserved: a full table at the widest
// line must fit.
const _: () = assert!(COLUMNS_MAX * TABLE_GROWTH < u16::MAX as usize);
// A cluster's offset in its line's arena is 21 bits of the cell: a full
// arena at the widest line, plus the cluster that triggers compaction,
// must fit.
// A line of placeholder cells names up to `PLACEHOLDER_BYTES_MAX` a cell
// beside its budget (ADR-1001, "Placeholder cells"): 376,961 bytes at the
// widest line.
const _: () = assert!(
    COLUMNS_MAX * TABLE_GROWTH * (CLUSTER_BYTES_PER_CELL + PLACEHOLDER_BYTES_MAX)
        + CLUSTER_BYTES_MAX
        < 1 << 21
);
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

/// The least byte budget `sb-memory` sets the scrollback.
pub const SB_MEMORY_MIN: usize = SB_MEMORY_MIN_BOUND.value;

/// The most byte budget `sb-memory` may set the scrollback, before a
/// quarter of the machine's memory caps it.
pub const SB_MEMORY_MAX: usize = SB_MEMORY_MAX_BOUND.value;

/// The most bytes of replies held before the host takes them.
pub const REPLIES_PENDING_MAX: usize = REPLIES_PENDING.value;

/// Image work an input call does before it stops, in a build with images.
#[cfg_attr(not(feature = "images"), allow(dead_code))]
pub(crate) const INPUT_WORK_MAX: u64 = INPUT_WORK.value as u64;

/// Reasons a terminal counts the image handlers' notes of, in a build with
/// images.
#[cfg_attr(not(feature = "images"), allow(dead_code))]
pub(crate) const IMAGE_NOTE_REASONS_MAX: usize = IMAGE_NOTE_REASONS.value;

/// Bytes of one image note's reason a terminal keeps, in a build with
/// images.
#[cfg_attr(not(feature = "images"), allow(dead_code))]
pub(crate) const IMAGE_NOTE_BYTES_MAX: usize = IMAGE_NOTE_BYTES.value;

/// Rows of the cursor's paragraph a change of its properties reaches, in
/// a build with the bidi feature.
#[cfg_attr(not(feature = "bidi"), allow(dead_code))]
pub(crate) const BIDI_PARAGRAPH_ROWS_MAX: usize = BIDI_PARAGRAPH_ROWS.value;

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

/// Bytes the link store charges for the OSC 8 links it holds.
pub const LINKS_BYTES_MAX: usize = LINKS_BYTES.value;

/// The most pens XTPUSHSGR keeps.
pub const SGR_STACK_MAX: usize = SGR_STACK.value;

/// The most palettes XTPUSHCOLORS and OSC 30001 keep.
pub const COLOR_STACK_MAX: usize = COLOR_STACK.value;

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
