# The C reference, part by part

How the C reference, the aaronsb/kmscon and aaronsb/libtsm forks, was put together, as logic flows: what happened, in what order, on which event, and why. vtium takes these flows, never the C code's structure; C types and functions appear here as citations, not as things to port. For each part the page names the vtium crate and issue that take it on, and whether vtium's plan follows the flow, follows it with a listed departure, or takes a different flow.

Every citation names a path and line at these commits, read with `git -C ../kmscon show origin/main:<path>`:

| Repository | Commit |
|---|---|
| aaronsb/kmscon | `b1f7a7c5df563f3378584db7298753848a7a9a13` |
| aaronsb/libtsm | `eeaf2931985051585bf50979e0c3f588b4a51ef9` |

A citation reads `kmscon/src/seat.c:166-177` or `libtsm/src/tsm/tsm-vte.c:3239`. The kmscon fork's own records are cited by path, as `kmscon/docs/architecture/render/ADR-103-...md:447`; vtium's records are ADR-1000 and up. <!-- adr-cite-ignore: kmscon's records -->

Each part ends with a verdict:

- **Follows:** vtium's plan takes the same flow. A Rust shape for the same flow (an owned guard for a restore, a calloop source for an fd, a `Result` for an error code) is expected and is not flagged.
- **Follows, with a listed departure:** the same flow, with a difference recorded where vtium lists departures (a crate README, an issue, ADR-1001), and the reason given there.
- **Different flow:** vtium's plan changes what happens, in what order or on which event, and no departure list records it yet, or the operator should see it before it is built. Each is marked **Check:**.
- **Not planned:** the C reference had the flow and no vtium issue carries it.

## 1. The process and its main loop

The C reference was one process with one thread and one event loop; extra processes were only the pty's child and the image helper, each forked (`kmscon/src/misc/pty.c:409`, `kmscon/src/image_host.c:260`).

Start-up ran in a fixed order: read the configuration (file, then command line), register the built-in font, text renderer and video backends, load the module plugins, create the loop, register SIGTERM, SIGINT and SIGPIPE, connect D-Bus if built, create the one seat, start it, and run the loop until something asked it to exit (`kmscon/src/main.c:161-201`).

SIGTERM and SIGINT asked the loop to exit; SIGPIPE was taken and ignored, so a write to a closed pty returned an error instead of killing the process (`kmscon/src/main.c:79-87`, `kmscon/src/main.c:131-147`).

D-Bus, when built and when no keyboard layout was configured, followed `localectl`'s layout and changed the keymap at run time (`kmscon/src/main.c:98-119`).

Exit ran in this order: report a start-up error from the VT wait, ask the seat to exit, run the loop for 100 ms more if the seat had started a switch back to the VT it came from, so the kernel's release signal was acknowledged, then free the seat and the app (`kmscon/src/main.c:201-222`).

The loop was epoll over file descriptors, and every other source was built on a descriptor: timers on timerfd, signals on signalfd, idle callbacks on one eventfd counter (`kmscon/src/shl/eloop.c:40-53`, `kmscon/src/shl/eloop.c:469-475`).

One turn of the loop ran the pre-turn hooks, waited in `epoll_wait`, called each ready source's callback in the order epoll returned them, and then ran the post-turn hooks (`kmscon/src/shl/eloop.c:779-848`); nothing in kmscon registered a pre-turn hook.

The post-turn hook was where drawing happened: the terminal registered one, so all the pty output parsed in a turn was drawn once, at the turn's end (`kmscon/src/terminal.c:1956`, `kmscon/src/terminal.c:571-591`).

An idle callback ran on every turn while it stayed registered, mixed in among the descriptor callbacks rather than before or after them (`kmscon/src/shl/eloop.c:469-475`, `kmscon/src/shl/eloop.c:516-545`); the pty used one to keep feeding output it held back, the VT used one to deliver an activation found at start-up, and the DRM backend used one to finish page flips (`kmscon/src/misc/pty.c:602`, `kmscon/src/uterm/vt_real.c:330`, `kmscon/src/video/drm_shared.c:1640`).

Each handled signal got its own signalfd, and the signal was blocked in the calling thread when it was added and never unblocked (`kmscon/src/shl/eloop.c:375-420`, `kmscon/src/shl/eloop.c:444-448`).

Children were reaped centrally: the first child callback registered SIGCHLD, and on each SIGCHLD the loop called `waitpid(-1, WNOHANG)` until none was left and told every child callback each pid, because two callbacks each calling `waitpid` would steal each other's children (`kmscon/src/shl/eloop.c:2217-2250`, `kmscon/src/shl/eloop.c:316-344`).

Exit was a flag the next turn checked; a callback could add or remove any source, the current one included, and recursive dispatch was refused (`kmscon/src/shl/eloop.c:35-38`, `kmscon/src/shl/eloop.c:787-794`, `kmscon/src/shl/eloop.c:867-903`).

