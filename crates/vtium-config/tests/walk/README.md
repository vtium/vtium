# The walk of vtium-config's rebuilt TUI

These tests drive the rebuilt TUI by script and check how it responds after every key. The TUI here is vtium-config-tui's App with the six tab crates' tabs, as the binary builds them. The tests drive `App::event` directly, with no terminal and no pty. The old TUI's walk in `tests/scenarios/` stays beside this one until the binary switches, and then this folder takes its place.

`sim.rs` holds the stand-ins. A stand-in terminal answers the questions at start, each font switch and each query, as a vtium console, a vtium console with `allow-font-ops` off, or another terminal. A stand-in machine answers Save, the editor, Read devices again and the install of a system font. Each runs on one of vtium-config-core's two fixtures: the screen map's machine, or the large machine. The App cannot be cloned, because it holds its tabs as trait objects. So the walk reaches a state again by starting a new TUI and pressing the keys that reach it.

## The map

The explorer in `explore.rs` starts the TUI on the screen map's machine, in each kind of terminal, at 80×25 and at 79×24. It reduces each state to a short abstract state, which `state.rs` defines. The abstract state names the kind of terminal, the node of the interface tree, and the tab under it. It also names the open question, by the row its yes hands back, so a path through one question cannot borrow another's answer. It says whether changes wait to be saved and whether a step waits to be undone.

What a tab highlights is read from the screen, since the `Tab` trait does not say. The abstract state keeps the group of the highlighted item, the first word of the heading above it, such as `Font` of `Font sets`. A menu's state keeps its highlighted item's place. A few key sequences, the seeds in `explore.rs`, reach items whose group does not tell them apart, and the walk explores from them too.

In every abstract state it reaches, the explorer tries every key of a fixed alphabet. The alphabet holds every key the key table names, and `z` for any other letter. It also clicks the frame's places, turns the wheel, and shrinks the terminal below the minimum size and grows it back.

The map is committed here, one file for each kind of terminal: `map-vtium.tsv`, `map-ops-off.tsv` and `map-other.tsv`. Each line is a transition: the state before, the key, the state after, and what the key caused. `change` is a change to the configuration, `install` is Save's privileged step, and `exit 0` is leaving. A key that leaves the state as it was and causes nothing has no line. The test fails when the explored map differs from the committed files, and names the lines that went and came. To write the map again after a change that moves it on purpose, run:

```sh
VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --test walk every_key
```

## The checks after every key

After every key, `checks.rs` checks the rules the records set:

- Every row is the terminal's width, and the screen has as many rows as the terminal.
- The action bar shows each labelled row of the node, on the bar or behind `[.] More`. Undo and Redo show only while there is a step to take. Search shows only where the node names search fields.
- Each button the bar places is on the bottom row as `[key] label`, and `[.] More` shows exactly when buttons are left out.
- Mouse reporting is on on every screen while the TUI runs, and off once it leaves.
- When the TUI leaves, the draft holds every change, so a quit loses nothing, and a vtium console draws its start font again.
- Revert's question says how many changes it drops.
- The shell and the open tab agree on the node, so the navigation module looks each key up in the rows of the screen the person sees.
- A key whose rows only move or do nothing changes nothing the person made and does not leave. Undo and Redo change the configuration where there is a step.
- The node the TUI is in is a node of the interface tree.
- Each key goes where `reference.rs` predicts, for the keys the tables decide whatever a tab holds.

After the walk, the test checks three more things. Every button a bar offers changes something in its node, at once or after a key that moves or a letter typed. Every supported node of the tree is a state the walk reaches. From every state, some key sequence leaves the TUI.

A failure names the scenario that shows it: the kind of terminal, the grid and the keys from the start, such as `vtium 80×25: 3 Down Enter`.

## Known defects and nodes that wait

`KNOWN` in `checks.rs` lists the defects the walk finds in files it does not own, each with its reason. The walk stays green while each shows, and fails once one no longer does, so the entry comes out with the fix. `WAITING` lists the supported nodes the walk cannot reach yet, each with its reason. The walk fails once it reaches one, so that entry comes out too.

## The tables

`tables.rs` reads ADR-1403's interface tree and key table by its own code, with the overlay of rows and nodes that ADR-1403 D11 adds, under vtium-config-widgets' `contract/`. It reads the files, not the compiled contract the TUI reads. Once the tables carry the overlay and its files go, it reads the tables alone.

`reference.rs` predicts where a key takes the person wherever the tables alone decide it. Examples are Tab, the digits, the Alt letters, F10, Ctrl-S, `q`, Ctrl-C, and Esc out of a level. The exhaustive walk compares every transition with the prediction.

## The property walk

`run.rs` presses random runs of the keys the reference can predict, in each kind of terminal. After every key it compares the node, the tab in front and the exit. proptest shrinks a failing run and prints it. The number of cases is a constant. `run.proptest-regressions` keeps each seed found once, with its shrunk run of keys, and a test of its own presses each recorded run as written.

## The other tests

- `every_file_and_actions_item_is_routed` checks that every item of File and Actions, over every tab, reaches an action, and none is greyed out as not built yet.
- `the_searchable_tabs_and_the_tree_s_search_fields_agree` checks ADR-1206 D9 in both directions: a node with search fields belongs to a `Searchable` tab, and a `Searchable` tab searches a node with fields.
- `the_interface_tree_and_the_key_table_name_each_other` checks that the two tables and the overlay hold together.

## The screens

`screens.rs` draws each node the map reaches, by the fewest keys from the start, at 100×30 and at 80×25, on both machines. Each row must be the screen's width. At 80×25 the same keys must reach the same node, and the bar must keep every button of 100×30, on it or behind More. Without color, a cell set apart by color alone must keep an attribute.

Each node's four screens are a text snapshot under `snapshots/`, each followed by the grid of its attributes without color. The snapshots are goldens, provisional until the maintainer accepts the layout. To write them again, run:

```sh
VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --test walk every_screen
```

## The two runs

`every_key_in_every_state_keeps_the_rules` is the 0-switch run: every key in every state once, with every check after every key. It runs with the crate's tests.

`every_pair_of_keys_through_questions_typing_and_leaving_keeps_the_rules` is the 1-switch run. It tries every key after each transition into a question or into typing, and `q` and Ctrl-C after every transition. It needs a release build to run in seconds, so it runs at the end of a card that changes the TUI:

```sh
cargo test --release -p vtium-config --test walk -- --ignored
```

References:
- ADR-1206 D5: how the TUI is judged, the exhaustive walk, the property test and the screens' snapshots.
- ADR-1206 D9: the trait and the interface tree agree.
- ADR-1403 D11: the action bar, the menus, the mouse, quitting at once, and Revert's question.
- Decision 130 of 2026-11-27: the map is the measure of the keys each action takes.
