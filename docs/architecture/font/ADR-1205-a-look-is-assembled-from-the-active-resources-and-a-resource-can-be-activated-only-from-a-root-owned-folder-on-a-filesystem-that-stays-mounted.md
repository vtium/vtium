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
  - operator: aaronsb
    level: directed
    said: I think filters and color schemes, while different, are different resources
    via: the steering session's brief of 2026-11-19, relaying the maintainer's decisions of that day on color schemes; the quoted words are the maintainer's, as the brief quotes them
  - operator: aaronsb
    level: directed
    said: '[vtium ships common color schemes] like monokai tokyo etc (typical color schemes for konsole or ghostty etc)'
    via: the steering session's brief of 2026-11-19, relaying the maintainer's decisions of that day on color schemes; the quoted words are the maintainer's, as the brief quotes them
  - operator: aaronsb
    level: directed
    said: Anyone can write a color scheme of their own, and a look picks one; the maintainer asked for an amendment to this record that makes a palette a resource file a look can pick.
    via: the steering session's brief of 2026-11-19, relaying the maintainer's decisions of that day on color schemes; the quoted words are the maintainer's, as the brief quotes them, and the comment of 2026-11-19 on the issue for vtium-config's Effects tab, posted from the maintainer's account
    paraphrase: true
  - evidence: 'Ghostty''s documentation, read 2026-11-19 at ghostty.org/docs/features/theme and ghostty.org/docs/config/reference: a theme is a Ghostty configuration file that typically sets only colors, and could set any option, so the page warns about themes from untrusted sources; a theme sets background, foreground, cursor-color, selection-foreground, selection-background and palette, and its example also sets cursor-text; palette takes N=COLOR with N from 0 to 255; a color is #RRGGBB, RRGGBB or a named X11 color; cursor-color, cursor-text and the selection keys also take cell-foreground and cell-background; theme = light:NAME,dark:NAME pairs two themes with the system''s appearance; the built-in themes come from iterm2-color-schemes'
  - evidence: 'theme files read 2026-11-19 through GitHub''s API: catppuccin/ghostty themes/catppuccin-mocha.conf sets palette 0 to 15, background and foreground as hex with no #, cursor-color, cursor-text, the two selection keys and split-divider-color; dracula/ghostty''s dracula sets palette 0 to 15 and the same five color keys with #; folke/tokyonight.nvim extras/ghostty/tokyonight_night sets palette 0 to 15, background, foreground, cursor-color and the two selection keys'
  - evidence: 'license files read 2026-11-19 through GitHub''s API: mbadolato/iTerm2-Color-Schemes LICENSE is MIT, Copyright (c) 2011 to Present Mark Badolato, and says it covers the collection while the copyright and license of each theme belong to that theme''s author; dracula/ghostty MIT, Copyright (c) 2023 Dracula Theme; catppuccin/ghostty MIT, Copyright (c) 2021 Catppuccin; folke/tokyonight.nvim Apache-2.0, with no NOTICE file; tokyo-night/tokyo-night-vscode-theme LICENSE.txt MIT, Copyright (c) 2018-present Enkia; morhetz/gruvbox has no license file and its README states MIT/X11; gruvbox-community/gruvbox LICENSE.md MIT, Copyright (c) 2018 Pavel Pertsev; ku1ik/vim-monokai LICENSE.txt MIT, Copyright (c) 2009-2016 Marcin Kulik; a web search found no license from Wimer Hazenberg for the original Monokai TextMate theme of 2006, and Monokai Pro is a paid product'
  - evidence: 'vtium-term at 665d8669, crates/vtium-term/src/terminal.rs: set_palette replaces the colors the program set with the configured palette, marks every cell changed, and reports a change of mode 2031''s color scheme; crates/vtium-term/src/palette.rs holds the nine named palettes and the colors OSC 12, 13, 14, 17 and 19 name, each unset until set; crates/vtium-term/src/terminal/color.rs reads X11 color names'
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
  - 'ADR-1403#D5: how a key set at two levels combines'
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "On screen and in vtium check-config's messages, a chosen font or effect is active. The actions are Activate and Deactivate, the words of Adobe Fonts, Extensis Suitcase and FontBase. The backend keeps batch for the file that lists what is active, and cullet for a resource."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
    covers: [the words]
  - operator: aaronsb
    said: "When a font is installed twice, once by a system font package and once copied by hand into /usr/local/share/fonts, the package's copy is active, and vtium-config points out the local copy as a duplicate."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
    covers: [a font installed twice]
  - operator: aaronsb
    said: "Pointer themes are exempt from the symbolic link rule when every link stays inside the same root-owned theme folder."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
    covers: [pointer themes]
  - operator: aaronsb
    said: "On a fresh install, the batch starts with the built-in font, Cullet Mono, Terminus, Unifont, and the console font that vconsole.conf's FONT= names, so the default terminus set draws with no setup."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
    covers: [what is active on a fresh install]
  - operator: aaronsb
    said: "Two resource tabs. The Fonts tab manages fonts only: their paths, which are available, previews of what each covers, activation, and the install-as-system-font step. An Effects tab manages effects (color maps and shaders, later filters) the same way: discovery, the trust marks, activation, and a preview of the effect. The two tabs share the controls for activation, trust and user-installed rows that are not usable, because effects and filters would make the Fonts tab too busy and each kind previews differently. The tabs read Fonts, Effects, Looks, Flow, Settings and Help, following the graph from resources to looks to terminals, and vtium-config still opens on Flow. Pointer themes get no tab of their own: Looks lists the installed pointer themes, each look picks one, and pointer themes follow the same trust rule and activation as other resources."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
    covers: [the tab order]
  - operator: aaronsb
    said: "Yes that makes sense a look takes priority over what is defined in settings"
    via: the steering session, 2026-11-18, on how a look's pointer theme sits beside vtium.conf's pointer-theme key
  - operator: aaronsb
    said: "I'm in agreement with that adr"
    via: the steering session, 2026-11-18, after reading the record at 01886728
  - operator: aaronsb
    said: "When /etc/vtium/batch.conf is missing, as on a machine upgraded from an earlier vtium, vtium treats the batch as the fresh-install list: the built-in font, Cullet Mono, Terminus, Unifont, and the console font that vconsole.conf's FONT= names. vtium check-config says that the file is missing and how to create it."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
  - operator: aaronsb
    said: "When vtium-config installs a user's font as a system font and /usr/local/share/fonts already holds a file of that name, vtium-config shows both files, with each one's size, date, and the font's own family name and version, and asks whether to replace the existing file or cancel. When the two files are identical, vtium-config says the font is already installed and does nothing."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
  - operator: aaronsb
    said: "Pointer themes are activated in the Looks tab. The Looks tab lists every installed pointer theme, marks the inactive ones, and offers Activate and Deactivate there, with the same trust marks as the Fonts and Effects tabs."
    via: the maintainer's answers to the steering session's questions, 2026-11-18
    paraphrase: true
  - operator: aaronsb
    said: "these leans are fine [read Ghostty's theme format as it is; the shipped schemes active from the first start; palettes managed in the Looks tab; Monokai left out for now; a palette file may set colors 16 to 255]"
    via: steering session chat, 2026-11-19, after reading the leans in a table
    covers: [the file format, the palette folders, where palettes are managed, Monokai]
  - operator: aaronsb
    said: "I'm not interested in interacting with wimer … let's make an original scheme in the same spirit. 'glassfish' maybe as a name or something like that. maybe not glassfish but...idunno something"
    via: steering session chat, 2026-11-19, after the steering session laid out the copyright and trademark risks of shipping Monokai's colors under another name
    covers: [Monokai]
  - operator: aaronsb
    said: "I like kiln … something like that [as the name of the original scheme in Monokai's spirit]"
    via: steering session chat, 2026-11-19, after the steering session suggested Night Market, Kiln and Lanternfish
  - operator: aaronsb
    said: "1 accept both [ADR-1205 D10 and ADR-1304 D21, as written]"
    via: steering session chat, 2026-11-19, after the steering session quoted both sections in full
  - operator: aaronsb
    said: "2a yes, 2b yes. I think this pattern is kind of similar to trusting paths"
    via: the steering session's chat, 2026-11-19, answering whether a pointer theme is activated by its folder (2a) and whether /usr/share/vtium/pointers.svg joins the fresh-install list (2b)
  - operator: aaronsb
    said: "I think we keep one convention"
    via: the steering session's chat, 2026-11-19, after the maintainer said pointers not following the same trust conventions would be 'a rake in the yard to step on', answering the proposal of one gate for every resource kind
  - operator: aaronsb
    said: "can we add one more profile, a sun workstation (the horrifying white background on dark text) … later it can be composed with an actual sun workstation serifed font for those who wax nostalgic for java and sparc … maybe we call it 'sunny'"
    via: the steering session's chat, 2026-11-19, after the maintainer saw the twelve shipped schemes' pictures
  - operator: aaronsb
    said: "yes a"
    via: "the steering session's chat, 2026-11-19, choosing (a): vtium rereads the batch at each new login, with the font settings, over (b) reading it only when vtium starts"
  - operator: aaronsb
    said: "i agree on 2 and 3 [2: the Effects tab keeps its preview picture in a vtium console that draws its own vtlook, and the line beside it says the console draws the picture through that vtlook as well; 3: the vtium packages build the filters feature from 0.1.0-alpha.10 on]"
    via: steering session, 2026-11-20, on the wave 4 candidate
  - operator: aaronsb
    said: "I'd say we aim for the lowest physical attached monitor, without complicating the code required to determine it. [the DPI vtium-config uses to turn a bitmap font's strikes into a look's point sizes]"
    via: steering session, 2026-11-20, on the Fonts tab review's C1
  - operator: aaronsb
    said: "A [look NAME] section can name its own pointer-theme, so each look picks its own theme, and a look with none draws the top-level one. Built in the next wave."
    via: the maintainer's decision of 2026-11-20, as the steering session's comment on the GitHub issue for a look's pointer theme records it
    paraphrase: true
  - operator: aaronsb
    said: "something our aur package should do is activate the fonts we ship"
    via: the maintainer in conversation with the steering session, 2026-11-27, as the steering session's decisions for that day record it
  - operator: aaronsb
    said: "vtium-fonts installs a list of the fonts it activates, a file the package manager owns, which vtium reads beside /etc/vtium/batch.conf, so an upgrade activates newly shipped fonts and removing the package deactivates them. The list holds the four fonts vtium-fonts carries and every font file its sets name from the Arch packages it depends on, and a listed file that is not installed counts as missing until its package arrives. The administrator can still turn a package's list, or one font in it, off from /etc."
    via: "the maintainer's choices through the question tool, 2026-11-27, as the steering session's decisions for that day record them: a package list file over an install script that edits batch.conf or a longer fresh-install list, and every font the sets name over only the four the package carries"
    paraphrase: true
  - operator: aaronsb
    said: "we need to be way more supportive and indicative of 'activated' fonts, and if any look or config has fonts defined that are not activated. it's frustrating to hit that. we could have an 'activate missing fonts' option in settings or fonts? that would basically take the config, enumerate what's not activated, and activate them through the normal activation path"
    via: the maintainer in conversation with the steering session, 2026-11-27, on the mock-up catalog of vtium-config's screens
