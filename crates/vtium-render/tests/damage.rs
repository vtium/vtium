//! What each frame redraws and reports as damage (vtium/vtium#16's gates),
//! under the frame contract with the displays: a buffer lent at age n gets
//! the cells that changed in the last n frames, at age 0 or past the
//! renderer's history the whole picture, and every buffer drawn equals a
//! whole draw of the same state whatever order the buffers come in.

mod common;

use common::{Screen, WriteOnly, rgb, style, unifont};
use vtium_render::{
    BlinkStyle, Canvas, Config, DrawState, Orientation, Rect, Renderer, Target, TextBlink,
};
use vtium_term::{Age, Attrs, CursorShape, Rgb};

const FG: &str = "c0c0c0";
const BG: &str = "101010";

fn state(s: &Screen) -> DrawState {
    DrawState {
        cursor_on: true,
        foreground: s.palette.fg(),
        background: s.palette.bg(),
        ..DrawState::default()
    }
}

/// Records the rectangles a frame wrote, to check them against its damage.
struct Recorder {
    canvas: WriteOnly,
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

/// Whether every pixel of `w` lies in some rectangle of `damage`: a run
/// may cross from one rectangle into the next, as the pointer's does on a
/// turned display.
fn inside(w: &Rect, damage: &[Rect]) -> bool {
    (w.y..w.y + w.height).all(|y| {
        (w.x..w.x + w.width).all(|x| {
            damage
                .iter()
                .any(|d| x >= d.x && y >= d.y && x < d.x + d.width && y < d.y + d.height)
        })
    })
}

/// `s` drawn whole by a fresh renderer.
fn fresh(s: &Screen, st: &DrawState, width: u32, height: u32, config: Config) -> Canvas {
    let mut r = Renderer::new(unifont(), width, height, config);
    let mut c = Canvas::new(width, height, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, st);
    c
}

/// Panics with the first pixel where `got` differs from `want`.
fn same_pixels(got: &Canvas, want: &Canvas, what: &str) {
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

/// A display lending its buffers as the frame contract does. The back
/// buffer's age is the frames since it was last drawn, 1 for the frame
/// before and 0 when unknown. A frame with damage is swapped; a swap that
/// fails keeps the back buffer, which then holds the failed frame. A frame
/// with no damage is not swapped and does not count. Every frame drawn is
/// checked: its writes lie inside its damage and the buffer equals a whole
/// draw of the same state.
struct Display {
    r: Renderer<vtium_font::Unifont>,
    since: Age,
    buffers: Vec<Recorder>,
    /// The frame each buffer holds, 0 when unknown.
    held: Vec<u64>,
    /// Frames swapped, or tried.
    frames: u64,
    back: usize,
    width: u32,
    height: u32,
    config: Config,
}

impl Display {
    fn new(buffers: usize, width: u32, height: u32) -> Display {
        Display::turned(buffers, width, height, Orientation::Normal)
    }

    fn turned(buffers: usize, width: u32, height: u32, turn: Orientation) -> Display {
        let config = Config {
            orientation: turn,
            ..Config::default()
        };
        Display::with(buffers, width, height, config)
    }

    fn with(buffers: usize, width: u32, height: u32, config: Config) -> Display {
        Display {
            r: Renderer::new(unifont(), width, height, config),
            since: Age::NEVER,
            buffers: (0..buffers)
                .map(|_| Recorder {
                    canvas: WriteOnly::new(width, height),
                    writes: Vec::new(),
                })
                .collect(),
            held: vec![0; buffers],
            frames: 0,
            back: 0,
            width,
            height,
            config,
        }
    }

    fn age(&self) -> u32 {
        match self.held[self.back] {
            0 => 0,
            h => (self.frames - h + 1) as u32,
        }
    }

    /// Draws the next frame into the back buffer and swaps it; returns its
    /// damage.
    fn draw(&mut self, s: &mut Screen, st: &DrawState) -> Vec<Rect> {
        self.frame(s, st, true)
    }

    /// Draws the next frame into the back buffer, whose swap fails.
    fn draw_failing(&mut self, s: &mut Screen, st: &DrawState) -> Vec<Rect> {
        self.frame(s, st, false)
    }

    fn frame(&mut self, s: &mut Screen, st: &DrawState, presents: bool) -> Vec<Rect> {
        let (back, age, frame) = (self.back, self.age(), self.frames + 1);
        let buf = &mut self.buffers[back];
        buf.writes.clear();
        let damage = self
            .r
            .draw(&s.frame(self.since), buf, age, st)
            .damage
            .to_vec();
        self.since = s.damage.checkpoint();
        for w in &buf.writes {
            assert!(
                inside(w, &damage),
                "frame {frame} at age {age} wrote {w:?} outside its damage {damage:?}"
            );
        }
        let want = fresh(s, st, self.width, self.height, self.config);
        same_pixels(
            buf.canvas.canvas(),
            &want,
            &format!("frame {frame}, buffer {back} at age {age}"),
        );
        if !damage.is_empty() {
            self.frames += 1;
            self.held[back] = self.frames;
            if presents {
                self.back = (back + 1) % self.buffers.len();
            }
        }
        damage
    }

    /// The display lost its buffers' contents, as when it powers on.
    fn lose_all(&mut self) {
        self.held.fill(0);
    }

    /// The display lost the back buffer's contents.
    fn lose_back(&mut self) {
        self.held[self.back] = 0;
    }

    /// The back buffer, which an idle frame drew into and did not swap.
    fn back(&self) -> &Recorder {
        &self.buffers[self.back]
    }
}

fn area(damage: &[Rect]) -> u64 {
    damage.iter().map(Rect::area).sum()
}

fn cell(col: u32, row: u32, w: u32, h: u32, cols: u32, rows: u32) -> Rect {
    // the grid is centred on the display
    let (ox, oy) = ((w - cols * 8) / 2, (h - rows * 16) / 2);
    Rect {
        x: ox + col * 8,
        y: oy + row * 16,
        width: 8,
        height: 16,
    }
}

fn prompt() -> Screen {
    let mut s = Screen::new(20, 4, rgb(FG), rgb(BG));
    s.text(0, 0, "$ ls", style(rgb("00ff00"), rgb(BG)));
    s.text(0, 1, "a b c", style(rgb(FG), rgb(BG)));
    s.text(0, 2, "$ ", style(rgb("00ff00"), rgb(BG)));
    s.cursor.row = 2;
    s.cursor.col = 2;
    s.cursor.visible = true;
    s
}

const PROMPT: (u32, u32) = (20 * 8 + 6, 4 * 16 + 4);

fn whole(w: u32, h: u32) -> Vec<Rect> {
    vec![Rect {
        x: 0,
        y: 0,
        width: w,
        height: h,
    }]
}

// aaronsb/kmscon#156: typing one character at a prompt damages the cell it
// wrote and the cell the cursor moved to, on this frame and the next
// buffer's, and nothing after
// covers: platform.compositor
#[test]
fn a_keystroke_damages_only_the_cells_it_changes() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    d.draw(&mut s, &st);
    assert!(
        d.draw(&mut s, &st).is_empty(),
        "an idle frame draws nothing"
    );

    s.put(2, 2, 'x', false, style(rgb(FG), rgb(BG)));
    s.cursor.col = 3;
    let want = vec![Rect {
        width: 16,
        ..cell(2, 2, w, h, 20, 4)
    }];
    assert_eq!(d.draw(&mut s, &st), want);
    assert_eq!(d.draw(&mut s, &st), want, "the other buffer");
    assert!(d.draw(&mut s, &st).is_empty());
    assert!(d.back().writes.is_empty(), "an idle frame writes nothing");
}

// aaronsb/kmscon#81: after a resize, the first two frames draw everything,
// one into each buffer, and the third only what changed
// covers: platform.compositor
#[test]
fn a_full_redraw_reaches_each_buffer_once() {
    let mut s = prompt();
    let (w, h) = (20 * 8 + 6, 6 * 16 + 4);
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    for _ in 0..3 {
        d.draw(&mut s, &st);
    }
    // the screen grows a row mid-run
    let mut grown = Screen::new(20, 5, rgb(FG), rgb(BG));
    grown.text(0, 0, "$ ls", style(rgb("00ff00"), rgb(BG)));
    grown.cursor = s.cursor;
    let mut s = grown;
    assert_eq!(d.draw(&mut s, &st), whole(w, h));
    assert_eq!(d.draw(&mut s, &st), whole(w, h));
    s.put(5, 4, 'z', false, style(rgb(FG), rgb(BG)));
    let third = d.draw(&mut s, &st);
    assert_eq!(third, vec![cell(5, 4, w, h, 20, 5)], "the third frame");
    assert_eq!(area(&d.draw(&mut s, &st)), 8 * 16);
    assert!(d.draw(&mut s, &st).is_empty());
}

// Startup and a wake-up lend every buffer at age 0: each is drawn whole
// once, and the frames after draw only what changed.
// covers: platform.compositor
#[test]
fn buffers_at_age_0_are_each_drawn_whole_once() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    for round in 0..2 {
        assert_eq!(d.draw(&mut s, &st), whole(w, h), "round {round}, first");
        assert_eq!(d.draw(&mut s, &st), whole(w, h), "round {round}, second");
        assert!(d.draw(&mut s, &st).is_empty(), "round {round}, idle");
        s.put(9, 1, 'q', false, style(rgb(FG), rgb(BG)));
        assert_eq!(d.draw(&mut s, &st), vec![cell(9, 1, w, h, 20, 4)]);
        assert_eq!(d.draw(&mut s, &st), vec![cell(9, 1, w, h, 20, 4)]);
        assert!(d.draw(&mut s, &st).is_empty(), "round {round}, idle after");
        d.lose_all();
    }
}

