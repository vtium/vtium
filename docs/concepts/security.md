# Security

This page explains how vtium protects your machine and your sessions, and why it is built the way it is. It is written for you if you are deciding whether to run vtium. It describes the main parts of the design and leaves out the details, because the details are still changing. Each section ends with links to the vtium pages that describe its part in detail. At the end of the page, [Further reading](#further-reading) lists papers and other projects' documentation on the same ideas.

## Two programs you already know

On most Linux machines, two kinds of program put text on your screen.

The first is getty. getty shows the login prompt on a virtual terminal (VT). A VT is one of the text screens you switch to with Ctrl+Alt and a function key. getty runs as root. It reads your user name and passes it to `/bin/login`. `login` checks your password and starts your shell with your rights. The kernel draws the text on the screen, and getty never reads what your programs print.

The second is a terminal emulator on your desktop. It runs as you. It draws every character your programs print. It also acts on the escape sequences they send. An escape sequence is a short code that a program prints to move the cursor, change colors, set a window title, or show a picture. Because the terminal emulator runs as you, anything a program makes it do happens with your rights.

vtium does the work of both. Like getty, it runs as root on a VT and starts `/bin/login`. Like a terminal emulator, it draws everything your programs print and acts on their escape sequences. So every program in your session talks to a process that runs as root, simply by printing. This is true even of a program that only prints a file you downloaded, and of a program on another machine that you reach over ssh. The rest of this page explains how vtium is built because of this, and where its protection ends.

Read more:

- [Seats and sessions](seats-and-sessions.md) and [Deployment and start-up](start-up.md) describe how vtium takes the place of getty.

## Root for the console, fewer rights for the helpers

vtium runs as root for its whole life. It needs root's rights for four jobs. It opens the VT and controls it, as getty does. It takes control of the displays. It reads the keyboards and pointers straight from the kernel's input devices. It starts `/bin/login`, as getty does. `login` must start as root, so that it can switch to your account when you log in.

systemd can sandbox a service: it can start the service with limits on what the service may do. vtium's systemd unit, the file that tells systemd how to start vtium, sets none of these limits. vtium starts your login session, so your session would inherit every limit the unit set. For example, `NoNewPrivileges=` would stop `sudo` from working in your session. Without those limits, your session works the same as a session that getty starts.

This has a cost. The code that reads your programs' output runs as root, so a mistake in that code is a mistake in a root process. vtium reduces the cost by doing the riskiest work in separate programs with fewer rights. Font files and compressed pictures, such as PNG and JPEG, have complex formats. The code that reads such formats is where programs most often have security defects. So vtium's root process never reads these formats itself. Two helper programs read them instead:

- The font helper, `vtium-config serve`, draws the glyphs, the shapes of the characters, from a font set. vtium starts it as the user `nobody`. The font helper also checks its own account, and it refuses to run as root.
- The image helper, `vtium-image`, is part of a vtium built with the `images` feature. It reads the image files that a program names, and it decodes pictures. vtium starts it as the user who is logged in on that console, so the helper can read only the files that user can read. Before anyone has logged in, the helper reads no files at all. It still decodes pictures that a program sends inside its output. It does that as the account `vtium-image`, which the package creates, or as `nobody` where that account is missing. When vtium cannot find the session's user, the helper never falls back to root.

Before a helper reads anything, it locks itself into a sandbox. A sandbox is a set of limits that the kernel enforces on a process. [No-new-privileges](https://docs.kernel.org/userspace-api/no_new_privs.html) stops the helper from gaining rights by starting another program. A [seccomp](https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html) filter limits the system calls the helper may make. [Landlock](https://docs.kernel.org/userspace-api/landlock.html) limits what the helper may do with files. Kernels before Linux 5.13 do not offer landlock, and neither do kernels built without it. On such a kernel, the font helper installs a second seccomp filter in landlock's place. That filter refuses the same changes to files and the same signals to other processes. vtium's log then says that the font helper runs without landlock. vtium also checks each helper's program file before it starts the helper. When a helper sends back a glyph or a picture, vtium checks it before using it.

The rest of vtium's root process is written to survive output that was crafted to harm it. vtium sets a limit on the size of everything a program can make it store. Its code is written so that no input can make it panic. A panic is Rust's name for a crash that a program chooses when something unexpected happens. vtium's output parser is also tested by fuzzing: a test program feeds it generated input for a long time, to find any input that breaks it.

Read more:

- [Fonts, generated at run time](fonts.md) explains the part the font helper plays in drawing. The READMEs of [vtium-config](https://github.com/vtium/vtium/blob/main/crates/vtium-config/README.md) and [vtium-image-helper](https://github.com/vtium/vtium/blob/main/crates/vtium-image-helper/README.md) describe each helper's sandbox in detail.

## What a program in your session can make vtium do

A program talks to vtium through a pty, short for pseudoterminal. A pty is a pair of devices in the kernel that lets one program act as the terminal of another. Any program that writes to the pty can send any bytes. Some of those bytes ask vtium to do something. vtium decides what each kind of request may do:

- **Replies.** Some escape sequences ask the terminal a question, such as where the cursor is. vtium builds its answer from what it knows itself, from fixed text, and from numbers, which it reads and writes out again. It never puts text from the program's output into an answer. The answer reaches your shell as if you had typed it. So a sequence hidden in a file can change what your screen shows, but it can never make vtium type a command into your shell.
- **Fonts and the size of the text area.** A program can switch the console's font. It can choose only among the font sets that are installed, the built-in fonts, and the font its console started with. It can never name a font file for vtium to open. A program can also ask vtium to change how many rows and columns the screen has. vtium does that only if you turn the setting on.
- **Links.** A program can mark text as a link. vtium never opens a link. When you Ctrl+click a link, vtium copies its address, and you decide where to paste it.
- **The clipboard.** A program can copy text into vtium's selection, the text that the middle mouse button pastes. A program can never read the selection, so it cannot see what you or another program copied.
- **Pictures from files.** A program can ask vtium to show a picture from a file, or from a shared-memory object, which is a block of memory that programs share by name. vtium's root process never opens it. The image helper opens it, as your user. The helper opens only ordinary files, and never a device. It refuses any file in the kernel's own folders, such as `/proc` and `/sys`.

Read more:

- [config.md](../config.md) lists the settings that control these requests, `allow-font-ops`, `allow-window-ops` and `clipboard-write`, and gives the default of each. [Determinism](../architecture.md#determinism) states the rule for replies in full.

## A configuration only root can change

vtium's configuration file is `/etc/vtium/vtium.conf`. It names the command that each console runs, and vtium runs that command as root. It also decides which keyboard types into which display. So whoever can write to the file can run commands as root.

So vtium checks the file before it reads it. vtium reads the file only when root owns it, and when neither the file's group nor other users may write to it. Every folder above the file must pass the same check. When the file fails the check, vtium reads none of it. It runs with its built-in configuration instead, and it shows a message on the screen that says so.

vtium itself never changes the file. `vtium config`, the tool that edits the configuration, keeps your changes in a draft in your home folder. It asks for sudo only at the moment it installs the draft. `vtium check-config` checks a configuration file the way vtium would read it. Any user can run it, and it opens no device.

Read more:

- [Configure vtium](../configure.md) describes the check, the draft, and the step that asks for sudo.

## Heads and stations

vtium can drive several displays from one VT. vtium.conf can divide those displays into heads. A head is a terminal on one display, or on a group of displays, with its own keyboards and its own sessions. For example, one display could run a kiosk program while another gives an operator a shell. Or several people could each work at a station of their own on one machine.

vtium.conf's rules decide which keyboard types into which head, the way a firewall's rules decide which traffic passes. Like a firewall, the file also has a default policy. The policy decides what happens to a keyboard that no rule names.

The open policy is the default. Under the open policy, a keyboard that no rule names types into the active head, the head that a key combination has selected. Two guards keep you from locking yourself out. The anti-lockout guard lets every keyboard type into the active head while no keyboard can reach a lit display. The rescue key, Ctrl+Alt and the function key of the VT on screen, lets the keyboard that pressed it type into the active head until the next boot.

Under the closed policy, vtium ignores any keyboard that no rule names, so its keys reach no session. The two guards are off. If vtium fails, systemd does not start a getty on its VT, because a getty's login prompt would take any keyboard. systemd starts vtium again instead. If vtium keeps failing, the VT is left with no login prompt. `vtium.enable=0` on the kernel command line turns vtium off for one boot, and under the closed policy it starts no getty on vtium's VTs either. The closed policy is strict on purpose, so a mistake in the rules can lock you out of the console on your own machine. [Heads](heads.md) describes how an operator keeps a way back in.

A head can also be isolated. On an isolated head's keyboards, Ctrl+Alt and a function key do not switch the VT. The session on the head receives those keys as ordinary keys. A roaming keyboard, one that types into whichever head is active, never types into an isolated head.

Read more:

- [Heads](heads.md) explains heads, the two policies, and the operator's way back in. [The sections of vtium.conf](../head-file.md#which-input-can-reach-whose-session) lists every way one keyboard can reach another person's session, and the setting that controls each way. [heads.md](../heads.md) shows complete configuration files for a kiosk and for stations.

## Logins and sessions

vtium never checks a password. By default, vtium starts `/bin/login` on each console, the same program a getty starts. `login` checks your password through PAM, the pluggable authentication modules. PAM is the part of the system that decides how a login is checked. So the rules your system sets for logging in at a console apply in vtium too.

logind is systemd's session manager. It keeps track of who is logged in, and on which seat. A seat is the set of devices one person uses, such as a screen, a keyboard, and a sound card. On a machine with systemd and logind, vtium runs each VT in one of two modes. Direct mode is the default. In direct mode, your login is the seat's active session while its VT is on screen, just as a login at a getty is. In seat mode, vtium holds the seat itself. Your login gets a session of its own, with no seat. polkit, the service that decides who may shut the machine down or mount a disk, treats such a session as inactive. So polkit asks for a password more often in seat mode.

A restart of vtium does not end your sessions. vtium gives systemd a copy of each session's terminal to hold. Without systemd, vtium gives the copies to a small holder process that only root can reach. When vtium starts again, it takes each session back from systemd or from the holder. There is one exception. On a systemd machine, when vtium crashes under the open policy, systemd starts a getty on the VT instead of vtium, and your sessions end.

Read more:

- [Seats and sessions](seats-and-sessions.md) explains both modes and what each gives your login. [systemd.md](../systemd.md) lists every place where vtium works with systemd, logind and PID 1, and what vtium does on a machine without them.

## Where the protection ends

vtium protects you from two things. It protects you from what the programs in your sessions print. It also protects you from people at the machine who reach a head that is not theirs. It cannot protect you from the following.

- **Anyone with root.** Root can change vtium.conf, replace vtium, and do anything else on the machine.
- **A defect in vtium itself.** vtium reads your programs' output in one process, which runs as root. If a defect in vtium let crafted output run code, that code would run as root. Every head on a VT runs in that same process. So output that crashes vtium stops every head at once.
- **Physical access.** Someone who can start the machine from other media can change any file. Someone who can edit the kernel command line can change what the machine runs at boot, for example with `init=`. `vtium.enable=0` on that line turns vtium off for one boot. Under the closed policy, vtium then leaves its VTs with no login prompt. Under the open policy, each of vtium's VTs gets a getty's login prompt, which takes any keyboard. Once vtium has stopped, Ctrl+Alt and a function key switch to the kernel's own VTs. If you run a closed vtium.conf, set a password on your boot loader. See [Turning vtium off for one boot](../systemd.md#turning-vtium-off-for-one-boot) for details.
- **What your screen shows.** A program in your session can draw anything on your screen, including text that looks like a login prompt.
- **A session left alone.** vtium has no screen lock yet. Anyone at a keyboard that reaches your session can type into it while you are away. See [the screen lock issue](https://github.com/vtium/vtium/issues/87).
- **A closed configuration that stops being trusted.** Suppose someone deletes a closed vtium.conf, or lets other users write to it. The next time vtium starts, it does not read the file. It runs its built-in configuration instead, and the built-in configuration uses the open policy. When the file is there but vtium cannot trust or open it, the screen shows `vtium.conf not used: running the built-in configuration.` vtium keeps no second record of the closed policy to fall back on. Such a record would have to come from `vtium config`, and `vtium config` is optional, since you can write vtium.conf by hand. So keep vtium.conf owned by root, and writable by root alone.
- **Root's pictures.** When root logs in on a vtium console, the image helper runs with root's user id. It has none of root's capabilities, the separate powers the kernel gives root. It can still read every file that root's user and root's group may read.

## Further reading

These are papers, specifications, and other projects' documentation on the ideas this page uses. Each one explains its idea in its own terms.

- H D Moore, [Terminal Emulator Security Issues](https://seclists.org/bugtraq/2003/Feb/315) (Bugtraq, 2003), shows how a terminal's features can be abused when it displays data nobody trusts. One example is a window title report that places text on the command line. It bears on [Two programs you already know](#two-programs-you-already-know) and on what a program can make vtium do.
- Jerome H. Saltzer and Michael D. Schroeder, [The Protection of Information in Computer Systems](https://www.cs.virginia.edu/~evans/cs551/saltzer/) (1975), states the principle of least privilege: every program should run with the least set of privileges it needs. It bears on [Root for the console, fewer rights for the helpers](#root-for-the-console-fewer-rights-for-the-helpers).
- Niels Provos, Markus Friedl and Peter Honeyman, [Preventing Privilege Escalation](https://www.usenix.org/legacy/events/sec03/tech/full_papers/provos_et_al/provos_et_al.pdf) (USENIX Security 2003), describes privilege separation. Privilege separation splits a service into parts that run with different levels of privilege, and the paper shows it in OpenSSH. It bears on the helpers, and on [Where the protection ends](#where-the-protection-ends).
- Chromium's [sandbox design](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/design/sandbox.md) describes a privileged broker process that hands work to sandboxed target processes. It bears on the helpers.
- The Linux kernel documents three of the limits a helper sets on itself. [Landlock](https://docs.kernel.org/userspace-api/landlock.html) explains how a process limits what it may do with files. [Seccomp filter](https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html) explains how a process limits the system calls it may make. [No_new_privs](https://docs.kernel.org/userspace-api/no_new_privs.html) explains how a process stops the programs it starts from gaining privileges. They bear on the helpers' sandboxes.
- Linux's [capabilities(7)](https://man7.org/linux/man-pages/man7/capabilities.7.html) explains how the kernel divides root's powers into separate capabilities. It bears on the image helper when root logs in.
- systemd's [systemd.exec(5)](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html) describes the sandboxing options a unit can set, such as `NoNewPrivileges=`. It also describes how a service's children inherit them. It bears on why vtium's unit sets none.
- The [Rust Fuzz Book](https://rust-fuzz.github.io/book/) introduces fuzz testing in Rust. It bears on how vtium's output parser is tested.
- xterm's [control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html) define the font sequence (OSC 50), the clipboard sequence (OSC 52), and the window operations. xterm's [manual](https://invisible-island.net/xterm/manpage/xterm.html) describes its `allowFontOps` and `allowWindowOps` resources, which control the same requests in xterm. They bear on [What a program in your session can make vtium do](#what-a-program-in-your-session-can-make-vtium-do).
- The [kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) sets the rules a terminal follows when a program names a file or a shared-memory object for a picture. It bears on pictures from files.
- Egmont Koblinger's [Hyperlinks in Terminal Emulators](https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda) specifies the link sequence (OSC 8). It advises that a plain click should not open a link. It bears on links.
- OpenSSH's [`StrictModes`](https://man.openbsd.org/sshd_config#StrictModes) checks who owns a user's files, and who may write to them, before sshd accepts a login. It bears on [A configuration only root can change](#a-configuration-only-root-can-change).
- Martin Flöser, [Looking at the security of Plasma/Wayland](https://blog.martin-graesslin.com/blog/2015/11/looking-at-the-security-of-plasmawayland/), describes a Wayland display server in which one program cannot read the key events meant for another program's window. It bears on [Heads and stations](#heads-and-stations).
- The kernel's [DRM userland interfaces](https://docs.kernel.org/gpu/drm-uapi.html), in their section on the DRM master, describe how one program at a time controls a display card. [ioctl_vt(2)](https://man7.org/linux/man-pages/man2/ioctl_vt.2.html) describes the requests that switch and control the VTs. They bear on why vtium runs as root.
- [seatd(1)](https://man.archlinux.org/man/seatd.1) describes a seat manager, which controls which sessions may use a seat's displays and input devices. [systemd-logind.service(8)](https://www.freedesktop.org/software/systemd/man/latest/systemd-logind.service.html) describes how logind tracks users and sessions. [pam_systemd(8)](https://www.freedesktop.org/software/systemd/man/latest/pam_systemd.html) describes how a login registers with logind. They bear on [Logins and sessions](#logins-and-sessions).
- Linux-PAM's [pam(8)](https://man7.org/linux/man-pages/man8/pam.8.html) introduces the pluggable authentication modules that check your password. [polkit(8)](https://www.freedesktop.org/software/polkit/docs/latest/polkit.8.html) describes how polkit treats active and inactive sessions. They bear on logins.
- systemd's [file descriptor store](https://systemd.io/FILE_DESCRIPTOR_STORE/) explains how a service keeps open files with systemd while it restarts. It bears on sessions that survive a restart of vtium.

## Reporting a vulnerability

Report a vulnerability privately, through the Security tab of [vtium's repository](https://github.com/vtium/vtium/security). [SECURITY.md](https://github.com/vtium/vtium/blob/main/SECURITY.md) says what is in scope and what a report should include.
