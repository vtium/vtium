use super::*;

/// A picture whose every pixel is opaque and differs from its
/// neighbours.
fn picture(w: u32, h: u32) -> Vec<u32> {
    (0..h)
        .flat_map(|y| (0..w).map(move |x| 0xff00_0000 | (x & 0xff) << 16 | (y & 0xff) << 8 | 0x40))
        .collect()
}

fn form(fit: Fit) -> Form {
    Form {
        fit,
        rows: None,
        position: None,
        scale: SCALE_FULL,
    }
}

const SCREEN: Geometry = Geometry {
    display: (80, 48),
    cell: (8, 16),
    grid: (10, 3),
    origin: (0, 0),
};

#[test]
fn a_background_made_at_the_displays_size_draws_one_to_one_under_cover() {
    let src = picture(80, 48);
    let made = make(&form(Fit::Cover), &SCREEN, (80, 48), &src).expect("made");
    assert_eq!((made.width, made.height), (80, 48));
    assert_eq!(made.pixels.as_slice(), &src[..]);
    assert!(made.opaque);
    assert_eq!(
        (made.place.cols, made.place.rows, made.place.z),
        (10, 3, i32::MIN)
    );
    assert_eq!(made.prompt_row, None);
}

#[test]
fn a_background_keeps_the_part_that_falls_on_the_grid() {
    // a display of 84x50 with a grid of 80x48 on it, centered across
    // with 2 pixel columns before it, and its last row on the display's
    // bottom edge with the 2 spare pixel rows above it: the picture of
    // 84x50 at one to one, cut to the grid, so the grid's first row
    // shows the picture's third
    let src = picture(84, 50);
    let geo = Geometry {
        display: (84, 50),
        origin: (2, 2),
        ..SCREEN
    };
    let made = make(&form(Fit::Cover), &geo, (84, 50), &src).expect("made");
    assert_eq!((made.width, made.height), (80, 48));
    for y in 0..48 {
        for x in 0..80 {
            assert_eq!(
                made.pixels.as_slice()[y * 80 + x],
                src[(y + 2) * 84 + x + 2]
            );
        }
    }
}

#[test]
fn contain_leaves_transparent_bands_and_cover_crops() {
    // a square picture on a display twice as wide as tall
    let src = vec![0xffff_0000; 16 * 16];
    let made = make(&form(Fit::Contain), &SCREEN, (16, 16), &src).expect("made");
    // 48 tall, so 48 wide, centered: 16 transparent columns each side,
    // outside the box of columns 2 to 7
    let at = |x| shown(&made, SCREEN.cell, (x, 0));
    assert_eq!((at(0), at(15)), (0, 0));
    assert_eq!((at(16), at(63)), (0xffff_0000, 0xffff_0000));
    assert_eq!((at(64), at(79)), (0, 0));
    assert_eq!((made.place.col, made.place.cols), (2, 6));
    let made = make(&form(Fit::Cover), &SCREEN, (16, 16), &src).expect("made");
    assert!(made.opaque);
    assert_eq!((made.place.cols, made.place.rows), (10, 3));
}

#[test]
fn stretch_fills_and_none_centres_at_its_own_size() {
    let src = vec![0xff00_ff00; 4 * 4];
    let made = make(&form(Fit::Stretch), &SCREEN, (4, 4), &src).expect("made");
    assert!(made.opaque);
    let made = make(&form(Fit::Natural), &SCREEN, (4, 4), &src).expect("made");
    // 4x4 centered on 80x48: columns 38 to 41, rows 22 to 25
    let at = |x, y| shown(&made, SCREEN.cell, (x, y));
    assert_eq!(at(37, 22), 0);
    assert_eq!(at(38, 22), 0xff00_ff00);
    assert_eq!(at(41, 25), 0xff00_ff00);
    assert_eq!(at(42, 25), 0);
    assert_eq!(at(38, 26), 0);
}

