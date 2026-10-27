//! Frames: what a renderer reads to draw the screen.
//!
//! A [`Frame`] views the visible lines with their colours resolved through
//! the palette at the time it is drawn, each cell marked changed or not
//! for the display reading it, the cursor, and the selection. Resolving at
//! draw time is what lets a palette change recolour text already on the
//! screen.

use core::ops::Range;

use crate::cell::Width;
use crate::damage::{Age, Damage};
use crate::line::{Glyph, Line, LinkId};
use crate::style::{Attrs, Color, Rgb};

/// The 256-colour palette and the default colours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    colors: [Rgb; 256],
    fg: Rgb,
    bg: Rgb,
}

impl Palette {
    pub fn new(colors: [Rgb; 256], fg: Rgb, bg: Rgb) -> Self {
        Self { colors, fg, bg }
    }

    pub fn color(&self, index: u8) -> Rgb {
        self.colors
            .get(usize::from(index))
            .copied()
            .unwrap_or_default()
    }

    pub fn set_color(&mut self, index: u8, rgb: Rgb) {
        if let Some(c) = self.colors.get_mut(usize::from(index)) {
            *c = rgb;
        }
    }

    pub fn fg(&self) -> Rgb {
        self.fg
    }

    pub fn bg(&self) -> Rgb {
        self.bg
    }

    pub fn set_fg(&mut self, rgb: Rgb) {
        self.fg = rgb;
    }

    pub fn set_bg(&mut self, rgb: Rgb) {
        self.bg = rgb;
    }

    fn resolve(&self, color: Color, default: Rgb) -> Rgb {
        match color {
            Color::Default => default,
            Color::Indexed(i) => self.color(i),
            Color::Rgb(rgb) => rgb,
        }
    }
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
    /// The foreground after the palette and inverse video.
    pub fg: Rgb,
    /// The background after the palette and inverse video.
    pub bg: Rgb,
    /// The underline colour SGR 58 set, resolved; `None` follows the
    /// configured underline colour, or the foreground.
    pub underline_color: Option<Rgb>,
    /// SGR's attributes, which a renderer draws: bold, dim, blink,
    /// conceal, the underline style, overline, strike, the script.
    pub attrs: Attrs,
    /// The background is the default one and not inverted: an image placed
    /// below cell backgrounds shows through (libtsm#35).
    pub default_bg: bool,
    pub link: Option<LinkId>,
    /// Inside the selection, which draws inverted.
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
        let style = line.style(cell);
        let mut fg = self.palette.resolve(style.fg, self.palette.fg);
        let mut bg = self.palette.resolve(style.bg, self.palette.bg);
        let selected = self.selection.get(row).is_some_and(|r| r.contains(&col));
        let inverse = (style.attrs.contains(Attrs::INVERSE) != self.reverse) != selected;
        if inverse {
            core::mem::swap(&mut fg, &mut bg);
        }
        let underline_color = match style.underline_color {
            Color::Default => None,
            c => Some(self.palette.resolve(c, fg)),
        };
        Some(FrameCell {
            glyph: line.glyph(cell),
            width: cell.width(),
            fg,
            bg,
            underline_color,
            attrs: style.attrs,
            default_bg: style.bg == Color::Default && !inverse,
            link: line.link(cell),
            selected,
            changed: self.damage.changed(row, col, self.since),
        })
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::line::LineId;
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
