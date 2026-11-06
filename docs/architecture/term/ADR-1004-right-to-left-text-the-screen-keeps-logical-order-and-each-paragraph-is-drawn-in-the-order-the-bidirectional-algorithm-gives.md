---
contract: adr/v1
kind: decision
verb: add
capability: screen
basis:
  - operator: aaronsb
    level: guided
    said: "**An image wider than the new width** is kept whole and overflows the edge away from its anchor: off the right edge normally, off the left edge for an image anchored to the right (as with right-to-left text, once bidi exists). It is not clipped or scaled."
    via: the C fork's reflow issue, the operator's decision 4, comment of 2026-10-26, as ADR-1003's basis quotes it
  - standard: "Terminal Working Group, BiDi in Terminal Emulators, v0.1 of 2019-01-29 by Egmont Koblinger, https://terminal-wg.pages.freedesktop.org/bidi/: BiDi is display-only, the model keeps logical order (recommendation/display-only.html); a paragraph is the text between hard newlines, the rows a wrap joins (paragraphs.html); implicit mode, BDSM set by CSI 8 h, is the default, and explicit mode is CSI 8 l (basic-modes.html, escape-sequences.html); SCP CSI 1 SP k and CSI 2 SP k set left-to-right and right-to-left, CSI 0 SP k the terminal's default; private modes 2500 (box drawing mirrored), 2501 (direction detected, weakly favoured off) and 1243 (arrow keys swapped, on), numbers it calls temporary (escape-sequences.html); erased cells at a row's end go to the paragraph's end side and are left out of the algorithm (erased-cells.html); a bar cursor stands on the right side of a right-to-left cell (cursor.html); mouse reports carry the model's column (display-only.html); pictures are never flipped and count as U+FFFC, box drawing is not mirrored by default, and a paragraph over 500 lines may be drawn as explicit (misc.html)"
  - standard: "Unicode Standard Annex #9, the Unicode Bidirectional Algorithm, https://www.unicode.org/reports/tr9/: rules P2 and P3 (a paragraph's level from its first strong character), X1 to X10, W1 to W7, N0 to N2 with BD16's bracket pairs, I1 and I2, and L1 to L4 applied a line at a time, L4 drawing a character with Bidi_Mirrored=Yes at an odd level by its Bidi_Mirroring_Glyph"
  - standard: "The Unicode Standard, chapter 9, section 9.2 Arabic, cursive joining, with the Unicode Character Database 18.0.0's DerivedJoiningType.txt: Joining_Type D, R, L, C and T, everything else Non_Joining by its @missing line, the Arabic Presentation Forms-A and -B among them; DerivedBidiClass.txt, BidiBrackets.txt and BidiMirroring.txt, the same version vtium-term's tables pin"
  - standard: "ECMA-48, 5th edition: BDSM (7.2.1), DCSM (7.2.3), SCP (8.3.111) and SPD"
  - upstream: "VTE at 1db464a7 (LGPL-3.0, read for its behaviour): src/modes.py declares BDSM set by default and writable, DCSM fixed set, 1243 set, 2500 and 2501 reset, and DECRLM and DECRLCM as probably not worth implementing; src/vteseq.cc takes SCP 0, 1 and 2 and SPD 0 and 3 as aliases; src/bidi.cc runs the algorithm a paragraph at a time with FriBidi, shapes Arabic with presentation-form characters in implicit and explicit lines alike, leaves out the lam-alef ligatures, and treats a paragraph of more than VTE_RINGVIEW_PARAGRAPH_LENGTH_MAX rows, 500 (src/vtedefines.hh:130), as explicit"
  - upstream: "The Terminal Working Group's survey of terminals, https://terminal-wg.pages.freedesktop.org/bidi/prior-work/terminals.html: PuTTY and Konsole reorder every line and strip the bidi controls, with no way to switch reordering off; mlterm reorders, detects the direction under its default, and selects by the visual region; Terminal.app reorders as implicit left-to-right"
  - upstream: "WezTerm at e68a8ec277 (MIT): config/src/config.rs:802-806 declares `bidi_enabled`, false by default, and `bidi_direction`; its paragraphs are resolved by its own implementation of UAX #9, the wezterm-bidi crate"
  - upstream: "Konsole's bidi rendering is on in the default profile since 4.8, https://bugs.kde.org/show_bug.cgi?id=198169"
  - upstream: "Vim's options.txt, https://vimhelp.org/options.txt.html: 'arabic' sets 'rightleft' unless 'termbidi' is set, and 'arabicshape' does Vim's own shaping only while 'termbidi' is off, so with 'termbidi' set Vim leaves reordering and shaping to the terminal"
  - upstream: "The GNU Emacs manual, Bidirectional Editing, https://www.gnu.org/software/emacs/manual/html_node/emacs/Bidirectional-Editing.html: Emacs reorders right-to-left text for display itself while bidi-display-reordering is non-nil, its default"
  - upstream: "The C reference at kmscon origin/main: tests/capabilities/probes.replay held the probe bidi as a placeholder, no bidirectional reordering anywhere in libtsm or kmscon; its font backend drew each codepoint by its glyph without shaping, as ADR-1302's record of it says"
  - evidence: "vtium at fac82b62, read 2026-11-06: the glyph helper shapes a joining form keyed by U+200D on each side the letter joins (crates/vtium-config/src/shaping/mod.rs:26-33, kind at :81, joins at :102; shape.rs:180, joined), but nothing in the console builds such a key: the console asks the helper for each cell's own character or cluster (crates/vtium/src/head.rs:1009-1039, crates/vtium/src/helper.rs:1054), so an Arabic letter draws in its isolated form unless the program wrote U+200D beside it; crates/vtium-config/README.md:239 says the helper shapes the clusters the terminal segments, one at a time"
  - evidence: "vtium at fac82b62, read 2026-11-06: LineFlags uses bits 0 to 6 and keeps nine bits spare for bidirectional state (crates/vtium-term/src/line.rs:88-108); vtium-term forbids unsafe code and has no dependency outside the workspace (crates/vtium-term/README.md:3); the frame hands renderers a row's cells by column (crates/vtium-term/src/frame.rs:223, :238)"
  - evidence: "Sizes measured 2026-11-06 on x86-64: `make minspec` at fac82b62 builds a 3,600,232-byte binary against the 3,670,016-byte budget (tools/tier3/t3_impl.py BINARY_BUDGET), 69,784 bytes of headroom; a test program built with the minspec settings (opt-level z, LTO, one codegen unit, panic abort, stripped) grows by 52,504 bytes when it calls unicode-bidi 0.3.18 (MIT or Apache-2.0, tables at Unicode 16.0.0, no rule L3 or L4) for its paragraph levels, runs and reordering; UCD 18.0.0 has 1,626 runs of Bidi_Class, 533 of Joining_Type other than Non_Joining, 438 mirroring pairs and 130 bracket pairs, and folding the two properties into vtium-term's run table takes it from 2,466 runs to 3,927"
  - precedent: ADR-1000
  - precedent: ADR-1001
  - precedent: ADR-1003
  - precedent: ADR-1301
  - precedent: ADR-1302
  - precedent: ADR-1503
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "The probe `bidi` passes: in the replay interface's display view, `abc` then the Hebrew word shalom then ` (1) def`, printed in the default mode, reads `abc (1) <shalom reversed> def`"
  - "vtium-term's resolver passes every case of the Unicode Character Database's BidiTest.txt and BidiCharacterTest.txt at the version its tables pin"
  - "Tier 2 screens of mixed Arabic, Hebrew and Latin lines, in both paragraph directions and in explicit mode, with the cursor and a selection on them, drawn alike by the CPU and GPU paths"
  - "The minspec binary, measured after each slice, stays inside the size budget"
  - "Typing an Arabic word at a bash prompt on a VT shows it joined and running right to left, and copying it pastes the word in the order it was typed (the maintainer's VT review)"
