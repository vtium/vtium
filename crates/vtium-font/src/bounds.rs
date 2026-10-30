//! The font layer's bounds: one source for the constants the code uses and
//! for the table in this crate's README.
//!
//! Each bound is a [`Bound`] row holding its value, what happens past it,
//! and where it comes from; the public constant takes its value from the
//! row. The README table is generated from [`BOUNDS`], and
//! `readme_table_matches_bounds` fails when the committed copy differs.
//! `VTIUM_FONT_BLESS=1 cargo test -p vtium-font readme_table` rewrites it.

/// One bound and its account, a row of the README table.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Bound {
    name: &'static str,
    value: u32,
    what: &'static str,
    past: &'static str,
    cite: &'static str,
}

const GLYPHS: Bound = Bound {
    name: "PSF_GLYPHS_MAX",
    value: 0xffff,
    what: "Glyphs a PSF font holds",
    past: "The font is refused",
    cite: "`PSF_MAX_GLYPHS`, font_psf.c:62-64, 74, 200 (aaronsb/kmscon#229). The C reference kept 0xffff free as its table's no-entry index; vtium keeps the value, and its table holds glyph indices in 16 bits",
};

const WIDTH: Bound = Bound {
    name: "PSF_WIDTH_MAX",
    value: 32,
    what: "A PSF glyph's width in pixels",
    past: "The font is refused",
    cite: "`PSF_MAX_WIDTH`, font_psf.c:65, 75, 200: a row is read into 32 bits",
};

const HEIGHT: Bound = Bound {
    name: "PSF_HEIGHT_MAX",
    value: 256,
    what: "A PSF glyph's height in pixels",
    past: "The font is refused",
    cite: "`PSF_MAX_HEIGHT`, font_psf.c:66-67, 76, 200: PSF1 holds the height in a byte",
};

const BYTES: Bound = Bound {
    name: "PSF_BYTES_MAX",
    value: 16 << 20,
    what: "Bytes a PSF file holds: the whole file, decompressed when gzipped",
    past: "The font is refused, whatever the bytes past the glyphs hold",
    cite: "`PSF_MAX_BYTES`, font_psf.c:68-69, 77, 207-208, 420-428; the C reference counted only the bytes it read, so it took a PSF1 font without a table whatever followed its glyphs",
};

const ENTRIES: Bound = Bound {
    name: "PSF_ENTRIES_MAX",
    value: 0x11_0000,
    what: "Codepoints a PSF unicode table maps, before duplicates are merged",
    past: "The font is refused",
    cite: "`PSF_MAX_ENTRIES`, font_psf.c:70-72, 78, 383-385: one entry for each Unicode codepoint",
};

const GZIP: Bound = Bound {
    name: "GZIP_BYTES_MAX",
    value: 1 << 20,
    what: "Bytes a gzipped PSF file holds before it is decompressed. kbd's largest gzipped console font is 6 KiB",
    past: "The font is refused before anything is decompressed",
    cite: "vtium's own: the inflater builds a Huffman table for each deflate block, about a microsecond, and a block can be 10 bits, so a 15 MiB file of empty blocks took 14 s to refuse. At 1 MiB the worst file takes about 1 s on the test machine. The C reference read through zlib, whose fixed tables are prebuilt",
};

const CELL: Bound = Bound {
    name: "CELL_SIDE_MAX",
    value: 2048,
    what: "A cell's width or height in pixels. A glyph is at most two cells wide",
    past: "A font scales by the largest whole factor that keeps its cell inside; a sprite or glyph asked for past it is not drawn",
    cite: "vtium's own: the C reference scaled a bitmap font to any height it was asked for (font_psf.c:247-249, font_unifont.c:229-231). A cell of 2048 is taller than a 7680x2160 display's one row",
};

const SET_BYTES: Bound = Bound {
    name: "SET_BYTES_MAX",
    value: 1 << 30,
    what: "Bytes a vitrified set holds",
    past: "The set is refused",
    cite: "vtium's own, from ADR-1200's measured sets: Noto Sans Mono CJK unlimited at 48 pixels is about 100 MB a strike (D1), a full GBK set at 48 pixels 49.8 MB (D13), and a set holds at most three sizes (D11)",
};

