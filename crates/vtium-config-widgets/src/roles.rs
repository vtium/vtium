//! The color roles: every colored element of the TUI is drawn through a
//! named role, never through a color. One table gives each role its look,
//! so a later theme can set the roles by name without touching a view.
//!
//! The table gives each role today's look, the `console` set: the
//! console's sixteen colors by SGR number, so the person's palette
//! applies, with blue and bright blue left out of every role as
//! [`crate::color`] leaves them out. Where the TUI draws no color, under
//! `NO_COLOR` or on a terminal without color, each role keeps only its
//! attributes, bold, underline and reverse video. A mark that a color
//! carries is also carried by its word, such as `not active` beside a
//! font, so each screen reads the same without color.
//!
//! References:
//! - ADR-1206 D4, the roles' table: each role, what it colors and its
//!   look, the new ones as the approved mock-ups draw them.
//! - ADR-1206 D10: sixteen colors, `NO_COLOR` and a monochrome console.
//! - ADR-1403 D11, "One set of marks for a font": the words that carry
//!   each mark, and the role of each.

use crate::color::{Color, Style};

/// A named role of an element the TUI colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// The top row's menu names and tab names.
    MenuBar,
    /// An open menu's box and items.
    Menu,
    /// The highlighted menu item.
    MenuHighlight,
    /// A menu item that cannot act here, and its reason.
    MenuGreyed,
    /// A menu's or a tab's Alt letter, and an item's letter in a menu.
    Letter,
    /// A tab's name.
    Tab,
    /// The open tab's name.
    TabOpen,
    /// The top row's name of the document and its ●.
    Facts,
    /// The top row's count of changes not saved.
    NotSaved,
    /// The problems count, and the mark beside a problem.
    Problems,
    /// The attention color: LIVE while a live macro run plays, and the
    /// mark that a live trial runs.
    Live,
    /// The top row's "scratch copy" while a macro plays on a scratch copy.
    ScratchCopy,
    /// An action bar button's key.
    BarKey,
    /// An action bar button's name.
    BarButton,
    /// A row of a list.
    Row,
    /// The highlighted row of a list.
    RowHighlight,
    /// The mark of an active font, effect or theme.
    Active,
    /// The mark of an item that is not active, or that root turned off.
    Inactive,
    /// The mark of a font a configuration uses that is not installed.
    Missing,
    /// A grey line: a hint, a placeholder, a fact of less weight.
    Dim,
    /// The search row's prompt and entry.
    Search,
    /// The syntax help at the right of the search row.
    SearchHelp,
    /// A heading inside the viewer.
    Heading,
    /// A line that needs attention but does not stop a save.
    Warning,
    /// An added line in Show changes and Save's review.
    DiffAdded,
    /// A removed line.
    DiffRemoved,
    /// A line of context.
    DiffContext,
    /// The words that changed inside a line.
    DiffWord,
    /// A Flow box's title, for a display.
    Display,
    /// A Flow box's title, for a terminal.
    Terminal,
    /// A Flow box's title, for a font set.
    FontSet,
    /// A Flow box's title, for an input.
    Input,
    /// A Flow box's border, for a part placed in a path.
    Placed,
    /// A Flow box's border, for a part waiting in NEW PATH.
    Waiting,
    /// A Flow box's border, for a part set aside.
    SetAside,
    /// A Flow box's border, for a part picked.
    Picked,
    /// The lines between Flow boxes.
    Line,
    /// The inside of a display's Flow box.
    DisplayFill,
    /// The inside of a terminal's Flow box.
    TerminalFill,
    /// The inside of a font set's Flow box.
    FontSetFill,
    /// The inside of an input's Flow box.
    InputFill,
}

/// The terminal's own colors, with no attribute.
const OWN: Style = Style::PLAIN;

/// Reverse video, the TUI's mark of what is highlighted or open.
const REVERSE: Style = Style {
    reverse: true,
    ..Style::PLAIN
};

/// Bold, with no color.
const BOLD: Style = Style {
    bold: true,
    ..Style::PLAIN
};

