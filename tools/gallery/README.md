# gallery

`gallery` shows PNG pictures as a grid of thumbnails in your terminal, with each file's name under its picture. It is one bash script, and it runs in any terminal that draws kitty graphics: kitty, WezTerm, Ghostty and vtium.

Try it on a directory of pictures:

```sh
tools/gallery/gallery ~/Pictures
```

## How it works

`gallery` sends each PNG file to the terminal as it is, with the kitty graphics protocol, and tells the terminal the box of cells to draw it in. The terminal scales the picture into the box, so `gallery` needs no image library and no converter. It needs only bash and the tools every Linux system has: `stty`, `od`, `base64`, `tr`, `fold` and `sort`.

A directory stands for the files in it, in name order, and no arguments means the current directory. The same pictures give the same output every time. `gallery` sends PNG files only. It skips any other file, such as a JPEG, and names the files it skipped on one line. To show a JPEG, convert it to PNG first with a program such as ImageMagick.

Before it draws, `gallery` asks the terminal whether it draws kitty graphics, with a kitty graphics query (`a=q`), and how large its cells are in pixels, with XTWINOPS 16 (`CSI 16 t`). A terminal that does not answer the first question within a second gets one line saying it does not draw kitty graphics, and `gallery` exits with status 1. A terminal at the far end of a slow link, such as an SSH connection across the world, can take longer than a second to answer. You can give it more time with `-w`.

## How big each thumbnail is

Each thumbnail is N rows tall, where N is the `-s` size, and keeps its picture's shape. Its width in columns is the picture's width over its height, times N, times the cell's height over its width, rounded to the nearest column:

    columns = round(N × picture width ÷ picture height × cell height ÷ cell width)

A thumbnail wider than twice its height in pixels is made narrower, and fewer rows tall, to fit that limit. When the terminal does not report its cell size, or `--no-probe` skips the question, `gallery` takes the cells to be twice as tall as they are wide, the shape of most terminal fonts. Then a square picture is 2N columns wide.

The grid's columns are all as wide as the widest thumbnail and two more, and they are spread evenly across the terminal's width. Each thumbnail is centered in its column, and its file name is centered under it, cut to the column's width less one with an ellipsis. A column narrower than three cells takes no name.

`gallery` measures a name in characters, not in the columns they fill. A name with wide characters, such as Chinese or Japanese, which take two columns each, can run into the next column. In a locale that is not UTF-8, such as `C`, a name is measured in bytes, and a cut name drops the bytes past plain ASCII at the cut's end so that no character is split. A control character in a name, such as an escape or a newline, is shown as `?`, so a file name cannot send the terminal a command.

## Options

| Option | What it does |
|---|---|
| `-s N`, `--size N` | Makes each thumbnail N rows tall (8 by default), as [How big each thumbnail is](#how-big-each-thumbnail-is) describes. |
| `-n N` | Shows the first N pictures, and says how many more there are. |
| `-L`, `--no-labels` | Leaves out the file names. |
| `-w N`, `--wait N` | Waits up to N seconds for the terminal to answer its question, in place of one second. |
| `--no-probe` | Draws without asking the terminal first, and takes its cells to be twice as tall as they are wide. Use it when you know the terminal draws kitty graphics and want the same bytes on every run. |
| `-h`, `--help` | Shows the options. |

The grid fills the terminal's width, which `gallery` reads with `stty`. When there is no terminal to read, it uses `COLUMNS` and `LINES`, or 80 by 24.

## Putting it on your PATH

Copy the script into a directory on your `PATH`:

```sh
install -m 755 tools/gallery/gallery ~/.local/bin/gallery
```

Then `gallery ~/Pictures` works from anywhere. The script carries its license, MIT, in its first lines, so you can copy it on its own.

## Tests

`make test-tools` runs `tests/test_gallery.py`. It runs `gallery` on small pictures it writes itself and compares the bytes `gallery` writes with the bytes expected. It also runs `gallery` on a stand-in terminal (`tests/terminal.py`), which answers the questions `gallery` asks, to check what `gallery` does when a terminal draws kitty graphics and when it does not. The test that interrupts `gallery` while it waits for the answer runs it with `-w 30`. The test sends the interrupt only once it has read the question, and on a loaded machine that can take longer than the default second, by which time `gallery` would already have stopped waiting.
