//! The image store's bounds: one source for the constants the code uses
//! and for the table in this crate's README (ADR-1100, D2 and D5).
//!
//! Each bound is a [`Bound`] row holding its value, what happens past it,
//! and where it comes from; the public constant takes its value from the
//! row. The values start from the C reference's and move later by
//! measurement on vtium. The README table is generated from [`BOUNDS`],
//! and `readme_table_matches_bounds` fails when the committed copy
//! differs; `VTIUM_IMAGE_BLESS=1 cargo test -p vtium-image readme_table`
//! rewrites it.

/// One bound and its account, a row of the README table.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Bound {
    name: &'static str,
    value: u64,
    what: &'static str,
    past: &'static str,
    cite: &'static str,
}

const IMAGES: Bound = Bound {
    name: "IMAGES_PER_BUFFER",
    value: 1024,
    what: "Images one buffer of a store holds, main or alternate",
    past: "A new image evicts from that buffer alone in the eviction order, and is refused when nothing there may go",
    cite: "The C reference's `TSM_SCREEN_IMAGES_PER_STORE`",
};

const PLACEMENTS: Bound = Bound {
    name: "PLACEMENTS_PER_BUFFER",
    value: 4096,
    what: "Placements one buffer holds",
    past: "A new placement evicts from that buffer alone, or is refused; a cut or a scroll keeps its largest pieces and drops the smallest, as if their cells were erased",
    cite: "The C reference's `TSM_SCREEN_PLACEMENTS_PER_STORE`",
};

const SIDE: Bound = Bound {
    name: "IMAGE_SIDE_MAX",
    value: 65_535,
    what: "Pixels an image has a side, in every protocol",
    past: "The image is refused (EINVAL); `max-graphic-size` sets the working limit below it",
    cite: "The C reference's `TSM_SCREEN_IMAGE_MAX`",
};

const CELL: Bound = Bound {
    name: "CELL_MAX",
    value: 1024,
    what: "Pixels a cell has a side, the unit a placement measures its pixels in",
    past: "The cell size is refused and the one before it stays",
    cite: "The C reference's `TSM_SCREEN_CELL_MAX`",
};

const BOX: Bound = Bound {
    name: "BOX_CELLS_MAX",
    value: 10_000,
    what: "Cells a placement's box has in each direction, columns and rows each",
    past: "The placement is refused (EINVAL), and so is a picture scaled past that box",
    cite: "The C reference's `TSM_SCREEN_IMAGE_MAX_CELLS`, checked against the columns and the rows each",
};

const RELATIVE_DEPTH: Bound = Bound {
    name: "RELATIVE_DEPTH_MAX",
    value: 8,
    what: "Relative kitty placements a chain holds below the placement it starts from, which is on a line or virtual",
    past: "The put is refused with ETOODEEP and places nothing",
    cite: "The kitty spec's least: \"at least 8 levels\"; ADR-1100, D13",
};

const IMAGE_CHARGE: Bound = Bound {
    name: "IMAGE_OVERHEAD",
    value: 512,
    what: "Bytes of the budget an image costs beside its pixels, which pay for its first placement",
    past: "Counted in every reservation",
    cite: "The C reference's `TSM_SCREEN_IMAGE_OVERHEAD`; the maintainer, 2026-11-01",
};

const PLACEMENT_CHARGE: Bound = Bound {
    name: "PLACEMENT_OVERHEAD",
    value: 256,
    what: "Bytes of the budget each placement of an image past its first costs, a piece a cut makes included",
    past: "A new placement whose charge the budget refuses is refused; a piece whose charge it refuses goes, smallest first",
    cite: "The C reference's `TSM_SCREEN_PLACEMENT_OVERHEAD`",
};

const FRAMES: Bound = Bound {
    name: "AUTO_FRAMES",
    value: 4,
    what: "Frames of the head's displays in RGBA that `image-storage-limit = auto` allows each console",
    past: "The sum is clamped to `AUTO_FLOOR` and `AUTO_CAP`",
    cite: "The maintainer, 2026-11-01: four frames, held as a named constant so it is easy to adjust",
};

const FLOOR: Bound = Bound {
    name: "AUTO_FLOOR",
    value: 16 << 20,
    what: "The fewest bytes `auto` allows a console, 16 MiB",
    past: "A smaller sum is raised to it",
    cite: "The maintainer, 2026-11-01, as the C reference's budget had it",
};

