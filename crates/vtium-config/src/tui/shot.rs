//! The TUI's previews (ADR-1201 D4), as the state machine's: a picture of
//! the console with the highlighted change applied, which leaves the live
//! console alone, and the live trial, which switches the live console to
//! the highlighted font on `t` and puts it back after a countdown unless
//! the user keeps it with `k`.
//!
//! A picture is asked for in two steps. The TUI first asks the glyph
//! helper for the font's cell at the change's size ([`Effect::Cell`]),
//! since the cell decides the grid the change gives on the display. It
//! then asks the loop to draw its own screen at that grid and to ask for
//! the picture of it ([`Effect::ScreenPicture`]); the Looks view's sample
//! screen goes in the request itself ([`Effect::Picture`]). Where the
//! terminal draws no kitty graphics, it asks for a line of text instead
//! ([`Effect::Summary`]).

use std::time::Duration;

use vtium_font::Cell;

use super::input::Key;
use super::model::{App, Effect, Font, FontState, Live, Shared, Size, View};
use super::words;
use crate::specimen::Shot;

/// How many ticks the live trial counts down before it puts the font
/// back, one a second (the agent's call, ADR-1201 D4).
pub const TRIAL_TICKS: u32 = 10;

/// One tick of the live trial's countdown: a wait of the TUI's loop.
pub const TICK: Duration = Duration::from_secs(1);

/// The display a picture shows where vtium-config finds none connected:
/// the VGA console's 640 by 480.
pub const ASSUMED_DISPLAY: (u32, u32) = (640, 480);

/// The screen the Looks view's preview draws, in pixels, whatever the real
/// display's size: a nominal screen of sample content from its top left.
/// The preview shows it as the Fonts view shows the detailed specimen: one
/// pixel of the console to one pixel of the picture, as many whole
/// character cells of the sample content as the preview area holds, so a
/// bigger size gives a bigger picture. The glyphs keep the cell the look
/// gets on the real display.
pub const PREVIEW_SCREEN: (u32, u32) = (1024, 768);

/// The sample content of the Looks view's preview, one line a row, each
/// run of text with its style: a shell prompt, a short listing, the eight
/// colors and their bright forms, bold and italic text, box drawing and
/// accented letters. It holds no wide character: the picture of a screen
/// gives each character one cell, and a wide one needs two. It stays fixed, so the pictures stay comparable.
pub fn sample_rows() -> Vec<Vec<(&'static str, super::color::Style)>> {
    use super::color::Color;
    use super::color::Style;
    let c = Style::text;
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    let italic = Style {
        italic: true,
        ..Style::PLAIN
    };
    vec![
        vec![
            ("user@desk", c(Color::BrightGreen)),
            (":", Style::PLAIN),
            ("~/notes", c(Color::Cyan)),
            ("$ ls -l", Style::PLAIN),
        ],
        vec![
            ("drwxr-xr-x  2 user user 4096 Nov 10 09:12 ", Style::PLAIN),
            ("drafts", c(Color::Cyan)),
        ],
        vec![(
            "-rw-r--r--  1 user user 1823 Nov 10 09:14 plan.txt",
            Style::PLAIN,
        )],
        vec![
            ("-rwxr-xr-x  1 user user  512 Nov  9 17:40 ", Style::PLAIN),
            ("build.sh", c(Color::BrightGreen)),
        ],
        vec![
            ("user@desk", c(Color::BrightGreen)),
            (":", Style::PLAIN),
            ("~/notes", c(Color::Cyan)),
            ("$ colors", Style::PLAIN),
        ],
        vec![
            ("red ", c(Color::Red)),
            ("green ", c(Color::Green)),
            ("yellow ", c(Color::Yellow)),
            ("magenta ", c(Color::Magenta)),
            ("cyan ", c(Color::Cyan)),
            ("white ", c(Color::White)),
        ],
        vec![
            ("grey ", c(Color::Grey)),
            ("red ", c(Color::BrightRed)),
            ("green ", c(Color::BrightGreen)),
            ("yellow ", c(Color::BrightYellow)),
            ("magenta ", c(Color::BrightMagenta)),
            ("cyan ", c(Color::BrightCyan)),
            ("white", c(Color::BrightWhite)),
        ],
        vec![
            ("Bold text", bold),
            (" and ", Style::PLAIN),
            ("italic text", italic),
            (" beside plain text.", Style::PLAIN),
        ],
        vec![("┌──────────┬──────────┐", Style::PLAIN)],
        vec![("│ name     │ size     │", Style::PLAIN)],
        vec![("├──────────┼──────────┤", Style::PLAIN)],
        vec![("│ plan.txt │ 1.8 KiB  │", Style::PLAIN)],
        vec![("└──────────┴──────────┘", Style::PLAIN)],
        vec![(
            "Accents: café, naïve, Ångström, Straße, žluťoučký",
            Style::PLAIN,
        )],
        vec![
            ("user@desk", c(Color::BrightGreen)),
            (":", Style::PLAIN),
            ("~/notes", c(Color::Cyan)),
            ("$ ", Style::PLAIN),
        ],
    ]
}

