---
contract: adr/v1
kind: decision
verb: change
capability: process
basis:
  - operator: aaronsb
    level: guided
    said: "I think we have created something that would be valuable to a lot of people, if it could be communicated what it is capable of."
    via: the steering session's brief for the front pages, 2026-11-14, quoting the maintainer
  - operator: aaronsb
    level: guided
    said: "we are not comparing against them in our claims, we instead 'hold our own' by telling the story of what it is."
    via: the steering session's brief for the front pages, 2026-11-14, quoting the maintainer on how the front pages describe vtium beside other terminals and consoles
  - operator: aaronsb
    level: guided
    said: "[vtium-config] needs a lot of polish, but the structure is there in its intentions"
    via: the steering session's brief for the front pages, 2026-11-14, quoting the maintainer
  - evidence: "a survey made 2026-11-14 for this record of the published front pages of seven terminals for desktops, read from their websites and READMEs, with no source read: the groups of capability a reader expects, in their usual order, are speed or the GPU, platforms, fonts, Unicode and emoji, color, images, windows and tabs, selection and search, configuration, scripting, and standards; one page states the rule it follows when specifications disagree; two state what they leave to other tools; the wording that serves a reader names a thing and its scope, and backs a claim with something a reader can check."
  - evidence: "the front pages on integration at 9f51a91e, read 2026-11-14: README.md and docs/index.md open with 'vtium is a terminal for the Linux console', then say what a VT is, how vtium renders, a list of capabilities in one sentence, the fallback to getty, a paragraph that calls the kernel's console legacy, the hardware range and where the code began. Neither page says what vtium does with several displays, which specifications it follows, which image protocols it reads, how it reads keyboards and mice, or what it leaves to other tools. Only README.md says that vtium renders right-to-left text, double-width CJK text, and through OpenGL ES 3.0; docs/index.md says 'it needs no display server'."
  - evidence: "ADR-1000's tables on integration at 9f51a91e, counted 2026-11-14: the features table has 187 rows, 126 supported, 2 partial (double-size lines, and font merging), 7 planned (kitty text sizing, ligatures, the Linux console's cursor blink interval, the pointer shape, input methods, Braille displays, and the login image), 44 ignored and 8 refused; the modes table has 131 rows, 44 supported, 10 ignored, 75 refused and 2 weighed."
  - evidence: "crates/vtium-term/src/terminal/reply.rs at 9f51a91e, reply_da1: vtium answers DA1 with CSI ? 62 ; 4 ; 22 c while sixel is offered and CSI ? 62 ; 22 c otherwise; crates/vtium-term/src/terminal.rs, set_conformance: DECSCL takes levels 61 to 64 and DECRQSS reports the level DECSCL set, 62 until one is set."
  - standard: "xterm's Control Sequences (ctlseqs), Send Device Attributes (Primary DA), read 2026-11-14 at invisible-island.net: the reply's first number names the terminal, 62 a VT220, 63 a VT320, 64 a VT420 and 65 the VT510 to VT525; the attributes that follow are 1 132 columns, 2 printer, 3 ReGIS graphics, 4 sixel graphics, 6 selective erase, 8 user-defined keys, 9 national replacement character sets, 15 technical characters, 16 locator port, 17 terminal state interrogation, 18 user windows, 21 horizontal scrolling, 22 ANSI color, 28 rectangular editing and 29 ANSI text locator."
  - standard: "the VT510 Video Terminal Programmer Information, DA1, read 2026-11-14 at vt100.net/docs/vt510-rm/DA1.html: 'The VT510 is a level 4 terminal so its service class code is 64.'"
  - precedent: ADR-1000
  - precedent: ADR-1503
  - precedent: ADR-1502
  - precedent: ADR-1401
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related:
  - ADR-1000
  - ADR-1401
  - ADR-1502
  - ADR-1503
amends:
  - "ADR-1502#D2: the README and the home page"
---

# ADR-1509: vtium is the terminal a Linux machine boots into, and its front pages tell what it does there in its own terms

## Summary