**In vtium:** `vtium-platform`'s event loop, vtium/vtium#20, on calloop (kmscon `ADR-304` §3, `kmscon/docs/architecture/fork/ADR-304-vtium-a-full-rust-port-of-kmscon-and-libtsm-minspec-first-judged-by-the-existing-tiers.md:204`). <!-- adr-cite-ignore: kmscon's record -->
The binary's start-up and exit order is vtium/vtium#21.

**Verdict: follows, with listed departures.** The branch `feat/platform-pty` (`crates/vtium-platform/src/event_loop.rs`) keeps one thread and one loop, one signalfd for every handled signal blocked in the thread that adds it, descriptors and timers as sources, and a callback after each turn, which is where the terminal's draw goes. Its README lists the departures:

- The child's exit is watched through a pidfd, so SIGCHLD keeps its default and the loop reaps nothing centrally; the C reference reaped every child on SIGCHLD. Same flow, a Rust shape, not flagged.
- The child's exit reaches the terminal only after a read finds no more output, where the C reference reported the exit from SIGCHLD and closed the pty with output still unread (part 6).

**Not planned:** the extra 100 ms of loop after exit, which let the switch back to the previous VT be acknowledged (`kmscon/src/main.c:203-209`), is in no issue; it belongs with vtium/vtium#19 or #21.

## 2. Seats, sessions and VTs

The process ran one seat (`kmscon/src/main.c:58-71`, `kmscon/src/seat.c:792`); several seats meant several processes, each started for its own seat.

A seat held the input devices, the video devices and their displays, the VT, and a list of sessions; each session held exactly one terminal (`kmscon/src/seat.c:53-108`).

A seat started with one session (`kmscon/src/seat.c:1128`); with `session-control` on, which was off by default and refused on a real VT, keys could open, close and cycle further sessions, up to `session-max`, 50 (`kmscon/src/config.c:1357-1358`, `kmscon/src/seat.c:841-845`, `kmscon/src/seat.c:595-630`).

A new session was linked next to the current one and given every display the seat already had (`kmscon/src/seat.c:1165-1210`).

Switching sessions deactivated the old terminal, moved the seat into or out of the foreground to match the new session, activated the new terminal, and recomputed focus (`kmscon/src/seat.c:298-319`).

Closing the current session picked the next one, switched to it, and then destroyed the closed terminal; a session was unlinked first, so the focus it recomputed never reported focus-in to the closing terminal (`kmscon/src/seat.c:1212-1239`).

The seat's state was two flags. Awake meant the VT was ours and input was read; asleep put input to sleep. Foreground meant the video devices held the display; background put every video to sleep, which released DRM master (`kmscon/src/seat.c:214-296`).

Going to the foreground woke every video device, initialised any that was found while asleep, and activated every display; activating a display turned it on (DPMS), reset the blank timer, added it to every session's terminal, and activated the current terminal (`kmscon/src/seat.c:179-253`).

A terminal had the focus (mode 1004) only while the seat was awake and in the foreground, its session was the current one and in the foreground, and at least one activated display was active; every transition recomputed it at its end, the others losing focus before the current one gained it, so two terminals never held it at once (`kmscon/src/seat.c:125-177`).

The VT backend was chosen at start: libseat when configured and it connected, otherwise the kernel's VT interface, otherwise a fake VT that was always active (`kmscon/src/uterm/vt.c:84-110`).

With the kernel's VT, the C reference opened the tty (its own if stderr was a VT, otherwise the next free one), put it in graphics mode, took VT switching with `VT_PROCESS` and SIGUSR1 to acquire and SIGUSR2 to release, saved the keyboard mode, and set the keyboard to `K_RAW` and then `K_OFF`, so keys reached only evdev (`kmscon/src/uterm/vt_real.c:261-339`, `kmscon/src/uterm/vt_real.c:627-675`).

Anything the tty itself read was flushed, since input came from evdev, and a hang-up on it ended the process (`kmscon/src/uterm/vt_real.c:180-195`, `kmscon/src/seat.c:464-471`).

A switch away started either from the kernel or from kmscon's own key handler, which took Ctrl+Alt+F1 to F12 and the XF86Switch_VT keys and asked the kernel to switch (`kmscon/src/uterm/vt_real.c:484-530`).

On the release signal the C reference checked the VT was still the active one, deactivated the terminal, put the video to sleep and input to sleep, and acknowledged with `VT_RELDISP 1`; if deactivation failed it refused with `VT_RELDISP 0` and stayed (`kmscon/src/uterm/vt_real.c:141-178`, `kmscon/src/seat.c:449-462`).

On the acquire signal it acknowledged with `VT_ACKACQ`, woke input, then the seat woke, went to the foreground and activated the current terminal, which redrew every display (`kmscon/src/uterm/vt_real.c:109-139`, `kmscon/src/seat.c:438-447`, `kmscon/src/terminal.c:1764-1778`).

