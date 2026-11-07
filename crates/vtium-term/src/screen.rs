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
//! inside the screen. A pending wrap at a right margin before the screen's
//! last column leaves the cursor on the margin's column, and a flag holds
//! the wrap.
//!
//! The left and right margins (DECSLRM) are set only while mode 69
//! (DECLRMM) is set, and are the screen's first and last columns while it
//! is reset. With the top and bottom margins they bound the region the
//! scrolling and editing functions work in, as xterm's ctlseqs and the
//! VT510 manual define them; `edit.rs` and `print.rs` say how each function
//! honours them.
//!
//! A resize that changes the column count rewraps the main screen and its
//! scrollback (`reflow.rs`, ADR-1003); no line keeps a cell past the right
//! margin.

mod edit;
mod images;
mod links;
mod marks;
#[cfg(feature = "bidi")]
pub(crate) mod paragraph;
mod print;
mod rect;
pub(crate) mod reflow;
mod selection;

use core::ops::Range;

use crate::Cell;
use crate::bounds::{COLUMNS_MAX, ROWS_MAX, SB_MEMORY_MAX, SB_MEMORY_MIN};
use crate::damage::{Age, Damage};
use crate::frame::{Cursor, CursorShape, Frame};
use crate::line::{Line, LineFlags, LineId, LinkId};
use crate::links::LinkStore;
use crate::palette::Palette;
use crate::scrollback::{Scrollback, lines_bytes_max, sb_memory_held};
use crate::style::Style;
use crate::unicode::Segmenter;

pub use edit::Erase;
use links::Hover;
pub(crate) use rect::{Extent, Rect};
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
    /// Mode 2028: printed rows rewrap at a resize.
    reflow: bool,
    /// Mode 69, DECLRMM: DECSLRM may set left and right margins.
    lr_margins: bool,
}

impl Flags {
    /// After a reset: autowrap and reflow on, as the terminal's reset
    /// leaves them, and everything else off.
    const RESET: Self = Self {
        insert: false,
        auto_wrap: true,
        origin: false,
        reverse: false,
        cursor_hidden: false,
        alternate: false,
        reflow: true,
        lr_margins: false,
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
    /// The main and alternate screens' lines, each `cols` cells. Each holds
    /// at least `rows` lines: lines below the screen after it shrank keep
    /// their text and come back when it grows again.
    main: Vec<Line>,
    alt: Vec<Line>,
    flags: Flags,
    cursor_x: usize,
    cursor_y: usize,
    /// The scroll region, `top..=bottom`.
    top: usize,
    bottom: usize,
    /// The left and right margins, `left..=right`: the screen's first and
    /// last columns while DECLRMM is reset.
    left: usize,
    right: usize,
    /// A pending wrap at the right margin, which stands before the screen's
    /// last column: the cursor is on the margin's column, and the next
    /// printed character wraps first. Every cursor movement ends it.
    margin_wrap: bool,
    tabs: Vec<bool>,
    scrollback: Scrollback,
    /// While `sb-memory` follows the grid, the most it may be; `None` once
    /// [`set_scrollback_memory`](Self::set_scrollback_memory) fixes it.
    sb_memory_cap: Option<usize>,
    damage: Damage,
    pen: Style,
    /// The open OSC 8 link printed text takes.
    link: Option<LinkId>,
    /// The links the lines' link tables name, on both screens and in the
    /// scrollback.
    links: LinkStore,
    hover: Hover,
    next_id: u64,
    segment: Option<Segment>,
    /// The codepoint that started the last printed cell, which REP repeats.
    last_printed: Option<char>,
    width_method: WidthMethod,
    selection: Selection,
    cursor_shape: CursorShape,
    cursor_blink: bool,
    /// DECSACE's extent for DECCARA and DECRARA.
    extent: Extent,
    /// The images and placements of both screens (ADR-1100).
    #[cfg(feature = "images")]
    images: vtium_image::Store,
    /// The current values of the paragraph properties, which each new row
    /// takes (ADR-1004 D2; `paragraph.rs`).
    #[cfg(feature = "bidi")]
    paragraph: LineFlags,
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
            left: 0,
            right: cols.saturating_sub(1),
            margin_wrap: false,
            tabs: default_tabs(cols),
            scrollback: Scrollback::new(scrollback_cap),
            sb_memory_cap: Some(SB_MEMORY_MAX),
            damage: Damage::new(cols, rows),
            pen: Style::DEFAULT,
            link: None,
            links: LinkStore::default(),
            hover: Hover::default(),
            next_id: 0,
            segment: None,
            last_printed: None,
            width_method: WidthMethod::default(),
            selection: Selection::default(),
            cursor_shape: CursorShape::default(),
            cursor_blink: true,
            extent: Extent::Stream,
            #[cfg(feature = "images")]
            images: vtium_image::Store::new(),
            #[cfg(feature = "bidi")]
            paragraph: LineFlags::empty(),
        };
        for _ in 0..rows {
            let line = screen.new_line(cols, &Style::DEFAULT);
            screen.main.push(line);
            let line = screen.new_line(cols, &Style::DEFAULT);
            screen.alt.push(line);
        }
        screen.follow_grid();
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

