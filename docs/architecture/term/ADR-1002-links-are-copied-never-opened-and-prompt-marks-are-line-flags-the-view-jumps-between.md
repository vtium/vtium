---
contract: adr/v1
kind: decision
verb: add
capability: screen
basis:
  - operator: aaronsb
    level: guided
    said: "The record decides the link table and its bound, what opening a link means on a VT with no browser, how marks survive scrollback and reflow, and the key bindings that jump between prompts."
    via: "the GitHub issue for OSC 8 hyperlinks and OSC 133 prompt marks, its body, filed under the operator's account in a session where agents drafted issues, so possibly agent-written"
  - operator: aaronsb
    level: guided
    said: "Done: the probes pass, and a prompt jump lands on each marked prompt in the scrollback."
    via: "the same issue's body, as above"
  - precedent: ADR-1001
  - precedent: ADR-1000
  - precedent: ADR-1400
  - upstream: "The C reference dropped both: libtsm kept at most 127 bytes of an OSC string and handed an OSC it did not take to the host (`src/tsm/tsm-vte-int.h:31`, `src/tsm/tsm-vte.c:2912-2921, 3004-3015` at the C reference's libtsm at eeaf293), and kmscon's host took only `setBackground` and `setForeground` (`src/terminal.c:859-870` at the C reference's kmscon at b1f7a7c5); its probes `osc-8-hyperlink` and `osc-133-marks` are placeholders (`tests/capabilities/probes.replay:435-442`)"
  - standard: "Hyperlinks in terminal emulators (Egmont Koblinger's OSC 8 specification, gist eb114294efbcd5adb1944c9f3cb5feda): the parameters and URI are bytes 32 to 126; VTE and iTerm2 limit the URI to 2083 bytes and VTE the id to 250; cells with the same URI and the same non-empty id are one link; a terminal should not open a link on a simple click and should let the user know the URI first"
  - standard: "Semantic prompts (Per Bothner's proposal, the FinalTerm OSC 133 sequences), read through WezTerm's parser: L fresh line; A fresh line and prompt start; N end of command, fresh line and prompt start; P prompt start; B and I input start; C output start; D command end with its status (`wezterm-escape-parser/src/osc.rs:706-747` at wezterm e68a8ec)"
  - upstream: "kitty 4f7dea3: a 16-bit link id per cell into a pool per screen (`kitty/data-types.h:91-93`), keyed by id and URI together, truncated at 2048 bytes, swept when full and then stripped of the scrollback's links (`kitty/hyperlink.c:12-13, 136-163`); links underlined on hover and opened on a click (`kitty/options/definition.py:1012-1035, 1321, 3259-3270`); a 2-bit prompt kind per line set without a fresh line (`kitty/line.h:94`, `kitty/screen.c:3627-3650`); prompt jumps on Ctrl+Shift+Z and X, on the main screen only (`definition.py:4027-4035`, `screen.c:3655-3657`)"
  - upstream: "VTE 5d75dcf: URI at most 2083 bytes and id 250, at most 2^20-2 links per screen (`src/vtedefines.hh:101-119`); an overlong id is ignored and an overlong URI ends the link (`src/vteseq.cc:1774-1820`); every link drawn with a dashed underline, the hovered one highlighted (`src/vte.cc:9474-9677`)"
  - upstream: "foot 152487e: per-row URI ranges, each with its own copy of the URI, and per-row prompt marker and command columns (`terminal.h:103-159`); OSC 133 A marks the row without a fresh line (`osc.c:1615-1650`); links underlined in URL mode only, opened through `xdg-open`, labelled for the keyboard by Ctrl+Shift+O, and prompt jumps on Ctrl+Shift+Z and X (`doc/foot.ini.5.scd:870-890, 1393-1463`)"
  - upstream: "Ghostty 9ee02ae: a link bit per cell with a map and a reference-counted set per page, a 2-bit prompt state per row and a 2-bit semantic content per cell (`src/terminal/page.zig:217-218, 2079-2121, 2214-2219`); an OSC 8 link is live only while Ctrl is held on Linux (`src/Surface.zig:4446-4472`); link previews (`src/config/Config.zig:1474-1492`); prompt jumps on Ctrl+Shift+Up and Down (`Config.zig:6980-6985`); A does a fresh line (`src/terminal/Terminal.zig:2161-2164`)"
  - upstream: "WezTerm e68a8ec: a shared link pointer in a cell's extra attributes and a 2-bit semantic type per cell (`wezterm-cell/src/lib.rs:95, 184-214`); a click opens the link (`wezterm-gui/src/inputmap.rs:185-237`); `ScrollToPrompt` bound to no key by default (`docs/config/lua/keyassignment/ScrollToPrompt.md`); xterm 5c75ead implements neither OSC 8 nor OSC 133 (`ctlseqs.txt`)"
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Yes: a link never opens anything; Ctrl+click only copies its address. A keyboard link mode for consoles without a mouse is acceptable as a later, low-priority issue."
    via: the session's ADR questions, 2026-10-27
    paraphrase: true
    covers: [nothing opens, no mouse]
  - operator: aaronsb
    said: "The overlay row covers only the address's own cells; the rest of the row shows what the program drew."
    via: the maintainer's answers in the steering session's interview, 2026-11-01
    paraphrase: true
  - operator: aaronsb
    said: "Keyboard link hints are built in this wave."
    via: the steering session's questions on 2026-11-14, on the wave plan
    paraphrase: true
    covers: [no mouse]
  - operator: aaronsb
    said: "Ctrl+Shift+O starts link hints, on the normal screen and in full-screen programs alike, as written."
    via: the steering session's questions on 2026-11-14, on the small records
    paraphrase: true
  - operator: aaronsb
    said: "Hints label only links marked with OSC 8, as written."
    via: the steering session's questions on 2026-11-14, on the small records
    paraphrase: true
  - operator: aaronsb
    said: "The hint line stays on the bottom row, and moves to the top row only while a label lies on the bottom row, so no label is ever hidden under it."
    via: the steering session's questions on 2026-11-15, on the code review of link hints
    paraphrase: true
  - operator: aaronsb
    said: "When a later label would overwrite part of an earlier one, the later label shifts right just far enough to clear the earlier one, so both read in full."
    via: the steering session's questions on 2026-11-15, on the code review of link hints
    paraphrase: true
