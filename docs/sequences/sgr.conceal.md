<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 8 and 28: conceal

| Sequence | What it does |
|---|---|
| `CSI 8 m` | conceal |
| `CSI 28 m` | reveal |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium draws a concealed cell as its background alone. The cell keeps its text, so a selection over it still copies the text.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs hide a typed password or a spoiler with it.
- **Note:** a concealed cell draws its background alone and keeps its text for selection; the xterm-vtium entry sets it with invis and sgr.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_concealed_cell_draws_its_background_alone` in [crates/vtium-render/tests/renditions.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/renditions.rs)
- `conceal_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)

## Where to read more

To follow the work on this row, see issue [#37](https://github.com/vtium/vtium/issues/37).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
