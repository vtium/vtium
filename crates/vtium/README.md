# vtium

The binary: the configuration and command line, the terminfo entry and the XTGETTCAP table generated from it, the session's command and environment, the respawn policy, the rules the terminal follows, and the seat wired to them through vtium-platform's loop. The rules are functions of what they are given, time included, so their tests need no VT, display or pty. The seat takes the time from the loop too: `tests/boot.rs` boots it with the fake VT, a video device in memory and a real child on a real pty.

| Path | What it is |
|---|---|
| `src/config/keys.rs` | The key table: every key and option, its type, default, bound, reader and source |
| `src/config/` | Reading `kmscon.conf`, `<seat>.seat.conf` and the command line through the table |
| `src/termcap.rs`, `src/termcap/table.rs` | The XTGETTCAP table, generated from `install/kmscon.ti` |
| `src/start.rs` | What start-up takes from the configuration: the VT, the switch to it, the keymap, the key repeat and pointers |
| `src/child.rs` | The session's command and environment |
| `src/respawn.rs` | Whether the session's command starts again |
| `src/wait.rs` | The start's wait for a display |
| `src/blink.rs`, `src/font.rs`, `src/pointer.rs` | The blink timers; the font's fallback, size, sprite policy and missing glyph; the pointer guard |
| `src/sync.rs` | When the terminal presents: the mode 2026 hold and coalescing without it |
| `src/grab.rs` | The terminal's grabs, third in the key order |
| `src/layout.rs` | The grid and each display's font height with several displays, and the zoom and rotation steps |
| `src/term.rs` | The session's terminal as the pty's consumer, and a view per display that draws it |
| `src/video.rs` | A video device behind one trait: drm2d, fbdev, or a test's stand-in |
| `src/app.rs` | The seat: the VT's events, the devices, the keys, the pointer's selection, copy and paste, the blank, the child, the exit |
| `src/logger.rs` | The log on stderr |
| `src/check.rs` | `vtium check-config`, and the warnings the start gives beyond the reports |
| `src/warn.rs` | Where the operator's warnings go: the configuration's reports and a font's partial ranges |
| `src/hook.rs` | The panic hook, which puts the VT back before the panic prints |
| `src/main.rs` | Start-up and the loop |
| `tests/check_config.rs` | `vtium check-config` run as a binary: its warnings, exit status and help, and with strace on the machine, that it opens nothing but the configuration |
| `tests/panic_hook.rs` | The panic hook's order, in a process of its own |
| `tests/boot.rs` | The seat booted with no hardware: output drawn, keys, a VT switch, the blank and what counts as activity, the respawn limit, DECARM, selection, copy and paste with the pointer, the font's policy, the wait for a display and the session with none, the exit |
| `install/kmscon.conf.example` | The example configuration, generated from the key table |
| `install/kmscon.ti` | The terminfo entry, installed compiled with `tic -x` |
| `install/kmsconvt@.service` | The VT unit: vtium in place of getty on a VT, which `make install-tree` installs and tier 3's guest starts |

`docs/config.md` describes every key. `VTIUM_BLESS=1 cargo test -p vtium` rewrites the three generated files; without it, a test fails when one differs from its source.

## The blank

