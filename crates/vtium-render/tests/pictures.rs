//! Pictures in the CPU compositor (ADR-1100 D4, D5, D8, D9; issue #30):
//! the layers a picture draws in against text, the cursor, the selection
//! and the pointer; scaling and turning; the scaled-copy cache, counted
//! through its own counters; the damage a picture causes; and pictures cut
//! at the grid's edge, malformed or huge. Every picture is made through the
//! image store directly, as no protocol handler is involved.
#![cfg(feature = "images")]

mod common;

use common::{Screen, WriteOnly, arrow_over, psf, rgb, style, unifont};
use vtium_font::{Face, Font};
use vtium_image::{
    FramePlacement, ImageKey, LineKey, NewImage, Pixels, Place, Protocol, Rows, Screens, Store,
};
use vtium_render::{
    Canvas, Config, DrawState, Orientation, PICTURE_CACHE_SCREENS, PictureCounts, Renderer, Target,
};
use vtium_term::{Age, CursorShape, Style};

const BG: u32 = 0x20_00_00;
const BLUE: u32 = 0xff00_30c0;

/// A console's lines and its store. Screen row `r` is line `r + 1`, as
/// the test screens number their lines, and the lines below the screen
/// scroll in as the view moves down.
struct Console {
    store: Store,
    keys: Vec<LineKey>,
    cols: usize,
    rows: usize,
}

impl Console {
    fn new(cols: usize, rows: usize, more: usize) -> Console {
        let mut store = Store::new();
        store.set_limit(1 << 30);
        Console {
            store,
            keys: (1..=(rows + more) as u64).map(LineKey::new).collect(),
            cols,
            rows,
        }
    }

    /// The view `top` lines down: its lines, and those above it, newest
    /// first.
    fn view(&self, top: usize) -> (Vec<LineKey>, Vec<LineKey>) {
        let screen = self.keys[top..top + self.rows].to_vec();
        let above = self.keys[..top].iter().rev().copied().collect();
        (screen, above)
    }

    fn image(
        &mut self,
        protocol: Protocol,
        id: u32,
        (w, h): (u32, u32),
        px: impl Fn(u32, u32) -> u32,
    ) -> ImageKey {
        let (screen, above) = self.view(0);
        let rows = Rows {
            screen: &screen,
            above: &above,
        };
        let pixels = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| px(x, y))
            .collect();
        let new = NewImage {
            protocol,
            width: w,
            height: h,
            pixels,
            id,
            number: 0,
            transient: false,
        };
        let screens = Screens {
            main: rows,
            alt: Rows::default(),
        };
        self.store
            .add_image(new, None, screens)
            .expect("the store takes the image")
    }

    fn place(&mut self, image: ImageKey, place: Place) {
        let (screen, above) = self.view(0);
        let screens = Screens {
            main: Rows {
                screen: &screen,
                above: &above,
            },
            alt: Rows::default(),
        };
        self.store
            .place(image, place, screens, self.cols)
            .expect("the store places it");
    }

    /// Text written over cells `x0` to `x1` of screen row `y`, as the
    /// terminal tells the store.
    fn cut(&mut self, y: usize, x0: usize, x1: usize) {
        let (screen, above) = self.view(0);
        let rows = Rows {
            screen: &screen,
            above: &above,
        };
        self.store.cut(y, x0, x1, rows, self.cols);
        self.store.settle_event(Screens {
            main: rows,
            alt: Rows::default(),
        });
    }

    /// The frame's placements with the view `top` lines down.
    fn frame(&self, top: usize) -> Vec<FramePlacement> {
        let (screen, above) = self.view(top);
        self.store.frame(
            Rows {
                screen: &screen,
                above: &above,
            },
            self.cols,
        )
    }
}

fn solid(c: u32) -> impl Fn(u32, u32) -> u32 {
    move |_, _| c
}

