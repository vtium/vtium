---
contract: adr/v1
kind: decision
verb: add
capability: protocols
basis:
  - operator: aaronsb
    level: directed
    said: "this could be far lighter than we think - almost just a shell tool that proves out using vtium-config in property mode to load a vtfrit and look compatible to display the image. 2. color translation, maybe, with the same theme as the first item 3. for now, a mapping between raw colors? 4. again, a mapper 5. either a filter, or vtfrit can define the stretch, just like we do for arabic fonts for example 6. rewrite. I'm beginning to pick up a theme - we just make a viewer that flexes vtium's current capabilities. 7. strip it out, map it 8. again, it's just a vtfrit 'mode' at this point. (same could be said with petscii, or atari fonts etc"
    via: the GitHub issue that tracks ANSI art, comment of 2026-11-16 headed "The maintainer's answers", posted from the maintainer's account; the numbers answer the issue's eight open questions
  - operator: aaronsb
    level: directed
    said: "this could be another package that sits along side vtium-fonts. 'vtium-ansi' or something"
    via: the same issue, comment of 2026-11-16 on packaging
  - operator: aaronsb
    level: directed
    said: "for now, a simple program that views .ans files. maybe later it becomes it's own shell that runs, (invokes as bash or zsh might)"
    via: the same issue, comment of 2026-11-16 on scope
  - operator: aaronsb
    level: directed
    said: "The maintainer decided on 2026-11-20 that the ANSI art viewer is in scope for 0.1.0, in the alpha phase."
    via: the same issue, comment of 2026-11-20; the wording is the steering session's
    paraphrase: true
  - standard: "SAUCE (Standard Architecture for Universal Comment Extensions) 00.5, acid.org/info/sauce/sauce.htm: the 128-byte record, the comment block, the end-of-file byte 0x1A, and for character files TInfo1 (width), TInfo2 (height), TFlags (non-blink mode, letter spacing, aspect ratio) and TInfoS (the font's name)"
  - standard: "SyncTERM's CTerm documentation, syncterm.bbsdev.net/cterm.html: ANSI music, the `CSI ? 33 h` and `l` iCE color mode, and the 24-bit color form `CSI 0|1 ; R ; G ; B t`"
  - standard: "IBM code page 437, as Unicode's mapping table for it gives bytes 0x80 to 0xFF, with the glyphs the IBM PC's character ROM shows for 0x01 to 0x1F and 0x7F"
  - evidence: "A research pass of 2026-11-16, recorded on the issue that tracks ANSI art: 1,006 `.ANS` files from 48 packs on sixteencolors.net, 1992 to 2024, each item ranked by the share of files that use it. CP437 bytes 0x80 to 0xFF appear in 97% of the files; SGR 0, 1, 30 to 37 and 40 to 47 in 30% to 93% each; CUF to skip cells in 74%; 0x1A in 80% and a SAUCE record in 68%; wrapping at column 80 in 56%; a full line followed by CR LF, which DOS turns into an extra row, in 14%; ED 2 in 21%, followed by text with no CUP in 17%; SGR 5 as a bright background in 8% with SAUCE's flag and 8% more without it; C0 bytes drawn as glyphs in 17%; the SAUCE font IBM VGA in 40%, with 8-pixel spacing in 35% and 9-pixel spacing in 14%; SyncTERM's `CSI ? 33 h` in 1%; PabloDraw's 24-bit form in 2%; widths other than 80 in 6.5%; Amiga fonts in 7.4%; the aspect stretch in 2.6%; ANSI music in none. No picture was rendered, and the packs were taken alphabetically within each year."
  - evidence: "Read by this record's agent on 2026-11-22 at integration 6f061cceb: `crates/vtium-term/src/palette.rs`, the palette `vga`, holds VGA's 16 colors (0 and 170 for the dark colors, with brown at 170, 85, 0, and 85 and 255 for the bright ones); `crates/vtium-config/frits/classic/ibm-vga.vtfrit` names Px437 IBM VGA 9x16 at its 9×16 cell, then the layers nerd-symbols, noto-emoji and unifont; `crates/vtium-config/frits/console/linux-vga-8x16.vtfrit` draws the kernel's VGA font from kbd's cp865-8x16, which lacks code page 437's ¢, ¥ and »; ADR-1000's tables mark `sgr.blink`, `color.palette.set`, `color.stack`, `text.font-change` and `window.resize` supported, and hold no row for ANSI art, CP437 input or SyncTERM's mode 33"
  - precedent: ADR-1007
  - precedent: ADR-1200
  - precedent: ADR-1204
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-22
deciders:
  - aaronsb
