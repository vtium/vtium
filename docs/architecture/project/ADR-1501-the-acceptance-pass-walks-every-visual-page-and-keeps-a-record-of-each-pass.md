---
contract: adr/v1
kind: decision
verb: add
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "the test harness in our kmscon fork c reference had a series of pages that exercised all manner of terminal capabilities. I'd like ... a script that allows an operator to essentially take our own interview process (looks correct, yes no, comments etc) without needing to type commands but instead run through all the visuals. I think a visual acceptance pass that records this for any given pass is helpful as a 'playtest' so to speak for me, the operator."
    via: "the steering session, 2026-11-03, quoted in this record's brief by the team lead"
  - operator: aaronsb
    level: directed
    said: "vtium is not copying the C reference and aims to exceed it. The C fork's pages carry hard-won knowledge of what's worth checking, so use them, but the C fork's page set is a starting point, not a boundary."
    via: "the steering session, 2026-11-03, relayed by the team lead; the wording is the team lead's"
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "Even though I only read English, it would be helpful to have other language features too, especially formatting on them like lines and italic bold super sub etc. We could present a series of pages essentially stating the same thing. For all the visuals, we use English language to describe what we are looking at."
    via: "the steering session, 2026-11-03, quoted by the team lead"
  - operator: aaronsb
    level: directed
    said: "Run each language page in a common outline font and in a bitmap font: DejaVu Sans Mono with Noto Sans fallbacks, and GNU Unifont with Terminus, named by frits kept beside the pages; each pass record names the frit and each font file's path and version or SHA-256; a page whose font is not installed records skip with the missing file named."
    via: "the steering session, 2026-11-03, relayed by the team lead; the wording is the team lead's"
    paraphrase: true
  - upstream: "aaronsb/kmscon origin/main, fork/demo/: demo.sh walked about thirty section scripts in fork/demo/sections/, one page each with its `# look-for:` and `# wrong-looks-like:` guidance below it, rated with the keys 1, 2 and 3, s, c, r, b and q; each answer appended a line to /tmp/kmscon-demo/feedback.tsv; feedback.sh printed a run and compared the latest run of each label side by side; the pages fell into review kits (text, sixel, kitty, glyphs); docs/fork/demo.md described it all"
  - evidence: "the tree on integration at 6d70669: tools/vt/demo.sh walked tools/vt/sections/ (10-sgr.sh alone) with the C reference's keys and appended each answer to /tmp/vtium-vt/feedback.tsv; tools/vt/checks/guide.sh asks y, n or skip and a note for each step of a guide and appends the answers to /tmp/vtium-vt/vt-check.log; make vt-demo runs demo.sh as the session of a review run through tools/vt/vt.sh and run-vt.sh, which pass the session DEMO_LABEL=\"vtium dev build <git describe>\" and clear the rest of its environment"
  - evidence: "measured 2026-11-03 in the work's worktree: each text page's output, fed to tools/conformance's vtium-replay at 80x25 below demo.sh's two header rows, gives the screen its guidance describes (the charsets pairs match, DECCRA's copy keeps its colors, DECFRA and DECERA fill and empty their rectangles, the scroll region keeps lines 7 to 12 between its markers, OSC 4 turns color 1 to #ffa500); the picture pages, run on a pseudo-terminal that answers as a build with pictures, place three sixel bars and a transparent frame, and four kitty placements in the three layers, and vtium-replay shows the third kitty-layers placement deleted"
  - evidence: "the tree on integration at 6d70669: vtium takes OSC 50 under allow-font-ops, on by default, switches the console to a frit in its frits directory (font-frits) at once, and answers OSC 50's query with the font in use (crates/vtium/src/app.rs, switch_font; crates/vtium-term/src/terminal/reply.rs); the frit format refuses style= other than regular (crates/vtium-config/src/frit.rs), so every font set draws bold by widening the regular glyph a pixel and italic upright (crates/vtium-font/src/set/read.rs, strike_by); joining forms are keyed only in the helper (crates/vtium-config/src/shaping), and the console orders no right-to-left text"
  - evidence: "measured 2026-11-03 in the work's worktree, on integration at 1167462f: each language page, drawn at 80x25 through the console's own face for a frit (crates/vtium/src/deferred.rs, DeferredFace over the store of the helper's glyphs), with the store filled as the console fills it, by one ask of vtium-config's engine for every codepoint and every cluster the screen shows (crates/vtium/src/helper.rs, crates/vtium-config/src/serve.rs), shows every combining mark, dependent vowel sign and virama in both frits: in acceptance-outline the helper shapes every cluster the pages hold (13 on the Vietnamese marks page, 20 on Hindi, 13 on Thai), and in acceptance-bitmap, which has no outline source to shape them, the console draws each cluster as its base with each mark overstruck, so a Devanagari vowel sign that stands beside its letter is drawn over that letter; Hebrew and Arabic come out letter by letter in typed order and the Arabic letters unjoined. An earlier measurement that showed every mark missing drew each cluster by its first character alone, a defect of that measurement and not of vtium"
  - evidence: "the tree on integration at 6d70669: vtium-term keeps SGR 8, 53, 6 and 73 to 75 and DECDWL, DECDHL and DECSWL in its cells and lines, and vtium-render reads no conceal, overline, script or line-size attribute when it draws"
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "The name is the acceptance pass, make vt-accept, which replaces vt-demo; committed passes go under docs/acceptance/."
    via: the maintainer's answers in the steering session's interview, 2026-11-03, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Commit one pass per batch that goes from stage to main. Each pass record names the hardware it ran on and says it is evidence about that hardware only, and the comparison warns when two passes ran on different hardware. The record argues its choices on vtium's own grounds, not on the C reference's."
    via: the maintainer's answers in the steering session's interview, 2026-11-03, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "with an understanding we only have the GPU we ran it on. We also need to leave the c reference behind."
    via: the steering session's interview, 2026-11-03, quoted by the team lead
  - operator: aaronsb
    said: "Pictures are on by default: make vt-accept builds with images unless FEATURES is named, and a minspec pass is make vt-accept FEATURES=."
    via: the maintainer's answers in the steering session's interview, 2026-11-03, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "A language page takes one answer, with the comment naming any style that looked wrong. Keep the UDHR text. Keep the u key for unsure. One pass covers both font sets."
    via: the maintainer's answers in the steering session's interview, 2026-11-03, relayed by the team lead; the wording is the team lead's
    paraphrase: true