/// The Looks view's preview screen laid out at `grid`, its sample content
/// from the top left.
fn sample_screen((cols, rows): (usize, usize)) -> Vec<super::canvas::Row> {
    let mut canvas = super::canvas::Canvas::new(cols, rows);
    for (y, line) in sample_rows().into_iter().enumerate().take(rows) {
        let mut x = 0usize;
        for (text, style) in line {
            x = canvas.put(x, y, text, style);
        }
    }
    canvas.into_rows()
}

/// The most columns and rows a picture's screen is laid out at, whatever
/// the cell: past this the screen's text is more than a picture holds.
pub const GRID_MAX: (usize, usize) = (1024, 1024);

/// The live trial: the font tried, the font it puts back, and the ticks
/// left before it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trial {
    pub font: String,
    pub back: String,
    pub left: u32,
}

/// What a picture of the console shows: the font, its size in points, and
/// a cell that a look states, which sets the cell when the size is
/// automatic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub font: String,
    pub points: u32,
    pub cell: Option<Cell>,
    /// The detailed specimen of the font in place of the picture of the
    /// console, which `s` chooses in the Fonts view.
    pub detail: bool,
}

/// The picture or line asked for last, and the terminal it was asked
/// for: its grid and its cell in pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub wanted: Wanted,
    pub grid: (usize, usize),
    pub cell_px: Option<(u32, u32)>,
    pub graphics: bool,
}

/// The line beside a picture: the display, the size, the cell and the
/// grid it gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caption {
    pub font: String,
    pub points: u32,
    pub display: (u32, u32),
    pub assumed: bool,
    pub cell: Cell,
    pub grid: (usize, usize),
}

/// The cells a picture on the screen covers: its first row and column,
/// counted from 0, and how many columns and rows it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Covered {
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
}

impl Covered {
    /// The cells a picture of `pixels` covers from the cell `place`, by
    /// the cell's size in pixels that `size` holds, and at most the
    /// picture's area `area`, in columns and rows. Where the terminal
    /// reports no size in pixels, the cell is the one the TUI assumes, as
    /// the picture was sized by.
    pub fn at(
        (row, col): (usize, usize),
        (area_cols, area_rows): (usize, usize),
        (width, height): (u32, u32),
        size: Size,
    ) -> Covered {
        let (cw, ch) = size.cell_px.unwrap_or(super::model::ASSUMED_CELL);
        let cells = |pixels: u32, cell: u32| {
            usize::try_from(pixels.div_ceil(cell.max(1))).unwrap_or(usize::MAX)
        };
        Covered {
            row,
            col,
            cols: cells(width, cw).min(area_cols),
            rows: cells(height, ch).min(area_rows),
        }
    }
}

