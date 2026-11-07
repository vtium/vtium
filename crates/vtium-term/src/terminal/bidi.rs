//! The paragraph direction in a build with the bidi feature (ADR-1004 D2,
//! D5): SCP and SPD, the direction vtium.conf's `text-direction` gives, and
//! what DECSTR and RIS return the paragraph properties to. The modes that
//! set the other properties, BDSM, 2500 and 2501, are rows of the mode
//! table (`modes.rs`).
//!
//! The input side of right-to-left text is here too: the arrow keys mode
//! 1243 swaps (ADR-1004 D11), and the logical cell and pixel a pointer
//! report names when the pointer is over a reordered row (D10).

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

impl Terminal {
    /// Sets the paragraph direction vtium.conf's `text-direction` gives.
    /// The current paragraph properties return to the defaults it gives,
    /// and so does every row of both screens that holds no text, so a
    /// terminal the host sets up before its program writes starts in that
    /// direction; a row already written keeps its values (ADR-1004 D5).
    pub fn set_text_direction(&mut self, direction: TextDirection) {
        self.state.host.text_direction = direction;
        let defaults = self.state.paragraph_defaults();
        self.state.screen.set_unwritten_paragraphs(defaults);
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

    /// The paragraph properties a paragraph starts with: BDSM, 2500 and
    /// 2501 at their rows' initial states, which `text-direction` sets for
    /// 2501, and the direction `text-direction` gives.
    pub(super) fn paragraph_defaults(&self) -> LineFlags {
        let mut defaults = super::modes::paragraph_modes_initial(self);
        if self.host.text_direction == TextDirection::Rtl {
            defaults.insert(LineFlags::RTL);
        }
        defaults
    }

    /// Returns the current paragraph properties to their defaults and
    /// applies them to the cursor's paragraph at its first logical
    /// position, as DECSTR and RIS do (ADR-1004 D2).
    pub(super) fn reset_paragraph(&mut self) {
        let defaults = self.paragraph_defaults();
        self.screen.set_paragraph(defaults);
    }
}

/// The X11 keysyms of the Left and Right arrow keys and their keypad twins
/// (xkbcommon's `xkbcommon-keysyms.h`).
const LEFT: u32 = 0xff51;
const RIGHT: u32 = 0xff53;
const KP_LEFT: u32 = 0xff96;
const KP_RIGHT: u32 = 0xff98;

impl State {
    /// The arrow key `keysym` sends: Left as Right and Right as Left, the
    /// keypad's included, while mode 1243 is set and the cursor's
    /// paragraph resolves right to left, so an editor that moves through a
    /// line in logical order moves the way the arrow points over
    /// right-to-left text (ADR-1004 D11). Every other key, Home and End
    /// among them, is itself.
    pub(super) fn swapped_arrow(&self, keysym: u32) -> u32 {
        let swap = self.modes.arrow_direction() && self.screen.cursor_paragraph_rtl();
        match keysym {
            LEFT if swap => RIGHT,
            RIGHT if swap => LEFT,
            KP_LEFT if swap => KP_RIGHT,
            KP_RIGHT if swap => KP_LEFT,
            other => other,
        }
    }

    /// The cell and the pixel column a pointer report names for a pointer
    /// over cell `col`, `row` of the view at pixel column `px`: the logical
    /// cell drawn there, and that cell's left edge plus the pointer's
    /// offset inside the cell it is over, so a program addresses the cell
    /// it wrote (ADR-1004 D10). Without the cell's size in pixels the pixel
    /// column is as the host gave it.
    pub(super) fn logical_pointer(&self, col: usize, row: usize, px: u32) -> (usize, u32) {
        let logical = self.screen.pointer_col(col, row);
        let px = match self.host.cell {
            Some((width, _)) if logical != col => {
                let at = |c: usize| u32::try_from(c).unwrap_or(u32::MAX).saturating_mul(width);
                let offset = px.saturating_sub(at(col)).min(width.saturating_sub(1));
                at(logical).saturating_add(offset)
            }
            _ => px,
        };
        (logical, px)
    }
}
