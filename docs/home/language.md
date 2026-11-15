# The terminal's language

Programs send a terminal control sequences: short codes mixed into the text they print. vtium builds on [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), the document known as ctlseqs.

From DEC's VT terminals, vtium implements the editing features that the [VT510 manual](https://vt100.net/docs/vt510-rm/contents.html) describes. They include left and right margins, column insert and delete, rectangular area operations, and protected characters. Double-width and double-height lines are partly implemented, and [the double-size lines issue](https://github.com/vtium/vtium/issues/61) tracks the rest.

Among the newer protocols, vtium supports sixel, the kitty graphics and keyboard protocols, OSC 8 links, OSC 133 prompt marks, OSC 52 copy, synchronized output, and resize reports. The kitty text sizing protocol is still to come; see [the text sizing issue](https://github.com/vtium/vtium/issues/60).

## How vtium interprets a sequence

- Each sequence follows its defining specification.
- Where two specifications give one sequence different meanings, vtium's feature table records which meaning vtium uses. For example, `CSI s` sets the left and right margins (DECSLRM) while mode 69 is set.
- Where a specification leaves a choice open, vtium's own tests settle it, and the choice is written down.

Above all of these comes the reply rule. vtium builds every answer to a query from its own state, so a program's output can never make vtium type into your shell. [The reply rule](security-and-system.md#the-reply-rule) shows what that protects.

## What vtium aims to support

vtium aims to support the capabilities that xterm's control sequences and other terminals' documentation both define, plus others chosen because they are useful on a console. [Modes](../modes.md) lists every mode vtium recognizes, and [The terminal](../features.md#the-terminal) covers the rest of what it implements.
