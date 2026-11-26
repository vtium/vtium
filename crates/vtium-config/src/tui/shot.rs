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
//!
//! The Effects tab's preview is the Looks view's sample screen drawn
//! through the highlighted effect ([`Source::Effect`]). The state machine
//! decides from what the tab lists whether vtium can use the effect: a
//! color map that is not active, or a shader, leaves the picture as it is,
//! and the line beside the picture says why. Otherwise the request carries
//! the effect, and the loop draws the picture through it as the console's
//! CPU path draws a frame ([`crate::specimen::through`]). The loop answers
//! with [`super::model::Event::Filtered`], which says whether the effect
//! was applied. The Effects tab asks for a picture in a vtium console too,
//! since the console's own text cannot show an effect.
//!
//! References:
//! - ADR-1205 D8: the Effects tab previews an effect as a picture of the console with the effect applied, and the preview leaves the live console alone.

use std::time::Duration;

use vtium_font::Cell;

use super::effects_found::{Effects, Vtlook};
use super::input::Key;
use super::model::{App, Effect, Font, FontState, Live, Shared, Size};
use super::resources::Resource;
use super::words;
use crate::specimen::{Filter, Filtered, Outcome, Shot};

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
/// The preview shows it as the Fonts tab shows the detailed specimen: one
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
/// Each row holds `cols` characters: the sample's text, cut at the right
/// edge, then blanks.
fn sample_screen((cols, rows): (usize, usize)) -> Vec<super::canvas::Row> {
    use super::canvas::{Row, put};
    let lines = sample_rows();
    (0..rows)
        .map(|y| {
            let mut row: Row = Vec::new();
            put(&mut row, 0, &" ".repeat(cols), super::color::Style::PLAIN);
            let mut x = 0usize;
            for (text, style) in lines.get(y).into_iter().flatten() {
                let shown: String = text.chars().take(cols.saturating_sub(x)).collect();
                x = put(&mut row, x, &shown, *style);
            }
            row
        })
        .collect()
}

/// The most columns and rows a picture's screen is laid out at, whatever
/// the cell: past this the screen's text is more than a picture holds.
pub const GRID_MAX: (usize, usize) = (1024, 1024);

/// The live trial: the font tried, the font it puts back, and the ticks
/// left before it does. A trial of a size leaves the font as it is, so
/// its font and the font it puts back are the same, and `grid` holds the
/// grid it asked the console for. A trial of a look holds what it tries
/// beside the font in `look`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trial {
    pub font: String,
    pub back: String,
    pub left: u32,
    pub grid: Option<GridTrial>,
    pub look: Option<LookParts>,
}

/// What the trial of a look tries beside its font set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookParts {
    /// The look's name.
    pub name: String,
    /// The trial set the console's colors to the look's color scheme.
    pub colors: bool,
    /// The look's size in points, still to try once the console draws
    /// the look's font set.
    pub size: Option<u32>,
    /// What of the look waits for vtium's next start, as the countdown
    /// says it.
    pub later: Option<String>,
    /// No terminal of the draft uses the look, and this is the terminal
    /// that `k` offers it to: the console's own, or the draft's only one.
    pub offer: Option<String>,
}

/// The bytes that set the console's sixteen colors and its default
/// foreground and background to `palette`: OSC 4 for colors 0 to 15, then
/// OSC 10 and OSC 11. Colors 16 to 255 are the same in every color
/// scheme, so the request leaves them alone.
///
/// References:
/// - ADR-1000, the rows `color.palette.set` and `color.dynamic.set`: vtium
///   sets its colors on OSC 4, 10 and 11.
pub fn colors_request(palette: &vtium_term::Palette) -> Vec<u8> {
    use std::fmt::Write as _;
    let spec = |c: vtium_term::Rgb| format!("rgb:{:02x}/{:02x}/{:02x}", c.r, c.g, c.b);
    let mut out = String::from("\x1b]4");
    for i in 0..16u8 {
        let _ = write!(out, ";{i};{}", spec(palette.color(i)));
    }
    let _ = write!(
        out,
        "\x1b\\\x1b]10;{}\x1b\\\x1b]11;{}\x1b\\",
        spec(palette.fg()),
        spec(palette.bg())
    );
    out.into_bytes()
}

/// The bytes that give the console back the colors its configuration
/// sets: OSC 104, 110 and 111 with no color named reset every color, the
/// default foreground and the default background.
///
/// References:
/// - ADR-1000, the rows `color.palette.set` and `color.dynamic.set`: OSC
///   104, 110 and 111 reset the colors to the configured ones.
pub const COLORS_RESET: &[u8] = b"\x1b]104\x1b\\\x1b]110\x1b\\\x1b]111\x1b\\";

