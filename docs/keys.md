# Keys

Each session in vtium runs with `TERM=xterm-vtium`. Programs look up that name in terminfo, the database that describes what each kind of terminal can do. Most remote hosts do not have a terminfo entry for `xterm-vtium`. Read [ssh from vtium](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#ssh-from-vtium) before you ssh from vtium.

vtium keeps the keys in the table below for itself. Two words in the table need explaining. A "head" is a terminal that shows on one or more displays. A "console" is one session within a head, with its own login, screen and scrollback.

| Keys | What vtium does | Key binding |
|---|---|---|
| Ctrl+Alt+F1 to F12 | switches the VT | none: these keys never change |
| Shift+Up, Shift+Down | scrolls the view a line, on the main screen | `key-scroll-up`, `key-scroll-down` |
| Shift+PageUp, Shift+PageDown | scrolls the view a page through the scrollback, on the main screen | `key-page-up`, `key-page-down` |
| Ctrl+Shift+Up, Ctrl+Shift+Down | moves the view to the shell prompt above or below, on the main screen. This works when the shell marks its prompts with OSC 133. | `key-prompt-up`, `key-prompt-down` |
| Ctrl+=, Ctrl+- | zooms the console in and out | `key-zoom-in`, `key-zoom-out` |
| Super+=, Super+- | rotates the head clockwise and counterclockwise | `key-rotate-cw`, `key-rotate-ccw` |
| Ctrl+Shift+Delete | resets the console on screen, as the `reset` command does | `key-reset` |
| Shift+Insert, the middle button | pastes the selection | `key-paste` for the key |
| Ctrl+click on a link | copies the link's address into the selection. vtium does not open the link. | none |
| Ctrl+Super+Right, Ctrl+Super+Left | shows the head's next or previous console | `key-console-next`, `key-console-previous` |
| Ctrl+Super+Return | opens a new console in the head | `key-console-open` |
| Ctrl+Super+Backspace | closes the console on screen, unless it is the head's last | `key-console-close` |
| Ctrl+Super+Up, Ctrl+Super+Down | makes the next or previous head active. Only a keyboard that roams among heads can use these keys. Such a keyboard types into whichever head is active. | `key-focus-next`, `key-focus-previous` |

Full-screen programs, such as editors, use a second screen called the alternate screen. While a program shows the alternate screen, vtium passes the scroll keys and the prompt jumps to the program. An editor can then use Shift with the arrows, Page Up and Page Down, for example to select text. On the main screen, where the shell runs, vtium keeps these keys for itself and scrolls.

Sometimes a program ends and leaves the keyboard sending escape codes, so that even Enter stops working. Ctrl+Shift+Delete is the way out. It clears the console's screen and scrollback, and turns off every mode that a program set. It resets only the console on screen.

When you select text with the pointer, vtium copies it into its selection. The console has no system clipboard. Instead, vtium holds one selection. A program can set the selection with OSC 52, but no program can read it back.

When the active head moves, each head shows a filled or a hollow dot at its top right, so you can see where your typing goes. See [The on-screen display and notices](concepts/osd-and-notices.md) for details.

vtium.conf can set a policy, which says what vtium does with devices that no rule names. When the policy is `closed`, Ctrl+Super+Return and Ctrl+Super+Backspace do nothing. Each head then keeps the consoles it started with.

By default, no key reboots, powers off or suspends the machine. To get such a key, bind one with `key-reboot`, `key-power-off` or `key-suspend`. See [config.md](config.md) and [systemd.md](systemd.md) for details.

## Changing the key bindings

You change vtium's keys in vtium.conf. It is the one place where they are set. Each key binding in the last column above is an ordinary line of the file. You can edit that line by hand in any text editor. When there is no vtium.conf, or the file sets none of these keys, vtium uses the keys in the table.

vtium-config, vtium's configuration tool, edits the same lines through its Settings view and its `set` command. vtium reads the file the same way, whichever of the two wrote it. In vtium-config, an empty value binds no key. The word `default` returns a key binding to its default.

A line names one or more key combinations, separated by commas. A combination starts with its modifiers, each in angle brackets: `<Shift>`, `<Ctrl>`, `<Alt>`, or `<Logo>` for Super. The key's name follows. Write the name the way xkb writes it. xkb is the keyboard layout system of Linux desktops. Examples are `Delete`, `Prior` for Page Up, and `equal`. For example, the lines below make three changes:

- They move the reset key to Ctrl+Alt+R.
- They scroll by a line with Alt+K and Alt+J, as well as with Shift and the arrows.
- They hand Ctrl+= to every program.

```
key-reset = <Ctrl><Alt>r
key-scroll-up = <Shift>Up,<Alt>k
key-scroll-down = <Shift>Down,<Alt>j
key-zoom-in =
```

When you give a key binding a new combination, vtium no longer keeps the old combination, so the old combination reaches the program. An empty value, such as `key-zoom-in =` above, gives the key binding no combination. Its key then always reaches the program. When a combination uses Shift, write the character that Shift makes: `<Ctrl><Shift>V`, not `<Ctrl><Shift>v`.

Ctrl+Alt+F1 to F12 always switch the VT. No line of vtium.conf can bind these keys or empty them, so you always have a way to another VT.

`vtium check-config` reports three kinds of mistake, each with its file and line:

- a line that tries to bind one of Ctrl+Alt+F1 to F12,
- a combination that two settings bind, and
- a combination that it cannot read.

Four of these settings had other names before they were called key bindings. vtium still reads each old name as the new one: `grab-paste` as `key-paste`, `grab-reboot` as `key-reboot`, `grab-power-off` as `key-power-off`, and `grab-suspend` as `key-suspend`. `vtium check-config` notes a line with an old name so that you can rename it. The file still passes the check. When vtium-config sets one of these key bindings, it comments out the line with the old name. When a file sets both names, vtium uses the new one.

When two settings bind one combination, vtium gives the combination to the setting it checks first. It checks the power keys first, then the console and head keys, then paste. It checks the others last, in the order of the table above. See [config.md](config.md#key-bindings) for every setting and its default.

[The terms of vtium.conf's sections](head-file.md#terms) define the keys that act on heads and consoles. These keys are the roaming chord, the console chords, and the rescue key.
