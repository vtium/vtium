# vtium-term

vtium's terminal state: the screen, its cells and lines, the scrollback, the modes, SGR and the replies, applied from the actions `vtium-parse` produces, and the frames a renderer reads. It forbids unsafe code and has no dependency outside the workspace. The cell, line and frame model (vtium/vtium#10, ADR-1001), the screen, scrollback, selection and Unicode tables (vtium/vtium#11) are in place; SGR, the modes and the replies come with vtium/vtium#12, and the character sets with vtium/vtium#13.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, its determinism lints and its public names. |
| `src/cell.rs` | `Cell`: 8 bytes, its content and width, and the indices of its style and hyperlink in its line's tables. |
| `src/style.rs` | `Style`, `Color` and `Attrs`: colours kept unresolved, and every SGR attribute, overline, conceal and superscript and subscript among them. |
| `src/line.rs` | `Line`: its cells, the style and link tables and the cluster arena they index, its `LineId` and its `LineFlags` (the reflow flag, the OSC 133 marks, the line size). |
| `src/damage.rs` | `Damage`: change ages by screen position, and the rule for what each screen event marks. |
| `src/frame.rs` | `Frame`, `FrameCell`, `Cursor` and `Palette`: what a renderer reads, with colours resolved at draw time and the selection drawn inverted. |
| `src/screen.rs` | `Screen`: the main and alternate screens' lines, the cursor, the margins and tab stops, the scrollback and its view, the pen, and the accessors the mode table drives; cursor movement and resizing. |
| `src/screen/print.rs` | Printing: grapheme clusters and the two width methods (mode 2027), wide characters and VS16, the pending wrap, insert mode, REP. |
| `src/screen/edit.rs` | Erasing (ED, EL, ECH, and their selective forms), inserting and deleting characters and lines, scrolling. |
| `src/screen/selection.rs` | The selection, held by line identity, its copy and its cells in the frame. |
| `src/scrollback.rs` | The scrollback and its view, `sb-size`'s default, and its cap: by default from the machine's memory, or as the configuration sets it. |
| `src/terminal.rs` | `Terminal`: the parser and the dispatch of its actions to the screen, with the character-set hook. |
| `src/unicode.rs` | Each codepoint's cell width, and the UAX #29 grapheme cluster rules. |
| `src/unicode/data.rs`, `src/unicode/grapheme_break_test.txt` | The property table and the GraphemeBreakTest cases, generated from the Unicode Character Database 18.0.0 by `gen-unicode/`, with the Unicode License v3 notice. |
| `gen-unicode/` | The generator of the Unicode tables, its own workspace: see its README. |
| `fuzz/` | The cargo-fuzz target feeding `vtium-parse` into the screen, its own workspace: see its README. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its citation: the one source for the constants and for the table below. |
| `tests/` | The integration tests: see its README. |

