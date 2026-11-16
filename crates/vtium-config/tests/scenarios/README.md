# Scenarios for vtium-config's TUI

These tests drive vtium-config's TUI by script and check how it responds after every key. They drive the state machine in `src/tui/model.rs` directly, with no terminal and no pty. A stand-in terminal answers font switches, and a stand-in desk keeps the draft in memory. So each key costs a couple of milliseconds, and the state after it can be read whole.

## The map

The explorer in `explore.rs` starts the TUI on the fixture of `src/tui/fixture.rs`, in each kind of terminal, at 80×25 and at 79×24. It reduces each state of the TUI to a short abstract state, which `state.rs` defines. The abstract state names the kind of terminal, the view, what the highlight is on, and what is open over the view. It also says whether changes wait to be saved and whether font sets wait to be installed. In every abstract state it reaches, the explorer tries every key of a fixed alphabet. This practice is called GUI ripping, and the result is a map of the TUI's states and the keys that move between them.

The map is committed here, one file for each kind of terminal: `map-vtium.tsv`, `map-ops-off.tsv` and `map-other.tsv`. Each line is a transition: the state before, the key, the state after, and what the key caused, such as `save`, `check`, `prepare` or `exit 0`. A key that leaves the state as it was and causes nothing has no line. The test fails when the explored map differs from the committed files, and it names the lines that went and came. So a change that adds, removes or redirects a transition shows up as a diff, which is reviewed like a golden, in a commit of its own.

To write the map again after a change that moves it on purpose, run:

```sh
VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --test scenarios every_key
```

## The checks after every key

After every key, `checks.rs` checks the rules the records set:

- Each key does what ADR-1403's key table, `docs/architecture/platform/ADR-1403-keys.tsv`, says it means in that kind of state. A planned or weighed row is checked in reverse: the check expects what vtium-config does today, so it fails once the work lands while the row still says planned.
- No key leaves the TUI with changes that are not saved, unless the person answered a question that named them.
- Tab and Shift-Tab change the view exactly when nothing is open over it.
- Mouse reporting is on exactly while the Flow view is in front, and off once the TUI leaves.
- When the TUI leaves a vtium console, the console draws the font it drew at the start.
- A question that throws the draft away counts the changes that are not saved.
- Every screen has as many rows as the terminal, and no row is wider than the terminal.
- Outside the Flow view, the key line shows every key it lists.
- From every state, some key sequence leaves the TUI.

After the walk, the test checks ADR-1201's views table, `docs/architecture/font/ADR-1201-views.tsv`. The table gives the tabs and their order, the view each one shows under, the actions each tab bar shows, and what each view writes into vtium.conf. A planned row is checked against its `now` column, which says what the TUI does today.

A failure names the scenario that shows it: the kind of terminal, the grid and the keys from the start, such as `vtium 80×25: Up Up Enter Down r q l`.

## The two runs

`every_key_in_every_state_keeps_the_rules` is the 0-switch run: every key in every state once. It runs with the crate's tests, one thread for each kind of terminal.

`every_pair_of_keys_through_questions_typing_and_leaving_keeps_the_rules` is the 1-switch run: every pair of keys where the first opens a question or starts typing, and `q` and Ctrl-C after every key. It takes minutes, so it runs at the end of a card that changes the TUI:

```sh
cargo test --release -p vtium-config --test scenarios -- --ignored
```
