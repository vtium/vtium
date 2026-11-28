//! The frame every screen of the TUI shares: the top row, the viewer and
//! the action bar. The top row holds the File and Actions menus, the six
//! tabs and the open document's facts. The viewer fills the rows between
//! a rule under the top row and a rule over the bottom row, and the open
//! tab draws in it. The bottom row is the action bar, where each action
//! is a button that names its key.
//!
//! [`FrameLayout`] works out where everything goes from the screen's size
//! and what the frame holds, without drawing. Its [`Places`] say what a
//! click at each cell hits, so update reads the same places the view
//! draws, and a click lands on what it hits. The view then draws the
//! frame from the same layout with [`FrameLayout::draw`], and the open tab
//! draws into [`Regions::viewer`].
//!
//! References:
//! - ADR-1403 D11, "The frame": the three regions, the top row's facts
//!   and how they shorten, and the message on the viewer's last row.
//! - ADR-1206 D4, "One frame for every screen": one layout function
//!   computes the frame with ratatui's Layout and returns the places that
//!   update reads.
//! - ADR-1206 D4, the color roles: every colored element is drawn through
//!   a named role.

mod bar;
mod layout;
mod places;
mod top;

pub use bar::{Bar, Button, Side};
pub use layout::{Options, Regions, SEARCH_ROWS_MAX};
pub use places::{Hit, Places};
pub use top::{Count, Facts, MenuName, Mode, TabName, TopRow};

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::text::Span;

use crate::canvas::printable;
use crate::color::Style;
use crate::roles::Role;

/// A message on the viewer's last row, such as what the last action did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The message, one line. A longer one is cut with `…`.
    pub text: String,
    /// The role it is drawn in, such as [`Role::Problems`] for a refusal.
    pub role: Role,
}

/// Where everything of the frame goes on a screen, worked out once from
/// the screen's size, the top row, the action bar's buttons and what the
/// frame holds.
#[derive(Clone, Debug)]
pub struct FrameLayout {
    regions: Regions,
    top: top::TopLayout,
    bar: bar::BarLayout,
    places: Places,
}

impl FrameLayout {
    /// The frame of a screen of `area`, with `top` on its top row and the
    /// buttons of `bar` on its action bar.
    pub fn new(area: Rect, options: Options, top: &TopRow<'_>, bar: &Bar) -> FrameLayout {
        let regions = Regions::of(area, options);
        let top = top::layout(regions.top, top);
        let bar = bar::layout(regions.bar, bar);
        let places = Places {
            menus: top.menus,
            tabs: top.tabs.clone(),
            not_saved: top.not_saved,
            problems: top.problems,
            buttons: bar.shown.clone(),
            more: bar.more,
            hidden: bar.hidden.clone(),
            regions,
        };
        FrameLayout {
            regions,
            top,
            bar,
            places,
        }
    }

    /// The frame's regions: the open tab draws in [`Regions::viewer`].
    pub fn regions(&self) -> &Regions {
        &self.regions
    }

    /// What a click at each cell of the frame hits.
    pub fn places(&self) -> &Places {
        &self.places
    }

    /// Draws the top row, the two rules, the message and the action bar
    /// into `buf`, in color where `color` holds. The viewer and the search
    /// rows are left to the open tab and the search component.
    pub fn draw(&self, buf: &mut Buffer, message: Option<&Message>, color: bool) {
        clear(buf, self.regions.top);
        clear(buf, self.regions.bar);
        paint(buf, &self.top.runs, color);
        for rule in [self.regions.top_rule, self.regions.bottom_rule] {
            let line = "─".repeat(usize::from(rule.width));
            paint(buf, &[Run::new(rule.x, rule.y, line, &[])], color);
        }
        if let (Some(area), Some(message)) = (self.regions.message, message) {
            clear(buf, area);
            let text = cut(&message.text, area.width.saturating_sub(1));
            paint(
                buf,
                &[Run::new(
                    area.x.saturating_add(1),
                    area.y,
                    text,
                    &[message.role],
                )],
                color,
            );
        }
        paint(buf, &self.bar.runs, color);
    }
}

/// A run of text at a place on the screen, in the roles it is drawn
/// through: the first role's look, with each later role's attributes and
/// color laid over it, as an Alt letter's bold underline lies over the
/// open tab's reverse video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub x: u16,
    pub y: u16,
    pub text: String,
    pub roles: Vec<Role>,
}

impl Run {
    pub(crate) fn new(x: u16, y: u16, text: impl Into<String>, roles: &[Role]) -> Run {
        Run {
            x,
            y,
            text: text.into(),
            roles: roles.to_vec(),
        }
    }

    /// The cells the run takes.
    pub(crate) fn width(&self) -> u16 {
        width(&self.text)
    }