status: proposed
date: 2026-11-03
deciders:
  - aaronsb
related: []
---

# ADR-1501: The acceptance pass walks every visual page and keeps a record of each pass

## Summary

- **Decided:** The acceptance pass is one walk, by the operator on a VT, through every page in `tools/vt/sections/`. A page draws one thing vtium supports and says below itself, in plain words, what right and wrong look like. The operator answers each page with one key (y looks right, n looks wrong, u unsure, s skip), may type a comment first, may go back a page, and may quit and resume later; no command is typed between pages. Each pass is a markdown record in `~/.local/state/vtium/acceptance/`, rewritten after every answer, that names the build, the hardware it ran on, the fonts, the VT, the terminal and each page's answer and comment, and says it is evidence about that hardware only. `make vt-accept` runs a pass, with pictures unless `FEATURES` is named, and one pass on the build of each batch that goes from `stage` to main is committed under `docs/acceptance/`. `tools/vt/feedback.sh --compare A B` prints the pages whose answer changed between two passes, warns first when the two ran on different hardware, and exits 1 when a page went from yes to no. Thirteen language pages each show one sentence under every style, and the walk shows each twice, switching the console with OSC 50 to an outline font set and then a bitmap one, and records every font file's SHA-256.
- **Trades away:** `make vt-demo`, its keys 1, 2 and 3 and its answers file in `/tmp`; earlier runs' answers are not read. Pages that need programs on the host (chafa, mpv, tmux), file media in a login, or the GPU path have no page yet. A pass takes the operator most of an hour. A pass is evidence about one machine's GPU and displays only.
- **One-way?** No. The pages are shell scripts and the record is a generated text file; the format can change with its reader in one change, and each of the maintainer's answers below is one setting. Committed records stay as they were written.
- **Probes:** *Confident (keys):* one key per answer, a typed comment, back, and quit-and-resume cover the interview the maintainer asked for, the way the guides' y, n and skip already do. *Not confident (cadence):* one committed pass per batch is often enough to catch a regression before main. The maintainer answered both (`considered`).
- **Inversion:** Between "record every answer in one log and compute passes from it" (a pass is a run id among many, and nothing can be resumed or committed on its own) and "commit every pass" (the repository grows with every look at the VT). The answer keeps one file per pass outside the repository, readable as it stands, and commits only the passes the maintainer chooses.

