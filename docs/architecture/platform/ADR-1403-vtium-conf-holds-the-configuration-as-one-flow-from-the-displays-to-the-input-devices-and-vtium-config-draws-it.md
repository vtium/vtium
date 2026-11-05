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
status: proposed
date: 2026-11-04
deciders:
  - aaronsb
related: []
---

# ADR-1403: vtium.conf holds the configuration as one flow from the displays to the input devices, and vtium-config draws it

## Summary

- **Decided:** The maintainer decided on 2026-11-04 that vtium's configuration is one flow, from the displays to the input devices: the displays, the display rules that send each display to a head, the heads, each head's look (its profile, the frit the profile names, and the fonts that frit draws with), the input rules that send each input device to a head, and the input devices. The whole flow lives in one file, `/etc/vtium/vtium.conf`. Its flat `key=value` lines stay, and they are the defaults every head inherits, so a kmscon.conf copied in still reads. The head file's sections, today in `/etc/vtium/heads.conf`, move into vtium.conf below those lines. vtium stops reading the seat file, `seat0.seat.conf`, which is a departure from the C reference. vtium-config gains a view that draws the flow as six columns, each in its own color, joined by lines. Its first version only reads: it places the machine's real displays and input devices on the file's flow with the matching `vtium check-config` already does, and marks each rule that matches nothing. This record's agent adds what the move forces (D3): vtium reads vtium.conf only when root owns it and no one else can write it, as it reads heads.conf today; the lines above the first section keep vtium.conf's forgiving reading, and the sections keep the head file's strict one, so an error in a section never drops a top-level key. Five questions remain for the maintainer, each with a recommendation: the syntax, how a key set at two levels combines, the migration, the view, and vtium-config's writes ([Open questions](#open-questions)).
- **Trades away:** One file holds two ways of handling a mistake, a forgiving one for the flat keys and a strict one for the sections. An installed heads.conf has to move. A vtium.conf that root does not own, which vtium reads today, is no longer read at all. With the recommended syntax, while the packaged build and the development build share `/etc/vtium/vtium.conf` (ADR-1500 D4), a build older than this change reads a head section's `login` line and a profile section's font lines as if they were top-level keys. The view's columns need a terminal at least 80 columns wide, and a narrower one, such as a minspec board's 40-column console, shows the flow as an indented list instead.
- **One-way?** No, but expensive once a release ships it: users then write their files in the merged form, and a later change of syntax means a second migration. Until that release every part of this changes in a session.
- **Probes:**
  - *Not confident (nested objects):* the recommendation keeps heads.conf's bracketed sections as they are, placed below vtium.conf's flat lines, and adds a `[profile NAME]` section for each look, so a head names its profile as it does today. The rules stay one ordered list in `[rules]`, outside the heads, because first match depends on their order across all heads. Is that the nesting you meant, or did you mean each head's rules written inside that head?
  - *Confident (no setup lost):* an installed heads.conf keeps working, with a warning at each start, until `vtium config fold` moves it into vtium.conf or the first beta release stops reading it. An installed `seat0.seat.conf` stops being read with this change; vtium says so at each start, and the same command folds its keys into vtium.conf.
- **Inversion:** Between "a file for each concern", the C reference's kmscon.conf with a seat file beside it, as vtium has today with heads.conf and planned profile files, and "one document that holds everything the console reads". vtium takes the second for the flow of devices, heads and looks, and keeps frits, vtlooks and shaders as files of their own, because the glyph helper and the renderer read them and vtium-config manages them one by one.

## Context

vtium's configuration is spread over three files today. vtium.conf holds the keys the C reference's kmscon.conf had, plus vtium's own. `seat0.seat.conf`, beside it, holds the same keys and wins over vtium.conf, because the C reference read a file for each seat over its main file and vtium carried that over without a decision. heads.conf holds the heads and the rules that place each display and each input device (ADR-1401 D3), and profiles were to get files of their own (ADR-1200 D16). A head inherits its font from vtium.conf, its frit from its profile, and its devices from heads.conf's rules, so no single file shows how a display and a keyboard end up in a terminal with a font.

