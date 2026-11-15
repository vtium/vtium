---
contract: adr/v1
kind: decision
verb: change
capability: glyphs
basis:
  - operator: aaronsb
    level: guided
    said: "I just want to avoid 'deco-collaging' a bunch of fonts together to get coverage, at least without a clear specification for the fonts."
    via: the maintainer in the steering session, 2026-11-14, while answering the questions on the text records, as the GitHub issue that asks for this specification quotes it
  - operator: aaronsb
    level: guided
    said: "The ones we've chosen for verification and testing (goldens) are fine because they're represenatitive of general fonts anyone can use."
    via: the same words of the maintainer, 2026-11-14, as the same issue quotes them
  - operator: aaronsb
    level: guided
    said: "It feels like we've already sort of made assumptions down that path in our fonts package (which is fine, because it facilitates the use) but I don't thinke we've really thought through how it should be organized"
    via: the same words of the maintainer, 2026-11-14, as the same issue quotes them
  - operator: aaronsb
    level: directed
    said: "No implicit emoji source for the frit made from font-name: fonts are not combined for coverage without a specification of how font sets are organized."
    via: the steering session's questions on 2026-11-14, on the text records, as ADR-1200's `considered` records the answer
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "vtium-fonts is defined loosely because we haven't had ADR-1204. Ship a catalog of frits that are named after and based on an actual font style, that do not over-represent the minspec or language fonts, and that live in a structure by category: console fonts (core, kbd, Terminus), classic hardware fonts (IBM code pages, Solaris, DEC VT), code-friendly fonts (JetBrains Mono, Fira Code, Cascadia Code and Caskaydia Cove, Hack, IBM Plex Mono, Source Code Pro and Sauce Code Pro, Monoid), Nerd Fonts as a family of their own, and scripts and regions, which may pair a Roman-style text font with the native script fonts. For many of these fonts we won't ship them with the packages; they are required resources that have to match, and vtium should be able to say which font is needed. Construct them ready to use, so glyphs, box drawing and emoji all work in them. They need to remain composable. Write each decision as a positive statement, without double negatives."
    via: the maintainer's review of the first draft of this record, 2026-11-14, relayed by the steering session in its revision brief; the wording is the steering session's, close to the maintainer's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "we should ship essentially one 'guaranteed to work out of the box without installing any outside fonts' for bitmap and outline, if possible."
    via: the maintainer, 2026-11-14, relayed in the steering session's brief for this revision; the words are the maintainer's as the brief quotes them
  - operator: aaronsb
    level: directed
    said: "I also wonder if it's a mistake to ship fonts in the binary, and instead externalize them just like any other font is, and load at runtime."
    via: the maintainer, 2026-11-14, relayed in the steering session's brief for this revision; the words are the maintainer's as the brief quotes them
  - operator: aaronsb
    level: directed
    said: "we keep a bitmap font that is just enough that's shipped, and not comprehensive. or at least, it should be very small."
    via: the maintainer, 2026-11-14, relayed in the steering session's brief for this revision; the words are the maintainer's as the brief quotes them
  - operator: aaronsb
    level: directed
    said: "the rescue font is when all the other fonts, frits, terminals, configuration files fail vtium to run. it's the builtin."
    via: the maintainer, 2026-11-14, relayed in the steering session's brief for this revision; the words are the maintainer's as the brief quotes them
  - operator: aaronsb
    level: directed
    said: "it's the font that if I deleted all the frits, config files, and all other resources and attempted to restart the service, it would use that font."
    via: the maintainer, 2026-11-14, relayed in the steering session's brief for this revision; the words are the maintainer's as the brief quotes them
  - operator: aaronsb
    level: guided
    said: "The built-in font could be an fbcon font of sorts, if the licenses are compatible, and it is the font of the OSD, the panic screen and the errors."
    via: the maintainer's questions of 2026-11-14, relayed in the steering session's brief for this revision; the wording is the steering session's
    paraphrase: true
  - evidence: "Measured 2026-11-14 on docs/font-sets: make minspec builds a 4,093,032-byte console against tools/tier3/t3_impl.py's BINARY_BUDGET of 4,194,304; the embedded unifont.vtglass is 1,344,071 bytes and unscii-8.vtglass 15,494. The built-in font's list of 354 characters cut from each candidate, raw bitmaps and zlib level 9: Unscii-8 8x8 2,832 and 1,253 bytes; upstream Terminus 4.49.1 ter-u16n.bdf 5,520 and 1,429, ter-u32n.bdf 22,080 and 2,539, each lacking U+2504-U+2507, U+254C-U+254F, U+2594 and U+2595; Unifont 8x16 5,728 and 1,594, holding all. kbd's ter-v16n.psf.gz holds 627 characters and lacks 144 of the 160 box and block characters. The string and character literals of crates/vtium/src, crates/vtium-render/src/notice.rs and crates/vtium-settings/src use U+00B7, U+2026, U+25CB and U+25CF beyond ASCII, and U+271B only in a test. Upstream Terminus 4.49.1 (terminus-font-4.49.1.tar.gz from SourceForge, SHA-256 d961c1b781627bf417f9b340693d64fc219e0113ad3a3af1a3424c7aa373ef79) carries OFL.TXT with the Reserved Font Name Terminus Font. The kernel's lib/fonts/font_8x16.c and font_ter16x32.c begin SPDX-License-Identifier: GPL-2.0 (torvalds/linux on GitHub). File sizes on this machine: JetBrains Mono 2.304's four faces 1,108,400 bytes, SymbolsNerdFontMono-Regular.ttf 2,610,012, NotoColorEmoji.ttf 10,730,124, ter-u16n.bdf under gzip 16,616, unifont.hex under gzip 1,687,527."
  - operator: aaronsb
    level: directed
    said: "I meant linux's fonts, not arch's fonts."
    via: the maintainer's answer of 2026-11-14 to this record's probe on what core fonts means, relayed by the steering session
  - evidence: "Read 2026-11-14 from torvalds/linux on GitHub: lib/fonts/Kconfig (FONTS, Select compiled-in fonts; FONT_8x8 and FONT_8x16 default y if !SPARC && !FONTS; FONT_6x11, 7x14, PEARL_8x8, ACORN_8x8, MINI_4x6, 6x10, 10x18, SUN8x16, SUN12x22, TER10x18, TER16x32, 6x8); every lib/fonts/font_*.c but font_mini_4x6.c begins SPDX-License-Identifier: GPL-2.0 on its first line; drivers/tty/Kconfig, config VT, bool Virtual terminal if EXPERT; drivers/video/console/Kconfig, config FRAMEBUFFER_CONSOLE, depends on FB_CORE; include/uapi/linux/kd.h, KD_FONT_OP_GET 1 and KD_FONT_OP_SET_DEFAULT 2, Set font to default, data points to name / NULL. Each kernel font's glyphs for U+0021 to U+00FF in code page 437 order were compared with every file in /usr/share/kbd/consolefonts on this machine (kbd 2.10.0 and terminus-font)."
  - evidence: "Read on integration at d682f48c, 2026-11-14: every frit in the tree. crates/vtium-config/frits holds minspec-16, minspec-8, proof and tiny5. tools/specimens/frits holds desktop and the nine sets for scripts. tools/tier3/guest/frits holds thirteen test sets, tools/vt/sections/frits six, and tools/tier2/src/fonts.rs builds tier 2's test set in code. The Makefile's FRITS and FONTS_FRITS say which a package installs. crates/vtium/src/font.rs, `Wanted::source`, makes the frit for a family that font-name names with the one source font:<family>. crates/vtium-config/src/frit.rs reads one file and has no key that includes another file."
  - evidence: "vtium-arch's recipes, read through the GitHub API on 2026-11-14: vtium/PKGBUILD installs FRITS and the recipe repository's own terminus.vtfrit (source=font:ter-v16n, then source=builtin:unifont). vtium-fonts/PKGBUILD stages FONTS_FRITS and depends on noto-fonts, noto-fonts-cjk, noto-fonts-emoji, terminus-font, ttf-jetbrains-mono and ttf-nerd-fonts-symbols-mono."
  - evidence: "Package indexes read 2026-11-14. Arch (pacman -Si): ttf-fira-code OFL-1.1, ttf-cascadia-code OFL, ttf-hack custom (MIT and Bitstream Vera), ttf-ibm-plex OFL, adobe-source-code-pro-fonts OFL-1.1, ttf-monoid, ttf-dejavu, and the Nerd builds ttf-jetbrains-mono-nerd and ttf-firacode-nerd (OFL-1.1-no-RFN), ttf-cascadia-code-nerd and ttf-sourcecodepro-nerd (OFL-1.1-RFN), ttf-hack-nerd (MIT, Bitstream-Vera) and ttf-monoid-nerd (MIT, OFL-1.1-no-RFN), all in extra; kbd's files include default8x16, sun12x22, solar24x32 and ter-v16n. Debian (sources.debian.org): fonts-jetbrains-mono, fonts-firacode, fonts-cascadia-code, fonts-hack, fonts-ibm-plex, fonts-monoid, fonts-noto, fonts-noto-cjk, fonts-noto-color-emoji, console-setup, xfonts-terminus, kbd, and fonts-nerd-symbols 3.4.0+dfsg2-1 in forky and sid only. Fedora (mdapi, rawhide): jetbrains-mono-fonts, fira-code-fonts, cascadia-code-fonts, ibm-plex-mono-fonts, adobe-source-code-pro-fonts, google-noto-sans-mono-fonts, google-noto-sans-arabic-fonts, google-noto-sans-cjk-fonts, google-noto-color-emoji-fonts, terminus-fonts-console and kbd; no package found under hack-fonts or under three names for Nerd Fonts' symbols."
  - evidence: "Licenses read 2026-11-14 from each project's repository or page: JetBrains Mono, Fira Code, IBM Plex and Source Code Pro OFL-1.1 (GitHub's license field); Cascadia Code OFL-1.1 (its LICENSE); Hack MIT for Hack's work and the Bitstream Vera License for the Vera glyphs (its LICENSE.md); Monoid MIT and OFL (its README); Glass TTY VT220 the Unlicense (svofski/glasstty); Spleen BSD-2-Clause (fcambus/spleen); the Ultimate Oldschool PC Font Pack, which holds the Px437 IBM fonts, Creative Commons Attribution-ShareAlike 4.0 (int10h.org's readme); kbd GPL-2.0-or-later as a package, with each console font's own notice; Tiny5 Mono OFL-1.1 (the tiny5 frit's comment)."
  - standard: "UTS #51, Unicode Emoji, version 18.0: the Emoji property and the emoji components in emoji-data.txt"
  - standard: "UAX #24, Unicode Script Property, Unicode 18.0: Script and Script_Extensions, with Common and Inherited"
  - standard: "UAX #11, East Asian Width, Unicode 18.0: the East_Asian_Width values W and F"
  - standard: "os-release(5), systemd 262: ID and ID_LIKE name the distribution"
  - precedent: ADR-1200
  - precedent: ADR-1500
  - precedent: ADR-1503
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "Every catalog frit passes the catalog check: its text font first, then its layers in the order script, symbols, emoji, each source supplying only characters of its role, each font named with its packages, and its license line naming every source's license"
  - "On a machine with every font of a catalog frit installed, its specimen draws Latin text from its text font, box drawing from vtium's sprites, a Powerline separator and a Nerd Fonts icon from its symbols source, and an emoji in color"
  - "With fira-code chosen and Fira Code absent, the console draws text in the built-in font with the frit's symbols and emoji, logs one line that names Fira Code and its package, and vtium config fonts prints the package for this machine's distribution"
  - "The frit that vtium makes for font-name=DejaVu Sans Mono holds that one source"
  - "With no vtium.conf, no font sets, no font files and no font helper, vtium reaches a login prompt drawn in the built-in font, and its notices draw in the built-in font"
  - "On a tier 3 guest with no font package installed, terminus and jetbrains-mono each draw a letter, a Powerline separator, a Nerd Fonts icon, an emoji and a Han character from their own files"
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related:
  - ADR-1200
  - ADR-1302
  - ADR-1402
  - ADR-1500
  - ADR-1503
