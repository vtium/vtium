//! Frames: what a renderer reads to draw the screen.
//!
//! A [`Frame`] views the visible lines with their colors resolved through
//! the palette at the time it is drawn, each cell marked changed or not
//! for the display reading it, the cursor, and the selection. Resolving at
//! draw time is what lets a palette change recolor text already on the
//! screen.
//!
//! A cell's colors resolve in this order: bold brightens a
//! foreground palette color below 8 to its bright twin, the palette gives
//! RGB, inverse video and DECSCNM swap the two, the selection swaps them
//! again or takes the selection colors OSC 17 and 19 set, and dim draws
//! the foreground halfway to the background.
//!
//! In a build with images, a kitty placeholder cell's foreground and
//! underline colors name an image and a placement (ADR-1100, D13), so the
//! frame resolves its colors with the default foreground and no underline
//! color. Reverse video then shows what a blank cell with the same
//! attributes shows, never the color that encodes the image id. The cell
//! keeps its glyph, by which the renderer knows to draw no text, underline
//! or strike-through there.

use core::ops::Range;

use crate::cell::{Cell, StyleIndex, Width};
use crate::damage::{Age, Damage};
#[cfg(all(feature = "bidi", feature = "rows-beyond"))]
use crate::line::LineFlags;
use crate::line::{Glyph, Line, LineSize, LinkId};
#[cfg(feature = "bidi")]
use crate::order::Order;
#[cfg(all(feature = "bidi", feature = "rows-beyond"))]
use crate::order::RowOrder;
use crate::palette::{Palette, Special};
use crate::style::{Attrs, Color, Rgb, Style};

/// SGR 2's foreground: halfway from `fg` to `bg`, each channel the sum of
/// the two halved. The renderer dims blinking text with it too.
pub fn dim(fg: Rgb, bg: Rgb) -> Rgb {
    let ch = |f: u8, b: u8| (f / 2).saturating_add(b / 2);
    Rgb::new(ch(fg.r, bg.r), ch(fg.g, bg.g), ch(fg.b, bg.b))
}

/// DECSCUSR's cursor shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

/// The cursor as a frame shows it. A renderer compares it with the cursor
/// it last drew: moving the cursor marks no cells changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    /// DECTCEM.
    pub visible: bool,
    pub shape: CursorShape,
    pub blink: bool,
    /// OSC 12's color; `None` draws the cell under it inverted.
    pub color: Option<Rgb>,
    /// The character under the cursor resolves right to left, so a bar
    /// cursor stands at its cell's right side (ADR-1004 D9).
    #[cfg(feature = "bidi")]
    pub rtl: bool,
}

/// Where the cursor draws on the display ([`Frame::cursor_extent`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorExtent {
    /// How many display cells the cursor covers across: the width of the
    /// character it stands on, doubled on a line of double size.
    pub cells: usize,
    /// The frame rows under the cursor: its own row, or both rows of a
    /// pair of double-height lines.
    pub rows: Range<usize>,
}

/// The row that shows the hovered link's URI while Ctrl is held
/// (ADR-1002, "Drawing"). It is no part of the screen: no cell, selection,
/// copy or reply sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overlay<'a> {
    /// The view row it is drawn over: the bottom one, or the top one while the
    /// pointer is on the bottom one.
    pub row: usize,
    /// The URI, printable ASCII.
    pub uri: &'a str,
}

/// A link's label while link hints are on: letters drawn over the first
/// cells of the link where it first appears in the view, so a person can
/// type them to copy the link's address. A label is no part of the screen:
/// no cell, selection, copy or reply sees it.
///
/// References:
/// - ADR-1002, section 7: which links get labels, where a label draws, and
///   that labels are no part of the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkHint {
    pub link: LinkId,
    /// The label's letters.
    pub label: String,
    /// The label begins with the letters typed so far, so it draws. Every
    /// cell of a labelled link takes the hover's dashed underline either
    /// way.
    pub shown: bool,
    /// Where the label starts in the view: a column and a row. The screen
    /// sets it where the link first appears. A label that would cover part
    /// of the label before it shifts right to clear it, and the labels at
    /// a row's end move left so that the last ends at the row's last
    /// column.
    pub at: (usize, usize),
}

/// One cell of a frame, resolved for drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameCell<'a> {
    pub glyph: Glyph<'a>,
    pub width: Width,
    /// The foreground after the palette, bold as bright, inverse video and
    /// dim.
    pub fg: Rgb,
    /// The background after the palette and inverse video.
    pub bg: Rgb,
    /// The underline color SGR 58 set, resolved and exact; `None` follows
    /// the configured underline color, or the foreground.
    pub underline_color: Option<Rgb>,
    /// SGR's attributes, which a renderer draws: the bold face, blink,
    /// conceal, the underline style, overline, strike, the script. Dim and
    /// bold as bright are already in the colors.
    pub attrs: Attrs,
    /// The background is the default one and not inverted: an image placed
    /// below cell backgrounds shows through, as the kitty graphics protocol
    /// draws one at a z-index below -1,073,741,824.
    pub default_bg: bool,
    pub link: Option<LinkId>,
    /// Part of the link under the pointer, which draws a dashed underline
    /// in place of the cell's own underline style.
    pub hovered: bool,
    /// Inside the selection, which draws inverted or in the selection
    /// colors.
    pub selected: bool,
    /// Changed since the age the reading display passed.
    pub changed: bool,
    /// The logical column the cell holds, the column the program wrote
    /// it at, which a reordered row draws elsewhere (ADR-1004 D8).
    #[cfg(feature = "bidi")]
    pub col: usize,
    /// The cell's character resolves right to left (ADR-1004 D8).
    #[cfg(feature = "bidi")]
    pub rtl: bool,
    /// Which sides of the cell join its neighbours, for a letter of a
    /// joining script such as Arabic (ADR-1004 D6).
    #[cfg(feature = "bidi")]
    pub joins: crate::order::Joins,
    /// The character the cell draws in place of its first character, its
    /// mirrored counterpart, with any marks of its cluster drawn on it;
    /// `None` when it draws as written (ADR-1004 D7). A single character
    /// drawn mirrored is the counterpart in `glyph` too.
    #[cfg(feature = "bidi")]
    pub mirror: Option<char>,
}

/// The visible screen, as one display reads it.
#[derive(Clone, Debug)]
pub struct Frame<'a> {
    lines: Vec<&'a Line>,
    damage: &'a Damage,
    palette: &'a Palette,
    since: Age,
    cursor: Cursor,
    reverse: bool,
    /// The selected columns of each row; a row past the list has none.
    selection: Vec<Range<usize>>,
    hovered: Option<LinkId>,
    overlay: Option<Overlay<'a>>,
    /// The labels of link hints, while they are on, in the order they lie
    /// in the view.
    link_hints: &'a [LinkHint],
    /// The links that have a label, sorted.
    labelled: &'a [LinkId],
    /// The image placements with a row in view, in draw order (ADR-1100,
    /// D9).
    #[cfg(feature = "images")]
    placements: Vec<vtium_image::FramePlacement>,
    /// The order each row is drawn in (ADR-1004 D8).
    #[cfg(feature = "bidi")]
    order: Order,
    /// For each row with an order, whether any row of its paragraph
    /// changed since `since`, which marks every cell of the row changed.
    #[cfg(feature = "bidi")]
    paragraph_changed: Vec<bool>,
    /// For each row with an order, whether any row of its paragraph
    /// changed in place since `since`, as the scroll record reads it.
    #[cfg(feature = "bidi")]
    paragraph_in_place: Vec<bool>,
    /// The rows captured beyond the screen, with what each carries.
    #[cfg(feature = "rows-beyond")]
    beyond: Beyond<'a>,
}

