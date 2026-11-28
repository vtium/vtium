# vtium-config-macro

This crate reads and writes vtium-config's macros. A macro is a short UTF-8 text file, usually named `*.vtmacro`, that drives vtium-config's TUI with one step on each line. A step presses a key with any of Ctrl, Alt and Shift, types text, or clicks a label. It can also check the screen or the TUI's state, pause, write a screenshot, or ask for a live run. The steps are a subset of the tape format of VHS, charmbracelet's tool for scripted terminal recordings, with a few steps of vtium's own. For example:

```
# The built-in font is always active, so Space on it changes nothing.
Alt+N
Expect At fonts
Click "built-in"
Space
Expect "always active"
Expect Saved
```

`parse` reads a macro file's text into a `Macro`, which keeps every line as a `Step`, comments and blank lines included. So the step at index `i` is on line `i + 1`. It stops at the first line that does not parse and returns an `Error` that names the line and says what the line should hold, as in `line 3: Clik is not a step. A step is a key, Type, Click, Expect, Sleep, Screenshot or Set Live.` `Parser` reads the same lines one at a time, for a macro that arrives on standard input. Its `read` takes one line from a reader and never reads more than a line may hold. So a stream is bounded by its step count and its line length, not by a file's size. A `Macro` writes canonical text that parses back into the same `Macro`. The canonical line puts one space between words and writes the modifiers as Ctrl, Alt, Shift. It writes `Escape` for `Esc`, leaves out a count of 1, and writes a pause in whole seconds where it can. It uses double quotes unless the text holds one, then single quotes, then backticks.

`KeyPath` holds the keys a player pressed from the start of a run and writes them as `Alt+N Down Down Space`. `failure_name` names a failure as the scenario tests name one, `vtium 80×25: Up Up Enter`, and `failure_report` writes the report of a failed step with its keys from the start. `KeyPath::to_macro` turns the keys into a macro that plays them again.

The crate knows nothing of the TUI. Its keys are names and characters with a set of modifiers, and the player in vtium-config turns them into the TUI's own key type. A key step takes a chord as a terminal sends it. Ctrl+I, Ctrl+M, Ctrl+H and Ctrl+[ send the bytes of Tab, Enter, Backspace and Escape, so they parse as those keys. Alt and `[`, `]`, `_` or a capital `P` send ESC and a byte that opens a control sequence or a control string. No terminal sends those as a key, so they fail to parse with that reason. The player still runs every key through the TUI's own reader. A macro may come over ssh from another machine, so the crate treats its input as untrusted. It forbids unsafe code and depends on Rust's standard library alone. It denies the lints that find a panic: unchecked arithmetic, lossy casts, unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

## Rules the crate sets where the record leaves them open

- A label after `Click` and a text after `Expect` hold words with one space between each, and no space at the start or end. The player reads the screen with one space between words, so any other spacing would never match, and an `Expect Not` with it would always pass.
- `Type`, `Click` and `Expect` take double quotes, single quotes or backticks alike, as VHS does.
- `Sleep` takes whole milliseconds, as in `500ms`, or seconds with at most three digits after the point, as in `2s`, `0.5s` or a bare `0.5`.
- A `Type` text, a label and a text are never empty, and a `Sleep` lasts at least 1 ms.
- A `Screenshot` name is at most 64 ASCII letters, digits, `-`, `_` and `.`, and does not start with a dot. The record bounds a name only by the line.
- A place after `Expect At` is lowercase words of letters, digits, `-` and `_`, joined by dots.
- A line may have spaces and tabs around its words and a carriage return before its newline, and the first line may start with a byte-order mark.
- A key is kept as the macro wrote it, so `Shift+a` and `A` are different steps, and the player decides what each one types. The chords that share a byte with another key are the exception above. `#` alone is a comment and never a key, so a macro types `#` with `Type "#"`.
- The chords refused at parse time are those of the record's list and every spelling of the same bytes, such as `Alt+P` and `Alt+_`. Other chords, such as `Ctrl+Alt+X`, are left to the player, which knows the TUI's reader.

## Files

| Path | What it is |
|---|---|
| `src/lib.rs` | The bounds, the types of a macro and its steps, and `Macro`, with its check that the steps write text that parses back. |
| `src/chord.rs` | Which chords a key step may press, by the bytes a terminal sends for them, and `Unpressable`. |
| `src/parse.rs` | `parse`, `parse_bytes`, `Parser` and `Error`: the text of a macro to its steps, the bounded reader of a stream, and the messages of each error. |
| `src/write.rs` | The canonical text of a key, a step and a macro. |
| `src/path.rs` | `KeyPath`, `failure_name` and `failure_report`. |
| `tests/grammar.rs` | Each row of the grammar, parsed and written back. |
| `tests/errors.rs` | Each error and each bound, with its line and its message. |
| `tests/examples.rs` | Both example macros of ADR-1208, step for step. |
| `tests/round_trip.rs` | Generated macros and generated texts, each of which writes canonical text that parses back the same. |
| `tests/path.rs` | The key path, the failure's name and its report. |
| `tests/chords.rs` | The chords taken as another key, and the chords refused with their reason. |
| `tests/stream.rs` | A stream's bounds, and its reader against a file's steps. |
| `fuzz/` | The fuzz target `parse`, which reads any bytes as a macro. Its README says how to run it. |

## Bounds

| Bound | Value | What it protects |
|---|---|---|
| `FILE_MAX` | 1 MiB, counting a newline after each line | The memory a macro file can take, since the parser reads it whole first. A stream does not count it. |
| `LINE_MAX` | 4,096 bytes | A file or a stream of one huge line, and the length of the error about a line. |
| `STEPS_MAX` | 10,000 lines | How long a run can take: at the highest count, a million keys. With `LINE_MAX`, it bounds a stream. |
| `COUNT_MAX` | 100 | A typo such as `Down 30000`. |
| `TYPE_MAX` | 1,000 characters | One step's length in visible play, and its line's length. |
| `LABEL_MAX` | 256 characters | A label or a text longer than a few rows of the screen, and the player's search for it. |
| `SLEEP_MAX` | 60 seconds | A typo such as `Sleep 600s` stalling a run. |
| `NAME_MAX` | 64 bytes | The length of a screenshot's file name and of a place in the TUI. |

`cargo test -p vtium-config-macro` runs the crate's tests.

References:
- ADR-1208 D1 and D2: the format, its grammar, its limits and its two example macros.
- ADR-1208 D3: a key as a terminal sends it, and the chords no terminal sends as a key.
- ADR-1208 D6: the failure report and the keys from the start.
- ADR-1208 D7: the crate of the format, its round trip and its fuzz target.
