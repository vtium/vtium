---
contract: adr/v1
kind: decision
verb: add
capability: protocols
basis:
  - operator: aaronsb
    level: directed
    said: "in general, vtium should cover what other terminals support"
    via: the GitHub issue for vtium-term's modes, SGR and replies, comment of 2026-10-26 listing the features the operator would use
  - operator: aaronsb
    level: directed
    said: "Title reporting answers with a fixed empty title, and an unknown DECRQSS gets the error form with no echo."
    via: the same issue, comment of 2026-10-26 adding the reply-rule gate
  - operator: aaronsb
    level: directed
    paraphrase: true
    said: "Refuse KAM, DECCOLM and DECNRCM as the C reference did; hold DECANM as a reserved row with an issue for VT52; refuse hilite tracking, DECUDK, OSC 46, kitty file transfer, kitty remote control and window manipulation; answer XTWINOPS reports only where vtium has the function, with real values; title reports empty, OSC 52 read off and write on; strip the paste end marker; ENQ answers empty; focus and visibility reports allowed; setBackground and setForeground behind a configuration key, on by default; bidi modes wait for [the bidirectional text work]; OSC 21 takes kitty's meaning; the new-mode rule stands; LNM is kept."
    via: Claude Code session, the operator's review of the inventory draft, relayed by the team lead, 2026-10-26
  - standard: xterm control sequences (XTerm patch 411), https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
  - standard: ECMA-48 5th edition (1991), section 7.2 (modes), 8.3.117 (SGR), annex F
  - standard: VT510 reference manual, DECRQM tables 5-7 and 5-8, https://vt100.net/docs/vt510-rm/DECRQM.html
  - standard: kitty protocol extensions, specification pages only, https://sw.kovidgoyal.net/kitty/protocol-extensions/
  - standard: contour VT extensions, https://contour-terminal.org/vt-extensions/
  - upstream: foot-ctlseqs(7); Ghostty's VT reference, external protocols and configuration reference; WezTerm's escape sequences, features and key encoding pages
  - upstream: the C reference's tables - libtsm src/tsm/tsm-modes.def, kmscon tests/capabilities/capabilities.list and COVERAGE.md
  - upstream: Ghostty 1.4.0 release notes, for kitty graphics animation, relative placements and Unicode placeholders, OSC 5522 and mode 5522, OSC 72 and OSC 99, https://ghostty.org/docs/install/release-notes/1-4-0
  - operator: aaronsb
    level: directed
    said: 'vtium presents itself as kitty, kmscon, xterm or another terminal wherever a program needs that to keep a capability, so that no program fails on an unknown terminal type; packaging work removes or renames no compatible identity: TERM values, the terminfo entries vtium installs, the kmscon one included, the XTVERSION and device-attribute replies, or protocol identification.'
    via: the steering session, 2026-11-02, relayed by the team lead
    paraphrase: true
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "all the generally supported capabilities, not esoteric 1970s terminal modes"
    via: operator, in session 2026-10-27, relayed by the team lead
    covers: [where the line falls]
  - operator: aaronsb
    said: "Modes 1007, 1034, 1035, 1036 and 1042 start set. DECSTR keeps the modes XTSAVE saved and the XTSHIFTESCAPE setting; RIS clears the saved modes and returns XTSHIFTESCAPE to 0, as Ghostty does, and vtium lists the RIS half as a departure from xterm, which keeps both. XTSAVE and XTRESTORE with an empty list follow kitty's documented rule, every mode free of side effects: they leave out DECOM (6), modes 47, 1047, 1048, 1049 and 2026, and the ANSI modes IRM, SRM and LNM."
    via: the maintainer's answers in the steering session, 2026-11-01
    paraphrase: true
  - operator: aaronsb
    said: "The bell: a BEL from any console rings the VT bell, shown or hidden, as the C reference did. The text.overhang row: Ghostty's column goes back to unverified, since the columns count only published documentation; the source-reading exception goes."
    via: the maintainer's answers in the steering session's interview, 2026-11-01
    paraphrase: true
  - operator: aaronsb
    said: "vtium names itself xterm-vtium by default. vtium establishes its own name: most programs and platforms do not know vtium, so the entry says what vtium can do, which is largely kitty-compatible. The term setting keeps any other name available: --term=kmscon for compatibility, --term=xterm-kitty once kitty graphics land. This machine keeps the kmscon-terminfo package for the kmscon name, and vtium ships its own entry only as xterm-vtium."
    via: the maintainer's decisions in the steering session, 2026-11-01, relayed by the team lead in the brief for the terminal-name work; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "vtium presents itself as a kitty-compatible terminal under its own name. For kitty's protocols it follows kitty's spec, and kitty's own behaviour, checked by black-box runs, where the spec is silent. It answers kitty's detection queries as kitty does. It keeps TERM=xterm-vtium and does not set KITTY_WINDOW_ID, because that variable tells programs this is kitty and invites kitty-only features vtium lacks, such as remote control."
    via: the maintainer, 2026-11-01, in the steering session
  - operator: aaronsb
    said: "agree"
    via: the steering session, 2026-11-07, asking whether ADR-1000's row text.vitrify becomes refused, because ADR-1200 has vtium generate every glyph at run time and build no glyph set ahead of time
  - operator: aaronsb
    said: "After the fast wake-up retries run out, vtium keeps retrying once a second for as long as its VT is the one shown. A key press, a pointer action or a hotplug still tries again at once. A VT release stops the slow retries too, and a VT acquire starts the fast retries again."
    via: the steering session's question on 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "vtium honors the Linux console's cursor blink interval, CSI 16 ; n ], so its row in the features table is planned"
    via: the steering session's question on 2026-11-13, on an outside scenario survey
    paraphrase: true
  - operator: aaronsb
    said: "The configuration key is `allow-display-ops`, on by default, beside allow-font-ops and allow-window-ops."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Only a console a head is showing may send setBackground."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Per port, in steps. Step one: setBackground gives up the whole cards the sending head draws on, so every existing program works; other heads on those cards pause. Step two: a vtium socket, named in each session's environment, hands a DRM lease of only the sending head's connectors (with a CRTC and planes) to a program that asks; vtium keeps its own drawing and its hotplug paths off leased objects, and revokes the lease at setForeground. Later, as a separate experiment: vtium answers as a seat manager (the seatd protocol), so an unmodified libseat program receives a lease."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Under a seat manager (logind or seatd), only the lease path acts. vtium ignores a plain setBackground there, logs that it did, and sends no focus report. The whole-card hand-over happens only on the kernel's VT."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Focus reports go out at the hand-over and also at a VT switch (focus out when vtium's VT is released, in when it is acquired), in the same work."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Gate leases on a prototype, file it as lower priority; we have a functional system now and leases would complicate the work"
    via: the maintainer, in the steering session, 2026-11-13
