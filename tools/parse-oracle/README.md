# parse-oracle

A differential between `vtium-parse` and the C reference's parser. The C reference's parser is taken by line range from a pinned libtsm commit and built into a C harness. `vtium-parse` is built into a Rust harness. Both run the same event scripts and write one trace in the same format, and the tool compares the two line by line. Every difference must be one of the departures `crates/vtium-parse/README.md` lists, and any other difference fails the run.

The tool needs git, gcc, cargo and python3, and a libtsm checkout or network access, so CI does not run it.

```
tools/parse-oracle/oracle.py run        # the differential: exit 0 agree, 1 a difference, 2 refused
tools/parse-oracle/oracle.py selftest   # shows the run turns red for every plant below
```

`run` takes `--seed` and `--streams` for the generator, `--without <departure>` to take one departure out of the expectations, and `--plant <name>` to build `vtium-parse` with a planted defect. Both commands read the C reference from `--libtsm` (default `../libtsm` beside this repository), only through `git show`. If that checkout lacks the pinned commit, they clone the pin's URL under `--out` (default `out/` here) and read from the clone. Every file the tool writes goes under `--out` and this directory's `target/`, and git ignores both.

| Path | What it is |
|---|---|
| `oracle.py` | The driver: pin check, extraction, both builds, the scripts, the comparison and the self-test. |
| `pin.tsv` | The libtsm commit the C side is taken from, and its URL. |
| `extract.tsv` | Each line range taken from the pin, with the text of its first and last lines. |
| `harness/oracle.c` | The C side: includes the extracted ranges unchanged and adds the parser-only state, a trace stub for each dispatch, and the departure switches. |
| `departures.tsv` | The expected differences: one row per departure in the parse README, naming the switch that applies it to the C side. |
| `src/main.rs`, `Cargo.toml`, `Cargo.lock` | The Rust side, in its own workspace so the main workspace and CI do not build it. |

## The C side

`oracle.py` reads each range in `extract.tsv` from the pinned commit with `git show` and writes it to `out/extract/<name>.inc`, with a `#line` directive pointing back at the source. Extraction refuses to proceed in three cases: the commit is missing, the first or last line of a range differs from the text recorded for it (whitespace collapsed), or the range's braces do not balance. So when the pin moves, every range has to be checked again.

The extracted code is the parser: the UTF-8 decoder, `do_clear`, `do_collect`, `do_param`, OSC collection, the DCS start, collect and end, `do_action`, the transition tables, `parse_data`, `vte_input`, and the parser's lines of `tsm_vte_reset`. `harness/oracle.c` includes it unchanged. When the harness needs to see or change a function, it renames the extracted function `ref_*` at the include and calls it from a wrapper that takes the original name.

The harness puts the trace at the point where vtium's parser hands work to its consumer:

- Each dispatch is traced as it is entered: `do_esc`, `do_csi`, `do_osc_end`, the APC handlers and XTGETTCAP's answer. What the C reference then did with the dispatch (its CSI table, charsets, replies) belongs to `vtium-term`.
- Every DCS string except XTGETTCAP is traced as hooked. `vtium-parse` leaves the choice of handler to its consumer, where the C reference made it in `do_dcs_start` and hooked only sixel.
- `vte_input` runs without the sixel and kitty fast paths, which hand the C reference's image code a run of bytes at once. Every byte goes through `parse_data`.

## The Rust side

`src/main.rs` drives `vtium_parse::Parser` with a consumer that writes the same trace. It cuts each data record into pieces of 1 to 256 bytes, or keeps the rest whole. A generator seeded with the stream's number picks the sizes, and a quarter of the pieces are one byte. The C side takes each record whole, so the comparison also checks that the actions do not depend on where the input is cut.

## Departures as expected differences

