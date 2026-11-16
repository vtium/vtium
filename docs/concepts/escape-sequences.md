# Escape sequences

A program in a terminal writes text, and the terminal shows it. The same stream of bytes also carries instructions: move the cursor, color the next word red, clear the screen, report the screen's size. These instructions are escape sequences and control characters. This page explains how they are put together, so that you can read the [reference page for each sequence](../sequences/index.md) that vtium interprets.

## The screen, the cursor and the pen

vtium shows a grid of character cells, such as 80 columns and 25 rows. Each cell holds one character, or half of a wide one such as a Chinese character or an emoji. The cursor marks the cell where the next character goes. When a program writes a letter, vtium puts it in the cursor's cell and moves the cursor one cell to the right. At the end of a row, the text wraps to the next row, and at the bottom of the screen the rows scroll up.

Each cell also keeps how its character looks: its foreground and background colors, and attributes such as bold, underline and italic. vtium holds the current look in what this documentation calls the pen. A sequence that sets bold changes the pen, and every character written after it takes the pen's look until another sequence changes it.

Colors come in three kinds. The 16 palette colors are the classic ones: black, red, green, yellow, blue, magenta, cyan and white, each in a normal and a bright form. The 256-color palette adds a 6×6×6 cube of colors and 24 grays. A direct color names its red, green and blue values, from 0 to 255 each. The palette colors can be changed, so a theme can make "red" a softer red.

## Control characters

The bytes 0x00 to 0x1F are the C0 control characters. A terminal acts on them instead of showing them. The common ones are these:

- CR, carriage return (0x0D), moves the cursor to the start of its row.
- LF, line feed (0x0A), moves the cursor down a row, and scrolls the screen at the bottom.
- BS, backspace (0x08), moves the cursor one cell to the left.
- HT, horizontal tab (0x09), moves the cursor to the next tab stop.
- BEL, bell (0x07), rings the bell.
- ESC, escape (0x1B), starts an escape sequence.

The bytes 0x80 to 0x9F are the C1 controls. Most programs write a C1 control as ESC and a second byte instead, so CSI is written ESC `[`.

## How a sequence is put together

An escape sequence starts with ESC. The bytes after it say which kind of sequence it is.

### ESC sequences

The shortest form is ESC and one or two more bytes. For example, ESC `7` saves the cursor's position, and ESC `8` restores it. ESC `#` `8` fills the screen with the letter E, a test pattern for lining up a display.

### CSI sequences

Most sequences are CSI sequences: control sequences, introduced by ESC `[`. A CSI sequence has three parts after ESC `[`:

1. The parameters, which are numbers separated by `;`. A parameter left out takes its default. Some sequences start the parameters with a private marker such as `?`, which picks a different set of functions.
2. Intermediate bytes, such as a space, `#` or `$`, which some sequences add before the end.
3. The final byte, a letter or a symbol, which names the function.

For example, `ESC [ 2 J` erases the whole screen. The parameter is 2 and the final byte is `J`. `ESC [ 10 ; 5 H` moves the cursor to row 10, column 5. `ESC [ 1 ; 31 m` makes the text that follows bold and red. `ESC [ ? 25 l` hides the cursor, with the private marker `?` and the final byte `l`.

The reference pages write these forms in the notation of [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), the document that most terminals follow. In it, CSI stands for ESC `[`, `Ps` for one number, `Pm` for a list of numbers, and `Pt` for text. So `CSI Ps J` is the form of the erase sequence above.

### OSC strings

An OSC string, an operating system command, starts with ESC `]`. A number follows, then `;` and some text, and ST ends the string. ST, the string terminator, is ESC `\`, and the bell character BEL may end an OSC string too. The number says what the text means. For example, `ESC ] 52 ; c ; aGVsbG8= ESC \` copies the word "hello", written in base64, to the clipboard. `ESC ] 8 ; ; https://example.com ESC \` starts a link.

### DCS and APC strings

A DCS string, a device control string, starts with ESC `P` and ends with ST. Like a CSI sequence, it has parameters and a final byte, and data follows them. Sixel images travel in DCS strings, and so do some queries, such as asking which terminfo capabilities vtium has.

An APC string, an application program command, starts with ESC `_` and ends with ST. vtium reads one kind, the kitty graphics protocol, which sends images in APC strings that start with `G`.

## Modes

Some CSI sequences turn a mode on or off. SM, set mode, ends in `h`, and RM, reset mode, ends in `l`. With the private marker `?`, they set the DEC private modes. For example, `ESC [ ? 1049 h` switches to the alternate screen, which full-screen programs such as editors use, and `ESC [ ? 1049 l` switches back. [Modes](../modes.md) lists every mode that vtium recognizes.

## Replies

Some sequences ask vtium a question, and vtium answers by writing a sequence back to the program, as if it had been typed. For example, `ESC [ 6 n` asks where the cursor is, and vtium answers `ESC [ row ; column R`. vtium builds each reply from its own state, never from the bytes of the question, so a program cannot make vtium type text of its choosing.

## What vtium does with a sequence it does not know

vtium reads every sequence to its end, even one it does not implement, so the bytes of an unknown sequence never appear on the screen. A sequence that vtium ignores has no effect. The [index of sequences](../sequences/index.md) shows, for each sequence, whether vtium supports it, ignores it or refuses it.
