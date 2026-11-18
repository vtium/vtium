# Fonts for other scripts

Many writing systems put marks above and below their letters. Arabic writes its vowels as marks, Hebrew adds vowel points under its letters, and Vietnamese stacks two marks on one letter. In a terminal's tight rows, those marks can touch the row above or below.

vtium ships five ready-made font sets for these scripts. A font set, or frit, is a short text file in `/etc/vtium/fonts` that lists the fonts vtium renders text with. Each of these sets names a font for its script, makes the rows taller, and lets the script's marks use the added room. vtium also ships four font sets for Chinese, Japanese and Korean, one for each region's forms of Han characters. This guide shows you how to install one and use it. See [the issue for ready-made font sets](https://github.com/vtium/vtium/issues/204) for the plans behind them.

In the commands below, `#` marks a command you run as root, for example with sudo. `$` marks a command you run as your own user.

## The font sets

Each set renders Latin text in Noto Sans Mono, which also sets the size of the character cell. It renders its script in that script's Noto font, at Noto Sans Mono's size. After the script, each set adds Nerd Fonts' icons, color emoji, and GNU Unifont for every other character. These shared parts are layers, files in `/etc/vtium/fonts/layers/` that every set in vtium's catalog can add. See [Fonts](concepts/fonts.md#the-catalog-of-font-sets) for the catalog.

| Font set | For | Its script's fonts | Line spacing |
|---|---|---|---|
| `noto-arabic` | Arabic, Persian and Urdu | Noto Sans Arabic | 30 |
| `noto-hebrew` | Hebrew, with vowel points and cantillation marks | Noto Sans Hebrew | 10 |
| `noto-indic` | Hindi, Bengali, Punjabi, Gujarati, Odia, Tamil, Telugu, Kannada, Malayalam and Sinhala | Noto Sans Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam and Sinhala | 40 |
| `noto-thai-lao` | Thai and Lao | Noto Sans Thai and Noto Sans Lao | 25 |
| `noto-vietnamese` | Vietnamese | Noto Sans Mono alone | 25 |

Line spacing is a percentage of the text's height. vtium adds half of it above each row of text and half below. For example, at 32 pixels of text, the `noto-arabic` set's 30 adds 5 pixels above each row and 5 below it.

The extra rows are room for the script's marks. A mark may reach into them, so it stays clear of the letters in the next row. Box drawing still stretches over the whole taller row, so the lines of a box still join.

With the `noto-indic` set, a syllable with a spacing vowel sign or a joined conjunct takes two character cells, so it draws at full size. You can set `cluster-width = single` for programs that count one cell. vtium shapes each syllable once across its cells, so Kannada and other conjuncts form as the font draws them. See [the shaping issue](https://github.com/vtium/vtium/issues/144) for details.

## Chinese, Japanese and Korean

Chinese, Japanese and Korean all use Han characters. Unicode encodes each Han character once, even where China, Taiwan, Japan and Korea write it in slightly different forms. For example, 直, 骨 and 刃 each look a little different in each region. Noto Sans CJK has a face for each region's forms, and vtium ships one font set for each face:

| Font set | Han in the forms of | Its font |
|---|---|---|
| `noto-cjk-sc` | Simplified Chinese, as mainland China and Singapore write it | Noto Sans CJK SC |
| `noto-cjk-tc` | Traditional Chinese, as Taiwan writes it | Noto Sans CJK TC |
| `noto-cjk-jp` | Japanese | Noto Sans CJK JP |
| `noto-cjk-kr` | Korean | Noto Sans CJK KR |

Each of the four sets renders Chinese, Japanese and Korean text, with Japanese kana and Korean Hangul. Only the forms of Han differ between them, so choose the set for the forms you read. Like the other sets, each one renders Latin text in Noto Sans Mono. Each Chinese, Japanese or Korean character takes two character cells.

These four sets add no line spacing. Han, kana and Hangul have no marks above or below the letters, and they fit inside the text's height.

## Install the font sets and their fonts

vtium ships the font sets, never the fonts. The optional `vtium-fonts` package installs all nine sets and their layers in `/etc/vtium/fonts`, and it depends on the fonts they name. From its next publish, installing it gives you both:

```
$ git clone https://aur.archlinux.org/vtium-fonts.git
$ cd vtium-fonts
$ makepkg -si
```

On Arch Linux, those fonts come from the `noto-fonts` package, and Noto Sans CJK comes from `noto-fonts-cjk`. On another distribution, install Noto Sans Mono and the Noto font for your script from its packages.

No vtium package carries a Nerd Fonts file. Every Nerd Fonts build includes the Font Logos icons, which Nerd Fonts lists with no license grant. To get the logos and every other Nerd Fonts icon, install your distribution's Nerd Fonts package. For example, Arch's `ttf-nerd-fonts-symbols-mono` installs Symbols Nerd Font Mono, and every set in this guide draws its icons from it. Until then, each icon draws as a hollow box.

When you build vtium from a checkout, name the sets in `FRITS` beside the default ones, and `make install` stages them too:

```
$ make install DESTDIR=DIR PREFIX=/usr FRITS='jetbrains-mono unifont-icons unscii-icons noto-arabic'
```

`make install-fonts DESTDIR=DIR` stages the catalog apart from the sets and layers that a console package installs. The sets' files are in `crates/vtium-config/frits/scripts`, and the layers are in `crates/vtium-config/frits/layers`.

When a font is missing, vtium leaves it out and logs one line that names it. The characters it would have rendered then come from the next font in the set that has them. To see which fonts a set still lacks, run:

```
$ vtium config fonts
```

The command lists each font set, whether vtium can read it, and the fonts it needs that this machine lacks. It names the web page where you can get each font. To check a set the way vtium reads it, run:

```
$ vtium config check /etc/vtium/fonts/noto-arabic.vtfrit
```

A good font set prints `ok` and the file's name.

## Use a font set

To use the set on every head, set `font-name` in `/etc/vtium/vtium.conf`:

```
font-name=noto-arabic
```

To use it on one head only, give that head a look of its own. A look is a `[look NAME]` section of vtium.conf that names a font set. For example, these commands give a head named `console` the `noto-arabic` set:

```
$ vtium config look add arabic --frit noto-arabic
$ vtium config head set console --look arabic
$ vtium config install
```

[Configure vtium](configure.md) explains looks, heads and the install step.

## Change the line spacing

Each set's line spacing is a starting point. The `font-line-spacing` key in vtium.conf overrides it, from 0 to 100. For example, this command gives the look `arabic` more room:

```
$ vtium config look set arabic --line-spacing 40
$ vtium config install
```

More line spacing gives the marks more room, and the screen fits fewer rows.