## Context

The maintainer judges what vtium draws by eye, on a VT of this machine. The tools for that exist (`docs/vt-review.md`): a review run puts a build of the checkout on the development VT, and a guide (`make vt-check GUIDE=NAME`) asks y, n or skip for each step, or the demo (`make vt-demo`) shows the pages in `tools/vt/sections/`. Before this record the demo had one page, the SGR attributes. The C reference's demo had about thirty, one for each kind of thing the C reference drew, and its answers went to a file in `/tmp`, which a reboot clears.

The maintainer asked for a visual acceptance pass, first called a playtest: a script that walks every visual page in order, says on each what correct looks like, takes the same interview answers the guides take with single keys, and records each pass, so that two passes can be compared and a regression shows as a page that went from yes to no.

The C reference's pages are where the acceptance pass starts, not where it stops: vtium aims to do more than the C reference did, and every visible thing vtium does gets a page. The maintainer reads only English and asked for pages in other scripts all the same, each described in English in shapes a person can check without reading the script, and run in an outline font and a bitmap font, because the two take different paths through vtium: a real face or a synthetic style, marks and joining from the font's data or from vtium's own rules, scaling to fit the cell or by whole multiples.

A page is a short script that draws one screen of output. The pass record is the file that holds one pass's answers. A build is the vtium binary under review, named by its `git describe` and the features it was built with.

## Decision

### D1: the walk

The acceptance pass is how the maintainer accepts what vtium draws on real hardware. The tiers judge bytes, screen state and pixels drawn in memory, and a VT pass judges what reaches a display through a real GPU and a real font, which only a person looking can do.


`tools/vt/accept.sh` is the walk. It shows a title page that names the build and lists the pages, then each page in file name order, then a summary. Each page is drawn below a header row that numbers it (`[3/27] color-forms`), and below the page a footer gives the page's guidance, `right:` from its `# look-for:` lines and `wrong:` from its `# wrong-looks-like:` lines. When the footer does not fit below the page, a key shows it on a screen of its own.

One key answers the page and goes to the next: y (yes, it looks right), n (no, it looks wrong), u (unsure: looked at and cannot tell) and s (skip: not judged this time). c opens a comment of as many lines as needed, ended by Ctrl-D or a line holding only a dot, and one of y, n, u or s then answers the page. r draws the page again, b goes back a page, where a new answer replaces the old one, and q quits to the summary. Keys typed before a prompt appears are discarded, so a double press never answers a page unseen. The walk turns on bracketed paste (DECSET 2004) while it runs and drops a paste whole while it waits for an answer, so a pasted y, n, c or q never answers a page; in a comment a paste is comment text, so an address can be pasted into one.

Before each page the walk puts back what the page before it may have changed: the pen, reverse video for the whole screen (DECSCNM), the cursor's shape and visibility, the palette (OSC 104), the scroll region, and kitty pictures. A page that changes any other state puts it back itself.

A page that this build cannot show, such as a pictures page under a build without pictures, says so on the screen in plain words and exits with status 77. The walk records it as skip, with the comment "this build cannot show this page", and any key goes on. Such a page is never recorded as no.

### D2: the pages

A page is one file, `tools/vt/sections/NN-name.sh`. `NN` orders the pages and `name` is what the title page lists and what `accept.sh name` runs alone. A page:

- holds both guidance fields, each in terms a person checks by eye, at most four rows of the footer at 80 columns;
- draws the same output on every run: no clock, no randomness and no dependence on the locale, with invisible code points written as UTF-8 bytes;
- fits 80x25 with the header and the footer: at most 17 rows, none wider than 80 columns;
- places anything by absolute row from the row it starts on, which the walk passes it as `ACCEPT_TOP`, and which `common.inc`'s `top_row` asks of the terminal when the page runs alone;
- asks the terminal what it needs through `tools/vt/sections/common.inc` (DA1, the cell size, a kitty query, whether the image helper decodes), and exits 77 through `cannot_show` when the build cannot show it.

A page that blinks text (SGR 5 or 6) names, below what it draws, the blink style vtium draws it in and what that looks like when it is right: `full` hides the text in the off phase, `partial` dims it halfway to its background and never hides it, and `off` keeps it steady. `tools/vt/blink.inc` reads the style as the user from the config file vtium reads, the last `blink-text=` line and the `blink` switch winning as they do in vtium, then from the blink options on vtium's command line; a review run names its own config, `tools/vt/config/vtium.conf`, and passes its blink options, and a pass run by hand reads `/etc/vtium/vtium.conf`. Without any of them the style is vtium's default, which the page reads when it runs from the checkout's key table (`crates/vtium-settings/src/keys.rs`), so the page and the build agree; a file the user cannot read is said to be one. With partial the page also says that on a 1-bit display vtium draws partial as full, so blinking text hides there.

A change that makes vtium draw something new adds its page.

These are the pages, and the ADR-1000 rows each one shows:

| Page | What it shows | ADR-1000 rows |
|---|---|---|
| 10-sgr | the sixteen colors as text and background, and each SGR attribute as a word | `sgr.bold`, `sgr.dim`, `sgr.italic`, `sgr.underline`, `sgr.blink`, `sgr.inverse`, `sgr.strike`, `sgr.palette` |
| 12-colors-256 | the 6x6x6 color cube, the grey ramp and a 24-bit ramp | `sgr.extended-color` |
| 15-color-forms | SGR 38 and 48 in the semicolon and colon forms, side by side | `sgr.extended-color` |
| 20-underline-styles | SGR 4:1 to 4:5 and 21, ended by 4:0 and 24, over color, bold, reverse and CJK | `sgr.underline-style`, `sgr.double-underline` |
| 25-underline-color | SGR 58 in both forms, with styles, and 59 | `sgr.underline-color` |
| 30-strike | SGR 9 and 29 with color, underline, reverse and CJK | `sgr.strike` |
| 35-blink | SGR 5 beside steady text, and SGR 25 halfway along a line | `sgr.blink`, `sgr.blink.style`, `sgr.blink.decorations` |
| 37-sync | twelve frames of mode 2026, one left open past `sync-timeout`, and what DECRQM answered | `DEC-2026`, `query.decrqm` |
| 40-boxdraw, 45-blocks, 47-mosaic | box drawing, block elements, Braille, sextants and octants, stacked across cells | `text.sprites` |
| 50-cjk | double-width characters beside Latin | `text.grapheme` |
| 55-wide-margin | wide characters placed and typed at the right margin | `text.vs-width` |
| 60-combining | letters built from a base and combining marks beside the precomposed ones | `text.grapheme` |
| 65-charsets | DEC special graphics through G0 and G1, and REP, each beside what it must look like | `text.charsets`, `text.rep` |
| 66-rectangles | DECCRA copying a box with its colors, DECFRA filling one, DECERA emptying one | `ctl.rect` |
| 67-scroll-region | twelve lines printed into a scroll region of six rows | `ctl.decstbm` |
| 70-emoji | a ZWJ family, a flag, a keycap, a skin tone, VS15 and VS16, each between bars | `text.emoji`, `text.vs-width` |
| 75-cursor-shape | DECSCUSR 4, a steady underline cursor | `cursor.style` |
| 77-reverse-screen | DECSCNM, the whole screen in reverse video | `DEC-5` |
| 78-palette-change | OSC 4 changing color 1 after the text in it was written | `color.palette.set`, `sgr.draw-time-color` |
| 80-links | OSC 8 links, underlined only under the pointer, and their address while Ctrl is held | `osc.8` |
| 81-english to 93-emoji-styles | the language pages (D7): one sentence in English, Vietnamese twice (precomposed, and every mark combining), Greek, Russian, Hebrew, Arabic, Hindi, Thai, Chinese, Japanese and Korean, and an emoji line, each under every style, each walked in both font sets (D6) | `sgr.*`, `text.grapheme`, `text.vs-width`, `text.emoji`, `text.outline` (the outline set's glyphs, in every style the page shows, bold and italic among them) |
| 95-sixel | sixel bars beside text cells of the same color, and a frame with a clear middle over words | `image.sixel`, `image.compositor` |
| 96-kitty | kitty pictures sent as raw pixels, one in each layer, and a placement that moves the cursor | `image.kitty`, `image.compositor` |
| 97-kitty-layers | one kitty picture placed over and under text, and a placement deleted | `image.kitty`, `image.overlap` |
| 98-kitty-png | kitty pictures sent as PNG and as zlib, which the image helper decodes | `image.kitty`, `image.helper` |
| 99-iterm2 | iTerm2 pictures, OSC 1337 File=, at two sizes | `image.iterm2`, `image.helper` |

The four picture pages that sixel and kitty's raw pixels serve need a build with pictures, which DA1's attribute 4 and an answer to a kitty query show. The two that the helper serves also need the image helper to decode a PNG, which a kitty query of one PNG pixel shows. Each says which of these it lacks and records skip.

These rows have a visible effect and no page yet. Each page comes with the change that makes it drawable or the change that adds it:

- `sgr.conceal`, `sgr.overline`, `sgr.script` and `text.line-size`: vtium-term keeps them, and vtium-render does not draw them yet.
- `sgr.rapid-blink`: vtium-render blinks SGR 6 at SGR 5's rate, so its page waits for the faster rate that ADR-1000 states.
- `text.ligatures`, `text.font-merge`, `text.bidi` and `text.kitty-sizing`: not built yet.
- `image.kitty-animation`, `image.kitty-placeholders` and `image.kitty-relative`: built; their pages are owed.
- `text.reflow` and `DEC-2028`: built; a page needs the grid to change size while it shows, which a font switch does, and is owed.
- The C reference's pages that need programs on the host (photos, photo-scroll, mpv-sixel and the tmux grids need chafa, mpv and tmux and the C reference's photograph), its sixel ramps, article and eviction pages, its kitty-files page, which needs a logind session of the program's own that a review run's session lacks, and its glyph cache pages, which judged the C reference's GPU renderer.

### D3: the pass record

A pass is one walk through the pages, named by the time it started and the walk's process id, such as `20261103T142200-12345`. Its record is the markdown file of that name in the passes directory, `$XDG_STATE_HOME/vtium/acceptance/` (`~/.local/state/vtium/acceptance/`), or the directory `ACCEPT_DIR` names. The walk creates the directory with mode 0700 and records nothing into a directory that is a symlink, another user's or open to anyone else, as the guides' log does; the title and summary pages then say why.

The record holds, in order:

1. a list: the pass, the label (`ACCEPT_LABEL`, which a review run sets to `vtium dev build <git describe>`), the VT, the binary and its features (`ACCEPT_VT`, `ACCEPT_BINARY` and `ACCEPT_FEATURES`, or `not given`), the terminal's `TERM` and size, its DA1 answer, the font OSC 50's query named at the start, the blink style the pages that blink text are judged in and where it comes from, each font set the pass uses with its frit file's SHA-256, each font file it names with its SHA-256 and the package that owns it (or `missing`), one SHA-256 over those font lines, the hardware (below), when the pass started, when it was last written, and how many pages it has answered;
2. a table of every page in order, with its answer (yes, no, unsure, skip, or `-` for none yet) and its comment, in which `&`, `<` and `>` are entities, a backslash and `|` are escaped with a backslash, and each line break is `<br>`;
3. what each page shows when it is right, its `# look-for:` text, so the record reads on its own after the pages change.

