# The macro player and live runs

Where this chapter starts: the save of 2026-11-28 at 12:10 activates Terminus: `padnoir · 3 problems`. My-Look draws at 12 pt, and History lists five versions.

A macro is a short text file of keys and checks that vtium-config's player plays on the TUI, as a person would press them. Its steps are a subset of the tape format of VHS, a program that records terminal sessions, so its keys read as their names: `Enter`, `Ctrl+S`, `Alt+L`. `Click "LABEL"` acts on what the screen labels so, through the keys a person would press, and `Expect` checks what the screen shows. A macro plays on a scratch copy unless the run is live. [ADR-1208](../../architecture/font/ADR-1208-vtium-config-s-tui-plays-macros-short-scripts-of-keys-and-checks-in-a-subset-of-vhs-s-tape-format-on-a-scratch-copy-unless-a-live-run-is-asked-for-and-allowed.md) decides the format and the player.

12 pt has turned out larger than the person wants for a whole day, so they settle on 11 pt again. This time they write the steps down as a macro, so the same change can be played on travel, or on another machine.

## The macro

The macro is a file, `cullet-11.vtmacro`. Its first step, `Set Live`, asks for a live run, which changes the real configuration.

```text
# Make My-Look draw Cullet Mono at 11 pt, and save it.
# Run it with: vtium config tui --macro cullet-11.vtmacro --live
Set Live
Alt+L
Expect At looks
Click "My-Look"
Enter
Click "Size"
Enter
Click "11 pt"
Enter
Expect "11 pt"
Ctrl+S
Enter
Expect Saved
```

A live run needs two things: the command line names `--live`, and the macro's first step is `Set Live`. So a macro written for the docs never runs live by accident. With only one of them, vtium-config refuses before the first step and says which is missing. The person runs the macro without `--live` first:

```text
user@padnoir:~$ vtium config tui --macro cullet-11.vtmacro
vtium-config: cullet-11.vtmacro asks for a live run with Set Live, but --live is missing.
user@padnoir:~$ _
```

*Marks: none. The shell, after vtium-config refused the run.*

## A rehearsal on a scratch copy

The person copies the macro to `cullet-11-rehearse.vtmacro` without its `Set Live` line, and plays it with `vtium config tui --macro cullet-11-rehearse.vtmacro`. The player plays on a scratch copy, a copy of the open configuration in a folder that goes when the TUI leaves. It pauses 100 milliseconds between keys, so the person can follow, and any key from the terminal stops it. The top row says "scratch copy" while it plays.