// A failed swap leaves the frame in the back buffer, which is lent again at
// age 1: the next frame draws only its own change there, and the other
// buffer then gets both.
// covers: platform.compositor
#[test]
fn a_buffer_lent_again_after_a_failed_swap_keeps_the_failed_frame() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    d.draw(&mut s, &st);
    let st_cell = style(rgb(FG), rgb(BG));
    s.put(5, 1, 'x', false, st_cell);
    assert_eq!(d.draw_failing(&mut s, &st), vec![cell(5, 1, w, h, 20, 4)]);
    s.put(6, 1, 'y', false, st_cell);
    assert_eq!(d.age(), 1);
    assert_eq!(d.draw_failing(&mut s, &st), vec![cell(6, 1, w, h, 20, 4)]);
    s.put(7, 1, 'z', false, st_cell);
    assert_eq!(d.age(), 1);
    assert_eq!(d.draw(&mut s, &st), vec![cell(7, 1, w, h, 20, 4)]);
    assert_eq!(d.age(), 4, "the other buffer holds the second frame");
    let three = Rect {
        width: 24,
        ..cell(5, 1, w, h, 20, 4)
    };
    assert_eq!(d.draw(&mut s, &st), vec![three]);
    assert!(d.draw(&mut s, &st).is_empty());
}

