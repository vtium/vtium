//! The frames of an animated picture in the CPU compositor (ADR-1102 D3,
//! D6): a placement shows the frame its entry carries, and the renderer
//! names what it keeps for a picture, its scaled copies and its record of
//! what it last drew, by the image's key, the frame's number and the
//! frame's pixel revision. The store fills in the image's current frame, the
//! root frame until `a=a` with `c` makes another current, and the binary's
//! animation clock will put the frame it chooses on the entry with
//! `FramePlacement::show`. No clock does so yet, so each test calls `show`
//! itself with the frame a clock would choose.
#![cfg(feature = "images")]

mod common;

use common::{Screen, rgb};
use vtium_font::{Font, Unifont};
use vtium_image::{
    FramePixels, FramePlacement, LineKey, NewImage, Pixels, Place, Protocol, Rows, Screens, Store,
};
use vtium_render::{
    Canvas, Config, DrawState, PICTURE_CACHE_SCREENS, PictureCounts, Rect, Renderer,
};
use vtium_term::Age;

/// The display: 160x128 pixels, 10x4 cells of unifont at 32 pixels and
/// 20x8 at 16.
const SIZE: (u32, u32) = (160, 128);

/// A picture whose every pixel differs from its neighbours, opaque.
fn ramp(x: u32, y: u32) -> u32 {
    0xff00_0000 | (x * 7 % 256) << 16 | (y * 5 % 256) << 8 | ((x + y) * 3 % 256)
}

/// The font: unifont's 16x32 cells draw a picture made in 8x16 cells at
/// twice its size, from a scaled copy; its 8x16 cells draw it at its own
/// size, from the image's pixels.
fn font(scaled: bool) -> Font<Unifont> {
    Font::new(Unifont::new(if scaled { 32 } else { 16 }))
}

/// The store's frame entries for one `w` by `h` kitty picture, made in 8x16
/// cells, with a placement for each of `places`, on a screen of `cols` by
/// `rows`. The store fills in frame 1, the root frame, at the image's own
/// pixel revision.
fn entries(
    (w, h): (u32, u32),
    places: &[Place],
    (cols, rows): (usize, usize),
) -> Vec<FramePlacement> {
    let keys: Vec<LineKey> = (1..=rows as u64).map(LineKey::new).collect();
    let view = Rows {
        screen: &keys,
        above: &[],
    };
    let screens = Screens {
        main: view,
        alt: Rows::default(),
    };
    let mut store = Store::new();
    store.set_limit(1 << 30);
    let new = NewImage {
        protocol: Protocol::KITTY,
        width: w,
        height: h,
        pixels: (0..h)
            .flat_map(|y| (0..w).map(move |x| ramp(x, y)))
            .collect(),
        id: 1,
        number: 0,
        transient: false,
        opaque: None,
    };
    let image = store.add_image(new, None, screens).unwrap();
    for place in places {
        store.place(image, *place, screens, cols).unwrap();
    }
    store.frame(view, cols)
}

/// A placement at column `col`, row `row`, at the picture's own size.
fn at(col: u32, row: u32) -> Place {
    Place {
        col,
        row,
        ..Place::default()
    }
}

/// Two placements of one picture, drawn at two sizes: placement 1 at its
/// own size at column 2, row 1, and placement 2 fitted to a box of 4 by 3
/// cells at column 6, row 0.
fn two_sizes() -> [Place; 2] {
    [
        Place { id: 1, ..at(2, 1) },
        Place {
            id: 2,
            cols: 4,
            rows: 3,
            ..at(6, 0)
        },
    ]
}

/// Frame `n` of a `w` by `h` picture at pixel revision `revision`: the ramp
/// moved by both, so no two frames, and no two revisions of one frame,
/// hold the same pixels.
fn frame(n: u32, revision: u64, (w, h): (u32, u32)) -> FramePixels {
    let shift = n * 11 + u32::try_from(revision % 64).unwrap() * 3;
    FramePixels {
        frame: n,
        revision,
        opaque: true,
        pixels: Pixels::new(
            (0..h)
                .flat_map(|y| (0..w).map(move |x| ramp(x + shift, y + n)))
                .collect(),
        ),
    }
}