If the VT was already active at start, activation was delivered from an idle callback on the first turn, not from inside the constructor (`kmscon/src/uterm/vt_real.c:329-337`, `kmscon/src/uterm/vt_real.c:99-107`).

Started on a VT that was not yet active and without `--switchvt`, the seat waited for the acquire signal with the loop running, so signals were served meanwhile; with `--switchvt`, the default, it asked the kernel to switch instead (`kmscon/src/seat.c:1121-1150`, `kmscon/src/uterm/vt_real.c:568-590`, `kmscon/src/config.c:1353`).

Closing the VT restored the saved keyboard mode, `VT_AUTO` and text mode (`kmscon/src/uterm/vt_real.c:363-400`); a setup failure after the open left none of them restored, the defect aaronsb/kmscon#253.

`real_retry`, which retried a pending switch for up to 3 s, had no caller (`kmscon/src/uterm/vt_real.c:532-566`; `uterm_vt_retry` is called nowhere in `kmscon/src`).

With libseat, the seat manager opened devices and reported enable and disable, which drove the same awake and foreground flow; libseat's descriptor was a loop source (`kmscon/src/uterm/vt_libseat.c:136-168`, `kmscon/src/uterm/vt_libseat.c:388-398`).

The C reference retried opening the seat 50 times and each device 20 times, 100 ms apart, with `usleep` on the loop, because udev events can arrive before the seat manager is ready (`kmscon/src/uterm/vt_libseat.c:170-200`, `kmscon/src/uterm/vt_libseat.c:339-361`).

Without libseat, a device was opened directly, read-write and non-blocking (`kmscon/src/uterm/vt.c:128-137`).

Devices were found through udev: the monitor enumerated and then watched the `drm` (`drm_minor`), `graphics` and `input` subsystems, kept those whose `ID_SEAT` matched the seat, and reported each as new or gone (`kmscon/src/uterm/monitor.c:352-428`, `kmscon/src/uterm/monitor.c:534-556`, `kmscon/src/uterm/monitor.c:657-675`).

A DRM `change` event carrying `HOTPLUG` made the seat poll that video device for connectors (`kmscon/src/uterm/monitor.c:442-468`, `kmscon/src/seat.c:726-731`).

A program could hand the display back without a VT switch: the OSC string `setBackground` put its session in the background, which deactivated the terminal and put video and input to sleep, and `setForeground` took it back (`kmscon/src/terminal.c:859-870`, `kmscon/src/seat.c:1241-1302`).

After `dpms-timeout` seconds without a key or pointer event, 600 by default, every activated display was turned off and the terminal deactivated; the next event turned them on and activated it again (`kmscon/src/config.c:1388`, `kmscon/src/seat.c:514-572`, `kmscon/src/seat.c:584-590`).

**In vtium:** the VT, udev and input are vtium/vtium#19 (`vtium-platform`); libseat is vtium/vtium#34; the hand-back, focus and wake-up retry are vtium/vtium#36; the session per seat is vtium/vtium#21 (`vtium`).

**Verdict: follows, with listed departures.** #19 restores the keyboard mode, `KD_TEXT` and `VT_AUTO` on every exit through a guard (aaronsb/kmscon#253), and investigates keys reaching the kernel console. #34 retries libseat from loop timers instead of sleeping. #36 retries a failed DRM wake-up while the session is in the foreground (aaronsb/kmscon#234).

**Not planned:**

- Several sessions per seat (`session-control` and the session keys) have no issue; #21 names one session per seat. The C reference had it off by default.
- The DPMS blank after `dpms-timeout`, on by default at 600 s, has no issue.

## 3. Video

A video device was one DRM card or one fbdev node, opened through the VT layer (libseat or a direct open) by the seat, which also closed it; the video backend never closed the descriptor (`kmscon/src/seat.c:960-1008`, `kmscon/src/seat.c:1079`, `kmscon/src/video/drm_shared.c:1310-1320`).

The seat chose the backend per device: drm3d when `hwaccel` was on, drm2d otherwise, falling back to drm2d on the same descriptor if drm3d failed, and fbdev for framebuffer nodes, skipping a framebuffer that a DRM driver backed (`kmscon/src/seat.c:960-1008`, `kmscon/src/seat.c:911-951`); `hwaccel` was off by default because kmscon never turned hardware acceleration on by itself (`kmscon/src/config.c:1413`, `kmscon/docs/architecture/render/ADR-108-images-draw-through-the-renderer-hwaccel-selects-and-hwaccel-stays-off-by-default.md:62`). <!-- adr-cite-ignore: kmscon's records -->

The text renderer followed the display: gltex on a display with GL, bbulk on every other (`kmscon/src/terminal.c:1252`).

