# Choose a font

This guide shows you how to set the font that vtium renders text in. [Fonts, generated at run time](concepts/fonts.md) explains how a font reaches the screen.

In the commands below, `#` marks a command you run as root, for example with sudo. `$` marks a command you run as your own user.

vtium takes its fonts from font set files, called frits. A frit is a short text file, `/etc/vtium/fonts/NAME.vtfrit`, that lists a font's sources in order. The `font-name` key in vtium.conf chooses a frit by its NAME. The `font-frits` key names a directory to use in place of `/etc/vtium/fonts`.

The console does not read any font file itself. Instead, it starts the font helper, `vtium-config serve`. The font helper reads the frit's sources and turns them into ready-made glyphs, the images of the characters. It sends the console glyphs at the size of the console's character cell, the box on the screen that holds one character.

When `font-name` is not set, vtium uses the console font that the system names. That name comes from `vconsole.font=` on the kernel command line, or from `FONT=` in `/etc/vconsole.conf`. When neither names a font, vtium uses its built-in fonts, Unifont and Unscii-8.

## An outline font

An outline font stores each character as a shape, so it can be rendered at any size. This frit uses Monoid Nerd Font Mono. For the emoji that Monoid does not have, it uses Noto Color Emoji. Save it as `/etc/vtium/fonts/monoid.vtfrit`:

```
name=monoid
license=MIT
source=font:Monoid Nerd Font Mono
source=font:Noto Color Emoji
```

Then set it in `/etc/vtium/vtium.conf`:

```
font-name=monoid
```

The font helper tries the sources in order. A later source supplies only the characters that the earlier sources do not have. The built-in Unifont and Unscii-8 supply every character that no source has.

The `name=` line gives the frit its name. Without it, the frit takes the file's name, here `monoid`. The `license=` line records the font's license.

A `source=font:FAMILY` line looks in two places. First, the font helper looks for a kbd console font with that name. A kbd console font is one of the Linux console's own fonts, from the kbd package. If there is none, the font helper looks for an outline font file: a `.ttf`, `.otf`, `.ttc` or `.otc` file. A file matches when its own name table gives that family name, in any language. The family name is the one that `fc-scan --format '%{family}\n' FILE` prints. The match ignores case, spaces, `-` and `_`.

The file itself may have any name. For example, `font:Monoid Nerd Font Mono` finds `MonoidNerdFontMono-Regular.ttf`. `font:JetBrains Mono` finds the variable font `JetBrainsMono[wght].ttf` too.

Several files can give the same family. The font helper then chooses one in this order:

1. A file named for the family, such as `MonoidNerdFontMono.ttf` or `MonoidNerdFontMono-Regular.ttf`, whose name table gives the family. The font helper takes this file whatever its style.
2. Otherwise, the first file whose subfamily is `Regular`. The subfamily is the style that the file's name table names.
3. Otherwise, the first file of the family.

Here, "first" means the file in the font directory that is listed first, and within that directory, the first by path.