observable:
  - "A replay case opens a link, prints across a wrap, scrolls it into the scrollback and resizes; each cell keeps its link, and a sweep frees only links no line names"
  - "A flood of distinct links past `LINKS_BYTES_MAX` strips the oldest scrollback lines' links first and keeps every link on the screen; fed whole and chunked, it ends in the same store"
  - "An over-long URI, an over-long id, a byte outside 0x21-0x7E and a URI with no scheme each give the state their rule names"
  - "On a VT: hovering a link underlines all of it, Ctrl shows its URI on the bottom row, and Ctrl+click then the middle button pastes the URI into the shell"
  - "With a shell that sends OSC 133, Ctrl+Shift+Up lands each marked prompt in the scrollback at the top of the view, one press each, and Ctrl+Shift+Down returns to the live screen"
status: accepted
date: 2026-10-27
deciders:
  - aaronsb
related: []
---

# ADR-1002: Links are copied, never opened, and prompt marks are line flags the view jumps between

## Summary

- **Decided:** OSC 8 links are kept per cell through ADR-1001's link index, in one link store bounded at 256 KiB that holds each distinct link once. A link is underlined while the pointer is over it, shows its URI on an overlay row while Ctrl is held, and Ctrl+click copies the URI into the selection buffer the middle button and Shift+Insert paste. Nothing is ever opened or run. OSC 133 marks set ADR-1001's three line flags; Ctrl+Shift+Up and Down move the view between marked prompts, and Ctrl+right-click copies the output of the command under the pointer. Link hints copy a link with the keyboard alone: `key-link-hints`, Ctrl+Shift+O by default, labels each visible link with letters, and typing a label copies that link's address (section 7, proposed).
- **Trades away:** No link opens a browser or any command. A link draws nothing until it is hovered or labelled. Ctrl+Shift+O never reaches a program while it is bound to link hints. On the main screen, Ctrl+Shift+Up and Down never reach a program. Past 256 KiB of distinct links, the oldest scrollback lines lose theirs.
- **One-way?** No. Reversible: the cell's link index and the line flags exist already, and nothing outlives the process.
- **Probes:**
  - *Confident (nothing opens):* a link on the console never starts a browser or any other program; Ctrl+click puts its address in the paste buffer, and you paste it where you choose, such as after `w3m` at a shell prompt.
  - *Not confident (no mouse):* on a console with no mouse, links do nothing until a keyboard way to pick one is built, as foot labels every link on the screen with a letter to type; that is left to its own issue.
