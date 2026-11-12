# Security

This page explains how vtium protects your machine and your sessions, and why it is built the way it is. It is written for you if you are deciding whether to run vtium. It covers what runs with which rights, what each boundary protects, and where the protection ends.

## Two programs you already know

On most Linux machines, two kinds of program put text on your screen.

The first is getty, which shows the login prompt on a virtual terminal (VT), one of the text screens you switch to with Ctrl+Alt and a function key. getty runs as root. It reads your user name and hands it to `/bin/login`, which checks your password and starts your shell as you. getty draws nothing itself: the kernel draws the text, and getty never reads what your programs print.

The second is a terminal emulator on your desktop, such as the one you open in a graphical session. It runs as you. It draws every character your programs print, and it acts on the escape sequences they send, which are the codes a program uses to move the cursor, change colors, set a window title, or show a picture. Whatever a program makes the terminal emulator do happens with your rights, and no more.

vtium does the work of both. Like getty, it runs as root on a VT and starts `/bin/login`. Like a terminal emulator, it draws everything your programs print and acts on their escape sequences. So every program in your session talks to a root process through its output, including a program that only prints a file you downloaded, and a program on another machine that you reach over ssh. The rest of this page follows from that.

## What runs with which rights

| Program | Runs as | What it does |
|---|---|---|
| `vtium`, the console | root, for its whole life | Opens the VT, the displays and the input devices, draws the screen, and starts each login |
| The login child | root, until `login` hands over to you | Runs `/bin/login -p`, or the command vtium.conf's `login` key names |
| Your shell and your programs | you | Whatever you run after you log in |
| `vtium-config serve`, the font helper | `nobody` | Draws the glyphs of a font set |
| `vtium-image`, the image helper | the user of the session it serves | Reads image files and decodes pictures, in a build with the `images` feature |
| `vtium config` and `vtium check-config` | you | Edit and check the configuration; only an install asks for sudo |

### Why vtium runs as root

vtium needs root for the same reasons getty does. It opens the VT and changes its modes. It takes control of the displays through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the kernel's graphics interface. It reads the keyboards and pointers directly from the kernel's input devices. It starts `/bin/login`, which must start as root so that it can switch to your account.

vtium's systemd unit sets none of systemd's sandboxing options. Your login session starts as a child of vtium, so it would inherit every limit the unit set. For example, systemd's `NoNewPrivileges=` would stop `sudo` and `su` from working in your session, and its mount options would hide parts of the file system from your shell. vtium keeps your session the same as a session that starts from getty.

The cost is plain. The code that reads your programs' output runs as root, so a defect in it is a defect in a root process. vtium answers that cost in three ways. It bounds everything a program can make it store, such as the length of a title, a link, or a copied text. The code that reads a program's output is written so that no input can reach a panic, the Rust term for a crash that the code chose: it checks every number and every index a program controls, and the project's checks refuse code that could panic on them. The output parser and the screen run under [fuzzing](https://en.wikipedia.org/wiki/Fuzzing), where a test program feeds them generated input for a long time to find inputs that break them.

### Fonts and pictures are read outside the root process

Font files and compressed picture formats, such as PNG, JPEG, and GIF, are complex. The code that parses them is where defects are most often found in any program. So vtium never parses a font file or decodes a compressed picture in its own root process. Two helper programs do that work, each with fewer rights than vtium and each locked into a sandbox, a set of limits the kernel enforces on a process. vtium checks everything a helper sends back before it uses it: the size of each glyph and picture, and every field of every reply. vtium reads two simple picture formats itself, sixel and the raw pixels of the kitty graphics protocol, under the same bounds as the rest of a program's output.

### The font helper

The font helper is `vtium-config serve`. It draws the glyphs of a font set (a frit, a small text file that names the fonts to draw from). vtium starts it the first time it needs glyphs, and the helper exits when it has had nothing to do for a while.

When vtium runs as root, it starts the font helper as the user `nobody`. The helper runs in `/`, with an empty environment, in a process group of its own, and holds only the pipes it talks to vtium through. vtium reads the helper's user and group ids back from the kernel once it runs, and refuses a helper that holds any other id or any supplementary group. The helper checks for itself too: it refuses to run when any of its user ids is root's, or when it holds root's group.

Before it reads anything, the helper locks itself in:

