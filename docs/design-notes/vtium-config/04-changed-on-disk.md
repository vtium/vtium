# When the file changed on disk

Where this chapter starts: the save of 2026-11-28 at 09:40 wrote `font-size = 11` into `/etc/vtium/vtium.conf`, and the console draws 11 pt: `padnoir · 4 problems`. History holds one version, the file of 2026-11-12 08:53.

## Two changes at once

vtium-config is still open. At 10:05, in a shell on another VT, the person runs `sudoedit /etc/vtium/vtium.conf` and changes `osd = full` to `osd = minimal`, so that vtium's on-screen notices take less room. That edit happens outside vtium-config, and padnoir knows nothing of it yet.

Back in vtium-config, 11 pt still looks small. The person opens My-Look's card again and picks 12 pt in the Size line's list box. The top row counts one change not saved, and Ctrl+S opens Save.

## The review shows the other change

The installed file is no longer the one padnoir was made from. Every editor meets this case, a file changed on disk while someone edited it. So the review shows that file's change beside the person's own: root's change of `osd` at line 8, then the person's change of `font-size` at line 15. A line says whether the two touch the same lines, and here they do not. [ADR-1403](../../architecture/platform/ADR-1403-vtium-conf-holds-the-configuration-as-one-flow-from-the-displays-to-the-input-devices-and-vtium-config-draws-it.md) D8 sets the choices.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf

 The file changed on disk since you opened it. Its change, by root at 2026-11-28 10:05:
      7  font-name = cullet-mono
    8 -osd = full
    8 +osd = minimal
 Your changes do not touch those lines.

 Your change: 1, as Show changes lists it.
     13  [look My-Look]
   15 -font-size = 11
   15 +font-size = 12

   [u] Update      bring the file's change into yours, then review again
   [o] Overwrite   save yours as they are; the version they replace stays in History
   [r] Revert      reload the file from disk and drop your changes

 When the installed file is missing, nothing can be overwritten, so this screen does not
 appear, and Save shows its review as usual.







────────────────────────────────────────────────────────────────────────────────────────────────────
 [u] Update  [o] Overwrite  [r] Revert  [w] Whole file  [Esc] Cancel           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab. In color, the first line is amber, `-` rows red and `+` rows green.*

Four choices follow, each a button of the action bar:

- `[u]` Update brings the file's change into the document. It merges three ways, from the file padnoir was made from, padnoir itself and the installed file, and then shows the review again.
- `[o]` Overwrite saves the document as it is, over the file's change. The file it replaces stays in History, so File › History can bring it back.
- `[r]` Revert asks first, then starts the document again from the installed file and drops the person's changes.
- Esc cancels and changes nothing.

`[w]` means Whole file here as it does on every screen, so Overwrite takes `o`.

The person presses `[u]`. The merge is one step of the undo history, so Ctrl+Z could take it back. The review shows again, against the installed file as root left it. Only the size differs from it now, so the review counts one change.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf                                           1 change · fonts unchanged

 Updated: the file's change, osd = minimal, is in yours now, and nothing of yours was lost.

 @@ [look My-Look]
     13  [look My-Look]
     14  palette = vga
   15 -font-size = 11
   15 +font-size = 12
     16  font-name = cullet-mono

 4 warnings (saving still works): 1 font My-Look uses is not active, Noto Color Emoji fails
 the trust rule, the layer nerd-symbols names a font that is not installed, and 1 keyboard and
 2 pointers are in no path. [Alt+P] lists them.

 Each terminal uses the change at its next login. sudo asks for your password once.










────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+S] Save  [w] Whole file  [Alt+P] Problems  [Esc] Cancel                 [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab. In color, Updated is green and the warnings' heading amber.*

Ctrl+S saves through sudo, as in [chapter 3](03-saving.md). vtium-config records the installed file as the person saw it, so the root side still refuses a file that changes again after this review. The person restarts vtium on VT 1, and the console draws 12 pt, with 11×24 cells and 174×45 characters.

History keeps each file that a save replaced. This save replaced root's file of 10:05, so History now lists that file and the one of 2026-11-12. The version saved at 09:40 is not in it, because root's edit replaced that one outside vtium-config.

## When both sides changed the same line

Had root also changed My-Look's size, say back to 10 pt, Update would list that line to settle, as a merge tool lists a conflict. The installed line sits above the person's. `[m]` keeps the person's line and `[i]` takes the installed one, one line at a time, and the count at the right says how many are settled. Save leaves the action bar until every line is settled, and Overwrite and Revert work as before.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf

 The file changed on disk since you opened it, and 1 line changed on both sides.  0 of 1 settled

     13  [look My-Look]
     14  palette = vga
 installed  15  font-size = 10               by root at 2026-11-28 10:05
 mine       15  font-size = 12

 [m] keeps mine and [i] takes the installed line, one line at a time.

 Its other change does not touch yours, and comes into yours with it:
    8 -osd = full
    8 +osd = minimal

 Save appears once every line both sides changed is settled. Overwrite and Revert work as above.










────────────────────────────────────────────────────────────────────────────────────────────────────
 [m] Mine  [i] Installed  [o] Overwrite  [r] Revert  [Esc] Cancel              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 9, `installed  15  font-size = 10  by root at 2026-11-28 10:05`.*

Once the last line is settled, Save comes back to the bar. It saves the person's lines with the file's other change in them, after one more review.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf

 The file changed on disk since you opened it, and 1 line changed on both sides.  1 of 1 settled

     13  [look My-Look]
     14  palette = vga
   15  font-size = 12                     settled: mine

 [m] keeps mine and [i] takes the installed line, one line at a time.

 Its other change does not touch yours, and comes into yours with it:
    8 -osd = full
    8 +osd = minimal

 [Ctrl+S] saves your changes with the file's change in them, then review again.











────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+S] Save  [m] Mine  [i] Installed  [o] Overwrite  [Esc] Cancel           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab.*

On the command line, `vtium config update` merges the installed file's change into the draft, and stops with a list of the lines that both sides changed. `vtium config install --overwrite` installs over the change. Both forms, and the `o` key, are calls that the steering session made for the maintainer's morning list.

Where it leaves you: the save of 2026-11-28 at 10:20 holds `osd = minimal` and My-Look at 12 pt, and the console draws 12 pt: `padnoir · 4 problems`. History lists two versions, the file root left at 10:05 and the file of 2026-11-12 08:53.