const SET_SECTIONS: Bound = Bound {
    name: "SET_SECTIONS_MAX",
    value: 16,
    what: "Sections a set's header lists",
    past: "The set is refused",
    cite: "vtium's own: format 2.0 writes five (ADR-1200 D2), and a minor version adds sections a reader skips",
};

const SET_META: Bound = Bound {
    name: "SET_META_BYTES_MAX",
    value: 1 << 20,
    what: "Bytes of a set's `META` provenance",
    past: "The set is refused",
    cite: "vtium's own: the frit verbatim and a line or three for each source (ADR-1200 D2)",
};

const SET_STRIKES: Bound = Bound {
    name: "SET_STRIKES_MAX",
    value: 32,
    what: "Strikes a set holds",
    past: "The set is refused",
    cite: "vtium's own: a strike for each of four styles at up to three sizes (ADR-1200 D1, D11), and color strikes beside them",
};

const SET_GLYPHS: Bound = Bound {
    name: "SET_GLYPHS_MAX",
    value: 0x11_0000,
    what: "Glyphs one strike holds",
    past: "The set is refused",
    cite: "One for each Unicode codepoint: a set maps single codepoints only (ADR-1200 D2, `CMAP`)",
};

const SET_RUNS: Bound = Bound {
    name: "SET_RUNS_MAX",
    value: 1 << 18,
    what: "Runs of codepoints a set's `CMAP` holds, over every strike",
    past: "The set is refused",
    cite: "vtium's own: the built-in unifont is 3,465 runs, and Noto Sans Mono CJK unlimited, about 44,000 glyphs (ADR-1200 D1), at most 44,000 a strike. A run and its glyph span take 36 bytes in memory, so the map holds at most 9 MiB, and its `CMAP` at most 16,777,352 bytes while it is read",
};

const SET_CHUNKS: Bound = Bound {
    name: "SET_CHUNKS_MAX",
    value: 1 << 18,
    what: "Glyph chunks a set's `GLYF` index lists",
    past: "The set is refused",
    cite: "vtium's own: `SET_BYTES_MAX` of raw chunks is 65,536 of them, and deflated ones are 1.5-4.7 times smaller (ADR-1200 D13)",
};

const SET_CHUNK: Bound = Bound {
    name: "SET_CHUNK_BYTES",
    value: 16 * 1024,
    what: "Bytes a glyph chunk holds, as stored and decoded. No glyph is split across two, so a glyph is at most this",
    past: "The set is refused; the writer starts a new chunk",
    cite: "ADR-1200 D2: \"chunks of at most 16 KiB, no glyph split across two\", the built-in unifont's chunk size",
};

const SET_RECORD: Bound = Bound {
    name: "SET_RECORD_BYTES_MAX",
    value: 64,
    what: "Bytes a record of a set's tables declares",
    past: "The set is refused",
    cite: "vtium's own: format 2.0's largest record is 48 bytes, and a minor version adds fields at a record's end (ADR-1200 D2); it bounds what a table's counts can make the reader hold",
};

const SET_CLUSTERS: Bound = Bound {
    name: "SET_CLUSTERS_MAX",
    value: 1 << 16,
    what: "Clusters a set's `CLUS` maps, over every strike",
    past: "The set is refused",
    cite: "vtium's own: Unicode 18.0's RGI emoji sequences other than single emoji are 2,563, or 3,536 with each keyed again without its U+FE0F; canonical decompositions 1,349 keys; a screen of 320 by 100 cells shows at most 32,000 clusters. A cluster takes 20 bytes in memory and its key at most 64, so the map holds at most 5.25 MiB",
};

const CLUSTER_SCALARS: Bound = Bound {
    name: "CLUSTER_SCALARS_MAX",
    value: 16,
    what: "Scalar values in a cluster's key",
    past: "The set is refused; a longer cluster is looked up as none",
    cite: "vtium's own: Unicode 18.0's longest RGI emoji sequence, a kiss with two skin tones, is 10",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 18] = [
    GLYPHS,
    WIDTH,
    HEIGHT,
    BYTES,
    ENTRIES,
    GZIP,
    CELL,
    SET_BYTES,
    SET_SECTIONS,
    SET_META,
    SET_STRIKES,
    SET_GLYPHS,
    SET_RUNS,
    SET_CHUNKS,
    SET_CHUNK,
    SET_RECORD,
    SET_CLUSTERS,
    CLUSTER_SCALARS,
];