/// The parts of the model that a view's preview reads: the open view, the
/// shared part and the terminal's size, whether the Fonts tab shows the
/// font's detailed specimen, and the size the Looks tab highlights. The
/// font the preview shows comes from the shared part, which the Fonts tab
/// keeps on its highlight. Update and the layout both read the preview
/// through this, and neither changes it.
#[derive(Clone, Copy, Debug)]
pub struct Preview<'a> {
    pub view: View,
    pub shared: &'a Shared,
    pub size: Size,
    /// The Fonts view shows the detailed specimen of the font.
    pub detail: bool,
    /// The size the Looks view highlights, in points, 0 for automatic.
    pub points: u32,
}

impl<'a> Preview<'a> {
    /// The Fonts view's preview: the font the previews show, at the
    /// draft's size, as its detailed specimen where `detail` says so.
    pub fn fonts(shared: &'a Shared, size: Size, detail: bool) -> Preview<'a> {
        Preview {
            view: View::Fonts,
            shared,
            size,
            detail,
            points: 0,
        }
    }

    /// The Looks view's preview: the font the previews show, at `points`,
    /// 0 for automatic.
    pub fn looks(shared: &'a Shared, size: Size, points: u32) -> Preview<'a> {
        Preview {
            view: View::Looks,
            shared,
            size,
            detail: false,
            points,
        }
    }

    /// The font the preview shows.
    pub fn font(&self) -> Option<&'a Font> {
        self.shared.preview_font()
    }

    /// The change the view previews now, if any: the highlighted font in
    /// the Fonts view at the draft's size; the highlighted size of the
    /// highlighted font in the Looks view, automatic having none; and on a
    /// look's card in the Flow view, the look with the value under the
    /// cursor of its frit, size or cell.
    pub fn wanted(&self) -> Option<Wanted> {
        if super::layout::too_small(self.size) {
            return None;
        }
        let readable = |name: &str| {
            self.shared
                .inputs
                .fonts
                .iter()
                .find(|f| f.name == name)
                .is_some_and(|f| !matches!(f.state, FontState::Refused(_)))
        };
        match self.view {
            View::Fonts => {
                let font = self.font()?.name.clone();
                readable(&font).then(|| Wanted {
                    font,
                    points: draft_points(self.shared),
                    cell: None,
                    detail: self.detail,
                })
            }
            View::Looks => {
                let font = self.font()?.name.clone();
                (readable(&font) && self.points > 0).then_some(Wanted {
                    font,
                    points: self.points,
                    cell: None,
                    detail: false,
                })
            }
            _ => None,
        }
    }

    /// The line that describes the preview: beside a picture, what the
    /// picture shows; with no pictures, the summary of the change.
    pub fn preview_line(&self) -> Option<String> {
        if self.shared.text_preview() && !super::layout::too_small(self.size) {
            return match self.view {
                View::Fonts => Some(words::console_font_caption(
                    self.shared.shown_font.as_deref(),
                )),
                View::Looks => Some(words::CONSOLE_SIZE_CAPTION.to_owned()),
                _ => None,
            };
        }
        let wanted = self.wanted()?;
        if self.shared.graphics && wanted.detail {
            let height = height_of(self.shared, &wanted);
            let (display, assumed) = display(self.shared);
            let summary = self
                .shared
                .specimen
                .as_ref()
                .filter(|(f, h, _)| *f == wanted.font && *h == height)
                .map(|(_, _, s)| s);
            return Some(words::detail_caption(
                &wanted.font,
                wanted.points,
                summary,
                display,
                assumed,
            ));
        }
        if self.shared.graphics {
            return self.shared.shot.as_ref().map(words::caption);
        }
        let height = height_of(self.shared, &wanted);
        let summary = self
            .shared
            .summary
            .as_ref()
            .filter(|(f, h, _)| *f == wanted.font && *h == height)
            .map(|(_, _, s)| match s {
                Ok(s) => words::size_summary(s),
                Err(why) => words::no_summary(why),
            });
        Some(match summary {
            Some(s) => format!("{} {s}", words::NO_PICTURES),
            None => words::NO_PICTURES.to_owned(),
        })
    }
}

/// The pixel height `wanted` asks the font for: its cell's height where it
/// states a cell, else its size at the density `shared` holds.
pub fn height_of(shared: &Shared, wanted: &Wanted) -> u32 {
    match wanted.cell {
        Some(c) => c.height,
        None => crate::specimen::pixel_height(wanted.points, shared.inputs.dpi.0),
    }
}

/// The draft's `font-size`, else the size `preview` draws an automatic
/// size at.
fn draft_points(shared: &Shared) -> u32 {
    shared
        .drafted("font-size")
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(crate::preview::POINTS)
}

/// The display the picture shows, and whether it is assumed.
fn display(shared: &Shared) -> ((u32, u32), bool) {
    match shared.inputs.display {
        Some(d) => (d, false),
        None => (ASSUMED_DISPLAY, true),
    }
}

/// The most pixels the picture takes: `area`, the picture area in the
/// terminal's cells, at the cell's size in pixels that `size` holds.
fn picture_max((cols, rows): (usize, usize), size: Size) -> (u32, u32) {
    let (cw, ch) = size.cell_px.unwrap_or(super::model::ASSUMED_CELL);
    (
        u32::try_from(cols)
            .unwrap_or(0)
            .saturating_mul(cw)
            .min(crate::specimen::PICTURE_MAX.0),
        u32::try_from(rows)
            .unwrap_or(0)
            .saturating_mul(ch)
            .min(crate::specimen::PICTURE_MAX.1),
    )
}

impl App {
    /// The parts of the model that the open view's preview reads.
    pub fn preview(&self) -> Preview<'_> {
        Preview {
            view: self.nav.view,
            shared: &self.shared,
            size: self.size,
            detail: self.fonts.detail(),
            points: self.looks.points(),
        }
    }