/// Every role, its name, and its look in the `console` set, in the order
/// of ADR-1206 D4's table.
const TABLE: [(Role, &str, Style); 41] = [
    (Role::MenuBar, "menu-bar", OWN),
    (Role::Menu, "menu", OWN),
    (Role::MenuHighlight, "menu-highlight", REVERSE),
    (Role::MenuGreyed, "menu-greyed", Style::text(Color::Grey)),
    (
        Role::Letter,
        "letter",
        Style {
            bold: true,
            underline: true,
            ..Style::PLAIN
        },
    ),
    (Role::Tab, "tab", OWN),
    (Role::TabOpen, "tab-open", REVERSE),
    (Role::Facts, "facts", OWN),
    (Role::NotSaved, "not-saved", OWN),
    (Role::Problems, "problems", Style::text(Color::BrightRed)),
    (
        Role::Live,
        "live",
        Style {
            bold: true,
            reverse: true,
            ..Style::text(Color::BrightYellow)
        },
    ),
    (Role::ScratchCopy, "scratch-copy", BOLD),
    (Role::BarKey, "bar-key", BOLD),
    (Role::BarButton, "bar-button", REVERSE),
    (Role::Row, "row", OWN),
    (Role::RowHighlight, "row-highlight", REVERSE),
    (Role::Active, "active", Style::text(Color::Green)),
    (Role::Inactive, "inactive", Style::text(Color::Grey)),
    (Role::Missing, "missing", Style::text(Color::Red)),
    (Role::Dim, "dim", Style::text(Color::Grey)),
    (Role::Search, "search", OWN),
    (Role::SearchHelp, "search-help", Style::text(Color::Grey)),
    (
        Role::Heading,
        "heading",
        Style {
            bold: true,
            ..Style::text(Color::BrightWhite)
        },
    ),
    (Role::Warning, "warning", Style::text(Color::Yellow)),
    (Role::DiffAdded, "diff-added", Style::text(Color::Green)),
    (Role::DiffRemoved, "diff-removed", Style::text(Color::Red)),
    (Role::DiffContext, "diff-context", OWN),
    (Role::DiffWord, "diff-word", BOLD),
    (Role::Display, "display", Style::text(Color::Cyan)),
    (Role::Terminal, "terminal", Style::text(Color::Yellow)),
    (Role::FontSet, "font-set", Style::text(Color::Magenta)),
    (Role::Input, "input", Style::text(Color::Green)),
    (Role::Placed, "placed", Style::text(Color::White)),
    (Role::Waiting, "waiting", Style::text(Color::Yellow)),
    (Role::SetAside, "set-aside", Style::text(Color::Grey)),
    (Role::Picked, "picked", Style::text(Color::BrightMagenta)),
    (Role::Line, "line", Style::text(Color::White)),
    (Role::DisplayFill, "display-fill", OWN),
    (Role::TerminalFill, "terminal-fill", OWN),
    (Role::FontSetFill, "font-set-fill", OWN),
    (Role::InputFill, "input-fill", OWN),
];

impl Role {
    /// Every role, in the order of ADR-1206 D4's table.
    pub fn all() -> impl Iterator<Item = Role> {
        TABLE.iter().map(|(role, _, _)| *role)
    }

    /// The role's row of the table. The test `every_role_has_one_row`
    /// checks that every role has exactly one, so the fallback, the
    /// terminal's own colors, never applies.
    fn entry(self) -> (&'static str, Style) {
        TABLE
            .iter()
            .find(|(role, _, _)| *role == self)
            .map_or(("", OWN), |(_, name, look)| (*name, *look))
    }

