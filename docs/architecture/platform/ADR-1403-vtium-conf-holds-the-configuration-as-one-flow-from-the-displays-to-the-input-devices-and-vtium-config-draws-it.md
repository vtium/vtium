---
contract: adr/v1
kind: decision
verb: change
capability: platform
amends:
  - 'ADR-1401#D3: the head file'
  - 'ADR-1201#D5: writing'
  - 'ADR-1201#D6: the head file has one writer'
  - 'ADR-1500#D4: the packaged vtium and a development vtium never collide'
basis:
  - operator: aaronsb
    level: directed
    said: "vtium's configuration describes one flow, from the displays to the input devices: displays, the rules that send each display to a head, the heads (a head is a terminal and the displays that show it), each head's look (its profile, frit and fonts), the rules that send each input device to a head, and the input devices. Every part is a known component, the rules that filter devices included, and the flow is nested objects in the configuration file."
    via: "the GitHub issue that records the maintainer's model for vtium's configuration, decided on 2026-11-04, section \"The model\"; posted from the maintainer's account; who wrote the wording is not recorded"
  - operator: aaronsb
    level: directed
    said: "The seat file goes. vtium stops reading `<seat>.seat.conf` (`seat0.seat.conf`), and the concept is removed from the console, vtium-config, the docs and the records. It came across from the C reference's multi-seat setup and no decision chose it. This is a departure from the C reference."
    via: "the same issue, section \"Decided\", item 1; posted from the maintainer's account; who wrote the wording is not recorded"
  - operator: aaronsb
    level: directed
    said: "One file, vtium.conf. vtium.conf holds the whole flow: its flat `key = value` lines stay as the defaults every head inherits, so a kmscon.conf copied in still reads, and the head, rule and look sections nest below them. heads.conf's content moves into vtium.conf."
    via: "the same issue, section \"Decided\", item 2; posted from the maintainer's account; who wrote the wording is not recorded"
  - operator: aaronsb
    level: directed
    said: "vtium-config draws the flow. A view in vtium-config shows the flow as columns, displays, display rules, heads, looks (profile, frit, fonts), input rules and input devices, each section in its own color, with lines connecting each component. The first slice is read-only and shows the file's flow with today's real displays and input devices placed on it, using the same read-only matching `vtium check-config` already does from sysfs and udev, and marks rules that match nothing. Editing in the view comes later."
    via: "the same issue, section \"Decided\", item 3; posted from the maintainer's account; who wrote the wording is not recorded"
  - upstream: "aaronsb/kmscon b1f7a7c5: kmscon_conf_load_main parsed the command line and then `<configdir>/kmscon.conf` (src/config.c:1500-1552); kmscon_conf_load_seat copied the main configuration and parsed `<configdir>/<seat>.seat.conf` over it (src/config.c:1554-1575), once for each seat as the seat was made (src/seat.c:786); parse_line read one `key=value` or switch a line, ended the line's text at a `#` anywhere but where it began a `#rrggbb` value, and knew no sections (src/misc/conf.c:402-500)"
  - evidence: "the tree on integration at 641961cf, read by this record's agent on 2026-11-04: vtium reads vtium.conf with no check of its owner or mode (crates/vtium/src/config/mod.rs, read_file), and heads.conf only when root owns it and no one else can write it (crates/vtium-settings/src/file.rs); vtium reads `seat0.seat.conf` over vtium.conf (crates/vtium/src/config/mod.rs, files and load; crates/vtium/src/main.rs:94), and vtium-config reads it for `font-frits` (crates/vtium-config/src/switch.rs:192-203); vtium.conf's line editor adds a new key at the end of the file (crates/vtium-settings/src/conf.rs); `apply` writes `vtium.conf` and `heads.conf` (crates/vtium-config/src/apply.rs:116-119); the TUI draws with reverse video alone and no color (crates/vtium-config/src/tui/view.rs:650-668); the mapping that turns a listed device into the identity the rules match lives in the console binary (crates/vtium/src/head/identity.rs, used by crates/vtium/src/check.rs:238); the shipped example vtium.conf holds 34,159 bytes and the head file's template 6,168, of a file bound of 65,536"
  - precedent: ADR-1401
  - precedent: ADR-1200
  - precedent: ADR-1201
  - precedent: ADR-1500
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "The syntax is option A, named sections: heads.conf's [head] and [rules] sections below the flat lines, with a [profile NAME] section for each look. The braces and the dotted flat keys go to the rejected alternatives. The rules are one ordered [rules] list after the heads, tried top to bottom across every head; that is the nesting meant."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
    covers: [nested objects]
  - operator: aaronsb
    said: "vtium refuses a vtium.conf that root does not own or that others can write, and says why. vtium-config gains an editing path like visudo's: an edit command opens a copy of vtium.conf in the user's editor, vim or any other, runs vtium check-config on the copy when the editor exits, and installs it owned by root only if it passes, offering to edit again otherwise. The TUI's saves go through the same check."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "There is no migration. vtium stops reading heads.conf and the seat file outright, with no warning period and no fold command. The only two installs that have them, the maintainer's desktop and laptop, are converted by hand."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
    covers: [no setup lost]
  - operator: aaronsb
    said: "Precedence between the levels (D5), the flow view (D7) and vtium-config's writes (D8) are accepted as drafted, with D8 extended by the editing path."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "The flow view is navigable: Tab moves between components, the arrow keys move within a column and along the lines, Enter opens the selected component, and the mouse can click a component. The view edits: opening a component offers its options to pick from, such as which head a display rule sends to, which profile a head uses, or a rule's selector from the devices present, or lets the user type a value, and every change goes through the same checked save as the rest of vtium-config, vtium check-config on the result before vtium.conf is installed. Displays, input devices, and heads and looks where it reads well, are drawn as minimal boxes, blocks of text on a background in their column's color, in place of line-drawn boxes, so each object reads as a shape; the lines between columns stay. The sixteen-color rule without blue holds, each foreground stays legible on its background, and every state keeps its words so the view works without color."
    via: the steering session, relaying the maintainer's input of 2026-11-04 after looking at the flow view's figure
    paraphrase: true
  - operator: aaronsb
    said: "Editing is in the first build: the flow view ships navigable and editable together, through the checked save."
    via: the steering session's question to the maintainer, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Everything works without the TUI, so it can be scripted: each change the flow view can make has a command-line form with stable exit codes, and the flow itself can be printed as text and as JSON, for example vtium config flow [--json], vtium config rule add, rule set, rule move, rule rm, vtium config head set NAME [--name NEW] [--profile P], vtium config head active NAME, vtium config profile set NAME [--frit F] [--size PT], and the visudo-style vtium config edit. Every one of them goes through the same checked save, and the TUI is a front end over the same operations."
    via: the steering session, relaying the maintainer's requirements of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "The view reads the devices present from sysfs and udev, read-only, as check-config does, and has a refresh action, a key in the TUI and a rescan on each run of a command. Unplugging a device never removes it from the configuration: a rule that names a specific device, by edid, connector, serial, path or name, whose device is not present shows that device as configured, not found, marked with an asterisk and words, in grey, still linked to its rule. Nothing is deleted unless the user deletes the rule."
    via: the steering session, relaying the maintainer's requirements of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "vtium config head active NAME is the run-time request through the planned control socket, with no start key for now. There are no separate commands to remove a whole head or profile; removing stays with vtium config edit. The flow view gets a new item at the bottom of the rules columns, the heads column and the looks column, so the whole configuration can be built in the view: a new display rule, a new input rule, a new head with a name and a look, and a new look, a profile with a name, frit and size. A look no head uses yet shows in the looks column as no head uses it. Since every view action has a command, D8 adds vtium config head add NAME --profile P and vtium config profile add NAME --frit F --size PT."
    via: the steering session, relaying the maintainer's answers of 2026-11-04 on the record's three open calls
    paraphrase: true
  - operator: aaronsb
    said: "Keep Tab past the last component going to the next view, a warning failing vtium config edit, mouse input, and the connector-path and bluetooth placeholders as written. Change the device reading: during active hardware management the view follows hotplug live. It watches udev, the same read-only source; a device plugged in appears at once, placed by the first rule that matches it, and a device unplugged turns into its configured, not found placeholder if a rule names it, or leaves the column if none does. The selection and any open editor stay put; if the selected device leaves, the selection moves to its placeholder or its neighbor, and the status line says what came and went. d still forces a full re-read. A placeholder can be deleted, which deletes the rule that names it; if the device is plugged in again it appears as a new device, placed by the remaining rules."
    via: the steering session, relaying the maintainer's word of 2026-11-04 on the record's five smaller calls
    paraphrase: true
  - operator: aaronsb
    said: "Space tags items across the columns, a display or device, a head, a look, and the tags show with a mark and a status line listing them. One action, C for connect, turns a complete tagged path into its changes: a display or device plus one head, or off, roaming or ignore, sets that device's own rule's target or adds a rule naming it before the general lines; one head plus one look sets the head's profile. The action shows the resulting commands before applying them, an incomplete or ambiguous tag set says what is missing, Esc clears the tags, and the changes still reach vtium.conf only through the checked save."
    via: the steering session, relaying the maintainer's idea of 2026-11-04 for the flow view
    paraphrase: true
  - operator: aaronsb
    said: "New rule placement: a rule that names one device, by edid, connector, connector-path, serial, path, bluetooth or name, goes before the first general rule of its kind, by class, id, card, udev or *; a general rule goes before the first * of its kind; else last. This applies to new rule…, tagging and rule add without --before. With no udev daemon, the status line says the view cannot follow hotplug, and d re-reads. Navigation: Tab or Left and Right move across the columns, Up and Down move within a column, Enter edits the properties of the selected card, Space tags cards to accumulate into a profile or configuration change, and C connects as before. Following a line is no longer on Left and Right; if kept, it gets another key."
    via: the steering session, relaying the maintainer's answers and design input of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "One known configuration state, and the user is usually not root: vtium-config keeps the user's working copy at $XDG_CONFIG_HOME/vtium/vtium.conf, edits go there, and apply installs it to /etc/vtium/vtium.conf through the checked, sudo-backed apply vtium-config already has. The view and vtium config flow always say which state they show, the working copy and whether it differs from the installed file. A user who is root may edit the installed file directly. The commands and vtium config edit are reconciled with this: each command edits the working copy unless told to apply."
    via: the steering session, relaying the maintainer's answers and design input of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Rules are taggable too: a tagged path may include a display rule or an input rule. A tagged rule plus a tagged head, or off, roaming or ignore, sets that rule's target; a tagged rule plus one tagged device makes that rule name the device, its selector set to the device's identity, unless it already matches it. A valid configuration wires at least one complete path from the left columns to the right: a display present is sent by its first matching rule to a head that at least one input device present types into, through an input rule to that head or to roaming. check-config refuses a file with no such path, with a message that says what is missing, so the checked save and every command fail on it with exit 1; the record says how this reads when no display or no input device is present, and keeps the refusal from locking out the shipped default."
    via: the steering session, relaying the maintainer's input of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Placement before the first general rule, by *, class, id, card or udev, is what the maintainer chose, and reading Esc as a key when it is the last byte of a read is fine."
    via: the steering session, relaying the maintainer's input of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Next to Save the view has Revert and New, and each asks for confirmation before it acts. Save, which applies, first runs the check, then asks whether to install the working copy, with a count of lines added and removed, and installs only on yes. Revert throws away the working copy's changes and copies the installed file back over it, after a confirmation that counts what is lost; the installed file is untouched. New empties the working copy to the flat defaults and one look named default, after a confirmation; nothing is installed until apply, and the complete-path rule then tells the user what to wire. The commands vtium config revert and vtium config new touch only the working copy; scripts confirm by running them, with --dry-run as for the others."
    via: the steering session, relaying the maintainer's input of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "The working copy may hold an unfinished configuration. Every change is written to it whatever the check says; the view and vtium config flow list what fails, and only apply requires a passing check, so New can start empty and a setup can be built in any order. A configuration viewer sits beside New, Save and Apply: a read-only pager over the live working copy the flow shows, unfinished or invalid, with vim-style keys, j and k or Up and Down for lines, h and l or Left and Right sideways, g and G for top and bottom, Ctrl-d and Ctrl-u for half pages, q or Esc back to the flow; it marks the lines the check complains about and the line of the card selected in the flow."
    via: the steering session, relaying the maintainer's decisions of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Drafts are always saved in the XDG path, never in /etc, which holds only the root-managed installed file; root also works on a draft under its own XDG config path and installs through apply. Drafts can be deleted with vtium config draft rm, and with an action next to Revert and New in the view behind a confirmation; the next edit starts again from the installed file."
    via: the steering session, relaying the maintainer's decisions of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Keep status text minimal: no running commentary such as a udev daemon runs; the status line shows only what the user needs to act on, the draft's state against the installed file, what changed, and what fails. The base configuration, the flat top-level keys such as policy, hwaccel, mouse, the keyboard layout and the font size, is editable in the view as well, through a base card opened with Enter like any card, and on the command line with vtium config set KEY VALUE and --unset KEY, both through the draft."
    via: the steering session, relaying the maintainer's decisions of 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Accept [as summarized: D1 to D8 as decided, and D9's code changes]"
    via: the steering session's summary of D1 to D9 in chat, 2026-11-04, final read
    paraphrase: true
