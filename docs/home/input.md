# Keyboards and mice

Your keyboard types with the layout your system already uses for the console, and programs get the mouse and the full keyboard. You don't set anything up for that.

![A console screen in a smooth monospaced font: a heading, a list of four web addresses, and a line that names the ten pointer shapes vtium draws. The mouse pointer, a white hand with a raised finger and a dark outline, points at the second address, which has a dashed underline while the pointer is over it.](../showcase/pointer.png)

*[The showcase](../showcase/input-and-pointer.md) has more screens like this one.*

Keyboards and mice are read from the kernel's input devices (evdev), and libxkbcommon turns keys into characters. A change to the console's layout takes effect too. The rules in vtium.conf send each device to a head, vtium's name for a terminal on one or more displays. A rule can also make a keyboard roam, so that it types into whichever head is active.

## What programs get

Programs receive keys in xterm's encodings or through the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). With the kitty protocol, a program can tell every key and modifier apart and see when a key is released. Mouse clicks, drags, and motion reach programs in the SGR, urxvt, and pixel encodings, along with focus changes and bracketed pastes.

You select text with the mouse and paste the selection with the middle button or Shift+Insert. Shift+PageUp and Shift+PageDown scroll back through earlier output. [Keys](../keys.md) lists every key vtium keeps for itself.

Two input features are still missing: an input method for composing Chinese, Japanese, or Korean text, and support for Braille displays. Each has an issue of its own, [the input method issue](https://github.com/vtium/vtium/issues/45) and [the Braille display issue](https://github.com/vtium/vtium/issues/46).

## Keys that act on the machine

No key reboots, powers off, or suspends the machine until you bind one with `key-reboot`, `key-power-off`, or `key-suspend` in vtium.conf. Bind them with care. `key-power-off` asks init to shut down, so services stop and filesystems are unmounted first. `key-reboot` shuts down the same way only when `/proc/sys/kernel/ctrl-alt-del` is 0; otherwise the kernel restarts at once, with no clean shutdown. More on these keys and the keyboard protocol is under [Input](../features.md#input).