After `dpms-timeout` with no activity every display powers down, and the next activity powers it up. Activity is the user's input: a key or pointer event, and the wake-up after the user switches back to vtium's VT. A change in which displays are attached counts too, for now: the last display going ends a blank in force and resets the idle clock, and the blank does not run while none is attached; the first display, and each that comes after none, starts the countdown afresh. A display coming or going among others does not count, nor does a re-take: a re-take sets the displays up again and keeps the blank and its timer as they were, and while blanked its set-up waits for the input that ends the blank (the operator's decision on vtium/vtium#21). The idle lock (vtium/vtium#87) will count the user's input alone.

## No display

At start vtium waits `display-wait` (10 s by default) for a frame on a display's screen, counted from the first time its VT is shown, and with none by then it exits with status 1, so the unit's `OnFailure=getty@%i` takes the VT. A device whose displays are never set up, as when the modeset fails, counts as none, as does a display announced whose every frame fails to show. `display-wait=0` exits at once without one; `forever` waits for the first display however long it takes, for a machine whose only display is a USB adapter (DisplayLink, `udl`) plugged in at any time after boot. The session's command starts during the wait, and is closed if vtium exits.

Once a display has come, vtium never exits for want of one. When the last goes, it logs it and the session runs on: the grid keeps the last display's size, so the program sees no SIGWINCH; keys still reach it; and the blank does not run (see "The blank"). When the next comes, it is logged and drawn whole, and the grid takes its size where that differs, one SIGWINCH (the operator's decisions on vtium/vtium#21).

## Selection, copy and paste

The pointer drives vtium-term's selection, and the seat keeps one selection buffer: what the user copied last, or what a program set last with OSC 52 while `clipboard-write` is on. The console has no system clipboard. The middle button and Shift+Insert paste the buffer through vtium-term's `Terminal::paste`, filtered, and bracketed while the program asks with mode 2004. The buffer holds at most the text of the scrollback and the screen, or vtium-term's `CLIPBOARD_MAX` bytes from one OSC 52.

## Bounds

A configuration file is untrusted input. The bounds of each key's value are in `docs/config.md`.

| Bound | Value | Past it |
|---|---|---|
| `FILE_MAX` | 65,536 bytes of a file | The rest is not read, and the file is reported |
| `LINE_MAX` | 8,192 bytes of a line | The line is ignored and reported |
| `TEXT_MAX` | 4,095 bytes of a text value or a `login` command, `PATH_MAX` less its NUL | The value is ignored and reported |
| `REPORTS_MAX` | 64 reports | The rest are counted in one report |
| A seat's name | letters, digits, `_` and `-`, as logind names seats, and a file name of at most 255 bytes | Its file is not read, and the name is reported |

## Departures from the C reference

Each was the C reference's behaviour at kmscon `b1f7a7c5`.

- **A file is read whole up to `FILE_MAX`,** where the C reference read any length into memory (`src/misc/conf.c:533-590`).
- **Only a regular file is read.** A FIFO, a device or a directory at a configuration file's path is reported and skipped, and the file is opened without blocking; the C reference opened any path that existed (`src/misc/conf.c:542-556`), so a FIFO with no writer held the start forever.
- **A value that cannot be read is reported and the rest of the file is read.** The C reference stopped reading the file at a value its parser refused, such as `render-timer=fast` (`src/misc/conf.c:486-490, 504-512`), and read numbers with `atoi`, so `font-size=12px` was 12, `sb-size=abc` 0 and `sb-size=-1` 4294967295. vtium takes digits only, clamps a number to its key's bound, and reports both.
- **A seat's file that cannot be read is reported and skipped;** the C reference failed the seat (`src/seat.c:786-790`).
- **Each option name is matched whole.** The C reference's `getopt_long` took any unique prefix (`--lo` for `--login`).
- **A command-line word that is no option refuses the start, with `--login` too.** The C reference's `getopt_long` moved it to the end of the arguments, so with `--login` it joined the command, and without it the C reference refused to start (`src/misc/conf.c:310-313`). `check-config` is the one such word vtium takes, wherever it stands before `--`.
- **A mode line is read whole:** a size is digits, each side 1 to 65535, and a name at most 63 bytes; anything else is reported. The C reference read it with `sscanf`, which took a leading number and ignored the rest (`src/config.c:564-617`).
- **`no-` turns off a switch only.** The C reference read `no-font-size=12` as `font-size=12` (`src/misc/conf.c:318-341`).
- **`configdir` is set on the command line only.** The C reference let `kmscon.conf` set the directory the seat's file was read from.
- **A seat's name is checked before it names a file;** the C reference formatted any name into the path (`src/config.c:1571`).
- **The respawn limit times each start from that start, on the monotonic clock.** The C reference recorded a start only when the exit before it was quick, in whole seconds of the wall clock (`src/misc/pty.c:747-767`), so after one slow restart a command that failed at once restarted without end. A slow exit after quick restarts now starts the command again with the count reset, where the C reference exited at the next exit whatever its timing.
- **The font fallback is tried once,** then the error is returned; the C reference retried for as long as the lookup failed (aaronsb/kmscon#63).
- **`KMS_START_SCRIPT` is not set.** The C reference set it to `kmscon-launch-gui` on a DRM display (`src/misc/pty.c:286-291`), a script vtium does not install; the operator's decision on vtium/vtium#21.
- **With no display by `display-wait` after the start, vtium exits with status 1** (see "No display"); the C reference kept running with no display, as vtium does under `display-wait=forever`.
- **A failed renderer set leaves the pointer drawn in software,** and the next successful set gives it back to the hardware cursor; the C reference drew it nowhere (aaronsb/kmscon#211).

- **A child's exit closes its pty after the output it left is read** (vtium-platform's departure), and the next child starts from a hard reset (RIS).
- **A triple click selects a line, and a fourth click starts again.** The C reference had the double click's word only (`src/terminal.c:1509-1542`). Spec: xterm's `on3Clicks`, whose default is `line`; vtium-term's README lists how the line is copied. Test: `a_double_click_selects_a_word_and_a_triple_click_a_line`.
- **Shift selects and pastes while the program tracks the pointer.** The C reference reported every pointer event while the program tracked it, so text could not be selected there (`src/terminal.c:1623-1627`). Spec: xterm's `ShiftOverride` in `button.c`, with `shiftEscape` false, its default: a button or motion with Shift held is not reported. vtium decides at a press, and the button selects or pastes until its release whatever Shift does meanwhile; xterm decides each event by the Shift held then. Test: `shift_selects_and_pastes_while_the_program_tracks_the_pointer`.
- **The middle button pastes at its release.** The C reference pasted at the press (`src/terminal.c:1531-1536`). Spec: xterm's translation `~Ctrl ~Meta <Btn2Up>: insert-selection(SELECT, CUT_BUFFER0)`. Test: `a_hostile_selection_is_pasted_filtered_once_a_press`.
- **A drag ends when the left button is no longer held, and at the VT's release.** The C reference kept a drag until the left button's release came, so after a release it missed, as across a VT switch, plain motion went on extending the selection. Spec: none covers it; the reason: motion with no button held is not a drag. Test: `a_drag_ends_without_its_button_and_at_a_vt_release`.
- **Shift+Insert pastes, once a press.** The C reference had no key to paste. Spec: xterm's translation `Shift <KeyPress> Insert: insert-selection(SELECT, CUT_BUFFER0)`; a repeat of the held key pastes nothing more. Tests: `shift_insert_pastes_as_in_xterm`, `a_hostile_selection_is_pasted_filtered_once_a_press`, and the triple click's above.
- **A click with no drag, or a double click on a space, leaves the selection buffer as it was.** The C reference copied the one cell a click selected, or emptied its buffer for the space, so the next paste sent that character or nothing. Spec: xterm's `SelectSet`, which takes a selection only when it is not empty. Test: the triple click's above.

## Not yet built

Each is the C reference's and waits for its own change: drawing through the GPU, which `hwaccel` asks for and which warns at start (vtium draws in software), the `multi-monitor` key's `clone` and `largest` (vtium lays several displays out as `scaled`, the operator's choice on vtium/vtium#21), the hardware cursor (the pointer is drawn in software), the render timer's on-screen display, outline fonts (`font-engine=freetype` and `pango` draw with unifont), the named palettes but `legacy` and `custom` (vtium-term holds no other yet), and the C reference's keys the table lacks: `multi-monitor`, `screen`, the grabs, the decoration colours and the rest.

## Left to tier 3

Each needs a real VT or display, which the boot tests stand in for:

- The panic hook under `panic=abort` on a real VT: the VT back in text mode with its keyboard before the message prints.
- The 100 ms the loop is served at exit for SIGUSR2 on a real switch back.
- A release a real flip in flight delays, acknowledged once the device reports asleep.
- A card udev raises during `display-wait`, and the getty `OnFailure=` starts when no display comes.
- Whether fbdev stays lit when a re-take while blanked defers its set-up to the input that ends the blank; on drm2d the displays stay dark.

## C behaviour kept

- The XTGETTCAP table is the C reference's: the same 137 names and values as its ncurses-built `src/terminal_termcap.h`, and `Ms`, which the C reference's entry lacked, for OSC 52.
- A `login` word that is `$NAME` becomes that variable of the session's environment, after the environment is reset.
- With no mouse tracking, or with Shift, the left button's drag selects and its release copies, a double click selects and copies a word, and the right button clears the selection, where xterm's extends it (`src/terminal.c:1509-1542`). A pointer in the strip past the grid is at the grid's nearest cell (`src/terminal.c:173-190`). A key, the pointer hiding and a change of mouse tracking clear the selection (`src/terminal.c:890-897, 1370, 1655-1659`).
- A wheel event is one report while the program tracks the pointer, else a zoom with Ctrl, else three lines of scrollback, whatever its number of steps (`src/terminal.c:1492-1497, 1639-1650`).