/// The rows a frame holds beyond the screen: rows of the scrollback above
/// it, and rows below it while the view is scrolled back. Each row
/// carries what a row on the screen carries, its cells and styles, its
/// selection, its line size and the pictures on it, but never the cursor.
///
/// References:
/// - ADR-1310 D2, "What a row beyond the screen carries": the list, and
///   that the cursor is reported only on the screen's rows.
#[cfg(feature = "rows-beyond")]
#[derive(Clone, Debug, Default)]
struct Beyond<'a> {
    /// The rows above the screen, top to bottom, so the last is row -1.
    above: Vec<&'a Line>,
    /// The rows below the screen, top to bottom, so the first is the row
    /// at the screen's row count.
    below: Vec<&'a Line>,
    /// The selected logical columns of each row above, top to bottom; a
    /// row past the list has none.
    selection_above: Vec<Range<usize>>,
    /// The selected logical columns of each row below, top to bottom.
    selection_below: Vec<Range<usize>>,
    /// The image placements with a row in the capture.
    #[cfg(feature = "images")]
    placements: Vec<vtium_image::FramePlacement>,
    /// The order each row above is drawn in, top to bottom.
    #[cfg(feature = "bidi")]
    order_above: Order,
    /// The order each row below is drawn in, top to bottom.
    #[cfg(feature = "bidi")]
    order_below: Order,
    /// How many rows above, counted up from row -1, continue the paragraph
    /// of the screen's top row, and how many rows below, counted down from
    /// the first, continue the paragraph of its bottom row.
    #[cfg(feature = "bidi")]
    joined: (usize, usize),
}

/// Where a row of a frame's capture lies: on the screen, or the index of
/// a row in the list above or below it.
#[cfg(feature = "rows-beyond")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Above(usize),
    Screen(usize),
    Below(usize),
}

impl<'a> Frame<'a> {
    /// A frame of `lines`, top to bottom, for a display that last drew at
    /// `since`. `reverse` is DECSCNM.
    pub fn new(
        lines: Vec<&'a Line>,
        damage: &'a Damage,
        palette: &'a Palette,
        since: Age,
        cursor: Cursor,
        reverse: bool,
    ) -> Self {
        Self {
            lines,
            damage,
            palette,
            since,
            cursor,
            reverse,
            selection: Vec::new(),
            hovered: None,
            overlay: None,
            link_hints: &[],
            labelled: &[],
            #[cfg(feature = "images")]
            placements: Vec::new(),
            #[cfg(feature = "bidi")]
            order: Order::default(),
            #[cfg(feature = "bidi")]
            paragraph_changed: Vec::new(),
            #[cfg(feature = "bidi")]
            paragraph_in_place: Vec::new(),
            #[cfg(feature = "rows-beyond")]
            beyond: Beyond::default(),
        }
    }

    /// The frame with each row drawn in the order `order` gives, as the
    /// screen built it. A row of a paragraph that draws in another order
    /// than it was written reads as changed in every cell whenever any row
    /// of the paragraph changed since `since` (ADR-1004 D8).
    ///
    /// The scroll record reads such a row the same way: it changed in
    /// place when any row of its paragraph changed in place.
    #[cfg(feature = "bidi")]
    pub fn with_order(mut self, order: Order) -> Self {
        let rows = self.lines.len();
        let mut changed = vec![false; rows];
        let mut in_place = vec![false; rows];
        let mut last: Option<(Range<usize>, bool, bool)> = None;
        for (row, r) in order.rows() {
            let (any, here) = match &last {
                Some((p, any, here)) if *p == r.paragraph => (*any, *here),
                _ => {
                    let p = r.paragraph.clone();
                    let any = p.clone().any(|y| self.damage.row_changed(y, self.since));
                    let here = p
                        .clone()
                        .any(|y| self.damage.row_changed_in_place(y, self.since));
                    last = Some((p, any, here));
                    (any, here)
                }
            };
            if let Some(slot) = changed.get_mut(row) {
                *slot = any;
            }
            if let Some(slot) = in_place.get_mut(row) {
                *slot = here;
            }
        }
        self.order = order;
        self.paragraph_changed = changed;
        self.paragraph_in_place = in_place;
        self
    }

    /// The order each row is drawn in, which a holder of the frame's parts
    /// hands back with [`with_order`](Self::with_order).
    #[cfg(feature = "bidi")]
    pub fn order(&self) -> &Order {
        &self.order
    }

    /// The logical column drawn at column `col` of `row`.
    fn logical(&self, row: usize, col: usize) -> usize {
        #[cfg(feature = "bidi")]
        if let Some(r) = self.order.row(row) {
            return r.logical_at(col);
        }
        let _ = row;
        col
    }

    /// The frame with the image placements the view shows, in draw order.
    #[cfg(feature = "images")]
    pub fn with_placements(mut self, placements: Vec<vtium_image::FramePlacement>) -> Self {
        self.placements = placements;
        self
    }

    /// The image placements with a row in view, in the order the frame
    /// draws them, each with its image's pixels shared (ADR-1100, D9).
    #[cfg(feature = "images")]
    pub fn placements(&self) -> &[vtium_image::FramePlacement] {
        &self.placements
    }

    /// The frame with the columns of each row, top to bottom, that the
    /// selection covers, as logical columns: in a row drawn in another
    /// order, the cells they hold can show in pieces (ADR-1004 D10).
    pub fn with_selection(mut self, selection: Vec<Range<usize>>) -> Self {
        self.selection = selection;
        self
    }

    /// The logical columns of each row, top to bottom, that the selection
    /// covers.
    pub fn selection(&self) -> &[Range<usize>] {
        &self.selection
    }

    /// The frame with the link under the pointer, and the overlay row
    /// showing a URI, a row and the URI.
    pub fn with_hover(
        mut self,
        hovered: Option<LinkId>,
        overlay: Option<(usize, &'a str)>,
    ) -> Self {
        self.hovered = hovered;
        self.overlay = overlay.map(|(row, uri)| Overlay { row, uri });
        self
    }

    /// The link under the pointer, whose visible cells are each
    /// [`hovered`](FrameCell::hovered).
    pub fn hovered(&self) -> Option<LinkId> {
        self.hovered
    }

    /// The overlay row showing the hovered link's URI, while Ctrl is held.
    pub fn overlay(&self) -> Option<Overlay<'a>> {
        self.overlay
    }

    /// The frame with the labels of link hints, `hints`, and the links
    /// that have a label, `labelled`. The labels come in the order they lie
    /// in the view: row by row from the top, and in each row from left to
    /// right, a later label drawing over an earlier one where they meet.
    /// `labelled` is sorted. The renderer looks a cell's label and its
    /// link up in these lists by binary search, so each cell costs little
    /// on a screen full of labels.
    pub fn with_link_hints(mut self, hints: &'a [LinkHint], labelled: &'a [LinkId]) -> Self {
        self.link_hints = hints;
        self.labelled = labelled;
        self
    }