related: [ADR-1000, ADR-1007, ADR-1200, ADR-1204, ADR-1205]
---

# ADR-1010: ANSI art from the BBS days is shown by vtium-ansi, a viewer that converts each file into the text and colors vtium already draws

## Summary

- **Decided:** ANSI art, the pictures made of text that bulletin board systems (BBSes) showed on the IBM PC, shows on vtium through a viewer of its own, `vtium-ansi`. The viewer reads a `.ans` file and its SAUCE record, the block of metadata at the file's end. It converts the file's code page 437 (CP437) bytes to Unicode and plays the file's cursor moves into a grid of its own, wrapping at column 80 as DOS did. It then writes that grid to the terminal as UTF-8 text with VGA's 16 colors as 24-bit colors. On a vtium console it switches to a font set made for the art, from the Px437 fonts, and switches back when you leave. vtium's terminal engine gets no new sequence, mode or character set, and ADR-1000 gains no row. The viewer and its font sets ship as a package of their own, `vtium-ansi`, beside vtium-fonts.
- **Trades away:** A raw `cat picture.ans` still shows garbage, because only the viewer knows CP437. A BBS client that sends ANSI art straight to the terminal gets no help from vtium, and SyncTERM's iCE color mode stays unbuilt in the engine. VGA's tall pixels draw square for now.
- **One-way?** No. The viewer is a separate program that nothing else depends on, and the engine stays as it is. The program's name and its font sets' names become expensive to change once the package ships.
- **Probes:**
  - *Not confident (the view):* `vtium-ansi FILE` opens a full-screen view: the picture in its own font set, scrolled with the arrow keys, and q gives your screen and font back. `vtium-ansi --print FILE` writes the picture to the terminal as `cat` would, and keeps the current font. The font set has to be given back while the picture is off the screen, so the view is the default. Should the default be the full-screen view, or print only?
  - *Not confident (iCE colors without a flag):* half of the files that use SGR 5 as a bright background, the iCE colors, carry no SAUCE flag that says so. Lean: iCE colors unless the file's SAUCE record says otherwise, with `--blink` to make SGR 5 blink. The other choices are blinking text unless the flag is set, as SAUCE reads, and iCE colors always.
  - *Confident (colors):* the viewer writes VGA's 16 colors as 24-bit colors and leaves the console's palette alone, so the picture keeps its colors after the viewer exits and in the scrollback. Changing the palette with OSC 4 would recolor everything on the screen and change back when the viewer left. Is that what you want?
  - *Not confident (a program, not a script):* you called it "almost just a shell tool". The record makes it a small Rust program in vtium's workspace, because the conversion reads untrusted bytes, needs bounds and tests, and vtium's tools are Rust. Is a Rust program right, or do you want a shell script that pipes through `iconv` first?
- **Inversion:** Between "the terminal speaks ANSI.SYS and CP437 itself, so any program can send ANSI art" and "only a docs page with an `iconv` recipe". The answer sits between them: a viewer that uses what vtium already draws, and leaves the engine to the specifications it follows.

## Context

ANSI art is a picture made of text. Artists drew it on the IBM PC for bulletin board systems in the 1990s, and the scene still makes it. A `.ans` file is a stream of bytes in code page 437, the IBM PC's character set, with the escape sequences of DOS's ANSI.SYS driver that move the cursor and set colors. Most files end with the byte 0x1A, which DOS read as the end of a text file, and then a SAUCE record, 128 bytes of metadata that give the picture's width, its font and how it uses blinking.

The maintainer asked on 2026-11-16 that someone fond of the BBS days can show their art on a vtium console in its own colors and characters, supporting "most as practical" of what the art uses. A research pass then scanned 1,006 files from 48 packs on sixteencolors.net, from 1992 to 2024, and counted what each one uses (`basis`). Almost every file needs CP437, most use the 16 colors and cursor skips, and many need SAUCE read and DOS's wrap at column 80.

