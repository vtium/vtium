---
contract: adr/v1
kind: decision
verb: add
capability: glyphs
basis:
  - operator: aaronsb
    level: directed
    said: "something we should consider is a way to drive a short macro command. if we do it right, we could load and run a macro (as a parameter 'vtium-config tui --macro mymacro' or in one of the menus. … The main purpose of this is to write macros in our test harnesss, and we can feed them to vtium and it will attempt to run the path. if it breaks, we could have a debug macro mode that shows the screen (maybe?) for the run, or at least how it got to parts of the application. the macro language could have a delay pragma or something for example, or other pragmas that help in different situations. I don't want to overbuild this, and I wonder if there's a crate that might do this for us already"
    via: the maintainer in conversation with the steering session, 2026-11-27, as the steering session's decisions for that day record it
  - operator: aaronsb
    level: directed
    said: "in a macro we could then send any alt+key or ctrl+key modifier we want"
    via: the same conversation, 2026-11-27
  - operator: aaronsb
    level: directed
    said: "this is good. it resolves several issues I have raised in the backlog about programmatically driving vitum, takes on the problem of evaluating all the menu choices, and lets us remotely drive it."
    via: the same conversation, the evening of 2026-11-27, accepting the steering session's proposed shape for macros (a subset of VHS's tape syntax, one step a line, one player against the TUI's state machine, a scratch copy for Show me, a debug mode that names the key path to a failure, and a parser crate of vtium's own)
  - operator: aaronsb
    level: directed
    said: "we could combine this with that ascienma for example - making it straightforward for documentation on the github pages or as a demo"
    via: the same conversation, the evening of 2026-11-27
  - operator: aaronsb
    level: directed
    said: "we need to have a debug way to actually run the real commands not in a temp copy. this should be somewhat not as obvious to run, and it's part of our test harness"
    via: the same conversation, the evening of 2026-11-27
  - operator: aaronsb
    level: directed
    said: "let's not hide it too much, it can be a --parameter. someone might want it"
    via: the same conversation, the evening of 2026-11-27, on how to start a live run
  - operator: aaronsb
    level: directed
    said: "also it's more honest that it's a path that can be automated. I'd almost prefer a way to lock it out instead."
    via: the same conversation, the evening of 2026-11-27, which led to the allow-macro-live key in /etc/vtium/vtium.conf
  - operator: aaronsb
    level: guided
    said: "Alpha.13 holds the macro player: the format's crate, --macro, --live and the allow-macro-live lock. Help's Show me and the asciinema recordings stay for later. One release at a time."
    via: the maintainer's choices through the question tool and in conversation on alpha.13's scope, as the steering session's decisions of 2026-11-27 record them
    paraphrase: true
  - evidence: "Read by this record's agent on 2026-11-27 at integration 794dd89ba: crates/vtium-config/tests/scenarios/README.md, sim.rs, state.rs and explore.rs, and src/tui/input.rs. The scenario tests drive the TUI's state machine, App::event in src/tui/model.rs, with no terminal and no pty; Sim::press hands it one key, a mouse event or a resize, and answers every effect at once through a stand-in desk and a stand-in terminal, so the state after a key is the state the TUI rests in. A failure names the scenario by the kind of terminal, the grid and the keys from the start, as in `vtium 80×25: Up Up Enter Down r q l`. The use-case runner follows steps in a person's words: it finds a label on the screen and presses the keys a person would press to reach it, and its checks read the screen's words with one space between each. The TUI's key type, Key in src/tui/input.rs, names the arrows, PageUp, PageDown, Home, End, Enter, Tab, BackTab, Backspace, Ctrl-C as Interrupt, Escape, a mouse event, a character, and Ctrl with the letters D and U only; the decoder drops Alt and a key. The abstract state names the node of ADR-1403's interface tree that the TUI is in, such as fonts, looks or looks.look, and whether changes wait for Save."
  - evidence: "The steering session's survey of 2026-11-27, as its decisions for that day record it: VHS's tape language is the familiar one, but VHS is written in Go; the Rust crates that parse tapes (xacli-testing) or drive a pty (expectrl, rexpect, termlens) are small or cover only the pty's side. No crate does the whole job."
  - precedent: ADR-1206
  - precedent: ADR-1403
  - precedent: ADR-1401
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-27
deciders:
  - aaronsb
related:
  - ADR-1201
  - ADR-1205
  - ADR-1206
  - ADR-1401
  - ADR-1403
