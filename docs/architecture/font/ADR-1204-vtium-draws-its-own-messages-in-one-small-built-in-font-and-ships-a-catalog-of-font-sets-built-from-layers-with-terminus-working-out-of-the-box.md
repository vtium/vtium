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
  - evidence: "Terminess, read and drawn 2026-11-14: Nerd Fonts v3.4.0's Terminus.tar.xz (SHA-256 e857b18a99701528f873275b6012792caf6e30c34c512ee92cda51520140054f) holds LICENSE.txt, the OFL-1.1 with the Reserved Font Names Terminus Font and Terminus (TTF), and a README that renames the font Terminess to comply with them; TerminessNerdFontMono-Regular.ttf is 2,665,032 bytes and -Bold.ttf 2,665,872. Nerd Fonts' license-audit.md lists Font Logos as Unlicensed. Arch's extra repository packages ttf-terminus-nerd 3.5.1 (OFL-1.1-RFN, 31.74 MiB installed); Debian's sources search and Fedora's mdapi find none. Drawn through vtium-config's engine on the branch that builds render=bitmap (feat/bitmap-outline at 9e0657e0) at 8x16 and 16x32 beside Terminus 4.49.1's BDF files: at 8x16, 1,214 pixels of a 73-character sample differ from bitmap Terminus, which inks 1,039, and Terminess loses strokes in t, f, r, c and n; at 16x32, 2,418 pixels differ against 4,420 inked, and the two read alike."
  - evidence: "Measured 2026-11-14: bitmap Terminus 4.49.1 (ter-u12n to ter-u32n) and Nerd Fonts' Terminess Mono, the patched Terminus (TTF), drawn 1-bit through vtium-config's engine with render=bitmap (feat/bitmap-outline at 9e0657e0) at each strike height: the outline differs from the bitmap in 0.75, 1.20, 1.17, 1.13, 0.79, 1.26, 0.95, 0.98 and 0.55 pixels per inked bitmap pixel at 12, 14, 16, 18, 20, 22, 24, 28 and 32 pixels; it loses strokes up to 16, draws heavier strokes from 18 to 24, and reads alike at 28 and 32. The regular and bold BDF files of the nine heights are 327,548 bytes under gzip. Terminus (TTF) 4.49.3 from files.ax86.net (latest.zip, SHA-256 0ead921d98d99a4590ffe6cd66dc037fc0a2ceea1c735d866ba73fe058257577) holds TerminusTTF-4.49.3.ttf, 500,668 bytes, its Bold, Italic and Bold Italic faces, and COPYING, the OFL-1.1 with both Reserved Font Names. fontforge, which Nerd Fonts' patcher needs, is not installed on this machine, so the patched font was not built here."
  - evidence: "Cullet Mono built 2026-11-14 in the agent's scratch directory: fontforge 20251009 ran font-patcher from Nerd Fonts v3.4.0's FontPatcher.zip (SHA-256 a8f11e511ed7c69e96680858c06b50a643ea7752e26d5cd13dd5e5cc53ab1760) with --mono --fontawesome --fontawesomeext --octicons --codicons --powersymbols --pomicons --powerline --powerlineextra --material --weather, on TerminusTTF-4.49.3.ttf and TerminusTTF-Bold-4.49.3.ttf, one face per run with --name Cullet Mono and Cullet Mono Bold. CulletMono-Regular.ttf is 2,617,476 bytes and CulletMono-Bold.ttf 2,618,316; fontTools reads family Cullet Mono, styles Regular and Bold, 11,638 mapped characters each, none in U+F300-U+F385, and Devicons, Powerline and Font Awesome present. Drawn 1-bit through vtium-config's engine (feat/bitmap-outline at 9e0657e0) at 10x18 and 12x24, a 73-character sample differs from Terminess Mono in 0 pixels."
  - evidence: "Read on integration at d682f48c, 2026-11-14: every frit in the tree. crates/vtium-config/frits holds minspec-16, minspec-8, proof and tiny5. tools/specimens/frits holds desktop and the nine sets for scripts. tools/tier3/guest/frits holds thirteen test sets, tools/vt/sections/frits six, and tools/tier2/src/fonts.rs builds tier 2's test set in code. The Makefile's FRITS and FONTS_FRITS say which a package installs. crates/vtium/src/font.rs, `Wanted::source`, makes the frit for a family that font-name names with the one source font:<family>. crates/vtium-config/src/frit.rs reads one file and has no key that includes another file."
  - evidence: "vtium-arch's recipes, read through the GitHub API on 2026-11-14: vtium/PKGBUILD installs FRITS and the recipe repository's own terminus.vtfrit (source=font:ter-v16n, then source=builtin:unifont). vtium-fonts/PKGBUILD stages FONTS_FRITS and depends on noto-fonts, noto-fonts-cjk, noto-fonts-emoji, terminus-font, ttf-jetbrains-mono and ttf-nerd-fonts-symbols-mono."
  - evidence: "Package indexes read 2026-11-14. Arch (pacman -Si): ttf-fira-code OFL-1.1, ttf-cascadia-code OFL, ttf-hack custom (MIT and Bitstream Vera), ttf-ibm-plex OFL, adobe-source-code-pro-fonts OFL-1.1, ttf-monoid, ttf-dejavu, and the Nerd builds ttf-jetbrains-mono-nerd and ttf-firacode-nerd (OFL-1.1-no-RFN), ttf-cascadia-code-nerd and ttf-sourcecodepro-nerd (OFL-1.1-RFN), ttf-hack-nerd (MIT, Bitstream-Vera) and ttf-monoid-nerd (MIT, OFL-1.1-no-RFN), all in extra; kbd's files include default8x16, sun12x22, solar24x32 and ter-v16n. Debian (sources.debian.org): fonts-jetbrains-mono, fonts-firacode, fonts-cascadia-code, fonts-hack, fonts-ibm-plex, fonts-monoid, fonts-noto, fonts-noto-cjk, fonts-noto-color-emoji, console-setup, xfonts-terminus, kbd, and fonts-nerd-symbols 3.4.0+dfsg2-1 in forky and sid only. Fedora (mdapi, rawhide): jetbrains-mono-fonts, fira-code-fonts, cascadia-code-fonts, ibm-plex-mono-fonts, adobe-source-code-pro-fonts, google-noto-sans-mono-fonts, google-noto-sans-arabic-fonts, google-noto-sans-cjk-fonts, google-noto-color-emoji-fonts, terminus-fonts-console and kbd; no package found under hack-fonts or under three names for Nerd Fonts' symbols."
  - evidence: "Licenses read 2026-11-14 from each project's repository or page: JetBrains Mono, Fira Code, IBM Plex and Source Code Pro OFL-1.1 (GitHub's license field); Cascadia Code OFL-1.1 (its LICENSE); Hack MIT for Hack's work and the Bitstream Vera License for the Vera glyphs (its LICENSE.md); Monoid MIT and OFL (its README); Glass TTY VT220 the Unlicense (svofski/glasstty); Spleen BSD-2-Clause (fcambus/spleen); the Ultimate Oldschool PC Font Pack, which holds the Px437 IBM fonts, Creative Commons Attribution-ShareAlike 4.0 (int10h.org's readme); kbd GPL-2.0-or-later as a package, with each console font's own notice; Tiny5 Mono OFL-1.1 (the tiny5 frit's comment)."
  - standard: "UTS #51, Unicode Emoji, version 18.0: the Emoji property and the emoji components in emoji-data.txt"
  - standard: "UAX #24, Unicode Script Property, Unicode 18.0: Script and Script_Extensions, with Common and Inherited"
  - standard: "UAX #11, East Asian Width, Unicode 18.0: the East_Asian_Width values W and F"
  - precedent: ADR-1200
  - precedent: ADR-1500
  - precedent: ADR-1503
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "jetbrains can be one of the frit definitions that works if it's installed, but I don't want to distribute jetbrains, only terminus (and the compiled in bitmap)"
    via: the steering session, 2026-11-14, answering D15's choice on how the guaranteed fonts arrive and the D2 draft of two guaranteed sets
  - operator: aaronsb
    said: "I think I'd rather just stick with terminus"
    via: the steering session, 2026-11-14, answering D15's choice on the font with none named
  - operator: aaronsb
    said: "essentially, a complete terminus outline font is vtium's default vtfrit. in a normal 'full build package' (images, etc) we get one good frit configured with terminus, that offers as much support as possible including color emojis etc"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "this is on top of the built in bitmap font"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "our fonts package, which can be installed, can offer a redistribution of all those other fonts to fully support all the frits?"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "notices and attributions clear in an independent fonts package makes a clean break for distribution, and makes all the shipped frits work well"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "we need to be careful to not base it solely on arch packages. later, we will be building packages for other distros"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "the terminess nerd font as the one installed outline font for vtium"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "maybe it works well enough and if the user wants better coverage, the vtfrit works without it, but degraded"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "agree"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "font logos comes from the nerd font, and the docs in vtium can plainly state that to get logos and full nerd font coverage, one must install the package in the system and then vtium will be able to use it (the frit will find all its optional dependencies)"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "terminus bitmap for 8x8 is fine, and is similar to ibm codepage 437 and most minspec displays. for larger consoles we can move to the outline"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "Ooh I love 'Cullet Mono'"
    via: the steering session, 2026-11-14
  - operator: aaronsb
    said: "Accept all leans"
    via: the steering session, 2026-11-14, answering every remaining choice of D15
    covers: [exact sources, renames]
  - operator: aaronsb
    said: "I don't think it needs to know the distro command to install fonts as much as the help docs for it could advise more generally"
    via: the steering session, 2026-11-15
  - operator: aaronsb
    said: "It's just hard to track a bunch of specific commands for distros"
    via: the steering session, 2026-11-15
  - operator: aaronsb
    said: "Add the four arrows [U+2190 to U+2193 join the built-in font's list]"
    via: the steering session, 2026-11-15, the question tool, after the built-in font card found notices naming chord keys with arrows
  - operator: aaronsb
    said: "The bare word \"Terminus\" counts as the reserved font name wherever it appears in the faces, the BDF table included. The faces still state where Cullet Mono comes from: the copyright string and a description credit Terminus, because the OFL reserves the name only as the font's name and expects the original to be credited."
    via: the steering session, 2026-11-15, relaying the maintainer's decision on the review of the Cullet Mono build, in the brief of its fix round
    paraphrase: true
  - operator: aaronsb
    said: "it should be a warning"
    via: the steering session, 2026-11-16, the maintainer's answer on how check-config and vtium-config treat a missing font, during the fix round of the code review of wave 1
  - operator: aaronsb
    said: "Warn, but still install"
    via: the steering session, 2026-11-16, the maintainer's answer on how check-config and vtium-config treat a missing font, during the fix round of the code review of wave 1
  - operator: aaronsb
    said: "A distinct exit status"
    via: the steering session, 2026-11-16, the maintainer's answer on how check-config and vtium-config treat a missing font, during the fix round of the code review of wave 1
  - operator: aaronsb
    said: "Fonts come in three tiers. The built-in font, compiled into the binary, is the absolute minimum: always there, so minspec always works. Above it, vtium ships GNU Unifont as a file with every build, minspec included. Above that sits the font set a head uses. If Unifont is disabled or missing, vtium falls back to the built-in font."
    via: the steering session, 2026-11-17, relaying the maintainer's decisions on the fonts that every build ships, in the brief for this amendment
    paraphrase: true
  - operator: aaronsb
    said: "The default font set and every catalog set end with Unifont, so the normal shipped shape, minspec's included, shows few or no hollow boxes. vtium reaches for Unifont for any character its other sources lack."
    via: the steering session, 2026-11-17, relaying the maintainer's decisions on the fonts that every build ships, in the brief for this amendment
    paraphrase: true
  - operator: aaronsb
    said: "there's no reason to compress unifont super small or cut it. most minspec devices have sufficient storage to hold the entire unifont"
    via: the steering session, 2026-11-17, relaying the maintainer's decisions on the fonts that every build ships, in the brief for this amendment; the words are the maintainer's as the brief quotes them
  - operator: aaronsb
    said: "Unifont ships whole and uncompressed, with every plane, not only the Basic Multilingual Plane."
    via: the steering session, 2026-11-17, relaying the maintainer's decisions on the fonts that every build ships, in the brief for this amendment
    paraphrase: true
  - operator: aaronsb
    said: "The built-in font keeps its three strikes, 8x8 from Unscii-8 and 8x16 and 16x32 from Terminus, and its list gains every character of IBM code page 437. It stays the font of last resort, compiled in, so minspec always works."
    via: the steering session, 2026-11-17, relaying the design the maintainer confirmed for the built-in font and the two sets for text
    paraphrase: true
  - operator: aaronsb
    said: "vtium ships two font sets for text, and the person chooses between them by font-name or a look. The bitmap set, terminus, is the default with no font named: bitmap Terminus at every strike from 12 to 32 pixels, regular and bold, then GNU Unifont with every plane, then the built-in font. A larger cell draws a strike at a whole multiple, and a cell between strikes draws the next smaller strike, centred. The outline set is Cullet Mono's own set, which scales well on HiDPI displays and carries the same coverage through its layers: Unifont, emoji, and the Nerd Fonts icons Cullet Mono holds. The bitmap set no longer switches to an outline."
    via: the steering session, 2026-11-17, relaying the design the maintainer confirmed for the built-in font and the two sets for text
    paraphrase: true
  - operator: aaronsb
    said: "for minspec performance reason"
    via: the steering session, 2026-11-17, relaying the design the maintainer confirmed for the built-in font and the two sets for text; the words are the maintainer's as the steering session quotes them
  - operator: aaronsb
    said: "can opt to choose between a vtfrit with all the bitmaps (minspec, lots of coverage, performant) or outlined (scales good in hidpi, also includes all the glyph coverage)"
    via: the steering session, 2026-11-17, relaying the design the maintainer confirmed for the built-in font and the two sets for text; the words are the maintainer's as the steering session quotes them
  - operator: aaronsb
    said: "Every build, minspec included, installs the bitmap set's files: every Terminus strike and the whole Unifont, uncompressed. The full package also installs Cullet Mono and its set. Whether minspec installs Cullet Mono is not decided. Every catalog set ends with the unifont layer, which names the shipped all-planes file. terminus becomes the font with none named in the same wave as these files."
    via: the steering session, 2026-11-17, relaying the design the maintainer confirmed for the built-in font and the two sets for text
    paraphrase: true
  - operator: aaronsb
    said: "Every build installs Cullet Mono, minspec included. Minspec defaults to the bitmap set and can choose cullet-mono."
    via: the steering session, 2026-11-17, relaying the maintainer's answers to the three questions this amendment put to him, each as the record leaned
    paraphrase: true
  - operator: aaronsb
    said: "vtium draws the four missing 8x8 glyphs (U+2219, U+2302, U+2310, U+263C) itself, under MIT, in a file beside unscii-8.hex, matching Unscii's style. The maintainer looks at them at 1:1 before they are accepted."
    via: the steering session, 2026-11-17, relaying the maintainer's answers to the three questions this amendment put to him, each as the record leaned
    paraphrase: true
  - operator: aaronsb
    said: "The unifont layer leaves both private-use areas out with cover= lines, and the file ships whole."
    via: the steering session, 2026-11-17, relaying the maintainer's answers to the three questions this amendment put to him, each as the record leaned
    paraphrase: true
  - operator: aaronsb
    said: "I agree with A. [cullet-mono draws Unifont, and so CJK, from 18 pixels up] … it's not really practical for cjk below 16 pixels I think anyway"
    via: "steering session, 2026-11-18: the wave 2 fonts page and a table of three options (amend D2, round outline cells up, let Unifont overhang)"
