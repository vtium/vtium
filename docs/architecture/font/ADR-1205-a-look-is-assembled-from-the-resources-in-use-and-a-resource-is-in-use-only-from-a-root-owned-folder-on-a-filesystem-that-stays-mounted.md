---
contract: adr/v1
kind: decision
verb: change
capability: [glyphs, platform]
basis:
  - operator: aaronsb
    level: directed
    said: "Resources (font files, color maps such as .cube tables, shaders) feed a look, a look feeds a terminal in the Flow view, and the terminal is wired to displays and inputs. vtium-config's tabs follow that graph: Fonts for the font resources, Looks for sizes, font sets assembled from fonts and effects, then Flow. The Fonts tab returns as a tab of its own."
    via: the steering session's brief of 2026-11-18, relaying the maintainer's decisions of that day on fonts, looks and effects; the wording is the steering session's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "The backend and the records keep the glassmaking words of why-vtium: a resource is a cullet, the font set stays a frit, and generating glyphs stays vitrify. On screen and in pages for users, vtium says font, effect, font set and look."
    via: the same brief, the same decisions
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "The maintainer dislikes the word tag for picking items, which the Flow view once used and retired."
    via: the same brief, which offers batch and Use as the steering session's proposal for the maintainer to confirm
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "vtium-config discovers fonts in well-known folders and sorts them with simple filters, so nobody types raw paths. A user's own fonts are listed and marked not usable, because the user installed them and the system did not. Previewing one in vtium-config's own window is allowed; trying it live on the console needs a system font."
    via: the same brief, the same decisions
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "Use is trust. A resource can be put in use only from a root-owned folder on a filesystem mounted at boot that stays mounted for the system's life. A hand-written path is always allowed and gets the same checks. A symlink, a filesystem that fails the rule, or the wrong owner or permissions make the file count as one that does not exist. The built-in font is always in use; Cullet Mono is in use by default and can be taken out. The selection lives in a root-owned file under /etc/vtium, written by vtium-config's sudo-backed install, so a draft's changes to it install with the draft."
    via: the same brief, the same decisions
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "A font reaches the machine as a system font package, as the person's own copy made as root (ideally under /usr/local/share/fonts), or through vtium-config's explicit install-as-a-system-font step, which copies into /usr/local/share/fonts and never /usr/share/fonts, through the existing sudo-backed install, root-owned with mode 0644, checked afterwards, showing the exact command, confirmed each time, and never as a side effect of putting a font in use."
    via: the same brief, the same decisions
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "A look is assembled by role, extending ADR-1204's roles without new terms: text, box drawing, symbols and icons, scripts by name, emoji, fallback. One font may fill every role. A look also takes the effects in use. A look offers only the sizes its fonts supply; a font-size typed in vtium.conf still works, and check-config warns when the look's fonts cannot supply it. Viewing a font shows what it covers and its sizes, and in a look what the look would still miss. Taking a font out of use warns and lists the looks that use it, and those looks draw without it."
    via: the same brief, the same decisions
    paraphrase: true
  - evidence: "Read 2026-11-18 on the agent's machine: /etc/fonts/fonts.conf of fontconfig 2.18.3 lists the font folders /usr/share/fonts, /usr/local/share/fonts, the folder fonts under the XDG data home (~/.local/share/fonts by default) and ~/.fonts, the last marked as to be removed in the future. /usr/share/fonts holds no symbolic link. The Adwaita cursor theme's folder /usr/share/icons/Adwaita/cursors holds 35 files and 28 symbolic links."
  - precedent: ADR-1200
  - precedent: ADR-1201
  - precedent: ADR-1204
  - precedent: ADR-1304
  - precedent: ADR-1309
  - precedent: ADR-1401
  - precedent: ADR-1403
amends:
  - "ADR-1200#D17: the cell"
  - "ADR-1200#D25: `font-paths`, the folders vtium searches for fonts"
  - "ADR-1201#D3: the TUI"
  - "ADR-1201#D5: writing"
  - "ADR-1204#D6: each source has a role, and the roles come in one order"
  - "ADR-1204#D8: how each font reaches the machine"
  - "ADR-1304#D11: vtium-config manages vtlooks"
  - "ADR-1403#D7: the Flow view"
  - "ADR-1403#D8: what vtium-config writes"
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "vtium-config's tabs read Fonts, Looks, Flow, Settings and Help, and vtium-config opens on Flow"
  - "The Fonts tab lists a font from ~/.local/share/fonts, marked as installed by the user and not usable, previews it in vtium-config's own window, and offers to install it as a system font"
  - "A font file in /usr/share/fonts that is a symbolic link, or that sits on a USB stick or an NFS mount, cannot be put in use; vtium draws without it, logs one line, and vtium check-config names the reason"
  - "Taking Cullet Mono out of use warns, lists each look that uses it, and those looks then draw as when Cullet Mono is missing; the built-in font cannot be taken out of use"
  - "Installing a font as a system font shows the exact command, asks each time, and leaves the file in /usr/local/share/fonts owned by root with mode 0644"
  - "A look whose only font is bitmap Terminus offers the sizes of Terminus's strikes and no other; font-size = 13 in vtium.conf still draws, and vtium check-config warns that the look's fonts do not supply it"
