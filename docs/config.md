# Configuration

<!-- Generated from crates/vtium/src/config/keys.rs; do not edit. `VTIUM_BLESS=1 cargo test -p vtium generated_files` rewrites it. -->

vtium reads `kmscon.conf` in the configuration directory (`--configdir`, `/etc/kmscon` by default), then `<seat>.seat.conf` there, then its command line. Each wins over the ones before it. A file that does not exist is skipped.

A file holds one key a line: `key=value`, or a switch's `key` to turn it on and `no-key` to turn it off. `#` starts a comment, except where it opens a `#rrggbb` colour value. Spaces and tabs around the key and the value are dropped. On the command line a key is `--key=VALUE` or `--key VALUE`, and a switch `--key` or `--no-key`.

vtium reports, once at start, a key it does not know, however often it appears, and a value it cannot use: a number out of bounds, which it clamps, and a value it cannot read, which it ignores, keeping the value the sources before it gave. It reads at most 65536 bytes of a file and 8192 of a line.

`install/kmscon.conf.example` in `crates/vtium` lists the same keys at their defaults. Source cites the C reference, kmscon at `b1f7a7c5` ([c-reference.md](c-reference.md)), or the decision a key comes from.

| Key | Option | Type | Default | Bound | Read by | Set in | What | Source |
|---|---|---|---|---|---|---|---|---|
| `configdir` | `--configdir=VALUE`, `-c` | text | `/etc/kmscon` | 4095 bytes, no control character | vtium | command line | The directory holding `kmscon.conf` and `<seat>.seat.conf`. | src/config.c:1349 |
| `help` | `--help`, `-h` | switch | off | on or off | vtium | command line | Print the options and exit. | src/config.c:1342 |
| `version` | `--version`, `-V` | switch | off | on or off | vtium | command line | Print the version and exit. | src/config.c:1343 |
| `vt` | `--vt=VALUE` | text | unset | 4095 bytes, no control character | vtium-platform | file, command line | The VT to run on: a number N for `/dev/ttyN`, a name under `/dev`, or a path. Unset, the VT vtium was started on, or the next free one. | src/config.c:329-361, 1352 |
| `switchvt` | `--switchvt`, `--no-switchvt` | switch | on | on or off | vtium-platform | file, command line | Switch to vtium's VT at start; off, wait until something else switches to it. | src/config.c:1353 |
| `login` | `--login -- COMMAND...`, `-l` | command | unset | 4095 bytes in a file | vtium | file, command line | The session's command. Unset, `/bin/login -p` with `reset-env` on and `/bin/login` with it off. A word that is `$NAME` becomes that variable of the session's environment, and `$$` starts a word with `$`. | src/config.c:372-494, 1365; src/misc/pty.c:252-302 |
| `oneshot` | `--oneshot`, `--no-oneshot` | switch | off | on or off | vtium | file, command line | Exit when the session's command exits, in place of starting it again. | src/config.c:1367; src/misc/pty.c:752-756 |
| `term` | `--term=VALUE`, `-t` | text | `kmscon` | 4095 bytes, no control character | vtium | file, command line | `TERM` in the session's environment. | src/config.c:1368; meson.options:51-52 |
| `reset-env` | `--reset-env`, `--no-reset-env` | switch | on | on or off | vtium | file, command line | Start the session with an empty environment and the variables vtium sets; off, with vtium's own environment as well. | src/config.c:1369; src/misc/pty.c:261-272 |
| `backspace-delete` | `--backspace-delete`, `--no-backspace-delete` | switch | on | on or off | vtium-platform | file, command line | The pty's erase character is DEL (0x7f); off, BS (0x08). | src/config.c:1370; src/misc/pty.c:360-366 |
| `sb-size` | `--sb-size=VALUE` | number (lines) | 1000 | 0 to 100000, clamped | vtium-term | file, command line | Lines of scrollback, at most `sb-size-cap`. | src/config.c:1371; ADR-1001, "Scrollback size" |
| `sb-size-cap` | `--sb-size-cap=VALUE` | number (lines) | unset | 0 to 100000, clamped | vtium-term | file, command line | The most lines `sb-size` may ask for. Unset, it follows the machine's memory: 10,000 lines below 2 GiB, 30,000 from 2 GiB and 100,000 from 4 GiB. | ADR-1001, "Scrollback size"; the operator on vtium/vtium#21 |
| `blink` | `--blink`, `--no-blink` | switch | on | on or off | vtium | file, command line | Blink text with SGR 5 and a blinking cursor, every 500 ms; off, neither blinks. | src/config.c:1373; src/terminal.c:163, 715-724 |
| `sync-timeout` | `--sync-timeout=VALUE` | number (ms) | 200 | 0 to 10000, clamped | vtium | file, command line | The longest a synchronized frame (mode 2026) is held before it is shown as drawn; 0 offers no mode 2026. | src/config.c:1374; src/terminal_sync.h:62-66, `TERMINAL_SYNC_MAX_MS` |
| `dpms-timeout` | `--dpms-timeout=VALUE` | number (s) | 600 | 0 to 4294967295, clamped | vtium-platform | file, command line | Turn the displays off after this long with no key or pointer event; 0 never does. | src/config.c:1388 |
| `hwaccel` | `--hwaccel`, `--no-hwaccel` | switch | off | on or off | vtium | file, command line | Draw through the GPU, in a build that has it. | src/config.c:1413 |
| `use-original-mode` | `--use-original-mode`, `--no-use-original-mode` | switch | off | on or off | vtium-platform | file, command line | Keep the mode each display is in; off, its preferred mode. | src/config.c:1416; vtium/vtium#5 |
| `font-engine` | `--font-engine=VALUE` | one of `freetype`, `pango`, `psf`, `unifont`, `8x16` | unset | the choices | vtium-font | file, command line | The font source to use. Unset, each the build has, in this order. | src/config.c:1422; src/font/font.c:331-368 |
| `font-name` | `--font-name=VALUE` | text | `monospace` | 4095 bytes, no control character | vtium-font | file, command line | The font's name, or with `font-engine=psf` the path of a PSF font. | src/config.c:1424 |
| `font-size` | `--font-size=VALUE` | number (px) | 0 | 0 to 2048, clamped | vtium-font | file, command line | The font's height; 0 picks one from the display's size. | src/config.c:1423; vtium-font's `CELL_SIDE_MAX` |
| `glyph-source` | `--glyph-source=VALUE` | one of `font`, `sprites`, `auto` | unset | the choices | vtium-font | file, command line | Where the six sprite ranges draw from: `font`, the font's glyphs; `sprites`, vtium's; `auto`, the font's where it has one. Unset, a named font's glyphs for each range it covers whole and the sprites for the rest; the built-in unifont draws the sprites. | vtium/vtium#85 |
| `glyph-source-box` | `--glyph-source-box=VALUE` | one of `font`, `sprites`, `auto` | unset | the choices | vtium-font | file, command line | `glyph-source` for box drawing, U+2500-U+257F, over it. | vtium/vtium#85 |
| `glyph-source-block` | `--glyph-source-block=VALUE` | one of `font`, `sprites`, `auto` | unset | the choices | vtium-font | file, command line | `glyph-source` for block elements, U+2580-U+259F, over it. | vtium/vtium#85 |
| `glyph-source-braille` | `--glyph-source-braille=VALUE` | one of `font`, `sprites`, `auto` | unset | the choices | vtium-font | file, command line | `glyph-source` for Braille, U+2800-U+28FF, over it. | vtium/vtium#85 |
| `glyph-source-sextants` | `--glyph-source-sextants=VALUE` | one of `font`, `sprites`, `auto` | unset | the choices | vtium-font | file, command line | `glyph-source` for sextants, U+1FB00-U+1FB3B, over it. | vtium/vtium#85 |
| `glyph-source-octants` | `--glyph-source-octants=VALUE` | one of `font`, `sprites`, `auto` | unset | the choices | vtium-font | file, command line | `glyph-source` for octants, U+1CD00-U+1CDE5, over it. | vtium/vtium#85 |
| `glyph-source-legacy` | `--glyph-source-legacy=VALUE` | one of `font`, `sprites`, `auto` | unset | the choices | vtium-font | file, command line | `glyph-source` for the rest of Symbols for Legacy Computing's block shapes, over it. | vtium/vtium#85 |
| `missing` | `--missing=VALUE` | one of `box`, `hex` | `box` | the choices | vtium-font | file, command line | What a character the font lacks draws as: `box`, U+25A1; `hex`, its codepoint in hex digits. | src/font/font.c:345-366; vtium/vtium#85 |
| `glyph-cache` | `--glyph-cache=VALUE` | one of `minspec`, `performance` | unset | the choices | vtium-render | file, command line | The glyph cache's budgets. Unset, `minspec`, or `performance` in a build with vtium-render's `performance-cache` feature. | ADR-1300; vtium/vtium#16 |
| `render-timer` | `--render-timer=VALUE` | one of `none`, `osd`, `log`, `both` | `none` | the choices | vtium-render | file, command line | Measure each display's frames once a second, and write the figures to the log, draw them on screen, or both. | src/config.c:972-1015, 1475 |
