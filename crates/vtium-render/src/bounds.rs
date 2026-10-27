//! The renderer's bounds: one source for the constants the code uses and
//! for the table in this crate's README.
//!
//! The README table is generated from [`BOUNDS`], and
//! `readme_table_matches_bounds` fails when the committed copy differs.
//! `VTIUM_RENDER_BLESS=1 cargo test -p vtium-render readme_table` rewrites
//! it.

/// One bound and its account, a row of the README table.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Bound {
    name: &'static str,
    value: usize,
    what: &'static str,
    past: &'static str,
    cite: &'static str,
}

const POOL_BYTES: Bound = Bound {
    name: "POOL_BYTES_PERFORMANCE",
    value: 64 * 1024 * 1024,
    what: "Bytes of glyph pixels each of the four glyph pools holds in the performance profile; minspec's pools hold two screens of their format",
    past: "Older glyphs are evicted; a glyph that would evict one the frame draws, or that alone passes it, is drawn uncached",
    cite: "gltex's per-pool budget, `GLTEX_CACHE_MAX_BYTES` (gltex.c:207-241); ADR-1300 D3 and D4",
};

const ROW_SCRATCH: Bound = Bound {
    name: "ROW_SCRATCH_MAX",
    value: 1024 * 1024,
    what: "Bytes of the row scratch, where each run of a row's drawn cells is composed",
    past: "The run is composed in bands of pixel lines that fit",
    cite: "ADR-1300 D5: one 16-pixel row across 1366 pixels is 87,424 bytes",
};

const TIMER_SAMPLES: Bound = Bound {
    name: "RENDER_TIMER_SAMPLES",
    value: 1024,
    what: "Frames whose time the render timer keeps in an interval, for the median and the 95th percentile",
    past: "Later frames count in the mean and the maximum only",
    cite: "`RENDER_TIMER_SAMPLES`, render_timer.h",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 3] = [POOL_BYTES, ROW_SCRATCH, TIMER_SAMPLES];

/// Bytes each glyph pool may hold in the performance profile.
pub const POOL_BYTES_PERFORMANCE: u64 = POOL_BYTES.value as u64;

/// Bytes the row scratch may hold; a taller row is composed in bands.
pub const ROW_SCRATCH_MAX: usize = ROW_SCRATCH.value;

/// Frames whose time an interval keeps for its percentiles.
pub const RENDER_TIMER_SAMPLES: usize = TIMER_SAMPLES.value;

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
        if std::env::var_os("VTIUM_RENDER_BLESS").is_some() {
            let fresh = format!("{}{want}{}", &readme[..start], &readme[end..]);
            std::fs::write(path, fresh).expect("rewrite README.md");
            return;
        }
        panic!(
            "README.md's bounds table differs from src/bounds.rs; \
             rerun with VTIUM_RENDER_BLESS=1 to rewrite it. Generated:\n{want}"
        );
    }
}
