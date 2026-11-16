//! The checks after every key: the invariants that ADR-1403 D7, ADR-1201
//! D3 and D4 set, each row of ADR-1403's key table, and ADR-1201's views
//! table. Each check sees one transition: the TUI before the key, the key,
//! the TUI after it and what the key caused.

use std::collections::{BTreeMap, BTreeSet};

use vtium_config::tui::model::{Then, View};

use super::explore::Step;
use super::sim::{Sim, Term};
use super::state::{StateKind, view_name};
use super::tables::{self, KeyRow, ViewRow};

/// One transition and the TUI on each side of it.
pub struct Seen<'a> {
    pub step: &'a Step,
    pub before: &'a Sim,
    pub after: &'a Sim,
}

impl Seen<'_> {
    fn has(&self, effect: &str) -> bool {
        self.step.did.effects.iter().any(|e| e == effect)
    }

    fn quiet(&self) -> bool {
        self.step.did.effects.is_empty()
    }

    fn idle(&self) -> bool {
        self.step.from == self.step.to && self.quiet()
    }

    fn exited(&self) -> bool {
        self.after.exited.is_some()
    }

    fn draft(sim: &Sim) -> Option<&str> {
        sim.app.draft.as_ref().map(|d| d.text.as_str())
    }

    /// Nothing the person made changed: the draft, the changes that wait
    /// for Save and the font sets that wait for an install.
    fn kept(&self) -> bool {
        Self::draft(self.before) == Self::draft(self.after)
            && self.before.app.unsaved == self.after.app.unsaved
            && self.before.app.pending == self.after.app.pending
    }

    /// The changes that waited for Save are in the draft the desk keeps.
    fn saved(&self) -> bool {
        self.before.app.unsaved == 0
            || (self.has("save") && self.after.desk.draft == Self::draft(self.before).unwrap_or(""))
    }

    /// The key quits, or asks the question that fits first: the quit
    /// question while changes wait for Save, the leave question while font
    /// sets wait for an install.
    fn quits_or_asks(&self) -> Result<(), String> {
        let app = &self.before.app;
        let want = if !app.pending.is_empty() {
            "question:leave"
        } else if app.unsaved > 0 {
            "question:quit"
        } else {
            return self.exits(0);
        };
        if self.step.to.table_state() == want {
            Ok(())
        } else {
            Err(format!("expected {want}"))
        }
    }

    fn exits(&self, status: u8) -> Result<(), String> {
        match self.step.did.exit {
            Some(s) if s == status => Ok(()),
            other => Err(format!("expected exit {status}, got {other:?}")),
        }
    }

    /// The key closes the question and changes nothing.
    fn answers_no(&self) -> Result<(), String> {
        if self.step.to.kind == StateKind::Question || self.exited() {
            return Err("the question is still open, or the TUI left".into());
        }
        if !self.kept() || !self.quiet() {
            return Err("answering no changed something".into());
        }
        Ok(())
    }

    fn nothing(&self) -> Result<(), String> {
        if self.idle() && self.kept() {
            Ok(())
        } else {
            Err("expected the key to do nothing".into())
        }
    }

    /// Ctrl-C's meaning everywhere: the changes are saved, the console's
    /// start font comes back, and the TUI leaves with 130.
    fn saves_and_quits(&self) -> Result<(), String> {
        self.exits(130)?;
        if !self.saved() {
            return Err("left without saving the changes that were not saved".into());
        }
        Ok(())
    }

    /// The live trial ends, its font is put back, and nothing else
    /// changes.
    fn puts_back(&self) -> Result<(), String> {
        let Some(trial) = &self.before.app.trial else {
            return Err("no trial".into());
        };
        if self.after.app.trial.is_some() || self.exited() || !self.kept() {
            return Err("the trial goes on, or something else changed".into());
        }
        if self.before.term == Term::Vtium && self.after.console_font != trial.back {
            return Err(format!("the console draws {}", self.after.console_font));
        }
        Ok(())
    }

    fn opens(&self, state: &str) -> Result<(), String> {
        if self.step.to.table_state() == state {
            Ok(())
        } else {
            Err(format!("expected {state}"))
        }
    }

    fn typed(sim: &Sim) -> Option<String> {
        let app = &sim.app;
        app.flow_view
            .typing
            .clone()
            .or_else(|| app.editing.clone())
            .or_else(|| match app.view {
                View::New => app.new_frit.fields.get(app.new_frit.at).cloned(),
                _ => None,
            })
    }
}

