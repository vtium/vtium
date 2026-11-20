//! The checks after every key: the invariants that ADR-1403 D7, ADR-1201
//! D3 and D4 set, each row of ADR-1403's key table, and ADR-1201's views
//! table. Each check sees one transition: the TUI before the key, the key,
//! the TUI after it and what the key caused.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use vtium_config::tui::flowlayout::{self, CardLine};
use vtium_config::tui::flowview::{Obj, Slot};
use vtium_config::tui::input::Key;
use vtium_config::tui::model::{Then, View};
use vtium_config::tui::paths::Piece;

use super::explore::Step;
use super::sim::{Press, Sim, Term};
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
        sim.app.shared.draft.as_ref().map(|d| d.text.as_str())
    }

    /// Nothing the person made changed: the draft, the changes that wait
    /// for Save and the font sets that wait for an install.
    fn kept(&self) -> bool {
        Self::draft(self.before) == Self::draft(self.after)
            && self.before.app.shared.unsaved == self.after.app.shared.unsaved
            && self.before.app.shared.pending == self.after.app.shared.pending
    }

    /// The changes that waited for Save are in the draft the desk keeps.
    fn saved(&self) -> bool {
        self.before.app.shared.unsaved == 0
            || (self.has("save") && self.after.desk.draft == Self::draft(self.before).unwrap_or(""))
    }

    /// The key quits, or asks the question that fits first: the quit
    /// question while changes wait for Save, the leave question while font
    /// sets wait for an install.
    fn quits_or_asks(&self) -> Result<(), String> {
        let app = &self.before.app;
        let want = if !app.shared.pending.is_empty() {
            "question:leave"
        } else if app.shared.unsaved > 0 {
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

    /// `.` shows the key line's next page, and the first page after the
    /// last, and changes nothing else; where the keys fit on one row it
    /// does nothing. Pressing `.` once for each page goes round every page
    /// and back: the pages together show every label the node's supported
    /// rows give, the first page shows the labels of the rows the key
    /// table puts there, and the keys would not fit on one row.
    ///
    /// References:
    /// - ADR-1403 D7, "The same keys in every view": the key line is one
    ///   row, and `.` shows its next page.
    fn next_page(&self) -> Result<(), String> {
        let line = |sim: &Sim| vtium_config::tui::layout::key_line(&sim.app).to_owned();
        let before = line(self.before);
        let Some((at, pages)) = page_of(&before) else {
            return self.nothing();
        };
        if !self.idle() || !self.kept() {
            return Err("`.` did more than show the next page".into());
        }
        let next = if at == pages { 1 } else { at.saturating_add(1) };
        let after = line(self.after);
        if page_of(&after) != Some((next, pages)) {
            return Err(format!("`.` on page {at} of {pages} shows {after:?}"));
        }
        let (want, first) = node_labels(&self.step.from.node());
        let mut sim = self.before.clone();
        let mut shown: Vec<&str> = Vec::new();
        let mut items: Vec<String> = Vec::new();
        for _ in 0..pages {
            let now = line(&sim);
            let (page, _) =
                page_of(&now).ok_or_else(|| format!("a page without its more item: {now:?}"))?;
            let mut got = labels_of(&now, &want)?;
            if page == 1 && !first.is_empty() {
                got.sort_unstable();
                if got != first {
                    return Err(format!(
                        "the first page shows {got:?}, and the key table puts {first:?} there"
                    ));
                }
            }
            shown.extend(got);
            items.extend(
                now.split("  ")
                    .filter(|i| !i.starts_with(". "))
                    .map(str::to_owned),
            );
            sim.press(Press::Key(Key::Char('.')));
        }
        if line(&sim) != before {
            return Err(format!(
                "{pages} presses of `.` do not come back to {before:?}"
            ));
        }
        shown.sort_unstable();
        shown.dedup();
        if shown != want {
            return Err(format!(
                "the pages show the labels {shown:?}, and the contract gives {want:?}"
            ));
        }
        let width = items.join("  ").chars().count();
        if width <= KEY_LINE_COLUMNS {
            return Err(format!(
                "the keys fit on one row of {width} columns, and the key line still has pages"
            ));
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
        let Some(trial) = &self.before.app.nav.trial else {
            return Err("no trial".into());
        };
        if self.after.app.nav.trial.is_some() || self.exited() || !self.kept() {
            return Err("the trial goes on, or something else changed".into());
        }
        if self.before.term == Term::Vtium && self.after.console_font != trial.back {
            return Err(format!("the console draws {}", self.after.console_font));
        }
        Ok(())
    }

    /// The key moves to the node `id` of the tree, and changes nothing
    /// the person made.
    fn goes_to(&self, id: &str) -> Result<(), String> {
        if self.exited() || !self.kept() {
            return Err("the key quit or changed what the person made".into());
        }
        let to = self.step.to.node();
        if to == id {
            Ok(())
        } else {
            Err(format!("expected {id}, and the TUI is in {to}"))
        }
    }

    fn opens(&self, state: &str) -> Result<(), String> {
        if self.step.to.table_state() == state {
            Ok(())
        } else {
            Err(format!("expected {state}"))
        }
    }

    /// The terminal card open in `sim`, with the line under its cursor:
    /// its object, its path and the line. `None` outside a card.
    fn card(sim: &Sim) -> Option<(Obj, vtium_config::tui::paths::Path, CardLine)> {
        let fv = sim.app.flow.state();
        let obj = fv.layer.clone()?;
        let path = flowlayout::card_path(sim.app.flow.state(), &sim.app.shared, &obj)?;
        let line = flowlayout::card_lines(&path, matches!(obj, Obj::New(_)))
            .get(fv.line)
            .cloned()?;
        Some((obj, path, line))
    }

    /// Enter on a card's line sets the line to the choice under the
    /// cursor, or starts typing where that choice is typed, or says in the
    /// message why it refuses the choice. A display's or a device's line
    /// has one choice, which disconnects it.
    fn sets_the_field(&self) -> Result<(), String> {
        let Some((_, _, line)) = Self::card(self.before) else {
            return Err("no card was open".into());
        };
        let pick = self.before.app.flow.state().pick;
        if self.exited() {
            return Err("Enter left the TUI".into());
        }
        if flowlayout::card_types(&self.before.app.shared, &line, pick) {
            return if self.after.app.flow.state().typing.is_some() {
                Ok(())
            } else {
                Err("Enter on a typed choice did not start the typing".into())
            };
        }
        if self.after.app.shared.message.is_some()
            && self.after.app.shared.message != self.before.app.shared.message
        {
            return Ok(());
        }
        // a display's or a device's one choice disconnects it, so its line
        // goes from the card
        if matches!(line, CardLine::Part(_)) {
            let still = Self::card(self.after).is_some_and(|(obj, path, _)| {
                flowlayout::card_lines(&path, matches!(obj, Obj::New(_))).contains(&line)
            });
            return if still {
                Err("Enter on a part left it on the card".into())
            } else {
                Ok(())
            };
        }
        // the card the change leaves open shows the line at the choice set
        let held = Self::card(self.after).and_then(|(_, path, after_line)| {
            (after_line == line)
                .then(|| flowlayout::card_chosen(&self.after.app.shared, &path, &line))
                .flatten()
        });
        if held == Some(pick) {
            Ok(())
        } else {
            Err(format!("Enter on choice {pick} left the line at {held:?}"))
        }
    }

    fn typed(sim: &Sim) -> Option<String> {
        let app = &sim.app;
        app.flow
            .state()
            .typing
            .clone()
            .or_else(|| app.settings.editing().map(str::to_owned))
            .or_else(|| match app.nav.view {
                View::New => app.looks.form().fields.get(app.looks.form().at).cloned(),
                View::Looks => app.looks.card().and_then(|c| c.typed()).map(str::to_owned),
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
    for id in ["period.view", "period.layer"] {
        m.insert(id, (|s| s.next_page(), Some(nothing)));
    }
    m.insert(
        "esc.view.flow",
        (
            |s| {
                if s.after.app.flow.state().new == Default::default() && !s.exited() {
                    Ok(())
                } else {
                    Err("NEW PATH is not empty".into())
                }
            },
            no_today,
        ),
    );
    let closes: Check = |s| {
        if s.step.to.kind == StateKind::View
            && s.after.app.nav.view.tab() == s.before.app.nav.view.tab()
        {
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
    for id in ["1.view", "2.view", "3.view", "4.view", "5.view", "6.view"] {
        m.insert(id, (goes_to_tab, Some(nothing)));
    }
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
                    return if s.has("install") || s.after.app.shared.message.is_none() {
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
    m.insert("n.question", (no, no_today));
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
    m.insert("s.view.fonts", (saves, no_today));
    m.insert("s.view.help", (nothing, no_today));
    m.insert(
        "s.layer.file-view",
        (
            |s| {
                saves(s)?;
                if s.after.app.flow.pager().is_some() {
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
                if s.after.app.flow.pager().is_some() {
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
        if s.before.app.flow.state().at == s.after.app.flow.state().at
            && s.after.app.nav.confirm.is_some()
        {
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
    m.insert("list.filter-picker", (not_built, None));
    // w asks whether to let programs resize the grid, or says the draft
    // already lets them
    m.insert(
        "window-ops.view.looks",
        (
            |s| {
                if s.before.app.shared.window_ops_drafted() {
                    s.nothing()
                } else {
                    s.opens("question:window-ops")
                }
            },
            None,
        ),
    );
    looks_checks(&mut m, not_built, nothing);
    m.insert(
        "enter.typing.settings",
        (
            |s| {
                if s.after.app.settings.editing().is_none() && !s.exited() {
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
    // today `o` turns allow-font-ops on in Help and Settings, and does
    // nothing on the Effects and Fonts tabs, which are new and follow the
    // row
    let ops_on: Check = |s| {
        let new_tab = matches!(s.before.app.nav.view, View::Effects | View::Fonts);
        if s.before.term == Term::OpsOff && !new_tab {
            if s.has("check") {
                Ok(())
            } else {
                Err("expected allow-font-ops turned on".into())
            }
        } else {
            s.nothing()
        }
    };
    m.insert("o.view.looks", (nothing, None));
    m.insert("o.view", (nothing, Some(ops_on)));
    // a card's own keys: Left and Right step the choice under the cursor,
    // Enter sets it, and e on a path's detail takes the path into NEW PATH
    m.insert(
        "left.layer.flow-card",
        (
            |s| {
                let (a, b) = (s.before.app.flow.state(), s.after.app.flow.state());
                if s.exited() || !s.kept() || a.line != b.line {
                    return Err("Left left the card, changed a line or changed the draft".into());
                }
                if b.pick == a.pick.saturating_sub(1) {
                    Ok(())
                } else {
                    Err(format!(
                        "Left stepped the choice from {} to {}",
                        a.pick, b.pick
                    ))
                }
            },
            no_today,
        ),
    );
    m.insert(
        "right.layer.flow-card",
        (
            |s| {
                let (a, b) = (s.before.app.flow.state(), s.after.app.flow.state());
                if s.exited() || !s.kept() || a.line != b.line {
                    return Err("Right left the card, changed a line or changed the draft".into());
                }
                let last = Seen::card(s.before)
                    .map(|(_, _, line)| flowlayout::card_choices(&s.before.app.shared, &line).len())
                    .unwrap_or(0)
                    .saturating_sub(1);
                if b.pick == a.pick.saturating_add(1).min(last) {
                    Ok(())
                } else {
                    Err(format!(
                        "Right stepped the choice from {} to {}",
                        a.pick, b.pick
                    ))
                }
            },
            no_today,
        ),
    );
    let sets: Check = |s| s.sets_the_field();
    m.insert("enter.layer.flow-card", (sets, no_today));
    // the lists the card's Enter is to open are not built: today Enter
    // sets the line as it sets any other
    for id in ["list.flow-card", "list.font-set-files", "list.terminals"] {
        m.insert(id, (not_built, Some(sets)));
    }
    detail_checks(&mut m);
    m.insert(
        "e.layer.flow-info",
        (
            |s| {
                if s.exited() {
                    return Err("e left the TUI".into());
                }
                // NEW PATH holds another path already, which e says
                if !s.before.app.flow.state().new.is_empty() {
                    return if s.after.app.shared.message.is_some() && s.kept() {
                        Ok(())
                    } else {
                        Err("e over a full NEW PATH said nothing or changed something".into())
                    };
                }
                if s.after.app.flow.state().revising.is_some()
                    && s.after.app.flow.state().layer.is_none()
                {
                    Ok(())
                } else {
                    Err("e did not take the path into NEW PATH".into())
                }
            },
            no_today,
        ),
    );
    m.insert(
        "enter.layer.restore",
        (
            |s| {
                if s.step.to.kind == StateKind::Question && !s.exited() && s.kept() {
                    Ok(())
                } else {
                    Err("Enter on an installed file did not ask before copying it".into())
                }
            },
            no_today,
        ),
    );
    m.insert(
        "t.layer.flow-card",
        (
            |s| {
                if s.before.term != Term::Vtium && s.step.did.switches > 0 {
                    return Err("a switch outside a vtium console".into());
                }
                if s.exited() || s.after.app.nav.view != View::Flow {
                    return Err("t left the card".into());
                }
                Ok(())
            },
            // today t does nothing on a card
            Some(nothing),
        ),
    );
    m.insert(
        "k.trial",
        (
            |s| {
                if s.after.app.nav.trial.is_none() && s.step.did.switches == 0 && s.kept() {
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
    settings_checks(&mut m);
    m
}

/// A part's detail: its first line is the part's label, and the lines
/// below it are the choices that narrow the part. On the label line, Up
/// does nothing, Down moves to the first choice and Enter starts typing
/// the label. Among the choices, Up and Down move by one line, Up from the
/// first choice goes back to the label line, and Enter picks the choice
/// under the cursor into NEW PATH and closes the detail.
///
/// References:
/// - ADR-1403 D7, "The contract": the key table's rows of the part's
///   detail and of its choices.
fn detail_checks(m: &mut BTreeMap<&'static str, (Check, Option<Check>)>) {
    // the line under the detail's cursor before and after the key, with
    // the number of choices below the label line
    fn lines(s: &Seen<'_>) -> Result<(usize, usize, usize), String> {
        let fv = s.before.app.flow.state();
        let obj = fv.layer.as_ref().ok_or("no detail was open")?;
        let len = flowlayout::choices_for(&s.before.app.shared, obj).len();
        Ok((fv.line, s.after.app.flow.state().line, len))
    }
    m.insert("up.layer.flow-detail", (|s| s.nothing(), None));
    m.insert(
        "down.layer.flow-detail",
        (
            |s| {
                s.goes_to("flow.detail.choices")?;
                match lines(s)? {
                    (0, 1, _) => Ok(()),
                    (a, b, _) => Err(format!("Down moved the line from {a} to {b}")),
                }
            },
            None,
        ),
    );
    m.insert(
        "enter.layer.flow-detail",
        (
            |s| {
                s.goes_to("flow.detail.typing")?;
                let doc = vtium_config::tui::paths::read(&s.before.app.shared.inputs.flow);
                let obj = s.before.app.flow.state().layer.clone().ok_or("no detail")?;
                let (kind, sel) = flowlayout::label_target(&obj).ok_or("no label")?;
                let now = doc.label(kind, &sel).unwrap_or_default().to_owned();
                match Seen::typed(s.after) {
                    Some(t) if t == now => Ok(()),
                    t => Err(format!("the typing starts at {t:?}, not the label {now:?}")),
                }
            },
            None,
        ),
    );
    // End and PgDn from the label line move into the choices, as far as
    // the last
    for (id, by) in [
        ("end.layer.flow-detail", usize::MAX),
        ("pgdn.layer.flow-detail", vtium_config::tui::listbox::PAGE),
    ] {
        let check: Check = match by {
            usize::MAX => |s| {
                s.goes_to("flow.detail.choices")?;
                match lines(s)? {
                    (0, b, len) if b == len => Ok(()),
                    (a, b, _) => Err(format!("End moved the line from {a} to {b}")),
                }
            },
            _ => |s| {
                s.goes_to("flow.detail.choices")?;
                match lines(s)? {
                    (0, b, len) if b == vtium_config::tui::listbox::PAGE.min(len) => Ok(()),
                    (a, b, _) => Err(format!("PgDn moved the line from {a} to {b}")),
                }
            },
        };
        m.insert(id, (check, None));
    }
    // Home goes back to the label line, and PgUp moves up a page, as far
    // as the label line
    m.insert(
        "home.layer.flow-choices",
        (
            |s| {
                s.goes_to("flow.detail")?;
                match lines(s)? {
                    (_, 0, _) => Ok(()),
                    (a, b, _) => Err(format!("Home moved the line from {a} to {b}")),
                }
            },
            None,
        ),
    );
    m.insert(
        "pgup.layer.flow-choices",
        (
            |s| {
                let (a, b, _) = lines(s)?;
                let want = a.saturating_sub(vtium_config::tui::listbox::PAGE);
                s.goes_to(if want == 0 {
                    "flow.detail"
                } else {
                    "flow.detail.choices"
                })?;
                if b == want {
                    Ok(())
                } else {
                    Err(format!("PgUp moved the line from {a} to {b}"))
                }
            },
            None,
        ),
    );
    m.insert(
        "up.layer.flow-choices",
        (
            |s| match lines(s)? {
                (1, 0, _) => s.goes_to("flow.detail"),
                (a, b, _) if a > 1 && b == a - 1 => s.goes_to("flow.detail.choices"),
                (a, b, _) => Err(format!("Up moved the line from {a} to {b}")),
            },
            None,
        ),
    );
    m.insert(
        "down.layer.flow-choices",
        (
            |s| {
                s.goes_to("flow.detail.choices")?;
                match lines(s)? {
                    (a, b, len) if b == a.saturating_add(1).min(len) => Ok(()),
                    (a, b, _) => Err(format!("Down moved the line from {a} to {b}")),
                }
            },
            None,
        ),
    );
    m.insert(
        "enter.layer.flow-choices",
        (
            |s| {
                let fv = s.before.app.flow.state();
                let obj = fv.layer.clone().ok_or("no detail was open")?;
                let list = flowlayout::choices_for(&s.before.app.shared, &obj);
                let Some(sel) = list
                    .get(fv.line.saturating_sub(1))
                    .and_then(|c| c.2.clone())
                else {
                    // a choice with no selector is a heading, and Enter on
                    // it does nothing
                    return s.nothing();
                };
                if s.exited() || s.after.app.flow.state().layer.is_some() || !s.kept() {
                    return Err("Enter left the detail open, quit, or changed the draft".into());
                }
                let piece = match obj {
                    Obj::Pal(Slot::Display(_)) | Obj::New(Slot::Display(_)) => Piece::Display(sel),
                    _ => Piece::Input(sel),
                };
                let said = s.after.app.shared.message.is_some()
                    && s.after.app.shared.message != s.before.app.shared.message;
                if s.after.app.flow.state().new.has(&piece) || said {
                    Ok(())
                } else {
                    Err(format!(
                        "Enter did not pick {piece:?} into NEW PATH or say why"
                    ))
                }
            },
            None,
        ),
    );
}

/// The Settings view's rows: Enter on a section opens it, Enter on a key
/// opens its value for typing or its allowed values as the shared list,
/// and Esc goes back to the sections. None of them writes anything.
///
/// References:
/// - ADR-1201 D3, the Settings bullet: the view opens on the sections,
///   Enter opens one and Esc goes back with the same section highlighted.
/// - ADR-1403 D7, "One list for every set of items": a key's allowed
///   values open in the shared list.
fn settings_checks(m: &mut BTreeMap<&'static str, (Check, Option<Check>)>) {
    m.insert(
        "enter.view.settings",
        (
            |s| {
                s.goes_to("settings.section")?;
                let (want, got) = (
                    s.before.app.settings.section_at(),
                    s.after.app.settings.open().map(|(at, list)| (at, list.at)),
                );
                if got == Some((want, 0)) {
                    Ok(())
                } else {
                    Err(format!(
                        "Enter on section {want} opened {got:?}, not its first key"
                    ))
                }
            },
            None,
        ),
    );
    m.insert(
        "enter.layer.settings-section",
        (
            |s| match s.step.from.focus.as_str() {
                "switch" | "choice" => s.goes_to("settings.section.list"),
                _ => s.goes_to("settings.section.typing"),
            },
            None,
        ),
    );
    // the list opens on the value the key has now
    m.insert(
        "list.settings-values",
        (
            |s| {
                s.goes_to("settings.section.list")?;
                let app = &s.after.app;
                let key = app.settings.key().ok_or("no key is highlighted")?;
                let words = vtium_config::tui::tabs::settings::fixed_values(key)
                    .ok_or("the key has no fixed values")?;
                let now = app.value(key).0;
                let at = app.settings.values().map(|l| l.at);
                match words.iter().position(|w| *w == now) {
                    Some(i) if at != Some(i) => Err(format!(
                        "the list highlights {at:?}, and the key's value {now} is item {i}"
                    )),
                    _ => Ok(()),
                }
            },
            None,
        ),
    );
    m.insert(
        "esc.layer.settings-section",
        (
            |s| {
                s.goes_to("settings")?;
                let open = s.before.app.settings.open().map(|(at, _)| at);
                if open == Some(s.after.app.settings.section_at()) {
                    Ok(())
                } else {
                    Err(format!(
                        "Esc from section {open:?} highlights section {}",
                        s.after.app.settings.section_at()
                    ))
                }
            },
            None,
        ),
    );
    // Up and Down move the highlight by one key and stay in the section
    fn moved(s: &Seen<'_>, by: isize) -> Result<(), String> {
        s.goes_to("settings.section")?;
        let at = |sim: &Sim| sim.app.settings.open().map(|(_, list)| list.at);
        let (a, b) = (at(s.before), at(s.after));
        let len = s
            .before
            .app
            .settings
            .section()
            .map_or(0, |sec| sec.keys().len());
        let want = a.map(|a| a.saturating_add_signed(by).min(len.saturating_sub(1)));
        if b == want {
            Ok(())
        } else {
            Err(format!("the highlight went from {a:?} to {b:?}"))
        }
    }
    m.insert("up.layer.settings-section", (|s| moved(s, -1), None));
    m.insert("down.layer.settings-section", (|s| moved(s, 1), None));
}

fn step_view(s: &Seen<'_>, forward: bool) -> Result<(), String> {
    let tabs = View::TABS;
    let at = tabs
        .iter()
        .position(|v| *v == s.before.app.nav.view.tab())
        .unwrap_or(0);
    let n = tabs.len();
    let next = if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    };
    if Some(&s.after.app.nav.view) == tabs.get(next) {
        Ok(())
    } else {
        Err(format!("expected {:?}", tabs.get(next)))
    }
}

/// A digit goes straight to its tab, the tab keeps what it had
/// highlighted, and nothing the person made changes: `1` Fonts, `2`
/// Effects, `3` Looks, `4` Flow, `5` Settings and `6` Help.
///
/// References:
/// - ADR-1403 D10, "The number keys": the digits and their tabs.
fn goes_to_tab(s: &Seen<'_>) -> Result<(), String> {
    let want = match s.step.key {
        "1" => "fonts",
        "2" => "effects",
        "3" => "looks",
        "4" => "flow",
        "5" => "settings",
        "6" => "help",
        other => return Err(format!("{other} is no digit of a tab")),
    };
    let to = view_name(s.after.app.nav.view);
    if to != want || s.step.to.kind != StateKind::View {
        return Err(format!("expected the {want} tab, and the TUI is in {to}"));
    }
    if s.exited() || !s.kept() {
        return Err("the digit quit or changed what the person made".into());
    }
    if s.after.app.flow.state().at != s.before.app.flow.state().at
        || s.after.app.fonts.at() != s.before.app.fonts.at()
        || s.after.app.settings.section_at() != s.before.app.settings.section_at()
    {
        return Err("a tab lost the item it had highlighted".into());
    }
    Ok(())
}

fn saves(s: &Seen<'_>) -> Result<(), String> {
    if s.before.app.shared.unsaved == 0 {
        return if s.quiet() && s.kept() {
            Ok(())
        } else {
            Err("nothing to save, but something happened".into())
        };
    }
    if s.saved() && s.after.app.shared.unsaved == 0 {
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

/// The checks of the Looks tab's rows and of a look's card: what Enter,
/// `n`, `r`, `t` and `p` do on each kind of item, and what the card's keys
/// do on each line.
fn looks_checks(
    m: &mut BTreeMap<&'static str, (Check, Option<Check>)>,
    not_built: Check,
    nothing: Check,
) {
    m.insert(
        "enter.view.looks",
        (
            |s| match looks_group(s) {
                "look" => s.goes_to("looks.look"),
                "no looks" => s.nothing(),
                "color scheme" => s.goes_to("looks.palette"),
                "pointer theme" => s.goes_to("looks.pointer"),
                _ => s.goes_to("looks.font-set"),
            },
            None,
        ),
    );
    m.insert(
        "n.view.looks",
        (
            |s| match looks_group(s) {
                "color scheme" | "pointer theme" => s.nothing(),
                // a new look goes into the draft, and its card opens with
                // its name being typed
                "look" | "no looks" => {
                    let to = s.step.to.node();
                    if to == "looks.look.typing" && Seen::draft(s.before) != Seen::draft(s.after) {
                        Ok(())
                    } else {
                        Err(format!(
                            "expected a new look being named, and the TUI is in {to}"
                        ))
                    }
                }
                _ => {
                    let to = s.step.to.node();
                    if to == "looks.new-frit.typing" {
                        Ok(())
                    } else {
                        Err(format!(
                            "expected the form for a new font set, and the TUI is in {to}"
                        ))
                    }
                }
            },
            None,
        ),
    );
    m.insert(
        "r.view.looks",
        (
            |s| match looks_group(s) {
                "look" => s.opens("question:change"),
                "no looks" | "color scheme" | "pointer theme" => s.nothing(),
                // a font set is marked for removal, or the message says why
                // it cannot be
                _ => {
                    if s.exited() || s.step.to.node() != "looks" {
                        Err("r left the list".into())
                    } else if s.before.app.shared.pending != s.after.app.shared.pending
                        || s.after.app.shared.message.is_some()
                    {
                        Ok(())
                    } else {
                        Err("r neither marked the font set nor said why".into())
                    }
                }
            },
            None,
        ),
    );
    let tries: Check = |s| {
        if s.exited() || !s.kept() {
            Err("t quit or changed what the person made".into())
        } else {
            Ok(())
        }
    };
    m.insert(
        "t.view.looks",
        (
            |s| match looks_group(s) {
                "color scheme" | "pointer theme" => s.nothing(),
                _ => {
                    if s.exited() || !s.kept() {
                        Err("t quit or changed what the person made".into())
                    } else {
                        Ok(())
                    }
                }
            },
            None,
        ),
    );
    m.insert("t.layer.look", (tries, None));
    m.insert(
        "p.view.looks",
        (
            |s| match looks_group(s) {
                // a color scheme's preview has no second form
                "color scheme" | "pointer theme" => s.nothing(),
                _ => {
                    if s.after.app.looks.picture() != s.before.app.looks.picture() && s.kept() {
                        Ok(())
                    } else {
                        Err("p did not switch the preview".into())
                    }
                }
            },
            None,
        ),
    );
    m.insert("left.layer.look", (|s| steps(s, true), None));
    m.insert("right.layer.look", (|s| steps(s, false), None));
    m.insert(
        "enter.layer.look",
        (|s| s.goes_to("looks.look.typing"), None),
    );
    for id in [
        "list.font-sets",
        "list.font-sizes",
        "list.palettes",
        "list.look-effects",
    ] {
        m.insert(id, (|s| s.goes_to("looks.look.list"), None));
    }
    // a look draws vtium.conf's pointer theme, so its line has no list
    m.insert("list.pointer-themes", (not_built, Some(nothing)));
}

/// The group of the item the Looks tab highlighted before the key, as
/// its focus names it: `look`, `no looks`, `color scheme`, `pointer theme`,
/// or a font set's kind.
fn looks_group<'a>(s: &Seen<'a>) -> &'a str {
    let focus = s.step.from.focus.as_str();
    if focus.starts_with("built-in scheme") || focus.starts_with("color scheme:") {
        "color scheme"
    } else if focus.starts_with("pointer theme:") {
        "pointer theme"
    } else {
        focus.split(',').next().unwrap_or("")
    }
}

/// Left, with `back`, and Right step the highlighted line of a look's
/// card to the choice before or after the look's value, which the look's
/// key then takes. At either end of the choices, and on a line with none,
/// they change nothing. Both stay on the card.
fn steps(s: &Seen<'_>, back: bool) -> Result<(), String> {
    if s.exited() || s.step.to.node() != "looks.look" {
        return Err("the key left the card".into());
    }
    let Some(card) = s.before.app.looks.card() else {
        return Err("no card was open before the key".into());
    };
    let (values, now) = card.values(&s.before.app.shared);
    let at = values.iter().position(|v| *v == now);
    let want = match at {
        Some(at) if back => at.checked_sub(1),
        Some(at) => at.checked_add(1).filter(|to| *to < values.len()),
        None => None,
    };
    let after = s
        .after
        .app
        .looks
        .card()
        .map(|c| c.values(&s.after.app.shared).1);
    match want.and_then(|to| values.get(to)) {
        None if s.kept() => Ok(()),
        None => Err("the key changed the draft with no choice to step to".into()),
        Some(to) if after.as_ref() == Some(to) && !s.kept() => Ok(()),
        Some(to) => Err(format!(
            "the key left the look's value at {after:?}, not at the next choice {to:?}"
        )),
    }
}

/// When a row applies, for rows that share a key in one node: the row is
/// checked, and counts as reached, only on a transition from a state it
/// applies in.
type Case = fn(&Seen<'_>) -> bool;

/// The cases of the rows that share a key in one node. Enter on a
/// terminal's card sets the line under the cursor today; the planned
/// lists it is to open each belong to one kind of line.
fn row_cases() -> BTreeMap<&'static str, Case> {
    let mut m: BTreeMap<&'static str, Case> = BTreeMap::new();
    m.insert("list.flow-card", |s| {
        matches!(
            card_line(s),
            Some(CardLine::Part(_) | CardLine::Setting("vt"))
        )
    });
    // the allowed values open as a list only on a key with a fixed set
    m.insert("list.settings-values", |s| {
        matches!(s.step.from.focus.as_str(), "switch" | "choice")
    });
    m.insert("list.font-set-files", |s| {
        matches!(card_line(s), Some(CardLine::Font))
    });
    m.insert("list.terminals", |s| {
        matches!(card_line(s), Some(CardLine::Name))
    });
    // each line of a look's card opens its own list, and Enter on the name
    // types it
    m.insert("enter.layer.look", |s| s.step.from.focus == "name");
    m.insert("list.font-sets", |s| s.step.from.focus == "font set");
    m.insert("list.font-sizes", |s| s.step.from.focus == "size");
    m.insert("list.palettes", |s| s.step.from.focus == "color scheme");
    m.insert("list.look-effects", |s| s.step.from.focus == "vtlook");
    m.insert("list.pointer-themes", |s| {
        s.step.from.focus == "pointer theme"
    });
    m
}

/// The line under the cursor of the card open before the key.
fn card_line(s: &Seen<'_>) -> Option<CardLine> {
    Seen::card(s.before).map(|(_, _, line)| line)
}

/// What one run of the checks found.
#[derive(Default)]
pub struct Findings {
    /// Each failure, with the scenario that shows it.
    pub failures: Vec<String>,
    /// The key table's rows some transition reached, each with the node
    /// of the tree it was reached in.
    pub reached: BTreeSet<(String, String)>,
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
    cases: BTreeMap<&'static str, Case>,
    pub found: Findings,
}

impl Checks {
    pub fn new() -> Checks {
        Checks {
            rows: tables::keys(),
            tree: tables::tree(),
            checks: row_checks(),
            cases: row_cases(),
            found: Findings::default(),
        }
    }

    /// How many lines the terminal card open in `sim` has, 0 outside a
    /// card.
    pub fn card_line_count(sim: &Sim) -> usize {
        sim.app
            .flow
            .state()
            .layer
            .as_ref()
            .and_then(|obj| {
                let path = flowlayout::card_path(sim.app.flow.state(), &sim.app.shared, obj)?;
                Some(flowlayout::card_lines(&path, matches!(obj, Obj::New(_))).len())
            })
            .unwrap_or(0)
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
        if let Err(e) = self.contract_key_line(&seen) {
            self.fail(step, format!("help.generated: {e}"));
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
            if let Some(case) = self.cases.get(row.id.as_str())
                && !case(&seen)
            {
                continue;
            }
            self.found.reached.insert((node_id.clone(), row.id.clone()));
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

    /// Whether the key table's row `id` is supported.
    fn supported(&self, id: &str) -> bool {
        self.rows
            .iter()
            .any(|r| r.id == id && r.status == "supported")
    }

    /// The key line the TUI shows after the key holds what the contract
    /// gives the node the TUI is in, and nothing else: each item is one
    /// label of the node's supported rows that the key line shows. A key
    /// line with one page shows every such label. A paged key line ends
    /// with its more item, `. more 2/3`, and shows its first page after a
    /// key that moves to another node; the row check of `.` checks that
    /// its pages together show every label. A node that is still planned
    /// keeps its hand-written key line until its work lands, so it is not
    /// checked.
    ///
    /// References:
    /// - ADR-1403 D7, "The contract": the key lines are generated from the
    ///   key table and the tree, and the row `help.generated` tracks them.
    /// - ADR-1403 D7, "The same keys in every view": the key line is one
    ///   row that shows its keys in pages.
    fn contract_key_line(&self, s: &Seen<'_>) -> Result<(), String> {
        if s.after.exited.is_some() || !self.supported("help.generated") {
            return Ok(());
        }
        let node_id = s.step.to.node();
        let Some(node) = self.tree.iter().find(|n| n.id == node_id) else {
            return Ok(());
        };
        if node.status != "supported" {
            return Ok(());
        }
        let (want, _) = node_labels(&node_id);
        let line = vtium_config::tui::layout::key_line(&s.after.app);
        let mut got = labels_of(line, &want).map_err(|e| format!("{node_id}: {e}"))?;
        got.sort_unstable();
        got.dedup();
        match page_of(line) {
            None if got != want => Err(format!(
                "{node_id}: the key line shows the labels {got:?}, and the contract gives {want:?}"
            )),
            None => Ok(()),
            Some((at, pages)) if at == 0 || at > pages => Err(format!(
                "{node_id}: the key line shows page {at} of {pages}"
            )),
            Some((at, _)) if at != 1 && s.step.from.node() != node_id => Err(format!(
                "{node_id}: the key line shows page {at} after a key that came from {}",
                s.step.from.node()
            )),
            Some(_) => Ok(()),
        }
    }

    /// The invariants after every key.
    fn invariants(&mut self, s: &Seen<'_>) -> Result<(), String> {
        let app = &s.after.app;
        if s.after.exited.is_none() {
            let shown = super::state::Shown::of(s.after);
            let screen = shown.rows();
            if screen.len() != app.size.rows {
                return Err(format!(
                    "{} rows on a screen of {}",
                    screen.len(),
                    app.size.rows
                ));
            }
            if let Some(row) = screen.iter().find(|r| r.chars().count() > app.size.cols) {
                return Err(format!("a row wider than {}: {row:?}", app.size.cols));
            }
            // the screen shows the node the navigation module names, read
            // from its text and its tab bar, so a tab's `node` that names
            // the wrong node fails here and not only in the map
            shown.agrees(&s.step.to.node())?;
            // mouse reporting is on exactly while the Flow view is in front
            if s.after.mouse != (app.nav.view == View::Flow) {
                return Err(format!(
                    "mouse reporting is {} in {:?}",
                    s.after.mouse, app.nav.view
                ));
            }
            // a question that throws the draft away counts the changes it
            // also throws away; the deactivate question changes the draft's
            // active resources and keeps its unsaved changes
            if let Some(c) = &app.nav.confirm
                && matches!(&c.then, Then::Desk(op) if !matches!(op, vtium_config::tui::desk::Op::Resource { .. }))
                && app.shared.unsaved > 0
                && !c.question.contains("not saved")
            {
                return Err(format!(
                    "the question does not count the unsaved changes: {}",
                    c.question
                ));
            }
            // every key the key line lists shows; in the Flow view and its
            // file view, once their key-line rows are supported
            let flow_lines =
                self.supported("key-line.view.flow") && self.supported("key-line.layer.file-view");
            if app.nav.view != View::Flow || app.nav.confirm.is_some() || flow_lines {
                let shown = screen.join("\n");
                let line = vtium_config::tui::layout::key_line(app);
                let small =
                    vtium_config::tui::layout::too_small(app.size) && app.nav.confirm.is_none();
                if let Some(key) = line.split("  ").find(|k| !shown.contains(k)) {
                    return Err(format!("the key line leaves out {key:?}"));
                }
                // the key line is one row, once the key-line rows say so
                let rows = vtium_config::tui::layout::key_rows(app).len();
                if !small && self.supported("key-line.any") && rows != 1 {
                    return Err(format!("the key line takes {rows} rows"));
                }
            }
        } else {
            if s.after.mouse {
                return Err("left with mouse reporting on".into());
            }
            if let Some(start) = &app.shared.start_font
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
            let moved = s.before.app.nav.view.tab() != app.nav.view.tab();
            if moved != (s.step.from.kind == StateKind::View) {
                return Err(format!("Tab changed the view: {moved}"));
            }
        }
        Ok(())
    }

    /// Records what the key wrote and what the tab bar shows, for the
    /// views table's check.
    fn views(&mut self, s: &Seen<'_>) {
        let view = view_name(s.before.app.nav.view);
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
                .entry(view_name(s.after.app.nav.view))
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
        if s.before.app.shared.pending != s.after.app.shared.pending && !s.has("install") {
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
        // lists it, and a row with a case only in a state it applies in
        for row in &self.rows {
            let missed: Vec<&str> = self
                .tree
                .iter()
                .filter(|n| n.keys.contains(&row.id) && self.found.nodes.contains_key(&n.id))
                .map(|n| n.id.as_str())
                .filter(|n| {
                    !self
                        .found
                        .reached
                        .contains(&((*n).to_owned(), row.id.clone()))
                })
                .collect();
            if !row.key_line && !missed.is_empty() {
                self.found.failures.push(format!(
                    "the key table's row {} matched no transition in {}",
                    row.id,
                    missed.join(", ")
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
            let under = [
                "flow", "looks", "fonts", "effects", "settings", "help", "new-frit",
            ]
            .iter()
            .zip([
                View::Flow,
                View::Looks,
                View::Fonts,
                View::Effects,
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
        // the view, the tab or the form exists today in another shape, the
        // Effects tab as a line that says it is coming
        "effects" | "looks.new-frit" | "looks.new-frit.typing" => true,
        // nothing like it exists today
        "flow.card.list" | "looks.new-frit.list" => false,
        // the six tabs' new screens and questions, which nothing builds yet
        "effects.filter" | "effects.effect" | "any.question.deactivate" => false,
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
    // a tab's list can be in more than one node by what the highlight is
    // on, as the Looks tab's list is on a color scheme, so a key that moves
    // the highlight stays in the tab's list
    let moves: Check = |s| {
        let (from, to) = (s.step.from.node(), s.step.to.node());
        let listed = super::reference::kind(&from) == "view"
            && super::reference::kind(&to) == "view"
            && super::reference::tab_of(&from) == super::reference::tab_of(&to);
        if s.exited() || (from != to && !listed) || !s.kept() {
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

/// The columns a key line fills at the minimum size: 80 less a column of
/// margin on each side.
const KEY_LINE_COLUMNS: usize = 78;

/// A key line's more item, `. more 2/3`, as the page it shows and the
/// number of pages; `None` for a key line with one page.
fn page_of(line: &str) -> Option<(usize, usize)> {
    let last = line.rsplit("  ").next()?;
    let (at, pages) = last.strip_prefix(". more ")?.split_once('/')?;
    Some((at.parse().ok()?, pages.parse().ok()?))
}

/// The labels a key line shows, each item read as its keys and the
/// longest label of `labels` that ends it, so that `save and quit` is not
/// read as `quit`. The more item is left out.
fn labels_of<'a>(line: &str, labels: &[&'a str]) -> Result<Vec<&'a str>, String> {
    line.split("  ")
        .filter(|i| !i.is_empty() && !i.starts_with(". more "))
        .map(|item| {
            labels
                .iter()
                .filter(|l| {
                    item.strip_suffix(**l)
                        .is_some_and(|keys| keys.ends_with(' ') && !keys.trim().is_empty())
                })
                .max_by_key(|l| l.len())
                .copied()
                .ok_or_else(|| {
                    format!(
                        "the key line shows {item:?}, which no supported row of the node labels"
                    )
                })
        })
        .collect()
}

/// The two tables, read once for the checks that need them outside a
/// [`Checks`].
fn tables() -> &'static (Vec<KeyRow>, Vec<tables::TreeNode>) {
    static TABLES: OnceLock<(Vec<KeyRow>, Vec<tables::TreeNode>)> = OnceLock::new();
    TABLES.get_or_init(|| (tables::keys(), tables::tree()))
}

/// The labels a key line shows in the node `id`: every label of the
/// node's supported rows but `.`'s, sorted, and those of the rows the key
/// table puts on a paged key line's first page.
fn node_labels(id: &str) -> (Vec<&'static str>, Vec<&'static str>) {
    let (rows, tree) = tables();
    let Some(node) = tree.iter().find(|n| n.id == id) else {
        return (Vec::new(), Vec::new());
    };
    let shown = || {
        rows.iter().filter(|r| {
            node.keys.contains(&r.id)
                && r.status == "supported"
                && r.label != "-"
                && r.keys != ["."]
        })
    };
    let mut all: Vec<&str> = shown().map(|r| r.label.as_str()).collect();
    let mut first: Vec<&str> = shown()
        .filter(|r| r.first_page)
        .map(|r| r.label.as_str())
        .collect();
    for labels in [&mut all, &mut first] {
        labels.sort_unstable();
        labels.dedup();
    }
    (all, first)
}
