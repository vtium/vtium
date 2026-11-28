//! The checks after every key: the rules the records set, judged on the
//! TUI before and after one transition of the walk. tests/scenarios/
//! README.md lists the rules the old TUI's walk checks; this walk keeps
//! them, with the rules ADR-1403 D11 changes:
//!
//! - The action bar takes the key line's place. Every labelled row of the
//!   node is a button on the bar or behind `[.] More`, and every button
//!   the bar shows is on the screen as `[key] label`.
//! - Mouse reporting is on on every screen while the TUI runs, and off
//!   once it leaves.
//! - A quit never loses a change, because the draft holds every change:
//!   when the TUI leaves, the desk's draft is the configuration.
//! - Revert's question counts the changes it drops.
//!
//! References:
//! - ADR-1403 D11, "The action bar", "The mouse", "Quitting, Ctrl-C and
//!   the signals" and "Revert to saved".
//! - ADR-1403 D7, "The contract": the scenario test proves that the code
//!   does what the tables say.
//! - ADR-1201 D4: the console gets its start font back on leaving.

use std::collections::BTreeSet;

use vtium_config_tui::nav::menus::Which;

use super::explore::Step;
use super::reference;
use super::sim::{self, Sim, Term};
use super::state::StateKind;
use super::tables::{self, Tables};

/// What the checks read of the TUI before a key, taken once for each
/// state of the walk.
#[derive(Clone, Debug)]
pub struct Before {
    pub screen: Vec<String>,
    /// A step waits to be redone.
    pub redo: bool,
}

impl Before {
    pub fn of(sim: &Sim) -> Before {
        Before {
            screen: sim.screen(),
            redo: sim.app.shared().documents.history().to_redo().is_some(),
        }
    }
}

/// The keys that move inside a list, a view or a scrolled text.
pub const MOVING: [&str; 8] = ["Up", "Down", "Left", "Right", "Home", "End", "PgUp", "PgDn"];

/// Every check of one transition: `step`, from a TUI that `before`
/// describes, to `after`. Each failure names the scenario that shows it.
pub fn see(step: &Step, before: &Before, after: &Sim, rows: &[String]) -> Vec<String> {
    let t = tables::tables();
    let mut out = Vec::new();
    let mut fail = |what: String| out.push(format!("{}: {what}", step.scenario()));
    if !(step.from == step.to && rows == before.screen.as_slice())
        && let Err(e) = rows_fit(after, rows)
    {
        fail(e);
    }
    if let Err(e) = mouse(after) {
        fail(e);
    }
    if let Err(e) = left_well(after) {
        fail(e);
    }
    // a key that leaves the state and the screen as they were has nothing
    // new to show
    let same = step.from == step.to && rows == before.screen.as_slice();
    if after.exited.is_none() {
        // the bar of a node the shell and the tab disagree on is the
        // tab's, so the disagreement is the one failure
        match one_node(after) {
            Err(e) => fail(e),
            Ok(()) if !same => {
                if let Err(e) = bar(t, after, rows) {
                    fail(e);
                }
            }
            Ok(()) => {}
        }
        if let Err(e) = revert_counts(after) {
            fail(e);
        }
        if t.node(after.app.node()).is_none() {
            fail(format!(
                "the TUI is in {}, which is no node of the interface tree",
                after.app.node()
            ));
        }
    }
    if let Err(e) = keeps_what_it_should(t, step, before) {
        fail(e);
    }
    if let Err(e) = as_the_tables_say(step) {
        fail(e);
    }
    out
}

/// Every row of the screen is the terminal's width, and the screen has as
/// many rows as the terminal.
fn rows_fit(sim: &Sim, rows: &[String]) -> Result<(), String> {
    let size = sim.app.size();
    if rows.len() != usize::from(size.rows) {
        return Err(format!(
            "the screen has {} rows on a terminal of {}",
            rows.len(),
            size.rows
        ));
    }
    match rows
        .iter()
        .enumerate()
        .find(|(_, r)| sim::width(r) != usize::from(size.cols))
    {
        Some((y, r)) => Err(format!("row {y} is not the terminal's width: {r:?}")),
        None => Ok(()),
    }
}