observable:
  - "vtium-config's tabs read Fonts, Effects, Looks, Flow, Settings and Help, and vtium-config opens on Flow"
  - "The Effects tab lists the color maps and shaders in the folders that ADR-1304 gives them, shows each one's trust mark, activates or deactivates each with the same controls as the Fonts tab, and previews an effect as a picture of the console"
  - "The Looks tab lists the installed pointer themes, and each look picks one; a pointer theme that is not active cannot be picked"
  - "The Fonts tab lists a font from ~/.local/share/fonts, marked as installed by the user and not usable, previews it in vtium-config's own window, and offers to install it as a system font"
  - "A font file in /usr/share/fonts that is a symbolic link, or that sits on a USB stick or an NFS mount, cannot be activated; vtium draws without it, logs one line, and vtium check-config names the reason"
  - "Deactivating Cullet Mono warns, lists each look that uses it, and those looks then draw as when Cullet Mono is missing; the built-in font cannot be deactivated"
  - "Installing a font as a system font shows the exact command, asks each time, and leaves the file in /usr/local/share/fonts owned by root with mode 0644"
  - "A look whose only font is bitmap Terminus offers the sizes of Terminus's strikes and no other; font-size = 13 in vtium.conf still draws, and vtium check-config warns that the look's fonts do not supply it"
  - "On a fresh install, the Fonts tab shows the built-in font, Cullet Mono, Terminus, Unifont and the console font that vconsole.conf's FONT= names as active, and the default terminus set draws with no setup"
  - "With the same font installed by a package in /usr/share/fonts and by hand in /usr/local/share/fonts, the package's copy is active and the Fonts tab marks the local copy as a duplicate"
  - With /etc/vtium/batch.conf missing, the built-in font, Cullet Mono, Terminus, Unifont and the console font that vconsole.conf's FONT= names are active, and vtium check-config says that the file is missing and how to create it
  - Installing a font as a system font when /usr/local/share/fonts already holds a different file of that name shows both files, with size, date, family name and version, and asks to replace or cancel; with an identical file, vtium-config says the font is already installed and writes nothing
  - The Looks tab lists every installed pointer theme, marks the inactive ones, shows their trust marks, and activates or deactivates a pointer theme with the same controls as the Fonts and Effects tabs
  - With vtium-fonts installed and no batch.conf, the Fonts tab shows Tiny5 Mono as active, its detail line names the vtium-fonts package, and a frit that names Tiny5 Mono draws it at the next login
  - With noto-fonts-cjk not installed, vtium config active shows the Noto Sans CJK files of the vtium-fonts list as not installed; once noto-fonts-cjk is installed, a CJK set draws with Noto Sans CJK from the next login, with no other step
  - An empty /etc/vtium/batch.d/vtium-fonts.conf turns the package's list off at the next login, and vtium config active shows that list as turned off
  - Deactivating a font of the vtium-fonts list in vtium-config and installing the draft writes a line that starts with - into batch.conf, leaves /usr/share/vtium/batch.d/vtium-fonts.conf as pacman installed it, and the font stays inactive after vtium-fonts is upgraded
  - Removing vtium-fonts deactivates its fonts at the next login, and batch.conf names none of their paths
  - A list in /usr/share/vtium/batch.d that another user can write activates nothing, and vtium check-config names the file and the check it failed
  - A symbolic link from /etc/vtium/batch.d/vtium-fonts.conf to /dev/null turns the package's list off at the next login, and vtium config active shows that list as turned off
status: accepted
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
  - ADR-1001
---

# ADR-1205: A look is assembled from the active resources, and a resource can be activated only from a root-owned folder on a filesystem that stays mounted

## Summary

- **Decided:** vtium's configuration is a graph from resources to screens. Font files, color maps, shaders and pointer themes are resources. A look is assembled from the active resources, a terminal in the Flow view takes a look, and the terminal is wired to its displays and inputs. vtium-config's tabs follow the graph: Fonts, Effects, Looks, Flow, then Settings and Help. Fonts and Effects are the two resource tabs, and they share one set of controls for activation, trust and the rows a user installed. The Fonts tab returns as a tab of its own, and finds fonts in well-known folders and sorts them with simple filters, so nobody types a raw path. The Effects tab does the same for color maps and shaders. Pointer themes have no tab of their own: the Looks tab lists them, and each look picks one. A resource can be activated only from a root-owned folder on a filesystem that stays mounted for the system's life. A file that fails that rule counts as a file that does not exist. A user's own fonts are listed and can be previewed in vtium-config's window, but cannot be activated. vtium-config can install such a font as a system font, in an explicit step that the person confirms each time. A look fills the roles of ADR-1204, now split finer: text, box drawing, symbols and icons, each script by name, emoji, and fallback. A look offers only the sizes its fonts supply. In the records, a resource is a cullet, and the batch is the file that lists the active resources. On screen, vtium says font, effect, font set and look, and a font or effect is active or not. A fresh install starts with the built-in font, Cullet Mono, Terminus, Unifont and the system's console font active, so the default font set draws with no setup. A font package can also install a list of the fonts it activates, which vtium reads beside the batch, and root can turn a package's list, or one font in it, off from `/etc` (D4, amended 2026-11-27). vtium-config marks each font a configuration uses as active, not active or missing, and activates in one change the installed and trusted fonts it uses that are not active (D4; ADR-1403 D11).
- **Trades away:** A font that the person put in their home folder no longer draws on the console, even when a folder of `font-paths` names it. A font installed as a symbolic link, or on removable, network or automounted storage, never draws. A frit that names a font by its path draws that font only while the batch holds it, so a font now passes two gates, being installed and being active. The Flow view no longer makes a look from a font set and a size, so a person who wants a new look goes to the Looks tab first. Installing a font package that ships such a list, as vtium-fonts does, activates its fonts without a step from the person, so the person turns off what they do not want rather than turning on what they do (D4, amended 2026-11-27).
- **One-way?** No. The batch is one root-owned file, the trust rule is a check before a file is read, and the tabs are vtium-config's layout. A session of work can change any of them. The words cullet, batch and active are the costly part to change once code and pages use them.
- **Probes:**
  - *Settled:* every probe is settled. The maintainer answered five probes on 2026-11-18. *The tab order:* two resource tabs, Fonts and Effects, and the tabs read Fonts, Effects, Looks, Flow, Settings and Help, with vtium-config still opening on Flow (D1, D3, D8). Pointer themes are picked in the Looks tab (D6). *The words:* a chosen font or effect is active, and the actions are Activate and Deactivate (D2). *A font installed twice:* the package's copy is active, and vtium-config points out the local copy as a duplicate (D5). *Pointer themes:* a pointer theme's symbolic links pass the trust rule while every link stays inside the theme's own root-owned folder (D4). *What is active on a fresh install:* the built-in font, Cullet Mono, Terminus, Unifont, and the console font that vconsole.conf's `FONT=` names (D4).
  - *Confident (the file format):* added 2026-11-19 for D10, which is proposed. A color scheme file is read in the format Ghostty uses for its themes, exactly as people already share them, so a scheme written for Ghostty works by copying the file in. vtium takes only the colors from such a file and ignores the rest. Is that what you want? Lean: yes, because hundreds of schemes already exist in that format and it is plain text. Against: vtium then follows a format another project controls, and its rare keys, such as `cursor-text`, do nothing in vtium (D10).
  - *Not confident (the palette folders):* added 2026-11-19 for D10. The color schemes vtium ships live in `/usr/share/vtium/palettes`, and your own in `/etc/vtium/palettes`, where they win over the shipped ones on the same name. Like fonts, a scheme must be active before a look can use it. Should the shipped schemes be active from the first start, as the default fonts are? Lean: yes, because a scheme file holds only colors, so nobody gains by activating it first, and the Looks tab then offers every shipped scheme at once. Against: one rule for every resource, with no exception for a kind, is easier to explain (D10).
  - *Not confident (where palettes are managed):* added 2026-11-19 for D10. Color schemes are not effects, so they do not join the Effects tab. Should the Looks tab list them, activate them and show a sample of each, as it does for pointer themes, with each look picking one? Lean: yes, because a look is where a scheme is chosen, and the tabs stay at six. Against: a scheme is a resource like a font, and a tab of its own would follow the graph from resources to looks more strictly (D10).
  - *Not confident (Monokai):* added 2026-11-19 for D10. Dracula, Catppuccin, Tokyo Night and Gruvbox each carry a license from their authors that lets vtium ship them. Monokai's author published the original in 2006 with no license that this record's agent could find, and now sells Monokai Pro. Ports of it carry the MIT license, but a port's author can only license what they hold. Should vtium leave Monokai out of the package for now, while anyone can still import it as their own file? Lean: yes, until a license from its author turns up. Against: Monokai is one of the best known schemes, and MIT ports of it are everywhere (D10).
  - *Confident (where package lists live):* added 2026-11-27 for D4's amendment. vtium-fonts puts its list in `/usr/share/vtium/batch.d/vtium-fonts.conf`, which pacman owns, and an administrator turns the whole list off with an empty file of the same name in `/etc/vtium/batch.d/`, the way systemd's folders work. Is that what you want? Lean: yes, because the package never touches `/etc`, and systemd's administrators already know the rule. Against: a second folder under `/etc/vtium` holds part of what is active, beside batch.conf (D4).
  - *Not confident (turning one font off):* added 2026-11-27 for D4's amendment. To turn off one font that a package's list activates, batch.conf gains a line with `-` before the path, and vtium-config's Deactivate writes that line, so the package's later upgrades still add new fonts and leave that one off. Is that what you want? Lean: yes, because batch.conf stays the one file that holds root's choices. Against: batch.conf then holds two kinds of line, and an older vtium reports the `-` line as a fault (D4).
