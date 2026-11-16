# Security and the system

vtium runs as root and reads everything your programs print. Fonts and images are the exception: a font or image file is parsed only by a separate, locked-down helper process, never by the terminal itself. No program's output can make vtium type into your shell, or read back what you copied. At boot, vtium takes a getty's place, with systemd and logind or without them.

## Helpers in a sandbox

The terminal runs as root in both of its modes. In direct mode it opens the displays and input devices itself, or, in a build with the libseat feature, gets them from seatd where seatd runs. In seat mode it gets them from logind. It needs root to open the VT, take control of the displays, read the input devices, and start `/bin/login`. The terminal's own work, reading and acting on everything your programs print, also happens in that root process. Font and image files are parsed in two helpers instead, because those complex formats are where programs most often have security defects. Neither helper runs as root. The font helper, `vtium-config serve`, runs as the user `nobody` and refuses to run as root. The image helper, `vtium-image`, runs as the user logged in on that console. Before anyone logs in, it reads no files at all.

Before a helper reads anything, it locks itself in. It sets no-new-privileges, installs a seccomp filter on its system calls, and enters a Landlock domain that limits what it may do with files. On a kernel without Landlock, a further seccomp filter takes the domain's place. On armv7 the image helper installs no seccomp filter and relies on Landlock alone. vtium checks every glyph and picture a helper sends back before using it. The root process itself caps how much a program can make it store, is written so that no input can make it panic, and has its output parser fuzz-tested.

Its own configuration gets a check too. vtium reads `/etc/vtium/vtium.conf` only when root owns the file and no one else may write to it; otherwise it runs its built-in configuration and says so on screen.

## The reply rule

Some sequences ask the terminal a question, such as where the cursor is. vtium builds each answer from its own state, fixed text, and numbers, and never copies the program's output into it. A sequence hidden in a file you `cat` can therefore change what your screen shows, but it can never make vtium type a command into your shell.

The clipboard works one way too. A program can copy text into it with OSC 52, but a request to read the clipboard gets no reply. Text you copied leaves the clipboard only when you paste it yourself.

[Security](../concepts/security.md) explains the whole design and where its protection ends. The READMEs of the [font helper](https://github.com/vtium/vtium/blob/main/crates/vtium-config/README.md#the-helper) and the [image helper](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/README.md) describe each sandbox step by step.

## With systemd and logind, and without them

Under systemd, vtium runs from `vtium@.service`, which takes a getty's place at boot on each free VT from 1 to 6. In seat mode, `vtium-seat@.service` holds the seat through a logind session of vtium's own, and vtium gets its displays and input devices through libseat. Either way, `pam_systemd` registers each login with logind on its seat and VT.

Your sessions survive a restart of vtium. A copy of each session's terminal goes to the unit's file-descriptor store, or, without systemd, to a small holder process, `vtium hold`, that only root can reach.

When vtium fails, its unit starts a getty on the VT, so a login prompt returns. The unit skips the getty under a closed policy, a vtium.conf setting under which vtium ignores any keyboard that no rule names. It also skips the getty when starting one would stop another program, such as a display manager. For systemd's rescue and emergency modes, vtium steps aside, and `vtium.enable=0` on the kernel command line turns it off for one boot.

Without systemd, an inittab line, an init script, or a supervisor runs vtium where it ran a getty. [systemd, logind and PID 1](../systemd.md) lists every place vtium touches systemd, and what it does without it.
