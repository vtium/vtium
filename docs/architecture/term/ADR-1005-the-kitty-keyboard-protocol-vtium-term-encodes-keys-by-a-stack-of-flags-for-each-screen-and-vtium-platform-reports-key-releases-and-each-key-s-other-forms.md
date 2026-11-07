---
contract: adr/v1
kind: decision
verb: add
capability: protocols
basis:
  - standard: "kitty's Comprehensive keyboard handling in terminals, https://sw.kovidgoyal.net/kitty/keyboard-protocol/, read 2026-11-07: the five progressive enhancement flags 1, 2, 4, 8 and 16; CSI = flags ; mode u with modes 1 (set exactly), 2 (set bits) and 3 (clear bits); CSI ? u answered CSI ? flags u; CSI > flags u pushes, flags omitted meaning 0, and CSI < number u pops, number omitted meaning 1; the terminal limits the stack's size, keeps separate stacks for the main and alternate screens, resets all flags when a pop empties the stack, and evicts the oldest entry when a push finds it full; the key code is the unshifted key, the shifted key is sent only with Shift, and the base layout key is the physical key's key in the standard PC-101 layout; the modifier bits shift 1, alt 2, ctrl 4, super 8, hyper 16, meta 32, caps_lock 64, num_lock 128, and a modifier key's own event carries its bit as the event leaves it; event types press 1, repeat 2, release 3; Enter, Tab and Backspace keep their legacy bytes and send no release unless flag 8 is set, so that a user can still type reset; detection by CSI ? u followed by DA1; in legacy mode a terminal may send functional keys that have no legacy encoding as CSI u or ignore them; the functional key table of Private Use Area numbers"
  - standard: "xterm's Control Sequences (ctlseqs), Alt and Meta Keys, PC-Style Function Keys, and XTMODKEYS and XTQMODKEYS (CSI > 4 ; Pv m and CSI ? 4 m), the encodings vtium sends today"
  - standard: "Linux's include/uapi/linux/input-event-codes.h: the kernel names each key code after the key's place on the US PC keyboard, KEY_Q 16 and KEY_A 30 among them, and KEY_102ND 86 for the extra key of ISO keyboards, which the US layout lacks"
  - standard: "xkeyboard-config 2.49, symbols/pc: Meta_L is on Mod1, the modifier Alt uses, and Hyper_L has no modifier map unless an option such as those of the symbols file hyper loads one"
  - precedent: ADR-1000
  - precedent: ADR-1002
  - precedent: ADR-1401
  - precedent: ADR-1505
  - evidence: "vtium at ee2883ec, read 2026-11-07, vtium-term: Terminal::key takes the keysym after Shift, the ASCII layout's keysym, the modifiers and one character, and reports presses only (crates/vtium-term/src/terminal/input.rs:560); modifyOtherKeys is consulted before the xterm encoding (input.rs, key_bytes); Mods numbers Shift 1, Lock 2, Ctrl 4, Alt 8, Logo 16 and NumLock 32 (input.rs:11-20); DECSTR sets modifyOtherKeys to 0 (crates/vtium-term/src/terminal.rs:668-683) and RIS runs DECSTR's reset first (terminal.rs:690); the saved cursor is kept once for each screen, indexed by whether the alternate screen is shown (terminal.rs:651); CSI u with no marker and no parameter restores the cursor (terminal.rs:842); CSI > 4 m sets modifyOtherKeys and CSI ? 4 m answers it (terminal.rs:1033-1039, reply.rs:321); the reply rule's list of queries is generated from the Query enum (crates/vtium-term/src/terminal/reply.rs:30)"
  - evidence: "vtium at ee2883ec, read 2026-11-07, vtium-platform: a keyboard dispatches a key event for a press only, and a release only stops the repeat (crates/vtium-platform/src/input/keyboard.rs:347); the event is built after xkb's state takes the key, so its modifiers include the key's own effect (keyboard.rs:334, :354); the modifiers are Shift, Lock, Control, Alt and Super, with no NumLock, Hyper or Meta (keyboard.rs:411; crates/vtium-platform/src/input/event.rs:8-14); the ASCII keysym is the first configured layout's that gives one at the key's current level (keyboard.rs:430); the repeat is vtium's own timer, and a repeat is marked (event.rs:35-48)"
  - evidence: "vtium at ee2883ec, read 2026-11-07, the vtium binary: the key order is the VT's keys, the power keys, the console and focus chords, the paste key, the grabs, DECARM, then the encoder (crates/vtium/src/app.rs:2724-2833); the grabs are Shift with Up, Down, Page Up and Page Down, Ctrl and Shift with Up and Down on the main screen, Ctrl with plus, equals and minus, and Super with plus, equals and minus (crates/vtium/src/grab.rs:50-63); the chords are Ctrl and Super with Right, Left, Return, BackSpace, Up and Down (crates/vtium/src/head/chord.rs:36-43); the VT keys are Ctrl+Alt+F1 to F12 and XF86Switch_VT_1 to 12 (crates/vtium-platform/src/vt/kernel.rs:784); the shown VT's own key is taken before routing (app.rs:2567); the binary passes the platform's modifier bits to vtium-term unchanged, so no key carries NumLock (app.rs:2818), though crates/vtium-term/README.md says the host adds it for mode 1035; a new session starts from RIS (crates/vtium/src/head/session.rs:490-494); vtium's terminfo entry resets with rs2=\\Ec (crates/vtium/install/xterm-vtium.ti:211)"
  - evidence: "kitty 0.49.2 run 2026-11-07 as a black box under its own Xvfb display, with an empty home directory and --config NONE, driven through its pty by a script that sent each sequence and then CSI ? u and DA1: it starts at 0; CSI = 3 u with nothing pushed gives 3, a push of 1 and one pop give 3 again, and a pop of 2 then gives 0; after pushes of 1 to 12 the pops read 12 down to 5 and then 0, so it keeps eight levels, the flags in force and seven saved, and drops the oldest; CSI < 0 u pops nothing; CSI > 255 u reads back 127, two bits more than the five the specification documents; each screen keeps its own flags, and the alternate screen's survive leaving it and entering it again by 47, 1047 and 1049; DECSTR and RIS each set both screens' flags to 0, and DECSTR leaves the alternate screen shown. Its key events were not observed: keys injected through XTEST did not reach its window"
  - evidence: "Alacritty 0.17.0 (94e7c887) run 2026-11-07 the same way: it keeps at least 300 pushed levels; CSI > 255 u reads back 31; the alternate screen's flags survive leaving it and entering it again by 1049; RIS sets both screens' flags to 0 and DECSTR changes neither; CSI = flags ; mode u changed nothing in this run"
  - upstream: "foot at 152487e494 (MIT), read 2026-11-07: eight flag levels for each screen (terminal.h:245); a push onto a full stack overwrites the oldest level (csi.c:1743-1756); a pop past the bottom leaves 0 (csi.c:1787-1806); term_reset clears both screens' stacks on a soft reset and a hard reset alike (terminal.c:2131-2133); the kitty encoding is used while the flags in force are not 0, and the legacy encoding, modifyOtherKeys included, while they are (input.c:1827-1829); the base layout key is the key's symbol at the first level of the keymap's first layout, and the key code is the key's first-level symbol unless a modifier the protocol cannot encode, such as AltGr, chose the symbol (input.c:1435-1471)"
  - upstream: "kitty's configuration documentation, https://sw.kovidgoyal.net/kitty/conf/, read 2026-11-07: map ctrl+shift+delete clear_terminal reset active, a default shortcut that resets the terminal"
  - operator: aaronsb
    level: guided
    said: "hand some to the program, but also, Do other terminal emulators allow remapping of these?"
    via: "the maintainer's answer, relayed by the steering session on 2026-11-07, to the probe on whether vtium's own keys stay vtium's while a program uses the protocol"
  - operator: aaronsb
    level: directed
    said: "yes bound by default, but, again, could we make this configurable, or|and: what do other terminals offer?"
    via: "the maintainer's answer, relayed by the steering session on 2026-11-07, to the probe on Ctrl+Shift+Delete resetting the shown console"
  - upstream: "kitty's documentation, read 2026-11-07. https://sw.kovidgoyal.net/kitty/conf/: map KEY ACTION binds a key; map with no action unmaps a key and passes it to the program; map KEY discard_event drops it; clear_all_shortcuts yes removes every default; kitty_mod, Ctrl+Shift by default, is the modifier of the defaults; the defaults scroll with Ctrl+Shift and Up, Down, Page Up, Page Down, Home and End, jump between prompts with Ctrl+Shift+Z and X, change the font size with Ctrl+Shift and Equal, Plus, Minus and Backspace, paste with Ctrl+Shift+V and Shift+Insert, and reset with Ctrl+Shift+Delete. https://sw.kovidgoyal.net/kitty/mapping/: map --when-focus-on title:keyboard.protocol kitty_mod+t unmaps Ctrl+Shift+T only for a window whose title matches, and only then does kitten show-key -m kitty see the key instead of a new tab opening"
  - upstream: "foot.ini(5), https://man.archlinux.org/man/foot.ini.5.en, read 2026-11-07: the [key-bindings] section takes action=combo1...comboN; action=none unbinds an action's keys; noop lists keys never sent to the program; the defaults are scrollback-up-page Shift+Page_Up, scrollback-down-page Shift+Page_Down, prompt-prev Control+Shift+z, prompt-next Control+Shift+x, font-increase Control+plus and Control+equal, font-decrease Control+minus, font-reset Control+0, clipboard-paste Control+Shift+v, primary-paste Shift+Insert; it documents no terminal reset action and nothing about its bindings under the kitty protocol"
  - upstream: "WezTerm's documentation, read 2026-11-07. https://wezterm.org/config/keys.html: config.keys entries override the defaults, and the action DisableDefaultAssignment turns one default off. https://wezterm.org/config/default-keys.html: disable_default_key_bindings = true turns them all off; the defaults scroll by page with Shift+PageUp and PageDown, change the font size with Ctrl or Super and -, = and 0, paste the primary selection with Shift+Insert and the clipboard with Ctrl+Shift+V, and bind no key to ResetTerminal. https://wezterm.org/config/key-encoding.html: input that does not match a key assignment is encoded and sent to the program. https://wezterm.org/config/lua/config/enable_kitty_keyboard.html: the kitty protocol is off unless enable_kitty_keyboard = true. https://wezterm.org/config/lua/keyassignment/ResetTerminal.html: ResetTerminal sends RIS to the pane"
  - upstream: "Ghostty's documentation, read 2026-11-07. https://ghostty.org/docs/config/keybind: keybind = trigger=action; unbind removes a binding and sends the key to the program; ignore drops the key; by default a keybind consumes the input, unconsumed: sends the key to the program as well, and performable: consumes it only when the action can be performed, as in performable:ctrl+c=copy_to_clipboard; ghostty +list-keybinds --default lists the defaults, which the pages do not. https://ghostty.org/docs/config/reference: keybind = clear removes every binding, the defaults included. https://ghostty.org/docs/config/keybind/reference: the reset action resets the terminal, equivalent to running the reset command"
  - upstream: "Alacritty's documentation, read 2026-11-07. https://alacritty.org/config-alacritty.html: [keyboard] bindings take key, mods, mode and an action, chars or command; the action ReceiveChar removes a default binding and None inhibits any action; mode is AppCursor, AppKeypad, Search, Alt or Vi, and ~ negates it; no action resets the terminal. https://alacritty.org/config-alacritty-bindings.html: the defaults bind Shift with PageUp, PageDown, Home and End to scrolling with the mode ~Alt, Ctrl with 0, =, + and - to the font size, and Shift+Insert to PasteSelection. The page lists the mode Alt without defining it; this record reads it as the alternate screen"
  - upstream: "xterm 411's manual page, xterm(1), read 2026-11-07 on this machine (the copy at invisible-island.net refused the download): the translations resource rebinds keys, and omitTranslation leaves out parts of the defaults; the default translations scroll half a page with Shift+Prior and Shift+Next, paste with Shift+Insert, and change the font with Shift+KP_Add and Shift+KP_Subtract; Soft Reset and Full Reset are entries of the VT Options menu, on no key"
  - upstream: "The Linux console's documentation, read 2026-11-07. keymaps(5), https://man.archlinux.org/man/keymaps.5.en: loadkeys binds actions to keys, as in shift keycode 107 = Scroll_Backward, control alt keycode 83 = Boot and alt keycode 105 = Decr_Console. kbd_mode(1), https://man.archlinux.org/man/kbd_mode.1.en: only the XLATE and UNICODE modes use the loadkeys key mapping, so a program that sets RAW or MEDIUMRAW receives every key, the console switching keys included. https://docs.kernel.org/admin-guide/sysrq.html: Alt+SysRq+R turns the keyboard's raw mode back to XLATE and Alt+SysRq+K kills every program on the console, under the value 4 of /proc/sys/kernel/sysrq; the page calls unraw very handy when an X server crashes"
  - operator: aaronsb
    level: guided
    said: "also, vtium-config might be able to assist with configuration of configurables."
    via: "the maintainer, relayed by the steering session on 2026-11-07, on how a user changes the vtium.conf keys this record adds"
  - operator: aaronsb
    level: directed
    said: "again, the configuration file is where mappings could exist, and the assumption is one does not need to actually use vtim-config to sanely configure via the config file (one can just edit it by hand if wanted)"
    via: "the maintainer, relayed by the steering session on 2026-11-07, clarifying where the key mappings live and vtium-config's part in them"
  - operator: aaronsb
    level: directed
    said: "the same pattern we have for defaults, where if no file at all exists, keeps our assumptions similar to the way I answered the adr questions, but they can always be remapped"
    via: "the maintainer, relayed by the steering session on 2026-11-07, on what the key mappings are with no vtium.conf"
  - evidence: "vtium at ee2883ec, read 2026-11-07, configuration: every vtium.conf key has its default in the key table, a grab key's as Default::Text or Default::Unset (crates/vtium-settings/src/keys.rs:1697-1745); the console reads a grab key's value with grab::parse (crates/vtium/src/config/value.rs:17), and a value it refuses is reported with its file and line (crates/vtium/src/config/mod.rs:142); the generated reference describes a grab key's value as key combinations, <Mod>...key, separated by commas (crates/vtium/src/config/doc.rs:26); vtium-config's Settings view lists every key vtium.conf takes, from the key table (crates/vtium-config/src/tui/model.rs:280-289), and edits a key that is not a switch or a choice as typed text on Enter (model.rs:1320-1351); vtium-config get and set read and write the same keys, and leave a value they do not check to the console's own check when the change is installed (crates/vtium-config/src/commands.rs:189-249); vtium-config edits vtium.conf's top level line by line through vtium-settings, so every comment and blank line stays (crates/vtium-config/README.md, the draft)"
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "hand some to the program, but also, Do other terminal emulators allow remapping of these?"
    via: "the maintainer's answer, relayed by the steering session on 2026-11-07, to the probe: the keys vtium takes for itself today stay vtium's while a program uses the protocol, and a program never sees them, pressed or released. Is that right?"
    covers: [vtium's keys stay vtium's]
  - operator: aaronsb
    said: "yes bound by default, but, again, could we make this configurable, or|and: what do other terminals offer?"
    via: "the maintainer's answer, relayed by the steering session on 2026-11-07, to the probe: Ctrl+Shift+Delete resets the shown console the way the reset command does, as a new grab-reset key that can be unset; lean yes, bound by default. Should vtium take it?"
    covers: [a key that undoes a stuck keyboard]
  - operator: aaronsb
    said: "also, vtium-config might be able to assist with configuration of configurables."
    via: the maintainer, relayed by the steering session on 2026-11-07, on how a user changes the vtium.conf keys this record adds
  - operator: aaronsb
    said: "again, the configuration file is where mappings could exist, and the assumption is one does not need to actually use vtim-config to sanely configure via the config file (one can just edit it by hand if wanted)"
    via: the maintainer, relayed by the steering session on 2026-11-07, clarifying where the key mappings live and vtium-config's part in them
  - operator: aaronsb
    said: "the same pattern we have for defaults, where if no file at all exists, keeps our assumptions similar to the way I answered the adr questions, but they can always be remapped"
    via: the maintainer, relayed by the steering session on 2026-11-07, on what the key mappings are with no vtium.conf
  - operator: aaronsb
    said: "Yes, the program gets them there (Recommended)"
    via: "the steering session's question tool, 2026-11-07: whether Shift with Up, Down, Page Up and Page Down go to the program while the alternate screen is shown, and keep scrolling on the main screen; the maintainer chose this option, whose wording is the steering session's"
    covers: [the scroll keys on the second screen]
  - operator: aaronsb
    said: "Yes, keep them fixed (Recommended)"
    via: "the steering session's question tool, 2026-11-07: whether Ctrl+Alt+F1 to F12 and the rescue key stay fixed with no vtium.conf key; the maintainer chose this option, whose wording is the steering session's"
    covers: [two keys stay fixed]
