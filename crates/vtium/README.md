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
| `src/blink.rs`, `src/font.rs`, `src/pointer.rs` | The blink timers; the font's fallback, size, sprite policy and missing glyph; the pointer guard |
| `src/sync.rs` | When the terminal presents: the mode 2026 hold and coalescing without it |
| `src/grab.rs` | The terminal's grabs, third in the key order |
| `src/layout.rs` | The grid and each display's font height with several displays, and the zoom and rotation steps |
| `src/term.rs` | The session's terminal as the pty's consumer, and a view per display that draws it |
| `src/video.rs` | A video device behind one trait: drm2d, fbdev, or a test's stand-in |
| `src/app.rs` | The seat: the VT's events, the devices, the keys, the blank, the child, the exit |
| `src/logger.rs` | The log on stderr |
| `src/check.rs` | `vtium check-config`, and the warnings the start gives beyond the reports |
| `src/warn.rs` | Where the operator's warnings go: the configuration's reports and a font's partial ranges |
| `src/hook.rs` | The panic hook, which puts the VT back before the panic prints |
| `src/main.rs` | Start-up and the loop |
| `tests/check_config.rs` | `vtium check-config` run as a binary: its warnings, exit status and help, and with strace on the machine, that it opens nothing but the configuration |
| `tests/panic_hook.rs` | The panic hook's order, in a process of its own |
| `tests/boot.rs` | The seat booted with no hardware: output drawn, keys, a VT switch, the blank and what counts as activity, the respawn limit, DECARM, the font's policy, the exit |
| `install/kmscon.conf.example` | The example configuration, generated from the key table |
| `install/kmscon.ti` | The terminfo entry, installed compiled with `tic -x` |
| `install/kmsconvt@.service` | The VT unit: vtium in place of getty on a VT, which `make install-tree` installs and tier 3's guest starts |

`docs/config.md` describes every key. `VTIUM_BLESS=1 cargo test -p vtium` rewrites the three generated files; without it, a test fails when one differs from its source.

## The blank

After `dpms-timeout` with no activity every display powers down, and the next activity powers it up. Activity is a key or pointer event, and the wake-up after the user switches back to vtium's VT. A hotplug and a re-take are not: a re-take sets the displays up again and keeps the blank and its timer as they were, and while blanked its set-up waits for the input that ends the blank (the operator's decision on vtium/vtium#21).

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
- **A failed renderer set leaves the pointer drawn in software,** and the next successful set gives it back to the hardware cursor; the C reference drew it nowhere (aaronsb/kmscon#211).

- **A child's exit closes its pty after the output it left is read** (vtium-platform's departure), and the next child starts from a hard reset (RIS).

## Not yet built

Each is the C reference's and waits for its own change: drawing through the GPU, which `hwaccel` asks for and which warns at start (vtium draws in software), the `multi-monitor` key's `clone` and `largest` (vtium lays several displays out as `scaled`, the operator's choice on vtium/vtium#21), the hardware cursor (the pointer is drawn in software), selection and copy with the pointer, pasting a selection (OSC 52's is taken and the last one held, at most vtium-term's `CLIPBOARD_MAX`, for that paste; `no-clipboard-write` drops it, and the console has no system clipboard to hand it to), the render timer's on-screen display, outline fonts (`font-engine=freetype` and `pango` draw with unifont), the named palettes but `legacy` and `custom` (vtium-term holds no other yet), and the C reference's keys the table lacks: `multi-monitor`, `screen`, the grabs, the decoration colours and the rest.

## Left to tier 3

Each needs a real VT or display, which the boot tests stand in for:

- The panic hook under `panic=abort` on a real VT: the VT back in text mode with its keyboard before the message prints.
- The 100 ms the loop is served at exit for SIGUSR2 on a real switch back.
- A release a real flip in flight delays, acknowledged once the device reports asleep.
- Whether fbdev stays lit when a re-take while blanked defers its set-up to the input that ends the blank; on drm2d the displays stay dark.

## C behaviour kept

- The XTGETTCAP table is the C reference's: the same 137 names and values as its ncurses-built `src/terminal_termcap.h`, and `Ms`, which the C reference's entry lacked, for OSC 52.
- A `login` word that is `$NAME` becomes that variable of the session's environment, after the environment is reset.
- A wheel event is one report while the program tracks the pointer, else a zoom with Ctrl, else three lines of scrollback, whatever its number of steps (`src/terminal.c:1492-1497, 1639-1650`).