status: proposed
date: 2026-11-18
deciders:
  - aaronsb
related:
  - ADR-1200
  - ADR-1201
  - ADR-1204
  - ADR-1301
  - ADR-1304
  - ADR-1309
  - ADR-1401
  - ADR-1403
---

# ADR-1205: A look is assembled from the resources in use, and a resource is in use only from a root-owned folder on a filesystem that stays mounted

## Summary

- **Decided:** vtium's configuration is a graph from resources to screens. Font files, color maps and shaders are resources. A look is assembled from the resources in use, a terminal in the Flow view takes a look, and the terminal is wired to its displays and inputs. vtium-config's tabs follow the graph: Fonts, Looks, Flow, then Settings and Help. The Fonts tab returns as a tab of its own. It finds fonts in well-known folders and sorts them with simple filters, so nobody types a raw path. A resource can be put in use only from a root-owned folder on a filesystem that stays mounted for the system's life. A file that fails that rule counts as a file that does not exist. A user's own fonts are listed and can be previewed in vtium-config's window, but cannot be put in use. vtium-config can install such a font as a system font, in an explicit step that the person confirms each time. A look fills the roles of ADR-1204, now split finer: text, box drawing, symbols and icons, each script by name, emoji, and fallback. A look offers only the sizes its fonts supply. In the records, a resource is a cullet and the selection in use is the batch. On screen, vtium says font, effect, font set and look, and a resource is "in use" or not.
- **Trades away:** A font that the person put in their home folder no longer draws on the console, even when a folder of `font-paths` names it. A font installed as a symbolic link, or on removable, network or automounted storage, never draws. A frit that names a font by its path draws that font only while the batch holds it, so a font now passes two gates, being installed and being in use. The Flow view no longer makes a look from a font set and a size, so a person who wants a new look goes to the Looks tab first.
- **One-way?** No. The batch is one root-owned file, the trust rule is a check before a file is read, and the tabs are vtium-config's layout. A session of work can change any of them. The words cullet and batch are the costly part to change once code and pages use them.
- **Probes:**
  - *Not confident (the words):* the records call a resource a cullet and the selection in use the batch, and the screen says a font is "in use", toggled with Use. "Batch" and "Use" are the steering session's proposal, since you dislike "tag". Are they the words you want? Lean: yes. A glassmaker's batch is the materials picked for one melt, and "in use" says what the toggle does.
  - *Not confident (a font installed twice):* a font the person installed as a system font in `/usr/local/share/fonts` may later arrive from a package in `/usr/share/fonts`. Which copy is in use? Lean: the package's copy, because the package keeps it current. vtium-config points out the local copy as a duplicate that the person can remove.
  - *Not confident (pointer themes):* a desktop cursor theme is a resource too, and its folder holds symbolic links, such as Adwaita's `xterm` to `text`. The trust rule refuses a link, while ADR-1309 D5's walk follows one. Are pointer themes exempt from the link rule? Lean: yes, so ADR-1309 D5 stands as written. Its walk already checks every folder it crosses, and no cursor theme works without its links.
  - *Not confident (what is in use on a fresh install):* you named the built-in font, always in use, and Cullet Mono, in use by default. The default set `terminus` draws bitmap Terminus and Unifont from files that every vtium build installs. Are those in use by default too? Lean: yes, together with the console font that the system's own console configuration names (vconsole's `FONT=`), so a fresh install draws as it does today. Every other font, those of vtium-fonts included, waits for Use.
  - *Confident (the tab order):* the tabs read Fonts, Looks, Flow, Settings and Help, in the order of the graph, and vtium-config still opens on Flow. Lean: yes, since your decision lists them in that order.
- **Inversion:** The decision sits between two ends. At one end, vtium draws any font that any folder holds, and a person types paths to name them. At the other, vtium draws only what it ships. vtium draws from a curated batch of resources that the system installed, finds those resources by itself, and lets the person install a missing one as a system font.

## Context

