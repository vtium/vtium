---
contract: adr/v1
kind: decision
verb: add
capability: glyphs
basis:
  - operator: aaronsb
    level: directed
    said: "Vitrify is how every font reaches the console. A kbd console font and an outline font such as a Nerd Font take the same path: the console loads only vitrified sets and its built-in ones, and never decompresses, parses a PSF or rasterises."
    via: vtium/vtium#3, comment of 2026-10-28 headed "Direction (operator, 2026-10-28)", posted from the operator's account as the operator's direction; the wording is agent-written
  - operator: aaronsb
    level: directed
    said: "The console loads a set only from the root-owned store, and only when its hash matches the manifest. The built-in sets (unifont, and Unscii-8 for the 320x240 floor) are trusted by construction."
    via: vtium/vtium#3, the same comment
  - operator: aaronsb
    level: directed
    said: "#107's KDFONTOP route vitrifies the font the kernel already holds."
    via: vtium/vtium#3, the same comment
  - operator: aaronsb
    level: directed
    said: "`set` reaches the console through xterm's OSC 50 font operation on its own tty, behind a config key like xterm's `allowFontOps`."
    via: vtium/vtium#3, comment of 2026-10-28 headed "Decided (operator, 2026-10-28)", answering the direction's four questions; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "`create` is a sudo command; there is no per-user store. A font set is first declared as a small recipe (its metadata and configuration: the sources, such as a packaged font or kbd name, a unimap, an outline font, the cell sizes and fallbacks). `create` runs the recipe: it fetches and reads the resources, and the finished set is a resource ready for the DRM renderer."
    via: vtium/vtium#3, the same comment
  - operator: aaronsb
    level: directed
    said: "vtium supplies the CLI, so `create` is non-interactive and scriptable (a set rebuilds when its recipe or source hashes change, and the built sets can be listed in a machine-readable form); the wrappers are other people's choice. A configured font that names no built set falls back to the built-in font, and the log names the set and the `create` command."
    via: vtium/vtium#3, the same comment
  - operator: aaronsb
    level: directed
    said: "the binary is named `vtium-vitrify`."
    via: vtium/vtium#3, comment of 2026-10-28, after "the tool is its own binary (working name `vtium-vitrify`) in the install tree, apart from the `vtium` binary"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "So a set is a portable file: `vtium-vitrify create --out FILE` builds one anywhere, as any user, with no store; `sudo vtium-vitrify install FILE` on the target checks its hashes and format version and adds it to the root-owned store."
    via: vtium/vtium#3, comment of 2026-10-28 headed "Direction (operator, 2026-10-28), for ADR-1200 before it's accepted"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "`vtium-vitrify` must self-bootstrap. Building on the target is a first-class path: `sudo vtium-vitrify create` builds into the local store for the machine's own display, including on a minspec board for its small panel, so a fresh install needs no other machine."
    via: vtium/vtium#3, comment of 2026-10-28 headed "Correction (operator, 2026-10-28)"; posted from the operator's account, the wording agent-written
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
    said: "The tool: preview (#120) and new/template (#121) belong to it, as the user; sudo once at the end of new."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead in this revision's brief; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "The recipe carries an explicit compress setting, with a matching #121 template field and builder choice, and the default is to compress. With the default, create deflates the strikes that gain (8-bit and color) and leaves 1-bit raw; compress=no stores everything raw."
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
    said: "A build that isn't minspec may include a rasteriser for sizes a set lacks, but only unprivileged and sandboxed, in ADR-1400's L1 helper (the session's user, NoNewPrivs, its sandbox; #31). The minspec build never rasterises."
    via: Claude Code session, 2026-10-28, the operator's decision relayed by the team lead, with the flow recorded in D15; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "**Fonts by kbd name.** `font-name=ter-v16n` resolves as kbd's `setfont` does"
    via: vtium/vtium#107's body, "The operator's decisions (2026-10-27)", posted from the operator's account, the wording agent-written; its routes are reshaped onto vitrify by this record
  - operator: aaronsb
    level: guided
    said: "Later it can serve as the metadata of a packaged font set (a \"vitrified font pack\")."
    via: vtium/vtium#85, "The operator's decisions (2026-10-27)", item 3, on the per-font sprite policy; posted from the operator's account, the wording agent-written
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
    said: "the set format's version is set now at 1.0, with the multi-format cells #23 added, and kept; a change older readers can't read takes a new major version from here on"
    via: the team lead's message of 2026-10-28, relaying the operator's decision; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "`font:` names a font family and nothing else; size and style go on their own lines (`size=`, `style=`)."
    via: vtium/vtium#121, comment of 2026-10-28 headed "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - upstream: "aaronsb/kmscon origin/main: with `font-engine=psf`, `font-name` was the path of a PSF font the C reference read and inflated at run time (src/font/font_psf.c; the fork's render ADR-103, 'A console font takes two settings'); the C reference made no KDFONTOP or GIO_UNIMAP call, resolved no kbd name and read no vconsole.conf"
  - upstream: "kbd (legionus/kbd master, read 2026-10-28; kbd 2.10.0 is the installed program): src/libkfont/setfont.c loads def.uni only for its default font, when that font has no Unicode table, and loads no map for a named font without one; the default font is the first of default, default8x16, default8x14 and default8x8 found; font lookup and suffixes as vtium/vtium#107 cites (src/libkfont/context.c, kbdfile.c)"
  - standard: "Linux 7.3 UAPI, include/uapi/linux/kd.h: KDFONTOP with KD_FONT_OP_GET and KD_FONT_OP_GET_TALL, and GIO_UNIMAP; the kernel refuses a font get on a VT outside text mode (drivers/tty/vt/vt.c, con_font_get, as vtium/vtium#107 cites)"
  - standard: "xterm ctlseqs, OSC 50 (set and query the font); xterm(1), allowFontOps: 'The default is “true”'"
  - standard: "vconsole.conf(5), systemd 262: FONT=, FONT_UNIMAP= and FONT_SCALE=, and the kernel command line's vconsole.font= overriding them"
  - evidence: "Unscii, fetched 2026-10-28 from http://viznut.fi/unscii/ (its https certificate failed to verify, so the files are pinned by hash): unscii-2.1-src.tar.gz, SHA-256 4d1215d495e49d005ae70828ec1529e997049b9a7f152b9439d78c7e70c65b57, whose unscii.txt says 'Everything made by me (viznut) is in the Public Domain.' and that only unscii-16-full, which takes glyphs from Unifont, falls under the GPL; unscii-8.hex, SHA-256 03094f7fbab7085cf6a6b624cee61e47e71ce5d0c2f308c2f4436afdc17f776c, 73,409 bytes, 3,191 glyphs, all 8x8 but U+0000 at 16x8, 19,836 bytes under gzip -9"
  - evidence: "the minspec binary at 2,429,200 bytes at integration 4086adf against the 2,621,440-byte budget (tools/tier3/README.md, BINARY_BUDGET), as the team lead reported in this record's brief"
  - precedent: ADR-1300
  - precedent: ADR-1400
  - precedent: ADR-1000
  - operator: aaronsb
    level: directed
    said: "the prerendered store is replaced by deferred rendering. vtium generates glyphs at run time from the configured frit, in the sandboxed helper (ADR-1301's run-time schedule, #123), on every build including minspec. On a cold start the built-in font (Unscii-8 under 16 px, unifont from 16) draws at once, and generated glyphs swap in as they arrive, so a cold start shows no lag. A performance problem is answered by that deferred pipeline, not by building sets ahead of time."
    via: vtium/vtium#145's body, "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "What goes: the root-owned store and its manifest; `vtium-vitrify create`, `install`, `list`, `verify` and `remove`; the console's store check and set loading (#124's store, #126's loading); target matching for built sets (#136's prebuilt half), named targets, the build-time half of the rule checker (#137); package hooks that build sets."
    via: vtium/vtium#145's body, the same decision
  - operator: aaronsb
    level: directed
    said: "What stays: frits, the definition the engine draws from (`NAME.vtfrit`); the built-in fonts; vitrify's sources, decoders, sandboxed parser child, rasteriser, color scaling and HarfRust shaping (#144), which the helper runs; the strike chunk format, as the helper's wire format and its per-boot cache; `preview` and `new` (#120, #121), with `new` writing a frit; OSC 50 switching (`set`), naming a frit; ADR-1301's display grid and rules."
    via: vtium/vtium#145's body, the same decision
  - operator: aaronsb
    level: directed
    said: "profiles keep their value and stay. vtium-vitrify remains the vitrification toolkit, with reduced scope: it defines profiles (a display's capabilities, named freely, with the frit and vtium configuration that suit it), writes and checks frits (`new`, the rule checker), previews them (`preview`), switches the console's frit (`set`), and supplies the engine the helper runs. It no longer builds, installs or maintains prerendered sets. #142's specimens are drawn per profile."
    via: vtium/vtium#145, comment of 2026-10-28 headed "The operator's direction (2026-10-28), refining this issue"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "vitrify builds profiles, and each profile states a frit, its cell and its options. vitrify previews them, guides a font to a profile, and lints a profile against a display. The helper runs vitrify's engine to generate the glyphs at run time."
    via: vtium/vtium#3, comment of 2026-10-28 headed "Reopened as the tracker for vtium-vitrify as the profile builder"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "no prerendered cache of any kind, persistent or otherwise beyond the helper's per-boot memory cache; vtium can't prerender the universe, so that complexity isn't carried. vtium keeps a compiled-in built-in font, as the C reference did, drawing at once on a cold start and wherever the helper is unavailable; everything else is generated at run time."
    via: vtium/vtium#145, comment of 2026-10-28 headed "The operator's decisions (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "build the deferred pipeline first and prove it works; then drop prerendered sets from vtium's code, docs and paths; then rewrite the decision records so they describe run-time generation as the design, with the prerendered store and any persistent glyph cache recorded only under Alternatives Considered, so no reader of the ADRs meets prerendered fonts as a live idea. ADR-1201's amendments fold into that rewrite."
    via: vtium/vtium#145, comment of 2026-10-28 headed "The operator's direction (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "1. Requests come first. The characters on screen, or newly requested, are answered first. 2. When idle, the helper fills the coverage in order: the screen's characters, then printable Latin and box drawing, then the rest of the frit's coverage. 3. A waiting request preempts the fill between batches, so it waits at most one batch. 4. Glyphs already held are served from the per-boot cache. 5. The store limit comes from the existing memory budget, the way `sb-memory` takes its own, and adds no new size decision or key. The fill stops at the limit, so speculative generation never churns. Only a request past the limit evicts, least recently used first. 6. The helper is recalled whenever needed. After an idle exit, the next need starts it again. After a crash or the CPU limit, the next request restarts it, a bounded number of times per boot. A helper that answers garbage gives that frit up for the boot."
    via: vtium/vtium#145, comment of 2026-10-28 headed "The operator decided the run-time fill model (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "The helper is not persisted. It exits after its idle interval and is started fresh on the next need, so no state a hostile font plants survives the process, and its CPU and memory budgets reset. A restart narrows the window and fixes no vulnerability; the console's checks and the sandbox contain one. Crash restarts are capped per boot, because each restart re-randomizes the address layout and unbounded restarts would let an exploit retry until it lands."
    via: vtium/vtium#145, comment of 2026-10-28 headed "The operator accepted the helper's lifetime (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "The grid never resizes because glyphs arrived. 1. The profile states the cell. The console reads it before the first frame, as text, so the console still parses no font. 2. The built-in draws in that cell until the generated glyphs land: centered when the cell is larger, and in the smaller strike when the cell is smaller. 3. The helper generates for the profile's cell and never moves it. A frit that cannot fit the cell fails with a logged reason, and the built-in keeps drawing. 4. A profile whose cell does not suit its frit is linted when vitrify builds or checks the profile, and in `vtium check-config`. 5. With no profile, the cell comes from the display, by the rule the C reference used for its automatic size: at most 120 columns, at least 40 rows, and never under 16 px. #118's small-display rule takes precedence at the 320x240 floor. 6. The grid resizes only on an explicit event, as it did in the C reference: a zoom, or a display hotplug while the cell is automatic. A resize sends SIGWINCH and redraws. Glyphs for the new size are requested, and the built-in fills the gap."
    via: vtium/vtium#145, comment of 2026-10-28 headed "The operator decided where the cell comes from (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "glyph swap-in is a damage source on the same path as program output (2026-10-28). Glyphs that arrive between frames merge into one damage pass. Damage comes from what each buffer displays, the scrollback view and cluster cells included, which takes in #149. Eviction at the store limit damages nothing. The C reference's flow is the floor and is kept in view: libtsm stamped cell and line ages, and kmscon redrew every cell on any font change, rasterising each glyph lazily in the draw path. vtium's asynchronous arrival damages only the cells showing the arrived characters, a listed departure."
    via: vtium/vtium#145, comment of 2026-10-28 headed "The operator decided that glyph swap-in is a damage source"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "The console falls back per character from the loaded set to the built-in font for the cell size (unifont from 16 px, Unscii-8 under 16): the set's strikes first, then the built-in, then the replacement glyph. A frit names `builtin:` only to change that order, and the curated frits drop their unifont and Unscii-8 sources."
    via: vtium/vtium#141, comment of 2026-10-28 headed "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "With `font-size=0`, when 16 px cells would leave fewer than 80x25 cells, vtium uses the set's smallest height: 320x240 draws 40x30 cells of 8x8, 480x320 draws 60x40, 640x400 and up are unchanged, and a CJK locale stays at 16."
    via: vtium/vtium#118, comment of 2026-10-28 headed "The operator's decision (2026-10-28)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    level: directed
    said: "HarfRust is part of the glyph engine's flow, since international text doesn't work without shaping."
    via: vtium/vtium#144, comment of 2026-10-28 headed "The operator's decisions (2026-10-28)"; posted from the operator's account, the wording agent-written
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
  - evidence: "vtium/vtium#145, Phase 1's measurements (feat/deferred at d075494), accepted by the operator on 2026-10-28: from vtium's start to the helper's first ask 51-55 ms in the tier 3 guest (1 vCPU, 512 MB); first batch 6-18 ms and whole coverage 29-39 ms for DejaVu Sans Mono there, 31-43 ms and 265-276 ms for WenQuanYi Micro Hei; the helper's peak 5.8 MB and 13.8 MB; on the development machine 2 ms and 16-23 ms with a 12 MB peak for DejaVu Sans Mono, 50-61 ms and 409-537 ms with a 43 MB peak for Noto Sans CJK; the first frame drawn in the built-in font 60 ms after start; the console's resident memory from 5.6 MiB to 6.1 MiB with DejaVu's glyphs"
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
    said: "The recipe carries an explicit compress setting, compress=yes or no, defaulting to yes, with the matching #121 template field and builder choice. With yes, create deflates 8-bit and color strikes and leaves 1-bit raw; with no, every strike is stored raw."
    via: Claude Code session, 2026-10-28, relayed by the team lead
    covers: [compressed chunks]
  - operator: aaronsb
    said: "lets accept it"
    via: Claude Code session, 2026-10-28, after reading the revised record at 497b6aa
  - operator: aaronsb
    said: "A frit's `cell=` line is where the cell lives until profiles have a file of their own (#121, #136, #137). A profile file takes it over later."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
    covers: [the cell's home]
  - operator: aaronsb
    said: "16 MiB is the store limit below 2 GiB, a 512 MB board included. Measured: the Latin fill used under 1 MB, and a full CJK cell at 8x16 is about 11 MB."
    via: the operator's answer in the team lead's session, 2026-10-28, accepting the team lead's recommendation
    paraphrase: true
    covers: [the store's limit on minspec]
  - operator: aaronsb
    said: "After login, the helper runs as the session's user, never root. It can then read that user's fonts. `nobody` stays its account before login. This lands with #31."
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
    via: the operator's decision on vtium/vtium#145, 2026-10-28, relayed by the team lead
  - operator: aaronsb
    said: "One side owns each fact: the console's per-boot store is the only record of which glyphs the console holds. The helper keeps what it sent for one request only, an ask or a fill, and skips repeats only within it. A later fill may send again glyphs the console still holds, and the console drops them; that cost is accepted."
    via: the team lead's reading of the operator's design rule (one side owns each fact), 2026-10-28
    paraphrase: true
  - operator: aaronsb
    said: "A frit whose sources are all bitmap (a kbd name, a PSF path, `FONT=` or `console:`) and that states no `cell=` takes the font's own cell, as the C reference did: it drew a PSF at its own size, scaled by whole factors. Before the first frame, the console asks the helper for that frit's cell. It waits a bounded time, about 100 ms, and falls back to the automatic cell with a logged line if no answer comes in time. This is the one case where a frame waits on the helper. It applies only to an unconfigured bitmap font, and ADR-1200's observable carries the exception. The console still never parses a font."
    via: the operator's decision on vtium/vtium#145, 2026-10-28, relayed by the team lead; the issue's comment of 2026-10-29T01:20Z headed "The operator decided how a bitmap font named without a cell finds its cell (2026-10-28)", posted from the operator's account, the wording agent-written
  - operator: aaronsb
    said: "`size=` in a frit and the `font-size` key are in points, 1/72 inch. The console converts them with the display's pixel density: pixels = points × DPI ÷ 72. DPI comes from the physical size the monitor reports in its EDID. When a display reports none (some panels, projectors, the QEMU display), vtium assumes 96 DPI and logs a warning saying the density is assumed. `cell=` stays in pixels, as an exact override. `font-size` defaults to automatic, as before. An explicit number is points. Outline frits step 1 pt, converted to pixels. All-bitmap frits keep stepping by whole multiples of the cell. This departs from the C reference, which fixed its DPI at 72, so its point size was a pixel height (src/font/font.c)."
    via: the operator's decision on vtium/vtium#145, 2026-10-28, relayed by the team lead; the issue's comment of 2026-10-28T23:26Z headed "The operator decided what font sizes measure (2026-10-28)"
  - operator: aaronsb
    said: "A `display-dpi` configuration key states the density. When it is set, vtium uses it and logs no warning. Like ADR-1301's `display-depth` and `display-subpixel` overrides, it wins over the measured value, which also covers monitors whose EDID reports a wrong size. The order is: `display-dpi` if configured, then the EDID's measured size, then 96 DPI with a warning saying the density is assumed."
    via: the operator's decision on vtium/vtium#145, 2026-10-28, relayed by the team lead; the issue's comment of 2026-10-28T23:27Z, "The operator added (2026-10-28)"
  - operator: aaronsb
    said: "4. **Zoom** steps by the source's engine, as the C reference's `increase_step` did: outline sources 1 px, all-bitmap frits by whole multiples of the cell, mixed frits 1 px. 5. **A family name** under `font-engine=pango` or `freetype` with no frit becomes a one-source `font:<family>` frit. 6. **The helper's memory** of what it sent lasts one request. It is a processor that knows only its job; the console, the protected process that consumes and dispatches, decides when a glyph is needed, again if need be."
    via: vtium/vtium#145, comment of 2026-10-28T23:25Z headed "The operator decided the open glyph questions (2026-10-28)", items 4 to 6; posted from the operator's account, the wording agent-written; item 4's pixel steps became points with the comment of 23:26Z
  - operator: aaronsb
    said: "an OSC 50 switch to a bitmap frit with no `cell=` keeps its wait of up to 100 ms for the helper's own cell, mid-session as before the first frame. A font switch is expected to resettle the screen for a moment. D17's \"the one case where a frame waits\" gains the OSC 50 switch; the ADR-1200 text is amended at the merge point with the other readings."
    via: vtium/vtium#145, comment of 2026-10-29T14:45Z headed "Decision (operator, 2026-10-29)"; posted from the operator's account, the wording agent-written
  - operator: aaronsb
    said: "every glyph the helper sends replaces the built-in drawing of its character in each renderer's cache, whether or not the character is on screen when it arrives; only the visible cells are damaged (D7: the built-in draws what the frit has not yet sent)."
    via: vtium/vtium#145, comment of 2026-10-29T14:46Z headed "Decision (operator, 2026-10-29)"; posted from the operator's account, the wording agent-written
observable:
  - "A glyph arriving for a character shown only in the scrollback view, or inside a cluster, redraws that cell on the next frame (#149)"
  - "With the store at its limit, the fill sends nothing more, and an ask evicts the glyphs least recently shown, redrawing nothing"
  - "A helper killed by its CPU limit is started again with its asks resent, at most RESTARTS_MAX times a boot; one that sends a frame it was not asked for is given up for the boot with one log line, and the built-in font keeps drawing"
  - "No file the console or the helper writes holds a glyph, and the console opens no font file"
  - "Tier 3's frit variants on the 1-vCPU, 512 MB guest draw the built-in font first and the frit once the helper answers"
  - On a cold start with a CJK frit, the first frame draws in the built-in font in the profile's cell, the frit's glyphs replace it as the helper sends them, no frame waits on the helper, and the grid's size never changes when they arrive; the frames that wait are the first of a frit of bitmap sources that states no cell, at the start or after an OSC 50 switch to it, each waiting at most OWN_CELL_MS (100 ms) for its fonts' own cell and draws in the display's cell, with one log line, when none comes
  - Tier 3's 2d-potato-min-psf draws kbd's default8x16, named by a path with no cell, in its own 8x16 cell at 1366x768, as the C reference's goldens do
status: accepted
date: 2026-10-28
deciders:
  - aaronsb
related: []
---

# ADR-1200: Glyphs are generated at run time in a sandboxed helper, from a profile's frit

## Summary

- **Decided:** vtium draws every glyph its built-in fonts do not hold by generating it at run time, from the frit a profile names, in a sandboxed helper process, on every build, minspec included. A profile, which `vtium-vitrify` builds, states a frit, its cell and its options; the console reads them as text before its first frame and parses no font. The built-in fonts, Unscii-8 under 16 pixels and unifont from 16, draw the first frame at once in that cell and stay the per-character fallback. The console asks the helper for the characters the screen shows, and while nothing is asked the helper fills the rest of the frit's coverage a batch at a time, a waiting ask going first between batches. Generated glyphs live in a per-boot memory store whose limit comes from the machine's memory: the fill stops at it, and only an ask evicts, the glyphs least recently shown first. A glyph is keyed by its cluster's text, bold, italic and width at a cell. An arrival is damage on the same path as program output, merged into one pass a frame and reaching only the cells that show the arrived characters; the grid never resizes because glyphs arrived. The helper starts on the first need, exits when idle, is restarted after a crash or its CPU limit a bounded number of times a boot, and is given up for the boot when it answers garbage. Nothing is prerendered, and no glyph outlives the boot.
- **Trades away:** Every boot parses and rasterises its fonts again: a CJK frit's whole coverage took 265-276 ms in the 1-vCPU tier 3 guest, with the helper at 13.8 MB. The first frames after a start, a zoom or a hotplug draw in the built-in font. The console trusts the helper's glyphs only to be well-formed; no root-owned file is checked by hash. Glyphs cannot be built on one machine for another.
- **One-way?** No. The pipeline lives in vtium and `vtium-vitrify` and changes in a session. Restoring a prerendered store once #145's last step removes its code is a project of several days.
- **Probes:**
  - *Not confident (the cell's home):* you decided that the profile states the cell. Profiles have no file of their own yet, so the pipeline built on `feat/deferred` reads the cell from a `cell=` line in the frit. Is the frit the right home for the cell until profiles have one? (Answered 2026-10-28; see `considered`.)
  - *Not confident (the store's limit on minspec):* the glyph store's limit follows the machine's memory as the scrollback's line cap does: 16 MiB below 2 GiB, 64 MiB from 2 GiB, 128 MiB from 4 GiB. On a 512 MB board that lets glyphs take as much memory again as the 16 MiB minspec peak the scrollback's budget is fitted inside. Keep 16 MiB there, or take a smaller share? (Answered 2026-10-28; see `considered`.)
- **Inversion:** Between "every glyph built ahead of time into a root-owned store", this record's first decision, and "the console loads its font before the first frame and rasterises in its root process", the C reference's flow. vtium generates at run time as the C reference did, in a sandboxed process instead of root, and draws the built-in font while it works instead of waiting.

> Edited 2026-10-28: the operator replaced the prerendered store with run-time generation (vtium/vtium#145: "the prerendered store is replaced by deferred rendering"), and this record was rewritten in place to decide the glyph pipeline, with the draft ADR-1201 folded in. It was titled "Fonts reach the console as vitrified sets from a root-owned store", and its Summary decided that every font reached vtium as a vitrified set that `vtium-vitrify create` built into a root-owned store, checked by owner and SHA-256 when loaded, with generation at run time as a per-boot cache beside it. Its four probes, *Confident (who builds):*, *Not confident (font operations default):*, *Not confident (compressed chunks):* and *Not confident (4-bit outlines):*, were asked of that design and answered on 2026-10-28 (`considered`). The basis entries that quote the store's design stand as said; the design they describe is under Alternatives Considered. The earlier text is in git at 64745e2.

## Context

vtium/vtium#3 proposed vitrifying fonts so the console would carry no font discovery or rasteriser, and this record first decided a root-owned store of prerendered sets. ADR-1301 then made one glyph engine run on two schedules, ahead of time and at run time, a built set being a cache of the frit's output. The operator replaced the store and its machinery, a manifest, five commands, target matching and package hooks, with run-time generation: vtium cannot prerender every font, size, cluster and display a machine meets, and a performance problem is answered by the deferred pipeline (vtium/vtium#145). Phase 1 of #145 built that pipeline on `feat/deferred` and measured it (Evidence below).

vtium draws through KMS/DRM on machines down to a Cortex-A53 with 512 MB and a 320x240 panel. The console runs as root for its whole life (ADR-1400). The renderer keeps its own drawn-glyph cache, keyed by text, style and width (ADR-1300, `crates/vtium-render/src/cache.rs`).

## Decision

The figure follows a glyph from the profile to the frame: the profile's cell and frit, the built-in font's first frame, asks and the fill going to the helper, its glyphs entering the per-boot store, the swap-in, damage and the next frame. The decisions keep the numbers code and other records cite, and appear in the order a glyph travels; D3 and D12 are retired.

![The glyph pipeline. On the left, a profile names a frit and states a cell, read as text. The cell sizes the grid and the built-in font, which draws the first frame at once. The console sends asks for the screen's characters, and fill requests while nothing is asked, to the sandboxed helper, which reads, shapes and rasterises the frit's fonts and sends glyph chunks back. The console checks each chunk and puts it in the per-boot store, which has a limit: the fill stops at it, and only an ask evicts. Each arrival is damage on the same path as program output, merged into one pass, and the next frame draws the cells that show the arrived characters. The helper starts on the first need, exits when idle, and restarts after a crash a bounded number of times a boot.](figures/ADR-1200-glyph-pipeline.svg)

### D16: profiles

- A profile is a display's capabilities, the four fields ADR-1301 D3 reads (depth, subpixel order with rotation folded in, density and resolution), named freely by its user, with the frit, the cell and the vtium options that suit that display, such as `font-antialias`, `display-depth` and `display-subpixel`. vtium ships example profiles with descriptive names, and #142's specimens are drawn per profile (`docs/specimens/`).
- The console reads a profile's settings as text, as it reads its configuration, before its first frame.
- **A head names a profile, and a profile names a frit** (ADR-1401 D1, D3). Profile files live where ADR-1401 places them, and `vtiumctl` writes them when asked (ADR-1401 D12; #121, #136, #137). Until profiles have a file of their own, the cell is a frit's `cell=` line (D1), and a profile's file takes it over when it lands (the operator, 2026-10-28).
- **The default profile.** vtium's compiled-in configuration holds a default profile, which names the default frit, and a head that names no profile takes it (ADR-1401 D3, D6). The default frit has no `source=` line, so it states no font, and a head with the default profile draws what D9's chain gives past the profile: the configuration's `font-name`, else vconsole's font where one is set, else the built-in fonts (D7).
- **A profile that fails** falls back by file: an invalid profile gives the heads that name it the default profile, and a profile naming a frit that does not exist draws the default frit, each with a log line (ADR-1401 D6, D7).
- **vtium never writes a profile or a frit** (ADR-1401 D6).
- **The more specific setting wins** (the operator, 2026-10-29): the configuration file's `font-name` and `font-size` are the base for every head; a head's profile overrides them for that head; and a `font-name` or `font-size` in the head's own section of the head file overrides both (ADR-1401 D3). `font-name` chooses the frit (D9) and `font-size` the cell (D17). A profile overrides only what it states: the default profile states no size, and its default frit no font, so a head with the default profile takes the configuration's keys. Within one level, a `font-size` wins over the cell of the frit that level names. `crates/vtium/README.md` states the rule as built before profiles have a file: "At start the font is `font-name` when a file or the command line sets it, else vconsole's [...]. Profiles have no file yet, so no profile names a frit.", and "A configured `font-size` wins over the frit's cell and size (ADR-1200 D16); `font-size=0` does not."
- The C reference had no profiles: its font came from kmscon.conf's `font-name`, `font-size` and `font-engine`.

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the more specific setting wins, which reverses the operator's answer of 2026-10-28 (`considered`) that an explicit key wins over the profile. The bullet read: "An explicit `font-size` or `font-name` in the configuration wins over the profile, as an explicit key overrides a default elsewhere (the operator, 2026-10-28): `font-size` over its cell (D17), `font-name` over its frit (D9). A key in the configuration applies to every head (ADR-1401 D3)." The default frit drew "vconsole's font where one is set, else the built-in fonts".

> Edited 2026-10-29: ADR-1401, accepted by the operator, gives profiles their place: a head names one, the compiled-in configuration holds the default profile and its default frit, an invalid profile falls back to the default, and vtium writes neither profiles nor frits. D16 read: "Where profiles are kept, and the commands that write them, are #121's, #136's and #137's."

### D1: the frit

A frit is a file `/etc/vtium/fonts/<name>.vtfrit`, the definition the helper draws from, in kmscon.conf's line syntax: one `key=value` a line, `#` comments, every line and the file bounded. An unknown key or a malformed line refuses the whole frit, naming the file and line. A frit written by hand, by `new` (D4) and from a `new --template` file are the same format.

| Key | Meaning |
|---|---|
| `name=` | The frit's name; unset, the file's stem. 1 to 64 characters of `A-Z a-z 0-9 . _ + -`, not starting with `.`, matched case-sensitively. `unifont` and `unscii-8` are reserved. |
| `source=` | Repeatable, in order: each later source fills only codepoints the earlier ones lack. An outline source is drawn at the cell (D17); a bitmap source is scaled by the largest whole factor that fits the cell and placed on its bottom row; a bitmap source that fits at no factor fills nothing. |
| `cover=` | Repeatable, for the `source=` line above it: limits that source to a named character set (`ascii`, `latin-1`, `gb2312`, `gbk`, `big5`, `jis-x-0208`, `ks-x-1001`) or a range such as `U+4E00-U+9FFF`. Unset, every glyph the source holds is in the frit's coverage. |
| `unimap=` | The Unicode map for the `source=` line above it: a kbd unimap name, resolved as `setfont -u` resolves it, or an absolute path. |
| `cp-size=` | For a CPI source above it: `8`, `14` or `16`. |
| `style=` | For the source above it: `regular`, the default, `bold`, `italic` or `bold-italic` (D21). |
| `cell=` | `WIDTHxHEIGHT`, each side 6 to 96 pixels: the cell, until profiles have a home (D16). |
| `size=` | An outline frit's starting size in points, 1/72 inch, drawn at points × DPI ÷ 72 pixels at the display's density, from which a zoom steps by 1 point (D17). A bitmap frit's is left aside. |
| `glyph-source=`, `glyph-source-<range>=` | The #85 sprite policy, with kmscon.conf's names and values. The configuration's keys override it. |
| `license=` | An SPDX identifier. Optional. |

`compress=`, which chose how a stored set was compressed, has nothing to act on: a frit that carries it is accepted with one warning naming the file and line, since refusing it would leave the console on the built-in font for a key that never changed what was drawn.

The source forms:

- `font:<family>`: a family, matched as fontconfig matches one, with case and blanks ignored: a kbd console font of that name first, else an installed outline font of that family. It names a family and nothing else (the operator, 2026-10-28, #121). A frit that names its fonts this way works on any machine with the family installed.
- `kbd:<name>`: resolved as `setfont` resolves it, in the kbd data directories (`/usr/share/kbd`, `/usr/share`, `/usr/lib/kbd`).
- `file:<absolute path>`: a PSF1, PSF2, CPI or raw font, plain or compressed with gzip, bzip2, xz or zstd.
- `bdf:<absolute path>`: a BDF bitmap font, plain or compressed.
- `vconsole`: `FONT=` and `FONT_UNIMAP=` from `/etc/vconsole.conf`, with the kernel command line's `vconsole.font=` and `vconsole.font_unimap=` over them.
- `console:<vt>`: the font the kernel holds for that VT (D8).
- `builtin:unifont`, `builtin:unscii-8`: a built-in font as a source, named only to change the fallback order (D7).
- `outline:<absolute path>`: an outline font, rasterised by swash (D14).

A font with no Unicode table takes its frit's `unimap=`, else kbd's `def.uni`, else the source fails naming `unimap=`.

```ini
# /etc/vtium/fonts/zh-cn.vtfrit
cell=12x24
source=outline:/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc
cover=ascii
cover=gb2312
source=bdf:/usr/share/fonts/fusion-pixel/fusion-pixel-12px-monospaced-zh_hans.bdf
```

### D4: `vtium-vitrify`, the profile builder

`vtium-vitrify` is its own binary in the install tree, apart from `vtium`, and holds every font decoder, parser, rasteriser and the shaper, outside the console and its budget.

| Command | Runs as | Does |
|---|---|---|
| `new FONT`, `new --template`, `new --from FILE` | the user, with `sudo` once at the end | The guided builder and its templates: guides a font to a profile and ends in a frit; at the end it shows one command, run with `sudo`, that writes the frit (#121). |
| `preview FONT...` | the user | Draws a sample of a font as the helper would for a profile, and writes nothing (#120). |
| The rule checker, a library | `vtiumctl check`, as the user | Lints a profile and its frit against a display (ADR-1301 D8, #137). `vtiumctl check FILE` checks every configuration file by its kind, frits included through vitrify's frit parser and this checker (ADR-1401 D12). |
| `set NAME` | the session's user | Switches the console's frit (D5). |
| `serve` | the account D15 names | The helper (D15). |

Each command is non-interactive where it writes, prints one line a result, and exits 0 on success, 1 on a failure and 2 on a usage error; #112's banner shows only at an interactive terminal. vtium/vtium#3 tracks the tool.

> Edited 2026-10-29: the team lead's correction from the heads gap-check (C9): ADR-1401 D12's `check FILE` and the rule checker are one command. The row read: "| The rule checker | the user | Lints a profile and its frit against a display (ADR-1301 D8, #137). |"

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the configuration tool is `vtiumctl`, a binary of its own, and `vtium-vitrify` keeps the fonts. The row read: "| `check FILE`, with the rule checker | the user | Checks any configuration file by its kind, a head file, a profile or a frit, as ADR-1401 D12 has it, and lints a profile and its frit against a display (ADR-1301 D8, #137). |"

### D11: the frit drawn when nothing names one

> Edited 2026-10-28: D11 chose the sets a machine held and their sizes: per locale, one bitmap set scaled by whole factors and at most one outline size for the display, built by `create --default` from a package hook, with their disk costs. With nothing built ahead of time, that choice is retired, and its figures are under Alternatives Considered.

With no font named, a head takes the default profile and its default frit (D16), which draws vconsole's font where one is set, else the built-in font (D9), and vtium picks no frit by locale (the operator, 2026-10-28). A frit for a locale can come later as a profile.

> Edited 2026-10-29: the default profile and frit of ADR-1401 D6 take this place (D16). It read: "With no font named, vtium draws the built-in font (D9), and picks no frit by locale".

### D17: the cell

- **From the profile, before the first frame.** The console reads the profile's cell as text before it draws, so the grid has its size from the first frame, without blocking on the helper and without parsing a font.
- **A frit of bitmap sources that states no cell takes its fonts' own cell,** as the C reference drew a PSF font at its own size, scaled by whole factors (the operator, 2026-10-28). The console sees from the frit's text that every source is a bitmap one (a kbd name, a PSF path, `FONT=`, `console:` or a built-in) and that no `cell=` is stated, and with no `font-size` configured it asks the helper for the cell before the frit's first frame, since the console parses no font: at the start, and at an OSC 50 switch to such a frit mid-session (D5), since a font switch is expected to resettle the screen for a moment (the operator, 2026-10-29). The helper answers the first source's cell when every other source's fits in it, and refuses an outline source or a source whose cell does not fit. The console waits for the answer at most `OWN_CELL_MS`, 100 ms, the one case where a frame waits on the helper, at the start or at that switch, and takes the cell at the largest whole multiple not over the automatic rule's height, at least once; a zoom steps by its height. A refusal, or no answer in time, leaves the display's cell with one logged line, and a later answer is dropped.
- **With no profile,** the cell comes from the display, by the automatic rule: at most 120 columns, taking a cell as half as wide as it is high, at least 40 rows, and never under 16 pixels. #118's small-display rule takes precedence: where 16-pixel cells would leave fewer than 80 by 25, the built-in font's smallest height is used, so 320x240 draws 40 by 30 cells of 8x8 and 480x320 60 by 40, and a zh, ja or ko locale stays at 16. The cell's width is the built-in font's at that height.
- **Sizes are points,** 1/72 inch (the operator, 2026-10-28): a configured `font-size` and an outline frit's `size=` draw at points × DPI ÷ 72 pixels, to the nearest pixel. The density is `display-dpi` where it is configured, with no line logged; else the display's, measured from the physical size it reports (ADR-1301 D3); else 96 DPI, with one warning for each display that reports none. Where several displays share one grid, a head mirrored across them (ADR-1401 D1), the lowest density among them converts, and a larger display draws larger by the layout rule. Points do not apply to bitmap fonts: an all-bitmap frit takes its stated cell or its fonts' own.
- **`cell=` and `size=`:** `cell=` is the cell in pixels, an exact override, and wins over the frit's `size=`; `size=` is an outline frit's starting size in points, and gives the cell its height where no `cell=` is stated.
- **A configured `font-size`** sets the height the automatic rule would pick, in points. A profile's cell overrides the configuration file's `font-size` for its head, and the head's own `font-size` overrides both (D16).
- **The grid resizes only on an explicit event:** a zoom, or a display hotplug while the cell is automatic. A resize sets the pty's size, which sends SIGWINCH, and redraws; the glyphs for the new cell are asked for, and the built-in font draws meanwhile. Glyphs arriving never move the cell.
- **The helper generates for the cell and never moves it.** A frit that cannot fit the cell fails with a logged reason, and the built-in font keeps drawing. A profile whose cell does not suit its frit is linted by `vtium-vitrify` when it builds or checks the profile, and by `vtium check-config`.
- **A zoom steps by the frit's engine,** as the C reference's zoom stepped by its engine's `increase_step` (C notes), starting from the frit's `size=` (D1) (the operator, 2026-10-28): an outline frit, and a frit mixing outline and bitmap sources, by 1 point, to the next whole point whose pixels at the density differ, and an all-bitmap frit by whole multiples of its cell.

The C reference loaded its font before its first frame and took the font's cell; vtium has the same certainty from the profile without waiting on a font, and waits only for a bitmap frit's own cell, bounded.

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the more specific setting wins, and the `font-size` bullet takes D16's order. It read: "A configured `font-size` sets the height the automatic rule would pick, in points, and wins over a profile's cell (D16)."

> Edited 2026-10-29: the operator decided that an OSC 50 switch to a bitmap frit with no `cell=` waits for its fonts' own cell as the start does (`considered`), as the code already did (`crates/vtium/src/app.rs`, `osc_50_to_a_bitmap_frit_with_no_cell_takes_its_fonts_own_cell`); that zoom steps by the frit's engine, a mixed frit as an outline one (`considered`, #145's item 4); and ADR-1401 D1 accepted that the lowest density converts where displays share one grid. The roles of `cell=` and `size=` moved into a bullet of their own. The bitmap bullet read "it asks the helper for the cell before the first frame [...] the one case where a frame waits on the helper, and takes"; the sizes bullet ended "and `cell=` stays in pixels, an exact override."; and the zoom bullet read "A zoom steps through the frit's sizes, as the C reference's zoom stepped by its font's `increase_step` [...] an outline frit by 1 point [...] and an all-bitmap frit by whole multiples of its cell."

> Edited 2026-10-28: the operator decided that sizes are points converted at the display's density, with `display-dpi` stating it (`considered`), which D1's `size=`, this decision and the departures now carry. `size=` and a configured `font-size` were pixel heights, a bitmap frit's `size=` set its height, and an outline frit zoomed by 1 pixel; the earlier text is in git at 9ccffbc.

> Edited 2026-10-28: the operator decided that a frit of bitmap sources with no cell takes its fonts' own cell (`considered`), which this decision, the observable, D10, the departures, the consequences and the alternatives now carry. A bitmap frit with no cell stated drew in the display's cell, and no frame waited on the helper; the earlier text is in git at fa6bbf6.

### D7: the built-in fonts

- **`unifont`:** GNU Unifont 17.0.03 under the SIL Open Font License 1.1.
- **`unscii-8`:** Unscii by Viznut, `unscii-8.hex` from <http://viznut.fi/unscii/unscii-8.hex>, SHA-256 `03094f7fbab7085cf6a6b624cee61e47e71ce5d0c2f308c2f4436afdc17f776c`, 3,191 glyphs, all 8x8 but U+0000, which is 16x8, in the public domain: `unscii.txt` in `unscii-2.1-src.tar.gz` (<http://viznut.fi/unscii/unscii-2.1-src.tar.gz>, SHA-256 `4d1215d495e49d005ae70828ec1529e997049b9a7f152b9439d78c7e70c65b57`) says "Everything made by me (viznut) is in the Public Domain.", and only `unscii-16-full`, which vtium does not use, falls under the GPL. vtium pins the published file by hash, with its URL and `unscii.txt` beside it in vtium-font's `data/`.
- Both are compiled into the binary, their glyph chunks deflated and inflated on first use, and trusted by construction. They are the only fonts the console's process holds itself.
- vtium draws `unscii-8` for cells under 16 pixels high and `unifont` from 16, except that `unscii-8` is never chosen in a zh, ja or ko locale, since it holds no CJK; there `unifont` serves. Unifont covers GB2312, GBK, Big5, JIS X 0208 and all 11,172 Hangul (measured), so a CJK console reads from its first frame.
- **The first frame:** the built-in font draws at once, in the cell D17 gives: centered when the cell is larger than the built-in's, and in the built-in's smaller strike, cropped where even that does not fit, when the cell is smaller. Its glyphs come at the cell's size, so the renderer stores them as it stores the frit's.
- **The per-character fallback** (#141): a character the frit lacks, or has not yet sent, draws in the built-in font for the cell, then as the replacement glyph (D22). A frit names `builtin:` only to change that order, and vtium's own frits carry no copy of unifont or Unscii-8.
- **Where the helper is unavailable,** as with `vtium-vitrify` not installed, the helper given up for the boot (D15) or a frit that fails, the built-in font draws everything, and the console logs one line naming the frit and the reason.
- Fusion Pixel 12 is not built in: it is 362 KB compressed against the 192 KB the binary's budget leaves, and one compiled-in variant would give Japanese users Chinese glyph forms.

### D9: the font at start, and its fallback

- The font is, the most specific first (D16): the head's own `font-name` (ADR-1401 D3); unset, the frit the head's profile names, where that frit has a source; unset, `font-name` from the command line or kmscon.conf; unset, `vconsole.font=` from the kernel command line, then `FONT=` from `/etc/vconsole.conf`; unset, the built-in font.
- A name is looked up as a frit under `/etc/vtium/fonts`. A kbd name, a font path given with `font-engine=psf`, a family given with `font-engine=freetype` or `pango`, or vconsole.conf's `FONT=` that no frit bears becomes a one-source frit made on the fly, with a `kbd:`, `file:`, `font:<family>` or `vconsole` source (D1), so existing console setups keep working (the operator, 2026-10-28).

> Edited 2026-10-29: the operator accepted an independent advisor's review of ADR-1401 (ADR-1401's `considered`): the more specific setting wins, and the first bullet takes D16's order. It read: "The font is `font-name` from the command line or kmscon.conf, which wins over the profile (D16); unset, the frit the head's profile names; unset, `vconsole.font=` [...]".

> Edited 2026-10-29: the operator decided that a family under `font-engine=pango` or `freetype` with no frit becomes a one-source `font:<family>` frit (`considered`, #145's item 5). The bullet read: "A kbd name, a font path given with `font-engine=psf`, or vconsole.conf's `FONT=` that no frit bears becomes a one-source frit made on the fly, with a `kbd:`, `file:` or `vconsole` source (D1)".
- A name that is neither a frit nor a source the helper can read draws the built-in font and logs one line: the name and the reason. A console with no font named draws the built-in font and logs nothing.

### D15: the helper

The helper is `vtium-vitrify serve`, a separate process the console starts, running the same reader, shaper and rasteriser `preview` runs (D14; ADR-1301 D1), so a frit draws the same wherever it is drawn.

- **Sandboxed.** It runs with an empty environment in `/`, in its own process group, holding only its pipes, with no-new-privileges, under a memory limit and a CPU limit over its life, and never as root: as `nobody` before login, the last account ADR-1400 D2 names there, and as the session's user after login, so it reads that user's fonts (the operator, 2026-10-28; the session's user lands with #31). It refuses to start as root. It reads the frit and its fonts itself, so a font must be readable by that account; the console's root process parses no font.
- **One helper for each frit in use,** and, once a session has logged in, for each session user of that frit; a helper is never shared across users (ADR-1401 D1).
- **Started on the first need, exited when idle.** The console starts it on its first ask or fill. It exits after `HELPER_IDLE_MS`, 60,000 ms, with no request and no fill, or when the console closes its end and its work is done; the next need starts a fresh one (ADR-1301 D2).
- **Not persisted.** No state a hostile font plants survives the process, and its CPU and memory budgets reset with each start. A restart narrows the window; the console's checks and the sandbox are what contain a vulnerability.
- **Restarted after a failure, a bounded number of times a boot.** One that crashes, reaches its CPU limit, gives no answer for `ANSWER_MS` (10,000 ms) while work is outstanding, leaves an ask unanswered for `REQUEST_MS` (60,000 ms), or exits with work outstanding is started again with its asks and fills sent again, at most `RESTARTS_MAX` times a boot. Each start re-randomizes its address layout, so an unbounded count would let an exploit retry until its guess lands. `RESTARTS_MAX` is 3.
- **Given up for the boot on garbage.** A helper that answers what is not a frame, what was not asked, or a failure is given up for the boot, logged once, and the built-in font goes on drawing (D7).
- **Checked, never trusted.** Each frame is checked before anything is allocated for it: its length against `FRAME_BYTES_MAX` (16 MiB), its glyphs through vtium-font's bounded reader, each at a cell asked for, an ask's glyphs among those asked for and not yet sent, a fill's among those the cell does not hold. A thread writes the requests and another reads the answers, so neither a slow nor a hung helper stalls the console's loop.

> Edited 2026-10-29: the team lead's correction from the heads gap-check (C5), as ADR-1401 D1 decides: several consoles logged in as different users cannot share one helper running as one of them, and heads with different frits need a helper each. D15 spoke of one helper, "a separate process the console starts".

### D14: parsing and rasterising safely

- Every font file is parsed in a process that is not the console: the helper (D15), or the child `preview` and `new` hand a font to, which drops to an unprivileged account, sets no-new-privileges, holds no descriptor but its pipes, and runs under a memory limit. A panic, fault or timeout in either fails that source and nothing else. The font research found swash panicking on Fusion Pixel's OTB ("chunk size must be non-zero").
- The outline rasteriser is swash, the only pure-Rust candidate with hinting, which matters most at 12-16 pixels, embedded bitmaps and color (CBDT and COLR): 4.0, 4.6 and 8.5 µs a glyph at 12, 16 and 48 pixels, 600 KB of code (measured). ab_glyph is faster (2.2 µs, 111 KB) but has no hinting, bitmaps or color and cannot read pixel fonts; fontdue processes every glyph at load, 278 MB for Noto Sans Mono CJK, which rules it out on 512 MB (measured).
- Pixel fonts are read by `vtium-vitrify`'s own BDF reader, never through swash.
- Shaping is HarfRust's (#144, ADR-1301 D1), in the helper. A cluster the console asks for is text a program wrote, so the shaper and the font parser behind it face input anyone with the terminal can choose; they run only in the helper, and the shaping path takes a fuzz target (#144).

### D6: decoders and rasterisers stay out of the console

- `vtium-vitrify` holds every decoder and reader: gzip, bzip2, xz and zstd, PSF1, PSF2, CPI, raw fonts, BDF, unimaps, kbd lookup, vconsole.conf, the outline reader, swash and HarfRust, each pure Rust with no C library.
- The `vtium` binary holds the frit's and the profile's text readers, the bounded glyph-chunk reader (D2), the built-in fonts and the inflate they need. The console's process parses no font, decompresses no file, shapes nothing and rasterises nothing, in any build.

### D8: the kernel's font

The console, which opens its VT as root already, reads the kernel's font once at start, while the VT is still in text mode and before it takes the VT for graphics: KDFONTOP's `KD_FONT_OP_GET_TALL`, falling back to `KD_FONT_OP_GET` on a kernel without it, and the Unicode map with GIO_UNIMAP. The kernel returns a plain bitmap and a map, so the console reads two ioctl replies and parses no font. It hands the bytes to the helper, which reads them as a frit's `console:` source, unprivileged, as it reads any font. The ioctls live in vtium-platform, the only crate with `unsafe`, each with the test of its request number. A VT already in graphics mode is refused by the kernel, and that source then fills nothing.

### D18: asks and the fill

- **Asks come first.** An ask names the characters a cell shows that the store does not hold, has not been told the frit lacks, and has not already asked for; the helper answers it whole, ahead of anything else.
- **The fill.** While nothing is asked, the helper fills the cell's coverage in order: the screen's characters, then printable Latin and box drawing, then the rest of the frit's coverage, 1,024 codepoints a batch, skipping what the console holds as the fill names it.
- **One record of what is held.** The console's store (D19) is the only record of which glyphs the console holds. The helper keeps what it sent for one request only, an ask or a fill, and sends a glyph once within it; a later request may send again a glyph the console holds, and the console drops it. The helper knows only its job; the console, the protected process that consumes and dispatches, decides when a glyph is needed, again if need be (the operator, 2026-10-28).
- **Preemption between batches.** A waiting ask is taken before each batch, so it waits at most one batch.
- **The fill stops at the store's limit** (D19), so speculative glyphs never churn the store. A stop ends a cell's fill, as on a zoom away from that cell.
- **Glyphs already held** are drawn from the store, with no request.

> Edited 2026-10-28: the fill skipped what the console held and what the helper had sent at the cell in any earlier request, so the helper kept a second record of what the console held. The team lead's reading of the operator's design rule, one side owns each fact (`considered`), leaves the store as the only record: the helper skips repeats within one request only. The earlier text is in git at e807139.

> Edited 2026-10-29: the operator decided the helper's memory of what it sent lasts one request (`considered`, #145's item 6), the rule the bullet already stated; the bullet gains the decision's reason, that the console decides when a glyph is needed.

### D19: the store

- **Per boot, in memory.** Generated glyphs live in the console's memory for the boot, and nothing the console or the helper writes to disk holds a glyph. The store holds only the cells the displays draw at; a zoom away drops the old cell's glyphs.
- **Its limit comes from the machine's memory,** the way the scrollback takes its own budget, with no new key or size decision: 16 MiB below 2 GiB, 64 MiB from 2 GiB and 128 MiB from 4 GiB, the tiers the scrollback's line cap takes (`store_limit` on `feat/deferred`, `sb_size_cap` in vtium-term). A 512 MB board takes 16 MiB (the operator, 2026-10-28): the Latin fill used under 1 MB, and a full CJK cell at 8x16 is about 11 MB (measured).
- **Only an ask evicts,** the glyphs least recently shown first; the fill stops at the limit instead. An evicted glyph is asked for again when the screen shows it.
- **Eviction compacts** (the operator, 2026-10-28). The store holds each helper answer as one set. When an ask's glyphs arrive with the store full, enough of the glyphs least recently shown leave to cover the overrun in one pass, and the console writes each set they leave again without them, through vtium-font's set writer, each glyph in its own pixel format, 1-bit glyphs staying 1-bit. The store counts the bytes it wrote, so its limit bounds the memory the sets take. The writer adds 71,472 bytes to minspec's console binary, 2,683,344 to 2,754,816 (measured), 22,096 of them its deflate, which the rewritten sets do not use.
- **Rejected:** hiding evicted glyphs without freeing them, which lets the store outgrow its limit while its count says otherwise; and storing glyphs one by one, which needs a second copy of vtium-font's drawing in the console, against ADR-1301's one engine.
- vtium keeps no prerendered or persistent glyph cache of any kind (Alternatives Considered).

> Edited 2026-10-28: the operator decided how a glyph leaves the store (vtium/vtium#145, `considered`), and D19 takes the decision in its compaction and rejected bullets. D19 said only that an ask evicted the glyphs least recently shown; the store then evicted a whole helper answer at a time, aged by its most recently shown glyph.

### D20: keys

- A glyph is keyed by its cluster's text (its codepoints), bold, italic and its width in cells, at a cell, as the renderer's cache keys a drawn glyph (ADR-1300, `crates/vtium-render/src/cache.rs`). The store keys by the same, so a glyph found in one is found in the other. On `feat/deferred`, which generates regular glyphs per codepoint (D21), a key's text is one codepoint and its style regular.
- Keying by the text itself needs no symbol table and no generation counter: two cells holding the same cluster share its glyph however their clusters were made, and a key can never name a different cluster later.
- Color, underline and strike stay out of the key: a glyph is a coverage mask, or RGBA for a color glyph, colored when it is composed, and the decorations are sprites vtium-render draws (ADR-1300).

### D21: cluster asks and style asks

*Intended design, not yet built.* The helper on `feat/deferred` generates regular glyphs per codepoint (`crates/vtium/src/helper.rs`, `crates/vtium/src/deferred.rs`).

- **Cluster asks.** The console asks the helper for a cluster, its text as the key gives it, and the helper shapes it with HarfRust and composes its glyphs into one glyph fitted to its 1 or 2 cells (#144, ADR-1301 D1).
- **Style asks.** The console asks for bold and italic as part of the key. The helper draws them from the frit's bold and italic faces (`style=`, D1) where the frit names them, and from the regular glyph emboldened, as vtium-font draws a missing style today, where it does not.

### D22: the lookup chain

The console finds a cell's glyph in the order ADR-1301 D1 sets, and parses no font at any step:

1. the cluster's glyph in the store (D19);
2. the cluster's NFC form, where it precomposes to one codepoint the store holds;
3. the base's glyph with each mark's glyph overstruck on it;
4. the built-in font for the cell (D7);
5. an ask to the helper (D18), drawn meanwhile from step 4 or 6;
6. the replacement glyph: U+FFFD where the font has it, else U+25A1.

### D2: the glyph chunk format, on the helper's pipe

The helper sends glyphs in the chunk format vtium-font reads, little-endian, every count, size and offset bounded by a constant in vtium-font's `src/bounds.rs`: a header with a magic and a format version, strikes each with its cell, baseline, underline and strike positions and thickness, pixel format and glyph count, runs of codepoints of one width, and glyph chunks of at most 16 KiB. The console reads it with the crate's no-panic reader and holds what it reads as the store's (D19). The format is a wire format: nothing writes it to disk, so it carries no file hashes, and a change to it changes both ends of the pipe in one release.

### D13: how glyphs are held and drawn

- Outline glyphs are R8, 8-bit coverage; bitmap glyphs 1-bit, expanded to R8 as each is drawn; color glyphs premultiplied RGBA. Every glyph the console hands the renderer is R8 or RGBA, which serves the CPU path now and a GPU path later (#33). Outline glyphs stay 8-bit: displays are good enough that 4-bit quantization would likely show (the operator, 2026-10-28).
- The CPU path's blend loop is written so the compiler vectorises it, with no hand-written SIMD: NEON is always present on aarch64, and on x86_64 an AVX2 version is chosen at run time. Per glyph, SSE2 against AVX2: 85 against 29 ns at 16 pixels, 597 against 264 ns at 48 (measured). The run-time dispatch needs `unsafe` or a crate outside vtium-render, which forbids `unsafe`.
- BC4 loses up to 25/255 and the boards minspec targets lack it; SDF and MSDF cost 55 times R8 on the CPU and blur small sizes, and stay a possible later form for the GPU path alone.

### D23: damage

- **Arrival is a damage source on the same path as program output.** Glyphs that arrive between frames merge into one set of cells, and the next frame draws it in one pass.
- **Damage comes from what each buffer displays,** the scrollback view and cluster cells included (#149): the cells to redraw are those whose shown text contains an arrived character, read from the view each display's buffer shows. A fill of characters no buffer shows draws no frame.
- **Every glyph that arrives replaces the built-in drawing** of its character in each renderer's drawn-glyph cache (ADR-1300), whether or not the character is on screen when it arrives, so a character shown later draws in the frit; only the cells that show it are damaged (the operator, 2026-10-29).
- **Eviction damages nothing:** cells already drawn keep their pixels, and an evicted glyph is asked for again when the screen next needs it.
- **The departure.** The C reference had no arrival: it loaded its font before drawing, rasterised a glyph inside the frame that asked for it, and redrew every cell of every buffer whenever the font changed (C notes). vtium redraws only the cells that show the arrived characters.

> Edited 2026-10-29: the operator decided that every glyph the helper sends replaces the built-in drawing of its character in each renderer's cache, shown or not (`considered`), after #145's verification found a glyph arriving while its character was off screen never replaced the built-in drawing. D23 named only the cells an arrival damages.

### D5: `vtium-vitrify set`, through OSC 50

- `set NAME` writes `OSC 50 ; NAME ST` to its controlling terminal, asks `OSC 50 ; ? ST`, and reads the reply in raw mode for at most a second. It exits 0 when the reply names NAME, and otherwise says font operations are off or the terminal is not vtium.
- vtium takes OSC 50 only with `allow-font-ops` on, which defaults on, as xterm's `allowFontOps` does. Its description says why: xterm's convention, and OSC 50 can only switch among frits root wrote and the built-in fonts. Off, vtium ignores OSC 50, the query included.
- A `Pt` that names a frit under `/etc/vtium/fonts`, or a built-in font, switches the live console: the built-in font draws what the helper has not yet sent for it, and the helper is asked (D18). The cell changes only where D17 lets it. Anything else, a path, an X font name or a name with no frit, leaves the font as it was and logs why.
- `OSC 50 ; ?` replies `OSC 50 ; <name> ST` with the current frit's or built-in font's name, from vtium's own state, within the reply rule. A switch changes the live console only; `font-name` in the configuration is what persists.

### D10: how it is judged

- **The pipeline:** with a stand-in helper that answers after a delay, the first frame draws in the built-in font in the profile's cell, no frame waits on the helper but a bitmap frit's first for its own cell, the grid's size is the same before and after the glyphs arrive, and the frame after they arrive draws the frit's glyphs in only the cells that show them, scrollback view and clusters included.
- **Asks and the fill:** an ask sent while a fill runs is answered before the next batch; the fill stops at the store's limit; an ask past it evicts the glyphs least recently shown; eviction redraws nothing.
- **The helper:** a crash, the CPU limit, a hang and an exit with work outstanding each restart it, up to `RESTARTS_MAX` times a boot; a frame it was not asked for gives it up for the boot with one log line; it refuses to start as root.
- **Nothing outlives the boot:** no file the console or the helper writes holds a glyph.
- **One engine:** the same frit, cell and display give the same glyph bytes from a minspec build and a full one.
- **vtium-vitrify:** frit parsing, each key and each refusal with its line; `compress=` accepted with its warning; kbd lookup's order of directories and suffixes; vconsole.conf and the kernel command line; each compression; PSF1, PSF2, CPI, raw fonts, BDF and unimaps; `cover=` against each named set's table; a font that panics the parser failing only its source; fixtures generated in Rust in vtium's tree.
- **KDFONTOP and GIO_UNIMAP:** request numbers in vtium-platform; the live read only in tier 3's guest, never on the maintainer's desktop.
- **Tier 3:** the frit variants on the minimal guest, 1 vCPU and 512 MB, draw the built-in font first and the frit once the helper answers; their goldens are vtium's own, provisional until the operator reviews them.

### Retired

- **D3, the trusted store,** and **D12, portable sets and the two ways to build them,** are retired with the prerendered store (Alternatives Considered).

### For the operator's check

The restart cap of 3 (D15) and the batch of 1,024 codepoints (D18) are agent calls the operator has not yet looked at.

> Edited 2026-10-28: the operator answered the seven points this section listed as not decided, each as the team lead recommended (`considered`), and each answer is folded into the decision that named it: the cell's home (D16), the store's limit on minspec (D19), the helper's account after login (D15), a profile against the configuration (D16, D17, D9), zoom with a frit (D1, D17), the frit drawn when nothing names one (D11), and names that are not frits (D9). The section was headed "Not decided" and asked each as an open question; the earlier text is in git at 4477806.

## Evidence: the deferred pipeline measured

Phase 1's measurements on `feat/deferred` at d075494, accepted by the operator (vtium/vtium#145):

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
- `terminal_get_auto_font_size` took the smallest display's size: twice its width over 120, at most 120 columns with a cell half as wide as it is high; at most its height over 40, at least 40 rows; never under 16 px; 16 when no display size was known (`src/terminal.c:899-919`).
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
- On a miss, `find_glyph` read the cluster's codepoints, called `kmscon_font_render` with bold and italic, rotated the glyph and inserted it (`src/render/bbulk.c:525-576`), from inside the cell's draw (call at 1427): rasterising was lazy and in the draw path, in the root process. `kmscon_font_render` drew box drawing and its kin itself, then the font's glyph, then U+25A1, then `?` (`src/font/font.c:311-368`).
- Color, underline and strike stayed out of the key: underline and strike were decoration sprites the renderer drew (`src/font/font.h:58-62`), and color was applied when the mask was composed. The older `tsm_screen_draw` id carried underline, inverse and blink bits as well (`src/tsm/tsm-render.c:296-312`); the frame key the cache used did not.

### Damage

- libtsm stamped each cell and line with an `age` from the screen's `age_cnt` as it changed (`src/tsm/tsm-screen.c:114-127`, `176`, `544-552`) and raised the counter once a frame (`screen_inc_age`, `src/tsm/libtsm-int.h:546-550`). On the counter's wrap, `screen_age_wrap` set every age to 1 and raised `age_reset` and `age_epoch` (`src/tsm/tsm-screen.c:214-236`).
- `tsm_screen_draw` took a cell's age as the largest of its own, its line's and the screen's, or 0 under `age_reset` (`src/tsm/tsm-render.c:284-292`); the frame API marked a cell changed when its age was newer than the caller's, and every cell on a new epoch or a new shape (`screen_frame`, `tsm_screen_frame_changed_since`, `src/tsm/tsm-render.c:587-715`, `741-775`).
- kmscon's `kmscon_text_redraw_all` emptied a buffer's age, so its next draw marked every cell changed (`src/render/text.h:214-217`): on `kmscon_text_unset`, which `kmscon_text_set` called first on each font change (`src/render/text.c:271-323`), on `kmscon_text_resize` (338-352), on a rotation (430-433), after a switch of screen or a failed draw (741-770), and in bbulk for two frames after the default or decoration colors changed (`src/render/bbulk.c:1797-1810`).
- A glyph existed the moment a frame asked for it, so no glyph arrived between frames and no arrival damaged anything.

## Where vtium departs from the C reference

- **Generated in a sandboxed helper, not in the root process:** the C reference parsed and rasterised fonts as root, in the draw path; vtium does both in the helper (D14, D15).
- **The first frame does not wait for a font:** the C reference loaded its font before drawing; vtium draws the built-in font at once and swaps the frit's glyphs in (D7, D17).
- **Sizes are points at the display's density:** the C reference fixed its density at 72 DPI, so its point size was a pixel height (`src/font/font.c:198-199`); vtium converts with `display-dpi`, else the density the display measures, else 96 DPI with a warning (D17).
- **The cell comes from the profile:** the C reference took its cell from the loaded font; vtium reads it from the profile as text, takes a bitmap frit's own cell only when the helper tells it within 100 ms, and the automatic rule yields to #118's small-display rule (D17).
- **Arrival damages only the cells that show it:** the C reference had no arrival and redrew everything on a font change (D23).
- **Glyphs keyed by text:** the C reference keyed its cache by a symbol and a generation; vtium keys by the cluster's text and needs neither (D20).
- **A per-character fallback to the built-in font,** and U+FFFD before U+25A1: the C reference fell back from its font to U+25A1, then `?` (D7, D22).
- **A store with a memory limit where only an ask evicts:** the C reference's LRU held twice the screen's cells and evicted on every insert past it (D19).
- **From `setfont`:** a named font with no Unicode table and no unimap takes kbd's `def.uni`; `setfont` left the kernel's map in place, and a frit has no kernel map to inherit (D1).

## Consequences

### Positive

- One path draws every font on every build, with no store, manifest, hash chain, build commands or package hooks to keep.
- A frit takes effect as soon as it is written, for whatever display it lands on.
- A start never waits on a font but for a bitmap frit's own cell, at most 100 ms, and the grid never jumps when glyphs arrive.
- The console's process parses no font, decompresses no file and shapes no text; each runs in a sandboxed process that does not outlive its idle interval.
- Memory follows what the screen shows: the store's limit bounds it, and the fill never churns it.
- A redraw after an arrival is as small as the cells that show the arrived characters.

### Negative

- Every boot parses and rasterises its fonts again, and a CJK frit's helper holds 13.8 MB while it works on the minspec guest.
- The first frames after a start, a zoom or a hotplug draw in the built-in font.
- An OSC 50 switch to a bitmap frit with no `cell=` holds the screen up to 100 ms for its fonts' own cell.
- The console trusts the helper's glyphs only to be well-formed.
- A frit's fonts must be readable by the helper's account.
- Glyphs cannot be built on one machine for another, and distributions cannot ship prebuilt glyphs.

### Neutral

- The store's code, its commands, set loading and tier 3's set-building step are removed in #145's third step.
- ADR-1301's two schedules become one, and its references to built sets take dated notes citing this record.

## Alternatives Considered

- **A prerendered store,** as this record first decided: `vtium-vitrify create` building vitrified sets into a root-owned store with a manifest of SHA-256 hashes, or `create --out` building a portable `.vtglass` file anywhere that `install` added to the store, the console loading only checked sets, and package hooks building the locale's default sets at install. Replaced by the operator: "vtium can't prerender the universe, so that complexity isn't carried" (#145). The store, its manifest and five commands existed only to keep a cache of what the helper generates.
- **A persistent glyph cache across boots,** per user or per machine, so a warm start skips rasterising. Not taken, by the operator: no prerendered cache of any kind, persistent or otherwise, beyond the helper's per-boot memory cache.
- **Sets sized per locale and display,** each one to three adjacent sizes. A ladder of sizes was 861 MB for full GBK alone (measured), and the default sets of a zh_CN desktop about 33 MB, 120 MB at 7680x2160. Retired with the store.
- **Compressed stored sets,** deflated 8-bit and color strikes at 2.2 to 4.7 times smaller, and **4-bit strikes** at half the size. Retired with the store; outline glyphs stay 8-bit.
- **The console loads its font before the first frame,** as the C reference did. Not taken: a CJK frit takes a quarter of a second or more to generate on minspec, and the built-in font draws at once.
- **No built-in font,** waiting for the helper's first glyphs. Not taken: the screen would be blank until the helper answers, and blank where it cannot run; the C reference likewise kept a compiled-in fallback.
- **Generated glyphs fill the built-in font's cell,** as phase 1 drew. Replaced by the operator: the profile states the cell, and the grid never resizes because glyphs arrived.
- **The console reads font files,** as the C reference's `font-engine=psf` did. Not taken: every decoder and parser would sit in the root process, facing files a user can name.
- **The console reads a bitmap font's header for its own cell.** Rejected by the operator: the first exception to the console never parsing a font. The helper tells the cell instead (D17).
- **A bitmap frit with no cell keeps the display's cell.** Rejected by the operator: an existing PSF setup would draw in the display's cell, 8x19 at 1366x768, instead of the font's 8x16.
- **Rasterising in the console's root process,** as the C reference did. Not taken: a font file would be parsed as root, and swash panicked on a pixel font's OTB (D14).
- **A persistent helper,** kept for the boot once started. Not taken, by the operator: state a hostile font plants would survive, and the CPU and memory budgets would never reset.
- **Unbounded restarts.** Not taken, by the operator: each restart re-randomizes the address layout, so an exploit could retry until it lands.
- **Keys by symbol and generation,** as the C reference's cache keyed them. Not taken: vtium's screen keeps a cluster's text with its cell, and keying by that text needs no table to intern it and no counter to tell reused keys apart (D20).
- **A full redraw on each arrival,** as the C reference redrew on a font change. Not taken: arrivals come in batches through a fill, and each would repaint the whole screen.
- **ab_glyph or fontdue as the rasteriser.** Not taken: ab_glyph has no hinting, embedded bitmaps or color and cannot read pixel fonts; fontdue needs 278 MB at load (D14).
- **A per-session socket for `set`.** Rejected by the operator in favor of OSC 50.
- **Fusion Pixel 12 built in.** Not taken: it is twice the budget left, and one variant gives other locales the wrong glyph forms (D7).
