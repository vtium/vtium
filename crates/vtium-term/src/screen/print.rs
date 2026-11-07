//! Printing: cutting text into cells, grapheme clusters, wide characters
//! and the pending wrap.
//!
//! A printed codepoint joins the cluster the last print wrote when UAX
//! #29's rules say the cluster continues ([`WidthMethod::Unicode`]), or when
//! it has no width ([`WidthMethod::Wcwidth`]); otherwise it starts a cell.
//! A codepoint of no width that joins nothing joins the cell before the
//! cursor, as foot does, or is dropped when there is none.
//!
//! A wide character takes two cells, the second a tail that draws nothing.
//! One that has only the last column left wraps whole to the next line with
//! autowrap, the cell it leaves a pad in the character's background, as
//! foot pads it and as a rewrap leaves it (ADR-1003); without autowrap it
//! is not printed, as xterm does (Ghostty's notes, `Terminal.zig`,
//! `print`). On a screen one column wide an empty cell is printed instead,
//! as Ghostty does. A VS16 that widens a one-cell cluster takes the cell
//! after it, or in the last column moves the cluster to the next line as a
//! wide character wraps. Ghostty and foot are MIT-licensed.
//!
//! Each print marks the row it writes to as rewrapping at a resize or not,
//! from mode 2028 (ADR-1003).
//!
//! Printed text wraps at the right margin, or at the screen's last column
//! when the cursor is right of the margin, and goes on at the left margin
//! of the next row, as xterm's `dotext` and `WrapLine` print. The wrap
//! scrolls the region at the bottom margin only when the cursor is between
//! the left and right margins, as a line feed scrolls. A row is marked as
//! continuing on the next row, for reflow, only when it wraps from the
//! screen's last column to the next row's first, since only then does its
//! text go on where reflow reads it. A wide character that does not fit
//! before a right margin inside the screen leaves the cell it skips
//! erased, as it does where the wrap returns to the same row. In insert
//! mode, text printed between the margins moves the cells up to the right
//! margin, and text printed outside them moves nothing (xterm's
//! `InsertChar`).

use super::{Screen, Segment, WidthMethod};
use crate::cell::Width;
use crate::line::{Glyph, LinkId};
use crate::style::Style;
use crate::unicode::{self, Segmenter};

/// The arena a cluster is assembled in before its line stores it: a line
/// keeps at most [`CLUSTER_MAX`](crate::CLUSTER_MAX) codepoints of it.
type ClusterText = String;

impl Screen {
    /// Prints `c` at the cursor in the pen's style and link.
    pub fn print(&mut self, c: char) {
        let width = unicode::char_width(c);
        if let Some(mut segment) = self.live_segment() {
            let joins = match self.width_method {
                WidthMethod::Unicode => segment.state.continues(c),
                WidthMethod::Wcwidth => width == 0,
            };
            if joins {
                self.append_to_cluster(segment.x, segment.y, c, segment.state);
                return;
            }
        }
        if width == 0 {
            self.join_before_cursor(c);
            return;
        }
        self.last_printed = Some(c);
        let mut glyph = Glyph::Char(c);
        let mut len = usize::from(width);
        let last = self.last_print_row(self.cursor_y);

        if self.cursor_x >= self.cols || self.margin_wrap {
            let edge = self.cursor_x.min(self.cols.saturating_sub(1));
            if self.flags.auto_wrap {
                self.mark_wrapped(self.cursor_y, edge, last);
                self.wrap_line(edge, last);
            } else {
                self.move_cursor(edge, self.cursor_y);
            }
        }

        if len == 2 && self.cursor_x == self.right_edge(self.cursor_x) {
            if self.cols < 2 {
                glyph = Glyph::Empty;
                len = 1;
            } else if !self.flags.auto_wrap {
                return;
            } else {
                let (x, y) = (self.cursor_x, self.cursor_y);
                self.split_wide(y, x);
                self.split_wide(y, x.saturating_add(1));
                let bg = self.pen.bg;
                self.leave_pad(x, y, last, bg);
                self.wrap_line(x, last);
            }
        }
        // A wrap carries the paragraph's properties to the row it starts.
        #[cfg(feature = "bidi")]
        if self.cursor_x == 0 {
            self.carry_paragraph(self.cursor_y);
        }

        let (x, y) = (self.cursor_x, self.cursor_y);
        let (pen, link) = (self.pen, self.link);
        self.write_cell(x, y, glyph, len, &pen, link);
        self.advance(x, x.saturating_add(len), y);
        let start = if glyph == Glyph::Empty { '\0' } else { c };
        self.set_segment(x, y, Segmenter::start(start));
    }

