---
contract: adr/v1
kind: decision
verb: constrain
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
  - evidence: "Read on integration at d682f48c, 2026-11-14: every frit in the tree. crates/vtium-config/frits holds minspec-16, minspec-8, proof and tiny5. tools/specimens/frits holds desktop and the nine sets for scripts (arabic, hebrew, indic, thai-lao, vietnamese, cjk-sc, cjk-tc, cjk-jp, cjk-kr). tools/tier3/guest/frits holds thirteen test sets, tools/vt/sections/frits six, and tools/tier2/src/fonts.rs builds tier 2's test set in code. The Makefile's FRITS (desktop minspec-16 minspec-8) and FONTS_FRITS (the nine) say which a package installs. crates/vtium/src/font.rs, `Wanted::source`, makes the frit for a family that font-name names with the one source font:<family>."
  - evidence: "vtium-arch's recipes, read through the GitHub API on 2026-11-14: vtium/PKGBUILD installs FRITS and the recipe repository's own terminus.vtfrit (source=font:ter-v16n, then source=builtin:unifont), and lists vtium-fonts as an optional dependency. vtium-fonts/PKGBUILD stages FONTS_FRITS with make install-fonts and depends on noto-fonts, noto-fonts-cjk, noto-fonts-emoji, terminus-font, ttf-jetbrains-mono and ttf-nerd-fonts-symbols-mono, each a package of Arch's own repositories."
  - evidence: "Package indexes read 2026-11-14. Debian's sources.debian.org lists the source packages fonts-jetbrains-mono, fonts-noto, fonts-noto-cjk, fonts-noto-color-emoji, console-setup and xfonts-terminus, and fonts-nerd-symbols 3.4.0+dfsg2-1 in forky and sid only. Fedora's mdapi for rawhide lists jetbrains-mono-fonts, google-noto-sans-mono-fonts, google-noto-sans-arabic-fonts, google-noto-sans-cjk-fonts, google-noto-color-emoji-fonts and terminus-fonts-console, and no package under symbols-nerd-fonts, nerd-fonts-symbols-only or fonts-nerd-symbols. The tiny5 frit's own comment says no Arch or AUR package installs Tiny5 Mono."
  - standard: "UTS #51, Unicode Emoji, version 18.0: the Emoji property and the emoji components (ZWJ, U+FE0F, the skin tone modifiers, the tag characters and U+20E3) in emoji-data.txt"
  - standard: "UAX #24, Unicode Script Property, Unicode 18.0: Script and Script_Extensions, with Common and Inherited for characters that several scripts share"
  - standard: "UAX #11, East Asian Width, Unicode 18.0: the East_Asian_Width values W and F"
  - precedent: ADR-1200
  - precedent: ADR-1500
  - precedent: ADR-1503
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "Every frit that a package installs from vtium's tree passes the shipped-set check: one text family first, then its script, symbols and emoji sources in that order, each source supplying only characters of its role, and each font listed in the catalog"
  - "The frit that vtium makes for font-name=DejaVu Sans Mono holds that one source, and an emoji in it draws from the built-in unifont"
  - "A frit that vtium-config writes with the everyday option names the person's family, then the catalog's symbols source with its cover= lines, then the catalog's emoji source, and nothing else"
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related:
  - ADR-1200
  - ADR-1302
  - ADR-1500
  - ADR-1503
---

# ADR-1204: A shipped font set combines fonts only for a named role, in a fixed order, from fonts that are open and packaged

## Summary

- **Decided:** A font set that vtium ships has one text font. It adds another font only for a named role: a script that the text font lacks, symbols in the Private Use Area, or color emoji. Each added font supplies only the characters of its role. The order is always the text font, then scripts, then symbols, then emoji, and then the built-in font, which no shipped set writes. Every font that a shipped set names comes from a catalog that this record keeps. A font enters the catalog when its license is open, when Arch and Debian or Fedora package it, and when it adds coverage for its role. vtium never adds a font that a person did not write, so the font set made from `font-name` stays one font. A person who wants color emoji with their own font asks vtium-config to write the symbols and emoji sources into a font set, where the person can see them. Test font sets, which judge vtium's drawing, are outside these rules apart from the license.
- **Trades away:** Coverage. A shipped set draws a character that its roles do not reach from the built-in unifont, in 1 bit, even where an installed font could have drawn it. A font set for a script carries no color emoji. Color emoji with a font that `font-name` names takes one more step.
- **One-way?** No. Font sets are text files, and this record changes nothing that a stored format holds. Renaming a shipped set would break each vtium.conf that names it, so the record keeps every name.
- **Probes:** *Confident (no hidden fonts):* vtium never adds a font that a person did not write, and the font set made from `font-name` stays that one font for good. *Not confident (scripts and emoji):* the font sets for scripts stay a text font and the script's fonts, with no symbols or emoji. Should they also take the desktop set's symbols and emoji?
- **Inversion:** The choice sits between two ends. At one end every set is one font, and the built-in font fills each gap. At the other a set gathers whatever installed fonts cover the most characters, which is the collage the maintainer named. vtium combines fonts, but only for a role that the set's kind has and only from the catalog.