status: accepted
date: 2026-11-04
deciders:
  - aaronsb
related: []
---

# ADR-1403: vtium.conf holds the configuration as one flow from the displays to the input devices, and vtium-config draws it

## Summary

- **Decided:** The maintainer decided on 2026-11-04 that vtium's configuration is one flow, from the displays to the input devices: the displays, the display rules that send each display to a head, the heads, each head's look (its profile, the frit the profile names, and the fonts that frit draws with), the input rules that send each input device to a head, and the input devices. The whole flow lives in one file, `/etc/vtium/vtium.conf`. Its flat `key=value` lines stay, and they are the defaults every head inherits, so a kmscon.conf copied in still reads. Below them come sections with names, as heads.conf writes them: a `[profile NAME]` section for each look, a `[head NAME]` section for each head, and one `[rules]` list, which each device tries top to bottom across every head (D4). vtium stops reading the seat file, `seat0.seat.conf`, which is a departure from the C reference, and stops reading heads.conf, with no migration: the two installs that have either file are converted by hand (D6). vtium refuses a vtium.conf that root does not own or that others can write, and says why (D3). vtium-config gains a view that draws the flow as six columns, each in its own color, with displays, heads, looks and input devices as blocks of color joined by lines. It places the machine's real displays and input devices on the file's flow, marks each rule that matches nothing, and moves and edits by keyboard and mouse from its first build, with `new…` items that build rules, heads and looks, every change going through the checked save. A rule that names one device keeps that device on the flow when it is unplugged, marked `configured, not found`, and nothing leaves the file unless the user deletes it (D7). vtium-config also gains `vtium config edit`, which works as visudo does: it opens the configuration in the user's editor and checks it with `vtium check-config` when the editor exits. Every edit, root's included, goes into the user's draft under the XDG path, `~/.config/vtium/vtium.conf`, which may be unfinished; only `apply` needs a passing check, and installs the draft to `/etc/vtium/vtium.conf` through the checked, sudo-backed apply. The view and `vtium config flow` always say how the draft stands, and a draft can be deleted (D8). The view also edits the top-level keys, through a base card, and opens the draft in a read-only pager (D7). Every change the view makes is also a `vtium config` command with fixed exit codes, and `vtium config flow --json` prints the flow for scripts, so everything works without the TUI (D8). A valid configuration wires at least one complete path, a display present sent to a head that a keyboard present reaches, and `vtium check-config` refuses a file without one, so nothing is installed that no one at the machine can type on; the shipped default always passes (D3). In the view, Save, Revert and New each ask before they act, and rules can be tagged with devices and heads (D7). This record's agent adds what the move forces: the lines above the first section keep vtium.conf's forgiving reading, and the sections keep the head file's strict one, so an error in a section never drops a top-level key (D3).
- **Trades away:** One file holds two ways of handling a mistake, a forgiving one for the flat keys and a strict one for the sections. A heads.conf or seat file left in place stops working, with no warning. A vtium.conf that root does not own, which vtium reads today, is no longer read at all. While the packaged build and the development build share `/etc/vtium/vtium.conf` (ADR-1500 D4), a build older than this change reads a head section's `login` line and a profile section's font lines as if they were top-level keys. The view's columns need a terminal at least 80 columns wide, and a narrower one, such as a minspec board's 40-column console, shows the flow as an indented list instead.
- **One-way?** No, but expensive once a release ships it: users then write their files in the merged form, and a later change of syntax means a migration. Until that release every part of this changes in a session.
- **Probes:**
  - *Not confident (nested objects):* the recommendation keeps heads.conf's bracketed sections as they are, placed below vtium.conf's flat lines, and adds a `[profile NAME]` section for each look, so a head names its profile as it does today. The rules stay one ordered list in `[rules]`, outside the heads, because first match depends on their order across all heads. Is that the nesting you meant, or did you mean each head's rules written inside that head? (Answered 2026-11-04: that is the nesting meant; see `considered`.)
  - *Confident (no setup lost):* an installed heads.conf keeps working, with a warning at each start, until `vtium config fold` moves it into vtium.conf or the first beta release stops reading it. An installed `seat0.seat.conf` stops being read with this change; vtium says so at each start, and the same command folds its keys into vtium.conf. (Answered 2026-11-04: no migration, the two installs are converted by hand; see `considered` and D6.)
- **Inversion:** Between "a file for each concern", the C reference's kmscon.conf with a seat file beside it, as vtium has today with heads.conf and planned profile files, and "one document that holds everything the console reads". vtium takes the second for the flow of devices, heads and looks, and keeps frits, vtlooks and shaders as files of their own, because the glyph helper and the renderer read them and vtium-config manages them one by one.

> Edited 2026-11-04: the maintainer answered the probes and the open questions the same day (`considered`), and the record now states the answers as decisions. The Summary listed five open questions with recommendations, and its first two sentences on the move read: "The head file's sections, today in `/etc/vtium/heads.conf`, move into vtium.conf below those lines. vtium stops reading the seat file, `seat0.seat.conf`, which is a departure from the C reference." The earlier text is in git at 33ea5db0.