`cargo test -p vtium-term` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_TERM_BLESS=1 cargo test -p vtium-term readme_table` rewrites it.

## Behaviour

The C reference is the floor: vtium does everything it does, and where it departs from a protocol specification, vtium follows the specification. Each departure is listed below with its reason, and with the specification it follows where one covers it. The test of each is named with it.

Text is cut into cells by UAX #29's extended grapheme clusters at the Unicode version the tables carry, or by codepoint width with mode 2027 reset. A codepoint takes no cell when its General_Category is Mn, Me, Cf or Cc (but U+00AD and Prepend characters), or it is a Hangul medial vowel or final consonant; two when its East_Asian_Width is W or F or it has Emoji_Presentation; one otherwise. A VS16 makes an emoji-style base two cells. These are the C reference's rules and tables, so tier 1's goldens judge them unchanged.

The C reference's cluster table took a hash seed from the system's entropy, and logged nothing when it fell back (aaronsb/libtsm#41). vtium keeps each line's clusters in the line's own arena (ADR-1001), so there is no shared table, no hash and no seed: the item has no counterpart.

## Departures

- **Cells past a line's end read in the default style.** A scrollback line narrower than the screen is padded, when drawn, with blank cells in the default style, never the pen. The C reference drew them with the current pen, so an SGR under BCE recoloured them with no change age moving (aaronsb/libtsm#47). No specification covers padding. The reason: padding is not text the program wrote, so the pen has no claim on it, and a pen change that moved no age left a display drawing stale colours. Tests: `cells_past_the_end_read_blank_in_the_default_style` (the line), `a_pen_change_while_scrolled_back_leaves_the_padding_alone` (the screen).
- **A region scroll marks only its rows.** Scrolling inside margins, IL and DL mark the rows that moved and leave the rows outside them unchanged; the C reference marked every cell and every placement of the frame (aaronsb/libtsm#34). No specification covers change tracking. The reason: a renderer redraws what is marked, and the C reference's rule redrew a whole screen for a one-row scroll in a status pane. Tests: `a_region_scroll_marks_only_the_region` (`Damage`'s rule, and the screen's), and the property test `a_frames_changed_set_covers_every_cell_that_differs` (aaronsb/libtsm#48), which a planted skip turns red.
- **Moving the cursor marks nothing.** The frame carries the cursor, and a renderer redraws the cells it left and covers (ADR-1001, "Change tracking"). The C reference marked the cell the cursor left and the cell it reached, so after a cut in the replay view a caller's next `frame-since` counted the cursor's cell: the C reference printed 1 where vtium prints 0, which masking does not hide (vtium/vtium#14). Test: `after_a_checkpoint_nothing_is_changed`.
- **The frame marks the default background.** A frame cell says whether its background is the default one and not inverted, so an image below cell backgrounds shows through exactly there. The C reference resolved the background to RGB with no mark and compared colours, so a background a program set to the default's colour also showed the image (aaronsb/libtsm#35). Spec: the kitty graphics protocol, z-index below -1,073,741,824: drawn "under cells with non-default background colors". Test: `default_bg_marks_only_the_default_background_uninverted`.
- **CHA and VPR keep to the cursor's row in origin mode.** The C reference passed the cursor's absolute row where a row relative to the region was due, so CHA moved the cursor down by the region's top, and VPR by that and its count. Spec: ECMA-48 8.3.9, CHA moves within the active line; 8.3.160, VPR moves relative to it. Test: `cha_and_vpr_in_origin_mode_keep_to_the_cursors_row`.
- **A private marker or an intermediate makes another function.** CUD, CUF, CUB, CNL, CPL, CHA, VPA, VPR, ED, EL, ECH, IL, DL, CHT, CBT and REP act only without either, ED and EL also with `?` (DECSED, DECSEL). The C reference ran them whatever marker or intermediate they carried. Spec: ECMA-48 5.4, "Together with the Final Byte F, [the intermediate bytes] identify the control function", and 5.4.1 b), a private marker makes the parameter string private (vtium-parse's README, "Notes for vtium-term"). Test: `a_marker_or_an_intermediate_makes_another_function`.
- **HPA and HPR move the cursor.** The C reference ignored them. Spec: ECMA-48 8.3.57 (HPA), 8.3.59 (HPR). Test: `hpa_and_hpr_move_the_cursor`.
- **A printed DEL does nothing.** The C reference wrote U+007F to the screen, where it joined the cell before as a character of no width. Spec: vt100.net, "terminals only claiming VT220 compatibility can always ignore 7F"; a 96-character set in GL draws it, and those come with vtium/vtium#13 through the dispatch's character-set hook. Test: `a_printed_del_does_nothing`.
- **A copied wide character brings no space.** The C reference copied a space for the second column of each wide character in a selection. No specification covers copying; the reason: the space is not text the program wrote. Test: `a_copied_wide_character_brings_no_space`.

## C behaviour kept

These follow the C reference where xterm or the DEC manuals differ, so tier 1's goldens judge them unchanged; each is a departure to take or leave by the operator's decision.

- **DECSTBM leaves the cursor where it is**, where DEC STD 070 and xterm move it to the home position. An omitted bottom margin with a top margin makes the whole screen the region, where the specification takes the omitted bottom as the screen's last line.
- **A line feed at the screen's last row below the region scrolls the region**, where xterm does nothing.
- **Lines leaving a region's top enter the scrollback wherever the region starts**, where xterm keeps only those leaving a region at the screen's top.

## Notes for the next issues

- **The modes (vtium/vtium#12)** drive the screen through its accessors: `set_insert_mode` (IRM), `set_auto_wrap` (DECAWM), `set_origin_mode` (DECOM), `set_reverse_video` (DECSCNM), `set_cursor_visible` (DECTCEM), `set_alternate` (47, 1047, 1049, whose clearing and cursor saving the mode does), `set_width_method` (2027), `set_cursor_style` (DECSCUSR), and `Terminal::set_newline_mode` (LNM). `Screen::reset` returns the screen's own state for RIS and DECSTR; the cursor, the text and the pen are the reset's to set. SGR sets `Screen::set_pen`, OSC 8 `set_link`. Replies go to the terminal's reply buffer, which the dispatch writes. SM, RM, SGR and every sequence the dispatch does not name fall through it untouched.
- **The character sets (vtium/vtium#13)** fill the dispatch's `map_charset` hook, which every printed character passes through; SO, SI, SS2, SS3 and ESC sequences with intermediates fall through the dispatch untouched.
- **The replay view (vtium/vtium#14)** reads the screen's state: the rows of `Screen::lines()`, or the view while scrolled back; `Screen::cursor()`, whose column is the screen's width while a wrap is pending; the flags from `insert_mode`, `auto_wrap`, `origin_mode`, `reverse_video`, `cursor_visible` and `alternate` (the C reference's `FIXED_POS` was never set); and the change counts from the frame's cells. The C reference's screen view printed a space with no underline as an empty cell, so a row's trailing spaces trim.
- **ADR-1001's cluster budget moves one golden:** on a screen one column wide, a heart and VS16 (7 bytes with its length byte) keep the heart, where the C reference kept both (`insert-wide-one-column`). Test: `the_cluster_budget_cuts_a_cluster_on_a_one_column_line`.

## Bounds

The byte stream comes from an untrusted program, so nothing it sends grows memory without a bound. A screen is at most `COLUMNS_MAX` by `ROWS_MAX`, and its size comes from the display and the font. Every path that writes, moves or erases cells gives back the cluster bytes it drops through the line, so a line's arena keeps its budget on each of them (`tests/flood.rs`). REP prints at most `PARAM_VALUE_CAP` copies (vtium-parse), as printed text does. A line's style and link tables hold at most twice its cells before they compact, so a program that writes a new style or link into every cell, over and over, never grows them past that. A grapheme cluster keeps at most `CLUSTER_MAX` codepoints, and the clusters a line's cells name take at most `CLUSTER_BYTES_PER_CELL` bytes a cell, with the arena compacting at twice that. Damage tracks at most `COLUMNS_MAX` by `ROWS_MAX` positions, and the scrollback keeps at most `sb-size` lines, capped by default by the machine's memory or by a configured cap, at most `SB_SIZE_MAX`: the screen is made with the cap `sb_size_cap` gives, and clamps `sb-size` to it.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Citation |
|---|---|---|---|---|
| `COLUMNS_MAX` | 8192 | Cells a line holds, and columns a screen and its damage have | A line is made or widened to at most this many cells, and a screen and its damage to at most this many columns; the grid's width comes from the display and the font, never from the stream | vtium's own: a line's style and link tables are indexed in 16 bits and compact at `TABLE_GROWTH` times the line's cells, which this keeps inside 65535; a 15360-pixel display in a 4-pixel font is 3840 columns |
| `ROWS_MAX` | 2048 | Rows a screen and its damage have | A screen and its damage are made or resized to at most this many rows | vtium's own: an 8640-pixel display in a 6-pixel font is 1440 rows; the bound keeps a resize from the stream (vtium/vtium#44) from allocating without limit |
| `CLUSTER_MAX` | 32 | Codepoints one grapheme cluster keeps | Codepoints past it are dropped, and the cluster keeps its first 32 | kmscon ADR 102, "Cells and clusters": "at most 32 codepoints each (extenders past 32 are dropped)" |
| `CLUSTER_BYTES_PER_CELL` | 6 | Bytes of grapheme clusters a line keeps, per cell of the line: each cluster's codepoints in UTF-8 and a length byte, so a base and two combining marks a cell on average | A cluster that would pass it keeps the longest run of its first codepoints that fits, and at least its first codepoint | vtium's own: it holds the worst case of the default 1,000-line scrollback inside the minspec peak (ADR-1001, "Memory") |
| `SB_SIZE_CAP_SMALL` | 10000 | Scrollback lines `sb-size` may ask for on a machine with less than 2 GiB of memory | `sb-size` is clamped to the cap | The operator's caps, vtium/vtium#10 (2026-10-26) |
| `SB_SIZE_CAP_MEDIUM` | 30000 | Scrollback lines `sb-size` may ask for with 2 GiB of memory to less than 4 GiB | `sb-size` is clamped to the cap | The operator's caps, vtium/vtium#10 (2026-10-26) |
| `SB_SIZE_CAP_LARGE` | 100000 | Scrollback lines `sb-size` may ask for with 4 GiB of memory or more | `sb-size` is clamped to the cap | The operator's caps, vtium/vtium#10 (2026-10-26) |
| `SB_SIZE_MAX` | 100000 | Scrollback lines any cap may allow, the configured override included | A configured cap is clamped to it | vtium's own: the largest of the operator's caps, where the worst case a program can write is 531 MiB (ADR-1001, "Memory"); the operator asked for the cap to be configurable (2026-10-26) |
<!-- end of bounds table -->
