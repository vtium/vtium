# vtium-parse

vtium's escape-sequence parser: bytes from the pty in, terminal actions out. It decodes UTF-8 (or 8-bit or 7-bit input), runs the DEC/ANSI state machine of the C reference's `tsm-vte.c`, and drives a `Consumer` with printable characters, C0 and C1 controls, and complete ESC, CSI, OSC, DCS and APC sequences. It forbids unsafe code, has no dependencies, and holds every buffer inline but the OSC string's, which the first OSC string reserves at its bound and every later one reuses. Charset mapping, screen effects and replies belong to `vtium-term`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation and public names. |
| `src/consumer.rs` | The `Consumer` trait, one method per kind of action, and the `Chunk` and `InputMode` types its methods take. |
| `src/parser.rs` | `Parser`: the state machine and the collection of parameters, intermediates and strings, its behaviour cited to the C reference's `tsm-vte.c`. |
| `src/params.rs` | `Params` and `Intermediates`: each parameter an `Option<u32>`, `None` when omitted, the private marker apart as `Params::marker`, and the intermediates as their bytes in order. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its citation: the one source for the constants and for the table below. |
| `src/utf8.rs` | The UTF-8 decoder in front of the state machine, with its contract to the Unicode Standard in seven rules, and the noncharacters an OSC string drops, as `tsm-unicode.c` has them. |
| `tests/` | The integration tests: see its README. |
| `fuzz/` | The cargo-fuzz target, its own workspace so CI does not build it: see its README. |