observable:
  - "Every catalog frit passes the catalog check: its text font first, then its layers in the order script, symbols, emoji, each source supplying only characters of its role, each font named with its upstream name, version and page, and its license line naming every source's license"
  - "On a machine with every font of a catalog frit installed, its specimen draws Latin text from its text font, box drawing from vtium's sprites, a Powerline separator and a Nerd Fonts icon from its symbols source, and an emoji in color, apart from terminus, which draws emoji from Unifont in 1 bit"
  - "With fira-code chosen and Fira Code absent, the console draws text in the built-in font with the frit's symbols and emoji, logs one line that names Fira Code and its version, and vtium config fonts names Fira Code, its version and its upstream page"
  - "The frit that vtium makes for font-name=DejaVu Sans Mono holds that one source"
  - "With no vtium.conf, no font sets, no font files and no font helper, vtium reaches a login prompt drawn in the built-in font, and its notices draw in the built-in font"
  - "On a tier 3 guest with no other font installed and no font named, each build draws a letter from bitmap Terminus and a Han character and U+20000 from Unifont; with the full package and font-name=cullet-mono, it draws a letter and a Nerd Fonts icon from Cullet Mono, an emoji from Noto Color Emoji and a Han character from Unifont"
status: accepted
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

# ADR-1204: vtium draws its own messages in one small built-in font and ships a catalog of font sets built from layers, with Terminus working out of the box

## Summary

- **Decided:** vtium has one built-in font. It is the font vtium draws with when every other font, font set and configuration file is missing or unusable, and vtium's own notices, on-screen display and panic screen always use it. It is small: three bitmap strikes that hold just the characters vtium's messages, a login prompt and a shell's basic output need, compiled into the console binary and drawn without the font helper. Every other font is an ordinary file that the font helper loads. vtium ships a catalog of font sets in six categories: console fonts, classic hardware fonts, code-friendly fonts, Nerd Fonts, scripts and regions, and small panels. Each set is named after the font it is based on, and each is ready to use: it draws text, box drawing, Powerline and Nerd Fonts icons, color emoji and every other character. The console category also holds sets for the Linux kernel's own fonts, which vtium loads from the person's system and never ships. vtium ships two sets for text. `terminus`, the default, is bitmap Terminus at every strike from 12 to 32 pixels, then all of GNU Unifont, with no outline rendering. Every build, minspec included, installs it and its files. `cullet-mono` draws Cullet Mono, vtium's own outline Terminus with Nerd Fonts' icons, with color emoji and Unifont, and suits a HiDPI display. Every build installs it too. A set is built from parts: its own first source is the text font, and `layer=` lines add shared layers for a script, for symbols, for emoji and for every other character, in that order. A person composes a set from the same parts. Each set names every font it needs precisely, and vtium-config names a missing one with its version and its upstream page. A font set holds exactly the sources its file and its layers name, with the built-in font at the end.
- **Trades away:** The built-in font draws only its short list of characters. On a machine where the font helper or every font file is missing, a CJK or emoji character draws as a hollow box, where today the compiled-in Unifont draws it. A translated notice in a script beyond that list shows in English. Every build installs about 13.9 MB of font files, its own outline Terminus with Nerd Fonts' icons among them. The full package depends on the distribution's emoji package where one exists. `terminus` draws emoji in 1 bit from Unifont and no Nerd Fonts icons, so a person who wants them chooses `cullet-mono`. Several shipped sets change name, so a vtium.conf that names `desktop`, `minspec-16`, `minspec-8` or a script set changes too. The frit format grows a layer file and five keys that describe a source.
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
- the arrows U+2190 to U+2193, which vtium's notices use to name chord keys, such as Ctrl+Super+↑.
- every other character of IBM code page 437, so that the built-in font draws the PC's whole character set: ☺☻♥♦♣♠•◘◙♂♀♪♫☼►◄↕‼▬↨∟↔▲▼⌂₧ƒ⌐αΓπΣστΦΘΩδ∞φε∩≡≥≤⌠⌡≈∙√ⁿ■.

