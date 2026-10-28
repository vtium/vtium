---
contract: adr/v1
kind: decision
verb: add
capability: screen
basis:
  - operator: aaronsb
    level: directed
    said: "**Reflow:** when the terminal's column count changes, text that wrapped because it reached the right margin is re-joined and wrapped again at the new width, in the visible screen and the scrollback, with the cursor kept at the same place in the text."
    via: aaronsb/libtsm#28, the operator's definition, comment of 2026-10-26, carried to vtium/vtium#41
  - operator: aaronsb
    level: directed
    said: "A change in row count alone needs no reflow; lines scroll."
    via: aaronsb/libtsm#28, the operator's definition, comment of 2026-10-26
  - operator: aaronsb
    level: directed
    said: "A newline the program sent is never joined to the next line. Only soft wraps move, so `struct line` needs a \"continues onto the next row\" flag first."
    via: aaronsb/libtsm#28, the operator's definition, "Kept as they are", comment of 2026-10-26
  - operator: aaronsb
    level: directed
    said: "The alternate screen does not reflow. Full-screen programs redraw on SIGWINCH, as in xterm, VTE and foot."
    via: aaronsb/libtsm#28, the operator's definition, "Kept as they are", comment of 2026-10-26
  - operator: aaronsb
    level: directed
    said: "Text placed by cursor movement does not move, since it was never soft-wrapped."
    via: aaronsb/libtsm#28, the operator's definition, "Kept as they are", comment of 2026-10-26
  - operator: aaronsb
    level: directed
    said: "**Wide characters:** a double-width character that no longer fits at the end of a row moves whole to the next row."
    via: aaronsb/libtsm#28, the operator's decision 2, comment of 2026-10-26
  - operator: aaronsb
    level: directed
    said: "**Scrollback cost:** reflow the visible rows plus a margin at once. Rewrapping can let more content fit on the screen, so the margin covers what could scroll into view. Reflow the rest of the scrollback on demand, as it is paged to."
    via: aaronsb/libtsm#28, the operator's decision 3, comment of 2026-10-26
  - operator: aaronsb
    level: directed
    said: "**Images:** sixel and kitty placements move with their line's text when it rewraps (ADR-103 anchors them to lines)."
    via: aaronsb/libtsm#28, the operator's decision 1, comment of 2026-10-26; ADR-103 is kmscon's record
  - operator: aaronsb
    level: directed
    said: "**An image wider than the new width** is kept whole and overflows the edge away from its anchor: off the right edge normally, off the left edge for an image anchored to the right (as with right-to-left text, once bidi, #103, exists). It is not clipped or scaled."
    via: aaronsb/libtsm#28, the operator's decision 4, comment of 2026-10-26; #103 is aaronsb/libtsm#103, carried as vtium/vtium#43
  - upstream: "The C reference at libtsm eeaf293 (tools/conformance/pin.tsv): `tsm_screen_resize` (src/tsm/tsm-screen.c:769-921) grew every line to a wider screen and never shrank one, with no flag for a wrapped line (src/tsm/libtsm-int.h:183-190); `line_resize` (:238-261) only grew; aaronsb/libtsm#19, cells past the visible area reappeared when the screen widened"
  - upstream: "Ghostty at 46e9dc9a92 (MIT): `Terminal.resize` (src/terminal/Terminal.zig) reflows the primary screen while wraparound is set and never the alternate; `Screen.resize` tracks the saved cursor by a pin; `PageList.resizeCols` (src/terminal/PageList.zig) reflows every page at once, keeps the rows below the cursor less the wraps the reflow added above it, and leaves a `spacer_head` cell where a wide character moved to the next row"
  - upstream: "foot at 152487e494 (MIT): `grid_resize_and_reflow` (grid.c:816) reflows the whole scrollback at once with tracking points for the cursor, the saved cursor and the selection, fills `CELL_SPACER` where a wide character moved, drops trailing cells that are empty with the default background, and replaces a wide character wider than the row with one empty cell; the alternate grid takes `grid_resize_without_reflow` (render.c)"
  - upstream: "WezTerm at e68a8ec277 (MIT): `Screen::resize` (term/src/screen.rs) rewraps every line of the primary screen at once (`rewrap_lines`) and prunes the alternate screen's lines to the new width"
  - upstream: "xterm 412 (ThomasDickey/xterm-snapshots 5c75ead9a7): `Reallocate` (screen.c:447) allocates each line at the new width, so a narrowing drops the text past it; `OPT_RESIZE_ADJUST`, which keeps it, is 0 by default (ptyx.h:760-762); no reflow"
  - upstream: "kitty's documentation only (its source is GPL-3.0 and not read, CONTRIBUTING.md): shell integration erases the prompt on resize so the shell redraws it; `scrollback_fill_enlarged_window` fills a taller window from the scrollback"
  - standard: "contour's line reflow mode, mode 2028: `CSI ? 2028 h` enables and `CSI ? 2028 l` disables reflow \"on the current line and the following lines\", https://contour-terminal.org/vt-extensions/line-reflow-mode/"
  - evidence: "Rewrapping 100,000 rows of 170 cells (17 million cells) in vtium-term at abb9cb6 on a Ryzen 9 9950X3D, release build, 2026-10-27: copying the cell words took 12-45 ms (0.7-2.6 ns a cell); writing each cell through `Line::write`, which looks each style up in the destination line's table, took 130-156 ms for plain and three-style lines (7.7-9.2 ns a cell) and 1.0-2.4 s for a style a cell (61-143 ns a cell)"
  - precedent: ADR-1001
  - precedent: ADR-1000
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Accepted. Zooming in on a full scrollback dropping its oldest lines is the cost of resizing. The fill and the pad cell are agreed. The timing gate is judged on the SBC estimate until real hardware is available."
    via: the session's ADR questions, 2026-10-27; the figure was shown
    paraphrase: true
    covers: [long lines, zooming in on a full scrollback]
