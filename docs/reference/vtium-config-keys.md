<!-- Generated from ADR-1403's interface tree and key table by the test generated_help_and_page_match_the_contract in crates/vtium-config/src/tui/contract.rs. VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config --lib generated_help rewrites it. -->

# vtium-config's screens and keys

vtium-config's TUI is its full-screen view of the configuration, which `vtium config tui` opens. This page lists every screen of the TUI and what each key does there. The TUI's Help tab and its key lines come from the same two tables as this page, so they say the same. [Configure vtium](../configure.md#the-tui) explains what the TUI is for.

The diagram shows how the screens open from one another. A dashed box is a screen that is not built as described yet. Its section links the issue that tracks it, and says what vtium-config has today.

```mermaid
flowchart LR
    tui["tui"]
    flow["flow"]
    flow_detail["flow.detail"]
    flow_detail_typing["flow.detail.typing"]
    flow_detail_choices["flow.detail.choices"]
    flow_info["flow.info"]
    flow_card["flow.card"]
    flow_card_list["flow.card.list"]
    flow_card_typing["flow.card.typing"]
    flow_new_card["flow.new-card"]
    flow_maker["flow.maker"]
    flow_maker_picker["flow.maker.picker"]
    flow_maker_picker_typing["flow.maker.picker.typing"]
    flow_others["flow.others"]
    flow_file["flow.file"]
    flow_restore["flow.restore"]
    looks["looks"]
    looks_look["looks.look"]
    looks_look_list["looks.look.list"]
    looks_look_typing["looks.look.typing"]
    looks_font_set["looks.font-set"]
    looks_pointer["looks.pointer"]
    looks_new_frit["looks.new-frit"]
    looks_new_frit_typing["looks.new-frit.typing"]
    looks_new_frit_list["looks.new-frit.list"]
    fonts["fonts"]
    fonts_filter["fonts.filter"]
    fonts_font["fonts.font"]
    effects["effects"]
    effects_filter["effects.filter"]
    effects_effect["effects.effect"]
    settings["settings"]
    settings_section["settings.section"]
    settings_section_typing["settings.section.typing"]
    settings_section_list["settings.section.list"]
    help["help"]
    any["any"]
    any_question_quit["any.question.quit"]
    any_question_leave["any.question.leave"]
    any_question_confirm["any.question.confirm"]
    any_question_deactivate["any.question.deactivate"]
    any_question_system_font["any.question.system-font"]
    any_question_window_ops["any.question.window-ops"]
    any_trial["any.trial"]
    any_too_small["any.too-small"]
    tui --> flow
    flow --> flow_detail
    flow_detail --> flow_detail_typing
    flow_detail --> flow_detail_choices
    flow --> flow_info
    flow --> flow_card
    flow_card --> flow_card_list
    flow_card --> flow_card_typing
    flow --> flow_new_card
    flow --> flow_maker
    flow_maker --> flow_maker_picker
    flow_maker_picker --> flow_maker_picker_typing
    flow --> flow_others
    flow --> flow_file
    flow --> flow_restore
    tui --> looks
    looks --> looks_look
    looks_look --> looks_look_list
    looks_look --> looks_look_typing
    looks --> looks_font_set
    looks --> looks_pointer
    looks --> looks_new_frit
    looks_new_frit --> looks_new_frit_typing
    looks_new_frit --> looks_new_frit_list
    tui --> fonts
    fonts --> fonts_filter
    fonts --> fonts_font
    tui --> effects
    effects --> effects_filter
    effects --> effects_effect
    tui --> settings
    settings --> settings_section
    settings_section --> settings_section_typing
    settings_section --> settings_section_list
    tui --> help
    tui --> any
    any --> any_question_quit
    any --> any_question_leave
    any --> any_question_confirm
    any --> any_question_deactivate
    any --> any_question_system_font
    any --> any_question_window_ops
    any --> any_trial
    any --> any_too_small
    classDef planned stroke-dasharray: 5 5
    class flow_card_list,looks,looks_look,looks_look_list,looks_look_typing,looks_font_set,looks_pointer,looks_new_frit,looks_new_frit_typing,looks_new_frit_list,fonts,fonts_filter,fonts_font,effects,effects_filter,effects_effect,any_question_deactivate,any_question_system_font,any_question_window_ops planned
```

Each section below names a screen by its place in the tree, says how you open and leave it, and lists its keys. The column "Shown as" gives the word the key line shows beside the key, and a dash marks a key that the key line leaves out.

## `tui`

Vtium-config's TUI, run at a terminal of at least 80×25.

**Open it:** Vtium config with no command, at a terminal.

**Leave it:** Q quits or asks; Ctrl-C saves and quits.

| Key | Shown as | What it does |
|---|---|---|
| Help and key line | - | Show, for the place you are in, the keys that act there and what each does. |

## `flow`

The Flow view: CONFIGURATION, NEW PATH and PARTS.

**Open it:** The TUI opens here; Tab from the tab before it, Shift-Tab from the tab after it, in the views table's order.

**Leave it:** Tab to the next tab, Shift-Tab to the tab before, in the views table's order.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config. If changes are not saved, it asks first. If new or removed font sets wait for an install, it asks whether to install them. |
| Esc | clear | Clears NEW PATH. If you were revising a path, the path leaves the configuration. |
| Tab | next view | Moves to the next view. |
| Shift-Tab | view before | Moves to the view before. |
| 1 | go to tab | Goes straight to the Fonts tab. |
| 2 | go to tab | Goes straight to the Effects tab. |
| 3 | go to tab | Goes straight to the Looks tab. |
| 5 | go to tab | Goes straight to the Settings tab. |
| 6 | go to tab | Goes straight to the Help tab. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s | save | Saves the changes that are not saved into the draft. |
| i | install | Saves, then asks whether to install the draft. |
| r | revert | Asks whether to throw away the draft's changes and copy the installed file back. |
| n | new | Asks whether to start a new configuration. |
| v | view file | Shows the draft's vtium.conf as text. |
| o | restore | Lists the files installed before, newest first, to copy one into the draft. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the box above. |
| Down | move | Moves to the box below. |
| Left | move | Moves to the box before, and on to the line before. |
| Right | move | Moves to the box after, and on to the next line. |
| Home | first | Goes to the first box. |
| End | last | Goes to the last box. |
| PgUp | scroll | Scrolls up by a window. |
| PgDn | scroll | Scrolls down by a window. |
| Space | pick | Picks the highlighted part into NEW PATH, or takes it out. |
| Enter | open | Opens the highlighted box: a part's detail, a path's card, a template's part maker, or the list of other inputs. |
| a | add path | Adds the path in NEW PATH to the configuration. |
| e | revise | Takes the highlighted path out of the configuration into NEW PATH, to change it. On a box that is not a path, it says how to choose one. |
| d | read devices | Reads the connected displays and input devices again. |
| p | preview | Shows a preview of the highlighted complete path. Today it only says that the preview is not available yet, or not in this terminal. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| t | trial | Says where a font set can be tried on this console: on a path card's font set field. Where this terminal is not a vtium console, or allow-font-ops is off, it says why the trial is off. |
| Click on a view's name | - | Switches to that view. |
| Click on a box | - | Highlights the box. A second click on the highlighted box opens it. |
| Wheel up | - | Moves to the box above, as Up does. |
| Wheel down | - | Moves to the box below, as Down does. |

## `flow.detail`

A part's detail on its label line: the part's name and label, with the choices that narrow it below.

**Open it:** Enter on a display or an input in PARTS; Up from the first choice.

**Leave it:** Esc to flow; Down, End and PgDn to flow.detail.choices; Enter to flow.detail.typing.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | - | Does nothing on the label line, which is the first line of the detail. |
| Down | choices | Moves from the label line to the part's first choice. |
| End | choices | Moves from the label line to the last choice. |
| PgDn | choices | Moves from the label line down by a page of choices. |
| Enter | label | Lets you type the part's label. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `flow.detail.typing`

A part's label being typed.

**Open it:** Enter on the detail's label line.

**Leave it:** Enter keeps it and Esc drops it, both to flow.detail.

| Key | Shown as | What it does |
|---|---|---|
| q | - | Types the letter q. |
| Esc | cancel | Cancels what you typed and goes back. |
| Tab | - | Does nothing while you type. |
| Shift-Tab | - | Does nothing while you type. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| Enter | keep | Keeps the value you typed. |
| Backspace | delete | Deletes the character before the cursor. |
| any letter | type | Types the letter. |
| Click | - | Does nothing while you type. |
| Wheel up, Wheel down | - | Do nothing while you type. |

## `flow.detail.choices`

A part's narrowing choices: the narrower parts the part's detail offers to pick into NEW PATH.

**Open it:** Down from the label line of a part's detail; Enter on a display or an input in NEW PATH, which opens on the part's own choice.

**Leave it:** Up from the first choice, Home, and PgUp near the top go to flow.detail; Enter picks a choice and goes to flow; Esc goes to flow.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the choice above, and from the first choice back to the label line. |
| Down | move | Moves to the choice below. |
| Home | top | Goes to the label line at the top of the detail. |
| End | last | Goes to the last choice. |
| PgUp | page up | Moves up by a page, as far as the label line. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the part narrowed to the highlighted choice into NEW PATH, and closes the detail. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |
| Click | - | Does nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `flow.info`

The detail of a display or an input of a path in CONFIGURATION, of the roaming inputs, or of Nowhere: what it matches and where it goes, read-only.

**Open it:** Enter on a display or an input of a path in CONFIGURATION, of the roaming inputs, or of Nowhere.

**Leave it:** Esc to flow; e takes the path into NEW PATH and goes to flow.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| e | revise | Takes this path out of the configuration into NEW PATH, to change it. While NEW PATH holds another path, e says so and changes nothing. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `flow.card`

An added path's terminal card, in CONFIGURATION.

**Open it:** Enter on the terminal box of a path in CONFIGURATION.

**Leave it:** Esc to flow; e revises the path and goes to flow.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| t | try live | Tries the card's look on this console for 10 seconds. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | On the card's look, shows the looks to choose from as a list. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | Shows the terminals this configuration defines as a list. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the field above. |
| Down | move | Moves to the field below. |
| Left | choose | Steps the highlighted field to its choice before. |
| Right | choose | Steps the highlighted field to its next choice. |
| Enter | set | Sets the highlighted field to the choice under the cursor, or lets you type a value where that choice is typed, such as the new terminal's name or a login command. |
| e | revise this path | Takes this path out of the configuration into NEW PATH, to change it. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `flow.card.list`

The shared list, as a list box inside the card: its displays, devices, VTs, font set files or terminals.

This screen is not built as described yet. See [#412](https://github.com/vtium/vtium/issues/412). Today: the card shows a set as items joined in a line, and Left and Right step a field through its choices.

**Open it:** Enter on a field of the card that holds a set of items.

**Leave it:** Enter picks and Esc closes, both to flow.card.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the highlighted item and closes the list. In a list that ends with Other…, picking Other… lets you type a value instead. |
| Esc | back | Closes the list without picking. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `flow.card.typing`

A field of a terminal card being typed: the new terminal's name, or a login command.

**Open it:** Enter on the name field of NEW PATH's card, or on a choice of a card that is typed, such as Other… on the login line.

**Leave it:** Enter keeps it and Esc drops it, both to the card.

| Key | Shown as | What it does |
|---|---|---|
| q | - | Types the letter q. |
| Esc | cancel | Cancels what you typed and goes back. |
| Tab | - | Does nothing while you type. |
| Shift-Tab | - | Does nothing while you type. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| Enter | keep | Keeps the value you typed. |
| Backspace | delete | Deletes the character before the cursor. |
| any letter | type | Types the letter. |
| Click | - | Does nothing while you type. |
| Wheel up, Wheel down | - | Do nothing while you type. |

## `flow.new-card`

NEW PATH's terminal card: the new terminal's name, its settings, and the displays and inputs it takes.

**Open it:** Enter on NEW PATH's terminal box.

**Leave it:** Esc to flow.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| t | try live | Tries the card's look on this console for 10 seconds. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | Shows the card's displays, devices or VTs as a list inside the card. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| Enter | choose | On the card's look, shows the looks to choose from as a list. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | Shows the terminals this configuration defines as a list. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the field above. |
| Down | move | Moves to the field below. |
| Left | choose | Steps the highlighted field to its choice before. |
| Right | choose | Steps the highlighted field to its next choice. |
| Enter | set | Sets the highlighted field to the choice under the cursor, or lets you type a value where that choice is typed, such as the new terminal's name or a login command. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `flow.maker`

A part maker: a template's choices, or an empty place in NEW PATH.

**Open it:** Enter on a template in PARTS or an empty place in NEW PATH.

**Leave it:** Esc to flow; Enter on a choice that makes a part goes to flow.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the choice above. |
| Down | move | Moves to the choice below. |
| Enter | choose | Makes the part from the highlighted choice, or opens the list of values for a choice that filters. |
| Home | first | Goes to the first line. |
| End | last | Goes to the last line. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `flow.maker.picker`

The filter picker: each value of a match with the devices it matches now, and Other… last.

**Open it:** Enter on a choice that filters.

**Leave it:** Esc to flow.maker; Enter on a value picks it and goes to flow.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the highlighted item and closes the list. In a list that ends with Other…, picking Other… lets you type a value instead. |
| Esc | back | Closes the list without picking. |
| Enter | choose | Shows the devices that each value matches as a list. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `flow.maker.picker.typing`

A value being typed after Other….

**Open it:** Enter on Other….

**Leave it:** Enter adds the value and goes to flow; Esc drops it and goes to flow.maker.picker.

| Key | Shown as | What it does |
|---|---|---|
| q | - | Types the letter q. |
| Esc | cancel | Cancels what you typed and goes back. |
| Tab | - | Does nothing while you type. |
| Shift-Tab | - | Does nothing while you type. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| Enter | keep | Keeps the value you typed. |
| Backspace | delete | Deletes the character before the cursor. |
| any letter | type | Types the letter. |
| Click | - | Does nothing while you type. |
| Wheel up, Wheel down | - | Do nothing while you type. |

## `flow.others`

The folded list of the inputs that are neither keyboards nor pointers.

**Open it:** Enter on the box of other inputs.

**Leave it:** Esc to flow.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `flow.file`

The file view: the draft's vtium.conf as text, read-only.

**Open it:** V.

**Leave it:** Esc to flow.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. Esc goes back to the configuration. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s | save | Saves the changes that are not saved into the draft. The file view stays open and shows the file as it now stands. |
| i | install | Saves, then asks whether to install the draft. The file view stays open. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| j | lines | Scrolls down by a line. |
| k | lines | Scrolls up by a line. |
| Down | lines | Scrolls down by a line. |
| Up | lines | Scrolls up by a line. |
| h | sideways | Scrolls left. |
| l | sideways | Scrolls right. |
| Left | sideways | Scrolls left. |
| Right | sideways | Scrolls right. |
| g | top | Goes to the top of the file. |
| G | bottom | Goes to the bottom of the file. |
| Home | top | Goes to the top of the file. |
| End | bottom | Goes to the bottom of the file. |
| Ctrl-D | half page | Scrolls down by half a page. |
| Ctrl-U | half page | Scrolls up by half a page. |
| PgDn | half page | Scrolls down by half a page. |
| PgUp | half page | Scrolls up by half a page. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `flow.restore`

Restore's list of the installed history, newest first.

**Open it:** O.

**Leave it:** Esc to flow; Enter asks any.question.confirm.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. Esc closes the list. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | restore | Asks whether to copy the highlighted installed file into the draft. |
| Click | - | Does nothing until you close what is open with Esc. |
| Wheel up | - | Acts as Up does in what is open. |
| Wheel down | - | Acts as Down does in what is open. |

## `looks`

The Looks tab: the looks, the font sets and the installed pointer themes, as three groups of one list, with a preview of the highlighted look or font set.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379). Today: the Looks tab is a list of sizes whose Enter writes the top-level font-size.

**Open it:** Tab from effects, Shift-Tab from flow; 3 from any tab with nothing open.

**Leave it:** Tab to flow, Shift-Tab to effects; a digit to another tab.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config. If changes are not saved, it asks first. If new or removed font sets wait for an install, it asks whether to install them. |
| Esc | - | Does nothing here. |
| Tab | next view | Moves to the next view. |
| Shift-Tab | view before | Moves to the view before. |
| 1 | go to tab | Goes straight to the Fonts tab. |
| 2 | go to tab | Goes straight to the Effects tab. |
| 4 | go to tab | Goes straight to the Flow tab. |
| 5 | go to tab | Goes straight to the Settings tab. |
| 6 | go to tab | Goes straight to the Help tab. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s | save | Saves the changes that are not saved into the draft. |
| i | install | Saves, then asks whether to install the draft. |
| r | remove | On a look, asks whether to delete it. On a font set, marks it for removal at the next install, or takes the mark back. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| n | new font set | Among the looks, makes a new look and lets you type its name. Among the font sets, opens the form for a new font set. Among the pointer themes, does nothing. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| t | try live | Tries the highlighted look or font set on this console for 10 seconds. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | open | On a look, opens its card. On a font set or a pointer theme, opens its detail. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| p | picture \| sample | Switches the preview between the sample characters and the picture of the console. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| w | resize grid | Asks whether to let programs resize the console's grid. y turns allow-window-ops on in the draft, and any other answer changes nothing. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| o | - | Does nothing here. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| Space | activate | On a font, an effect or a pointer theme, activates it, or deactivates it when it is active. Deactivating asks first when a look uses it. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |

## `looks.look`

A look's card: its name, font set, size, color map, shader and pointer theme, and below them what the look still misses, by role.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379). Today: vtium-config has no card for a look.

**Open it:** Enter on a look, or n among the looks, which opens the card with its name being typed.

**Leave it:** Esc to looks, with each change kept.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the line above. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Down | move | Moves to the line below. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Left | change | Steps the highlighted line to its choice before. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Right | change | Steps the highlighted line to its next choice. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | On the look's name, lets you type a new name. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | On the look's font set, shows the font sets to choose from as a list. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | On the look's size, shows the sizes that the look's fonts supply as a list. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | On the look's color map or shader, shows the active ones as a list, with none first. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | choose | On the look's pointer theme, shows the active pointer themes as a list, with vtium's own pictures first. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| t | try live | Tries this look on this console for 10 seconds. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |

## `looks.look.list`

The shared list of the choices for a line of the look's card: font sets, the sizes the fonts supply, active color maps, active shaders or active pointer themes.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379). Today: vtium-config has no such list.

**Open it:** Enter on a line of the look's card other than its name.

**Leave it:** Enter picks and Esc closes, both to looks.look.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the highlighted item and closes the list. In a list that ends with Other…, picking Other… lets you type a value instead. |
| Esc | back | Closes the list without picking. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `looks.look.typing`

The look's name being typed.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379). Today: vtium-config has no card for a look.

**Open it:** Enter on the name of the look's card; n among the looks.

**Leave it:** Enter keeps it and Esc drops it, both to looks.look.

| Key | Shown as | What it does |
|---|---|---|
| q | - | Types the letter q. |
| Esc | cancel | Cancels what you typed and goes back. |
| Tab | - | Does nothing while you type. |
| Shift-Tab | - | Does nothing while you type. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| Enter | keep | Keeps the value you typed. |
| Backspace | delete | Deletes the character before the cursor. |
| any letter | type | Types the letter. |

## `looks.font-set`

A font set's detail, read-only: its fonts by role, what it still misses by role, and the looks that use it.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379). Today: vtium-config has no detail for a font set.

**Open it:** Enter on a font set.

**Leave it:** Esc to looks.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | scroll | Scrolls the detail up by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| Down | scroll | Scrolls the detail down by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |

## `looks.pointer`

A pointer theme's detail, read-only: its folder, whether it is active, its trust mark and the check it fails, and the looks that use it.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379), [#442](https://github.com/vtium/vtium/issues/442). Today: vtium-config lists no pointer themes.

**Open it:** Enter on a pointer theme.

**Leave it:** Esc to looks.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | scroll | Scrolls the detail up by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| Down | scroll | Scrolls the detail down by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |

## `looks.new-frit`

The form for a new font set: a field for each role, from the text font to the fallback, then the font set's name (the views table's new-frit).

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379), [#412](https://github.com/vtium/vtium/issues/412). Today: the form opens under the Fonts tab, o writes the top-level allow-font-ops in a vtium console with allow-font-ops off, and i installs from the last row.

**Open it:** N among the font sets.

**Leave it:** Esc to looks; its last row adds the font set and goes to looks.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the field above. |
| Down | move | Moves to the field below. |
| Enter | add | On the last row, adds the new font set to the draft. |
| Home | first | Goes to the first row. |
| End | last | Goes to the last row. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | choose | On a role's field, shows the active fonts that can fill the role as a list to pick from. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |

## `looks.new-frit.typing`

A field of the form being typed.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379), [#412](https://github.com/vtium/vtium/issues/412). Today: the form opens under the Fonts tab, and Up and Down move to another field while a value is typed.

**Open it:** The form opens on its first field; Enter on a field goes to the next field.

**Leave it:** Enter on the last field goes to the form's last row, in looks.new-frit; Esc closes the form and goes to looks.

| Key | Shown as | What it does |
|---|---|---|
| q | - | Types the letter q. |
| Esc | cancel | Cancels what you typed and goes back. |
| Tab | - | Does nothing while you type. |
| Shift-Tab | - | Does nothing while you type. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| Enter | next field | Keeps the value and goes to the next field. |
| Backspace | delete | Deletes the character before the cursor. |
| any letter | type | Types the letter. |
| Up, Down, Home, End, PgUp, PgDn | - | Do nothing while you type. Enter keeps the value and goes to the next field, and Esc closes the form. |

## `looks.new-frit.list`

The shared list of the active fonts that can fill a role.

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379). Today: the form has three source fields that take typed text, and no list.