The tree constrains the move in four ways, each read from the code on 2026-11-04 (`basis`). vtium reads heads.conf only when root owns it and no one else can write it, and it reads vtium.conf with no such check. The two files handle mistakes differently: vtium.conf reports an unknown key or a bad value and goes on, keeping the last value a key is given, while heads.conf drops what each error takes under ADR-1401 D7's three levels and treats a key given twice as an error. vtium.conf's line editor adds a new key at the end of the file, which in a file with sections lands inside the last section. And vtium-config draws its screens with reverse video alone, so the flow view brings the first color into it.

## Decision

D1 to D3 state the maintainer's decisions and what the move forces. D4 to D8 hold the open questions, each with the options and this record's recommendation, which the maintainer answers before the record is accepted. D9 lists what the code changes to fit.

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

- vtium reads `vtium.conf` in its configuration directory and then its command line, and nothing else, apart from heads.conf during the migration (D6).
- vtium serves seat0 alone (ADR-1401 D2), so the only seat file it read was seat0's. When the later extension for other seats is built, a seat's flow goes into vtium.conf with the rest, and ADR-1401 D2's sentence that the head file gains a file for each seat no longer holds.
- What vtium does with an installed seat file is part of the migration (D6).

This is a departure from the C reference, which read `<configdir>/<seat>.seat.conf` over kmscon.conf for each seat (C notes).

### D3: one file, vtium.conf

The maintainer decided on 2026-11-04 that vtium.conf holds the whole flow. Its flat `key=value` lines stay as the defaults every head inherits, so a kmscon.conf copied to `/etc/vtium/vtium.conf` still reads, and the head, rule and look sections come below them. heads.conf's content moves into vtium.conf.

The move forces these decisions, which this record's agent makes from ADR-1401 and the tree:

- **The top level and the sections.** The top level is every line above the first section heading. It holds vtium's keys, as vtium.conf does today, and the two keys heads.conf had above its first section, `version` and `policy`. Everything from the first section heading on is the sections. A key line after a section heading belongs to that section, so the top level is written first, as heads.conf's top-level keys were.
- **A file with no section is a kmscon.conf.** It needs no `version` line, and vtium runs the compiled-in flow for it (ADR-1401 D6), as it does today when heads.conf is missing. A file with a section needs `version = 1` at the top level. A missing or unknown version is ADR-1401 D7's error that drops the whole head file but its policy; under one file it drops every section but the policy, and the top-level keys still apply.
- **Trust covers the whole file.** vtium reads vtium.conf only when root owns it and neither its group nor others can write it or a directory above it, the check ADR-1401 D7 and D10 make of heads.conf. A vtium.conf that fails the check is not read at all: vtium runs its compiled-in keys and flow, and every head shows `invalid config skipped, using builtin`, as for an untrusted heads.conf. The check covers the top level as well as the sections because vtium.conf now places keyboards, and because its `login` key already names the command vtium starts as root. A kmscon.conf copied in by root, with the usual mode 0644, passes.
- **Two ways of handling a mistake, one for each part.** The top level keeps vtium.conf's reading, which follows the C reference's: an unknown key is reported once and ignored, a value out of bounds is clamped, a value vtium cannot read is ignored, and a key set twice takes its last value. The sections keep ADR-1401 D7's three levels unchanged, and what ADR-1401 D7 calls the whole file is now the sections and the policy. An error in a section never drops a top-level key, and a top-level mistake never drops a section.
- **The policy line.** ADR-1401 D7's rule for a policy line holds: a line above the first section that begins with `policy`, in any mix of case, is a policy line, and one vtium cannot read makes the sections closed and broken. Among vtium's keys none begins with `policy`, so no existing key changes meaning.
- **One bound for the file.** vtium.conf keeps its bounds, 65,536 bytes a file and 8,192 a line, which heads.conf shares. The shipped example vtium.conf and the head file's template together hold 40,327 bytes. A test fails when the generated example passes 49,152 bytes, so at least 16 KiB stay for a user's own heads, profiles and rules.
- **One source for the compiled-in flow.** The head file's template, `crates/vtium-settings/templates/heads.conf`, becomes the sections of the shipped example: the example vtium.conf is the key table's commented lines, then `version = 1`, then the template's sections with the shipped rules as live lines, as ADR-1401 D6 has them. vtium's compiled-in flow is still parsed from that same text.
- **`vtium check-config FILE`** checks a candidate vtium.conf, both parts in one run with one exit status, where today FILE names a candidate head file.

