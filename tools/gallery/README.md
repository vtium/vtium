# gallery

`gallery` shows PNG pictures as a grid of thumbnails in your terminal, with each file's name under its picture. It is one bash script, and it runs in any terminal that draws kitty graphics: kitty, WezTerm, Ghostty and vtium.

Try it on a directory of pictures:

```sh
tools/gallery/gallery ~/Pictures
```

## How it works

`gallery` sends each PNG file to the terminal as it is, with the kitty graphics protocol, and tells the terminal the box of cells to draw it in. The terminal scales the picture into the box, so `gallery` needs no image library and no converter. It needs only bash and the tools every Linux system has: `stty`, `od`, `base64`, `tr` and `sort`.

A directory stands for the files in it, in name order, and no arguments means the current directory. The same pictures give the same output every time. `gallery` sends PNG files only. It skips any other file, such as a JPEG, and names the files it skipped on one line. To show a JPEG, convert it to PNG first with a program such as ImageMagick.

Before it draws, `gallery` asks the terminal whether it draws kitty graphics and how large its cells are in pixels. A terminal that does not answer within a second gets one line saying it does not draw kitty graphics, and `gallery` exits with status 1. The size of the cells lets `gallery` give each picture a box of its own shape, so a square picture stays square.

## Options

| Option | What it does |
|---|---|
| `-s N`, `--size N` | Makes each thumbnail N rows tall (8 by default). A square picture is about 2N columns wide, and no thumbnail is more than twice as wide as it is tall. |
| `-n N` | Shows the first N pictures, and says how many more there are. |
| `-L`, `--no-labels` | Leaves out the file names. |
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

`make test-tools` runs `tests/test_gallery.py`. It runs `gallery` on small pictures it writes itself and compares the bytes `gallery` writes with the bytes expected. It also runs `gallery` on a stand-in terminal (`tests/terminal.py`), which answers the questions `gallery` asks, to check what `gallery` does when a terminal draws kitty graphics and when it does not.
