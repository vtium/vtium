//! Save's review: what saving changes, read from the open document when
//! the review opens. It lists the lines of vtium.conf that differ from the
//! installed file as hunks with a few lines of context, or the whole file
//! after `[w]`; then the font sets and active fonts the save changes, or
//! "fonts unchanged"; then the problems the check reports, those that stop
//! Save first; then where the change takes effect. While a problem stops
//! Save, the action bar has no Save and the review says why.
//!
//! References:
//! - ADR-1403 D8, "Save: the review, then sudo once": the review's parts
//!   in their order, and Save greyed out while the check refuses the
//!   document.
//! - ADR-1403 D11, "The frame": the problems that stop Save first, then
//!   the warnings, as the Problems list orders them.

use vtium_config_core::batch;
use vtium_config_core::drafts::{FritEdit, says_no_route};
use vtium_config_core::ui::Shared;
use vtium_config_core::versions::diff;
use vtium_config_widgets::roles::Role;

use super::words;
use crate::sheets::problems::{Problem, gather};

/// The review, as it opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    /// The installed vtium.conf's text, empty where none is installed.
    installed: String,
    /// The document's vtium.conf.
    text: String,
    /// The count not saved.
    count: usize,
    /// The font sets the save changes, each with its line.
    font_sets: Vec<String>,
    /// The fonts the save activates or deactivates, each with its line.
    active: Vec<String>,
    problems: Vec<Problem>,
    /// A line on what Save did before the review, such as taking in the
    /// installed file's change.
    note: Option<String>,
    /// Whether the whole file shows, rather than its changes.
    whole: bool,
}

impl Review {
    /// The review of the open document in `shared`, with `note` under its
    /// title.
    pub fn of(shared: &Shared, note: Option<String>) -> Review {
        let document = shared.document();
        let review = document.review();
        let font_sets = review
            .font_sets
            .iter()
            .map(|(name, edit)| match edit {
                FritEdit::Add => format!("        +{name}  (added)"),
                FritEdit::Replace => format!("        ~{name}  (replaced)"),
                FritEdit::Remove => format!("        -{name}  (removed)"),
            })
            .collect();
        let active = review
            .active
            .iter()
            .map(|(path, change)| match change {
                batch::Change::Activate => format!("        +{}", path.display()),
                batch::Change::Deactivate => format!("        -{}", path.display()),
            })
            .collect();
        let mut problems = gather(shared);
        problems.sort_by_key(|p| !p.stops);
        Review {
            installed: document.installed().text.clone().unwrap_or_default(),
            text: document.text().to_owned(),
            count: document.count(),
            font_sets,
            active,
            problems,
            note,
            whole: false,
        }
    }

    /// The review with `problems` in place of those the shared state
    /// holds, put in the order the review lists them.
    pub fn with_problems(mut self, mut problems: Vec<Problem>) -> Review {
        problems.sort_by_key(|p| !p.stops);
        self.problems = problems;
        self
    }

    /// Whether a problem the check reports stops Save.
    pub fn stops(&self) -> bool {
        self.problems.iter().any(|p| p.stops)
    }

    /// Whether every problem that stops Save is a missing complete route,
    /// which Save asks about rather than refusing.
    ///
    /// References:
    /// - ADR-1403 D8: "When the only problem is a missing complete route
    ///   (D3), the second Ctrl+S asks the question of D7 before it saves".
    pub fn no_route_only(&self) -> bool {
        let mut stopping = self.problems.iter().filter(|p| p.stops).peekable();
        stopping.peek().is_some() && stopping.all(|p| says_no_route(&p.line))
    }

    /// `[w]`: the whole file, or only its changes again.
    pub fn toggle_whole(&mut self) {
        self.whole = !self.whole;
    }

    /// Whether the whole file shows.
    pub fn whole(&self) -> bool {
        self.whole
    }

