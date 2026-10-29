//! The console's own drawing over a display's frame (ADR-1401): a
//! [`Notice`], vtium's message in a box (D7, D8, D9), and a head's focus
//! [`Mark`] (D5). Neither passes through the terminal: no cell, pty,
//! scrollback or reply sees them, and the renderer's record of the cells
//! stays as it was.
//!
//! Each is drawn with the font it is given, which the console makes from
//! the built-in sets at the head's cell size (ADR-1200 D7), and turns with
//! the display. Drawing is stateless, so the owner decides how long a
//! notice shows, until a key or for a time (ADR-1401's probe 5): it draws
//! the notice again over each frame that writes beneath it, adds the area
//! returned to the frame's damage, and once the notice ends draws the next
//! frame whole (age 0), which restores every cell beneath it.
//!
//! The notice sits across the top of the text's view, where a getty prints
//! its banner; the mark sits in the view's top right cell.

use vtium_font::{Face, Font, Format, Glyph, Style, Width};
use vtium_term::Rgb;

use crate::blend::{mask, over_bg, pixel};
use crate::mono::{Depth, Ink, Threshold};
use crate::target::{Orientation, Rect, Target};

/// A display's buffer as the console's own drawing sees it: its size, how
/// it is turned and its depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub orientation: Orientation,
    pub depth: Depth,
}

impl Surface {
    /// The text's view, unturned: its width and height.
    fn view(&self) -> (u32, u32) {
        if self.orientation.turned() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// The display pixel that draws the view's pixel `x`, `y`, as the
    /// renderer turns the grid.
    fn place(&self, x: u32, y: u32) -> (u32, u32) {
        let last = |n: u32, v: u32| n.saturating_sub(1).saturating_sub(v);
        match self.orientation {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (last(self.width, x), last(self.height, y)),
            Orientation::Right => (last(self.width, y), x),
            Orientation::Left => (y, last(self.height, x)),
        }
    }

    /// The display area that draws the view's area `r`.
    fn area(&self, r: Rect) -> Rect {
        let (x0, y0) = self.place(r.x, r.y);
        let far = |v: u32, n: u32| v.saturating_add(n).saturating_sub(1);
        let (x1, y1) = self.place(far(r.x, r.width), far(r.y, r.height));
        let (width, height) = if self.orientation.turned() {
            (r.height, r.width)
        } else {
            (r.width, r.height)
        };
        Rect {
            x: x0.min(x1),
            y: y0.min(y1),
            width,
            height,
        }
    }

    /// Writes `picture`, the view's area at its `x`, `y`, to `target`,
    /// turned and, at 1 bit, each pixel lit or unlit. Returns the display
    /// area it covers.
    fn put(&self, at: (u32, u32), picture: &Picture, target: &mut impl Target) -> Rect {
        let view = Rect {
            x: at.0,
            y: at.1,
            width: picture.width,
            height: picture.height,
        };
        let area = self.area(view);
        let mut line = Vec::new();
        let mut target = Threshold::new(target, self.depth, &mut line);
        let mut row = Vec::with_capacity(usize::try_from(area.width).unwrap_or(0));
        for dy in 0..area.height {
            row.clear();
            for dx in 0..area.width {
                let (vx, vy) = self.unplace(area.x.saturating_add(dx), area.y.saturating_add(dy));
                row.push(picture.get(vx.saturating_sub(view.x), vy.saturating_sub(view.y)));
            }
            target.write(area.x, area.y.saturating_add(dy), &row);
        }
        area
    }

    /// The view's pixel the display pixel `x`, `y` draws: [`Surface::place`]
    /// undone.
    fn unplace(&self, x: u32, y: u32) -> (u32, u32) {
        let last = |n: u32, v: u32| n.saturating_sub(1).saturating_sub(v);
        match self.orientation {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (last(self.width, x), last(self.height, y)),
            Orientation::Right => (y, last(self.width, x)),
            Orientation::Left => (last(self.height, y), x),
        }
    }
}

/// The colors the console draws in: the text, what it stands on, and a
/// notice's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub ink: Rgb,
    pub paper: Rgb,
    pub edge: Rgb,
}

impl Colors {
    /// ADR-1401's figure of the notice: dark text in a pale red box with a
    /// red edge.
    pub const NOTICE: Colors = Colors {
        ink: Rgb::new(0x1f, 0x29, 0x33),
        paper: Rgb::new(0xfe, 0xe2, 0xe2),
        edge: Rgb::new(0xb9, 0x1c, 0x1c),
    };

