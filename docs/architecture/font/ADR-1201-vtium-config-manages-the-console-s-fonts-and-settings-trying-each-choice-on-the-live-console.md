---
contract: adr/v1
kind: decision
verb: add
capability: glyphs
basis:
  - operator: aaronsb
    level: directed
    said: "vtium-vitrify becomes an interactive tool that manages the lifecycle of the console's fonts and configuration: frits (NAME.vtfrit in /etc/vtium/fonts) and /etc/vtium/vtium.conf."
    via: the maintainer's decisions in the steering session, 2026-11-02, relayed by the team lead in its brief for this record; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "Its interactive mode is a TUI run inside a vtium console. While the user browses fonts and sizes, the TUI switches the live console to each choice by sending the same OSC 50 sequence `vtium-vitrify set NAME` sends today (the console honours it under `allow-font-ops`), so the user sees the real console in the real font as they choose."
    via: the maintainer's decisions in the steering session, 2026-11-02, relayed by the team lead in its brief for this record; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "The maintainer tried `vtium-vitrify set monoid` on a VT and found the instant switch felt like setfont; the tool builds on that."
    via: the maintainer's report in the steering session, 2026-11-02, relayed by the team lead in its brief for this record; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "It stays one tool, vtium-vitrify (not vtiumctl)."
    via: the maintainer's decisions in the steering session, 2026-11-02, relayed by the team lead in its brief for this record; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "Its banner tagline, \"vitrified font sets for vtium\" (crates/vtium-vitrify/src/banner.rs:419), is stale since ADR-1200's 2026-10-28 decision moved glyphs to run time; the record names the new tagline's purpose and the build changes it."
    via: the maintainer's decisions in the steering session, 2026-11-02, relayed by the team lead in its brief for this record; the wording is the team lead's
    paraphrase: true
  - upstream: "the C reference's kmscon at b1f7a7c5: the C reference read its font from kmscon.conf's font-name, font-size and font-engine at start (src/terminal.c, font_set, 921-955); while it ran, only the zoom keys changed the font, by its size (zoom_in and zoom_out, 1324-1355); its OSC handler took only setBackground and setForeground (osc_event, 859-870), so it had no OSC 50 and no tool that changed its font or its configuration"
  - standard: "xterm ctlseqs: OSC 50 (set the font by name, and the query `?`), its font-menu forms beginning with `#`, and xterm(1)'s allowFontOps; XTVERSION (CSI > q); primary device attributes, DA1 (CSI c), which every terminal of the VT100 line answers"
  - standard: "fonts-conf(5), fontconfig's font names: a pattern writes a size as `:size=` after the family, as in `Monospace:size=12`"
  - evidence: "the tree at integration 55217dc4, read 2026-11-02: crates/vtium-vitrify/src/switch.rs (`set` writes OSC 50 and its query to /dev/tty in raw mode, waits a second, holds SIGTERM, SIGHUP, SIGINT and SIGQUIT while raw, and checks no terminal before it writes); crates/vtium-term/src/terminal/font.rs (vtium takes a Pt of 1 to 64 frit-name characters and ignores xterm's `#` font-menu forms); crates/vtium/src/app.rs (`font_name` answers the query with the frit's name, or the built-in font the head draws at its height, `unscii-8` or `unifont`); crates/vtium-term/src/terminal/reply.rs (XTVERSION replies `vtium(VERSION)`); crates/vtium/src/config/file.rs (vtium.conf's line syntax, `#` comments anywhere but a color value); crates/vtium/install/vtium.conf.example (each key a `##` description line and a commented `#key=default` line)"
  - evidence: "`vtium check-config --configdir DIR`, built at integration 55217dc4 and run 2026-11-02 on a directory whose vtium.conf held an unknown key and a font-size that is not a number, reported each with its file and line and exited 1"
  - precedent: ADR-1200
  - precedent: ADR-1401
  - precedent: ADR-1402
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "No size suffix on OSC 50. The TUI previews a font at each size as a picture, drawn through kitty graphics, without changing the console's font: the glyph engine renders a specimen of the frit at that size into RGBA, and the TUI shows it with kitty graphics' raw-pixel direct transmission (f=32, t=d), which vtium decodes without the image helper. Live font switching stays OSC 50 with a name only. Where the console draws no kitty graphics, as a build without images, the TUI previews fonts live by OSC 50 and sizes only by saving. preview becomes the same picture renderer. Outside vtium the TUI sends no OSC 50, and shows pictures only where the terminal answers the kitty graphics query."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
    covers: [live sizes]
  - operator: aaronsb
    said: "Live switches take effect at once. When a console's session ends, vtium returns that console to the configured font, font-name and font-size from vtium.conf including anything the TUI saved, so the next login starts with the permanent choices."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "The command names stay: fonts, check, config, remove-frit and apply DIR, beside set, preview, new, and the TUI with no command."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Profiles move from vtiumctl to vtium-vitrify, writing and choosing them. vtiumctl keeps controlling a running console, and heads.conf."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "As proposed: the head's font resets only when the session ends in the console that made the last live switch, or in the head's only console."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
    covers: [consoles that share a head]
  - operator: aaronsb
    said: "Yes: the reset sets font-size too, so it also undoes zoom."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
    covers: [zoom at logout]
  - operator: aaronsb
    said: "vtium-vitrify also assigns a profile to a head, when it runs with rights to write the configuration, in its apply step under sudo. To keep a single writer of heads.conf, vtium-vitrify does not write heads.conf itself: it calls the heads.conf writer that vtiumctl uses; we could also maintain the split by shelling out."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
    covers: [choosing a profile]
  - operator: aaronsb
    said: "Pin D6: the heads.conf writer is a module of vtium-config and stays the one writer for good. vtium-vitrify, vtiumctl and vtium check-config all link it, and no tool shells out to another to edit heads.conf. The only subprocess boundary in this design is the privilege step, sudo vtium-vitrify apply DIR, which keeps the code that runs as root small."
    via: the maintainer, in the steering session, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "vitrify is an unusual word. Users type vtium config, a subcommand of the console binary that hands off to a separate binary, vtium-config, so the TUI's code never enters the console binary; vtium-config also works directly, and every command becomes vtium config COMMAND or vtium-config COMMAND. The binary stays the console's glyph helper, vtium-config serve, at /usr/bin/vtium-config. The library crate vtium-config is renamed vtium-settings. The banner keeps the glass logo and titles the tool VTium Vitrifier; vitrify stays for the glass theme, not for anything a user types. The rename happens now, in one branch, before the TUI is built, and ships in the next release."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Most configuration work happens in the Flow view, and vtium-config opens on Flow. vtium-config keeps three views: Flow, Fonts and Settings. The Heads view goes, since the flow is the heads. The Sizes view goes: size moves into the look card, where Enter on a look offers its sizes, with live preview where it works, keeping the quick try-and-keep loop the Sizes view had. Settings stays as a view, the full list of top-level keys, and the base card in Flow edits the common few. Fonts becomes a font resource manager: the places vtium finds fonts, listed with what each holds; the fonts installed there, with their coverage read only in the sandbox; the frits as files, with new (the guided builder), rename and delete, and a note on which looks use each; and live preview of a font."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "vtium-config manages looks too: a Looks view manages each look, its filters, color and effects included, and looks can then be picked from the look column in the Flow view."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Add a Help view, with Help and About pages; About shows vtium-config's ANSI picture, the same one the command line shows."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "in the web based previewer all the layout gets broken at 40 columns. let's experiment with 100x30 to start and see how it looks."
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "someone could ssh to a very low spec machine and bring their ssh terminal to a large size and use it correctly"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "The frit commands are renamed new-frit and use-frit, and the flow's commands keep set, new and apply."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "or just copy the file in - remember it creates a compatible draft"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "The view's Save becomes Install and vtium config apply becomes vtium config install; install is the one action that writes /etc/vtium/vtium.conf, through sudo, and the privileged step sudo vtium-config apply DIR may keep its name."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "It actually looks like 80x25 will work"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "vtium config KEY becomes vtium config get KEY, the pair of set."
    via: the steering session, relaying the maintainer's answers of 2026-11-05 to the passages work's questions
    paraphrase: true
  - operator: advisor
    said: "The section writer finds what it edits by its lines and never refuses an edit because the file or the result fails the check. It refuses only an edit it cannot place: a missing section or rule, a section named twice, or text it cannot write as one line. The check guards install and check-config."
    via: the steering session's call of 2026-11-05 while building slice C, made under D8's ruling that a draft may be unfinished
    paraphrase: true
  - operator: aaronsb
    said: "vtium-config's TUI previews a change as a picture of the console with the change applied, and leaves the live console alone by default. Highlighting a font, a size, or a look's font, size or cell draws the whole display at its real resolution in that font at that size, with the grid that results, scaled down to fit the picture area, from the same glyph helper vtium draws with. The live console changes only on a key, where live preview worked before, and goes back to its font by itself after a countdown unless the user presses a key that keeps it; keeping it lasts until the TUI leaves, as before. Where no picture can be drawn, the TUI shows a line of text, and the key still works where the console can be switched. The status line says which preview is in use and whether the live trial is available."
    via: the steering session, relaying the maintainer's decision of 2026-11-05 in its brief for this amendment
    paraphrase: true
    covers: [live preview]
  - operator: aaronsb
    said: "Beside the picture of the console, the TUI offers a detailed preview: a full specimen of the highlighted font at the chosen size, with printable ASCII, Latin accents and punctuation, box drawing and block elements, common symbols and arrows, and samples of Greek, Cyrillic, CJK and emoji, each drawn where the font or the frit's later sources have it, with a plain marker where it falls to the built-in font or is missing, so a gap is visible. It extends the specimen vtium-config already draws, and the TUI and `preview` show the same one. A key switches the picture area between the two."
    via: the steering session, relaying the maintainer's addition of 2026-11-05 to this amendment
    paraphrase: true
    covers: [live preview]
  - operator: aaronsb
    said: "Read it at session end"
    via: the steering session's question tool, 2026-11-06, on whether a session's end reads font-line-spacing again with the other font keys; the option wording is the steering session's
  - operator: aaronsb
    said: "do you think those kinds for colors are appropriate? they're bright primary colors, and maybe could use an externalised configuration for vtium-config (something kept in xdg user scope). we're not trying to define a full theme...(yet) but at least, give an option for some color sets. For now it's just defined in the config file and we could set up 3 examples, and one must have the other two configs commented out"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "we should add one more style where the interior of each node(box) has a background color (and an appropriate foreground color for the text)"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "I think the actual display of colors etc is resolved for now. we can add more color schemes as we mature more (such as a colorblind or accessibility high contracst color) in later passes"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "I reviewed the record artifact and I'm happy with the figures and your calls."
    via: chat, 2026-11-10, after reading the record page https://claude.ai/artifact/9umLz4TBBFaZ77eQQC7pW1 (the changed sections of ADR-1403, ADR-1401 and ADR-1201 at f9160eb0, figures included)
  - operator: aaronsb
    said: "there's a problem with looks where viewing the selected font and sizes, re-scales the image in the console to fit a small area. we should allow the image to show 1:1"
    via: the maintainer's report in the steering session, 2026-11-10, relayed by the team lead in its brief for the fix that draws the Looks preview at 1:1
