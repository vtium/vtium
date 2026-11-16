<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 21: kitty color protocol

| Sequence | What it does |
|---|---|
| `OSC 21 ; Pt ST` | set, ask for or reset colors by key |

## Parameters

- `Pt`: A list of keys separated by `;`. `key=spec` sets a color, `key=?` asks for it, `key=` unsets it, and a bare key resets it. The keys are `foreground`, `background`, `cursor`, `selection_background`, `selection_foreground`, and the palette indexes 0 to 255.

## What vtium does

vtium sets, unsets and resets each key in turn. It answers every query of the string in one reply, ended as the string was. `key=` unsets only the cursor and the selection colors, and vtium ignores it for the other keys. vtium reads a spec as OSC 4 reads it.

vtium leaves out of its reply a key that it does not know, instead of answering it with `unknown=` and the key's name in base64. A reply from vtium never carries bytes of the request. See [Colors in vtium-term's README](https://github.com/vtium/vtium/blob/main/crates/vtium-term/README.md#colors).

## Status

- **Status:** supported. vtium implements this as the specifications below define it, apart from the departure above.
- **Why:** programs written for kitty set and read colors through it, and Ghostty shares it.
- **Note:** kitty's meaning; a key vtium does not know is left out of the reply, where kitty answers `unknown=` with its name in base64 (a departure from kitty, by the reply rule).
- **Specifications:** kitty color-stack; Ghostty ext

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `osc_21_sets_queries_and_resets_by_key` in [crates/vtium-term/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/colors.rs)

## Where to read more

To follow the work on this row, see issue [#38](https://github.com/vtium/vtium/issues/38).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
