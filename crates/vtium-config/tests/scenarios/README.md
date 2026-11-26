# Scenarios for vtium-config's TUI

These tests drive vtium-config's TUI by script and check how it responds after every key. They drive the state machine in `src/tui/model.rs` directly, with no terminal and no pty. A stand-in terminal answers font switches, and a stand-in desk keeps the draft in memory. The stand-in desk's draft ends with comment lines, so the file view has more lines than its window. The desk keeps two installed files, so Restore's list has two items. So each key costs a couple of milliseconds, and the state after it can be read whole.

## The map

The explorer in `explore.rs` starts the TUI on the fixture of `src/tui/fixture.rs`, in each kind of terminal, at 80×25 and at 79×24. It reduces each state of the TUI to a short abstract state, which `state.rs` defines. The abstract state names the kind of terminal, the view, what the highlight is on, and what is open over the view. It also says whether changes wait to be saved and whether font sets wait to be installed. In every abstract state it reaches, the explorer tries every key of a fixed alphabet. It also shrinks the terminal below the minimum size and grows it back, so it reaches that screen with changes waiting. This practice is called GUI ripping, and the result is a map of the TUI's states and the keys that move between them.

The map is committed here, one file for each kind of terminal: `map-vtium.tsv`, `map-ops-off.tsv` and `map-other.tsv`. Each line is a transition: the state before, the key, the state after, and what the key caused, such as `save`, `check`, `prepare` or `exit 0`. A key that leaves the state as it was and causes nothing has no line. The test fails when the explored map differs from the committed files, and it names the lines that went and came. So a change that adds, removes or redirects a transition shows up as a diff, which is reviewed like a golden, in a commit of its own.

To write the map again after a change that moves it on purpose, run:

```sh
VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --test scenarios every_key
```

## The checks after every key

After every key, `checks.rs` checks the rules the records set:

- Each key does what ADR-1403's key table, `docs/architecture/platform/ADR-1403-keys.tsv`, says it means in that kind of state. The check runs in each node of the tree that lists the row, and the walk must reach the row in every such node.
- A row that `row_checks` in `checks.rs` names has a check of its own, which tests what the row means. A row it does not name gets a check by the kind of key, which proves less. A key that moves must stay in its node and change nothing the person made. A letter typed into a value must stay in the typing, and any other key must not leave the TUI.
- When rows of one node share a key, each row says which states it applies in, and it counts only there. For example, Enter on a terminal's card sets the line under the cursor today, and each planned list that Enter is to open belongs to one kind of line.
- The abstract state does not tell a terminal card's lines apart. So after the walk, the test moves down each line of each card it reached, NEW PATH's card included, and presses Enter, Left and Right on it. The map records none of those keys.
- A planned or weighed row with a check of today is checked in reverse. The check expects what vtium-config does today, so it fails once the work lands while the row still says planned. A planned or weighed row with no check of today only records that the walk reached it.
- No key leaves the TUI with changes that are not saved, unless the person answered a question that named them.
- Tab and Shift-Tab change the view exactly when nothing is open over it.
- Mouse reporting is on exactly while the Flow view is in front, and off once the TUI leaves.
- When the TUI leaves a vtium console, the console draws the font it drew at the start.
- A question that throws the draft away counts the changes that are not saved.
- Every screen has as many rows as the terminal, and no row is wider than the terminal.
- The key line takes one row, and the screen shows every key it lists. Its items are labels that the contract gives the node the TUI is in: all of them on a key line of one page. A paged key line ends with its more item, such as `. more 2/5`, and shows its first page after a key that moves to another node.
- `.` shows the next page of a paged key line, and the first page after the last. Pressed once for each page, it goes round every page, and the pages together show every label of the node. The first page shows the labels of the rows that the key table's `page` column puts there.
- Every key a key line names changes something in its node: the screen, its colors included, the state, or an effect. After the walk, the test presses each such key in every state of the node it reached. Where the key changes nothing there, the test tries it again after one other key, and then after two keys that move. So a highlight at the edge of a list moves away first. A key line that offers a key that does nothing fails the test.
- After the walk, the test also presses every key from the second page of each paged key line. A key that moves to another node must show that node's first page.
- From every state, some key sequence leaves the TUI.

