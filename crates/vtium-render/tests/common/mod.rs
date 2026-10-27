//! Screens for the renderer's tests: lines written through vtium-term's
//! model, and the frames a display reads from them.
#![allow(dead_code)]

use vtium_font::{Font, Unifont};
use vtium_render::{Canvas, Target};
use vtium_term::{
    Age, Attrs, Color, Cursor, Damage, Frame, Glyph, Line, LineId, Palette, Rgb, Style,
    UnderlineStyle, Width,
};

pub fn unifont() -> Font<Unifont> {
    Font::new(Unifont::new(16))
}

pub fn rgb(hex: &str) -> Rgb {
    let v = u32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("not a colour: {hex}"));
    Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A screen of lines, its damage, palette and cursor.
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<Line>,
    pub damage: Damage,
    pub palette: Palette,
    pub cursor: Cursor,
}

impl Screen {
    pub fn new(cols: usize, rows: usize, fg: Rgb, bg: Rgb) -> Screen {
        let mut colors = [Rgb::default(); 256];
        for (i, c) in colors.iter_mut().enumerate() {
            let i = i as u8;
            *c = Rgb::new(i, i.wrapping_mul(3), i.wrapping_mul(7));
        }
        Screen {
            cols,
            rows,
            lines: (0..rows)
                .map(|r| Line::new(LineId::new(r as u64 + 1), cols))
                .collect(),
            damage: Damage::new(cols, rows),
            palette: Palette::new(colors, fg, bg),
            cursor: Cursor::default(),
        }
    }

    /// Writes `c` at `col`, `row`, two cells wide when `wide`, and marks
    /// what it wrote.
    pub fn put(&mut self, col: usize, row: usize, c: char, wide: bool, style: Style) {
        let line = &mut self.lines[row];
        let width = if wide { Width::Wide } else { Width::Narrow };
        line.write(col, Glyph::Char(c), width, &style, None);
        self.damage.mark_cell(row, col);
        if wide && col + 1 < self.cols {
            line.write(col + 1, Glyph::Empty, Width::WideTail, &style, None);
            self.damage.mark_cell(row, col + 1);
        }
    }

    /// Writes `text` from `col`, one narrow cell a character.
    pub fn text(&mut self, col: usize, row: usize, text: &str, style: Style) {
        for (i, c) in text.chars().enumerate() {
            self.put(col + i, row, c, false, style);
        }
    }

    pub fn frame(&self, since: Age) -> Frame<'_> {
        Frame::new(
            self.lines.iter().collect(),
            &self.damage,
            &self.palette,
            since,
            self.cursor,
            false,
        )
    }
}

pub fn style(fg: Rgb, bg: Rgb) -> Style {
    Style {
        fg: Color::Rgb(fg),
        bg: Color::Rgb(bg),
        ..Style::DEFAULT
    }
}

/// The screens of tests/reference/screens.tsv, by name.
pub fn reference_screens() -> Vec<(String, Screen)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/reference/screens.tsv");
    let text = std::fs::read_to_string(path).expect("screens.tsv");
    let mut out: Vec<(String, Screen)> = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "screen" => out.push((
                f[1].to_owned(),
                Screen::new(
                    f[2].parse().unwrap(),
                    f[3].parse().unwrap(),
                    rgb(f[4]),
                    rgb(f[5]),
                ),
            )),
            "cell" => {
                let (_, s) = out.last_mut().expect("a cell before its screen");
                let (col, row): (usize, usize) = (f[1].parse().unwrap(), f[2].parse().unwrap());
                let c = char::from_u32(u32::from_str_radix(f[3], 16).unwrap()).unwrap();
                let mut st = style(rgb(f[5]), rgb(f[6]));
                if f[7] == "1" {
                    st.attrs.insert(Attrs::BOLD);
                }
                st.attrs.set_underline(match f[8] {
                    "0" => UnderlineStyle::None,
                    "1" => UnderlineStyle::Single,
                    "2" => UnderlineStyle::Double,
                    "3" => UnderlineStyle::Curly,
                    "4" => UnderlineStyle::Dotted,
                    _ => UnderlineStyle::Dashed,
                });
                if f[9] == "1" {
                    st.attrs.insert(Attrs::STRIKE);
                }
                if f[10] != "-" {
                    st.underline_color = Color::Rgb(rgb(f[10]));
                }
                s.put(col, row, c, f[4] == "2", st);
            }
            _ => panic!("bad line: {line}"),
        }
    }
    out
}

/// FNV-1a 64 over the width and height as little-endian u32, then each
/// pixel as a little-endian u32.
pub fn digest(c: &Canvas) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: u32| {
        for b in v.to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
    };
    eat(c.width());
    eat(c.height());
    for &p in c.pixels() {
        eat(p);
    }
    h
}

/// A display buffer that keeps what is written, for a test to read back
/// after the renderer is done with it. The renderer's [`Target`] has no
/// way to read, so it cannot see these pixels.
pub struct WriteOnly {
    canvas: Canvas,
    pub writes: usize,
}

impl WriteOnly {
    pub fn new(width: u32, height: u32) -> WriteOnly {
        WriteOnly {
            canvas: Canvas::new(width, height, 0xdead_beef),
            writes: 0,
        }
    }

    pub fn into_canvas(self) -> Canvas {
        self.canvas
    }

    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }
}

impl Target for WriteOnly {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        assert!(
            x as usize + pixels.len() <= self.canvas.width() as usize && y < self.canvas.height(),
            "a write past the display: {x},{y} +{}",
            pixels.len()
        );
        self.writes += 1;
        self.canvas.write(x, y, pixels);
    }
}