// A buffer drawn whole at age 0 leaves the other buffer's history alone: it
// still gets only what changed since it was drawn.
// covers: platform.compositor
#[test]
fn a_buffer_drawn_whole_leaves_the_others_alone() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    d.draw(&mut s, &st);
    let st_cell = style(rgb(FG), rgb(BG));
    s.put(5, 1, 'x', false, st_cell);
    assert_eq!(d.draw(&mut s, &st), vec![cell(5, 1, w, h, 20, 4)]);
    d.lose_back();
    s.put(9, 0, 'y', false, st_cell);
    assert_eq!(d.draw(&mut s, &st), whole(w, h), "the lost buffer");
    assert_eq!(d.age(), 2);
    assert_eq!(d.draw(&mut s, &st), vec![cell(9, 0, w, h, 20, 4)]);
    assert!(d.draw(&mut s, &st).is_empty());
}

// A buffer older than anything the renderer drew holds a picture it never
// made: it is drawn whole.
// covers: platform.compositor
#[test]
fn an_age_past_the_history_draws_the_buffer_whole() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let st = state(&s);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let want = fresh(&s, &st, w, h, Config::default());
    let mut bufs: Vec<WriteOnly> = (0..3).map(|_| WriteOnly::new(w, h)).collect();
    let mut since = Age::NEVER;
    // the first two frames at age 0; the third buffer at age 3 holds a
    // frame from before the first; the first, at age 3, holds the first
    // frame and lacks nothing; the second's age passes any history
    for (b, age, drawn) in [
        (0, 0, true),
        (1, 0, true),
        (2, 3, true),
        (0, 3, false),
        (1, u32::MAX, true),
        (2, 2, false),
    ] {
        let damage = r
            .draw(&s.frame(since), &mut bufs[b], age, &st)
            .damage
            .to_vec();
        since = s.damage.checkpoint();
        let want_damage = if drawn { whole(w, h) } else { Vec::new() };
        assert_eq!(damage, want_damage, "buffer {b} at age {age}");
        same_pixels(bufs[b].canvas(), &want, &format!("buffer {b} at age {age}"));
    }
}

