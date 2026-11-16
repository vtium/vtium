<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 8 hyperlinks

| Sequence | What it does |
|---|---|
| `OSC 8 ; params ; URI ST` | start a link |
| `OSC 8 ; ; ST` | end the link |

## Parameters

- `params`: `key=value` pairs separated by `:`. The key `id` joins cells printed apart into one link.
- `URI`: The link's address, from 1 to 2,083 bytes of printable ASCII with a scheme such as `https:`.

## What vtium does

vtium links each cell printed while the link is open, and keeps each distinct link once. It never opens a link. It underlines a link while the pointer is over it, copies it with Ctrl+click, and shows its address while Ctrl is held.

A URI past its bound, without a scheme, or with a byte outside printable ASCII closes the open link. RIS and a switch of screens end the open link, and an erased cell takes no link.

vtium refuses a space in a URI, which the specification allows, so that the address vtium shows is the one a copy carries. See [OSC strings and the Linux console's sequences in vtium-term's README](https://github.com/vtium/vtium/blob/main/crates/vtium-term/README.md#osc-strings-and-the-linux-consoles-sequences).

## Status

- **Status:** supported. vtium implements this as the specifications below define it, apart from the departure above.
- **Why:** programs such as ls and gcc print links the user can copy.
- **Note:** links are kept in their cells, underlined while the pointer is over them, copied with Ctrl+click, and, while Ctrl is held, shown with their address on the overlay row.
- **Specifications:** VTE and iTerm2 spec (Ghostty ext); contour clickable-links

## Tests

These tests check what vtium does with the sequences on this page.

- `a_hovered_link_draws_dashed_in_its_cells_underline_color` in [crates/vtium-render/tests/hover.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/hover.rs)
- `an_address_longer_than_the_row_is_cut_with_an_ellipsis` in [crates/vtium-render/tests/link_overlay.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/link_overlay.rs)
- `the_cursor_and_the_selection_are_not_drawn_under_the_overlay` in [crates/vtium-render/tests/link_overlay.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/link_overlay.rs)
- `the_overlay_draws_inverted_against_the_screen_under_reverse_video` in [crates/vtium-render/tests/link_overlay.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/link_overlay.rs)
- `the_overlay_draws_the_address_inverted_from_the_rows_first_column` in [crates/vtium-render/tests/link_overlay.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/link_overlay.rs)
- `the_overlay_goes_with_its_frame` in [crates/vtium-render/tests/link_overlay.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/link_overlay.rs)
- `a_cancelled_osc_8_changes_nothing` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_flood_of_links_strips_the_oldest_scrollback_lines_first` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_link_arriving_under_a_still_pointer_is_hovered` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_link_keeps_its_cells_through_a_wrap_the_scrollback_and_a_resize` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_link_sent_again_takes_the_entry_the_store_holds` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_sweep_after_an_erase_under_the_pointer_follows_the_bytes` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_sweep_frees_only_links_no_line_names` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_sweep_keeps_the_links_of_the_other_screen` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_uri_that_fails_its_rule_closes_the_open_link` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `an_id_that_fails_its_rule_leaves_the_link_with_none` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `erases_resets_and_screen_switches_end_or_drop_the_link` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `every_line_keeps_its_own_link_through_sweeps_and_strips` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `the_cells_printed_while_a_link_is_open_name_it` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `the_hovered_link_and_its_rows_are_in_the_frame` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `the_overlay_row_is_marked_when_its_uri_changes` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `the_overlay_shows_the_uri_on_the_bottom_row_or_the_top` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `the_stores_heap_stays_at_or_under_its_charge` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `when_the_screens_hold_the_bound_a_new_link_prints_unlinked` in [crates/vtium-term/tests/links.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/links.rs)
- `a_rotation_moves_the_hover_to_the_link_now_under_the_pointer` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `a_zoom_moves_the_hover_to_the_link_now_under_the_pointer` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `ctrl_click_on_a_link_copies_its_uri_and_the_middle_button_pastes_it` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `ctrl_shift_click_copies_a_link_while_the_program_tracks_the_pointer` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `the_pointer_over_a_link_hovers_it_and_ctrl_shows_its_uri` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `typing_that_hides_the_pointer_ends_the_hover` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `a_console_chord_moves_the_hover_to_the_console_it_shows` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `a_vt_switch_away_and_back_ends_the_overlay` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `releasing_ctrl_over_a_link_hides_its_overlay` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `the_head_losing_the_focus_shows_no_overlay` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `unplugging_the_keyboard_that_holds_ctrl_hides_the_overlay` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)

## Where to read more

To follow the work on this row, see issue [#40](https://github.com/vtium/vtium/issues/40).

To read why vtium does what it does here, see [ADR-1002, "3. What a link does"](../architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md#3-what-a-link-does).