observable:
  - "In a vtium console, moving the TUI's highlight from one frit to the next leaves the console's font as it was and, where the console draws kitty graphics, shows a picture of the TUI's screen in that frit at the grid it gives on the display; `t` switches the console to the frit, and the console goes back to its font after 10 s unless `k` keeps it"
  - "Leaving the TUI without saving, by its quit key, by Ctrl-C, or by SIGTERM sent from another terminal, puts the console back in the font it had when the TUI started, checked by the OSC 50 query"
  - "Run in a terminal that is not vtium, the TUI writes no OSC 50 byte, says the live trial is off and why, and still saves"
  - "Saving a new font-size into the shipped vtium.conf.example changes one line and leaves every other byte of the file as it was"
  - "Saving asks for the sudo password once, for one command the TUI shows first, and a write that fails leaves each file it did not finish exactly as it was"
  - "In a vtium built with `images`, highlighting a size shows a picture of the TUI's screen in the frit at that size and leaves the console's font and grid as they were"
  - "After the TUI switches a console's font and the session in that console ends, the next login prompt draws in the font and size vtium.conf names, a font the TUI saved included"
status: accepted
date: 2026-11-02
deciders:
  - aaronsb
related: []
---

# ADR-1201: vtium config manages the console's fonts and settings, trying each choice on the live console

## Summary

- **Decided:** The tool, until now vtium-vitrify, is renamed: users type `vtium config`, which hands off to the separate binary `vtium-config`, and the configuration library takes the name vtium-settings. vtium-config becomes the one tool that manages the console's fonts and settings for as long as they exist: it lists, checks, previews, switches, writes and removes frits (font set files, `NAME.vtfrit` in `/etc/vtium/fonts`), and reads and edits `/etc/vtium/vtium.conf`. Run as `vtium config` with no command at a terminal, it opens a TUI, a full-screen program drawn in the terminal's text cells. While the user browses, the TUI previews each change, a font, a size or a look's cell, as a picture of the console with the change applied, and the live console's font stays as it was: the glyph engine draws the TUI's own screen in that font at that size, laid out at the grid the change gives on the whole display, and the TUI shows it scaled down through kitty graphics' raw pixels, which vtium decodes in the console. When the TUI runs inside a vtium console with `allow-font-ops` on, a key tries the highlighted font on the live console through OSC 50, the sequence `vtium-config set` sends, and a countdown puts the console's font back unless the user presses the key that keeps it; leaving without installing puts the console back in the font it had at the start. Where the console draws no kitty graphics, as in a build without `images`, the TUI states the cell and the grid in a line of text instead. In a terminal that is not vtium the TUI sends no OSC 50, and shows pictures only where the terminal answers kitty's graphics query. When a console's session ends, vtium returns it to the font and size vtium.conf names, so the next login starts with the installed choices. Every action the TUI offers is also a non-interactive command, and every change, from the TUI or a command, goes into the user's draft of vtium.conf (ADR-1403 D8). Install writes through one command, `apply DIR`: the TUI runs as the user, builds an install bundle from the draft, and runs `sudo vtium-config apply DIR` once. `apply DIR` checks every file, edits vtium.conf line by line so that comments and order stay, and replaces each file in one rename. The TUI is built from rustix, which the tool already links, with no TUI library. Looks, the profiles of ADR-1200 D16, are vtium-config's to write, choose and assign to a head; it writes the head file, now the sections of vtium.conf, through the one section writer vtiumctl also uses (ADR-1403 D3, D8), and the running console's controls stay vtiumctl's. vtium-config takes its colors by role from a file of the person's own, `~/.config/vtium/vtium-config.conf`, which holds four sets, `console`, `muted`, `mono` and `filled`, and `NO_COLOR` wins over it (D13).

> Edited 2026-11-05: ADR-1403 put the configuration in one draft and the head file in vtium.conf (ADR-1403 D3, D8). The Summary read: "the TUI does its writing through one of them, `apply DIR`: the TUI runs as the user, gathers what the user saved into an install bundle, and runs `sudo vtium-config apply DIR` once." and "Profiles are vtium-config's to write, choose and assign to a head; it changes a head's `look =` line through the one head-file writer vtiumctl also uses, and the rest of the head file and the running console's controls stay vtiumctl's."

> Edited 2026-11-05: the maintainer made the picture of the console the default preview and the live console a trial on a key (D4, `considered`, 2026-11-05). The Summary read: "When the TUI runs inside a vtium console with `allow-font-ops` on, each font the user highlights switches the live console to it through OSC 50, the sequence `vtium-config set` sends, so the user judges the real console in the real font; leaving without installing puts the console back in the font it had at the start. Sizes are previewed as pictures: the glyph engine draws a specimen of the frit at each size, and the TUI shows it through kitty graphics' raw pixels, which vtium decodes in the console, without changing the console's font. Where the console draws no kitty graphics, as in a build without `images`, sizes take effect only when installed."
> Edited 2026-11-10: the maintainer asked for a color file with sets to choose from (`considered`, 2026-11-10), and the Summary's last sentence, on that file, was added with D13.

- **Trades away:** vtium-config ships on every install as the glyph helper, so the TUI's code lands on minspec boards too, outside the console's size budget but inside the install tree tier 3 measures. The TUI cannot put the font back after SIGKILL or once its terminal is gone, and the console then keeps the previewed font until that session ends. Sizes are never tried on the live console, and a build without `images` cannot preview them at all. At a session's end vtium reads vtium.conf's font keys again, the one time it reads its configuration after start. Inside tmux or screen, or over ssh from another terminal, there is no live preview.
- **One-way?** No. The tool and its commands change in a session. The command names become expensive to change once scripts and packages call them.
- **Probes:**
  - *Confident (the TUI and the commands):* every choice the TUI offers is also a command a script can run, and the TUI saves through the same command a script uses (`apply`). The earlier decision that vtium supplies a scriptable tool and leaves the wrappers to others stands, and the TUI is the one guided face vtium ships on top of it. Is that the shape you want?
  - *Confident (the head file):* the TUI does not edit `heads.conf`, the file that places displays and keyboards into heads. vtiumctl keeps it, and vtium-config keeps the fonts and vtium.conf. Should the head file stay out?
  - *Not confident (live sizes):* you asked for sizes to switch live as fonts do. Today the console takes only a font's name over OSC 50, so live sizes need the console to take a size too, in a form only vtium reads, such as `monoid:size=12`, the way fontconfig writes a size. Until that lands, the TUI shows what grid each size gives and applies the size only when it is saved. Is a vtium-only form acceptable, or should sizes stay applied on save? (Answered 2026-11-02: no size suffix, sizes as pictures; see `considered`.)
  - *Not confident (consoles that share a head):* you decided that a console goes back to the configured font when its session ends. vtium keeps one font for a whole head, and a head may hold several consoles, each with its own login. So vtium returns the head's font when the session ends in the console that last switched it, or in a head's only console, and leaves it when someone else's session on the same head ends. Is that the rule you want, or should each console keep a font of its own? (Answered 2026-11-02: as proposed; see `considered`.)
  - *Confident (zoom at logout):* the return sets the size from vtium.conf too, so a zoom made with the zoom keys is undone at the same moment, and the next login starts at the saved size. Is that right? (Answered 2026-11-02: yes; see `considered`.)
  - *Confident (choosing a profile):* vtium-config is where a user builds a profile (its frit, its cell and its options), checks it against a display and picks it. Which profile a head uses stays a line in `heads.conf`, which vtiumctl writes. Is that the split you meant? (Answered 2026-11-02: vtium-config also assigns the profile, through vtiumctl's head-file writer; see `considered`.)
- **Inversion:** Between "vtium supplies only non-interactive commands, and any interactive front end is someone else's", ADR-1200's earlier stance, and "an interactive program is the way to configure the console", as a desktop's settings panel is. vtium keeps every action scriptable and ships one interactive front end on top. The one thing the TUI adds, switching the live console, rests on OSC 50, which any program may send.

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). Where the Summary names `vtiumctl`, vtium-config does that work: the section writer is the one vtium-config and `vtium check-config` link, and the running console's controls, `status`, the trial and the overlay, are vtium-config's.

## Context

vtium-config is vtium's font tool, a binary of its own outside the console (ADR-1200 D4, D6). It serves the console as its glyph helper (`serve`, ADR-1200 D15) and switches the running console's frit (`set`, ADR-1200 D5). ADR-1200 plans two more commands, `preview` and the guided builder `new`, and states that each command is non-interactive where it writes. ADR-1401 D12 makes `vtiumctl` the configuration tool for the head file, and has it write and install frits too. The tool was named vtium-vitrify until this record renamed it (D12); the records before this one, and this record's dated notes, use that name.

The maintainer tried `vtium-vitrify set monoid`, the tool's name then, on a VT, and the console changed font at once, as `setfont` changes a Linux console's font. The maintainer decided that vtium-config grows from that into an interactive tool for the console's fonts and its configuration file, whose TUI switches the live console as the user browses.

The C reference had no such tool. It read its font from kmscon.conf at start, and while it ran only the zoom keys changed the font, by its size; it had no OSC 50 (`src/terminal.c` at kmscon `b1f7a7c5`, `font_set`, `zoom_in`, `osc_event`). vtium's OSC 50 and its query already exist, behind `allow-font-ops`, which is on by default.

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). The context above tells what ADR-1401 D12 said when this record was written. ADR-1401 D12 now makes vtium-config the configuration tool for the whole of vtium.conf and the frits.

