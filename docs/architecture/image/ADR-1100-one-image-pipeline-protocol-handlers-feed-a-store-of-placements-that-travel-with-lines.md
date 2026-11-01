---
contract: adr/v1
kind: decision
verb: add
capability: images
basis:
  - operator: aaronsb
    level: directed
    said: "One image pipeline, shared by both protocols: the store, the protocol tag and its rules, the decode boundary, and the handler trait the protocol front ends implement."
    via: vtium/vtium#28, the issue body, posted from the operator's account; the steering session's brief for this record names #28's design as the maintainer's direction
  - operator: aaronsb
    level: directed
    said: "The difference is that one function holds every protocol-specific rule, so the store and the compositor never branch on the protocol themselves."
    via: vtium/vtium#28, the issue body, the same text
  - operator: aaronsb
    level: directed
    said: "the store's use clock is per store and advances only on stream events (a transmission, a placement, a reference), never on a drawn frame."
    via: vtium/vtium#28, comment headed "Determinism follow-up (vtium/vtium#50)", posted from the operator's account
  - operator: aaronsb
    level: directed
    said: "kitty graphics relative placements (`image.kitty-relative`) and Unicode placeholders (`image.kitty-placeholders`) are both supported targets, as ADR-1000's tables now record (integration 73adb77). Both belong to this issue."
    via: vtium/vtium#49, comment of 2026-10-27 headed "The operator (2026-10-27)", posted from the operator's account
  - upstream: "kmscon ADR-103 (aaronsb/kmscon b1f7a7c5, docs/architecture/render/): one store per screen buffer; placements anchored to the line of their top row; sixel as cell content and kitty by its spec's erase rules; eviction by rank, then last use, with kitty images on the screen's own lines oldest transmission first and sixels on the screen never evicted; payloads decoded as they stream in; the host hook, the pause and the helper; one budget per process sized from the displays, 16 to 192 MiB. Its Notes record the operator's choices of per-cell sixel coverage (aaronsb/libtsm#49), the storage-quota rule and the minimal build without the decoders"
  - upstream: "libtsm eeaf293 (aaronsb/libtsm): src/tsm/tsm-image.c (the store, eviction, the frame's placement list renewing each drawn image's use at line 1120 on one process-wide clock, lines 46-50), tsm-sixel.c, tsm-kitty.c, tsm-vte-image.c (routing, the pause, the work bound), and tsm-replay's fixture host"
  - upstream: "aaronsb/kmscon b1f7a7c5: src/image_host.c, src/image_helper.c, src/image_identity.c, src/render/image_budget.c, src/terminal_image.c, and tests/test_pty.c's test_pause"
  - upstream: "fixes the C reference made and vtium carries: a reservation that cannot fit refused before anything is evicted (aaronsb/kmscon#138); scroll and ED 2 scans counted as input work (aaronsb/libtsm#52); a refused replacement keeps the old image (aaronsb/libtsm#37)"
  - standard: "the kitty graphics protocol (sw.kovidgoyal.net/kitty/graphics-protocol): per-buffer storage, the storage quota's order, ED 2 as the only erase that reaches a placement, region scrolls moving only placements wholly inside, the z bands; read as quoted in kmscon ADR-103, not re-verified here"
  - standard: "xterm's ctlseqs and the VT330/VT340 graphics manual: sixel, DECSDM, modes 1070 and 8452, XTSMGRAPHICS"
  - evidence: "docs/c-reference.md §9, Images: its Check (a use clock per store leaves eviction across stores, or a budget split per store, to decide) and its Not planned (whether the compositor's scaled pictures share the image budget)"
  - evidence: "vtium/vtium#187, Keep the GPU and image paths open: lines carry a LineId for placements, Damage has mark_rect, FrameCell::default_bg serves the lowest layer, Snapshot carries no placements yet and takes them at #28 with their pixels shared"
  - precedent: ADR-1000
  - precedent: ADR-1001
  - precedent: ADR-1003
  - precedent: ADR-1200
  - precedent: ADR-1300
  - precedent: ADR-1400
  - precedent: ADR-1401
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "`cargo tree -p vtium -e normal` with default features lists no vtium-image, and with `--features images` lists it"
  - "The flood test feeds a stream of transmissions and placements past a full store whole and cut at random points, with frames drawn at different points, and every run ends in the same store; with the input-work count removed from a scan, planted, it fails"
  - "A test feeds an interleaved sixel and kitty stream across a pending decode, and the placements land in stream order; with the barrier removed, planted, it fails"
  - "A `compile_fail` doctest in vtium-image shows that code outside the rules module cannot match on a protocol tag"
  - "tier 1's image cases (`img-`, `six-` and `kit-` in `tools/conformance/corpus/cases.list`) pass through vtium-replay built with `images`"
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1100: One image pipeline: protocol handlers feed a store of placements that travel with lines

## Summary