/// What the row's `means` column says, as a check; `None` for a row whose
/// check is not built (none is, today).
type Check = fn(&Seen<'_>) -> Result<(), String>;

/// The checks of the key table's rows, by id: what the row means, and,
/// for a planned or weighed row, what vtium-config does today, which the
/// check expects in reverse until the work lands. A weighed row with no
/// check of today records nothing but that the explorer reached it.
fn row_checks() -> BTreeMap<&'static str, (Check, Option<Check>)> {
    let no_today: Option<Check> = None;
    let mut m: BTreeMap<&'static str, (Check, Option<Check>)> = BTreeMap::new();
    let quit: Check = |s| s.quits_or_asks();
    let no: Check = |s| s.answers_no();
    let nothing: Check = |s| s.nothing();
    let ctrl_c: Check = |s| s.saves_and_quits();
    let back: Check = |s| s.puts_back();
    // today, Ctrl-C leaves at once, without saving
    let leaves: Check = |s| {
        s.exits(130)?;
        if s.has("save") {
            Err("Ctrl-C saved".into())
        } else {
            Ok(())
        }
    };
    m.insert("q.view", (quit, no_today));
    m.insert("q.layer", (quit, no_today));
    m.insert("q.layer.file-view", (quit, Some(|s| s.opens("view:flow"))));
    m.insert("q.layer.restore", (quit, Some(|s| s.opens("view:flow"))));
    m.insert(
        "q.typing",
        (
            |s| match (Seen::typed(s.before), Seen::typed(s.after)) {
                (Some(a), Some(b)) if b == format!("{a}q") => Ok(()),
                (a, b) => Err(format!("typed {a:?} became {b:?}")),
            },
            no_today,
        ),
    );
    m.insert("q.question", (no, no_today));
    m.insert("q.trial", (back, no_today));
    m.insert("q.too-small", (quit, Some(|s| s.exits(0))));
    m.insert("esc.view", (nothing, no_today));
    m.insert(
        "esc.view.flow",
        (
            |s| {
                if s.after.app.flow_view.new == Default::default() && !s.exited() {
                    Ok(())
                } else {
                    Err("NEW PATH is not empty".into())
                }
            },
            no_today,
        ),
    );
    let closes: Check = |s| {
        if s.step.to.kind == StateKind::View && s.after.app.view.tab() == s.before.app.view.tab() {
            Ok(())
        } else {
            Err("expected the view under it".into())
        }
    };
    m.insert("esc.layer", (closes, no_today));
    m.insert(
        "esc.typing",
        (
            |s| {
                if s.step.to.kind != StateKind::Typing
                    && Seen::draft(s.before) == Seen::draft(s.after)
                {
                    Ok(())
                } else {
                    Err("the value is still typed, or the draft changed".into())
                }
            },
            no_today,
        ),
    );
    m.insert("esc.question", (no, no_today));
    m.insert("esc.trial", (back, no_today));
    m.insert("esc.too-small", (nothing, no_today));
    m.insert("tab.view", (|s| step_view(s, true), no_today));
    m.insert("shift-tab.view", (|s| step_view(s, false), no_today));
    for id in [
        "tab.layer",
        "tab.typing",
        "tab.too-small",
        "shift-tab.layer",
        "shift-tab.typing",
        "shift-tab.too-small",
    ] {
        m.insert(id, (nothing, no_today));
    }
    for id in ["tab.question", "shift-tab.question"] {
        m.insert(id, (no, no_today));
    }
    for id in ["tab.trial", "shift-tab.trial"] {
        m.insert(id, (back, no_today));
    }
    m.insert("ctrl-c.any", (ctrl_c, no_today));
    m.insert("ctrl-c.question", (ctrl_c, Some(|s| s.answers_no())));
    m.insert("ctrl-c.trial", (ctrl_c, Some(leaves)));
    m.insert("ctrl-c.too-small", (ctrl_c, Some(leaves)));
    m.insert(
        "y.question",
        (
            |s| {
                if s.step.to.kind != StateKind::Question && (!s.quiet() || !s.kept()) {
                    Ok(())
                } else {
                    Err("yes did nothing".into())
                }
            },
            no_today,
        ),
    );
    let save_quit: Check = |s| {
        s.exits(0)?;
        if s.saved() {
            Ok(())
        } else {
            Err("left without saving".into())
        }
    };
    m.insert("y.question.quit", (save_quit, no_today));
    m.insert("y.question.leave", (save_quit, Some(|s| s.answers_no())));
    m.insert(
        "n.question.quit",
        (
            |_| Ok(()),
            Some(|s| {
                s.exits(0)?;
                if s.has("save") {
                    Err("n saved".into())
                } else {
                    Ok(())
                }
            }),
        ),
    );
    m.insert(
        "i.question.leave",
        (
            no,
            Some(|s| {
                if s.has("prepare") {
                    Ok(())
                } else {
                    Err("expected an install".into())
                }
            }),
        ),
    );
    m.insert(
        "l.question.leave",
        (
            no,
            Some(|s| {
                s.exits(0)?;
                if s.has("save") {
                    Err("l saved".into())
                } else {
                    Ok(())
                }
            }),
        ),
    );
    m.insert("other.question", (no, no_today));
    m.insert("s.view", (saves, no_today));
    m.insert(
        "s.view.fonts",
        (
            saves,
            Some(|s| {
                // outside a vtium console, s switches the preview and saves
                // nothing
                if s.before.term != Term::Vtium
                    && (s.has("save") || s.after.app.unsaved != s.before.app.unsaved)
                {
                    Err("s saved outside a vtium console".into())
                } else {
                    Ok(())
                }
            }),
        ),
    );
    m.insert("s.view.help", (nothing, no_today));
    m.insert(
        "s.layer.file-view",
        (
            |s| {
                saves(s)?;
                if s.after.app.pager.is_some() {
                    Ok(())
                } else {
                    Err("the file view closed".into())
                }
            },
            no_today,
        ),
    );
    m.insert("i.view", (installs, no_today));
    m.insert("i.view.help", (nothing, Some(installs)));
    m.insert(
        "i.layer.file-view",
        (
            |s| {
                installs(s)?;
                if s.after.app.pager.is_some() {
                    Ok(())
                } else {
                    Err("the file view closed".into())
                }
            },
            no_today,
        ),
    );
    m.insert("action.layer", (|_| Ok(()), None));
    m.insert("r.view.flow", (|s| s.opens("question:revert"), no_today));
    m.insert("n.view.flow", (|s| s.opens("question:new"), no_today));
    m.insert("v.view.flow", (|s| s.opens("layer:file-view"), no_today));
    m.insert("o.view.flow", (|s| s.opens("layer:restore"), no_today));
    // the Looks view lists sizes today, and its own keys do not act
    for id in ["r.view.looks", "n.view.looks", "t.view.looks"] {
        m.insert(
            id,
            (
                |_| Err("the Looks view's own keys are not built".into()),
                Some(nothing),
            ),
        );
    }
    let ops_on: Check = |s| {
        if s.before.term == Term::OpsOff {
            if s.has("check") {
                Ok(())
            } else {
                Err("expected allow-font-ops turned on".into())
            }
        } else {
            s.nothing()
        }
    };
    m.insert("o.view.looks", (ops_on, Some(ops_on)));
    m.insert("o.view", (nothing, Some(ops_on)));
    m.insert(
        "t.layer.flow-card",
        (
            |s| {
                if s.before.term != Term::Vtium && s.step.did.switches > 0 {
                    return Err("a switch outside a vtium console".into());
                }
                if s.exited() || s.after.app.view != View::Flow {
                    return Err("t left the card".into());
                }
                Ok(())
            },
            no_today,
        ),
    );
    m.insert(
        "k.trial",
        (
            |s| {
                if s.after.app.trial.is_none() && s.step.did.switches == 0 && s.kept() {
                    Ok(())
                } else {
                    Err("k did not keep the font".into())
                }
            },
            no_today,
        ),
    );
    m.insert("other.trial", (back, no_today));
    m.insert(
        "u.too-small",
        (
            |s| {
                if s.exited() || !s.kept() {
                    return Err("u left or changed something".into());
                }
                if s.before.term == Term::Vtium && s.after.console_font != super::sim::START_FONT {
                    return Err("the start font is not back".into());
                }
                Ok(())
            },
            no_today,
        ),
    );
    m.insert("other.too-small", (nothing, no_today));
    m.insert(
        "updown.typing.settings",
        (
            |_| Ok(()),
            Some(|s| {
                if s.after.app.editing.is_none() && s.after.app.message == s.before.app.message {
                    Ok(())
                } else {
                    Err("expected the value dropped silently".into())
                }
            }),
        ),
    );
    m
}