    /// The labels of link hints, while they are on, in the order they lie
    /// in the view.
    pub fn link_hints(&self) -> &'a [LinkHint] {
        self.link_hints
    }

    /// The links that have a label, sorted.
    pub fn labelled_links(&self) -> &'a [LinkId] {
        self.labelled
    }

    pub fn rows(&self) -> usize {
        self.lines.len()
    }

    pub fn cols(&self) -> usize {
        self.damage.cols()
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// Whether reverse video (DECSCNM) is set, which swaps the default
    /// colors of every cell the frame resolves.
    pub fn reverse(&self) -> bool {
        self.reverse
    }

    /// The palette the frame resolves through, for the colors a renderer
    /// resolves itself at draw time.
    pub fn palette(&self) -> &'a Palette {
        self.palette
    }

    /// The line drawn at `row`, for its flags and identity.
    pub fn line(&self, row: usize) -> Option<&'a Line> {
        self.lines.get(row).copied()
    }

    /// The size `row` draws at: its line's size, or the normal size on the
    /// overlay row, which draws the hovered link's URI over the row at the
    /// normal size. A row outside the frame draws at the normal size.
    pub fn size(&self, row: usize) -> LineSize {
        if self.overlay.is_some_and(|o| o.row == row) {
            return LineSize::Single;
        }
        self.line(row)
            .map_or(LineSize::Single, |l| l.flags().size())
    }

    /// The other row of the pair of double-height lines that `row` belongs
    /// to, by `pair_of`'s rule over the sizes the rows draw at. A row
    /// whose partner the frame does not show, and the overlay row, belong
    /// to no pair. The pair decides where the cursor draws
    /// ([`Frame::cursor_extent`]).
    ///
    /// References:
    /// - ADR-1308 D1, "The cursor on a scaled glyph scales with the glyph":
    ///   a double-height pair is a top line directly followed by its bottom
    ///   line.
    pub fn pair_row(&self, row: usize) -> Option<usize> {
        pair_of(row, |r| self.size(r))
    }

    /// Where the cursor draws when it stands on a character of `row` that
    /// takes `cells` of the line's columns: 1, or 2 for a wide character.
    /// The cursor scales with the glyph. On a line of double size it covers
    /// twice as many display cells, so a bar or an underline is twice as
    /// thick. On a pair of double-height lines it covers both rows of the
    /// pair, whichever of them it stands on: a block covers both halves of
    /// the character, a bar runs down both rows, and an underline lies
    /// under the bottom row. On a top or bottom line with no partner it
    /// covers its own row. The terminal's cursor stays on its own row, and
    /// only the drawing spans the pair. A ligature or a joined syllable
    /// does not widen the cursor, so `cells` is the width of the cell the
    /// cursor stands on, never the width of the run it belongs to.
    ///
    /// References:
    /// - ADR-1308 D1, "The cursor on a scaled glyph scales with the glyph":
    ///   the extent on double-width lines and on double-height pairs.
    pub fn cursor_extent(&self, row: usize, cells: usize) -> CursorExtent {
        let scale = if self.size(row).is_double() { 2 } else { 1 };
        let rows = match self.pair_row(row) {
            Some(other) => row.min(other)..row.max(other).saturating_add(1),
            None => row..row.saturating_add(1),
        };
        CursorExtent {
            cells: cells.saturating_mul(scale),
            rows,
        }
    }

    /// Whether any cell of `row` changed for this display.
    pub fn row_changed(&self, row: usize) -> bool {
        #[cfg(feature = "bidi")]
        if self.order.row(row).is_some() {
            return self.paragraph_changed.get(row).copied().unwrap_or(true);
        }
        self.damage.row_changed(row, self.since)
    }

    /// The rows the whole view moved since this display's age, from the
    /// scroll record: up counting positive, down negative. A display that
    /// kept what it drew can move its picture by this many rows, then
    /// redraw only the rows and cells that
    /// [`row_changed_in_place`](Self::row_changed_in_place) and
    /// [`changed_in_place`](Self::changed_in_place) mark. `None` when the
    /// record cannot say, as [`Damage::scrolled`] lists; the display then
    /// redraws what [`row_changed`](Self::row_changed) and
    /// [`FrameCell::changed`] mark, which covers every row that moved.
    ///
    /// References:
    /// - ADR-1310 D2, "A scroll record in every build": the count each
    ///   display reads since its own buffer's age.
    pub fn scrolled(&self) -> Option<isize> {
        self.damage.scrolled(self.since)
    }

    /// Whether any cell of `row` still differs for this display once it
    /// moved its picture by [`scrolled`](Self::scrolled) rows. While the
    /// record cannot say, it is [`row_changed`](Self::row_changed).
    pub fn row_changed_in_place(&self, row: usize) -> bool {
        if self.scrolled().is_none() {
            return self.row_changed(row);
        }
        #[cfg(feature = "bidi")]
        if self.order.row(row).is_some() {
            return self.paragraph_in_place.get(row).copied().unwrap_or(true);
        }
        self.damage.row_changed_in_place(row, self.since)
    }

    /// Whether the cell drawn at `row`, column `col` from the left still
    /// differs for this display once it moved its picture by
    /// [`scrolled`](Self::scrolled) rows. While the record cannot say, it
    /// is the cell's [`FrameCell::changed`].
    pub fn changed_in_place(&self, row: usize, col: usize) -> bool {
        #[cfg(feature = "bidi")]
        if self.order.row(row).is_some() {
            let changed = if self.scrolled().is_none() {
                &self.paragraph_changed
            } else {
                &self.paragraph_in_place
            };
            return changed.get(row).copied().unwrap_or(true);
        }
        let col = self.logical(row, col);
        if self.scrolled().is_none() {
            return self.damage.changed(row, col, self.since);
        }
        self.damage.changed_in_place(row, col, self.since)
    }

    /// The share of each sized run that crosses `row`, ordered by column:
    /// the block of cells that one OSC 66 sequence writes, with the row of
    /// the run this line holds, the run's columns, its scale, its width,
    /// its fraction and its alignment. Each cell of a run is marked
    /// ([`crate::Cell::in_run`]), and the run's first cell on each row
    /// holds its text.
    ///
    /// References:
    /// - ADR-1308 D9: the renderer draws a run's text whole, at its scale,
    ///   inside its block.
    pub fn sized_runs(&self, row: usize) -> &'a [crate::line::SizedRun] {
        self.line(row).map_or(&[], |l| l.runs())
    }

    /// The cell drawn at `row`, column `col` from the left, resolved;
    /// `None` outside the frame. A row drawn in another order than it was
    /// written shows there the cell of another logical column
    /// (`FrameCell::col` in a build with the bidi feature).
    pub fn cell(&self, row: usize, col: usize) -> Option<FrameCell<'a>> {
        let line = self.line(row)?;
        if col >= self.cols() {
            return None;
        }
        let col = self.logical(row, col);
        let cell = line.cell(col);
        let selected = self.selection.get(row).is_some_and(|r| r.contains(&col));
        let placeholder = is_placeholder(line, cell);
        let colors = self.resolve(drawn(line.style(cell), placeholder), selected);
        Some(self.frame_cell(line, cell, row, col, colors))
    }

    /// The cells of `row` as drawn, left to right, each as [`Frame::cell`]
    /// reads it; none outside the frame. A run of cells in one style, all
    /// selected or none, resolves its colors once.
    pub fn cells(&self, row: usize) -> impl Iterator<Item = FrameCell<'a>> + '_ {
        let line = self.line(row);
        let cols = line.map_or(0, |_| self.cols());
        let selection = self.selection.get(row);
        let mut last: Option<(StyleIndex, bool, bool, Colors)> = None;
        line.into_iter()
            .flat_map(move |line| (0..cols).map(move |col| (line, col)))
            .map(move |(line, col)| {
                let col = self.logical(row, col);
                let cell = line.cell(col);
                let selected = selection.is_some_and(|r| r.contains(&col));
                let placeholder = is_placeholder(line, cell);
                let colors = match last {
                    Some((style, sel, ph, colors))
                        if style == cell.style() && sel == selected && ph == placeholder =>
                    {
                        colors
                    }
                    _ => {
                        let colors = self.resolve(drawn(line.style(cell), placeholder), selected);
                        last = Some((cell.style(), selected, placeholder, colors));
                        colors
                    }
                };
                self.frame_cell(line, cell, row, col, colors)
            })
    }

    fn frame_cell(
        &self,
        line: &'a Line,
        cell: Cell,
        row: usize,
        col: usize,
        c: Colors,
    ) -> FrameCell<'a> {
        let link = line.link(cell);
        #[cfg(feature = "bidi")]
        let order = self.order.row(row);
        #[cfg(feature = "bidi")]
        let (glyph, changed) = match order {
            Some(r) => (
                r.drawn(col, line.glyph(cell)),
                self.paragraph_changed.get(row).copied().unwrap_or(true),
            ),
            None => (line.glyph(cell), self.damage.changed(row, col, self.since)),
        };
        #[cfg(not(feature = "bidi"))]
        let (glyph, changed) = (line.glyph(cell), self.damage.changed(row, col, self.since));
        FrameCell {
            glyph,
            width: cell.width(),
            fg: c.fg,
            bg: c.bg,
            underline_color: c.underline_color,
            attrs: c.attrs,
            default_bg: c.default_bg,
            link,
            hovered: link.is_some() && link == self.hovered,
            selected: c.selected,
            changed,
            #[cfg(feature = "bidi")]
            col,
            #[cfg(feature = "bidi")]
            rtl: order.is_some_and(|r| r.rtl_at(col)),
            #[cfg(feature = "bidi")]
            joins: order.map(|r| r.joins_at(col)).unwrap_or_default(),
            #[cfg(feature = "bidi")]
            mirror: order.and_then(|r| r.mirror(col, line.glyph(cell))),
        }
    }

    /// `style`'s colors as a cell draws them, `selected` or not.
    fn resolve(&self, style: Style, selected: bool) -> Colors {
        let fg_color = match style.fg {
            Color::Indexed(i) if i < 8 && style.attrs.contains(Attrs::BOLD) => {
                Color::Indexed(i.saturating_add(8))
            }
            c => c,
        };
        let mut fg = self.palette.resolve(fg_color, self.palette.fg());
        let mut bg = self.palette.resolve(style.bg, self.palette.bg());
        let mut inverse = style.attrs.contains(Attrs::INVERSE) != self.reverse;
        if inverse {
            core::mem::swap(&mut fg, &mut bg);
        }
        let mut default_bg = style.bg == Color::Default && !inverse;
        if selected {
            let sel_bg = self.palette.special(Special::SelectionBg);
            let sel_fg = self.palette.special(Special::SelectionFg);
            if sel_bg.is_none() {
                core::mem::swap(&mut fg, &mut bg);
                inverse = !inverse;
                default_bg = style.bg == Color::Default && !inverse;
            }
            if let Some(c) = sel_bg {
                bg = c;
                default_bg = false;
            }
            if let Some(c) = sel_fg {
                fg = c;
            }
        }
        if style.attrs.contains(Attrs::DIM) {
            fg = dim(fg, bg);
        }
        let underline_color = match style.underline_color {
            Color::Default => None,
            c => Some(self.palette.resolve(c, fg)),
        };
        Colors {
            fg,
            bg,
            underline_color,
            attrs: style.attrs,
            default_bg,
            selected,
        }
    }
}