// The picture lines up with the grid where the renderer placed it. On
// a display turned left with 3 spare columns and 5 spare lines in its
// own pixels, the renderer centres a 6x2 grid of 8x16 cells with the
// odd line at the display's bottom, which is the text's left: the grid
// starts 3 pixels in from the text's left and 1 below its top. A 4x4
// picture centred on the display starts at 24,15 as the text reads,
// so at 21,14 on the grid.
//
// References:
// - ADR-1310 D12, "With smooth scrolling off": the grid centred on both
//   axes in the display's own pixels.
// - ADR-1100 D17: the login picture fitted to the grid on the display.
#[test]
fn a_picture_starts_from_the_origin_the_renderer_gives() {
    let turned_left = Geometry {
        display: (53, 35),
        cell: (8, 16),
        grid: (6, 2),
        origin: (3, 1),
    };
    let src = vec![0xff00_ff00; 4 * 4];
    let made = make(&form(Fit::Natural), &turned_left, (4, 4), &src).expect("made");
    let at = |x, y| shown(&made, turned_left.cell, (x, y));
    assert_eq!(at(21, 14), 0xff00_ff00);
    assert_eq!(at(24, 17), 0xff00_ff00);
    assert_eq!((at(20, 14), at(21, 13)), (0, 0));
    assert_eq!((at(25, 17), at(24, 18)), (0, 0));
}

#[test]
fn downscaling_averages_and_keeps_colors_within_alpha() {
    // black and half-transparent white, alternating: each 2x2 square
    // averages to the same pixel
    let src: Vec<u32> = (0..16)
        .map(|i| {
            if (i + i / 4) % 2 == 0 {
                0xff00_0000
            } else {
                0x8080_8080
            }
        })
        .collect();
    let out = resample(Source::Argb(&src), (4, 4), (2, 2), (0, 0), (2, 2)).expect("resampled");
    // alpha (255 + 128) / 2 = 191.5, color 64
    assert_eq!(out, vec![0xc040_4040; 4]);
    for p in out {
        let [a, r, g, b] = channels(p);
        assert!(r <= a && g <= a && b <= a);
    }
}

#[test]
fn a_banner_takes_its_own_size_and_the_prompt_starts_below_it() {
    let src = picture(20, 20);
    let geo = Geometry {
        grid: (10, 5),
        ..SCREEN
    };
    let made = make(&form(Fit::Banner), &geo, (20, 20), &src).expect("made");
    assert_eq!((made.width, made.height), (20, 20));
    assert_eq!(made.pixels.as_slice(), &src[..]);
    // 20 pixels of 16-pixel rows: 2 rows
    assert_eq!(made.prompt_row, Some(2));
    assert_eq!(
        (made.place.col, made.place.cols, made.place.rows),
        (0, 0, 0)
    );
}

#[test]
fn a_banner_of_n_rows_and_one_too_large_for_the_grid() {
    let src = picture(40, 20);
    let geo = Geometry {
        grid: (80, 10),
        ..SCREEN
    };
    let rows = Form {
        rows: Some(3),
        ..form(Fit::Banner)
    };
    let made = make(&rows, &geo, (40, 20), &src).expect("made");
    assert_eq!((made.width, made.height), (96, 48));
    assert_eq!(made.prompt_row, Some(3));
    // 20 rows asked of a grid of 10: scaled down to 8 rows, 128 pixels
    let tall = Form {
        rows: Some(20),
        ..form(Fit::Banner)
    };
    let made = make(&tall, &geo, (40, 20), &src).expect("made");
    assert_eq!((made.width, made.height), (256, 128));
    assert_eq!(made.prompt_row, Some(8));
    // a grid of 2 rows has no room for a banner
    let flat = Geometry {
        grid: (80, 2),
        ..SCREEN
    };
    assert_eq!(make(&form(Fit::Banner), &flat, (40, 20), &src), None);
}

#[test]
fn a_centred_banner_leaves_the_odd_column_on_its_right() {
    // 9 columns of 8 pixels on a grid of 80: 71 spare, 35 on the left
    let src = picture(72, 16);
    let geo = Geometry {
        display: (640, 160),
        cell: (8, 16),
        grid: (80, 10),
        origin: (0, 0),
    };
    let at = |position| {
        let f = Form {
            position: Some(position),
            ..form(Fit::Banner)
        };
        make(&f, &geo, (72, 16), &src).expect("made").place.col
    };
    assert_eq!(at(Position::Left), 0);
    assert_eq!(at(Position::Center), 35);
    assert_eq!(at(Position::Right), 71);
}

#[test]
fn a_picture_of_the_wrong_length_or_no_room_is_refused() {
    assert_eq!(make(&form(Fit::Cover), &SCREEN, (4, 4), &[0; 15]), None);
    let none = Geometry {
        grid: (0, 3),
        ..SCREEN
    };
    assert_eq!(make(&form(Fit::Cover), &none, (4, 4), &[0; 16]), None);
}