    /// The review's lines, each with its role.
    pub fn lines(&self) -> Vec<(String, Role)> {
        let fonts = match (self.font_sets.len(), self.active.len()) {
            (0, 0) => "fonts unchanged".to_owned(),
            (0, a) => active_count(a),
            (f, 0) => vtium_config_core::drafts::frit_count(f),
            (f, a) => format!(
                "{}, {}",
                vtium_config_core::drafts::frit_count(f),
                active_count(a)
            ),
        };
        let mut out = vec![(
            format!(" {} · {fonts}", words::changes(self.count)),
            Role::Heading,
        )];
        if let Some(note) = &self.note {
            out.push((format!(" {note}"), Role::Warning));
        }
        out.push((String::new(), Role::Row));
        if self.whole {
            out.push((" The whole file:".to_owned(), Role::Heading));
            for line in diff::whole_file(&self.installed, &self.text) {
                out.push(super::diff_line(&line));
            }
        } else {
            let d = diff::diff(&self.installed, &self.text);
            if d.is_empty() {
                out.push((" vtium.conf is unchanged.".to_owned(), Role::Dim));
            }
            super::hunks(&d, &mut out);
        }
        if !self.font_sets.is_empty() {
            out.push((" Font sets it changes".to_owned(), Role::Heading));
            out.extend(self.font_sets.iter().map(|l| (l.clone(), Role::Row)));
        }
        if !self.active.is_empty() {
            out.push((
                " Fonts it activates or deactivates".to_owned(),
                Role::Heading,
            ));
            out.extend(self.active.iter().map(|l| (l.clone(), Role::Row)));
        }
        out.push((String::new(), Role::Row));
        self.problem_lines(&mut out);
        out.push((String::new(), Role::Row));
        out.push((format!(" {}", words::TAKES_EFFECT), Role::Row));
        out
    }

    /// The problems: those that stop Save, then the warnings, each with
    /// its place.
    fn problem_lines(&self, out: &mut Vec<(String, Role)>) {
        let stop: Vec<&Problem> = self.problems.iter().filter(|p| p.stops).collect();
        let warn: Vec<&Problem> = self.problems.iter().filter(|p| !p.stops).collect();
        if stop.is_empty() && warn.is_empty() {
            out.push((" The check reports no problems.".to_owned(), Role::Dim));
            return;
        }
        let place_w = self
            .problems
            .iter()
            .map(|p| p.place.chars().count())
            .max()
            .unwrap_or(0)
            .min(PLACE_COLUMNS);
        let line = |p: &Problem| {
            let place = crate::sheets::cut(&p.place, PLACE_COLUMNS);
            format!("   {place:<place_w$}  {}", p.line)
        };
        if !stop.is_empty() {
            let head = if self.no_route_only() {
                "Save asks first: the configuration has no complete path.".to_owned()
            } else {
                match stop.len() {
                    1 => "1 problem stops Save: vtium check-config refuses the file until it is fixed.".to_owned(),
                    n => format!("{n} problems stop Save: vtium check-config refuses the file until they are fixed."),
                }
            };
            out.push((format!(" {head}"), Role::Problems));
            out.extend(stop.iter().map(|p| (line(p), Role::Problems)));
        }
        if !warn.is_empty() {
            let head = match warn.len() {
                1 => "1 warning (saving still works):".to_owned(),
                n => format!("{n} warnings (saving still works):"),
            };
            out.push((format!(" {head}"), Role::Warning));
            out.extend(warn.iter().map(|p| (line(p), Role::Row)));
        }
        out.push((
            " [Alt+P] lists the problems, and [Enter] there goes to each one.".to_owned(),
            Role::Dim,
        ));
    }
}

/// The widest the place column grows before it is cut.
const PLACE_COLUMNS: usize = 28;

/// `n` changes to the active fonts.
fn active_count(n: usize) -> String {
    match n {
        1 => "1 change to the active fonts".to_owned(),
        n => format!("{n} changes to the active fonts"),
    }
}