The hardware is what the walk reads from `/sys` and `/proc` as the user: the machine's model from `/sys/class/dmi` when it is there, the kernel's version, each DRM card with its kernel driver, its PCI vendor and device and, with `lspci`, its name, each connected display's connector, its preferred mode and the make, product code and model its EDID gives, and the text area vtium reports (XTWINOPS 14), which is the mode in use as far as a program can see it. A line with one SHA-256 over those, the text area left out, says the pass is evidence about the hardware above only. A pass on this machine says nothing of another GPU, display controller or display, so the record says so where every reader sees it.

The walk writes the whole record after every answer, to a file beside it that it then renames into place, so a record is never seen half written and a pass that ends any way keeps every answer given.

When the walk starts and the newest pass with the same label still has a page without an answer, the title page offers to resume it at that page, or to start a new pass. A pass is never resumed under another label, once any font file it named has changed, or on other hardware, so one pass judges one build in one set of fonts on one machine.

### D4: comparing passes

`tools/vt/feedback.sh` reads records. With no option it prints the newest pass, or the one named; `--list` lists every pass; `--compare A B` compares two passes, each named by its id or by the path of a record, such as one committed to the repository; `--compare` alone compares the two newest. The comparison names both passes, then lists each page whose answer differs, with both answers and both comments, and marks a page that went from yes to no. It then lists every font set or font file line that is in one pass and not the other, so a page that changed between two passes is not blamed on vtium when a font package changed under it. When the two passes name different hardware, the comparison prints a warning before the pages, listing each hardware line that is in one pass and not the other: a page that changed may be the hardware's doing, and each pass is evidence about its own hardware only. It exits 1 when any page went from yes to no, and 0 otherwise.

### D5: running a pass

A pass runs as the session of a review run (`docs/vt-review.md`), with the build's details passed to the walk:

- `make vt-accept` runs it, in place of `make vt-demo`, and builds the console with pictures (`FEATURES=images`) unless `FEATURES` is named on make's command line, so a pass covers every page; a pass of the minspec build is `make vt-accept FEATURES=`. A binary named with `VTIUM_BIN` is run as it is, so without `FEATURES` the target takes its features from the binary: images when it carries the line only a build with pictures carries, the one `make vt-install` tells such a build by, and none otherwise. The target exports `FEATURES` to `vt.sh`;
- `vt.sh` passes `run-vt.sh` the VT, the binary, the features and the passes directory, and after the run prints the pass the run wrote with `feedback.sh`;
- `run-vt.sh` puts `ACCEPT_VT`, `ACCEPT_BINARY`, `ACCEPT_FEATURES` and `ACCEPT_DIR` into the session's environment beside `ACCEPT_LABEL`, each checked for the characters it may hold, and copies the system's frits and the acceptance pass's into `/run/vtium-vt/frits`, a directory of root's alone made for the run, and runs vtium with `--font-frits` naming it, so the walk can switch among them (D6). With pictures in the build, it copies the image helper built with the console, `vtium-image`, into the same directory and runs vtium with `--image-helper` naming the copy, as it does for the glyph helper, so the helper pages show pictures.

The walk also runs in any terminal by hand, `ACCEPT_LABEL='foot 1.21' tools/vt/accept.sh`, so a page can be compared in vtium and in another terminal; that pass carries its own label.

### D6: font sets

A font set is a frit (a font set file, ADR-1200 D1). The acceptance pass keeps two in `tools/vt/sections/frits/`:

- `acceptance-outline`: DejaVu Sans Mono, then Noto Sans Hebrew, Arabic, Devanagari and Thai, Noto Sans CJK and Noto Color Emoji, each by its file in Arch Linux's packages, then the built-in unifont.
- `acceptance-bitmap`: Terminus at 8x16 (`ter-v16n`, from `terminus-font`) for Latin, Greek and Cyrillic, then the built-in GNU Unifont, which holds every other script on the pages. GNU Unifont is vtium's own built-in font, so no unifont file is needed.