status: accepted
date: 2026-11-07
deciders:
  - aaronsb
related: []
---

# ADR-1005: The kitty keyboard protocol: vtium-term encodes keys by a stack of flags for each screen, and vtium-platform reports key releases and each key's other forms

## Summary

- **Decided:** vtium implements the kitty keyboard protocol from its published specification. A program opts in by pushing flags with `CSI > flags u`, and until it does, every key sends exactly the bytes it sends today. vtium-term keeps the flags and encodes every key. It keeps one stack of flags for the main screen and one for the alternate screen, each holding eight levels, and a push onto a full stack drops the oldest level. RIS and DECSTR clear both stacks, a switch between the screens leaves each stack as it is, and a new session starts with none. `CSI ? u` answers the flags in force, which never include a flag vtium does not implement. While the flags in force are not 0, the protocol decides every key's bytes, and modifyOtherKeys waits until they are 0 again. vtium-platform reports releases, the key's unshifted form, its key on the US PC-101 layout, and the NumLock, Hyper and Meta modifiers. Which keys vtium takes for itself does not depend on the protocol, but on the screen shown. The VT keys, the rescue key, the console and focus chords, the power keys, paste, zoom, rotate and a new reset key are vtium's on both screens. The scroll keys and the prompt jumps are vtium's while the main screen is shown, and they reach the program while the alternate screen is shown, where editors and other full-screen programs run. Every key vtium takes, apart from Ctrl+Alt+F1 to F12 and the rescue key, becomes a `grab-` key of vtium.conf, an ordinary line a person can read and edit by hand, so a user can bind it to another key or empty it, as every terminal surveyed allows. Each key's default is built into the key table, as every vtium.conf key's is, so with no vtium.conf, or one that sets none of these keys, vtium behaves as the maintainer's answers say. vtium-config's Settings view and its `get` and `set` commands edit the same keys, and hold nothing of their own. The reset key, `grab-reset`, is Ctrl+Shift+Delete by default and resets the shown console as the `reset` command does. A program that asked for releases gets one for every press it got. The protocol is in every build, minspec included.
- **Trades away:** A program that crashes on the alternate screen leaves its flags there, and the next program that uses the alternate screen inherits them, as in kitty, foot and Alacritty; the reset key is the way out. Shift with Up, Down, Page Up or Page Down no longer scrolls vtium's view while the alternate screen is shown. vtium.conf grows by seventeen keys. A key is handed to a program by a fixed rule for each key, not by a condition the user writes for each binding, as Ghostty's `unconsumed:` and kitty's `--when-focus-on` allow. The host's key report grows from four values to a structure, and the replay interface's `key` command gains fields. Keys with no xterm encoding, such as F21 to F35 and the media keys, still send nothing to a program that has not opted in, where kitty would send them as `CSI u`.
- **One-way?** No. A program receives the protocol only after it asks for it, and the query tells it which flags work, so each slice can widen what vtium accepts without breaking a program. The new vtium.conf keys default to today's keys, so a vtium.conf written today keeps working. The replay interface gains fields in a minor version, and every existing golden stays.
- **Probes:**
  - *Confident (vtium's keys stay vtium's):* the keys vtium takes for itself today stay vtium's while a program uses the protocol: Ctrl+Alt+F1 to F12, the console and focus chords (Ctrl+Super with an arrow, Return or BackSpace), the power keys when they are bound, Shift+Insert to paste, Shift with Up, Down, Page Up or Page Down to scroll, Ctrl with plus or minus to zoom, Super with plus or minus to rotate, and Ctrl+Shift with Up or Down to jump between prompts on the main screen. A program never sees them, pressed or released. Is that right? (Answered 2026-11-07: hand some to the program, and make them configurable as other terminals do; see `considered`, D7 and D10.)
  - *Not confident (a key that undoes a stuck keyboard):* Ctrl+Shift+Delete resets the shown console the way the `reset` command does, clearing its screen, its scrollback and every keyboard mode. That way a program that dies and leaves the keyboard sending escape codes for Enter can be undone from the keyboard. kitty binds the same key to its own reset by default. The key is a new `grab-reset` key in vtium.conf, which can be unset. Lean: yes, bound by default. Should vtium take it? (Answered 2026-11-07: yes, bound by default and configurable; see `considered` and D10.)
  - *Not confident (the scroll keys on the second screen):* while a full-screen program such as an editor shows the alternate screen, Shift with Up, Down, Page Up and Page Down reach the program instead of scrolling vtium's view, so an editor can use them to select text. Alacritty's defaults bind its scroll keys the same way, and vtium's prompt jumps already do. On the main screen they still scroll, so a fish prompt that uses the protocol keeps them. Zoom, rotate, paste and reset stay vtium's on both screens. Lean: yes. Is that the split you want?
  - *Confident (two keys stay fixed):* Ctrl+Alt+F1 to F12 and the rescue key stay fixed and get no vtium.conf key, because they are the way back to every other VT and to a keyboard's head (ADR-1401). A head whose program should receive them is already set `isolated`. Every other key vtium takes can be bound to another key or emptied. Lean: yes. Right?
