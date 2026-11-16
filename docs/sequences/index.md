<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Escape sequences

A program controls vtium by writing escape sequences and control characters among its text. [Escape sequences](../concepts/escape-sequences.md) explains how they are put together, for a reader who has never met them. The modes that a program sets and resets with SM and RM are on a page of their own, [Modes](../modes.md).

Each line below gives a sequence's form, what it does, and whether vtium supports it. Its link leads to the page of the row in ADR-1000's features table that holds the sequence. A row whose sequences fall in two groups appears in each.

The forms use the notation of [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html). `Ps` stands for one number, `Pm` for a list of numbers separated by `;`, and `Pt` for text.

## Control characters

A control character is a single byte. The C0 controls are the bytes 0x00 to 0x1F, and the C1 controls are 0x80 to 0x9F, which a program can also write as ESC and a second byte.

| Sequence | What it does | Status | Page |
|---|---|---|---|
| `ENQ` | enquiry: ask for the answerback message | supported | [query.enq](query.enq.md) |
| `BEL` | ring the bell (0x07) | supported | [ctl.bel](ctl.bel.md) |
| `SO` | LS1: G1 into GL | supported | [text.charsets](text.charsets.md) |
| `SI` | LS0: G0 into GL | supported | [text.charsets](text.charsets.md) |
| `0x80 to 0x9F` | the C1 controls as single bytes | supported | [text.c1](text.c1.md) |

## ESC sequences

An ESC sequence is ESC followed by a few bytes, the last of which says what the sequence does.

| Sequence | What it does | Status | Page |
|---|---|---|---|
| `ESC # 3` | DECDHL: top half of a double-height line | supported | [text.line-size](text.line-size.md) |
| `ESC # 4` | DECDHL: bottom half of a double-height line | supported | [text.line-size](text.line-size.md) |
| `ESC # 5` | DECSWL: single-width line | supported | [text.line-size](text.line-size.md) |
| `ESC # 6` | DECDWL: double-width line | supported | [text.line-size](text.line-size.md) |
| `ESC 6` | DECBI: back index | supported | [ctl.decic](ctl.decic.md) |
| `ESC 7` | DECSC: save the cursor | supported | [ctl.decsc](ctl.decsc.md) |
| `ESC # 8` | DECALN: fill the screen with E | supported | [ctl.decaln](ctl.decaln.md) |
| `ESC 8` | DECRC: restore the cursor | supported | [ctl.decsc](ctl.decsc.md) |
| `ESC 9` | DECFI: forward index | supported | [ctl.decic](ctl.decic.md) |
| `ESC =` | DECKPAM: keypad in application mode | supported | [ctl.deckpam](ctl.deckpam.md) |
| `ESC >` | DECKPNM: keypad in numeric mode | supported | [ctl.deckpam](ctl.deckpam.md) |
| `ESC ( C` | SCS: designate a set into G0 | supported | [text.charsets](text.charsets.md) |
| `ESC ) C` | SCS: designate a set into G1 | supported | [text.charsets](text.charsets.md) |
| `ESC * C` | SCS: designate a set into G2 | supported | [text.charsets](text.charsets.md) |
| `ESC + C` | SCS: designate a set into G3 | supported | [text.charsets](text.charsets.md) |
| `ESC D` | IND: index, down one row | supported | [ctl.edit](ctl.edit.md) |
| `ESC E` | NEL: next line | supported | [ctl.edit](ctl.edit.md) |
| `ESC SP F` | S7C1T: send 7-bit controls | supported | [text.c1](text.c1.md) |
| `ESC SP G` | S8C1T: send 8-bit controls | supported | [text.c1](text.c1.md) |
| `ESC H` | HTS: set a tab stop at the cursor's column | supported | [ctl.tabs](ctl.tabs.md) |
| `ESC M` | RI: reverse index, up one row | supported | [ctl.edit](ctl.edit.md) |
| `ESC N` | SS2: G2 for the next character | supported | [text.charsets](text.charsets.md) |
| `ESC O` | SS3: G3 for the next character | supported | [text.charsets](text.charsets.md) |
| `ESC Z` | DECID: identify the terminal | supported | [query.decid](query.decid.md) |
| `ESC c` | RIS: reset to initial state | supported | [ctl.decstr](ctl.decstr.md) |
| `ESC n` | LS2: G2 into GL | supported | [text.charsets](text.charsets.md) |
| `ESC o` | LS3: G3 into GL | supported | [text.charsets](text.charsets.md) |
| <code>ESC &#124;</code> | LS3R: G3 into GR | supported | [text.charsets](text.charsets.md) |
| `ESC }` | LS2R: G2 into GR | supported | [text.charsets](text.charsets.md) |
| `ESC ~` | LS1R: G1 into GR | supported | [text.charsets](text.charsets.md) |

## CSI sequences

