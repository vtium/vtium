# What vtium does

This page lists what vtium draws and does, and what is still to come. [concepts.md](concepts.md) explains how its pieces fit together, and [why-vtium.md](why-vtium.md) compares it with the other consoles for the Linux VT.

## Status

vtium's latest release is the pre-release v0.1.0-alpha.3. It wakes the display again after a desktop has moved it to another display engine, draws kitty graphics sent in chunks the way kitty does, and only redraws when the screen changed. It also brings `vtium-config` help and settings files laid out in sections, and the groundwork of a GPU renderer that is not switched on yet. A checkout builds and runs everything on this page except these, which are designed and landing:

- **The GPU renderer,** on glow over EGL and GBM ([the GPU path's issue](https://github.com/vtium/vtium/issues/33)). Today every build draws on the CPU.
- **libseat,** for seat access through seatd or logind ([the libseat issue](https://github.com/vtium/vtium/issues/34)). Today vtium opens the VT, the cards and the input devices itself, as root.
- **Handing the display back without a VT switch,** through the `setBackground` sequence ([the hand-back issue](https://github.com/vtium/vtium/issues/36)).
- **Showing a link's address in the console:** Ctrl+click copies an OSC 8 link's address, Ctrl+Shift+Up and Ctrl+Shift+Down move the view between the prompts OSC 133 marks, and the link under the pointer draws a dashed underline. The renderer does not yet draw the overlay row that shows the link's address while Ctrl is held ([ADR-1002](architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md)).

Also not built yet: glyph ink reaching into the next cell ([ADR-1302](architecture/render/ADR-1302-glyph-ink-reaches-one-cell-past-its-own-side-to-side-and-is-cut-at-its-row.md), decided), ligatures and shaping across cells, the bold and italic faces of outline fonts, the kitty keyboard protocol, bidirectional text, input methods, `vtiumctl`, the font builder (`vtium config new`), the picture preview command (`vtium config preview`), `vtium-clipboard`, and reloading the head file without a restart. The plan and its order are in [the tracker issue](https://github.com/vtium/vtium/issues/47).

## Displays

- vtium draws into DRM dumb buffers, plain memory buffers the display scans out, with atomic modesetting where the card offers it and the legacy calls where it does not. On fbdev it draws at 32, 24 or 16 bits, or at 1 bit for monochrome panels.
- It follows displays as they are plugged in and out, powers them down after an idle time, and draws in four orientations. A monitor swapped on a connector without a disconnect is set up again as a new display, and a connector that lists new modes gets its mode chosen again. A mode the card refuses is held back, with no freeze and no flood of retries. A change of refresh rate alone paces the display's frames again ([crates/vtium-platform/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/README.md)).
- The CPU renderer is the default build's renderer and draws everywhere. The GPU renderer, behind a build feature, draws the CPU renderer's pixels ([ADR-1301](architecture/render/ADR-1301-one-glyph-engine-draws-at-run-time-for-the-display-s-capabilities.md)): tier 2 draws each screen on both, the GPU path under llvmpipe, and requires the same bytes, except for pictures scaled with filtering ([ADR-1300](architecture/render/ADR-1300-the-drawn-glyph-cache-and-frame-composition.md)). Landing; see [Status](#status).

## Sharing the display

vtium shares the graphics card with whatever else draws on the machine: vtium runs on one VT, and the program on another VT may be a Wayland compositor, an X server, a getty or another vtium. [concepts.md](concepts.md#switching-vts) explains a switch between them. Only one program at a time is the card's DRM master, the one allowed to set modes, and master can be dropped and taken again ([kernel DRM documentation](https://docs.kernel.org/gpu/drm-uapi.html)). vtium hands the card over the same way whatever is on the other VT:

- vtium takes its VT's switching from the kernel (`VT_PROCESS`). When the kernel asks for a switch away, vtium blanks its buffers, waits for a frame still on its way to the display (at most 1 second, `FLIP_WAIT_MS`), hides its cursor and overlay planes and drops DRM master, and only then lets the kernel switch.
- When its VT comes back, vtium takes DRM master again and sets every display. While the other program still holds master, vtium tries again every 50 ms, 40 times, and then on the next key press, pointer action or hotplug.
- It presents a frame with an atomic commit that carries the frame's damage as `FB_DAMAGE_CLIPS` (the kernel's "optional plane property to mark damaged regions", [KMS documentation](https://docs.kernel.org/gpu/drm-kms.html)) where the card offers it, with the legacy calls and whole frames where it does not, and through fbdev where there is no DRM.
- A display the head file leaves dark is turned off at each modeset, so a frame another master left on it goes dark. On a card with only the legacy calls, such a display still shows that frame.

With systemd, vtium runs on a VT in a getty's place, `pam_systemd` registers the login with logind, and a fallback unit starts a getty if vtium fails, unless the head file's policy is closed. The package, and `make install`, install the unit as `vtium@.service` with its fallback `vtium-getty@.service`, and give vtium no VT until `vtium console enable N` gives it VT N or `vtium console enable --all` gives it every VT; logind then starts vtium on those VTs where it would start a getty. `make install-tree`, the tree tier 3 installs in its guest, installs the same two units, and tier 3 gives vtium the guest's VTs with `vtium console enable --all`, as a user does. Without systemd, another init runs `vtium --vt=ttyN --no-switchvt` from an inittab line, a script or a supervisor where it ran a getty, or a person starts it by hand, and elogind, where it runs, registers the login in logind's place. Of the other inits only sysvinit is tested; OpenRC, runit, s6, busybox init and elogind are not. busybox init does not catch `SIGRTMIN+4`, so the power-off key powers nothing off there. Tier 3 boots vtium under sysvinit on Debian 13, with no logind, from an inittab line, with the udev daemon running (`sysvinit-640x480`) and with it stopped (`sysvinit-no-udev`), and its reboot and power-off keys go through sysvinit. The screens it draws there match vtium's own goldens to the pixel, and those goldens still wait for the maintainer's review. [systemd.md](systemd.md) lists each place vtium touches systemd, logind or PID 1, and what it does without them.

With libseat, vtium takes the VT, the cards and the input devices from the seat manager, seatd or logind, whichever is running. libseat is the seat management library "allowing applications to use whatever seat management is available" ([seatd's README](https://git.sr.ht/~kennylevinsen/seatd/blob/master/README.md)). The process still runs as root to start `/bin/login` ([ADR-1400](architecture/platform/ADR-1400-the-privilege-and-sandbox-model.md)). Landing; see [Status](#status).

## Heads: a terminal per display

One vtium process runs on one VT and drives every display it finds. A head is a terminal for one or more displays, with its own grid, its own font and its own consoles. A console is one session with its own login, screen and scrollback, and a head shows one of its consoles at a time.

With no configuration, vtium runs one head on every display, sized for the smallest. The head file, `/etc/vtium/heads.conf`, declares heads and an ordered list of rules. Each display, keyboard and pointer walks the rules from the top, as a packet walks a firewall's rules, and the first rule that matches puts it in a head, makes it roam (type into whichever head is active), ignores it, or switches it off. A rule names a device by its port, by the monitor its EDID names, by its DisplayPort multi-stream hub port, or by its name.

The file's policy, `open` or `closed`, decides what happens to whatever its rules do not mention. Under `policy = closed`, vtium ignores any input no rule names and isolates every head unless the file says otherwise, which is the setup a kiosk needs; under `policy = open`, such a keyboard types into the active head, and vtium guards against locking every keyboard out. [concepts.md](concepts.md#two-policies-open-and-closed) explains the two policies.

`vtium check-config FILE` checks a head file before it is installed, with the same parser and rules the start uses. [heads.md](heads.md) gives a complete head file for each setup: the default, two heads with their own keyboards, a kiosk, a kiosk with an operator's head, one person to each head, and everything off. [head-file.md](head-file.md) is the file's reference.

## Input

vtium reads keyboards and pointers directly from their devices, with libxkbcommon for the keymap. The pointer selects text, the middle button and Shift+Insert paste it, and a program can copy into the same selection with OSC 52. A program cannot read the selection back. Keys can be bound to reboot, power off or suspend ([systemd.md](systemd.md)). The reboot key sends PID 1 the kernel's Ctrl+Alt+Del signal, so systemd or sysvinit stops the services before the restart; where the kernel's `ctrl-alt-del` setting is not 0, vtium syncs the disks and restarts the machine at once, with no shutdown. The power-off key sends PID 1 `SIGRTMIN+4`, which systemd and sysvinit take as a clean power-off, and vtium warns at start when PID 1 does not catch that signal. The suspend key asks logind to suspend in a build with the `logind` feature, and otherwise writes `mem` to `/sys/power/state`; a suspend stops no service.

## Fonts

The console parses no font file. A frit (a font set file, `/etc/vtium/fonts/NAME.vtfrit`) lists a font's sources in order, and later sources fill the characters the earlier ones lack. A helper program, `vtium-config serve`, reads the sources and rasterizes them at run time, and sends the console ready-to-draw glyphs for its cell size. The console checks each answer and keeps the glyphs in memory for the boot.

- When vtium runs as root, the helper runs as `nobody`. The console restarts a crashed helper at most 3 times a boot, and draws its built-in fonts meanwhile.
- The helper reads PSF and raw console fonts, kbd font names, BDF fonts, the kernel's own VT font, and outline fonts by family or file, color emoji included (COLR version 0, CBDT and sbix). It rasterizes outlines with swash and shapes each grapheme cluster with HarfRust.
- Unifont and Unscii-8 are built into the console. They draw the first frame and every character a frit lacks.
- `vtium config set` switches the running console's font through OSC 50, and Ctrl+= and Ctrl+- zoom it. A switch or a zoom lasts for the session: when the session ends, vtium returns the head to the font and size vtium.conf names.
- `vtium config` with no command, run at a terminal, opens a full-screen tool for the fonts and settings. It lists the frits and the built-in fonts, and in a vtium console it switches the live console to each font you move the highlight to, through OSC 50, so you judge the real console. Where the console draws kitty graphics, it shows each size as a picture. It edits vtium.conf's font keys and each head's profile, and saves through `sudo vtium-config apply DIR`, the only step that runs as root. It needs a terminal of at least 40 by 25 cells. [fonts.md](fonts.md) walks through it, and [ADR-1201](architecture/font/ADR-1201-vtium-config-manages-the-console-s-fonts-and-settings-trying-each-choice-on-the-live-console.md) records the design.

## Pictures

vtium draws pictures in a build with the `images` feature, which minspec leaves out. The AUR packages build minspec, so a package draws no pictures until it is built with `VTIUM_FEATURES='images'` ([install.md](install.md)). Such a build handles four kinds of picture:

- **Sixel.**
- **The kitty graphics protocol.** A program sends pixels directly, or names a file, a temporary file or a shared-memory object. A placement can sit relative to another placement (the `P`, `Q`, `H` and `V` keys), and a program can show a virtual placement by writing Unicode placeholder cells (`U=1`).
- **Kitty animation.** A store keeps each frame as the program sent it and composes the pixels when they are needed, within a frame allowance of its own. The binary's animation clock plays the frames, and arms no timer while nothing plays or while the console is hidden.
- **iTerm2's inline images** (OSC 1337 `File=`, and the `MultipartFile`, `FilePart` and `FileEnd` form for a transfer in parts). An animated GIF sent this way plays through kitty animation.

The console keeps each console's pictures and their placements in one store, bounded in memory, and draws them with the CPU compositor ([ADR-1100](architecture/image/ADR-1100-one-image-pipeline-protocol-handlers-feed-a-store-of-placements-that-travel-with-lines.md)). A placement is anchored to the line holding its top row, so it moves with the text, and each protocol's own rules decide what erases it and how it scrolls. Kitty animation is in [ADR-1102](architecture/image/ADR-1102-kitty-animation-frames-are-stored-as-the-program-sent-them-and-composed-before-drawing-and-a-clock-outside-the-store-plays-them.md) and iTerm2's pictures in [ADR-1103](architecture/image/ADR-1103-iterm2-pictures-are-sized-by-their-keys-and-decoded-in-the-image-helper.md).

The console never opens a file a program names and never decodes PNG, JPEG, GIF or zlib itself. The image helper, a separate program (`/usr/bin/vtium-image`), does both as the session's user and never as root, behind a seccomp filter and a landlock domain ([ADR-1101](architecture/image/ADR-1101-the-image-helper-is-its-own-sandboxed-program-run-as-the-session-s-user.md)). A picture that changes redraws only the cells it covers: in tmux's grids of sixel panes, the C reference damaged 97 to 100 percent of the screen on each flip. Tier 3's `img-tmux-pane` screen measures vtium on such a grid, with one pane playing through mpv's sixel output: vtium damaged a mean of 58,101 pixels a frame, 83.1 percent of the 69,888 pixels of the playing pane and 18.9 percent of the screen ([vtium/vtium#30](https://github.com/vtium/vtium/issues/30)).

## The terminal

- vtium draws 24-bit color, the 256-color palette and named palettes. It draws bold, faint, strike, blink and reverse, and underline in five styles (single, double, curly, dotted, dashed), each in a color of its own.
- It cuts Unicode text into extended grapheme clusters, draws wide characters and emoji presentation, and uses the codepoint-width method when a program resets mode 2027.
- It reports the mouse (modes 9, 1000, 1002 and 1003, in the SGR, urxvt and pixel encodings), focus, bracketed pastes and in-band resizes (mode 2048), and holds frames for synchronized output (mode 2026).
- It keeps OSC 8 links and OSC 133 prompt marks. Ctrl+click copies a link's address, never opening it, and Ctrl+Shift+Up and Ctrl+Shift+Down move the view from prompt to prompt.
- Programs set, query and reset colors with OSC 4, 10, 11 and 12, and with xterm's and kitty's color stacks. vtium carries out DEC's rectangular area operations, character protection, XTSAVE and XTRESTORE, and answers XTGETTCAP, XTVERSION and DECRQM.
- A reply to a query is built from vtium's own state. It never repeats bytes the program sent, so text a program prints cannot type into the program reading the replies.

[ADR-1000](architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md) lists every mode and feature vtium supports, ignores or refuses, and [modes.md](modes.md), generated from the code, lists each mode with its test.

## Scrollback and reflow

Each console's scrollback is bounded in bytes. By default the bound keeps minspec's grids inside their 16 MiB peak: it is what that peak leaves after the console's two screens, more on a small display, and at 170 by 48 5 MiB, or 4,653,056 bytes (about 4.4 MiB) in a build with images, which also prices kitty placeholder cells into a screen row; a larger display keeps the 170 by 48 figure, so `sb-memory` raises it on a machine that can spare more. All consoles together stay within a quarter of the machine's memory. When the width changes, vtium rewraps the main screen and its scrollback, and carries the cursor, the selection and the view with them ([ADR-1003](architecture/term/ADR-1003-reflow-on-a-width-change-and-no-cell-past-the-margin.md)).

## Small machines

The default build, minspec, is the smallest complete console: the CPU renderer, bitmap fonts, the parser and screen, the pty and input. It is built for a single-board computer with ARM cores of the Cortex-A53 class or older, 512 MB of memory or less, and no GPU, and its smallest display is a 320x240 panel with 8x8 cells, 40 columns by 30 rows. `make arm` builds it for aarch64 and armv7.

Tier 3 measures it in a guest with 1 vCPU and 512 MB. These figures come from `make tier3-minimal` at commit `c107f9a`, run on 2026-11-01:

| Measure | Value |
|---|---|
| Console binary | 3,216,864 bytes, against a budget of 3,670,016 bytes (3.5 MiB) that fails the run past it |
| Install tree, the font helper included | 7.63 MiB |
| Peak resident memory at 320x240, 8x8 cells | 5.81 MiB |
| Peak resident memory at 640x480, 8x16 cells | 7.77 MiB |
| Peak resident memory at 1366x768, 8x16 cells | 12.07 MiB |