/// A picture whose every pixel differs from its neighbours, opaque.
fn ramp(x: u32, y: u32) -> u32 {
    0xff00_0000 | (x * 7 % 256) << 16 | (y * 5 % 256) << 8 | ((x + y) * 3 % 256)
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// `s` drawn whole on a `w` by `h` display, with `placements`.
fn draw_with<F: Face>(
    font: Font<F>,
    s: &Screen,
    placements: Vec<FramePlacement>,
    (w, h): (u32, u32),
    config: Config,
) -> (Canvas, PictureCounts) {
    let mut r = Renderer::new(font, w, h, config);
    let mut c = Canvas::new(w, h, 0);
    r.draw(
        &s.frame(Age::NEVER).with_placements(placements),
        &mut c,
        0,
        &state(s),
    );
    (c, r.picture_counts())
}

fn draw(s: &Screen, placements: Vec<FramePlacement>) -> Canvas {
    let size = (s.cols as u32 * 8, s.rows as u32 * 16);
    draw_with(unifont(), s, placements, size, Config::default()).0
}

/// The pixels of the cell at `col`, `row` in 8x16 cells, row by row.
fn cell(c: &Canvas, col: u32, row: u32) -> Vec<u32> {
    (0..16)
        .flat_map(|y| (0..8).map(move |x| (col * 8 + x, row * 16 + y)))
        .map(|(x, y)| c.pixel(x, y).unwrap())
        .collect()
}

fn screen() -> Screen {
    let mut s = Screen::new(10, 4, rgb("c0c0c0"), rgb("000000"));
    let st = style(rgb("ffffff"), rgb("200000"));
    s.text(0, 1, "XXXXXXXXXX", st);
    s
}

// covers: image.compositor
#[test]
fn a_picture_at_its_own_size_draws_its_own_pixels_and_nothing_else() {
    let s = screen();
    let mut con = Console::new(10, 4, 0);
    let img = con.image(Protocol::SIXEL, 0, (16, 32), ramp);
    con.place(
        img,
        Place {
            col: 2,
            row: 1,
            ..Place::default()
        },
    );
    let plain = draw(&s, Vec::new());
    let (c, counts) = draw_with(unifont(), &s, con.frame(0), (80, 64), Config::default());
    for y in 0..64 {
        for x in 0..80 {
            let inside = (16..32).contains(&x) && (16..48).contains(&y);
            let want = if inside {
                ramp(x - 16, y - 16) & 0xff_ffff
            } else {
                plain.pixel(x, y).unwrap()
            };
            assert_eq!(c.pixel(x, y), Some(want), "{x},{y}");
        }
    }
    // read straight from the image: nothing scaled, nothing kept
    assert_eq!(counts, PictureCounts::default());
}

// ADR-1100 D4: from the bottom, the layer below backgrounds, the cell
// backgrounds, the layer below text, the text, the layer above text
// covers: image.compositor
#[test]
fn text_draws_over_a_picture_below_text_and_under_one_above_it() {
    let mut s = screen();
    // a cell of the default background, beside the colored ones
    s.text(0, 2, "YY", Style::DEFAULT);
    let plain = draw(&s, Vec::new());
    // a letter's ink: the pixels unlike its cell's background
    let glyph = |px: &[u32], bg: u32| px.iter().map(|&p| p != bg).collect::<Vec<bool>>();
    for (z, layer) in [
        (-1, "below text"),
        (0, "above text"),
        (i32::MIN, "below backgrounds"),
    ] {
        let mut con = Console::new(10, 4, 0);
        let img = con.image(Protocol::KITTY, 1, (16, 32), solid(BLUE));
        con.place(
            img,
            Place {
                col: 0,
                row: 1,
                z,
                ..Place::default()
            },
        );
        let c = draw(&s, con.frame(0));
        // row 1 holds an X on a dark red background, row 2 a Y on the
        // default background
        for (col, row) in [(0, 1), (1, 1), (0, 2), (1, 2)] {
            let (got, was) = (cell(&c, col, row), cell(&plain, col, row));
            let ink = glyph(&was, if row == 1 { BG } else { 0 });
            assert!(ink.contains(&true), "the cell shows a letter");
            for (i, (&g, &w)) in got.iter().zip(&was).enumerate() {
                let want = match (layer, ink[i], row) {
                    ("above text", _, _) => BLUE & 0xff_ffff,
                    (_, true, _) => w,
                    ("below text", false, _) => BLUE & 0xff_ffff,
                    // the dark red background hides the lowest layer; the
                    // default background shows it
                    (_, false, 1) => w,
                    (_, false, _) => BLUE & 0xff_ffff,
                };
                assert_eq!(g, want, "z {z}, cell {col},{row}, pixel {i}");
            }
        }
        // the cells beside the picture are drawn as without it
        assert_eq!(cell(&c, 2, 1), cell(&plain, 2, 1), "z {z}");
    }
    // half transparent, below text and above it: the letter lies over the
    // first and under the second
    let half = 0x8000_0060;
    for (z, above) in [(-1, false), (0, true)] {
        let mut con = Console::new(10, 4, 0);
        let img = con.image(Protocol::KITTY, 1, (8, 16), solid(half));
        con.place(
            img,
            Place {
                col: 0,
                row: 1,
                z,
                ..Place::default()
            },
        );
        let c = draw(&s, con.frame(0));
        let (got, was) = (cell(&c, 0, 1), cell(&plain, 0, 1));
        for (i, (&g, &w)) in got.iter().zip(&was).enumerate() {
            let want = if above || w == BG {
                arrow_over(w, half)
            } else {
                w
            };
            assert_eq!(g, want, "z {z}, pixel {i}");
        }
    }
}

// covers: image.compositor
#[test]
fn a_picture_partly_transparent_goes_over_the_cell_as_drawn() {
    let s = screen();
    let half = 0x8000_0060; // half alpha, premultiplied blue
    let mut con = Console::new(10, 4, 0);
    let img = con.image(Protocol::SIXEL, 0, (8, 16), solid(half));
    con.place(
        img,
        Place {
            col: 3,
            row: 1,
            ..Place::default()
        },
    );
    let (c, plain) = (draw(&s, con.frame(0)), draw(&s, Vec::new()));
    for (&g, &w) in cell(&c, 3, 1).iter().zip(&cell(&plain, 3, 1)) {
        assert_eq!(g, arrow_over(w, half));
    }
}

// ADR-1100 D4: the cursor, the pointer and vtium's marks draw above every
// picture
// covers: image.compositor
#[test]
fn the_cursor_the_selection_and_the_pointer_draw_above_every_picture() {
    let mut s = Screen::new(10, 4, rgb("c0c0c0"), rgb("000000"));
    // blank cells of a dark red background, so a cursor's pixels are the
    // ones it changes
    s.text(0, 1, "          ", style(rgb("ffffff"), rgb("200000")));
    s.cursor.visible = true;
    s.cursor.row = 1;
    s.cursor.col = 4;
    let mut con = Console::new(10, 4, 0);
    let img = con.image(Protocol::SIXEL, 0, (80, 16), solid(BLUE));
    con.place(
        img,
        Place {
            col: 0,
            row: 1,
            ..Place::default()
        },
    );
    for shape in [CursorShape::Block, CursorShape::Bar, CursorShape::Underline] {
        s.cursor.shape = shape;
        let c = draw(&s, con.frame(0));
        let with = draw(&s, Vec::new());
        let mut bare = Screen::new(10, 4, rgb("c0c0c0"), rgb("000000"));
        bare.text(0, 1, "          ", style(rgb("ffffff"), rgb("200000")));
        let without = draw(&bare, Vec::new());
        let (got, mark, under) = (cell(&c, 4, 1), cell(&with, 4, 1), cell(&without, 4, 1));
        assert_ne!(mark, under, "{shape:?} shows");
        for (i, ((&g, &m), &u)) in got.iter().zip(&mark).zip(&under).enumerate() {
            let want = if m != u || shape == CursorShape::Block {
                m
            } else {
                BLUE & 0xff_ffff
            };
            assert_eq!(g, want, "{shape:?}, pixel {i}");
        }
        assert!(cell(&c, 3, 1).iter().all(|&p| p == BLUE & 0xff_ffff));
    }
    // the selection is vtium's mark too: its cells draw as selected
    s.cursor.visible = false;
    let frame = |p: Vec<FramePlacement>| {
        s.frame(Age::NEVER)
            .with_placements(p)
            .with_selection(vec![0..0, 2..4])
    };
    let mut r = Renderer::new(unifont(), 80, 64, Config::default());
    let mut c = Canvas::new(80, 64, 0);
    r.draw(&frame(con.frame(0)), &mut c, 0, &state(&s));
    let mut r = Renderer::new(unifont(), 80, 64, Config::default());
    let mut sel = Canvas::new(80, 64, 0);
    r.draw(&frame(Vec::new()), &mut sel, 0, &state(&s));
    for col in 2..4 {
        assert_eq!(cell(&c, col, 1), cell(&sel, col, 1), "selected cell {col}");
    }
    assert!(cell(&c, 4, 1).iter().all(|&p| p == BLUE & 0xff_ffff));
    // the pointer's arrow, blended as each line is written, lies over it
    let mut r = Renderer::new(unifont(), 80, 64, Config::default());
    let mut c = Canvas::new(80, 64, 0);
    let st = DrawState {
        pointer: Some((60, 20)),
        ..state(&s)
    };
    r.draw(
        &s.frame(Age::NEVER).with_placements(con.frame(0)),
        &mut c,
        0,
        &st,
    );
    let arrow = r.arrow().clone();
    let (hx, hy) = arrow.hot();
    let (ax, ay) = (60 - hx, 20 - hy);
    let mut drawn = 0;
    for y in 0..arrow.height() {
        for x in 0..arrow.width() {
            let a = arrow.pixels()[(y * arrow.width() + x) as usize];
            // the arrow's pixels over the picture's row
            if a >> 24 != 0 && (16..32).contains(&(ay + y)) && ax + x < 80 {
                assert_eq!(
                    c.pixel(ax + x, ay + y),
                    Some(arrow_over(BLUE & 0xff_ffff, a))
                );
                drawn += 1;
            }
        }
    }
    assert!(drawn > 0);
}

// covers: image.compositor
#[test]
fn a_picture_at_the_screen_edge_is_cut_at_the_grid() {
    // 10 by 4 cells of 8x16 on a display 5 pixels wider and 6 taller: a
    // border of 2 and 3 pixels left and above, 3 and 3 right and below
    let s = screen();
    let mut con = Console::new(10, 4, 0);
    let img = con.image(Protocol::SIXEL, 0, (40, 48), ramp);
    // five columns and three rows from column 8, row 2: past the right
    // edge and the bottom
    con.place(
        img,
        Place {
            col: 8,
            row: 2,
            ..Place::default()
        },
    );
    let (w, h) = (85, 70);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut t = WriteOnly::new(w, h);
    r.draw(
        &s.frame(Age::NEVER).with_placements(con.frame(0)),
        &mut t,
        0,
        &state(&s),
    );
    let c = t.into_canvas();
    let bg = s.palette.bg();
    let border = (bg.r as u32) << 16 | (bg.g as u32) << 8 | bg.b as u32;
    for y in 0..h {
        for x in 0..w {
            let in_grid = (2..82).contains(&x) && (3..67).contains(&y);
            let in_pic = (2 + 64..82).contains(&x) && (3 + 32..67).contains(&y);
            let px = c.pixel(x, y).unwrap();
            if !in_grid {
                assert_eq!(px, border, "border {x},{y}");
            } else if in_pic {
                assert_eq!(px, ramp(x - 66, y - 35) & 0xff_ffff, "{x},{y}");
            }
        }
    }
}

// covers: image.compositor
#[test]
fn malformed_zero_sized_and_huge_placements_draw_safely() {
    let s = screen();
    let mut con = Console::new(10, 4, 0);
    let img = con.image(Protocol::KITTY, 1, (64, 64), ramp);
    con.place(
        img,
        Place {
            col: 1,
            row: 1,
            ..Place::default()
        },
    );
    let good = con.frame(0);
    let plain = draw(&s, Vec::new());
    let bad = |f: &dyn Fn(&mut FramePlacement)| {
        let mut p = good.clone();
        f(&mut p[0]);
        p
    };
    let nothing: [&dyn Fn(&mut FramePlacement); 8] = [
        &|p| p.width = 0,
        &|p| p.height = 0,
        &|p| p.src_width = 0,
        &|p| p.cell_width = 0,
        &|p| p.cols = 0,
        &|p| p.pixels = Pixels::new(vec![0xffff_ffff; 3]),
        &|p| p.src_x = 60,
        &|p| p.row = -100,
    ];
    for (i, f) in nothing.iter().enumerate() {
        assert_eq!(draw(&s, bad(f)), plain, "case {i}");
    }
    // far past every side: nothing to draw, nothing to panic
    let far: [&dyn Fn(&mut FramePlacement); 5] = [
        &|p| p.col = u32::MAX,
        &|p| p.row = i64::MIN,
        &|p| p.row = i64::MAX,
        &|p| (p.x, p.width, p.cols) = (u32::MAX, u32::MAX, u32::MAX),
        &|p| (p.cell_width, p.cell_height) = (1, 1),
    ];
    for f in far {
        draw(&s, bad(f));
    }
    // ten thousand cells wide, the store's widest box, and five thousand
    // tall, so the picture fills it: drawn scaled where the grid shows it,
    // never kept, and the cache untouched
    let mut con = Console::new(10, 4, 0);
    let img = con.image(Protocol::KITTY, 1, (64, 64), ramp);
    con.place(
        img,
        Place {
            col: 0,
            row: 0,
            cols: 10_000,
            rows: 5_000,
            ..Place::default()
        },
    );
    let (c, counts) = draw_with(unifont(), &s, con.frame(0), (80, 64), Config::default());
    assert_eq!((counts.kept, counts.bytes, counts.copies), (0, 0, 0));
    assert!(counts.uncached >= 1);
    // the box is 80,000 pixels square, and the picture's top-left source
    // pixel alone fills the first 624 of them each way, the whole grid
    assert!(c.pixels().iter().all(|&p| p == ramp(0, 0) & 0xff_ffff));
}

// ADR-1100 D2 and D8: a cut leaves pieces of one picture, which a cell size
// change scales with their boxes; the pieces meet with no gap or overlap,
// and share the one copy of their picture
// covers: image.compositor
#[test]
fn the_pieces_of_a_cut_picture_meet_at_any_scale_and_share_one_copy() {
    let s = Screen::new(10, 4, rgb("c0c0c0"), rgb("000000"));
    let picture = |cut: bool| {
        let mut con = Console::new(10, 4, 0);
        let img = con.image(Protocol::SIXEL, 0, (40, 48), ramp);
        con.place(
            img,
            Place {
                col: 1,
                row: 0,
                x: 3,
                y: 5,
                ..Place::default()
            },
        );
        if cut {
            con.cut(1, 2, 3);
        }
        con.frame(0)
    };
    let pieces = picture(true);
    assert_eq!(pieces.len(), 4, "above, left, right and below the cut");
    // made in 8x16 cells, drawn in 12x24 ones
    let size = (120, 96);
    let (whole, _) = draw_with(psf(12, 24), &s, picture(false), size, Config::default());
    let (cut, counts) = draw_with(psf(12, 24), &s, pieces, size, Config::default());
    let (plain, _) = draw_with(psf(12, 24), &s, Vec::new(), size, Config::default());
    assert_eq!((counts.scaled, counts.kept), (1, 1));
    for y in 0..size.1 {
        for x in 0..size.0 {
            // the cut cells, columns 2 and 3 of row 1
            let erased = (24..48).contains(&x) && (24..48).contains(&y);
            let want = if erased {
                plain.pixel(x, y)
            } else {
                whole.pixel(x, y)
            };
            assert_eq!(cut.pixel(x, y), want, "{x},{y}");
        }
    }
}

/// `s` with the picture of `con` drawn into `frames` frames, the view one
/// line further down each frame: the counts after each frame and the last
/// frame drawn.
fn scroll<F: Face>(
    r: &mut Renderer<F>,
    s: &Screen,
    con: &Console,
    frames: usize,
    size: (u32, u32),
) -> (Vec<PictureCounts>, Canvas) {
    let mut out = Vec::new();
    let mut c = Canvas::new(size.0, size.1, 0);
    for top in 0..frames {
        c = Canvas::new(size.0, size.1, 0);
        r.draw(
            &s.frame(Age::NEVER).with_placements(con.frame(top)),
            &mut c,
            0,
            &state(s),
        );
        out.push(r.picture_counts());
    }
    (out, c)
}

// issue #30: the C reference kept a scaled picture under its visible part,
// so one scrolling off the top was scaled again every frame
// covers: image.compositor
#[test]
fn a_scaled_placement_moved_up_a_row_each_frame_is_scaled_once() {
    // made in 8x16 cells, drawn in unifont's 16x32: twice the size
    let size = (160, 192);
    let mut r = Renderer::new(
        vtium_font::Font::new(vtium_font::Unifont::new(32)),
        size.0,
        size.1,
        Config::default(),
    );
    let (cols, rows) = r.grid_size();
    let s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("000000"));
    let mut con = Console::new(cols, rows, 6);
    let img = con.image(Protocol::SIXEL, 0, (16, 48), ramp);
    con.place(
        img,
        Place {
            col: 2,
            row: 3,
            ..Place::default()
        },
    );
    // rows 3, 2, 1, 0, -1 and -2: the last two frames draw its lower rows
    let (counts, last) = scroll(&mut r, &s, &con, 6, size);
    for (i, n) in counts.iter().enumerate() {
        assert_eq!(
            (n.scaled, n.kept, n.evicted, n.uncached),
            (1, 1, 0, 0),
            "frame {i}"
        );
    }
    // the kept copy draws what scaling the picture afresh draws
    let fresh = draw_with(
        vtium_font::Font::new(vtium_font::Unifont::new(32)),
        &s,
        con.frame(5),
        size,
        Config::default(),
    )
    .0;
    assert_eq!(last, fresh);
    // at twice the size, bilinear: the ramp is linear in each channel, so
    // each pixel is the ramp at its centre, (2u + 1) / 4 - 1/2 source
    // pixels, kept inside the picture, rounded half up. Its top is two
    // rows of 32 pixels above the view, so display row 16 is its row 80.
    let at = |u: u32, side: u32| (f64::from(u) / 2.0 - 0.25).clamp(0.0, f64::from(side - 1));
    let (py, gy) = (at(80, 48), 80u32);
    assert_eq!(gy, 2 * 32 + 16);
    for u in 0..32 {
        let px = at(u, 16);
        let r = (7.0 * px).round() as u32;
        let g = (5.0 * py).round() as u32;
        let b = (3.0 * (px + py)).round() as u32;
        assert_eq!(
            last.pixel(32 + u, 16),
            Some(r << 16 | g << 8 | b),
            "column {u}"
        );
    }
}

