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
    m.insert("q.question", (nothing, no_today));
    m.insert("q.trial", (back, no_today));
    m.insert("q.too-small", (quit, no_today));
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
        m.insert(id, (nothing, no_today));
    }
    for id in ["tab.trial", "shift-tab.trial"] {
        m.insert(id, (back, no_today));
    }
    m.insert("ctrl-c.any", (ctrl_c, no_today));
    m.insert("ctrl-c.question", (no, no_today));
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
    m.insert(
        "y.question.leave",
        (
            |s| {
                // an install refused or failed keeps the TUI, which says why
                if !s.exited() {
                    return if s.has("install") || s.after.app.message.is_none() {
                        Err("the TUI stays after an install, or says nothing".into())
                    } else {
                        Ok(())
                    };
                }
                s.exits(0)?;
                if s.has("install") && s.saved() {
                    Ok(())
                } else {
                    Err("expected a save and an install before the TUI quits".into())
                }
            },
            no_today,
        ),
    );
    m.insert(
        "n.question.leave",
        (
            |s| {
                s.exits(0)?;
                if s.saved() && !s.has("install") {
                    Ok(())
                } else {
                    Err("expected a save and no install".into())
                }
            },
            no_today,
        ),
    );
    m.insert("n.question", (nothing, no_today));
    m.insert(
        "n.question.quit",
        (
            |s| {
                s.exits(0)?;
                if s.has("save") {
                    Err("n saved".into())
                } else {
                    Ok(())
                }
            },
            no_today,
        ),
    );
    m.insert("i.question.leave", (nothing, no_today));
    m.insert("l.question.leave", (nothing, no_today));
    m.insert("other.question", (nothing, no_today));
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
    m.insert("action.layer", (nothing, None));
    // the shared list: the keys move inside it, and Enter and Esc leave it
    for id in [
        "up.layer.list",
        "down.layer.list",
        "home.layer.list",
        "end.layer.list",
        "pgup.layer.list",
        "pgdn.layer.list",
    ] {
        m.insert(
            id,
            (
                |s| {
                    if s.exited() || s.step.from.node() != s.step.to.node() || !s.kept() {
                        Err("the key left the list or changed something".into())
                    } else {
                        Ok(())
                    }
                },
                None,
            ),
        );
    }
    let leaves_list: Check = |s| {
        if s.exited() || s.step.from.node() == s.step.to.node() {
            Err("the list stays open".into())
        } else {
            Ok(())
        }
    };
    m.insert("enter.layer.list", (leaves_list, None));
    // the mouse: over a question, a layer or a value being typed it does
    // nothing, and at the trial it puts the font back
    for id in ["click.layer", "click.typing", "wheel.typing"] {
        m.insert(id, (nothing, None));
    }
    // under a question the highlight stays too, which the abstract state
    // of a question does not show
    let still: Check = |s| {
        s.nothing()?;
        if s.before.app.flow_view.at == s.after.app.flow_view.at && s.after.app.confirm.is_some() {
            Ok(())
        } else {
            Err("the highlight moved under the question, or it closed".into())
        }
    };
    m.insert("click.question", (still, None));
    m.insert("wheel.question", (still, None));
    m.insert("mouse.trial", (back, no_today));
    // the new-frit form's moving keys move between its rows, onto a field
    // whose text is typed or onto the row that adds the font set
    for id in [
        "up.layer.new-frit",
        "down.layer.new-frit",
        "home.layer.new-frit",
        "end.layer.new-frit",
        "pgup.layer.new-frit",
        "pgdn.layer.new-frit",
    ] {
        m.insert(
            id,
            (
                |s| {
                    if s.exited() || !s.kept() || !s.step.to.node().starts_with("looks.new-frit") {
                        Err("the key left the form or changed something".into())
                    } else {
                        Ok(())
                    }
                },
                None,
            ),
        );
    }
    m.insert("esc.layer.list", (leaves_list, None));
    // rows of what is not built yet, whose nodes the walk reaches in
    // another shape today: what the key does there today is not pinned
    let not_built: Check = |_| Err("not built".into());
    for id in [
        "list.settings-values",
        "list.flow-card",
        "list.filter-picker",
        "list.font-set-files",
        "list.terminals",
        "list.font-sets",
        "list.font-sizes",
        "enter.view.settings",
        "enter.layer.settings-section",
        "esc.layer.settings-section",
        "enter.view.looks",
    ] {
        m.insert(id, (not_built, None));
    }
    // the Looks view lists sizes today, where p and w do nothing
    m.insert("p.view.looks", (not_built, Some(nothing)));
    m.insert("window-ops.view.looks", (not_built, Some(nothing)));
    m.insert(
        "enter.typing.settings",
        (
            |s| {
                if s.after.app.editing.is_none() && !s.exited() {
                    Ok(())
                } else {
                    Err("the value is still typed".into())
                }
            },
            no_today,
        ),
    );
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
    m.insert("o.view.looks", (nothing, Some(ops_on)));
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
    m.insert("updown.typing.settings", (nothing, None));
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
    /// The tree's nodes the walk reached, each with the keys that first
    /// reach it.
    pub nodes: BTreeMap<String, String>,
    /// What each view wrote, by the views table's names.
    pub writes: BTreeMap<&'static str, BTreeSet<String>>,
    /// The actions each view's tab bar showed.
    pub actions: BTreeMap<&'static str, BTreeSet<String>>,
}

/// The checks, with the tables read once.
pub struct Checks {
    rows: Vec<KeyRow>,
    tree: Vec<tables::TreeNode>,
    checks: BTreeMap<&'static str, (Check, Option<Check>)>,
    pub found: Findings,
}

