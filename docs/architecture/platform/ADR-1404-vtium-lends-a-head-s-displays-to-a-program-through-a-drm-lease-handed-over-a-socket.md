---
contract: adr/v1
kind: decision
verb: add
capability: platform
basis:
  - operator: aaronsb
    level: directed
    paraphrase: true
    said: "Per port, in steps. Step one: setBackground gives up the whole cards the sending head draws on, so every existing program works; other heads on those cards pause. Step two: a vtium socket, named in each session's environment, hands a DRM lease of only the sending head's connectors (with a CRTC and planes) to a program that asks; vtium keeps its own drawing and its hotplug paths off leased objects, and revokes the lease at setForeground. Later, as a separate experiment: vtium answers as a seat manager (the seatd protocol), so an unmodified libseat program receives a lease."
    via: the maintainer, in the steering session, 2026-11-13, relayed by the team lead; the wording is the team lead's
  - operator: aaronsb
    level: directed
    paraphrase: true
    said: "Under a seat manager (logind or seatd), only the lease path acts. vtium ignores a plain setBackground there, logs that it did, and sends no focus report. The whole-card hand-over happens only on the kernel's VT."
    via: the maintainer, in the steering session, 2026-11-13, relayed by the team lead; the wording is the team lead's
  - standard: "Linux v7.2, drivers/gpu/drm/drm_lease.c (the `DOC: drm leasing` comment; validate_lease, which needs at least one CRTC and one connector, and at least one plane when the owner's file has universal planes on; drm_lease_create, which refuses an object another lessee holds with EBUSY; drm_mode_create_lease_ioctl, which refuses a lease from a lessee; drm_mode_revoke_lease_ioctl; drm_lease_filter_crtcs), drm_auth.c (drm_is_current_master_locked, under which a lessee is master only while its owner is the card's master; drm_master_release, which revokes every lease when the owner's file closes; drm_master_open), drm_ioctl.c (CREATE_LEASE, SETCRTC, SETPLANE, PAGE_FLIP and ATOMIC need DRM master), include/uapi/drm/drm_mode.h (struct drm_mode_create_lease), Documentation/gpu/drm-uapi.rst (\"Device Hot-Unplug\": a new lease fails with ENODEV once the card is gone)"
  - upstream: "seatd 0.9.3 (MIT): libseat/libseat.c picks its backend from LIBSEAT_BACKEND; libseat/backend/seatd.c connects to the socket SEATD_SOCK names; include/protocol.h holds the wire protocol of about nine opcodes; seatd/seat.c calls drm_set_master in seat_activate_device and drm_drop_master in seat_deactivate_device. The seatd README promises no stable protocol"
  - standard: "wayland-protocols 1.50, staging/drm-lease/drm-lease-v1.xml: \"The compositor guarantees it will not use the leased DRM objects itself until it sends the finished event\" (lease_fd)"
  - evidence: "The steering session's research on handing back one port, 2026-11-13, read from the kernel and seatd sources above: no mainstream full-screen program runs on a lease fd today (mpv's command line, SDL3's KMSDRM backend, kmscube, Xorg, weston 15, mutter and KWin take a device path or a card index); Vulkan's VK_EXT_acquire_drm_display and libmpv's MPV_RENDER_PARAM_DRM_DISPLAY_V2 take a DRM fd, so a program built on them can run on a lease; a program that opens /dev/dri/cardN while vtium is master gets a file that is not master and cannot set a mode on any connector; on a lease fd, drmSetMaster succeeds and does nothing while the owner is master, and drmDropMaster fails with EINVAL"
  - precedent: ADR-1401
  - precedent: ADR-1000
  - precedent: ADR-1400
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "The configuration key is `allow-display-ops`, on by default, beside allow-font-ops and allow-window-ops."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Only a console a head is showing may send setBackground."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "The work is two cards: first the strings, the key and the hand-over; then the focus reports. The lease socket is a later card of its own."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
status: proposed
date: 2026-11-13
deciders:
  - aaronsb
related: [ADR-1000, ADR-1401, ADR-1400]
---

# ADR-1404: vtium lends a head's displays to a program through a DRM lease handed over a socket

## Summary