A video's displays were its connected connectors with at least one mode; a hot-plug scan marked every display unavailable, walked the connectors, bound a display for each new one, unbound every display still unavailable, dropped DRM master if none was left, and ran one modeset if anything was new (`kmscon/src/video/drm_shared.c:1455-1552`).

A display's mode was, in order: the console's current mode when `use-original-mode` was set, a mode matching a configured `mode=WxH[@rate]` for that connector, or the connector's preferred mode, or its first mode when none was preferred (`kmscon/src/video/drm_shared.c:1338-1389`); `use-original-mode` was off by default (`kmscon/src/config.c:1416`).

A modeset prepared every display into one atomic request, tried it as a test-only commit, and on failure retried once with every display at its default mode; the legacy path, for a device without atomic or universal planes, set each CRTC in turn (`kmscon/src/video/drm_shared.c:861-937`, `kmscon/src/video/drm_shared.c:939-1015`, `kmscon/src/video/drm_shared.c:1269-1283`).

drm2d drew into two dumb buffers per display, XRGB8888, mapped into memory and cleared at creation; the renderer wrote into the back one and a swap flipped to it (`kmscon/src/video/drm2d_video.c:61-104`, `kmscon/src/video/drm2d_video.c:121-145`, `kmscon/src/video/drm2d_video.c:212-225`).

A swap refused while a flip was in flight, then queued a non-blocking atomic commit with a flip event, carrying the frame's damage rectangles as `FB_DAMAGE_CLIPS`, and marked the display as flipping (`kmscon/src/video/drm_shared.c:1028-1087`, `kmscon/src/video/drm_shared.c:627-630`).

When the flip event arrived on the DRM descriptor, the display was marked flipped, and the page-flip hook told the terminal, which drew the frame it owed, if any (`kmscon/src/video/drm_shared.c:1105-1124`, `kmscon/src/video/drm_shared.c:1159-1186`, `kmscon/src/terminal.c:669-690`).

Frames were thus paced by flip completion: no frame was drawn into a buffer the display might still scan out, and nothing ticked at the refresh rate (part 7).

The refresh rate, used only to bound how long a draw may wait for a write to end, came from the mode's pixel clock and totals (`kmscon/src/video/drm_refresh.h:32-51`).

Before a modeset or a sleep the C reference waited for a pending flip by polling the DRM descriptor for up to 1 s, blocking the loop meanwhile (`kmscon/src/video/drm_shared.c:834-859`, `kmscon/src/video/drm_shared.c:1611-1650`).

Going to sleep (a VT switch away, or the hand-back) waited for flips, cleared the cursor and overlay planes, and dropped DRM master (`kmscon/src/video/drm_shared.c:1571-1595`); drm2d first cleared the front buffer (`kmscon/src/video/drm2d_video.c:244-271`, `kmscon/src/video/drm2d_video.c:306-310`).

Waking took DRM master and ran a hot-plug scan with a modeset; if it failed, as when another process still held master, a 50 ms timer retried up to 20 times and then gave up until the next transition (`kmscon/src/video/drm_shared.c:1554-1569`, `kmscon/src/video/drm2d_video.c:312-324`, `kmscon/src/video/drm_shared.c:1188-1224`, `kmscon/src/video/drm_shared.c:51`).

DPMS was set through the connector's property, and a swap on a display that was not on was refused (`kmscon/src/video/drm_shared.c:724-811`, `kmscon/src/video/drm_shared.c:1067-1068`).

The pointer was drawn on the hardware cursor plane where the display had one, as an I-beam sized to the font, and in software otherwise (`kmscon/src/terminal.c:303-338`, `kmscon/src/video/drm_shared.c:401-514`).

fbdev asked the driver for a virtual height twice the visible one to get two buffers, and swapped by panning; without that it drew straight into the one visible buffer (`kmscon/src/video/fbdev_video.c:132-190`, `kmscon/src/video/fbdev_video.c:374-402`).

fbdev had no flip event, so a timer at the mode's refresh interval (60 Hz when unknown, clamped to 1 to 200 Hz) stood in for it (`kmscon/src/video/fbdev_video.c:47-79`, `kmscon/src/video/fbdev_video.c:250-271`).

drm3d drew through GBM and EGL into GL; it is the GPU path and is out of the minspec build (README "Minimal build", `kmscon/README.md:109`).

`display_swap` dropped the backend's return value, so a failed or busy commit counted as presented (`kmscon/src/video/video.c:305-313`), aaronsb/kmscon#93.

**In vtium:** drm2d and fbdev are vtium/vtium#18 in `vtium-platform`, with mode selection vtium/vtium#5 and syscalls through rustix vtium/vtium#6; drm3d and gltex are vtium/vtium#33; one grid across several displays is vtium/vtium#35; the wake-up retry is vtium/vtium#36.

