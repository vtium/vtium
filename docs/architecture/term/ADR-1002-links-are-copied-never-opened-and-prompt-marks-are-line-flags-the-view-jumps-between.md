---
contract: adr/v1
kind: decision
verb: add
capability: screen
basis:
  - operator: aaronsb
    level: guided
    said: "The record decides the link table and its bound, what opening a link means on a VT with no browser, how marks survive scrollback and reflow, and the key bindings that jump between prompts."
    via: "vtium/vtium#40, the issue body, filed under the operator's account in a session where agents drafted issues, so possibly agent-written"
  - operator: aaronsb
    level: guided
    said: "Done: the probes pass, and a prompt jump lands on each marked prompt in the scrollback."
    via: "vtium/vtium#40, the issue body, as above"
  - precedent: ADR-1001
  - precedent: ADR-1000
  - precedent: ADR-1400
  - upstream: "The C reference dropped both: libtsm kept at most 127 bytes of an OSC string and handed an OSC it did not take to the host (`src/tsm/tsm-vte-int.h:31`, `src/tsm/tsm-vte.c:2912-2921, 3004-3015` at aaronsb/libtsm eeaf293), and kmscon's host took only `setBackground` and `setForeground` (`src/terminal.c:859-870` at aaronsb/kmscon b1f7a7c5); its probes `osc-8-hyperlink` and `osc-133-marks` are placeholders (`tests/capabilities/probes.replay:435-442`)"
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

- **Decided:** OSC 8 links are kept per cell through ADR-1001's link index, in one link store bounded at 256 KiB that holds each distinct link once. A link is underlined while the pointer is over it, shows its URI on an overlay row while Ctrl is held, and Ctrl+click copies the URI into the selection buffer the middle button and Shift+Insert paste. Nothing is ever opened or run. OSC 133 marks set ADR-1001's three line flags; Ctrl+Shift+Up and Down move the view between marked prompts, and Ctrl+right-click copies the output of the command under the pointer.
- **Trades away:** No link opens a browser or any command. A link draws nothing until it is hovered, and a console with no pointer cannot use links. On the main screen, Ctrl+Shift+Up and Down never reach a program. Past 256 KiB of distinct links, the oldest scrollback lines lose theirs.
- **One-way?** No. Reversible: the cell's link index and the line flags exist already, and nothing outlives the process.
- **Probes:**
  - *Confident (nothing opens):* a link on the console never starts a browser or any other program; Ctrl+click puts its address in the paste buffer, and you paste it where you choose, such as after `w3m` at a shell prompt.
  - *Not confident (no mouse):* on a console with no mouse, links do nothing until a keyboard way to pick one is built, as foot labels every link on the screen with a letter to type; that is left to its own issue.
- **Inversion:** Between "the console as a launcher", where kitty, foot, Ghostty and WezTerm open a link through the desktop's opener, and "links parsed and dropped", as the C reference had them. The answer sits between: links kept, shown and copied, with the launch left out.

## Context

OSC 8 attaches a URI to the cells printed while it is open; OSC 133 marks where a shell's prompt, the command typed and its output begin. ADR-1001 placed both in the cell model: each cell carries a 16-bit index into its line's link table of 4-byte `LinkId`s, which name entries in "the link store vtium/vtium#40 builds", and each line carries prompt, input and output flags. This record decides the store, what a link may hold and does, how both are drawn, and what the marks are for.

The C reference dropped both sequences: libtsm kept 127 bytes of an OSC string and handed OSC 8 and OSC 133 to kmscon, which took only its own two strings. Its probes for both are placeholders. ADR-1000 lists `osc.8` and `osc.133` as supported, at the `top` layer.

The console shapes the choices. vtium runs on a VT with no desktop, no browser and no opener such as `xdg-open`; its main process holds root for life (ADR-1400). It has a pointer, keyboard grabs, and a selection buffer that the user's copies and OSC 52 fill and the middle button and Shift+Insert paste (vtium/vtium#91, #39). At minspec, ADR-1001's worst default scrollback leaves 0.34 MiB under the 15.7 MiB peak.

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

A `file://` URI of 50 bytes is charged 114, so the store holds about 2,300 such links. At the default 1,000 lines, ADR-1001's worst case with a full store comes to about 15.61 MiB, 0.09 MiB under the 15.7 MiB peak; at the 10,000-line cap the store adds its 0.25 MiB to 63.2 MiB.

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
- **With no pointer**, links do nothing. A keyboard mode that labels each visible link for a key to pick, as foot's `show-urls-copy` and kitty's hints do, goes to its own issue, opened when the operator asks for it or a console without a pointer needs links.

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| Open the link with a configured command, such as `xdg-open` or a console browser | the command would run as root, from root's process, unless vtium switched it to the session's user; its argument is pty input; a console has no opener, and a console browser needs a VT or the session's pty to draw on | kitty, foot, Ghostty, WezTerm, VTE's applications | Rejected: an action no console offers, with a security boundary vtium would have to build |
| Copy on Ctrl+click | one modifier, as Ghostty arms a link | | Taken |
| Copy on a plain click | a click meant to clear the selection replaces the buffer | kitty and WezTerm open on a plain click | Rejected |
| Type the URI into the program on a click | text from the stream reaches the program's input with no paste by the user | | Rejected: it breaks the reason behind "No reflection" |
| Keep links for hover only | the user retypes the URI | | Rejected |

