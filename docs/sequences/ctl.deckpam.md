<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECKPAM, DECKPNM

| Sequence | What it does |
|---|---|
| `ESC =` | DECKPAM: keypad in application mode |
| `ESC >` | DECKPNM: keypad in numeric mode |

## What vtium does

DECKPAM makes the keypad's keys send their application sequences, and DECKPNM makes them send their digits and symbols. The two set and reset DECNKM, mode 66, so DECRQM reports the keypad's state through that mode.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs read the keypad in application form.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `deckpam_switches_the_keypad` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- [tools/conformance/scenarios/keypad-deckpam.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/keypad-deckpam.scenario)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
