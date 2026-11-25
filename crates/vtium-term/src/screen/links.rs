//! OSC 8's links on the screen (ADR-1002, sections 1 to 4 and 7): the open
//! link printed cells take, the link store's sweep and strip, the hovered
//! link, and the overlay row a frame carries. Where the labels of link
//! hints and the hint line go is decided in `hints.rs`, which lists its
//! rules.
//!
//! A sweep runs only when an open would pass [`LINKS_BYTES_MAX`], so the
//! store follows the bytes (docs/architecture.md, "Determinism"), and at
//! most once per quarter of the bound of links opened since the last; an
//! open that comes sooner and does not fit is refused. It reads
//! every line's link table on both screens and in the scrollback, the
//! hovered link and the labelled links, and frees each entry nothing names. When that leaves
//! less than a quarter of the bound free, the scrollback's oldest lines
//! lose their links first until a quarter is. When the screens alone hold
//! more than the bound, the open is refused and the text prints unlinked.
//! The entries kept are renumbered, and every line's table with them.

use super::Screen;
use crate::bounds::LINKS_BYTES_MAX;
use crate::frame::LinkHint;
use crate::line::{Line, LinkId};
use crate::links::{self, LinkStore, Osc8};

/// Where the pointer is, and what it hovers; and the labels of link hints
/// and the note on the overlay row, which vtium sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Hover {
    /// The view's cell under the pointer, a column and a row.
    pointer: Option<(usize, usize)>,
    /// Ctrl is held: the hovered link's URI shows on an overlay row.
    show_uri: bool,
    pub(super) link: Option<LinkId>,
    /// The view row the overlay covers.
    pub(super) overlay: Option<usize>,
    /// The labels of link hints, each placed where its link first appears
    /// in the view, in the order they lie in the view: row by row from the
    /// top, and in each row from left to right.
    pub(super) hints: Vec<LinkHint>,
    /// The links that have a label, sorted, so that drawing a cell finds
    /// out quickly whether its link has one.
    pub(super) labelled: Vec<LinkId>,
    /// What vtium shows on the overlay row in place of a URI, such as the
    /// line that tells how to pick a link hint.
    note: Option<String>,
}

/// The index of `link` in a list by `LinkId`.
pub(super) fn slot(link: LinkId) -> usize {
    usize::try_from(link.get()).unwrap_or(usize::MAX)
}

impl Screen {
    /// OSC 8, `params ; URI` as the string after `8;` carries it: opens the
    /// link printed text takes, or closes it (ADR-1002, "What a link may
    /// hold"). A link the store holds already, by the same id and URI, is
    /// taken again; a new one is added, sweeping the store when it would
    /// pass [`LINKS_BYTES_MAX`].
    pub fn set_hyperlink(&mut self, args: &[u8]) {
        self.link = None;
        let Osc8::Open { id, uri } = links::parse(args) else {
            return;
        };
        if let Some(found) = self.links.find(id, uri) {
            self.link = Some(found);
            return;
        }
        let cost = LinkStore::cost(id, uri);
        self.links.opened(cost);
        if !self.links.fits(cost) && self.links.may_sweep() {
            self.sweep(cost);
        }
        if self.links.fits(cost) {
            self.link = self.links.insert(id, uri);
        }
    }

    /// Frees the links no line names, stripping the scrollback's oldest
    /// lines when that leaves less than a quarter of the bound free with
    /// `cost` more, and renumbers the rest. The hover is found again
    /// first, so a link the stream erased under the pointer keeps nothing
    /// alive, however the bytes were cut.
    fn sweep(&mut self, cost: usize) {
        self.refresh_hover();
        let mut refs = self.count_refs();
        let stripped = self.strip_oldest(&mut refs, cost);
        self.renumber(&refs);
        if stripped && self.scrollback.offset() != 0 {
            self.damage.mark_all();
        }
        // A stripped line may hold the hovered or a labelled link, whose
        // cells lose their underline; a frame can capture that line above
        // the view, where it keeps no damage of its own.
        #[cfg(feature = "rows-beyond")]
        if stripped && (self.hover.link.is_some() || !self.hover.hints.is_empty()) {
            self.damage.mark_all();
        }
    }