- **Decided:** vtium's two front pages, the README on GitHub and the site's Home, open with one sentence: vtium is a terminal that runs on the Linux console, in place of the login prompt a Linux machine boots into. An introduction says what that gives the reader, where a full terminal helps, and that vtium is alpha software. A linked list then names one topic page for each part of what vtium does there: text in any script, a terminal on each display, images, keyboards and mice, the control language and the rules vtium follows, where vtium runs, security and the system, and what vtium leaves to other tools. Each front page ends with where to start. Every claim rests on a row that ADR-1000's tables mark supported, or on the code; a planned or partial row is called planned or partial and links its issue. The pages never define vtium by comparison with another terminal or console, and they name a protocol by its specification where a reader needs the name.
- **Trades away:** the README grows back from the short page of ADR-1502 D2 to about one screen of sections. Because the two pages tell the same story, a change to one goes to the other in the same change. A reader who wants to know how vtium compares with other consoles reads Why vtium, which this record leaves as it is.
- **One-way?** No. The pages are text that a later change can rewrite, and this record is the yardstick that change is checked against.
- **Probes:** *Confident (the story):* you want the front pages to say what vtium gives the person who boots into it, part by part, and to name no other terminal or console in a claim. *Not confident (the device class):* vtium answers DA1 as a VT220 (62). Should it answer as a VT420 (64), the class whose editing set it implements, and add the attributes for selective erase (6) and rectangular editing (28)? The probe under Decision gives the case for and against.
- **Inversion:** Between "a list of features" and "an argument against the kernel's console". A list tells a reader what is there, but not why each part matters to them. An argument tells the reader what is wrong elsewhere, but not what vtium is. The answer tells the story of the console a person meets at boot, from their side, and lets each claim stand on vtium's own tables.

## Context

vtium has not been announced publicly yet. When it is, the README and the site's Home are what a reader meets first. The maintainer believes vtium would serve a lot of people if the pages said what it can do (`basis`). Today both pages open with a sentence and a list of capabilities, then spend a paragraph on why the kernel's console is called legacy. Neither says what most sets vtium apart: a terminal on each display, the specifications it follows, the image protocols it reads, how it reads keyboards and mice, and what it leaves to other tools. The two pages have also drifted apart: only the README mentions right-to-left text and OpenGL ES 3.0, and the Home frames vtium by what it does without (`basis`).

ADR-1503 D2 decided that vtium's behaviour is defined by the specifications it cites, ADR-1000's tables and its goldens, and that no committed text defines vtium by comparison with another program. The maintainer has now said the same of the front pages' claims: vtium holds its own by telling the story of what it is (`basis`). ADR-1502 D2 decided on a short README and a Home that share one paragraph. That shape leaves no room for the story, so this record amends ADR-1502 D2.

## Decision

### D1: one sentence says what vtium is

Both pages open with the same sentence: vtium is a terminal that runs on the Linux console. Three short paragraphs follow it, above the screenshot:

1. What that means from the reader's side: vtium takes the place of getty on a VT, so the console a machine boots into is a terminal with the features a program expects of one. The paragraph also says how vtium renders, through KMS/DRM on the CPU or the GPU.
2. Why a reader wants that: where a full terminal helps at the machine's own screen, such as a server without a desktop, a kiosk, a small board, or the console before a desktop starts.
3. The release status: vtium is alpha software, at the version the workspace's `Cargo.toml` names. A package takes every free VT when it is installed, so the paragraph points to the tutorial that tries vtium on one VT. It also says that, with the default setup under systemd, a getty takes the VT back when vtium fails.

> Amended 2026-11-14, while the record is proposed: a cold read of the draft front pages led the maintainer to ask for a why paragraph, the release status, the Security and the system topic, and a language page that separates how vtium interprets a sequence from how it chooses what to support. D1, D4 and D8, and the Summary's Decided line, now describe the pages as built. Every other decision and every field is unchanged, and the earlier text is in git at 069d8d84.

### D2: the pages tell the story by what vtium does

Each claim says what vtium does, in the present tense, and rests on a supported row of ADR-1000's tables or on the code. A partial or planned row is called partial or planned and links the GitHub issue that tracks it, as AGENTS.md asks of pages for users. The pages name no other terminal or console in a claim, and avoid "unlike", "like X" and "no X needed". Where a reader needs a protocol's name, the pages give it by its specification: xterm's control sequences, the DEC VT510 manual, the kitty graphics protocol, the kitty keyboard protocol, sixel, OSC 8 and OSC 52. The history of where vtium's code began stays in the past tense, in one sentence that points to the Provenance section of VTium in depth.

### D3: where vtium runs

vtium runs on a Linux VT in place of getty, started by its systemd unit or from the init system's entry where a getty ran. It renders into DRM dumb buffers through KMS, or through fbdev where there is no DRM. Every build renders on the CPU, and a build with the `gpu` feature also renders on the GPU through OpenGL ES 3.0, drawing the same pixels as the CPU path (ADR-1303). The smallest build, minspec, is built for a board with 512 MB of memory and a 320×240 display. A figure on the front pages names its source, as tier 3's measurement of minspec does in What vtium does: a 3,216,864-byte console binary and a peak resident memory of 5.81 MiB at 320×240, from `make tier3-minimal` at commit `c107f9a` on 2026-11-01. At the other end, one vtium drives several large displays.