- **Decided:** A program in a console that a head shows can ask vtium for that head's displays alone. It asks over a Unix socket whose path vtium puts in each console's environment. vtium answers with a DRM lease: a new file of the card that is DRM master of only the head's connectors, with a CRTC and planes for each. The program sets modes and draws on that file as on a card of its own. Every other head keeps drawing, on the same card or another. vtium keeps its own commits, its placement search and its hotplug handling off every leased object. vtium revokes the lease at setForeground, when the program lets go, and when the console's session ends. The lease path works the same on the kernel's VT and under a seat manager, so it is the only hand-over under logind or seatd. It is the second step of the hand-over. The first step, which gives up whole cards for any program, is ADR-1401 D16. Later, as an experiment of its own, vtium may answer as a seat manager, so that an unmodified libseat program receives a lease.
- **Trades away:** Only a program written for vtium's socket, or one built on an API that takes a DRM fd and wired to the socket, can use the lease path. Today no mainstream full-screen program can. vtium takes on a small protocol and its bookkeeping, and a socket that any local process can reach.
- **One-way?** No while no release ships it. Once a release ships the socket, its wire format is a format that programs are written against, and changing it costs those programs a version step.
- **Probes:**
  - *Confident (what other heads see):* while a program in one head's console draws on that head's monitors, every other head keeps drawing and taking keys. That holds even for a head on another port of the same card.
  - *Not confident (who may ask):* any program whose controlling terminal is the shown console of a head may take that head's displays, and no other program may. A program inside tmux in that console cannot, because tmux gives it a terminal of tmux's own.
- **Inversion:** The decision sits between two ends. One is "a hand-over takes whole cards", which every existing program accepts but which pauses heads that had nothing to do with it. The other is "a hand-over is always per port", which leaves every existing program without a display. vtium does both, the whole cards for any program on the kernel's VT and the lease for a program that asks.

## Context

The kernel gives drawing rights for a whole card. The one open file of a card that may set modes is its DRM master. A program that opens `/dev/dri/cardN` itself while vtium is master gets a file that is not master, and every mode set from it fails with `EACCES`. So a program that is not written for anything else can take a display only by taking the whole card, and setBackground does that on the kernel's VT (ADR-1401 D16). On a machine where one card drives several heads, that pauses heads that did not ask for anything.

Since Linux 4.15 a master can lend some of its objects to a new open file. That loan is a lease. It lends a set of connectors, CRTCs and planes to a new file, which is master of those objects alone. A connector is a port. A CRTC is a scanout engine, which sends a frame to a connector. A plane is a layer that a CRTC combines into the frame. The lessee sees every other object as idle or disconnected. The owner keeps full control of everything, including what it has lent, so the kernel does not stop the owner from drawing over a lessee. A compositor that lends a lease promises to keep off the leased objects, as `wp_drm_lease_v1` says. A lessee is master only while its owner is the card's master. So at a VT switch the lessee's commits fail with `EACCES` and work again after the switch back, and the lease survives the switch. That also holds under a seat manager, since logind and seatd switch master on vtium's own file.

Under a seat manager, giving up a card does not reach a user's program. vtium's session holds the seat's control, logind refuses a second controller in the session, and seatd refuses a second client of a VT-bound seat while vtium is its active client. A lease goes from vtium to the program directly, so neither seat manager stands in its way.

## Decision

### D1: the lease path, and when it acts

- **A program asks; vtium lends the shown head's displays.** A program connects to vtium's socket and asks for a lease. vtium finds the console the program runs in (D2). When a head shows that console, vtium lends that head's displays and pauses that head. When no head shows it, vtium refuses.
- **What a lease holds.** For each card that drives a lit display of the head, vtium creates one lease. A lease holds each of the head's connectors on that card, the CRTC that drives each one now, and that CRTC's primary plane and cursor plane. vtium turns universal planes on for its own file, so the kernel takes only the planes vtium lists.
- **The lease starts from what vtium shows.** vtium leaves each CRTC lit with its last frame when it lends it, so the program starts from a lit display in a known mode. vtium keeps that frame's buffer alive until the lease ends, since the CRTC may still scan it out.
- **One lease at a time for each head.** A second request for a head whose displays are lent is refused as busy. A request for a head that holds no lit display is refused, since there is nothing to lend.
- **The key `allow-display-ops` governs this path too.** With `allow-display-ops = no`, vtium creates no socket and sets no variable in a console's environment, as it reads no setBackground (ADR-1000, "Replies and policy").
- **The path works on the kernel's VT and under a seat manager alike.** Under logind or seatd it is the only hand-over vtium makes (ADR-1000, "Replies and policy").
- **A lent head pauses.** vtium draws nothing on its displays and sends its shown console a focus-out, as at the whole-card hand-over (ADR-1401 D5 and D16). Unlike the whole-card hand-over, vtium keeps delivering the head's keys and pointer events to its console, because the program that asked for the lease can read them from its terminal. The head's console chords still act. When they show another console in the head, vtium ends the lease first (D4).

### D2: the socket and who may use it

