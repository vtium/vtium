//! What the TUI does in a vtium console it can switch, `Live::On`
//! (ADR-1201 D4): the preview of the Fonts and Looks views is text the
//! console draws itself, and `s` in the Fonts view saves the font the
//! console shows into the draft.
//!
//! The preview's text is the content the pictures show elsewhere, taken
//! from where the pictures take it: the Fonts view prints the detailed
//! specimen's lines ([`crate::specimen::detail_lines`] and
//! [`crate::specimen::SCRIPT_LINE`]), and the Looks view prints the sample
//! screen ([`super::shot::sample_rows`]). The console draws them in the
//! font it draws now, so after `t` they show the font tried. Its
//! superscripts and subscripts are SGR 73 and 74, which vtium draws as
//! small glyphs (ADR-1202), and the sample screen's colors are the
//! console's own palette.
//!
//! `s` writes the font the console shows where the console's head takes
//! its font from, as the Flow view's card would: the head's look's
//! `font-name` where that look names one, through the paths' plan, which
//! makes a look for the font and size and removes the old look once no
//! head uses it ([`super::paths::plan`]); otherwise the top-level
//! `font-name`. The head is the one `VTIUM_HEAD` names, which vtium sets
//! in each console's session (ADR-1401 D5). The draft is written only
//! when `vtium check-config` passes it ([`super::desk::Op::WriteChecked`]).

use super::canvas::Canvas;
use super::color::Style;
use super::desk::Op;
use super::edit::Change;
use super::flow::{Flow, Look};
use super::model::{App, Effect, Live, View};
use super::words;

/// Where the font the console shows is saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// The look of the console's head, which names a font of its own:
    /// the head's name, and the look it draws with afterwards.
    Look { head: String, look: String },
    /// The top-level `font-name`.
    Top,
}

