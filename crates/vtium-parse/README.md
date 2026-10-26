# vtium-parse

vtium's escape-sequence parser: bytes from the pty in, terminal actions out. It decodes UTF-8 (or 8-bit or 7-bit input), runs the DEC/ANSI state machine of the C reference's `tsm-vte.c`, and drives a `Consumer` with printable characters, C0 and C1 controls, and complete ESC, CSI, OSC, DCS and APC sequences. It forbids unsafe code, has no dependencies, holds every buffer inline and allocates nothing. Charset mapping, screen effects and replies belong to `vtium-term`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation and public names. |
| `src/consumer.rs` | The `Consumer` trait, one method per kind of action, and the `Chunk` and `InputMode` types its methods take. |
| `src/parser.rs` | `Parser`: the state machine, ported from `parse_data`, `do_trans` and `do_action`, with the collection half of each handler. |
| `src/params.rs` | `Params` and `Intermediates`, in the C reference's representation: `int` parameters with -1 for omitted, and intermediates as `csi_flags`. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its C citation: the one source for the constants and for the table below. |
| `src/utf8.rs` | The UTF-8 decoder in front of the state machine, written to the Unicode Standard, and the encoder OSC strings are collected with, ported from `tsm-unicode.c`. |
| `tests/common/mod.rs` | A consumer that records every action and checks, at each call, the bounds and ranges the parser promises. |
| `tests/state_machine.rs` | Each state's transitions, transcribed from `parse_data` and libtsm's test_vte cases. |
| `tests/bounds.rs` | Every bound at its edge, and the C reference's behaviour past it. |
| `tests/xtgettcap.rs` | XTGETTCAP collection, from libtsm's `test_vte_xtgettcap_*` cases. |
| `tests/input.rs` | UTF-8, 8-bit and 7-bit input, mode changes mid-input, and pausing. |
| `tests/random.rs` | Seeded random and adversarial streams: no panic, every bound holds, and a stream cut anywhere, fed a byte at a time or paused gives the actions it gives whole. |
| `fuzz/` | The cargo-fuzz target, its own workspace so CI does not build it: `cargo +nightly fuzz run parse`. |

`cargo test -p vtium-parse` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_PARSE_BLESS=1 cargo test -p vtium-parse readme_table` rewrites it.

## Behaviour

The C reference is the floor: vtium does everything it does, and where it departs from a protocol specification, vtium follows the specification. The state machine otherwise reproduces the C reference's behaviour, each rule cited to its line in `tsm-vte.c`, so that the conformance goldens judge it unchanged. Each departure is listed below with its reason and the specification it follows, and each has a test seen red against the C behaviour.

## Departures

- **UTF-8 decoding follows the Unicode Standard.** Each maximal subpart of an ill-formed sequence becomes one U+FFFD, the byte that ends it is decoded again from the start, and overlongs, surrogates and values past U+10FFFF are ill-formed, so no ill-formed input yields ESC or any other control. An incomplete sequence survives a cut in the input, and a reset drops it with no U+FFFD. The C reference took overlong forms, surrogates and values past U+10FFFF, and consumed the byte that broke a sequence; it is fixed to the same contract in a separate aaronsb/libtsm change. Spec: Unicode Table 3-7, and §3.9, "U+FFFD Substitution of Maximal Subparts".
- **A reset drops the `\` of an APC's ST.** After a reset between the ESC that ended an APC string and the `\` of its ST, that `\` is dropped. The C reference printed it, a stray character after a host cancelled a kitty request (aaronsb/libtsm#53).
- **Every empty parameter is omitted.** An empty parameter is -1 wherever it falls, so an explicit 0 and a default stay distinct; the C reference made an empty parameter before a `;` 0 (tsm-vte.c:1284-1286). Spec: ECMA-48 5.4.2 e), "An empty parameter sub-string represents a default value which depends on the control function", and g).
- **C1 controls do nothing in UTF-8 mode.** U+0080 to U+009F (`C2 80` to `C2 9F`) are neither executed nor printed, so a program that strips ESC from untrusted text also stops every control sequence; `C2 9B` was CSI in the C reference (tsm-vte.c:3289-3319). C1 controls still work in their 7-bit form (ESC Fe) and as bytes in 8-bit mode. ECMA-48 5.3 defines C1 for 7-bit and 8-bit codes only; this is the operator's decision.
- **GR acts as GL inside sequences in 8-bit mode.** A byte from A0 to FF inside an ESC, CSI or DCS header acts as the byte 80 below it, so `9B 31 ED` is SGR 1; in ground and in strings it stays itself. The C reference ignored such a CSI, and collected the byte as an ESC intermediate or took it as a DCS final. Spec: vt100.net, "In all states, these codes are treated identically to GL codes 20-7F."
- **A character past ASCII ends a sequence header in UTF-8 mode.** In a CSI or DCS header it ignores the sequence to its end, and in an ESC sequence it drops the sequence. The C reference did the first for CSI only, took the character as a DCS final, and collected it as an ESC intermediate (tsm-vte.c:3377, 3512). No specification covers a character past ASCII inside a sequence; this is the operator's decision, following the C reference's own CSI rule.
- **A truncated OSC string is a prefix.** Once a character would pass `OSC_MAX`, nothing after it is kept; the C reference kept a shorter character after the one it dropped, a string the program never sent (tsm-vte.c:2915-2917). No specification bounds OSC strings, so this is the operator's decision.

## Bounds

The byte stream comes from an untrusted program, so nothing it sends grows memory. Intermediates are a set of flags, so their count takes no room, and DCS and APC data are streamed to the consumer, whose bound applies, as the C reference's sixel decoder and kitty parser bound theirs.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | C reference |
|---|---|---|---|---|
| `PARAMS_MAX` | 16 | Parameters a CSI sequence or DCS string keeps | Dropped, and the sequence still dispatches with 16 | `CSI_ARG_MAX`, tsm-vte-int.h:25; tsm-vte.c:1283, 1293-1294, 2542-2543 |
| `SUBPARAMS_MAX` | 8 | Colon sub-parameters one parameter keeps | The colon and every digit up to the next `;` are dropped, and the sequence still dispatches | `CSI_SUBARG_MAX`, tsm-vte-int.h:28; tsm-vte.c:1298-1308 |
| `PARAM_VALUE_CAP` | 65535 | A parameter or sub-parameter takes digits while it is at most this | Further digits are dropped, so a value reaches at most 655359 | tsm-vte.c:1314-1316 |
| `OSC_MAX` | 127 | Bytes an OSC string keeps, as UTF-8: `OSC_MAX_LEN` (128) less the NUL that ends it | The character that would pass it and everything after it are dropped, so the kept bytes are a prefix, and the string still dispatches | `OSC_MAX_LEN`, tsm-vte-int.h:31; tsm-vte.c:2912-2921, 3006 |
| `XTGETTCAP_MAX` | 1039 | Bytes an XTGETTCAP payload keeps: 16 hex-encoded names of 32 bytes and the `;` between each two | The rest is dropped and the request is flagged `overflowed`, which the C reference answers `DCS 0 + r ST` | `XTGETTCAP_MAX_LEN`, tsm-vte-int.h:33-38; libtsm.h:1538; tsm-vte.c:3087-3091, 3149-3152 |
<!-- end of bounds table -->