observable:
  - "The probe `reflow` (kmscon tests/capabilities/probes.replay) passes: `abcdefghijklmno` printed on 10 columns reads as one row after a resize to 20"
  - "A replay case for each rule of this record, under the decision it pins"
  - "A property test: rewrapping to another width and back restores the text of the scrollback and the screen"
  - "A property test: the screen after a stream with resizes is the same fed whole or cut at random points, and the same with the scrollback reflowed lazily or all at once"
  - "Reflow time and memory measured with a 100,000-line scrollback on the minspec machine, recorded in vtium-term's README"
status: accepted
date: 2026-10-27
deciders:
  - aaronsb
related: []
---

# ADR-1003: Reflow on a width change, and no cell past the margin

## Summary

- **Decided:** When the column count changes, the main screen and its scrollback rewrap as one run of text. Rows joined by a wrap become one line again and wrap at the new width; a line the program ended with a newline, or a row it placed by moving the cursor, is never joined to the next; a line longer than the new width wraps onto extra rows, which rejoin when the screen widens. The cursor and the main screen's saved cursor stay at their place in the text. A wide character that no longer fits moves whole to the next row and leaves a pad cell. The alternate screen, rows written while mode 2028 is reset and rows of double size never rewrap. No cell is kept past the right margin: what the main screen does not rewrap, and every row the alternate screen holds, loses its cells past a narrower margin. A widening's new columns are blank, or on the main screen take the colour the program erased the line's end with, which a line keeps as its fill. The screen, the rows a widening can pull onto it and the rows being viewed rewrap at once; the rest of the scrollback rewraps when something reaches it, with a result identical to rewrapping it all at once.
- **Trades away:** A narrowing that adds rows past `sb-size` drops the oldest, and widening again does not bring them back. Text past the margin on the alternate screen is lost where the C reference kept it. Rewrapping on demand costs a second representation of the scrollback and a test holding it to the eager result.
- **One-way?** No. Reversible now; expensive once tier 1's goldens and the image store (vtium/vtium#28) record reflowed screens.
- **Probes:**
  - *Confident (long lines):* a line 120 columns long that the program ended with a newline, on a screen narrowed to 80, wraps onto a second row and rejoins when the screen widens again; it never joins the line after it.
  - *Not confident (zooming in on a full scrollback):* zooming in on a scrollback already holding its `sb-size` lines drops its oldest lines, which zooming back out does not restore. Counting `sb-size` in lines of text instead would keep them, at the cost of no fixed bound on rows when the screen gets very narrow.
