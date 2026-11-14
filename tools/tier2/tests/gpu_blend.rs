//! The GPU path blends partly transparent picture pixels in linear light
//! on the CPU path's bytes (ADR-1300 D6, D7). A screen of kitty pictures
//! whose alpha ramps from clear to opaque, over rows of several background
//! colors, over text, under text and over another picture, draws on Mesa's
//! llvmpipe exactly as the CPU path draws it: with GPU scaling, and with
//! each scaled picture drawn from the CPU path's copy.
//!
//! The screen is built here and is in no case list, so it has no golden:
//! the CPU path's own drawing judges the GPU path's.
#![cfg(feature = "gpu")]

use std::fs;
use std::path::Path;

use vtium_tier2::compare::Verdict;
use vtium_tier2::draw::{Feed, Screen};
use vtium_tier2::gpu;
use vtium_tier2::pictures::Kitty;

const COLS: usize = 48;
const ROWS: usize = 8;

/// A row of each background, the top four blank, the rest with text the
/// pictures go over and under.
const TEXT: &[u8] = concat!(
    "\x1b[40m                                                \x1b[0m\r\n",
    "\x1b[41m                                                \x1b[0m\r\n",
    "\x1b[42m                                                \x1b[0m\r\n",
    "\x1b[44m                                                \x1b[0m\r\n",
    "\x1b[30;47mtext over and under glass, black on white      \x1b[0m\r\n",
    "\x1b[93;48;5;94mbright yellow on palette brown, 0123456789    \x1b[0m\r\n",
    "\x1b[38;2;10;20;30;48;2;128;128;128mdark on 24-bit grey: the quick brown fox \x1b[0m\r\n",
    "default colors: jumps over the lazy dog. #@%&*",
)
.as_bytes();

/// The colors of the ramp's eight bands, one band to a row of 16 pixels.
const BANDS: [(u32, u32, u32); 8] = [
    (204, 0, 0),
    (0, 204, 0),
    (0, 0, 204),
    (255, 255, 255),
    (255, 128, 0),
    (30, 60, 90),
    (128, 0, 255),
    (0, 0, 0),
];

/// `(r, g, b)` at alpha `a`, premultiplied and rounded.
fn premultiplied(a: u32, (r, g, b): (u32, u32, u32)) -> u32 {
    let c = |v: u32| (v * a + 127) / 255;
    a << 24 | c(r) << 16 | c(g) << 8 | c(b)
}

/// 256x128: alpha `x` across, so every alpha from clear to opaque, in a
/// band of one color for each row of 16 pixels.
fn across(x: u32, y: u32) -> u32 {
    premultiplied(x, BANDS[(y / 16) as usize])
}

/// 64x64: alpha `4 * y` down, in colors that change across.
fn down(x: u32, y: u32) -> u32 {
    premultiplied(y * 4, (x * 4, 255 - x * 4, 128))
}

/// The ramp across at its own size above the text; the ramp down above
/// it, over its left part and the text; and a small ramp across, made
/// 64x32 and grown to 16 by 4 cells, below the text on the right.
fn ramps(f: &mut Feed) -> Result<(), String> {
    let pictures = [
        Kitty {
            id: 1,
            size: (256, 128),
            rule: across,
            at: (0, 0),
            cells: (0, 0),
            z: 1,
        },
        Kitty {
            id: 2,
            size: (64, 64),
            rule: down,
            at: (4, 3),
            cells: (0, 0),
            z: 2,
        },
        Kitty {
            id: 3,
            size: (64, 32),
            rule: across,
            at: (32, 2),
            cells: (16, 4),
            z: -1,
        },
    ];
    for p in &pictures {
        f.bytes(&p.bytes())?;
    }
    Ok(())
}

const SCREEN: Screen = Screen {
    pictures: Some(ramps),
    ..Screen::new(
        "linear-blend-ramps",
        "Kitty pictures whose alpha ramps from clear to opaque, over several backgrounds, over and under text and over another picture, each partly transparent pixel blended in linear light.",
        TEXT,
        COLS,
        ROWS,
    )
};

// ADR-1300 D7's example on the CPU path's drawing, as a check that the
// screen draws the ramp blended in linear light: red 204 at alpha 17 over
// the black first row shows as red 58, where a blend in sRGB shows 14.
#[test]
fn the_ramp_draws_red_204_at_alpha_17_over_black_as_58() {
    let cpu = vtium_tier2::draw::draw(&SCREEN).unwrap();
    let w = cpu.width() as usize;
    let at = |x: usize, y: usize| &cpu.rgb()[(y * w + x) * 3..(y * w + x) * 3 + 3];
    assert_eq!(at(17, 0), [58, 0, 0]);
    assert_eq!(at(0, 0), [0, 0, 0]);
    assert_eq!(at(255, 0), [204, 0, 0]);
}

// ADR-1300 D6 and D7: on llvmpipe, the GPU path's picture shader blends
// through the same tables in the same whole numbers, so every pixel of the
// screen is the CPU path's, with GPU scaling and with the CPU path's copy.
#[test]
fn partly_transparent_pictures_draw_the_cpu_paths_bytes_on_llvmpipe() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gpu-blend");
    fs::create_dir_all(root.join("gpu")).unwrap();
    fs::write(
        root.join(format!("gpu/{}.tol", SCREEN.name)),
        "max_delta=0\nmax_pixels=0\n",
    )
    .unwrap();
    let mut gl = gpu::llvmpipe().unwrap();
    let judged = gpu::judge(&SCREEN, &root, &mut gl).unwrap();
    assert_eq!(
        judged.comparison.verdict,
        Verdict::Pass,
        "GPU scaling: {:?}",
        judged.comparison
    );
    let judged = gpu::judge_copy(&SCREEN, &mut gl).unwrap();
    assert_eq!(
        judged.comparison.verdict,
        Verdict::Pass,
        "the CPU copy: {:?}",
        judged.comparison
    );
}
