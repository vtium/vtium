# Architecture

vtium is a Cargo workspace. Each crate owns one responsibility, and the dependency arrows point one way: from the binary down to the terminal state and the platform.

```text
vtium (binary)
├── vtium-platform   displays (DRM/KMS, fbdev), VT, seat, input, pty, event loop
├── vtium-render     frames to pixels: CPU compositor; GPU path behind a feature
│   ├── vtium-term   screen, cells, lines, scrollback, modes, SGR, replies
│   │   └── vtium-parse   bytes to actions: the bounded escape-sequence parser
│   └── vtium-font   glyph sources: PSF and unifont; outline fonts behind a feature
└── vtium-image      sixel and kitty graphics, the image store (feature `images`)
```

Only `vtium-platform` may use `unsafe`. Every other crate carries `#![forbid(unsafe_code)]`.

`vtium-platform` makes its syscalls through rustix, with no `libc` call and no `extern "C"` declaration for one (vtium/vtium#6). Its `unsafe` is of four kinds: an ioctl rustix does not wrap, defined once in the crate's ioctl module with a test that its request number equals the kernel header's; an mmap of a framebuffer; the pre-exec hooks of the pty child and the suspend shell, which make only raw syscalls (both clear the signal mask, and the pty child's makes it a session leader with the pty as its controlling terminal); and FFI to a C library where vtium must call one, such as libseat (vtium/vtium#34). C stays at that boundary. The crate denies `unsafe_code`, so each site carries an `#[expect]` with its reason.

The console starts its children, the session on the pty and the power keys' suspend shell, only from its one thread, marking every inherited descriptor close-on-exec just before the fork (vtium/vtium#133); a new thread that opens descriptors must revisit that call, which a descriptor opened between the marking and the fork would pass.

## The minspec build

The default build is minspec: the smallest complete console. It covers drm2d or fbdev, the CPU renderer, bitmap fonts, the parser and screen, the pty and keyboard input. Images, outline fonts and the GPU path are features added on top. `cargo build --profile minspec` builds it size-optimised. Its size and memory are measured from the first working build.

The machine minspec is judged against is a single-board computer that presents a DRM buffer (vtium/vtium#75):

- ARM cores, Cortex-A53 class or older, aarch64 and armv7;
- 512 MB of memory or less;
- a DRM dumb buffer to draw into, with no GPU vtium uses;
- displays that may be small, down to SPI panels through tinydrm.

The display floor is a 320x240 panel with an 8x8 font, a 40 by 30 grid. There vtium starts, draws and stays correct, and a shell, line editing and vim are usable; a program that needs 80 columns is not promised to fit. At 640x480 with an 8x16 font, 80 by 30, everything vtium supports is usable. The 8x8 font comes from a console font on the system for now (vtium/vtium#75).

Tier 3's x86 variant keeps the small-machine standard: 1366x768, 1 vCPU, 512 MB.

`make arm` builds the minspec profile for `aarch64-unknown-linux-gnu` and `armv7-unknown-linux-gnueabihf`, and `make arm-test` runs the workspace's tests on aarch64 under qemu-user, with no cross toolchain and no root ([tools/arm/README.md](../tools/arm/README.md)).

## Where each part comes from

vtium reimplements in Rust what the [aaronsb/kmscon](https://github.com/aaronsb/kmscon) fork's components do. The fork models behaviour, and its code is the floor for it (see [Beyond the C reference](#beyond-the-c-reference)). How vtium gets there is idiomatic Rust: its types, ownership and modules follow Rust, not the C code's layout. The table names the components whose behaviour each crate takes on.

| vtium crate | Fork components it replaces |
|---|---|
| `vtium-parse` | libtsm's escape parser (`tsm-vte.c` state machine) |
| `vtium-term` | libtsm's screen, selection, mode registry (`tsm-modes.def`), replies, Unicode tables |
| `vtium-image` | libtsm's sixel decoder, kitty graphics parser and image store; kmscon's image budget and helper |
| `vtium-font` | kmscon's font backends (PSF, unifont, FreeType, Pango) and the box/block sprites |
| `vtium-render` | kmscon's text renderers (bbulk, gltex) and the render timer |
| `vtium-platform` | kmscon's uterm (video, VT, monitor), seat, input, pty and event loop |
| `vtium` | kmscon's `main.c`, config and terminal wiring |

[c-reference.md](c-reference.md) maps how each of those components worked, as logic flows cited to the forks' code, with the vtium issue that takes each one and whether vtium's plan follows it.

## How it is judged

By vtium's harness, in tiers:

- **Unit tests** in each crate, run by `cargo test`.
- **Conformance:** byte streams in, screen state and replies out, compared with reviewed goldens. They cover the parser and terminal state with no display.
- **Capability probes:** each capability vtium claims has a probe that fails when the capability is missing, judged against the capability list (below).
- **Pixel tests (tier 2):** the renderer draws known screens, compared pixel for pixel with goldens.
- **VM runs (tier 3):** a VM boots vtium on a VT, drives keys and compares screenshots pixel for pixel, including a minspec variant on the small-machine standard.
- **VT review:** the maintainer judges a build by eye on a real VT, guided step by step, started only by the steering session ([vt-review.md](vt-review.md)).

Work is built in bundles of items that don't collide, then evaluated once per bundle.

## Beyond the C reference

vtium covers everything the C reference does, apart from the listed departures below, judged by the C reference's goldens and probes, and goes further. The capabilities the C reference lacked are vtium issues.

Where the C reference departs from a protocol specification, vtium follows the specification. Each departure is listed in the departures list beside vtium's conformance runner (vtium/vtium#14), with its reason and the specification it follows, and the runner expects vtium's own output for exactly the listed cases. A defect found in the C reference, a security defect included, is a departure like any other: vtium fixes it and lists it. The C reference is frozen: it changed for the test infrastructure vtium was judged by before its goldens were copied into vtium (#111), and for the one security fix the operator asked for (aaronsb/libtsm#83).

A character decoded from UTF-8 prints as itself whatever G1 to G3 hold, as in xterm's UTF-8 mode; the C reference ran U+00A1 to U+00FE through GR (vtium/vtium#13).

A bound vtium takes from the C reference is a starting value. vtium may set its own, with the reason in the row that defines it.

## Determinism

The screen, the scrollback, the modes, the cursor, the image store and the replies follow from the bytes a program sends, with time entering only at the places named below.

| Rule | The check that fails when it breaks |
|---|---|
| **Chunking invariance.** The state after a byte stream does not depend on how the stream was split or how fast it came. The input work bound (a fixed pixel count per input call) and the pause while an image decodes change when work is done, never what state results. | A property test feeds each stream whole and cut at random points, mid-UTF-8, mid-sequence, at the work bound and across a pause, and compares the full state and the reply bytes. The streams come from a grammar-aware generator and from raw random bytes, and a failure prints its seed. |
| **Bounds.** Every resource has a bound. The parser's are in `vtium-parse`'s bounds table; the image store's, the scrollback's and the replies' are rows of their own crates' bounds tables, added with those crates. Input past a bound gives the state its row describes. | A flood past each row of a bounds table gives that row's state. |
| **Eviction follows the bytes.** The image store evicts by rank, then by a per-store use clock that only stream events advance (a transmission or a placement), and placed kitty images oldest transmission first. A reservation that cannot fit is refused before anything is evicted. A sixel placed on the screen's own lines is never evicted, so a store full of them refuses. The C reference also renewed an image's use on every drawn frame, which made eviction depend on which frames were drawn and when; vtium lists that as a departure. | The flood test fed whole and chunked ends in the same store, with frames drawn at different points. |
| **Time enters in named places only.** A decode the helper does not finish in 2 seconds (the timer is held in `vtium-platform`) gets the `FAILED` reply, the helper is stopped and the next request starts a new one, and input stays paused until either outcome, so no input reorders around it. Mode 2026's timeout, held in `vtium-platform`, changes when a frame shows, never the state behind it. The conformance runner's fixture host answers synchronously, so no case depends on time, and the chunking test runs against it. The binary's respawn limit, 5 quick restarts within 2 seconds, is given each exit's time from the monotonic clock, which the event loop reads; it decides only whether the session's command starts again, never the terminal's state. | `clippy::disallowed_methods` denies `Instant::now`, `SystemTime::now` and `thread::sleep`. The list lives in the workspace `clippy.toml`; `vtium-platform` and `vtium-render`, which hold the timers, allow the lint, and the eviction test guards the render timer's effect on state. The respawn policy (`crates/vtium/src/respawn.rs`) takes the time as an argument, and its tests pass the times in. |
| **No order from a hash.** std's `HashMap` iterates in a randomised order, so nothing that reaches the screen, a reply or eviction iterates one. | `clippy::disallowed_types` denies `HashMap` and `HashSet`, listed in the workspace `clippy.toml`. `vtium-platform` and the binary allow it where no order reaches state. |
| **No input-reachable panic.** Arithmetic, casts and indexing on program-controlled values are checked, saturating or explicitly wrapping, and nothing unwraps. | `arithmetic_side_effects`, `cast_possible_truncation`, `cast_possible_wrap`, `cast_sign_loss`, `indexing_slicing`, `unwrap_used`, `expect_used`, `panic`, `unreachable` and `todo` are denied in the `lib.rs` of `vtium-parse`, `vtium-term`, `vtium-image` and `vtium-render`; CI runs clippy with warnings denied. An allowed site carries `#[expect(..., reason = "...")]`. Tests and fuzzing build with overflow checks. |
| **No reflection.** A reply is built only from vtium's state, fixed strings, and values it parses and re-encodes: numbers, a selector from a fixed set (such as the BEL or ST that ended the query), and hex. It never contains raw request bytes or text stored from the stream, so injected sequences can rewrite the screen but cannot type into the program reading the replies. An XTGETTCAP name is printed in uppercase hex from the matched table entry; the C reference echoed the request's spelling, which vtium lists as a departure. Title reporting answers with a fixed empty title, and OSC 52 reads stay off. | A test sends every query vtium answers with a random token in each field it accepts, and fails if any reply contains the token or any substring of the injected text of 4 bytes or more. It also pins each reply's exact bytes for a fixed-state fixture. The list of queries is generated from the code that writes replies, so a new reply cannot escape it. Title reporting after a hostile OSC 0 or 2 gives the fixed empty title, and an OSC 52 query gives no reply. |

The release and minspec profiles set `panic = "abort"`, so any panic ends the console. They leave overflow checks off: a missed overflow then wraps, which is defined behaviour, and the lints and fuzzing are what find it.

## Tables, references and maps

A fact vtium states in more than one place has one source. Where a test can be written, the other places are generated from the source or checked against it, and the test fails on drift. The coverage map and the path tables are kept by hand, in the change that alters what they describe.

| Source | What is generated or checked | The test fails when |
|---|---|---|
| The mode table in `vtium-term`: one row per ANSI or DEC private mode vtium recognises, with kind, number, name, status (supported, partial, ignored, refused), default (fixed or a host setting), reset, what the host must do, terminfo, spec and tests | The dispatch's and the resets' calls into each mode's accessors, and the DECRQM replies; `docs/modes.md` and `docs/modes.tsv`, describing the build with `images` | The committed `docs/modes.md` or `.tsv` differs from the table; a number with no row (DEC 0-65535, ANSI 0-255) gets a DECRQM reply other than 0, or SM and RM of it change any of the terminal's state (cell contents, tab stops and image state included); a number with a row replies 0; SM, RM or DECRQM do anything but expand the table; a mode's state is reachable outside its accessors, which vtium enforces with module privacy where the C reference needed a source check; a supported mode's row names no test that sets and resets it, or names a test that does not exist |
| The parser's bounds: rows in `vtium-parse`'s `src/bounds.rs`, from which its constants take their values | The bounds table in the crate's README | The committed table differs from the rows |
| The capability list: each probe `works` or `missing`, with the issue for each gap; a `works` probe may name the feature it needs | The probe verdicts | A `works` probe fails, or fails to fail without its feature; a `missing` probe passes; a probe is not in the list, or the list names one that does not exist |
| The key table in the binary (`crates/vtium/src/config/keys.rs`): one row per configuration key and command-line option, with its type, default, bound, the crate that reads it and its source | `docs/config.md` and the installed example, `crates/vtium/install/kmscon.conf.example` | Either committed file differs from the table |
| The terminfo entry | The XTGETTCAP table | The committed table differs from the entry |
| The departures list: each case where vtium follows a specification the C reference departed from, with its reason, the specification and vtium's expected output | The conformance runner's expected output | A case differs from the C reference's golden and is not listed, or a listed case matches the golden |

The coverage map lists each feature vtium builds, the tests that guard it and the paths they reach (font, renderer, display), and the issue for each gap. A gap is closed by a light test in the same change or filed.

Every directory of tests or data carries a README with a table of what each path is.

The mode table follows libtsm's `tsm-modes.def`, and the capability list and terminfo entry follow kmscon's, so vtium is judged by the conformance corpus and the probe verdicts the C reference passed.
