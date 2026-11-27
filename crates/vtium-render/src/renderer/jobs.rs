//! The jobs of one cell: the colors, decorations and text it paints, the
//! bar cursor's bar, and the helpers that find the cells a cell draws over
//! and the text it draws.

#[cfg(feature = "bidi")]
use vtium_font::Font;
#[cfg(feature = "bidi")]
use vtium_font::syllable::Piece;
use vtium_font::{Face, Script, Width as FontWidth};
use vtium_term::{Attrs, Cursor, CursorShape, FrameCell, Glyph, Width, dim};

use crate::compose::{Bar, Job};
use crate::deco::{Kind, Line};
use crate::mono::{self, Depth, Ink};
#[cfg(feature = "images")]
use crate::picture::{Mark, Under};
#[cfg(all(doc, feature = "bidi"))]
use crate::syllable::Runs;
#[cfg(feature = "bidi")]
use crate::syllable::Syllable;

use super::layout::Grid;
use super::pass::{Paint, Pass, Place};
use super::{BlinkStyle, DrawState, Renderer, TextBlink, double};

/// How many pixels wide a bar cursor is in a cell `cw` pixels wide: one
/// eighth of the cell, rounded to the nearest pixel, and at least one. It
/// is the width vtium-font gives its sprite for U+258F, LEFT ONE EIGHTH
/// BLOCK, so the bar stands where that block would.
fn bar_width(cw: i64) -> i64 {
    cw.saturating_mul(2).saturating_add(8).div_euclid(16).max(1)
}

/// How many cells a cell `width` wide, after a cell `before` wide, draws
/// over: 0 for the trailing half of a wide character, which the leading
/// half draws; 2 for a leading half with its trailing half next on the
/// screen, whose width `next` reads; else 1. A half whose partner is
/// missing draws as one cell, so no frame leaves a cell undrawn.
pub(super) fn span(
    width: Width,
    before: Option<Width>,
    next: impl FnOnce() -> Option<Width>,
) -> usize {
    match width {
        Width::WideTail if before == Some(Width::Wide) => 0,
        Width::Wide if next() == Some(Width::WideTail) => 2,
        _ => 1,
    }
}

/// The cells `fc` at `col`, `row` draws over when it takes part in a
/// lam-alef ligature (ADR-1004 D6): 2 for the alef, which draws the
/// ligature, and 0 for the lam beside it; `None` for any other cell.
#[cfg(feature = "bidi")]
pub(super) fn paired(p: &Pass<'_, '_>, at: (usize, usize), fc: &FrameCell<'_>) -> Option<usize> {
    // the overlay's characters join nothing, and the letter beside them
    // joins none of them
    let (col, row) = at;
    if p.covered(col.saturating_sub(1), row) {
        return None;
    }
    match crate::joined::span_at(p.frame, p.frame_at(at), fc, p.held_at()) {
        crate::joined::Span::Pair => Some(2),
        crate::joined::Span::Tail => Some(0),
        crate::joined::Span::One => None,
    }
}

/// The syllable `fc` at `col`, `row` belongs to when it draws joined, and
/// the piece the cell draws. A syllable draws joined only while the rules
/// of [`Syllable::joined`] allow it and `font` holds every piece. The
/// pass's [`Runs`] finds the syllable and remembers whether it draws
/// joined, so each run is walked and judged once for its row. `out` is
/// scratch.
///
/// References:
/// - ADR-1203 D4: when a syllable draws split.
#[cfg(feature = "bidi")]
pub(super) fn joined_syllable<F: Face>(
    font: &Font<F>,
    p: &Pass<'_, '_>,
    (col, row): (usize, usize),
    fc: &FrameCell<'_>,
    out: &mut Vec<char>,
) -> Option<(Syllable, Piece)> {
    // a cell under the link overlay draws the overlay's text, so it joins
    // nothing, and its text stays out of the row's runs
    let covered = |c: usize| p.covered(c, row);
    if covered(col) {
        return None;
    }
    let (at, held) = (p.frame_at((col, row)), p.held_at());
    let mut runs = p.runs.borrow_mut();
    let (run, index) = runs.find_at(p.frame, at, fc, p.ligatures)?;
    let joined = runs.verdict(run, |s| {
        s.joined(at.1, held, covered)
            && (0..s.len()).all(|i| {
                out.clear();
                s.key(i, out).is_some() && font.holds(out, s.variant())
            })
    });
    if !joined {
        return None;
    }
    let s = runs.get(run)?;
    Some((s.clone(), s.piece(index)?))
}