## Context

A font set, or frit, is a file that names fonts in order (ADR-1200 D1). vtium draws each character from the first font in the list that has it. A character that no font in the list has falls back to vtium's built-in font for the cell: unifont from 16 pixels high, and Unscii-8 below that (ADR-1200 D7).

vtium ships several font sets that combine fonts, and no rule says how they are organized. The vtium package installs `desktop`, `minspec-16` and `minspec-8` from vtium's tree, and `terminus` from its recipe repository. The vtium-fonts package installs nine sets for scripts, and it depends on the fonts that every shipped set names. vtium ships no font itself (ADR-1500 D8).

The work on text records found that the font set made from `font-name=DejaVu Sans Mono` draws every emoji from the built-in unifont, in 1 bit. It proposed a key, `font-emoji`, that would add Noto Color Emoji to that set. The maintainer declined it, because fonts should not be combined for coverage without a clear specification. ADR-1200 D9 records that answer, and this record is that specification.

The maintainer finds the fonts of the goldens and specimens fine, because they represent general fonts that anyone can use. The concern is collage: fonts added one by one wherever a gap shows, with no rule that says which font may fill which gap.

## Decision

### D1: the kinds of font set

Every font set in vtium's tree and its recipes is one of five kinds. Its first comment line says which kind it is and what it is for.

| Kind | What it is for | Who installs it | The sets now |
|---|---|---|---|
| Everyday | A console's text on a display of ordinary size: one text font, with the symbols and emoji that prompts and file listers draw. | The vtium package. | `desktop` (outline), `terminus` (bitmap) |
| Script | One writing system that the everyday sets lack, often with line spacing for its marks (ADR-1302 D13). | The vtium-fonts package. | `arabic`, `hebrew`, `indic`, `thai-lao`, `vietnamese`, `cjk-sc`, `cjk-tc`, `cjk-jp`, `cjk-kr` |
| Smallest build | The minspec build on a small panel. The built-in font is its text font, and the set adds a chosen list of symbols and emoji. | The vtium package and the minspec builds. | `minspec-16`, `minspec-8` |
| Example | One font that shows something vtium can do. A person installs it by hand. | No package. | `tiny5` |
| Test | Judging vtium's drawing in tier 2, tier 3, the specimens, the acceptance pass and the VT guides. | No package. It names pinned files and is never installed. | tier 2's set, the sets in `tools/tier3/guest/frits/` and `tools/vt/sections/frits/` |

The rules of D2 to D6 bind the first four kinds, which this record calls the shipped sets. A test set may combine whatever fonts its check needs. ADR-1503 D9 chooses the fonts of tier 2 and tier 3, each a general open font that anyone can use, and pins each one by its URL and SHA-256. A test set follows only D5's license rule.

These rules do not bind a font set that a person writes. vtium draws any font set that reads (ADR-1200 D1).

### D2: each added font has a role

A shipped set has exactly one text font, its first source. The text font sets the character cell (ADR-1200 D17). Its own bold and italic faces may follow it with `style=` lines, and they count as the text font.

Each source after the text font has one role, and the set's kind says which roles it may hold:

| Role | What the source brings | The characters it may supply | Kinds that hold it |
|---|---|---|---|
| Script | A writing system that the text font lacks. | Characters whose Script_Extensions name the script, and combining marks (Script=Inherited). A source for Han, kana and Hangul may also supply wide and fullwidth characters (East_Asian_Width W or F). | Script |
| Symbols | The icons that prompts and file listers draw. | The ranges that the symbol font's project documents as its own, written as `cover=` lines. | Everyday, smallest build |
| Emoji | Color emoji. | Characters with Unicode's Emoji property, and the emoji components. | Everyday, smallest build |

