//! Glyph ink drawn past its own cells (ADR-1302, ADR-1306): into the cells
//! beside it, and into the rows above and below. Each cell draws its own
//! background first and the ink its neighbours send it over that, so no
//! background ever paints over another cell's ink; a changed cell damages
//! the neighbours its old or new ink lies in, so every buffer, at every
//! age, equals a whole draw of the same state.

mod common;

use std::ops::Range;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Face, Font, Format, Glyph, Style, Width as FontWidth};
use vtium_render::{
    BlinkStyle, Budgets, Canvas, Config, Depth, DrawState, Orientation, Rect, Renderer, Storage,
    Target, TextBlink,
};
use vtium_term::{Age, Attrs, CursorShape, Rgb};

const CW: u32 = 8;
const CH: u32 = 16;
/// Where the grid's top left lies on the display, which [`size`] centres
/// it on.
const OX: u32 = 1;
const OY: u32 = 2;
const FG: &str = "c0c0c0";
const BG: &str = "101010";

/// A rectangle of ink from `x0`, `y0` to `x1`, `y1`, from the top left of
/// the glyph's first cell, at coverage `a`.
type Ink = (i32, i32, i32, i32, u8);

/// The ink each test character draws, and the cells it spans. A body
/// inside the cell, and ink past it:
/// - `A`, an accent 3 pixels above the row, one pixel of it half covered;
/// - `g`, a descender 3 pixels below it;
/// - `f`, a lean 2 pixels into the cell after it;
/// - `j`, a lean 2 pixels into the cell before it;
/// - `q`, ink in the corner above and before it and below and after it;
/// - `Q`, ink in every direction, the corners too;
/// - `G`, a descender 14 pixels into the row below;
/// - `V`, an accent 6 pixels into the row above;
/// - `W`, two cells wide, with an accent over both and a lean after it;
/// - `x` and `_`, ink inside the cell, the underscore on its last row.
fn shape(c: char) -> (&'static [Ink], u32) {
    const BODY: Ink = (1, 6, 7, 14, 255);
    match c {
        'A' => (&[BODY, (3, -3, 5, 0, 255), (5, -3, 6, -2, 128)], 1),
        'g' => (&[BODY, (2, 14, 4, 19, 255)], 1),
        'f' => (&[BODY, (6, 2, 10, 4, 255)], 1),
        'j' => (&[BODY, (-2, 12, 2, 14, 255)], 1),
        'q' => (&[BODY, (-2, -2, 0, 0, 255), (8, 16, 10, 18, 255)], 1),
        'Q' => (
            &[
                BODY,
                (-3, -3, 11, 0, 255),
                (-3, 16, 11, 19, 255),
                (-3, 0, 0, 16, 200),
                (8, 0, 11, 16, 200),
            ],
            1,
        ),
        'G' => (&[BODY, (2, 10, 5, 30, 255)], 1),
        'V' => (&[BODY, (3, -6, 6, 0, 255)], 1),
        'W' => (
            &[
                (1, 6, 15, 14, 255),
                (2, -2, 14, 0, 255),
                (14, 3, 18, 5, 255),
            ],
            2,
        ),
        'x' => (&[BODY], 1),
        '_' => (&[(0, 15, 8, 16, 255)], 1),
        _ => (&[], 1),
    }
}

/// `c`'s glyph: its pixels inside its cells, and its ink box when its ink
/// passes them.
fn glyph(c: char) -> Option<Glyph> {
    let (rects, cells) = shape(c);
    let (gw, gh) = ((CW * cells) as i32, CH as i32);
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
    let width = if cells == 2 {
        FontWidth::Wide
    } else {
        FontWidth::Narrow
    };
    let g = Glyph::from_pixels(gw as u32, gh as u32, Format::Mask, own)?.with_cells(width);
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

/// An outline face in 8x16 cells whose glyphs reach past their cells as
/// [`shape`] draws them.
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

fn font() -> Font<Reaching> {
    Font::new(Reaching)
}

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// A screen of `cols` by `rows` with an invisible cursor.
fn screen(cols: usize, rows: usize) -> Screen {
    let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
    s.cursor.visible = false;
    s
}

/// The display's size for `s` drawn as `config` turns it: its grid and a
/// border.
fn size(s: &Screen, config: Config) -> (u32, u32) {
    let (w, h) = (s.cols as u32 * CW + 3, s.rows as u32 * CH + 5);
    match config.orientation {
        Orientation::Right | Orientation::Left => (h, w),
        Orientation::Normal | Orientation::UpsideDown => (w, h),
    }
}

/// The pictures a frame draws: kitty placements and sixels, in a build
/// that draws them.
#[cfg(feature = "images")]
type Pics = Vec<vtium_image::FramePlacement>;
#[cfg(not(feature = "images"))]
type Pics = ();

/// `s`'s frame for a display that last drew at `since`, with the
/// selection `sel` and the pictures `pics`.
fn view<'a>(s: &'a Screen, since: Age, sel: &[Range<usize>], pics: &Pics) -> vtium_term::Frame<'a> {
    let f = s.frame(since).with_selection(sel.to_vec());
    #[cfg(feature = "images")]
    let f = f.with_placements(pics.clone());
    #[cfg(not(feature = "images"))]
    let () = *pics;
    f
}

