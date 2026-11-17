<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 22: pointer shape

| Sequence | What it does |
|---|---|
| `OSC 22 ; Pt ST` | set the pointer's shape |
| `OSC 22 ; = Pt ST` | set the pointer's shape |
| `OSC 22 ; > Pt ST` | push shapes on the pointer's stack |
| `OSC 22 ; < ST` | pop a shape from the pointer's stack |
| `OSC 22 ; ? Pt ST` | ask which shapes vtium has |

## Parameters

- `Pt`: One or more shape names, separated by `,`.

## What vtium does

vtium draws the pointer in the shape a program asks for while the pointer is over its text. It has these shapes: default, text, pointer, crosshair, wait, progress, not-allowed, move, ew-resize and ns-resize. The X cursor names left_ptr, xterm, hand1, hand2, watch, fleur, sb_h_double_arrow and sb_v_double_arrow name the same shapes. A name vtium does not know gives the default shape.

A set replaces the shape on top of the stack, and takes the first name in the list that vtium knows. A push adds each name in turn, skips empty names, and keeps at most 16 entries. The main and alternate screens each keep a stack of their own, and RIS empties both.

A query answers `OSC 22 ; Pt ST` with one answer for each name: 1 when vtium has the shape and 0 when it does not. `__current__` answers the shape on top of the stack, or 0 when the stack is empty. `__default__` and `__grabbed__` answer `default`. The reply ends with BEL when the query did.

The `pointer-theme` key can replace vtium's own pictures of the shapes with those of an SVG file or a cursor theme.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** vtium draws the pointer, and programs change its shape over links and edges.
- **Note:** vtium's set is default, text, pointer, crosshair, wait, progress, not-allowed, move, ew-resize and ns-resize, each with a picture of its own, and xterm's X cursor names left_ptr, xterm, hand1, hand2, watch, fleur, sb_h_double_arrow and sb_v_double_arrow name the same shapes; another name gives the default shape; a set replaces the top of the screen's stack, a push skips empty names and keeps at most 16, the main and alternate screens keep a stack each and RIS empties both; a query answers 1 or 0 for a name, 0 for an empty name, the shape on top for __current__ (0 for an empty stack) and default for __default__ and __grabbed__; pointer-char draws the default shape. `pointer-theme` names an SVG file or a cursor theme's folder whose pictures replace vtium's own for each shape the file holds, drawn by the font helper (ADR-1309).
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); kitty pointer-shapes

## Tests

These tests check what vtium does with the sequences on this page.

- `each_shape_has_its_own_picture_a_cell_tall` in [crates/vtium-render/src/arrow.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/arrow.rs)
- `the_text_and_crosshair_pictures_at_16_pixels` in [crates/vtium-render/src/arrow.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/src/arrow.rs)
- `a_new_shape_draws_at_the_next_frame` in [crates/vtium-render/tests/pointer_shapes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/pointer_shapes.rs)
- `a_pointer_files_picture_comes_after_pointer_char_and_before_vtiums_own` in [crates/vtium-render/tests/pointer_shapes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/pointer_shapes.rs)
- `a_shape_draws_its_own_picture_under_pointer_char` in [crates/vtium-render/tests/pointer_shapes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/pointer_shapes.rs)
- `each_shape_draws_its_picture_at_the_pointer` in [crates/vtium-render/tests/pointer_shapes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/pointer_shapes.rs)
- `the_gpu_path_draws_the_new_shape` in [crates/vtium-render/tests/pointer_shapes.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/pointer_shapes.rs)
- `a_full_stack_drops_its_bottom_entry` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `a_long_query_is_answered_for_its_first_names` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `a_push_skips_empty_names` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `a_query_answers_each_name_in_turn` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `a_set_names_the_shape_with_or_without_its_prefix` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `an_unknown_name_gives_the_default_shape` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `each_screen_keeps_its_own_stack_and_ris_empties_both` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `push_and_pop_keep_a_stack_of_shapes` in [crates/vtium-term/tests/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/pointer.rs)
- `a_new_shape_goes_to_the_renderer_and_the_plane` in [crates/vtium/src/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/pointer.rs)
- `a_new_shape_on_a_turned_display_is_turned` in [crates/vtium/src/pointer.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/pointer.rs)
- `a_pointer_shape_changes_the_screen_and_goes_to_the_plane` in [crates/vtium/src/term.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/term.rs)
- `each_symbol_of_the_shipped_file_draws_as_the_compiled_in_picture` in [tools/tier2/tests/pointer_file.rs](https://github.com/vtium/vtium/blob/main/tools/tier2/tests/pointer_file.rs)

## Where to read more

To follow the work on this row, see issue [#59](https://github.com/vtium/vtium/issues/59).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
