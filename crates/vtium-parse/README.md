# vtium-parse

vtium's escape-sequence parser: bytes from the pty in, terminal actions out. It decodes UTF-8 (or 8-bit or 7-bit input), runs the DEC/ANSI state machine that vt100.net describes, and drives a `Consumer` with printable characters, C0 and C1 controls, and complete ESC, CSI, OSC, DCS and APC sequences. It forbids unsafe code, has no dependencies, and holds every buffer inline but the OSC string's, which grows to at most its bound and is released after a long string. Charset mapping, screen effects and replies belong to `vtium-term`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation and public names. |
| `src/consumer.rs` | The `Consumer` trait, one method per kind of action, and the `Chunk` and `InputMode` types its methods take. |
| `src/parser.rs` | `Parser`: the state machine and the collection of parameters, intermediates and strings. |
| `src/params.rs` | `Params` and `Intermediates`: each parameter an `Option<u32>`, `None` when omitted, the private marker apart as `Params::marker`, and the intermediates as their bytes in order. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its citation: the one source for the constants and for the table below. |
| `src/utf8.rs` | The UTF-8 decoder in front of the state machine, with its contract to the Unicode Standard in seven rules, and the noncharacters an OSC string drops. |
| `tests/` | The integration tests: see its README. |
| `fuzz/` | The cargo-fuzz target, its own workspace so CI does not build it: see its README. |

