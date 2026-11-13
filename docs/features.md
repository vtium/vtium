# What vtium does

This page lists what vtium renders and does, and what is still to come. [Concepts](concepts/index.md) explains how its parts fit together. [why-vtium.md](why-vtium.md) compares it with the other consoles for the Linux VT.

## Status

See [the releases](https://github.com/vtium/vtium/releases) for the latest version and what changed in it. A checkout of the source builds and runs everything on this page, except the items below. These items are designed and being built, but they are not finished yet:

- **The GPU path's remaining pieces.** A console built with the `gpu` feature renders on the GPU, through glow over EGL and GBM. It does so on each head whose `hwaccel` setting is on. `hwaccel` is off unless the build turns it on ([config.md](config.md)). The AUR's `vtium` and `vtium-git` packages turn it on from v0.1.0-alpha.6 on. A display wider than the GPU's largest texture is still rendered on the CPU. See [the texture tiling issue](https://github.com/vtium/vtium/issues/297).
- **libseat,** for access to the seat through seatd or logind. A seat is the set of displays and input devices that one person uses. See [the libseat issue](https://github.com/vtium/vtium/issues/34). Today vtium opens the VT, the graphics cards, and the input devices itself, as root.
- **Handing the display back without a VT switch,** through the `setBackground` sequence. See [the hand-back issue](https://github.com/vtium/vtium/issues/36).

These are also not built yet:

- Ligatures, and shaping across character cells.
- Input methods.
- The rest of the configuration tool's job: identifying a device by a keypress, `vtium config status`, and staging a trial. See [the issue that tracks it](https://github.com/vtium/vtium/issues/367).
- `vtium-clipboard`.
- Reading a changed vtium.conf without a restart.

The plan and its order are in [the tracker issue](https://github.com/vtium/vtium/issues/47).

## Displays

- vtium renders into DRM dumb buffers. A dumb buffer is a plain memory buffer that the display reads its picture from. vtium sets display modes with atomic modesetting where the graphics card offers it, and with the legacy calls where it does not. On fbdev, vtium renders at 32, 24 or 16 bits per pixel, or at 1 bit per pixel for monochrome panels.
- vtium follows displays as they are plugged in and out. It powers displays down after an idle time, and it renders in four orientations. When a monitor is swapped for another on a connector without a disconnect in between, vtium sets it up again as a new display. When a connector lists new modes, vtium chooses its mode again. When the card refuses a mode, the display keeps the mode it has and vtium holds the refused mode back. The display does not freeze, and vtium does not flood the card with retries. When only the refresh rate changes, vtium paces the display's frames to the new rate. See [crates/vtium-platform/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/README.md) for details.
- The CPU renderer is the default build's renderer, and it works on every machine. In a build with the `gpu` feature, the GPU renderer renders each head whose `hwaccel` setting is on. The GPU renderer produces the same pixels as the CPU renderer. Tier 2, vtium's pixel tests, renders each screen with both renderers, the GPU one under llvmpipe, a software GPU driver. Tier 2 requires the same bytes from both, except for images scaled with filtering. [Status](#status) lists the GPU renderer's remaining pieces.

## Sharing the display

vtium shares the graphics card with every other program that draws on the machine. vtium runs on one VT. The program on another VT may be a Wayland compositor, an X server, a getty, or another vtium. [Deployment and start-up](concepts/start-up.md#switching-vts) explains what happens when you switch between them.

Only one program at a time is the card's DRM master, which is the program allowed to set display modes. A program can drop master and take it again later ([kernel DRM documentation](https://docs.kernel.org/gpu/drm-uapi.html)). vtium hands the card over in the same way, whatever runs on the other VT:

- vtium takes control of its VT's switching from the kernel (`VT_PROCESS`). When the kernel asks to switch away, vtium does these things in order. It blanks its buffers. It waits for a frame still on its way to the display, for at most 1 second (`FLIP_WAIT_MS`). It hides its cursor and overlay planes where they show something. It drops DRM master. Only then does it let the kernel switch.
- When its VT comes back, vtium takes DRM master again and sets up every display. If the other program still holds master, vtium tries again every 50 ms, up to 40 times. After that, vtium tries again once a second for as long as its VT is shown. A key press, a pointer action or a hotplug also tries again at once. So a display with no keyboard, such as a kiosk's, lights up about a second after the other program lets go.
- Where the card offers atomic commits, vtium presents each frame with an atomic commit. The commit carries the frame's damage, the regions that changed, as `FB_DAMAGE_CLIPS`. The kernel calls this an "optional plane property to mark damaged regions" ([KMS documentation](https://docs.kernel.org/gpu/drm-kms.html)). Where the card does not offer atomic commits, vtium uses the legacy calls and presents whole frames. Where there is no DRM, it presents through fbdev.
- vtium.conf's rules can leave a display dark. vtium turns such a display off each time it sets the display modes, so a frame that another DRM master left on it goes dark. On a card with only the legacy calls, such a display still shows that frame.

With systemd, vtium runs on a VT in a getty's place, and `pam_systemd` registers the login with logind. If vtium fails, a fallback unit starts a getty. The fallback unit starts no getty when vtium.conf's policy is closed, or when starting the getty would stop another program, such as a display manager.

The package and `make install` both install the unit as `vtium@.service`, with its fallback `vtium-getty@.service`. They give vtium no VT at first. `vtium console enable N` gives vtium VT N, and `vtium console enable --all` gives it every free VT. systemd then starts vtium on those VTs at boot, where it would otherwise start a getty. logind starts vtium on a free VT from 2 to 6 where vtium has stopped. `make install-tree` makes the install tree that tier 3, vtium's virtual machine tests, installs in its guest. That tree installs the same two units. Tier 3 gives vtium the guest's VTs with `vtium console enable --all`, as a user does.

Without systemd, another init system runs `vtium --vt=ttyN --no-switchvt` where it ran a getty: from an inittab line, a script, or a supervisor. A person can also start vtium by hand. Where elogind runs, it registers the login in logind's place. Of the other init systems, only sysvinit is tested. OpenRC, runit, s6, busybox init, and elogind are not tested. busybox init does not catch `SIGRTMIN+4`, so under busybox init the power-off key does not power the machine off.

Tier 3 boots vtium under sysvinit on Debian 13, with no logind, from an inittab line. It boots vtium once with the udev daemon running (`sysvinit-640x480`) and once with it stopped (`sysvinit-no-udev`). There, vtium's reboot and power-off keys go through sysvinit. The screens vtium renders there match vtium's own goldens to the pixel. A golden is a saved expected result. Those goldens still wait for the maintainer's review. [systemd.md](systemd.md) lists each place vtium touches systemd, logind or PID 1, and what it does without them.

With libseat, vtium gets the VT, the graphics cards, and the input devices from the seat manager. The seat manager is seatd or logind, whichever is running. libseat is the seat management library "allowing applications to use whatever seat management is available" ([seatd's README](https://git.sr.ht/~kennylevinsen/seatd/blob/master/README.md)). The vtium process still runs as root, so that it can start `/bin/login`. libseat support is designed and being built. See [Status](#status).

## Heads: a terminal per display

One vtium process runs on one VT and drives every display it finds. A head is a terminal for one or more displays, with its own grid, its own font and its own consoles. A console is one session, with its own login, screen and scrollback. A head shows one of its consoles at a time.

With no configuration, vtium runs one head across every display, with its grid sized for the smallest display. vtium's configuration file is `/etc/vtium/vtium.conf`. Below its settings, sections of the file declare the heads, the look each head renders with, and an ordered list of rules. A look is a head's font set and its font settings.

vtium checks each display, keyboard and pointer against the rules from the top, as a firewall checks a packet against its rules. The first rule that matches decides what happens to the device. The rule can put the device in a head. It can make the device roam, which means it types into whichever head is active. It can make vtium ignore the device, or switch it off. A rule names a device by its port, by the monitor that its EDID names, by its DisplayPort multi-stream hub port, or by its name. The EDID is the identity data that a monitor reports.

The file's policy, `open` or `closed`, decides what vtium does with any device the rules do not mention. Under `policy = closed`, vtium ignores any input device that no rule names. It also isolates every head unless the file says otherwise. An isolated head keeps its keyboards to itself: their VT-switch keys reach its session as plain keys, and no roaming keyboard types into it. The closed policy is the setup a kiosk needs. Under `policy = open`, a keyboard that no rule names types into the active head, and vtium guards against locking every keyboard out. [Heads](concepts/heads.md#two-policies-open-and-closed) explains the two policies.

`vtium check-config FILE` checks a vtium.conf before you install it. It uses the same parser and rules that vtium uses when it starts. It refuses a file that leaves no keyboard able to reach a display. [heads.md](heads.md) gives the sections for each setup. The setups are the default, two heads with their own keyboards, a kiosk, a kiosk with an operator's head, one person at each head, and everything off. [head-file.md](head-file.md) is the reference for the sections.

## Input

vtium reads keyboards and pointers directly from their devices. It uses libxkbcommon for the keymap.

A program can ask for the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/), which editors such as Neovim and Helix and shells such as fish use. Under it, the program can tell every key and modifier apart, Escape from the start of an escape sequence and Ctrl+I from Tab among them. It can also ask for a key's repeats and releases, for every key as an escape code, and for the text a key types. On a non-Latin layout it gets each key's place on the US layout too, so a shortcut such as Ctrl+C works there. See [the kitty keyboard issue](https://github.com/vtium/vtium/issues/42).

You select text with the pointer. The middle button and Shift+Insert paste it. A program can copy text into the same selection with OSC 52. A program cannot read the selection back.

You can bind keys to reboot, power off or suspend the machine. See [systemd.md](systemd.md) for details.

- The reboot key sends PID 1 the kernel's Ctrl+Alt+Del signal, so systemd or sysvinit stops the services before the restart. Where the kernel's `ctrl-alt-del` setting is not 0, vtium instead syncs the disks and restarts the machine at once, with no shutdown.
- The power-off key sends PID 1 `SIGRTMIN+4`. systemd and sysvinit take that signal as a clean power-off. When vtium starts, it warns if PID 1 neither catches nor blocks that signal.
- In a build with the `logind` feature, the suspend key asks logind to suspend the machine. In other builds, vtium writes `mem` to `/sys/power/state`. A suspend stops no service.

## Fonts

The console never parses a font file itself. A frit is a font set file, `/etc/vtium/fonts/NAME.vtfrit`. It lists a font's sources in order, and each later source fills in the characters that the earlier ones lack. A helper program, `vtium-config serve`, reads the sources and rasterizes them while vtium runs. It sends the console ready-to-draw glyphs, the drawn shapes of the characters, at the console's character cell size. The console checks each answer from the helper, and keeps the glyphs in memory until the next boot.

- When vtium runs as root, the helper runs as `nobody`. The console restarts a crashed helper at most 3 times per boot. Until the helper is back, the console draws with its built-in fonts.
- The helper reads PSF and raw console fonts, kbd font names, BDF fonts, and the kernel's own VT font. It also reads outline fonts, by family or by file, including color emoji (COLR version 0, CBDT, and sbix). It rasterizes outlines with swash. It shapes each grapheme cluster, a character as a reader sees it, with HarfRust.
- Unifont and Unscii-8 are built into the console. They draw the first frame and every character a frit lacks.
- `vtium config use-frit` switches the running console's font through OSC 50. Ctrl+= and Ctrl+- zoom the font. A switch or a zoom lasts until the session ends. Then vtium returns the head to the font and size that vtium.conf names.
- `vtium config tui`, run at a terminal, opens vtium-config's full-screen tool. The tool opens on its Flow view, which draws the configuration as six columns. The columns run from the displays, through the rules, heads and looks, to the input devices. The view places the devices that are plugged in now. Every change goes into a draft of your own. Only Install writes `/etc/vtium/vtium.conf`. Install runs one `sudo vtium-config apply DIR`, the only step that runs as root. For scripts, every change the view makes is also a `vtium config` command with fixed exit statuses. The tool needs a terminal of at least 80 by 25 cells.
- The tool's Fonts view lists the frits and the built-in fonts. In a vtium console, as you move the highlight to a font, the Fonts view switches the live console to that font through OSC 50. So you judge each font on the real console. Where the console draws kitty graphics, the view shows each size as an image.
- `vtium config new-frit` makes a frit of the fonts you name, in the order you name them. Run with no fonts at a terminal, it opens the same tool at its new-frit view. `vtium config preview` draws a sample of each font you name, as the helper would draw it, and writes nothing. [configure.md](configure.md) walks through the draft and the Flow view, and [fonts.md](fonts.md) walks through the fonts.

## Images

vtium shows images in a build with the `images` feature. The default build, minspec, leaves that feature out. The `vtium` and `vtium-git` packages build it when `VTIUM_FEATURES` names `images` ([install.md](install.md#arch-linux)). A build with images handles four kinds of image. It also ships `vtium-gallery`, a tool that shows PNG images as a grid of thumbnails:

- **Sixel.**
- **The kitty graphics protocol.** A program sends the pixels directly, or names a file, a temporary file or a shared-memory object that holds them. A placement, one showing of an image on the screen, can sit relative to another placement (the `P`, `Q`, `H` and `V` keys). A program can also show a virtual placement by writing Unicode placeholder cells (`U=1`).
- **Kitty animation.** A store keeps each frame as the program sent it. It composes the pixels only when they are needed, within its own allowance for frames. The vtium binary's animation clock plays the frames. The clock sets no timer while nothing plays, or while the console is hidden.
- **iTerm2's inline images** (OSC 1337 `File=`, and the `MultipartFile`, `FilePart` and `FileEnd` form for a transfer in parts). An animated GIF sent this way plays through kitty animation.

vtium keeps each console's images and their placements in one store, which has a limit on its memory. It draws them with the CPU compositor. A placement is anchored to the line that holds its top row, so it moves with the text. Each protocol's own rules decide what erases a placement and how it scrolls.

The console never opens a file that a program names. It never decodes PNG, JPEG, GIF or zlib data itself. A separate program, the image helper (`/usr/bin/vtium-image`), does both. The image helper runs as the session's user, never as root. A seccomp filter and a landlock domain limit what it can do.

When an image changes, vtium redraws only the cells it covers. Tier 3's `img-tmux-pane` screen measures this on a tmux grid of sixel panes, with one pane playing through mpv's sixel output. In that test, vtium redrew a mean of 58,101 pixels a frame. That is 83.1 percent of the 69,888 pixels of the playing pane, and 18.9 percent of the screen. See [the issue that adds pictures to the CPU compositor](https://github.com/vtium/vtium/issues/30).

## The terminal

- vtium renders 24-bit color, the 256-color palette and named palettes. It renders bold, faint, strike, blink and reverse text. It renders underline in five styles (single, double, curly, dotted, dashed), each in a color of its own.
- It splits Unicode text into extended grapheme clusters, the characters as a reader sees them. It draws wide characters and emoji presentation. When a program resets mode 2027, vtium measures text width with the codepoint-width method instead.
- A console built with the `bidi` feature lays out right-to-left text, such as Arabic and Hebrew. It shows the text in the order that the Unicode bidirectional algorithm gives. It draws mirrored characters as their counterparts, and it joins Arabic letters. The cursor, the selection, mouse reports and the arrow keys all move in that same order.
- It reports the mouse with modes 9, 1000, 1002 and 1003, in the SGR, urxvt and pixel encodings. It also reports focus, bracketed pastes, and in-band resizes (mode 2048). It holds frames back for synchronized output (mode 2026).
- It keeps OSC 8 links and OSC 133 prompt marks. vtium draws a dashed underline under the link beneath the pointer. While you hold Ctrl over a link, vtium shows the link's address on the bottom row. When the pointer is on the bottom row, vtium shows the address on the top row instead. Ctrl+click copies a link's address and never opens the link. Ctrl+Shift+Up and Ctrl+Shift+Down move the view from prompt to prompt.
- Programs set, query and reset colors with OSC 4, 10, 11 and 12, and with xterm's and kitty's color stacks. vtium carries out DEC's rectangular area operations, character protection, XTSAVE, and XTRESTORE. It answers XTGETTCAP, XTVERSION, and DECRQM.
- A reply to a query is built from vtium's own state. It never repeats bytes the program sent, so text a program prints cannot type into the program reading the replies.

[modes.md](modes.md) lists each mode vtium recognizes, with its test. It is generated from the code. A reference page for every escape sequence vtium understands is still to come. See [the escape sequence reference issue](https://github.com/vtium/vtium/issues/345).

## Scrollback and reflow

Each console's scrollback has a limit in bytes, which the `sb-memory` key sets. By default, the limit keeps the minspec build's screens and scrollback within a peak of 16 MiB. The default limit is what remains of that peak once the console's two screens, the main screen and the alternate screen, are counted. A small display has small screens, so more is left for scrollback. A grid of 170 by 48 leaves 5 MiB. In a build with images, it leaves 4,653,056 bytes (about 4.4 MiB), because that build counts room in each row for kitty's placeholder cells. A display larger than 170 by 48 keeps that same limit. On a machine that can spare more memory, `sb-memory` raises it. All consoles together stay within a quarter of the machine's memory.

When the width changes, vtium rewraps the main screen and its scrollback. It carries the cursor, the selection and the view along with the text.

## Small machines

The default build, minspec, is the smallest build that is still a complete console. It holds the CPU renderer, bitmap fonts, the parser and screen, the pty, and input. It is built for a single-board computer with ARM cores of the Cortex-A53 class or older, 512 MB of memory or less, and no GPU. Its smallest display is a 320×240 panel with 8×8 cells, which gives 40 columns by 30 rows. `make arm` builds it for aarch64 and armv7.

Tier 3 measures minspec in a virtual machine guest with 1 vCPU and 512 MB of memory. These figures come from `make tier3-minimal` at commit `c107f9a`, run on 2026-11-01:

| Measure | Value |
|---|---|
| Console binary | 3,216,864 bytes. The run fails past a budget of 4,194,304 bytes (4 MiB). |
| Install tree, the font helper included | 7.63 MiB |
| Peak resident memory at 320×240, 8×8 cells | 5.81 MiB |
| Peak resident memory at 640×480, 8×16 cells | 7.77 MiB |
| Peak resident memory at 1366×768, 8×16 cells | 12.07 MiB |