**Open it:** Enter on a role's field of the form.

**Leave it:** Enter picks and Esc closes, both to looks.new-frit.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the highlighted item and closes the list. In a list that ends with Other…, picking Other… lets you type a value instead. |
| Esc | back | Closes the list without picking. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `fonts`

The Fonts tab: a line of filters, then the fonts found on this system, each with its active mark and trust mark, with a preview of the highlighted font.

This screen is not built as described yet. See [#375](https://github.com/vtium/vtium/issues/375). Today: the Fonts tab lists the font sets, with n, r, t and s on them.

**Open it:** Tab from help, Shift-Tab from effects; 1 from any tab with nothing open.

**Leave it:** Tab to effects, Shift-Tab to help; a digit to another tab.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config. If changes are not saved, it asks first. If new or removed font sets wait for an install, it asks whether to install them. |
| Esc | - | Does nothing here. |
| Tab | next view | Moves to the next view. |
| Shift-Tab | view before | Moves to the view before. |
| 2 | go to tab | Goes straight to the Effects tab. |
| 3 | go to tab | Goes straight to the Looks tab. |
| 4 | go to tab | Goes straight to the Flow tab. |
| 5 | go to tab | Goes straight to the Settings tab. |
| 6 | go to tab | Goes straight to the Help tab. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s | save | Saves the changes that are not saved into the draft. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375). |
| i | install | Saves, then asks whether to install the draft. |
| o | - | Does nothing here. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Space | activate | On a font, an effect or a pointer theme, activates it, or deactivates it when it is active. Deactivating asks first when a look uses it. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | open | On the filter line, shows the filters as a list. On a font, opens its detail: its files, what it covers, its sizes, its trust mark and the looks that use it. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375). |
| t | try live | Tries the highlighted font on this console for 10 seconds. A font that is not an active system font cannot be tried, and t says why. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375). |
| p | preview | Switches the preview between the sample characters and the picture of the console. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375). |
| n | - | Does nothing here. New font sets are made in the Looks tab. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375). |
| r | - | Does nothing here. Font sets are removed in the Looks tab. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375). |

