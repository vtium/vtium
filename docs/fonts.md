# Choose a font

This guide shows you how to set the font that vtium renders text in. [Fonts, generated at run time](concepts/fonts.md) explains how a font reaches the screen.

In the commands below, `#` marks a command you run as root, for example with sudo. `$` marks a command you run as your own user.

vtium takes its fonts from font set files, called frits. A frit is a short text file, `/etc/vtium/fonts/NAME.vtfrit`, that lists a font's sources in order. The `font-name` key in vtium.conf chooses a frit by its NAME. The `font-frits` key names a directory to use in place of `/etc/vtium/fonts`.

The console does not read any font file itself. Instead, it starts the font helper, `vtium-config serve`. The font helper reads the frit's sources and turns them into ready-made glyphs, the images of the characters. It sends the console glyphs at the size of the console's character cell, the box on the screen that holds one character.

When `font-name` is not set, vtium uses the console font that the system names. That name comes from `vconsole.font=` on the kernel command line, or from `FONT=` in `/etc/vconsole.conf`. When neither names a font, vtium uses the font set `terminus`, bitmap Terminus at every size from 12 to 32 pixels. You can choose the outline set `cullet-mono` instead, which scales to any size and adds Nerd Fonts icons and color emoji. Every build installs both sets, with GNU Unifont for the characters that Terminus lacks.

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

vtium ships ready-made frits for Arabic, Hebrew, the Indic scripts, Thai, Lao and Vietnamese, whose marks need room above and below the letters. It also ships four frits for Chinese, Japanese and Korean, one for each region's forms of Han characters. See [Fonts for other scripts](fonts-for-scripts.md) for details.

vtium's service unit runs vtium as root. When vtium runs as root, it starts the font helper as the user `nobody`. The font helper looks for outline fonts under `/usr/share/fonts` and `/usr/local/share/fonts`, and renders only the fonts that are active. The `font-paths` key in vtium.conf replaces those two folders with a list of your own, in the order you give. A font installed in your home directory, for example under `~/.local/share/fonts`, is never active. To use such a font, install it as a system font, then activate it. [Choosing a font with vtium config](#choosing-a-font-with-vtium-config) gives the steps in order. To copy the files by hand instead, run these two lines, then activate the fonts as [Active fonts](#active-fonts) describes:

```
$ sudo install -Dm644 -t /usr/local/share/fonts/monoid-nerd-font-mono ~/.local/share/fonts/monoid-nerd-font-mono/*.ttf
# fc-cache -f
```

The first line runs through sudo from your own shell on purpose. Your shell turns `~` and `*.ttf` into the files in your home directory before sudo runs. In a root shell, `~` would be root's home directory. The font helper reads the font directories itself and does not use the fontconfig cache. The second line, `fc-cache -f`, updates that cache for the other programs on the machine.

## Active fonts

vtium renders only the fonts that are active. A font is active when `/etc/vtium/batch.conf` lists its file. The file holds one absolute path on each line, and a line that starts with `#` is a comment. Only root can change the file, so only root decides which fonts the console renders. The same file lists the other resources that a look uses, such as color schemes.

A fresh install activates the fonts that vtium ships in `/usr/share/vtium/fonts`: Terminus, GNU Unifont and Cullet Mono. It also activates the console font that `FONT=` names in `/etc/vconsole.conf`, the color schemes in `/usr/share/vtium/palettes`, and vtium's pointer file, `/usr/share/vtium/pointers.svg`. vtium uses this list while `batch.conf` does not exist, so the default font set `terminus` works with no setup. The built-in fonts are always active.

vtium-config's Fonts tab and `vtium config activate` write `batch.conf` for you, as [Choosing a font with vtium config](#choosing-a-font-with-vtium-config) shows. To activate another font by hand, add the path of each of its files to `batch.conf`. Once the file exists, vtium uses only the paths it lists. So when you create the file, list the fonts, color schemes and pointer file that vtium ships too:

```
$ ls -d /usr/share/vtium/fonts/* /usr/share/vtium/palettes/* /usr/share/vtium/pointers.svg | sudo tee /etc/vtium/batch.conf
$ echo /usr/local/share/fonts/monoid-nerd-font-mono/MonoidNerdFontMono-Regular.ttf | sudo tee -a /etc/vtium/batch.conf
```

