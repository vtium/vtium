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

Only `vtium-platform` may use `unsafe`, for ioctls and FFI. Every other crate carries `#![forbid(unsafe_code)]`.

## The minspec build

The default build is minspec: the smallest complete console, for the small-machine standard (1366x768, 1 vCPU, 512 MB). It covers drm2d or fbdev, the CPU renderer, bitmap fonts, the parser and screen, the pty and keyboard input. Images, outline fonts and the GPU path are features added on top. `cargo build --profile minspec` builds it size-optimised. Its size and memory are measured from the first working build.

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

## How it is judged

By vtium's harness, in tiers:

- **Unit tests** in each crate, run by `cargo test`.
- **Conformance:** byte streams in, screen state and replies out, compared with reviewed goldens. They cover the parser and terminal state with no display.
- **Capability probes:** each capability vtium claims has a probe that fails when the capability is missing, judged against the capability list (below).
- **Pixel tests (tier 2):** the renderer draws known screens, compared pixel for pixel with goldens.
- **VM runs (tier 3):** a VM boots vtium on a VT, drives keys and compares screenshots pixel for pixel, including a minspec variant on the small-machine standard.

Work is built in bundles of items that don't collide, then evaluated once per bundle.

## Beyond the C reference

vtium covers everything the C reference does, apart from the listed departures below, judged by the C reference's goldens and probes, and goes further. The capabilities the C reference lacked are vtium issues.

Where the C reference departs from a protocol specification, vtium follows the specification. Each departure is listed in the departures list beside vtium's conformance runner (vtium/vtium#14), with its reason and the specification it follows, and the runner expects vtium's own output for exactly the listed cases. A defect found in the C reference, a security defect included, is a departure like any other: vtium fixes it and lists it. The C reference changes only for the test infrastructure vtium is judged by.

A bound vtium takes from the C reference is a starting value. vtium may set its own, with the reason in the row that defines it.

## Determinism

vtium's state follows from the bytes it reads, whatever a program sends and however the bytes arrive.

- **Chunking invariance.** The screen, the scrollback and the image store after a byte stream depend only on the bytes, never on how the pty split them or how fast they came. A property test feeds each stream whole and cut at random points, and requires the same state. The work bound and the image pause change when work is done, never what state results.
- **Every resource is bounded, and its overflow is defined.** The parser's buffers truncate as `vtium-parse`'s bounds table says. The image budget evicts in a fixed order (rank, then sequence number) and refuses a reservation it cannot fit rather than evicting first. A flood reaches a defined state, never a race.
- **Time enters in named places only, each with a defined outcome.** A decode the helper does not finish in time gets a refusal reply, and input stays paused until either outcome, so nothing reorders around it. Mode 2026's timeout changes when a frame shows, never the state behind it. The replay harness's fixture host answers synchronously, so the conformance runs contain no time.
- **No observable order comes from a hash.** Rust seeds `HashMap` per process, so nothing that reaches the screen, a reply or eviction iterates one. Images and placements live in ordered maps or carry sequence numbers.
- **Arithmetic on input-derived values is checked or saturating.** Sizes computed from a program's parameters (width × height × 4, cell boxes, offsets) never wrap. `clippy::arithmetic_side_effects` is denied in the crates that read the pty's bytes (`vtium-parse`, `vtium-term`, `vtium-image`). Tests and fuzzing run with overflow checks. The release profiles keep them off, because with `panic = "abort"` a missed case would let input end the console.

## Tables, references and maps

A fact vtium states in more than one place has one source. Where a test can be written, the other places are generated from the source or checked against it, and the test fails on drift. The coverage map and the path tables are kept by hand, in the change that alters what they describe.

| Source | What is generated or checked | The test fails when |
|---|---|---|
| The mode table in `vtium-term`: one row per ANSI or DEC private mode vtium recognises, with kind, number, name, status (supported, partial, ignored, refused), default (fixed or a host setting), reset, what the host must do, terminfo, spec and tests | The dispatch's and the resets' calls into each mode's accessors, and the DECRQM replies; `docs/modes.md` and `docs/modes.tsv`, describing the build with `images` | The committed `docs/modes.md` or `.tsv` differs from the table; a number with no row (DEC 0-65535, ANSI 0-255) gets a DECRQM reply other than 0, or SM and RM of it change any of the terminal's state (cell contents, tab stops and image state included); a number with a row replies 0; SM, RM or DECRQM do anything but expand the table; a mode's state is reachable outside its accessors, which vtium enforces with module privacy where the C reference needed a source check; a supported mode's row names no test that sets and resets it, or names a test that does not exist |
| The parser's bounds: rows in `vtium-parse`'s `src/bounds.rs`, from which its constants take their values | The bounds table in the crate's README | The committed table differs from the rows |
| The capability list: each probe `works` or `missing`, with the issue for each gap; a `works` probe may name the feature it needs | The probe verdicts | A `works` probe fails, or fails to fail without its feature; a `missing` probe passes; a probe is not in the list, or the list names one that does not exist |
| The terminfo entry | The XTGETTCAP table | The committed table differs from the entry |
| The departures list: each case where vtium follows a specification the C reference departed from, with its reason, the specification and vtium's expected output | The conformance runner's expected output | A case differs from the C reference's golden and is not listed, or a listed case matches the golden |

The coverage map lists each feature vtium builds, the tests that guard it and the paths they reach (font, renderer, display), and the issue for each gap. A gap is closed by a light test in the same change or filed.

Every directory of tests or data carries a README with a table of what each path is.

The mode table follows libtsm's `tsm-modes.def`, and the capability list and terminfo entry follow kmscon's, so vtium is judged by the conformance corpus and the probe verdicts the C reference passed.