    /// These colors at `depth`: at 1 bit the paper lit where it is light
    /// and the text and edge its opposite, as a cell's are.
    fn at(self, depth: Depth) -> Colors {
        let ink = Ink::new(depth, self.paper);
        Colors {
            ink: ink.fg(self.ink),
            paper: ink.bg(),
            edge: ink.fg(self.edge),
        }
    }
}

/// `text` from a file, such as a head's name, in double quotes, with every
/// character a reader could not see or could mistake escaped as Rust
/// escapes it: a newline, a control character, a quote, a backslash, a
/// bidirectional or zero-width format character. The notice and the log
/// put a file's text through it, so the file cannot add a line or hide a
/// character in vtium's message.
pub fn quote(text: &str) -> String {
    format!("\"{}\"", text.escape_debug())
}

/// A message of vtium's own, drawn in a box over the display's frame, as a
/// getty prints its banner (ADR-1401 D7). A newline starts a line, and
/// each line wraps at a space to the box's width, a word longer than the
/// box breaking inside it. Text quoted from a file goes through [`quote`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    text: String,
}

impl Notice {
    pub fn new(text: impl Into<String>) -> Notice {
        Notice { text: text.into() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Draws the notice across the top of `display`'s view with `font`, its
    /// lines as many as fit and the rest cut, and returns the display area
    /// it covers. `None`, writing nothing, when the display holds no line.
    pub fn draw<F: Face>(
        &self,
        font: &Font<F>,
        display: Surface,
        colors: Colors,
        target: &mut impl Target,
    ) -> Option<Rect> {
        let cell = font.cell();
        let (cw, ch) = (cell.width.max(1), cell.height.max(1));
        let (vw, vh) = display.view();
        // an edge of an eighth of the cell, inside it half a cell across
        // and a quarter down
        let edge = (ch / 8).max(1);
        let inset = (edge.saturating_add(cw / 2), edge.saturating_add(ch / 4));
        let cols = vw
            .saturating_sub(inset.0.saturating_mul(2))
            .checked_div(cw)
            .unwrap_or(0);
        let room = vh
            .saturating_sub(inset.1.saturating_mul(2))
            .checked_div(ch)
            .unwrap_or(0);
        if cols == 0 || room == 0 {
            return None;
        }
        let mut lines = wrap(&self.text, cols, |c| cells(font, c));
        lines.truncate(usize::try_from(room).unwrap_or(usize::MAX));
        let shown = u32::try_from(lines.len().max(1)).unwrap_or(room);
        let colors = colors.at(display.depth);
        let mut picture = Picture::new(
            vw,
            inset
                .1
                .saturating_mul(2)
                .saturating_add(shown.saturating_mul(ch)),
            colors.paper,
        );
        picture.frame(edge, colors.edge);
        for (y, line) in (inset.1..)
            .step_by(usize::try_from(ch).unwrap_or(1))
            .zip(&lines)
        {
            let mut x = inset.0;
            for &(c, width) in line {
                if let Some(g) = font.glyph(&[c], width, Style::default()) {
                    picture.glyph(x, y, &g, colors);
                }
                x = x.saturating_add(cw.saturating_mul(width.cells()));
            }
        }
        Some(display.put((0, 0), &picture, target))
    }
}

/// A head's focus mark (ADR-1401 D5): a filled circle on the head that
/// holds the focus and a hollow one on every other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Focus,
    NoFocus,
}

impl Mark {
    /// The character it draws as.
    fn char(self) -> char {
        match self {
            Mark::Focus => '\u{25cf}',
            Mark::NoFocus => '\u{25cb}',
        }
    }

