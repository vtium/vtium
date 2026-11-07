# Keys

Sessions in vtium have `TERM=xterm-vtium`. Most remote hosts lack that terminfo entry, so read [ssh from vtium](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#ssh-from-vtium) before you ssh from vtium.

vtium keeps these keys for itself. A "head" is a terminal for one or more displays, and a "console" is one session within a head, with its own login, screen and scrollback.

| Keys | What vtium does | Key binding |
|---|---|---|
| Ctrl+Alt+F1 to F12 | switches the VT | none: these keys never change |
| Shift+Up, Shift+Down | scrolls the view a line, on the main screen | `key-scroll-up`, `key-scroll-down` |
| Shift+PageUp, Shift+PageDown | scrolls the view a page through the scrollback, on the main screen | `key-page-up`, `key-page-down` |
| Ctrl+Shift+Up, Ctrl+Shift+Down | moves the view to the shell prompt above or below, where the shell marks its prompts with OSC 133, on the main screen | `key-prompt-up`, `key-prompt-down` |
| Ctrl+=, Ctrl+- | zooms the console in and out | `key-zoom-in`, `key-zoom-out` |
| Super+=, Super+- | rotates the head clockwise and counterclockwise | `key-rotate-cw`, `key-rotate-ccw` |
| Ctrl+Shift+Delete | resets the console on screen, as the `reset` command does | `key-reset` |
| Shift+Insert, the middle button | pastes the selection | `key-paste` for the key |
| Ctrl+click on a link | copies the link's address into the selection; vtium opens nothing | none |
| Ctrl+Super+Right, Ctrl+Super+Left | shows the head's next or previous console | `key-console-next`, `key-console-previous` |
| Ctrl+Super+Return | opens a new console in the head | `key-console-open` |
| Ctrl+Super+Backspace | closes the console on screen, unless it is the head's last | `key-console-close` |
| Ctrl+Super+Up, Ctrl+Super+Down | makes the next or previous head active, from a keyboard that roams among heads | `key-focus-next`, `key-focus-previous` |

Full-screen programs such as editors use the alternate screen. While a program shows it, the scroll keys and the prompt jumps reach the program, so an editor can use Shift with the arrows and Page Up and Page Down, for example to select text. On the main screen, where the shell runs, vtium takes them and scrolls.

Ctrl+Shift+Delete is the way out when a program ends and leaves the keyboard sending escape codes, so that even Enter stops working. It clears the console's screen and scrollback and turns off every mode a program set. It resets only the console on screen.

Selecting text with the pointer copies it into vtium's selection. The console has no system clipboard: vtium holds one selection, and a program can set it with OSC 52, but no program can read it back. Under a vtium.conf whose policy is `closed`, Ctrl+Super+Return and Ctrl+Super+Backspace do nothing, so each head keeps the consoles it started with.

No key reboots, powers off or suspends the machine until you bind one with `key-reboot`, `key-power-off` or `key-suspend` ([config.md](config.md), [systemd.md](systemd.md)).

## Changing the key bindings

You change vtium's keys in vtium.conf, the one place where they are set. Each key binding in the last column above is an ordinary line of the file, which you can edit by hand in any text editor. With no vtium.conf, or with a vtium.conf that sets none of these keys, vtium uses the keys in the table. vtium-config's Settings view and its `set` command edit the same lines, and vtium reads the file the same way whichever wrote it. In vtium-config, an empty value binds no key, and the word `default` returns a key binding to its default.

A line names one or more key combinations, separated by commas. Each combination is its modifiers in angle brackets, `<Shift>`, `<Ctrl>`, `<Alt>` or `<Logo>` for Super, then the key's name as xkb writes it, such as `Delete`, `Prior` for Page Up or `equal`. For example, these lines move the reset key to Ctrl+Alt+R, scroll by a line with Alt+K and Alt+J as well as with Shift and the arrows, and hand Ctrl+= to every program:

```
key-reset = <Ctrl><Alt>r
key-scroll-up = <Shift>Up,<Alt>k
key-scroll-down = <Shift>Down,<Alt>j
key-zoom-in =
```

When you bind a key to a new combination, its old combination reaches the program. An empty value, as `key-zoom-in =` above, takes no combination, so the key always reaches the program. With Shift, write the character Shift makes: `<Ctrl><Shift>V`, not `<Ctrl><Shift>v`.

Ctrl+Alt+F1 to F12 always switch the VT, and no line of vtium.conf can bind or empty them, so you always have a way to another VT. `vtium check-config` reports a line that tries to bind one, a combination that two settings bind, and a combination it cannot read, each with its file and line. Four of these settings had other names before they were called key bindings: `grab-paste`, `grab-reboot`, `grab-power-off` and `grab-suspend`. vtium still reads each old name as `key-paste`, `key-reboot`, `key-power-off` or `key-suspend`. `vtium check-config` notes the line so that you can rename it, and the file still passes the check. When vtium-config sets one of these key bindings, it comments out the line with the old name. When a file sets both names, vtium uses the new one.

When two settings bind one combination, vtium gives it to the setting it checks first: the power keys, then the console and head keys, then paste, then the others in the order of the table above. See [config.md](config.md#key-bindings) for every setting and its default.

[The terms of vtium.conf's sections](head-file.md#terms) define the keys that act on heads and consoles: the roaming chord, the console chords and the rescue key.
