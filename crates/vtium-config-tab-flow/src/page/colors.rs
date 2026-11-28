//! The roles the Flow page draws each element through. A box's title takes
//! its kind's role, displays, terminals, font sets and inputs, and its
//! border takes its state's role, placed, waiting, set aside or picked.
//! The highlight is a heavy border and a bold, underlined title in the
//! box's own roles, so it shows without color. Every role keeps its own
//! attributes where no color is drawn, and every mark a color carries has
//! its word or its glyph too: `in no path`, `not connected`, `✓`, a double
//! border on a template, a dashed one in NEW PATH.
//!
//! References:
//! - ADR-1403 D7, "Colors, borders and lines": the kind's color on the
//!   title, the state's on the border, and the kinds of line.
//! - ADR-1206 D4, the color roles: every colored element is drawn through
//!   a named role.

use vtium_config_widgets::color::Style;
use vtium_config_widgets::roles::Role;

use super::layout::{Mood, NoteKind, Obj};
use crate::parts::Stage;

/// The role of a box's title and of its kind's column and group titles.
pub const fn stage_role(stage: Stage) -> Role {
    match stage {
        Stage::Display => Role::Display,
        Stage::Terminal => Role::Terminal,
        Stage::Look => Role::FontSet,
        Stage::Input => Role::Input,
    }
}

/// The role of the inside of a box of `stage`, which a theme may fill.
pub const fn fill_role(stage: Stage) -> Role {
    match stage {
        Stage::Display => Role::DisplayFill,
        Stage::Terminal => Role::TerminalFill,
        Stage::Look => Role::FontSetFill,
        Stage::Input => Role::InputFill,
    }
}

/// The stage whose kind a box shows.
pub fn obj_stage(obj: &Obj) -> Stage {
    match obj {
        Obj::Doc(_, s) | Obj::New(s) | Obj::Pal(s) | Obj::Roam(s) | Obj::Nothing(s) => s.stage(),
        Obj::Others => Stage::Input,
    }
}

/// The role of a box's border, by its state.
pub const fn mood_role(mood: Mood) -> Role {
    match mood {
        Mood::Placed => Role::Placed,
        Mood::Waiting => Role::Waiting,
        Mood::Aside => Role::SetAside,
        Mood::Picked => Role::Picked,
    }
}

/// The role of a line of text on the page.
pub const fn note_role(kind: NoteKind) -> Role {
    match kind {
        NoteKind::Heading => Role::Heading,
        NoteKind::Needs => Role::Waiting,
        NoteKind::Ready => Role::Active,
        NoteKind::Grey | NoteKind::Stub => Role::Dim,
        NoteKind::Title(stage) => stage_role(stage),
    }
}

/// Whether a line of text of `kind` is drawn bold: the titles, and NEW
/// PATH's status once the path is complete.
pub const fn note_bold(kind: NoteKind) -> bool {
    matches!(kind, NoteKind::Title(_) | NoteKind::Ready)
}

/// `style` in bold, the highlight's weight.
pub const fn bold(style: Style) -> Style {
    Style {
        bold: true,
        ..style
    }
}

/// `style` in bold and underlined, the highlighted box's title.
pub const fn marked(style: Style) -> Style {
    Style {
        bold: true,
        underline: true,
        ..style
    }
}
