# Conventions

These rules hold for the interface of every crate under `crates/`. Each crate's README says how the crate applies them, and its reference documents each item.

## What is stable

Any interface may change in any release. vtium has not yet decided which interfaces and file formats it promises to keep. [The issue on vtium's promises](https://github.com/vtium/vtium/issues/343) tracks that decision. Until it is made, a program that builds on a crate pins the commit it builds against.

## Bounded memory and bounded input

Every resource a program's output can grow has a bound. Each crate defines its bounds once, in `src/bounds.rs`, and its README has a table of them. The table says what happens to input past each bound. For example, vtium-parse keeps at most `OSC_MAX` bytes of an OSC string, and vtium-term drops a string that was cut at that bound.

A host can therefore feed a crate any amount of output from a program. The crate's memory stays inside its bounds, and the state past a bound is the one the bound's row describes.

## No panic on any input

Nothing a program sends can make vtium-parse, vtium-term, vtium-image or vtium-render panic. Their `lib.rs` denies the lints that find a panic a program could reach: unchecked arithmetic, lossy casts, unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`. Where a crate must still allow one, the code there carries `#[expect(..., reason = "...")]` with its reason. Every crate forbids `unsafe` code, apart from vtium-platform and vtium-image-helper, which hold vtium's unsafe code.

vtium's release builds set `panic = "abort"`, so a panic ends the console.

## How errors are returned

A function that can fail returns a `Result` with an error type of its crate. For example, `vtium_font::PsfError` describes a font file that does not read, and `vtium_settings::Error` a line of vtium.conf that vtium cannot use. An error type says what went wrong in words a person can read, and `Display` prints it.

The bytes a program writes are not an error. `Terminal::input` takes any byte stream and returns how many bytes it took. A sequence the terminal does not support, or one that breaks the grammar of escape sequences, is handled the way the specifications say, which is mostly to ignore it. [ADR-1000](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md) lists what vtium supports, ignores and refuses.

## Which cargo feature turns on what

The default build of each crate is the minspec build's, the smallest console. A feature adds one part on top of it.

| Feature | Crates | What it adds |
|---|---|---|
| `images` | vtium, vtium-term, vtium-render, vtium-platform | Inline images: the image store, the protocol handlers, and the image helper's channel. |
| `bidi` | vtium, vtium-term, vtium-render | Right-to-left text, laid out by the Unicode bidirectional algorithm. |
| `gpu` | vtium, vtium-render, vtium-platform | The GPU renderer, through OpenGL ES 3.0 on the display's render node. |
| `logind` | vtium, vtium-platform | Talking to systemd-logind through sd-bus, which the power keys use to suspend the machine. |
| `libseat` | vtium, vtium-platform | The seat's display devices and VT switching from the seat manager, seatd or logind, through libseat. |
| `psf`, `gzip` | vtium-font | Reading PSF fonts, and gzip-compressed ones. |
| `unifont`, `unscii` | vtium-font | The fonts built into the console, on by default. |
| `deflate` | vtium-font | The compression the glyph set writer uses. vtium-config turns it on, and the console leaves it out. |
| `performance-cache` | vtium-render | The glyph cache starts in its performance profile. |
| `kitty-check` | vtium-config | vtium-config's tests read images through the console's terminal. No shipped build turns it on. |

The reference on this site is built with every feature, so it documents the items each feature adds.
