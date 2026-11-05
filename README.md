# VTium

vtium is a terminal for the Linux virtual console: it runs on a VT, one of the full-screen text screens the kernel keeps beside a desktop, and draws the terminal there itself, with no display server.

The kernel's own console turns 256-color and 24-bit color into 16 colors, holds at most 512 characters of one bitmap font at a time, and shows no pictures. vtium takes the display through KMS/DRM, or through the older fbdev framebuffer where that is all a machine has, and gives the programs on the console what a desktop terminal gives them: 24-bit color, Unicode with Chinese, Japanese and Korean text and color emoji, outline fonts, mouse reporting, scrollback that rewraps when the width changes, and pictures. It is written in Rust, needs no GPU and does not rely on systemd, and its smallest build runs on a single-board computer with 512 MB of memory and a 320x240 panel.

**Documentation:** [vtium.github.io/vtium](https://vtium.github.io/vtium/)

![A console screen showing a shell session in a smooth monospaced font: prompts in blue, gray and green segments, a colored listing of the vtium source tree, a git log with a branch graph and yellow commit hashes, the crates of vtium, a passing test run, and three lines of greetings and thanks in English, Chinese, Japanese, Korean, Thai, Greek, Russian, Vietnamese, French and German with emoji.](docs/showcase/hero.png)

*vtium's own drawing of a shell session on a 1920x1080 display. [The showcase](docs/showcase/README.md) says what this screen shows, and has more.*

![vtium on a 640x480 display: the labels fg30 to fg97 in their own colors and bg40 to bg107 on their own backgrounds, each set wrapped over two rows, with fg30 and bg40 drawn black on black; the words bold, underline, reverse, bold-underline-red, faint and italic in their styles, the 256-color cube as six bands, a 24-bit ramp from green to red, and a shell prompt with a block cursor](docs/showcase/small/sgr.png)

*vtium's own output: the minspec build running on a VT of a QEMU/KVM guest with 1 vCPU and 512 MB, on a virtio-vga display at 640x480, in vtium's 8x16 PSF test font, made from Unifont's glyphs (80 by 30 cells). Tier 3 captured it from the guest's screen. The maintainer reviewed and accepted it as a golden on 2026-10-28, and a run at commit `c107f9a` matched it to the pixel. The font has no italic face, so `italic` draws upright.*

![vtium on a 320x240 display: a ruler of 40 columns, a colored directory listing, the 16 background colors, six color names, bold, faint, underlined, reversed and struck words, a box of sprite lines around cpu, mem and load figures, a line of Latin-1, four underline styles, a progress bar at 50%, and a prompt](docs/showcase/small/small-console.png)

*vtium's own output: the same build and guest on a 320x240 display, the smallest vtium supports, in vtium's 8x8 PSF test font, Unifont's glyphs folded to half height (40 by 30 cells). It is also a golden the maintainer accepted on 2026-10-28, matched to the pixel at `c107f9a`.*

## Status

vtium's latest release is the pre-release v0.1.0-alpha.4. It draws on the CPU. A build from a checkout with the `gpu` feature draws on the GPU where `hwaccel` is on, and seat access through libseat and more are landing; [What vtium does](docs/features.md#status) lists what is built and what is not yet.

## Try it

[Try vtium on one VT](docs/tutorial-try-vtium.md) installs vtium from the AUR on Arch Linux, gives it one spare VT, and takes it off again, and your desktop keeps running throughout. On another distribution, it runs vtium from a checkout of this repository with `make vt-shell` and installs nothing.

## Documentation

| If you want to | Read |
|---|---|
| learn by doing | the tutorial [Try vtium on one VT](docs/tutorial-try-vtium.md) |
| get one task done | the how-to guides: [Install vtium and give it VTs](docs/install.md), [Configure vtium](docs/configure.md), [Choose a font](docs/fonts.md), [Set up heads](docs/heads.md), [Find out what went wrong](docs/troubleshooting.md) |
| look something up | the reference: [What vtium does](docs/features.md), [Keys](docs/keys.md), [Configuration keys](docs/config.md), [The head file](docs/head-file.md), [Modes](docs/modes.md), [systemd, logind and PID 1](docs/systemd.md) |
| understand why and how | the explanation: [Why vtium](docs/why-vtium.md), [Concepts](docs/concepts/index.md) |
| know why a decision was made | [the decision records](docs/architecture/INDEX.md) |
| contribute | [Architecture](docs/architecture.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [AGENTS.md](AGENTS.md), which gives every gate and test command |

## How it is judged

vtium's behaviour is judged by its tests, which compare vtium's screens and replies with saved expected results, its goldens, as text and pixel for pixel, from unit tests up to a virtual machine that boots vtium on a real VT. [How vtium is judged](docs/concepts/how-vtium-is-judged.md) explains the tiers, and [Architecture](docs/architecture.md#how-it-is-judged) gives the detail.

## Provenance

vtium began as a Rust program derived from `kmscon`, a C terminal for the Linux console, and `libtsm`, its terminal library, as [COPYING](COPYING) says. `kmscon` was written by David Herrmann and the contributors COPYING lists, and `libtsm` by Aetf, David Herrmann and Ran Benita. vtium's console and libraries are Rust written for vtium.

The maintainer's code in vtium is written by large language models, under the maintainer's review, and judged by vtium's tests. Each decision is recorded with the maintainer's words in [the decision records](docs/architecture/).

Contributions are welcome, written by hand or by a large language model. Each pull request says how its code was written. Nothing is rejected for how it was written: every change is judged by the same tests and the same review. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

vtium is under the MIT licence ([LICENSE](LICENSE)). The Unifont glyphs built into the console are under the SIL Open Font License 1.1, and Unscii-8 is in the public domain ([COPYING](COPYING), [crates/vtium-font/README.md](crates/vtium-font/README.md)). COPYING also keeps the author lists of `kmscon` and `libtsm` and their copyright notices.