/// The entries with `f` put on each, as the binary's animation clock will
/// put the frame it chooses.
fn showing(entries: &[FramePlacement], f: &FramePixels) -> Vec<FramePlacement> {
    entries
        .iter()
        .cloned()
        .map(|mut p| {
            p.show(f.clone());
            p
        })
        .collect()
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// A renderer drawing one console's frames into one buffer, which always
/// holds the frame before: each draw is at age 1.
struct Player {
    r: Renderer<Unifont>,
    s: Screen,
    c: Canvas,
    since: Age,
    scaled: bool,
}

impl Player {
    fn new(scaled: bool) -> Player {
        let r = Renderer::new(font(scaled), SIZE.0, SIZE.1, Config::default());
        let (cols, rows) = r.grid_size();
        Player {
            r,
            s: Screen::new(cols, rows, rgb("c0c0c0"), rgb("000000")),
            c: Canvas::new(SIZE.0, SIZE.1, 0),
            since: Age::NEVER,
            scaled,
        }
    }

    fn grid(&self) -> (usize, usize) {
        self.r.grid_size()
    }

    /// Draws `placements` over the frame before: the damage rectangles
    /// and the cells drawn.
    fn draw(&mut self, placements: &[FramePlacement]) -> (Vec<Rect>, u32) {
        let d = self.r.draw(
            &self
                .s
                .frame(self.since)
                .with_placements(placements.to_vec()),
            &mut self.c,
            1,
            &state(&self.s),
        );
        let out = (d.damage.to_vec(), d.stats.cells);
        self.since = self.s.damage.checkpoint();
        out
    }

    /// `placements` drawn whole by a renderer of their own.
    fn fresh(&self, placements: &[FramePlacement]) -> Canvas {
        let mut r = Renderer::new(font(self.scaled), SIZE.0, SIZE.1, Config::default());
        let mut c = Canvas::new(SIZE.0, SIZE.1, 0);
        r.draw(
            &self
                .s
                .frame(Age::NEVER)
                .with_placements(placements.to_vec()),
            &mut c,
            0,
            &state(&self.s),
        );
        c
    }

    fn counts(&self) -> PictureCounts {
        self.r.picture_counts()
    }
}

/// The display rectangle of the cells of `p`'s box.
fn box_px(p: &FramePlacement, (cw, ch): (u32, u32)) -> Rect {
    Rect {
        x: p.col * cw,
        y: u32::try_from(p.row).unwrap() * ch,
        width: p.cols * cw,
        height: p.rows * ch,
    }
}

fn inside(d: &Rect, b: &Rect) -> bool {
    d.x >= b.x && d.y >= b.y && d.x + d.width <= b.x + b.width && d.y + d.height <= b.y + b.height
}

// ADR-1102 D6: two snapshots of one animated placement that differ only in
// the frame the clock chose, with the same image key and store revision,
// draw each frame's own pixels; the frame that follows draws the picture's
// cells again and no others. Frame 3 is given the root frame's pixel
// revision, so only its number tells it from frame 1.
// covers: image.kitty-animation
#[test]
fn two_snapshots_that_differ_only_in_the_frame_shown_draw_each_frames_pixels() {
    let side = (16, 24);
    for scaled in [false, true] {
        let mut p = Player::new(scaled);
        let entry = entries(side, &[at(2, 1)], p.grid());
        let cell = if scaled { (16, 32) } else { (8, 16) };
        let picture = box_px(&entry[0], cell);
        let cells = entry[0].cols * entry[0].rows;
        let root = entry[0].pixel_revision;
        assert_eq!(entry[0].frame, 1);
        p.draw(&entry);
        assert_eq!(p.c, p.fresh(&entry), "scaled {scaled}: the root frame");
        for (n, revision) in [(2, root + 1), (3, root), (1, root)] {
            let shown = if n == 1 {
                entry.clone()
            } else {
                showing(&entry, &frame(n, revision, side))
            };
            assert_eq!(
                (shown[0].image, shown[0].revision),
                (entry[0].image, entry[0].revision)
            );
            let before = p.c.clone();
            let (damage, drawn) = p.draw(&shown);
            assert_eq!(drawn, cells, "scaled {scaled}, frame {n}");
            for d in &damage {
                assert!(inside(d, &picture), "scaled {scaled}, frame {n}: {d:?}");
            }
            assert_eq!(p.c, p.fresh(&shown), "scaled {scaled}, frame {n}");
            assert_ne!(p.c, before, "scaled {scaled}, frame {n}");
        }
    }
}

// ADR-1102 D3: the copy of a frame is kept and drawn again; the same frame
// shown twice is scaled once and draws nothing the second time
// covers: image.kitty-animation
#[test]
fn the_same_frame_shown_twice_is_scaled_once() {
    let side = (16, 24);
    let mut p = Player::new(true);
    let entry = entries(side, &[at(2, 1)], p.grid());
    let two = frame(2, entry[0].pixel_revision + 1, side);
    p.draw(&showing(&entry, &two));
    let n = p.counts();
    assert_eq!((n.scaled, n.kept, n.copies), (1, 1, 1));
    // a later snapshot of the same frame: its own entries, the same pixels
    let again = showing(&entries(side, &[at(2, 1)], p.grid()), &two);
    let (damage, drawn) = p.draw(&again);
    assert_eq!((damage.len(), drawn), (0, 0));
    assert_eq!(p.counts().scaled, 1);
    // back to the root and on to frame 2: each frame's copy is kept
    for shown in [entry.clone(), showing(&entry, &two), entry.clone()] {
        p.draw(&shown);
        assert_eq!(p.c, p.fresh(&shown));
    }
    let n = p.counts();
    assert_eq!((n.scaled, n.kept, n.copies, n.evicted), (2, 2, 2, 0));
}

// ADR-1102 D3: a command that changes what the frame on show shows gives
// it a new pixel revision, and the picture's cells draw again from the new
// pixels; the copy of the revision before is dropped
// covers: image.kitty-animation
#[test]
fn an_edit_to_the_frame_on_show_redraws_its_cells_from_the_new_pixels() {
    let side = (16, 24);
    for scaled in [false, true] {
        let mut p = Player::new(scaled);
        let entry = entries(side, &[at(2, 1)], p.grid());
        let cells = entry[0].cols * entry[0].rows;
        let rev = entry[0].pixel_revision;
        p.draw(&showing(&entry, &frame(2, rev + 1, side)));
        for edit in [rev + 2, rev + 7] {
            let shown = showing(&entry, &frame(2, edit, side));
            let before = p.c.clone();
            let (_, drawn) = p.draw(&shown);
            assert_eq!(drawn, cells, "scaled {scaled}, revision {edit}");
            assert_eq!(p.c, p.fresh(&shown), "scaled {scaled}, revision {edit}");
            assert_ne!(p.c, before, "scaled {scaled}, revision {edit}");
        }
        if scaled {
            let n = p.counts();
            assert_eq!((n.scaled, n.copies), (3, 1));
            assert_eq!(n.bytes, 32 * 48 * 4);
        }
    }
}

// The bound a playing animation keeps its copies under: at most one copy
// for each frame of the picture, at that frame's newest pixel revision,
// and every copy together within PICTURE_CACHE_SCREENS screens. A loop
// whose copies all fit is scaled in its first pass alone.
// covers: image.kitty-animation
#[test]
fn an_animation_looping_over_its_frames_keeps_one_copy_for_each_frame() {
    let side = (16, 24);
    let frames = 6u32;
    let mut p = Player::new(true);
    let entry = entries(side, &[at(2, 1)], p.grid());
    let rev = entry[0].pixel_revision;
    let copy = 32 * 48 * 4;
    let mut edits = 0u64;
    for pass in 0..4u64 {
        // frame 3 is edited in each pass after the first
        let three = if pass == 0 { rev + 3 } else { rev + 100 + pass };
        for n in 1..=frames {
            let revision = if n == 3 { three } else { rev + u64::from(n) };
            let shown = if n == 1 {
                entry.clone()
            } else {
                showing(&entry, &frame(n, revision, side))
            };
            p.draw(&shown);
            assert_eq!(p.c, p.fresh(&shown), "pass {pass}, frame {n}");
            let c = p.counts();
            assert!(c.copies <= frames as usize, "pass {pass}, frame {n}");
            assert_eq!(c.bytes, c.copies as u64 * copy);
        }
        if pass > 0 {
            edits += 1;
        }
        let c = p.counts();
        assert_eq!(
            (c.scaled, c.copies, c.evicted, c.uncached),
            (u64::from(frames) + edits, frames as usize, 0, 0),
            "pass {pass}"
        );
    }
}

// A loop whose copies do not all fit evicts the copy placed longest ago
// for each frame it draws, and its copies never pass their bound: the
// picture fills the display, so two copies fill the cache
// covers: image.kitty-animation
#[test]
fn an_animation_too_large_for_the_cache_stays_within_its_bound() {
    let side = (80, 64);
    let mut p = Player::new(true);
    // placed at the grid's top left, the copy is the whole display
    let entry = entries(side, &[at(0, 0)], p.grid());
    let limit = PICTURE_CACHE_SCREENS as u64 * u64::from(SIZE.0 * SIZE.1) * 4;
    let copy = u64::from(SIZE.0 * SIZE.1) * 4;
    let rev = entry[0].pixel_revision;
    let mut draws = 0u64;
    for pass in 0..3 {
        for n in 1..=5u32 {
            let shown = if n == 1 {
                entry.clone()
            } else {
                showing(&entry, &frame(n, rev + u64::from(n), side))
            };
            p.draw(&shown);
            draws += 1;
            assert_eq!(p.c, p.fresh(&shown), "pass {pass}, frame {n}");
            let c = p.counts();
            assert!(
                c.bytes <= limit,
                "pass {pass}, frame {n}: {} bytes",
                c.bytes
            );
            assert!(c.copies as u64 <= limit / copy, "pass {pass}, frame {n}");
            // the copy of the frame it draws went two frames back: each draw scales
            assert_eq!(c.scaled, draws);
        }
    }
    assert!(p.counts().evicted > 0);
}

// ADR-1102 D3: every placement of an image shows the same frame, and a
// placement drawn at another size keeps a copy of its own of that frame; an
// edit to the frame replaces the copies at both sizes
// covers: image.kitty-animation
#[test]
fn two_placements_at_two_sizes_share_a_frame_and_its_edits() {
    let side = (16, 24);
    let mut p = Player::new(true);
    let entry = entries(side, &two_sizes(), p.grid());
    assert_eq!(entry.len(), 2);
    assert_ne!(
        (entry[0].drawn_width, entry[0].drawn_height),
        (entry[1].drawn_width, entry[1].drawn_height)
    );
    let rev = entry[0].pixel_revision;
    for (n, revision) in [
        (2, rev + 1),
        (2, rev + 4),
        (1, rev),
        (2, rev + 4),
        (2, rev + 8),
    ] {
        let shown = if n == 1 {
            entry.clone()
        } else {
            showing(&entry, &frame(n, revision, side))
        };
        p.draw(&shown);
        assert_eq!(p.c, p.fresh(&shown), "frame {n}, revision {revision}");
    }
    // the root frame at two sizes, and frame 2 at its newest revision at two
    let c = p.counts();
    assert_eq!((c.copies, c.evicted), (4, 0), "{c:?}");
}

// Two placements of one image that show different frames, or different
// revisions of one frame, in the same draw each draw their own pixels: a
// copy that a placement in this draw shows is never dropped for another
// placement's newer revision
// covers: image.kitty-animation
#[test]
fn two_placements_showing_different_frames_in_one_draw_each_show_their_own() {
    let side = (16, 24);
    for scaled in [false, true] {
        let mut p = Player::new(scaled);
        let entry = entries(side, &two_sizes(), p.grid());
        let rev = entry[0].pixel_revision;
        let mix = |a: (u32, u64), b: (u32, u64)| {
            let mut v = entry.clone();
            for (e, (n, revision)) in v.iter_mut().zip([a, b]) {
                if n != 1 {
                    e.show(frame(n, revision, side));
                }
            }
            v
        };
        for (a, b) in [
            ((2, rev + 1), (3, rev + 2)),
            ((3, rev + 2), (2, rev + 1)),
            ((2, rev + 1), (2, rev + 5)),
            ((2, rev + 5), (2, rev + 1)),
            ((1, rev), (3, rev + 2)),
        ] {
            let shown = mix(a, b);
            p.draw(&shown);
            assert_eq!(p.c, p.fresh(&shown), "scaled {scaled}: {a:?} and {b:?}");
        }
    }
}
