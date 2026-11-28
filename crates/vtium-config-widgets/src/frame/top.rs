//! The top row: the File and Actions menus, each marked ▾, a bar, the six
//! tabs, and at the right the open document's facts. Each menu's and each
//! tab's Alt letter is drawn bold and underlined, the open tab and an open
//! menu's name in reverse video.
//!
//! The facts read `padnoir ● 35 not saved · 3 problems`: the document's
//! name, ● while it holds changes not saved, the count of those changes,
//! and the count of problems the check finds. While a macro plays, "scratch
//! copy" or LIVE follows the name. On a narrow screen the facts shorten:
//! the name goes first, then the words "not saved", down to `● 35 · 3
//! problems`. Where even that does not fit, as at 80 columns, the tabs
//! close up to one space apart, and then ● stands alone for the changes
//! not saved and opens Show changes as their count does. Each count opens
//! what it counts, and a count that cannot be opened is not drawn.
//!
//! References:
//! - ADR-1403 D11, "The frame": the top row's menus, tabs and facts, the
//!   order in which the facts shorten, and that each count opens what it
//!   counts.
//! - ADR-1403 D11, "Alt letters": F, A, N, E, L, O, S and H, bold and
//!   underlined, which stay under `NO_COLOR` because they are attributes.
//! - ADR-1208 D5: "scratch copy" and LIVE in the top row, through the
//!   roles `scratch-copy` and `live`.

use ratatui_core::layout::Rect;

use super::{Run, cut, lettered, width};
use crate::roles::Role;

/// One of the top row's two menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuName {
    /// File, on Alt+F: what a person does with the configuration as a
    /// document.
    File,
    /// Actions, on Alt+A: what acts at once on the machine or the console.
    Actions,
}

impl MenuName {
    /// The menu's name on the top row.
    pub const fn name(self) -> &'static str {
        match self {
            MenuName::File => "File",
            MenuName::Actions => "Actions",
        }
    }

    /// The menu's Alt letter.
    pub const fn letter(self) -> char {
        match self {
            MenuName::File => 'f',
            MenuName::Actions => 'a',
        }
    }
}

/// A tab's name on the top row and its Alt letter, as the tab's
/// [`crate::tab::Tab`] gives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabName<'a> {
    /// The tab's name, such as `Fonts`.
    pub name: &'a str,
    /// The letter that, with Alt, goes to the tab, such as `n` for Fonts.
    /// The first letter of the name that matches it, in either case, is
    /// drawn bold and underlined.
    pub letter: char,
}

/// What a macro's run makes of the document, which the top row says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The macro plays on a scratch copy, so nothing reaches /etc.
    ScratchCopy,
    /// The macro plays live, on the real configuration.
    Live,
}

/// A count of the top row and whether a click can open what it counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    /// How many there are. A count of nothing is not drawn.
    pub n: usize,
    /// Whether a click on the count, or its key, can open what it counts
    /// here. A count that cannot be opened is not drawn.
    pub opens: bool,
}

impl Count {
    /// `n`, which a click opens.
    pub const fn opening(n: usize) -> Count {
        Count { n, opens: true }
    }

    fn shown(self) -> bool {
        self.n > 0 && self.opens
    }
}

/// The open document's facts at the right of the top row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts<'a> {
    /// The document's name, such as `padnoir`.
    pub name: &'a str,
    /// What a macro's run makes of the document, while one plays.
    pub mode: Option<Mode>,
    /// The changes the document holds against the installed file. ●
    /// shows whenever there is one, and the count opens Show changes.
    pub not_saved: Count,
    /// The problems the check finds, which open the Problems list.
    pub problems: Count,
}

/// What the top row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopRow<'a> {
    /// The tabs, in the top row's order.
    pub tabs: &'a [TabName<'a>],
    /// The index of the open tab.
    pub open_tab: usize,
    /// The menu that is open, whose name is drawn as highlighted.
    pub open_menu: Option<MenuName>,
    /// The document's facts.
    pub facts: Facts<'a>,
}