    /// The role's name, as the docs and a later theme name it, such as
    /// `menu-highlight`.
    pub fn name(self) -> &'static str {
        self.entry().0
    }

    /// The role named `name`.
    pub fn of_name(name: &str) -> Option<Role> {
        TABLE
            .iter()
            .find(|(_, n, _)| *n == name)
            .map(|(r, _, _)| *r)
    }

    /// The role's look in the `console` set, today's look.
    pub fn look(self) -> Style {
        self.entry().1
    }

    /// The style the role draws in: its look where the TUI draws color,
    /// and only its attributes where it draws none, under `NO_COLOR` or on
    /// a terminal without color ([`crate::color::enabled`]).
    pub fn style(self, color: bool) -> Style {
        let look = self.look();
        if color {
            look
        } else {
            Style {
                fg: None,
                rgb: None,
                bg: None,
                back: None,
                ..look
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::palette;

    /// What a look is made of, for comparing against the record: the
    /// text's SGR color, if any, and bold, underline and reverse video.
    fn parts(s: Style) -> (Option<u8>, bool, bool, bool) {
        (s.fg.map(Color::fg), s.bold, s.underline, s.reverse)
    }

    /// Each role's look as ADR-1206 D4's table gives it, written against
    /// color.rs. The match names every role, so a role added without a
    /// look fails to compile here.
    fn record(role: Role) -> (Option<u8>, bool, bool, bool) {
        let own = (None, false, false, false);
        let text = |c: Color| (Some(c.fg()), false, false, false);
        match role {
            Role::MenuBar
            | Role::Menu
            | Role::Tab
            | Role::Facts
            | Role::NotSaved
            | Role::Row
            | Role::Search
            | Role::DiffContext
            | Role::DisplayFill
            | Role::TerminalFill
            | Role::FontSetFill
            | Role::InputFill => own,
            Role::MenuHighlight | Role::TabOpen | Role::BarButton | Role::RowHighlight => {
                (None, false, false, true)
            }
            Role::Letter => (None, true, true, false),
            Role::ScratchCopy | Role::BarKey | Role::DiffWord => (None, true, false, false),
            Role::Live => (Some(93), true, false, true),
            Role::Heading => (Some(97), true, false, false),
            Role::Problems => text(palette::ERROR),
            Role::Dim | Role::MenuGreyed | Role::SearchHelp | Role::Inactive => text(palette::GREY),
            Role::SetAside => text(Color::Grey),
            Role::Active | Role::DiffAdded | Role::Input => text(Color::Green),
            Role::Missing | Role::DiffRemoved => text(Color::Red),
            Role::Warning | Role::Terminal | Role::Waiting => text(Color::Yellow),
            Role::Display => text(Color::Cyan),
            Role::FontSet => text(Color::Magenta),
            Role::Placed | Role::Line => text(Color::White),
            Role::Picked => text(Color::BrightMagenta),
        }
    }

    #[test]
    fn every_role_has_one_row_and_a_name_of_its_own() {
        let all: Vec<Role> = Role::all().collect();
        assert_eq!(all.len(), 41);
        for (i, role) in all.iter().enumerate() {
            assert!(!all.iter().skip(i + 1).any(|r| r == role), "{role:?} twice");
            assert!(!role.name().is_empty(), "{role:?}");
            assert_eq!(Role::of_name(role.name()), Some(*role));
        }
        assert_eq!(Role::of_name("menu-highlight"), Some(Role::MenuHighlight));
        assert_eq!(Role::of_name("blue"), None);
    }

    #[test]
    fn each_role_takes_today_s_look() {
        for role in Role::all() {
            assert_eq!(parts(role.look()), record(role), "{}", role.name());
            let look = role.look();
            assert_eq!(
                (look.bg, look.back, look.rgb, look.italic),
                (None, None, None, false),
                "{}",
                role.name()
            );
        }
        // the SGR numbers the record writes beside the roles
        assert_eq!(Role::Problems.look().fg.map(Color::fg), Some(91));
        assert_eq!(Role::Active.look().fg.map(Color::fg), Some(32));
        assert_eq!(Role::Dim.look().fg.map(Color::fg), Some(90));
        assert_eq!(Role::Warning.look().fg.map(Color::fg), Some(33));
        assert_eq!(Role::Picked.look().fg.map(Color::fg), Some(95));
    }

    #[test]
    fn no_role_is_blue_or_bright_blue() {
        for role in Role::all() {
            for c in [role.look().fg, role.look().bg].into_iter().flatten() {
                assert!(![4, 12].contains(&c.index()), "{}", role.name());
            }
        }
    }

    #[test]
    fn without_color_a_role_keeps_only_its_attributes() {
        for role in Role::all() {
            let plain = role.style(false);
            assert_eq!(
                (plain.fg, plain.rgb, plain.bg, plain.back),
                (None, None, None, None),
                "{}",
                role.name()
            );
            let (_, bold, underline, reverse) = record(role);
            assert_eq!(
                (plain.bold, plain.underline, plain.reverse),
                (bold, underline, reverse),
                "{}",
                role.name()
            );
            assert_eq!(role.style(true), role.look());
        }
    }
}