    /// How many lines name each link, by `LinkId`, the hovered link and
    /// each labelled link counted once more, so a sweep never frees a link
    /// while its label shows.
    fn count_refs(&self) -> Vec<u32> {
        let mut refs = vec![0u32; self.links.len()];
        let lines = self.main.iter().chain(&self.alt);
        let tables = lines
            .chain(self.scrollback.all_rows())
            .map(Line::link_table);
        let hinted = self.hover.hints.iter().map(|h| h.link);
        for link in tables.flatten().chain(self.hover.link).chain(hinted) {
            if let Some(r) = refs.get_mut(slot(link)) {
                *r = r.saturating_add(1);
            }
        }
        refs
    }

    /// Strips the scrollback's oldest lines of their links, counting each
    /// down in `refs`, until a quarter of the bound would be free with
    /// `cost` more. Returns whether any line lost a link.
    fn strip_oldest(&mut self, refs: &mut [u32], cost: usize) -> bool {
        let store = &self.links;
        let mut live = (0..refs.len())
            .filter(|&i| refs.get(i).is_some_and(|&r| r > 0))
            .filter_map(|i| u32::try_from(i).ok())
            .map(|i| store.cost_of(LinkId::new(i)))
            .fold(0, usize::saturating_add);
        let free = |live: usize| LINKS_BYTES_MAX.saturating_sub(live.saturating_add(cost));
        let mut stripped = false;
        self.scrollback.rows_mut_while(|line| {
            if free(live) >= LINKS_BYTES_MAX / 4 {
                return false;
            }
            for link in line.link_table() {
                if let Some(r) = refs.get_mut(slot(link)) {
                    *r = r.saturating_sub(1);
                    if *r == 0 {
                        live = live.saturating_sub(store.cost_of(link));
                    }
                }
            }
            stripped |= line.strip_links();
            true
        });
        stripped
    }

    /// Keeps the links `refs` counts, renumbered, and renames them in
    /// every line's table, in the hover and in the labels of link hints.
    fn renumber(&mut self, refs: &[u32]) {
        let keep: Vec<bool> = refs.iter().map(|&r| r > 0).collect();
        let map = self.links.retain(&keep);
        let rename = |link: LinkId| map.get(slot(link)).copied().flatten();
        for line in self.main.iter_mut().chain(&mut self.alt) {
            line.rename_links(rename);
        }
        self.scrollback.rows_mut_while(|line| {
            line.rename_links(rename);
            true
        });
        self.hover.link = self.hover.link.and_then(rename);
        for hint in &mut self.hover.hints {
            // the count named each labelled link, so the map keeps it
            if let Some(link) = rename(hint.link) {
                hint.link = link;
            }
        }
        for link in &mut self.hover.labelled {
            if let Some(renamed) = rename(*link) {
                *link = renamed;
            }
        }
        // the store keeps its entries in order, so the renamed ids stay
        // sorted; sorting again costs little and keeps the lookup right
        self.hover.labelled.sort_unstable();
    }

    /// Ends the open link, as RIS and a switch of screens do.
    pub(crate) fn close_link(&mut self) {
        self.link = None;
    }

    /// The URI of `link`.
    pub fn link_uri(&self, link: LinkId) -> Option<&str> {
        self.links.get(link).map(|(_, uri)| uri)
    }

    /// The id parameter `link` was opened with, if it had one.
    pub fn link_id_param(&self, link: LinkId) -> Option<&str> {
        self.links.get(link).and_then(|(id, _)| id)
    }

    /// The bytes the link store charges for the links it holds, at most
    /// [`LINKS_BYTES_MAX`].
    pub fn link_store_bytes(&self) -> usize {
        self.links.charged()
    }

    /// The bytes the link store holds on the heap, by capacity.
    pub fn link_store_heap(&self) -> usize {
        self.links.heap_bytes()
    }

    /// The link of the cell drawn at column `x` of view row `y`, if it has
    /// one: on a line of double size, the doubled character drawn there,
    /// and in a row drawn in another order than it was written, the cell
    /// of the logical column shown there (ADR-1004 D10). The id is valid
    /// until the next [`Terminal::input`](crate::Terminal::input): a sweep
    /// of the link store renumbers the links it keeps.
    pub fn link_at(&self, x: usize, y: usize) -> Option<LinkId> {
        if x >= self.cols {
            return None;
        }
        let x = self.character_col(x, y);
        let line = self.view_line(y)?;
        line.link(line.cell(x))
    }

    /// Sets the view's cell under the pointer, a column and a row, or none
    /// when the pointer is off the screen, and whether Ctrl is held, which
    /// shows the hovered link's URI on an overlay row: the bottom one, or
    /// the top one while the pointer is on the bottom one (ADR-1002,
    /// "Drawing"). A change of the hovered link marks the view rows whose
    /// link tables hold the old or the new link, and the overlay marks its
    /// row as it appears, moves or goes.
    pub fn set_pointer(&mut self, at: Option<(usize, usize)>, show_uri: bool) {
        self.hover.pointer = at;
        self.hover.show_uri = show_uri;
        self.refresh_hover();
    }

