# vtium

The binary: the configuration and command line, the terminfo entry and the XTGETTCAP table generated from it, the session's command and environment, the respawn policy, and the rules the terminal wiring follows. Each is a function of what it is given, time included, so its tests need no VT, display or pty. The wiring of the VT, the displays and the loop comes with vtium/vtium#21's second part.

| Path | What it is |
|---|---|
| `src/config/keys.rs` | The key table: every key and option, its type, default, bound, reader and source |
| `src/config/` | Reading `kmscon.conf`, `<seat>.seat.conf` and the command line through the table |
| `src/termcap.rs`, `src/termcap/table.rs` | The XTGETTCAP table, generated from `install/kmscon.ti` |
| `src/child.rs` | The session's command and environment |
| `src/respawn.rs` | Whether the session's command starts again |
| `src/blink.rs`, `src/font.rs`, `src/pointer.rs` | The blink timers, the font fallback and the pointer guard |
| `install/kmscon.conf.example` | The example configuration, generated from the key table |
| `install/kmscon.ti` | The terminfo entry, installed compiled with `tic -x` |

`docs/config.md` describes every key. `VTIUM_BLESS=1 cargo test -p vtium` rewrites the three generated files; without it, a test fails when one differs from its source.

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
- **A value that cannot be read is reported and the rest of the file is read.** The C reference stopped reading the file at a value its parser refused, such as `render-timer=fast` (`src/misc/conf.c:486-490, 504-512`), and read numbers with `atoi`, so `font-size=12px` was 12, `sb-size=abc` 0 and `sb-size=-1` 4294967295. vtium takes digits only, clamps a number to its key's bound, and reports both.
- **A seat's file that cannot be read is reported and skipped;** the C reference failed the seat (`src/seat.c:786-790`).
- **Each option name is matched whole.** The C reference's `getopt_long` took any unique prefix (`--lo` for `--login`).
- **A command-line word that is no option is reported and dropped.** The C reference's `getopt_long` moved it to the end of the arguments, into the `--login` command.
- **`no-` turns off a switch only.** The C reference read `no-font-size=12` as `font-size=12` (`src/misc/conf.c:318-341`).
- **`configdir` is set on the command line only.** The C reference let `kmscon.conf` set the directory the seat's file was read from.
- **A seat's name is checked before it names a file;** the C reference formatted any name into the path (`src/config.c:1571`).
- **The respawn limit times each start from that start, on the monotonic clock.** The C reference recorded a start only when the exit before it was quick, in whole seconds of the wall clock (`src/misc/pty.c:747-767`), so after one slow restart a command that failed at once restarted without end. A slow exit after quick restarts now starts the command again with the count reset, where the C reference exited at the next exit whatever its timing.
- **The font fallback is tried once,** then the error is returned; the C reference retried for as long as the lookup failed (aaronsb/kmscon#63).
- **A failed renderer set leaves the pointer drawn in software,** and the next successful set gives it back to the hardware cursor; the C reference drew it nowhere (aaronsb/kmscon#211).

## C behaviour kept

- The XTGETTCAP table is the C reference's: the same 137 names and values as its ncurses-built `src/terminal_termcap.h`.
- `KMS_START_SCRIPT=kmscon-launch-gui` is set on a DRM display, though vtium installs no such script.
- A `login` word that is `$NAME` becomes that variable of the session's environment, after the environment is reset.
