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
observable:
  - "In a vtium console, moving the TUI's highlight from one frit to the next switches the whole console to that frit, and the TUI's own screen redraws at the console's new grid"
  - "Leaving the TUI without saving, by its quit key, by Ctrl-C, or by SIGTERM sent from another terminal, puts the console back in the font it had when the TUI started, checked by the OSC 50 query"
  - "Run in a terminal that is not vtium, the TUI writes no OSC 50 byte, says live preview is off and why, and still saves"
  - "Saving a new font-size into the shipped vtium.conf.example changes one line and leaves every other byte of the file as it was"
  - "Saving asks for the sudo password once, for one command the TUI shows first, and a write that fails leaves each file it did not finish exactly as it was"
status: proposed
date: 2026-11-02
deciders:
  - aaronsb
related: []
---

# ADR-1201: vtium-vitrify manages the console's fonts and settings, trying each choice on the live console

## Summary

- **Decided:** vtium-vitrify becomes the one tool that manages the console's fonts and settings for as long as they exist: it lists, checks, previews, switches, writes and removes frits (font set files, `NAME.vtfrit` in `/etc/vtium/fonts`), and reads and edits `/etc/vtium/vtium.conf`. Run with no command at a terminal, it opens a TUI, a full-screen program drawn in the terminal's text cells. When the TUI runs inside a vtium console with `allow-font-ops` on, each font the user highlights switches the live console to it through OSC 50, the sequence `vtium-vitrify set` sends, so the user judges the real console in the real font; leaving without saving puts the console back in the font it had at the start. In any other terminal the TUI sends no OSC 50 and previews without switching. Every action the TUI offers is also a non-interactive command, and the TUI does its writing through one of them, `apply`: the TUI runs as the user, gathers what the user saved into a draft directory, and runs `sudo vtium-vitrify apply DIR` once. `apply` checks every file, edits vtium.conf line by line so that comments and order stay, and replaces each file in one rename. The TUI is built from rustix, which the tool already links, with no TUI library. The head file stays vtiumctl's.
- **Trades away:** vtium-vitrify ships on every install as the glyph helper, so the TUI's code lands on minspec boards too, outside the console's size budget but inside the install tree tier 3 measures. The TUI cannot put the font back after SIGKILL or once its terminal is gone, and the console then keeps the previewed font until the next switch or start. Live size preview needs the console to take a size over OSC 50, which it does not do yet. Inside tmux or screen, or over ssh from another terminal, there is no live preview.
- **One-way?** No. The tool and its commands change in a session. The command names become expensive to change once scripts and packages call them.
- **Probes:**
  - *Confident (the TUI and the commands):* every choice the TUI offers is also a command a script can run, and the TUI saves through the same command a script uses (`apply`). The earlier decision that vtium supplies a scriptable tool and leaves the wrappers to others stands, and the TUI is the one guided face vtium ships on top of it. Is that the shape you want?
  - *Confident (the head file):* the TUI does not edit `heads.conf`, the file that places displays and keyboards into heads. vtiumctl keeps it, and vtium-vitrify keeps the fonts and vtium.conf. Should the head file stay out?
  - *Not confident (live sizes):* you asked for sizes to switch live as fonts do. Today the console takes only a font's name over OSC 50, so live sizes need the console to take a size too, in a form only vtium reads, such as `monoid:size=12`, the way fontconfig writes a size. Until that lands, the TUI shows what grid each size gives and applies the size only when it is saved. Is a vtium-only form acceptable, or should sizes stay applied on save?
- **Inversion:** Between "vtium supplies only non-interactive commands, and any interactive front end is someone else's", ADR-1200's earlier stance, and "an interactive program is the way to configure the console", as a desktop's settings panel is. vtium keeps every action scriptable and ships one interactive front end on top. The one thing the TUI adds, switching the live console, rests on OSC 50, which any program may send.

## Context

vtium-vitrify is vtium's font tool, a binary of its own outside the console (ADR-1200 D4, D6). It serves the console as its glyph helper (`serve`, ADR-1200 D15) and switches the running console's frit (`set`, ADR-1200 D5). ADR-1200 plans two more commands, `preview` and the guided builder `new`, and states that each command is non-interactive where it writes. ADR-1401 D12 makes `vtiumctl` the configuration tool for the head file, and has it write and install frits too.