- **Inversion:** Between "the console as a launcher", where kitty, foot, Ghostty and WezTerm open a link through the desktop's opener, and "links parsed and dropped", as the C reference had them. The answer sits between: links kept, shown and copied, with the launch left out.

> Edited 2026-11-14: section 7 is new and proposed, and it waits for the maintainer's reading. The maintainer decided that keyboard link hints are built in this wave (`considered`), which answers the probe on a console with no mouse. The Decided line gains its last sentence. The Trades away line read: "No link opens a browser or any command. A link draws nothing until it is hovered, and a console with no pointer cannot use links." Section 3's bullet on a console with no pointer, section 6's last option and the Negative consequences change with it, each with a note.

## Context

OSC 8 attaches a URI to the cells printed while it is open; OSC 133 marks where a shell's prompt, the command typed and its output begin. ADR-1001 placed both in the cell model: each cell carries a 16-bit index into its line's link table of 4-byte `LinkId`s, which name entries in "the link store that the work on OSC 8 hyperlinks builds", and each line carries prompt, input and output flags. This record decides the store, what a link may hold and does, how both are drawn, and what the marks are for.

The C reference dropped both sequences: libtsm kept 127 bytes of an OSC string and handed OSC 8 and OSC 133 to kmscon, which took only its own two strings. Its probes for both are placeholders. ADR-1000 lists `osc.8` and `osc.133` as supported, at the `top` layer.

The console shapes the choices. vtium runs on a VT with no desktop, no browser and no opener such as `xdg-open`; its main process holds root for life (ADR-1400). It has a pointer, keyboard grabs, and a selection buffer that the user's copies and OSC 52 fill and the middle button and Shift+Insert paste. At minspec, ADR-1001's worst default scrollback, held to `sb-memory`'s default, stays inside the 16 MiB peak.

What other terminals do, from their source at the commits the basis names:

| Terminal | Link storage | Link bound | What a link does | Drawn | Prompt marks | Prompt jump |
|---|---|---|---|---|---|---|
| kitty | 16-bit id per cell, a pool per screen | id and URI truncated at 2048 bytes together; 65,535 links, swept, then scrollback links dropped | a click opens it; `ask` asks first | underlined on hover | 2 bits per line; no fresh line | Ctrl+Shift+Z, X; main screen only |
| VTE | 20-bit index per cell, a pool per screen | URI 2083 bytes, id 250; longer URI ends the link | handed to the application that embeds VTE to open | dashed always, hovered one highlighted | none | none |
| foot | per-row ranges, the URI copied into each | none | opened by `xdg-open` from URL mode, Ctrl+Shift+O, by typed labels | underlined in URL mode only | a flag and two columns per row; no fresh line | Ctrl+Shift+Z, X |
| Ghostty | a bit per cell, a map and counted set per page | the OSC string's 2048 bytes | live only while Ctrl is held; Ctrl+click opens | underlined while Ctrl is held; preview | 2 bits per row and 2 per cell; fresh line on A | Ctrl+Shift+Up, Down |
| WezTerm | a shared pointer in a cell's extra attributes | none found | a click opens it | underlined on hover | 2 bits per cell | an action, bound to no key |
| xterm | none | | | | none | none |

## Decision

### 1. Link storage

A cell names its link through ADR-1001's 16-bit index into its line's link table, and the table's `LinkId`s name entries in one link store per terminal, shared by the main screen, the alternate screen and the scrollback. An entry holds the link's id parameter and its URI. An OSC 8 that opens a link the store holds already, by the same id and URI, takes that entry, so a program that sends its links again on every redraw adds nothing. Links with no id and the same URI are one link, as kitty keys them.

The store is bounded by `LINKS_BYTES_MAX`, 256 KiB, a row of vtium-term's bounds table. Each entry is charged its id and URI bytes plus 64 for its slot, its index entry and allocation, and a test pins the store's heap at or under its charge. When an open would pass the bound, the store sweeps: it reads every line's link table on both screens and in the scrollback, and the hovered link, and frees each entry nothing names; freed `LinkId`s are reused. When the sweep leaves less than a quarter of the bound free, the scrollback's oldest lines lose their links first, each line's link indices cleared and its table compacted, until a quarter is free. When the screens alone hold more than the bound, the open is refused and the text prints unlinked. Sweeps and strips run on stream events only, so the store follows the bytes (docs/architecture.md, "Determinism").

