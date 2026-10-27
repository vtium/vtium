---
contract: adr/v1
kind: decision
verb: add
capability: protocols
basis:
  - operator: aaronsb
    level: directed
    said: "in general, vtium should cover what other terminals support"
    via: vtium/vtium#12, comment of 2026-10-26 listing the features the operator would use
  - operator: aaronsb
    level: directed
    said: "Title reporting answers with a fixed empty title, and an unknown DECRQSS gets the error form with no echo."
    via: vtium/vtium#12, comment of 2026-10-26 adding the reply-rule gate
  - operator: aaronsb
    level: directed
    paraphrase: true
    said: "Refuse KAM, DECCOLM and DECNRCM as the C reference did; hold DECANM as a reserved row with an issue for VT52; refuse hilite tracking, DECUDK, OSC 46, kitty file transfer, kitty remote control and window manipulation; answer XTWINOPS reports only where vtium has the function, with real values; title reports empty, OSC 52 read off and write on; strip the paste end marker; ENQ answers empty; focus and visibility reports allowed; setBackground and setForeground behind a configuration key, on by default; bidi modes wait for #43; OSC 21 takes kitty's meaning; the new-mode rule stands; LNM is kept."
    via: Claude Code session, the operator's review of the inventory draft, relayed by the team lead, 2026-10-26
  - standard: xterm control sequences (XTerm patch 411), https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
  - standard: ECMA-48 5th edition (1991), section 7.2 (modes), 8.3.117 (SGR), annex F
  - standard: VT510 reference manual, DECRQM tables 5-7 and 5-8, https://vt100.net/docs/vt510-rm/DECRQM.html
  - standard: kitty protocol extensions, specification pages only, https://sw.kovidgoyal.net/kitty/protocol-extensions/
  - standard: contour VT extensions, https://contour-terminal.org/vt-extensions/
  - upstream: foot-ctlseqs(7); Ghostty's VT reference, external protocols and configuration reference; WezTerm's escape sequences, features and key encoding pages
  - upstream: the C reference's tables - libtsm src/tsm/tsm-modes.def, kmscon tests/capabilities/capabilities.list and COVERAGE.md
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "agree to widen the rule"
    via: operator, in session 2026-10-27
    covers: [where the line falls]
  - operator: aaronsb
    said: "the union of our tables and the operator's (my) listings. we're not trying to build a 'super terminal' of all the different references."
    via: operator, in session 2026-10-27, relayed by the team lead
    covers: [where the line falls]
status: proposed
date: 2026-10-26
deciders:
  - aaronsb
related: []
---

# ADR-1000: The modes and features vtium supports

## Summary

- **Decided:** vtium's target set is two tables beside this record: every ANSI and DEC private mode, and every capability beyond modes, that the C reference, xterm, ECMA-48, the DEC references, foot, Ghostty, WezTerm, contour or kitty's specifications define. Each row gives what the C reference did, which terminals document it, vtium's status, its layer and the issue that carries it. A new mode or feature is supported when xterm and one of foot, Ghostty, WezTerm or kitty document it, when it is on the operator's list, or when the console has a named use for it. The record also fixes the refusals and the reply policy.
- **Trades away:** A mode that falls short of xterm and one other terminal gets no row unless it is on the operator's list or its row names a console use, so DECRQM replies 0 for it, as the C reference did. Features that put clipboard data, host files or terminal control in the stream's reach are refused even where another terminal offers them behind a setting.
- **One-way?** No. Each row changes in one edit; a row that turns supported changes what DECRQM and the capability probes report, which is the intent.
- **Probes:** *Confident (honest replies):* a program that asks vtium what it supports gets an answer that matches what vtium does, and never a reply carrying text from the stream. *Not confident (where the line falls):* that "xterm plus one other terminal" draws the line where the operator wants it for keyboard modes such as 1034 and 1035, which come in under it.
- **Inversion:** Between "the C reference's set, no more" and "every sequence any terminal accepts". The answer sits between: the C reference is the floor, the documented common set is the target, and anything that lets output read from or steer the host is refused whoever offers it.

## Context

