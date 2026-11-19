//! Each node of the interface tree as a text snapshot of the frame the
//! view draws into ratatui-core's TestBackend. The explorer finds the
//! shortest keys that reach each node in a vtium console at 80×25. The
//! test presses those keys, resizes the terminal to each grid the
//! snapshots cover, draws one frame, and compares the frame's text with
//! the node's snapshot under `snapshots/`.
//!
//! Every snapshot is provisional until the maintainer accepts the layout,
//! and `tests/screens/provisional.tsv` lists it. A provisional snapshot
//! judges nothing: when its frame moves, insta writes the new frame beside
//! it as a `.snap.new` file for review, and the test reports the move
//! without failing. A snapshot the file does not list fails when it moves.
//!
//! References:
//! - ADR-1206 D5, "The screens are text snapshots": the grids, the
//!   shortest keys, insta, and that the snapshots stay provisional until
//!   the maintainer accepts the layout.

use std::collections::BTreeMap;
use std::path::Path;

use ratatui_core::backend::TestBackend;
use ratatui_core::terminal::Terminal;
use vtium_config::tui::view;

use super::explore::{self, ALPHABET};
use super::sim::{Press, Sim, Term};

/// The grids each node is drawn at: the minimum, a common larger grid,
/// and one column and one row below the minimum, where the TUI shows only
/// the screen that asks for a larger terminal.
pub const GRIDS: [(usize, usize); 3] = [(80, 25), (100, 30), (79, 24)];

/// The press that the explorer's alphabet names `name`.
fn press_of(name: &str) -> Press {
    ALPHABET
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, p)| *p)
        .unwrap_or_else(|| panic!("no key named {name}"))
}

/// The snapshot names `provisional.tsv` lists.
fn provisional() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/screens/provisional.tsv");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split('\t').next())
        .map(str::to_owned)
        .collect()
}

/// The text of the frame the view draws for `sim`'s TUI into a
/// TestBackend of its size, one line a row with its trailing spaces cut.
fn frame(sim: &Sim) -> String {
    let size = sim.app.size;
    let cols = u16::try_from(size.cols).expect("a grid's columns");
    let rows = u16::try_from(size.rows).expect("a grid's rows");
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).expect("a TestBackend");
    terminal
        .draw(|f| view::render(&sim.app, f))
        .expect("a frame");
    view::as_text(&view::rows(terminal.backend().buffer()))
}

/// The snapshot's name for `node` at `cols` by `rows`, which insta also
/// writes into the file's name. The `node-` in front keeps it apart from
/// the screen goldens' names, which share `provisional.tsv`.
pub fn name(node: &str, cols: usize, rows: usize) -> String {
    format!("node-{}-{cols}x{rows}", node.replace('.', "_"))
}

/// Each node the explorer reaches in a vtium console, with the shortest
/// keys that reach it from the start at 80×25. The node the TUI has left
/// has no screen.
pub fn nodes() -> BTreeMap<String, Vec<&'static str>> {
    let map = explore::explore(Term::Vtium, |_, _, _| {});
    let mut first: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for (abs, (sim, path, (_, cols, rows))) in &map.reps {
        if sim.exited.is_some() || (*cols, *rows) != (80, 25) {
            continue;
        }
        let node = abs.node();
        match first.get(&node) {
            Some(p) if p.len() <= path.len() => {}
            _ => {
                first.insert(node, path.clone());
            }
        }
    }
    first
}

#[test]
fn each_node_s_frame_matches_its_snapshot() {
    let listed = provisional();
    let nodes = nodes();
    assert!(nodes.len() > 20, "only {} nodes reached", nodes.len());
    let mut moved = Vec::new();
    for (node, path) in &nodes {
        for (cols, rows) in GRIDS {
            let mut sim = Sim::start(Term::Vtium, 80, 25);
            for key in path {
                sim.press(press_of(key));
            }
            sim.press(Press::Resize(cols, rows));
            let text = frame(&sim);
            let name = name(node, cols, rows);
            let check = || {
                insta::with_settings!({
                    description => format!("{node} at {cols}×{rows}, keys: {}", path.join(" ")),
                    omit_expression => true,
                    prepend_module_to_snapshot => false,
                }, {
                    insta::assert_snapshot!(name.clone(), text.clone());
                });
            };
            if listed.contains(&name) {
                if std::panic::catch_unwind(check).is_err() {
                    moved.push(name);
                }
            } else {
                check();
            }
        }
    }
    if !moved.is_empty() {
        eprintln!(
            "{} provisional snapshots moved; they judge nothing: {}",
            moved.len(),
            moved.join(", ")
        );
    }
}
