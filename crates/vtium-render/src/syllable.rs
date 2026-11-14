//! The syllables a row draws joined across their clusters (ADR-1203).
//!
//! Unicode's grapheme cluster rules cut many syllables into several
//! clusters, as they cut every Kannada conjunct: ಕ್ರ is ಕ್ in one cell and ರ
//! in the next. The terminal keeps those clusters, so the cursor and what
//! programs count do not change. Only the drawing joins. [`Syllable::find`]
//! reads a row of the frame for the run of cells around a cell whose
//! clusters join by ADR-1203 D1 ([`vtium_font::syllable::joins`]). The
//! cells must stand side by side on the row in the order they were written,
//! left to right. A run whose text holds more than
//! [`SYLLABLE_SCALARS_MAX`] scalar values is no syllable, and its cells
//! draw on their own.
//!
//! The console asks the glyph helper for each piece of a syllable: its
//! whole text, followed by a mark that says where one cluster's cells lie
//! among the syllable's ([`Syllable::key`]). The helper shapes the text
//! once, draws it across all the syllable's cells, and cuts the drawing at
//! the clusters' edges. Each cell then draws its own piece with its own
//! background and decorations. [`Syllable::joined`] says when the pieces
//! draw: never while the visible cursor stands on any cell of the
//! syllable, or its cells differ in foreground color, in style or in
//! script, or a link's overlay covers one of them (ADR-1203 D4). The
//! renderer also draws the syllable split while its font lacks any piece.

use vtium_font::Variant;
use vtium_font::syllable::{Piece, SYLLABLE_SCALARS_MAX, joins, may_follow, may_link};
use vtium_term::{CLUSTER_MAX, Frame, FrameCell, Glyph, Rgb, Width};

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
}

/// A run of cells whose clusters join into one syllable, on one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Syllable {
    members: [Member; CLUSTERS_MAX],
    len: usize,
    text: [char; SYLLABLE_SCALARS_MAX],
    text_len: usize,
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

