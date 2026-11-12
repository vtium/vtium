# Security

This page explains how vtium protects your machine and your sessions, and why it is built the way it is. It is written for you if you are deciding whether to run vtium. It stays with the large blocks of the design, because the details are still settling. Each section ends with the pages that hold the details, and with outside reading on the ideas behind it.

## Two programs you already know

On most Linux machines, two kinds of program put text on your screen.

The first is getty, which shows the login prompt on a virtual terminal (VT), one of the text screens you switch to with Ctrl+Alt and a function key. getty runs as root. It reads your user name and hands it to `/bin/login`, which checks your password and starts your shell as you. The kernel draws the text, and getty never reads what your programs print.

The second is a terminal emulator on your desktop. It runs as you. It draws every character your programs print, and it acts on the escape sequences they send, which are the codes a program uses to move the cursor, change colors, set a window title, or show a picture. Whatever a program makes the terminal emulator do happens with your rights.

vtium does the work of both. Like getty, it runs as root on a VT and starts `/bin/login`. Like a terminal emulator, it draws everything your programs print and acts on their escape sequences. So every program in your session talks to a root process through its output. That includes a program that only prints a file you downloaded, and a program on another machine that you reach over ssh. The rest of this page follows from that.

Read more:

- [Seats and sessions](seats-and-sessions.md) and [Deployment and start-up](start-up.md) describe how vtium takes the place of getty.
- H D Moore, [Terminal Emulator Security Issues](https://seclists.org/bugtraq/2003/Feb/315) (Bugtraq, 2003), shows how a terminal's features can be abused when it displays data nobody trusts.

## Root for the console, fewer rights for the helpers

vtium runs as root for its whole life. It needs root for the same reasons getty does: to open the VT, to take control of the displays, to read the keyboards and pointers straight from the kernel, and to start `/bin/login`, which must start as root so that it can switch to your account.

vtium's systemd unit sets none of systemd's sandboxing options. Your login session starts as a child of vtium, so it would inherit every limit the unit set. For example, `NoNewPrivileges=` would stop `sudo` from working in your session. vtium keeps your session the same as one that starts from getty.

The cost is that the code that reads your programs' output runs as root. vtium meets that cost by keeping the riskiest work out of its root process. Font files and compressed pictures, such as PNG and JPEG, are complex formats, and the code that parses such formats is where defects are most often found. vtium never parses them itself. Two helper programs do it:

- The font helper, `vtium-config serve`, draws the glyphs of a font set. It runs as the user `nobody`, and it refuses to run as root.
- The image helper, `vtium-image`, reads the image files a program names and decodes pictures, in a build with the `images` feature. It runs as the user of the session it serves, so it can read only what that user can read. Before anyone logs in, it reads no file, and it decodes pictures as the account `vtium-image`, or as `nobody` where that account is missing.

Each helper locks itself into a sandbox, a set of limits the kernel enforces on a process, before it reads anything. The limits include [no-new-privileges](https://docs.kernel.org/userspace-api/no_new_privs.html), [seccomp](https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html) filters on the system calls it may make, and [landlock](https://docs.kernel.org/userspace-api/landlock.html) limits on the files it may touch. vtium checks the program file before it starts a helper, and it checks every glyph and picture a helper sends back before it uses it.

The rest of vtium's root process is written to survive hostile output. It bounds everything a program can make it store, it is written so that no input can make it panic, which is Rust's word for a crash the code chose, and its output parser runs under fuzzing, where a test program feeds it generated input to find inputs that break it.

Read more:

- [Fonts, generated at run time](fonts.md) explains the font helper's place in drawing. The READMEs of [vtium-config](https://github.com/vtium/vtium/blob/main/crates/vtium-config/README.md) and [vtium-image-helper](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/README.md) describe each helper's sandbox in detail.
- Jerome H. Saltzer and Michael D. Schroeder, [The Protection of Information in Computer Systems](https://www.cs.virginia.edu/~evans/cs551/saltzer/) (1975), states the principle of least privilege: every program should run with the least set of privileges it needs.
- Niels Provos, Markus Friedl and Peter Honeyman, [Preventing Privilege Escalation](https://www.usenix.org/legacy/events/sec03/tech/full_papers/provos_et_al/provos_et_al.pdf) (USENIX Security 2003), describes privilege separation: splitting a service so that its parts run with different levels of privilege.
- Chromium's [sandbox design](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/design/sandbox.md) describes a privileged broker process that hands work to sandboxed target processes.
- Linux's [capabilities(7)](https://man7.org/linux/man-pages/man7/capabilities.7.html) explains how the kernel divides root's powers into separate capabilities.
- systemd's [systemd.exec(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html) describes the sandboxing options a unit can set, and how a service's children inherit them.
- The [Rust Fuzz Book](https://rust-fuzz.github.io/book/) introduces fuzz testing in Rust.

## What a program in your session can make vtium do

A program talks to vtium through its pty, a pseudoterminal: the kernel's pair of devices that lets one program act as another program's terminal. Any program that writes to the pty can send any bytes. vtium decides what each kind of request may do:

- **Replies.** Some escape sequences ask the terminal a question, such as where the cursor is. vtium builds every reply only from its own state, fixed strings, and numbers it reads and writes out again, never from text that came from the program's output. So a sequence hidden in a file can change what your screen shows, but it can never make vtium type a command into your shell.
- **Fonts and the size of the text area.** A program can switch the console's font, but only among the installed font sets, the built-in fonts, and the font its console started in. It can never name a file. vtium changes the size of the text area for a program only when you allow it.
- **Links.** A program can mark text as a link. vtium never opens a link. It copies the address when you Ctrl+click, and you decide where to paste it.
- **The clipboard.** A program can copy text into vtium's selection. It can never read the selection back.
- **Pictures from files.** A program can name a file or a shared-memory object for a picture. Only the image helper reads it, as your user. The helper opens only regular files, never a device, and refuses the kernel's own folders, such as `/proc` and `/sys`.

Read more:

- [config.md](../config.md) lists the settings that govern these requests, `allow-font-ops`, `allow-window-ops` and `clipboard-write`, with their defaults. [Determinism](../architecture.md#determinism) states the rule for replies in full.
- xterm's [control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) define the font sequence (OSC 50), the clipboard sequence (OSC 52) and the window operations. xterm's [manual](https://invisible-island.net/xterm/manpage/xterm.html) describes its `allowFontOps` and `allowWindowOps` resources, which govern the same requests.
- The [kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) sets the rules a terminal follows when a program names a file or a shared-memory object for a picture.
- Egmont Koblinger's [Hyperlinks in Terminal Emulators](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) specifies the link sequence (OSC 8) and its security advice.

## A configuration only root can change

vtium's configuration file, `/etc/vtium/vtium.conf`, decides which command each console runs as root, and which keyboard types into which display. So whoever can write the file can run commands as root.

For that reason, vtium reads the file only when root owns it and neither its group nor other users can write it, nor any folder above it. When the file fails that check, vtium reads none of it, runs its built-in configuration, and says so on the screen. vtium never writes the file. `vtium config` keeps your changes in a draft of your own, and asks for sudo only to install it. `vtium check-config` checks a file without privilege, and opens no device.

Read more:

- [Configure vtium](../configure.md) describes the check, the draft and the step that asks for sudo.
- OpenSSH's [`StrictModes`](https://man.openbsd.org/sshd_config#StrictModes) makes the same kind of check on a user's files before sshd accepts a login.

## Heads and stations

vtium can drive several displays from one VT. Each display, or group of displays, can be a head: a terminal with its own keyboards and its own sessions. For example, one display could run a kiosk program while another gives an operator a shell, or several people could each work at a station of their own on one machine.

vtium.conf's rules decide which keyboard types into which head, the way a firewall's rules decide which traffic passes. Like a firewall, the file has a default policy for what its rules do not mention. Under the open policy, the default, a keyboard no rule mentions types into the active head, and two guards keep you from locking yourself out. Under the closed policy, a keyboard no rule mentions types nowhere, the guards are off, and no failure of vtium hands its VT to a getty's login prompt. An isolated head keeps its keyboards to itself: their keys never switch the VT, and no roaming keyboard types into the head.

Read more:

- [Heads](heads.md) explains heads, the two policies and the operator's way back in. [The sections of vtium.conf](../head-file.md#which-input-can-reach-whose-session) lists each route by which one keyboard can reach another person's session, and the switch that governs it. [heads.md](../heads.md) shows complete files for a kiosk and for stations.
- Martin Flöser, [Looking at the security of Plasma/Wayland](https://blog.martin-graesslin.com/blog/2015/11/looking-at-the-security-of-plasmawayland/), describes a display server in which one program cannot read the key events meant for another's window.

## Logins and sessions

vtium never checks a password. By default, it starts `/bin/login` on each console, the same program a getty starts, and `login` checks your password through PAM, the pluggable authentication modules. So the rules your system sets for a console login apply.

On a systemd machine with logind, systemd's session manager, vtium runs each VT in one of two modes. In direct mode, the default, your login is the seat's active session while its VT is on screen, as at a getty. In seat mode, vtium holds the seat itself, and your login gets a session of its own with no seat, which polkit treats as inactive.

A restart of vtium does not end your sessions. vtium keeps a copy of each session's terminal with systemd, or without systemd in a small holder process that only root can reach, and a vtium that starts again takes each session back. On a systemd machine, when vtium crashes under the open policy, a getty takes the VT instead, and the sessions end.

Read more:

- [Seats and sessions](seats-and-sessions.md) explains both modes and what each gives your login. [systemd.md](../systemd.md) lists every place vtium touches systemd, logind and PID 1, and what it does without them.
- Linux-PAM's [pam(8)](https://man7.org/linux/man-pages/man8/pam.8.html) introduces PAM. systemd's [systemd-logind.service(8)](https://www.freedesktop.org/software/systemd/man/latest/systemd-logind.service.html) and [pam_systemd(8)](https://www.freedesktop.org/software/systemd/man/latest/pam_systemd.html) describe how logind tracks sessions and how a login registers with it. [polkit(8)](https://www.freedesktop.org/software/polkit/docs/latest/polkit.8.html) describes how polkit treats active and inactive sessions.
- systemd's [file descriptor store](https://systemd.io/FILE_DESCRIPTOR_STORE/) explains how a service keeps open files with systemd across a restart.
- The kernel's [DRM userland interfaces](https://docs.kernel.org/gpu/drm-uapi.html), in their section on the DRM master, [ioctl_vt(2)](https://man7.org/linux/man-pages/man2/ioctl_vt.2.html) and [seatd(1)](https://man.archlinux.org/man/seatd.1) describe how one program at a time controls the displays and the VTs, and how a seat manager hands them out.

## Where the protection ends

vtium protects you from what the programs in your sessions print, and from people at the machine who reach a head that is not theirs. Some things are outside what it can protect against.

- **Anyone with root.** Root can change vtium.conf, replace vtium, and do anything else on the machine.
- **A defect in vtium itself.** vtium is one root process that reads your programs' output. A defect that lets crafted output run code would run that code as root. Every head on a VT runs in that one process, so output that crashes vtium stops every head at once.
- **Physical access.** Someone who can boot other media, or edit the kernel command line, can change any file or turn vtium off for a boot. Once vtium has ended, Ctrl+Alt and a function key reach the kernel's own VTs.
- **What your screen shows.** A program in your session can draw anything on your screen, including text that looks like a login prompt.
- **A session left alone.** vtium has no screen lock yet, so anyone at a keyboard that reaches your session can type into it. See [the screen lock issue](https://github.com/vtium/vtium/issues/87).
- **A closed configuration that stops being trusted.** A closed vtium.conf that someone deletes, or makes writable by other users, is not read at the next start, and vtium runs its built-in configuration, which is open.
- **Root's pictures.** When root logs in on a vtium console, the image helper runs with root's user id, though without root's capabilities.

## Reporting a vulnerability

Report a vulnerability privately, through the Security tab of [vtium's repository](https://github.com/vtium/vtium/security). [SECURITY.md](https://github.com/vtium/vtium/blob/main/SECURITY.md) says what is in scope and what a report should include.
