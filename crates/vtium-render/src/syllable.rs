//! The syllables a row draws joined across their clusters, and the
//! programming ligatures it draws across their cells.
//!
//! Unicode's grapheme cluster rules cut many syllables into several
//! clusters, as they cut every Kannada conjunct: ಕ್ರ is ಕ್ in one cell and ರ
//! in the next. The terminal keeps those clusters, so the cursor and what
//! programs count do not change. Only the drawing joins. [`Syllable::find`]
//! reads a row of the frame for the run of cells around a cell whose
//! clusters join ([`vtium_font::syllable::joins`]). The cells must stand
//! side by side on the row in the order they were written, left to right.
//! A run whose text holds more than [`SYLLABLE_SCALARS_MAX`] scalar values
//! is no syllable, and its cells draw on their own.
//!
//! With ligatures on, `font-ligatures` in vtium.conf, a run of 2 to 14
//! cells side by side that each hold one ASCII symbol is found the same
//! way ([`vtium_font::syllable::is_ligature_symbol`]), so that `->` or
//! `!=` draws as the font's ligature across its cells. Such a run is a
//! drawing only: the terminal keeps each character in its own cell. The
//! cells must be written left to right on a line of normal size, and none
//! of them may belong to a sized run, the block of cells that one OSC 66
//! sequence writes. So no ligature draws backwards in right-to-left text,
//! on a double-size line, or across the cells of a sized run. The rest of
//! this module treats a ligature run as a syllable, and the helper answers
//! that a run the font does not join has no pieces.
//!
//! The console asks the glyph helper for each piece of a syllable: its
//! whole text, followed by a mark that says where one cluster's cells lie
//! among the syllable's ([`Syllable::key`]). The helper shapes the text
//! once, draws it across all the syllable's cells, and cuts the drawing at
//! the clusters' edges. Each cell then draws its own piece with its own
//! background and decorations. [`Syllable::joined`] says when the pieces
//! draw: never while the visible cursor stands on any cell of the
//! syllable, while the selection covers some of its cells and not the
//! others, while its cells differ in foreground color, in style or in
//! script, or while a link's overlay covers one of them. The renderer also
//! draws the syllable split while its font lacks any piece.
//!
//! [`Runs`] keeps the runs found on one row, so that the renderer walks
//! each run once however many times it reads the run's cells.
//!
//! References:
//! - ADR-1203 D1: which clusters join into a syllable.
//! - ADR-1203 D3: the pieces and their keys.
//! - ADR-1203 D4: when a syllable draws split.
//! - ADR-1308 D8: the programming ligatures, which cells they join, and
//!   when they draw apart.

use core::ops::RangeInclusive;

use vtium_font::Variant;
use vtium_font::syllable::{
    Piece, SYLLABLE_SCALARS_MAX, is_ligature_symbol, joins, may_follow, may_link,
};
use vtium_term::{CLUSTER_MAX, Frame, FrameCell, Glyph, Line, LineSize, Rgb, Width};

/// The most clusters a syllable holds: each holds one scalar value at
/// least.
const CLUSTERS_MAX: usize = SYLLABLE_SCALARS_MAX;

/// One cluster of a syllable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Member {
    /// Its first column, as drawn.
    col: usize,
    /// Its cells, 1 or 2.
    cells: u8,
    fg: Rgb,
    variant: Variant,
    /// Whether the selection covers it.
    selected: bool,
}

/// A run of cells whose clusters join into one syllable, on one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Syllable {
    members: [Member; CLUSTERS_MAX],
    len: usize,
    text: [char; SYLLABLE_SCALARS_MAX],
    text_len: usize,
}

/// What a walk from one cell along its row finds.
#[expect(
    clippy::large_enum_variant,
    reason = "a walk's result moves once, into the row's runs, and a box would allocate for each run"
)]
enum Walk {
    /// The cell joins no neighbour.
    Alone,
    /// The cell's run holds more than [`SYLLABLE_SCALARS_MAX`] scalar
    /// values, so none of its cells draws joined. The range holds the
    /// columns walked, all of them cells of the run.
    Long(RangeInclusive<usize>),
    /// The cell's syllable, and the cell's place among its clusters.
    Run(Syllable, usize),
}