On a scratch copy, Save checks the file and builds the install bundle as it does for real. Then it shows the command it would run and the files that command would change, and counts the scratch copy as saved, so `Expect Saved` passes. The TUI stays open where the macro ended, and the message line says which step that was.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help padnoir · scratch copy · 3 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           11 pt                                                   ◂ ▸
   Color scheme   vga
   vtlook         No filter
   Pointer theme  vtium's own pictures

 11 pt on eDP-1: 10×22 px cells, 192×49 characters. A size applies when vtium starts again.

 Fonts by role                                                   2 fonts missing
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ● active
      ┌ Save, on a scratch copy ────────────────────────────────────────────────────────── × ┐
      │ This run plays on a scratch copy, so Save checks the file and builds the install     │
      │ bundle, then writes nothing. A live run would run:                                   │
      │                                                                                      │
      │   sudo vtium-config apply DIR                                                        │
      │                                                                                      │
      │ which would change /etc/vtium/vtium.conf. The scratch copy now counts as saved.      │
      └──────────────────────────────────────────────────────────────────────────────────────┘
 cullet-11-rehearse.vtmacro ended after line 14: every step passed. The keys are yours now.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Change  [Enter] Choose  [Esc] Back  [t] Try live  [Ctrl+S] Save          [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab. In color, scratch copy is amber.*

Quitting throws the scratch copy away and asks nothing, since nothing of the person's is lost.

### When a step fails

Had the person typed `Expect "11pt"`, with no space, the step would fail. The macro stops there and the TUI stays open, and the message line names the file, the line and what the step expected. F1 opens the rest: the screen and the keys from the start, written as a macro, so pasting them into a file reaches the same screen. `--macro-debug DIR` also writes the screen after every step into a folder of its own.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help       scratch copy ● 1 · 3 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           11 pt                                                   ◂ ▸
   Color scheme   vga
   vtlook         No filter
   Pointer theme  vtium's own pictures

 11 pt on eDP-1: 10×22 px cells, 192×49 characters. A size applies when vtium starts again.

 Fonts by role                                                   2 fonts missing
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ● active








 cullet-11-rehearse.vtmacro:11: Expect "11pt": the screen does not show it. [F1] says more.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Change  [Enter] Choose  [Esc] Back  [t] Try live  [Ctrl+S] Save          [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Size  11 pt  ◂ ▸`. In color, scratch copy and the message are amber.*

## The live run

A live run starts only from a saved configuration, so its Save installs only what the macro changed. padnoir has no change not saved, and the person runs `vtium config tui --macro cullet-11.vtmacro --live`. While the run plays, the top row shows LIVE in the attention color. At line 13, Ctrl+S opens Save's review, the same review as in [chapter 3](03-saving.md).

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help     LIVE ● 1 not saved · 3 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf                                           1 change · fonts unchanged

 @@ [look My-Look]
     13  [look My-Look]
     14  palette = vga
   15 -font-size = 12
   15 +font-size = 11
     16  font-name = cullet-mono

 3 warnings (saving still works): Noto Color Emoji fails the trust rule, the layer
 nerd-symbols names a font that is not installed, and 1 keyboard and 2 pointers are in no path.
 [Alt+P] lists them.

 Each terminal uses the change at its next login. sudo asks for your password once.












────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+S] Save  [w] Whole file  [Alt+P] Problems  [Esc] Cancel                 [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab. In color, LIVE and the warnings' heading are amber.*

Enter at line 14 saves. A live run runs the real commands, and it changes nothing about privilege. sudo asks for the password as it always does, and the player waits while sudo reads the terminal. Before vtium-config runs a real command, it logs one line that names the macro, the step's line and the command. The line goes to the system journal, where sudo writes its own line for the same command.

```text
user@padnoir:~$ journalctl -t vtium-config --since 12:20
Nov 28 12:21:07 padnoir vtium-config[5120]: live macro cullet-11.vtmacro line 14 runs: sudo vtium-co
Nov 28 12:21:07 padnoir sudo[5131]:     user : TTY=tty1 ; PWD=/home/user ; USER=root ; COMMAND=/usr/
```

*Marks: none. The shell, reading the journal after the run.*

`Expect Saved` at line 15 passes, and the macro ends with the TUI open on My-Look's card at 11 pt.

### Who may run live

Any user may play a live macro. An administrator can turn live runs off for the whole machine with `allow-macro-live = off` in `/etc/vtium/vtium.conf`. vtium-config reads that key only from the installed file, under its owner and mode check, so a person who cannot write the file cannot lift the lock. Had padnoir's file turned live runs off, the run would stop before its first step:

```text
user@padnoir:~$ vtium config tui --macro cullet-11.vtmacro --live
vtium-config: live macro runs are off on this machine: allow-macro-live = off in /etc/vtium/vtium.co
user@padnoir:~$ _
```

*Marks: none. The shell, after vtium-config refused a live run.*

### Playing without a terminal

`--macro -` reads the steps from standard input, so a macro can come from a test harness on another machine. With no terminal at all, as under `ssh padnoir vtium config tui --macro - < steps`, the player plays headless on a screen of 100×30 cells. It prints each step's line and its result, and exits with 0 when every step passed and 1 when one failed.

Where it leaves you: the live run's save of 2026-11-28 at 12:21 sets My-Look back to 11 pt: `padnoir · 3 problems`. The console draws Cullet Mono at 11 pt from its next login. That was the change the person first set out to make, and History lists six versions, the newest the file of 12:10.