status: proposed
date: 2026-11-06
deciders:
  - aaronsb
related: []
---

# ADR-1004: Right-to-left text: the screen keeps logical order and each paragraph is drawn in the order the bidirectional algorithm gives

## Summary

- **Decided (proposed):** vtium shows Arabic, Hebrew, Persian, Urdu, Syriac, Thaana and every other right-to-left script the way the Terminal Working Group's BiDi recommendation describes. The screen keeps text in the order the program wrote it, and when vtium draws a paragraph (the rows a wrap joins) it runs the Unicode bidirectional algorithm over it and draws each row in the resulting order. This implicit mode is on by default, so a shell, `ls`, `cat` or `less` that writes Arabic shows it right to left with no setting. A program that lays out right-to-left text itself turns implicit mode off with BDSM (`CSI 8 l`) and gets its cells drawn as it sent them. A paragraph runs left to right unless a program asks for right to left with SCP (`CSI 2 SP k`) or for detection with mode 2501, or vtium.conf's new `text-direction` key says otherwise. Arabic letters join by their neighbours in the order they were written, before the row is reordered, and a mirrored character such as a bracket draws its counterpart's glyph. Escape sequences, cursor reports, mouse reports, the selection and copy all work in written order. vtium-term computes the drawing order in the frame it hands every renderer, so the CPU and GPU paths draw the same cells. vtium writes its own implementation of the algorithm in vtium-term from tables at the Unicode version vtium-term already pins, and it is part of the default, minspec build.
- **Trades away:** about 30 to 45 KB of the minspec binary's 68 KB of headroom (an estimate; the first slice measures it). A paragraph holding right-to-left text redraws every visible row of it when any of its cells changes. A program that reorders text itself, such as Emacs, or Vim with `arabic` set and `termbidi` off, shows its right-to-left text reversed until it sends `CSI 8 l` or its own reordering is turned off. In a line that mixes directions, the cursor and the selection follow the written order, so a selection can show in pieces and the cursor can sit far from where the next letter appears. Every golden that shows right-to-left text today, the specimens' Hebrew and Arabic lines among them, moves.
- **One-way?** No. Reversible now; expensive once programs and users rely on the default mode, and once the goldens of the bidirectional screens are accepted.
- **Probes:**
  - *Confident (typing Arabic at the shell):* typing an Arabic word at a bash prompt shows its letters joined and running right to left, starting just after the prompt, and copying it with the mouse pastes the word in the order it was typed.
  - *Not confident (where a line of Arabic sits):* a line that is all Arabic, such as a file name `ls` prints, sits at the left edge of the screen with its letters running right to left, as GNOME Terminal shows it. It moves to the right edge only when a program asks for right to left or for detection, or when `text-direction` in vtium.conf says `rtl` or `auto`. Detecting each line's direction by default would put such lines at the right edge, and make lines jump between the two edges as their first letter changes.