status: accepted
date: 2026-10-26
deciders:
  - aaronsb
related: [ADR-1503]
---

# ADR-1000: The modes and features vtium supports

## Summary

- **Decided:** vtium's target set is the capabilities terminals generally support, combined with the operator's list. Two tables beside this record list the candidates considered, every ANSI and DEC private mode and every capability beyond modes that the C reference, xterm, ECMA-48, the DEC references, foot, Ghostty, WezTerm, contour or kitty's specifications define, each with its status and the reason for it, so a refused mode is the decision not to support it. Each row gives what the Linux kernel's own console does, which terminals document it, vtium's status, its layer and the issue that carries it. A new mode or feature is supported when xterm and one of foot, Ghostty, WezTerm or kitty document it, when it is on the operator's list, or when the console has a named use for it. The record also fixes the refusals and the reply policy.
- **Trades away:** A mode that falls short of xterm and one other terminal is refused unless it is on the operator's list or its row names a console use, so DECRQM replies 0 for it, as the C reference did. Features that put clipboard data, host files or terminal control in the stream's reach are refused even where another terminal offers them behind a setting.
- **One-way?** No. Each row changes in one edit; a row that turns supported changes what DECRQM and the capability probes report, which is the intent.
- **Probes:** *Confident (honest replies):* a program that asks vtium what it supports gets an answer that matches what vtium does, and never a reply carrying text from the stream. *Not confident (where the line falls):* that "xterm plus one other terminal" draws the line where the operator wants it for keyboard modes such as 1034 and 1035, which come in under it.
- **Inversion:** Between "the C reference's set, no more" and "every sequence any terminal accepts". The answer sits between: the C reference is the floor, the documented common set is the target, and anything that lets output read from or steer the host is refused whoever offers it.

## Context

