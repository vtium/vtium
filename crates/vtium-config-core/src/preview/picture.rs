//! The picture of the console, asked of the font helper in steps (ADR-1201
//! D4), and what came of each step.
//!
//! [`Pictures::ask`] first asks the font helper for the font's character
//! cell at the change's size ([`Effect::Cell`]), since the cell decides the
//! grid the change gives on the display. [`Pictures::celled`] then asks
//! the loop for the picture: of vtium-config's own screen drawn at that
//! grid ([`Effect::ScreenPicture`]), or of the sample screen, which goes
//! in the request itself ([`Effect::Picture`]). The detailed specimen is
//! one step ([`Effect::Specimen`]). Where this terminal shows no pictures,
//! the step asks for a line of text instead ([`Effect::Summary`]). One
//! picture is drawn at a time, the one asked for last is not asked for
//! again, and a picture that comes after the highlight moved on is not
//! shown: the next is asked for.
//!
//! The steps return an [`Answer`]: the effects for the loop, the picture
//! to show at its place or the one to delete, and a message for the
//! message line, which the shell puts there unless the live trial's
//! countdown holds it. Nothing is asked while vtium-config still asks
//! this terminal what it is or runs the privileged step, and the shell
//! hands the pictures no answer while vtium-config is leaving.
//!
//! References:
//! - ADR-1201 D4: the picture of the console, in the font's cell at the change's size, on the display.
//! - ADR-1205 D8: the Effects tab's picture drawn through the effect, as the console's CPU path draws a frame.
//! - ADR-1206 D2: update hands a picture to the pictures, and the loop alone asks the font helper.

use vtium_font::Cell;

use super::{
    Covered, GRID_MAX, PREVIEW_SCREEN, Place, Preview, Wanted, console, display, height_of, words,
};
use crate::specimen::{Outcome, Picture, Shot, Summary};
use crate::ui::{Effect, Message};

/// The picture or line asked for last, and the terminal it was asked for:
/// its grid, its character cell in pixels, and whether it shows pictures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub wanted: Wanted,
    pub grid: (usize, usize),
    pub cell_px: Option<(u32, u32)>,
    pub graphics: bool,
}

/// What the line beside a picture says: the display, the size, the cell
/// and the grid it gives.
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
    /// gives one item's outcome beside another's.
    pub outcome: Option<(Wanted, Outcome)>,
}

impl Caption {
    /// The line beside the picture.
    pub fn words(&self) -> Message {
        words::caption(
            &self.font,
            self.points,
            self.display,
            self.assumed,
            self.cell,
            self.grid,
        )
    }
}

/// Why no picture is on the screen for the preview asked for last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unpictured {
    /// The tab's place for it holds no pixel.
    NoRoom,
    /// Measuring the font or painting the picture failed, and the message
    /// line says why.
    Failed,
}

/// A picture to show: the cell it is shown from, row and column counted
/// from 1, and its pixels. The loop writes the cursor's move to that cell
/// and the picture's kitty graphics after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Show {
    pub at: (usize, usize),
    pub picture: Picture,
}

/// What a step gives back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    /// The effects for the loop.
    pub effects: Vec<Effect>,
    /// The picture on the screen is to be deleted.
    pub delete: bool,
    /// The picture to show in its place.
    pub show: Option<Show>,
    /// A message for the message line, unless the live trial's countdown
    /// holds it.
    pub say: Option<String>,
}

impl Answer {
    /// The answer that asks the loop for `effect`.
    fn effect(effect: Effect) -> Answer {
        Answer {
            effects: vec![effect],
            ..Answer::default()
        }
    }
}

/// The pictures: what was asked last, whether one is being drawn, the one
/// on the screen, and what the font helper said of the font, for the line
/// beside the preview.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pictures {
    /// The picture or line asked for last.
    pub asked: Option<Asked>,
    /// A picture is being drawn, so the next is asked for once it comes.
    pub drawing: bool,
    /// The cells under the picture on the screen, while one shows.
    pub shown: Option<Covered>,
    /// The line beside the picture asked for last.
    pub caption: Option<Caption>,
    /// Why no picture shows for the one asked for last, if none does.
    pub unpictured: Option<Unpictured>,
    /// The font helper's summary of the detailed specimen last drawn: the
    /// font, the pixel height and the summary.
    pub specimen: Option<(String, u32, Summary)>,
    /// The font helper's summary of a font at a height, for a terminal
    /// that shows no pictures, or why the size cannot be used.
    pub summary: Option<(String, u32, Result<Summary, String>)>,
}