/// Mouse reporting is on on every screen while the TUI runs, and off once
/// it has left.
fn mouse(sim: &Sim) -> Result<(), String> {
    match (sim.exited, sim.mouse) {
        (None, false) => Err("mouse reporting is off while the TUI runs".to_owned()),
        (Some(_), true) => Err("mouse reporting is on after the TUI left".to_owned()),
        _ => Ok(()),
    }
}

/// The shell and the open tab agree on where the person is: with nothing
/// over the tab but a menu, the level the shell's navigation module has open at the
/// top of the tab is the node the tab says it is on. Where they differ,
/// the module looks a key up in one node's rows while the tab acts as if
/// in another, as `q` quitting while a name is typed.
///
/// References:
/// - ADR-1206 D3: one navigation module owns where the person is.
fn one_node(sim: &Sim) -> Result<(), String> {
    let app = &sim.app;
    let nav = app.nav();
    if app.size().too_small()
        || nav.question().is_some()
        || nav.search().is_some()
        || !nav.sheets().is_empty()
        || app.live().trial.is_some()
    {
        return Ok(());
    }
    let shell = nav.tab_node(app.tabs());
    let Some(tab) = app.tabs().get(nav.tab()) else {
        return Ok(());
    };
    if tab.node() != shell {
        return Err(format!(
            "the {} tab says it is on {}, and the shell has {shell} open, whose rows it looks each key up in",
            tab.name(),
            tab.node()
        ));
    }
    Ok(())
}

/// When the TUI has left, the draft holds every change the configuration
/// had, so the quit lost nothing, and a vtium console draws the font it
/// drew at the start.
fn left_well(sim: &Sim) -> Result<(), String> {
    if sim.exited.is_none() {
        return Ok(());
    }
    let document = sim.app.shared().document();
    let content = document.content();
    if document.count() > 0 && sim.draft().as_ref() != Some(content) {
        return Err(format!(
            "the TUI left, and the desk's draft is not the configuration: {:?}",
            sim.draft().map(|c| c.text)
        ));
    }
    if sim.term == Term::Vtium && sim.console_font != sim::START_FONT {
        return Err(format!(
            "the TUI left a vtium console drawing {}, not its start font {}",
            sim.console_font,
            sim::START_FONT
        ));
    }
    Ok(())
}

/// The node whose buttons the bar shows: the node under More's menu while
/// it is open, since More lists what that bar has no room for.
fn bar_node(sim: &Sim) -> &'static str {
    let app = &sim.app;
    match app.nav().menu() {
        Some(m) if m.which == Which::More => {
            app.nav().under_menu(app.tabs(), app.size().too_small())
        }
        _ => app.node(),
    }
}

