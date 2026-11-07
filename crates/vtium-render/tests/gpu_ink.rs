//! Glyph ink drawn past its own cells on the GPU path (ADR-1306 D9,
//! ADR-1300 D6): a cell receives the ink of the glyphs around it as the CPU
//! path draws it, in D4's order, with D6's colors and D7's picture layers,
//! and a changed cell redraws the neighbours its ink reaches (D5).
//!
//! The stand-in GL draws the cells through its model of the cell shader,
//! so each test compares the cells the GPU path composes with the CPU
//! path's whole drawing of the same screen, pixel for pixel. Tier 2 judges
//! the shader itself on a real GL.
#![cfg(feature = "gpu")]

mod common;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Face, Font, Format as FontFormat, Glyph, Style, Width as FontWidth};
use vtium_render::gpu::{Call, DrawTarget, Format, Gl, Pipeline, Quad, Size, StandIn};
use vtium_render::{Canvas, Config, DrawState, Orientation, Rect, Renderer};
use vtium_term::{Age, CursorShape};

const CW: u32 = 8;
const CH: u32 = 16;
const FG: &str = "c0c0c0";
const BG: &str = "101010";

/// A rectangle of ink from `x0`, `y0` to `x1`, `y1`, from the top left of
/// the glyph's cell, at coverage `a`.
type Ink = (i32, i32, i32, i32, u8);

/// The ink each test character draws, a body inside its cell and ink past
/// it:
/// - `A`, an accent 3 pixels above the row, its edge half covered;
/// - `G`, a descender 14 pixels into the row below;
/// - `V`, an accent 6 pixels into the row above, where `G`'s descender
///   from two rows up meets it;
/// - `Q`, ink in every direction, the corners too, some of it partly
///   covered;
/// - `x`, ink inside its cell, partly covered at its edges.
fn shape(c: char) -> &'static [Ink] {
    const BODY: Ink = (1, 6, 7, 14, 255);
    match c {
        'A' => &[
            BODY,
            (2, 4, 6, 6, 100),
            (3, -3, 5, 0, 255),
            (5, -3, 6, -2, 128),
        ],
        'G' => &[BODY, (2, 10, 5, 30, 255)],
        'V' => &[BODY, (3, -6, 6, 0, 255)],
        'Q' => &[
            BODY,
            (-3, -3, 11, 0, 255),
            (-3, 16, 11, 19, 255),
            (-3, 0, 0, 16, 200),
            (8, 0, 11, 16, 200),
        ],
        'x' => &[BODY, (0, 6, 1, 14, 90), (7, 6, 8, 14, 90)],
        _ => &[],
    }
}

/// `c`'s glyph: its pixels inside its cell, and its ink box when its ink
/// passes the cell.
fn glyph(c: char) -> Option<Glyph> {
    let rects = shape(c);
    let (gw, gh) = (CW as i32, CH as i32);
    let cov = |x: i32, y: i32| {
        rects
            .iter()
            .rev()
            .find(|&&(x0, y0, x1, y1, _)| x >= x0 && x < x1 && y >= y0 && y < y1)
            .map_or(0, |r| r.4)
    };
    let own = (0..gh)
        .flat_map(|y| (0..gw).map(move |x| cov(x, y)))
        .collect();
    let g = Glyph::from_pixels(CW, CH, FontFormat::Mask, own)?.with_cells(FontWidth::Narrow);
    let (mut l, mut t, mut r, mut b) = (0, 0, gw, gh);
    for &(x0, y0, x1, y1, _) in rects {
        (l, t, r, b) = (l.min(x0), t.min(y0), r.max(x1), b.max(y1));
    }
    if (l, t, r, b) == (0, 0, gw, gh) {
        return Some(g);
    }
    let px = (t..b)
        .flat_map(|y| (l..r).map(move |x| cov(x, y)))
        .collect();
    g.with_ink((l, t), ((r - l) as u32, (b - t) as u32), px)
}