    /// The left and right margins' columns (DECSLRM): the screen's first
    /// and last while DECLRMM is reset.
    pub fn lr_margins(&self) -> (usize, usize) {
        (self.left, self.right)
    }

    /// Whether a pending wrap waits at a right margin that stands before
    /// the screen's last column, with the cursor on the margin's column.
    pub fn margin_wrap(&self) -> bool {
        self.margin_wrap
    }

    /// Mode 69, DECLRMM: DECSLRM may set left and right margins.
    pub fn lr_margin_mode(&self) -> bool {
        self.flags.lr_margins
    }

    /// Sets DECLRMM. Resetting it returns the left and right margins to
    /// the screen's first and last columns, as xterm's
    /// `set_left_right_margin_mode` does; the cursor stays.
    pub fn set_lr_margin_mode(&mut self, on: bool) {
        self.flags.lr_margins = on;
        if !on {
            self.reset_lr_margins();
        }
    }

    /// The left and right margins become the screen's first and last
    /// columns.
    pub(super) fn reset_lr_margins(&mut self) {
        self.left = 0;
        self.right = self.cols.saturating_sub(1);
        self.margin_wrap = false;
    }

    /// DECSLRM, while DECLRMM is set: the left and right margins at
    /// columns `left` and `right`, counted from 1, 0 standing for an
    /// omitted parameter: the first column and the last. A right margin
    /// past the screen is its last column. Margins that leave fewer than
    /// two columns between them, or are inverted, are ignored, as xterm
    /// ignores them; otherwise the cursor goes home, the region's top left
    /// corner in origin mode (ctlseqs, DECSLRM; VT510 manual, DECSLRM).
    pub fn set_lr_margins(&mut self, left: usize, right: usize) {
        if !self.flags.lr_margins {
            return;
        }
        let left = left.max(1);
        let right = if right == 0 || right > self.cols {
            self.cols
        } else {
            right
        };
        if right <= left {
            return;
        }
        self.left = left.saturating_sub(1);
        self.right = right.saturating_sub(1);
        self.move_to(0, 0);
    }

    /// Whether the left and right margins are the screen's first and last
    /// columns, so whole lines move when the region scrolls.
    pub(crate) fn full_width(&self) -> bool {
        self.left == 0 && self.right.saturating_add(1) >= self.cols
    }

    /// Whether column `x` is inside the left and right margins. A pending
    /// wrap at the screen's width stands on the last column.
    pub(crate) fn in_lr_margins(&self, x: usize) -> bool {
        let x = x.min(self.cols.saturating_sub(1));
        x >= self.left && x <= self.right
    }

    /// The last column the cursor at column `x` prints to before it wraps,
    /// and that CUF stops at: the right margin, or the screen's last
    /// column when `x` is past the right margin (xterm's `dotext` and
    /// `CursorForward`).
    pub(crate) fn right_edge(&self, x: usize) -> usize {
        if x > self.right {
            self.cols.saturating_sub(1)
        } else {
            self.right
        }
    }

    /// The line at `row` of the screen in use, main or alternate.
    pub fn line(&self, row: usize) -> Option<&Line> {
        self.active().get(row).filter(|_| row < self.rows)
    }

    /// The lines of the screen in use, top to bottom.
    pub fn lines(&self) -> &[Line] {
        self.active().get(..self.rows).unwrap_or_default()
    }