    /// Draws the mark in the top right cell of `display`'s view with
    /// `font`, its circle in `colors.ink` on `colors.paper`, and returns
    /// the display area it covers. `None`, writing nothing, when the
    /// display is smaller than the mark.
    pub fn draw<F: Face>(
        self,
        font: &Font<F>,
        display: Surface,
        colors: Colors,
        target: &mut impl Target,
    ) -> Option<Rect> {
        let c = self.char();
        let width = cells(font, c);
        let g = font.glyph(&[c], width, Style::default())?;
        let (vw, vh) = display.view();
        if g.width() > vw || g.height() > vh {
            return None;
        }
        let colors = colors.at(display.depth);
        let mut picture = Picture::new(g.width(), g.height(), colors.paper);
        picture.glyph(0, 0, &g, colors);
        Some(display.put((vw.saturating_sub(g.width()), 0), &picture, target))
    }
}

/// The cells `c` takes in `font`: its face's own width, one cell for a
/// character the face lacks.
fn cells<F: Face>(font: &Font<F>, c: char) -> Width {
    font.face()
        .draw(&[c], Style::default())
        .map_or(Width::Narrow, |g| g.cells())
}

/// `text`'s lines, each wrapped to `cols` cells by `width`: broken after
/// its last space that fits, the space dropped, or else after the last
/// character that fits, at least one a line.
fn wrap(text: &str, cols: u32, width: impl Fn(char) -> Width) -> Vec<Vec<(char, Width)>> {
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let mut rest: &[(char, Width)] =
            &paragraph.chars().map(|c| (c, width(c))).collect::<Vec<_>>();
        loop {
            let mut used = 0u32;
            let fit = rest
                .iter()
                .take_while(|(_, w)| {
                    used = used.saturating_add(w.cells());
                    used <= cols
                })
                .count();
            if fit == rest.len() {
                out.push(rest.to_vec());
                break;
            }
            let fit = fit.max(1);
            let (line, next) = match rest.get(..=fit).and_then(|head| {
                head.iter()
                    .rposition(|&(c, _)| c == ' ')
                    .filter(|&at| at > 0)
            }) {
                Some(at) => (rest.get(..at), rest.get(at.saturating_add(1)..)),
                None => (rest.get(..fit), rest.get(fit..)),
            };
            out.push(line.unwrap_or_default().to_vec());
            rest = next.unwrap_or_default();
            if rest.is_empty() {
                break;
            }
        }
    }
    out
}

/// The view's pixels of what the console draws, row by row.
struct Picture {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Picture {
    fn new(width: u32, height: u32, fill: Rgb) -> Picture {
        let n = u64::from(width).saturating_mul(u64::from(height));
        Picture {
            width,
            height,
            pixels: vec![pixel(fill); usize::try_from(n).unwrap_or(0)],
        }
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = u64::from(y)
            .saturating_mul(u64::from(self.width))
            .saturating_add(u64::from(x));
        usize::try_from(i).ok()
    }

    fn get(&self, x: u32, y: u32) -> u32 {
        self.index(x, y)
            .and_then(|i| self.pixels.get(i))
            .copied()
            .unwrap_or(0)
    }

    fn set(&mut self, x: u32, y: u32, p: u32) {
        if let Some(px) = self.index(x, y).and_then(|i| self.pixels.get_mut(i)) {
            *px = p;
        }
    }

    /// An edge `thick` pixels wide around it, in `color`.
    fn frame(&mut self, thick: u32, color: Rgb) {
        let p = pixel(color);
        for y in 0..self.height {
            for x in 0..self.width {
                let inside = x >= thick
                    && y >= thick
                    && x < self.width.saturating_sub(thick)
                    && y < self.height.saturating_sub(thick);
                if !inside {
                    self.set(x, y, p);
                }
            }
        }
    }

