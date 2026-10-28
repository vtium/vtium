# parse-oracle

A differential between `vtium-parse` and the C reference's parser. The C reference's parser is taken by line range from a pinned libtsm commit and compiled into this tool. Both parsers play the same streams of events and write one trace in the same format, which the tool compares line by line. Every difference must be one of the departures `crates/vtium-parse/README.md` lists, and any other difference fails the run.

The tool is its own cargo workspace. Building it needs gcc and the pinned libtsm commit, so the main workspace and CI never build it. It runs on request and is not one of the gates in AGENTS.md: vtium-parse's own tests and tier 1 judge the parser, and no gate runs the C reference's code (#111).

```
cargo run --release --manifest-path tools/parse-oracle/Cargo.toml -- run       # the differential
cargo run --release --manifest-path tools/parse-oracle/Cargo.toml -- selftest  # the plants, below
cargo run --release --manifest-path tools/parse-oracle/Cargo.toml -- replay out/difference.script
```

`run` exits 0 when the traces agree, 1 at a difference, and 2 when it refuses to run. It takes `--seed` and `--streams` for the generator, `--without <departure>` to take a departure out of the expectations, and `--out` for where differences and the self-test's builds go (default `out/` here, which git ignores).

The build reads the pinned commit from `$LIBTSM` (default `../libtsm` beside this repository) through `git show` only, and builds nothing there. If that checkout lacks the commit, the build clones the pin's URL into its own `OUT_DIR`.

| Path | What it is |
|---|---|
| `pin.tsv` | The libtsm commit the C side is taken from, and its URL. |
| `extract.tsv` | Each line range taken from the pin, with the text of its first and last lines. |
| `build.rs` | Extracts the ranges, refusing a moved one, compiles them with `harness/shim.c` through the `cc` crate, and embeds the C reference's conformance inputs at the pin as the corpus. |
| `harness/shim.c` | The only hand-written C: the extracted ranges, the fields of `struct tsm_vte` they read, and the `#define`s that send each dispatch and each function a departure acts on to Rust. |
| `departures.tsv` | The expected differences: one row per departure in the parse README, naming the switch in `src/c_side.rs` that applies it. |
| `src/pin.rs` | Reading the pin and the ranges, extraction with its checks, and the corpus. `build.rs` and the self-test share it. |
| `src/c_side.rs` | The C side through FFI: the mirror of `struct tsm_vte`, checked against the shim's layout at start, the callbacks that write the trace, and the hooks that apply the departures. |
| `src/rust_side.rs` | The Rust side: vtium-parse with a consumer that writes the same trace. |
| `src/script.rs` | Streams, the trace writer and its triggers, the driver that plays a stream through either side, and the script file a difference is saved to. |
| `src/generate.rs` | The corpus streams, the written cases and the seeded generator. |
| `src/departures.rs` | The departures, and the check that `departures.tsv` and the parse README list the same ones. |
| `src/plants.rs` | The defects the self-test plants in copies of vtium-parse. |
| `src/main.rs` | The commands, the comparison and the report. |

## The C side

`build.rs` reads each range in `extract.tsv` from the pinned commit and writes it to `OUT_DIR/extract/<name>.inc`, with a `#line` directive back to the source. Extraction refuses to proceed in three cases: the commit is missing, the first or last line of a range differs from the text recorded for it (whitespace collapsed), or the range's braces do not balance. So when the pin moves, every range has to be checked again, and so does the struct mirror: the fields of `struct tsm_vte` in `harness/shim.c` and in `src/c_side.rs` against the pin's `tsm-vte-int.h`. The layout check compares the shim with the mirror, not with the C reference's header.

The extracted code is the parser: the UTF-8 decoder, `do_clear`, `do_collect`, `do_param`, OSC collection, the DCS start, collect and end, `do_action`, the transition tables, `parse_data`, `vte_input`, and the parser's lines of `tsm_vte_reset`. `harness/shim.c` includes it unchanged and holds no logic of its own:

- Each dispatch the parser makes is a Rust callback that writes the trace line: print, execute, `do_esc`, `do_csi`, `do_osc_end`, XTGETTCAP's answer and the APC handlers.
- Each function a departure acts on is renamed `ref_*` as it is included. Its callers reach a Rust hook, which applies the departures and calls the C function back through an `oracle_ref_*` export.
- Logging, the charset mapping, the sixel handler and the image fast paths compile to nothing.

Unsafe code lives only in `src/c_side.rs`, the FFI: `src/main.rs` denies `unsafe_code` and allows it for that module alone. `src/c_side.rs` mirrors `struct tsm_vte` and the C reference's constants. On start it checks both against the shim's `oracle_layout` and `oracle_constants` and refuses to run on any difference. The C side runs only inside a call from the Rust side, and a callback reaches the side's state through a thread-local cell for the length of that call.

The trace is written where vtium's parser hands work to its consumer. Each dispatch is traced as it is entered, and what the C reference then did with it belongs to `vtium-term`. Every DCS string except XTGETTCAP is traced as hooked: vtium-parse leaves the choice of handler to its consumer, where the C reference made it in `do_dcs_start` and hooked only sixel. `vte_input` runs without the sixel and kitty fast paths, so every byte goes through `parse_data`.

## The Rust side

`src/rust_side.rs` drives `vtium_parse::Parser` with a consumer that writes the same trace. It cuts each data record into pieces of 1 to 256 bytes, or keeps the rest whole. A generator seeded with the stream's number picks the sizes, and a quarter of the pieces are one byte. The C side takes each record whole, so the comparison also checks that the actions do not depend on where the input is cut.

## Departures as expected differences

With no departure applied, the C side's trace is the C reference's own. `departures.tsv` names one switch in `src/c_side.rs` per departure in the parse README. A run applies every row, so the C side reads as the C reference with vtium-parse's listed departures, and any difference left is unlisted. Before anything runs, the tool checks that the rows and the README's Departures list match, bold lead for bold lead.

Two plants show that each switch carries weight:

- `run --without <departure>` takes a departure out of the expectations and leaves the list check in place. The run must find a difference.
- Deleting the row instead makes the list check refuse the run.

`osc-bound` is the one switch that replaces part of the C side: the C reference's `osc_arg` holds 128 bytes, so the switch collects the OSC string on the Rust side, by the C reference's rule at the 1 MiB bound, and a clear empties it where it empties `osc_arg`. The bound is written in `src/c_side.rs` and `src/generate.rs`, not taken from vtium-parse, so a wrong `OSC_MAX` there shows as a difference; the written cases cross it.

Each switch is a few lines of Rust written from the README's text, which `departures.tsv` restates. A departure misread the same way in vtium-parse and in its switch still goes unseen. The README text is the check on both.

With `intermediates` taken out, the C side prints the C reference's intermediate flag set (`f<hex>`) where the trace shows the bytes, since the flag set was all the C reference kept.

## Streams

A stream is a fresh parser in one mode (UTF-8, 8-bit or 7-bit) and a list of records:

| Record | Meaning |
|---|---|
| data | Bytes fed to the parser. |
| reset | Reset the parser between calls. |
| mode | Switch the mode between calls. |
| pause after k | Pause after the k-th counted trace line from here, with a reset at the pause when asked, the way a consumer handles RIS. The rest of the record is fed after the pause. |
| mode after k | Switch the mode after the k-th ESC or CSI dispatch from here, inside a call, as DECSCL does. |

Every trace line counts toward a pause except `dcs_put` and `apc_put`. The Rust side checks for a pause after each run of string data, and the C side after each byte, so a pause raised inside a run would stop the two sides at different bytes. A mode switch is counted only on ESC and CSI dispatches, the only actions that change the mode in a terminal.

The streams each run compares:

- every conformance input of the C reference at the pin (`test/conformance/cases` and `recorded`), whole in each mode, and once more cut and with events;
- the written cases in `src/generate.rs`, for edges a random cut reaches rarely, such as a reset between the ESC and the `\` of an ST;
- `--streams` generated streams from `--seed`: random bytes, tokens that include a run at each bound of the parse README's bounds table, and grammar-built sequences with mangled headers and every string end. Most are cut into records with resets, mode switches, pauses and mode triggers between them, half the cuts falling right after an ESC.

At the first difference the run prints the lines around it from both sides and saves that stream to `out/difference.script`, which `replay` plays again. The script holds, per stream, `S`, the mode and the stream's number (8 bytes). Each record follows as one of: `D` with a 4-byte length and the bytes, `R`, `M` with the mode, `P` or `Q` (with a reset) with a 4-byte count, or `T` with a 4-byte count and the mode. Integers are little-endian.

## Trace

One line per action, the same on both sides:

| Line | Action |
|---|---|
| `mode` m, `reset`, `pause` offset | The stream's own events. A pause gives the offset in its data record where the parser stopped. |
| `print` c, `exec` c | A character printed and a control executed, in hex. |
| `esc` final ims | An ESC dispatch. `ims` is the private marker and the intermediates as hex bytes, split by `/` (`3f/24`, `/2121`), or `-` for none. |
| `csi` final ims [params] | A CSI dispatch. Each parameter is a number, or -1 when omitted, followed by `:`-joined sub-parameters. |
| `osc` end [`t`] data | An OSC string's end (`st`, `bel` or `cancel:`byte), `t` when truncated, and its bytes in hex. |
| `dcs_hook`, `dcs_put`, `dcs_unhook` | A DCS string's header, each data character and its end. |
| `apc_start`, `apc_put`, `apc_end` | The same for an APC string. |
| `tcap` payload | An XTGETTCAP request in hex, or `none` past its bound. |

## Self-test

`selftest` first runs the baseline, which must agree. It then takes each departure out in turn, and builds the tool against copies of vtium-parse under `out/plants/`, each with one defect from `src/plants.rs`. It also moves each range in `extract.tsv` a line up and a line down. Each must turn the run red, or be refused in the case of a moved range. Three plants show only with the stream events: `pause-ignored` with pauses, `reset-keeps-pending-end` with resets, and `mode-read-once-per-call` with a mode switch inside a call.