`cargo test -p vtium-parse` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_PARSE_BLESS=1 cargo test -p vtium-parse readme_table` rewrites it.

## Behaviour

The parser follows vt100.net's description of the DEC parser, and ECMA-48 where ECMA-48 defines the syntax of a control function. Where the two read differently, or neither covers a case, the sections below give the rule the parser follows and why. Each rule has a test in `tests/`.

UTF-8 decoding follows the Unicode Standard; its seven rules are in `src/utf8.rs`.

### Strings

- **A string acts only when its terminator completes it.** The end of an OSC, DCS or APC string carries a `StringEnd`. `St` (0x9C, or ESC followed by `\`) and `Bel` complete it. CAN, SUB, another C1 control, or an ESC followed by anything else cancel it, and a consumer acts only on a complete string. An ESC therefore waits for the next character before the string ends, and a kitty command reaches the consumer at the `\` of its ST. Spec: vt100.net, "the C0 controls CAN and SUB cancel any escape sequence, control sequence or control string in progress", and ESC "will "cancel" the control string"; ECMA-48 8.3.6, "this data shall be ignored", and 5.6, a control string ends with ST.
- **A reset drops the `\` of an APC's ST.** When the host resets the parser between the ESC that ended an APC string and the `\` of its ST, as it does when it cancels a kitty request, the parser drops that `\`, so no stray character prints (vtium/vtium#9). After a reset at an OSC's or a DCS's ESC, the `\` prints.
- **An OSC string collects DEL,** as vt100.net's `osc_put` takes it, where ECMA-48 5.6 would leave it out of a control string.

### Sequences

- **Intermediates keep their order, count and bytes.** An ESC, CSI or DCS sequence passes its intermediates in order, each as its own byte, and its private marker apart from them, so `CSI ! ! p` is not DECSTR. A third intermediate makes the sequence do nothing (`INTERMEDIATES_MAX`). Spec: ECMA-48 5.4 c), intermediate bytes "Together with the Final Byte F, they identify the control function"; vt100.net, collect: "If more than two intermediate characters arrive, the parser can just flag this so that the dispatch can be turned into a null operation."
- **Every empty parameter is omitted.** An empty parameter is `None` wherever it falls, before a `;` too, so an explicit 0 and a default stay distinct. Spec: ECMA-48 5.4.2 e), "An empty parameter sub-string represents a default value which depends on the control function", and g).
- **A character past ASCII ends a sequence header in UTF-8 mode.** In a CSI or DCS header the parser ignores the sequence to its end, as vt100.net's parser does with a byte that a header cannot hold, and in an ESC sequence it drops the sequence. No specification covers a character past ASCII inside a sequence.

### Input

- **C1 controls do nothing in UTF-8 mode.** U+0080 to U+009F (`C2 80` to `C2 9F`) are neither executed nor printed, so a program that strips ESC from untrusted text also stops every control sequence. C1 controls still work in their 7-bit form (ESC Fe) and as bytes in 8-bit mode. ECMA-48 5.3 defines C1 for 7-bit and 8-bit codes only, and the maintainer chose to ignore the decoded form. Other terminals differ: foot ignores them (its decoder hands U+0080 to U+009F to the printer, which draws only characters of positive width, `vt.c:1043` and `terminal.c:4228, 4447-4448` at `152487e`, and its comment at `vt.c:257-266` says it ignores C1 in UTF-8 mode, as xterm does); Ghostty ignores them (`src/terminal/stream.zig:1334-1357` at `9ee02ae`, "Ignore C1 that came via UTF-8 decoding, matching xterm", tested at line 3764); WezTerm acts on them as controls (`vtparse/src/lib.rs:690-709` at `e68a8ec` runs a decoded U+0080 to U+00FF through the state machine, its test `utf8_control` at lines 1035-1040 expects U+008D to execute, and `wezterm-escape-parser/src/parser/mod.rs:218-220` turns that into a control).
- **GR acts as GL inside sequences in 8-bit mode.** A byte from A0 to FF inside an ESC, CSI or DCS header acts as the byte 80 below it, so `9B 31 ED` is SGR 1; in ground and in strings it stays itself. Spec: vt100.net, "In all states, these codes are treated identically to GL codes 20-7F."
- **7-bit mode masks bit 8,** so 0x9B is ESC and 0xC1 is `A`.
- **DEL reaches `print` in ground state,** as vt100.net has it; vtium-term decides whether it draws.

## Notes for vtium-term

- Draw the DEL (0x7F) that reaches `print` only when a 96-character set is in GL, and ignore it otherwise: vt100.net, "When a 96-character set is mapped into GL, both 20 and 7F may cause a character to be displayed", and "terminals only claiming VT220 compatibility can always ignore 7F".
- Act on an OSC, DCS or APC string only when its `StringEnd` is complete (`St` or `Bel`). Reply to an OSC ended by `Bel` with BEL and to one ended by `St` with `ESC \`.
- Refuse an OSC string that `osc_end` reports truncated wherever part of it would do harm: an OSC 52 clipboard write, an OSC 8 link, an OSC 4 color.
- `dcs_hook` fires for every well-formed DCS string but XTGETTCAP, whatever its final byte. Pick the handler by the final byte, the private marker and the intermediates, and ignore the rest.
- Match intermediates and the private marker exactly, so that `CSI 0 % m`, which the conformance case `prog-vim` contains, does not run as SGR 0.
- Treat a `None` parameter as the control function's default.

## Bounds

The byte stream comes from an untrusted program, so nothing it sends grows memory. DCS and APC data are streamed to the consumer, whose own bounds apply: vtium-term's image handlers bound what they keep of a picture.

`OSC_MAX` is measured against what programs send in OSC strings. A title and an OSC 133 mark take tens to hundreds of bytes. VTE and iTerm2 cap an OSC 8 URI at 2083 bytes (egmontkoblinger's OSC 8 gist at `5965d6de`, "Length limits"), and xterm's manual has "A few OSC commands may be as large as 10 kilobytes" (`xterm.man`, `maxStringParse`). OSC 52 carries a selection in base64, four bytes for every three, so vtium-term's `CLIPBOARD_MAX` of 512 KiB takes 699,052 bytes after its `52;` and targets; the bound holds that, and one byte over the cap still reaches vtium-term, which refuses it. A string past the bound keeps its first `OSC_MAX` bytes and is marked truncated, and vtium-term acts on no truncated string. The bytes past the bound are dropped as they arrive, however the input is cut (`osc_at_its_bound_split_at_every_offset`). The buffer starts at 4 KiB, which holds a title, a mark or a URI, and doubles as a string needs, so a string at the bound reallocates it eight times and never once per piece of input; memory the system refuses truncates the string in place of ending the terminal. Each string is emptied from the buffer as its end is handed on, so a selection's base64 does not wait for the next string, and a buffer grown past 64 KiB is released then, so a long OSC 52 leaves no megabyte behind (`the_osc_buffer_doubles_to_the_bound_and_is_released_after_a_long_string`, `the_osc_string_is_cleared_as_it_ends`).

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Citation |
|---|---|---|---|---|
| `PARAMS_MAX` | 16 | Parameters a CSI sequence or DCS string keeps | Dropped, and the sequence still dispatches with 16 | vtium's own: the parser holds the parameters inline, with no allocation, so this bound sets its size |
| `SUBPARAMS_MAX` | 8 | Colon sub-parameters one parameter keeps | The colon and every digit up to the next `;` are dropped, and the sequence still dispatches | vtium's own: ITU T.416's full direct-color form, `38:2:Pi:R:G:B:unused:tolerance:tolerance-space`, takes eight sub-parameters after the 38, and the parser holds them inline |
| `PARAM_VALUE_CAP` | 65535 | A parameter or sub-parameter takes digits while it is at most this | Further digits are dropped, so a value reaches at most 655359 | vtium's own: vt100.net asks a parser to take values up to at least 16383, and a value of up to 655359 is more than any count or position on a screen within vtium-term's bounds |
| `OSC_MAX` | 1048576 | Bytes an OSC string keeps, as UTF-8, in a buffer that doubles from 4 KiB to at most this size and is released after a string past 64 KiB: an OSC 52 payload at vtium-term's `CLIPBOARD_MAX` with room to spare, and 500 times an OSC 8 URI at the 2083 bytes VTE and iTerm2 allow | The character that would pass it and everything after it are dropped as they arrive, so the kept bytes are a prefix, and the string still dispatches, marked truncated; vtium-term acts on no truncated string, so it is dropped whole | vtium's own, at tmux's `input-buffer-size` (`tmux.h:3509` at `df302c5`), which drops a longer sequence; xterm ignores a string past `maxStringParse`, 20000 bytes, or 600000 with sixel or ReGIS (`main.h:277-283` at `5c75ead`); Ghostty keeps 2048 bytes of an OSC string and 8 MiB of an OSC 52 (`src/terminal/osc.zig:361, 365` at `a732580`) and drops a longer one; foot grows its buffer with no bound short of half the address space (`osc.c:1708-1723` at `152487e`) |
| `XTGETTCAP_MAX` | 1039 | Bytes an XTGETTCAP payload keeps: 16 hex-encoded names of 32 bytes and the `;` between each two | The rest is dropped and the request reaches `xtgettcap` with no payload, which vtium-term answers `DCS 0 + r ST` | vtium's own: the 16 names vtium-term answers in one request, each at most its `TERMCAP_NAME_MAX`, 32 bytes, hex-encoded |
| `INTERMEDIATES_MAX` | 2 | Intermediate bytes an ESC, CSI or DCS sequence keeps, in order | The sequence dispatches nothing | vt100.net, collect: "If more than two intermediate characters arrive, the parser can just flag this so that the dispatch can be turned into a null operation" |
<!-- end of bounds table -->
