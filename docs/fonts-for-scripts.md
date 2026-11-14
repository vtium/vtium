# Fonts for scripts with marks

Many writing systems put marks above and below their letters. Arabic writes its vowels as marks, Hebrew adds vowel points under its letters, and Vietnamese stacks two marks on one letter. In a terminal's tight rows, those marks can touch the row above or below.

vtium ships five ready-made font sets for these scripts. A font set, or frit, is a short text file in `/etc/vtium/fonts` that lists the fonts vtium renders text with. Each of these sets names a font for its script, makes the rows taller, and lets the script's marks use the added room. This guide shows you how to install one and use it. See [the issue for ready-made font sets](https://github.com/vtium/vtium/issues/204) for the plans behind them.

In the commands below, `#` marks a command you run as root, for example with sudo. `$` marks a command you run as your own user.

## The font sets

Each set renders Latin text in Noto Sans Mono, which also sets the size of the character cell. It renders its script in that script's Noto font, at Noto Sans Mono's size.

| Font set | For | Its script's fonts | Line spacing |
|---|---|---|---|
| `arabic` | Arabic, Persian and Urdu | Noto Sans Arabic | 30 |
| `hebrew` | Hebrew, with vowel points and cantillation marks | Noto Sans Hebrew | 10 |
| `indic` | Hindi, Bengali, Punjabi, Gujarati, Odia, Tamil, Telugu, Kannada, Malayalam and Sinhala | Noto Sans Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam and Sinhala | 40 |
| `thai-lao` | Thai and Lao | Noto Sans Thai and Noto Sans Lao | 25 |
| `vietnamese` | Vietnamese | Noto Sans Mono alone | 25 |

Line spacing is a percentage of the text's height. vtium adds half of it above each row of text and half below. For example, at 32 pixels of text, the `arabic` set's 30 adds 5 pixels above each row and 5 below it.

The extra rows are room for the script's marks. A mark may reach into them, so it stays clear of the letters in the next row. Box drawing still stretches over the whole taller row, so the lines of a box still join.

The `indic` set renders some scripts poorly for now. Tamil, Malayalam and Sinhala come out smaller than the text, and Kannada shows its conjuncts as separate letters. See [the shaping issue](https://github.com/vtium/vtium/issues/144) for the work that fixes this.

## Install the font sets and their fonts

vtium ships the font sets, never the fonts. The optional `vtium-fonts` package installs all five sets in `/etc/vtium/fonts`, and it depends on the fonts they name. From its next publish, installing it gives you both:

```
$ git clone https://aur.archlinux.org/vtium-fonts.git
$ cd vtium-fonts
$ makepkg -si
```

On Arch Linux, those fonts come from the `noto-fonts` package. On another distribution, install Noto Sans Mono and the Noto font for your script from its packages.

When you build vtium from a checkout, name the sets in `FRITS` beside the default ones, and `make install` stages them too:

```
$ make install DESTDIR=DIR PREFIX=/usr FRITS='desktop minspec-16 minspec-8 arabic'
```

`make install-fonts DESTDIR=DIR` stages the five sets alone. Their files are in `tools/specimens/frits`.

When a font is missing, vtium leaves it out and says nothing. The characters it would have rendered then come from vtium's built-in font. To see which fonts a set still lacks, run:

```
$ vtium config fonts
```

The command lists each font set, whether vtium can read it, and the fonts it needs that this machine lacks. To check a set the way vtium reads it, run:

```
$ vtium config check /etc/vtium/fonts/arabic.vtfrit
```

A good font set prints `ok` and the file's name.

## Use a font set

To use the set on every head, set `font-name` in `/etc/vtium/vtium.conf`:

```
font-name=arabic
```

To use it on one head only, give that head a look of its own. A look is a `[look NAME]` section of vtium.conf that names a font set. For example, these commands give a head named `console` the `arabic` set:

```
$ vtium config look add arabic --frit arabic
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