- **Inversion:** Between "the terminal is a grid of cells and the program owns the order" (xterm, foot, Ghostty and the C reference draw cells left to right as they arrive) and "the terminal lays out text like a word processor" (every line detected and reordered, with no way for a program to opt out, as PuTTY and Konsole do). The answer reorders for display only, under modes a program can read and switch, and keeps the grid every escape sequence addresses unchanged.

## Context

The maintainer wants vtium to serve readers of the right-to-left scripts. Today vtium draws every row left to right in the order its cells were written. A program that writes the Hebrew word שלום writes ש first, so ש lands in the leftmost cell and the word reads backwards. An Arabic letter also draws in its isolated form: the glyph helper can draw a letter's joined forms, but nothing in the console asks for them (the evidence in `basis`). The specimens show both defects in their Hebrew and Arabic lines.

A few terms, used through this record:

- **Logical order** is the order a program writes characters in, which is the order they are read. **Visual order** is the order they appear on the screen from left to right. In right-to-left text the two run opposite ways.
- A **paragraph** is the text between two newlines the program sent. On the screen it is one row, or several rows that printing wrapped at the right margin, which `LineFlags::WRAPPED` records (ADR-1003). The alternate screen's full-screen programs usually end each row with a newline, so there a paragraph is usually one row.
- The **paragraph direction** decides which edge a paragraph's rows start from and the order of its runs: left to right starts at the left edge, right to left at the right edge.
- The **bidirectional algorithm** is Unicode's rule, UAX #9, for turning a paragraph in logical order into visual order. It gives each character an embedding level, even for left to right and odd for right to left, and then reverses runs of odd level line by line.
- **Implicit mode** is the terminal running that algorithm. **Explicit mode** is the terminal not running it: it draws the cells as sent, left to right, or right to left in a right-to-left paragraph, and the program has done any reordering.
- **Mirroring** is the algorithm's rule L4: a character such as `(` inside right-to-left text is drawn with the glyph of its counterpart `)`, so it still opens toward its contents.
- **Joining** is how Arabic, Syriac, N'Ko and the other cursive scripts connect letters: each letter takes an isolated, initial, medial or final form by whether it joins the letter before it and the letter after it in logical order.

Programs split into three kinds. Most programs, the shell, the core utilities, `less`, `git` and nearly every editor, write text in logical order and know nothing of direction; they need the terminal to reorder. A few programs lay out right-to-left text themselves and write it in visual order: Emacs reorders for display on its own, and Vim with `arabic` set and `termbidi` off does too. And a program that knows the recommendation switches the terminal's modes to say which of the two it does.

The Terminal Working Group's recommendation, written by the author of VTE's implementation, is the one specification of how a terminal does this, and VTE (GNOME Terminal, Tilix and the other VTE terminals) implements its first level. Its central rule is that bidirectional text is a matter of display: the screen the escape sequences act on keeps logical order, and reordering happens only when the screen is drawn. Konsole, PuTTY, mlterm and Terminal.app reorder too, each in its own way and none with modes a program can query (`basis`). xterm, foot, Ghostty and kitty do not reorder, and WezTerm reorders only while its `bidi_enabled` option is set, which is off by default.

The figure shows one line as the screen keeps it and as vtium draws it in each paragraph direction. The program wrote `abc`, the Hebrew word shalom, ` (1) ` and `def`, and the cursor sits after `def`.