A source supplies a character only where every source before it lacks that character (ADR-1200 D1). So an added source can draw characters beyond its role wherever the text font has a gap. The rule above forbids that: an added source fills no gap outside its role. Where a font holds more than its role, its `cover=` lines limit it to the role.

A set holds at most one symbols source and at most one emoji source. It holds as many script sources as its scripts need, one font for each script. It holds at most one face for Han, because each face draws one region's forms (`cjk-sc`, `cjk-tc`, `cjk-jp`, `cjk-kr`).

No source is added to fill a gap in the text font's own scripts. For example, a second Latin font may not supply the Latin letters that the text font lacks. The built-in font draws those.

An everyday set takes no script source, and a script set takes no symbols or emoji source. Each set does one job, so each set is judged against that job alone.

### D3: the order and the choice of fonts

The sources come in this order:

1. the text font, with its own style faces;
2. the script sources;
3. the symbols source;
4. the emoji source;
5. the built-in font, which comes last without being written.

Text fonts and script fonts come first, so that each character a text font holds draws as text. That is the default for a character whose Emoji_Presentation is No. The emoji font comes last, and it still draws every character that asks for emoji presentation. The helper takes such a character's color glyph from wherever the color source stands in the list (ADR-1200 D21). The symbols source stands before the emoji source. Its `cover=` lines keep it to its own ranges, so the two never compete for a character.

A shipped set does not write `source=builtin:` last, because ADR-1200 D7 already puts the built-in font at the end of every set. A set writes `builtin:` only to put the built-in font before another source.

The smallest-build sets have no text source. The built-in font is their text font, and vtium consults it after the set's sources. So every source of such a set carries `cover=` lines, and the built-in font draws everything else.

The fonts are chosen by these rules:

- **The text font** is monospaced, holds every printable ASCII character, and is the Regular face of its family. Bold, italic and bold italic faces come only from the same family.
- **A script font** comes from the text font's own superfamily where that superfamily has a face for the script. Noto Sans Mono and Noto Sans Arabic share one design, for example. Where the text font's family has no such face, as JetBrains Mono has none, the script set takes Noto Sans Mono as its text font instead. Its text and its script then share one design. Noto is the superfamily for scripts, because it has a face for nearly every script under one license, and the common distributions package it. A script font may be proportional, because vtium fits each glyph to its cells (ADR-1302 D2).
- **Metrics need not match.** vtium draws each later outline source at the text font's size and on the text font's baseline (ADR-1200 D17). Matching the style is the chooser's job: a sans text font takes sans script fonts of a similar weight.
- **The symbols font** is the one-cell-wide build, such as Symbols Nerd Font Mono. Its `cover=` lines name its project's documented ranges, less any range that carries no license grant, such as Font Logos.
- **The emoji font** is a color font that vtium reads.
- **Behind a bitmap text font,** every outline source draws 1-bit by default, so an added font looks like the bitmap font around it (ADR-1200 D1, `render=`).

### D4: vtium adds no font that a person did not write

vtium never adds a source to a font set. That holds for the font set that vtium makes on the fly from `font-name`, a kbd name, a font path or vconsole's `FONT=` (ADR-1200 D9), and for every font set a person writes. The one exception is the built-in font. It is compiled into vtium, it is always readable, and ADR-1200 D7 defines its place at the end of every set.

So the font set made from `font-name` stays one font for good. An emoji that the family lacks draws from the built-in unifont.

A font that vtium added by itself would make the font set's file wrong about where its glyphs come from. `vtium config fonts`, the preview and the file would then disagree. A console would draw differently whenever a package happened to install an emoji font. And the console would collage fonts, which is what the maintainer asked vtium to avoid.

Instead, vtium-config writes the sources when a person asks for them. `vtium config new-frit` gains an option, and its TUI view gains a choice, that writes the catalog's symbols source with its `cover=` lines and the catalog's emoji source after the person's own font. The font set that results is an everyday set, and the person sees every source before installing it. Without the option, `new-frit` writes only the fonts the person names, as it does now.

With no font named, vtium still draws the built-in fonts (ADR-1200 D11). vtium never picks a shipped set by itself.

