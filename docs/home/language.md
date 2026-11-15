# The terminal's language

Programs steer a terminal with control sequences, which are short codes mixed into the text the programs print. vtium's base is [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), a document known as ctlseqs.

vtium also carries out the editing features of DEC's VT terminals, as the [VT510 manual](https://vt100.net/docs/vt510-rm/contents.html) describes them. These features include left and right margins, inserting and deleting columns, rectangular area operations, and protected characters. Double-width and double-height lines are partly built. See [the double-size lines issue](https://github.com/vtium/vtium/issues/61).

Of the newer protocols, vtium supports sixel, the kitty graphics and keyboard protocols, OSC 8 links, OSC 133 prompt marks, and OSC 52 copy. vtium also supports synchronized output and reports of a resize. The kitty text sizing protocol is still to come. See [the text sizing issue](https://github.com/vtium/vtium/issues/60).

## The rules vtium follows

vtium follows four rules to decide what a sequence does:

- Each sequence does what the specification that defines the sequence says.
- vtium takes in a capability when xterm's control sequences and another terminal's documentation both define the capability. vtium also takes in a capability that the maintainer asks for, or one that the console has a use for.
- Where two specifications give one sequence different meanings, vtium's table of features says which meaning vtium takes.
- Where a specification leaves a choice open, vtium's own tests decide, and the choice is written down.

One rule comes before the four rules above. A program's output never reads from your machine or steers your machine. vtium builds every reply from vtium's own state, so text that a program prints cannot type into the program that reads the replies.

## More about the language

- [Modes](../modes.md) lists each mode vtium recognizes.
- [The terminal](../features.md#the-terminal), in What vtium does, lists the rest of what vtium carries out.