- It sets no-new-privileges, so no program it could start gains rights from a setuid bit.
- It limits its memory and its processor time. It may write no file and start no process.
- It enters a [landlock](https://docs.kernel.org/userspace-api/landlock.html) domain, a kernel feature that lets a process give up rights to the file system and the network. In that domain the helper cannot write, create, remove, or rename any file, cannot open a network connection, and cannot signal a process outside itself.
- It installs a [seccomp](https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html) filter, a list of system calls the kernel refuses for the process, under which it cannot make a socket of any kind.

The helper can still read files, because a font set may name a font anywhere on the machine. It reads them as `nobody`, so it can read only what every user of the machine may read. When the font helper cannot start, vtium logs why and draws with its built-in fonts. See [Fonts, generated at run time](fonts.md) for how the helper fits into drawing.

### The image helper

The image helper is `vtium-image`. A build with the `images` feature, which the `vtium` package includes, uses it to read the files and the shared memory that a program names for a picture, and to decode PNG, JPEG, GIF, and pixels compressed with zlib.

The image helper runs as the user of the session it serves, so it can read only the files that user can read. vtium finds that user before each request:

- Where logind runs, it is the user of the login's own logind session.
- Where logind does not run, it is the owner of the session's terminal device, which `login` gives to you when you log in.
- Before anyone logs in, the helper reads no file at all. A picture a program sends inline, as bytes in its output, decodes as the account `vtium-image`, which the package creates, or as `nobody` when that account is missing.

The helper never runs as root because vtium could not find a user. When vtium goes by the terminal's owner and that owner is root, the helper reads no file. When root itself logs in on a vtium console, the helper runs with root's user id, with every capability removed. [Capabilities](https://man7.org/linux/man-pages/man7/capabilities.7.html) are the separate powers the kernel gives root. Without them, a process that runs as root cannot, for example, load a kernel module or read another user's private files.

The image helper starts in `/` with an empty environment, with no-new-privileges set, no capabilities, a limit on its memory, a limit on the size of any file it writes, and only its socket to vtium. It installs a seccomp filter that allows a short list of system calls and refuses the rest. Each picture is decoded in a worker process, which opens the one file the picture names, if any, and then locks itself in with landlock, so it can open nothing more while it decodes. A stream of inline pictures of one format and size, such as video frames, shares one worker, locked in the same way.

A request that the helper does not answer in 2 seconds fails, and vtium stops the helper. After the helper crashes three times, vtium starts no more image helpers for that console until its session ends, so a hostile picture cannot retry against the decoders without end. Sixel pictures, and raw pixels a program sends inline, keep working, because vtium reads them itself.

### The checks before a helper starts

vtium opens each helper's program once, when vtium starts. It refuses the program, and does not start it, when:

- its path is not absolute,
- it is not a regular file,
- it is setuid or setgid, which would let it run with its owner's rights instead of the rights vtium gives it,
- it belongs to an account that is neither root nor the account vtium runs as, or
- its group or other users may write it.

vtium checks the file it opened, and starts every helper from that open file through `/proc/self/fd`, so the file it checked is the file that runs. Someone who replaces the program on disk after vtium started reaches no running console. A package upgrade reaches a console when that console starts again.

## What a program in your session can make vtium do

A program talks to vtium through its pty (a [pseudoterminal](https://en.wikipedia.org/wiki/Pseudoterminal), the kernel's pair of devices that lets one program act as another program's terminal). Any program that writes to the pty can send any bytes. That includes `cat` of a file from the internet, and a program on a remote machine over ssh. Each thing such bytes can ask for has a rule:

- **Replies.** Some escape sequences ask the terminal a question, such as where the cursor is. vtium builds every reply only from its own state, fixed strings, and numbers it parses and writes again. A reply never contains text that came from the program's output. So a sequence hidden in a file can change what your screen shows, but it can never make vtium type a command into your shell. For the same reason, when a program asks for the window title, vtium always answers with an empty title. See [Determinism](../architecture.md#determinism) for the full rule.
- **Fonts.** A program can switch the console's font with xterm's font sequence, OSC 50, while the setting `allow-font-ops` is on, as it is by default. A program can choose only the font sets installed in vtium's font folder, `/etc/vtium/fonts` by default, the built-in fonts, and the font its console started in. It can never name a file. Font sets reach that folder only through root: the package installs them, and `vtium config install` installs yours through sudo. The switch lasts until the session of the program that asked for it ends.
- **The size of the text area.** A program could ask vtium to change the number of rows and columns with xterm's window operation. vtium ignores that request unless you turn `allow-window-ops` on, and it is off by default. A program can always ask what the size is.
- **Links.** A program can mark text as a link. vtium never opens a link. Ctrl+click on a link copies its address into vtium's selection, and you decide where to paste it.
- **The clipboard.** A program can copy text into vtium's selection with the clipboard sequence, OSC 52, while `clipboard-write` is on, as it is by default. A program can never read the selection: vtium gives no answer to that request. The console has no system clipboard, so the selection holds only what you or your programs put there.
- **Pictures from files.** A program can name a file or a shared-memory object for a picture, with the kitty graphics protocol. Only the image helper reads it, as your user. The helper refuses any path in `/proc`, `/sys`, or `/dev`, apart from `/dev/shm`, and opens only a regular file, never a device or a pipe. Every failure gets the same answer, so a program learns nothing about a file it could not read itself. A temporary file is read, and removed, only when its name contains `tty-graphics-protocol` and it lies in `/tmp`, `/dev/shm`, or vtium's own temporary folder, as the kitty specification asks.

See [config.md](../config.md) for `allow-font-ops`, `allow-window-ops`, and `clipboard-write`.

## Who may change vtium.conf

vtium's configuration file, `/etc/vtium/vtium.conf`, decides which command each console runs, which keyboards type into which displays, and which keys vtium acts on itself. Its `login` key names the command each console runs as root. So whoever can write the file can run commands as root.

For that reason, vtium reads the file only when root owns it and neither its group nor other users can write it. Every folder above it must pass the same check, including each folder along a symbolic link, because whoever can write a folder can replace the file in it. This is the same check that [`sshd`'s `StrictModes`](https://man.openbsd.org/sshd_config#StrictModes) makes on a user's key files. Run as root, vtium reads no configuration file that a user supplies.

When the file fails the check, vtium reads none of it. It runs its built-in configuration instead, and every head shows `vtium.conf not used: running the built-in configuration.` with the next step, `To see why, run vtium check-config /etc/vtium/vtium.conf.` vtium never changes or replaces the file itself.

You can change the file as root with any editor, or with `vtium config`, which keeps your changes in a draft in your home folder. Only the install step asks for sudo, and it runs one command, `sudo /usr/bin/vtium-config apply DIR`, which takes no option that names a path or a program. So a sudo rule can allow exactly that command. See [When vtium-config asks for sudo](../configure.md#when-vtium-config-asks-for-sudo) for details.

`vtium check-config` checks a file the way vtium would read it at start. Any user can run it. It opens nothing under `/dev` and writes nothing: it learns which devices are connected from sysfs and udev's database, which every user may read.

## Keyboards and displays across heads

vtium can drive several displays from one VT, each with its own terminal. Each of those terminals is a head, a terminal for one or more displays with its own keyboards and its own sessions. For example, one display could run a kiosk program while another gives an operator a shell. vtium.conf's rules decide which keyboard types into which head, the way a firewall's rules decide which traffic passes. See [Heads](heads.md) for how heads work.

Like a firewall, vtium.conf has a default policy for what its rules do not mention:

- **Open**, the default. A keyboard that no rule mentions roams: it types into the active head, the one a chord selects. Two guards keep you from locking yourself out. The anti-lockout guard makes every keyboard roam while no keyboard reaches a lit display. The rescue key, Ctrl+Alt and the function key of the VT already on screen, makes the keyboard that pressed it roam until the next boot.
- **Closed**, for a kiosk or for stations that several people use at once. A keyboard that no rule mentions types nowhere. Every head is isolated unless the file says otherwise. The anti-lockout guard and the rescue key are off, so no keyboard ever goes anywhere the file did not send it.

An isolated head keeps its own keyboards to itself. On a keyboard of an isolated head, Ctrl+Alt and a function key reach the session as plain keys and switch no VT. No roaming keyboard ever types into an isolated head. So someone at an isolated head cannot type into another head's session, switch the displays to another VT, or move a keyboard to another head.

A keyboard that the rules send nowhere delivers nothing to any session, and vtium logs nothing it types. Under the open policy, vtium still opens such a keyboard to watch for the VT keys alone, the rescue key among them, and only while vtium's VT is on screen. Under the closed policy, vtium does not open it at all.

A head with `switch-on-enter = yes` lets Enter on that head's keyboard switch to the head's VT from another VT. It works only under the open policy and never on an isolated head.

Once vtium has read `policy = closed` from a file it trusts, the policy stays closed for as long as vtium runs, whatever happens while it runs. Under the closed policy, no failure of vtium hands its VT to a getty either, because a getty's login prompt takes any keyboard. [Which input can reach whose session](../head-file.md#which-input-can-reach-whose-session) lists every route by which one keyboard can reach another person's session, and the switch that governs each.

## Logins and sessions

vtium never checks a password. On each console, vtium starts `/bin/login -p` with a fresh environment, the same program a getty starts. `login` asks for your name and your password, and checks them through [PAM](https://en.wikipedia.org/wiki/Pluggable_authentication_module), the pluggable authentication modules. So the rules your system's PAM configuration sets for a console login apply, as they do at a getty.

On a systemd machine with logind, systemd's session manager, vtium runs each VT in one of two modes:

- In **direct mode**, the default, your login registers with logind on the seat and on vtium's VT, as a login at a getty does. Your login is the seat's active session while its VT is on screen, so it gets the seat's devices, such as the sound card.
- In **seat mode**, vtium holds the seat through a logind session of its own, and starts each login in a systemd scope of its own, a group of processes that systemd tracks apart from vtium. Your login gets a logind session with no seat. polkit, the service that decides who may shut the machine down or mount a disk, then treats your session as inactive, so it asks for a password for actions it allows the active session without one.

See [Seats and sessions](seats-and-sessions.md) for both modes and what each gives your login.

A restart of vtium does not end your sessions. vtium keeps a copy of each session's pty in [systemd's file descriptor store](https://systemd.io/FILE_DESCRIPTOR_STORE/), where systemd holds open files for a service while it restarts. Without systemd, vtium keeps the copies in a small holder process, `vtium hold`, whose socket only root may open. A vtium that starts again on the VT takes each pty back, and the program in it redraws its screen. vtium's unit takes messages for the store from vtium alone, never from a program in a session, so no program in a session can add a file to the store or remove one from it.

This changes how a failure looks. On a systemd machine, when vtium crashes under the open policy, the unit starts a getty on the VT, the stored ptys close, and your sessions end. Under the closed policy no getty comes, so vtium starts again and your sessions survive. In seat mode, a login prompt nobody has used yet ends at a restart, and vtium shows a fresh prompt.

## Where the protection ends

vtium protects you from what the programs in your sessions print, and from people at the machine who reach a head that is not theirs. Some things are outside what it can protect against.

- **Anyone with root.** Root can change vtium.conf, replace vtium, read every session's memory, and do anything else on the machine.
- **A defect in vtium itself.** vtium is one root process that reads your programs' output. A defect that lets crafted output run code would run that code as root. vtium has no separate unprivileged process for that work.
- **All heads in one process.** Every head on a VT runs in the same vtium process. Output that crashes vtium stops every head's displays at once, until vtium starts again.
- **Physical access.** Someone who can boot other media can change any file. Someone who can edit the kernel command line can turn vtium off for a boot with `vtium.enable=0`. vtium then exits before it reads vtium.conf, so on a systemd machine a getty's login prompt takes the VT, whatever the file's policy. Once vtium has ended, Ctrl+Alt and a function key reach the kernel's own VTs and any getty that runs on them. The operator of a closed machine also turns off those gettys, as [Heads](heads.md#two-policies-open-and-closed) describes.
- **What your screen shows.** A program in your session can draw anything on your screen, including text that looks like a login prompt or a message from vtium.
- **A logged-in session left alone.** vtium has no screen lock yet, so anyone at a keyboard that reaches your session can type into it. See [the screen lock issue](https://github.com/vtium/vtium/issues/87).
- **Your own shared memory.** Any program that writes to your terminal can make the image helper read and then remove a shared-memory object that you own. The helper removes only objects your user owns.
- **Root's pictures.** When root logs in on a vtium console, the image helper runs with root's user id, without capabilities. It can read every file whose permissions let root's user or root's group read it.
- **A closed file that stops being trusted.** A closed vtium.conf that someone deletes, or makes writable by its group or by other users, is not read at the next start of vtium. vtium then runs its built-in configuration, which is open.
- **A kiosk program.** A head's `login` command runs as root. A kiosk program drops its rights itself, or starts through a tool that does.
- **Older kernels and 32-bit ARM.** On a kernel without landlock, the font helper runs without its landlock limits, and the image helper's workers use a seccomp filter that refuses to open any file in place of landlock. On 32-bit ARM, both helpers run without their seccomp filters.

## Reporting a vulnerability

Report a vulnerability privately, through the Security tab of [vtium's repository](https://github.com/vtium/vtium/security). [SECURITY.md](https://github.com/vtium/vtium/blob/main/SECURITY.md) says what is in scope and what a report should include.