/// The live trial's countdown for the font set `font` with `left` seconds
/// to go. The console reads only installed font sets, so on a font set the
/// draft replaces the countdown also says that the console shows the
/// installed version.
///
/// References:
/// - ADR-1403 D8, "The live trial": `t` tries an installed frit, since the console reads frits only from the frits' directory.
fn countdown(shared: &Shared, font: &str, left: u32) -> String {
    if super::tabs::looks::fontsets::replaced(shared, font) {
        words::trial_countdown_installed(font, left)
    } else {
        words::trial_countdown(font_shown(font), left)
    }
}

/// Whether `name`, the text of a font switch, is a font file's path. The
/// Fonts tab's `t` sends a path, and the Looks tab's `t` sends the name of
/// a font set or a built-in font, which never holds a `/`.
pub fn is_file(name: &str) -> bool {
    name.contains('/')
}

/// The font `name` as the message line names it: a font file by its
/// file's name, since its whole path would fill much of the message
/// line, and a font set or a built-in font by its own name.
pub fn font_shown(name: &str) -> String {
    match std::path::Path::new(name).file_name() {
        Some(file) if is_file(name) => crate::onscreen::text(&file.to_string_lossy()),
        _ => name.to_owned(),
    }
}

/// A size tried on the console: the size in points, the grid it gives on
/// the display, and the grid the console had before, which the trial puts
/// back. Each grid is columns by rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridTrial {
    pub points: u32,
    pub to: (usize, usize),
    pub back: (usize, usize),
}

/// The size a trial waits for the font helper to measure: the font set,
/// the pixel height and the size in points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SizeAsked {
    pub font: String,
    pub height: u32,
    pub points: u32,
}

/// The grid request, `CSI 8 ; rows ; cols t`, which asks the console for a
/// grid of `cols` by `rows`. vtium meets it by changing the font size, and
/// only while `allow-window-ops` is on.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": the Looks tab changes
///   the console's size with the grid request.
/// - ADR-1000, the row `window.resize`: vtium honours the request under
///   `allow-window-ops`.
pub fn grid_request((cols, rows): (usize, usize)) -> Vec<u8> {
    format!("\x1b[8;{rows};{cols}t").into_bytes()
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
    /// console.
    pub detail: bool,
    /// The effect the picture is drawn through, which the Effects tab
    /// previews.
    pub filter: Option<Filtered>,
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
    /// What became of the effect the picture is drawn through, once the
    /// picture has come, with the preview it answers, so the line never
    /// gives one item's outcome beside another.
    pub outcome: Option<(Wanted, Outcome)>,
}

/// Why no picture is on the screen for the preview asked for last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unpictured {
    /// The view leaves no room for it.
    NoRoom,
    /// Measuring the font or painting the picture failed, and the
    /// message line says why.
    Failed,
}

/// The cells under a picture on the screen: the picture's first row and
/// column, counted from 0, and how many columns and rows it takes.
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

/// What a tab previews, and in which font. A tab that previews chooses its
/// own font, so no tab reads another tab's highlight.
#[derive(Clone, Copy, Debug)]
pub enum Source<'a> {
    /// The tab previews nothing.
    Nothing,
    /// `font` at the draft's size: its detailed specimen where `detail` is
    /// set, and otherwise the picture of the TUI's screen drawn in it.
    Font {
        font: Option<&'a Font>,
        detail: bool,
    },
    /// `font` at `points` on the sample screen of [`PREVIEW_SCREEN`]
    /// pixels. A size of 0 is automatic, which has no preview.
    Size { font: Option<&'a Font>, points: u32 },
    /// `font` at the draft's size on the sample screen, drawn through
    /// `effect`.
    Effect {
        font: Option<&'a Font>,
        effect: Applied<'a>,
    },
}