## Decision

Terms used below: a *frit* is a font set file, `NAME.vtfrit`, in the frits' directory, `/etc/vtium/fonts` unless `font-frits` names another (ADR-1200 D1). The *TUI* is vtium-config's full-screen interactive mode. *Live preview* is the TUI switching the console it runs in to the highlighted choice. An *install bundle* is the directory of files that Install builds from the draft and `apply DIR` installs (ADR-1403 D8). None of this is built yet, apart from `set` and `serve`.

> Edited 2026-11-05: the words follow ADR-1403's fixed words, draft and install (ADR-1403 D7). The Summary said "leaving without saving", "sizes take effect only when saved" and "the saved choices", and an install bundle was "a directory of files the user saved but that are not installed yet".

### D1: what the tool manages

- **Frits.** vtium-config lists the frits and the built-in fonts, checks a frit as the helper reads it, previews it, switches the console to it, writes a new or changed frit, and removes one.
- **vtium.conf.** vtium-config shows each key of `/etc/vtium/vtium.conf` with its value, its default and its description, and changes keys. The font keys come first (`font-name`, `font-size`, `font-frits`, `font-hinting`, `font-line-spacing`, `allow-font-ops`, `display-dpi` and the sprite policy's keys), and every other key is listed after them. A key's description, type and bounds come from vtium's own key table, `crates/vtium/src/config/keys.rs`, which moves into vtium-settings, the configuration crate vtium already links (ADR-1401 D12), so the console and the tool read one table.
- **The head file,** the sections of `/etc/vtium/vtium.conf` (ADR-1403 D3): the Flow view and the commands of ADR-1403 D8 write its rules, heads and looks through the section writer (D6).
- **Looks,** the profiles of ADR-1200 D16 under their new name, are `[look NAME]` sections of vtium.conf that hold a frit, a size and font options (ADR-1403 D1, D5), and are vtium-config's to write and to choose (the maintainer, 2026-11-02): the Looks view is where a user builds a look, checks it against a display (the rule checker, ADR-1200 D4) and picks among the looks (D3). A frit's `cell=` line carries the cell, and the TUI edits it as part of the frit. A look's `cell` wins over its frit's `cell=` line, and within one look `font-size` wins over `cell` (ADR-1200 D16, ADR-1403 D5). vtium-config also controls a running console, through `vtium config status`, the trial and the overlay, and detects devices (ADR-1401 D12).
- **Assigning a look to a head** (the maintainer, 2026-11-02). The Flow view's head cards show the heads, read with vtium-settings's parser, and the user picks the look each head draws with (D3; ADR-1403 D7). The change is a head's `look =` line, as in `[head desk]` with `look = desk`, which the section writer writes into the draft (D6), and Install installs (D5).

> Edited 2026-11-02: the maintainer had vtium-vitrify also assign a profile to a head, through the head-file writer vtiumctl uses (`considered`). The head-file bullet read: "Not the head file, `/etc/vtium/heads.conf`, which stays vtiumctl's (D6).", and the profiles bullet ended "and the head file, in which a head names its profile (ADR-1401 D12)."

> Edited 2026-11-02: the maintainer moved profiles from vtiumctl to vtium-vitrify, writing and choosing them (`considered`). The bullet read: "Profiles, once they have a file of their own (ADR-1200 D16), are a frit, a cell and font options, and are vtium-vitrify's to write.", and the record left the move to the maintainer's check.

> Edited 2026-11-04: the Heads view went into the Flow view's head cards (D3; `considered`, 2026-11-04). The bullet on assigning a profile read: "The TUI lists the heads `heads.conf` declares, read with vtium-settings's parser,".

> Edited 2026-11-05: ADR-1403 moved the head file into vtium.conf, where vtium-config writes its rules, heads and looks, and renamed the profile the look (ADR-1403 D1, D3, D8). The bullets read: "**One line of the head file,** `/etc/vtium/heads.conf`: a head's `look =` line, which assigns the head its profile, written through the head-file writer vtiumctl uses (D6). The rest of the head file stays vtiumctl's."; "**Profiles** are a frit, a cell and font options [...]. Until profiles have a file of their own, a frit's `cell=` line carries the cell [...]. vtiumctl keeps controlling a running console (`status`, the trial and the overlay) and the head file (ADR-1401 D12)." The bullets also read "**Assigning a profile to a head** [...] and the user picks the look each head draws with, a profile among them [...] and it is written as D6 says, in the apply step under sudo. A head names a profile only once profiles have a file of their own, so the assignment is built with them." The earlier text is in git at e53919ea.

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). Detecting devices and controlling a running console, `status`, the trial and the overlay, are vtium-config's, where the looks bullet names `vtiumctl`.

> Edited 2026-11-09: the 2026-11-07 decision left the looks bullet naming `vtiumctl`, and ADR-1401 D12 now settles how vtium-config controls a running console. The bullet ended: "vtiumctl keeps controlling a running console (`status`, the trial and the overlay), and detecting devices (ADR-1401 D12)."

### D2: one tool, and its commands

vtium-config stays one binary, apart from vtiumctl. Each command is typed as `vtium config COMMAND`, or as `vtium-config COMMAND` (D12). Every action the TUI offers is also one of these commands, and each command but the TUI is non-interactive, prints one line a result, and exits 0 on success, 1 on a failure and 2 on a usage error, as ADR-1200 D4 has it:

| Command | Runs as | Does |
|---|---|---|
| none (`vtium config`), at a terminal | the user | Opens the TUI. With no command and stdin or stdout not a terminal, it says that no command was given and exits 2, as it does today. |
| `fonts` | any user | Lists the frits and the built-in fonts, one line each, tab-separated: the name, whether the frit reads, and the sources this machine lacks, found through the sandboxed child (D3). |
| `check FILE` | any user | Checks a frit with the helper's frit reader, or a vtium.conf with `vtium check-config`, the console's own reader (D5). Writes nothing. |
| `get KEY` | any user | Prints a top-level key's value in the draft, and its default when the draft leaves it unset; `set KEY VALUE` and `set --unset KEY` change it (ADR-1403 D8). |
| `new-frit ...` | the user; root to install | The guided builder (ADR-1200 D4). Its non-interactive forms, `--template`, `--from FILE` and its flags, stay; at a terminal with no such input it opens the TUI at its new-frit view. |
| `remove-frit NAME` | the user; root to install | Removes a frit, refusing one that vtium.conf's `font-name` names. |
| `apply DIR` | root | Checks and installs an install bundle (D5). |
| `preview`, `use-frit`, `serve` | as ADR-1200 D4 | Unchanged but for `use-frit`'s name, and `use-frit` checks the terminal first (D4). |
| `flow`, `route`, `diff`, `install`, `set`, `get`, `revert`, `new`, `rule`, `head`, `look`, `draft`, `history`, `restore`, `library` and `edit` | the user; sudo to install | The flow's commands, with the meanings and the exit codes 0 to 5 that ADR-1403 D8 gives them. |

- **Every command that writes changes the draft,** the draft of ADR-1403 D8 under the user's XDG path, root's included, and installs nothing. `install` builds an install bundle from the draft under `$XDG_STATE_HOME/vtium/drafts/<time>/`, the directory ADR-1401 D12 gives vtiumctl's drafts, whose name predates the word install bundle, and installs it through `sudo vtium-config apply DIR`, or run as root through the same steps without sudo. A script therefore never needs the TUI.
- **The frit commands are `new-frit` and `use-frit`** (the maintainer, 2026-11-05), since the flow's commands of ADR-1403 D8 keep `set` and `new`. `install` installs the current draft, which it hands to `apply DIR`, the privileged step that runs as root and keeps its name (the maintainer, 2026-11-05).

  > Edited 2026-11-05: the maintainer renamed the flow's `apply` to `install` and the view's Save to Install (`considered`). The bullet read: "keep `set`, `new` and `apply`. `apply` with no argument installs the current draft".
- The names `fonts` and `remove-frit` differ from the retired store's `list` and `remove` (ADR-1200 D3), which stay unknown commands, so an old command never does a new thing.

> Edited 2026-11-05: the maintainer renamed the frit commands `new-frit` and `use-frit` (`considered`). They were `new` and `set`.

> Edited 2026-11-05: ADR-1403 added the flow's commands and made every write go into the draft (ADR-1403 D8). The table had no row for them, `apply DIR` checked "a draft", and the bullet read: "**Run as a user,** a command that writes makes an install bundle under `$XDG_STATE_HOME/vtium/drafts/<time>/`, [...] and prints the install bundle's path and the command that installs it, `sudo vtium-config apply DIR`. **Run as root,** it installs at once through the same steps `apply DIR` takes."

> Edited 2026-11-05: the maintainer renamed the command that reads a key `get`, the pair of `set` (`considered`). The row read: "| `config KEY`, `config KEY VALUE`, `config --unset KEY` | the user; root to install | Prints a key's value; with a value or `--unset`, makes that one change (D5). |"

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). vtium-config is the one configuration binary, and no `vtiumctl` stands beside it.

### D3: the TUI