- **Inversion:** The decision sits between "keys are xterm's bytes, and a program that wants to tell more keys apart has no way to ask", which is vtium today, and "every key the bytes cannot tell apart is sent as `CSI u` whether the program asked or not", which kitty's legacy mode does for keys with no legacy encoding and which fixterms proposed. vtium sends the protocol only to a program that asks, with the flags it asked for, and leaves every other program's bytes as they are. For vtium's own keys it sits between "the terminal keeps every shortcut", which kitty, WezTerm and Ghostty do by default, and "the program gets every key", which the Linux console's raw keyboard mode does; vtium keeps the keys that lead out of a program, and hands the keys a full-screen program reads to that program.

## Context

A program running in a terminal reads keys as bytes. In xterm's encoding, which vtium sends today, many keys share bytes: Escape is the first byte of every escape sequence, Ctrl+I is Tab, Ctrl+M is Enter, and Ctrl+Shift+R is Ctrl+R. No key sends anything when it is let go, and a modifier key pressed alone sends nothing. Editors such as Neovim, Helix and Kakoune, shells such as fish and nushell, and libraries such as crossterm and notcurses use the kitty keyboard protocol to tell every key and its release apart. ADR-1000 lists the protocol as the row `input.kitty-keyboard`, planned, and the capability probe `kitty-keyboard` is listed as missing. vtium already supports xterm's modifyOtherKeys (`input.modify-other-keys`), and it ignores fixterms (`input.fixterms`), the earlier proposal the kitty protocol replaces.