// The console checks the image helper's answer before it reads a
// pixel: at a scale of 100, cover and stretch fill the grid and contain
// fills one side of it, so a picture the helper did not fit is refused.
#[test]
fn a_size_the_fit_does_not_make_is_refused_before_a_pixel_is_read() {
    let fitting = |fit, scale| Fitting {
        form: Form { scale, ..form(fit) },
        geometry: SCREEN,
    };
    for fit in [Fit::Cover, Fit::Stretch, Fit::Contain] {
        assert_eq!(place(&fitting(fit, SCALE_FULL), (8, 8)), None, "{fit:?}");
        assert!(place(&fitting(fit, SCALE_FULL), (80, 48)).is_some());
        assert!(place(&fitting(fit, 50), (8, 8)).is_some(), "{fit:?}");
    }
    assert!(place(&fitting(Fit::Contain, SCALE_FULL), (40, 48)).is_some());
    assert!(place(&fitting(Fit::Natural, SCALE_FULL), (8, 8)).is_some());
    // nothing larger than the grid, and nothing empty
    assert_eq!(place(&fitting(Fit::Natural, SCALE_FULL), (81, 8)), None);
    assert_eq!(place(&fitting(Fit::Natural, SCALE_FULL), (0, 8)), None);
}

/// A small pseudo-random number generator, xorshift64, so a sweep is
/// the same on every run.
struct Dice(u64);

impl Dice {
    /// A number from `lo` to `hi`, both included.
    fn roll(&mut self, lo: u32, hi: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        lo + (self.0 % u64::from(hi - lo + 1)) as u32
    }
}

// The image helper runs `fit`, and the console checks the helper's
// answer with `place` and `Spot::made`. Over random screens, forms and
// sizes, nothing panics, `room` says exactly when `fit` makes a
// picture, every size `fit` makes passes the console's checks, and
// `place` refuses any other size or gives a box that holds it.
#[test]
fn fit_place_and_made_agree_over_random_screens_and_forms() {
    let fits = [
        Fit::Contain,
        Fit::Cover,
        Fit::Stretch,
        Fit::Natural,
        Fit::Banner,
    ];
    let mut dice = Dice(0x9e37_79b9_7f4a_7c15);
    for case in 0..4000 {
        let cell = (dice.roll(0, 12), dice.roll(0, 20));
        let grid = (dice.roll(0, 24), dice.roll(0, 12));
        // the origins the renderer gives: centred, its odd pixel on
        // either side as the display's turn puts it, or down, flush to
        // the text's bottom edge
        let spare = (dice.roll(0, 12), dice.roll(0, 12));
        let geometry = Geometry {
            display: (grid.0 * cell.0 + spare.0, grid.1 * cell.1 + spare.1),
            cell,
            grid,
            origin: (
                [spare.0 / 2, spare.0.div_ceil(2)][dice.roll(0, 1) as usize],
                [spare.1 / 2, spare.1.div_ceil(2), spare.1][dice.roll(0, 2) as usize],
            ),
        };
        let position = match dice.roll(0, 9) {
            9 => None,
            n => Position::ALL.get(n as usize).copied(),
        };
        let form = Form {
            fit: fits[dice.roll(0, 4) as usize],
            rows: [None, Some(dice.roll(0, 6))][dice.roll(0, 1) as usize],
            position,
            scale: [SCALE_FULL, dice.roll(1, 100) as u8][dice.roll(0, 1) as usize],
        };
        let fitting = Fitting { form, geometry };
        let size = (dice.roll(1, 64), dice.roll(1, 64));
        let src = picture(size.0, size.1);
        let made = fit(&fitting, size, Source::Argb(&src));
        assert_eq!(made.is_some(), room(&fitting), "case {case}: {fitting:?}");
        if let Some((w, h, pixels)) = made {
            let spot = place(&fitting, (w, h));
            assert!(spot.is_some(), "case {case}: {fitting:?} refuses {w}×{h}");
            let made = spot.and_then(|spot| spot.made((w, h), pixels, true));
            assert!(made.is_some(), "case {case}: {fitting:?}");
        }
        let other = (dice.roll(0, 300), dice.roll(0, 300));
        if let Some(spot) = place(&fitting, other) {
            assert!(
                spot.inset.0 + other.0 <= spot.frame.0 && spot.inset.1 + other.1 <= spot.frame.1,
                "case {case}: {fitting:?} puts {other:?} outside its box"
            );
            let pixels = vec![0xff00_0000; (other.0 * other.1) as usize];
            assert!(spot.made(other, pixels, true).is_some(), "case {case}");
        }
    }
}