vtium-config has grown its font and look tools one view at a time. A look is a `[look NAME]` section of vtium.conf that names a font set, a size and a vtlook (ADR-1403 D1, ADR-1304 D4). The Looks tab of ADR-1201 D3 took in the Fonts view on 2026-11-16. The Flow view of ADR-1403 D7 makes a look on its own whenever a path's font set or size changes. A person names a font by its family in a frit, or by a path, and `font-paths` adds folders to search (ADR-1200 D25). A user's own font folder can be added there, and the font helper's sandbox leaves reads open, because a frit may name a font anywhere.

That leaves three gaps. First, the screens do not follow how the parts depend on each other. A person meets the font sets inside the Looks tab and the looks inside the Flow view, with no single place for the fonts themselves. Second, nothing says which files vtium may trust as resources. vtium.conf and the frits pass an ownership check (ADR-1401 D10), and so does a pointer theme (ADR-1309 D5). A font file named in a frit, a `.cube` table and a shader each pass their own checks or none. Third, the person types font names and paths without seeing what the system holds or what a look still lacks.

The maintainer decided on 2026-11-18 that the configuration is one composition graph, from resources to screens, and that putting a resource in use is an act of trust. This record states those decisions and amends the records they touch.

## Decision

### D1: the composition graph

vtium's configuration is a graph from resources to screens:

| Stage | What it is | Where vtium-config edits it |
|---|---|---|
| Resources | font files, color maps such as `.cube` tables, and shaders | the Fonts tab, for fonts; the Looks tab, for color maps and shaders |
| A look | a font set assembled from fonts in use, its sizes, and the effects in use (D6, D7) | the Looks tab |
| A terminal | a head, which takes one look (ADR-1401 D1) | the Flow view |
| Displays and inputs | the devices wired to the terminal by rules (ADR-1403 D7) | the Flow view |

vtium-config's tabs follow the graph, in this order: Fonts, Looks, Flow, then Settings and Help. vtium-config still opens on Flow, where most configuration work happens (ADR-1201 D3).

- **Fonts** manages the font resources: it finds them, filters them, shows what each covers, puts each in use or takes it out, and installs a user's font as a system font (D3, D5, D8). The Fonts tab returns as a tab of its own.
- **Looks** assembles each look by role from the fonts in use, offers the sizes those fonts supply, and adds the effects in use (D6, D7). It keeps its form for a new font set.
- **Flow** wires terminals to displays and inputs. A terminal takes a look from the Looks tab. The Flow view no longer makes a look itself.

### D2: the words