> Edited 2026-11-15: the list gains the four arrows. The scan of 2026-11-14 missed them, because the code writes them as `\u{}` escapes. Terminus lacks ten characters of the list, not eight: the ranges this decision names hold ten.

> Amended 2026-11-17: the maintainer decided that the built-in font holds every character of IBM code page 437 (`considered`, 2026-11-17). The list lacked 52 of them, which the last bullet adds, for 410 characters in all. They add 416 bytes of glyph data to the 8×8 strike, 832 to the 8×16 strike and 3,328 to the 16×32 strike, before compression. Terminus 4.49.1's `ter-u16n.bdf` and `ter-u32n.bdf` hold all 52. Unscii-8 lacks four: U+2219 ∙, U+2302 ⌂, U+2310 ⌐ and U+263C ☼.

vtium draws those four 8×8 glyphs itself, in the style of Unscii-8's own glyphs, and keeps them under vtium's MIT license in a file of its own beside `unscii-8.hex`. The build reads that file with `unscii-8.hex` when it cuts the 8×8 strike. The maintainer looks at the four glyphs at 1:1 before they are accepted. The Linux kernel's 8×8 fonts hold them too, but they carry the GPL-2.0, which this decision keeps out of the binary.

> Note 2026-11-17: the maintainer decided where the four glyphs come from, as the record leaned (`considered`, 2026-11-17). The other choice left them as hollow boxes in cells under 16 pixels.

A scan of the string and character literals in `crates/vtium/src`, `crates/vtium-render/src/notice.rs` and `crates/vtium-settings/src` on 2026-11-14 found only these characters beyond Latin-1, apart from one in a test.

The built-in font has three strikes. A cell larger than a strike draws it at the largest whole multiple that fits, centred, as ADR-1200 D7 fits a built-in glyph today.

| Strike | Draws cells | Cut from | License | Glyph data for the list |
|---|---|---|---|---|
| 8×8 | under 16 pixels high | Unscii-8, `crates/vtium-font/data/unscii-8.hex` | public domain, as `crates/vtium-font/data/unscii.txt` says | 2,832 bytes, 1,253 deflated |
| 8×16 | from 16 to 31 pixels high | Terminus 4.49.1, `ter-u16n.bdf` | OFL-1.1 with the Reserved Font Name "Terminus Font", as upstream's `OFL.TXT` says | 5,520 bytes, 1,429 deflated |
| 16×32 | from 32 pixels high | Terminus 4.49.1, `ter-u32n.bdf` | the same | 22,080 bytes, 2,539 deflated |

The 8×8 strike keeps the size that minspec's 320×240 panel reads at, 40 columns of 8-pixel characters, which the notices are written for (`TEXT_BESIDE_MIN` in `crates/vtium-render/src/notice.rs`). The two Terminus strikes give the larger heads a font drawn for reading on a console, and the 16×32 strike gives a 4K head a sharp font at twice the size. Terminus holds every character of the list apart from ten box and block characters, U+2504 to U+2507, U+254C to U+254F, U+2594 and U+2595; vtium's sprites draw those.

The strikes come from upstream files. Every font in the Linux kernel's `lib/fonts/`, its copy of Terminus `font_ter16x32.c` included, carries `SPDX-License-Identifier: GPL-2.0`, and vtium is MIT. Upstream Terminus carries the OFL, which lets a font be bundled with software when each copy carries the font's copyright notice and the license. A subset is a modified version, and "Terminus Font" is a reserved name, so vtium names its strikes "vtium built-in". COPYING carries Terminus's copyright line, and the package carries the OFL text that `crates/vtium-font/data/OFL-1.1.txt` already holds.

The binary carries all of GNU Unifont today, 1,344,071 bytes of compressed glyph chunks, and Unscii-8, 15,494 bytes. The built-in font's glyph data comes to about 5.2 KB deflated, so the minspec console shrinks by about 1.35 MB, from 4,093,032 bytes to about 2.74 MB against its 4,194,304-byte budget. Unifont stays in the catalog as a file, the `unifont` layer (D5).

When the built-in font is all vtium has, it draws each character of its list. Every other character draws as a hollow box as wide as its cells, which vtium draws itself, as it draws its sprites. A zh, ja or ko console therefore shows hollow boxes for Han, kana and Hangul until its font set's glyphs arrive. vtium shows a translated message when the built-in font holds every character of it, and the English message otherwise, so a notice always reads (ADR-1402).

The acceptance check: vtium starts with no vtium.conf, no font sets, no font files and no font helper. It reaches a login prompt drawn in the built-in font, and its notices draw in the built-in font. A unit test in vtium checks the drawing with an empty configuration directory and no helper. A tier 3 case removes `/etc/vtium`, vtium's font files and the helper from the guest, restarts vtium's service, and compares the login prompt and a notice with their goldens.

> Note 2026-11-15: card 5 of D14 lands in two parts. Card 5a compiles the built-in font in and draws vtium's notices, on-screen display, focus mark and panic screen in it. In those messages, a character outside the list draws as the hollow box. Card 5a also gives vtium the check that shows a translation beyond the list in English. The terminal itself still draws in the compiled-in GNU Unifont and Unscii-8 when no font is named, so with nothing installed the login prompt draws in Unifont. Card 5b removes Unifont and Unscii-8 from the binary and moves the terminal to the built-in font. Only then does the acceptance check's login prompt draw in the built-in font, and only then does a character outside the list draw as a hollow box in the terminal.

### D2: the default set, `terminus`, and the outline set, `cullet-mono`

vtium draws with three tiers of fonts. The built-in font (D1) is the last resort. vtium always has it, and it always draws vtium's own messages, so a minspec console always works. Above it, every build installs GNU Unifont as a file, the `unifont` layer (D5). Above Unifont sits the font set that a head uses. A character that none of the set's own sources holds draws from Unifont. Where Unifont's file is missing, or a person's own set leaves the `unifont` layer out, vtium falls back to the built-in font. A character that the built-in font lacks too draws as a hollow box.

vtium ships two font sets for text, both in `console/`, and the person chooses between them with `font-name` or a look. The maintainer: the person "can opt to choose between a vtfrit with all the bitmaps (minspec, lots of coverage, performant) or outlined (scales good in hidpi, also includes all the glyph coverage)".

- **`terminus`, the bitmap set,** is the default, which vtium draws when no font is named. The set holds bitmap Terminus at every strike from 12 to 32 pixels, then Unifont, then the built-in font. Drawing it needs no outline rendering, which the maintainer wants "for minspec performance reason".
- **`cullet-mono`, the outline set,** draws Cullet Mono, vtium's own outline Terminus with Nerd Fonts' icons (below). An outline scales to any cell, so this set suits a HiDPI display. Through its layers the set has the coverage of the bitmap set and more: Nerd Fonts' icons, color emoji, and Unifont.

**The bitmap set.** `terminus` names bitmap Terminus 4.49.1's nine strikes of 12, 14, 16, 18, 20, 22, 24, 28 and 32 pixels, regular and bold, as one font at nine heights. The key that does this is `strike=`, which now follows a bitmap source as well as an outline one. A bitmap source and its strikes are one font, and vtium picks the strike for the cell. These are the set's source lines, with D7's `version=`, `sha256=` and `url=` lines left out:

```
source=bdf:/usr/share/vtium/fonts/ter-u16n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u12n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u14n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u18n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u20n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u22n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u24n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u28n.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u32n.bdf.gz
source=bdf:/usr/share/vtium/fonts/ter-u16b.bdf.gz
style=bold
strike=bdf:/usr/share/vtium/fonts/ter-u12b.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u14b.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u18b.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u20b.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u22b.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u24b.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u28b.bdf.gz
strike=bdf:/usr/share/vtium/fonts/ter-u32b.bdf.gz
layer=unifont
```

When the set states no `cell=` and no font size is configured, vtium takes the cell of the tallest strike no taller than the automatic rule's height (ADR-1200 D17). Above 32 pixels it takes a whole multiple of the 32-pixel strike's cell. So each glyph fills its cell exactly. A cell that `cell=` or a font size puts between two strikes draws the next smaller strike, centred in the cell. A cell taller than 32 pixels draws the 32-pixel strike at the largest whole factor that fits, centred. Unifont draws in each cell that its 16-pixel glyphs fit at a whole factor, so from 16 pixels up.

`terminus` names no symbols layer and no emoji layer, because both are outline fonts. So `terminus` is the one catalog set that draws no color emoji. Unifont draws emoji in 1 bit, and a Nerd Fonts icon draws as a hollow box, because the `unifont` layer leaves the private-use areas out (D5). A person who wants color emoji and the icons chooses `cullet-mono`.

**An 8-pixel cell.** Terminus has no strike under 12 pixels. A cell 8 pixels high, which ADR-1200 D17's small-display rule picks on a 320×240 panel, draws the built-in font's 8×8 strike, cut from Unscii-8, whose shapes follow the IBM PC's code page 437 font. That holds with `terminus` named too, because no strike fits and the set falls through to the built-in font. A Terminus 6×12 cell would give a 320×240 panel 53 columns of 20 rows, against 40 columns of 30 rows at 8×8, and the notices are written for 40 columns. So vtium keeps the 8×8 cell there, and `terminus` draws its 12-pixel strike only where the cell is 12 pixels high.

