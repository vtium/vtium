//! The Backend through which ratatui draws the TUI: it turns ratatui's
//! cells into ECMA-48 cursor positions, SGR and text in a buffer of bytes,
//! and the loop writes those bytes to the terminal. ratatui itself never
//! writes to the terminal, so vtium-config keeps every byte it sends, and
//! the pty tests check them.
//!
//! The console measures a character's width by vtium-term's rule, while
//! ratatui's buffer measures it by Unicode's tables, and the two can
//! disagree. The Backend therefore follows the terminal's cursor by the
//! console's rule. Whenever the next cell ratatui hands it is not where
//! that cursor stands, the Backend writes a cursor position first. So a
//! character the two rules measure differently can misdraw its own cell,
//! but it never shifts the rest of the row.
//!
//! References:
//! - ADR-1206 D1: no ratatui backend crate; vtium-config's own Backend over
//!   its own terminal layer, and the console's width rule decides where the
//!   cursor goes.

use std::io::{self, Write as _};

use ratatui_core::backend::{Backend, ClearType, WindowSize};
use ratatui_core::buffer::Cell;
use ratatui_core::layout::{Position, Size};
use ratatui_core::style::{Color, Modifier};

/// The modifier that stands for SGR 73, superscript, which ratatui has no
/// modifier of its own for. The TUI never blinks, so the Backend reads
/// ratatui's two blink modifiers as the two script positions that only
/// the font preview in a vtium console asks for.
pub const SUPER: Modifier = Modifier::SLOW_BLINK;

/// The modifier that stands for SGR 74, subscript; see [`SUPER`].
pub const SUB: Modifier = Modifier::RAPID_BLINK;

/// ratatui's cells as bytes for a terminal of `cols` by `rows` cells.
/// [`Vt::take`] hands the loop what a frame wrote.
#[derive(Debug)]
pub struct Vt {
    out: Vec<u8>,
    cols: u16,
    rows: u16,
    /// Whether the terminal is sent colors. With colors off, the Backend
    /// still sends bold, italic, underline, reverse video and the script
    /// positions.
    color: bool,
}

impl Vt {
    pub fn new(cols: u16, rows: u16, color: bool) -> Vt {
        Vt {
            out: Vec::new(),
            cols,
            rows,
            color,
        }
    }

    /// Sets the terminal's size, which ratatui reads before each frame; a
    /// new size makes ratatui clear the screen and draw the next frame
    /// whole.
    pub fn set_size(&mut self, cols: u16, rows: u16) {
        (self.cols, self.rows) = (cols, rows);
    }

    pub fn set_color(&mut self, color: bool) {
        self.color = color;
    }

    /// The bytes written since the last call.
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }
}

/// How many cells the console gives `symbol`: vtium-term's width rule for
/// the cluster, so the Backend knows where the terminal's cursor stops.
pub fn console_width(symbol: &str) -> u16 {
    let chars: Vec<char> = symbol.chars().collect();
    if chars.is_empty() {
        return 0;
    }
    u16::from(vtium_term::cluster_width(&chars))
}

/// SGR's parameter for `color` as the text's color (`base` 30) or the
/// background's (`base` 40), or nothing for the terminal's own color.
fn color_param(color: Color, base: u8) -> Option<String> {
    let index = match color {
        Color::Reset => return None,
        Color::Rgb(r, g, b) => return Some(format!("{};2;{r};{g};{b}", base.saturating_add(8))),
        Color::Indexed(n) => n,
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
    };
    // the sixteen colors by their own codes, as the TUI has always sent
    // them, and the rest of the 256 by their index
    Some(match index {
        0..=7 => base.saturating_add(index).to_string(),
        8..=15 => base
            .saturating_add(60)
            .saturating_add(index.saturating_sub(8))
            .to_string(),
        _ => format!("{};5;{index}", base.saturating_add(8)),
    })
}