## Context

vtium's configuration is spread over three files today. vtium.conf holds the keys the C reference's kmscon.conf had, plus vtium's own. `seat0.seat.conf`, beside it, holds the same keys and wins over vtium.conf, because the C reference read a file for each seat over its main file and vtium carried that over without a decision. heads.conf holds the heads and the rules that place each display and each input device (ADR-1401 D3), and profiles were to get files of their own (ADR-1200 D16). A head inherits its font from vtium.conf, its frit from its profile, and its devices from heads.conf's rules, so no single file shows how a display and a keyboard end up in a terminal with a font.

The tree constrains the move in four ways, each read from the code on 2026-11-04 (`basis`). vtium reads heads.conf only when root owns it and no one else can write it, and it reads vtium.conf with no such check. The two files handle mistakes differently: vtium.conf reports an unknown key or a bad value and goes on, keeping the last value a key is given, while heads.conf drops what each error takes under ADR-1401 D7's three levels and treats a key given twice as an error. vtium.conf's line editor adds a new key at the end of the file, which in a file with sections lands inside the last section. And vtium-config draws its screens with reverse video alone, so the flow view brings the first color into it.

## Decision

D1 to D3 state the maintainer's decisions and what the move forces. D4 to D8 were the open questions of this record's draft, which the maintainer answered on 2026-11-04 (`considered`); each states the answer. D9 lists what the code changes to fit.

### D1: one flow, from the displays to the input devices

vtium's configuration describes one flow, and every part of it is a component with a name and a place in vtium.conf (the maintainer, 2026-11-04):

| Section of the flow | What each component is | Where it is written |
|---|---|---|
| Displays | a monitor or panel on a connector, as vtium finds it | nowhere: the machine supplies them |
| Display rules | a `display MATCH -> TARGET` line, which sends each display it matches to a head, or `off` | `[rules]` |
| Heads | a terminal and the displays that show it (ADR-1401 D1) | `[head NAME]` |
| A head's look | the head's profile, the frit (a font set file) the profile names, and the fonts that frit draws with | the profile's section, the frit's file, the font files |
| Input rules | an `input MATCH -> TARGET` line, which sends each input device it matches to a head, to `roaming` or to `ignore` | `[rules]` |
| Input devices | a keyboard, pointer or other input, as vtium finds it | nowhere: the machine supplies them |

- **The rules filter devices, and they are components too.** Each device walks the rules of its own kind, top to bottom, and the first that matches places it, as ADR-1401 D3 decides. The rules stay one ordered list, because which rule matches first depends on their order across every head.
- **The targets that are not heads** are components as well: `roaming`, which types into the active head, `ignore` and `off` (ADR-1401 D3). So is the policy, which places a device no rule matches.
- **"Look" in this record** is the maintainer's word for what a head draws with: its profile, frit and fonts. ADR-1304's file of frame filters is always called a vtlook here. Where ADR-1304 is built, the vtlook a head names with `look =` belongs to the head's look too, and the flow view shows it beside the profile.

### D2: the seat file goes

The maintainer decided on 2026-11-04 that vtium stops reading `<seat>.seat.conf`, and that the seat file is removed from the console, vtium-config, the docs and the records. It came across from the C reference's setup for several seats, and no decision chose it.

- vtium reads `vtium.conf` in its configuration directory and then its command line, and nothing else.
- vtium serves seat0 alone (ADR-1401 D2), so the only seat file it read was seat0's. When the later extension for other seats is built, a seat's flow goes into vtium.conf with the rest, and ADR-1401 D2's sentence that the head file gains a file for each seat no longer holds.
- vtium does not look for a seat file, and says nothing of one left in place (D6).

This is a departure from the C reference, which read `<configdir>/<seat>.seat.conf` over kmscon.conf for each seat (C notes).

### D3: one file, vtium.conf

The maintainer decided on 2026-11-04 that vtium.conf holds the whole flow. Its flat `key=value` lines stay as the defaults every head inherits, so a kmscon.conf copied to `/etc/vtium/vtium.conf` still reads, and the head, rule and look sections come below them. heads.conf's content moves into vtium.conf, and vtium no longer reads heads.conf (D6).

- **Trust covers the whole file, and vtium says why it refuses one** (the maintainer, 2026-11-04). vtium reads vtium.conf only when root owns it and neither its group nor others can write it or a directory above it, the check ADR-1401 D7 and D10 make of heads.conf today. vtium refuses a vtium.conf that fails the check: it reads none of it, runs its compiled-in keys and flow, and every head shows `invalid config skipped, using builtin` with its next step, `Run vtium check-config /etc/vtium/vtium.conf to see why.` The log and `vtium check-config` say why: the file or directory that fails, its owner or its mode, and the command that mends it, `chown root:root` or `chmod go-w` with the path. The check covers the top level as well as the sections because vtium.conf now places keyboards, and because its `login` key already names the command vtium starts as root. A kmscon.conf copied in by root, with the usual mode 0644, passes.

The move forces these decisions, which this record's agent makes from ADR-1401 and the tree:

- **The top level and the sections.** The top level is every line above the first section heading. It holds vtium's keys, as vtium.conf does today, and the two keys heads.conf had above its first section, `version` and `policy`. Everything from the first section heading on is the sections. A key line after a section heading belongs to that section, so the top level is written first, as heads.conf's top-level keys were.
- **A file with no section is a kmscon.conf.** It needs no `version` line, and vtium runs the compiled-in flow for it (ADR-1401 D6), as it does today when heads.conf is missing. A file with a section needs `version = 1` at the top level. A missing or unknown version is ADR-1401 D7's error that drops the whole head file but its policy; under one file it drops every section but the policy, and the top-level keys still apply.
- **Two ways of handling a mistake, one for each part.** The top level keeps vtium.conf's reading, which follows the C reference's: an unknown key is reported once and ignored, a value out of bounds is clamped, a value vtium cannot read is ignored, and a key set twice takes its last value. The sections keep ADR-1401 D7's three levels unchanged, and what ADR-1401 D7 calls the whole file is now the sections and the policy. An error in a section never drops a top-level key, and a top-level mistake never drops a section.
- **The policy line.** ADR-1401 D7's rule for a policy line holds: a line above the first section that begins with `policy`, in any mix of case, is a policy line, and one vtium cannot read makes the sections closed and broken. Among vtium's keys none begins with `policy`, so no existing key changes meaning.
- **One bound for the file.** vtium.conf keeps its bounds, 65,536 bytes a file and 8,192 a line, which heads.conf shares. The shipped example vtium.conf and the head file's template together hold 40,327 bytes. A test fails when the generated example passes 49,152 bytes, so at least 16 KiB stay for a user's own heads, profiles and rules.
- **One source for the compiled-in flow.** The head file's template, `crates/vtium-settings/templates/heads.conf`, becomes the sections of the shipped example: the example vtium.conf is the key table's commented lines, then `version = 1`, then the template's sections with the shipped rules as live lines, as ADR-1401 D6 has them. vtium's compiled-in flow is still parsed from that same text.
- **`vtium check-config FILE`** checks a candidate vtium.conf, both parts in one run with one exit status, where today FILE names a candidate head file.

> Edited 2026-11-04: the maintainer decided that vtium refuses a vtium.conf it cannot trust and says why, and that heads.conf is not read (`considered`). The trust bullet was this record's agent's call among the forced decisions, and said only that every head shows `invalid config skipped, using builtin`.

#### A valid configuration wires at least one complete path

The maintainer decided on 2026-11-04 that a valid configuration wires at least one complete path from the left of the flow to the right (`considered`). `vtium check-config` refuses a file without one, as an error, so Save and `apply` fail on it with exit 1, and the message says what is missing. The draft may still lack one while the user builds it (D8); the view and `vtium config flow` list what is missing until it is wired.

- **A complete path** is a display present that its first matching rule sends to a head, and at least one keyboard present that reaches that head: through an input rule whose target is that head, or by roaming, from a rule whose target is `roaming` or, under `policy = open`, from no rule at all, when the head can be the active head because it is not isolated (ADR-1401 D5). A keyboard is an input device of udev's keyboard class (ADR-1401 D4), since a mouse or a media-key device types nothing.
- **What the message says** names the first thing missing along the way, in the order a person would wire it, such as `No display present is sent to a head: display DP-8 falls to rule 3, which sends it to off.`, or `Head desk shows DP-8, but no keyboard present reaches desk: add an input rule that sends a keyboard to desk or to roaming.`, or `Head desk is isolated, so roaming keyboards do not reach it.` The flow view shows the same message on its status line while the draft has no complete path.
- **With no display or no keyboard present,** as when the check runs in a container, on a build machine, or on a desktop checking a file for another machine, the devices present cannot show a path. The check then judges the rules alone: some display rule sends displays to a head that some input rule sends keyboards to, directly or by roaming as above, with a `class:keyboard`, `*` or device rule. It notes that it judged without devices. `vtium check-config --no-devices FILE` asks for that reading on any machine, so a file written for another machine, whose rules name that machine's monitor and keyboard, can be checked where those devices are absent.
- **The shipped default always passes.** Its rules, `display * -> console` and `input class:keyboard -> roaming` with a head that is not isolated, wire a complete path on every machine with a display and a keyboard, and by the rules alone where none is present. A test holds the compiled-in configuration and the shipped example to both readings. With no file at all, nothing is checked, and vtium runs the compiled-in configuration as ADR-1401 D6 has it.
- **The console does not apply this rule when it starts.** Missing hardware never makes a file invalid at start (ADR-1401 D7): a keyboard plugged in after boot is normal, and ADR-1401 D9's guards keep a keyboard typing whatever the rules say. The rule belongs to `vtium check-config` and to vtium-config's checked save, so a file is not installed without a way for someone at the machine to type.