**The outline set, and vtium's own outline Terminus.** Nerd Fonts' Terminess holds the Font Logos icons, so vtium builds its own patched font. The build takes Terminus (TTF) 4.49.3 by Tilman Blumenbach, `TerminusTTF-4.49.3.ttf` and `TerminusTTF-Bold-4.49.3.ttf`, from its release archive (SHA-256 0ead921d98d99a4590ffe6cd66dc037fc0a2ceea1c735d866ba73fe058257577). Their license is the OFL-1.1, with the Reserved Font Names "Terminus Font" and "Terminus (TTF)". The build runs Nerd Fonts' patcher, under MIT, at a pinned release, with every icon set apart from Font Logos. It names the result "Cullet Mono", the maintainer's choice, after cullet, the broken glass that a glassmaker melts again with new glass. That name uses neither reserved name, and it leaves "Terminess" to Nerd Fonts. The build is reproducible from pinned sources, each with its SHA-256, and the notices list each icon set's license. Built on 2026-11-14 with Nerd Fonts' patcher 3.4.0, `--mono` and every icon-set option but `--fontlogos`, Cullet Mono Regular is 2,617,476 bytes and Bold 2,618,316 bytes. Each maps 11,638 characters, none of them in U+F300 to U+F385. Drawn through vtium's font helper at 18 and 24 pixels, Cullet Mono matches Nerd Fonts' Terminess pixel for pixel.

(Amended 2026-11-15: the maintainer reads the bare word "Terminus" as the reserved name wherever it names the font, the names of the bitmap strikes in FontForge's `BDF ` table included, so no name of Cullet Mono holds it. The license reserves the word only as the font's name, so the copyright notice and the description of each face still state that Cullet Mono is derived from Terminus Font and Terminus (TTF).)

`cullet-mono` names Cullet Mono's two faces, then the `nerd-symbols` layer, so that it finds Symbols Nerd Font Mono once the system has it installed and draws the Font Logos icons from it. Then come `noto-emoji` and `unifont`. These are its source lines, with D7's keys left out:

```
source=outline:/usr/share/vtium/fonts/CulletMono-Regular.ttf
source=outline:/usr/share/vtium/fonts/CulletMono-Bold.ttf
style=bold
layer=nerd-symbols
layer=noto-emoji
layer=unifont
```

Cullet Mono sits in `console/` beside `terminus`, because both draw the Terminus design. A Nerd Fonts set adds the emoji layer alone (D6). `cullet-mono` adds `nerd-symbols` too, because Cullet Mono leaves Font Logos out.

**What draws each cell height.** At every height vtium's sprites draw box drawing, blocks, Braille and the legacy computing shapes.

| Cell height | `terminus`, the bitmap set | `cullet-mono`, the outline set |
|---|---|---|
| under 12 pixels | The built-in font's 8×8 strike draws the text and every character of its list, code page 437 included. Every other character draws as a hollow box. | Cullet Mono draws the text and the icons, and Noto Color Emoji the emoji. Unifont's glyphs fit no cell under 16 pixels, so the built-in font draws the rest of its list. |
| 12 to 15 pixels | The 12- or 14-pixel strike draws the text. A character Terminus lacks draws from the built-in font's 8×8 strike when its list holds it, and as a hollow box otherwise. | As under 12 pixels. |
| 16 to 31 pixels | The strike of that height, or the next smaller one, draws the text. Unifont draws every character Terminus lacks, at its own size. | Cullet Mono draws the text and the icons, and Noto Color Emoji the emoji. From 18 pixels, Unifont draws the rest at its own size. At 16 and 17 pixels Cullet Mono's cell is 7 pixels wide, too narrow for Unifont's 8-pixel glyphs, so Han, kana and Hangul draw as hollow boxes. |
| 32 pixels and above | The 32-pixel strike draws the text at the largest whole factor that fits. Unifont draws the rest at its largest whole factor: twice its size from 32 pixels, three times from 48. | Cullet Mono, Noto Color Emoji and Unifont draw as from 18 pixels, with Unifont at its largest whole factor: twice its size once the cell is 16 pixels wide, from 35 pixels high. |

> Amended 2026-11-18: the maintainer decided that `cullet-mono` draws CJK through Unifont from 18 pixels up (`considered`, 2026-11-18). Cullet Mono's glyphs are half an em wide and its line is 1.09 em tall, so its cell is 0.46 times as wide as it is high: 7×16 at 16 pixels, 8×18 at 18 and 11×24 at 24. The tier 3 check of this decision found the 7-pixel cell. The table said that Unifont drew in `cullet-mono` from 16 pixels. The other choices were to round an outline font's cell width up, which widens every outline font's spacing, or to let Unifont's glyphs overhang the cell and lose a column. The maintainer: "it's not really practical for cjk below 16 pixels I think anyway". A console that needs CJK at 16 or 17 pixels draws `terminus`.

**What each build installs.** vtium installs its font files in `/usr/share/vtium/fonts/`, which is in the build's list of font folders (ADR-1200 D25). Each set names its files by path with their `sha256=` lines (D7), and `make install` writes the package's prefix into the paths.

| Characters | Drawn from | How the font reaches the machine |
|---|---|---|
| Text in `terminus` | bitmap Terminus 4.49.1, the eighteen BDF files of its nine strikes, regular and bold | every build installs them, 327,314 bytes under gzip |
| Text and Nerd Fonts icons in `cullet-mono` | Cullet Mono | every build installs its two faces, 5,234,744 bytes |
| Box drawing, blocks, Braille and legacy computing | vtium's sprites | built in |
| Color emoji in `cullet-mono` | Noto Color Emoji, through `noto-emoji` | the full package's recipe depends on the distribution's package that provides Noto Color Emoji, where one exists |
| Every other character, such as Han, kana and Hangul, in both sets | GNU Unifont 17.0.03, every plane, through `unifont`, in 1 bit | every build installs the file uncompressed, 8,329,308 bytes (D5) |
| The Font Logos icons, U+F300 to U+F385, in `cullet-mono` | Symbols Nerd Font Mono, through `nerd-symbols`, once the system has it installed; a hollow box until then | a Nerd Fonts build, which the person installs; vtium-config names the font and its page |

> Edited 2026-11-18: the table gave Cullet Mono's two faces as 5,235,792 bytes. The faces that `make cullet-mono` builds, and that `tools/cullet-mono/pins.tsv` pins, are 2,616,944 and 2,617,800 bytes, 5,234,744 in all.

Every build, minspec included, installs both sets and their files: `terminus` with the Terminus and Unifont files, about 8.7 MB, and `cullet-mono` with Cullet Mono's faces, about 13.9 MB of font files in all. The maintainer: "there's no reason to compress unifont super small or cut it. most minspec devices have sufficient storage to hold the entire unifont". A minspec console draws `terminus` when no font is named, and the person can choose `cullet-mono`. The full package, the build with images, the GPU path and bidi that the AUR `vtium` package builds, also depends on the distribution's package that provides Noto Color Emoji. Where a distribution packages none, `cullet-mono` still works, and emoji draw as hollow boxes until the person installs the font.

> Note 2026-11-17: the maintainer decided that the minspec builds install Cullet Mono too, as the record leaned (`considered`, 2026-11-17). Cullet Mono adds less than Unifont does, and an outline costs drawing time only when the person chooses `cullet-mono`. No distribution packages Cullet Mono, so a minspec machine has no other way to get it. The other choice left the minspec builds without Cullet Mono's faces, 5.2 MB that a small display seldom uses.

With no font named, vtium draws `terminus` where its files read, and the built-in font otherwise. On a display where ADR-1200 D17's small-display rule picks 8-pixel rows, such as a 320×240 panel, vtium draws the built-in font's 8×8 strike instead. The order in which vtium chooses a font becomes: the set that a head's look names, then `font-name`, then vconsole's font, then `terminus`, then the built-in font (ADR-1200 D9, D11).

The check is a tier 3 case on a guest with no other font package installed. With each build's install tree and no font named, it draws the specimen at 16, 24 and 40 pixels and asks the font helper which source drew each character. A letter and a Powerline separator come from Terminus's strike of that height at 16 and 24 pixels, and from the 32-pixel strike at 40. A Han character and a character beyond the Basic Multilingual Plane, such as U+20000, come from Unifont, and an emoji draws from Unifont in 1 bit. With the full package's tree and `font-name=cullet-mono`, a letter and a Nerd Fonts icon come from Cullet Mono at each height, an emoji from Noto Color Emoji, and the Han character from Unifont at 24 and 40 pixels and as a hollow box at 16. A Font Logos icon draws as a hollow box until the case installs the distribution's Symbols Nerd Font Mono and restarts vtium.

> Note 2026-11-18: the tier 3 case `font-sources` checks less than this paragraph describes, in two places. It draws `terminus` with no font named only on the minspec build's install tree, and it runs the full package's tree only with `font-name=cullet-mono`. Both trees install the same `terminus` set and the same font files, and they differ only in the console's features, so the case runs the default set once. The case also leaves out the step that installs Symbols Nerd Font Mono and restarts vtium. It checks that the Font Logos icon draws as a hollow box, but not that the icon draws once the font is installed. The guest image holds no Symbols Nerd Font Mono, and adding it would change the base image that every tier 3 variant boots.

> Amended 2026-11-17: the maintainer split the default set into two sets for text (`considered`, 2026-11-17): `terminus`, which holds every bitmap strike of Terminus and no outline, and `cullet-mono`, which holds Cullet Mono and its layers. The person "can opt to choose between a vtfrit with all the bitmaps (minspec, lots of coverage, performant) or outlined (scales good in hidpi, also includes all the glyph coverage)". Every build installs `terminus` with Terminus's nine strikes and the whole of Unifont. D2 described one set in two tiers. `terminus` drew bitmap Terminus's 12, 14 and 16 pixel strikes in cells up to 16 pixels, and Cullet Mono, through `strike=` under its outline source, in every cell above 16 pixels and for the icons. The maintainer had chosen the outline for every larger console on 2026-11-14, heavier strokes included: "terminus bitmap for 8x8 is fine, and is similar to ibm codepage 437 and most minspec displays. for larger consoles we can move to the outline". `terminus` also named `nerd-symbols` and `noto-emoji`. The full package installed the three strikes, 97,162 bytes under gzip, Cullet Mono, and Unifont's file, about 1.7 MB under gzip, about 7.0 MB of font files in all. The minspec builds installed bitmap Terminus's 16-pixel files alone, `ter-u16n.bdf` and `ter-u16b.bdf`, 33,383 bytes under gzip, and drew Han, kana and Hangul as hollow boxes. The tier 3 check drew the full package's letter from Cullet Mono at 26 pixels. The Summary, the last observable, D3's `terminus` row, D4, D5's `unifont` row, D8's first bullet and its Unifont row, D9's package lists, D13's `terminus` row and the Consequences said the same, and this note covers them.