The protocol is progressive. A program sets flags, and each flag adds to what the terminal reports:

| Flag | Name in the specification | What the program gets |
|---|---|---|
| 1 | Disambiguate escape codes | Escape, and keys held with Alt or Ctrl, as `CSI number ; modifiers u`, and the keypad's keys as keys of their own; Enter, Tab and Backspace keep their bytes |
| 2 | Report event types | Repeats and releases, marked in the modifiers field |
| 4 | Report alternate keys | The shifted key and the key on the US PC-101 layout, beside the key's own code |
| 8 | Report all keys as escape codes | Every key, text keys and modifier keys included, as an escape code, and no text |
| 16 | Report associated text | The text a key makes, inside its escape code |

The specification leaves several things to the terminal: how deep the stacks go, what RIS and DECSTR do to them, what a switch between screens does, and what happens when a program exits without popping. It also leaves open which keys the terminal keeps for itself. kitty's source code is GPL-licensed, and vtium never reads it (CONTRIBUTING.md). So these answers come from the specification, from kitty run as a program, from Alacritty run the same way, and from foot's source code, which is MIT-licensed (`basis`).

How the three terminals answer the open questions:

| Question | kitty 0.49.2, run | foot at 152487e, source read | Alacritty 0.17.0, run |
|---|---|---|---|
| Levels kept on each screen | 8, the oldest dropped | 8, the oldest overwritten | at least 300 |
| Flags read back after a push of 255 | 127 | 31 | 31 |
| The alternate screen's flags after leaving it and coming back | kept | kept | kept |
| DECSTR | both screens set to 0 | both screens set to 0 | no change |
| RIS | both screens set to 0 | both screens set to 0 | both screens set to 0 |

### How terminals let a user change their own keys

The maintainer asked whether other terminals let a user remap the keys they take for themselves, and what they offer. Every terminal surveyed lets a user bind each of its own keys to another key or remove it. The sources are each terminal's published documentation, read on 2026-11-07, listed in `basis`. No terminal's source code was read for this table.

| Terminal | How a user changes or removes one of its keys | Its keys while a program uses the kitty protocol or another full-key mode | A key that resets the terminal | Its scroll, prompt, font size and paste keys by default |
|---|---|---|---|---|
| kitty | `map ctrl+shift+t new_tab` binds a key; `map ctrl+shift+t` with no action removes it and passes the key to the program; `discard_event` drops a key; `clear_all_shortcuts yes` removes every default; `kitty_mod` changes the modifier of every default at once | kitty keeps taking them. `map --when-focus-on title:...` removes a key only for the windows that match, and the documentation's example shows Ctrl+Shift+T reaching a program that uses the protocol only after such a line | Ctrl+Shift+Delete, bound by default | Ctrl+Shift with Up, Down, Page Up, Page Down, Home and End; Ctrl+Shift+Z and X; Ctrl+Shift with Equal, Plus, Minus and Backspace; Ctrl+Shift+V and Shift+Insert |
| foot | `[key-bindings]` with `action=combo...`; `action=none` removes an action's keys; `noop=` lists keys never sent to the program | not documented | none; it documents only `font-reset` | Shift+Page_Up and Shift+Page_Down; Control+Shift+z and x; Control with plus, equal, minus and 0; Control+Shift+v and Shift+Insert |
| WezTerm | `config.keys` entries; the action `DisableDefaultAssignment` removes one default, and `disable_default_key_bindings = true` removes them all | WezTerm keeps taking them: input that matches no key assignment is encoded for the program. It honours the kitty protocol only with `enable_kitty_keyboard = true` | the action `ResetTerminal`, which sends RIS, on no key by default | Shift+PageUp and PageDown; Ctrl or Super with -, = and 0; Ctrl+Shift+V and Shift+Insert |
| Ghostty | `keybind = trigger=action`; `unbind` removes a binding and sends the key to the program; `ignore` drops the key; `keybind = clear` removes every binding | a binding takes the key by default. Per binding, `unconsumed:` sends the key to the program as well, and `performable:` takes it only when the action can act, as `performable:ctrl+c=copy_to_clipboard` copies only while there is a selection | the action `reset`, "equivalent to running the reset command"; the pages do not say whether a key is bound to it | not listed in the online pages; `ghostty +list-keybinds --default` prints them |
| Alacritty | `[keyboard] bindings` entries with `key`, `mods`, `mode` and an action; the action `ReceiveChar` removes a default, and `None` makes a key do nothing; `mode` makes a binding depend on a terminal mode, and `~` negates the mode | not documented | none among its actions | Shift with PageUp, PageDown, Home and End, each with the mode `~Alt`, which reads as "not on the alternate screen"; Ctrl with 0, =, + and -; Shift+Insert |
| xterm 411 | the X resource `*VT100*translations: #override ...` rebinds keys, and `omitTranslation` leaves out parts of the defaults | xterm has no full-key mode | Soft Reset and Full Reset are entries of a menu, on no key | Shift+Prior and Shift+Next scroll half a page; Shift+KP_Add and Shift+KP_Subtract change the font; Shift+Insert pastes |
| The Linux console | `loadkeys` reads keymaps with lines such as `shift keycode 107 = Scroll_Backward` and `alt keycode 105 = Decr_Console` | a program that sets the keyboard's raw or keycode mode receives every key, the console switching keys included, because only the translating modes use the keymap | Alt+SysRq+R returns a raw keyboard to the translating mode, and Alt+SysRq+K kills every program on the console, when `/proc/sys/kernel/sysrq` allows them | Shift+PageUp scrolls back in the documentation's example keymap |