    /// The most pixels the picture takes in the open view.
    fn picture_room(&self) -> (u32, u32) {
        picture_max(super::layout::picture_area(self), self.size)
    }

    /// Asks for the preview of what the view shows now, where it is not
    /// the one asked for last: the font's cell, toward a picture, where
    /// the terminal draws kitty graphics, and a line of text where it does
    /// not. With nothing to preview, the picture on the screen is deleted.
    pub(crate) fn ask_picture(&mut self) -> Vec<Effect> {
        if self.shared.live == Live::Asking
            || self.shared.leaving.is_some()
            || self.shared.installing
        {
            return Vec::new();
        }
        // in a vtium console the console draws the preview as text, so the
        // helper draws nothing
        if self.shared.text_preview() {
            self.shared.asked = None;
            self.shared.shot = None;
            return self.drop_picture();
        }
        let asked = self.preview().wanted().map(|wanted| Asked {
            wanted,
            grid: (self.size.cols, self.size.rows),
            cell_px: self.size.cell_px,
            graphics: self.shared.graphics,
        });
        if asked == self.shared.asked || self.shared.drawing {
            return Vec::new();
        }
        self.shared.asked = asked.clone();
        let Some(asked) = asked else {
            self.shared.shot = None;
            return self.drop_picture();
        };
        let height = height_of(&self.shared, &asked.wanted);
        let Wanted {
            font,
            points,
            cell,
            detail,
        } = asked.wanted;
        if !self.shared.graphics {
            self.shared.drawing = true;
            return vec![Effect::Summary { font, height }];
        }
        let max = self.picture_room();
        if max.0 == 0 || max.1 == 0 {
            self.shared.shot = None;
            return self.drop_picture();
        }
        self.shared.drawing = true;
        if detail {
            self.shared.shot = None;
            let display = display(&self.shared).0;
            return vec![Effect::Specimen {
                font,
                height,
                max,
                display,
            }];
        }
        match cell {
            Some(cell) => self.picture_of(font, points, height, (cell, cell.height)),
            None => vec![Effect::Cell { font, height }],
        }
    }