### D3: the catalog and its categories

vtium ships a catalog of font sets. Each set is named after the font it is based on, and each belongs to one category. The categories and their first members:

| Category | Folder | What its sets are for | First members |
|---|---|---|---|
| Console fonts | `console/` | The bitmap fonts of the Linux console: kbd's, Terminus, and the Linux kernel's own, and Cullet Mono, vtium's outline Terminus. | `terminus`, `cullet-mono`, `default8x16`, `linux-console`, `linux-vga-8x16`, `linux-vga-8x8`, `linux-sun-8x16`, `linux-sun-12x22`, `linux-ter-16x32` |
| Classic hardware fonts | `classic/` | The look of a historic terminal or workstation, from re-creations of its font. | `ibm-vga`, `sun-gallant`, `dec-vt220` |
| Code-friendly fonts | `code/` | Outline fonts designed for code, on a display of ordinary size. | `jetbrains-mono`, `fira-code`, `cascadia-code`, `hack`, `ibm-plex-mono`, `source-code-pro`, `monoid`, `dejavu-sans-mono` |
| Nerd Fonts | `nerd/` | Code fonts in their Nerd Fonts build, which carries the icons inside the font. | `jetbrains-mono-nerd`, `caskaydia-cove`, `sauce-code-pro`, `hack-nerd`, `fira-code-nerd`, `monoid-nerd`, `terminess` |
| Scripts and regions | `scripts/` | A writing system beside Latin text. Noto Sans Mono draws the Latin text, and the script's Noto font draws the script. | `noto-arabic`, `noto-hebrew`, `noto-indic`, `noto-thai-lao`, `noto-vietnamese`, `noto-cjk-sc`, `noto-cjk-tc`, `noto-cjk-jp`, `noto-cjk-kr` |
| Small panels | `small/` | Cells of 8×8 to 8×16 on the minspec build. Unifont or Unscii-8 draws the text, as a file, and the set adds a chosen list of icons and emoji. | `unifont-icons`, `unscii-icons`, `tiny5` |

The members, their fonts and their sources:

| Set | Based on | Its sources, in order | The fonts |
|---|---|---|---|
| `terminus` | Terminus, its nine bitmap strikes | bitmap Terminus with its strikes (D2) | installed by vtium |
| `cullet-mono` | Cullet Mono, vtium's outline Terminus with Nerd Fonts' icons | Cullet Mono; `nerd-symbols`; `noto-emoji` | installed by vtium (D2) |
| `default8x16` | kbd's `default8x16` | kbd's `default8x16`; `nerd-symbols`; `noto-emoji` | required |
| `linux-console` | the font the Linux console holds when vtium starts | the VT's font, through `console:`; `nerd-symbols`; `noto-emoji` | required, from the system |
| `linux-vga-8x16`, `linux-vga-8x8`, `linux-sun-8x16`, `linux-sun-12x22`, `linux-ter-16x32` | the Linux kernel's font of that name | the file of kbd or Terminus that holds the same glyphs (below); `nerd-symbols`; `noto-emoji` | required |
| `ibm-vga` | Px437 IBM VGA 9x16, of the Ultimate Oldschool PC Font Pack | Px437 IBM VGA 9x16; `nerd-symbols`; `noto-emoji` | required |
| `sun-gallant` | kbd's `sun12x22`, Sun's Gallant console font | kbd's `sun12x22`; `nerd-symbols`; `noto-emoji` | required |
| `dec-vt220` | Glass TTY VT220 | Glass TTY VT220; `nerd-symbols`; `noto-emoji` | required |
| `jetbrains-mono` | JetBrains Mono | JetBrains Mono; `nerd-symbols`; `noto-emoji` | required |
| `fira-code`, `cascadia-code`, `hack`, `ibm-plex-mono`, `source-code-pro`, `monoid`, `dejavu-sans-mono` | the font of the same name | that font; `nerd-symbols`; `noto-emoji` | required |
| `jetbrains-mono-nerd`, `caskaydia-cove`, `sauce-code-pro`, `hack-nerd`, `fira-code-nerd`, `monoid-nerd`, `terminess` | the font's Nerd Font Mono build | that build; `noto-emoji` | required |
| `noto-arabic`, `noto-hebrew`, `noto-indic`, `noto-thai-lao`, `noto-cjk-sc`, `noto-cjk-tc`, `noto-cjk-jp`, `noto-cjk-kr` | Noto Sans Mono and the script's Noto font | Noto Sans Mono; the script's layer; `nerd-symbols`; `noto-emoji` | brought |
| `noto-vietnamese` | Noto Sans Mono, which holds every Vietnamese letter | Noto Sans Mono; `nerd-symbols`; `noto-emoji` | brought |
| `unifont-icons`, `unscii-icons` | GNU Unifont or Unscii-8, as files | Unifont or Unscii-8; `nerd-symbols-small`; `noto-emoji-small` | required |
| `tiny5` | Tiny5 Mono | Tiny5 Mono's BDF file; `nerd-symbols-small`; `noto-emoji-small` | required |

The last column says how the font reaches a machine without vtium-fonts. Once vtium-fonts is installed, every font of the catalog is there, apart from a Nerd Fonts build on a distribution that packages none (D8). Every vtium build installs the Terminus and Unifont files of `terminus` itself, so that set works out of the box, and Cullet Mono's faces for `cullet-mono` (D2). Every set ends with the `unifont` layer, which the table leaves out. The script sets keep the line spacing they have now (ADR-1302 D13). Each set states its starting `size=` or its `cell=`, as the set it replaces does.

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
| Nerd Fonts | its text font; without it, the `unifont` layer | vtium's sprites | its text font; without it, a hollow box | the `noto-emoji` layer | the `unifont` layer |
| Scripts and regions | Noto Sans Mono, and the script's layer | vtium's sprites | the `nerd-symbols` layer | the `noto-emoji` layer | the `unifont` layer |
| Small panels | Unifont or Unscii-8 | vtium's sprites | the `nerd-symbols-small` layer | the `noto-emoji-small` layer | the `unifont` layer |

`terminus` is the one exception. The set names no symbols or emoji layer, so drawing it needs no outline rendering, and Unifont draws its emoji in 1 bit (D2).

Behind a bitmap text font, the icons of an outline symbols font draw 1-bit, so they match the bitmap font around them (ADR-1200 D1, `render=`). A color emoji keeps its color behind every text font. A Nerd Fonts set works without its Nerd font, degraded: its text draws from the `unifont` layer and its icons as hollow boxes, and vtium-config names the font that completes it and its upstream page (D8). Where a font of the set is missing, the built-in font draws the characters of its list in that font's place (D1).

### D5: a font set is built from a text font and layers

A **layer** is a file that adds sources for one role. It lives in the `layers/` folder of the frits' directory as `NAME.vtlayer`. Its first key, `role=`, names its role: `script`, `symbols`, `emoji` or `fallback`. Its other lines are `source=` lines and the keys that describe a source: `cover=`, `fit=`, `render=`, `style=`, `strike=`, and the four keys of D7.

> Edited 2026-11-16: D7 names four keys, `version=`, `check=`, `sha256=` and `url=`, so this sentence says four where it said five. `strike=` joins the list, because a layer's outline source takes it as a set's does. The five new keys that the Summary and the Consequences count are D7's four and D2's `strike=`.

A font set adds a layer with a `layer=NAME` line. vtium reads the set's lines in order and puts the layer's sources in place of that line. A layer holds sources alone. The set's own keys, such as `cell=`, `size=` and `line-spacing=`, apply to every source, the layers' included.

The set's own first source is its text font, and it sets the character cell (ADR-1200 D17). When the text font is on the machine, the rule is the one that ADR-1200 D17 gives. When the text font is missing, the built-in font takes its place: the cell is the set's `cell=`, else the cell for the configured size, as for a console with no font named. So a layer's font never sets the cell.

The console reads each layer with the same ownership check it gives a frit, and sends the helper the set with its layers in place. The helper sees one list of sources, as it does now. `vtium config check` reads a set's layers too.

The catalog's layers:

| Layer | Role | Its sources |
|---|---|---|
| `nerd-symbols` | symbols | Symbols Nerd Font Mono, covering U+E000–U+F8FF and U+F0001–U+F1AF0, Font Logos included |
| `noto-emoji` | emoji | Noto Color Emoji |
| `nerd-symbols-small`, `noto-emoji-small` | symbols, emoji | the chosen ranges and emoji of today's `minspec-16` and `minspec-8` |
| `arabic`, `hebrew`, `indic`, `thai-lao` | script | the script's Noto fonts, as today's script sets name them, with their `fit=` lines |
| `cjk-sc`, `cjk-tc`, `cjk-jp`, `cjk-kr` | script | Noto Sans CJK in the region's face |
| `unifont` | fallback | GNU Unifont 17.0.03, every plane apart from the private-use areas, as the `.hex` file that every build installs (below). Unifont draws every character that the sources before it lack, in 1 bit, as the compiled-in Unifont draws them today. |

A person composes a set from the same parts. `vtium config new-frit FONT` takes `--layer NAME`, once for each layer, and its TUI view offers the layers as choices. For example, `vtium config new-frit "Iosevka Term" --layer arabic --layer nerd-symbols --layer noto-emoji` writes a set with Iosevka Term for text, the Arabic layer, Nerd Fonts' icons and color emoji.