// A screen has room for a background when its grid has a cell, and
// for a banner when it also has more than 2 rows.
#[test]
fn room_needs_a_cell_and_a_banner_needs_three_rows() {
    let fitting = |fit, grid, cell| Fitting {
        form: form(fit),
        geometry: Geometry {
            grid,
            cell,
            ..SCREEN
        },
    };
    assert!(room(&fitting(Fit::Cover, (10, 3), (8, 16))));
    assert!(room(&fitting(Fit::Banner, (10, 3), (8, 16))));
    assert!(room(&fitting(Fit::Cover, (10, 2), (8, 16))));
    assert!(!room(&fitting(Fit::Banner, (10, 2), (8, 16))));
    assert!(!room(&fitting(Fit::Cover, (0, 3), (8, 16))));
    assert!(!room(&fitting(Fit::Contain, (10, 3), (8, 0))));
}

#[test]
fn the_words_of_the_keys() {
    assert_eq!(Fit::parse("none"), Some(Fit::Natural));
    assert_eq!(Fit::parse("banner"), Some(Fit::Banner));
    assert_eq!(Fit::parse("fill"), None);
    assert_eq!(Position::parse("center"), Some(Position::Center));
    for p in Position::ALL {
        assert_eq!(Position::parse(p.word()), Some(p));
    }
    for word in ["middle", "top-center", "30%", "Top", ""] {
        assert_eq!(Position::parse(word), None, "{word:?}");
    }
}

/// The form of a background under `fit` at `position` and `scale`.
fn placed(fit: Fit, position: Option<Position>, scale: u8) -> Form {
    Form {
        fit,
        rows: None,
        position,
        scale,
    }
}

/// The pixel of `made` at `(x, y)` on the grid, in pixels from the
/// grid's top left; `None` outside its box.
fn on_grid(made: &Made, cell: (u32, u32), (x, y): (u32, u32)) -> Option<u32> {
    let x0 = made.place.col * cell.0;
    let y0 = made.place.row * cell.1;
    let (bx, by) = (x.checked_sub(x0)?, y.checked_sub(y0)?);
    if bx >= made.width || by >= made.height {
        return None;
    }
    made.pixels
        .as_slice()
        .get(usize::try_from(by * made.width + bx).ok()?)
        .copied()
}

/// The pixel of `made` the grid shows at `(x, y)`: transparent outside
/// its box.
fn shown(made: &Made, cell: (u32, u32), at: (u32, u32)) -> u32 {
    on_grid(made, cell, at).unwrap_or(0)
}

/// The opaque pixels of `made` as a rectangle on the grid: its left,
/// top, width and height in pixels.
fn picture_on_grid(made: &Made, cell: (u32, u32)) -> (u32, u32, u32, u32) {
    let w = made.width;
    let opaque: Vec<(u32, u32)> = (0..made.height)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| made.pixels.as_slice()[(y * w + x) as usize] >> 24 != 0)
        .collect();
    let left = opaque.iter().map(|p| p.0).min().expect("a pixel");
    let right = opaque.iter().map(|p| p.0).max().expect("a pixel");
    let top = opaque.iter().map(|p| p.1).min().expect("a pixel");
    let bottom = opaque.iter().map(|p| p.1).max().expect("a pixel");
    // the opaque pixels fill their rectangle
    assert_eq!(
        opaque.len() as u32,
        (right - left + 1) * (bottom - top + 1),
        "a picture with a hole"
    );
    (
        made.place.col * cell.0 + left,
        made.place.row * cell.1 + top,
        right - left + 1,
        bottom - top + 1,
    )
}

/// A picture whose pixel at `(x, y)` holds `x` in its red byte and `y`
/// in its green byte, opaque.
fn coordinates(w: u32, h: u32) -> Vec<u32> {
    (0..h)
        .flat_map(|y| (0..w).map(move |x| 0xff00_0000 | x << 16 | y << 8))
        .collect()
}