Two patterns stand out. A terminal's keys are always the user's to change, and kitty, WezTerm, Ghostty and Alacritty document that removing a binding sends the key on to the program. While a program uses a full-key mode, the terminals that document it keep their keys by default and let the user hand a key over, per window in kitty and per binding in Ghostty. Alacritty's defaults hand the scroll keys over by screen, and the Linux console hands every key over in raw mode, but keeps SysRq as the way back.

vtium takes these keys today:

| Key | What it does | Can the user change it today? |
|---|---|---|
| Ctrl+Alt+F1 to F12, XF86Switch_VT_1 to 12 | switch the VT (ADR-1401), from a head that is not `isolated` | no |
| The shown VT's own Ctrl+Alt+F key | the rescue key (ADR-1401 D9) | no |
| Ctrl+Super with Right, Left, Return and BackSpace | the console chords | no |
| Ctrl+Super with Up and Down | the focus chord, from a roaming keyboard | no |
| `grab-reboot`, `grab-power-off`, `grab-suspend` | the power keys | yes, unset by default |
| Shift+Insert | paste | yes, `grab-paste` |
| Shift with Up and Down, Shift with Page Up and Page Down | scroll the view by a line and by a page | no |
| Ctrl+Shift with Up and Down | jump between prompts, on the main screen only (ADR-1002) | no |
| Ctrl with plus or equals, Ctrl with minus | zoom | no |
| Super with plus or equals, Super with minus | rotate | no |

Reading vtium's input path for this record turned up a gap. vtium-term's README says the host adds NumLock to a key's modifiers for mode 1035, but the binary passes vtium-platform's modifier bits unchanged, and those have no NumLock bit, so mode 1035 never sees NumLock on (`basis`). The protocol needs the same bit, and D8 adds it.

## Decision

The figure shows the path a key takes. vtium-platform reads the key, and the binary offers it to each key vtium keeps for itself in turn. A scroll key or a prompt jump is vtium's only while the main screen is shown. A key that vtium does not take reaches vtium-term's encoder, which reads the flags in force on the shown screen.

```mermaid
flowchart TD
    DEV["A keyboard (evdev)"] --> PLAT["vtium-platform, through libxkbcommon:<br>press, repeat or release; the keysym after Shift;<br>the unshifted key; the key on the US PC-101 layout;<br>the text; the modifiers"]
    PLAT --> VT{"Ctrl+Alt+F1 to F12 from a head<br>that is not isolated,<br>or the rescue key?<br>(fixed)"}
    VT -- "yes" --> TAKEN["vtium acts on it.<br>The program sees neither<br>the press nor the release."]
    VT -- "no" --> BOTH{"A key vtium.conf binds to a power key,<br>a console or focus chord, paste,<br>zoom, rotate or reset?"}
    BOTH -- "yes" --> TAKEN
    BOTH -- "no" --> MAIN{"A key vtium.conf binds to scrolling<br>or a prompt jump?"}
    MAIN -- "yes, and the main<br>screen is shown" --> TAKEN
    MAIN -- "no, or the alternate<br>screen is shown" --> FLAGS{"The flags in force<br>on the shown screen"}
    FLAGS -- "0" --> XTERM["xterm's encoding,<br>with modifyOtherKeys when set,<br>as today; no releases"]
    FLAGS -- "not 0" --> KITTY["The kitty protocol's encoding,<br>for exactly the flags in force"]
    XTERM --> PTY["The program, through the pty"]
    KITTY --> PTY
    classDef dev fill:#475569,stroke:#94a3b8,color:#ffffff
    classDef plat fill:#2d7d9a,stroke:#4a5568,color:#ffffff
    classDef ask fill:#fbbf24,stroke:#4a5568,color:#1a1a1a
    classDef vtium fill:#f6821f,stroke:#4a5568,color:#1a1a1a
    classDef enc fill:#7c3aed,stroke:#4a5568,color:#ffffff
    classDef out fill:#2d8e5e,stroke:#4a5568,color:#ffffff
    class DEV dev
    class PLAT plat
    class VT,BOTH,MAIN,FLAGS ask
    class TAKEN vtium
    class XTERM,KITTY enc
    class PTY out
```

### D1: where the encoding lives

vtium-term encodes every key, in both encodings. It already holds the xterm encoding, modifyOtherKeys and every mode that changes a key's bytes, and the flags are terminal state that a program sets through the pty, as those modes are. Keeping the encoder in vtium-term also lets tier 1 replay a key and check its bytes with no keyboard.

vtium-platform supplies the key events, because it alone reads the keyboard and the keymap. The binary turns each platform event into vtium-term's own description of a key and decides, through the key order (D7), whether a key reaches vtium-term at all. vtium-term never depends on vtium-platform: the binary fills vtium-term's key description from the platform's event, as it fills the four values `Terminal::key` takes today.

### D2: what the host tells vtium-term about a key

`Terminal::key` takes one description of a key, in place of the four values it takes today. The description holds:

- whether the key was pressed, repeated or released;
- the keysym the key makes with the modifiers held, as today;
- the keysym the key makes with no modifier at all in the active layout, which is the unshifted key;
- the key's character on the US PC-101 layout, which is the base layout key;
- the ASCII layout's keysym, which Ctrl reads in xterm's encoding, as today;
- the text the key makes, after compose, as today;
- the modifiers held, with the effect of this event included.

vtium-term's modifier bits gain Hyper and Meta beside Shift, Lock, Ctrl, Alt, Super and NumLock. vtium-term maps them to the protocol's bits (shift 1, alt 2, ctrl 4, super 8, hyper 16, meta 32, caps_lock 64, num_lock 128) only where it encodes a key. When the flags in force are 0, vtium-term sends nothing for a release, so a host that reports releases changes nothing for a program that has not opted in.

### D3: the flag stacks

Each screen keeps its own stack of flags. The top of the main screen's stack is in force while the main screen is shown, and the top of the alternate screen's stack is in force while the alternate screen is shown. A stack holds at most eight sets of flags: the set in force and seven saved sets. The bound is a row of vtium-term's bounds table, so a stream of pushes cannot grow the terminal's memory.

- `CSI > flags u` saves the flags in force and puts `flags` in force. A push with no number puts 0 in force. When the stack already holds eight sets, the oldest saved set is dropped, as the specification requires.
- `CSI < number u` pops `number` sets, 1 when the number is left out. The newest saved set that remains comes back into force. A pop of more sets than are saved leaves the stack empty and the flags 0, as the specification says a pop that empties the stack does. `CSI < 0 u` pops nothing, as kitty 0.49.2 does.
- `CSI = flags ; mode u` changes the flags in force and saves nothing. Mode 1, or no mode, sets exactly `flags`. Mode 2 sets the bits `flags` holds and leaves the others. Mode 3 clears the bits `flags` holds and leaves the others. Any other mode changes nothing.
- A flag bit vtium does not implement is dropped as it arrives, by a push or a set alike. The specification documents five bits, 1 to 16, so vtium keeps at most 31. While the work is in slices (D13), vtium also drops the bits of each flag it does not implement yet.

The eight levels follow kitty 0.49.2 and foot, which keep eight. The specification asks the terminal to bound the stack and gives no number, and eight levels cover a program that pushes inside a program that pushed, several times over.

### D4: what resets the flags