- **One socket for each vtium,** in a directory that root owns, such as `/run/vtium/`, named for vtium's VT or seat. vtium puts its path in the variable `VTIUM_DISPLAY_SOCKET` in each console's environment, beside `VTIUM_HEAD` and `VTIUM_CONSOLE` (ADR-1401 D5). The variable reaches a program only through a login that keeps the environment, as those two do.
- **Any local process may connect, and vtium decides which console it is.** vtium reads the caller's process through `SO_PEERPIDFD` (Linux 6.5), or through `SO_PEERCRED` and a check that the process is still the one it read. The caller's controlling terminal must be the pty of one of vtium's consoles. A caller in another terminal, such as an ssh session or a terminal of tmux's own, is refused. vtium records the caller's uid in its log line for each lease.
- **The protocol is small and fixed.** Every message has a fixed header, an opcode and a length, as seatd's has. vtium reads at most one message at a time from each client, refuses a length over the largest message, and closes a client that sends anything else. vtium builds every reply from its own state, never from the bytes a client sent, as the reply rule asks of a terminal reply (docs/architecture.md, "Determinism"). The root process parses nothing else from a client.

### D3: the protocol in outline

The build fixes the opcodes and their layout. This outline names the messages and what each one carries.

| From | Message | What it carries |
|---|---|---|
| the program | hello | the protocol version the program speaks |
| vtium | hello | the version vtium speaks, the lower of the two |
| the program | lease | nothing more: vtium lends the displays of the head that shows the caller's console |
| vtium | lease | for each card, the lease fd as `SCM_RIGHTS`, and the ids of its connectors, CRTCs and planes |
| vtium | refused | a reason: the key is off, no head shows this console, the head is lent already, the head has no lit display, or the card refused the lease |
| vtium | paused | vtium's VT was released, or the seat manager disabled the seat: the lease is held, and every commit fails until resumed |
| vtium | resumed | vtium's VT was acquired again, or the seat enabled: commits work again |
| the program | release | the program is done |
| vtium | revoked | the lease is over; vtium sends it at every end (D4), and then closes the connection |

A program that receives `paused` can stop drawing until `resumed`. The kernel itself tells a lessee nothing at a VT switch, so without these two messages a program learns of it only from a failed commit.

### D4: the end of a lease

- **The lease ends at any of these:**
  - a `release` from the program, or the program closing the socket;
  - a setForeground from the console that holds the lease;
  - the end of that console's session, or the head showing another console;
  - the card going away, or vtium exiting. Closing vtium's own file of the card revokes every lease too, so a crash of vtium frees the displays.
- **At the end, vtium revokes the lease with `REVOKE_LEASE`.** The program's fd stays open but is master of nothing. vtium then confirms through `LIST_LESSEES` that the lease is gone, sends `revoked`, and closes the connection.
- **vtium then takes the displays back** with a mode set on each of them. That mode set resets each connector's color, as every mode set does (ADR-1401 D14). The head then draws again. The head's shown console gets a focus-in (ADR-1401 D5). vtium does not wait for the program to close its fd, since the kernel sends no event when a lessee closes it.

### D5: what vtium keeps off a leased object

- **Drawing.** vtium makes no commit, page flip, cursor update or power change (DPMS) on a leased connector, CRTC or plane. Its render thread for the lent head draws nothing.
- **Placement.** When a hotplug makes vtium place displays on CRTCs again (ADR-1401 D8), it never moves a leased CRTC and never gives one to another display. A display that can be lit only on a leased CRTC stays off until the lease ends, and is then tried again.
- **Turning off what nothing uses.** Any step that turns off CRTCs or planes that no lit display uses skips the leased ones.
- **Hotplug of a leased connector.** When the monitor on a leased connector is unplugged, the lease stays as it is, and the program sees the connector disconnected. vtium treats the head's display as lost for its sessions, as ADR-1401 D8 says. When the monitor comes back, the connector is still lent, so vtium does not light it until the lease ends. A new display that a rule places in a lent head is not added to the lease, since a lease is fixed when it is made. It stays off until the lease ends.
- **The card goes away.** The kernel keeps a lease whose card is unplugged. vtium ends it there as at any other end (D4), so the program receives `revoked`.

### D6: VT switches and the seat

- **At a VT release, vtium holds the lease.** On the kernel's VT, vtium drops master at a VT release as it does today, and the kernel then refuses the lessee's commits too. Under a seat manager, the seat manager drops master on vtium's file at a disable. In both cases vtium sends `paused` and keeps the lease.
- **At a VT acquire, the lease resumes.** vtium sends `resumed`. Its wake-up and its wake-up retry light vtium's own displays and leave the leased ones to the program (ADR-1000, "Replies and policy").
- **Focus follows the VT** as for any head (ADR-1401 D5). The console that holds the lease had its focus-out when the lease began. It gets no focus-in at the acquire while the lease lasts.