**Verdict: follows, with listed departures.** #18 lists four: damage clips only with atomic commits, a failed swap that is not counted and whose `-EBUSY` disables the screen (aaronsb/kmscon#93), a power cycle that skips the gone connector (aaronsb/kmscon#237), and tests against a stand-in DRM device.

**Check:**

- vtium/vtium#5 says the C reference defaulted to `use-original-mode=true`. At the commit read here it defaulted to false and opened the preferred mode (`kmscon/src/config.c:1416`, `kmscon/src/video/drm_shared.c:1357-1381`; kmscon commit `55cbdf1e`, 2026-09-29, "video: Fix use_original, and disable it by default"). With no preferred mode, the C reference took the connector's first mode, where #5 takes the largest. #5's departure is smaller than it states; its fallback is new.
- The blocking wait of up to 1 s for a pending flip, before a modeset and before giving up the display on a VT switch, has no planned counterpart. `vtium-platform` states that its loop never blocks (`crates/vtium-platform/src/lib.rs` on `feat/platform-pty`), so the switch away needs its own flow: acknowledge the release only once the flip completes or a timer expires.

## 4. Input

The seat owned one input object that merged every keyboard and pointer of the seat (`kmscon/src/input/input.h:26-31`, `kmscon/src/seat.c:757`).

A device was taken when it had keys, or, with `mouse` on (the default), relative or absolute motion with buttons or touch (`kmscon/src/input/input.c:377-389`, `kmscon/src/config.c:1385`).

A device was opened through libseat or directly, non-blocking, and its descriptor became a loop source (`kmscon/src/input/input.c:152-174`).

Each readable event drained the descriptor 16 events at a time and handed each event on by type: keys to xkbcommon and the pointer, motion and sync to the pointer (`kmscon/src/input/input.c:56-113`).

A hang-up kept the device and closed it, since a seat manager revokes devices on a switch; it was reopened on the next wake, and only udev's remove event freed it (`kmscon/src/input/input.c:78-92`, `kmscon/src/input/input.c:205-212`).

Sleeping closed every device and stopped key repeat; waking reopened them; both counted, so the VT's wake and the seat's wake had to agree before devices opened (`kmscon/src/input/input.c:193-203`, `kmscon/src/input/input.c:721-770`).

The keymap came from `xkb-keymap` (a file) or from model, layout, variant and options, with a built-in fallback keymap when compilation failed (`kmscon/src/input/input_uxkb.c:89-156`); without any configured, D-Bus's `localectl` layout was used at start and followed at run time (`kmscon/src/main.c:108-119`).

Compose came from `xkb-compose-file` or the locale's table, the locale read from the environment as glibc would, without setting it (`kmscon/src/input/input_uxkb.c:158-189`, `kmscon/src/seat.c:639-657`).

Each keyboard kept its own xkb state, so modifiers held on one keyboard did not apply to another (`kmscon/src/input/input_uxkb.c:214-241`).

A key event went through xkb and compose in a fixed order: read the keysyms, feed compose on a press, update the xkb state, update the keyboard's LEDs if they changed, drop the event while composing or after a cancel, start or stop repeat, and dispatch only presses (`kmscon/src/input/input_uxkb.c:431-545`).

The kernel's own autorepeat was ignored; kmscon repeated a held key from a timer per device, after `xkb-repeat-delay` (250 ms) and then every `xkb-repeat-rate` milliseconds (50), only for keys the keymap marks as repeating, and marked the event as a repeat (`kmscon/src/input/input_uxkb.c:442-443`, `kmscon/src/input/input_uxkb.c:381-429`, `kmscon/src/input/input_uxkb.c:205-212`, `kmscon/src/config.c:1383-1384`).

The event carried the keycode, the modifiers, the keysyms, their codepoints, an ASCII keysym for the shortcut keys, a repeat flag, and a `handled` flag (`kmscon/src/input/input.h:55-65`).

Key callbacks ran in registration order, each seeing whether an earlier one had handled the key (`kmscon/src/shl/hook.h:112-126`); the order was the VT's switch keys, then the seat's session keys, then each session's terminal (`kmscon/src/uterm/vt_real.c:705`, `kmscon/src/seat.c:812`, `kmscon/src/seat.c:1189`).

The VT took Ctrl+Alt+F1 to F12 and the XF86Switch_VT keys (`kmscon/src/uterm/vt_real.c:484-530`); with libseat it asked the seat manager to switch session (`kmscon/src/uterm/vt_libseat.c:418-446`).

The seat took the session keys (`grab-session-next`, `-prev`, `-close`, `grab-terminal-new`, off unless `session-control`) and `grab-reboot`, off unless set, and every key reset the blank timer (`kmscon/src/seat.c:584-637`).

The terminal, only when awake, opened, in the foreground and not handled, cleared the selection, stopped an asciicast, restarted the cursor blink, and then matched its own grabs in order: scrollback up and down by a line or a page, zoom in and out, and rotate either way, each redrawing (`kmscon/src/terminal.c:1357-1419`).