The maintainer tried `vtium-vitrify set monoid` on a VT, and the console changed font at once, as `setfont` changes a Linux console's font. The maintainer decided that vtium-vitrify grows from that into an interactive tool for the console's fonts and its configuration file, whose TUI switches the live console as the user browses.

The C reference had no such tool. It read its font from kmscon.conf at start, and while it ran only the zoom keys changed the font, by its size; it had no OSC 50 (`src/terminal.c` at kmscon `b1f7a7c5`, `font_set`, `zoom_in`, `osc_event`). vtium's OSC 50 and its query already exist, behind `allow-font-ops`, which is on by default.

## Decision

Terms used below: a *frit* is a font set file, `NAME.vtfrit`, in the frits' directory, `/etc/vtium/fonts` unless `font-frits` names another (ADR-1200 D1). The *TUI* is vtium-vitrify's full-screen interactive mode. *Live preview* is the TUI switching the console it runs in to the highlighted choice. A *draft* is a directory of files the user saved but that are not installed yet. None of this is built yet, apart from `set` and `serve`.

### D1: what the tool manages

- **Frits.** vtium-vitrify lists the frits and the built-in fonts, checks a frit as the helper reads it, previews it, switches the console to it, writes a new or changed frit, and removes one.
- **vtium.conf.** vtium-vitrify shows each key of `/etc/vtium/vtium.conf` with its value, its default and its description, and changes keys. The font keys come first (`font-name`, `font-size`, `font-frits`, `font-hinting`, `font-line-spacing`, `allow-font-ops`, `display-dpi` and the sprite policy's keys), and every other key is listed after them. A key's description, type and bounds come from vtium's own key table, `crates/vtium/src/config/keys.rs`, which moves into vtium-config, the configuration crate vtium already links (ADR-1401 D12), so the console and the tool read one table.
- **Not the head file,** `/etc/vtium/heads.conf`, which stays vtiumctl's (D6).
- **Profiles,** once they have a file of their own (ADR-1200 D16), are a frit, a cell and font options, and are vtium-vitrify's to write. Until then a frit's `cell=` line carries the cell, and the TUI edits it as part of the frit.

### D2: one tool, and its commands

vtium-vitrify stays one binary, apart from vtiumctl. Every action the TUI offers is also one of these commands, and each command but the TUI is non-interactive, prints one line a result, and exits 0 on success, 1 on a failure and 2 on a usage error, as ADR-1200 D4 has it:

| Command | Runs as | Does |
|---|---|---|
| none, at a terminal | the user | Opens the TUI. With no command and stdin or stdout not a terminal, it says that no command was given and exits 2, as it does today. |
| `fonts` | any user | Lists the frits and the built-in fonts, one line each, tab-separated: the name, whether the frit reads, and the sources this machine lacks, found through the sandboxed child (D3). |
| `check FILE` | any user | Checks a frit with the helper's frit reader, or a vtium.conf with `vtium check-config`, the console's own reader (D5). Writes nothing. |
| `config KEY`, `config KEY VALUE`, `config --unset KEY` | the user; root to install | Prints a key's value; with a value or `--unset`, makes that one change (D5). |
| `new ...` | the user; root to install | The guided builder (ADR-1200 D4). Its non-interactive forms, `--template`, `--from FILE` and its flags, stay; at a terminal with no such input it opens the TUI at its new-frit view. |
| `remove-frit NAME` | the user; root to install | Removes a frit, refusing one that vtium.conf's `font-name` names. |
| `apply DIR` | root | Checks and installs a draft (D5). |
| `preview`, `set`, `serve` | as ADR-1200 D4 | Unchanged, except that `set` checks the terminal first (D4). |

- **Run as a user,** a command that writes makes a draft under `$XDG_STATE_HOME/vtium/drafts/<time>/`, the directory ADR-1401 D12 gives vtiumctl's drafts, and prints the draft's path and the command that installs it, `sudo vtium-vitrify apply DIR`. **Run as root,** it installs at once through the same steps `apply` takes. A script therefore never needs the TUI.
- The names `fonts` and `remove-frit` differ from the retired store's `list` and `remove` (ADR-1200 D3), which stay unknown commands, so an old command never does a new thing.

### D3: the TUI

- **What it shows.** A list of the frits and the built-in fonts, each with its state; the sizes for the highlighted font, in points as `font-size` takes them, with automatic first; vtium.conf's keys (D1); the new-frit view, which is `new`'s guided flow; and a save view, which lists each file the save changes and each line of vtium.conf that changes, with the one command it runs.
- **Where it runs.** Inside a vtium console, where live preview works, and in any other terminal, where it does not (D4). It fits the 320x240 floor's grid of 40 by 30 cells, as ADR-1401 D12 asks of vtiumctl's dialog. When a previewed font leaves a grid smaller than that, the TUI draws one line naming the font and the keys that keep it or go back, so a user can always get out.
- **How it draws.** It switches to the alternate screen (DEC mode 1049), so the shell's screen comes back when it leaves, hides the cursor, and redraws whole on each change and on SIGWINCH, which the console sends when a font switch changes the grid (ADR-1200 D17).
- **Fonts are read only in the sandbox.** The TUI and the commands read frits as text, as the console does. They parse a font file, even to read the family name inside it that a `font:` source matches, only through the sandboxed child that `preview` and `new` use (ADR-1200 D14), never in their own process, so listing installed fonts or a font's coverage puts no font parser in a process with the user's rights and a terminal.
- **Its words** live in a catalog module of the crate's own, as the code rules ask, and are English, as ADR-1402 keeps what vtium's tools print in a terminal.

An example of the font view at 40 columns, to show that it fits; its layout is the build's to settle and stays provisional until the maintainer reviews it:

```text
 Fonts                live preview: on
 > minspec-16         reads
   proof              1 source missing
   tiny5              reads
   unifont            built in
   unscii-8           built in
 Size     automatic   80x30 cells
 ---------------------------------------
 Enter keep   s save   q leave, undo
```

### D4: live preview, and giving the console back

- **Finding out what the terminal is.** At start the TUI writes three requests at once: XTVERSION (`CSI > q`), the OSC 50 query (`OSC 50 ; ? ST`) and DA1 (`CSI c`). Every terminal of the VT100 line answers DA1, and terminals answer requests in the order they arrive, so a reply that has not come before DA1's will not come. The TUI waits for DA1's reply at most one second, `set`'s wait, for a terminal that answers nothing.
  - XTVERSION names vtium (`vtium(VERSION)`) and the query is answered: live preview is on, and the query's answer is the font to give back.
  - XTVERSION names vtium and the query is not answered: `allow-font-ops` is off. Live preview is off, the TUI says so and names the key, and offers to turn it on as a change saved like any other.
  - Anything else, another terminal, a multiplexer such as tmux, an ssh client's terminal or a serial line: the TUI never sends OSC 50, since xterm, for one, would look for an X font of that name. It previews with `preview`'s pictures where the terminal draws kitty graphics or sixel, and with a text summary otherwise (the coverage, the cell, the grid it gives).
  - `set` makes the same check before it writes, and sends nothing to a terminal that is not vtium. Today it writes OSC 50 to any terminal and only reads the reply.
- **While the user browses.** Each font the user highlights is sent as `OSC 50 ; NAME ST` followed by the query, the bytes `set` sends. At most one switch is outstanding: the next is sent when the query's reply to the last comes back, and it names whatever is highlighted then, so holding an arrow key sends one switch per reply and never a queue of stale ones. A reply that names another font means the console refused the switch; the TUI marks that font "kept by the console, see its log" and goes on. There is no countdown that reverts a font the user has not confirmed: the TUI stays in control, and leaving it gives the font back.
- **Sizes.** The console ignores every OSC 50 form but a frit's name (`crates/vtium-term/src/terminal/font.rs`), so until it takes a size the TUI shows what grid each size gives and applies a size only when it is saved. Live sizes need a change to the console, made with ADR-1200 D5 and ADR-1000's `text.font-change` row and their own tests: OSC 50 takes a name with a size, and the query reports both. This record recommends `NAME:size=POINTS`, the way fontconfig writes a size in a font's name, which no frit's name can be mistaken for, since a frit's name holds no colon (ADR-1200 D1). The query's reply to `set` stays the name alone, so `set` keeps working. That change must also let the TUI give back exactly what it found, which the name alone cannot do in one case: when the console draws a built-in font it chose by height, the query answers `unscii-8` or `unifont`, and switching back to that name fixes the built-in font, so a later zoom across 16 pixels would not change it as the automatic choice does.
- **Giving the console back.** Leaving without saving switches the console back to the font the start query named, and then asks the query to check it. The TUI does this on its quit key; on Ctrl-C, which arrives as a byte in raw mode; on SIGINT, SIGTERM, SIGQUIT and SIGHUP, which it holds while the terminal is raw, as `set` does, and handles in its loop; and on a panic, from the guard that puts the terminal's mode back. It cannot after SIGKILL, or once its terminal is gone, as when the console's session ends: the console then keeps the previewed font until the next switch or start, since an OSC 50 switch changes only the live console and `font-name` decides the font at start (ADR-1200 D5).
- **After saving,** the console stays in the saved font, which is the font the saved vtium.conf names for the next start.

### D5: writing

- **Sudo once, at the moment of writing.** The TUI runs as the user from start to end and never as root, so no font is parsed and nothing is written into the user's home as root. When the user saves, it writes the draft, shows the command, `sudo vtium-vitrify apply DIR`, and runs it. sudo asks for the password once, in the TUI's terminal. The TUI leaves its alternate screen while sudo asks, and comes back after. Nothing else in the TUI needs privilege.
- **What a draft holds.** Each frit to write, each frit to remove, and for vtium.conf the whole edited file, with the SHA-256 of each file it replaces as that file was when the draft was made.
- **What `apply` does,** as root:
  1. It reads the draft without following a link, takes only regular files, and bounds each file's size, since root is reading a directory the user owns.
  2. It refuses the draft, writing nothing, when a file it replaces has changed since the draft was made, naming the file, so an edit made meanwhile by hand or by another tool is never lost.
  3. It checks every file before it writes any: each frit with the helper's frit reader, strictly, as the helper reads it; and the edited vtium.conf with `vtium check-config --configdir` naming the draft's directory, the console's own reader, so the verdict before writing is the verdict at the next start, as ADR-1401 D12 has for the head file. A file that fails stops the apply before anything is written, with each problem and its line.
  4. It writes the frits first and vtium.conf last, so vtium.conf never names a frit that is not yet there. Each file is written to a new file in the same directory, owned by root with mode 0644, flushed to disk, and renamed over the old one, and the old one is kept beside it with a `.bak` suffix, one generation, as vtiumctl's `install` keeps what it replaces.
  5. It prints one line a file, `wrote PATH` or `failed PATH` with the reason, and exits 0 when it wrote them all.
- **What a failed write leaves.** A file whose write fails before its rename is left exactly as it was, and the new file is removed. A file renamed before a later file failed stays written, and `apply` names which were written and which were not. No file is ever left half written. The draft stays, so the user can run `apply` again.
- **Editing vtium.conf line by line.** The tool changes only the lines it must, and every other byte, comments, blank lines, order and line endings, stays as it was:
  - A key that is set has the value on the line vtium takes it from replaced in place, with any comment after it kept.
  - A key that is not set but has a commented line, as `#font-size=0` in the shipped example, gets its new line directly below that comment, beside its description.
  - Any other new key is added at the end of the file.
  - `--unset` comments out the line vtium takes the value from, so the key falls back to its default and the line is there to set again.
  - A switch is written as vtium.conf writes switches, `allow-font-ops` or `no-allow-font-ops`.

### D6: the head file stays out

The TUI does not read or edit `heads.conf`. vtiumctl keeps the head file and the devices: detecting them, setting up heads, the trial, and installing the head file (ADR-1401 D12). Once profiles have a file and a head names a profile with its own font, a profile wins over vtium.conf's `font-name` for that head (ADR-1200 D16), so the TUI's save view then says which heads the saved font does not reach.

### D7: what the TUI is built with

- **No TUI library.** The TUI is a small screen layer in vtium-vitrify over rustix's terminal calls, which the crate already links with the features it needs, and the raw-mode guard and signal holding that `set` already has (`src/switch.rs`). It draws with ECMA-48's cursor position, erase and SGR sequences and DEC modes 25 and 1049, which vtium supports and every terminal of the VT100 line has, and decodes keys from the bytes a terminal sends for them, as vtium-term encodes them.
- **The TUI is a state machine apart from its terminal.** Its state takes events (a key, a reply, a resize, a signal) and returns effects (bytes to write, a command to run, a draft to write), so tests drive it without a terminal (D8), and the loop around it does the reading and writing.
- vtium-vitrify is not part of the minspec console binary and is outside its size budget. It ships on every install as the glyph helper, so the TUI's size counts in the install tree that tier 3 measures; a library would add more than the screen layer does.

### D8: how it is judged

- **The state machine,** driven by events in unit tests: every view, the one-switch-outstanding rule, the refusal mark, the size list, and the save view's list of changes.
- **The real binary on a pty,** with vtium-term's terminal answering on the other side, as `tests/switch.rs` drives `set`: the test types keys into the pty, reads the terminal's screen as text, and records every byte the TUI writes. It checks the exact bytes: the three requests at start; `OSC 50 ; NAME ST` and the query for each switch, and no second switch before the first's reply; and the switch back to the start font and its query when the TUI leaves by its quit key, by the Ctrl-C byte, and by SIGTERM. Against a stand-in terminal that answers only DA1, and against vtium-term with font operations off, it checks that the TUI writes no OSC 50 byte. Screen captures at 40 by 30 and 80 by 25 are vtium's own goldens, provisional until the maintainer reviews the layout.
- **Writing,** in a temporary directory standing in for `/etc/vtium` through the tool's path options, never as root: the shipped `vtium.conf.example` with one key changed differs in that one line, and with no change is byte-identical; each placement rule of D5 has its test; a draft whose target changed is refused with nothing written; a check that fails writes nothing; a write that fails before its rename, planted with a directory the test makes read-only, leaves the old file whole. The command the TUI runs for privilege is a seam whose default is `sudo`; the stand-in a test puts in its place runs `apply` with the same path options, so it checks what `apply` checks, and no test runs sudo.
- **Tier 3:** in the guest, on a real VT, a screenshot of the console in the font the TUI previews, and a screenshot after the TUI leaves without saving, which must match the screenshot taken before the TUI started.
- **The VT review:** a guide step, run with `make vt-check`, in which the maintainer opens the TUI, moves through three fonts and sees the console change with each, presses Ctrl-C and sees the start font come back, then saves a font, gives the sudo password once, and reads the one changed line of vtium.conf.

### D9: `new`, `preview` and the TUI

The TUI is where `new` and `preview` live when a person runs them at a terminal. `new`'s guided flow is the TUI's new-frit view, and it ends in a draft and `apply`, which replaces ADR-1200 D4's "one command, run with `sudo`, that writes the frit". `preview`'s pictures are what the TUI shows where live preview is off. Both keep their non-interactive forms as commands, for scripts and packages.

### D10: the banner's tagline

The tagline under the tool's banner says what the tool is for now: the console's fonts and settings, tried on the live console. "vitrified font sets" names the store of prerendered sets that ADR-1200 retired. This record recommends "fonts and settings for the vtium console" for the wide banner and "the console's fonts" for the narrow one. The build that adds the TUI changes both taglines in `src/banner.rs` and their test, the crate's description in `Cargo.toml`, and the usage text's summary line. The tool keeps its name.

### Passages this record changes

These change when this record is accepted, in their owners' changes; this record edits none of them.

- **ADR-1200 D4:** "Each command is non-interactive where it writes" becomes: each command but the TUI is non-interactive, and the TUI writes only through `apply`. The table gains D2's rows, and `new`'s row ends in a draft and `apply` (D9). The basis entry that leaves "the wrappers" to "other people's choice" stands as said; vtium-vitrify ships one wrapper of its own.
- **ADR-1200 D5:** `set` checks the terminal before it writes (D4). The size form, once the console takes it, is that decision's to add.
- **ADR-1401 D12:** vtiumctl's drafts and `install DIR` of frits move to vtium-vitrify's `apply`; vtiumctl keeps the head file. `vtiumctl check` still checks frits through vtium-vitrify's frit reader. The minspec bullet's "a frit is previewed by trying it on the live console through OSC 50 under the same countdown" becomes the TUI's live preview, with no countdown (D4). "`vtium-vitrify` keeps the fonts" gains vtium.conf.
- **ADR-1200 D16:** "`vtiumctl` writes them when asked" of profile files becomes vtium-vitrify (D1).
- **The tracker issue for vtium-vitrify** lists this work.

### For the maintainer's check

These are agent calls the maintainer has not looked at: the command names `fonts`, `config`, `remove-frit` and `apply`; profiles written by vtium-vitrify rather than vtiumctl (D1); and whether the console should put a head back in its configured font when the session that switched it ends, which would close the case D4 cannot.

## Consequences

### Positive

- A user chooses a font by seeing the real console in it, on every build, minspec included, where `preview` can show no picture because the console there draws none.
- Everything the TUI does, a script or a package can do with the same commands, and the TUI's writes go through the same checked path as theirs.
- vtium.conf keeps its comments and order, so a file a person wrote stays theirs after the tool has changed it.
- One sudo prompt, for a command shown first, and no step of browsing or previewing runs as root.
- A file is either the old one or the new one, never half of each.

### Negative

- The console keeps a previewed font when the TUI is killed with SIGKILL or its terminal goes away.
- Live sizes wait for a change to the console's OSC 50, which departs from xterm's form.
- No live preview inside tmux or screen, over ssh from another terminal, or with `allow-font-ops` off.
- vtium-vitrify grows on every install by the TUI and its screen layer.
- Checking vtium.conf needs the `vtium` binary installed beside the tool, which the install tree always has.

### Neutral

- vtium's key table moves into vtium-config, which the console already links.
- `set` stops writing OSC 50 to terminals that are not vtium.
- Profiles, once they have a file, are written by vtium-vitrify.

## Alternatives Considered

- **The TUI in vtiumctl, or as a third binary.** Not taken, by the maintainer: it stays one tool, vtium-vitrify.
- **ratatui, with crossterm or termion beneath it.** Not taken: a widget and layout framework and a second terminal layer beside the raw-mode and signal handling `set` already has, for a screen of lists, on every install including minspec boards.
- **ncurses, or dialog and whiptail.** Not taken: ncurses is a C library, against vtium's rule that its code is Rust with C only at a boundary it must cross; dialog and whiptail are C programs that minspec images may lack, and the tool cannot send OSC 50 between their prompts.
- **Preview by pictures alone,** as `preview` plans. Not taken as the main path: a picture is not the console, and the minspec console draws no pictures. It stays the preview where live preview is off.
- **Preview by writing vtium.conf and restarting the console.** Not taken: each step would need root and a restart, which ends the user's session.
- **Sending OSC 50 to any terminal,** as `set` does today. Not taken: another terminal may act on it, as xterm looks for an X font of that name.
- **A countdown that reverts an unconfirmed font,** as a desktop's display settings do and as ADR-1401 D12 planned for vtiumctl on minspec. Not taken: the TUI stays in control and gives the font back when it leaves, and a countdown would take away a font the user is still looking at.
- **Rewriting vtium.conf from its parsed keys.** Not taken: it would lose every comment and the file's order.
- **Running the whole TUI as root, or asking sudo at each step.** Not taken: the first parses fonts and writes into the user's home as root, against ADR-1200 D14 and ADR-1401 D12; the second asks for a password many times.
- **Editing the head file in the TUI.** Not taken: vtiumctl keeps it (D6).