The C reference recognised 34 modes in one declarative table (libtsm's `tsm-modes.def`) and judged its capabilities through a probe list. vtium carries both forward, and its work items need a declared target before the code: which modes and features vtium supports, which it ignores or refuses, and which issue carries each one. Without a declared set, each work item decides scope alone, and a capability can be dropped by omission.

## Decision

### The target set

The tables are `ADR-1000-modes.tsv` (131 rows: 44 supported, 10 ignored, 75 refused, 2 weighed) and `ADR-1000-features.tsv` (186 rows: 123 supported, 2 partial, 44 ignored, 8 refused, 9 planned). Both tables use the same statuses:

- **supported**: set and reset, or the feature, do what the specification says.
- **partial**: vtium does part of it; the note says what is missing, and the issue column names the work that builds the rest.
- **planned**: vtium will build it, and its issue says how; nothing of it is built yet. Input methods and Braille displays are among them.
- **weighed**: another issue decides whether vtium builds it, and the row takes that decision: foot's input method mode, which the work on input methods decides.
- **ignored**: vtium accepts it and does nothing with it; the reason says why.
- **refused**: vtium deliberately does not build it, for the reason given.

DECRQM, the query by which a program asks whether a mode is set, answers from vtium's mode table in vtium-term. A mode the table lists answers 1 (set) or 2 (reset) when it is supported, and its fixed state when it is ignored or refused: 3 (permanently set) for DECANM, for 1011 and 1046, which vtium always has, and for DCSM, and 4 (permanently reset) for the rest, KAM, DECCOLM, DECNRCM, 40, 1001, 1005 and 1010 among them. A mode whose note says it is "not in vtium's mode table" is one vtium does not recognize: DECRQM answers it 0, and SM and RM change nothing. Every planned and weighed mode is such a mode, and so are the refused modes that carry that note; a planned mode joins the table, and its answer changes, in the change that builds it. vtium-term's test `the_table_agrees_with_adr_1000` holds the two to each other: a mode marked supported, partial, ignored or refused has a row in the table unless its note says it is not in vtium's mode table, and no other mode has one.

> Edited 2026-11-06: the two tables now share one list of statuses, where parked and weighed were the features table's alone. An audit of the open issues found one mode row and seventeen feature rows marked supported that vtium does not implement, so a reader took them for capabilities vtium has. DECLRMM (DEC 69), which nothing implements, is planned, since vtium has no left and right margins yet. Of the feature rows, fourteen, which nothing implements, are planned: kmscon's setBackground and setForeground strings, XTWINOPS 8, OSC 22, kitty's text sizing, double-size lines, the login image, ligatures, the kitty keyboard protocol, bidirectional text, SGR 6, 8, 53 and 55, and 73 to 75, and DECIC with its kin. Three, which vtium implements in part, are partial: the display hand-back, whose wake-up retry is built; font merging, whose second source a frit's `cover=` already serves; and DECSLRM's row, whose SCOSC and SCORC are built. Each row's note says what is missing, and the decision to support each of them stands. The same day the maintainer retired `no row`: each of the 80 modes vtium does not recognize took a status and a reason, 74 refused and 6 weighed, with a note that it is not in vtium's mode table, and no DECRQM answer changed. The maintainer then had five of them answer the state vtium always has: 1011 and 1046 joined vtium's mode table as ignored and permanently set, and 40, 1005 and 1010 as ignored and permanently reset, so DECRQM answers them 3 and 4 where it answered 0. vtium keeps the view where it is while output arrives, so 1010, scroll to the bottom on output, is never on. Before this edit the counts above read 40 supported modes and 80 with no row, and 179 feature rows with 125 supported and 2 parked. The same day, the maintainer renamed `parked` to `planned`, to say that vtium will build each such row and its issue says how.

> Edited 2026-11-06: ADR-1004, the record on right-to-left text, decided the bidirectional modes, and the work that builds its paragraph state set their rows. BDSM, weighed before, is supported; DCSM, weighed, is ignored and answers 3; DECRLM and DECRLCM, weighed, are refused; and three new modes, 1243, 2500 and 2501, are supported. Only a build with the cargo feature `bidi` has the rows of BDSM, DCSM, 1243, 2500 and 2501, as only a build with `images` has the sixel rows, so a build without it answers DECRQM 0 for each; those rows name the new layer `bidi`. The feature table gains SCP and SPD, supported, and bidirectional text, planned before, is partial.

> Edited 2026-11-06: the row for libseat seats, which said vtium runs unprivileged and was marked supported, is planned, since vtium does not build libseat yet. The maintainer decided the same day that vtium stays root under libseat, as ADR-1400 D1 says: it takes the display and input devices from the seat manager and keeps root to start /bin/login and its helpers. The row's feature, reason and note say so.

> Edited 2026-11-07: `conformance issues` now holds the issue column to GitHub. It fails when a planned, partial or weighed row names no open issue, when an open issue is named only by supported rows, and when a number in the column is not an issue. Its first run found nine such cases, and this edit settles the five that the tables themselves got wrong. OSC 8 links, supported before, are partial: vtium keeps links, underlines the one under the pointer and copies it with Ctrl+click, and it does not yet draw the overlay row that shows the link's address while Ctrl is held. Multi-monitor, supported before, is partial: one grid shows on every display of a head, but a picture that the other display starves waits for input before it draws, and no test drives two displays. Glyphs baked into vtium as read-only atlases, weighed before, are refused, since ADR-1200 decided that vtium generates every glyph at run time and builds no glyph set ahead of time. The weighed modes 2034 and 737769 named the closed issue for vtium-term's modes; 737769 now names the work on input methods, and 2034 an issue of its own that decides it. The other four cases are issues whose work is done, or waits only on the maintainer's review on a real VT; they close outside the tables. Before this edit the counts above read 128 mode rows, with 39 supported, 9 ignored, 73 refused, 1 planned and 6 weighed, and 181 feature rows, with 110 supported, 3 partial, 44 ignored, 7 refused, 16 planned and 1 weighed; edits since 2026-11-06 had added rows without changing them.

> Edited 2026-11-07: the work on left and right margins builds DECLRMM, DECSLRM, DECIC, DECDC, DECBI and DECFI, so mode 69 and the rows for DECSLRM and for DECIC with its kin are supported. Before this edit the counts above read 43 supported modes and 1 planned, and 110 supported, 5 partial and 16 planned feature rows.

> Edited 2026-11-07: mode 1035 stays supported, since setting and resetting it work and the mode table in vtium-term has no partial status; its note names the gap the work on the kitty keyboard protocol found, that the host passes vtium-term no NumLock state with a key, and the issue that closes it.

> Edited 2026-11-07: the work on SGR renditions draws conceal (SGR 8 and 28) and overline (SGR 53 and 55), so `sgr.conceal` and `sgr.overline` are supported; rapid blink and superscript and subscript stay planned until they are drawn. The work on libseat seats takes the seat, its display devices and VT switching from the seat manager, so `platform.libseat` is partial; its note names what is not built or proven yet. Before this edit the counts above read 112 supported, 4 partial and 15 planned feature rows, which no longer matched the table.

> Edited 2026-11-11: the work on runtime font changes builds XTWINOPS 8 behind the `allow-window-ops` key, off by default, so `window.resize` is supported; its note says how vtium meets a request. Before this edit the counts above read 183 feature rows, 114 supported, 6 partial and 11 planned, which no longer matched the table's 185 rows.

> Edited 2026-11-13: `platform.libseat` is supported. The maintainer checked the libseat build on a laptop's VTs in seat mode under logind, and every step of the check passed; tier 3 runs the same build on seatd and on logind. Before this edit the counts above read 120 supported and 5 partial feature rows, which already did not match the table's 121 supported and 4 partial.

> Edited 2026-11-13: the features table gains `ctl.linux-blink-interval`, planned. The Linux console sets how fast the cursor blinks with `CSI 16 ; n ]`. The maintainer decided that vtium honors the sequence, and the work on the cursor blink interval builds it. A survey of an outside collection of terminal test scenarios found the sequence. Before this edit the counts above read 185 feature rows, 122 supported, 3 partial and 8 planned. Those counts did not match the table's 123 supported and 2 partial.

Each row names the layer it lands in, from the project's plan: minspec, text, images, gpu, seat or top, or `bidi` for a row that only a build with the cargo feature `bidi` has (ADR-1004 D13).

Each row's `origin` names where the mode or feature comes from: the standard, the terminal or the program that first defined it. The tables together are a composite of what each origin defines, and the origin is one entry from this list:

- **ECMA-48**: the ECMA-48 standard (ISO 6429, ANSI X3.64), with ITU T.416 for its colon form.
- **DEC**, or a named DEC terminal where the row comes from one model or its manual: DEC VT52, DEC VT100, DEC VT220, DEC VT240, DEC VT340, DEC VT420 and DEC VT510.
- **xterm**: xterm's control sequences and resources.
- **rxvt** and **urxvt**: rxvt and its Unicode fork, rxvt-unicode.
- **AT&T 610**: the AT&T 610 terminal, whose cursor blink mode xterm took.
- **kitty**: kitty's protocol specifications.
- **iTerm2**, **WezTerm**, **Ghostty**, **foot**, **mintty**, **VTE**, **contour** and **ConEmu**: the terminal that first defined the sequence or the feature, as its documentation says.
- **FinalTerm**: the terminal whose OSC 133 prompt marks shells adopted.
- **fixterms**: the key encoding Paul Evans proposed, which the kitty keyboard protocol supersedes.
- **tmux**: tmux's control mode.
- **X11**: the X Window System, whose color names vtium reads.
- **Unicode**: the Unicode Standard and its annexes.
- **Linux console**: the Linux kernel's own console, as console_codes(4) describes it, and its PSF fonts.
- **kmscon**: the released `kmscon` program, whose private OSC strings a program may send.
- **vtium**: vtium's own, a feature that no other origin defines, such as heads or the image helper.

A row whose origin the list lacks adds the origin to the list in the same edit.

> Edited 2026-11-06: the maintainer set ADR-1000's shape: a complete table of every feature vtium aims to support, as a composite of what each origin defines, where each row says where the feature comes from and why vtium does what it does with it. The tables gained the `origin` and `reason` columns that day. Each reason that sat in a note moved into `reason`, and the notes that named an issue by number say what the issue is about instead.

vtium supports everything the C reference supported, apart from the departures vtium lists, and goes beyond it.

### The rule for a new mode or feature

A mode or feature the C reference had keeps its status unless this record changes it. Beyond those, the target set is the union of two parts: the capabilities terminals generally support, which this rule admits, and the operator's list, the features the operator named as ones they use (the comment of 2026-10-26 on the issue for vtium-term's modes, SGR and replies). The rule admits a new one when xterm's control sequences and at least one of foot, Ghostty, WezTerm or kitty document it, or when the console has a use for it that its row names (mode 2028 for reflow, mode 2033 for VT switching, mode 2048 for a display hot-plugged or changing mode). vtium is not a superset of every reference terminal, and obscure DEC-era modes are not the aim: a new mode neither part brings in is refused and stays out of vtium's mode table, and a new feature neither part brings in is ignored. A refusal carries a reason a reader can check.

