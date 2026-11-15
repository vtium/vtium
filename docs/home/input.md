# Keyboards and mice

vtium reads keyboards and mice from the kernel's input devices. It uses the keyboard layout your system already sets for the console, and picks up a change to that layout. The rules in vtium.conf send each device to a head, and a rule can also make a keyboard roam, so that it types into whichever head is active.

## What programs get

Programs receive keys in xterm's encodings or through the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). With the kitty protocol, a program can tell every key and modifier apart and see when a key is released. vtium reports mouse clicks, drags, and motion in the SGR, urxvt, and pixel encodings, along with focus changes and bracketed pastes.

You select text with the mouse and paste the selection with the middle button or Shift+Insert.

## Still to come

An input method for composing Chinese, Japanese, or Korean text is still to come; see [the input method issue](https://github.com/vtium/vtium/issues/45). Support for Braille displays is planned too, in [the Braille display issue](https://github.com/vtium/vtium/issues/46).

## More about input

- [Keys](../keys.md): the keys vtium keeps for itself.
- [Input](../features.md#input) in What vtium does: the keyboard protocol, and the keys that reboot or power off the machine.