/// The characters of a cell's glyph.
fn chars(glyph: Glyph<'_>) -> impl Iterator<Item = char> + '_ {
    let (one, many) = match glyph {
        Glyph::Empty => (None, None),
        Glyph::Char(c) => (Some(c), None),
        Glyph::Cluster(s) => (None, Some(s.chars())),
    };
    one.into_iter().chain(many.into_iter().flatten())
}

/// The cells a leading cell spans: 2 for the first half of a wide
/// character, else 1.
fn cells(fc: &FrameCell<'_>) -> u8 {
    if fc.width == Width::Wide { 2 } else { 1 }
}

/// Whether `fc`, a cell of `line`, is one narrow cell holding one ASCII
/// symbol and no cell of a sized run, so that a ligature may join it.
///
/// References:
/// - ADR-1308 D8: which cells a ligature joins, and that the cells of a
///   sized run never join into one.
fn symbol(line: &Line, fc: &FrameCell<'_>) -> bool {
    fc.width == Width::Narrow
        && matches!(fc.glyph, Glyph::Char(c) if is_ligature_symbol(c))
        && !line.cell(fc.col).in_run()
}

/// Whether `left`, the cell drawn at column `col`, and `right`, the cell
/// drawn just after it, are clusters written one after the other, left to
/// right, whose clusters join. With `symbols`, the line the cells belong
/// to when ligatures may join them, two ASCII symbols link too.
///
/// References:
/// - ADR-1203 D1: which clusters join into a syllable.
/// - ADR-1308 D8: which cells a ligature joins.
fn linked(left: &FrameCell<'_>, right: &FrameCell<'_>, symbols: Option<&Line>) -> bool {
    if right.width == Width::WideTail || left.rtl || right.rtl {
        return false;
    }
    if left.col.checked_add(usize::from(cells(left))) != Some(right.col) {
        return false;
    }
    if symbols.is_some_and(|line| symbol(line, left) && symbol(line, right)) {
        return true;
    }
    let Some(head) = chars(right.glyph).next() else {
        return false;
    };
    // the whole left cluster, since a consonant must come before its
    // linker (rule GB9c of UAX #29); the terminal keeps at most
    // CLUSTER_MAX scalar values in a cluster
    let mut text = ['\0'; CLUSTER_MAX];
    let mut len: usize = 0;
    for (slot, c) in text.iter_mut().zip(chars(left.glyph)) {
        *slot = c;
        len = len.saturating_add(1);
    }
    joins(text.get(..len).unwrap_or_default(), &[head])
}

impl Syllable {
    /// The syllable the cell `fc`, drawn at `col` of `row`, belongs to,
    /// and its place among the syllable's clusters: the longest run of
    /// cells on the row around it whose clusters join, or, with
    /// `ligatures` on a line of normal size, that are ASCII symbols.
    /// `None` when no neighbour joins it, when it is the second half of a
    /// wide character, and when the run's text holds more than
    /// [`SYLLABLE_SCALARS_MAX`] scalar values.
    ///
    /// References:
    /// - ADR-1203 D1: which clusters join into a syllable.
    /// - ADR-1308 D8: which cells a ligature joins.
    pub fn find(
        frame: &Frame<'_>,
        at: (usize, usize),
        fc: &FrameCell<'_>,
        ligatures: bool,
    ) -> Option<(Syllable, usize)> {
        match Syllable::walk(frame, at, fc, ligatures, false) {
            Walk::Run(s, index) => Some((s, index)),
            Walk::Alone | Walk::Long(_) => None,
        }
    }

    /// Walks the row from `fc` in both directions for the cells its
    /// cluster joins. A walk past [`SYLLABLE_SCALARS_MAX`] scalar values
    /// stops there, unless `whole` asks it to go on to both ends of the
    /// run, so that [`Runs`] learns every column of a long run at once.
    fn walk(
        frame: &Frame<'_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
        ligatures: bool,
        whole: bool,
    ) -> Walk {
        if fc.width == Width::WideTail {
            return Walk::Alone;
        }
        // cells of a double-size line never join into a ligature
        let symbols = frame
            .line(row)
            .filter(|_| ligatures && frame.size(row) == LineSize::Single);
        // a quick answer for most cells: one that neither ends with a
        // linker, nor starts with a letter that follows one, nor is a
        // symbol a ligature may join, joins nothing
        let (Some(first), Some(last)) = (chars(fc.glyph).next(), chars(fc.glyph).last()) else {
            return Walk::Alone;
        };
        if !may_link(last) && !may_follow(first) && !symbols.is_some_and(|l| symbol(l, fc)) {
            return Walk::Alone;
        }
        let member = |col: usize, fc: &FrameCell<'_>| Member {
            col,
            cells: cells(fc),
            fg: fc.fg,
            variant: crate::variant::of(fc.attrs),
            selected: fc.selected,
        };
        let mut scalars = chars(fc.glyph).count();
        let (mut lo, mut hi) = (
            col,
            col.saturating_add(usize::from(cells(fc)).saturating_sub(1)),
        );
        // the clusters before the cell, nearest first
        let mut before: [Option<(usize, FrameCell<'_>)>; CLUSTERS_MAX] = [None; CLUSTERS_MAX];
        let mut len_before = 0usize;
        let (mut at, mut here) = (col, *fc);
        while let Some(left_col) = at.checked_sub(1) {
            let Some(mut left) = frame.cell(row, left_col) else {
                return Walk::Alone;
            };
            let mut lead = left_col;
            if left.width == Width::WideTail {
                let Some(cell) = left_col.checked_sub(1).and_then(|c| frame.cell(row, c)) else {
                    return Walk::Alone;
                };
                (lead, left) = (left_col.saturating_sub(1), cell);
            }
            if !linked(&left, &here, symbols) {
                break;
            }
            scalars = scalars.saturating_add(chars(left.glyph).count());
            lo = lead;
            match before.get_mut(len_before) {
                Some(slot) if scalars <= SYLLABLE_SCALARS_MAX => {
                    *slot = Some((lead, left));
                    len_before = len_before.saturating_add(1);
                }
                _ if !whole => return Walk::Long(lo..=hi),
                _ => {}
            }
            (at, here) = (lead, left);
        }
        let mut s = Syllable {
            members: [Member::default(); CLUSTERS_MAX],
            len: 0,
            text: ['\0'; SYLLABLE_SCALARS_MAX],
            text_len: 0,
        };
        if scalars <= SYLLABLE_SCALARS_MAX {
            let own = Some((col, *fc));
            let run = before.get(..len_before).unwrap_or_default().iter().rev();
            for (c, cell) in run.chain(core::iter::once(&own)).flatten() {
                s.push(member(*c, cell), cell.glyph);
            }
        }
        // the clusters after it
        let (mut at, mut here) = (col, *fc);
        loop {
            let Some(next) = at.checked_add(usize::from(cells(&here))) else {
                return Walk::Alone;
            };
            let Some(right) = frame.cell(row, next) else {
                break;
            };
            if !linked(&here, &right, symbols) {
                break;
            }
            scalars = scalars.saturating_add(chars(right.glyph).count());
            hi = next.saturating_add(usize::from(cells(&right)).saturating_sub(1));
            if scalars <= SYLLABLE_SCALARS_MAX {
                s.push(member(next, &right), right.glyph);
            } else if !whole {
                return Walk::Long(lo..=hi);
            }
            (at, here) = (next, right);
        }
        if scalars > SYLLABLE_SCALARS_MAX {
            return Walk::Long(lo..=hi);
        }
        if s.len < 2 {
            return Walk::Alone;
        }
        Walk::Run(s, len_before)
    }

    /// Adds the cluster `m`, which draws `glyph`, at the end of the
    /// syllable. The walk adds no more than [`SYLLABLE_SCALARS_MAX`]
    /// scalar values, so both fit.
    fn push(&mut self, m: Member, glyph: Glyph<'_>) {
        if let Some(slot) = self.members.get_mut(self.len) {
            *slot = m;
            self.len = self.len.saturating_add(1);
        }
        for c in chars(glyph) {
            if let Some(slot) = self.text.get_mut(self.text_len) {
                *slot = c;
                self.text_len = self.text_len.saturating_add(1);
            }
        }
    }

    /// The number of clusters the syllable joins.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the syllable joins no cluster, which [`Syllable::find`]
    /// never gives.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The column of each cell of the syllable, as drawn.
    pub fn cols(&self) -> impl Iterator<Item = usize> + '_ {
        let members = self.members.get(..self.len).unwrap_or_default();
        members
            .iter()
            .flat_map(|m| m.col..m.col.saturating_add(usize::from(m.cells)))
    }

    /// The syllable's text, its clusters' characters in written order.
    pub fn text(&self) -> &[char] {
        self.text.get(..self.text_len).unwrap_or_default()
    }

    /// The variant of its first cell, in which the console asks for its
    /// pieces.
    ///
    /// References:
    /// - ADR-1203 D3: the variant a syllable's pieces are asked in.
    pub fn variant(&self) -> Variant {
        self.members.first().map(|m| m.variant).unwrap_or_default()
    }

    /// Where the cells of the cluster at `index` lie among the syllable's.
    pub fn piece(&self, index: usize) -> Option<Piece> {
        let members = self.members.get(..self.len)?;
        let cells = |ms: &[Member]| ms.iter().map(|m| u32::from(m.cells)).sum::<u32>();
        let start = u8::try_from(cells(members.get(..index)?)).ok()?;
        let total = u8::try_from(cells(members)).ok()?;
        Piece::new(start, members.get(index)?.cells, total)
    }

    /// Appends to `out` the key of the cluster at `index`'s piece
    /// ([`Piece::key`]): the syllable's text, U+001F and the piece's mark.
    pub fn key(&self, index: usize, out: &mut Vec<char>) -> Option<Piece> {
        let piece = self.piece(index)?;
        piece.key(self.text(), out);
        Some(piece)
    }

    /// Whether the syllable may draw joined: the visible cursor, at `held`
    /// in either phase of its blink, stands on none of its cells on `row`;
    /// the selection covers all of its cells or none of them; its cells
    /// agree in foreground color, in style and in script; and `covered`
    /// says no link overlay covers any of them. The selection is checked
    /// on its own because selected cells can keep the color of the text,
    /// when a program sets the selection's background color alone.
    ///
    /// References:
    /// - ADR-1203 D4: when a syllable draws split.
    /// - ADR-1308 D8: a ligature draws apart under the same rules.
    pub fn joined(
        &self,
        row: usize,
        held: Option<(usize, usize)>,
        covered: impl Fn(usize) -> bool,
    ) -> bool {
        let members = self.members.get(..self.len).unwrap_or_default();
        let Some(first) = members.first() else {
            return false;
        };
        members.iter().all(|m| {
            let cols = m.col..m.col.saturating_add(usize::from(m.cells));
            let under = held.is_some_and(|(c, r)| r == row && cols.contains(&c));
            !under
                && m.fg == first.fg
                && m.variant == first.variant
                && m.selected == first.selected
                && !cols.clone().any(&covered)
        })
    }
}

/// What [`Runs`] knows about one column of its row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Slot {
    /// Not read yet.
    #[default]
    Unknown,
    /// The cell draws on its own.
    Alone,
    /// The cell leads cluster `index` of run `run`.
    In { run: u16, index: u8 },
}