The support columns count only published documentation: for xterm and foot, whose documents list everything they do, absence reads "no"; elsewhere silence reads "unverified". kitty is read from its specification pages, never its source.

The `linux` column records the Linux kernel's own console: the escape and control sequence handling of the kernel's VT layer, which every Linux machine has, which fbcon draws, and which programs meet under `TERM=linux`. Its source is console_codes(4), the manual page that lists the controls the console acts on. Where the page says nothing about a row, the kernel's source decides, and the cell cites the line of drivers/tty/vt/vt.c or drivers/tty/vt/selection.c in Linux 7.2 that settles it: the page has not caught up with every sequence the kernel handles, so bracketed paste reads "yes (vt.c:2037)". A sequence that neither the page nor the source handles reads "no", as for xterm and foot. A row whose behaviour neither settles reads "unverified", and docs/vt-review-linux.md lists those rows as steps for the maintainer's review on a real VT, since an agent on the development machine never opens a kernel console. "n/a" marks a row about vtium's own parts, such as its compositor or its heads, that a terminal's control sequences do not cover. The column uses the words the other terminals' columns use: yes, no, partial with what is missing, custom only, unverified and n/a, with a short reason in parentheses where a bare word would mislead.

### Refusals

- **KAM (ANSI 2):** a program could lock the keyboard of the only terminal on the console.
- **DECCOLM (DEC 3):** the console's size comes from the display and the font.
- **DECNRCM (DEC 42):** national replacement sets are obsolete next to UTF-8.
- **Hilite mouse tracking (DEC 1001) and XTHIMOUSE:** the terminal waits on the program's answer before it can show a selection.
- **XTWINOPS window manipulation (1-7, 9, 10 and 24 and above):** vtium has no window to move, raise, iconify or resize. A program resizing the grid in characters (CSI 8 t) is held apart, for the work on runtime font changes.
- **DECUDK:** program output would program a key that later types into the shell.
- **OSC 46:** it writes a log file on the host.
- **kitty file transfer (OSC 5113):** it reads and writes host files at the stream's request.
- **kitty remote control (DCS @kitty-cmd):** program output would drive the terminal.
- **OSC 52 read:** the reply would hand clipboard text to whatever reads the pty.