When `/etc/vconsole.conf` names a console font, list that font's file as well, because your new file replaces the list that held it. For `FONT=ter-v16n`, kbd keeps the file in its `consolefonts` folder, which is `/usr/share/kbd/consolefonts` on many distributions:

```
$ ls /usr/share/kbd/consolefonts/ter-v16n.* | sudo tee -a /etc/vtium/batch.conf
```

vtium reads `batch.conf` when it starts and again each time a session ends. A font you activate or deactivate therefore takes effect at the next login, and vtium needs no restart.

A listed font file must also pass four checks:

- Root owns the file and every folder above it.
- Neither the group nor other users can write the file or any of those folders.
- Neither the file nor any folder on its path is a symbolic link.
- The filesystem that holds the file stays mounted while the system runs. A network filesystem, a FUSE filesystem, an automounted filesystem and a removable disk, such as a USB stick, fail this check.

vtium treats a font that is not listed, or that fails a check, as missing. vtium then renders the font's characters from the next font in the frit. It logs one line for the file, once a boot, which names the file and the check it failed.

A font in your home folder never renders, even when you name its folder in the `font-paths` key of `vtium.conf`. You own that folder, not root, so the font fails the first check. A program you run as yourself could change that font while the console uses it. Install the font as a system font instead, and then activate it, as [Choosing a font with vtium config](#choosing-a-font-with-vtium-config) shows. See [the trust rule issue](https://github.com/vtium/vtium/issues/438) and [the font-paths issue](https://github.com/vtium/vtium/issues/441).

## A console font

A kbd console font from `/usr/share/kbd/consolefonts` needs no frit. When no frit has the name that `font-name` gives, vtium looks the name up as a kbd font. So this line makes vtium use `eurlatgr`:

```
font-name=eurlatgr
```

## Layers that frits share

Several frits often want the same extra fonts, such as a symbol font for the icons in prompts and color emoji. A layer holds those fonts once, so each frit can add them by name. A layer is a file `/etc/vtium/fonts/layers/NAME.vtlayer`. Its first line says the layer's role: `script`, `symbols`, `emoji` or `fallback`. Its other lines are `source=` lines, each with the lines that describe that source, such as `cover=`. Save this layer as `/etc/vtium/fonts/layers/noto-emoji.vtlayer`:

```
role=emoji
source=font:Noto Color Emoji
```

A frit adds the layer with a `layer=` line. The layer's sources take the place of that line, so this frit tries Monoid first and then Noto Color Emoji:

```
source=font:Monoid Nerd Font Mono
layer=noto-emoji
```

A layer holds sources alone. A frit's own lines, such as `cell=` and `size=`, apply to the layer's sources too, and a layer cannot hold them. A frit holds one `symbols`, one `emoji` and one `fallback` layer at most, and as many `script` layers as you need. vtium reads a layer under the same ownership check as a frit. When a layer is missing or vtium does not read it, vtium draws the frit without that layer and logs why. `vtium check-config` reports it too.

`vtium-config new-frit` adds layers with `--layer`. It writes them in the order of their roles, whatever order you give them in: scripts, then symbols, then emoji, then the fallback.

```
$ vtium-config new-frit "Iosevka Term" --layer noto-emoji --layer nerd-symbols
```

## A bitmap font with an outline for larger cells

Some fonts come both as bitmap files, drawn pixel by pixel for a few sizes, and as an outline font of the same design. A `strike=` line under the outline font's `source=` line names one bitmap file. The bitmap files draw the small cells, and the outline draws the larger ones:

```
source=outline:/usr/share/fonts/terminus/TerminusTTF-4.49.3.ttf
strike=bdf:/usr/share/fonts/terminus/ter-u14n.bdf
strike=bdf:/usr/share/fonts/terminus/ter-u16n.bdf
```

A cell up to 16 pixels high, the tallest bitmap file's height, draws the tallest bitmap file that fits it. So a 15-pixel cell draws the 14-pixel file, and a 16-pixel cell draws the 16-pixel file. A cell of 17 pixels or more draws the outline. A character that the bitmap files lack, such as a Nerd Fonts icon, comes from the outline at every size.

## Outline fonts behind a bitmap font

A bitmap font often lacks characters you need, such as Chinese characters or icons. You can list an outline font after it to supply them. This frit uses Terminus, and takes the characters Terminus lacks from Noto Sans CJK:

```
source=kbd:ter-v16n
source=font:Noto Sans CJK SC
```

When a frit's first source is a bitmap font, vtium renders the outline fonts after it the way it renders the bitmap font. It fits each character to the pixel grid, and each pixel is either fully on or fully off. The added characters then have the same sharp edges as the bitmap font's own. Color emoji keep their color.

A `render=` line under an outline font's `source=` line chooses how vtium renders that font. With `render=smooth`, vtium renders it antialiased, with soft edges, as it renders outline fonts elsewhere:

```
source=kbd:ter-v16n
source=font:Noto Sans CJK SC
render=smooth
```

With `render=bitmap`, vtium renders an outline font with sharp edges even when it comes first in the frit. A bitmap font always has sharp edges, so vtium refuses a frit with a `render=` line under a bitmap font's `source=` line. See [the issue about outline fonts behind a bitmap font](https://github.com/vtium/vtium/issues/311).

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

`vtium-config preview FONT` ends its preview with a line of superscripts and subscripts, and so does the preview in the Fonts tab of `vtium config tui`. That line shows x2 + y2 = z2 and H2O with the digits small. Both also say how the font makes them. See [the superscript and subscript issue](https://github.com/vtium/vtium/issues/37).

## Install a missing font

A frit can say which font each source needs, so that vtium-config can name the font when it is missing. Four lines can follow a `source=` line:

- `version=` gives the lowest version of the font that the frit was written for, such as `3.4.0`.
- `check=` names a character that only the right font holds, such as `U+E0B0`. A Nerd Fonts build holds that character, and the plain font of a similar name does not.
- `sha256=` gives the SHA-256 of a font file that the frit names by its path.
- `url=` gives the font's page on the web, where you can get it.

For example, this frit names the Nerd Fonts build of Fira Code precisely:

```
name=fira-code-nerd
source=font:FiraCode Nerd Font Mono
version=3.4.0
check=U+E0B0
url=https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.4.0
```

When a font is missing, vtium renders its characters from the next font in the frit that has them. A font that lacks the `check=` character counts as missing too. vtium logs one line for each missing font, once a boot. An older version of a font, or a font file with another SHA-256, still renders.

To see which fonts your frits need, run:

```
$ vtium config fonts
```

After the list of frits, the command names each missing font, with the version the frit asks for and the font's page. It also names each installed font that is older than the frit asks for, and each font file whose SHA-256 differs.

To install a missing font, use one of these two ways:

- Search your distribution's packages for the font's name, and install the package that provides it. Many distributions package common fonts, such as JetBrains Mono, Noto Color Emoji and the Nerd Fonts builds.
- Download the font from its page, and copy its files into `/usr/local/share/fonts`. The font helper looks for fonts under `/usr/share/fonts` and `/usr/local/share/fonts`, or in the folders that `font-paths` names instead. See [An outline font](#an-outline-font) for details.

Then list the font's files in `/etc/vtium/batch.conf`, as [Active fonts](#active-fonts) describes. vtium uses the font from the next login. You don't need to change the frit. Run `vtium config fonts` again to check that the font is found.

[crates/vtium-config/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium-config/README.md#what-a-source-needs) describes each of the four lines in full.

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

Two of the tool's tabs work with fonts. The Fonts tab lists the font files on this machine and sets which of them vtium may use. The Looks tab combines fonts into font sets, and gives each look a font set and a size. The tool opens on its Flow tab. Press `1` for the Fonts tab and `3` for the Looks tab. [The TUI](configure.md#the-tui) describes every tab and its keys.

To render text in a font you have not used yet, take these steps:

1. Skip this step unless the font is in your own folder, such as `~/.local/share/fonts`. vtium cannot use such a font, because root does not own the file, so install it as a system font first. In the Fonts tab, highlight the font and press Enter to open its detail. Press Enter again. vtium-config shows the command, which copies the file into `/usr/local/share/fonts` through sudo, and asks before it runs the command.
2. In the Fonts tab, highlight the font and press Space to activate it. For a font you installed in step 1, highlight the copy in `/usr/local/share/fonts`.
3. In the Looks tab, highlight a font set and press `n` to open the form for a new font set. Type the font's family name, such as `JetBrains Mono`, in the first font field. Press Enter on each field, then Enter on the form's last row to add the font set to the install.
4. Press `i` to install the draft. The install writes the new font set, and adds the font to the list of active fonts.
5. Log out and back in, because the console reads the list of active fonts again at the next login. Then run `vtium config tui` again.
6. In the Looks tab, highlight the new font set and press `t` to try it on the console.
7. Highlight a look and press Enter to open its card. Choose the new font set on its Font set line, and a size on its Size line. For a bitmap font, the Size line offers only the point sizes that draw the font's bitmap sizes. When your displays have different densities, these sizes are for the display with the lowest density.
8. Press `s` to save the draft and `i` to install it. The look's new font set takes effect when vtium next starts on the VT, and the install offers to restart vtium for you.

Moving the highlight never changes the console's font. `t` in the Looks tab switches the console to the highlighted font set for 10 seconds. Press `k` to keep it until you quit the tool. Any other key puts back the font the console used before, and so does the end of the 10 seconds. So a font that makes the screen unreadable undoes itself. To keep a font set after you quit, choose it on a look's card, then save and install. The trial needs a vtium console with the `allow-font-ops` key on.

Under the list, both tabs preview the highlighted item, and `p` switches the preview between the sample characters and a picture of the console. What the preview shows depends on where the tool runs:

- In a vtium console with `allow-font-ops` on, the preview is text that the console draws in the font it uses now. To see a font set there, try it with `t`.
- Anywhere else, the preview shows the highlighted font. If the terminal can show kitty graphics images, the preview is an image. Otherwise, the tool describes the character cell and the grid in words. The grid is the number of columns and rows of characters that fit on the display.

An image of the sample characters is a specimen, a sample of every character the font covers, at its real size. A character that the font itself does not have shows in grey where the built-in font supplies it. It shows as an empty box where no font supplies it. `vtium config preview FONT` prints the same specimen. The Looks tab previews a look at the look's size. A vtium console shows images when vtium is built with the `images` feature.

A head is one terminal of vtium, on one or more displays, and a look is what a head draws with: its font set, size, color scheme and vtlook. To give one head a font of its own, give that head a look of its own. See [The sections of vtium.conf](head-file.md#reading-the-sections) for details.

Before it installs, the tool asks you to confirm. It tells you how many lines of vtium.conf change, and how many font sets it installs with them. Then it runs the one command that needs root, `sudo /usr/bin/vtium-config apply DIR`. sudo asks for your password once.

That command checks each frit, the list of active fonts and the new vtium.conf with vtium's own checks. It writes the frits first, then the list, and vtium.conf last. It keeps each old file beside the new one as `NAME.bak`, such as `vtium.conf.bak`. vtium-config changes only the lines that need to change, so your comments stay. [When vtium-config asks for sudo](configure.md#when-vtium-config-asks-for-sudo) describes each step.

A font set that a look names takes effect when vtium next starts on the VT, like most changes to vtium.conf. Until then, each head keeps the font it had before the install. A restart of vtium keeps your sessions running. A font you activate takes effect sooner, at the next login. [The file](configure.md#the-file) says what vtium reads when.

You can also run the tool in a terminal that is not vtium, such as a desktop terminal. There it shows the same lists and installs the same way. It does not send that terminal a font, and it tells you so. When the `allow-font-ops` key is off, the tool tells you that too, and says how to turn the key on in the Settings tab.

`vtium config new-frit` makes a frit, and `vtium config remove-frit` removes one, without the full-screen tool. Like the tool, each command keeps its change in your draft and installs nothing. `vtium config install` then installs the draft with its font sets. When `remove-frit` names a font set that only your draft adds, it takes that font set back out of the draft. A removal waits in the draft even while a look uses the font set. The check then names the look, and `install` refuses the draft until the look uses another font.

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
