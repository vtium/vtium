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
  - upstream: "aaronsb/kmscon b1f7a7c5: the C reference read its font from kmscon.conf's font-name, font-size and font-engine at start (src/terminal.c, font_set, 921-955); while it ran, only the zoom keys changed the font, by its size (zoom_in and zoom_out, 1324-1355); its OSC handler took only setBackground and setForeground (osc_event, 859-870), so it had no OSC 50 and no tool that changed its font or its configuration"
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
observable:
  - "In a vtium console, moving the TUI's highlight from one frit to the next switches the whole console to that frit, and the TUI's own screen redraws at the console's new grid"
  - "Leaving the TUI without saving, by its quit key, by Ctrl-C, or by SIGTERM sent from another terminal, puts the console back in the font it had when the TUI started, checked by the OSC 50 query"
  - "Run in a terminal that is not vtium, the TUI writes no OSC 50 byte, says live preview is off and why, and still saves"
  - "Saving a new font-size into the shipped vtium.conf.example changes one line and leaves every other byte of the file as it was"
  - "Saving asks for the sudo password once, for one command the TUI shows first, and a write that fails leaves each file it did not finish exactly as it was"
  - "In a vtium built with `images`, highlighting a size shows a specimen picture of the frit at that size and leaves the console's font and grid as they were"
  - "After the TUI switches a console's font and the session in that console ends, the next login prompt draws in the font and size vtium.conf names, a font the TUI saved included"
status: accepted
date: 2026-11-02
deciders:
  - aaronsb
related: []
---

# ADR-1201: vtium config manages the console's fonts and settings, trying each choice on the live console

## Summary

- **Decided:** The tool, until now vtium-vitrify, is renamed: users type `vtium config`, which hands off to the separate binary `vtium-config`, and the configuration library takes the name vtium-settings. vtium-config becomes the one tool that manages the console's fonts and settings for as long as they exist: it lists, checks, previews, switches, writes and removes frits (font set files, `NAME.vtfrit` in `/etc/vtium/fonts`), and reads and edits `/etc/vtium/vtium.conf`. Run as `vtium config` with no command at a terminal, it opens a TUI, a full-screen program drawn in the terminal's text cells. When the TUI runs inside a vtium console with `allow-font-ops` on, each font the user highlights switches the live console to it through OSC 50, the sequence `vtium-config set` sends, so the user judges the real console in the real font; leaving without saving puts the console back in the font it had at the start. Sizes are previewed as pictures: the glyph engine draws a specimen of the frit at each size, and the TUI shows it through kitty graphics' raw pixels, which vtium decodes in the console, without changing the console's font. Where the console draws no kitty graphics, as in a build without `images`, sizes take effect only when saved. In a terminal that is not vtium the TUI sends no OSC 50, and shows pictures only where the terminal answers kitty's graphics query. When a console's session ends, vtium returns it to the font and size vtium.conf names, so the next login starts with the saved choices. Every action the TUI offers is also a non-interactive command, and the TUI does its writing through one of them, `apply DIR`: the TUI runs as the user, gathers what the user saved into an install bundle, and runs `sudo vtium-config apply DIR` once. `apply DIR` checks every file, edits vtium.conf line by line so that comments and order stay, and replaces each file in one rename. The TUI is built from rustix, which the tool already links, with no TUI library. Profiles are vtium-config's to write, choose and assign to a head; it changes a head's `look =` line through the one head-file writer vtiumctl also uses, and the rest of the head file and the running console's controls stay vtiumctl's.
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

## Context

vtium-config is vtium's font tool, a binary of its own outside the console (ADR-1200 D4, D6). It serves the console as its glyph helper (`serve`, ADR-1200 D15) and switches the running console's frit (`set`, ADR-1200 D5). ADR-1200 plans two more commands, `preview` and the guided builder `new`, and states that each command is non-interactive where it writes. ADR-1401 D12 makes `vtiumctl` the configuration tool for the head file, and has it write and install frits too. The tool was named vtium-vitrify until this record renamed it (D12); the records before this one, and this record's dated notes, use that name.

