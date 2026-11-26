---
contract: adr/v1
kind: decision
verb: change
capability: platform
amends:
  - 'ADR-1403#D10: one way to move through the six tabs'
  - 'ADR-1206#D2: the message architecture'
basis:
  - operator: aaronsb
    level: guided
    said: "it's time to not clear the screen on a subconfiguration screen and instead use a window centered over the previous screen. think how many tui apps as you dig deeper into configurations just place a stack of windows in z order over the last."
    via: "the maintainer in the steering session on 2026-11-25, during the VT review of vtium-config's looks; the GitHub issue that asks for vtium-config's deeper screens to open as stacked windows records the request"
  - evidence: "Read by this record's agent on 2026-11-25 at integration 2831f4bd1. Each level below a tab replaces the tab's whole area between the status line and the key line. The provisional snapshots node-looks-80x25, node-looks_look-80x25 and node-looks_look_list-80x25 show the list of looks, then the look's card in the list's place, then the font set list in the card's place. Only a question shares the screen with what it covers, on two rows above the key line (node-any_question_quit-80x25). Below 80×25 the TUI draws only the screen that asks for a larger terminal (`too_small` in crates/vtium-config/src/tui/layout.rs, node-any_too-small-79x24). The look's card says in its own text that Esc keeps the changes for Save (`CARD_HINT` in crates/vtium-config/src/tui/words/looks.rs)."
  - precedent: ADR-1403
  - precedent: ADR-1206
  - precedent: ADR-1201
observable:
  - "In the Looks tab at 80×25, Enter on a look opens its card as a window with the list of looks visible around it, and Enter on the card's Font set line opens the font sets as a smaller window over the card, with the card's Font set line visible above it, as the figure shows"
  - "Esc closes the font set list and shows the card with its Font set line highlighted, and Esc again closes the card and shows the Looks tab with the same look highlighted; no frame clears the terminal"
  - "A look's card says in its lower border that Esc keeps its changes for Save, and a list of choices says that Esc changes nothing; while a name is typed, the card's border says that Esc drops the typed name"
  - "With two windows open, shrinking the terminal to 79×24 shows the screen that asks for a larger terminal, and growing it back to 80×25 shows the same two windows with the same lines highlighted"
  - "The scenario walk's three maps stay byte-identical, every use case passes without an edit to its steps, and below the tab bar at most one line is in reverse video after every key"
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "it's time to not clear the screen on a subconfiguration screen and instead use a window centered over the previous screen. think how many tui apps as you dig deeper into configurations just place a stack of windows in z order over the last."
    via: the maintainer in the steering session on 2026-11-25, during the VT review of vtium-config's looks; the request that this record answers
  - operator: aaronsb
    said: "I've decided this refactor is more important than after 0.1.0"
    via: the maintainer in conversation with the steering session on 2026-11-25, about the stacked windows, which had been filed for after 0.1.0 earlier that day; the change moves into 0.1.0, after the work that fixes vtium-config's looks and the work that writes its use cases as scripts
status: proposed
date: 2026-11-25
deciders:
  - aaronsb
related:
  - ADR-1201
  - ADR-1205
---

# ADR-1405: vtium-config opens each deeper screen as a window centered over the screen below, and Esc closes the top window

## Summary