- **Decided:** vtium keeps the C reference's image flow as one pipeline. A handler for each protocol (sixel, kitty graphics, and iTerm2's inline images) reads its control string as it streams in, decodes what is safe to decode in the console and hands the rest to the sandboxed image helper, with the console's input paused until the helper answers. Each console has one store of images and placements, with a main and an alternate buffer. A placement is anchored to the line holding its top row and travels with the text. Every placement carries the tag of the protocol that made it, and one function in vtium-image maps a tag to that protocol's rules: what cuts it, how a region scroll moves it, whether it can be evicted while on the screen, and the layer it draws in. Nothing else in vtium can match on a tag. The store's use clock moves only on stream events, so eviction follows the bytes and never the drawn frames. Each console has its own budget, and the compositor's scaled copies live in a cache of the renderer's own that never evicts a stored image. A frame's snapshot carries the placements with their pixels shared, and every display of a head composites the same snapshot. The whole pipeline is the `images` Cargo feature, and the minspec build carries none of it.
- **Trades away:** A sixel is held as premultiplied RGBA, 4 bytes a pixel, where palette indices would take 1 or 2. A file or compressed image costs a round trip to the helper, and that console's output waits for it. With a budget per console, a vtium running several consoles can hold several budgets of images at once. A picture in a mode 2026 hold keeps its memory until the hold ends, even if its image was deleted meanwhile.
- **One-way?** No while nothing is built. It becomes expensive once tier 1's image goldens are judged against vtium's output and programs rely on the replies, and once the handlers, the compositor and the helper are built against the store's interface.
- **Probes:**
  - *Not confident (a budget per console):* each console gets its own image budget, `image-storage-limit`, covering its main and alternate screens, so what one console keeps depends only on what its own programs sent. The C reference had one budget for the whole process, where one console's pictures could push out another's. With four consoles, vtium may then hold four budgets of pictures at once. Is a budget per console what you want?
  - *Confident (scaled copies apart):* when a display draws a picture at a size other than its own, as on a zoomed or rotated display, the scaled copy lives in the renderer's own cache, and drawing it never pushes a stored picture out of the scrollback, as the C reference's scaled copy could. Yes or no?
- **Inversion:** Between "images are cell content" (sixel's own model, where text over a picture replaces it) and "images are a free layer text never touches" (the kitty spec's model). The answer keeps one layer of placements and gives each protocol the erase and scroll rules its own specification describes, held in one function so a third protocol adds a tag and one arm.

## Context

vtium/vtium#28 asks for one image pipeline that the sixel handler (#48), the kitty graphics handler (#49), the iTerm2 handler (#62), a picture over a picture (#29), pictures in the CPU compositor (#30), the image helper and its sandbox (#31), kitty animation (#32) and the login image (#63) all build on. `crates/vtium-image` holds only its crate documentation today. ADR-1000 lists the rows this record serves: `image.pipeline`, `image.sixel`, `image.kitty`, `image.kitty-animation`, `image.kitty-relative`, `image.kitty-placeholders`, `image.overlap`, `image.compositor`, `image.helper`, `image.iterm2`, `image.login`, `query.xtsmgraphics`, and the modes `DEC-80` (DECSDM), `DEC-1070` (private sixel color registers) and `DEC-8452` (sixel cursor to the right), each in the `images` layer.

The C reference built this in kmscon ADR-103 and its Notes: one store per screen buffer in libtsm, two protocol front ends, a helper process for files and compressed formats, and one budget for the whole process. vtium takes that logic flow and leaves its structure.

Several of vtium's own records already decide parts of the pipeline's surroundings. ADR-1001 gives each line a permanent `LineId` for placements to anchor to, and says that a placement added, moved, clipped or removed marks the cells under its old and new boxes. ADR-1003 decides how an anchor moves through a rewrap. ADR-1300 fixes the color glyph format, premultiplied RGBA, and the row scratch that pictures are composed in. ADR-1400 D2 carries the C reference's helper forward: its own process, the session's user, never root by fallback, a 2-second timeout with input paused. ADR-1401 D1 gives each head its consoles, each a session with its own screen, and D2 gives each head a render thread of its own. `docs/architecture.md` names the eviction rule and the decode timeout among its determinism rules. vtium/vtium#187 found that the line, damage and frame models already have room for placements, and that the head's `Snapshot` carries none yet.

`docs/c-reference.md` §9 left two questions this record answers: how eviction orders images across stores once each store has its own use clock, and whether the compositor's scaled pictures share the image budget.

A measurement spike on the C reference, aaronsb/kmscon#259, checks ADR-103's A5 and A7 targets (full-motion kitty video, and several kitty videos at once) before the store's design settles, and files each shortfall on #28. It has not reported yet.

## Decision

The figure shows the pipeline: who owns each part, and the path a picture takes from a program's output to each display.