---

# ADR-1204: vtium draws its own messages in one small built-in font and ships a catalog of font sets built from layers, two of them with every font they need

## Summary

- **Decided:** vtium has one built-in font. It is the font vtium draws with when every other font, font set and configuration file is missing or unusable, and vtium's own notices, on-screen display and panic screen always use it. It is small: three bitmap strikes that hold just the characters vtium's messages, a login prompt and a shell's basic output need, compiled into the console binary and drawn without the font helper. Every other font is an ordinary file that the font helper loads. vtium ships a catalog of font sets in six categories: console fonts, classic hardware fonts, code-friendly fonts, Nerd Fonts, scripts and regions, and small panels. Each set is named after the font it is based on, and each is ready to use: it draws text, box drawing, Powerline and Nerd Fonts icons, color emoji and every other character. The console category also holds sets for the Linux kernel's own fonts, which vtium loads from the person's system and never ships. Two sets work out of the box, because the vtium package installs every font they need: `terminus`, a bitmap set, and `jetbrains-mono`, an outline set. A set is built from parts: its own first source is the text font, and `layer=` lines add shared layers for a script, for symbols, for emoji and for every other character, in that order. A person composes a set from the same parts. Each set names every font it needs precisely, and vtium-config names the package that provides a missing one. A font set holds exactly the sources its file and its layers name, with the built-in font at the end.
- **Trades away:** The built-in font draws only its short list of characters. On a machine where the font helper or every font file is missing, a CJK or emoji character draws as a hollow box, where today the compiled-in Unifont draws it. A translated notice in a script beyond that list shows in English. The vtium package grows by about 16.2 MB of font files. Several shipped sets change name, so a vtium.conf that names `desktop`, `minspec-16`, `minspec-8` or a script set changes too. The frit format grows a layer file and five keys that describe a source.
- **One-way?** No. Font sets and layers are text files, and the new keys describe fonts without changing how a glyph draws. The renames are the costly part, so this record makes them once.
- **Probes:** *Confident (exact sources):* a font set holds exactly the sources its file and its layers name, so a person who wants color emoji with their own font picks a catalog set or composes one. *Not confident (renames):* the record renames `desktop` to `jetbrains-mono`, the smallest-build sets to `unifont-icons` and `unscii-icons`, and the script sets to `noto-` names, so that every catalog set is named after its font. Should the old names go now?
- **Inversion:** The choice sits between two ends. At one end each set is a single font, and the built-in font draws every other character. At the other a set gathers whatever installed fonts reach the most characters, which is the collage the maintainer named. vtium builds each set from a text font and a few shared layers, and each layer has one named role.