    /// The link under the pointer, valid until the next input, as
    /// [`link_at`](Self::link_at)'s is.
    pub fn hovered_link(&self) -> Option<LinkId> {
        self.hover.link
    }

    /// Finds the link under the pointer again, after the stream or the view
    /// changed what is there.
    pub(crate) fn refresh_hover(&mut self) {
        // The labels are placed first, since where they lie decides the
        // row of the hint line.
        self.refresh_link_hints();
        let pointer = self.hover.pointer;
        let link = pointer.and_then(|(x, y)| self.link_at(x, y));
        let last = self.rows.saturating_sub(1);
        // vtium's note, while one is set, shows in place of a URI, so its
        // own rule picks the row
        let overlay = if self.hover.note.is_some() {
            self.note_row()
        } else {
            pointer
                .filter(|_| self.hover.show_uri && link.is_some())
                .map(|(_, y)| if y == last && last > 0 { 0 } else { last })
        };
        let changed = link != self.hover.link;
        if changed {
            self.mark_link_rows(self.hover.link);
            self.mark_link_rows(link);
            self.hover.link = link;
        }
        // The overlay shows the hovered link's URI, so a new link redraws
        // its row even where the row stays.
        if changed || overlay != self.hover.overlay {
            for row in [self.hover.overlay, overlay].into_iter().flatten() {
                self.damage.mark_rows(row..row.saturating_add(1));
            }
            self.hover.overlay = overlay;
        }
    }

    /// Marks the view rows whose link tables hold `link`, and everything
    /// when a row a frame captures beyond the view holds it.
    fn mark_link_rows(&mut self, link: Option<LinkId>) {
        let Some(link) = link else {
            return;
        };
        let rows: Vec<usize> = self
            .view()
            .iter()
            .enumerate()
            .filter(|(_, line)| line.link_table().any(|l| l == link))
            .map(|(row, _)| row)
            .collect();
        for row in rows {
            self.damage.mark_rows(row..row.saturating_add(1));
        }
        #[cfg(feature = "rows-beyond")]
        self.mark_all_if_beyond(|line| line.link_table().any(|l| l == link));
    }

    /// The text the overlay row shows: vtium's note while one is set, else
    /// the hovered link's URI.
    pub(super) fn overlay_text(&self) -> Option<&str> {
        self.hover
            .note
            .as_deref()
            .or_else(|| self.hover.link.and_then(|l| self.link_uri(l)))
    }

    /// The row vtium's note covers while one is set. While link hints are
    /// on, the note is the hint line, and [`hint_line_row`] picks its row,
    /// or none. While they are off, the note shows on the bottom row, or on
    /// the top row while the pointer is on the bottom row, as the URI's
    /// overlay row does.
    ///
    /// [`hint_line_row`]: super::hints::hint_line_row
    ///
    /// References:
    /// - ADR-1002, section 4: the overlay row and the pointer.
    fn note_row(&self) -> Option<usize> {
        self.hover.note.as_ref()?;
        let last = self.rows.saturating_sub(1);
        if self.hover.hints.is_empty() {
            let on_last = self.hover.pointer.is_some_and(|(_, y)| y == last);
            return Some(if on_last && last > 0 { 0 } else { last });
        }
        let hints = &self.hover.hints;
        super::hints::hint_line_row(self.rows, |row| hints.iter().any(|h| h.at.1 == row))
    }