// issue #30: a turned screen scrolling twelve pictures evicts only what its
// first turn evicted
// covers: image.compositor
#[test]
fn a_turned_screen_scrolling_twelve_pictures_evicts_only_what_its_first_turn_evicted() {
    // turned a quarter right: 10 columns of 8 pixels down a display 80
    // tall, and 10 rows of 16 across it, 160 wide; the copies hold two of
    // its screens, 102,400 bytes
    let size = (160, 80);
    let config = Config {
        orientation: Orientation::Right,
        ..Config::default()
    };
    let mut r = Renderer::new(unifont(), size.0, size.1, config);
    let (cols, rows) = r.grid_size();
    assert_eq!((cols, rows), (10, 10));
    let s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("000000"));
    let limit = PICTURE_CACHE_SCREENS as u64 * 160 * 80 * 4;
    // the frame before shows an earlier picture of 24,000 pixels, whose
    // copy the twelve need the room of
    let mut first = Console::new(cols, rows, 0);
    let big = first.image(Protocol::SIXEL, 0, (160, 150), ramp);
    first.place(
        big,
        Place {
            col: 0,
            row: 0,
            ..Place::default()
        },
    );
    r.draw(
        &s.frame(Age::NEVER).with_placements(first.frame(0)),
        &mut Canvas::new(size.0, size.1, 0),
        0,
        &state(&s),
    );
    let before = r.picture_counts();
    assert_eq!((before.kept, before.evicted), (1, 0));
    assert!(160 * 150 * 4 + 12 * 16 * 32 * 4 > limit);
    // twelve pictures of 2x2 cells, three to a row of the grid
    let mut con = Console::new(cols, rows, 10);
    for i in 0..12u32 {
        let img = con.image(Protocol::SIXEL, 0, (16, 32), move |x, y| {
            ramp(x + i, y + 3 * i)
        });
        con.place(
            img,
            Place {
                col: (i % 3) * 3,
                row: (i / 3) * 2 + 2,
                ..Place::default()
            },
        );
    }
    // eight frames, the view a line further down each: the top pictures
    // scroll off the top a row at a time
    let (counts, last) = scroll(&mut r, &s, &con, 8, size);
    let turn = counts[0];
    assert_eq!(
        (turn.kept, turn.evicted),
        (13, 1),
        "the first turn evicts the earlier picture"
    );
    for (i, n) in counts.iter().enumerate() {
        assert_eq!(
            (n.scaled, n.evicted, n.uncached),
            (turn.scaled, turn.evicted, 0),
            "frame {i}"
        );
        assert!(n.bytes <= limit);
    }
    let fresh = draw_with(unifont(), &s, con.frame(7), size, config).0;
    assert_eq!(last, fresh);
}

