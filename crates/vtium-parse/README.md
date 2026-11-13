# vtium-parse

vtium-parse is vtium's escape-sequence parser. It takes in the bytes that arrive from the pty, and it puts out terminal actions. It decodes UTF-8, and it also reads 8-bit or 7-bit input. It runs the DEC/ANSI state machine that vt100.net describes. It drives a `Consumer` with printable characters, C0 and C1 controls, and complete ESC, CSI, OSC, DCS and APC sequences. The crate forbids unsafe code and has no dependencies. It holds every buffer inline, apart from the buffer of the OSC string. That buffer grows to at most its bound, and the parser releases it after a long string. Charset mapping, screen effects and replies belong to `vtium-term`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation and public names. |
| `src/consumer.rs` | The `Consumer` trait, with one method per kind of action, and the `Chunk` and `InputMode` types that its methods take. |
| `src/parser.rs` | `Parser`: the state machine, and the collection of parameters, intermediates and strings. |
| `src/params.rs` | `Params` and `Intermediates`. Each parameter is an `Option<u32>`, which is `None` when the parameter is omitted. `Params::marker` holds the private marker apart from the parameters. `Intermediates` holds the intermediate bytes in order. |
| `src/bounds.rs` | Every bound, with its value, what happens past it, and its citation. This file is the one source for the constants and for the table below. |
| `src/utf8.rs` | The UTF-8 decoder in front of the state machine. It states its contract to the Unicode Standard in seven rules. It also lists the noncharacters that an OSC string drops. |
| `tests/` | The integration tests. See its README. |
| `fuzz/` | The cargo-fuzz target. It is a workspace of its own, so CI does not build it. See its README. |

`cargo test -p vtium-parse` runs every test. The bounds table below is generated from `src/bounds.rs`. The test `readme_table_matches_bounds` fails when the table drifts from that file. `VTIUM_PARSE_BLESS=1 cargo test -p vtium-parse readme_table` rewrites the table.

## Behaviour

The parser follows vt100.net's description of the DEC parser. It follows ECMA-48 where ECMA-48 defines the syntax of a control function. Sometimes the two read differently, or neither covers a case. For those cases, the sections below give the rule the parser follows and why. Each rule has a test in `tests/`.

UTF-8 decoding follows the Unicode Standard. `src/utf8.rs` gives its seven rules.

### Strings