/// `s` drawn whole by a fresh renderer, with the selection `sel`.
fn whole(s: &Screen, sel: &[Range<usize>], st: &DrawState, config: Config) -> Canvas {
    whole_with(s, (sel, &Pics::default()), st, config)
}

/// `s` drawn whole by a fresh renderer, with the selection and pictures.
fn whole_with(
    s: &Screen,
    (sel, pics): (&[Range<usize>], &Pics),
    st: &DrawState,
    config: Config,
) -> Canvas {
    let (w, h) = size(s, config);
    let mut r = Renderer::new(font(), w, h, config);
    let mut c = Canvas::new(w, h, 0);
    r.draw(&view(s, Age::NEVER, sel, pics), &mut c, 0, st);
    c
}

/// Records the rectangles a frame wrote.
struct Recorder {
    canvas: Canvas,
    writes: Vec<Rect>,
}

impl Target for Recorder {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        self.writes.push(Rect {
            x,
            y,
            width: pixels.len() as u32,
            height: 1,
        });
        self.canvas.write(x, y, pixels);
    }
}

/// A display lending its buffers as the frame contract does, as
/// `tests/damage.rs` models it: each frame drawn is checked, its writes
/// inside its damage and its buffer equal to a whole draw.
struct Display {
    r: Renderer<Reaching>,
    since: Age,
    buffers: Vec<Recorder>,
    held: Vec<u64>,
    frames: u64,
    back: usize,
    config: Config,
    sel: Vec<Range<usize>>,
    pics: Pics,
}

impl Display {
    fn new(s: &Screen, buffers: usize, config: Config) -> Display {
        let (w, h) = size(s, config);
        Display {
            r: Renderer::new(font(), w, h, config),
            since: Age::NEVER,
            buffers: (0..buffers)
                .map(|_| Recorder {
                    canvas: Canvas::new(w, h, 0xdead_beef),
                    writes: Vec::new(),
                })
                .collect(),
            held: vec![0; buffers],
            frames: 0,
            back: 0,
            config,
            sel: Vec::new(),
            pics: Pics::default(),
        }
    }

    fn age(&self) -> u32 {
        match self.held[self.back] {
            0 => 0,
            h => (self.frames - h + 1) as u32,
        }
    }

    /// Draws the next frame into the back buffer and swaps it; returns its
    /// damage and the buffer drawn.
    fn draw(&mut self, s: &mut Screen, st: &DrawState) -> (Vec<Rect>, Canvas) {
        let (back, age) = (self.back, self.age());
        let buf = &mut self.buffers[back];
        buf.writes.clear();
        let frame = view(s, self.since, &self.sel, &self.pics);
        let damage = self.r.draw(&frame, buf, age, st).damage.to_vec();
        self.since = s.damage.checkpoint();
        for w in &buf.writes {
            let inside = (w.x..w.x + w.width).all(|x| {
                damage
                    .iter()
                    .any(|d| x >= d.x && w.y >= d.y && x < d.x + d.width && w.y < d.y + d.height)
            });
            assert!(inside, "a write {w:?} outside the damage {damage:?}");
        }
        let want = whole_with(s, (&self.sel, &self.pics), st, self.config);
        same(
            &buf.canvas,
            &want,
            &format!("frame {}, age {age}", self.frames + 1),
        );
        let drawn = buf.canvas.clone();
        if !damage.is_empty() {
            self.frames += 1;
            self.held[back] = self.frames;
            self.back = (back + 1) % self.buffers.len();
        }
        (damage, drawn)
    }
}

/// Panics with the first pixel where `got` differs from `want`.
fn same(got: &Canvas, want: &Canvas, what: &str) {
    if got == want {
        return;
    }
    let w = want.width();
    let i = got
        .pixels()
        .iter()
        .zip(want.pixels())
        .position(|(a, b)| a != b)
        .unwrap();
    panic!(
        "{what}: differs from a whole draw at {},{}: {:06x} for {:06x}",
        i as u32 % w,
        i as u32 / w,
        got.pixels()[i],
        want.pixels()[i]
    );
}

/// The pixel at `x`, `y` of cell `col`, `row`.
fn px(c: &Canvas, (col, row): (u32, u32), (x, y): (u32, u32)) -> u32 {
    c.pixel(OX + col * CW + x, OY + row * CH + y).unwrap()
}

/// `c` as a display pixel.
fn pixel(c: Rgb) -> u32 {
    u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
}

/// The cells the damage `d` covers, as columns and rows.
fn cells(d: &[Rect], s: &Screen) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for row in 0..s.rows as u32 {
        for col in 0..s.cols as u32 {
            let (x, y) = (OX + col * CW, OY + row * CH);
            if d.iter()
                .any(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height)
            {
                out.push((col, row));
            }
        }
    }
    out
}

/// The cells `col0..=col1` by `row0..=row1`.
fn block(cols: Range<u32>, rows: Range<u32>) -> Vec<(u32, u32)> {
    rows.flat_map(|r| cols.clone().map(move |c| (c, r)))
        .collect()
}