With no departure named, the C side's trace is the C reference's own. `departures.tsv` names one switch in `harness/oracle.c` per departure in the parse README. A run applies every row, so the C side reads as the C reference with vtium-parse's listed departures, and any difference left is unlisted. Before anything is built, the run checks that the rows and the README's Departures list match, bold lead for bold lead.

Two plants show that each switch carries weight:

- `run --without <departure>` takes a departure out of the expectations and leaves the list check in place. The run must find a difference.
- Deleting the row instead makes the list check refuse the run.

Each switch is a few lines of C written from the README's text, which `departures.tsv` restates. A departure misread the same way in the Rust code and in its switch still goes unseen. The README text is the check on both.

With `intermediates` taken out, the C side prints the C reference's intermediate flag set (`f<hex>`) where the trace shows the bytes, since the flag set was all the C reference kept.

## Scripts

A script is a run of records, each a kind byte and its arguments, with integers as 4-byte little-endian:

| Record | Meaning |
|---|---|
| `S` mode | A new stream: a fresh parser in mode `u` (UTF-8), `8` (8-bit) or `7` (7-bit). |
| `D` length bytes | Data, fed to the parser. |
| `R` | Reset the parser between data records. |
| `M` mode | Switch the mode between data records. |
| `P` k | Pause after the k-th counted trace line from here. The rest of the record is fed after the pause. |
| `Q` k | The same pause, followed by a reset, the way a consumer handles RIS. |
| `T` k mode | Switch the mode after the k-th ESC or CSI dispatch from here, inside a call, as DECSCL does. |

Every trace line counts toward a pause except `dcs_put` and `apc_put`. The Rust side checks for a pause after each run of string data, and the C side after each byte, so a pause raised inside a run would stop the two sides at different bytes. A mode switch is counted only on ESC and CSI dispatches, the only actions that change the mode in a terminal.

The streams each run compares:

- every conformance input of the C reference at the pin (`test/conformance/cases` and `recorded`), whole in each mode, and once more cut and with events;
- the scripts in `CASES`, written for edges a random cut reaches rarely, such as a reset between the ESC and the `\` of an ST;
- `--streams` generated streams from `--seed`: random bytes, tokens that include a run at each bound of the parse README's bounds table, and grammar-built sequences with mangled headers and every string end. Most are cut into records with resets, mode switches, pauses and mode triggers between them, half the cuts falling right after an ESC.

## Trace

One line per action, the same on both sides:

| Line | Action |
|---|---|
| `stream` n, `mode` m, `reset`, `pause` offset | The script's own events. A pause gives the offset in its data record where the parser stopped. |
| `print` c, `exec` c | A character printed and a control executed, in hex. |
| `esc` final ims | An ESC dispatch. `ims` is the private marker and intermediates as hex bytes, or `-` for none. |
| `csi` final ims [params] | A CSI dispatch. Each parameter is a number, or -1 when omitted, followed by `:`-joined sub-parameters. |
| `osc` end [`t`] data | An OSC string's end (`st`, `bel` or `cancel:`byte), `t` when truncated, and its bytes in hex. |
| `dcs_hook`, `dcs_put`, `dcs_unhook` | A DCS string's header, each data character and its end. |
| `apc_start`, `apc_put`, `apc_end` | The same for an APC string. |
| `tcap` payload | An XTGETTCAP request in hex, or `none` past its bound. |

At the first difference the run prints the lines around it from both sides and saves the stream's script to `out/difference.script`. The two harnesses replay that file: `out/oracle-c <departures...> < out/difference.script` and `target/release/parse-oracle < out/difference.script`.

## Self-test

`selftest` first runs the baseline, which must agree. It then takes each departure out in turn, builds each defect in `PLANTS` into a copy of `vtium-parse` under `out/plants/`, and moves each range in `extract.tsv` a line up and a line down. Each of these must turn the run red, or be refused in the case of a moved range. Three plants show only with the script's events: `pause-ignored` with pauses, `reset-keeps-pending-end` with resets, and `mode-read-once-per-call` with a mode switch inside a call.