// A display of 100×60 pixels with a grid of 12×5 cells of 7×10 pixels
// on it, centered across and its last row on the display's bottom
// edge: the grid starts 8 pixels in and 10 pixels down.
const ROOMY: Geometry = Geometry {
    display: (100, 60),
    cell: (7, 10),
    grid: (12, 5),
    origin: (8, 10),
};

// A picture of 15×30 under contain on the display of 100×60 scales
// twice over to 30×60, which copies each source pixel to 2×2. It is 10
// pixels taller than the grid, so the anchor's row chooses which 50
// rows show, and it leaves 54 pixels of room across, which the
// anchor's column places it in.
//
// References:
// - ADR-1100 D17, "Where the picture sits": how each anchor places a
//   background.
#[test]
fn each_anchor_places_a_contained_picture_at_the_offset_worked_out_by_hand() {
    let src = coordinates(15, 30);
    for position in Position::ALL {
        let made = make(
            &placed(Fit::Contain, Some(position), SCALE_FULL),
            &ROOMY,
            (15, 30),
            &src,
        )
        .expect("made");
        // across: against the grid's left edge; centered on the
        // display, (100 - 30) / 2 = 35 less the grid's 8; against the
        // grid's right edge, 84 - 30
        let x = match position {
            Position::TopLeft | Position::Left | Position::BottomLeft => 0,
            Position::Top | Position::Center | Position::Bottom => 27,
            _ => 54,
        };
        // down: the first scaled row that shows, 0 at the top; centered,
        // the picture starts at the display's top, 10 rows above the
        // grid, and at the bottom its last row is the grid's last, so
        // 10 rows of it lie above the grid too
        let first_row = match position {
            Position::TopLeft | Position::Top | Position::TopRight => 0,
            _ => 10,
        };
        let at = picture_on_grid(&made, ROOMY.cell);
        assert_eq!(at, (x, 0, 30, 50), "{position:?}");
        let corner = on_grid(&made, ROOMY.cell, (x, 0)).expect("the corner");
        assert_eq!(
            corner,
            0xff00_0000 | (first_row / 2) << 8,
            "{position:?}: the top left shows source row {}",
            first_row / 2
        );
    }
}

// A picture shorter than the grid, centered on the display, can reach
// into the spare pixel rows above the grid, and the grid shows only the
// rest of it. At its own size, 20×46 on ROOMY's display of 100×60, it
// spans the display's rows 7 to 52, and the grid starts at row 10, so
// the grid's first row shows the picture's row 3 and its last 43 rows
// show. Across, it starts at (100 - 20) / 2 = 40, the grid's 32.
//
// References:
// - ADR-1100 D17, "Where the picture sits": a side the anchor does not
//   name centers the picture on the display.
// - ADR-1310 D10: the grid's last row on the display's bottom edge.
#[test]
fn a_centered_picture_shorter_than_the_grid_loses_its_rows_above_the_grid() {
    let src = coordinates(20, 46);
    let form = placed(Fit::Natural, None, SCALE_FULL);
    // the image helper hands back only the rows that show
    let fitting = Fitting {
        form,
        geometry: ROOMY,
    };
    let (w, h, _) = fit(&fitting, (20, 46), Source::Argb(&src)).expect("fitted");
    assert_eq!((w, h), (20, 43));
    let made = make(&form, &ROOMY, (20, 46), &src).expect("made");
    assert_eq!(picture_on_grid(&made, ROOMY.cell), (32, 0, 20, 43));
    assert_eq!(
        on_grid(&made, ROOMY.cell, (32, 0)),
        Some(0xff00_0000 | 3 << 8)
    );
    assert_eq!(
        on_grid(&made, ROOMY.cell, (51, 42)),
        Some(0xff00_0000 | 19 << 16 | 45 << 8)
    );
}

// Unset, a background is centered, which is where `center` puts it.
#[test]
fn an_unset_anchor_centers_a_background() {
    let src = coordinates(15, 30);
    let make_at = |position| {
        make(
            &placed(Fit::Contain, position, SCALE_FULL),
            &ROOMY,
            (15, 30),
            &src,
        )
        .expect("made")
    };
    assert_eq!(make_at(None), make_at(Some(Position::Center)));
}

