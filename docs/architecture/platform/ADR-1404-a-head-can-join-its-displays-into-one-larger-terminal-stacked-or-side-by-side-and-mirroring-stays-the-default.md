---
contract: adr/v1
kind: decision
verb: add
capability: platform
amends:
  - "ADR-1401#D1: heads"
basis:
  - operator: aaronsb
    level: guided
    said: "this is probably the time to think about joining multiple displays into a combined terminal (like, two screens physically laid out one above the other, and forming a single terminal with more rows."
    via: the maintainer in the steering session, 2026-11-09, quoted in the body of the GitHub issue for joined displays
  - operator: aaronsb
    level: directed
    said: "The decision record for joined displays is written now, and the build is part of the plan to 0.1.0, in the alpha phase, among the remaining features that follow the privilege split, reload, try-and-revert, the lock and paste."
    via: "the maintainer's interview in the steering session, 2026-11-20, round 3 and round 5, as the steering session's comment on the GitHub issue for joined displays records it"
    paraphrase: true
  - evidence: "vtium at integration 5872cd2e2: a head that holds several displays mirrors one grid across them. crates/vtium/src/layout.rs gives the grid as the smallest that every display of the head holds at the base font, and gives each display its own font height, the largest that still holds that grid, so displays of different sizes already draw at different heights. The point rule converts at the lowest density among a mirrored head's displays (ADR-1401 D1)."
  - evidence: "ADR-1401 D8 at integration 5872cd2e2: a head keeps its sessions while any of its displays remains, and a head that loses its last display does what `when-display-lost` says; a display left off stays present but dark, and a head reattaches only to a display its rules match."
  - precedent: ADR-1401
  - precedent: ADR-1403
  - precedent: ADR-1003
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Same physical size"
    via: the steering session's interview on 2026-11-22 (question tool, brief/for/against/lean per probe); the figure ADR-1404-joined-displays.svg was rendered and shown first; the maintainer chose the lean
    covers: [the size on each display]
  - operator: aaronsb
    said: "Shrink the grid"
    via: the steering session's interview on 2026-11-22 (question tool, brief/for/against/lean per probe); the figure ADR-1404-joined-displays.svg was rendered and shown first; the maintainer chose the lean
    covers: [an unplugged display]
observable:
  - "A head with `arrange = stacked` over two displays of the same size shows one grid with the rows of both, and a line that a program writes on the last row of the top display continues on the first row of the bottom one"
  - "The same head with `arrange = side-by-side` shows one grid with the columns of both, and a line wraps only at the right edge of the right display"
  - "Unplugging one of the two displays resizes the grid to the display that remains, and the shell redraws its prompt at the new size; plugging the display in again gives back the joined grid"
  - "A head with no `arrange` key mirrors its displays as it does today"
status: proposed
date: 2026-11-20
deciders:
  - aaronsb
related:
  - ADR-1200
  - ADR-1300
---

# ADR-1404: A head can join its displays into one larger terminal, stacked or side by side, and mirroring stays the default

## Summary

- **Decided:** A head can join its displays into one larger terminal: stacked, one above the other, for more rows, or side by side for more columns. A head's section in vtium.conf names the arrangement with `arrange = stacked` or `arrange = side-by-side`, and names the order of the displays with `display-order`. Mirroring stays the default, so a head with no `arrange` key works as it does today. A joined head has one grid, one set of consoles and one cursor. Each display draws the rows or columns that fall on it, at a font height of its own, chosen so that text is about the same physical size on every display. When one of the joined displays is unplugged, the grid shrinks to the displays that remain, as a resize does, and grows back when the display returns. The maintainer chose both on 2026-11-22, and the build follows in the alpha phase.
- **Trades away:** A joined head shows nothing twice, so a person who wants the same text on a second display keeps mirroring for that head. The seam between two displays is a bezel the text crosses, and on displays of different density the rows or columns line up only to within a pixel or two. Unplugging a joined display resizes every program on the head.
- **One-way?** No. `arrange` is a new key with mirroring as its default, so a configuration that names no arrangement never changes. Once people keep joined heads in their vtium.conf, renaming the key or its values costs them an edit (ADR-1510).
- **Probes:**
  - *Not confident (the size on each display):* on two displays of different density, each display draws text at the same physical size, so a cell has a different number of pixels on each. The other way keeps one cell size in pixels on every display, so the grid lines up exactly but text is smaller on the denser display. Is the same physical size what you want?
  - *Not confident (an unplugged display):* when one joined display is unplugged, the grid shrinks to the displays that remain, and programs redraw at the smaller size. The other way keeps the grid as it was, and the rows or columns of the missing display go on unseen until it returns. Is shrinking what you want?