/// The syllables and ligature runs found on one row of one frame, so that
/// each run is walked once however many cells of it are read, and a long
/// run of symbols, such as a line of `=`, is walked once in all rather
/// than once for each of its cells. The renderer keeps one for each frame
/// it draws and reads every row through it. Each run also keeps a verdict
/// the reader computes once: whether it draws joined.
#[derive(Clone, Debug, Default)]
pub struct Runs {
    row: Option<usize>,
    ligatures: bool,
    slots: Vec<Slot>,
    found: Vec<(Syllable, Option<bool>)>,
}

impl Runs {
    /// Forgets every run, as a new frame needs.
    pub fn clear(&mut self) {
        self.row = None;
    }

    /// The run the cell `fc` at `col`, `row` belongs to, as
    /// [`Syllable::find`] finds it, and the cell's place among its
    /// clusters: the run's number for [`Runs::get`] and [`Runs::verdict`].
    /// Reading another row, or with another `ligatures`, forgets the runs
    /// of the row read before.
    pub fn find(
        &mut self,
        frame: &Frame<'_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
        ligatures: bool,
    ) -> Option<(usize, usize)> {
        if self.row != Some(row) || self.ligatures != ligatures {
            self.row = Some(row);
            self.ligatures = ligatures;
            self.slots.clear();
            self.slots.resize(frame.cols(), Slot::Unknown);
            self.found.clear();
        }
        match *self.slots.get(col)? {
            Slot::Alone => return None,
            Slot::In { run, index } => return Some((usize::from(run), usize::from(index))),
            Slot::Unknown => {}
        }
        match Syllable::walk(frame, (col, row), fc, ligatures, true) {
            Walk::Alone => {
                self.mark(col..=col, Slot::Alone);
                None
            }
            Walk::Long(cols) => {
                self.mark(cols, Slot::Alone);
                None
            }
            Walk::Run(s, index) => {
                let run = u16::try_from(self.found.len()).ok()?;
                let members = s.members.get(..s.len).unwrap_or_default();
                for (i, m) in members.iter().enumerate() {
                    let index = u8::try_from(i).unwrap_or(u8::MAX);
                    self.mark(m.col..=m.col, Slot::In { run, index });
                }
                self.found.push((s, None));
                Some((usize::from(run), index))
            }
        }
    }