    /// Moves the cursor past text written from column `from` to `to` on
    /// row `y`. Past a right margin inside the screen, the cursor stays on
    /// the margin's column with a pending wrap.
    fn advance(&mut self, from: usize, to: usize, y: usize) {
        let edge = self.right_edge(from);
        if to > edge && edge.saturating_add(1) < self.cols {
            self.move_cursor(edge, y);
            self.margin_wrap = true;
        } else {
            self.move_cursor(to, y);
        }
    }

    /// Whether a wrap from column `edge` of row `y`, whose last row is
    /// `last`, takes the cursor to another row: below `last`, or at the
    /// bottom margin, which scrolls when `edge` is between the left and
    /// right margins. Otherwise the wrap returns to the same row.
    fn wrap_moves(&self, y: usize, edge: usize, last: usize) -> bool {
        y < last || (last == self.bottom && self.in_lr_margins(edge))
    }

    /// Wraps from column `edge` of the cursor's row: the cursor goes to the
    /// left margin of the next row, and at the bottom margin the region
    /// scrolls when `edge` is between the left and right margins (xterm's
    /// `WrapLine`, which indexes as `xtermIndex` does).
    fn wrap_line(&mut self, edge: usize, last: usize) {
        let y = self.cursor_y;
        if y < last {
            self.move_cursor(self.left, y.saturating_add(1));
            return;
        }
        if self.wrap_moves(y, edge, last) {
            self.scroll_up(1);
        }
        self.move_cursor(self.left, last);
    }

    /// REP: prints the codepoint that started the last printed cell `n`
    /// more times, through the path printed text takes, so the copies wrap
    /// and honour insert mode. After a base and a combining mark it repeats
    /// the base, as Ghostty's `printRepeat` does; each copy starts a cell of
    /// its own. Nothing is repeated before the first print.
    pub fn repeat(&mut self, n: usize) {
        let Some(c) = self.last_printed else {
            return;
        };
        for _ in 0..n {
            self.segment = None;
            self.print(c);
        }
    }

    /// The last row a print at row `y` may use: the bottom margin, where a
    /// wrap scrolls the region, or below the region the screen's last row,
    /// where a wrap returns to the first column and nothing scrolls (VT510
    /// manual, DECAWM: "Any text on the page scrolls up if the cursor is at
    /// the end of the scrolling region").
    pub(super) fn last_print_row(&self, y: usize) -> usize {
        if y <= self.bottom || y >= self.rows {
            self.bottom
        } else {
            self.rows.saturating_sub(1)
        }
    }

    /// Marks row `y`'s text as continuing on the next row, for reflow,
    /// when a wrap from column `edge` reaches another row (`wrap_moves`)
    /// and goes from the screen's last column to the next row's first. On
    /// the screen's last row below the region the wrap returns to the same
    /// row, whose text does not continue. Returns whether it marked the
    /// row.
    fn mark_wrapped(&mut self, y: usize, edge: usize, last: usize) -> bool {
        let edges = self.left == 0 && edge.saturating_add(1) >= self.cols;
        if !edges || !self.wrap_moves(y, edge, last) {
            return false;
        }
        if let Some(line) = self.line_mut(y) {
            line.flags_mut().insert(crate::LineFlags::WRAPPED);
        }
        true
    }

    /// A wide character leaves column `x`, the last before the wrap, of
    /// row `y` for the next row: the row wraps, and the column becomes a
    /// pad in the background `bg`. Where the row is not marked as
    /// continuing (`mark_wrapped`), the column is erased instead.
    fn leave_pad(&mut self, x: usize, y: usize, last: usize, bg: crate::Color) {
        if self.mark_wrapped(y, x, last) {
            let pad = Style {
                bg,
                ..Style::DEFAULT
            };
            if let Some(line) = self.line_mut(y) {
                line.write(x, Glyph::Empty, Width::Pad, &pad, None);
            }
            self.mark_cells(y, x..x.saturating_add(1));
        } else {
            self.erase_cells(y, x..x.saturating_add(1));
        }
    }