- **Inversion:** Between "a head mirrors every display it holds", as vtium does today, and "every display is a head of its own", where a person who wants a taller terminal runs one terminal per display. A joined head is one terminal across several displays, and mirroring stays the default.

## Context

A head is a terminal for one or more physical displays, with its own grid and consoles (ADR-1401 D1). A head that holds several displays mirrors one grid across all of them: the grid is the smallest that every display holds, and each larger display draws the same grid at a larger font height (`crates/vtium/src/layout.rs`, `basis`). The maintainer asked for the other use of several displays: "two screens physically laid out one above the other, and forming a single terminal with more rows" (`basis`). Side by side is the same idea turned a quarter, for more columns. The maintainer decided on 2026-11-20 that this record is written now and that the build is part of the plan to 0.1.0 (`basis`).

Several words recur below. A head's displays are **joined** when each shows a different part of one grid, and **mirrored** when each shows the whole grid. The **seam** is the edge where two joined displays meet. The **shared side** is the side that every joined display must hold the same number of cells along: the columns of a stacked head, and the rows of a head side by side.

## Decision

### D1: the arrangement, in vtium.conf

- **`arrange` in a `[head NAME]` section** names how the head lays out its displays: `mirror`, the default, `stacked`, or `side-by-side`. A head with one display ignores the key. vtium-settings checks the value as it checks every head key (ADR-1403 D5), so an unknown value is an error in that head's section.
- **`display-order` names the order of the joined displays,** first on top for `stacked` and first on the left for `side-by-side`. Each entry is a display's label from vtium.conf's `[labels]` section, or its connector, such as `DP-8` (ADR-1403 D4), separated by commas, as in `display-order = Top monitor, Bottom monitor`. A display that the head holds and the list does not name follows the named ones in the order of its connector's name, and `vtium check-config` warns about it. A display that the list names and that is not connected is skipped.
- **The rules stay the same.** Display rules still send each display to a head (ADR-1401 D3). `arrange` and `display-order` only say how the head lays out the displays its rules give it.
- The two keys are this record's agent's call. The other way is one key that holds both, such as `arrange = stacked: Top monitor, Bottom monitor`, which keeps the arrangement in one line but packs a list into a value that is otherwise a word. Two keys keep each value simple, and a person can change the order without touching the arrangement.

### D2: one grid across the displays

- **A joined head has one grid, one set of consoles, one cursor and one scrollback.** A program sees one terminal of the joined size, and a resize of the joined grid reaches it as any resize does, through SIGWINCH and the reflow of ADR-1003.
- **The shared side is the smallest any display holds.** A stacked head's columns are the fewest columns any of its displays holds, and its rows are the sum of each display's rows. A head side by side takes the fewest rows and the sum of the columns.
- **Each display draws its own part of the grid,** the rows or columns that fall on it, and nothing else. A row never splits across a seam: each row of a stacked head lies wholly on one display, and each column of a head side by side lies wholly on one display.
- **The cursor, the selection and the pointer cross the seam.** The pointer moves from one display into the next across the joined edge, and a selection can run from a cell on one display to a cell on another.
- **vtium's on-screen display draws on the first display of the order,** so that a message never lands across a seam. The identify overlay still names each display of a joined head on that display. This is this record's agent's call.

### D3: how rows and columns line up on displays of different size and density

- **Each display draws at its own font height,** as a mirrored head's displays already do (`layout.rs`). For a joined head the height is the one that gives a cell about the same physical size on every display, from each display's density and the head's font size in points. Text then reads the same size on both sides of the seam, and a line that crosses a seam keeps its size. The maintainer chose this on 2026-11-22 (`considered`). The other way is one cell size in pixels on every display, so the cells line up exactly, but text is smaller on the denser display.
- **Pixels that fill no whole cell lie at the outer edges,** away from the seam, in the console's background color. On a stacked head the top display's spare rows of pixels lie at its top and the bottom display's at its bottom, so the rows meet at the seam with no gap but the bezel. Across the shared side, each display centres its cells, so the columns of a stacked head line up about the middle of each display.
- **A display that holds fewer cells along the shared side** than another sets the shared side for the head, and the larger display leaves its extra width, or height, in the background color, centred. vtium-config notes when a joined head leaves more than a few cells unused this way, because the person may want to swap the order or the displays.
- **A bitmap font draws at a whole multiple of its own size on each display** (ADR-1200 D17), so the physical sizes on two displays match only as closely as the whole multiples allow. The rule takes the multiple on each display that comes closest to the same physical size.