A sweep reads each line's link table once, so its cost is linear in the lines held, about 100,000 at the largest cap, and it runs once per quarter-bound of new link bytes, at least 64 KiB, about 570 links of 50-byte URIs.

What links cost, against ADR-1001's 48-byte line and 8-byte cell:

| What a line holds | Its cells | Its line | The store |
|---|---|---|---|
| No link | nothing: the index is inside the 8-byte cell | nothing | nothing |
| One link, on a line with styles already, as `ls --color --hyperlink` writes | nothing | 4 bytes in its link table | the id and URI and 64, once for every line that names it |
| One link, on a line with no tables | nothing | 84 bytes: the 80-byte table block and 4 | as above |
| A link per cell, 170 cells, ADR-1001's worst line | nothing | 680 bytes once trimmed, which ADR-1001's worst case counts | within 256 KiB in all |

A `file://` URI of 50 bytes is charged 114, so the store holds about 2,300 such links. With a full store, ADR-1001's worst default scrollback at the minspec grid, held to `sb-memory`'s default, comes to 16,776,192 bytes (16.00 MiB) against the 16 MiB peak; at the 10,000-line cap the store adds its 0.25 MiB to 63.2 MiB.

> Edited 2026-11-02, with `sb-memory`'s default following the console's grid and the screens priced at their worst: the worst default scrollback came to "about 15.99 MiB".

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| A link index per cell into a store of distinct links | nothing per cell; 4 bytes per link per line; a sweep | VTE, kitty | Taken |
| Runs per line, a start, an end and a `LinkId` | 8 bytes per run; the cell stays 8 bytes, since its 4-byte content word aligns it; an overwrite inside a run splits it, and inserted and deleted characters shift every later run | foot | Rejected: it saves no byte and makes every edit of a linked line edit its runs |
| The URI in each line's table, with no store | no sweep; but a link redrawn or wrapped is copied per line, and one line of 170 distinct 2,083-byte links is 354 KB | foot | Rejected: unbounded by anything but the scrollback |
| A link bit per cell and a map from cell to link per line | about 8 bytes per linked cell in the map | Ghostty | Rejected: costs more than the index ADR-1001 already pays for |
| Reference counts on store entries | a line leaving the scrollback must reach the store to count down, so each table entry becomes a counted pointer, 16 bytes in place of 4 | Ghostty's set, WezTerm's shared pointer | Rejected for the sweep |
| When full, refuse new links until old ones scroll away | simplest | VTE's cap | Rejected: links on the screen stop working while dead ones in the scrollback hold the store |

### 2. What a link may hold

`OSC 8 ; params ; URI ST` opens a link, and an empty URI closes it. The URI is 1 to 2,083 bytes, VTE's and iTerm2's limit, each between 0x21 and 0x7E, and starts with a scheme as RFC 3986 §3.1 defines one, letters, digits, `+`, `-` and `.` after a first letter, then `:`. A URI that fails any of these closes the open link, so the text that follows prints unlinked, as VTE does with an over-long URI. A URI is never shortened: a shortened address copies as a different one, as kitty's 2,048-byte key can.

The parameters are `key=value` pairs separated by `:`. Only `id` is read: 1 to 250 bytes between 0x21 and 0x7E, VTE's limit. A longer or malformed id is ignored and the link has none; other keys are ignored.

The specification allows bytes 32 to 126; vtium also refuses 32, the space, which a URI percent-encodes. With every byte printable ASCII, the overlay that shows a URI shows no control character, no bidirectional override and no lookalike letter: what it shows is what the copy carries. With nothing opened, vtium keeps no list of allowed schemes; the overlay shows the scheme, and a `file://` URI's host, which the specification says decides whether a file is local.

The open link is part of the pen: printed cells take it, erased cells take none, and RIS and a switch between the main and alternate screens end it, as VTE ends it. OSC 8 has no query, so no link text reaches a reply.

### 3. What a link does

