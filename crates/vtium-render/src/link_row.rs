//! The overlay row that shows the hovered link's address while Ctrl is held
//! (ADR-1002, "Drawing"), or a note of vtium's own in its place, and the
//! labels of link hints. The frame names the row and its text
//! ([`Frame::overlay`]) and the labels ([`Frame::link_hints`]). The
//! renderer draws the row's text from the row's first column, one byte to
//! a character cell, in the palette's default colors inverted against what
//! the screen shows: swapped while reverse video (DECSCNM) is off, and as
//! they are while it is on. An address longer than the row is cut at the
//! row's last column, which shows `…` in place of the rest.
//!
//! The overlay covers only the cells its text takes. The rest of the row
//! shows what the program drew there. The renderer reads each cell through
//! [`LinkRow::cell`], which puts the overlay's character in place of the
//! screen's, so the CPU path and the GPU path draw it the same way. Under
//! those cells the text cursor and the selection are not drawn. The
//! pointer draws above the overlay, as it draws above every cell.
//!
//! A label of link hints draws the same way: each of its letters takes a
//! character cell, from the cell where the frame places the label, in the
//! inverted default colors, and the cursor and the selection are not
//! drawn under it. A label that the letters typed no longer begin draws
//! nothing. The screen places labels so that they do not overlap, unless a
//! row is too short for its labels; then a later label draws over an
//! earlier one. Every visible cell of a labelled link takes the dashed
//! underline that the pointer's hover gives a link, shown label or not.
//!
//! The frame marks the overlay's row changed when the overlay appears,
//! moves, changes its text or goes, and the rows of labels and labelled
//! links when they change, so the renderer redraws the row's cells then.
//!
//! References:
//! - ADR-1002, section 4 ("Drawing"): the overlay row, its colors and what
//!   is not drawn under it.
//! - ADR-1002, section 7 ("Keyboard link hints"): how labels are drawn,
//!   and the dashed underline of every labelled link.

use vtium_term::{Attrs, Frame, FrameCell, Glyph, LinkHint, LinkId, Rgb, Width};

/// The character that ends an address cut at the row's end.
pub(crate) const CUT: char = '\u{2026}';

#[cfg(test)]
std::thread_local! {
    /// The labels and labelled links that drawing has looked at on this
    /// thread, which a test counts to check that a cell costs little
    /// however many labels the screen holds.
    static PROBES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Counts one label or labelled link looked at, in a test build.
fn probe() {
    #[cfg(test)]
    PROBES.with(|p| p.set(p.get().saturating_add(1)));
}

/// The overlay row and the labels of link hints of one frame, fitted to
/// the grid's columns.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LinkRow<'f> {
    /// The row the overlay covers; past every row when no overlay shows.
    row: usize,
    uri: &'f str,
    /// The cells the overlay covers, from column 0.
    len: usize,
    /// The address is longer than the row, so its last cell shows `…`.
    cut: bool,
    fg: Rgb,
    bg: Rgb,
    /// Reverse video is set, under which a cell written inverted shows the
    /// default background, which an image below the cells shows through.
    reverse: bool,
    /// The labels of link hints, in the order they lie in the view.
    hints: &'f [LinkHint],
    /// The links that have a label, sorted.
    labelled: &'f [LinkId],
    /// The letters of the longest label, so a cell looks back over only
    /// the labels that start close enough to reach it.
    reach: usize,
    /// The grid's columns, where every label ends at the latest.
    cols: usize,
}

impl<'f> LinkRow<'f> {
    /// The overlay and the labels `frame` shows, on a grid of `cols`
    /// columns and `rows` rows; `None` when the frame shows neither, or
    /// when the grid is empty. An overlay whose row is past the grid, or
    /// whose text is empty, shows nothing.
    pub(crate) fn new(frame: &Frame<'f>, cols: usize, rows: usize) -> Option<Self> {
        let hints = frame.link_hints();
        let overlay = frame
            .overlay()
            .filter(|o| o.row < rows && !o.uri.is_empty());
        if cols == 0 || (overlay.is_none() && hints.is_empty()) {
            return None;
        }
        let (row, uri) = overlay.map_or((usize::MAX, ""), |o| (o.row, o.uri));
        let palette = frame.palette();
        // the default colors inverted against the screen: under reverse
        // video the screen shows them swapped, so the overlay swaps them
        // back, as a cell written inverted resolves
        let (fg, bg) = if frame.reverse() {
            (palette.fg(), palette.bg())
        } else {
            (palette.bg(), palette.fg())
        };
        Some(LinkRow {
            row,
            uri,
            len: uri.len().min(cols),
            cut: uri.len() > cols,
            fg,
            bg,
            reverse: frame.reverse(),
            hints,
            labelled: frame.labelled_links(),
            reach: hints.iter().map(|h| h.label.len()).max().unwrap_or(0),
            cols,
        })
    }

    /// The row the overlay covers.
    pub(crate) fn row(&self) -> usize {
        self.row
    }

    /// Whether the overlay or a shown label covers the cell at `col` of
    /// `row`.
    pub(crate) fn covers(&self, col: usize, row: usize) -> bool {
        (row == self.row && col < self.len) || self.label_at(col, row).is_some()
    }