**The `unifont` layer's file.** The layer names `hex:/usr/share/vtium/fonts/unifont_all-17.0.03.hex`, with `version=17.0.03`, its `sha256=` and its `url=`. The file is Unifoundry's `unifont_all-17.0.03.hex.gz`, decompressed: 8,329,308 bytes, SHA-256 90a5d4dd44a5ef8d3fd02a3f397926b2f9d98640f0b86a3f050faef7bb2683b2. Unifont's 126,641 glyphs are `codepoint:bits` lines, 8×16 and 16×16, in planes 0 to 3, 14 and 15. The tree holds the same file as `crates/vtium-font/data/unifont.hex`, from which the build compiles today's built-in Unifont. vtium installs it uncompressed, so the font helper's 1 MiB limit on a gzipped file does not apply to it. Unifoundry publishes every plane only as `.hex` files and as OpenType, and its BDF file holds the Basic Multilingual Plane alone. The helper's BDF reader also takes 65,535 glyphs at most, fewer than Unifont's 126,641.

The font helper reads no `.hex` file today. The helper's sources read PSF, BDF and outline files, and only the build reads a `.hex` file, to pack the compiled-in Unifont. So the build adds a source kind, `hex:PATH`, which reads Unifont's `.hex` format. Each line holds one glyph: a code point in hex, a colon, and 32 or 64 hex digits for an 8×16 or a 16×16 glyph. vtium reads a `hex:` file uncompressed, up to the 16 MiB that bounds every bitmap file, with one glyph at most for each code point. A `.hex` file states no version, so vtium-config checks it by its `sha256=` line, as D7 treats any font that states no version.

**Unifont's private-use glyphs.** The all-planes file holds 11,690 glyphs from the ConScript Unicode Registry, a private registry of invented scripts such as Klingon. 3,432 sit in the Basic Multilingual Plane's Private Use Area and 8,258 in plane 15. The Nerd Fonts icons use the same code points: U+E000 to U+F8FF and U+F0001 to U+F1AF0, where 6,111 of the plane 15 glyphs fall. So the layer carries two `cover=` lines, `U+0000-U+DFFF` and `U+F900-U+EFFFF`, which leave both private-use areas out, and the file ships whole. A Nerd Fonts icon that no earlier source holds then draws as a hollow box in every set, `terminus` included, and not as a letter of an invented script.

> Note 2026-11-17: the maintainer decided that the layer leaves the private-use areas out, as the record leaned (`considered`, 2026-11-17). The other choice let Unifont draw them, as the compiled-in Unifont does today.

> Amended 2026-11-17: the `unifont` layer named Unifoundry's `unifont-17.0.03.bdf`, which holds the Basic Multilingual Plane alone, decompressed from its gzipped file because the font helper reads a gzipped file of 1 MiB at most. The person installed it as a required font. Every build now installs the all-planes `.hex` file uncompressed (D2).

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

> Note 2026-11-18: ADR-1205, accepted on this date, changes this decision. A look is assembled by role, and the roles split finer (ADR-1205 D6). Box drawing joins as a role: vtium's own sprites fill it by default, and a font fills it when the look names one, as `glyph-source` chooses today (ADR-1200 D1). Box drawing is not a place in the order of the sources. The Symbols role is named symbols and icons. The one Script role becomes a role for each script by name, such as CJK, Arabic, Hebrew, Indic, and Thai and Lao. The text, emoji and fallback roles, and the order of the roles, stay as this decision says. One font may fill every role, or several fonts may share them.

### D7: a set names each font precisely

Four keys follow a `source=` line, in a set or in a layer, and describe the font that the source needs:

| Key | What it states | What vtium does with it |
|---|---|---|
| `version=` | the lowest version that the set was written against, as the font's own version string gives it | A font of a lower version still draws, and vtium-config reports its version and the version the set asks for. |
| `check=` | characters that the right font holds, as code points or ranges | A font that lacks any of them counts as missing. For example, `check=U+E0B0` tells a Nerd Fonts build from the plain font of a similar name. |
| `sha256=` | for a source named by its path, the file's SHA-256 | A file with another hash still draws, and vtium-config reports the difference. |
| `url=` | the font's upstream page, with the version | vtium-config names it beside the font's name. |

vtium draws a set with every named font that it finds and that passes its `check=`. Each other font is missing, and its characters draw from the next source that has them, as for any missing source (ADR-1200 D1). When the text font is missing, the built-in font draws the text (D5). The console logs one line for each missing font, once a boot. The line names the set, the font and its version.

vtium-config tells the person before they meet a gap. `vtium config fonts` lists each set's missing fonts, each by its name, its version and its upstream page. The TUI's Fonts and Looks views mark a set with a missing font, and name the missing fonts when the person chooses that set. `vtium check-config` prints the same lines for each set that vtium.conf names.

vtium-config names fonts and leaves installing them to the person (amended 2026-11-15). The maintainer chose general advice over a command for each distribution, because a list of commands for each distribution is hard to keep current. vtium's help page on fonts says how to install a font: search the distribution's packages for the font's name, or download it from its upstream page into a fonts folder. Each report of a missing font points to that page. (Amended 2026-11-15, after the code review of this amendment: eight places elsewhere in this record still named a distribution's package for a font, and D12's check still accepted a `package=` line. They now say what D7 decides: a missing font is named by its upstream name, its version and its upstream page, and every source carries `url=`. Only the vtium-fonts recipes name distribution packages.)

> Note 2026-11-16: the maintainer decided how the two programs treat a missing font. `vtium check-config` reports a missing font as a warning. vtium-config warns about a draft whose only problem is a missing font, and still installs the draft. check-config signals that case with an exit status of its own, 6, and with 1 when any other problem stands beside the missing fonts. vtium-config decides by that status, so it never reads check-config's words.

### D8: how each font reaches the machine

vtium ships every catalog set and layer as a text file. A font reaches the machine in one of three ways:

- **Installed by vtium.** Every vtium build installs the Terminus, Unifont and Cullet Mono files (D2).
- **Brought by vtium-fonts.** vtium-fonts is an optional package of its own, apart from vtium. Once it is installed, every catalog set works. It provides each font in the way that suits that font. Each recipe of vtium-fonts depends on its distribution's package that provides a font, where that distribution packages it. Where it does not, the recipe carries a pinned copy of the upstream file with its license text, when the license allows redistribution. So a font may be brought when the common distribution families package it, or when a recipe may carry it under its license.
- **Required.** A font that neither vtium nor vtium-fonts provides is one the person installs. The set names it with D7's keys, and vtium-config names the font and its upstream page.

vtium-fonts carries no Nerd Fonts file. Every Nerd Fonts build, Symbols Nerd Font Mono included, holds the Font Logos icons, which Nerd Fonts' `license-audit.md` lists as "Unlicensed". So a recipe names its distribution's Nerd Fonts packages as optional dependencies, and carries none of them. Noto Sans CJK, 299 MiB on Arch, is an optional dependency too, because only the four CJK sets use it. A set works without such a font, degraded: its text draws from the next source it names, its Nerd Fonts icons as hollow boxes, and its Han, kana and Hangul from Unifont. vtium-config names the font that completes the set and its upstream page.

**A set finds its optional sources.** Every source a set names is optional in this sense: the font helper leaves out a source the machine lacks (ADR-1200 D1), and uses it from the next start once the system has it, with no edit to the set.

The maintainer gave the reason for a package of its own: "notices and attributions clear in an independent fonts package makes a clean break for distribution, and makes all the shipped frits work well".

Each font is named first by its upstream identity. The packaging table follows it, by distribution family, with the names read on 2026-11-14: Arch with `pacman -Si`, Debian and Ubuntu on sources.debian.org, and Fedora and RHEL with Fedora's mdapi for rawhide. Alpine was not checked. "None" means no package was found under the names tried, and the recipe for that family then carries the font or leaves it required, as the rule above says.

