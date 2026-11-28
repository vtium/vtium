//! The screens of the rebuilt TUI as text snapshots: each node the map
//! reaches, by the fewest keys from the start that the committed map of a
//! vtium console gives, drawn at 100×30 and at 80×25, on the screen map's
//! machine and on the large machine. Each snapshot holds the four
//! screens, each followed by the grid of the attributes its cells keep
//! without color. They are goldens, provisional until the maintainer
//! accepts the layout, and `VTIUM_CONFIG_BLESS=1 cargo test -p
//! vtium-config --test walk screens` writes them again under
//! `tests/walk/snapshots/`.
//!
//! The test also checks each screen: every row is the screen's width;
//! at 80×25 the same keys reach the same node, and every button of the
//! action bar at 100×30 is on the bar or behind More, so the screen is
//! cramped but still works; and drawn without color, as under `NO_COLOR`
//! and on a monochrome console, a cell set apart by color alone keeps an
//! attribute, as a highlight keeps reverse video.
//!
//! References:
//! - ADR-1206 D10, "Designed for 100×30, usable at 80×25" and "Sixteen
//!   colors, `NO_COLOR` and a monochrome console".
//! - ADR-1206 D5, "The screens are text snapshots".

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use ratatui_core::buffer::Buffer;
use ratatui_core::style::{Color, Modifier};

use super::explore;
use super::sim::{self, Machine, Sim, Term};

/// The sizes every screen is checked at: the one vtium-config is designed
/// for, and the minimum.
const SIZES: [(u16, u16); 2] = [(100, 30), (80, 25)];

/// The fewest keys from the start at 80×25 to each node, by the committed
/// map of a vtium console, read as a graph of its lines. The keys that
/// resize the terminal are left out, so each path works at both sizes.
fn shortest() -> BTreeMap<String, Vec<String>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/walk/map-vtium.tsv");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut next: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("from\t"))
    {
        let mut cols = line.split('\t');
        let (Some(from), Some(key), Some(to)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        if matches!(key, "Shrink" | "Grow") || from.contains("below 80×25") {
            continue;
        }
        next.entry(from).or_default().push((key, to));
    }
    let start = super::state::of(&Sim::start(Term::Vtium, Machine::Map, 80, 25)).to_string();
    let mut keys: BTreeMap<&str, Vec<String>> = BTreeMap::from([(start.as_str(), Vec::new())]);
    let mut queue = VecDeque::from([start.as_str()]);
    while let Some(state) = queue.pop_front() {
        let here = keys.get(state).cloned().unwrap_or_default();
        for (key, to) in next.get(state).into_iter().flatten() {
            if !keys.contains_key(to) {
                let mut k = here.clone();
                k.push((*key).to_owned());
                keys.insert(to, k);
                queue.push_back(to);
            }
        }
    }
    // a state reads `vtium NODE(…) +unsaved`; the node is its second word
    let mut by_node: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (state, k) in keys {
        let node = state
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .split('(')
            .next()
            .unwrap_or_default()
            .to_owned();
        if node.is_empty() || node == "left" {
            continue;
        }
        let shorter = by_node.get(&node).is_none_or(|have| k.len() < have.len());
        if shorter {
            by_node.insert(node, k);
        }
    }
    by_node
}

/// The TUI on `machine` at `size`, drawn in color or not, after `keys`.
fn reach(machine: Machine, size: (u16, u16), color: bool, keys: &[String]) -> Sim {
    let mut sim = Sim::start_in(Term::Vtium, machine, size.0, size.1, color);
    for k in keys {
        sim.press(explore::press_of(k));
    }
    sim
}

/// A grid of the attributes each cell of `buf` keeps without color: `.`
/// none, `b` bold, `u` underline, `r` reverse, `B` bold and underline, `R`
/// bold and reverse, `U` underline and reverse, `X` all three.
fn attributes(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.y..a.bottom())
        .map(|y| {
            (a.x..a.right())
                .map(|x| {
                    let m = buf.cell((x, y)).map_or(Modifier::empty(), |c| c.modifier);
                    match (
                        m.contains(Modifier::BOLD),
                        m.contains(Modifier::UNDERLINED),
                        m.contains(Modifier::REVERSED),
                    ) {
                        (false, false, false) => '.',
                        (true, false, false) => 'b',
                        (false, true, false) => 'u',
                        (false, false, true) => 'r',
                        (true, true, false) => 'B',
                        (true, false, true) => 'R',
                        (false, true, true) => 'U',
                        (true, true, true) => 'X',
                    }
                })
                .collect()
        })
        .collect()
}

/// Whether `symbol` draws a shape that reads without color: a line of a
/// box or a rule, an arrow, an ellipsis, or a mark such as ● or ○, whose
/// glyph says what it is.
fn shape(symbol: &str) -> bool {
    symbol.chars().all(|c| {
        matches!(c, '\u{2190}'..='\u{21ff}' | '\u{2500}'..='\u{25ff}' | '…' | '›' | '‹' | '✓' | '✗' | '·')
    })
}