/// The Fonts view's preview text for `locale`: the detailed specimen's
/// lines, then its line of superscripts and subscripts, each run with its
/// style.
pub fn fonts_lines(locale: &str) -> Vec<Vec<(&'static str, Style)>> {
    let mut out: Vec<Vec<(&'static str, Style)>> = crate::specimen::detail_lines(locale)
        .into_iter()
        .map(|line| vec![(line, Style::PLAIN)])
        .collect();
    out.push(
        crate::specimen::SCRIPT_LINE
            .iter()
            .map(|&(run, script)| {
                (
                    run,
                    Style {
                        script,
                        ..Style::PLAIN
                    },
                )
            })
            .collect(),
    );
    out
}

/// The preview text of `app`'s view, empty for a view with none.
pub fn lines(app: &App) -> Vec<Vec<(&'static str, Style)>> {
    match app.view {
        View::Fonts => fonts_lines(&app.inputs.locale),
        View::Looks => super::shot::sample_rows(),
        _ => Vec::new(),
    }
}

/// Draws the preview text on `canvas` in the picture area, where a
/// picture would go, as many lines as the area holds, each cut at the
/// screen's edge.
pub fn draw(canvas: &mut Canvas, app: &App) {
    if !app.text_preview() {
        return;
    }
    let (row, col) = super::view::picture_at(app);
    let (_, rows) = super::view::picture_area(app);
    let top = row.saturating_sub(1);
    let left = col.saturating_sub(1);
    for (i, line) in lines(app).into_iter().take(rows).enumerate() {
        let mut x = left;
        for (text, style) in line {
            x = canvas.put_cells(x, top.saturating_add(i), text, style);
        }
    }
}

impl App {
    /// Whether the Fonts and Looks views preview as text the console
    /// draws: in a vtium console the TUI can switch, so `t` shows a font
    /// on the console itself. Anywhere else they preview as pictures, or
    /// as a written description where the terminal shows no pictures.
    pub fn text_preview(&self) -> bool {
        self.live == Live::On
    }

    /// The console's head in `flow`, by `VTIUM_HEAD`, and the look it
    /// draws with where that look names a font of its own.
    fn head_look<'f>(&self, flow: &'f Flow) -> Option<(&'f str, &'f Look)> {
        let name = self.inputs.head.as_deref()?;
        let h = flow.heads.iter().position(|h| h.name == name)?;
        let look = flow.look_of(h).and_then(|l| flow.looks.get(l))?;
        (!look.frit.is_empty()).then_some((flow.heads.get(h)?.name.as_str(), look))
    }

    /// The font the draft gives the console's head: its look's
    /// `font-name` where the look names one, else the top-level
    /// `font-name`; `None` for vtium's default font.
    pub fn head_font(&self) -> Option<String> {
        let text = self.conf_text()?;
        let flow = crate::flowmodel::build(text, &self.present, None);
        match self.head_look(&flow) {
            Some((_, look)) => Some(look.frit.clone()),
            None => vtium_settings::setting(text, "font-name"),
        }
    }

    /// The changes to `text`, the draft, that make the console's head
    /// draw `font`, and where they write it; empty where the draft gives
    /// it that font already. Refused where no head is known and a look of
    /// the draft names a font, since the top level might not reach the
    /// console's head.
    pub(crate) fn tried_changes(
        &self,
        text: &str,
        font: &str,
    ) -> Result<(Vec<Change>, Place), String> {
        let flow = crate::flowmodel::build(text, &self.present, None);
        if let Some((head, look)) = self.head_look(&flow) {
            let mut doc = super::paths::read(&flow);
            for path in &mut doc.paths {
                if path.name.as_deref() == Some(head) {
                    path.font = Some(font.to_owned());
                }
            }
            let changes = super::paths::plan(&flow, &doc);
            // the look the head names afterwards: a new one for the font
            // and size, or the one it had where the font is already its
            let after = changes
                .iter()
                .find_map(|c| match c {
                    Change::HeadKey {
                        name,
                        key: "look",
                        value: Some(v),
                    } if name == head => Some(v.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| look.name.clone());
            let place = Place::Look {
                head: head.to_owned(),
                look: after,
            };
            return Ok((changes, place));
        }
        if self.inputs.head.is_none() && flow.looks.iter().any(|l| !l.frit.is_empty()) {
            return Err(words::tried_no_head(font));
        }
        let changes = if vtium_settings::setting(text, "font-name").as_deref() == Some(font) {
            Vec::new()
        } else {
            vec![Change::Top {
                key: "font-name",
                value: Some(font.to_owned()),
            }]
        };
        Ok((changes, Place::Top))
    }

    /// `s` in the Fonts view of a vtium console: writes the font the
    /// console shows into the draft, where the console's head takes its
    /// font from, with the changes not yet saved, once `vtium
    /// check-config` passes the result; a draft it fails is not written,
    /// and the message line shows what the check says.
    pub(crate) fn save_tried(&mut self) -> Vec<Effect> {
        let Some(draft) = &self.draft else {
            self.message = Some(words::NO_DESK.to_owned());
            return Vec::new();
        };
        let Some(font) = self.shown_font.clone() else {
            self.message = Some(words::TRIED_UNKNOWN.to_owned());
            return Vec::new();
        };
        let name = draft.name.clone();
        let text = draft.text.clone();
        let (changes, place) = match self.tried_changes(&text, &font) {
            Ok(planned) => planned,
            Err(why) => {
                self.message = Some(why);
                return Vec::new();
            }
        };
        if changes.is_empty() && self.unsaved == 0 {
            self.message = Some(words::tried_already(&font));
            return Vec::new();
        }
        let new = match super::edit::apply_all(&text, &changes) {
            Ok(new) => new,
            Err(fail) => {
                self.message = Some(fail.text().to_owned());
                return Vec::new();
            }
        };
        self.saving_tried = Some(match place {
            Place::Look { head, look } => words::tried_saved_look(&font, &head, &look, &name),
            Place::Top => words::tried_saved_top(&font, &name),
        });
        vec![Effect::Desk(Op::WriteChecked(new))]
    }
}