// Under cover at `top`, the picture's top edge lies on the grid's top
// edge, so the source's top row shows on the grid's first pixel row and
// the bottom is cut.
#[test]
fn cover_at_top_keeps_the_sources_top_row() {
    // a square, red in its top row and blue below: cover scales it to
    // 100×100, so its first 6 or 7 rows come from the red row alone
    let src: Vec<u32> = (0..15 * 15)
        .map(|i| if i < 15 { 0xffff_0000 } else { 0xff00_00ff })
        .collect();
    let top = make(
        &placed(Fit::Cover, Some(Position::Top), SCALE_FULL),
        &ROOMY,
        (15, 15),
        &src,
    )
    .expect("made");
    for x in 0..84 {
        assert_eq!(on_grid(&top, ROOMY.cell, (x, 0)), Some(0xffff_0000), "{x}");
    }
    let center = make(
        &placed(Fit::Cover, Some(Position::Center), SCALE_FULL),
        &ROOMY,
        (15, 15),
        &src,
    )
    .expect("made");
    assert_eq!(on_grid(&center, ROOMY.cell, (0, 0)), Some(0xff00_00ff));
}

// A logo of 1000×1000 on a display of 1920×1080 with 8×16 cells,
// contained at 1080×1080 and scaled to 30%, at `bottom-right`.
//
// References:
// - ADR-1100 D17, "A background at a share of its size": this example,
//   with its size and its box.
#[test]
fn a_scaled_logo_in_a_corner_has_the_size_and_box_worked_out_by_hand() {
    let hd = Geometry {
        display: (1920, 1080),
        cell: (8, 16),
        grid: (240, 67),
        origin: (0, 8),
    };
    let src = vec![0xff40_8020; 1000 * 1000];
    let made = make(
        &placed(Fit::Contain, Some(Position::BottomRight), 30),
        &hd,
        (1000, 1000),
        &src,
    )
    .expect("made");
    // 324×324, its right and bottom edges on the grid's, 1920 and 1072
    assert_eq!(picture_on_grid(&made, hd.cell), (1596, 748, 324, 324));
    // the box: columns 199 to 239 and rows 46 to 66
    assert_eq!(
        (
            made.place.col,
            made.place.row,
            made.place.cols,
            made.place.rows
        ),
        (199, 46, 41, 21)
    );
    assert_eq!((made.width, made.height), (328, 336));
    assert_eq!(made.cost(), 441_344);
    assert!(!made.opaque);
    // under none at 50, a photo of 3840×2160 draws at 1920×1080: on the
    // grid of 1920×1072, centered, that is the whole grid
    let photo = vec![0xff10_2030; 3840 * 2160];
    let made = make(&placed(Fit::Natural, None, 50), &hd, (3840, 2160), &photo).expect("made");
    assert_eq!(picture_on_grid(&made, hd.cell), (0, 0, 1920, 1072));
}

// No scale makes a picture larger than its fit, and each side keeps at
// least one pixel.
#[test]
fn no_scale_makes_a_picture_larger_than_its_fit() {
    for fit in [Fit::Contain, Fit::Cover, Fit::Stretch, Fit::Natural] {
        for size in [(1, 1), (3, 7), (1000, 10), (97, 61)] {
            let full = fitted(fit, size, ROOMY.display).expect("fitted");
            for scale in 1..=100 {
                let (w, h) = shrunk(full, scale);
                assert!(w <= full.0 && h <= full.1, "{fit:?} {size:?} at {scale}");
                assert!(w >= 1 && h >= 1, "{fit:?} {size:?} at {scale}");
            }
            assert_eq!(shrunk(full, 100), full);
            // a scale past 100 counts as 100
            assert_eq!(shrunk(full, 101), full);
            assert_eq!(shrunk(full, 255), full);
        }
    }
    // 30% of 1080, rounded to the nearest pixel, and 1% of 3 pixels
    assert_eq!(shrunk((1080, 1080), 30), (324, 324));
    assert_eq!(shrunk((3, 150), 1), (1, 2));
}