    /// Sets the slot of each column of `cols`.
    fn mark(&mut self, cols: RangeInclusive<usize>, slot: Slot) {
        let (start, end) = cols.into_inner();
        let end = end.min(self.slots.len().saturating_sub(1));
        if let Some(slots) = self.slots.get_mut(start..=end) {
            slots.fill(slot);
        }
    }

    /// Run `run` of the row read last.
    pub fn get(&self, run: usize) -> Option<&Syllable> {
        self.found.get(run).map(|(s, _)| s)
    }

    /// Whether run `run` draws joined, computed by `judge` the first time
    /// it is asked for, and remembered until the row's runs are forgotten.
    pub fn verdict(&mut self, run: usize, judge: impl FnOnce(&Syllable) -> bool) -> bool {
        let Some((s, verdict)) = self.found.get_mut(run) else {
            return false;
        };
        *verdict.get_or_insert_with(|| judge(s))
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use vtium_term::{Age, Attrs, Cursor, Damage, Line, Palette, Style};

    fn line(text: &str, cols: usize) -> Line {
        let mut t = vtium_term::Terminal::new(cols, 1, 0);
        let _ = t.input(text.as_bytes());
        t.screen().line(0).unwrap().clone()
    }

    fn palette() -> Palette {
        Palette::new(
            [Rgb::default(); 256],
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
        )
    }

    /// Each syllable of `text` on one row, as its cells' first columns,
    /// read from every cell, with ligatures off.
    fn syllables(text: &str) -> Vec<Vec<usize>> {
        runs(text, false)
    }

    /// Each run of `text` on one row that draws joined, a syllable or,
    /// with `ligatures`, a run of ASCII symbols, as its cells' first
    /// columns, read from every cell.
    fn runs(text: &str, ligatures: bool) -> Vec<Vec<usize>> {
        let l = line(text, 40);
        let damage = Damage::new(40, 1);
        let p = palette();
        let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false);
        let mut out: Vec<Vec<usize>> = Vec::new();
        for (col, fc) in frame.cells(0).enumerate() {
            if let Some((s, i)) = Syllable::find(&frame, (col, 0), &fc, ligatures) {
                let cols: Vec<usize> = s.members[..s.len].iter().map(|m| m.col).collect();
                assert_eq!(cols[i], col, "the cell's own place");
                if !out.contains(&cols) {
                    out.push(cols);
                }
            }
        }
        out
    }

