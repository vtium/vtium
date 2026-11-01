# VTium

vtium is a terminal for the Linux virtual console. It runs on a VT, one of the full-screen text consoles the kernel provides, takes the display through KMS/DRM (or through the older fbdev framebuffer where that is all a machine has), and draws the terminal itself in software, with no display server.

It is written in Rust. Its behaviour is judged against the C reference, the maintainer's forks of the C terminal `kmscon` and its library `libtsm`, which vtium learned from: tests compare vtium's screens and replies with the C reference's saved results, as text and pixel for pixel.

![vtium on a 640x480 display: a row of foreground colors fg31 to fg97, a row of background colors bg41 to bg107, the words bold, underline, reverse, bold-underline-red, faint and italic in their styles, the 256-color cube as six bands, a 24-bit ramp from green to red, and a shell prompt with a block cursor](tools/tier3/goldens/2d-640x480-psf16/sgr.png)

*vtium's own output: the minspec build running on a VT of a QEMU/KVM guest with 1 vCPU and 512 MB, on a virtio-vga display at 640x480, in vtium's 8x16 PSF test font, made from Unifont's glyphs (80 by 30 cells). Tier 3 captured it from the guest's screen. The maintainer reviewed and accepted it as a golden on 2026-10-28, and the run at this README's commit matched it to the pixel. The font has no italic face, so `italic` draws upright.*

![vtium on a 320x240 display: a ruler of 40 columns, a colored directory listing, the 16 background colors, six color names, bold, faint, underlined, reversed and struck words, a box of sprite lines around cpu, mem and load figures, a line of Latin-1, four underline styles, a progress bar at 50%, and a prompt](tools/tier3/goldens/2d-320x240-psf8/small-console.png)

*vtium's own output: the same build and guest on a 320x240 display, the smallest vtium supports, in vtium's 8x8 PSF test font, Unifont's glyphs folded to half height (40 by 30 cells). It is also a golden the maintainer accepted on 2026-10-28, matched to the pixel at this README's commit.*

## What it does today

### Displays

- vtium draws into DRM dumb buffers, plain memory buffers the display scans out, with atomic modesetting where the card offers it and the legacy calls where it does not. On fbdev it draws at 32, 24 or 16 bits, or at 1 bit for monochrome panels.
- It follows displays as they are plugged in and out, powers them down after an idle time, gives the display back at a VT switch and takes it again on return, and draws in four orientations.

### Heads: a terminal per display

One vtium process runs on one VT and drives every display it finds. A head is a terminal for one or more displays, with its own grid, its own font and its own consoles. A console is one session with its own login, screen and scrollback, and a head shows one of its consoles at a time.

With no configuration, vtium runs one head on every display, sized for the smallest. The head file, `/etc/vtium/heads.conf`, declares heads and an ordered list of rules. Each display, keyboard and pointer walks the rules from the top, as a packet walks a firewall's rules, and the first rule that matches puts it in a head, makes it roam (type into whichever head is active), ignores it, or switches it off. A rule names a device by its port, by the monitor its EDID names, by its DisplayPort multi-stream hub port, or by its name.

The file sets a policy for what its rules do not mention:

- Under `policy = open`, a keyboard no rule names roams. vtium also guards against lockout: a keyboard that reaches no lit head roams, and `Ctrl-Alt-F<n>` for vtium's own VT makes the keyboard that pressed it roam until the next boot.
- Under `policy = closed`, vtium ignores any input no rule names. Every head is isolated unless the file says otherwise: its keyboards' VT-switch keys reach its session as plain keys. When nothing in a closed file survives its errors, no keyboard types until the file is fixed from outside vtium. This is the setup a kiosk needs.

`vtium check-config FILE` checks a head file before it is installed, with the same parser and rules the start uses. [docs/heads.md](docs/heads.md) gives a complete head file for each setup: the default, two heads with their own keyboards, a kiosk, a kiosk with an operator's head, one person to each head, and everything off.

### Input

vtium reads keyboards and pointers directly from their devices, with libxkbcommon for the keymap. The pointer selects text, the middle button and Shift+Insert paste it, and a program can copy into the same selection with OSC 52. A program cannot read the selection back. Keys can be bound to reboot, power off or suspend, and vtium asks the system to do it, so services stop cleanly.

### Fonts

The console parses no font file. A frit (a font set file, `/etc/vtium/fonts/NAME.vtfrit`) lists a font's sources in order, and later sources fill the characters the earlier ones lack. A helper program, `vtium-vitrify serve`, reads the sources and rasterizes them at run time, and sends the console ready-to-draw glyphs for its cell size. The console checks each answer and keeps the glyphs in memory for the boot.