const WHITE: Rgb = Rgb::new(250, 250, 250);
const BLUE: Rgb = Rgb::new(20, 40, 200);
const RED: Rgb = Rgb::new(220, 30, 30);
const GREEN: Rgb = Rgb::new(30, 200, 60);

// covers: text.overhang
#[test]
fn an_accent_from_the_row_below_lies_over_the_row_aboves_background() {
    let mut s = screen(6, 3);
    let st = state(&s);
    // the row above on blue, the accented capital in white below it
    for col in 0..6 {
        s.put(col, 0, ' ', false, style(WHITE, BLUE));
    }
    s.put(2, 1, 'A', false, style(WHITE, rgb(BG)));
    let mut d = Display::new(&s, 2, Config::default());
    let (_, first) = d.draw(&mut s, &st);
    // the accent's columns 3 and 4 on the last three lines of the row above
    for y in 13..16 {
        assert_eq!(px(&first, (2, 0), (3, y)), pixel(WHITE), "line {y}");
        assert_eq!(px(&first, (2, 0), (2, y)), pixel(BLUE), "line {y}");
    }
    // its half-covered pixel blends white over blue, never XOR
    let half = px(&first, (2, 0), (5, 13));
    assert_eq!(half, 0x8791e1, "half covered: {half:06x}");
    // the row above changes its background: its cells draw again, under
    // the accent, which stays on top in every buffer
    for col in 0..6 {
        s.put(col, 0, ' ', false, style(WHITE, RED));
    }
    for _ in 0..3 {
        let (_, c) = d.draw(&mut s, &st);
        assert_eq!(px(&c, (2, 0), (3, 14)), pixel(WHITE));
        assert_eq!(px(&c, (2, 0), (2, 14)), pixel(RED));
        s.damage.mark_cell(0, 0);
    }
}

// covers: text.overhang
#[test]
fn where_a_descender_meets_an_accent_the_row_below_lies_on_top() {
    let mut s = screen(4, 3);
    let st = state(&s);
    // a long descender in red from the top row, a tall accent in green
    // from the bottom row, meeting in the blank cell between them
    s.put(1, 0, 'G', false, style(RED, rgb(BG)));
    s.put(1, 2, 'V', false, style(GREEN, rgb(BG)));
    let mut d = Display::new(&s, 1, Config::default());
    let (_, c) = d.draw(&mut s, &st);
    // the descender alone: columns 2 to 4 of the middle row's lines 0 to 9
    assert_eq!(px(&c, (1, 1), (2, 5)), pixel(RED));
    // the accent alone: column 5, lines 10 to 15
    assert_eq!(px(&c, (1, 1), (5, 12)), pixel(GREEN));
    // both: columns 3 and 4, lines 10 to 13, the row below's on top
    for y in 10..14 {
        assert_eq!(px(&c, (1, 1), (3, y)), pixel(GREEN), "line {y}");
    }
}

// covers: text.overhang
#[test]
fn a_changed_cell_redraws_exactly_its_block() {
    let mut s = screen(7, 5);
    let st = state(&s);
    s.text(0, 0, "xxxxxxx", style(rgb(FG), rgb(BG)));
    s.text(0, 4, "xxxxxx", style(rgb(FG), rgb(BG)));
    // the font has drawn a glyph whose ink passes its cells before the
    // frames measured, so none of them is the frame that draws whole for it
    s.put(6, 4, 'A', false, style(rgb(FG), rgb(BG)));
    let mut d = Display::new(&s, 1, Config::default());
    d.draw(&mut s, &st);
    // a glyph whose ink reaches every cell around it
    s.put(3, 2, 'Q', false, style(WHITE, rgb(BG)));
    let (damage, _) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s), block(2..5, 1..4));
    // it becomes one whose ink stays in its cell: the old block repairs
    s.put(3, 2, 'x', false, style(WHITE, rgb(BG)));
    let (damage, _) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s), block(2..5, 1..4));
    // a change of a cell whose ink stays in it damages it alone
    s.put(3, 2, '_', false, style(WHITE, rgb(BG)));
    let (damage, _) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s), vec![(3, 2)]);
    // an accent reaches the cell above, and only it
    s.put(3, 2, 'A', false, style(WHITE, rgb(BG)));
    let (damage, _) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s), vec![(3, 1), (3, 2)]);
    // the corners: up and before, down and after
    s.put(3, 2, 'q', false, style(WHITE, rgb(BG)));
    let (damage, _) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s), block(2..5, 1..4));
}

// covers: text.overhang
#[test]
fn a_cell_whose_reach_shrinks_repairs_its_old_block_in_every_buffer() {
    let mut s = screen(7, 5);
    let st = state(&s);
    for buffers in 1..=3 {
        let mut d = Display::new(&s, buffers, Config::default());
        for c in ['Q', 'A', 'q', 'g', 'f', 'j', 'x', ' ', 'W', 'Q', ' '] {
            s.put(3, 2, c, c == 'W', style(WHITE, rgb(BG)));
            d.draw(&mut s, &st);
            // and a frame with no change for the older buffers
            d.draw(&mut s, &st);
        }
    }
}