- **Five views, and it opens on Flow** (the maintainer, 2026-11-04 and 2026-11-05). Most configuration work happens in the Flow view of ADR-1403 D7, so vtium-config opens there. Tab steps through the views in this order:
  - **Flow** shows the configuration as paths, each a terminal with its displays, inputs and font set, and builds them from parts (ADR-1403 D7). A path's card sets its terminal's font set and size, which vtium-config writes as the head's look (ADR-1403 D8). Flow has no card for the top-level keys, since the Settings view edits them, and a part's detail lists the rules that name the part, for each VT (ADR-1403 D7).

  > Edited 2026-11-10: ADR-1403 D7 rebuilt the Flow view around paths. The bullet read: "**Flow** draws the configuration from the displays to the input devices and edits it (ADR-1403 D7). Enter on a head's look card picks from the looks the Looks view defines, and `new look…` opens the Looks view's editor on a new look. Its head cards assign each head its look. Its base card edits the common top-level keys."
  - **Looks** manages the looks, each one object, a `[look NAME]` section of vtium.conf: its frit and size, and `vtlook =`, which names the vtlook of ADR-1304 that holds the filters, color and effects (ADR-1403 D1). It creates, edits, renames and deletes a look, each change previewed as a picture of the console (D4). Its sizes are where the Sizes view went: a look offers its sizes, in points as `font-size` takes them, with automatic first, and previews each as a picture of the console at that size, so the user tries a size and keeps it as quickly as before. Until ADR-1304's filters are built, the Looks view manages the profile part and shows the filters as not yet available. Deleting a look is confirmed, and the confirmation names the heads that use it (ADR-1403 D8).
  - **Fonts** manages the font resources. It lists the places vtium finds fonts, each with what it holds: the folders of `font-paths` (ADR-1200 D25), kbd's console-font folders, vconsole's font and the built-in fonts. It lists the fonts installed in those places, with each font's coverage, which vtium-config reads only in the sandbox (below). It lists the frits as files, with how each reads, and offers new, which is `new-frit`'s guided flow (D9), rename and delete, each frit noting which looks use it. It previews a font as a picture of the console, and `t` tries it on the live console where that works (D4). It adds, removes and orders the folders of `font-paths`, which it writes into the draft like any key.
  - **Settings** lists every top-level key of vtium.conf (D1), and edits each one.
  - **Help** stays on its own tab (the maintainer, 2026-11-05). Help holds two pages. Help lists the keys and what each view does, from the catalog the command line's help draws on (`crates/vtium-config/src/help.rs`). About shows what `vtium-config --help about` prints, the tool's name, version and logo, drawn from that one source so the two never drift; where the logo is wider than the terminal, it shows the narrow banner (D10).
  The Heads view goes, since the flow is the heads, and the Sizes view goes into the Looks view. The save view goes too. Each view's tab bar shows its own actions with their keys: the Flow view's are View file, Save, Install, Revert and New, with Restore on `o` (edited 2026-11-10: they read "View, Save, Install, Revert, Restore and New", ADR-1403 D7); the Looks, Fonts and Settings views' are Save and Install; and Help has none. Save writes the TUI's unsaved changes into the draft, and Install saves first. The view has no Delete draft, and `vtium config draft rm` deletes a draft (ADR-1403 D7, D8).

  > Edited 2026-11-10: the maintainer dropped the Flow view's base card and route card (ADR-1403 D7 and its `considered`, 2026-11-10: "Drop both (Recommended)"). The Flow bullet ended: "Whether Flow keeps a card for the common top-level keys is open (ADR-1403 D7)." The Settings bullet ended: "the full list of which the base card in Flow edits a common few." The Help bullet read: "stays on its own tab; in the Flow view `?` opens the route card of ADR-1403 D7, not help (the maintainer, 2026-11-05)."

  > Edited 2026-11-09: ADR-1403 set the TUI's actions per view, added Save and took Delete draft off the screen (ADR-1403 D7, D8). The sentence read: "The save view goes too: Install, with View, Revert, Restore, New and Delete draft, is an action in the tab bar of every view (ADR-1403 D7, D8)."
- **Where it runs.** Inside a vtium console, where the live trial works, and in any other terminal, where it does not (D4). It needs a terminal of at least 80 by 25 cells (the maintainer, 2026-11-05). At 80 by 25 the view cuts a long name in the middle with `…`, a device's name to about 10 characters, and selectors and captions likewise; a rule may take two rows; and the key line shows the keys that fit. The minimum applies to the terminal vtium-config runs in, not to the machine: a minspec board with a 40-column console is configured over ssh from a terminal at 80 by 25 or larger, without the live trial, with the commands at any size, or with a draft built on another machine and copied over, since a draft is a complete vtium.conf (ADR-1403 D8). Below the minimum, as when a previewed font leaves the grid too small, the TUI draws no view: it shows one screen with the size it needs, the `vtium config` commands, and the keys to leave and to undo the font preview, so a user can always get out.

  > Edited 2026-11-05: the maintainer raised the floor to 100 by 30, provisionally, after the layout broke at 40 columns in the previewer (`considered`). The bullet read: "It fits a grid of 40 by 25 cells. When a previewed font leaves a grid smaller than that, the TUI draws one line naming the font and the keys that keep it or go back, so a user can always get out."

  > Edited 2026-11-05: after looking at the mockup at 80 by 25 and 100 by 30, the maintainer set the minimum to 80 by 25 and dropped provisional (`considered`). It read "at least 100 by 30 cells, a provisional number the maintainer judges from the previewer and may change".

  > Edited 2026-11-03: the maintainer set the floor to 40 by 25 cells. The record first named the 320x240 floor's grid of 40 by 30 cells, as ADR-1401 D12 asks of vtiumctl's dialog, but the VGA console's usual grid is 80 by 25, which D8 captures, and a 30-row floor would have given that console the one-line form.
- **How it draws.** It switches to the alternate screen (DEC mode 1049), so the shell's screen comes back when it leaves, hides the cursor, and redraws whole on each change and on SIGWINCH, which the console sends when a font switch changes the grid (ADR-1200 D17).
- **Fonts are read only in the sandbox.** The TUI and the commands read frits as text, as the console does. They parse a font file, even to read the family name inside it that a `font:` source matches, only through the sandboxed child that `preview` and `new` use (ADR-1200 D14), never in their own process, so listing installed fonts or a font's coverage puts no font parser in a process with the user's rights and a terminal.
- **Its words** live in a catalog module of the crate's own, as the code rules ask, and are English, as ADR-1402 keeps what vtium's tools print in a terminal.

An example of the Fonts view's content, narrower than the view draws it; its layout is the build's to settle and stays provisional until the maintainer reviews it:

```text
 Flow Looks Fonts Settings Help
 Frits
 > minspec-16  reads       looks: small
   proof       1 missing   looks: none
   tiny5       reads       looks: big
 Built in      unifont, unscii-8
 Places        4 folders, 212 fonts
 ---------------------------------------
 Enter open  n new  r rename  d delete
```

> Edited 2026-11-05: with D4's amendment of the same day, the Looks view's sizes, which read "tries each on the live console", and its looks, which had "live preview where it works", are previewed as pictures of the console, and the Fonts view, which "previews a font live", previews a font as a picture and tries it live on `t`; "Where it runs" named live preview where it now names the live trial.

> Edited 2026-11-05: `?` in the Flow view opens the route card of ADR-1403 D7, and help stays on the Help tab (`considered`); the Help bullet named no key.

> Edited 2026-11-04 and 2026-11-05: the maintainer reduced vtium-config to Flow, Fonts and Settings, opening on Flow, made Fonts a font resource manager, added a Looks view that manages looks with their filters, and added a Help view with Help and About pages (`considered`, 2026-11-04 and 2026-11-05). The bullet read: "**What it shows.** A list of the frits and the built-in fonts, each with its state; the sizes for the highlighted font, in points as `font-size` takes them, with automatic first; vtium.conf's keys (D1); the new-frit view, which is `new`'s guided flow; and a save view, which lists each file the save changes and each line of vtium.conf that changes, with the one command it runs." The example showed the font view with a size line. The earlier text is in git at 9f4fd304.

### D4: pictures of the console, the live trial, and giving the console back

> Edited 2026-11-05: the maintainer decided that the TUI previews a change as a picture of the console with the change applied and leaves the live console alone, and that the live console changes only on a key, for a trial that reverts by itself (`considered`, 2026-11-05). A font that left the grid too small or unreadable could strand a user while the highlight switched the live console. The heading read "D4: live preview, and giving the console back". The record's title still says "trying each choice on the live console"; the default is now the picture, and the live console is tried only on the trial key.

- **Finding out what the terminal is.** At start the TUI writes four requests at once: XTVERSION (`CSI > q`), the OSC 50 query (`OSC 50 ; ? ST`), kitty's graphics query (`APC G i=N,s=1,v=1,a=q,t=d,f=24;AAAA ST`, a one-pixel image the terminal checks and does not keep) and DA1 (`CSI c`). Every terminal of the VT100 line answers DA1, and terminals answer requests in the order they arrive, so a reply that has not come before DA1's will not come. The TUI waits for DA1's reply at most one second, `set`'s wait, for a terminal that answers nothing.
  - XTVERSION names vtium (`vtium(VERSION)`) and the OSC 50 query is answered: the live trial works, and the query's answer is the font to give back.
  - XTVERSION names vtium and the OSC 50 query is not answered: `allow-font-ops` is off. The live trial does not work. When the person presses `t`, the TUI says so, names the key, and offers to turn it on as a change saved like any other.
  - Anything else, another terminal, a multiplexer such as tmux, an ssh client's terminal or a serial line: the TUI never sends OSC 50, since xterm, for one, would look for an X font of that name. It says that the live trial is off only when the person presses `t` (ADR-1403 D7).
  > Edited 2026-11-09: ADR-1403 D7 shows the notice that the live trial is off only when the person presses `t`, not on every screen. The first bullet read: "The live trial does not work; the TUI says so, names the key, and offers to turn it on as a change saved like any other.", and the second ended at "an X font of that name."
  - Separately, the kitty graphics query answered before DA1 means the terminal draws kitty graphics, and the TUI previews each change as a picture of the console. vtium answers it only when built with `images` (ADR-1100), which the packaged build is not today. With no answer, the TUI shows a line of text instead: the cell, and the grid the change gives on the display.
  - The status line says which, at its right: `picture` or `text`, then `t: live` where the trial key works and `no live` where it does not, with the reason on the message line at start (the agent's words, short enough to leave the draft's state room at 80 columns).
  > Edited 2026-11-05: with the maintainer's decision above, the first two cases read "fonts are previewed live" and "Fonts are not previewed live", the fourth read "the TUI previews sizes, and fonts it cannot switch to live, as pictures" and its text summary gave "the coverage, the cell and the grid each size gives", and the status line said `live preview: on` or `live preview: off`.
  - `set` makes the same check before it writes, and sends nothing to a terminal that is not vtium. Today it writes OSC 50 to any terminal and only reads the reply.
