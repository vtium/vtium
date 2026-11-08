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
  - upstream: "VTE (LGPL-3.0; documentation only, its source not read): the Terminal Working Group's implementations page, https://terminal-wg.pages.freedesktop.org/bidi/implementations.html, says VTE implements level 1 of the recommendation, planned for VTE 0.58; its misc page says VTE's work in progress uses DECSET 2500 to mirror box drawing; its cursor page says VTE's I-beam changes shape over a character of the other direction"
  - upstream: "mintty (GPL-3.0; documentation only, its source not read), https://github.com/mintty/mintty/wiki/CtrlSeqs: bidi on by default; BDSM CSI 8 h and l select implicit and explicit lines; SCP CSI 1 and 2 SP k set left-to-right and right-to-left, and CSI 0 SP k means detection with a left-to-right fallback, its default; mode 2501 detection, on by default, which the page says spoils tables drawn with box drawing; mode 2500 mirrors box drawing and block elements; SPD 0 and 3 kept as deprecated; private modes 77096 and 7796 switch bidi off globally and on the current line; its option Bidi=1 switches bidi off on the alternate screen"
  - upstream: "mlterm at 610985245c (BSD-3-Clause), read for its structure only: doc/en/README.bidi (bidi through FriBidi, Arabic shaped through presentation forms or HarfBuzz); doc/en/PROTOCOL:106-107 (bidi_mode = normal | left | right | switch, and bidi_separators); vtemu/vt_logical_visual.h:49-56 (the model is turned into visual order in place for rendering and back before any change); vtemu/libctl/vt_line_bidi.c:381 and :526 (a logical-to-visual and a visual-to-logical index for each line, which the cursor and the selection use), :554 (copy takes a range of visual positions and gives its text in logical order) and :13-35 (a character is mirrored when a neighbour beside it in visual order is reversed, a test of adjacency, not of its level)"
  - upstream: "Contour at e000176d4c (Apache-2.0): docs/internals/text-stack.md:175-178 says bidirectional text is not implemented"
  - evidence: "Observed 2026-11-06 on this machine, each terminal run under its own Xvfb display with an empty home directory and fed the same thirteen byte streams (a mixed line, an all-Arabic line, an all-Hebrew line, a prompt with Arabic, Arabic joining, brackets, numbers, a wrapped Hebrew sentence, BDSM reset, SCP 2, mode 2501, mode 2500 under SCP 2, and the cursor after Hebrew) and asked DECRQM for modes 8, 9, 34, 96, 1243, 2500 and 2501, with no source opened: Konsole 26.08.2 (GPL) reorders every line as a left-to-right paragraph, keeps an all-Arabic line at the left edge, joins Arabic, draws the brackets of `(1)` inside Hebrew unmirrored as `)1(`, ignores BDSM reset, SCP 2 and mode 2501, and answered none of the four DECRQM queries it was asked before the capture; kitty 0.49.2 (GPL) does not reorder a line but draws each word right to left as one shaped run, so `שלום עולם` shows its two words in written order from left to right, joins Arabic inside a word, answers 0 to every query and logs SCP and mode 2500 as unknown; Alacritty 0.17.0 (Apache-2.0) neither reorders nor joins, and answers 0 to every query"
  - upstream: "The Terminal Working Group's survey of terminals, https://terminal-wg.pages.freedesktop.org/bidi/prior-work/terminals.html: PuTTY and Konsole reorder every line and strip the bidi controls, with no way to switch reordering off; mlterm reorders, detects the direction under its default, and selects by the visual region; Terminal.app reorders as implicit left-to-right"
  - upstream: "WezTerm at e68a8ec277 (MIT), read for its structure only: config/src/config.rs:802-806 declares `bidi_enabled`, false by default, and `bidi_direction`; wezterm-surface/src/line/linebits.rs:34-52 keeps a line's implicit mode, its right-to-left direction and its detection as three bits of the line; term/src/screen.rs:76 gives each new line the terminal's current mode; term/src/terminalstate/mod.rs:1748-1760 sets and reports BDSM, term/src/terminalstate/performer.rs:510-523 takes SCP 0, 1 and 2, and :716-717 clears both at a reset; wezterm-gui/src/termwindow/render/screen_line.rs:744-766 and wezterm-surface/src/cellcluster.rs:155-195 resolve each line on its own in the renderer and shape each run in logical order before placing it; a code search of the repository for bidi finds no use in its selection or mouse code; its own crate, wezterm-bidi (bidi/README.md), passes every case of BidiTest.txt and BidiCharacterTest.txt"
  - upstream: "Konsole's bidi rendering is on in the default profile since 4.8, https://bugs.kde.org/show_bug.cgi?id=198169"
  - upstream: "Vim's options.txt, https://vimhelp.org/options.txt.html: 'arabic' sets 'rightleft' unless 'termbidi' is set, and 'arabicshape' does Vim's own shaping only while 'termbidi' is off, so with 'termbidi' set Vim leaves reordering and shaping to the terminal"
  - upstream: "The GNU Emacs manual, Bidirectional Editing, https://www.gnu.org/software/emacs/manual/html_node/emacs/Bidirectional-Editing.html: Emacs reorders right-to-left text for display itself while bidi-display-reordering is non-nil, its default"
  - upstream: "The C reference at kmscon origin/main: tests/capabilities/probes.replay held the probe bidi as a placeholder, no bidirectional reordering anywhere in libtsm or kmscon; its font backend drew each codepoint by its glyph without shaping, as ADR-1302's record of it says"
  - evidence: "vtium at fac82b62, read 2026-11-06: the glyph helper shapes a joining form keyed by U+200D on each side the letter joins (crates/vtium-config/src/shaping/mod.rs:26-33, kind at :81, joins at :102; shape.rs:180, joined), but nothing in the console builds such a key: the console asks the helper for each cell's own character or cluster (crates/vtium/src/head.rs:1009-1039, crates/vtium/src/helper.rs:1054), so an Arabic letter draws in its isolated form unless the program wrote U+200D beside it; crates/vtium-config/README.md:239 says the helper shapes the clusters the terminal segments, one at a time"
  - evidence: "vtium at fac82b62, read 2026-11-06: LineFlags uses bits 0 to 6 and keeps nine bits spare for bidirectional state (crates/vtium-term/src/line.rs:88-108); vtium-term forbids unsafe code and has no dependency outside the workspace (crates/vtium-term/README.md:3); the frame hands renderers a row's cells by column (crates/vtium-term/src/frame.rs:223, :238)"
  - evidence: "Sizes measured 2026-11-06 on x86-64: `make minspec` at fac82b62 builds a 3,600,232-byte binary against the 3,670,016-byte budget (tools/tier3/t3_impl.py BINARY_BUDGET), 69,784 bytes of headroom; a test program built with the minspec settings (opt-level z, LTO, one codegen unit, panic abort, stripped) grows by 52,504 bytes when it calls unicode-bidi 0.3.18 (MIT or Apache-2.0, tables at Unicode 16.0.0, no rule L3 or L4) for its paragraph levels, runs and reordering; UCD 18.0.0 has 1,626 runs of Bidi_Class, 533 of Joining_Type other than Non_Joining, 438 mirroring pairs and 130 bracket pairs, and folding the two properties into vtium-term's run table takes it from 2,466 runs to 3,927"
  - evidence: "Measured 2026-11-06 with `make minspec` on x86-64 from a scratch copy of fac82b62 that defines a `bidi` feature and calls unicode-bidi 0.3.18 from the binary: the build without the feature is 3,600,248 bytes, the 3,600,232 of fac82b62 plus 16 bytes of a longer revision string; the vtium package's features `images logind gpu` give 4,382,376 bytes; with `bidi` added, 4,429,208 bytes (+46,832) when the crate uses its own Unicode 16 tables, and 4,417,080 bytes (+34,704) when it is built without its default features and fed Bidi_Class at Unicode 18.0.0 (1,272 runs) through its BidiDataSource trait"
  - evidence: "unicode-bidi 0.3.18 run 2026-11-06 through its public API against Unicode's BidiTest.txt and BidiCharacterTest.txt, checking each case's paragraph level, every level the test gives, and the visual order: at 16.0.0, 770,241 BidiTest cases and 91,707 BidiCharacterTest cases, all passing; at 18.0.0 the same, with the crate's tables and with Unicode 18 classes fed through BidiDataSource; a planted defect, levels read before rule L1, failed 173,333 cases. Outside the files: against DerivedBidiClass.txt its tables give 3,970 unassigned code points a different class at 16.0.0, and about 228 more characters, added in Unicode 17 and 18, differ at 18.0.0. Its `visual_runs` panics with OutOfRangeNumber (src/lib.rs:956) on the 3,637-byte input attached to its maintainers' report of a level-overflow panic, and on any empty line range; the same input through `reordered_levels_per_char` and `reorder_visual` does not, nor did 300,000 generated lines on non-empty ranges"
  - upstream: "unicode-bidi's project, https://github.com/servo/unicode-bidi and https://crates.io/crates/unicode-bidi, read 2026-11-06: owned on crates.io by SimonSapin, larsbergstrom, metajack, mbrubeck, behnam and the servo publishing team; one release in the last two years, 0.3.18 on 2024-12-16; unreleased on its main branch, the Unicode 17.0 tables (2025-10-13), the guard against the level overflow (2026-07-19), a fix to bracket pairing when its stack overflows (2026-09-15) and its conformance test run in all three paragraph directions (2026-09-10); its CI (.github/workflows/main.yml) runs tests/conformance_tests.rs against the 15.0.0 files, comparing levels and not the visual order; no open issue reports wrong ordering; 570,584,094 downloads, 105,489,498 in the last 90 days, and 223 crates that depend on it directly"
  - precedent: ADR-1000
  - precedent: ADR-1001
  - precedent: ADR-1003
  - precedent: ADR-1301
  - precedent: ADR-1302
  - precedent: ADR-1503
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "yes, I think. Is this how terminal emulators typically do this?"
    via: the steering session's question tool, 2026-11-06, after the maintainer saw the figure and the observed-terminals contact sheet; the steering session answered that among terminals that reorder at all, on by default is the convention (Konsole, PuTTY, Terminal.app, mintty, and the TWG defaults GNOME Terminal's engine follows), with WezTerm off by default and kitty, Alacritty, foot, Ghostty and xterm not reordering
    covers: [typing Arabic at the shell]
  - operator: aaronsb
    said: "Again, we don't need to break new ground, we need to follow conventions that other terminal emulators that are well known to be competent in operating in this way"
    via: the steering session's question tool, 2026-11-06, on the default edge of an all-Arabic line; the convention is the left edge with no per-line detection (Konsole observed, the TWG defaults, Terminal.app per the TWG survey), which is the record's lean
    covers: [where a line of Arabic sits]
  - operator: aaronsb
    said: "bidirectional unicode capability could be a compiler flag option, like images etc. turning it off would drop the terminal capabilities. this would allow a minspec build to avoid the memory cost but drop the functionality. minspec is that same sort of 'Hello world I am a 320x240 CGA display with a codepage 437 character set and that is all I know' sort of idea. (not that it's exactly conformant to that but you get what I mean)"
    via: the maintainer's message to the steering session, 2026-11-06, on open question 2, whether bidi belongs in the minspec build
  - operator: aaronsb
    said: "unicode-bidi, guarded"
    via: "the steering session's question tool, 2026-11-06, after the conformance, maintenance and size measurements; the maintainer had earlier written: unicode-bidi is code that is probably seen by more people and likely is better than what we could write. thoughts?; the option wording is the steering session's"
  - operator: aaronsb
    said: "On there too"
    via: the steering session's question tool, 2026-11-06, on open question 4; the option wording is the steering session's
  - operator: aaronsb
    said: "Join by drawn order"
    via: the steering session's question tool, 2026-11-06, on open question 5; the option wording is the steering session's
  - operator: aaronsb
    said: "Typed-order range"
    via: the steering session's question tool, 2026-11-06, on open question 6; the option wording is the steering session's
  - operator: aaronsb
    said: "Accept it"
    via: the steering session's question tool, 2026-11-06, after all seven answers were recorded and the successor research found none better than unicode-bidi; the option wording is the steering session's
  - operator: aaronsb
    said: "[A direction change sent mid-paragraph: follow the recommendation or keep D2?] Follow the recommendation (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