## Context

A font set, or frit, is a file that names fonts in order (ADR-1200 D1). vtium draws each character from the first font in the list that has it. A character that the whole list lacks draws from vtium's built-in font for the cell: unifont from 16 pixels high, and Unscii-8 below that (ADR-1200 D7). vtium also draws box drawing, blocks, Braille and the legacy computing shapes as sprites of its own, unless a set's `glyph-source=` asks for the font's glyphs (ADR-1200 D1).

vtium's shipped sets grew one at a time. The vtium package installs `desktop`, `minspec-16` and `minspec-8` from vtium's tree, and `terminus` from its recipe repository. The vtium-fonts package installs nine sets for scripts, and it depends on the fonts that every shipped set names (ADR-1500 D8). The sets are named for a purpose rather than a font, and they sit in two flat folders. Only `desktop` and the smallest-build sets carry symbols and emoji.

The work on text records found that the font set made from `font-name=DejaVu Sans Mono` draws every emoji from the built-in unifont, in 1 bit. It proposed a key, `font-emoji`, that would add an emoji font to that set. The maintainer declined it until a specification says how font sets are organized (ADR-1200 D9). This record is that specification. The maintainer's review of its first draft asked for a catalog of sets named after fonts, grouped by category, ready to use and composable, with the fonts each set requires stated precisely.

The goldens and specimens use general open fonts, which the maintainer finds fine.

The console binary carries two built-in fonts today, all of GNU Unifont and Unscii-8 (ADR-1200 D7). The minspec console built on 2026-11-14 is 4,093,032 bytes against its budget of 4,194,304, and Unifont's compressed glyphs are 1,344,071 bytes of it. The maintainer asked for one built-in font that is very small and just enough, with every other font an ordinary file. The maintainer also asked for one bitmap set and one outline set that work on a fresh install with no other font installed.

## Decision

The figure shows how three catalog sets share their layers, and how the `unifont` layer and vtium's sprites complete each one.