// ADR-1100 D5: the copies stay under their bound, whatever is drawn
// covers: image.compositor
#[test]
fn the_scaled_copies_never_pass_their_bound() {
    let size = (160, 96);
    let mut r = Renderer::new(
        vtium_font::Font::new(vtium_font::Unifont::new(32)),
        size.0,
        size.1,
        Config::default(),
    );
    let (cols, rows) = r.grid_size();
    let s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("000000"));
    let limit = PICTURE_CACHE_SCREENS as u64 * 160 * 96 * 4;
    for frame in 0..30u32 {
        let mut con = Console::new(cols, rows, 0);
        for i in 0..4 {
            let side = 8 + (frame * 4 + i) % 5 * 8;
            let img = con.image(Protocol::SIXEL, 0, (side, side * 2), move |x, y| {
                ramp(x + frame, y + i)
            });
            con.place(
                img,
                Place {
                    col: i * 2,
                    row: 0,
                    ..Place::default()
                },
            );
        }
        r.draw(
            &s.frame(Age::NEVER).with_placements(con.frame(0)),
            &mut Canvas::new(size.0, size.1, 0),
            0,
            &state(&s),
        );
        let n = r.picture_counts();
        assert!(n.bytes <= limit, "frame {frame}: {} bytes", n.bytes);
    }
    let n = r.picture_counts();
    assert!(n.evicted > 0 && n.kept > n.evicted);
}