/// The top row as it is drawn, and its places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TopLayout {
    pub runs: Vec<Run>,
    pub menus: [Rect; 2],
    pub tabs: Vec<Rect>,
    pub not_saved: Option<Rect>,
    pub problems: Option<Rect>,
}

/// A piece of the facts: its text, its role, and which count it is, if a
/// click opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Piece {
    text: String,
    role: Role,
    count: Option<Which>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    NotSaved,
    Problems,
}

fn piece(text: impl Into<String>, role: Role) -> Piece {
    Piece {
        text: text.into(),
        role,
        count: None,
    }
}

/// The facts as pieces, with or without the document's name, the count
/// not saved and the words "not saved". Without the count, ● opens Show
/// changes in its place.
fn facts(f: &Facts<'_>, name: bool, number: bool, words: bool) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    if name {
        out.push(piece(f.name, Role::Facts));
    }
    if let Some(mode) = f.mode {
        if !out.is_empty() {
            out.push(piece(" · ", Role::Facts));
        }
        out.push(match mode {
            Mode::ScratchCopy => piece("scratch copy", Role::ScratchCopy),
            Mode::Live => piece("LIVE", Role::Live),
        });
    }
    if f.not_saved.n > 0 {
        if !out.is_empty() {
            out.push(piece(" ", Role::Facts));
        }
        let dot = out.len();
        out.push(piece("●", Role::Facts));
        if f.not_saved.opens && !number {
            if let Some(p) = out.get_mut(dot) {
                p.count = Some(Which::NotSaved);
            }
        } else if f.not_saved.opens {
            let n = f.not_saved.n;
            out.push(piece(" ", Role::Facts));
            out.push(Piece {
                text: if words {
                    format!("{n} not saved")
                } else {
                    n.to_string()
                },
                role: Role::NotSaved,
                count: Some(Which::NotSaved),
            });
        }
    }
    if f.problems.shown() {
        if !out.is_empty() {
            out.push(piece(" · ", Role::Facts));
        }
        let n = f.problems.n;
        out.push(Piece {
            text: format!("{n} {}", if n == 1 { "problem" } else { "problems" }),
            role: Role::Problems,
            count: Some(Which::Problems),
        });
    }
    out
}

fn pieces_width(pieces: &[Piece]) -> u16 {
    pieces
        .iter()
        .fold(0u16, |n, p| n.saturating_add(width(&p.text)))
}

/// The menus' and tabs' part of the row, with the tabs `gap` cells apart:
/// its runs, the menus' places, the tabs' places, and the column after
/// the last tab's name.
fn menus_and_tabs(row: Rect, top: &TopRow<'_>, gap: u16) -> (Vec<Run>, [Rect; 2], Vec<Rect>, u16) {
    let y = row.y;
    let mut runs = Vec::new();
    let mut x = row.x;
    let mut menus = [Rect::default(); 2];
    for (place, menu) in menus.iter_mut().zip([MenuName::File, MenuName::Actions]) {
        let text = format!(" {} ▾ ", menu.name());
        let w = width(&text);
        let open = top.open_menu == Some(menu);
        let base = if open {
            Role::MenuHighlight
        } else {
            Role::MenuBar
        };
        runs.extend(lettered(x, y, &text, menu.letter(), base));
        *place = Rect::new(x, y, w, 1);
        x = x.saturating_add(w);
    }
    runs.push(Run::new(x, y, "│", &[Role::MenuBar]));
    x = x.saturating_add(1);
    // Each tab's name starts a gap after the last, with one cell of the
    // gap as its leading pad: the open tab's reverse video covers that
    // pad, the name and one cell after it.
    let mut names: Vec<(u16, &str, char)> = Vec::new();
    let mut at = x.saturating_add(1);
    for tab in top.tabs {
        names.push((at, tab.name, tab.letter));
        at = at.saturating_add(width(tab.name)).saturating_add(gap);
    }
    let end = names
        .last()
        .map_or(x, |(at, name, _)| at.saturating_add(width(name)));
    let mut tabs = Vec::new();
    for (i, (at, name, letter)) in names.iter().enumerate() {
        let open = i == top.open_tab;
        let lead = at.saturating_sub(1);
        if open {
            let text = format!(" {name} ");
            runs.extend(lettered(lead, y, &text, *letter, Role::TabOpen));
        } else {
            runs.extend(lettered(*at, y, name, *letter, Role::Tab));
        }
        // The places tile the row: from a tab's leading pad to the next
        // tab's, and the last one cell past its name.
        let next = names
            .get(i.saturating_add(1))
            .map_or(at.saturating_add(width(name)).saturating_add(1), |n| {
                n.0.saturating_sub(1)
            });
        tabs.push(Rect::new(lead, y, next.saturating_sub(lead), 1).intersection(row));
    }
    let menus = menus.map(|r| r.intersection(row));
    (runs, menus, tabs, end)
}