    /// `g` at `x`, `y`, in `colors.ink` on `colors.paper`.
    fn glyph(&mut self, x: u32, y: u32, g: &Glyph, colors: Colors) {
        for gy in 0..g.height() {
            for gx in 0..g.width() {
                let p = match (g.format(), g.pixel(gx, gy)) {
                    (Format::Mask, Some(&[a])) => mask(colors.ink, colors.paper, a),
                    (Format::Rgba, Some(&[r, gr, b, a])) => {
                        over_bg(colors.paper, u32::from_be_bytes([a, r, gr, b]))
                    }
                    _ => continue,
                };
                self.set(x.saturating_add(gx), y.saturating_add(gy), p);
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::{Canvas, Config, DrawState, Renderer};
    use vtium_font::set::{SetFace, builtin};
    use vtium_term::{Age, Cursor, Damage, Frame, Glyph, Line, LineId, Palette};

    const FIGURE: &str =
        "vtium: heads.conf:5: rule names undefined head \"desc\"; running the shipped rules";

    fn font(name: &str, height: u32) -> Font<SetFace> {
        let set = builtin(name).expect("a built-in set");
        Font::new(SetFace::new(set, height).expect("a cell"))
    }

    fn display(width: u32, height: u32) -> Surface {
        Surface {
            width,
            height,
            orientation: Orientation::Normal,
            depth: Depth::Color,
        }
    }

    /// Rows of `cols` cells of `c`.
    fn lines(cols: usize, rows: usize, c: char) -> Vec<Line> {
        (0..rows)
            .map(|r| {
                let mut line = Line::new(LineId::new(r as u64 + 1), cols);
                for col in 0..cols {
                    line.write(
                        col,
                        Glyph::Char(c),
                        vtium_term::Width::Narrow,
                        &vtium_term::Style::DEFAULT,
                        None,
                    );
                }
                line
            })
            .collect()
    }

    fn frame<'a>(
        rows: &'a [Line],
        damage: &'a Damage,
        palette: &'a Palette,
        since: Age,
    ) -> Frame<'a> {
        Frame::new(
            rows.iter().collect(),
            damage,
            palette,
            since,
            Cursor::default(),
            false,
        )
    }

    fn palette() -> Palette {
        Palette::new(
            [Rgb::default(); 256],
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
        )
    }

    // ADR-1401 D7: the notice is vtium's own, drawn over the display's
    // frame and never through the terminal's cells
    #[test]
    fn a_notice_draws_above_the_grid_and_leaves_the_terminals_cells_unchanged() {
        let (w, h) = (320, 240);
        let rows = lines(40, 30, 'x');
        let before = rows.clone();
        let mut damage = Damage::new(40, 30);
        let palette = palette();
        let mut r = Renderer::new(font("unscii-8", 8), w, h, Config::default());
        let mut canvas = Canvas::new(w, h, 0);
        r.draw(
            &frame(&rows, &damage, &palette, Age::NEVER),
            &mut canvas,
            0,
            &DrawState::default(),
        );
        let since = damage.checkpoint();
        let grid = canvas.clone();

        let area = Notice::new(FIGURE)
            .draw(
                &font("unscii-8", 8),
                display(w, h),
                Colors::NOTICE,
                &mut canvas,
            )
            .expect("a notice fits 320x240");
        assert_eq!((area.x, area.y, area.width), (0, 0, w));
        assert!(area.height > 0 && area.height < h, "{area:?}");
        let c = Colors::NOTICE;
        let drawn = [pixel(c.ink), pixel(c.paper), pixel(c.edge)];
        for y in 0..h {
            for x in 0..w {
                let now = canvas.pixel(x, y).expect("inside");
                if y < area.height {
                    assert!(drawn.contains(&now), "the box at {x},{y}: {now:06x}");
                } else {
                    assert_eq!(Some(now), grid.pixel(x, y), "outside the box at {x},{y}");
                }
            }
        }
        assert_eq!(canvas.pixel(0, 0), Some(pixel(c.edge)));
        assert_eq!(rows, before, "the terminal's cells");
        // the renderer's own record of the cells is untouched: a frame
        // that changes nothing draws nothing, and the box stays
        let shown = canvas.clone();
        let drawn = r.draw(
            &frame(&rows, &damage, &palette, since),
            &mut canvas,
            1,
            &DrawState::default(),
        );
        assert!(drawn.damage.is_empty(), "{:?}", drawn.damage);
        assert_eq!(canvas, shown);
    }

    fn text(lines: &[Vec<(char, Width)>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|&(c, _)| c).collect())
            .collect()
    }

    fn narrow(_: char) -> Width {
        Width::Narrow
    }

    #[test]
    fn a_long_message_wraps_at_its_spaces_to_the_box() {
        // 38 cells: 320 pixels of 8, less the edge and half a cell each side
        assert_eq!(
            text(&wrap(FIGURE, 38, narrow)),
            [
                "vtium: heads.conf:5: rule names",
                "undefined head \"desc\"; running the",
                "shipped rules",
            ]
        );
        assert_eq!(text(&wrap("one\n\ntwo", 8, narrow)), ["one", "", "two"]);
        // a break on the last space leaves no empty line after it
        assert_eq!(text(&wrap("abcd efgh ", 4, narrow)), ["abcd", "efgh"]);
    }

    #[test]
    fn a_word_longer_than_the_box_breaks_inside_it() {
        assert_eq!(
            text(&wrap("abcdefghij kl", 4, narrow)),
            ["abcd", "efgh", "ij", "kl"]
        );
        // a wide character that does not fit goes to the next line, and one
        // wider than the box takes a line of its own
        let wide = |c: char| {
            if c == '\u{6f22}' {
                Width::Wide
            } else {
                Width::Narrow
            }
        };
        assert_eq!(text(&wrap("ab\u{6f22}", 3, wide)), ["ab", "\u{6f22}"]);
        assert_eq!(text(&wrap("\u{6f22}a", 1, wide)), ["\u{6f22}", "a"]);
    }