/// The action bar of the screen: every labelled row of the node, by the
/// walk's own reading of the tables, is a button on the bar or behind
/// More, Undo and Redo only while there is a step to take; every button it
/// shows is on the bottom row as `[key] label`; and `[.] More` shows
/// exactly when buttons are left out. A sheet over a tab names its own
/// buttons, and the screen below the minimum size has no bar.
///
/// References:
/// - ADR-1403 D11, "The action bar": one row of buttons, each naming its
///   key, the lowest left out first behind `[.] More`.
/// - ADR-1403 D11, "Search, on ?": only a screen whose node declares the
///   search shows `[?] Search`.
fn bar(t: &Tables, sim: &Sim, rows: &[String]) -> Result<(), String> {
    let app = &sim.app;
    if app.size().too_small() || app.node() == "any.trial" {
        return Ok(());
    }
    let node = bar_node(sim);
    let sheet = app.nav().sheets().last().is_some_and(|s| s.node() == node);
    let layout = vtium_config_tui::view::layout(app);
    // one button for each action: rows of one label share it, as Ctrl+C's
    // Quit shares q's
    let labels: BTreeSet<String> = layout
        .bar
        .buttons
        .iter()
        .map(|b| b.label.to_lowercase())
        .collect();
    let ids: BTreeSet<&str> = layout.bar.buttons.iter().filter_map(|b| b.action).collect();
    let history = app.shared().documents.history();
    if !sheet && let Some(n) = t.node(node) {
        for row in t
            .rows_of(n)
            .filter(|r| r.on_bar() && !r.keys.contains(&"."))
        {
            // only a screen whose node names search fields shows Search
            let search = matches!(row.id.as_str(), "question.view" | "question.layer");
            let step = if search {
                !n.search.is_empty()
            } else if row.id.starts_with("ctrl-z.") {
                history.to_undo().is_some()
            } else if row.id.starts_with("ctrl-y.") {
                history.to_redo().is_some()
            } else {
                !CONDITIONAL.contains(&row.id.as_str())
            };
            // a tab may name the button its own way, as Deactivate on an
            // active font, so the button's row counts as well as its label
            let shown = ids.contains(row.id.as_str()) || labels.contains(&row.label.to_lowercase());
            if step && !shown {
                return Err(format!(
                    "the action bar of {node} leaves out the labelled row {} ([{}] {}), neither on the bar nor behind [.] More",
                    row.id, row.key, row.label
                ));
            }
        }
    }
    let places = layout.frame.places();
    let bottom = rows.last().cloned().unwrap_or_default();
    for (i, _) in &places.buttons {
        let Some(b) = layout.bar.buttons.get(*i) else {
            continue;
        };
        let shown = format!("[{}] {}", b.key, b.label);
        if !bottom.contains(&shown) {
            return Err(format!(
                "the action bar of {node} places {shown:?}, and the bottom row does not show it: {bottom:?}"
            ));
        }
    }
    if places.hidden.is_empty() == bottom.contains("[.] More") {
        return Err(format!(
            "the action bar of {node} leaves out {} buttons, and the bottom row reads {bottom:?}",
            places.hidden.len()
        ));
    }
    Ok(())
}

/// The labelled rows whose button the records show only while it can act:
/// Activate missing, while the configuration uses a font that is
/// installed and trusted but not active, and Install as a system font, on
/// a font from the person's own folders.
///
/// References:
/// - ADR-1403 D11, "Fonts the configuration uses but has not activated":
///   the Fonts tab's bar shows `[A] Activate missing (3)` while there is
///   such a font.
/// - ADR-1403's interface tree, `fonts.font`: Enter on a font from your own
///   folders asks before it installs it as a system font.
const CONDITIONAL: [&str; 3] = [
    "shift-a.view.fonts",
    "shift-a.layer.look",
    "enter.layer.font",
];

/// The rows whose action is Revert to saved, which ask before they drop
/// every change.
fn reverts(t: &Tables) -> Vec<&str> {
    t.rows
        .iter()
        .filter(|r| r.means.contains("Revert to saved") || r.label.starts_with("Revert"))
        .map(|r| r.id.as_str())
        .collect()
}

/// Revert's question names how many changes it drops.
///
/// References:
/// - ADR-1403 D11, "Revert to saved": the question counts what is lost.
fn revert_counts(sim: &Sim) -> Result<(), String> {
    let Some(q) = sim.app.nav().question() else {
        return Ok(());
    };
    // the yes of Revert's question names it, as `y.question.revert`
    let revert = q.then.row.contains("revert")
        || reverts(tables::tables()).contains(&q.then.row)
        || q.text.to_lowercase().contains("revert");
    let n = sim.app.shared().document().count();
    if revert && !q.text.contains(&n.to_string()) {
        return Err(format!(
            "Revert's question does not say how many changes it drops ({n}): {:?}",
            q.text
        ));
    }
    Ok(())
}

