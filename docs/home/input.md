# Keyboards and mice

vtium reads keyboards and mice from the kernel's input devices. It uses the keyboard layout your system already sets for the console, and picks up a change to that layout. The rules in vtium.conf send each device to a head, and a rule can also make a keyboard roam, so that it types into whichever head is active.

## What programs get

Programs receive keys in xterm's encodings or through the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). With the kitty protocol, a program can tell every key and modifier apart and see when a key is released. vtium reports mouse clicks, drags, and motion in the SGR, urxvt, and pixel encodings, along with focus changes and bracketed pastes.

You select text with the mouse and paste the selection with the middle button or Shift+Insert. [Keys](../keys.md) lists the keys vtium keeps for itself.

vtium has no input method yet for composing Chinese, Japanese, or Korean text, and no support for Braille displays. Both are planned: see [the input method issue](https://github.com/vtium/vtium/issues/45) and [the Braille display issue](https://github.com/vtium/vtium/issues/46).

## Keys that act on the machine

No key reboots, powers off, or suspends the machine until you bind one with `key-reboot`, `key-power-off`, or `key-suspend` in vtium.conf. Bind them with care. `key-power-off` asks init to shut down, so services stop and filesystems are unmounted first. `key-reboot` shuts down the same way only when `/proc/sys/kernel/ctrl-alt-del` is 0; otherwise the kernel restarts at once. [Input](../features.md#input) describes these keys and the keyboard protocol in full.