    /// The scrollback's rows at the current width, oldest first. After a
    /// resize that changed the width, the oldest rows may still wait to be
    /// rewrapped, and are not among them until something reaches them, or
    /// [`rewrap_scrollback`](Self::rewrap_scrollback).
    pub fn scrollback(&self) -> impl ExactSizeIterator<Item = &Line> + '_ {
        self.scrollback.lines()
    }

    /// Rewraps every scrollback row still waiting since the width changed:
    /// the scrollback then holds the rows rewrapping it all at the resize
    /// would have made.
    pub fn rewrap_scrollback(&mut self) {
        self.scrollback.rewrap_all();
        self.images_dropped();
    }

    /// The rows the scrollback holds, the rows still waiting to be
    /// rewrapped since the width changed counted at the current width.
    pub fn scrollback_lines(&self) -> usize {
        self.scrollback.total()
    }

    /// The most lines the scrollback keeps: as the host last set it, at
    /// most the cap the screen was made with.
    pub fn scrollback_max(&self) -> usize {
        self.scrollback.max()
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

    /// The hyperlink printed text takes: the link OSC 8 opened
    /// ([`set_hyperlink`](Self::set_hyperlink)).
    pub fn link(&self) -> Option<LinkId> {
        self.link
    }

    /// The style erased, inserted and scrolled-in cells take: the pen's
    /// background and nothing else of it (background color erase). SGR 7
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

    /// Mode 2028: rows printed while it is set rewrap at a resize.
    pub fn reflow(&self) -> bool {
        self.flags.reflow
    }

    /// Sets mode 2028, which marks the cursor's row from it.
    pub fn set_reflow(&mut self, on: bool) {
        self.flags.reflow = on;
        self.mark_reflow(self.cursor_y);
    }

    /// Marks row `y` as rewrapping at a resize or not, from mode 2028.
    fn mark_reflow(&mut self, y: usize) {
        let fixed = !self.flags.reflow;
        if let Some(line) = self.line_mut(y) {
            if fixed {
                line.flags_mut().insert(crate::LineFlags::FIXED);
            } else {
                line.flags_mut().remove(crate::LineFlags::FIXED);
            }
        }
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

    /// Marks everything: a new palette recolors every cell.
    pub(crate) fn recolor(&mut self) {
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
    /// The open link ends, the selection goes, the view returns to the
    /// screen, and everything is marked.
    pub fn set_alternate(&mut self, on: bool) {
        if self.flags.alternate == on {
            return;
        }
        self.flags.alternate = on;
        self.images_alternate(on);
        self.segment = None;
        self.close_link();
        self.selection.reset();
        self.scrollback.reset_view();
        self.damage.mark_all();
        self.refresh_hover();
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
    /// need it: the flags, DECLRMM among them, the margins, the tab stops, the main screen in
    /// use, no selection and the view on the screen. The text, the cursor
    /// and the pen stay; the terminal's reset clears and moves them. Mode
    /// 2028 stays: the mode table sets it on RIS only.
    pub fn reset(&mut self) {
        if self.flags.alternate {
            self.images_alternate(false);
        }
        self.segment = None;
        self.last_printed = None;
        self.flags = Flags {
            reflow: self.flags.reflow,
            ..Flags::RESET
        };
        self.top = 0;
        self.bottom = self.rows.saturating_sub(1);
        self.reset_lr_margins();
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
    /// the cursor in the palette's cursor color, the selection, the
    /// hovered link and the overlay showing its URI.
    pub fn frame<'a>(&'a self, palette: &'a Palette, since: Age) -> Frame<'a> {
        let lines = self.view();
        let offset = self.scrollback.offset();
        let row = self.cursor_y.saturating_add(offset);
        let col = self.cursor_x.min(self.cols.saturating_sub(1));
        // the cursor stays on its logical cell and is drawn where that
        // cell is (ADR-1004 D9)
        #[cfg(feature = "bidi")]
        let order = self.order();
        #[cfg(feature = "bidi")]
        let drawn = order.row(row);
        let cursor = Cursor {
            row,
            #[cfg(feature = "bidi")]
            col: drawn.map_or(col, |r| r.visual_at(col)),
            #[cfg(not(feature = "bidi"))]
            col,
            visible: !self.flags.cursor_hidden && row < self.rows,
            shape: self.cursor_shape,
            blink: self.cursor_blink,
            color: palette.cursor(),
            #[cfg(feature = "bidi")]
            rtl: drawn.is_some_and(|r| r.rtl_at(col)),
        };
        let selected = self.selected_columns(&lines);
        #[cfg(all(feature = "images", feature = "bidi"))]
        let placements = self.placements_in(&order);
        #[cfg(all(feature = "images", not(feature = "bidi")))]
        let placements = self.placements();
        let frame = Frame::new(
            lines,
            &self.damage,
            palette,
            since,
            cursor,
            self.flags.reverse,
        )
        .with_selection(selected)
        .with_hover(
            self.hover.link,
            self.hover
                .overlay
                .zip(self.hover.link.and_then(|l| self.link_uri(l))),
        );
        #[cfg(feature = "images")]
        let frame = frame.with_placements(placements);
        #[cfg(feature = "bidi")]
        let frame = frame.with_order(order);
        frame
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

    /// The line the view shows at row `y`.
    fn view_line(&self, y: usize) -> Option<&Line> {
        let screen = self.lines().iter();
        self.scrollback
            .in_view()
            .chain(screen)
            .take(self.rows)
            .nth(y)
    }

    /// Sets how many lines the scrollback keeps, at most the cap the screen
    /// was made with; the oldest lines past it go.
    pub fn set_scrollback_max(&mut self, lines: usize) {
        self.scrollback.set_max(lines);
        self.images_dropped();
        self.damage.mark_all();
    }

    /// Sets the most bytes the scrollback holds, `sb-memory`, which
    /// [`sb_memory`](crate::sb_memory) fences; the oldest lines past it go.
    /// A line that alone passes it does not enter. The budget stays fixed
    /// through every resize, as a configured `sb-memory` is.
    pub fn set_scrollback_memory(&mut self, bytes: usize) {
        self.sb_memory_cap = None;
        self.apply_scrollback_memory(bytes);
    }

    /// Makes `sb-memory` follow the grid, as it does when the configuration
    /// does not set it, and as a new screen starts: the budget is what the
    /// minspec peak leaves after the rest of the process, the rows the two
    /// screens hold at their worst and a full link store, never less than
    /// [`SB_MEMORY_DEFAULT`](crate::SB_MEMORY_DEFAULT), the value at the
    /// minspec grid ([`sb_memory_default`](crate::sb_memory_default)), and
    /// at most `cap`. Each resize works it out again, and the oldest lines
    /// past a lower budget go.
    pub fn set_scrollback_memory_from_grid(&mut self, cap: usize) {
        self.sb_memory_cap = Some(cap);
        self.follow_grid();
    }

    /// The scrollback's budget for the grid, while it follows the grid.
    /// The rows priced are those the screens hold: each screen holds at
    /// least its rows, and keeps lines below them after it shrank.
    fn follow_grid(&mut self) {
        let Some(cap) = self.sb_memory_cap else {
            return;
        };
        let lines = self
            .main
            .len()
            .max(self.rows)
            .saturating_add(self.alt.len().max(self.rows));
        let screens = lines_bytes_max(self.cols, self.rows, lines);
        self.apply_scrollback_memory(sb_memory_held(screens).max(SB_MEMORY_MIN).min(cap));
    }

    fn apply_scrollback_memory(&mut self, bytes: usize) {
        self.scrollback.set_bytes_max(bytes);
        self.images_dropped();
        self.damage.mark_all();
    }

    /// The most bytes the scrollback holds, `sb-memory`: as set, or as the
    /// grid gives it while it follows the grid.
    pub fn scrollback_memory(&self) -> usize {
        self.scrollback.bytes_max()
    }

    /// The bytes the scrollback's lines hold: each line's struct and its
    /// heap, by capacity, counted against `sb-memory`.
    pub fn scrollback_bytes(&self) -> usize {
        self.scrollback.bytes()
    }

    /// The rows the scrollback has read to find its oldest logical line
    /// waiting to be rewrapped, as each drop past `sb-memory` does. Tests
    /// read it to check that a drop reads each row a bounded number of
    /// times.
    pub fn scrollback_rows_read(&self) -> usize {
        self.scrollback.rows_read()
    }

    /// Empties the scrollback (ED 3).
    pub fn clear_scrollback(&mut self) {
        self.images_clear_scrollback();
        self.scrollback.clear();
        self.damage.mark_all();
    }

    /// Scrolls the view `n` lines back into the scrollback.
    pub fn scroll_view_up(&mut self, n: usize) {
        self.scrollback.view_up(n);
        self.images_dropped();
        self.damage.mark_all();
        self.refresh_hover();
    }

    /// Scrolls the view `n` lines forward, towards the screen.
    pub fn scroll_view_down(&mut self, n: usize) {
        self.scrollback.view_down(n);
        self.damage.mark_all();
        self.refresh_hover();
    }

    /// Returns the view to the screen.
    pub fn reset_view(&mut self) {
        if self.scrollback.offset() != 0 {
            self.scrollback.reset_view();
            self.damage.mark_all();
            self.refresh_hover();
        }
    }

    /// Resizes the screen to `cols` by `rows`, as
    /// [`resize_carrying`](Self::resize_carrying) does with the saved
    /// cursor home, whose new place goes.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.resize_carrying(cols, rows, &mut (0, 0));
    }

    /// Resizes the screen to `cols` by `rows`, each at least 1 and at most
    /// [`COLUMNS_MAX`] by [`ROWS_MAX`]. The margins go, DECLRMM staying as
    /// it is, and the tab stops return to every eighth column.
    ///
    /// A change in the column count rewraps the main screen and its
    /// scrollback, keeping the cursor at its place in the text, and cuts
    /// the alternate screen's lines at the new width (ADR-1003, `reflow`).
    /// `saved` is the main screen's saved cursor, a column and a row, the
    /// column the width while a wrap is pending: it is carried as the
    /// cursor is, and while the alternate screen is shown the main screen
    /// rewraps around it.
    ///
    /// Then a shorter main screen pushes its top lines into the scrollback,
    /// keeping the cursor's line on the screen; with nothing in the
    /// scrollback, or on the alternate screen, it scrolls only as far as
    /// the cursor needs. A taller main screen pulls lines back from the
    /// scrollback. A host resizes through
    /// [`Terminal::resize`](crate::Terminal::resize), which carries the
    /// saved cursor and reports the resize to a program that set mode
    /// 2048.
    pub fn resize_carrying(&mut self, cols: usize, rows: usize, saved: &mut (usize, usize)) {
        let cols = cols.clamp(1, COLUMNS_MAX);
        let rows = rows.clamp(1, ROWS_MAX);
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        self.segment = None;
        if cols != self.cols {
            self.rewrap(cols, saved);
        }
        self.main
            .reserve_exact(rows.saturating_sub(self.main.len()));
        self.alt.reserve_exact(rows.saturating_sub(self.alt.len()));
        while self.main.len() < rows {
            let line = self.new_line(cols, &Style::DEFAULT);
            self.main.push(line);
        }
        while self.alt.len() < rows {
            let line = self.new_line(cols, &Style::DEFAULT);
            self.alt.push(line);
        }
        self.damage.resize(cols, rows);
        self.top = 0;
        self.bottom = self.rows.saturating_sub(1);
        self.left = 0;
        self.right = cols.saturating_sub(1);
        self.margin_wrap = false;
        self.tabs = default_tabs(cols);
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
            let diff = rows.saturating_sub(self.rows).min(self.scrollback.total());
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
        self.follow_grid();
        self.refresh_hover();
    }

    /// Replaces the top `n` lines with the newest `n` of the scrollback,
    /// the newest at the bottom of them, painted to the screen's width.
    fn pull_from_scrollback(&mut self, n: usize) {
        let cols = self.cols;
        #[cfg(feature = "images")]
        let mut gone = Vec::new();
        for row in (0..n).rev() {
            let Some(mut line) = self.scrollback.pop_newest() else {
                self.images_dropped();
                #[cfg(feature = "images")]
                self.discard_lines(gone);
                return;
            };
            self.images_dropped();
            line.paint(cols);
            // The line replaced is one `rotate_down` just made, which holds
            // no selection and no placement; with images it goes as every
            // replaced line does, so the store hears of it.
            if let Some(slot) = self.main.get_mut(row) {
                #[cfg(feature = "images")]
                gone.push(core::mem::replace(slot, line));
                #[cfg(not(feature = "images"))]
                {
                    *slot = line;
                }
            }
        }
        #[cfg(feature = "images")]
        self.discard_lines(gone);
    }

    // Cursor movement.

    /// Moves the cursor, which ends the cluster the last print wrote and a
    /// pending wrap at the right margin.
    fn move_cursor(&mut self, x: usize, y: usize) {
        self.segment = None;
        self.margin_wrap = false;
        self.cursor_x = x;
        self.cursor_y = y;
    }

    /// The first and last columns the cursor may be addressed to: the
    /// screen's, or the left and right margins' in origin mode.
    pub(super) fn origin_cols(&self) -> (usize, usize) {
        if self.flags.origin {
            (self.left, self.right)
        } else {
            (0, self.cols.saturating_sub(1))
        }
    }

    /// CUP: to column `x`, row `y`, counted from the region's top left
    /// corner in origin mode, and kept inside the screen, or the region:
    /// the rows between the top and bottom margins and the columns between
    /// the left and right margins (xterm's `CursorSet`).
    pub fn move_to(&mut self, x: usize, y: usize) {
        let (y, last) = if self.flags.origin {
            (self.top.saturating_add(y), self.bottom)
        } else {
            (y, self.rows.saturating_sub(1))
        };
        let (first_col, last_col) = self.origin_cols();
        let x = first_col.saturating_add(x).min(last_col);
        self.move_cursor(x, y.min(last));
    }

    /// The cursor's column as CUP counts it: from the left margin in
    /// origin mode, a pending wrap counting as the last column (xterm's
    /// `CursorCol`).
    pub fn origin_col(&self) -> usize {
        let x = self.cursor_x.min(self.cols.saturating_sub(1));
        x.saturating_sub(self.origin_cols().0)
    }

    /// VPA: to row `y`, counted as CUP counts it, keeping the column as
    /// CUP counts it.
    pub fn move_to_row(&mut self, y: usize) {
        self.move_to(self.origin_col(), y);
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

    /// DECRC of a pending wrap at the right margin: kept while autowrap is
    /// on and the cursor stands on a right margin before the screen's last
    /// column, as DECSC saves the wrap (VT510 manual, DECSC).
    pub fn restore_margin_wrap(&mut self) {
        self.margin_wrap = self.flags.auto_wrap
            && self.cursor_x == self.right
            && self.right.saturating_add(1) < self.cols;
    }

    /// CHA and HPA: to column `x` of the cursor's row, counted from the
    /// left margin and kept inside the margins in origin mode.
    pub fn move_to_column(&mut self, x: usize) {
        let (first, last) = self.origin_cols();
        self.move_cursor(first.saturating_add(x).min(last), self.cursor_y);
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
    /// stops at the screen's first row and nothing scrolls, and outside
    /// the left and right margins nothing scrolls either (xterm's
    /// `RevIndex`).
    pub fn move_up(&mut self, n: usize, scroll: bool) {
        if n == 0 {
            return;
        }
        let scroll = scroll && self.in_lr_margins(self.cursor_x);
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
    /// cursor stops at the screen's last row and nothing scrolls, and
    /// outside the left and right margins nothing scrolls either (xterm's
    /// `xtermIndex`).
    pub fn move_down(&mut self, n: usize, scroll: bool) {
        if n == 0 {
            return;
        }
        let scroll = scroll && self.in_lr_margins(self.cursor_x);
        let end = if self.cursor_y <= self.bottom {
            self.bottom.saturating_add(1)
        } else {
            self.rows
        };
        let room = end.saturating_sub(self.cursor_y).saturating_sub(1);
        // A line feed (LF, IND, NEL) that moves the cursor or scrolls takes
        // it into the next row's paragraph (ADR-1004 D2).
        #[cfg(feature = "bidi")]
        let moves = scroll && n == 1 && (room > 0 || self.cursor_y <= self.bottom);
        if n > room {
            if scroll && self.cursor_y <= self.bottom {
                self.scroll_up(n.saturating_sub(room));
            }
            self.move_cursor(self.cursor_x, end.saturating_sub(1));
        } else {
            self.move_cursor(self.cursor_x, self.cursor_y.saturating_add(n));
        }
        #[cfg(feature = "bidi")]
        if moves {
            self.line_fed(self.cursor_y);
        }
    }

    /// The column CUB, BS and CR stop at from column `x`: the left
    /// margin, or the screen's first column when `x` is left of the margin
    /// (xterm's `CursorBack` and `CarriageReturn`).
    fn left_edge(&self, x: usize) -> usize {
        if x >= self.left { self.left } else { 0 }
    }

    /// CUB and BS: `n` columns left, stopping at the left margin, or at the
    /// first column when the cursor starts left of the margin.
    pub fn move_left(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let x = self.cursor_x.min(self.cols.saturating_sub(1));
        let stop = self.left_edge(x);
        self.move_cursor(x.saturating_sub(n).max(stop), self.cursor_y);
    }

    /// CUB and BS with reverse wraparound (mode 45, with autowrap on), as
    /// xterm's XTREVWRAP moves: `n` columns left, where a pending wrap
    /// takes the first step without moving, and from the left margin the
    /// cursor goes on from the right margin of the row above when printing
    /// wrapped from that row into this one. It stops at the left margin of
    /// a row the row above did not wrap into, and of the screen's first
    /// row. A cursor left of the left margin takes the first column for
    /// it, as xterm's `CursorBack` does. Printing marks a row wrapped only
    /// where it wraps from the screen's last column to the next row's
    /// first (`print.rs`), so inside narrower margins the cursor stops at
    /// the left margin.
    pub fn move_left_reverse_wrap(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let last = self.cols.saturating_sub(1);
        let (mut x, mut y) = (self.cursor_x, self.cursor_y);
        let mut left = n;
        if x > last || self.margin_wrap {
            x = x.min(last);
            left = left.saturating_sub(1);
        }
        let stop = self.left_edge(x);
        while left > 0 {
            if x > stop {
                let step = left.min(x.saturating_sub(stop));
                x = x.saturating_sub(step);
                left = left.saturating_sub(step);
                continue;
            }
            let wrapped = y
                .checked_sub(1)
                .and_then(|above| self.line(above))
                .is_some_and(|line| line.flags().contains(LineFlags::WRAPPED));
            if !wrapped {
                break;
            }
            y = y.saturating_sub(1);
            x = self.right;
            left = left.saturating_sub(1);
        }
        self.move_cursor(x, y);
    }

    /// CUF and HPR: `n` columns right, stopping at the right margin, or at
    /// the last column when the cursor starts right of the margin.
    pub fn move_right(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let stop = self.right_edge(self.cursor_x);
        let x = self.cursor_x.saturating_add(n.min(self.cols)).min(stop);
        self.move_cursor(x, self.cursor_y);
    }

    /// CR: to the left margin, or to the first column when the cursor is
    /// left of the margin outside origin mode (VT510 manual, CR; xterm's
    /// `CarriageReturn`).
    pub fn move_line_home(&mut self) {
        let x = if self.flags.origin {
            self.left
        } else {
            self.left_edge(self.cursor_x)
        };
        self.move_cursor(x, self.cursor_y);
    }

    /// NEL, and LF in newline mode: the next row, scrolling at the bottom
    /// margin, then CR.
    pub fn newline(&mut self) {
        self.move_down(1, true);
        self.move_line_home();
    }

    /// HT and CHT: to the `n`th tab stop right, no further than the right
    /// margin, or to the right margin when no stop is left before it
    /// (VT510 manual, HT). From right of the right margin, the cursor goes
    /// back to the margin, as xterm's `TabToNextStop` moves it. A tab never
    /// leaves a pending wrap.
    pub fn tab_right(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let stop = self.right;
        let end = stop.saturating_add(1);
        let mut x = self.cursor_x;
        for _ in 0..n {
            let from = x.saturating_add(1);
            x = (from..end)
                .find(|&c| self.tabs.get(c).copied().unwrap_or(false))
                .unwrap_or(end);
            if x.saturating_add(1) >= end {
                break;
            }
        }
        self.move_cursor(x.min(stop), self.cursor_y);
    }

    /// CBT: to the `n`th tab stop left, or the first column, or in origin
    /// mode no further left than the left margin (xterm's
    /// `TabToPrevStop`).
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
        if self.flags.origin {
            x = x.max(self.left);
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
    /// line of this screen has had, holding the current paragraph
    /// properties.
    fn new_line(&mut self, cols: usize, style: &Style) -> Line {
        let mut line = Line::new(LineId::new(self.next_id), cols);
        self.next_id = self.next_id.wrapping_add(1);
        #[cfg(feature = "bidi")]
        line.flags_mut().insert(self.paragraph);
        if *style != Style::DEFAULT {
            line.erase(0..cols, style);
        }
        line
    }

    /// The logical column view row `y` shows at column `x`, where the
    /// pointer is: the cell a selection, a mouse report or a link's hover
    /// takes. In a build with the bidi feature a row drawn in another
    /// order than it was written shows another column there (ADR-1004
    /// D10); otherwise every column shows itself.
    pub(crate) fn pointer_col(&self, x: usize, y: usize) -> usize {
        #[cfg(feature = "bidi")]
        if let Some(r) = self.row_order(y) {
            return r.logical_at(x);
        }
        let _ = y;
        x
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