/// The item the Effects tab highlights, as its preview applies it.
#[derive(Clone, Copy, Debug)]
pub enum Applied<'a> {
    /// The filter line, which applies no effect.
    Nothing,
    Vtlook(&'a Vtlook),
    Map(&'a Resource),
    Shader(&'a Resource),
}

impl Applied<'_> {
    /// The effect the picture is drawn through, with the color maps
    /// active in the draft; `Ok(None)` for one that changes no color; or
    /// why vtium cannot use it, so the picture shows the console without
    /// it. The reasons are the ones the list's marks show: a color map
    /// that is not active or fails the trust rule counts as missing, and
    /// a vtlook that does not read, or names such a color map, gives way
    /// to the built-in vtlook.
    ///
    /// References:
    /// - ADR-1205 D4: a resource the batch does not list, or that fails the trust rule, counts as missing.
    /// - ADR-1304 D6: a vtlook that fails at any stage gives way to the built-in vtlook.
    pub fn filter(&self, effects: &Effects) -> Result<Option<Filtered>, String> {
        let filtered = |filter| {
            Ok(Some(Filtered {
                filter,
                active: effects
                    .maps
                    .iter()
                    .filter(|r| r.active() && !r.own)
                    .map(|r| r.path.clone())
                    .collect(),
            }))
        };
        match *self {
            Applied::Nothing => Ok(None),
            Applied::Shader(_) => Err(words::effect_without("shader", words::EFFECT_SHADER)),
            Applied::Map(r) => match unusable(r) {
                Some(why) => Err(words::effect_without(
                    "color map",
                    &words::map_unusable(why),
                )),
                None => filtered(Filter::Cube(r.path.clone())),
            },
            Applied::Vtlook(v) => {
                let Some(file) = &v.file else {
                    return Ok(None);
                };
                if let Err(error) = &v.read {
                    return Err(words::effect_without(
                        "vtlook",
                        &words::effect_vtlook_broken(error),
                    ));
                }
                for named in v
                    .files
                    .iter()
                    .filter(|f| f.extension().is_some_and(|e| e == "cube"))
                {
                    let why = match effects.resource(named) {
                        None => Some(words::MAP_MISSING),
                        Some(r) => unusable(r),
                    };
                    if let Some(why) = why {
                        let name = named.file_name().map_or_else(
                            || named.display().to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        return Err(words::effect_without(
                            "vtlook",
                            &words::vtlook_map_unusable(&name, why),
                        ));
                    }
                }
                filtered(Filter::Vtlook {
                    name: v.name.clone(),
                    file: file.clone(),
                })
            }
        }
    }
}

/// Why vtium cannot use the color map `r`, in the words of its marks, or
/// `None` when it can. Failing the trust rule comes before not being
/// active, since a file that fails the rule cannot be activated.
fn unusable(r: &Resource) -> Option<&'static str> {
    if r.own {
        Some(words::MAP_OWN)
    } else if r.trust.is_err() {
        Some(words::MAP_UNTRUSTED)
    } else if !r.active() {
        Some(words::MAP_INACTIVE)
    } else {
        None
    }
}

/// The parts of the model that the open tab's preview reads: the shared
/// part, the terminal's size, and what the tab previews. Update and the
/// layout both read the preview through this, and neither changes it.
#[derive(Clone, Copy, Debug)]
pub struct Preview<'a> {
    pub shared: &'a Shared,
    pub size: Size,
    pub source: Source<'a>,
    /// The font set the preview stands for, where only the draft adds it.
    /// The console reads only installed font sets, so the caption of the
    /// console's own preview says that `t` can try it after the install.
    pub in_draft: Option<&'a str>,
}

