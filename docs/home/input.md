# Keyboards and mice

vtium reads each keyboard and mouse from the kernel's input devices. vtium uses the keyboard layout that your system already sets for the console, and follows a change to that layout. vtium.conf's rules send each device to a head. A rule can also make a keyboard roam, so the keyboard types into whichever head is active.

## What programs get

Programs get keys in xterm's encodings, or in the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). Under the kitty keyboard protocol, a program can tell every key and modifier apart, and can hear when a key is released. vtium reports mouse clicks, drags, and motion in the SGR, urxvt, and pixel encodings. vtium also reports focus changes and bracketed pastes.

You select text with the mouse, and you paste the selection with the middle button or Shift+Insert.

## Still to come

An input method for composing Chinese, Japanese, or Korean text is still to come. See [the input method issue](https://github.com/vtium/vtium/issues/45). Braille displays are planned too. See [the Braille display issue](https://github.com/vtium/vtium/issues/46).

## More about input

- [Keys](../keys.md) lists the keys vtium keeps for itself.
- [Input](../features.md#input), in What vtium does, describes the keyboard protocol and the keys that reboot or power off the machine.
