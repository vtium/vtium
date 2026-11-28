//! The Problems list, which Alt+P or a click on the top row's problems
//! count opens: each problem in one line with where it is, as its tab and
//! item or its line of vtium.conf. The problems that stop Save come
//! first, then the warnings, and each group in the tabs' order: Fonts,
//! Effects, Looks, Flow, Settings, then the lines of vtium.conf that no
//! tab owns. Enter, or a click on a problem, goes to its place. On the
//! line of fonts the configuration uses that are not active, Enter asks
//! whether to activate them, through the Fonts tab's Activate missing. A
//! line too long for the list is cut, and F1 opens all of it.
//!
//! The list reads two things the shared state holds: what the check found
//! in the open document as it stands, and the fonts the document uses. A
//! font that fails the trust rule gets a warning line of its own with the
//! reason behind F1. A font that is missing gets none here, since the check
//! names each missing font itself. Until the check has judged the document
//! as it stands, the list says so in place of "No problems".
//!
//! References:
//! - ADR-1403 D11, "The frame": the list, its order, and Enter going to
//!   the place.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   the list's line for those fonts offers Activate missing in a
//!   question.
//! - ADR-1403 D11, "Fonts that cannot be activated": a font that fails
//!   the trust rule gets a warning line with its reason, and Enter goes to
//!   the Fonts tab.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::fontuse::State;
use vtium_config_core::fontuse::activate::words as activate;
use vtium_config_core::ui::shared::Finding;
use vtium_config_core::ui::{Action, Shared};
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::roles::Role;

use super::{Act, bar_row, put, window};

/// The sheet's title.
pub const TITLE: &str = "Problems";

/// The row Enter on a problem hands the tab it goes to, carrying the
/// problem's item where it names one.
pub const GO: &str = "enter.layer.problems";

/// The tabs' nodes in the order the list sorts each group by.
const TAB_ORDER: &[&str] = &["fonts", "effects", "looks", "flow", "settings", "help"];

/// One problem the check finds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Whether the problem stops Save, rather than only warning.
    pub stops: bool,
    /// Where it is, as the list's first column shows it, such as
    /// `Looks › My-Look › Scripts` or `vtium.conf line 32`.
    pub place: String,
    /// The problem, in one line.
    pub line: String,
    /// The rest of what the check says, which F1 opens, where there is
    /// more than the line.
    pub more: Option<String>,
    /// Where Enter goes.
    pub goes: Goes,
}

/// Where Enter on a problem goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goes {
    /// To the tab of this node, and to its item there by the tab's own
    /// index where the problem names one.
    Item {
        tab: &'static str,
        item: Option<usize>,
    },
    /// To this line of vtium.conf, in Show changes' whole file.
    Line(usize),
    /// The fonts the configuration uses that are not active: asks
    /// whether to activate these many.
    Activate(usize),
    /// Nowhere vtium-config edits, such as a line of a file the
    /// configuration does not hold: Enter does nothing, and F1 opens the
    /// whole text.
    Nowhere,
}

impl Problem {
    /// The tab the problem sorts under, after every tab where it has none.
    fn order(&self) -> usize {
        match &self.goes {
            Goes::Item { tab, .. } => TAB_ORDER
                .iter()
                .position(|t| t == tab)
                .unwrap_or(TAB_ORDER.len()),
            Goes::Activate(_) => 0,
            Goes::Line(_) | Goes::Nowhere => TAB_ORDER.len(),
        }
    }
}

/// The line of `n` fonts the configuration uses that are not active, whose
/// Enter offers Activate missing.
pub fn inactive_fonts(n: usize) -> Problem {
    Problem {
        stops: false,
        place: "Fonts".to_owned(),
        line: activate::problem(n),
        more: None,
        goes: Goes::Activate(n),
    }
}

/// A font that fails the trust rule, by its name, with the check it
/// failed; `own` for a font in the person's own folders.
fn untrusted(name: &str, check: &str, own: bool) -> Problem {
    Problem {
        stops: false,
        place: "Fonts".to_owned(),
        line: if own {
            words::own_font(name)
        } else {
            words::fails_trust(name)
        },
        more: Some(check.to_owned()),
        goes: Goes::Item {
            tab: "fonts",
            item: None,
        },
    }
}

/// The check's `finding` as the list shows it: in the draft's vtium.conf
/// by its line, in Flow where no complete route is wired, and in a font
/// set's own file by that file's name.
fn checked(finding: Finding) -> Problem {
    let name = |path: &str| {
        std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
    };
    let (place, goes) = match (&finding.file, finding.line) {
        (None, _) if finding.route => (
            "Flow".to_owned(),
            Goes::Item {
                tab: "flow",
                item: None,
            },
        ),
        (None, Some(n)) => (words::line(n), Goes::Line(n)),
        (None, None) => ("vtium.conf".to_owned(), Goes::Line(1)),
        (Some(file), line) => {
            let place = match line {
                Some(n) => format!("{} line {n}", name(file)),
                None => name(file),
            };
            let goes = if file.ends_with(".vtfrit") {
                Goes::Item {
                    tab: "looks",
                    item: None,
                }
            } else {
                Goes::Nowhere
            };
            (place, goes)
        }
    };
    Problem {
        stops: finding.stops,
        place,
        line: finding.text,
        more: finding.more,
        goes,
    }
}