### Declined for now

VT52 (DECANM, DEC 2) has a reserved row, ignored: DECRQM reports the ANSI state (DECANM set) as permanent and a reset does nothing. Its own issue holds the space for full VT52 support in a later layer.

### Replies and policy

Every reply follows the reply rule (docs/architecture.md, "Determinism"): it is built from vtium's state, fixed strings and re-encoded numbers, selectors and hex, never from request bytes.

- **A report answers only where vtium has the thing it reports, with its real value.** The text area in cells (XTWINOPS 18) and in pixels (14), the cell size (16), and the screen in cells (19) and pixels (15), the screen being the display the grid fills, get real numbers. Reports about a window or icon vtium does not have (11, 13, 14;2, 20) get no reply.
- **Title reporting (XTWINOPS 21) answers a fixed empty title.** OSC 0, 1 and 2 are ignored, so no title is kept.
- **OSC 52 write is on**, inside the OSC string bound; the read is refused above.
- **A paste never carries its own end marker:** vtium filters every paste, bracketed or not, as xterm's paste filtering does with `allowPasteControls` false: each C0 control but HT, LF and CR, DEL, and each C1 control, a code point in UTF-8, becomes a space.
- **ENQ answers an empty answerback.** The C reference answered ACK (0x06); vtium lists this as a departure.
- **Focus (1004) and visibility (2033) reports are allowed:** a program opts in, and what they reveal is that the user switched VTs.
- **setBackground and setForeground**, kmscon's private OSC strings (`kmscon/src/terminal.c:863` in the C reference), hand the display to a graphical program and take it back. No DEC or ECMA specification defines them. vtium reads them behind the key `allow-display-ops`, on by default, as the C reference behaved. The key sits beside `allow-font-ops` and `allow-window-ops`. Only a console that a head shows may send setBackground. On the kernel's VT, setBackground gives up every card the sending head draws on, and other heads on those cards pause (ADR-1401 D16). Under a seat manager, logind or seatd, vtium ignores setBackground, logs that it did, and sends no focus report. There, a graphical program runs in a session of its own on another VT. Handing a program only some ports of a card, through a DRM lease that vtium lends, is left to a later decision. That decision waits for a prototype in which an unmodified compositor runs on a lease from vtium. A shown console whose head can no longer draw gets a focus-out (1004) at the hand-over and a focus-in when the hand-over ends. Each shown console that has the focus gets the same at a VT release and acquire (ADR-1401 D5).

