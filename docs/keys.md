# Keys

Sessions in vtium have `TERM=xterm-vtium`. Most remote hosts lack that terminfo entry, so read [ssh from vtium](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#ssh-from-vtium) before you ssh from vtium.

vtium keeps these keys for itself. A "head" is a terminal for one or more displays, and a "console" is one session within a head, with its own login, screen and scrollback.

| Keys | What vtium does |
|---|---|
| Ctrl+Alt+F1 to F12 | switches the VT |
| Shift+PageUp, Shift+PageDown | scrolls the view a page through the scrollback |
| Shift+Up, Shift+Down | scrolls the view a line |
| Ctrl+Shift+Up, Ctrl+Shift+Down | moves the view to the shell prompt above or below, where the shell marks its prompts with OSC 133 |
| Ctrl+=, Ctrl+- | zooms the console in and out |
| Super+=, Super+- | rotates the head clockwise and counterclockwise |
| Shift+Insert, the middle button | pastes the selection |
| Ctrl+click on a link | copies the link's address into the selection; vtium opens nothing |
| Ctrl+Super+Right, Ctrl+Super+Left | shows the head's next or previous console |
| Ctrl+Super+Return | opens a new console in the head |
| Ctrl+Super+Backspace | closes the console on screen, unless it is the head's last |
| Ctrl+Super+Up, Ctrl+Super+Down | makes the next or previous head active, from a keyboard that roams among heads |

Selecting text with the pointer copies it into vtium's selection. The console has no system clipboard: vtium holds one selection, and a program can set it with OSC 52, but no program can read it back. Under a vtium.conf whose policy is `closed`, Ctrl+Super+Return and Ctrl+Super+Backspace do nothing, so each head keeps the consoles it started with.

No key reboots, powers off or suspends the machine until you bind one with `grab-reboot`, `grab-power-off` or `grab-suspend` ([config.md](config.md), [systemd.md](systemd.md)). `grab-paste` changes the paste keys.

[The terms of vtium.conf's sections](head-file.md#terms) define the keys that act on heads and consoles: the roaming chord, the console chords and the rescue key.