    /// The font's cell has come: the picture of the screen at the grid it
    /// gives, where it is still the one wanted.
    pub(crate) fn celled(
        &mut self,
        font: &str,
        height: u32,
        cell: Result<(Cell, u32), String>,
    ) -> Vec<Effect> {
        let Some(asked) = self.shared.asked.clone() else {
            self.shared.drawing = false;
            return Vec::new();
        };
        let wanted = asked.wanted;
        // a cell for a change no longer highlighted: the next is asked for
        if wanted.font != font
            || height_of(&self.shared, &wanted) != height
            || wanted.cell.is_some()
            || self.preview().wanted().as_ref() != Some(&wanted)
            || self.shared.leaving.is_some()
        {
            self.shared.drawing = false;
            return Vec::new();
        }
        match cell {
            Ok(cell) => self.picture_of(wanted.font, wanted.points, height, cell),
            Err(why) => {
                self.shared.drawing = false;
                self.shared.shot = None;
                self.say_unless_trial(words::no_picture(why));
                self.drop_picture()
            }
        }
    }

    /// The picture of the TUI's screen laid out at the grid `cell` gives
    /// on the display, in `font` at `height` pixels.
    fn picture_of(
        &mut self,
        font: String,
        points: u32,
        height: u32,
        (cell, face_height): (Cell, u32),
    ) -> Vec<Effect> {
        let (display, assumed) = display(&self.shared);
        let across =
            |px: u32, side: u32| usize::try_from(px.checked_div(side).unwrap_or(0)).unwrap_or(0);
        let grid = (
            across(display.0, cell.width).min(GRID_MAX.0),
            across(display.1, cell.height).min(GRID_MAX.1),
        );
        self.shared.shot = Some(Caption {
            font: font.clone(),
            points,
            display,
            assumed,
            cell,
            grid,
        });
        // the Looks view draws a nominal screen of sample content at the
        // look's cell; the caption still counts the real display's grid.
        // Elsewhere the loop draws the TUI's own screen at the grid.
        let looks = self.nav.view == View::Looks;
        let (display, grid, screen) = if looks {
            let shown = (
                across(PREVIEW_SCREEN.0, cell.width).min(GRID_MAX.0),
                across(PREVIEW_SCREEN.1, cell.height).min(GRID_MAX.1),
            );
            (PREVIEW_SCREEN, shown, sample_screen(shown))
        } else {
            (display, grid, Vec::new())
        };
        let palette = self
            .conf_text()
            .and_then(|c| vtium_settings::setting(c, "palette"))
            .unwrap_or_default();
        let shot = Shot {
            font,
            height,
            cell,
            face_height,
            display,
            grid,
            screen,
            palette,
            color: self.shared.inputs.color,
            max: self.picture_room(),
        };
        vec![if looks {
            Effect::Picture(shot)
        } else {
            Effect::ScreenPicture(shot)
        }]
    }

    /// Deletes the picture on the screen, if any.
    pub(crate) fn drop_picture(&mut self) -> Vec<Effect> {
        if self.shared.picture.take().is_some() {
            vec![Effect::Write(crate::kitty::delete(crate::kitty::IMAGE_ID))]
        } else {
            Vec::new()
        }
    }