| Font, upstream | License | Arch | Debian, Ubuntu | Fedora, RHEL | Where no package exists |
|---|---|---|---|---|---|
| Terminus 4.49.1, `ter-u*.bdf`, terminus-font-4.49.1.tar.gz | OFL-1.1, Reserved Font Name "Terminus Font" | terminus-font, 3.0 MiB | console-setup, xfonts-terminus | terminus-fonts, terminus-fonts-console | vtium installs its own files |
| Cullet Mono, built by vtium from Terminus (TTF) 4.49.3 | OFL-1.1, and the icon sets' own licenses | none | none | none | vtium installs it |
| kbd 2.10.0's console fonts: `default8x16`, `sun12x22`, `cp865-8x16`, `cp865-8x8` | GPL-2.0-or-later, with each file's own notice | kbd, 3.3 MiB | kbd | kbd | the recipe may carry them under the GPL, with its text |
| The Linux kernel's own fonts | GPL-2.0 | none needed: `linux-console` reads the running kernel's font | none needed | none needed | none needed |
| Px437 IBM VGA 9×16, `Px437_IBM_VGA_9x16.ttf`, Ultimate Oldschool PC Font Pack 2.2 | CC-BY-SA-4.0 | none | not checked | not checked | carried, 26,264 bytes |
| Glass TTY VT220, `Glass_TTY_VT220.ttf`, svofski/glasstty | Unlicense | none | not checked | not checked | carried, 88,404 bytes |
| JetBrains Mono 2.304 | OFL-1.1 | ttf-jetbrains-mono, 7.4 MiB | fonts-jetbrains-mono | jetbrains-mono-fonts | carried |
| Fira Code | OFL-1.1 | ttf-fira-code, 1.7 MiB | fonts-firacode | fira-code-fonts | carried |
| Cascadia Code | OFL-1.1 | ttf-cascadia-code, 14.2 MiB | fonts-cascadia-code | cascadia-code-fonts | carried |
| Hack | MIT and Bitstream-Vera | ttf-hack, 1.2 MiB | fonts-hack | none found | carried |
| IBM Plex Mono | OFL-1.1 | ttf-ibm-plex, every Plex family, 99.4 MiB | fonts-ibm-plex | ibm-plex-mono-fonts | carried |
| Source Code Pro | OFL-1.1 | adobe-source-code-pro-fonts, 1.9 MiB | none found | adobe-source-code-pro-fonts | carried |
| Monoid | MIT or OFL-1.1 | ttf-monoid, 0.6 MiB | fonts-monoid | none found | carried |
| DejaVu Sans Mono 2.37 | Bitstream-Vera | ttf-dejavu, 9.8 MiB | fonts-dejavu | dejavu-sans-mono-fonts | carried |
| Nerd Fonts 3.4.0's Mono builds of JetBrains Mono, Fira Code, Hack, Monoid, CaskaydiaCove, SauceCodePro and Terminess | the base font's license and MIT for the patcher; CaskaydiaCove, SauceCodePro and Terminess carry new names because their base fonts reserve theirs; the icon sets under their own licenses, CC-BY-4.0 for Font Awesome and Codicons, and none for Font Logos | ttf-*-nerd, seven packages, 569.3 MiB together; optional | none found | none found | required, with `url=`, because of Font Logos |
| Symbols Nerd Font Mono 3.4.0, `SymbolsNerdFontMono-Regular.ttf` | MIT, with the icon sets' licenses as above | ttf-nerd-fonts-symbols-mono, 2.5 MiB | fonts-nerd-symbols, in forky and sid | none found | required, with `url=`, because of Font Logos |
| Noto Sans Mono and the Noto script fonts | OFL-1.1 | noto-fonts, 106.8 MiB | fonts-noto | google-noto-sans-mono-fonts, google-noto-sans-arabic-fonts and one package for each script | carried |
| Noto Sans CJK | OFL-1.1 | noto-fonts-cjk, 298.8 MiB; optional | fonts-noto-cjk | google-noto-sans-cjk-fonts | carried |
| Noto Color Emoji | OFL-1.1 | noto-fonts-emoji, 10.2 MiB | fonts-noto-color-emoji | google-noto-color-emoji-fonts | carried |
| GNU Unifont 17.0.03, `unifont_all-17.0.03.hex` | OFL-1.1, the option vtium takes of its two licenses | none | unifont | unifont-fonts | every vtium build installs its own file |
| Unscii-8 2.1, `unscii-8.hex` | public domain | none | not checked | not checked | carried, 73,409 bytes as hex, converted to a form the font helper reads |
| Tiny5 Mono 3.000, `Tiny5Mono.bdf` | OFL-1.1 | none | not checked | not checked | carried, 571,375 bytes |

On a distribution that packages every font the table lists, vtium-fonts carries 759,452 bytes of fonts, and their license texts beside them. On Arch, vtium-fonts and its required dependencies install about 262 MiB. Its optional dependencies add 569 MiB for the Nerd Fonts builds and 299 MiB for Noto Sans CJK, about 1,130 MiB in all.

A recipe's metadata then holds:

- `depends=`, the distribution's packages that provide the fonts in the table, by that distribution's names, apart from the optional ones;
- optional dependencies, the distribution's Nerd Fonts packages and its Noto Sans CJK package;
- `source=`, the fonts the recipe carries, each pinned by its upstream URL and SHA-256;
- `license=`, the licenses of what the package itself carries: MIT for the font sets, and OFL-1.1, CC-BY-SA-4.0, Unlicense and the public domain for the carried fonts. The licenses of the fonts it depends on stay with their own packages, and the notices file names them.

One font set file serves every distribution, because it names each font by its upstream name, version and page, and names no distribution's package (D7). Only the recipes name distribution packages.

**The notices.** vtium-fonts carries one notices file. For each font that a catalog set names, it gives the font's source, its version, its copyright line, its license, and its Reserved Font Name or the attribution its license asks for, such as Px437's and the CC-BY-4.0 icon sets' in the Nerd Fonts builds. It says that vtium-fonts carries no Nerd Fonts file because of Font Logos. Beside it, vtium-fonts carries the full text of each license. vtium itself carries only the notices of its own fonts: the built-in font's Unscii-8 and Terminus lines, the OFL text, and the notices of the Terminus and Unifont files it installs. vtium names vtium-fonts as an optional dependency.

**The notices of the files every build installs.** vtium installs the eighteen Terminus files and the Unifont file as upstream wrote them, so each keeps its upstream name. The Terminus files are gzipped. The OFL lets a font be bundled with software when each copy carries the font's copyright notice and the license. Each Terminus file states its own in its `COPYRIGHT` and `NOTICE` properties: "Copyright (C) 2020 Dimitar Toshkov Zhekov", under the OFL-1.1, with the Reserved Font Name "Terminus Font". A `.hex` file holds glyphs alone, so Unifont's notice travels beside it. Unifont's copyright line is the one that Unifoundry's BDF build states: "Copyright (C) 1998-2025 Roman Czyborra, Paul Hardy, Qianqian Fang, Andrew Miller, Johnnie Weaver, David Corbett, Ælla Chiana Moskopp, Rebecca Bettencourt, Minseo Lee, Ho-Seok Ee, et al." vtium takes Unifont under the OFL-1.1, and Unifont reserves no font name. vtium's COPYING names each installed file with its copyright line, its license and its Reserved Font Name. The package carries the OFL text beside COPYING, as it does for the built-in font.

**Font Logos.** vtium draws the Font Logos icons, U+F300 to U+F385, from a Nerd Fonts file that the system installed, and from no file that vtium ships. Drawing a glyph from a font the person installed redistributes nothing. Cullet Mono, which vtium ships, leaves Font Logos out. So a prompt that shows its system's logo, such as Starship's operating-system module with Arch Linux's logo at U+F303, draws a hollow box until the person installs the distribution's Symbols Nerd Font Mono or a Nerd Fonts build, and the logo from then on. The docs say this plainly: to get the logos and the full Nerd Fonts coverage, install the distribution's Nerd Fonts package, and vtium uses it.

> Note 2026-11-18: ADR-1205, accepted on this date, changes this decision. The three ways above say how vtium's packages provide fonts. From the person's side, a font reaches the machine as a system font package, as the person's own copy made as root, ideally in `/usr/local/share/fonts`, or through vtium-config's step that installs a user's font as a system font (ADR-1205 D5). A font then draws only once it is active, and it can be activated only from a root-owned folder on a filesystem that stays mounted (ADR-1205 D4). On a fresh install, the built-in font, Cullet Mono, Terminus, Unifont and the console font that vconsole.conf's `FONT=` names are active, so the default set `terminus` draws with no setup. Since 2026-11-27, the fonts that vtium-fonts brings are active from its install, through the list of active fonts that the package installs (ADR-1205 D4).

### D9: names, folders and the package lists

A catalog set is named after its font, in lowercase words joined by `-`: `fira-code`, `sun-gallant`, `noto-cjk-jp`. A Nerd Fonts set takes the build's own name where Nerd Fonts gives one, as `caskaydia-cove` and `sauce-code-pro`, and the base name with `-nerd` otherwise. A smallest-build set is named after its built-in font and what it adds: `unifont-icons`, `unscii-icons`. A layer is named after what it adds: `nerd-symbols`, `noto-emoji`, `arabic`. Names stay unique across every folder.

In vtium's tree the sets live in `crates/vtium-config/frits/CATEGORY/NAME.vtfrit`, and the layers in `crates/vtium-config/frits/layers/NAME.vtlayer`. They install into the same folders under `/etc/vtium/fonts/`. vtium finds a name in the frits' directory itself first, where a person's own sets live, and then in each category folder. vtium-config refuses to install a set whose name another folder already holds. `terminus` moves into `console/` from the recipe repository, and it names Terminus by its upstream name, version and page.

The full package installs `terminus` with its Terminus and Unifont files and `cullet-mono` with Cullet Mono's faces. The full package also depends on the distribution's package that provides Noto Color Emoji where one exists, and installs `jetbrains-mono`, `unifont-icons`, `unscii-icons` and the layers they use as text files. The minspec builds install `terminus` with its Terminus and Unifont files, `cullet-mono` with Cullet Mono's faces, and `unifont-icons`, `unscii-icons` and their layers as text files. The vtium-fonts package installs the whole catalog, and it depends on every brought font. The Makefile's lists name the sets by category, as ADR-1500 D8 lets a recipe name its sets.

> Note 2026-11-18: every console package, the minspec builds included, installs the same sets and layers: `terminus`, `cullet-mono`, `jetbrains-mono`, `unifont-icons`, `unscii-icons` and the five layers they add. The minspec builds gain `jetbrains-mono`, a small text file whose fonts the distribution provides. The vtium-fonts package installs the rest of the catalog. pacman refuses to install two packages that own the same file, so a vtium-fonts package that held the whole catalog could not be installed beside any console package.

### D10: the font set made from `font-name` holds the one font it names

The font set that vtium makes on the fly from `font-name`, a kbd name, a font path or vconsole's `FONT=` holds that one source (ADR-1200 D9). Every font set holds exactly the sources that its file and its layers name, with the built-in font at the end (ADR-1200 D7). So the file always says where each glyph comes from, and the console draws the same on every machine that has those fonts.

A person who wants color emoji and icons with a font picks the catalog set of that font, or composes one with `new-frit --layer` (D5).

### D11: test sets keep their fonts

A test set judges vtium's drawing in tier 2, tier 3, the specimens, the acceptance pass and the VT guides. Test sets stay where they are, with the fonts they name. ADR-1503 D9 chooses the fonts of tier 2 and tier 3, each a general open font pinned by its URL and SHA-256. A test set combines whatever fonts its check needs, and it names only fonts whose license lets vtium's tools fetch and use them. The tier 3 showcase sets combine many scripts with symbols and emoji, because their pictures show everything vtium draws at once. The specimens draw the catalog sets they name from the catalog's folders.