### D4: the syntax of the sections (open)

The maintainer asked for the head, rule and look sections to nest below the flat lines, and asked how other `.conf` files nest objects. Two families of syntax nest objects in a file that people edit by hand, and both keep a kmscon.conf readable:

- **Sections with names,** as in an INI file. git's configuration writes `[remote "origin"]` and `[branch "main"]`, systemd's unit files write `[Service]`, and heads.conf already writes `[head desk]` and `[rules]`. The section's heading names the object, and every line below it, up to the next heading, belongs to it. A deeper link is a key that names another section: git's `[branch "main"]` holds `remote = origin`, which names the `[remote "origin"]` section, and a head's `profile = big` names a profile the same way. git puts a section's name in quotes so that it can hold any character, as in `[display "edid:AAA:0x0001:LARGE00001"]`; a vtium name holds no blank (ADR-1401 D3), so vtium needs no quotes, and heads.conf has none.
- **Blocks in braces,** as sway, i3, Hyprland and nginx write them: `server { location / { ... } }` in nginx. sway is the closest to vtium, since it matches displays and input devices by a selector at the head of each block: `output eDP-1 { mode 1920x1080 }` and `input type:keyboard { xkb_layout us }`. sway applies every block whose selector matches a device and merges their settings. ADR-1401's rules place a device by the first rule that matches, in the order the file gives, so under braces vtium keeps its rules as one ordered list inside one block, rather than a block for each selector.

Two other forms lose, for these reasons:

- **TOML** gives real nesting with existing parsers, but a text value must be quoted, as `xkb-layout = "us"`, and every key needs a value, so a switch written alone on its line, such as `hwaccel` or `mouse`, is invalid. A kmscon.conf would not parse, which breaks the maintainer's decision that one copied in still reads. YAML loses for the same reason, and makes indentation carry meaning.
- **Dotted flat keys,** as sysctl writes them (`net.ipv4.ip_forward = 1`), would write a head as `head.desk.profile=big`. Every line stays a flat key, which an older vtium ignores as unknown, but the structure is invisible: a head is spread over lines that only share a prefix, nothing groups them, and the rules need a key such as `rule=` that keeps every value it is given, where every other key keeps its last.

The two forms that remain follow, each as the same whole vtium.conf, for a machine with two displays, a large monitor and a small panel, and one keyboard that types into whichever head is active. The top level is the same in both: two of vtium's keys and a switch.

**A: sections with names, as heads.conf writes them, with a section for each profile** (recommended).

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

