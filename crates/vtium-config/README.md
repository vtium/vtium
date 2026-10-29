# vtium-config

The head file of ADR-1401: its reader, its parser, and the evaluator that places each display and input in a head, in the focus, in `ignore` or in `dark`, with the guards over the result. `vtium` and `vtiumctl` both link it, so the verdict before install is the verdict at boot. It depends on neither `vtium` nor `vtium-platform`: a device reaches it as an `Identity` its caller fills, and the guards are functions of a placement and the set of devices. It forbids unsafe code, denies every input-reachable panic, and depends only on rustix, to open the file as the console opens kmscon.conf.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation and public names. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its source: the one source for the constants and for the table below. |
| `src/file.rs` | `read`, the file from disk with its ownership check, and `text`, the checks on its bytes. |
| `src/parse.rs` | `parse`, the text to a `Config` or rejected whole, with the warnings and notes of a valid file. |
| `src/config.rs` | `Config`, `Head`, `Rule` and `Guards`; `TEMPLATE` and `Config::builtin`, its parse. |
| `src/identity.rs` | `Identity`, a device as the rules see it, and udev's input classes. |
| `src/matching.rs` | The match keys and the glob. |
| `src/place.rs` | The first-match walk, the guards, the rescue key, the layer 1 warnings and the paste-ready rule for a dark display. |
| `src/cmdline.rs` | `vtium.heads=` on the kernel command line. |
| `src/error.rs` | Errors, warnings and notes, and `Quoted`, the escaped text an error quotes. |
| `templates/heads.conf` | The commented template; the compiled-in configuration is its parse. |
| `tests/` | The integration tests, one file a part: the parser, the reader, the placement and guards, the determinism property and the template. |