    /// The cluster the last print wrote, if the next print may still join
    /// it: nothing has changed its cell since.
    fn live_segment(&self) -> Option<Segment> {
        self.segment
            .filter(|s| s.x < self.cols && self.line(s.y).is_some_and(|l| l.cell(s.x) == s.cell))
    }

    fn set_segment(&mut self, x: usize, y: usize, state: Segmenter) {
        self.segment = self.line(y).map(|l| Segment {
            x,
            y,
            cell: l.cell(x),
            state,
        });
    }

    /// What the cell at `x`, `y` shows, as text, with its style and link.
    fn cell_text(&self, x: usize, y: usize) -> Option<(ClusterText, Width, Style, Option<LinkId>)> {
        let line = self.line(y)?;
        let cell = line.cell(x);
        let text = match line.glyph(cell) {
            Glyph::Empty => ClusterText::new(),
            Glyph::Char(c) => c.into(),
            Glyph::Cluster(s) => s.into(),
        };
        Some((text, cell.width(), line.style(cell), line.link(cell)))
    }

    /// Adds `c` to the cluster in the cell at `x`, `y`, whose segmentation
    /// state after `c` is `state`. A cluster a VS16 makes wide takes two
    /// cells.
    fn append_to_cluster(&mut self, x: usize, y: usize, c: char, state: Segmenter) {
        let Some((mut text, width, style, link)) = self.cell_text(x, y) else {
            return;
        };
        text.push(c);
        let glyph = if text.chars().nth(1).is_some() {
            Glyph::Cluster(&text)
        } else {
            Glyph::Char(c)
        };
        if let Some(line) = self.line_mut(y) {
            line.write(x, glyph, width, &style, link);
        }
        self.mark_cells(y, x..x.saturating_add(1));
        self.mark_reflow(y);
        self.set_segment(x, y, state);
        if width == Width::Narrow && self.stored_width(x, y) == 2 {
            self.widen(x, y, state);
        }
    }

    /// The cells the cluster stored at `x`, `y` takes: its first
    /// codepoint's width, or, when clusters follow UAX #29,
    /// [`unicode::cluster_width`]'s, which is two for an emoji-style base
    /// with a VS16. What counts is what the line kept, since a
    /// line past its cluster budget keeps only the first codepoints.
    fn stored_width(&self, x: usize, y: usize) -> u8 {
        let Some(line) = self.line(y) else {
            return 1;
        };
        let text = match line.glyph(line.cell(x)) {
            Glyph::Empty => return 1,
            Glyph::Char(c) => return unicode::char_width(c),
            Glyph::Cluster(s) => s,
        };
        if self.width_method == WidthMethod::Unicode {
            return unicode::width_of(text.chars());
        }
        text.chars().next().map_or(1, unicode::char_width)
    }

    /// A VS16 made the one-cell cluster at `x`, `y` two cells wide: it takes
    /// the cell after it. In the last column the cluster moves to the start
    /// of the next line, as a wide character wraps, and the cell it left is
    /// cleared; without autowrap, or on a screen one column wide, it stays
    /// narrow.
    fn widen(&mut self, x: usize, y: usize, state: Segmenter) {
        let Some((text, _, style, link)) = self.cell_text(x, y) else {
            return;
        };
        let next = x.saturating_add(1);
        if next <= self.right_edge(x) {
            let insert = self.flags.insert && self.in_lr_margins(x);
            let end = if insert {
                self.right.saturating_add(1)
            } else {
                self.cols
            };
            let last = if insert { end.saturating_sub(1) } else { next };
            self.images_cut(y, next, last);
            if insert {
                // The rest of the line up to the right margin moves right
                // one cell, as for an inserted wide character; a wide
                // character the margin cuts, before or after the move,
                // goes whole.
                self.split_wide(y, end.saturating_sub(1));
                self.split_wide(y, end);
                let blank = self.blank_style();
                if let Some(line) = self.line_mut(y) {
                    line.insert_cells(next, 1, end, &blank);
                }
                self.mark_cells(y, next..end);
            } else {
                // A wide character there loses its lead, so its tail goes
                // too, past the margin as well.
                self.split_wide(y, next.saturating_add(1));
            }
            if let Some(line) = self.line_mut(y) {
                line.write(next, Glyph::Empty, Width::WideTail, &style, link);
                line.write(x, glyph_of(&text), Width::Wide, &style, link);
            }
            self.mark_cells(y, x..next.saturating_add(1));
            if (self.cursor_x, self.cursor_y) == (next, y) && !self.margin_wrap {
                self.advance(x, next.saturating_add(1), y);
            }
            self.set_segment(x, y, state);
            return;
        }

        if !self.flags.auto_wrap || self.cols < 2 {
            return;
        }
        let last = self.last_print_row(y);
        self.leave_pad(x, y, last, style.bg);
        self.move_cursor(x, y);
        self.wrap_line(x, last);
        let (x, y) = (self.cursor_x, self.cursor_y);
        self.write_cell(x, y, glyph_of(&text), 2, &style, link);
        self.advance(x, x.saturating_add(2), y);
        self.set_segment(x, y, state);
    }

