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

## Size

`font-size` in `vtium.conf` sets the size in points, 1/72 of an inch, which vtium converts to pixels at the display's density. An outline frit may set its own starting size with a `size=` line. A `font-size` above 0 wins over the frit's `size=`. At `font-size=0`, the default, the frit's `size=` decides, and with none vtium picks a size from the display, such as 8x16 cells at 640x480. A bitmap font draws at its own cell or a whole multiple of it, since points do not apply to it.

## Choosing a font with vtium config

`vtium config tui`, run inside a vtium console, opens a full-screen tool that lists the frits and the built-in fonts. Each font you move the highlight to becomes the console's font at once, so you judge the real console in it:

```
$ vtium config tui
```

Up and Down move the highlight, Enter keeps the highlighted font for saving, Tab moves to the sizes, to vtium.conf's font keys and to the heads, `s` shows what a save changes, and `q` or Ctrl-C leaves and puts back the font the console had when the tool started. Where the console draws kitty graphics, a vtium built with the `images` feature, each size shows a picture of the font at that size, and the console's own font and grid stay as they were. Without pictures, a size shows only once it is saved.

Saving lists each line of vtium.conf that changes and the one command it runs, `sudo /usr/bin/vtium-config apply DRAFT`, which asks for your password once. Run as root, vtium-config takes no option that names a directory or a program, so that command can change only `/etc/vtium/vtium.conf`, `/etc/vtium/heads.conf` and the frits in `/etc/vtium/fonts`, each after vtium's own checks have accepted it, and you can allow exactly that command in a sudo rule. The command checks the changed files with `vtium check-config`, installs them, and keeps each old file beside the new one as `vtium.conf.bak` or `heads.conf.bak`; it changes only the lines that need it, so your comments stay. A font saved this way, like any change to vtium.conf, takes effect at the console's next start, and the console you saved from stays in the saved font meanwhile.

In a terminal that is not vtium, such as a desktop terminal, the tool shows the same lists and saves the same way, but sends the terminal no font, and says so. With `allow-font-ops` off, it says so and offers to turn the key on as part of the save.

## Switching and zooming

`vtium config set NAME` switches the console it runs in to the frit NAME, or to a built-in font, `unifont` or `unscii-8`:

```
$ vtium config set monoid
```

It writes xterm's font sequence, OSC 50, to its own terminal, and vtium takes it while the `allow-font-ops` key is on, as it is by default.

`vtium config set` first asks the terminal what it is, and sends nothing to a terminal that is not vtium, such as a desktop terminal; there it prints `failed`. Run it inside a vtium console.

Ctrl+= and Ctrl+- zoom the console on screen in and out, and so does Ctrl with the mouse wheel.

A font switch and a zoom last until you log out. When the session in a console ends, vtium reads `font-name`, `font-size` and `font-engine` from `vtium.conf` again and returns that console to the font and size they give before the next login prompt appears. So a font you saved with `vtium config` is the font the next login sees, and a font you only tried is gone. When a screen has several consoles, logging out of one console returns the font only if the switch came from that console, or if it is the only console left. Closing the console the switch came from with Ctrl+Super+Backspace returns the font too, and so does vtium hanging up a screen's sessions when its last display goes under `when-display-lost = hangup`. A font key that vtium cannot read is logged and skipped, as at the start. vtium reads the other keys of `vtium.conf` only when it starts again. Set `font-name` and `font-size` ([Size](#size)) for a font and size that last.
