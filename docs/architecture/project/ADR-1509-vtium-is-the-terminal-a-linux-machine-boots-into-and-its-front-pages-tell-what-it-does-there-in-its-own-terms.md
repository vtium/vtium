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

- **Decided:** vtium's two front pages, the README on GitHub and the site's Home, open with one sentence: vtium is a terminal that runs on the Linux console, in place of the login prompt a Linux machine boots into. Each page then gives one plain section to each part of what vtium does there: text in any script, a terminal on each display, images, keyboards and mice, the control language and the rule vtium follows, and where it runs. Then each says what vtium leaves to other tools, and where to start. Every claim rests on a row that ADR-1000's tables mark supported, or on the code. A planned or partial row is said to be planned or partial, with a link to its issue. The pages never define vtium by comparison with another terminal or console. They name a protocol by its specification where a reader needs the name.
- **Trades away:** the README grows back from the short page ADR-1502 D2 made it, to about the length of one screen of sections. The two pages hold the same story, so a change to one goes to the other in the same change. A reader who wants to know how vtium stands beside other consoles reads Why vtium, which this record leaves as it is.
- **One-way?** No. The pages are text, and a later change rewrites them. The record is the yardstick that change is checked against.
- **Probes:** *Confident (the story):* you want the front pages to say what vtium gives the person who boots into it, part by part, and to name no other terminal or console in a claim. *Not confident (the device class):* vtium answers DA1 as a VT220 (62). Should it answer as a VT420 (64), the class whose editing set it implements, and add the attributes for selective erase (6) and rectangular editing (28)? The probe under Decision gives the case for and against.
- **Inversion:** Between "a list of features" and "an argument against the kernel's console". A list tells a reader what is there and not why it matters to them. An argument tells them what is wrong elsewhere and not what vtium is. The answer tells the story of the console a person meets at boot, from their side, and lets each claim stand on vtium's own tables.

## Context

vtium is not announced anywhere public yet. The README and the site's Home are what a reader meets first when it is. The maintainer believes vtium would serve a lot of people, if the pages said what it can do (`basis`). Today both pages open with a sentence and a list of capabilities, and then spend a paragraph on why the kernel's console is called legacy. They leave out what most sets vtium apart: a terminal on each display, the specifications it follows, the image protocols it reads, how it reads keyboards and mice, and what it leaves to other tools. The two pages have also drifted apart: only the README mentions right-to-left text and OpenGL ES 3.0, and the Home frames vtium by what it does without (`basis`).

ADR-1503 D2 decided that vtium's behaviour is defined by the specifications it cites, ADR-1000's tables and its goldens, and that no committed text defines vtium by comparison with another program. The maintainer has now said the same of the front pages' claims: vtium holds its own by telling the story of what it is (`basis`). ADR-1502 D2 decided a short README and a Home that share one paragraph. That shape left no room for the story, so this record amends it.

## Decision

### D1: one sentence says what vtium is

Both pages open with the same sentence: vtium is a terminal that runs on the Linux console. The paragraph after it says, from the reader's side, what that means: vtium takes the place of getty on a VT, so the console a machine boots into is a terminal with the features a program expects of one. It says how vtium renders, through KMS/DRM on the CPU or on the GPU, and that getty and the kernel's console take the VT back when vtium cannot start.

### D2: the pages tell the story by what vtium does

Each claim says what vtium does, in the present tense, and rests on a supported row of ADR-1000's tables or on the code. A partial or planned row is named as partial or planned, with a link to the GitHub issue that tracks it, as AGENTS.md asks of pages for users. The pages name no other terminal or console in a claim, and use no "unlike", "like X" or "no X needed". A protocol is named by its specification where a reader needs the name: xterm's control sequences, the DEC VT510 manual, the kitty graphics protocol, the kitty keyboard protocol, sixel, OSC 8 and OSC 52. The history of where vtium's code began stays in the past tense, in one sentence that points to the Provenance section of VTium in depth.

### D3: where vtium runs

vtium runs on a Linux VT in place of getty, started by its systemd unit or from an init system's line where a getty ran. It renders into DRM dumb buffers through KMS, or through fbdev where there is no DRM. It renders on the CPU in every build, and on the GPU through OpenGL ES 3.0 in a build with the `gpu` feature. The GPU path draws the same pixels as the CPU path (ADR-1303). The smallest build, minspec, is built for a board with 512 MB of memory and a 320×240 display. A figure on the front pages names its source, as tier 3's measurement of minspec does in What vtium does: a 3,216,864-byte console binary and a peak resident memory of 5.81 MiB at 320×240, from `make tier3-minimal` at commit `c107f9a` on 2026-11-01. At the other end, one vtium drives several large displays.