// The pointer drawn in software, on a display without a cursor plane,
// damages the cells its arrow covers only when it appears, moves or hides;
// a cell under it that changes damages itself alone. An idle frame with the
// pointer shown draws nothing.
// covers: platform.compositor
#[test]
fn the_pointer_damages_its_cells_only_when_it_changes() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let base = state(&s);
    let at = |p: Option<(u32, u32)>| DrawState { pointer: p, ..base };
    // the two by two cells from `col`, `row`: a rectangle a row
    let quad = |col: u32, row: u32| {
        [row, row + 1].map(|r| Rect {
            width: 16,
            ..cell(col, r, w, h, 20, 4)
        })
    };
    let (shown, moved) = (at(Some((60, 4))), at(Some((100, 36))));
    // the arrow, 11 by 16 with its tip 1,1 in, at 60,4 covers columns 7
    // and 8 of rows 0 and 1; at 100,36, columns 12 and 13 of rows 2 and 3
    for st in [&shown, &shown] {
        assert_eq!(d.draw(&mut s, st), whole(w, h));
    }
    assert!(d.draw(&mut s, &shown).is_empty(), "idle, the pointer shown");
    assert!(d.back().writes.is_empty(), "an idle frame writes nothing");
    for buffer in ["moved", "the other buffer"] {
        assert_eq!(
            d.draw(&mut s, &moved),
            [quad(7, 0), quad(12, 2)].concat(),
            "{buffer}"
        );
    }
    assert!(d.draw(&mut s, &moved).is_empty(), "idle after the move");
    s.put(13, 3, 'k', false, style(rgb(FG), rgb(BG)));
    // the arrow is blended over each cell as it is drawn: a cell under it
    // changes alone
    let under = [cell(13, 3, w, h, 20, 4)];
    assert_eq!(d.draw(&mut s, &moved), under, "a cell under it");
    assert_eq!(d.draw(&mut s, &moved), under, "the other buffer");
    assert!(d.draw(&mut s, &moved).is_empty());
    let hidden = at(None);
    assert_eq!(d.draw(&mut s, &hidden), quad(12, 2), "hidden");
    assert_eq!(d.draw(&mut s, &hidden), quad(12, 2), "the other buffer");
    assert!(d.draw(&mut s, &hidden).is_empty(), "idle, no pointer");
}

// The arrow past the grid's edge, as at a prompt in the bottom row,
// redraws only the border it covered and covers, clipped to the display,
// never the whole border: the arrow at 60,60 in the grid, its corner at
// 62,61 on the display, reaches 7 pixels into the bottom border.
#[test]
fn a_move_in_the_bottom_row_damages_only_its_cells_and_the_arrow() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let base = state(&s);
    let at = |p: Option<(u32, u32)>| DrawState { pointer: p, ..base };
    let (shown, moved) = (at(Some((60, 60))), at(Some((100, 60))));
    for st in [&shown, &shown] {
        assert_eq!(d.draw(&mut s, st), whole(w, h));
    }
    assert!(d.draw(&mut s, &shown).is_empty(), "idle at the edge");
    let pair = |col: u32| Rect {
        width: 16,
        ..cell(col, 3, w, h, 20, 4)
    };
    let arrow = |x: u32| Rect {
        x,
        y: 61,
        width: 11,
        height: 7,
    };
    let want = [pair(7), pair(12), arrow(62), arrow(102)];
    for buffer in ["moved", "the other buffer"] {
        assert_eq!(d.draw(&mut s, &moved), want, "{buffer}");
    }
    assert!(d.draw(&mut s, &moved).is_empty(), "idle after the move");
}

