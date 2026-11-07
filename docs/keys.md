# Keys

Sessions in vtium have `TERM=xterm-vtium`. Most remote hosts lack that terminfo entry, so read [ssh from vtium](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#ssh-from-vtium) before you ssh from vtium.

vtium keeps these keys for itself. A "head" is a terminal for one or more displays, and a "console" is one session within a head, with its own login, screen and scrollback.

| Keys | What vtium does | Setting |
|---|---|---|
| Ctrl+Alt+F1 to F12 | switches the VT | none: these keys never change |
| Shift+Up, Shift+Down | scrolls the view a line, on the main screen | `grab-scroll-up`, `grab-scroll-down` |
| Shift+PageUp, Shift+PageDown | scrolls the view a page through the scrollback, on the main screen | `grab-page-up`, `grab-page-down` |
| Ctrl+Shift+Up, Ctrl+Shift+Down | moves the view to the shell prompt above or below, where the shell marks its prompts with OSC 133, on the main screen | `grab-prompt-up`, `grab-prompt-down` |
| Ctrl+=, Ctrl+- | zooms the console in and out | `grab-zoom-in`, `grab-zoom-out` |
| Super+=, Super+- | rotates the head clockwise and counterclockwise | `grab-rotate-cw`, `grab-rotate-ccw` |
| Ctrl+Shift+Delete | resets the console on screen, as the `reset` command does | `grab-reset` |
| Shift+Insert, the middle button | pastes the selection | `grab-paste` for the key |
| Ctrl+click on a link | copies the link's address into the selection; vtium opens nothing | none |
| Ctrl+Super+Right, Ctrl+Super+Left | shows the head's next or previous console | `grab-console-next`, `grab-console-previous` |
| Ctrl+Super+Return | opens a new console in the head | `grab-console-open` |
| Ctrl+Super+Backspace | closes the console on screen, unless it is the head's last | `grab-console-close` |
| Ctrl+Super+Up, Ctrl+Super+Down | makes the next or previous head active, from a keyboard that roams among heads | `grab-focus-next`, `grab-focus-previous` |

Full-screen programs such as editors use the alternate screen. While a program shows it, the scroll keys and the prompt jumps reach the program, so an editor can use Shift with the arrows and Page Up and Page Down, for example to select text. On the main screen, where the shell runs, vtium takes them and scrolls.

Ctrl+Shift+Delete is the way out when a program ends and leaves the keyboard sending escape codes, so that even Enter stops working. It clears the console's screen and scrollback and turns off every mode a program set. It resets only the console on screen.

Selecting text with the pointer copies it into vtium's selection. The console has no system clipboard: vtium holds one selection, and a program can set it with OSC 52, but no program can read it back. Under a vtium.conf whose policy is `closed`, Ctrl+Super+Return and Ctrl+Super+Backspace do nothing, so each head keeps the consoles it started with.

No key reboots, powers off or suspends the machine until you bind one with `grab-reboot`, `grab-power-off` or `grab-suspend` ([config.md](config.md), [systemd.md](systemd.md)).

## Changing the keys

You change vtium's keys in vtium.conf, the one place where they are set. Each key in the Setting column above is an ordinary line of the file, which you can edit by hand in any text editor. With no vtium.conf, or with a vtium.conf that sets none of these keys, vtium uses the keys in the table. vtium-config's Settings view and its `set` command edit the same lines, and vtium reads the file the same way whichever wrote it.

A line names one or more key combinations, separated by commas. Each combination is its modifiers in angle brackets, `<Shift>`, `<Ctrl>`, `<Alt>` or `<Logo>` for Super, then the key's name as xkb writes it, such as `Delete`, `Prior` for Page Up or `equal`. For example, these lines move the reset key to Ctrl+Alt+R, scroll by a line with Alt+K and Alt+J as well as with Shift and the arrows, and hand Ctrl+= to every program:

```
grab-reset = <Ctrl><Alt>r
grab-scroll-up = <Shift>Up,<Alt>k
grab-scroll-down = <Shift>Down,<Alt>j
grab-zoom-in =
```

When you bind a key to a new combination, its old combination reaches the program. An empty value, as `grab-zoom-in =` above, takes no combination, so the key always reaches the program. With Shift, write the character Shift makes: `<Ctrl><Shift>V`, not `<Ctrl><Shift>v`.

Ctrl+Alt+F1 to F12 always switch the VT, and no line of vtium.conf can bind or empty them, so you always have a way to another VT. `vtium check-config` reports a line that tries to bind one, a combination that two settings bind, and a combination it cannot read, each with its file and line. When two settings bind one combination, vtium gives it to the setting it checks first: the power keys, then the console and head keys, then paste, then the others in the order of the table above. See [config.md](config.md#keys-vtium-takes) for every setting and its default.

[The terms of vtium.conf's sections](head-file.md#terms) define the keys that act on heads and consoles: the roaming chord, the console chords and the rescue key.