After the walk, the test checks ADR-1201's views table, `docs/architecture/font/ADR-1201-views.tsv`. The table gives the tabs and their order, the view each one shows under, the actions each tab bar shows, and what each view writes. A view writes parts of vtium.conf, the font sets that wait for an install, and the draft's changes to the active resources, which the table calls `batch`. A planned row is checked against its `now` column, which says what the TUI does today.

A failure names the scenario that shows it: the kind of terminal, the grid and the keys from the start, such as `vtium 80×25: Up Up Enter Down r q l`.

## Where each key goes

`reference.rs` reads the interface tree and the key table by its own code, apart from the TUI's navigation module. It predicts where a key takes the person wherever the tables alone decide that. Examples are Tab, the digits, `q`, Ctrl-C, Esc out of a layer, and a key whose row says it does nothing. Two walks judge the TUI against it.

- The exhaustive walk is the map's walk above. After every key in every state, it compares the node the TUI is in, and the tab under it, with the prediction. A failure names the shortest keys from the start that take the transition.
- The property test in `walk.rs` presses random sequences of the keys the reference can predict, in each kind of terminal. After every key it compares the node, the highlighted tab and the exit. proptest shrinks a failing sequence to a short one and prints it. The number of cases is a constant, and `walk.proptest-regressions` keeps the seed of each case found once, which proptest runs first in every later run. A seed replays its case only while the two tables and the explorer's alphabet stay as they are, because proptest draws each key again from the keys the reference offers. So each line of the file also records the shrunk sequence of keys, and a test of its own presses each recorded sequence as it is written. That test fails on a line that does not read as a seed, and on a key the tables no longer decide.

## The two runs

`every_key_in_every_state_keeps_the_rules` is the 0-switch run: every key in every state once. It runs with the crate's tests, one thread for each kind of terminal.

`every_pair_of_keys_through_questions_typing_and_leaving_keeps_the_rules` is the 1-switch run: every pair of keys where the first opens a question or starts typing, and `q` and Ctrl-C after every key. It needs a release build to run in seconds, so it runs at the end of a card that changes the TUI:

```sh
cargo test --release -p vtium-config --test scenarios -- --ignored
```

## Use cases

The walk checks each key on its own, and a use case follows one task a person does from its first key to its end. Each use case is a file in `cases/`, named for the task, such as `a-make-a-look-and-install-it.case`. `usecase/mod.rs` runs every file in the folder, with the test `every_use_case_does_what_the_person_asked`.

A case runs on the real desk, vtium-config's own store, so Save writes the draft's file and Install installs it. Each case has a machine of its own, a folder under the test target's temporary directory. The folder stands in for `/etc/vtium`, the frits' directory, the font files and the person's home. The install's privileged step runs vtium-config's own binary as `apply DIR` as the user, into that folder, so no case needs sudo or touches the machine's own files. The font helper is vtium-config's binary too, run as `serve`, so the Looks tab learns what each font set lacks the way it does for a person.

On the other side of the TUI is a stand-in console, in `usecase/console.rs`, which refuses what vtium refuses. When an OSC 50 switch names a font set, the console loads the set with the font helper's own code, behind the gate of batch.conf's active fonts. A font set whose files are not installed, or are installed but not active, draws nothing, and the console then draws the built-in font. The console follows OSC 4 and OSC 104 for its palette, and the grid request for its size. When a case restarts vtium, the console reads the installed vtium.conf for its head again, through vtium-settings' own reading of a head's keys.

### How a case file reads

Each line starts with a word that says what it does. A line that starts with `#` is a comment, and an indented line belongs to the `file`, `frit` or `draft` line above it. In the text of a file, `ROOT` stands for the machine's folder.