- **Decided:** vtium-config's TUI opens each level below a tab as a window centered over what is already on screen, and the screen below stays visible around it. A card, a detail, a list of choices, a form, the file view, Restore's list and a question each open as a window. The windows stack in the order the person opened them. Esc closes the top window and shows the one below again, with the item it opened from highlighted. Each window's lower border says what closing it does with what changed there: a card keeps its changes for Save, and a list closed with Esc changes nothing. Closing a window never writes the draft; Save does. The tab bar, the status line and the key line keep their rows, and the key line shows the top window's keys. Every key keeps the meaning that ADR-1403's key table gives it, and the interface tree keeps its nodes. The change is in how the TUI draws, not in what a key does. Only the top window shows a highlight. No window reaches past the area between the status line and the key line, and at 80×25 a large window may fill that area. Below 80×25, as at 79×24 or on a 320×240 console with an 8×8 font, the TUI shows the screen that asks for a larger terminal (ADR-1201 D3). The windows come back unchanged when the terminal grows. The scenario walk's maps, the use-case files and the screen snapshots guard the change. A line of a map that moves does so in a reviewed commit of its own.
- **Trades away:** A window has less room than the whole area, because it leaves a margin of the screen below around it. At 80×25 a large window fills the area, so the person sees little of the screen below there. A picture of the console under a window is taken down while the window is open. Every screen snapshot above the minimum size moves once and goes back to the maintainer.
- **One-way?** No. The change is in how the TUI draws its levels. The keys, the interface tree and the files vtium-config writes stay as they are. So the change can be undone in a session, at the cost of drawing the snapshots again.
- **Probes:**
  - *Not confident (window size):* Each window takes the size its content needs, centered, with a margin of two columns and one row of the screen below on each side. A window whose content does not fit inside that margin fills the whole area between the status line and the key line, and scrolls inside (D2). Lean: size each window to its content, because a list or a question then leaves most of the screen below in sight, which is what you asked for. The other end is one size for every window, such as three quarters of the area. That looks more regular, but is too large for a short list and too small for a long card. Should each window take the size its content needs?
  - *Not confident (dimming):* The screen below keeps its colors and is not dimmed. Only the top window shows a highlight, and its border is drawn in double lines where the windows below have single lines, as the figure shows (D3). Lean: no dimming. Dim text needs SGR 2 (faint) or color, which `NO_COLOR`, the mono color set and some terminals do not show, and a double line reads the same in every terminal. The other end dims the screen below in the muted color set, which makes the top window stand out more in a color terminal. Should the screen below stay undimmed?
  - *Not confident (a window wider than the terminal):* No window is drawn wider or taller than the area between the status line and the key line. Its text wraps at the window's width, a name too long for its line is cut in the middle with `…`, and a long list or card scrolls (D2, D6). Below 80×25 the TUI draws no window and shows the screen that asks for a larger terminal. So a 40×30 console, a 320×240 panel with an 8×8 font, is configured over ssh or with the commands, as ADR-1201 D3 decides. Lean: keep the 80×25 minimum, because a 40-column layout means laying out every card and the Flow view again. The other end lowers the minimum, so that on a small console each window fills the screen. Should the 80×25 minimum stay?
- **Inversion:** The decision sits between "one screen at a time", where each level replaces the tab's area as it does today, and "a window manager", where the person moves, resizes and overlaps windows at will. vtium-config takes the middle: centered windows that keys alone open and close, in the order the interface tree already gives.

## Context

vtium-config's TUI has six tabs, and each tab opens levels below it, as ADR-1403 D10's tree of use sets out: a detail or a card, then a list of choices or a typed value. Today each level replaces the tab's whole area between the status line and the key line (`basis`). Opening a look's card hides the list of looks, and opening a line's choices hides the card. The person loses sight of where they came from, and Esc reads as going somewhere else more than as closing what is open. Only a question shares the screen with what it covers, on two rows above the key line.

The maintainer raised this on 2026-11-25, during a VT review of vtium-config's looks: "it's time to not clear the screen on a subconfiguration screen and instead use a window centered over the previous screen. think how many tui apps as you dig deeper into configurations just place a stack of windows in z order over the last." The same review found that Esc saved a look's changes without saying so. The work that fixes that review's findings makes Esc's effect clear in the card's text, and this record moves that statement into every window's border.

The maintainer first placed the change after the first release, then moved it into that release the same day (`considered`). It is built after the work that fixes the looks and the work that writes vtium-config's use cases as scripts. Those use cases then check every task once the levels become windows.

ADR-1206 builds the TUI on ratatui, with one navigation module that knows which node of the interface tree is open. Every level is already a node of that tree. So the windows are a new way to draw levels the TUI already has, and the keys and the tree stay as they are.