/// `r`'s frame of `placements` into `c`, which holds the last frame:
/// the damage rectangles and the cells drawn.
fn redraw<F: Face>(
    r: &mut Renderer<F>,
    s: &mut Screen,
    since: &mut Age,
    placements: Vec<FramePlacement>,
    c: &mut Canvas,
) -> (Vec<vtium_render::Rect>, u32) {
    let d = r.draw(
        &s.frame(*since).with_placements(placements),
        c,
        1,
        &state(s),
    );
    let out = (d.damage.to_vec(), d.stats.cells);
    *since = s.damage.checkpoint();
    out
}

// issue #30: in tmux sixel grids the C reference damaged 97 to 100 percent
// of the screen per flip; a picture that changes damages only its pane
// covers: image.compositor
#[test]
fn a_picture_that_changes_or_moves_redraws_only_the_cells_it_covers() {
    let mut s = Screen::new(20, 4, rgb("c0c0c0"), rgb("000000"));
    let mut con = Console::new(20, 4, 0);
    let left = con.image(Protocol::SIXEL, 0, (32, 32), ramp);
    con.place(
        left,
        Place {
            col: 0,
            row: 0,
            ..Place::default()
        },
    );
    let right = con.image(Protocol::SIXEL, 0, (32, 32), solid(BLUE));
    con.place(
        right,
        Place {
            col: 12,
            row: 0,
            ..Place::default()
        },
    );
    let mut r = Renderer::new(unifont(), 160, 64, Config::default());
    let mut c = Canvas::new(160, 64, 0);
    let mut since = Age::NEVER;
    redraw(&mut r, &mut s, &mut since, con.frame(0), &mut c);
    // nothing changed: nothing drawn
    let (damage, cells) = redraw(&mut r, &mut s, &mut since, con.frame(0), &mut c);
    assert_eq!((damage.len(), cells), (0, 0));
    // the right pane's picture is replaced: a new sixel over it cuts it
    let next = con.image(Protocol::SIXEL, 0, (32, 32), ramp);
    con.place(
        next,
        Place {
            col: 12,
            row: 0,
            ..Place::default()
        },
    );
    let (damage, cells) = redraw(&mut r, &mut s, &mut since, con.frame(0), &mut c);
    assert_eq!(cells, 8, "the right pane's four by two cells");
    for d in &damage {
        assert!(
            d.x >= 96 && d.x + d.width <= 128 && d.y + d.height <= 32,
            "{d:?}"
        );
    }
    // the left one moves two columns right, as a frame shows it moved:
    // its old cells and its new ones draw, and no other
    let mut moved = con.frame(0);
    let at = moved.iter().position(|p| p.col == 0).unwrap();
    moved[at].col = 2;
    let (damage, cells) = redraw(&mut r, &mut s, &mut since, moved, &mut c);
    assert_eq!(cells, 12, "columns 0 to 5 of rows 0 and 1");
    for d in &damage {
        assert!(d.x + d.width <= 48 && d.y + d.height <= 32, "{d:?}");
    }
    // a frame that drops it draws its cells again without it
    let mut gone = con.frame(0);
    gone.retain(|p| p.col != 0);
    let (_, cells) = redraw(&mut r, &mut s, &mut since, gone.clone(), &mut c);
    assert_eq!(cells, 8);
    let fresh = draw_with(unifont(), &s, gone, (160, 64), Config::default()).0;
    assert_eq!(c, fresh);
}