- **RIS** sets both screens' stacks to empty and their flags to 0. RIS returns the terminal to its initial state, and kitty, foot and Alacritty all clear the flags on RIS. vtium's terminfo entry sends RIS as its reset string, so the `reset` command clears them.
- **DECSTR** does the same as RIS to both stacks. vtium's DECSTR already resets modifyOtherKeys and the other modes that change a key's bytes, and kitty and foot clear the flags on DECSTR too.
- **A switch between the screens**, by mode 47, 1047 or 1049, changes neither stack. The program on the alternate screen and the program on the main screen each keep their own flags, which is the reason the specification asks for two stacks. kitty, foot and Alacritty all keep the alternate screen's flags when a program leaves it and enters it again.
- **A program that exits without popping** leaves its flags in force on its screen, because the terminal cannot see a program inside the session end. The flags stay until a program pops or sets them, or until RIS or DECSTR, or until the session ends.
- **A new session** starts from RIS (`crates/vtium/src/head/session.rs`), so flags never pass from one session to the next.
- Each console of each head has its own terminal, so its own two stacks.

### D5: the query and the reply rule

`CSI ? u` is answered with `CSI ? flags u`, where `flags` is the flags in force on the shown screen, written as a decimal number. The reply is a number vtium re-encodes from its own state, so it keeps the reply rule (docs/architecture.md, "Determinism"). It is written under a `Query` variant of its own, so the reply rule's test sends hostile text through it with every other query.

A program finds out whether vtium supports the protocol by sending `CSI ? u` and then DA1, as the specification says. vtium answers both, the protocol's reply first. Because a flag vtium does not implement is dropped as it arrives (D3), a program that sets flags and then asks reads back exactly the flags vtium honours, as the specification's note on partial implementations asks.

### D6: which encoding a key gets, and modifyOtherKeys

- **While the flags in force are 0**, every key sends what it sends today: xterm's encoding, with modifyOtherKeys at the level a program set, and with DECCKM (mode 1), the keypad modes (DECKPAM and mode 66) and modes 1034, 1035 and 1036 as they stand. A functional key with no xterm encoding, such as F21 or a media key, sends nothing, as today. The specification allows a terminal to ignore such keys in legacy mode, and ADR-1000 ignores fixterms, whose `CSI u` encoding the protocol's legacy mode extends.
- **While the flags in force are not 0**, the specification decides every key's bytes, for exactly the flags in force. Where a flag the program set does not cover a key, the specification's own legacy section decides it, and DECCKM applies only where that section names cursor key mode. modifyOtherKeys and modes 1034, 1035 and 1036 are not consulted. The host's Backspace setting still chooses DEL or BS for Backspace's legacy byte, which the specification lets a program learn from terminfo's `kbs`.
- **modifyOtherKeys keeps its own state.** `CSI > 4 ; n m` still sets its level while the flags are not 0, and `CSI ? 4 m` still reports that level. The level takes effect again once the flags in force are 0. foot chooses between its two encodings the same way, by whether the flags in force are 0.

### D7: the keys vtium keeps, and the keys it hands to the program

The maintainer asked that vtium hand some of its keys to the program (`considered`, 2026-11-07). The protocol itself changes only what reaches the encoder. Which keys vtium takes depends on the screen shown, not on the flags, because a shell such as fish sets flags at its own prompt on the main screen, and a rule keyed to the flags would take scrolling away from that prompt. The key order stays as it is: the VT's keys, then the power keys, then the console and focus chords, then the paste key, then the grabs, then DECARM, then the encoder (`crates/vtium/src/app.rs`). The reset key (D10) joins the grabs.

| Key | While the main screen is shown | While the alternate screen is shown |
|---|---|---|
| Ctrl+Alt+F1 to F12 and the XF86Switch_VT keys | vtium switches the VT, from a head that is not isolated (ADR-1401) | the same |
| The rescue key, the shown VT's own Ctrl+Alt+F key | vtium (ADR-1401 D9) | the same |
| The console chords, and the focus chord from a roaming keyboard | vtium | the same |
| The power keys, paste, zoom, rotate and reset | vtium | the same |
| Scroll by a line and by a page | vtium scrolls its view | the program |
| The prompt jumps | vtium (ADR-1002) | the program (ADR-1002) |

The keys vtium keeps on both screens are the keys that lead out of a program or change the console itself. kitty, WezTerm and Ghostty also keep their keys while a program uses the protocol, and the Linux console keeps SysRq even in raw mode. The scroll keys move to the program on the alternate screen because editors and other full-screen programs run there and read Shift with the arrows and Page Up and Page Down, for example to select text. Alacritty's default scroll bindings carry the mode `~Alt` for the same reason, and ADR-1002 already hands the prompt jumps to the program there. From a keyboard of an isolated head, the VT keys reach the session as keys and so the encoder, and from a claimed keyboard the focus chord does, both as ADR-1401 decides. A user who wants another split binds a key elsewhere or empties it (D10).

A key vtium takes is vtium's for its whole life. vtium remembers which keys of each keyboard it took, and it takes their repeats and their releases as well, so a program never sees a release whose press it did not see.

A program that asked for releases gets one for every press it got. The binary remembers, for each console, the keys it delivered there and that are still held. When the console stops receiving the keyboard while such a key is held, the binary sends the console a release for each of them. That happens when the VT is switched away, when a console chord shows another console, when the focus chord moves the keyboard to another head, and when the keyboard is unplugged. vtium-term encodes these releases only while flag 2 is in force, as it does any release. The specification does not say what a terminal does with keys held when it loses the keyboard. Without this rule, a program that saw Ctrl pressed before a VT switch would believe Ctrl is still held when the VT comes back, because vtium-platform starts a fresh keyboard state when it wakes.

### D8: releases, repeats and the modifiers, from vtium-platform

- **Releases.** A keyboard reports a release for every key whose press it reported. A key whose press went into a compose sequence reports no release, because no press was reported for it. Today the platform reports presses only, and a release only stops the repeat.
- **Repeats.** vtium's own repeat timer keeps making the repeats, marked as repeats, as today. DECARM still decides whether the binary sends them on, so a program that turned repeat off with mode 8 gets no repeat events.
- **NumLock.** The platform reports NumLock among the modifiers while it is on. This also gives mode 1035 the NumLock state that vtium-term's README says the host adds.
- **Hyper and Meta.** The platform reports Hyper and Meta when the keymap gives them a modifier of their own, one that neither Alt nor Super uses. In xkeyboard-config's default keymaps, Meta shares Alt's modifier and Hyper has none, so there the platform reports Alt and Super alone, and a program never sees Meta set beside every Alt. The slice that builds this proves it against the libxkbcommon vtium links, with a keymap that gives Hyper a modifier of its own.
- **The modifier key's own event.** The platform reads a key's modifiers after xkb's state has taken the key, so pressing Ctrl reports Ctrl set and releasing it reports Ctrl clear, as the specification requires. That is how the platform builds its events today.

### D9: the unshifted key and the base layout key

- **The unshifted key** is the keysym the key makes at the first shift level of the active layout. vtium-term sends it as the key's code: Ctrl+Shift+A is sent with the code 97, `a`. When a modifier the protocol cannot encode chose the key's symbol, such as AltGr or the level-five shift, the platform reports no unshifted key, and vtium-term sends the symbol the key made. Otherwise AltGr+Q, which makes `@` on a German layout, would be sent as Q with no modifier, and a program could not tell it from Q. foot follows the same rule.
- **The base layout key** comes from a fixed table in vtium-platform, from the kernel's key code to the character the key types on the US PC-101 layout. The kernel names its key codes after their places on that keyboard, `KEY_Q` and `KEY_A` among them, so the table gives exactly the key the specification describes, whatever layout the user has configured. On a Russian layout, Ctrl+С is sent with the base layout key 99, `c`. A key the PC-101 layout lacks, such as the extra key of ISO keyboards, has no base layout key. vtium-term sends the base layout key only under flag 4, and only where it differs from the key's code.
- **The shifted key** is the keysym the key makes with the modifiers held. vtium-term sends it under flag 4 only while Shift is held, as the specification says.