A CSI sequence is ESC [ followed by numeric parameters and a final byte that says what the sequence does. A private marker such as `?` or `>` can come before the parameters, and intermediate bytes such as `$` or a space can come before the final byte. Either one makes the sequence a different function. The sequences below are sorted by their final byte.

| Sequence | What it does | Status | Page |
|---|---|---|---|
| `CSI Ps @` | ICH: insert blank characters | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps A` | CUU: cursor up | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps B` | CUD: cursor down | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps C` | CUF: cursor forward | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps D` | CUB: cursor back | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps E` | CNL: cursor to the start of a following line | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps F` | CPL: cursor to the start of a preceding line | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps G` | CHA: cursor to a column | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps ; Ps H` | CUP: cursor to a row and column | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps I` | CHT: forward to a following tab stop | supported | [ctl.tabs](ctl.tabs.md) |
| `CSI ? Ps J` | DECSED: erase the unprotected cells of the display | supported | [ctl.protect](ctl.protect.md) |
| `CSI Ps J` | ED: erase in display | supported | [ctl.edit](ctl.edit.md) |
| `CSI 3 J` | ED 3: erase the scrollback | supported | [ctl.ed3](ctl.ed3.md) |
| `CSI 22 J` | move the screen's text into the scrollback, then clear the screen | ignored | [ctl.kitty-clear](ctl.kitty-clear.md) |
| `CSI ? Ps K` | DECSEL: erase the unprotected cells of the line | supported | [ctl.protect](ctl.protect.md) |
| `CSI Ps K` | EL: erase in line | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps L` | IL: insert lines | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps M` | DL: delete lines | supported | [ctl.edit](ctl.edit.md) |
| `CSI Pm # P` | XTPUSHCOLORS: push the colors | supported | [color.stack](color.stack.md) |
| `CSI Ps P` | DCH: delete characters | supported | [ctl.edit](ctl.edit.md) |
| `CSI Pm # Q` | XTPOPCOLORS: pop the colors | supported | [color.stack](color.stack.md) |
| `CSI # R` | XTREPORTCOLORS: report the stack | supported | [color.stack](color.stack.md) |
| `CSI # S` | XTTITLEPOS: report the title stack's position | ignored | [window.xtsmtitle](window.xtsmtitle.md) |
| `CSI ? Pi ; Pa ; Pv S` | XTSMGRAPHICS: set or read a graphics attribute | supported | [query.xtsmgraphics](query.xtsmgraphics.md) |
| `CSI Ps ; Ps SP S` | SPD: select the presentation directions | supported | [ctl.spd](ctl.spd.md) |
| `CSI Ps S` | SU: scroll up | supported | [ctl.edit](ctl.edit.md) |
| `CSI > Pm T` | XTRMTITLE: reset the title's encoding features | ignored | [window.xtsmtitle](window.xtsmtitle.md) |
| `CSI Ps + T` | unscroll: bring lines back from the scrollback | ignored | [ctl.kitty-unscroll](ctl.kitty-unscroll.md) |
| `CSI Ps ; Ps ; Ps ; Ps ; Ps T` | XTHIMOUSE: the program's answer to hilite mouse tracking | refused | [input.xthimouse](input.xthimouse.md) |
| `CSI Ps T` | SD: scroll down | supported | [ctl.edit](ctl.edit.md) |
| `CSI ? 5 W` | DECST8C: set a tab stop every 8 columns | ignored | [ctl.decst8c](ctl.decst8c.md) |
| `CSI Ps X` | ECH: erase characters | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps Z` | CBT: back to a preceding tab stop | supported | [ctl.tabs](ctl.tabs.md) |
| `CSI 9 ; Ps ]` | set the screen blank's timeout | supported | [ctl.linux-blank](ctl.linux-blank.md) |
| `CSI 13 ]` | wake the screen | supported | [ctl.linux-unblank](ctl.linux-unblank.md) |
| `CSI 14 ; Ps ]` | set the VESA powerdown interval | ignored | [ctl.linux-powerdown](ctl.linux-powerdown.md) |
| `CSI 16 ; Ps ]` | set the cursor's blink interval | supported | [ctl.linux-blink-interval](ctl.linux-blink-interval.md) |
| `` CSI Ps ` `` | HPA: cursor to a column, as CHA does | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps a` | HPR: cursor forward, as CUF does | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps b` | REP: repeat the last character | supported | [text.rep](text.rep.md) |
| `CSI = Ps c` | DA3: tertiary device attributes | supported | [query.da3](query.da3.md) |
| `CSI > Ps c` | DA2: secondary device attributes | supported | [query.da2](query.da2.md) |
| `CSI Ps c` | DA1: primary device attributes | supported | [query.da1](query.da1.md) |
| `CSI Ps d` | VPA: cursor to a row | supported | [ctl.edit](ctl.edit.md) |
| `CSI Ps e` | VPR: cursor down, as CUD does | supported | [ctl.edit](ctl.edit.md) |
| `CSI > Pp ; Pv f` | XTFMTKEYS: choose the format of modified keys | ignored | [input.format-other-keys](input.format-other-keys.md) |
| `CSI Ps ; Ps f` | HVP: cursor to a row and column, as CUP does | supported | [ctl.edit](ctl.edit.md) |
| `CSI ? Pp g` | XTQFMTKEYS: ask for that format | ignored | [input.format-other-keys](input.format-other-keys.md) |
| `CSI Ps g` | TBC: clear tab stops | supported | [ctl.tabs](ctl.tabs.md) |
| `CSI Ps ; Ps SP k` | SCP: select the character path | supported | [ctl.scp](ctl.scp.md) |
| `CSI ? 2 l` | DECANM reset: enter VT52 mode | ignored | [ctl.vt52](ctl.vt52.md) |
| `CSI 0 m` | reset every attribute and both colors | supported | [sgr.bold](sgr.bold.md) |
| `CSI 1 m` | bold | supported | [sgr.bold](sgr.bold.md) |
| `CSI 2 m` | dim, also called faint | supported | [sgr.dim](sgr.dim.md) |
| `CSI 3 m` | italic | supported | [sgr.italic](sgr.italic.md) |
| `CSI 4 : 0 m` | no underline | supported | [sgr.underline-style](sgr.underline-style.md) |
| `CSI 4 : 1 m` | single underline | supported | [sgr.underline-style](sgr.underline-style.md) |
| `CSI 4 : 2 m` | double underline | supported | [sgr.underline-style](sgr.underline-style.md) |
| `CSI 4 : 3 m` | curly underline | supported | [sgr.underline-style](sgr.underline-style.md) |
| `CSI 4 : 4 m` | dotted underline | supported | [sgr.underline-style](sgr.underline-style.md) |
| `CSI 4 : 5 m` | dashed underline | supported | [sgr.underline-style](sgr.underline-style.md) |
| `CSI 4 m` | single underline | supported | [sgr.underline](sgr.underline.md) |
| `CSI > 4 ; Pv m` | XTMODKEYS: set the modifyOtherKeys level | supported | [input.modify-other-keys](input.modify-other-keys.md) |
| `CSI ? 4 m` | XTQMODKEYS: ask for the modifyOtherKeys level | supported | [input.modify-other-keys](input.modify-other-keys.md) |
| `CSI 5 m` | blink | supported | [sgr.blink](sgr.blink.md) |
| `CSI 6 m` | rapid blink | supported | [sgr.rapid-blink](sgr.rapid-blink.md) |
| `CSI 7 m` | inverse | supported | [sgr.inverse](sgr.inverse.md) |
| `CSI 8 m` | conceal | supported | [sgr.conceal](sgr.conceal.md) |
| `CSI 9 m` | strike through | supported | [sgr.strike](sgr.strike.md) |
| `CSI 10 m to CSI 20 m` | primary and alternative fonts, and Fraktur | ignored | [sgr.ecma-other](sgr.ecma-other.md) |
| `CSI 21 m` | double underline | supported | [sgr.double-underline](sgr.double-underline.md) |
| `CSI 22 m` | normal intensity: neither bold nor dim | supported | [sgr.bold](sgr.bold.md) |
| `CSI 23 m` | not italic | supported | [sgr.italic](sgr.italic.md) |
| `CSI 24 m` | no underline | supported | [sgr.underline](sgr.underline.md) |
| `CSI 25 m` | no blink | supported | [sgr.blink](sgr.blink.md) |
| `CSI 26 m` | proportional spacing | ignored | [sgr.ecma-other](sgr.ecma-other.md) |
| `CSI 27 m` | not inverse | supported | [sgr.inverse](sgr.inverse.md) |
| `CSI 28 m` | reveal | supported | [sgr.conceal](sgr.conceal.md) |
| `CSI 29 m` | no strike | supported | [sgr.strike](sgr.strike.md) |
| `CSI 30 m to CSI 37 m` | foreground from palette colors 0 to 7 | supported | [sgr.palette](sgr.palette.md) |
| `CSI 38 : 2 : Pi : Pr : Pg : Pb m` | foreground as red, green and blue | supported | [sgr.extended-color](sgr.extended-color.md) |
| `CSI 38 : 5 : Ps m` | foreground from the 256-color palette | supported | [sgr.extended-color](sgr.extended-color.md) |
| `CSI 38 : 6 : : Pr : Pg : Pb : Pa m` | foreground with an alpha value | ignored | [sgr.rgba](sgr.rgba.md) |
| `CSI 39 m` | default foreground | supported | [sgr.palette](sgr.palette.md) |
| `CSI 40 m to CSI 47 m` | background from palette colors 0 to 7 | supported | [sgr.palette](sgr.palette.md) |
| `CSI 48 : 2 : Pi : Pr : Pg : Pb m` | background as red, green and blue | supported | [sgr.extended-color](sgr.extended-color.md) |
| `CSI 48 : 5 : Ps m` | background from the 256-color palette | supported | [sgr.extended-color](sgr.extended-color.md) |
| `CSI 48 : 6 : : Pr : Pg : Pb : Pa m` | background with an alpha value | ignored | [sgr.rgba](sgr.rgba.md) |
| `CSI 49 m` | default background | supported | [sgr.palette](sgr.palette.md) |
| `CSI 50 m to CSI 52 m` | no proportional spacing, framed and encircled | ignored | [sgr.ecma-other](sgr.ecma-other.md) |
| `CSI 53 m` | overline | supported | [sgr.overline](sgr.overline.md) |
| `CSI 54 m` | neither framed nor encircled | ignored | [sgr.ecma-other](sgr.ecma-other.md) |
| `CSI 55 m` | no overline | supported | [sgr.overline](sgr.overline.md) |
| `CSI 58 : 2 : : Pr : Pg : Pb m` | underline color as red, green and blue | supported | [sgr.underline-color](sgr.underline-color.md) |
| `CSI 58 : 5 : Ps m` | underline color from the 256-color palette | supported | [sgr.underline-color](sgr.underline-color.md) |
| `CSI 58 : 6 : : Pr : Pg : Pb : Pa m` | underline color with an alpha value | ignored | [sgr.rgba](sgr.rgba.md) |
| `CSI 59 m` | underline in the text's color | supported | [sgr.underline-color](sgr.underline-color.md) |
| `CSI 60 m to CSI 65 m` | ideogram underlines, overlines and stress marks | ignored | [sgr.ecma-other](sgr.ecma-other.md) |
| `CSI 73 m` | superscript | supported | [sgr.script](sgr.script.md) |
| `CSI 74 m` | subscript | supported | [sgr.script](sgr.script.md) |
| `CSI 75 m` | neither superscript nor subscript | supported | [sgr.script](sgr.script.md) |
| `CSI 90 m to CSI 97 m` | foreground from the bright palette colors 8 to 15 | supported | [sgr.palette](sgr.palette.md) |
| `CSI 100 m to CSI 107 m` | background from the bright palette colors 8 to 15 | supported | [sgr.palette](sgr.palette.md) |
| `CSI 221 m` | turn off bold alone | ignored | [sgr.bold-faint-reset](sgr.bold-faint-reset.md) |
| `CSI 222 m` | turn off dim alone | ignored | [sgr.bold-faint-reset](sgr.bold-faint-reset.md) |
| `CSI > 4 n` | turn modifyOtherKeys off | supported | [input.modify-other-keys](input.modify-other-keys.md) |
| `CSI 5 n` | DSR: device status report | supported | [query.dsr](query.dsr.md) |
| `CSI 6 n` | CPR: report the cursor's position | supported | [query.dsr](query.dsr.md) |
| `CSI ? 6 n` | DECXCPR: report the cursor's position and page | supported | [query.dsr](query.dsr.md) |
| `CSI ? 15 n` | printer status | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 25 n` | user-defined keys status | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 26 n` | keyboard status | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 55 n` | locator status | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 56 n` | locator type | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 62 n` | macro space report | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 63 n` | memory checksum report | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 75 n` | data integrity report | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 85 n` | multiple-session status | ignored | [query.dsr-other](query.dsr-other.md) |
| `CSI ? 996 n` | ask whether the colors are dark or light | supported | [color.scheme-report](color.scheme-report.md) |
| `CSI ! p` | DECSTR: soft terminal reset | supported | [ctl.decstr](ctl.decstr.md) |
| `CSI > Ps p` | XTSMPOINTER: hide the pointer while typing | ignored | [window.xtsmpointer](window.xtsmpointer.md) |
| `CSI ? Pd $ p` | DECRQM: report a DEC private mode | supported | [query.decrqm](query.decrqm.md) |
| `CSI Pa $ p` | DECRQM: report an ANSI mode | supported | [query.decrqm](query.decrqm.md) |
| `CSI Pl ; Pc " p` | DECSCL: set the conformance level | supported | [ctl.decscl](ctl.decscl.md) |
| `CSI Pm # p` | XTPUSHSGR, the other form | supported | [sgr.stack](sgr.stack.md) |
| `CSI # q` | XTPOPSGR, the other form | supported | [sgr.stack](sgr.stack.md) |
| `CSI > Ps ; Pt SP q` | add or remove extra cursors | ignored | [cursor.kitty-multiple](cursor.kitty-multiple.md) |
| `CSI > Ps q` | XTVERSION: report the terminal's name and version | supported | [query.xtversion](query.xtversion.md) |
| `CSI Ps " q` | DECSCA: protect the characters printed next | supported | [ctl.protect](ctl.protect.md) |
| `CSI Ps SP q` | DECSCUSR: set the cursor style | supported | [cursor.style](cursor.style.md) |
| `CSI Ps q` | DECLL: load the keyboard's LEDs | ignored | [ctl.decll](ctl.decll.md) |
| `CSI ? Pm r` | XTRESTORE: restore DEC private modes | supported | [ctl.xtsave](ctl.xtsave.md) |
| `CSI Ps ; Ps r` | DECSTBM: set the top and bottom margins | supported | [ctl.decstbm](ctl.decstbm.md) |
| `CSI Pt ; Pl ; Pb ; Pr ; Pm $ r` | DECCARA: change the attributes of a rectangle | supported | [ctl.rect](ctl.rect.md) |
| `CSI > Ps s` | XTSHIFTESCAPE: let Shift reach mouse reports | supported | [input.xtshiftescape](input.xtshiftescape.md) |
| `CSI ? Pm s` | XTSAVE: save DEC private modes | supported | [ctl.xtsave](ctl.xtsave.md) |
| `CSI Ps ; Ps s` | DECSLRM: set the left and right margins, while mode 69 is set | supported | [ctl.decslrm](ctl.decslrm.md) |
| `CSI s` | SCOSC: save the cursor, while mode 69 is reset | supported | [ctl.decslrm](ctl.decslrm.md) |
| `CSI > Pm t` | XTSMTITLE: set the title's encoding features | ignored | [window.xtsmtitle](window.xtsmtitle.md) |
| `CSI Ps SP t` | DECSWBV: set the warning bell's volume | ignored | [ctl.bell-volume](ctl.bell-volume.md) |
| `CSI Ps t` | DECSLPP: set lines per page, for Ps of 24 or more | ignored | [ctl.page-size](ctl.page-size.md) |
| `CSI Pt ; Pl ; Pb ; Pr ; Pm $ t` | DECRARA: reverse the attributes of a rectangle | supported | [ctl.rect](ctl.rect.md) |
| `CSI 1 t` | XTWINOPS: de-iconify the window | refused | [window.manipulate](window.manipulate.md) |
| `CSI 2 t` | XTWINOPS: iconify the window | refused | [window.manipulate](window.manipulate.md) |
| `CSI 3 ; Px ; Py t` | XTWINOPS: move the window | refused | [window.manipulate](window.manipulate.md) |
| `CSI 4 ; Ph ; Pw t` | XTWINOPS: resize the window in pixels | refused | [window.manipulate](window.manipulate.md) |
| `CSI 5 t` | XTWINOPS: raise the window | refused | [window.manipulate](window.manipulate.md) |
| `CSI 6 t` | XTWINOPS: lower the window | refused | [window.manipulate](window.manipulate.md) |
| `CSI 7 t` | XTWINOPS: refresh the window | refused | [window.manipulate](window.manipulate.md) |
| `CSI 8 ; Ph ; Pw t` | XTWINOPS: resize the text area in characters | supported | [window.resize](window.resize.md) |
| `CSI 9 ; Ps t` | XTWINOPS: maximize or restore the window | refused | [window.manipulate](window.manipulate.md) |
| `CSI 10 ; Ps t` | XTWINOPS: full screen | refused | [window.manipulate](window.manipulate.md) |
| `CSI 11 t` | XTWINOPS: report whether the window is iconified | ignored | [window.state-report](window.state-report.md) |
| `CSI 13 t` | XTWINOPS: report the window's position | ignored | [window.state-report](window.state-report.md) |
| `CSI 14 ; 2 t` | XTWINOPS: report the window's size in pixels | ignored | [window.state-report](window.state-report.md) |
| `CSI 14 t` | XTWINOPS: report the text area's size in pixels | supported | [window.size-report](window.size-report.md) |
| `CSI 15 t` | XTWINOPS: report the screen's size in pixels | supported | [window.screen-pixels](window.screen-pixels.md) |
| `CSI 16 t` | XTWINOPS: report the cell's size in pixels | supported | [window.size-report](window.size-report.md) |
| `CSI 18 t` | XTWINOPS: report the text area's size in characters | supported | [window.size-report](window.size-report.md) |
| `CSI 19 t` | XTWINOPS: report the screen's size in characters | supported | [window.size-report](window.size-report.md) |
| `CSI 20 t` | XTWINOPS: report the icon's label | ignored | [window.icon-report](window.icon-report.md) |
| `CSI 21 t` | XTWINOPS: report the window's title | supported | [window.title-report](window.title-report.md) |
| `CSI 22 ; Ps t` | XTWINOPS: push the title on a stack | ignored | [window.title-stack](window.title-stack.md) |
| `CSI 23 ; Ps t` | XTWINOPS: pop the title from the stack | ignored | [window.title-stack](window.title-stack.md) |
| `CSI & u` | DECRQUPSS: report the user-preferred character set | ignored | [query.decreqtparm](query.decreqtparm.md) |
| `CSI < Ps u` | pop sets of flags | supported | [input.kitty-keyboard](input.kitty-keyboard.md) |
| `CSI = flags ; Pm u` | change the flags in force | supported | [input.kitty-keyboard](input.kitty-keyboard.md) |
| `CSI > flags u` | push a set of flags | supported | [input.kitty-keyboard](input.kitty-keyboard.md) |
| `CSI ? u` | ask for the flags in force | supported | [input.kitty-keyboard](input.kitty-keyboard.md) |
| `CSI Pk ; Pm u` | a key, as vtium reports it | supported | [input.kitty-keyboard](input.kitty-keyboard.md) |
| `CSI Ps $ u` | DECRQTSR: report the terminal's state | ignored | [query.decreqtparm](query.decreqtparm.md) |
| `CSI Ps SP u` | DECSMBV: set the margin bell's volume | ignored | [ctl.bell-volume](ctl.bell-volume.md) |
| `CSI u` | SCORC: restore the cursor | supported | [ctl.decslrm](ctl.decslrm.md) |
| `CSI " v` | DECRQDE: report the window's extent | ignored | [query.decreqtparm](query.decreqtparm.md) |
| `CSI Pt ; Pl ; Pb ; Pr ; Pp ; Pt ; Pl ; Pp $ v` | DECCRA: copy a rectangle | supported | [ctl.rect](ctl.rect.md) |
| `CSI Ps $ w` | DECRQPSR: report the cursor (DECCIR) or the tab stops (DECTABSR) | ignored | [query.decrqpsr](query.decrqpsr.md) |
| `CSI Pt ; Pl ; Pb ; Pr ' w` | DECEFR: enable a filter rectangle | ignored | [query.locator](query.locator.md) |
| `CSI Pc ; Pt ; Pl ; Pb ; Pr $ x` | DECFRA: fill a rectangle with a character | supported | [ctl.rect](ctl.rect.md) |
| `CSI Ps * x` | DECSACE: choose what DECCARA and DECRARA act on | supported | [ctl.rect](ctl.rect.md) |
| `CSI Ps x` | DECREQTPARM: report the serial line's parameters | ignored | [query.decreqtparm](query.decreqtparm.md) |
| `CSI Pi ; Pg ; Pt ; Pl ; Pb ; Pr * y` | DECRQCRA: report the checksum of an area | ignored | [query.xtreportsgr](query.xtreportsgr.md) |
| `CSI Ps # y` | XTCHECKSUM: choose how DECRQCRA computes its checksum | ignored | [query.xtreportsgr](query.xtreportsgr.md) |
| `CSI Ps ; Pu ' z` | DECELR: enable locator reports | ignored | [query.locator](query.locator.md) |
| `CSI Pt ; Pl ; Pb ; Pr $ z` | DECERA: erase a rectangle | supported | [ctl.rect](ctl.rect.md) |
| `CSI Pm # {` | XTPUSHSGR: push the pen | supported | [sgr.stack](sgr.stack.md) |
| `CSI Pm ' {` | DECSLE: select locator events | ignored | [query.locator](query.locator.md) |
| `CSI Pt ; Pl ; Pb ; Pr $ {` | DECSERA: erase the unprotected cells of a rectangle | supported | [ctl.rect](ctl.rect.md) |
| <code>CSI Ps $ &#124;</code> | DECSCPP: set columns per page | ignored | [ctl.page-size](ctl.page-size.md) |
| <code>CSI Ps ' &#124;</code> | DECRQLP: request the locator's position | ignored | [query.locator](query.locator.md) |
| <code>CSI Ps * &#124;</code> | DECSNLS: set lines per screen | ignored | [ctl.page-size](ctl.page-size.md) |
| <code>CSI Pt ; Pl ; Pb ; Pr # &#124;</code> | XTREPORTSGR: report the SGR of an area | ignored | [query.xtreportsgr](query.xtreportsgr.md) |
| `CSI # }` | XTPOPSGR: pop the pen | supported | [sgr.stack](sgr.stack.md) |
| `CSI Ps $ }` | DECSASD: select the active display | ignored | [ctl.status-line](ctl.status-line.md) |
| `CSI Ps ' }` | DECIC: insert columns | supported | [ctl.decic](ctl.decic.md) |
| `CSI Ps $ ~` | DECSSDT: select the status line's type | ignored | [ctl.status-line](ctl.status-line.md) |
| `CSI Ps ' ~` | DECDC: delete columns | supported | [ctl.decic](ctl.decic.md) |