// docs/architecture.md, "Determinism": the frames a display draws into its
// buffers, whatever their ages, end as one whole draw does
// covers: image.compositor
#[test]
fn every_buffer_ends_as_a_whole_draw_of_the_last_frame() {
    let size = (160, 128);
    let mut s = Screen::new(20, 8, rgb("c0c0c0"), rgb("000000"));
    s.text(
        0,
        3,
        "text under a picture",
        style(rgb("ffffff"), rgb("200000")),
    );
    let mut con = Console::new(20, 8, 8);
    let img = con.image(Protocol::KITTY, 1, (24, 40), ramp);
    con.place(
        img,
        Place {
            col: 4,
            row: 5,
            z: -1,
            ..Place::default()
        },
    );
    let half = con.image(Protocol::SIXEL, 0, (16, 16), solid(0x8040_0000));
    con.place(
        half,
        Place {
            col: 5,
            row: 3,
            ..Place::default()
        },
    );
    let mut r = Renderer::new(psf(12, 24), size.0, size.1, Config::default());
    let (cols, rows) = r.grid_size();
    let mut s = Screen::new(cols, rows, rgb("c0c0c0"), rgb("000000"));
    s.text(0, 3, "text under", style(rgb("ffffff"), rgb("200000")));
    let mut bufs = [
        Canvas::new(size.0, size.1, 0),
        Canvas::new(size.0, size.1, 0),
    ];
    let (mut held, mut frames) = ([0u64; 2], 0u64);
    let mut since = Age::NEVER;
    for top in 0..6 {
        let back = top % 2;
        let age = match held[back] {
            0 => 0,
            h => (frames - h + 1) as u32,
        };
        let d = r.draw(
            &s.frame(since).with_placements(con.frame(top)),
            &mut bufs[back],
            age,
            &state(&s),
        );
        if !d.damage.is_empty() {
            frames += 1;
            held[back] = frames;
        }
        since = s.damage.checkpoint();
        let fresh = draw_with(psf(12, 24), &s, con.frame(top), size, Config::default()).0;
        assert_eq!(bufs[back], fresh, "frame {top}");
    }
}