    /// The pixels of the `w` by `h` block at `x`, `y`.
    fn block(c: &Canvas, x: u32, y: u32, w: u32, h: u32) -> Vec<u32> {
        (y..y + h)
            .flat_map(|y| (x..x + w).map(move |x| (x, y)))
            .map(|(x, y)| c.pixel(x, y).expect("inside"))
            .collect()
    }

    // legible down to minspec's floor: each line in the built-in font's
    // glyphs, one a cell, in the box's colors
    #[test]
    fn the_notice_draws_each_line_in_the_fonts_glyphs() {
        for (w, h, name, ch, lines) in [(320, 240, "unscii-8", 8, 3), (640, 480, "unifont", 16, 2)]
        {
            let font = font(name, ch);
            let mut canvas = Canvas::new(w, h, 0);
            let area = Notice::new(FIGURE)
                .draw(&font, display(w, h), Colors::NOTICE, &mut canvas)
                .expect("it fits");
            let (edge, cw) = ((ch / 8).max(1), 8);
            let inset = (edge + cw / 2, edge + ch / 4);
            assert_eq!(area.height, 2 * inset.1 + lines * ch, "{name}");
            let cols = (w - 2 * inset.0) / cw;
            let c = Colors::NOTICE;
            for (row, line) in wrap(FIGURE, cols, narrow).iter().enumerate() {
                for (col, &(ch_, _)) in line.iter().enumerate() {
                    let g = font
                        .glyph(&[ch_], Width::Narrow, Style::default())
                        .expect("a glyph");
                    let want: Vec<u32> = g
                        .pixels()
                        .iter()
                        .map(|&a| mask(c.ink, c.paper, a))
                        .collect();
                    let (x, y) = (inset.0 + col as u32 * cw, inset.1 + row as u32 * ch);
                    assert_eq!(block(&canvas, x, y, cw, ch), want, "{name} {ch_:?}");
                }
            }
        }
    }

    #[test]
    fn a_notice_taller_than_the_display_is_cut_to_whole_lines() {
        let mut canvas = Canvas::new(320, 240, 0);
        let area = Notice::new("x\n".repeat(100))
            .draw(
                &font("unscii-8", 8),
                display(320, 240),
                Colors::NOTICE,
                &mut canvas,
            )
            .expect("it fits");
        // 29 lines of 8 between the edges and the quarter-cell insets
        assert_eq!(area.height, 6 + 29 * 8);
        // a display that holds no line draws nothing
        let mut small = Canvas::new(16, 8, 7);
        let none = Notice::new("x").draw(
            &font("unscii-8", 8),
            display(16, 8),
            Colors::NOTICE,
            &mut small,
        );
        assert_eq!(none, None);
        assert!(small.pixels().iter().all(|&p| p == 7));
    }

    // the notice and the mark turn with the display, as the grid does
    #[test]
    fn every_orientation_draws_the_same_notice_and_mark_turned() {
        const UNTOUCHED: u32 = 0x0012_3456;
        let font = font("unscii-8", 8);
        let (w, h) = (320, 240);
        let draw = |o: Orientation, dw: u32, dh: u32| {
            let mut c = Canvas::new(dw, dh, UNTOUCHED);
            let d = Surface {
                orientation: o,
                ..display(dw, dh)
            };
            let a = Notice::new(FIGURE)
                .draw(&font, d, Colors::NOTICE, &mut c)
                .expect("it fits");
            let b = Mark::Focus
                .draw(&font, d, Colors::NOTICE, &mut c)
                .expect("it fits");
            (c, a, b)
        };
        let (normal, _, _) = draw(Orientation::Normal, w, h);
        for o in [
            Orientation::UpsideDown,
            Orientation::Right,
            Orientation::Left,
        ] {
            let (dw, dh) = if o.turned() { (h, w) } else { (w, h) };
            let (turned, a, b) = draw(o, dw, dh);
            for y in 0..h {
                for x in 0..w {
                    let (tx, ty) = match o {
                        Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
                        Orientation::Right => (h - 1 - y, x),
                        Orientation::Left => (y, w - 1 - x),
                        Orientation::Normal => (x, y),
                    };
                    assert_eq!(turned.pixel(tx, ty), normal.pixel(x, y), "{o:?} {x},{y}");
                }
            }
            // the areas returned are exactly the pixels written
            let inside = |r: Rect, x: u32, y: u32| {
                x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
            };
            for y in 0..dh {
                for x in 0..dw {
                    let written = turned.pixel(x, y) != Some(UNTOUCHED);
                    assert_eq!(written, inside(a, x, y) || inside(b, x, y), "{o:?} {x},{y}");
                }
            }
        }
    }

