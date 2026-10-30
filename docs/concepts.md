# VTium's concepts

This page introduces the ideas behind vtium: what it is, the programs it is made of and who runs each, how fonts reach it, how it is installed, started and used, and how it is judged. It explains, at the level of concepts; each section links to the decision record, issue or document that holds the detail, and the commands and setting names live there too. A piece that is not built yet is marked proposed, with the issue or record that holds it.

## What VTium is

vtium is a terminal for the Linux virtual console, the full-screen text terminals the kernel provides, each called a VT. It draws the terminal itself, straight to the display through the kernel's display interface (KMS/DRM, or the older fbdev framebuffer where that is all a machine has), with no display server. A program run in vtium sees a modern terminal: colors, text styles, Unicode and mouse reporting, with images to come. The features vtium supports, ignores or refuses are listed in [ADR-1000](architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md).

These three screens are vtium's own drawing, made in memory by its renderer: the color palette, double-width Japanese, Chinese and Korean text, and the underline styles in their own colors.

![vtium's drawing of the palette: the 16 background colors, the 16 foreground colors on letter pairs, a ramp through the 256-color palette, and a 24-bit gradient from red to green.](figures/concepts-screen-palette.png) ![vtium's drawing of double-width text: Japanese, Chinese and Korean, full-width Latin, narrow letters between wide characters, and wide text in bold and in reverse.](figures/concepts-screen-cjk.png) ![vtium's drawing of underlines: single, double, curly, dotted and dashed, each in the text color and in a color of its own, then underlines on a background, in reverse and in bold.](figures/concepts-screen-underline.png)

vtium is written in Rust, and is inspired by kmscon and libtsm, the C terminal and terminal library it learned its behaviour from. Their code, in the maintainer's fork, is the C reference: vtium is judged against what it did, and goes further where the specifications do ([architecture.md](architecture.md#beyond-the-c-reference)).

### Small machines first

vtium is built to run on small machines first and large ones as well. Its default build, minspec, is the smallest complete console: it draws through the CPU into a plain memory buffer the display shows, uses bitmap fonts, and needs no GPU; images, outline fonts and a GPU path are features built on top of it. Its yardstick is a single-board computer with ARM cores and 512 MB of memory or less, and its smallest display is a 320x240 panel, where an 8x8 font gives a grid of 40 columns by 30 rows. The same build draws on large displays too. [architecture.md](architecture.md#the-minspec-build) sets out the machine, the display floor and what is promised at each size.

The same shell session at the display floor and one step up, as vtium draws it: 320x240 with an 8x8 font, 40 columns by 30 rows, and 640x480 with an 8x16 font, 80 columns by 30 rows.

![A shell session on a 320x240 display in an 8x8 font: a reversed title row, a wrapped command line, a box around styled words, the 16 colors, underline styles and a reversed status line, in 40 columns by 30 rows](figures/concepts-screen-small-320x240.png)

![The same session on a 640x480 display in an 8x16 font, 80 columns by 30 rows, with a directory listing, four box styles, and 256-color and 24-bit color ramps](figures/concepts-screen-small-640x480.png)

## The suite

vtium is one console and a few programs and files around it. The table lists each, who runs it, and whether it is built or proposed. vtium-vitrify's design is ADR-1200, accepted on 2026-10-28 (vtium/vtium#3), with the commands later issues add.

| Piece | What it is | Run by | State | Held by |
|---|---|---|---|---|
| `vtium` | The console: one process per VT, which owns that VT, its display and its input devices, and runs the session | root, started by init or logind | built | [ADR-1400](architecture/platform/ADR-1400-the-privilege-and-sandbox-model.md), vtium/vtium#21 |
| `vtium blank`, `vtium wake` | Blank or wake a VT's displays from outside its session, by signalling the vtium on that VT | root, or the user vtium runs as | built | vtium/vtium#116 |
| `vtium-vitrify preview` | Draws a sample of a font as the helper would draw it, writing nothing | the user | proposed | vtium/vtium#120 |
| `vtium-vitrify new` | The guided builder, and the templates it writes and reads, which end in a frit | the user, with sudo once at the end | proposed | vtium/vtium#121 |
| `vtium-vitrify serve` | The helper: generates a frit's glyphs at run time in a sandbox, started on the console's first need and gone when idle | the console, never as root | built: PSF and kbd sources, and outline and color fonts | vtium/vtium#145, #23 |
| `vtium-vitrify set` | Switches the running console among the frits and the built-in fonts | the user, in the session | built | vtium/vtium#126 |
| `vtiumctl detect`, `setup`, `status`, `check`, `template`, `install` | The configuration tool, a small binary of its own: lists every display and input with a rule ready to paste, writes a head file by asking which keyboard belongs to which display, reports what the running console did with each device, checks any configuration file, frits included, writes the commented default configuration when asked, and installs a checked file | any user; root to install the file | proposed | [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) |
| `vtium-clipboard`, also `vticlip` | Copies to the console's selection from a script or a pipe | the user, in the session | proposed; copy decided, paste open | vtium/vtium#113 |
| The frits | Small text files that name a font's sources, which the helper draws from | written by root | built | vtium/vtium#3, #145 |
| The built-in fonts | Unifont, and Unscii-8 for small cells, compiled into vtium: the first frame's font and the fallback for each character a frit lacks | inside `vtium` | built: Unscii-8 for cells under 16 pixels and unifont from 16 | vtium/vtium#3, #141 |
| The configuration | One file, a file for the seat, and the command line | written by root, read by `vtium` | built | [config.md](config.md) |
| The head file | The heads and the ordered rules that place each display and input; vtium runs its compiled-in rules when there is none and never writes one | written by root, or by `vtiumctl setup` or `template`, read by `vtium` | proposed | [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) |
| The unit | systemd's service for vtium on a VT, in getty's place | started by logind | built | [systemd.md](systemd.md) |
| The terminfo entry | Tells programs in the session what vtium can do | read by the session's programs | built | [crates/vtium](../crates/vtium/README.md), vtium/vtium#21 |

vtium is the only piece needed to use the console. vtium-vitrify, vtiumctl and vtium-clipboard are separate binaries, so the font decoders, the rasteriser and the configuration tool stay out of the console's binary, and neither counts against the minspec build's budget.

### From install to use

```mermaid
flowchart LR
    PKG["a distribution<br>package installs<br>vtium"] --> INIT["init or logind<br>starts vtium<br>on a VT"]
    INIT --> TAKE["vtium takes<br>the VT, the display<br>and the input"]
    TAKE --> PTY["vtium starts<br>login in a pty"]
    PTY --> USE["the user's<br>session"]
    classDef step fill:#2d7d9a,color:#ffffff,stroke:#94a3b8
    classDef core fill:#7c3aed,color:#ffffff,stroke:#94a3b8
    classDef user fill:#475569,color:#ffffff,stroke:#94a3b8
    class PKG,INIT,PTY step
    class TAKE core
    class USE user
```

### What runs as root, and what as the user

The figure shows the suite in two bands, what runs as root above and what runs as the user who logged in below. The session reaches the console only through the pty, the kernel's pair of devices that lets one program act as the terminal of another; the font builder reaches root's side once, through sudo. Dashed pieces are proposed.

![Two bands. Above, running as root: the kernel's devices (the VT, the display, keyboards and pointers) joined to vtium, the console; vtium-vitrify create and install writing font sets into the root-owned store, which gives vtium checked sets. Between the bands, the pty, joined to vtium by bytes both ways. Below, running as the user who logged in: vtium-vitrify set, vtium-clipboard, and shell and programs, each joined to the pty; and vtium-vitrify preview, and vtium-vitrify new with its templates, the second of which hands a frit to create with sudo, once. Every vtium-vitrify piece and vtium-clipboard are drawn dashed, as proposed; the store is drawn solid, as built.](figures/concepts-pieces.svg)

The console runs as root for its whole life, as the C reference did. Root lets it open the VT, take the display, read the keyboards and pointers directly, and start the system's login program, which authenticates the user and starts their shell with the user's rights. [ADR-1400](architecture/platform/ADR-1400-the-privilege-and-sandbox-model.md) holds the privilege model, the image helper that decodes pictures as the session's user included.

A program in the session reaches vtium only through the bytes it writes to its terminal. vtium's replies to those bytes are built from its own state and never echo what a program sent, so text a program prints cannot type into another program reading the replies ([architecture.md](architecture.md#determinism), "No reflection").

Only root writes frits, so the configuration and OSC 50 name only fonts root chose. Everything before that runs as the user: previews, the builder's questions and templates. The helper that draws a frit's glyphs never runs as root, and the console trusts its glyphs only to be well-formed. Switching among frits and copying to the clipboard run in the session and speak to vtium through the terminal like any other program.

## Deployment

A distribution package installs vtium's programs, the service unit, the terminfo entry and the example configuration, and frits where it ships them. Nothing is built at install: the console generates a frit's glyphs at run time (ADR-1200).

On a systemd machine, vtium's unit takes the place of getty, the program that prints the login prompt on a VT: enabled, it makes logind start vtium on each VT it would have given a getty. When vtium cannot start, for example because no display appears, it exits with a failure, and the unit starts getty on that VT instead, so a user is never left at a dead screen.

vtium runs without systemd and without logind as well: an inittab line, an init script or a supervisor starts it where it started a getty, and a person can start it by hand. Each place vtium touches systemd, logind or init, and what it does without them, is listed in [systemd.md](systemd.md).

## Start-up and the VT

### Taking the VT and the display

At start vtium opens its VT and asks the kernel to let it manage switches to and from that VT itself. It then takes the display: it becomes the program allowed to set the display's mode and show frames on it. It reads the keyboards and pointers directly. vtium waits a bounded time for a display; one plugged in later joins the console, and the grid takes its size.

### The session

Once it holds the display, vtium starts the session in the pty: by default the system's login program, and otherwise a command the configuration names. When the session's command exits, vtium starts it again, as getty does after a logout. vtium serves one seat, the group of displays, keyboards and pointers that belong together; most machines have exactly one. Each VT that runs vtium runs its own vtium process with its own session.

### Switching VTs

The figure follows one switch away from vtium's VT and back: who draws on the display, where keys and pointer events go, and the session, which runs throughout.

![A timeline. The session keeps running in its pty the whole time. At first vtium draws on the display and receives keys and the pointer. The user switches away: vtium finishes its frame, gives up the display and stops reading input, and the other VT's owner draws and receives input. The user switches back: vtium takes the display, sets its modes and redraws the whole screen, and receives input again.](figures/concepts-vt-handover.svg)

The other VT's owner can be a desktop, a getty or another vtium.

## Heads: a terminal per display

This section describes the design [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) proposes; none of it is built.

The kernel shows one VT on every display, so vtium today draws one grid mirrored across them all. In the design, vtium keeps its one VT and drives every display from one process as a set of heads. A head is a terminal for one or more displays, with its own inputs, its own look and a stack of virtual consoles, each a session of its own, so a small panel and a large monitor each get a grid sized for them, with a keyboard typing into each.

A head file in `/etc/vtium` declares the heads and an ordered list of rules, read as a firewall reads its rules: each display and each keyboard or pointer walks them from the top, and the first that matches places it in a head, in the focus, which a chord moves between heads, or out of use. A rule names a device by its port, its serial number or its monitor's identity, or more weakly by its name. The rules vtium ships put every display in one head and every keyboard and pointer in the focus, by the input classes udev gives each device, and leave media keys, joysticks and the like closed; that is the console as it is today, and a user adds rules above them. Like a firewall, the file has a default policy for what its rules don't mention: under `default = allow`, the shipped rules' policy, a keyboard no rule names follows the focus, and under `default = deny` it is ignored and each head has only the rights the file grants it, as a kiosk needs. [heads.md](heads.md) shows the head file in its scenarios, from the default to a kiosk and several people working at once on one machine, each with a complete file.

With no file, vtium runs its compiled-in configuration and writes nothing; a broken one is reported on the screen and in the log, left as it is, and replaced in memory by the shipped rules, unless it declares `default = deny`, when it lets no keyboard type until it is fixed from outside vtium. Whatever the rules say, a keyboard that reaches no lit head follows the focus, and `Ctrl-Alt-F<n>` for the VT already shown moves the keyboard that pressed it to the focus until the next boot.

vtiumctl writes the file for whoever prefers not to, from the console or a desktop terminal, with or without privilege: it lists every device with a rule ready to paste, asks which keyboard belongs to which display, reports what the running console did, and checks a candidate before it is installed.

## Fonts, generated at run time

This section describes the design ADR-1200 and ADR-1301 decide (accepted 2026-10-28, vtium/vtium#145), with the preview of vtium/vtium#120 and the guided builder of vtium/vtium#121. The helper, the console's per-boot store, the built-in fonts and switching are built; preview and new are proposed.

The console parses no font. A **frit** names a font's sources, and the **helper**, vtium-vitrify's serve command in a sandbox, reads them, rasterises them and sends their glyphs at the console's cell, ready-to-draw bitmaps with their map from Unicode characters to glyphs and their metrics. The console checks each answer, holds the glyphs in memory for the boot and draws them. Nothing is prerendered, and no glyph outlives the boot.

The figure follows a font from choosing it to the console. Each command is vtium-vitrify's. vtium, `serve` and `set` are built; the rest is proposed and drawn dashed.

```mermaid
flowchart LR
    P["preview<br>try many fonts<br>(the user)"] -.-> R
    W["new: the wizard<br>or a template<br>(the user)"] -.-> R
    H["by hand"] -.-> R
    R["frit<br>names the sources<br>(written by root)"] --> HE["serve, the helper<br>sandboxed,<br>never root"]
    HE -->|"glyphs, checked"| ST[("the per-boot store<br>in vtium's memory")]
    ST --> V["vtium"]
    B["built-in fonts<br>Unifont, Unscii-8"] -->|"the first frame<br>and the fallback"| V
    S["set<br>(the user,<br>in the session)"] -->|"OSC 50"| V
    classDef step fill:#2d7d9a,color:#ffffff,stroke:#94a3b8
    classDef core fill:#7c3aed,color:#ffffff,stroke:#94a3b8
    classDef store fill:#2d8e5e,color:#ffffff,stroke:#94a3b8
    classDef input fill:#fbbf24,color:#1a1a1a,stroke:#94a3b8
    classDef user fill:#475569,color:#ffffff,stroke:#94a3b8
    classDef proposed fill:#475569,color:#ffffff,stroke:#94a3b8,stroke-dasharray:5 3
    class HE step
    class V core
    class ST,B store
    class R input
    class S user
    class P,W,H proposed
```

### Try many, keep one

Someone choosing a font tries many and keeps one. vtium-vitrify's preview command draws a sample of a font as the helper would draw it, at the terminal's own cell size, as a picture where the terminal shows pictures and as a text summary where it does not, and writes nothing.

### The frit

A font is declared as a **frit**, named for the ground glass a glassmaker melts again: a small text file, `/etc/vtium/fonts/NAME.vtfrit`, naming its sources in order, later ones filling characters the earlier ones lack, and the cell it is drawn in. Its sources can be console fonts, font files, the kernel's own font, and outline fonts, color emoji among them, found among the fonts installed on the machine by their family. A frit is written in one of three ways, which give the same file: by hand; by vtium-vitrify's new command, a guided builder that asks about the target display, the size, the coverage and the fallbacks and previews each choice; or from a template that command writes, which reads back and writes out again with nothing lost, so a script or a package can make the builder's choices without its questions. The builder asks for sudo once, at the end, for a command it shows first, which writes the frit. At an interactive terminal the tool opens with a small banner of its name in glass, and never in a script or a pipe (vtium/vtium#112). Later, vtium-vitrify may find installed fonts that match chosen profiles without a frit written for each (vtium/vtium#117, a direction not yet decided).

### Drawing at run time

At start the built-in font draws the first frame at once, in the cell the frit states, and the frit's glyphs replace it, character by character, as the helper sends them: the characters on the screen first, and while nothing is asked, printable Latin and box drawing, then the rest of the frit's coverage. The glyphs live in a per-boot store whose limit follows the machine's memory: the helper's fill stops at it, and only an ask for the screen's characters evicts, the glyphs least recently shown first. The helper starts on the console's first need, exits when idle, and is started again after a crash a bounded number of times a boot.

### The fallback and switching

When the configured font names no frit, and no font a frit can be made from on the fly, or the helper is unavailable, the console draws the built-in fonts and logs why. The built-in fonts are Unifont, and Unscii-8 for cells under 16 pixels, compiled into vtium; they also draw each character a frit lacks.

vtium-vitrify's set command switches the running console among the frits and the built-in fonts through the terminal, with xterm's font control sequence (OSC 50), which vtium takes while its allow-font-ops setting is on, as it is by default. A switch changes the live console only; the configuration decides the font at the next start.

## Using VTium

### The configuration

vtium reads one configuration file, then a file for its seat over it, then its command line over both, once at start. It reads the file the C reference read, with its settings, and adds its own. [config.md](config.md) lists every setting.

In the proposed design of ADR-1401, a head file beside it places each display and input ([Heads](#heads-a-terminal-per-display)).

### Selection and the clipboard

The console has no system clipboard: vtium holds one selection, the text the user last selected with the pointer, and pastes it into the session. A program in the session can set that selection with the standard clipboard control sequence (OSC 52), which passes through ssh, so a program on a remote machine can copy to the console.

vtium-clipboard, proposed in vtium/vtium#113, gives scripts the same copy from the command line, as `pbcopy` does on macOS: it writes its input to the selection through OSC 52 on its own terminal. How it would paste is undecided, since vtium refuses a program's request to read the selection; the choices wait for a decision record.

### Links and prompts

Programs can mark text as a link, and shells can mark their prompts. vtium shows a link's address and copies it for the user to paste, and moves the view from one prompt to the next ([ADR-1002](architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md)).

### Keys

vtium reads the keyboard itself and passes keys to the session, apart from the keys it keeps for itself, such as those that switch VTs. The configuration can bind keys to reboot, power off or suspend the machine; each asks the system to act, so services stop cleanly ([systemd.md](systemd.md)).

### Blanking

After a period with no key press or pointer action (a move, a button or the wheel), vtium turns its displays off, and the next one turns them on; output alone never keeps them on. A program can set the timeout or wake the screen with the Linux console's own sequences, and setting the timeout wakes the screen too, as on the Linux console. A setting, on by default, lets the administrator turn that program control off, and no program can turn the screen off at once (vtium/vtium#79).

From outside the session, `SIGRTMIN+1` blanks the displays at once and `SIGRTMIN+2` wakes them, so a script or a power daemon acts without writing to the session's terminal. `vtium blank --vt N` and `vtium wake --vt N` find the vtium on VT N and send it the signal. Root or the user vtium runs as may send them (vtium/vtium#116).

## How VTium is judged

vtium is judged in tiers, each with its harness in vtium's tree; [AGENTS.md](../AGENTS.md#how-vtium-is-judged) lists them with their commands.

- **Unit tests:** each crate's behaviour.
- **Tier 1, conformance:** byte streams replayed through vtium's terminal, the screen and the replies compared as text with goldens ([tools/conformance](../tools/conformance/README.md)).
- **Capability probes:** each capability vtium claims has a replay that fails without it, run by the same tool.
- **Tier 2, pixels:** the renderer draws known screens into memory, compared pixel for pixel with goldens ([tools/tier2](../tools/tier2/README.md)).
- **Tier 3, the VM:** vtium's install tree on a VT in a QEMU/KVM guest with a real display device, its screens captured and compared with goldens, with size and memory figures ([tools/tier3](../tools/tier3/README.md)).
- **The VT review:** the maintainer judges a build by eye on a real VT, guided step by step ([vt-review.md](vt-review.md)).

The goldens are the judge: a difference from one is a listed departure, a listed pending case, or a failure. Tier 1's corpus and goldens and the capability probes were made by the C reference and are copied into vtium's tree, each file with the commit it came from and its hash. Tier 2's and tier 3's goldens from the C reference are copied the same way, beside vtium's own reviewed goldens for the screens the C reference never drew, and vtium builds tier 3's guest images from its own scripts. No gate, CI job or build runs the C reference's code or reads its checkouts: the C reference is read as the reference for behaviour, and the provenance checks read a checkout, for review, when the copies change.

## What VTium does not do

Some limits are deliberate, each recorded with its reason:

- **It never opens a link.** No browser or other program is ever started from the console; a link's address is only copied ([ADR-1002](architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md)).
- **It never hands the clipboard to a program.** A program's request to read the selection gets no answer, since the answer would go to whatever reads the terminal ([ADR-1000](architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md), `osc.52.read`).
- **It never repeats a window title back.** A title query gets a fixed empty title, so a title a hostile program set cannot be replayed as input ([architecture.md](architecture.md#determinism)).
- **It does not sandbox its unit.** The login session vtium starts would inherit every restriction the unit set ([ADR-1400](architecture/platform/ADR-1400-the-privilege-and-sandbox-model.md)).