![Three catalog font sets built from shared layers. Each column is one font set, with its fonts from top to bottom in the order vtium consults them. The code-friendly set fira-code starts with its text font, Fira Code, then the layer nerd-symbols, then the layer noto-emoji. The script set noto-arabic starts with Noto Sans Mono, then the layer arabic, then the same nerd-symbols and noto-emoji layers. The Nerd Fonts set caskaydia-cove starts with CaskaydiaCove Nerd Font Mono, which holds the symbols itself, then the layer noto-emoji. Under every column, vtium's sprites draw box drawing and blocks, and the layer unifont, a fallback layer, draws every other character. The built-in font draws in place of any of these fonts whose file is missing.](figures/ADR-1204-layers.svg)

### D1: the built-in font

The built-in font is the font vtium draws with when every other font, font set and configuration file is missing or unusable. If every font set, every configuration file and every font file were deleted and vtium's service restarted, vtium would draw its login prompt in this font. vtium's own messages always use it: the notices, the on-screen display, the focus mark and the panic screen, which the console draws outside the terminal (`crates/vtium/src/notice.rs`, `crates/vtium/src/osd.rs`, `crates/vtium-render/src/notice.rs`).

The built-in font is compiled into the console binary as trusted data. The console draws it itself, with no font helper and no font file. So it draws during an initramfs boot before the root file system mounts, after the font helper has failed, and on a minspec machine with no font files at all.

The built-in font is small, and it holds just enough: the characters of vtium's own messages, a login prompt and a shell's basic output. The list is a file in the tree, `crates/vtium-font/data/builtin-chars.txt`, and a build step cuts each strike from its source with that list:

- printable ASCII, U+0020 to U+007E;
- Latin-1, U+00A0 to U+00FF, which holds the middle dot of the on-screen display's line;
- box drawing and blocks, U+2500 to U+259F, for a head whose `glyph-source` asks for font glyphs instead of vtium's sprites;
- the ellipsis U+2026, which vtium uses to shorten a long name, and the circles U+25CF and U+25CB, the focus marks.

A scan of the string and character literals in `crates/vtium/src`, `crates/vtium-render/src/notice.rs` and `crates/vtium-settings/src` on 2026-11-14 found only these characters beyond Latin-1, apart from one in a test.

The built-in font has three strikes. A cell larger than a strike draws it at the largest whole multiple that fits, centred, as ADR-1200 D7 fits a built-in glyph today.

| Strike | Draws cells | Cut from | License | Glyph data for the list |
|---|---|---|---|---|
| 8×8 | under 16 pixels high | Unscii-8, `crates/vtium-font/data/unscii-8.hex` | public domain, as `crates/vtium-font/data/unscii.txt` says | 2,832 bytes, 1,253 deflated |
| 8×16 | from 16 to 31 pixels high | Terminus 4.49.1, `ter-u16n.bdf` | OFL-1.1 with the Reserved Font Name "Terminus Font", as upstream's `OFL.TXT` says | 5,520 bytes, 1,429 deflated |
| 16×32 | from 32 pixels high | Terminus 4.49.1, `ter-u32n.bdf` | the same | 22,080 bytes, 2,539 deflated |

The 8×8 strike keeps the size that minspec's 320×240 panel reads at, 40 columns of 8-pixel characters, which the notices are written for (`TEXT_BESIDE_MIN` in `crates/vtium-render/src/notice.rs`). The two Terminus strikes give the larger heads a font drawn for reading on a console, and the 16×32 strike gives a 4K head a sharp font at twice the size. Terminus holds every character of the list apart from four dashed box lines, two dashed heavy lines and two block characters, U+2504 to U+2507, U+254C to U+254F, U+2594 and U+2595; vtium's sprites draw those.

The strikes come from upstream files. Every font in the Linux kernel's `lib/fonts/`, its copy of Terminus `font_ter16x32.c` included, carries `SPDX-License-Identifier: GPL-2.0`, and vtium is MIT. Upstream Terminus carries the OFL, which lets a font be bundled with software when each copy carries the font's copyright notice and the license. A subset is a modified version, and "Terminus Font" is a reserved name, so vtium names its strikes "vtium built-in". COPYING carries Terminus's copyright line, and the package carries the OFL text that `crates/vtium-font/data/OFL-1.1.txt` already holds.

The binary carries all of GNU Unifont today, 1,344,071 bytes of compressed glyph chunks, and Unscii-8, 15,494 bytes. The built-in font's glyph data comes to about 5.2 KB deflated, so the minspec console shrinks by about 1.35 MB, from 4,093,032 bytes to about 2.74 MB against its 4,194,304-byte budget. Unifont stays in the catalog as a file, the `unifont` layer (D5).

When the built-in font is all vtium has, it draws each character of its list. Every other character draws as a hollow box as wide as its cells, which vtium draws itself, as it draws its sprites. A zh, ja or ko console therefore shows hollow boxes for Han, kana and Hangul until its font set's glyphs arrive. vtium shows a translated message when the built-in font holds every character of it, and the English message otherwise, so a notice always reads (ADR-1402).

The acceptance check: vtium starts with no vtium.conf, no font sets, no font files and no font helper. It reaches a login prompt drawn in the built-in font, and its notices draw in the built-in font. A unit test in vtium checks the drawing with an empty configuration directory and no helper. A tier 3 case removes `/etc/vtium`, vtium's font files and the helper from the guest, restarts vtium's service, and compares the login prompt and a notice with their goldens.

### D2: two sets work out of the box

Two catalog sets draw every role with font files that the vtium package installs itself. They work on a fresh install with no other font installed: `terminus` in `console/`, a bitmap set, and `jetbrains-mono` in `code/`, an outline set.

| Role | `terminus` | `jetbrains-mono` |
|---|---|---|
| Text | Terminus 4.49.1, `ter-u16n.bdf`, OFL-1.1, 16,616 bytes under gzip | JetBrains Mono 2.304, its Regular, Bold, Italic and Bold Italic faces, OFL-1.1, 1,108,400 bytes |
| Box drawing and blocks | vtium's sprites | vtium's sprites |
| Powerline and Nerd Fonts icons | Symbols Nerd Font Mono 3.4.0, drawn 1-bit behind the bitmap text (ADR-1200 D1, `render=`), MIT with its icon sets' licenses, 2,610,012 bytes | the same file, drawn smooth |
| Color emoji | Noto Color Emoji, OFL-1.1, 10,730,124 bytes | the same file |
| Every other character | GNU Unifont 17.0.03, OFL-1.1, about 1.7 MB under gzip, measured on its hex form | the same file |

The two sets share the icon, emoji and Unifont files, so the vtium package grows by about 16.2 MB. It installs the files in `/usr/share/vtium/fonts/`, which the build adds to its list of font folders (ADR-1200 D25). The sets name Terminus and Unifont by path and the outline fonts by family, each with its `sha256=` line (D7). `make install` writes the package's prefix into those paths. Every license above allows vtium to ship the file (D8).

The minspec builds install `terminus` with its Terminus and Unifont files, 1.7 MB. Its icon and emoji fonts, 13.3 MB together, stay optional there. Without them, Unifont draws the Powerline and Nerd Fonts icons it holds and every emoji in 1 bit, so the set still draws every role.

With no font named, vtium draws `terminus` where its files read, and the built-in font otherwise. The order in which vtium chooses a font becomes: the set that a head's look names, then `font-name`, then vconsole's font, then `terminus`, then the built-in font (ADR-1200 D9, D11).

The check that proves both sets work out of the box is a tier 3 case on a guest with no font package installed. The case installs vtium's install tree, draws each set's specimen, and asks the font helper which source drew each character: a letter, a Powerline separator, a Nerd Fonts icon, an emoji and a Han character. Each must come from the set's own files.

### D3: the catalog and its categories

vtium ships a catalog of font sets. Each set is named after the font it is based on, and each belongs to one category. The categories and their first members:

| Category | Folder | What its sets are for | First members |
|---|---|---|---|
| Console fonts | `console/` | The bitmap fonts of the Linux console: kbd's, Terminus, and the Linux kernel's own. | `terminus`, `default8x16`, `linux-console`, `linux-vga-8x16`, `linux-vga-8x8`, `linux-sun-8x16`, `linux-sun-12x22`, `linux-ter-16x32` |
| Classic hardware fonts | `classic/` | The look of a historic terminal or workstation, from re-creations of its font. | `ibm-vga`, `sun-gallant`, `dec-vt220` |
| Code-friendly fonts | `code/` | Outline fonts designed for code, on a display of ordinary size. | `jetbrains-mono`, `fira-code`, `cascadia-code`, `hack`, `ibm-plex-mono`, `source-code-pro`, `monoid`, `dejavu-sans-mono` |
| Nerd Fonts | `nerd/` | Code fonts in their Nerd Fonts build, which carries the icons inside the font. | `jetbrains-mono-nerd`, `caskaydia-cove`, `sauce-code-pro`, `hack-nerd`, `fira-code-nerd`, `monoid-nerd` |
| Scripts and regions | `scripts/` | A writing system beside Latin text. Noto Sans Mono draws the Latin text, and the script's Noto font draws the script. | `noto-arabic`, `noto-hebrew`, `noto-indic`, `noto-thai-lao`, `noto-vietnamese`, `noto-cjk-sc`, `noto-cjk-tc`, `noto-cjk-jp`, `noto-cjk-kr` |
| Small panels | `small/` | Cells of 8×8 to 8×16 on the minspec build. Unifont or Unscii-8 draws the text, as a file, and the set adds a chosen list of icons and emoji. | `unifont-icons`, `unscii-icons`, `tiny5` |

The members, their fonts and their sources:

| Set | Based on | Its sources, in order | The fonts |
|---|---|---|---|
| `terminus` | Terminus, kbd's `ter-v16n` | Terminus; `nerd-symbols`; `noto-emoji` | installed by vtium |
| `default8x16` | kbd's `default8x16` | kbd's `default8x16`; `nerd-symbols`; `noto-emoji` | required |
| `linux-console` | the font the Linux console holds when vtium starts | the VT's font, through `console:`; `nerd-symbols`; `noto-emoji` | required, from the system |
| `linux-vga-8x16`, `linux-vga-8x8`, `linux-sun-8x16`, `linux-sun-12x22`, `linux-ter-16x32` | the Linux kernel's font of that name | the file of kbd or Terminus that holds the same glyphs (below); `nerd-symbols`; `noto-emoji` | required |
| `ibm-vga` | Px437 IBM VGA 9x16, of the Ultimate Oldschool PC Font Pack | Px437 IBM VGA 9x16; `nerd-symbols`; `noto-emoji` | required |
| `sun-gallant` | kbd's `sun12x22`, Sun's Gallant console font | kbd's `sun12x22`; `nerd-symbols`; `noto-emoji` | required |
| `dec-vt220` | Glass TTY VT220 | Glass TTY VT220; `nerd-symbols`; `noto-emoji` | required |
| `jetbrains-mono` | JetBrains Mono | JetBrains Mono; `nerd-symbols`; `noto-emoji` | installed by vtium |
| `fira-code`, `cascadia-code`, `hack`, `ibm-plex-mono`, `source-code-pro`, `monoid`, `dejavu-sans-mono` | the font of the same name | that font; `nerd-symbols`; `noto-emoji` | required |
| `jetbrains-mono-nerd`, `caskaydia-cove`, `sauce-code-pro`, `hack-nerd`, `fira-code-nerd`, `monoid-nerd` | the font's Nerd Font Mono build | that build; `noto-emoji` | required |
| `noto-arabic`, `noto-hebrew`, `noto-indic`, `noto-thai-lao`, `noto-cjk-sc`, `noto-cjk-tc`, `noto-cjk-jp`, `noto-cjk-kr` | Noto Sans Mono and the script's Noto font | Noto Sans Mono; the script's layer; `nerd-symbols`; `noto-emoji` | brought |
| `noto-vietnamese` | Noto Sans Mono, which holds every Vietnamese letter | Noto Sans Mono; `nerd-symbols`; `noto-emoji` | brought |
| `unifont-icons`, `unscii-icons` | GNU Unifont or Unscii-8, as files | Unifont or Unscii-8; `nerd-symbols-small`; `noto-emoji-small` | installed by vtium |
| `tiny5` | Tiny5 Mono | Tiny5 Mono's BDF file; `nerd-symbols-small`; `noto-emoji-small` | required |

A brought font comes with the vtium-fonts package, and a required font is one the person installs (D8). The vtium package installs the fonts of `terminus` and `jetbrains-mono` itself, so those two sets work out of the box (D2). Every set ends with the `unifont` layer, which the table leaves out. The script sets keep the line spacing they have now (ADR-1302 D13). Each set states its starting `size=` or its `cell=`, as the set it replaces does.

#### The Linux kernel's own fonts

The console category holds sets for the fonts that the Linux kernel builds in for its text console. Each font file in the kernel's `lib/fonts/` begins `SPDX-License-Identifier: GPL-2.0`, apart from `font_mini_4x6.c`, which states no license of its own and so falls under the kernel's GPL-2.0. vtium ships none of them. Each set loads its font at run time from the person's own system.

A kernel font is a required font that depends on the system, for three reasons:

- **The kernel's build chooses its fonts.** With `CONFIG_FONTS` set, each `CONFIG_FONT_*` option is chosen one by one. Without it, the kernel takes VGA 8×8 and 8×16 on most machines (`lib/fonts/Kconfig`). Each distribution picks its own fonts, and a custom kernel may carry one or none.
- **The fonts exist only while the kernel has a text console.** A kernel built without `CONFIG_VT` has no VTs (`drivers/tty/Kconfig`), and the framebuffer console is an option of its own, `CONFIG_FRAMEBUFFER_CONSOLE` (`drivers/video/console/Kconfig`). Tier 3 already runs vtium as on a machine without kernel VTs, with `/dev/tty0` hidden (`NOVT_VARIANTS` in `tools/tier3/t3_scenario.py`).
- **The kernel hands out the font a VT holds now.** `KD_FONT_OP_GET` returns the VT's current font, which may be one a person loaded with `setfont`. `KD_FONT_OP_SET_DEFAULT` loads a built-in font by name, and doing so changes the console's font (`include/uapi/linux/kd.h`). vtium leaves each VT's font as its owner set it.

So each kernel-font set draws with its font when the system offers it, and with the built-in font when it does not, by the missing-font rule of D7. The kernel-font sets sit beside the two out-of-the-box sets and the built-in font, which D1 and D2 guarantee.

The fonts reach vtium in two ways. `linux-console` reads the font the VT holds when vtium starts, through the `console:` source (ADR-1200 D8), whichever font that is. So it reaches every kernel font, and also a font the person loaded, and it draws exactly what the console showed before vtium took the VT. The other sets name a file of kbd or of Terminus whose glyphs match a kernel font, so they draw the same font on every machine that has the file, whatever the console holds. On 2026-11-14 each kernel font's 222 glyphs from U+0021 to U+00FF, as code page 437 places them, were compared with every console font file on an Arch machine:

| Kernel font | Its option | The file with the same glyphs | Glyphs that match |
|---|---|---|---|
| VGA 8×16 | `FONT_8x16` | kbd's `cp865-8x16` | 218 of 222; code page 865 puts ø and Ø where 437 has ¢ and ¥ |
| VGA 8×8 | `FONT_8x8` | kbd's `cp865-8x8` | 218 of 222 |
| Sun 8×16 | `FONT_SUN8x16` | kbd's `default8x16` | 221 of 222 |
| Sun 12×22 | `FONT_SUN12x22` | kbd's `sun12x22` | 218 of 222 |
| Terminus 16×32 | `FONT_TER16x32` | Terminus's `ter-i32b`, from the terminus-font package | 222 of 222 |

No console font file on that machine matches the kernel's Mac 6×11, 7×14, Pearl 8×8, Acorn 8×8, 6×10, OLED 6×8, 10×18 or Terminus 10×18, and Mini 4×6 was not compared. Those fonts reach vtium through `linux-console` alone, while the kernel holds one of them. When the VT holds another font, `linux-console` draws that font, because the set's job is the console's current font. When the kernel has no text console, or refuses the font read, `linux-console` draws with the built-in font, and vtium logs one line that says why.

### D4: every catalog set is ready to use

Each catalog set draws every kind of character that a terminal shows. The table says what supplies each kind:

| Kind of set | Text | Box drawing, blocks, Braille, legacy computing | Powerline and Nerd Fonts icons | Color emoji | Every other character |
|---|---|---|---|---|---|
| Console, classic, code | its text font | vtium's sprites | the `nerd-symbols` layer | the `noto-emoji` layer | the `unifont` layer |
| Nerd Fonts | its text font | vtium's sprites | its text font | the `noto-emoji` layer | the `unifont` layer |
| Scripts and regions | Noto Sans Mono, and the script's layer | vtium's sprites | the `nerd-symbols` layer | the `noto-emoji` layer | the `unifont` layer |
| Small panels | Unifont or Unscii-8 | vtium's sprites | the `nerd-symbols-small` layer | the `noto-emoji-small` layer | the `unifont` layer |

Behind a bitmap text font, the icons of an outline symbols font draw 1-bit, so they match the bitmap font around them (ADR-1200 D1, `render=`). A color emoji keeps its color behind every text font. Where a font of the set is missing, the built-in font draws the characters of its list in that font's place (D1).

### D5: a font set is built from a text font and layers

A **layer** is a file that adds sources for one role. It lives in the `layers/` folder of the frits' directory as `NAME.vtlayer`. Its first key, `role=`, names its role: `script`, `symbols`, `emoji` or `fallback`. Its other lines are `source=` lines and the keys that describe a source: `cover=`, `fit=`, `render=`, `style=`, and the five keys of D7.

A font set adds a layer with a `layer=NAME` line. vtium reads the set's lines in order and puts the layer's sources in place of that line. A layer holds sources alone. The set's own keys, such as `cell=`, `size=` and `line-spacing=`, apply to every source, the layers' included.

The set's own first source is its text font, and it sets the character cell (ADR-1200 D17). When the text font is on the machine, the rule is the one that ADR-1200 D17 gives. When the text font is missing, the built-in font takes its place: the cell is the set's `cell=`, else the cell for the configured size, as for a console with no font named. So a layer's font never sets the cell.

The console reads each layer with the same ownership check it gives a frit, and sends the helper the set with its layers in place. The helper sees one list of sources, as it does now. `vtium config check` reads a set's layers too.

The catalog's layers:

| Layer | Role | Its sources |
|---|---|---|
| `nerd-symbols` | symbols | Symbols Nerd Font Mono, covering U+E000–U+F2FF, U+F386–U+F8FF and U+F0001–U+F1AF0. Font Logos, U+F300–U+F385, stays out, because it carries no license grant. |
| `noto-emoji` | emoji | Noto Color Emoji |
| `nerd-symbols-small`, `noto-emoji-small` | symbols, emoji | the chosen ranges and emoji of today's `minspec-16` and `minspec-8` |
| `arabic`, `hebrew`, `indic`, `thai-lao` | script | the script's Noto fonts, as today's script sets name them, with their `fit=` lines |
| `cjk-sc`, `cjk-tc`, `cjk-jp`, `cjk-kr` | script | Noto Sans CJK in the region's face |
| `unifont` | fallback | GNU Unifont 17.0.03 as a BDF file, which the vtium package installs. It draws every character that the sources before it lack, in 1 bit, as the compiled-in Unifont draws them today. |

A person composes a set from the same parts. `vtium config new-frit FONT` takes `--layer NAME`, once for each layer, and its TUI view offers the layers as choices. For example, `vtium config new-frit "Iosevka Term" --layer arabic --layer nerd-symbols --layer noto-emoji` writes a set with Iosevka Term for text, the Arabic layer, Nerd Fonts' icons and color emoji.

### D6: each source has a role, and the roles come in one order

A catalog set has one text font, its own first source. That font's own bold and italic faces may follow it with `style=` lines. Each layer brings one role, and supplies only characters of that role:

| Role | What it brings | The characters it supplies |
|---|---|---|
| Script | a writing system that the text font lacks | characters whose Script_Extensions name the script, and combining marks (Script=Inherited). A layer for Han, kana and Hangul also supplies wide and fullwidth characters (East_Asian_Width W or F). |
| Symbols | the icons that prompts and file listers draw | the ranges that the symbol font's project documents as its own, written as `cover=` lines |
| Emoji | color emoji | characters with Unicode's Emoji property, and the emoji components |
| Fallback | the rest of Unicode | every character that the sources before it lack |

A source fills only the characters that every source before it lacks (ADR-1200 D1). So a layer's `cover=` lines keep its font to its role wherever the font holds more.

The sources come in this order: the text font, then the script layers, then the symbols layer, then the emoji layer, then the fallback layer, and last the built-in font (D1). Text and script fonts come first, so each character they hold draws as text, the default for a character whose Emoji_Presentation is No. The emoji layer comes last. The helper still takes a color glyph from it for every character that asks for emoji presentation, wherever the layer stands (ADR-1200 D21).

A set holds one symbols layer, one emoji layer and one fallback layer at most, and one Han face at most, because each face draws one region's forms. A Nerd Fonts set takes its symbols from its text font, and it adds the emoji layer alone.

The fonts are chosen by these rules. A text font is monospaced, holds every printable ASCII character, and is the Regular face of its family. A script layer uses Noto, which has a face for nearly every script under one license, and which the common distributions package. A script set pairs Noto Sans Mono with that layer, so its Latin text and its script share one design. A script font may be proportional, because vtium fits each glyph to its cells (ADR-1302 D2). Metrics need to match only loosely, because vtium draws each later outline source at the text font's size and on its baseline (ADR-1200 D17).

### D7: a set names each font precisely

Five keys follow a `source=` line, in a set or in a layer, and describe the font that the source needs:

| Key | What it states | What vtium does with it |
|---|---|---|
| `version=` | the lowest version that the set was written against, as the font's own version string gives it | A font of a lower version still draws, and vtium-config reports its version and the version the set asks for. |
| `check=` | characters that the right font holds, as code points or ranges | A font that lacks any of them counts as missing. For example, `check=U+E0B0` tells a Nerd Fonts build from the plain font of a similar name. |
| `sha256=` | for a source named by its path, the file's SHA-256 | A file with another hash still draws, and vtium-config reports the difference. |
| `package=` | `DISTRO:NAME`, once for each distribution, where DISTRO is the distribution's ID in os-release(5) | vtium-config names the package for this machine's ID, else for the first of its ID_LIKE entries that a line names. |
| `url=` | the font's upstream page, with the version | vtium-config names it when no `package=` line matches this machine. |

vtium draws a set with every named font that it finds and that passes its `check=`. Each other font is missing, and its characters draw from the next source that has them, as for any missing source (ADR-1200 D1). When the text font is missing, the built-in font draws the text (D5). The console logs one line for each missing font, once a boot. The line names the set, the font, its version and the package for this distribution.

vtium-config tells the person before they meet a gap. `vtium config fonts` lists each set's missing fonts with the package or the page that provides each one. The TUI's Fonts and Looks views mark a set with a missing font, and they show the install command for this distribution when the person chooses that set. `vtium check-config` prints the same lines for each set that vtium.conf names.

### D8: what vtium brings, what it requires, and the bar for each

vtium ships the catalog's sets and layers as text files. The vtium package also installs the font files of the two sets that work out of the box (D2). Every other font reaches a machine in one of two ways:

- **Brought.** The vtium-fonts package depends on the font's distribution package, so installing vtium-fonts installs the font. A brought font has one of the licenses OFL-1.1, Apache-2.0, MIT or Bitstream-Vera. Arch's own repositories package it, and Debian or Fedora package it too, in a release or in its development branch. A font is brought when a shared layer or the first member of a category uses it.
- **Required.** The set names the font with D7's keys, and the person installs it. A required font has a license that lets the person install it and use it, and it can be found: a distribution package names it, or its `url=` gives a stable upstream release. The set's `license=` line names the font's license, so the person sees it before installing.

The catalog's fonts, as read on 2026-11-14:

| Font | License | Arch | Debian | Fedora | Reaches the machine | vtium could ship the file |
|---|---|---|---|---|---|---|
| Terminus | OFL-1.1 | terminus-font | console-setup, xfonts-terminus | terminus-fonts-console | installed by vtium | yes |
| kbd's `default8x16`, `sun12x22` | kbd's GPL-2.0-or-later, and each font's own notice | kbd | kbd | kbd | required, with kbd | with the GPL's terms |
| The Linux kernel's own fonts | GPL-2.0, by each file's SPDX line or the kernel's own license | not applicable | not applicable | not applicable | required, from the system | no |
| Px437 IBM VGA 9x16 | CC-BY-SA-4.0 | AUR only | none checked | none checked | required, by `url=` | yes, with attribution and the same license |
| Glass TTY VT220 | Unlicense | none checked | none checked | none checked | required, by `url=` | yes |
| JetBrains Mono | OFL-1.1 | ttf-jetbrains-mono | fonts-jetbrains-mono | jetbrains-mono-fonts | installed by vtium | yes |
| Fira Code | OFL-1.1 | ttf-fira-code | fonts-firacode | fira-code-fonts | required | yes |
| Cascadia Code | OFL-1.1 | ttf-cascadia-code | fonts-cascadia-code | cascadia-code-fonts | required | yes |
| Hack | MIT and Bitstream-Vera | ttf-hack | fonts-hack | none found | required | yes |
| IBM Plex Mono | OFL-1.1 | ttf-ibm-plex | fonts-ibm-plex | ibm-plex-mono-fonts | required | yes |
| Source Code Pro | OFL-1.1 | adobe-source-code-pro-fonts | not found under the name tried | adobe-source-code-pro-fonts | required | yes |
| Monoid | MIT and OFL-1.1 | ttf-monoid | fonts-monoid | not checked | required | yes |
| DejaVu Sans Mono | Bitstream-Vera | ttf-dejavu | fonts-dejavu | dejavu-sans-mono-fonts | required | yes |
| The Nerd Font Mono builds of JetBrains Mono, Fira Code, Hack and Monoid | the base font's license, and MIT for the patcher | ttf-*-nerd | none found | none found | required | yes |
| CaskaydiaCove and SauceCodePro Nerd Font Mono | OFL-1.1 with Reserved Font Names, which is why the builds carry new names | ttf-cascadia-code-nerd, ttf-sourcecodepro-nerd | none found | none found | required | yes, under their new names |
| Noto Sans Mono and the Noto script fonts | OFL-1.1 | noto-fonts | fonts-noto | google-noto-sans-*-fonts | brought | yes |
| Noto Sans CJK | OFL-1.1 | noto-fonts-cjk | fonts-noto-cjk | google-noto-sans-cjk-fonts | brought | yes |
| Symbols Nerd Font Mono | MIT; its icon sets MIT, CC-BY-4.0 and Apache-2.0 | ttf-nerd-fonts-symbols-mono | fonts-nerd-symbols, forky and sid | none found | installed by vtium | yes, with the icon sets' attribution |
| Noto Color Emoji | OFL-1.1 | noto-fonts-emoji | fonts-noto-color-emoji | google-noto-color-emoji-fonts | installed by vtium | yes |
| GNU Unifont 17.0.03 | OFL-1.1, the option vtium takes of its two licenses | not in Arch's own repositories | unifont | unifont-fonts | installed by vtium | yes |
| Tiny5 Mono | OFL-1.1 | none | not checked | not checked | required, by `url=` | yes |

The DejaVu row's packages were not checked here. The catalog work checks each `package=` line before it lands. A set that takes CC-BY-4.0 icon sets gives their attribution in its own comment.

### D9: names, folders and the package lists

A catalog set is named after its font, in lowercase words joined by `-`: `fira-code`, `sun-gallant`, `noto-cjk-jp`. A Nerd Fonts set takes the build's own name where Nerd Fonts gives one, as `caskaydia-cove` and `sauce-code-pro`, and the base name with `-nerd` otherwise. A smallest-build set is named after its built-in font and what it adds: `unifont-icons`, `unscii-icons`. A layer is named after what it adds: `nerd-symbols`, `noto-emoji`, `arabic`. Names stay unique across every folder.

In vtium's tree the sets live in `crates/vtium-config/frits/CATEGORY/NAME.vtfrit`, and the layers in `crates/vtium-config/frits/layers/NAME.vtlayer`. They install into the same folders under `/etc/vtium/fonts/`. vtium finds a name in the frits' directory itself first, where a person's own sets live, and then in each category folder. vtium-config refuses to install a set whose name another folder already holds. `terminus` moves into `console/` from the recipe repository, and its `package=` lines name Terminus's package on each distribution.

The vtium package installs `jetbrains-mono`, `terminus`, `unifont-icons`, `unscii-icons` and the layers they use. The minspec builds install `terminus`, `unifont-icons`, `unscii-icons`, their layers, and the files of Terminus, Unifont and Unscii-8. The vtium-fonts package installs the whole catalog, and it depends on every brought font. The Makefile's lists name the sets by category, as ADR-1500 D8 lets a recipe name its sets.

### D10: the font set made from `font-name` holds the one font it names

The font set that vtium makes on the fly from `font-name`, a kbd name, a font path or vconsole's `FONT=` holds that one source (ADR-1200 D9). Every font set holds exactly the sources that its file and its layers name, with the built-in font at the end (ADR-1200 D7). So the file always says where each glyph comes from, and the console draws the same on every machine that has those fonts.

A person who wants color emoji and icons with a font picks the catalog set of that font, or composes one with `new-frit --layer` (D5).

### D11: test sets keep their fonts

A test set judges vtium's drawing in tier 2, tier 3, the specimens, the acceptance pass and the VT guides. Test sets stay where they are, with the fonts they name. ADR-1503 D9 chooses the fonts of tier 2 and tier 3, each a general open font pinned by its URL and SHA-256. A test set combines whatever fonts its check needs, and it names only fonts whose license lets vtium's tools fetch and use them. The tier 3 showcase sets combine many scripts with symbols and emoji, because their pictures show everything vtium draws at once. The specimens draw the catalog sets they name from the catalog's folders.

### D12: a check holds the catalog to this record

A test in vtium-config reads every catalog set and layer. Its text half runs in every build. It checks the category folder, that the name is unique, the order of the roles, that every symbols and emoji layer carries `cover=` lines or an emoji font, that every source carries `package=` or `url=`, and that the `license=` line names every source's license. Its font half runs with the pinned fonts that tier 2 and the specimens fetch. It asks the helper's engine which source supplies each character, and it checks that each layer supplies characters of its role alone, and some of them. The catalog's font table moves from this record into a TSV beside it, which the test reads.

### D13: how the existing sets map onto the catalog

| Set now | Becomes | What changes |
|---|---|---|
| `desktop` | `code/jetbrains-mono` | Renamed. Its Nerd Fonts and emoji sources become the two layers, and its last line, `source=builtin:unifont`, goes, because the built-in font ends every set (ADR-1200 D7). It gives its icon sets' attribution itself. It is the outline set that works out of the box (D2). |
| `proof` | `code/jetbrains-mono` | Folded in. It names the same fonts, and the tests that read it read `jetbrains-mono`. |
| `terminus`, in the recipe repository | `console/terminus` | Moves into vtium's tree, gains the two layers, and drops its last line, `source=builtin:unifont`. It is the bitmap set that works out of the box (D2). |
| `minspec-16`, `minspec-8` | `small/unifont-icons`, `small/unscii-icons` | Renamed. Their chosen ranges become the two small layers. |
| `tiny5` | `small/tiny5` | Gains the two small layers. |
| `arabic`, `hebrew`, `indic`, `thai-lao`, `vietnamese`, `cjk-sc`, `cjk-tc`, `cjk-jp`, `cjk-kr` | `scripts/noto-…` | Renamed. Each script's fonts become a layer, and each set gains `nerd-symbols` and `noto-emoji`. |
| The test sets | unchanged | They stay beside their tier or guide. |

Whether the built-in font's glyphs sit in the same place when the built-in font is written last and when it ends the set by itself is measured before those lines go. A written `builtin:` source is drawn by the helper, and the built-in font at the end by the console's fallback. The specimens of `desktop` are accepted goldens, so any pixel that moves goes to the maintainer in a golden commit of its own.

### D14: what follows for the work that waits on this record

- **The emoji source for the font set made from `font-name`** has its answer in D10: the person picks the catalog set of their font, or composes one with `new-frit --layer`. The ligature work that shares its issue goes on by itself.
- **The small-panel fonts for wider coverage** join the `small/` category as members or as small layers. A small font that a distribution packages may be brought. Any other is required, by `url=`.
- **The script font sets** keep their fonts, become `scripts/noto-…` sets built from layers, and gain the symbols and emoji layers. `docs/fonts-for-scripts.md` follows the new names.

The build cards, in order:

1. **Layers.** The frit reader takes `layer=` and reads `.vtlayer` files with `role=`. The console puts each layer's sources in place and checks each layer's ownership. The cell rule for a missing text font. `new-frit --layer` and the TUI's choice. ADR-1200 D1@@ D9 and D19 gain dated notes.
2. **Font requirements.** The keys `version=`, `check=`, `sha256=`, `package=` and `url=`. The helper's match against them, the console's log line, and vtium-config's report, with the os-release lookup.
3. **The catalog.** The kernel-font sets, each checked glyph by glyph against the kernel's font it names. The category folders and layers, every member of D3, the renames, the fold of `proof`, the Makefile's lists, and the docs. Specimens that move go in a golden commit of their own.
4. **The catalog check** of D12, and the TSV beside this record.
5. **The built-in font.** The list file, the build step that cuts the three strikes, the hollow box for every other character, the English message for a translation beyond the list, and the acceptance check of D1, as a unit test and a tier 3 case.
6. **The two sets that work out of the box.** The vtium package installs their font files in `/usr/share/vtium/fonts/`, `terminus` becomes the font with none named, and the tier 3 case of D2 runs on a guest with no font package.
7. **The recipes.** vtium-arch's vtium-fonts installs the catalog and keeps its dependencies on the brought fonts. The vtium recipe drops its own `terminus.vtfrit`.

### D15: the maintainer's choices

Each choice gives the options, what each trades away, and the record's lean. The decisions above follow the leans.

1. **The categories.** (a) Six: console, classic, code, Nerd Fonts, scripts and regions, and small panels. (b) Five, with each code font's Nerd build in `code/` beside it. That keeps a font's two sets together, and it makes `code/` twice as long. **Lean: (a),** because the maintainer named Nerd Fonts as a family of their own.
2. **How many members each category starts with.** (a) The list in D3: two console sets, three classic, eight code, six Nerd, nine scripts and three small. (b) One or two in each category, with more added as people ask. That keeps the first card small, and it leaves categories that look empty. **Lean: (a).**
3. **Brought or required.** (a) vtium-fonts brings the fonts of the shared layers and of the script sets, which are the fonts it depends on today, and every other font is required. (b) It also brings every required font that Arch's own repositories package, such as Fira Code and Hack. The package then installs about a dozen more fonts. (c) vtium-fonts also carries the font files of fonts that no distribution packages and whose license allows it: Glass TTY VT220, Tiny5 Mono and the Px437 fonts. That reverses ADR-1500 D8's rule that a vtium package holds text files alone. **Lean: (a).**
4. **The layering form.** (a) `layer=NAME` lines and `.vtlayer` files with a `role=`, in `layers/`. (b) `include=NAME` of a whole frit. The included frit's own keys, such as `cell=`, would then need rules for which one wins. (c) Named sections inside one file. That changes vtium.conf's line syntax that frits share. **Lean: (a).**
5. **A missing required font at run time.** (a) vtium draws with the fonts it has, the built-in font draws the text when the text font is missing, the console logs one line, and vtium-config names the package. (b) vtium draws the whole set in the built-in fonts until every required font is there. The screen then looks the same before and after a partial install, and it loses the fonts that are there. (c) As (a), plus a one-time notice on the console. The notice reaches someone who never reads the log, and it writes into the session's screen. **Lean: (a).**
6. **A font that differs from its pin.** (a) A lower version or another hash still draws, and vtium-config reports it; only a failed `check=` counts the font as missing. (b) Any difference counts the font as missing. A distribution's newer or rebuilt font would then fall back on every update. **Lean: (a).**
7. **The renames.** (a) `desktop`, `minspec-16`, `minspec-8` and the nine script sets take the catalog's names now. (b) The old names stay as sets that hold one `layer=`-style alias each. Two names then mean one set. **Lean: (a),** while few installs exist.
8. **How the two out-of-the-box sets get their fonts.** (a) The vtium package installs its own copies in `/usr/share/vtium/fonts/`, about 16.2 MB, pinned by hash. (b) The vtium package depends on the distributions' font packages. Each distribution's recipe then maps the names, and a font update can move the sets' pictures. (c) The fonts are compiled into the binary, which D1 decides against. **Lean: (a),** because the sets then draw the same on every distribution.
9. **The font with none named.** (a) `terminus`, then the built-in font. (b) `jetbrains-mono`, which looks smoother on a large display and starts the outline rasteriser on every boot. (c) The built-in font, as today. **Lean: (a),** because it reads the same at every size and on every build.
10. **Emoji on the minspec builds.** (a) The minspec builds install Terminus and Unifont alone, and `terminus` draws icons and emoji in 1 bit from Unifont until the person installs the other two fonts. (b) They install all four, 13.3 MB more on a small board. **Lean: (a).**
11. **The built-in 8×16 strike.** (a) Terminus, as D1 decides. (b) A Unifont subset. It holds the same list, its data is about the same size, 1,594 bytes deflated, its focus marks are larger, and it keeps today's look. Terminus is drawn for reading on a console, and it matches the `terminus` set. **Lean: (a).**

## Consequences

### Positive

- The minspec console shrinks by about 1.35 MB, and it still draws its own messages with no font file at all.
- A fresh install draws text, icons and color emoji with nothing else installed.
- Every catalog set draws text, box drawing, icons and color emoji, so a person who picks a font gets a working console.
- Each source in a set has a named role, and a check holds the catalog to it, so each added font has a stated reason.
- A person composes a set from the same layers the catalog uses, with one command.
- A missing font is named, with the package that provides it on the person's distribution.
- The vtium-fonts package has a reason for each dependency.

### Negative

- With no font helper or font files, a CJK or emoji character draws as a hollow box.
- The vtium package grows by about 16.2 MB of font files.
- vtium.conf files that name a renamed set need the new name.
- The frit format gains a file kind and five keys, which vtium-config's template, its check and its TUI learn.
- A required font that the person has not installed leaves the set's text in the built-in font.
- The catalog check's font half needs the pinned fonts, so it runs where tier 2's and the specimens' fonts are fetched.

### Neutral

- A person's own font sets keep working as written, with or without layers.
- The test sets keep their fonts, and ADR-1503 D9 keeps choosing them.
- ADR-1200 D9's sentence that waits on "a specification [that] decides how font sets are organized" gains a dated note that names this record.

## Alternatives Considered

- **One font for every set, with the built-in font drawing the rest.** It is the simplest rule. Every set would draw emoji and icons in 1 bit, which falls short of a ready-to-use console. It was rejected.
- **A coverage-first set,** which gathers installed fonts until most characters draw. It is the collage the maintainer named, and its look changes with each machine's fonts. It was rejected.
- **An implicit emoji source for the font set made from `font-name`,** through the key `font-emoji`. The maintainer declined it (ADR-1200 D9). D10 gives the person the same result through a set they choose.
- **A flat catalog in one folder.** It keeps the lookup simple. The maintainer asked for a structure by category, and thirty sets read better in six folders. It was rejected.
- **Each catalog set written out in full, without layers.** Each file would stand alone. The same symbols and emoji lines would repeat in thirty files, and a person could not reuse them. It was rejected.
- **Keeping Unifont compiled in.** The built-in font would draw every script with no file. It costs 1.34 MB of the minspec binary, and the `unifont` layer gives the same coverage as a file. It was rejected.
- **The Linux kernel's console fonts as the built-in font.** They have the console's look. They carry GPL-2.0, and compiling them into vtium's MIT binary would make it a GPL combination. A set may still name kbd's copies as required fonts, which redistributes nothing. It was rejected for the built-in font.
- **Spleen,** under BSD-2-Clause, has 5×8 to 32×64 sizes. It is a clean choice too. Terminus already is the catalog's console font, so the built-in font and `terminus` share one look. It was rejected.
- **Bundling every font in a vtium package.** Every set would work at once. It would carry font files that distributions already package and update, and it reverses ADR-1500 D8. It was rejected, and D15's third choice asks about the narrow case.