fn step_view(s: &Seen<'_>, forward: bool) -> Result<(), String> {
    let tabs = View::TABS;
    let at = tabs
        .iter()
        .position(|v| *v == s.before.app.view.tab())
        .unwrap_or(0);
    let n = tabs.len();
    let next = if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    };
    if Some(&s.after.app.view) == tabs.get(next) {
        Ok(())
    } else {
        Err(format!("expected {:?}", tabs.get(next)))
    }
}

fn saves(s: &Seen<'_>) -> Result<(), String> {
    if s.before.app.unsaved == 0 {
        return if s.quiet() && s.kept() {
            Ok(())
        } else {
            Err("nothing to save, but something happened".into())
        };
    }
    if s.saved() && s.after.app.unsaved == 0 {
        Ok(())
    } else {
        Err("expected a save".into())
    }
}

fn installs(s: &Seen<'_>) -> Result<(), String> {
    if s.has("prepare") && s.saved() {
        Ok(())
    } else {
        Err("expected a save and an install".into())
    }
}

/// What one run of the checks found.
#[derive(Default)]
pub struct Findings {
    /// Each failure, with the scenario that shows it.
    pub failures: Vec<String>,
    /// The key table's rows some transition reached.
    pub reached: BTreeSet<String>,
    /// What each view wrote, by the views table's names.
    pub writes: BTreeMap<&'static str, BTreeSet<String>>,
    /// The actions each view's tab bar showed.
    pub actions: BTreeMap<&'static str, BTreeSet<String>>,
}