- **The pointer over a link** underlines every visible cell of that link (section 4).
- **Ctrl held over a link** also shows its URI on an overlay row (section 4).
- **Ctrl+left click on a link** copies its URI into the selection buffer, replacing what it held, and clears the selection on the screen; the middle button and Shift+Insert then paste it, filtered and bracketed as any paste. Nothing is opened, run or sent to the program. When a program tracks the pointer, Shift is added, as it is for selection.
- **A plain click on a link** selects, as a click anywhere does.
- **With no pointer**, link hints copy a link from the keyboard (section 7).

  > Edited 2026-11-14: the maintainer decided that link hints are built (`considered`). The bullet read: "**With no pointer**, links do nothing. A keyboard mode that labels each visible link for a key to pick, as foot's `show-urls-copy` and kitty's hints do, goes to its own issue, opened when the operator asks for it or a console without a pointer needs links."

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| Open the link with a configured command, such as `xdg-open` or a console browser | the command would run as root, from root's process, unless vtium switched it to the session's user; its argument is pty input; a console has no opener, and a console browser needs a VT or the session's pty to draw on | kitty, foot, Ghostty, WezTerm, VTE's applications | Rejected: an action no console offers, with a security boundary vtium would have to build |
| Copy on Ctrl+click | one modifier, as Ghostty arms a link | | Taken |
| Copy on a plain click | a click meant to clear the selection replaces the buffer | kitty and WezTerm open on a plain click | Rejected |
| Type the URI into the program on a click | text from the stream reaches the program's input with no paste by the user | | Rejected: it breaks the reason behind "No reflection" |
| Keep links for hover only | the user retypes the URI | | Rejected |

### 4. Drawing

A link draws nothing until the pointer is over it, so a program's output looks as the program drew it. While hovered, every visible cell of the link takes a dashed underline in place of its own underline style, in the cell's underline color: SGR 58's color when set, otherwise the foreground, as the frame resolves it already. OSC 8 carries no color, so the hover adds none and no configuration key.

While Ctrl is held over a link, the renderer draws an overlay on the bottom row, or on the top row when the pointer is on the bottom one: the URI from its first byte in the default colors inverted, cut at the row's end with `…` when longer. The overlay covers only the cells the address takes, from the row's first column; the rest of the row shows what the program drew there (the maintainer, 2026-11-01). Under the cells the overlay covers, the text cursor and the selection are not drawn, and the pointer is drawn above the overlay. The overlay is no part of the screen: no cell, selection, copy or reply sees it.

> Edited 2026-11-01: the maintainer's answer in the steering session's interview (`considered`, 2026-11-01): the overlay covers only the address's own cells. The paragraph said the overlay was drawn "over the bottom row" and did not say how much of the row it covers, nor what the cursor, the selection and the pointer do under it; the earlier text is in git at 292d0b5.

```text
 drwxr-xr-x  2 aaron aaron 4096 Oct 27 09:12 docs
 -rw-r--r--  1 aaron aaron  913 Oct 27 09:12 README.md      <- the pointer, Ctrl held
                                              ‾ ‾ ‾ ‾ ‾ ‾ ‾ ‾ ‾
 file://pixie/home/aaron/Projects/vtium/README.md            <- the overlay, inverted
```

A hidden pointer hovers nothing. The pointer hides when its hide timer, `pointer-hide-timeout`, runs out, and at a key that reaches the program when `pointer-hide-while-typing` is on. A hidden pointer is over no cell, so the link it was over loses its dashed underline and the overlay goes, even while Ctrl is held (`crates/vtium/src/head.rs`, `Head::hover`). The next movement shows the pointer again, and it hovers the link under it from then on. A console that is switched away hovers nothing either: vtium clears its hover as it switches, and the shown console's hover is set again once its grid is laid out.

> Edited 2026-11-01: the steering session's brief for the second wave of records, checked against the code (`crates/vtium/src/head.rs`, `Head::hover`, `Head::hide_pointer` and `Head::swap_shown`; `crates/vtium/tests/boot.rs`, `typing_that_hides_the_pointer_ends_the_hover`): a hidden pointer hovers nothing. The section did not say what a hidden pointer or a hidden console hovers; the earlier text is in git at 725c369.