/// The rows beyond the screen. A row of the capture is addressed by its
/// position: negative above the screen, from 0 up to the screen's last row
/// on it, and from the screen's row count up below it. Each method answers
/// for a row on the screen as the method of the screen's rows does.
///
/// References:
/// - ADR-1310 D2, "The snapshot asks for them": the positions, and the
///   count of rows the capture holds on each side.
#[cfg(feature = "rows-beyond")]
impl<'a> Frame<'a> {
    /// The frame with the rows beyond the screen: `above`, top to bottom,
    /// ending with the row just above the screen, and `below`, top to
    /// bottom, starting with the row just below it.
    pub fn with_rows_beyond(mut self, above: Vec<&'a Line>, below: Vec<&'a Line>) -> Self {
        #[cfg(feature = "bidi")]
        {
            let wrapped = |l: &&&Line| l.flags().contains(LineFlags::WRAPPED);
            let up = above.iter().rev().take_while(wrapped).count();
            let last = self.lines.last().into_iter();
            let down = last.chain(&below).take_while(wrapped).count();
            self.beyond.joined = (up, down.min(below.len()));
        }
        self.beyond.above = above;
        self.beyond.below = below;
        self
    }

    /// The frame with the logical columns the selection covers in each row
    /// above the screen and in each row below it, each list top to bottom.
    pub fn with_selection_beyond(
        mut self,
        above: Vec<Range<usize>>,
        below: Vec<Range<usize>>,
    ) -> Self {
        self.beyond.selection_above = above;
        self.beyond.selection_below = below;
        self
    }

    /// The frame with the image placements that have a row in the
    /// capture, in draw order.
    #[cfg(feature = "images")]
    pub fn with_placements_beyond(mut self, placements: Vec<vtium_image::FramePlacement>) -> Self {
        self.beyond.placements = placements;
        self
    }

    /// The frame with the order each row above and each row below the
    /// screen is drawn in, each top to bottom, as the screen built them.
    #[cfg(feature = "bidi")]
    pub fn with_order_beyond(mut self, above: Order, below: Order) -> Self {
        self.beyond.order_above = above;
        self.beyond.order_below = below;
        self
    }

    /// How many rows the capture holds above the screen.
    pub fn rows_above(&self) -> usize {
        self.beyond.above.len()
    }

    /// How many rows the capture holds below the screen.
    pub fn rows_below(&self) -> usize {
        self.beyond.below.len()
    }

    /// The positions of every row the capture holds, from the topmost row
    /// above the screen to the last row below it.
    pub fn captured(&self) -> Range<isize> {
        let above = isize::try_from(self.rows_above()).unwrap_or(isize::MAX);
        let end = self.rows().saturating_add(self.rows_below());
        0isize.saturating_sub(above)..isize::try_from(end).unwrap_or(isize::MAX)
    }

    /// The rows above the screen, top to bottom, for a holder of the
    /// frame's parts.
    pub fn lines_above(&self) -> &[&'a Line] {
        &self.beyond.above
    }

    /// The rows below the screen, top to bottom.
    pub fn lines_below(&self) -> &[&'a Line] {
        &self.beyond.below
    }

    /// The selected logical columns of each row above and each row below
    /// the screen, each list top to bottom.
    pub fn selection_beyond(&self) -> (&[Range<usize>], &[Range<usize>]) {
        (&self.beyond.selection_above, &self.beyond.selection_below)
    }

    /// The image placements with a row in the capture, in the order the
    /// frame draws them. Each placement's row is the row of its box's top,
    /// counted from the screen's top and negative above it, and the
    /// renderer cuts the box to the rows it draws, as for
    /// [`placements`](Self::placements). A renderer reads this list for a
    /// row beyond the screen, and `placements` for the screen's rows.
    #[cfg(feature = "images")]
    pub fn placements_beyond(&self) -> &[vtium_image::FramePlacement] {
        &self.beyond.placements
    }

    /// The order each row above and each row below the screen is drawn
    /// in, each top to bottom.
    #[cfg(feature = "bidi")]
    pub fn order_beyond(&self) -> (&Order, &Order) {
        (&self.beyond.order_above, &self.beyond.order_below)
    }

    /// Where the row at position `row` lies, or `None` outside the
    /// capture.
    fn place(&self, row: isize) -> Option<Place> {
        match usize::try_from(row) {
            Ok(y) if y < self.rows() => Some(Place::Screen(y)),
            Ok(y) => {
                let i = y.checked_sub(self.rows())?;
                (i < self.rows_below()).then_some(Place::Below(i))
            }
            Err(_) => {
                let back = row.unsigned_abs();
                self.rows_above().checked_sub(back).map(Place::Above)
            }
        }
    }