`cargo test -p vtium-config` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config readme_table` rewrites it.

## Behaviour

- **The file.** `read` takes the path with symlinks followed, and returns `None` for a file that does not exist, the compiled-in configuration's case. It refuses anything but a regular file, opening without blocking so a FIFO is refused at once; a file root does not own or that its group or others can write; a directory above it, on the path given or the path its symlinks lead to, that root does not own or that its group or others can write; more than `FILE_MAX` bytes; bytes that are not UTF-8; and any control character but tab and newline, a carriage return and the C1 controls among them. Each refusal makes the file invalid. `Trust::owner` trusts a test's own user in place of root, since no test runs as root.
- **The syntax.** The first line that is not blank or a comment is `version = 1`. `#` starts a comment anywhere on a line, so a value holds no `#`; blanks around `=` and between tokens are dropped, so a value holds no blank, and a glob's `?` stands for a space. A switch is `on` or `off`. A key set twice in one section, and a second `[rules]` or `[guards]`, make the file invalid.
- **Rejected whole.** Every line is read whatever came before it, so a check names every error, each with its line, in the file's order; `ERRORS_MAX` are kept and the rest counted. Text an error quotes is cut at `QUOTE_MAX` characters and shown with every control and format character escaped; a path is quoted whole.
- **Heads.** A name is 1 to 64 of `A-Z a-z 0-9 . _ + -`, and `focus`, `ignore` and `dark` are reserved; a profile's name keeps the same rule. `consoles` is 1 to `CONSOLES_MAX`, and more gets the cap with a warning. `login` is kept as written, for vtium to read as kmscon.conf's `login`. `on-loss` is `nohup`, `hangup`, or `hangup DURATION`, the duration a count of seconds or with an `s`, `m` or `h` suffix, at most `u32::MAX` seconds. `font-size` is 0 to `FONT_SIZE_MAX`. A head's font is its own keys over its profile over kmscon.conf's keys; until profiles have files, the `default` profile states nothing and any other states its frit, the frit of its name.
- **Rules.** A rule is `display MATCH -> TARGET` or `input MATCH -> TARGET`, with `*` alone or at most `TOKENS_MAX` tokens; a rule may name a head declared after it. `focus` and `ignore` are an input's targets, `dark` a display's. `path` matches its `ID_PATH` and every interface below it (`...:3.3` takes `...:3.3:1.0`, never `...:3.30` or `...:3.3.1`); `serial` and `edid` split their value at the first two `:`, so the serial may hold more; `id`, `serial`'s vendor and product and `edid`'s product are hex with or without `0x`; `connector` splits at its last `/`; `name`, `card`, both halves of `connector` and `udev`'s value are globs (`*` and `?`); every other value matches as written. A key of the other kind's devices matches nothing.
- **Warnings and notes.** A rule an earlier rule of its kind shadows (the earlier is `*`, or its tokens are among the later rule's) and a head's consoles past the cap are warnings. A guard turned off, a head's `rescue = off`, and input sent to a head by `id` or `name` alone are notes. `Config::needs_udev` lists the rules using a key that reads udev's database, for `vtium check-config` to note where no daemon runs.
- **Placement.** `walk` places one device by the first rule of its kind that matches it; with none, a display goes dark and an input follows the focus. `place` walks a set and runs the guards: the display guard, when no display reaches a head, lights the boot display (the connector lit at start, else the boot card's first connector in the kernel's order, else the first connector of the other cards in the kernel's order, so a connected display always lights; the boot card, the card's place in the kernel's order, the connector's place and then the whole identity break a tie) in the first head, or the default head where the file has none; then the anti-lockout guard, when no device counted as a keyboard reaches a head with a lit display, sends every keyboard-class device to the focus and moves the focus to the boot display's head, else the first lit head in the file. `place` takes the head holding the focus and returns where it rests: a device following the focus reaches a lit head only when that head is lit; with no focus yet, the focus starts by the same rule; and while a keyboard reaches a lit head the focus stays where it is. A device udev marks `VTIUM_ROLE=token` or `VTIUM_ROLE=numpad` is not counted. `rescue` says whether the rescue key takes a keyboard, `disposition` whether an input is opened, opened for the rescue key alone or left closed, `lockout` the layer 1 warnings of the rules alone, and `idle_rules` the rules no present device matches.
- **The kernel command line.** `heads_word` reads the last `vtium.heads=`: `builtin`, or a value vtium logs and ignores.

## Departures

- **An invalid head file is rejected whole.** kmscon.conf's reader, the C reference's and vtium's, takes each line it can and reports the rest; a head file routes input and chooses login commands, so no part of an invalid one applies (ADR-1401 D7).
- **The file must be root's, in root's directories, writable by no one else.** The C reference read any file at its path (ADR-1401 D10).

## Bounds

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Source |
|---|---|---|---|---|
| `FILE_MAX` | 65536 | Bytes of a head file | The file is invalid, rejected whole | ADR-1401 D3, kmscon.conf's bound (`crates/vtium/src/config/mod.rs`, `FILE_MAX`) |
| `LINE_MAX` | 8192 | Bytes of one line, its newline apart | The file is invalid, the line named | ADR-1401 D3, kmscon.conf's bound (`crates/vtium/src/config/mod.rs`, `LINE_MAX`) |
| `NAME_MAX` | 64 | Characters of a head's or a profile's name, each of `A-Z a-z 0-9 . _ + -` | The file is invalid, the line named | ADR-1401 D3 |
| `HEADS_MAX` | 16 | Heads a file declares | The file is invalid, the first head past it named | vtium's own: a head a display, with room for a machine of many outputs |
| `RULES_MAX` | 256 | Rules a file holds, so a device's walk reads at most this many | The file is invalid, the first rule past it named | vtium's own: a rule a device with room to spare |
| `TOKENS_MAX` | 8 | `key:value` tokens one rule's match combines | The file is invalid, the rule named | vtium's own: the strongest binding takes two (`path` and `serial`, or `connector` and `edid`) |
| `CONSOLES_MAX` | 50 | Virtual consoles one head asks for with `consoles` | The head gets this many, with a warning | a starting value, the C reference's `session-max` (`kmscon/src/config.c:1357`); ADR-1401 D10 and its probe 6 set the caps |
| `FONT_SIZE_MAX` | 2048 | A head's `font-size`, in points, as kmscon.conf's `font-size` takes it | The file is invalid, the line named | vtium-font's `CELL_SIDE_MAX`, which `font_size_max_is_vtium_fonts_cell_bound` checks |
| `ERRORS_MAX` | 64 | Errors, warnings and notes a check keeps, each kind apart | Counted, not kept | kmscon.conf's `REPORTS_MAX` (`crates/vtium/src/config/mod.rs`) |
| `QUOTE_MAX` | 64 | Characters of the file's text an error quotes, escaped when shown | The quote is cut, and shown ending `...` | vtium's own: a name, a value or a token |
<!-- end of the bounds table -->