- **Inversion:** The decision sits between two ends. At one end, vtium draws any font that any folder holds, and a person types paths to name them. At the other, vtium draws only what it ships. vtium draws from a curated batch of resources that the system installed and the person activated, finds those resources by itself, and lets the person install a missing one as a system font.

> Note 2026-11-19: D10 joins this record as a proposal. The maintainer decided on 2026-11-19 that a palette, which sets the terminal's colors, is a resource of a different kind from a filter, which changes the finished picture, and that vtium ships common color schemes that anyone can add to (`basis`). D10 proposes the rest: the folders, the file format, which is Ghostty's theme format read as it is, how a look picks a palette, and which schemes vtium ships. Its four probes above wait for the maintainer, and the rest of this Summary describes D1 to D9, which stay accepted.

> Note 2026-11-27: D4 gains an amendment, [a package's list of active resources](#a-packages-list-of-active-resources). The maintainer decided on 2026-11-27 that the vtium-fonts package activates the fonts it ships, through a list file that the package installs (`considered`). The amendment decides where the lists live, how `/etc` turns one off, the trust rule for a list, and how vtium-config shows and deactivates a font that a package activates. Its two probes above wait for the maintainer.

> Edited 2026-11-27: the maintainer asked vtium-config to show and activate the fonts a configuration uses but has not activated (`considered`). D4 gained [fonts a configuration uses but has not activated](#fonts-a-configuration-uses-but-has-not-activated), and the Summary its last sentence.

## Context

vtium-config has grown its font and look tools one view at a time. A look is a `[look NAME]` section of vtium.conf that names a font set, a size and a vtlook (ADR-1403 D1, ADR-1304 D4). The Looks tab of ADR-1201 D3 took in the Fonts view on 2026-11-16. The Flow view of ADR-1403 D7 makes a look on its own whenever a path's font set or size changes. A person names a font by its family in a frit, or by a path, and `font-paths` adds folders to search (ADR-1200 D25). A user's own font folder can be added there, and the font helper's sandbox leaves reads open, because a frit may name a font anywhere.

That leaves three gaps. First, the screens do not follow how the parts depend on each other. A person meets the font sets inside the Looks tab and the looks inside the Flow view, with no single place for the fonts themselves. Second, nothing says which files vtium may trust as resources. vtium.conf and the frits pass an ownership check (ADR-1401 D10), and so does a pointer theme (ADR-1309 D5). A font file named in a frit, a `.cube` table and a shader each pass their own checks or none. Third, the person types font names and paths without seeing what the system holds or what a look still lacks.

The maintainer decided on 2026-11-18 that the configuration is one composition graph, from resources to screens, and that activating a resource is an act of trust. This record states those decisions and amends the records they touch.

## Decision

### D1: the composition graph

vtium's configuration is a graph from resources to screens:

| Stage | What it is | Where vtium-config edits it |
|---|---|---|
| Resources | font files, effects (color maps such as `.cube` tables, and shaders), and pointer themes | the Fonts tab, for fonts; the Effects tab, for effects; the Looks tab, for pointer themes |
| A look | a font set assembled from active fonts, its sizes, the active effects, and an active pointer theme (D6, D7) | the Looks tab |
| A terminal | a head, which takes one look (ADR-1401 D1) | the Flow view |
| Displays and inputs | the devices wired to the terminal by rules (ADR-1403 D7) | the Flow view |

vtium-config's tabs follow the graph from resources to looks to terminals, in this order: Fonts, Effects, Looks, Flow, then Settings and Help. vtium-config still opens on Flow, where most configuration work happens (ADR-1201 D3).

- **Fonts** manages the font resources: it finds them, filters them, shows what each covers, activates or deactivates each, and installs a user's font as a system font (D3, D5, D8). The Fonts tab returns as a tab of its own, and it holds fonts only.
- **Effects** manages the effects, color maps and shaders today and the filters that later records add, in the same way: it finds them, shows each one's trust mark, activates or deactivates each, and previews an effect (D3, D8).
- **Looks** assembles each look by role from the active fonts, offers the sizes those fonts supply, adds the active effects, and picks the look's pointer theme (D6, D7). It keeps its form for a new font set.
- **Flow** wires terminals to displays and inputs. A terminal takes a look from the Looks tab. The Flow view no longer makes a look itself.

**The two resource tabs share their controls.** Fonts and Effects activate and deactivate a row with the same keys, show the same trust marks, and mark a row the user installed as not usable in the same way. They differ in what they find and how they preview it. The maintainer split them on 2026-11-18 because effects and filters would crowd the Fonts tab, and because a font and an effect each need a preview of their own kind.

Pointer themes have no tab of their own. The Looks tab lists the installed pointer themes, and each look picks one (D6).

### D2: the words

The backend and the records keep the glassmaking words of [Why VTium](../../why-vtium.md#why-glass). Pages for users and every string on screen use the everyday words.

| In the records and the code | On screen and in pages for users | What it means |
|---|---|---|
| cullet | font, effect, color map, shader, pointer theme | one resource: a font file, a color map such as a `.cube` table, a shader, or a pointer theme |
| batch | the active fonts and effects | the file that lists the resources vtium may draw from (D4) |
| activate, deactivate | Activate, Deactivate | the actions that add a resource to the batch or remove it |
| frit | font set | a font set file, as ADR-1200 D1 defines it |
| vitrify | (not shown) | generating glyphs from fonts |
| look | look | a `[look NAME]` section of vtium.conf |

A glassmaker's cullet is broken glass that goes back into the furnace with the raw materials ([Wikipedia, Cullet](https://en.wikipedia.org/wiki/Cullet)). A glassmaker's batch is the raw materials picked and weighed for one melt. vtium's batch likewise lists the resources picked for the console. This record defines cullet once. The word already sits in the name of vtium's outline font, Cullet Mono, and of the test set `cullet-alone`. In this record's sense, any resource is a cullet, Cullet Mono being one.

The batch of this record is not the batch of ADR-1200 D18, which is a group of 1,024 code points that the font helper fills at once. A record that means D18's batch says "a batch of code points".

The maintainer chose the words on screen on 2026-11-18. A chosen font or effect is active, on screen and in `vtium check-config`'s messages, and the actions are Activate and Deactivate. Font managers such as Adobe Fonts, Extensis Suitcase and FontBase use the same words for the same toggle. The maintainer dislikes "tag", which ADR-1403 once used for picking items with Space in the Flow view and has retired.

### D3: vtium-config finds the fonts and the effects

vtium-config finds fonts in well-known folders, so nobody types a raw path:

- **The system's folders:** `/usr/share/fonts` and `/usr/local/share/fonts`, which are fontconfig's default folders for the system, `/usr/share/vtium/fonts`, where vtium installs its own font files, and kbd's console-font folders (ADR-1200 D25).
- **The folders of `font-paths`,** where vtium.conf sets it (ADR-1200 D25).
- **The user's folders:** `~/.local/share/fonts` and `~/.fonts`, which fontconfig searches for a user's own fonts.

The Fonts tab sorts what it finds with simple filters: bitmap or outline, monospaced or not, the scripts each font covers, and other well-known groups where a font states one, such as a Nerd Fonts build or a color emoji font. vtium-config reads a font's family, coverage and sizes only through the sandboxed font helper, never in its own process (ADR-1201 D3).

**A user's own fonts** are listed and marked as not usable, because the user installed them and the system did not. The Fonts tab says so, so the person knows to install the font as a system font (D5) before activating it. vtium-config may preview such a font in its own window, because the font helper parses it in the sandbox (ADR-1201 D3, ADR-1301). Trying a font live on the console, with ADR-1201 D4's live trial, needs an active system font.

**The Effects tab** finds color maps and shaders in the folders that ADR-1304 gives them: `/etc/vtium/looks` for `.cube` tables, and `/usr/share/vtium/shaders` and `/etc/vtium/shaders` for shaders. The tab lists each one with its trust mark, and activates or deactivates it with the Fonts tab's controls. A color map or shader that the person keeps in their own folders is listed as not usable, as a user's font is. The person installs it through the draft's Install step, which ADR-1304 D11 already gives shaders and `.cube` tables, and then activates it.

### D4: activating a resource is an act of trust

vtium draws only from the active resources. A font that a frit names, by its family or by its path, draws only while the batch holds it. Otherwise it counts as missing, and its characters draw from the next source, as for any missing font (ADR-1204 D7). A color map or a shader that a vtlook names counts as missing unless the batch holds it.

**The rule.** A resource can be activated only when all of these hold:

- The file sits in a folder that root owns, and root owns the file.
- Neither the group nor other users can write the file or any folder above it, as ADR-1401 D10 asks of vtium.conf.
- The file and the folders on the way to it are not symbolic links.
- The filesystem that holds it is mounted at boot and stays mounted for the system's life. Removable media, a network filesystem and an automounted filesystem fail this rule.

A path that the person writes by hand is always allowed, in the batch or in a frit's `source=` line, and it passes the same checks. A file that fails any check counts as a file that does not exist. vtium draws without it and logs one line for it, once. `vtium check-config` and vtium-config name the file and the check it failed.

**Pointer themes.** A pointer theme (ADR-1309) is a resource too, and a desktop cursor theme's folder holds symbolic links, such as Adwaita's `xterm` to `text`. A pointer theme's links pass the rule while every link stays inside the same root-owned theme folder. A link that leaves the theme's folder fails the rule, and the file it names counts as missing. The theme's folder and its files pass every other check.

> Note 2026-11-19: the maintainer decided two things about pointer themes and the batch (`considered`). A pointer theme is activated by its folder: the batch lists the theme's folder, such as `/usr/share/icons/Adwaita`, and every file inside the folder counts as active while the folder is listed, the same way the trust rule judges a theme as one folder. vtium's own pictures, `/usr/share/vtium/pointers.svg`, join the list of a fresh install, so vtium's own pointer pictures stay active on every machine.

> Note 2026-11-19: the maintainer decided that every kind of resource follows one convention (`considered`). Fonts, color schemes, color maps, shaders and pointer themes pass through the same gate: the trust rule on the file vtium opened, and the batch listing the resource. The batch lists what a person activates, a file, or a pointer theme's folder. The rule that a theme's links stay inside its folder is part of that gate, and it is the only difference between the kinds.

> Note 2026-11-19, later the same day: the maintainer decided when a change to the batch takes effect (`considered`). vtium reads the batch again where it rereads the font settings, at each new login, so activating or deactivating a resource takes effect at the next login, as a change to a font setting does, with no restart of vtium.

> Note 2026-11-20: the maintainer decided what a running session sees when a login changes the batch (`considered`). A console whose session still runs keeps the colors it draws, including the colors its program set with OSC 4. The new color scheme becomes what that console's color resets and its next session return to, and a console with no running session draws it at once. Fonts and the pointer pictures change at once on every display, because a display draws all its consoles through one font store and one font helper, and vtium reads one pointer file for the seat. So a deactivated font stops drawing in sessions that are still running, and its characters fall to the next font that has them.

The program that reads a resource checks the rule on the file it opened, so the file cannot be swapped between the check and the read. When vtium runs unprivileged, as the tests and a run by hand do, it accepts its own effective user as the owner, as ADR-1401 D10 does for vtium.conf. These two points are this record's agent's calls. How vtium tells a filesystem that stays mounted from one that does not is left to the work that builds the rule, which reads the system's mount table.

> Note 2026-11-19: the work that builds the rule decides how vtium tells a filesystem that stays mounted. vtium finds the mount that holds the opened file in the kernel's mount table, `/proc/self/mountinfo`, by the mount ID that `statx` reports for the file's descriptor. The device number alone does not do, because on btrfs it names the subvolume and the mount table names the whole filesystem. A kernel older than 5.8 reports no mount ID, so vtium then matches the device number. Failing that, it takes the longest mount point above the file's path, but only when that mount point's own folder has the file's device number. vtium refuses a network filesystem, such as nfs, cifs, ceph, 9p, afs or virtiofs, because a server elsewhere can change or withdraw its files. It refuses every FUSE filesystem, `fuse.*` and `fuseblk`, because a program serves it. It refuses an automount point, and any filesystem mounted below one, because the system mounts those only on demand. It refuses a filesystem on a removable disk, which vtium finds in the device's folder in sysfs. btrfs gives the mount table an anonymous device number, so for btrfs vtium finds the device by the path in the mount's source. A partition is judged by its disk. A device-mapper or md device, such as an encrypted volume or a RAID array, is judged by every device that its `slaves` folder lists, down to the disks. A loop device is judged by the filesystem that holds its backing file, which the whole rule judges in turn. A disk counts as removable when its `removable` attribute is 1, or when its sysfs path lies below a USB host controller, because many USB disks report themselves as fixed. The maintainer decided on 2026-11-19 that a disk on the USB bus counts as removable, unless it is the disk that holds the root filesystem. When vtium cannot read a disk's folder in sysfs, as inside a sandbox that hides `/sys`, the file fails the rule. The filesystem mounted on `/` stays mounted for the system's life whatever disk holds it, as on a system that boots from a USB stick. So vtium finds the disks beneath `/`, through device-mapper and md devices and from a partition to its disk. A filesystem whose disks all lie beneath `/` is exempt from the removable rules, such as `/home` or `/boot` on the boot stick, while a second USB disk still fails. When vtium cannot find the disks beneath `/`, only the filesystem mounted on `/` itself is exempt. A file whose mount vtium cannot find in the mount table fails the rule. vtium reads the table again once before it decides this, because the mount may be newer than the table.

**What is active from the start.** The built-in font (ADR-1204 D1) is always active, and the person cannot deactivate it. On a fresh install, the batch also lists Cullet Mono, Terminus, Unifont, and the console font that the system's console configuration names in vconsole.conf's `FONT=`. So the default set `terminus` draws with no setup, as it did before this rule. The person can deactivate each of these. Every other font waits until the person activates it, or until a package's list activates it, as [a package's list of active resources](#a-packages-list-of-active-resources) below decides.

> Edited 2026-11-27: the maintainer decided that the vtium-fonts package activates the fonts it ships (`considered`, 2026-11-27). The sentence read: "Every other font, those of vtium-fonts included, waits until the person activates it."

**Where the batch lives.** The batch is a root-owned file under `/etc/vtium`. The name `/etc/vtium/batch.conf`, with one path to a resource on each line, is this record's agent's call. vtium-config writes the batch through its sudo-backed install, as it writes vtium.conf (ADR-1201 D5). A draft keeps its changes to the batch beside the draft, as it keeps its font set changes (ADR-1403 D8), so the changes install with the draft.

> Note 2026-11-18: the maintainer decided what vtium does when the batch file is missing (`considered`, 2026-11-18). A machine upgraded from an earlier vtium has no `/etc/vtium/batch.conf`. vtium then treats the batch as the list of a fresh install: the built-in font, Cullet Mono, Terminus, Unifont, and the console font that vconsole.conf's `FONT=` names. So the default set `terminus` still draws after the upgrade. `vtium check-config` says that the file is missing and how to create it.

> Note 2026-11-19: the work that builds the batch decides where vtium finds the console font of a fresh install. vconsole.conf's `FONT=` names the font by an absolute path or by kbd's name, such as `ter-v16n`. vtium looks for a font named by kbd's name only in kbd's font folders: `/usr/share/kbd/consolefonts`, `/usr/lib/kbd/consolefonts`, `/lib/kbd/consolefonts` and `/usr/share/consolefonts`. The list of a fresh install holds each file there whose name is the font's name with or without one of kbd's suffixes, such as `.psf.gz`. A file of that name in any other folder is not active. The same work decides what vtium does with a batch file it cannot use. When the file is not a regular file, fails the owner and mode check, is larger than 64 KiB, or cannot be read, vtium uses the list of a fresh install, as for a missing file. A batch that someone other than root could have written says nothing about what root chose. `vtium check-config` names the file and says why vtium ignores it.

**This rule sits beside ADR-1401 D10.** D10 keeps its scope: vtium.conf and the frits, which route input and choose login commands. This rule covers the resources a look draws from.

#### A package's list of active resources

> Amended 2026-11-27: the maintainer decided that the vtium-fonts package activates the fonts it ships (`considered`, 2026-11-27). The maintainer chose a list file that the package installs and the package manager owns, over an install script that edits batch.conf and over a longer list of a fresh install. The maintainer also chose that the list holds every font the package's sets name, and not only the four fonts the package carries. The paragraphs below decide the rest, and they are this record's agent's calls, for the maintainer to read before the build. Where a paragraph names the steering session's call, the steering session settled a point that a fresh review of the amendment raised, on 2026-11-27, in the maintainer's absence.

**Where a package's list lives.** A package can install a list of the resources it activates, in the batch file's format: one absolute path on each line, with `#` comments and blank lines. The list lives in `/usr/share/vtium/batch.d/` and is named for the package, as in `/usr/share/vtium/batch.d/vtium-fonts.conf`. The package manager owns the file, so an upgrade that ships a new font also activates it, and removing the package removes its list. `/usr/share` is where a package puts its read-only data, and `/etc` stays the administrator's, so pacman never finds a changed configuration file to keep. The folder is set at build time, as the package's font folder is with `VTIUM_PACKAGE_FONTS`, so a console installed under `/usr/local` reads `/usr/local/share/vtium/batch.d/`.

**How vtium reads the lists.** vtium reads the lists wherever it reads the batch, at each new login, so a package's change takes effect at the next login, as a change to batch.conf does. The active resources are then the built-in font, what batch.conf lists, and what each package's list names, less what batch.conf deactivates (below). Where vtium reads no batch.conf, the list of a fresh install takes its place, as before. A package's list only adds to the batch: it never replaces batch.conf or the list of a fresh install. In each folder, vtium takes the entries whose names end in `.conf`, and passes over every other name. So pacman's `.pacnew` and `.pacsave` copies activate nothing. It reads the lists of both folders in the byte order of their names, a name in `/etc/vtium/batch.d/` taking the place of the same name under `/usr/share` (below).

**The limits.** vtium takes at most 64 entries from each folder, a named constant beside the batch file's size limit. It counts every entry whose name ends in `.conf`, whatever its type, in the byte order of the names, before any of them masks a package's list. `vtium check-config` names each entry past that number, which activates nothing. The merged batch also has a bound of its own: the paths it holds, batch.conf's lines and every list's together, come to at most 1 MiB, counted as the font helper counts its request of active fonts. The bound is a named constant beside the batch file's limit, and its comment says what it protects: the size of the request that carries the active fonts to the font helper, and the memory that the console and the font helper spend on the merged batch. vtium adds the lists in the order above, and a list that would take the merged batch past the bound is left out, with every list after it. `vtium check-config` names each list left out. How the entries are counted, the merged bound and its size are the steering session's calls.

**The trust rule for a list.** vtium reads each list file exactly as it reads batch.conf, through vtium-settings's `read_limited`, with batch.conf's owner and mode rule (ADR-1401 D10). It follows a symbolic link on the way and checks each folder it crosses, through every link: root owns the file and each of those folders, and neither the group nor other users can write them. The file must be a regular file once the links are followed, and it holds at most 64 KiB. A list has no rule about links beyond batch.conf's, which is the steering session's call. A list that fails the check activates nothing, and `vtium check-config` names the file and the check it failed. A refused list falls back to nothing, because a list only adds. Each path a list names then passes this record's rule when vtium opens the file, as a path in batch.conf does. So a list can activate only a file that root installed in a root-owned folder on a filesystem that stays mounted, and never a user's own font, a file on a USB stick or a font installed as a symbolic link.

**A listed file that is not installed.** A list may name a file whose package is optional, such as a Noto Sans CJK file from `noto-fonts-cjk`. While no file is at that path, the font counts as missing, as any missing font does, and its characters draw from the next source that has them. Once the package installs the file, the font is active from the next login, with no further step. `vtium check-config` does not report a listed file that is missing, because a package's list names optional fonts on purpose. `vtium config active` shows it as not installed.

**Turning a package's list off from /etc.** The administrator's folder is `batch.d` beside the batch file, so it is `/etc/vtium/batch.d/` when the batch file is `/etc/vtium/batch.conf`. vtium takes its folder from the folder of the batch file that `VTIUM_BATCH` sets at build time, which is the steering session's call. A file of the same name in `/etc/vtium/batch.d/` replaces the package's list. An empty file, or one that holds only comments, turns the package's list off. A file there with lines of its own replaces the package's list with the administrator's. A file in `/etc/vtium/batch.d/` whose name no package's list uses is a list of the administrator's own, read in the same way. These are the rules systemd follows for its own configuration folders, where a file in `/etc` overrides the file of the same name under `/usr/lib` ([systemd.unit](https://www.freedesktop.org/software/systemd/man/latest/systemd.unit.html), [tmpfiles.d](https://www.freedesktop.org/software/systemd/man/latest/tmpfiles.d.html)), so an administrator of a systemd distribution already knows them.

Masking goes by name. Every entry in `/etc/vtium/batch.d/` whose name ends in `.conf` masks the package's list of the same name, whatever the entry's type, because masking can only shrink what is active. Reading takes regular files only: vtium reads a list from an entry in either folder only when the entry is a regular file once links are followed, and the entry passes the trust rule above. So a symbolic link to `/dev/null`, which systemd's administrators use to mask a unit, turns a package's list off and activates nothing, and `vtium config active` shows the list as turned off. A regular file in `/etc/vtium/batch.d/` that fails the trust rule masks the package's list too, and `vtium check-config` names the file and the check it failed. This split between masking and reading is the steering session's call.

**Turning one font off.** batch.conf gains a line that deactivates a path: `-` followed by the absolute path, as in `-/usr/share/fonts/noto/NotoSansMono-Bold.ttf`. A deactivated path is not active, whichever list names it, batch.conf's own lines included. Only batch.conf deactivates. A package's list and a list in `/etc/vtium/batch.d/` cannot, so no package can turn off a resource that root activated. A `-` line in any list of either folder is a fault: `vtium check-config` reports the list and the line, and vtium ignores the line and reads the rest of the list, which is the steering session's call. batch.conf stays the one file that holds root's own choices, and the one file vtium-config writes. An older vtium reports a `-` line as a line that does not hold an absolute path, and ignores it.

**How vtium-config shows a package's font.** The Fonts tab shows a font that a package's list activates as active, as any other, and its detail line names the list, as in "Active: the vtium-fonts package activates it." The Effects and Looks tabs do the same for any other resource a list names. Deactivate works on such a font as on any active font: it warns and names the looks that use it (D8), and the draft records `deactivate PATH` beside it, as before. When the draft is installed, vtium-config writes a `-PATH` line into batch.conf for each deactivated path that a list in either folder names, and takes out each line of batch.conf that lists the path, as before. An administrator's own list in `/etc/vtium/batch.d/` counts as a list here, so deactivating one of its paths writes a `-PATH` line too, which is the steering session's call. The package's list stays as the package manager installed it, so the font stays off after the package's upgrades. Activate on a path that batch.conf deactivates takes the `-` line out, so the package's list activates the path again. vtium-config adds a line of its own only when no list would activate the path then. vtium-config never copies a package's list into batch.conf, also when it first writes batch.conf from the list of a fresh install, so removing the package still deactivates its fonts. vtium-config never writes in `/usr/share/vtium/batch.d/` or `/etc/vtium/batch.d/`, so turning a whole list off stays root's edit by hand.

**What `vtium config active` prints.** `vtium config active` prints one line for each resource that is active, that the draft changes, or that batch.conf deactivates. Each line holds four columns, separated by tabs: the kind, the path, the state, and where the state comes from. The state is one of today's words, or `not installed` for a path that a list activates and where no file exists. Where the state comes from is `built in`, `batch.conf`, `fresh install`, the name of a list, such as `vtium-fonts`, or `the draft`. After the resources, one line for each list in the two folders gives the kind `list`, its path, its state and the file that sets it. The state of a list is `read`, `turned off`, `replaced` or `refused`. For example:

```
font	built-in	always active	built in
font	/usr/share/vtium/fonts/ter-u16n.bdf.gz	active	fresh install
font	/usr/share/fonts/vtium/Tiny5Mono.bdf	active	vtium-fonts
font	/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc	not installed	vtium-fonts
font	/usr/share/fonts/noto/NotoSansMono-Bold.ttf	not active	batch.conf
font	/usr/local/share/fonts/Hack-Regular.ttf	activates on install	the draft
list	/usr/share/vtium/batch.d/vtium-fonts.conf	read	/usr/share/vtium/batch.d/vtium-fonts.conf
list	/usr/share/vtium/batch.d/other-fonts.conf	turned off	/etc/vtium/batch.d/other-fonts.conf
```

**What vtium-fonts's list holds.** The list of the vtium-fonts package holds the four fonts that vtium-fonts carries in `/usr/share/fonts/vtium/`, which are Px437 IBM VGA 9×16, Glass TTY VT220, Tiny5 Mono and Unscii-8. It also holds every font file that the catalog's sets and layers name, the sets that the console packages ship included, such as `jetbrains-mono`, which is the steering session's call. Those files come from Arch's packages, such as `ttf-jetbrains-mono`, `ttf-fira-code`, `noto-fonts`, `noto-fonts-cjk`, `terminus-font` and the Nerd Fonts packages. For a font that a set names by its family, the list holds each face file of that family that the package installs, because the font helper picks a family's regular, bold and italic faces and each face it may pick must be active. For a console font that a set names by kbd's name, such as `sun12x22`, it holds the files of that name in kbd's folders. The recipe depends on, or optionally depends on, each package whose files the list names, as it does today, so Noto Sans CJK and the Nerd Fonts builds stay optional dependencies. The list names exact paths and no wildcard, so it says exactly what it activates, and a file that another package later installs under a matching name stays inactive.

**How the list is made.** `make install-fonts` writes the list into the staged tree at `$(DESTDIR)$(PREFIX)/share/vtium/batch.d/vtium-fonts.conf`. It writes the list from a table in vtium's tree that names, for each family and console font that the package's sets name, the Arch package and the files it installs. A test checks that every family and console font the catalog's sets and layers name has a row, so a new set cannot ship a font that the list leaves out. A check that runs on an Arch machine compares the rows with what the packages install. The four fonts that vtium-fonts carries come from the AUR, whose packages `pacman -Fl` cannot list, so the check compares their rows with the paths that the vtium-fonts recipe's own `package()` function installs. It compares every other row with `pacman -Fl`, which lists each repository package's files from the package database without installing the package. So a package that renames a file fails that check before a release. How the four rows are checked is the steering session's call.

**The recipe.** The vtium-fonts recipe lives in vtium-arch, and its change waits for the maintainer's ask, as every change to vtium-arch does. The recipe stages the list with `make install-fonts PREFIX=/usr`. The Makefile's `PREFIX` defaults to `/usr/local`, so without it the list would land in `/usr/local/share/vtium/batch.d/`, where a vtium built for Arch never looks. Requiring `PREFIX=/usr` is the steering session's call.

**What changes in the code.** vtium-settings's batch reader reads the lists of both folders beside batch.conf with `read_limited`, as it reads batch.conf, with a finding for each list it refuses, each entry past the count, each list past the merged bound, and each `-` line in a list. It masks by name over every `.conf` entry in `/etc/vtium/batch.d/`, reads batch.conf's `-` lines, and tells for each active path where its activation comes from. Its `Places` gains the two folders. The merged bound is a new constant beside `BATCH_MAX`, with a comment on what it protects. The font helper's request of active fonts carries the merged batch, as one list of paths, and its bound, `ACTIVE_BYTES_MAX` in vtium-config's `serve/wire.rs`, rises from `BATCH_MAX` to the merged bound, so the build changes both. vtium-config's batch module reads the lists the same way, writes `-` lines into batch.conf on install, never copies a package's list, and prints the fourth column and the list lines in `active`. The text of `allow-font-ops` in vtium-settings's key table says that vtium takes a font file only when `batch.conf` lists it, so it gains the lists, and `config.md` is generated from it again. The use-case tests' machine gains the two folders, so a case can install a package's list. The Makefile's `install-fonts` writes the list.

#### Fonts a configuration uses but has not activated

Added 2026-11-27, after the amendment above. A font set that a look uses can name a font that is installed and trusted but not active, so the look draws without it. The maintainer: "we need to be way more supportive and indicative of 'activated' fonts, and if any look or config has fonts defined that are not activated. it's frustrating to hit that." So one function in vtium-config-core lists them for the open document. It walks every font set the configuration uses: the top-level `font-name`, each look's font set, and their layers. For each font file that a font set would draw, it reports one of three states. Here the batch is what vtium reads with the document's changes: batch.conf and the lists of both `batch.d` folders, less what batch.conf deactivates.

- **active:** the batch holds the file, and the file passes the rule above;
- **not active:** the file is installed and passes the rule, but the batch does not hold it, whether nothing lists it, its list is turned off or batch.conf deactivates it;
- **missing:** no file of the font passes the rule, because none is installed or because each one fails a check, and the state names the check, such as "on removable storage". A listed file that is not at its path is missing with the reason "not installed", the word that `vtium config active` prints for it.

The function reads the font sets, batch.conf and the lists with the code that vtium and the font helper read them with. So its list never disagrees with what vtium draws. vtium-config's screens show these three states in these words everywhere a font appears (ADR-1403 D11). Activate missing puts every font in the not active state into the document's batch, as one change, through the same path as activating one font with Space. That path takes out batch.conf's `-` line for a deactivated path, and adds a line only where no list would activate the path, as "How vtium-config shows a package's font" above says. The change waits in the draft and installs with Save, as "Where the batch lives" says. Activate missing also activates a font that batch.conf deactivates, because the configuration uses the font and Undo takes the change back before Save. That is the steering session's call. A missing font is never activated by it: a file that fails the rule stays a file that does not exist, and vtium-config shows it with the check it failed.

### D5: how a font reaches the machine

A font reaches the machine as a system font in one of three ways:

- **A system font package,** which installs into a root-owned folder such as `/usr/share/fonts`.
- **The person's own copy, made as root,** ideally in `/usr/local/share/fonts`.
- **vtium-config's "install as a system font" step,** for a font the Fonts tab lists from a user's folder.

The install step copies one font file into `/usr/local/share/fonts`, and never into `/usr/share/fonts`, which the packages own. It runs through vtium-config's existing sudo-backed install (ADR-1201 D5). The copy is owned by root with mode 0644, and vtium-config checks it afterwards against D4's rule. vtium-config shows the exact command before it runs it, and asks the person to confirm each time. Activating a font never installs it as a side effect. A user's font is first installed as a system font, then activated.

> Note 2026-11-18: the maintainer decided what the install step does when `/usr/local/share/fonts` already holds a file of the same name (`considered`, 2026-11-18). vtium-config shows both files, each with its size, its date, and the font's own family name and version. vtium-config then asks the person whether to replace the existing file or to cancel. When the two files are identical, vtium-config says that the font is already installed and does nothing.

**A font installed twice.** A package may later install the same font in `/usr/share/fonts` that the person installed by hand in `/usr/local/share/fonts`. The package's copy is then the active one, and the Fonts tab points out the local copy as a duplicate that the person can remove.

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

A look also takes active effects: the color maps and shaders that its vtlook names (ADR-1304 D4, D11).

**A look picks its pointer theme.** The Looks tab lists the installed pointer themes: vtium's own `/usr/share/vtium/pointers.svg`, and each desktop cursor theme in `/usr/share/icons` or `/usr/local/share/icons` whose folder holds a `cursors` folder (ADR-1309 D2). Those two folders are this record's agent's call. Each look picks one of them, or none for vtium's own pictures. A pointer theme follows D4's rule and is activated like any other resource, so the Looks tab offers only active themes and marks the others as not active. A look's choice takes priority over the `pointer-theme` key in vtium.conf (ADR-1309 D1). That key stays the default for a look that picks no theme, so an existing configuration keeps its pointers.

> Note 2026-11-20: the maintainer decided on 2026-11-20 that a `[look NAME]` section can name its own `pointer-theme`, with the same values as the top-level key: an absolute path to an SVG file or to a cursor theme's folder. A look with none uses the top-level `pointer-theme`, and with neither set the look draws vtium's own pictures. ADR-1403 D5 adds `pointer-theme` to the keys a look takes. The paragraph above said that a look's choice wins over the top-level key, and named no key in the look's section.

> Note 2026-11-18: the maintainer decided where a pointer theme is activated (`considered`, 2026-11-18). The Looks tab lists every installed pointer theme and marks the ones that are not active. The person activates or deactivates a pointer theme there, with Activate and Deactivate. The Looks tab shows the same trust marks as the Fonts and Effects tabs, and a look still picks only an active theme.

### D7: sizes come from the look

A look offers only the sizes its fonts supply. A bitmap font supplies its strikes, and an outline font supplies its range of sizes. The Looks tab lists those sizes and no others.

A `font-size` written in vtium.conf still works, and vtium draws it as ADR-1200 D17 decides. `vtium check-config` warns when the look's fonts cannot supply that size.

> Note 2026-11-20: the maintainer decided which display's density turns a bitmap font's sizes into a look's point sizes (`considered`, 2026-11-20). vtium-config uses the lowest density among the monitors physically attached, as `display-dpi` or the displays' EDIDs give it, and does not work out which displays each look's terminals draw on. When the displays' densities differ, the Size line says the sizes are for the lowest-density display.

### D8: viewing a resource shows what works

The Fonts tab shows what a font covers and the sizes it supplies. Within a look, it shows what the look would still be missing, by role.

The Effects tab previews an effect as a picture of the console with the effect applied, as ADR-1201 D4 previews other changes. The preview leaves the live console alone.

Deactivating a font or an effect warns, and the warning lists the looks that use it. Those looks then draw without it: a font as a missing font (ADR-1204 D7), and an effect as a file that does not exist (D4).

> Note 2026-11-20: the maintainer decided what the Effects tab's preview does when vtium-config runs in a vtium console that draws a vtlook of its own (`considered`, 2026-11-20). The console draws its whole frame through its own vtlook, the preview picture included, so the person sees the highlighted effect and the console's own effect together. The tab keeps the picture, and the line beside it says the console draws the picture through its own vtlook as well, naming that vtlook. With No filter highlighted, the picture then shows exactly what the console's own vtlook does.

### D9: what this record changes in other records

Each record below carries a dated note at the passage this record changes. No record is superseded whole.

| Record and decision | What changes |
|---|---|
| ADR-1200 D25 | vtium-config lists fonts in a user's folders but never activates them, and a user's folder in `font-paths` yields no active font. The helper's sandbox keeps reads open for a new reason. |
| ADR-1200 D17 | A look offers only the sizes its fonts supply. A configured `font-size` still works, and the check warns when the fonts cannot supply it. |
| ADR-1201 D3 and its views table | The tabs read Fonts, Effects, Looks, Flow, Settings and Help. The Fonts tab returns, its row of `ADR-1201-views.tsv` goes back to planned, and a planned Effects row joins it. The Looks tab assembles looks by role and picks each look's pointer theme. |
| ADR-1201 D5 | The install bundle carries the batch file, and the install step for a system font runs through the same privileged step. |
| ADR-1204 D6 | The roles split finer: box drawing joins, Symbols becomes symbols and icons, and each script has a role by name. |
| ADR-1204 D8 | A font reaches the machine from the person's side in the three ways of D5, and draws once it is active. A fresh install starts with the default set's fonts active. |
| ADR-1304 D11 | The Effects tab finds, activates and previews the color maps and shaders, and a look's effects come from the active ones. |
| ADR-1403 D7 and D8 | A terminal takes a look from the Looks tab, and the Flow view makes and prunes no look. |
| ADR-1309 D5 | A pointer theme passes D4's rule. Its walk follows a symbolic link only while the link stays inside the same root-owned theme folder. The Looks tab lists the installed pointer themes, and each look picks an active one. |
| ADR-1401 D10 | D4's rule sits beside D10, which keeps its scope. |
| ADR-1403 D5 | A look takes `pointer-theme`, and its value wins over the top level's (D6's note of 2026-11-20). |

> Note 2026-11-19: D10, proposed on this date, would change two more records if the maintainer accepts it. ADR-1403 D5 gives a look the key `palette`, and ADR-1201 D3's Looks tab lists the palettes. Each carries a dated note that says the change waits on D10.

### D10: palettes are a resource kind of their own

> Accepted 2026-11-19: the maintainer read this section and accepted it on this date, with every probe's lean and Kiln in place of Monokai (`considered`). The notes of this date elsewhere in this record and in ADR-1201, ADR-1304 and ADR-1403 that call D10 proposed tell its history; D10 now decides what it says.

**The maintainer's decisions (2026-11-19):** a palette and a filter are different kinds of resource. In the maintainer's words, "I think filters and color schemes, while different, are different resources". Anyone can write a palette of their own, and vtium ships common color schemes, "like monokai tokyo etc (typical color schemes for konsole or ghostty etc)" (`basis`).

**What a palette is.** A palette sets what the terminal's colors are: the 16 ANSI colors, the default foreground and background, and, where the file gives them, the cursor's color and the selection's colors. Programs name those colors by number, and vtium resolves each cell through the palette when it draws (ADR-1001). A filter does something else: it changes the finished picture after the terminal has drawn it (ADR-1304). So a palette changes which colors programs get, and a filter changes how the whole picture looks. A look takes one of each, and the palette comes first.

```mermaid
flowchart LR
    F["a palette file<br>/etc/vtium/palettes/NAME.palette"]:::cfg
    B["a built-in palette<br>legacy, nord, vga and the rest"]:::cfg
    K["the look's palette key,<br>else vtium.conf's"]:::proc
    P["the configured palette"]:::data
    O["the colors programs see:<br>OSC 4 and 10 to 19 change them,<br>OSC 104 to 119 reset them"]:::data
    D["the frame: each cell<br>resolved through those colors"]:::proc
    L["the look's filters<br>change the finished picture"]:::path
    S["the display"]:::fall
    F --> K
    B --> K
    K --> P --> O --> D --> L --> S
    classDef cfg fill:#fbbf24,color:#1a1a1a,stroke:#4a5568
    classDef proc fill:#2d7d9a,color:#ffffff,stroke:#4a5568
    classDef data fill:#2d8e5e,color:#ffffff,stroke:#4a5568
    classDef path fill:#7c3aed,color:#ffffff,stroke:#4a5568
    classDef fall fill:#475569,color:#ffffff,stroke:#94a3b8
```

The figure follows one head's colors from left to right. A look names a palette file or a built-in palette, and that palette becomes the head's configured palette. Programs change their copy of it and reset the copy back to it. vtium draws the frame through that copy, and only then do the look's filters change the finished picture.

**The words.** In D2's words a palette is a cullet. On screen, vtium-config calls it a color scheme, the word people who have themed Ghostty or Konsole already know, and vtium.conf keeps the key `palette`. This choice is this record's agent's.

**Where palette files live.** vtium reads palettes from two folders, as it reads shaders (ADR-1304 D15). The package installs its color schemes in `/usr/share/vtium/palettes`, and an administrator's palettes go in `/etc/vtium/palettes`, which wins over the package's on the same name. Each file is `NAME.palette`. A name follows the rule for a vtlook's name, 1 to 64 characters of `A-Z a-z 0-9 . _ + -` (ADR-1304 D4), so `Catppuccin Mocha` ships as `catppuccin-mocha`. A file may not take the name of a built-in palette or `custom`: `vtium check-config` reports such a file, and vtium treats it as missing. A file holds at most 16 KiB, a row of vtium-settings's bounds table. A file that sets all 256 colors and every other key it may set is under 7 KiB.

**Trust and activation.** A palette file is a resource under D4's rule. It draws only while the batch holds it, it must sit in a root-owned folder on a filesystem that stays mounted, and a symbolic link fails the rule. The probe *the palette folders* asks whether the palettes the package ships are active on a fresh install, as the default fonts are. This record leans yes: a palette file holds colors in a fixed grammar and can change nothing else.

**The file format.** vtium reads Ghostty's theme file format as it is, so a scheme written for Ghostty imports unchanged. Ghostty's documentation describes a theme as a Ghostty configuration file that usually sets only colors. Each setting is a `key = value` line, a line that starts with `#` is a comment, and blank lines are allowed. vtium reads these keys:

| Key | Value | What vtium does with it |
|---|---|---|
| `palette` | `N=COLOR`, N from 0 to 255 | Sets color N. A color the file leaves unset keeps the value it has in `legacy`, so colors 16 to 255 stay xterm's cube and gray ramp unless the file sets them. |
| `foreground`, `background` | `COLOR` | Set the default foreground and background, the colors OSC 10 and 11 report and change. Unset, each keeps `legacy`'s. |
| `cursor-color` | `COLOR` | Sets the cursor's color, the color OSC 12 reports and changes. |
| `selection-background`, `selection-foreground` | `COLOR` | Set the selection's colors, the colors OSC 17 and OSC 19 report and change. |

`COLOR` is `#rrggbb`, `rrggbb`, or an X11 color name, which vtium already reads in OSC 4. In the last three keys, the values `cell-foreground` and `cell-background` leave the color unset, so vtium draws the cursor or the selection by inverting the cell, as it does with no color set.

vtium ignores every other key, because a theme from Ghostty's collection may carry keys that belong to Ghostty alone, such as `cursor-text`, `split-divider-color`, `palette-generate`, `minimum-contrast`, a font or a cursor style. `vtium check-config` lists each ignored key once, as a note and not as an error. vtium never follows a `config-file` or `theme` line. Ghostty warns that a theme can set any of its options. A palette file in vtium sets colors and nothing else.

When a key appears twice, the later line wins. A line that is not `key = value`, a color vtium cannot read, an index past 255, and a file over the size limit are errors. Any error makes the whole file count as missing. The look then draws with the palette vtium.conf names, vtium logs one line for it, and `vtium check-config` names the file, the line and the reason.

vtium's console process reads a palette file itself, as it reads a vtlook. The file is data in a fixed grammar, and vtium-settings reads it with a small, strict reader of its own. Palettes work in every build, the minspec build included, because a palette needs no `filters` feature.

Ghostty can pair a light and a dark theme and switch between them with the system's appearance. That pairing belongs to Ghostty's configuration, not to a theme file, and a console has no system appearance to follow. A look names one palette.

**How a look picks a palette.** The key `palette` joins the keys a look section may set (ADR-1403 D5). Under D5's rule, the most specific level wins: a look's `palette` wins over vtium.conf's top-level `palette`, as a look's pointer theme wins over `pointer-theme` (D6). A look that names no palette takes the top level's, and with neither set, a head draws `legacy`. The value names a built-in palette, `custom`, or an active palette file. `custom` takes its colors from the top level's `palette-*` keys, as it does today. A name that matches no built-in palette and no active file is a warning in `vtium check-config`, and the look draws with the palette vtium.conf names. The key table's `palette` key then takes a name, where it now takes one of a fixed list of names.

```ini
# /etc/vtium/vtium.conf
palette = solarized

[look evening]
font-size = 14
palette = catppuccin-mocha
vtlook = amber
```

Here a head that takes the look `evening` draws Catppuccin Mocha's colors, and the vtlook `amber` then tints the finished picture. Every other head draws `solarized`.

**Where vtium-config manages palettes.** The probe *where palettes are managed* asks this. This record leans to the Looks tab, as D6 does for pointer themes. The Looks tab lists the built-in palettes and the palette files, shows each file's trust mark, activates or deactivates each with the controls of the Fonts and Effects tabs, and previews a palette as its 16 colors beside text in its foreground on its background. Each look picks one active palette.

**What stays built in.** The nine palettes that vtium-term holds stay as palettes that need no file: `legacy`, `nord`, `solarized`, `solarized-black`, `solarized-white`, `soft-black`, `base16-dark`, `base16-light` and `vga`. Like the built-in font, they are always active, and nobody can deactivate them. `custom` keeps working with the `palette-*` keys.

**Programs that change the colors.** The palette a look picks is the head's configured palette. Programs change their copy of it with OSC 4 and OSC 10 to 19, and OSC 104 to 119 return a color to the configured palette's, now the file's color. RIS and DECSTR's color reset return to the file's colors too, and XTPUSHCOLORS saves and restores the copy as before. When the look's palette changes, as when vtium rereads its configuration or a head takes another look, vtium does what it does today when the `palette` key changes. The new palette replaces the colors programs set, every cell is drawn again, and a program that asked for color scheme reports with mode 2031 hears whether the scheme turned dark or light.

**The palette and the filters.** The palette sets the colors, vtium draws the frame through them, and then the look's filters change the finished picture (ADR-1304). Under the amber tint, a Dracula palette's colors become shades of amber, each by its own brightness. A filter never changes the colors that programs see or that OSC 4 reports.

**The color schemes vtium ships.** Each scheme comes from its author's own repository. Each scheme's license is permissive: MIT, Apache-2.0, BSD, ISC, zlib, the Unlicense or CC0. The package installs each scheme's license text with its other license notices, and each file's opening comments name the scheme's source, its author and its license. Ghostty's bundled themes come from the iTerm2-Color-Schemes collection. That collection's license covers the collection and leaves each theme's copyright with its author, so the collection alone gives vtium no license for a scheme (`basis`).

| Scheme | Files | Source | License, read 2026-11-19 |
|---|---|---|---|
| Dracula | `dracula` | the `dracula` file of github.com/dracula/ghostty | MIT, "Copyright (c) 2023 Dracula Theme" |
| Catppuccin | `catppuccin-latte`, `catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha` | `themes/*.conf` of github.com/catppuccin/ghostty | MIT, "Copyright (c) 2021 Catppuccin" |
| Tokyo Night | `tokyo-night`, `tokyo-night-storm`, `tokyo-night-moon`, `tokyo-night-day` | `extras/ghostty/` of github.com/folke/tokyonight.nvim | Apache-2.0, with no NOTICE file. The scheme began as Enkia's Visual Studio Code theme, which is MIT. |
| Gruvbox | `gruvbox-dark`, `gruvbox-light` | the palette in github.com/gruvbox-community/gruvbox, written out as Ghostty's format | MIT, "Copyright (c) 2018 Pavel Pertsev", the author of the original morhetz/gruvbox, whose README states MIT/X11 |
| Kiln, an original scheme in Monokai's spirit | `kiln`, the maintainer's working name of 2026-11-19, settled when the maintainer judges its colors | vtium's own, its colors chosen independently: a warm near-black background, saturated accents and a light warm foreground | vtium's license. The file claims no lineage from Monokai and names no Monokai. |

Nord and Solarized are already built-in palettes. vtium does not ship Monokai. Wimer Hazenberg's TextMate theme of 2006 carries no license from its author, who now sells Monokai Pro, and a port's license, such as the MIT of github.com/ku1ik/vim-monokai, cannot grant more than the port's author holds. The maintainer decided on 2026-11-19 that vtium ships an original scheme in the same spirit instead (`considered`). The slice that builds palettes chooses its colors without sampling Monokai's, and the maintainer judges them by eye on the picture page, as with the green and white presets. The maintainer named it Kiln, a warm dark chamber with glowing glazes, as a working name: "I like kiln … something like that" (`considered`). The name does not pun on Monokai, since a name that calls the mark to mind is what trademark law guards against, and a search on 2026-11-19 found no color scheme named Kiln. A person can still import Monokai as a file of their own.

> Note 2026-11-19: the maintainer asked for one more original scheme, Sunny (`considered`): dark text on a white background, in the spirit of the white consoles of 1990s Unix workstations. vtium ships it as `sunny`, its colors chosen by vtium for legibility on white and judged by the maintainer on the picture page, as with Kiln. A later look can pair Sunny with a serifed workstation console font, once a font with a license vtium can ship is found.

**What D10 changes in other records.**

| Record and decision | What changes |
|---|---|
| ADR-1403 D5 | A look section may set `palette`, which wins over the top level's. |
| ADR-1201 D3 | The Looks tab lists, activates and previews palettes, as the probe *where palettes are managed* leans. |
| ADR-1304 D2 | The palette sets the colors before the frame is drawn, and the color layer maps the finished picture's colors after. ADR-1304 carries a dated note at D2. |

ADR-1000's row `color.palette.named` names ADR-1403 as its source and gains this decision once it is accepted.

## Consequences

### Positive

- The tabs follow the order in which the parts depend on each other: two tabs hold the resources, one the looks, and one the wiring.
- Fonts and effects share one set of controls, so a person who has activated a font knows how to activate an effect. Each tab keeps a preview that suits its kind.
- A person picks fonts from what the system holds instead of typing paths, and sees what a look still lacks before choosing it.
- Every file vtium draws from passes one trust rule. A font that another user, a removable disk or a network share supplies never reaches the console.
- The batch installs with the draft, so a draft carries its active fonts to another machine as it carries its font sets.

### Negative

- A font in a user's folder, which `font-paths` let vtium draw after login, no longer draws until it is installed as a system font.
- A distribution that installs a font as a symbolic link inside a system folder makes that font unusable until the person copies it.
- A frit that names a font by its path draws the font only while the batch holds it, so installing vtium-fonts no longer makes a catalog set draw by itself (D4).

> Note 2026-11-27: D4's amendment changes the consequence above. The vtium-fonts package now installs a list of the fonts its sets name, so installing it makes a catalog set draw again once the set's fonts are installed. A font that a person installs by hand still draws only once it is active.
- The Flow view loses its quick way to make a look from a font set and a size.

### Neutral

- vtium-config gains the batch file, its folder of draft changes, and the install step for a system font. `vtium check-config` gains the trust rule's reasons and the size warning.
- The views table, the Fonts tab's scenarios, and the Looks tab's form follow the new tab order and roles. The Effects tab is a new view with scenarios of its own.

> Note 2026-11-19: D10, proposed on this date, would add these consequences. A person picks a color scheme in a look as they pick its fonts and effects, and a scheme written for Ghostty works by copying its file in. A palette works in every build, the minspec build included. On the other side, vtium follows a file format that another project defines, and keys of that format that vtium does not implement, such as `cursor-text`, do nothing. Every scheme vtium ships needs its license checked and its notice carried, so the shipped list stays short. A tinted look turns a scheme's colors into shades of one color, so the scheme shows its colors only in a look without a tint.

## Alternatives Considered

- **Draw from any font that any searched folder holds,** as `font-paths` does today. Not taken: the maintainer decided that activating a resource is an act of trust, and a folder the user or removable media controls cannot carry it.
- **Let a user's own font draw after login,** as ADR-1200 D25 noted. Not taken: the maintainer decided that a font the user installed is listed but not usable, and that the person installs it as a system font to use it.
- **Install a font as a side effect of activating it.** Not taken: the maintainer asked for an explicit step that shows its command and is confirmed each time.
- **Keep the Fonts view inside the Looks tab,** as ADR-1201 D3 decided on 2026-11-16. Not taken: the maintainer decided that the tabs follow the composition graph, with the fonts as resources of their own.
- **Call the selection "tags", or picking "tagging".** Not taken: the maintainer dislikes the word, and ADR-1403 retired it.
- **Manage fonts and effects on one Fonts tab.** Not taken: the maintainer decided that effects and filters would make the Fonts tab too busy, and that each kind previews differently.
- **Give pointer themes a tab of their own.** Not taken: the maintainer decided that each look picks its pointer theme in the Looks tab.
- **Say "in use" on screen, toggled with Use.** Not taken: the maintainer chose active, with Activate and Deactivate, the words that font managers already use.
- **An install script in vtium-fonts that edits batch.conf** (D4, amended 2026-11-27). Not taken: the maintainer chose a list file that the package installs and the package manager owns. A script would write root's own file from a package, and removing the package would leave its lines behind.
- **A longer list of a fresh install** that holds vtium-fonts's fonts too (D4, amended 2026-11-27). Not taken: the maintainer chose the package's list. The list of a fresh install applies only while batch.conf does not exist, so a machine with a batch.conf would never gain the package's fonts.

> Note 2026-11-19: the four alternatives below belong to D10, which is proposed.

- **Make a palette a color filter,** a look's table that maps each color to another. Not taken: the maintainer decided that palettes and filters are different resources. A filter changes the finished picture, so it cannot change the colors programs ask for and OSC 4 reports, and it would fold different colors that share a brightness into one under a tint.
- **Define a palette format of vtium's own,** in vtium.conf's syntax. Not taken: Ghostty's theme format already carries the colors a palette needs, and reading it as it is lets people use the schemes they already have.
- **Ship schemes from the iTerm2-Color-Schemes collection,** which Ghostty bundles. Not taken: the collection's license leaves each theme's copyright with its author, so each scheme still needs its author's license, which its own repository gives.
- **Keep palettes as built-in names only,** adding each new scheme to vtium-term's table. Not taken: the maintainer wants anyone to be able to write their own palette, which needs a file.
