//! Link hints: a short label of letters on each link in the view, which
//! a person types to copy that link's address, and the hint line, the help
//! line on the overlay row that tells how to pick a label. The `vtium`
//! crate starts and ends hints, chooses the labels' letters and reads the
//! keys typed. This module decides where each label and the hint line go,
//! and vtium-render draws them.
//!
//! Four rules decide where things go. Each rule lives in one function, so
//! changing a rule means changing that function and the tests named here.
//!
//! 1. **Where a label goes.** A label starts on the first cell where its
//!    link appears in the view, reading the rows from the top and each row
//!    from left to right as it is drawn. [`Screen::first_cells`] finds
//!    those cells. Test: `labels_sit_where_their_links_first_appear`.
//! 2. **Labels never overlap.** A label that would cover part of the label
//!    before it on the row shifts right just far enough to clear it.
//!    [`clear_the_label_before`] does this. Test:
//!    `a_label_shifts_right_to_clear_the_label_before_it`.
//! 3. **The row's end.** When labels would pass the row's end, the last
//!    label ends at the row's last column, and each label before it moves
//!    left just far enough to clear the one after it. A row whose labels
//!    together are longer than the row cannot hold them all: there, each
//!    label that would pass the end ends at the last column, and a later
//!    label draws over an earlier one. [`fit_the_row_end`] does this.
//!    Tests: `labels_at_the_row_end_move_left_to_clear_each_other` and
//!    `labels_longer_than_their_row_pile_up_at_its_end`.
//! 4. **Where the hint line goes.** The hint line sits on the bottom row.
//!    It moves to the top row while a label lies on the bottom row, and it
//!    does not show while labels lie on both the top and the bottom rows,
//!    so it never hides a label. A screen of one row counts as both. The
//!    pointer does not move it. [`hint_line_row`] does this. Tests:
//!    `the_hint_line_leaves_the_bottom_row_while_a_label_lies_there` and
//!    `no_hint_line_shows_while_labels_lie_on_the_top_and_bottom_rows`.
//!
//! [`Screen::place_link_hints`] applies rules 1 to 3 after every input,
//! so each label follows its link. It finds each link's first cell with
//! one lookup per linked cell, which keeps a screen full of links cheap.
//!
//! References:
//! - ADR-1002, section 7 ("Keyboard link hints"): the four rules, which
//!   the maintainer decided on 2026-11-15, and how hints start, end and
//!   copy a link.

use super::Screen;
use super::links::slot;
use crate::frame::LinkHint;
use crate::line::LinkId;