/// The top row in `row`.
pub(crate) fn layout(row: Rect, top: &TopRow<'_>) -> TopLayout {
    let right = row.right();
    let f = &top.facts;
    // The forms of the row, widest first: the facts in full, without the
    // name, without the words "not saved", with the tabs closed up, and
    // with ● alone for the changes not saved.
    let forms = [
        (true, true, true, 2),
        (false, true, true, 2),
        (false, true, false, 2),
        (false, true, false, 1),
        (false, false, false, 1),
    ];
    let mut chosen = None;
    for (name, number, words, gap) in forms {
        let (runs, menus, tabs, end) = menus_and_tabs(row, top, gap);
        let pieces = facts(f, name, number, words);
        let facts_w = if pieces.is_empty() {
            0
        } else {
            pieces_width(&pieces).saturating_add(1)
        };
        if end.saturating_add(facts_w) <= right {
            chosen = Some((runs, menus, tabs, end, pieces));
            break;
        }
    }
    let (mut runs, menus, tabs, end, mut pieces) = match chosen {
        Some(c) => c,
        None => {
            let (runs, menus, tabs, end) = menus_and_tabs(row, top, 1);
            (runs, menus, tabs, end, facts(f, false, false, false))
        }
    };
    // Where even the shortest form does not fit, the counts go from the
    // end, and the mode, which says the run is live, stays last.
    let room = right.saturating_sub(end.saturating_add(1));
    while pieces_width(&pieces) > room && pieces.len() > 1 {
        pieces.pop();
        while pieces
            .last()
            .is_some_and(|p| p.count.is_none() && p.role == Role::Facts)
        {
            pieces.pop();
        }
    }
    if let [only] = pieces.as_mut_slice() {
        only.text = cut(&only.text, room);
    }
    let mut x = right.saturating_sub(pieces_width(&pieces));
    let (mut not_saved, mut problems) = (None, None);
    for p in pieces {
        let run = Run::new(x, row.y, p.text, &[p.role]);
        match p.count {
            Some(Which::NotSaved) => not_saved = Some(run.rect()),
            Some(Which::Problems) => problems = Some(run.rect()),
            None => {}
        }
        x = x.saturating_add(run.width());
        runs.push(run);
    }
    TopLayout {
        runs,
        menus,
        tabs,
        not_saved,
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_letter_is_found_in_either_case_and_only_once() {
        let runs = lettered(3, 0, "Effects", 'e', Role::Tab);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs.first().map(|r| (r.x, r.text.as_str())), Some((3, "E")));
        assert_eq!(
            runs.get(1).map(|r| (r.x, r.text.as_str())),
            Some((4, "ffects"))
        );
        let runs = lettered(0, 0, "Fonts", 'n', Role::Tab);
        assert_eq!(runs.get(1).map(|r| (r.x, r.text.as_str())), Some((2, "n")));
        assert_eq!(lettered(0, 0, "Help", 'z', Role::Tab).len(), 1);
    }

    #[test]
    fn one_problem_is_a_problem() {
        let f = Facts {
            name: "padnoir",
            mode: None,
            not_saved: Count::default(),
            problems: Count::opening(1),
        };
        let text: String = facts(&f, true, true, true)
            .into_iter()
            .map(|p| p.text)
            .collect();
        assert_eq!(text, "padnoir · 1 problem");
    }
}