/// Appends to `out` the text the cell `fc` at `col`, `row` draws: its
/// piece's key where its syllable draws joined (ADR-1203 D3), else its
/// text as [`crate::joined::drawn`] gives it, its joining sides and
/// mirroring, a lam-alef ligature's two letters, or nothing for the lam
/// (ADR-1004 D6, D7).
#[cfg(feature = "bidi")]
pub(super) fn drawn_text<F: Face>(
    font: &Font<F>,
    p: &Pass<'_, '_>,
    at: (usize, usize),
    fc: &FrameCell<'_>,
    out: &mut Vec<char>,
) {
    let mut scratch = Vec::new();
    match joined_syllable(font, p, at, fc, &mut scratch) {
        Some((s, piece)) => piece.key(s.text(), out),
        None => {
            crate::joined::drawn_at(p.frame, p.frame_at(at), fc, p.held_at(), out);
        }
    }
}

/// The width of the cell after `col` in `row`, among the grid's `cols`.
pub(super) fn next_width(p: &Pass<'_, '_>, row: usize, col: usize, cols: usize) -> Option<Width> {
    let next = col.saturating_add(1);
    (next < cols)
        .then(|| p.cell(row, next))
        .flatten()
        .map(|fc| fc.width)
}

/// Whether `fc` draws in the text blink's off phase: blinking, not under
/// the cursor, in the off phase of its own blink, with `blink` not off. A
/// cell with SGR 6 blinks at the rapid blink's phase, SGR 5 or not.
pub(super) fn blink_off(
    fc: &FrameCell<'_>,
    is_cursor: bool,
    state: &DrawState,
    blink: TextBlink,
) -> bool {
    let hidden = if fc.attrs.contains(Attrs::RAPID_BLINK) {
        state.rapid_hidden
    } else {
        fc.attrs.contains(Attrs::BLINK) && state.blink_hidden
    };
    !is_cursor && hidden && blink.style != BlinkStyle::Off
}

/// Whether a cell holding `glyph` draws no text and no decoration of its own:
/// a kitty Unicode placeholder, a cluster whose first scalar value is
/// U+10EEEE, marks where a virtual placement's picture draws (ADR-1100 D13).
/// Its background, the cursor and the selection draw as on any cell. In a
/// build without `images`, U+10EEEE is a private-use character like any
/// other.
pub(super) fn draws_no_text(glyph: Glyph<'_>) -> bool {
    const PLACEHOLDER: char = '\u{10EEEE}';
    cfg!(feature = "images")
        && match glyph {
            Glyph::Empty => false,
            Glyph::Char(c) => c == PLACEHOLDER,
            Glyph::Cluster(s) => s.starts_with(PLACEHOLDER),
        }
}

impl<F: Face> Renderer<F> {
    /// At 1 bit, whether `fc`, at `col` in `row`, draws its background lit
    /// (`mono::marked`): the selection inverts what the cell draws
    /// unselected. `None` at any other depth.
    #[cfg(not(feature = "scroll-pan"))]
    pub(super) fn one_bit_lit(
        &self,
        p: &Pass<'_, '_>,
        fc: &FrameCell<'_>,
        row: usize,
        col: usize,
    ) -> Option<bool> {
        if self.config.depth != Depth::One {
            return None;
        }
        let plain = p.plain();
        let natural =
            |c: &FrameCell<'_>| mono::marked(c.bg, c.fg, c.attrs.contains(Attrs::INVERSE));
        Some(if fc.selected {
            !plain
                .cell(row, col)
                .map_or_else(|| natural(fc), |c| natural(&c))
        } else {
            natural(fc)
        })
    }