## OSC strings

An OSC string is ESC ] followed by text, and ST or BEL ends it. Most strings start with a number that gives the text its meaning. A few start with a word or a letter instead, such as `setBackground` or `l`. The strings below are sorted by their number.

| Sequence | What it does | Status | Page |
|---|---|---|---|
| `OSC 0 ; Pt ST` | set the icon name and the title | ignored | [window.title](window.title.md) |
| `OSC 1 ; Pt ST` | set the icon name | ignored | [window.title](window.title.md) |
| `OSC 2 ; Pt ST` | set the title | ignored | [window.title](window.title.md) |
| `OSC 3 ; Pt ST` | set an X property on the window | ignored | [window.xprop](window.xprop.md) |
| `OSC 4 ; c ; ? ST` | ask for palette color c | supported | [color.palette.query](color.palette.query.md) |
| `OSC 4 ; c ; spec ST` | set palette color c | supported | [color.palette.set](color.palette.set.md) |
| `OSC 5 ; c ; spec ST` | set a special color | ignored | [color.special](color.special.md) |
| `OSC 6 ; c ; f ST` | turn a special color on or off | ignored | [color.special](color.special.md) |
| `OSC 7 ; URI ST` | report the working directory | ignored | [osc.7](osc.7.md) |
| `OSC 8 ; ; ST` | end the link | supported | [osc.8](osc.8.md) |
| `OSC 8 ; params ; URI ST` | start a link | supported | [osc.8](osc.8.md) |
| `OSC 9 ; 4 ; Ps ; Ps ST` | report progress | ignored | [osc.9.4](osc.9.4.md) |
| `OSC 9 ; Pt ST` | show a notification | ignored | [osc.notify](osc.notify.md) |
| `OSC 10 ; ? ST` | ask for the default foreground | supported | [color.dynamic.query](color.dynamic.query.md) |
| `OSC 10 ; spec ST` | set the default foreground | supported | [color.dynamic.set](color.dynamic.set.md) |
| `OSC 11 ; ? ST` | ask for the default background | supported | [color.dynamic.query](color.dynamic.query.md) |
| `OSC 11 ; spec ST` | set the default background | supported | [color.dynamic.set](color.dynamic.set.md) |
| `OSC 12 ; ? ST` | ask for the cursor's color | supported | [color.dynamic.set](color.dynamic.set.md) |
| `OSC 12 ; ? ST` | ask for the cursor's color | supported | [cursor.color](cursor.color.md) |
| `OSC 12 ; spec ST` | set the cursor's color | supported | [color.dynamic.set](color.dynamic.set.md) |
| `OSC 12 ; spec ST` | set the cursor's color | supported | [cursor.color](cursor.color.md) |
| `OSC 13 ; spec ST` | set the pointer's foreground | supported | [color.pointer](color.pointer.md) |
| `OSC 14 ; spec ST` | set the pointer's background | supported | [color.pointer](color.pointer.md) |
| `OSC 15 ; spec ST` | set the Tektronix foreground | ignored | [color.tektronix](color.tektronix.md) |
| `OSC 16 ; spec ST` | set the Tektronix background | ignored | [color.tektronix](color.tektronix.md) |
| `OSC 17 ; spec ST` | set the selection's background | supported | [color.selection](color.selection.md) |
| `OSC 18 ; spec ST` | set the Tektronix cursor's color | ignored | [color.tektronix](color.tektronix.md) |
| `OSC 19 ; spec ST` | set the selection's foreground | supported | [color.selection](color.selection.md) |
| `OSC 21 ; Pt ST` | set, ask for or reset colors by key | supported | [color.kitty-protocol](color.kitty-protocol.md) |
| `OSC 22 ; < ST` | pop a shape from the pointer's stack | supported | [window.pointer-shape](window.pointer-shape.md) |
| `OSC 22 ; = Pt ST` | set the pointer's shape | supported | [window.pointer-shape](window.pointer-shape.md) |
| `OSC 22 ; > Pt ST` | push shapes on the pointer's stack | supported | [window.pointer-shape](window.pointer-shape.md) |
| `OSC 22 ; ? Pt ST` | ask which shapes vtium has | supported | [window.pointer-shape](window.pointer-shape.md) |
| `OSC 22 ; Pt ST` | set the pointer's shape | supported | [window.pointer-shape](window.pointer-shape.md) |
| `OSC 46 ; Pt ST` | change the log file | refused | [osc.46](osc.46.md) |
| `OSC 50 ; ? ST` | ask which font set is in use | supported | [text.font-change](text.font-change.md) |
| `OSC 50 ; Pt ST` | switch to the font set Pt | supported | [text.font-change](text.font-change.md) |
| `OSC 51 ; Pt ST` | reserved for Emacs | ignored | [osc.app-specific](osc.app-specific.md) |
| `OSC 52 ; Pc ; ? ST` | ask for the clipboard's text | refused | [osc.52.read](osc.52.read.md) |
| `OSC 52 ; Pc ; Pd ST` | set the clipboard | supported | [osc.52.write](osc.52.write.md) |
| `OSC 60 ; Pt ST` | contour's font settings | ignored | [text.contour-fonts](text.contour-fonts.md) |
| `OSC 60 ST` | XTQALLOWED: list the allowed features | ignored | [query.xtqallowed](query.xtqallowed.md) |
| `OSC 61 ; Pt ST` | XTQDISALLOWED: list the disallowed features | ignored | [query.xtqallowed](query.xtqallowed.md) |
| `OSC 62 ; Pt ST` | XTQALLOWABLE: list the features that may be allowed | ignored | [query.xtqallowed](query.xtqallowed.md) |
| `OSC 66 ; metadata ; Pt ST` | write text at a larger size | partial | [text.kitty-sizing](text.kitty-sizing.md) |
| `OSC 72 ; Pt ST` | kitty's drag and drop | ignored | [osc.72](osc.72.md) |
| `OSC 99 ; Pt ST` | kitty's notifications | ignored | [osc.notify](osc.notify.md) |
| `OSC 104 ; c ST` | reset palette color c | supported | [color.palette.set](color.palette.set.md) |
| `OSC 105 ; c ST` | reset a special color | ignored | [color.special](color.special.md) |
| `OSC 106 ; c ; f ST` | turn a special color on or off | ignored | [color.special](color.special.md) |
| `OSC 110 ST` | reset the default foreground | supported | [color.dynamic.set](color.dynamic.set.md) |
| `OSC 111 ST` | reset the default background | supported | [color.dynamic.set](color.dynamic.set.md) |
| `OSC 112 ST` | reset the cursor's color | supported | [color.dynamic.set](color.dynamic.set.md) |
| `OSC 112 ST` | reset the cursor's color | supported | [cursor.color](cursor.color.md) |
| `OSC 113 ST` | reset the pointer's foreground | supported | [color.pointer](color.pointer.md) |
| `OSC 114 ST` | reset the pointer's background | supported | [color.pointer](color.pointer.md) |
| `OSC 115 ST` | reset the Tektronix foreground | ignored | [color.tektronix](color.tektronix.md) |
| `OSC 116 ST` | reset the Tektronix background | ignored | [color.tektronix](color.tektronix.md) |
| `OSC 117 ST` | reset the selection's background | supported | [color.selection](color.selection.md) |
| `OSC 118 ST` | reset the Tektronix cursor's color | ignored | [color.tektronix](color.tektronix.md) |
| `OSC 119 ST` | reset the selection's foreground | supported | [color.selection](color.selection.md) |
| `OSC 133 ; A ST` | a prompt starts | supported | [osc.133](osc.133.md) |
| `OSC 133 ; B ST` | the user's input starts | supported | [osc.133](osc.133.md) |
| `OSC 133 ; C ST` | the command's output starts | supported | [osc.133](osc.133.md) |
| `OSC 133 ; D ; Ps ST` | the command ends, with its exit status | supported | [osc.133](osc.133.md) |
| `OSC 133 ; I ST` | the user's input starts, as B | supported | [osc.133](osc.133.md) |
| `OSC 133 ; L ST` | move to a fresh line | supported | [osc.133](osc.133.md) |
| `OSC 133 ; N ST` | a prompt starts, as A | supported | [osc.133](osc.133.md) |
| `OSC 133 ; P ST` | a part of the prompt, which vtium ignores | supported | [osc.133](osc.133.md) |
| `OSC 176 ; Pt ST` | set an application id | ignored | [osc.app-specific](osc.app-specific.md) |
| `OSC 555 ST` | flash the screen | ignored | [osc.app-specific](osc.app-specific.md) |
| `OSC 777 ; notify ; Pt ST` | show a notification with a title | ignored | [osc.notify](osc.notify.md) |
| `OSC 1337 ; File=keys : Pt ST` | a picture in one string | supported | [image.iterm2](image.iterm2.md) |
| `OSC 1337 ; FileEnd ST` | end the picture sent in parts | supported | [image.iterm2](image.iterm2.md) |
| `OSC 1337 ; FilePart=Pt ST` | a part of the picture | supported | [image.iterm2](image.iterm2.md) |
| `OSC 1337 ; MultipartFile=keys ST` | start a picture sent in parts | supported | [image.iterm2](image.iterm2.md) |
| `OSC 1337 ; Pt ST` | iTerm2's other strings, such as SetUserVar | ignored | [osc.1337](osc.1337.md) |
| `OSC 3008 ; Pt ST` | report a context | ignored | [osc.app-specific](osc.app-specific.md) |
| `OSC 5113 ; Pt ST` | kitty's file transfer | refused | [osc.5113](osc.5113.md) |
| `OSC 5522 ; Pt ST` | kitty's clipboard with several types | ignored | [osc.5522](osc.5522.md) |
| `OSC 7501 ; Pt ST` | report a program's status | ignored | [osc.app-specific](osc.app-specific.md) |
| `OSC 30001 ST` | push the colors | supported | [color.stack](color.stack.md) |
| `OSC 30101 ST` | pop the colors | supported | [color.stack](color.stack.md) |
| `OSC I Pt ST` | set the icon from a file, in Sun's form | ignored | [window.xprop](window.xprop.md) |
| `OSC L Pt ST` | set the icon name, in Sun's form | ignored | [window.xprop](window.xprop.md) |
| `OSC l Pt ST` | set the title, in Sun's form | ignored | [window.xprop](window.xprop.md) |
| `OSC setBackground ST` | hand the display to a graphical program | supported | [osc.kmscon-display](osc.kmscon-display.md) |
| `OSC setForeground ST` | take the display back | supported | [osc.kmscon-display](osc.kmscon-display.md) |

