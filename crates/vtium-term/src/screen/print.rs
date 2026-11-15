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
//! as Ghostty does. A codepoint that widens a one-cell cluster takes the
//! cell after it, or in the last column moves the cluster to the next line
//! as a wide character wraps: a VS16 after an emoji-style base, and under
//! `cluster-width = double` a spacing mark or the consonant of a virama
//! conjunct (ADR-1006 D3). Ghostty and foot are MIT-licensed.
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
//!
//! A line of double size (DECDWL, DECDHL) holds half the screen's columns,
//! so printing on it wraps after its last column, half the screen's width,
//! as the right margin of a single-size line makes it wrap. The pending
//! wrap there stands on the last column, as it does before a right margin
//! inside the screen. Such a row is never marked as continuing, since a
//! line of double size does not rewrap (`reflow.rs`). A cursor that a
//! scroll left past the last column of a double-size line prints from
//! that column.
//!
//! OSC 66, kitty's text sizing protocol, writes a sized run: text drawn at
//! a scale across a block of cells, s × w columns wide and s rows high,
//! which every edit treats as one unit. The run's first cell on each row
//! holds its text, and the line's run table holds its place and its size
//! (`Line::runs`). A run too large for the screen is discarded. One that
//! does not fit before the right margin goes to the next line with
//! autowrap, and without autowrap moves left until it fits. One whose last
//! row would lie below the bottom margin scrolls the region up first, and
//! one taller than the region is discarded. The cursor moves past the run
//! on the run's top row. Printed text that lands on a run follows the
//! protocol's overwriting rules: a combining character joins the run's
//! text, text over the run's top row erases the whole run, and text over a
//! lower row moves past the run's cells on that row first. On a line of
//! double size a run takes only its width, one row high.
//!
//! References:
//! - ADR-1308 D2: the keys, and text that sizes nothing printing as it is.
//! - ADR-1308 D3: the run's text, kept to whole grapheme clusters within
//!   the row's cluster budget and one cluster's codepoints.
//! - ADR-1308 D4: writing a run, its cursor and the overwriting rules.
//! - ADR-1308 D7: a run on a double-size line takes only its width.

use core::ops::Range;