#[cfg(test)]
std::thread_local! {
    /// The entries that placing the labels of link hints has looked at on
    /// this thread, which a test counts to check that the work grows with
    /// the cells and the labels, not with their product.
    static PROBES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Counts one entry looked at while placing labels, in a test build.
fn probe() {
    #[cfg(test)]
    PROBES.with(|p| p.set(p.get().saturating_add(1)));
}

impl Screen {
    /// The distinct links of the view, in the order each first appears:
    /// row by row from the top, and in each row from left to right as the
    /// row is drawn. Link hints label them in this order.
    pub fn view_links(&self) -> Vec<LinkId> {
        self.first_cells()
            .into_iter()
            .map(|(link, _)| link)
            .collect()
    }

    /// Rule 1, where a label goes: each distinct link of the view, with
    /// the cell where it first appears, a column and a row, in the order
    /// [`view_links`] gives. A label starts on that cell before rules 2 and
    /// 3 move it.
    ///
    /// Each cell's link is found as [`link_at`](Self::link_at) finds it,
    /// with the line, its size and its drawing order looked up once per
    /// row. A list by link slot records the links already found, so each
    /// linked cell costs one lookup. Link hints place their labels again
    /// after every input, so this runs often on a screen full of links.
    ///
    /// [`view_links`]: Self::view_links
    fn first_cells(&self) -> Vec<(LinkId, (usize, usize))> {
        let mut seen = vec![false; self.links.len()];
        let mut found: Vec<(LinkId, (usize, usize))> = Vec::new();
        for (y, line) in self.view().into_iter().enumerate() {
            // a row whose link table is empty holds no link
            if line.link_table().next().is_none() {
                continue;
            }
            let double = line.flags().size().is_double();
            #[cfg(feature = "bidi")]
            let order = self.row_order(y);
            for x in 0..self.cols {
                let col = if double { x / 2 } else { x };
                #[cfg(feature = "bidi")]
                let col = order.as_ref().map_or(col, |r| r.logical_at(col));
                let Some(link) = line.link(line.cell(col)) else {
                    continue;
                };
                probe();
                if let Some(seen) = seen.get_mut(slot(link))
                    && !*seen
                {
                    *seen = true;
                    found.push((link, (x, y)));
                }
            }
        }
        found
    }

    /// Labels `hints`, each a link and its label's letters, every label
    /// shown, in place of the labels the screen held; an empty list ends
    /// link hints. Each label is placed where its link first appears in
    /// the view, and a link not in the view gets none. The rows of every
    /// link that gains or loses a label are marked, since each cell of a
    /// labelled link takes the hover's dashed underline.
    pub fn set_link_hints(&mut self, hints: Vec<(LinkId, String)>) {
        let old = core::mem::take(&mut self.hover.hints);
        for hint in &old {
            self.damage
                .mark_rows(hint.at.1..hint.at.1.saturating_add(1));
        }
        self.hover.hints = hints
            .into_iter()
            .map(|(link, label)| LinkHint {
                link,
                label,
                shown: true,
                at: (0, 0),
            })
            .collect();
        let changed: Vec<LinkId> = old
            .iter()
            .chain(&self.hover.hints)
            .map(|h| h.link)
            .collect();
        self.mark_rows_of_links(changed.into_iter());
        // places the labels, and moves the hint line if a label now lies
        // on its row or no longer does
        self.refresh_hover();
        if self.hover.hints.is_empty() {
            self.hover.labelled.clear();
        }
    }

    /// Marks the view rows whose link tables hold any of `links`, in one
    /// pass over the view, with a list by link slot to look each entry up,
    /// and everything when a row a frame captures beyond the view holds
    /// one of them.
    fn mark_rows_of_links(&mut self, links: impl Iterator<Item = LinkId>) {
        let mut wanted = vec![false; self.links.len()];
        for link in links {
            if let Some(w) = wanted.get_mut(slot(link)) {
                *w = true;
            }
        }
        let rows: Vec<usize> = self
            .view()
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.link_table()
                    .any(|l| wanted.get(slot(l)).copied().unwrap_or(false))
            })
            .map(|(row, _)| row)
            .collect();
        for row in rows {
            self.damage.mark_rows(row..row.saturating_add(1));
        }
        #[cfg(feature = "rows-beyond")]
        self.mark_all_if_beyond(|line| {
            line.link_table()
                .any(|l| wanted.get(slot(l)).copied().unwrap_or(false))
        });
    }

    /// The labels of link hints, while they are on, each placed where its
    /// link now first appears in the view.
    pub fn link_hints(&self) -> &[LinkHint] {
        &self.hover.hints
    }

    /// Shows the labels whose letters `shown` accepts and hides the rest,
    /// marking the row of each label that appears or goes.
    pub fn show_link_hints(&mut self, shown: impl Fn(&str) -> bool) {
        for i in 0..self.hover.hints.len() {
            let Some(hint) = self.hover.hints.get_mut(i) else {
                continue;
            };
            let now = shown(&hint.label);
            if now != hint.shown {
                hint.shown = now;
                let row = hint.at.1;
                self.damage.mark_rows(row..row.saturating_add(1));
            }
        }
    }

    /// Places each label of link hints again, after the stream or the view
    /// changed what is there.
    pub(super) fn refresh_link_hints(&mut self) {
        if !self.hover.hints.is_empty() {
            self.place_link_hints();
        }
    }

    /// Places every label by rules 1 to 3, after each input and each change
    /// of the view: each label starts where [`first_cells`] finds its link
    /// (rule 1), then each row's labels go through
    /// [`clear_the_label_before`] (rule 2) and [`fit_the_row_end`] (rule
    /// 3). The label of a link no longer in the view goes, and keeps no
    /// label from then on. The labels are kept in the order they lie in the
    /// view, and the rows a label leaves or arrives on are marked.
    ///
    /// [`first_cells`]: Self::first_cells
    fn place_link_hints(&mut self) {
        let first = self.first_cells();
        let mut wanted = vec![None; self.links.len()];
        for &(link, at) in &first {
            if let Some(w) = wanted.get_mut(slot(link)) {
                *w = Some(at);
            }
        }
        let cols = self.cols;
        let mut marks = Vec::new();
        // the links whose label goes, whose cells beyond the view lose the
        // dashed underline
        #[cfg(feature = "rows-beyond")]
        let mut gone: Vec<LinkId> = Vec::new();
        let mut kept: Vec<(LinkHint, (usize, usize))> = Vec::new();
        for hint in core::mem::take(&mut self.hover.hints) {
            probe();
            match wanted.get(slot(hint.link)).copied().flatten() {
                Some(at) => kept.push((hint, at)),
                None => {
                    marks.push(hint.at.1);
                    #[cfg(feature = "rows-beyond")]
                    gone.push(hint.link);
                }
            }
        }
        // a stable sort, so labels whose links start in the same cell keep
        // their order
        kept.sort_by_key(|&(_, (x, y))| (y, x));
        let mut row: Vec<(usize, usize)> = Vec::new();
        for same_row in kept.chunk_by_mut(|a, b| a.1.1 == b.1.1) {
            row.clear();
            row.extend(same_row.iter().map(|(h, (x, _))| (*x, h.label.len())));
            clear_the_label_before(&mut row);
            fit_the_row_end(cols, &mut row);
            for ((hint, (_, y)), &(x, _)) in same_row.iter_mut().zip(&row) {
                let at = (x, *y);
                if at != hint.at {
                    marks.extend([hint.at.1, *y]);
                    hint.at = at;
                }
            }
        }
        self.hover.hints = kept.into_iter().map(|(hint, _)| hint).collect();
        let mut labelled: Vec<LinkId> = self.hover.hints.iter().map(|h| h.link).collect();
        labelled.sort_unstable();
        labelled.dedup();
        self.hover.labelled = labelled;
        for row in marks {
            self.damage.mark_rows(row..row.saturating_add(1));
        }
        #[cfg(feature = "rows-beyond")]
        if !gone.is_empty() {
            self.mark_all_if_beyond(|line| line.link_table().any(|l| gone.contains(&l)));
        }
    }

    /// The links that have a label, sorted.
    pub(super) fn labelled_links(&self) -> &[LinkId] {
        &self.hover.labelled
    }
}