- **While the user browses: a picture of the console** (the maintainer, 2026-11-05). When the user highlights a font in the Fonts view, a size in the Looks view, or a frit, a size or a cell on a look's card in the Flow view, the TUI draws, in its picture area, the display as it would look with that change, and sends the live console nothing. The picture is the whole display at its resolution, the first connected display's first mode as vtium-config reads it from sysfs, or 640 by 480 where it finds none, drawn in that font at that size. The grid that results, columns by rows, is the display's pixels divided by the font's cell at that size, as the console divides them, and a line beside the picture states the display, the cell and the grid. The picture shows the console one pixel to one pixel: where the picture area is smaller than the display, it shows the part of the display that fits, from its top left. It is sent and deleted as the next bullet says. In the Flow view the picture takes the place of the columns while the card's cursor is on one of those three fields.
  - *What the picture shows* (the agent's call). The TUI's own screen as it is at that moment, laid out again at the new grid. The TUI's screen is a function of its state and its grid alone (D7), so it can lay itself out at any grid, and the user sees the screen they would face after the change, the screen that says the grid is too small included, where the change leaves fewer than 80 by 25 cells. A fixed sample screen was the other choice; it would show the font but not what the change does to the TUI itself.
  - *How it is drawn* (the agent's call). The glyph helper (ADR-1200 D14, D15) draws every character on that screen at the font's cell, as it draws them for the console, and the built-in font draws what the frit lacks, as the console does (ADR-1200 D7). vtium-config's picture renderer, the one `preview` uses (D9), lays the glyphs out cell by cell, each cell in its text and background color from the palette the draft's `palette` key names, through the console's own palette table, one pixel of the console to one pixel of the picture. Where the picture area is smaller than the screen, the picture is the part of the screen that fits the area, from the screen's top left, which is where the Looks view's sample content sits; nothing is averaged or scaled, and the picture is at most 1024 by 768 pixels. Where the area cannot show one line of text at the font's cell, because it is less than the cell high or across, the TUI shows no picture and says so on the message line. It does not run vtium-render, the console's renderer, which would put the renderer into the glyph helper's binary on every install, so it draws no underline, cursor or other decoration. The helper draws on a thread of its own, one preview at a time, so the TUI goes on reading keys while it draws; a highlight that moves meanwhile is drawn once the preview in progress comes back, and a preview of a change no longer highlighted is not shown.
  > Edited 2026-11-10: the maintainer reported "there's a problem with looks where viewing the selected font and sizes, re-scales the image in the console to fit a small area. we should allow the image to show 1:1" (`considered`, 2026-11-10). The picture of the console is drawn one pixel to one pixel and cut from the top left to the picture area, in the Looks view and the Fonts view alike. This entry read "and averages the pixels down to the picture's size", and the bullet above read "The picture is scaled down to fit the picture area, and sent and deleted as the next bullet says."
  - *A look's cell and an automatic size* (the agent's call). A look's `cell` sets the cell when the look's size is automatic, as vtium.conf's `cell` key says. Where the size is automatic, the Fonts view and a look card draw the picture at 12 points, as `preview` does, and the line beside it says so; the Looks view's automatic row has no picture, since the console chooses that size from the display and the font's strikes.
- **The detailed specimen** (the maintainer, 2026-11-05). The Fonts view's picture area shows the detailed specimen of the highlighted font at the draft's size, and `s` (the agent's key) switches it to the picture of the console and back; the steering session chose the Fonts view, and the maintainer asked for the specimen first.
  - *How the picture holds the grid and the specimen* (the agent's call). The specimen is drawn at its real pixel size, one pixel of the picture to one pixel of the console, so the font reads as it would on the console; it shows as many whole lines and columns as the picture area holds. The grid is in words on the line beside it: the cell, the columns and rows that cell gives on the display, and how many of the specimen's characters the font draws itself, the count the marks below show. The picture of the console, which `s` brings up, shows the grid as a picture. The picture of the console shows how the console would look; the specimen shows what the font covers. Its lines hold printable ASCII, Latin letters with accents and punctuation, box drawing and block elements, common symbols and arrows, Powerline's symbols, and samples of Greek, Cyrillic, CJK and emoji, then the locale's line where it has one, each line at most 40 cells (the agent's choice of characters). The helper draws them as it draws the console's glyphs, and each character shows where it comes from (the agent's marks): a character the frit's sources draw is in the ink, one that falls to the built-in font is grey, and one that nothing draws is an empty grey box as wide as the character, so a gap is visible. The line beside it says what the marks mean. It is the same specimen `preview` prints (D9), drawn by the same code, and where the terminal draws no kitty graphics the TUI shows the line of text instead, as for the picture of the console.
- **The live trial, on a key** (the maintainer, 2026-11-05). `t` (the agent's key) switches the live console to the highlighted font, in the Fonts view or on a look card's frit field, by sending `OSC 50 ; NAME ST` followed by the query, the bytes `set` sends. It works only where the live console can be switched: a vtium console that answered the OSC 50 query. The message line then counts down, "Trying NAME on the console for N s: press k to keep it, any other key puts it back", from 10 s (the agent's length), counted as ten one-second waits of the TUI's loop. `k` (the agent's key) keeps the font. Any other key, or the countdown's end, switches the console back to the font it drew before the trial, as `u` on the small screen does (D3), and the key does nothing else. Ctrl-C, and the signals that **Giving the console back** names, leave the TUI, which gives the start font back. A kept font stays on the live console until the TUI leaves, which gives the start font back, unless Install made it the configured font. At most one switch is outstanding: a switch is sent only once the query's reply to the last has come, and a revert asked for while the trial's switch is in flight is sent when that reply comes. A reply that names another font means the console refused the switch; the TUI marks that font "kept by the console, see its log" and ends the trial.
  > Edited 2026-11-05: the bullet read: "**While the user browses.** Each font the user highlights is sent as `OSC 50 ; NAME ST` followed by the query, the bytes `set` sends. At most one switch is outstanding: the next is sent when the query's reply to the last comes back, and it names whatever is highlighted then, so holding an arrow key sends one switch per reply and never a queue of stale ones. A reply that names another font means the console refused the switch; the TUI marks that font 'kept by the console, see its log' and goes on. There is no countdown that reverts a font the user has not confirmed: the TUI stays in control, and leaving it gives the font back."
- **Sizes, and how a picture is sent.** The live trial stays OSC 50 with a frit's name alone, and the TUI never changes the console's size. A size is previewed only as a picture of the console at that size. The glyph engine runs in the sandboxed child, never in the TUI's process. The pixel height comes from the density the console uses, in ADR-1200 D17's order: vtium.conf's `display-dpi` where it is set, else the density the TUI reads from the displays' EDID in sysfs, else 96, and the TUI says which it used. The TUI sends the picture with kitty graphics' direct transmission of raw pixels (`a=T`, `f=32`, `t=d`, its width and height in `s` and `v`, the base64 split into chunks with `m=1`), which vtium decodes in the console without the image helper (ADR-1100 D7), shows it in its preview area at its own pixel size, replaces it once the picture of the next highlight comes, and deletes it (`a=d`) when the view has nothing to preview and when the TUI leaves (the agent's call: deleting it as the highlight moves would blank the area at every step of a held arrow key). Where the terminal draws no kitty graphics, a size takes effect only when it is saved.
  > Edited 2026-11-05: in the fix round after the first review, the maintainer asked for the detailed specimen first in the Fonts view, readable at its real size; the specimen bullet had `s` switch from the picture of the console to the specimen. The bullet read "deletes it (`a=d`) when the highlight moves", which the build never did; the agent chose to replace the picture once the next one comes.
  > Edited 2026-11-05: the bullet's picture was a specimen: "For each size of the highlighted frit, the glyph engine the helper runs (ADR-1200 D14, D15) draws a specimen, a few lines of Latin, box drawing and the locale's script, at the pixel height that size gives, into an RGBA picture." With the maintainer's decision of 2026-11-05 it is the picture of the console the bullet above describes.
  > Edited 2026-11-02: the maintainer answered the live-sizes probe (`considered`): no size suffix on OSC 50, and sizes are previewed as pictures. The bullet recommended that the console take `NAME:size=POINTS` over OSC 50 and report the size in its query, and had the TUI apply sizes only when saved until then.
- **Giving the console back.** Leaving without saving switches the console back to the font the start query named, and then asks the query to check it. The TUI does this on its quit key; on Ctrl-C, which arrives as a byte in raw mode; on SIGINT, SIGTERM, SIGQUIT and SIGHUP, which it holds while the terminal is raw, as `set` does, and handles in its loop; and on a panic, from the guard that puts the terminal's mode back. It cannot after SIGKILL, or once its terminal is gone: the console then keeps the tried font until that console's session ends, when vtium returns it to the configured font (D11). One case the name alone cannot give back exactly: when the console draws a built-in font it chose by height, the query answers `unscii-8` or `unifont`, and switching back to that name fixes that built-in font, so a later zoom across 16 pixels does not change it as the automatic choice would. The session's end clears it (D11).
- **After saving,** the console stays in the saved font, which is the font the saved vtium.conf names for the next start.

### D5: writing

> Edited 2026-11-05: the directory of files `apply DIR` takes is called an install bundle, since the word draft now means the working copy of ADR-1403 D8 alone; this record called it a draft until then (`considered`, 2026-11-05).

> Edited 2026-11-04 and 2026-11-05: the save view went with the reduction of the views (D3; `considered`, 2026-11-04). Install is an action in the tab bar of every view, beside View, Revert, Restore, New and Delete draft, and what it installs is the draft of ADR-1403 D8, through `apply DIR`. The steps below are what `apply DIR` runs.

- **Sudo once, at the moment of writing.** The TUI runs as the user from start to end and never as root, so no font is parsed and nothing is written into the user's home as root. When the user installs, it builds the install bundle from the draft and its base (ADR-1403 D8), shows the command, `sudo vtium-config apply DIR`, and runs it. sudo asks for the password once, in the TUI's terminal. The TUI leaves its alternate screen while sudo asks, and comes back after. Nothing else in the TUI needs privilege.
- **Every change goes into the draft, and only Install must pass the check.** Each change from a command, and each change the TUI saves, is written to the draft whatever the check says. The TUI runs `vtium check-config` on the draft with its unsaved changes after each change and shows what fails, and Install, and `apply DIR` after it, refuse a draft that fails. The one exception is a draft whose only problem is a missing complete route (ADR-1403 D3): Install asks first, `vtium config install --force` asks nothing, the install bundle's `MANIFEST` records the confirmation or the `--force`, and `apply DIR` then accepts that one problem and refuses every other (ADR-1403 D7, D8).

> Edited 2026-11-09: ADR-1403 added Save to the TUI and let Install install a draft with no complete route after a confirmation or `--force` (ADR-1403 D7, D8). The bullet read: "Each change from the TUI or a command is written to the draft whatever the check says. The TUI runs `vtium check-config` on the draft after each change and shows what fails, and Install, and `apply DIR` after it, refuse a draft that fails (ADR-1403 D7, D8)."
- **What an install bundle holds.** Each frit to write, each frit to remove, and for vtium.conf the whole edited file, with the SHA-256 of each file it replaces as that file was when the install bundle was made.
- **What `apply DIR` does,** as root:
  1. It reads the install bundle without following a link, takes only regular files, and bounds each file's size, since root is reading a directory the user owns.
  2. It refuses the install bundle, writing nothing, when a file it replaces has changed since the install bundle was made, naming the file, so an edit made meanwhile by hand or by another tool is never lost.
  3. It checks every file before it writes any: each frit with the helper's frit reader, strictly, as the helper reads it; and the edited vtium.conf with `vtium check-config FILE`, the console's own reader, one run for its top-level keys and its sections (ADR-1403 D3), so the verdict before writing is the verdict at the next start, as ADR-1401 D12 has for the head file. A file that fails stops the apply before anything is written, with each problem and its line. A missing complete route in vtium.conf does not stop it when the `MANIFEST` records the person's confirmation or `--force` (above; edited 2026-11-09, ADR-1403 D8).
  4. It writes the frits first and vtium.conf last, so vtium.conf never names a frit that is not yet there. Each file is written to a new file in the same directory, owned by root with mode 0644, flushed to disk, and renamed over the old one, and the old one is kept beside it with a `.bak` suffix, one generation, as vtiumctl's `install` keeps what it replaces.
  5. It prints one line a file, `wrote PATH` or `failed PATH` with the reason, and exits 0 when it wrote them all.
- **What a failed write leaves.** A file whose write fails before its rename is left exactly as it was, and the new file is removed. A file renamed before a later file failed stays written, and `apply DIR` names which were written and which were not. No file is ever left half written. The install bundle stays, so the user can run `apply` again.
- **Editing vtium.conf's top level line by line.** The line editor changes only top-level lines, the lines above the first section heading, and only the lines it must, and every other byte, comments, blank lines, order and line endings, stays as it was (ADR-1403 D8). The section writer of D6 edits the sections:
  - A key that is set has the value on the top-level line vtium takes it from replaced in place, never a section's line of the same name, with any comment after it kept.
  - A key that is not set but has a commented top-level line, as `#font-size=0` in the shipped example, gets its new line directly below that comment, beside its description.
  - Any other new key goes after the last top-level line, never at the end of the file.
  - `--unset` comments out the line vtium takes the value from, so the key falls back to its default and the line is there to set again.
  - A switch is written as vtium.conf writes switches, `allow-font-ops` or `no-allow-font-ops`.

> Edited 2026-11-05: ADR-1403 made the draft the one place every change goes, moved the head file into vtium.conf, and kept the line editor to the top level (ADR-1403 D3, D8). The steps were headed "What `install` does" and read "the draft" for the install bundle; an install bundle held "for vtium.conf and `heads.conf` the whole edited file"; step 3 checked "the edited `heads.conf` with `vtium check-config FILE`, which checks a head file before it is installed (ADR-1401 D7); and the edited vtium.conf with `vtium check-config --configdir` naming the install bundle's directory"; step 4 wrote "the frits first, then `heads.conf`, and vtium.conf last"; the first bullet read "When the user saves, it writes the install bundle"; and the line editor's bullet was headed "Editing vtium.conf line by line" and added "Any other new key [...] at the end of the file". D5 had no bullet on the check every change goes through. The earlier text is in git at e53919ea.

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). The install bundles live under `$XDG_STATE_HOME/vtium/drafts/`, which ADR-1401 D12 now gives vtium-config's install bundles, and `apply` keeps the file it replaces as `NAME.bak` on its own account, with no `vtiumctl` `install` to match.

### D6: the head file has one writer

- **The head file is the sections of vtium.conf** (ADR-1403 D3), and vtium-config writes its rules, heads and looks: the Flow and Looks views and the commands of ADR-1403 D8. vtiumctl keeps detecting devices, `setup` and the trial (ADR-1401 D12), and its writes go through the same writer and the same check (ADR-1403 D8).
- **One writer, a library.** vtium-config never writes section text of its own. The section writer is a module of vtium-settings, the configuration crate that already holds the head file's parser and evaluator, and it stays the one writer for good (the maintainer, 2026-11-02). vtium-config, vtiumctl and `vtium check-config` all link it, and no tool runs another to edit the file. It finds what it edits by its lines, changes only the lines it needs and keeps every other byte, and it never refuses an edit because the file or the result fails the check, since a draft may be unfinished (ADR-1403 D8). It refuses only an edit it cannot place: a section or rule that is missing, a section named twice, or text it cannot write as one line. The check guards install and `vtium check-config`. It reads vtium.conf through the same split into a top level and sections as D5's line editor, so the two writers agree where the top level ends (ADR-1403 D8).

> Edited 2026-11-05: the steering session made the writer edit any draft, as ADR-1403 D8's unfinished draft needs (`considered`). The sentence read: "It edits only a file vtium reads whole, changes only the lines it needs, keeps every other byte, and parses its result before handing it back."
- **Why a library.** The three tools are Rust crates in one workspace, built and shipped together. Linking one module gives them one parser, one check, one rule for editing without losing comments, and one wording of each error, fixed when they are compiled. Nothing parses another program's text output, a development build and a packaged one cannot disagree about the file, and vtium-config does not need vtiumctl to be built. A shared piece that shows a change and asks to confirm it keeps the two tools' screens alike.
- **The one process boundary** in this design is the privilege step, `sudo vtium-config apply DIR` (D5), which keeps the code that runs as root small.
- **Checked before it is written.** The TUI writes each change into the draft as the user, Install builds the install bundle from the draft, and `apply DIR`, under sudo, checks vtium.conf with `vtium check-config FILE` before it writes it, as it checks every file (D5). A look that names its own font wins over vtium.conf's top-level `font-name` for the heads that name it (ADR-1200 D16), so Install's question also says which heads a top-level font change does not reach.

> Edited 2026-11-02: the maintainer pinned the head file's writer to a module of vtium-config for good, linked by every tool, with no tool running another to edit the file (`considered`). The bullet said that vtiumctl, once built, would link the module, and that whether vtium-vitrify kept linking it or ran `vtiumctl` was the vtiumctl work's choice.

> Edited 2026-11-05: ADR-1403 moved the head file into vtium.conf, where vtium-config writes its rules, heads and looks through the section writer (ADR-1403 D3, D8). D6 began: "**vtiumctl keeps the head file and the devices:** detecting them, setting up heads, rules, policy, the trial, and installing the head file (ADR-1401 D12). vtium-config changes one thing in it, a head's `look =` line (D1)." The writer's bullet read "vtium-config never writes `heads.conf` text of its own. The head file's writer [...]" and ended "setting a head's `look =` line in its section and keeping every other byte.", and the last bullet had the TUI write "the edited head file into the install bundle" and waited on profiles having a file. The heading keeps its words, since ADR-1403 names it. The earlier text is in git at e53919ea.

> Edited 2026-11-02: the maintainer had vtium-vitrify assign a head's profile through the head-file writer vtiumctl uses (`considered`). D6 was headed "the head file stays out" and began: "The TUI does not read or edit `heads.conf`. vtiumctl keeps the head file and the devices: detecting them, setting up heads, the trial, and installing the head file (ADR-1401 D12)."

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). Detecting devices, setup and the trial are vtium-config's, so the section writer is linked by vtium-config and `vtium check-config`, and the reasons for one library hold between those two.

### D7: what the TUI is built with

- **No TUI library.** The TUI is a small screen layer in vtium-config over rustix's terminal calls, which the crate already links with the features it needs, and the raw-mode guard and signal holding that `set` already has (`src/switch.rs`). It draws with ECMA-48's cursor position, erase and SGR sequences and DEC modes 25 and 1049, which vtium supports and every terminal of the VT100 line has, and decodes keys from the bytes a terminal sends for them, as vtium-term encodes them.
- **The TUI is a state machine apart from its terminal.** Its state takes events (a key, a reply, a resize, a signal) and returns effects (bytes to write, a command to run, an install bundle to write), so tests drive it without a terminal (D8), and the loop around it does the reading and writing.
- vtium-config is not part of the minspec console binary and is outside its size budget. It ships on every install as the glyph helper, so the TUI's size counts in the install tree that tier 3 measures; a library would add more than the screen layer does.

### D8: how it is judged

- **The state machine,** driven by events in unit tests: every view, the one-switch-outstanding rule, the refusal mark, the size list, and the save view's list of changes.
- **The real binary on a pty,** with vtium-term's terminal answering on the other side, as `tests/switch.rs` drives `set`: the test types keys into the pty, reads the terminal's screen as text, and records every byte the TUI writes. It checks the exact bytes: the four requests at start; `OSC 50 ; NAME ST` and the query for each switch, and no second switch before the first's reply; and the switch back to the start font and its query when the TUI leaves by its quit key, by the Ctrl-C byte, and by SIGTERM. Against a stand-in terminal that answers only DA1, and against vtium-term with font operations off, it checks that the TUI writes no OSC 50 byte. With vtium-term built with `images`, it checks each size picture's bytes, `a=T,f=32,t=d` with the specimen's width and height and its chunks, and its `a=d` when the highlight moves and when the TUI leaves; without `images`, it checks that the TUI sends no picture after the unanswered query.
- **The session's end,** in the console's own tests, as `tests/heads.rs` drives heads: after an OSC 50 switch and a zoom, the console's program exits, and the next frame draws in the font and size vtium.conf names, a vtium.conf changed since the start included; in a head of two consoles, a session ending in the console that did not switch leaves the font. Screen captures at 80 by 25, the minimum of D3, and at 100 by 30, and below it, of the one screen that names the size it needs, are vtium's own goldens, provisional until the maintainer reviews the layout.
- **Writing,** in a temporary directory standing in for `/etc/vtium` through the tool's path options, never as root: the shipped `vtium.conf.example` with one key changed differs in that one line, and with no change is byte-identical; each placement rule of D5 has its test; an install bundle whose target changed is refused with nothing written; a check that fails writes nothing; a write that fails before its rename, planted with a directory the test makes read-only, leaves the old file whole. The command the TUI runs for privilege is a seam whose default is `sudo`; the stand-in a test puts in its place runs `apply DIR` with the same path options, so it checks what `apply DIR` checks, and no test runs sudo.
- **Tier 3:** in the guest, on a real VT, a screenshot of the console in the font the TUI previews, and a screenshot after the TUI leaves without saving, which must match the screenshot taken before the TUI started.
- **The VT review:** a guide step, run with `make vt-check`, in which the maintainer opens the TUI, moves through three fonts and sees the console change with each, presses Ctrl-C and sees the start font come back, then saves a font, gives the sudo password once, and reads the one changed line of vtium.conf.

### D9: `new`, `preview` and the TUI

The TUI is where `new-frit`, named `new` until 2026-11-05 (D2), and `preview` live when a person runs them at a terminal. `new-frit`'s guided flow is the Fonts view's new frit (D3), and it ends in an install bundle and `apply DIR`, which replaces ADR-1200 D4's "one command, run with `sudo`, that writes the frit". `preview` becomes the same picture renderer the TUI uses for sizes, in the Looks view and the Fonts view (D3, D4): `preview FONT...` draws the detailed specimen of each font, the one the Fonts view shows on `s` (D4), through kitty graphics' raw pixels where the terminal answers the graphics query, and a text summary otherwise, whose count of the characters the font draws itself is over the detailed specimen. It takes kitty graphics alone, and the sixel fallback that the preview work planned is not part of this record. Both keep their non-interactive forms as commands, for scripts and packages.

> Edited 2026-11-02: the maintainer made `preview` the TUI's picture renderer (`considered`). The paragraph read: "`preview`'s pictures are what the TUI shows where live preview is off."

> Edited 2026-11-05: with the maintainer's addition of the detailed specimen to D4, `preview` draws the detailed specimen; the paragraph read "draws the engine's specimen of each font".

> Edited 2026-11-04: the views were reduced to three (D3; `considered`, 2026-11-04). The paragraph named "the TUI's new-frit view" and "the same picture renderer the TUI uses for sizes (D4)".

### D10: the banner's tagline

The tagline under the tool's banner says what the tool is for now: the console's fonts and settings, tried on the live console. "vitrified font sets" names the store of prerendered sets that ADR-1200 retired. This record recommends "fonts and settings for the vtium console" for the wide banner and "the console's fonts" for the narrow one. The banner keeps its glass logo and titles the tool "VTium Vitrifier", while the command is `vtium config` (D12). The build that adds the TUI changes both taglines in `src/banner.rs` and their test, the crate's description in `Cargo.toml`, and the usage text's summary line.

> Edited 2026-11-02: the maintainer renamed the tool (`considered`, D12). The paragraph ended "The tool keeps its name."

> Edited 2026-11-05: the banner also shows in the TUI's About page, the wide banner where the terminal fits it and the narrow one otherwise, from the same source as `vtium-config --help about` (D3; `considered`, 2026-11-05).

### D11: a session's end returns its console to the configured font

- **A switch lasts for the session.** An OSC 50 switch, from the TUI, `set` or any program, takes effect at once (the maintainer, 2026-11-02). When the session in a console ends, the program vtium started there exiting (`child_exited` in `crates/vtium/src/app.rs`), vtium returns that console to its configured font before the next login starts there, so the next login starts with the permanent choices.
- **The configured font is read again.** vtium reads vtium.conf's font keys again, `font-name`, `font-size`, `font-engine`, which ranks with `font-name` (ADR-1200 D16), and `font-line-spacing` (ADR-1302 D13), with the reader it starts with, and takes the font that ADR-1200 D9's chain gives the head, at the size `font-size` gives. A font the TUI saved is therefore the font the next login draws. A key that does not read is reported and dropped as it is at start. Every other key still applies at the next start, as docs/config.md says.
- **The size comes back with it,** so a zoom made since is undone at the same moment.

> Edited 2026-11-06: the maintainer decided that a session's end reads `font-line-spacing` again with the other font keys, so a change of line spacing applies at the next login as a change of font does. The list of keys read again had been `font-name`, `font-size` and `font-engine`.
- **One font for a head.** vtium holds the font for a whole head, and every console of the head draws in it (`Head::font` in `crates/vtium/src/head.rs`). A switch records the console it came from, and vtium returns the head's font when the session ends in that console, or in a head's only console. A session ending in another console of the same head leaves the font, so it never changes under another person's session.
- The C reference had no OSC 50 and kept its font for the life of its process, changed only by its zoom keys (`src/terminal.c` at kmscon `b1f7a7c5`, `zoom_in`, `zoom_out`), and a session's end left the size where the zoom put it. vtium returns the size at a session's end, a listed departure in the vtium crate's README.

> Edited 2026-11-02: D11 is built in the vtium crate, whose README lists the departure. A bullet that read "**Not built yet.** Today a switch lasts until the next switch or until vtium starts again." is gone, and the last bullet ended "a listed departure in the vtium crate's README once built".

### D12: the tool's name, and how users reach it

- **`vtium config`.** "vitrify" is an unusual word (the maintainer, 2026-11-02), so nothing a user types carries it. A user types `vtium config`, a subcommand of the console binary that replaces itself with the separate binary `vtium-config` by exec, passing on its arguments, as git runs its subcommands. The console binary holds only that hand-off, so the TUI's code never enters it or its minspec size budget. vtium finds `vtium-config` at the path it was built with, `/usr/bin/vtium-config` by default, as it finds the glyph helper.
- **`vtium-config` also works directly,** with the same commands (D2). The privilege step is written `sudo vtium-config apply DIR`, so root runs the small tool alone and never the console binary first.
- **The same binary is the glyph helper,** `vtium-config serve` (ADR-1200 D15), installed at `/usr/bin/vtium-config`, and the default of the `font-helper` key follows.
- **The library crate is renamed.** The crate now named vtium-config, which reads vtium.conf and the head file and holds the one head-file writer (D6), becomes vtium-settings, so the binary and the library never share a name.
- **"Vitrify" stays for the glass theme:** the banner's logo and its title, "VTium Vitrifier" (D10), and "vitrified" in prose where it means glyphs the engine draws. No command, path or crate a user meets is named for it.
- **When.** The rename lands now, in one branch, before the TUI is built, and the next release ships it. The AUR package's `/usr/bin/vtium-vitrify` becomes `/usr/bin/vtium-config` with that release.

### D13: vtium-config's colors, in a file of the person's own

> Edited 2026-11-10: this decision was added when the maintainer asked for vtium-config's colors to live in a file in the person's own configuration folder, with sets to choose from (`considered`, 2026-11-10). Until then the Flow view's colors were fixed in the code, as ADR-1403 D7 listed them.

The maintainer asked for a file that sets vtium-config's colors, with a few sets to choose from and not yet a whole theme (2026-11-10): "maybe could use an externalised configuration for vtium-config (something kept in xdg user scope). we're not trying to define a full theme...(yet) but at least, give an option for some color sets. For now it's just defined in the config file and we could set up 3 examples, and one must have the other two configs commented out". A fourth set, which fills the inside of each box, followed the same day: "we should add one more style where the interior of each node(box) has a background color (and an appropriate foreground color for the text)".

- **The file.** vtium-config reads its colors from `$XDG_CONFIG_HOME/vtium/vtium-config.conf`, which is `~/.config/vtium/vtium-config.conf` when `XDG_CONFIG_HOME` is unset. It reads the file once, when it starts. vtium itself never reads it. The file is written as vtium.conf is: `key = value` lines, and a `#` at the start of a line starts a comment. Its `[colors]` section holds the colors. The lines above that section are vtium-config's own settings, `draft = NAME` and `history = N`, which ADR-1403 D8 describes and vtium-config keeps up to date itself.
- **Colors by role, not a theme.** Each line of `[colors]` sets the color of one role:

  | Role | What it colors |
  |---|---|
  | `display`, `terminal`, `font-set`, `input` | the title of a box of that kind, and the titles of the columns and groups of that kind |
  | `placed` | the border of a part in a path, or free to pick |
  | `waiting` | the border of a connected device that is in no path yet |
  | `set-aside` | the border of a part that cannot be picked, and its italic text |
  | `picked` | the border of a part picked into the new path |
  | `line` | the lines between the boxes |
  | `display-fill`, `terminal-fill`, `font-set-fill`, `input-fill` | the inside of a box of that kind, its background |

- **How a color is written.** A color is `#rrggbb / BASIC`, such as `#7fb3c8 / cyan`. vtium-config draws the 24-bit color `#rrggbb` where the terminal takes 24-bit color, which it knows when `COLORTERM` is `truecolor` or `24bit`, and the basic color elsewhere, as on the Linux console's own text mode. A color can also be a basic color alone, or `none`, for no color, so that bold and underline alone mark the highlight. The basic colors are the sixteen of the console: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, and each of them with `bright-` in front. vtium-config draws no text in blue or bright blue, which are hard to read on black.
- **A fill uses the eight normal colors.** The basic color of a fill role is one of `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan` and `white`, because the Linux console has no bright background colors. Blue is allowed as a fill, because light text on blue reads well. A fill covers every cell inside a box's border. The border stays on the terminal's own background and keeps the color of its state. A fill role that the file leaves out, or sets to `none`, fills nothing.
- **The text on a fill is chosen for it.** The person does not choose the color of the text inside a filled box. On a 24-bit fill, vtium-config computes the fill's relative luminance, as WCAG 2.x defines it, and takes near-black text (`#1c1c1c`) or near-white text (`#f2f2f2`), whichever contrasts more with the fill. On a basic fill it takes black text on green, yellow, cyan and white, and bright white text on black, red, blue and magenta. The title inside a filled box is bold, since the fill already shows the box's kind. The text of a box that is set aside is italic. It also takes the `set-aside` color where both that color and the fill are 24-bit colors that contrast at 3:1 or more.
- **Four sets.** The first time vtium-config finds no `[colors]` section, it writes one with four sets, each under a comment that names it. The `console` set is live, and the other three are commented out. To use another set, the person puts a `#` before each line of the live set and takes the `#` away from each line of the other.

  | Set | What it looks like |
  |---|---|
  | `console` | the console's own basic colors: `display = cyan`, `terminal = yellow`, `font-set = magenta`, `input = green`, `placed = white`, `waiting = yellow`, `set-aside = bright-black`, `picked = bright-magenta`, `line = white`; no fills |
  | `muted` | softer 24-bit tones of the same hues, each with a basic color for a terminal without 24-bit color, such as `display = #7fb3c8 / cyan`; no fills |
  | `mono` | every role `none`, so bold and underline carry the highlight |
  | `filled` | each kind's boxes filled with a deep tone of its hue, deep enough for near-white text: `display-fill = #0f4c57 / cyan`, `terminal-fill = #5a4012 / yellow`, `font-set-fill = #4a2a60 / magenta`, `input-fill = #1b4a24 / green`; lighter titles and borders on black, such as `display = #6fc6d6 / cyan` and `set-aside = #969696 / bright-black` |

- **Writing the file the first time.** When the file is missing, or has no `[colors]` section, vtium-config adds the section at the end, keeping every line the file has, and makes the folders it needs. A new file is readable by everyone and writable by its owner, mode 0644. Run as root, vtium-config writes only into a folder root owns, never into another user's home, as D5 has it for every file it writes. The status line says once where it wrote the file. The comment at the top of the section explains the roles, the forms of a color and the sets in plain sentences, wrapped at 78 columns, as the configuration files vtium ships are.
- **A line it cannot read** is skipped, and the status line names it once, with its number and what is wrong, such as a bright color in a fill. A role that the file leaves out, or sets on a line vtium-config skips, takes the `console` set's color.
- **`NO_COLOR` wins.** With `NO_COLOR` set, vtium-config draws no color, whatever the file says, and the borders and words carry every state (ADR-1403 D7).
- **More sets come later.** Sets made for color blindness and for high contrast come in later passes. The maintainer: "I think the actual display of colors etc is resolved for now. we can add more color schemes as we mature more (such as a colorblind or accessibility high contracst color) in later passes".

### Passages this record changes

These change when this record is accepted, in their owners' changes; this record edits none of them.

- **ADR-1200 D4:** "Each command is non-interactive where it writes" becomes: each command but the TUI is non-interactive, and the TUI writes only through `apply DIR`. The table gains D2's rows, and `new`'s row ends in an install bundle and `apply DIR` (D9). The basis entry that leaves "the wrappers" to "other people's choice" stands as said; vtium-config ships one wrapper of its own.
- **ADR-1200 D5:** `set` checks the terminal before it writes (D4). "A switch changes the live console only; `font-name` in the configuration is what persists" gains: until the session that switched ends, when vtium returns the head to its configured font (D11). docs/config.md's `allow-font-ops` row says the same.
- **ADR-1200 D4's `preview` row** draws the engine's specimen through kitty graphics (D9).
- **ADR-1401 D12:** vtiumctl's drafts and `install DIR` of frits move to vtium-config's `apply DIR`; vtiumctl keeps the head file, but for a head's `look =` line, which vtium-config's `apply DIR` also writes, through the same writer (D6). `vtiumctl check` still checks frits through vtium-config's frit reader. The minspec bullet's "a frit is previewed by trying it on the live console through OSC 50 under the same countdown" becomes the TUI's live preview, with no countdown (D4). "`vtium-config` keeps the fonts" gains vtium.conf.
- **ADR-1200 D16:** "`vtiumctl` writes them when asked" of profile files becomes vtium-config, which writes and chooses them (D1).
- **ADR-1401 D12's `setup`,** which asks "which profile and frit" for each display, asks which profile, among those vtium-config wrote. vtium-config also sets a head's profile (D1). Both write through one head-file writer, a module of vtium-settings that `vtium check-config` also links, and no tool runs another to edit the file; D12's "Detection and setup live only in `vtiumctl`" and its "one crate parses and evaluates the configuration" gain (D6).
- **The tool's name throughout ADR-1200 and ADR-1401.** Every `vtium-vitrify` command they name, `serve`, `set`, `preview` and `new` among them, becomes `vtium-config` (`vtium config` as a user types it), the helper's program path `/usr/bin/vtium-vitrify` becomes `/usr/bin/vtium-config`, and the crate those records call vtium-config is vtium-settings (D12). ADR-1200 and ADR-1401 take a dated note naming the rename where their text changes; this record edits neither.
- **docs/config.md's `font-helper` row,** `crates/vtium/README.md`, `crates/vtium-vitrify/README.md` and the install tree change with the rename (D12).
- **The tracker issue for the tool** lists this work.

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). The passages above that concern `vtiumctl` describe ADR-1401 D12 as it stood before; ADR-1401 D12 now gives vtium-config every job it gave `vtiumctl`.

### For the maintainer's check

The maintainer decided on 2026-11-02 the three calls this section listed (`considered`): the command names stay as D2 has them; profiles move to vtium-config (D1); and a session's end returns its console to the configured font (D11).

## Consequences

### Positive

- A user chooses a font by seeing the real console in it, on every build, minspec included, where the console draws no pictures.
- A size is judged from a picture drawn by the same engine at the same pixel height, without the console's grid moving.
- A session never inherits the previous session's switched font or zoom.
- Everything the TUI does, a script or a package can do with the same commands, and the TUI's writes go through the same checked path as theirs.
- vtium.conf keeps its comments and order, so a file a person wrote stays theirs after the tool has changed it.
- One sudo prompt, for a command shown first, and no step of browsing or previewing runs as root.
- A file is either the old one or the new one, never half of each.

### Negative

- The console keeps a previewed font until the session ends when the TUI is killed with SIGKILL or its terminal goes away.
- Sizes are never tried on the live console, and where the console draws no kitty graphics, the packaged build today included, a size is seen only after it is saved.
- vtium reads vtium.conf's font keys again at each session's end, the one time it reads its configuration after start.
- A zoom no longer outlives the session it was made in.
- No live trial inside tmux or screen, over ssh from another terminal, or with `allow-font-ops` off.
- vtium-config grows on every install by the TUI and its screen layer.
- Checking vtium.conf needs the `vtium` binary installed beside the tool, which the install tree always has.

### Neutral

- vtium's key table moves into vtium-settings, which the console already links.
- The tool, its helper path, its crate and the configuration library are renamed in one branch before the TUI is built (D12).
- `set` stops writing OSC 50 to terminals that are not vtium.
- Profiles are written, chosen and assigned to heads in vtium-config.
- The head file's writer is a module of vtium-settings, which every tool that touches the head file links.
- `preview` and the TUI share one picture renderer.

## Alternatives Considered

- **The TUI in vtiumctl, or as a third binary.** Not taken, by the maintainer: it stays one tool, vtium-config.
- **Keeping the name vtium-vitrify.** Not taken, by the maintainer: the word is unusual for a command people type, and `vtium config` says what the tool does.
- **Building the tool into the console binary as a subcommand.** Not taken: the TUI's code would count against the minspec budget; `vtium config` hands off by exec instead.
- **ratatui, with crossterm or termion beneath it.** Not taken: a widget and layout framework and a second terminal layer beside the raw-mode and signal handling `set` already has, for a screen of lists, on every install including minspec boards.
- **ncurses, or dialog and whiptail.** Not taken: ncurses is a C library, against vtium's rule that its code is Rust with C only at a boundary it must cross; dialog and whiptail are C programs that minspec images may lack, and the tool cannot send OSC 50 between their prompts.
- **Preview by pictures alone,** as `preview` plans. Not taken as the main path for fonts: a picture is not the console, and the minspec console draws no pictures. Pictures serve sizes, and fonts where live preview is off.
  > Edited 2026-11-05: the maintainer made pictures the default preview for fonts too, pictures of the console rather than of a specimen, and the live console a trial on a key (D4, `considered`, 2026-11-05).
- **A size suffix on OSC 50,** `NAME:size=POINTS` as fontconfig writes a size. Not taken, by the maintainer: live switching stays a name only, and sizes are previewed as pictures.
- **Keeping a switched font after its session ends.** Not taken, by the maintainer: the next login starts with the permanent choices.
- **Sixel pictures where kitty graphics are not drawn.** Not taken here: inside vtium both need `images`, so sixel would help only other terminals that draw sixel and not kitty graphics, such as foot or xterm, at the cost of a second encoder; the preview work may add it later.
- **Preview by writing vtium.conf and restarting the console.** Not taken: each step would need root and a restart, which ends the user's session.
- **Sending OSC 50 to any terminal,** as `set` does today. Not taken: another terminal may act on it, as xterm looks for an X font of that name.
- **A countdown that reverts an unconfirmed font,** as a desktop's display settings do and as ADR-1401 D12 planned for vtiumctl on minspec. Not taken: the TUI stays in control and gives the font back when it leaves, and a countdown would take away a font the user is still looking at.
- **Rewriting vtium.conf from its parsed keys.** Not taken: it would lose every comment and the file's order.
- **Running the whole TUI as root, or asking sudo at each step.** Not taken: the first parses fonts and writes into the user's home as root, against ADR-1200 D14 and ADR-1401 D12; the second asks for a password many times.
- **Editing the head file in the TUI beyond a head's profile.** Not taken: vtiumctl keeps it (D6).

> Edited 2026-11-05: ADR-1403 took this alternative: vtium-config writes the head file's rules, heads and looks, now vtium.conf's sections, through the one section writer (D6; ADR-1403 D3, D8). The next two alternatives still hold: one writer, linked by every tool.

- **A head-file writer of vtium-config's own.** Not taken, by the maintainer: two writers of one file would drift, so vtium-config uses vtiumctl's (D6).
- **Running `vtiumctl` to edit the head file.** Not taken, by the maintainer: it would parse text across a process boundary, let a development build and a packaged one disagree, and need vtiumctl built; the tools link one module instead (D6).

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). The alternatives that name `vtiumctl` record what was weighed then, and vtium-config now does the work they gave `vtiumctl`.