/// The screen drawn without color has the text of the screen in color and
/// no color, and every cell the color screen sets apart by color alone
/// keeps an attribute or a shape that reads without it: a cell with a
/// background color, such as a highlight, keeps an attribute, such as
/// reverse video, and a colored cell that is no letter, digit or shape,
/// keeps one too.
fn marks_survive_without_color(color: &Buffer, mono: &Buffer) -> Result<(), String> {
    let a = color.area;
    for y in a.y..a.bottom() {
        for x in a.x..a.right() {
            let (Some(c), Some(m)) = (color.cell((x, y)), mono.cell((x, y))) else {
                continue;
            };
            if c.symbol() != m.symbol() {
                return Err(format!("the text differs without color at {x},{y}"));
            }
            if m.fg != Color::Reset || m.bg != Color::Reset {
                return Err(format!("{x},{y} has color under NO_COLOR"));
            }
            let plain = m.modifier.is_empty();
            if c.bg != Color::Reset && plain {
                return Err(format!(
                    "{:?} at {x},{y} is set apart by its background color alone",
                    c.symbol()
                ));
            }
            let glyph = c.symbol().trim();
            let reads = glyph.is_empty()
                || glyph.chars().any(char::is_alphanumeric)
                || shape(glyph)
                || glyph.chars().all(|ch| ch.is_ascii_punctuation());
            if c.fg != Color::Reset && plain && !reads {
                return Err(format!("{glyph:?} at {x},{y} is marked by color alone"));
            }
        }
    }
    Ok(())
}

/// The ids of the rows the action bar offers, on the bar or behind More.
fn bar_ids(sim: &Sim) -> BTreeSet<String> {
    vtium_config_tui::view::layout(&sim.app)
        .bar
        .buttons
        .iter()
        .filter_map(|b| b.action.map(str::to_owned))
        .collect()
}

fn snapshot_path(node: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/walk/snapshots")
        .join(format!("{node}.txt"))
}

/// Every node the map reaches, drawn at both sizes on both machines, each
/// screen checked and kept as a snapshot.
#[test]
fn every_screen_fits_works_at_80x25_and_reads_without_color() {
    let bless = std::env::var_os("VTIUM_CONFIG_BLESS").is_some();
    let nodes: Vec<(String, Vec<String>)> = shortest().into_iter().collect();
    assert!(nodes.len() > 30, "the map reaches {} nodes", nodes.len());
    let found = super::par::map(&nodes, |(node, keys)| {
        let mut failures = Vec::new();
        let mut text = String::new();
        let scenario = |machine: Machine, (c, r): (u16, u16)| {
            format!(
                "{} {c}×{r} on the {} machine: {}",
                node,
                machine.name(),
                keys.join(" ")
            )
        };
        for machine in Machine::BOTH {
            let wide = reach(machine, (100, 30), true, keys);
            for size in SIZES {
                let sim = reach(machine, size, true, keys);
                let mono = reach(machine, size, false, keys);
                let (color_buf, mono_buf) = (sim.draw(), mono.draw());
                let at = scenario(machine, size);
                for (y, row) in sim::rows(&color_buf).iter().enumerate() {
                    if sim::width(row) != usize::from(size.0) {
                        failures.push(format!("{at}: row {y} is not the screen's width: {row:?}"));
                    }
                }
                if let Err(e) = marks_survive_without_color(&color_buf, &mono_buf) {
                    failures.push(format!("{at}: {e}"));
                }
                // More and a click land on what the width puts there, so
                // their keys reach other screens at other widths
                let by_width = keys.iter().any(|k| k == "." || k.starts_with("Click"));
                if size == (80, 25) && !by_width && sim.exited.is_none() && wide.exited.is_none() {
                    if sim.app.node() != wide.app.node() {
                        failures.push(format!(
                            "{at}: the keys reach {} at 80×25 and {} at 100×30",
                            sim.app.node(),
                            wide.app.node()
                        ));
                    } else {
                        let lost: Vec<String> =
                            bar_ids(&wide).difference(&bar_ids(&sim)).cloned().collect();
                        if !lost.is_empty() {
                            failures.push(format!(
                                "{at}: the action bar at 80×25 loses {lost:?}, neither on it nor behind More"
                            ));
                        }
                    }
                }
                text.push_str(&format!(
                    "== {} machine, {}×{}: {} (keys: {})\n",
                    machine.name(),
                    size.0,
                    size.1,
                    sim.app.node(),
                    keys.join(" ")
                ));
                text.push_str(&sim::rows(&color_buf).join("\n"));
                text.push_str("\n-- attributes without color --\n");
                text.push_str(&attributes(&mono_buf).join("\n"));
                text.push('\n');
            }
        }
        let path = snapshot_path(node);
        if bless {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(&path, &text) {
                failures.push(format!("{}: {e}", path.display()));
            }
        } else if std::fs::read_to_string(&path).unwrap_or_default() != text {
            failures.push(format!(
                "the snapshot {} moved; with VTIUM_CONFIG_BLESS=1 the test writes it again",
                path.display()
            ));
        }
        failures
    });
    let failures = super::checks::expected(found.into_iter().flatten().collect());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
