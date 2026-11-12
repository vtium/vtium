# Using VTium

## The configuration

vtium reads its settings once, when it starts. It reads one configuration file, `/etc/vtium/vtium.conf`, and then its command line. A setting on the command line overrides the same setting in the file.

The settings at the top of the file accept the keys of `kmscon`'s configuration file, and vtium adds keys of its own. So you can copy a `kmscon` configuration to `/etc/vtium/vtium.conf`, and vtium reads it. [config.md](../config.md) lists every setting.

Below the settings, sections of the file declare the heads, the looks, and the rules. A head is a terminal for one or more displays. A look is the font set, size and other font settings that a head renders with. The rules decide which head each display and each input device belongs to. See [Heads](heads.md) for details.

`vtium config` keeps your changes in a draft of your own. When you install the draft, it becomes `/etc/vtium/vtium.conf`. The install is one step, and that step runs with sudo. See [Configure vtium](../configure.md) for details.

## Selection and the clipboard

The console has no system clipboard. Instead, vtium holds one selection: the text you last selected with the pointer. vtium pastes the selection into the session.

A program in the session can set the selection with the standard clipboard control sequence, OSC 52. That sequence passes through ssh, so a program on a remote machine can copy text to the console.

A proposed tool, vtium-clipboard, would let scripts make the same copy from the command line, as `pbcopy` does on macOS. It writes its input to the selection by sending OSC 52 to its own terminal. See [the vtium-clipboard issue](https://github.com/vtium/vtium/issues/113). How vtium-clipboard would paste is not decided yet, because vtium refuses a program's request to read the selection. A decision record will choose between the options.

## Links and prompts

A program can mark text as a link, and a shell can mark its prompts. vtium keeps the links and the prompt marks. When you Ctrl+click a link, vtium copies its address so that you can paste it. Ctrl+Shift+Up and Ctrl+Shift+Down move the view from one prompt to the next. While you hold Ctrl over a link, vtium shows the link's address on the bottom row of the screen. When the pointer is on the bottom row, vtium shows the address on the top row instead.

## Keys

vtium reads the keyboard itself and passes the keys to the session. It keeps some keys for itself, such as the keys that switch VTs.

The configuration can bind keys to reboot, power off or suspend the machine. The reboot and power-off keys ask PID 1, the init system, to act, and PID 1 stops the services first. There is one exception. When the `kernel.ctrl-alt-del` setting is not zero, the reboot key restarts the machine at once. The suspend key asks logind to suspend the machine, or asks the kernel directly, and a suspend stops no service. See [systemd.md](../systemd.md) for details.

## Blanking

vtium turns its displays off after a period with no key press and no pointer action. A pointer action is a move, a button press or a turn of the wheel. The next key press or pointer action turns the displays on again. Output from a program never keeps the displays on by itself.

A program can set the timeout, or wake the screen, with the Linux console's own escape sequences. Setting the timeout also wakes the screen, as it does on the Linux console. A setting decides whether programs may do this. It is on by default, and the administrator can turn it off. No program can turn the screen off at once. See [the screen blanking issue](https://github.com/vtium/vtium/issues/79).

You can also blank and wake the displays from outside the session, with signals. `SIGRTMIN+1` blanks the displays at once, and `SIGRTMIN+2` wakes them. A script or a power daemon can send them without writing to the session's terminal. The commands `vtium blank --vt N` and `vtium wake --vt N` find the vtium running on VT N and send it the signal. Root, or the user that vtium runs as, may send these signals. See [the issue that blanks and wakes from outside the session](https://github.com/vtium/vtium/issues/116).