A page whose `# frits:` line names font sets is an entry of the walk for each, as `84-greek@acceptance-outline` and `84-greek@acceptance-bitmap`. Before such an entry the walk switches the console to the frit with OSC 50, makes a round trip to the terminal so the switch has been read, and asks OSC 50's query which font is in use; a page with no `# frits:` line switches the console back to the font the query named when the walk started. vtium switches the live console among the frits in its frits directory, so one pass covers both font sets, and the review run gives vtium a frits directory holding the acceptance pass's frits (D5). The page prints which font set it is drawn in and, from the frit's `# checks:` lines, what that set checks: glyphs scaled to the cell with marks placed from the font's own data, or whole-multiple bitmaps with marks placed by vtium's rules.

An entry is recorded as skip, with the reason as its comment, when a file its frit names is missing on this machine (the comment names the file), or when vtium does not switch to it (the comment names what OSC 50's query answered). Neither acceptance frit names a bold or an italic face, so vtium draws both styles from each set's regular glyphs: the outline set's bold grows each stroke sideways by a 24th of the size, a pixel at least, and its italic slants the glyph by 0.2 about the baseline; the bitmap set's bold widens each row by a pixel, and its italic shears the glyph by 0.2 one pixel row at a time (ADR-1302 D4).

### D7: the language pages

Each language page shows, from `tools/vt/sections/text/udhr-article1.tsv`:

1. the first sentence of Article 1 of the Universal Declaration of Human Rights in its language, which may wrap onto a second row, and its English: "All human beings are born free and equal in dignity and rights.";
2. a few of its words, with their English, in sixteen styles, two to a row: plain, bold, italic, bold italic, dim, reverse, underline, double, curly, dotted and dashed underlines, a red underline, strike, overline, and SGR 73 and 74, superscript and subscript;
3. Unicode's superscript and subscript characters, digits for every script and the small letters where Unicode has them (Latin, Greek and one Cyrillic letter), with a sentence that says where it has none;
4. a line marking the styles vtium does not draw yet with `*`: overline and SGR 73 and 74, which look plain;
5. the font set (D6) and what it checks.

The sentences come from the UDHR in XML project at a fixed commit; `tools/vt/sections/text/README.md` names it and the three entries that differ from the files. Each page's guidance describes the right picture in shapes: marks stacked on their letters and not cut at the row's top, Arabic letters each with its dots, Hangul syllables as square blocks, a decoration reaching across both cells of a wide character. Where vtium does not yet do what the script needs, the guidance says what vtium draws today and that it is expected: Hebrew and Arabic run left to right in typed order, since vtium does not order right-to-left text yet, and Arabic letters stand apart, since the console does not key joining forms yet. In the bitmap set, which has no outline source to shape a cluster, a Devanagari vowel sign that stands beside its letter is drawn over that letter. The operator judges the rest of the page and answers yes when only those look as described. The two Vietnamese pages draw the same words, once precomposed and once with every mark a combining character, so a difference between them is vtium's handling of combining marks.

> Edited 2026-11-04: the work that draws bold and italic through the glyph helper (ADR-1302 D4). D2, D6 and D7 said that a frit takes only regular faces, so bold was the regular glyph widened a pixel and italic stayed upright, and D2 listed `text.outline`'s bold and italic faces among the rows with no page. A frit now names bold, italic and bold-italic faces of its own, and vtium synthesises the styles it names no face for, so the language pages draw bold, italic and bold italic, and italic leaves the list of styles marked `*`. The earlier text is in git at 2aea866a.

### D8: committing passes, and the maintainer's answers

The maintainer answered the record's questions in the steering session's interview of 2026-11-03 (`considered`):

1. **The name.** It is the acceptance pass, run by `make vt-accept`, which replaces `make vt-demo`.
2. **Committed passes.** The steering session runs a pass on the build of each batch that goes from `stage` to main, compares it with the last committed pass, names any page that went from yes to no in the stage PR's body, and commits the pass on `integration` with the batch, as `docs/acceptance/<YYYY-MM-DD>-<git describe>.md`, the record as it stands. One pass a batch is enough because a batch is the unit that reaches main: a pass between batches judges work that has not landed, and a pass for each batch puts the judgement next to the change it judges. Passes between batches stay in the passes directory. Each committed pass is evidence about the hardware it names (D3), and the comparison warns when two passes ran on different hardware (D4).
3. **Pictures.** On by default: `make vt-accept` builds with pictures unless `FEATURES` is named (D5).
4. **A language page** takes one answer, with the comment naming any style that looked wrong (D7).
5. **The UDHR text** stays as it is (D7).
6. **unsure** keeps its key, u: a page looked at and not judged is different from a page skipped.
7. **Fonts.** One pass walks both font sets (D6).

## C notes: the C reference's demo

The C reference's `fork/demo/demo.sh` walked the section scripts in `fork/demo/sections/`, most of them links to tier 3's screen scripts, with the same footer of guidance. Its keys were 1 right, 2 wrong, 3 unsure, s skip, c comment, r redraw, b back and q quit. Each answer appended a line to `/tmp/kmscon-demo/feedback.tsv`: the time, a run id, the label, `TERM`, the size, the section, the rating and the note. `fork/demo/feedback.sh` printed one run and compared the latest run of each label side by side. A `# kit:` line put each page in a review kit, and `make vt-demo KIT=NAME` ran one kit. A picture page whose terminal lacked the protocol printed so and exited 0, and the operator rated it. The idea of committing one record per bring-home first appeared in the C reference's records for its conformance runs; the acceptance pass's cadence rests on its own grounds (D8).

## Where vtium departs from the C reference

- The answers are y, n, u and s, the guides' words, in place of 1, 2 and 3.
- Each pass is one record, rewritten whole after each answer, in a directory a reboot keeps; the C reference appended to one answers file in `/tmp`.
- A pass quit part way resumes at its first page without an answer.
- A page the build cannot show records skip by itself.
- A comparison is between two passes, by id or record path, and its exit status marks a page that went from yes to no; the C reference compared the latest run of each label.
- One pass covers every page; there are no review kits.
- The walk puts back the terminal state a page may have changed before the next page.
- Each pass names the hardware it ran on, and the comparison warns across hardware.
- Thirteen language pages, each walked in an outline and a bitmap font set with the fonts recorded; the C reference's demo ran in whatever font its console had.

## Consequences

### Positive

- The maintainer walks every visual page in one sitting, with keys only, and can stop and come back.
- Each pass names its build, so a regression between two builds shows as a page that went from yes to no, and the comparison's exit status lets the steering session check it.
- A record reads on its own: what ran, what each page should show, and what the operator said.

### Negative

- A pass takes most of an hour: 27 pages and the 13 language pages twice. A page added later is unanswered in every earlier pass.
- The pages that need programs, file media or the GPU path, and the rows vtium does not draw yet, are not judged by a pass.
- The tests of the earlier demo in `tools/vt/tests/test_vt.py` move to `tools/vt/tests/test_accept.py`, and `vt.sh`, `run-vt.sh` and the Makefile change with the walk.
- A committed pass says nothing of other GPUs, display controllers or displays; vtium on other hardware needs its own pass.

### Neutral

- `feedback.tsv` in `/tmp/vtium-vt` is no longer written; files there from earlier runs stay until a reboot clears them.
- `docs/vt-review.md`'s section on the demo becomes "The acceptance pass".

## Alternatives Considered

- **An acceptance guide for `guide.sh`.** A guide step runs a command and asks y, n or skip, so each page could be a `run:` step. Rejected: a guide asks Enter before each step's command, records to a log of every guide, and has no back, no resume and no record per pass; the demo's walk already had the footer, the keys and back.
- **Keep appending to the answers file, and compute passes from it.** Rejected: a run id among many lines is not a pass a person can read, resume or commit, and `/tmp` does not survive a reboot.
- **Write the records into the checkout's `target/`.** Rejected: `cargo clean` removes them, and each worktree would hold its own passes.
- **Commit every pass.** Rejected: most passes look at work in flight, which has not landed, and the repository would grow with every look at the VT.
- **A pass per font set.** Rejected by the maintainer (D8): one pass shows the same page in both fonts one after the other.