observable:
  - "The probe `bidi` passes: in the replay interface's display view, `abc` then the Hebrew word shalom then ` (1) def`, printed in the default mode, reads `abc (1) <shalom reversed> def`"
  - "vtium-term, calling unicode-bidi as D13 guards it, passes every case of the Unicode Character Database's BidiTest.txt and BidiCharacterTest.txt at the version its tables pin, the visual order included"
  - "Tier 2 screens of mixed Arabic, Hebrew and Latin lines, in both paragraph directions and in explicit mode, with the cursor and a selection on them, drawn alike by the CPU and GPU paths"
  - "The minspec binary is the same size with and without this work, since it leaves the `bidi` feature out; `vtium --version` of a build with the feature lists `bidi` among its features"
  - "A build without the `bidi` feature answers DECRQM 0 for BDSM, DCSM and modes 1243, 2500 and 2501, and draws right-to-left text in the order it was written"
  - "Typing an Arabic word at a bash prompt on a VT shows it joined and running right to left, and copying it pastes the word in the order it was typed (the maintainer's VT review)"
status: accepted
date: 2026-11-06
deciders:
  - aaronsb
related: []
---

# ADR-1004: Right-to-left text: the screen keeps logical order and each paragraph is drawn in the order the bidirectional algorithm gives

## Summary