impl Checks {
    pub fn new() -> Checks {
        Checks {
            rows: tables::keys(),
            tree: tables::tree(),
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
        // every state the walk reaches is a node of the interface tree
        let node_id = step.from.node();
        let to_id = step.to.node();
        for (id, abs) in [(&node_id, &step.from), (&to_id, &step.to)] {
            if id != "left" && !self.found.nodes.contains_key(id) {
                let path = if std::ptr::eq(abs, &step.to) {
                    step.scenario()
                } else {
                    let (term, cols, rows) = step.start;
                    format!("{} {cols}×{rows}: {}", term.name(), step.path.join(" "))
                };
                self.found.nodes.insert(id.clone(), path);
            }
        }
        let Some(node) = self.tree.iter().find(|n| n.id == node_id).cloned() else {
            return self.views(&seen);
        };
        // a resize is no key, so the key table says nothing about it
        let resize = matches!(step.key, "Shrink" | "Grow");
        let rows: Vec<KeyRow> = if resize {
            Vec::new()
        } else {
            tables::rows_for(&self.rows, &node, step.key)
                .into_iter()
                .cloned()
                .collect()
        };
        // every key that acts in a node has a row there: a key that
        // changes the state or causes something, and that no row the node
        // lists names, is a key the records do not describe
        if !resize && rows.is_empty() && !step.idle() {
            self.fail(
                step,
                format!(
                    "{} acts in {node_id}, and no row of the node names it",
                    step.key
                ),
            );
        }
        for row in rows {
            self.found.reached.insert(row.id.clone());
            let check = self
                .checks
                .get(row.id.as_str())
                .copied()
                .or_else(|| generic(&row));
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
                let small = vtium_config::tui::view::too_small(app) && app.confirm.is_none();
                if !small && let Some(key) = line.split("  ").find(|k| !shown.contains(k)) {
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
        // each key row is checked in every node the walk reaches that
        // lists it
        for row in &self.rows {
            let listing: Vec<&str> = self
                .tree
                .iter()
                .filter(|n| n.keys.contains(&row.id) && self.found.nodes.contains_key(&n.id))
                .map(|n| n.id.as_str())
                .collect();
            if !row.key_line && !listing.is_empty() && !self.found.reached.contains(&row.id) {
                self.found.failures.push(format!(
                    "the key table's row {} matched no transition in {}",
                    row.id,
                    listing.join(", ")
                ));
            }
        }
        self.found
            .failures
            .extend(walk_against_tree(&self.tree, &self.found.nodes));
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

/// Whether the walk reaches each planned node of the tree today, by the
/// node's `now` column. A planned node is checked in reverse, as a planned
/// key row is: the check expects what vtium-config does today, so it fails
/// once the work lands and the node still says planned.
fn reached_today(id: &str) -> Option<bool> {
    Some(match id {
        // the view, the tab or the form exists today in another shape
        "looks" | "fonts" | "settings" | "looks.new-frit" | "looks.new-frit.typing" => true,
        "settings.section.typing" => true,
        // nothing like it exists today
        "flow.card.list" | "looks.list" | "looks.new-frit.list" => false,
        "settings.section" | "settings.section.list" | "any.question.window-ops" => false,
        _ => return None,
    })
}

/// The walk and the tree meet in the middle: every state the walk reaches
/// is a node of the tree, and every supported node is reached. `tui` and
/// `any` group the other nodes and are no state of their own.
pub fn walk_against_tree(
    tree: &[tables::TreeNode],
    reached: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (id, keys) in reached {
        if !tree.iter().any(|n| n.id == *id) {
            out.push(format!(
                "the walk reaches {id}, which the tree has no node for: {keys}"
            ));
        }
    }
    for node in tree
        .iter()
        .filter(|n| !matches!(n.id.as_str(), "tui" | "any"))
    {
        let is = reached.contains_key(&node.id);
        let want = match node.status.as_str() {
            "supported" => true,
            _ => match reached_today(&node.id) {
                Some(want) => want,
                None => {
                    out.push(format!(
                        "the planned node {} has no check of what the TUI does today",
                        node.id
                    ));
                    continue;
                }
            },
        };
        if is != want {
            out.push(format!(
                "the tree's node {} is {}, and the walk {} it",
                node.id,
                node.status,
                if is { "reaches" } else { "never reaches" }
            ));
        }
    }
    out
}

/// The keys that move inside a list, a view or the file view.
const MOVES: &[&str] = &[
    "Up", "Down", "Left", "Right", "Home", "End", "PgUp", "PgDn", "j", "k", "h", "l", "g", "G",
    "Ctrl-D", "Ctrl-U",
];

/// A check for a row that the table in `row_checks` does not name, by
/// what kind of key the row is about: a key that moves stays in its node
/// and changes nothing the person made; a letter or Backspace typed into a
/// value stays in the typing; any other key acts without leaving the TUI.
/// A planned row of this kind records only that the walk reached it.
fn generic(row: &KeyRow) -> Option<(Check, Option<Check>)> {
    let moves: Check = |s| {
        if s.exited() || s.step.from.node() != s.step.to.node() || !s.kept() {
            Err("the key left its node, quit, or changed what the person made".into())
        } else {
            Ok(())
        }
    };
    let types: Check = |s| {
        if s.exited()
            || s.step.to.kind != StateKind::Typing
            || Seen::draft(s.before) != Seen::draft(s.after)
        {
            Err("the key left the typing or changed the draft".into())
        } else {
            Ok(())
        }
    };
    let stays: Check = |s| {
        if s.exited() {
            Err("the key quit".into())
        } else {
            Ok(())
        }
    };
    let means = if row.letters || row.keys == ["Backspace"] {
        types
    } else if !row.keys.is_empty() && row.keys.iter().all(|k| MOVES.contains(k)) {
        moves
    } else {
        stays
    };
    Some((means, None))
}