use super::{Screen, Segment, WidthMethod};
use crate::bounds::CLUSTER_BYTES_PER_CELL;
use crate::cell::Width;
use crate::line::{Glyph, LinkId, SizedRun, TextSize, run_fit, whole_clusters};
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
                if self.row_has_runs(segment.y)
                    && self
                        .line(segment.y)
                        .and_then(|l| l.run_at(segment.x))
                        .is_some()
                {
                    self.append_to_run(segment.x, segment.y, c, segment.state);
                } else {
                    self.append_to_cluster(segment.x, segment.y, c, segment.state);
                }
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
        let line_last = self.line_cols(self.cursor_y).saturating_sub(1);
        if self.cursor_x > line_last && self.cursor_x < self.cols {
            self.move_cursor(line_last, self.cursor_y);
        }

        if self.cursor_x >= self.cols || self.margin_wrap {
            let edge = self.cursor_x.min(self.cols.saturating_sub(1));
            if self.flags.auto_wrap {
                self.mark_wrapped(self.cursor_y, edge, last);
                self.wrap_line(edge, last);
            } else {
                self.move_cursor(edge, self.cursor_y);
            }
        }

        if !self.skip_lower_rows(last) {
            return;
        }

        if len == 2 && self.cursor_x == self.print_edge(self.cursor_x, self.cursor_y) {
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
        let edge = self.print_edge(from, y);
        if to > edge && edge.saturating_add(1) < self.cols {
            self.move_cursor(edge, y);
            self.margin_wrap = true;
        } else {
            self.move_cursor(to, y);
        }
    }

    /// The last column printing from column `x` of row `y` writes to
    /// before it wraps: the right margin, or the screen's last column right
    /// of the margin (`right_edge`), and on a line of double size no
    /// further than the line's last column.
    fn print_edge(&self, x: usize, y: usize) -> usize {
        self.right_edge(x).min(self.line_cols(y).saturating_sub(1))
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

    /// Marks row `y`'s text as continuing on the next row, for reflow and
    /// reverse wraparound, when a wrap from column `edge` reaches another
    /// row (`wrap_moves`) and goes from the row's last column to the next
    /// row's first. The row's last column is the screen's, or on a line of
    /// double size the last of the half as many columns the line holds. On
    /// the screen's last row below the region the wrap returns to the same
    /// row, whose text does not continue. Returns whether it marked the
    /// row.
    ///
    /// References:
    /// - VT100 User Guide, chapter 3, DECDWL and DECDHL: "The use of
    ///   double-width characters reduces the number of characters per line
    ///   by half."
    fn mark_wrapped(&mut self, y: usize, edge: usize, last: usize) -> bool {
        let edges = self.left == 0 && edge.saturating_add(1) >= self.line_cols(y);
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
    /// state after `c` is `state`. A cluster that `c` makes wide takes two
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
    /// [`unicode::cluster_width_with`]'s under the screen's `cluster-width`,
    /// which is two for an emoji-style base with a VS16 and, under
    /// `double`, for a spacing mark or a virama conjunct (ADR-1006 D1).
    /// What counts is what the line kept, since a line past its cluster
    /// budget keeps only the first codepoints.
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
            return unicode::width_of(text.chars(), self.cluster_width);
        }
        text.chars().next().map_or(1, unicode::char_width)
    }

    /// A codepoint made the one-cell cluster at `x`, `y` two cells wide: a
    /// VS16, a spacing mark or the consonant of a virama conjunct. The
    /// cluster takes the cell after it, and the cursor moves one cell
    /// further (ADR-1006 D3). In the last column the cluster moves to the start
    /// of the next line, as a wide character wraps, and the cell it left is
    /// cleared; without autowrap, or on a screen one column wide, it stays
    /// narrow.
    fn widen(&mut self, x: usize, y: usize, state: Segmenter) {
        let Some((text, _, style, link)) = self.cell_text(x, y) else {
            return;
        };
        let next = x.saturating_add(1);
        if next <= self.print_edge(x, y) {
            let insert = self.flags.insert && self.in_lr_margins(x);
            let end = if insert { self.edit_end(y) } else { self.cols };
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
        let run = self.line(y).and_then(|l| l.run_at(x));
        if let Some(run) = run {
            x = run.col();
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
        // A run's text holds several clusters, and the codepoint joins the
        // last of them.
        if run.is_some()
            && let Some(last) = last_cluster_state(&text)
        {
            state = last;
        }
        state.continues(c);
        if run.is_some() {
            self.append_to_run(x, y, c, state);
        } else {
            self.append_to_cluster(x, y, c, state);
        }
    }

    /// Before printed text is written at the cursor, moves the cursor past
    /// the cells of each sized run whose lower row it stands on, as the
    /// protocol's fourth overwriting rule asks. A run whose cells reach
    /// the right margin sends the text where text written past the margin
    /// goes: to the next line with autowrap, and nowhere without it.
    /// Returns false when the text goes nowhere.
    ///
    /// References:
    /// - ADR-1308 D4, "Overwriting", rule 4, and vtium's reading where the
    ///   run reaches the margin.
    #[inline]
    fn skip_lower_rows(&mut self, last: usize) -> bool {
        !self.row_has_runs(self.cursor_y) || self.step_past_lower_rows(last)
    }

    /// The work of [`skip_lower_rows`](Self::skip_lower_rows) on a row that
    /// holds runs.
    #[cold]
    #[inline(never)]
    fn step_past_lower_rows(&mut self, last: usize) -> bool {
        // Each pass moves the cursor right or down, so the screen's cells
        // bound the passes.
        for _ in 0..self.cols.saturating_mul(self.rows).saturating_add(1) {
            let (x, y) = (self.cursor_x, self.cursor_y);
            let Some(run) = self.line(y).and_then(|l| l.run_at(x)) else {
                return true;
            };
            if run.row() == 0 {
                return true;
            }
            let edge = self.print_edge(x, y);
            if run.span().end <= edge {
                self.move_cursor(run.span().end, y);
            } else if self.flags.auto_wrap {
                self.wrap_line(edge, last);
            } else {
                return false;
            }
        }
        false
    }

    /// OSC 66: writes `text` at the cursor at the size `size` gives, as
    /// kitty's text sizing protocol defines it. With the width key set,
    /// the whole text is one run s × w columns wide. Without it, each
    /// grapheme cluster of the text, cut as printed text is cut, is a run
    /// of its own, s times the cluster's width. A size that sizes nothing
    /// prints the text as it is. On a line of double size only the width
    /// counts. After a run, REP repeats nothing.
    pub(crate) fn print_sized(&mut self, text: &str, size: TextSize) {
        let size = if self.line_cols(self.cursor_y) < self.cols {
            TextSize {
                width: size.width,
                ..TextSize::NORMAL
            }
        } else {
            size
        };
        if size.is_normal() {
            for c in text.chars() {
                self.print(c);
            }
            return;
        }
        let scale = usize::from(size.scale.max(1));
        if size.width_set() {
            self.write_run(text, scale.saturating_mul(usize::from(size.width)), size);
        } else {
            for cluster in self.clusters(text) {
                let width = usize::from(self.text_width(&cluster));
                if width > 0 {
                    self.write_run(&cluster, scale.saturating_mul(width), size);
                }
            }
        }
        self.last_printed = None;
    }

    /// `text` cut into grapheme clusters as printing cuts it: by UAX #29,
    /// or under mode 2027 reset by codepoint width.
    fn clusters(&self, text: &str) -> Vec<ClusterText> {
        let mut out: Vec<ClusterText> = Vec::new();
        let mut state: Option<Segmenter> = None;
        for c in text.chars() {
            let joins = match (&mut state, out.last()) {
                (Some(state), Some(_)) => match self.width_method {
                    WidthMethod::Unicode => state.continues(c),
                    WidthMethod::Wcwidth => unicode::char_width(c) == 0,
                },
                _ => false,
            };
            match out.last_mut() {
                Some(cluster) if joins => cluster.push(c),
                _ => {
                    out.push(c.into());
                    state = Some(Segmenter::start(c));
                }
            }
        }
        out
    }

    /// The cells the grapheme cluster `text` takes when printed.
    fn text_width(&self, text: &str) -> u8 {
        if self.width_method == WidthMethod::Unicode {
            return unicode::width_of(text.chars(), self.cluster_width);
        }
        text.chars().next().map_or(0, unicode::char_width)
    }

    /// Writes one sized run of `text`, `cols` columns wide and the scale's
    /// rows high, at the cursor, and moves the cursor past it. When a row
    /// the run would lie on is double size, vtium writes the text one row
    /// high instead, as OSC 66 writes on a double-size line.
    fn write_run(&mut self, text: &str, cols: usize, size: TextSize) {
        let rows = usize::from(size.scale.max(1));
        if cols == 0 || cols > self.line_cols(self.cursor_y) || rows > self.rows {
            return;
        }
        self.sized_runs = true;
        self.segment = None;
        let last = self.last_print_row(self.cursor_y);
        // A pending wrap goes first, as it does before printed text.
        if self.cursor_x >= self.cols || self.margin_wrap {
            let edge = self.cursor_x.min(self.cols.saturating_sub(1));
            if self.flags.auto_wrap {
                self.mark_wrapped(self.cursor_y, edge, last);
                self.wrap_line(edge, last);
            } else {
                self.move_cursor(edge, self.cursor_y);
            }
        }
        if !self.skip_lower_rows(last) {
            return;
        }
        let (x, y) = (self.cursor_x, self.cursor_y);
        let edge = self.print_edge(x, y);
        let room = edge.saturating_add(1);
        if x.saturating_add(cols) > room {
            let fits_after_wrap = cols <= room.saturating_sub(self.left);
            if self.flags.auto_wrap && x > self.left && fits_after_wrap {
                self.wrap_line(edge, last);
                if !self.skip_lower_rows(last) {
                    return;
                }
            } else {
                self.move_cursor(room.saturating_sub(cols), y);
            }
        }
        let (x, mut y) = (self.cursor_x, self.cursor_y);
        if x.saturating_add(cols) > self.line_cols(y) {
            return;
        }
        if rows > 1 && !self.run_rows_single_size(y, rows) {
            self.write_run_flat(text, cols.checked_div(rows).unwrap_or(cols), size);
            return;
        }
        // The run's last row lies on or above the bottom margin: the region
        // scrolls up first, and the cursor moves up with the text. Below
        // the region the run must fit above the screen's last row.
        let bottom_row = y.saturating_add(rows).saturating_sub(1);
        if y >= self.top && y <= self.bottom {
            if rows > self.bottom.saturating_add(1).saturating_sub(self.top) {
                return;
            }
            if bottom_row > self.bottom {
                let n = bottom_row.saturating_sub(self.bottom);
                self.scroll_up(n);
                y = y.saturating_sub(n);
                self.move_cursor(x, y);
            }
        } else if bottom_row >= self.rows {
            return;
        }
        #[cfg(feature = "bidi")]
        if x == 0 {
            self.carry_paragraph(y);
        }
        let (pen, link) = (self.pen, self.link);
        let end = x.saturating_add(cols);
        // Every row is cleared before any is written, so no erase of an old
        // run takes a share of the new one.
        for r in 0..rows {
            let row = y.saturating_add(r);
            self.erase_runs_in(row, x..end);
            self.split_wide(row, x);
            self.split_wide(row, end);
            self.images_cut(row, x, end.saturating_sub(1));
        }
        // Every row holds the same text: what the line with the least room
        // keeps.
        let room = (y..y.saturating_add(rows))
            .filter_map(|row| self.line(row).map(|l| l.run_room(x..end)))
            .min()
            .unwrap_or(0);
        let own = cols.saturating_mul(CLUSTER_BYTES_PER_CELL);
        let kept = run_fit(whole_clusters(text, own), room);
        let glyph = glyph_of(kept);
        for r in 0..rows {
            let row = y.saturating_add(r);
            if let Some(line) = self.line_mut(row) {
                line.put_run(SizedRun::new(x, cols, r, size), glyph, &pen, link);
            }
            self.mark_cells(row, x..end);
            self.mark_reflow(row);
        }
        self.advance(x, end, y);
        if let Some(state) = last_cluster_state(kept) {
            self.set_segment(x, y, state);
        }
    }

    /// Whether every row that a run `rows` high, written at row `y`, lies on
    /// is single size. Inside the scrolling region, the rows past its
    /// bottom margin are the new single-size lines that the region scrolls
    /// in for the run.
    fn run_rows_single_size(&self, y: usize, rows: usize) -> bool {
        let mut last = y.saturating_add(rows).saturating_sub(1);
        if y >= self.top && y <= self.bottom {
            last = last.min(self.bottom);
        }
        (y..=last).all(|row| self.line(row).is_none_or(|l| !l.flags().size().is_double()))
    }

    /// Writes `text` as OSC 66 writes it on a double-size line: as a run of
    /// scale 1 `width` columns wide when the program set the width, and as
    /// printed text when it did not. The other keys size nothing there.
    ///
    /// References:
    /// - ADR-1308 D7: a block of several rows across lines of different
    ///   sizes has no meaning, so vtium writes it one row high.
    fn write_run_flat(&mut self, text: &str, width: usize, size: TextSize) {
        let flat = TextSize {
            width: size.width,
            ..TextSize::NORMAL
        };
        if flat.is_normal() {
            for c in text.chars() {
                self.print(c);
            }
        } else {
            self.write_run(text, width, flat);
        }
    }

    /// Adds `c`, a codepoint that joins the text before it, to the text of
    /// the sized run whose cell `x`, `y` is, on every row of the run, when
    /// the text keeps within its bound. The run keeps its cells: no
    /// codepoint changes a run's width.
    ///
    /// References:
    /// - ADR-1308 D2: a cell whose width w set keeps it, whatever VS15,
    ///   VS16 or a spacing mark says.
    /// - ADR-1308 D4, "Overwriting", rule 1: a combining character joins
    ///   the run's text, within D3's bound.
    fn append_to_run(&mut self, x: usize, y: usize, c: char, state: Segmenter) {
        let Some(run) = self.line(y).and_then(|l| l.run_at(x)) else {
            return;
        };
        let Some((mut text, ..)) = self.cell_text(run.col(), y) else {
            return;
        };
        text.push(c);
        let top = y.saturating_sub(run.row());
        // Every row must hold the longer text whole, so all rows keep the
        // same text.
        let room = (top..top.saturating_add(run.rows()))
            .filter_map(|row| self.line(row).map(|l| l.run_room(run.span())))
            .min()
            .unwrap_or(0);
        let own = run.cols().saturating_mul(CLUSTER_BYTES_PER_CELL);
        if run_fit(whole_clusters(&text, own), room).len() < text.len() {
            return;
        }
        for r in 0..run.rows() {
            let row = top.saturating_add(r);
            let same = self
                .line(row)
                .and_then(|l| l.run_at(run.col()))
                .filter(|s| s.span() == run.span() && s.row() == r);
            if let (Some(share), Some(line)) = (same, self.line_mut(row)) {
                line.set_run_text(share, glyph_of(&text));
                self.mark_cells(row, run.span());
            }
        }
        self.set_segment(run.col(), y, state);
    }

    /// Erases each sized run on row `y` that a write of the cells `cols`
    /// cuts or covers, whole: with `insert` the margin where insert mode
    /// moves the row's cells up to, each run the move would cut as well.
    /// The erase also takes each run across the boundaries the write
    /// splits wide characters at, so the write splits only characters.
    #[cold]
    #[inline(never)]
    fn erase_runs_written_over(&mut self, y: usize, cols: Range<usize>, insert: Option<usize>) {
        if let Some(margin) = insert {
            let n = cols.len();
            self.erase_runs_shifted(y..=y, cols.start, n, margin, true);
        }
        self.erase_runs_in(y, cols);
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
        let margin = if insert { self.edit_end(y) } else { cols };
        // text written over a picture text cuts removes the part it
        // covers; in insert mode the rest of the row shifts, as ICH does
        let last = if insert {
            margin.saturating_sub(1)
        } else {
            end.saturating_sub(1)
        };
        self.images_cut(y, x, last);
        if self.row_has_runs(y) {
            self.erase_runs_written_over(y, x..end, insert.then_some(margin));
        }
        self.split_wide_char(y, x);
        if insert {
            if len < margin {
                self.split_wide_char(y, margin.saturating_sub(len));
            }
            self.split_wide_char(y, margin);
            if len < margin && x < margin.saturating_sub(len) {
                let blank = self.blank_style();
                if let Some(line) = self.line_mut(y) {
                    line.insert_cells(x, len, margin, &blank);
                }
                self.mark_cells(y, x..margin);
            }
        } else {
            self.split_wide_char(y, end);
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

/// The segmentation state after the last grapheme cluster of `text`, for
/// a codepoint printed after it to join, or `None` for empty text.
fn last_cluster_state(text: &str) -> Option<Segmenter> {
    let mut state: Option<Segmenter> = None;
    for c in text.chars() {
        if !state.as_mut().is_some_and(|s| s.continues(c)) {
            state = Some(Segmenter::start(c));
        }
    }
    state
}

/// The glyph of a cell's text: one codepoint, or a cluster.
pub(super) fn glyph_of(text: &str) -> Glyph<'_> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (None, _) => Glyph::Empty,
        (Some(c), None) => Glyph::Char(c),
        _ => Glyph::Cluster(text),
    }
}