[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display edid:BBB:0x0002:SMALL00001 -> panel
display * -> desk
input class:keyboard -> roaming
input * -> ignore
```

**B: blocks in braces, as sway writes them, with each head's look written inside the head.**

```text
# /etc/vtium/vtium.conf: two displays, one keyboard.
# The top level: vtium's keys, the defaults every head inherits.
font-size=12
xkb-layout=us
mouse
version = 1
policy = open

head desk {
    profile big {
        font-name = monoid
        font-size = 14
    }
}

head panel {
    consoles = 2
    profile small {
        font-name = dejavu
    }
}

rules {
    display edid:AAA:0x0001:LARGE00001 -> desk
    display edid:BBB:0x0002:SMALL00001 -> panel
    display * -> desk
    input class:keyboard -> roaming
    input * -> ignore
}
```

How they compare:

| | A: sections with names | B: blocks in braces |
|---|---|---|
| A kmscon.conf still reads | yes | yes |
| What shows the nesting | a head names its profile, as git's branch names its remote; the flow view draws the name as a line | the profile is written inside its head |
| heads.conf moves in | pasted below the top level, unchanged | rewritten into blocks |
| Parser | the head file's, which exists, with one new section kind | a new one; a missing brace swallows every line after it, so ADR-1401 D7 has no clear line to drop |
| A profile two heads share | written once and named by both | written as a block of its own and named by both, as in A, or copied into each head |
| An older vtium sharing the file (ADR-1500 D4) | reads a section's `login` and a profile's font lines as top-level keys | the same, since an older vtium reads line by line and ignores the braces as unknown keys |
| ADR-1304's vtlook | uses the same syntax | differs |

The recommendation is A. It is heads.conf's syntax, which the maintainer decided moves into vtium.conf rather than being redesigned, its parser and ADR-1401 D7's drop rules exist, and ADR-1304's vtlook file is written the same way. A head names its profile, as git links its sections, and the flow view draws that name as a line, so the nesting the maintainer asked for shows in the view while the rules keep their single order. A `[profile NAME]` section is the profile's file that ADR-1200 D16 left for later: it takes the keys D5 lists, under the same names the top level uses. B writes the nesting most literally and follows sway, the closest program to vtium, at the cost of a new parser, of a missing brace that drops more than its own line, and of a second way to write a profile that two heads share. Both share one cost: an older build misreads a section's `login` or font lines while two builds share the file, which ends once both are updated.

Under A, one comment rule serves the whole file, the head file's: a `#` starts a comment at the start of a line or after a blank, a `#` inside a word is part of the word, and `\#` gives a literal `#` after a blank. vtium.conf's exception is kept: a `#` that opens a `#rrggbb` value is part of the value. A value at the top level can then hold a `#` inside it, as a head's `login` command already can. In one case a top-level line reads differently from the C reference: a `#` directly after a value, with no blank before it, as in `font-size=12#big`, stays in the value, which vtium then reports as one it cannot read. This is listed as a departure. Section keys always take `key = value`; the switches `key` and `no-key` stay at the top level.

### D5: how a key set at two levels combines (open)

The recommendation:

- **Three levels, and the most specific wins:** a head's section, then the section of the profile the head names, then the top level. The command line sets top-level keys and wins over the file's top level, as it does today, and a section still wins over both, as a head's profile wins over vtium.conf's `font-name` today (ADR-1200 D16). A key the head and its profile leave unset takes the top level's value, and a key the top level leaves unset takes its default.
- **Each section kind takes a fixed list of keys.** A head takes ADR-1401 D3's head keys, `profile`, `consoles`, `login`, `when-display-lost` and `isolated`, and ADR-1304 D5's `look` once it is built. A profile takes the keys ADR-1200 D16 gives a profile that the key table has: `font-name`, which names the profile's frit, `font-engine`, `font-size`, `font-hinting`, `font-line-spacing`, `display-depth` and `display-dpi`. A head sets no font key of its own, since a head's font has one path, from the head to its profile to the profile's frit (ADR-1401 D3).
- **A key in a section has the top level's name, values and bounds** where the top level has the key too, as `login` and the font keys do. Inside one section a key given twice is an error, as in heads.conf today.
- **A key a section kind cannot take is an error in that section,** which drops the section under ADR-1401 D7: a head drops with its rules, and a profile drops, so a head that names it takes the default profile (ADR-1200 D16). The error names the levels where the key may be set.
- **The key table records where each key may be set.** Its column that says a key is set in a file or on the command line gains the values `profile` and `head`, so docs/config.md lists each key's levels, and allowing one more key in a section later is an edit to one row.
- **Keys that belong to the whole process stay at the top level,** such as `display-buffers`, `consoles-max`, `hwaccel` and `sb-memory`, for the reason ADR-1401 D2 gives for `display-buffers`: they trade the machine's memory or set the process's behaviour, and are the same for every head.

The other way is to let every key be set in a head's section. It would give each head its own palette or keyboard layout, but many keys belong to the process, the seat or an input device rather than a head, and each would need a rule for what a head's value means. A key can move into the head's list later, one row at a time, when someone needs it.

### D6: migration (open)

**heads.conf.** The recommendation is both of the options the maintainer named, with a fixed end:

- vtium reads `/etc/vtium/heads.conf` as before, under the same trust check and the same three levels, only while vtium.conf has no section. It then logs a warning at each start, and `vtium check-config` warns, exiting 1, naming the file and `vtium config fold`.
- When vtium.conf has a section and heads.conf is there too, vtium reads vtium.conf's sections alone and warns that heads.conf is not read.
- From the first beta release vtium never reads heads.conf, and warns at each start while one is present. vtium never changes or moves either file (ADR-1401 D6).
- `vtium config fold` is a vtium-config command, which the TUI also offers when it finds either file. It writes a draft of vtium.conf with heads.conf's text placed below vtium.conf's top level, and its `version` and `policy` lines moved to the top level. It saves the draft through `sudo vtium-config apply`, which checks the result with `vtium check-config` before it writes anything (ADR-1201 D5). Once vtium.conf is written, `apply` renames heads.conf to `heads.conf.bak` beside it, the one generation `apply` keeps of any file it replaces, so vtium no longer finds it. `fold` refuses a heads.conf vtium does not read whole, naming each error, as the head file's writer refuses one today, so a broken file is mended before it moves.
- Packages keep heads.conf as a configuration file they track until vtium stops reading it. A package that dropped the file sooner would have its package manager set a changed copy aside under another name, such as pacman's `.pacsave`, where vtium no longer finds it, and the user's heads would be gone without a word.

**The seat file.** vtium stops reading `seat0.seat.conf` with this change, as the maintainer decided. While the file is present, vtium logs a warning at each start naming it and `vtium config fold`, and `vtium check-config` warns, exiting 1. `fold` sets each of the seat file's keys at vtium.conf's top level through the line editor, its values winning as they did over vtium.conf, and renames the file to `seat0.seat.conf.bak`. vtium never read another seat's file, since it serves seat0 alone, and says nothing of one.

The other ways: reading heads.conf with no end leaves two places for the same configuration for good; a fold done by vtium itself breaks ADR-1401 D6's rule that vtium never writes a configuration file; and stopping at once loses a working setup on the next update.

### D7: the flow view (open)

The figure shows the recommended view on the machine of D4's example, with D4's option A, as it looks on a dark console. The keyboard is selected, and its path through the flow is spelled out on the line below the columns.

![vtium-config's flow view on a black console background, under the tabs Fonts, Sizes, Settings, Heads, Flow (selected) and Save. A status line reads /etc/vtium/vtium.conf, policy open, a udev daemon runs, 2 displays and 2 inputs present. Six columns follow, each heading in its own color: DISPLAYS in bright cyan lists DP-8 (LARGE00001) and HDMI-A-2 (SMALL00001); DISPLAY RULES in cyan lists line 18, edid LARGE00001 to desk, line 19, edid SMALL00001 to panel, and line 20, display star to desk, in grey with the words matches no display present; HEADS in bright yellow and LOOK in bright magenta share a box for each head: desk, 1 console, the active head, with profile big at 14 pt, frit monoid and the font Monoid, and panel, 2 consoles, with profile small at 12 pt, frit dejavu and the font DejaVu Sans Mono; below them grey boxes for roaming (types into desk, active), ignore (vtium never opens it) and off (vtium sets it off). INPUT RULES in green lists line 22, a serial match to desk, in grey with the words matches no device present, line 23, class keyboard to roaming, and line 24, star to ignore. INPUT DEVICES in bright green lists the keyboard, selected in reverse video, at usb-0:3.3, and the Power Button, class key. Cyan arrows run from each display to its rule and from each rule to its head; green arrows run from each input device to its rule and from each rule to its target; grey dashed arrows run from rules 20 and 22 to desk. A legend explains the solid and dashed lines, a detail line reads keyboard, usb-0:3.3, class keyboard: line 23, input class:keyboard -> roaming, so it types into desk, the active head, and a key line reads arrows move, Enter shows the line in the file, Tab next view, q quits.](figures/ADR-1403-flow-view.svg)

The recommendation:

- **The columns,** left to right: displays, display rules, heads, look, input rules, input devices. Each head is one box spanning the heads and look columns, with the head's name and its consoles on the left and its profile, frit and fonts on the right, so lines from display rules enter the box from the left and lines from input rules from the right, and no line crosses the look column. Below the heads, boxes for `roaming`, `ignore` and `off` take the rules that target them, and a box for the policy takes the devices no rule matches, with the words of what the policy gives them.
- **What a line means.** A line from a device to a rule says that rule is the device's first match. A line from a rule to a box says that box is the rule's target. Every rule has its target line whether or not any device is present; only devices present have lines of their own. The look's entries hang from the profile as a small tree: the profile, the frit it names, and each font the frit draws with that this machine has.
- **Colors,** each section in its own, from the console's sixteen colors so the user's palette applies: displays bright cyan, display rules cyan, heads bright yellow, the look bright magenta, input rules green, input devices bright green. A line takes the color of the rules it passes through. Grey, the palette's dark grey, marks what is not in play. Bright red marks an error. The view uses no blue, since the palette's blue on black is hard to read, and no color as the only sign of anything: every state also has its words, and with `NO_COLOR` set or on a terminal without color the view draws the same text without color.
- **A rule that matches nothing** present is drawn in grey with the words `matches no display present` or `matches no device present`, and its line to its target is dashed. A rule an earlier rule shadows, a profile that names a frit that does not exist, a font the frit names that this machine lacks, and a line the file's errors drop each show in place with their own words, the same words `vtium check-config` prints, and an error in bright red.
- **With no udev daemon,** the status line says so, and the view lists the devices sysfs shows, with the input classes vtium's own copy of udev's `input_id` rule gives (ADR-1401 D4). Each rule that uses `udev:`, `path`, `serial`, `card` or `connector` is drawn in grey with the words `needs udev`, since it matches nothing on that machine, which is the note `vtium check-config` gives.
- **Moving and selecting.** The arrow keys move between components. The selected component and the whole path through it are drawn in reverse video, the TUI's selection mark today, and the line below the columns says that path in words. Enter shows the selected component's lines in the file, with their line numbers.
- **Narrow terminals.** The columns need 80 columns. On a narrower terminal the view draws the same flow as an indented list, one block for each head with its displays, rules, look and inputs beneath it, then the roaming, ignore, off and policy blocks, so it fits the 40 columns of a minspec console.
- **How to reach it.** A tab named Flow in the TUI, after Heads, and a command, `vtium config flow`, that prints the view as text and exits, as every action of the TUI is also a command (ADR-1201 D2). The view reads vtium.conf, the frits and the devices as the user, with no privilege, from the console or a desktop terminal. On a desktop, displays are named as the kernel names them.
- **It only reads.** The first version changes nothing. Editing in the view comes later, through the same writers as every other edit (D8).

### D8: what vtium-config writes (open)

vtium-config writes vtium.conf, heads.conf and frits today, through drafts that `sudo vtium-config apply` checks and installs (ADR-1201 D5, D6). The recommendation:

- **One configuration file.** vtium-config writes vtium.conf and the frits, and, under ADR-1304 once it is built, vtlooks. A draft holds the whole edited vtium.conf, and names heads.conf or `seat0.seat.conf` only as a file for `apply` to set aside after a fold (D6).
- **One reading of the file for every writer.** vtium-settings reads vtium.conf into its top level and its sections, and each writer works from that reading, so they agree where the top level ends. The line editor of ADR-1201 D5 changes only the top level: a key is replaced on the top-level line vtium takes it from, never on a section's line of the same name; a commented line such as `#font-size=0` counts only at the top level; a new key goes after the last top-level line, never at the end of the file; and `--unset` comments out top-level lines alone. The section writer, which sets a head's profile today (ADR-1201 D6), keeps its rules for every section it edits: it edits only a file vtium reads whole, changes only the lines it needs, and parses its result before handing it back.
- **What each part of the TUI writes.** The Settings view writes top-level keys. The Heads view sets a head's profile. A profile's keys go in its `[profile NAME]` section, once D4's option A is taken, through the section writer. The Flow view writes nothing until editing comes.
- **`apply`** checks vtium.conf with `vtium check-config FILE`, one run for both parts. It writes the frits first and vtium.conf after them, so vtium.conf never names a frit that is not there, and sets aside a folded file last. It keeps one `.bak` of each file it replaces, as today.

### D9: what the code changes to fit

These follow from D1 to D3 and the tree, whichever answers D4 to D8 get:

- vtium.conf's reader stops at the first section heading and hands the rest to vtium-settings's head-file reading, through one function that splits the file, so the console, `vtium check-config` and vtium-config agree where the top level ends.
- The trust check vtium-settings makes of heads.conf applies to vtium.conf.
- The seat file's code goes: the seat's path in `files`, the seat's name check and its report in `crates/vtium/src/config/mod.rs`, the seat's argument to `load` in `main.rs`, and `SEAT` in `crates/vtium-config/src/switch.rs`.
- The mapping from a listed device to the identity the rules match moves out of the console binary into a crate that vtium and vtium-config both link, so the flow view and `vtium check-config` place each device by one function. The work that builds the view chooses the crate.
- The TUI gains color, through the sixteen-color SGR codes alone, and reads `NO_COLOR`.
- The example vtium.conf is generated from the key table and the sections template together, with the size test of D3.

## Open questions

Each question is answered by the maintainer before this record is accepted. The recommendation for each, in one line:

1. **The syntax (D4):** heads.conf's sections with names below the top level, as they are, with a `[profile NAME]` section for each look and one comment rule for the whole file, rather than sway's blocks in braces; TOML and dotted flat keys lose.
2. **Two levels (D5):** a head's section over its profile over the top level, with a fixed list of keys for each section kind, recorded in the key table.
3. **Migration (D6):** vtium reads heads.conf with a warning while vtium.conf has no section, until the first beta; `vtium config fold` moves heads.conf and the seat file's keys in and sets the old files aside; the seat file stops being read at once, with a warning.
4. **The view (D7):** six columns with a box for each head spanning heads and look, lines for first matches and targets in the colors of the rules they pass, grey and words for a rule that matches nothing, a list on terminals under 80 columns, reached by a Flow tab and `vtium config flow`.
5. **Writes (D8):** vtium-config writes one configuration file, vtium.conf, through one reading that both of its writers share.

## Passages this record changes

Once the maintainer accepts this record, each passage below is edited in place with a dated note that cites ADR-1403:

- ADR-1401 D3: the head file is the sections of `/etc/vtium/vtium.conf`, and its top-level keys sit among vtium's keys. D2: a later seat's flow goes into vtium.conf, not a file of its own. D6: the template is the sections of the example vtium.conf. D7: the trust check covers vtium.conf, and "the whole file" means the sections and the policy. D12: `vtiumctl template` and `install` write vtium.conf.
- ADR-1200 D16: a profile is a `[profile NAME]` section of vtium.conf, if D4's option A is taken.
- ADR-1201 D5 and D6: what a draft holds, the line editor's top level, and the section writer.
- ADR-1500 D4: the rows for the seat file and for heads.conf.
- ADR-1304 D5, still proposed: a head's `look` is set in its section of vtium.conf; its author changes it before acceptance.
- docs/config.md, docs/configure.md, docs/head-file.md, docs/heads.md and the files under docs/heads/, the READMEs of the vtium, vtium-config and vtium-settings crates, and the configuration example and template.

## C notes: the C reference's flow

- `kmscon_conf_load_main` parsed the command line, then `<configdir>/kmscon.conf`, then set the log level again so that the file's `debug` and `verbose` took effect (`kmscon/src/config.c:1500-1552`).
- `kmscon_conf_load_seat` copied the main configuration and parsed `<configdir>/<seat>.seat.conf` over it, with the flag `seat_config` set (`kmscon/src/config.c:1554-1575`). The seat called it once as it was made (`kmscon/src/seat.c:786`).
- `parse_line` read one key a line: up to a `=`, a `#` or the end of the line, then a value up to a `#` or the end, where a `#` that began a `#rrggbb` value was part of it. A line with no `=` was a switch. It knew no sections (`kmscon/src/misc/conf.c:402-500`).
- The C reference read its files with no check of their owner or mode.

## Where vtium departs from the C reference

- **No seat file.** The C reference read `<seat>.seat.conf` over kmscon.conf for each seat. vtium reads vtium.conf and its command line alone, and its configuration for every seat it will serve lives in vtium.conf (D2).
- **Sections below the keys.** The C reference's file held keys alone. vtium.conf holds the flow's sections below them, and a kmscon.conf with no section reads as it did (D3).
- **Root's file.** The C reference read its file whoever owned it. vtium reads vtium.conf only when root owns it and no one else can write it (D3).
- **A `#` glued to a value,** if D4's option A and its comment rule are taken: the C reference ended a value at any `#` that did not open a `#rrggbb` value, and vtium keeps a `#` with no blank before it in the value (D4).

## Consequences

### Positive

- One file shows how each display and each keyboard reaches a terminal and what that terminal draws with, and the flow view draws it with the machine's own devices on it.
- A head's inheritance is written where it happens: the keys a head inherits sit above its section in the same file.
- vtium.conf gains the trust check heads.conf had, which also closes the path by which a vtium.conf that root does not own could name the command vtium starts as root.
- The seat file's second layer of the same keys goes, so a key has one place at each level.

### Negative

- One file mixes a forgiving reading at the top level with a strict one in the sections, and a reader has to know which part a line is in. The flow view and `vtium check-config` say which.
- A user who runs vtium with a vtium.conf root does not own loses that file's settings until it is root's, and sees `invalid config skipped, using builtin`.
- heads.conf and the seat file each need a migration, a command and two warnings, and heads.conf stays readable until the first beta.
- While two builds share `/etc/vtium/vtium.conf`, a build older than this change misreads a section's `login` and a profile's font lines under either syntax D4 weighs.
- The flow view brings color into vtium-config, which needs a rule for terminals without color and a list for narrow ones.

### Neutral

- Frits, vtlooks, shaders and fonts stay files of their own, which the flow names and the view draws.
- `vtiumctl`, not built yet, writes vtium.conf where ADR-1401 D12 has it write heads.conf.
- The view's first version only reads; editing in it needs its own decisions on undo and the trial of ADR-1401 D12.

## Alternatives Considered

- **Keep heads.conf beside vtium.conf, and draw the flow from both files.** The view could join two files, and nothing would migrate. The maintainer decided on one file, and two files keep the inheritance split from what it inherits.
- **Keep the seat file.** It is a second layer of the same keys, which no decision chose, and vtium serves seat0 alone. The maintainer removed it.
- **A new file for the flow, such as `flow.conf`, with vtium.conf left as it is.** It keeps kmscon.conf untouched, and it keeps the split the maintainer decided to end.
- **TOML, YAML or dotted flat keys for the whole file.** D4 gives the reasons each loses: TOML and YAML cannot read a kmscon.conf, and dotted keys hide the structure. vtium's parsers are also written to bound untrusted input (ADR-1401 D10), which a general-purpose parser is not.