A hover change, the pointer entering, leaving or crossing between links or a link arriving under a still pointer, marks the visible rows whose link tables hold the old or the new link; finding them reads one small table per visible row. A move within one link marks nothing. The overlay marks its row as it appears, moves or goes. A hover over one `ls` row redraws that row and the overlay's, where a selection change marks the whole screen.

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| Underline on hover | links cannot be found without the pointer | kitty's default | Taken |
| Underline always, the hovered one differently | changes how every program's output looks | VTE; foot's and kitty's `always` | Rejected as the default |
| Underline only while Ctrl is held | links cannot be found without holding Ctrl | Ghostty | Rejected |
| A configured link color | a key and a palette entry | kitty's `url_color` | Rejected: the underline color exists |
| The URI shown on hover, always | the overlay covers a row whenever the pointer rests on a link | Ghostty's previews | Rejected |
| The URI never shown | the user copies an address unseen | kitty's default | Rejected: the specification asks to show it first |

### 5. Prompt marks

| Sequence | What vtium does |
|---|---|
| `OSC 133 ; A` | A fresh line, then the cursor's line takes the prompt flag |
| `OSC 133 ; N` | As A: the end of the last command and a new prompt |
| `OSC 133 ; L` | A fresh line |
| `OSC 133 ; P` | Nothing: a prompt start with no fresh line, for continuation, secondary and right prompts, which are no jump targets |
| `OSC 133 ; B`, `OSC 133 ; I` | The cursor's line takes the input flag |
| `OSC 133 ; C` | The cursor's line takes the output flag |
| `OSC 133 ; D` | Nothing: the next prompt ends the output, and the exit status is not kept |

A fresh line is a carriage return and a line feed when the cursor is not in the left margin's column, and nothing when it is, as the proposal defines it and Ghostty and WezTerm do; kitty and foot set the mark where the cursor is. The `aid`, `k` and `cl` options are ignored. `cl` asks the terminal to move the cursor to a click by sending the shell arrow keys; vtium sends nothing on the stream's say.

The marks are the three line flags ADR-1001 holds, and cost no byte. They travel with their line into the scrollback and leave with it; a resize that drops lines drops their marks. Reflow gives a rejoined line the marks of all its rows, on its first row: A and N begin a line with their fresh line, and C comes after the command's line feed, so the marks that matter begin logical lines. ED 2, ED 3 for the scrollback, and RIS clear the marks of the lines they erase; EL and partial erases keep them, since shells redraw a prompt with them.

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| Flags per line | nothing beyond ADR-1001's 3 bits | kitty, foot, Ghostty's rows | Taken |
| A semantic type per cell, prompt, input or output | 2 of the content word's 8 spare bits, so no byte, but a pen state and a stamp on every printed cell | Ghostty, WezTerm | Rejected: the marks that matter begin lines, so a line holds them |
| No fresh line on A | a prompt after output with no final newline is marked mid-line | kitty, foot | Rejected for the proposal |

### 6. What the marks are for

- **A prompt jump.** Ctrl+Shift+Up moves the view so the nearest prompt-flagged line above the view's top row is the view's top row; Ctrl+Shift+Down moves to the nearest below it, and past the newest prompt returns the view to the live screen. A jump moves the view as Shift+Page Up does, and marks everything as moving the view does.
- **A command's output.** Ctrl+right-click on a line of output selects the lines from the output-flagged line above it to the line before the next prompt-flagged line, or to the last written line, as a line selection, and copies it into the selection buffer. When a program tracks the pointer, Shift is added.
- **On the alternate screen** the keys and the click go to the program, which has no scrollback of prompts; full-screen programs that use Ctrl+Shift+Up and Down run there.

The keys join the terminal's grabs (`crates/vtium/src/grab.rs`), where Shift+Up and Down scroll by a line and Shift+Page Up and Down by a page, so the prompt jumps sit beside them as Ghostty places its own.

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| Ctrl+Shift+Up and Down, on the main screen | a program on the main screen never gets CSI 1;6A or 1;6B | Ghostty | Taken |
| Ctrl+Shift+Z and X | apart from vtium's scroll keys; taken from a program that reads them through modifyOtherKeys or the kitty keyboard protocol | kitty, foot | Rejected |
| No key, a configured action | nothing to find | WezTerm | Rejected: the issue's done condition is a jump |
| Show or pipe a command's output to another program | starts a program | kitty's pager, foot's `pipe-command-output` | Rejected, as in section 3 |
| Copy a command's output by a key | no pointer needed; which command the key means needs a rule | kitty's `show_last_command_output` | Left open: link hints (section 7) copy links only |

> Edited 2026-11-14: with link hints decided (section 7), the last option's verdict read "Left for the keyboard issue in section 3".

### 7. Keyboard link hints

