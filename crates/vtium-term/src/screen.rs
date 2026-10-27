//! The screen: the lines a program writes, the cursor, the margins and tab
//! stops, the alternate screen, the scrollback and the selection.
//!
//! The screen is what the terminal's dispatch drives: printing, cursor
//! movement, erasing, inserting and deleting, scrolling and resizing. The
//! modes that change how it behaves (insert, autowrap, origin, reverse
//! video, the cursor's visibility, the alternate screen, how printed text
//! is cut into cells) are plain accessors here; the mode table that maps
//! SM, RM and DECRQM onto them is the terminal's (vtium/vtium#12).
//!
//! Every change marks what it changed in the screen's [`Damage`], by the
//! rule `damage.rs` states, and [`Screen::frame`] gives a renderer the view
//! with each cell marked changed or not for it.
//!
//! The cursor's column may equal the screen's width: the next printed
//! character wraps first (the pending wrap). Every other position is
//! inside the screen.

mod edit;
mod print;
mod selection;

use core::ops::Range;

use crate::Cell;
use crate::bounds::{COLUMNS_MAX, ROWS_MAX};
use crate::damage::{Age, Damage};
use crate::frame::{Cursor, CursorShape, Frame};
use crate::line::{Line, LineId, LinkId};
use crate::palette::Palette;
use crate::scrollback::Scrollback;
use crate::style::Style;
use crate::unicode::Segmenter;

pub use edit::Erase;
use selection::Selection;

/// How printed text is cut into cells: mode 2027.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WidthMethod {
    /// Grapheme clusters by UAX #29, one cell or two each, a VS16 widening
    /// an emoji-style base (mode 2027 set).
    #[default]
    Unicode,
    /// Each codepoint of positive width starts a cell, as wcwidth counts
    /// it; a codepoint of no width joins the cell before (mode 2027 reset).
    Wcwidth,
}

/// The screen's flags, as the replay interface names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    insert: bool,
    auto_wrap: bool,
    origin: bool,
    reverse: bool,
    cursor_hidden: bool,
    alternate: bool,
}

impl Flags {
    /// After a reset: autowrap on, as the terminal's reset leaves it, and
    /// everything else off.
    const RESET: Self = Self {
        insert: false,
        auto_wrap: true,
        origin: false,
        reverse: false,
        cursor_hidden: false,
        alternate: false,
    };
}

/// The cluster the last print wrote, which the next print may join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Segment {
    x: usize,
    y: usize,
    /// The cell as the print left it: anything else there now means the
    /// cluster is gone.
    cell: Cell,
    state: Segmenter,
}

/// A terminal screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    cols: usize,
    rows: usize,
    /// The main and alternate screens' lines. Each holds at least `rows`
    /// lines: lines below the screen after it shrank keep their text and
    /// come back when it grows again, as in the C reference.
    main: Vec<Line>,
    alt: Vec<Line>,
    flags: Flags,
    cursor_x: usize,
    cursor_y: usize,
    /// The scroll region, `top..=bottom`.
    top: usize,
    bottom: usize,
    tabs: Vec<bool>,
    scrollback: Scrollback,
    damage: Damage,
    pen: Style,
    link: Option<LinkId>,
    next_id: u64,
    segment: Option<Segment>,
    /// The codepoint that started the last printed cell, which REP repeats.
    last_printed: Option<char>,
    width_method: WidthMethod,
    selection: Selection,
    cursor_shape: CursorShape,
    cursor_blink: bool,
}