/// Rule 2, labels never overlap: shifts each label in `labels`, one row's
/// labels as a column and a length each, in the order their links appear
/// from left to right, right just far enough to clear the label before it.
/// Each column comes in as the cell where rule 1 starts the label.
fn clear_the_label_before(labels: &mut [(usize, usize)]) {
    let mut end = 0usize;
    for (x, len) in labels.iter_mut() {
        *x = (*x).max(end);
        end = x.saturating_add(*len);
    }
}

/// Rule 3, the row's end: keeps the labels of one row of `cols` columns,
/// laid out by [`clear_the_label_before`], inside the row. The last label
/// ends at the row's last column at the latest, and each label before it
/// moves left just far enough to clear the one after it, so every label
/// still reads in full. When the labels together are longer than the row,
/// that cannot fit: each label that would pass the row's end ends at its
/// last column instead, so those labels lie over each other there, and the
/// later one draws on top.
fn fit_the_row_end(cols: usize, labels: &mut [(usize, usize)]) {
    let total = labels
        .iter()
        .map(|&(_, len)| len)
        .fold(0usize, usize::saturating_add);
    if total <= cols {
        let mut limit = cols;
        for (x, len) in labels.iter_mut().rev() {
            *x = (*x).min(limit.saturating_sub(*len));
            limit = *x;
        }
    } else {
        for (x, len) in labels.iter_mut() {
            *x = (*x).min(cols.saturating_sub(*len));
        }
    }
}