![Three rows of twenty cells. The first row, the cells the program wrote in logical order, holds a b c, a space, the four Hebrew letters shin lamed vav final-mem from left to right, a space, an opening bracket, 1, a closing bracket, a space, d e f, the cursor on column 16, and four erased cells. The second row, drawn in a left-to-right paragraph, reads a b c, a space, then the brackets and the Hebrew word reversed: an opening bracket, 1, a closing bracket, a space, final-mem vav lamed shin, then a space and d e f, with the cursor on column 16 and the erased cells at the right; the two brackets are tinted, since each draws its counterpart's glyph. The third row, drawn in a right-to-left paragraph, starts with the four erased cells at the left, the cursor's column among them, then d e f, a space, the bracketed 1, a space, the reversed Hebrew word, a space and a b c at the right edge. Under each cell a small number gives the logical column it shows.](figures/ADR-1004-bidi.svg)

## Decision

### D1: the screen keeps logical order

The screen, its lines, the scrollback, the cursor, the margins and every escape sequence work in logical columns, exactly as they do without this record. A cursor position report, a rectangle operation, an insert or a delete counts logical columns. vtium-term computes the visual order only when a frame is taken (D8), and nothing it computes is stored in a line. Reflow (ADR-1003) and the image store work on logical lines as they do now.

DCSM (ANSI mode 9), ECMA-48's switch between editing the data and editing the presentation, is permanently set: editing always acts on the data. VTE fixes it the same way, citing the recommendation.

### D2: paragraphs and their properties

Each paragraph carries four properties, held in four of the nine spare bits of `LineFlags` (ADR-1001) on every row of the paragraph:

| Property | Set by | Default |
|---|---|---|
| implicit or explicit | BDSM, `CSI 8 h` and `CSI 8 l` | implicit |
| left to right or right to left | SCP `CSI 1 SP k` and `CSI 2 SP k`; `CSI 0 SP k` and `CSI SP k` restore vtium's default (D5); SPD `CSI 0 SP S` and `CSI 3 SP S` as aliases of SCP 1 and 2, as the recommendation allows and VTE does | from `text-direction` (D5) |
| direction detected | DEC private mode 2501 | from `text-direction` (D5) |
| box drawing mirrored | DEC private mode 2500 | off |

The terminal keeps a current value of each. A sequence that changes one sets it on every row of the cursor's paragraph. A new row, one that a line feed or a scroll brings onto the screen, takes the current values. A wrap carries the paragraph's values to the row it starts. A rewrap (ADR-1003) gives every row it makes the values of the paragraph's first row. When the scrollback drops a paragraph's first rows, its remaining rows keep their values. Other SCP and SPD parameters are ignored.

RIS and DECSTR return the current values to their defaults and apply them to the cursor's paragraph, as SM and RM would. DECRQM reports BDSM, 2500 and 2501 as set or reset, DCSM as permanently set, and 1243 (D11) as set or reset.

### D3: implicit mode

In implicit mode vtium runs the bidirectional algorithm over each paragraph and draws each row in the order it gives:

- The algorithm reads one character a cell: the first codepoint of the cell's cluster, since a cell's marks belong to its base and never move away from it, as the recommendation's chapter on combining characters requires. The second cell of a wide character, and the pad a wide character leaves when it moves to the next row (ADR-1003), go with the character.
- An erased cell inside a row reads as a space. The erased cells after a row's last written cell take no part in the algorithm, and they are drawn at the row's end in the paragraph's direction: at the right in a left-to-right paragraph and at the left in a right-to-left one, as the recommendation's chapter on erased cells says.
- The paragraph's level comes from its direction (D5): left to right, right to left, or, while detection is on, the first strong character's direction by rules P2 and P3, falling back to the paragraph's own direction when it has none.
- Rules X1 to I2 run over the whole paragraph, and rules L1 and L2 run a row at a time, so text never moves from one row to another. By rule L1 the spaces at a row's end take the paragraph's direction, so they stay at the row's end side instead of joining a right-to-left word before them.
- The explicit formatting characters (U+202A to U+202E and U+2066 to U+2069) and the marks U+200E, U+200F and U+061C take no cell of their own, since vtium-term gives format characters no width, and the algorithm reads none of them. This is the recommendation's first level. Honouring them is its second level, which the recommendation leaves for later.
- A paragraph of more than `BIDI_PARAGRAPH_ROWS_MAX` rows, 500 as the recommendation suggests and VTE uses, is drawn as explicit in its own direction, so the work a frame does for one paragraph is bounded.

### D4: explicit mode

In explicit mode vtium runs no algorithm. A left-to-right paragraph draws its cells left to right, as vtium does today. A right-to-left paragraph draws them right to left: logical column 0 at the right edge, its erased cells at the left. Mirroring (D7) applies to every character of a right-to-left paragraph in explicit mode, as the recommendation says.

A program that lays out right-to-left text itself sends `CSI 8 l` and then its rows in visual order, and gets them drawn as it sent them.

### D5: the paragraph direction and its detection

A paragraph runs left to right unless something says otherwise: the recommendation's default, and VTE's. A program says otherwise with SCP or mode 2501. A user says otherwise with a new key in vtium.conf:

- `text-direction = ltr`, the default: paragraphs run left to right and detection is off.
- `text-direction = rtl`: paragraphs run right to left and detection is off, for a console used mostly in a right-to-left language.
- `text-direction = auto`: detection is on, and a paragraph with no strong character runs left to right.

The key sets the default that RIS, DECSTR, `CSI 0 SP k` and a reset of mode 2501 return to. It changes no row already written.

### D6: Arabic joining, before reordering

Joining comes from logical order, and vtium decides each letter's form before it reorders the row:

- vtium-term computes, for each cell of a joining script, whether it joins the character before it and the character after it, by the rules of the Unicode Standard's section 9.2 and the Joining_Type property: marks (type T) are skipped, a letter of type D or R joins the one before it when that one is of type D, L or C, and a letter of type D or L joins the one after it when that one is of type D, R or C. The neighbours are the cells before and after it in the paragraph, so a word that wraps still joins across the wrap. Colors and other attributes do not break a join.
- In implicit mode the order is logical order. In explicit mode the program has already reordered, so vtium reads the cells in the order they are drawn from right to left, as VTE does. A program that writes the Arabic presentation forms, which it has shaped itself, is unaffected, since those forms are Non_Joining.
- The frame carries the two joining sides of each cell (D8). The console asks the glyph helper for the letter keyed with U+200D on each side it joins, the key the helper already shapes (ADR-1301, the listable clusters), and its lookup tries that key first. Lam followed by alef is asked for as the two-cell ligature the helper already shapes, drawn across the two cells where they sit after reordering.
- Where no source holds a joined form, as with a bitmap font, the console draws the letter's form from Unicode's Arabic Presentation Forms when the source holds that codepoint, as GNU Unifont does, and else the letter's own glyph. So the minspec build's built-in font joins Arabic too.
- A change that alters a cell's joining sides changes that cell's glyph, which counts as an ink change (ADR-1302 D9).

Reordering then moves whole cells. Since the joined forms are drawn for right-to-left reading, the letters of a word meet once the row is reversed.

### D7: mirroring

A character with Bidi_Mirrored=Yes at an odd level draws the glyph of its Bidi_Mirroring_Glyph, as rule L4 says and ADR-1302 D11 already decided; a mirrored character with no counterpart draws as it is, and no glyph is ever drawn flipped. The cell keeps the character the program wrote, so a copy brings back `)` where the screen shows `(`.

While mode 2500 is set, a box-drawing character (U+2500 to U+257F) in a right-to-left paragraph draws its left-right counterpart, by the table VTE uses, so a frame drawn for left-to-right text reads correctly mirrored. Mode 2500 is off by default, since the recommendation says box drawing is not mirrored.

### D8: where reordering happens

vtium-term computes the drawing order when it builds a frame, and a renderer draws what the frame gives:

- The frame's rows give their cells in visual order, each carrying its logical column, its resolved direction, its joining sides and the character to draw after mirroring. `Frame::cell(row, col)` and `Frame::cells(row)` take visual columns. A left-to-right paragraph whose characters are all left to right or neutral takes a short path whose order is the identity, so a screen with no right-to-left text costs one scan of each changed paragraph's classes.
- The CPU and the GPU paths read the same frame, so they draw the same cells in the same places, and tier 2's comparison of the two paths covers right-to-left text with no new code in either path.
- **Damage.** vtium-term keeps change ages by screen position (ADR-1001). When a frame is taken, a paragraph that holds any right-to-left or joining character, or that is explicit and right to left, has all its visible rows marked changed whenever any of its cells changed since the caller's last frame. A paragraph of left-to-right text keeps the per-cell damage it has today.
- **Pictures.** A picture is never flipped. It is placed by the direction of the paragraph its anchor row belongs to: in a left-to-right paragraph where it is today, whatever the text under it does, and in a right-to-left paragraph mirrored as a whole, so its box starts at the right edge of its first logical column's cell and extends left. This is the anchor on the right that ADR-1003's rule for a picture wider than the screen names. A kitty placeholder cell is text, and its private-use character is left to right, so a run of placeholder cells keeps its order inside the run.
- **Double-size rows.** A double-width or double-height row reorders as any row does, over its half as many cells.

The computation is a pure function of the screen and the four properties, so a frame of the same screen is the same however the output reached it, which the determinism rules require.

### D9: the cursor

The cursor stays on its logical cell and is drawn wherever that cell is drawn, as the recommendation says. A bar cursor stands on the right side of its cell when the cell's character resolves right to left, and on the left side otherwise; this settles what ADR-1302 D8 left to this record. A block or underline cursor covers its cell. A cursor on an erased cell at a row's end is drawn where that erased cell is drawn (D3). A cursor waiting to wrap at the right margin is drawn on the row's last logical cell, as today.

### D10: the selection, copy and the mouse