The maintainer tried `vtium-vitrify set monoid`, the tool's name then, on a VT, and the console changed font at once, as `setfont` changes a Linux console's font. The maintainer decided that vtium-config grows from that into an interactive tool for the console's fonts and its configuration file, whose TUI switches the live console as the user browses.

The C reference had no such tool. It read its font from kmscon.conf at start, and while it ran only the zoom keys changed the font, by its size; it had no OSC 50 (`src/terminal.c` at kmscon `b1f7a7c5`, `font_set`, `zoom_in`, `osc_event`). vtium's OSC 50 and its query already exist, behind `allow-font-ops`, which is on by default.

## Decision

Terms used below: a *frit* is a font set file, `NAME.vtfrit`, in the frits' directory, `/etc/vtium/fonts` unless `font-frits` names another (ADR-1200 D1). The *TUI* is vtium-config's full-screen interactive mode. *Live preview* is the TUI switching the console it runs in to the highlighted choice. An *install bundle* is a directory of files the user saved but that are not installed yet. None of this is built yet, apart from `set` and `serve`.

### D1: what the tool manages

- **Frits.** vtium-config lists the frits and the built-in fonts, checks a frit as the helper reads it, previews it, switches the console to it, writes a new or changed frit, and removes one.
- **vtium.conf.** vtium-config shows each key of `/etc/vtium/vtium.conf` with its value, its default and its description, and changes keys. The font keys come first (`font-name`, `font-size`, `font-frits`, `font-hinting`, `font-line-spacing`, `allow-font-ops`, `display-dpi` and the sprite policy's keys), and every other key is listed after them. A key's description, type and bounds come from vtium's own key table, `crates/vtium/src/config/keys.rs`, which moves into vtium-settings, the configuration crate vtium already links (ADR-1401 D12), so the console and the tool read one table.
- **One line of the head file,** `/etc/vtium/heads.conf`: a head's `look =` line, which assigns the head its profile, written through the head-file writer vtiumctl uses (D6). The rest of the head file stays vtiumctl's.
- **Profiles** are a frit, a cell and font options (ADR-1200 D16), and are vtium-config's to write and to choose (the maintainer, 2026-11-02): the TUI is where a user builds a profile, checks it against a display (the rule checker, ADR-1200 D4) and picks among the profiles. Until profiles have a file of their own, a frit's `cell=` line carries the cell, and the TUI edits it as part of the frit. vtiumctl keeps controlling a running console (`status`, the trial and the overlay) and the head file (ADR-1401 D12).
- **Assigning a profile to a head** (the maintainer, 2026-11-02). The Flow view's head cards show the heads, read with vtium-settings's parser, and the user picks the look each head draws with, a profile among them (D3; ADR-1403 D7). The change is a head's `look =` line, as in `[head desk]` with `look = desk`, and it is written as D6 says, in the apply step under sudo. A head names a profile only once profiles have a file of their own, so the assignment is built with them.

> Edited 2026-11-02: the maintainer had vtium-vitrify also assign a profile to a head, through the head-file writer vtiumctl uses (`considered`). The head-file bullet read: "Not the head file, `/etc/vtium/heads.conf`, which stays vtiumctl's (D6).", and the profiles bullet ended "and the head file, in which a head names its profile (ADR-1401 D12)."

> Edited 2026-11-02: the maintainer moved profiles from vtiumctl to vtium-vitrify, writing and choosing them (`considered`). The bullet read: "Profiles, once they have a file of their own (ADR-1200 D16), are a frit, a cell and font options, and are vtium-vitrify's to write.", and the record left the move to the maintainer's check.

> Edited 2026-11-04: the Heads view went into the Flow view's head cards (D3; `considered`, 2026-11-04). The bullet on assigning a profile read: "The TUI lists the heads `heads.conf` declares, read with vtium-settings's parser,".

### D2: one tool, and its commands

vtium-config stays one binary, apart from vtiumctl. Each command is typed as `vtium config COMMAND`, or as `vtium-config COMMAND` (D12). Every action the TUI offers is also one of these commands, and each command but the TUI is non-interactive, prints one line a result, and exits 0 on success, 1 on a failure and 2 on a usage error, as ADR-1200 D4 has it:

| Command | Runs as | Does |
|---|---|---|
| none (`vtium config`), at a terminal | the user | Opens the TUI. With no command and stdin or stdout not a terminal, it says that no command was given and exits 2, as it does today. |
| `fonts` | any user | Lists the frits and the built-in fonts, one line each, tab-separated: the name, whether the frit reads, and the sources this machine lacks, found through the sandboxed child (D3). |
| `check FILE` | any user | Checks a frit with the helper's frit reader, or a vtium.conf with `vtium check-config`, the console's own reader (D5). Writes nothing. |
| `config KEY`, `config KEY VALUE`, `config --unset KEY` | the user; root to install | Prints a key's value; with a value or `--unset`, makes that one change (D5). |
| `new-frit ...` | the user; root to install | The guided builder (ADR-1200 D4). Its non-interactive forms, `--template`, `--from FILE` and its flags, stay; at a terminal with no such input it opens the TUI at its new-frit view. |
| `remove-frit NAME` | the user; root to install | Removes a frit, refusing one that vtium.conf's `font-name` names. |
| `apply DIR` | root | Checks and installs a draft (D5). |
| `preview`, `use-frit`, `serve` | as ADR-1200 D4 | Unchanged but for `use-frit`'s name, and `use-frit` checks the terminal first (D4). |

- **Run as a user,** a command that writes makes an install bundle under `$XDG_STATE_HOME/vtium/install bundles/<time>/`, the directory ADR-1401 D12 gives vtiumctl's drafts, and prints the install bundle's path and the command that installs it, `sudo vtium-config apply DIR`. **Run as root,** it installs at once through the same steps `apply DIR` takes. A script therefore never needs the TUI.
- **The frit commands are `new-frit` and `use-frit`** (the maintainer, 2026-11-05), since the flow's commands of ADR-1403 D8 keep `set` and `new`. `install` installs the current draft, which it hands to `apply DIR`, the privileged step that runs as root and keeps its name (the maintainer, 2026-11-05).

  > Edited 2026-11-05: the maintainer renamed the flow's `apply` to `install` and the view's Save to Install (`considered`). The bullet read: "keep `set`, `new` and `apply`. `apply` with no argument installs the current draft".
- The names `fonts` and `remove-frit` differ from the retired store's `list` and `remove` (ADR-1200 D3), which stay unknown commands, so an old command never does a new thing.

> Edited 2026-11-05: the maintainer renamed the frit commands `new-frit` and `use-frit` (`considered`). They were `new` and `set`.

### D3: the TUI

- **Five views, and it opens on Flow** (the maintainer, 2026-11-04 and 2026-11-05). Most configuration work happens in the Flow view of ADR-1403 D7, so vtium-config opens there. Tab steps through the views in this order:
  - **Flow** draws the configuration from the displays to the input devices and edits it (ADR-1403 D7). Enter on a head's look card picks from the looks the Looks view defines, and `new look…` opens the Looks view's editor on a new look. Its head cards assign each head its look. Its base card edits the common top-level keys.
  - **Looks** manages the looks, each one object, a `[look NAME]` section of vtium.conf: its frit and size, and `vtlook =`, which names the vtlook of ADR-1304 that holds the filters, color and effects (ADR-1403 D1). It creates, edits, renames and deletes a look, with live preview where it works (D4). Its sizes are where the Sizes view went: a look offers its sizes, in points as `font-size` takes them, with automatic first, and tries each on the live console, so the user tries a size and keeps it as quickly as before. Until ADR-1304's filters are built, the Looks view manages the profile part and shows the filters as not yet available. Deleting a look is confirmed, and the confirmation names the heads that use it (ADR-1403 D8).
  - **Fonts** manages the font resources. It lists the places vtium finds fonts, each with what it holds: the folders of `font-paths` (ADR-1200 D25), kbd's console-font folders, vconsole's font and the built-in fonts. It lists the fonts installed in those places, with each font's coverage, which vtium-config reads only in the sandbox (below). It lists the frits as files, with how each reads, and offers new, which is `new-frit`'s guided flow (D9), rename and delete, each frit noting which looks use it. It previews a font live (D4). It adds, removes and orders the folders of `font-paths`, which it writes into the draft like any key.
  - **Settings** lists every top-level key of vtium.conf (D1), the full list of which the base card in Flow edits a common few.
  - **Help** stays on its own tab; in the Flow view `?` opens the route card of ADR-1403 D7, not help. Help holds two pages. Help lists the keys and what each view does, from the catalog the command line's help draws on (`crates/vtium-config/src/help.rs`). About shows what `vtium-config --help about` prints, the tool's name, version and logo, drawn from that one source so the two never drift; where the logo is wider than the terminal, it shows the narrow banner (D10).
  The Heads view goes, since the flow is the heads, and the Sizes view goes into the Looks view. The save view goes too: Install, with View, Revert, Restore, New and Delete draft, is an action in the tab bar of every view (ADR-1403 D7, D8).
- **Where it runs.** Inside a vtium console, where live preview works, and in any other terminal, where it does not (D4). It needs a terminal of at least 100 by 30 cells, a provisional number the maintainer judges from the previewer and may change (the maintainer, 2026-11-05). The minimum applies to the terminal vtium-config runs in, not to the machine: a minspec board with a 40-column console is configured over ssh from a terminal at 100 by 30 or larger, without live preview, with the commands at any size, or with a draft built on another machine and copied over, since a draft is a complete vtium.conf (ADR-1403 D8). Below the minimum, as when a previewed font leaves the grid too small, the TUI draws no view: it shows one screen with the size it needs, the `vtium config` commands, and the keys to leave and to undo the font preview, so a user can always get out.

  > Edited 2026-11-05: the maintainer raised the floor to 100 by 30, provisionally, after the layout broke at 40 columns in the previewer (`considered`). The bullet read: "It fits a grid of 40 by 25 cells. When a previewed font leaves a grid smaller than that, the TUI draws one line naming the font and the keys that keep it or go back, so a user can always get out."

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

> Edited 2026-11-04 and 2026-11-05: the maintainer reduced vtium-config to Flow, Fonts and Settings, opening on Flow, made Fonts a font resource manager, added a Looks view that manages looks with their filters, and added a Help view with Help and About pages (`considered`, 2026-11-04 and 2026-11-05). The bullet read: "**What it shows.** A list of the frits and the built-in fonts, each with its state; the sizes for the highlighted font, in points as `font-size` takes them, with automatic first; vtium.conf's keys (D1); the new-frit view, which is `new`'s guided flow; and a save view, which lists each file the save changes and each line of vtium.conf that changes, with the one command it runs." The example showed the font view with a size line. The earlier text is in git at 9f4fd304.

### D4: live preview, and giving the console back

- **Finding out what the terminal is.** At start the TUI writes four requests at once: XTVERSION (`CSI > q`), the OSC 50 query (`OSC 50 ; ? ST`), kitty's graphics query (`APC G i=N,s=1,v=1,a=q,t=d,f=24;AAAA ST`, a one-pixel image the terminal checks and does not keep) and DA1 (`CSI c`). Every terminal of the VT100 line answers DA1, and terminals answer requests in the order they arrive, so a reply that has not come before DA1's will not come. The TUI waits for DA1's reply at most one second, `set`'s wait, for a terminal that answers nothing.
  - XTVERSION names vtium (`vtium(VERSION)`) and the OSC 50 query is answered: fonts are previewed live, and the query's answer is the font to give back.
  - XTVERSION names vtium and the OSC 50 query is not answered: `allow-font-ops` is off. Fonts are not previewed live; the TUI says so, names the key, and offers to turn it on as a change saved like any other.
  - Anything else, another terminal, a multiplexer such as tmux, an ssh client's terminal or a serial line: the TUI never sends OSC 50, since xterm, for one, would look for an X font of that name.
  - Separately, the kitty graphics query answered before DA1 means the terminal draws kitty graphics, and the TUI previews sizes, and fonts it cannot switch to live, as pictures. vtium answers it only when built with `images` (ADR-1100), which the packaged build is not today. With no answer, the TUI shows a text summary instead: the coverage, the cell and the grid each size gives.
  - `set` makes the same check before it writes, and sends nothing to a terminal that is not vtium. Today it writes OSC 50 to any terminal and only reads the reply.
- **While the user browses.** Each font the user highlights is sent as `OSC 50 ; NAME ST` followed by the query, the bytes `set` sends. At most one switch is outstanding: the next is sent when the query's reply to the last comes back, and it names whatever is highlighted then, so holding an arrow key sends one switch per reply and never a queue of stale ones. A reply that names another font means the console refused the switch; the TUI marks that font "kept by the console, see its log" and goes on. There is no countdown that reverts a font the user has not confirmed: the TUI stays in control, and leaving it gives the font back.
- **Sizes, as pictures.** Live switching stays OSC 50 with a frit's name alone, and the TUI never changes the console's size. For each size of the highlighted frit, the glyph engine the helper runs (ADR-1200 D14, D15) draws a specimen, a few lines of Latin, box drawing and the locale's script, at the pixel height that size gives, into an RGBA picture. The engine runs in the sandboxed child, never in the TUI's process. The pixel height comes from the density the console uses, in ADR-1200 D17's order: vtium.conf's `display-dpi` where it is set, else the density the TUI reads from the displays' EDID in sysfs, else 96, and the TUI says which it used. The TUI sends the picture with kitty graphics' direct transmission of raw pixels (`a=T`, `f=32`, `t=d`, its width and height in `s` and `v`, the base64 split into chunks with `m=1`), which vtium decodes in the console without the image helper (ADR-1100 D7), shows it in its preview area at its own pixel size, and deletes it (`a=d`) when the highlight moves and when the TUI leaves. Where the terminal draws no kitty graphics, a size takes effect only when it is saved.
  > Edited 2026-11-02: the maintainer answered the live-sizes probe (`considered`): no size suffix on OSC 50, and sizes are previewed as pictures. The bullet recommended that the console take `NAME:size=POINTS` over OSC 50 and report the size in its query, and had the TUI apply sizes only when saved until then.
- **Giving the console back.** Leaving without saving switches the console back to the font the start query named, and then asks the query to check it. The TUI does this on its quit key; on Ctrl-C, which arrives as a byte in raw mode; on SIGINT, SIGTERM, SIGQUIT and SIGHUP, which it holds while the terminal is raw, as `set` does, and handles in its loop; and on a panic, from the guard that puts the terminal's mode back. It cannot after SIGKILL, or once its terminal is gone: the console then keeps the previewed font until that console's session ends, when vtium returns it to the configured font (D11). One case the name alone cannot give back exactly: when the console draws a built-in font it chose by height, the query answers `unscii-8` or `unifont`, and switching back to that name fixes that built-in font, so a later zoom across 16 pixels does not change it as the automatic choice would. The session's end clears it (D11).
- **After saving,** the console stays in the saved font, which is the font the saved vtium.conf names for the next start.

### D5: writing

> Edited 2026-11-05: the directory of files `apply DIR` takes is called an install bundle, since the word draft now means the working copy of ADR-1403 D8 alone; this record called it a draft until then (`considered`, 2026-11-05).

> Edited 2026-11-04 and 2026-11-05: the save view went with the reduction of the views (D3; `considered`, 2026-11-04). Install is an action in the tab bar of every view, beside View, Revert, Restore, New and Delete draft, and what it installs is the draft of ADR-1403 D8, through `apply DIR`. The steps below are what `apply DIR` runs.

- **Sudo once, at the moment of writing.** The TUI runs as the user from start to end and never as root, so no font is parsed and nothing is written into the user's home as root. When the user saves, it writes the install bundle, shows the command, `sudo vtium-config apply DIR`, and runs it. sudo asks for the password once, in the TUI's terminal. The TUI leaves its alternate screen while sudo asks, and comes back after. Nothing else in the TUI needs privilege.
- **What an install bundle holds.** Each frit to write, each frit to remove, and for vtium.conf and `heads.conf` the whole edited file, with the SHA-256 of each file it replaces as that file was when the install bundle was made.
- **What `install` does,** as root:
  1. It reads the draft without following a link, takes only regular files, and bounds each file's size, since root is reading a directory the user owns.
  2. It refuses the install bundle, writing nothing, when a file it replaces has changed since the install bundle was made, naming the file, so an edit made meanwhile by hand or by another tool is never lost.
  3. It checks every file before it writes any: each frit with the helper's frit reader, strictly, as the helper reads it; the edited `heads.conf` with `vtium check-config FILE`, which checks a head file before it is installed (ADR-1401 D7); and the edited vtium.conf with `vtium check-config --configdir` naming the install bundle's directory, the console's own reader, so the verdict before writing is the verdict at the next start, as ADR-1401 D12 has for the head file. A file that fails stops the apply before anything is written, with each problem and its line.
  4. It writes the frits first, then `heads.conf`, and vtium.conf last, so neither configuration file names a frit that is not yet there. Each file is written to a new file in the same directory, owned by root with mode 0644, flushed to disk, and renamed over the old one, and the old one is kept beside it with a `.bak` suffix, one generation, as vtiumctl's `install` keeps what it replaces.
  5. It prints one line a file, `wrote PATH` or `failed PATH` with the reason, and exits 0 when it wrote them all.
- **What a failed write leaves.** A file whose write fails before its rename is left exactly as it was, and the new file is removed. A file renamed before a later file failed stays written, and `apply DIR` names which were written and which were not. No file is ever left half written. The install bundle stays, so the user can run `apply` again.
- **Editing vtium.conf line by line.** The tool changes only the lines it must, and every other byte, comments, blank lines, order and line endings, stays as it was:
  - A key that is set has the value on the line vtium takes it from replaced in place, with any comment after it kept.
  - A key that is not set but has a commented line, as `#font-size=0` in the shipped example, gets its new line directly below that comment, beside its description.
  - Any other new key is added at the end of the file.
  - `--unset` comments out the line vtium takes the value from, so the key falls back to its default and the line is there to set again.
  - A switch is written as vtium.conf writes switches, `allow-font-ops` or `no-allow-font-ops`.

### D6: the head file has one writer

- **vtiumctl keeps the head file and the devices:** detecting them, setting up heads, rules, policy, the trial, and installing the head file (ADR-1401 D12). vtium-config changes one thing in it, a head's `look =` line (D1).
- **One writer, a library.** vtium-config never writes `heads.conf` text of its own. The head file's writer is a module of vtium-settings, the configuration crate that already holds the head file's parser and evaluator, and it stays the one writer for good (the maintainer, 2026-11-02). vtium-config, vtiumctl and `vtium check-config` all link it, and no tool runs another to edit the head file. The writer edits line by line as D5's vtium.conf rules do, setting a head's `look =` line in its section and keeping every other byte.
- **Why a library.** The three tools are Rust crates in one workspace, built and shipped together. Linking one module gives them one parser, one check, one rule for editing without losing comments, and one wording of each error, fixed when they are compiled. Nothing parses another program's text output, a development build and a packaged one cannot disagree about the file, and vtium-config does not need vtiumctl to be built. A shared piece that shows a change and asks to confirm it keeps the two tools' screens alike.
- **The one process boundary** in this design is the privilege step, `sudo vtium-config apply DIR` (D5), which keeps the code that runs as root small.
- **Checked before it is written.** The TUI writes the edited head file into the install bundle as the user, and `apply DIR`, under sudo, checks it with `vtium check-config FILE` before it writes it, as it checks every file (D5). Once profiles have a file and a head names a profile with its own font, a profile wins over vtium.conf's `font-name` for that head (ADR-1200 D16), so Install's question then says which heads the saved font does not reach.

> Edited 2026-11-02: the maintainer pinned the head file's writer to a module of vtium-config for good, linked by every tool, with no tool running another to edit the file (`considered`). The bullet said that vtiumctl, once built, would link the module, and that whether vtium-vitrify kept linking it or ran `vtiumctl` was the vtiumctl work's choice.

> Edited 2026-11-02: the maintainer had vtium-vitrify assign a head's profile through the head-file writer vtiumctl uses (`considered`). D6 was headed "the head file stays out" and began: "The TUI does not read or edit `heads.conf`. vtiumctl keeps the head file and the devices: detecting them, setting up heads, the trial, and installing the head file (ADR-1401 D12)."

### D7: what the TUI is built with

- **No TUI library.** The TUI is a small screen layer in vtium-config over rustix's terminal calls, which the crate already links with the features it needs, and the raw-mode guard and signal holding that `set` already has (`src/switch.rs`). It draws with ECMA-48's cursor position, erase and SGR sequences and DEC modes 25 and 1049, which vtium supports and every terminal of the VT100 line has, and decodes keys from the bytes a terminal sends for them, as vtium-term encodes them.
- **The TUI is a state machine apart from its terminal.** Its state takes events (a key, a reply, a resize, a signal) and returns effects (bytes to write, a command to run, an install bundle to write), so tests drive it without a terminal (D8), and the loop around it does the reading and writing.
- vtium-config is not part of the minspec console binary and is outside its size budget. It ships on every install as the glyph helper, so the TUI's size counts in the install tree that tier 3 measures; a library would add more than the screen layer does.

### D8: how it is judged

- **The state machine,** driven by events in unit tests: every view, the one-switch-outstanding rule, the refusal mark, the size list, and the save view's list of changes.
- **The real binary on a pty,** with vtium-term's terminal answering on the other side, as `tests/switch.rs` drives `set`: the test types keys into the pty, reads the terminal's screen as text, and records every byte the TUI writes. It checks the exact bytes: the four requests at start; `OSC 50 ; NAME ST` and the query for each switch, and no second switch before the first's reply; and the switch back to the start font and its query when the TUI leaves by its quit key, by the Ctrl-C byte, and by SIGTERM. Against a stand-in terminal that answers only DA1, and against vtium-term with font operations off, it checks that the TUI writes no OSC 50 byte. With vtium-term built with `images`, it checks each size picture's bytes, `a=T,f=32,t=d` with the specimen's width and height and its chunks, and its `a=d` when the highlight moves and when the TUI leaves; without `images`, it checks that the TUI sends no picture after the unanswered query.
- **The session's end,** in the console's own tests, as `tests/heads.rs` drives heads: after an OSC 50 switch and a zoom, the console's program exits, and the next frame draws in the font and size vtium.conf names, a vtium.conf changed since the start included; in a head of two consoles, a session ending in the console that did not switch leaves the font. Screen captures at 100 by 30, the provisional minimum of D3, and below it, of the one screen that names the size it needs, are vtium's own goldens, provisional until the maintainer reviews the layout.
- **Writing,** in a temporary directory standing in for `/etc/vtium` through the tool's path options, never as root: the shipped `vtium.conf.example` with one key changed differs in that one line, and with no change is byte-identical; each placement rule of D5 has its test; an install bundle whose target changed is refused with nothing written; a check that fails writes nothing; a write that fails before its rename, planted with a directory the test makes read-only, leaves the old file whole. The command the TUI runs for privilege is a seam whose default is `sudo`; the stand-in a test puts in its place runs `apply DIR` with the same path options, so it checks what `apply DIR` checks, and no test runs sudo.
- **Tier 3:** in the guest, on a real VT, a screenshot of the console in the font the TUI previews, and a screenshot after the TUI leaves without saving, which must match the screenshot taken before the TUI started.
- **The VT review:** a guide step, run with `make vt-check`, in which the maintainer opens the TUI, moves through three fonts and sees the console change with each, presses Ctrl-C and sees the start font come back, then saves a font, gives the sudo password once, and reads the one changed line of vtium.conf.

### D9: `new`, `preview` and the TUI

The TUI is where `new-frit`, named `new` until 2026-11-05 (D2), and `preview` live when a person runs them at a terminal. `new-frit`'s guided flow is the Fonts view's new frit (D3), and it ends in an install bundle and `apply DIR`, which replaces ADR-1200 D4's "one command, run with `sudo`, that writes the frit". `preview` becomes the same picture renderer the TUI uses for sizes, in the Looks view and the Fonts view (D3, D4): `preview FONT...` draws the engine's specimen of each font through kitty graphics' raw pixels where the terminal answers the graphics query, and a text summary otherwise. It takes kitty graphics alone, and the sixel fallback that the preview work planned is not part of this record. Both keep their non-interactive forms as commands, for scripts and packages.

> Edited 2026-11-02: the maintainer made `preview` the TUI's picture renderer (`considered`). The paragraph read: "`preview`'s pictures are what the TUI shows where live preview is off."

> Edited 2026-11-04: the views were reduced to three (D3; `considered`, 2026-11-04). The paragraph named "the TUI's new-frit view" and "the same picture renderer the TUI uses for sizes (D4)".

### D10: the banner's tagline

The tagline under the tool's banner says what the tool is for now: the console's fonts and settings, tried on the live console. "vitrified font sets" names the store of prerendered sets that ADR-1200 retired. This record recommends "fonts and settings for the vtium console" for the wide banner and "the console's fonts" for the narrow one. The banner keeps its glass logo and titles the tool "VTium Vitrifier", while the command is `vtium config` (D12). The build that adds the TUI changes both taglines in `src/banner.rs` and their test, the crate's description in `Cargo.toml`, and the usage text's summary line.

> Edited 2026-11-02: the maintainer renamed the tool (`considered`, D12). The paragraph ended "The tool keeps its name."

> Edited 2026-11-05: the banner also shows in the TUI's About page, the wide banner where the terminal fits it and the narrow one otherwise, from the same source as `vtium-config --help about` (D3; `considered`, 2026-11-05).

### D11: a session's end returns its console to the configured font

- **A switch lasts for the session.** An OSC 50 switch, from the TUI, `set` or any program, takes effect at once (the maintainer, 2026-11-02). When the session in a console ends, the program vtium started there exiting (`child_exited` in `crates/vtium/src/app.rs`), vtium returns that console to its configured font before the next login starts there, so the next login starts with the permanent choices.
- **The configured font is read again.** vtium reads vtium.conf's font keys again, `font-name`, `font-size` and `font-engine`, which ranks with `font-name` (ADR-1200 D16), with the reader it starts with, and takes the font that ADR-1200 D9's chain gives the head, at the size `font-size` gives. A font the TUI saved is therefore the font the next login draws. A key that does not read is reported and dropped as it is at start. Every other key still applies at the next start, as docs/config.md says.
- **The size comes back with it,** so a zoom made since is undone at the same moment.
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
- No live preview inside tmux or screen, over ssh from another terminal, or with `allow-font-ops` off.
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
- **A size suffix on OSC 50,** `NAME:size=POINTS` as fontconfig writes a size. Not taken, by the maintainer: live switching stays a name only, and sizes are previewed as pictures.
- **Keeping a switched font after its session ends.** Not taken, by the maintainer: the next login starts with the permanent choices.
- **Sixel pictures where kitty graphics are not drawn.** Not taken here: inside vtium both need `images`, so sixel would help only other terminals that draw sixel and not kitty graphics, such as foot or xterm, at the cost of a second encoder; the preview work may add it later.
- **Preview by writing vtium.conf and restarting the console.** Not taken: each step would need root and a restart, which ends the user's session.
- **Sending OSC 50 to any terminal,** as `set` does today. Not taken: another terminal may act on it, as xterm looks for an X font of that name.
- **A countdown that reverts an unconfirmed font,** as a desktop's display settings do and as ADR-1401 D12 planned for vtiumctl on minspec. Not taken: the TUI stays in control and gives the font back when it leaves, and a countdown would take away a font the user is still looking at.
- **Rewriting vtium.conf from its parsed keys.** Not taken: it would lose every comment and the file's order.
- **Running the whole TUI as root, or asking sudo at each step.** Not taken: the first parses fonts and writes into the user's home as root, against ADR-1200 D14 and ADR-1401 D12; the second asks for a password many times.
- **Editing the head file in the TUI beyond a head's profile.** Not taken: vtiumctl keeps it (D6).
- **A head-file writer of vtium-config's own.** Not taken, by the maintainer: two writers of one file would drift, so vtium-config uses vtiumctl's (D6).
- **Running `vtiumctl` to edit the head file.** Not taken, by the maintainer: it would parse text across a process boundary, let a development build and a packaged one disagree, and need vtiumctl built; the tools link one module instead (D6).