### D12: a check holds the catalog to this record

A test in vtium-config reads every catalog set and layer, and the notices file of vtium-fonts. The notices half checks that every font vtium-fonts carries has an entry, and that no entry names a font the package lacks. Its text half runs in every build. It checks the category folder, that the name is unique, the order of the roles, that every symbols and emoji layer carries `cover=` lines or an emoji font, that every source carries `url=`, and that the `license=` line names every source's license. Its font half runs with the pinned fonts that tier 2 and the specimens fetch. It asks the helper's engine which source supplies each character, and it checks that each layer supplies characters of its role alone, and some of them. The catalog's font table moves from this record into a TSV beside it, which the test reads.

### D13: how the existing sets map onto the catalog

| Set now | Becomes | What changes |
|---|---|---|
| `desktop` | `code/jetbrains-mono` | Renamed. Its Nerd Fonts and emoji sources become the two layers, and its last line, `source=builtin:unifont`, goes, because the built-in font ends every set (ADR-1200 D7). It gives its icon sets' attribution itself. Its font, JetBrains Mono, is required (D8). |
| `proof` | `code/jetbrains-mono` | Folded in. It names the same fonts, and the tests that read it read `jetbrains-mono`. |
| `terminus`, in the recipe repository | `console/terminus` | Moves into vtium's tree, names bitmap Terminus's nine strikes and the `unifont` layer, and drops its last line, `source=builtin:unifont`. It is the set that works out of the box (D2). |
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

1. **Layers.** The `strike=` key, by which one source draws bitmap strikes at their heights and its outline between them. The frit reader takes `layer=` and reads `.vtlayer` files with `role=`. The console puts each layer's sources in place and checks each layer's ownership. The cell rule for a missing text font. `new-frit --layer` and the TUI's choice. ADR-1200 D1, D9 and D17 gain dated notes.
2. **Font requirements.** The keys `version=`, `check=`, `sha256=` and `url=`. The helper's match against them, the console's log line, vtium-config's report, and the help page on installing fonts that each report points to.
3. **The catalog.** `docs/concepts/fonts.md` and `docs/fonts-for-scripts.md` say plainly that installing a Nerd Fonts build gives the Font Logos icons and full Nerd Fonts coverage. The kernel-font sets, each checked glyph by glyph against the kernel's font it names. The category folders and layers, every member of D3, the renames, the fold of `proof`, the Makefile's lists, and the docs. Specimens that move go in a golden commit of their own.
4. **The catalog check** of D12, and the TSV beside this record.
5. **The built-in font.** The list file, the build step that cuts the three strikes, the hollow box for every other character, the English message for a translation beyond the list, and the acceptance check of D1, as a unit test and a tier 3 case.

   > Note 2026-11-15: this card lands as cards 5a and 5b, as D1's note of the same date says.

6. **The default set in two tiers.** The full package installs the Terminus and Unifont files in `/usr/share/vtium/fonts/` and depends on the icon and emoji packages; the minspec builds install the Terminus files alone. `terminus` becomes the font with none named, and the tier 3 case of D2 runs with each install tree on a guest with no other font package.
7. **Cullet Mono.** The build step that patches Terminus (TTF) 4.49.3 with Nerd Fonts' patcher at a pinned release and every icon set apart from Font Logos, reproducibly from pinned sources; and its notices.
8. **The recipes.** vtium-arch's vtium-fonts installs the catalog, depends on the distribution's font packages, carries the four pinned fonts with their license texts and the notices file, and carries no Nerd Fonts file. The vtium recipe drops its own `terminus.vtfrit`, installs the Terminus and Unifont files, and names vtium-fonts as optional.

> Note 2026-11-17: D2's amendment of the same date pulls card 6 and the parts of card 8 that install fonts forward, into the wave that builds the catalog of card 3. The maintainer's design settles card 6's question: `terminus` is the font set that vtium draws when no font is named. The cards of that wave build:
>
> - the `hex:` source and the `unifont` layer's path (D5);
> - `strike=` under a bitmap source, the choice of strike for a cell, and the cell that the strikes give (D2);
> - `terminus` with its strikes and `cullet-mono`, its two sets for text;
> - every build's install of the Terminus, Unifont and Cullet Mono files, through `make install` and every vtium recipe of vtium-arch, the minspec recipes included, which drop their own `terminus.vtfrit`;
> - `terminus` as the set with no font named, and D2's tier 3 case;
> - the code page 437 characters of the built-in font's list, and vtium's own file of four 8×8 glyphs, which the build reads beside `unscii-8.hex` (D1);
> - the `unifont` layer's two `cover=` lines (D5).
>
> Card 8 keeps the vtium-fonts recipe and the vtium recipe's optional dependence on vtium-fonts. Card 5b, which removes the compiled-in Unifont and Unscii-8, stays a card of its own.

### D15: the maintainer's choices

The maintainer decided every choice this record put to them on 2026-11-14, each as the record leaned (`considered`). The decisions above follow those answers:

- the six categories and the full starting list of members (D3);
- `layer=` lines and `.vtlayer` files (D5);
- a missing required font draws what is there, logs one line, and vtium-config names the font and its page (D7);
- a font that differs from its pin draws and is reported (D7);
- the renames happen now, without aliases (D13);
- the built-in 8×16 strike is cut from Terminus (D1);
- vtium-fonts provides each font in the way that suits it (D8), Unifont ships in the full package (D2), Font Logos come from a Nerd font the system installed (D8), and vtium's own outline Terminus is named Cullet Mono (D2).

> Note 2026-11-17: Unifont and bitmap Terminus's strikes from 12 to 32 pixels now ship in every build, and Cullet Mono has a set of its own. D2's note of that date says so.

## Consequences

### Positive

- The minspec console shrinks by about 1.35 MB, and it still draws its own messages with no font file at all.
- A fresh install of every build draws text in bitmap Terminus from 12 to 32 pixels, box drawing, and every script that Unifont holds, with no outline rendering. In the full package, `cullet-mono` adds Nerd Fonts icons and color emoji at any size.
- Every catalog set draws text, box drawing, icons and color emoji, so a person who picks a font gets a working console.
- Each source in a set has a named role, and a check holds the catalog to it, so each added font has a stated reason.
- A person composes a set from the same layers the catalog uses, with one command.
- A missing font is named, with its version and its upstream page.
- The vtium-fonts package has a reason for each dependency.

### Negative

- With no font helper or font files, a CJK or emoji character draws as a hollow box.
- `terminus`, the default, draws emoji in 1 bit and no Nerd Fonts icons. A person who wants them chooses `cullet-mono`.
- Every build installs about 13.9 MB of font files, which most minspec machines have the storage for.
- Until card 5b of D14 lands, every build carries Unifont twice: compiled into the console and as the installed file.
- vtium.conf files that name a renamed set need the new name.
- The frit format gains a file kind and five keys, which vtium-config's template, its check and its TUI learn.
- A required font that the person has not installed leaves the set's text in the built-in font.
- The catalog check's font half needs the pinned fonts, so it runs where tier 2's and the specimens' fonts are fetched.

### Neutral

- A person's own font sets keep working as written, with or without layers.
- The console binary's size does not change with the installed files. Card 5b still removes the compiled-in Unifont and keeps the built-in font, so the minspec console shrinks as D1 says.
- The test sets keep their fonts, and ADR-1503 D9 keeps choosing them.
- ADR-1200 D9's sentence that waits on "a specification [that] decides how font sets are organized" gains a dated note that names this record.

## Alternatives Considered

- **One font for every set, with the built-in font drawing the rest.** It is the simplest rule. Every set would draw emoji and icons in 1 bit, which falls short of a ready-to-use console. It was rejected.
- **A coverage-first set,** which gathers installed fonts until most characters draw. It is the collage the maintainer named, and its look changes with each machine's fonts. It was rejected.
- **An implicit emoji source for the font set made from `font-name`,** through the key `font-emoji`. The maintainer declined it (ADR-1200 D9). D10 gives the person the same result through a set they choose.
- **A flat catalog in one folder.** It keeps the lookup simple. The maintainer asked for a structure by category, and thirty sets read better in six folders. It was rejected.
- **Each catalog set written out in full, without layers.** Each file would stand alone. The same symbols and emoji lines would repeat in thirty files, and a person could not reuse them. It was rejected.
- **Unifont's Basic Multilingual Plane alone, or Unifont compressed.** Either keeps the minspec install smaller. The maintainer chose the whole font on 2026-11-17: "there's no reason to compress unifont super small or cut it. most minspec devices have sufficient storage to hold the entire unifont".
- **One default set that draws the bitmap strikes up to 16 pixels and Cullet Mono above,** the first form of D2. One set then drew crisp small cells and scaled to large ones. That set needed outline rendering at every size above 16 pixels, and the minspec builds installed the 16-pixel strike alone. On 2026-11-17 the maintainer chose a bitmap set and an outline set, and the bitmap set as the default "for minspec performance reason".
- **Keeping Unifont compiled in.** The built-in font would draw every script with no file. It costs 1.34 MB of the minspec binary, and the `unifont` layer gives the same coverage as a file. It was rejected.
- **The Linux kernel's console fonts as the built-in font.** They have the console's look. They carry GPL-2.0, and compiling them into vtium's MIT binary would make it a GPL combination. A set may still name kbd's copies as required fonts, which redistributes nothing. It was rejected for the built-in font.
- **Spleen,** under BSD-2-Clause, has 5×8 to 32×64 sizes. It is a clean choice too. Terminus already is the catalog's console font, so the built-in font and `terminus` share one look. It was rejected.
- **Bundling every font in a vtium package.** Every set would work at once. It would carry font files that distributions already package and update, and it reverses ADR-1500 D8. It was rejected, and D15's third choice asks about the narrow case.