A head is a terminal for one or more displays, with its own grid, font and consoles (ADR-1401). With no configuration, vtium runs one head across every display. Rules in vtium.conf place each display and input device in a head, and the first matching rule wins, so each display can show a terminal of its own. Sessions survive a restart of vtium, because a keeper holds each session's terminal while vtium restarts. The pages link the security page for the one case where sessions end.

### D4: the control language and the rule vtium follows

The base of vtium's control language is xterm's control sequences (ctlseqs). The spec column of ADR-1000's tables names the specification each row follows, and most rows name ctlseqs. From the DEC terminals, vtium supports left and right margins, column insert and delete, rectangular area operations, character protection, the DEC character sets, 8-bit controls, DECSCL, DECRQM, DECRQSS and DA3. Double-width and double-height lines are partial. Among the newer protocols, it supports sixel, the kitty graphics protocol with its animation and Unicode placeholders, iTerm2's inline images, the kitty keyboard protocol, OSC 8 links, OSC 133 prompt marks, OSC 52 copy, synchronized output and in-band resize reports. The kitty text sizing protocol is planned.

The pages state how vtium interprets a sequence, as ADR-1000 and ADR-1503 decide it, in three rules:

1. Each sequence follows its defining specification, the one its row names.
2. Where two specifications give one sequence different meanings, vtium's feature table records which meaning vtium uses. The pages give `CSI s` as the example: it sets the left and right margins (DECSLRM) while mode 69 is set. OSC 21, which follows the kitty color protocol, is another.
3. Where a specification leaves a choice open, vtium's own tests settle it, and the choice is written down: vtium's goldens decide, and the crate's README states the choice (ADR-1503 D2).

Apart from those rules, the pages say how vtium chooses what to support: the capabilities that xterm's control sequences and other terminals' documentation both define, plus others chosen because they are useful on a console (ADR-1000, "The rule for a new mode or feature"). The pages name no person in that statement.

> Amended 2026-11-14: see the note under D1.

One rule comes before all three: vtium refuses any capability that lets a program's output read from or control the machine, whoever offers it (ADR-1000, the inversion). vtium builds every reply from its own state and never echoes bytes the program sent.

### D5: text

vtium splits text into Unicode's extended grapheme clusters and gives each one or two cells. It renders combining marks, emoji presentation and color emoji, and Chinese, Japanese and Korean text takes two cells a character. A font helper shapes each cluster, so scripts such as Devanagari and Tamil join their syllables. A build with the `bidi` feature lays out right-to-left text by the Unicode bidirectional algorithm and joins Arabic letters. A font set, or frit, lists fonts in order, and later fonts fill in the characters earlier ones lack. vtium reads bitmap fonts (PSF, BDF and the kernel's own) and outline fonts, and two built-in fonts draw the first frame. Ligatures are planned. Box drawing, blocks, Braille patterns, sextants and octants are drawn to fit the cell exactly, whatever the font.

### D6: input

vtium reads each keyboard and pointer from the kernel's input devices, and libxkbcommon turns keys into characters with the system's keyboard layout. vtium.conf's rules route each device to a head. Programs get keys in xterm's encodings, with modifyOtherKeys, or in the kitty keyboard protocol. vtium reports the mouse in modes 9, 1000, 1002 and 1003, in the SGR, urxvt and pixel encodings, and reports focus and bracketed pastes. A user selects text with the pointer and pastes the selection with the middle button or Shift+Insert. An input method for composing Chinese, Japanese or Korean text is not built yet, and an issue tracks it. Braille displays are planned.

### D7: what vtium leaves to other tools

The pages say plainly what vtium does not do, with the reason where a reader would expect one. vtium never opens a link; it copies the address. It never lets a program read the clipboard back. It never echoes a window title back, moves a window or transfers a file, because a console has one screen and those features would let output control the machine. A head shows one console at a time, and keys switch between its consoles. vtium draws no tabs or split panes; a terminal multiplexer such as tmux runs in vtium as an ordinary program and splits a console into panes. Desktop notifications and progress bars belong to a desktop, and vtium shows neither.

### D8: the shape of the two pages

The README and the Home tell the same story in the same order: the introduction of D1, the screenshot, a linked list of topics, and where to start. The list names one topic page for each part of D3 to D7, each a page under the site's Home: Text in any script, A terminal on each display, Images, Keyboards and mice, The terminal's language, Where vtium runs, Security and the system, and What vtium leaves to other tools. Security and the system gathers what runs as root, the sandboxed font and image helpers, the reply rule of D4, and how vtium works with systemd and logind and without them (D3). Where to start gives the Arch install from the AUR and one table of tasks and pages, the same on both front pages. The two pages differ only where their settings differ. The README keeps the badges, of which only the AUR's packaging badge remains, the sections on contributing and the license, and links each topic page on the site. The Home keeps the vtium-config paragraph of D9 and its note that the site describes `main`. A change to the story goes to both pages in the same change. Each page says each fact once and links the fuller page.