- When vtium runs as root, the helper runs as `nobody`. The console restarts a crashed helper at most 3 times a boot, and draws its built-in fonts meanwhile.
- The helper reads PSF and raw console fonts, kbd font names, BDF fonts, the kernel's own VT font, and outline fonts by family or file, color emoji included (COLR version 0, CBDT and sbix). It rasterizes outlines with swash and shapes each grapheme cluster with HarfRust.
- Unifont and Unscii-8 are built into the console. They draw the first frame and every character a frit lacks.
- `vtium-vitrify set` switches the running console's font through OSC 50, and Ctrl+= and Ctrl+- zoom it.

### The terminal

- 24-bit color, the 256-color palette and named palettes. Bold, faint, underline in five styles (single, double, curly, dotted, dashed) each in a color of its own, strike, blink and reverse.
- Unicode text in extended grapheme clusters, wide characters and emoji presentation, and the codepoint-width method when a program resets mode 2027.
- Mouse reporting (modes 9, 1000, 1002 and 1003, in the SGR, urxvt and pixel encodings), focus events, bracketed paste, synchronized output (mode 2026) and in-band resize reports (mode 2048).
- OSC 8 links: vtium shows a link's address and copies it on Ctrl+click, and never opens it. OSC 133 prompt marks, with keys that move the view from prompt to prompt.
- Colors set, queried and reset by OSC 4, 10, 11 and 12, with xterm's and kitty's color stacks. DEC rectangular area operations, character protection, XTSAVE and XTRESTORE, XTGETTCAP, XTVERSION and DECRQM.
- A reply to a query is built from vtium's own state. It never repeats bytes the program sent, so text a program prints cannot type into the program reading the replies.

[ADR-1000](docs/architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md) lists every mode and feature vtium supports, ignores or refuses, and [docs/modes.md](docs/modes.md), generated from the code, lists each mode with its test.

### Scrollback and reflow

Each console's scrollback is bounded in bytes, 5,963,776 by default, and all consoles together stay within a quarter of the machine's memory. When the width changes, vtium rewraps the main screen and its scrollback, and carries the cursor, the selection and the view with them ([ADR-1003](docs/architecture/term/ADR-1003-reflow-on-a-width-change-and-no-cell-past-the-margin.md)).

### Small machines

The default build, minspec, is the smallest complete console: the CPU renderer, bitmap fonts, the parser and screen, the pty and input. It is built for a single-board computer with ARM cores of the Cortex-A53 class or older, 512 MB of memory or less, and no GPU, and its smallest display is a 320x240 panel with 8x8 cells, 40 columns by 30 rows. `make arm` builds it for aarch64 and armv7.

Tier 3 measures it in a guest with 1 vCPU and 512 MB. At commit `cfeca3a`:

| Measure | Value |
|---|---|
| Console binary | 3,216,864 bytes, against a budget of 3,670,016 bytes (3.5 MiB) that fails the run past it |
| Install tree, the font helper included | 7.63 MiB |
| Peak resident memory at 320x240, 8x8 cells | 5.76 MiB |
| Peak resident memory at 640x480, 8x16 cells | 7.71 MiB |
| Peak resident memory at 1366x768, 8x16 cells | 12.11 MiB |

## What it does not do yet

- Pictures: sixel, kitty graphics and iTerm2 inline images. [ADR-1100](docs/architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md) decides the pipeline, and none of it is built.
- Glyph ink reaching into the next cell ([ADR-1302](docs/architecture/render/ADR-1302-glyph-ink-reaches-one-cell-past-its-own-side-to-side-and-is-cut-at-its-row.md), decided and not built), ligatures and shaping across cells, and the bold and italic faces of outline fonts.
- A GPU renderer.
- libseat. vtium runs as root for its whole life ([ADR-1400](docs/architecture/platform/ADR-1400-the-privilege-and-sandbox-model.md)).
- The kitty keyboard protocol, bidirectional text, and input methods.
- `vtiumctl`, the font builder and preview (`vtium-vitrify new` and `preview`), `vtium-clipboard`, and reloading the head file without a restart.
- A release or a distribution package.

