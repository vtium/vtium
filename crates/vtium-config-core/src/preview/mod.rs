//! The previews of the Fonts, Looks and Effects tabs (ADR-1201 D4): what a
//! tab previews and in which font ([`Source`], [`Preview`]), the picture
//! of the console that the font helper draws through [`crate::specimen`]
//! ([`picture`]), the lines a vtium console draws as the preview in its own
//! font ([`console`]), and where both go in the tab ([`place`]). A preview
//! never changes the live console or the document.
//!
//! A tab says what it previews with a [`Source`], which names the font by
//! itself, so no tab reads another tab's highlight. [`Preview`] reads the
//! source against the shared state and the [`Facts`] the shell knows about
//! the terminal and the machine, and gives what to ask the font helper for
//! ([`Preview::wanted`]) and the line beside the preview
//! ([`Preview::line`]). [`picture::Pictures`] keeps what was asked and
//! what came, and turns each answer of the font helper into the next step.
//! Where the picture goes is a `Rect` the tab gives, inside the viewer the
//! frame hands it, never a fixed row.
//!
//! In a vtium console that vtium-config can switch, the Fonts and Looks
//! tabs preview as the console's own text, so a font tried with `t` shows
//! there; the Effects tab and a font file still preview as pictures, since
//! the console's text can show neither. Where this terminal shows no
//! pictures, the preview is a line that describes the font at its size.
//!
//! References:
//! - ADR-1201 D4: a picture of the console with the change applied, which leaves the live console alone, and the console's own text in a vtium console.
//! - ADR-1205 D8: the Effects tab previews an effect as a picture of the console with the effect applied.
//! - ADR-1206 D2: the pictures are part of the shared state, and the loop alone asks the font helper.
//! - ADR-1206 D4, "What the frame replaces in layout.rs": a picture is placed inside the viewer's Rect.

pub mod console;
pub mod picture;
pub mod place;
pub mod words;

use std::path::Path;

use crate::found::effects::Vtlook;
use crate::found::{Activation, Origin, Resource};
use crate::specimen::{Filter, Filtered, Outcome};
use crate::ui::{Console, Live, Message, Shared};

pub use picture::{Answer, Asked, Caption, Pictures, Show, Unpictured};
pub use place::{Covered, Place};

/// The size in points a preview draws where the document sets none, or
/// sets it automatic: `vtium-config preview`'s default.
pub const POINTS: u32 = 12;

/// The display a picture shows where vtium-config finds none connected:
/// the VGA console's 640 by 480.
pub const ASSUMED_DISPLAY: (u32, u32) = (640, 480);

/// The screen the Looks and Effects tabs' pictures draw, in pixels,
/// whatever the real display's size: a nominal screen of sample content
/// from its top left. The picture shows it as the detailed specimen is
/// shown: one pixel of the console to one pixel of the picture, as many
/// whole character cells as the preview's place holds. The glyphs keep
/// the character cell the change gets on the real display.
pub const PREVIEW_SCREEN: (u32, u32) = (1024, 768);

/// The most columns and rows a picture's screen is laid out at, whatever
/// the character cell. It bounds the work and memory of a picture, and
/// sits above what a full 8K display holds: about 1,100 columns at a
/// 7-pixel cell.
pub const GRID_MAX: (usize, usize) = (2048, 2048);

/// The character cell in pixels that vtium-config assumes where this
/// terminal reports no size in pixels.
pub const ASSUMED_CELL: (u32, u32) = (8, 16);

/// The smallest grid vtium-config runs in, columns by rows. Below it no
/// screen is drawn, so nothing is previewed.
///
/// References:
/// - ADR-1201 D3, "Where it runs": vtium-config needs a terminal of at least 80×25 cells.
pub const MIN_GRID: (usize, usize) = (80, 25);

/// A vtium console's text area in pixels, as XTWINOPS 14 reports it, and
/// the grid, columns by rows, that the console held when vtium-config
/// asked. vtium reports the text area as the grid times its font's cell,
/// so the text area is the displays' size cut down to whole cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextArea {
    pub pixels: (u32, u32),
    pub grid: (usize, usize),
}