D8 amends ADR-1502 D2, which made the README short and gave the two pages one shared paragraph. The README stays the front door on GitHub and the Home the front door of the site, as ADR-1502 D2 decided. What changes is that both now carry the story: a short introduction and a list of topic pages.

> Amended 2026-11-14: see the note under D1.

### D9: vtium-config, as it is today

The pages describe vtium-config by what it does today. It runs in a terminal and draws vtium.conf as a flow from the displays, through the rules, heads and looks, to the input devices. Every change goes into a draft, and one `sudo` step installs the file. Its Fonts view tries each font on the live console as the user moves through the list. An issue tracks the rest of vtium-config's planned work. The pages do not call vtium-config finished or polished.

### The probe: which device class vtium reports

**Context.** DA1 is the query a program sends to ask what kind of terminal it talks to. vtium answers `CSI ? 62 ; 4 ; 22 c`: a VT220 (62), with sixel (4) while sixel is offered, and ANSI color (22). xterm's ctlseqs lists 63 for a VT320, 64 for a VT420 and 65 for the VT510 to VT525. The VT510's own manual says it is a level 4 terminal and answers 64. DECSCL already takes levels 61 to 64, and DECRQSS reports the level DECSCL set, so DECRQSS can report level 4 while DA1 reports a VT220.

**The case for 64.** vtium implements the VT420's editing set: left and right margins (`ctl.decslrm`, DEC-69), column insert and delete with back and forward index (`ctl.decic`), rectangular area operations (`ctl.rect`), character protection (`ctl.protect`), DECRQSS (`query.decrqss`) and DA3 (`query.da3`). A program that reads the class before it uses those features skips them while vtium says 62. The attributes 6 (selective erase) and 28 (rectangular editing) would say so too, and vtium implements both. 64 is also the highest level DECSCL takes, so DA1 and DECRQSS would agree.

**The case against.** A class promises a whole level, and vtium leaves out parts of the VT420's. It ignores the presentation state reports (`query.decrqpsr`), the terminal state reports (`query.decreqtparm`), the checksum of a rectangle (`query.xtreportsgr`), the page size (`ctl.page-size`) and the status line (`ctl.status-line`), and refuses 132 columns (DEC-3) and the page modes (DEC-60, DEC-61, DEC-64). A test program that trusts the class asks for those reports and modes and finds them missing. 65 promises more still: the VT510's keyboard and serial settings, which vtium refuses (DEC-68, DEC-73, DEC-81, DEC-97 to DEC-106), and its bell volumes and keyboard lights, which vtium ignores (`ctl.bell-volume`, `ctl.decll`). A change to DA1 also changes, at once, what every program that asks sees, so it belongs to vtium-term's replies and tier 1 goldens, not to the front pages.

**The lean.** Report 64 and add the attributes 6 and 28, so DA1 says what vtium implements. Leave 65 alone, since the VT510's own manual answers 64 and its additions are mostly hardware settings vtium refuses. Not confident: the parts of level 4 vtium ignores are reports few programs ask for, but the record cannot name which programs read the class. Until the maintainer answers, the front pages make no claim about the device class.

## Consequences

### Positive

- A reader learns, in one screen, what vtium offers them on the console and where to read about each part in full.
- Every claim on the front pages can be checked against a row of ADR-1000's tables or the code, and a gap links its issue.
- The README and the Home stop drifting, because they carry one story in one shape.

### Negative

- The README is longer than ADR-1502 D2 made it.
- A change to the story has two pages to edit.
- Pages that list what vtium does, such as What vtium does and VTium in depth, still carry older wording in places. The front pages are now the yardstick for bringing those pages up to date, one at a time.

### Neutral

- Why vtium keeps its comparison with other consoles. The front pages link Why vtium as a page to read, and make no comparison themselves.
- The device class stays 62 until the probe's answer, and a change to the device class is a change to vtium-term.

## Alternatives Considered

- **Keep the short README of ADR-1502 D2, and put the story on the Home alone.** Not taken: a reader on GitHub meets the README first, and the short page leaves out what vtium does with displays, input and specifications.
- **Lead with the case against the kernel's console.** Not taken: the maintainer's rule is that vtium holds its own by telling the story of what vtium is (`basis`), and an argument against another console defines vtium by that console.
- **A feature table in place of sections.** Not taken: a table lists what is there and not what each feature gives the reader. What vtium does, Modes and ADR-1000's tables already list the features, and the front pages link them.