/// Glyphs a PSF font holds; a font with more is refused.
pub const PSF_GLYPHS_MAX: u32 = GLYPHS.value;

/// A PSF glyph's width in pixels; a wider font is refused.
pub const PSF_WIDTH_MAX: u32 = WIDTH.value;

/// A PSF glyph's height in pixels; a taller font is refused.
pub const PSF_HEIGHT_MAX: u32 = HEIGHT.value;

/// Bytes a PSF file holds, decompressed; a larger file is refused.
pub const PSF_BYTES_MAX: u32 = BYTES.value;

/// Codepoints a PSF unicode table maps; a table listing more is refused.
pub const PSF_ENTRIES_MAX: u32 = ENTRIES.value;

/// Bytes a gzipped PSF file holds before it is decompressed; a larger one
/// is refused.
pub const GZIP_BYTES_MAX: u32 = GZIP.value;

/// A cell's width or height in pixels. A font scales only as far as keeps
/// its cell inside, and a glyph is at most two cells wide.
pub const CELL_SIDE_MAX: u32 = CELL.value;

/// Bytes a vitrified set file holds; a larger one is refused.
pub const SET_BYTES_MAX: u32 = SET_BYTES.value;

/// Sections a set's header lists; a set listing more is refused.
pub const SET_SECTIONS_MAX: u32 = SET_SECTIONS.value;

/// Bytes of a set's `META`; a larger one is refused.
pub const SET_META_BYTES_MAX: u32 = SET_META.value;

/// Strikes a set holds; a set with more is refused.
pub const SET_STRIKES_MAX: u32 = SET_STRIKES.value;

/// Glyphs one strike holds; a strike with more is refused.
pub const SET_GLYPHS_MAX: u32 = SET_GLYPHS.value;

/// Runs of codepoints a set's map holds over every strike; a set with
/// more is refused.
pub const SET_RUNS_MAX: u32 = SET_RUNS.value;

/// Glyph chunks a set holds; a set with more is refused.
pub const SET_CHUNKS_MAX: u32 = SET_CHUNKS.value;

/// Bytes a glyph chunk holds, stored and decoded, and so the most a glyph
/// takes in a set.
pub const SET_CHUNK_BYTES: u32 = SET_CHUNK.value;

/// Bytes a record of a set's tables declares; a larger one is refused.
pub const SET_RECORD_BYTES_MAX: u32 = SET_RECORD.value;

/// Clusters a set's `CLUS` maps over every strike; a set with more is
/// refused.
pub const SET_CLUSTERS_MAX: u32 = SET_CLUSTERS.value;

/// Scalar values in a cluster's key; a set with a longer key is refused,
/// and a longer cluster is looked up as none.
pub const CLUSTER_SCALARS_MAX: u32 = CLUSTER_SCALARS.value;

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
            "| Bound | Value | What it bounds | Past it | Source |\n|---|---|---|---|---|\n",
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
        if std::env::var_os("VTIUM_FONT_BLESS").is_some() {
            let fresh = format!("{}{want}{}", &readme[..start], &readme[end..]);
            std::fs::write(path, fresh).expect("rewrite README.md");
            return;
        }
        panic!(
            "README.md's bounds table differs from src/bounds.rs; \
             rerun with VTIUM_FONT_BLESS=1 to rewrite it. Generated:\n{want}"
        );
    }

    // font_psf.c:74-78
    #[test]
    fn values_are_the_c_references() {
        assert_eq!(PSF_GLYPHS_MAX, 0xffff);
        assert_eq!(PSF_WIDTH_MAX, 32);
        assert_eq!(PSF_HEIGHT_MAX, 256);
        assert_eq!(PSF_BYTES_MAX, 16 << 20);
        assert_eq!(PSF_ENTRIES_MAX, 0x11_0000);
    }
}