/// The SGR sequence that sets `cell`'s style from the terminal's plain
/// one, or nothing for plain text. The parameters come in the order the
/// TUI has always sent them: bold, italic, underline, reverse video, the
/// script position, the text's color, the background's.
pub fn sgr(cell: &Cell, color: bool) -> String {
    let m = cell.modifier;
    let mut params: Vec<String> = Vec::new();
    for (bit, code) in [
        (Modifier::BOLD, "1"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::REVERSED, "7"),
        (SUPER, "73"),
        (SUB, "74"),
    ] {
        if m.contains(bit) {
            params.push(code.to_owned());
        }
    }
    if color {
        params.extend(color_param(cell.fg, 30));
        params.extend(color_param(cell.bg, 40));
    }
    if params.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", params.join(";"))
    }
}

impl Backend for Vt {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        // where the terminal's cursor stands, by the console's width rule;
        // none after a character in the last column, where the cursor
        // waits to wrap
        let mut cursor: Option<(u16, u16)> = None;
        // the style the terminal draws in now; every frame starts and
        // ends in plain text
        let mut style = String::new();
        for (x, y, cell) in content {
            if cursor != Some((x, y)) {
                let _ = write!(
                    self.out,
                    "\x1b[{};{}H",
                    y.saturating_add(1),
                    x.saturating_add(1)
                );
            }
            let want = sgr(cell, self.color);
            if want != style {
                if !style.is_empty() {
                    self.out.extend_from_slice(b"\x1b[0m");
                }
                self.out.extend_from_slice(want.as_bytes());
                style = want;
            }
            // a name read from a file never carries a control onto the
            // screen
            let shown: String = cell
                .symbol()
                .chars()
                .map(|c| if c.is_control() { '?' } else { c })
                .collect();
            self.out.extend_from_slice(shown.as_bytes());
            let after = x.saturating_add(console_width(&shown));
            cursor = (after < self.cols).then_some((after, y));
        }
        if !style.is_empty() {
            self.out.extend_from_slice(b"\x1b[0m");
        }
        Ok(())
    }

    /// The loop's own sequences hide the cursor on entry and show it on
    /// the way out, so the Backend leaves the cursor as it is.
    fn hide_cursor(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(Position::ORIGIN)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let p = position.into();
        let _ = write!(
            self.out,
            "\x1b[{};{}H",
            p.y.saturating_add(1),
            p.x.saturating_add(1)
        );
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.out.extend_from_slice(b"\x1b[2J");
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.out.extend_from_slice(match clear_type {
            ClearType::All => b"\x1b[2J",
            ClearType::AfterCursor => b"\x1b[0J",
            ClearType::BeforeCursor => b"\x1b[1J",
            ClearType::CurrentLine => b"\x1b[2K",
            ClearType::UntilNewLine => b"\x1b[0K",
        });
        Ok(())
    }

    fn size(&self) -> io::Result<Size> {
        Ok(Size::new(self.cols, self.rows))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: Size::new(self.cols, self.rows),
            pixels: Size::new(0, 0),
        })
    }

    /// The loop writes the bytes, with [`Vt::take`], so a flush keeps them.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use ratatui_core::buffer::Buffer;
    use ratatui_core::layout::Rect;
    use ratatui_core::style::{Color, Style};
    use ratatui_core::terminal::Terminal;
    use vtium_term::Glyph;

    use super::*;

    /// The rows a vtium console shows after `bytes`, on a screen of
    /// `cols` by `rows`.
    fn shown(bytes: &[u8], cols: u16, rows: u16) -> Vec<String> {
        let mut t = vtium_term::Terminal::new(usize::from(cols), usize::from(rows), 0);
        t.input(bytes);
        let s = t.screen();
        (0..s.rows())
            .map(|r| {
                let line = s.line(r).expect("a row");
                line.cells()
                    .iter()
                    .map(|&c| match line.glyph(c) {
                        Glyph::Char(ch) => ch.to_string(),
                        Glyph::Cluster(s) => s.to_owned(),
                        Glyph::Empty => " ".to_owned(),
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    /// The bytes ratatui's Terminal writes through [`Vt`] for each of
    /// `frames` in turn, each drawn by `draw` into the frame's buffer.
    fn frames(cols: u16, rows: u16, frames: &[&dyn Fn(&mut Buffer)]) -> Vec<Vec<u8>> {
        let mut t = Terminal::new(Vt::new(cols, rows, true)).expect("a terminal");
        frames
            .iter()
            .map(|draw| {
                t.draw(|f| draw(f.buffer_mut())).expect("a frame");
                t.backend_mut().take()
            })
            .collect()
    }

    #[test]
    fn a_character_the_two_rules_measure_differently_never_shifts_the_rest_of_its_row() {
        // U+17A4, a Khmer vowel, takes two cells by Unicode's tables as
        // ratatui reads them and one by the console's rule
        assert_eq!(console_width("\u{17a4}"), 1);
        let drawn = frames(
            10,
            1,
            &[&|b: &mut Buffer| {
                b.set_string(0, 0, "a\u{17a4}bc", Style::new());
            }],
        );
        let row = &shown(&drawn[0], 10, 1)[0];
        // ratatui put `b` in column 3, after the vowel's two cells; the
        // console keeps it there, and the cell between shows nothing
        assert_eq!(
            row,
            "a\u{17a4} bc",
            "{:?}",
            String::from_utf8_lossy(&drawn[0])
        );
    }

    #[test]
    fn a_second_frame_writes_only_the_cells_that_changed() {
        let drawn = frames(
            12,
            2,
            &[
                &|b: &mut Buffer| {
                    b.set_string(0, 0, "keep this", Style::new());
                    b.set_string(0, 1, "old", Style::new());
                },
                &|b: &mut Buffer| {
                    b.set_string(0, 0, "keep this", Style::new());
                    b.set_string(0, 1, "new", Style::new());
                },
            ],
        );
        let second = String::from_utf8_lossy(&drawn[1]);
        assert_eq!(second, "\x1b[2;1Hnew");
        let mut both = drawn[0].clone();
        both.extend_from_slice(&drawn[1]);
        assert_eq!(shown(&both, 12, 2), ["keep this", "new"]);
    }

    #[test]
    fn styles_change_only_between_runs_and_a_frame_ends_in_plain_text() {
        let drawn = frames(
            8,
            1,
            &[&|b: &mut Buffer| {
                b.set_string(0, 0, "ab", Style::new().fg(Color::Indexed(1)));
                b.set_string(2, 0, "c", Style::new().bg(Color::Rgb(1, 2, 3)));
                b.set_string(3, 0, "d", Style::new());
            }],
        );
        assert_eq!(
            String::from_utf8_lossy(&drawn[0]),
            "\x1b[1;1H\x1b[31mab\x1b[0m\x1b[48;2;1;2;3mc\x1b[0md"
        );
    }

    #[test]
    fn without_color_a_cell_keeps_its_attributes_and_loses_its_colors() {
        let mut t = Terminal::new(Vt::new(4, 1, false)).expect("a terminal");
        t.draw(|f| {
            f.buffer_mut().set_string(
                0,
                0,
                "x",
                Style::new()
                    .fg(Color::Indexed(9))
                    .add_modifier(Modifier::REVERSED),
            );
        })
        .expect("a frame");
        assert_eq!(
            String::from_utf8_lossy(&t.backend_mut().take()),
            "\x1b[1;1H\x1b[7mx\x1b[0m"
        );
    }

    #[test]
    fn a_new_size_clears_the_screen_and_draws_the_frame_whole() {
        let mut t = Terminal::new(Vt::new(4, 1, true)).expect("a terminal");
        let draw = |f: &mut ratatui_core::terminal::Frame| {
            f.buffer_mut().set_string(0, 0, "ab", Style::new());
        };
        t.draw(draw).expect("a frame");
        t.backend_mut().take();
        t.draw(draw).expect("a frame");
        assert!(t.backend_mut().take().is_empty(), "nothing changed");
        t.backend_mut().set_size(5, 1);
        t.draw(draw).expect("a frame");
        assert_eq!(
            String::from_utf8_lossy(&t.backend_mut().take()),
            "\x1b[2J\x1b[1;1Hab"
        );
        assert_eq!(t.get_frame().area(), Rect::new(0, 0, 5, 1));
    }

    #[test]
    fn a_control_in_a_cell_reaches_the_terminal_as_a_question_mark() {
        let mut cell = Cell::default();
        cell.set_symbol("\u{1b}");
        let mut vt = Vt::new(4, 1, true);
        vt.draw(std::iter::once((0, 0, &cell))).expect("drawn");
        assert_eq!(String::from_utf8_lossy(&vt.take()), "\x1b[1;1H?");
    }
}