    // vtium/vtium#135: at 1 bit the box is lit where its paper is light, its
    // text and edge unlit, and every pixel black or white
    #[test]
    fn a_1_bit_display_draws_the_notice_in_black_and_white() {
        let mut canvas = Canvas::new(320, 240, 0x0080_8080);
        let d = Surface {
            depth: Depth::One,
            ..display(320, 240)
        };
        // a rounded corner and a diagonal, sprites of partial coverage
        let area = Notice::new(format!("{FIGURE} \u{256d}\u{2571}"))
            .draw(&font("unscii-8", 8), d, Colors::NOTICE, &mut canvas)
            .expect("it fits");
        let box_ = block(&canvas, 0, 0, area.width, area.height);
        assert!(box_.iter().all(|&p| p == 0 || p == 0x00ff_ffff));
        assert_eq!(canvas.pixel(0, 0), Some(0), "the edge");
        assert_eq!(canvas.pixel(1, 1), Some(0x00ff_ffff), "the paper");
        assert!(box_.iter().filter(|&&p| p == 0).count() > 2 * 320 + 2 * area.height as usize);
        // text on its paper's side of the cut still shows, as a cell's does
        let pale = Colors {
            ink: Rgb::new(0xc0, 0xc0, 0xc0),
            paper: Rgb::new(0xff, 0xff, 0xff),
            edge: Rgb::new(0xe0, 0xe0, 0xe0),
        };
        let mut canvas = Canvas::new(320, 240, 0);
        Notice::new("x").draw(&font("unscii-8", 8), d, pale, &mut canvas);
        assert_eq!(canvas.pixel(0, 0), Some(0), "the edge");
        assert!(block(&canvas, 5, 3, 8, 8).contains(&0), "the text");
    }

    // ADR-1401 D5: every head shows whether it holds the focus, filled where
    // it does and hollow where it does not, legible at 8 pixels
    #[test]
    fn the_mark_shows_whether_its_head_holds_the_focus() {
        for (name, ch) in [("unscii-8", 8), ("unifont", 16), ("unifont", 32)] {
            let font = font(name, ch);
            let ink = |mark: Mark| {
                assert!(font.face().has_glyph(mark.char()), "{name} {mark:?}");
                let mut canvas = Canvas::new(320, 240, 0);
                let area = mark
                    .draw(&font, display(320, 240), Colors::NOTICE, &mut canvas)
                    .expect("it fits");
                let cell = font.cell();
                assert_eq!(
                    area,
                    Rect {
                        x: 320 - cell.width,
                        y: 0,
                        width: cell.width,
                        height: cell.height
                    }
                );
                block(&canvas, area.x, 0, area.width, area.height)
                    .iter()
                    .filter(|&&p| p == pixel(Colors::NOTICE.ink))
                    .count()
            };
            let (held, not) = (ink(Mark::Focus), ink(Mark::NoFocus));
            assert!(
                not > 0 && held > not,
                "{name} at {ch}: {held} against {not}"
            );
        }
    }

    // ADR-1401 D7: text quoted from a file is escaped first, so it cannot
    // add a line to the notice or hide a character in it
    #[test]
    fn quote_escapes_what_a_file_could_hide() {
        assert_eq!(quote("desc"), "\"desc\"");
        assert_eq!(quote("a\nb"), "\"a\\nb\"");
        assert_eq!(quote("\u{1b}[2J"), "\"\\u{1b}[2J\"");
        assert_eq!(quote("\u{202e}csed"), "\"\\u{202e}csed\"");
        assert_eq!(quote("x\u{200b}y"), "\"x\\u{200b}y\"");
        assert_eq!(quote("say \"hi\" \\"), "\"say \\\"hi\\\" \\\\\"");
        assert_eq!(quote("caf\u{e9} \u{6f22}"), "\"caf\u{e9} \u{6f22}\"");
    }
}