/// Whether `left`, the cell drawn at column `col`, and `right`, the cell
/// drawn just after it, are clusters written one after the other, left to
/// right, whose clusters join (ADR-1203 D1).
fn linked(left: &FrameCell<'_>, right: &FrameCell<'_>) -> bool {
    if right.width == Width::WideTail || left.rtl || right.rtl {
        return false;
    }
    if left.col.checked_add(usize::from(cells(left))) != Some(right.col) {
        return false;
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
    /// cells on the row around it whose clusters join (ADR-1203 D1).
    /// `None` when no neighbour joins it, when it is the second half of a
    /// wide character, and when the run's text holds more than
    /// [`SYLLABLE_SCALARS_MAX`] scalar values.
    pub fn find(
        frame: &Frame<'_>,
        (col, row): (usize, usize),
        fc: &FrameCell<'_>,
    ) -> Option<(Syllable, usize)> {
        if fc.width == Width::WideTail {
            return None;
        }
        // a quick answer for most cells: one that neither ends with a
        // linker nor starts with a letter that follows one joins nothing
        let first = chars(fc.glyph).next()?;
        let last = chars(fc.glyph).last()?;
        if !may_link(last) && !may_follow(first) {
            return None;
        }
        let member = |col: usize, fc: &FrameCell<'_>| Member {
            col,
            cells: cells(fc),
            fg: fc.fg,
            variant: crate::variant::of(fc.attrs),
        };
        let mut scalars = chars(fc.glyph).count();
        // the clusters before the cell, nearest first
        let mut before: Vec<(Member, FrameCell<'_>)> = Vec::new();
        let (mut at, mut here) = (col, *fc);
        while let Some(left_col) = at.checked_sub(1) {
            let mut lead = left_col;
            let mut left = frame.cell(row, lead)?;
            if left.width == Width::WideTail {
                lead = left_col.checked_sub(1)?;
                left = frame.cell(row, lead)?;
            }
            if !linked(&left, &here) {
                break;
            }
            scalars = scalars.saturating_add(chars(left.glyph).count());
            if scalars > SYLLABLE_SCALARS_MAX {
                return None;
            }
            before.push((member(lead, &left), left));
            (at, here) = (lead, left);
        }
        // the clusters after it
        let mut after: Vec<(Member, FrameCell<'_>)> = Vec::new();
        let (mut at, mut here) = (col, *fc);
        loop {
            let next = at.checked_add(usize::from(cells(&here)))?;
            let Some(right) = frame.cell(row, next) else {
                break;
            };
            if !linked(&here, &right) {
                break;
            }
            scalars = scalars.saturating_add(chars(right.glyph).count());
            if scalars > SYLLABLE_SCALARS_MAX {
                return None;
            }
            after.push((member(next, &right), right));
            (at, here) = (next, right);
        }
        if before.is_empty() && after.is_empty() {
            return None;
        }
        let index = before.len();
        let mut s = Syllable {
            members: [Member::default(); CLUSTERS_MAX],
            len: 0,
            text: ['\0'; SYLLABLE_SCALARS_MAX],
            text_len: 0,
        };
        let own = (member(col, fc), *fc);
        let run = before
            .iter()
            .rev()
            .chain(core::iter::once(&own))
            .chain(&after);
        for (m, cell) in run {
            *s.members.get_mut(s.len)? = *m;
            s.len = s.len.saturating_add(1);
            for c in chars(cell.glyph) {
                *s.text.get_mut(s.text_len)? = c;
                s.text_len = s.text_len.saturating_add(1);
            }
        }
        Some((s, index))
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
    /// pieces (ADR-1203 D3).
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

    /// Appends to `out` the key of the cluster at `index`'s piece: the
    /// syllable's text and the piece's mark.
    pub fn key(&self, index: usize, out: &mut Vec<char>) -> Option<Piece> {
        let piece = self.piece(index)?;
        out.extend_from_slice(self.text());
        out.push(piece.mark());
        Some(piece)
    }

    /// Whether the syllable may draw joined (ADR-1203 D4): the visible
    /// cursor, at `held` in either phase of its blink, stands on none of
    /// its cells on `row`; its cells agree in foreground color, in style
    /// and in script; and `covered` says no link overlay covers any of
    /// them. A partly selected syllable has cells of two colors, so it
    /// draws split, and a wholly selected one joined.
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
            !under && m.fg == first.fg && m.variant == first.variant && !cols.clone().any(&covered)
        })
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
    /// read from every cell.
    fn syllables(text: &str) -> Vec<Vec<usize>> {
        let l = line(text, 40);
        let damage = Damage::new(40, 1);
        let p = palette();
        let frame = Frame::new(vec![&l], &damage, &p, Age::NEVER, Cursor::default(), false);
        let mut out: Vec<Vec<usize>> = Vec::new();
        for (col, fc) in frame.cells(0).enumerate() {
            if let Some((s, i)) = Syllable::find(&frame, (col, 0), &fc) {
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
        assert!(linked(&ka, &ra), "left to right");
        assert!(
            !linked(&FrameCell { rtl: true, ..ka }, &ra),
            "ka right to left"
        );
        assert!(
            !linked(&ka, &FrameCell { rtl: true, ..ra }),
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
        assert!(Syllable::find(&frame, (2, 0), &ka).is_none());
        let ra = frame.cell(1, 0).unwrap();
        assert!(Syllable::find(&frame, (0, 1), &ra).is_none());
        // eight clusters of ka and virama, then ka: 17 scalar values
        let long: String = "ಕ್".repeat(8) + "ಕ";
        assert!(syllables(&long).is_empty(), "past the bound");
        let fits: String = "ಕ್".repeat(7) + "ಕ";
        assert_eq!(syllables(&fits), [(0..8).collect::<Vec<_>>()]);
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
        let (s, i) = Syllable::find(&frame, (1, 0), &fc).unwrap();
        assert_eq!(i, 1);
        let total = s.members[..s.len].iter().map(|m| m.cells).sum::<u8>();
        assert_eq!(s.piece(1), Piece::new(1, 1, total));
        let mut key = Vec::new();
        let piece = s.key(2, &mut key).unwrap();
        assert_eq!(
            &key[..key.len() - 1],
            "ಸ್ತ್ರೀ".chars().collect::<Vec<_>>().as_slice()
        );
        assert_eq!(key.last(), Some(&piece.mark()));
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
        let (s, i) = Syllable::find(&frame, (0, 0), &ka).unwrap();
        assert_eq!((i, s.len()), (0, 2));
        assert_eq!(s.cols().collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(s.piece(1), Piece::new(1, 2, 3));
        let ra = frame.cell(0, 1).unwrap();
        assert_eq!(Syllable::find(&frame, (1, 0), &ra).map(|(_, i)| i), Some(1));
        let tail = frame.cell(0, 2).unwrap();
        assert!(Syllable::find(&frame, (2, 0), &tail).is_none());
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
            Syllable::find(&frame, (0, 0), &fc).unwrap().0
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
}
