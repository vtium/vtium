# Color schemes

A color scheme sets the colors that programs get: the 16 basic colors, the default text and background colors, and the colors of the cursor and the selection. vtium.conf calls a color scheme a palette, so the key that picks one is `palette`.

A color scheme and a frame filter do different things. Programs name colors by number, and the color scheme says which color each number is. vtium draws the screen in those colors. A filter then changes the finished picture, as a tint turns every color into a shade of amber. A look takes one of each, and the color scheme comes first. See [Vtlook files](looks.md) for the filters.

## Built-in palettes and palette files

Nine palettes are built into vtium, so they need no file: `legacy`, `nord`, `solarized`, `solarized-black`, `solarized-white`, `soft-black`, `base16-dark`, `base16-light` and `vga`. The name `custom` takes its colors from vtium.conf's `palette-*` keys. See the Colors section of [Configuration keys](config.md#colors) for those keys.

Every other color scheme is a palette file, `NAME.palette`, in one of two folders:

| Folder | What it holds |
|---|---|
| `/usr/share/vtium/palettes` | The color schemes vtium installs |
| `/etc/vtium/palettes` | Your own color schemes |

When both folders hold a file of the same name, vtium reads the one in `/etc/vtium/palettes`. A name has 1 to 64 of the characters `A-Z a-z 0-9 . _ + -`, so `Catppuccin Mocha` ships as `catppuccin-mocha`. A file may not take the name of a built-in palette or `custom`. vtium never reads such a file, and `vtium check-config` reports it.

vtium uses a color scheme only when its file passes the trust rule and the color scheme is active. The trust rule asks four things of the file:

- Root owns the file and every folder above it.
- Neither its group nor other users can write the file or a folder above it.
- Neither the file nor a folder above it is a symbolic link.
- The file sits on a filesystem that stays mounted, not on a removable disk, a network share or a FUSE filesystem.

A color scheme is active while `/etc/vtium/batch.conf` lists its file's path, one path on each line. When that file does not exist, vtium uses the list of a fresh install, which holds every color scheme vtium installs. So the installed color schemes work from the first start, and your own work once you add them to the list. vtium reads the list again each time a session ends, so a color scheme you add takes effect at the next login. A file that fails the rule, or that the list leaves out, counts as missing, and `vtium check-config` says why. See [the trust rule issue](https://github.com/vtium/vtium/issues/438) for the work on the trust rule.

A palette file holds at most 16 KiB, and a file that sets all 256 colors takes under 7 KiB.

## Choose a color scheme

The top-level `palette` key sets the color scheme of every head. A `[look NAME]` section can set a `palette` of its own, which wins for the heads that use that look. With no `palette` anywhere, a head draws `legacy`.

```ini
# /etc/vtium/vtium.conf
palette = solarized
version = 1

[look evening]
font-size = 14
palette = catppuccin-mocha
vtlook = amber
```

Here a head that uses the look `evening` draws Catppuccin Mocha's colors, and the vtlook `amber` then tints the finished picture. Every other head draws `solarized`.

You can set a look's color scheme from the command line too: `vtium-config look set evening --palette catppuccin-mocha`.

vtium-config's Looks tab lists the color schemes after the font sets: the built-in ones first, then the palette files. Under the list, the highlighted scheme shows its 16 colors and a line of text in its own colors. Enter opens the scheme's detail, with its file, whether it is active, its trust mark, the looks that use it, and its colors. Space activates or deactivates a palette file, and asks first when a look uses it. Space does nothing on a built-in scheme, which is always active. An activation takes effect at the next login after you install the draft, and so does a deactivation. On a look's card, the color scheme line offers the scheme that vtium.conf sets first, then the built-in schemes, then the active palette files.