    /// At 1 bit, whether `fc`, at `col` in `row`, draws its background lit
    /// (`mono::marked`): the selection inverts what the cell draws
    /// unselected. `None` at any other depth.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn one_bit_lit(
        &self,
        p: &Pass<'_, '_>,
        fc: &FrameCell<'_>,
        row: usize,
        col: usize,
    ) -> Option<bool> {
        if self.config.depth != Depth::One {
            return None;
        }
        let natural =
            |c: &FrameCell<'_>| mono::marked(c.bg, c.fg, c.attrs.contains(Attrs::INVERSE));
        Some(if fc.selected {
            !p.plain_cell(row, col)
                .map_or_else(|| natural(fc), |c| natural(&c))
        } else {
            natural(fc)
        })
    }

    /// The job for one cell. `off` draws it in the text blink's off phase.
    /// A bar cursor keeps the cell's letter: the letter draws as it does
    /// with no cursor, and the bar draws over it along the left edge of the
    /// character's cells. In a build with the bidi feature, `drawn` is the
    /// text the cell draws ([`crate::joined::drawn`]), and the bar stands
    /// along the right edge of a character that resolves right to left.
    ///
    /// References:
    /// - ADR-1004 D9: the bar's side on a right-to-left character.
    #[cfg_attr(
        feature = "bidi",
        expect(
            clippy::too_many_arguments,
            reason = "the drawn text joins the cell's own in a build with the bidi feature"
        )
    )]
    pub(super) fn push_jobs(
        &mut self,
        g: &Grid,
        fc: &FrameCell<'_>,
        at: Place,
        cursor: Option<&Cursor>,
        off: bool,
        lit: Option<bool>,
        #[cfg(feature = "bidi")] drawn: &[char],
    ) {
        let force = self.config.decorations.force;
        let (deco_ul, deco_strike) = self.deco;
        // the frame has dimmed the foreground; the cursor draws over it
        let (mut fg, mut bg) = (fc.fg, fc.bg);
        // a placeholder cell and a concealed one (SGR 8) draw their
        // background alone: no glyph and no line, the cursor aside
        let placeholder = draws_no_text(fc.glyph) || fc.attrs.contains(Attrs::CONCEAL);
        // A cell of the link under the pointer draws a dashed underline in
        // place of its own style, in its own underline color (ADR-1002,
        // "Drawing").
        let mut underline = if placeholder {
            vtium_term::UnderlineStyle::None
        } else if fc.hovered {
            vtium_term::UnderlineStyle::Dashed
        } else {
            fc.attrs.underline()
        };
        let mut ul_cursor = false;
        let mut bar = false;
        // a superscript or subscript cell's own lines follow its small
        // glyph (ADR-1202 D8)
        let script = crate::variant::of(fc.attrs).script;
        let mut cell_lit = lit;
        // at 1 bit a mark's text always shows: the selection's and a block
        // cursor's ink is never their background
        if lit.is_some() && fc.selected {
            fg = mono::ink_on(bg);
        }
        match cursor.map(|c| (c.shape, c.color)) {
            Some((CursorShape::Block, color)) => {
                core::mem::swap(&mut fg, &mut bg);
                if let Some(c) = color {
                    bg = c;
                }
                cell_lit = lit.map(|l| !l);
                if lit.is_some() {
                    fg = mono::ink_on(bg);
                }
            }
            Some((CursorShape::Underline, _)) => {
                underline = match underline {
                    vtium_term::UnderlineStyle::None => vtium_term::UnderlineStyle::Single,
                    _ => vtium_term::UnderlineStyle::None,
                };
                ul_cursor = true;
            }
            Some((CursorShape::Bar, _)) => bar = true,
            None => {}
        }
        // at 1 bit a line cursor is the cell's ink, whatever its color
        let cursor_color = match lit {
            Some(_) if !matches!(cursor.map(|c| c.shape), Some(CursorShape::Block)) => {
                cursor.map(|_| mono::ink_on(bg))
            }
            _ => cursor.and_then(|c| c.color),
        };
        let ul_color = if ul_cursor {
            cursor_color.unwrap_or(fg)
        } else {
            match (fc.underline_color, force, deco_ul) {
                (Some(c), false, _) => c,
                (_, _, Some(c)) => c,
                (_, _, None) => fg,
            }
        };
        // the strike and the overline draw over the glyph as one sprite, in
        // the strike's color
        let over = Kind::over(
            fc.attrs.contains(Attrs::STRIKE),
            fc.attrs.contains(Attrs::OVERLINE),
        )
        .filter(|_| !placeholder)
        .map(|kind| {
            let line = Line {
                kind,
                script,
                skips_ink: false,
            };
            (line, deco_strike.unwrap_or(fg))
        });
        // the underline cursor keeps a normal cell's rows, so it has one
        // height along a line
        let ul_script = if ul_cursor { Script::Baseline } else { script };
        let mut underline = Kind::underline(underline).map(|kind| {
            let line = Line {
                kind,
                script: ul_script,
                skips_ink: !ul_cursor,
            };
            (line, ul_color)
        });
        let mut over = over;
        let blink = self.config.blink;
        let (hidden, dimmed) = match blink.style {
            BlinkStyle::Full => (off, false),
            BlinkStyle::Partial => (false, off),
            BlinkStyle::Off => (false, false),
        };
        if off && blink.decorations {
            if hidden {
                (underline, over) = (None, None);
            } else if dimmed {
                underline = underline.map(|(k, c)| (k, dim(c, bg)));
                over = over.map(|(k, c)| (k, dim(c, bg)));
            }
        }
        if dimmed {
            fg = dim(fg, bg);
        }
        let style = crate::variant::of(fc.attrs);
        let text = &mut self.compose.text;
        let start = text.len();
        #[cfg(feature = "bidi")]
        if !hidden && !placeholder {
            text.extend_from_slice(drawn);
        }
        #[cfg(not(feature = "bidi"))]
        if !hidden && !placeholder {
            match fc.glyph {
                Glyph::Empty => {}
                Glyph::Char(c) => text.push(c),
                Glyph::Cluster(s) => text.extend(s.chars()),
            }
        }
        // a lam-alef ligature spans its two narrow cells
        #[cfg(feature = "bidi")]
        let wide = fc.width == Width::Wide || at.ncells == 2;
        #[cfg(not(feature = "bidi"))]
        let wide = fc.width == Width::Wide;
        let width = if wide {
            FontWidth::Wide
        } else {
            FontWidth::Narrow
        };
        let len = text.len().saturating_sub(start);
        let text = (len > 0).then_some((start, len, width, style));
        // vtium's mark, which draws above every picture (ADR-1100 D4)
        #[cfg(feature = "images")]
        let under = |mark: Mark| Under {
            default_bg: fc.default_bg,
            selected: fc.selected,
            mark,
        };
        // the bar stands along the left edge of the character's cells, or
        // along their right edge where the character resolves right to left
        let bar = bar.then(|| {
            #[cfg(feature = "bidi")]
            let right = cursor.is_some_and(|c| c.rtl);
            #[cfg(not(feature = "bidi"))]
            let right = false;
            let w = bar_width(self.layout.cw);
            Bar {
                area: self
                    .layout
                    .bar_area(g, (at.col, at.row, at.ncells), w, right),
                color: cursor_color.unwrap_or(fg),
                w,
                right,
            }
        });
        let glyph_cells = if width == FontWidth::Wide { 2 } else { 1 };
        let paint = Paint {
            fg,
            bg,
            underline,
            over,
            text,
            lit: cell_lit,
            bar,
        };
        let job = self.job(g, at, glyph_cells, paint);
        #[cfg(feature = "images")]
        let job = Job {
            under: under(match cursor.map(|c| c.shape) {
                Some(CursorShape::Block) => Mark::Opaque,
                Some(CursorShape::Bar) => Mark::Bar,
                Some(CursorShape::Underline) => Mark::Underline,
                None => Mark::None,
            }),
            ..job
        };
        self.compose.jobs.push(job);
    }

    fn job(&self, g: &Grid, at: Place, glyph_cells: usize, paint: Paint) -> Job {
        let l = &self.layout;
        let depth = self.config.depth;
        let ink = match paint.lit {
            Some(lit) => Ink::with_state(depth, paint.bg, lit),
            None => Ink::new(depth, paint.bg),
        };
        Job {
            posx: at.col,
            ncells: at.ncells,
            r: l.cells_area(g, at.col, at.row, at.ncells),
            g: l.cells_area(g, at.col, at.row, glyph_cells),
            tiles: [
                l.cells_area(g, at.col, at.row, 1),
                l.cells_area(g, at.col.saturating_add(1), at.row, 1),
            ],
            fg: ink.fg(paint.fg),
            bg: ink.bg(),
            underline: paint.underline.map(|(k, c)| (k, ink.fg(c))),
            over: paint.over.map(|(k, c)| (k, ink.fg(c))),
            text: paint.text,
            silhouette: depth == Depth::One,
            piece: double::Piece::WHOLE,
            #[cfg(feature = "images")]
            under: Under::default(),
            bar: paint.bar.map(|b| Bar {
                color: ink.fg(b.color),
                ..b
            }),
        }
    }

    /// When the row draws a cell of several layers, every cell between the
    /// first and the last of them is composed with them in the row scratch,
    /// so those not yet drawn join the jobs.
    pub(super) fn widen_to_span(&mut self, p: &Pass<'_, '_>, row: usize) {
        let Some((first, end)) = self.compose.layered_span() else {
            return;
        };
        let g = &p.g;
        let mut before = first
            .checked_sub(1)
            .and_then(|c| p.cell(row, c))
            .map(|fc| fc.width);
        let run = p.cells(row).take(end.min(g.cols)).enumerate();
        for (here, fc) in run.skip(first) {
            let ncells = span(fc.width, before, || next_width(p, row, here, g.cols));
            #[cfg(feature = "bidi")]
            let ncells = paired(p, (here, row), &fc).unwrap_or(ncells);
            before = Some(fc.width);
            // each cell of a sized run draws its own piece of the run, as
            // the row's scan draws it
            let in_run = self.row_runs && p.run_at(row, here).is_some();
            let ncells = if in_run { 1 } else { ncells };
            if ncells == 0 {
                continue;
            }
            let covered = self
                .compose
                .jobs
                .iter()
                .any(|j| here >= j.posx && here < j.posx.saturating_add(j.ncells));
            if covered {
                continue;
            }
            if in_run {
                self.cells.mark_drawn(here, row, 1);
                self.push_cell_jobs(p, (here, row), &fc, 1);
                continue;
            }
            let is_cursor = p.shows_cursor(here, row);
            let off = blink_off(&fc, is_cursor, p.state, self.config.blink);
            self.cells.mark_drawn(here, row, ncells);
            #[cfg(feature = "bidi")]
            let mut drawn = core::mem::take(&mut self.drawn);
            #[cfg(feature = "bidi")]
            {
                drawn.clear();
                drawn_text(&self.font, p, (here, row), &fc, &mut drawn);
            }
            self.push_jobs(
                g,
                &fc,
                Place {
                    col: here,
                    row,
                    ncells,
                },
                is_cursor.then_some(&p.cursor),
                off,
                self.one_bit_lit(p, &fc, row, here),
                #[cfg(feature = "bidi")]
                &drawn,
            );
            #[cfg(feature = "bidi")]
            {
                self.drawn = drawn;
            }
        }
        self.compose.jobs.sort_by_key(|j| j.posx);
    }
}