impl TextArea {
    /// The displays' size in pixels that the console fills: `mode`, the
    /// size of the mode the first display runs at, where the text area is
    /// that mode cut down to whole cells (at least the text area and less
    /// than one more cell on each side). Otherwise the displays are not
    /// that mode, as when the console spans several displays, and the text
    /// area itself is the size.
    pub fn display(&self, mode: Option<(u32, u32)>) -> (u32, u32) {
        let cell = |px: u32, n: usize| u32::try_from(n).ok().and_then(|n| px.checked_div(n));
        let (Some(cw), Some(ch)) = (
            cell(self.pixels.0, self.grid.0),
            cell(self.pixels.1, self.grid.1),
        ) else {
            return self.pixels;
        };
        let within = |m: u32, px: u32, c: u32| m >= px && m < px.saturating_add(c.max(1));
        match mode {
            Some(m) if within(m.0, self.pixels.0, cw) && within(m.1, self.pixels.1, ch) => m,
            _ => self.pixels,
        }
    }
}

/// What a preview reads beside the shared state: what the shell knows of
/// this terminal and of the machine. The shell works these out as the
/// answers and the devices come, and hands them in with each call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    /// This terminal's grid, columns by rows.
    pub grid: (usize, usize),
    /// This terminal's character cell in pixels, where it reports its
    /// size in pixels.
    pub cell_px: Option<(u32, u32)>,
    /// The density a size in points is drawn at, in dots per inch.
    pub dpi: u32,
    /// The locale the detailed specimen's lines are chosen for.
    pub locale: String,
    /// Whether vtium-config draws color at all.
    pub color: bool,
    /// The size in pixels of the mode the first display runs at under the
    /// open document, else its preferred mode; `None` where no display is
    /// connected.
    pub display: Option<(u32, u32)>,
    /// A vtium console's text area, where it reported one.
    pub text_area: Option<TextArea>,
    /// The console's terminal, as `VTIUM_HEAD` names it.
    pub head: Option<String>,
}