/// The problems of the open document that the shared state holds: what
/// the check found in the document as it stands, the line of fonts the
/// document uses that are not active, and each font that fails the trust
/// rule.
pub fn gather(shared: &Shared) -> Vec<Problem> {
    let mut out: Vec<Problem> = shared
        .findings()
        .unwrap_or_default()
        .into_iter()
        .map(checked)
        .collect();
    let used = shared.fonts_used();
    let n = used.to_activate().len();
    if n > 0 {
        out.push(inactive_fonts(n));
    }
    for font in &used.fonts {
        if let State::FailsTrust { check, own } = &font.state {
            out.push(untrusted(&font.name(), check, *own));
        }
    }
    out
}

/// The list's state: the problems in their order, and the highlighted
/// one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problems {
    problems: Vec<Problem>,
    at: Option<usize>,
    /// Whether the check has judged the document as it stands.
    checked: bool,
}

impl Problems {
    /// The list of `problems`, put in the list's order: those that stop
    /// Save first, then the warnings, each group in the tabs' order, and
    /// the check's own order within a tab.
    pub fn new(mut problems: Vec<Problem>) -> Problems {
        problems.sort_by_key(|p| (!p.stops, p.order()));
        let at = (!problems.is_empty()).then_some(0);
        Problems {
            problems,
            at,
            checked: true,
        }
    }

    /// The list of the problems `shared` holds for the open document,
    /// which says so where the check has not judged the document as it
    /// stands.
    pub fn of(shared: &Shared) -> Problems {
        Problems {
            checked: shared.findings().is_some(),
            ..Problems::new(gather(shared))
        }
    }

    /// The problems in the list's order.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// The highlighted problem.
    pub fn highlighted(&self) -> Option<&Problem> {
        self.problems.get(self.at?)
    }

    /// Acts on `key`, which chose the row `id` of `any.problems`.
    pub fn key(&mut self, key: &Key, id: &str) -> Act {
        match id {
            "esc.layer.problems" => Act::Close,
            "move.layer.problems" => {
                let last = isize::try_from(self.problems.len()).unwrap_or(isize::MAX);
                self.step(match key.code {
                    Code::Up => -1,
                    Code::Down => 1,
                    Code::PageUp => -PAGE,
                    Code::PageDown => PAGE,
                    Code::Home => -last,
                    _ => last,
                });
                Act::Stay
            }
            GO => self.go(),
            "question.layer" => Act::Stay,
            _ => Act::Pass,
        }
    }

    /// Moves the highlight `by` problems, stopping at the first and last.
    pub fn step(&mut self, by: isize) {
        if let Some(last) = self.problems.len().checked_sub(1) {
            self.at = Some(self.at.unwrap_or(0).saturating_add_signed(by).min(last));
        }
    }

    /// A click on the sheet's line `line`, of `rows` lines in view.
    pub fn click(&mut self, line: usize, rows: usize) -> Act {
        let shown = self.shown();
        let top = window(shown.len(), rows, self.line_of_highlight(&shown));
        let Some(i) = shown.get(top.saturating_add(line)).and_then(|l| l.2) else {
            return Act::Stay;
        };
        self.at = Some(i);
        self.go()
    }

    fn go(&self) -> Act {
        match self.highlighted().map(|p| &p.goes) {
            Some(Goes::Item { tab, item }) => Act::GoNode {
                tab,
                action: item.map(|i| Action::item(GO, i)),
            },
            Some(Goes::Line(_)) => Act::Shell(Action::of("v.view.flow")),
            Some(Goes::Activate(n)) => Act::Activate(*n),
            Some(Goes::Nowhere) | None => Act::Stay,
        }
    }

    /// The highlighted problem's whole text, which F1 opens: its place as
    /// the title, then its line and the rest of what the check says.
    pub fn full_text(&self) -> Option<(String, String)> {
        let p = self.highlighted()?;
        let text = match &p.more {
            Some(more) => format!("{}\n\n{more}", p.line),
            None => p.line.clone(),
        };
        Some((p.place.clone(), text))
    }

    /// The summary at the top: how many there are, and whether Save still
    /// works.
    fn summary(&self) -> String {
        let n = self.problems.len();
        let stop = self.problems.iter().filter(|p| p.stops).count();
        match (n, stop) {
            (0, _) if !self.checked => words::NOT_CHECKED.to_owned(),
            (0, _) => words::NONE.to_owned(),
            (1, 0) => format!("1 problem  {}", words::WARNING_ONE),
            (n, 0) => format!("{n} problems  {}", words::WARNINGS),
            (1, _) => format!("1 problem  {}", words::STOPS),
            (n, 1) => format!("{n} problems  1 {}", words::STOPS_ONE),
            (n, s) => format!("{n} problems  {s} {}", words::STOP_SOME),
        }
    }