    // ADR-1203 D1 on a row: ka-ra, ra-ka, stree and the five consonants
    // of kartsnya are syllables, each over the cells of its clusters;
    // kannada's na-na too
    #[test]
    fn a_row_finds_the_syllables_its_clusters_join() {
        assert_eq!(syllables("ಕ್ರ"), [vec![0, 1]]);
        assert_eq!(syllables("ರ್ಕ"), [vec![0, 1]]);
        assert_eq!(syllables("ಸ್ತ್ರೀ"), [vec![0, 1, 2]]);
        // ka with its spacing vowel sign aa takes two cells (ADR-1006 D1)
        assert_eq!(syllables("ಕಾರ್ತ್ಸ್ನ್ಯ"), [vec![2, 3, 4, 5, 6]]);
        assert_eq!(syllables("ಕನ್ನಡ"), [vec![1, 2]]);
        // a nukta stands between ka and its virama, so the whole cluster
        // before ra counts, not its last two characters
        assert_eq!(
            syllables("ಕ\u{cbc}್ರ"),
            [vec![0, 1]],
            "ka, nukta, virama, ra"
        );
        assert_eq!(
            syllables("a b ಕ್ ರ"),
            Vec::<Vec<usize>>::new(),
            "a space apart"
        );
        assert_eq!(syllables("ಕ್\u{200c}ರ"), Vec::<Vec<usize>>::new(), "ZWNJ");
        assert_eq!(syllables("ಕ್\u{c15}"), Vec::<Vec<usize>>::new(), "Telugu ka");
        assert_eq!(syllables("नमस्ते"), Vec::<Vec<usize>>::new(), "Devanagari");
        // a linker after no consonant joins nothing (rule GB9c of UAX #29)
        assert_eq!(
            syllables("x\u{ccd}ರ"),
            Vec::<Vec<usize>>::new(),
            "x, virama, ra"
        );
        assert_eq!(
            syllables("\u{5d0}\u{ccd}ರ"),
            Vec::<Vec<usize>>::new(),
            "alef, virama, ra"
        );
    }