/// Scrolls rows `rows` of `s` up by one, as a scroll region does: the
/// last row of the region comes in blank, and every row in it changed.
fn scroll_up(s: &mut Screen, rows: Range<usize>) {
    let blank = vtium_term::Line::new(vtium_term::LineId::new(900 + rows.end as u64), s.cols);
    s.lines[rows.clone()].rotate_left(1);
    s.lines[rows.end - 1] = blank;
    s.damage.mark_rows(rows);
}

/// Scrolls rows `rows` of `s` down by one.
fn scroll_down(s: &mut Screen, rows: Range<usize>) {
    let blank = vtium_term::Line::new(vtium_term::LineId::new(800 + rows.start as u64), s.cols);
    s.lines[rows.clone()].rotate_right(1);
    s.lines[rows.start] = blank;
    s.damage.mark_rows(rows);
}

// covers: text.overhang
#[test]
fn damage_follows_ink_across_a_scroll_and_a_scroll_regions_margins() {
    for buffers in 1..=3 {
        let mut s = screen(6, 7);
        let st = state(&s);
        s.text(0, 0, "xxxxxx", style(rgb(FG), BLUE));
        s.text(0, 6, "xxxxxx", style(rgb(FG), BLUE));
        let mut d = Display::new(&s, buffers, Config::default());
        // accents and descenders at the region's margins, rows 1 and 5
        s.text(0, 1, "AgQAqG", style(WHITE, rgb(BG)));
        s.text(0, 5, "gAQVfj", style(WHITE, rgb(BG)));
        s.text(0, 3, "AAAggg", style(GREEN, rgb(BG)));
        d.draw(&mut s, &st);
        for step in 0..8 {
            if step % 2 == 0 {
                scroll_up(&mut s, 1..6);
            } else {
                scroll_down(&mut s, 1..6);
            }
            s.text(0, 1 + step % 5, "QAgjf", style(RED, rgb(BG)));
            d.draw(&mut s, &st);
            d.draw(&mut s, &st);
        }
        // the whole screen scrolls
        for _ in 0..4 {
            scroll_up(&mut s, 0..7);
            s.text(0, 6, "AQgVG", style(WHITE, rgb(BG)));
            d.draw(&mut s, &st);
        }
    }
}

// covers: text.overhang
#[test]
fn the_letter_under_the_cursor_keeps_its_ink_above_and_below_in_both_phases() {
    let mut s = screen(5, 3);
    s.put(2, 1, 'Q', false, style(WHITE, rgb(BG)));
    s.cursor.visible = true;
    s.cursor.col = 2;
    s.cursor.row = 1;
    s.cursor.shape = CursorShape::Block;
    let mut d = Display::new(&s, 1, Config::default());
    let on = state(&s);
    let off = DrawState {
        cursor_on: false,
        ..on
    };
    let (_, a) = d.draw(&mut s, &on);
    // above and below in the colors it has without the cursor
    assert_eq!(px(&a, (2, 0), (4, 14)), pixel(WHITE));
    assert_eq!(px(&a, (2, 2), (4, 1)), pixel(WHITE));
    // nothing beside it, the corners included
    assert_eq!(px(&a, (3, 1), (1, 8)), pixel(rgb(BG)));
    assert_eq!(px(&a, (1, 1), (6, 8)), pixel(rgb(BG)));
    assert_eq!(px(&a, (3, 0), (1, 14)), pixel(rgb(BG)));
    // the blink changes the cursor's cell alone
    for st in [off, on, off, on] {
        let (damage, c) = d.draw(&mut s, &st);
        assert_eq!(cells(&damage, &s), vec![(2, 1)]);
        for cell in [(2, 0), (2, 2), (3, 1), (1, 1)] {
            for y in 0..CH {
                for x in 0..CW {
                    assert_eq!(px(&c, cell, (x, y)), px(&a, cell, (x, y)), "{cell:?}");
                }
            }
        }
    }
    // the cursor leaves: the letter leans into the cells beside it again
    s.cursor.col = 0;
    s.damage.mark_cell(0, 0);
    let (damage, c) = d.draw(&mut s, &on);
    assert!(cells(&damage, &s).contains(&(3, 1)));
    assert_eq!(px(&c, (3, 1), (1, 8)), 0xc7c7c7);
    d.draw(&mut s, &on);
}

// covers: text.overhang
#[test]
fn a_blinking_letters_ink_stays_in_its_cell_in_both_phases() {
    let mut s = screen(5, 3);
    let mut blink = style(WHITE, rgb(BG));
    blink.attrs.insert(Attrs::BLINK);
    s.put(2, 1, 'Q', false, blink);
    let config = Config {
        blink: TextBlink {
            style: BlinkStyle::Full,
            decorations: false,
        },
        ..Config::default()
    };
    let mut d = Display::new(&s, 2, config);
    let shown = state(&s);
    let hidden = DrawState {
        blink_hidden: true,
        ..shown
    };
    for st in [shown, hidden, shown, hidden] {
        let (_, c) = d.draw(&mut s, &st);
        // the accent above and the lean beside it are cut in both phases
        assert_eq!(px(&c, (2, 0), (4, 14)), pixel(rgb(BG)));
        assert_eq!(px(&c, (3, 1), (1, 8)), pixel(rgb(BG)));
    }
    // with blinking text drawn steady, the letter's ink reaches as any
    let steady = Config {
        blink: TextBlink {
            style: BlinkStyle::Off,
            decorations: false,
        },
        ..Config::default()
    };
    let c = whole(&s, &[], &hidden, steady);
    assert_eq!(px(&c, (2, 0), (4, 14)), pixel(WHITE));
}