- **Decided (proposed):** vtium shows Arabic, Hebrew, Persian, Urdu, Syriac, Thaana and every other right-to-left script the way the Terminal Working Group's BiDi recommendation describes. The screen keeps text in the order the program wrote it, and when vtium draws a paragraph (the rows a wrap joins) it runs the Unicode bidirectional algorithm over it and draws each row in the resulting order. This implicit mode is on by default, so a shell, `ls`, `cat` or `less` that writes Arabic shows it right to left with no setting. A program that lays out right-to-left text itself turns implicit mode off with BDSM (`CSI 8 l`) and gets its cells drawn as it sent them. A paragraph runs left to right unless a program asks for right to left with SCP (`CSI 2 SP k`) or for detection with mode 2501, or vtium.conf's new `text-direction` key says otherwise. Arabic letters join by their neighbours in the order they were written, before the row is reordered, and a mirrored character such as a bracket draws its counterpart's glyph. Escape sequences, cursor reports, mouse reports, the selection and copy all work in written order. vtium-term computes the drawing order in the frame it hands every renderer, so the CPU and GPU paths draw the same cells. Bidirectional text is a cargo feature, `bidi`, as pictures are the feature `images`: the vtium package builds with it, and the minspec build leaves it out and has no bidirectional capability (the maintainer's decision). vtium-term computes the order with servo's `unicode-bidi` crate, pinned, fed vtium-term's own Unicode tables and called through a restricted path until a release carries the fixes waiting on its main branch (the maintainer's decision).
- **Trades away:** the minspec build draws Arabic and Hebrew in the order they were typed, backwards and unjoined; the build with the feature grows by about 35 KB for the algorithm and its class table, with mirroring, joining and the frame's code still to measure. A paragraph holding right-to-left text redraws every visible row of it when any of its cells changes. A program that reorders text itself, such as Emacs, or Vim with `arabic` set and `termbidi` off, shows its right-to-left text reversed until it sends `CSI 8 l` or its own reordering is turned off. In a line that mixes directions, the cursor and the selection follow the written order, so a selection can show in pieces and the cursor can sit far from where the next letter appears. Every golden that shows right-to-left text today, the specimens' Hebrew and Arabic lines among them, moves.
- **One-way?** No. Reversible now; expensive once programs and users rely on the default mode, and once the goldens of the bidirectional screens are accepted.
- **Probes:**
  - *Confident (typing Arabic at the shell):* typing an Arabic word at a bash prompt shows its letters joined and running right to left, starting just after the prompt, and copying it with the mouse pastes the word in the order it was typed.
  - *Not confident (where a line of Arabic sits):* a line that is all Arabic, such as a file name `ls` prints, sits at the left edge of the screen with its letters running right to left, as Konsole shows it (observed). It moves to the right edge only when a program asks for right to left or for detection, or when `text-direction` in vtium.conf says `rtl` or `auto`. Detecting each line's direction by default, as mintty documents, would put such lines at the right edge, and make lines jump between the two edges as their first letter changes.
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

The Terminal Working Group's recommendation, written by the author of VTE's implementation, is the one specification of how a terminal does this, and VTE (GNOME Terminal, Tilix and the other VTE terminals) implements its first level. Its central rule is that bidirectional text is a matter of display: the screen the escape sequences act on keeps logical order, and reordering happens only when the screen is drawn. mintty documents the recommendation's sequences too. Konsole, PuTTY, mlterm and Terminal.app reorder, each in its own way and none with modes a program can query (`basis`, and the prior-art table). xterm, foot, Ghostty and kitty do not reorder, and WezTerm reorders only while its `bidi_enabled` option is set, which is off by default.

The figure shows one line as the screen keeps it and as vtium draws it in each paragraph direction. The program wrote `abc`, the Hebrew word shalom, ` (1) ` and `def`, and the cursor sits after `def`.

![Three rows of twenty cells. The first row, the cells the program wrote in logical order, holds a b c, a space, the four Hebrew letters shin lamed vav final-mem from left to right, a space, an opening bracket, 1, a closing bracket, a space, d e f, the cursor on column 16, and four erased cells. The second row, drawn in a left-to-right paragraph, reads a b c, a space, then the brackets and the Hebrew word reversed: an opening bracket, 1, a closing bracket, a space, final-mem vav lamed shin, then a space and d e f, with the cursor on column 16 and the erased cells at the right; the two brackets are tinted, since each draws its counterpart's glyph. The third row, drawn in a right-to-left paragraph, starts with the four erased cells at the left, the cursor's column among them, then d e f, a space, the bracketed 1, a space, the reversed Hebrew word, a space and a b c at the right edge. Under each cell a small number gives the logical column it shows.](figures/ADR-1004-bidi.svg)

## Decision

### D1: the screen keeps logical order

The screen, its lines, the scrollback, the cursor, the margins and every escape sequence work in logical columns, exactly as they do without this record. A cursor position report, a rectangle operation, an insert or a delete counts logical columns. vtium-term computes the visual order only when a frame is taken (D8), and nothing it computes is stored in a line. Reflow (ADR-1003) and the image store work on logical lines as they do now.

DCSM (ANSI mode 9), ECMA-48's switch between editing the data and editing the presentation, is permanently set: editing always acts on the data, since the recommendation has escape sequences act on logical positions.

### D2: paragraphs and their properties

Each paragraph carries four properties, held in four of the nine spare bits of `LineFlags` (ADR-1001) on every row of the paragraph. WezTerm keeps its line's bidi state the same way, as bits of the line that each new line takes from the terminal's current mode (`basis`):

| Property | Set by | Default |
|---|---|---|
| implicit or explicit | BDSM, `CSI 8 h` and `CSI 8 l` | implicit |
| left to right or right to left | SCP `CSI 1 SP k` and `CSI 2 SP k`; `CSI 0 SP k` and `CSI SP k` restore vtium's default (D5); SPD `CSI 0 SP S` and `CSI 3 SP S` as aliases of SCP 1 and 2, as the recommendation allows and mintty documents | from `text-direction` (D5) |
| direction detected | DEC private mode 2501 | from `text-direction` (D5) |
| box drawing mirrored | DEC private mode 2500 | off |

The terminal keeps a current value of each, and a sequence that changes one always changes its current value. When the cursor stands at the first logical position of its paragraph, the first column of the paragraph's first row, the sequence also sets the new value on every row of that paragraph, even where the current value already had it, and the paragraph's other properties stay as they are. Anywhere else, the first column of the paragraph's second or a later row included, it changes no row, and the new value reaches only the paragraphs that start after it. This is the recommendation's rule in its chapter on how these sequences apply. For this rule the screen's first row starts a paragraph, so a paragraph whose first rows have scrolled into the scrollback starts again there.

A new row, one that a scroll brings onto the screen, takes the current values. A line feed, IND or NEL that moves the cursor into a new paragraph, on a new row or on one already on the screen, gives that paragraph the current values; one that moves the cursor to the next row of the same paragraph changes nothing, as the recommendation says. ED gives every row it clears whole the current values, as the recommendation says: ED 0 the rows below the cursor, ED 1 the rows above it, and ED 2 every row of the screen, the scrollback left out, and so does the clear that mode 1049 makes as it enters the alternate screen. Each such row ends its paragraph, as vtium-term already ends a cleared row's wrap. A wrap carries the paragraph's values to the row it starts. A rewrap (ADR-1003) gives every row it makes the values of the paragraph's first row. When the scrollback drops a paragraph's first rows, its remaining rows keep their values. Other SCP and SPD parameters are ignored.

RIS and DECSTR return the current values to their defaults and apply them to the cursor's paragraph, as SM and RM would, so to its rows only when the cursor stands at the paragraph's first logical position. RIS then clears the screen, which gives every row the defaults. DECRQM reports BDSM, 2500 and 2501 as set or reset, DCSM as permanently set, and 1243 (D11) as set or reset.

> Edited 2026-11-06: the review of the paragraph state's code against the Terminal Working Group's recommendation found three rules this section had stated otherwise or left out. Before this edit, a sequence set its value on every row of the cursor's paragraph wherever the cursor stood, a line feed gave the current values only to a new row, and ED left the rows it cleared as they were. The maintainer chose to follow the recommendation for a change sent in the middle of a paragraph (`considered`), and the line feed and ED rules take the recommendation's wording as well.

### D3: implicit mode

In implicit mode vtium runs the bidirectional algorithm over each paragraph and draws each row in the order it gives:

- The algorithm reads one character a cell: the first codepoint of the cell's cluster, since a cell's marks belong to its base and never move away from it, as the recommendation's chapter on combining characters requires. The second cell of a wide character, and the pad a wide character leaves when it moves to the next row (ADR-1003), go with the character.
- An erased cell inside a row reads as a space. The erased cells after a row's last written cell take no part in the algorithm, and they are drawn at the row's end in the paragraph's direction: at the right in a left-to-right paragraph and at the left in a right-to-left one, as the recommendation's chapter on erased cells says.
- The paragraph's level comes from its direction (D5): left to right, right to left, or, while detection is on, the first strong character's direction by rules P2 and P3, falling back to the paragraph's own direction when it has none.
- Rules X1 to I2 run over the whole paragraph, and rules L1 and L2 run a row at a time, so text never moves from one row to another. By rule L1 the spaces at a row's end take the paragraph's direction, so they stay at the row's end side instead of joining a right-to-left word before them.
- The explicit formatting characters (U+202A to U+202E and U+2066 to U+2069) and the marks U+200E, U+200F and U+061C take no cell of their own, since vtium-term gives format characters no width, and the algorithm reads none of them. This is the recommendation's first level. Honouring them is its second level, which the recommendation leaves for later.
- A paragraph of more than `BIDI_PARAGRAPH_ROWS_MAX` rows, 500 as the recommendation suggests, is drawn as explicit in its own direction, so the work a frame does for one paragraph is bounded.

### D4: explicit mode

In explicit mode vtium runs no algorithm. A left-to-right paragraph draws its cells left to right, as vtium does today. A right-to-left paragraph draws them right to left: logical column 0 at the right edge, its erased cells at the left. Mirroring (D7) applies to every character of a right-to-left paragraph in explicit mode, as the recommendation says.

A program that lays out right-to-left text itself sends `CSI 8 l` and then its rows in visual order, and gets them drawn as it sent them.

### D5: the paragraph direction and its detection

A paragraph runs left to right unless something says otherwise, as the recommendation's default has it and Konsole shows (observed). This default follows the convention of the terminals known to handle bidirectional text well: Konsole (observed), the recommendation's defaults, which VTE, GNOME Terminal's engine, implements, and Terminal.app, by the working group's survey, all start a paragraph at the left edge and detect nothing unless asked (the maintainer, 2026-11-06, `considered`). mintty detects the direction by default instead. A program says otherwise with SCP or mode 2501. A user says otherwise with a new key in vtium.conf:

- `text-direction = ltr`, the default: paragraphs run left to right and detection is off.
- `text-direction = rtl`: paragraphs run right to left and detection is off, for a console used mostly in a right-to-left language.
- `text-direction = auto`: detection is on, and a paragraph with no strong character runs left to right.

The key sets the default that RIS, DECSTR, `CSI 0 SP k` and a reset of mode 2501 return to. It changes no row already written.

### D6: Arabic joining, before reordering

Joining comes from logical order, and vtium decides each letter's form before it reorders the row:

- vtium-term computes, for each cell of a joining script, whether it joins the character before it and the character after it, by the rules of the Unicode Standard's section 9.2 and the Joining_Type property: marks (type T) are skipped, a letter of type D or R joins the one before it when that one is of type D, L or C, and a letter of type D or L joins the one after it when that one is of type D, R or C. The neighbours are the cells before and after it in the paragraph, so a word that wraps still joins across the wrap. Colors and other attributes do not break a join.
- In implicit mode the order is logical order. In explicit mode the program has already reordered, so vtium reads the cells in the order they are drawn from right to left. A program that writes the Arabic presentation forms, which it has shaped itself, is unaffected, since those forms are Non_Joining.
- The frame carries the two joining sides of each cell (D8). The console asks the glyph helper for the letter keyed with U+200D on each side it joins, the key the helper already shapes (ADR-1301, the listable clusters), and its lookup tries that key first. Lam followed by alef is asked for as the two-cell ligature the helper already shapes, drawn across the two cells where they sit after reordering.
- Where no source holds a joined form, as with a bitmap font, the console draws the letter's form from Unicode's Arabic Presentation Forms when the source holds that codepoint, as GNU Unifont does, and else the letter's own glyph. So the minspec build's built-in font joins Arabic too.
- A change that alters a cell's joining sides changes that cell's glyph, which counts as an ink change (ADR-1302 D9).

Reordering then moves whole cells. Since the joined forms are drawn for right-to-left reading, the letters of a word meet once the row is reversed. WezTerm takes the same order, shaping each run in logical order and then placing it (`basis`). kitty shows what goes wrong without reordering: it shapes each word as a right-to-left run but leaves the words in written order (observed).

### D7: mirroring

A character with Bidi_Mirrored=Yes at an odd level draws the glyph of its Bidi_Mirroring_Glyph, as rule L4 says and ADR-1302 D11 already decided; a mirrored character with no counterpart draws as it is, and no glyph is ever drawn flipped. Under mode 2500, a box-drawing character pairs with the character whose Unicode name swaps LEFT and RIGHT, taking the two halves around AND in either order, so that LIGHT LEFT AND HEAVY RIGHT pairs with HEAVY LEFT AND LIGHT RIGHT; a box-drawing character with no such partner draws as it is. The cell keeps the character the program wrote, so a copy brings back `)` where the screen shows `(`.

While mode 2500 is set, a box-drawing character (U+2500 to U+257F) in a right-to-left paragraph draws its left-right counterpart: the character whose Unicode name swaps LEFT and RIGHT, such as U+2510 BOX DRAWINGS LIGHT DOWN AND LEFT for U+250C BOX DRAWINGS LIGHT DOWN AND RIGHT, from a table `gen-unicode` builds from UnicodeData.txt. A character with no such partner draws as it is. A frame drawn for left-to-right text then reads correctly mirrored. Mode 2500 is off by default, since the recommendation says box drawing is not mirrored. Konsole draws `(1)` inside Hebrew as `)1(` (observed), and mlterm mirrors only a character whose visual neighbour is reversed (`basis`); vtium mirrors by the resolved level, as rule L4 says.

### D8: where reordering happens

vtium-term computes the drawing order when it builds a frame, and a renderer draws what the frame gives:

- The frame's rows give their cells in visual order, each carrying its logical column, its resolved direction, its joining sides and the character to draw after mirroring. `Frame::cell(row, col)` and `Frame::cells(row)` take visual columns. A left-to-right paragraph whose characters are all left to right or neutral takes a short path whose order is the identity, so a screen with no right-to-left text costs one scan of each changed paragraph's classes.
- The CPU and the GPU paths read the same frame, so they draw the same cells in the same places, and tier 2's comparison of the two paths covers right-to-left text with no new code in either path.
- **Damage.** vtium-term keeps change ages by screen position (ADR-1001). When a frame is taken, a paragraph that holds any right-to-left or joining character, or that is explicit and right to left, has all its visible rows marked changed whenever any of its cells changed since the caller's last frame. A paragraph of left-to-right text keeps the per-cell damage it has today.
- **Pictures.** A picture is never flipped. It is placed by the direction of the paragraph its anchor row belongs to: in a left-to-right paragraph where it is today, whatever the text under it does, and in a right-to-left paragraph mirrored as a whole, so its box starts at the right edge of its first logical column's cell and extends left. This is the anchor on the right that ADR-1003's rule for a picture wider than the screen names. A kitty placeholder cell is text, and its private-use character is left to right, so a run of placeholder cells keeps its order inside the run.
- **Double-size rows.** A double-width or double-height row reorders as any row does, over its half as many cells.

The computation is a pure function of the screen and the four properties, so a frame of the same screen is the same however the output reached it, which the determinism rules require.

> Edited 2026-11-06: building the frame found four points this section left open. A row whose last right-to-left character is erased draws in written order again, so a frame computed from the screen alone could not tell that its cells moved. Each line therefore keeps a flag besides the four paragraph properties, set when a right-to-left or joining character is written into it or a rewrap gives it one, and a paragraph that holds such a line redraws all its visible rows on any change; that leaves four of `LineFlags`' spare bits, not five. A change to a paragraph property marks every row whose properties it changes. The replay interface's screen and frame views keep printing each row in the order it was written, so their goldens hold in both builds, and the display view of D14 alone prints the drawn order.

> Edited 2026-11-06, after the review of the frame: a character with marks on it is mirrored too, as D7 says, which the first note had left as a limit. The frame carries the cell's mirrored first character beside the cell's own cluster, and a renderer draws the marks on it. An edit that ends a row's wrap splits its paragraph, and the rows of a split paragraph that draws in another order are marked changed. A paragraph of more than `BIDI_PARAGRAPH_ROWS_MAX` rows, 501 or more, draws as explicit; the first build resolved one of exactly 501.

### D9: the cursor

The cursor stays on its logical cell and is drawn wherever that cell is drawn, as the recommendation says. A bar cursor stands on the right side of its cell when the cell's character resolves right to left, and on the left side otherwise; this settles what ADR-1302 D8 left to this record. A block or underline cursor covers its cell. A cursor on an erased cell at a row's end is drawn where that erased cell is drawn (D3). A cursor waiting to wrap at the right margin is drawn on the row's last logical cell, as today.

### D10: the selection, copy and the mouse

- vtium-term maps a visual cell back to its logical cell inside every function the host calls with a pointer position, as the recommendation's display-only chapter has the mouse do and as mlterm keeps a visual-to-logical index for each line (`basis`): `Terminal::mouse`, the selection's `select_start`, `select_to`, `select_word` and `select_line`, and `Screen::set_pointer`. The host passes the cell under the pointer as it does today.
- The selection is the logical range between the cell where the drag started and the cell under the pointer, since the screen and every operation on it are logical (D1). In a row that mixes directions its cells can show in pieces on the screen. A word and a line selection take the logical word and the paragraph's rows, as today.
- Copy gives the selected text in logical order, the order it was written, with the characters the program wrote, never their mirrored glyphs.
- A mouse report carries the logical column, so a program addresses the cell it drew. With pixel reports (mode 1016), the pixel position is the logical cell's left edge plus the pointer's offset inside the cell it is over.
- A link's hover underlines each of its cells wherever they are drawn, and the overlay showing its URI draws the URI left to right, since a URI has no right-to-left reading.

### D11: the arrow keys

DEC private mode 1243, on by default, swaps the sequences the Left and Right arrow keys send while the cursor's paragraph resolves right to left, as the recommendation says. A program that edits a line in logical order, as readline does, then moves the cursor the way the arrow points over right-to-left text. Home and End are not swapped. With the default `text-direction = ltr` no paragraph resolves right to left unless a program or the user asks, so the swap changes nothing until then.

### D12: the modes and features, row by row

| ADR-1000 row | Status | DECRQM | Note |
|---|---|---|---|
| ANSI-8, BDSM | supported | 1 or 2, set by default | implicit and explicit mode (D3, D4) |
| ANSI-9, DCSM | ignored | 3, permanently set | editing always acts on the data (D1) |
| DEC-34, DECRLM | refused | 0 | the VT510's right-to-left cursor direction for a terminal that stores text in visual order; SCP and BDSM replace it, and xterm and foot do not document it |
| DEC-96, DECRLCM | refused | 0 | the same, for the VT510's right-to-left copy direction |
| new: DEC-1243 | supported | 1 or 2, set by default | the arrow keys (D11) |
| new: DEC-2500 | supported | 1 or 2, reset by default | box drawing mirrored (D7) |
| new: DEC-2501 | supported | 1 or 2, by `text-direction` | the direction detected (D5) |
| text.bidi | supported | | implicit mode, level 1 of the recommendation |
| new: SCP | supported | | the paragraph direction (D2) |
| new: SPD | supported, 0 and 3 only | | aliases of SCP 1 and 2 |

ADR-1000's rule for a new mode admits 1243, 2500 and 2501 through the use their rows name, since xterm documents none of them. The recommendation calls their numbers temporary; vtium takes the recommendation's numbers, which mintty documents too. The ADR-1000 edit that lands with the second slice (D15) sets these statuses and forms the new ids by the coverage script's rules. Each of these rows is supported only in a build with the `bidi` feature (D13), as the sixel rows are only in a build with `images`: their layer column names `bidi`, and their note says that a build without the feature has no such row in its mode table, so DECRQM answers 0, and draws every row in the order it was written.

### D13: a build feature, `bidi`, which the minspec build leaves out

Bidirectional text is a cargo feature, `bidi`, as pictures are the feature `images` (the maintainer, 2026-11-06, `considered`). The vtium package turns it on beside `images`, `logind` and `gpu`, and the minspec build leaves it out, so the minspec binary carries neither the algorithm nor its tables. The vtium crate's `bidi` feature turns on the same feature in vtium-term, which holds the algorithm, the paragraph state and the frame's order, and in the console's own code for the joining keys and the presentation forms.

A build without the feature has no bidirectional capability, the way a build without `images` has no pictures:

- It draws every row in the order its cells were written, left to right, as vtium does before this record and as Alacritty does (observed): Arabic and Hebrew read backwards, and Arabic letters stand unjoined, since joined forms laid out left to right would point the wrong way.
- The rows of BDSM, DCSM, 1243, 2500 and 2501 stay in vtium-term's mode table for its documents, and only a build with the feature has them, as only a build with `images` has the sixel rows (`crates/vtium-term/src/terminal/modes.rs`, `Row::built`). DECRQM answers 0 for each, SM and RM do nothing, and SCP and SPD are ignored.
- vtium.conf's `text-direction` key is accepted and changes nothing, as the image keys are in a build without `images`, and the key table says so.
- `vtium --version` lists `bidi` among a build's features when it has it.

In a build with the feature, vtium-term computes the levels and the order with servo's `unicode-bidi` crate, guarded (the maintainer, 2026-11-06, open question 3, `considered`). The crate passes every case of Unicode's conformance files, and more people have used and checked it than will use code vtium writes itself. Four guards hold while vtium uses it:

- **The pin.** vtium-term depends on vtium's fork of the crate, https://github.com/vtium/unicode-bidi, by its git URL and a revision, with `default-features = false, features = ["std"]`, and only its `bidi` feature turns the dependency on. The revision is a commit of the fork's branch `vtium/fixes`, which holds servo's main branch with vtium's fixes on top. The crate therefore changes only when a vtium change moves the revision.
- **vtium's classes.** vtium-term passes the crate each character's Bidi_Class and bracket pair from its own tables, at the Unicode Character Database version its other tables pin (18.0.0 today), through the crate's `BidiDataSource` trait, so the crate's own tables are never linked. vtium-term needs its own class table anyway, for rule L1, for detection and for joining, so the binary carries one copy of it. vtium's Unicode version then moves only when `gen-unicode`'s pins move. vtium-term generates its mirroring and Joining_Type tables with `gen-unicode` too, since the crate has neither.
- **The call path.** vtium-term resolves a paragraph once with the crate's `ParagraphBidiInfo::new_with_data_source`, reads its levels, applies rule L1 to each drawn row itself, and orders a row with `reorder_visual`. Clippy refuses `visual_runs`, `reorder_line`, `reordered_levels` and `reordered_levels_per_char`, because each of them copies and walks the whole paragraph's levels for the row it is asked about.
- **vtium's own checks.** vtium-term's tests run BidiTest.txt and BidiCharacterTest.txt at vtium's Unicode version through the crate as vtium-term calls it, with vtium-term's own classes and its own rule L1, and check the paragraph level, the levels and the visual order. Two more tests pin fixes of the fork's that no case in Unicode's files reaches: rule BD16's limit of 63 opening brackets, and `visual_runs` on an empty line. The screen's fuzz target drives that call path.

> Edited 2026-11-06: the third guard changed with the frame. Asking the crate's `reordered_levels_per_char` for each row of a paragraph copied and walked the whole paragraph's levels for every row, so a frame of one long paragraph cost its rows times its length: 9.4 ms for 400 rows of 80 columns. vtium-term now has the crate run rules X1 to I2 over a paragraph once, through `ParagraphBidiInfo::new_with_data_source` as before, and reads the result's public `paragraph_level` and `levels` fields. It applies rule L1 to each drawn row itself, with its own class table, as the crate's line functions apply it: a separator, the whitespace and isolate formatting characters before one and at the row's end take the paragraph's level, and a character rule X9 removed takes the level before it. It still orders a row only with `reorder_visual`, never calls `visual_runs` or `reorder_line`, which clippy refuses, and never hands the crate an empty paragraph, so the reasons for the guard hold: none of the calls it makes panics on the overflow input or on an empty line. BidiTest.txt and BidiCharacterTest.txt still pass in full through the new path, and the same frame takes 1.5 ms.

When a release of the crate ships the fixes now on its main branch (the guard against the level overflow, the Unicode 17 tables and the fix to bracket pairing), vtium moves to it, and the change that moves it reviews each guard against the new release: whether the call path may widen, and whether the pin moves.

> Edited 2026-11-06: the maintainer decided to fork the crate and pin vtium against the fork: "let's just fork unicode-bidi now, and pin against that. that way it's at least in our sphere", and "I don't like 2 year old out of date libraries even if they're popular". vtium-term now depends on vtium's fork of unicode-bidi, https://github.com/vtium/unicode-bidi, pinned by revision to `9a8c7b03be01def5cf7014b89d5af90f7382fdad`, with `default-features = false, features = ["std"]` as before. That revision is servo's main branch as it stood on 2026-09-15. The crate still calls itself 0.3.18 and still declares Rust 1.47 as the oldest Rust it builds on.
>
> The fork carries the fixes servo's maintainers made after the 0.3.18 release: the guard in `visual_runs` against the level overflow at the maximum implicit level (commit 9177c40); rule BD16 returning an empty list of bracket pairs when its stack of 63 openings overflows, as UAX #9 says, where 0.3.18 kept the pairs it had found before the overflow (410b31e); the Unicode 17 tables (67c0218); a fix to `ParagraphBidiInfo::has_rtl` (0db7f33); a fix to the iterator that walks an isolating run sequence backwards, which did not walk backwards (1743dcf); and its conformance test run in all three paragraph directions (ca8cb37). The four guards, reviewed against that revision:
>
> - **The pin** stays, in a new form: a git revision of vtium's own fork in place of `=0.3.18` from crates.io, so the crate changes only when a vtium change moves the revision.
> - **vtium's classes** stay. The fork's tables are at Unicode 17 and vtium-term's at 18.0.0, and the fork's bracket table still lacks U+2E62 and U+2E63, so rule N0 still needs vtium-term's bracket pairs to pair them.
> - **The call path** stays. `visual_runs` no longer panics on the level overflow input (9177c40), but it still panics with an index out of bounds on an empty line at the end of the text and on an empty text (src/lib.rs:946 at that revision). `visual_runs`, `reorder_line`, `reordered_levels` and `reordered_levels_per_char` each copy the whole paragraph's levels for the row they are asked about. So clippy still refuses all eight of them, and the root `clippy.toml` gives those reasons.
> - **vtium's own checks** stay. A correction to that guard: the crate's own conformance test does compare the visual order, in 0.3.18 and in the fork. It does not run vtium-term's classes at vtium's Unicode version through the path vtium-term calls, with vtium-term's own rule L1, and in 0.3.18 it ran BidiTest.txt's cases in the automatic paragraph direction only. Through the fork, vtium-term's tests pass all 770,241 cases of BidiTest.txt and all 91,707 of BidiCharacterTest.txt at 18.0.0.
>
> This settles the paragraph above about moving to a newer release: vtium moves the revision of its own fork, and the change that moves it reviews each guard again. It also resolves the Negative consequence about a pin to a release 23 months old.

> Edited 2026-11-07: the fork has a branch, `vtium/fixes`, and vtium-term now pins its commit `5927633397bc767eb23e0d0c80541272cb88b4ab`. Servo's main branch had no commits after `9a8c7b03be01`, so merging it into the branch changed nothing. The branch adds three changes of vtium's on top of it:
>
> - `visual_runs` returns no level runs for an empty line. Before, it panicked with an index out of bounds on an empty line at the end of the text and on any line of an empty text, and so did `reorder_line`, which calls it.
> - The tables and the conformance test data are at Unicode 18.0.0, the version vtium-term's tables pin. The bracket table gains U+2E62 and U+2E63.
> - The table generator reads each codepoint's class from DerivedBidiClass.txt, whose @missing lines give unassigned codepoints their defaults. Its own list of default ranges had fallen behind: at 18.0.0 it gave 3,916 codepoints a class other than the one DerivedBidiClass.txt gives, such as L in place of BN for the noncharacters.
>
> The crate's own tests pass on that commit, and so do its conformance tests: all 770,241 cases of BidiTest.txt and all 91,707 lines of BidiCharacterTest.txt at 18.0.0. The fork's README lists the changes too. The guards, reviewed against that commit:
>
> - **The pin** stays, at the new commit.
> - **vtium's classes** stay, for a different reason. The fork's tables now agree with vtium-term's, at 18.0.0 and with the same bracket pairs, so the version mismatch that was the reason no longer holds. The guard stays because vtium-term needs its own class table anyway, so passing it to the crate keeps one copy in the binary, and because vtium's Unicode version then moves with `gen-unicode`'s pins alone.
> - **The call path** loses its half about panics. The fork's `visual_runs` and `reorder_line` no longer panic on an empty line. vtium-term no longer skips the crate for an empty paragraph or an empty list of levels. That skip guarded no panic: `new_with_data_source` and `reorder_visual` accept empty input in 0.3.18 and in the fork alike. Clippy still refuses the eight methods, for their cost alone, and the root `clippy.toml` gives only that reason now.
> - **vtium's own checks** stay, with two tests added. `bracket_pairing_stops_when_its_stack_overflows` checks that one opening bracket past BD16's stack of 63 leaves every bracket of the isolating run sequence unpaired. UAX #9's BD16 says to "stop processing BD16 for the remainder of the isolating run sequence and return an empty list", and the fork follows it, where 0.3.18 kept the pairs it had found before the overflow. `the_forks_visual_runs_accepts_an_empty_line` calls `visual_runs` and `reorder_line` on an empty line. Both tests fail when vtium-term is pointed back at 0.3.18.
>
> This note also rewrites the four guards above to the state at this commit. Before, the pin read `unicode-bidi = "=0.3.18"`; vtium's classes kept out "the crate's own Unicode 16 tables"; the call path went "only through `reordered_levels_per_char` and `reorder_visual`, never through `visual_runs` or `reorder_line`, and never with an empty line range, because `visual_runs` in 0.3.18 panics on an input its maintainers were sent and on any empty range"; and vtium's own checks covered the visual order, "which the crate's own tests do not check". The notes of 2026-11-06 above record how the frame and the fork had already changed those guards.

Measured in the vtium package's build, the crate fed Unicode 18 classes adds 34,704 bytes; with its own Unicode 16 tables it would add 46,832 (`basis`). The minspec build is unchanged.

### D14: tests

- **The algorithm** is tested against Unicode's own conformance files for the pinned version, BidiTest.txt and BidiCharacterTest.txt, copied into vtium-term's tree with the Unicode licence as GraphemeBreakTest.txt is.
- **Tier 1:** the replay interface gains a display view, under a new feature `bidi` in its versioning, that prints each row of the screen in visual order with mirrored characters drawn as their counterparts. Replay cases cover each rule of D2 to D11: the modes and their DECRQM answers, SCP and SPD, a paragraph that wraps, the erased cells, detection, explicit mode in each direction, joining sides across a wrap, mirroring, box drawing under 2500, the cursor's drawn cell and side, a mouse report from a reordered cell, the arrow keys under 1243, and a reflow that carries the properties.
- **The probe `bidi`** in `tools/conformance/scenarios/bidi.scenario` stops being a placeholder: it prints a mixed line in the default mode and reads it back through the display view, as the first line of `observable` says.
- **Selection and copy:** vtium-term tests that a drag across a mixed row selects the logical range and copies it in logical order, and that word and line selections behave as D10 says.
- **Tier 2** gains vtium's own screens, provisional until the maintainer reviews them as pictures: mixed Arabic, Hebrew and Latin lines with digits and brackets in a left-to-right and a right-to-left paragraph; a line in explicit mode in each direction; the block and the bar cursor on a right-to-left cell; a selection across a mixed line; and the same Arabic lines in the built-in GNU Unifont, joined through the presentation forms. Following ADR-1503 D9's rule of one well-known open font for each script, tier 2's font set gains Noto Sans Arabic and Noto Sans Hebrew, each pinned by URL and SHA-256 and limited with `cover=` to its script's blocks. They follow DejaVu Sans Mono, whose own `cover=` lines leave out the Hebrew and Arabic blocks, so each script draws from its own font, and DejaVu Sans Mono, the first regular source, still sets the cell and the size every later outline font draws at (ADR-1200 D17). The slice checks that no accepted golden moves when the fonts join the set, apart from those that show Hebrew or Arabic.

> Edited 2026-11-06: building the screens found that the glyph helper takes the cell, and the size of every later outline font, from the frit's first regular source whatever its `cover=` (ADR-1200 D17), so two Noto fonts listed first would have set the cell and moved every golden drawn in the test font set. The maintainer agreed that DejaVu Sans Mono stays first with `cover=` lines around the Hebrew and Arabic blocks, which keeps the intent, each script from its own font, without moving the cell. Only the four ink screens that show Arabic letters side by side, with their twins, moved, as this record's Consequences foresaw. The sentence read: "each pinned by URL and SHA-256 and limited with `cover=` to its script's blocks, ahead of DejaVu Sans Mono, so each script draws from its own font. The slice checks that no accepted golden moves when the fonts join the set."
- **Both builds:** the tests of bidirectional behaviour carry `#[cfg(feature = "bidi")]` and run under the gate line's `cargo test --workspace --all-features`; tests without the feature, under its `cargo test --workspace`, check that such a build answers DECRQM 0 for the bidi modes and draws a mixed line in the order it was written. The conformance files run against the crate as vtium-term calls it, with vtium-term's classes and its own rule L1, the visual order included.

  > Edited 2026-11-07: the crate's own conformance tests do check the visual order, through the crate's own tables and its own line functions, so the last sentence no longer says they do not. It read: "The conformance files run against the crate as vtium-term calls it, the visual order included, which the crate's own tests do not check."
- **Fuzzing:** the screen's fuzz target reaches the new modes and printing with the feature on, and the author of each slice that changes code it reaches runs it, as AGENTS.md says.

### D15: the work, in slices

Each slice is a card of its own, branched from `integration`, and lands in this order.

1. **The feature, the tables and the crate** (vtium-term, the vtium crate's manifest, `gen-unicode`): the `bidi` feature in vtium-term and the vtium crate, the generated Bidi_Class, mirroring and Joining_Type tables, the crate behind the feature with its four guards (D13), the joining sides and the mirroring lookup, with the conformance files' tests. Gates: fmt, clippy and the tests of vtium-term with and without the feature; the generator's own tests (`cargo test --manifest-path crates/vtium-term/gen-unicode/Cargo.toml`), which `make test-tools` does not run; `make minspec`, whose size must not change, and the size of the package's build, recorded.
2. **The paragraph state** (vtium-term): the four line flags, BDSM, DCSM, SCP, SPD and modes 1243, 2500 and 2501 in the mode table, DECRQM, the resets, the rows a wrap, a scroll and a rewrap make, and the `text-direction` key in the key table and vtium.conf's example, each mode row built only with the feature; with the ADR-1000 edit, whose rows name the `bidi` layer and say what a build without the feature answers, and `docs/scripts/coverage --update` in commits of their own. Gates: vtium-term's and vtium-settings' tests, `make conformance`, the regenerated `docs/modes.md`, `docs/scripts/coverage`.
3. **The frame** (vtium-term): visual order, mirroring, joining sides, the damage rule, the cursor's cell and side, the selection's and the mouse's mapping, the arrow keys, pictures and placeholder runs; the replay interface's display view, the probe `bidi`, and the thirteen streams the prior-art observations used, as replay cases. Gates: vtium-term's tests, `make conformance`, `make conformance-probes`, the fuzz target.
4. **Drawing** (vtium, vtium-render): the console's joining keys in its asks and its lookup, the presentation-form fallback in `gen-lookup`'s tables, the bar cursor's side, all behind the feature. Gates: the tests of vtium and vtium-render with and without the feature, `make tier2` with the feature, `make tier3-minimal` unchanged.
5. **The screens** (tools/tier2, tools/specimens): the pinned fonts, tier 2's screens and their provisional goldens, and the specimens regenerated, each set of goldens in a commit of its own that the maintainer reviews as pictures. Gates: `make tier2`, `make specimens-check`, and a tier 3 screen of a shell typing Arabic if the maintainer wants one for the VT review.

## Consequences

### Positive

- A shell, `ls`, `cat`, `less`, `git` and every other program that writes logical order show right-to-left text readably with no setting, in the built-in font too, in every build with the `bidi` feature.
- The minspec build pays nothing for bidirectional text: not a byte of its size budget, not a mode in its table.
- Every escape sequence, cursor report and mouse report means what it means today, so no full-screen program's layout breaks because a row holds Hebrew.
- A program that knows the recommendation, or any program that sends `CSI 8 l`, keeps full control of its own layout, and can ask vtium which mode is on.
- One computation in vtium-term serves both drawing paths, the selection, the mouse and the replay view, so tier 1 can judge reordering without pixels.
- Arabic joins in every font vtium draws: outline fonts through the helper's shaped forms, bitmap fonts through the presentation forms.

### Negative

- A console built with the minspec build shows Arabic and Hebrew backwards and unjoined, and the minspec packages say so; an Arabic or Hebrew reader on the smallest board needs the package with the feature, or a minspec build with `bidi` added and the size budget it then passes or fails.
- vtium-term gains its first dependency outside the workspace, pinned to a release that is 23 months old with fixes waiting on its main branch, and called through a restricted path until a release carries them; vtium-term's README says so where it says it has no other dependency.

  > Edited 2026-11-06: resolved by D13's note of this date. vtium-term now depends on vtium's fork of the crate, pinned by revision, which carries those fixes.

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

Each question carries the lean this record wrote as its decision. The maintainer has decided all six, five of them with the lean and the second as a build feature, and each answer is in `considered`.

1. **The default paragraph direction and detection.** Decided by the maintainer through the probe on where a line of Arabic sits (2026-11-06, `considered`): left to right with detection off, following the terminals known to do this well, and a `text-direction` key (D5). For: the recommendation weakly favours detection off, and Konsole keeps an all-Arabic line at the left edge (observed); a screen keeps one alignment; arrow-key swapping changes nothing until asked for. Against: a console used mostly in Arabic shows each Arabic line at the left edge until its user sets the key, where mintty, mlterm and PuTTY detect each line; mintty's own page warns that detection spoils tables drawn with box drawing. Check: the recommendation's escape-sequences chapter, mintty's CtrlSeqs page, and the observed screenshots.
2. **The minspec build.** Decided by the maintainer (2026-11-06, `considered`): bidirectional text is a cargo feature, `bidi`, which the vtium package turns on and the minspec build leaves out, with no bidirectional capability in a build without it (D13).
3. **A dependency on the `unicode-bidi` crate.** Decided by the maintainer (2026-11-06, `considered`): vtium-term uses servo's `unicode-bidi` crate, guarded as D13 says. The case for it: it passes every case of Unicode's conformance files at 16.0.0 and 18.0.0, levels and visual order alike; 223 crates depend on it, and it is downloaded about a million times a day; fed vtium's classes, it costs 34,704 bytes in the package's build. The case against, which the guards answer and do not remove: its one release in two years is 0.3.18, of 2024-12-16; the guard against a panic its `visual_runs` hits on a 3.6 KB input, the Unicode 17 tables and a fix to bracket pairing wait unreleased on its main branch, so vtium pins a release and restricts its calls until a newer one comes; vtium-term gains its first dependency outside the workspace; and WezTerm wrote its own crate instead.
4. **Implicit mode on the alternate screen.** Decided by the maintainer (2026-11-06, `considered`): on, as on the main screen. For: `less`, `man`, `nano` and `mc` write logical order and need it; the recommendation draws no line between the screens. Against: the programs that reorder themselves, Emacs and Vim with `arabic`, are full-screen programs, and turning implicit mode off on the alternate screen would serve them with no setting; mintty offers exactly that as its option Bidi=1, though not as its default. Check: the Emacs manual's Bidirectional Editing, Vim's `termbidi` and mintty's CtrlSeqs page in `basis`.
5. **Joining in explicit mode.** Decided by the maintainer (2026-11-06, `considered`): join by the drawn order, read right to left (D6). For: a program that sends visual order and unshaped letters gets joined text, and one that shapes itself sends presentation forms, which never join, so nothing it draws changes. Against: the recommendation says the terminal does no shaping in explicit mode. Check: the recommendation's basic-modes chapter and DerivedJoiningType.txt.
6. **The selection in a mixed row.** Decided by the maintainer (2026-11-06, `considered`): the logical range, which can show in pieces (D10). For: copy gives the text between the two points as it is read, and every other operation on the screen is logical. Against: mlterm selects the cells between the two points as drawn and copies them in logical order (`basis`), which looks contiguous on the screen but can copy text that was never adjacent. Check: the recommendation's survey of terminals and mlterm's vt_line_bidi.c:554.

## Prior art

vtium copies no code from any of these programs. Where a pattern helps, the record takes the pattern, meaning how a program structures the problem or which behaviour it chooses, and says so in the decision it shapes. vtium reads the source only of permissively licensed programs, and only for their structure. It knows copyleft programs only from their published documentation and from running them. "Observed" means the program was run on this machine with the thirteen byte streams `basis` lists. Those streams become tier 1 cases in the third slice (D15).

| Terminal | Licence | vtium may read its source | What it does | Pattern vtium takes | How known |
|---|---|---|---|---|---|
| The C reference | MIT | yes | No reordering; each codepoint's glyph drawn unshaped, so Arabic drew isolated and Hebrew reversed; an upstream `kmscon` user reported Arabic not shown | none | its source and probes, read |
| VTE (GNOME Terminal) | LGPL-3.0 | no | Level 1 of the recommendation, whose author wrote it; mode 2500 mirrors box drawing; the I-beam shows the direction under it | none taken from VTE itself; the recommendation it implements is D1 to D11's source | documentation only (TWG pages) |
| mintty | GPL-3.0 | no | Bidi on by default; BDSM, SCP, SPD, modes 2500 and 2501, detection on by default; private modes to switch bidi off globally or for a line; an option to switch it off on the alternate screen | the recommendation's sequences and numbers, which it shares (D2, D12) | documentation only (its CtrlSeqs page) |
| Konsole | GPL | no | Reorders every line as a left-to-right paragraph; joins Arabic; does not mirror brackets; ignores BDSM reset, SCP 2 and 2501; answered none of the DECRQM queries it was asked before the capture | its default placement of an all-Arabic line at the left edge (D5) | observed, 26.08.2 |
| kitty | GPL-3.0 | no | No reordering; each word shaped as a right-to-left run, so words keep written order; DECRQM 0 for every bidi mode | none; it shows why joining must come before reordering whole cells (D6) | observed, 0.49.2 |
| WezTerm | MIT | yes | Implicit mode off by default; BDSM and SCP; each row resolved on its own in the GUI renderer; runs shaped in logical order; no bidi in its selection or mouse code; its own UAX #9 crate | the paragraph's properties as bits of each line, taken from the terminal's current mode (D2); shaping in logical order before placing (D6) | source read for structure, e68a8ec277 |
| mlterm | BSD-3-Clause | yes | Reorders through FriBidi, detecting each line's direction by default; the model turned into visual order in place for drawing and back; mirroring by visual adjacency; selection by visual range copied in logical order; Arabic through presentation forms | a two-way index between logical and visual columns for the cursor and the pointer (D9, D10); presentation forms for fonts that hold them (D6) | source read for structure, 610985245c |
| PuTTY | MIT | yes, not read | Reorders every line, detecting its direction; strips the bidi controls; no way to switch reordering off | none | the TWG survey only |
| Terminal.app | proprietary | no | Reorders as implicit left to right | none | the TWG survey only |
| Contour | Apache-2.0 | yes | No bidi, by its own text-stack document | none | its documentation |
| Alacritty | Apache-2.0 | yes | No reordering and no joining; DECRQM 0 for every bidi mode | none | observed, 0.17.0 |
| xterm, foot, Ghostty | X11, MIT, MIT | yes | No reordering, by ADR-1000's columns | none | ADR-1000's columns |

## Alternatives Considered

- **Reorder in vtium-render, as the work item first proposed.** Rejected: the selection, the mouse, the arrow keys and the replay view all need the same order, and they live in vtium-term; computing it in the renderer would compute it twice, once in each drawing path, and keep tier 1 blind to it.
- **Store each row in visual order.** Rejected: every escape sequence, reflow and the selection would need translating, and a program could no longer address the cell it wrote.
- **Implicit mode off by default, as ECMA-48's BDSM default reads.** Rejected: the programs that write logical order cannot be told to switch modes, which is why the recommendation makes implicit mode the default.
- **Detect every paragraph's direction by default, as mlterm and PuTTY do.** Not taken, for now: open question 1.
- **Each row its own paragraph.** Rejected: a sentence that wraps would be reordered in pieces, and the paragraph's level would change from row to row of one sentence.
- **Shape across cells with the full shaper at draw time.** Rejected: the console never parses fonts (ADR-1200), and joining forms are the listable clusters the helper already makes.
- **DECRLM and DECRLCM.** Refused (D12): they belong to terminals that stored text in visual order.
- **vtium's own implementation of UAX #9, as WezTerm wrote its own.** Not taken (open question 3): the crate's conformance is complete and it is far more widely used, and once vtium feeds it its own classes the two differ in size by at most about 8 KB.
- **Bidirectional text in the minspec build.** Rejected by the maintainer (open question 2): the minspec build is the smallest console, and a reader of right-to-left scripts takes the package with the feature.
- **Honour the explicit formatting characters now, the recommendation's second level.** Deferred: it needs those characters kept in the screen, which they are not today, and no program in the target set sends them for layout.