/// A key whose rows in the node only move, or say they do nothing,
/// changes nothing the person made and does not leave; Undo changes the
/// configuration back where there is a step to undo, and Redo where there
/// is one to redo, on a tab or a level; neither changes it while a value
/// is typed or a menu is open.
fn keeps_what_it_should(t: &Tables, step: &Step, before: &Before) -> Result<(), String> {
    if matches!(step.from.kind, StateKind::Left) {
        return Ok(());
    }
    let changed = step.did.effects.iter().any(|e| e == "change");
    let Some(node) = t.node(step.from.node) else {
        return Ok(());
    };
    let rows = t.rows_for(node, step.key);
    let moves = ["Moves", "Goes to the", "Scrolls", "Move ", "Does nothing"];
    if !rows.is_empty() && rows.iter().all(|r| moves.iter().any(|m| r.means(m))) {
        if changed {
            return Err(format!(
                "{} in {} changed the configuration, and its rows only move or do nothing",
                step.key, step.from.node
            ));
        }
        if step.did.exit.is_some() {
            return Err(format!(
                "{} in {} left the TUI, and its rows only move or do nothing",
                step.key, step.from.node
            ));
        }
    }
    let acting = matches!(step.from.kind, StateKind::View | StateKind::Layer)
        && !step.from.node.starts_with("any.save");
    let undo = step.key == "Ctrl-Z" && acting && step.from.undo;
    let redo = step.key == "Ctrl-Y" && acting && before.redo;
    if (undo || redo) && !changed {
        return Err(format!(
            "{} in {} left the configuration as it was, with a step to take",
            step.key, step.from.node
        ));
    }
    Ok(())
}

/// Where the key took the TUI, against where the reading of the tables in
/// `reference.rs` says it goes, for the keys whose effect the tables
/// decide whatever a tab holds.
fn as_the_tables_say(step: &Step) -> Result<(), String> {
    if step.from.kind == StateKind::Left {
        return Ok(());
    }
    let Some(want) = reference::expect(step.from.node, step.key) else {
        return Ok(());
    };
    let mut agrees = reference::agrees(&want, step.to.node, step.did.exit);
    // a key that keeps the TUI where it was keeps the tab under it too
    if agrees.is_ok()
        && !step.from.small
        && !step.to.small
        && matches!(&want, reference::Expect::At(id) if id == step.from.node)
        && step.from.tab.is_some()
        && step.to.tab != step.from.tab
    {
        agrees = Err(format!(
            "the tables keep the TUI at {}, and the tab under it went from {:?} to {:?}",
            step.to.node, step.from.tab, step.to.tab
        ));
    }
    agrees.map_err(|e| format!("from {}: {e}", step.from.node))
}

/// Whether a key changed anything a person can see or that the TUI did:
/// an effect, leaving, the abstract state, or the screen.
pub fn acted(before: &[String], after: &[String], step: &Step) -> bool {
    !step.did.effects.is_empty()
        || step.did.exit.is_some()
        || step.from != step.to
        || after != before
}

/// The nodes no state of the walk reaches yet, each with the reason.
const WAITING: [(&str, &str); 12] = [
    ("flow.detail.typing", FLOW_LEVELS),
    ("flow.card.typing", FLOW_LEVELS),
    ("flow.card.list", FLOW_LEVELS),
    ("flow.maker.picker", FLOW_LEVELS),
    ("flow.maker.picker.typing", FLOW_LEVELS),
    (
        "any.question.deactivate",
        "the screen map's machine has no resource that a look uses and that can be deactivated",
    ),
    ("any.question.system-font", ASKED_AS_CONFIRM),
    ("any.question.window-ops", ASKED_AS_CONFIRM),
    ("any.save.changed", CHANGED_ON_DISK),
    ("any.save.settle", CHANGED_ON_DISK),
    (
        "any.editor",
        "the TUI is there only while the loop has stepped aside for the editor, which the stand-in loop answers at once",
    ),
    (
        "any.question.edit-again",
        "the stand-in editor's copy passes the check, so nothing asks to edit again",
    ),
];