| Line | What it does |
|---|---|
| `title TEXT` | The task in the person's words, and what they expect to see. Every case has one. |
| `waits-for TEXT` | The defect that makes the case fail today. The runner reports the case as expected-red while a check fails, and the test fails once every check passes, so the line comes out with the fix. |
| `terminal COLSxROWS` | The size of the terminal, 100×30 unless a case says otherwise. |
| `head NAME` | The head the console runs, `console` unless a case says otherwise. |
| `view NAME` | The view the TUI opens in: `flow`, `effects`, `looks`, `fonts`, `settings` or `help`. The Flow view is the default. |
| `file PATH` | A file of the machine at PATH under its folder, such as `etc/vtium.conf` or `etc/batch.conf`, with the indented lines below as its text. |
| `frit NAME` | The installed font set NAME, `etc/fonts/NAME.vtfrit`, with the indented lines below as its text. |
| `font PATH WxH` | A bitmap font of that cell at PATH, which the runner writes in BDF. |
| `draft from-installed` | The draft starts as a copy of the installed vtium.conf, as it does without this line. A `draft` line followed by indented lines starts the draft with that text instead. |
| `press KEYS` | Presses each key in turn: a named key such as `Enter`, `Esc`, `Tab`, `Shift-Tab`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PgUp`, `PgDn`, `Backspace`, `Space` or `Ctrl-C`, or one character such as `q`. |
| `type "TEXT"` | Types the text, one character at a time. |
| `wait N` | Lets N seconds of the live trial's countdown pass. |
| `command WORDS` | Runs vtium-config on the command line with the machine's folders, such as `command install`. |
| `restart` | The person restarts vtium, and the console reads the installed files again. |
| `at NAME` | Names the point the checks below belong to, which each failure names. |
| `check ...` | A check, below. |

The checks:

| Check | What must hold |
|---|---|
| `check screen shows "TEXT"`, `check screen lacks "TEXT"` | The screen shows the text, or does not. The screen's words are read with one space between each, so a text that wraps across rows still matches. |
| `check said shows "TEXT"`, `check said lacks "TEXT"` | What the install and the command lines printed holds the text, or does not. |
| `check draft has "TEXT"`, `check installed lacks "TEXT"` | The saved draft or the installed vtium.conf holds the text, or does not. |
| `check draft head NAME KEY VALUE` | vtium takes VALUE for KEY on the head NAME when it runs the saved draft. `installed` in place of `draft` reads the installed vtium.conf. The key `look` reads the head's own section. |
| `check draft look NAME KEY VALUE` | The section `[look NAME]` sets KEY to VALUE. |
| `check console font NAME` | The console was last asked for the font set NAME. |
| `check console draws NAME` | The console draws the font set NAME, or `built-in` when the set it was asked for draws nothing. |
| `check console fell-back yes` | The console draws the built-in font in place of the set it was asked for, or with `no`, it does not. |
| `check console cell WxH`, `check console grid COLSxROWS` | The console's cell in pixels, and its grid. |
| `check console palette NAME` | The console's sixteen colors are those of vtium's palette NAME, or `custom`. |
| `check console received "TEXT"`, `check console never-received "TEXT"` | The bytes vtium-config wrote to the console hold the text, or do not. `\e` in the text is ESC. |
| `check exited STATUS` | vtium-config exited with STATUS, or `no` while it runs. |

### An example

This case, `cases/readme-example.case`, makes a look in the Looks tab, saves it with the quit question, and installs it from the command line:

```
title Make a look with the font set mono at 24 pt, quit with q and save, then install it: the console shows the look.

file etc/vtium.conf
    version = 1
    [head console]
    vt = 5
    look = big
file etc/batch.conf
    ROOT/fonts/mono.bdf
font fonts/mono.bdf 8x16
frit mono
    source=bdf:ROOT/fonts/mono.bdf

# n makes a look, whose name is typed over new-look.
press 3 n
press Backspace Backspace Backspace Backspace Backspace Backspace Backspace Backspace
type "big"
press Enter Down Right
press Down Enter End Up Up Enter Esc
press q
check screen shows "y save and quit"
press y
at the end of vtium-config
check exited 0
check draft look big font-name mono

command install
restart
at the restart
check console draws mono
check console cell 16x32
```

To see the screen and the console after each step while you write a case, run the case alone with `VTIUM_USECASE`:

```sh
VTIUM_USECASE=a-make-a-look-and-install-it VTIUM_USECASE_SHOW=1 cargo test -p vtium-config --test scenarios every_use_case -- --nocapture
```