> Added 2026-11-14, proposed: this section is new and waits for the maintainer's reading. The maintainer decided that keyboard link hints are built in this wave (`considered`). The section decides how they work.

> Edited 2026-11-15: the maintainer decided where the hint line goes and that a label shifts right to clear the label before it (`considered`). The bullets "Labels that would overlap" and "Where the hint line goes" are new. What happens at the row's end, and in a row too short for its labels, follows from that rule and waits for the maintainer's reading.

Link hints let you copy a link with the keyboard alone, on a console with no mouse or with one. You press a key, vtium puts a short label of letters on each link on the screen, and you type the label of the link you want. vtium then copies that link's address, as Ctrl+click does.

- **The key.** `key-link-hints` starts link hints, and its default is `<Ctrl><Shift>O`. Like every key binding, an empty value hands the key back to the program. The key works on the main screen and on the alternate screen, since full-screen programs such as pagers and mail readers print links too. It acts on the console shown on the head that the keyboard types into.
- **A screen with no links.** When no link is visible, vtium starts no hints. It shows "No links are on the screen." on the overlay row of section 4 until the next key press or pointer movement, and that key press reaches the program as usual.
- **Which links get a label.** Each distinct link visible in the view gets one label, a link being one entry of the link store. vtium orders the links by the cell where each first appears, row by row from the top, and in each row from left to right as the row is drawn. Only OSC 8 links get labels. Text that merely looks like an address does not.
- **How labels are chosen.** Labels are made from the 26 lowercase letters, in this order: the home row `a s d f g h j k l`, then `q w e r t y u i o p`, then `z x c v b n m`. Every label has the same number of letters: the fewest that give each visible link its own label. That is one letter for up to 26 links, two letters for up to 676, and so on. The first link takes the first label, the second link the second, counting through the letters in the order above. Because all labels are the same length, no label begins another, so the last letter you type picks the link and you never press Enter. The same screen always gets the same labels.
- **How labels are drawn.** Each label covers the first cells of its link where the link first appears, one letter to a character cell. Labels draw in the default colors inverted, as the overlay row draws an address. While hints are on, every visible cell of a labelled link also takes the dashed underline that the pointer's hover gives it (section 4). The overlay row shows "Type a link's label to copy its address. Press Escape to stop." Labels and that row draw with vtium's own notices, above every program's text and pictures (ADR-1100 D4). Under a label the text cursor and the selection are not drawn. Labels are no part of the screen: no cell, selection, copy or reply sees them.
- **Labels that would overlap.** A label that would cover part of the label before it on the row shifts right just far enough to clear it, so both read in full. When that pushes labels past the row's end, the last label ends at the row's last column. Each label before it then moves left just far enough to clear the one after it. A row whose labels together are longer than the row cannot hold them all. There, each label that would pass the row's end ends at its last column instead, and a later label draws over an earlier one.
- **Where the hint line goes.** The line that tells how to pick a label stays on the bottom row. It moves to the top row only while a label lies on the bottom row, so it never hides a label there. The pointer does not move it. A label on the top row can lie under the line while both rows hold labels.
- **Typing a label.** While hints are on, vtium takes every key press from the head's keyboards, and none reaches the program. A letter that continues at least one label is kept, and the labels it does not continue disappear. A letter that continues no label is ignored. Backspace takes back the last letter kept. When the letters kept spell a whole label, vtium copies that link's address into the selection buffer, replacing what it held. It also clears the selection on the screen, as Ctrl+click does. The middle button and Shift+Insert then paste it. Hints end, and the overlay row shows "Copied: " followed by the address until the next key press or pointer movement, so you see what you copied. Nothing is opened, run or sent to the program.
- **How hints end without a copy.** Escape ends them, and so does `key-link-hints` again. Any other key that is not a letter or Backspace ends them too, and vtium drops that key, so a key meant for the hints never types into the program. Hints also end at a pointer click, a scroll of the view, or a switch to another console or VT. They end when the grid's size, zoom or rotation changes, and when the console's session ends.
- **Output while hints are on.** Hints stay on while the program writes. vtium fixes the labels when hints start, so a label always means the same link. Each label follows its link to where the link now first appears. A link that leaves the view loses its label, and a link that arrives gets none. The labelled links count as named when the link store sweeps (section 1), as the hovered link does, so the store never frees a link while its label shows.
- **Several displays.** A head shows one console on all its displays, so each display of the head draws the same labels.