/// Rule 4, where the hint line goes, on a screen of `rows` rows while link
/// hints are on, where `label_on` says whether a label lies on a row: the
/// bottom row; the top row while a label lies on the bottom row; and no
/// row while labels lie on both the top and the bottom rows, which a
/// screen of one row does whenever it holds a label.
pub(super) fn hint_line_row(rows: usize, label_on: impl Fn(usize) -> bool) -> Option<usize> {
    let last = rows.saturating_sub(1);
    match (label_on(0), label_on(last)) {
        (true, true) => None,
        (false, true) => Some(0),
        _ => Some(last),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The labels' letters, shown flags and places.
    fn placed(s: &Screen) -> Vec<(&str, bool, (usize, usize))> {
        s.link_hints()
            .iter()
            .map(|h| (h.label.as_str(), h.shown, h.at))
            .collect()
    }

    // ADR-1002, section 7: the view's links in the order each first
    // appears, and each label where its link first appears, moved left to
    // end at the row's last column.
    #[test]
    fn labels_sit_where_their_links_first_appear() {
        let mut t = crate::Terminal::new(8, 4, 0);
        let bytes = format!(
            "ab{}\r\n{}{}\r\n{}",
            linked("x:one", "1"),
            linked("x:two", "2"),
            linked("x:one", "1"),
            linked("x:end", "\x1b[8Gz"),
        );
        t.input(bytes.as_bytes());
        let s = t.screen_mut();
        let links = s.view_links();
        let uris: Vec<_> = links.iter().filter_map(|&l| s.link_uri(l)).collect();
        assert_eq!(uris, ["x:one", "x:two", "x:end"]);
        let hints = links
            .iter()
            .zip(["a", "s", "dd"])
            .map(|(&l, t)| (l, t.to_owned()))
            .collect();
        s.set_link_hints(hints);
        assert_eq!(
            placed(s),
            [
                ("a", true, (2, 0)),
                ("s", true, (0, 1)),
                ("dd", true, (6, 2))
            ]
        );
        let palette = crate::Palette::default();
        assert_eq!(
            s.frame(&palette, crate::Age::NEVER).link_hints(),
            s.link_hints()
        );
    }

    // ADR-1002, section 7: every cell of a labelled link takes the dashed
    // underline, so the rows of each link that gains or loses a label are
    // marked, and the row of a label that shows or hides.
    #[test]
    fn the_rows_of_links_that_gain_or_lose_a_label_are_marked() {
        let mut t = crate::Terminal::new(8, 5, 0);
        let bytes = format!(
            "{}\r\nplain\r\n{}\r\nplain\r\n{}",
            linked("x:a", "a"),
            linked("x:b", "b"),
            linked("x:a", "a"),
        );
        t.input(bytes.as_bytes());
        let age = t.screen_mut().checkpoint();
        let links = t.screen().view_links();
        assert_eq!(links.len(), 2);
        let Some(&a) = links.first() else { return };
        t.screen_mut().set_link_hints(vec![(a, "a".to_owned())]);
        assert_eq!(marked(&t, age, 5), [0, 4], "the link's rows, on gain");
        let age = t.screen_mut().checkpoint();
        t.screen_mut().show_link_hints(|l| l.starts_with('s'));
        assert_eq!(marked(&t, age, 5), [0], "the label's row, as it hides");
        assert_eq!(placed(t.screen()), [("a", false, (0, 0))]);
        let age = t.screen_mut().checkpoint();
        t.screen_mut().set_link_hints(Vec::new());
        assert_eq!(marked(&t, age, 5), [0, 4], "the link's rows, on loss");
        assert!(t.screen().link_hints().is_empty());
    }

    // ADR-1002, section 7: a label follows its link to where the link now
    // first appears, marking both rows, and a link that leaves the view
    // loses its label.
    #[test]
    fn a_label_follows_its_link_and_goes_when_the_link_leaves() {
        let mut t = crate::Terminal::new(8, 4, 0);
        let bytes = format!("{}\r\n\r\n  {}", linked("x:a", "a"), linked("x:a", "a"));
        t.input(bytes.as_bytes());
        let links = t.screen().view_links();
        assert_eq!(links.len(), 1);
        let Some(&a) = links.first() else { return };
        t.screen_mut().set_link_hints(vec![(a, "a".to_owned())]);
        let age = t.screen_mut().checkpoint();
        // erase the first row, where the label sat
        t.input(b"\x1b[H\x1b[2K");
        assert_eq!(placed(t.screen()), [("a", true, (2, 2))]);
        assert!(marked(&t, age, 4).contains(&2), "the row it moved to");
        t.input(b"\x1b[3;1H\x1b[2K");
        assert!(t.screen().link_hints().is_empty());
        t.input(linked("x:a", "a").as_bytes());
        assert!(t.screen().link_hints().is_empty(), "lost for good");
    }

    /// Labels every link in `s`'s view with `labels`, in order.
    fn label_all(s: &mut Screen, labels: &[&str]) {
        let hints = s
            .view_links()
            .into_iter()
            .zip(labels)
            .map(|(l, t)| (l, (*t).to_owned()))
            .collect();
        s.set_link_hints(hints);
    }

    // Every input places the labels again while link hints are on, so the
    // placing looks each linked cell and each label up once: a screen
    // where every cell is its own link costs its cells, not its cells
    // times its links.
    #[test]
    fn placing_labels_looks_each_cell_and_label_up_once() {
        let (cols, rows) = (40, 20);
        let mut t = crate::Terminal::new(cols, rows, 0);
        let mut bytes = String::new();
        for y in 0..rows {
            bytes.push_str(&format!("\x1b[{};1H", y + 1));
            for x in 0..cols {
                bytes.push_str(&linked(&format!("x:{x},{y}"), "a"));
            }
        }
        t.input(bytes.as_bytes());
        let links = t.screen().view_links();
        assert_eq!(links.len(), cols * rows);
        let labels: Vec<String> = (0..links.len()).map(|i| format!("{i:03}")).collect();
        let hints = links.into_iter().zip(labels).collect();
        t.screen_mut().set_link_hints(hints);
        PROBES.with(|p| p.set(0));
        t.input(b"\x1b[1;1H");
        let probes = PROBES.with(core::cell::Cell::get);
        assert!(
            probes <= 2 * cols * rows,
            "{probes} entries looked at for {} cells",
            cols * rows
        );
    }

    // ADR-1002, section 7, as the maintainer decided on 2026-11-15: the
    // hint line stays on the bottom row, and moves to the top row only
    // while a label lies on the bottom row, so no label hides under it.
    #[test]
    fn the_hint_line_leaves_the_bottom_row_while_a_label_lies_there() {
        let mut t = crate::Terminal::new(80, 4, 0);
        t.input(
            format!(
                "\x1b[2;1H{}\x1b[4;1H{}",
                linked("x:top", "top"),
                linked("x:bottom", "bottom")
            )
            .as_bytes(),
        );
        let palette = crate::Palette::default();
        let row = |t: &crate::Terminal| {
            t.screen()
                .frame(&palette, crate::Age::NEVER)
                .overlay()
                .map(|o| o.row)
        };
        label_all(t.screen_mut(), &["a", "s"]);
        t.screen_mut().set_overlay_note(Some("pick".to_owned()));
        assert_eq!(
            placed(t.screen()),
            [("a", true, (0, 1)), ("s", true, (0, 3))]
        );
        assert_eq!(row(&t), Some(0), "a label lies on the bottom row");
        let age = t.screen_mut().checkpoint();
        // the bottom row's link scrolls up a row, so no label lies there
        t.input(b"\r\n");
        assert_eq!(
            placed(t.screen()),
            [("a", true, (0, 0)), ("s", true, (0, 2))]
        );
        assert_eq!(row(&t), Some(3), "back on the bottom row");
        assert!(marked(&t, age, 4).contains(&0) && marked(&t, age, 4).contains(&3));
        t.screen_mut().set_pointer(Some((5, 3)), false);
        assert_eq!(row(&t), Some(3), "the pointer does not move the hint line");
        // a link that arrives after hints start gets no label, and Ctrl
        // over it on the bottom row leaves the hint line there too
        t.input(format!("\x1b[4;1H{}", linked("x:new", "new")).as_bytes());
        t.screen_mut().set_pointer(Some((1, 3)), true);
        assert_eq!(
            t.screen()
                .hovered_link()
                .and_then(|l| t.screen().link_uri(l)),
            Some("x:new")
        );
        assert_eq!(
            row(&t),
            Some(3),
            "Ctrl over a link does not move the hint line"
        );
    }

    // ADR-1002, section 7, as the maintainer decided on 2026-11-15: while
    // labels lie on both the top and the bottom rows, vtium shows no hint
    // line, so it hides no label. A screen of one row is both.
    #[test]
    fn no_hint_line_shows_while_labels_lie_on_the_top_and_bottom_rows() {
        let mut t = crate::Terminal::new(80, 4, 0);
        let bytes = format!(
            "{}\x1b[4;1H{}",
            linked("x:top", "top"),
            linked("x:bottom", "bottom")
        );
        t.input(bytes.as_bytes());
        let palette = crate::Palette::default();
        let row = |t: &crate::Terminal| {
            t.screen()
                .frame(&palette, crate::Age::NEVER)
                .overlay()
                .map(|o| o.row)
        };
        label_all(t.screen_mut(), &["a", "s"]);
        t.screen_mut().set_overlay_note(Some("pick".to_owned()));
        assert_eq!(
            placed(t.screen()),
            [("a", true, (0, 0)), ("s", true, (0, 3))]
        );
        assert_eq!(row(&t), None, "labels on both rows");
        let age = t.screen_mut().checkpoint();
        // the top row's link scrolls away, so the bottom row is free
        t.input(b"\r\n");
        assert_eq!(placed(t.screen()), [("s", true, (0, 2))]);
        assert_eq!(row(&t), Some(3), "the hint line returns to the bottom row");
        assert!(marked(&t, age, 4).contains(&3));

        let mut t = crate::Terminal::new(8, 1, 0);
        t.input(linked("x:a", "abc").as_bytes());
        label_all(t.screen_mut(), &["a"]);
        t.screen_mut().set_overlay_note(Some("pick".to_owned()));
        assert_eq!(row(&t), None, "one row with a label");
    }

    // ADR-1002, section 7, as the maintainer decided on 2026-11-15: a
    // label that would cover part of the one before it shifts right just
    // far enough to clear it, so both read in full.
    #[test]
    fn a_label_shifts_right_to_clear_the_label_before_it() {
        let mut t = crate::Terminal::new(10, 1, 0);
        t.input(
            format!(
                "{}{}{}",
                linked("x:a", "a"),
                linked("x:b", "b"),
                linked("x:c", "cccc")
            )
            .as_bytes(),
        );
        label_all(t.screen_mut(), &["as", "sd", "df"]);
        assert_eq!(
            placed(t.screen()),
            [
                ("as", true, (0, 0)),
                ("sd", true, (2, 0)),
                ("df", true, (4, 0))
            ]
        );
    }

    // ADR-1002, section 7, as the maintainer decided on 2026-11-15: where
    // shifting right would run past the row's end, the last label ends at
    // the row's last column and the labels before it move left to clear
    // it, so every label still reads in full.
    #[test]
    fn labels_at_the_row_end_move_left_to_clear_each_other() {
        let mut t = crate::Terminal::new(8, 1, 0);
        t.input(
            format!(
                "abcde{}{}{}",
                linked("x:a", "a"),
                linked("x:b", "b"),
                linked("x:c", "c")
            )
            .as_bytes(),
        );
        label_all(t.screen_mut(), &["as", "sd", "df"]);
        assert_eq!(
            placed(t.screen()),
            [
                ("as", true, (2, 0)),
                ("sd", true, (4, 0)),
                ("df", true, (6, 0))
            ]
        );
    }

    // ADR-1002, section 7, as the maintainer decided on 2026-11-15: when a
    // row's labels together are longer than the row, the labels that run
    // past its end each end at its last column, so they lie over each
    // other there, and the later one draws on top.
    #[test]
    fn labels_longer_than_their_row_pile_up_at_its_end() {
        let mut t = crate::Terminal::new(5, 1, 0);
        t.input(
            format!(
                "{}{}{}{}",
                linked("x:a", "a"),
                linked("x:b", "b"),
                linked("x:c", "c"),
                linked("x:d", "d")
            )
            .as_bytes(),
        );
        label_all(t.screen_mut(), &["as", "sd", "df", "fg"]);
        assert_eq!(
            placed(t.screen()),
            [
                ("as", true, (0, 0)),
                ("sd", true, (2, 0)),
                ("df", true, (3, 0)),
                ("fg", true, (3, 0))
            ]
        );
    }
}