    /// The line at position `row`, on the screen or beyond it.
    pub fn line_at(&self, row: isize) -> Option<&'a Line> {
        match self.place(row)? {
            Place::Above(i) => self.beyond.above.get(i).copied(),
            Place::Screen(y) => self.line(y),
            Place::Below(i) => self.beyond.below.get(i).copied(),
        }
    }

    /// The size the row at position `row` draws at, as
    /// [`size`](Self::size) gives it on the screen. A row outside the
    /// capture draws at the normal size.
    pub fn size_at(&self, row: isize) -> LineSize {
        match self.place(row) {
            Some(Place::Screen(y)) => self.size(y),
            _ => self
                .line_at(row)
                .map_or(LineSize::Single, |l| l.flags().size()),
        }
    }

    /// The other row of the pair of double-height lines that the row at
    /// position `row` belongs to, by the rule of [`pair_row`](Self::pair_row)
    /// over every row of the capture. A pair split by the screen's top or
    /// bottom edge pairs here, where `pair_row`, which sees only the
    /// screen, leaves both halves alone.
    pub fn pair_at(&self, row: isize) -> Option<isize> {
        let top = self.captured().start;
        let index = usize::try_from(row.checked_sub(top)?).ok()?;
        let size = |i: usize| {
            let r = isize::try_from(i).map_or(isize::MAX, |i| top.saturating_add(i));
            self.size_at(r)
        };
        let other = pair_of(index, size)?;
        isize::try_from(other).ok().map(|o| top.saturating_add(o))
    }

    /// The logical columns of the row at position `row` that the
    /// selection covers; an empty range where it covers none.
    pub fn selection_at(&self, row: isize) -> Range<usize> {
        let cols = match self.place(row) {
            Some(Place::Above(i)) => self.beyond.selection_above.get(i),
            Some(Place::Screen(y)) => self.selection.get(y),
            Some(Place::Below(i)) => self.beyond.selection_below.get(i),
            None => None,
        };
        cols.cloned().unwrap_or(0..0)
    }

    /// Whether the rows beyond the screen changed for this display: only
    /// in a frame drawn whole, when the scroll record cannot say how the
    /// rows moved since the display's age. A line of the scrollback never
    /// changes in place, and output while the view is scrolled back marks
    /// everything, so a row beyond the screen has no change of its own.
    /// The row's line id tells a display whether it already drew the line.
    ///
    /// References:
    /// - ADR-1310 D2, "What the capture holds under `scroll-pan`": a row
    ///   beyond the screen reads as changed only in a frame drawn whole.
    pub fn beyond_changed(&self) -> bool {
        self.scrolled().is_none()
    }

    /// Whether any cell of the row at position `row` changed for this
    /// display: [`row_changed`](Self::row_changed) on the screen, and
    /// [`beyond_changed`](Self::beyond_changed) beyond it.
    ///
    /// Under bidi, a row beyond the screen that continues the paragraph of
    /// the screen's top or bottom row is ordered with that whole paragraph,
    /// so it also reads as changed whenever that screen row does.
    pub fn row_changed_at(&self, row: isize) -> bool {
        match self.place(row) {
            Some(Place::Screen(y)) => self.row_changed(y),
            Some(place) => self.beyond_row_changed(place),
            None => false,
        }
    }

    /// Whether the row beyond the screen at `place` changed for this
    /// display, as [`row_changed_at`](Self::row_changed_at) reads it.
    fn beyond_row_changed(&self, place: Place) -> bool {
        if self.beyond_changed() {
            return true;
        }
        #[cfg(feature = "bidi")]
        {
            let (up, down) = self.beyond.joined;
            let edge = match place {
                Place::Above(i) if self.rows_above().saturating_sub(i) <= up => Some(0),
                Place::Below(i) if i < down => self.rows().checked_sub(1),
                _ => None,
            };
            if let Some(y) = edge {
                return self.row_changed(y);
            }
        }
        #[cfg(not(feature = "bidi"))]
        let _ = place;
        false
    }

    /// The line beyond the screen at `place`, with its selection and, in a
    /// build with the bidi feature, its order.
    fn beyond_row(&self, place: Place) -> Option<BeyondRow<'a, '_>> {
        let changed = self.beyond_row_changed(place);
        let (line, selection, i, below) = match place {
            Place::Above(i) => (
                self.beyond.above.get(i)?,
                self.beyond.selection_above.get(i),
                i,
                false,
            ),
            Place::Below(i) => (
                self.beyond.below.get(i)?,
                self.beyond.selection_below.get(i),
                i,
                true,
            ),
            Place::Screen(_) => return None,
        };
        #[cfg(feature = "bidi")]
        let order = if below {
            self.beyond.order_below.row(i)
        } else {
            self.beyond.order_above.row(i)
        };
        #[cfg(not(feature = "bidi"))]
        let _ = (i, below);
        Some(BeyondRow {
            line,
            selection,
            changed,
            #[cfg(feature = "bidi")]
            order,
        })
    }

    /// The cell drawn at position `row`, column `col` from the left, as
    /// [`cell`](Self::cell) reads a cell of the screen; `None` outside the
    /// capture.
    pub fn cell_at(&self, row: isize, col: usize) -> Option<FrameCell<'a>> {
        let place = self.place(row)?;
        if let Place::Screen(y) = place {
            return self.cell(y, col);
        }
        if col >= self.cols() {
            return None;
        }
        let r = self.beyond_row(place)?;
        let col = r.logical(col);
        let cell = r.line.cell(col);
        let selected = r.selection.is_some_and(|s| s.contains(&col));
        let colors = self.resolve(
            drawn(r.line.style(cell), is_placeholder(r.line, cell)),
            selected,
        );
        Some(self.beyond_cell(&r, cell, col, colors))
    }

    /// The cells of the row at position `row` as drawn, left to right, as
    /// [`cells`](Self::cells) reads a row of the screen; none outside the
    /// capture.
    pub fn cells_at(&self, row: isize) -> impl Iterator<Item = FrameCell<'a>> + '_ {
        let place = self.place(row);
        let screen = match place {
            Some(Place::Screen(y)) => Some(y),
            _ => None,
        };
        let beyond = place.and_then(|p| self.beyond_row(p));
        let cols = self.cols();
        let mut last: Option<(StyleIndex, bool, bool, Colors)> = None;
        let beyond_cells = beyond
            .into_iter()
            .flat_map(move |r| (0..cols).map(move |col| (r, col)));
        screen
            .into_iter()
            .flat_map(move |y| self.cells(y))
            .chain(beyond_cells.map(move |(r, col)| {
                let col = r.logical(col);
                let cell = r.line.cell(col);
                let selected = r.selection.is_some_and(|s| s.contains(&col));
                let placeholder = is_placeholder(r.line, cell);
                let colors = match last {
                    Some((style, sel, ph, colors))
                        if style == cell.style() && sel == selected && ph == placeholder =>
                    {
                        colors
                    }
                    _ => {
                        let colors = self.resolve(drawn(r.line.style(cell), placeholder), selected);
                        last = Some((cell.style(), selected, placeholder, colors));
                        colors
                    }
                };
                self.beyond_cell(&r, cell, col, colors)
            }))
    }

    /// A cell of a row beyond the screen, at logical column `col`. The
    /// frame builds it as a cell of no row on the screen, which has no
    /// order and no damage, then gives it the row's change and, in a build
    /// with the bidi feature, the row's order.
    fn beyond_cell(
        &self,
        r: &BeyondRow<'a, '_>,
        cell: Cell,
        col: usize,
        c: Colors,
    ) -> FrameCell<'a> {
        let mut out = self.frame_cell(r.line, cell, usize::MAX, col, c);
        out.changed = r.changed;
        #[cfg(feature = "bidi")]
        if let Some(order) = r.order {
            let glyph = r.line.glyph(cell);
            out.glyph = order.drawn(col, glyph);
            out.rtl = order.rtl_at(col);
            out.joins = order.joins_at(col);
            out.mirror = order.mirror(col, glyph);
        }
        out
    }
}

/// A row beyond the screen, as a frame reads its cells.
#[cfg(feature = "rows-beyond")]
#[derive(Clone, Copy)]
struct BeyondRow<'a, 'f> {
    line: &'a Line,
    selection: Option<&'f Range<usize>>,
    /// Whether the row changed for this display.
    changed: bool,
    #[cfg(feature = "bidi")]
    order: Option<&'f RowOrder>,
}

#[cfg(feature = "rows-beyond")]
impl BeyondRow<'_, '_> {
    /// The logical column drawn at column `col`.
    fn logical(&self, col: usize) -> usize {
        #[cfg(feature = "bidi")]
        if let Some(r) = self.order {
            return r.logical_at(col);
        }
        col
    }
}

/// The other row of the pair of double-height lines that `row` belongs
/// to, where `size` gives the size of each row: a top line (ESC # 3)
/// pairs with the row directly below it when that row is a bottom line
/// (ESC # 4), and a bottom line pairs with the row directly above it when
/// that row is a top line. A top or bottom line with no such partner and a
/// line of any other size belong to no pair. The frame finds its rows'
/// pairs with this rule ([`Frame::pair_row`]), and the selection finds the
/// pairs of the lines in the scrollback and on the screen with it, so the
/// cursor, the highlight and the copy agree on which lines pair.
pub(crate) fn pair_of(row: usize, size: impl Fn(usize) -> LineSize) -> Option<usize> {
    match size(row) {
        LineSize::DoubleHeightTop => {
            let below = row.checked_add(1)?;
            (size(below) == LineSize::DoubleHeightBottom).then_some(below)
        }
        LineSize::DoubleHeightBottom => {
            let above = row.checked_sub(1)?;
            (size(above) == LineSize::DoubleHeightTop).then_some(above)
        }
        LineSize::Single | LineSize::DoubleWidth => None,
    }
}

/// Whether `cell` of `line` is a kitty placeholder cell, whose colors a
/// frame resolves as a blank cell's (ADR-1100, D13). A build without
/// images draws U+10EEEE as any private-use character.
fn is_placeholder(line: &Line, cell: Cell) -> bool {
    #[cfg(feature = "images")]
    {
        crate::placeholder::is_placeholder_cell(line, cell)
    }
    #[cfg(not(feature = "images"))]
    {
        let _ = (line, cell);
        false
    }
}

/// The style a frame resolves a cell's colors from: a placeholder cell's
/// with the default foreground and underline colors, since those name its
/// image and its placement.
fn drawn(style: Style, placeholder: bool) -> Style {
    if !placeholder {
        return style;
    }
    Style {
        fg: Color::Default,
        underline_color: Color::Default,
        ..style
    }
}