- **A string acts only when its terminator completes it.** The end of an OSC, DCS or APC string carries a `StringEnd`. `St` (0x9C, or ESC followed by `\`) and `Bel` complete the string. CAN, SUB, another C1 control, or an ESC followed by anything else cancels the string. A consumer acts only on a complete string. So after an ESC, the parser waits for the next character before it ends the string. A kitty command therefore reaches the consumer at the `\` of its ST. Spec: vt100.net says "the C0 controls CAN and SUB cancel any escape sequence, control sequence or control string in progress". It also says that ESC "will "cancel" the control string". ECMA-48 8.3.6 says "this data shall be ignored", and ECMA-48 5.6 says that a control string ends with ST.
- **A reset drops the `\` of an APC's ST.** The host can reset the parser between the ESC that ended an APC string and the `\` of its ST. The host does this when it cancels a kitty request. The parser then drops that `\`, so no stray character prints (vtium/vtium#9). After a reset at the ESC of an OSC or a DCS string, the `\` prints.
- **An OSC string collects DEL.** The parser follows vt100.net's `osc_put`, which takes DEL into the string. ECMA-48 5.6 would leave DEL out of a control string.

### Sequences

- **Intermediates keep their order, count and bytes.** An ESC, CSI or DCS sequence passes its intermediates in order, each as its own byte. It passes its private marker apart from them. So `CSI ! ! p` is not DECSTR. A third intermediate makes the sequence do nothing (`INTERMEDIATES_MAX`). Spec: ECMA-48 5.4 c) says of intermediate bytes, "Together with the Final Byte F, they identify the control function". vt100.net says under collect: "If more than two intermediate characters arrive, the parser can just flag this so that the dispatch can be turned into a null operation."
- **Every empty parameter is omitted.** An empty parameter is `None` wherever it falls, before a `;` too. So an explicit 0 and a default stay distinct. Spec: ECMA-48 5.4.2 e), "An empty parameter sub-string represents a default value which depends on the control function", and g).
- **A character past ASCII ends a sequence header in UTF-8 mode.** In a CSI or DCS header, the parser ignores the sequence up to its end. vt100.net's parser does the same with a byte that a header cannot hold. In an ESC sequence, the parser drops the sequence. No specification covers a character past ASCII inside a sequence.

### Input

- **C1 controls do nothing in UTF-8 mode.** The parser neither executes nor prints U+0080 to U+009F (`C2 80` to `C2 9F`). So a program that strips ESC from untrusted text also stops every control sequence. C1 controls still work in their 7-bit form (ESC Fe), and as bytes in 8-bit mode. ECMA-48 5.3 defines C1 for 7-bit and 8-bit codes only. The maintainer chose to ignore the decoded form. Other terminals differ:
  - foot ignores them. Its decoder hands U+0080 to U+009F to the printer, which draws only characters of positive width (`vt.c:1043` and `terminal.c:4228, 4447-4448` at `152487e`). Its comment at `vt.c:257-266` says that it ignores C1 in UTF-8 mode, as xterm does.
  - Ghostty ignores them (`src/terminal/stream.zig:1334-1357` at `9ee02ae`). The code there says "Ignore C1 that came via UTF-8 decoding, matching xterm". Its test is at line 3764.
  - WezTerm acts on them as controls. `vtparse/src/lib.rs:690-709` at `e68a8ec` runs a decoded U+0080 to U+00FF through the state machine. Its test `utf8_control` at lines 1035-1040 expects U+008D to execute. `wezterm-escape-parser/src/parser/mod.rs:218-220` turns that into a control.
- **GR acts as GL inside sequences in 8-bit mode.** Inside an ESC, CSI or DCS header, a byte from A0 to FF acts as the byte 80 below it. So `9B 31 ED` is SGR 1. In ground state and in strings, the byte stays itself. Spec: vt100.net, "In all states, these codes are treated identically to GL codes 20-7F."
- **7-bit mode masks bit 8,** so 0x9B is ESC and 0xC1 is `A`.
- **DEL reaches `print` in ground state,** as vt100.net has it. vtium-term decides whether it draws.

## Notes for vtium-term

- Draw a DEL (0x7F) that reaches `print` only when a 96-character set is in GL. Ignore the DEL otherwise. vt100.net says "When a 96-character set is mapped into GL, both 20 and 7F may cause a character to be displayed". It also says "terminals only claiming VT220 compatibility can always ignore 7F".
- Act on an OSC, DCS or APC string only when its `StringEnd` is complete (`St` or `Bel`). Reply to an OSC string ended by `Bel` with BEL. Reply to an OSC string ended by `St` with `ESC \`.
- `osc_end` reports when an OSC string is truncated. Refuse a truncated string wherever part of the string would do harm: an OSC 52 clipboard write, an OSC 8 link, or an OSC 4 color.
- `dcs_hook` fires for every well-formed DCS string apart from XTGETTCAP, whatever its final byte. Pick the handler by the final byte, the private marker and the intermediates. Ignore the rest.
- Match intermediates and the private marker exactly. Then `CSI 0 % m`, which the conformance case `prog-vim` contains, does not run as SGR 0.
- Treat a `None` parameter as the control function's default.

## Bounds

The byte stream comes from an untrusted program. So nothing that the program sends can grow the parser's memory without a bound. The parser streams DCS and APC data to the consumer, and the consumer's own bounds apply. For example, vtium-term's image handlers bound what they keep of an image.

`OSC_MAX` is measured against what programs send in OSC strings. A title and an OSC 133 mark take tens to hundreds of bytes. VTE and iTerm2 cap an OSC 8 URI at 2083 bytes (egmontkoblinger's OSC 8 gist at `5965d6de`, "Length limits"). xterm's manual says "A few OSC commands may be as large as 10 kilobytes" (`xterm.man`, `maxStringParse`).

OSC 52 carries a selection in base64, which takes four bytes for every three. So vtium-term's `CLIPBOARD_MAX` of 512 KiB takes 699,052 bytes after its `52;` and targets. The bound holds that many bytes. A selection one byte over the cap still reaches vtium-term, which refuses it.

A string past the bound keeps its first `OSC_MAX` bytes, and the parser marks the string truncated. vtium-term acts on no truncated string. The parser drops the bytes past the bound as they arrive, however the input is cut (`osc_at_its_bound_split_at_every_offset`).

The buffer starts at 4 KiB, which holds a title, a mark or a URI. The buffer doubles as a string needs more room. So a string at the bound reallocates the buffer eight times, and never once per piece of input. When the system refuses memory for the buffer, the parser truncates the string. The parser does not end the terminal.

The parser empties each string from the buffer as it hands on the string's end. So a selection's base64 does not wait in the buffer for the next string. At that point the parser also releases a buffer that has grown past 64 KiB. So a long OSC 52 leaves no megabyte behind (`the_osc_buffer_doubles_to_the_bound_and_is_released_after_a_long_string`, `the_osc_string_is_cleared_as_it_ends`).

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