### D4: the syntax of the sections

The maintainer chose on 2026-11-04 sections with names, as heads.conf writes them (`considered`). Below the top level come a `[profile NAME]` section for each look, a `[head NAME]` section for each head, and one `[rules]` section after the heads, whose rules each device tries top to bottom across every head. This is the nesting the maintainer meant: a section's heading names the object, every line below it up to the next heading belongs to it, and a deeper link is a key that names another section, as git's `[branch "main"]` names its remote with `remote = origin` and a head names its profile with `profile = big`. The rules stay one list outside the heads because first match depends on their order across every head (ADR-1401 D3). A vtium name holds no blank (ADR-1401 D3), so a heading needs no quotes, as git's do.

A whole vtium.conf for a machine with two displays, a large monitor and a small panel, and one keyboard that types into whichever head is active:

```ini
# /etc/vtium/vtium.conf: two displays, one keyboard.
# The top level: vtium's keys, the defaults every head inherits.
font-size=12
xkb-layout=us
mouse
version = 1
policy = open

# A head's look: the profile, which names a frit with font-name.
[profile big]
font-name = monoid
font-size = 14

[profile small]
font-name = dejavu

[head desk]
profile = big

[head panel]
profile = small
consoles = 2

# Each display and each input tries these rules top to bottom,
# across every head, and the first that matches places it.
[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display edid:BBB:0x0002:SMALL00001 -> panel
display * -> desk
input class:keyboard -> roaming
input * -> ignore
```

- **heads.conf's sections move in unchanged.** Its parser and ADR-1401 D7's drop rules read them, and ADR-1304's vtlook file is written in the same syntax.
- **A `[profile NAME]` section is the profile's file** that ADR-1200 D16 left for later. It takes the keys D5 lists, under the same names the top level uses.
- **One comment rule serves the whole file,** the head file's: a `#` starts a comment at the start of a line or after a blank, a `#` inside a word is part of the word, and `\#` gives a literal `#` after a blank. vtium.conf's exception is kept: a `#` that opens a `#rrggbb` value is part of the value. A value at the top level can then hold a `#` inside it, as a head's `login` command already can. In one case a top-level line reads differently from the C reference: a `#` directly after a value, with no blank before it, as in `font-size=12#big`, stays in the value, which vtium then reports as one it cannot read. This is listed as a departure.
- **Section keys always take `key = value`;** the switches `key` and `no-key` stay at the top level.

Blocks in braces, as sway writes them, TOML and dotted flat keys were weighed and lost; Alternatives Considered gives each one's reasons.

### D5: how a key set at two levels combines

The maintainer accepted these rules on 2026-11-04 as this record drafted them (`considered`):

- **Three levels, and the most specific wins:** a head's section, then the section of the profile the head names, then the top level. The command line sets top-level keys and wins over the file's top level, as it does today, and a section still wins over both, as a head's profile wins over vtium.conf's `font-name` today (ADR-1200 D16). A key the head and its profile leave unset takes the top level's value, and a key the top level leaves unset takes its default.
- **Each section kind takes a fixed list of keys.** A head takes ADR-1401 D3's head keys, `profile`, `consoles`, `login`, `when-display-lost` and `isolated`, and ADR-1304 D5's `look` once it is built. A profile takes the keys ADR-1200 D16 gives a profile that the key table has: `font-name`, which names the profile's frit, `font-engine`, `font-size`, `font-hinting`, `font-line-spacing`, `display-depth` and `display-dpi`. A head sets no font key of its own, since a head's font has one path, from the head to its profile to the profile's frit (ADR-1401 D3).
- **A key in a section has the top level's name, values and bounds** where the top level has the key too, as `login` and the font keys do. Inside one section a key given twice is an error, as in heads.conf today.
- **A key a section kind cannot take is an error in that section,** which drops the section under ADR-1401 D7: a head drops with its rules, and a profile drops, so a head that names it takes the default profile (ADR-1200 D16). The error names the levels where the key may be set.
- **The key table records where each key may be set.** Its column that says a key is set in a file or on the command line gains the values `profile` and `head`, so docs/config.md lists each key's levels, and allowing one more key in a section later is an edit to one row.
- **Keys that belong to the whole process stay at the top level,** such as `display-buffers`, `consoles-max`, `hwaccel` and `sb-memory`, for the reason ADR-1401 D2 gives for `display-buffers`: they trade the machine's memory or set the process's behaviour, and are the same for every head.

The other way is to let every key be set in a head's section. It would give each head its own palette or keyboard layout, but many keys belong to the process, the seat or an input device rather than a head, and each would need a rule for what a head's value means. A key can move into the head's list later, one row at a time, when someone needs it.

### D6: no migration

There is no migration, as the maintainer decided on 2026-11-04 (`considered`). With this change vtium stops reading `/etc/vtium/heads.conf` and `seat0.seat.conf` outright: it does not look for either file, shows no warning, and has no period in which it reads either, and vtium-config has no command that folds them into vtium.conf. The only two installs that have either file are the maintainer's desktop and laptop, which the maintainer converts by hand, so the code and the docs carry no path that nobody else needs. A heads.conf or seat file left in `/etc/vtium` does nothing, and vtium never removes or changes it (ADR-1401 D6).

> Edited 2026-11-04: the maintainer decided that there is no migration (`considered`). D6 was headed "migration (open)" and recommended that vtium read heads.conf with a warning until the first beta release, that `vtium config fold` move heads.conf and the seat file's keys into vtium.conf and rename the old files to `.bak`, and that packages keep tracking heads.conf until reading stopped. The earlier text is in git at 33ea5db0.

### D7: the flow view

The figure shows the view on the machine of D4's example, with one more input rule for a keyboard that is unplugged, as it looks on a dark console. Rule 5 is selected and opened for editing below the columns.