/// A cell's colors and attributes, resolved from its style.
#[derive(Clone, Copy)]
struct Colors {
    fg: Rgb,
    bg: Rgb,
    underline_color: Option<Rgb>,
    attrs: Attrs,
    default_bg: bool,
    selected: bool,
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::line::{LineId, LinkId};
    use crate::style::Style;

    const WHITE: Rgb = Rgb::new(0xff, 0xff, 0xff);
    const BLACK: Rgb = Rgb::new(0, 0, 0);

    fn palette() -> Palette {
        let mut colors = [Rgb::default(); 256];
        for (i, c) in colors.iter_mut().enumerate() {
            let i = u8::try_from(i).unwrap();
            *c = Rgb::new(i, i, i);
        }
        Palette::new(colors, WHITE, BLACK)
    }

    fn styled(style: Style) -> Line {
        let mut line = Line::new(LineId::new(1), 4);
        line.write(0, Glyph::Char('a'), Width::Narrow, &style, None);
        line
    }

    // ADR-1000's feature row "Colors resolved at draw time, so a palette
    // change recolors text" (the capability probe palette-at-draw).
    // covers: sgr.draw-time-color
    #[test]
    fn a_palette_change_recolors_text_already_written() {
        let line = styled(Style {
            fg: Color::Indexed(1),
            ..Style::DEFAULT
        });
        let damage = Damage::new(4, 1);
        let mut palette = palette();
        let before = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        )
        .cell(0, 0)
        .unwrap()
        .fg;
        assert_eq!(before, Rgb::new(1, 1, 1));
        palette.set_color(1, Rgb::new(0xcd, 0, 0));
        let frame = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        assert_eq!(frame.cell(0, 0).unwrap().fg, Rgb::new(0xcd, 0, 0));
    }

    #[test]
    fn default_colors_follow_the_palettes_defaults() {
        let line = styled(Style::DEFAULT);
        let damage = Damage::new(4, 1);
        let mut palette = palette();
        palette.set_bg(Rgb::new(0x10, 0x20, 0x30));
        let frame = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        let cell = frame.cell(0, 0).unwrap();
        assert_eq!((cell.fg, cell.bg), (WHITE, Rgb::new(0x10, 0x20, 0x30)));
        assert!(cell.default_bg);
    }

    // A background set to the default's color is not the default
    // background, and an inverted default background is not.
    #[test]
    fn default_bg_marks_only_the_default_background_uninverted() {
        let damage = Damage::new(4, 1);
        let palette = palette();
        let explicit = styled(Style {
            bg: Color::Rgb(BLACK),
            ..Style::DEFAULT
        });
        let frame = Frame::new(
            vec![&explicit],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        let cell = frame.cell(0, 0).unwrap();
        assert_eq!(cell.bg, BLACK);
        assert!(!cell.default_bg);

        let mut inverse = Style::DEFAULT;
        inverse.attrs.insert(Attrs::INVERSE);
        let inverted = styled(inverse);
        let frame = Frame::new(
            vec![&inverted],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        let cell = frame.cell(0, 0).unwrap();
        assert_eq!((cell.fg, cell.bg), (BLACK, WHITE));
        assert!(!cell.default_bg);
    }

    // ctlseqs, OSC 17 and 19: a selection background replaces the inversion,
    // so an inverse cell selected keeps its swapped foreground over it, and
    // the background drawn is no longer the default one; dim follows the
    // colors drawn.
    #[test]
    fn selection_colors_replace_the_inversion() {
        let red = Rgb::new(0xcd, 0, 0);
        let mut style = Style::DEFAULT;
        style.attrs.insert(Attrs::INVERSE);
        let line = styled(style);
        let damage = Damage::new(4, 1);
        let mut palette = palette();
        palette.set_special(Special::SelectionBg, Some(red));
        let frame = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        )
        .with_selection(vec![Range { start: 0, end: 1 }]);
        let cell = frame.cell(0, 0).unwrap();
        assert_eq!((cell.fg, cell.bg), (BLACK, red));
        assert!(cell.selected && !cell.default_bg);

        style.attrs.insert(Attrs::DIM);
        palette.set_special(Special::SelectionFg, Some(WHITE));
        let dimmed = styled(style);
        let frame = Frame::new(
            vec![&dimmed],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        )
        .with_selection(vec![Range { start: 0, end: 1 }]);
        let cell = frame.cell(0, 0).unwrap();
        assert_eq!((cell.fg, cell.bg), (Rgb::new(0xe5, 0x7f, 0x7f), red));
    }

    #[test]
    fn reverse_video_and_inverse_cancel() {
        let mut inverse = Style::DEFAULT;
        inverse.attrs.insert(Attrs::INVERSE);
        let line = styled(inverse);
        let damage = Damage::new(4, 1);
        let palette = palette();
        let frame = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            true,
        );
        let cell = frame.cell(0, 0).unwrap();
        assert_eq!((cell.fg, cell.bg), (WHITE, BLACK));
        assert!(cell.default_bg);
        let blank = frame.cell(0, 1).unwrap();
        assert_eq!((blank.fg, blank.bg), (BLACK, WHITE));
    }

    // A set underline color stays under reverse video, and the default
    // follows the foreground.
    #[test]
    fn a_set_underline_color_survives_inverse() {
        let mut style = Style {
            underline_color: Color::Indexed(9),
            ..Style::DEFAULT
        };
        style.attrs.insert(Attrs::INVERSE);
        let line = styled(style);
        let damage = Damage::new(4, 1);
        let palette = palette();
        let frame = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        assert_eq!(
            frame.cell(0, 0).unwrap().underline_color,
            Some(Rgb::new(9, 9, 9))
        );
        assert_eq!(frame.cell(0, 1).unwrap().underline_color, None);
    }

    fn with(mut style: Style, attrs: &[Attrs]) -> Style {
        for &a in attrs {
            style.attrs.insert(a);
        }
        style
    }

    /// The colors of a cell written in `style`, `reverse` being DECSCNM.
    struct Colors {
        fg: Rgb,
        bg: Rgb,
        underline_color: Option<Rgb>,
    }

    fn resolved(style: Style, reverse: bool) -> Colors {
        let line = styled(style);
        let damage = Damage::new(4, 1);
        let palette = palette();
        let frame = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            reverse,
        );
        let cell = frame.cell(0, 0).unwrap();
        Colors {
            fg: cell.fg,
            bg: cell.bg,
            underline_color: cell.underline_color,
        }
    }

    // Bold adds 8 to a palette color below 8 in the foreground. Colors 8
    // to 255, RGB, the default
    // foreground, the background and SGR 58's color stay as set.
    // covers: sgr.bold
    #[test]
    fn bold_brightens_the_first_eight_palette_colors_of_the_foreground() {
        let bold = |fg| {
            resolved(
                with(
                    Style {
                        fg,
                        ..Style::DEFAULT
                    },
                    &[Attrs::BOLD],
                ),
                false,
            )
            .fg
        };
        assert_eq!(bold(Color::Indexed(0)), Rgb::new(8, 8, 8));
        assert_eq!(bold(Color::Indexed(1)), Rgb::new(9, 9, 9));
        assert_eq!(bold(Color::Indexed(7)), Rgb::new(15, 15, 15));
        assert_eq!(bold(Color::Indexed(8)), Rgb::new(8, 8, 8));
        assert_eq!(bold(Color::Indexed(9)), Rgb::new(9, 9, 9));
        assert_eq!(bold(Color::Indexed(16)), Rgb::new(16, 16, 16));
        assert_eq!(
            bold(Color::Rgb(Rgb::new(1, 2, 3))),
            Rgb::new(1, 2, 3),
            "RGB"
        );
        assert_eq!(bold(Color::Default), WHITE, "the default foreground");

        let plain = Style {
            fg: Color::Indexed(1),
            ..Style::DEFAULT
        };
        assert_eq!(resolved(plain, false).fg, Rgb::new(1, 1, 1), "not bold");

        let cell = resolved(
            with(
                Style {
                    fg: Color::Indexed(1),
                    bg: Color::Indexed(2),
                    underline_color: Color::Indexed(3),
                    ..Style::DEFAULT
                },
                &[Attrs::BOLD],
            ),
            false,
        );
        assert_eq!(cell.bg, Rgb::new(2, 2, 2), "the background");
        assert_eq!(cell.underline_color, Some(Rgb::new(3, 3, 3)), "SGR 58");
    }

    // Bold brightens the foreground before inverse video and DECSCNM swap
    // it: the bright color becomes the background.
    // covers: sgr.bold
    #[test]
    fn bold_brightens_before_inverse_video() {
        let style = Style {
            fg: Color::Indexed(1),
            bg: Color::Indexed(4),
            ..Style::DEFAULT
        };
        let inverse = resolved(with(style, &[Attrs::BOLD, Attrs::INVERSE]), false);
        assert_eq!(
            (inverse.fg, inverse.bg),
            (Rgb::new(4, 4, 4), Rgb::new(9, 9, 9))
        );
        let reverse = resolved(with(style, &[Attrs::BOLD]), true);
        assert_eq!(
            (reverse.fg, reverse.bg),
            (Rgb::new(4, 4, 4), Rgb::new(9, 9, 9))
        );
    }

    const FG: Rgb = Rgb::new(0xff, 0x80, 0x40);
    const BG: Rgb = Rgb::new(0x21, 0xa0, 0xc3);
    /// [`dim`] of `FG` and `BG`: ff/2 + 21/2,
    /// 80/2 + a0/2, 40/2 + c3/2. A rounded mean would give 90 in red.
    const DIM: Rgb = Rgb::new(0x8f, 0x90, 0x81);

    // SGR 2: the foreground halfway to the background, each channel the
    // sum of the two halved; the background and SGR 58's color stay.
    // covers: sgr.dim
    #[test]
    fn dim_draws_the_foreground_halfway_to_the_background() {
        let style = Style {
            fg: Color::Rgb(FG),
            bg: Color::Rgb(BG),
            ..Style::DEFAULT
        };
        let cell = resolved(with(style, &[Attrs::DIM]), false);
        assert_eq!((cell.fg, cell.bg), (DIM, BG));
        assert_eq!(cell.underline_color, None);

        let set = Style {
            underline_color: Color::Rgb(Rgb::new(0, 0xff, 0)),
            ..style
        };
        let cell = resolved(with(set, &[Attrs::DIM]), false);
        assert_eq!(cell.underline_color, Some(Rgb::new(0, 0xff, 0)));

        let white = Style {
            fg: Color::Rgb(WHITE),
            bg: Color::Rgb(WHITE),
            ..Style::DEFAULT
        };
        assert_eq!(
            resolved(with(white, &[Attrs::DIM]), false).fg,
            Rgb::new(254, 254, 254),
            "each half rounds down"
        );
    }

    // Dim mixes the colors inverse video and DECSCNM left, so the old
    // foreground stays behind the mix.
    // covers: sgr.dim
    #[test]
    fn dim_follows_inverse_video() {
        let style = Style {
            fg: Color::Rgb(FG),
            bg: Color::Rgb(BG),
            ..Style::DEFAULT
        };
        let inverse = resolved(with(style, &[Attrs::DIM, Attrs::INVERSE]), false);
        assert_eq!((inverse.fg, inverse.bg), (DIM, FG));
        let reverse = resolved(with(style, &[Attrs::DIM]), true);
        assert_eq!((reverse.fg, reverse.bg), (DIM, FG));
    }

    // The selection swaps the colors as inverse video does, and dim mixes
    // what it left; over inverse text the two cancel and dim mixes the
    // colors as set.
    // covers: sgr.dim
    #[test]
    fn dim_follows_the_selection() {
        let style = Style {
            fg: Color::Rgb(FG),
            bg: Color::Rgb(BG),
            ..Style::DEFAULT
        };
        let selected = |style: Style| {
            let line = styled(style);
            let damage = Damage::new(4, 1);
            let palette = palette();
            let frame = Frame::new(
                vec![&line],
                &damage,
                &palette,
                Age::NEVER,
                Cursor::default(),
                false,
            )
            .with_selection(vec![Range { start: 0, end: 1 }]);
            let cell = frame.cell(0, 0).unwrap();
            assert!(cell.selected);
            (cell.fg, cell.bg)
        };
        assert_eq!(selected(with(style, &[Attrs::DIM])), (DIM, FG));
        assert_eq!(
            selected(with(style, &[Attrs::DIM, Attrs::INVERSE])),
            (DIM, BG)
        );
    }

    // Bold brightens first and dim mixes the bright color: 9/2 + 21/2,
    // 9/2 + a0/2, 9/2 + c3/2.
    // covers: sgr.bold, sgr.dim
    #[test]
    fn bold_and_dim_dim_the_bright_color() {
        let style = Style {
            fg: Color::Indexed(1),
            bg: Color::Rgb(BG),
            ..Style::DEFAULT
        };
        let cell = resolved(with(style, &[Attrs::BOLD, Attrs::DIM]), false);
        assert_eq!((cell.fg, cell.bg), (Rgb::new(0x14, 0x54, 0x65), BG));
    }

    #[test]
    fn cells_past_a_trimmed_line_read_blank_and_outside_read_none() {
        let mut line = styled(Style {
            bg: Color::Indexed(4),
            ..Style::DEFAULT
        });
        line.trim();
        let damage = Damage::new(4, 1);
        let palette = palette();
        let frame = Frame::new(
            vec![&line],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        let past = frame.cell(0, 3).unwrap();
        assert_eq!(past.glyph, Glyph::Empty);
        assert_eq!(past.bg, BLACK);
        assert!(past.default_bg);
        assert!(frame.cell(0, 4).is_none());
        assert!(frame.cell(1, 0).is_none());
    }

    // The row accessor reads each cell as `cell` does: styles changing
    // and repeating, dim and an underline color, a wide pair, a cluster,
    // a link, a selection starting and ending inside a style's run, a
    // changed cell and the blanks past a trimmed line.
    #[test]
    fn a_rows_cells_read_as_each_cell_does() {
        let mut line = Line::new(LineId::new(1), 8);
        let red = Style {
            fg: Color::Indexed(1),
            ..Style::DEFAULT
        };
        let bold = with(
            Style {
                bg: Color::Rgb(BG),
                underline_color: Color::Indexed(3),
                ..red
            },
            &[Attrs::BOLD, Attrs::INVERSE, Attrs::DIM],
        );
        line.write(0, Glyph::Char('a'), Width::Narrow, &red, None);
        line.write(
            1,
            Glyph::Char('b'),
            Width::Narrow,
            &red,
            Some(LinkId::new(7)),
        );
        line.write(2, Glyph::Char('c'), Width::Narrow, &bold, None);
        line.write(3, Glyph::Char('\u{4e00}'), Width::Wide, &bold, None);
        line.write(4, Glyph::Empty, Width::WideTail, &bold, None);
        line.write(5, Glyph::Cluster("e\u{301}"), Width::Narrow, &red, None);
        line.trim();
        let other = Line::new(LineId::new(2), 8);
        let mut damage = Damage::new(8, 2);
        let seen = damage.checkpoint();
        damage.mark_cell(0, 2);
        let palette = palette();
        let frame = Frame::new(
            vec![&line, &other],
            &damage,
            &palette,
            seen,
            Cursor::default(),
            true,
        )
        .with_selection(vec![Range { start: 1, end: 4 }]);
        for row in 0..2 {
            let each: Vec<_> = (0..8).map(|col| frame.cell(row, col).unwrap()).collect();
            let cells: Vec<_> = frame.cells(row).collect();
            assert_eq!(cells, each, "row {row}");
        }
        let at = |col: usize| frame.cells(0).nth(col).unwrap();
        assert!(at(2).changed);
        assert!(at(1).selected);
        assert_eq!(at(1).link, Some(LinkId::new(7)));
        assert_eq!(at(3).underline_color, Some(Rgb::new(3, 3, 3)));
        // bold's 9 dimmed toward BG, 9/2 + 21/2, 9/2 + a0/2, 9/2 + c3/2
        assert_eq!(at(3).fg, Rgb::new(0x14, 0x54, 0x65), "dim");
        assert_eq!(frame.cells(2).count(), 0, "outside the frame");
    }

    #[test]
    fn a_frame_reports_what_changed_for_its_display() {
        let lines = [Line::new(LineId::new(1), 4), Line::new(LineId::new(2), 4)];
        let mut damage = Damage::new(4, 2);
        let seen = damage.checkpoint();
        damage.mark_cell(1, 2);
        let palette = palette();
        let rows = lines.iter().collect();
        let frame = Frame::new(rows, &damage, &palette, seen, Cursor::default(), false);
        assert!(frame.cell(1, 2).unwrap().changed);
        assert!(!frame.cell(1, 1).unwrap().changed);
        assert!(!frame.row_changed(0));
        assert!(frame.row_changed(1));
        assert_eq!(frame.line(1).unwrap().id(), LineId::new(2));
    }

    // ADR-1100, D13 (the maintainer, 2026-11-02): a placeholder cell's
    // foreground names its image, so under SGR 7, under DECSCNM and inside
    // a selection drawn by swapping it takes the colors a space with the
    // same background and attributes takes, and never the image id's
    // color. It keeps its glyph and attributes, by which the renderer draws
    // no text, underline or strike-through there.
    // covers: image.kitty-placeholders
    #[cfg(feature = "images")]
    #[test]
    fn a_placeholder_cell_takes_a_blank_cells_colors_under_reverse_video() {
        use crate::style::UnderlineStyle;
        let mut attrs = Attrs::empty();
        attrs.insert(Attrs::INVERSE);
        attrs.insert(Attrs::STRIKE);
        attrs.set_underline(UnderlineStyle::Single);
        let placeholder = Style {
            fg: Color::Indexed(42),
            bg: Color::Indexed(4),
            underline_color: Color::Indexed(7),
            attrs,
        };
        let mut blank_attrs = Attrs::empty();
        blank_attrs.insert(Attrs::INVERSE);
        let space = Style {
            bg: Color::Indexed(4),
            attrs: blank_attrs,
            ..Style::DEFAULT
        };
        let mut line = Line::new(LineId::new(1), 4);
        let cluster = "\u{10EEEE}\u{0305}\u{030D}";
        line.write(
            0,
            Glyph::Cluster(cluster),
            Width::Narrow,
            &placeholder,
            None,
        );
        line.write(
            1,
            Glyph::Char('\u{10EEEE}'),
            Width::Narrow,
            &placeholder,
            None,
        );
        line.write(2, Glyph::Char(' '), Width::Narrow, &space, None);
        // a row whose first cell is text in the placeholder's own style:
        // the colors `cells` resolves once for a run of one style are not
        // the placeholder's
        let mut mixed = Line::new(LineId::new(2), 4);
        mixed.write(0, Glyph::Char('a'), Width::Narrow, &placeholder, None);
        mixed.write(
            1,
            Glyph::Cluster(cluster),
            Width::Narrow,
            &placeholder,
            None,
        );
        mixed.write(2, Glyph::Char(' '), Width::Narrow, &space, None);
        let damage = Damage::new(4, 2);
        let palette = palette();
        let frame = Frame::new(
            vec![&line, &mixed],
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        let cells: Vec<FrameCell<'_>> = frame.cells(1).collect();
        let cell = |i: usize| cells.get(i).copied().unwrap();
        let (text, read, blank) = (cell(0), cell(1), cell(2));
        assert_eq!((read.fg, read.bg), (blank.fg, blank.bg));
        assert_ne!((text.fg, text.bg), (read.fg, read.bg));
        for (reverse, selection) in [(false, None), (true, None), (false, Some(0..4))] {
            let mut frame = Frame::new(
                vec![&line],
                &damage,
                &palette,
                Age::NEVER,
                Cursor::default(),
                reverse,
            );
            if let Some(cols) = selection.clone() {
                frame = frame.with_selection(vec![cols]);
            }
            let blank = frame.cell(0, 2).unwrap();
            let cells: Vec<FrameCell<'_>> = frame.cells(0).collect();
            for (col, &read) in cells.iter().enumerate().take(2) {
                for cell in [frame.cell(0, col).unwrap(), read] {
                    let at = format!("column {col}, reverse {reverse}, selection {selection:?}");
                    assert_eq!((cell.fg, cell.bg), (blank.fg, blank.bg), "{at}");
                    assert_ne!(cell.bg, Rgb::new(42, 42, 42), "{at}");
                    assert_eq!(cell.default_bg, blank.default_bg, "{at}");
                    assert_eq!(cell.underline_color, None, "{at}");
                    assert_eq!(cell.attrs.underline(), UnderlineStyle::Single, "{at}");
                }
            }
        }
    }

    /// Lines of the given sizes, one per row, four columns wide.
    fn sized(sizes: &[LineSize]) -> Vec<Line> {
        sizes
            .iter()
            .map(|&size| {
                let mut line = Line::new(LineId::new(1), 4);
                line.flags_mut().set_size(size);
                line
            })
            .collect()
    }

    /// The cursor's extent on `row` of a frame of `lines`, with the
    /// overlay row at `overlay` if any, for a character `cells` wide: the
    /// display cells across and the rows.
    fn extent(
        lines: &[Line],
        overlay: Option<usize>,
        row: usize,
        cells: usize,
    ) -> (usize, Range<usize>) {
        let damage = Damage::new(4, lines.len());
        let palette = palette();
        let frame = Frame::new(
            lines.iter().collect(),
            &damage,
            &palette,
            Age::NEVER,
            Cursor::default(),
            false,
        )
        .with_hover(None, overlay.map(|r| (r, "u")));
        let e = frame.cursor_extent(row, cells);
        (e.cells, e.rows)
    }

    #[test]
    fn the_cursor_covers_its_own_cells_on_a_single_line() {
        use LineSize::Single;
        let lines = sized(&[Single, Single]);
        assert_eq!(extent(&lines, None, 1, 1), (1, 1..2));
        assert_eq!(extent(&lines, None, 0, 2), (2, 0..1));
    }

    #[test]
    fn the_cursor_doubles_its_width_on_a_double_width_line() {
        let lines = sized(&[LineSize::DoubleWidth, LineSize::DoubleHeightBottom]);
        assert_eq!(extent(&lines, None, 0, 1), (2, 0..1));
        assert_eq!(extent(&lines, None, 0, 2), (4, 0..1));
    }

    #[test]
    fn the_cursor_covers_both_rows_of_a_double_height_pair() {
        use LineSize::{DoubleHeightBottom, DoubleHeightTop, Single};
        let lines = sized(&[Single, DoubleHeightTop, DoubleHeightBottom, Single]);
        for row in [1, 2] {
            assert_eq!(extent(&lines, None, row, 1), (2, 1..3), "row {row}");
        }
        assert_eq!(extent(&lines, None, 2, 2), (4, 1..3));
    }

    #[test]
    fn a_double_height_line_with_no_partner_covers_its_own_row() {
        use LineSize::{DoubleHeightBottom, DoubleHeightTop, DoubleWidth};
        // Rows 2 and 3 are the only pair. Row 0 is a bottom line at the
        // frame's first row, row 1 is a top line above another top line,
        // row 4 is a bottom line below another bottom line, row 6 is a
        // bottom line below a double-width line, and row 7 is a top line
        // at the frame's last row. None of them has a partner.
        let lines = sized(&[
            DoubleHeightBottom,
            DoubleHeightTop,
            DoubleHeightTop,
            DoubleHeightBottom,
            DoubleHeightBottom,
            DoubleWidth,
            DoubleHeightBottom,
            DoubleHeightTop,
        ]);
        for row in [0, 1, 4, 6, 7] {
            assert_eq!(extent(&lines, None, row, 1), (2, row..row + 1), "row {row}");
        }
        assert_eq!(extent(&lines, None, 2, 1), (2, 2..4));
        assert_eq!(extent(&lines, None, 3, 1), (2, 2..4));
    }

    #[test]
    fn the_overlay_row_draws_at_the_normal_size_and_pairs_with_nothing() {
        use LineSize::{DoubleHeightBottom, DoubleHeightTop};
        let lines = sized(&[DoubleHeightTop, DoubleHeightBottom]);
        assert_eq!(extent(&lines, Some(1), 0, 1), (2, 0..1));
        assert_eq!(extent(&lines, Some(1), 1, 1), (1, 1..2));
    }
}