- vtium-term maps a visual cell back to its logical cell inside every function the host calls with a pointer position: `Terminal::mouse`, the selection's `select_start`, `select_to`, `select_word` and `select_line`, and `Screen::set_pointer`. The host passes the cell under the pointer as it does today.
- The selection is the logical range between the cell where the drag started and the cell under the pointer, as VTE selects. In a row that mixes directions its cells can show in pieces on the screen. A word and a line selection take the logical word and the paragraph's rows, as today.
- Copy gives the selected text in logical order, the order it was written, with the characters the program wrote, never their mirrored glyphs.
- A mouse report carries the logical column, so a program addresses the cell it drew. With pixel reports (mode 1016), the pixel position is the logical cell's left edge plus the pointer's offset inside the cell it is over.
- A link's hover underlines each of its cells wherever they are drawn, and the overlay showing its URI draws the URI left to right, since a URI has no right-to-left reading.

### D11: the arrow keys

DEC private mode 1243, on by default, swaps the sequences the Left and Right arrow keys send while the cursor's paragraph resolves right to left, as the recommendation says and VTE does. A program that edits a line in logical order, as readline does, then moves the cursor the way the arrow points over right-to-left text. Home and End are not swapped. With the default `text-direction = ltr` no paragraph resolves right to left unless a program or the user asks, so the swap changes nothing until then.

### D12: the modes and features, row by row

| ADR-1000 row | Status | DECRQM | Note |
|---|---|---|---|
| ANSI-8, BDSM | supported | 1 or 2, set by default | implicit and explicit mode (D3, D4) |
| ANSI-9, DCSM | ignored | 3, permanently set | editing always acts on the data (D1) |
| DEC-34, DECRLM | refused | 0 | the VT510's right-to-left cursor direction for a terminal that stores text in visual order; SCP and BDSM replace it; VTE leaves it out, and xterm and foot do not document it |
| DEC-96, DECRLCM | refused | 0 | the same, for the VT510's right-to-left copy direction |
| new: DEC-1243 | supported | 1 or 2, set by default | the arrow keys (D11) |
| new: DEC-2500 | supported | 1 or 2, reset by default | box drawing mirrored (D7) |
| new: DEC-2501 | supported | 1 or 2, by `text-direction` | the direction detected (D5) |
| text.bidi | supported | | implicit mode, level 1 of the recommendation |
| new: SCP | supported | | the paragraph direction (D2) |
| new: SPD | supported, 0 and 3 only | | aliases of SCP 1 and 2 |

ADR-1000's rule for a new mode admits 1243, 2500 and 2501 through the use their rows name, since xterm documents none of them. The recommendation calls their numbers temporary; vtium takes VTE's numbers, which programs that use them already send. The ADR-1000 edit that lands with the second slice (D15) sets these statuses and forms the new ids by the coverage script's rules.

### D13: in the minspec build, written in vtium-term

The algorithm, the joining sides and mirroring are part of the default build. Without them a console on the smallest board cannot show a sentence of Arabic or Hebrew in the right order, and the minspec build's built-in font already holds the glyphs.

vtium-term implements UAX #9's first level itself, in a module of its own, with its Bidi_Class, bracket pair, mirroring and Joining_Type tables generated by `gen-unicode` from the same Unicode Character Database version as its other tables, 18.0.0, each file pinned by its SHA-256. This keeps vtium-term free of dependencies outside the workspace and keeps every Unicode property vtium-term reads at one version. The tables cost about 12 to 15 KB as runs of their own: 1,626 runs of Bidi_Class, 533 of Joining_Type, 438 mirroring pairs and 130 bracket pairs (`basis`). The code is estimated at 15 to 30 KB at the minspec settings. The minspec binary has 69,784 bytes of headroom at fac82b62. The first slice measures the binary with `make minspec` and records the figure in vtium-term's README; a slice whose binary would be larger than the budget stops and goes to the maintainer.

### D14: tests

- **The resolver** is tested against Unicode's own conformance files for the pinned version, BidiTest.txt and BidiCharacterTest.txt, copied into vtium-term's tree with the Unicode licence as GraphemeBreakTest.txt is.
- **Tier 1:** the replay interface gains a display view, under a new feature `bidi` in its versioning, that prints each row of the screen in visual order with mirrored characters drawn as their counterparts. Replay cases cover each rule of D2 to D11: the modes and their DECRQM answers, SCP and SPD, a paragraph that wraps, the erased cells, detection, explicit mode in each direction, joining sides across a wrap, mirroring, box drawing under 2500, the cursor's drawn cell and side, a mouse report from a reordered cell, the arrow keys under 1243, and a reflow that carries the properties.
- **The probe `bidi`** in `tools/conformance/probes/probes.replay` stops being a placeholder: it prints a mixed line in the default mode and reads it back through the display view, as the first line of `observable` says.
- **Selection and copy:** vtium-term tests that a drag across a mixed row selects the logical range and copies it in logical order, and that word and line selections behave as D10 says.
- **Tier 2** gains vtium's own screens, provisional until the maintainer reviews them as pictures: mixed Arabic, Hebrew and Latin lines with digits and brackets in a left-to-right and a right-to-left paragraph; a line in explicit mode in each direction; the block and the bar cursor on a right-to-left cell; a selection across a mixed line; and the same Arabic lines in the built-in GNU Unifont, joined through the presentation forms. Following ADR-1503 D9's rule of one well-known open font for each script, tier 2's font set gains Noto Sans Arabic and Noto Sans Hebrew, each pinned by URL and SHA-256 and limited with `cover=` to its script's blocks, ahead of DejaVu Sans Mono, so each script draws from its own font. The slice checks that no accepted golden moves when the fonts join the set.
- **Fuzzing:** the screen's fuzz target reaches the new modes and printing, and the author of each slice that changes code it reaches runs it, as AGENTS.md says.