A head is a terminal for one or more displays, with its own grid, font and consoles (ADR-1401). With no configuration, vtium runs one head across every display. Rules in vtium.conf place each display and each input device in a head, first match first, so each display can show a terminal of its own. A restart of vtium keeps the sessions running, because a keeper holds each session's terminal while vtium restarts. The pages link the security page for the one case where sessions end.

### D4: the control language and the rule vtium follows

The base of vtium's control language is xterm's control sequences (ctlseqs). The spec column of ADR-1000's tables names the specification each row follows, and most rows name ctlseqs. Of the DEC terminals' features, vtium supports left and right margins, column insert and delete, rectangular area operations, character protection, the DEC character sets, 8-bit controls, DECSCL, DECRQM, DECRQSS and DA3. Double-width and double-height lines are partial. Of the newer protocols, vtium supports sixel, the kitty graphics protocol with its animation and Unicode placeholders, iTerm2's inline images, the kitty keyboard protocol, OSC 8 links, OSC 133 prompt marks, OSC 52 copy, synchronized output and in-band resize reports. The kitty text sizing protocol is planned.

The pages state the rule vtium follows, as ADR-1000 and ADR-1503 decide it, in four parts:

1. Each capability follows the specification its row names.
2. A capability joins the target set when xterm's control sequences and the published documentation of another terminal define it, when the maintainer names it, or when the console has a use for it that its row names (ADR-1000, "The rule for a new mode or feature").
3. Where two specifications give one sequence different meanings, the row says which meaning vtium takes, as OSC 21 takes the kitty color protocol's and `CSI s` takes DECSLRM's while mode 69 is set.
4. Where a specification leaves a choice open, vtium's goldens decide, and the crate's README states the choice (ADR-1503 D2).

One rule comes before all four: vtium refuses any capability that lets a program's output read from or steer the machine, whoever offers it (ADR-1000, the inversion). Every reply is built from vtium's own state and never repeats bytes the program sent.

### D5: text

vtium cuts text into Unicode's extended grapheme clusters and gives each one or two cells. It draws combining marks, emoji presentation and color emoji. A font helper shapes each cluster, so scripts such as Devanagari and Tamil join their syllables. A build with the `bidi` feature lays out right-to-left text by the Unicode bidirectional algorithm and joins Arabic letters. Chinese, Japanese and Korean text takes two cells a character. A font set, a frit, lists fonts in order, and later fonts fill the characters earlier ones lack. vtium reads bitmap fonts (PSF, BDF and the kernel's own) and outline fonts, and two built-in fonts draw the first frame. Ligatures are planned. Box drawing, blocks, Braille patterns, sextants and octants are drawn to fit the cell exactly, whatever the font.

### D6: input

vtium reads each keyboard and pointer from the kernel's input devices, and libxkbcommon turns keys into characters with the system's keyboard layout. vtium.conf's rules route each device to a head. Programs get keys in xterm's encodings, with modifyOtherKeys, or in the kitty keyboard protocol. vtium reports the mouse in modes 9, 1000, 1002 and 1003, in the SGR, urxvt and pixel encodings, and reports focus and bracketed pastes. You select text with the pointer and paste it with the middle button or Shift+Insert. An input method for composing Chinese, Japanese or Korean text does not exist yet, and its issue tracks it. Braille displays are planned.

### D7: what vtium leaves to other tools

The pages say plainly what vtium does not do, and why where a reader would expect it. vtium never opens a link: it copies the address. It never lets a program read the clipboard back. It never repeats a window title, moves a window or transfers a file, because a console has one screen and those features would let output steer the machine. A head shows one console at a time, and keys switch between its consoles. vtium draws no tabs and no split panes, and a terminal multiplexer such as tmux, which runs in vtium as a plain program, splits a console into panes. vtium shows no desktop notifications and no progress bars, which belong to a desktop.

### D8: the shape of the two pages

The README and the Home tell the same story, in the same sections and the same order: what vtium is, the screenshot, one section for each part of D3 to D6 with a link to the page that says more, what vtium leaves to other tools, and where to start. The two pages differ only where their places differ. The README keeps the badges, the table of documentation, the sections on contributing and the licence, and links pages under `docs/` by relative path. The Home keeps its doors into the site's four sections, and its note that the site describes `main`. A change to the story goes to both pages in the same change. Each page says each fact once, and points to the page that says more.