> Edited 2026-11-13: the maintainer named the key, chose which console may send setBackground, chose what a hand-over gives up on the kernel's VT and under a seat manager, added the focus reports at a VT switch, and left a hand-over of single ports through a DRM lease to a later decision (`considered`, 2026-11-13). The bullet ended: "vtium keeps them behind a configuration key, on by default, as the C reference behaved."
- **OSC 21** takes kitty's color-protocol meaning, which Ghostty shares, not xterm's DECSWT title. A query for a key vtium does not know is left out of the reply. kitty answers it with `unknown=` and the key's name in base64, which carries request bytes, so vtium lists this as a departure from kitty.
- **Bidirectional modes** follow ADR-1004: BDSM, 1243, 2500 and 2501 are supported and DCSM is ignored and permanently set, each only in a build with the `bidi` feature, which the minspec build leaves out. DECRLM and DECRLCM are refused, since SCP and BDSM replace them.
- **LNM (ANSI 20) stays supported**, as xterm and the C reference have it, though ECMA-48's 5th edition withdraws it.
- **The wake-up retry keeps going while vtium's VT is shown.** vtium may fail to take its displays back while its VT is shown, as while another program holds DRM master. vtium then tries again every 50 ms for about 2 seconds. After that, vtium tries again once a second for as long as its VT is shown. A key press, a pointer action or a hotplug still tries again at once. A VT release stops the tries, and a VT acquire starts the fast tries again. So a display with no keyboard, such as a kiosk's, lights within about a second of the other program letting go of the card.

> Edited 2026-11-13: the maintainer decided that vtium keeps trying to wake its displays after the fast tries run out, and the last bullet above is new. Before, the record did not describe the wake-up retry. After about 2 seconds of tries, only the next key press, pointer action or hotplug tried again, so a display that nobody touched stayed dark. The edit of 2026-11-06 above calls the hand-back's wake-up retry built, in the words "the display hand-back, whose wake-up retry is built". The retry it meant is the fast one.

### The terminfo entry and TERM

vtium ships its own terminfo entry, named `xterm-vtium` and also `vtium` (`crates/vtium/install/xterm-vtium.ti`), and sets `TERM=xterm-vtium` in each session by default. Most programs and platforms do not know vtium, so vtium establishes a name of its own, and the entry says what vtium does: the rows these tables mark supported, which are largely what kitty does. Where vtium behaves as kitty does, the entry writes the capability as kitty's entry writes it, and a comment beside each capability names the rows it rests on.

- **The name starts with `xterm-`,** because programs that do not know a terminal recognize an xterm-like one by that prefix and turn on what such a terminal has: mc, for one, turns its mouse on.
- **The entry reports the mouse in SGR form,** `kmous=\E[<` with mode 1006 (DEC-1006), so a click past column 223 reports its column. The `kmscon` entry that the kmscon-terminfo package ships reports the mouse in X10 form, which stops at column 223.
- **The `term` key keeps any other name available.** `--term=kmscon` names the C reference's entry, for programs that know it, and `--term=xterm-kitty` will name kitty's entry once kitty graphics land.
- **XTGETTCAP answers from the table of the entry `TERM` names.** The C reference answered from its own table while the session's `TERM` was its entry's name, `kmscon`, and vtium follows that flow for each entry it has a table for. Under `xterm-vtium`, or under `vtium`, the entry's other name, it answers from the table generated from vtium's entry, and TN, the terminal's name, answers `xterm-vtium` under both. Under `kmscon` it answers from the C reference's table, generated from the C reference's entry with `Ms` (`crates/vtium/install/kmscon.ti`), which the C reference's capability probe judges. The kmscon-terminfo package that a machine keeps for the `kmscon` name ships a different entry: it lacks `Ms`, `Smulx`, `Setulc`, `Sync`, `XM`, `XF`, `E3`, `BD`, `BE`, `PS`, `PE`, `fe`, `fd`, `kxIN`, `kxOUT`, `smxx`, `rmxx`, `sitm`, `ritm` and `dim`, reports the mouse with `kmous=\E[M` where the table has `\E[<`, and ends `setrgbf` and `setrgbb` with `;m`. vtium keeps the C reference's answers anyway, because the C reference's copied probe judges them. Under any other name every capability XTGETTCAP asks for is unknown.
- **Each session also gets `TERM_PROGRAM=vtium` and `TERM_PROGRAM_VERSION`,** vtium's version, which name the terminal whatever `TERM` names.

vtium presents itself as a kitty-compatible terminal under its own name. For kitty's protocols, vtium follows kitty's specification, and where the specification says nothing, kitty's own behaviour, which black-box runs of kitty check. vtium answers kitty's detection queries as kitty answers them. It keeps `TERM=xterm-vtium` and does not set `KITTY_WINDOW_ID`. That variable tells a program that it runs in kitty, so the program would use features only kitty has, such as remote control, which vtium does not have. ADR-1100 records the same decision for kitty's image protocol.