### D15: the work, in slices

Each slice is a card of its own, branched from `integration`, and lands in this order.

1. **The tables and the resolver** (vtium-term, `gen-unicode`): the generated tables, the level-1 resolver, the joining sides and the mirroring lookup, with the conformance files' tests. Gates: fmt, clippy and the tests of vtium-term; `make test-tools` for the generator; `make minspec` and its size, recorded.
2. **The paragraph state** (vtium-term): the four line flags, BDSM, DCSM, SCP, SPD and modes 1243, 2500 and 2501 in the mode table, DECRQM, the resets, the rows a wrap, a scroll and a rewrap make, and the `text-direction` key in the key table and vtium.conf's example; with the ADR-1000 edit and `docs/scripts/coverage --update` in commits of their own. Gates: vtium-term's and vtium-settings' tests, `make conformance`, the regenerated `docs/modes.md`, `docs/scripts/coverage`.
3. **The frame** (vtium-term): visual order, mirroring, joining sides, the damage rule, the cursor's cell and side, the selection's and the mouse's mapping, the arrow keys, pictures and placeholder runs; the replay interface's display view and the probe `bidi`. Gates: vtium-term's tests, `make conformance`, `make conformance-probes`, the fuzz target.
4. **Drawing** (vtium, vtium-render): the console's joining keys in its asks and its lookup, the presentation-form fallback in `gen-lookup`'s tables, the bar cursor's side. Gates: the tests of vtium and vtium-render, `make tier2`.
5. **The screens** (tools/tier2, tools/specimens): the pinned fonts, tier 2's screens and their provisional goldens, and the specimens regenerated, each set of goldens in a commit of its own that the maintainer reviews as pictures. Gates: `make tier2`, `make specimens-check`, and a tier 3 screen of a shell typing Arabic if the maintainer wants one for the VT review.

## Consequences

### Positive

- A shell, `ls`, `cat`, `less`, `git` and every other program that writes logical order show right-to-left text readably with no setting, on the smallest build and in the built-in font.
- Every escape sequence, cursor report and mouse report means what it means today, so no full-screen program's layout breaks because a row holds Hebrew.
- A program that knows the recommendation, or any program that sends `CSI 8 l`, keeps full control of its own layout, and can ask vtium which mode is on.
- One computation in vtium-term serves both drawing paths, the selection, the mouse and the replay view, so tier 1 can judge reordering without pixels.
- Arabic joins in every font vtium draws: outline fonts through the helper's shaped forms, bitmap fonts through the presentation forms.

### Negative

- About 30 to 45 KB of the minspec budget's 68 KB of headroom goes to this, by estimate. The next feature that needs room in the minspec binary meets a smaller margin, or a decision to raise the budget.
- A change in a paragraph that holds right-to-left text redraws all of that paragraph's visible rows, where left-to-right text redraws a few cells.
- Emacs, and Vim with `arabic` set and `termbidi` off, show right-to-left text reversed in the default mode until the program sends `CSI 8 l` or its user turns its own reordering off (`bidi-display-reordering` in Emacs) or sets `termbidi` in Vim. vtium's documentation says so.
- In a line that mixes directions, the cursor at the end of a right-to-left word in a left-to-right paragraph sits at the right of the line while the next letter typed appears at the word's left end, and a selection can show in pieces. The recommendation accepts both, since a terminal's cursor marks a cell and not a point between characters.
- Every golden that draws right-to-left text moves: the specimens' Hebrew and Arabic lines, and any screen that shows them. Each goes to the maintainer as pictures in a golden commit.
- The new modes take numbers the recommendation calls temporary. If the working group renumbers them, vtium follows with an edit to ADR-1000.

### Neutral

- ADR-1001's line flags give four of their nine spare bits to the paragraph's properties; five stay spare.
- The selection keeps holding logical cells by line identity, as today; only the cells a pointer names are mapped.
- ADR-1302's two items left to this record, the bar cursor's side and the redraw implicit mode needs, are settled by D9 and D8.
- The glyph helper needs no change: it already shapes the keys D6 asks for.
- Level two of the recommendation, which honours the explicit formatting characters, stays for a later record.

