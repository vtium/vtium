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
    what: "Bytes a PSF file holds: the whole file, decompressed when gzipped, and a gzipped file's own bytes",
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

const CELL: Bound = Bound {
    name: "CELL_SIDE_MAX",
    value: 2048,
    what: "A cell's width or height in pixels. A glyph is at most two cells wide",
    past: "A font scales by the largest whole factor that keeps its cell inside; a sprite or glyph asked for past it is not drawn",
    cite: "vtium's own: the C reference scaled a bitmap font to any height it was asked for (font_psf.c:247-249, font_unifont.c:229-231). A cell of 2048 is taller than a 7680x2160 display's one row",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 6] = [GLYPHS, WIDTH, HEIGHT, BYTES, ENTRIES, CELL];

/// Glyphs a PSF font holds; a font with more is refused.
pub const PSF_GLYPHS_MAX: u32 = GLYPHS.value;

/// A PSF glyph's width in pixels; a wider font is refused.
pub const PSF_WIDTH_MAX: u32 = WIDTH.value;

/// A PSF glyph's height in pixels; a taller font is refused.
pub const PSF_HEIGHT_MAX: u32 = HEIGHT.value;

/// Bytes a PSF file holds, decompressed, and a gzipped file's own bytes; a
/// larger file is refused.
pub const PSF_BYTES_MAX: u32 = BYTES.value;

/// Codepoints a PSF unicode table maps; a table listing more is refused.
pub const PSF_ENTRIES_MAX: u32 = ENTRIES.value;

/// A cell's width or height in pixels. A font scales only as far as keeps
/// its cell inside, and a glyph is at most two cells wide.
pub const CELL_SIDE_MAX: u32 = CELL.value;

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