#[test]
fn damage_merges_cells_up_to_three_apart() {
    let mut s = Screen::new(20, 2, rgb(FG), rgb(BG));
    let (w, h) = (160, 32);
    let mut d = Display::new(1, w, h);
    let st = DrawState {
        cursor_on: false,
        ..state(&s)
    };
    d.draw(&mut s, &st);
    for col in [0, 3, 7, 11, 12] {
        s.put(col, 1, '#', false, style(rgb(FG), rgb(BG)));
    }
    let r = |col: u32, cells: u32| Rect {
        x: col * 8,
        y: 16,
        width: cells * 8,
        height: 16,
    };
    // 0 and 3 are three apart and share a rectangle; 7 is four from 3 and
    // 11 four from 7, so each starts another, which 12 joins
    assert_eq!(d.draw(&mut s, &st), vec![r(0, 4), r(7, 1), r(11, 2)]);
}

/// Steps through edits, cursor moves, blink phases and the pointer on a
/// display of `buffers` buffers poisoned at the start, with swaps that fail
/// and buffers whose contents are lost; the display checks every frame
/// against a whole draw of the same state.
fn run(buffers: usize, turn: Orientation) {
    run_with(
        buffers,
        Config {
            orientation: turn,
            ..Config::default()
        },
    );
}

fn run_with(buffers: usize, config: Config) {
    let (cols, rows) = (24usize, 6usize);
    let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
    let (w, h) = if matches!(config.orientation, Orientation::Right | Orientation::Left) {
        (rows as u32 * 16 + 5, cols as u32 * 8 + 3)
    } else {
        (cols as u32 * 8 + 3, rows as u32 * 16 + 5)
    };
    let mut d = Display::with(buffers, w, h, config);
    s.cursor.visible = true;
    let mut seed = 0x1234_5678u32;
    let mut rand = move |n: u32| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed % n
    };
    for step in 0..160 {
        for _ in 0..rand(4) {
            let (col, row) = (rand(cols as u32) as usize, rand(rows as u32) as usize);
            let mut st = style(
                Rgb::new(rand(256) as u8, 200, 100),
                Rgb::new(0, rand(256) as u8, 40),
            );
            // blinking cells carry decorations too, for each blink option
            if rand(3) == 0 {
                st.attrs.insert(Attrs::BLINK);
            }
            if rand(3) == 0 {
                st.attrs.set_underline(vtium_term::UnderlineStyle::Curly);
            }
            if rand(3) == 0 {
                st.attrs.insert(Attrs::STRIKE);
            }
            let wide = rand(5) == 0 && col + 1 < cols;
            let c = if wide {
                '中'
            } else {
                char::from_u32(0x41 + rand(26)).unwrap()
            };
            s.put(col, row, c, wide, st);
        }
        if rand(3) == 0 {
            s.cursor.col = rand(cols as u32) as usize;
            s.cursor.row = rand(rows as u32) as usize;
            s.cursor.shape = match rand(3) {
                0 => CursorShape::Block,
                1 => CursorShape::Underline,
                _ => CursorShape::Bar,
            };
        }
        let st = DrawState {
            cursor_on: rand(4) != 0,
            blink_hidden: (step / 7) % 2 == 1,
            pointer: (rand(3) == 0).then(|| (rand(cols as u32 * 8), rand(rows as u32 * 16))),
            ..state(&s)
        };
        match rand(20) {
            0 => d.lose_back(),
            1 => d.lose_all(),
            _ => {}
        }
        if rand(6) == 0 {
            d.draw_failing(&mut s, &st);
        } else {
            d.draw(&mut s, &st);
        }
    }
}

// covers: platform.compositor
#[test]
fn every_frame_matches_a_whole_draw_with_one_buffer() {
    run(1, Orientation::Normal);
}