/// An outline face in 8x16 cells whose glyphs reach as [`shape`] draws.
struct Reaching;

impl Face for Reaching {
    fn cell(&self) -> Cell {
        Cell {
            width: CW,
            height: CH,
        }
    }
    fn size_step(&self) -> u32 {
        1
    }
    fn has_glyph(&self, c: char) -> bool {
        c.is_ascii_graphic() || c == ' '
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<Glyph> {
        glyph(*cluster.first()?)
    }
}

/// A screen of `cols` by `rows` with an invisible cursor.
fn screen(cols: usize, rows: usize) -> Screen {
    let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    s
}

fn plain() -> vtium_term::Style {
    style(rgb(FG), rgb(BG))
}

/// The display for `s` turned as `o` says: its grid inside a border of 2
/// pixels across and 1 down, upright.
fn display(s: &Screen, o: Orientation) -> Size {
    let (w, h) = (s.cols as u32 * CW + 4, s.rows as u32 * CH + 2);
    match o {
        Orientation::Right | Orientation::Left => Size::new(h, w),
        Orientation::Normal | Orientation::UpsideDown => Size::new(w, h),
    }
}

fn config(o: Orientation) -> Config {
    Config {
        orientation: o,
        ..Config::default()
    }
}

fn state(s: &Screen, cursor_on: bool) -> DrawState {
    DrawState {
        cursor_on,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// `s` drawn whole on the CPU path.
fn whole(s: &Screen, st: &DrawState) -> Canvas {
    whole_turned(s, st, Orientation::Normal)
}

/// `s` drawn whole on the CPU path, turned as `o` says.
fn whole_turned(s: &Screen, st: &DrawState, o: Orientation) -> Canvas {
    let size = display(s, o);
    let mut r = Renderer::new(Font::new(Reaching), size.width, size.height, config(o));
    let mut c = Canvas::new(size.width, size.height, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, st);
    c
}

/// A display on the stand-in GL, whose upright frame keeps every frame
/// the GPU path composed.
struct Gpu {
    gl: StandIn,
    pipeline: Pipeline,
    buffer: DrawTarget,
    r: Renderer<Reaching>,
    since: Age,
    orientation: Orientation,
}

impl Gpu {
    fn new(s: &Screen) -> Gpu {
        Gpu::turned(s, Orientation::Normal)
    }

    fn turned(s: &Screen, o: Orientation) -> Gpu {
        let size = display(s, o);
        let mut gl = StandIn::new();
        let pipeline = Pipeline::new(&mut gl, size, vtium_render::gpu::Config::default()).unwrap();
        let buffer = gl.import_buffer(&size).unwrap();
        Gpu {
            gl,
            pipeline,
            buffer,
            r: Renderer::new(Font::new(Reaching), size.width, size.height, config(o)),
            since: Age::NEVER,
            orientation: o,
        }
    }

    /// Draws `s` into the buffer one frame old, the pipeline's upright
    /// frame keeping the last; gives the frame's damage. The calls it made
    /// are left in the stand-in.
    fn frame(&mut self, s: &mut Screen, st: &DrawState) -> Vec<Rect> {
        self.gl.clear_calls();
        let frame = s.frame(self.since);
        let drawn = self
            .r
            .draw_gpu(
                &mut self.gl,
                &mut self.pipeline,
                &frame,
                &self.buffer,
                1,
                st,
                &[],
            )
            .unwrap();
        let damage = drawn.damage.to_vec();
        self.since = s.damage.checkpoint();
        damage
    }

    /// The upright frame's pixels.
    fn upright(&self) -> Vec<u32> {
        let name = *self.gl.frame_names().last().unwrap();
        self.gl.frame_pixels(name).unwrap().1.to_vec()
    }

    /// The cell quads of the last frame, through the cell shader with ink
    /// and without, in the order they were drawn.
    fn cells(&self) -> Vec<Quad> {
        self.gl
            .calls()
            .iter()
            .filter_map(|c| match c {
                Call::Draw { quads, inputs, .. }
                    if inputs.contains(&"cell") && !inputs.contains(&"under_at") =>
                {
                    Some(quads.clone())
                }
                _ => None,
            })
            .flatten()
            .collect()
    }
}

/// Fails, saying where, unless the GPU path's upright frame is `cpu`'s
/// pixels.
fn assert_same(g: &Gpu, cpu: &Canvas, what: &str) {
    let gpu = g.upright();
    let (w, h) = (cpu.width(), cpu.height());
    // the upright pixel the display's pixel x, y shows, and the upright
    // frame's width (turn.rs)
    let up = |x: u32, y: u32| match g.orientation {
        Orientation::Normal => (x, y, w),
        Orientation::UpsideDown => (w - 1 - x, h - 1 - y, w),
        Orientation::Right => (y, w - 1 - x, h),
        Orientation::Left => (h - 1 - y, x, h),
    };
    let differ: Vec<(u32, u32, u32, u32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let c = cpu.pixel(x, y)?;
            let (ux, uy, stride) = up(x, y);
            let p = gpu[(uy * stride + ux) as usize];
            (c != p).then_some((x, y, c, p))
        })
        .collect();
    assert!(
        differ.is_empty(),
        "{what}: {} pixels differ from the CPU path's, the first (x, y, cpu, gpu) {:x?}",
        differ.len(),
        differ.first()
    );
}

/// The display rectangle of the cell at `col`, `row`.
fn cell(col: u32, row: u32) -> Rect {
    Rect {
        x: 2 + col * CW,
        y: 1 + row * CH,
        width: CW,
        height: CH,
    }
}

/// The flags of the last cell quad drawn at the cell `col`, `row`, which
/// the frame's pixels show: whether it receives ink, and whether a mark
/// covers it. A frame can draw a cell twice, once with its row and again
/// once the row below has sent it ink.
fn flags(quads: &[Quad], col: u32, row: u32) -> u32 {
    let r = cell(col, row);
    let q = quads
        .iter()
        .rev()
        .find(|q| q.dst == [r.x as i32, r.y as i32, r.width as i32, r.height as i32])
        .unwrap_or_else(|| panic!("no quad at {col},{row}"));
    q.params[1] >> 24
}

fn covers(damage: &[Rect], r: Rect) -> bool {
    damage.iter().any(|d| {
        d.x <= r.x
            && d.y <= r.y
            && d.x + d.width >= r.x + r.width
            && d.y + d.height >= r.y + r.height
    })
}

// ADR-1306 D4, D9: an accent reaching into the row above draws over that
// row's background, and a cell the ink reaches is the only one that
// receives; a keystroke that writes an accented letter redraws the cell
// above it (D5).
// covers: text.overhang
#[test]
fn an_accent_over_the_row_above_draws_as_on_the_cpu_path() {
    let mut s = screen(6, 3);
    s.text(0, 0, "x x x ", style(rgb(FG), rgb("2040a0")));
    s.text(0, 1, "AxAAx", plain());
    let mut g = Gpu::new(&s);
    let st = state(&s, true);
    g.frame(&mut s, &st);
    assert_same(&g, &whole(&s, &st), "the first frame");
    let quads = g.cells();
    let receiving: Vec<u32> = (0..6).filter(|&c| flags(&quads, c, 0) & 1 != 0).collect();
    assert_eq!(
        receiving,
        [0, 2, 3],
        "the cells above an accent receive its ink"
    );
    assert_eq!(
        flags(&quads, 0, 1),
        0,
        "a letter below no ink receives none"
    );
    // the boxes: one texture of slots four cells wide and three rows tall
    assert!(
        g.gl.calls()
            .contains(&Call::CreateTexture(Format::R8, Size::new(2048, 48))),
        "the boxes' texture"
    );
    s.text(5, 1, "A", plain());
    let damage = g.frame(&mut s, &st);
    assert!(
        covers(&damage, cell(5, 1)) && covers(&damage, cell(5, 0)),
        "{damage:?}"
    );
    assert!(!covers(&damage, cell(4, 0)), "{damage:?}");
    assert_same(&g, &whole(&s, &st), "after a keystroke");
}

// ADR-1306 D4: where a descender from the row above meets an accent from
// the row below, the row below's ink lies on top.
// covers: text.overhang
#[test]
fn a_descender_meeting_an_accent_draws_the_row_below_on_top() {
    let mut s = screen(5, 3);
    s.text(2, 0, "G", style(rgb("e04040"), rgb(BG)));
    s.text(2, 2, "V", style(rgb("40e040"), rgb(BG)));
    let mut g = Gpu::new(&s);
    let st = state(&s, true);
    g.frame(&mut s, &st);
    assert_same(&g, &whole(&s, &st), "the descender and the accent");
    // in the middle row, both reach x 3 and 4 from line 10 to 13
    let (w, at) = (display(&s, Orientation::Normal).width, cell(2, 1));
    let px = g.upright()[((at.y + 11) * w + at.x + 3) as usize];
    assert_eq!(px, 0x40e040, "the accent from the row below lies on top");
    assert_eq!(flags(&g.cells(), 2, 1), 1);
}

// ADR-1306 D6: the letter under the visible cursor keeps its ink above and
// below its row, in its colors without the cursor, in both phases of the
// blink, and its ink beside it stays in its cell; inside the block
// cursor's cell, ink from a neighbour takes the cursor's text color.
// covers: text.overhang
#[test]
fn the_letter_under_the_cursor_keeps_its_ink_above_and_below_in_both_phases() {
    let mut s = screen(5, 3);
    s.text(0, 1, "xQQx", style(rgb("e0c040"), rgb("302020")));
    s.text(1, 0, "xxx", plain());
    s.text(1, 2, "xxx", plain());
    s.cursor.visible = true;
    s.cursor.shape = CursorShape::Block;
    (s.cursor.col, s.cursor.row) = (1, 1);
    let mut g = Gpu::new(&s);
    for (n, on) in [true, false, true].into_iter().enumerate() {
        let st = state(&s, on);
        let damage = g.frame(&mut s, &st);
        assert_same(&g, &whole(&s, &st), &format!("frame {n}, cursor on {on}"));
        if n > 0 {
            // the blink changes the cursor's cell alone
            assert!(!covers(&damage, cell(1, 0)), "frame {n}: {damage:?}");
            assert!(!covers(&damage, cell(0, 1)), "frame {n}: {damage:?}");
        }
    }
    // the letter after the cursor's sends ink into the cursor's cell, which
    // the block cursor marks; the letter under the cursor sends none into
    // the cell before it
    let quads = g.cells();
    assert_eq!(flags(&quads, 1, 1), 3, "the cursor's cell receives, marked");
    assert!(
        !quads
            .iter()
            .any(|q| q.dst[0] == cell(0, 1).x as i32 && q.dst[1] == cell(0, 1).y as i32),
        "the cell before the cursor is not drawn again"
    );
}

// ADR-1306 D7: ink that reaches a cell is part of its text layer, which a
// picture above the text hides and a picture below the text shows: the
// text layer of a cell under pictures carries the ink it receives, drawn
// between the pictures below the text and those above it.
// covers: text.overhang
#[cfg(feature = "images")]
#[test]
fn a_picture_above_the_text_draws_over_the_ink_a_cell_receives() {
    use vtium_image::{FramePlacement, LineKey, NewImage, Place, Protocol, Rows, Screens, Store};

    let mut s = screen(4, 2);
    s.text(1, 1, "A", plain());
    let keys: Vec<LineKey> = (1..=2).map(LineKey::new).collect();
    let view = Rows {
        screen: &keys,
        above: &[],
    };
    let screens = Screens {
        main: view,
        alt: Rows::default(),
    };
    let placements = |z: i32| -> Vec<FramePlacement> {
        let mut store = Store::new();
        store.set_limit(1 << 20);
        let new = NewImage {
            protocol: Protocol::KITTY,
            width: 8,
            height: 16,
            pixels: vec![0xff20_6080; 8 * 16],
            id: 1,
            number: 0,
            transient: false,
            opaque: None,
        };
        let image = store.add_image(new, None, screens).unwrap();
        let place = Place {
            col: 1,
            row: 0,
            cols: 1,
            rows: 1,
            z,
            ..Place::default()
        };
        store.place(image, place, screens, 4).unwrap();
        store.frame(view, 4)
    };
    for (z, text_first) in [(1, true), (-5, false)] {
        let mut g = Gpu::new(&s);
        g.gl.clear_calls();
        let frame = s.frame(Age::NEVER).with_placements(placements(z));
        g.r.draw_gpu(
            &mut g.gl,
            &mut g.pipeline,
            &frame,
            &g.buffer,
            0,
            &state(&s, true),
            &[],
        )
        .unwrap();
        // the draws of the text layers and the pictures, in order
        let order: Vec<(&str, Vec<Quad>)> =
            g.gl.calls()
                .iter()
                .filter_map(|c| match c {
                    Call::Draw { quads, inputs, .. }
                        if inputs.contains(&"under_at") && inputs.contains(&"ink") =>
                    {
                        Some(("text", quads.clone()))
                    }
                    Call::Draw { quads, inputs, .. } if inputs.contains(&"picture") => {
                        Some(("picture", quads.clone()))
                    }
                    _ => None,
                })
                .collect();
        let names: Vec<&str> = order.iter().map(|(n, _)| *n).collect();
        let want = if text_first {
            ["text", "picture"]
        } else {
            ["picture", "text"]
        };
        assert_eq!(names, want, "z {z}");
        let text = &order.iter().find(|(n, _)| *n == "text").unwrap().1;
        assert_eq!(
            flags(text, 1, 0) & 1,
            1,
            "z {z}: the text layer receives the accent"
        );
    }
}

// ADR-1306 D5, D9: a script typing and erasing reaching letters in colors
// of its own, the GPU path's upright frame checked after every frame
// against the CPU path's whole draw, at each turn of the display: a cell
// whose ink comes or goes redraws the neighbours it reached, no cell reads
// a neighbour's ink that has gone, and a display line that ink reaches is
// composed as the CPU path composes it, along a column of the upright
// frame on a display turned a quarter.
// covers: text.overhang
#[test]
fn every_gpu_frame_of_typing_matches_a_whole_draw() {
    for o in [
        Orientation::Normal,
        Orientation::Right,
        Orientation::UpsideDown,
        Orientation::Left,
    ] {
        typing(o);
    }
}

fn typing(o: Orientation) {
    let (cols, rows) = (8usize, 4usize);
    let mut s = screen(cols, rows);
    let mut g = Gpu::turned(&s, o);
    let mut seed = 0x2545_f491u32;
    let mut rand = move |n: u32| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed % n
    };
    for step in 0..80 {
        for _ in 0..=rand(3) {
            let c = ['A', 'G', 'V', 'Q', 'x', ' '][rand(6) as usize];
            let fg = vtium_term::Rgb::new(rand(256) as u8, 120, 220);
            let bg = vtium_term::Rgb::new(16, rand(64) as u8, 16);
            let at = (rand(cols as u32) as usize, rand(rows as u32) as usize);
            s.text(at.0, at.1, &c.to_string(), style(fg, bg));
        }
        let st = state(&s, true);
        g.frame(&mut s, &st);
        assert_same(
            &g,
            &whole_turned(&s, &st, o),
            &format!("{o:?}, step {step}"),
        );
    }
}