The C reference recognised 34 modes in one declarative table (libtsm's `tsm-modes.def`) and judged its capabilities through a probe list. vtium carries both forward, and its work items need a declared target before the code: which modes and features vtium supports, which it ignores or refuses, and which issue carries each one. Without a declared set, each work item decides scope alone, and a capability can be dropped by omission.

## Decision

### The target set

The tables are `ADR-1000-modes.tsv` (128 rows: 40 supported, 4 ignored, 4 refused, 80 with no row) and `ADR-1000-features.tsv` (162 rows: 109 supported, 43 ignored, 7 refused, 2 parked, 1 weighed in another issue). Their status vocabulary is the C reference's, with two statuses the features table adds:

- **supported**: set and reset, or the feature, do what the specification says.
- **partial**: part of it, with the missing part's issue.
- **ignored**: accepted with no effect; the note says why or names the issue for the effect.
- **refused**: deliberately not built, with the reason.
- **no row** (modes): vtium does not recognise the number; DECRQM replies 0 and SM and RM change nothing.
- **parked** (features): its own issue designs it before anything is built: input methods (#45) and Braille displays (#46).
- **weighed** (features): another issue decides whether vtium builds it, and the row takes that decision: baked glyph atlases (#3), weighed in #23.

An ignored or refused mode answers DECRQM with its fixed default, as the C reference did: 3 (permanently set) for DECANM, and 4 (permanently reset) for the rest, KAM, DECCOLM, DECNRCM and 1001 among them.

Each row names the layer it lands in, from the tracker (vtium/vtium#47): minspec, text, images, gpu, seat or top.

vtium supports everything the C reference supported, apart from the departures vtium lists (#14), and goes beyond it.

### The rule for a new mode or feature

A mode or feature the C reference had keeps its status unless this record changes it. Beyond those, the target set is the union of two parts: the capabilities terminals generally support, which this rule admits, and the operator's list, the features the operator named as ones they use (vtium/vtium#12, the comment of 2026-10-26). The rule admits a new one when xterm's control sequences and at least one of foot, Ghostty, WezTerm or kitty document it, or when the console has a use for it that its row names (mode 2028 for reflow, mode 2033 for VT switching, mode 2048 for a display hot-plugged or changing mode). vtium is not a superset of every reference terminal, and obscure DEC-era modes are not the aim: a new mode neither part brings in gets no row, and a new feature neither part brings in is ignored. A refusal carries a reason a reader can check.

The support columns count only published documentation: for xterm and foot, whose documents list everything they do, absence reads "no"; elsewhere silence reads "unverified". kitty is read from its specification pages, never its source.

### Refusals

- **KAM (ANSI 2):** a program could lock the keyboard of the only terminal on the console.
- **DECCOLM (DEC 3):** the console's size comes from the display and the font.
- **DECNRCM (DEC 42):** national replacement sets are obsolete next to UTF-8.
- **Hilite mouse tracking (DEC 1001) and XTHIMOUSE:** the terminal waits on the program's answer before it can show a selection.
- **XTWINOPS window manipulation (1-7, 9, 10 and 24 and above):** vtium has no window to move, raise, iconify or resize. A program resizing the grid in characters (CSI 8 t) is held apart, for #44.
- **DECUDK:** program output would program a key that later types into the shell.
- **OSC 46:** it writes a log file on the host.
- **kitty file transfer (OSC 5113):** it reads and writes host files at the stream's request.
- **kitty remote control (DCS @kitty-cmd):** program output would drive the terminal.
- **OSC 52 read:** the reply would hand clipboard text to whatever reads the pty.

### Declined for now

VT52 (DECANM, DEC 2) has a reserved row, ignored: DECRQM reports the ANSI state (DECANM set) as permanent and a reset does nothing. vtium/vtium#65 holds the space for full VT52 support in a later layer.

### Replies and policy

Every reply follows the reply rule (docs/architecture.md, "Determinism"): it is built from vtium's state, fixed strings and re-encoded numbers, selectors and hex, never from request bytes.

- **A report answers only where vtium has the thing it reports, with its real value.** The text area in cells (XTWINOPS 18) and in pixels (14), the cell size (16), and the screen in cells (19) and pixels (15), the screen being the display the grid fills, get real numbers. Reports about a window or icon vtium does not have (11, 13, 14;2, 20) get no reply.
- **Title reporting (XTWINOPS 21) answers a fixed empty title.** OSC 0, 1 and 2 are ignored, so no title is kept.
- **OSC 52 write is on**, inside the OSC string bound (#39); the read is refused above.
- **A paste never carries its own end marker:** vtium strips CSI 201 ~ from pasted text inside bracketed paste.
- **ENQ answers an empty answerback.** The C reference answered ACK (0x06); vtium lists this as a departure.
- **Focus (1004) and visibility (2033) reports are allowed:** a program opts in, and what they reveal is that the user switched VTs.
- **setBackground and setForeground**, kmscon's private OSC strings (`kmscon/src/terminal.c:863` in the C reference), hand the display back and take it again. No DEC or ECMA specification defines them. vtium keeps them behind a configuration key, on by default, as the C reference behaved (#36).
- **OSC 21** takes kitty's colour-protocol meaning, which Ghostty shares, not xterm's DECSWT title. A query for a key vtium does not know is left out of the reply. kitty answers it with `unknown=` and the key's name in base64, which carries request bytes, so vtium lists this as a departure from kitty.
- **Bidirectional modes** (BDSM, DCSM, DECRLM, DECRLCM) have no row until #43 decides vtium's bidi model.
- **LNM (ANSI 20) stays supported**, as xterm and the C reference have it, though ECMA-48's 5th edition withdraws it.

### The tables as data

The TSVs are the source for the target set until vtium-term's mode table (#12) and the capability list (#14) exist. From then those become the source: the TSVs are generated from them, and a test fails when the committed TSVs differ, as docs/architecture.md's "Tables, references and maps" requires of every fact stated twice. A row's status changes in its source, in the same change as the work that changes it.

The mode table's columns: kind, number, name, spec, c_reference, xterm, foot, ghostty, wezterm, kitty, status, note, layer, issue. The feature table's: group, feature, spec, c_reference, the five terminals, status, note, layer, issue. A `c_reference` cell opens with a tsm-modes.def status in capitals; works, missing or partial, naming the probes behind the verdict where there are any; parsed, for a sequence the C reference read and did nothing with; audited without a probe; unverified; or absent. What follows says what the C reference did.

Each row proves its coverage by the contract of vtium/vtium#67, whose columns and lint land with that issue:

- Every row has a stable `id`: `ANSI-4` and `DEC-2026` for modes, and a dotted name such as `sgr.dim` for features.
- A test that covers a row carries `// covers: <id>`, and a replay file carries the same tag in a comment.
- A `tests` column lists the tests that cite the row. Only the lint writes it.
- `status` records the decision. Coverage is derived from the tags.
- Ignored and refused rows are covered too, by a test that the mode or feature stays ignored or refused.
- The minspec slice is done when every minspec row other than `no row` has a test.

## Consequences

### Positive

- Every work item inherits its scope from a row, and a dropped capability shows as a row whose status changed.
- DECRQM, the capability probes and the terminfo entry have one list to agree with.
- The refusals and the reply policy are settled once, so a handler author does not reopen them.

### Negative

- 80 mode rows record "no row"; the table carries them so the decision is visible.
- The Ghostty, WezTerm and kitty columns are mostly "unverified" for standard modes, since their docs do not list them; a later edit can settle them from documentation.
- The ignored and refused rows have to stay ignored and refused through every later handler.

### Neutral

- Eleven issues carry the new rows: vtium/vtium#54 (modes and controls beyond the C reference), #55 (left and right margins), #56 (rectangular areas and protection), #57 (in-band reports), #58 (SGR and colour stacks), #59 (pointer shapes), #60 (kitty text sizing), #61 (double-width and double-height lines), #62 (iTerm2 inline images), #63 (a static image at the login prompt) and #64 (font merging), with #65 for VT52. #37, #38 and #23 are widened for SGR 6 and 8, the pointer, selection and kitty colours, and ligatures as an option.
- The departures list (#14) gains ENQ's empty answerback, DA1's conformance level (one of xterm's in place of the C reference's 60), and, as a departure from kitty, OSC 21's omission of unknown keys.

## Alternatives Considered

- **The C reference's set alone.** Rejected: the operator's direction is that vtium covers what other terminals support, and the C reference is the floor.
- **Every sequence any listed terminal accepts.** Rejected: one-terminal extensions would each carry handler and test cost with no common use, and several put host files, the clipboard or terminal control in the stream's reach.
- **Rows for every recognised number, ignored rather than absent.** Rejected: a fixed DECRQM reply for a mode vtium has no concept of tells a program nothing it can use, and changes replies the C reference's goldens record as 0.
- **Answer every XTWINOPS report with fixed values, as foot does.** Rejected: an invented window position or iconified state is a reply about a function vtium does not have.