// The box is the cells the picture touches: a picture of 5×3 pixels
// under none on cells of 4×4, at the grid's right edge, starts in the
// middle of a cell, so its box is the two cells it touches across and
// the one down, and the pixels of the box outside it are transparent.
#[test]
fn the_box_is_the_cells_the_picture_touches_and_clear_outside_it() {
    let geo = Geometry {
        display: (40, 20),
        cell: (4, 4),
        grid: (10, 5),
        origin: (0, 0),
    };
    let src = vec![0xffff_ffff; 5 * 3];
    for position in Position::ALL {
        let made = make(
            &placed(Fit::Natural, Some(position), SCALE_FULL),
            &geo,
            (5, 3),
            &src,
        )
        .expect("made");
        let (x, y, w, h) = picture_on_grid(&made, geo.cell);
        assert_eq!((w, h), (5, 3), "{position:?}");
        let cols = (x + w).div_ceil(4) - x / 4;
        let rows = (y + h).div_ceil(4) - y / 4;
        assert_eq!(
            (
                made.place.col,
                made.place.row,
                made.place.cols,
                made.place.rows
            ),
            (x / 4, y / 4, cols, rows),
            "{position:?}"
        );
        assert_eq!((made.width, made.height), (cols * 4, rows * 4));
        // every pixel outside the picture is transparent
        let clear = made.pixels.as_slice().iter().filter(|&&p| p == 0).count();
        assert_eq!(clear as u32, cols * rows * 16 - 15, "{position:?}");
    }
}

/// A banner of 20×20 pixels on a grid of 10×6 cells of 8×16 at
/// `position`: its placement and the row the login prompt starts on.
fn banner_at(position: Option<Position>, size: (u32, u32)) -> (Place, Option<u32>) {
    let geo = Geometry {
        display: (80, 96),
        cell: (8, 16),
        grid: (10, 6),
        origin: (0, 0),
    };
    let f = Form {
        position,
        ..form(Fit::Banner)
    };
    let made = make(&f, &geo, size, &picture(size.0, size.1)).expect("made");
    (made.place, made.prompt_row)
}

// A bottom anchor puts the banner's box in the grid's last rows, and
// the login prompt starts on the first row.
//
// References:
// - ADR-1100 D17, "A banner at the bottom": where the banner and the
//   prompt go.
#[test]
fn a_bottom_banner_takes_the_last_rows_and_the_prompt_starts_at_the_top() {
    // 20 pixels of 16-pixel rows: 2 rows, rows 4 and 5 of 6; 3 columns
    let (place, prompt) = banner_at(Some(Position::BottomRight), (20, 20));
    assert_eq!((place.col, place.row, place.z), (7, 4, Z));
    assert_eq!(prompt, Some(0));
    let (place, _) = banner_at(Some(Position::BottomLeft), (20, 20));
    assert_eq!((place.col, place.row), (0, 4));
    let (place, _) = banner_at(Some(Position::Bottom), (20, 20));
    assert_eq!((place.col, place.row), (3, 4));
    // a banner taller than the grid keeps two rows free above it
    let (place, prompt) = banner_at(Some(Position::Bottom), (40, 400));
    assert_eq!((place.row, prompt), (2, Some(0)));
}

// `left`, `center` and `right` name no row, so for a banner they mean
// the top, as do the top anchors and no anchor at all.
#[test]
fn a_banner_without_a_bottom_anchor_sits_at_the_top() {
    for (position, col) in [
        (None, 0),
        (Some(Position::Left), 0),
        (Some(Position::Center), 3),
        (Some(Position::Right), 7),
        (Some(Position::TopLeft), 0),
        (Some(Position::Top), 3),
        (Some(Position::TopRight), 7),
    ] {
        let (place, prompt) = banner_at(position, (20, 20));
        assert_eq!((place.col, place.row), (col, 0), "{position:?}");
        assert_eq!(prompt, Some(2), "{position:?}");
    }
}

// On a display turned a quarter turn, the geometry is the screen as
// the text reads on it, so `bottom-right` puts the box at the end of
// the grid's last row, wherever that falls on the panel. A panel of
// 96×80 turned a quarter reads as 80×96, a grid of 10×6 cells.
//
// References:
// - ADR-1100 D17, "Where the picture sits": the anchors on a rotated
//   display.
#[test]
fn on_a_turned_display_bottom_right_ends_the_last_row() {
    let turned = Geometry {
        display: (80, 96),
        cell: (8, 16),
        grid: (10, 6),
        origin: (0, 0),
    };
    let src = vec![0xff00_ff00; 20 * 20];
    let made = make(
        &placed(Fit::Natural, Some(Position::BottomRight), SCALE_FULL),
        &turned,
        (20, 20),
        &src,
    )
    .expect("made");
    assert_eq!(picture_on_grid(&made, turned.cell), (60, 76, 20, 20));
    assert_eq!(
        (
            made.place.col + made.place.cols,
            made.place.row + made.place.rows
        ),
        (10, 6)
    );
}
