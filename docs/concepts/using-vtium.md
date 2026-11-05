# Using VTium

## The configuration

vtium reads one configuration file, `/etc/vtium/vtium.conf`, then its command line over it, once at start. The top of the file takes the keys of `kmscon`'s configuration file and adds vtium's own, so vtium reads a `kmscon` configuration that a user copies to `/etc/vtium/vtium.conf`. [config.md](../config.md) lists every setting. Below the settings, sections declare the heads, the look each draws with, and the rules that place each display and input (ADR-1401, ADR-1403, [Heads](heads.md)).

`vtium config` keeps a user's changes in a draft of their own and installs the draft as `/etc/vtium/vtium.conf` through one step run with sudo ([Configure vtium](../configure.md)).

## Selection and the clipboard

The console has no system clipboard: vtium holds one selection, the text the user last selected with the pointer, and pastes it into the session. A program in the session can set that selection with the standard clipboard control sequence (OSC 52), which passes through ssh, so a program on a remote machine can copy to the console.

vtium-clipboard, proposed in vtium/vtium#113, gives scripts the same copy from the command line, as `pbcopy` does on macOS: it writes its input to the selection through OSC 52 on its own terminal. How it would paste is undecided, since vtium refuses a program's request to read the selection; the choices wait for a decision record.

## Links and prompts

Programs can mark text as a link, and shells can mark their prompts. vtium's terminal keeps the links and the marks. Ctrl+click on a link copies its address for the user to paste, and Ctrl+Shift+Up and Ctrl+Shift+Down move the view from one prompt to the next. Showing a link's address on screen is decided and not yet drawn ([ADR-1002](../architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md)).

## Keys

vtium reads the keyboard itself and passes keys to the session, apart from the keys it keeps for itself, such as those that switch VTs. The configuration can bind keys to reboot, power off or suspend the machine. The reboot and power-off keys ask PID 1 to act, which stops the services first, except that a reboot under a non-zero `kernel.ctrl-alt-del` restarts the machine at once. The suspend key asks logind, or the kernel directly, and stops no service ([systemd.md](../systemd.md)).

## Blanking

After a period with no key press or pointer action (a move, a button or the wheel), vtium turns its displays off, and the next one turns them on; output alone never keeps them on. A program can set the timeout or wake the screen with the Linux console's own sequences, and setting the timeout wakes the screen too, as on the Linux console. A setting, on by default, lets the administrator turn that program control off, and no program can turn the screen off at once (vtium/vtium#79).

From outside the session, `SIGRTMIN+1` blanks the displays at once and `SIGRTMIN+2` wakes them, so a script or a power daemon acts without writing to the session's terminal. `vtium blank --vt N` and `vtium wake --vt N` find the vtium on VT N and send it the signal. Root or the user vtium runs as may send them (vtium/vtium#116).