// covers: text.overhang
#[test]
fn an_accent_from_the_row_below_crosses_an_underline_cursor() {
    let mut s = screen(5, 3);
    s.put(2, 1, 'x', false, style(GREEN, rgb(BG)));
    s.put(2, 2, 'A', false, style(WHITE, rgb(BG)));
    s.cursor.visible = true;
    s.cursor.col = 2;
    s.cursor.row = 1;
    s.cursor.shape = CursorShape::Underline;
    let mut d = Display::new(&s, 1, Config::default());
    let st = state(&s);
    let (_, c) = d.draw(&mut s, &st);
    // the cursor's line is the cell's last row, in the cell's green
    assert_eq!(px(&c, (2, 1), (1, 15)), pixel(GREEN));
    // the accent crosses it
    assert_eq!(px(&c, (2, 1), (3, 15)), pixel(WHITE));
}

// ADR-1306 D6: ink takes the colors of the cell it came from, so the
// accent of a reverse-video capital draws in the reversed foreground, its
// cell's background color, over the unmarked cell above
// covers: text.overhang
#[test]
fn a_reversed_letters_accent_takes_its_reversed_foreground_in_the_cell_above() {
    let mut s = screen(3, 3);
    let st = state(&s);
    let mut reversed = style(WHITE, BLUE);
    reversed.attrs.insert(Attrs::INVERSE);
    s.put(1, 2, 'A', false, reversed);
    let mut d = Display::new(&s, 1, Config::default());
    let (_, c) = d.draw(&mut s, &st);
    // the capital's own cell is reversed: white behind the letter
    assert_eq!(px(&c, (1, 2), (0, 0)), pixel(WHITE));
    // its accent, over the unmarked cell above, is blue, the reversed
    // foreground, on that cell's own background
    assert_eq!(px(&c, (1, 1), (3, 14)), pixel(BLUE));
    assert_eq!(px(&c, (1, 1), (0, 14)), pixel(rgb(BG)));
}

// covers: text.overhang
#[test]
fn a_mark_takes_over_the_ink_inside_it_and_ink_outside_it_keeps_its_colors() {
    let mut s = screen(5, 3);
    let st = state(&s);
    s.text(0, 1, "xxxxx", style(GREEN, BLUE));
    s.put(2, 2, 'A', false, style(WHITE, rgb(BG)));
    let mut d = Display::new(&s, 2, Config::default());
    d.draw(&mut s, &st);
    // the selection covers the accented capital, not the row above: its
    // accent keeps the color it has unselected
    d.sel = vec![0..0, 0..0, 0..5];
    s.damage.mark_rows(2..3);
    let (_, c) = d.draw(&mut s, &st);
    assert_eq!(px(&c, (2, 1), (3, 14)), pixel(WHITE));
    // the selection covers the row above and not the capital: the accent
    // there takes the selected cell's text color, its inverted blue
    d.sel = vec![0..0, 0..5];
    s.damage.mark_rows(1..3);
    let (_, c) = d.draw(&mut s, &st);
    assert_eq!(px(&c, (2, 1), (3, 14)), pixel(BLUE));
    assert_eq!(px(&c, (2, 1), (0, 14)), pixel(GREEN));
    // a block cursor on the cell above does the same
    d.sel = Vec::new();
    s.cursor.visible = true;
    s.cursor.col = 2;
    s.cursor.row = 1;
    s.cursor.shape = CursorShape::Block;
    s.damage.mark_rows(1..3);
    let (_, c) = d.draw(&mut s, &st);
    assert_eq!(px(&c, (2, 1), (3, 14)), pixel(BLUE));
    d.draw(&mut s, &st);
}

// covers: text.overhang
#[test]
fn every_frame_matches_a_whole_draw_while_reaching_glyphs_are_typed_and_erased() {
    for buffers in 1..=3 {
        for blink in [BlinkStyle::Full, BlinkStyle::Partial, BlinkStyle::Off] {
            typing(
                buffers,
                Config {
                    blink: TextBlink {
                        style: blink,
                        decorations: true,
                    },
                    ..Config::default()
                },
            );
        }
    }
}

// Turned, at 1 bit, with no glyph cache and with each storage saving off.
// covers: text.overhang
#[test]
fn every_frame_matches_a_whole_draw_turned_at_1_bit_and_uncached() {
    let configs = [
        Orientation::Right,
        Orientation::UpsideDown,
        Orientation::Left,
    ]
    .map(|orientation| Config {
        orientation,
        ..Config::default()
    });
    let more = [
        Config {
            depth: Depth::One,
            ..Config::default()
        },
        Config {
            budgets: Some(Budgets::glyphs(0)),
            ..Config::default()
        },
        Config {
            storage: Storage {
                bits: false,
                unscaled: false,
            },
            ..Config::default()
        },
    ];
    for config in configs.into_iter().chain(more) {
        typing(2, config);
    }
}