/// Why the Flow tab's typed and listed levels wait.
const FLOW_LEVELS: &str = "the Flow tab opens it without a request to the shell, so the shell never has it open (see KNOWN)";

/// Why two questions of their own wait.
const ASKED_AS_CONFIRM: &str = "vtium-config-core's Request::Ask names no node, so the shell asks every question as any.question.confirm, this one included";

/// Why Save's screens for a file changed on disk wait.
const CHANGED_ON_DISK: &str = "the stand-in machine keeps the installed file the configuration was made from, so Save never finds it changed on disk";

/// Every supported node of the tree is a state the walk reaches, apart
/// from the two that group others, `tui` and `any`, and the nodes that
/// wait; and no node that waits is reached, so its entry comes out once
/// the walk reaches it.
pub fn unreached(reached: &BTreeSet<&str>) -> Vec<String> {
    let waits = |id: &str| WAITING.iter().any(|(w, _)| *w == id);
    let mut out: Vec<String> = tables::tables()
        .tree
        .iter()
        .filter(|n| n.supported && !matches!(n.id.as_str(), "tui" | "any"))
        .filter(|n| !reached.contains(n.id.as_str()) && !waits(&n.id))
        .map(|n| format!("no state of the walk is in the supported node {}", n.id))
        .collect();
    for (id, why) in WAITING {
        if reached.contains(id) {
            out.push(format!(
                "the walk reaches {id}, which WAITING says it cannot ({why}): take it out"
            ));
        }
    }
    out
}

/// The defects the walk finds outside its own files, which wait for the
/// card that owns their files: each by a part of its failure's text and
/// the reason it stays. The walk stays green while each shows, and fails
/// once one no longer does, so the entry comes out with the fix.
const KNOWN: [(&str, &str); 4] = [
    (
        "the TUI is in any.new, which is no node of the interface tree",
        "File › New…'s sheet in vtium-config-tui's src/sheets/new.rs names the node any.new, which the tree and the overlay of ADR-1403 D11 lack",
    ),
    (
        "the Flow tab says it is on",
        "vtium-config-tab-flow moves between a detail and its choices, and into a label or a name being typed, with no Request::Open or Close, so the shell looks each key up in the wrong node and q quits while a name is typed",
    ),
    (
        "கொ",
        "the Settings key cluster-width's help holds கொ, which the buffer draws in two cells and unicode-width measures as one, so F1's sheet and Find's match of it read a column short of the screen",
    ),
    (
        "offers the row question.layer",
        "the action bar shows [?] Search on levels whose node names no search fields, and on list boxes with no more choices than they show, where ? does nothing; ADR-1403 D11 says only a screen that offers the search shows it",
    ),
];

/// `failures` without those that [`KNOWN`] expects, for a run that shows
/// only some of them, as the 1-switch run.
pub fn expected(failures: Vec<String>) -> Vec<String> {
    failures
        .into_iter()
        .filter(|f| !KNOWN.iter().any(|(needle, _)| f.contains(needle)))
        .collect()
}

/// `failures` without those that [`KNOWN`] expects, and a failure for
/// each entry of it that no failure shows any more.
pub fn known(failures: Vec<String>) -> Vec<String> {
    let mut seen = [false; KNOWN.len()];
    let mut out: Vec<String> = failures
        .into_iter()
        .filter(|f| {
            let hit = KNOWN.iter().position(|(needle, _)| f.contains(needle));
            if let Some(i) = hit
                && let Some(s) = seen.get_mut(i)
            {
                *s = true;
            }
            hit.is_none()
        })
        .collect();
    for ((needle, why), shown) in KNOWN.iter().zip(seen) {
        if !shown {
            out.push(format!(
                "the known defect {needle:?} ({why}) no longer shows: take it out of KNOWN"
            ));
        }
    }
    out
}