/// The checks, with the tables read once.
pub struct Checks {
    rows: Vec<KeyRow>,
    checks: BTreeMap<&'static str, (Check, Option<Check>)>,
    pub found: Findings,
}

impl Checks {
    pub fn new() -> Checks {
        Checks {
            rows: tables::keys(),
            checks: row_checks(),
            found: Findings::default(),
        }
    }

    fn fail(&mut self, step: &Step, what: impl std::fmt::Display) {
        self.found.failures.push(format!(
            "{}\n    {} -> {}\n    {what}",
            step.scenario(),
            step.from,
            step.to
        ));
    }

    /// Runs every check on one transition.
    pub fn see(&mut self, step: &Step, before: &Sim, after: &Sim) {
        let seen = Seen {
            step,
            before,
            after,
        };
        if let Err(e) = self.invariants(&seen) {
            self.fail(step, e);
        }
        if let Some(row) = tables::row_for(&self.rows, &step.from, step.key).cloned() {
            self.found.reached.insert(row.id.clone());
            let check = self.checks.get(row.id.as_str()).copied();
            let verdict = match (check, row.status.as_str()) {
                (None, _) => Err(format!("the key table's row {} has no check", row.id)),
                (Some((means, _)), "supported") => means(&seen),
                (Some((_, Some(today))), _) => today(&seen).map_err(|e| {
                    format!(
                        "{e}; the row is {}, and this check expects what vtium-config does today",
                        row.status
                    )
                }),
                (Some((_, None)), _) => Ok(()),
            };
            if let Err(e) = verdict {
                self.fail(step, format!("{}: {e}", row.id));
            }
        }
        self.views(&seen);
    }

