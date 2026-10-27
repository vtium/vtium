# vtium-term

vtium's terminal state: the screen, its cells and lines, the scrollback, the modes, SGR and the replies, applied from the actions `vtium-parse` produces, and the frames a renderer reads. It forbids unsafe code and has no dependency outside the workspace. The cell, line and frame model is in place (vtium/vtium#10, ADR-1001); the screen, scrollback and Unicode tables come with vtium/vtium#11.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, its determinism lints and its public names. |
| `src/cell.rs` | `Cell`: 8 bytes, its content and width, and the indices of its style and hyperlink in its line's tables. |
| `src/style.rs` | `Style`, `Color` and `Attrs`: colours kept unresolved, and every SGR attribute, overline, conceal and superscript and subscript among them. |
| `src/line.rs` | `Line`: its cells, the style, link and cluster tables they index, its `LineId` and its `LineFlags` (the reflow flag, the OSC 133 marks, the line size). |
| `src/damage.rs` | `Damage`: change ages by screen position, and the rule for what each screen event marks. |
| `src/frame.rs` | `Frame`, `FrameCell`, `Cursor` and `Palette`: what a renderer reads, with colours resolved at draw time. |
| `src/scrollback.rs` | `sb-size`'s default and its cap by the machine's memory. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its citation: the one source for the constants and for the table below. |
| `tests/` | The integration tests: see its README. |

`cargo test -p vtium-term` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_TERM_BLESS=1 cargo test -p vtium-term readme_table` rewrites it.

## Behaviour

The C reference is the floor: vtium does everything it does, and where it departs from a protocol specification, vtium follows the specification. Each departure is listed below with its reason, and each has a test seen red against the C behaviour.

## Departures

- **Cells past a line's end read in the default style.** A scrollback line narrower than the screen is padded, when drawn, with blank cells in the default style, never the pen. The C reference drew them with the current pen, so an SGR under BCE recoloured them with no change age moving (aaronsb/libtsm#47). No specification covers padding; this follows from vtium's model, where a line stores no trailing blank cells and every cell past its end is `Cell::BLANK`.
- **A region scroll marks only its rows.** Scrolling inside margins marks the rows of the region changed and leaves the rows outside it unchanged; the C reference marked every cell and every placement of the frame (aaronsb/libtsm#34). No specification covers change tracking. `Damage` holds the rule, and vtium/vtium#11 applies it in the screen, with a property test over the frame's changed set.
- **The frame marks the default background.** A frame cell says whether its background is the default one and not inverted, so an image below cell backgrounds shows through exactly there. The C reference resolved the background to RGB with no mark, and kmscon compared colours, so a background a program set to the default's colour also showed the image (aaronsb/libtsm#35). Spec: the kitty graphics protocol, z-index below -1,073,741,824: drawn "under cells with non-default background colors".

## Bounds

The byte stream comes from an untrusted program, so nothing it sends grows memory without a bound. A line's tables hold at most twice its cells before they compact, so a program that writes a new style or link into every cell, over and over, never grows them past that; a grapheme cluster keeps at most `CLUSTER_MAX` codepoints; and the scrollback keeps at most `sb-size` lines, capped by the machine's memory.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Citation |
|---|---|---|---|---|
| `COLUMNS_MAX` | 8192 | Cells a line holds | A line is made or widened to at most this many cells; the grid's width comes from the display and the font, never from the stream | vtium's own: a line's style and link tables are indexed in 16 bits and compact when they reach twice the line's cells, which this keeps inside 65535; a 15360-pixel display in a 4-pixel font is 3840 columns |
| `CLUSTER_MAX` | 32 | Codepoints one grapheme cluster keeps | Codepoints past it are dropped, and the cluster keeps its first 32 | kmscon ADR 102, "Cells and clusters": "at most 32 codepoints each (extenders past 32 are dropped)" |
| `SB_SIZE_CAP_SMALL` | 10000 | Scrollback lines `sb-size` may ask for on a machine with less than 2 GiB of memory | `sb-size` is clamped to the cap | The operator's caps, vtium/vtium#10 (2026-10-26) |
| `SB_SIZE_CAP_MEDIUM` | 30000 | Scrollback lines `sb-size` may ask for with 2 GiB of memory to less than 4 GiB | `sb-size` is clamped to the cap | The operator's caps, vtium/vtium#10 (2026-10-26) |
| `SB_SIZE_CAP_LARGE` | 100000 | Scrollback lines `sb-size` may ask for with 4 GiB of memory or more | `sb-size` is clamped to the cap | The operator's caps, vtium/vtium#10 (2026-10-26) |
<!-- end of bounds table -->