// covers: platform.compositor
#[test]
fn every_frame_matches_a_whole_draw_with_two_buffers() {
    run(2, Orientation::Normal);
}

// covers: platform.compositor
#[test]
fn every_frame_matches_a_whole_draw_with_three_buffers() {
    run(3, Orientation::Normal);
}

// covers: platform.compositor
#[test]
fn every_frame_matches_a_whole_draw_turned() {
    for turn in [
        Orientation::Right,
        Orientation::UpsideDown,
        Orientation::Left,
    ] {
        run(2, turn);
    }
}

// Each text blink option keeps the history exact: a phase change redraws
// what the option changes, and only that.
// covers: sgr.blink.style, sgr.blink.decorations
#[test]
fn every_frame_matches_a_whole_draw_under_each_blink_option() {
    for style in [BlinkStyle::Full, BlinkStyle::Partial, BlinkStyle::Off] {
        for decorations in [false, true] {
            let blink = TextBlink { style, decorations };
            run_with(
                2,
                Config {
                    blink,
                    ..Config::default()
                },
            );
        }
    }
}

#[test]
fn a_palette_background_change_redraws_the_border() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(1, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    assert!(d.draw(&mut s, &st).is_empty());
    s.palette.set_bg(rgb("202020"));
    s.damage.mark_all();
    let st = state(&s);
    assert_eq!(area(&d.draw(&mut s, &st)), u64::from(w * h));
}

/// Draws `s` with the cursor as `edit` leaves it, at rest, into both
/// buffers: each frame redraws only the cursor's cell, and the frame after
/// draws nothing.
fn cursor_at_rest(d: &mut Display, s: &mut Screen, st: &DrawState, edit: impl Fn(&mut Screen)) {
    let (w, h) = PROMPT;
    edit(s);
    for buffer in ["this buffer", "the other buffer"] {
        assert_eq!(d.draw(s, st), vec![cell(2, 2, w, h, 20, 4)], "{buffer}");
    }
    assert!(d.draw(s, st).is_empty(), "idle after");
}

// DECSCUSR with the cursor still, as vim's `i` sends it: the cell under
// the cursor redraws in the new shape
// covers: cursor.style
#[test]
fn a_cursor_shape_change_at_rest_redraws_its_cell() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    d.draw(&mut s, &st);
    for shape in [CursorShape::Bar, CursorShape::Underline, CursorShape::Block] {
        cursor_at_rest(&mut d, &mut s, &st, |s| s.cursor.shape = shape);
    }
}

// OSC 12 with the cursor still: the cell under it redraws in the color
// covers: cursor.color
#[test]
fn a_cursor_color_change_at_rest_redraws_its_cell() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let st = state(&s);
    d.draw(&mut s, &st);
    d.draw(&mut s, &st);
    for color in [Some(rgb("ff8000")), Some(rgb("00ff00")), None] {
        cursor_at_rest(&mut d, &mut s, &st, |s| s.cursor.color = color);
    }
    s.cursor.shape = CursorShape::Bar;
    cursor_at_rest(&mut d, &mut s, &st, |s| {
        s.cursor.color = Some(rgb("ff8000"))
    });
}

// DECTCEM and the cursor's blink phase with the cursor still: the cell
// under it redraws as it hides and shows
// covers: platform.compositor
#[test]
fn the_cursor_hiding_and_blinking_at_rest_redraws_its_cell() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let mut d = Display::new(2, w, h);
    let on = state(&s);
    let off = DrawState {
        cursor_on: false,
        ..on
    };
    d.draw(&mut s, &on);
    d.draw(&mut s, &on);
    cursor_at_rest(&mut d, &mut s, &on, |s| s.cursor.visible = false);
    cursor_at_rest(&mut d, &mut s, &on, |s| s.cursor.visible = true);
    cursor_at_rest(&mut d, &mut s, &off, |_| {});
    cursor_at_rest(&mut d, &mut s, &on, |_| {});
}

