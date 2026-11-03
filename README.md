# VTium

vtium is a terminal for the Linux virtual console. It runs on a VT, one of the full-screen text consoles the kernel provides, takes the display through KMS/DRM (or through the older fbdev framebuffer where that is all a machine has), and draws the terminal itself, with text, color and pictures. It draws on the CPU today, and a GPU renderer is to come ([Status](#status)). It needs no display server and does not rely on systemd.

It is written in Rust. Its behaviour is judged against the C reference, the maintainer's forks of the C terminal `kmscon` and its library `libtsm`: tests compare vtium's screens and replies with the C reference's saved results, as text and pixel for pixel.

If you are new to vtium, [docs/why-vtium.md](docs/why-vtium.md) says why you might choose it over the kernel's console, `kmscon` or a desktop terminal, and why its font tools take their names from glass; [docs/concepts.md](docs/concepts.md) introduces vtium's pieces and how they fit together. [docs/managing-vtium.md](docs/managing-vtium.md) gives the steps of running vtium day to day, from installing it and giving it VTs to choosing its font and finding out what went wrong.

![vtium on a 640x480 display: the labels fg30 to fg97 in their own colors and bg40 to bg107 on their own backgrounds, each set wrapped over two rows, with fg30 and bg40 drawn black on black; the words bold, underline, reverse, bold-underline-red, faint and italic in their styles, the 256-color cube as six bands, a 24-bit ramp from green to red, and a shell prompt with a block cursor](tools/tier3/goldens/2d-640x480-psf16/sgr.png)

*vtium's own output: the minspec build running on a VT of a QEMU/KVM guest with 1 vCPU and 512 MB, on a virtio-vga display at 640x480, in vtium's 8x16 PSF test font, made from Unifont's glyphs (80 by 30 cells). Tier 3 captured it from the guest's screen. The maintainer reviewed and accepted it as a golden on 2026-10-28, and a run at commit `c107f9a` matched it to the pixel. The font has no italic face, so `italic` draws upright.*

![vtium on a 320x240 display: a ruler of 40 columns, a colored directory listing, the 16 background colors, six color names, bold, faint, underlined, reversed and struck words, a box of sprite lines around cpu, mem and load figures, a line of Latin-1, four underline styles, a progress bar at 50%, and a prompt](tools/tier3/goldens/2d-320x240-psf8/small-console.png)

*vtium's own output: the same build and guest on a 320x240 display, the smallest vtium supports, in vtium's 8x8 PSF test font, Unifont's glyphs folded to half height (40 by 30 cells). It is also a golden the maintainer accepted on 2026-10-28, matched to the pixel at `c107f9a`.*

## Status

vtium's first release is the pre-release v0.1.0-alpha.1. A checkout builds and runs everything on this page except these, which are designed and landing:

- **Pictures from files and shared memory, and iTerm2's inline images.** The image helper, the separate program that is to decode files, shared memory, PNG and zlib as the session's user and never as root ([ADR-1101](docs/architecture/image/ADR-1101-the-image-helper-is-its-own-sandboxed-program-run-as-the-session-s-user.md)), is not built yet, so vtium answers each such request as it would with no helper running. iTerm2's inline images have no handler yet; the design gives them one that decodes each picture on the kitty path and places it under sixel's rules ([ADR-1100](docs/architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md)).
- **The GPU renderer,** on glow over EGL and GBM ([the GPU path's issue](https://github.com/vtium/vtium/issues/33)). Today every build draws on the CPU.
- **libseat,** for seat access through seatd or logind ([the libseat issue](https://github.com/vtium/vtium/issues/34)). Today vtium opens the VT, the cards and the input devices itself, as root.
- **Handing the display back without a VT switch,** through the `setBackground` sequence ([the hand-back issue](https://github.com/vtium/vtium/issues/36)).
- **Showing a link's address in the console:** Ctrl+click copies an OSC 8 link's address, Ctrl+Shift+Up and Ctrl+Shift+Down move the view between the prompts OSC 133 marks, and the link under the pointer draws a dashed underline. The renderer does not yet draw the overlay row that shows the link's address while Ctrl is held ([ADR-1002](docs/architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md)).

Also not built yet: glyph ink reaching into the next cell ([ADR-1302](docs/architecture/render/ADR-1302-glyph-ink-reaches-one-cell-past-its-own-side-to-side-and-is-cut-at-its-row.md), decided), ligatures and shaping across cells, the bold and italic faces of outline fonts, the kitty keyboard protocol, bidirectional text, input methods, `vtiumctl`, the font builder and preview (`vtium-config new` and `preview`), `vtium-clipboard`, and reloading the head file without a restart. The plan and its order are in [the tracker issue](https://github.com/vtium/vtium/issues/47).

## What it does

### Displays

- vtium draws into DRM dumb buffers, plain memory buffers the display scans out, with atomic modesetting where the card offers it and the legacy calls where it does not. On fbdev it draws at 32, 24 or 16 bits, or at 1 bit for monochrome panels.
- It follows displays as they are plugged in and out, powers them down after an idle time, and draws in four orientations.
- The CPU renderer is the default build's renderer and draws everywhere. The GPU renderer, behind a build feature, draws the CPU renderer's pixels ([ADR-1301](docs/architecture/render/ADR-1301-one-glyph-engine-draws-at-run-time-for-the-display-s-capabilities.md)): tier 2 draws each screen on both, the GPU path under llvmpipe, and requires the same bytes, except for pictures scaled with filtering ([ADR-1300](docs/architecture/render/ADR-1300-the-drawn-glyph-cache-and-frame-composition.md)). Landing; see [Status](#status).

### Sharing the display

vtium shares the graphics card with whatever else draws on the machine. vtium runs on one VT, and the program on another VT may be a Wayland compositor, an X server, a getty or another vtium ([docs/concepts.md](docs/concepts.md)). Only one program at a time is the card's DRM master, the one allowed to set modes, and master can be dropped and taken again ([kernel DRM documentation](https://docs.kernel.org/gpu/drm-uapi.html)). vtium hands the card over the same way whatever is on the other VT:

- vtium takes its VT's switching from the kernel (`VT_PROCESS`). When the kernel asks for a switch away, vtium blanks its buffers, waits for a frame still on its way to the display (at most 1 second, `FLIP_WAIT_MS`), hides its cursor and overlay planes and drops DRM master, and only then lets the kernel switch.
- When its VT comes back, vtium takes DRM master again and sets every display. While the other program still holds master, vtium tries again every 50 ms, 40 times, and then on the next key press, pointer action or hotplug.
- It presents a frame with an atomic commit that carries the frame's damage as `FB_DAMAGE_CLIPS` (the kernel's "optional plane property to mark damaged regions", [KMS documentation](https://docs.kernel.org/gpu/drm-kms.html)) where the card offers it, with the legacy calls and whole frames where it does not, and through fbdev where there is no DRM.
- A display the head file leaves dark is turned off at each modeset, so a frame another master left on it goes dark. On a card with only the legacy calls, such a display still shows that frame.

With systemd, vtium runs on a VT in a getty's place, `pam_systemd` registers the login with logind, and a fallback unit starts a getty if vtium fails, unless the head file's policy is closed. The package, and `make install`, install the unit as `vtium@.service` with its fallback `vtium-getty@.service`, and give vtium no VT until `vtium console enable N` gives it VT N or `vtium console enable --all` gives it every VT; logind then starts vtium on those VTs where it would start a getty. `make install-tree`, the tree tier 3 installs in its guest, installs the same two units, and tier 3 gives vtium the guest's VTs with `vtium console enable --all`, as a user does. Without systemd, another init runs `vtium --vt=ttyN --no-switchvt` from an inittab line, a script or a supervisor where it ran a getty, or a person starts it by hand, and elogind, where it runs, registers the login in logind's place. Of the other inits only sysvinit is tested; OpenRC, runit, s6, busybox init and elogind are not. busybox init does not catch `SIGRTMIN+4`, so the power-off key powers nothing off there. Tier 3 boots vtium under sysvinit on Debian 13, with no logind, from an inittab line, with the udev daemon running (`sysvinit-640x480`) and with it stopped (`sysvinit-no-udev`), and its reboot and power-off keys go through sysvinit. The screens it draws there match vtium's own goldens to the pixel, and those goldens still wait for the maintainer's review. [docs/systemd.md](docs/systemd.md) lists each place vtium touches systemd, logind or PID 1, and what it does without them.

With libseat, vtium takes the VT, the cards and the input devices from the seat manager, seatd or logind, whichever is running. libseat is the seat management library "allowing applications to use whatever seat management is available" ([seatd's README](https://git.sr.ht/~kennylevinsen/seatd/blob/master/README.md)). The process still runs as root to start `/bin/login` ([ADR-1400](docs/architecture/platform/ADR-1400-the-privilege-and-sandbox-model.md)). Landing; see [Status](#status).

### Heads: a terminal per display

One vtium process runs on one VT and drives every display it finds. A head is a terminal for one or more displays, with its own grid, its own font and its own consoles. A console is one session with its own login, screen and scrollback, and a head shows one of its consoles at a time.

With no configuration, vtium runs one head on every display, sized for the smallest. The head file, `/etc/vtium/heads.conf`, declares heads and an ordered list of rules. Each display, keyboard and pointer walks the rules from the top, as a packet walks a firewall's rules, and the first rule that matches puts it in a head, makes it roam (type into whichever head is active), ignores it, or switches it off. A rule names a device by its port, by the monitor its EDID names, by its DisplayPort multi-stream hub port, or by its name.

The file sets a policy for what its rules do not mention:

- Under `policy = open`, a keyboard no rule names roams. vtium also guards against lockout: while no keyboard reaches a lit head, every keyboard roams, with a notice, until one does; and `Ctrl-Alt-F<n>` for vtium's own VT makes the keyboard that pressed it roam until the next boot.
- Under `policy = closed`, vtium ignores any input no rule names. Every head is isolated unless the file says otherwise: its keyboards' VT-switch keys reach its session as plain keys. When nothing in a closed file survives its errors, no keyboard types until the file is fixed from outside vtium. This is the setup a kiosk needs.

`vtium check-config FILE` checks a head file before it is installed, with the same parser and rules the start uses. [docs/heads.md](docs/heads.md) gives a complete head file for each setup: the default, two heads with their own keyboards, a kiosk, a kiosk with an operator's head, one person to each head, and everything off.

### Input

vtium reads keyboards and pointers directly from their devices, with libxkbcommon for the keymap. The pointer selects text, the middle button and Shift+Insert paste it, and a program can copy into the same selection with OSC 52. A program cannot read the selection back. Keys can be bound to reboot, power off or suspend ([docs/systemd.md](docs/systemd.md)). The reboot key sends PID 1 the kernel's Ctrl+Alt+Del signal, so systemd or sysvinit stops the services before the restart; where the kernel's `ctrl-alt-del` setting is not 0, vtium syncs the disks and restarts the machine at once, with no shutdown. The power-off key sends PID 1 `SIGRTMIN+4`, which systemd and sysvinit take as a clean power-off, and vtium warns at start when PID 1 does not catch that signal. The suspend key asks logind to suspend in a build with the `logind` feature, and otherwise writes `mem` to `/sys/power/state`; a suspend stops no service.

### Fonts

The console parses no font file. A frit (a font set file, `/etc/vtium/fonts/NAME.vtfrit`) lists a font's sources in order, and later sources fill the characters the earlier ones lack. A helper program, `vtium-config serve`, reads the sources and rasterizes them at run time, and sends the console ready-to-draw glyphs for its cell size. The console checks each answer and keeps the glyphs in memory for the boot.

- When vtium runs as root, the helper runs as `nobody`. The console restarts a crashed helper at most 3 times a boot, and draws its built-in fonts meanwhile.
- The helper reads PSF and raw console fonts, kbd font names, BDF fonts, the kernel's own VT font, and outline fonts by family or file, color emoji included (COLR version 0, CBDT and sbix). It rasterizes outlines with swash and shapes each grapheme cluster with HarfRust.
- Unifont and Unscii-8 are built into the console. They draw the first frame and every character a frit lacks.
- `vtium config set` switches the running console's font through OSC 50, and Ctrl+= and Ctrl+- zoom it.

### Pictures

vtium draws pictures today in a build with the `images` feature, which minspec leaves out. It decodes the pictures programs send with sixel, and those they send as pixels with the kitty graphics protocol, keeps each console's pictures and their placements in one store, bounded in memory, and draws them with the CPU compositor ([ADR-1100](docs/architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md)). A placement is anchored to the line holding its top row, so it moves with the text, and each protocol's own rules decide what erases it and how it scrolls. [Status](#status) says what is still to come.

### The terminal

- vtium draws 24-bit color, the 256-color palette and named palettes. It draws bold, faint, strike, blink and reverse, and underline in five styles (single, double, curly, dotted, dashed), each in a color of its own.
- It cuts Unicode text into extended grapheme clusters, draws wide characters and emoji presentation, and uses the codepoint-width method when a program resets mode 2027.
- It reports the mouse (modes 9, 1000, 1002 and 1003, in the SGR, urxvt and pixel encodings), focus, bracketed pastes and in-band resizes (mode 2048), and holds frames for synchronized output (mode 2026).
- It keeps OSC 8 links and OSC 133 prompt marks. Ctrl+click copies a link's address, never opening it, and Ctrl+Shift+Up and Ctrl+Shift+Down move the view from prompt to prompt.
- Programs set, query and reset colors with OSC 4, 10, 11 and 12, and with xterm's and kitty's color stacks. vtium carries out DEC's rectangular area operations, character protection, XTSAVE and XTRESTORE, and answers XTGETTCAP, XTVERSION and DECRQM.
- A reply to a query is built from vtium's own state. It never repeats bytes the program sent, so text a program prints cannot type into the program reading the replies.

[ADR-1000](docs/architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md) lists every mode and feature vtium supports, ignores or refuses, and [docs/modes.md](docs/modes.md), generated from the code, lists each mode with its test.

### Scrollback and reflow

Each console's scrollback is bounded in bytes. By default the bound keeps minspec's grids inside their 16 MiB peak: it is what that peak leaves after the console's two screens, more on a small display and 5 MiB at 170 by 48, and a larger display keeps the 5 MiB, so `sb-memory` raises it on a machine that can spare more. All consoles together stay within a quarter of the machine's memory. When the width changes, vtium rewraps the main screen and its scrollback, and carries the cursor, the selection and the view with them ([ADR-1003](docs/architecture/term/ADR-1003-reflow-on-a-width-change-and-no-cell-past-the-margin.md)).

### Small machines

The default build, minspec, is the smallest complete console: the CPU renderer, bitmap fonts, the parser and screen, the pty and input. It is built for a single-board computer with ARM cores of the Cortex-A53 class or older, 512 MB of memory or less, and no GPU, and its smallest display is a 320x240 panel with 8x8 cells, 40 columns by 30 rows. `make arm` builds it for aarch64 and armv7.

Tier 3 measures it in a guest with 1 vCPU and 512 MB. These figures come from `make tier3-minimal` at commit `c107f9a`, run on 2026-11-01:

| Measure | Value |
|---|---|
| Console binary | 3,216,864 bytes, against a budget of 3,670,016 bytes (3.5 MiB) that fails the run past it |
| Install tree, the font helper included | 7.63 MiB |
| Peak resident memory at 320x240, 8x8 cells | 5.81 MiB |
| Peak resident memory at 640x480, 8x16 cells | 7.77 MiB |
| Peak resident memory at 1366x768, 8x16 cells | 12.07 MiB |

## How vtium is built against other work

**The C reference.** vtium is measured against the maintainer's forks of `kmscon/kmscon` and `kmscon/libtsm`, [aaronsb/kmscon](https://github.com/aaronsb/kmscon) and [aaronsb/libtsm](https://github.com/aaronsb/libtsm). Both are MIT-licensed, with third-party parts their COPYING files list, among them: each has a hash table from CCAN under the LGPL 2.1, kmscon's Unifont backend is under the SIL OFL 1.1, and libtsm's Unicode tables are under the Unicode License v3. vtium took their logic flow and none of their code structure: each answer about behaviour starts from how the C reference handled the case, and vtium's types, ownership and modules follow Rust ([AGENTS.md](AGENTS.md)). Where the C reference departed from a protocol's specification, vtium follows the specification and lists the case as a departure. vtium copied the C reference's expected outputs, its goldens, into its own tree, and records where each file came from, with its repository, commit, source path and SHA-256 (`tools/conformance/provenance.tsv`, `tools/tier2/provenance.tsv`, `tools/tier3/provenance.tsv`). No build, test or CI job reads a checkout of the C reference. Tier 3's harness, written in Python, came from the C fork, lives and changes in vtium's tree, and runs on every tier 3 run; 47 of its files, the screen scripts and pictures, the configurations, the guest files and `t3img.py`, stay byte-identical copies of the fork's ([tools/tier3/README.md](tools/tier3/README.md)).

**Other terminals.** ADR-1000's tables hold, for every mode and feature, a column for each of the C reference, xterm, foot, Ghostty, WezTerm and kitty, filled where that terminal's behaviour was verified. vtium reads the code of permissively licensed implementations and names the source of anything it adapts. For a GPL-licensed terminal it reads only documentation and specifications, and may run the program to compare its screens. [CONTRIBUTING.md](CONTRIBUTING.md) states the rule for kitty: "Implement from published protocol specifications and from permissively licensed implementations such as Ghostty, WezTerm, foot and libsixel. kitty is GPL-3.0, and its source is not read while working on vtium, by a person or by an agent. kitty as a running program may be used for black-box comparison." The rule comes from the C reference's own record, [ADR-300 in the kmscon fork](https://github.com/aaronsb/kmscon/blob/main/docs/architecture/fork/ADR-300-write-from-protocol-specs-and-permissive-sources-never-from-kitty-s-code.md), which says "No source from a GPL-licensed terminal, kitty above all, is read while writing it, by a person or by an agent", so that the work stays MIT and anyone can use it. <!-- adr-cite-ignore: the kmscon fork's record, not one of vtium's -->

## vtium beside the alternatives

These are the other ways to get a terminal on a Linux console without a display server. Each row is taken from the sources listed under the table. vtium's row includes what is landing ([Status](#status)).

| | Draws through | Colors | Fonts | Pictures | Latest release |
|---|---|---|---|---|---|
| The kernel's VT console (fbcon) | the kernel's framebuffer device | 256-color and 24-bit requests are "shoehorned into" 16 foreground and 8 background colors | console bitmap fonts, at most 512 glyphs at once; with more than 256, only 8 of the 16 colors, as kbd's `setfont(8)` says | none in `console_codes(4)` | ships with the kernel |
| `kmscon/kmscon` 10.1.0, with `kmscon/libtsm` 4.8.0 | KMS/DRM, in software or through OpenGL ES 2, or fbdev | 24-bit | FreeType, Pango, PSF, and a built-in 8x16 font and Unifont | sixel is an open feature request | 10.1.0, 2026-10-26 |
| fbterm 1.7 | fbdev, or a VESA video card | 256, through its own private sequences under `TERM=fbterm` | fontconfig and FreeType | none listed | 1.7, 2010-10-06 |
| yaft 0.2.9 | fbdev at 8 to 32 bits per pixel, on Linux and FreeBSD, and on NetBSD and OpenBSD as experimental | 256 | BDF fonts converted into its source when it is built | sixel and DRCS, marked experimental | tag v0.2.9; last commit 2018-11-14 |
| fbpad | Linux fbdev | 256 | its own tinyfont format and PSF2 | none listed | last commit 2026-10-23 |
| vtium | KMS/DRM dumb buffers drawn on the CPU, or fbdev; a GPU renderer to come | 24-bit | PSF, kbd, BDF, outline and color fonts, drawn by a helper that never runs as root; built-in Unifont and Unscii-8 | sixel and kitty graphics; iTerm2 inline images to come | 0.1.0-alpha.1, a pre-release |

Sources, one per row:

- The kernel's VT console: [`console_codes(4)`](https://man7.org/linux/man-pages/man4/console_codes.4.html), from man-pages 6.19; kbd's [`setfont(8)`](https://man7.org/linux/man-pages/man8/setfont.8.html), from kbd's repository as of 2026-08-04; [the fbcon documentation](https://docs.kernel.org/fb/fbcon.html); and [the commit that removed fbcon's soft scrollback](https://github.com/torvalds/linux/commit/50145474f6ef4a9c19205b173da6264a644c7489), first in Linux 5.9.
- `kmscon/kmscon` and `kmscon/libtsm`: their READMEs and release notes for `kmscon` 10.0.0 to 10.1.0 and `libtsm` 4.8.0, and `kmscon`'s open feature request for sixel, its issue 138.
- fbterm: [its archived project page](https://code.google.com/archive/p/fbterm/), whose news dates release 1.7 to 2010-10-06, and the man page in its 1.7 source.
- yaft: [its repository](https://github.com/uobikiemukot/yaft), README and tags.
- fbpad: [its repository](https://github.com/aligrudi/fbpad) and README.
- vtium: this page and the records it links.

Besides what the table shows, the others do these things vtium does not:

- The kernel console needs no program in user space: fbcon takes over the console by itself.
- `kmscon` sets each monitor's rotation in its configuration, where vtium rotates a head with a key, and it follows the keyboard layout set through `localectl`.
- fbterm has a client-server input method framework and up to 10 windows.
- yaft runs on the BSD consoles.
- fbpad keeps two terminals to a tag and splits the screen between them.

On vtium's side are the heads, with their first-match rules and the closed policy for kiosks; fonts parsed and rasterized in a helper process apart from the console; scrollback bounded in bytes and rewrapped on a width change, where the kernel console's soft scrollback (Shift+PageUp) was removed in Linux 5.9; and the tests below.

## How it is judged

- **Unit tests:** each crate tests its own behaviour.
- **Tier 1, conformance:** byte streams are replayed through vtium's terminal, and the screen and the replies are compared as text with the C reference's goldens.
- **Capability probes:** each capability vtium claims has a replay that fails without it.
- **Tier 2, pixels:** the renderer draws known screens in memory, and they are compared pixel for pixel with goldens.
- **Tier 3, a virtual machine:** vtium's install tree boots on a VT of a QEMU/KVM guest with a real KMS device, keys are typed into it, and its screens are compared pixel for pixel with goldens. The run also records the size and memory figures above. The same run, `make tier3-minimal` at `c107f9a` on 2026-11-01, gave 47 passes, 28 listed departures and 16 cases against provisional goldens, with no failure. Tier 3 boots two pinned guests: Arch Linux from the archive snapshot of 2026-10-20, with systemd, and Debian 13 with sysvinit ([tools/tier3/README.md](tools/tier3/README.md)).
- **The VT review:** the maintainer checks a build by eye on a real VT, guided step by step ([docs/vt-review.md](docs/vt-review.md)).

Where vtium differs from a golden, the difference is a listed departure or the test fails. Goldens vtium made for screens the C reference never drew stay provisional, and judge nothing, until the maintainer reviews them.

The state vtium shows follows only from the bytes a program sends: the same stream gives the same screen and the same replies however it is split, which a property test checks. The parser, terminal and renderer deny every panic a program's input could reach, and only the platform crate may use `unsafe`. [docs/architecture.md](docs/architecture.md) lists each of these rules with the check that fails when it breaks.

## Trying it

Building needs Rust 1.88 or later and clang, and the first build fetches a Debian sysroot with glibc 2.36 to link against ([tools/arm/README.md](tools/arm/README.md)); `make install-tree` also needs `tic`. On a machine with systemd and logind, `make vt-shell` builds the minspec binary, picks the VT where `make vt-install` put a development build of vtium, or a free VT on a machine without one, asks for sudo, and runs your own shell in vtium there. Exit the shell to end the run, and the VT's own service starts again. [docs/vt-review.md](docs/vt-review.md) describes this and the other VT runs. [crates/vtium/README.md](crates/vtium/README.md#setting-it-up) says how to give an installed vtium a VT, or every VT, and how ssh behaves from it.

`make install-tree` stages what a system installs under `target/install-tree`: the console, `vtium-config`, the VT unit and its getty fallback, the terminfo entry and the example configuration. vtium reads its configuration from `/etc/vtium/vtium.conf`, which takes the C reference's keys and adds vtium's own ([docs/config.md](docs/config.md)).

[AGENTS.md](AGENTS.md) gives every gate and test command.

## Provenance

vtium is derived from `kmscon` and `libtsm`, as [COPYING](COPYING) says. `kmscon` was written by David Herrmann and the contributors COPYING lists, and `libtsm` by Aetf, David Herrmann and Ran Benita. vtium's console and libraries are Rust written for vtium; its tier 3 harness is the C fork's, carried into vtium's tree as described above.

The maintainer's code in vtium is written by large language models, under the maintainer's review, and judged by the tests above. Each decision is recorded with the maintainer's words in the decision records under [docs/architecture/](docs/architecture/).

Contributions are welcome, written by hand or by a large language model. Each pull request says how its code was written. Nothing is rejected for how it was written: every change is judged by the same tests and the same review. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

vtium is under the MIT licence ([LICENSE](LICENSE)). The Unifont glyphs built into the console are under the SIL Open Font License 1.1, and Unscii-8 is in the public domain ([COPYING](COPYING), [crates/vtium-font/README.md](crates/vtium-font/README.md)). COPYING also keeps the author lists of `kmscon` and `libtsm` and their copyright notices.
