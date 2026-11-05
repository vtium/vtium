---
contract: adr/v1
kind: decision
verb: add
capability: glyphs
basis:
  - operator: aaronsb
    level: directed
    said: "Vitrify is how every font reaches the console. A kbd console font and an outline font such as a Nerd Font take the same path: the console loads only vitrified sets and its built-in ones, and never decompresses, parses a PSF or rasterises."
    via: the GitHub issue that tracks vtium-vitrify, comment of 2026-10-28 headed "Direction (operator, 2026-10-28)", posted from the operator's account as the operator's direction; the wording is agent-written
  - operator: aaronsb
    level: directed
    said: "The console loads a set only from the root-owned store, and only when its hash matches the manifest. The built-in sets (unifont, and Unscii-8 for the 320x240 floor) are trusted by construction."
    via: the same issue, the same comment
  - operator: aaronsb
    level: directed
    said: "[The console-fonts issue's] KDFONTOP route vitrifies the font the kernel already holds."
    via: the same issue, the same comment
  - operator: aaronsb
    level: directed
    said: "`set` reaches the console through xterm's OSC 50 font operation on its own tty, behind a config key like xterm's `allowFontOps`."
    via: the same issue, comment of 2026-10-28 headed "Decided (operator, 2026-10-28)", answering the direction's four questions; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "`create` is a sudo command; there is no per-user store. A font set is first declared as a small recipe (its metadata and configuration: the sources, such as a packaged font or kbd name, a unimap, an outline font, the cell sizes and fallbacks). `create` runs the recipe: it fetches and reads the resources, and the finished set is a resource ready for the DRM renderer."
    via: the same issue, the same comment
  - operator: aaronsb
    level: directed
    said: "vtium supplies the CLI, so `create` is non-interactive and scriptable (a set rebuilds when its recipe or source hashes change, and the built sets can be listed in a machine-readable form); the wrappers are other people's choice. A configured font that names no built set falls back to the built-in font, and the log names the set and the `create` command."
    via: the same issue, the same comment
  - operator: aaronsb
    level: directed
    said: "the binary is named `vtium-vitrify`."
    via: the same issue, comment of 2026-10-28, after "the tool is its own binary (working name `vtium-vitrify`) in the install tree, apart from the `vtium` binary"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "So a set is a portable file: `vtium-vitrify create --out FILE` builds one anywhere, as any user, with no store; `sudo vtium-vitrify install FILE` on the target checks its hashes and format version and adds it to the root-owned store."
    via: the same issue, comment of 2026-10-28 headed "Direction (operator, 2026-10-28), for ADR-1200 before it's accepted"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "`vtium-vitrify` must self-bootstrap. Building on the target is a first-class path: `sudo vtium-vitrify create` builds into the local store for the machine's own display, including on a minspec board for its small panel, so a fresh install needs no other machine."
    via: the same issue, comment of 2026-10-28 headed "Correction (operator, 2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "Sizes: no ladder. Per locale, one 1-bit bitmap set scaled by whole factors (Fusion Pixel 12's locale variant for zh/ja/ko; nothing beyond the built-ins for English), plus at most one outline size for the display the hook can see (snapped to 24, 32 or 48), restricted to the locale's standard set; none when no display is visible. A machine holds a few sets (default plus a couple of favorites), each 1-3 adjacent sizes. Zoom below a set's smallest size stops there with a log line."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead in this revision's brief; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "Portable sets and two build paths: `create` on the target is first-class (self-bootstrap); `create --out FILE` builds anywhere as any user; `install FILE` (root) verifies and adds to the store; distributions may ship prebuilt per-locale set packages whose hook runs install. Portability (fixed byte order, reproducible bytes, nothing machine-specific) is a requirement. Recipes built for another machine name their sizes."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead in this revision's brief; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "The tool: preview and new/template belong to it, as the user; sudo once at the end of new."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead in this revision's brief; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "The recipe carries an explicit compress setting, with a matching [guided builder] template field and builder choice, and the default is to compress. With the default, create deflates the strikes that gain (8-bit and color) and leaves 1-bit raw; compress=no stores everything raw."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "The reader always reads both forms. Every vtium build carries the inflate, so any set, compressed or raw, chunk by chunk, loads on any build, minspec included; compress= changes only disk size, never what is drawn."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "compress=no exists for power draw as well as compute: on a battery- or power-limited board, a raw set avoids the inflate work at load. The choice belongs to the recipe, and so to whoever builds the set, a package for low-power boards included."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "A build that isn't minspec may include a rasteriser for sizes a set lacks, but only unprivileged and sandboxed, in ADR-1400's L1 helper (the session's user, NoNewPrivs, its sandbox). The minspec build never rasterises."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead, with the flow recorded in D15; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "**Fonts by kbd name.** `font-name=ter-v16n` resolves as kbd's `setfont` does"
    via: the body of the GitHub issue for console fonts, "The operator's decisions (2026-10-27)", posted from the operator's account, the wording agent-written; its routes are reshaped onto vitrify by this record
  - operator: aaronsb
    level: guided
    said: "Later it can serve as the metadata of a packaged font set (a \"vitrified font pack\")."
    via: the GitHub issue for choosing between a font's glyphs and vtium's sprites, "The operator's decisions (2026-10-27)", item 3, on the per-font sprite policy; posted from the operator's account, the wording agent-written
  - evidence: "the console-font research of 2026-10-28 (prior art, per-locale sizes, disk cost, rasterisers, storage formats), compiled by the team lead from the font-research agent's measurements, each figure marked measured, sourced or inferred; the report and its data sit in the steering session's scratchpad (font-research/report.md, out/, bench/), uncommitted. Measured on a Ryzen 9 9950X3D on one core; Cortex-A53 figures are inferred by a stated factor. It is the basis of the technical decisions on storage (D13), the frit's cover=, bdf: sources and fitted fallbacks (D1), parsing in an unprivileged child with swash as the rasteriser (D14), and the built-ins (D7)"
  - operator: aaronsb
    level: directed
    said: "A set does not have to be built. Generation is one engine on two schedules, and both respect every frit option: a frit drawn at run time gives what the same frit built ahead of time gives. A built set is a cache of the frit's output, kept and available once made; prebuilding is a performance, security and repeatability gain, not a requirement."
    via: the team lead's message of 2026-10-28, relaying the operator's decision, recorded in full in ADR-1301; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "a minspec machine may also rasterise at run time; it is likely not ideal there (memory, about 22 MB for a CJK font while rasterising), so a minspec install prefers built sets, but nothing forbids it"
    via: the team lead's message of 2026-10-28, relaying the operator's decision reversing D15's ban; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "the set format's version is set now at 1.0, with the multi-format cells [the outline-font work] added, and kept; a change older readers can't read takes a new major version from here on"
    via: the team lead's message of 2026-10-28, relaying the operator's decision; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "`font:` names a font family and nothing else; size and style go on their own lines (`size=`, `style=`)."
    via: the GitHub issue for vitrify's guided builder, comment of 2026-10-28 headed "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - upstream: "aaronsb/kmscon origin/main: with `font-engine=psf`, `font-name` was the path of a PSF font the C reference read and inflated at run time (src/font/font_psf.c; the fork's render ADR-103, 'A console font takes two settings'); the C reference made no KDFONTOP or GIO_UNIMAP call, resolved no kbd name and read no vconsole.conf"
  - upstream: "kbd (legionus/kbd master, read 2026-10-28; kbd 2.10.0 is the installed program): src/libkfont/setfont.c loads def.uni only for its default font, when that font has no Unicode table, and loads no map for a named font without one; the default font is the first of default, default8x16, default8x14 and default8x8 found; font lookup and suffixes as the console-fonts issue cites (src/libkfont/context.c, kbdfile.c)"
  - standard: "Linux 7.3 UAPI, include/uapi/linux/kd.h: KDFONTOP with KD_FONT_OP_GET and KD_FONT_OP_GET_TALL, and GIO_UNIMAP; the kernel refuses a font get on a VT outside text mode (drivers/tty/vt/vt.c, con_font_get, as the console-fonts issue cites)"
  - standard: "xterm ctlseqs, OSC 50 (set and query the font); xterm(1), allowFontOps: 'The default is “true”'"
  - standard: "vconsole.conf(5), systemd 262: FONT=, FONT_UNIMAP= and FONT_SCALE=, and the kernel command line's vconsole.font= overriding them"
  - evidence: "Unscii, fetched 2026-10-28 from http://viznut.fi/unscii/ (its https certificate failed to verify, so the files are pinned by hash): unscii-2.1-src.tar.gz, SHA-256 4d1215d495e49d005ae70828ec1529e997049b9a7f152b9439d78c7e70c65b57, whose unscii.txt says 'Everything made by me (viznut) is in the Public Domain.' and that only unscii-16-full, which takes glyphs from Unifont, falls under the GPL; unscii-8.hex, SHA-256 03094f7fbab7085cf6a6b624cee61e47e71ce5d0c2f308c2f4436afdc17f776c, 73,409 bytes, 3,191 glyphs, all 8x8 but U+0000 at 16x8, 19,836 bytes under gzip -9"
  - evidence: "the minspec binary at 2,429,200 bytes at integration 4086adf against the 2,621,440-byte budget (tools/tier3/README.md, BINARY_BUDGET), as the team lead reported in this record's brief"
  - precedent: ADR-1300
  - precedent: ADR-1400
  - precedent: ADR-1000
  - operator: aaronsb
    level: directed
    said: "the prerendered store is replaced by deferred rendering. vtium generates glyphs at run time from the configured frit, in the sandboxed helper (ADR-1301's run-time schedule), on every build including minspec. On a cold start the built-in font (Unscii-8 under 16 px, unifont from 16) draws at once, and generated glyphs swap in as they arrive, so a cold start shows no lag. A performance problem is answered by that deferred pipeline, not by building sets ahead of time."
    via: the body of the GitHub issue that replaces the prerendered store with deferred rendering, "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "What goes: the root-owned store and its manifest; `vtium-vitrify create`, `install`, `list`, `verify` and `remove`; the console's store check and set loading; target matching for built sets ([the prebuilt half of the work on recording a set's target display and reading the live display]), named targets, the build-time half of the rule checker; package hooks that build sets."
    via: the same issue's body, the same decision
  - operator: aaronsb
    level: directed
    said: "What stays: frits, the definition the engine draws from (`NAME.vtfrit`); the built-in fonts; vitrify's sources, decoders, sandboxed parser child, rasteriser, color scaling and HarfRust shaping, which the helper runs; the strike chunk format, as the helper's wire format and its per-boot cache; `preview` and `new`, with `new` writing a frit; OSC 50 switching (`set`), naming a frit; ADR-1301's display grid and rules."
    via: the same issue's body, the same decision
  - operator: aaronsb
    level: directed
    said: "profiles keep their value and stay. vtium-vitrify remains the vitrification toolkit, with reduced scope: it defines profiles (a display's capabilities, named freely, with the frit and vtium configuration that suit it), writes and checks frits (`new`, the rule checker), previews them (`preview`), switches the console's frit (`set`), and supplies the engine the helper runs. It no longer builds, installs or maintains prerendered sets. [The demonstration set's] specimens are drawn per profile."
    via: the same issue, comment of 2026-10-28 headed "The operator's direction (2026-10-28), refining this issue"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "vitrify builds profiles, and each profile states a frit, its cell and its options. vitrify previews them, guides a font to a profile, and lints a profile against a display. The helper runs vitrify's engine to generate the glyphs at run time."
    via: the GitHub issue that tracks vtium-vitrify, comment of 2026-10-28 headed "Reopened as the tracker for vtium-vitrify as the profile builder"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "no prerendered cache of any kind, persistent or otherwise beyond the helper's per-boot memory cache; vtium can't prerender the universe, so that complexity isn't carried. vtium keeps a compiled-in built-in font, as the C reference did, drawing at once on a cold start and wherever the helper is unavailable; everything else is generated at run time."
    via: the GitHub issue that replaces the prerendered store with deferred rendering, comment of 2026-10-28 headed "The operator's decisions (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "build the deferred pipeline first and prove it works; then drop prerendered sets from vtium's code, docs and paths; then rewrite the decision records so they describe run-time generation as the design, with the prerendered store and any persistent glyph cache recorded only under Alternatives Considered, so no reader of the ADRs meets prerendered fonts as a live idea. ADR-1201's amendments fold into that rewrite."
    via: the same issue, comment of 2026-10-28 headed "The operator's direction (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "1. Requests come first. The characters on screen, or newly requested, are answered first. 2. When idle, the helper fills the coverage in order: the screen's characters, then printable Latin and box drawing, then the rest of the frit's coverage. 3. A waiting request preempts the fill between batches, so it waits at most one batch. 4. Glyphs already held are served from the per-boot cache. 5. The store limit comes from the existing memory budget, the way `sb-memory` takes its own, and adds no new size decision or key. The fill stops at the limit, so speculative generation never churns. Only a request past the limit evicts, least recently used first. 6. The helper is recalled whenever needed. After an idle exit, the next need starts it again. After a crash or the CPU limit, the next request restarts it, a bounded number of times per boot. A helper that answers garbage gives that frit up for the boot."
    via: the same issue, comment of 2026-10-28 headed "The operator decided the run-time fill model (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "The helper is not persisted. It exits after its idle interval and is started fresh on the next need, so no state a hostile font plants survives the process, and its CPU and memory budgets reset. A restart narrows the window and fixes no vulnerability; the console's checks and the sandbox contain one. Crash restarts are capped per boot, because each restart re-randomizes the address layout and unbounded restarts would let an exploit retry until it lands."
    via: the same issue, comment of 2026-10-28 headed "The operator accepted the helper's lifetime (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "The grid never resizes because glyphs arrived. 1. The profile states the cell. The console reads it before the first frame, as text, so the console still parses no font. 2. The built-in draws in that cell until the generated glyphs land: centered when the cell is larger, and in the smaller strike when the cell is smaller. 3. The helper generates for the profile's cell and never moves it. A frit that cannot fit the cell fails with a logged reason, and the built-in keeps drawing. 4. A profile whose cell does not suit its frit is linted when vitrify builds or checks the profile, and in `vtium check-config`. 5. With no profile, the cell comes from the display, by the rule the C reference used for its automatic size: at most 120 columns, at least 40 rows, and never under 16 px. [The 320x240 font-size issue's] small-display rule takes precedence at the 320x240 floor. 6. The grid resizes only on an explicit event, as it did in the C reference: a zoom, or a display hotplug while the cell is automatic. A resize sends SIGWINCH and redraws. Glyphs for the new size are requested, and the built-in fills the gap."
    via: the same issue, comment of 2026-10-28 headed "The operator decided where the cell comes from (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "glyph swap-in is a damage source on the same path as program output (2026-10-28). Glyphs that arrive between frames merge into one damage pass. Damage comes from what each buffer displays, the scrollback view and cluster cells included, which takes in [the fix that redraws clusters and scrolled-back cells when their glyphs swap in]. Eviction at the store limit damages nothing. The C reference's flow is the floor and is kept in view: libtsm stamped cell and line ages, and kmscon redrew every cell on any font change, rasterising each glyph lazily in the draw path. vtium's asynchronous arrival damages only the cells showing the arrived characters, a listed departure."
    via: the same issue, comment of 2026-10-28 headed "The operator decided that glyph swap-in is a damage source"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "The console falls back per character from the loaded set to the built-in font for the cell size (unifont from 16 px, Unscii-8 under 16): the set's strikes first, then the built-in, then the replacement glyph. A frit names `builtin:` only to change that order, and the curated frits drop their unifont and Unscii-8 sources."
    via: the GitHub issue for the per-character console fallback, comment of 2026-10-28 headed "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "With `font-size=0`, when 16 px cells would leave fewer than 80x25 cells, vtium uses the set's smallest height: 320x240 draws 40x30 cells of 8x8, 480x320 draws 60x40, 640x400 and up are unchanged, and a CJK locale stays at 16."
    via: the GitHub issue for the font size at the 320x240 floor, comment of 2026-10-28 headed "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "HarfRust is part of the glyph engine's flow, since international text doesn't work without shaping."
    via: the GitHub issue for text shaping with HarfRust, comment of 2026-10-28 headed "The operator's decisions (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "Yes: on a cold boot the built-in font draws at once and the configured font replaces it as it is generated; this also gives vtium a path that works very early in boot."
    via: the team lead's message of 2026-10-28, relaying the operator's answer to the draft ADR-1201's probe on the cold start, folded into this record; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "Keep the console: source: the console, which opens the VT as root already, reads the kernel's font once at start (the kernel returns a plain bitmap) and hands the bytes to the helper, which parses them unprivileged. compress= dropped stands."
    via: the team lead's message of 2026-10-28, relaying the operator's answer to the draft ADR-1201's probe on the kernel's font, folded into this record; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "Light hinting looks less heavy-handed and is likely correct, and it stays the default."
    via: "the maintainer in the steering session, 2026-11-02, relayed by the team lead, on light hinting, after looking at the before and after cards of the helper's hinting work; it stands beside the light-by-default decision of 2026-11-01"
    paraphrase: true
  - evidence: "the work that replaces the prerendered store with deferred rendering, Phase 1's measurements (feat/deferred at d075494), accepted by the operator on 2026-10-28: from vtium's start to the helper's first ask 51-55 ms in the tier 3 guest (1 vCPU, 512 MB); first batch 6-18 ms and whole coverage 29-39 ms for DejaVu Sans Mono there, 31-43 ms and 265-276 ms for WenQuanYi Micro Hei; the helper's peak 5.8 MB and 13.8 MB; on the development machine 2 ms and 16-23 ms with a 12 MB peak for DejaVu Sans Mono, 50-61 ms and 409-537 ms with a 43 MB peak for Noto Sans CJK; the first frame drawn in the built-in font 60 ms after start; the console's resident memory from 5.6 MiB to 6.1 MiB with DejaVu's glyphs"
  - upstream: "aaronsb/kmscon b1f7a7c5 and aaronsb/libtsm eeaf293, the C reference's font load, automatic size, resize and zoom, cluster table, glyph cache and damage, each cited by file and line in this record's C notes"
  - precedent: ADR-1301
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Agreed: vtium draws its built-in set and logs the create command. The built-in set is the template vitrified set. A distribution package's hook (AUR, deb, rpm and the like) runs create to build at least the default set at install and upgrade. Later work may find installed fonts that match chosen profiles and build more sets, as a direction, not yet a decision."
    via: Claude Code session, 2026-10-28, relayed by the team lead
    covers: [who builds]
  - operator: aaronsb
    said: "allow-font-ops is on by default. The config file and docs/config.md describe the key in a comment: the reason it is on by default (xterm's convention, and that it can only switch among built sets), and what each state does."
    via: Claude Code session, 2026-10-28, relayed by the team lead
    covers: [font operations default]
  - operator: aaronsb
    said: "Outline strikes stay at 8-bit. Displays are good enough now that 4-bit quantization would likely show."
    via: Claude Code session, 2026-10-28, relayed by the team lead
    covers: [4-bit outlines]
  - operator: aaronsb
    said: "The concern with compressed chunks is adding a library to the console, not disk or speed. The console already carries an inflate for the built-in unifont's chunks, so a set may mark chunks as deflated, decoded by that same inflate once at load. The console decompresses only with a decoder it already carries for another reason; no new decompressor enters it, and any format that would need one stays uncompressed."
    via: Claude Code session, 2026-10-28, relayed by the team lead
    covers: [compressed chunks]
  - operator: Aaron Bockelie
    said: "The recipe carries an explicit compress setting, compress=yes or no, defaulting to yes, with the matching [guided builder] template field and builder choice. With yes, create deflates 8-bit and color strikes and leaves 1-bit raw; with no, every strike is stored raw."
    via: Claude Code session, 2026-10-28, relayed by the team lead
    covers: [compressed chunks]
  - operator: aaronsb
    said: "A frit's `cell=` line is where the cell lives until profiles have a file of their own. A profile file takes it over later."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
    covers: [the cell's home]
  - operator: aaronsb
    said: "16 MiB is the store limit below 2 GiB, a 512 MB board included. Measured: the Latin fill used under 1 MB, and a full CJK cell at 8x16 is about 11 MB."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
    covers: [the store's limit on minspec]
  - operator: aaronsb
    said: "After login, the helper runs as the session's user, never root. It can then read that user's fonts. `nobody` stays its account before login. This lands with [the image helper's sandbox work]."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
  - operator: aaronsb
    said: "An explicit `font-size` or `font-name` key wins over the profile, as an explicit key overrides a default elsewhere."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
  - operator: aaronsb
    said: "A zoom steps through the frit's pixel sizes, as the C reference's `increase_step` did. `size=` is the starting pixel height."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
  - operator: aaronsb
    said: "With no font named, vtium draws the built-in font. A per-locale frit can come later as a profile, so ADR-1301's \"else the locale's default frit\" goes."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
  - operator: aaronsb
    said: "A kbd name, a `font-engine=psf` path, or vconsole.conf's `FONT=` that no frit bears becomes a one-source frit made on the fly, so existing console setups keep working."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
  - operator: aaronsb
    said: "The store holds each helper answer as one set buffer. Evicting a glyph compacts: the console rebuilds each touched set without the evicted glyphs, through vtium-font's set writer, keeping 1-bit glyphs 1-bit, and counts the rebuilt bytes. So the limit bounds real memory. The rebuild runs only when an ask arrives with the store full, and it evicts enough of the least recently shown glyphs to cover the overrun in one pass. Two options were rejected: Hiding evicted glyphs without freeing them lets the store outgrow its limit while its count says otherwise. Storing glyphs individually needs a second copy of vtium-font's drawing in the console, against ADR-1301's one engine."
    via: the operator's decision on the GitHub issue that replaces the prerendered store with deferred rendering, 2026-10-28, relayed by the team lead
  - operator: aaronsb
    said: "One side owns each fact: the console's per-boot store is the only record of which glyphs the console holds. The helper keeps what it sent for one request only, an ask or a fill, and skips repeats only within it. A later fill may send again glyphs the console still holds, and the console drops them; that cost is accepted."
    via: the team lead's reading of the operator's design rule (one side owns each fact), 2026-10-28
    paraphrase: true
  - operator: aaronsb
    said: "A frit whose sources are all bitmap (a kbd name, a PSF path, `FONT=` or `console:`) and that states no `cell=` takes the font's own cell, as the C reference did: it drew a PSF at its own size, scaled by whole factors. Before the first frame, the console asks the helper for that frit's cell. It waits a bounded time, about 100 ms, and falls back to the automatic cell with a logged line if no answer comes in time. This is the one case where a frame waits on the helper. It applies only to an unconfigured bitmap font, and ADR-1200's observable carries the exception. The console still never parses a font."
    via: the operator's decision on the same issue, 2026-10-28, relayed by the team lead; the issue's comment of 2026-10-29T01:20Z headed "The operator decided how a bitmap font named without a cell finds its cell (2026-10-28)", posted from the operator's account, the wording agent-written
  - operator: aaronsb
    said: "`size=` in a frit and the `font-size` key are in points, 1/72 inch. The console converts them with the display's pixel density: pixels = points × DPI ÷ 72. DPI comes from the physical size the monitor reports in its EDID. When a display reports none (some panels, projectors, the QEMU display), vtium assumes 96 DPI and logs a warning saying the density is assumed. `cell=` stays in pixels, as an exact override. `font-size` defaults to automatic, as before. An explicit number is points. Outline frits step 1 pt, converted to pixels. All-bitmap frits keep stepping by whole multiples of the cell. This departs from the C reference, which fixed its DPI at 72, so its point size was a pixel height (src/font/font.c)."
    via: the operator's decision on the same issue, 2026-10-28, relayed by the team lead; the issue's comment of 2026-10-28T23:26Z headed "The operator decided what font sizes measure (2026-10-28)"
  - operator: aaronsb
    said: "A `display-dpi` configuration key states the density. When it is set, vtium uses it and logs no warning. Like ADR-1301's `display-depth` and `display-subpixel` overrides, it wins over the measured value, which also covers monitors whose EDID reports a wrong size. The order is: `display-dpi` if configured, then the EDID's measured size, then 96 DPI with a warning saying the density is assumed."
    via: the operator's decision on the same issue, 2026-10-28, relayed by the team lead; the issue's comment of 2026-10-28T23:27Z, "The operator added (2026-10-28)"
  - operator: aaronsb
    said: "4. **Zoom** steps by the source's engine, as the C reference's `increase_step` did: outline sources 1 px, all-bitmap frits by whole multiples of the cell, mixed frits 1 px. 5. **A family name** under `font-engine=pango` or `freetype` with no frit becomes a one-source `font:<family>` frit. 6. **The helper's memory** of what it sent lasts one request. It is a processor that knows only its job; the console, the protected process that consumes and dispatches, decides when a glyph is needed, again if need be."
    via: the same issue, comment of 2026-10-28T23:25Z headed "The operator decided the open glyph questions (2026-10-28)", items 4 to 6; posted from the operator's account, the wording agent-written; item 4's pixel steps became points with the comment of 23:26Z
  - operator: aaronsb
    said: "an OSC 50 switch to a bitmap frit with no `cell=` keeps its wait of up to 100 ms for the helper's own cell, mid-session as before the first frame. A font switch is expected to resettle the screen for a moment. D17's \"the one case where a frame waits\" gains the OSC 50 switch; the ADR-1200 text is amended at the merge point with the other readings."
    via: the same issue, comment of 2026-10-29T14:45Z headed "Decision (operator, 2026-10-29)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    said: "every glyph the helper sends replaces the built-in drawing of its character in each renderer's cache, whether or not the character is on screen when it arrives; only the visible cells are damaged (D7: the built-in draws what the frit has not yet sent)."
    via: the same issue, comment of 2026-10-29T14:46Z headed "Decision (operator, 2026-10-29)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    said: "Accepted: font-engine= ranks with font-name. Tier 3's binary budget rises to about 3 MiB, the old 2,621,440 B figure being arbitrary, and deflate (22,032 B) is cut from the console, revisited after the first cut of heads."
    via: the operator's answers in conversation with the team lead, 2026-10-29, relayed in the team lead's brief; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "The minspec console binary's size budget rises to 3.5 MiB."
    via: conversation with the steering session, 2026-10-31, after comparing vtium's size with kmscon's on this machine
    paraphrase: true
  - operator: aaronsb
    said: "A zoom of an outline frit whose helper doesn't answer within the 100 ms wait uses the built-in font's width for that step, logs one line, and drops a late answer; the next zoom asks again: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "A mixed frit with no cell= takes its first font's cell (an outline first font gives the W-rule cell, a bitmap first font its own size), and later fonts are fitted to it: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Every zoom step of an outline frit asks the helper for the exact cell and waits up to 100 ms: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Glyphs are hinted lightly by default, as the C reference's FT_LOAD_TARGET_LIGHT did; full hinting is a frit or profile option. Not built yet. The cell's W measure stays unhinted."
    via: the maintainer's answers in the steering session's interview, 2026-11-01
    paraphrase: true
  - operator: aaronsb
    said: "Just as a frit assigns sets of characters to fonts, each set also carries how its glyphs use the row's height: emoji never stretch, Arabic gets extra room, Latin letters and digits keep their size, and the line and block characters stretch to fill the taller row."
    via: the maintainer's answers in the steering session's interview, 2026-11-01
    paraphrase: true
  - operator: aaronsb
    said: "The glyph helper's path and the frits' folder become run-time settings, with the build-time values as their defaults; the helper runs as nobody and the configuration is root-owned, so the keys add no new way in, but vtium refuses a helper that is not absolute, not a regular file, or writable by anyone other than root, and logs why. A running console keeps the helper it started with across a package upgrade: it opens the helper once at start and starts every later helper from that open file, because an old console and a new helper could disagree on how they talk; a fix to the helper reaches a console when the console restarts, as a fix to vtium does."
    via: the maintainer's decisions in the steering session, 2026-11-01, relayed by the team lead in the brief for the helper-path work; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "vtium keeps the glyph helper's answer to a frit's own-cell query when it arrives after the wait, and uses it, instead of dropping it. The answer applies from the next frame, and the 100 ms wait stays only as the bound on how long the first frame waits."
    via: the maintainer's decision in the steering session, 2026-11-03, relayed by the steering session in the brief for the work that keeps a late own-cell answer; the wording is the steering session's
    paraphrase: true
  - operator: aaronsb
    said: "Bold italic where the frit names no bold-italic face is the regular glyph emboldened and then slanted."
    via: "the steering session, 2026-11-04; the wording is the steering session's"
    paraphrase: true
  - operator: aaronsb
    said: "Bold and italic glyphs come only when the console asks for them; the background fill draws regular glyphs alone."
    via: "the steering session, 2026-11-04; the wording is the steering session's"
    paraphrase: true
observable:
  - "A glyph arriving for a character shown only in the scrollback view, or inside a cluster, redraws that cell on the next frame"
  - "With the store at its limit, the fill sends nothing more, and an ask evicts the glyphs least recently shown, redrawing nothing"
  - "A helper killed by its CPU limit is started again with its asks resent, at most RESTARTS_MAX times a boot; one that sends a frame it was not asked for is given up for the boot with one log line, and the built-in font keeps drawing"
  - "No file the console or the helper writes holds a glyph, and the console opens no font file"
  - "Tier 3's frit variants on the 1-vCPU, 512 MB guest draw the built-in font first and the frit once the helper answers"
  - Tier 3's 2d-potato-min-psf draws kbd's default8x16, named by a path with no cell, in its own 8x16 cell at 1366x768, as the C reference's goldens do
  - vtium-config report, run as a user on a machine with vtium, prints every part, names the command that reads each part it cannot, and holds neither the hostname nor a monitor serial
  - On a cold start with a CJK frit, the first frame draws in the built-in font in the profile's cell, the frit's glyphs replace it as the helper sends them, no frame waits on the helper for glyphs, and the grid's size never changes when the glyphs arrive. The only frames that wait are the first frame of a frit with no cell= line, or of an outline frit whose cell= a configured font-size overrides, at the start or after an OSC 50 switch to it, and the frame after an outline frit's zoom or relayout reaches a height whose cell it has not yet asked for. Each waits at most OWN_CELL_MS (100 ms) for the helper's answer. When none comes in time, a zoom step takes the built-in font's width at its new height and any other such frame draws in the display's cell, each with one log line
status: accepted
date: 2026-10-28
deciders:
  - aaronsb
related: []
---

# ADR-1200: Glyphs are generated at run time in a sandboxed helper, from a profile's frit

## Summary

- **Decided:** vtium generates at run time every glyph that its built-in fonts do not hold, on every build, minspec (the smallest build) included. A sandboxed helper process generates the glyphs from the frit (a font set file) that a profile names. A profile, which `vtium-vitrify` builds, states a frit, its cell (the size in pixels of one character cell) and its options. The console reads the profile as text before its first frame, and it parses no font. The built-in fonts, Unscii-8 for cells under 16 pixels high and unifont from 16 pixels, draw the first frame at once in that cell, and they stay the fallback for each character the frit lacks. The console asks the helper for the characters the screen shows, and each such request is an ask. While no ask is waiting, the helper generates the rest of the characters that the frit covers, one batch at a time, and before each batch it answers any ask that has come in. Generated glyphs live in a memory store that lasts one boot, and the store's limit comes from the machine's memory. The fill stops at that limit, and only an ask evicts glyphs, the least recently shown first. The store finds a glyph by its cluster's text (a cluster is one character as the reader sees it, which can be several codepoints), whether it is bold or italic, its width in cells, and the cell size it was drawn at. When glyphs arrive, the console marks the cells that show those characters for redrawing, the same way it marks the cells that program output changes; the record calls such marked cells damage. It merges all the arrivals between two frames into one redraw, and redraws only those cells. The grid never resizes because glyphs arrived. The console starts the helper on the first need, and the helper exits when it is idle. The console restarts it after a crash or after it reaches its CPU limit, a bounded number of times a boot, and gives it up for the boot when it answers garbage. Nothing is prerendered, and no glyph outlives the boot.
- **Trades away:** Every boot parses and rasterizes its fonts again: generating a CJK frit's whole coverage took 265-276 ms in the tier 3 guest with one virtual CPU, and the helper used 13.8 MB while it worked. The first frames after a start, a zoom or a hotplug draw in the built-in font. The console checks only that the helper's glyphs are well-formed; unlike the store this record first decided, no root-owned file of glyphs is checked by hash. Glyphs cannot be built on one machine for another.
- **One-way?** No. The pipeline lives in vtium and `vtium-vitrify`, and a session of work can change it. Once the last step of the deferred-rendering work removes the prerendered store's code, bringing the store back is a project of several days.
- **Probes:**
  - *Not confident (the cell's home):* you decided that the profile states the cell. Profiles have no file of their own yet, so the pipeline built on `feat/deferred` reads the cell from a `cell=` line in the frit. Is the frit the right home for the cell until profiles have one? (Answered 2026-10-28; see `considered`.)
  - *Not confident (the store's limit on minspec):* the glyph store's limit follows the machine's memory as the scrollback's line cap does: 16 MiB below 2 GiB, 64 MiB from 2 GiB, 128 MiB from 4 GiB. On a 512 MB board, a 16 MiB glyph store lets glyphs use as much memory again as minspec's whole peak of 16 MiB, which the scrollback's budget is sized to fit under. Keep 16 MiB there, or take a smaller share? (Answered 2026-10-28; see `considered`.)
- **Inversion:** The design sits between two extremes. One is "every glyph built ahead of time into a root-owned store", which this record first decided. The other is "the console loads its font before the first frame and rasterizes in its root process", which was the C reference's flow. vtium generates glyphs at run time, as the C reference did, but in a sandboxed process instead of the root process, and it draws the built-in font while the helper works instead of waiting for it.

> Edited 2026-10-28: the operator replaced the prerendered store with run-time generation ("the prerendered store is replaced by deferred rendering"), and this record was rewritten in place to decide the glyph pipeline, with the draft ADR-1201 folded in. It was titled "Fonts reach the console as vitrified sets from a root-owned store", and its Summary decided that every font reached vtium as a vitrified set that `vtium-vitrify create` built into a root-owned store, checked by owner and SHA-256 when loaded, with generation at run time as a per-boot cache beside it. Its four probes, *Confident (who builds):*, *Not confident (font operations default):*, *Not confident (compressed chunks):* and *Not confident (4-bit outlines):*, were asked of that design and answered on 2026-10-28 (`considered`). The basis entries that quote the store's design stand as said; the design they describe is under Alternatives Considered. The earlier text is in git at 64745e2.

> Note 2026-11-02 (the rename): ADR-1201 D12 renamed the font tool vtium-vitrify to vtium-config. A user runs it as `vtium config`, the console starts its glyph helper as `vtium-config serve` at `/usr/bin/vtium-config` by default, and the console's build sets that path with VTIUM_FONT_HELPER where it set VTIUM_VITRIFY. This record keeps the names it was written with.

> Edited 2026-11-04: a plain-language pass rewrote this record's prose under AGENTS.md's writing rules, and a cold reader's list of the sentences it still had to read twice was worked through after it. It changed wording only: every decision and every Summary line keeps its meaning, and nothing was added or dropped. The dated notes keep the text they quote. Where the Alternatives quoted the operator on why the prerendered store went, they now point to the Context, which gives the reason in plain words. The earlier text is in git at d20074b1.

## Context

The issue that tracks vtium-vitrify proposed vitrifying fonts, which means building them ahead of time into a form ready to draw, so that the console would carry no font discovery and no rasterizer. This record first decided a root-owned store of such prerendered sets. ADR-1301 then let one glyph engine run either ahead of time or at run time, so that a set built ahead of time was a saved copy of what the engine draws from the frit. The operator replaced the store and its machinery (a manifest, five commands, target matching and package hooks) with run-time generation, because vtium cannot prerender every font, size, cluster and display that a machine meets, and because the deferred pipeline is how vtium answers a performance problem. Phase 1 of the deferred-rendering work built that pipeline on `feat/deferred` and measured it (Evidence below).

vtium draws through KMS/DRM on machines as small as a Cortex-A53 with 512 MB of memory and a 320x240 panel. The console runs as root for its whole life (ADR-1400). The renderer keeps its own cache of drawn glyphs, keyed by text, style and width (ADR-1300, `crates/vtium-render/src/cache.rs`).

## Decision

The figure follows a glyph from the profile to the frame: the profile's cell and frit, the first frame in the built-in font, the asks and the fill that go to the helper, the helper's glyphs entering the per-boot store, the swap-in, the damage and the next frame. The decisions keep the numbers that code and other records cite, and they appear in the order a glyph travels. D3 and D12 are retired.

![The glyph pipeline. On the left, a profile names a frit and states a cell, read as text. The cell sizes the grid and the built-in font, which draws the first frame at once. The console sends asks for the screen's characters, and fill requests while nothing is asked, to the sandboxed helper, which reads, shapes and rasterizes the frit's fonts and sends glyph chunks back. The console checks each chunk and puts it in the per-boot store, which has a limit: the fill stops at it, and only an ask evicts. Each arrival is damage on the same path as program output, merged into one pass, and the next frame draws the cells that show the arrived characters. The helper starts on the first need, exits when idle, and restarts after a crash a bounded number of times a boot.](figures/ADR-1200-glyph-pipeline.svg)

### D16: profiles

- A profile describes a display's capabilities, the four fields that ADR-1301 D3 reads: depth, subpixel order (with the display's rotation folded in), density and resolution. Its user names it freely. It also holds the frit, the cell and the vtium options that suit that display, such as `font-antialias`, `display-depth` and `display-subpixel`. vtium ships example profiles with descriptive names, and the demonstration set's specimens are drawn once for each profile (`docs/specimens/`).
- The console reads a profile's settings as text before its first frame, as it reads its configuration.
- **A head names a profile, and a profile names a frit** (ADR-1401 D1, D3). A head is a terminal for one or more displays. Profile files live where ADR-1401 places them, and `vtiumctl` writes them when asked (ADR-1401 D12). Until profiles have a file of their own, the cell is the frit's `cell=` line (D1), and the profile's file takes it over when it lands (the operator, 2026-10-28).
- **The default profile.** vtium's compiled-in configuration holds a default profile, which names the default frit. A head that names no profile takes the default profile (ADR-1401 D3, D6). The default frit has no `source=` line, so it names no font, and a head with the default profile falls through to the next choices in D9's order: the configuration's `font-name`, else vconsole's font where one is set, else the built-in fonts (D7).
- **A profile that fails** falls back, and so does a frit, each on its own: a head that names an invalid profile uses the default profile, and a profile that names a frit that does not exist draws the default frit. vtium logs a line for each case (ADR-1401 D6, D7).
- **vtium never writes a profile or a frit** (ADR-1401 D6).
- **The more specific setting wins** (the operator, 2026-10-29): the configuration file's `font-name` and `font-size` are the base for every head, and a head's profile overrides them for that head. A head's font has one path, from the head to its profile to the profile's frit, and the head file sets no font key of its own (the operator, 2026-10-30; ADR-1401 D3). `font-name` chooses the frit (D9), and `font-size` chooses the cell (D17). A profile overrides only what it states. The default profile states no size, and its default frit names no font, so a head with the default profile takes the configuration's keys. A level is one place that sets fonts, such as the configuration file or a profile. Within one level, a `font-size` wins over the cell of the frit that the same level names. `font-engine` ranks with `font-name`: vtium reads a `font-engine` at one level together with that level's `font-name` (the operator, 2026-10-29). `crates/vtium/README.md` states the rule as built before profiles have a file: "At start the font is `font-name` when a file or the command line sets it, else vconsole's [...]. Profiles have no file yet, so no profile names a frit.", and "A configured `font-size` wins over the frit's cell and size (ADR-1200 D16); `font-size=0` does not."
- The C reference had no profiles: its font came from kmscon.conf's `font-name`, `font-size` and `font-engine`.

> Edited 2026-10-29: the operator decided that `font-engine` ranks with `font-name` (`considered`), which `crates/vtium/README.md` left unranked against a head's profile.

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the more specific setting wins, which reverses the operator's answer of 2026-10-28 (`considered`) that an explicit key wins over the profile. The bullet read: "An explicit `font-size` or `font-name` in the configuration wins over the profile, as an explicit key overrides a default elsewhere (the operator, 2026-10-28): `font-size` over its cell (D17), `font-name` over its frit (D9). A key in the configuration applies to every head (ADR-1401 D3)." The default frit drew "vconsole's font where one is set, else the built-in fonts".

> Edited 2026-10-30: the operator gave a head's font one path, head to profile to frit (ADR-1401's `considered`). The bullet read: "a head's profile overrides them for that head; and a `font-name` or `font-size` in the head's own section of the head file overrides both (ADR-1401 D3)." The heads first cut still parses a head's `font-name` and `font-size`; their removal is not built yet.

> Edited 2026-10-29: ADR-1401, accepted by the operator, gives profiles their place: a head names one, the compiled-in configuration holds the default profile and its default frit, an invalid profile falls back to the default, and vtium writes neither profiles nor frits. D16 read: "Where profiles are kept, and the commands that write them, are [the guided builder's, the target-display work's and the rule checker's]."

### D1: the frit

A frit (a font set file) is a file named `<name>.vtfrit` in the frits' directory, which is `/etc/vtium/fonts` unless the `font-frits` key names another. It is the definition that the helper draws from, written in vtium.conf's line syntax: one `key=value` a line, `#` comments, and a bound on the length of every line and of the file. vtium refuses the whole frit at an unknown key or a malformed line, and names the file and the line. A frit written by hand, a frit that `new` writes (D4) and a frit written from a `new --template` file have the same format.

> Edited 2026-11-02: the maintainer moved vtium's configuration file to `/etc/vtium/vtium.conf` (ADR-1500 D4), so this record names it `vtium.conf` wherever it read `kmscon.conf` as vtium's file, and `vtium.conf.example` for the example vtium ships. The record's other text is unchanged.

| Key | Meaning |
|---|---|
| `name=` | The frit's name. Unset, it is the file's name without `.vtfrit`. It is 1 to 64 characters of `A-Z a-z 0-9 . _ + -`, does not start with `.`, and is matched case-sensitively. `unifont` and `unscii-8` are reserved. |
| `source=` | A font the frit draws from. It is repeatable, and the order matters: each later source fills only the codepoints that the earlier ones lack. An outline source is drawn at the cell's size (D17). A bitmap source is scaled by the largest whole factor that fits the cell and placed on the cell's bottom row, and a bitmap source that fits at no whole factor fills nothing. |
| `cover=` | Repeatable, for the `source=` line above it: it limits that source to a named character set (`ascii`, `latin-1`, `gb2312`, `gbk`, `big5`, `jis-x-0208`, `ks-x-1001`) or to a range such as `U+4E00-U+9FFF`. Unset, every glyph that the source holds is in the frit's coverage. |
| `unimap=` | The Unicode map for the `source=` line above it: a kbd unimap name, which vtium resolves as `setfont -u` resolves it, or an absolute path. |
| `cp-size=` | For a CPI source above it: `8`, `14` or `16`. |
| `style=` | For the source above it: `regular`, the default, `bold`, `italic` or `bold-italic` (D21). |
| `cell=` | `WIDTHxHEIGHT` in pixels, each side 6 to 96: the cell, until profiles have a home of their own (D16). |
| `size=` | An outline frit's starting size, in points of 1/72 inch. vtium draws it at points × DPI ÷ 72 pixels at the display's density, and a zoom steps from it by 1 point (D17). vtium ignores a bitmap frit's `size=`. |
| `glyph-source=`, `glyph-source-<range>=` | The per-font sprite policy: for the character ranges that vtium also draws as sprites (box drawing, block elements, Braille, sextants, octants and legacy computing shapes), it chooses whether each font's glyphs or vtium's sprites draw them. It takes vtium.conf's key names and values, and vtium.conf's keys override it. |
| `line-spacing=` | Height added to each row, as a whole percentage of the text's height, from `0`, the default, to `100`, split evenly above and below the text (ADR-1302 D13). When vtium uses a frit's `cell=` exactly as written, line spacing adds nothing. It does apply when a configured `font-size` sets the size over that `cell=`: the cell then grows by the spacing, split evenly above and below each row. vtium.conf's `font-line-spacing`, which a profile may also set (D16), overrides it for the console. Not built yet. |
| `fit=` | Repeatable, for the `source=` or `cover=` line above it: how that set's glyphs use the row's height when spacing is added (ADR-1302 D13). `fill` stretches them to the whole cell, so that they join from row to row. `natural` keeps the font's size on its baseline and leaves the added height blank around it. `room` keeps the letter as `natural` does and lets its marks use the added height. It is one key with three values, not a separate key for marks, because a set uses the row in one way only. Unset, line and block characters are `fill`; Arabic, Hebrew, Syriac, Thaana, N'Ko and combining marks are `room`; and the rest is `natural`. Color glyphs are always `natural`. Not built yet. |
| `license=` | An SPDX identifier. Optional. |
| `hinting=` | How the helper hints the frit's outline glyphs, one key for the whole frit (the maintainer, 2026-11-01). Hinting moves a glyph's outline onto the pixel grid so that it draws sharper. `light` is the default, which the maintainer found less heavy-handed after seeing both (2026-11-02, `basis`). `light` snaps each glyph to the pixel grid vertically only, as the C reference's FreeType engine hinted every glyph with `FT_LOAD_TARGET_LIGHT` (`kmscon/src/font/font_freetype.c:119`). The helper gives a TrueType font to skrifa's automatic hinter, which moves rows only, and keeps a CFF font's own hints, as FreeType did for a light load. `full`, skrifa's Normal target, runs a TrueType font's own instructions, or the automatic hinter at full strength for a font that has none, so a TrueType glyph can also move sideways onto whole columns. A CFF font's hints move rows only, so `full` and `light` draw a CFF font the same, as in FreeType. Bitmap sources take no hinting. The helper builds both values. vtium.conf's `font-hinting`, which a profile may also set (D16), is meant to override the key for the console, as `font-antialias` overrides `antialias=` (ADR-1301 D4). That is not built yet, because no request to the helper carries the console's setting. |

> Edited 2026-11-03: the maintainer decided that line spacing applies when a configured `font-size` overrides a frit's `cell=` (ADR-1302 D13). The `line-spacing=` row read: "It adds nothing to a stated `cell=`."

> Edited 2026-11-01: the maintainer added line spacing and per-set fitting of the row's height (`considered` in ADR-1302, 2026-11-01). The table had no `line-spacing=` or `fit=` row; the earlier text is in git at 38ee007.

> Edited 2026-11-02: the helper's side of `hinting=` is built, on the branch that adds hinting to the helper (`crates/vtium-vitrify/src/frit.rs`, `Hinting`; `src/outline.rs`, the hinter's options), and the row says what is built and what is not. The row ended "overrides it for the console, as `font-antialias` overrides `antialias=` (ADR-1301 D4). Not built yet." and named no hinter. The earlier text is in git at 61a5c41. Corrected the same day after the fonts review: a CFF font draws the same under both values, where the row said of `full` "and a CFF font's hints, so a glyph can also move sideways onto whole columns"; the earlier text is in git at 04de5a0.

> Edited 2026-11-01: the maintainer decided that glyphs are hinted lightly by default and fully on request (`considered`, 2026-11-01). The table had no `hinting=` row, and the helper's hinting was not stated; the earlier text is in git at 292d0b5.

`compress=` chose how a stored set was compressed, and it now has nothing to act on. vtium accepts a frit that carries it, with one warning that names the file and the line, because refusing the frit would leave the console on the built-in font for a key that never changed what was drawn.

The source forms:

- `font:<family>`: a font family, matched as fontconfig matches one, ignoring case and blanks: first a kbd console font of that name, else an installed outline font of that family. It names a family and nothing else (the operator, 2026-10-28). A frit that names its fonts this way works on any machine that has the family installed.
- `kbd:<name>`: a kbd console font, found as `setfont` finds it, in the kbd data directories (`/usr/share/kbd`, `/usr/share`, `/usr/lib/kbd`).
- `file:<absolute path>`: a PSF1, PSF2, CPI or raw font, plain or compressed with gzip, bzip2, xz or zstd.
- `bdf:<absolute path>`: a BDF bitmap font, plain or compressed. vtium-vitrify reads a BDF font only when its character set is ISO 10646-1, whose encodings are Unicode codepoints. It takes the character set's registry and encoding from the `CHARSET_REGISTRY` and `CHARSET_ENCODING` properties. Where a property is missing, it takes them from the last two fields of the XLFD name on the `FONT` line, such as `iso10646-1`; Tiny5 Mono's BDF file names its character set only there. A glyph with `ENCODING -1` has no codepoint, so vtium-vitrify leaves it out and does not count it against the font's `CHARS` line. Tiny5 Mono's `CHARS` line leaves such glyphs out of its count.
- `vconsole`: `FONT=` and `FONT_UNIMAP=` from `/etc/vconsole.conf`, with the kernel command line's `vconsole.font=` and `vconsole.font_unimap=` over them.
- `console:<vt>`: the font the kernel holds for that VT (D8).
- `builtin:unifont`, `builtin:unscii-8`: a built-in font as a source, named only to change the fallback order (D7).
- `outline:<absolute path>`: an outline font, rasterized by swash (D14).

A font with no Unicode table takes the map that its frit's `unimap=` names, else kbd's `def.uni`. When it has neither, the source fails with a message that names `unimap=`.

When a source is not on this machine, the helper leaves it out quietly, because a frit may name fonts that a machine does not install. When a source is on the machine and the helper refuses it, such as a BDF file that its reader refuses, the helper leaves it out with one warning that names the frit, the line and the reason. The console logs that warning once a boot, as it logs the frit's other warnings, such as the one for `compress=`. A frit whose sources are all left out fails, and the built-in font draws (D7).

> Edited 2026-11-02: the work that adds Tiny5 Mono as an example frit has vtium-vitrify read a BDF font's character set from its XLFD name and leave out glyphs of `ENCODING -1`, and has the helper warn of a source it refuses (`crates/vtium-vitrify/src/bdf.rs`; `src/serve.rs`, `Engine::load`). The `bdf:` form read "a BDF bitmap font, plain or compressed.", and nothing said what the helper did with a source it refused while the frit's other sources loaded: it left the source out with no warning.

```ini
# /etc/vtium/fonts/zh-cn.vtfrit
cell=12x24
source=outline:/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc
cover=ascii
cover=gb2312
source=bdf:/usr/share/fonts/fusion-pixel/fusion-pixel-12px-monospaced-zh_hans.bdf
```

### D4: `vtium-vitrify`, the profile builder

`vtium-vitrify` is a binary of its own in the install tree, apart from `vtium`. It holds every font decoder, parser and rasterizer, and the shaper, outside the console and outside the console's size budget.

| Command | Runs as | Does |
|---|---|---|
| `new FONT`, `new --template`, `new --from FILE` | the user, with `sudo` once at the end | The guided builder and its templates walk the user from a font to a profile and produce a frit. At the end the builder shows one command, run with `sudo`, that writes the frit. |
| `preview FONT...` | the user | Draws a sample of a font as the helper would draw it for a profile, and writes nothing. |
| The rule checker, a library | `vtiumctl check`, as the user | Lints a profile and its frit against a display (ADR-1301 D8). `vtiumctl check FILE` checks each configuration file by its kind, and it checks frits through vitrify's frit parser and this checker (ADR-1401 D12). |
| `set NAME` | the session's user | Switches the console's frit (D5). |
| `serve` | the account D15 names | The helper (D15). |
| `report` | the user, or root for the parts only root reads | Gathers what a bug report needs into one document and changes nothing (D24). |

Each command is non-interactive where it writes, prints one line for each result, and exits 0 on success, 1 on a failure and 2 on a usage error. The tool shows its banner only at an interactive terminal. The tracker issue for vtium-vitrify follows the tool's work.

> Edited 2026-10-29: the team lead's correction from the heads gap-check (C9): ADR-1401 D12's `check FILE` and the rule checker are one command. The row read: "| The rule checker | the user | Lints a profile and its frit against a display (ADR-1301 D8, [the rule checker's issue]). |"

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the configuration tool is `vtiumctl`, a binary of its own, and `vtium-vitrify` keeps the fonts. The row read: "| `check FILE`, with the rule checker | the user | Checks any configuration file by its kind, a head file, a profile or a frit, as ADR-1401 D12 has it, and lints a profile and its frit against a display (ADR-1301 D8, [the rule checker's issue]). |"

### D11: the frit drawn when nothing names one

> Edited 2026-10-28: D11 chose the sets a machine held and their sizes: per locale, one bitmap set scaled by whole factors and at most one outline size for the display, built by `create --default` from a package hook, with their disk costs. With nothing built ahead of time, that choice is retired, and its figures are under Alternatives Considered.

With no font named, a head takes the default profile and its default frit (D16), which draws vconsole's font where one is set, else the built-in font (D9). vtium picks no frit by locale (the operator, 2026-10-28). Frits chosen by locale can come later, through profiles.

> Edited 2026-10-29: the default profile and frit of ADR-1401 D6 take this place (D16). It read: "With no font named, vtium draws the built-in font (D9), and picks no frit by locale".

### D17: the cell

- **From the profile, before the first frame.** The console reads the profile's cell as text before it draws, so the grid has its size from the first frame. The console does not wait on the helper for it, and it parses no font.
- **A frit with no `cell=` line draws at its fonts' own cell** (the maintainer, 2026-11-01): the cell size that the fonts themselves give, as the two bullets nested below say for each kind of font. The first of the frit's sources that this machine has sets it. When the helper loads the frit, it leaves out each source that does not resolve or cannot be read (`crates/vtium-vitrify/src/serve.rs`, `Engine::load`), and when it answers for the cell, it leaves out each source that has failed while it drew (`Engine::own_cell`). So when the first source is an outline font that is missing, the next source that the machine has sets the cell instead, by the rule for its own kind, bitmap or outline.

  There is one exception. When that first source is a bitmap font and a later bitmap source's cell does not fit inside the first source's cell, the helper refuses the frit's own-cell query and names the source that does not fit. The helper refuses only the question about the cell, not the frit. It goes on serving the frit's glyphs at the display's cell, the cell that the automatic rule gives for the display, and the console uses that cell, as it does after any refusal (below).

  The console parses no font, so it cannot measure the fonts itself. It sees from the frit's text that no `cell=` is stated, and it asks the helper for the cell before the frit's first frame: at the start, and at an OSC 50 switch to such a frit in the middle of a session (D5), since a font switch is expected to resettle the screen for a moment (the operator, 2026-10-29). The console waits at most `OWN_CELL_MS`, 100 ms, for the answer.

  A frame waits on the helper only for that answer, and when an outline frit reaches a height whose cell it has not yet asked for, since each height has its own cell. That happens in two cases: a zoom (below), and a relayout to a new height, as when a display joins the head or the density changes (`App::relayout` in `crates/vtium/src/app.rs`, on the branch that finishes the console's binary). A bitmap frit asks only at its first frame and at an OSC 50 switch to it, and its answer, or its refusal, holds at every height.

  A refusal, or no answer in time, leaves the display's cell in use, with one logged line. An answer that arrives after the wait is kept and used (the maintainer, 2026-11-03). The console records it for the frit, at the height it asked at, in place of the missing answer, and logs one line. From the next frame, each head that draws the frit and takes its cell from that answer draws in that cell, and its grid is laid out again. A refusal that arrives after the wait is kept, as one that arrives in time is, so later switches to that frit do not ask again. Until an answer arrives, an OSC 50 switch back to the frit asks again. The 100 ms is only the bound on how long a frame waits.

  Later sources draw into the cell that the first source sets (the maintainer, 2026-11-01). Where the first source is an outline font, a later bitmap source that fits that cell at no whole factor fills nothing, as D1's `source=` row says, and the built-in font draws its characters. Where the first source is a bitmap font, that case never arises, because the exception above refuses the query instead. The frit then draws in the display's cell, and there too each bitmap source whose cell does not fit fills nothing.
  - **A bitmap first source** (a kbd name, a PSF path, `FONT=`, `console:` or a built-in) gives its fonts' own cell, as the C reference drew a PSF font at its own size, scaled by whole factors (the operator, 2026-10-28). The console asks for it only when no `font-size` is configured. The helper answers with the first source's cell when every other bitmap source's cell fits inside it, and otherwise refuses the query, naming the bitmap source whose cell does not fit. The console takes the largest whole multiple of that cell, at least 1, whose height is not over the automatic rule's height. A zoom steps by the cell's height.
  - **An outline first source** gives its cell by the W rule (the operator, 2026-10-26; the maintainer, 2026-11-01): the cell's height is the font's size, and its width comes from the letter W at that size. The height is the one that the configured `font-size` names, else the frit's `size=`, else the automatic rule, in that order, since a configured `font-size` wins over the frit's (D16, `crates/vtium/src/font.rs`). The width is the wider of two measures of W at that height: its advance (how far the pen moves after it) and its ink. Both are measured from the unhinted outline, whatever `hinting=` says (D1), and from the bold face too where the frit names one, as the C reference took the larger of its regular and bold faces' cells. An outline frit that states `cell=` while a `font-size` is configured takes the configured height and the W rule's width, as a frit with no `cell=` does, since the configured `font-size` wins (D16); that was the steering session's call under the maintainer's rules. A zoom steps by 1 point (the zoom bullet below) and asks the helper for the exact cell at the new height, with the same wait (the maintainer, 2026-11-01). When no answer comes in time, that zoom step takes the built-in font's width at the new height, with one logged line, and the next zoom asks again (the maintainer, 2026-11-01). When the answer comes, it is kept and used, as the bullet above says (the maintainer, 2026-11-03).
- **With no profile,** the cell comes from the display, by the automatic rule: a cell half as wide as it is high, at most 120 columns, at least 40 rows, and never under 16 pixels high. The small-display rule takes precedence: where 16-pixel cells would leave fewer than 80 by 25 cells, vtium uses the built-in font's smallest height, so a 320x240 display draws 40 by 30 cells of 8x8, and a 480x320 display draws 60 by 40. A zh, ja or ko locale stays at 16. The cell's width is the built-in font's width at that height.
- **Sizes are points** of 1/72 inch (the operator, 2026-10-28). A configured `font-size` and an outline frit's `size=` draw at points × DPI ÷ 72 pixels, rounded to the nearest pixel. The density, in DPI, is `display-dpi` where it is configured, and then vtium logs nothing. Else it is the display's own, measured from the physical size that the display reports (ADR-1301 D3). Else it is 96 DPI, with one warning for each display that reports no size. Where several displays share one grid, as when a head is mirrored across them (ADR-1401 D1), the lowest density among them converts the points, and a larger display draws larger, by the layout rule. Points do not apply to bitmap fonts: an all-bitmap frit takes its stated cell or its fonts' own cell.
- **Line spacing** (not built yet) adds height to a cell that vtium works out: the fonts' own cell, the automatic rule's cell, or the cell that a configured `font-size` gives a frit that states `cell=` (ADR-1302 D13; the maintainer, 2026-11-03). It adds nothing to a frit's `cell=` that vtium uses exactly as written. `line-spacing=` and `font-line-spacing` add a whole percentage of the text's height, split evenly above and below the text, and the grid takes fewer rows (ADR-1302 D13; the maintainer, 2026-11-01). The W rule's width does not change.
- **`cell=` and `size=`:** `cell=` is the cell in pixels, an exact override, and it wins over the frit's `size=`. `size=` is an outline frit's starting size in points, and it gives the cell its height where no `cell=` is stated.
- **A configured `font-size`** sets, in points, the height that the automatic rule would otherwise pick. A profile's cell overrides the configuration file's `font-size` for the profile's head (D16).
- **The grid resizes only on an explicit event:** a zoom, or a display hotplug while the cell is automatic. A resize sets the pty's size, which sends SIGWINCH, and redraws. The console asks for the glyphs for the new cell, and the built-in font draws meanwhile. Arriving glyphs never change the cell.
- **The helper generates glyphs for the cell and never changes it.** A frit that cannot fit the cell fails with a logged reason, and the built-in font keeps drawing. `vtium-vitrify` lints a profile whose cell does not suit its frit when it builds or checks the profile, and so does `vtium check-config`.
- **A zoom's step depends on the kind of font the frit draws,** as the C reference's zoom stepped by its engine's `increase_step` (C notes), and a zoom starts from the frit's `size=` (D1) (the operator, 2026-10-28). An outline frit, and a frit that mixes outline and bitmap sources, steps by 1 point: to the next whole point size that gives a different size in pixels at the display's density. An all-bitmap frit steps by whole multiples of its cell.

The C reference loaded its font before its first frame and took the font's cell. vtium knows the cell just as surely, from the profile, without waiting on a font. It waits only for the cell of a frit that states none, and only for a bounded time.

> Edited 2026-11-03: the maintainer decided that line spacing applies when a configured `font-size` overrides a frit's `cell=` (ADR-1302 D13). The Line spacing bullet read: "**Line spacing** (not built yet) adds height to a cell vtium works out, the fonts' own or the automatic rule's, never to a stated `cell=`: ..."

> Edited 2026-11-03: the maintainer decided that vtium keeps a helper's answer to the own-cell query that arrives after the wait, and uses it from the next frame (`considered`, 2026-11-03). The general bullet's sentence read: "A refusal, or no answer in time, leaves the display's cell with one logged line, and a later answer is dropped." The outline bullet's zoom ended: "with one logged line, a later answer is dropped, and the next zoom asks again (the maintainer, 2026-11-01)."

> Edited 2026-11-01: the maintainer decided that an outline frit that states no cell takes its fonts' own cell by the W rule, asked of the helper before its first frame, as a bitmap frit's is (ADR-1302, the maintainer's answers of 2026-11-01). Such a frit drew in the built-in font's cell at its height (`crates/vtium/src/font.rs`, `frit_cell_at`). The first bullet read: "A frit of bitmap sources that states no cell takes its fonts' own cell, as the C reference drew a PSF font at its own size, scaled by whole factors (the operator, 2026-10-28). The console sees from the frit's text that every source is a bitmap one (a kbd name, a PSF path, `FONT=`, `console:` or a built-in) and that no `cell=` is stated, and with no `font-size` configured it asks the helper for the cell before the frit's first frame, since the console parses no font: at the start, and at an OSC 50 switch to such a frit mid-session (D5), since a font switch is expected to resettle the screen for a moment (the operator, 2026-10-29). The helper answers the first source's cell when every other source's fits in it, and refuses an outline source or a source whose cell does not fit. The console waits for the answer at most `OWN_CELL_MS`, 100 ms, the one case where a frame waits on the helper, at the start or at that switch, and takes the cell at the largest whole multiple not over the automatic rule's height, at least once; a zoom steps by its height. A refusal, or no answer in time, leaves the display's cell with one logged line, and a later answer is dropped." This paragraph ended "and waits only for a bitmap frit's own cell, bounded." Corrected the same day after a review: the outline height's order puts the configured `font-size` first, as D16 has it, and a zoom that gets no answer in time takes the built-in font's width.

> Edited 2026-11-01: the maintainer added line spacing (`considered` in ADR-1302, 2026-11-01), and D17 gains its bullet. The earlier text is in git at 38ee007.

> Edited 2026-11-01: the maintainer decided the hinting of outline glyphs (`considered`, 2026-11-01), and the cell's W measure stays unhinted. The outline bullet read "as wide as the wider of W's advance and W's ink at that height,". The earlier text is in git at 292d0b5.

> Edited 2026-11-02: an outline frit also asks for its cell, and waits up to `OWN_CELL_MS`, when a relayout reaches a height it has not asked at, on the branch that finishes the console's binary (`App::relayout`, `App::own_cell`; `Frit::answer_at`, which keeps a bitmap frit's answer for every height). Corrected the same day after the records re-check: the relayout's wait is not limited to the automatic size, since with `font-size` set a change of density also reaches a new height. The sentence read: "The console waits for the answer at most `OWN_CELL_MS`, 100 ms; this wait, and an outline frit's zoom below, are the only times a frame waits on the helper." The earlier text is in git at 928e9a3.

> Edited 2026-11-01: the steering session's brief for the second wave of records, checked against the helper's code (`crates/vtium-vitrify/src/serve.rs`, `Engine::own_cell`; `crates/vtium/src/app.rs`, which logs the refusal and keeps the display's cell): the exception refuses the own-cell query and not the frit, a missing first source lets the next source the machine has answer whatever its kind, and "fills nothing" applies under an outline first source; the bitmap bullet says the same refusal. The bullet read "so where an outline first source is missing, the first bitmap source after it answers with its own cell", its exception ended "the helper refuses the frit's own-cell query, and the console takes the display's cell, as the bitmap bullet below says.", and its last sentence read "a later bitmap source that fits that cell at no whole factor fills nothing, as D1's `source=` row says." The bitmap bullet read "and refuses a bitmap source whose cell does not fit". The earlier text is in git at 725c369.

> Edited 2026-11-01: the steering session's brief: the general bullet names the source that sets the cell as the first one this machine has, and the one case the helper refuses, which the bitmap bullet already stated and the helper already did. It read: "A frit that states no cell takes its fonts' own cell (the maintainer, 2026-11-01), and its first source sets it." The earlier text is in git at 22dc899.

> Edited 2026-10-30: the operator gave a head's font one path (ADR-1401's `considered`). The `font-size` bullet ended: "and the head's own `font-size` overrides both (D16)."

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the more specific setting wins, and the `font-size` bullet takes D16's order. It read: "A configured `font-size` sets the height the automatic rule would pick, in points, and wins over a profile's cell (D16)."

> Edited 2026-10-29: the operator decided that an OSC 50 switch to a bitmap frit with no `cell=` waits for its fonts' own cell as the start does (`considered`), as the code already did (`crates/vtium/src/app.rs`, `osc_50_to_a_bitmap_frit_with_no_cell_takes_its_fonts_own_cell`); that zoom steps by the frit's engine, a mixed frit as an outline one (`considered`, item 4 of the open glyph questions); and ADR-1401 D1 accepted that the lowest density converts where displays share one grid. The roles of `cell=` and `size=` moved into a bullet of their own. The bitmap bullet read "it asks the helper for the cell before the first frame [...] the one case where a frame waits on the helper, and takes"; the sizes bullet ended "and `cell=` stays in pixels, an exact override."; and the zoom bullet read "A zoom steps through the frit's sizes, as the C reference's zoom stepped by its font's `increase_step` [...] an outline frit by 1 point [...] and an all-bitmap frit by whole multiples of its cell."

> Edited 2026-10-28: the operator decided that sizes are points converted at the display's density, with `display-dpi` stating it (`considered`), which D1's `size=`, this decision and the departures now carry. `size=` and a configured `font-size` were pixel heights, a bitmap frit's `size=` set its height, and an outline frit zoomed by 1 pixel; the earlier text is in git at 9ccffbc.

> Edited 2026-10-28: the operator decided that a frit of bitmap sources with no cell takes its fonts' own cell (`considered`), which this decision, the observable, D10, the departures, the consequences and the alternatives now carry. A bitmap frit with no cell stated drew in the display's cell, and no frame waited on the helper; the earlier text is in git at fa6bbf6.

### D7: the built-in fonts

- **`unifont`:** GNU Unifont 17.0.03 under the SIL Open Font License 1.1.
- **`unscii-8`:** Unscii, by Viznut. vtium uses `unscii-8.hex` from <http://viznut.fi/unscii/unscii-8.hex>, SHA-256 `03094f7fbab7085cf6a6b624cee61e47e71ce5d0c2f308c2f4436afdc17f776c`, which holds 3,191 glyphs, all 8x8 except U+0000, which is 16x8. It is in the public domain: `unscii.txt` in `unscii-2.1-src.tar.gz` (<http://viznut.fi/unscii/unscii-2.1-src.tar.gz>, SHA-256 `4d1215d495e49d005ae70828ec1529e997049b9a7f152b9439d78c7e70c65b57`) says "Everything made by me (viznut) is in the Public Domain.", and only `unscii-16-full`, which vtium does not use, falls under the GPL. vtium pins the published file by its hash and keeps it, with its URL and `unscii.txt`, in vtium-font's `data/`.
- Both fonts are compiled into the binary. Their glyph chunks are stored deflated and are inflated on first use. vtium trusts them without a check, since they are built into the binary. They are the only fonts that the console's own process holds.
- vtium draws `unscii-8` for cells under 16 pixels high and `unifont` for cells from 16 pixels. The exception is a zh, ja or ko locale: there vtium never chooses `unscii-8`, because it holds no CJK characters, and `unifont` serves instead. Unifont covers GB2312, GBK, Big5, JIS X 0208 and all 11,172 Hangul syllables (measured), so a CJK console is readable from its first frame.
- **The first frame:** the built-in font draws at once, in the cell that D17 gives, fitted to that cell as the next bullet says. Its glyphs come at the cell's size, so the renderer stores them as it stores the frit's.
- **How a built-in glyph fits a cell of another size.** vtium picks the built-in strike (a font's glyphs at one size) for the cell. It scales unifont by the largest whole factor that keeps the scaled glyph inside the cell. When even unifont's 8x16 does not fit, it takes the smaller strike, Unscii-8 at its own 8x8, except in a zh, ja or ko locale, where vtium cuts unifont's 8x16 to the cell in the same way. vtium then centers that strike's glyph in the cell and never scales it by a fraction. Where the cell is larger than the glyph, the spare pixels go around the glyph, and an odd spare pixel goes after it, to its right or below it. Where the cell is smaller, vtium cuts the glyph evenly from both sides, and an odd pixel comes off its right or bottom edge, as the C reference cut a glyph wider than its cells (C notes). `tiny5`, the example frit of Tiny5 Mono, shows the narrow case. It is an example at the small end of the cells a frit may state, the narrowest of which is 6 pixels wide, and it is never a default. Its cell is 6x9: Unscii-8's 8x8 glyph loses its first and last columns, keeps its 8 rows, and has a blank 9th row below it. Unscii-8 draws its letters in its first 7 columns and leaves the 8th blank, so cutting one column from each side removes fewer inked columns than any other cut of two columns. Unscii-8's baseline, below its 7th row, lines up with Tiny5 Mono's, which has 7 rows above it. vtium does not scale the glyph to fit. Scaling 8 columns to 6 drops two columns at uneven places, so that a stroke two pixels wide stays two wide in one glyph and becomes one wide in the next, and scaling by coverage turns a sharp 1-bit glyph into a gray, blurred one. The built-in font draws only until the frit's glyphs arrive, and for the characters the frit lacks, so a sharp glyph with a column cut off serves the reader better than a complete glyph that is smeared. The same fit holds in the first frame and for the per-character fallback below.
- **The per-character fallback:** a character that the frit lacks, or has not yet sent, draws in the built-in font for the cell, and failing that as the replacement glyph (D22). A frit names `builtin:` only to change that order, and vtium's own frits carry no copy of unifont or Unscii-8.
- **Where the helper is unavailable,** as when `vtium-vitrify` is not installed, when the console has given the helper up for the boot (D15) or when a frit fails, the built-in font draws everything, and the console logs one line that names the frit and the reason.
- Fusion Pixel 12 is not built in. Compressed, it is 362 KB, against the 192 KB that the binary's budget of 2,621,440 bytes left at the time; the budget has since been raised (D19). And one compiled-in variant would give Japanese users Chinese glyph forms.

> Edited 2026-11-02: the work that adds Tiny5 Mono as an example frit recommends, for the maintainer to read before it merges, that a built-in glyph is centered and cut evenly in a cell narrower than it, and never scaled, as the code already drew it (`crates/vtium/src/deferred.rs`, `DeferredFace::fit`; the test `unscii_8_in_a_narrower_cell_is_centered_and_cut_evenly` in `crates/vtium/src/font.rs`). The fit gets a bullet of its own. The first-frame bullet read: "the built-in font draws at once, in the cell D17 gives: centered when the cell is larger than the built-in's, and in the built-in's smaller strike, cropped where even that does not fit, when the cell is smaller. Its glyphs come at the cell's size, so the renderer stores them as it stores the frit's." The per-character fallback named no fit.

### D9: the font at start, and its fallback

- vtium chooses the font in this order, the most specific first (D16): the frit that the head's profile names, where that frit has a source; else `font-name` from the command line or vtium.conf; else `vconsole.font=` from the kernel command line, then `FONT=` from `/etc/vconsole.conf`; else the built-in font.
- vtium looks a name up as a frit under `/etc/vtium/fonts`. When no frit has that name, a kbd name, a font path given with `font-engine=psf`, a family given with `font-engine=freetype` or `pango`, or vconsole.conf's `FONT=` becomes a one-source frit that vtium makes on the fly, with a `kbd:`, `file:`, `font:<family>` or `vconsole` source (D1), so that existing console setups keep working (the operator, 2026-10-28).

> Edited 2026-10-30: the operator gave a head's font one path (ADR-1401's `considered`). The first bullet began: "The font is, the most specific first (D16): the head's own `font-name` (ADR-1401 D3); unset, the frit the head's profile names,".

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the more specific setting wins, and the first bullet takes D16's order. It read: "The font is `font-name` from the command line or kmscon.conf, which wins over the profile (D16); unset, the frit the head's profile names; unset, `vconsole.font=` [...]".

> Edited 2026-10-29: the operator decided that a family under `font-engine=pango` or `freetype` with no frit becomes a one-source `font:<family>` frit (`considered`, item 5 of the open glyph questions). The bullet read: "A kbd name, a font path given with `font-engine=psf`, or vconsole.conf's `FONT=` that no frit bears becomes a one-source frit made on the fly, with a `kbd:`, `file:` or `vconsole` source (D1)".
- A name that is neither a frit nor a source that the helper can read draws the built-in font, and vtium logs one line with the name and the reason. A console with no font named draws the built-in font and logs nothing.

### D15: the helper

The helper is `vtium-vitrify serve`, a separate process that the console starts. It runs the same reader, shaper and rasterizer that `preview` runs (D14; ADR-1301 D1), so a frit draws the same wherever it is drawn.

- **Sandboxed.** The helper runs with an empty environment, in `/`, in its own process group, and holds only its pipes. It runs with no-new-privileges, so that it can never gain privileges, under a memory limit and a CPU limit over its whole life, and never as root. Before anyone logs in, it runs as `nobody`, the last account in ADR-1400 D2's list for that time, and after login it runs as the session's user, so that it can read that user's fonts (the operator, 2026-10-28; running as the session's user lands with the image helper's sandbox work). It refuses to start as root. It reads the frit and its fonts itself, so a font must be readable by that account; the console's root process parses no font.
- **One helper for each frit in use,** and, once a session has logged in, one for each session user of that frit. vtium never shares a helper between users (ADR-1401 D1).
- **Started on the first need, exited when idle.** The console starts the helper at its first ask or fill. The helper exits after `HELPER_IDLE_MS`, 60,000 ms, with no request and no fill, or when the console closes its end of the pipe and the helper's work is done. The next need starts a fresh helper (ADR-1301 D2).
- **Not persisted.** No state that a hostile font plants survives the process, and the helper's CPU and memory budgets reset with each start. A restart narrows the window in which an exploit can act, but the console's checks and the sandbox are what contain a vulnerability.
- **Restarted after a failure, a bounded number of times a boot.** The console starts the helper again, and sends its asks and fills again, when the helper crashes, reaches its CPU limit, gives no answer for `ANSWER_MS` (10,000 ms) while work is outstanding, leaves an ask unanswered for `REQUEST_MS` (60,000 ms), or exits with work outstanding. It does so at most `RESTARTS_MAX` times a boot, and `RESTARTS_MAX` is 3. Each start re-randomizes the helper's address layout, so an unbounded count would let an exploit retry until its guess lands.
- **Started from the program vtium opened at start.** The `font-helper` key names the helper's program, which is `/usr/bin/vtium-vitrify` unless vtium was built with another path, in the same way that `font-frits` defaults to the directory that vtium was built with. vtium opens the program once, when it starts, and starts every helper from that open file. A package upgrade replaces the program by renaming a new file into place, so the open file stays the old program, and a running console goes on starting the helper it started with. An old console and a new helper could disagree on how they talk over the pipe, so a fix to the helper reaches a console when the console starts again, as a fix to vtium does (the maintainer, 2026-11-01).
- **Refused unless only root, or the account vtium runs as, can change it.** vtium refuses a program path that is not absolute, a file that is not a regular file, a setuid or setgid file, a file owned by an account other than root or the account vtium runs as, and a file that its group or others may write. A setuid-root helper program would run as root even though vtium switches to `nobody` before it starts the program, because the helper turns on no-new-privileges only after it starts. vtium checks the file it opened, so the file it checked is the file that runs. When it refuses, it logs why and draws the built-in font (D7). The configuration files belong to root, so the two keys give no other account a way to choose the program. `check-config` notes a program that vtium would refuse. When an account other than root runs `check-config`, it also notes a program that vtium, started as root, would refuse, since vtium started as root accepts only a file that root owns.
- **Given up for the boot on garbage.** When a helper sends something that is not a frame (a frame is one message on the helper's pipe), a frame that was not asked for, or a failure, the console gives it up for the boot and logs that once, and the built-in font goes on drawing (D7).
- **Checked, never trusted.** The console checks each frame before it allocates anything for it. It checks the frame's length against `FRAME_BYTES_MAX` (16 MiB), and its glyphs through vtium-font's bounded reader, each at a cell that was asked for. The glyphs that answer an ask must be ones that the console asked for and that were not yet sent, and the glyphs of a fill must be ones that the store does not yet hold at that cell size. Each glyph's ink box must stay within its reach: vtium refuses a box that passes more than one cell beyond the glyph's own cells, on either side (ADR-1302 D1, D6). One thread writes the requests and another reads the answers, so neither a slow helper nor a hung one stalls the console's loop.

> Edited 2026-11-01: the maintainer's decision on the helper's path (`considered`, 2026-11-01) adds the bullets "Started from the program vtium opened at start" and "Refused unless only root, or the account vtium runs as, can change it", and D1 names the frits' directory by its key. Until then both paths were fixed when vtium was built, and each start ran the program found at its path then.

> Edited 2026-11-01: glyph ink reaches one cell past its own on each side (ADR-1302, accepted 2026-11-01), so the console's check of an answer also refuses an ink box past that reach (ADR-1302 D6). The bullet ended: "a fill's among those the cell does not hold." The earlier text is in git at 1d51e46.

> Edited 2026-10-29: the team lead's correction from the heads gap-check (C5), as ADR-1401 D1 decides: several consoles logged in as different users cannot share one helper running as one of them, and heads with different frits need a helper each. D15 spoke of one helper, "a separate process the console starts".

### D14: parsing and rasterizing safely

- Every font file is parsed in a process that is not the console: either the helper (D15), or the child process to which `preview` and `new` hand a font. That child drops to an unprivileged account, sets no-new-privileges, holds no file descriptor except its pipes, and runs under a memory limit. A panic, a fault or a timeout in either process fails that one source and nothing else. The font research found swash panicking on Fusion Pixel's OTB file ("chunk size must be non-zero").
- The outline rasterizer is swash. It is the only pure-Rust candidate with hinting (which matters most at 12-16 pixels), embedded bitmaps and color (CBDT and COLR). It takes 4.0, 4.6 and 8.5 µs a glyph at 12, 16 and 48 pixels, and its code is 600 KB (measured). ab_glyph is faster (2.2 µs, 111 KB), but it has no hinting, embedded bitmaps or color, and it cannot read pixel fonts. fontdue processes every glyph at load, 278 MB for Noto Sans Mono CJK, which rules it out on 512 MB (measured).
- Pixel fonts are read by `vtium-vitrify`'s own BDF reader, never through swash.
- Shaping is HarfRust's (ADR-1301 D1), and it runs in the helper. A cluster that the console asks for is text that a program wrote, so the shaper, and the font parser behind it, face input that anyone with the terminal can choose. They therefore run only in the helper, and the shaping path has a fuzz target.

### D6: decoders and rasterizers stay out of the console

- `vtium-vitrify` holds every decoder and reader: gzip, bzip2, xz and zstd, PSF1, PSF2, CPI, raw fonts, BDF, unimaps, kbd lookup, vconsole.conf, the outline reader, swash and HarfRust, each in pure Rust with no C library.
- The `vtium` binary holds the text readers for frits and profiles, the bounded glyph-chunk reader (D2), the built-in fonts and the inflate they need. In any build, the console's process parses no font, decompresses no file, shapes nothing and rasterizes nothing.

### D8: the kernel's font

The console already opens its VT as root, and it reads the kernel's font once at start, while the VT is still in text mode and before it takes the VT for graphics. It reads the font with KDFONTOP's `KD_FONT_OP_GET_TALL`, falling back to `KD_FONT_OP_GET` on a kernel without it, and it reads the Unicode map with GIO_UNIMAP. The kernel returns a plain bitmap and a map, so the console reads two ioctl replies and parses no font. It hands the bytes to the helper, which reads them as a frit's `console:` source, unprivileged, as it reads any font. The ioctls live in vtium-platform, the only crate with `unsafe` code, each with a test of its request number. The kernel refuses the read on a VT that is already in graphics mode, and that source then fills nothing.

### D18: asks and the fill

- **Asks come first.** An ask is the console's request to the helper for characters that the screen shows at a cell size and that the store does not hold at that size, leaving out any that the console has been told the frit lacks and any that it has already asked for. The helper answers an ask whole, ahead of anything else.
- **The fill.** While nothing is asked, the helper fills in the frit's coverage at that cell size in this order: the screen's characters, then printable Latin and box drawing, then the rest of the frit's coverage, 1,024 codepoints a batch. It skips the characters that the fill request says the console already holds.
- **One record of what is held.** The console's store (D19) is the only record of which glyphs the console holds. The helper remembers what it sent for one request only, an ask or a fill, and sends each glyph once within that request. A later request may send again a glyph that the console holds, and the console drops the copy. The helper knows only its job. The console, the protected process that receives the glyphs and sends out the requests, decides when a glyph is needed, and again if need be (the operator, 2026-10-28).
- **Preemption between batches.** The helper takes a waiting ask before each batch, so an ask waits at most one batch.
- **The fill stops at the store's limit** (D19), so speculative glyphs never churn the store, which means they never make it throw out glyphs to make room. Once the fill stops at the limit, it does not resume for that cell size, as a zoom to another cell size ends the old size's fill.
- **Glyphs already held** are drawn from the store, with no request.

> Edited 2026-10-28: the fill skipped what the console held and what the helper had sent at the cell in any earlier request, so the helper kept a second record of what the console held. The team lead's reading of the operator's design rule, one side owns each fact (`considered`), leaves the store as the only record: the helper skips repeats within one request only. The earlier text is in git at e807139.

> Edited 2026-10-29: the operator decided the helper's memory of what it sent lasts one request (`considered`, item 6 of the open glyph questions), the rule the bullet already stated; the bullet gains the decision's reason, that the console decides when a glyph is needed.

### D19: the store

- **Per boot, in memory.** Generated glyphs live in the console's memory for the boot, and nothing that the console or the helper writes to disk holds a glyph. The store keeps glyphs only for the cell sizes that the displays draw at, and a zoom to another size drops the old size's glyphs.
- **Its limit comes from the machine's memory,** the way the scrollback takes its own budget, with no new key and no new size decision: 16 MiB below 2 GiB, 64 MiB from 2 GiB and 128 MiB from 4 GiB, the same tiers as the scrollback's line cap (`store_limit` on `feat/deferred`, `sb_size_cap` in vtium-term). A 512 MB board takes 16 MiB (the operator, 2026-10-28): the Latin fill used under 1 MB, and a CJK font's whole coverage at an 8x16 cell is about 11 MB (measured).
- **Only an ask evicts,** the least recently shown glyphs first; the fill stops at the limit instead. The console asks again for an evicted glyph when the screen shows it.
- **Eviction compacts** (the operator, 2026-10-28). The store holds each answer from the helper as one set. When an ask's glyphs arrive while the store is full, enough of the least recently shown glyphs leave to cover the overrun, in one pass. The console rewrites each set that lost glyphs, without the evicted ones, through vtium-font's set writer. Each glyph keeps its own pixel format, so 1-bit glyphs stay 1-bit. The store counts the bytes it wrote, so its limit bounds the memory that the sets take. The writer adds 71,472 bytes to minspec's console binary, from 2,683,344 to 2,754,816 bytes (measured), and 22,096 of them are its deflate, which the rewritten sets do not use.
- **Rejected:** hiding evicted glyphs without freeing them, which lets the store outgrow its limit while its count says otherwise; and storing glyphs one by one, which needs a second copy of vtium-font's drawing code in the console, against ADR-1301's one engine.
- vtium keeps no prerendered or persistent glyph cache of any kind (Alternatives Considered).

> Edited 2026-10-28: the operator decided how a glyph leaves the store (`considered`), and D19 takes the decision in its compaction and rejected bullets. D19 said only that an ask evicted the glyphs least recently shown; the store then evicted a whole helper answer at a time, aged by its most recently shown glyph.

> Added 2026-10-29: the operator raised tier 3's budget for the minspec console binary to about 3 MiB, since the 2,621,440-byte figure was arbitrary, and the console drops the set writer's deflate, which the rewritten sets do not use: 22,240 bytes, measured by cutting it on integration 532b417 (minspec 2,921,080 to 2,898,840 bytes, `feat/size-budget` cd0c97f), where the 22,096 bytes above were measured on an older tree; both are revisited after the first cut of heads (ADR-1401 D11, `considered`). The figure is tier 3's `BINARY_BUDGET` (tools/tier3/README.md).

> Added 2026-10-31: the operator raised the budget again, to 3.5 MiB (3,670,016 bytes), so that new work is not trimmed to fit. Main's minspec binary was 3,137,216 bytes at c86ccd3, and the notice catalog branch needed trims to stay under 3 MiB. Measured like for like on the maintainer's machine, the C reference's own code and data were about 1.82 MB (its binary, its modules and libtsm), of which its unifont module was 1,284,240 bytes beside the binary; vtium builds unifont in, at 1,344,063 bytes packed. The figure is tier 3's `BINARY_BUDGET` (tools/tier3/README.md).

### D20: keys

- A glyph is keyed by its cluster's text (its codepoints), bold, italic and its width in cells, at a cell, as the renderer's cache keys a drawn glyph (ADR-1300, `crates/vtium-render/src/cache.rs`). The store uses the same key, so a glyph found in one is found in the other. On `feat/deferred`, which generates regular glyphs one codepoint at a time (D21), a key's text is one codepoint and its style is regular.
- Keying by the text itself needs no symbol table and no generation counter: two cells that hold the same cluster share its glyph however their clusters were made, and a key can never come to name a different cluster later.
- Color, underline and strike stay out of the key. A glyph is a coverage mask, or RGBA for a color glyph, and it is colored when it is composed. The decorations are sprites that vtium-render draws (ADR-1300).

### D21: cluster asks and style asks

Both are built. The console asks the helper for each cluster that the screen shows, and for each style that the screen shows (bold, italic or bold italic) in an ask of that style. It keys the store by the cluster's text and its style (`crates/vtium/src/helper.rs`, `crates/vtium/src/deferred.rs`). The helper shapes clusters and draws or synthesizes styles (`crates/vtium-config/src/serve.rs`, `crates/vtium-config/src/outline.rs`). The background fill draws regular glyphs alone, and bold and italic glyphs come only when the console asks for them.

- **Cluster asks.** The console asks the helper for a cluster by its text, as the key holds it. The helper shapes the cluster with HarfRust and composes its glyphs into one glyph, fitted to its 1 or 2 cells and to its reach, which is one more cell on each side along its row (ADR-1301 D1, ADR-1302 D1).
- **Style asks.** The console asks for bold and italic as part of the key. The helper draws them from the frit's bold and italic faces (`style=`, D1) where the frit names them, and from the regular glyph where it does not. A missing bold is the regular glyph emboldened, as vtium-font draws it today. A missing italic is the regular glyph slanted by 0.2 about the baseline: the helper slants an outline's points before it rasterizes them, and a bitmap's rows one at a time (ADR-1302 D4). Where the frit names a bold or an italic face but no bold-italic face, a bold-italic glyph is the regular glyph emboldened and then slanted, and never the bold face slanted (the maintainer, 2026-11-04).

> Edited 2026-11-04: cluster asks and style asks are built, on the branch that builds style requests, and the opening says so. It read: "*Intended design, not yet built.* The helper on `feat/deferred` generates regular glyphs per codepoint (`crates/vtium/src/helper.rs`, `crates/vtium/src/deferred.rs`)."

> Edited 2026-11-04: the maintainer decided where bold italic comes from when the frit names no bold-italic face (`considered`, 2026-11-04). The style bullet ended at "(ADR-1302 D4)."

> Edited 2026-11-01: glyph ink reaches one cell past its own on each side, and synthetic italic is a slant (ADR-1302, accepted 2026-11-01). The cluster bullet read "fitted to its 1 or 2 cells (ADR-1301 D1).", and the style bullet ended: "and from the regular glyph emboldened, as vtium-font draws a missing style today, where it does not." The earlier text is in git at 1d51e46.

### D22: the lookup chain

The console finds a cell's glyph in the order ADR-1301 D1 sets, and parses no font at any step:

1. the cluster's glyph in the store (D19);
2. the glyph of the cluster's NFC form (Unicode's composed form), where the cluster composes to one codepoint that the store holds;
3. the base character's glyph, with each nonspacing mark's glyph drawn over it and each spacing mark's glyph standing beside it (ADR-1302 D14);
4. the built-in font for the cell (D7);
5. an ask to the helper (D18), with the cell drawn meanwhile from step 4 or 6;
6. the replacement glyph: U+FFFD where the font has it, else U+25A1.

> Edited 2026-11-03: the maintainer decided ADR-1302 D14, where a spacing mark (General_Category Mc), such as a Devanagari vowel sign, stands beside its letter instead of over it. Step 3 read "the base's glyph with each mark's glyph overstruck on it;". The earlier text is in git at 8e0af22b.

### D2: the glyph chunk format, on the helper's pipe

The helper sends glyphs in the chunk format that vtium-font reads. The format is little-endian, and a constant in vtium-font's `src/bounds.rs` bounds every count, size and offset in it. It has a header with a magic number and a format version; strikes (a strike is a font's glyphs at one cell size), each with its cell, its baseline, its underline and strike positions and thickness, its pixel format and its glyph count; runs of codepoints of one width; and glyph chunks of at most 16 KiB. A glyph is exactly its 1 or 2 cells, except a glyph whose ink passes its cells, in an outline strike or in a bitmap strike slanted for italic. Such a glyph also carries its ink box: the columns that its cells and its ink cover, limited to one cell beyond its own on each side. The box is given as an offset from the cell's left edge and a width, and it spans the cell's full height (ADR-1302 D6; the maintainer, 2026-11-01). A glyph whose box, its cells and its ink together, would pass the 16 KiB chunk is cut to its own cells instead. A set holds a regular strike at each of its cell sizes, except an answer to a bold or italic ask, which holds only the strikes of the style asked for (D21; the steering session, 2026-11-04). The console reads the chunks with the crate's no-panic reader and holds what it reads in the store (D19). The format is a wire format: nothing writes it to disk, so it carries no file hashes, and a change to it changes both ends of the pipe in one release.

> Edited 2026-11-04: an answer to a style ask holds only that style's strikes, which the set format allows (ADR-1302's `considered`, 2026-11-04). The paragraph said nothing of which strikes a set holds.

