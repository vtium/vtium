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

vtium is a port of the [aaronsb/kmscon](https://github.com/aaronsb/kmscon) fork's components. The fork's code is the floor for behaviour: vtium does everything it does, and more. It is no guide to layout.

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

vtium covers everything the C reference does, judged by its goldens and probes, and goes further. The capabilities the C reference lacked are vtium issues, listed in the tracker.

Where the C reference departs from a protocol specification, vtium follows the specification. Each departure is listed with its reason and the specification it follows, and the conformance runner expects vtium's own output for exactly the listed cases, so no departure is silent. A security defect is fixed in the C reference too, so the goldens agree.

A bound vtium takes from the C reference is a starting value. vtium may set its own, with the reason in the row that defines it.

## Tables, references and maps

A fact vtium states in more than one place has one source. Where a test can be written, the other places are generated from the source or checked against it, and the test fails on drift. The coverage map and the path tables are kept by hand, in the change that alters what they describe.

| Source | What is generated or checked | The test fails when |
|---|---|---|
| The mode table in `vtium-term`: one row per ANSI or DEC private mode vtium recognises, with kind, number, name, status (supported, partial, ignored, refused), default (fixed or a host setting), reset, what the host must do, terminfo, spec and tests | The calls into each mode's accessors from the dispatch, the DECRQM replies and the resets; `docs/modes.md` and `docs/modes.tsv`, describing the build with `images` | The committed `docs/modes.md` or `.tsv` differs from the table; a number with no row (DEC 0-65535, ANSI 0-255) gets a DECRQM reply other than 0, or SM and RM of it change the terminal's state; a number with a row replies 0; SM, RM or DECRQM do anything but expand the table, or a mode's state is touched outside its accessors; a supported mode's row names no test that sets and resets it, or names a test that does not exist |
| The parser's bounds: rows in `vtium-parse`'s `src/bounds.rs`, from which its constants take their values | The bounds table in the crate's README | The committed table differs from the rows |
| The capability list: each probe `works` or `missing`, with the issue for each gap; a `works` probe may name the feature it needs | The probe verdicts | A `works` probe fails, or fails to fail without its feature; a `missing` probe passes; a probe is not in the list, or the list names one that does not exist |
| The terminfo entry | The XTGETTCAP table | The committed table differs from the entry |

The coverage map lists each feature vtium builds, the tests that guard it and the paths they reach (font, renderer, display), and the issue for each gap. A gap is closed by a light test in the same change or filed.

Every directory of tests or data carries a README with a table of what each path is.

The mode table follows libtsm's `tsm-modes.def`, and the capability list and terminfo entry follow kmscon's, so one conformance corpus and one set of probe verdicts are checked against vtium and the C reference alike.