![vtium-config's flow view on a black console background, under the tabs Fonts, Sizes, Settings, Heads and Flow (selected), followed by the actions View, Save, Revert, New and Delete draft. A status line reads draft ~/.config/vtium/vtium.conf: 1 line added, 1 removed, no problems. Below it, the base card, black on light grey, reads base, policy open, font-size 12, xkb-layout us, mouse, 101 keys at their defaults. Six columns follow, each heading in its own color. DISPLAYS: two blocks of black text on bright cyan, DP-8 (LARGE00001) and HDMI-A-2 (SMALL00001). DISPLAY RULES, as cyan text: rule 1, edid LARGE00001 to desk; rule 2, edid SMALL00001 to panel; and rule 3, display star to desk, in grey with the words matches no display present; then + new rule…, in cyan. HEADS and LOOK: for each head a block of black text on bright yellow beside a block of black text on bright magenta: desk, 1 console, the active head, with profile big at 14 pt, frit monoid and the font Monoid; panel, 2 consoles, with profile small at 12 pt, frit dejavu and the font DejaVu Sans Mono; under them + new head… in yellow and + new look… in magenta. Below them, blocks of black text on grey: roaming (types into desk, active), ignore (vtium never opens it) and off (vtium sets it off). INPUT RULES: rule 4, a serial match to desk, in grey; rule 5, class keyboard to roaming, selected, so drawn in reverse video as black on green; and rule 6, star to ignore, in green; then + new rule…, in green. INPUT DEVICES: a grey block marked with an asterisk, 046d:c31c:…, with the words configured, not found, joined to rule 4 by a dashed line; and two blocks of black text on bright green, the keyboard at usb-0:3.3 and the Power Button, class key. Cyan arrows run from each display block to its rule and from each rule to its head; green arrows run from each input device to its rule and from each rule to its target; grey dashed arrows run from rules 3 and 4 to desk. A legend explains the solid and dashed lines and the asterisk. Below a green line, the opened rule: rule 5, line 23, input class:keyboard -> roaming; sends to desk, panel, roaming (marked as the current choice) or ignore; matches class:keyboard (marked), path:pci-0000:15:00.0-usb-0:3.3 or id:046d:c31c, or type a selector. A key line reads Tab and Left Right for columns, Up Down to move, Enter edit, Space tag, c connect, f follow, d devices, v view, s save, r revert, n new, x delete, q leave.](figures/ADR-1403-flow-view.svg)

The maintainer accepted the columns, lines, colors and states as this record drafted them, and on looking at the figure decided that the view is navigable and edits, that its objects are drawn as blocks of color, and that editing is in the first build (`considered`, 2026-11-04):

- **The columns,** left to right: displays, display rules, heads, look, input rules, input devices. Each head sits in the heads and look columns as two blocks side by side, the head's name and consoles on the left and its profile, frit and fonts on the right, so lines from display rules reach the head's block from the left and lines from input rules reach the look's block from the right, and no line crosses the look column. A profile that no head names yet still shows in the look column, as a block of its own below the heads' looks with the words `no head uses it`. Below the heads, blocks for `roaming`, `ignore` and `off` take the rules that target them, and a block for the policy takes the devices no rule matches, with the words of what the policy gives them.
- **New items.** The display rules column and the input rules column each end with a `new rule…` item, the heads column with `new head…` and the look column with `new look…`, so the whole configuration can be built in the view (the maintainer, 2026-11-04). Opening `new rule…` starts a rule of that column's kind with the choices an existing rule offers (below). Opening `new head…` asks for the head's name, which follows ADR-1401 D3's rule for names, and its look, picked from the profiles in the file, `default`, or a new look made on the spot; until a rule sends something to it, the head carries the note `vtium check-config` gives a head no rule names. Opening `new look…` asks for the profile's name, its frit, picked from the frits vtium-config lists, and its size. Where a new rule goes is decided by what it names (the maintainer, 2026-11-04), so that an earlier, wider rule does not match its devices first and hide it. A rule that names one device, by `edid`, `connector`, `connector-path`, `serial`, `path`, `bluetooth` or `name`, goes before the first general rule of its kind, one that names a kind of device, by `class`, `id`, `card`, `udev` or `*`. A general rule goes before the first rule of its kind whose selector is `*`. Any other rule, and any rule with nothing of the sort to go before, goes after the last rule. The same placement serves `new rule…`, connecting tags, and `vtium config rule add` without `--before`; the user moves a rule from there. Removing a whole head or a profile is not offered in the view; it is done with `vtium config edit` (D8).
- **Objects are blocks of color; rules are text.** A display, an input device, a head, a look and a target that is not a head are each drawn as a block: its text on a background of its column's color, with no line around it, so each object reads as one shape. A rule is drawn as text in its column's color, since a rule is a filter between objects, not an object. The lines between columns stay as they are.
- **Rules are numbered.** Each rule shows its number, its place in `[rules]` counted from 1, which is the number the commands of D8 take; opening a rule also shows its line in the file.
- **What a line means.** A line from a device to a rule says that rule is the device's first match. A line from a rule to a block says that block is the rule's target. Every rule has its target line whether or not any device is present; only devices present have lines of their own. The look's entries hang from the profile as a small tree: the profile, the frit it names, and each font the frit draws with that this machine has.
- **Colors,** each section in its own, from the console's sixteen colors so the user's palette applies: displays bright cyan, display rules cyan, heads bright yellow, the look bright magenta, input rules green, input devices bright green, and grey, the palette's dark grey, for the targets that are not heads and for what is not in play. Bright red marks an error. Every block's text is black, the palette's color 0, which stays legible on each of these backgrounds under the default palette: from about 5:1 on grey and bright red, through about 7:1 on bright magenta, to about 20:1 on bright yellow. The view uses no blue, since the palette's blue on black is hard to read, and no color as the only sign of anything: every state also has its words. With `NO_COLOR` set or on a terminal without color, the view draws the same text with no color, each block's text in brackets.
- **A rule that matches nothing** present is drawn in grey, and its line to its target is dashed. A rule whose selector names a kind of device, by `*`, `class`, `id`, `card` or `udev`, carries the words `matches no display present` or `matches no device present`. A rule an earlier rule shadows, a profile that names a frit that does not exist, a font the frit names that this machine lacks, and a line the file's errors drop each show in place with their own words, the same words `vtium check-config` prints, and an error in bright red.
- **A device that is configured but not found.** A rule whose selector names one device, by `edid`, `connector`, `connector-path`, `serial`, `path`, `bluetooth` or `name`, stands for that device whether it is plugged in or not. When the device is not present, the view still draws it in its column, as a grey block marked with an asterisk and the words `configured, not found`, named by the selector's value and linked to its rule by a dashed line. Unplugging a device never removes it from the configuration: vtium-config deletes a rule only when the user deletes it, in the view, by deleting the rule or its device's block, or with `vtium config rule rm` (D8) (the maintainer, 2026-11-04).
- **The devices present** are read from sysfs and udev's database, without privilege and opening nothing under `/dev`, as `vtium check-config` reads them (ADR-1401 D7), when the view opens. Each command of D8 reads them afresh on every run.
- **The view follows hotplug live** (the maintainer, 2026-11-04). While it is open it watches udev's device events, the same read-only source, through the monitor vtium-platform gives the console, so the user can plug and unplug devices while configuring them:
  - A device plugged in appears in its column at once, placed by the first rule that matches it, or by the policy when none does.
  - A device unplugged turns into its `configured, not found` block when a rule names it, and leaves the column when none does.
  - The selection and any opened component stay where they are. When the selected device leaves, the selection moves to its `configured, not found` block, or to its neighbor in the column when it has none.
  - The status line says what came and what went, such as `keyboard at usb-0:3.3 came; Power Button went`, until the next change.
  - `d` still reads every device again from the start, for an event the view missed.
  - With no udev daemon no events arrive, so the status line says that the view cannot follow hotplug, and `d` is how the user brings it up to date.
- **Deleting a `configured, not found` block deletes the rule that names it,** since the block stands for that rule's device. When that device is plugged in again, it appears as a new device and is placed by the rules that remain.
- **With no udev daemon,** the status line says so, and the view lists the devices sysfs shows, with the input classes vtium's own copy of udev's `input_id` rule gives (ADR-1401 D4). Each rule that uses `udev:`, `path`, `serial`, `card` or `connector` is drawn in grey with the words `needs udev`, since it matches nothing on that machine, which is the note `vtium check-config` gives.
- **Moving** (the maintainer, 2026-11-04). Tab and the Right arrow move to the next column, and Shift-Tab and the Left arrow to the column before, landing on the item nearest the same row. Tab past the last column goes on to the next view of the TUI, and Shift-Tab before the first to the view before, so Tab still steps through the views as it does in every other view; the arrows stop at the first and the last column. The Up and Down arrows move within a column. `f` follows a line: it moves to the component the selected one's line reaches, toward the heads, and pressed again it steps through the others when there are several. The selected component is drawn in reverse video, the TUI's selection mark today, so a selected block shows its color as text on black. `d` reads the devices present again from the start. The mouse selects a component with a click and opens it with a second click, and a click on a view's name in the tab bar switches to that view. The view turns on mouse reporting, modes 1000 and 1006, which vtium supports (ADR-1000 `DEC-1000`, `DEC-1006`), and turns it off when the TUI leaves the view; in a terminal without mouse reporting the keys do everything.
- **Opening and editing.** Enter opens the selected component below the columns to edit its properties; a rule opens with its number and its line. Each choice offers the values it can take to pick from, with the current one marked, and lets the user type a value instead. A display rule offers its target, from the heads in the file and `off`, and its selector, from the match keys of the displays present, the strongest first, as `vtiumctl detect` lists them (ADR-1401 D12). An input rule offers its target, from the heads, `roaming` and `ignore`, and its selector from the inputs present in the same way. A head offers its profile, from the profiles in the file and `default`, and its other keys, `consoles`, `login`, `when-display-lost` and `isolated`. A profile offers its frit, from the frits vtium-config lists, and its other keys (D5). A rule can also be added for the selected device, moved up or down, since its place decides what it matches first, or removed. A device or a display is not edited, since the machine supplies it; opening one shows its identity, every match key it offers, and the rule that places it.
- **Tagging and connecting** (the maintainer, 2026-11-04). Space tags the selected item, or untags it, in any column: a display or an input device, a `configured, not found` block, a display rule or an input rule, a head, a target that is not a head (`off`, `roaming` or `ignore`), or a look. A tagged item shows the mark `◆` before its name, and the status line lists every tag, such as `tagged: keyboard, desk, big`. `c`, connect, turns the tags into the changes they describe:
  - **Devices and a head.** One or more displays or input devices with one head, or with `off` for displays or `roaming` or `ignore` for inputs, send each of those devices there. Where the device has a rule of its own, the first rule that names that one device, `c` sets that rule's target. Otherwise it adds a rule naming the device by its strongest key, `edid` for a display and `path` with `serial` where the device has a serial, `path` alone otherwise, as the head file's template advises (ADR-1401 D4), where a new rule goes (above), before the first general rule of its kind.
  - **A rule and a head.** One tagged rule with one head, or with `off` for a display rule or `roaming` or `ignore` for an input rule, sets that rule's target (the maintainer, 2026-11-04).
  - **A rule and a device.** One tagged rule with one tagged device of its kind makes the rule name that device: its selector becomes the device's identity by its strongest key, as above, unless the rule already matches the device, when nothing changes. A rule with a device and a head makes both changes.
  - **A head and a look.** One head with one look sets the head's profile to that look.
  - **The whole path.** Devices, one head and one look together make both changes.
  - **Before anything changes,** `c` shows the commands of D8 that the changes amount to, such as `vtium config rule add --before 5 'input path:pci-0000:15:00.0-usb-0:3.4 -> panel'` and `vtium config head set panel --profile small`, and adds them to the view's changes only when the user confirms. The tags clear once they are added. Each change goes into the working copy through the check, as every change does, and reaches the installed file only through Save.
  - **A tag set that describes no change** changes nothing, and `c` says what is missing or what does not fit: no head, two heads, two looks, two rules, a rule with a device of the other kind, a look with devices but no head, a display tagged with `roaming`, or an input device tagged with `off`.
  - **Esc clears every tag.**
- **Every change goes into the draft at once.** The view shows the user's draft of vtium.conf, the working copy of D8. Each change the user makes is written to the draft as it is made, through the writers of D8, whatever the check says, so a configuration can be built in any order and left unfinished. The view runs `vtium check-config` on the draft after each change, shows each problem in place on the card it concerns, and the status line counts what fails. The view redraws the flow from the draft at once, placing the devices present by the same matching, so the user sees where each device goes before anything is installed (the maintainer, 2026-11-04).
- **The status line is minimal** (the maintainer, 2026-11-04). It shows only what the user needs to act on: the draft's state against the installed file, as lines added and removed, what just changed, such as a device that came or went, and what fails, such as the complete path still missing (D3). It carries no running commentary.
- **The base card.** The flat top-level keys, such as `policy`, `hwaccel`, `mouse`, `xkb-layout` and `font-size`, which every head inherits (D5), are a card of their own, `base`, in a row above the columns (the maintainer, 2026-11-04). Enter opens it like any card: it lists the keys the draft sets, and every other key of the key table at its default, grouped as docs/config.md groups them, and each offers its values to pick from, the choices the key table gives, or a value to type. Changes go through the line editor of D8 into the draft's top level. `vtium config set KEY VALUE` and `vtium config set --unset KEY` do the same from the command line (D8).
- **View, Save, Revert, New and Delete draft** (the maintainer, 2026-11-04) sit next to each other in the tab bar, and `v`, `s`, `r`, `n` and `x` reach them from the keyboard. Each but View asks for confirmation before it acts, and acts only on yes:
  - **View** opens the draft in a read-only pager: the live draft the flow shows, unfinished or invalid as it may be. `j` and `k` or Up and Down scroll by lines, `h` and `l` or Left and Right scroll sideways, `g` and `G` go to the top and the bottom, Ctrl-d and Ctrl-u move half a page, and `q` or Esc go back to the flow. It marks each line the check complains about, and the line of the card selected in the flow, and opens at that line.
  - **Save** installs the draft, as `vtium config apply` does (D8). It runs the check first, and refuses, listing what fails, when the check does not pass; when it passes, it asks `Install the working copy? 3 lines added, 1 removed.` and installs only on yes.
  - **Revert** throws away the working copy's changes and copies the installed file back over it, after asking `Throw away the working copy's changes? 3 lines added and 1 removed are lost.` The installed file is untouched.
  - **Delete draft** removes the draft, after asking `Delete the draft? 3 lines added and 1 removed are lost.`, as `vtium config draft rm` does. The next change starts a new draft from the installed file. The installed file is untouched.
  - **New** empties the draft, after asking `Start a new configuration? The working copy's heads, rules and looks are lost.`, to the top level at its defaults, every key commented out as in the shipped example, and one look, `[profile default]`, with no keys. Nothing is installed until Save, and until the user wires a complete path, the status line says what is missing (D3).
  An installed change takes effect when vtium next reads its configuration.
- **Narrow terminals.** The columns need 80 columns. On a narrower terminal the view draws the same flow as an indented list, one block for each head with its displays, rules, look and inputs beneath it, then the roaming, ignore, off and policy blocks, so it fits the 40 columns of a minspec console. Moving, opening and saving work the same in the list.
- **How to reach it.** A tab named Flow in the TUI, after Heads, and a command, `vtium config flow`, that prints the flow as text, or as JSON with `--json`, and exits. Every change the view makes is also a command (D8), and the view is a front end over the same operations. The view reads vtium.conf, the frits and the devices as the user, with no privilege, from the console or a desktop terminal; only the save asks for sudo. On a desktop, displays are named as the kernel names them.
- **Editing is in the first build.** The view ships navigable and editable together, through the checked save.

> Edited 2026-11-04: the maintainer decided that the view is navigable and edits, that its objects are blocks of color, and that editing is in the first build (`considered`). This reverses the first version reading only, which the maintainer's decision of the same morning set (`basis`). D7 drew each head as a line-drawn box, named the arrow keys and Enter alone for moving, and ended: "It only reads. The first version changes nothing. Editing in the view comes later, through the same writers as every other edit (D8)." The figure drew line-drawn boxes and no opened component. The earlier text and figure are in git at 7453e1bd.

### D8: what vtium-config writes

vtium-config wrote vtium.conf, heads.conf and frits through drafts that `sudo vtium-config apply` checks and installs (ADR-1201 D5, D6). The maintainer decided on 2026-11-04 that vtium-config keeps one working copy of the configuration, which every edit changes and `apply` installs, added the edit command and the commands for scripts, and accepted the rest as this record drafted it (`considered`).

#### The working copy and the installed file

- **One known state, the draft** (the maintainer, 2026-11-04). vtium-config keeps the user's working copy of vtium.conf, which the view and the commands call the draft, at `$XDG_CONFIG_HOME/vtium/vtium.conf`, which is `~/.config/vtium/vtium.conf` when `XDG_CONFIG_HOME` is unset. Every edit, from the TUI, a command or `vtium config edit`, changes the draft. Drafts live only under the XDG path, never in `/etc`, which holds only the installed file that root manages, `/etc/vtium/vtium.conf`. vtium reads only the installed file, so nothing the user edits reaches the console until it is applied.
- **Where the working copy comes from.** When there is none, vtium-config makes it as a copy of the installed file, or of the shipped example when no vtium.conf is installed. Beside it, under `$XDG_STATE_HOME/vtium/`, vtium-config keeps the SHA-256 of the installed file the working copy was last made from or applied as, its base.
- **The draft may be unfinished** (the maintainer, 2026-11-04). vtium-config writes every change to the draft whatever the check says, so New can start empty and a configuration can be built in any order. The view and `vtium config flow` run `vtium check-config` on the draft and list what fails, the complete path of D3 among it. Only Save and `apply` need the check to pass.
- **Which state is shown.** The flow view's status line and the first line of `vtium config flow` always say how the draft stands against the installed file: `draft same as installed`, `draft: 3 lines added, 1 removed`, or `draft made before the installed file changed` when the installed file's hash is no longer the base, and how many problems the check finds. `vtium config diff` prints the difference between the draft and the installed file.
- **`apply` installs the working copy.** `vtium config apply`, and Save in the TUI, check the working copy with `vtium check-config FILE`, one run for both parts, refuse it when the installed file is no longer the base, and install it through `sudo vtium-config apply`, as ADR-1201 D5 has it: owned by root with mode 0644, in one rename, keeping the old file as `vtium.conf.bak`. vtium-config builds the draft `sudo vtium-config apply` takes from the working copy and its base, so the privileged step stays the one that `crates/vtium-config/src/apply.rs` already has. It writes the frits first and vtium.conf after them, so vtium.conf never names a frit that is not there. After a successful apply the base is the new installed file, and the working copy and the installed file are the same.
- **Root works on a draft too** (the maintainer, 2026-11-04). A user who runs vtium-config as root keeps a draft under root's own XDG config path, `/root/.config/vtium/vtium.conf` by default, and installs it through `apply` like anyone else, without sudo. ADR-1201 D5 says vtium-config writes nothing into the invoking user's home as root; that rule keeps root-owned files out of another user's home, which `sudo` can leave in `$HOME`. This record reconciles the two this way: run as root, vtium-config finds root's home from the password database, never from `$HOME` or `$XDG_CONFIG_HOME` inherited through `sudo`, so a root draft lands only in root's own home, which is root's to write.
- **Deleting a draft.** `vtium config draft rm`, and Delete draft in the view after a confirmation, remove the draft and its base. The next edit starts a new draft from the installed file (the maintainer, 2026-11-04).
- **Editing by hand stays possible.** Root can still edit `/etc/vtium/vtium.conf` with any editor. vtium then reads it as D3 says. A working copy made before that edit shows that the installed file changed since, and Save and `apply` refuse it until the user reverts or merges by hand.

#### The writers

- **One configuration file.** vtium-config writes vtium.conf and the frits, and, under ADR-1304 once it is built, vtlooks.
- **One reading of the file for every writer.** vtium-settings reads vtium.conf into its top level and its sections, and each writer works from that reading, so they agree where the top level ends. The line editor of ADR-1201 D5 changes only the top level: a key is replaced on the top-level line vtium takes it from, never on a section's line of the same name; a commented line such as `#font-size=0` counts only at the top level; a new key goes after the last top-level line, never at the end of the file; and `--unset` comments out top-level lines alone. The section writer, which sets a head's profile today (ADR-1201 D6), keeps its rules for every section it edits: it edits only a file vtium reads whole, changes only the lines it needs, and parses its result before handing it back.
- **What each part of the TUI writes,** all into the working copy. The Settings view writes top-level keys. The Heads view sets a head's profile. A profile's keys go in its `[profile NAME]` section, through the section writer. The Flow view writes rules, heads and profiles through the section writer. In every view a change goes into the working copy as it is made, and Save, Revert and New act as in the flow view (D7).

#### `vtium config edit`, the way visudo edits sudoers

visudo copies `/etc/sudoers` to a temporary file, opens it in the user's editor, checks the copy's syntax when the editor exits, and installs it only when it parses, asking whether to edit again otherwise; `crontab -e` and `systemctl edit` work the same way. `vtium config edit` brings that to vtium.conf, for a user who prefers vim or any other editor to the TUI:

1. It copies the draft to a temporary file under `$XDG_STATE_HOME/vtium/`, and opens it in the editor `$VISUAL` names, else `$EDITOR`, else `vi`.
2. When the editor exits with the copy unchanged, it says so and changes nothing.
3. Otherwise the copy becomes the draft, and vtium-config runs `vtium check-config` on it. When the check fails, with an error or a warning, it prints each problem with its line and asks whether to edit again. Notes, which do not fail the check, are printed and do not ask.
4. With `--apply`, it then applies the draft as `apply` does, which installs it only when the check passes. Unlike visudo, it offers no way to install a draft that fails.

Removing a whole head or a profile is done here, and has no command of its own (the maintainer, 2026-11-04).

#### Commands for scripts

Everything the TUI does works without it, so it can be scripted (the maintainer, 2026-11-04). Each change the flow view makes is also a `vtium config` command, and the view and the commands call one set of operations in vtium-config, each of which takes vtium.conf's text and gives back the edited text through the writers above. Every command that changes the configuration reads the draft and the devices present afresh, makes its change, writes the draft, and prints what `vtium check-config` finds in the result. With `--apply` it then applies the draft, as `vtium config apply` does, which installs it only when the check passes. A command asks nothing but sudo's password, and that only when it applies. A rule's `N` is its number in `[rules]`, counted from 1, as `vtium config flow` prints it.

| Command | What it does |
|---|---|
| `vtium config flow [--json]` | Prints the flow of the working copy with the devices present placed on it, as the view draws it, under a first line that says which state it shows (above): as text, or as JSON whose top level carries `"version": 1` and the state, so a script can rely on its fields |
| `vtium config diff` | Prints the difference between the working copy and the installed file |
| `vtium config apply` | Installs the working copy, as above |
| `vtium config set KEY VALUE`, or `set --unset KEY` | Sets a top-level key, the base card's (D7), or leaves it at its default, through the line editor |
| `vtium config draft rm` | Removes the draft; the next edit starts a new one from the installed file |
| `vtium config revert` | Throws away the working copy's changes and copies the installed file back over it; the installed file is untouched |
| `vtium config new` | Empties the working copy to the top level at its defaults and one look, `[profile default]`, as New does (D7) |
| `vtium config rule add [--before N] 'KIND SELECTOR -> TARGET'` | Adds a rule, such as `'input path:pci-0000:15:00.0-usb-0:3.3 -> desk'`, before rule N, or where the view's `new rule…` puts it (D7) |
| `vtium config rule set N 'KIND SELECTOR -> TARGET'`, or `rule set N [--selector S] [--target T]` | Replaces rule N, or changes its selector or its target |
| `vtium config rule move N --before M`, or `rule move N --last` | Moves rule N, since a rule's place decides what it matches first |
| `vtium config rule rm N` | Removes rule N, the only way, with the view's remove, that a rule leaves the file |
| `vtium config head add NAME [--profile P]` | Adds a `[head NAME]` section, with the profile P, or the default profile when `--profile` is left out |
| `vtium config head set NAME [--name NEW] [--profile P] [--consoles N] [--login CMD] [--when-display-lost V] [--isolated yes\|no]` | Changes a head's keys. `--name` renames the head and every rule that names it, in one change |
| `vtium config head active NAME` | Asks the vtium running on this VT to make NAME the active head, as the roaming chord does (ADR-1401 D5). It changes no file; it needs the control socket under `/run/vtium/<tty>/` that ADR-1401 D12 plans, and comes with it. No head key sets the head that is active at start (the maintainer, 2026-11-04) |
| `vtium config profile add NAME [--frit F] [--size PT]` | Adds a `[profile NAME]` section with its `font-name` and `font-size`; a key left out is inherited from the top level (D5) |
| `vtium config profile set NAME [--frit F] [--size PT] [--engine E] [--hinting H] [--line-spacing PCT]` | Changes a profile's keys: `--frit` sets its `font-name`, `--size` its `font-size`, and so on through the keys D5 lists |
| `vtium config edit [--apply]` | Edits the whole working copy in the user's editor, as above |

Each command that changes the configuration also takes `--dry-run`, which prints the change as a diff and the check's verdict and writes nothing; for `revert` and `new` it prints what would be lost. A command asks no confirmation: running it is the confirmation, and `apply`, `revert` and `new` act at once. The exit codes are fixed, so a script can tell the outcomes apart:

| Exit code | Meaning |
|---|---|
| 0 | Done: the draft is written, whatever the check finds, which the command prints; or the flow or the difference is printed; or the draft is installed. `vtium config flow` exits 0 whenever it prints, whatever problems it lists; `vtium check-config` is the command whose status judges a file |
| 1 | The check failed where it must pass: `apply`, or a command's `--apply`, found an error or a warning, each printed with its line, and nothing is installed; the draft keeps the change |
| 2 | The command could not be used: an unknown option, a rule number or a head or profile that does not exist, or a value its key cannot take |
| 3 | The installed vtium.conf changed since the working copy was made from it, so nothing is installed; the working copy keeps the change, and `diff` and `revert` show the way on |
| 4 | Nothing could be installed without privilege: sudo was refused or is missing. The working copy keeps the change, and the command prints how to apply it |

> Edited 2026-11-04: the maintainer added the edit command, the commands for scripts and the working copy, and accepted the rest (`considered`). D8 was headed "what vtium-config writes (open)" and had none of them; until the working copy, each change went into a new draft under `$XDG_STATE_HOME/vtium/drafts/`, and each command and `vtium config edit` installed its change at once. The earlier text is in git at e5308782.

### D9: what the code changes to fit

These follow from D1 to D8 and the tree:

- vtium.conf's reader stops at the first section heading and hands the rest to vtium-settings's head-file reading, through one function that splits the file, so the console, `vtium check-config` and vtium-config agree where the top level ends.
- The trust check vtium-settings makes of heads.conf applies to vtium.conf, with the reasons and the mending commands of D3.
- heads.conf's reading goes from the console's start and from `vtium check-config`, and `apply` no longer takes heads.conf in a draft.
- The seat file's code goes: the seat's path in `files`, the seat's name check and its report in `crates/vtium/src/config/mod.rs`, the seat's argument to `load` in `main.rs`, and `SEAT` in `crates/vtium-config/src/switch.rs`.
- The mapping from a listed device to the identity the rules match moves out of the console binary into a crate that vtium and vtium-config both link, so the flow view and `vtium check-config` place each device by one function. The work that builds the view chooses the crate.
- The TUI gains color, through the sixteen-color SGR codes alone, and reads `NO_COLOR`. Its input decoder binds nothing to a lone ESC today, since a lone ESC cannot be told from the start of a sequence without waiting; for the flow view's Esc it reads an ESC as the key when it is the last byte of a read, since a terminal writes each sequence it sends whole, and no timer is involved. It decodes no mouse input today, so its input decoder (`crates/vtium-config/src/tui/input.rs`) gains SGR mouse reports for the flow view.
- vtium-config's TUI watches device events through vtium-platform's udev monitor (`crates/vtium-platform/src/udev.rs`, `Udev`), the console's own, on the event loop it already runs.
- vtium-config gains the commands of D8 and `flow`, over one set of operations that the view calls too, and a JSON form of the flow with a version field.
- vtium-config keeps the working copy and its base's hash under the user's XDG directories, and `apply` builds the draft that `sudo vtium-config apply` takes (`crates/vtium-config/src/apply.rs`) from the working copy, so the privileged step is unchanged. The TUI's Save view shows the difference between the working copy and the installed file and applies it, in place of a new draft for each save.
- New rules are placed by what they name, with one function the view, connecting and `rule add` share.
- vtium-config's TUI gains the read-only pager of D7, the base card over the key table, and Delete draft; the draft's path comes from the XDG variables for a user and from the password database for root.
- The complete-path rule of D3 is one function in vtium-settings, which takes the rules and either the devices present or none, so `vtium check-config`, its `--no-devices` reading and vtium-config judge a file alike; the console's start does not call it.
- The example vtium.conf is generated from the key table and the sections template together, with the size test of D3.

## The maintainer's answers

The maintainer answered the draft's open questions on 2026-11-04 (`considered`):

1. **The syntax (D4):** sections with names, with a `[profile NAME]` section for each look and one `[rules]` list after the heads, tried top to bottom across every head.
2. **Two levels (D5):** as drafted: a head's section over its profile over the top level, with a fixed list of keys for each section kind.
3. **Migration (D6):** none; the two installs that have heads.conf or a seat file are converted by hand.
4. **The view (D7):** as drafted, then navigable by keys and mouse (Tab and Left and Right across the columns, Up and Down within one, Enter to edit, Space to tag rules, devices, heads and looks, `c` to connect), with View, Save, Revert, New and Delete draft, each confirmed but View, a base card for the top-level keys, a minimal status line, editing through the checked save from the first build, with objects drawn as blocks of color, hotplug followed live, with `d` for a full re-read, a device that is configured but not found kept on the flow until its rule is deleted, `new…` items at the foot of the rules, heads and look columns, so the whole configuration can be built in the view, and tagging with Space and connecting the tags with `c`.
5. **Writes (D8):** as drafted, with a valid file needing one complete path (D3), with `vtium config edit` added, a command for every change the view makes, `head add` and `profile add` among them, with fixed exit codes, no command to remove a whole head or profile, `head active` as a request to the running vtium, every edit going into the user's draft at `~/.config/vtium/vtium.conf`, root's under root's own XDG path, unfinished if need be, with only `apply` needing a passing check, `set` for top-level keys and `draft rm`, and vtium refusing an untrusted vtium.conf with its reasons (D3).

## Passages this record changes

Once the maintainer accepts this record, each passage below is edited in place with a dated note that cites ADR-1403:

- ADR-1401 D3: the head file is the sections of `/etc/vtium/vtium.conf`, and its top-level keys sit among vtium's keys. D2: a later seat's flow goes into vtium.conf, not a file of its own. D6: the template is the sections of the example vtium.conf. D7: the trust check covers vtium.conf and says why it refuses one, and "the whole file" means the sections and the policy. D12: `vtiumctl template` and `install` write vtium.conf.
- ADR-1200 D16: a profile is a `[profile NAME]` section of vtium.conf.
- ADR-1201 D2: the commands of D8 and `flow`. D3: the Flow tab, and Save, Revert and New in every view. D5: the working copy in place of a draft for each save, `apply` building its draft from the working copy, the line editor's top level, and the check every change goes through. D6: the section writer edits vtium.conf.
- ADR-1500 D4: the rows for the seat file and for heads.conf, and the shared-head-file probe and answer, which now concern vtium.conf.
- ADR-1304 D4 and D5, still proposed: a vtlook uses vtium.conf's section syntax, and a head's `look` is set in its section of vtium.conf; its author changes them before acceptance.
- docs/config.md, docs/configure.md, docs/head-file.md, docs/heads.md and the files under docs/heads/, the READMEs of the vtium, vtium-config and vtium-settings crates, and the configuration example and template.

## C notes: the C reference's flow

- `kmscon_conf_load_main` parsed the command line, then `<configdir>/kmscon.conf`, then set the log level again so that the file's `debug` and `verbose` took effect (`kmscon/src/config.c:1500-1552`).
- `kmscon_conf_load_seat` copied the main configuration and parsed `<configdir>/<seat>.seat.conf` over it, with the flag `seat_config` set (`kmscon/src/config.c:1554-1575`). The seat called it once as it was made (`kmscon/src/seat.c:786`).
- `parse_line` read one key a line: up to a `=`, a `#` or the end of the line, then a value up to a `#` or the end, where a `#` that began a `#rrggbb` value was part of it. A line with no `=` was a switch. It knew no sections (`kmscon/src/misc/conf.c:402-500`).
- The C reference read its files with no check of their owner or mode.

## Where vtium departs from the C reference

- **No seat file.** The C reference read `<seat>.seat.conf` over kmscon.conf for each seat. vtium reads vtium.conf and its command line alone, and its configuration for every seat it will serve lives in vtium.conf (D2).
- **Sections below the keys.** The C reference's file held keys alone. vtium.conf holds the flow's sections below them, and a kmscon.conf with no section reads as it did (D3, D4).
- **Root's file.** The C reference read its file whoever owned it. vtium refuses a vtium.conf that root does not own or that others can write, and says why (D3).
- **A `#` glued to a value.** The C reference ended a value at any `#` that did not open a `#rrggbb` value. vtium keeps a `#` with no blank before it in the value (D4).

## Consequences

### Positive

- One file shows how each display and each keyboard reaches a terminal and what that terminal draws with, and the flow view draws it with the machine's own devices on it.
- A head's inheritance is written where it happens: the keys a head inherits sit above its section in the same file.
- vtium.conf gains the trust check heads.conf had, which also closes the path by which a vtium.conf that root does not own could name the command vtium starts as root.
- The flow view edits from its first build and redraws each change at once with the devices present placed on it, so a user sees where each device goes before saving.
- Every change has a command with fixed exit codes, and the flow prints as JSON, so a script can do what the view does.
- No file reaches `/etc/vtium/vtium.conf` through vtium-config or passes `vtium check-config` unless someone at the machine can type into a head with a display, while the shipped default and a machine with no devices present are never refused for it.
- Unplugging a device never costs its rule: the view keeps the device on the flow as configured but not found.
- Tagging a device, a head and a look and pressing `c` makes the rule and profile changes they describe, shown as commands before they are applied.
- The view follows hotplug while it is open, so a user plugs a keyboard in and sees at once which rule takes it, and keeps their place on screen while devices come and go.
- Save, Revert, New and Delete draft each ask first and count what changes, and only Save touches the installed file. A configuration can be built in any order, since the draft may be unfinished, and View shows it as text with each problem marked.
- `vtium config edit` lets a user edit the whole configuration in their own editor with the same check the TUI's changes get.
- One working copy holds every change the user makes, whichever tool made it, and the view and `vtium config flow` say how it stands against the installed file, so the user always knows which configuration they are looking at and what `apply` will install.
- The seat file's second layer of the same keys goes, so a key has one place at each level.

### Negative

- A user who is not root has two files to keep in mind, the working copy and the installed file, and a change made directly to the installed file, as by root or a package, makes `apply` refuse the working copy until it is reset or merged by hand.
- `vtium check-config` refuses a file the console would still run, since the console does not apply the complete-path rule at start. A file written for another machine has to be checked with `--no-devices` where its devices are absent.
- One file mixes a forgiving reading at the top level with a strict one in the sections, and a reader has to know which part a line is in. The flow view and `vtium check-config` say which.
- A user who runs vtium with a vtium.conf root does not own loses that file's settings until it is root's.
- A heads.conf or seat file left in place stops working without a word. The maintainer converts the two installs that have one by hand.
- While two builds share `/etc/vtium/vtium.conf`, a build older than this change misreads a section's `login` and a profile's font lines.
- The view watches device events while it is open, so vtium-config gains an event source of its own, and what it shows can move while the user reads it; the selection and the opened component stay put, and the status line names each change.
- The flow view brings color into vtium-config, which needs a rule for terminals without color and a list for narrow ones.

### Neutral

- Frits, vtlooks, shaders and fonts stay files of their own, which the flow names and the view draws.
- `vtiumctl`, not built yet, writes vtium.conf where ADR-1401 D12 has it write heads.conf.
- The flow view edits rules, heads and profiles, which overlaps what ADR-1401 D12 plans for `vtiumctl setup`; both write through the same section writer and the same check, and a change applies at vtium's next reading of its configuration until the trial of ADR-1401 D12 is built.

## Alternatives Considered

- **Blocks in braces, as sway, i3, Hyprland and nginx write them.** sway is the closest program to vtium, since it matches displays and input devices by a selector at the head of each block, as in `output eDP-1 { mode 1920x1080 }` and `input type:keyboard { xkb_layout us }`. vtium's form would write `head desk { profile big { font-name = monoid } }` and a `rules { ... }` block. It writes the nesting most literally, and it costs a new parser, a missing brace that swallows every line after it so that ADR-1401 D7 has no single line to drop, and a second way to write a profile two heads share. sway also applies every block whose selector matches a device and merges their settings, where vtium places a device by the first rule that matches, so the rules would stay one ordered block in any case. The maintainer chose sections with names (`considered`).
- **Dotted flat keys,** as sysctl writes them, such as `head.desk.profile=big` and `rule=display * -> desk`. Every line stays a flat key, which an older vtium ignores as unknown, but the structure is invisible: a head is spread over lines that only share a prefix, and `rule` would keep every value it is given, where every other key keeps its last.
- **TOML or YAML for the whole file.** TOML nests with existing parsers, but a text value must be quoted, as `xkb-layout = "us"`, and every key needs a value, so a switch alone on its line, such as `hwaccel` or `mouse`, is invalid, and a kmscon.conf would not parse. YAML fails the same way and makes indentation carry meaning. vtium's parsers are also written to bound untrusted input (ADR-1401 D10).
- **A migration:** vtium reading heads.conf with a warning until the first beta, and `vtium config fold` moving heads.conf and the seat file's keys into vtium.conf. The only two installs that need it are the maintainer's, converted by hand, so the maintainer chose no migration (D6).
- **Keep heads.conf beside vtium.conf, and draw the flow from both files.** The view could join two files, and nothing would move. The maintainer decided on one file, and two files keep the inheritance split from what it inherits.
- **Keep the seat file.** It is a second layer of the same keys, which no decision chose, and vtium serves seat0 alone. The maintainer removed it.
- **A new file for the flow, such as `flow.conf`, with vtium.conf left as it is.** It keeps kmscon.conf untouched, and it keeps the split the maintainer decided to end.
