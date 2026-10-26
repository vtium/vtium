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

vtium is a port of the [aaronsb/kmscon](https://github.com/aaronsb/kmscon) fork's components. The fork's code is the reference for behaviour, not for layout.

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
- **Capability probes:** each capability vtium claims has a probe that fails when the capability is missing, and the probes stay in step with the capability list.
- **Pixel tests (tier 2):** the renderer draws known screens, compared pixel for pixel with goldens.
- **VM runs (tier 3):** a VM boots vtium on a VT, drives keys and compares screenshots pixel for pixel, including a minspec variant on the small-machine standard.

Work is built in bundles of items that don't collide, then evaluated once per bundle.

## Tables, references and maps

A fact vtium states in more than one place has one source, and the other places are generated from it or checked against it by a test.

| Source | What is generated or checked | The test fails when |
|---|---|---|
| The mode table in `vtium-term`: one row per ANSI and DEC private mode, with kind, number, name, status (supported, partial, ignored, refused), default, reset, host setting, terminfo, spec and tests | The mode dispatch, DECRQM replies, resets and accessors; `docs/modes.md` and `docs/modes.tsv` | The committed reference differs from the table; a number with no row replies anything but 0 or changes state; SM, RM or DECRQM bypass the table |
| The parser's bounds, as named constants in `vtium-parse` | The bounds table in the crate's reference | The committed table differs from the constants |
| The capability list: each probe `works` or `missing`, with the issue for each gap | The probe verdicts | A `works` probe fails, or a `missing` probe passes |
| The terminfo entry | The XTGETTCAP replies | The entry and the replies disagree |

The coverage map lists each feature vtium builds, the tests that guard it, and the issue for each gap. A change that adds a feature adds its row.

Every directory of tests or data carries a README with a table of what each path is. The mode table, the capability list and the terminfo entry follow the C reference's, so the replay probes judge both.