### 4. Drawing

A link draws nothing until the pointer is over it, so a program's output looks as the program drew it. While hovered, every visible cell of the link takes a dashed underline in place of its own underline style, in the cell's underline colour: SGR 58's colour when set, otherwise the foreground, as the frame resolves it already. OSC 8 carries no colour, so the hover adds none and no configuration key.

While Ctrl is held over a link, the renderer draws an overlay over the bottom row, or over the top row when the pointer is on the bottom one: the URI from its first byte in the default colours inverted, cut at the row's end with `…` when longer. The overlay is no part of the screen: no cell, selection, copy or reply sees it.

```text
 drwxr-xr-x  2 aaron aaron 4096 Oct 27 09:12 docs
 -rw-r--r--  1 aaron aaron  913 Oct 27 09:12 README.md      <- the pointer, Ctrl held
                                              ‾ ‾ ‾ ‾ ‾ ‾ ‾ ‾ ‾
 file://pixie/home/aaron/Projects/vtium/README.md            <- the overlay, inverted
```

A hover change, the pointer entering, leaving or crossing between links or a link arriving under a still pointer, marks the visible rows whose link tables hold the old or the new link; finding them reads one small table per visible row. A move within one link marks nothing. The overlay marks its row as it appears, moves or goes. A hover over one `ls` row redraws that row and the overlay's, where a selection change marks the whole screen.

| Option | Cost | Who does it | Verdict |
|---|---|---|---|
| Underline on hover | links cannot be found without the pointer | kitty's default | Taken |
| Underline always, the hovered one differently | changes how every program's output looks | VTE; foot's and kitty's `always` | Rejected as the default |
| Underline only while Ctrl is held | links cannot be found without holding Ctrl | Ghostty | Rejected |
| A configured link colour | a key and a palette entry | kitty's `url_color` | Rejected: the underline colour exists |
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

The marks are the three line flags ADR-1001 holds, and cost no byte. They travel with their line into the scrollback and leave with it; a resize that drops lines drops their marks. Reflow (vtium/vtium#41) gives a rejoined line the marks of all its rows, on its first row: A and N begin a line with their fresh line, and C comes after the command's line feed, so the marks that matter begin logical lines. ED 2, ED 3 for the scrollback, and RIS clear the marks of the lines they erase; EL and partial erases keep them, since shells redraw a prompt with them.

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
| Copy a command's output by a key | no pointer needed; which command the key means needs a rule | kitty's `show_last_command_output` | Left for the keyboard issue in section 3 |

### Where each part lands

| Part | Crate |
|---|---|
| OSC 8 and OSC 133 parsed and checked, the pen's link, the store, its sweep and strip, the line flags, the fresh line | `vtium-term`, with `LINKS_BYTES_MAX` in its bounds table |
| The hovered link and the overlay's text in the frame; the rows a hover change marks | `vtium-term` |
| The dashed hover underline and the overlay row | `vtium-render` |
| The hover from the pointer, Ctrl+click and Ctrl+right-click, the prompt jump grabs | `vtium` |

## Consequences

### Positive

- Links cost no byte in a cell or line that has none, and the store's bound keeps the worst default scrollback under the minspec peak.
- Nothing from the stream starts a program, and nothing it writes reaches a reply or the program's input without the user's paste.
- The URI the overlay shows is the URI the copy carries, byte for byte.
- A program that sends the same links on every redraw adds nothing to the store.
- Prompt jumps and output copies cost no storage beyond ADR-1001's flags.

### Negative

- A link cannot be followed from the console; the user pastes its address into a program that can.
- Links are invisible until hovered, and unusable without a pointer until the keyboard mode is built.
- Ctrl+Shift+Up and Down never reach a program on the main screen.
- A long session with more than about 2,300 distinct links in the scrollback loses the oldest ones' links.
- A sweep is linear in the lines held, about 100,000 table reads at the largest cap, once per 64 KiB of new links.
- The fresh line on OSC 133 A moves the cursor where kitty and foot leave it.

### Neutral

- The replay interface's screen view shows neither links nor line flags, so tier 1 cannot observe them; vtium's own replay cases and unit tests judge them, and the probes `osc-8-hyperlink` and `osc-133-marks`, placeholders in the frozen fork, are written in vtium's tree.
- The corpus case `osc-sequences` sends an OSC 8 link; its text and attributes stay the golden's.
- Reflow (vtium/vtium#41) carries the marks rule of section 5.
- vtium-render draws the dashed underline it has already; the overlay is its first text drawn from outside the screen.

## Alternatives Considered

Each decision above lists its options with their costs and verdicts. The direction set aside whole is opening links: every desktop terminal surveyed does it, and on a console it needs a launcher that runs as the session's user, an allowlist of schemes and a target to draw on. A future decision can add it on top of this one, which leaves the store, the bounds and the overlay as they are.