## DCS strings

A DCS string is ESC P followed by parameters, a final byte, the data, and ST to end it.

| Sequence | What it does | Status | Page |
|---|---|---|---|
| `DCS $ q Pt ST` | DECRQSS: request a setting | supported | [query.decrqss](query.decrqss.md) |
| `DCS + Q Pt ST` | XTGETXRES: request X resources | ignored | [query.xtsettcap](query.xtsettcap.md) |
| `DCS + p Pt ST` | XTSETTCAP: set a terminfo capability | ignored | [query.xtsettcap](query.xtsettcap.md) |
| `DCS + q Pt ST` | XTGETTCAP: request terminfo capabilities | supported | [query.xtgettcap](query.xtgettcap.md) |
| `DCS @kitty-cmd Pt ST` | kitty's remote control | refused | [dcs.kitty-cmd](dcs.kitty-cmd.md) |
| `DCS P1 ; P2 ; P3 q Pt ST` | a sixel picture | supported | [image.sixel](image.sixel.md) |
| `DCS Pm p Pt ST` | a ReGIS drawing | ignored | [image.regis](image.regis.md) |
| `DCS Pm { Pt ST` | DECDLD: load a soft font | ignored | [image.decdld](image.decdld.md) |
| `DCS Ps $ t Pt ST` | DECRSPS: restore the cursor or the tab stops | ignored | [query.decrqpsr](query.decrqpsr.md) |
| <code>DCS Ps ; Ps &#124; Pt ST</code> | DECUDK: define user-defined keys | refused | [input.decudk](input.decudk.md) |
| `DCS = 1 s ST` | begin a synchronized update | supported | [ctl.sync-update](ctl.sync-update.md) |
| `DCS = 2 s ST` | end a synchronized update | supported | [ctl.sync-update](ctl.sync-update.md) |
| `DCS 1000 p Pt ST` | tmux's control mode | ignored | [dcs.tmux](dcs.tmux.md) |

## APC strings

An APC string is ESC _ followed by data and ST. vtium reads kitty's graphics protocol from it.

| Sequence | What it does | Status | Page |
|---|---|---|---|
| `APC G a=a,keys ST` | control the animation | supported | [image.kitty-animation](image.kitty-animation.md) |
| `APC G a=c,keys ST` | compose one frame onto another | supported | [image.kitty-animation](image.kitty-animation.md) |
| `APC G a=f,keys ; Pt ST` | send or edit a frame | supported | [image.kitty-animation](image.kitty-animation.md) |
| `APC G a=p,P=Ps,Q=Ps,H=Ps,V=Ps,keys ST` | place a picture relative to another | supported | [image.kitty-relative](image.kitty-relative.md) |
| `APC G a=p,U=1,keys ST` | make a virtual placement | supported | [image.kitty-placeholders](image.kitty-placeholders.md) |
| `APC G a=q,e=Pt,keys ST` | ask which keys an action takes | supported | [image.kitty-feature-query](image.kitty-feature-query.md) |
| `APC G keys ; Pt ST` | a kitty graphics command | supported | [image.kitty](image.kitty.md) |