### D5: what a font must meet to ship

A font may appear in a shipped set, or be a dependency of the vtium-fonts package, only once it is in the catalog below. A font enters the catalog when it meets all three of these tests:

- **License.** Its license is one of OFL-1.1, Apache-2.0, MIT or Bitstream-Vera. CC-BY-4.0 is accepted for an icon set that a symbols font carries, when the set's comment gives the attribution the license asks for. Adding a license to this list is an edit of this record. The set's `license=` line names the SPDX expression of all its sources.
- **Packaging.** Arch's own repositories package it, because vtium-fonts depends on Arch's packages and the AUR is not enough for that. Debian or Fedora also package it, in a release or in its development branch. A font that no distribution packages, such as Tiny5 Mono, may appear only in an example set.
- **Coverage.** It supplies characters of its role that the sources before it lack, in each set that names it. A source that supplies none of its role leaves the set.

The catalog, as read on 2026-11-14:

| Font | Role | License | Arch | Debian | Fedora | Named by |
|---|---|---|---|---|---|---|
| JetBrains Mono | text | OFL-1.1 | ttf-jetbrains-mono | fonts-jetbrains-mono | jetbrains-mono-fonts | `desktop` |
| Noto Sans Mono | text | OFL-1.1 | noto-fonts | fonts-noto | google-noto-sans-mono-fonts | the script sets |
| Terminus (`ter-v16n`) | text, bitmap | OFL-1.1 | terminus-font | console-setup, xfonts-terminus | terminus-fonts-console | `terminus` |
| Noto Sans for each script (Arabic, Hebrew, Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam, Sinhala, Thai, Lao) | script | OFL-1.1 | noto-fonts | fonts-noto | one package for each script; google-noto-sans-arabic-fonts was checked | the script sets |
| Noto Sans CJK, SC, TC, JP and KR faces | script | OFL-1.1 | noto-fonts-cjk | fonts-noto-cjk | google-noto-sans-cjk-fonts | `cjk-sc`, `cjk-tc`, `cjk-jp`, `cjk-kr` |
| Symbols Nerd Font Mono | symbols | MIT; its icon sets under MIT, CC-BY-4.0 and Apache-2.0 | ttf-nerd-fonts-symbols-mono | fonts-nerd-symbols, in forky and sid | none found | `desktop`, `minspec-16`, `minspec-8` |
| Noto Color Emoji | emoji | OFL-1.1 | noto-fonts-emoji | fonts-noto-color-emoji | google-noto-color-emoji-fonts | `desktop`, `minspec-16`, `minspec-8` |

Tiny5 Mono (OFL-1.1) has no Arch or AUR package, so it stays in the example set `tiny5` and in no other shipped set. The test sets' fonts, such as DejaVu Sans Mono and Iosevka Term, are pinned by tier 2, tier 3 and the specimens, and stay outside the catalog.

### D6: names, groups and where the sets live

**Names.** A set whose fonts vtium may change is named for what it is for: `desktop`, `arabic`, `cjk-jp`, `minspec-16`. A set that is one particular font is named for that font: `terminus`, `tiny5`. A name is lowercase words joined by `-`. A new script set is named for its script or its language, as its readers name it, and a new Han set as `cjk-` and its region. Every existing name already follows these rules, so no set is renamed. A rename would break each vtium.conf whose `font-name` names the set.

**Groups.** The package that installs a set is its group. The vtium package installs the everyday and smallest-build sets. The minspec builds install the smallest-build sets. The vtium-fonts package installs the script sets, and it depends on every font that the shipped sets name. The Makefile's `FRITS` and `FONTS_FRITS` lists say which set each package installs, as ADR-1500 D8 decides.

**Where they live.** Every set that a package installs from vtium's tree lives in `crates/vtium-config/frits/`, with the example sets. `desktop` and the nine script sets move there from `tools/specimens/frits/`. The specimens then draw `desktop` from there, as they draw `tiny5` now. Each test set stays beside the tier or the guide that uses it. `terminus` stays in the vtium recipe repository. The recipe says the set names the console font by the file name that Arch's package gives it.

### D7: a check holds the shipped sets to this record

A test in vtium-config reads every set in `crates/vtium-config/frits/` and checks it against this record. The test has two halves.