    /// The invariants after every key.
    fn invariants(&mut self, s: &Seen<'_>) -> Result<(), String> {
        let app = &s.after.app;
        if s.after.exited.is_none() {
            let screen = s.after.screen();
            if screen.len() != app.rows {
                return Err(format!("{} rows on a screen of {}", screen.len(), app.rows));
            }
            if let Some(row) = screen.iter().find(|r| r.chars().count() > app.cols) {
                return Err(format!("a row wider than {}: {row:?}", app.cols));
            }
            // mouse reporting is on exactly while the Flow view is in front
            if s.after.mouse != (app.view == View::Flow) {
                return Err(format!(
                    "mouse reporting is {} in {:?}",
                    s.after.mouse, app.view
                ));
            }
            // a question that throws the draft away counts the changes it
            // also throws away
            if let Some(c) = &app.confirm
                && matches!(c.then, Then::Desk(_))
                && app.unsaved > 0
                && !c.question.contains("not saved")
            {
                return Err(format!(
                    "the question does not count the unsaved changes: {}",
                    c.question
                ));
            }
            // every key the key line lists shows, outside the Flow view,
            // whose key lines its own drawing picks
            if app.view != View::Flow || app.confirm.is_some() {
                let shown = screen.join("\n");
                let line = vtium_config::tui::view::key_line(app);
                if s.step.to.kind != StateKind::TooSmall
                    && let Some(key) = line.split("  ").find(|k| !shown.contains(k))
                {
                    return Err(format!("the key line leaves out {key:?}"));
                }
            }
        } else {
            if s.after.mouse {
                return Err("left with mouse reporting on".into());
            }
            if let Some(start) = &app.start_font
                && s.after.term == Term::Vtium
                && s.after.console_font != *start
            {
                return Err(format!(
                    "left with the console drawing {}, not {start}",
                    s.after.console_font
                ));
            }
        }
        if matches!(s.step.key, "Tab" | "Shift-Tab") {
            let moved = s.before.app.view.tab() != app.view.tab();
            if moved != (s.step.from.kind == StateKind::View) {
                return Err(format!("Tab changed the view: {moved}"));
            }
        }
        Ok(())
    }

    /// Records what the key wrote and what the tab bar shows, for the
    /// views table's check.
    fn views(&mut self, s: &Seen<'_>) {
        let view = view_name(s.before.app.view);
        if s.after.exited.is_none() && matches!(s.step.to.kind, StateKind::View) {
            let top = s.after.screen().first().cloned().unwrap_or_default();
            let bar = top
                .split_once(" Help ")
                .map_or(String::new(), |(_, r)| r.to_owned());
            let shown: BTreeSet<String> = ["View file", "Save", "Install", "Revert", "New"]
                .iter()
                .filter(|a| bar.contains(*a))
                .map(|a| (*a).to_owned())
                .collect();
            self.found
                .actions
                .entry(view_name(s.after.app.view))
                .or_default()
                .extend(shown);
        }
        // the TUI's own edits: a change checked, or the font a vtium
        // console shows saved; Revert, Restore and New replace the draft
        if s.has("check") || s.has("save checked") {
            let before = Seen::draft(s.before).unwrap_or("");
            let after = Seen::draft(s.after).unwrap_or("");
            let parts = written(before, after);
            self.found.writes.entry(view).or_default().extend(parts);
        }
        if s.before.app.pending != s.after.app.pending && !s.has("install") {
            self.found
                .writes
                .entry(view)
                .or_default()
                .insert("frits".into());
        }
    }