After the grabs, a repeat was dropped while the program had DECARM off, so held keys still scrolled and zoomed (`kmscon/src/terminal.c:1421-1425`).

A key with several keysyms was dropped (`kmscon/src/terminal.c:1427-1431`); the rest went to the vte's key encoder, and a key that produced bytes reset the scrollback view to the bottom and redrew (`kmscon/src/terminal.c:1433-1438`).

The pointer accumulated relative motion, or scaled absolute motion, into a position clamped to the screen, and reported motion, buttons with double clicks, the wheel, and a sync at the end of each evdev frame; a timer hid it after a while without motion (`kmscon/src/input/input_pointer.c:26-60`, `kmscon/src/input/input.c:471-479`).

With a mouse mode on, the terminal forwarded every pointer event to the vte as a report, motion with a held button as button plus 32 and the wheel as buttons 4 and 5 (`kmscon/src/terminal.c:1467-1507`, `kmscon/src/terminal.c:1623-1627`).

With no mouse mode, the left button selected (a double click selected a word), releasing copied into a buffer of kmscon's own, the middle button pasted it through the vte, the right button cleared it, the wheel scrolled three lines, and Ctrl with the wheel zoomed (`kmscon/src/terminal.c:1509-1542`, `kmscon/src/terminal.c:1629-1660`).

Keyboard LEDs followed the xkb state, and a program could set them through DECLL, routed to every keyboard of the seat (`kmscon/src/input/input_uxkb.c:260-316`, `kmscon/src/terminal.c:882-888`, `kmscon/src/seat.c:1317-1324`).

**In vtium:** evdev, xkbcommon with compose, and the VT switch keys are vtium/vtium#19 (`vtium-platform`); key and mouse encoding and DECARM are vtium/vtium#12 (`vtium-term`); the terminal's grabs, zoom, the hardware pointer and the blink are vtium/vtium#21 (`vtium`); the pointer drawn in software and the four orientations are vtium/vtium#16; selection is vtium/vtium#11; the kitty keyboard protocol is vtium/vtium#42.

**Verdict: follows.** No issue records a different input flow. #19 adds one investigation: keys reaching the kernel console after a compositor exits.