const CAP: Bound = Bound {
    name: "AUTO_CAP",
    value: 192 << 20,
    what: "The most bytes `auto` allows a console, 192 MiB",
    past: "A larger sum is lowered to it",
    cite: "The maintainer, 2026-11-01, as the C reference's budget had it",
};

const FRAMES_COUNT: Bound = Bound {
    name: "FRAMES_MAX",
    value: 1024,
    what: "Frames one kitty animation holds, its root frame included",
    past: "A new frame is refused with ENOSPC and the animation stays as it was",
    cite: "ADR-1102, D1: a starting value, held as a named constant",
};

const FRAME_WHOLE: Bound = Bound {
    name: "FRAME_WHOLE_MAX",
    value: 65_536,
    what: "Bytes of a whole frame at or below which every frame of the image is composed into whole pixels when its command arrives, as a picture of 128 by 128 pixels is",
    past: "A larger image keeps a frame as the change the program sent, unless another of D1's rules composes it",
    cite: "The maintainer, 2026-11-02: a starting value (ADR-1102, D1)",
};

const FRAME_CHAIN: Bound = Bound {
    name: "FRAME_CHAIN_MAX",
    value: 8,
    what: "Layers composing any frame applies, counted from the nearest whole frame or color beneath it",
    past: "A frame that would apply more is composed into whole pixels when its command arrives",
    cite: "The maintainer, 2026-11-02: a starting value (ADR-1102, D1)",
};

const FRAME_CACHE: Bound = Bound {
    name: "FRAME_CACHE_FRAMES",
    value: 2,
    what: "Composed frames the store keeps for each image that holds a change frame, and the whole frames the frame allowance charges that image for its cache",
    past: "The frame composed longest ago leaves the cache",
    cite: "The maintainer, 2026-11-02: a starting value (ADR-1102, D1)",
};

const FRAME_ALLOWANCE: Bound = Bound {
    name: "FRAME_ALLOWANCE_FACTOR",
    value: 5,
    what: "Times the console's image budget that its frame allowance holds, for the frames past each animation's root frame",
    past: "A frame command frees other animations' frames, least recently used first, or is refused with ENOSPC",
    cite: "The maintainer, 2026-11-01, as kitty keeps its frames under a quota of five times its base (ADR-1102, D1)",
};

const FRAME_CHARGE: Bound = Bound {
    name: "FRAME_OVERHEAD",
    value: 64,
    what: "Bytes of the frame allowance each whole frame, and each layer of a change frame, costs beside its pixels",
    past: "Counted in every frame reservation",
    cite: "ADR-1102, D1: a starting value",
};

/// Every bound, in the README's order.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BOUNDS: &[Bound] = &[
    IMAGES,
    PLACEMENTS,
    SIDE,
    CELL,
    BOX,
    RELATIVE_DEPTH,
    IMAGE_CHARGE,
    PLACEMENT_CHARGE,
    FRAMES,
    FLOOR,
    CAP,
    FRAMES_COUNT,
    FRAME_WHOLE,
    FRAME_CHAIN,
    FRAME_CACHE,
    FRAME_ALLOWANCE,
    FRAME_CHARGE,
];