The plan and its order are in [the tracker issue](https://github.com/vtium/vtium/issues/47).

## vtium beside the alternatives

These are the other ways to get a terminal on a Linux console without a display server. Each line below is taken from the project's own documentation, repository or release notes, at the version named.

| | Draws through | Colors | Fonts | Pictures | Latest release |
|---|---|---|---|---|---|
| The kernel's VT console (fbcon) | the kernel's framebuffer device | 256-color and 24-bit requests are "shoehorned into" 16 foreground and 8 background colors | console bitmap fonts, at most 512 glyphs at once; with more than 256, only 8 of the 16 colors | none in `console_codes(4)` | ships with the kernel |
| `kmscon/kmscon` 10.1.0, with `kmscon/libtsm` 4.8.0 | KMS/DRM, in software or through OpenGL ES 2, or fbdev | 24-bit | FreeType, Pango, PSF, and a built-in 8x16 font and Unifont | sixel is an open feature request | 10.1.0, 2026-10-26 |
| fbterm 1.7 | fbdev, or a VESA video card | 256, through its own private sequences under `TERM=fbterm` | fontconfig and FreeType | none listed | 1.7, 2010-10-06 |
| yaft 0.2.9 | fbdev at 8 to 32 bits per pixel, on Linux and FreeBSD, and on NetBSD and OpenBSD as experimental | 256 | BDF fonts converted into its source when it is built | sixel and DRCS, marked experimental | tag v0.2.9; last commit 2018-11-14 |
| fbpad | Linux fbdev | 256 | its own tinyfont format and PSF2 | none listed | last commit 2026-10-23 |
| vtium | KMS/DRM dumb buffers, or fbdev | 24-bit | PSF, kbd, BDF, outline and color fonts, drawn by a helper that never runs as root; built-in Unifont and Unscii-8 | decided, not built | none yet |

Besides what the table shows, such as `kmscon`'s OpenGL ES 2 renderer and yaft's sixel, the others do these things vtium does not:

- The kernel console needs no program in user space: fbcon takes over the console by itself.
- `kmscon` supports libseat when built with it, sets each monitor's mode and rotation separately, and follows the keyboard layout set through `localectl`.
- fbterm has an input method framework for Chinese, Japanese and Korean input, and up to 10 windows.
- fbpad keeps two terminals to a tag and splits the screen between them.

On vtium's side are the heads, with their first-match rules and the closed policy for kiosks; fonts parsed and rasterized in a helper process apart from the console; scrollback bounded in bytes and rewrapped on a width change, where the kernel console's soft scrollback (Shift+PageUp) was removed in Linux 5.9; OSC 8 links and OSC 133 prompt marks; monochrome panels; and the tests below.

## How it is judged

- **Unit tests** in each crate.
- **Tier 1, conformance:** byte streams replayed through vtium's terminal, the screen and the replies compared as text with goldens, the saved results the C reference produced. The goldens were copied into vtium's tree, each with the file and commit it came from.
- **Capability probes:** each capability vtium claims has a replay that fails without it.
- **Tier 2, pixels:** the renderer draws known screens in memory, compared pixel for pixel with goldens.
- **Tier 3, a virtual machine:** vtium's install tree boots on a VT of a QEMU/KVM guest with a real KMS device, keys are typed into it, and its screens are compared pixel for pixel with goldens. The run also records the size and memory figures above. At `cfeca3a` the minspec run gave 47 passes, 28 listed departures and 16 provisional goldens, with no failure.
- **The VT review:** the maintainer checks a build by eye on a real VT, guided step by step ([docs/vt-review.md](docs/vt-review.md)).

Where vtium differs from a golden, the difference is a listed departure or the test fails. Where the C reference departed from a protocol's specification, vtium follows the specification and lists the case. Goldens vtium made for screens the C reference never drew stay provisional, and judge nothing, until the maintainer reviews them.

The state vtium shows follows only from the bytes a program sends: the same stream gives the same screen and the same replies however it is split, which a property test checks. The parser, terminal and renderer deny every panic a program's input could reach, and only the platform crate may use `unsafe`. [docs/architecture.md](docs/architecture.md) lists each of these rules with the check that fails when it breaks.

## Trying it

Building needs Rust and clang, and the first build fetches a Debian sysroot with glibc 2.36 to link against ([tools/arm/README.md](tools/arm/README.md)); `make install-tree` also needs `tic`. On a machine with systemd and logind, `make vt-shell` builds the minspec binary, picks a free VT, asks for sudo, and runs your own shell in vtium there. Exit the shell to end the run, and the VT's own service starts again. [docs/vt-review.md](docs/vt-review.md) describes this and the other VT runs.

`make install-tree` stages what a system installs under `target/install-tree`: the console, `vtium-vitrify`, the VT unit `kmsconvt@.service` that takes getty's place, its getty fallback, the terminfo entry and the example configuration. vtium reads the C reference's configuration file, `/etc/kmscon/kmscon.conf`, with its keys, and adds its own ([docs/config.md](docs/config.md)).

[docs/concepts.md](docs/concepts.md) introduces vtium's pieces and how they fit together, and [AGENTS.md](AGENTS.md) gives every gate and test command.

## Provenance

vtium carries forward the work of the [aaronsb/kmscon](https://github.com/aaronsb/kmscon) and [aaronsb/libtsm](https://github.com/aaronsb/libtsm) forks of `kmscon/kmscon` and `kmscon/libtsm`, by David Herrmann and their contributors ([COPYING](COPYING)). The forks are the C reference. vtium is a separate project, and its code is new Rust code.

The maintainer's code in vtium is written by large language models, under the maintainer's review, and judged by the tests above. Each decision is recorded with the maintainer's words in the decision records under [docs/architecture/](docs/architecture/).

Contributions are welcome, written by hand or by a large language model. Each pull request says how its code was written. Nothing is rejected for how it was written: every change is judged by the same tests and the same review. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

MIT, as `kmscon` and `libtsm` are: see [LICENSE](LICENSE). [COPYING](COPYING) keeps the author lists of `kmscon` and `libtsm` and their original copyright notices.