**Not planned:** the key order (VT keys, then seat keys, then the terminal's grabs, then DECARM, then the encoder) is stated in no issue; #19 and #21 together need it. The session keys and `grab-reboot` follow the missing session control (part 2). The `localectl` layout through D-Bus is out of minspec, as in the C minimal build (`kmscon/README.md:114`), and in no later issue.

## 5. The terminal

The terminal was split across two places: libtsm held the terminal's state and its parser with no I/O, and kmscon's terminal layer connected it to a pty, the displays, input and timers (`kmscon/src/terminal.c:96-160`).

Each session's terminal owned one libtsm screen, one parser bound to that screen, one pty and one font, and a list of per-display screens, each with its own renderer (`kmscon/src/terminal.c:1884-1960`, `kmscon/src/terminal.c:66-83`).

Bytes reached the parser in bounded calls: each call decoded UTF-8 (or 7-bit or 8-bit input), fed one codepoint at a time to the state machine, and stopped early when its image work (2^24 pixels) was spent, when an image decode paused input, or right after the reset that ends a synchronized frame; the caller learned how many bytes were taken (`libtsm/src/tsm/tsm-vte.c:3660-3687`, `libtsm/src/tsm/tsm-vte.c:3689-3744`).

While a kitty image request was out to the host, a bounded call took nothing (`libtsm/src/tsm/tsm-vte.c:3736-3744`).

The state machine was Paul Williams' vt100.net DEC parser, written from scratch: CAN, SUB and the C1 controls acted from any state; every transition ran the old state's exit action, the transition's action and the new state's entry action, so a string's end handler ran on leaving the string state (`libtsm/src/tsm/tsm-vte.c:36-47`, `libtsm/src/tsm/tsm-vte.c:3238-3273`, `libtsm/src/tsm/tsm-vte.c:3280-3298`).

An action either printed a character through the charset mapping into the screen, executed a control, collected a parameter or intermediate, or dispatched a complete ESC, CSI, DCS, OSC or APC sequence (`libtsm/src/tsm/tsm-vte.c:3173-3236`).

Collection was bounded: 16 CSI parameters with colon sub-parameters, an OSC string of 127 bytes (`OSC_MAX_LEN`, 128 with the NUL), and XTGETTCAP at 16 names with a reply of at most 4096 bytes (`libtsm/src/tsm/tsm-vte-int.h:25-37`, `libtsm/src/tsm/tsm-vte.c:131`).

Every mode was one row of a registry, from which set, reset, DECRQM replies, resets and a generated reference page all expanded (`libtsm/src/tsm/tsm-modes.def`; `kmscon/docs/architecture/terminal/ADR-201-one-mode-registry-in-libtsm-and-synchronized-output-mode-2026.md:83`). <!-- adr-cite-ignore: kmscon's records -->

Replies went out through one write callback, which kmscon pointed at the pty's queue (`libtsm/src/tsm/tsm-vte.c:918-944`, `kmscon/src/terminal.c:1877-1882`); a reply generated while parsing was marked as a reply by the pty, so the reply bound applied (part 6).

The parser called the host back for an OSC it did not handle (kmscon took only `setBackground` and `setForeground`), for BEL (a bell on the VT, only with `bell` on), for DECLL (the keyboard LEDs), and for a change of mouse mode (which cleared the selection) (`kmscon/src/terminal.c:859-897`, `kmscon/src/config.c:1372`).

The screen held a main and an alternate line array, margins, tab stops at every eighth column, and a scrollback list of `sb-size` lines, 1,000 by default (`libtsm/src/tsm/tsm-screen.c:1170-1191`, `kmscon/src/terminal.c:1915`, `kmscon/src/config.c:1371`).

A line scrolled off the top of the main screen's scroll region went into the scrollback, the oldest line leaving when full; on the alternate screen it was cleared and reused (`libtsm/src/tsm/tsm-screen.c:358-405`, `libtsm/src/tsm/tsm-screen.c:274-315`).

A line leaving the scrollback for good dropped its image placements' top row, and placements taller than one row moved to the next line (`libtsm/src/tsm/tsm-screen.c:303-307`).

Change tracking was by age: a counter advanced on each change, each cell and each line kept the age it last changed at, and the screen kept one age that marked everything when set; any scroll, a region scroll included, set the screen's age (`libtsm/src/tsm/libtsm-int.h:173-190`, `libtsm/src/tsm/tsm-screen.c:363-373`, `libtsm/src/tsm/tsm-screen.c:274-280`).

A renderer asked for a frame since the age it last drew; the frame gave every visible cell with its colours resolved, its cluster key and whether it changed, and the cursor as a cell of its own; the frame was valid only until the next call (`libtsm/src/tsm/libtsm.h:380-440`, `libtsm/src/tsm/libtsm.h:520-545`).

A turn of output with no visible change drew nothing, because the terminal first asked whether anything changed since that display's age (`kmscon/src/terminal.c:571-591`, `libtsm/src/tsm/libtsm.h:533-545`).

The terminal layer drew at the end of each loop turn in which output was parsed, for each display whose frame changed, unless synchronized output held the frame or the write was still arriving (part 7) (`kmscon/src/terminal.c:571-591`).

A new size, from a display added or removed, a font change or a zoom, picked the grid by `multi-monitor` (`scaled` by default: the smallest grid that fits every display), resized each renderer, gave libtsm the cell size and the new grid, set the pty's window size, and redrew (`kmscon/src/terminal.c:1107-1170`, `kmscon/src/config.c:1418`).

Opening the terminal hard-reset the parser, opened the pty at the grid's size, wrote `/etc/issue` if configured, and redrew (`kmscon/src/terminal.c:1711-1737`).

Activating the terminal (a VT switch back, a session switch, an unblank) started the blink timers, opened the terminal if it was closed, resumed image playback, and redrew every display; deactivating gave back the renderers' scaled pictures and stopped the timers (`kmscon/src/terminal.c:1764-1795`).

**In vtium:** the parser is `vtium-parse`, vtium/vtium#9; the cells, lines and frames are `vtium-term`, vtium/vtium#10 and ADR-1001; the screen, scrollback and Unicode are vtium/vtium#11; modes, SGR, replies, and key and mouse encoding are vtium/vtium#12; the wiring of terminal, pty, displays and timers is `vtium`, vtium/vtium#21.

**Verdict: follows, with listed departures.** `crates/vtium-parse/README.md` lists the parser's departures (a string acts only when its terminator completes it, intermediates kept in order, empty parameters omitted, C1 controls inert in UTF-8, and others), each with its specification. ADR-1001 and `crates/vtium-term/README.md` list the model's: 8-byte cells with per-line style, link and cluster tables in place of 32-byte cells and a screen-wide cluster table; change ages per screen position, row and screen in place of per cell and per line; a region scroll that marks only its rows (aaronsb/libtsm#34); the default background marked in the frame (aaronsb/libtsm#35); padding in the default style (aaronsb/libtsm#47). #12 lists its own (DA1, DECSTR's mouse reset kept, DECSC and the cursor style, the 1049 erase).

The change-tracking flow is the same: each display keeps the age it drew at, and a frame says what changed since then. The storage of ages and clusters is a Rust shape. Images anchor by a `LineId` the store holds, where the C reference listed placements on the line itself; the flow, a placement travelling with its line into the scrollback and back, is the same.

**Check:** a line on the main screen entered the scrollback whenever it scrolled off the top of a scroll region, whatever the top margin (`libtsm/src/tsm/tsm-screen.c:388-399`); xterm saves only when the top margin is the first row. No vtium issue or departure list says which vtium follows.

## 6. The pty

The pty was opened non-blocking and close-on-exec, its descriptor made an edge-triggered loop source, a child callback registered, and the child forked (`kmscon/src/misc/pty.c:785-828`).

The child cleared its signal mask and reset every handler, became a session leader, opened the pty's other end as its controlling terminal, set `VERASE` to DEL or backspace by `backspace-delete`, set the window size, and made the pty its standard input, output and error (`kmscon/src/misc/pty.c:304-397`).

It then reset the environment when `reset-env` was on (the default), set `TERM` (`term`, by default the build's `term` option, `kmscon`; `vt220` when unset), `COLORTERM=truecolor`, `XDG_SEAT`, `XDG_VTNR` and `TERM_SESSION_TYPE`, and ran the configured `login` command, `/bin/login -p` by default (`kmscon/src/misc/pty.c:252-302`, `kmscon/src/terminal.c:1940-1942`, `kmscon/src/config.c:1365-1370`, `kmscon/meson.options:51-52`).

Reading took up to 16 KiB at a time, at most 50 reads per readiness event, then re-armed the edge so the next turn read on (`kmscon/src/misc/pty.c:50`, `kmscon/src/misc/pty.c:620-654`).

Each read went to the terminal's input callback, which might take only part of it; the rest was held, nothing more was read, and an idle callback handed the rest over on later turns, so input, drawing and VT switches ran between the parts (`kmscon/src/misc/pty.c:589-618`, `kmscon/src/misc/pty.c:656-667`).

A pause, used while an image decode was out, stopped reading and feeding until resumed (`kmscon/src/misc/pty.c:669-705`, `kmscon/src/terminal_image.c:88-122`).

Writes went straight to the pty when nothing was queued; what the child did not take was queued, and the descriptor watched for writability until the queue drained (`kmscon/src/misc/pty.c:862-890`, `kmscon/src/misc/pty.c:521-550`).

The queue kept the user's bytes (keys, pastes, pointer reports) apart from replies, in runs in order; user bytes were queued whole and never stopped reading (`kmscon/src/misc/pty.c:55-77`, `kmscon/src/misc/pty.c:474-517`).

Replies stopped reading at 64 KiB queued and resumed below 16 KiB, so a child that queried without reading stalled itself; a reply that would pass 256 KiB, reachable only within one callback call, was dropped with the rest of that call's replies (`kmscon/src/misc/pty.c:55-70`, `kmscon/src/misc/pty.c:479-484`, `kmscon/src/misc/pty.c:542-548`).

The window size was set with `TIOCSWINSZ` when the grid or the cell size changed, with the text area in pixels (`kmscon/src/misc/pty.c:941-964`).

A hang-up on the pty was ignored, since `login` hangs up its tty before it runs; the child was tracked by its pid alone (`kmscon/src/misc/pty.c:707-735`).

When the child exited, the terminal closed: the image request was cancelled, the pty closed, its queue flushed so nothing reached the next child, and the parser hard-reset on the next open (`kmscon/src/misc/pty.c:737-768`, `kmscon/src/terminal.c:1740-1748`, `kmscon/src/misc/pty.c:830-848`, `kmscon/src/terminal.c:1719`).

Then the terminal opened again with a new child, unless `oneshot` was set or the child had died within 2 s of its last start five times running; in those cases kmscon exited (`kmscon/src/misc/pty.c:52-53`, `kmscon/src/misc/pty.c:747-767`, `kmscon/src/terminal.c:1856-1868`).

Output still unread when the child exited was lost with the closed pty (`kmscon/src/misc/pty.c:830-848`).

**In vtium:** the pty is vtium/vtium#20 in `vtium-platform`, built on the branch `feat/platform-pty`; the command, environment and respawn policy are vtium/vtium#21.

**Verdict: follows, with listed departures.** The branch's README keeps the read size, the reads per turn, the reply bounds, the held remainder fed on later turns, the pause, the queue's runs, and the flushed queue on reopen, each cited to `pty.c`. It lists three departures: the consumer stops at the reply that fills the queue, so no reply is dropped and the result does not depend on how reads split the stream; pointer motion is coalesced for a child that does not read (aaronsb/kmscon#247); and the exit is reported only after the child's last output is read.

**Check:** with `reset-env` the C reference ran `/bin/login -p`, without it `/bin/login` (`kmscon/src/misc/pty.c:261-272`). The branch's README names `/bin/login` as the default. Which argv vtium uses for each setting belongs in #21.

**Not planned:** the exit of the whole process after five quick restarts or under `oneshot` (`kmscon/src/terminal.c:1856-1868`) is assigned to #21 by the branch's README, and #21's body does not yet list it.
