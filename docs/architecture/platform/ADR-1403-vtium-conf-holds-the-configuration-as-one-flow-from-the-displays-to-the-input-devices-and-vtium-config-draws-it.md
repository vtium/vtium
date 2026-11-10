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
  - upstream: "the C reference's kmscon at b1f7a7c5: kmscon_conf_load_main parsed the command line and then `<configdir>/kmscon.conf` (src/config.c:1500-1552); kmscon_conf_load_seat copied the main configuration and parsed `<configdir>/<seat>.seat.conf` over it (src/config.c:1554-1575), once for each seat as the seat was made (src/seat.c:786); parse_line read one `key=value` or switch a line, ended the line's text at a `#` anywhere but where it began a `#rrggbb` value, and knew no sections (src/misc/conf.c:402-500)"
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
  - operator: aaronsb
    said: "Most configuration work happens in the Flow view, and vtium-config opens on Flow. vtium-config keeps three views: Flow, Fonts and Settings. The Heads view goes, since the flow is the heads. The Sizes view goes: size moves into the look card, where Enter on a look offers its sizes, with live preview where it works, keeping the quick try-and-keep loop the Sizes view had. Settings stays as a view, the full list of top-level keys, and the base card in Flow edits the common few. Fonts becomes a font resource manager: the places vtium finds fonts, listed with what each holds; the fonts installed there, with their coverage read only in the sandbox; the frits as files, with new (the guided builder), rename and delete, and a note on which looks use each; and live preview of a font."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "vtium-config manages looks too: a Looks view manages each look, its filters, color and effects included, and looks can then be picked from the look column in the Flow view."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "Allow delete: the TUI carries the full lifecycle, since that is likely how people edit the configuration; otherwise they edit the file directly and check it."
    via: the steering session's interview, 2026-11-04
    paraphrase: true
  - operator: aaronsb
    said: "keep them, broken - highlight red or something or at least error symbology somehow"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "Heads of a deleted look move to the default look, named in the view's confirmation, and on the command line that takes --force. The [profile NAME] section is renamed [look NAME]; it holds the frit, the size and vtlook = FILE, and heads name a look."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "we could keep a general vtium-config 'config' in the user's xdg path. this would allow the user to retain general config elements (essentially, the individual nodes making up the flow) and also is a store to keep their drafts in"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "in the web based previewer all the layout gets broken at 40 columns. let's experiment with 100x30 to start and see how it looks."
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "someone could ssh to a very low spec machine and bring their ssh terminal to a large size and use it correctly"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "Add exit code 5, couldn't read or write a file, with the message naming the path. The golden commit for the ten goldens that show heads.conf is a commit of its own, reviewed in the maintainer's golden pass."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "or just copy the file in - remember it creates a compatible draft"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "A library node is copied into a draft; a card marks a copy that differs from its library node, and the user can push or pull the change. The store's format, deleting a look section while a vtlook file stays protected, and the Looks view replacing ADR-1304 D11's view are accepted as leaned."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "no you name drafts, by default it offers a hostname and an increment to avoid overwriting"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "Connect: c on a complete tagged path shows the resulting commands, then writes them into the draft at once and clears the tags; there is no stack of pending paths. The top row counts the draft against the installed file, as draft NAME, 3 changes, 1 problem. The view and the docs use five words, each meaning one thing: tag, Space marks an item; path, a complete chain of tagged items; connect, c turns a path into changes in the draft; draft, the working copy, saved on every change; install, the one action that writes /etc/vtium/vtium.conf, through sudo. Save becomes Install and vtium config apply becomes vtium config install; configuration means only the installed file and the draft together."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "Keep an installed history: each install also saves the file it replaces, numbered, in vtium-config's store. A new action, Restore, lists that history and copies the chosen file into the draft, where the user looks at it and installs it the usual way; nothing but install writes /etc. Revert keeps its meaning, putting the draft back to the installed file. Restore is a sixth fixed word."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "? on any selected item opens a summary card, computed from the draft and the devices present, that says what the item does, which the maintainer calls you are here. The card lists the whole route from top to bottom, display, display rule, head, look, input rule, input device, with the selected item highlighted; an item that branches lists each route; an item nothing references says so in plain words, such as not used: no rule sends anything here; a broken item says what is broken. It is named the route card, and the six fixed words stay unchanged."
    via: the steering session's interview, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "It actually looks like 80x25 will work"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "History shows last 10"
    via: the steering session's interview, 2026-11-05
  - operator: aaronsb
    said: "A vtium.conf with no sections and policy = closed leaves vtium dark: it opens nothing. vtium check-config refuses the file, naming the missing rules, and vtium-config does not install it, because it has no complete route."
    via: the steering session, relaying the maintainer's answers of 2026-11-05 to the passages work's questions
    paraphrase: true
  - operator: aaronsb
    said: "A [look NAME] section takes cell = as well as size =. The existing rule holds: at one level, font-size wins over a cell."
    via: the steering session, relaying the maintainer's answers of 2026-11-05 to the passages work's questions
    paraphrase: true
  - operator: advisor
    said: "The section writer finds what it edits by its lines and never refuses an edit because the file or the result fails the check. It refuses only an edit it cannot place: a missing section or rule, a section named twice, or text it cannot write as one line. The check guards install and check-config."
    via: the steering session's call of 2026-11-05 while building slice C, made under D8's ruling that a draft may be unfinished
    paraphrase: true
  - operator: aaronsb
    said: "At 100 columns and wider, each line between two columns gets its own column in the gap, so a dashed line stays dashed and no line passes through another line's junction; at 80 columns the lines keep sharing one column. A line whose target is out of sight runs to the edge of the visible columns and ends in an arrow pointing toward the target, beside the count of what is out of sight, in every column."
    via: the steering session, relaying the maintainer's review of the TUI's screen goldens, 2026-11-05
    paraphrase: true
  - operator: aaronsb
    said: "[Should a draft keep its font set changes?] Draft keeps them (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
  - operator: aaronsb
    said: "[When you start a draft over with New, should that folder be cleared too, as Revert clears it?] Yes, New clears it (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
  - operator: aaronsb
    said: "[Removing a font set that a look in the draft still uses: allow it?] Write it, check flags (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
  - operator: aaronsb
    said: "[Is .bak and diff enough, or should vtium-config detect that the installed file changed since the draft started?] .bak and diff are enough (Rec.)"
    via: AskUserQuestion, 2026-11-06; the option label is the steering session's wording
  - operator: aaronsb
    said: "tab navigates forward (shift+tab could navigate backwards) the top menu items and arrow keys navigate around inside. right now tab and arrrow are mixed and on the flow tab it's especially confusing."
    via: the steering session, 2026-11-07, after the maintainer tried vtium-config from the alpha.6 build
  - operator: aaronsb
    said: "[Adding a finished path: today c writes whatever you've marked into the draft at once, with no question, as chosen on 5 November. Ask when the marked items form a complete path, or keep c writing at once?] I thin kit's ask when it's complete, and its possible the 5 november choice wasn't clearly asked at the time"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[What counts as placing a device: any rule that reaches a head, or only rules that name it?] Any rule that reaches a head"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[The inputs that can never be part of a path, such as the power button, a receiver's System Control and Consumer Control nodes and audio jacks: fold them into one block, hide them, or list them all?] Fold them into one block"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[Keeping a working configuration: always keep a working path, refusing a change that removes the last one, or allow a draft with no working path while the person builds it?] allow while building, with a status line that clearly states the path isn't complete"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[This display: the words this screen on the head that VTIUM_HEAD names and on its displays, the selection starting and staying there; over ssh no mark and no message; the Live trial is off notice only on t] As proposed"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[The tab bar's actions: each action shown with its key, the key line showing only the keys that fit what the person is doing now, Delete draft folded into Revert, vtium config draft rm kept] this sounds like contextual controls (by tab) and I think it makes more sense to declutter"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[After a path is added: say it in the top row, complete · i installs, or ask Install now?] asking to install per path is strange. the user can add paths and edit, and when done, save. if the operator/user elects to install, the config goes through the save path first then installs. either way the configuration file is validated. "
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[What should Install do with a draft that has no working path, for example when every display rule points at a head with no keyboard: refuse and say what's missing, or ask, then allow it?] Ask, then allow it"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[Leaving with unsaved changes: q asks whether to save first. Ctrl-C, closing the terminal, logging out or the system stopping the tool all save your changes into the draft without asking, so nothing is lost. Only a crash loses them. OK?] As proposed"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[Installing a draft with no working path from the command line: at a terminal it asks the same question; in a script, with no terminal, it refuses unless given a new flag, --no-complete-path] ask at a terminal no flag, with force flag (that is common, if we have one) to avoid the interactive approach ( you just have to know what you're doing because it will accept the config)"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[When a marked path is complete: the marks form a complete path when they include one head, at least one display and at least one keyboard; a look and the rules are optional; pressing n or Esc at Add this path? keeps your marks. As proposed, or also require a look?] probably offer to preview (using our timeout delay if things go wrong)"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[1. A display sent to off, or an input sent to ignore, by a rule naming that one device counts as placed, and the top row says complete once everything connected is placed. 2. The inputs that can be part of a path are the keyboards and the pointer classes, tablet-pad included, and a device a rule names on its own stays out of the folded block. 3. The TUI keeps one set of unsaved changes for every view. 4. Save writes into the draft whatever the check finds, and asks nothing. 5. Save and Install show in Looks, Fonts and Settings too, and Help shows no actions. 6. save becomes a seventh fixed word.] Accept all"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "[The preview: a preview always goes back to the old configuration when it ends. Pressing k during the 10-second countdown means I like it: the path is added to your unsaved changes, and you Save or Install it later as usual. Any other key, or the countdown running out, goes back and keeps your marks. Starting a preview needs sudo, like a trial. The console needs a second kind of trial that always goes back. OK?] as proposed. this is like kde display settings if you don't keep them they revert"
    via: the steering session's question to the maintainer on 2026-11-09, after the gap-check of the Flow view against the maintainer's circuit model
  - operator: aaronsb
    said: "ok so I started a new config 'n' and I have a bunch of yellow blocks along the bottom. … my next expectation is that I'm going to pick another device. as the user, I could select another input device (say, the first one I picked was a keyboard, and the next device was a number pad, or a mouse or something), then the next device (the third pick) is a display. since keyboard to display is a full path, that should present as a fully valid path. I could, possibly, add a second display to the path too (this could be eventually the way we add multi-display composing, but in this case my expectation is that the display output would be mirrored on both). finally, there's a terminal configuration implictiness in this too. we should have the .vtfrit config files ( fonts, styles, sizes, etc) that is one of these composable objects. they can be optional, because if the user just picks the start and end of the path and nothing else, then they're basically defaulting to the default configuration. … the idea is that one is creating one valid config path at a time, and after a complete path ( minimal or otherwise ) is added, it gets added to the scrollable 'document' showing all the config. the user can then just add another path. if they pick the same elements as the first path, they're basically duplicating it and it would get deduplicated if it was exactly the same. … this way the operator is picking pieces, composing a flow, adding it to the 'document' (the config file) until they are satisfied. any of the flows in the document can be previewed, and any of the vtfrits can be 'entered' into in the path and see the font preview pages."
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "[Paths on different VTs] Same devices always conflict"
    via: the steering session's follow-up questions on the paths try-out, 2026-11-09; the bracketed question is the steering session's wording
  - operator: aaronsb
    said: "[One keyboard in two paths] Yes, and it roams"
    via: the steering session's follow-up questions on the paths try-out, 2026-11-09; the bracketed question is the steering session's wording
  - operator: aaronsb
    said: "[\"Any\" pieces] offer 'any keyboard' pieces. should we offer 'any display'? could that be changable into a filtered block too? (any, filtered, down to a specific device). … once it's added to the document, we can't alter it because it makes it too complicated to compute conflicts - in that case, just take the path out of the document ( which removes it) and returns it to the proposed path again for editing."
    via: the steering session's follow-up questions on the paths try-out, 2026-11-09; the bracketed question is the steering session's wording
  - operator: aaronsb
    said: "the general path, going backwards from the config file to be saved: the document view just renders/displays the configuration. … one of the things the user can do is revise it for editing, (some sort of wording like that) which actually removes it from the doc. this pops it from the config doc, and pushes into the editor. in this place, the user can connect and disconnect objects from the components at the bottom. each piece is fully editable or selectable here. once they're satisfied (including the name of it if they'd like) they can add it back to the doc/config wher eit's validated again. if it's invalid, it doesn't get added and remains in the editor with some guidance as to what was in conflict."
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "I guess maybe the document itself is minimally editable - the name of a flow for instance, or altering some aspects of it. I don't want it to be too flexible - sometimes having some rules helps keep structure. but also, too much churn (having to take it out of the doc to edit it) is also bad. I think the main reason to restrict changes in the config doc has more to do with resource allocation (displays, input devices, available font resources) than anything else. so altering the default character size in points, or the name of the flow, or the font used (only pickable from defined fonts) when pressing 'enter' (entering into a flow in the doc) maintains the stability of resource conflicts."
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "and obviously, if it's a net new path never added to the doc, the user has a chance to name it and set those same properties in the pre-document editor before sending it to the config doc"
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "I like this because it's a three stage process: parts, from the system -> composed into a flow, configurable -> added to the config, to be integrated with other flows"
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "right - the difference is deliberate becuase the number of ways in which it can be wired up is pretty bounded."
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "if you wanted to add to d7 that it's because we have the rendering path, the glyph generation path, even the display effects path (color grading, or with the gpu, effects) and each of those is defined in one of our objects"
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "due to this known path bound, it's easy for a user to pick the most minimum of config: any keyboard, any display, no font selection, all VT slots. this basically matches the default VT experience"
    via: chat, 2026-11-09
  - operator: aaronsb
    said: "I am very pleased with our rule builder. it feels like the 'rules view' (v) is a bit unnecessary. we could almost use some of the categorical colors from rules view in the flow view. we could drop the rules view and use the same key (v) to view the configuration file. (I don't see an obvious way to just view the file as it is configured)"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "[Does a keyboard in two paths roam only between those two terminals, which changes vtium.conf, or does roaming mean every terminal?] Roaming means every terminal"
    via: AskUserQuestion, 2026-11-10; the option label is the steering session's wording
  - operator: aaronsb
    said: "in the viewing config file, we should allow to install/save as well."
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "looks good, but I was able to edit a roaming relationship between a terminal and a device (logitech usb reciever by id) but once in edit mode was unable to add it back until I also added a display. so there's a fault in the logic to round trip that sort of configuration"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "I think the right fit is that it's 'any display' then the terminal style and usb reciever which should fit the path I think?"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "[The steering session's reading: roaming is a path whose terminal part is the active terminal, with no display, which writes exactly its input … -> roaming rules] ok that makes sense"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "for a terminal with displays and no inputs like a kiosk screen, can we use a null definition? this could be used for displays and inputs, and allow flexibility while evaluating the rules consistently"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "[An exception to a broad part: a device placed in a path of its own gets a rule above the broad rule, or vtium.conf gains a way to say not] the exception goes elsewhere"
    via: AskUserQuestion, 2026-11-10; the option label is the steering session's wording
  - operator: aaronsb
    said: "I think what we need to do is refine the parts picker system. … I think instead, we should pick a path where we have a display part, terminal part, input part, and font set part. on opening a flow, we enumerate all the candidate devices and create parts based on whats found. if the state changes, we rerun this same process, diff against the objects found, and only create new objects. if a device is removed, we don't delete the part but ghost the part to indicate it's not connected any more, but still recognized. we need to rename the 'any display' or 'any pointer' or 'any keyboard' to just mean display, pointer, keyboard (and maybe font set). you can't actually add the template object (it could always be considered a template part) and if one enters into a part, this is where one creates a new part. the options in the part, such as 'any x', then all ways it can be filtered (DP, HDMI, etc) (by it's connector, by EDID) are now options. picking one of those options in the detail view of the part, brings up a picker listing all the relevant matches. the last item on the list of matches is a fill in the blank match. I think this pattern resolves some of the hard coding we have, and gives the parts purpose."
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "inside the templates. then when the template is instanced, as a device or a special device (no input, every keyboard, etc) the part gets the name displayed."
    via: AskUserQuestion, 2026-11-10; the maintainer's own words, typed as the answer
  - operator: aaronsb
    said: "I think this is working really well."
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "on a small terminal, 80x25, let's make it so each object box is composable on a line. … if we can only fit a few boxes before running out of columns, it's a 'newline' (but an entire box in a newline) (so we just treat the box as the composable element). if I shrink it down very narrow, then we only get one or two boxes and it stretches out. this goes back to the original idea that the upper section, CONFIGURATION can be scrolled up and down on a small screen. The NEW PATH and PARTS then are part of the overall 'document' and each section (CONFIGURATION, NEW PATH, PARTS) are sections of the whole document. I think that solves the 80x25 question."
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "I guess by 'on a small terminal let's make it...' I actualy mean, we always make it reflowable - regardless of size"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "I'd like to be able to add a label to each part, which is just a name attached to the part. This sort of follows the TERMINALS convention where the name of it is cusomizable. These names actually could make it all the way to the OSD to show what path is what. If there is no customizable name then the OSD just displays what was picked up by the system."
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "we should also think of the phrasing \"Enter makes a part\" is a strange way of saying \"New Part [Enter]\". Inside the part maker, \"A choice that starts with by lists the values the connected devices give.\" is hard to parse. It's literally telling me that 'by x' is the way it starts. We could jus drop 'by' and it would likely still make sense. This grammar fixup should happen now while we're still exploring the ui"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "think about real software that exists that sort of offers this ability. usually they're phrased pretty plain/obvious"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "I think your proposed strings already would make readability significantly better"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "honestly, I think the vtium-config tool, the configuration file and what can be configured, is likely as important as the entire terminal system we've built - otherwise it'll be obtuse to control and not really any better than kmscon or other terminal applications."
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "in our configuration, where do we represent post filters (color filters, effects shaders) - I think they're 'inside' the terminal"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "maybe as their own subflow"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "Well, let's imagine we defined three flows, in our demo app: DP-8 is the display, and connected to the '8 bit do keyboard'. the terminals in the middle, are variants - they're not bound to each VT, but VT3, VT4, and VT5 - that's what allows all three flows to exist in CONFIGURATION (so they're unique). So, this means that the terminal itself defines the effects, and the display is just a display output. Our effects had requires directives I believe, so a GPU requirement would loose part of the filter set (probably not prevent it, but might set a warning in the log). This means that the TERMINALS part is where we keep our subflow."
    via: AskUserQuestion, 2026-11-10; the maintainer's own words, typed as the answer to where effects belong
  - operator: aaronsb
    said: "[Does the record name the subflow's place only, or draw the subflow now?] Draw it now"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[How does a terminal say which VTs it runs on? `vt = 3` (or a list, `vt = 3 4`) in a terminal's section. A terminal without it runs on every vtium VT, as today. A display or keyboard may be in one path per VT.] vt key, default every VT (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Do a terminal's own consoles stay beside VT binding? A VT picks the terminal (its look and effects); inside it, vtium's own keys switch between that terminal's consoles.] Keep both (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "this would allow the ability to have multiple screens and multiple keyboards still - the TERMINAL is still bound to a display, and the display dictates what can be rendered (colors (or not), hardware (or not) and this means that when a user navigates with console commands to a different terminal they're already logged into, that either means all displays get that terminal, or if they're all differentiated, the character and personality changes as the user changes focus. if the user had unique keyboards tied to each other, then that keyboard stays with it's attached terminals, as another 'path test' (imagining it in my mind)"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "[A device whose rule sends it to a terminal that does not run on the VT now shown: On a VT where none of its terminals run, the device does nothing: a keyboard types nowhere (rescue key still watched under the open policy), a display stays dark.] Stays with its terminals (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "we are, slowly through description, building the policy state machine basically"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "[Placement: on the VT shown, vtium reads the rules top to bottom for each device. (1) A rule that doesn't match: read on. (2) A rule that matches, whose target runs on this VT: it places the device; stop. (3) A rule that matches, whose terminal runs on another VT: note it, read on. (4) No rule placed it: if step 3 noted a terminal, the device stays with it; otherwise the policy places it. Rules for other VTs are read past; a device claimed by any rule never falls to the policy; the record writes this as one table, and the path tests check each step.] Yes, this procedure (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Rule order: one device (serial), USB port, model, device name, device type, then the catch-alls.] Port before model (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[The base card and the route card: paths show routes; Settings shows top-level keys; a part's detail lists the rules that name it, per VT.] Drop both (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[The words part, template, path, pick, add, revise and label] Accept all seven (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Once a rule has tied a device to a terminal on another VT, can a later broad rule take it on this VT? After a rule ties it to another VT's terminal, only rules naming a terminal of this VT can place it; broad rules and the policy leave it alone.] Stays with its terminal (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: Aaron Bockelie
    said: "I think it's 'stays with it's terminal'. another way I was thinking about this is if we say 'all devices' and then also specifically say a device, the broad rules are applied after specific rules (thinking like firewall policy order)"
    via: chat, 2026-11-10
  - operator: aaronsb
    said: "[What \"no input\" means: No input rule names the head, and vtium-config sets isolated = yes, so roaming keyboards never reach it either. A head with no input rules and isolated = yes reads back as \"no input\".] Nothing types (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Whether \"no terminal\" exists: \"no terminal\" sits in PARTS beside \"the active terminal\". An input path to it writes `-> ignore`; a display path to it writes `-> off`. Every rule becomes a path.] Yes, add it (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Whether \"no display\" exists: Every path with a terminal has at least one display, real or not yet connected. A display sent nowhere uses \"no terminal\" (off).] Don't offer it (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Which templates: Four templates, one per stage. Keyboard and pointer are the first options inside Input's detail.] Display, Input, Font set, Terminal (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Which parts persist: A part goes grey when its device leaves, for as long as the view is open. Devices the draft's rules name always come back, greyed out if not connected.] Seen now, or named in file (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
  - operator: aaronsb
    said: "[Where a label lives: `input MATCH = Left keyboard` / `display MATCH = Big monitor`, one line per labelled part. Rules are unchanged; vtium reads the section for the on-screen display.] A [labels] section (Recommended)"
    via: AskUserQuestion, 2026-11-10; the option's wording is the steering session's
status: accepted
date: 2026-11-04
deciders:
  - aaronsb
related: []
---

# ADR-1403: vtium.conf holds the configuration as one flow from the displays to the input devices, and vtium-config draws it

## Summary

- **Decided:** The maintainer decided on 2026-11-04 that vtium's configuration is one flow, from the displays to the input devices: the displays, the display rules that send each display to a head, the heads, each head's look (its frit and size, the fonts that frit draws with, and the vtlook that holds its filters), the input rules that send each input device to a head, and the input devices. The whole flow lives in one file, `/etc/vtium/vtium.conf`. Its flat `key=value` lines stay, and they are the defaults every head inherits, so a kmscon.conf copied in still reads. Below them come sections with names, as heads.conf writes them: a `[look NAME]` section for each look, a `[head NAME]` section for each head, and one `[rules]` list, which each device tries top to bottom across every head (D4). vtium stops reading the seat file, `seat0.seat.conf`, which is a departure from the C reference, and stops reading heads.conf, with no migration: the two installs that have either file are converted by hand (D6). vtium refuses a vtium.conf that root does not own or that others can write, and says why (D3). vtium-config gains a view, Flow, that shows the configuration as paths, each a row of bordered boxes that joins displays → a terminal ← inputs, with the terminal's font set inside it. The view builds a path in three stages: the person picks parts, made from the machine's own devices and from four templates, into a new path, then adds the path to the configuration, and vtium-config writes its head and rules, most specific first (D7, D8). A part whose device is unplugged stays on screen, greyed out, and nothing leaves the file unless the person takes it out (D7). vtium-config also gains `vtium config edit`, which works as visudo does: it opens the configuration in the user's editor and checks it with `vtium check-config` when the editor exits. Every edit, root's included, goes into the user's draft under the XDG path, `~/.config/vtium/drafts/`, which may be unfinished; only `install` needs a passing check, and installs the draft to `/etc/vtium/vtium.conf` through the privileged step `sudo vtium-config apply DIR`. The view and `vtium config flow` always say how the draft stands, and a draft can be deleted (D8). A draft keeps its font set changes, the frits it adds or replaces and the installed frits it removes, in a folder beside it, `~/.config/vtium/drafts/NAME.frits/`: they survive quitting the TUI, show in `vtium config diff`, travel with the draft to another machine, and install with it, frits before vtium.conf, so a failure part-way leaves a working system (D8). The view shows the draft as text on `v`, and the person can label a part with a name of their own, which vtium.conf keeps in a `[labels]` section. vtium shows a display's label when vtium-config identifies the displays (D4, D7). Every change the view makes is also a `vtium config` command with fixed exit codes, and `vtium config flow --json` prints the flow for scripts, so everything works without the TUI (D8). A valid configuration wires at least one complete route, a display present sent to a head that a keyboard present reaches, and `vtium check-config` refuses a file without one; Install asks before it installs a draft without one, and the shipped default always passes (D3). As soon as a new path is complete, the view asks the person to add it, and Revise takes a path back out to change it and adds it back byte for byte. The view's changes wait until Save writes them into the draft, and Install saves first (D7). This record's agent adds what the move forces: the lines above the first section keep vtium.conf's forgiving reading, and the sections keep the head file's strict one, so an error in a section never drops a top-level key (D3).
- **Trades away:** One file holds two ways of handling a mistake, a forgiving one for the flat keys and a strict one for the sections. A heads.conf or seat file left in place stops working, with no warning. A vtium.conf that root does not own, which vtium reads today, is no longer read at all. While the packaged build and the development build share `/etc/vtium/vtium.conf` (ADR-1500 D4), a build older than this change reads a head section's `login` line and a look section's font lines as if they were top-level keys. The TUI needs a terminal of at least 80 by 25 cells, so a minspec board's 40-column console is configured over ssh, with the commands, or with a draft copied in. Every draft command reads the draft's font set folder as well as its vtium.conf, and a draft carried to another machine is two paths, its file and its folder.
- **One-way?** No, but expensive once a release ships it: users then write their files in the merged form, and a later change of syntax means a migration. Until that release every part of this changes in a session.
- **Probes:**
  - *Not confident (nested objects):* the recommendation keeps heads.conf's bracketed sections as they are, placed below vtium.conf's flat lines, and adds a `[look NAME]` section for each look, so a head names its look as it does today. The rules stay one ordered list in `[rules]`, outside the heads, because first match depends on their order across all heads. Is that the nesting you meant, or did you mean each head's rules written inside that head? (Answered 2026-11-04: that is the nesting meant; see `considered`.)
  - *Confident (no setup lost):* an installed heads.conf keeps working, with a warning at each start, until `vtium config fold` moves it into vtium.conf or the first beta release stops reading it. An installed `seat0.seat.conf` stops being read with this change; vtium says so at each start, and the same command folds its keys into vtium.conf. (Answered 2026-11-04: no migration, the two installs are converted by hand; see `considered` and D6.)
- **Inversion:** Between "a file for each concern", the C reference's kmscon.conf with a seat file beside it, as vtium has today with heads.conf and planned look files, and "one document that holds everything the console reads". vtium takes the second for the flow of devices, heads and looks, and keeps frits, vtlooks and shaders as files of their own, because the glyph helper and the renderer read them and vtium-config manages them one by one.

> Edited 2026-11-04: the maintainer answered the probes and the open questions the same day (`considered`), and the record now states the answers as decisions. The Summary listed five open questions with recommendations, and its first two sentences on the move read: "The head file's sections, today in `/etc/vtium/heads.conf`, move into vtium.conf below those lines. vtium stops reading the seat file, `seat0.seat.conf`, which is a departure from the C reference." The earlier text is in git at 33ea5db0.

> Edited 2026-11-06: the maintainer decided that a draft keeps its font set changes (`considered`), and D8 says how. The Summary gained the sentence on the draft's font set folder, and Trades away its cost.

> Edited 2026-11-10: the maintainer settled the Flow view as paths built from parts (`considered`), and D4, D7 and D8 say how. The Summary read: "vtium-config gains a view that draws the flow as six columns, each in its own color, with displays, heads, looks and input devices as blocks of color joined by lines. It places the machine's real displays and input devices on the file's flow, marks each rule that matches nothing, and moves and edits by keyboard and mouse from its first build, with `new…` items that build rules, heads and looks, every change going through the checked install. A rule that names one device keeps that device on the flow when it is unplugged, marked `configured, not found`, and nothing leaves the file unless the user deletes it (D7).", "The view also edits the top-level keys, through a base card, and opens the draft in a read-only pager (D7)." and "In the view, the person tags items across the columns to build a path, and vtium-config asks to add the path as soon as it is complete."

> Edited 2026-11-09: the maintainer answered the gap-check of the Flow view against the circuit model (`considered`), and D3, D7 and D8 say how. The Summary read: "`vtium check-config` refuses a file without one, so nothing is installed that no one at the machine can type on; the shipped default always passes (D3). In the view, Install, Revert and New each ask before they act, and rules can be tagged with devices and heads (D7)."

## Context

vtium's configuration is spread over three files today. vtium.conf holds the keys the C reference's kmscon.conf had, plus vtium's own. `seat0.seat.conf`, beside it, holds the same keys and wins over vtium.conf, because the C reference read a file for each seat over its main file and vtium carried that over without a decision. heads.conf holds the heads and the rules that place each display and each input device (ADR-1401 D3), and looks were to get files of their own (ADR-1200 D16). A head inherits its font from vtium.conf, its frit from its look, and its devices from heads.conf's rules, so no single file shows how a display and a keyboard end up in a terminal with a font.

The maintainer gives the configuration tool as much weight as the console itself (2026-11-10): "honestly, I think the vtium-config tool, the configuration file and what can be configured, is likely as important as the entire terminal system we've built - otherwise it'll be obtuse to control and not really any better than kmscon or other terminal applications." That is why this record decides vtium.conf's shape and the view that edits it together.

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
| A head's look | what the head draws with: a frit (a font set file) and a size, the fonts that frit draws with, and a vtlook, ADR-1304's file of frame filters | the `[look NAME]` section, the frit's file, the font files, the vtlook's file |
| Input rules | an `input MATCH -> TARGET` line, which sends each input device it matches to a head, to `roaming` or to `ignore` | `[rules]` |
| Input devices | a keyboard, pointer or other input, as vtium finds it | nowhere: the machine supplies them |

- **The rules filter devices, and they are components too.** Each device walks the rules of its own kind, top to bottom, and the first that matches places it, as ADR-1401 D3 decides. The rules stay one ordered list, because which rule matches first depends on their order across every head.
- **The targets that are not heads** are components as well: `roaming`, which types into the active head, `ignore` and `off` (ADR-1401 D3). So is the policy, which places a device no rule matches.
- **A look** is what a head draws with, one `[look NAME]` section of vtium.conf: its frit, its size and the other keys D5 lists, and `vtlook = FILE`, which names the vtlook of ADR-1304 that holds its filters (the maintainer, 2026-11-05). A head names its look with `look = NAME`. ADR-1304's file of frame filters is always called a vtlook here.

> Edited 2026-11-05: the maintainer renamed the profile section `[look NAME]` and put the vtlook's name in it (`considered`). Until then the look was a `[profile NAME]` section, a head named it with `profile =`, and a head named its vtlook apart with `look =`. This record's text says look throughout; its earlier text is in git at 204fbd68.

### D2: the seat file goes

The maintainer decided on 2026-11-04 that vtium stops reading `<seat>.seat.conf`, and that the seat file is removed from the console, vtium-config, the docs and the records. It came across from the C reference's setup for several seats, and no decision chose it.

- vtium reads `vtium.conf` in its configuration directory and then its command line, and nothing else.
- vtium serves seat0 alone (ADR-1401 D2), so the only seat file it read was seat0's. When the later extension for other seats is built, a seat's flow goes into vtium.conf with the rest, and ADR-1401 D2's sentence that the head file gains a file for each seat no longer holds.
- vtium does not look for a seat file, and says nothing of one left in place (D6).

This is a departure from the C reference, which read `<configdir>/<seat>.seat.conf` over kmscon.conf for each seat (C notes).

### D3: one file, vtium.conf

The maintainer decided on 2026-11-04 that vtium.conf holds the whole flow. Its flat `key=value` lines stay as the defaults every head inherits, so a kmscon.conf copied to `/etc/vtium/vtium.conf` still reads, and the head, rule and look sections come below them. heads.conf's content moves into vtium.conf, and vtium no longer reads heads.conf (D6).

- **Trust covers the whole file, and vtium says why it refuses one** (the maintainer, 2026-11-04). vtium reads vtium.conf only when root owns it and neither its group nor others can write it or a directory above it, the check ADR-1401 D7 and D10 make of heads.conf today. vtium refuses a vtium.conf that fails the check: it reads none of it, runs its compiled-in keys and flow, and every head shows `invalid config skipped, using builtin` with its next step, `Run vtium check-config /etc/vtium/vtium.conf to see why.` The log and `vtium check-config` say why: the file or directory that fails, its owner or its mode, and the command that mends it, `chown root:root` or `chmod go-w` with the path. The check covers the top level as well as the sections because vtium.conf now places keyboards, and because its `login` key already names the command vtium starts as root. A kmscon.conf copied in by root, with the usual mode 0644, passes. When vtium runs unprivileged, as the tests and a run by hand do, it also accepts a vtium.conf that its own effective user owns, under the same check of who else can write it; `run-vt.sh` installs the configuration of a review run into a root-owned directory before it starts vtium. This record's agent makes that call, on the steering session's reading of the gap-check: without it no test could hand vtium a file, since the tests cannot make files root's.

The move forces these decisions, which this record's agent makes from ADR-1401 and the tree:

- **The top level and the sections.** The top level is every line above the first section heading. It holds vtium's keys, as vtium.conf does today, and the two keys heads.conf had above its first section, `version` and `policy`. Everything from the first section heading on is the sections. A key line after a section heading belongs to that section, so the top level is written first, as heads.conf's top-level keys were.
- **A file with no section is a kmscon.conf.** It needs no `version` line, and vtium runs the compiled-in flow for it (ADR-1401 D6), as it does today when heads.conf is missing. A file with a section needs `version = 1` at the top level. A missing or unknown version is ADR-1401 D7's error that drops the whole head file but its policy; under one file it drops every section but the policy, and the top-level keys still apply. A readable `policy = closed` in a file with no section makes it a closed file that declares no head: vtium opens nothing and every display stays dark, as ADR-1401 D7 has it for a closed file that declares no head. `vtium check-config` refuses such a file, naming the missing rules, since it wires no complete route (below), so vtium-config does not install it (the maintainer, 2026-11-05).
- **Two ways of handling a mistake, one for each part.** The top level keeps vtium.conf's reading, which follows the C reference's: an unknown key is reported once and ignored, a value out of bounds is clamped, a value vtium cannot read is ignored, and a key set twice takes its last value. The sections keep ADR-1401 D7's three levels unchanged, and what ADR-1401 D7 calls the whole file is now the sections and the policy. An error in a section never drops a top-level key, and a top-level mistake never drops a section.
- **The policy line.** ADR-1401 D7's rule for a policy line holds: a line above the first section that begins with `policy`, in any mix of case, is a policy line, and one vtium cannot read makes the sections closed and broken. Among vtium's keys none begins with `policy`, so no existing key changes meaning.
- **One bound for the file.** vtium.conf keeps its bounds, 65,536 bytes a file and 8,192 a line, which heads.conf shares. The shipped example vtium.conf and the head file's template together hold 40,327 bytes. A test fails when the generated example passes 49,152 bytes, so at least 16 KiB stay for a user's own heads, looks and rules.
- **One source for the compiled-in flow.** The head file's template, `crates/vtium-settings/templates/heads.conf`, becomes the sections of the shipped example: the example vtium.conf is the key table's commented lines, then `version = 1`, then the template's sections with the shipped rules as live lines, as ADR-1401 D6 has them. vtium's compiled-in flow is still parsed from that same text.
- **`vtium check-config FILE`** checks a candidate vtium.conf, both parts in one run with one exit status, where today FILE names a candidate head file.

> Edited 2026-11-04: the maintainer decided that vtium refuses a vtium.conf it cannot trust and says why, and that heads.conf is not read (`considered`). The trust bullet was this record's agent's call among the forced decisions, and said only that every head shows `invalid config skipped, using builtin`.

> Edited 2026-11-05: the maintainer decided that a file with no section and a readable `policy = closed` leaves vtium dark and fails the check (`considered`). The bullet on a file with no section ended at "the top-level keys still apply.", which read as if such a file ran the compiled-in flow whatever its policy.

#### A valid configuration wires at least one complete route

The maintainer decided on 2026-11-04 that a valid configuration wires at least one complete route from the left of the flow to the right (`considered`). `vtium check-config` refuses a file without one, as an error with exit 1, and the message says what is missing. The draft may still lack one while the user builds it (D8), and the view's status line says so plainly until it is wired (D7). Install asks before it installs a draft whose only problem is a missing complete route, and installs it when the person answers yes (the maintainer, 2026-11-09; D7, D8).

- **A complete route** is a display present that its first matching rule sends to a head, and at least one keyboard present that reaches that head: through an input rule whose target is that head, or by roaming, from a rule whose target is `roaming` or, under `policy = open`, from no rule at all, when the head can be the active head because it is not isolated (ADR-1401 D5). A keyboard is an input device of udev's keyboard class (ADR-1401 D4), since a mouse or a media-key device types nothing.
- **What the message says** names the first thing missing along the way, in the order a person would wire it, such as `No display present is sent to a head: display DP-8 falls to rule 3, which sends it to off.`, or `Head desk shows DP-8, but no keyboard present reaches desk: add an input rule that sends a keyboard to desk or to roaming.`, or `Head desk is isolated, so roaming keyboards do not reach it.` The flow view shows the same message on its status line while the draft has no complete route.
- **With no display or no keyboard present,** as when the check runs in a container, on a build machine, or on a desktop checking a file for another machine, the devices present cannot show a path. The check then judges the rules alone: some display rule sends displays to a head that some input rule sends keyboards to, directly or by roaming as above, with a `class:keyboard`, `*` or device rule. It notes that it judged without devices. `vtium check-config --no-devices FILE` asks for that reading on any machine, so a file written for another machine, whose rules name that machine's monitor and keyboard, can be checked where those devices are absent.
- **The shipped default always passes.** Its rules, `display * -> console` and `input class:keyboard -> roaming` with a head that is not isolated, wire a complete route on every machine with a display and a keyboard, and by the rules alone where none is present. A test holds the compiled-in configuration and the shipped example to both readings. With no file at all, nothing is checked, and vtium runs the compiled-in configuration as ADR-1401 D6 has it.
- **The console does not apply this rule when it starts.** Missing hardware never makes a file invalid at start (ADR-1401 D7): a keyboard plugged in after boot is normal, and ADR-1401 D9's guards keep a keyboard typing whatever the rules say. The rule belongs to `vtium check-config` and to vtium-config's install, which installs a file without a way for someone at the machine to type only after the person answers yes to Install's question (D7).

> Edited 2026-11-09: the maintainer decided that Install asks before it installs a draft with no complete route, then installs it (`considered`, "Ask, then allow it"). This subsection's first paragraph read: "`vtium check-config` refuses a file without one, as an error, so Install fails on it with exit 1, and the message says what is missing. The draft may still lack one while the user builds it (D8); the view and `vtium config flow` list what is missing until it is wired." Its last bullet ended: "The rule belongs to `vtium check-config` and to vtium-config's checked install, so a file is not installed without a way for someone at the machine to type."

### D4: the syntax of the sections

The maintainer chose on 2026-11-04 sections with names, as heads.conf writes them (`considered`). Below the top level come a `[look NAME]` section for each look, a `[head NAME]` section for each head, one `[rules]` section after the heads, whose rules each device tries top to bottom across every head, and, when the person has labelled a part, one `[labels]` section after the rules. This is the nesting the maintainer meant: a section's heading names the object, every line below it up to the next heading belongs to it, and a deeper link is a key that names another section, as git's `[branch "main"]` names its remote with `remote = origin` and a head names its look with `look = big`. The rules stay one list outside the heads because first match depends on their order across every head (ADR-1401 D3). A vtium name holds no blank (ADR-1401 D3), so a heading needs no quotes, as git's do.

A whole vtium.conf for a machine with two displays, a large monitor and a small panel, and one keyboard that types into whichever head is active. The person has labelled the large monitor:

```ini
# /etc/vtium/vtium.conf: two displays, one keyboard.
# The top level: vtium's keys, the defaults every head inherits.
font-size=12
xkb-layout=us
mouse
version = 1
policy = open

# A head's look: the look, which names a frit with font-name.
[look big]
font-name = monoid
font-size = 14

[look small]
font-name = dejavu

[head desk]
look = big

[head panel]
look = small
consoles = 2

# Each display and each input tries these rules top to bottom,
# across every head, and the first that matches places it.
[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display edid:BBB:0x0002:SMALL00001 -> panel
display * -> desk
input class:keyboard -> roaming
input * -> ignore

# The person's own names for displays and inputs. vtium shows a
# display's label when vtium-config identifies the displays.
[labels]
display edid:AAA:0x0001:LARGE00001 = Left monitor
```

- **The `[labels]` section** names displays and inputs in the person's own words (the maintainer, 2026-11-10; D7). It comes after `[rules]`, and each line labels one part: `display MATCH = LABEL` or `input MATCH = LABEL`. The MATCH is written as a rule's MATCH, and it is the MATCH that the part's rule uses. The label is everything after the `=` that stands alone between blanks, to the end of the line, so it can hold blanks, and it holds at most 40 characters. A `#` that would start a comment is written `\#`, as the comment rule below has it. The rules do not change for a label. When two lines of the section match one device, the first line wins, as the first matching rule does (the steering session's call, 2026-11-10). vtium reads the section for its identify overlay, which names each display by its label (ADR-1401 D12). The on-screen display's line names no device, so it shows no label (ADR-1401 D5). vtium-config reads and writes the section (D8).
- **A bad label line is dropped alone.** A line that starts with neither `display` nor `input`, that has no `=` standing alone, that has nothing after its `=`, whose label is longer than 40 characters, or whose MATCH a rule could not hold, is reported with its line number and ignored. The rules and every other line still apply, as a top-level line's error drops that line alone (D3). A second `[labels]` heading is an error, as any section named twice is.

- **A head names its VTs** (the maintainer, 2026-11-10: "vt key, default every VT (Recommended)"). A `[head NAME]` section takes `vt = N`, or a list, `vt = N M`. The vtium on ttyN builds every head whose `vt` names N, and every head with no `vt`, so a head with no `vt` runs on every VT vtium runs on, as before (ADR-1401 D2). A display or an input may be in one path for each VT: two heads whose VTs differ may both take the same display and keyboard. Inside a head, vtium's own keys switch between the head's consoles, as before: a VT picks the head and its look, and the head keeps its consoles (the maintainer, 2026-11-10: "Keep both (Recommended)").
- **heads.conf's sections move in unchanged.** Its parser and ADR-1401 D7's drop rules read them, and ADR-1304's vtlook file is written in the same syntax.
- **A `[look NAME]` section is the profile's file** that ADR-1200 D16 left for later, under its new name. It takes the keys D5 lists, under the same names the top level uses.
- **One comment rule serves the whole file,** the head file's: a `#` starts a comment at the start of a line or after a blank, a `#` inside a word is part of the word, and `\#` gives a literal `#` after a blank. vtium.conf's exception is kept: a `#` that opens a `#rrggbb` value is part of the value. A value at the top level can then hold a `#` inside it, as a head's `login` command already can. In one case a top-level line reads differently from the C reference: a `#` directly after a value, with no blank before it, as in `font-size=12#big`, stays in the value, which vtium then reports as one it cannot read. This is listed as a departure.
- **Section keys always take `key = value`;** the switches `key` and `no-key` stay at the top level.

> Edited 2026-11-10: the maintainer gave parts labels, kept in vtium.conf, and tied heads to VTs with the `vt` key (`considered`, 2026-11-10). The example ended at `input * -> ignore`, and D4 named neither the `[labels]` section nor `vt`.

Blocks in braces, as sway writes them, TOML and dotted flat keys were weighed and lost; Alternatives Considered gives each one's reasons.

### D5: how a key set at two levels combines

The maintainer accepted these rules on 2026-11-04 as this record drafted them (`considered`):

- **Three levels, and the most specific wins:** a head's section, then the section of the look the head names, then the top level. The command line sets top-level keys and wins over the file's top level, as it does today, and a section still wins over both, as a head's look wins over vtium.conf's `font-name` today (ADR-1200 D16). A key the head and its look leave unset takes the top level's value, and a key the top level leaves unset takes its default.
- **Each section kind takes a fixed list of keys.** A head takes ADR-1401 D3's head keys, `look`, which names its look, `consoles`, `login`, `when-display-lost` and `isolated`, and `vt`, the VTs it runs on (D4; edited 2026-11-10, when the maintainer added `vt`). A look takes the keys ADR-1200 D16 gives a look that the key table has: `font-name`, which names the look's frit, `font-engine`, `font-size`, `font-hinting`, `font-line-spacing`, `display-depth` and `display-dpi`, `cell`, the cell in pixels as a frit's `cell=` line writes it, and `vtlook`, which names the vtlook file of ADR-1304 that holds its filters (the maintainer, 2026-11-05). The key table gains `cell`, which only a look sets; within one look, `font-size` wins over its `cell`, as ADR-1200 D16 has a level's size win over its cell. `vtlook` is also a top-level key, the vtlook of every look that names none (ADR-1304 D5).
- **A head that names a look with no section** uses the default look, and `vtium check-config` warns, as ADR-1200 D16 has a head that names a missing look use the default. This record's agent makes that call from ADR-1200 D16.
- **A value out of bounds in a section** is an error in that section under ADR-1401 D7's strict reading, as D3 says the sections are read, and never clamped as a top-level value is. This record's agent makes that call from D3. A head sets no font key of its own, since a head's font has one path, from the head to its look to the look's frit (ADR-1401 D3).
- **A key in a section has the top level's name, values and bounds** where the top level has the key too, as `login` and the font keys do. Inside one section a key given twice is an error, as in heads.conf today.
- **A key a section kind cannot take is an error in that section,** which drops the section under ADR-1401 D7: a head drops with its rules, and a look drops, so a head that names it takes the default look (ADR-1200 D16). The error names the levels where the key may be set.
- **The key table records where each key may be set.** Its column that says a key is set in a file or on the command line gains the values `look` and `head`, so docs/config.md lists each key's levels, and allowing one more key in a section later is an edit to one row.
- **Keys that belong to the whole process stay at the top level,** such as `display-buffers`, `consoles-max`, `hwaccel` and `sb-memory`, for the reason ADR-1401 D2 gives for `display-buffers`: they trade the machine's memory or set the process's behaviour, and are the same for every head.

> Edited 2026-11-05: the maintainer gave a look the key `cell` (`considered`), and the steering session made `vtlook` a top-level key as well, the default that a look's `vtlook =` overrides (ADR-1304 D5). A look's keys ended at `display-dpi` and `vtlook`, and the bullet named no top-level `vtlook`.

The other way is to let every key be set in a head's section. It would give each head its own palette or keyboard layout, but many keys belong to the process, the seat or an input device rather than a head, and each would need a rule for what a head's value means. A key can move into the head's list later, one row at a time, when someone needs it.

### D6: no migration

There is no migration, as the maintainer decided on 2026-11-04 (`considered`). With this change vtium stops reading `/etc/vtium/heads.conf` and `seat0.seat.conf` outright: it does not look for either file, shows no warning, and has no period in which it reads either, and vtium-config has no command that folds them into vtium.conf. The only two installs that have either file are the maintainer's desktop and laptop, which the maintainer converts by hand, so the code and the docs carry no path that nobody else needs. A heads.conf or seat file left in `/etc/vtium` does nothing, and vtium never removes or changes it (ADR-1401 D6).

> Edited 2026-11-04: the maintainer decided that there is no migration (`considered`). D6 was headed "migration (open)" and recommended that vtium read heads.conf with a warning until the first beta release, that `vtium config fold` move heads.conf and the seat file's keys into vtium.conf and rename the old files to `.bak`, and that packages keep tracking heads.conf until reading stopped. The earlier text is in git at 33ea5db0.

### D7: the Flow view

> Edited 2026-11-10: the maintainer and the steering session settled a new design for the Flow view over six try-out rounds (`considered`, 2026-11-09 and 2026-11-10), and D7 is rewritten for it. The view of six columns, with rules drawn as items, tags, `c` and the route card, gives way to paths built from parts. The answers of the earlier amendments that still hold are kept, and the notes at the end of D7 name each one the new design replaces. The earlier text is in git at aca1ce30.

The figure shows the view 120 cells wide, with the `console` color set, on a machine like D4's example. It draws the whole document, as the person scrolls through it under the fixed lines at the top and the bottom. CONFIGURATION holds the path of the terminal desk, which runs on VT 3, the path of the roaming inputs, and a path to Nowhere. NEW PATH holds a path being built for a terminal on VT 4, which shares the display DP-8 with desk. PARTS shows its four groups, each led by its template.

![vtium-config's Flow view on a black console background, drawn whole, 120 cells wide. The top line holds the tabs Flow (selected), Looks, Fonts, Settings and Help, and at the right the Flow view's actions: v View file, s Save, i Install, r Revert, n New. The status line reads draft workstation, then 1 keyboard and 1 display in no path yet in amber, then 2 changes, no problems. A grey line under it reads policy open: a keyboard that no rule names types into the active terminal. The first section, CONFIGURATION, has three column titles, DISPLAYS in cyan, TERMINALS in yellow and INPUTS in green, and three rows. The first row is the path of the terminal desk: a white-bordered box DP-8, LARGE00001 by EDID, with a cyan title, joined by a solid arrow to a white-bordered box desk, font set monoid · VT 3, with a yellow title. A solid arrow from the right joins the green-titled box Left keyboard, whose second line is the system's name, Logitech USB Receiver. A grey dotted line branches from that arrow and ends in the grey italic words roaming inputs. The second row is the roaming path: the word ROAMING in green and the grey sentence These inputs type into whichever terminal is active., then a box with a dotted border, Whichever terminal is active, whichever is in use, which a dotted arrow joins to the box All mice and touchpads, mouse, touchpad, trackball. The third row is the path to Nowhere: a grey-bordered box in grey italic, Nowhere, off and set aside, which a solid arrow joins to the grey box All input devices, what no other rule names. The second section, NEW PATH, has the amber status This path is complete. Press a to add it. and one row drawn with dashed borders and dotted arrows: DP-8, LARGE00001 by EDID, to terminal-1, default font set · VT 4, from 8BitDo Keyboard, USB 3.4. The display DP-8 is in both paths, because desk runs on VT 3 and terminal-1 on VT 4. The third section, PARTS, has four groups, each led by its template, which has a double border. DISPLAYS: the template Display, Add display… [Enter]; DP-8, in desk, and the new path, with a check mark and a bright magenta border; and HDMI-A-2, in no path, with an amber border. TERMINALS: the template Terminal, Add terminal… [Enter]; and Whichever terminal is active. INPUTS: the template Input, Add input… [Enter]; Left keyboard, Logitech USB Receiver, highlighted with a heavy white border and a bold underlined title; 8BitDo Keyboard, USB 3.4, in the new path, with a check mark and a bright magenta border; All mice and touchpads, roaming; and, in grey, 046d:c52b:0, Not connected; 3 other inputs, media keys, power and sleep buttons; and All input devices, in Nowhere. FONT SETS: the template Font set, Add font set… [Enter]; monoid, in the terminal desk; and dejavu, in no path. The grey key line at the bottom reads arrows move, Space pick, Enter open, a add path, e revise, v view file, Esc clear, i install, o restore, n new, q quit.](figures/ADR-1403-flow-view.svg)

#### Three stages: parts, paths and the configuration

The maintainer named the three stages of building a configuration (2026-11-09): "I like this because it's a three stage process: parts, from the system -> composed into a flow, configurable -> added to the config, to be integrated with other flows". A part is one piece that a path can hold: a display, a terminal, an input or a font set. A path joins parts the way vtium joins them when it runs: the displays show a terminal, the terminal draws with a font set, and the inputs type into it. The configuration is the set of paths that vtium.conf holds.

- **PARTS** holds the parts, made from what the system reports and from the templates (below).
- **NEW PATH** is where the person composes one path from parts.
- **CONFIGURATION** shows the paths that vtium.conf holds. Adding a path puts it there, and it then works alongside the other paths.

The view lays the three out in that order from the top: CONFIGURATION, NEW PATH, PARTS. The person works upward: they pick parts in PARTS into NEW PATH, and add the path to CONFIGURATION.

#### Why vtium-config draws the wires

vtium-config draws every line between the boxes itself, and the person never draws one. The maintainer: "right - the difference is deliberate becuase the number of ways in which it can be wired up is pretty bounded." The shape is bounded because a path follows vtium's own pipeline from keys to pixels, and one kind of part sets each stage of that pipeline (the maintainer, 2026-11-09: "if you wanted to add to d7 that it's because we have the rendering path, the glyph generation path, even the display effects path (color grading, or with the gpu, effects) and each of those is defined in one of our objects"):

| Stage of the pipeline | The kind of part that sets it |
|---|---|
| What reaches the session | the inputs |
| The console session and its grid | the terminal |
| Glyph generation, in the font helper | the font set |
| The effects: the color filters and the spatial filters | the terminal's look, drawn as the terminal's subflow |
| Output, with its depth and color handling | the display |

The effects sit inside the terminal, as its subflow (below), and the display is an output only (the maintainer, 2026-11-10: "So, this means that the terminal itself defines the effects, and the display is just a display output."). A stage gets a kind of part of its own only once vtium builds that stage, through its own record. So a path always reads displays → terminal ← inputs, with the font set inside the terminal, and vtium-config draws the lines from that shape.

#### One document with three sections

The three sections are parts of one document, which scrolls as a whole at every size (the maintainer, 2026-11-10). The tab bar, the status line, the policy line and the key line stay fixed. Each section's title scrolls with its section. Moving the highlight scrolls the document just enough to keep the highlighted box in sight, line by line, as an editor keeps its cursor visible. PgUp and PgDn scroll by a window. A line whose other end is scrolled out of sight ends at the window's edge with a short label that names its target, such as `↑ desk`.

When anything that changes the layout changes, vtium-config lays the document out again and redraws it. A device plugged in or removed changes it, and so does an edit. The highlight stays on the same box, and that box stays at the same height on screen, so the page does not jump.

#### A path

A path in CONFIGURATION is one bordered row under three column titles, DISPLAYS, TERMINALS and INPUTS. Its displays sit on the left, its terminal in the middle and its inputs on the right, and the lines run from border to border: displays → terminal ← inputs. Several boxes that join one terminal merge into one line before they reach it. The terminal's box shows the terminal's name, and its font set on the second line, `default font set` when the path names none.

A path in vtium.conf is one head with its rules (D8). It holds:

- **a terminal**: a new terminal of its own, which is a head, or one of the two special terminals, "Whichever terminal is active" and "Nowhere" (below);
- **one or more displays**, which all show the terminal. Several displays mirror it;
- **inputs**, which type into the terminal, or "Nothing (display only)";
- **one font set**, which is optional. Picking a second font set replaces the first, because a terminal draws with one font set at a time;
- **the terminal's settings**: its name, its character size in points, how many consoles it has, its login, what happens when its display is unplugged, and the VTs it runs on.

A terminal's name holds no blank, as a head's name does (ADR-1401 D3). vtium-config names a new terminal `console` when the configuration has no terminal yet, and `terminal-1`, `terminal-2` and so on after that. The person can change the name.

**Roaming is a path too.** An input that roams types into whichever terminal is active, among every terminal of the configuration (the maintainer, 2026-11-10: "Roaming means every terminal"). vtium.conf does not change for it. The configuration draws the roaming inputs once, as a path whose terminal is "Whichever terminal is active": a box with a dotted border, because the terminal it stands for changes as the person switches terminals. Its inputs sit on the right, and it has no display, since every real terminal has displays of its own. Beside it, the word ROAMING and one sentence explain it: `These inputs type into whichever terminal is active.` Each terminal that roaming inputs can reach, because it is not isolated, has a short dotted line labelled `roaming inputs` on its inputs' side. This reading is the steering session's, of the maintainer's "I think the right fit is that it's 'any display' then the terminal style and usb reciever which should fit the path I think?", which the maintainer accepted: "ok that makes sense".

**An input in two paths roams** (the maintainer, 2026-11-09: "Yes, and it roams"). When the person picks an input that another path holds on the same VT, the status line says in one sentence that it will type into whichever terminal is active. Adding the path moves that input to the roaming path. A display or an input can be in one path for each VT (the maintainer, 2026-11-10: "vt key, default every VT (Recommended)"). A terminal with `vt` (D4) shows its VTs on its box's second line, such as `font set monoid · VT 3`, and two paths may share a display or a keyboard when their terminals' VTs differ. A part already in a path can be picked again for a path whose terminal runs on another VT. vtium-config refuses it only when the two paths clash on one VT, and the sentence that refuses it names that VT (the steering session's call, 2026-11-10). The maintainer's example: "DP-8 is the display, and connected to the '8 bit do keyboard'. the terminals in the middle, are variants - they're not bound to each VT, but VT3, VT4, and VT5 - that's what allows all three flows to exist in CONFIGURATION (so they're unique)." Among terminals that share a VT, or that have no `vt`, a display is in one path only (the maintainer, 2026-11-09: "Same devices always conflict"), and a conflict names the VT where it clashes. On a VT where none of a device's terminals runs, the device stays with them and does nothing there: a keyboard types nowhere, and a display stays dark (the maintainer, 2026-11-10: "Stays with its terminals (Recommended)"; ADR-1401 D3). vtium does not hand it to the policy there. On each VT, vtium places each device by the procedure in ADR-1401 D3's table, which D7's path tests check step by step. Under the open policy, vtium still watches such a keyboard for the rescue key. The maintainer: "when a user navigates with console commands to a different terminal they're already logged into, that either means all displays get that terminal, or if they're all differentiated, the character and personality changes as the user changes focus. if the user had unique keyboards tied to each other, then that keyboard stays with it's attached terminals".

#### Composing and adding a path

Space on a part picks it into NEW PATH, and Space again takes it out. A picked part keeps its place in PARTS, with a check mark `✓` and the border color of the picked state. NEW PATH draws the path being built in the same shape as a path in CONFIGURATION, with dashed borders, and an empty place in it reads `Choose… [Enter]`. Its status line says in words what the path still needs:

- `Pick an input and a display to make a path.`, while it is empty;
- `Add a display to finish this path.`, while it has a terminal and inputs but no display;
- `This path is complete. Press a to add it.`, once it is complete.

**When a path is complete.** A path to a terminal of its own is complete with its terminal and at least one display. With no input of its own, only the roaming inputs type into it, while it is active, and the status line says so: `This path is complete. Only roaming inputs type into its terminal, while it is active. Press a to add it.` A path to "Whichever terminal is active" is complete with at least one input and no display. A path to "Nowhere" is complete with at least one display or input. As soon as the path is complete, the status line asks the person to add it (the maintainer, 2026-11-09: "I thin kit's ask when it's complete, and its possible the 5 november choice wasn't clearly asked at the time"). The whole configuration still needs a complete route from a display to a keyboard (D3), and the status line says while it has none.

**`a` adds the path** to CONFIGURATION, and vtium-config checks it there (the maintainer, 2026-11-09: "once they're satisfied (including the name of it if they'd like) they can add it back to the doc/config wher eit's validated again. if it's invalid, it doesn't get added and remains in the editor with some guidance as to what was in conflict."):

- A path with exactly the parts of a path already added changes nothing, and the status line says so.
- A display that another path holds on the same VT is a conflict. The path stays in NEW PATH, and one plain sentence names the terminal that has the display and the VT where they clash.
- A name that another path uses is refused the same way.

**A complete path can be previewed first** (the maintainer, 2026-11-09: "probably offer to preview (using our timeout delay if things go wrong)"). Where ADR-1401 D12's trial can run, the status line of a complete path reads `This path is complete. Press a to add it, or p to try it on the console for 10 seconds first.` On `p`, vtium-config hands vtium the draft with the unsaved changes and the path applied, as a trial: vtium applies it and counts down 10 s with a notice on every lit display, as ADR-1401 D12 describes. The preview always ends by going back to the configuration vtium ran before it, and the installed file never changes. The key `k`, pressed on the console during the countdown, keeps the path: vtium-config then adds it to CONFIGURATION as `a` would. Any other key, or the end of the countdown, adds nothing, and the path stays in NEW PATH so the person can change it (the maintainer, 2026-11-09: "as proposed. this is like kde display settings if you don't keep them they revert"). The trial can run when vtium-config runs on a vtium console that can apply a candidate while it runs, and the person can start a trial, which goes through the privileged step, so sudo may ask for a password (ADR-1401 D12). Elsewhere the status line does not mention a preview. `p` there says in one sentence that previewing a path needs a vtium console. The preview comes with the build of ADR-1401 D12's trial and the console's live apply.

#### Templates and the part makers

PARTS holds four groups, DISPLAYS, TERMINALS, INPUTS and FONT SETS. Each group starts with its template, one for each stage of the pipeline: Display, Input, Font set and Terminal (the maintainer, 2026-11-10). A template is never placed in a path. It is where the person makes a new part (the maintainer: "you can't actually add the template object (it could always be considered a template part) and if one enters into a part, this is where one creates a new part."). A template's box has a double border, its title in the kind's color, and the second line `Add display… [Enter]`, `Add input… [Enter]`, `Add terminal… [Enter]` or `Add font set… [Enter]`. Space on a template says what it does, as in `The Display template adds displays. Press Enter to add one.`

Enter on a template, or on an empty place in NEW PATH, opens its part maker. The part maker asks one question and lists the choices that answer it. Each choice says in plain words what it catches, and the description on its right names what it writes in vtium.conf. The words follow the settings of software people know, as the maintainer asked (2026-11-10: "think about real software that exists that sort of offers this ability. usually they're phrased pretty plain/obvious"): Windows and macOS say "Add device", Outlook's rules and Gmail's filters name each condition plainly, and a list where you can type your own value ends with "Other…", as macOS's list of Wi-Fi networks does. The maintainer accepted these strings: "I think your proposed strings already would make readability significantly better".

| Part maker | Heading, then its line | Choices, each with its description |
|---|---|---|
| Input | `ADD AN INPUT`, then `Which inputs should this include?` | `Nothing (display only)`, no keyboard or mouse reaches this terminal; `All keyboards`, class keyboard; `All mice and touchpads`, mouse, touchpad, trackball; `All input devices`, everything. Under the label `OR CHOOSE BY`: `USB port`, anything plugged into that port; `This one device`, its serial number, follows it to any port; `Model`, every unit of that make and model; `Bluetooth device`, by its address; `Device name`, the name the device reports; `Device type`, touchscreen, tablet, joystick, …; `udev property (advanced)` |
| Display | `ADD A DISPLAY`, then `Which displays should this include?` | `All displays`, connected now or later; `This monitor`, its EDID, follows it to any port; `Connector`, whatever is plugged into HDMI-A-1, DP-2, …; `Hub port`, one port of a DisplayPort hub; `Graphics card`, every output of one card |
| Terminal | `ADD A TERMINAL`, then `Where should this path go?` | `New terminal…`, a terminal of its own; `Whichever terminal is active`, the inputs follow you between terminals; `Nowhere`, displays stay off; inputs are ignored |
| Font set | `ADD A FONT SET`, then `Which font set should the terminal use?` | the font sets the draft defines, and the default font set |

Keyboards and pointers are choices inside the Input template, not templates of their own. `New terminal…` asks for the terminal's name, font set and size, as the terminal's card does (below).

**A choice that filters opens a picker.** The picker lists every value the connected devices give for that match, each with the devices it matches now. Its heading is the choice's label in capitals, such as `USB PORT`, and its line reads `Each port lists what is plugged into it now.`, or for another picker `Each <value> lists what matches it now.`, such as `Each model lists what matches it now.`. The last item is `Other…`, `type a value`, where the person types a value, and the key line reads `type a value  Enter add  Esc cancel`. vtium-config checks a typed value as vtium.conf's parser checks a rule's match. A value that fails gets one sentence, topic first, such as `This USB port is not a valid port path. A port path looks like pci-0000:00:14.0-usb-0:3.4.`

**A choice makes a part.** The part joins its group in PARTS, named by what it matches: a device's own name, the value typed, or the choice's own words. The status line then says, for example, `Added DP-9. Press Space to put it in the new path.` When the part maker was opened from NEW PATH, the new part is also picked into the new path. The maintainer chose that the whole-kind choices and the nothing choices live inside the templates (2026-11-10: "inside the templates. then when the template is instanced, as a device or a special device (no input, every keyboard, etc) the part gets the name displayed."). So the parts they make carry the same words as the choice: `All keyboards`, `All mice and touchpads`, `All input devices`, `All displays`, `Nothing (display only)`, `Whichever terminal is active` and `Nowhere`.

**An exception needs no new syntax** (the maintainer, 2026-11-10: "the exception goes elsewhere"). A device placed in a path of its own gets a rule above the broad rule, most specific first (D8), so the broad rule never reaches it. For example, with All keyboards in the path of desk, a keypad picked into a path of its own types into that path's terminal and not into desk. The device stays free to place anywhere. Only a device that no rule matches falls to the policy.

#### The nothing choices

The maintainer asked for a way to say "nothing" in a path (2026-11-10: "for a terminal with displays and no inputs like a kiosk screen, can we use a null definition? this could be used for displays and inputs, and allow flexibility while evaluating the rules consistently"). Each nothing choice writes vtium.conf's own words (D8), so every rule reads back as a path:

- **Nothing (display only)** stands in the inputs' place. Nothing types into the terminal, roaming inputs included. The box draws with no line to the terminal. A kiosk's screen is the example.
- **Nowhere** stands in the terminal's place. A display in a path to Nowhere stays off, and an input in it is ignored. Its box is drawn as set aside.
- **No display** is not a choice. Every path to a terminal has at least one display, connected or not.

#### Parts from the devices present

When the view opens, vtium-config makes parts from three sources (the maintainer, 2026-11-10: "on opening a flow, we enumerate all the candidate devices and create parts based on whats found. if the state changes, we rerun this same process, diff against the objects found, and only create new objects. if a device is removed, we don't delete the part but ghost the part to indicate it's not connected any more, but still recognized."):

- every connected display, keyboard and pointer;
- every device or match that the draft's rules or its `[labels]` lines name;
- the special parts the draft uses, such as All keyboards or Nowhere.

vtium-config reads the devices from sysfs and udev's database, without privilege and opening nothing under `/dev`, as `vtium check-config` reads them (ADR-1401 D7). While the view is open, it follows hotplug through the udev monitor vtium-platform gives the console (the maintainer, 2026-11-04). When a device comes or goes, vtium-config reads the devices again and compares the result with its parts by each part's match. It adds a part only for a device that is new. A part whose device leaves goes grey, its second line reads `Not connected`, and it stays for as long as the view is open. A device that comes back makes its grey part normal again, with no second part. A part that a rule names is grey and `Not connected` from the start when nothing matches it. Parts the person made stay until the view closes. vtium-config stores nothing new for the parts: they are made again from the draft and the devices each time the view opens (the maintainer, 2026-11-10). `d` reads every device again from the start, for an event the view missed. With no udev daemon no events arrive, so the status line says that the view cannot follow hotplug, and `d` brings it up to date. A part that matches by `udev:`, `path`, `serial`, `card` or `connector` then shows the words `needs udev`, the note `vtium check-config` gives.

Unplugging a device never removes it from the configuration (the maintainer, 2026-11-04). Its rule goes only when the person takes its part out of a path, through Revise (below).

**The inputs that can be part of a path** (the maintainer, 2026-11-09). An input can be part of a path when udev, or vtium's own copy of udev's `input_id` rule where no udev daemon runs, gives it the keyboard class or a pointer class: `mouse`, `touchpad`, `touchscreen`, `trackball`, `pointingstick`, `tablet` or `tablet-pad` (ADR-1401 D4). Every other input folds into one grey box at the end of the INPUTS group, such as `3 other inputs`, whose second line names what they are, such as `media keys, power and sleep buttons` (the maintainer, 2026-11-09: "Fold them into one block"). The box cannot be picked. Enter on it lists each of its devices with the rule that catches it. A device of this kind that a rule names by a match for one device, such as `name:Power Button`, is a part of its own, since the person chose a rule for it (the maintainer, 2026-11-09: "Accept all").

#### Boxes that reflow

Boxes flow like words, at every size (the maintainer, 2026-11-10: "I guess by 'on a small terminal let's make it...' I actualy mean, we always make it reflowable - regardless of size"). In each group of PARTS, the template comes first and the parts follow. A box is the unit that wraps (the maintainer: "if we can only fit a few boxes before running out of columns, it's a 'newline' (but an entire box in a newline) (so we just treat the box as the composable element). if I shrink it down very narrow, then we only get one or two boxes and it stretches out."). Each box takes the width its text needs, up to a limit. A box that does not fit on the line starts the next line, whole. When a line holds one or two boxes because the terminal is narrow, those boxes stretch to share the line's width. Several boxes on one side of the path in NEW PATH flow the same way. The rows of CONFIGURATION keep their three columns, because the lines between the boxes depend on them. On a narrow terminal their boxes narrow and their second lines shorten.

#### The terminal's subflow: font set and effects

The maintainer placed the effects inside the terminal (2026-11-10: "in our configuration, where do we represent post filters (color filters, effects shaders) - I think they're 'inside' the terminal", then "maybe as their own subflow"), and asked for the subflow to be drawn now ("Draw it now"). The maintainer: "This means that the TERMINALS part is where we keep our subflow."

The figure shows the card of the terminal desk, which runs on VT 3, with its subflow: the font set monoid, one spatial filter, two color filters, and the display DP-8. A second line shows what the CPU path draws.

![A terminal's card on a black console background. The heading reads TERMINAL desk, followed in yellow by VT 3, and under it, in grey, look amber, size 14 pt, 1 console. Below, under the title WHAT THIS TERMINAL DRAWS, a row of boxes joined left to right by solid arrows: a magenta-titled box Font set, monoid; a box Spatial filters, scanlines; a box Color filters, tint, contrast; and a cyan-titled box DP-8, LARGE00001. Under the row, a grey line reads Requires: gpu any. On the CPU path, vtium draws the color filters and skips the spatial filters. A key line reads arrows move, Enter open, e revise this path, Esc back.](figures/ADR-1403-terminal-subflow.svg)

- **What the subflow shows.** Enter on a terminal's box, in CONFIGURATION or in NEW PATH, opens the terminal's card, and the card draws the terminal's subflow as a small path, left to right: the font set → the spatial filters → the color filters → the terminal's displays. The card's heading names the VTs the terminal runs on, such as `TERMINAL desk · VT 3`. The order of the filters is the one ADR-1303 D1 and ADR-1304 D4 decide: the spatial filters, then the color filters. On 2026-11-10 the steering session corrected the figure to follow those records, since an earlier note had the order the other way round (the steering session's call). The subflow draws what ADR-1304 decides. The terminal's look names a vtlook (ADR-1304 D5). The vtlook's color layer is one table built from its color filters (ADR-1304 D2), and its spatial layer is a chain of shaders that run as the frame is presented (ADR-1304 D3, D15). Each filter box names its filters in their order. A look with no vtlook draws one box, `default look (built in)`, between the font set and the displays.
- **The display is an output only, and it decides what can be drawn on it.** A display part carries no effects. A terminal's look and effects apply on each of its displays as far as that display can draw them: its color depth, and whether the GPU path is available (ADR-1304 D7). The maintainer: "the TERMINAL is still bound to a display, and the display dictates what can be rendered (colors (or not), hardware (or not)". Two terminals on different VTs that share a display (D4) each draw it with their own look.
- **Requirements.** Under the subflow, one grey line gives the vtlook's `[requires]` section (ADR-1304 D7) and what follows from it. With `gpu = any`, a terminal drawn on the CPU path draws the color filters and skips the spatial filters (ADR-1304 D3, D7), and the line says so. A display that does not meet the look's requirements draws the default look alone, while the terminal's other displays keep the look, and vtium's log names the display and the requirement it misses (ADR-1304 D7). The subflow draws that display's box in grey with the same words. A look that fails puts the terminal on the default look (ADR-1304 D6), and the card shows the error's report.
- **Editing the effects is the Looks view's job** (ADR-1201 D3, ADR-1304 D11). Enter on a filter box opens the Looks view on the terminal's look.

> Edited 2026-11-10: the maintainer placed the effects inside the terminal, as its subflow, and asked for it to be drawn now (`considered`, 2026-11-10). D7's first version of this rewrite said that the display effects would arrive as a kind of part of their own, through their own record.

#### Path tests

Each of these worked examples is a scenario the build tests, from vtium.conf through the Flow view to what vtium does on each VT (the maintainer, 2026-11-10, who imagined the fourth of them as "another 'path test'"). vtium places each device on the VT shown by the procedure in ADR-1401 D3's table, which has four steps. Each test names the steps it checks:

1. **One display, one keyboard, three terminals.** Terminals A, B and C run on VT 3, VT 4 and VT 5, and each one's path holds the same display and keyboard. Ctrl-Alt-F4 shows terminal B, with B's look and effects, and the keyboard types into B. It checks steps 3 and 2: on VT 4, vtium notes the rules of A and reads past them, and B's rules place the display and the keyboard.
2. **Two displays, a terminal for each on each VT.** Each display has its own terminal on VT 3 and another on VT 4. A switch to VT 4 changes both displays at once, each to its own VT 4 terminal. It checks steps 1, 3 and 2: the rules of one display do not match the other, the rules of the VT 3 terminals are noted, and the rules of the VT 4 terminals place the displays.
3. **A terminal with no `vt` on both displays.** Every VT shows it, as before `vt` existed. It checks step 2 for a terminal with no `vt`, which runs on every VT.
4. **Two keyboards, each tied to its own terminals.** After a switch, each keyboard types into its own terminal on the new VT. It checks steps 1, 3 and 2, as test 2 does for displays.
5. **A keyboard whose only terminal runs on VT 3.** On VT 4 it types nowhere, and under the open policy its rescue key still works. It checks step 4 after step 3: the keyboard stays with its terminal, and the policy does not place it.
6. **A display whose terminals run on VT 3 only.** On VT 4 it stays dark. It checks step 4 after step 3, for a display.
7. **A keyboard tied to VT 3, and a broad rule after it.** The keyboard is in a path whose terminal runs on VT 3, and the file's last input rule is All keyboards → Whichever terminal is active. On VT 4 the keyboard types nowhere. It checks the exception in step 2: once step 3 has tied the keyboard to the VT 3 terminal, the broad rule leaves it alone (the maintainer, 2026-11-10: "Stays with its terminal (Recommended)").
8. **Two terminals that share a display and a keyboard.** Terminals on VT 3 and VT 4 each hold DP-8 and the same keyboard in their paths. Each VT shows its own terminal on DP-8, and the keyboard types into the terminal of the VT shown. It checks steps 3 and 2 for two terminals that share both devices, as the first figure's two paths do.

> Edited 2026-11-10: the maintainer decided that a device stays with its terminals on a VT where none of them runs, and that the display decides what a terminal's look can draw on it (`considered`, 2026-11-10); the path tests were added with it.

> Edited 2026-11-10: each path test now names the steps of ADR-1401 D3's table that it checks, and tests 7 and 8 were added, after the maintainer accepted the placement procedure and settled the exception in its step 2 (`considered`, 2026-11-10). The first sentence said the maintainer imagined "the last of them" as a path test.

#### Labels

The person can give a part a label, a name of their own (the maintainer, 2026-11-10: "I'd like to be able to add a label to each part, which is just a name attached to the part. This sort of follows the TERMINALS convention where the name of it is cusomizable. These names actually could make it all the way to the OSD to show what path is what. If there is no customizable name then the OSD just displays what was picked up by the system.").

- Enter on a part opens its detail, which has the field `Label`. For a display, the field is described as `shown on the display when you identify the displays`. For an input, it is described as `shown in vtium-config`. The field takes up to 40 characters, spaces included, and does not take a `#` as its first character. Clearing the field removes the label. The detail also lists the rules that name the part, for each VT (the maintainer, 2026-11-10: "Drop both (Recommended)").
- A labelled part shows its label as its box's title, and what the system calls the device on the second line, such as `Left keyboard` over `Logitech USB Receiver`. A part with no label shows what it shows today.
- A part made from a whole-kind choice, such as All keyboards, can carry a label. Nothing (display only), Whichever terminal is active and Nowhere cannot. A terminal has its name instead.
- vtium-config writes each label into vtium.conf's `[labels]` section (D4, D8). A label belongs to its part, not to a path, so revising a path leaves it as it is.
- vtium shows a display's label on the identify overlay, and the display's connector when it has no label (ADR-1401 D12). The on-screen display's top line stays as it is, and shows no device's label (ADR-1401 D5). This is the steering session's call (2026-11-10), made on the maintainer's wish that labels "could make it all the way to the OSD". An input's label shows in vtium-config's views only.

#### Colors, borders and lines

Color tells two things apart, and every state also has its words, so the view works without color.

- **The kind's color is on the title.** Each box's title, and each column and group title, takes its kind's color: displays cyan, terminals yellow, font sets magenta, inputs green (the maintainer, 2026-11-10: "we could almost use some of the categorical colors from rules view in the flow view").
- **The border shows the state.** A white border is a part placed in a path, or free to pick. A part in a path stays free to pick, since a path whose terminal runs on another VT can share it (D7, "A path"). An amber border is a connected device in no path yet. A grey border, with its text in italic, is a part that cannot be picked: a part in a path to Nowhere, a part whose device is not connected, or the folded other inputs. A picked part has a border in a color no other state uses, and a check mark.
- **The kind of border shows the kind of box.** A template has a double border. A box in NEW PATH has a dashed border, because the path is not added yet. Whichever terminal is active has a dotted border, because it stands for a terminal that changes.
- **The highlight** is a heavy border, `┏━┓┃┗━┛`, in bold, which keeps the border's state color, and a bold, underlined title.
- **Each kind of line means one thing.** A solid line is a connection in force in the configuration. A dotted line is a connection that is not in force all the time: a path still being composed in NEW PATH, or a roaming input's line, which holds only while its terminal is active. A heavy line is a connection of the highlighted box.

vtium-config takes these colors by role from a file of the person's own, `$XDG_CONFIG_HOME/vtium/vtium-config.conf`, which holds four sets of colors and can fill each kind's boxes (ADR-1201 D13). With `NO_COLOR` set or on a terminal without color, the view draws the same text and borders with no color.

#### The keys

The key line shows only the keys that fit what the person is doing now (the maintainer, 2026-11-09). With nothing open, it reads `↑↓←→ move  Space pick  Enter open  a add path  e revise  v view file  Esc clear  i install  o restore  n new  q quit`, and it adds `p preview` where a preview can run and `PgUp PgDn scroll` where there is room.

- **Moving.** The arrow keys move the highlight from box to box in reading order. Left and Right move along a line of boxes and on to the next line. Up and Down move to the box above or below that is nearest in column. Home and End go to the first and the last box. PgUp and PgDn scroll by a window. Tab moves to the next view of the TUI and Shift-Tab to the view before, wherever the highlight is, as in every other view (the maintainer, 2026-11-07). While a detail, a part maker, a picker, the file view or a question is open, Tab and Shift-Tab do nothing, and Esc closes it. Coming back to the Flow view keeps the box that was highlighted.
- **Space** picks the highlighted part into NEW PATH, or takes it out.
- **Enter** opens the highlighted box's detail: a part's detail with its label, a template's part maker, a path's card in CONFIGURATION, or the folded other inputs' list.
- **`a`** adds the path in NEW PATH. **`e`** revises the highlighted path in CONFIGURATION. **Esc** clears NEW PATH.
- **`v`** views the file (below).
- **`s`** saves, **`i`** installs, **`r`** reverts, **`o`** restores and **`n`** starts a new configuration (below). **`d`** reads the devices again.
- **`p`** previews a complete path (above), and **`t`** tries a font on the live console (ADR-1201 D4). The notice that the live trial is off shows only when the person presses `t`.
- **`q`** quits.

The mouse selects a box with a click and opens it with a second click, and a click on a view's name in the tab bar switches to that view while nothing is open (the maintainer, 2026-11-04). The view turns on mouse reporting, modes 1000 and 1006, which vtium supports (ADR-1000 `DEC-1000`, `DEC-1006`), and turns it off when the TUI leaves the view. In a terminal without mouse reporting, the keys do everything.

#### `v` views the file

`v` shows the draft's vtium.conf as text, read-only, with the unsaved changes applied, exactly what Install would write (the maintainer, 2026-11-10: "we could drop the rules view and use the same key (v) to view the configuration file. (I don't see an obvious way to just view the file as it is configured)"). It opens at the line of the highlighted box's rule and marks each line the check complains about. `j` and `k` or Up and Down scroll by lines, `h` and `l` or Left and Right scroll sideways, `g` and `G` go to the top and the bottom, Ctrl-d and Ctrl-u move half a page, and `q` or Esc go back to the configuration with the same box highlighted. The file view saves and installs too (the maintainer, 2026-11-10: "in the viewing config file, we should allow to install/save as well."): `s` and `i` act as they do in the configuration, and the file view then stays open and shows the file as it now stands. The view of six columns, which drew each rule as an item, is gone. The rule order shows in the file, and vtium-config sets that order itself (D8).

#### Editing an added path, and Revise

**Enter on a path in CONFIGURATION opens its card,** which changes in place what allocates nothing (the maintainer, 2026-11-09: "I think the main reason to restrict changes in the config doc has more to do with resource allocation (displays, input devices, available font resources) than anything else. so altering the default character size in points, or the name of the flow, or the font used (only pickable from defined fonts) when pressing 'enter' (entering into a flow in the doc) maintains the stability of resource conflicts."). The card changes the path's name, which is refused when another path has it; its font set, chosen from the font sets the draft defines and never typed; its character size in points; and the terminal's consoles, login and what happens when its display is unplugged. The card does not change the VTs the terminal runs on. A path's `vt` changes only through Revise, so that vtium-config checks each VT for clashes when the path is added again (the steering session's call, 2026-11-10). The card says in one line how to change the rest: `Which displays and inputs this path joins change only when you revise it: press e.` The same card serves NEW PATH, where it also connects and disconnects parts (the maintainer, 2026-11-09: "and obviously, if it's a net new path never added to the doc, the user has a chance to name it and set those same properties in the pre-document editor before sending it to the config doc"). Enter on a font set opens its preview, the Fonts view's picture of the console in that font set.

**Revise, on `e`,** takes the highlighted path out of CONFIGURATION and puts it into NEW PATH with its parts picked, ready to change (the maintainer, 2026-11-09: "one of the things the user can do is revise it for editing, (some sort of wording like that) which actually removes it from the doc."). While NEW PATH already holds parts, `e` asks the person to add them or clear them first. Esc on a revised path takes it out of the configuration, which is how a path is removed, and the status line says so: `Took the path you were revising out of the configuration.`

**Revise comes back unchanged.** Whatever Revise takes out of CONFIGURATION can be added straight back, and adding it back leaves the draft's text byte for byte as it was, its labels included. The maintainer found the fault this rule closes (2026-11-10: "I was able to edit a roaming relationship between a terminal and a device (logitech usb reciever by id) but once in edit mode was unable to add it back until I also added a display. so there's a fault in the logic to round trip that sort of configuration"). The build keeps a test that revises and adds back each kind of path the configuration shows: one display and several, a font set and a size, the shipped default, the roaming path, a path with a narrowed input, a path with Nothing (display only), both kinds of path to Nowhere, the catch-all ignore, and a terminal with displays and no input rule.

#### The shipped default as it reads

The shipped configuration is the smallest one a person can pick (the maintainer, 2026-11-09: "due to this known path bound, it's easy for a user to pick the most minimum of config: any keyboard, any display, no font selection, all VT slots. this basically matches the default VT experience"). With no vtium.conf, or after New, CONFIGURATION shows the shipped rules (D3) as three rows, under one grey line, `This is vtium's built-in default: every display shows the terminal console, and every keyboard and pointer types into it.`:

- All displays → console, with the default font set and the roaming inputs' short line;
- Whichever terminal is active ← All keyboards and All mice and touchpads;
- Nowhere ← All input devices, the catch-all `input * -> ignore`.

On a configuration with no terminal and no rule, the first path's inputs roam, as the shipped default's do. So picking All displays and All keyboards, and nothing else, writes exactly the shipped default's rules, pointers included, and reads back as the same rows. Adding that path to a configuration that already holds it changes nothing.

#### What carries over

- **The configuration shown.** CONFIGURATION shows the draft being edited, with its unsaved changes, and the top line names it, as `draft north` (D8). The drafts, New and Restore reach another configuration, and Install installs it, through the flows that exist (the maintainer, 2026-11-09).
- **Changes wait for Save** (the maintainer, 2026-11-09: "the user can add paths and edit, and when done, save. if the operator/user elects to install, the config goes through the save path first then installs. either way the configuration file is validated."). The TUI keeps the changes from every view as one set of unsaved changes (the maintainer, 2026-11-09: "Accept all"). After each change, the view runs `vtium check-config` on the draft with the changes applied, shows each problem in place on the box it concerns, and redraws. A configuration can be built in any order and left unfinished.
  - **Save**, on `s`, runs `vtium check-config` on the draft with the unsaved changes applied, and writes the result into the draft, and the font set changes into the draft's font set folder (D8). It writes whatever the check finds, since a draft may be unfinished, and asks nothing (the maintainer, 2026-11-09: "Accept all").
  - **Moving to another view** keeps the unsaved changes.
  - **Quitting with `q`** while changes are unsaved asks `2 changes are not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back.` (the maintainer, 2026-11-09: "As proposed").
  - **When the TUI stops for another reason,** by Ctrl-C, by SIGINT, SIGTERM, SIGQUIT or SIGHUP, or because its terminal closed, it saves the unsaved changes into the draft before it leaves (the maintainer, 2026-11-09: "As proposed"). A panic and SIGKILL lose them.
  - **Revert, Restore and New** throw the unsaved changes away with what they replace, and their questions count the unsaved changes among what is lost.
- **Each view shows its own controls** (the maintainer, 2026-11-09: "this sounds like contextual controls (by tab) and I think it makes more sense to declutter"). Beside the views' names, the tab bar shows only the actions of the view in front, each with its key. In the Flow view they are View file (`v`), Save (`s`), Install (`i`), Revert (`r`) and New (`n`), and Restore is on `o`. In the Looks, Fonts and Settings views they are Save and Install, and Help shows none (the maintainer, 2026-11-09: "Accept all"). The view has no Delete draft: `vtium config draft rm` deletes a draft (D8). Each action but View file and Save asks before it acts:
  - **Install** saves first, then installs the draft, as `vtium config install` does (D8). When the check finds a problem other than a missing complete route, Install refuses and lists what fails. When the only problem is that no complete route exists (D3), it asks `This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches. After the install, nobody at this machine may be able to type into vtium. Install it anyway? 3 lines added, 1 removed.` and installs only on yes (the maintainer, 2026-11-09: "Ask, then allow it"). When the check passes, it asks `Install the draft? 3 lines added, 1 removed.` and installs only on yes.
  - **Revert** asks `Throw away the draft's changes? 3 lines added and 1 removed are lost.`, then copies the installed file back over the draft. The installed file is untouched.
  - **Restore** lists the installed history (D8), newest first, and copies the chosen file into the draft after asking `Replace the draft with installed file 3? 2 lines added and 5 removed are lost.` Nothing but Install writes `/etc/vtium/vtium.conf` (the maintainer, 2026-11-05).
  - **New** asks `Start a new configuration? The draft's heads, rules and looks are lost.`, then empties the draft to the top level at its defaults, every key commented out as in the shipped example, and one look, `[look default]`, with no keys.
  An installed change takes effect when vtium next reads its configuration.
- **The status line is minimal** (the maintainer, 2026-11-04). It names the draft, then what is not placed yet, then how many changes the draft holds against the installed file, how many are not saved, and how many problems the check finds, as in `draft north · 1 keyboard and 1 display in no path yet · 2 changes · 1 not saved · no problems`. Under it, one grey line says what the policy gives a device no rule names, such as `policy open: a keyboard that no rule names types into the active terminal`. A connected display is placed when any rule sends it to a head, and a keyboard or pointer when it reaches a head, through a rule or, under `policy = open`, through no rule at all (the maintainer, 2026-11-09: "Any rule that reaches a head"). A display that a rule naming that one display sends to off, and an input that a rule naming that one device ignores, count as placed too (the maintainer, 2026-11-09: "Accept all"). When everything connected is placed, the status line says `complete`. While no complete route exists (D3), the status line says so plainly and names what is not placed (the maintainer, 2026-11-09: "allow while building, with a status line that clearly states the path isn't complete"), as in `No complete path yet: no connected display is sent to a head that a connected keyboard reaches. Not placed yet: HDMI-A-2, keyboard at usb-0:3.4.` Every message about a problem names a key that fixes it. The configuration's purpose is at least one working path (the maintainer, 2026-11-09: "the config's main purpose is to maintain at least one valid path, all the way to all paths the hardware could support. (the user doesn't have to define every path)").
- **A rule that fits no path.** Every rule in `[rules]` reads as a path, the catch-all included. A rule that does not, such as one that sends to a head that does not exist, is counted in a grey line in CONFIGURATION, `1 rule belongs to no path. v shows it in the file.`, and the check lists it.
- **A draft with no sections** runs the shipped rules (D3), and the view shows them as the shipped default. The first change to such a draft first adds the shipped sections to the draft's unsaved changes, `version = 1` and the template's looks, heads and rules, and then makes the change (the steering session's call, 2026-11-09).
- **This screen** (the maintainer, 2026-11-09: "we need to be able to remain on the display we're editing ( like a, 'this screen' or 'this display path')"). vtium-config learns the head it runs on from `VTIUM_HEAD` (ADR-1401 D5, D12). The view writes the words `this screen` on the box of that head's terminal and on the box of each display the head shows, and it opens with the highlight on the first of those displays. The highlight stays there while devices come and go and while vtium-config identifies a device (ADR-1401 D12), until the person moves it. When `VTIUM_HEAD` is unset, as over ssh or in a terminal that is not a vtium console, the view marks no terminal, says nothing about it, and opens with the highlight on the first box of CONFIGURATION (the maintainer, 2026-11-09: "As proposed").
- **The terminal's minimum size** is 80 by 25 cells (the maintainer, 2026-11-05). It stays 80×25 now that the view is one document that reflows (the steering session's call, 2026-11-10). The minimum applies to the terminal vtium-config runs in, not to the machine or its console: a minspec board with a 40-column console is configured over ssh from a terminal at 80 by 25 or larger, with the commands of D8 at any size, or with a draft built on another machine and copied over, since a draft is a complete vtium.conf (D8). Below the minimum, the TUI draws no view: it shows one screen with the size it needs, the `vtium config` commands, and the keys to leave and to undo a font preview that made the grid too small.
- **How to reach it.** Flow is the first of vtium-config's views, Flow, Looks, Fonts, Settings and Help, and the one vtium-config opens on (ADR-1201 D3). `vtium config flow` prints the flow as text, or as JSON with `--json`, and exits. Every change the view makes is also a command (D8), and the view is a front end over the same operations. The view reads vtium.conf, the frits and the devices as the user, with no privilege; only Install asks for sudo.

#### The words

The view and the docs use these words for one thing each. The maintainer fixed seven of them on 2026-11-10, part, template, path, pick, add, revise and label ("Accept all seven (Recommended)"). The words draft, save, install and restore come from the maintainer's earlier lists of 2026-11-05 and 2026-11-09. A **part** is one display, terminal, input or font set that a path can hold. A **template** is the box that makes parts of one kind. A **path** is a terminal with its displays, inputs and font set. To **pick** is what Space does: it puts a part into NEW PATH. To **add** is what `a` does: it puts the new path into CONFIGURATION. To **revise** is what `e` does: it takes a path out of CONFIGURATION into NEW PATH. A **label** is the person's own name for a part. The **draft** is the user's copy of vtium.conf, which Save writes. To **save** is to write the TUI's unsaved changes into the draft. To **install** is the one action that writes `/etc/vtium/vtium.conf`, through sudo. To **restore** is to copy a file from the installed history into the draft. The word configuration means the paths of the draft and of the installed file. The rule of D3 that a valid file lets a keyboard reach a head with a display is called a complete route, and the match key `path:` of ADR-1401 D4 keeps its name, since it names a USB port's path.

#### What the new design replaces

> Edited 2026-11-10: the new design replaces these answers. Each stays in `considered` as it was said.
>
> - The six columns with rules drawn as text, the blocks of color, the rule numbers, the lanes and arrows between columns, and the `↓ 4 more` counts (the maintainer, 2026-11-04 and 2026-11-05) give way to the rows of paths, because vtium-config now draws the lines from a path's fixed shape.
> - The `new…` items at the foot of the columns (the maintainer, 2026-11-04) give way to the four templates in PARTS.
> - Tagging with Space and connecting with `c`, rules and looks among the tagged items (the maintainer, 2026-11-04 and 2026-11-05), give way to picking parts into NEW PATH and adding the path with `a`.
> - `Add this path? … (y/n)` (the maintainer, 2026-11-09) gives way to the status line of a complete path, which names `a`, and the preview's `n` that kept the tags now keeps the path in NEW PATH.
> - A complete path needed one head, a display and a keyboard (the agent's call, 2026-11-09). A path now needs no keyboard of its own, so that a terminal with displays and no input rule can be revised and added back.
> - Deleting a head, a look or a rule in the view (the maintainer, 2026-11-04 and 2026-11-05) gives way to revising a path and clearing it, and `head rm`, `look rm` and `rule rm` stay on the command line (D8).
> - The `configured, not found` block marked with an asterisk (the maintainer, 2026-11-04) gives way to a grey part whose second line reads `Not connected`.
> - The two cards above the columns and on `?` (the maintainer, 2026-11-04 and 2026-11-05) go (the maintainer, 2026-11-10: "Drop both (Recommended)"). The paths show each route, the Settings view edits every top-level key (ADR-1201 D3), and a part's detail lists the rules that name the part, for each VT. The command that printed a route goes with them, and so does the figure that drew one.
> - `f`, which followed a line toward the heads (the maintainer, 2026-11-04), goes, since the arrow keys reach every box of a row.
> - The seven words, with tag and connect (the maintainer, 2026-11-05 and 2026-11-09), give way to the words above.
> - "any display", "any keyboard" and "any pointer" as standing parts (the maintainer, 2026-11-09) become choices inside the templates (the maintainer, 2026-11-10: "we need to rename the 'any display' or 'any pointer' or 'any keyboard' to just mean display, pointer, keyboard (and maybe font set).").

### D8: what vtium-config writes

> Edited 2026-11-05: the term was unified under the maintainer's six words (D7): this record says draft wherever it said working copy, the user's copy of vtium.conf that every edit changes. The considered entries keep the words that were said.

vtium-config wrote vtium.conf, heads.conf and frits through install bundles that `sudo vtium-config apply DIR` checks and installs (ADR-1201 D5, D6). The maintainer decided on 2026-11-04 that vtium-config keeps one draft of the configuration, which every edit changes and `install` installs, added the edit command and the commands for scripts, and accepted the rest as this record drafted it (`considered`).

#### The draft and the installed file

- **One known state, the draft** (the maintainer, 2026-11-04). vtium-config keeps the user's draft of vtium.conf in vtium-config's store, `$XDG_CONFIG_HOME/vtium/drafts/NAME.conf`, the current one named in the status line (below). Every edit, from the TUI, a command or `vtium config edit`, changes the draft. Drafts live only under the XDG path, never in `/etc`, which holds only the installed file that root manages, `/etc/vtium/vtium.conf`. vtium reads only the installed file, so nothing the user edits reaches the console until it is installed.
- **Where the draft comes from.** When there is none, vtium-config makes it as a copy of the installed file, or of the shipped example when no vtium.conf is installed. Beside it, under `$XDG_STATE_HOME/vtium/`, vtium-config keeps the SHA-256 of the installed file the draft was last made from or applied as, its base.
- **The draft may be unfinished** (the maintainer, 2026-11-04). vtium-config writes every change to the draft whatever the check says, when a command runs or when the person saves in the TUI (D7), so New can start empty and a configuration can be built in any order. The view and `vtium config flow` run `vtium check-config` on the draft and list what fails, the complete route of D3 among it. Only Install needs the check to pass, and it accepts a missing complete route once the person confirms it (below).
- **Which state is shown.** The flow view's status line and the first line of `vtium config flow` always say how the draft stands against the installed file: `draft same as installed`, `draft: 3 lines added, 1 removed`, or `draft made before the installed file changed` when the installed file's hash is no longer the base, and how many problems the check finds. When the draft holds font set changes (below), the line counts them too, as in `draft: 3 lines added, 1 removed, 2 font set changes`. A draft is the same as installed only when its text is the installed file's and it holds no font set change. `vtium config diff` prints the difference between the draft and the installed file, its font set changes included.
- **`install` installs the draft.** Install in the TUI saves first (D7). `vtium config install`, and Install in the TUI, check the draft with `vtium check-config FILE`, one run for both parts, refuse it when the installed file is no longer the base, and install it through `sudo vtium-config apply DIR`, as ADR-1201 D5 has it: owned by root with mode 0644, in one rename, keeping the old file as `vtium.conf.bak`. vtium-config builds the install bundle that `sudo vtium-config apply DIR` takes from the draft and its base, so the privileged step stays the one that `crates/vtium-config/src/apply.rs` already has. The install bundle carries the draft's font set changes too, and the privileged step writes the frits first, then vtium.conf, then removes the frits the draft removes, so vtium.conf never names a frit that is not there ("A draft's font set changes", below). After a successful install the base is the new installed file, the draft holds no font set change, and the draft and the installed file are the same. A draft whose only problem is a missing complete route (D3) installs only after the person confirms it: in the TUI by answering yes to Install's question (D7), and on the command line by answering the same question at a terminal, or with `vtium config install --force`, which asks nothing (the maintainer, 2026-11-09: "ask at a terminal no flag, with force flag (that is common, if we have one) to avoid the interactive approach ( you just have to know what you're doing because it will accept the config)"). `--force` is the word vtium-config already uses for `look rm`. It skips that one question and installs the draft, and the person answers for what it accepts. It skips none of the check's other refusals, and never the refusal of a draft made before the installed file changed. Without a terminal and without `--force`, `vtium config install` refuses with exit 1 and says how to install the draft anyway: `This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches. To install it anyway, run vtium config install --force.` The install bundle's `MANIFEST` records the confirmation or the `--force`, and `apply` then accepts that one problem from `vtium check-config` and refuses every other, as it does today (the maintainer, 2026-11-09).

> Edited 2026-11-09: the maintainer chose `--force` for the command line and kept the bundle's record of the confirmation (`considered`). The passage read: "and on the command line by answering the same question at a terminal, or with `--no-complete-path` where no terminal can ask; without either, `vtium config install` exits 1 (the agent's call). The install bundle's `MANIFEST` then records the confirmation, and `apply` accepts that one problem from `vtium check-config` and refuses every other, as it does today (the agent's call)."
- **Root works on a draft too** (the maintainer, 2026-11-04). A user who runs vtium-config as root keeps a draft under root's own XDG config path, `/root/.config/vtium/drafts/` by default, and installs it through `install` like anyone else, without sudo. ADR-1201 D5 says vtium-config writes nothing into the invoking user's home as root; that rule keeps root-owned files out of another user's home, which `sudo` can leave in `$HOME`. This record reconciles the two this way: run as root, vtium-config finds root's home from the password database, never from `$HOME` or `$XDG_CONFIG_HOME` inherited through `sudo`, so a root draft lands only in root's own home, which is root's to write.
- **Deleting a draft.** `vtium config draft rm` removes the draft, its font set folder and its base. The next edit starts a new draft from the installed file (the maintainer, 2026-11-04).
- **Editing by hand stays possible.** Root can still edit `/etc/vtium/vtium.conf` with any editor. vtium then reads it as D3 says. A draft made before that edit shows that the installed file changed since, and Install refuses it until the user reverts or merges by hand.

> Edited 2026-11-06: a draft keeps its font set changes in a folder beside it (`considered`; "A draft's font set changes", below). The bullet on install read: "It writes the frits first and vtium.conf after them, so vtium.conf never names a frit that is not there. After a successful install the base is the new installed file, and the draft and the installed file are the same." The status line counted no font set changes, `vtium config diff` showed vtium.conf alone, and deleting a draft removed "the draft and its base".

#### The writers

- **One configuration file.** vtium-config writes vtium.conf and the frits, and, under ADR-1304 once it is built, vtlooks.
- **One reading of the file for every writer.** vtium-settings reads vtium.conf into its top level and its sections, and each writer works from that reading, so they agree where the top level ends. The line editor of ADR-1201 D5 changes only the top level: a key is replaced on the top-level line vtium takes it from, never on a section's line of the same name; a commented line such as `#font-size=0` counts only at the top level; a new key goes after the last top-level line, never at the end of the file; and `--unset` comments out top-level lines alone. The section writer, which sets a head's look today (ADR-1201 D6), keeps one rule for every section it edits: it finds what it edits by its lines and changes only the lines it needs, and it never refuses an edit because the file or the result fails the check, since a draft may be unfinished. It refuses only an edit it cannot place: a section or rule that is missing, a section named twice, or text it cannot write as one line. The check guards install and `vtium check-config`.

  > Edited 2026-11-05: the steering session made the writer edit any draft, as D8's unfinished draft needs (`considered`). The bullet read: "it edits only a file vtium reads whole, changes only the lines it needs, and parses its result before handing it back."
- **What each part of the TUI writes,** all into the draft. The Flow view writes heads, looks, rules and labels through the section writer, as "What the Flow view writes" (below) says. The Settings view writes any top-level key. The Looks view writes looks, their look sections and, once ADR-1304 is built, their vtlooks. The Fonts view writes `font-paths` (ADR-1200 D25), and writes the frits it adds, replaces and removes into the draft's font set folder (below). In every view a change waits with the TUI's other unsaved changes until Save writes it into the draft, and Save, Install, Revert and New act as in the flow view (D7).

  > Edited 2026-11-10: with D7's rewrite around paths, the Flow view writes labels too, and its writes are listed below. The sentence read: "The Flow view writes rules, heads and looks through the section writer, and the common top-level keys through its base card."

> Edited 2026-11-09: the maintainer brought in Save, took Delete draft off the view's screen, and decided that Install asks, then allows a draft with no complete route (`considered`). The bullet on an unfinished draft read: "vtium-config writes every change to the draft whatever the check says, so New can start empty and a configuration can be built in any order. … Only Install needs the check to pass." The install bullet did not save first and accepted no draft that failed the check. Deleting a draft read: "`vtium config draft rm`, and Delete draft in the view after a confirmation, remove the draft, its font set folder and its base." This bullet ended: "In every view a change goes into the draft as it is made, and Install, Revert and New act as in the flow view (D7)."

#### What the Flow view writes

> Edited 2026-11-10: this subsection was added with D7's rewrite around paths (`considered`, 2026-11-09 and 2026-11-10). Until then the Flow view wrote one change at a time, from a card or from connected tags, and the placement of a new rule was stated in D7's bullet on new items.

A path in the Flow view (D7) is one head of vtium.conf and the rules that send devices to it. When the person adds, changes or revises a path, vtium-config works out the draft's sections and rules from the paths in CONFIGURATION, and writes the difference through the section writer above. It reads every rule back as a path, so the person sees in the view exactly what the file holds.

- **A path to a terminal of its own** is a `[head NAME]` section with the terminal's settings, `consoles`, `login`, `when-display-lost` and `vt`, and one rule for each part: `display MATCH -> NAME` for each display, and `input MATCH -> NAME` for each input. The path's font set and size are the head's look (D1). vtium-config sets `look =` to a look of the draft that names that frit at that size, and makes a new look named for the frit when the draft has none, such as `[look monoid-16]` with `font-name = monoid` and `font-size = 16`. A path with no font set leaves `look` out, so the head draws with the default look. When the person changes a path's font set or its size, vtium-config picks a look of the draft that names the new frit at the new size and the same `vtlook` as the head's old look. When the draft has none, it makes one, with the old look's `vtlook =` line. So the path's effects survive the change (the steering session's call, 2026-11-10).
- **Nothing (display only)** writes no input rule that names the head, and writes `isolated = yes` in the head's section, so roaming inputs stay out of it too (ADR-1401 D5). A head with no input rule and `isolated = yes` reads back as a path with Nothing (display only). A head with displays, no input rule and no `isolated` reads back as a path that only the roaming inputs reach.
- **Nowhere** writes `input MATCH -> ignore` for each input in the path and `display MATCH -> off` for each display, so the catch-all `input * -> ignore` reads back as the path Nowhere ← All input devices.
- **Whichever terminal is active** writes exactly its `input MATCH -> roaming` rules, one for each input, and nothing else. An input that two paths hold is written once, `-> roaming`, since it types into whichever terminal is active (D7).
- **The whole-kind parts** write the rules the shipped default writes for them. All displays is `display *`. All keyboards is `input class:keyboard` and `input class:key`. All mice and touchpads is one rule for each pointer class the shipped default lists: `mouse`, `touchpad`, `touchscreen`, `trackball` and `pointingstick`. All input devices is `input *`. Reading the file back gathers those rules into the same parts again.
- **Rules go most specific first.** vtium-config places every rule it writes, and the person never orders rules in the view (`v` shows the order). vtium still reads the rules in file order, top to bottom (ADR-1401 D3). vtium-config keeps the file in an order of rank, from the rule that names one device to the catch-alls, so that a device placed in a path of its own is matched before a broad part reaches it (D7). The maintainer chose the order (2026-11-10: "Port before model (Recommended)"), and gave the reason for it with a firewall's rules: "if we say 'all devices' and then also specifically say a device, the broad rules are applied after specific rules (thinking like firewall policy order)". The places of the graphics card and of a udev property in the order are the steering session's call (2026-11-10): the graphics card goes right after the port, because it is a physical place, and a udev property goes with the device type, just before the catch-alls.

  | Rank | What the rule's match names | Its keys (ADR-1401 D4) |
  |---|---|---|
  | 1 | one device, wherever it is plugged in | `serial`, `edid`, `bluetooth` |
  | 2 | a port: a USB port, or a graphics card's connector | `path`, `connector`, `connector-path` |
  | 3 | a graphics card | `card` |
  | 4 | a model | `id` |
  | 5 | a device name | `name` |
  | 6 | a device type, or a udev property | `class`, `udev` |
  | 7 | every device of its kind: the catch-all | `*` |

  vtium-config writes a new rule just before the first rule of its kind whose rank is lower, and after the last rule when there is none. The same placement serves `vtium config rule add` without `--before`.

  > Edited 2026-11-10: the maintainer chose the order of ranks, and the steering session placed the graphics card and the udev property in it (`considered`, 2026-11-10). The bullet read: "A rule that names one device, by `edid`, `connector`, `connector-path`, `serial`, `path`, `bluetooth` or `name`, goes before the first general rule of its kind, one that names a kind of device, by `class`, `id`, `card`, `udev` or `*`. A general rule goes before the first rule of its kind whose match is `*`. Any other rule, and any rule with nothing of the sort to go before, goes after the last rule (the maintainer, 2026-11-04). So a device placed in a path of its own is matched before a broad part reaches it (D7)."
- **Labels** go into a `[labels]` section after `[rules]` (D4): one line for each labelled part that has a match, `display MATCH = LABEL` or `input MATCH = LABEL`, with exactly the MATCH its rule uses. The lines follow the order of the rules, and a label for a part that no rule uses comes after them. vtium-config writes the section's comment in the style of the shipped vtium.conf, wrapped at 78 columns, saying what a label is and that vtium shows a display's label when vtium-config identifies the displays.
- **Revise and add back write nothing new.** Adding a path that Revise took out leaves the draft's text byte for byte as it was (D7). Removing a path, by clearing it after Revise, removes its head's section and its rules, and leaves its parts' labels.
- **The rules that fit no path** are rules vtium-config does not write itself, such as a rule that sends to a head that does not exist. vtium-config keeps them where they are, and D7 counts them.

#### `vtium config edit`, the way visudo edits sudoers

visudo copies `/etc/sudoers` to a temporary file, opens it in the user's editor, checks the copy's syntax when the editor exits, and installs it only when it parses, asking whether to edit again otherwise; `crontab -e` and `systemctl edit` work the same way. `vtium config edit` brings that to vtium.conf, for a user who prefers vim or any other editor to the TUI:

1. It copies the draft to a temporary file under `$XDG_STATE_HOME/vtium/`, and opens it in the editor `$VISUAL` names, else `$EDITOR`, else `vi`.
2. When the editor exits with the copy unchanged, it says so and changes nothing.
3. Otherwise the copy becomes the draft, and vtium-config runs `vtium check-config` on it. When the check fails, with an error or a warning, it prints each problem with its line and asks whether to edit again. Notes, which do not fail the check, are printed and do not ask.
4. With `--install`, it then installs the draft as `install` does, which installs it only when the check passes. Unlike visudo, it offers no way to install a draft that fails.

> Edited 2026-11-04: the maintainer allowed deleting heads and looks in the TUI and from the command line, for the full lifecycle (`considered`). This read: "Removing a whole head or a profile is done here, and has no command of its own (the maintainer, 2026-11-04)." `vtium config edit` stays the way to edit the whole file; `head rm` and `look rm` delete one item, and the profile commands are named `look` to fit the Looks view.

#### Commands for scripts

Everything the TUI does works without it, so it can be scripted (the maintainer, 2026-11-04). Each change the flow view makes is also a `vtium config` command, and the view and the commands call one set of operations in vtium-config, each of which takes vtium.conf's text and gives back the edited text through the writers above. Every command that changes the configuration reads the draft and the devices present afresh, makes its change, writes the draft, and prints what `vtium check-config` finds in the result. With `--install` it then installs the draft, as `vtium config install` does, which installs it only when the check passes. A command asks nothing but sudo's password, and that only when it installs. A rule's `N` is its number in `[rules]`, counted from 1, as `vtium config flow` prints it.

| Command | What it does |
|---|---|
| `vtium config flow [--json]` | Prints the flow of the draft with the devices present placed on it, as the view draws it, under a first line that says which state it shows (above): as text, or as JSON whose top level carries `"version": 1` and the state, so a script can rely on its fields |
| `vtium config diff` | Prints the difference between the draft and the installed file, then each font set change the draft holds (below) |
| `vtium config install [--force]` | Installs the draft, as above; with `--force` it installs a draft whose only problem is a missing complete route without asking (edited 2026-11-09; the row read "`vtium config install` \| Installs the draft, as above") |
| `vtium config set KEY VALUE`, or `set --unset KEY` | Sets a top-level key, as the Settings view does, or leaves it at its default, through the line editor (edited 2026-11-10: the row named "the base card's (D7)") |
| `vtium config get KEY` | Prints a top-level key's value in the draft, and its default when the draft leaves it unset; it changes nothing (the maintainer, 2026-11-05) |
| `vtium config draft rm` | Removes the draft and its font set folder; the next edit starts a new one from the installed file |
| `vtium config revert` | Throws away the draft's changes, its font set changes among them, and copies the installed file back over it; the installed file is untouched |
| `vtium config new` | Empties the draft to the top level at its defaults and one look, `[look default]`, as New does (D7), and throws away its font set changes |
| `vtium config rule add [--before N] 'KIND SELECTOR -> TARGET'` | Adds a rule, such as `'input path:pci-0000:15:00.0-usb-0:3.3 -> desk'`, before rule N, or where "What the Flow view writes" places it (edited 2026-11-10: the row read "where the view's `new rule…` puts it (D7)") |
| `vtium config rule set N 'KIND SELECTOR -> TARGET'`, or `rule set N [--selector S] [--target T]` | Replaces rule N, or changes its selector or its target |
| `vtium config rule move N --before M`, or `rule move N --last` | Moves rule N, since a rule's place decides what it matches first |
| `vtium config rule rm N` | Removes rule N, the only way, with the view's remove, that a rule leaves the file |
| `vtium config head add NAME [--look L]` | Adds a `[head NAME]` section, with the look L, or the default look when `--look` is left out |
| `vtium config head set NAME [--name NEW] [--look L] [--consoles N] [--login CMD] [--when-display-lost V] [--isolated yes\|no]` | Changes a head's keys. `--name` renames the head and every rule that names it, in one change |
| `vtium config head active NAME` | Asks the vtium running on this VT to make NAME the active head, as the roaming chord does (ADR-1401 D5). It changes no file. It sends `head active NAME` over vtium's control socket, `/run/vtium/<tty>/control`, which takes it from root, from the user vtium runs as and from a user logged in on one of that vtium's consoles (ADR-1401 D12), and it shows vtium's answer as a sentence in the user's language. No head key sets the head that is active at start (the maintainer, 2026-11-04) |
| `vtium config look add NAME [--frit F] [--size PT]` | Adds a `[look NAME]` section with its `font-name` and `font-size`; a key left out is inherited from the top level (D5) |
| `vtium config look set NAME [--name NEW] [--frit F] [--size PT] [--engine E] [--hinting H] [--line-spacing PCT]` | Changes a look's keys: `--frit` sets its `font-name`, `--size` its `font-size`, and so on through the keys D5 lists. `--name` renames the look and every head that names it |
| `vtium config look rm NAME [--force]` | Deletes a look. While a head uses it, it refuses and names the heads, exit 2; with `--force` it sets those heads to the default look |
| `vtium config head rm NAME` | Deletes a head. The rules that send to it stay, broken, and the command names them; the check lists them until each is pointed elsewhere or deleted |
| `vtium config edit [--install]` | Edits the whole draft in the user's editor, as above, and stays the way to edit the whole file |

> Edited 2026-11-09: the maintainer decided who may use vtium's control socket, "Root + console users", and that vtium answers with codes that vtium-config shows as translated sentences, "Codes, translated sentences" (ADR-1401 D12 and its `considered`, 2026-11-09). The `head active` row read: "It changes no file; it needs the control socket under `/run/vtium/<tty>/` that ADR-1401 D12 plans, and comes with it."

Each command that changes the configuration also takes `--dry-run`, which prints the change as a diff and the check's verdict and writes nothing; for `revert` and `new` it prints what would be lost, each font set change among it. A command asks no confirmation: running it is the confirmation, and `install`, `revert` and `new` act at once. The exit codes are fixed, so a script can tell the outcomes apart:

| Exit code | Meaning |
|---|---|
| 0 | Done: the draft is written, whatever the check finds, which the command prints; or the flow or the difference is printed; or the draft is installed. `vtium config flow` exits 0 whenever it prints, whatever problems it lists; `vtium check-config` is the command whose status judges a file |
| 1 | The check failed where it must pass: `install`, or a command's `--install`, found an error or a warning, each printed with its line, and nothing is installed; the draft keeps the change |
| 2 | The command could not be used: an unknown option, a rule number or a head or look that does not exist, or a value its key cannot take |
| 3 | The installed vtium.conf changed since the draft was made from it, so nothing is installed; the draft keeps the change, and `diff` and `revert` show the way on |
| 4 | Nothing could be installed without privilege: sudo was refused or is missing. The draft keeps the change, and the command prints how to install it |
| 5 | A file could not be read or written, and the message names its path (the maintainer, 2026-11-05) |

#### vtium-config's store: drafts and the library

vtium-config keeps a store of its own under the user's XDG paths, which holds the drafts and a library of nodes the user keeps outside any draft (the maintainer, 2026-11-05):

| Path | What it holds |
|---|---|
| `$XDG_CONFIG_HOME/vtium/drafts/NAME.conf` | Each draft, a whole vtium.conf, named by the user. vtium-config offers the machine's hostname as the name, with `-2`, `-3` and so on appended when that name exists, and never overwrites a draft; the first draft, which vtium-config makes on its own from the installed file, takes that name too |
| `$XDG_CONFIG_HOME/vtium/drafts/NAME.frits/` | Each draft's font set changes: `FRIT.vtfrit` for a frit the draft adds or replaces, and an empty `FRIT.removed` for an installed frit it removes ("A draft's font set changes", below) |
| `$XDG_CONFIG_HOME/vtium/library.conf` | The library: looks, heads and rules the user saved from a draft, written in vtium.conf's section syntax, `[look NAME]`, `[head NAME]` and a `[rules]` list |
| `$XDG_CONFIG_HOME/vtium/vtium-config.conf` | The tool's own settings, `key = value`, among them `draft = NAME`, the current draft |
| `$XDG_CONFIG_HOME/vtium/installed/DRAFT-N.conf` | The installed history: each install saves the file it replaces here, numbered from 1 for each draft, and vtium-config keeps the 10 newest, a count the tool's settings change with `history = N` (the maintainer, 2026-11-05) |
| `$XDG_STATE_HOME/vtium/` | Each draft's base, the hash of the installed file it was made from or applied as |

`XDG_CONFIG_HOME` is `~/.config` and `XDG_STATE_HOME` is `~/.local/state` when unset. Root's store lives under root's own home, found from the password database (above).

- **A draft is a complete vtium.conf,** and can be carried between machines (the maintainer, 2026-11-05). A configuration built in vtium-config on one machine is copied to another, a small board among them, and installed there with `install`. Where the draft's devices are absent, as on the machine that built it, `vtium check-config --no-devices` checks it by its rules alone (D3).
- **One draft is current.** Every edit, from the TUI, a command or `vtium config edit`, changes the current draft, and the Flow view's status line and the first line of `vtium config flow` name it. `install` installs the current draft.
- **Nodes move between a draft and the library.** Saving a look, a head or a rule into the library copies it there under its name; taking one from the library copies it into the current draft, where it is checked as any change is.

| Command | What it does |
|---|---|
| `vtium config draft list` | Lists the drafts, marking the current one, each with its state against the installed file |
| `vtium config draft new [NAME] [--from installed\|empty\|DRAFT]` | Makes a draft, from the installed file unless told otherwise, and makes it current. From another draft it copies that draft's font set folder too. Without a name it takes the hostname, with an increment when that name exists (the maintainer, 2026-11-05) |
| `vtium config draft switch NAME` | Makes another draft current |
| `vtium config draft rm [NAME]` | Deletes a draft and its font set folder, the current one when no name is given; the next edit starts a new draft from the installed file, named as above, when none is left |
| `vtium config history` | Lists the installed history, newest first, each file with its number, its draft and when it was replaced |
| `vtium config restore [N]` | Copies installed-history file N, the newest when no number is given, into the current draft, where the check runs and the user installs it as usual |
| `vtium config library list` | Lists the library's looks, heads and rules |
| `vtium config library add look\|head NAME`, or `library add rule N` | Copies a look, a head or rule N of the current draft into the library |
| `vtium config library take look\|head NAME`, or `library take rule NAME` | Copies a node from the library into the current draft |
| `vtium config library rm look\|head\|rule NAME` | Deletes a node from the library; drafts are untouched |

The maintainer settled the store's three questions on 2026-11-05 (`considered`):

1. **Copied, not linked.** A node is copied when it moves, either way. A card marks a draft's copy that differs from its library node, and the user can push the change into the library or pull the library's version into the draft.
2. **No draft per machine.** The user names drafts; vtium-config offers the hostname, with an increment so nothing is overwritten (above). A draft whose rules name devices not present shows them as configured, not found (D7).
3. **The format.** The library is written in vtium.conf's section syntax, so a node reads the same in the library and in a draft and one parser reads both; the tool's settings are `key = value` lines.

#### A draft's font set changes

The maintainer decided on 2026-11-06 that a draft keeps its font set changes (`considered`). A font set change is a frit (a font set file, `NAME.vtfrit`) that the draft adds or replaces, or an installed frit that the draft removes. vtium.conf only names frits by their names. Their files live in the frits' directory, `/etc/vtium/fonts` unless `font-frits` names another (ADR-1200 D1), so a draft's vtium.conf cannot hold them, and vtium-config keeps them in a folder beside the draft.

- **The folder.** The draft `NAME.conf` keeps its font set changes in `$XDG_CONFIG_HOME/vtium/drafts/NAME.frits/`. A frit the draft adds or replaces is there as `FRIT.vtfrit`, with its whole text. A frit the draft removes is there as an empty file, `FRIT.removed`. Each change is one file, so adding or removing a frit writes or deletes one file and never rewrites a shared list, and listing the folder shows the changes. The folder never holds both files for one frit: writing `FRIT.vtfrit` deletes `FRIT.removed`, and writing `FRIT.removed` deletes `FRIT.vtfrit`. vtium-config makes the folder with the draft's first font set change and deletes it when it is empty, so a draft with no font set change has no folder. vtium-config reads only regular files whose names are a frit's name followed by one of those two endings, each no larger than the largest frit the glyph helper reads, and the check lists anything else in the folder as a problem, which Install refuses.
- **The frits a draft sees.** Every command that reads the draft reads its folder too. The frits a draft sees are the installed frits, plus each `FRIT.vtfrit` in the folder, minus each `FRIT.removed`. The Fonts view lists them, marking each frit the draft adds, replaces or removes, and a look card offers them for `font-name`. A `FRIT.vtfrit` whose text is the installed frit's is no change: the view does not mark it, `diff` does not print it, and `install` does not write it.
- **The check sees the same frits.** The check warns when a look's `font-name` names a frit that is not installed (ADR-1401 D7). vtium-config passes `vtium check-config` one `--with-frit FRIT` for each frit the folder adds or replaces and one `--without-frit FRIT` for each frit it removes. The console's check counts a frit as installed when the frits' directory holds it or a `--with-frit` names it, and not when a `--without-frit` names it. A look that names a frit the draft adds therefore passes, and a look that names a frit the draft removes gets the warning, which Install refuses. vtium-config also reads each frit in the folder with the frit reader, strictly, as the glyph helper reads it, and the check lists each frit the reader refuses, by its path, as a problem.
- **Every font set change is kept at once.** vtium-config writes a font set change into the folder the moment it is made, whatever the check says, as it writes every change to a draft. In the TUI, the Fonts view's new frit writes `FRIT.vtfrit`. `r` on a frit only the draft adds deletes its `FRIT.vtfrit`, `r` on an installed frit writes `FRIT.removed`, and `r` on a removed frit takes the removal back. On the command line, `new-frit` and `remove-frit` write the current draft's folder and install nothing, as ADR-1201 D2 has every command that writes do, and with `--install` they then install the draft as `install` does. A removal is written even when the draft's looks name the frit, and the check then lists each of those looks (the maintainer, 2026-11-06). vtium-config refuses only a change it cannot make: a built-in font's name, a name a frit cannot have, a frit the frit reader refuses, and the removal of a frit that is neither installed nor in the folder.
- **Quitting loses nothing.** The TUI keeps no font set change in memory, since each is in the folder, so `q` and Ctrl-C leave at once, and the TUI no longer asks whether to install or lose new font sets before it leaves. The status line still counts the font set changes that wait to be installed.
- **The live trial.** The live trial of ADR-1201 D4 asks the console to switch to a frit by name, and the console reads frits only from the frits' directory. `t` therefore tries an installed frit, and on a frit that only the draft holds the TUI says that the font set can be tried once it is installed.

Each draft command, and each action of the TUI that does the same, does this with the folder:

| Command | What it does with the folder |
|---|---|
| `draft new NAME`, with `--from installed` or `--from empty` | Makes a draft with no folder |
| `draft new NAME --from DRAFT` | Copies DRAFT's folder with its file |
| `draft switch NAME` | Makes NAME's folder the one the Fonts view, the check and `install` read |
| `draft rm [NAME]` | Deletes the folder with the draft and its base |
| `revert` and Revert | Deletes the folder: the draft becomes the installed file, which holds no font set change |
| `new` and New | Deletes the folder too, since New starts the draft over (the maintainer, 2026-11-06) |
| `restore [N]` | Leaves the folder as it is, since the installed history holds vtium.conf alone |
| `diff` | After vtium.conf's difference, prints each font set change under the frit's path in the frits' directory: a frit the folder adds as added lines, a frit it replaces as its difference from the installed frit, and a frit it removes as removed lines |
| `flow` | Counts the font set changes in its first line, as above, and with `--json` lists them under `"frits"`, each a name with `"add"`, `"replace"` or `"remove"` |
| `install` and Install | Installs the folder's changes with vtium.conf, below |
| `edit`, `library`, `history`, `set`, `get` and the `rule`, `head` and `look` commands | Leave the folder as it is |

vtium-config has no command that renames, exports or imports a draft. `draft new NAME --from DRAFT` is the copy. `--dry-run` on `revert` and `new`, and the question Revert and New ask in the TUI, name each font set change that would be lost.

- **Carrying a draft to another machine.** A user carries a draft by copying its file and its folder, `NAME.conf` and `NAME.frits/`, into the other machine's drafts directory. There, a removal of a frit that machine does not have removes nothing, and `install` notes it. A frit in the folder with the text that machine already has is no change. A draft copied without its folder keeps its vtium.conf, and the check warns about each look whose frit that machine does not have, so Install refuses the draft and never installs a look with no font set.
- **What `install` does.** `install` checks the draft with the folder's changes as above. It then builds the install bundle of ADR-1201 D5 from the draft and the folder: vtium.conf, each frit the folder adds or replaces whose text differs from the installed frit, and a removal for each removed frit that is installed. `apply DIR` checks every file before it writes any, and its run of `vtium check-config` passes `--with-frit` and `--without-frit` for the bundle's frits, so its verdict is the draft's. It writes in this order: the frits the draft adds or replaces, into the frits' directory the installed vtium.conf names; then vtium.conf; then the removals, each frit renamed to `FRIT.vtfrit.bak`, as `apply DIR` keeps every file it replaces. A failure stops at the file that failed, and every stop leaves a working system. Before vtium.conf is written, the installed vtium.conf names only frits that are still there, since nothing has been removed yet. Once it is written, the new vtium.conf names only frits already in place, and a removal that fails leaves behind a frit that nothing names. A draft that changes `font-frits` and also holds font set changes is refused at install, as `apply DIR` refuses it, until the move of the frits' directory is installed on its own (ADR-1201 D5).
- **After the privileged step,** whether it finished or stopped, vtium-config compares the system with the draft and records what is now true. When the installed vtium.conf is the draft's text, it keeps the file that was replaced in the installed history and makes the draft's base the new installed file. Each `FRIT.vtfrit` whose text the frits' directory now holds, and each `FRIT.removed` whose frit is gone, leaves the folder. A finished install leaves the draft the same as the installed file, with no folder. A stopped install leaves the draft holding what is still to do, and running `install` again does only that. The installed history keeps vtium.conf alone; a frit that an install replaced or removed is kept beside its path as `FRIT.vtfrit.bak`.
- **No base for a frit** (the maintainer, 2026-11-06). vtium-config records no hash of an installed frit when the draft changes it, so it does not notice when root edits that frit by hand afterwards. `diff` shows the draft's frit against the frit installed now, before an install, and the install keeps the frit it replaces as `FRIT.vtfrit.bak`. vtium.conf keeps its base and the refusal that comes with it (above).
- **The hand conversion of D6 does not change.** The two installs that have heads.conf or a seat file keep the frits already in their frits' directory, which no draft needs to list, and a converted vtium.conf copied in as a draft installs with `install` like any other draft.

> Edited 2026-11-06: the maintainer decided that a draft keeps its font set changes (`considered`), and this section was added. Until then no record said where a font set change waits before it is installed; the TUI held new frits and removals in memory until Install, and lost them, after a question, when the user left.

> Edited 2026-11-05: the maintainer gave vtium-config a store of its own, with named drafts and a library of nodes (`considered`). The draft was one file, `$XDG_CONFIG_HOME/vtium/vtium.conf`.

> Edited 2026-11-04: the maintainer added the edit command, the commands for scripts and the working copy, and accepted the rest (`considered`). D8 was headed "what vtium-config writes (open)" and had none of them; until the working copy, each change went into a new draft under `$XDG_STATE_HOME/vtium/drafts/`, and each command and `vtium config edit` installed its change at once. The earlier text is in git at e5308782.

### D9: what the code changes to fit

These follow from D1 to D8 and the tree:

- vtium.conf's reader stops at the first section heading and hands the rest to vtium-settings's head-file reading, through one function that splits the file, so the console, `vtium check-config` and vtium-config agree where the top level ends.
- The trust check vtium-settings makes of heads.conf applies to vtium.conf, with the reasons and the mending commands of D3.
- heads.conf's reading goes from the console's start and from `vtium check-config`, and `install` no longer takes heads.conf in a draft.
- The seat file's code goes: the seat's path in `files`, the seat's name check and its report in `crates/vtium/src/config/mod.rs`, the seat's argument to `load` in `main.rs`, and `SEAT` in `crates/vtium-config/src/switch.rs`.
- The mapping from a listed device to the identity the rules match moves out of the console binary into a crate that vtium and vtium-config both link, so the flow view and `vtium check-config` place each device by one function. The work that builds the view chooses the crate.
- The TUI gains color, through the sixteen-color SGR codes alone, and reads `NO_COLOR`. Its input decoder binds nothing to a lone ESC today, since a lone ESC cannot be told from the start of a sequence without waiting; for the flow view's Esc it reads an ESC as the key when it is the last byte of a read, since a terminal writes each sequence it sends whole, and no timer is involved. It decodes no mouse input today, so its input decoder (`crates/vtium-config/src/tui/input.rs`) gains SGR mouse reports for the flow view.
- vtium-config's TUI watches device events through vtium-platform's udev monitor (`crates/vtium-platform/src/udev.rs`, `Udev`), the console's own, on the event loop it already runs.
- vtium-config gains the commands of D8 and `flow`, over one set of operations that the view calls too, and a JSON form of the flow with a version field.
- vtium-config keeps the draft and its base's hash under the user's XDG directories, and `install` builds the install bundle that `sudo vtium-config apply DIR` takes (`crates/vtium-config/src/apply.rs`) from the draft, so the privileged step is unchanged. The TUI's Install action shows the difference between the draft and the installed file and installs it, in place of a new install bundle for each save.
- New rules are placed by the rank of what they name (D8), with one function that the Flow view and `rule add` share.
- vtium-config's TUI gains the Flow view of D7, with the read-only file view on `v` and Save; the draft's path comes from the XDG variables for a user and from the password database for root.

> Edited 2026-11-10: the maintainer dropped the Flow view's two cards and took Delete draft off the screen (D7, `considered`, 2026-11-09 and 2026-11-10). The two bullets read: "New rules are placed by what they name, with one function the view, connecting and `rule add` share." and "vtium-config's TUI gains the read-only pager of D7, the base card over the key table, and Delete draft".
- The complete-route rule of D3 is one function in vtium-settings, which takes the rules and either the devices present or none, so `vtium check-config`, its `--no-devices` reading and vtium-config judge a file alike; the console's start does not call it.
- The example vtium.conf is generated from the key table and the sections template together, with the size test of D3.
- The ten goldens that show heads.conf change in a commit of their own, which the maintainer reviews in the golden pass (the maintainer, 2026-11-05; AGENTS.md's rule for goldens).
- vtium-config keeps each draft's font set changes in its folder, `drafts/NAME.frits/`, and reads them with the draft (D8, "A draft's font set changes"). The TUI's list of font set changes held in memory goes, and with it the question it asked before leaving. `new-frit` and `remove-frit` write the current draft's folder and take `--install`. `vtium check-config` takes `--with-frit` and `--without-frit`, and both vtium-config's check of a draft and `apply DIR`'s check pass them. `install` builds the install bundle from the draft and its folder, and after the privileged step records what the system now matches, whether the step finished or stopped.

> Edited 2026-11-06: the bullet on the draft's font set folder was added when the maintainer decided that a draft keeps its font set changes (`considered`).

## The maintainer's answers

The maintainer answered the draft's open questions on 2026-11-04 (`considered`):

1. **The syntax (D4):** sections with names, with a `[look NAME]` section for each look and one `[rules]` list after the heads, tried top to bottom across every head.
2. **Two levels (D5):** as drafted: a head's section over its look over the top level, with a fixed list of keys for each section kind.
3. **Migration (D6):** none; the two installs that have heads.conf or a seat file are converted by hand.
4. **The view (D7):** as drafted, navigable by keys and mouse, with a minimal status line, editing through the checked install from the first build, and hotplug followed live, with `d` for a full re-read. The maintainer later replaced its columns with paths built from parts (2026-11-09 and 2026-11-10), and D7's notes on what the new design replaces name each part of this answer that went.
5. **Writes (D8):** as drafted, with a valid file needing one complete route (D3), with `vtium config edit` added, a command for every change the view makes, `head add` and `look add` among them, with fixed exit codes, `head rm` and `look rm` (the maintainer reversed the earlier ruling that only `edit` removes them), `head active` as a request to the running vtium, every edit going into the user's draft under `~/.config/vtium/drafts/`, root's under root's own XDG path, unfinished if need be, with only `install` needing a passing check, `set` for top-level keys and `draft rm`, and vtium refusing an untrusted vtium.conf with its reasons (D3).

> Edited 2026-11-10: item 4 follows D7's rewrite around paths. It read: "**The view (D7):** as drafted, then navigable by keys and mouse (Left and Right across the columns, Up and Down within one, Tab and Shift-Tab between the views, Enter to edit, Space to tag rules, devices, heads and looks, `c` to connect), with View, Install, Revert, New and Delete draft, each confirmed but View, a base card for the top-level keys, a minimal status line, editing through the checked install from the first build, with objects drawn as blocks of color, hotplug followed live, with `d` for a full re-read, a device that is configured but not found kept on the flow until its rule is deleted, `new…` items at the foot of the rules, heads and look columns, so the whole configuration can be built in the view, and tagging with Space and connecting the tags with `c`."

## Passages this record changes

Once the maintainer accepts this record, each passage below is edited in place with a dated note that cites ADR-1403:

- ADR-1401 D3: the head file is the sections of `/etc/vtium/vtium.conf`, and its top-level keys sit among vtium's keys. D2: a later seat's flow goes into vtium.conf, not a file of its own. D6: the template is the sections of the example vtium.conf. D7: the trust check covers vtium.conf and says why it refuses one, and "the whole file" means the sections and the policy. D12: `vtiumctl template` and `install` write vtium.conf.
- ADR-1200 D16: a look is a `[look NAME]` section of vtium.conf.
- ADR-1201 D2: the commands of D8 and `flow`. D3: the Flow tab, and Install, Revert and New in every view. D5: the draft in place of an install bundle for each save, `install` building its install bundle from the draft, the line editor's top level, and the check every change goes through. D6: the section writer edits vtium.conf. D2 and D9 also: `new-frit` and `remove-frit` write the current draft's font set folder (D8), and the new frit ends in that folder, not in an install bundle of its own.
- ADR-1500 D4: the rows for the seat file and for heads.conf, and the shared-head-file probe and answer, which now concern vtium.conf.
- ADR-1304 D4 and D5, accepted: a vtlook uses vtium.conf's section syntax, and a head's `look` is set in its section of vtium.conf; its author changes them before acceptance.
- docs/config.md, docs/configure.md, docs/head-file.md, docs/heads.md and the files under docs/heads/, the READMEs of the vtium, vtium-config and vtium-settings crates, and the configuration example and template.

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). The ADR-1401 D12 passage above names `vtiumctl template` and `install`; ADR-1401 D12 now gives those jobs to the example vtium.conf a package installs and to `vtium config install`.

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
- Picking parts into a new path and pressing `a` makes the head, look and rule changes the path describes, and Revise brings a path back unchanged (edited 2026-11-10: the bullet read "Tagging a device, a head and a look and pressing `c` makes the rule and look changes they describe, shown as commands before they are applied.").
- The view follows hotplug while it is open, so a user plugs a keyboard in and sees at once which rule takes it, and keeps their place on screen while devices come and go.
- Install, Revert, Restore and New each ask first and count what changes, and only Install touches the installed file. A configuration can be built in any order, since the draft may be unfinished, and View shows it as text with each problem marked.
- `vtium config edit` lets a user edit the whole configuration in their own editor with the same check the TUI's changes get.
- One draft holds every change the user makes, whichever tool made it, and the view and `vtium config flow` say how it stands against the installed file, so the user always knows which configuration they are looking at and what `install` will install.
- The seat file's second layer of the same keys goes, so a key has one place at each level.

### Negative

- Deleting a head leaves its rules broken, and the draft fails the check until each is pointed elsewhere or deleted; deleting a look moves its heads to the default look. The question before each delete lists what it leaves broken or changes.
- A user who is not root has two files to keep in mind, the draft and the installed file, and a change made directly to the installed file, as by root or a package, makes `install` refuse the draft until it is reset or merged by hand.
- `vtium check-config` refuses a file the console would still run, since the console does not apply the complete-route rule at start. A file written for another machine has to be checked with `--no-devices` where its devices are absent.
- One file mixes a forgiving reading at the top level with a strict one in the sections, and a reader has to know which part a line is in. The flow view and `vtium check-config` say which.
- A user who runs vtium with a vtium.conf root does not own loses that file's settings until it is root's.
- A heads.conf or seat file left in place stops working without a word. The maintainer converts the two installs that have one by hand.
- While two builds share `/etc/vtium/vtium.conf`, a build older than this change misreads a section's `login` and a look's font lines.
- The view watches device events while it is open, so vtium-config gains an event source of its own, and what it shows can move while the user reads it; the selection and the opened component stay put, and the status line names each change.
- The flow view brings color into vtium-config, which needs a rule for terminals without color and one screen for a terminal below its minimum.

### Neutral

- Frits, vtlooks, shaders and fonts stay files of their own, which the flow names and the view draws.
- `vtiumctl`, not built yet, writes vtium.conf where ADR-1401 D12 has it write heads.conf.
- The flow view edits rules, heads and looks, which overlaps what ADR-1401 D12 plans for `vtiumctl setup`; both write through the same section writer and the same check, and a change applies at vtium's next reading of its configuration until the trial of ADR-1401 D12 is built.

> Edited 2026-11-07: the maintainer decided that vtium-config takes over the job ADR-1401 D12 planned for `vtiumctl`, a separate binary, and vtium does not build `vtiumctl` (ADR-1401 D12). vtium-config writes vtium.conf for every job of ADR-1401 D12, and the flow view and ADR-1401 D12's setup are one tool, so the overlap the second bullet names is gone. The trial of ADR-1401 D12 is staged by `vtium config install` once it is built.

## Alternatives Considered

- **Blocks in braces, as sway, i3, Hyprland and nginx write them.** sway is the closest program to vtium, since it matches displays and input devices by a selector at the head of each block, as in `output eDP-1 { mode 1920x1080 }` and `input type:keyboard { xkb_layout us }`. vtium's form would write `head desk { look big { font-name = monoid } }` and a `rules { ... }` block. It writes the nesting most literally, and it costs a new parser, a missing brace that swallows every line after it so that ADR-1401 D7 has no single line to drop, and a second way to write a look two heads share. sway also applies every block whose selector matches a device and merges their settings, where vtium places a device by the first rule that matches, so the rules would stay one ordered block in any case. The maintainer chose sections with names (`considered`).
- **Dotted flat keys,** as sysctl writes them, such as `head.desk.look=big` and `rule=display * -> desk`. Every line stays a flat key, which an older vtium ignores as unknown, but the structure is invisible: a head is spread over lines that only share a prefix, and `rule` would keep every value it is given, where every other key keeps its last.
- **TOML or YAML for the whole file.** TOML nests with existing parsers, but a text value must be quoted, as `xkb-layout = "us"`, and every key needs a value, so a switch alone on its line, such as `hwaccel` or `mouse`, is invalid, and a kmscon.conf would not parse. YAML fails the same way and makes indentation carry meaning. vtium's parsers are also written to bound untrusted input (ADR-1401 D10).
- **A migration:** vtium reading heads.conf with a warning until the first beta, and `vtium config fold` moving heads.conf and the seat file's keys into vtium.conf. The only two installs that need it are the maintainer's, converted by hand, so the maintainer chose no migration (D6).
- **Delete a head or a look only with `vtium config edit`.** It keeps the view from removing what other parts of the flow still use. The maintainer chose the full lifecycle in the TUI instead, since the TUI is how people are likely to edit the configuration, and without delete they would edit the file by hand (`considered`, 2026-11-04).
- **Keep heads.conf beside vtium.conf, and draw the flow from both files.** The view could join two files, and nothing would move. The maintainer decided on one file, and two files keep the inheritance split from what it inherits.
- **Keep the seat file.** It is a second layer of the same keys, which no decision chose, and vtium serves seat0 alone. The maintainer removed it.
- **A new file for the flow, such as `flow.conf`, with vtium.conf left as it is.** It keeps kmscon.conf untouched, and it keeps the split the maintainer decided to end.