- **The text half** runs in every build. It checks the kind on the first comment line, the order of the roles, the `cover=` lines on every symbols and emoji source, that each font is in the catalog, and that the `license=` line names only D5's licenses.
- **The font half** needs the fonts. It runs with the pinned fonts that tier 2 and the specimens already fetch. For each set it asks the helper's engine which source supplies each character. It fails when an added source supplies a character outside its role, or supplies none of its role.

The catalog then moves from this record into a TSV beside it, which the test reads, so that the catalog is written in one place.

### D8: how each font set maps onto this record

| Set | Kind | Its sources, in order | Fits, or what changes |
|---|---|---|---|
| `desktop` | everyday, outline | JetBrains Mono; Symbols Nerd Font Mono, covering U+E000–U+F2FF, U+F386–U+F8FF and U+F0001–U+F1AF0; Noto Color Emoji; `builtin:unifont` | Its roles fit. Its last line, `source=builtin:unifont`, goes (D3). It gives the CC-BY-4.0 attribution for its icon sets itself, where now it points to `proof`'s comment. It moves to `crates/vtium-config/frits/`. |
| `terminus` | everyday, bitmap | Terminus `ter-v16n`; `builtin:unifont` | Its last line, `source=builtin:unifont`, goes, in the vtium recipe repository. |
| `minspec-16` | smallest build | the built-in unifont as its text font, unwritten; Symbols Nerd Font Mono, a chosen list of ranges; Noto Color Emoji, a chosen list of emoji | Fits as it is. |
| `minspec-8` | smallest build | the built-in Unscii-8 as its text font, unwritten; then as `minspec-16` | Fits as it is. |
| `arabic`, `hebrew` | script | Noto Sans Mono; Noto Sans Arabic or Noto Sans Hebrew | Fit. They move. |
| `indic` | script | Noto Sans Mono; Noto Sans Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam and Sinhala | Fits. It moves. |
| `thai-lao` | script | Noto Sans Mono; Noto Sans Thai; Noto Sans Lao | Fits. It moves. |
| `vietnamese` | script | Noto Sans Mono alone, in four `cover=` groups for `fit=` | Fits, as a script set of one font. It moves. |
| `cjk-sc`, `cjk-tc`, `cjk-jp`, `cjk-kr` | script | Noto Sans Mono; Noto Sans CJK in the region's face | Fit. They move. |
| `proof` | none: the proof of the outline font stack, installed by no package | JetBrains Mono; Symbols Nerd Font Mono; Noto Color Emoji | It does the job of `desktop` with the same fonts. It folds into `desktop`, and the tests that read it read `desktop`. |
| `tiny5` | example | Tiny5 Mono's BDF file | Fits. Its font is in no distribution, so it stays an example. |
| The test sets | test | pinned files, as each test needs | Fit as test sets. The tier 3 showcase sets combine many scripts with symbols and emoji in one set, because their pictures show everything vtium draws at once. |

Whether the built-in font's glyphs sit in the same place when the built-in font is written last and when it is not written is measured before those lines go. A written `builtin:` source is drawn by the helper, and an unwritten one by the console's fallback (ADR-1200 D7). The specimens of `desktop` are accepted goldens, so any pixel that moves goes to the maintainer in a golden commit of its own.

### D9: what follows for the work that waits on this record

- **The emoji source for the font set made from `font-name`** is refused as an implicit source (D4). It is reshaped as the `new-frit` option and the TUI choice that write the symbols and emoji sources where the person sees them. The ligature work that shares its issue does not wait on this record.
- **The small-panel fonts for wider coverage** are sources of the smallest-build kind. Each must pass D5. A small bitmap font that no distribution packages may appear only in an example set. A smallest-build set adds a font only for the symbols or emoji role. A small font for a script goes into a script set for small panels, which this record allows as a new script set at a stated `cell=`.
- **The script font sets** keep their fonts and names. They move to `crates/vtium-config/frits/` (D6), and each first comment line names its kind. `docs/fonts-for-scripts.md` then names their new place.

### D10: the maintainer's choices

Each choice gives the options, what each trades away, and the record's lean. The decisions above follow the leans.

