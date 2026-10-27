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