vtium already draws most of what the art needs. It supports the cursor moves and erases that ANSI.SYS knows, SGR's 16 colors and 24-bit colors, blinking text, OSC 4 and the color stack, and a switch of the font set with OSC 50 (ADR-1200 D5). Its catalog holds `classic/ibm-vga`, from the Px437 IBM VGA 9x16 font (ADR-1204 D3). What vtium does not do is the part that belongs to DOS:

- In UTF-8, each byte from 0x80 to 0xFF is not a character on its own, and 0x80 to 0x9F are C1 controls in 8-bit input. CP437's characters therefore come out wrong.
- The art draws bytes 0x01 to 0x1F as glyphs, such as ☺, ♥ and ►. A terminal acts on them as controls or drops them.
- vtium prints the byte 0x1A as a character, and then shows the SAUCE record as text.
- DOS wraps the moment column 80 is written, where vtium keeps the pending wrap that VT510 defines. A full line followed by CR LF then gives one row on vtium and two on DOS.
- ANSI.SYS's `CSI 2 J` clears the screen and also homes the cursor; ECMA-48's ED leaves the cursor where it is.
- An SGR 5 in iCE art means a bright background, not blinking text. vtium's SGR 1 also draws a bold face, where DOS only brightened the color.
- vtium's grid is usually wider than 80 columns.

The maintainer answered the issue's eight questions on 2026-11-16 with one theme: "we just make a viewer that flexes vtium's current capabilities". The viewer converts, rewrites and strips; the terminal engine stays as it is. Each art family that needs other characters, such as Amiga, PETSCII or Atari art, is a font set plus the viewer's mapping. The maintainer then asked for a package of its own beside vtium-fonts, named `vtium-ansi`, and for a simple viewer first, with a shell mode as a later step. On 2026-11-20 the maintainer put the viewer in scope for 0.1.0.

## Decision

### D1: a viewer, and no change to the engine

- **The program** is `vtium-ansi`, a binary in a crate of its own, `crates/vtium-ansi/`, written in Rust, as vtium-clipboard is (ADR-1007). It is outside the minspec build and its size budget.
- **The engine stays as it is.** vtium-term gets no CP437 character set, no ANSI.SYS mode and no SyncTERM mode 33, and its pending wrap stays as VT510 defines it. ADR-1000 gains no row and changes none. CP437 is a property of the file, so the program that reads the file converts it, and every other program keeps the terminal it has.
- **The output is ordinary terminal text.** The viewer writes UTF-8 text, CUP, SGR with 24-bit colors and SGR 5, and on a vtium console OSC 50 and the alternate screen. All of these are rows that ADR-1000 already marks supported. So the viewer's `--print` mode works in any terminal that supports UTF-8 and 24-bit color, over ssh included.

### D2: reading the file and its SAUCE record

