# The terminal's language

Programs control the screen by mixing short codes into the text they print: move the cursor, switch to bold, clear a line, show a picture. vtium follows the published specifications for those codes.

![A console screen of color swatches: the 6 by 6 by 6 color cube and the gray ramp of the 256-color palette, the 16 numbered standard colors, smooth gradients through every hue and from black to white, and nine rows of 16 swatches, one row for each of vtium's named palettes.](../showcase/color.png)

*[The showcase](../showcase/color.md) has more screens like this one.*

The codes are control sequences. The base is [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), the document known as ctlseqs.

From DEC's VT terminals come the editing features that the [VT510 manual](https://vt100.net/docs/vt510-rm/contents.html) describes. They include left and right margins, column insert and delete, rectangular area operations, and protected characters. Double-width and double-height lines are supported too, in bitmap and outline fonts.

Among the newer protocols, vtium supports sixel, the kitty graphics and keyboard protocols, OSC 8 links, OSC 133 prompt marks, OSC 52 copy, synchronized output, and resize reports. The kitty text sizing protocol is planned; see [the text sizing issue](https://github.com/vtium/vtium/issues/60).

## How vtium interprets a sequence

- Each sequence follows its defining specification.
- Where two specifications give one sequence different meanings, vtium's feature table records which meaning vtium uses. For example, `CSI s` sets the left and right margins (DECSLRM) while mode 69 is set.
- Where a specification leaves a choice open, vtium's own tests settle it, and the choice is written down.

Those three rules say what each sequence does. One more rule limits all of them, whatever a specification allows. When a sequence asks the terminal a question, vtium builds the answer from its own state, so a program's output can never make vtium type into your shell. [The reply rule](security-and-system.md#the-reply-rule) shows what that protects.

## What vtium aims to support

The aim is the set of capabilities that xterm's control sequences and other terminals' documentation both define, plus others chosen because they are useful on a console. Every mode vtium recognizes is in [Modes](../modes.md), and the rest of what it implements is under [The terminal](../features.md#the-terminal).