// the orientation test's rule, with pictures: each turned screen, turned
// back, is the unturned screen, at the picture's own size and scaled
// covers: image.compositor
#[test]
fn every_orientation_draws_the_same_pictures_turned() {
    let mut s = Screen::new(20, 5, rgb("c0c0c0"), rgb("000000"));
    s.text(0, 1, "pictures turn", style(rgb("ffffff"), rgb("200000")));
    s.cursor.visible = true;
    s.cursor.row = 2;
    s.cursor.col = 6;
    s.cursor.shape = CursorShape::Bar;
    let mut con = Console::new(20, 5, 0);
    let a = con.image(Protocol::SIXEL, 0, (40, 30), ramp);
    con.place(
        a,
        Place {
            col: 1,
            row: 1,
            x: 3,
            y: 5,
            ..Place::default()
        },
    );
    let b = con.image(Protocol::KITTY, 1, (17, 9), |x, y| {
        if (x + y) % 3 == 0 { 0 } else { 0xc000_8040 }
    });
    con.place(
        b,
        Place {
            col: 9,
            row: 0,
            cols: 7,
            rows: 4,
            z: -5,
            ..Place::default()
        },
    );
    // in the 8x16 cells the store made them in, and scaled to 12x24 cells
    for font in [16u32, 24] {
        // a border of 2 pixels each side and 1 above and below, which turns
        // with the grid
        let (cw, ch) = if font == 16 { (8, 16) } else { (12, 24) };
        let (w, h) = (20 * cw + 4, 5 * ch + 2);
        let drawn = |o: Orientation| {
            let size = if matches!(o, Orientation::Right | Orientation::Left) {
                (h, w)
            } else {
                (w, h)
            };
            let config = Config {
                orientation: o,
                ..Config::default()
            };
            if font == 16 {
                draw_with(unifont(), &s, con.frame(0), size, config).0
            } else {
                draw_with(psf(12, 24), &s, con.frame(0), size, config).0
            }
        };
        let normal = drawn(Orientation::Normal);
        for o in [
            Orientation::Right,
            Orientation::UpsideDown,
            Orientation::Left,
        ] {
            let c = drawn(o);
            let (cw, ch) = (normal.width(), normal.height());
            let mut back = Canvas::new(cw, ch, 0);
            for y in 0..ch {
                let row: Vec<u32> = (0..cw)
                    .map(|x| {
                        let (tx, ty) = match o {
                            Orientation::Normal => (x, y),
                            Orientation::UpsideDown => (cw - 1 - x, ch - 1 - y),
                            Orientation::Right => (ch - 1 - y, x),
                            Orientation::Left => (y, cw - 1 - x),
                        };
                        c.pixel(tx, ty).unwrap()
                    })
                    .collect();
                back.write(0, y, &row);
            }
            assert!(back == normal, "{o:?} at a {font}-pixel font");
        }
    }
}