- **Bounds.** The viewer reads at most `FILE_MAX` bytes of a file, 4 MiB (this record's agent's choice, which the build checks against the largest file of the scan's packs). It refuses a larger file and says so.
- **The SAUCE record.** When the last 128 bytes of the file begin with `SAUCE00`, the viewer reads them as the SAUCE specification lays them out. A comment block may sit before the record. It begins with `COMNT`, and the record's comment count gives its size in 64-byte lines. The picture ends at the byte before the comment block, or before the record when there is none, and also at the first 0x1A byte.
- **What the viewer takes from it,** for a character file of type ASCII or ANSi:
  - `TInfo1`, the width in columns. A width of 0, or one over `WIDTH_MAX` (1,000 columns), counts as 80.
  - `TInfo2`, the height in rows. The viewer uses it only to check its own count, and logs a difference with `--verbose`.
  - `TFlags`. Bit 0 is the non-blink mode, which turns SGR 5 into a bright background (D4). Bits 1 and 2 give the letter spacing, 8 or 9 pixels. Bits 3 and 4 give the aspect ratio, which the viewer reads but does not apply (D7).
  - `TInfoS`, the font's name, such as `IBM VGA`, which picks the font set (D5).
- **Without a SAUCE record,** the picture is 80 columns wide, in IBM VGA with 9-pixel spacing, and SGR 5 follows the default that the probe on iCE colors settles. A malformed record counts as none, and the viewer says which field it could not read.

### D3: playing the file into a grid of the viewer's own

The viewer does not pass the file's sequences on. It plays the file into a grid of cells of its own, as ANSI.SYS would have drawn it on a screen the picture's width, and then writes that grid (D6). Each cell holds a character, a foreground and a background color, and a blink flag. The grid grows by rows as the picture needs, up to `ROWS_MAX`, 10,000 rows (this record's agent's choice), and the viewer stops reading at that row and says so.

- **Characters.** Bytes 0x20 to 0x7E are ASCII. Bytes 0x80 to 0xFF become their Unicode characters by CP437's standard table, such as 0xB0 to ░, 0xC4 to ─ and 0xDB to █. Bytes 0x01 to 0x1F and 0x7F become the glyphs the IBM PC shows for them, such as 0x01 to ☺, 0x10 to ► and 0x7F to ⌂, apart from the controls below. 0x00 draws a space.
- **The controls ANSI.SYS acts on.** CR moves to the row's first column. LF moves down one row in the same column. BS moves one column left. HT moves to the next multiple of 8 columns. ESC begins a sequence. BEL makes no sound and draws nothing. 0x1A ends the picture.
- **The wrap.** Writing a character in the last column moves the cursor to the start of the next row at once, as DOS did. So a full row followed by CR LF leaves a blank row, as it did on DOS, and the SAUCE height counts that row.
- **The sequences.** The viewer plays CUU, CUD, CUF, CUB, CUP and HVP, all within the grid's width; ED, where `CSI 2 J` clears the grid and also homes the cursor, as ANSI.SYS did; EL; SGR (D4); and `CSI s` and `CSI u`, which save and restore the cursor. `CSI ? 7 h` and `l` turn the wrap on and off. SyncTERM's `CSI ? 33 h` turns iCE colors on and `CSI ? 33 l` turns them off. PabloDraw's `CSI 0 ; R ; G ; B t` sets the background and `CSI 1 ; R ; G ; B t` the foreground to a 24-bit color.
- **ANSI music** is dropped: from `CSI M` or `CSI N` with no parameter, through the 0x0E byte that ends the tune, as SyncTERM's CTerm documentation describes it. A `CSI M` with a parameter is DL, which ANSI.SYS does not support, and the viewer drops it too.
- **Everything else** is dropped, and the viewer counts what it dropped for `--verbose`: any other sequence, and any sequence that does not end within 64 bytes.

### D4: colors

- **The 16 colors** are VGA's, the values of vtium's `vga` palette: 0 and 170 for the eight dark colors, with brown at 170, 85, 0, and 85 and 255 for the eight bright ones. The viewer writes every cell's colors as 24-bit SGR, `38;2` and `48;2`, so the picture looks the same whatever palette the console uses, and the console's palette never changes.
- **SGR 1** makes the foreground bright, as DOS did. The viewer never sends SGR 1 itself, so vtium draws no bold face. SGR 22 makes the foreground dark again.
- **SGR 5.** With iCE colors on, SGR 5 makes the background bright, so the art can use all 16 background colors. With iCE colors off, it marks the cell as blinking, and the viewer sends SGR 5 for the cell, which vtium blinks. SGR 25 clears it.
- **SGR 7** swaps the cell's two colors. SGR 8 draws the cell's character in its background color. SGR 0 returns to light grey on black, with iCE colors as they were. vtium.conf's `blink-text` decides how blinking text looks, as for any program.

### D5: the font sets

- **The `vtium-ansi` package ships font sets** for the art, under the category folder `ansi/` of the frits' directory (ADR-1204 D3). Each draws code page 437 from a Px437 font of the Ultimate Oldschool PC Font Pack, at the font's own cell. Each states `glyph-source=font`, so the font's own box drawing, blocks and shades ░, ▒ and ▓ draw, with VGA's dot patterns, in place of vtium's sprites (ADR-1200 D1). After the Px437 font comes the `unifont` layer, which draws any character the font lacks.
- **The first sets:**

| Font set | The font | Cell | Chosen for |
|---|---|---|---|
| `ansi/ibm-vga-9x16` | Px437 IBM VGA 9x16 | 9×16 | the SAUCE font `IBM VGA` with 9-pixel spacing, and files with no SAUCE record |
| `ansi/ibm-vga-8x16` | Px437 IBM VGA 8x16 | 8×16 | `IBM VGA` with 8-pixel spacing, or with no spacing given |
| `ansi/ibm-ega-8x14` | Px437 IBM EGA 8x14 | 8×14 | `IBM EGA` |
| `ansi/ibm-ega-8x8` | Px437 IBM EGA 8x8 | 8×8 | `IBM VGA50` and `IBM EGA43` |

- **Any other SAUCE font,** an Amiga font for one, draws in `ansi/ibm-vga-9x16`, and the viewer says which font the file asked for. Amiga, PETSCII and Atari art each become a font set and a mapping later (D7).
- **The fonts.** vtium-fonts already carries Px437 IBM VGA 9x16 (ADR-1204), and the `vtium-ansi` package depends on vtium-fonts for it. The package carries the other three Px437 files itself, under the same license, CC-BY-SA-4.0, with the attribution the license asks for, in vtium-fonts' notices format (ADR-1204).
- **The switch.** On a vtium console, the viewer finds the console the way vtium-config's `use-frit` does: XTVERSION, the OSC 50 query and DA1 (ADR-1201 D4). It keeps the name the query reports, sends `OSC 50 ; ansi/... ST` for the picture's set, and sends the kept name back when it leaves. `--font NAME` names another set. In a terminal that is not vtium, or with `allow-font-ops` off, the viewer sends no OSC 50, and the picture draws in the terminal's font.

### D6: how a person shows a picture

- **`vtium-ansi FILE...`** opens a full-screen view on the alternate screen, in the picture's font set, with the picture at the top left. The arrow keys, Page Up, Page Down, Home and End scroll it, and n and p go to the next and the previous file. q or Esc leaves: the viewer switches the font set back, leaves the alternate screen, and your screen is as it was. A picture wider than the grid scrolls left and right as well. Where the grid is wider than the picture, the columns to its right stay black.
- **`vtium-ansi --print FILE...`** writes each picture to standard output, one row at a time, each row ending with SGR 0 and CR LF, and sends no OSC 50. The picture stays in the scrollback and works over ssh and in a pipe. When standard output is a terminal narrower than the picture, the viewer cuts each row at the terminal's width, so the terminal's own wrap does not break the picture.
- **The options.** `--ice` and `--blink` set how SGR 5 reads, over the SAUCE record. `--width N` sets the width of a file that has no SAUCE record. `--font NAME` picks the font set. `--verbose` reports the SAUCE fields and what the viewer dropped. The viewer reads no configuration file.
- **Signals.** Ctrl+C, SIGINT, SIGTERM, SIGHUP and SIGQUIT in the view do what q does, and the viewer holds them while the terminal is raw, as vtium-config's `use-frit` does. After SIGKILL the console keeps the picture's font set until someone switches it back or the session ends (ADR-1201 D4).

### D7: what stays out

- **CP437 in the engine,** such as the Linux console's `ESC ( U`: 0x80 to 0x9F are C1 controls that a UTF-8 console keeps, and the C0 glyphs collide with LF, BS and the shifts.
- **SyncTERM's mode 33 in the engine,** a `bold-face` key, and PabloDraw's 24-bit form in the engine. The viewer rewrites all three.
- **Playing ANSI music** on the PC speaker. The viewer strips the tune.
- **VGA's aspect ratio,** pixels about 1.35 times as tall as they are wide. The art draws with square pixels until a font set can state a stretch, as the maintainer suggested, or a spatial filter does it.
- **Amiga, PETSCII and Atari art,** each a font set and a mapping in a later version. ANSiMation files play into the grid and show their last frame.
- **Other formats:** Binary Text and XBin files, Avatar, RIPscrip and PCBoard's color codes.
- **A shell mode,** `vtium-ansi --shell`, which would run as a child of your login shell and return to it on `exit`. It is a later step, as the maintainer said.
- **Art from archives in the tree.** The pictures on sixteencolors.net belong to their artists. A file enters the tree only under a license that allows it, and archive art stays a manual check in the VT review.

### D8: how it is judged

- **The conversion.** Unit tests feed the converter files that the project makes, each a few hundred bytes, and compare its grid with an expected grid written from SAUCE, CP437's table and ANSI.SYS's behaviour. The cases: all 256 bytes in a 16×16 grid; an 80-character row followed by more text, which wraps; an 80-character row followed by CR LF, which leaves a blank row; `CSI 2 J` then text with no CUP; SGR 5 with and without the iCE flag, and under `--blink`; SGR 1 on each color; a file ending in 0x1A, a comment block and a SAUCE record; PabloDraw's 24-bit form; a tune that must not delete a row; and a file at `FILE_MAX` and one byte over.
- **The output.** A test compares `--print`'s bytes for each case with expected bytes, and a tier 1 scenario feeds those bytes to vtium through the replay interface, so tier 1 judges what vtium shows. Its expected results come from ctlseqs, not from vtium's output.
- **The look.** A tier 2 picture draws ░, ▒, ▓ and the line drawing under `ansi/ibm-vga-9x16`, recorded as provisional for the maintainer.
- **A fuzz target** feeds the converter generated files.
- **The VT tour** shows a few pictures from the archives in the view and with `--print`, which the maintainer judges by eye.
- **Before the build,** the build card checks four points against their sources: that ANSI.SYS's `CSI 2 J` homes the cursor; that LF alone keeps the column; that Px437 IBM VGA 9x16 already draws VGA's ninth column for the line-drawing bytes 0xC0 to 0xDF; and the exact form of ANSI music in CTerm's documentation.

## Consequences

### Positive

- A person can show BBS-era art on a vtium console with one command, in the right characters, colors and font, and gets the screen and font back afterwards.
- The engine stays as its specifications define it, so no program that sends ordinary text sees new behaviour.
- `--print` works in any terminal with UTF-8 and 24-bit color, so the converter is useful beyond the console.
- The art's untrusted bytes are read by a small program that writes only text and SGR, so a malformed file can stop the viewer but cannot reach the console's parser with anything new.

### Negative

- A raw `cat` of a `.ans` file, or a BBS client that sends art straight to the terminal, still shows the wrong characters.
- The package depends on vtium-fonts, and carries three font files of its own.
- An art file that needs another family's font, such as an Amiga picture, draws in the VGA font until its font set exists.

### Neutral

- The four font sets join the catalog's category folders as `ansi/`, and ADR-1204's catalog pages list them where vtium-ansi is installed.
- The vtium-arch recipes gain a `vtium-ansi` package.
- ADR-1000's tables do not change.

## Alternatives Considered

- **A CP437 input mode in the engine,** as the Linux console's `ESC ( U` selects. Not taken: 0x80 to 0x9F are C1 controls in a UTF-8 console, the C0 glyphs collide with the controls a terminal acts on, and the maintainer chose a viewer.
- **An ANSI helper beside the image helper,** which a program hands the art in a sequence of vtium's own, and which converts it in a sandbox into cells. Suggested on 2026-11-16 as option (d), then set aside by the maintainer's answers of the same day, which ask for a viewer that uses what vtium can already do. It would work from any program over any connection, but it adds a sequence, a helper and an ADR-1000 row.
- **Only a docs page with an `iconv` recipe.** Not taken: `iconv` keeps 0x01 to 0x1F as controls, and cannot handle SAUCE, the wrap or iCE colors.
- **The palette set with OSC 4** inside the color stack, in place of 24-bit colors. Not taken: popping the stack when the viewer leaves would recolor the picture on the screen and in the scrollback.
- **An 80-column grid with `CSI 8 ; ; 80 t`.** Not taken: `allow-window-ops` is off by default, and the viewer places the picture on any grid by itself.
- **Passing the file's sequences on after converting the characters.** Not taken: DOS's immediate wrap, ANSI.SYS's ED 2 and cursor moves past the picture's width all differ from vtium's, and playing the file into a grid of the viewer's own gets each of them right in one place.