    /// The checks after the walk: every row of the key table reached and
    /// checked, and the views table met.
    pub fn finish(&mut self, views: &[ViewRow]) {
        for row in &self.rows {
            if !row.key_line && !self.found.reached.contains(&row.id) {
                self.found.failures.push(format!(
                    "the key table's row {} matched no transition",
                    row.id
                ));
            }
        }
        for (i, tab) in View::TABS.iter().enumerate() {
            let name = view_name(*tab);
            match views.iter().find(|v| v.id == name) {
                Some(v) if v.order == Some(i + 1) => {}
                Some(v) => self.found.failures.push(format!(
                    "the views table puts {name} at {:?}, the TUI at {}",
                    v.order,
                    i + 1
                )),
                None => self
                    .found
                    .failures
                    .push(format!("the views table has no row for the tab {name}")),
            }
        }
        for v in views {
            if let Some(order) = v.order
                && View::TABS.get(order - 1).map(|t| view_name(*t)) != Some(v.id.as_str())
            {
                self.found.failures.push(format!(
                    "the views table's {} is tab {order}, which the TUI does not have",
                    v.id
                ));
            }
            let under = ["flow", "looks", "fonts", "settings", "help", "new-frit"]
                .iter()
                .zip([
                    View::Flow,
                    View::Looks,
                    View::Fonts,
                    View::Settings,
                    View::Help,
                    View::New,
                ])
                .find(|(n, _)| **n == v.id)
                .map(|(_, view)| view_name(view.tab()));
            if let Some(under) = under
                && v.under != "-"
                && v.under != under
            {
                self.found.failures.push(format!(
                    "the views table shows {} under {}, the TUI under {under}",
                    v.id, v.under
                ));
            }
            if let Some(shown) = self.found.actions.get(v.id.as_str()) {
                let want: BTreeSet<String> = v.actions.iter().cloned().collect();
                if *shown != want {
                    self.found.failures.push(format!(
                        "the {} tab bar shows {shown:?}, the views table {want:?}",
                        v.id
                    ));
                }
            }
            if let Some(wrote) = self.found.writes.get(v.id.as_str()) {
                for part in wrote {
                    if !v.writes.contains(part) {
                        self.found.failures.push(format!(
                            "{} writes {part}, which the views table does not list",
                            v.id
                        ));
                    }
                }
            }
        }
    }
}

/// The parts of vtium.conf that differ between `before` and `after`, by
/// the views table's names: `top`, `look`, `head`, `rules` and `labels`.
/// The `version = 1` line that the first change to a draft with no
/// sections writes is left out, as the views table's note says.
fn written(before: &str, after: &str) -> BTreeSet<String> {
    fn lines(text: &str) -> BTreeMap<(String, String), usize> {
        let mut out = BTreeMap::new();
        let mut part = "top".to_owned();
        for line in text.lines() {
            let t = line.trim();
            if let Some(head) = t.strip_prefix('[') {
                part = head
                    .trim_end_matches(']')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_owned();
            }
            if t.is_empty() || t.starts_with('#') || t.replace(' ', "") == "version=1" {
                continue;
            }
            *out.entry((part.clone(), t.to_owned())).or_insert(0) += 1;
        }
        out
    }
    let (a, b) = (lines(before), lines(after));
    a.keys()
        .chain(b.keys())
        .filter(|k| a.get(*k) != b.get(*k))
        .map(|(part, _)| part.clone())
        .collect()
}