impl Screen {
    /// A blank screen of `cols` by `rows`, each at least 1 and at most
    /// [`COLUMNS_MAX`] by [`ROWS_MAX`], whose scrollback may be set to at
    /// most `scrollback_cap` lines. The binary passes `sb_size_cap` for
    /// the machine, and the scrollback starts empty, holding no lines until
    /// [`set_scrollback_max`](Self::set_scrollback_max) sets `sb-size`.
    pub fn new(cols: usize, rows: usize, scrollback_cap: usize) -> Self {
        let cols = cols.clamp(1, COLUMNS_MAX);
        let rows = rows.clamp(1, ROWS_MAX);
        let mut screen = Self {
            cols,
            rows,
            main: Vec::with_capacity(rows),
            alt: Vec::with_capacity(rows),
            flags: Flags::RESET,
            cursor_x: 0,
            cursor_y: 0,
            top: 0,
            bottom: rows.saturating_sub(1),
            tabs: default_tabs(cols),
            scrollback: Scrollback::new(scrollback_cap),
            damage: Damage::new(cols, rows),
            pen: Style::DEFAULT,
            link: None,
            next_id: 0,
            segment: None,
            last_printed: None,
            width_method: WidthMethod::default(),
            selection: Selection::default(),
            cursor_shape: CursorShape::default(),
            cursor_blink: true,
        };
        for _ in 0..rows {
            let line = screen.new_line(cols, &Style::DEFAULT);
            screen.main.push(line);
            let line = screen.new_line(cols, &Style::DEFAULT);
            screen.alt.push(line);
        }
        screen
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The cursor's column and row. The column equals [`cols`](Self::cols)
    /// while a wrap is pending.
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_x, self.cursor_y)
    }

    /// The scroll region's first and last rows (DECSTBM).
    pub fn margins(&self) -> (usize, usize) {
        (self.top, self.bottom)
    }

    /// The line at `row` of the screen in use, main or alternate.
    pub fn line(&self, row: usize) -> Option<&Line> {
        self.active().get(row).filter(|_| row < self.rows)
    }

    /// The lines of the screen in use, top to bottom.
    pub fn lines(&self) -> &[Line] {
        self.active().get(..self.rows).unwrap_or_default()
    }

    /// The scrollback's lines, oldest first.
    pub fn scrollback(&self) -> impl ExactSizeIterator<Item = &Line> + '_ {
        self.scrollback.lines()
    }

    /// How many lines the view is scrolled back: 0 shows the screen.
    pub fn scrolled_back(&self) -> usize {
        self.scrollback.offset()
    }

    /// The pen: the style printed text takes. SGR sets it
    /// (vtium/vtium#12).
    pub fn pen(&self) -> &Style {
        &self.pen
    }

    pub fn set_pen(&mut self, pen: Style) {
        self.pen = pen;
    }

    /// The hyperlink printed text takes (OSC 8, vtium/vtium#40).
    pub fn link(&self) -> Option<LinkId> {
        self.link
    }

    pub fn set_link(&mut self, link: Option<LinkId>) {
        self.link = link;
    }

    /// The style erased, inserted and scrolled-in cells take: the pen's
    /// background and nothing else of it (background colour erase). SGR 7
    /// is not applied: the cell takes the background as set.
    fn blank_style(&self) -> Style {
        Style {
            bg: self.pen.bg,
            ..Style::DEFAULT
        }
    }

    /// IRM: printed text moves the rest of the line right.
    pub fn insert_mode(&self) -> bool {
        self.flags.insert
    }

    pub fn set_insert_mode(&mut self, on: bool) {
        self.flags.insert = on;
    }

    /// DECAWM: printing past the last column wraps to the next line.
    pub fn auto_wrap(&self) -> bool {
        self.flags.auto_wrap
    }

    pub fn set_auto_wrap(&mut self, on: bool) {
        self.flags.auto_wrap = on;
    }

    /// DECOM: cursor addressing is relative to the scroll region.
    pub fn origin_mode(&self) -> bool {
        self.flags.origin
    }

    pub fn set_origin_mode(&mut self, on: bool) {
        self.flags.origin = on;
    }

    /// DECSCNM: the whole screen in reverse video.
    pub fn reverse_video(&self) -> bool {
        self.flags.reverse
    }

    /// Marks everything: a new palette recolours every cell.
    pub(crate) fn recolour(&mut self) {
        self.damage.mark_all();
    }

    /// Marks everything when it changes: every cell draws differently.
    pub fn set_reverse_video(&mut self, on: bool) {
        if self.flags.reverse != on {
            self.flags.reverse = on;
            self.damage.mark_all();
        }
    }

    /// DECTCEM.
    pub fn cursor_visible(&self) -> bool {
        !self.flags.cursor_hidden
    }

    pub fn set_cursor_visible(&mut self, on: bool) {
        self.flags.cursor_hidden = !on;
    }

    /// DECSCUSR's shape and blink.
    pub fn set_cursor_style(&mut self, shape: CursorShape, blink: bool) {
        self.cursor_shape = shape;
        self.cursor_blink = blink;
    }

    /// Whether the alternate screen is in use (modes 47, 1047, 1049).
    pub fn alternate(&self) -> bool {
        self.flags.alternate
    }

    /// Switches between the main and the alternate screen. Neither is
    /// cleared and the cursor stays: the modes that switch decide that.
    /// The selection goes, the view returns to the screen, and everything
    /// is marked.
    pub fn set_alternate(&mut self, on: bool) {
        if self.flags.alternate == on {
            return;
        }
        self.flags.alternate = on;
        self.segment = None;
        self.selection.reset();
        self.scrollback.reset_view();
        self.damage.mark_all();
    }

    pub fn width_method(&self) -> WidthMethod {
        self.width_method
    }

    /// Sets how printed text is cut into cells (mode 2027). Text printed
    /// after the change starts a cell of its own.
    pub fn set_width_method(&mut self, method: WidthMethod) {
        self.width_method = method;
        self.segment = None;
    }

    /// Ends the cluster the last print wrote, so the next print starts a
    /// cell: a control character does this.
    pub fn break_cluster(&mut self) {
        self.segment = None;
    }

    /// Returns the screen to its state after creation, as RIS and DECSTR
    /// need it: the flags, the margins, the tab stops, the main screen in
    /// use, no selection and the view on the screen. The text, the cursor
    /// and the pen stay; the terminal's reset clears and moves them.
    pub fn reset(&mut self) {
        self.segment = None;
        self.last_printed = None;
        self.flags = Flags::RESET;
        self.top = 0;
        self.bottom = self.rows.saturating_sub(1);
        self.tabs = default_tabs(self.cols);
        self.selection.reset();
        self.scrollback.reset_view();
        self.damage.mark_all();
    }

    /// The age a display keeps after drawing a frame: changes after it are
    /// newer.
    pub fn checkpoint(&mut self) -> Age {
        self.damage.checkpoint()
    }

    pub fn damage(&self) -> &Damage {
        &self.damage
    }

    /// The view, as the display that last drew at `since` reads it: the
    /// screen, or the scrollback and the screen while scrolled back, with
    /// the cursor in the palette's cursor colour and the selection.
    pub fn frame<'a>(&'a self, palette: &'a Palette, since: Age) -> Frame<'a> {
        let lines = self.view();
        let offset = self.scrollback.offset();
        let row = self.cursor_y.saturating_add(offset);
        let cursor = Cursor {
            row,
            col: self.cursor_x.min(self.cols.saturating_sub(1)),
            visible: !self.flags.cursor_hidden && row < self.rows,
            shape: self.cursor_shape,
            blink: self.cursor_blink,
            color: palette.cursor(),
        };
        let selected = self.selected_columns(&lines);
        Frame::new(
            lines,
            &self.damage,
            palette,
            since,
            cursor,
            self.flags.reverse,
        )
        .with_selection(selected)
    }

    /// The lines the view shows, top to bottom: from the scrollback while
    /// scrolled back, then the screen.
    fn view(&self) -> Vec<&Line> {
        let screen = self.lines().iter();
        self.scrollback
            .in_view()
            .chain(screen)
            .take(self.rows)
            .collect()
    }

    /// Sets how many lines the scrollback keeps, at most the cap the screen
    /// was made with; the oldest lines past it go.
    pub fn set_scrollback_max(&mut self, lines: usize) {
        self.scrollback.set_max(lines);
        self.damage.mark_all();
    }

    /// Empties the scrollback (ED 3).
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
        self.damage.mark_all();
    }

    /// Scrolls the view `n` lines back into the scrollback.
    pub fn scroll_view_up(&mut self, n: usize) {
        self.scrollback.view_up(n);
        self.damage.mark_all();
    }

    /// Scrolls the view `n` lines forward, towards the screen.
    pub fn scroll_view_down(&mut self, n: usize) {
        self.scrollback.view_down(n);
        self.damage.mark_all();
    }

    /// Returns the view to the screen.
    pub fn reset_view(&mut self) {
        if self.scrollback.offset() != 0 {
            self.scrollback.reset_view();
            self.damage.mark_all();
        }
    }

    /// Resizes the screen to `cols` by `rows`, each at least 1 and at most
    /// [`COLUMNS_MAX`] by [`ROWS_MAX`]. The margins go and the tab stops
    /// return to every eighth column. A shorter main screen pushes its top
    /// lines into the scrollback, keeping the cursor's line on the screen;
    /// with nothing in the scrollback, or on the alternate screen, it
    /// scrolls only as far as the cursor needs. A taller main screen pulls
    /// lines back from the scrollback. Lines keep cells past a narrower
    /// screen's edge, which come back when it widens.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.clamp(1, COLUMNS_MAX);
        let rows = rows.clamp(1, ROWS_MAX);
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        let width = cols.max(self.cols);
        while self.main.len() < rows {
            let line = self.new_line(width, &Style::DEFAULT);
            self.main.push(line);
            let line = self.new_line(width, &Style::DEFAULT);
            self.alt.push(line);
        }
        if cols > self.cols {
            for line in self.main.iter_mut().chain(self.alt.iter_mut()) {
                if line.len() < cols {
                    line.resize(cols);
                }
            }
        }
        self.segment = None;
        self.damage.resize(cols, rows);
        self.top = 0;
        self.bottom = self.rows.saturating_sub(1);
        self.tabs = default_tabs(cols);
        self.cols = cols;
        if self.cursor_x >= cols {
            self.move_cursor(cols.saturating_sub(1), self.cursor_y);
        }
        if rows < self.rows {
            if self.scrollback.is_empty() || self.flags.alternate {
                if self.cursor_y >= rows {
                    let diff = self.cursor_y.saturating_sub(rows).saturating_add(1);
                    self.scroll_up(diff);
                    self.move_cursor(self.cursor_x, rows.saturating_sub(1));
                }
            } else {
                let diff = self.rows.saturating_sub(rows);
                self.scroll_up(diff);
                self.move_cursor(self.cursor_x, self.cursor_y.saturating_sub(diff));
            }
        } else if rows > self.rows && !self.flags.alternate {
            let diff = rows.saturating_sub(self.rows).min(self.scrollback.len());
            if diff > 0 {
                self.rows = rows;
                self.bottom = rows.saturating_sub(1);
                self.rotate_down(diff);
                self.pull_from_scrollback(diff);
                self.move_cursor(self.cursor_x, self.cursor_y.saturating_add(diff));
            }
        }
        self.rows = rows;
        self.bottom = rows.saturating_sub(1);
        if self.cursor_y >= rows {
            self.move_cursor(self.cursor_x, rows.saturating_sub(1));
        }
    }

    /// Replaces the top `n` lines with the newest `n` of the scrollback,
    /// the newest at the bottom of them, widened to the screen.
    fn pull_from_scrollback(&mut self, n: usize) {
        let cols = self.cols;
        for row in (0..n).rev() {
            let Some(mut line) = self.scrollback.pop_newest() else {
                return;
            };
            if line.len() < cols {
                line.resize(cols);
            }
            // The line replaced is one `rotate_down` just made, which
            // holds no selection.
            if let Some(slot) = self.main.get_mut(row) {
                *slot = line;
            }
        }
    }

    // Cursor movement.

    /// Moves the cursor, which ends the cluster the last print wrote.
    fn move_cursor(&mut self, x: usize, y: usize) {
        self.segment = None;
        self.cursor_x = x;
        self.cursor_y = y;
    }

    /// CUP: to column `x`, row `y`, counted from the scroll region's top
    /// in origin mode, and kept inside the screen, or the region.
    pub fn move_to(&mut self, x: usize, y: usize) {
        let (y, last) = if self.flags.origin {
            (self.top.saturating_add(y), self.bottom)
        } else {
            (y, self.rows.saturating_sub(1))
        };
        self.move_cursor(x.min(self.cols.saturating_sub(1)), y.min(last));
    }

    /// DECRC: to column `x`, row `y` of the screen, whatever the origin
    /// mode, kept inside the screen. A column at the screen's width is the
    /// pending wrap DECSC saved, kept while autowrap is on.
    pub fn restore_cursor(&mut self, x: usize, y: usize) {
        let last = if self.flags.auto_wrap {
            self.cols
        } else {
            self.cols.saturating_sub(1)
        };
        self.move_cursor(x.min(last), y.min(self.rows.saturating_sub(1)));
    }

    /// CHA and HPA: to column `x` of the cursor's row.
    pub fn move_to_column(&mut self, x: usize) {
        self.move_cursor(x.min(self.cols.saturating_sub(1)), self.cursor_y);
    }

    /// VPR: `n` rows down, kept inside the screen, or the region in origin
    /// mode, with no scrolling. The column leaves a pending wrap.
    pub fn move_rows_down(&mut self, n: usize) {
        let last = if self.flags.origin {
            self.bottom
        } else {
            self.rows.saturating_sub(1)
        };
        let y = self.cursor_y.saturating_add(n).min(last);
        self.move_cursor(self.cursor_x.min(self.cols.saturating_sub(1)), y);
    }

    /// CUU, and RI with `scroll`: `n` rows up, stopping at the top margin
    /// when the cursor is inside the region, and scrolling the region down
    /// by what is left when `scroll` is set. Above the region the cursor
    /// stops at the screen's first row and nothing scrolls.
    pub fn move_up(&mut self, n: usize, scroll: bool) {
        if n == 0 {
            return;
        }
        let stop = if self.cursor_y >= self.top {
            self.top
        } else {
            0
        };
        let room = self.cursor_y.saturating_sub(stop);
        if n > room {
            if scroll && self.cursor_y >= self.top {
                self.scroll_down(n.saturating_sub(room));
            }
            self.move_cursor(self.cursor_x, stop);
        } else {
            self.move_cursor(self.cursor_x, self.cursor_y.saturating_sub(n));
        }
    }

    /// CUD, and IND and LF with `scroll`: `n` rows down, stopping at the
    /// bottom margin when the cursor is inside the region, and scrolling the
    /// region up by what is left when `scroll` is set. Below the region the
    /// cursor stops at the screen's last row and nothing scrolls.
    pub fn move_down(&mut self, n: usize, scroll: bool) {
        if n == 0 {
            return;
        }
        let end = if self.cursor_y <= self.bottom {
            self.bottom.saturating_add(1)
        } else {
            self.rows
        };
        let room = end.saturating_sub(self.cursor_y).saturating_sub(1);
        if n > room {
            if scroll && self.cursor_y <= self.bottom {
                self.scroll_up(n.saturating_sub(room));
            }
            self.move_cursor(self.cursor_x, end.saturating_sub(1));
        } else {
            self.move_cursor(self.cursor_x, self.cursor_y.saturating_add(n));
        }
    }

    /// CUB and BS: `n` columns left, stopping at the first.
    pub fn move_left(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let x = self.cursor_x.min(self.cols.saturating_sub(1));
        self.move_cursor(x.saturating_sub(n), self.cursor_y);
    }

    /// CUF and HPR: `n` columns right, stopping at the last.
    pub fn move_right(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let x = self
            .cursor_x
            .saturating_add(n.min(self.cols))
            .min(self.cols.saturating_sub(1));
        self.move_cursor(x, self.cursor_y);
    }

    /// CR: to the first column.
    pub fn move_line_home(&mut self) {
        self.move_cursor(0, self.cursor_y);
    }

    /// NEL, and LF in newline mode: the first column of the next row,
    /// scrolling at the bottom margin.
    pub fn newline(&mut self) {
        self.move_down(1, true);
        self.move_line_home();
    }

    /// HT and CHT: to the `n`th tab stop right, or the last column. A tab
    /// never leaves a pending wrap.
    pub fn tab_right(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let mut x = self.cursor_x;
        for _ in 0..n {
            let from = x.saturating_add(1);
            x = (from..self.cols)
                .find(|&c| self.tabs.get(c).copied().unwrap_or(false))
                .unwrap_or(self.cols);
            if x.saturating_add(1) >= self.cols {
                break;
            }
        }
        self.move_cursor(x.min(self.cols.saturating_sub(1)), self.cursor_y);
    }

    /// CBT: to the `n`th tab stop left, or the first column.
    pub fn tab_left(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let mut x = self.cursor_x.min(self.cols);
        for _ in 0..n {
            match (1..x)
                .rev()
                .find(|&c| self.tabs.get(c).copied().unwrap_or(false))
            {
                Some(c) => x = c,
                None => {
                    x = 0;
                    break;
                }
            }
        }
        self.move_cursor(x, self.cursor_y);
    }

    /// HTS: a tab stop at the cursor's column.
    pub fn set_tab_stop(&mut self) {
        if let Some(t) = self.tabs.get_mut(self.cursor_x) {
            *t = true;
        }
    }

    /// TBC 0: clears the tab stop at the cursor's column.
    pub fn clear_tab_stop(&mut self) {
        if let Some(t) = self.tabs.get_mut(self.cursor_x) {
            *t = false;
        }
    }

    /// TBC 3: clears every tab stop.
    pub fn clear_tab_stops(&mut self) {
        self.tabs.fill(false);
    }

    /// The tab stops, a flag for each column.
    pub fn tab_stops(&self) -> &[bool] {
        &self.tabs
    }

    /// DECSTBM: the scroll region from row `top` to row `bottom`, counted
    /// from 1, 0 standing for an omitted parameter: the first row and the
    /// last. A bottom past the screen is its last row. A region of one row,
    /// or an inverted one, is ignored, as xterm ignores it; otherwise the
    /// cursor goes home, the region's top in origin mode.
    pub fn set_margins(&mut self, top: usize, bottom: usize) {
        let top = top.clamp(1, self.rows);
        let bottom = if bottom == 0 {
            self.rows
        } else {
            bottom.min(self.rows)
        };
        if bottom <= top {
            return;
        }
        self.top = top.saturating_sub(1);
        self.bottom = bottom.saturating_sub(1);
        self.move_to(0, 0);
    }

    // Lines and damage.

    fn active(&self) -> &Vec<Line> {
        if self.flags.alternate {
            &self.alt
        } else {
            &self.main
        }
    }

    fn active_mut(&mut self) -> &mut Vec<Line> {
        if self.flags.alternate {
            &mut self.alt
        } else {
            &mut self.main
        }
    }

    fn line_mut(&mut self, row: usize) -> Option<&mut Line> {
        let rows = self.rows;
        self.active_mut().get_mut(row).filter(|_| row < rows)
    }

    /// A new line of `cols` cells of `style`, with an identity no other
    /// line of this screen has had.
    fn new_line(&mut self, cols: usize, style: &Style) -> Line {
        let mut line = Line::new(LineId::new(self.next_id), cols);
        self.next_id = self.next_id.wrapping_add(1);
        if *style != Style::DEFAULT {
            line.erase(0..cols, style);
        }
        line
    }

    /// Marks cells of `row` changed. While the view is scrolled back, the
    /// screen's rows are not where the view shows them, so everything is.
    fn mark_cells(&mut self, row: usize, cols: Range<usize>) {
        if self.scrollback.offset() != 0 {
            self.damage.mark_all();
        } else {
            self.damage.mark_cells(row, cols);
        }
    }

    /// Marks whole rows changed, as `mark_cells` does. Rows move only when
    /// lines do, and a selection spans lines in their order, so while one
    /// is active any row outside the moved ones may have left or entered
    /// it: everything is marked.
    fn mark_rows(&mut self, rows: Range<usize>) {
        if self.scrollback.offset() != 0 || self.selection.is_active() {
            self.damage.mark_all();
        } else {
            self.damage.mark_rows(rows);
        }
    }
}

/// A tab stop every eighth column, from the first.
fn default_tabs(cols: usize) -> Vec<bool> {
    (0..cols).map(|c| c & 7 == 0).collect()
}