observable:
  - "`vtium config tui --macro FILE` plays the macro on the screen at a pace a person can follow, and any key stops it with a line that names the step it stopped at"
  - "A macro that does not parse is refused before its first step, with the file, the line and what the line should hold, and the exit status is 2"
  - "A macro whose Expect fails stops at that step; the message names the file, the line, what was expected and the keys from the start, and the exit status is 1"
  - "`--macro-debug DIR` writes the screen after each step, each Screenshot, and on a failure the step, the screen and the keys from the start, which played as a macro reach the same screen"
  - "Without `--live`, a macro's Save says which command it would run and changes nothing under /etc; the top row says that the TUI is on a scratch copy"
  - "`--live` with a macro that does not start with `Set Live`, or `Set Live` without `--live`, refuses before the first step"
  - "With `allow-macro-live = off` in /etc/vtium/vtium.conf, `--live` refuses with one line naming the key and the file, and the macro does not run"
  - "During a live run, the top row shows LIVE, and the journal holds one line for each real command, naming the macro, its line and the command"
  - "`vtium config tui --macro -` over ssh with no terminal plays headless, prints each step's result, and exits 0 when every step passes"
  - "The scenario tests play every macro under tests/scenarios/macros/ against the state machine, and a failure names the macro, the line and the keys"
  - A macro step `Alt+[` is refused before the first step, with the line and the reason that a terminal sends it as ESC [, and `Ctrl+I` reaches the TUI as Tab
  - On a machine with no journal, a live run writes each real command's line to standard error before the command runs, and to live.log in debug mode
  - A live run with changes that are not saved refuses to start, with one line saying to save or revert them first
---

# ADR-1208: vtium-config's TUI plays macros, short scripts of keys and checks in a subset of VHS's tape format, on a scratch copy unless a live run is asked for and allowed

## Summary

- **Decided by the maintainer (2026-11-27):** vtium-config's TUI plays macros. A macro is a short text file in a subset of the tape format of VHS, charmbracelet's tool for scripted terminal recordings, with one step on each line. It presses keys by name with any modifier, types text, clicks a named button, tab or item, checks the screen, and paces itself for a person watching. One player plays a macro against the TUI's state machine, the one the scenario tests drive. `vtium config tui --macro FILE` plays a macro on the screen, and any key stops it, and `--macro -` reads the steps from standard input as they arrive. A macro plays on a scratch copy of the configuration, so it changes nothing on the machine. A live run, with `--live` and a macro that starts with `Set Live`, plays on the real configuration and runs the real commands. The new vtium.conf key `allow-macro-live`, on by default, lets an administrator lock live runs out. Debug mode writes the screen after each step and the keys that reached a failure. The format's parser is a small crate of vtium's own and adds no dependency. Help's Show me, asciinema recordings and tier 3's runner use the same player later (D9).
- **Proposed by this record's agent, for the maintainer to confirm:** the grammar (D2), a click acting through the keys a person would press (D3), the file type `.vtmacro` and the crate's name `vtium-config-macro` (D7), headless play when vtium-config has no terminal (D4), the option `--macro-debug DIR` (D6), the limits on a macro's size, counts and pauses (D1, D4), the pause of 100 milliseconds between keys and the exit statuses 0, 1 and 2 (D4), the words "scratch copy" in the top row and a refused live run when vtium.conf cannot be read (D5), and a line in the system journal for each real command of a live run (D5).
- **Settled by the steering session after a fresh review (2026-11-27), in the maintainer's absence:** a key reaches the TUI as the TUI's decoder makes it from the bytes a terminal sends, and a key that no terminal can send fails when the macro is parsed (D3); the spellings that are vtium's own, bare seconds after `Sleep`, backtick quotes, and the refusal of `Key@time` (D1, D2); at most 10,000 steps, which bound `--macro -` in place of the 1 MiB of a file (D1); the rule for a Screenshot's name (D2); any user may play a live run, which refuses to start while changes are not saved and always reads its lock from `/etc/vtium/vtium.conf` (D5); and the log of a live run's commands on a machine without a journal, each line logged against the step that ran it (D5). Later the same night, the steering session added `Click Mouse "LABEL"`, a click sent as a mouse click at the label's cell, for the tests of the mouse (D2).
- **Trades away:** A macro drives the state machine, so it never tests the decoder that turns a terminal's bytes into keys, which keeps its own tests. A macro names screens by their labels, so a change of wording breaks the macros that use the old label, as it already breaks the use cases. A machine that allows live runs lets any user play a live macro. Its privileged steps, Save's apply and Install as a system font, still go through sudo, so a live macro does only what the user who plays it could do by hand.
- **One-way?** No. The format is a few lines of grammar in one small crate, and the player sits beside the state machine. Once the tests, the docs and tier 3 hold many macros, renaming a step costs an edit of each macro, so the step names are the costly part.
- **Probes:**
  - *Confident (a click acts through keys):* `Click "Save"` makes the player press the keys a person would press to reach Save, such as its menu letter, and not a mouse click at its cell. So a macro works on every screen and in every terminal, and the keys it reports on a failure play back as a macro. Is that what you want? Lean: yes, because the use-case runner already finds a label and presses its keys. Against: a plain click then never tests the TUI's mouse handling, which only `Click Mouse` reaches (D2, D3).
  - *Not confident (playing with no terminal):* when vtium-config has no terminal, as under `ssh host vtium config tui --macro - < steps`, the player plays headless at 100×30 and prints each step's result. Is that what you want? Lean: yes, because it makes remote driving one command, and the state machine needs no terminal. Against: it is a second way to run the TUI, beyond the visible play you asked for (D4).
- **Inversion:** The decision sits between two ends. At one end, a test drives the TUI through a pty, typing bytes and reading the terminal back, as an expect script does, and the test knows nothing of the TUI's states. At the other, a test calls the state machine's Rust functions directly, which only a Rust test can do. A macro is plain text that anyone can write, and the player hands its steps to the state machine as keys, so it reads the TUI's own state.

## Context

vtium-config's scenario tests already drive the TUI without a terminal. The explorer presses every key in every state, and the use-case runner follows the steps of a task in a person's words (ADR-1403, ADR-1206). Both live inside the test crate, and only a Rust test can use them. The maintainer asked for a way to drive the TUI from a short script, for tests first, and also for a person, for Help, and for another machine. The maintainer wanted a debug mode that shows how the run reached a failure, and asked not to overbuild it.

VHS is a tool that records terminal demos from a tape file, a script of `Type`, key and `Sleep` commands, one on each line. ratatui uses it for its own widget demos, so its syntax is familiar to people who write terminal programs in Rust. VHS itself is a Go program that drives a real terminal and writes a GIF, and no Rust crate parses tapes and plays them against a state machine.

## Decision

### D1: a macro is a short text file of steps

A macro is a UTF-8 text file with one step on each line. Its file type is `.vtmacro`, as vtium's own file types are `.vtfrit`, `.vtlayer` and `.vtlook`. The steps are a subset of VHS's tape format: the key names, `Type`, `Sleep`, `Screenshot` and `Set` keep VHS's spelling. `Click`, `Click Mouse`, `Expect`, `Expect Not` and the function keys are vtium's own, because VHS has no use for them, and so are the key names `Esc`, beside VHS's `Escape`, and `Home`. A macro is therefore not a VHS tape, and VHS cannot play one. A step that VHS has and this format lacks, such as `Output`, `Hide` or `Wait`, does not parse, so a macro never silently skips a line its author wrote. VHS's speed after a step's name, as in `Type@500ms "cullet"` or `Down@100ms 3`, does not parse either, and the line that refuses it says that the player sets its own pace (D4).

A line holds at most 4,096 bytes, a macro at most 10,000 steps, and a file at most 1 MiB, each a named constant in the format's crate. `--macro -` reads a stream, which the step count and the line length bound in place of the file's 1 MiB, and the parser keeps one line of it at a time. The step count and its size are the steering session's call. A key's count runs from 1 to 100, and a `Sleep` lasts at most 60 seconds, so a typo cannot press a key a million times or stall a test for a day.

### D2: the grammar

| Step | Examples | What it does |
|---|---|---|
| A key by name | `Enter`, `Escape` (or vtium's own `Esc`), `Tab`, `Space`, `Backspace`, `Up`, `Down`, `Left`, `Right`, `PageUp`, `PageDown`, vtium's own `Home`, `End`, vtium's own `F1` to `F12` | Presses the key once, as D3 says. |
| A key with modifiers | `Ctrl+S`, `Alt+L`, `Shift+Tab`, `Alt+Shift+L` | Presses the key with Ctrl, Alt or Shift held, in any combination. The key after the last `+` is a key name or one character. A letter there names the key, and only `Shift` makes it a capital: `Alt+L` sends `ESC l`, and `Alt+Shift+L` sends `ESC L`. The TUI matches an Alt letter regardless of case (ADR-1206 D2), so both open Looks. The TUI gets what a terminal sends for the chord, as D3 says, and `Ctrl+C` sends the byte the TUI reads as the interrupt. |
| A count | `Down 3`, `Backspace 5`, `Tab 2` | Presses the key that many times, from 1 to 100. |
| `Type "TEXT"` | `Type "cullet"`, `Type 'say "hi"'`, ``Type `it's` `` | Types each character of TEXT as its own key. TEXT is in double quotes, single quotes or backticks, and holds no quote of its own kind, as in VHS. There are no escapes. The other steps that take text quote it the same way. |
| `Click "LABEL"` | `Click "Looks"`, `Click "Save"`, `Click "cullet-mono"` | Acts on the label as a click does: opens the tab or the menu, presses the button or the menu item, or highlights the list item or the box (D3). |
| `Click Mouse "LABEL"` | `Click Mouse "Save"`, `Click Mouse "Looks"` | Sends a mouse click at the character cell of the label, which the player finds as it finds a `Click`'s label (D3), so a macro can test the TUI's mouse handling. A screen that does not turn mouse reporting on gets no click, as with a person's mouse. `Click "Mouse"` is still a plain click on a label that reads Mouse. This step is the steering session's call. |
| `Expect "TEXT"` | `Expect "11 pt"` | The screen shows TEXT, read with one space between each word, so a text that wraps across rows still matches. Otherwise the run fails at this step. |
| `Expect Not "TEXT"` | `Expect Not "problems"` | The screen does not show TEXT. |
| `Expect STATE` | `Expect At looks.look`, `Expect Saved`, `Expect Unsaved`, `Expect Exited 0` | The TUI is in that node of ADR-1403's interface tree, holds no change that is not saved, holds one, or has exited with that status. |
| `Sleep TIME` | `Sleep 500ms`, `Sleep 2s`, `Sleep 2`, `Sleep 0.5` | Waits that long before the next step in visible play (D4). A bare number is seconds, as in VHS. Tests and headless play ignore it. |
| `Screenshot NAME` | `Screenshot after-save` | In debug mode, writes the screen as text to `NAME.txt` (D6). Otherwise it does nothing. NAME is one plain file name of the characters `A` to `Z`, `a` to `z`, `0` to `9`, `.`, `_` and `-`, and does not start with a dot. The NAMEs that start with `step-`, and `failure` and `live`, belong to debug mode's own files, so the parser refuses them when it parses the macro. It also refuses a NAME that an earlier Screenshot of the macro used, which would overwrite that file. |
| `Set Live` | `Set Live` | Asks for a live run (D5). It must be the first step, and only comments and blank lines may come before it. |
| A comment | `# the Looks tab` | A line whose first character other than a space is `#`. A blank line is allowed. |

> Edited 2026-11-28: a fresh review read `Alt+L` as the character L, which sends `ESC L`, so the modifiers' row says that a letter after a modifier names the key. The row read: "The key after the last `+` is a key name or one character. The TUI gets what a terminal sends for the chord, as D3 says: `Shift` and a letter sends the capital letter, and `Ctrl+C` sends the byte the TUI reads as the interrupt."

> Edited 2026-11-28: the steering session's call, in the list of decision 137. The table gains the `Click Mouse "LABEL"` row, and D1 counts `Click Mouse` among vtium's own steps, so that the tests of the TUI's mouse handling can be macros too. The table had no such row, and the Probes said that a macro never tests the mouse.

The parser reads the whole file before the first step plays, so a typo on the last line stops the run before anything happens. It names the file, the line and what the line should hold, as in `cullet-11.vtmacro:7: Clik is not a step. A step is a key, Type, Click, Expect, Sleep, Screenshot or Set Live.` With `--macro -`, the player parses each line as it arrives, and a line that does not parse ends the run at that line.

**Two macros.** This macro does the maintainer's task: the console draws Cullet Mono at 11 pt, and the change is saved. The labels are those of the rebuilt screens, as the mock-up catalog shows them, so the build's own copy of the macro uses the labels the screens finally show.

```
# Make the console draw Cullet Mono at 11 pt, and save it.
# Run it with: vtium config tui --macro cullet-11.vtmacro --live
Set Live
Alt+L
Expect At looks
Click "console"
Enter
Click "Font set"
Enter
Click "cullet-mono"
Enter
Click "Size"
Enter
Click "11 pt"
Enter
Expect "cullet-mono"
Expect "11 pt"
Ctrl+S
Enter
Expect Saved
```

Without its `Set Live` line, the same macro rehearses the task on a scratch copy, and its Save says which command it would run (D5). This second macro is a test that expects a refusal. It checks that the built-in font cannot be deactivated and that the attempt changes nothing.

```
# The built-in font is always active, so Space on it changes nothing.
Alt+N
Expect At fonts
Click "built-in"
Space
Expect "always active"
Expect Saved
```

### D3: the player

The player plays a macro against the TUI's state machine. It hands each key to the state machine as an input event, as the scenario tests' `Sim::press` does. Before each step, the player waits until the TUI rests, which is when every effect of the step before has its answer and no reply from the terminal is due. So an `Expect` reads the state the TUI rests in. While a real command reads the terminal, such as sudo asking for a password, the player waits too. A step after the TUI has exited fails, unless it is an `Expect Exited`.

**A key reaches the TUI as a terminal sends it.** For each key step, the player takes the bytes that a vtium console sends for that key, as vtium-term encodes keys, and hands the TUI exactly what the TUI's decoder makes from those bytes. So the TUI gets from a macro only what it could get from a person at a terminal. Some chords arrive as another key, because a terminal sends the same byte for both: `Ctrl+I` arrives as Tab, `Ctrl+M` as Enter, `Ctrl+H` as Backspace and `Ctrl+[` as Esc. A terminal cannot send some chords as a key at all. Alt and a key sends ESC and the key, so `Alt+[` opens a control sequence. `Alt+Shift+O`, `Alt+Shift+P`, `Alt+]` and `Alt+Shift+_` begin SS3, a DCS, an OSC and an APC, and still arrive as Alt chords, because the decoder reads ESC and one of those bytes at the end of a read with no reply due as Alt and the key (ADR-1206 D2). A step whose key the decoder does not make into a key fails when the player parses the macro, before the first step plays, and the line names the key and the reason, as in `nav.vtmacro:4: Alt+[ cannot be pressed: a terminal sends it as ESC [, which opens a control sequence.` So a macro never presses a key that silently does nothing. This rule is the steering session's call.

> Edited 2026-11-28: ADR-1206 D2 now reads ESC and `O`, `P`, `]` or `_` at the end of a read as an Alt chord. The sentence read: "Alt and a key sends ESC and the key, so `Alt+[` opens a control sequence, and `Alt+Shift+P`, `Alt+]` and `Alt+Shift+_` open a DCS, an OSC and an APC, which the decoder drops."

**A click acts through the keys a person would press.** `Click "LABEL"` looks for the label on the screen, in this order: the items of an open menu, the buttons of the action bar, the menus and tabs of the top row, and then the lines and boxes of the viewer. A menu, a tab, a button or a menu item matches when its label equals LABEL. A line of the viewer matches when it begins with LABEL. The first place that holds a match decides. Two matches there fail the step, and the failure names both. No match anywhere fails the step too. The player reads the screen's regions from the TUI's state, never by reading characters back off the terminal. Once the player finds the label, it presses the keys that reach it: a tab's or a menu's Alt letter, a button's key, a menu item's letter, or Up and Down until the line is highlighted, and Left and Right for a box in the Flow tab. A menu item or a button that is greyed out is pressed all the same, as a person may press it, and the TUI shows its reason, which an `Expect` can check. The use-case runner of the scenario tests reaches a label in the same way. That code moves from the test crate into the TUI, beside the player, and the runner calls it there.

A click works through keys for three reasons. Keys reach every screen in every kind of terminal, where the mouse reaches only the screens that turn mouse reporting on. The keys the player pressed form the key path of a failure, which plays back as a macro (D6). And a recording made from a macro shows the keys a person would press.

### D4: playing a macro from the command line

`vtium config tui --macro FILE` starts the TUI as usual and then plays FILE on the screen. The player pauses 100 milliseconds between keys, so that a person can follow, and a `Sleep` step waits on top of that. Any key from the terminal stops the macro, and the TUI does not act on that key. The TUI stays open where the macro stopped or ended, and a line on the message row says which step that was, so the person carries on by hand. A failed step stops the macro in the same way, and its line names the failure (D6).

`--macro -` reads the steps from standard input as they arrive, so a macro can come from a test harness on another machine. The TUI then draws on the controlling terminal, `/dev/tty`, and reads the key that stops the macro from it.

**Playing with no terminal.** When vtium-config has no controlling terminal, as under `ssh host vtium config tui --macro - < steps` with no terminal on the far side, the player plays headless. It plays against the state machine with a screen of 100×30 cells, the use cases' default, in the kind of terminal that is not a vtium console. It ignores `Sleep`, prints each step's line and its result on standard output, and prints the failure report of D6 at a failure.

**The exit status.** vtium-config exits with status 0 when every step passed, with 1 when a step failed or the run was refused, and with 2 when the macro does not parse or the options are wrong, as vtium-config's other usage errors do. A live run that names `--live` without `Set Live`, or `Set Live` without `--live`, is refused, and so is one that `allow-macro-live` locks out (D5), so each exits 1. `--live` without `--macro` is a wrong option, and exits 2. These statuses are the steering session's call (added 2026-11-28). In visible play, the status is the one the macro left when the person quits the TUI.

### D5: a scratch copy, unless the run is live

**A macro plays on a scratch copy by default.** The scratch copy is a copy of the open configuration document and of the drafts, in a temporary folder that belongs to the run and goes when the TUI leaves. The installed files under `/etc` are read as they are and never written. On a scratch copy, Save runs the check and builds the install bundle, as it does for real. Then it shows the command it would run and the files that command would change, and marks the scratch document saved, so `Expect Saved` holds. Try live shows the font set or look it would try, and Install as a system font shows its command, and neither one acts. While the TUI is on a scratch copy, the top row says "scratch copy", through the role `scratch-copy` (ADR-1206 D4), where a live run shows LIVE. The person can look around once the macro ends, and quitting throws the copy away without a question, because nothing of the person's is lost.

**A live run plays on the real configuration.** It runs the real commands: Save through sudo and `vtium-config apply`, Try live on the console, and Install as a system font. A live run needs two things, so that a macro written for the docs or for Help never runs live by accident. The command line names `--live` beside `--macro`, and the macro's first step is `Set Live`. When only one of them is there, vtium-config refuses before the first step, with a line that says which one is missing. `vtium config tui --help` and the man page list `--live`, with one plain line on what it does.

**A live run starts from a saved document.** A live run refuses to start while the document holds changes that are not saved, with one line, as in `vtium-config: the configuration has changes that are not saved: save or revert them before a live run.` So a live run's Save installs only what the macro changed. This is the steering session's call.

**Who may play a live run.** Any user may play a live macro, which is the steering session's call. Live mode changes nothing about privilege, as the paragraph on what a live run says below decides.

**An administrator can lock live runs out.** vtium.conf gains the key `allow-macro-live`, a switch at the top level, on by default, beside `allow-font-ops` and `allow-window-ops`. vtium ignores the key, and vtium-config reads it from the installed `/etc/vtium/vtium.conf` under the owner and mode check of ADR-1401 D10, never from a draft, an option or the environment. It reads the lock from that file even when `--configdir` names another folder, so a folder of the person's choice cannot lift the lock, which is the steering session's call. So a person who cannot write vtium.conf cannot lift the lock. With the key off, `--live` refuses with one line, as in `vtium-config: live macro runs are off on this machine: allow-macro-live = off in /etc/vtium/vtium.conf.`, and the macro does not run. A vtium.conf that vtium-config cannot read under the check also refuses live runs, because the lock's state is then unknown. A missing vtium.conf leaves the key at its default.

**A live run says what it does.** While a live run plays, the top row shows LIVE in the attention color, through the role `live` (ADR-1206 D4). vtium-config logs one line for each real command, before it runs the command, naming the macro, the line of the step that ran it, and the command. A command is logged against the step whose key made the TUI run it. In the first macro of D2, `Ctrl+S` on line 18 opens Save and `Enter` on line 19 confirms it, so the line reads `live macro cullet-11.vtmacro line 19 runs: sudo vtium-config apply DIR`, with the real folder in place of DIR. The line goes to the system journal, through the system's log socket, where the machine has a journal. The journal is where an administrator looks for what changed the machine, and sudo writes its own line there for the same command. On a machine with no journal, such as a container or a machine without systemd, the line goes to standard error. Debug mode writes the same lines into its folder in either case (D6). vtium-config never runs a real command that it has not logged: when it can write the line nowhere, the step fails and the command does not run. A live run does not refuse to start on a machine without a journal, because vtium-config is for every machine that vtium runs on. How a live run logs without a journal is the steering session's call. Live mode changes nothing about privilege: sudo still asks for a password unless the machine is set up not to, as tier 3's VM is, and apply's checks still hold. Agents that work on the maintainer's desktop never run a live macro, as the maintainer decided.

> Edited 2026-11-28: ADR-1206 D4 made `live` the attention color and added `scratch-copy`, which a fresh review found missing. This paragraph said "through the top row's role for it", the paragraph on the scratch copy named no role, and D10 said "drawn through the top row's roles".

### D6: debug mode and the failure report

`--macro-debug DIR` turns debug mode on. vtium-config creates DIR and refuses a DIR that already exists, so one run's files never mix with another's. After each step it writes the screen as text, one line for each row, to `step-NNNN.txt`, numbered by the macro's line. Each `Screenshot NAME` writes `NAME.txt`, under D2's rule for NAME. A live run's log lines go to `live.log`, whether or not the machine has a journal, and a failure writes `failure.txt`.

**A failure names its step and the keys that reached it.** The report gives the file and the line, the step, what it expected, the screen, and the keys from the start. The keys from the start list every key the player pressed, with each `Click` written as the keys it pressed, in the macro's own syntax. So the key path, pasted into a macro, reaches the same screen from the same start, in the way the scenario tests name a failure by its keys. For example:

```
refuse-builtin.vtmacro:6: Expect "always active": the screen does not show it.
Keys from the start: Alt+N Down Down Space
```

The report goes to the message row in visible play, to standard output in headless play, and into the test's failure in the scenario tests, whether or not debug mode is on.

### D7: the format's crate

The parser is a crate of vtium's own, `vtium-config-macro`, under `crates/`. It reads a macro's text into steps, each with its line, and refuses a line with the line's number and what it should hold. It also writes steps back as text, which the key path of a failure uses. It knows nothing of the TUI: its keys are names with modifiers, and the player turns them into the TUI's key type. The crate depends on nothing beyond Rust's standard library, so the macros add no dependency to vtium. A macro may come over ssh from another machine, so the parser treats its input as untrusted. It never panics, it holds to D1's limits, and a fuzz target runs it. A property test checks that the text the crate writes from steps parses back into the same steps.

> Edited 2026-11-28: the crate was built as `vtium-config-macro`, the name in the steering session's brief for it. This record named it `vtium-macro` here, in the Summary and twice in D10.

### D8: the scenario tests play macros

The scenario tests gain a folder of macros, `tests/scenarios/macros/`, and one test plays each against the state machine with the stand-in desk and terminal of `sim.rs`. A failure names the macro, the line and the keys from the start. The two macros of D2 are the first ones, the first one without its `Set Live` line, since a test never runs live. One more test plays a macro headless through vtium-config's own binary, with no terminal, and checks its output and exit status (D4). The explorer, the walk and the use cases stay as they are. A use case speaks in a person's words and becomes a help page, while a macro speaks in keys and labels and drives the TUI. Both reach a label through the same code (D3).

### D9: what comes later

These use the same player and need nothing more from this record. Each waits for its own work, after alpha.13.

- **Help's Show me.** Help becomes navigable pages, packed in a small crate of their own, and each page can carry a few Show me macros. A Show me plays on a scratch copy and never saves or runs an action, so it cannot change the machine, and Help never offers a live run.
- **Recordings for the docs.** Playing a macro can write an asciinema recording, the asciicast v2 format: a JSON header, then one line for each write with its time and its bytes. vtium-config writes every byte of its screen through its own backend, so the player records exactly what it sends, and the times come from the macro's `Sleep` steps rather than a clock. So the same macro always yields the same recording. A make target turns the docs' macros into recordings, the site's pages embed them, and a check fails when a recording no longer matches the TUI, as the specimens' check does.
- **Tier 3's runner.** The work that gives tier 3 a format for the steps of its runs against a real console uses macros, played live inside tier 3's VM, where checking what really happened after each step is the point.
- **The map of the TUI's screens, and demonstrations.** The browsable map of vtium-config's choices can give each choice a macro that reaches it, and the scripted demonstrations and the demo image can play macros.

### D10: what changes in the code and in other records

- **vtium-config's command line.** `vtium config tui` takes `--macro FILE`, `--macro -`, `--live` and `--macro-debug DIR`. `--help` and the man page give each one plain line.
- **vtium-settings's key table** gains `allow-macro-live`, a switch, on by default, at the top level only. The page `config.md`, which the key table generates, then documents it.
- **The TUI's key type** carries a set of modifiers, Ctrl, Alt and Shift in any combination, and a key: a named key, a function key from F1 to F12, or a character. The rebuilt TUI's decoder will read Alt chords, F10 and Ctrl+S for its menus once ADR-1206's amendment for the rebuilt TUI lands, in the same batch as this record. alpha.13's Help on F1, Find on Ctrl+F, and undo and redo on Ctrl+Z and Ctrl+Y need the same keys.
- **The player** encodes each key step as vtium-term does, decodes the bytes with the TUI's decoder, and refuses a step whose key the decoder does not make into a key, when it parses the macro (D3).
- **The label finder** moves from the use-case runner into the TUI, where the player and the runner both call it (D3).
- **The update loop** takes the player's steps as a second source of events beside the terminal's input, and lets a key from the terminal stop the player.
- **The top row** gains the facts "scratch copy" and LIVE, drawn through the roles `scratch-copy` and `live` of ADR-1206 D4.
- **vtium-config** gains a writer for one line to the system journal, through the system's log socket, with no new dependency, and writes the line to standard error where the machine has no journal (D5).
- **The new crate** `vtium-config-macro`, and its fuzz target.
- Once vtium-config is split into crates, the player moves with the TUI, and `vtium-config-macro` stays a crate of its own.

## Consequences

### Positive

- A test of the TUI can be a text file, so a person can write one without Rust, and a harness on another machine can send one over ssh.
- A failure names its line and the keys that reached it, and the keys play back as a macro, so a failure reproduces with one command.
- The same file drives a test, a person's demonstration, and later a help page, a recording and tier 3's runner, through one player.
- An administrator decides whether a machine allows live runs, in the file that already holds the machine's other permissions.

### Negative

- vtium maintains a parser, a player, a journal writer and a fuzz target of its own.
- A macro tests the state machine and never the decoder, so a fault in how a terminal's bytes become keys stays a matter for the decoder's own tests.
- A macro names labels, so every change of wording on a screen means editing the macros that click that label.
- Headless play is a second way to run the TUI, which the walk and the use cases do not reach, so it needs a test of its own (D8).

### Neutral

- The format stays a subset of VHS's syntax, so a person who knows VHS reads a macro at once, but a VHS tape needs editing before it plays here.
- `allow-macro-live` is on by default, so a fresh install allows live runs, and only an administrator who turns the key off changes that.

## Alternatives Considered

- **VHS itself.** VHS plays tapes against a real terminal and writes a GIF, a video or a text file of the screens. It is a Go program, so it would add a toolchain to vtium's build, and it reads only the terminal, never the TUI's state. A macro needs `Expect` on the TUI's state and a click by label, which VHS does not offer.
- **A pty driver, such as expectrl or rexpect.** Such a crate types bytes into a pty and matches what comes back. It tests the decoder as well, but it reads the TUI only as characters on a terminal, and it covers only the pty's side of the job. The scenario tests already rest on the state machine, and a macro keeps to the same ground.
- **A tape parser from crates.io, such as xacli-testing.** It is small and little used, and it parses VHS's full syntax, of which a macro needs a subset with additions of vtium's own. A parser of a few hundred lines in vtium's own crate costs less than a dependency to audit and follow.
- **The use cases' steps as the macro format.** A use case speaks in a person's words, such as `Choose New look.`, and becomes a help page. A macro must also press any key with any modifier, and it must play from standard input over ssh, where a person's sentences are harder to write and to check. The two formats share the code that finds a label.
- **A mouse click at the label's cell, for every `Click`.** That would test the mouse handling, but mouse reporting is on only on some screens, and a key path of clicks at cells breaks whenever the layout moves. The keys a person would press keep working while the labels stay. So `Click` presses keys, and `Click Mouse` sends the mouse click beside it, for the tests of the mouse (D2).

> Edited 2026-11-28: the steering session's call, in the list of decision 137, added `Click Mouse` (D2). This alternative read "A mouse click at the label's cell", and was rejected for every click.
- **A live run behind one option alone.** With `--live` alone, a macro written for the docs could run live through a slip of the command line. With `Set Live` alone, the person running the command could miss that the macro asks for it. Both together make a live run a choice of the macro's author and of the person who runs it.