impl Pictures {
    /// Deletes the picture on the screen, if one shows.
    fn drop_picture(&mut self) -> Answer {
        Answer {
            delete: self.shown.take().is_some(),
            ..Answer::default()
        }
    }

    /// Asks for the preview of what the tab shows now, in `place`, where it
    /// is not the one asked for last: the font's character cell, toward a
    /// picture, where this terminal shows pictures, and a line of text
    /// where it does not. With nothing to preview, or where the console
    /// draws the preview as its own text, the picture on the screen is
    /// deleted.
    pub fn ask(&mut self, preview: &Preview<'_>, place: Place) -> Answer {
        let shared = preview.shared;
        if shared.console.live == crate::ui::Live::Asking || shared.installs.running {
            return Answer::default();
        }
        if preview.in_console() {
            self.asked = None;
            self.caption = None;
            return self.drop_picture();
        }
        let console = &shared.console;
        let asked = preview.wanted().map(|wanted| Asked {
            wanted,
            grid: preview.facts.grid,
            cell_px: preview.facts.cell_px,
            graphics: console.graphics,
        });
        if asked == self.asked || self.drawing {
            return Answer::default();
        }
        self.asked = asked.clone();
        self.unpictured = None;
        let Some(asked) = asked else {
            self.caption = None;
            return self.drop_picture();
        };
        let height = height_of(preview.facts, &asked.wanted);
        let font = asked.wanted.font;
        if !console.graphics {
            // an effect has no written description: its line says that
            // this terminal shows no pictures
            if matches!(preview.source, super::Source::Effect { .. }) {
                self.caption = None;
                return self.drop_picture();
            }
            self.drawing = true;
            return Answer::effect(Effect::Summary { font, height });
        }
        let max = place.max(preview.facts.cell_px);
        if max.0 == 0 || max.1 == 0 {
            self.caption = None;
            self.unpictured = Some(Unpictured::NoRoom);
            return self.drop_picture();
        }
        self.drawing = true;
        if asked.wanted.detail {
            self.caption = None;
            return Answer::effect(Effect::Specimen {
                font,
                height,
                max,
                display: display(preview.facts).0,
            });
        }
        Answer::effect(Effect::Cell { font, height })
    }

    /// The font's character cell has come, `cell` with the height its
    /// faces draw at: the picture at the grid it gives, where it is still
    /// the one wanted.
    pub fn celled(
        &mut self,
        preview: &Preview<'_>,
        place: Place,
        font: &str,
        height: u32,
        cell: Result<(Cell, u32), String>,
    ) -> Answer {
        let Some(asked) = self.asked.clone() else {
            self.drawing = false;
            return Answer::default();
        };
        let wanted = asked.wanted;
        // a cell for a change no longer highlighted: the next is asked for
        if wanted.font != font
            || height_of(preview.facts, &wanted) != height
            || preview.wanted().as_ref() != Some(&wanted)
        {
            self.drawing = false;
            return Answer::default();
        }
        match cell {
            Ok(cell) => self.picture_of(preview, place, wanted, height, cell),
            Err(why) => {
                self.drawing = false;
                self.caption = None;
                self.unpictured = Some(Unpictured::Failed);
                Answer {
                    say: Some(words::no_picture(why)),
                    ..self.drop_picture()
                }
            }
        }
    }