/// What a tab previews, and in which font: a font set's or a built-in
/// font's name, or a font file's path.
#[derive(Clone, Copy, Debug)]
pub enum Source<'a> {
    /// The tab previews nothing.
    Nothing,
    /// `font` at the document's size: its detailed specimen where `detail`
    /// is set, and otherwise a picture of vtium-config's screen drawn in
    /// it.
    Font { font: Option<&'a str>, detail: bool },
    /// `font` at `points` on the sample screen of [`PREVIEW_SCREEN`]
    /// pixels. A size of 0 is automatic, which has no preview.
    Size { font: Option<&'a str>, points: u32 },
    /// `font` at the document's size on the sample screen, drawn through
    /// `effect`.
    Effect {
        font: Option<&'a str>,
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
    /// active in the document; `Ok(None)` for one that changes no color;
    /// or why vtium cannot use it, so the picture shows the console
    /// without it. A color map that is not active or fails the trust rule
    /// counts as missing, and a vtlook that does not read, or names such a
    /// color map, gives way to the built-in vtlook.
    ///
    /// References:
    /// - ADR-1205 D4: a resource the batch does not list, or that fails the trust rule, counts as missing.
    /// - ADR-1304 D6: a vtlook that fails at any stage gives way to the built-in vtlook.
    pub fn filter(&self, shared: &Shared) -> Result<Option<Filtered>, Message> {
        let plain = |text: String| Message {
            line: text,
            more: None,
        };
        let filtered = |filter| {
            Ok(Some(Filtered {
                filter,
                active: shared
                    .found
                    .effects
                    .maps
                    .iter()
                    .filter(|r| active(shared, r) && !r.own)
                    .map(|r| r.path.clone())
                    .collect(),
            }))
        };
        match *self {
            Applied::Nothing => Ok(None),
            Applied::Shader(_) => Err(plain(words::EFFECT_SHADER.to_owned())),
            Applied::Map(r) => match unusable(shared, r) {
                Some(why) => Err(plain(words::map_unusable(why))),
                None => filtered(Filter::Cube(r.path.clone())),
            },
            Applied::Vtlook(v) => {
                let Some(file) = &v.file else {
                    return Ok(None);
                };
                if let Err(error) = &v.reads {
                    return Err(words::effect_vtlook_broken(error));
                }
                for named in v
                    .files
                    .iter()
                    .filter(|f| f.extension().is_some_and(|e| e == "cube"))
                {
                    let why = match shared.found.resource(named) {
                        None => Some(words::MAP_MISSING),
                        Some(r) => unusable(shared, r),
                    };
                    if let Some(why) = why {
                        let name = named.file_name().map_or_else(
                            || named.display().to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        return Err(plain(words::vtlook_map_unusable(&name, why)));
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

/// Whether the resource `r` is active once the open document's draft is
/// counted: the draft's change where it makes one, else the installed
/// batch's list.
fn active(shared: &Shared, r: &Resource) -> bool {
    match r.activation(&shared.document().content().active) {
        Activation::BuiltIn => true,
        Activation::Draft(change) => change == crate::batch::Change::Activate,
        Activation::Installed(origin) => matches!(
            origin,
            Origin::BatchFile | Origin::FreshInstall | Origin::List { .. }
        ),
    }
}

/// Why vtium cannot use the color map `r`, in the words of its marks, or
/// `None` when it can. Failing the trust rule comes before not being
/// active, since a file that fails the rule cannot be activated.
fn unusable(shared: &Shared, r: &Resource) -> Option<&'static str> {
    if r.own {
        Some(words::MAP_OWN)
    } else if r.trust.is_err() {
        Some(words::MAP_UNTRUSTED)
    } else if !active(shared, r) {
        Some(words::MAP_INACTIVE)
    } else {
        None
    }
}

/// What a picture of the console shows: the font, its size in points,
/// whether it is the detailed specimen in place of the picture of the
/// console, and the effect the picture is drawn through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub font: String,
    pub points: u32,
    pub detail: bool,
    pub filter: Option<Filtered>,
}

/// Whether `name` is a font file's path rather than a font set's or a
/// built-in font's name, which never holds a `/`.
pub fn is_file(name: &str) -> bool {
    name.starts_with('/')
}

/// Whether the font helper can draw `name`: a built-in font, an installed
/// font set that reads, or a font file found on the machine. A font set
/// that does not read is refused, and its list marks it so.
pub fn readable(shared: &Shared, name: &str) -> bool {
    if is_file(name) {
        return shared.found.fonts.file(Path::new(name)).is_some();
    }
    if crate::frit::RESERVED.contains(&name) {
        return vtium_font::set::builtin(name).is_some();
    }
    shared
        .document()
        .installed()
        .frits
        .get(name)
        .is_some_and(|bytes| {
            crate::frit::Frit::parse(bytes, &Path::new(name).with_extension("frit"), name).is_ok()
        })
}

/// Whether the Fonts and Looks tabs preview as text the console draws: in
/// a vtium console vtium-config can switch, so `t` shows a font on the
/// console itself.
pub fn text_preview(console: &Console) -> bool {
    console.live == Live::On
}

/// The open document's `font-size`, else [`POINTS`] for an automatic size
/// or none.
pub fn document_points(shared: &Shared) -> u32 {
    vtium_settings::setting(shared.document().text(), "font-size")
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(POINTS)
}

/// The pixel height `wanted` asks the font for: its size at the density
/// the facts hold.
pub fn height_of(facts: &Facts, wanted: &Wanted) -> u32 {
    crate::specimen::pixel_height(wanted.points, facts.dpi)
}

/// The display a picture's grid is counted on, and whether it is assumed.
/// In a vtium console that reported its text area, the grid comes from
/// that text area, so it is the grid that fills the console at the mode it
/// runs at. Elsewhere it comes from the first display's mode, else the
/// assumed 640×480.
pub fn display(facts: &Facts) -> ((u32, u32), bool) {
    if let Some(area) = &facts.text_area {
        return (area.display(facts.display), false);
    }
    match facts.display {
        Some(d) => (d, false),
        None => (ASSUMED_DISPLAY, true),
    }
}

/// The vtlook this vtium console draws with, where it is not the built-in
/// one: the installed vtium.conf's vtlook for the console's terminal, from
/// that terminal's look, else the top level. The console draws its whole
/// frame through that vtlook, so a picture shown in it is filtered a
/// second time. `None` outside a vtium console.
///
/// References:
/// - ADR-1304 D5: a head's vtlook comes from its look, else the top level, else the built-in one.
pub fn console_vtlook(shared: &Shared, facts: &Facts) -> Option<String> {
    if !matches!(shared.console.live, Live::On | Live::OpsOff) {
        return None;
    }
    let conf = shared.document().installed().text.as_deref()?;
    let top = vtium_settings::setting(conf, "vtlook");
    let own = facts.head.as_deref().and_then(|head| {
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

/// What the open tab's preview reads: the shared state, the facts, and
/// what the tab previews. Update and the tab's view both read the preview
/// through this, and neither changes it.
#[derive(Clone, Copy, Debug)]
pub struct Preview<'a> {
    pub shared: &'a Shared,
    pub facts: &'a Facts,
    pub source: Source<'a>,
    /// The font set the preview stands for, where only the document adds
    /// it. The console reads only installed font sets, so the line beside
    /// the console's own preview says that `t` can try it after Save.
    pub in_document: Option<&'a str>,
}

impl<'a> Preview<'a> {
    /// The preview of `source`, on the shared state and the facts.
    pub fn of(shared: &'a Shared, facts: &'a Facts, source: Source<'a>) -> Preview<'a> {
        Preview {
            shared,
            facts,
            source,
            in_document: None,
        }
    }

    /// The preview of a tab that previews nothing.
    pub fn nothing(shared: &'a Shared, facts: &'a Facts) -> Preview<'a> {
        Preview::of(shared, facts, Source::Nothing)
    }

    /// This preview, standing for `name`, a font set only the document
    /// adds, where `name` is given.
    pub fn in_document(self, name: Option<&'a str>) -> Preview<'a> {
        Preview {
            in_document: name,
            ..self
        }
    }

    /// The font the preview shows.
    pub fn font(&self) -> Option<&'a str> {
        match self.source {
            Source::Font { font, .. } | Source::Size { font, .. } | Source::Effect { font, .. } => {
                font
            }
            Source::Nothing => None,
        }
    }

    /// Whether the grid is below the smallest vtium-config runs in.
    fn too_small(&self) -> bool {
        self.facts.grid.0 < MIN_GRID.0 || self.facts.grid.1 < MIN_GRID.1
    }

    /// Whether the tab previews as a picture in a vtium console as well,
    /// where the other tabs preview as the console's own text: the
    /// Effects tab does, since the console's text cannot show an effect,
    /// and so does a font file, which the console cannot draw its own text
    /// in.
    pub fn pictured_in_console(&self) -> bool {
        matches!(self.source, Source::Effect { .. }) || self.font().is_some_and(is_file)
    }

    /// Whether the preview is the console's own text: in a vtium console
    /// vtium-config can switch, for a preview the console can draw.
    pub fn in_console(&self) -> bool {
        text_preview(&self.shared.console) && !self.pictured_in_console()
    }

    /// Whether the picture shows the sample screen, which the Looks and
    /// Effects tabs draw, in place of vtium-config's own screen.
    pub fn sample(&self) -> bool {
        matches!(self.source, Source::Size { .. } | Source::Effect { .. })
    }

    /// The change the tab previews now, if any: a font the font helper
    /// can draw, at the document's size or at a size, automatic having
    /// none.
    pub fn wanted(&self) -> Option<Wanted> {
        if self.too_small() {
            return None;
        }
        let font = self.font()?;
        if !readable(self.shared, font) {
            return None;
        }
        let font = font.to_owned();
        match self.source {
            Source::Font { detail, .. } => Some(Wanted {
                font,
                points: document_points(self.shared),
                detail,
                filter: None,
            }),
            Source::Size { points, .. } => (points > 0).then_some(Wanted {
                font,
                points,
                detail: false,
                filter: None,
            }),
            Source::Effect { effect, .. } => Some(Wanted {
                font,
                points: document_points(self.shared),
                detail: false,
                // an effect vtium cannot use leaves the picture plain
                filter: effect.filter(self.shared).ok().flatten(),
            }),
            Source::Nothing => None,
        }
    }

    /// The line beside the preview, with the full text behind it: beside
    /// a picture, what the picture shows; beside the console's own text,
    /// what the console draws it in; where this terminal shows no
    /// pictures, the font helper's description of the font at its size.
    /// `pictures` holds what was asked and what came.
    pub fn line(&self, pictures: &Pictures) -> Option<Message> {
        if let Source::Effect { effect, .. } = self.source {
            return self.effect_line(effect, pictures);
        }
        if self.in_console() && !self.too_small() {
            return match (self.source, self.in_document) {
                (Source::Font { .. }, Some(name)) => {
                    Some(words::console_caption_in_document(name, false))
                }
                (Source::Size { .. }, Some(name)) => {
                    Some(words::console_caption_in_document(name, true))
                }
                (Source::Font { .. }, None) => Some(words::console_font_caption(
                    self.shared.console.shown_font.as_deref(),
                )),
                (Source::Size { .. }, None) => Some(Message {
                    line: words::CONSOLE_SIZE_CAPTION.to_owned(),
                    more: None,
                }),
                (Source::Effect { .. } | Source::Nothing, _) => None,
            };
        }
        let wanted = self.wanted()?;
        let height = height_of(self.facts, &wanted);
        if self.shared.console.graphics && wanted.detail {
            let (display, assumed) = display(self.facts);
            let summary = pictures
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
        if self.shared.console.graphics {
            return pictures.caption.as_ref().map(Caption::words);
        }
        let summary = pictures
            .summary
            .as_ref()
            .filter(|(f, h, _)| *f == wanted.font && *h == height)
            .map(|(_, _, s)| s);
        Some(words::no_pictures(summary))
    }

    /// The line beside the Effects tab's picture: what it shows, and why
    /// it shows the console without the effect, where it does. The line
    /// speaks of the picture only while one is on the screen or on its
    /// way, and gives the outcome of the effect only for the preview that
    /// outcome answers. In a vtium console that draws a vtlook of its own,
    /// the full text adds that the console draws the picture through that
    /// vtlook too.
    fn effect_line(&self, effect: Applied<'_>, pictures: &Pictures) -> Option<Message> {
        let plain = |text: &str| Message {
            line: text.to_owned(),
            more: None,
        };
        if self.too_small() {
            return None;
        }
        if !self.shared.console.graphics {
            return Some(plain(words::EFFECT_NO_PICTURES));
        }
        let Some(wanted) = self.wanted() else {
            return Some(plain(words::EFFECT_NO_FONT));
        };
        if pictures.asked.as_ref().is_some_and(|a| a.wanted == wanted) {
            match pictures.unpictured {
                Some(Unpictured::NoRoom) => return Some(plain(words::EFFECT_NO_ROOM)),
                Some(Unpictured::Failed) => return Some(plain(words::EFFECT_NOT_DRAWN)),
                None => {}
            }
        }
        let outcome = pictures
            .caption
            .as_ref()
            .and_then(|c| c.outcome.as_ref())
            .filter(|(answers, _)| *answers == wanted)
            .map(|(_, outcome)| outcome);
        let mut said = match effect.filter(self.shared) {
            Err(why) => why,
            Ok(None) => plain(words::EFFECT_PLAIN),
            Ok(Some(_)) => {
                let (name, what) = match effect {
                    Applied::Vtlook(v) => (v.name.clone(), "vtlook"),
                    Applied::Map(r) => (r.name(), "color map"),
                    Applied::Nothing | Applied::Shader(_) => (String::new(), "effect"),
                };
                let shown = match effect {
                    Applied::Vtlook(_) => words::effect_vtlook(&name),
                    Applied::Map(_) => words::effect_map(&name),
                    Applied::Nothing | Applied::Shader(_) => words::EFFECT_PLAIN.to_owned(),
                };
                match outcome {
                    Some(Outcome::Failed(why)) => words::effect_failed(what, why),
                    Some(Outcome::ColorOnly) => words::effect_color_only(&name),
                    Some(Outcome::Applied) | None => plain(&shown),
                }
            }
        };
        if let Some(own) = console_vtlook(self.shared, self.facts) {
            let base = said.more.take().unwrap_or_else(|| said.line.clone());
            said.more = Some(format!("{base} {}", words::effect_console_vtlook(&own)));
        }
        Some(said)
    }
}