### D10: vtium's own keys in vtium.conf, the reset key among them

The maintainer asked whether other terminals let a user remap these keys, and asked that the reset key be configurable (`considered`, 2026-11-07). Every terminal surveyed in Context lets a user bind its keys elsewhere or remove them. So every key vtium takes, apart from the VT keys and the rescue key, is a key of vtium.conf of the kind `grab-paste` already is. Its value is a list of key combinations, `<Mod>...key` separated by commas, as `docs/config.md` describes for `grab-paste`. Its default is the key vtium takes today, so a console with no vtium.conf behaves as it does today, apart from D7's alternate screen. An empty value takes no key for that action, and the key reaches the program.

**Where the mappings live, and who edits them.** The mappings live in vtium.conf and nowhere else, as ordinary top-level keys, one line each, such as `grab-reset = <Ctrl><Shift>Delete` or `grab-scroll-up =` to hand Shift+Up to every program. A person reads and edits them in any text editor. `crates/vtium/install/vtium.conf.example` carries each one with the comment the key table's description generates, as it does every other key. Each default is the key's built-in default in the key table (`crates/vtium-settings/src/keys.rs`), as every vtium.conf key gets its default, so the defaults need no file at all: with no vtium.conf, or a vtium.conf that sets none of these keys, vtium takes the keys D7 lists and resets the shown console on Ctrl+Shift+Delete, as the maintainer answered. A file can bind any of them elsewhere or empty it. vtium-config is an optional editor of the same keys. Its Settings view lists every key vtium.conf takes from the key table, so the new keys appear there with no change to vtium-config, and Enter edits a key's value as text; `vtium-config get` and `vtium-config set` read and write them from the command line. vtium-config stores nothing of its own about them, and vtium behaves the same whether a person or vtium-config wrote the file. `vtium check-config`, and the console when it starts, report a bad mapping with the file and the line it is on, as they report every bad value today: an unknown modifier or key name, a combination that a fixed key or another mapping already takes, and a line that tries to bind a fixed key.

| vtium.conf key | Default | What the key does |
|---|---|---|
| `grab-scroll-up`, `grab-scroll-down` | `<Shift>Up`, `<Shift>Down` | scroll the view by a line, main screen only (D7) |
| `grab-page-up`, `grab-page-down` | `<Shift>Prior`, `<Shift>Next` | scroll the view by a page, main screen only (D7) |
| `grab-prompt-up`, `grab-prompt-down` | `<Ctrl><Shift>Up`, `<Ctrl><Shift>Down` | jump between prompts, main screen only (ADR-1002) |
| `grab-zoom-in`, `grab-zoom-out` | `<Ctrl>plus,<Ctrl>equal`, `<Ctrl>minus` | zoom |
| `grab-rotate-cw`, `grab-rotate-ccw` | `<Logo>plus,<Logo>equal`, `<Logo>minus` | rotate |
| `grab-reset` | `<Ctrl><Shift>Delete` | reset the shown console |
| `grab-console-next`, `grab-console-previous` | `<Ctrl><Logo>Right`, `<Ctrl><Logo>Left` | show the head's next or previous console (ADR-1401) |
| `grab-console-open`, `grab-console-close` | `<Ctrl><Logo>Return`, `<Ctrl><Logo>BackSpace` | open or close a console, under an open policy (ADR-1401) |
| `grab-focus-next`, `grab-focus-previous` | `<Ctrl><Logo>Up`, `<Ctrl><Logo>Down` | move the focus between heads, from a roaming keyboard (ADR-1401) |

`grab-paste`, `grab-reboot`, `grab-power-off` and `grab-suspend` stay as they are. ADR-1401 still decides what each chord does and when it acts; this record makes only its key configurable, and the notices that name a chord name the key vtium.conf binds. `vtium check-config` reports a key combination that two of these keys bind, and vtium then gives the combination to the key that comes first in the key order.

**The reset key.** `grab-reset`, bound by default as the maintainer answered, resets the shown console as RIS does: both flag stacks, modifyOtherKeys and every other mode return to their initial state, and the screen and the scrollback are cleared. It is a grab, matched with the other grabs, on both screens. The key exists because a program that dies with flag 8 set leaves a shell that receives every key as an escape code, Enter and Ctrl+C included. The specification keeps Enter, Tab and Backspace as plain bytes under flags 1 and 2 so that a user can type `reset`, but flag 8 takes them too. On a desktop, the user closes the window. On the console there is no window, and the chords only lead to another console. kitty binds the same key to its own reset by default, and Ghostty and WezTerm offer a reset action that a user binds to a key.

**The VT keys and the rescue key stay fixed.** They get no vtium.conf key. They are the way back to every other VT and the way to win a keyboard back to its head (ADR-1401 D9), and a vtium.conf that emptied them could leave a user with no way off a console. A head whose program should receive the VT keys is set `isolated`, as ADR-1401 decides.

**Not decided here: a condition for each binding.** Ghostty's `unconsumed:` and `performable:`, kitty's `--when-focus-on` and Alacritty's `mode` let a user say, for each binding, when the terminal hands the key over. vtium's rule is fixed for each key (D7). A later decision adds such conditions if users need a key handed over in a case D7 does not cover.

**Not decided here: pressing a key to bind it in vtium-config.** Typing `<Ctrl><Shift>Delete` works in vtium-config today. A Settings field that records the combination as the user presses it would be easier, but it must read keys that this terminal's own encoding may hide, such as Ctrl+Shift with a letter, and so it needs the protocol this record adds. A later decision for vtium-config designs that field, once slice 3 has landed.

### D11: in every build

The protocol is in every build, minspec included, with no cargo feature. Its state is about sixteen bytes for each terminal, eight levels of one byte for each screen, and its code is the encoder and the specification's table of more than a hundred functional keys. The program that most needs to tell Escape from the start of an escape sequence, an editor, runs on a small machine as on a large one. The first slice measures how much the minspec binary grows and records it in vtium-term's README.

### D12: ADR-1000's rows and the replay interface

- `input.kitty-keyboard` moves from planned to partial when the first slice lands, and to supported when the last one does. Each change is an edit to ADR-1000's table, in the slice that earns it. The capability probe `kitty-keyboard` moves to works in the first slice, which answers `CSI ? u`.
- `input.fixterms` stays ignored, and its test `no_key_sends_fixterms_csi_u` keeps passing: with the flags at 0, no key sends `CSI u`.
- The replay interface's `key` command gains fields for the event type, the unshifted key, the base layout key and the text, and its `mods` range grows to take NumLock, Hyper and Meta. These are additions, so the interface's minor version rises and every existing golden stays as it is.

### D13: the work, in slices

Each slice is a card of its own, branched from `integration`, and lands in this order. Each names what it proves.