    /// The picture of vtium-config's screen, or of the sample screen,
    /// laid out at the grid `cell` gives on the display, in the wanted
    /// font at `height` pixels.
    fn picture_of(
        &mut self,
        preview: &Preview<'_>,
        place: Place,
        wanted: Wanted,
        height: u32,
        (cell, face_height): (Cell, u32),
    ) -> Answer {
        let (shown_on, assumed) = display(preview.facts);
        let across =
            |px: u32, side: u32| usize::try_from(px.checked_div(side).unwrap_or(0)).unwrap_or(0);
        let grid = (
            across(shown_on.0, cell.width).min(GRID_MAX.0),
            across(shown_on.1, cell.height).min(GRID_MAX.1),
        );
        self.caption = Some(Caption {
            font: wanted.font.clone(),
            points: wanted.points,
            display: shown_on,
            assumed,
            cell,
            grid,
            outcome: None,
        });
        // the Looks and Effects tabs draw a nominal screen of sample
        // content at the change's cell, and the caption still counts the
        // real display's grid; elsewhere the loop draws vtium-config's own
        // screen at the grid
        let sample = preview.sample();
        let (shown_on, grid, screen) = if sample {
            let shown = (
                across(PREVIEW_SCREEN.0, cell.width).min(GRID_MAX.0),
                across(PREVIEW_SCREEN.1, cell.height).min(GRID_MAX.1),
            );
            (PREVIEW_SCREEN, shown, console::sample_screen(shown))
        } else {
            (shown_on, grid, Vec::new())
        };
        let shot = Shot {
            font: wanted.font,
            height,
            cell,
            face_height,
            display: shown_on,
            grid,
            screen,
            palette: vtium_settings::setting(preview.shared.document().text(), "palette")
                .unwrap_or_default(),
            color: preview.facts.color,
            max: place.max(preview.facts.cell_px),
            filter: wanted.filter,
        };
        Answer::effect(if sample {
            Effect::Picture(shot)
        } else {
            Effect::ScreenPicture(shot)
        })
    }

    /// The picture has come: it replaces the one on the screen, at the top
    /// left of `place`, where it is still the one wanted.
    pub fn pictured(
        &mut self,
        preview: &Preview<'_>,
        place: Place,
        result: Result<Picture, String>,
    ) -> Answer {
        self.drawing = false;
        let mut out = self.drop_picture();
        // a picture of a change no longer highlighted is not shown: the
        // next is asked for
        let current = self
            .asked
            .as_ref()
            .is_some_and(|a| preview.wanted().as_ref() == Some(&a.wanted));
        if !current {
            return out;
        }
        match result {
            Ok(picture) => {
                self.shown =
                    Some(place.covered((picture.width, picture.height), preview.facts.cell_px));
                out.show = Some(Show {
                    at: place.at(),
                    picture,
                });
            }
            Err(why) => {
                self.caption = None;
                self.unpictured = Some(Unpictured::Failed);
                out.say = Some(words::no_picture(why));
            }
        }
        out
    }

    /// The picture drawn through an effect has come, with what became of
    /// the effect, which the line beside the picture says. It is shown as
    /// any picture is.
    pub fn filtered(
        &mut self,
        preview: &Preview<'_>,
        place: Place,
        picture: Picture,
        outcome: Outcome,
    ) -> Answer {
        // the outcome is kept only for the preview it answers: a picture
        // that comes after the highlight moved on says nothing
        let current = self
            .asked
            .as_ref()
            .map(|a| a.wanted.clone())
            .filter(|w| preview.wanted().as_ref() == Some(w));
        if let (Some(wanted), Some(caption)) = (current, self.caption.as_mut()) {
            caption.outcome = Some((wanted, outcome));
        }
        self.pictured(preview, place, Ok(picture))
    }

    /// The detailed specimen of `font` at `height` has come: its summary
    /// is kept for the line beside it, and it is shown as a picture is.
    pub fn detailed(
        &mut self,
        preview: &Preview<'_>,
        place: Place,
        font: String,
        height: u32,
        result: Result<(Picture, Summary), String>,
    ) -> Answer {
        match result {
            Ok((picture, summary)) => {
                self.specimen = Some((font, height, summary));
                self.pictured(preview, place, Ok(picture))
            }
            Err(why) => self.pictured(preview, place, Err(why)),
        }
    }

    /// The font helper's summary of `font` at `height` has come, for a
    /// terminal that shows no pictures: it is kept for the line in place
    /// of the picture.
    pub fn summarized(&mut self, font: String, height: u32, result: Result<Summary, String>) {
        self.drawing = false;
        self.summary = Some((font, height, result));
    }
}