impl<'a> Preview<'a> {
    /// The preview of `source`, on the shared part and a screen of `size`.
    pub fn of(shared: &'a Shared, size: Size, source: Source<'a>) -> Preview<'a> {
        Preview {
            shared,
            size,
            source,
            in_draft: None,
        }
    }

    /// This preview, standing for `name`, a font set that only the draft
    /// adds, where `name` is given.
    pub fn in_draft(self, name: Option<&'a str>) -> Preview<'a> {
        Preview {
            in_draft: name,
            ..self
        }
    }

    /// The preview of a tab that previews nothing.
    pub fn nothing(shared: &'a Shared, size: Size) -> Preview<'a> {
        Preview::of(shared, size, Source::Nothing)
    }

    /// The font the preview shows.
    pub fn font(&self) -> Option<&'a Font> {
        match self.source {
            Source::Font { font, .. } | Source::Size { font, .. } | Source::Effect { font, .. } => {
                font
            }
            Source::Nothing => None,
        }
    }

    /// Whether the tab previews as a picture in a vtium console as well,
    /// where the other tabs preview as the console's own text: the
    /// Effects tab does, since the console's text cannot show an effect,
    /// and so does a font file of the Fonts tab, which the console cannot
    /// draw its own text in.
    pub fn pictured_in_console(&self) -> bool {
        matches!(self.source, Source::Effect { .. })
            || self
                .font()
                .is_some_and(|f| super::fonts_found::is_file_name(&f.name))
    }

    /// Whether the picture shows the sample screen, which the Looks tab
    /// draws at a size's cell, in place of the TUI's own screen.
    pub fn sample(&self) -> bool {
        matches!(self.source, Source::Size { .. } | Source::Effect { .. })
    }

    /// The change the tab previews now, if any: a font at the draft's
    /// size, or a font at a size, automatic having none.
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
                || (super::fonts_found::is_file_name(name)
                    && self
                        .shared
                        .inputs
                        .font_files
                        .file(std::path::Path::new(name))
                        .is_some())
        };
        match self.source {
            Source::Font { detail, .. } => {
                let font = self.font()?.name.clone();
                readable(&font).then(|| Wanted {
                    font,
                    points: draft_points(self.shared),
                    cell: None,
                    detail,
                    filter: None,
                })
            }
            Source::Size { points, .. } => {
                let font = self.font()?.name.clone();
                (readable(&font) && points > 0).then_some(Wanted {
                    font,
                    points,
                    cell: None,
                    detail: false,
                    filter: None,
                })
            }
            Source::Effect { effect, .. } => {
                let font = self.font()?.name.clone();
                readable(&font).then(|| Wanted {
                    font,
                    points: draft_points(self.shared),
                    cell: None,
                    detail: false,
                    // an effect vtium cannot use leaves the picture plain
                    filter: effect.filter(&self.shared.inputs.effects).ok().flatten(),
                })
            }
            Source::Nothing => None,
        }
    }

    /// The line beside the Effects tab's picture: what it shows, and why
    /// it shows the console without the effect, where it does. The line
    /// speaks of the picture only while one is on the screen or on its
    /// way, and gives the outcome of the effect only for the preview that
    /// outcome answers. In a vtium console that draws a vtlook of its own,
    /// the line adds that the console draws the picture through that
    /// vtlook too.
    fn effect_line(&self, effect: Applied<'_>) -> Option<String> {
        if super::layout::too_small(self.size) {
            return None;
        }
        if !self.shared.graphics {
            return Some(words::EFFECT_NO_PICTURES.to_owned());
        }
        let Some(wanted) = self.wanted() else {
            return Some(words::EFFECT_NO_FONT.to_owned());
        };
        let asked_now = self
            .shared
            .asked
            .as_ref()
            .is_some_and(|a| a.wanted == wanted);
        if asked_now {
            match self.shared.unpictured {
                Some(Unpictured::NoRoom) => return Some(words::EFFECT_NO_ROOM.to_owned()),
                Some(Unpictured::Failed) => return Some(words::EFFECT_NOT_DRAWN.to_owned()),
                None => {}
            }
        }
        let outcome = self
            .shared
            .shot
            .as_ref()
            .and_then(|c| c.outcome.as_ref())
            .filter(|(answers, _)| *answers == wanted)
            .map(|(_, outcome)| outcome);
        let mut line = match effect.filter(&self.shared.inputs.effects) {
            Err(why) => why,
            Ok(None) => words::EFFECT_PLAIN.to_owned(),
            Ok(Some(_)) => {
                let (shown, what) = match effect {
                    Applied::Vtlook(v) => (words::effect_vtlook(&v.name), "vtlook"),
                    Applied::Map(r) => (words::effect_map(&r.name()), "color map"),
                    Applied::Nothing | Applied::Shader(_) => {
                        (words::EFFECT_PLAIN.to_owned(), "effect")
                    }
                };
                match outcome {
                    Some(Outcome::Failed(why)) => words::effect_failed(what, why),
                    Some(Outcome::ColorOnly) => format!("{shown} {}", words::EFFECT_COLOR_ONLY),
                    Some(Outcome::Applied) | None => shown,
                }
            }
        };
        if let Some(own) = self.shared.console_vtlook() {
            line.push(' ');
            line.push_str(&words::effect_console_vtlook(&own));
        }
        Some(line)
    }

    /// The line that describes the preview: beside a picture, what the
    /// picture shows; with no pictures, the summary of the change.
    pub fn preview_line(&self) -> Option<String> {
        if let Source::Effect { effect, .. } = self.source {
            return self.effect_line(effect);
        }
        if self.shared.text_preview()
            && !self.pictured_in_console()
            && !super::layout::too_small(self.size)
        {
            return match (self.source, self.in_draft) {
                (Source::Font { .. }, Some(name)) => {
                    Some(words::console_caption_in_draft(name, false))
                }
                (Source::Size { .. }, Some(name)) => {
                    Some(words::console_caption_in_draft(name, true))
                }
                (Source::Font { .. }, None) => Some(words::console_font_caption(
                    self.shared.shown_font.as_deref(),
                )),
                (Source::Size { .. }, None) => Some(words::CONSOLE_SIZE_CAPTION.to_owned()),
                (Source::Effect { .. } | Source::Nothing, _) => None,
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

impl Shared {
    /// The vtlook this vtium console draws with, when it is not the
    /// built-in one: the installed vtium.conf's vtlook for the console's
    /// head, from the head's look, else the top level. The console draws
    /// its whole frame through that vtlook, so a picture shown in it is
    /// filtered a second time. `None` outside a vtium console.
    ///
    /// References:
    /// - ADR-1304 D5: a head's vtlook comes from its look, else the top level, else the built-in one.
    pub fn console_vtlook(&self) -> Option<String> {
        if !matches!(
            self.live,
            super::model::Live::On | super::model::Live::OpsOff
        ) {
            return None;
        }
        let conf = self.inputs.conf.as_deref()?;
        let top = vtium_settings::setting(conf, "vtlook");
        let own = self.inputs.head.as_deref().and_then(|head| {
            let config = vtium_settings::sections(conf.as_bytes())?.parsed.config;
            let look = config.heads.iter().find(|h| h.name == head)?.look.clone();
            config
                .looks
                .into_iter()
                .find(|l| l.name == look)
                .and_then(|l| l.vtlook)
        });
        own.or(top)
            .filter(|v| v != vtium_settings::vtlook::DEFAULT && !v.is_empty())
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
    /// The parts of the model that the open tab's preview reads, with
    /// what the tab previews.
    pub fn preview(&self) -> Preview<'_> {
        self.tab().preview(&self.shared, self.size)
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
        // helper draws nothing; an effect still needs a picture
        if self.shared.text_preview() && !self.preview().pictured_in_console() {
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
        self.shared.unpictured = None;
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
            ..
        } = asked.wanted;
        if !self.shared.graphics {
            // an effect has no written description: its line says that
            // this terminal shows no pictures
            if matches!(self.preview().source, Source::Effect { .. }) {
                self.shared.shot = None;
                return self.drop_picture();
            }
            self.shared.drawing = true;
            return vec![Effect::Summary { font, height }];
        }
        let max = self.picture_room();
        if max.0 == 0 || max.1 == 0 {
            self.shared.shot = None;
            self.shared.unpictured = Some(Unpictured::NoRoom);
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
                self.shared.unpictured = Some(Unpictured::Failed);
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
            outcome: None,
        });
        // the Looks view draws a nominal screen of sample content at the
        // look's cell; the caption still counts the real display's grid.
        // Elsewhere the loop draws the TUI's own screen at the grid.
        let looks = self.preview().sample();
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
            filter: self
                .shared
                .asked
                .as_ref()
                .and_then(|a| a.wanted.filter.clone()),
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
        if self.shared.leaving.is_some() || !current {
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
                self.shared.unpictured = Some(Unpictured::Failed);
                self.say_unless_trial(words::no_picture(why));
            }
        }
        out
    }

    /// The picture drawn through an effect has come, with what became of
    /// the effect, which the line beside the picture says. It is shown as
    /// any picture is.
    pub(crate) fn filtered(
        &mut self,
        picture: crate::specimen::Picture,
        outcome: Outcome,
    ) -> Vec<Effect> {
        // the outcome is kept only for the preview it answers: a picture
        // that comes after the highlight moved on says nothing
        let current = self
            .shared
            .asked
            .as_ref()
            .map(|a| a.wanted.clone())
            .filter(|w| self.preview().wanted().as_ref() == Some(w));
        if let (Some(wanted), Some(caption)) = (current, self.shared.shot.as_mut()) {
            caption.outcome = Some((wanted, outcome));
        }
        self.pictured(Ok(picture))
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
    /// switched. `name` is a font set's or a built-in font's name from the
    /// Looks tab, or a font file's path from the Fonts tab, and both run
    /// the same trial. A font the console refused before is not sent again.
    ///
    /// References:
    /// - ADR-1200 D5, "Trying one font from vtium-config's Fonts tab": a
    ///   font file's trial runs as a font set's does.
    pub(crate) fn try_live(&mut self, name: String) -> Vec<Effect> {
        if let Some(off) = self.shared.trial_off() {
            self.shared.message = Some(off.to_owned());
            return Vec::new();
        }
        if self.shared.refused.contains(&name) {
            self.shared.message = Some(if is_file(&name) {
                words::font_file_refused(font_shown(&name))
            } else {
                words::KEPT.to_owned()
            });
            return Vec::new();
        }
        let back = self
            .shared
            .want
            .clone()
            .or_else(|| self.shared.shown_font.clone())
            .unwrap_or_default();
        if back == name {
            self.shared.message = Some(words::trial_same(font_shown(&name)));
            return Vec::new();
        }
        self.nav.trial = Some(Trial {
            font: name.clone(),
            back,
            left: TRIAL_TICKS,
            grid: None,
            look: None,
        });
        self.shared.want = Some(name.clone());
        self.shared.message = Some(countdown(&self.shared, &name, TRIAL_TICKS));
        let mut out = self.follow();
        out.push(Effect::Wait(TICK));
        out
    }

    /// `t` on the look `name`: tries the whole look on the live console.
    /// Its font set goes to the console as a font set's trial does, its
    /// color scheme with OSC 4, 10 and 11, and its size with the grid
    /// request once the console draws the font set, where the console lets
    /// programs resize its grid. Where it does not, the countdown says that
    /// the size takes effect when vtium starts again. A part the console
    /// already shows is not sent. When the console already shows every
    /// part, `t` says so, and when no terminal of the draft uses the look,
    /// it says that too and offers the look to the console's terminal.
    ///
    /// References:
    /// - ADR-1201 D4, "The live trial, on a key": `t` on a look tries it
    ///   on the live console, and the countdown puts it back.
    /// - ADR-1201 D4, "Sizes, live on a vtium console": the grid request,
    ///   which vtium honours under `allow-window-ops`.
    pub(crate) fn try_look(&mut self, name: String) -> Vec<Effect> {
        if let Some(off) = self.shared.trial_off() {
            self.shared.message = Some(off.to_owned());
            return Vec::new();
        }
        let Some(look) = super::tabs::looks::list::look(&self.shared, &name) else {
            self.shared.message = Some(words::look_gone(&name));
            return Vec::new();
        };
        let back = self
            .shared
            .want
            .clone()
            .or_else(|| self.shared.shown_font.clone())
            .unwrap_or_default();
        let font = look
            .frit
            .clone()
            .or_else(|| self.shared.drafted("font-name"))
            .filter(|f| *f != back && !self.shared.refused.contains(f));
        let scheme = look
            .palette
            .clone()
            .or_else(|| self.shared.drafted("palette"));
        let colors = scheme
            .as_deref()
            .and_then(|s| scheme_colors(&self.shared, s))
            .filter(|p| {
                Some(p) != scheme_colors(&self.shared, &console_scheme(&self.shared)).as_ref()
            });
        let points: u32 = look
            .size
            .clone()
            .or_else(|| self.shared.drafted("font-size"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let window_ops = self.shared.window_ops();
        let size = (points > 0 && window_ops).then_some(points);
        let later = (points > 0 && !window_ops).then(|| words::look_size_later(points));
        let unused = super::tabs::looks::list::heads(&self.shared, &name).is_empty();
        let offer = unused.then(|| offer_head(&self.shared)).flatten();
        if font.is_none() && colors.is_none() && size.is_none() {
            let same = words::look_same(&name, later.as_deref());
            match offer {
                Some(head) => self.ask(
                    words::look_use_question(&same, &name, &head),
                    use_look(head, name),
                ),
                None if unused => {
                    self.shared.message = Some(words::look_unused_flow(&same, &name));
                }
                None => self.shared.message = Some(same),
            }
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(p) = &colors {
            out.push(Effect::Write(colors_request(p)));
        }
        self.shared.message = Some(words::look_countdown(&name, TRIAL_TICKS, later.as_deref()));
        self.nav.trial = Some(Trial {
            font: font.clone().unwrap_or_else(|| back.clone()),
            back,
            left: TRIAL_TICKS,
            grid: None,
            look: Some(LookParts {
                name,
                colors: colors.is_some(),
                size,
                later,
                offer,
            }),
        });
        if let Some(font) = font {
            self.shared.want = Some(font);
            out.extend(self.follow());
        } else {
            out.extend(self.look_size());
        }
        out.push(Effect::Wait(TICK));
        out
    }

    /// The size a look's trial still has to try, once the console draws
    /// the look's font set: the font helper measures the font set's cell
    /// at the size, and [`App::size_measured`] sends the grid request.
    pub(crate) fn look_size(&mut self) -> Vec<Effect> {
        let Some(t) = self.nav.trial.as_mut() else {
            return Vec::new();
        };
        if self.shared.shown_font.as_deref() != Some(t.font.as_str()) {
            return Vec::new();
        }
        let Some(points) = t.look.as_mut().and_then(|l| l.size.take()) else {
            return Vec::new();
        };
        let font = t.font.clone();
        let height = crate::specimen::pixel_height(points, self.shared.inputs.dpi.0);
        self.shared.size_asked = Some(SizeAsked {
            font: font.clone(),
            height,
            points,
        });
        vec![Effect::Cell { font, height }]
    }

    /// A key while the live trial counts down: `k` keeps the font, Ctrl-C
    /// leaves, and any other key puts the font back and does nothing else.
    pub(crate) fn trial_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Char('k') => {
                if let Some(Trial {
                    look: Some(look), ..
                }) = &self.nav.trial
                {
                    let look = look.clone();
                    self.nav.trial = None;
                    self.shared.colors_moved |= look.colors;
                    let kept = words::look_kept(&look.name);
                    match look.offer {
                        Some(head) => self.ask(
                            words::look_use_question(&kept, &look.name, &head),
                            use_look(head, look.name),
                        ),
                        None => self.shared.message = Some(kept),
                    }
                    return Vec::new();
                }
                if let Some(t) = self.nav.trial.take() {
                    self.shared.message = Some(match t.grid {
                        Some(g) => words::size_kept(g.points),
                        None if is_file(&t.font) => words::font_file_kept(font_shown(&t.font)),
                        None => words::trial_kept(&t.font),
                    });
                }
                Vec::new()
            }
            _ => self.put_back(),
        }
    }

    /// Ends the live trial and switches the console back to the font it
    /// drew before it, or for a trial of a size, asks the console for the
    /// grid it had before.
    pub(crate) fn put_back(&mut self) -> Vec<Effect> {
        let Some(t) = self.nav.trial.take() else {
            return Vec::new();
        };
        if let Some(look) = &t.look {
            return self.put_look_back(&t, look);
        }
        if let Some(g) = t.grid {
            // a console back at its starting grid needs nothing more when
            // the TUI leaves
            self.shared.grid_moved = g.back != self.shared.start_grid;
            self.shared.message = Some(words::size_back(g.back));
            return vec![Effect::Write(grid_request(g.back))];
        }
        self.shared.message = Some(words::trial_back(font_shown(&t.back)));
        self.shared.want = Some(t.back);
        self.follow()
    }

    /// Ends the trial `t` of the look `look`: the colors, the font and the
    /// grid it changed go back. The grid request follows the font's switch,
    /// because a font switch changes the console's grid too.
    fn put_look_back(&mut self, t: &Trial, look: &LookParts) -> Vec<Effect> {
        let mut out = Vec::new();
        if look.colors {
            out.push(Effect::Write(COLORS_RESET.to_vec()));
        }
        let back = words::look_back(&look.name);
        self.shared.message = Some(match look.offer {
            Some(_) => format!("{back} {}", words::look_unused(&look.name)),
            None => back,
        });
        self.shared.size_asked = None;
        if t.font != t.back {
            self.shared.want = Some(t.back.clone());
            out.extend(self.follow());
            if t.grid.is_some() {
                self.shared.grid_after_font = true;
                out.extend(self.grid_after_font());
            }
        } else if let Some(g) = t.grid {
            self.shared.grid_moved = g.back != self.shared.start_grid;
            out.push(Effect::Write(grid_request(g.back)));
        }
        out
    }

    /// Enter on a size in the shared list of a look's card: in a vtium
    /// console that lets programs resize its grid, tries the size on the
    /// console as a timed trial. The font helper measures the cell of the
    /// font set `font` at the size first, and [`App::size_measured`] sends
    /// the grid request once the cell comes. An automatic size has no grid
    /// of its own, so it is not tried. Outside a vtium console the size
    /// shows only in the preview. With `allow-window-ops` off the tab says
    /// why: with the live trial on, the preview is the console's own text
    /// at its current size, so the size takes effect when vtium starts
    /// again; with it off, the size shows only in the preview.
    ///
    /// References:
    /// - ADR-1201 D4, "Sizes, live on a vtium console": Enter applies the
    ///   highlighted size as a timed trial, and the grid is the one the
    ///   size gives on the display.
    /// - ADR-1201 D4, its note of 2026-11-21: with the live trial on and
    ///   `allow-window-ops` off, the preview is the console's own text.
    pub(crate) fn try_size(&mut self, font: Option<String>, points: u32) -> Vec<Effect> {
        if !self.shared.in_vtium() || points == 0 {
            return Vec::new();
        }
        if !self.shared.window_ops() {
            self.shared.message = Some(
                if self.shared.text_preview() {
                    words::SIZE_AT_NEXT_START
                } else {
                    words::SIZE_PICTURE_ONLY
                }
                .to_owned(),
            );
            return Vec::new();
        }
        let Some(font) = font.or_else(|| self.shared.shown_font.clone()) else {
            return Vec::new();
        };
        let height = crate::specimen::pixel_height(points, self.shared.inputs.dpi.0);
        self.shared.size_asked = Some(SizeAsked {
            font: font.clone(),
            height,
            points,
        });
        vec![Effect::Cell { font, height }]
    }

    /// The cell of a size that [`App::try_size`] asked for has come, or
    /// `None` when the cell came for something else: the grid the cell
    /// gives on the display goes to the console as the grid request, and
    /// the trial's countdown starts. A size whose grid is the console's
    /// grid now changes nothing.
    pub(crate) fn size_measured(
        &mut self,
        font: &str,
        height: u32,
        cell: &Result<(Cell, u32), String>,
    ) -> Option<Vec<Effect>> {
        let asked = self.shared.size_asked.as_ref()?;
        if asked.font != font || asked.height != height {
            return None;
        }
        let points = asked.points;
        self.shared.size_asked = None;
        let look_trial = self.nav.trial.as_ref().is_some_and(|t| t.look.is_some());
        if self.shared.leaving.is_some() || (self.nav.trial.is_some() && !look_trial) {
            return Some(Vec::new());
        }
        let cell = match cell {
            Ok((cell, _)) => *cell,
            Err(why) => {
                // a look's trial goes on without its size, and its
                // countdown says why
                match self.nav.trial.as_mut().and_then(|t| t.look.as_mut()) {
                    Some(look) => look.later = Some(words::size_unmeasured(why)),
                    None => self.shared.message = Some(words::size_unmeasured(why)),
                }
                return Some(Vec::new());
            }
        };
        let (display, _) = display(&self.shared);
        let across =
            |px: u32, side: u32| usize::try_from(px.checked_div(side).unwrap_or(0)).unwrap_or(0);
        let to = (
            across(display.0, cell.width).clamp(1, GRID_MAX.0),
            across(display.1, cell.height).clamp(1, GRID_MAX.1),
        );
        let back = (self.size.cols, self.size.rows);
        if let Some(t) = self.nav.trial.as_mut().filter(|t| t.look.is_some()) {
            if to == back {
                return Some(Vec::new());
            }
            t.grid = Some(GridTrial { points, to, back });
            self.shared.grid_moved = true;
            self.shared.grid_after_font = false;
            return Some(vec![Effect::Write(grid_request(to))]);
        }
        if to == back {
            self.shared.message = Some(words::size_same(points));
            return Some(Vec::new());
        }
        let font = self.shared.shown_font.clone().unwrap_or_default();
        self.nav.trial = Some(Trial {
            font: font.clone(),
            back: font,
            left: TRIAL_TICKS,
            grid: Some(GridTrial { points, to, back }),
            look: None,
        });
        self.shared.grid_moved = true;
        // a trial of a size replaces the start grid that `u` asked for
        self.shared.grid_after_font = false;
        self.shared.message = Some(words::size_countdown(points, TRIAL_TICKS));
        Some(vec![Effect::Write(grid_request(to)), Effect::Wait(TICK)])
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
        self.shared.message = Some(match (&t.look, t.grid) {
            (Some(look), _) => words::look_countdown(&look.name, t.left, look.later.as_deref()),
            (None, Some(g)) => words::size_countdown(g.points, t.left),
            (None, None) => countdown(&self.shared, &t.font, t.left),
        });
        vec![Effect::Wait(TICK)]
    }
}

/// The colors of the color scheme `name`: a scheme built into vtium, else
/// a scheme file the Looks tab found that reads.
fn scheme_colors(shared: &Shared, name: &str) -> Option<vtium_term::Palette> {
    vtium_term::Palette::named(name).or_else(|| {
        shared
            .inputs
            .effects
            .palettes
            .iter()
            .find(|f| f.name == name)
            .and_then(|f| f.colors.clone().ok())
    })
}

/// The color scheme the console draws now, as the installed vtium.conf
/// gives it: the `palette` of the look that the console's terminal uses,
/// else the top level's, else `legacy`, the default.
fn console_scheme(shared: &Shared) -> String {
    let conf = shared.inputs.conf.as_deref().unwrap_or_default();
    let of_look = shared.inputs.head.as_deref().and_then(|head| {
        let reading = vtium_settings::sections(conf.as_bytes())?;
        let config = reading.parsed.config;
        let look = config.heads.iter().find(|h| h.name == head)?.look.clone();
        config
            .looks
            .into_iter()
            .find(|l| l.name == look)?
            .settings
            .into_iter()
            .find(|s| s.key.name == "palette")
            .map(|s| s.value)
    });
    of_look
        .or_else(|| vtium_settings::setting(conf, "palette"))
        .unwrap_or_else(|| "legacy".to_owned())
}

/// The terminal of the draft that a look no terminal uses is offered to:
/// the console's own terminal, which `VTIUM_HEAD` names, else the draft's
/// only terminal. With several terminals and no `VTIUM_HEAD`, there is
/// none to offer.
fn offer_head(shared: &Shared) -> Option<String> {
    let heads: Vec<String> = shared
        .conf_text()
        .and_then(|c| vtium_settings::sections(c.as_bytes()))
        .map(|r| r.parsed.config.heads.into_iter().map(|h| h.name).collect())
        .unwrap_or_default();
    match (&shared.inputs.head, heads.as_slice()) {
        (Some(head), _) if heads.contains(head) => Some(head.clone()),
        (_, [only]) => Some(only.clone()),
        _ => None,
    }
}

/// What `y` does to the question that offers the look `look` to the
/// terminal `head`: sets the terminal's `look =` line in the draft.
fn use_look(head: String, look: String) -> super::model::Then {
    super::model::Then::Change(super::edit::Change::HeadKey {
        name: head,
        key: "look",
        value: Some(look),
    })
}