1. **The stacks, the query and flag 1** (vtium-term, the replay interface, ADR-1000's row). The two stacks with their bound, push, pop and set, the resets, the reply, the key description of D2 filled from what the host reports today, and flag 1. Bits 2, 4, 8 and 16 are dropped as they arrive. Proves: a program that pushes 1 gets Escape, Alt and Ctrl keys and the keypad in the forms the specification gives; the query reports 1 after a push of 31; each screen keeps its own flags across a switch; RIS and DECSTR clear both; a ninth push drops the oldest level; every existing input test passes unchanged, so the flags at 0 change no byte; modifyOtherKeys is not consulted while the flags are not 0 and works again once they are 0; the minspec binary's growth, measured.
2. **Releases, repeats and the modifiers** (vtium-platform, the vtium binary, vtium-term, the replay interface). Release events from the platform, the NumLock, Hyper and Meta modifiers, the binary's taken keys and its releases on leaving (D7), and flag 2. Proves: every press a program got is matched by exactly one release, across a VT switch, a console chord, a focus move and an unplugged keyboard; a key vtium took never reaches the program, press or release; Enter, Tab and Backspace send no release under flag 2 alone; mode 1035 sees NumLock; a keymap that gives Hyper a modifier of its own reports it, and the default keymap reports neither Hyper nor Meta.
3. **The key's other forms** (vtium-platform, vtium-term, the replay interface). The unshifted key, the base layout key table, the shifted key, and flags 4, 8 and 16. Proves: Ctrl+С on a Russian layout carries the base layout key 99; Ctrl+Shift+A is sent with the code 97 and, under flag 4, the shifted key 65; AltGr+Q on a German layout is sent as `@`; under flag 8 every key is an escape code, modifier keys and their releases included; under flag 16 the text arrives inside the code and holds no control character. With this slice `input.kitty-keyboard` is supported.
4. **vtium's keys in vtium.conf, the reset key and the screen rule** (vtium-settings' key table, the vtium binary, the generated `docs/config.md` and `vtium.conf.example`). The seventeen keys of D10 with today's keys as defaults, the chord notices that name the bound key, check-config's report of a combination bound twice, `grab-reset`, and D7's rule for the scroll keys. It can land before slice 2 or after it, since it reads only the key order. Proves: with no vtium.conf, every key does on the main screen what it does today; on the alternate screen Shift+Up, Shift+Page Up and the prompt jumps reach the program and on the main screen they scroll; a key bound elsewhere acts on its new combination, and its old combination reaches the program; an emptied key reaches the program; a combination bound twice is reported, and the first key in the key order takes it; Ctrl+Shift+Delete resets the shown console and no other, under flag 8 too; a chord's notice names the key vtium.conf binds; Ctrl+Alt+F1 to F12 and the rescue key cannot be bound or emptied; a bad mapping is reported with its file and line; a vtium.conf edited by hand and the same file written by `vtium-config set` give the same keys; vtium-config's Settings view lists the seventeen keys with their built-in defaults.

A case the specification leaves unclear while a slice is built is settled by running kitty as a program, and the crate's README records what kitty did. Keys reach kitty under Xvfb through XTEST only once its window has the keyboard focus, which this record's runs did not achieve.

## Consequences

### Positive

- Editors and shells built for the protocol tell Escape from Alt, Ctrl+I from Tab, and every modifier combination apart on the console, and games and editors that need releases get them.
- A program that has not opted in sees no change at all, so every existing input test and golden keeps passing.
- The flags never outlive a session, and `reset` clears them.
- Mode 1035 gets the NumLock state it was written for.
- A Russian, Greek or other non-Latin layout keeps Ctrl shortcuts working in programs that match the base layout key.
- Editors on the alternate screen receive Shift with the arrows and Page Up and Page Down, under the protocol and under xterm's encoding alike.
- A user can move or remove every key vtium takes, apart from the two that lead off a console, and a stuck keyboard has a key that undoes it.

### Negative

- `Terminal::key` changes its signature, and every caller and test that sends a key changes with it.
- The binary keeps a small record of held keys for each console and of the keys it took, and it must send releases at every point where a console stops receiving the keyboard. A missed point leaves a program believing a key is held.
- A program that leaves flag 8 set on the alternate screen leaves the next full-screen program receiving escape codes too, until a reset.
- A user who scrolled vtium's view with Shift+Page Up while a full-screen program was shown loses that, unless the program itself scrolls.
- vtium.conf and its generated reference grow by seventeen keys, and the chord notices read the key from the configuration.

### Neutral

- vtium-platform's key events grow by the event type, the unshifted keysym and the base layout key, and its modifiers by three bits.
- The replay interface's minor version rises.
- The terminfo entry gains nothing: no terminfo capability describes the protocol, and a program detects it by its query.

## Alternatives Considered

- **Encode in the binary, or in vtium-platform.** Rejected: the flags are terminal state that the pty sets, and the encoder needs the modes vtium-term already holds; tier 1 could no longer check a key's bytes.
- **Clear the alternate screen's flags whenever a program enters the alternate screen.** Not taken: it would spare the next full-screen program a crashed program's flags, but kitty, foot and Alacritty all keep them, and a program that follows the specification pushes after it enters the alternate screen and pops before it leaves. The reset key (D10) answers the crash instead.
- **Keep DECSTR away from the flags, as Alacritty does.** Rejected: vtium's DECSTR resets every other mode that changes a key's bytes, and kitty and foot clear the flags on DECSTR.
- **A deeper stack, as Alacritty keeps.** Rejected: eight levels are what kitty and foot keep, and every level is memory a program's output controls.
- **Keep the newest pushes and drop the push that overflows, as vtium's color stack does with xterm.** Rejected: the specification says a push onto a full stack evicts the oldest entry.
- **Send keys with no xterm encoding as `CSI u` even with the flags at 0, as kitty does.** Rejected: a program that has not opted in might not parse them, and ADR-1000 ignores fixterms, whose encoding this is. The specification allows either.
- **Take the base layout key from the first layout the user configured, as foot does.** Rejected: a user with only a Russian layout would get a Cyrillic base layout key, and Ctrl+C shortcuts would fail in the programs the key exists for. The kernel's key codes already name the PC-101 keys.
- **Hand vtium's keys to the program while the flags are not 0.** Rejected: fish sets flags at its own prompt on the main screen, so its user would lose scrolling there, and the flags say nothing about whether a program reads a given key.
- **Keep every key vtium takes today, on both screens.** Not taken: the maintainer asked that some keys go to the program, and editors on the alternate screen read the scroll keys.
- **A condition for each binding, as Ghostty's `unconsumed:` and `performable:` and kitty's `--when-focus-on` give.** Deferred (D10): it widens the configuration language, and the rule for each key in D7 covers the programs in view.
- **vtium.conf keys for the VT keys and the rescue key too, as the Linux console's keymaps allow.** Rejected: a vtium.conf that emptied them could leave a user with no way off a console, and an isolated head already hands the VT keys to its program.
- **Keep the chords fixed, and make only the grabs configurable.** Not taken: the maintainer's question covered the chords too, and their notices already read their names from the one table that binds them.
- **Watch the pty's foreground process group, and clear the flags when the shell gets the terminal back.** Rejected: no specification describes it, the terminal cannot tell a program that exited from one that was suspended, and a shell such as fish sets its own flags at each prompt.
- **Put the protocol behind a cargo feature, as right-to-left text is.** Rejected: its cost is small and measured in the first slice, and the programs that use it run on small machines too.