1. **The emoji source for the font set made from `font-name`.** (a) No implicit source; `new-frit` writes the symbols and emoji sources on request. It costs one step for the person. (b) vtium adds Noto Color Emoji to that set when the font is installed, as the work on text records first proposed. The file then no longer says where every glyph comes from, and the drawing changes with what packages install. (c) A vtium.conf key names an emoji font for every set, as the declined `font-emoji` did. Two places then name a set's sources. (d) No implicit source and no help. The person must know Nerd Fonts' ranges and license notes to write the set by hand. **Lean: (a).**
2. **Symbols and emoji in the script sets.** (a) The script sets stay a text font and the script's fonts. An Arabic or Japanese reader gets no color emoji or prompt icons unless they write a set or use (1a). (b) Each script set also takes `desktop`'s symbols and emoji sources after its scripts. Each set then holds four or more fonts and is judged on two jobs. vtium-fonts already depends on those fonts, so the package does not grow. **Lean: (a),** because each set then does one job and (1a) adds the rest in one command.
3. **The packaging bar.** (a) Arch's own repositories, and Debian or Fedora. (b) Arch alone. A shipped set could then name a font that other distributions lack. (c) Arch, Debian and Fedora all. Symbols Nerd Font Mono fails that bar, since Fedora has no package of it, so `desktop` and the smallest-build sets would lose their symbols. **Lean: (a).**
4. **`proof`.** (a) It folds into `desktop`, and its tests move to `desktop`. (b) It stays as a set for the tests alone, beside them. Two sets then hold the same fonts for one job. **Lean: (a).**
5. **Where the shipped sets live.** (a) All in `crates/vtium-config/frits/`. Paths in the Makefile, the specimens and the docs change once. (b) They stay split as now. The check must then read two directories, and the specimens' directory mixes shipped sets with the sets it draws. **Lean: (a).**
6. **The check.** (a) The test of D7 holds the shipped sets to this record. It costs a test with two halves and a TSV. (b) Review against this record alone. Drift is then caught only by eye. **Lean: (a).**
7. **The built-in font written last.** (a) The shipped sets never write `builtin:` last, as D3 says. If the measurement in D8 shows that the built-in glyphs move, the maintainer sees the pictures before the lines go. (b) The shipped outline sets write `builtin:unifont` last on purpose, if the helper places those glyphs better on the text font's baseline. ADR-1200 D7's sentence that vtium's own sets carry no copy of unifont would then change. **Lean: (a),** pending the measurement.

## Consequences

### Positive

- Each source in a shipped set has a stated role, and a check enforces it, so no font enters a set to fill one gap.
- The font set made from `font-name` is exactly what its name says, on every machine.
- The vtium-fonts package's dependencies follow from the catalog, so its list of fonts has a reason for each entry.
- A new font set for a script, or for a small panel, has a pattern to follow and a bar to pass.

### Negative

- A shipped set draws a character beyond its roles from the built-in unifont, in 1 bit, where an installed font might have drawn it better.
- A script set carries no color emoji or prompt icons.
- The move of eleven sets changes paths in the Makefile, the specimens, the docs and the vtium-fonts package's build.
- The check's font half needs the pinned fonts, so it runs only where tier 2's and the specimens' fonts are fetched.

### Neutral

- A person's own font sets are untouched. vtium draws any font set that reads.
- The test sets keep their fonts, and ADR-1503 D9 keeps choosing them.
- ADR-1200 D9's sentence that vtium adds no font "until a specification decides how font sets are organized" gains a dated note that this record decides it never does.

## Alternatives Considered

- **One font for every set, the built-in font filling each gap.** It is the simplest rule and needs no catalog. It drops color emoji and prompt icons from every shipped set, and it drops every script set, which the maintainer finds useful. It was rejected.
- **A coverage-first set,** which gathers installed fonts until most characters draw. It reaches the widest coverage. It is the collage the maintainer named, and its look changes with each machine's fonts. It was rejected.
- **An implicit emoji source for the font set made from `font-name`,** through the key `font-emoji`. The maintainer declined it (ADR-1200 D9), and D4 gives the reasons.
- **A `role=` key under each source,** which the frit reader would check. It would make each role explicit in the file. It changes the frit format for every person's font set, while the problem lies only in the shipped sets. D7's check reads the role from the catalog instead. It was rejected.
- **A set that includes another set,** so that a script set could reuse `desktop`'s symbols and emoji. It would save repeated lines. It adds a new key and an order between files, and the script sets take no symbols or emoji under D2. It was rejected.