vtium presents itself as kitty, kmscon, xterm or another terminal wherever a program needs that to keep a capability, so that no program fails on a terminal type it does not know, and so vtium keeps every such compatible identity, the `TERM` names and terminfo entries it installs (among them the `kmscon` entry, which `make install-tree` installs in tier 3's guest), its XTVERSION and device-attribute replies, and its answers to protocols' detection queries, whatever its packaging or install changes.

The C reference's default `TERM` was `kmscon`; vtium lists its own default as a departure in `crates/vtium/README.md`. vtium's terminfo package, `vtium-terminfo`, ships only vtium's own entry, under its two names `xterm-vtium` and `vtium`, and a machine that keeps the `kmscon` name takes its entry from the `kmscon-terminfo` package. A host that has no `xterm-vtium` entry, such as a machine reached over ssh, does not know the name: there the user copies the entry over (`infocmp -x xterm-vtium | ssh HOST tic -x -`) or sets another `TERM`, and on the console `--term` names another entry for every session.

> Edited 2026-11-06: vtium's entry gains the bare name `vtium` beside `xterm-vtium`, so XTGETTCAP answers from vtium's table under `TERM=vtium` as it does under `TERM=xterm-vtium`, and TN answers `xterm-vtium` under both. Until then only `xterm-vtium` named the entry, and under `vtium` every capability XTGETTCAP asked for was unknown. The 2026-11-01 entry in `considered` keeps its words, "vtium ships its own entry only as xterm-vtium", as the record of that day's decision.

> Edited 2026-11-02: the maintainer's rule on compatible identities (`basis`, 2026-11-02) adds the paragraph after the one on kitty, so that the work that gives vtium's install its own unit names keeps every name and reply programs know vtium by. The same edit names the install that holds each entry: `make install-tree` installs the `kmscon` entry, and the package ships only `xterm-vtium`. The paragraph after it read: "vtium ships only the `xterm-vtium` entry, and a machine that keeps the `kmscon` name takes its entry from the kmscon-terminfo package."

> Edited 2026-11-01: the maintainer's decision on vtium's terminal name (`considered`, 2026-11-01) adds this section, and the maintainer's direction of the same day adds the paragraph on presenting vtium as a kitty-compatible terminal. Until then vtium's entry was named `kmscon`, `TERM` defaulted to `kmscon`, and the XTGETTCAP table, the C reference's, answered under every `TERM`.

### The tables as data

The TSVs are the source for the target set until vtium-term's mode table and the capability list exist. From then those become the source: the TSVs are generated from them, and a test fails when the committed TSVs differ, as docs/architecture.md's "Tables, references and maps" requires of every fact stated twice. A row's status changes in its source, in the same change as the work that changes it.

The mode table's columns: id, kind, number, name, spec, origin, linux, xterm, foot, ghostty, wezterm, kitty, status, reason, note, layer, issue, tests. The feature table's: id, group, feature, spec, origin, linux, the five terminals, status, reason, note, layer, issue, tests. A row's `reason` says, in a plain phrase, why vtium does what its status says: which programs or users rely on a supported row, or why a row is planned, weighed, ignored or refused. Its `note` says what is missing and how the row is built. Neither names an issue by number; the `issue` column does that. On 2026-10-26 the work that traces each row to its tests added the `id` and `tests` columns; `docs/scripts/coverage` states how a feature's id is formed and is the only writer of `tests`. On 2026-10-27 the comparison with Ghostty 1.4.0's release notes added two rows, kitty graphics relative placements and Unicode placeholders, which the C reference left to step 5 of its ADR-103 with animation, and the Ghostty column took those notes as its source for kitty animation, OSC 5522 and mode 5522, OSC 72 and OSC 99. The work on named palettes added a row for the C reference's named palettes and its custom one, which the `palette` key chooses. On 2026-10-27 the work on pointer selection added two rows the pointer's selection brought, the paste and Shift's override of mouse tracking, each documented by xterm and at least one other terminal. On 2026-10-27 the work on blink options added two rows for the text blink's options, `blink-text` and `blink-decorations`, on the operator's list from the CPU compositor's review. On 2026-10-27 the operator decided that vtium reads X11 color names in OSC color strings in every build, and the work on X11 color names added the row for them. On 2026-10-27 the work on screen blanking added a row for each of the Linux console's blank sequences, whose console use is the DPMS blank. On 2026-10-28 the operator decided that vtium spells it color, and the feature ids that spelled colour were renamed, the one change ever made to a committed id: `sgr.underline-colour` to `sgr.underline-color`, `sgr.extended-colour` to `sgr.extended-color`, `sgr.draw-time-colour` to `sgr.draw-time-color`, `colour.palette.query` to `color.palette.query`, `colour.palette.set` to `color.palette.set`, `colour.names` to `color.names`, `colour.palette.named` to `color.palette.named`, `colour.dynamic.query` to `color.dynamic.query`, `colour.dynamic.set` to `color.dynamic.set`, `colour.pointer` to `color.pointer`, `colour.selection` to `color.selection`, `colour.tektronix` to `color.tektronix`, `colour.special` to `color.special`, `colour.kitty-protocol` to `color.kitty-protocol`, `colour.stack` to `color.stack`, `colour.wide-gamut` to `color.wide-gamut`, `colour.scheme-report` to `color.scheme-report` and `cursor.colour` to `cursor.color`. On 2026-10-29 ADR-1401 added six rows for heads, `platform.heads`, `platform.heads.rules`, `platform.heads.focus`, `platform.heads.guards`, `platform.heads.rescue-key` and `platform.heads.on-loss`, in the seat layer with `platform.multi-monitor`, their issue cell empty until the heads build has an issue. On 2026-11-01 ADR-1302, the record on glyph ink that reaches past its cell, answered the `text.overhang` row's foot column from its evidence: foot documents `overflowing-glyphs`, which lets a glyph draw into one more cell to its right. The Ghostty column stays unverified, since the columns count only published documentation. On 2026-11-01 the maintainer decided that a BEL from any console rings the VT bell, whether that console is shown or hidden, as the C reference did, which the `ctl.bel` row's note says. On 2026-11-06 the `linux` column took the place of the `c_reference` column, which recorded what the C reference did for each row; "The rule for a new mode or feature" describes the new column.

> Edited 2026-11-06: the maintainer's decision that vtium judges itself by its own goldens (ADR-1503) makes this change, so that the tables compare vtium with a console that every Linux machine has and that a reader can try, and no longer with a program a reader cannot open. The three refused modes whose note read "the C reference's reason" now state the reason. The paragraph above ended: "A `c_reference` cell opens with a tsm-modes.def status in capitals; works, missing or partial, naming the probes behind the verdict where there are any; parsed, for a sequence the C reference read and did nothing with; audited without a probe; unverified; or absent. What follows says what the C reference did." git keeps the column's values.

> Edited 2026-11-01: the maintainer's answers in the steering session's interview (`considered`, 2026-11-01): the `text.overhang` row's Ghostty column goes back to unverified, and the `ctl.bel` row's note says a hidden console's BEL rings the VT bell. The paragraph above said the Ghostty column was answered with "yes" from Ghostty's source, and the column read "yes (no clip, read from Ghostty's source by ADR-1302)"; the earlier text is in git at 292d0b5.

Each row proves its coverage by the contract of the issue that traces each row to its tests, whose columns and lint land with that issue:

- Every row has a stable `id`: `ANSI-4` and `DEC-2026` for modes, and a dotted name such as `sgr.dim` for features.
- A test that covers a row carries `// covers: <id>`, and a replay file carries the same tag in a comment.
- A `tests` column lists the tests that cite the row. Only the lint writes it.
- `status` records the decision. Coverage is derived from the tags.
- Ignored and refused rows are covered too, by a test that the mode or feature stays ignored or refused.
- The minspec slice is done when every minspec row that needs a test has one: every row but a mode that is not in vtium's mode table.

## Consequences

### Positive

- Every work item inherits its scope from a row, and a dropped capability shows as a row whose status changed.
- DECRQM, the capability probes and the terminfo entry have one list to agree with.
- The refusals and the reply policy are settled once, so a handler author does not reopen them.

### Negative

- 69 mode rows are refused and 6 weighed with no row in vtium's mode table; the table carries them so each decision and its reason are visible.
- The Ghostty, WezTerm and kitty columns are mostly "unverified" for standard modes, since their docs do not list them; a later edit can settle them from documentation.
- The ignored and refused rows have to stay ignored and refused through every later handler.

### Neutral

- Eleven issues carry the new rows, one each on modes and controls beyond the C reference, left and right margins, rectangular areas and protection, in-band reports, SGR and colour stacks, pointer shapes, kitty text sizing, double-width and double-height lines, iTerm2 inline images, a static image at the login prompt and font merging, with one more for VT52. The issues for SGR overline, superscript and subscript, for the OSC 4, 10, 11 and 12 colors, and for outline fonts are widened for SGR 6 and 8, the pointer, selection and kitty colors, and ligatures as an option.
- The conformance work's departures list gains ENQ's empty answerback, DA1's conformance level (one of xterm's in place of the C reference's 60), and, as a departure from kitty, OSC 21's omission of unknown keys.

## Alternatives Considered

- **The C reference's set alone.** Rejected: the operator's direction is that vtium covers what other terminals support, and the C reference is the floor.
- **Every sequence any listed terminal accepts.** Rejected: one-terminal extensions would each carry handler and test cost with no common use, and several put host files, the clipboard or terminal control in the stream's reach.
- **Rows for every recognised number, ignored rather than absent.** Rejected: a fixed DECRQM reply for a mode vtium has no concept of tells a program nothing it can use, and changes replies the C reference's goldens record as 0.
- **Answer every XTWINOPS report with fixed values, as foot does.** Rejected: an invented window position or iconified state is a reply about a function vtium does not have.
