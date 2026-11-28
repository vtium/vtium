# vtium-config in use

This book shows how a person uses vtium-config's full-screen tool, the TUI that `vtium config tui` opens, as 0.1.0-alpha.13 builds it. It follows one person through every screen in the order they meet them, from the first run to a live macro.

**The book is a reference, not a golden.** A golden is a saved result that a test compares the output with. Nothing fails when the TUI differs from this book. A difference is reviewed, and then either the book or the code changes.

## Where the book comes from

The screens come from the mock-up catalog of every screen and menu item, which the maintainer accepted for the alpha.13 rebuild. The book follows the records the catalog was drawn for:

- [ADR-1403](../../architecture/platform/ADR-1403-vtium-conf-holds-the-configuration-as-one-flow-from-the-displays-to-the-input-devices-and-vtium-config-draws-it.md) D8 and D11: the configuration as one document, Save, and the frame;
- [ADR-1205](../../architecture/font/ADR-1205-a-look-is-assembled-from-the-active-resources-and-a-resource-can-be-activated-only-from-a-root-owned-folder-on-a-filesystem-that-stays-mounted.md) D4: fonts that a configuration uses but has not activated;
- [ADR-1206](../../architecture/font/ADR-1206-vtium-config-s-tui-draws-through-ratatui-s-core-and-widgets-over-its-own-terminal-layer-and-each-tab-is-a-module-that-one-navigation-module-and-one-update-loop-connect.md): how the TUI's code builds the screens;
- [ADR-1208](../../architecture/font/ADR-1208-vtium-config-s-tui-plays-macros-short-scripts-of-keys-and-checks-in-a-subset-of-vhs-s-tape-format-on-a-scratch-copy-unless-a-live-run-is-asked-for-and-allowed.md): the macro player;
- [ADR-1304](../../architecture/render/ADR-1304-vtium-s-frame-filters-are-built-in-and-a-vtlook-file-chooses-them-and-sets-their-parameters.md): the shader filters, which [chapter 5](05-looks-and-effects.md) shows as planned.

Where a screen of the catalog and a record differ, the book draws what the record says.

## How to read a screen

Each screen is a text block of 100×30 characters, the size vtium-config is designed for. A text block cannot show bold, underline, color or a highlight. So the line under each block names the letters drawn bold and underlined and the highlighted rows, and says where color carries meaning. The top row's Alt letters are the same on every screen: F, A, n, E, L, o, S and H, for File, Actions and the six tabs.

Every key a screen names for pressing is written in brackets, as `[Ctrl+S] Save`. The action bar along the bottom draws each action as its key and its label, and a click on either does what the key does. Where the row cannot hold every action, its last button is `[.] More`, which opens the rest as a small menu.

## The person and the machine

One person uses vtium-config on padnoir, a laptop with one built-in display, eDP-1, and its own keyboard. They keep one configuration, padnoir, from chapter to chapter, and [chapter 9](09-history-and-configurations.md) adds travel and kiosk-test beside it. Each chapter opens with the line "Where this chapter starts:" and closes with "Where it leaves you:", so it can be read on its own. The top row's facts and History's entries carry from one chapter to the next.

## Contents

| | Chapter | What the person does | The top row at its end |
|---|---|---|---|
| 1 | [Opening vtium-config on a first run](01-first-run.md) | runs vtium-config for the first time and meets the frame, the tabs and the menus | `padnoir · 4 problems` |
| 2 | [Choosing a font and its size](02-font-and-size.md) | looks through the fonts, sets My-Look to 11 pt, undoes and redoes it, and tries it live | `padnoir ● 1 not saved · 4 problems` |
| 3 | [Saving, and what Save shows](03-saving.md) | comes back to the change, reads Show changes and Save's review, and saves | `padnoir · 4 problems`, 11 pt saved |
| 4 | [When the file changed on disk](04-changed-on-disk.md) | saves 12 pt while root has changed the file by hand, and merges the two | `padnoir · 4 problems`, 12 pt saved |
| 5 | [Looks and effects](05-looks-and-effects.md) | reads the Looks tab's lists and the Effects tab, makes the look Evening, and saves | `padnoir · 4 problems` |
| 6 | [Wiring displays and inputs in Flow](06-flow.md) | gives console VT 1 and adds a second terminal, work, on VT 2, and saves | `padnoir · 4 problems` |
| 7 | [Settings](07-settings.md) | changes a setting and edits the text by hand, then reverts both | `padnoir · 4 problems` |
| 8 | [Finding things](08-finding-things.md) | filters lists with `?`, finds with Ctrl+F, and reads F1 and the Help tab | `padnoir · 4 problems` |
| 9 | [History and several configurations](09-history-and-configurations.md) | views and uses an earlier version, makes kiosk-test and travel, and deletes kiosk-test | `padnoir · 4 problems` |
| 10 | [Problems and fonts not yet active](10-problems-and-fonts.md) | goes through the Problems list and activates the font My-Look was missing, and saves | `padnoir · 3 problems` |
| 11 | [The macro player and live runs](11-macros.md) | rehearses a macro on a scratch copy, then plays it live to set 11 pt again | `padnoir · 3 problems`, 11 pt saved |

Later, each chapter will carry the macro that plays it, so that the real screens can be set beside the book's for review.
