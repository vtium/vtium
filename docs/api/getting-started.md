# Getting started

vtium-term keeps a terminal's state. A program that embeds it feeds it the bytes a child program writes, and reads back the screen, the cursor and the replies the child asked for. This page does both with vtium-term alone, with no display, VT or pty.

vtium-term's tests compile and run the code on this page, so the code works with the vtium-term of the same commit.

## Feed bytes and read the screen

```rust
use vtium_term::{Glyph, Line, Terminal, Width};

/// A line's text: each cell's character, nothing for the right half of a
/// wide character, and the trailing blanks dropped.
fn text(line: &Line) -> String {
    let mut out = String::new();
    for &cell in line.cells() {
        match line.glyph(cell) {
            Glyph::Char(c) => out.push(c),
            Glyph::Cluster(s) => out.push_str(s),
            Glyph::Empty if cell.width() == Width::WideTail => {}
            Glyph::Empty => out.push(' '),
        }
    }
    out.trim_end().to_owned()
}

// A terminal of 20 columns by 3 rows, which keeps at most 100 lines of
// scrollback.
let mut term = Terminal::new(20, 3, 100);

// The bytes a program writes: a word, a carriage return and a line feed,
// and a second word in bold (SGR 1, then SGR 0 to end it).
term.input(b"Hello\r\n\x1b[1mworld\x1b[0m");

let screen = term.screen();
assert_eq!(text(screen.line(0).unwrap()), "Hello");
assert_eq!(text(screen.line(1).unwrap()), "world");

// The cursor's column and row, counted from 0: just after "world".
assert_eq!(screen.cursor(), (5, 1));
```

`Terminal::input` returns how many bytes it took. It takes fewer than all of them only in a few cases, such as at the end of a synchronized update, and its reference lists them. A host feeds the bytes it did not take in its next call.

## Read the replies

A program asks the terminal questions with escape sequences, and the terminal answers by writing bytes back to the program. vtium-term keeps those answers until the host takes them and writes them to the program.

```rust
use vtium_term::Terminal;

let mut term = Terminal::new(20, 3, 100);
term.input(b"Hello");

// DSR 6 asks where the cursor is. The reply counts rows and columns from 1.
term.input(b"\x1b[6n");
assert_eq!(term.take_replies(), b"\x1b[1;6R");
```

## Where to go next

The [reference of vtium-term](https://vtium.github.io/vtium/api/vtium_term/) documents `Terminal`, `Screen` and `Frame`, the snapshot a renderer reads. [Conventions](conventions.md) gives the rules every crate's interface follows.