    /// The picture has come: it replaces the one on the screen, at its
    /// place in the view.
    pub(crate) fn pictured(
        &mut self,
        result: Result<crate::specimen::Picture, String>,
    ) -> Vec<Effect> {
        self.shared.drawing = false;
        let mut out = self.drop_picture();
        // a picture of a change no longer highlighted is not shown: the
        // next is asked for
        let current = self
            .shared
            .asked
            .as_ref()
            .is_some_and(|a| self.preview().wanted().as_ref() == Some(&a.wanted));
        if !matches!(self.nav.view, View::Looks | View::Fonts | View::Flow)
            || self.shared.leaving.is_some()
            || !current
        {
            return out;
        }
        match result {
            Ok(p) => {
                let (row, col) = super::layout::picture_at(self);
                let mut bytes = format!("\x1b[{row};{col}H").into_bytes();
                bytes.extend(crate::kitty::show(
                    crate::kitty::IMAGE_ID,
                    p.width,
                    p.height,
                    &p.rgba,
                ));
                self.shared.picture = Some(Covered::at(
                    (row.saturating_sub(1), col.saturating_sub(1)),
                    super::layout::picture_area(self),
                    (p.width, p.height),
                    self.size,
                ));
                out.push(Effect::Write(bytes));
            }
            Err(why) => {
                self.shared.shot = None;
                self.say_unless_trial(words::no_picture(why));
            }
        }
        out
    }

    /// Puts `line` on the message line, unless the live trial's countdown
    /// holds it.
    fn say_unless_trial(&mut self, line: String) {
        if self.nav.trial.is_none() {
            self.shared.message = Some(line);
        }
    }

    /// The detailed specimen has come: what it gives is kept for the line
    /// beside it, and it is shown as a picture is.
    pub(crate) fn detailed(
        &mut self,
        font: String,
        height: u32,
        result: Result<(crate::specimen::Picture, crate::specimen::Summary), String>,
    ) -> Vec<Effect> {
        match result {
            Ok((picture, summary)) => {
                self.shared.specimen = Some((font, height, summary));
                self.pictured(Ok(picture))
            }
            Err(why) => self.pictured(Err(why)),
        }
    }

    /// `t`: switches the live console to `name` and starts the countdown
    /// that puts the font it drew back (D4), where the console can be
    /// switched.
    pub(crate) fn try_live(&mut self, name: String) -> Vec<Effect> {
        if self.shared.live != Live::On {
            self.shared.message = Some(words::TRIAL_OFF.to_owned());
            return Vec::new();
        }
        if self.shared.refused.contains(&name) {
            self.shared.message = Some(words::KEPT.to_owned());
            return Vec::new();
        }
        let back = self
            .shared
            .want
            .clone()
            .or_else(|| self.shared.shown_font.clone())
            .unwrap_or_default();
        if back == name {
            self.shared.message = Some(words::trial_same(&name));
            return Vec::new();
        }
        self.nav.trial = Some(Trial {
            font: name.clone(),
            back,
            left: TRIAL_TICKS,
        });
        self.shared.want = Some(name.clone());
        self.shared.message = Some(words::trial_countdown(&name, TRIAL_TICKS));
        let mut out = self.follow();
        out.push(Effect::Wait(TICK));
        out
    }

    /// A key while the live trial counts down: `k` keeps the font, Ctrl-C
    /// leaves, and any other key puts the font back and does nothing else.
    pub(crate) fn trial_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Char('k') => {
                if let Some(t) = self.nav.trial.take() {
                    self.shared.message = Some(words::trial_kept(&t.font));
                }
                Vec::new()
            }
            _ => self.put_back(),
        }
    }

    /// Ends the live trial and switches the console back to the font it
    /// drew before it.
    pub(crate) fn put_back(&mut self) -> Vec<Effect> {
        let Some(t) = self.nav.trial.take() else {
            return Vec::new();
        };
        self.shared.message = Some(words::trial_back(&t.back));
        self.shared.want = Some(t.back);
        self.follow()
    }

    /// One tick of the live trial's countdown: the next wait, or, at its
    /// end, the font put back.
    pub(crate) fn tick(&mut self) -> Vec<Effect> {
        let Some(t) = self.nav.trial.as_mut() else {
            return Vec::new();
        };
        t.left = t.left.saturating_sub(1);
        if t.left == 0 {
            return self.put_back();
        }
        let (font, left) = (t.font.clone(), t.left);
        self.shared.message = Some(words::trial_countdown(&font, left));
        vec![Effect::Wait(TICK)]
    }
}