// A frame that draws nothing takes no serial, so a buffer's age still
// counts only frames that drew. Two buffers drawn whole are frames 1 and 2;
// the first buffer, at age 2, takes a red cell as frame 3, and is lent
// again at age 1 with nothing changed, which draws nothing. The second
// buffer holds frame 2, at age 2, and must get the red cell.
// covers: platform.compositor
#[test]
fn an_empty_damage_frame_takes_no_serial() {
    let mut s = prompt();
    let (w, h) = PROMPT;
    let st = state(&s);
    let mut r = Renderer::new(unifont(), w, h, Config::default());
    let mut bufs: Vec<WriteOnly> = (0..2).map(|_| WriteOnly::new(w, h)).collect();
    let mut since = Age::NEVER;
    let mut frame = |r: &mut Renderer<_>, s: &mut Screen, b: &mut WriteOnly, age: u32| {
        let damage = r.draw(&s.frame(since), b, age, &st).damage.to_vec();
        since = s.damage.checkpoint();
        damage
    };
    assert_eq!(frame(&mut r, &mut s, &mut bufs[0], 0), whole(w, h));
    assert_eq!(frame(&mut r, &mut s, &mut bufs[1], 0), whole(w, h));
    s.put(9, 1, ' ', false, style(rgb(FG), rgb("ff0000")));
    let red = cell(9, 1, w, h, 20, 4);
    assert_eq!(frame(&mut r, &mut s, &mut bufs[0], 2), vec![red]);
    assert!(frame(&mut r, &mut s, &mut bufs[0], 1).is_empty(), "idle");
    assert_eq!(
        frame(&mut r, &mut s, &mut bufs[1], 2),
        vec![red],
        "the other"
    );
    for b in &bufs {
        for y in red.y..red.y + red.height {
            for x in red.x..red.x + red.width {
                assert_eq!(b.canvas().pixel(x, y), Some(0xff_00_00), "{x},{y}");
            }
        }
    }
}

// The pointer drawn in software over plain text, which no decorated cell
// widens, at cell sizes whose halves round: every frame equals a whole
// draw as the pointer crosses the grid and its edges, in every
// orientation, so the cells it marks cover every pixel it drew.
// covers: platform.compositor
#[test]
fn the_pointer_leaves_nothing_behind_at_odd_cell_sizes() {
    for (cw, ch) in [(7u32, 13u32), (9, 17), (13, 11), (8, 16), (6, 9)] {
        for turn in [
            Orientation::Normal,
            Orientation::Right,
            Orientation::UpsideDown,
            Orientation::Left,
        ] {
            let (cols, rows) = (9usize, 4usize);
            let turned = matches!(turn, Orientation::Right | Orientation::Left);
            let (gw, gh) = (cols as u32 * cw, rows as u32 * ch);
            let (w, h) = if turned {
                (gh + 5, gw + 3)
            } else {
                (gw + 3, gh + 5)
            };
            let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
            for row in 0..rows {
                let line: String = "pointer! ".chars().cycle().skip(row).take(cols).collect();
                s.text(0, row, &line, style(rgb(FG), rgb(BG)));
            }
            let config = Config {
                orientation: turn,
                ..Config::default()
            };
            let mut r = Renderer::new(common::psf(cw, ch), w, h, config);
            let mut c = Canvas::new(w, h, 0x00ab_cdef);
            let mut since = Age::NEVER;
            let mut age = 0;
            let mut at = Vec::new();
            for py in (0..gh + 4).step_by(3) {
                for px in (0..gw + 4).step_by(2) {
                    at.push(Some((px, py)));
                }
            }
            at.push(None);
            for (i, &pointer) in at.iter().enumerate() {
                let st = DrawState {
                    pointer,
                    ..state(&s)
                };
                r.draw(&s.frame(since), &mut c, age, &st);
                since = s.damage.checkpoint();
                age = 1;
                let mut whole = Renderer::new(common::psf(cw, ch), w, h, config);
                let mut want = Canvas::new(w, h, 0);
                whole.draw(&s.frame(Age::NEVER), &mut want, 0, &st);
                let what = format!("{cw}x{ch} {turn:?}, frame {i}, pointer {pointer:?}");
                same_pixels(&c, &want, &what);
            }
        }
    }
}