    // ADR-1203 D1: two cells join only where they were written left to
    // right, so ka with its virama next to ra joins, and joins neither
    // when ka resolves right to left, as under U+202E RIGHT-TO-LEFT
    // OVERRIDE, nor when ra does
    #[test]
    fn a_cell_that_resolves_right_to_left_joins_nothing() {
        let l = line("ಕ್ರ", 4);
        let damage = Damage::new(4, 1);
        let p = palette();
        let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false);
        let (ka, ra) = (frame.cell(0, 0).unwrap(), frame.cell(0, 1).unwrap());
        assert!(linked(&ka, &ra, None), "left to right");
        assert!(
            !linked(&FrameCell { rtl: true, ..ka }, &ra, None),
            "ka right to left"
        );
        assert!(
            !linked(&ka, &FrameCell { rtl: true, ..ra }, None),
            "ra right to left"
        );
    }

    // ADR-1203 D1: a syllable stays on its row; a run past the bound is no
    // syllable and draws split; a Brahmi or Newa stacker joins
    #[test]
    fn a_syllable_ends_at_its_row_and_its_bound() {
        // ka and virama end a row of 3, and ra wraps to the next row
        let mut t = vtium_term::Terminal::new(3, 2, 0);
        let _ = t.input("xxಕ್ರ".as_bytes());
        let (top, next) = (t.screen().line(0).unwrap(), t.screen().line(1).unwrap());
        let damage = Damage::new(3, 2);
        let p = palette();
        let frame = Frame::new(
            vec![top, next],
            &damage,
            &p,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        let ka = frame.cell(0, 2).unwrap();
        assert_eq!(ka.glyph, Glyph::Cluster("ಕ್"));
        assert!(Syllable::find(&frame, (2, 0), &ka, false).is_none());
        let ra = frame.cell(1, 0).unwrap();
        assert!(Syllable::find(&frame, (0, 1), &ra, false).is_none());
        // seven clusters of ka and virama, then ka: 15 scalar values; six,
        // then ka with its vowel sign aa: 14, which fit
        let long: String = "ಕ್".repeat(7) + "ಕ";
        assert!(syllables(&long).is_empty(), "past the bound");
        let fits: String = "ಕ್".repeat(6) + "ಕ\u{cbe}";
        assert_eq!(syllables(&fits), [(0..7).collect::<Vec<_>>()]);
        assert_eq!(syllables("\u{11003}\u{11013}"), [vec![0, 1]], "Brahmi");
        assert_eq!(syllables("\u{11460}\u{1140e}"), [vec![0, 1]], "Newa");
    }

    // ADR-1203 D3: each piece names the syllable's text and where its
    // cluster's cells lie among the syllable's
    #[test]
    fn a_piece_names_its_cells_among_the_syllables() {
        let l = line("ಸ್ತ್ರೀ", 10);
        let damage = Damage::new(10, 1);
        let p = palette();
        let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false);
        let fc = frame.cell(0, 1).unwrap();
        let (s, i) = Syllable::find(&frame, (1, 0), &fc, false).unwrap();
        assert_eq!(i, 1);
        let total = s.members[..s.len].iter().map(|m| m.cells).sum::<u8>();
        assert_eq!(s.piece(1), Piece::new(1, 1, total));
        let mut key = Vec::new();
        let piece = s.key(2, &mut key).unwrap();
        assert_eq!(
            &key[..key.len() - 2],
            "ಸ್ತ್ರೀ".chars().collect::<Vec<_>>().as_slice()
        );
        assert_eq!(key[key.len() - 2..], ['\u{1f}', piece.mark()]);
        assert_eq!(piece.start, 2);
    }

    // ADR-1203 D2: a cluster two cells wide, as ADR-1006 widens a cluster
    // with a spacing mark, takes both its cells in the syllable, and its
    // second half belongs to no syllable of its own
    #[test]
    fn a_wide_cluster_takes_its_two_cells_in_a_syllable() {
        let mut l = line("ಕ್", 6);
        l.write(
            1,
            Glyph::Char('\u{cb0}'),
            Width::Wide,
            &Style::DEFAULT,
            None,
        );
        l.write(2, Glyph::Empty, Width::WideTail, &Style::DEFAULT, None);
        l.write(3, Glyph::Char('x'), Width::Narrow, &Style::DEFAULT, None);
        let damage = Damage::new(6, 1);
        let p = palette();
        let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false);
        let ka = frame.cell(0, 0).unwrap();
        let (s, i) = Syllable::find(&frame, (0, 0), &ka, false).unwrap();
        assert_eq!((i, s.len()), (0, 2));
        assert_eq!(s.cols().collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(s.piece(1), Piece::new(1, 2, 3));
        let ra = frame.cell(0, 1).unwrap();
        assert_eq!(
            Syllable::find(&frame, (1, 0), &ra, false).map(|(_, i)| i),
            Some(1)
        );
        let tail = frame.cell(0, 2).unwrap();
        assert!(Syllable::find(&frame, (2, 0), &tail, false).is_none());
        // the cursor on the second half splits it too
        assert!(!s.joined(0, Some((2, 0)), |_| false));
    }

    // ADR-1203 D4: the cursor on any cell, a cell of another color, style
    // or script, or a link overlay over a cell splits the syllable
    #[test]
    fn the_cursor_a_color_a_style_and_an_overlay_split_a_syllable() {
        let mut l = line("ಕ್ರ", 4);
        let damage = Damage::new(4, 1);
        let p = palette();
        let read = |l: &Line| {
            let frame = Frame::new(vec![l], &damage, &p, Age::NEVER, Cursor::default(), false);
            let fc = frame.cell(0, 0).unwrap();
            Syllable::find(&frame, (0, 0), &fc, false).unwrap().0
        };
        let s = read(&l);
        assert!(s.joined(0, None, |_| false));
        assert!(s.joined(0, Some((2, 0)), |_| false), "the cursor after it");
        assert!(
            s.joined(0, Some((1, 1)), |_| false),
            "the cursor on another row"
        );
        assert!(!s.joined(0, Some((1, 0)), |_| false), "the cursor on ra");
        assert!(!s.joined(0, Some((0, 0)), |_| false), "the cursor on ka");
        assert!(!s.joined(0, None, |c| c == 1), "an overlay over ra");
        let restyle = |l: &mut Line, style: Style| {
            l.write(1, Glyph::Char('\u{cb0}'), Width::Narrow, &style, None);
        };
        let mut bold = Style::DEFAULT;
        bold.attrs = Attrs::BOLD;
        restyle(&mut l, bold);
        assert!(!read(&l).joined(0, None, |_| false), "bold ra");
        let mut red = Style::DEFAULT;
        red.fg = vtium_term::Color::Rgb(Rgb::new(200, 0, 0));
        restyle(&mut l, red);
        assert!(!read(&l).joined(0, None, |_| false), "red ra");
        restyle(&mut l, Style::DEFAULT);
        assert!(read(&l).joined(0, None, |_| false), "as written");
    }

    // ADR-1308 D8: with ligatures on, each run of 2 to 14 ASCII symbols
    // side by side draws joined, and its piece keys name its text; letters,
    // digits and spaces join nothing, a run of 15 draws each cell alone,
    // and a syllable still joins. With ligatures off, no run of symbols is
    // found
    // covers: text.ligatures
    #[test]
    fn with_ligatures_on_a_run_of_ascii_symbols_draws_joined() {
        let code = "a -> b != c; <!-- www 0x";
        assert_eq!(
            runs(code, true),
            [vec![2, 3], vec![7, 8], vec![13, 14, 15, 16]]
        );
        assert_eq!(runs(code, false), Vec::<Vec<usize>>::new());
        assert_eq!(runs(&"=".repeat(14), true), [(0..14).collect::<Vec<_>>()]);
        assert_eq!(runs(&"=".repeat(15), true), Vec::<Vec<usize>>::new());
        assert_eq!(runs("- > ! =", true), Vec::<Vec<usize>>::new());
        assert_eq!(runs("ಕ್ರ ->", true), [vec![0, 1], vec![3, 4]]);
        let l = line("->", 4);
        let damage = Damage::new(4, 1);
        let p = palette();
        let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false);
        let fc = frame.cell(0, 1).unwrap();
        let (s, i) = Syllable::find(&frame, (1, 0), &fc, true).unwrap();
        assert_eq!((i, s.text()), (1, ['-', '>'].as_slice()));
        let mut key = Vec::new();
        assert_eq!(s.key(1, &mut key), Piece::new(1, 1, 2));
        assert_eq!(key[..2], ['-', '>']);
    }

    // ADR-1308 D8: a run of symbols on a double-size line, or written
    // right to left, never joins, and one whose characters differ in color
    // draws apart, as a syllable does
    #[test]
    fn a_ligature_never_joins_on_a_double_size_line_or_backwards() {
        let mut t = vtium_term::Terminal::new(10, 2, 0);
        let _ = t.input(b"\x1b#6->\r\n\x1b[38;2;200;0;0m-\x1b[38;2;0;200;0m>");
        let (wide, plain) = (t.screen().line(0).unwrap(), t.screen().line(1).unwrap());
        let damage = Damage::new(10, 2);
        let p = palette();
        let frame = Frame::new(
            vec![wide, plain],
            &damage,
            &p,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        let arrow = frame.cell(0, 0).unwrap();
        assert!(
            Syllable::find(&frame, (0, 0), &arrow, true).is_none(),
            "double width"
        );
        let (dash, gt) = (frame.cell(1, 0).unwrap(), frame.cell(1, 1).unwrap());
        assert!(linked(&dash, &gt, Some(plain)));
        assert!(
            !linked(&FrameCell { rtl: true, ..dash }, &gt, Some(plain)),
            "right to left"
        );
        let (s, _) = Syllable::find(&frame, (0, 1), &dash, true).unwrap();
        assert!(!s.joined(1, None, |_| false), "two colors");
    }

    // ADR-1308 D8: a selection over part of a ligature draws it apart even
    // when the selected cells keep the text's color, as they do when a
    // program sets the selection's background color (OSC 17) and not its
    // text color; a selection over the whole run keeps it joined. The same
    // holds for a syllable (ADR-1203 D4)
    // covers: text.ligatures
    #[test]
    fn a_selection_over_part_of_a_run_draws_it_apart() {
        let l = line("-> ಕ್ರ", 8);
        let damage = Damage::new(8, 1);
        let mut p = palette();
        p.set_special(vtium_term::Special::SelectionBg, Some(Rgb::new(50, 60, 70)));
        let joined = |selection: core::ops::Range<usize>, col: usize| {
            let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false)
                .with_selection(vec![selection]);
            let fc = frame.cell(0, col).unwrap();
            let (s, _) = Syllable::find(&frame, (col, 0), &fc, true).unwrap();
            (s.joined(0, None, |_| false), fc.fg)
        };
        let (part, fg) = joined(0..1, 0);
        assert_eq!(
            fg,
            palette().fg(),
            "the selected cell keeps the text's color"
        );
        assert!(!part, "the dash alone selected");
        assert!(joined(0..2, 0).0, "the whole arrow selected");
        assert!(joined(4..8, 0).0, "nothing of the arrow selected");
        assert!(!joined(3..4, 3).0, "ka alone selected");
        assert!(joined(3..5, 3).0, "the whole syllable selected");
    }

    // ADR-1308 D8: the cells of a sized run, the block one OSC 66 sequence
    // writes, never join into a ligature, whether the run holds one
    // symbol or several; the same symbols written plainly join
    // covers: text.ligatures
    #[test]
    fn the_cells_of_a_sized_run_never_join_into_a_ligature() {
        let read = |bytes: &[u8]| {
            let mut t = vtium_term::Terminal::new(10, 1, 0);
            let _ = t.input(bytes);
            let l = t.screen().line(0).unwrap().clone();
            let damage = Damage::new(10, 1);
            let p = palette();
            let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false);
            let in_run: Vec<bool> = (0..3).map(|c| l.cell(c).in_run()).collect();
            let found: Vec<bool> = (0..3)
                .map(|c| {
                    let fc = frame.cell(0, c).unwrap();
                    Syllable::find(&frame, (c, 0), &fc, true).is_some()
                })
                .collect();
            (in_run, found)
        };
        let (in_run, found) = read(b"\x1b]66;w=1;-\x1b\\\x1b]66;w=1;>\x1b\\");
        assert_eq!(in_run[..2], [true, true]);
        assert_eq!(found[..2], [false, false], "two runs of one cell");
        let (in_run, found) = read(b"\x1b]66;w=1;-\x1b\\>");
        assert_eq!(in_run[..2], [true, false]);
        assert_eq!(
            found[..2],
            [false, false],
            "a run of one cell beside a plain >"
        );
        let (in_run, found) = read(b"->");
        assert_eq!(in_run[..2], [false, false]);
        assert_eq!(found[..2], [true, true], "written plainly");
    }

    // Runs finds on each row what Syllable::find finds from each cell,
    // whichever cell of a run is read first, for syllables, short and long
    // runs of symbols, and runs at the row's ends; and it forgets a row's
    // runs when another row or another ligatures setting is read
    #[test]
    fn runs_find_what_each_cell_finds() {
        let rows = [
            "a -> b != c; <!-- www ಕ್ರ =",
            "==============================",
            "->=============== <=>  ಸ್ತ್ರೀ ->",
            "ಕ್ಕ್ಕ್ಕ್ಕ್ಕ್ಕ್ಕ ->",
            "ಕ್ರ===============ಕ್ರ->",
        ];
        let lines: Vec<Line> = rows.iter().map(|r| line(r, 30)).collect();
        let damage = Damage::new(30, rows.len());
        let p = palette();
        let frame = Frame::new(
            lines.iter().collect(),
            &damage,
            &p,
            Age::NEVER,
            Cursor::default(),
            false,
        );
        for ligatures in [true, false] {
            for order in [false, true] {
                let mut runs = Runs::default();
                for row in 0..rows.len() {
                    let mut cols: Vec<usize> = (0..30).collect();
                    if order {
                        cols.reverse();
                    }
                    for col in cols {
                        let fc = frame.cell(row, col).unwrap();
                        let want = Syllable::find(&frame, (col, row), &fc, ligatures);
                        let got = runs
                            .find(&frame, (col, row), &fc, ligatures)
                            .map(|(run, i)| (runs.get(run).unwrap().clone(), i));
                        assert_eq!(got, want, "row {row}, column {col}, {ligatures}, {order}");
                    }
                }
            }
        }
        // a verdict is kept for its run until the row changes
        let mut runs = Runs::default();
        let fc = frame.cell(0, 2).unwrap();
        let (run, _) = runs.find(&frame, (2, 0), &fc, true).unwrap();
        assert!(!runs.verdict(run, |_| false));
        assert!(!runs.verdict(run, |_| true), "kept");
        let other = frame.cell(1, 0).unwrap();
        assert!(
            runs.find(&frame, (0, 1), &other, true).is_none(),
            "too long"
        );
        let (run, _) = runs.find(&frame, (2, 0), &fc, true).unwrap();
        assert!(runs.verdict(run, |_| true), "forgotten with its row");
        assert!(
            runs.find(&frame, (2, 0), &fc, false).is_none(),
            "forgotten with the setting"
        );
    }
}