- **Inversion:** Between "the screen is a grid that a resize crops" (xterm, the C reference) and "the screen is a document the width lays out" (every row rewraps, the alternate screen included). The answer lays out the main screen's text and leaves grids drawn by full-screen programs as grids, which they redraw.

## Context

vtium/vtium#41 builds reflow on the line model of ADR-1001: each line carries `LineFlags::WRAPPED`, set when printing wrapped at the right margin and cleared by the edits crates/vtium-term/README.md lists. Selection already follows wrapped lines. Until reflow lands, a row whose text runs past the margin after a narrowing does not continue, and lines keep their cells past the margin so a widening shows them again. The operator defined reflow and made four decisions on aaronsb/libtsm#28, which vtium/vtium#41 carries.

The C reference did not reflow. A resize grew each line to the wider of the two widths and never shrank one, so a narrowing hid the cells past the margin and a widening showed them again with whatever they held, or the white-on-black cells the screen was created with (aaronsb/libtsm#19). It clamped the cursor's column, cleared the margins, reset the tab stops, pushed lines into the scrollback for a shorter screen and pulled them back for a taller one. Its lines had no flag for a wrap.

Other terminals:

| Terminal | Rewraps | When | The alternate screen | A wide character that no longer fits |
|---|---|---|---|---|
| xterm 412 | Nothing | | Cut at the new width, as the main screen | |
| Ghostty | The primary screen and its scrollback, while wraparound is set | All at once | Cut at the new width | Moves whole, leaving a `spacer_head` cell |
| foot | The main grid and its scrollback; during an interactive resize it cuts, and reflows once the resize ends | All at once | Cut at the new width | Moves whole, leaving `CELL_SPACER` cells |
| WezTerm | The primary screen and its scrollback | All at once | Cut at the new width | |
| kitty (documentation only) | The screen and its scrollback; with shell integration it erases the prompt for the shell to redraw | | | |

Ghostty, foot and WezTerm keep the cursor at its place in the text, and Ghostty and foot the saved cursor too.

The figure shows the rules on one main screen: a line wrapped at 10 columns, a line the program ended with a newline, a line holding a wide character, and the prompt with the cursor.

![The same main screen at 10, 5 and 14 columns. At 10 columns "abcdefghij" wraps to "klmnop", then "wxyz中" and "$ " with the cursor follow. Narrowed to 5, the wrapped line becomes four rows, "wxyz中" becomes "wxyz" with a pad cell and "中" on the next row, and the cursor stays after "$ ". Widened to 14, the first line becomes "abcdefghijklmn" and "op", "wxyz中" is one row again, and no row joins the one after it.](figures/ADR-1003-reflow.svg)

## Decision

### Which lines rewrap

A change in the column count rewraps the main screen's lines and its scrollback as one run of rows, oldest first, whichever screen is shown. A change in the row count alone rewraps nothing, and keeps today's flow: a shorter main screen pushes its top lines into the scrollback, a taller one pulls them back.

Never rewrapped, and cut at the new width instead:

- The alternate screen. Full-screen programs redraw on SIGWINCH.
- A row written while mode 2028 is reset (below).
- A row whose line size is double width or double height (vtium/vtium#61). It holds half the columns and is drawn by cursor movement.

### Logical lines

A logical line is a run of rows each marked wrapped but the last. A rewrap joins each logical line's cells, then cuts them into rows of the new width, marking each row but the last wrapped. The last row keeps whatever ended it: a newline the program sent, a cursor movement, or nothing yet. So a line ended by a newline, or a row whose text the program placed by moving the cursor, joins nothing after it.

A logical line's text runs to its last cell that is not empty. Empty cells inside the text, erased ones among them, stay where they are in it; a printed space is text. A line on the screen also runs at least to the cursor while the cursor is on it, so the cursor's place in the text survives a narrowing, as Ghostty and foot keep it.

The run of empty cells of one style that ends a row at the margin is the line's fill, not text: the default style after plain text, or the colour an erase to the line's end left. Empty cells before that run stay as text. A screen row's fill is read from its cells each time the row is rewrapped or enters the scrollback, and a line in the scrollback keeps it as a 16-bit index into its style table, in place of the cells, in the 6 bytes of padding ADR-1001's 48-byte line has spare. A rewrap carries the fill to the logical line's last row, which it paints from the end of the text to the margin at any width, so a widening's new columns take it: blank in the default style for a line nobody erased in a colour.

So a rewrap keeps every cell of text and the fill, and drops only pad cells, and rewrapping a line through several widths gives the same rows as rewrapping it once to the last of them. That is what lets the scrollback rewrap on demand (Cost).

### The margin

No row keeps a cell past the right margin. The main screen keeps the text a narrowing would hide by wrapping it onto extra rows; the alternate screen, rows written while mode 2028 is reset and rows of double size drop their cells past the new width. A widening shows only text that a rewrap brings back and the line's fill, so nothing stale reappears (aaronsb/libtsm#19); the alternate screen's new columns are blank in the default style, and its program redraws them.

### Wide characters and clusters

A wide character that does not fit in the columns left on a row moves whole to the next row, and the column it leaves is a pad cell. The pad is ADR-1001's fourth width value (bits 22-23 hold narrow, wide and the second column of a wide cell, and the fourth is free): it holds no content and draws as an empty cell in the background of the character that moved, as printing erases it today. A join drops pad cells, and a copy skips them. Printing's own wrap of a wide character, and of a cluster a VS16 widens in the last column, writes the same pad, so a line printed at one width and a line printed at another then rewrapped to it hold the same cells. On a screen one column wide a wide character becomes one empty cell, as printing makes it there; it is the one character a rewrap loses.

A grapheme cluster is one cell, or two when wide, and moves whole with its cell into the destination row's cluster arena. A rewrap never cuts a cluster: ADR-1001's budget of 6 bytes a cell holds for the logical line as written, so one row may name more than 6 bytes for each of its own cells while the logical line names no more than its budget, and the memory bound stands. A later write into such a row is held to the row's own budget, as every write is.

Styles and links move with their cells: each source row's table indices are mapped into the destination row's tables through a map built once a row, as compaction maps them, so the cost is linear in the cells moved. Looking each style up in the destination table by search, as `Line::write` does, is quadratic in a line whose every cell has its own style (Cost).

### The cursor and the saved cursors

The cursor is held through the rewrap as its offset in its logical line, and returns to the row and column holding that offset. A pending wrap stands after its cell: after the rewrap it is on the next column, or the pending wrap again when that cell ends a row. The main screen's saved cursor (DECSC's slot, and 1048 and 1049's save) is carried the same way, its saved pending wrap included. The alternate screen's saved cursor is clamped to the new size, as its screen is cut. While the alternate screen is shown, the main screen rewraps around the main slot's saved cursor in place of the cursor. The cluster the last print wrote ends at a resize, as it does today, since its cell has moved.

After the rewrap, the screen's rows below the cursor's row keep their number, less the rows the screen's text up to the cursor now takes beyond what it took before. Blank rows are removed from the bottom, or added there, to make it so; rows of text are never removed, and rows of text that no longer fit below the screen stay below it, as a shorter screen keeps them today. A narrowing therefore fills the blank rows below the cursor first and then pushes rows from the top into the scrollback; a widening refills the rows it frees above the cursor from the scrollback, newest first, and with the scrollback empty the screen's text moves up and blank rows fill the bottom. This is Ghostty's rule. The margins go and the tab stops return to every eighth column, as on every resize.

The view, while scrolled back, keeps the text of its top row at its top. The selection keeps its text: its ends are carried as the cursor is. An image placement's anchor is carried the same way (Images).

### Line identity and marks

A logical line's first row keeps its `LineId`. Each other row a rewrap makes takes a new id, and rows a join removes give theirs up. Ids are allocated in the order of the rows, when the resize happens, for the whole run, the rows rewrapped on demand included (Cost), so the ids do not depend on when a row was reached. Anything anchored to a row (a placement, the selection, the view) is carried by its offset in the logical line, never by its old id. OSC 133's marks (vtium/vtium#40) are kept on the row that holds the text they mark, the first row of the logical line when no finer place is known.

> Note 2026-10-28 (vtium/vtium#41): every mark of a logical line goes to its first row. A mark records no column, so a finer place depends on the widths the line passed through, and a scrollback rewrapped late would put it elsewhere than one rewrapped at each resize. A wrapped prompt's INPUT mark moves to the prompt's first row; vtium/vtium#40 can record a finer place.

### The scrollback's bound

`sb-size` counts rows at the current width, as it counts them today. A narrowing that leaves more rows than `sb-size` drops the oldest, cutting a logical line where its dropped rows end, and a widening brings nothing back. A row's memory under ADR-1001's bound is unchanged by a rewrap, since a rewrapped row holds fewer cells than the rows it came from or the same cells joined, so the scrollback's worst case at a cap stands.

> Note 2026-10-28 (vtium/vtium#41): a rewrapped row may name more cluster bytes than its own cells' budget, by the pad columns a widening drops and a last row's bytes on few cells. Across the scrollback and the screen the rows name at most one column's budget more than the widest screen's a row, since a widening frees the rows it joins for new text: ADR-1001's worst case grows by 6 bytes a row, to 531.58 MiB at 100,000 rows from 531.01 (`crates/vtium-term/tests/flood.rs`).

### Cost: the screen at once, the rest on demand

At once, as the resize happens:

- the screen, and the main screen's rows below it;
- while scrolled back, the rows the view shows after the resize;
- the margin: the newest scrollback rows that could reach the screen, as many as the screen has rows, plus as many rows as the tallest image placement has above the screen and above the view, which a frame walks;
- a count of the rows each remaining logical line takes at the new width, from which the drops past `sb-size` and the ids of the rows to come are settled. A line that fits in one row, or that holds no wide character, is counted from its length; only a line longer than the new width with a wide character in it is walked cell by cell.

The rest of the scrollback is held as its logical lines and rewraps when something reaches it: moving the view, a selection or a copy that spans it, the replay interface's views, or another resize, which rewraps it from its lines straight to the newest width. Rewrapping through several widths gives the rows rewrapping once gives (Logical lines), and the drops past `sb-size` are settled by the count at each resize, so a scrollback rewrapped late equals one rewrapped at each resize. Nothing rewraps by time, in the background or in slices: no timer reaches the screen (docs/architecture.md, "Determinism").

The figures, measured on the development machine and to be measured again on the minspec machine (vtium/vtium#41's gate), for 100,000 rows of 170 cells, 17 million cells: copying the cells takes 12-45 ms, a fixed cost of 0.7-2.6 ns a cell; through `Line::write` it takes 130-156 ms for plain and shell lines and up to 2.4 s for a style a cell, which the table map avoids. A rewrap with the map is taken as at most twice the copy, about 4 ns a cell. A single-board computer of the minspec class (a Cortex-A53 at 1.2 GHz) is taken as ten times slower, about 40 ns a cell, an assumption the measurement replaces. Tier 3's minspec VM runs on a host core, so it measures memory, not a board's speed.

| Rows rewrapped, 170 cells each | Development machine | A minspec board (estimate) |
|---|---|---|
| The screen and its margin, 2 × 48 rows | under 0.1 ms | under 1 ms |
| 1,000, the default `sb-size` | about 0.7 ms | about 7 ms |
| 10,000, the cap below 2 GiB | about 7 ms | about 70 ms |
| 100,000, `SB_SIZE_MAX` | about 70 ms | about 0.7 s |

Those are the worst case, where every row is full and wrapped. A shell's scrollback is mostly lines shorter than either width, which are neither joined nor cut, so a resize reads their length and flags and moves nothing. The count at the resize reads each row's length and flags, an estimated millisecond for 100,000 rows on the development machine, and walks the cells only of long lines holding wide characters.

A rewrap builds each logical line's new rows as it consumes its old ones, so its extra memory is a row or two, never a second scrollback, even for one logical line spanning the whole scrollback.

> Note 2026-10-28 (vtium/vtium#41): as built, three parts differ from the plan above. The waiting rows are one stretch, the oldest, so the rows the view and the selection reach rewrap at once with every row below them to the screen; a view at the top of 100,000 rows of wrapped pairs 170 cells wide costs 55-85 ms on the development machine, and keeping it to the view's rows needs a second waiting stretch, left to the operator. A line with no wide character breaks at multiples of the width, so the margin takes only the rows it needs of a longer one and its head waits, and a drop past `sb-size` cuts its first rows by length: one logical line spanning 100,000 rows resizes in about 2 ms. A resize to one column rewraps the whole scrollback at once, since a wide character becomes an empty cell there and a row that waited would keep it. Each line keeps a mark that it may hold a wide character in its padding, so the count reads lengths alone: 100,000 rows of wrapped pairs count in 2-3 ms.

### Determinism

The screen, the scrollback, the cursor, the image store and the replies after a run of bytes and resizes follow from that run, in order, and never from how the bytes between two resizes were split, nor from what the view, a selection or a copy reached in between. A resize falls between two input calls; a sequence or a UTF-8 character it falls inside completes after it against the new size, as the parser holds it.

Readers that take the screen by shared reference see only rewrapped rows: the operations that reach further (moving the view, a selection or copy spanning the scrollback, the replay interface's views) rewrap what they reach first. A test build rewraps the whole scrollback at every resize, and the property test compares it with the lazy one, whole and chunked, across random streams and resizes.

The host decides which resizes reach the terminal. A narrowing in between can drop scrollback rows, so a host that merged two quick resizes into one would make the state depend on time: vtium's host (vtium/vtium#21, #44) passes each resize it decides on and merges none by time.

### Images

Images are not built yet (vtium/vtium#28); this fixes what the store receives. A placement is anchored by the `LineId` of the row holding its top and a column (ADR-1001). A rewrap returns, for each anchor in the rows it rewrapped, the row and column now holding that cell, and the store moves the placement there, so a picture moves with its line's text. A placement wider than the columns left from its new column is kept whole and overflows the edge away from its anchor, the right edge for text running left to right; it is neither clipped nor scaled, and a widening shows it whole again. A placement anchored in the scrollback not yet rewrapped keeps its old anchor until its rows are rewrapped, which the margin's rows for the tallest placement make happen before a frame draws it. How an erase or text cuts a placement does not change.

### Mode 2028

Mode 2028, contour's line reflow mode, is set by default and by RIS. SM and RM 2028 mark the cursor's row, and each print marks the row it writes to, as rewrapping or not, from the mode's state. A row marked by the reset mode joins nothing, is joined into nothing and is cut at a narrower margin. The mark is one of ADR-1001's ten spare line flag bits, leaving nine for bidirectional text (vtium/vtium#43). DECRQM reports the mode as the mode table reports every mode (ADR-1000, DEC-2028).

> Note 2026-10-28 (vtium/vtium#41): DECSTR keeps mode 2028, which RIS sets, the mode table's class for a mode only RIS resets (`docs/modes.md`).

## Consequences

### Positive

- The shell's output reads whole at any width, in the screen and the scrollback, and the probe `reflow` passes.
- No stale cell comes back on a widening (aaronsb/libtsm#19), and the alternate screen's lines give back their memory past the margin.
- A resize costs the screen and its margin, under a millisecond at the minspec estimate, however large the scrollback.
- Printing at a width and rewrapping to it give the same cells, which a property test can hold.

### Negative

- Zooming in on a full scrollback drops its oldest rows for good.
- Text past the margin on the alternate screen, or on a row written with mode 2028 reset, is lost; the C reference kept it.
- The scrollback keeps rows of two kinds, rewrapped and waiting, and every reader that reaches past the screen must rewrap first. A test holds the lazy path to the eager one.
- `LineId`s are reallocated for rows a rewrap makes, so everything anchored to rows needs the carried offset.
- A shell that redraws its prompt on SIGWINCH may leave a copy of the old prompt where the rewrapped one now sits, until vtium/vtium#40 can erase the marked prompt for the shell to redraw, as kitty's shell integration does.

### Neutral

- ADR-1001's model gains three things, none growing a cell or a line: the pad cell takes the fourth width value, mode 2028 one of the ten spare line flags, and the fill a 16-bit style index in the line's padding. A line trimmed for the scrollback drops its fill's cells, coloured or not, where ADR-1001's trim keeps cells erased in a colour. ADR-1001 takes a dated note pointing here when this record is accepted.
- Cells past a line's end read in its fill, which is the default style unless the program erased the line's end in a colour; the pen still has no claim on them (vtium-term's README, "Cells past a line's end read in the default style").
- Printing's wrap of a wide character writes a pad where it erases a cell today; the cell draws the same.
- The selection's check for text past a narrower margin (crates/vtium-term/src/screen/selection.rs, `continues`) and the README's sentence on it go when reflow lands, since no row has any.
- vtium-term's README lists the departures from the C reference this brings: rewrapping, the cut at the margin, and the fill in a widening's new columns.

## Alternatives Considered

- **Keep cells past the margin, as the C reference and vtium today do.** Rejected: a line longer than a narrowed screen hides its end, which reflow exists to show, and stale cells come back on a widening (aaronsb/libtsm#19).
- **Cut every line at the new width and rewrap nothing, as xterm does.** Rejected by the operator's definition.
- **Cut a line the program ended with a newline instead of wrapping it.** Rejected: it hides text, and the line's end stays a newline when it wraps, so it still joins nothing after it.
- **Count the empty cells a colour erased as text, as foot does.** Rejected: a screen cleared in a colour and then narrowed to half its width wraps every blank row into two, and pushes half the screen into the scrollback.
- **Drop every empty cell after the text, coloured or not.** Rejected: a diff whose lines a pager paints in colour to the margin loses the colour on the first resize. The fill keeps it for one 16-bit index a line.
- **Drop the empty cells after the text only where they pass the new width.** Rejected: a narrowing and a widening would keep fewer than the widening alone, so the scrollback could not rewrap on demand and match the eager result.
- **Rewrap the whole scrollback at every resize, as Ghostty, foot and WezTerm do.** Simpler, with one representation. Not taken: the operator chose the screen and a margin at once, and at the 100,000-line cap it is about 0.7 s a resize on a minspec board in the worst case, paid on each zoom step. It remains the test build's reference.
- **Rewrap the scrollback in the background, a slice at a time.** Rejected: it brings time into the screen's state, which only the named places may hold.
- **Count `sb-size` in logical lines, so a narrowing drops nothing.** Not taken: a screen one column wide would turn each line into as many rows as it has cells, with no bound on rows. This is the probe on zooming in.
- **Rewrap only while DECAWM is set, as Ghostty does.** Not taken: DECAWM decides how printing wraps, and rows a program wrote with it set still carry their wraps after it resets it; mode 2028 is the switch for a program that wants its rows left alone.
- **Clear the selection at a resize.** Not taken: the selection is carried as the cursor is, at no further cost, as foot and Ghostty carry theirs.
- **Keep the cursor's row fixed, or keep the screen's top fixed.** Each loses one case: a fixed row pushes a cleared screen's prompt into the scrollback when it wraps, and a fixed top leaves blank rows at the bottom of a full screen that widened. Ghostty's rule keeps the rows below the cursor and serves both.
