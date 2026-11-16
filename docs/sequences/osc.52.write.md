<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 52 write, inside the OSC string bound

| Sequence | What it does |
|---|---|
| `OSC 52 ; Pc ; Pd ST` | set the clipboard |

## Parameters

- `Pc`: The selections to set. `c`, `p`, `s` and an empty list all set vtium's one selection. `q` and the cut buffers `0` to `7` set nothing. The default is an empty list.
- `Pd`: The text to copy, encoded in base64.

## What vtium does

vtium decodes the text and hands it to the console's clipboard, where a paste finds it. The text is UTF-8, and each ill-formed sequence in it becomes U+FFFD. A string that is too long, or that the parser cut short, sets nothing, and the selection keeps what it held.

Data that is not base64 sets nothing, where ctlseqs clears the selection. See [OSC strings and the Linux console's sequences in vtium-term's README](https://github.com/vtium/vtium/blob/main/crates/vtium-term/README.md#osc-strings-and-the-linux-consoles-sequences).

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs such as tmux and vim copy text to the clipboard with it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `ill_formed_osc_52_data_is_stored_with_u_fffd` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `osc_52_at_the_cap_sets_and_one_byte_over_is_dropped` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `osc_52_sets_the_selection` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `osc_52_sets_the_selection_for_c_p_and_s` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `osc_52_split_at_every_offset` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `osc_52_text_is_pasted_in_the_inputs_encoding` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `osc_52_text_past_the_cap_once_replaced_is_dropped` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `osc_52_with_invalid_base64_is_dropped` in [crates/vtium-term/tests/clipboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/clipboard.rs)
- `a_hostile_selection_is_pasted_filtered_once_a_press` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
