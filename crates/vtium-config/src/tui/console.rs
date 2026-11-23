//! What the TUI does in a vtium console it can switch, `Live::On`: the
//! preview of the Looks tab is text the console draws itself.
//!
//! The preview's text is the content the pictures show elsewhere, taken
//! from where the pictures take it: a font set's preview prints the detailed
//! specimen's lines ([`crate::specimen::detail_lines`] and
//! [`crate::specimen::SCRIPT_LINE`]), and the Looks view prints the sample
//! screen ([`super::shot::sample_rows`]). The console draws them in the
//! font it draws now, so after `t` they show the font tried. Its
//! superscripts and subscripts are SGR 73 and 74, which vtium draws as
//! small glyphs, and the sample screen's colors are the console's own
//! palette.
//!
//! A font kept after `t` stays on the console alone. `s` saves the draft's
//! changes as it does in every view, and the person writes a font set into
//! the draft by choosing it on a look's card.
//!
//! References:
//! - ADR-1201 D4: the preview as text in a vtium console, and the Looks
//!   tab's `s`, which replaces the save of the font tried.
//! - ADR-1202: superscripts and subscripts as SGR 73 and 74.

use super::color::Style;
use super::flow::{Flow, Look};
use super::model::{App, Live, Shared};

/// The preview text of `app`'s open tab, as the tab gives it, empty for a
/// tab with none.
pub fn lines(app: &App) -> Vec<Vec<(&'static str, Style)>> {
    app.tab().console_lines(&app.shared, app.size)
}

impl Shared {
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
}