    /// The sheet's lines: the summary, each group under its heading, and
    /// the order's note last. Each entry is the line's text, its role,
    /// and the problem it shows.
    fn shown(&self) -> Vec<(String, Role, Option<usize>)> {
        let mut out = vec![(self.summary(), Role::Heading, None)];
        if self.problems.is_empty() {
            return out;
        }
        let place_w = self
            .problems
            .iter()
            .map(|p| p.place.chars().count())
            .max()
            .unwrap_or(0)
            .min(PLACE_COLUMNS);
        for (stops, heading) in [(true, words::STOP_HEADING), (false, words::WARN_HEADING)] {
            let group: Vec<usize> = (0..self.problems.len())
                .filter(|&i| self.problems.get(i).is_some_and(|p| p.stops == stops))
                .collect();
            if group.is_empty() {
                out.push((format!(" {heading}   {}", words::NO_GROUP), Role::Dim, None));
                continue;
            }
            out.push((format!(" {heading}"), Role::Heading, None));
            for i in group {
                let Some(p) = self.problems.get(i) else {
                    continue;
                };
                let place = super::cut(&p.place, PLACE_COLUMNS);
                let role = if p.stops {
                    Role::Problems
                } else {
                    Role::Warning
                };
                out.push((format!("   {place:<place_w$}  {}", p.line), role, Some(i)));
            }
        }
        out.push((words::ORDER.to_owned(), Role::Dim, None));
        out
    }

    /// The line of `shown` that holds the highlighted problem.
    fn line_of_highlight(&self, shown: &[(String, Role, Option<usize>)]) -> Option<usize> {
        shown
            .iter()
            .position(|(_, _, p)| p.is_some() && *p == self.at)
    }

    /// How many lines the sheet holds.
    pub fn lines(&self) -> usize {
        self.shown().len()
    }

    /// The sheet's box in `viewer`: across the viewer, at its top.
    pub fn area(&self, viewer: Rect) -> Rect {
        super::top_box(viewer, viewer.width, self.lines())
    }

    /// Draws the list into `area`, the highlighted problem kept in view.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        let shown = self.shown();
        let rows = usize::from(area.height);
        let top = window(shown.len(), rows, self.line_of_highlight(&shown));
        let width = usize::from(area.width);
        for (y, (text, role, problem)) in shown.iter().skip(top).take(rows).enumerate() {
            let y = u16::try_from(y).unwrap_or(u16::MAX);
            let lit = problem.is_some() && *problem == self.at;
            let role = if lit {
                bar_row(buf, area, y, Role::RowHighlight, color);
                Role::RowHighlight
            } else {
                *role
            };
            put(buf, area, 0, y, &super::cut(text, width), role, color);
        }
    }
}

/// The step of PgUp and PgDn in the list.
const PAGE: isize = 10;

/// The widest the place column grows before it is cut.
const PLACE_COLUMNS: usize = 34;

/// The list's words.
mod words {
    pub const NONE: &str = "No problems: the check finds nothing in this configuration.";
    pub const NOT_CHECKED: &str =
        "No problems listed: the check has not run on the configuration yet.";
    pub const WARNING_ONE: &str = "a warning: Save still works";
    pub const WARNINGS: &str = "each a warning: Save still works";
    pub const STOPS: &str = "it stops Save";
    pub const STOPS_ONE: &str = "stops Save";
    pub const STOP_SOME: &str = "stop Save";
    pub const STOP_HEADING: &str = "Stop Save";
    pub const WARN_HEADING: &str = "Warnings";
    pub const NO_GROUP: &str = "none";
    pub const ORDER: &str =
        "Problems that stop Save come first, then warnings, each group in tab order.";

    /// Where a line of vtium.conf is.
    pub fn line(n: usize) -> String {
        format!("vtium.conf line {n}")
    }

    /// A font that fails the trust rule, which Activate missing leaves.
    pub fn fails_trust(name: &str) -> String {
        format!("{name} cannot be activated: it fails the trust rule (F1 says why)")
    }

    /// A font in the person's own folders, which vtium takes only once it
    /// is installed as a system font.
    pub fn own_font(name: &str) -> String {
        format!("{name} is not usable yet: it is in your own fonts folder (F1 says why)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(stops: bool, tab: &'static str, line: &str) -> Problem {
        Problem {
            stops,
            place: tab.to_owned(),
            line: line.to_owned(),
            more: None,
            goes: Goes::Item { tab, item: None },
        }
    }

    #[test]
    fn the_highlight_stops_at_the_first_and_the_last_problem() {
        let mut list = Problems::new(vec![
            problem(false, "fonts", "a"),
            problem(false, "looks", "b"),
        ]);
        list.step(-1);
        assert_eq!(list.highlighted().map(|p| p.line.as_str()), Some("a"));
        list.step(5);
        assert_eq!(list.highlighted().map(|p| p.line.as_str()), Some("b"));
        let mut empty = Problems::new(Vec::new());
        empty.step(1);
        assert_eq!(empty.highlighted(), None);
    }
}
