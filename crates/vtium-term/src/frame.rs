//! Frames: what a renderer reads to draw the screen.
//!
//! A [`Frame`] views the visible lines with their colours resolved through
//! the palette at the time it is drawn, each cell marked changed or not
//! for the display reading it, the cursor, and the selection. Resolving at
//! draw time is what lets a palette change recolour text already on the
//! screen.
//!
//! A cell's colours resolve in the C reference's order: bold brightens a
//! foreground palette colour below 8 to its bright twin, the palette gives
//! RGB, inverse video and DECSCNM swap the two, the selection swaps them
//! again or takes the selection colours OSC 17 and 19 set, and dim draws
//! the foreground halfway to the background.

use core::ops::Range;

use crate::cell::{Cell, StyleIndex, Width};
use crate::damage::{Age, Damage};
use crate::line::{Glyph, Line, LinkId};
use crate::palette::{Palette, Special};
use crate::style::{Attrs, Color, Rgb, Style};

/// SGR 2's foreground: halfway from `fg` to `bg`, each channel the sum of
/// the two halved (the C reference's `color_dim`). The renderer dims
/// blinking text with it too.
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
    /// OSC 12's colour; `None` draws the cell under it inverted.
    pub color: Option<Rgb>,
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
    /// The underline colour SGR 58 set, resolved and exact; `None` follows
    /// the configured underline colour, or the foreground.
    pub underline_color: Option<Rgb>,
    /// SGR's attributes, which a renderer draws: the bold face, blink,
    /// conceal, the underline style, overline, strike, the script. Dim and
    /// bold as bright are already in the colours.
    pub attrs: Attrs,
    /// The background is the default one and not inverted: an image placed
    /// below cell backgrounds shows through (libtsm#35).
    pub default_bg: bool,
    pub link: Option<LinkId>,
    /// Inside the selection, which draws inverted or in the selection
    /// colours.
    pub selected: bool,
    /// Changed since the age the reading display passed.
    pub changed: bool,
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
        }
    }

    /// The frame with the columns of each row, top to bottom, that the
    /// selection covers.
    pub fn with_selection(mut self, selection: Vec<Range<usize>>) -> Self {
        self.selection = selection;
        self
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

    /// The palette the frame resolves through, for the colours a renderer
    /// resolves itself at draw time.
    pub fn palette(&self) -> &'a Palette {
        self.palette
    }

    /// The line drawn at `row`, for its flags and identity.
    pub fn line(&self, row: usize) -> Option<&'a Line> {
        self.lines.get(row).copied()
    }

    /// Whether any cell of `row` changed for this display.
    pub fn row_changed(&self, row: usize) -> bool {
        self.damage.row_changed(row, self.since)
    }

    /// The cell at `row`, `col`, resolved; `None` outside the frame.
    pub fn cell(&self, row: usize, col: usize) -> Option<FrameCell<'a>> {
        let line = self.line(row)?;
        if col >= self.cols() {
            return None;
        }
        let cell = line.cell(col);
        let selected = self.selection.get(row).is_some_and(|r| r.contains(&col));
        let colors = self.resolve(line.style(cell), selected);
        Some(self.frame_cell(line, cell, row, col, colors))
    }

    /// The cells of `row`, left to right, each as [`Frame::cell`] reads it;
    /// none outside the frame. A run of cells in one style, all selected or
    /// none, resolves its colours once.
    pub fn cells(&self, row: usize) -> impl Iterator<Item = FrameCell<'a>> + '_ {
        let line = self.line(row);
        let cols = line.map_or(0, |_| self.cols());
        let selection = self.selection.get(row);
        let mut last: Option<(StyleIndex, bool, Colors)> = None;
        line.into_iter()
            .flat_map(move |line| (0..cols).map(move |col| (line, col)))
            .map(move |(line, col)| {
                let cell = line.cell(col);
                let selected = selection.is_some_and(|r| r.contains(&col));
                let colors = match last {
                    Some((style, sel, colors)) if style == cell.style() && sel == selected => {
                        colors
                    }
                    _ => {
                        let colors = self.resolve(line.style(cell), selected);
                        last = Some((cell.style(), selected, colors));
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
        FrameCell {
            glyph: line.glyph(cell),
            width: cell.width(),
            fg: c.fg,
            bg: c.bg,
            underline_color: c.underline_color,
            attrs: c.attrs,
            default_bg: c.default_bg,
            link: line.link(cell),
            selected: c.selected,
            changed: self.damage.changed(row, col, self.since),
        }
    }

    /// `style`'s colours as a cell draws them, `selected` or not.
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

/// A cell's colours and attributes, resolved from its style.
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

    // ADR-1000's feature row "Colours resolved at draw time, so a palette
    // change recolours text" (the C reference's probe palette-at-draw).
    // covers: sgr.draw-time-colour
    #[test]
    fn a_palette_change_recolours_text_already_written() {
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
    fn default_colours_follow_the_palettes_defaults() {
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

    // libtsm#35: a background set to the default's colour is not the
    // default background, and an inverted default background is not.
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
    // colours drawn.
    #[test]
    fn selection_colours_replace_the_inversion() {
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

    // kmscon ADR 102: a set underline colour stays under reverse video,
    // and the default follows the foreground.
    #[test]
    fn a_set_underline_colour_survives_inverse() {
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

    /// The colours of a cell written in `style`, `reverse` being DECSCNM.
    struct Colours {
        fg: Rgb,
        bg: Rgb,
        underline_color: Option<Rgb>,
    }

    fn resolved(style: Style, reverse: bool) -> Colours {
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
        Colours {
            fg: cell.fg,
            bg: cell.bg,
            underline_color: cell.underline_color,
        }
    }

    // The C reference's color_resolve: bold adds 8 to a palette colour
    // below 8 in the foreground. Colours 8 to 255, RGB, the default
    // foreground, the background and SGR 58's colour stay as set.
    // covers: sgr.bold
    #[test]
    fn bold_brightens_the_first_eight_palette_colours_of_the_foreground() {
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
    // it: the bright colour becomes the background.
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
    /// The C reference's color_dim of `FG` and `BG`: ff/2 + 21/2,
    /// 80/2 + a0/2, 40/2 + c3/2. A rounded mean would give 90 in red.
    const DIM: Rgb = Rgb::new(0x8f, 0x90, 0x81);

    // SGR 2: the foreground halfway to the background, each channel the
    // sum of the two halved; the background and SGR 58's colour stay.
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

    // Dim mixes the colours inverse video and DECSCNM left, so the old
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

    // The selection swaps the colours as inverse video does, and dim mixes
    // what it left; over inverse text the two cancel and dim mixes the
    // colours as set.
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

    // Bold brightens first and dim mixes the bright colour: 9/2 + 21/2,
    // 9/2 + a0/2, 9/2 + c3/2.
    // covers: sgr.bold, sgr.dim
    #[test]
    fn bold_and_dim_dim_the_bright_colour() {
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
    // and repeating, dim and an underline colour, a wide pair, a cluster,
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
}