## Decision

D1 says what opens as a window and where it sits, D2 how large it is, and D3 what the screen below shows. D4 sets Esc and the words that say what closing a window does. D5 keeps the keys' meanings, and D6 sets the sizes the stack works at. D7 says how the TUI draws the stack within ADR-1206's structure, D8 what guards the change, and D9 what this record changes in ADR-1403 and ADR-1206.

The figure shows the lean of D1 to D4 at 80×25: the Looks tab, a look's card as a window over it, and the font set list as the top window over the card. The tab underneath is the provisional snapshot of the Looks tab as it reads today.

![vtium-config at 80×25, white text on a black terminal. The tab bar shows Fonts, Effects, Looks (in reverse video), Flow, Settings and Help, with Save and Install at the right, and the status line reads draft mine, no changes, no problems. Underneath is the Looks tab: the list of looks and font sets at the left and the text preview at the right. Over it, centered, is a window with a single-line border titled Look proof. It holds the look's card: Name proof, Font set proof in bold, Size, Color scheme, vtlook, Pointer theme, then its fonts by role and what it still misses. Its lower border reads Esc: back, changes kept for Save. Over the card, centered and smaller, is the top window, with a double-line border titled Font set. It lists six font sets, with proof marked and highlighted in reverse video, and its lower border reads Esc: back, no change. The card's Font set line shows above the top window. The Looks tab's list and preview show around the card. The key line at the bottom reads up and down move, Enter pick, Esc back, q quit, period more 1/3.](figures/ADR-1405-stacked-windows.svg)

### D1: each level below a tab opens as a window

- **What opens as a window.** Every node of ADR-1403's interface tree below a tab opens as a window: a detail, a card, a list of choices, a part maker, a picker, the new-frit form, the file view, Restore's list, a Settings section and a question. Three things are not windows. The tab itself fills the area as it does today. A typed value is edited in its line of the window it belongs to. The live trial's countdown and the screen that asks for a larger terminal stand over everything, as they do today.
- **The stack.** The windows stack in the order the person opened them, each over the last. The stack is the levels that the navigation module names for the open tab, so the TUI keeps no second record of where the person is.
- **Where a window sits.** Each window is centered over the work area, the rows between the status line and the key line at the terminal's full width. Every window is centered on that area, whatever window it opened from. So a smaller window sits inside the larger one below it, and a deep level never drifts toward an edge.
- **What stays outside the windows.** The tab bar, the status line and the key line keep their rows, and no window covers them. The key line shows the keys of the top window's node, as it shows the keys of the open node today.
- **A question** opens as a window over the top window. Its own text says what each answer does, and the key line shows its answer keys, as ADR-1403 D7 decides.
- **A window's title.** A window's upper border names what it shows, such as `Look proof` or `Font set`, in words from vtium-config's catalog.

### D2: the size of a window

This section states the lean of the probe on window size, which the maintainer settles.

- **To its content.** A window takes the size its content needs, with a margin of two columns and one row of the screen below on each side. At 80×25 the work area is 80×20, so a window with its margin is at most 76×18. vtium-config lays the content out at that widest width. When the content's height fits too, the window shrinks to the content's width and height.
- **Filling the area.** A window whose content does not fit inside the margin fills the whole work area. Its content scrolls inside it, with the keys that scroll a list or a detail. The file view fills the area at every size, because a file is longer than any screen.
- **Never wider than the area.** No window is wider or taller than the work area. Its text wraps at the window's width less its border and one column of padding on each side. A name too long for its line is cut in the middle with `…`, as ADR-1201 D3 cuts long names at 80×25.
- **Stable while open.** vtium-config sizes a window when it opens and when the terminal's size changes, and at no other time. So a window does not jump while the person changes a value in it.

### D3: what the screen below shows

This section states the lean of the probe on dimming, which the maintainer settles.

- **The screen below keeps its colors.** The tab and every window below the top one keep their text and colors as the model draws them, undimmed.
- **One highlight.** Only the top window draws its highlighted line in reverse video. A window below marks the line it opened from in bold, so the person sees which line the top window belongs to. The tab bar keeps the open tab in reverse video, because the tab bar is above the work area.
- **Borders.** The top window's border is drawn in double lines and the borders of the windows below in single lines. So the top window stands out in every terminal, `NO_COLOR` and the mono color set included.
- **Pictures under a window.** A terminal draws a kitty graphics picture at z-index 0 over the text cells, so a window drawn over a picture would stay hidden behind it. While any window covers part of a picture of the console, vtium-config deletes the picture. vtium-config places the picture again when the windows over it close. In a vtium console the Looks tab's preview is text, which a window covers like any other text.

### D4: Esc, and what closing a window does with its changes

- **Esc closes the top window**, as the key table's `esc.layer` row says: the window below shows again, with the same item highlighted. vtium-config draws the cells the window covered again from the model and does not clear the terminal.
- **The lower border says what closing does.** Each window's lower border says what Esc does with what changed in that window, in words from vtium-config's catalog:
  - A card keeps each change as the person makes it, and Esc keeps those changes for Save, as ADR-1403 D10 decides. The border says so, as in `Esc: back, changes kept for Save`.
  - A detail, a list of choices, the file view and Restore's list change nothing when Esc closes them, and their border says so, as in `Esc: back, no change`. Enter on a list's item picks it and closes the list, and the change lands in the card below.
  - While a value is typed in a window's line, the border says that Esc drops the typed value and Enter keeps it.
- **Closing never saves.** No window writes the draft when it closes. Save writes the TUI's changes into the draft, and Install saves first, as ADR-1403 D7 decides. The status line keeps counting the changes that are not saved.
- **One statement in one place.** The card's own hint, which ends "Esc keeps the changes for Save.", gives that sentence to the border, so the card does not say it twice.

### D5: the keys keep their meanings

- **No row and no node changes.** Every row of ADR-1403's key table keeps its meaning, and the interface tree keeps every node. Tab, Shift-Tab and the digits act only on a tab with nothing open, so they do nothing while a window is open, as today. `q` quits or asks from any window, and the action keys do nothing while a window is open, the file view excepted, as ADR-1403 D7 decides.
- **The mouse.** A click does nothing while a window is open, as the key table's `click.layer` row says. A click outside the top window does not close it.
- **Esc on a tab** keeps its meaning there: in the Flow view it clears NEW PATH, and elsewhere it does nothing.

### D6: the sizes the stack works at

- **From 80×25 up,** D1 to D5 hold. At 80×25 a large window, such as the file view or a long card, fills the work area (D2).
- **Below 80×25,** as at 79×24, the TUI draws no window and shows the screen that asks for a larger terminal, as ADR-1201 D3 decides. The stack stays as it was under that screen. When the terminal grows to 80×25 or more, the same windows come back with the same lines highlighted, each sized again for the new area.
- **A small console,** such as a 320×240 panel with an 8×8 font, has a grid of 40×30, below the minimum, so it shows that screen too. It is configured over ssh from a larger terminal, with the commands, or with a draft built elsewhere, as ADR-1201 D3 decides. The probe on a window wider than the terminal asks whether the minimum stays.
- **A change of size,** from SIGWINCH or a font switch in the live trial, sizes every window again for the new area and keeps the stack.

### D7: how the TUI draws the stack

- **The view draws from the bottom up.** The view draws the tab into the work area, then each open level as a window, from the bottom of the stack to the top. Each window clears its rectangle with ratatui's `Clear` widget and draws its level inside a `Block` that carries its border, its title and its lower border's words.
- **A layout function gives each rectangle.** Each window's rectangle comes from a layout function of the tab, which takes the tab's state, `shared` and the size, as ADR-1206 D4 decides for every fact about the layout. Update uses the same rectangles to scroll a window and to keep its highlight in view, and never calls the view.
- **Each level draws at its window's size.** A level's view takes its window's inner rectangle as its area, so a card wraps and scrolls at the window's width and height.
- **Only the changed cells are written.** ratatui writes only the cells that change between frames, as ADR-1206 D5 decides. So opening or closing a window writes the window's cells and the cells it uncovers, with no clear of the terminal.

### D8: what guards the change

- **The maps.** The windows change how the TUI draws and no transition, so the scenario walk's maps, `map-vtium.tsv`, `map-ops-off.tsv` and `map-other.tsv`, stay byte-identical. A line of a map that moves does so in a commit of its own, which names each moved line and why. A fresh reviewer reads that commit before it merges, as a golden is reviewed.
- **The use cases.** Every case under `crates/vtium-config/tests/scenarios/cases/` passes before the change and after it, with no edit to its steps. The runner finds the highlighted line in reverse video below the tab bar, and only the top window draws one, so its steps keep finding their lines. A check that reads the whole screen now also sees the windows below. A case whose check passed only because the screen was cleared is reported to the steering session, and the case changes in a reviewed commit of its own. The author of the change never edits it.
- **The snapshots.** Every node's snapshot at 80×25 and at 100×30 moves. The snapshots stay provisional and are drawn again in a commit of their own for the maintainer, as ADR-1206 D5 decides. The snapshots at 79×24 show only the screen that asks for a larger terminal, so they stay as they are.
- **New checks after every key.** The walk's checks gain four rules. Every window lies inside the work area. The tab bar, the status line and the key line keep their rows. Below the tab bar, at most one line is in reverse video, and it lies in the top window. The top window's lower border shows the words for its node. The explorer already shrinks the terminal below the minimum and grows it back. It does so with windows open too, and checks that the same windows return.

### D9: what this record changes in ADR-1403 and ADR-1206

Once this record is accepted, ADR-1403 D10's tree of use gains a dated note. The note says that each level below a tab opens as a window centered over the work area, and that Esc closes the top window (D1, D4). ADR-1206 D2's bullet on the view gains a dated note, which says that the view draws the open levels as windows after the tab, from the bottom of the stack up (D7). Neither record changes in anything else.

## Consequences

### Positive

- The person sees where they came from at every level, and the line they opened it from is marked.
- Esc reads as closing what is on top, which is what it does.
- What closing does with a window's changes is written where the person closes it, so Esc no longer keeps changes without saying so.
- No key changes meaning and no node comes or goes, so the key table, the interface tree, the help and the scenario walk's maps stay as they are.
- The stack is the navigation module's own record of the open levels, so the screen and the keys cannot disagree about where the person is.

### Negative

- A window has less room than the whole work area, and at 80×25 a large window fills the area, so the stack shows little of the screen below there.
- A picture of the console disappears while a window covers part of it.
- Every snapshot at 80×25 and 100×30 moves, and the maintainer reviews them all again.
- A use case whose check relied on a cleared screen needs a reviewed change.
- Each frame draws the screen below as well as the top window, which costs cells, though ratatui writes only the cells that change.

### Neutral

- The 80×25 minimum, the screen below it, the commands and the files vtium-config writes do not change.
- The help pages that are generated from the use cases do not change, because a use case's steps name labels, not positions.
- The live trial's countdown keeps its own screen.

## Alternatives Considered

- **One screen at a time, with a line that names the path, such as `Looks › proof › Font set`.** Not taken: the line names where the person came from but does not show it, and the maintainer asked for windows over the screen below.
- **Windows that cascade, each one offset down and to the right of the window below.** Not taken: the maintainer asked for centered windows, and each offset level loses columns at 80, where a look's card needs about 70.
- **Levels side by side, each in a column to the right of the one before.** Not taken: three levels at 80 columns leave about 26 columns each, too narrow for a card.
- **Windows the person moves and resizes with the mouse.** Not taken: the keys do everything in vtium-config, mouse reporting is on only in the Flow view, and a click does nothing while something is open (ADR-1403 D7).
- **Dimming the screen below.** Open: the probe on dimming asks the maintainer, and D3 states the lean against it.
