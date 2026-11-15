# The terminal's language

Programs send a terminal control sequences: short codes mixed into the text they print. vtium builds on [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), the document known as ctlseqs.

From DEC's VT terminals, vtium implements the editing features that the [VT510 manual](https://vt100.net/docs/vt510-rm/contents.html) describes. They include left and right margins, column insert and delete, rectangular area operations, and protected characters. Double-width and double-height lines are partly implemented, and [the double-size lines issue](https://github.com/vtium/vtium/issues/61) tracks the rest.

Among the newer protocols, vtium supports sixel, the kitty graphics and keyboard protocols, OSC 8 links, OSC 133 prompt marks, OSC 52 copy, synchronized output, and resize reports. The kitty text sizing protocol is still to come; see [the text sizing issue](https://github.com/vtium/vtium/issues/60).

## The rules vtium follows

Four rules decide what a sequence does:

- Each sequence follows its defining specification.
- vtium adopts a capability when both xterm's control sequences and another terminal's documentation define it, when the maintainer asks for it, or when the console has a use for it.
- Where two specifications give one sequence different meanings, vtium's feature table records which meaning vtium uses.
- Where a specification leaves a choice open, vtium's own tests settle it, and the choice is written down.

One rule overrides all four: a program's output never reads from or controls your machine. vtium builds every reply from its own state, so text a program prints cannot type into the program that reads the replies.

## More about the language

- [Modes](../modes.md): every mode vtium recognizes.
- [The terminal](../features.md#the-terminal) in What vtium does: everything else vtium implements.