`cargo test -p vtium-parse` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_PARSE_BLESS=1 cargo test -p vtium-parse readme_table` rewrites it.

## Behaviour

The C reference is the floor: vtium does everything it does, and where it departs from a protocol specification, vtium follows the specification. The state machine otherwise reproduces the C reference's behaviour, each rule cited to its line in `tsm-vte.c`, so that the conformance goldens judge it unchanged. Each departure is listed below with its reason and the specification it follows, and each has a test seen red against the C behaviour.

UTF-8 decoding follows the Unicode Standard, and the C reference's decoder follows the same contract (aaronsb/libtsm#83); its seven rules are in `src/utf8.rs`.

## Departures

- **A string acts only when its terminator completes it.** The end of an OSC, DCS or APC string carries a `StringEnd`: `St` (0x9C, or ESC followed by `\`) or `Bel` complete it, and CAN, SUB, another C1 control, or an ESC followed by anything else cancel it; a consumer acts only on a complete string. An ESC therefore waits for the next character before the string ends. The C reference acted on a string however it ended, and ended it at the ESC (tsm-vte.c:3291-3301, 3004-3015); it waited for the `\` only for XTGETTCAP (tsm-vte.c:3162-3167, 3282-3287), the rule vtium applies to every string. A kitty command reaches the consumer at the `\` of its ST. Spec: vt100.net, "the C0 controls CAN and SUB cancel any escape sequence, control sequence or control string in progress", and ESC "will "cancel" the control string"; ECMA-48 8.3.6, "this data shall be ignored", and 5.6, a control string ends with ST.
- **Intermediates keep their order, count and bytes.** An ESC, CSI or DCS sequence passes its intermediates in order, each as its own byte, and its private marker apart from them; a third intermediate makes the sequence do nothing (`INTERMEDIATES_MAX`). The C reference kept a set of flags, so `CSI ! ! p` ran as DECSTR and every unnamed intermediate was one flag, OTHER (tsm-vte.c:1232-1275). Spec: ECMA-48 5.4 c), intermediate bytes "Together with the Final Byte F, they identify the control function"; vt100.net, collect: "If more than two intermediate characters arrive, the parser can just flag this so that the dispatch can be turned into a null operation."
- **A reset drops the `\` of an APC's ST.** After a reset between the ESC that ended an APC string and the `\` of its ST, that `\` is dropped. The C reference printed it, a stray character after a host cancelled a kitty request (aaronsb/libtsm#53, vtium/vtium#9). The `\` after a reset at an OSC's or a DCS's ESC still prints, as in the C reference.
- **Every empty parameter is omitted.** An empty parameter is `None` wherever it falls, so an explicit 0 and a default stay distinct; the C reference made an empty parameter before a `;` 0 (tsm-vte.c:1284-1286). Spec: ECMA-48 5.4.2 e), "An empty parameter sub-string represents a default value which depends on the control function", and g).
- **C1 controls do nothing in UTF-8 mode.** U+0080 to U+009F (`C2 80` to `C2 9F`) are neither executed nor printed, so a program that strips ESC from untrusted text also stops every control sequence; `C2 9B` was CSI in the C reference (tsm-vte.c:3289-3319). C1 controls still work in their 7-bit form (ESC Fe) and as bytes in 8-bit mode. ECMA-48 5.3 defines C1 for 7-bit and 8-bit codes only; this is the operator's decision. Other terminals differ: foot ignores them (its decoder hands U+0080 to U+009F to the printer, which draws only characters of positive width, `vt.c:1043` and `terminal.c:4228, 4447-4448` at `152487e`, and its comment at `vt.c:257-266` says it ignores C1 in UTF-8 mode, as xterm does); Ghostty ignores them (`src/terminal/stream.zig:1334-1357` at `9ee02ae`, "Ignore C1 that came via UTF-8 decoding, matching xterm", tested at line 3764); WezTerm acts on them as controls (`vtparse/src/lib.rs:690-709` at `e68a8ec` runs a decoded U+0080 to U+00FF through the state machine, its test `utf8_control` at lines 1035-1040 expects U+008D to execute, and `wezterm-escape-parser/src/parser/mod.rs:218-220` turns that into a control).
- **GR acts as GL inside sequences in 8-bit mode.** A byte from A0 to FF inside an ESC, CSI or DCS header acts as the byte 80 below it, so `9B 31 ED` is SGR 1; in ground and in strings it stays itself. The C reference ignored such a CSI, and collected the byte as an ESC intermediate or took it as a DCS final. Spec: vt100.net, "In all states, these codes are treated identically to GL codes 20-7F."
- **A character past ASCII ends a sequence header in UTF-8 mode.** In a CSI or DCS header it ignores the sequence to its end, and in an ESC sequence it drops the sequence. The C reference did the first for CSI only, took the character as a DCS final, and collected it as an ESC intermediate (tsm-vte.c:3377, 3512). No specification covers a character past ASCII inside a sequence; this is the operator's decision, following the C reference's own CSI rule.
- **An OSC string keeps 1 MiB.** `OSC_MAX` holds an OSC 52 payload at vtium-term's cap, where the C reference kept 127 bytes, too few for a URI or a clipboard's base64. No specification bounds OSC strings; the value and its reasons are in the bounds table and below it.
- **A truncated OSC string is a prefix.** Once a character would pass `OSC_MAX`, nothing after it is kept, and `osc_end` reports the string truncated; the C reference kept a shorter character after the one it dropped, a string the program never sent (tsm-vte.c:2915-2917). No specification bounds OSC strings, so this is the operator's decision.

## C behaviour kept

Classified against ECMA-48, vt100.net's DEC parser and xterm's ctlseqs, and decided with the operator:

- **DEL prints in ground state** (tsm-vte.c:3333), as vt100.net has it.
- **OSC collects DEL** (tsm-vte.c:3598), following vt100.net's `osc_put` where ECMA-48 5.6 would exclude it.
- **A parameter stops taking digits past 0xffff** (tsm-vte.c:1314-1316); vt100.net asks for at least 16383.
- **7-bit mode masks bit 8** (tsm-vte.c:3660-3664).

## Notes for vtium-term

- Draw the DEL (0x7F) that reaches `print` only when a 96-character set is in GL, and ignore it otherwise: vt100.net, "When a 96-character set is mapped into GL, both 20 and 7F may cause a character to be displayed", and "terminals only claiming VT220 compatibility can always ignore 7F". The C reference wrote U+007F to the screen.
- Act on an OSC, DCS or APC string only when its `StringEnd` is complete (`St` or `Bel`). Reply to an OSC ended by `Bel` with BEL and to one ended by `St` with `ESC \`.
- Refuse an OSC string that `osc_end` reports truncated wherever part of it would do harm: an OSC 52 clipboard write, an OSC 8 link, an OSC 4 colour.
- `dcs_hook` fires for every well-formed DCS string but XTGETTCAP, whatever its final byte. Pick the handler by the final byte, the private marker and the intermediates, and ignore the rest; the C reference hooked only sixel, `DCS q` with no marker or intermediate.
- Match intermediates and the private marker exactly. The C reference's SGR ran with any intermediate (tsm-vte.c:2683-2690), so the `CSI 0 % m` that the conformance case `prog-vim` contains once ran as SGR 0. A port that rejects the `%` may change that case's screen where the SGR state matters.
- Treat a `None` parameter as the control function's default.

## Bounds

The byte stream comes from an untrusted program, so nothing it sends grows memory. DCS and APC data are streamed to the consumer, whose bound applies, as the C reference's sixel decoder and kitty parser bound theirs.

`OSC_MAX` is measured against what programs send in OSC strings. A title and an OSC 133 mark take tens to hundreds of bytes. VTE and iTerm2 cap an OSC 8 URI at 2083 bytes (egmontkoblinger's OSC 8 gist, "Length limits"), and xterm's manual has "A few OSC commands may be as large as 10 kilobytes" (`xterm.man`, `maxStringParse`). OSC 52 carries a selection in base64, four bytes for every three, so vtium-term's `CLIPBOARD_MAX` of 512 KiB takes 699,052 bytes after its `52;` and targets; the bound holds that, and one byte over the cap still reaches vtium-term, which refuses it. A string past the bound keeps its first `OSC_MAX` bytes and is marked truncated, and vtium-term acts on no truncated string. The bytes past the bound are dropped as they arrive, however the input is cut, and the buffer is reserved once, at the bound, so no string allocates again (`the_osc_buffer_is_reserved_once_at_its_bound`, `osc_at_its_bound_split_at_every_offset`).

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Citation |
|---|---|---|---|---|
| `PARAMS_MAX` | 16 | Parameters a CSI sequence or DCS string keeps | Dropped, and the sequence still dispatches with 16 | `CSI_ARG_MAX`, tsm-vte-int.h:25; tsm-vte.c:1283, 1293-1294, 2542-2543 |
| `SUBPARAMS_MAX` | 8 | Colon sub-parameters one parameter keeps | The colon and every digit up to the next `;` are dropped, and the sequence still dispatches | `CSI_SUBARG_MAX`, tsm-vte-int.h:28; tsm-vte.c:1298-1308 |
| `PARAM_VALUE_CAP` | 65535 | A parameter or sub-parameter takes digits while it is at most this | Further digits are dropped, so a value reaches at most 655359 | tsm-vte.c:1314-1316 |
| `OSC_MAX` | 1048576 | Bytes an OSC string keeps, as UTF-8, in one buffer the first OSC string reserves at this size and every later one reuses: an OSC 52 payload at vtium-term's `CLIPBOARD_MAX` with room to spare, and 500 times an OSC 8 URI at the 2083 bytes VTE and iTerm2 allow | The character that would pass it and everything after it are dropped as they arrive, so the kept bytes are a prefix, and the string still dispatches, marked truncated; vtium-term acts on no truncated string, so it is dropped whole | vtium's own, at tmux's `input-buffer-size` (`tmux.h:3509` at `df302c5`), which drops a longer sequence; xterm ignores a string past `maxStringParse`, 20000 bytes, or 600000 with sixel or ReGIS (`main.h:277-283` at `5c75ead`); Ghostty keeps 2048 bytes of an OSC string and 8 MiB of an OSC 52 (`src/terminal/osc.zig:361, 365` at `a732580`) and drops a longer one; foot grows its buffer with no bound short of half the address space (`osc.c:1708-1723` at `152487e`). The C reference kept 127 (`OSC_MAX_LEN`, tsm-vte-int.h:31; tsm-vte.c:2912-2921) |
| `XTGETTCAP_MAX` | 1039 | Bytes an XTGETTCAP payload keeps: 16 hex-encoded names of 32 bytes and the `;` between each two | The rest is dropped and the request reaches `xtgettcap` with no payload, which the C reference answers `DCS 0 + r ST` | `XTGETTCAP_MAX_LEN`, tsm-vte-int.h:33-38; libtsm.h:1538; tsm-vte.c:3087-3091, 3149-3152 |
| `INTERMEDIATES_MAX` | 2 | Intermediate bytes an ESC, CSI or DCS sequence keeps, in order | The sequence dispatches nothing (vt100.net); the C reference kept intermediates as a set of flags with no count bound | vt100.net, collect: "If more than two intermediate characters arrive, the parser can just flag this so that the dispatch can be turned into a null operation" |
<!-- end of bounds table -->
