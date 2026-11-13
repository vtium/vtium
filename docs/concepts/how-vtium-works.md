# How vtium works

This page shows what runs when vtium holds a VT. It is written for a reader who wants to review the system as a whole. It names the processes, their threads, the crates each one runs, and the channels between them. Then it follows the VT and a session through their states. Last, it follows one key and one byte of output through the crates. Each section links the page that holds the details.

A VT, or virtual terminal, is one of the text screens you switch between with Ctrl+Alt and a function key. vtium runs on a VT in place of getty, the program that normally shows the login prompt there.

## Processes, threads and channels

The figure shows every process that takes part when vtium runs. The color of each box says which user the process runs as. Each arrow is a channel, and its label says what crosses it.

![The processes and their channels. At the top, outside vtium, are the system's services: udev, which reports each device that comes or goes; seatd or logind, reached through libseat in the libseat build; the D-Bus services logind, systemd and localed, in the logind build; and systemd as PID 1, which starts vtium@ttyN and keeps the fd store. On the left, the kernel: the VT at /dev/ttyN, joined to vtium by mode ioctls and the signals SIGUSR1 and SIGUSR2; the KMS/DRM card or fbdev, joined by buffers and page flips; the evdev devices, which send key and pointer events; and /dev/kmsg, which sends the kernel's messages. In the middle, vtium, one process per VT, running as root. Inside it are the event loop on the main thread, which serves the VT, the devices, the ptys, the image helper and the timers, and runs the crates vtium, vtium-platform, vtium-term, vtium-parse, vtium-settings, vtium-devices, vtium-font and, in the images build, vtium-image; a render thread for each head, which composes each frame in the display's buffer with vtium-render and passes frames to and from the loop; and the font helper's writer, reader and reaper threads, which pass glyphs to the loop. systemd and vtium exchange READY=1 and the pty masters, in and out of the store. To the right of vtium, vtium hold, which runs as root and keeps the pty masters over a socket where systemd's store is missing; vtium blank and vtium wake, which send SIGRTMIN+1 or SIGRTMIN+2; and vtium-image, the image helper, which runs as the session's user in a sandbox, one for each console in the images build, and talks to vtium over a socket pair of requests, replies and memory files. It forks a decode worker for each request. Below vtium, the pty, a kernel device, carries bytes both ways and the window size; vtium-config serve, the font helper, runs as nobody in a sandbox, one for each font set, with the crates vtium-config and vtium-font, and takes requests on a pipe and sends glyph sets back. At the bottom, running as the user who logs in, /bin/login, vtium's child, uses the pty as the session's terminal and starts the user's shell and its programs.](../figures/concepts-works-processes.svg)

### The console process

vtium runs one process for each VT it holds. The process runs as root for its whole life. It needs root to open the VT, take the display, read the keyboards directly and start the login program. [Security](security.md) explains what root means here, and which parts run with fewer rights.

The process has one main thread, which runs the event loop. The loop waits on every file descriptor vtium holds. Among them are the VT's signals, the display cards, the input devices, udev's monitor, each session's pty and each image helper's socket. It also runs vtium's timers, such as the key repeat and the blank. After each wake-up, it handles the VT's events first, then the devices', then the input, then what each session's program asked for. Last, it draws.

A head is a terminal that vtium draws on one display or on several displays together. Each head has its own render thread. The thread composes the head's frames, so that a slow redraw on one head delays neither another head nor the input. The loop hands the thread a frame to draw and a buffer that the display lends. The thread draws the frame straight into that buffer and hands it back. Only the loop talks to the display. See [Heads](heads.md) for how vtium places displays and keyboards in heads.

Each running font helper adds three threads to the console. One writes the requests, one reads the answers and wakes the loop, and one waits for the helper to exit. So a slow or hung helper never stalls the loop.

The table lists the crates and where each one runs. [The architecture page](../architecture.md) shows how they depend on each other.

| Crate | Runs in | What it does |
|---|---|---|
| vtium | the console, on the main thread | The binary: the heads, their sessions, the key order, and each turn of the loop |
| vtium-platform | the console, on the main thread | The kernel and the system: the VT, the displays, the input, the pty, udev, the event loop, and the image helper's host |
| vtium-parse | the console, on the main thread | Turns the bytes a program writes into actions |
| vtium-term | the console, on the main thread | The terminal's state: the screen, the cells, the scrollback, the modes and the replies |
| vtium-render | the console, on each render thread | Composes a frame into a buffer, on the CPU or, with the `gpu` feature, on the GPU |
| vtium-font | the console and the font helper | The built-in fonts and the format the glyphs travel in |
| vtium-settings | the console and vtium-config | The keys of vtium.conf and its sections, the heads and the rules |
| vtium-devices | the console and vtium-config | Describes each display and input device so that the rules can match it |
| vtium-image | the console and the image helper, with the `images` feature | The image store and the packets to and from the image helper |
| vtium-config | the font helper, and the configuration tool | Reads a font set's fonts and draws their glyphs |
| vtium-image-helper | the image helper | Opens the files a program names and decodes pictures |

### The kernel's devices

vtium talks to five kinds of kernel device. Each is a file under `/dev`.

- **The VT.** vtium puts its VT in graphics mode and turns off the kernel's own keyboard handling there. It asks the kernel to signal each switch. SIGUSR1 says the VT is shown, and SIGUSR2 says another VT asks for the screen.
- **The display.** [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern Linux graphics interface, lets vtium set a display's mode and show a buffer of pixels on it. vtium draws into buffers in memory and asks the card to show each one. On a machine with only a framebuffer device, fbdev, vtium draws into the framebuffer.
- **The input.** [evdev](https://en.wikipedia.org/wiki/Evdev) is the kernel's interface to input devices. vtium reads each keyboard and pointer from its own evdev device. libxkbcommon turns each key code into a key in the system's keyboard layout.
- **The kernel's messages.** vtium reads `/dev/kmsg` so that it can show the kernel's messages on screen.
- **The pty.** A pty, or pseudo-terminal, is a pair of kernel devices that acts as a terminal. vtium holds one end, the master. The session's programs use the other end as their terminal. Bytes cross it in both directions, and vtium sets its window size.

### The helpers

vtium starts two helper programs. Each runs with fewer rights than the console, in a sandbox, and the console checks everything it sends back. [Security](security.md#root-for-the-console-fewer-rights-for-the-helpers) describes both sandboxes.

The font helper is `vtium-config serve`. It reads the fonts of a font set and draws their glyphs. A font set, or frit, is a small file that names fonts in order. vtium starts one font helper for each font set in use, on the first need for its glyphs. Every head that draws that font set shares it. The helper runs as `nobody`, and it exits after 60 seconds with no work. vtium writes requests to the helper's standard input, and the helper answers on its standard output with sets of glyphs. An ask names the characters on screen that vtium lacks. A fill asks for the rest of the font set, a batch at a time, and a stop ends a fill. A cell query asks for the font set's own cell size. See [Fonts, generated at run time](fonts.md) for details.

The image helper is `vtium-image`. It is part of a vtium built with the `images` feature. Each console has its own host for it, and the host starts the helper at the first image request. The helper runs as the user who is logged in on that console. Before anyone logs in, it runs as the account `vtium-image` and reads no files. vtium talks to it over a pair of connected sockets that keep each message whole. A request carries the picture's bytes beside it in a sealed memory file, and the reply carries the decoded pixels the same way. The helper forks a worker for each request. The worker opens what the request names, locks itself in, and then decodes. For video, one long-lived worker decodes every frame. While a request is out, vtium stops reading that console's pty, for at most 2 seconds.

### Keeping sessions through a restart

vtium keeps a copy of each session's pty master outside itself. While a copy stays open, the kernel does not hang the session up. So when vtium crashes or restarts, the session's programs keep running.

On a systemd machine, the copy goes into systemd's file descriptor store. The fd store is a place where systemd keeps a service's open files while the service restarts. vtium sends each master to systemd over the notification socket, the socket a service uses to tell systemd its state. When systemd starts vtium again, it passes the masters back.

Where there is no fd store and vtium runs as root, vtium starts a holder process, `vtium hold`. The holder keeps the masters and hands them to the next vtium on that VT. It talks to vtium over a socket in `/run/vtium/`, which only root can open. A vtium with no keeper, such as one a user starts by hand, keeps nothing, and its sessions end with it.

### The system's services

- **systemd** starts vtium on a VT through the unit `vtium@.service`, in getty's place. vtium tells systemd it is ready once it holds its VT open, before it opens a graphics card. [Deployment and start-up](start-up.md) describes the unit, and [systemd.md](../systemd.md) lists each place vtium touches systemd.
- **udev** reports each display card, framebuffer and input device as it comes and goes. vtium lists them once at its start and then follows udev's reports.
- **seatd or logind, through libseat.** A seat is the set of displays, keyboards and pointers one person uses. In the build with libseat, vtium can ask a seat manager for its devices and for VT switching. Then the manager's enable and disable messages take the place of SIGUSR1 and SIGUSR2. See [Seats and sessions](seats-and-sessions.md) for which machine uses which.
- **D-Bus**, in the build with logind. vtium asks logind to suspend the machine when a key bound to suspend is pressed. In seat mode, it asks systemd to give each login child a scope of its own. It follows localed's signal when the system's keyboard layout changes.
- **vtium blank** and **vtium wake** blank and wake the displays of the vtium on a given VT. They send it the signal SIGRTMIN+1 or SIGRTMIN+2.

## The VT's life

The figure follows vtium's VT from vtium's start to its end. Each rounded box is a state, and each arrow is the event that moves the VT to the next state.

![A state machine. vtium starts: it reads vtium.conf, opens the VT and sends READY=1. Once the VT is open, it enters a group of states that last while vtium runs. In waiting, vtium waits for its VT to be shown. SIGUSR1, the VT being shown, moves it to shown, where vtium draws and reads the input. A switch away, SIGUSR2, moves it to releasing, where the input and every display go to sleep. Once all are asleep, vtium lets go, and the VT is away: another VT is shown, and the sessions run on. A switch back, SIGUSR1, moves it to shown again, with the displays set up again. From any of these states, a reason to end moves vtium to ending. A box lists why vtium ends: with exit status 0, SIGTERM or SIGINT, or oneshot sessions that ended; with exit status 1, the VT hung up, no display in time, a session that cannot start, or sessions that end too fast. In ending, vtium switches back if it switched at its start, leaves the sessions to the keeper, and puts back text mode. Then, under systemd: after exit 0, the unit stops and its sessions hang up; after exit 1 under the open policy, a getty takes the VT through vtium-getty@; after exit 1 under the closed policy, systemd starts vtium again, at most 5 starts in 10 s, and past that the VT has no login. A note says that with libseat, the seat manager's enable and disable take the place of SIGUSR1 and SIGUSR2.](../figures/concepts-works-vt.svg)

### Starting

vtium reads vtium.conf before it opens anything. If the file sets the closed policy, vtium writes a marker file, `/run/vtium/ttyN.closed`. The marker keeps the unit's getty fallback off this VT. Under any other policy, vtium removes the marker. Then vtium finds its keeper, opens its VT and tells systemd it is ready. See [Heads](heads.md#two-policies-open-and-closed) for the two policies.

### Shown, released and shown again

The unit starts vtium with `--no-switchvt`, so vtium waits for its VT to be shown. When the VT is shown, vtium wakes every input device and every display, and draws whole frames. The sessions start once the VT is shown and a display is set up.

When you switch to another VT, the kernel sends SIGUSR2. vtium stops drawing, closes its input devices and puts every display to sleep. Once every display reports that it is asleep, vtium allows the switch. The sessions keep running in their ptys the whole time. When you switch back, the kernel sends SIGUSR1, and vtium sets every display up again and redraws. [Deployment and start-up](start-up.md#switching-vts) shows the same switch on a timeline.

### Ending

vtium ends on SIGTERM or SIGINT, when its VT hangs up, or when no display appears within `display-wait`. It also ends when a session cannot start, or when sessions keep ending as soon as they start under the open policy. As it ends, vtium switches back to the VT that was shown at its start, if it switched away from it. It lets go of the sessions without hanging them up, so the keeper still holds them. Last, it puts the VT back in text mode.

A clean end exits with status 0, and every failure exits with status 1. On a failure, systemd restarts vtium. From systemd 254 on, it also starts the unit's fallback, `vtium-getty@`. Under the open policy, the fallback starts a getty on the VT, which stops vtium's unit. Under the closed policy, the marker stops the fallback, because a getty's prompt would accept any keyboard. So systemd starts vtium again, up to its limit of 5 starts in 10 seconds. Past that limit, the VT has no login prompt.

## A session's life

Each head holds one or more consoles, and each console runs one session. The figure follows one console's session.

![A state machine. A console starts in not started, where it waits for its VT and a display. Once the VT is shown and a display is set up, it moves to running, where vtium's child, login, runs on the pty: the login prompt, then the user's shell. When the child exits, or vtium hangs it up, vtium resets the screen and starts login again on a new pty. When the child exits under oneshot, the console moves to ended and starts nothing more. The sixth exit in a row, each within 2 s of its start, moves it to vtium gives up: under the open policy vtium exits with status 1, and under the closed policy vtium holds the VT with a notice and takes no input. When vtium crashes or restarts, the session moves to kept, where no vtium runs and the keeper, the fd store or vtium hold, holds the pty. When the next vtium takes the pty back, the session is running again. A note says that each console of each head has a session of its own, which moves through these states alone.](../figures/concepts-works-session.svg)

A console starts its session once its VT is shown and one of its head's displays is set up. vtium opens a new pty and starts the login command on it. By default, that command is `/bin/login -p`. The login program asks for a name and a password, and then starts the user's shell. vtium watches only the child it started, so it cannot tell the prompt from the shell.

When the child exits, vtium resets the console's screen and starts the login command again. So a logout brings back the login prompt. An exit within 2 seconds of the child's start is a quick exit. A slower exit starts the count again. After the sixth quick exit in a row, vtium gives up. Under the open policy, vtium exits with status 1, and the unit's getty takes the VT. Under the closed policy, vtium hangs up every session and keeps its VT, with a notice on the screen.

When vtium crashes or restarts, the session keeps running, and its keeper holds its pty. The next vtium on that VT takes each kept pty back to the console it ran on. It sends the program a new window size, so the program redraws. Only the screen's contents and the scrollback are lost. A session that ended while no vtium ran counts as an exit when vtium takes it back, but never as a quick one.

## One key and one byte of output

The figure follows a key from the keyboard to the session, and a byte of output from a program to the display. It shows which crate handles each step, and which thread runs it.

![Two flows. A key: the keyboard; evdev, the kernel's input event; vtium-platform reads the event, and xkb makes a key; vtium picks the head, then applies the key order; vtium-term encodes the key as bytes; the pty, which the session's program reads. A byte: a program writes to its terminal; the pty, where vtium's side has bytes to read; vtium-platform reads them and hands them on; vtium-parse turns bytes into actions; vtium-term changes the screen's cells; vtium takes the frame and a buffer; vtium-render composes the frame in it, on the head's render thread; vtium gives the buffer back to show; vtium-platform makes a page flip on KMS/DRM, or uses fbdev; the display shows the frame. Every vtium step but vtium-render's runs on vtium's main thread, the event loop.](../figures/concepts-works-flow.svg)

A key starts as an event on an evdev device. vtium-platform reads the events and turns each key code into a key with libxkbcommon. vtium sends the key to a head. A rule in vtium.conf can tie the keyboard to one head, and otherwise the key goes to the active head. Then vtium offers the key to each part that may take it, in a fixed order. The VT's switch keys come first. The power keys, the heads' key chords and vtium's own keys, such as the scroll keys, come next. A key that none of them takes reaches vtium-term, which encodes it as bytes for the program. vtium writes the bytes to the pty.

A byte of output takes the other direction. When the pty has bytes to read, vtium-platform reads them and hands them to the console's terminal. vtium-parse turns the bytes into actions, such as "print this character" or "move the cursor". vtium-term applies each action to the screen. The state that results depends only on the bytes, never on how they were split or how fast they came. See [the architecture page](../architecture.md#determinism) for that rule.

To draw, the loop takes the screen's frame and a buffer that the display lends. The head's render thread composes the frame into the buffer with vtium-render. The loop then gives the buffer back to the display, which shows it. On KMS/DRM, the card flips to the new buffer. The card reports when the frame is on screen, and that report paces the display's next frame.