![vtium's image pipeline. One console's pty feeds program output, in order, to vtium-parse, which hands DCS q, APC G and OSC 1337 File= strings to the handlers in vtium-image: sixel, kitty and iTerm2, run in stream order, which decode sixel, base64 and raw RGBA themselves. For PNG, zlib, files and shared memory a handler sends a request to the image helper, which runs as the session's user in the sandbox of ADR-1400, and input is paused until the reply, or FAILED after 2 seconds. The handlers place pictures in the store, one per console, owned by vtium-image: main and alternate buffers of images and placements, each placement with its anchor line and column, box, crop, z, layer, sequence number and protocol tag, and a budget whose use clock moves only on a transmission, a placement or a reference. Beside the store, rules(tag) is the one place each protocol's rules live. The vtium-term screen calls the store on each write, erase, scroll, reflow, alternate screen switch and RIS. The frame's placements on the visible rows go into the head's snapshot, made by the binary, with the pixels shared; the last complete snapshot stays while a mode 2026 hold lasts. Each display's compositor draws the same snapshot by layer, then z, then sequence number, at its own scale. A legend colors each box by its owner: vtium-term, vtium-parse, vtium-image, vtium-platform and the helper, vtium and vtium-render.](figures/ADR-1100-pipeline.svg)

### D1: which crate owns what

| Owner | What it holds |
|---|---|
| `vtium-image` | The store: images, placements, the budget and eviction (D2, D5). The protocol tag and its rules function (D3). The handler trait (D6) and the handlers: `protocol::sixel` (#48), `protocol::kitty` (#49), `protocol::iterm2` (#62). The decoders that run in the console: sixel, base64, and raw RGB and RGBA into premultiplied RGBA. The decode request and reply types (D7). It depends on `vtium-parse` alone, for the string pieces and ends the parser hands out, and it names a line by an opaque 64-bit key, so it never depends on `vtium-term`. It forbids `unsafe`, denies the arithmetic, cast and indexing lints `docs/architecture.md` lists, and uses no `HashMap`. |
| `vtium-term`, behind its `images` feature | One store for each terminal it runs. The routing of DCS `q`, APC `G` and OSC 1337 `File=` to the handlers, and the on and off switch of each protocol. The call into the store on every screen event (D8), with the `LineId` of each line as its key. The marking of `Damage` for each box the store reports changed, as ADR-1001 asks. The anchors carried through a rewrap (ADR-1003). The mode rows 80, 1070 and 8452, DA1's attribute 4, and the XTSMGRAPHICS reply (#48). The frame's placement list (D9). |
| `vtium-render`, behind its `images` feature | Drawing a snapshot's placements in their layers, the scaled copies and their cache, and the damage pictures cause (#30, D9). |
| `vtium-platform` | The helper's process: starting it as the session's user, its socket, its 2-second timer, and killing it (#31, ADR-1400 D2). Pausing a pty's reads while a request is out. |
| `vtium`, the binary | Sending a request to the helper and handing its reply back to the terminal. The head's snapshot, which takes the frame's placements (D9). The configuration keys (D11). Turning the protocols on for a head (D10). |
| The helper | Reading files and shared memory under the file rules, and decoding PNG, zlib and the iTerm2 formats, as the session's user (#31). Whether it is a second executable or the console's own binary is #31's to decide, as ADR-1400 D2 leaves it. |

### D2: the store

- **One store for each console,** a console being ADR-1401 D1's session with its own pty, screen and scrollback. The store holds two buffers, main and alternate, and each buffer has its own images, its own kitty ids and numbers, and its own placements, as the kitty spec sets its storage quota per buffer and the C reference kept one store per screen buffer. The two buffers share one budget and one use clock (D5).
- **An image** has a key, a 64-bit number the store counts up and never reuses; its protocol tag; for kitty, its id (1 to 2^32-1) and image number, if it has them; its width and height; its pixels, premultiplied RGBA, 4 bytes a pixel, the color glyph format of ADR-1300, held behind a shared reference so a snapshot can hold them without a copy; whether every pixel is opaque; for a tag whose rules say a new picture covers older ones per cell, which cells it covers fully and which it leaves empty (#29); the use stamp and the transmission stamp of D5; and whether kitty's `N=1` marked it transient.
- **A placement** has a key, counted up and never reused; its image's key; its protocol tag; its anchor, which is the key of the line holding its top row and a column; its box in cells; its offset in pixels inside its first cell; its source rectangle in the image's pixels, which is the crop; its z; the layer its tag's rules give it (D4); its sequence number (D4); its kitty placement id, or none; and the cell size in pixels it was made in, which is the unit of its pixel values.
- **Cutting splits a placement into pieces.** Each piece is a placement of the same image with a smaller box, and its source rectangle follows from that box. The picture itself is never cut or copied. A piece clipped to nothing is removed, and a sixel's image goes when its last piece goes (D3).
- **The store's maps are ordered.** Images and placements are kept in ordered maps by key, and an index from line key to the placements anchored there is used for lookups only. Nothing orders lines by their key, as ADR-1003 notes that line ids are unique but carry no order. Whatever reaches the screen, a reply or eviction is taken in key order, sequence order or use order, never from a hash.
- **Bounds.** vtium-image keeps a bounds table, as `vtium-parse` and `vtium-term` do, whose rows start from the C reference's values: 1024 images and 4096 placements in each buffer; an image side of 65,535 pixels, below which `max-graphic-size` sets the working limit (D11); a cell of at most 1024 pixels a side; a box of at most 10,000 cells; and 256 bytes of the budget charged for each placement of an image past its first. A count bound is met by eviction in D5's order, and refused when nothing more may go. Every size computed from a program's numbers is checked arithmetic, in 64 bits.

### D3: the protocol tag and the rules function

Every image and placement carries the tag of the protocol that made it. The tag's variants are private to one module of vtium-image, `rules`, and that module's one public function maps a tag to a table of rules. The store, the screen and the compositor ask the table and never see which protocol it came from, and code outside the module cannot name a variant to match on it. vtium enforces this by module privacy, where a check that searches the source would be needed in C, and a `compile_fail` doctest shows a match outside the module failing to build. A third protocol adds a tag and one arm of the function.

| Rule | Sixel | Kitty graphics | iTerm2 (#62) |
|---|---|---|---|
| A character written into a cell it covers | Cuts the part under that cell | No effect | As sixel |
| ED 0, ED 1, EL, ECH, DECSED, DECSEL, and a rectangle erase or fill (DECERA, DECSERA, DECFRA) over it | Cuts the part under the erased cells | No effect (kitty spec) | As sixel |
| A rectangle copy (DECCRA) into cells it covers | Cuts the part under the destination, as a write there does | No effect | As sixel |
| ICH, DCH, and printing in insert mode on a row it covers | Cuts the part from the cursor to the end of the row, since shifting part of a picture by whole cells would tear it | No effect | As sixel |
| A new picture of the same tag over it | The parts under the cells the new picture covers fully are removed, and a piece left with no set pixel is removed (#29) | No effect | As sixel |
| A region scroll (margins, IL, DL) | The part inside the moving band moves with it, the placement splits at the band's edges, and parts pushed out of the region are clipped | A placement wholly inside the band moves and is clipped at its edges; one that crosses an edge stays (kitty spec: "only images that are entirely within the page area must be scrolled") | As sixel |
| Eviction while placed on the screen's own lines | Never | Allowed, oldest transmission first (D5) | Never |
| Its layer | Above text, at z 0 | From its z (D4) | Above text, at z 0 |
| Its order among equal layer and z | After kitty placements, by sequence number | By image id, lower first, then sequence number | As sixel |
| Kitty's delete commands | Never reach it | Reach it, by the spec's table | Never reach it |
| Its image without placements | Is freed, since nothing can name it again | Stays until it is deleted or evicted, so a lowercase delete keeps its data (kitty spec) | Is freed |

The events every placement meets alike are D8's and are not in the table.

### D4: the order a frame draws placements in

A frame draws, from the bottom: the default background; the layer below backgrounds, kitty placements with z below -1,073,741,824; the cell backgrounds that are not the default; the layer below text, kitty placements with z from -1,073,741,824 to -1; glyphs and decorations; the layer above text, kitty placements with z of 0 or more and every sixel and iTerm2 placement; and last, above every placement, the cursor, the pointer and vtium's own notices and marks, so no program's picture can hide them. The lowest layer shows only through cells whose background is the default, which the frame cell's `default_bg` tells the compositor (ADR-1001).

Within a layer, placements draw by z, then by the order the rules give at equal z (kitty placements before sixel and iTerm2, and kitty placements by image id, lower first, as the kitty spec orders them), then by sequence number. The sequence number is a per-store count, taken by each placement when it is made, in stream order, so sixel and kitty strings interleaved in one stream draw in the order they arrived. A piece cut from a placement keeps its placement's number, so cutting never reorders a picture. The store hands the frame its placements already in this order (D9), and the compositor draws them as handed.

The figure shows the layers, and what one erase does to each kind of placement.

![Two panels. A: the layers of a frame from the bottom: 1, the default background; 2, kitty placements with z below -1,073,741,824; 3, cell backgrounds that are not the default; 4, kitty placements with z from -1,073,741,824 to -1; 5, glyphs and decorations; 6, kitty placements with z of 0 or more, and sixel and iTerm2 at z 0; 7, the cursor, the pointer and vtium's own notices. B: a grid of ten columns and four rows before and after EL 2 erases its third row. An orange sixel four columns wide covers all four rows, and a teal kitty placement three columns wide beside it covers all four rows. After the erase, the sixel has lost the erased row and remains as two pieces, the top two rows and the bottom row, while the kitty placement is unchanged. Each piece shows part of the same picture, and the picture itself is never cut.](figures/ADR-1100-layers.svg)

### D5: the budget and eviction

- **A budget for each console.** `image-storage-limit` bounds the bytes of every image in one console's store, main and alternate buffers together, so what a console keeps follows from its own stream and the host settings alone. An image costs width × height × 4 bytes, and each placement of an image past its first costs 256 bytes (D2). The store reserves an image's whole cost before it allocates a byte of it, and a payload buffered for the helper reserves each growth before it grows (D7).
- **The default, `auto`,** is the C reference's: four frames of the console's head's displays in RGBA, width × height × 4 added over the head's displays, at least 16 MiB and at most 192 MiB. It is 16 MiB at 1366x768 and 31.6 MiB at 1920x1080. It is computed again when a display joins or leaves the head or changes mode, and a smaller `auto` evicts nothing at once: the next reservation evicts down to the limit it finds, as the C reference settled after a font change briefly uncounted a display. A limit set in the configuration applies as set. `0` turns both protocols off (D10).
- **Refused before anything is evicted.** When a reservation would pass the budget, the store first counts what eviction may free. When that falls short, it evicts nothing and refuses: kitty answers ENOSPC, and a sixel or iTerm2 picture is dropped. This is the fix the C reference made in aaronsb/kmscon#138. The image a kitty transmission replaces is held while the store makes room, so a refused replacement keeps the old image (aaronsb/libtsm#37).
- **The eviction order,** across the console's two buffers:
  1. images with no placement, transient ones before the rest, each least recently used first;
  2. images whose every placement lies in the scrollback, least recently used first, with their placements;
  3. images whose rules allow eviction while placed on the screen's own lines, kitty's, oldest transmission first, with all their placements.

  An image whose rules never allow eviction while placed on the screen's own lines, a sixel or an iTerm2 picture, is never evicted while it is, so a store full of them refuses. The screen's own lines are the buffer's visible rows, never the scrollback.
- **The use clock is the store's own.** It is a count that moves by one at each stream event that touches an image: a transmission, a placement made, and a command that names an existing image, such as a kitty put or query by id or number. Each image takes the clock's value as its use stamp at each such event, and as its transmission stamp when it is transmitted. A drawn frame, a snapshot, a resize, a VT switch and a display's arrival never move it, so eviction follows the bytes. The C reference renewed every drawn image's use on each frame, on a clock shared by the whole process, so its eviction order depended on which frames were drawn and when; vtium lists that as a departure. Each stamp is unique within the store, so the order is total.
- **The renderer's scaled copies are outside this budget.** A display that draws a picture at another size keeps the scaled copy in a cache of the renderer's own, bounded per display (#30 sets the bound; the C reference's was twice the screen in RGBA). A copy that does not fit is drawn scaled for that frame without being kept, so the cache changes speed and never pixels, the rule ADR-1300 D3 sets for glyphs. No scaled copy ever evicts a stored image. The C reference let a scaled copy for a picture on screen evict pictures in the scrollback (ADR-103's Notes, Q1); vtium lists that as a departure, because the store would then depend on what was drawn.
- **Scans count as input work.** Every walk over placements, on a scroll, an ED 2, a kitty delete or an eviction, adds what it visits to the input call's work, as decoded pixels do, so a flood of line feeds or ED 2 against a full store stops the input call early and the rest is taken by the next call (aaronsb/libtsm#52). The work bound is a row of vtium-term's bounds table, starting from the C reference's 2^24 a call. It changes when work is done and never the state that results (`docs/architecture.md`, "Chunking invariance").

### D6: the handler trait

A handler turns one protocol's control strings into store and screen changes. vtium-image defines the trait, and each protocol module implements it:

- **`start`** takes the string's introducer: a DCS string's parameters, or the start of an APC or OSC string the router has matched to the handler.
- **`feed`** takes the string's data in the pieces the parser hands out, decoding as they arrive and never holding a whole payload: a sixel string grows its picture as its bands arrive, and a kitty string decodes its base64 into the pending image. It reports the work it did (D5).
- **`finish`** runs when the string ended with its terminator, which `StringEnd::is_complete` reports. It places the picture, or makes a decode request (D7), and replies.
- **`cancel`** runs when the string was cancelled, by CAN, SUB or an ESC not followed by `\`. The handler drops what it holds and releases what it reserved; nothing is placed and nothing is answered. Acting only on a complete string is vtium-parse's listed departure from the C reference, which acted on a string however it ended.
- **`decoded`** takes the helper's answer to the handler's request, and places, stores or replies as `finish` would have.

Each method gets one context, which vtium-term implements: the console's store and the buffer in use; a decode request sink; the cell size in pixels and the grid's size; the cursor, which a handler reads and moves through the context; the index the screen does at the bottom margin, which a sixel uses to scroll; the reply sink, which accepts only replies built under the reply rule; the protocol switches; the limits of D11; and the input-work count. A handler never reaches the screen's lines itself, so every scroll it causes passes through the screen and the store hooks of D8.

The router in vtium-term sends DCS `q` with no intermediate to the sixel handler and APC strings whose first byte is `G` to the kitty handler, each only while its protocol is on; OSC 1337 `File=` goes to the iTerm2 handler (#62). Every other DCS and APC string stays ignored, as ADR-1000's rows say. Kitty's keys section is buffered up to 1 KiB, and a longer one makes the whole string ignored, as in the C reference.

### D7: the decode boundary and the barrier

- **In the console:** sixel, base64, and raw RGB and RGBA (kitty's `f=24` and `f=32`), each converted into premultiplied RGBA as it arrives. These decoders face anything a program writes, and each takes a fuzz target (#48, #49).
- **In the helper:** PNG (`f=100`), zlib (`o=z`), file, temporary-file and shared-memory media (`t=f`, `t=t`, `t=s`), and the image formats iTerm2's `File=` sends (#62). The helper runs as the session's user in ADR-1400 D2's sandbox, and #31 decides its file rules, its identity modes and whether a syscall filter narrows it.
- **The request and reply types live in vtium-image,** in the C reference's mechanical terms, so the helper reads no part of any protocol's specification: what to open (a path, a shared-memory name, or the received bytes), where to start, the fewest bytes the image needs and the most that may be read, whether to inflate or decode, and, for a refused temporary file or shared-memory object, a discard that removes it and reads nothing. The largest image the request may return comes from the budget and `max-graphic-size`, and the helper checks a picture's header against it before it allocates. A reply is one outcome: OK with the pixels or the bytes asked for, or `UNREADABLE`, `TOO_SHORT`, `CORRUPT`, `TOO_LARGE`, `OVER_BUDGET` or `FAILED`. The handler turns the outcome into its protocol's reply, such as kitty's EBADF, EINVAL or ENOSPC. The console checks every reply's sizes before it maps or stores anything, as it checks the glyph helper's chunks (ADR-1200 D15).
- **The barrier.** When a handler makes a request, the terminal's input call stops right after the byte that ended the string and returns what it took, and takes nothing more until the reply is handed back. The binary keeps the bytes that follow and stops reading that console's pty; other consoles, drawing, keys and VT switching carry on. One request is out for each console at a time. A later string therefore cannot land before an earlier image, whichever protocol each belongs to, and a client may send a put right after a file transmission without waiting.
- **Time enters in one named place.** A request the helper has not answered in 2 seconds, held by vtium-platform's timer, gets `FAILED`, the helper is stopped, and the next request starts a new one, as `docs/architecture.md` and ADR-1400 D2 already decide. Input stays paused until the reply or the timeout, so no byte reorders around either. A console whose session ends drops its request without a reply.
- **Determinism for the replay interface.** The state after a stream follows from its bytes and the replies to its requests, in order. The replay interface's fixture host answers each request at once from the case's fixture files (`tools/conformance/corpus/INTERFACE.md`, "The fixture host"), so no case depends on time or on a helper, and the chunking property test runs through it: a stream fed whole, or cut anywhere, including between a request and its reply, ends in the same state and the same replies.
- **Without a helper,** as in a host that installs none, file, temporary-file and shared-memory media answer EBADF and PNG and zlib answer EINVAL, as the C reference answered without its host hook, so #49 can land before #31.

### D8: how a placement travels with text

The events below act on every placement alike; the cutting rules each tag has are D3's.

| Event | What happens to the placements |
|---|---|
| Scrolling the whole screen (LF or IND at the bottom margin, SU, SD, RI at the top) | They move with their lines, and lines entering the scrollback carry them. |
| A line discarded (the top of a region scroll, or the scrollback past `sb-size`) | Placements anchored there are clipped by that row and anchored on the next line. |
| ED 2 | Every placement on the screen's own lines is removed, as kitty's `a=d` does; a kitty image's data stays (kitty spec, "so that the clear command works"), and a sixel's or iTerm2 picture's image goes with its last placement. |
| ED 3 | Placements anchored on the dropped scrollback lines go. |
| A width change on the main screen | The rewrap of ADR-1003 returns, for each anchor in the rows it rewrapped, the row and column now holding that cell, and the store moves the placement there. A placement wider than the columns left keeps its whole picture and overflows the far edge, the right one for text running left to right. Rows not yet rewrapped keep their anchors until they are reached. |
| Any other resize | Lines keep their placements, and drawing clips at the right edge and the bottom; lines the height change moves to or from the scrollback carry theirs. |
| A cell size change (zoom, or a head's grid at another size) | A picture scales with its box, so it stays aligned with the text around it. |
| Entering or leaving the alternate screen (47, 1047, 1049) | The alternate buffer is emptied, on entry as the kitty spec asks ("all images in the alternate screen must be cleared") and on leaving, as the C reference did, so a full-screen program's pictures do not hold memory after it exits. The main buffer's placements show again on leaving. |
| RIS | Both buffers are emptied. |
| DECSTR | No effect, since a soft reset leaves the text. |
| Moving the view through the scrollback | No effect on the store. A frame finds placements by walking the visible lines, and as many lines above the top as the tallest placement has rows, so a picture whose top has left the view still draws its lower rows. |
| Every change above | vtium-term marks the cells under each placement's old and new box changed (ADR-1001). |

### D9: the frame's snapshot, and mode 2026

- **The frame gains a placement list:** every placement with a visible row, in the draw order of D4. Each entry carries its image's key, its visible row (negative when its top is above the view) and column, its box, its pixel offset, its source rectangle, the cell size it was made in, its layer, whether its image is opaque, and a shared reference to the image's pixels. The entry carries no tag.
- **The head's snapshot takes the list** with its pixels shared, never copied, as vtium/vtium#187 asked. A head's render thread composes from its snapshot alone and never reaches the store, so drawing cannot change a console's state, and frames drawn at any moment leave the store as the bytes made it.
- **Every display of a head composites the same snapshot,** each at its own scale, as a head mirrors one grid across its displays (ADR-1401 D1).
- **Mode 2026.** While a hold is in force no new snapshot is taken, and the displays keep the last complete frame with its pictures. An image deleted or evicted during the hold stays in memory, held by the snapshot's reference, until the hold ends and a new snapshot drops it. The budget counts the store's images only, so eviction still follows the bytes; the memory a vtium uses may pass the budgets by the pictures that only a held or in-flight snapshot still holds, at most two snapshots for each head.

### D10: the `images` feature, and what minspec carries

- **One Cargo feature, `images`,** in the binary, which turns on `vtium-term/images` and `vtium-render/images` and links vtium-image. vtium-term takes vtium-image as an optional dependency.
- **The minspec build links none of it.** Without `images` there is no store, no handler and no decoder in the binary, as the operator chose for the C reference's minimal build, where the decoders were absent rather than unreachable. DCS `q`, APC `G` and OSC 1337 `File=` are ignored as every control string vtium does not take is. DA1 reports no attribute 4. Modes 80, 1070 and 8452 have no row in that build's mode table, so DECRQM answers 0 for them and SM and RM change nothing. A kitty query gets no reply, so a client's detection (a query followed by DA1) finds no support. #48 pins XTSMGRAPHICS's bytes, with one rule: a build without `images` answers it as a build with sixel switched off does. XTWINOPS 14, 16 and 18 report sizes in every build, as their minspec rows already say.
- **A protocol is offered only where it can be drawn,** as the operator decided for the C reference: a head offers sixel and kitty graphics, and stores new pictures, only while `image-storage-limit` is above 0, the protocol's switch is on, and every display of the head has a renderer that draws placements. Until #30 lands no renderer does, so a build with `images` offers nothing on a real head, while the replay interface's host, which draws nothing, offers both. Pictures stored before a display without placement drawing joined the head stay, counted and not drawn, and show again when it leaves.
- **The gates test both builds:** the gate line already runs `cargo test --workspace` with default features beside `--all-features`. Tier 3's install tree takes a feature list, with the size budget applied to the minspec tree alone (#187 item 5, #48).

### D11: settings

Each setting takes the name another terminal gives it, as the C reference's did, and becomes a row of the binary's key table (`crates/vtium/src/config/keys.rs`) in the change that first reads it.

| Key | Named after | Default | Meaning |
|---|---|---|---|
| `image-storage-limit` | Ghostty's `image-storage-limit` | `auto` (D5) | The bytes of each console's images, main and alternate together. Plain bytes or K, M and G, up to 1 TiB. `0` turns sixel and kitty graphics off. |
| `max-graphic-size` | xterm's `maxGraphicSize` | `10000x10000` | The largest picture in pixels, for every protocol. |
| `num-color-registers` | xterm's `numColorRegisters` | `1024` | Sixel color registers. |
| `private-color-registers` | xterm's `privateColorRegisters` | `true` | The state of mode 1070 at reset. |
| `sixel`, `kitty-graphics` | the C reference's switches | on | Turn one protocol off: it is neither offered nor decoded. |

The replay interface's reference host sets the budget to 320,000,000 bytes for each screen, which vtium reads as each console's store, and turns both protocols on.

### D12: the order the issues build in

1. **aaronsb/kmscon#259 reports** on A5 and A7, and each shortfall it finds is filed on #28 and answered by an edit to this record before #28's store is built.
2. **#28:** vtium-image's store, tag and rules, budget and eviction, handler trait and decode types; vtium-term's `images` feature, routing, store hooks on every screen event, and the frame's placement list; the head's snapshot taking it; and vtium-replay built with `images`, declaring the `images`, `sixel`, `kitty-graphics` and `image-host` features with its fixture host. The replay interface's `image` and `place` commands drive the store before any handler exists.
3. **#48 and #49 in parallel,** each in its own module of vtium-image. #49's media through the helper answer as D7's "Without a helper" says until #31 lands.
4. **#31 in parallel with #48 and #49,** once D7's request and reply types are fixed, as the C reference built its helper beside its kitty parser against a fixed interface. It needs its own record, as its issue asks.
5. **#30** after #16 and #28. A head offers the protocols once #30 draws placements (D10).
6. **#29** after #28 and #48. **#62** after #28, #31 and #39. **#32** after #28 and #29, with its own record. **#63** after #21, #28, #30 and #31, with its design note.

### Verification

- **Store tests for each tag:** a write, each erase, ICH and DCH, a region scroll, eviction and the layer, for sixel and kitty, each seen to fail first.
- **The rules function is the only match on a tag:** the `compile_fail` doctest of D3.
- **Stream order across a pending decode:** interleaved sixel and kitty strings, with a request out between them through the fixture host, land in stream order; the same test with the barrier removed, planted, fails.
- **The budget:** a reservation too large for what eviction could free evicts nothing and is refused, seen to fail with eviction planted before the count; a refused kitty replacement keeps the old image; the eviction order, rank by rank.
- **The flood:** a stream of transmissions, placements, line feeds and ED 2 against a full store, fed whole and cut at random points, with frames drawn at different points and none, ends in the same store; with a scan's input-work count removed, planted, the bounded input call no longer stops early and the test fails.
- **No order from a hash:** `clippy::disallowed_types` denies `HashMap` and `HashSet` in vtium-image, as in every crate that reaches state.
- **The reply rule:** every reply a handler sends is in the generated list of the reply-rule test (`docs/architecture.md`, "No reflection"): kitty's replies carry only re-encoded numbers and fixed messages.
- **The minspec build:** `cargo tree -p vtium -e normal` with default features lists no vtium-image, and the minspec mode table has no rows 80, 1070 or 8452.
- **Tier 1:** the corpus's image cases pass through vtium-replay built with `images` (#14), and the capability probes that need `images` pass in that build and fail without it.
- **Coverage:** each test that drives an `image.*` row of ADR-1000 through vtium-image carries its `covers:` tag.

### For the operator's check

Each item is an agent call written into this record as the decision. The maintainer confirms or overrules each, and an overruled item changes the places it names.

1. **A budget for each console** (D5, D11). Lean: yes, so a console's pictures depend on its own output alone. The alternative is the C reference's one budget for the whole process, where one console's pictures can evict another's and eviction follows how two consoles' outputs interleave in time; four consoles can hold four budgets at once under the lean.
2. **Scaled copies in the renderer's own cache** (D5). Lean: yes; a copy that does not fit is drawn scaled each frame without being kept, and no scaled copy evicts a stored picture. This departs from the C reference's answer to ADR-103's Q1, where a picture on screen could push pictures out of the scrollback.
3. **`auto` stays the C reference's rule,** four frames of the head's displays, 16 to 192 MiB (D5). Lean: keep. On the minspec board it is 16 MiB for each console, about 3% of 512 MB.
4. **Memory a held snapshot keeps** (D9). Lean: accept that a mode 2026 hold, or a frame being drawn, keeps a deleted picture's pixels alive outside the budget until it ends, at most two snapshots for each head.
5. **iTerm2 pictures take sixel's rules** (D3): cut by text and erases, never evicted while on the screen's own lines, above text at z 0. Lean: yes, as WezTerm, which documents the protocol vtium follows, keeps them in cells.
6. **The cursor, the pointer and vtium's notices draw above every placement** (D4). Lean: yes, so no program's picture can hide the cursor or a notice vtium shows. ADR-103 gave the cursor no place in its layer order.
7. **At equal z, sixel draws above kitty** (D3, D4), as the C reference ordered them. Lean: keep.
8. **One pixel format, premultiplied RGBA, for every tag** (D2). Lean: keep, so composition has one path, at the cost of a sixel taking two to four times the bytes palette indices would.
9. **The bounds' starting values** from the C reference: 1024 images and 4096 placements a buffer, 256 bytes for each extra placement, a work bound of 2^24 a call (D2, D5). Lean: keep as starting values, which vtium may change with the reason in each row.
10. **Kitty ids are per buffer,** so the alternate screen's ids are its own (D2). Lean: yes, as the C reference's store per buffer had it.
11. **A kitty placement that replaces one with the same image and placement id takes a new sequence number** (D4). Lean: yes, as any placement made does.
12. **When the record may be accepted against aaronsb/kmscon#259** (D12). Lean: the record can be read and accepted now, and #28's build waits for #259's report, each shortfall answered by an edit here first.

## C notes: the C reference's flow

How the C reference handled images, as the logical reference for the decisions above, read at kmscon `b1f7a7c5` and libtsm `eeaf293`.

- **Stores.** libtsm kept a `struct tsm_image_store` for each screen buffer and reached the one in use through `cur_store`. Placements were listed on their line (`placement_attach`) and cut by `placement_split` and `placement_cut`; screen events reached the store through `screen_images_scroll`, `screen_images_cut`, `screen_images_erase_screen`, `screen_images_clear_alt`, `screen_images_reset` and `screen_images_discard_line` (`libtsm/src/tsm/tsm-image.c`).
- **Eviction.** `image_rank` gave each image its rank, `image_order` its order within the rank (`transmitted` for kitty images on the screen, `last_use` otherwise), and `image_victim` picked the next to go across both buffers. `evictable_bytes` counted what could go before anything went. `screen_images_frame` stamped `last_use = ++image_clock` on every placement each frame drew, and `image_clock` was one static counter for the whole process (`tsm-image.c:46-50`, `tsm-image.c:1120`).
- **Bounds and charges.** `TSM_SCREEN_IMAGES_PER_STORE` (1024), `TSM_SCREEN_PLACEMENTS_PER_STORE` (4096) and `TSM_SCREEN_PLACEMENT_OVERHEAD` (256) in `libtsm.h`; `placement_make_room` charged the overhead, and `screen_images_settle` paid a debt the budget refused by dropping the smallest placements first.
- **Routing and the pause.** `vte_dcs_start`, `vte_dcs_collect` and `vte_dcs_end` fed the sixel decoder (`tsm_sixel_feed_some`, `tsm_sixel_finish`), and `vte_apc_start`, `vte_apc_collect` and `vte_apc_end` the kitty parser (`tsm-vte-image.c`). A bounded input call stopped at `VTE_INPUT_WORK`, 2^24 (`tsm-vte.c:3690`), and took nothing while `vte_image_paused` held. kmscon's `kmscon_pty_pause` and `kmscon_pty_resume` stopped and restarted a pty's reads (`src/misc/pty.c`), tested by `test_pause` in `tests/test_pty.c`, and `tsm_vte_image_reply` delivered the helper's answer.
- **The host.** `image_host.c` started and killed the helper, `image_identity.c` found the session's user, `image_helper.c` applied the file rules and decoded PNG and zlib, and `render/image_budget.c` held the process's one budget, whose `make_room` trimmed the renderers' pictures before evicting libtsm's images.
- **The replay.** `tsm-replay`'s fixture host answered requests from a case's fixture files, which vtium's replay interface carries as "The fixture host".

## Where vtium departs from the C reference

- **The use clock is each store's own and moves only on stream events.** The C reference renewed every drawn image's use on each frame, on one clock for the whole process, so its eviction depended on which frames were drawn (D5).
- **Each console has its own budget,** where the C reference had one for the whole process across every screen (D5), pending the operator's check.
- **Scaled copies never evict a stored picture.** The C reference's scaled copy for a picture on screen could evict pictures in the scrollback (D5).
- **A placement's change marks the cells under it,** by ADR-1001's rule, where the C reference left each renderer to damage placement rectangles itself.
- **Rectangle operations cut cell-content pictures** as the erases and writes they are (D3); the C reference had no rectangle operations.
- **A width change rewraps the main screen and carries each anchor** (D8, ADR-1003); the C reference never reflowed.
- **A handler acts only on a string ended by its terminator,** vtium-parse's listed departure.
- **The cursor, the pointer and vtium's notices draw above every placement** (D4), pending the operator's check.
- **Relative placements, Unicode placeholders and animation are supported targets** (ADR-1000), which the C reference answered with EINVAL; #49 and #32 build them on this pipeline.

## Consequences

### Positive

- One store, one budget and one compositor path serve every protocol, so a fix or a speed-up in any of them serves all.
- A protocol's rules sit in one place, and the compiler stops any other code from branching on the protocol.
- A console's pictures follow from its own bytes, whole or chunked, whatever was drawn and however its output interleaved with another console's.
- File, shared-memory and compressed media never reach the console's root process.
- The minspec build carries no image code at all.

### Negative

- A sixel takes two to four times the bytes palette indices would.
- A file or compressed image pauses that console's output for a round trip to the helper, up to 2 seconds when the helper hangs.
- Several consoles can each hold a full budget of pictures, and a held snapshot can keep deleted pictures alive past the budgets.
- A picture too large for the scaled-copy cache is scaled again on every frame it draws.
- Every handler, the compositor and the helper build against the store's interface, so changing it later touches all of them.

### Neutral

- The replay interface already carries the image commands and the fixture host the C reference's goldens were made with, and vtium-replay gains them with `images`.
- #187's remaining items (a buffer token for each display, drm2d's logic apart from its dumb buffers) are not this record's, and nothing here closes them off.
- #31's record decides the helper's sandbox beyond ADR-1400 D2, and #32's decides animation frames.

## Alternatives Considered

- **One budget for the whole process,** as the C reference had. Not taken, pending the operator's check: one console's output could evict another console's pictures, in an order that depends on how the two consoles' outputs interleave in time.
- **Scaled copies inside the image budget,** as the C reference's bbulk kept them. Rejected: a frame drawn or not drawn would then change what the store keeps, which the determinism rules forbid.
- **Branching on the protocol in the store and the compositor,** as the C reference's code did. Rejected: #28 asks for one function to hold every protocol rule, and a private tag lets the compiler hold the line.
- **The image store inside vtium-term,** with vtium-image holding only the decoders. Not taken: the store, its rules and its budget are one unit that the handlers act on, and keeping them in vtium-image leaves vtium-term without image code in the minspec build and vtium-image free of vtium-term.
- **vtium-image depending on vtium-term,** with the binary wiring the two together. Not taken: vtium-term would then reach the store on every screen event through a hook object the binary installs, so every scroll and erase would pass through that hook in every build, minspec included, where an optional dependency leaves the minspec build with no image code at all.
- **Inflating zlib in the console,** which already holds an inflate for its built-in fonts (ADR-1200 D6). Not taken: the C reference sent zlib to its helper, and a fault in a decoder facing a program's bytes then costs that request and a helper restart, never the console.
- **Holding sixel pictures as palette indices.** Not taken for now: one pixel format keeps one composition path; the C reference's cost note measured the trade, and the operator may revisit it (For the operator's check, item 8).