    /// The letter a shown label draws at `col` of `row`, if one does. The
    /// overlay row draws above the labels.
    ///
    /// The labels come in the order they lie in the view, so a binary
    /// search finds the last label that starts at or before the cell. The
    /// labels before it on the row are read back from there, only as far
    /// as one could still reach the cell. The first shown label that
    /// covers the cell draws there, so a later label draws over an earlier
    /// one, and a hidden label lets the one under it show.
    fn label_at(&self, col: usize, row: usize) -> Option<char> {
        if col >= self.cols {
            return None;
        }
        let end = self
            .hints
            .partition_point(|h| (h.at.1, h.at.0) <= (row, col));
        self.hints
            .get(..end)?
            .iter()
            .rev()
            .take_while(|h| {
                probe();
                h.at.1 == row && h.at.0.saturating_add(self.reach) > col
            })
            .filter(|h| h.shown)
            .find_map(|h| h.label.chars().nth(col.saturating_sub(h.at.0)))
    }

    /// Whether `link` has a label, so each of its cells takes the hover's
    /// dashed underline.
    fn labelled(&self, link: Option<LinkId>) -> bool {
        link.is_some_and(|l| {
            probe();
            self.labelled.binary_search(&l).is_ok()
        })
    }

    /// The character the overlay shows at `col`, inside the cells it
    /// covers.
    fn char_at(&self, col: usize) -> Option<char> {
        if col >= self.len {
            return None;
        }
        if self.cut && col.saturating_add(1) == self.len {
            return Some(CUT);
        }
        // The frame's address is printable ASCII (ADR-1002, "What a link
        // may hold"), so each byte is one character.
        self.uri.as_bytes().get(col).map(|&b| char::from(b))
    }