## Open questions

Each question carries the lean this record already writes as its decision, so accepting the record as it stands answers each one with its lean.

1. **The default paragraph direction and detection.** Lean: left to right with detection off, and a `text-direction` key (D5). For: the recommendation and VTE choose it; a screen keeps one alignment; arrow-key swapping changes nothing until asked for. Against: a console used mostly in Arabic shows each Arabic line at the left edge until its user sets the key, where mlterm and PuTTY detect each line. Check: the recommendation's escape-sequences chapter, which weakly favours detection off, and VTE's `src/modes.py`.
2. **The minspec build.** Lean: in the default build, written in vtium-term (D13). For: right-to-left text is basic legibility, and the measured headroom covers the estimate. Against: it spends most of the headroom; a cargo feature `bidi`, on in the main package as `images`, `logind` and `gpu` are, would keep it out of the smallest build. Check: the sizes in `basis`, and `make minspec` after the first slice.
3. **vtium's own implementation or the `unicode-bidi` crate.** Lean: vtium's own (D13). For: one Unicode version across vtium-term, no dependency outside the workspace, and tables that share vtium-term's run format; WezTerm made the same choice with its own crate. Against: `unicode-bidi` is servo's tested code and saves writing the algorithm; it costs 52,504 bytes, pins Unicode 16.0.0 and leaves rules L3 and L4 to the caller. Check: the measurement in `basis`, and the crate's `UNICODE_VERSION`.
4. **Implicit mode on the alternate screen.** Lean: on, as on the main screen. For: `less`, `man`, `nano` and `mc` write logical order and need it; the recommendation draws no line between the screens. Against: the programs that reorder themselves, Emacs and Vim with `arabic`, are full-screen programs, and turning implicit mode off on the alternate screen would serve them with no setting. Check: the Emacs manual's Bidirectional Editing and Vim's `termbidi` in `basis`.
5. **Joining in explicit mode.** Lean: join by the drawn order, read right to left (D6). For: VTE does it; a program that sends visual order and unshaped letters gets joined text, and one that shapes itself sends presentation forms, which never join. Against: the recommendation says the terminal does no shaping in explicit mode. Check: VTE's `explicit_line_shape` in `src/bidi.cc`, the recommendation's basic-modes chapter, and DerivedJoiningType.txt.
6. **The selection in a mixed row.** Lean: the logical range, which can show in pieces (D10). For: copy gives the text between the two points as it is read, as VTE does. Against: mlterm selects the visual rectangle, which looks contiguous and copies text that was never adjacent. Check: the recommendation's survey of terminals.

## Prior art

- **The C reference** did not reorder text: its capability probe `bidi` was a placeholder saying that libtsm and kmscon had no bidirectional reordering, and its font backend drew each codepoint's glyph without shaping, so Arabic letters drew isolated and Hebrew words reversed. An upstream `kmscon` user reported that Arabic was not shown.
- **VTE** implements the recommendation's first level with FriBidi, shapes Arabic with presentation-form characters, and leaves out the lam-alef ligatures. Its private modes are the numbers D12 takes.
- **Konsole** has reordered by default since 4.8, with a profile setting to turn it off; **PuTTY** reorders with no way to stop it; **mlterm** reorders and detects each line's direction; **Terminal.app** reorders as implicit left to right.
- **WezTerm** reorders only while its `bidi_enabled` option is set, off by default, and resolves paragraphs with its own implementation of UAX #9.
- **xterm, foot, Ghostty and kitty** draw cells left to right as they arrive, as vtium did before this record.

## Alternatives Considered

- **Reorder in vtium-render, as the work item first proposed.** Rejected: the selection, the mouse, the arrow keys and the replay view all need the same order, and they live in vtium-term; computing it in the renderer would compute it twice, once in each drawing path, and keep tier 1 blind to it.
- **Store each row in visual order.** Rejected: every escape sequence, reflow and the selection would need translating, and a program could no longer address the cell it wrote.
- **Implicit mode off by default, as ECMA-48's BDSM default reads.** Rejected: the programs that write logical order cannot be told to switch modes, which is why the recommendation makes implicit mode the default.
- **Detect every paragraph's direction by default, as mlterm and PuTTY do.** Not taken, for now: open question 1.
- **Each row its own paragraph.** Rejected: a sentence that wraps would be reordered in pieces, and the paragraph's level would change from row to row of one sentence.
- **Shape across cells with the full shaper at draw time.** Rejected: the console never parses fonts (ADR-1200), and joining forms are the listable clusters the helper already makes.
- **DECRLM and DECRLCM.** Refused (D12): they belong to terminals that stored text in visual order.
- **Honour the explicit formatting characters now, the recommendation's second level.** Deferred: it needs those characters kept in the screen, which they are not today, and no program in the target set sends them for layout.