## `fonts.filter`

The shared list of the Fonts tab's filters: bitmap or outline, monospaced or not, each script, and well-known groups such as Nerd Fonts and color emoji.

This screen is not built as described yet. See [#375](https://github.com/vtium/vtium/issues/375). Today: vtium-config has no filters.

**Open it:** Enter on the filter line.

**Leave it:** Enter picks a filter and Esc closes, both to fonts.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the highlighted item and closes the list. In a list that ends with Other…, picking Other… lets you type a value instead. |
| Esc | back | Closes the list without picking. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `fonts.font`

A font's detail, read-only: its family and files, what it covers, its sizes, its trust mark and the check it fails, the looks that use it, and a duplicate mark.

This screen is not built as described yet. See [#375](https://github.com/vtium/vtium/issues/375). Today: vtium-config has no detail for a font.

**Open it:** Enter on a font.

**Leave it:** Esc to fonts; Enter on a font from your own folders asks any.question.system-font.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | scroll | Scrolls the detail up by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| Down | scroll | Scrolls the detail down by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | install as system font | On a font from your own folders, asks whether to install it as a system font. The question shows the exact command first. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375). |

## `effects`

The Effects tab: a line of filters, then the color maps and shaders found on this system, each with its active mark and trust mark, with a preview of the console with the highlighted effect applied.

This screen is not built as described yet. See [#439](https://github.com/vtium/vtium/issues/439). Today: the Effects tab shows one line that says the tab is coming, and lists no color maps or shaders.

**Open it:** Tab from fonts, Shift-Tab from looks; 2 from any tab with nothing open.

**Leave it:** Tab to looks, Shift-Tab to fonts; a digit to another tab.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config. If changes are not saved, it asks first. If new or removed font sets wait for an install, it asks whether to install them. |
| Esc | - | Does nothing here. |
| Tab | next view | Moves to the next view. |
| Shift-Tab | view before | Moves to the view before. |
| 1 | go to tab | Goes straight to the Fonts tab. |
| 3 | go to tab | Goes straight to the Looks tab. |
| 4 | go to tab | Goes straight to the Flow tab. |
| 5 | go to tab | Goes straight to the Settings tab. |
| 6 | go to tab | Goes straight to the Help tab. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s | save | Saves the changes that are not saved into the draft. |
| i | install | Saves, then asks whether to install the draft. |
| o | - | Does nothing here. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Space | activate | On a font, an effect or a pointer theme, activates it, or deactivates it when it is active. Deactivating asks first when a look uses it. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| Enter | open | On the filter line, shows the filters as a list. On a color map or a shader, opens its detail: its file, its trust mark and the looks that use it. Not built yet: see [#439](https://github.com/vtium/vtium/issues/439). |

## `effects.filter`

The shared list of the Effects tab's filters: color maps or shaders.

This screen is not built as described yet. See [#439](https://github.com/vtium/vtium/issues/439). Today: the Effects tab lists nothing yet, so it has no filter line.

**Open it:** Enter on the filter line.

**Leave it:** Enter picks a filter and Esc closes, both to effects.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the highlighted item and closes the list. In a list that ends with Other…, picking Other… lets you type a value instead. |
| Esc | back | Closes the list without picking. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `effects.effect`

A color map's or a shader's detail, read-only: its file, whether it is active, its trust mark and the check it fails, and the looks whose vtlook names it.

This screen is not built as described yet. See [#439](https://github.com/vtium/vtium/issues/439). Today: the Effects tab lists nothing yet, so it has no detail to open.

**Open it:** Enter on a color map or a shader.

**Leave it:** Esc to effects.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Esc | back | Goes back, with the same item highlighted. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | scroll | Scrolls the detail up by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |
| Down | scroll | Scrolls the detail down by a line. Not built yet: see [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). |

## `settings`

The Settings view: the sections of vtium.conf's keys, as the shared list.

**Open it:** Tab from the tab before it, Shift-Tab from the tab after it, in the views table's order.

**Leave it:** Tab to the next tab, Shift-Tab to the tab before, in the views table's order.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config. If changes are not saved, it asks first. If new or removed font sets wait for an install, it asks whether to install them. |
| Esc | - | Does nothing here. |
| Tab | next view | Moves to the next view. |
| Shift-Tab | view before | Moves to the view before. |
| 1 | go to tab | Goes straight to the Fonts tab. |
| 2 | go to tab | Goes straight to the Effects tab. |
| 3 | go to tab | Goes straight to the Looks tab. |
| 4 | go to tab | Goes straight to the Flow tab. |
| 6 | go to tab | Goes straight to the Help tab. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s | save | Saves the changes that are not saved into the draft. |
| i | install | Saves, then asks whether to install the draft. |
| o | - | Does nothing here. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| Enter | open | Opens the highlighted section, with its keys in titled groups. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |

## `settings.section`

One section: its groups as headed blocks, with their keys beneath.

**Open it:** Enter on a section.

**Leave it:** Esc to settings.

| Key | Shown as | What it does |
|---|---|---|
| Enter | change | Lets you type the highlighted key's value, or shows its allowed values as a list. |
| Esc | back | Goes back to the list of sections. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| Enter | - | Shows the key's allowed values as a list to pick from, with the highlight on the value the key has now. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Up | move | Moves to the key above. |
| Down | move | Moves to the key below. |
| Home | first | Goes to the first key of the section. |
| End | last | Goes to the last key of the section. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |

## `settings.section.typing`

A key's value being typed.

**Open it:** Enter on a key whose value is typed.

**Leave it:** Enter keeps it and Esc drops it, both to settings.section.

| Key | Shown as | What it does |
|---|---|---|
| q | - | Types the letter q. |
| Esc | cancel | Cancels what you typed and goes back. |
| Tab | - | Does nothing while you type. |
| Shift-Tab | - | Does nothing while you type. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| Up, Down | - | Do nothing while you type. |
| Enter | keep | Keeps the value you typed. |
| Backspace | delete | Deletes the character before the cursor. |
| any letter | type | Types the letter. |

## `settings.section.list`

The shared list of a key's allowed values.

**Open it:** Enter on a key with a fixed set of values.

**Leave it:** Enter picks and Esc closes, both to settings.section.

| Key | Shown as | What it does |
|---|---|---|
| Up | move | Moves to the item above. |
| Down | move | Moves to the item below. |
| Home | first | Goes to the first item. |
| End | last | Goes to the last item. |
| PgUp | page up | Moves up by a page. |
| PgDn | page down | Moves down by a page. |
| Enter | pick | Picks the highlighted item and closes the list. In a list that ends with Other…, picking Other… lets you type a value instead. |
| Esc | back | Closes the list without picking. |
| q | quit | Quits vtium-config, and asks first if changes are not saved. |
| Tab | - | Does nothing until you close what is open with Esc. |
| Shift-Tab | - | Does nothing until you close what is open with Esc. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s i r n v o | - | Do nothing until you close what is open with Esc. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |

## `help`

The Help view: the Help and About pages.

**Open it:** Tab from the tab before it, Shift-Tab from the tab after it, in the views table's order.

**Leave it:** Tab to the next tab, Shift-Tab to the tab before, in the views table's order.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config. If changes are not saved, it asks first. If new or removed font sets wait for an install, it asks whether to install them. |
| Esc | - | Does nothing here. |
| Tab | next view | Moves to the next view. |
| Shift-Tab | view before | Moves to the view before. |
| 1 | go to tab | Goes straight to the Fonts tab. |
| 2 | go to tab | Goes straight to the Effects tab. |
| 3 | go to tab | Goes straight to the Looks tab. |
| 4 | go to tab | Goes straight to the Flow tab. |
| 5 | go to tab | Goes straight to the Settings tab. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| s | - | Does nothing here. |
| i | - | Does nothing here. |
| o | - | Does nothing here. Not built yet: see [#412](https://github.com/vtium/vtium/issues/412). |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| . | more | Shows the key line's next page, and the first page again after the last. Does nothing where the keys fit on one row. |
| Help and key line | - | Show, for the place you are in, the keys that act there and what each does. |
| Up | scroll | Scrolls the page up by a line. |
| Down | scroll | Scrolls the page down by a line. |
| Home | top | Goes to the top of the page. |
| End | bottom | Goes to the bottom of the page. |
| PgUp | page up | Scrolls the page up by a window. |
| PgDn | page down | Scrolls the page down by a window. |
| Left | Help, About | Switches between the Help and About pages. |
| Right | Help, About | Switches between the Help and About pages. |

## `any`

What opens over any view: questions, the live trial and the screen below the minimum size.

## `any.question.quit`

The quit question: changes are not saved.

**Open it:** Q while changes are not saved.

**Leave it:** Y saves and quits; n quits without saving; Esc and Ctrl-C go back to where q was pressed.

| Key | Shown as | What it does |
|---|---|---|
| y | save and quit | Saves the changes that are not saved into the draft, then quits. |
| n | quit without saving | Quits without saving the changes that are not saved. |
| Esc | cancel | Cancels the question. Nothing changes. |
| Ctrl-C | cancel | Cancels the question, as Esc does. Nothing changes. |
| q | - | Does nothing. The question stays until you answer it or cancel it. |
| Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| Shift-Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| any other key | - | Does nothing. The question stays until you answer it or cancel it. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| Click | - | Does nothing. The question stays until you answer it or cancel it. |
| Wheel up, Wheel down | - | Do nothing. The question stays until you answer it or cancel it. |

## `any.question.leave`

The leave question: new or removed font sets wait for an install.

**Open it:** Q while font set changes wait in memory.

**Leave it:** Y installs and quits; n saves the changes that are not saved and quits without installing; Esc and Ctrl-C go back.

| Key | Shown as | What it does |
|---|---|---|
| y | install and quit | Installs the draft, then quits. Install saves the changes that are not saved first. |
| n | quit without installing | Saves the changes that are not saved into the draft, then quits without installing. The new and removed font sets that wait for an install are lost. |
| i | - | Does nothing. The question stays until you answer it or cancel it. |
| l | - | Does nothing. The question stays until you answer it or cancel it. |
| Esc | cancel | Cancels the question. Nothing changes. |
| Ctrl-C | cancel | Cancels the question, as Esc does. Nothing changes. |
| q | - | Does nothing. The question stays until you answer it or cancel it. |
| Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| Shift-Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| any other key | - | Does nothing. The question stays until you answer it or cancel it. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| Click | - | Does nothing. The question stays until you answer it or cancel it. |
| Wheel up, Wheel down | - | Do nothing. The question stays until you answer it or cancel it. |

## `any.question.confirm`

A question before an action: Install, Revert, Restore, New, or deleting a look.

**Open it:** I, r, n, or Enter on Restore's list.

**Leave it:** Y acts; Esc and Ctrl-C go back without acting; every other key, n included, does nothing.

| Key | Shown as | What it does |
|---|---|---|
| y | yes | Answers yes, and vtium-config does what the question asks. |
| Esc | cancel | Cancels the question. Nothing changes. |
| Ctrl-C | cancel | Cancels the question, as Esc does. Nothing changes. |
| q | - | Does nothing. The question stays until you answer it or cancel it. |
| Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| Shift-Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| any other key | - | Does nothing. The question stays until you answer it or cancel it. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |
| Click | - | Does nothing. The question stays until you answer it or cancel it. |
| Wheel up, Wheel down | - | Do nothing. The question stays until you answer it or cancel it. |

## `any.question.deactivate`

The question before a font, an effect or a pointer theme that a look uses is deactivated, which lists those looks.

This screen is not built as described yet. See [#375](https://github.com/vtium/vtium/issues/375), [#439](https://github.com/vtium/vtium/issues/439), [#379](https://github.com/vtium/vtium/issues/379). Today: vtium-config has no such question.

**Open it:** Space on an active font, effect or pointer theme that a look uses.

**Leave it:** Y deactivates it; Esc and Ctrl-C go back without acting; every other key does nothing.

| Key | Shown as | What it does |
|---|---|---|
| y | yes | Answers yes, and vtium-config does what the question asks. |
| Esc | cancel | Cancels the question. Nothing changes. |
| Ctrl-C | cancel | Cancels the question, as Esc does. Nothing changes. |
| q | - | Does nothing. The question stays until you answer it or cancel it. |
| Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| Shift-Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| any other key | - | Does nothing. The question stays until you answer it or cancel it. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |

## `any.question.system-font`

The question before a font from your own folders is installed as a system font, which shows the exact command.

This screen is not built as described yet. See [#375](https://github.com/vtium/vtium/issues/375), [#440](https://github.com/vtium/vtium/issues/440). Today: vtium-config has no such question.

**Open it:** Enter in the detail of a font from your own folders.

**Leave it:** Y installs the font; Esc and Ctrl-C go back without acting; every other key does nothing.

| Key | Shown as | What it does |
|---|---|---|
| y | yes | Answers yes, and vtium-config does what the question asks. |
| Esc | cancel | Cancels the question. Nothing changes. |
| Ctrl-C | cancel | Cancels the question, as Esc does. Nothing changes. |
| q | - | Does nothing. The question stays until you answer it or cancel it. |
| Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| Shift-Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| any other key | - | Does nothing. The question stays until you answer it or cancel it. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |

## `any.question.window-ops`

"Let programs resize the grid?"

This screen is not built as described yet. See [#379](https://github.com/vtium/vtium/issues/379). Today: vtium-config has no such question.

**Open it:** W in looks while allow-window-ops is off.

**Leave it:** Y writes allow-window-ops = on and n, Esc and Ctrl-C write nothing, all to looks.

| Key | Shown as | What it does |
|---|---|---|
| y | yes | Answers yes, and vtium-config does what the question asks. |
| n | no | Answers no. Nothing changes. |
| Esc | cancel | Cancels the question. Nothing changes. |
| Ctrl-C | cancel | Cancels the question, as Esc does. Nothing changes. |
| q | - | Does nothing. The question stays until you answer it or cancel it. |
| Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| Shift-Tab | - | Does nothing. The question stays until you answer it or cancel it. |
| any other key | - | Does nothing. The question stays until you answer it or cancel it. |
| key line | - | Shows one row. Where the keys do not fit on one row, it shows them in pages, and ends with `. more` and the page number. |

## `any.trial`

The live trial's countdown: a font set or a size tried on the console.

**Open it:** T in looks or on a card's font set field; Enter on a size in looks.

**Leave it:** K keeps it; any other key or the countdown's end puts it back; both return to where the trial began.

| Key | Shown as | What it does |
|---|---|---|
| k | keep | Keeps the font or size you are trying. |
| any other key | put back | Puts back the console's font or size, and does nothing else. |
| q | - | Puts back the console's font or size, and does nothing else. |
| Esc | - | Puts back the console's font or size, and does nothing else. |
| Tab | - | Puts back the console's font or size, and does nothing else. |
| Shift-Tab | - | Puts back the console's font or size, and does nothing else. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font, and quits without asking. |
| Click or wheel | - | Puts back the console's font or size, and does nothing else. |

## `any.too-small`

The screen below 80×25, which draws no view.

**Open it:** The terminal shrinks below 80×25, or a tried font or size leaves it so.

**Leave it:** The terminal grows back to the view; q quits or asks the quit question on one row.

| Key | Shown as | What it does |
|---|---|---|
| q | quit | Quits vtium-config. If changes are not saved, it asks first, on one row. |
| u | undo | Puts back the font and size the console had when vtium-config started. Not built yet: see [#379](https://github.com/vtium/vtium/issues/379). |
| any other key | - | Does nothing here. |
| Esc | - | Does nothing here. |
| Tab | - | Does nothing here. |
| Shift-Tab | - | Does nothing here. |
| Ctrl-C | save and quit | Saves the changes that are not saved into the draft, puts back the console's font and size, and quits without asking. |