    /// The characters the overlay shows, left to right.
    pub(crate) fn chars(&self) -> impl Iterator<Item = char> + '_ {
        (0..self.len).filter_map(|col| self.char_at(col))
    }

    /// The cell `under`, at `col` of `row`, as the frame draws it with the
    /// overlay and the labels: the overlay's or a label's character in its
    /// place where either covers it, and elsewhere `under`, hovered when
    /// its link has a label.
    pub(crate) fn cell(&self, col: usize, row: usize, under: FrameCell<'f>) -> FrameCell<'f> {
        let overlay = self.char_at(col).filter(|_| row == self.row);
        let Some(c) = overlay.or_else(|| self.label_at(col, row)) else {
            let mut fc = under;
            fc.hovered |= self.labelled(fc.link);
            return fc;
        };
        let mut fc = under;
        fc.glyph = Glyph::Char(c);
        fc.width = Width::Narrow;
        fc.fg = self.fg;
        fc.bg = self.bg;
        fc.underline_color = None;
        // the attribute that a cell written in the default colors inverted
        // carries, which a 1-bit display reads to light the background
        fc.attrs = Attrs::INVERSE;
        fc.default_bg = self.reverse;
        fc.link = None;
        fc.hovered = false;
        fc.selected = false;
        #[cfg(feature = "bidi")]
        {
            fc.rtl = false;
            fc.joins = vtium_term::Joins::default();
            fc.mirror = None;
        }
        fc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_term::{Age, Palette, Terminal};

    /// A terminal 8 columns by 3 rows with `x:a` linked on "abc" from
    /// column 2 of the top row, and plain text after it.
    fn linked() -> Terminal {
        let mut t = Terminal::new(8, 3, 0);
        t.input(b"  \x1b]8;;x:a\x1b\\abc\x1b]8;;\x1b\\de");
        t
    }

    /// Labels the terminal's one link `label`.
    fn label(t: &mut Terminal, label: &str) {
        let links = t.screen().view_links();
        let s = t.screen_mut();
        s.set_link_hints(links.into_iter().map(|l| (l, label.to_owned())).collect());
    }

    /// What the cell at `col`, `row` draws: its character, whether it is
    /// inverted, and whether it takes the hover's underline.
    fn drawn(row: &LinkRow<'_>, frame: &Frame<'_>, col: usize, y: usize) -> (char, bool, bool) {
        let fc = frame.cell(y, col).map(|fc| row.cell(col, y, fc));
        fc.map_or((' ', false, false), |fc| {
            let c = match fc.glyph {
                Glyph::Char(c) => c,
                _ => ' ',
            };
            (c, fc.attrs.contains(Attrs::INVERSE), fc.hovered)
        })
    }

    // ADR-1002, section 7: a label covers the first cells of its link,
    // inverted, and every cell of a labelled link takes the hover's
    // underline.
    #[test]
    fn a_label_draws_over_its_link_and_the_link_underlines() {
        let mut t = linked();
        label(&mut t, "sd");
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        let row = LinkRow::new(&frame, 8, 3);
        assert!(row.is_some(), "no link row");
        let Some(row) = row else { return };
        assert_eq!(drawn(&row, &frame, 2, 0), ('s', true, false));
        assert_eq!(drawn(&row, &frame, 3, 0), ('d', true, false));
        assert_eq!(
            drawn(&row, &frame, 4, 0),
            ('c', false, true),
            "the link's last cell"
        );
        assert_eq!(drawn(&row, &frame, 5, 0), ('d', false, false), "plain text");
        assert!(row.covers(2, 0) && row.covers(3, 0));
        assert!(!row.covers(4, 0) && !row.covers(2, 1));
    }

    // ADR-1002, section 7: a label the letters typed no longer begin draws
    // nothing, and its link keeps the underline.
    #[test]
    fn a_hidden_label_draws_nothing_and_its_link_still_underlines() {
        let mut t = linked();
        label(&mut t, "sd");
        t.screen_mut().show_link_hints(|l| l.starts_with('a'));
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        let row = LinkRow::new(&frame, 8, 3);
        assert!(row.is_some(), "no link row");
        let Some(row) = row else { return };
        assert_eq!(drawn(&row, &frame, 2, 0), ('a', false, true));
        assert!(!row.covers(2, 0));
    }

    // ADR-1002, section 7: an overlay row draws from the row's first
    // column, above any label on that row. The screen never puts its note
    // over a label, so the frame here is given the overlay directly, to
    // check what the renderer draws if a frame holds both.
    #[test]
    fn the_overlay_row_draws_above_a_label() {
        let mut t = Terminal::new(8, 1, 0);
        t.input(b"\x1b]8;;x:a\x1b\\abc\x1b]8;;\x1b\\");
        label(&mut t, "s");
        let palette = Palette::default();
        let frame = t
            .screen()
            .frame(&palette, Age::NEVER)
            .with_hover(None, Some((0, "Copied")));
        let row = LinkRow::new(&frame, 8, 1);
        assert!(row.is_some(), "no link row");
        let Some(row) = row else { return };
        assert_eq!(drawn(&row, &frame, 0, 0), ('C', true, false));
        assert_eq!(drawn(&row, &frame, 6, 0), (' ', false, false));
    }

    // With neither an overlay nor a label, the frame has no link row.
    #[test]
    fn no_overlay_and_no_label_is_no_link_row() {
        let t = linked();
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        assert!(LinkRow::new(&frame, 8, 3).is_none());
    }

    // Drawing reads every cell through the link row, so finding a cell's
    // label and whether its link has one costs a few lookups, not one for
    // each label on the screen.
    #[test]
    fn a_cell_looks_up_few_labels_on_a_screen_full_of_them() {
        let (cols, rows) = (40, 20);
        let mut t = Terminal::new(cols, rows, 0);
        let mut bytes = String::new();
        for y in 0..rows {
            bytes.push_str(&format!("\x1b[{};1H", y + 1));
            // links of four cells, so the labels of three letters leave a
            // cell of each link uncovered
            for x in 0..cols / 4 {
                bytes.push_str(&format!("\x1b]8;;x:{x},{y}\x1b\\abcd\x1b]8;;\x1b\\"));
            }
        }
        t.input(bytes.as_bytes());
        let links = t.screen().view_links();
        let labels = (0..links.len()).map(|i| format!("{i:03}"));
        let hints = links.into_iter().zip(labels).collect();
        t.screen_mut().set_link_hints(hints);
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        let row = LinkRow::new(&frame, cols, rows);
        assert!(row.is_some(), "no link row");
        let Some(row) = row else { return };
        PROBES.with(|p| p.set(0));
        for y in 0..rows {
            for x in 0..cols {
                let _ = drawn(&row, &frame, x, y);
                let _ = row.covers(x, y);
            }
        }
        let probes = PROBES.with(core::cell::Cell::get);
        assert!(
            probes <= 8 * cols * rows,
            "{probes} lookups for {} cells",
            cols * rows
        );
    }

    // ADR-1002, section 7, as the maintainer decided on 2026-11-15: labels
    // that a row is too short for lie over each other at its end, and the
    // later one draws on top. Once the letters typed hide the later one,
    // the earlier one shows.
    #[test]
    fn a_later_label_draws_over_an_earlier_one_until_it_hides() {
        let mut t = Terminal::new(5, 1, 0);
        for uri in ["x:a", "x:b", "x:c", "x:d"] {
            t.input(format!("\x1b]8;;{uri}\x1b\\a\x1b]8;;\x1b\\").as_bytes());
        }
        let links = t.screen().view_links();
        let hints = links
            .into_iter()
            .zip(["as", "sd", "df", "fg"])
            .map(|(l, s)| (l, s.to_owned()))
            .collect();
        t.screen_mut().set_link_hints(hints);
        let palette = Palette::default();
        let read = |t: &Terminal| {
            let frame = t.screen().frame(&palette, Age::NEVER);
            let row = LinkRow::new(&frame, 5, 1);
            (0..5)
                .map(|x| row.map_or(' ', |r| drawn(&r, &frame, x, 0).0))
                .collect::<String>()
        };
        assert_eq!(
            read(&t),
            "assfg",
            "fg draws over df, and over the end of sd"
        );
        t.screen_mut().show_link_hints(|l| l.starts_with('d'));
        assert_eq!(read(&t), "aaadf", "only df shows");
    }
}