    /// A codepoint of no width after the cursor moved, or that the break
    /// rules keep apart, joins the cell before the cursor (as foot does);
    /// with no cell there it is dropped.
    fn join_before_cursor(&mut self, c: char) {
        let y = self.cursor_y;
        if (self.cursor_x == 0 && !self.margin_wrap) || y >= self.rows {
            return;
        }
        // With a pending wrap at the right margin, the cursor stands on
        // the cell before it.
        let before = if self.margin_wrap {
            self.cursor_x
        } else {
            self.cursor_x.saturating_sub(1)
        };
        let mut x = before.min(self.cols.saturating_sub(1));
        if self
            .line(y)
            .is_some_and(|l| l.cell(x).width() == Width::WideTail)
            && x > 0
        {
            x = x.saturating_sub(1);
        }
        let Some((text, ..)) = self.cell_text(x, y) else {
            return;
        };
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return;
        };
        let mut state = Segmenter::start(first);
        for rest in chars {
            state.continues(rest);
        }
        state.continues(c);
        self.append_to_cluster(x, y, c, state);
    }

    /// Writes `glyph`, `len` cells wide, at `x`, `y` in `style`, its tail
    /// cell taking the same style and link so an erase that skips protected
    /// cells keeps or clears both halves. In insert mode the rest of the line
    /// moves right first. A wide character the write cuts goes whole: at
    /// `x`, and where the write ends or, in insert mode, where the margin
    /// cuts the cells it moves.
    pub(super) fn write_cell(
        &mut self,
        x: usize,
        y: usize,
        glyph: Glyph<'_>,
        len: usize,
        style: &Style,
        link: Option<LinkId>,
    ) {
        let cols = self.cols;
        if len == 0 || x >= cols || y >= self.rows {
            return;
        }
        let end = x.saturating_add(len);
        // Insert mode moves the cells up to the right margin, and only
        // from a column between the margins.
        let insert = self.flags.insert && self.in_lr_margins(x);
        let margin = if insert {
            self.right.saturating_add(1).min(cols)
        } else {
            cols
        };
        // text written over a picture text cuts removes the part it
        // covers; in insert mode the rest of the row shifts, as ICH does
        let last = if insert {
            margin.saturating_sub(1)
        } else {
            end.saturating_sub(1)
        };
        self.images_cut(y, x, last);
        self.split_wide(y, x);
        if insert {
            if len < margin {
                self.split_wide(y, margin.saturating_sub(len));
            }
            self.split_wide(y, margin);
            if len < margin && x < margin.saturating_sub(len) {
                let blank = self.blank_style();
                if let Some(line) = self.line_mut(y) {
                    line.insert_cells(x, len, margin, &blank);
                }
                self.mark_cells(y, x..margin);
            }
        } else {
            self.split_wide(y, end);
        }
        let width = if len == 2 { Width::Wide } else { Width::Narrow };
        if let Some(line) = self.line_mut(y) {
            line.write(x, glyph, width, style, link);
            for tail in x.saturating_add(1)..end.min(cols) {
                line.write(tail, Glyph::Empty, Width::WideTail, style, link);
            }
        }
        self.mark_cells(y, x..end);
        self.mark_reflow(y);
    }
}

/// The glyph of a cell's text: one codepoint, or a cluster.
fn glyph_of(text: &str) -> Glyph<'_> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (None, _) => Glyph::Empty,
        (Some(c), None) => Glyph::Char(c),
        _ => Glyph::Cluster(text),
    }
}