/// A script typing and erasing glyphs whose ink reaches past their cells,
/// moving the cursor and the selection and scrolling, drawn through
/// `buffers` buffers as `config` says, each frame checked against a whole
/// draw.
fn typing(buffers: usize, config: Config) {
    let (cols, rows) = (12usize, 6usize);
    {
        {
            let mut s = screen(cols, rows);
            s.cursor.visible = true;
            let mut d = Display::new(&s, buffers, config);
            let mut seed = 0x9e37_79b9u32 ^ buffers as u32;
            let mut rand = move |n: u32| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed % n
            };
            const CHARS: &[char] = &['A', 'g', 'f', 'j', 'q', 'Q', 'G', 'V', 'W', 'x', '_', ' '];
            for step in 0..120 {
                for _ in 0..rand(4) {
                    let (col, row) = (rand(cols as u32) as usize, rand(rows as u32) as usize);
                    let c = CHARS[rand(CHARS.len() as u32) as usize];
                    let mut st = style(
                        Rgb::new(rand(256) as u8, 200, 100),
                        Rgb::new(0, rand(256) as u8, 40),
                    );
                    if rand(5) == 0 {
                        st.attrs.insert(Attrs::BLINK);
                    }
                    if rand(4) == 0 {
                        st.attrs.set_underline(vtium_term::UnderlineStyle::Single);
                    }
                    if rand(6) == 0 {
                        st.attrs.insert(Attrs::STRIKE);
                    }
                    if rand(8) == 0 {
                        st.fg = st.bg;
                    }
                    let wide = c == 'W' && col + 1 < cols;
                    s.put(col, row, c, wide, st);
                }
                if rand(3) == 0 {
                    s.cursor.col = rand(cols as u32) as usize;
                    s.cursor.row = rand(rows as u32) as usize;
                    s.cursor.visible = rand(5) != 0;
                    s.cursor.shape = match rand(3) {
                        0 => CursorShape::Block,
                        1 => CursorShape::Underline,
                        _ => CursorShape::Bar,
                    };
                }
                if rand(5) == 0 {
                    let row = rand(rows as u32) as usize;
                    let (a, b) = (rand(cols as u32) as usize, rand(cols as u32) as usize);
                    let mut sel = vec![0..0; rows];
                    sel[row] = a.min(b)..a.max(b) + 1;
                    for r in 0..rows {
                        s.damage.mark_rows(r..r + 1);
                    }
                    d.sel = sel;
                }
                if rand(9) == 0 {
                    scroll_up(&mut s, 1..rows - 1);
                }
                let st = DrawState {
                    cursor_on: rand(4) != 0,
                    blink_hidden: (step / 5) % 2 == 1,
                    pointer: (rand(4) == 0)
                        .then(|| (rand(cols as u32 * CW), rand(rows as u32 * CH))),
                    ..state(&s)
                };
                if rand(15) == 0 {
                    d.held[d.back] = 0;
                }
                d.draw(&mut s, &st);
            }
        }
    }
}

// covers: text.overhang
#[test]
fn ink_past_the_grids_edges_is_cut() {
    let mut s = screen(3, 2);
    s.put(0, 0, 'Q', false, style(WHITE, rgb(BG)));
    s.put(2, 1, 'Q', false, style(WHITE, rgb(BG)));
    let st = state(&s);
    let c = whole(&s, &[], &st, Config::default());
    let (w, h) = size(&s, Config::default());
    let border = c.pixel(w - 1, h - 1).unwrap();
    // the border around the grid shows no ink
    let (gx, gy) = (OX..OX + s.cols as u32 * CW, OY..OY + s.rows as u32 * CH);
    for y in 0..h {
        for x in 0..w {
            if !gx.contains(&x) || !gy.contains(&y) {
                assert_eq!(c.pixel(x, y).unwrap(), border, "{x},{y}");
            }
        }
    }
    // and the neighbours inside it show the ink
    assert_eq!(px(&c, (1, 0), (1, 8)), 0xc7c7c7);
    assert_eq!(px(&c, (0, 1), (4, 1)), pixel(WHITE));
}

// covers: text.overhang
#[test]
fn glyphs_whose_ink_stays_in_their_cells_damage_no_neighbour() {
    let mut s = screen(6, 3);
    let st = state(&s);
    let mut d = Display::new(&s, 1, Config::default());
    d.draw(&mut s, &st);
    for (i, c) in "x_x_".chars().enumerate() {
        s.put(1 + i, 1, c, false, style(WHITE, rgb(BG)));
        let (damage, _) = d.draw(&mut s, &st);
        assert_eq!(cells(&damage, &s), vec![(1 + i as u32, 1)]);
    }
}

/// Pictures made through the image store directly, as `tests/pictures.rs`
/// makes them: screen row `r` is line `r + 1`.
#[cfg(feature = "images")]
mod pictures {
    use vtium_image::{FramePlacement, LineKey, NewImage, Place, Protocol, Rows, Screens, Store};

    pub struct Console {
        store: Store,
        keys: Vec<LineKey>,
        cols: usize,
    }

    impl Console {
        pub fn new(cols: usize, rows: usize) -> Console {
            let mut store = Store::new();
            store.set_limit(1 << 30);
            Console {
                store,
                keys: (1..=rows as u64).map(LineKey::new).collect(),
                cols,
            }
        }