### D7: checks the build makes first

The research behind this record read the kernel and seatd sources and tested none of the following. The build checks each one in tier 3's guest, whose virtio-gpu device has two outputs, before vtium depends on it. No agent takes DRM master on the maintainer's machine, so tier 3 is where these run.

1. **A lease beside vtium's own drawing.** vtium lends one connector, its CRTC and its planes, and keeps drawing on the other connector of the same card. A small test lessee draws a frame. Both displays show what their owner drew.
2. **The CRTC lent while lit.** The lessee sees the CRTC's current mode and can commit on it. vtium's framebuffer stays valid until the lessee's first frame replaces it.
3. **What the screen shows after `REVOKE_LEASE`.** The kernel's revoke path turns no display off, so the lessee's last frame is expected to stay until vtium's mode set. The check confirms that or shows otherwise, and D4's mode set covers either answer.
4. **A VT switch with a lease out,** on the kernel's VT, under seatd and under logind. The lessee's commits fail with `EACCES` after the release, the lease survives, and the commits work after the acquire.
5. **The calls a lessee makes on its fd.** `drmSetMaster` succeeds and does nothing while vtium is master, and `drmDropMaster` fails with `EINVAL`. The build's test lessee records both, so the protocol's documentation can warn a program that treats either as fatal.
6. **A path to the lease fd.** Opening `/dev/fd/N` or `/proc/self/fd/N` for a character device opens the device again, as a new file that is not master. If the check confirms it, a program that takes only a device path, such as mpv's `--drm-device`, cannot use a lease, and the protocol's documentation says so.

Two questions from the research no longer bear on vtium, because the whole-card hand-over acts only on the kernel's VT. One is what logind does when it resumes a card whose master another process holds. The other is which user holds the card nodes' access list in seat mode.

### D8: later, vtium as a seat manager

An experiment of its own, after the socket ships, may have vtium answer the seatd protocol. vtium would listen on a socket that `SEATD_SOCK` names, and set `LIBSEAT_BACKEND=seatd`, in the environment of a program started for it. An unmodified libseat program would then receive a lease when it opens the card. Such programs include sway, cage, labwc and weston's libseat launcher. When the program opens input devices, it would receive only the head's own. The experiment starts only after a check that wlroots or weston runs on a lease fd at all, which nobody has confirmed. seatd's README promises no stable protocol, and vtium would also serve the seat's enable and disable messages at VT switches. A record of its own decides whether vtium builds it.

## Consequences

### Positive

- A program that asks gets one head's monitors, and every other head keeps working, even on another port of the same card.
- The hand-over works under logind and seatd, where giving up a card reaches no user's program.
- A crash of vtium, or of the program, frees the displays: the kernel revokes every lease when vtium's file closes, and vtium ends a lease when the program's socket closes.
- A lease survives a VT switch, and the program is told when it pauses and resumes.

### Negative

- No mainstream program uses the socket today. Until one does, the lease path serves programs written for vtium, and programs built on Vulkan's `VK_EXT_acquire_drm_display` or libmpv's DRM display that someone wires to the socket.
- Input is not confined. The program reads keys from its terminal, and a program running as root can still open every keyboard.
- vtium carries lease bookkeeping through its drawing, placement and hotplug paths, and a mistake there draws over a lessee, since the kernel does not stop the owner.
- The root process listens on a socket that any local process can reach. The protocol is fixed and small, and vtium parses nothing else from it (D2).

### Neutral

- The first step, the whole-card hand-over on the kernel's VT, is ADR-1401 D16, and it is built before this record.
- ADR-1400's table of what the stream reaches gains the socket as a channel to the display, behind `allow-display-ops`.
- vtium as a lessee of a desktop's lease, which ADR-1401 D2 leaves to a later decision, is a different feature and this record does not touch it.

## Alternatives Considered

- **Only whole cards.** Every existing program works, but a hand-over pauses every head on the card, and under a seat manager it reaches no user's program. vtium keeps it as the first step on the kernel's VT (ADR-1401 D16) and adds this path beside it.
- **Only leases.** setBackground would stop working for every existing program, because the kernel gives no per-port path to a program that does not accept a lease.
- **The lease handed over by a device path.** A program would open a path that vtium names. Reopening a character device gives a new file that is not master, so the lease cannot travel that way (D7 item 6 checks it).
- **`wp_drm_lease_v1`.** It needs vtium to be a Wayland server, and its clients, such as Monado and Xwayland, expect a full compositor.
- **A socket for each console, owned by the session's user.** vtium does not know a console's user when it starts the login child as root, and a socket file per console adds files to create and remove. Asking the kernel which terminal the caller runs in gives the same answer from one socket.
