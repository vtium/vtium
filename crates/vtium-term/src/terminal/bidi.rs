//! The paragraph direction in a build with the bidi feature (ADR-1004 D2,
//! D5): SCP and SPD, the direction vtium.conf's `text-direction` gives, and
//! what DECSTR and RIS return the paragraph properties to. The modes that
//! set the other properties, BDSM, 2500 and 2501, are rows of the mode
//! table (`modes.rs`).

use super::{State, Terminal};
use crate::line::LineFlags;
use vtium_parse::Params;

/// The paragraph direction vtium.conf's `text-direction` gives (ADR-1004
/// D5): the direction DECSTR, RIS and SCP 0 return a paragraph to, and
/// mode 2501's state after them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDirection {
    /// `ltr`, the default: paragraphs run left to right and their direction
    /// is not detected.
    #[default]
    Ltr,
    /// `rtl`: paragraphs run right to left and their direction is not
    /// detected.
    Rtl,
    /// `auto`: each paragraph's direction is detected from its first strong
    /// character, and a paragraph with none runs left to right.
    Auto,
}

impl TextDirection {
    /// The paragraph properties a paragraph starts with: implicit mode, no
    /// box drawing mirrored, and the direction and detection this gives.
    pub(super) fn paragraph(self) -> LineFlags {
        match self {
            Self::Ltr => LineFlags::empty(),
            Self::Rtl => LineFlags::RTL,
            Self::Auto => LineFlags::DETECT,
        }
    }
}

impl Terminal {
    /// Sets the paragraph direction vtium.conf's `text-direction` gives.
    /// The current paragraph properties return to the defaults it gives,
    /// and so does every row of both screens that holds no text, so a
    /// terminal the host sets up before its program writes starts in that
    /// direction; a row already written keeps its values (ADR-1004 D5).
    pub fn set_text_direction(&mut self, direction: TextDirection) {
        self.state.host.text_direction = direction;
        self.state
            .screen
            .set_unwritten_paragraphs(direction.paragraph());
    }
}

impl State {
    /// SCP, `CSI Ps1 ; Ps2 SP k`: 1 sets the cursor's paragraph left to
    /// right and 2 right to left, and 0, or no parameter, returns it to the
    /// direction `text-direction` gives. Any other value, and the second
    /// parameter, are ignored (ADR-1004 D2).
    pub(super) fn select_path(&mut self, params: &Params) {
        let rtl = match params.get(0).unwrap_or(0) {
            0 => self.host.text_direction == TextDirection::Rtl,
            1 => false,
            2 => true,
            _ => return,
        };
        self.screen.set_paragraph_flag(LineFlags::RTL, rtl);
    }

    /// SPD, `CSI Ps1 ; Ps2 SP S`: 0, or no parameter, is SCP 1 and 3 is SCP
    /// 2, as the Terminal Working Group's recommendation allows and mintty
    /// documents. Every other value, and the second parameter, are ignored
    /// (ADR-1004 D2).
    pub(super) fn select_presentation_direction(&mut self, params: &Params) {
        let rtl = match params.get(0).unwrap_or(0) {
            0 => false,
            3 => true,
            _ => return,
        };
        self.screen.set_paragraph_flag(LineFlags::RTL, rtl);
    }

    /// Returns the current paragraph properties to the defaults
    /// `text-direction` gives and applies them to the cursor's paragraph,
    /// as DECSTR and RIS do (ADR-1004 D2).
    pub(super) fn reset_paragraph(&mut self) {
        let defaults = self.host.text_direction.paragraph();
        self.screen.set_paragraph(defaults);
    }
}