        fn screens(&self) -> Screens<'_> {
            Screens {
                main: Rows {
                    screen: &self.keys,
                    above: &[],
                },
                alt: Rows::default(),
            }
        }

        /// A picture of one color, `argb`, `w` by `h` pixels, placed at
        /// `col`, `row` at `z`: a kitty picture, or a sixel.
        pub fn picture(
            &mut self,
            kitty: bool,
            argb: u32,
            (w, h): (u32, u32),
            (col, row, z): (u32, u32, i32),
        ) {
            let new = NewImage {
                protocol: if kitty {
                    Protocol::KITTY
                } else {
                    Protocol::SIXEL
                },
                width: w,
                height: h,
                pixels: vec![argb; (w * h) as usize],
                id: u32::from(kitty),
                number: 0,
                transient: false,
                opaque: None,
            };
            let keys = self.keys.clone();
            let screens = || Screens {
                main: Rows {
                    screen: &keys,
                    above: &[],
                },
                alt: Rows::default(),
            };
            let img = self
                .store
                .add_image(new, None, screens())
                .expect("the store takes it");
            let place = Place {
                col,
                row,
                z,
                ..Place::default()
            };
            self.store
                .place(img, place, screens(), self.cols)
                .expect("the store places it");
        }

        pub fn frame(&self) -> Vec<FramePlacement> {
            let Screens { main, .. } = self.screens();
            self.store.frame(main, self.cols)
        }
    }
}

// Received ink is part of the text layer of the cell it lands in (ADR-1306
// D7): a picture below the text shows it on top, a picture at z 0 or more
// and a sixel hide it.
// covers: text.overhang
#[cfg(feature = "images")]
#[test]
fn a_picture_below_the_text_shows_received_ink_and_one_above_it_hides_it() {
    const PIC: u32 = 0xff20_c0c0;
    let mut s = screen(5, 3);
    let st = state(&s);
    s.put(2, 2, 'A', false, style(WHITE, rgb(BG)));
    // each picture covers the whole cell, and then its first 6 columns,
    // which the accent's columns 3 to 5 lie under, so that the cell
    // composes its layers line by line
    let cases = [
        (true, i32::MIN, true),
        (true, -1, true),
        (true, 0, false),
        (false, 0, false),
    ];
    for ((kitty, z, shows), width) in cases.into_iter().flat_map(|c| [(c, CW), (c, 6)]) {
        let mut con = pictures::Console::new(5, 3);
        con.picture(kitty, PIC, (width, CH), (2, 1, z));
        let pics = con.frame();
        assert!(!pics.is_empty(), "{kitty} {z}: the picture is in the frame");
        let c = whole_with(&s, (&[], &pics), &st, Config::default());
        let accent = px(&c, (2, 1), (3, 14));
        let beside = px(&c, (2, 1), (1, 14));
        assert_eq!(
            beside,
            PIC & 0xff_ffff,
            "kitty {kitty} at z {z}: the picture shows"
        );
        let want = if shows { pixel(WHITE) } else { PIC & 0xff_ffff };
        assert_eq!(accent, want, "kitty {kitty} at z {z}: the accent");
    }
}

// A picture that appears, changes layer and goes under an accent from the
// row below leaves every buffer equal to a whole draw.
// covers: text.overhang
#[cfg(feature = "images")]
#[test]
fn a_picture_coming_and_going_under_received_ink_matches_a_whole_draw() {
    for buffers in 1..=3 {
        let mut s = screen(6, 4);
        let st = state(&s);
        s.text(1, 2, "AQV", style(WHITE, rgb(BG)));
        s.text(1, 0, "gGq", style(GREEN, rgb(BG)));
        let mut d = Display::new(&s, buffers, Config::default());
        d.draw(&mut s, &st);
        for (step, z) in [
            Some(-1),
            Some(0),
            None,
            Some(i32::MIN),
            Some(-1),
            None,
            None,
        ]
        .into_iter()
        .enumerate()
        {
            let mut con = pictures::Console::new(6, 4);
            if let Some(z) = z {
                let px = 0xff30_6090 + step as u32;
                con.picture(true, px, (2 * CW + 3, CH + 5), (1, 1, z));
            }
            d.pics = con.frame();
            d.draw(&mut s, &st);
            d.draw(&mut s, &st);
        }
    }
}

// covers: text.overhang
#[test]
fn at_1_bit_received_ink_is_the_opposite_of_its_cells_background() {
    let mut s = screen(4, 3);
    let st = state(&s);
    // a light background above the first accent, a dark one above the
    // second; the accents are as light as the light background
    s.put(1, 0, ' ', false, style(WHITE, WHITE));
    s.put(2, 0, ' ', false, style(WHITE, rgb(BG)));
    s.put(1, 1, 'A', false, style(WHITE, rgb(BG)));
    s.put(2, 1, 'A', false, style(WHITE, rgb(BG)));
    let config = Config {
        depth: Depth::One,
        ..Config::default()
    };
    let c = whole(&s, &[], &st, config);
    assert_eq!(px(&c, (1, 0), (1, 14)), 0xffffff, "the light background");
    assert_eq!(px(&c, (1, 0), (3, 14)), 0x000000, "the accent on it");
    assert_eq!(px(&c, (2, 0), (1, 14)), 0x000000, "the dark background");
    assert_eq!(px(&c, (2, 0), (3, 14)), 0xffffff, "the accent on it");
}