[crates/vtium-config/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium-config/README.md#frits) lists every source form and frit key.

vtium's service unit runs vtium as root. When vtium runs as root, it starts the font helper as the user `nobody`. The font helper looks for outline fonts only under `/usr/share/fonts` and `/usr/local/share/fonts`. It never sees a font installed in your home directory, for example under `~/.local/share/fonts`. To use such a font, copy it into `/usr/local/share/fonts`:

```
$ sudo install -Dm644 -t /usr/local/share/fonts/monoid-nerd-font-mono ~/.local/share/fonts/monoid-nerd-font-mono/*.ttf
# fc-cache -f
```

The first line runs through sudo from your own shell on purpose. Your shell turns `~` and `*.ttf` into the files in your home directory before sudo runs. In a root shell, `~` would be root's home directory. The font helper reads the font directories itself and does not use the fontconfig cache. The second line, `fc-cache -f`, updates that cache for the other programs on the machine.

## A console font

A kbd console font from `/usr/share/kbd/consolefonts` needs no frit. When no frit has the name that `font-name` gives, vtium looks the name up as a kbd font. So this line makes vtium use `eurlatgr`:

```
font-name=eurlatgr
```

## Superscripts and subscripts in a bitmap font

A program can ask for superscript and subscript text. For example, it can write x2 or H2O with ordinary digits and ask for the digit to be raised or lowered, as in x² or H₂O. vtium renders each superscript and subscript character smaller, against the top or the bottom of its row.

A bitmap font stores each character as a fixed grid of pixels. In a bitmap font, vtium takes superscripts and subscripts from a smaller size of the same font, where it finds one. That way every pixel stays fully on or off, and the characters keep the font's design. vtium finds a smaller size in these places:

- When vtium shows a bitmap font at two or more times its size, as on a high-DPI display, it uses the same font at a smaller multiple.
- A CPI font file holds several heights of the same font, and vtium can use one of the other heights.
- You can name a smaller font of the same family with a `script-source=` line under the font's `source=` line.

For example, this frit uses Terminus at 16 pixels, and Terminus at 12 pixels for its superscripts and subscripts:

```
source=kbd:ter-116n
script-source=kbd:ter-112n
```

The smaller font's characters must be from half to three quarters as high as the main font's characters. `vtium-config check /etc/vtium/fonts/NAME.vtfrit` tells you when a `script-source=` font is outside that range.

A bitmap font with no smaller size shows its superscripts and subscripts scaled to two thirds. An outline font renders them at two thirds of its size.

`vtium-config preview FONT` ends its preview with a line of superscripts and subscripts, and so does the font view of `vtium-config`. That line shows x2 + y2 = z2 and H2O with the digits small. Both also say how the font makes them. See [the superscript and subscript issue](https://github.com/vtium/vtium/issues/37).

## Size

The `font-size` key in `vtium.conf` sets the size in points. A point is 1/72 of an inch. vtium converts points to pixels at the display's density. A frit for an outline font may set its own starting size with a `size=` line. vtium chooses the size like this:

- When `font-size` is above 0, it wins over the frit's `size=`.
- When `font-size` is 0, the default, the frit's `size=` sets the size.
- When `font-size` is 0 and the frit has no `size=`, vtium picks a size from the display. For example, it uses 8×16 character cells at 640×480.

Points do not apply to a bitmap font. A bitmap font renders at its own character cell size, or at a whole multiple of it.

## Choosing a font with vtium config

Run this command inside a vtium console to open vtium-config's full-screen tool:

```
$ vtium config tui
```

The tool's Fonts view lists the frits and the built-in fonts, with a preview beside the list. Moving the highlight never changes the console's font. What the preview shows depends on where the tool runs:

- In a vtium console with the `allow-font-ops` key on, the preview is text that the console draws in the font it uses now. To see the highlighted font there, try it on the console itself with `t`. The preview text then shows that font.
- Anywhere else, the preview shows the highlighted font. If the terminal can show kitty graphics images, the preview is an image. Otherwise, the tool describes the character cell and the grid in words. The grid is the number of columns and rows of characters that fit on the display.

The tool opens on its Flow view. To reach Fonts, press Tab twice, or click Fonts in the top row. In the Fonts view, these keys work:

- Up and Down move the highlight.
- Enter keeps the highlighted font. The tool writes it as `font-name` into your draft. The draft is your working copy of vtium.conf.
- `t` switches the console to the highlighted font for 10 seconds. Press `k` to keep it. Any other key puts back the font the console used before, and so does the end of the 10 seconds. So a font that makes the screen unreadable undoes itself.
- `s` in a vtium console with `allow-font-ops` on saves the font the console shows now into your draft. Elsewhere, `s` switches the image between the specimen and the console rendered in the font.
- `n` makes a new frit.
- `r` marks a frit to be removed.
- `i` installs the draft.
- `q` or Ctrl-C leaves the tool and puts back the font the console had when the tool started.

An image preview is a specimen, a sample of every character the font covers, at its real size. A character that the font itself does not have shows in grey where the built-in font supplies it. It shows as an empty box where no font supplies it. The line under the list gives the grid that the font makes on the display. `vtium config preview FONT` prints the same specimen.

The Looks view previews each size the same way. In a vtium console with `allow-font-ops` on, its preview is a sample screen that the console draws as text. Elsewhere, its preview is an image of the display in the font at that size. A vtium console shows images when vtium is built with the `images` feature.

A head is one terminal of vtium, on one or more displays, and a look is the set of font settings a head uses. To give one head a font of its own, set the frit as the `font-name` of that head's look. See [The sections of vtium.conf](head-file.md#reading-the-sections) for details.

Before it installs, the tool asks you to confirm. It tells you how many lines of vtium.conf change, and how many frits it installs with them. Then it runs the one command that needs root, `sudo /usr/bin/vtium-config apply DIR`. sudo asks for your password once.

That command checks each frit and the new vtium.conf with vtium's own checks. It writes the frits first, and vtium.conf after them. It keeps each old file beside the new one as `NAME.bak`, such as `vtium.conf.bak`. vtium-config changes only the lines that need to change, so your comments stay. [When vtium-config asks for sudo](configure.md#when-vtium-config-asks-for-sudo) describes each step.

A font installed this way takes effect at the console's next start, like any change to vtium.conf. Until then, the console you installed from stays in the font you installed.

You can also run the tool in a terminal that is not vtium, such as a desktop terminal. There it shows the same lists and installs the same way. It does not send that terminal a font, and it tells you so. When the `allow-font-ops` key is off, the tool tells you that too, and offers to turn the key on in the draft.

`vtium config new-frit` makes a frit, and `vtium config remove-frit` removes one, without the full-screen tool. When you run either as your own user, it writes an install bundle and prints the `sudo` command that installs it. When you run either as root, it installs the change at once.

## Switching and zooming

`vtium config use-frit NAME` switches the console it runs in to the frit NAME. NAME can also be a built-in font, `unifont` or `unscii-8`:

```
$ vtium config use-frit monoid
```

The command writes xterm's font sequence, OSC 50, to its own terminal. vtium acts on that sequence while the `allow-font-ops` key is on, as it is by default.

`vtium config use-frit` first asks the terminal what it is. It sends nothing to a terminal that is not vtium, such as a desktop terminal, and prints `failed` there. Run it inside a vtium console.

Ctrl+= zooms the console on screen in, and Ctrl+- zooms it out. Ctrl with the mouse wheel zooms too.

A font switch and a zoom last until you log out. When the session in a console ends, vtium reads `font-name`, `font-size`, `font-engine` and `font-line-spacing` from `vtium.conf` again. It returns that console to the font, size and line spacing those keys give, before the next login prompt appears. So the next login sees the font you installed with `vtium config`, and a font you only tried is gone.

A screen can have several consoles. Logging out of one of them returns the font in only two cases. In the first, the switch came from that console. In the second, that console is the only one left. Two other events return the font too:

- You close the console the switch came from with Ctrl+Super+Backspace.
- vtium hangs up a screen's sessions because the screen's last display is gone, under `when-display-lost = hangup`.

When vtium cannot read one of these font keys, it logs the key and skips it, as it does at start. vtium reads the other keys of `vtium.conf` only when it starts again. For a font and size that last, set `font-name` and `font-size`. See [Size](#size) for details.