> Edited 2026-11-01: the review of the helper's ink box: the box is the union of the glyph's cells and its ink, and the chunk bound applies to that union. The paragraph read "the columns its ink covers" and "A glyph whose box would pass the 16 KiB chunk". The earlier text is in git at fa62f7f.

> Edited 2026-11-01: the maintainer decided that glyph ink reaches one cell past its own on each side, outline glyphs and bitmap glyphs slanted for italic alike (ADR-1302, the maintainer's answers of 2026-11-01), so the format carries an ink box. The paragraph said nothing of a glyph's box: every glyph was exactly its 1 or 2 cells.

### D13: how glyphs are held and drawn

- Outline glyphs are R8, 8-bit coverage. Bitmap glyphs are 1-bit, and vtium expands each to R8 as it draws it. Color glyphs are premultiplied RGBA. Every glyph that the console hands the renderer is R8 or RGBA, which serves the CPU path now and a GPU path later. Outline glyphs stay 8-bit, since displays are good enough that 4-bit quantization would likely show (the operator, 2026-10-28).
- The CPU path's blend loop is written so that the compiler vectorizes it, with no hand-written SIMD. NEON is always present on aarch64, and on x86_64 vtium chooses an AVX2 version at run time. Per glyph, SSE2 against AVX2 took 85 against 29 ns at 16 pixels, and 597 against 264 ns at 48 (measured). The choice at run time needs `unsafe`, or a crate outside vtium-render, which forbids `unsafe`.
- BC4, a GPU texture compression, loses up to 25/255 of each value, and the boards that minspec targets lack it. SDF and MSDF cost 55 times what R8 costs on the CPU and blur small sizes. They stay a possible later form for the GPU path alone.

### D23: damage

- **Arrival is a damage source on the same path as program output.** Damage is the set of cells that a frame must redraw. Glyphs that arrive between frames merge into one set of cells, and the next frame draws it in one pass.
- **Damage comes from what each buffer displays,** the scrollback view and cluster cells included. vtium redraws the cells whose visible text contains an arrived character, and the neighboring cells that those characters' ink reaches, before or after the arrival (ADR-1302 D9). It reads the text from the view that each display's buffer shows. A fill of characters that no buffer shows draws no frame.
- **Every glyph that arrives replaces the built-in drawing** of its character in each renderer's drawn-glyph cache (ADR-1300), whether or not the character is on screen when it arrives, so a character shown later draws in the frit. Only the cells that show it are damaged (the operator, 2026-10-29).
- **Eviction damages nothing:** cells already drawn keep their pixels, and the console asks again for an evicted glyph when the screen next needs it.
- **The departure.** The C reference had no arrival: it loaded its font before drawing, rasterized a glyph inside the frame that asked for it, and redrew every cell of every buffer whenever the font changed (C notes). vtium redraws only the cells that show the arrived characters.

> Edited 2026-11-01: glyph ink reaches one cell past its own on each side (ADR-1302, accepted 2026-11-01), so an arrival also damages the neighbors its ink reaches (ADR-1302 D9). The bullet read: "the cells to redraw are those whose shown text contains an arrived character, read from the view each display's buffer shows." The earlier text is in git at 1d51e46.

> Edited 2026-10-29: the operator decided that every glyph the helper sends replaces the built-in drawing of its character in each renderer's cache, shown or not (`considered`), after the deferred-rendering work's verification found a glyph arriving while its character was off screen never replaced the built-in drawing. D23 named only the cells an arrival damages.

### D5: `vtium-vitrify set`, through OSC 50

- `set NAME` writes `OSC 50 ; NAME ST` to its controlling terminal, asks `OSC 50 ; ? ST`, and reads the reply in raw mode for at most a second. It exits 0 when the reply names NAME. Otherwise it says that font operations are off or that the terminal is not vtium.
- vtium takes OSC 50 only with `allow-font-ops` on, and the key defaults to on, as xterm's `allowFontOps` does. The key's description says why: it is xterm's convention, and OSC 50 can only switch among the frits that root wrote and the built-in fonts. With the key off, vtium ignores OSC 50, the query included.
- A `Pt` (the text that the sequence carries) that names a frit under `/etc/vtium/fonts`, or a built-in font, switches the live console: the built-in font draws what the helper has not yet sent for the frit, and the console asks the helper (D18). The cell changes only where D17 lets it. Anything else, such as a path, an X font name or a name with no frit, leaves the font as it was, and vtium logs why.
- `OSC 50 ; ?` replies `OSC 50 ; <name> ST` with the name of the current frit or built-in font, from vtium's own state, within the reply rule. A switch changes the live console only; `font-name` in the configuration is what persists.

### D10: how it is judged

- **The pipeline:** tests use a stand-in helper that answers after a delay. With it, the first frame draws in the built-in font in the profile's cell. No frame waits on the helper except these: the first frame of a frit that states no cell, which waits for the frit's own cell; an OSC 50 switch to such a frit; and an outline frit's zoom or relayout to a height whose cell it has not yet asked for (D17). The grid's size is the same before and after the glyphs arrive, and the frame after they arrive draws the frit's glyphs in only the cells that show them, the scrollback view and clusters included.
- **Asks and the fill:** an ask sent while a fill runs is answered before the next batch; the fill stops at the store's limit; an ask past it evicts the glyphs least recently shown; eviction redraws nothing.
- **The helper:** a crash, the CPU limit, a hang and an exit with work outstanding each restart it, up to `RESTARTS_MAX` times a boot. A frame (one message on its pipe) that it was not asked for gives it up for the boot, with one log line. It refuses to start as root.
- **Nothing outlives the boot:** no file the console or the helper writes holds a glyph.
- **One engine:** the same frit, cell and display give the same glyph bytes from a minspec build and a full one.
- **vtium-vitrify:** tests cover frit parsing, each key and each refusal with its line; `compress=` accepted with its warning; kbd lookup's order of directories and suffixes; vconsole.conf and the kernel command line; each compression; PSF1, PSF2, CPI, raw fonts, BDF and unimaps; `cover=` against each named set's table; and a font that panics the parser, failing only its source. The fixtures are generated in Rust in vtium's tree.
- **KDFONTOP and GIO_UNIMAP:** tests check the request numbers in vtium-platform. The live read runs only in tier 3's guest, never on the maintainer's desktop.
- **Tier 3:** the frit variants on the minimal guest, 1 vCPU and 512 MB, draw the built-in font first and the frit once the helper answers; their goldens are vtium's own, provisional until the operator reviews them.

> Edited 2026-11-02: an outline frit's relayout to a new height waits for its cell too (D17), and an OSC 50 switch to a frit that states no cell, which D17 already named, is listed. The bullet read: "no frame waits on the helper but the first frame of a frit that states no cell, for its own cell, and an outline frit's zoom (D17),". The earlier text is in git at 928e9a3.

> Edited 2026-11-01: the maintainer decided that an outline frit that states no cell takes its fonts' own cell, asked of the helper before its first frame and at each zoom (D17). This bullet read: "no frame waits on the helper but a bitmap frit's first for its own cell,".

### D24: `vtium-config report`

`vtium-config report` gathers what a bug report needs into one plain-text document, so that someone who has never read vtium's code can attach the evidence a defect leaves. It prints the document to standard output. `vtium-config report --save` writes it instead to a new file in the current directory, named `vtium-report-YYYYMMDD-HHMMSS.txt` after the time in UTC, and `--save=PATH` writes it to PATH. Either prints the file's path, and neither replaces a file that already exists. `--lines N` sets how many of each console's latest log lines the document holds, 300 by default. `--previous-boot` reads the journal of the boot before this one, `journalctl -b -1`, in place of this boot's, since a defect that forced a reboot leaves its evidence there. Where the journal holds no boot before this one, the logs part says so, and that journald keeps its logs across a reboot only when `/var/log/journal` exists.

The document holds these parts, each under a plain heading, in this order:

1. What the report holds, and what to check before sharing it.
2. Each vtium console binary installed, `/usr/bin/vtium` and the development install's `/usr/local/bin/vtium-dev` where there is one, with its version and the features it was built with, and vtium-config's own version.
3. The kernel's release, the architecture, the distribution's `NAME` and `VERSION_ID` from `/etc/os-release`, else from `/usr/lib/os-release`, and whether systemd and logind run.
4. `vtium console status`, and the state and start time of each `vtium@`, `vtium-dev@`, `vtium-getty@` and `vtium-dev-getty@` unit, with the revision each console logged when it last started in the boot the report reads.
5. Each DRM card's driver and render node, and each of its connectors' name, status, enabled state and preferred mode, with the monitor's name and size from its EDID.
6. vtium.conf's lines without its comments and blank lines, the names of the files in `/etc/vtium` and in its `fonts` directory, and what `vtium check-config` prints.
7. The latest log lines of each of those units in the boot the report reads. The font helper and the image helper run inside their console's unit, so their lines are in its log.
8. Run as root, the kernel's display state from debugfs, `/sys/kernel/debug/dri/N/state`, for each card.

The report changes nothing on the machine. It reads files under `/sys`, `/proc`, `/etc`, `/usr/lib` and `/run`, and up to 256 MiB of each vtium binary where it looks for marker text, and runs only programs that read: `journalctl`, `systemctl list-units`, `systemctl show`, and the vtium binaries with `--version`, `--help`, `console status` and `check-config`. It runs a binary's `console status` or `check-config` only after that binary's `--help` names the subcommand, because an older vtium that is given a word it does not know could start as a console. It opens no card node, takes no DRM master and opens no VT.

It runs as the user, and run as root it adds the parts only root can read. Each part it cannot read says so. Where the user is refused, it says `needs root: sudo vtium-config report > vtium-report.txt`. Where root is refused debugfs with `EPERM`, it names the kernel's lockdown as the cause. Where `journalctl` says that the user cannot see the system's messages, whether it says so for a unit's log or when the report asks which units the journal names, the logs part says to run the report as root or to join the `systemd-journal` group, whose members read the whole journal.

Run as root, the report reads only and writes nothing: vtium-config refuses `--save` in any form as root, and says to run `sudo vtium-config report > vtium-report.txt`, so the user's own shell writes the file, as the user. A rule that lets a user run the report as root therefore never lets that user create a file wherever root can, such as `/etc/ld.so.preload` or a unit file.

> Edited 2026-11-03: the team lead decided, under the maintainer's standing grant and after the fresh review of the report, and the maintainer was told, that root's report writes no file, since `--save=PATH` as root let a user allowed only `sudo vtium-config report` create any file and own it. The paragraph ended: "Run as root with `--save`, the report gives the file it writes to the user that sudo, pkexec or doas ran it for, found as `apply` finds that user, so that user can read and attach it."

The document holds neither the machine's hostname nor any monitor's serial number. The report never prints an EDID's serial fields or its raw bytes, and it runs `journalctl` with `--no-hostname`. A log line or a program's output can still carry them, so the report replaces two things in the gathered text. First, it replaces with `[serial]` the serial in every device id written in a form the head file can hold, whatever device the id names, so the serial of a monitor unplugged earlier in the boot goes too. The serial is the last field of each `edid:MAKER:PRODUCT:SERIAL` and `serial:VENDOR:PRODUCT:SERIAL` token, whatever its other fields hold, since `check-config` echoes a malformed head file as it is. It is also the last field of each bare monitor id, `MAKER:0xPRODUCT:SERIAL` (vtium-platform's `EdidId`), in the forms vtium writes and the head file takes: the maker in either case, and the product as one to four hex digits, with or without `0x` or `0X`. Second, it replaces whole words. The hostname, as `/proc/sys/kernel/hostname` and `/etc/hostname` give it, and its first label become `[hostname]`. Each connected monitor's serial string and serial number become `[serial]`; the report matches the serial string in the form that vtium-platform's reader gives and vtium logs, in which `#` and the other bytes the head file cannot hold read as `_`. It leaves a name shorter than three characters alone, since replacing it would garble the text and a name that short identifies no machine. The opening part says that the report can still name user accounts, since a login writes the user's name to the log, `vtium console status` names the user of each graphical session, and a line of vtium.conf can hold a user's name, and that the logs' times show the time zone, and it asks the reader to look the report over before sharing it.

vtium's `--version` prints its version on its first line, as it always has, then `revision REV` and `features ...`, which names each feature the build has on top of minspec, or `none`. The report reads the revision and the features from those lines. For a binary too old to print those lines, the report searches the binary for a line of text that only a build with the feature carries, the way the development install's script tells a build with images: `cannot open the image helper ` for `images`, and `logind is not on the bus (` for `logind`. A feature with no marker in such a binary, `gpu` among them, is reported as unknown. vtium-config stores each marker split in two and joins the halves only when it searches, so that neither its source nor its own binary holds the marker, and the marker appears only where a build with the feature carries it.

`--root DIR` makes the report read every path under DIR in place of `/` and run its programs from DIR's `usr/bin` or `bin`, so that tests run each part against a stand-in tree. Run as root, vtium-config refuses `--root`, as it refuses its other path options.

The C reference had no such command. It logged its revision when it started and printed its version for `--version`.

> Added 2026-11-03: the maintainer asked for one command that gathers what a bug report needs, so that people who try vtium outside the project can report a defect with the evidence it leaves (the team lead's brief of 2026-11-03).

### Retired

- **D3, the trusted store,** and **D12, portable sets and the two ways to build them,** are retired with the prerendered store (Alternatives Considered).

### For the operator's check

The restart cap of 3 (D15) and the batch of 1,024 codepoints (D18) are agent calls that the operator has not yet looked at. So are two calls in D24: the report replaces only names of three characters or more, and it reads a binary's features from marker text where the binary's `--version` does not list them. The team lead decided a third call on 2026-11-03, and the maintainer was told: run as root, the report reads only and writes nothing. On 2026-11-01 the maintainer decided the three calls that the amendment of that day made in D17 (`considered`): a frit's first source sets its cell; each zoom of an outline frit with no cell waits up to `OWN_CELL_MS` for the exact cell; and a zoom that gets no answer in time takes the built-in font's width.

> Edited 2026-10-28: the operator answered the seven points this section listed as not decided, each as the team lead recommended (`considered`), and each answer is folded into the decision that named it: the cell's home (D16), the store's limit on minspec (D19), the helper's account after login (D15), a profile against the configuration (D16, D17, D9), zoom with a frit (D1, D17), the frit drawn when nothing names one (D11), and names that are not frits (D9). The section was headed "Not decided" and asked each as an open question; the earlier text is in git at 4477806.

## Evidence: the deferred pipeline measured

Phase 1's measurements on `feat/deferred` at d075494, accepted by the operator:

| Where | Frit | Helper asked after vtium start | First batch | Whole coverage | Helper peak |
|---|---|---|---|---|---|
| tier 3 guest, 1 vCPU, 512 MB | DejaVu Sans Mono (Latin) | 52–55 ms | 6–18 ms | 29–39 ms | 5.8 MB |
| tier 3 guest, 1 vCPU, 512 MB | WenQuanYi Micro Hei (CJK) | 51 ms | 31–43 ms | 265–276 ms | 13.8 MB |
| development machine | DejaVu Sans Mono | | 2 ms | 16–23 ms | 12 MB |
| development machine | Noto Sans CJK | | 50–61 ms | 409–537 ms | 43 MB |

The first frame is drawn in the built-in font 60 ms after vtium starts, in the same turn the helper is asked. The console's resident memory goes from 5.6 MiB to 6.1 MiB with DejaVu's glyphs.

## C notes: the C reference's flow

How the C reference drew its glyphs, as the logical reference for each decision above, read at kmscon `b1f7a7c5` and libtsm `eeaf293`. Each step below was found at those commits.

### The font load and the cell

- At start, with `font-size` set, `font_set` loaded the font before the pty and the first frame; with `font-size=0` it set `auto_font_size` and loaded the font at the first `terminal_update_size` (`src/terminal.c:1927-1934`, `1107-1119`).
- `font_set` called `kmscon_font_find` with the configured name, size and engine; when that failed and no font was loaded yet, it retried at 16 px (`src/terminal.c:921-941`). `kmscon_font_find` tried the configured engine, then the others in turn (`src/font/font.c:124-167`, `203-232`).
- `terminal_get_auto_font_size` worked from the smallest display's size and took the smaller of two heights: twice the display's width divided by 120, which gave at most 120 columns of cells half as wide as they are high, and the display's height divided by 40, which gave at least 40 rows. It never went under 16 px, and it used 16 when no display size was known (`src/terminal.c:899-919`).
- The cell was the font's own width and height, which `terminal_update_size_notify` handed to libtsm with `tsm_screen_set_cell_size` (`src/terminal.c:1151-1162`).

### The resize, and zoom

- `terminal_update_size_notify` recomputed the grid, called `kmscon_pty_resize`, which set TIOCSWINSZ (`src/misc/pty.c:941-958`), and on a changed size called `tsm_screen_resize` (libtsm `src/tsm/tsm-screen.c:769`) and redrew every screen (`src/terminal.c:1151-1171`). It ran on a rotation (`rotate_cw_all`, `rotate_ccw_all`), a display added (`terminal_add_display`, `src/terminal.c:1213`, call at 1267) or removed (`free_screen`, 1289, call at 1307), and a zoom.
- `zoom_in` and `zoom_out` turned the automatic size off, moved the size by the font's `increase_step`, refused above 150 px and below 10 px, loaded the new font with `font_set`, handed it to every screen with `font_update_all`, and resized (`src/terminal.c:1324-1355`, `943-955`). `increase_step` was 1 for the freetype and pango engines, the font's height for psf, and 16 for unifont and the 8x16 font (`src/font/font_freetype.c:280`, `font_pango.c:399`, `font_psf.c:254`, `font_unifont.c:235`, `font_8x16.c:68`).

### Clusters

- libtsm segmented text by UAX #29's extended grapheme cluster rules (revision 49, Unicode 18.0), one codepoint at a time (`tsm_gb_start`, `tsm_gb_continues`, `src/tsm/tsm-unicode.c:103-139`); `screen_write_char` joined a continuing codepoint to the previous cell's cluster with `cluster_append` (`src/tsm/tsm-screen.c:1430-1450`, `1485-1503`).
- The symbol table interned each cluster by content (`tsm_symbol_append`, `src/tsm/tsm-unicode.c:476-523`) and named it `TSM_CLUSTER_BASE`, 2^21, plus its slot; a key below that was the codepoint itself; a cluster held at most `TSM_CLUSTER_MAX`, 32, codepoints (`src/tsm/libtsm-int.h:40-49`).
- Each cluster counted the cells holding it (`tsm_symbol_ref`, `tsm_symbol_unref`, `src/tsm/tsm-unicode.c:532-551`, called from `cell_set_ch` and `cell_release`, `src/tsm/tsm-screen.c:137-158`); at zero it was freed and its slot went to a free list that `take_slot` reused first (`src/tsm/tsm-unicode.c:434-474`).
- Because a slot was reused, each cluster took a generation from a counter (`tsm_symbol_generation`, `src/tsm/tsm-unicode.c:525-530`), and the frame's key carried it in its high 32 bits, so a key never named two clusters (`frame_key`, `src/tsm/tsm-render.c:477-485`; `src/tsm/libtsm.h:390-400`).

### The glyph cache

- `struct kmscon_glyph_key` held the frame's key, bold, italic, the width in cells and `extra`, which only the GPU renderer used, for decorations it baked in; `kmscon_glyph_key_hash` mixed the fields and finished with splitmix64's finaliser (`src/font/font.h:64-95`).
- bbulk kept glyphs in an LRU of twice the screen's cells, which evicted the least recently used at its size (`bbulk_set`, `src/render/bbulk.c:432-435`; `shl_lru_insert`, `src/shl/lru.h:145-168`), and freed it whole when the font was unset or the display rotated (`bbulk_unset`, `bbulk_rotate`, `src/render/bbulk.c:468-486`, `511-516`).
- On a miss, `find_glyph` read the cluster's codepoints, called `kmscon_font_render` with bold and italic, rotated the glyph and inserted it (`src/render/bbulk.c:525-576`), from inside the cell's draw (call at 1427): rasterizing was lazy and in the draw path, in the root process. `kmscon_font_render` drew box drawing and its kin itself, then the font's glyph, then U+25A1, then `?` (`src/font/font.c:311-368`).
- `kmscon_font_render` passed every glyph but a box-drawing one through `fit_glyph`, which re-boxed a glyph whose width was not its cells' width: it centered the glyph, and where the glyph was wider it cut the overflow evenly from both sides, the odd column from the right, `(sw - w) / 2` columns off the left (`src/font/font.c:267-308`, call at 367). It kept the glyph's own height, since each engine drew its glyphs at its font's height. A glyph of one font never drew in another font's cell, because the C reference fell back only within its font, to U+25A1 and then `?` (D7, D22).
- Color, underline and strike stayed out of the key: underline and strike were decoration sprites the renderer drew (`src/font/font.h:58-62`), and color was applied when the mask was composed. The older `tsm_screen_draw` id carried underline, inverse and blink bits as well (`src/tsm/tsm-render.c:296-312`); the frame key the cache used did not.

### Damage

- libtsm stamped each cell and line with an `age` from the screen's `age_cnt` as it changed (`src/tsm/tsm-screen.c:114-127`, `176`, `544-552`) and raised the counter once a frame (`screen_inc_age`, `src/tsm/libtsm-int.h:546-550`). On the counter's wrap, `screen_age_wrap` set every age to 1 and raised `age_reset` and `age_epoch` (`src/tsm/tsm-screen.c:214-236`).
- `tsm_screen_draw` took a cell's age as the largest of its own, its line's and the screen's, or 0 under `age_reset` (`src/tsm/tsm-render.c:284-292`); the frame API marked a cell changed when its age was newer than the caller's, and every cell on a new epoch or a new shape (`screen_frame`, `tsm_screen_frame_changed_since`, `src/tsm/tsm-render.c:587-715`, `741-775`).
- kmscon's `kmscon_text_redraw_all` emptied a buffer's age, so its next draw marked every cell changed (`src/render/text.h:214-217`): on `kmscon_text_unset`, which `kmscon_text_set` called first on each font change (`src/render/text.c:271-323`), on `kmscon_text_resize` (338-352), on a rotation (430-433), after a switch of screen or a failed draw (741-770), and in bbulk for two frames after the default or decoration colors changed (`src/render/bbulk.c:1797-1810`).
- A glyph existed the moment a frame asked for it, so no glyph arrived between frames and no arrival damaged anything.

## Where vtium departs from the C reference

- **Generated in a sandboxed helper, not in the root process:** the C reference parsed and rasterized fonts as root, in the draw path; vtium does both in the helper (D14, D15).
- **The first frame does not wait for a font:** the C reference loaded its font before drawing; vtium draws the built-in font at once and swaps the frit's glyphs in (D7, D17).
- **Sizes are points at the display's density:** the C reference fixed its density at 72 DPI, so its point size was a pixel height (`src/font/font.c:198-199`); vtium converts with `display-dpi`, else the density the display measures, else 96 DPI with a warning (D17).
- **The cell comes from the profile:** the C reference took its cell from the loaded font. vtium reads the cell from the profile as text. It takes a frit's own cell when the helper tells it, waiting at most 100 ms before a frame, and it uses an answer that comes later from the frame after the answer arrives. Its automatic rule yields to the small-display rule (D17).
- **Arrival damages only the cells that show it:** the C reference had no arrival and redrew everything on a font change (D23).
- **Glyphs keyed by text:** the C reference keyed its cache by a symbol and a generation counter; vtium keys it by the cluster's text and needs neither (D20).
- **A per-character fallback to the built-in font,** and U+FFFD before U+25A1: the C reference fell back from its font to U+25A1, then `?` (D7, D22).
- **A store with a memory limit where only an ask evicts:** the C reference's LRU held twice the screen's cells and evicted on every insert past it (D19).
- **From `setfont`:** a named font that has no Unicode table and no `unimap=` takes kbd's `def.uni`. `setfont` left the kernel's map in place for such a font, but a frit has no kernel map to inherit (D1).

## Consequences

### Positive

- One path draws every font on every build, with no store, manifest, hash chain, build commands or package hooks to keep.
- A frit takes effect as soon as it is written, for whatever display it lands on.
- A start never waits on a font, except for the own cell of a frit with no `cell=` line, or of an outline frit whose `cell=` a configured `font-size` overrides, and then for at most 100 ms, and the grid never jumps when glyphs arrive.
- The console's process parses no font, decompresses no file and shapes no text. Each of those runs in a sandboxed process that exits after its idle interval.
- Memory follows what the screen shows: the store's limit bounds it, and the fill never churns the store.
- A redraw after an arrival is as small as the cells that show the arrived characters.

### Negative

- Every boot parses and rasterizes its fonts again, and a CJK frit's helper holds 13.8 MB while it works on the minspec guest.
- The first frames after a start, a zoom or a hotplug draw in the built-in font.
- An OSC 50 switch to a frit with no `cell=`, or to an outline frit whose `cell=` a configured `font-size` overrides, holds the screen up to 100 ms for its fonts' own cell, and so does an outline frit's zoom or relayout to a height whose cell it has not yet asked for.
- The console checks only that the helper's glyphs are well-formed.
- A frit's fonts must be readable by the helper's account.
- Glyphs cannot be built on one machine for another, and distributions cannot ship prebuilt glyphs.

> Edited 2026-11-04: the cold-start observable, the Positive bullet on what a start waits for and the Negative bullet on the OSC 50 switch now match D17, which since 2026-11-01 has every frit with no `cell=` line ask for its own cell, outline frits included, treats an outline frit whose `cell=` a configured `font-size` overrides as one with no `cell=`, and has an outline frit's zoom or relayout to a new height wait for its cell too. They read only of a bitmap frit: the observable said "the frames that wait are the first of a frit of bitmap sources that states no cell, at the start or after an OSC 50 switch to it", the Positive bullet "A start never waits on a font but for a bitmap frit's own cell, at most 100 ms", and the Negative bullet "An OSC 50 switch to a bitmap frit with no `cell=` holds the screen up to 100 ms for its fonts' own cell." The earlier text is in git at 3f08e336.

### Neutral

- The store's code, its commands, set loading and tier 3's set-building step are removed in the deferred-rendering work's third step.
- ADR-1301's two schedules become one, and its references to built sets take dated notes citing this record.

## Alternatives Considered

- **A prerendered store,** as this record first decided: `vtium-vitrify create` built vitrified sets into a root-owned store with a manifest of SHA-256 hashes, or `create --out` built a portable `.vtglass` file anywhere, which `install` added to the store; the console loaded only checked sets; and package hooks built the locale's default sets at install. The operator replaced it with run-time generation, for the reasons the Context gives. The store, its manifest and its five commands existed only to keep a cache of what the helper generates.
- **A persistent glyph cache across boots,** per user or per machine, so that a warm start skips rasterizing. Not taken, by the operator: there is no prerendered cache of any kind, persistent or otherwise, beyond the helper's per-boot memory cache.
- **Sets sized per locale and display,** each with one to three adjacent sizes. A ladder of sizes was 861 MB for full GBK alone (measured), and the default sets of a zh_CN desktop were about 33 MB, or 120 MB at 7680x2160. Retired with the store.
- **Compressed stored sets,** deflated 8-bit and color strikes at 2.2 to 4.7 times smaller, and **4-bit strikes** at half the size. Retired with the store; outline glyphs stay 8-bit.
- **The console loads its font before the first frame,** as the C reference did. Not taken: a CJK frit takes a quarter of a second or more to generate on minspec, and the built-in font draws at once.
- **No built-in font,** waiting for the helper's first glyphs. Not taken: the screen would be blank until the helper answers, and blank where it cannot run; the C reference likewise kept a compiled-in fallback.
- **Generated glyphs drawn in the built-in font's cell,** as phase 1 drew them. Replaced by the operator: the profile states the cell, and the grid never resizes because glyphs arrived.
- **The console reads font files,** as the C reference's `font-engine=psf` did. Not taken: every decoder and parser would sit in the root process, facing files a user can name.
- **The console reads a bitmap font's header for its own cell.** Rejected by the operator: it would be the first exception to the rule that the console never parses a font. The helper tells the console the cell instead (D17).
- **A bitmap frit with no cell keeps the display's cell.** Rejected by the operator: an existing PSF setup would draw in the display's cell, 8x19 at 1366x768, instead of the font's 8x16.
- **Rasterising in the console's root process,** as the C reference did. Not taken: a font file would be parsed as root, and swash panicked on a pixel font's OTB (D14).
- **A persistent helper,** kept for the boot once started. Not taken, by the operator: state a hostile font plants would survive, and the CPU and memory budgets would never reset.
- **Unbounded restarts.** Not taken, by the operator: each restart re-randomizes the address layout, so an exploit could retry until it lands.
- **Keys by symbol and generation,** as the C reference's cache keyed its glyphs. Not taken: vtium's screen keeps a cluster's text with its cell, and keying by that text needs no table to intern it and no counter to tell reused keys apart (D20).
- **A full redraw on each arrival,** as the C reference redrew on a font change. Not taken: arrivals come in batches through a fill, and each would repaint the whole screen.
- **ab_glyph or fontdue as the rasterizer.** Not taken: ab_glyph has no hinting, embedded bitmaps or color and cannot read pixel fonts; fontdue needs 278 MB at load (D14).
- **A per-session socket for `set`.** Rejected by the operator in favor of OSC 50.
- **A built-in glyph scaled to a cell smaller than it,** as a 6x9 frit's cell is smaller than Unscii-8's 8x8. Not taken, on the recommendation that the maintainer reads with the work that adds Tiny5 Mono: scaling 8 columns to 6 drops columns unevenly or blurs the glyph, so the built-in glyph is centered and cut evenly instead (D7).
- **Fusion Pixel 12 built in.** Not taken: it is twice the budget that was left, and one variant gives other locales the wrong glyph forms (D7).
