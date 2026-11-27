<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty animation frames

| Sequence | What it does |
|---|---|
| `APC G a=f,keys ; Pt ST` | send or edit a frame |
| `APC G a=a,keys ST` | control the animation |
| `APC G a=c,keys ST` | compose one frame onto another |

## Parameters

- `keys`: The command's keys, `key=value` pairs separated by commas, such as `a=T,f=100,i=1`. The key `a` names the action.
- `Pt`: The payload, in base64: the picture's data, or the name of a file or of shared memory to read it from.

## What vtium does

vtium keeps the frames as the program sent them and composes each one before it draws it. The frames count against a frame allowance, five times the console's image budget, and never evict a still picture. vtium plays the frames with their gaps and loops, and the key `s` starts and stops the animation.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs play animations through kitty's protocol.
- **Specifications:** kitty graphics-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `a_composition_lays_frame_r_onto_frame_c_and_gives_c_a_new_pixel_revision` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frame_from_the_helper_is_reserved_of_the_allowance_and_stored` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frame_rectangle_past_the_image_is_cut_to_it_as_kittys_is` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frame_short_of_its_rectangle_is_enodata_as_kitty_answers_it` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frame_to_a_missing_image_names_the_r_it_gave` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frames_buffered_payload_is_charged_to_the_frame_allowance_and_evicts_nothing` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frames_refusal_names_the_image_as_its_success_does` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frames_reply_names_its_frame_as_kitty_does` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_raw_frame_without_s_or_v_is_refused_as_kitty_refuses_it` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `an_animation_command_moves_its_images_use_clock` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `an_animation_control_is_answered_only_when_its_image_is_missing` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `an_edit_of_a_missing_frame_makes_a_new_frame_as_kittys_does` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `an_edit_of_the_root_frame_gives_it_a_new_pixel_revision` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `animation_commands_are_answered_as_a_put_is` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `d_f_deletes_every_frame_but_the_root_and_needs_an_image` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `frames_and_controls_fed_whole_or_cut_end_in_the_same_store_whatever_is_drawn` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `z_on_an_edit_of_the_root_frame_sets_the_roots_gap` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `z_on_an_edit_sets_the_gap_of_the_frame_it_edits` in [crates/vtium-image/src/protocol/kitty/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests.rs)
- `a_frame_in_chunks_is_answered_once_with_its_frame_number` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_frame_while_a_pictures_upload_is_open_is_its_next_chunk` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_put_a_composition_and_a_control_leave_the_upload_open` in [crates/vtium-image/src/protocol/kitty/tests/uploads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/protocol/kitty/tests/uploads.rs)
- `a_change_covering_half_the_picture_is_composed_when_it_arrives` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_change_frames_opacity_follows_its_base_and_its_layers` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_command_that_names_the_frame_on_show_makes_edits_of_other_frames_report_nothing` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_composed_frame_shows_its_base_and_its_layers_in_order` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_composition_answers_each_refusal_and_changes_nothing` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_composition_onto_a_base_composes_the_frames_laid_on_it_first` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_composition_onto_the_frame_on_show_reports_every_placement_of_the_image` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_frame_composed_when_shown_holds_the_bytes_of_the_same_frame_composed_on_arrival` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_frame_entry_names_its_pixels_by_image_frame_and_pixel_revision` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_frame_is_reserved_of_the_allowance_before_it_is_allocated_and_never_of_the_budget` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_frame_past_frame_chain_max_layers_is_composed_when_it_arrives` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_frame_that_freeing_cannot_fit_is_enospc_and_frees_nothing` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `a_small_picture_is_composed_when_its_frame_arrives` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `an_animation_holds_at_most_frames_max_frames` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `an_edit_of_a_base_composes_the_frames_laid_on_it_first` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `an_edit_of_a_frame_reports_the_placements_once_the_clock_picks_the_frame` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `an_edit_of_the_frame_on_show_reports_every_placement_of_the_image` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `an_image_freed_gives_its_frames_allowance_back` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `controls_set_the_state_and_move_the_stamps_by_what_they_set` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `deleting_the_frames_leaves_the_root_stopped_and_moves_both_stamps` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `frames_freed_for_the_allowance_show_the_root_and_edits_of_other_frames_report_nothing` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `the_allowance_frees_other_animations_least_recently_used_first_and_never_its_own_or_a_root` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `the_cache_drops_the_frame_asked_for_longest_ago` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `the_cache_is_charged_its_full_bound_whatever_it_holds` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `the_pixel_identity_moves_with_the_frame_shown_and_with_a_change_to_it` in [crates/vtium-image/src/store/anim/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-image/src/store/anim/tests.rs)
- `an_animation_looping_over_its_frames_keeps_one_copy_for_each_frame` in [crates/vtium-render/tests/picture_frames.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/picture_frames.rs)
- `an_animation_too_large_for_the_cache_stays_within_its_bound` in [crates/vtium-render/tests/picture_frames.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/picture_frames.rs)
- `an_edit_to_the_frame_on_show_redraws_its_cells_from_the_new_pixels` in [crates/vtium-render/tests/picture_frames.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/picture_frames.rs)
- `the_same_frame_shown_twice_is_scaled_once` in [crates/vtium-render/tests/picture_frames.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/picture_frames.rs)
- `two_placements_at_two_sizes_share_a_frame_and_its_edits` in [crates/vtium-render/tests/picture_frames.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/picture_frames.rs)
- `two_placements_showing_different_frames_in_one_draw_each_show_their_own` in [crates/vtium-render/tests/picture_frames.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/picture_frames.rs)
- `two_snapshots_that_differ_only_in_the_frame_shown_draw_each_frames_pixels` in [crates/vtium-render/tests/picture_frames.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/picture_frames.rs)
- `a_gapless_frame_is_passed_over` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `a_loading_animation_goes_on_to_a_frame_that_arrives` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `a_looping_animation_shows_each_frame_for_its_gap` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `a_placeholder_picture_plays` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `a_stopped_animation_arms_no_timer` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `a_year_hidden_costs_a_few_gaps_to_catch_up` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `an_animation_whose_loops_ran_out_is_stopped` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `hidden_the_clock_arms_no_timer_and_shown_again_it_skips_to_the_time` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `one_frame_that_shows_wakes_the_loop_for_nothing` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `the_clock_answers_each_command_by_its_stamps` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `the_loops_run_out_and_the_animation_stops_on_its_last_frame` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `the_snapshot_shows_the_clocks_frame_asked_once_per_image` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `the_stores_current_frame_is_composed_once_while_the_clock_plays_others` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `the_timer_fires_no_more_often_than_the_floor` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `with_nothing_animating_the_clock_never_wakes_the_loop` in [crates/vtium/src/animation/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/animation/tests.rs)
- `a_hidden_consoles_clock_arms_no_timer` in [crates/vtium/src/app/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/tests.rs)
- `a_vt_switch_away_stops_the_clock_and_back_it_skips_to_the_time` in [crates/vtium/src/app/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/tests.rs)
- `the_loop_wakes_for_an_animations_frames_and_never_for_a_still_picture` in [crates/vtium/src/app/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/tests.rs)
- `the_display_draws_the_frame_the_clock_chose` in [crates/vtium/src/term.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/term.rs)
- [tools/conformance/vtium/cases/kit-anim-refusals.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-anim-refusals.case)
- [tools/conformance/vtium/cases/kit-spec-anim-numbers.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/kit-spec-anim-numbers.case)

## Where to read more

To follow the work on this row, see issue [#32](https://github.com/vtium/vtium/issues/32).

To read why vtium does what it does here, see [ADR-1102 D1](../architecture/image/ADR-1102-kitty-animation-frames-are-stored-as-the-program-sent-them-and-composed-before-drawing-and-a-clock-outside-the-store-plays-them.md#d1-how-frames-are-stored-and-charged).