    /// The run's place on its row.
    pub(crate) fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width(), 1)
    }
}

/// `text` at `x` in the role `base`, with the first character that matches
/// `letter`, in either case, laid over with the role `letter`.
pub(crate) fn lettered(x: u16, y: u16, text: &str, letter: char, base: Role) -> Vec<Run> {
    let at = text
        .char_indices()
        .find(|(_, c)| c.to_lowercase().eq(letter.to_lowercase()));
    let Some((i, c)) = at else {
        return vec![Run::new(x, y, text, &[base])];
    };
    let (before, rest) = text.split_at(i);
    let after = rest.get(c.len_utf8()..).unwrap_or("");
    let lx = x.saturating_add(width(before));
    let ax = lx.saturating_add(width(c.encode_utf8(&mut [0; 4])));
    let mut out = Vec::new();
    if !before.is_empty() {
        out.push(Run::new(x, y, before, &[base]));
    }
    out.push(Run::new(lx, y, c.to_string(), &[base, Role::Letter]));
    if !after.is_empty() {
        out.push(Run::new(ax, y, after, &[base]));
    }
    out
}

/// The style of `roles` laid one over another: the color of the last role
/// that has one, and every role's bold, underline and reverse video.
pub(crate) fn merged(roles: &[Role], color: bool) -> Style {
    let mut out = Style::PLAIN;
    for role in roles {
        let s = role.style(color);
        if s.fg.is_some() {
            out.fg = s.fg;
        }
        if s.bg.is_some() {
            out.bg = s.bg;
        }
        out.bold |= s.bold;
        out.underline |= s.underline;
        out.reverse |= s.reverse;
    }
    out
}

/// Writes each run into `buf`, cut at the buffer's edge, so a run never
/// reaches past the screen.
pub(crate) fn paint(buf: &mut Buffer, runs: &[Run], color: bool) {
    let area = buf.area;
    for run in runs {
        if run.y < area.y || run.y >= area.bottom() || run.x < area.x || run.x >= area.right() {
            continue;
        }
        buf.set_stringn(
            run.x,
            run.y,
            &run.text,
            usize::from(area.right().saturating_sub(run.x)),
            merged(&run.roles, color).cell(),
        );
    }
}

/// Blanks every cell of `area` that lies in `buf`, as ratatui-widgets'
/// `Clear` does under a popup, so nothing of what was drawn before shows
/// through.
pub(crate) fn clear(buf: &mut Buffer, area: Rect) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
            }
        }
    }
}

/// The cells `text` takes, as ratatui measures it when it fills the
/// buffer, so a place covers the cells its text was written to.
pub(crate) fn width(text: &str) -> u16 {
    u16::try_from(Span::raw(printable(text)).width()).unwrap_or(u16::MAX)
}

/// `text` with its control characters shown as `?`, in at most `n`
/// cells, cut at its end with `…`.
pub(crate) fn cut(text: &str, n: u16) -> String {
    let text = printable(text);
    if width(&text) <= n {
        return text;
    }
    let mut out = String::new();
    let mut used = 0u16;
    for c in text.chars() {
        let w = width(c.encode_utf8(&mut [0; 4]));
        if used.saturating_add(w).saturating_add(1) > n {
            break;
        }
        out.push(c);
        used = used.saturating_add(w);
    }
    if n > 0 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_text_is_cut_with_an_ellipsis_in_its_cells() {
        assert_eq!(cut("padnoir", 10), "padnoir");
        assert_eq!(cut("padnoir", 5), "padn…");
        assert_eq!(width(&cut("名前の長い設定", 6)), 5);
        assert_eq!(cut("a\u{7}b", 5), "a?b");
        assert_eq!(cut("abc", 0), "");
    }

    #[test]
    fn a_letter_lies_over_a_highlight_and_keeps_both() {
        let s = merged(&[Role::TabOpen, Role::Letter], false);
        assert!(s.reverse && s.bold && s.underline);
        let s = merged(&[Role::Menu, Role::MenuGreyed], true);
        assert_eq!(s.fg, Role::MenuGreyed.look().fg);
        assert_eq!(merged(&[Role::Problems], false).fg, None);
    }

    #[test]
    fn painting_never_reaches_past_the_buffer() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        paint(
            &mut buf,
            &[
                Run::new(2, 0, "abcdef", &[]),
                Run::new(9, 0, "x", &[]),
                Run::new(0, 3, "y", &[]),
            ],
            true,
        );
        let row: String = (0..4)
            .map(|x| buf.cell((x, 0)).map_or("", |c| c.symbol()).to_owned())
            .collect();
        assert_eq!(row, "  ab");
    }
}