// Until the font draws a glyph whose ink passes its cells, no reach is
// read; the frame that draws the first one draws the whole grid again with
// every reach read, and the frames after it damage only what changed.
// covers: text.overhang
#[test]
fn the_first_glyph_whose_ink_passes_its_cells_redraws_the_grid_once() {
    let mut s = screen(5, 3);
    let st = state(&s);
    s.text(0, 0, "xxxxx", style(rgb(FG), rgb(BG)));
    let mut d = Display::new(&s, 1, Config::default());
    d.draw(&mut s, &st);
    s.put(2, 2, 'x', false, style(WHITE, rgb(BG)));
    let (damage, _) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s), vec![(2, 2)]);
    s.put(2, 2, 'A', false, style(WHITE, rgb(BG)));
    let (damage, c) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s).len(), 15, "the whole grid");
    assert_eq!(px(&c, (2, 1), (3, 14)), pixel(WHITE));
    s.put(2, 2, 'x', false, style(WHITE, rgb(BG)));
    let (damage, _) = d.draw(&mut s, &st);
    assert_eq!(cells(&damage, &s), vec![(2, 1), (2, 2)]);
}

// ADR-1306 D6: ink invisible in its own cell, its foreground the same as
// its background, is invisible everywhere.
// covers: text.overhang
#[test]
fn ink_its_own_background_hides_reaches_no_other_cell() {
    let mut s = screen(4, 3);
    let st = state(&s);
    s.put(1, 2, 'Q', false, style(BLUE, BLUE));
    s.put(2, 2, 'A', false, style(WHITE, rgb(BG)));
    let c = whole(&s, &[], &st, Config::default());
    // the visible capital's accent shows, so reaches are read
    assert_eq!(px(&c, (2, 1), (3, 14)), pixel(WHITE));
    // the hidden letter's ink reaches neither the row above nor the side
    for (cell, at) in [((1, 1), (4, 14)), ((0, 2), (6, 8)), ((0, 1), (6, 14))] {
        assert_eq!(px(&c, cell, at), pixel(rgb(BG)), "{cell:?}");
    }
}

// A wide glyph's ink past its second cell reaches the cell after it.
// covers: text.overhang
#[test]
fn a_wide_glyphs_ink_reaches_past_its_second_cell() {
    let mut s = screen(5, 2);
    let st = state(&s);
    s.put(1, 1, 'W', true, style(WHITE, rgb(BG)));
    let c = whole(&s, &[], &st, Config::default());
    // its lean, columns 14 to 17 of its cells at lines 3 and 4, lies in
    // the first two columns of cell 3
    assert_eq!(px(&c, (3, 1), (1, 3)), pixel(WHITE));
    // and its accent over both cells in the row above
    assert_eq!(px(&c, (2, 0), (4, 15)), pixel(WHITE));
}

/// A face whose `A` gains an accent above its row once `arrived` is set,
/// as a glyph from the helper replaces the built-in drawing.
struct Arriving(std::rc::Rc<std::cell::Cell<bool>>);

impl Face for Arriving {
    fn cell(&self) -> Cell {
        Reaching.cell()
    }
    fn size_step(&self) -> u32 {
        1
    }
    fn has_glyph(&self, c: char) -> bool {
        Reaching.has_glyph(c)
    }
    fn draw(&self, cluster: &[char], style: Style) -> Option<Glyph> {
        match cluster.first() {
            Some('A') if !self.0.get() => glyph('x'),
            _ => Reaching.draw(cluster, style),
        }
    }
}

// ADR-1306 D5: a glyph arriving from the helper in place of the drawing
// before it is an ink change, and the reach the renderer read for the old
// glyph is read again.
// covers: text.overhang
#[test]
fn a_glyph_that_arrives_with_ink_past_its_cell_draws_it() {
    let arrived = std::rc::Rc::new(std::cell::Cell::new(false));
    let mut s = screen(4, 3);
    let st = state(&s);
    let (w, h) = size(&s, Config::default());
    let mut r = Renderer::new(
        Font::new(Arriving(arrived.clone())),
        w,
        h,
        Config::default(),
    );
    let mut c = Canvas::new(w, h, 0);
    // the font has drawn a reaching glyph, so the old A's reach is read
    s.put(0, 2, 'g', false, style(WHITE, rgb(BG)));
    s.put(2, 2, 'A', false, style(WHITE, rgb(BG)));
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &st);
    assert_eq!(px(&c, (2, 1), (3, 14)), pixel(rgb(BG)), "no accent yet");
    let since = s.damage.checkpoint();
    arrived.set(true);
    r.glyphs_changed(&['A']);
    let drawn = r.draw(&s.frame(since), &mut c, 1, &st);
    assert!(!drawn.damage.is_empty());
    assert_eq!(px(&c, (2, 1), (3, 14)), pixel(WHITE), "the accent arrived");
}