This amends ADR-1502 D2, which made the README short and gave the two pages one shared paragraph. The README stays the front door on GitHub and the Home the front door of the site, as D2 decided. What changes is that both now carry the story, at about one screen of short sections.

### D9: vtium-config, as it is today

The pages describe vtium-config by what it does today. It runs at a terminal and draws vtium.conf as a flow from the displays, through the rules, heads and looks, to the input devices. Every change goes into a draft, and one `sudo` step installs the file. Its Fonts view tries each font on the live console as you move to it. The rest of its planned job is tracked in an issue. The pages do not call it finished or polished.

### The probe: which device class vtium reports

**Context.** DA1 is the query by which a program asks what kind of terminal it talks to. vtium answers `CSI ? 62 ; 4 ; 22 c`: a VT220 (62), with sixel (4) while sixel is offered, and ANSI color (22). xterm's ctlseqs lists 63 for a VT320, 64 for a VT420 and 65 for the VT510 to VT525. The VT510's own manual says the VT510 is a level 4 terminal and answers 64. DECSCL already takes levels 61 to 64, and DECRQSS reports the level DECSCL set, so vtium can say it is at level 4 while DA1 says it is a VT220.

**The case for 64.** vtium implements the VT420's editing set: left and right margins (`ctl.decslrm`, DEC-69), column insert and delete with back and forward index (`ctl.decic`), rectangular area operations (`ctl.rect`), character protection (`ctl.protect`), DECRQSS (`query.decrqss`) and DA3 (`query.da3`). A program that reads the class before it uses those features skips them while vtium says 62. The attributes 6 (selective erase) and 28 (rectangular editing) would say so too, and vtium implements both. 64 is also the highest level DECSCL takes, so DA1 and DECRQSS would agree.

**The case against.** A class is a promise of a whole level, and vtium leaves out parts of the VT420's level. It ignores the presentation state reports (`query.decrqpsr`), the terminal state reports (`query.decreqtparm`), the checksum of a rectangle (`query.xtreportsgr`), the page size (`ctl.page-size`) and the status line (`ctl.status-line`), and it refuses 132 columns (DEC-3) and the page modes (DEC-60, DEC-61, DEC-64). A test program that trusts the class asks for those and finds them missing. 65 promises more again: the VT510's keyboard and serial settings, which vtium refuses (DEC-68, DEC-73, DEC-81, DEC-97 to DEC-106), and its bell volumes and keyboard lights, which it ignores (`ctl.bell-volume`, `ctl.decll`). A change to DA1 also changes what every program that asks sees at once, so it is a change to vtium-term's replies and its tier 1 goldens, not to the front pages.

**The lean.** Report 64 and add the attributes 6 and 28, so DA1 says what vtium implements. Leave 65 alone, since the VT510's own manual answers 64 and the VT510's additions are mostly hardware settings vtium refuses. Not confident: the parts of level 4 vtium ignores are reports few programs ask for, but the record cannot name which programs read the class. Until the maintainer answers, the front pages make no claim about the device class.

## Consequences

### Positive

- A reader learns, in one screen, what vtium gives them on the console and where each part is described in full.
- Every claim on the front pages can be checked against a row of ADR-1000's tables or the code, and a gap links its issue.
- The README and the Home stop drifting, because they carry one story in one shape.

### Negative

- The README is longer than ADR-1502 D2 made it.
- A change to the story has two pages to edit.
- Pages that list what vtium does, such as What vtium does and VTium in depth, still carry older wording in places. The front pages are now the yardstick those pages are brought to, one at a time.

### Neutral

- Why vtium keeps its comparison with other consoles. The front pages link it as a page to read, and make no comparison themselves.
- The device class stays 62 until the probe's answer, and a change to it is a change to vtium-term.

## Alternatives Considered

- **Keep the short README of ADR-1502 D2, and put the story on the Home alone.** Not taken: a reader on GitHub meets the README first, and the short page leaves out what vtium does with displays, input and specifications.
- **Lead with the case against the kernel's console.** Not taken: the maintainer's rule is that vtium holds its own by telling the story of what it is (`basis`), and an argument against another console defines vtium by that console.
- **A feature table in place of sections.** Not taken: a table lists what is there and not what it gives the reader. What vtium does, Modes and ADR-1000's tables already list the features, and the front pages link them.