The backend and the records keep the glassmaking words of [Why VTium](../../why-vtium.md#why-glass). Pages for users and every string on screen use the everyday words.

| In the records and the code | On screen and in pages for users | What it means |
|---|---|---|
| cullet | font, effect, color map, shader | one resource: a font file, a color map such as a `.cube` table, or a shader |
| batch | the fonts and effects "in use" | the set of resources that vtium may draw from |
| put in the batch, take out of the batch | Use, and taking a font out of use | the toggle that adds a resource to the batch or removes it |
| frit | font set | a font set file, as ADR-1200 D1 defines it |
| vitrify | (not shown) | generating glyphs from fonts |
| look | look | a `[look NAME]` section of vtium.conf |

A glassmaker's cullet is broken glass that goes back into the furnace with the raw materials ([Wikipedia, Cullet](https://en.wikipedia.org/wiki/Cullet)). A glassmaker's batch is the raw materials picked and weighed for one melt. vtium's batch is likewise the resources picked for the console. This record defines cullet once. The word already sits in the name of vtium's outline font, Cullet Mono, and of the test set `cullet-alone`. In this record's sense, any resource is a cullet, Cullet Mono being one.

The batch of this record is not the batch of ADR-1200 D18, which is a group of 1,024 code points that the font helper fills at once. A record that means D18's batch says "a batch of code points".

"Batch" and "Use" are the steering session's proposal, for the maintainer to confirm (Summary). The maintainer dislikes "tag", which ADR-1403 once used for picking items with Space in the Flow view and has retired.

### D3: vtium-config finds the fonts

vtium-config finds fonts in well-known folders, so nobody types a raw path:

- **The system's folders:** `/usr/share/fonts` and `/usr/local/share/fonts`, which are fontconfig's default folders for the system, `/usr/share/vtium/fonts`, where vtium installs its own font files, and kbd's console-font folders (ADR-1200 D25).
- **The folders of `font-paths`,** where vtium.conf sets it (ADR-1200 D25).
- **The user's folders:** `~/.local/share/fonts` and `~/.fonts`, which fontconfig searches for a user's own fonts.

The Fonts tab sorts what it finds with simple filters: bitmap or outline, monospaced or not, the scripts each font covers, and other well-known groups where a font states one, such as a Nerd Fonts build or a color emoji font. vtium-config reads a font's family, coverage and sizes only through the sandboxed font helper, never in its own process (ADR-1201 D3).

**A user's own fonts** are listed and marked as not usable, because the user installed them and the system did not. The Fonts tab says so, so the person knows that they would install the font as a system font (D5) to use it. vtium-config may preview such a font in its own window, because the font helper parses it in the sandbox (ADR-1201 D3, ADR-1301). Trying a font live on the console, with ADR-1201 D4's live trial, needs a system font in use.

Color maps and shaders keep the folders that ADR-1304 gives them, and the Looks tab lists them (D6).

### D4: putting a resource in use is an act of trust

vtium draws only from the resources in use. A font that a frit names, by its family or by its path, draws only while the batch holds it. Otherwise it counts as missing, and its characters draw from the next source, as for any missing font (ADR-1204 D7). A color map or a shader that a vtlook names counts as missing unless the batch holds it.

**The rule.** A resource can be put in use only when all of these hold:

- The file sits in a folder that root owns, and root owns the file.
- Neither the group nor other users can write the file or any folder above it, as ADR-1401 D10 asks of vtium.conf.
- The file and the folders on the way to it are not symbolic links.
- The filesystem that holds it is mounted at boot and stays mounted for the system's life. Removable media, a network filesystem and an automounted filesystem fail this rule.

A path that the person writes by hand is always allowed, in the batch or in a frit's `source=` line, and it passes the same checks. A file that fails any check counts as a file that does not exist. vtium draws without it and logs one line for it, once. `vtium check-config` and vtium-config name the file and the check it failed.

The program that reads a resource checks the rule on the file it opened, so the file cannot be swapped between the check and the read. When vtium runs unprivileged, as the tests and a run by hand do, it accepts its own effective user as the owner, as ADR-1401 D10 does for vtium.conf. These two points are this record's agent's calls. How vtium tells a filesystem that stays mounted from one that does not is left to the work that builds the rule, which reads the system's mount table.

**What is always in use.** The built-in font (ADR-1204 D1) is always in use, and the person cannot take it out. Cullet Mono is in use by default, and the person can take it out. Which other fonts are in use on a fresh install is a probe in the Summary.

**Where the batch lives.** The batch is a root-owned file under `/etc/vtium`. The name `/etc/vtium/batch.conf`, with one path to a resource on each line, is this record's agent's call. vtium-config writes the batch through its sudo-backed install, as it writes vtium.conf (ADR-1201 D5). A draft keeps its changes to the batch beside the draft, as it keeps its font set changes (ADR-1403 D8), so the changes install with the draft.

**This rule sits beside ADR-1401 D10.** D10 keeps its scope: vtium.conf and the frits, which route input and choose login commands. This rule covers the resources a look draws from.

### D5: how a font reaches the machine

A font reaches the machine as a system font in one of three ways:

- **A system font package,** which installs into a root-owned folder such as `/usr/share/fonts`.
- **The person's own copy, made as root,** ideally in `/usr/local/share/fonts`.
- **vtium-config's "install as a system font" step,** for a font the Fonts tab lists from a user's folder.

The install step copies one font file into `/usr/local/share/fonts`, and never into `/usr/share/fonts`, which the packages own. It runs through vtium-config's existing sudo-backed install (ADR-1201 D5). The copy is owned by root with mode 0644, and vtium-config checks it afterwards against D4's rule. vtium-config shows the exact command before it runs it, and asks the person to confirm each time. Putting a font in use never installs it as a side effect. A user's font is first installed as a system font, then put in use.

**A font installed twice** is an open question for the maintainer (Summary). When a package later installs the same font in `/usr/share/fonts`, the lean is that the package's copy is in use, and vtium-config points out the local copy as a duplicate.

### D6: a look is assembled by role

A look's font set is assembled by role. The roles extend ADR-1204 D6's, with no new terms:

| Role | What fills it | Change from ADR-1204 D6 |
|---|---|---|
| Text | the text font, with its bold and italic faces | as before |
| Box drawing | vtium's own sprites by default, or a font when the look names one for this role, as `glyph-source` chooses today (ADR-1200 D1) | new |
| Symbols and icons | the icons that prompts and file listers draw | the Symbols role, named for both |
| Scripts, each by name | one font for each script the look fills: CJK, Arabic, Hebrew, Indic, Thai and Lao, and others | D6 has one generic Script role |
| Emoji | color emoji | as before |
| Fallback | every character the fonts before it lack | as before |

One font may fill every role, or several fonts may share them. The fonts keep ADR-1204 D6's order: the text font, the scripts, the symbols and icons, the emoji, the fallback, and last the built-in font. Box drawing is not a place in that order. vtium's sprites draw it whatever the order, unless the look gives the role to a font.

A look also takes effects in use: the color maps and shaders that its vtlook names (ADR-1304 D4, D11).

### D7: sizes come from the look

A look offers only the sizes its fonts supply. A bitmap font supplies its strikes, and an outline font supplies its range of sizes. The Looks tab lists those sizes and no others.

A `font-size` written in vtium.conf still works, and vtium draws it as ADR-1200 D17 decides. `vtium check-config` warns when the look's fonts cannot supply that size.

### D8: viewing a font shows what works

The Fonts tab shows what a font covers and the sizes it supplies. Within a look, it shows what the look would still be missing, by role.

Taking a font out of use warns, and the warning lists the looks that use the font. Those looks then draw without it, as for a missing font (ADR-1204 D7).

### D9: what this record changes in other records

Each record below carries a dated note at the passage this record changes. No record is superseded whole.

| Record and decision | What changes |
|---|---|
| ADR-1200 D25 | vtium-config lists fonts in a user's folders but never puts them in use, and a user's folder in `font-paths` yields no font in use. The helper's sandbox keeps reads open for a new reason. |
| ADR-1200 D17 | A look offers only the sizes its fonts supply. A configured `font-size` still works, and the check warns when the fonts cannot supply it. |
| ADR-1201 D3 and its views table | The tabs read Fonts, Looks, Flow, Settings and Help. The Fonts tab returns, its row of `ADR-1201-views.tsv` goes back to planned, and the Looks tab assembles looks by role. |
| ADR-1201 D5 | The install bundle carries the batch file, and the install step for a system font runs through the same privileged step. |
| ADR-1204 D6 | The roles split finer: box drawing joins, Symbols becomes symbols and icons, and each script has a role by name. |
| ADR-1204 D8 | A font reaches the machine from the person's side in the three ways of D5, and draws once it is in use. |
| ADR-1304 D11 | A look's effects come from the color maps and shaders in use. |
| ADR-1403 D7 and D8 | A terminal takes a look from the Looks tab, and the Flow view makes and prunes no look. |
| ADR-1309 D5 | Whether a pointer theme's symbolic links are exempt from D4's rule is a probe. Until it is answered, D5 stands. |
| ADR-1401 D10 | D4's rule sits beside D10, which keeps its scope. |

## Consequences

### Positive

- One screen holds the fonts, one the looks, and one the wiring, in the order the parts depend on each other.
- A person picks fonts from what the system holds instead of typing paths, and sees what a look still lacks before choosing it.
- Every file vtium draws from passes one trust rule. A font that another user, a removable disk or a network share supplies never reaches the console.
- The batch installs with the draft, so a draft carries its fonts in use to another machine as it carries its font sets.

### Negative

- A font in a user's folder, which `font-paths` let vtium draw after login, no longer draws until it is installed as a system font.
- A distribution that installs a font as a symbolic link inside a system folder makes that font unusable until the person copies it.
- A frit that names a font by its path draws the font only while the batch holds it, so installing vtium-fonts no longer makes a catalog set draw by itself (Summary, the fresh-install probe).
- The Flow view loses its quick way to make a look from a font set and a size.

### Neutral

- vtium-config gains the batch file, its folder of draft changes, and the install step for a system font. `vtium check-config` gains the trust rule's reasons and the size warning.
- The views table, the Fonts tab's scenarios, and the Looks tab's form follow the new tab order and roles.

## Alternatives Considered

- **Draw from any font that any searched folder holds,** as `font-paths` does today. Not taken: the maintainer decided that putting a resource in use is an act of trust, and a folder the user or removable media controls cannot carry it.
- **Let a user's own font draw after login,** as ADR-1200 D25 noted. Not taken: the maintainer decided that a font the user installed is listed but not usable, and that the person installs it as a system font to use it.
- **Install a font as a side effect of putting it in use.** Not taken: the maintainer asked for an explicit step that shows its command and is confirmed each time.
- **Keep the Fonts view inside the Looks tab,** as ADR-1201 D3 decided on 2026-11-16. Not taken: the maintainer decided that the tabs follow the composition graph, with the fonts as resources of their own.
- **Call the selection "tags", or picking "tagging".** Not taken: the maintainer dislikes the word, and ADR-1403 retired it.
