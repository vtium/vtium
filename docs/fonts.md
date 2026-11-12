# Choose a font

This guide sets the font vtium draws in. [Fonts, generated at run time](concepts/fonts.md) explains how a font reaches the screen.

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user.

vtium draws fonts from frits. A frit (a font set file) is a short text file, `/etc/vtium/fonts/NAME.vtfrit`, that lists a font's sources in order. The `font-name` key names the frit by its NAME, and the `font-frits` key names another directory than `/etc/vtium/fonts`. The console parses no font file itself. It starts the glyph helper, `vtium-config serve`, which reads the frit's sources and sends the console ready-made glyphs for its cell size.

With no `font-name` set, vtium draws the console font that `vconsole.font=` on the kernel command line or `FONT=` in `/etc/vconsole.conf` names, and with neither, its built-in fonts, Unifont and Unscii-8.

## An outline font

This frit draws Monoid Nerd Font Mono, with Noto Color Emoji for the emoji that Monoid lacks. Save it as `/etc/vtium/fonts/monoid.vtfrit`:

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

The helper tries the sources in order, and each later source draws only the characters the earlier ones lack. The built-in Unifont and Unscii-8 draw every character that no source has. `name=` names the frit, and the file's name, `monoid`, is the default. `license=` records the font's license.

`source=font:FAMILY` finds a kbd console font of that name first. Without one, it finds an outline font file, `.ttf`, `.otf`, `.ttc` or `.otc`, whose own name table gives that family in any language, as `fc-scan --format '%{family}\n' FILE` prints it, with case, spaces, `-` and `_` ignored. The file may have any name, so `font:Monoid Nerd Font Mono` finds `MonoidNerdFontMono-Regular.ttf`, and `font:JetBrains Mono` finds the variable font `JetBrainsMono[wght].ttf` too. A file named for the family, such as `MonoidNerdFontMono.ttf` or `MonoidNerdFontMono-Regular.ttf`, is taken first when its name table gives the family, whatever its style. Otherwise, when several files give the family, the helper takes the first whose subfamily, the style its name table names, is `Regular`, and else the first of the family, where first means in the font directory listed first, then by path. [crates/vtium-config/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium-config/README.md#frits) lists every source form and frit key.

When vtium runs as root, as its unit runs it, it starts the helper as the user `nobody`. The helper looks for outline fonts only under `/usr/share/fonts` and `/usr/local/share/fonts`. A font installed in your home directory, for example under `~/.local/share/fonts`, never reaches it, so copy that font into `/usr/local/share/fonts`:

```
$ sudo install -Dm644 -t /usr/local/share/fonts/monoid-nerd-font-mono ~/.local/share/fonts/monoid-nerd-font-mono/*.ttf
# fc-cache -f
```

The first line runs through sudo from your own shell on purpose: your shell turns `~` and `*.ttf` into your home directory's files before sudo runs. In a root shell, `~` is root's home directory. The helper reads the font directories itself and uses no fontconfig cache, and `fc-cache -f` updates that cache for the other programs on the machine.

## A console font

A kbd console font from `/usr/share/kbd/consolefonts` needs no frit. A name that no frit bears is looked up as a kbd font, so this line draws `eurlatgr`:

```
font-name=eurlatgr
```

## Superscripts and subscripts in a bitmap font

A program can ask for superscript and subscript text, as in x² or H₂O written with ordinary digits. vtium draws each superscript and subscript character smaller, against the top or the bottom of its row. In a bitmap font, vtium draws them from a smaller size of the same font where it finds one, so that every pixel stays fully on or off and keeps the font's design. When vtium draws a bitmap font at two or more times its size, as on a high-DPI display, it uses the font itself at a smaller multiple. A CPI font's other heights work too. You can also name a smaller font of the same family with a `script-source=` line under the font's `source=` line. For example, this frit draws Terminus at 16 pixels and its superscripts and subscripts in Terminus at 12:

```
source=kbd:ter-116n
script-source=kbd:ter-112n
```

The smaller font's characters must be from half to three quarters as high as the font's own. `vtium-config check /etc/vtium/fonts/NAME.vtfrit` tells you when a `script-source=` font is outside that range. A bitmap font with no smaller size draws its superscripts and subscripts scaled to two thirds, and an outline font draws them at two thirds of its size. `vtium-config preview FONT` and the font view of `vtium-config` end their preview with a line of superscripts and subscripts, x2 + y2 = z2 and H2O with the digits small, and say how the font makes them. See [the superscript and subscript issue](https://github.com/vtium/vtium/issues/37).

## Size

`font-size` in `vtium.conf` sets the size in points, 1/72 of an inch, which vtium converts to pixels at the display's density. An outline frit may set its own starting size with a `size=` line. A `font-size` above 0 wins over the frit's `size=`. At `font-size=0`, the default, the frit's `size=` decides, and with none vtium picks a size from the display, such as 8x16 cells at 640x480. A bitmap font draws at its own cell or a whole multiple of it, since points do not apply to it.

## Choosing a font with vtium config

`vtium config tui`, run inside a vtium console, opens vtium-config's full-screen tool. Its Fonts view lists the frits and the built-in fonts. Where the console draws kitty graphics, the tool shows each font you move the highlight to as a picture, and leaves the console's own font alone; without pictures, it says the cell and the grid in words. `t` tries the highlighted font on the console itself:

```
$ vtium config tui
```

The tool opens on its Flow view. Press Tab twice to reach Fonts, or click Fonts in the top row. Up and Down move the highlight, and Enter keeps the highlighted font: the tool writes it as `font-name` into your draft, your working copy of vtium.conf. `t` switches the console to the highlighted font for 10 seconds: `k` keeps it, and any other key, or the end of the 10 seconds, puts back the font the console drew before, so a font that leaves the screen unreadable undoes itself. The picture beside the list is a specimen of every character the font covers, at its real size, in which a character the font itself lacks shows grey where the built-in font draws it and as an empty box where nothing does, and the line under the list gives the grid the font makes on the display; `s` switches the picture to the console drawn in the font, and `vtium config preview FONT` prints the same specimen. `n` makes a new frit and `r` marks a frit to be removed. `i` installs the draft, and `q` or Ctrl-C leaves and puts back the font the console had when the tool started. The Looks view shows each size the same way, as a picture of the display in the font at that size; the console draws pictures when vtium is built with the `images` feature. To give one head a font of its own, set the frit as the `font-name` of that head's look ([The sections of vtium.conf](head-file.md#reading-the-sections)).

Install asks first, counting the lines of vtium.conf that change and the frits that go in with it, and then runs the one command that needs root, `sudo /usr/bin/vtium-config apply DIR`, which asks for your password once. That command checks each frit and the new vtium.conf with vtium's own checks, writes the frits first and vtium.conf after them, and keeps each old file beside the new one as `NAME.bak`, such as `vtium.conf.bak`. vtium-config changes only the lines that need it, so your comments stay. [When vtium-config asks for sudo](configure.md#when-vtium-config-asks-for-sudo) describes each step. A font installed this way, like any change to vtium.conf, takes effect at the console's next start, and the console you installed from stays in that font meanwhile.

In a terminal that is not vtium, such as a desktop terminal, the tool shows the same lists and installs the same way, but sends the terminal no font, and says so. With `allow-font-ops` off, it says so and offers to turn the key on in the draft.

`vtium config new-frit` and `vtium config remove-frit` make and remove a frit without the full-screen tool. Run as you, each writes an install bundle and prints the `sudo` command that installs it; run as root, each installs at once.

## Switching and zooming

`vtium config use-frit NAME` switches the console it runs in to the frit NAME, or to a built-in font, `unifont` or `unscii-8`:

```
$ vtium config use-frit monoid
```

It writes xterm's font sequence, OSC 50, to its own terminal, and vtium takes it while the `allow-font-ops` key is on, as it is by default.

`vtium config use-frit` first asks the terminal what it is, and sends nothing to a terminal that is not vtium, such as a desktop terminal; there it prints `failed`. Run it inside a vtium console.

Ctrl+= and Ctrl+- zoom the console on screen in and out, and so does Ctrl with the mouse wheel.

A font switch and a zoom last until you log out. When the session in a console ends, vtium reads `font-name`, `font-size`, `font-engine` and `font-line-spacing` from `vtium.conf` again and returns that console to the font, size and line spacing they give before the next login prompt appears. So a font you installed with `vtium config` is the font the next login sees, and a font you only tried is gone. When a screen has several consoles, logging out of one console returns the font only if the switch came from that console, or if it is the only console left. Closing the console the switch came from with Ctrl+Super+Backspace returns the font too, and so does vtium hanging up a screen's sessions when its last display goes under `when-display-lost = hangup`. A font key that vtium cannot read is logged and skipped, as at the start. vtium reads the other keys of `vtium.conf` only when it starts again. Set `font-name` and `font-size` ([Size](#size)) for a font and size that last.