The figure shows both arrangements with the numbers of two examples. On the left, a stacked head joins a 1920×1080 monitor above a 1280×720 one at the same density, so the bottom monitor sets the shared side at 106 columns. On the right, a head side by side joins two panels of the same physical size, one at twice the density of the other, so a cell has twice the pixels on the right and text reads the same size across the seam.

![Two joined heads. On the left, arrange = stacked with display-order = Top monitor, Bottom monitor: the top monitor, 1920×1080, holds rows 1 to 45 in cells of 12×24 pixels, with spare pixels on its left and right because the bottom monitor, 1280×720, holds only 106 columns; the bottom monitor holds rows 46 to 75 in cells of the same size; a dashed line marks the seam between them, and a line under the bottom monitor marks 106 columns as the shared side. On the right, arrange = side-by-side with display-order = Left panel, Right panel: the left panel, 1920×1080 at 96 DPI, holds columns 1 to 160 in cells of 12×24 pixels; the right panel, 3840×2160 at 192 DPI, holds columns 161 to 320 in cells of 24×48 pixels; one row runs across the dashed seam at the same physical size on both sides, and the shared side is 45 rows, so the grid is 320×45. Notes beside it say that each display draws only its own rows or columns at a font height of its own, that spare pixels lie at the outer edges in the background color, and that unplugging one display shrinks the grid while plugging it in grows it back.](figures/ADR-1404-joined-displays.svg)

### D4: a display that leaves and comes back

- **When one joined display is unplugged, the grid shrinks** to the displays that remain, as a resize does: the programs see the new size, and the scrollback rewraps as ADR-1003 decides. When the display returns and its rule still matches, the head joins it again in its place in `display-order`, and the grid grows back. The maintainer chose this on 2026-11-22 (`considered`). The other way keeps the grid and leaves the missing display's part unseen, so programs never see a resize, but a person on the remaining display cannot see where the cursor is.
- **A display left off,** which stays present but dark (ADR-1401 D8), counts as unplugged here, because the person cannot see its part.
- **When the head's last display leaves,** `when-display-lost` applies as it does to every head (ADR-1401 D8).

### D5: vtium-config

- **The Flow view shows a joined head's displays in their arrangement:** stacked one above the other, or side by side, in the order of `display-order`, each with its label or connector and the cells it holds. It marks the display that vtium-config runs on, which vtium-config finds from the head it runs on (ADR-1401 D12).
- **The head's card in the Flow view sets `arrange` and `display-order`,** and `vtium config head set NAME` sets them from the command line, as it sets the other head keys (ADR-1403 D8).

### D6: how it is judged

- vtium's unit tests drive `layout.rs` with joined heads of two and three displays, of the same and of different sizes and densities, and check the grid, each display's rows or columns and font height, and where the spare pixels lie.
- A tier 3 variant runs a guest with two displays, stacked and then side by side, and checks the screenshots against goldens of its own, provisional until the maintainer accepts them. It unplugs one display and plugs it in again, and checks that the grid shrinks and grows back.
- A VT tour step shows a joined head on the maintainer's two displays.

## Consequences

### Positive

- Two displays can make one taller or wider terminal, which suits a portrait pair, a stack of small panels, or a wide desk.
- Mirroring stays the default, so no existing configuration changes.
- The rule for each display's font height already exists for mirroring, and a joined head reuses it with a different shared side.

### Negative

- The seam is a bezel that text crosses, and on displays of different density the cells line up only to within a pixel or two.
- Unplugging a joined display resizes every program on the head, which a full-screen program redraws and a long scrollback rewraps.
- `layout.rs`, the renderer's damage and the pointer each gain a second way to map a cell to a display.

### Neutral

- The tier 3 case that unplugs and plugs in a mirrored display stays as it is, and joined heads get a case of their own.
- A grid of displays in two dimensions, such as four displays in a square, is not part of this record. `arrange` can gain a value for it later.

## Alternatives Considered

- **One head per display, and a multiplexer across them.** Not taken: a program cannot span two heads, and the maintainer asked for one terminal with more rows.
- **Join by default when a head holds several displays.** Not taken: it would change every configuration that mirrors today, and the maintainer's question kept mirroring as the shipped default.
- **Order the displays by the order of the display rules.** Not taken: one rule such as `display * -> desk` sends several displays to a head with no order between them, so the order needs a key of its own.
- **Let a row split across the seam.** Not taken: half a character on each side of a bezel reads worse than a whole row on each display.