    /// Shows `note` on the overlay row in place of a URI, or nothing of
    /// vtium's own with `None`. vtium shows the line that tells how to pick
    /// a link hint, and what a link hint copied. The note is printable
    /// ASCII, as a URI is. The overlay's row is marked when the note
    /// changes.
    pub fn set_overlay_note(&mut self, note: Option<String>) {
        if note == self.hover.note {
            return;
        }
        if let Some(row) = self.hover.overlay {
            self.damage.mark_rows(row..row.saturating_add(1));
        }
        self.hover.note = note;
        // the overlay row moves to the note's, or back to the URI's
        self.hover.overlay = None;
        self.refresh_hover();
        if let Some(row) = self.hover.overlay {
            self.damage.mark_rows(row..row.saturating_add(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-1002, "Link storage": a sweep is linear in the lines held and
    // runs at most once per quarter of the bound of new link bytes, so a
    // stream of opens the full screens refuse cannot sweep at each one.
    #[test]
    fn refused_opens_sweep_once_per_quarter_of_the_bound() {
        let mut s = Screen::new(200, 2, 0);
        for i in 0..125 {
            let uri = format!("https://{i:04}.example/{}", "q".repeat(2000));
            s.set_hyperlink(format!(";{uri}").as_bytes());
            s.print('x');
        }
        let before = s.links.sweeps;
        let mut opened = 0usize;
        for i in 0..2000 {
            let uri = format!("x:{i}");
            opened += LinkStore::cost("", &uri);
            s.set_hyperlink(format!(";{uri}").as_bytes());
        }
        let sweeps = s.links.sweeps - before;
        assert!(sweeps >= 1);
        assert!(
            sweeps <= 1 + opened / (LINKS_BYTES_MAX / 4),
            "{sweeps} sweeps for {opened} bytes of opens"
        );
    }

    /// `OSC 8 ; ; uri ST`, then `text`, then the link closed.
    fn linked(uri: &str, text: &str) -> String {
        format!("\x1b]8;;{uri}\x1b\\{text}\x1b]8;;\x1b\\")
    }

    /// The rows of `t`'s view marked since `age`.
    fn marked(t: &crate::Terminal, age: crate::Age, rows: usize) -> Vec<usize> {
        let palette = crate::Palette::default();
        let frame = t.screen().frame(&palette, age);
        (0..rows).filter(|&r| frame.row_changed(r)).collect()
    }

    // ADR-1002, section 7: a sweep counts each labelled link as named, as
    // it counts the hovered link, and renames it with the links it keeps.
    #[test]
    fn a_sweep_counts_labelled_links_and_renames_them() {
        let mut t = crate::Terminal::new(8, 2, 0);
        let bytes = format!("{}\r\n{}", linked("x:gone", "g"), linked("x:kept", "k"));
        t.input(bytes.as_bytes());
        let s = t.screen_mut();
        let links = s.view_links();
        assert_eq!(links.len(), 2);
        let &[gone, kept] = links.as_slice() else {
            return;
        };
        s.set_link_hints(vec![(kept, "s".to_owned())]);
        let refs = s.count_refs();
        assert_eq!(refs.get(slot(kept)), Some(&2), "its line and its label");
        assert_eq!(refs.get(slot(gone)), Some(&1));
        // the first link's row scrolls away with no scrollback to keep it,
        // so a sweep frees it
        t.input(b"\r\n");
        let s = t.screen_mut();
        let refs = s.count_refs();
        assert_eq!(refs.get(slot(gone)), Some(&0));
        s.renumber(&refs);
        assert_ne!(
            s.link_hints().first().map(|h| h.link),
            Some(kept),
            "renumbered"
        );
        let labelled: Vec<LinkId> = s.link_hints().iter().map(|h| h.link).collect();
        let uris: Vec<_> = labelled.iter().map(|&l| s.link_uri(l)).collect();
        assert_eq!(uris, [Some("x:kept")]);
        assert_eq!(s.view_links(), labelled);
    }

    // ADR-1002, section 7: vtium's note shows on the overlay row in place
    // of a URI, and its row is marked as it appears, changes and goes.
    #[test]
    fn the_note_takes_the_overlay_row_and_marks_it() {
        let mut t = crate::Terminal::new(8, 3, 0);
        let palette = crate::Palette::default();
        let age = t.screen_mut().checkpoint();
        t.screen_mut().set_overlay_note(Some("pick".to_owned()));
        let s = t.screen();
        let overlay = s.frame(&palette, age).overlay();
        assert_eq!(overlay.map(|o| (o.row, o.uri)), Some((2, "pick")));
        assert_eq!(marked(&t, age, 3), [2]);
        let age = t.screen_mut().checkpoint();
        t.screen_mut().set_overlay_note(Some("done".to_owned()));
        assert_eq!(marked(&t, age, 3), [2], "a new note on the same row");
        t.screen_mut().set_pointer(Some((0, 2)), false);
        let overlay = t.screen().frame(&palette, age).overlay();
        assert_eq!(
            overlay.map(|o| o.row),
            Some(0),
            "the top row, with the pointer at the bottom"
        );
        let age = t.screen_mut().checkpoint();
        t.screen_mut().set_overlay_note(None);
        assert_eq!(t.screen().frame(&palette, age).overlay(), None);
        assert_eq!(marked(&t, age, 3), [0]);
    }
}