In this example, `ls --hyperlink` printed three links. The labels `a`, `s` and `d`, drawn inverted, each cover the first cell of a name, and the overlay row is the screen's last row.

```text
 drwxr-xr-x  2 aaron aaron 4096 Oct 27 09:12 aocs
 -rw-r--r--  1 aaron aaron  913 Oct 27 09:12 sEADME.md
 -rw-r--r--  1 aaron aaron 1270 Oct 27 09:12 dargo.toml

 Type a link's label to copy its address. Press Escape to stop.
```

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| Labels of letters, typed to copy the link | a key taken from programs while it is bound | foot's URL mode with `show-urls-copy`, kitty's hints | Taken |
| Labels of equal length, so the last letter picks the link | three letters on a screen with more than 676 links | | Taken |
| Labels of different lengths, shortest for the first links | a label that begins another needs Enter to pick the shorter one | | Rejected |
| Number the links, typed and then Enter | one more key for every copy, and numbers sit far from the home row | | Rejected |
| Move a highlight from link to link with the arrow keys | many presses on a screen full of links | | Rejected |
| Label text that looks like an address, found by a pattern | a pattern that finds addresses where a program printed none, and a second way for a link to come about | foot, kitty | Not taken in this section: OSC 8 links are the links vtium keeps |
| `<Ctrl><Shift>O` | a program that reads Ctrl+Shift+O, through the kitty keyboard protocol, loses it while the key is bound | foot binds it to its URL mode | Taken |

### Where each part lands

| Part | Crate |
|---|---|
| OSC 8 and OSC 133 parsed and checked, the pen's link, the store, its sweep and strip, the line flags, the fresh line | `vtium-term`, with `LINKS_BYTES_MAX` in its bounds table |
| The hovered link and the overlay's text in the frame; the rows a hover change marks | `vtium-term` |
| The dashed hover underline and the overlay row | `vtium-render` |
| The hover from the pointer, Ctrl+click and Ctrl+right-click, the prompt jump grabs | `vtium` |
| Link hints: the key, the labels, the letters typed and the copy | `vtium` |
| The labels drawn over their links, and the hint row | `vtium-render` |
| The labelled links counted as named when the store sweeps | `vtium-term` |

> Edited 2026-11-14: the three rows for link hints are new (section 7).

## Consequences

### Positive

- Links cost no byte in a cell or line that has none, and the store's bound keeps the worst default scrollback under the minspec peak.
- Nothing from the stream starts a program, and nothing it writes reaches a reply or the program's input without the user's paste.
- The URI the overlay shows is the URI the copy carries, byte for byte.
- A program that sends the same links on every redraw adds nothing to the store.
- Prompt jumps and output copies cost no storage beyond ADR-1001's flags.

### Negative

- A link cannot be followed from the console; the user pastes its address into a program that can.
- Links are invisible until hovered or labelled.
- Ctrl+Shift+O never reaches a program while `key-link-hints` binds it.

> Edited 2026-11-14: with link hints (section 7), the first bullet above read "Links are invisible until hovered, and unusable without a pointer until the keyboard mode is built.", and the bullet on Ctrl+Shift+O is new.
- Ctrl+Shift+Up and Down never reach a program on the main screen.
- A long session with more than about 2,300 distinct links in the scrollback loses the oldest ones' links.
- A sweep is linear in the lines held, about 100,000 table reads at the largest cap, once per 64 KiB of new links.
- The fresh line on OSC 133 A moves the cursor where kitty and foot leave it.

### Neutral

- The replay interface's screen view shows neither links nor line flags, so tier 1 cannot observe them; vtium's own replay cases and unit tests judge them, and the probes `osc-8-hyperlink` and `osc-133-marks`, placeholders in the frozen fork, are written in vtium's tree.
- The corpus case `osc-sequences` sends an OSC 8 link; its text and attributes stay the golden's.
- Reflow carries the marks rule of section 5.
- vtium-render draws the dashed underline it has already; the overlay is its first text drawn from outside the screen.

## Alternatives Considered

Each decision above lists its options with their costs and verdicts. The direction set aside whole is opening links: every desktop terminal surveyed does it, and on a console it needs a launcher that runs as the session's user, an allowlist of schemes and a target to draw on. A future decision can add it on top of this one, which leaves the store, the bounds and the overlay as they are.