vtium reads vtium.conf and the palette files when it starts. So a change to vtium.conf or to a palette file reaches a VT when vtium next starts there, as [The file](configure.md#the-file) describes.

When a look names a palette file that is missing or has an error, the heads that use the look draw the color scheme that the top level names. When the top-level color scheme fails the same way, those heads draw `legacy`. vtium logs one line for each such file. Run `vtium check-config` to see the file, the line and the reason.

## A color scheme for each display

To give each monitor its own color scheme, give each monitor its own head, and give each head a look with its own `palette`. A head is a terminal on one or more displays, and the rules in the `[rules]` section say which display shows which head. This example puts a light scheme on the left monitor and a dark one on the right:

```ini
# /etc/vtium/vtium.conf
version = 1
policy = open

[look day]
palette = sunny

[look night]
palette = tokyo-night

[head left]
look = day

[head right]
look = night

[rules]
display edid:AAA:0x0001:LEFT000001 -> left
display edid:BBB:0x0002:RIGHT00001 -> right
display * -> left
input class:keyboard -> roaming
input class:mouse -> roaming
input * -> ignore
```

Each `edid:` value names one monitor by the identity it reports, and `vtium config flow --json` lists the values of the monitors connected now. The keyboard and the mouse roam: they type into one head at a time, and `Ctrl-Super-Up` and `Ctrl-Super-Down` move them to the other head. Each look can also set its own font and vtlook. [Heads, by scenario](heads.md) gives more setups, such as a keyboard for each head.

## Programs that change the colors

A program can change the colors in its own console with escape sequences. OSC 4 changes one of the 256 colors. OSC 10, 11 and 12 change the default foreground, the default background and the cursor's color. OSC 13 and 14 change the mouse pointer's colors, and OSC 17 and 19 the selection's colors. OSC 104, 110 to 114, 117 and 119 reset them to the color scheme's colors. vtium ignores OSC 15, 16 and 18, xterm's Tektronix colors. A program can also use kitty's color protocol, OSC 21, and save and restore all the colors with xterm's and kitty's color stacks. RIS, the full reset, and DECSTR, the soft reset, return the colors to the color scheme's too. See [Escape sequences](sequences/index.md) for each sequence.

## The file format

vtium reads Ghostty's theme format as it is, so a color scheme written for Ghostty works unchanged. Each setting is a `key = value` line. A line that starts with `#` is a comment, and blank lines are allowed.

```ini
# /etc/vtium/palettes/example.palette
background = #20242a
foreground = #dcdcd0
cursor-color = #e0a040
selection-background = #3a4250
selection-foreground = cell-foreground
palette = 0=#20242a
palette = 1=#d05050
palette = 2=#70b070
palette = 4=SteelBlue
```

vtium reads these keys:

| Key | Value | What it sets |
|---|---|---|
| `palette` | `N=COLOR`, with N from 0 to 255. You can write N in binary, octal or hexadecimal after `0b`, `0o` or `0x`. | Color N. A color the file leaves unset keeps its value in `legacy`. So colors 16 to 255 stay the standard 256-color cube and gray ramp unless the file sets them. |
| `foreground`, `background` | `COLOR` | The default text and background colors. |
| `cursor-color` | `COLOR` | The cursor's color. |
| `selection-background`, `selection-foreground` | `COLOR` | The colors of selected text. |

A `COLOR` is `#rrggbb`, `rrggbb`, or an X11 color name such as `DarkSlateGray`. In the last three keys, `cell-foreground` and `cell-background` leave the color unset. vtium then draws the cursor or the selection by inverting the cell, as it does when the color scheme sets no cursor or selection color.

When a key appears twice, the later line wins. vtium ignores every other key, such as a font or a cursor style, and `vtium check-config` lists each one as a note. vtium never follows a `config-file` or `theme` line, so a color scheme sets colors and nothing else.

These are errors: a line that is not `key = value`, a color vtium cannot read, a color number past 255, and a file over 16 KiB. Any error makes the whole file count as missing.

## The color schemes vtium installs

vtium installs these color schemes in `/usr/share/vtium/palettes`. Each comes from its author's own repository, under a permissive license. The opening comments of each file name its source, its author and its license. vtium installs each license's text with its other license notices.

| Color scheme | Names | Source | License |
|---|---|---|---|
| Dracula | `dracula` | [dracula/ghostty](https://github.com/dracula/ghostty) | MIT |
| Catppuccin | `catppuccin-latte`, `catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha` | [catppuccin/ghostty](https://github.com/catppuccin/ghostty) | MIT |
| Tokyo Night | `tokyo-night`, `tokyo-night-storm`, `tokyo-night-moon`, `tokyo-night-day` | [folke/tokyonight.nvim](https://github.com/folke/tokyonight.nvim) | Apache-2.0 |
| Gruvbox | `gruvbox-dark`, `gruvbox-light` | [gruvbox-community/gruvbox](https://github.com/gruvbox-community/gruvbox) | MIT |
| Kiln | `kiln` | vtium's own: a warm near-black background, saturated accents and a light warm foreground | vtium's license |
| Sunny | `sunny` | vtium's own: black text on a white background, like the white consoles of 1990s Unix workstations | vtium's license |
| Ink | `ink` | vtium's own: black and red on white, for a display of three inks such as a three-color e-paper panel | vtium's license |

Ink holds only black, red and white, so a panel that shows those three inks displays every one of its colors. Red, yellow and magenta print in red ink, and the other colors in black, so text a program colors stays readable on white. Colors 7 and 15 are the white of the paper, so text in those colors shows only on a colored background.

Nord and Solarized are built-in palettes, so they need no file. To use a color scheme vtium does not install, copy its Ghostty theme file to `/etc/vtium/palettes/NAME.palette` as root, and add that path to `/etc/vtium/batch.conf`.
