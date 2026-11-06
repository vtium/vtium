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
    cite: "ADR-1300 D3 and D4: it bounds the glyph cache's memory, and a full cache changes no pixel",
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
    cite: "It keeps the timer's memory fixed: 1,024 frames is about 17 seconds at 60 frames a second",
};

const POINTER_SIDE: Bound = Bound {
    name: "POINTER_SIDE_MAX",
    value: 64,
    what: "Pixels on the longest side of the pointer's arrow, which is a cell tall",
    past: "The arrow stays this tall over taller cells",
    cite: "64 by 64 is the cursor size DRM reports for `DRM_CAP_CURSOR_WIDTH` and `DRM_CAP_CURSOR_HEIGHT` when a driver sets none, so the arrow fits a cursor plane",
};

const PICTURE_SCREENS: Bound = Bound {
    name: "PICTURE_CACHE_SCREENS",
    value: 2,
    what: "Screens of the display, in 4-byte pixels, that its scaled and turned copies of pictures hold, in a build with `images`",
    past: "The copy no placement of the frame shows that was placed longest ago is evicted; a copy that still does not fit is scaled for each frame it draws and not kept",
    cite: "ADR-1100 D5: it bounds the memory a display's copies take, whatever pictures a program sends",
};

const PRESENT_FRAMES: Bound = Bound {
    name: "PRESENT_HISTORY",
    value: 3,
    what: "Frames whose composed rectangles the GPU path's present keeps, to draw what a buffer of that age lacks, in a build with `gpu`",
    past: "A buffer older than the history is presented whole",
    cite: "`display-buffers`, at most 3 buffers a display (ADR-1401 D2); ADR-1303 D6",
};

const FAILED: Bound = Bound {
    name: "FAILED_GLYPHS",
    value: 1024,
    what: "Glyphs the font drew nothing for that the glyph cache remembers, so the font is not asked for them again every frame",
    past: "A glyph that lands on a remembered one's place takes it over, and the one it replaced is asked for again the next time it is drawn",
    cite: "It bounds the table's memory however many characters a screen shows that no font holds; the table keeps no expiry, since `Renderer::glyphs_changed` and `forget_glyphs` forget the glyphs whose characters arrive",
};

const GLYPH_TEXTURE: Bound = Bound {
    name: "GLYPH_TEXTURE_SIDE",
    value: 2048,
    what: "Pixels on the longer side of each texture a GPU glyph pool lays its slots out in, at most the GPU's largest texture, in a build with `gpu`",
    past: "The pool takes another texture",
    cite: "2048 is the least `GL_MAX_TEXTURE_SIZE` OpenGL ES 3.0 allows, so every GPU takes the pools' textures; ADR-1300 D6",
};

const GLYPH_CEILING: Bound = Bound {
    name: "GLYPH_TEXTURE_CEILING",
    value: 512 * 1024 * 1024,
    what: "Bytes of the textures one display's GPU glyph pools and decoration sprites hold together, in a build with `gpu`",
    past: "A pool takes no new texture: a glyph it has no free slot for is drawn blank, and the pool's slots are reused as its budget evicts glyphs",
    cite: "It bounds the GPU memory a display's glyphs take at twice the 256 MiB the four pools' budgets hold together, which leaves room for the slots a frame holds past its budgets; the issue on GPU texture garbage collection; ADR-1303 D9",
};

const IDLE: Bound = Bound {
    name: "GPU_IDLE_FRAMES",
    value: 600,
    what: "Frames a GPU texture may go unused before the GPU path gives it back: a glyph pool's last texture with no glyph in it, a picture's texture no placement showed, and the copy of the pixels beneath that no draw read, in a build with `gpu`",
    past: "The texture is freed, and made again if a later frame needs it; frames are counted, never wall time, so a display that draws nothing frees nothing",
    cite: "600 frames is ten seconds at 60 frames a second; ADR-1303 D9",
};

/// Every bound, in the README table's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: [Bound; 10] = [
    POOL_BYTES,
    ROW_SCRATCH,
    TIMER_SAMPLES,
    POINTER_SIDE,
    PICTURE_SCREENS,
    PRESENT_FRAMES,
    FAILED,
    GLYPH_TEXTURE,
    GLYPH_CEILING,
    IDLE,
];

/// Bytes each glyph pool may hold in the performance profile.
pub const POOL_BYTES_PERFORMANCE: u64 = POOL_BYTES.value as u64;

/// Bytes the row scratch may hold; a taller row is composed in bands.
pub const ROW_SCRATCH_MAX: usize = ROW_SCRATCH.value;

/// Frames whose time an interval keeps for its percentiles.
pub const RENDER_TIMER_SAMPLES: usize = TIMER_SAMPLES.value;

/// Screens of 4-byte pixels a display's copies of pictures hold.
#[cfg(feature = "images")]
pub const PICTURE_CACHE_SCREENS: usize = PICTURE_SCREENS.value;

/// Frames of composed rectangles the GPU path's present keeps.
#[cfg(feature = "gpu")]
pub const PRESENT_HISTORY: usize = PRESENT_FRAMES.value;

/// Glyphs the font drew nothing for that the cache remembers.
pub(crate) const FAILED_GLYPHS: usize = FAILED.value;

/// The longer side of a GPU glyph pool's texture.
#[cfg(feature = "gpu")]
#[expect(clippy::cast_possible_truncation, reason = "the bound is 2048")]
pub(crate) const GLYPH_TEXTURE_SIDE: u32 = GLYPH_TEXTURE.value as u32;

/// Bytes of one display's GPU glyph textures together.
#[cfg(feature = "gpu")]
pub(crate) const GLYPH_TEXTURE_CEILING: u64 = GLYPH_CEILING.value as u64;

/// Frames an unused GPU texture is kept before it is given back.
#[cfg(feature = "gpu")]
pub const GPU_IDLE_FRAMES: u64 = IDLE.value as u64;

/// The pointer's longest side in pixels.
#[expect(clippy::cast_possible_truncation, reason = "the bound is 64")]
pub const POINTER_SIDE_MAX: u32 = POINTER_SIDE.value as u32;

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
