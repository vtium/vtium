# vtium-config-core

This crate holds the configuration that vtium-config's command line and TUI share, and the commands they call (ADR-1201, ADR-1403). It keeps the user's draft of vtium.conf and edits it through one set of operations. It holds vtium-config's store, the library and the history, and the active resources beside a draft. It turns an install into an install bundle and runs the privileged step that installs it. It also builds the flow from a draft and the devices present, and finds the fonts on this machine. It starts the font helper for a frit's specimen, and switches the running console's frit.

vtium-config depends on this crate and re-exports each of its modules at its own root, so `vtium_config::drafts` and the rest keep their paths. The crate builds on vtium-font-helper, whose modules its code names at its root, such as `crate::frit` and `crate::serve`. It also builds on vtium-config-widgets, because a specimen's picture of the console is painted in the toolkit's rows of styled text and its colors. It depends on no part of vtium-config's TUI or command line.

Configuration files, fonts and the terminal's replies are untrusted input, and each read stops at its bound ([Bounds](#bounds)). The crate forbids unsafe code and denies the lints that find a panic: unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's lints, its modules, and the font helper's modules that its code names. |
| `src/options.rs` | The options that need a value, in the one table that the parser, `--help` and vtium-config(1) read. |
| `src/places.rs` | Where vtium-config reads and saves: `Paths`, the directories and programs the command line names, the defaults of the directory of vtium.conf and the drafts' directory, and `read_conf`, the bounded read of a configuration file. The commands and the TUI share them. |
| `src/fontlist.rs` | The fonts vtium-config lists: the frits installed and the built-in fonts, each with its state, and the layers a font set can use. |
| `src/shell.rs` | A word quoted for a shell, for the command lines that vtium-config shows. |
| `src/words.rs` | The words that the commands print and the TUI shows too, such as Install's question for a draft with no complete path. The TUI's catalog, vtium-config's `src/tui/words/`, re-exports them. |
| `src/commands.rs` | `fonts`, `check FILE`, `remove-frit NAME` and `new-frit`'s non-interactive forms (ADR-1201 D2), and the key checks that `get` and `set` share. |
| `src/help.rs` | The help text: the usage that `--help` prints, and the help that `--help TOPIC` prints for one command. The options come from `src/options.rs`'s table, and vtium-config(1), the man page that tools/man generates, reads the same lists. |
| `src/banner.rs` | The banner that `vtium-config --help about` draws at a terminal: vtium-config's name in glass, in the color depth the environment reports, or a line of text where the terminal cannot show it. |
| `src/store.rs` | vtium-config's store under the XDG directories (ADR-1403 D8). It holds the drafts, each draft's font set folder `NAME.frits/`, the library, the tool's settings, the installed history and each draft's base with its text, in `bases/`. It finds root's home from the password database. |
| `src/drafts.rs` | The current draft, made from the installed file when there is none. The draft's state against the installed file, its font set changes against the installed frits, the check that every change runs, and `install`, which writes the new frits, the batch file, vtium.conf, and then the removals. |
| `src/batch.rs` | The active resources beside a draft (ADR-1205 D4): the draft's changes to the batch file in `NAME.batch`, the batch file an install writes from them, the trust rule each activated file passes, and the commands `activate`, `deactivate` and `active`. |
| `src/vts.rs` | The step after an install of the system's vtium.conf (ADR-1201 D5). It reads `vtium console status`. It enables vtium with `vtium console enable` on each VT that a head names, where vtium is not enabled yet. It runs the `systemctl start` and `restart` that apply the new file, only after a yes. |
| `src/ops.rs` | The one set of operations on the draft. Each operation takes vtium.conf's text and gives back the edited text, through vtium-settings's line editor and section writer. The commands and the TUI call them alike. |
| `src/flowmodel.rs` | The Flow view's model, built from a draft and the devices present, and placed through vtium-devices. Also `flow`'s text and JSON. |
| `src/flowcli.rs` | The flow's commands and their exit codes. |
| `src/outcome.rs` | The exit codes 1 to 5 of ADR-1403 D8. |
| `src/flow.rs` | The flow that the Flow view draws, as plain data: the displays, the rules, the heads, the looks and the input devices, each with its state. `flowmodel.rs` builds it from the draft and the devices present. |
| `src/library.rs` | The library, `library.conf`. Looks and heads are copied between the library and the draft. |
| `src/draft.rs` | What an install bundle changes: each file before and after, each frit removed, and the lines that differ. The install view and `diff` show these. |
| `src/document/` | The configuration as the TUI's one open document: what it holds, the one function every change passes, which writes the draft through a desk, the undo history, and the count Save's review shows, which `refresh` counts again against the installed file read again. mod.rs holds the document, conf.rs the edits to vtium.conf, desk.rs the store's desk and the one kept in memory, undo.rs the history. |
| `src/versions/` | The document's versions: whether the installed file changed on disk since the base (base.rs), Update's three-way merge (merge.rs), Overwrite, Revert to saved and History's changes (overwrite.rs, revert.rs, history.rs), the hunks and changed words Show changes shows (diff.rs, the one caller of imara-diff), and the command line's update, install --overwrite and draft rename (command.rs). |
| `src/apply.rs` | The install bundle that an install becomes, and the argv of the privileged step. `apply DIR` checks an install bundle and installs it. `apply DIR`'s check passes `--with-frit` for a copy of each frit the bundle writes, and `--without-frit` for each frit it removes. Root writes the copies into a new private folder under `/run`, and a user's install writes them under `$XDG_RUNTIME_DIR`. apply first checks that folder with the console's ownership rule, and refuses the install in its own words when the folder fails. When a bundle writes or removes a frit, `apply DIR` checks the frits' directory before it reads anything there, with the walk the console reads frits through, `vtium_settings::folder`. It then works through a descriptor it holds on that directory. It reads each installed frit only as a regular file, never through a link and without waiting on a FIFO, and writes each one with `vtium_settings::install_file_at`. It looks up a new frit's name in the catalog's category folders through the same descriptor, without following a link, so anything at that name in a category folder counts as a set of the catalog. |
| `src/specimen.rs` | The glyph helper as vtium-config starts it, under the helper's own sandbox. It gives a frit's specimen at a size, laid out from the glyphs that the helper answers with. It also gives the sources that a frit lacks, which the helper finds. |
| `src/discover.rs` | The fonts vtium-config finds on this machine ([Finding fonts](../vtium-config/README.md#finding-fonts)): the folders, the scan, the trust mark, the batch state, the not-usable mark and the duplicate mark of each font file, and the helper's description of each. |
| `src/switch.rs` | `use-frit`: OSC 50 and its query on the controlling terminal. The reply is read in raw mode, within its wait. |
| `src/report/` | `report` (ADR-1200 D24). `parts.rs` gathers each part of the document. `binary.rs` reads each vtium binary's version and features. `units.rs` finds the vtium units and reads their logs. `edid.rs` reads a monitor's name, size and serials. `read.rs` is the only way that the report reads files and runs programs, under the root that `--root` names. `scrub.rs` replaces the hostname and the monitors' serials in what was gathered. |
| `src/scratch.rs` | A unit test's own temporary directory, removed when the test ends. A copy of vtium-config's, which its TUI's tests still use. |

## Bounds

| Bound | Limit | Past it |
|---|---|---|
| `CONF_MAX` in `src/places.rs` | 1 MiB of a vtium.conf that vtium-config reads | The read fails with an error |
| `MANIFEST_MAX` in `src/apply.rs` | 64 KiB of an install bundle's manifest | `apply` refuses the bundle |
| `FILES_MAX` in `src/discover.rs` | 20,000 font files in one scan | The scan lists no more |
| `ENTRIES_MAX` in `src/discover.rs` | 65,536 entries read in one folder | The scan reads no more of that folder |
| `DEPTH_MAX` in `src/discover.rs` | the font helper's `FONT_DEPTH_MAX` of folders under a font folder | The scan looks no deeper |
| `PICTURE_MAX` in `src/specimen.rs` | 1024×768 pixels of a picture | The picture is cut to this size |
| `READ_MAX` in `src/switch.rs` | 4096 bytes read while `use-frit` waits for the terminal's reply | `use-frit` stops reading and takes what it has |
| `FILE_MAX` in `src/report/read.rs` | 256 KiB of a file that the report quotes | The report quotes the first 256 KiB |
| `OUTPUT_MAX` in `src/report/read.rs` | 1 MiB of a program's output that the report quotes | The report quotes the newest 1 MiB and says how much it left out |
| `BINARY_MAX` in `src/report/read.rs` | 256 MiB read of a vtium binary, looking for its markers | The report reads no further |
| `LINES_MAX` in `src/report/mod.rs` | 100,000 log lines for `--lines` | `report` refuses the option |

`cargo test -p vtium-config-core` runs the crate's unit tests. vtium-config's integration tests run the same code through the binary: `tests/apply.rs`, `tests/batch.rs`, `tests/commands.rs`, `tests/flow.rs`, `tests/report.rs`, `tests/vts.rs` and the TUI's scenarios.

References:
- ADR-1201: vtium-config's commands, its draft of vtium.conf and the step after an install.
- ADR-1403 D8: the store, the drafts and the exit codes.
- ADR-1205 D4: the active resources beside a draft.
- ADR-1513 D1 and D4: the split that made this crate, which moved the code without changing it.