/// Images one buffer holds.
pub const IMAGES_PER_BUFFER: u64 = IMAGES.value;
/// Placements one buffer holds.
pub const PLACEMENTS_PER_BUFFER: u64 = PLACEMENTS.value;
/// Pixels an image has a side.
pub const IMAGE_SIDE_MAX: u64 = SIDE.value;
/// Pixels a cell has a side.
pub const CELL_MAX: u64 = CELL.value;
/// Cells a box has in each direction.
pub const BOX_CELLS_MAX: u64 = BOX.value;
/// Relative placements a chain holds below its first placement.
pub const RELATIVE_DEPTH_MAX: u64 = RELATIVE_DEPTH.value;
/// Bytes an image costs beside its pixels.
pub const IMAGE_OVERHEAD: u64 = IMAGE_CHARGE.value;
/// Bytes each placement past an image's first costs.
pub const PLACEMENT_OVERHEAD: u64 = PLACEMENT_CHARGE.value;
/// Frames `auto` allows.
pub const AUTO_FRAMES: u64 = FRAMES.value;
/// The fewest bytes `auto` allows.
pub const AUTO_FLOOR: u64 = FLOOR.value;
/// The most bytes `auto` allows.
pub const AUTO_CAP: u64 = CAP.value;
/// Frames an animation holds, its root included.
pub const FRAMES_MAX: u64 = FRAMES_COUNT.value;
/// Bytes of a whole frame at or below which frames are stored whole.
pub const FRAME_WHOLE_MAX: u64 = FRAME_WHOLE.value;
/// Layers composing a frame applies at most.
pub const FRAME_CHAIN_MAX: u64 = FRAME_CHAIN.value;
/// Composed frames cached for each image with a change frame.
pub const FRAME_CACHE_FRAMES: u64 = FRAME_CACHE.value;
/// Times the image budget the frame allowance holds.
pub const FRAME_ALLOWANCE_FACTOR: u64 = FRAME_ALLOWANCE.value;
/// Bytes a whole frame or a layer costs beside its pixels.
pub const FRAME_OVERHEAD: u64 = FRAME_CHARGE.value;

/// The budget `image-storage-limit = auto` gives a console whose head has
/// displays of the sizes `displays` lists, in pixels: [`AUTO_FRAMES`]
/// frames of each in RGBA, added over the displays, clamped to
/// [`AUTO_FLOOR`] and [`AUTO_CAP`] (ADR-1100, D5). The binary calls it
/// again when a display joins or leaves the head or changes mode.
pub fn auto_limit(displays: impl IntoIterator<Item = (u32, u32)>) -> u64 {
    let frame = |(w, h): (u32, u32)| {
        u64::from(w)
            .saturating_mul(u64::from(h))
            .saturating_mul(4)
            .saturating_mul(AUTO_FRAMES)
    };
    displays
        .into_iter()
        .map(frame)
        .fold(0u64, u64::saturating_add)
        .clamp(AUTO_FLOOR, AUTO_CAP)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    const BEGIN: &str = "<!-- bounds:begin -->\n";
    const END: &str = "<!-- bounds:end -->";

    fn table() -> String {
        let mut out = String::from(
            "| Bound | Value | What it bounds | Past it | Source |\n|---|---|---|---|---|\n",
        );
        for b in BOUNDS {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                b.name, b.value, b.what, b.past, b.cite
            ));
        }
        out
    }

    #[test]
    fn auto_is_four_frames_of_the_heads_displays_within_its_fence() {
        // D5: about 16 MiB at 1366x768, and 31.6 MiB at 1920x1080
        assert_eq!(auto_limit([(1366, 768)]), 1366 * 768 * 4 * 4);
        assert_eq!(auto_limit([(1024, 768)]), AUTO_FLOOR);
        assert_eq!(auto_limit([(1920, 1080)]), 1920 * 1080 * 4 * 4);
        assert_eq!(
            auto_limit([(1920, 1080), (1920, 1080)]),
            2 * 1920 * 1080 * 4 * 4
        );
        assert_eq!(auto_limit([]), AUTO_FLOOR);
        assert_eq!(auto_limit([(7680, 4320); 4]), AUTO_CAP);
        assert_eq!(auto_limit([(u32::MAX, u32::MAX)]), AUTO_CAP);
    }

    #[test]
    fn readme_table_matches_bounds() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");
        let readme = std::fs::read_to_string(path).expect("README.md");
        let start = readme
            .find(BEGIN)
            .expect("README.md lacks the begin marker")
            .saturating_add(BEGIN.len());
        let end = readme.find(END).expect("README.md lacks the end marker");
        let want = table();
        if readme.get(start..end) == Some(want.as_str()) {
            return;
        }
        if std::env::var_os("VTIUM_IMAGE_BLESS").is_some() {
            let fresh = format!(
                "{}{want}{}",
                readme.get(..start).unwrap_or_default(),
                readme.get(end..).unwrap_or_default()
            );
            std::fs::write(path, fresh).expect("rewrite README.md");
            return;
        }
        panic!(
            "README.md's bounds table differs from src/bounds.rs; \
             rerun with VTIUM_IMAGE_BLESS=1 to rewrite it. Generated:\n{want}"
        );
    }
}
