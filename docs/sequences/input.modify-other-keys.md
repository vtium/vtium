<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# modifyOtherKeys: XTMODKEYS, XTQMODKEYS

| Sequence | What it does |
|---|---|
| `CSI > 4 ; Pv m` | XTMODKEYS: set the modifyOtherKeys level |
| `CSI > 4 n` | turn modifyOtherKeys off |
| `CSI ? 4 m` | XTQMODKEYS: ask for the modifyOtherKeys level |

## Parameters

- `Pv`: The level: 0 turns modifyOtherKeys off, 1 reports the keys whose modifiers would otherwise be lost, and 2 reports every key pressed with a modifier. The default is 0.

## What vtium does

At level 1 or 2, vtium sends a key pressed with Shift, Alt or Ctrl as `CSI 27 ; Pm ; Pc ~`, where `Pm` gives the modifiers and `Pc` the key's code. The key table in ctlseqs decides which keys take that form at each level. `CSI > m` with no parameters also returns the level to 0.

XTQMODKEYS is answered `CSI > 4 ; Pv m` with the level in force. vtium ignores the other resources that XTMODKEYS can name. RIS and DECSTR return the level to 0.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors such as vim and Emacs tell Ctrl and Shift combinations apart with it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `modify_other_keys_encodes_modified_keys` in [crates/vtium-term/tests/input.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/input.rs)
- `xtmodkeys_sets_and_reports_the_level` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- [tools/conformance/scenarios/key-modify-other-keys.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/key-modify-other-keys.scenario)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
