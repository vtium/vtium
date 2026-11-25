---
contract: adr/v1
kind: decision
verb: change
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "something that needs to happen for the 0.1.0 non alpha release is a no-feature-change split of the larger files into smaller crates/libraries. everything has gotten quite big. I also wonder if we would benefit from doing that earlier"
    via: "the maintainer in the steering session on 2026-11-25, in chat"
  - operator: aaronsb
    level: directed
    said: "Record, then vtium now (Recommended)"
    via: "the maintainer through the question tool in the steering session on 2026-11-25: a record of the target layout comes first, then one card splits crates/vtium's app.rs, helper.rs and check.rs into modules before the privilege split's next cards start, and the other crates follow one at a time, each when no card changes it, all before 0.1.0"
  - operator: aaronsb
    level: directed
    said: "Modules, crates at seams (Recommended)"
    via: "the maintainer through the question tool in the steering session on 2026-11-25: split into modules everywhere, and make a new crate only where this record names a clean boundary, such as the broker and its protocol"
  - evidence: "vtium's integration branch at 2b29781e7, measured 2026-11-25: the Rust files under crates/ hold 535,009 lines. 57 files are over 1,500 lines and hold 188,946 of them: 34 source files (109,779 lines, of which 40,305 are unit tests in `#[cfg(test)]` blocks and 69,474 are code), 17 test files (56,636 lines), 5 generated tables (19,521 lines) and one hand-written table, vtium-settings' keys.rs (3,010 lines). 19 of the source files hold more than 1,500 lines of code, and 11 more hold between 1,000 and 1,500. Another 20 files of 1,500 lines or fewer also hold more than 1,000 lines of code. ADR-1513-files.tsv lists each file over 1,500 lines."
  - evidence: "the same tree, read 2026-11-25: crates/vtium/src/broker/protocol.rs uses only std and rustix. Outside the broker's own modules, only main.rs reaches the broker, through `broker::SPLIT`, `broker::drawer` and `broker::split`. The fuzz target `protocol` includes protocol.rs by its file path, because the file sits inside the vtium binary. The broker's other modules use vtium-platform's event loop and nothing else of the vtium crate."
  - evidence: "the same tree, read 2026-11-25: crates/vtium/src/deferred.rs and its modules fits.rs, joined.rs and data.rs use vtium-font, vtium-term and one item of the vtium crate, the type alias `crate::font::DisplayFace`, which is `Box<dyn Face + Send>`. They take the `bidi` feature in 27 places. app.rs, app/batch.rs, app/batch_login.rs, head.rs, helper.rs, font.rs and check.rs use the store."
  - evidence: "the same tree, read 2026-11-25: the glyph helper's wire format is written twice. vtium-config's serve.rs defines the request magic, the request kinds, the frame kinds and their bounds as public constants, with the request reader and the frame writer. The console's helper.rs keeps its own copies of the magic, the kinds and the bounds, with its own request encoder and frame reader, because the console links no part of vtium-config. The vtium crate takes vtium-config only as a dev-dependency, so that tests/boot.rs can run the real helper."
  - evidence: "the same tree, read 2026-11-25: serve.rs's code uses vtium-config's needs, outline, seccomp, shaping, sources, pointer_svg, names, frit, describe, decode, xcursor, hex, bdf, made, font and child modules. check.rs and check/resources.rs use the vtium crate's config, console, notice, helper, font, deferred, head::images and app::locale."
  - evidence: "`make minspec` on the same tree, 2026-11-25: the minspec binary is 4,351,440 bytes, 39,472 bytes under tier 3's budget of 4,390,912. The release profile, which minspec inherits, builds with `lto = true` and `codegen-units = 1`. The stripped binary still holds the source path of each file whose code can panic, 202 of them, such as `crates/vtium/src/head/route.rs`, because a panic's location names its file."
  - evidence: "the same tree, read 2026-11-25: docs' claim files name 47 claims in crates/vtium/src/app.rs across 17 files, 21 in helper.rs across 4 files and 2 in check.rs across 2 files. ADR-1000's `tests` column names 14 tests by their file. crates/vtium/README.md's file table and docs/systemd.md name app.rs, helper.rs or check.rs."
  - evidence: "the steering session's briefs for the privilege split's second and third cards, read 2026-11-25: the card that moves the devices and the VT into the broker adds a broker backend for the drawer in vtium-platform, at src/vt/broker.rs, beside the libseat backend. The card that moves the process jobs into the broker changes app.rs and app/*, helper.rs's launcher, child.rs, survive.rs, power.rs, blank.rs, kernel.rs and getty.rs."
  - operator: aaronsb
    level: directed
    said: "I'd like to be able to make sure that our split will afford future use of these libraries not as a whole (someone could take the rendering engine for example)"
    via: "the maintainer in the steering session on 2026-11-25, relayed by the steering session"
  - evidence: "the same tree, read 2026-11-25: the distinct paths each library crate's non-test code names in another vtium crate. vtium-render names 33 of vtium-term (the frame model only), 31 of vtium-font and 5 of vtium-image. vtium-term names 9 of vtium-parse and 28 of vtium-image. vtium-platform names 16 of vtium-image, all in the image helper's host. vtium-settings names 2 of vtium-font, 9 of vtium-term and 12 of vtium-render's filter declarations. vtium-devices names 5 of vtium-platform and 6 of vtium-settings, vtium-clipboard 2 of vtium-term, and vtium-image 2 of vtium-parse. vtium-term's frame model (cell, line, style, palette, damage, order, frame, unicode, bounds, links, placeholder) uses no vtium-parse item, and the rest of vtium-term reaches it through 91 pub(crate) items."
  - evidence: "the same tree, 2026-11-25: `cargo check -p <crate> --lib` passes alone, with no features and with all features, for vtium-parse, vtium-font, vtium-image, vtium-term, vtium-render, vtium-platform, vtium-settings and vtium-devices. `make api` passes; rustdoc lists 17 public items for vtium-parse, 153 for vtium-font, 137 for vtium-image, 108 for vtium-term, 121 for vtium-render, 189 for vtium-platform, 422 for vtium-settings, 12 for vtium-devices and 689 for the vtium crate's library. No crate sets missing_docs or publish."
  - precedent: ADR-1400
  - precedent: ADR-1503
  - precedent: ADR-1505
  - precedent: ADR-1100
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "yes ~1000 and tests out at 1500 with warnings over not hard stops"
    via: the question tool, 2026-11-25, on the reading page of ADR-1513, answering in the maintainer's own words
    covers: [the size]
  - operator: aaronsb
    said: "Protocol crate first (Recommended)"
    via: the question tool, 2026-11-25, on the reading page of ADR-1513; the option's wording is the steering session's
    paraphrase: true
    covers: [the broker's messages]
  - operator: aaronsb
    said: "Make it a crate (Recommended)"
    via: the question tool, 2026-11-25, on the reading page of ADR-1513; the option's wording is the steering session's
    paraphrase: true
    covers: [the glyph store]
  - operator: aaronsb
    said: "Two halves (Recommended)"
    via: the question tool, 2026-11-25, on the reading page of ADR-1513; the option's wording is the steering session's
    paraphrase: true
    covers: [the first card]
  - operator: aaronsb
    said: "we just need to be careful about not changing code yet. We should just consider the logical splits we have now. actually making it work more independent later is not in this set of split cards"
    via: in chat, 2026-11-25
status: proposed
date: 2026-11-25
deciders:
  - aaronsb
related:
  - ADR-1400
  - ADR-1503
  - ADR-1505
  - ADR-1100
  - ADR-1200
---

# ADR-1513: vtium's large source files split into modules with no change in behaviour, and a new crate comes only at a clean seam

## Summary

- **Decided:** Before 0.1.0, vtium's large source files split into modules by what their code does. A file aims at about 1,000 lines of code, and unit tests move to a `tests.rs` beside their module once a file passes 1,500 lines (the maintainer, `considered`, 2026-11-25). A file whose code passes 1,500 lines draws a warning, not a hard stop: its crate's split card and any check report it, and nothing fails on it. A file between 1,000 and 1,500 lines of code splits when its crate's split card finds a natural cut. Generated tables, test files and the key table are exempt (D2). A split changes no behaviour. The same tests pass, no golden moves, the public items stay the same, and the minspec binary grows by at most 2 KiB for each card, or 4 KiB for a card that makes a crate. A reviewer checks in the diff that moved code is only moved (D1). A new crate comes only where this record shows a clean boundary. The broker's protocol becomes a crate before the drawer's broker backend is built, because that backend lives in vtium-platform (D4; the maintainer, `considered`, 2026-11-25). The deferred glyph store becomes a crate once the privilege split's first slice has landed (D4; the maintainer, `considered`, 2026-11-25). The first card splits crates/vtium's app.rs, helper.rs and check.rs before the privilege split's next cards start, as two halves that run side by side (D6; the maintainer, `considered`, 2026-11-25). Proposed: each split card draws its module boundaries along the lines a later separation of a library would follow, such as vtium-render's reading of vtium-term's frame kept apart from its drawing. Making the libraries independent is later work outside these cards (D7). The other crates follow one at a time, each while no other card changes its files (D5).
- **Trades away:** Every split card moves thousands of lines, so `git blame` without `-C` and `git log` without `--follow` stop at the move, and a branch that is open across a split meets a conflict it can only resolve by moving its change by hand. The splits use up to about 4 KiB of the 38 KiB left under tier 3's size budget, mostly for the paths of about 65 new files in the console's crates. The split cards take the place of feature work for several waves, and the claim files, crate READMEs and ADR-1000's `tests` column change with each one.
- **One-way?** No. A module split is reversible: the code moves back in one commit. A new crate is cheap to fold back while nothing outside the workspace uses it, and nothing does.
- **Probes:**
  - *Confident (the size):* a file aims at about 1,000 lines of code, a file must split once its code passes 1,500 lines, and unit tests move out once a file passes 1,500 lines in all. Generated tables, test files and the hand-written key table keep their size (D2). Lean: these numbers, because a card's brief can then hand an agent whole files it reads in one pass. Is about 1,000 lines the size you want? (Answered 2026-11-25; see `considered`.)
  - *Not confident (the broker's messages):* the messages between the broker and the drawer move into a small crate of their own, `vtium-broker-protocol`, in a card that runs after the first split and before the card that moves the devices and the VT into the broker (D4, D5). Lean: yes, because that card builds the drawer's side of the protocol inside vtium-platform, which cannot use the vtium crate. The other way is to put that backend in the vtium crate, which splits the device code across two crates. Should the protocol crate come first? (Answered 2026-11-25; see `considered`.)
  - *Not confident (the glyph store):* the store of glyphs the font helper sends, crates/vtium/src/deferred.rs and its tables, becomes a crate of its own once the privilege split's first slice has landed (D4). Lean: yes, because only one type alias crosses its boundary, and the vtium crate then compiles about 8,000 fewer lines on each change. Do you want this crate, or should the store stay a module of the console? (Answered 2026-11-25; see `considered`.)
  - *Not confident (the first card):* the first card is cut in two halves that run at the same time, one for app.rs and one for helper.rs and check.rs (D6). Lean: two halves, because app.rs alone moves about 10,000 lines and its file set touches no file of the other half. Is that how you want your "one card" run? (Answered 2026-11-25; see `considered`.)
- **Inversion:** Between "one file for each subject, however large it grows" and "a crate for each subject". vtium splits files into modules by what the code does, and makes a crate only where little crosses the boundary.

> Edited 2026-11-25: the maintainer answered the size probe "yes ~1000 and tests out at 1500 with warnings over not hard stops" (`considered`), so a file past 1,500 lines of code draws a warning instead of having to split. The Decided item began: "Before 0.1.0, every source file in crates/ that holds more than 1,500 lines of code splits into modules by what its code does. A file aims at about 1,000 lines of code. A file between 1,000 and 1,500 lines of code splits when its crate's split card finds a natural cut. Unit tests move to a `tests.rs` beside their module once a file passes 1,500 lines."

## Context

vtium grew one feature at a time, and several files took in each feature that touched their subject. crates/vtium/src/app.rs holds 10,317 lines, vtium-config's serve.rs 10,144, and crates/vtium/src/helper.rs 8,099. The maintainer asked for a split of the larger files with no change in features before the 0.1.0 release that is not an alpha (`basis`).

A large file costs more than its reading time. The working loop gives each card a set of files that no other card edits at the same time (AGENTS.md, "Cooperating across worktrees"). When one file holds the event loop, the VT's events, the devices, the sessions, the font helper's asks and the drawing, two cards that change different parts of it still own the same file. The privilege split's next two cards both change app.rs, and both add lines to the broker's dispatch, so their briefs have to fence each other's edits by hand. An agent that reads a 10,000-line file to change 50 lines also spends its context on the rest.

Rust lets one type's methods sit in several modules. A child module can write `impl App` and read `App`'s private fields, and app.rs already does this for app/hand_over.rs and app/survive.rs. A split into modules is therefore mostly a move. What changes beyond the moved lines is the module declarations, the `use` lines and the visibility of the moved items.

ADR-1505 sets the verification levels a card runs. ADR-1503 makes vtium's own goldens the judge of its output. ADR-1400 D4 plans the broker, and its slice list plans a card of its own that separates the terminal side from the drawing side, beside the second slice. That card and this layout touch the same files, so D4 says how they fit together.

## Decision

### D1: the rule for a split

A split changes no behaviour. A split card proves this with five checks, each run before its first move and again at its end.

1. **The same tests pass.** The card lists every test the crate runs, with `cargo test -p <crate> --all-features -- --list` and again with no features, before and after. The two lists are the same, apart from a test's module path when its module moved. A test that stays in its module keeps its name: `#[cfg(test)] mod tests;` with the tests in `tests.rs` keeps the path `app::tests::...`. Each test passes, and no test's body changes apart from its `use` lines. The integration tests under `tests/` change only their `use` paths.
2. **No golden moves.** The card runs the tiers that ADR-1505's V1 names for the code it moves: `make conformance` for vtium-parse, vtium-term or the replies, `make tier2` for drawing and fonts, and `make tier3-minimal` once for the VT, the sessions, DRM/KMS or the GPU context. Every golden compares equal. No golden or provisional list changes in a split card.
3. **The public items stay the same.** The card runs `make api` before and after and compares the item list rustdoc writes for each crate it touches, `target/doc/<crate>/all.html`. A public item keeps its path: when it moves, its old module re-exports it with `pub use`. The list changes only where this record names a change, as the new crates of D4 do.
4. **The minspec binary stays within its allowance.** The card runs `make minspec` before and after. A module split may grow the binary by at most 2,048 bytes, and a card that makes a new crate by at most 4,096 bytes. The binary also stays under tier 3's budget. The allowance comes from how the binary is built. The release profile links with `lto = true` and one codegen unit, so module and crate boundaries do not change which code the compiler generates. A move adds the path of each new file, about 40 bytes, which a panic's location carries. A move can also change the order in which the optimizer meets the functions, and so where it inlines a call, which moves a function's size by a few hundred bytes either way. Growth past the allowance means more than that changed, and the card finds out why before it merges.

> Note 2026-11-25: the steering session doubled the allowance after the first split card measured what a move costs. The split of app.rs into twelve modules grew the minspec binary by 1,120 bytes: 608 bytes of the new files' paths, 56 of unwind tables, and 464 of code, because the optimizer inlined one more formatting call into `App::turn` once the functions sat in a new order; every other function kept its size. The paragraph's allowance read: "A module split may grow the binary by at most 1,024 bytes, and a card that makes a new crate by at most 2,048 bytes." Its fourth and last sentences read: "so module and crate boundaries do not change the code the compiler generates. What a move does add is the path of each new file, about 40 bytes, which a panic's location carries. A split into 20 files therefore costs under 1 KiB. Growth past the allowance means the code generation changed, and the card finds out why before it merges." The Summary's "1 KiB for each card, or 2 KiB for a card that makes a crate" changed with it.
5. **The gates of what moved pass.** `cargo fmt --all --check`, `cargo clippy -p <crate> --all-features --all-targets -- -D warnings` and the same with no features, `docs/scripts/claims`, `docs/scripts/coverage`, and the fuzz targets the crate's fuzz README names, each built once.

A reviewer reads the move with `git diff --color-moved=dimmed-zebra --color-moved-ws=allow-indentation-change <base> <head>`. Moved lines show dimmed, so only the lines that are not moved need reading. Each of those lines is one of: a `mod` line, a `use` line, a visibility change (`pub(super)` on an item or a field), an `impl X {` that opens a moved block and its closing brace, a `pub use` that keeps a public path, or the `//!` comment that says what a new module does. Any other line is an edit, and an edit does not belong in a split. Where a cut needs a small change first, such as a helper that two new modules share, that change is a commit of its own before the move, with every test passing at that commit, and the card's report names it.

Moved items take the narrowest visibility that compiles. A private method that moves into a child module becomes `pub(super)`, because the parent and its other children call it. A type that moves into a child module while other modules read its fields gives those fields `pub(super)`. Nothing becomes `pub` in a split.

History stays readable. Each split moves code in one commit that only moves, as above, and any later change comes in a later commit. The commit's message lists each new file and what it took. `git blame -C -C <file>` then shows each moved line's original commit, and `git log -L` follows a function into its new file. A split never mixes a move and an edit in one commit, because `git blame -C` cannot see through an edited line. A split card updates the claim files that name a moved symbol, the crate's README where it names a moved file, and ADR-1000's `tests` column through `docs/scripts/coverage --update`, in a commit after the move. The `basis` entries of the decision records stay as written, because each one records what was read at a commit.

### D2: the size a file aims for

A source file aims at about 1,000 lines of code outside its unit tests (the maintainer, `considered`, 2026-11-25). A file whose code passes 1,500 lines draws a warning, not a hard stop. The card that splits its crate splits it by what its code does, and that card and any check that counts lines report it, but no gate fails on a file's size. A file between 1,000 and 1,500 lines of code splits when the card that splits its crate finds a natural cut, and otherwise stays whole. The band keeps a file from splitting over a few hundred lines, and keeps files that are already modules of an earlier split, such as vtium-term's screen/edit.rs, from splitting again for little gain. A file that passes 1,500 lines in all moves its unit tests into a `tests.rs` beside it, as `console/tests.rs` and `config/tests.rs` already do.

A check of file sizes, if one is built, lists each file over the aim and each file whose code passes 1,500 lines as a warning, and never fails a gate, as the gates' report of the minspec binary's size does.

> Edited 2026-11-25: the maintainer said "yes ~1000 and tests out at 1500 with warnings over not hard stops" (`considered`). The paragraph began: "A source file aims at about 1,000 lines of code outside its unit tests. A file whose code passes 1,500 lines splits by what its code does. A file between 1,000 and 1,500 lines of code splits when the card that splits its crate finds a natural cut, and otherwise stays whole."

The aim is 1,000 lines because that is a file an agent reads whole in one pass, so a card's brief can name the files it owns and the agent reads each one completely. It is also about the size at which one file holds one subject, so two cards rarely need the same file. AGENTS.md sizes each card to finish in well under half of an agent's context, and smaller files keep a brief's file set honest about what the card touches.

Four kinds of file are exempt:

- **Generated tables.** `crates/vtium-config/src/shaping/data.rs`, `crates/vtium-font/src/letters/data.rs`, `crates/vtium/src/deferred/data.rs`, `crates/vtium-config/src/cover.rs` and `crates/vtium-config/src/tui/contract_text.rs`. A generator writes each one and nobody edits it by hand, so its size costs no reader anything. Splitting one would only complicate its generator.
- **Test files.** The files under a crate's `tests/`, each `tests.rs`, and test-only stand-ins such as vtium-platform's `display/fake.rs`. A reader takes one test at a time, and each test stands alone. A test file moves with its module and splits only when a card that changes it finds a natural cut. Each file under `tests/` is a test binary of its own, so splitting one adds a link step to every test run.
- **A hand-written table.** vtium-settings' `keys.rs` holds the key table: a constant for each of its 140 keys, and the list `KEYS`. vtium's example configuration, its man pages and vtium-config's help come from it. One table in one file lets a reader see every key in order.
- **One function that cannot be cut.** A function whose body is one match over its cases, or one long sequence of steps, stays whole, because cutting it would change its code. It goes into a module of its own, so the module around it still keeps to the aim. vtium-config's `Engine::glyphs`, about 630 lines, is one.

ADR-1513-files.tsv lists every file over 1,500 lines as measured on 2026-11-25, with its kind, its test lines, its code lines and its plan. Of the 57 files, 19 pass 1,500 lines of code and draw the warning, so their crates' cards split them, 11 sit in the band where a split is optional, 4 only move their tests out, and 23 are exempt. Twenty more files of 1,500 lines or fewer hold between 1,000 and 1,500 lines of code, such as vtium-term's terminal.rs and screen.rs, vtium-platform's seat.rs and input.rs, and vtium-image's protocol/kitty.rs and store/anim.rs. They are in the same band, and the card that splits their crate weighs each one.

### D3: the layout of each crate

The modules below are named from the code as it stands on 2026-11-25. Each split card checks its file against this list first, because code moves between the record and the card. A card may adjust a module's edge by a few functions, and it reports each change. Unless a line says otherwise, each cut is clean today: the moved code compiles with only the changes D1 allows. A file marked *optional* holds between 1,000 and 1,500 lines of code, so its cut is the one its crate's card makes if it finds the cut natural.

**crates/vtium**

- **app.rs** (5,980 lines of code). app.rs keeps `App`'s fields, `App::new`, the `set_*` methods, the read accessors, `Exit`, `SeatVt`, the login scope and the small helpers (`after`, `scale`, `locale`, `physical_memory`), about 1,000 lines. `App` stays in app.rs, so each child module reads its private fields.
  - `app/displays.rs`: a display joins a head and leaves it. `add_video`, `remove_video`, `let_go_of_gone_cards`, `identity`, `head_for`, `darken`, `add_display`, `relayout`, `bound_pointers`, `give_colors`, `give_colors_again`, `draws_on_gpu`, `dpi`, `head_of`.
  - `app/devices.rs`: the VT's events and the devices'. `vt_event`, `video_event`, `device_events`, `release_now`, `device_event`, `add_video_device`, `settled_videos`, `open_video`, `wrap_video`, `depth`.
  - `app/sessions.rs`: each console's session. `first_start`, `place`, `truecolor`, `spawn`, `start_session`, `watch`, `open_console`, `child_exited`, `gave_up`, `session_turn`.
  - `app/fonts.rs`: the font each head draws and its zoom. `choose_font`, `choose_font_adding`, `head_config`, `configured_font`, `own_cell`, `own_cell_within`, `share`, `prune`, `font_name`, `switch_font`, `zoom`, `zoom_struck`, `zoom_stops`, `take_size`, `return_font`, `reread_font_keys`, `retry_late_cell`, `configured_fit`, `draw_height`, `height_at`, `same_font`.
  - `app/glyph_asks.rs`: what the console asks the font helper. `ask_glyphs`, `ask_visible`, `helper`, `helper_wake`, `helper_turn`, `waits_for_cell`, `end_cell_waits`, `ask_cells_again`, `late_cell`, `late_refusal`, `frit_cells`, `shown`.
  - `app/blanking.rs`: the blank and the terminals' activity. `activate`, `deactivate`, `any_display`, `blank_now`, `user_input`, `activity`, `blank_request`, `outside_order`, `wake`, `restart_blank`, `lose_every_view`.
  - `app/notices.rs`: the notices on displays no head holds. `wake_notices`, `show_notices`, `silence_notices`, `notice_cycle`, `park_notice`.
  - `app/guard.rs`: the guards after a device comes or goes. `guard`, `screen_shows`, `screen_notice`, `unlight`, `attachment`, `hang_up`.
  - `app/input.rs`: input devices, focus and the pointer. `report_focus`, `take_focus`, `move_focus`, `add_input`, `remove_input`, `rehover`, `set_natural_scrolling`, `focus_head`, `take_focus_move`, `input_event`, `device_input`, `drained_input`, `deliver`, `release`, `send_release`, `let_go_of_held_keys`, `rescue`, `rescue_line`, `pointer_event`, `pointer_to`, `dismisses`, `KeyFrom`, `KeyShown`, `term_key`.
  - `app/keys.rs`: the key order and the heads' chords. `key`, `chord`.
  - `app/turn.rs`: the end of each turn and the timers. `turn`, `timers`, `animate`, `report`, `exiting`, `next_wake`.
  - `app/drawing.rs`: the render threads and the frames. `render_timer`, `measure`, `draw`, `osd`, `let_go_of_painters`, `painter`, `gpu_source`, `frames`, `draw_head`, `left_gpu`, `failed_gpu_frame`, `gpu_notes`, `refused_gpu_display`, `back_on_gpu`, `display_lines`, `display_line`, `power_down`, `render_timer_line`.
  - `app/tests.rs`: app.rs's unit tests, 4,188 lines, with the test helpers that app/rapid.rs and the other children import from `super::tests`.
- **helper.rs** (3,680). helper.rs keeps the module's account of the helper, `Failures`, `Helper` and its public methods from `new` to `retain`, about 950 lines.
  - `helper/sandbox.rs`: what the console logs of the helper's sandbox and the options that turn it off. `failure`, `seccomp_reason`, `domain_reason`, `because`, `sandbox_notice`, `log_sandbox_once`, `sandbox_on`, `sandbox_args`.
  - `helper/launcher.rs`: how the console starts the helper and checks the account it runs as. `nobody`, `Command`, `Launcher`, `active_request`, `status_ids`, `holds_account`, `font_dirs`, `program`, `open_program`, `refusal`.
  - `helper/wire.rs`: the requests the console writes and the frames it reads. The request and frame constants, `put32`, `len32`, `FritText`, `encode`, `OwnCell`, `NoCell`, `cell_answer`, `Event`, `head`, `mapped_runs`, `frame`, `coverage_answer`, and `Helper::send` and `Helper::request`.
  - `helper/cells.rs`: what the console knows of each cell. `Answer`, `Bits`, `part_of`, `codepoints`, `Fill`, `Clusters`, `CellState`, `Ask`. Their fields take `pub(super)`, because `Helper`'s methods read them.
  - `helper/process.rs`: the helper's process and its threads. `Running`, `Helper::start`, `fail`, `stop`, `submit`, `post`, `Drop for Helper`, `thread`, `reap`.
  - `helper/search.rs`: the search for the source that makes the helper fail. `Search`, `Open`, `Suspect`, `chance`, `no_chance_left`, `bit`, `leave_out_request`, `source_lines`, and `Helper::restart`, `blame`, `start_again`, `try_sources`, `tried`, `settle`, `in_use`, `leave_out`, `search_event`.
  - `helper/answers.rs`: taking what the helper sends. `poll`, `glyphs`, `quiet`, `warn`, `sandboxed`, `store_full`, `done`, `ask_bold_early`, `ask_bold_waiting`, `stop_taken`, `filled`, `closed`, `resend`, `send_deferred`, `escaped`.
  - `helper/tests.rs`: 4,419 lines of tests, with the test stand-ins that open the `#[cfg(test)]` part of the file.
- **check.rs** (1,461, *optional*, split by the first card). check.rs keeps `run`, `Host`, `Machine`, `VtsEnabled`, `Outcome`, `outcome`, `outcome_with` and `main`.
  - `check/frits.rs`: the frits a draft adds and how the check judges them. `DraftFrits`, `Found`, `with_frit_name`, `frit`, `named_frit`, `Judged`, `judge`, `frit_layers`, `font_name_frit`, `look_frits`, `squeeze_note`.
  - `check/notes.rs`: the warnings and notes beyond the reports. `semantics`, `init_takes`, `semantics_under`, `warnings`, `counts`, `warnings_of`, `notes`, `login_image_note`, `look_login_images`, `font_paths_notes`, `font_path_note`, `helper_note`, `image_helper_note`, `sandbox_note`, `refusal_note`.
  - `check/conf.rs`: the check of vtium.conf's sections against the machine. `Helpers`, `HeadCheck`, `same_file`, `Settings`, `FontSources`, `Squeeze`, `conf_file`, `runs_file`, `denied`, `errors`, `findings`, `vt_findings`, `connector_path_rules`, `reported`, `device_findings`.
  - `check/tests.rs`: 4,225 lines. check/resources.rs stays as it is.
- **deferred.rs** (2,174) becomes a crate (D4), whose modules are `shown` (`Shown`, `read_shown`, `shown_index`, `read_doubled`, `doubled_of`, `variant_of`, `settle`), `unicode` (`Sides`, `is_mark`, `is_ignorable`, `droppable`, `class`, `compose`, `decompose`, `decomposition`, `precomposed`, `spacing_side`), `layers` (`Layers`, `overstrike`, `drawn_beside`, `ink_rows`, `ink_columns`, `Drawn`, `Part`, `after_linker`, `in_cells`, `beside`), `store` (`Entry`, `draw_text`, `rewrite`, `ink_bytes`, `pack`, `Generated`, `CellLines`, `lock`, `Held`, `Fresh`) and `face` (`DeferredFace`, `Builtins`, `presents_as_text`), beside fits.rs, joined.rs and data.rs.
- **font.rs** (1,855). font.rs keeps the sprite policy and the sizes. `font/frits.rs`: a frit's name, its trust, its layers and its lines. `font/batch.rs`: `Sources`, `Came`, `Wanted`, `Held`, the batch and the gate. `font/named.rs`: `wanted`, `frit_named`, `builtin_named`, `by_name`, `font_folders`, `trusted_folder`, `one_source`, `qualifying`. `font/chosen.rs`: `Chosen`, `Rung`, `zoom`. `font/tests.rs`.
- **console.rs** (2,010), the `vtium console` subcommand. console.rs keeps the command line's parse and `run`. `console/systemd.rs`: `Systemd`, `RunningSystemd`, `Host`, `Link`, `UnitFile`, `Record` and the unit names. `console/state.rs`: `State`, `Enabled`, `templates`, `enabled_vts`, `running_vts` and `State`'s methods. `console/verbs.rs`: `Run` and each verb's steps. console/tests.rs already exists.
- **head.rs** (2,160) and **term.rs** (1,800) split along ADR-1400's seam between the terminal side and the drawing side (D4). term.rs keeps the session side, `Term`, `Echoes`, `Shown` and `Consumer`, and `term/snapshot.rs`, `term/overlay.rs` and `term/view.rs` take `Snapshot`, `Overlay` and `View` with the frame types. head.rs keeps `Head`, its consoles and its pty. `head/overlays.rs` takes the notices, the on-screen display and the kernel's messages, `head/pointer.rs` the pointer, links and selection, and `head/start.rs` `Frit`, `Start`, the rules and `place_display`.
- **painter.rs** moves its tests out. **broker/protocol.rs** becomes a crate (D4).

**crates/vtium-config**

- **serve.rs** (5,351), the glyph helper. serve.rs keeps `serve`, `Work`, `answer_requests` and `read_requests`, the helper's loop. `serve/wire.rs`: the constants, `FritText`, `ServeRequest`, `Fields`, `Request`, `DescribeAsk`, `ActiveAsk`, `read_request`, the code functions and the frame writers. `serve/pointer.rs`: `PointerAsk`, `PointerPicture` and the shape codes. `serve/engine.rs`: `Engine`, its loading, `LeftOut`, `DrawsNothing`, `Found` and the source readers. `serve/engine/metrics.rs`: the own cell, the sizes, the baselines and the lines. `serve/engine/draw.rs`: the `draw_*` methods and `glyphs`, the one function of D2's last kind. `serve/bitmap.rs`: the bitmap placement helpers. `serve/sandbox.rs`: `limits`, `refuse_root`, the landlock domain, the socket and fallback filters, `Sandboxed`, `shed_inherited` and `command`. `serve/tests.rs`: 4,793 lines. `Engine` stays in serve/engine.rs, so its child modules read its fields.
- **outline.rs** (2,819) moves its inline modules `strike` and `raster` into files once swash's removal has rewritten them.
- **frit.rs** (1,610): frit.rs keeps the types of a frit, `frit/parse.rs` takes `impl Frit`'s parser, and `frit/needs.rs` the layers, the cover and the needs.
- **apply.rs** (2,014): apply.rs keeps `apply` and its steps. `apply/draft.rs` takes `Targets` and the draft's writing, `apply/owners.rs` the routes and the owners' checks, `apply/frits_dir.rs` `FritsDir` and the system font's install, and `apply/scratch.rs` `Scratch`.
- **specimen.rs** (1,724): `specimen/helper.rs` (the specimen's helper process), `specimen/sample.rs` (`Sample` and its asks), `specimen/shot.rs` (`Shot`, the filters and the screen) and `specimen/paint.rs` (the layout and the paint).
- The TUI: **tui/model.rs** (1,547) moves `App`'s replies into `model/replies.rs` and its keys into `model/keys.rs`. **tui/tabs/flow.rs** (1,488, *optional*) moves the Flow tab's keys into `flow/keys.rs` and its edits into `flow/edit.rs`. **tui/flowdraw.rs** (1,447, *optional*) moves the window and its layers into `flowdraw/layers.rs`. **tui/flowview.rs** (1,425, *optional*) moves the rows into `flowview/rows.rs` and the palette into `flowview/palette.rs`. **tui/paths.rs** (1,382, *optional*) moves the plans of new heads and rules into `paths/plan.rs`.
- **sources.rs** and **names.rs** move their tests out.

**crates/vtium-platform**

- **display/drm2d.rs** (4,107). drm2d.rs keeps the types, `Drm2d`, its setters and its queries. New beside the existing children: `drm2d/frames.rs` (`lend`, `give_back`, `swap`, `commit`, `send`), `drm2d/power.rs` (`set_dpms`, `unblank`, `input`), `drm2d/cursor.rs` (the cursor plane), `drm2d/wake.rs` (the wake-up, its retries, the sleep and DRM master), `drm2d/events.rs` (`poll`, `dispatch`, the flips and their bounds), `drm2d/scan.rs` (the scan of the connectors and the placement on CRTCs), `drm2d/staging.rs` (each display's new buffers and the pan's tall buffers), `drm2d/modeset.rs` (the legacy modeset and the commits around it) and `drm2d/atomic.rs` (the atomic modeset, its tests and the color properties).

**crates/vtium-render**

- **renderer.rs** (3,138). renderer.rs keeps the public types and `Renderer` with its setters. `renderer/layout.rs` (`Grid`, `Layout`), `renderer/pass.rs` (`Pass`, `Place`, `Paint`, `Plan`), `renderer/cpu.rs` (`Back`, `Cpu`), `renderer/frame.rs` (`draw`, `plan`, `paint`, `finish`), `renderer/changes.rs` (what changed and each cell's reach), `renderer/rows.rs` (each row's drawing), `renderer/jobs.rs` (`push_jobs`, `job` and the cell helpers) and `renderer/pointer.rs` (the pointer's arrow).
- **gpu/compose.rs** (2,160): compose.rs keeps the shaders and `Work`, `compose/composition.rs` takes `impl Composition`, and `compose/back.rs` `GpuBack` and the quads.
- **gpu/glyphs.rs** (1,369, *optional*): `glyphs/boxes.rs` takes `Boxes` and the sprite textures.
- **notice.rs** and **cache.rs** move their tests out.

**crates/vtium-term**

- **line.rs** (2,250): line.rs keeps `Line` and its cells. `line/runs.rs` takes the sized runs, `line/edit.rs` `write`, `erase`, `insert_cells`, `delete_cells`, `resize` and the copies, `line/memory.rs` the budget and the compaction, and `line/tables.rs` `Tables`, `U24`, `ClusterArena`, `RunTable` and `IndexMap`.
- **scrollback.rs** (1,268, *optional*): `scrollback/bounds.rs` takes `sb_size`, `sb_memory` and the charge, and `scrollback/waiting.rs` `Waiting`, `RowsRead` and the moves.
- **terminal/images.rs** moves its tests out.

**crates/vtium-settings**: **parse.rs** (1,513) moves the line readers into `parse/lines.rs` and `impl Parser` into `parse/parser.rs`. keys.rs is exempt (D2).

**crates/vtium-image**: **store.rs** (1,738) keeps `Rows`, `Image`, `Placement`, `Buffer`, `Store` and its accessors. `store/add.rs` takes the `add_image` methods, `store/place.rs` `place` and `place_at`, `store/room.rs` the holds, the reservations, the settling, the eviction, the frees and the spare, and `store/geometry.rs` `Geometry`. The card checks whether `store/room.rs` belongs in the existing `store/reserve.rs` instead. **login.rs** moves its tests out.

vtium-font, vtium-parse, vtium-devices, vtium-clipboard and vtium-image-helper have no source file whose code passes 1,500 lines.

### D4: the crates at seams

A boundary becomes a crate when little crosses it and the crate gains something a module cannot give. The evidence is what crosses the boundary today: the items each side uses of the other.

- **`vtium-broker-protocol`, now.** The messages between the broker and the drawer, and the socket that carries them, crates/vtium/src/broker/protocol.rs. The file uses only std and rustix (`basis`). Its public interface is the message types `Request`, `Order`, `Got`, `Refusal` and `Errno`, the field types `Head`, `DevicePath`, `ConsoleRef`, `Grid`, `Display`, `Helper`, `PowerKey` and `Status`, and `Link`, which sends and receives them. The crate is needed because both ends of the socket must speak it. The broker lives in the vtium crate. The drawer's backend for the broker lives in vtium-platform, beside the libseat backend that ADR-1400 D4 names as its model, and vtium-platform cannot depend on the vtium crate. The crate carries `#![forbid(unsafe_code)]` and the same list of denied lints that broker/mod.rs sets today, because the broker parses what the drawer sends. A reviewer checks that list in the crate's lib.rs. The fuzz target `protocol` moves into the crate and depends on it instead of including the file by its path. The crate adds about 1,060 lines of code to the build graph, and it depends on rustix alone, so it builds in parallel with vtium-platform's dependencies. With fat LTO and one codegen unit, it costs the minspec binary nothing beyond D1's allowance.
- **`vtium-glyph-store`, after the privilege split's first slice.** The store of the glyphs the font helper sends, crates/vtium/src/deferred.rs with fits.rs, joined.rs and the generated data.rs. One item of the vtium crate crosses the boundary, the type alias `DisplayFace`, which the new crate defines itself and the vtium crate re-exports (`basis`). The store takes vtium-font and vtium-term, and a `bidi` feature that the vtium crate's `bidi` turns on. Its public interface is what the console uses today: `Shared`, `lock`, `Generated`, `Fresh`, `Held`, `DeferredFace`, `Shown` with its readers, `store_limit`, `droppable` and `decomposition`. The vtium crate then compiles about 8,000 fewer lines on each change, 3,089 of them generated tables, and the store's tests build and run without the console. The minspec binary pays nothing beyond D1's allowance. The move changes the `use` paths in app.rs, helper.rs and head.rs, so it waits until the card that moves the process jobs into the broker has merged.
- **The broker itself, decided after the first slice.** Today broker/mod.rs, start.rs, watch.rs, outbox.rs and refusals.rs use vtium-platform's event loop and nothing else of the vtium crate. The first slice's next cards add the devices, the sessions and the helpers to the broker, and they may reuse the console's child.rs and the helper's launcher. Once the slice has landed, a check lists every `crate::` path under broker/. If the broker reaches only its own modules, vtium-platform, vtium-settings and the protocol crate, it becomes the crate `vtium-broker`. Its Cargo.toml then shows everything the root process can call, and the code that reads and draws the sessions' output cannot be called from it by mistake. If the broker reaches more of the vtium crate, it stays a module, and the record of that check says what crossed.
- **The glyph helper's wire format, in a card of its own.** The console's helper.rs and vtium-config's serve.rs each define the request magic, the request kinds, the frame kinds and their bounds (`basis`). A crate that holds them once, with the frame reader and writer, would serve both sides, as vtium-image holds the image helper's requests and replies for vtium and vtium-image-helper (ADR-1100). Joining two copies into one is not a move, so it is not a split card. It comes after serve.rs and helper.rs are split, when helper/wire.rs and serve/wire.rs stand side by side, in a card whose tests show that both sides still agree: tests/boot.rs runs the real helper against the console. The crate must not pull vtium-config's decoders or rasterizer into the console.
- **No crate for serve.rs or for check-config.** serve.rs's code uses most of vtium-config's font code: the frit, the sources, the needs, the outline rasterizer, shaping, the decoders and the sandbox (`basis`). The seam worth weighing there is between vtium-config's font engine and its TUI, and swash's removal changes that code now. The check is made again once swash is gone. `vtium check-config` reuses the console's own reading of the configuration, the console's VTs, the notices, the font helper, the font and the glyph store, which is the point of the check. Its code stays modules of the vtium crate.

**ADR-1400's seam between the terminal side and the drawing side.** ADR-1400 D4 plans a card beside the second slice in which the code that becomes a session process and the code that becomes the compositor come to talk only through `Snapshot`, the requests, the input and the console's size. That card and this layout both touch term.rs, head.rs and app.rs, so they keep from undoing each other in three ways. The split of app.rs keeps the session side (app/sessions.rs, app/glyph_asks.rs) and the drawing side (app/drawing.rs, app/displays.rs) in separate modules, so the seam card finds each crossing as a `use` between them. term.rs and head.rs split in the seam card's wave, as that card's first commit, a move-only commit cut along the seam's line (D3). The seam card then moves only the items that cross. No split moves code across the seam in the meantime. The layout makes no crate at that seam, because the third slice decides how the session process and the compositor are built.

### D5: the order

A split never runs while another card changes the same files. The steering session gives a split card the whole file set of the files it splits, and no other card in the wave owns any of them.

1. **Wave 6c, first: crates/vtium's app.rs, helper.rs and check.rs** (D6), before the privilege split's next two cards start. Their briefs then name the new modules in their file sets.
2. **Wave 6c, next: `vtium-broker-protocol`** (D4), after the first card and before the card that moves the devices and the VT into the broker. It changes broker/protocol.rs, broker/mod.rs's `use` lines, the vtium crate's fuzz target and a new crate. That card's brief then names the crate.
3. **Whenever no card changes them:** vtium-settings' parse.rs and vtium-image's store.rs and login.rs. In wave 6c no card changes either crate, so each can run beside the first card. The steering session confirms this against the wave's file sets.
4. **After the pan's cards land:** vtium-render's renderer.rs, notice.rs, gpu/compose.rs, gpu/glyphs.rs and cache.rs, and vtium-term's line.rs, scrollback.rs and terminal/images.rs. The pan's cards change renderer.rs, ring.rs, notice.rs and damage.rs, and vtium-term's frame and its rows beyond the screen. The vtium-render split and the vtium-term split own disjoint crates, so they run side by side.
5. **After swash's removal lands:** vtium-config, serve.rs first, then frit.rs, outline.rs, apply.rs and specimen.rs, then the TUI's files. serve.rs alone is about the size of app.rs, so its card owns serve.rs and nothing else.
6. **After the privilege split's first slice lands:** vtium-platform's drm2d.rs; the vtium crate's font.rs, console.rs and painter.rs; `vtium-glyph-store`; and the check that decides the broker's crate (D4). The slice's cards change drm2d.rs, card.rs, the VT, input, udev and pty in vtium-platform, and app.rs, broker/, child.rs, survive*, helper.rs, power.rs, blank.rs, kernel.rs and getty.rs in the vtium crate.
7. **With ADR-1400's seam card, beside the second slice:** term.rs and head.rs, as that card's first commit (D4).
8. **Before 0.1.0:** the glyph helper's wire crate (D4), once serve.rs and helper.rs are split.

Each split lands on `integration` like any card. A branch that is open when its files split merges `integration` before it continues, and moves its own change into the new module by hand. That is why a split waits for the cards that change its files.

### D6: the first card

The first card splits crates/vtium's app.rs, helper.rs and check.rs into the modules D3 names. Together they move about 11,100 lines of code and 13,000 lines of tests. check.rs's 1,461 lines of code sit in D2's optional band, and the card splits them because it owns the file and the cuts are natural. A split moves code by line ranges with a script, without reading each line, so most of an agent's context goes to compiling, to visibility fixes and to the checks. app.rs alone moves about 10,000 lines into 13 files, and its visibility fixes are the bulk of the work. The card is therefore cut into two halves that run at the same time. Their file sets do not meet, apart from claim files they change on different lines.

| Card | File set | Modules | Commits |
|---|---|---|---|
| S1a | crates/vtium/src/app.rs and the new app/ modules of D3; the existing app/*.rs only where a `use` line must change; the claim files that name a symbol of app.rs; crates/vtium/README.md's row for app.rs; ADR-1000's TSVs and tools/conformance/COVERAGE.md through `docs/scripts/coverage --update` | app/displays.rs, devices.rs, sessions.rs, fonts.rs, glyph_asks.rs, blanking.rs, notices.rs, guard.rs, input.rs, keys.rs, turn.rs, drawing.rs, tests.rs | 1: app.rs's tests move to app/tests.rs. 2: app.rs's code moves into the modules. 3: the claims, the README and the `tests` column |
| S1b | crates/vtium/src/helper.rs and check.rs and their new modules; check/resources.rs only where a `use` line must change; the claim files that name a symbol of helper.rs or check.rs; crates/vtium/README.md's rows for helper.rs and check.rs; docs/systemd.md's line that names check.rs; ADR-1000's TSVs and COVERAGE.md through `docs/scripts/coverage --update` | helper/sandbox.rs, launcher.rs, wire.rs, cells.rs, process.rs, search.rs, answers.rs, tests.rs; check/frits.rs, notes.rs, conf.rs, tests.rs | 1: both files' tests move out. 2: helper.rs's code moves. 3: check.rs's code moves. 4: the claims, the README, docs/systemd.md and the `tests` column |

Both halves run at V1 under D1's checks. Before the first move, each records the test lists with and without features, `make minspec`'s size, and `target/doc/vtium/all.html` from `make api`. At the end, each runs:

- `cargo test -p vtium --all-features` and `cargo test -p vtium`, and compares the test lists;
- `cargo clippy -p vtium --all-features --all-targets -- -D warnings` and the same with no features;
- `make minspec`, within 2,048 bytes of the size before;
- `make api`, with the same `all.html`;
- `docs/scripts/claims` and `docs/scripts/coverage`;
- `make tier3-minimal` once, which S1a runs because app.rs holds the VT's and the sessions' handling, and which also checks the binary against the budget;
- `make tier2` once, which S1a runs because app.rs drives the render threads;
- the vtium crate's fuzz target `protocol` built once, since it includes a file of the crate.

S1b runs no tier, because helper.rs and check.rs reach no golden that unit tests and tests/boot.rs do not already cover. A fresh reviewer reads each half's diff with `--color-moved` as D1 says, and checks every line that is not dimmed. Each half reports the new files, the lines each took, every visibility change, and any small change made before the move.

### D7: the split follows the lines along which a library could later stand alone (proposed)

The maintainer wants the split to leave vtium's libraries usable apart from the rest, so that someone could later take the rendering engine alone (`basis`). The maintainer then narrowed it: the split cards change no code for this, and making the libraries more independent is later work outside these split cards (`considered`, 2026-11-25). So the split cards stay pure moves under D1. They replace no concrete type with a trait or an interface, remove no dependency between crates, and add no check that builds a library alone. What this decision asks of them is where they draw their module boundaries. Each split card chooses its modules along the logical lines that a later separation of a library would follow, so that the later work moves whole modules instead of cutting through them.

> Edited 2026-11-25: the maintainer said "we just need to be careful about not changing code yet. We should just consider the logical splits we have now. actually making it work more independent later is not in this set of split cards" (`considered`). D7 was titled "each library crate can be taken on its own". It proposed a layering of the crates, a crate `vtium-cells` for vtium-term's frame model that vtium-render would depend on instead of vtium-term, features for vtium's own parts inside the libraries, and a layering check that would fail the gates. It carried two probes, on the frame model's crate and on which crates are libraries. All of that is dropped. The audit below stays as information.

**What crosses each edge today.** The counts below are the distinct paths each crate's non-test code names in the other, read on 2026-11-25 (`basis`).

| Edge | What crosses | Does the library's job need it? |
|---|---|---|
| vtium-term → vtium-parse | 9 paths: `Parser`, `Consumer`, `Params`, `Chunk`, `Intermediates`, `InputMode`, `StringEnd`, `SUBPARAMS_MAX` | Yes. The terminal interprets what the parser reads. |
| vtium-term → vtium-image (feature `images`) | 28 paths: the image store, its handlers, keys, limits and the helper's `Request` and `Reply` | Yes, for images, and only with the feature on. |
| vtium-image → vtium-parse | 2 paths: `Chunk`, `Params` | Yes. The image protocols arrive as control strings. |
| vtium-render → vtium-font | 31 paths: `Font`, `Face`, `Glyph`, `Cell`, `Variant`, the sprites and the syllable rules | Yes. The renderer draws glyphs from a font, through the `Face` trait. |
| vtium-render → vtium-image (feature `images`) | 5 paths: `FramePlacement`, `Layer`, `Pixels`, `PlacementKey` | Yes, for pictures, and only with the feature on. |
| vtium-render → vtium-term | 33 paths in 24 files: `Frame`, `FrameCell`, `Line`, `LineId`, `LineSize`, `SizedRun`, `Glyph`, `Width`, `Style`, `Attrs`, `Rgb`, `Palette`, `Cursor`, `Damage`, `Age`, `UnderlineStyle`, `PointerShape`, `LinkHint`, `cluster_width`, `dim` and a few more | Only the frame model: the cells, lines, styles, colors and damage of a screen as one display reads it. The renderer never names the terminal, the screen or the parser, yet depending on vtium-term brings all three, with vtium-parse. |
| vtium-platform → vtium-image (feature `images`) | 16 paths, all in the image helper's host: the helper's `Request`, `Reply` and wire types | Not for the platform's job, which is displays, the VT, input, ptys and the event loop. The host lives there because it needs `unsafe` to start the helper, and it stays behind `images`. |
| vtium-settings → vtium-font, vtium-term, vtium-render (`filters`) | Constants and tables: `CELL_SIDE_MAX` and `set::own::holds` from vtium-font; the palettes and the scrollback's bounds from vtium-term; the frame filters' declarations from vtium-render | Yes. The key table states each bound the crate that owns it sets. |
| vtium-devices → vtium-platform, vtium-settings | 11 paths: the identities that udev and the EDID give, and the rule matcher | Yes. The crate exists to join the two. |
| vtium-clipboard → vtium-term | 2 paths: `CLIPBOARD_MAX`, `clipboard_len` | A bound only, for a program. |

The frame model in vtium-term is the seam a later separation of vtium-render would follow. Its modules `cell`, `line`, `style`, `palette`, `damage`, `order`, `frame`, `unicode`, `bounds`, `links` and `placeholder` hold about 9,000 lines, and none of them uses vtium-parse. vtium-term's screen and terminal reach into them through 91 items that are `pub(crate)` today, 29 of them in line.rs.

**What a user taking each library alone needs today.** Each library builds alone: `cargo check -p <crate> --lib`, with no features and with all features, passes for vtium-parse, vtium-font, vtium-image, vtium-term, vtium-render, vtium-platform, vtium-settings and vtium-devices (`basis`). Each has a README, and `make api` builds each one's reference with no warning. No crate sets `missing_docs`, and none is published.

- **vtium-parse** (17 public items) and **vtium-font** (153) depend on no other vtium crate and hold no `unsafe`. vtium-font carries the built-in Unifont and Unscii-8 behind features, and its glyph chunk format is vtium's own.
- **vtium-image** (137) depends on vtium-parse alone. Its `decode::wire` is the wire between vtium and vtium-image-helper, which is vtium's own.
- **vtium-term** (108) has the features `images` and `bidi`. Two parts are vtium's own: the XTGETTCAP tables, compiled from the terminfo entries vtium installs (`Termcap`), and the tables that generate the reference pages for escape sequences. The XTVERSION reply defaults to vtium's name, and the host sets it.
- **vtium-render** (121) holds no `unsafe`. It draws into a `Target` and, on the GPU path, through a `Gl` trait that the host implements, so it carries no platform assumption. Some of its public items draw vtium's own screens: the notices, the on-screen display, the pointer files and the frame filters.
- **vtium-platform** (189) is Linux-only. It holds 233 lines that use `unsafe`, for its ioctls, mmap and the C libraries libxkbcommon, libudev, libsystemd and libseat. Two parts are vtium's own: the image helper's host and the power keys' signal to PID 1. The privilege split adds the drawer's broker backend.
- **vtium-settings** (422), **vtium-devices** (12) and the `vtium` crate's library (689) describe vtium.conf, vtium's rules and the console itself. They are vtium's own layer, not libraries for others.

**The logical lines each split card follows.** Within D1's rule, a card can choose which functions go into which new module, and it chooses along these lines. Where a function serves both sides, the card leaves it where D3 puts it and reports it.

- **vtium-render.** The code that reads vtium-term's frame stays apart from the drawing itself. The frame is read in `renderer/pass.rs`, `renderer/changes.rs`, `link_row.rs`, `syllable.rs` and `joined.rs`. The drawing is in `renderer/rows.rs`, `renderer/jobs.rs`, `renderer/cpu.rs`, `cache.rs`, `compose.rs` and the GPU path's modules. The card puts each function that names a vtium-term type on the reading side where it can by moving the whole function, and reports the drawing modules that still name one. vtium's own screens, the notices, the on-screen display, the pointer files and the frame filters, stay in modules of their own, apart from the general renderer.
- **vtium-term.** The frame model, which is `cell`, `line`, `style`, `palette`, `damage`, `order`, `frame`, `unicode`, `bounds`, `links` and `placeholder`, stays apart from the screen, the terminal and the parser's use. The split of line.rs keeps its new modules inside the frame model. Nothing the screen or the terminal does moves into them. vtium's own tables, the XTGETTCAP tables in `termcap` and the sequence reference tables in `terminal/sequences`, stay in their own modules.
- **vtium-platform.** The image helper's host stays in `image_helper/`, and the drawer's broker backend stays in its own module. The split of drm2d.rs keeps the code for a card whose DRM master another process holds in modules apart from the rest, as far as moving whole functions allows. The power keys' signal to PID 1 stays in `power.rs`.
- **vtium-image.** The image helper's wire, `decode::wire`, stays apart from the store and the protocols. The split of store.rs keeps it there.
- **The vtium crate.** The crates of D4 already sit at seams, and its modules belong to vtium. No line applies.

vtium-parse and vtium-font have nothing to split. vtium-settings and vtium-devices describe vtium.conf and vtium's rules, so no line applies to them.

**Later work.** Making a library usable on its own is outside these split cards and has no card and no date yet. That work would decide which crates are libraries for others, which way dependencies may point, whether vtium-term's frame model becomes a crate of its own, which of vtium's own parts move behind features, and whether a check enforces it. The audit above is where it would start.

## Consequences

### Positive

- A card's file set names the part of the console it changes. The privilege split's next cards own app/sessions.rs or app/devices.rs instead of sharing app.rs.
- An agent reads the files it owns whole, and spends its context on the change.
- The broker's protocol becomes a crate that both ends of the socket share, and its fuzz target depends on the crate instead of a path into the vtium binary's sources.
- The glyph store builds and tests as a crate of its own, and the vtium crate compiles less on each change.
- The layout gives ADR-1400's seam card the session side and the drawing side as separate modules.

### Negative

- History through a move needs `git blame -C -C` and `git log -L`, and tools that do not follow moves show the split commit as the author of every moved line.
- A branch open across a split has to move its change by hand. The order in D5 keeps this rare, at the cost of holding each split until its files are quiet.
- The splits take several waves of cards that add no feature, and each one changes claim files, READMEs and ADR-1000's `tests` column.
- Up to about 4 KiB of the 38 KiB left under tier 3's size budget goes to the paths of new files.

### Neutral

- docs/architecture.md is not a record, and this record does not edit it. When the crates of D4 land, these passages change. The crate tree at the top of the page gains `vtium-broker-protocol` under vtium and vtium-platform, and later `vtium-glyph-store` under vtium, and `vtium-broker` if the check in D4 makes it a crate. The sentence "Every other crate's library carries `#![forbid(unsafe_code)]`" stays true, because each new crate carries it. The sentence "The binary's `src/app/hand_over.rs` holds these rules (ADR-1401 D16)" stays true.
- No decision record states a list of crates that this layout changes, so no record gets a dated note. ADR-1508's evidence lists the crates of its day, as history.
- Each crate's README names its files, and each split card updates the README of the crate it splits.

## Alternatives Considered

- **Crates for every subject.** Each large file, or each group of files, would become a crate. A crate boundary needs a public interface, so every item that crosses it becomes `pub` and part of what `make api` publishes. The console's parts call each other's private items in many places: app.rs alone reaches into `Head`, `Helper`, the painter and the glyph store. Making those interfaces public changes code, which a split does not do, and widens the API the site publishes. The maintainer chose modules everywhere and crates at seams (`basis`).
- **Split as each card touches a file.** Each card would split the file it changes as part of its work. Moves would then mix with edits in one diff, which `--color-moved` and `git blame -C` cannot separate. A file nobody changes would never split, and the work would never finish before 0.1.0.
- **Split nothing until after 0.1.0.** The maintainer asked for the split before the release that is not an alpha, and wondered whether it would help earlier (`basis`). The privilege split's next cards answer that: both change app.rs now.
- **One split card for the whole workspace.** One card would move about 70,000 lines of code at once. Every other card in flight would conflict with it, and no reviewer could read its diff in one pass.
- **A lower aim, such as 500 lines.** About twice as many files, many holding a few methods each, for little gain in reading or ownership. The cost in new file paths in the minspec binary would double as well.
